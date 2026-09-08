//! Classic annotated-map image registry.
//!
//! Lich's map database names an image and a pixel rectangle for many rooms.
//! The browser must never receive an arbitrary filesystem path, so this
//! registry is the seam between that trusted local directory and web
//! renderers: callers deal only in discovered filenames.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::Serialize;

use crate::core::mapdb::MapDb;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClassicMapEntry {
    pub name: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassicMapAsset {
    pub name: String,
    pub path: PathBuf,
    pub mime: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClassicMapRoom {
    pub id: u32,
    pub rect: [f64; 4],
}

/// One game session's trusted catalog of classic annotated map images.
///
/// The catalog is deliberately an instance rather than process-global state:
/// Vellum can host more than one character, and each session may be attached
/// to a different Lich installation. Sharing an `Arc<ClassicMapCatalog>` with
/// that session's renderers keeps filesystem authority scoped to the session.
#[derive(Debug, Default)]
pub struct ClassicMapCatalog {
    maps: RwLock<BTreeMap<String, ClassicMapAsset>>,
    rooms: RwLock<BTreeMap<String, Vec<ClassicMapRoom>>>,
}

impl ClassicMapCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace this session's catalog with supported images discovered in one
    /// Lich `maps/` directory. Symlinks and subdirectories are intentionally not
    /// followed; the resulting registry is the only path lookup web handlers use.
    pub fn reload_from_dir(&self, dir: Option<&Path>) -> usize {
        let next = dir.map(scan_dir).unwrap_or_default();
        let count = next.len();
        *self.maps.write().expect("classic map catalog poisoned") = next;
        count
    }

    pub fn get(&self, name: &str) -> Option<ClassicMapAsset> {
        self.maps
            .read()
            .expect("classic map catalog poisoned")
            .get(&name.to_ascii_lowercase())
            .cloned()
    }

    pub fn entries(&self) -> Vec<ClassicMapEntry> {
        self.maps
            .read()
            .expect("classic map catalog poisoned")
            .values()
            .map(|asset| ClassicMapEntry {
                name: asset.name.clone(),
                label: display_label(&asset.name),
            })
            .collect()
    }

    /// Refresh the clickable room rectangles sourced from the active mapdb.
    /// Image keys are case-insensitive, matching asset lookup.
    pub fn reload_rooms(&self, db: &MapDb) -> usize {
        let mut next: BTreeMap<String, Vec<ClassicMapRoom>> = BTreeMap::new();
        for room in db.all_rooms() {
            let (Some(image), Some(rect)) = (&room.image, room.image_coords) else {
                continue;
            };
            if !rect.iter().all(|value| value.is_finite()) {
                continue;
            }
            next.entry(image.to_ascii_lowercase())
                .or_default()
                .push(ClassicMapRoom { id: room.id, rect });
        }
        for rooms in next.values_mut() {
            rooms.sort_by_key(|room| room.id);
        }
        let count = next.values().map(Vec::len).sum();
        *self.rooms.write().expect("classic map rooms poisoned") = next;
        count
    }

    pub fn clear_rooms(&self) {
        self.rooms
            .write()
            .expect("classic map rooms poisoned")
            .clear();
    }

    /// Clickable room rectangles for a discovered classic image. Unknown
    /// assets never disclose mapdb metadata.
    pub fn rooms(&self, name: &str) -> Option<Vec<ClassicMapRoom>> {
        let key = name.to_ascii_lowercase();
        if !self
            .maps
            .read()
            .expect("classic map catalog poisoned")
            .contains_key(&key)
        {
            return None;
        }
        Some(
            self.rooms
                .read()
                .expect("classic map rooms poisoned")
                .get(&key)
                .cloned()
                .unwrap_or_default(),
        )
    }
}

fn scan_dir(dir: &Path) -> BTreeMap<String, ClassicMapAsset> {
    let mut next = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(mime) = mime_for_path(&path) else {
                continue;
            };
            next.insert(
                name.to_ascii_lowercase(),
                ClassicMapAsset {
                    name: name.to_string(),
                    path,
                    mime,
                },
            );
        }
    }
    next
}

fn mime_for_path(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn display_label(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    stem.replace(['_', '-'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_only_returns_discovered_supported_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("wl-town.png"), b"png").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"nope").unwrap();
        std::fs::create_dir(dir.path().join("nested.jpg")).unwrap();

        let maps = scan_dir(dir.path());
        assert_eq!(maps.len(), 1);
        assert_eq!(maps.get("wl-town.png").unwrap().mime, "image/png");
        assert!(!maps.contains_key("notes.txt"));
        assert!(!maps.contains_key("../wl-town.png"));
        assert_eq!(display_label(&maps["wl-town.png"].name), "wl town");
    }

    #[test]
    fn catalogs_do_not_observe_another_sessions_files() {
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        std::fs::write(first_dir.path().join("landing.png"), b"first").unwrap();
        std::fs::write(second_dir.path().join("icemule.jpg"), b"second").unwrap();

        let first = ClassicMapCatalog::new();
        let second = ClassicMapCatalog::new();
        first.reload_from_dir(Some(first_dir.path()));
        second.reload_from_dir(Some(second_dir.path()));

        assert!(first.get("landing.png").is_some());
        assert!(first.get("icemule.jpg").is_none());
        assert!(second.get("landing.png").is_none());
        assert!(second.get("icemule.jpg").is_some());
        assert_eq!(first.entries()[0].name, "landing.png");
        assert_eq!(second.entries()[0].name, "icemule.jpg");
    }

    #[test]
    fn room_rectangles_are_grouped_by_discovered_image() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("landing.png"), b"map").unwrap();
        let catalog = ClassicMapCatalog::new();
        catalog.reload_from_dir(Some(dir.path()));
        let db = MapDb::from_json(
            r#"[
                {"id": 2, "location": "Landing", "image": "LANDING.PNG", "image_coords": [30, 40, 50, 60]},
                {"id": 1, "location": "Landing", "image": "landing.png", "image_coords": [10, 20, 30, 40]},
                {"id": 3, "location": "Landing", "image": "missing.png", "image_coords": [0, 0, 1, 1]}
            ]"#,
        )
        .unwrap();

        assert_eq!(catalog.reload_rooms(&db), 3);
        assert_eq!(
            catalog.rooms("landing.png").unwrap(),
            vec![
                ClassicMapRoom { id: 1, rect: [10.0, 20.0, 30.0, 40.0] },
                ClassicMapRoom { id: 2, rect: [30.0, 40.0, 50.0, 60.0] },
            ]
        );
        assert_eq!(catalog.rooms("missing.png"), None);
    }
}
