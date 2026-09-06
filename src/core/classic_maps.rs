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

/// One game session's trusted catalog of classic annotated map images.
///
/// The catalog is deliberately an instance rather than process-global state:
/// Vellum can host more than one character, and each session may be attached
/// to a different Lich installation. Sharing an `Arc<ClassicMapCatalog>` with
/// that session's renderers keeps filesystem authority scoped to the session.
#[derive(Debug, Default)]
pub struct ClassicMapCatalog {
    maps: RwLock<BTreeMap<String, ClassicMapAsset>>,
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
}
