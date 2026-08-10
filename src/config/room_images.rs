//! Room art mappings loaded from room_images.toml.
//!
//! Assigns a pool image to a set of room uids, so walking into one of those
//! rooms shows the art without a script. The game hands the client an empty
//! `sprite` component on every room change (and sends the uid first, via
//! `<nav rm=>`), so the client fills that slot rather than rewriting any
//! game text — see `core::messages::component`.
//!
//! Stored image-major, because one picture usually covers many rooms:
//!
//! ```toml
//! [[image]]
//! name = "krakens_fall_pier"
//! rooms = [7118245, 7118250, 7118251]
//! rows = 6
//! align = "left"
//!
//! [names]
//! 7118245 = "Kraken's Fall, Third Pier"
//! ```
//!
//! Files: ~/.vellum-fe/global/room_images.toml plus an optional per-character
//! ~/.vellum-fe/profiles/{character}/room_images.toml. A character entry with
//! the same image name replaces the global one wholesale; new names append.

use crate::config::Config;
use crate::data::FloatAlign;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

/// Height in text rows used when a mapping doesn't state one. Room art is
/// the "big picture" case, unlike a 1-row inline `<vellumImg>`.
pub const DEFAULT_ROOM_IMAGE_ROWS: f32 = 4.0;

/// Root of room_images.toml.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoomImagesConfig {
    /// One entry per picture, each naming the rooms it covers.
    #[serde(default, rename = "image")]
    pub images: Vec<RoomImageDef>,
    /// Cached room uid -> room name, for readable editor rows. Advisory
    /// only: never used for matching, so a stale label cannot mis-assign
    /// art.
    #[serde(default)]
    pub names: HashMap<String, String>,
}

/// One picture and the rooms it covers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoomImageDef {
    /// Pool image name under global/images/inline/ (no extension).
    pub name: String,
    /// Game room uids (`<nav rm=>`) that show this picture.
    #[serde(default)]
    pub rooms: Vec<u64>,
    /// Height in text rows; None uses [`DEFAULT_ROOM_IMAGE_ROWS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<f32>,
    /// Which side the art floats on; None uses Left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<FloatAlign>,
}

impl RoomImageDef {
    pub fn rows_or_default(&self) -> f32 {
        self.rows.unwrap_or(DEFAULT_ROOM_IMAGE_ROWS)
    }

    pub fn align_or_default(&self) -> FloatAlign {
        self.align.unwrap_or_default()
    }
}

/// Room-major view built once at load: the injection path needs uid -> art
/// on every room change, and scanning the image list each time would be
/// wasteful. The file stays image-major for humans.
#[derive(Debug, Clone, Default)]
pub struct RoomImageIndex {
    by_room: HashMap<u64, RoomImageDef>,
}

impl RoomImageIndex {
    /// Build the reverse index. A uid listed under two images is a user
    /// mistake the format permits: the FIRST wins and the collision is
    /// logged naming both, because silently picking one reads as "the
    /// mapping didn't take".
    pub fn build(config: &RoomImagesConfig) -> Self {
        let mut by_room: HashMap<u64, RoomImageDef> = HashMap::new();
        for def in &config.images {
            if def.name.trim().is_empty() {
                continue;
            }
            for room in &def.rooms {
                if let Some(existing) = by_room.get(room) {
                    if existing.name != def.name {
                        tracing::warn!(
                            "room_images: room {} is mapped to both '{}' and '{}'; keeping '{}'",
                            room,
                            existing.name,
                            def.name,
                            existing.name
                        );
                    }
                    continue;
                }
                by_room.insert(*room, def.clone());
            }
        }
        Self { by_room }
    }

    /// The art for a room uid, if any.
    pub fn get(&self, room: u64) -> Option<&RoomImageDef> {
        self.by_room.get(&room)
    }

    /// Room uids that have art, for reporting.
    pub fn len(&self) -> usize {
        self.by_room.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_room.is_empty()
    }
}

/// Merge character entries over global ones: same image name replaces the
/// global entry wholesale, new names append. Name labels merge per-uid.
fn merge_room_images(base: &mut RoomImagesConfig, overlay: RoomImagesConfig) {
    for def in overlay.images {
        match base.images.iter_mut().find(|d| d.name == def.name) {
            Some(existing) => *existing = def,
            None => base.images.push(def),
        }
    }
    base.names.extend(overlay.names);
}

impl Config {
    /// Load global room images from ~/.vellum-fe/global/room_images.toml.
    pub fn load_common_room_images() -> Result<RoomImagesConfig> {
        let path = Self::common_room_images_path()?;
        if !path.exists() {
            return Ok(RoomImagesConfig::default());
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common room images: {:?}", path))?;
        toml::from_str(&contents).context("Failed to parse global room_images.toml")
    }

    /// Load only character-specific room images (not merged with global).
    pub fn load_character_room_images_only(
        character: Option<&str>,
    ) -> Result<RoomImagesConfig> {
        let path = Self::room_images_path(character)?;
        if !path.exists() {
            return Ok(RoomImagesConfig::default());
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read character room images: {:?}", path))?;
        toml::from_str(&contents).context("Failed to parse character room_images.toml")
    }

    /// Load room images for a character: global entries, then character
    /// entries layered over them.
    pub fn load_room_images(character: Option<&str>) -> Result<RoomImagesConfig> {
        let mut config = Self::load_common_room_images()?;
        let character_config = Self::load_character_room_images_only(character)?;
        merge_room_images(&mut config, character_config);
        Ok(config)
    }

    /// Write room images to the chosen scope file.
    pub fn save_room_images(
        config: &RoomImagesConfig,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        let path = if is_global {
            Self::common_room_images_path()?
        } else {
            Self::room_images_path(character)?
        };
        let text = toml::to_string_pretty(config).context("Failed to serialize room images")?;
        crate::config::write_atomic(&path, text.as_bytes())
            .with_context(|| format!("Failed to write room images: {:?}", path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(images: Vec<RoomImageDef>) -> RoomImagesConfig {
        RoomImagesConfig {
            images,
            names: HashMap::new(),
        }
    }

    fn def(name: &str, rooms: &[u64]) -> RoomImageDef {
        RoomImageDef {
            name: name.to_string(),
            rooms: rooms.to_vec(),
            rows: None,
            align: None,
        }
    }

    /// The file is image-major; the runtime needs room-major. Every room in
    /// an entry must resolve to that entry.
    #[test]
    fn index_maps_every_room_to_its_image() {
        let config = cfg(vec![
            def("pier", &[7118245, 7118250, 7118251]),
            def("river", &[7503201]),
        ]);
        let index = RoomImageIndex::build(&config);
        assert_eq!(index.len(), 4);
        assert_eq!(index.get(7118250).unwrap().name, "pier");
        assert_eq!(index.get(7503201).unwrap().name, "river");
        assert!(index.get(999).is_none());
    }

    /// A uid under two images keeps the first, so the result is stable
    /// rather than depending on map iteration order.
    #[test]
    fn index_keeps_first_on_duplicate_room() {
        let config = cfg(vec![def("first", &[7118245]), def("second", &[7118245])]);
        let index = RoomImageIndex::build(&config);
        assert_eq!(index.get(7118245).unwrap().name, "first");
    }

    /// An entry with no name is skipped rather than indexing rooms to art
    /// that can never resolve.
    #[test]
    fn index_skips_unnamed_entries() {
        let config = cfg(vec![def("", &[7118245])]);
        assert!(RoomImageIndex::build(&config).is_empty());
    }

    /// Room art defaults to a multi-row picture, not the 1-row inline
    /// default, and floats left.
    #[test]
    fn defaults_suit_room_art() {
        let d = def("pier", &[1]);
        assert_eq!(d.rows_or_default(), DEFAULT_ROOM_IMAGE_ROWS);
        assert_eq!(d.align_or_default(), FloatAlign::Left);
    }

    /// A character entry replaces the global entry of the same name and
    /// appends new ones; labels merge per-uid.
    #[test]
    fn character_entries_layer_over_global() {
        let mut base = cfg(vec![def("pier", &[1, 2]), def("river", &[3])]);
        base.names.insert("1".into(), "Pier One".into());

        let mut overlay = cfg(vec![def("pier", &[9]), def("forest", &[4])]);
        overlay.names.insert("4".into(), "Deep Forest".into());

        merge_room_images(&mut base, overlay);

        let pier = base.images.iter().find(|d| d.name == "pier").unwrap();
        assert_eq!(pier.rooms, vec![9], "same name replaces wholesale");
        assert!(base.images.iter().any(|d| d.name == "river"), "global kept");
        assert!(base.images.iter().any(|d| d.name == "forest"), "new appended");
        assert_eq!(base.names.get("1").unwrap(), "Pier One");
        assert_eq!(base.names.get("4").unwrap(), "Deep Forest");
    }

    /// Round-trips through TOML, including the bare form with no rows/align.
    #[test]
    fn round_trips_through_toml() {
        let mut config = cfg(vec![RoomImageDef {
            name: "pier".into(),
            rooms: vec![7118245, 7118250],
            rows: Some(6.0),
            align: Some(FloatAlign::Right),
        }]);
        config.images.push(def("river", &[7503201]));
        config.names.insert("7118245".into(), "Third Pier".into());

        let text = toml::to_string_pretty(&config).unwrap();
        let back: RoomImagesConfig = toml::from_str(&text).unwrap();
        assert_eq!(back, config);
        // The bare entry must not gain phantom rows/align on the way out.
        assert!(!text.contains("rows = 4"), "{text}");
    }
}
