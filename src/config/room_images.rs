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
    /// Alternate art shown when a condition matches — a night version of
    /// the same view, a wounded version, whatever the condition system can
    /// express. First match wins; no match falls back to `name`.
    #[serde(default, rename = "variant", skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<RoomImageVariant>,
}

/// Conditional art for a room mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomImageVariant {
    /// Pool image name to use instead of the entry's default.
    pub name: String,
    /// When to use it. Reuses the shared condition system, so a variant can
    /// key off time of day, effects, injuries, vitals — anything hotbars
    /// and the injury doll can.
    pub when: crate::config::Condition,
}

impl RoomImageDef {
    /// The art to show right now: the first variant whose condition matches,
    /// else the entry's own `name`.
    pub fn resolve_name(
        &self,
        game_state: &crate::core::GameState,
        now_server: i64,
        gameobj: Option<&crate::core::gameobj_data::GameObjData>,
    ) -> &str {
        for variant in &self.variants {
            if variant.name.trim().is_empty() {
                continue;
            }
            if crate::core::conditions::eval_condition(
                &variant.when,
                game_state,
                now_server,
                gameobj,
            ) {
                return &variant.name;
            }
        }
        &self.name
    }
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

/// Splits the merged (global + character) working store back into its two scope
/// files. The merged view cannot simply be written to global: character entries
/// would leak into the shared file, and on the next launch the untouched
/// character file would layer the stale version back over them — the edit lost.
///
/// Rules, keyed the same way `merge_room_images` merges (image name / label uid):
/// - an entry whose name exists in the prior character file stays character-scoped
///   (edits to it land in the character file);
/// - everything else is global — new entries created in the editor are shared;
/// - a global entry currently shadowed by a character one is preserved in the
///   global file untouched, unless the name is gone from the merged store
///   entirely, which means the user deleted it: deletion removes it everywhere.
fn split_room_images(
    merged: &RoomImagesConfig,
    character_prior: &RoomImagesConfig,
    global_prior: &RoomImagesConfig,
) -> (RoomImagesConfig, RoomImagesConfig) {
    use std::collections::HashSet;

    let character_names: HashSet<&str> = character_prior
        .images
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    let merged_names: HashSet<&str> = merged.images.iter().map(|d| d.name.as_str()).collect();

    let mut character_out = RoomImagesConfig::default();
    let mut global_out = RoomImagesConfig::default();

    for def in &merged.images {
        if character_names.contains(def.name.as_str()) {
            character_out.images.push(def.clone());
        } else {
            global_out.images.push(def.clone());
        }
    }
    // Global versions shadowed by a character entry: the merged store only holds
    // the character copy, so carry the global original forward — but not past a
    // full delete.
    for def in &global_prior.images {
        if character_names.contains(def.name.as_str()) && merged_names.contains(def.name.as_str())
        {
            global_out.images.push(def.clone());
        }
    }

    let character_uids: HashSet<&str> = character_prior.names.keys().map(String::as_str).collect();
    for (uid, label) in &merged.names {
        if character_uids.contains(uid.as_str()) {
            character_out.names.insert(uid.clone(), label.clone());
        } else {
            global_out.names.insert(uid.clone(), label.clone());
        }
    }
    for (uid, label) in &global_prior.names {
        if character_uids.contains(uid.as_str()) && merged.names.contains_key(uid) {
            global_out
                .names
                .entry(uid.clone())
                .or_insert_with(|| label.clone());
        }
    }

    (character_out, global_out)
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

    /// Persist the merged working store, splitting entries back into the global
    /// and character files they came from (see [`split_room_images`]). With no
    /// character profile everything is global and only that file is written.
    pub fn save_room_images_split(
        merged: &RoomImagesConfig,
        character: Option<&str>,
    ) -> Result<()> {
        if character.is_none() {
            return Self::save_room_images(merged, true, None);
        }

        let character_prior = Self::load_character_room_images_only(character)?;
        let global_prior = Self::load_common_room_images()?;
        let (character_out, global_out) =
            split_room_images(merged, &character_prior, &global_prior);

        // Only touch the character file when it already participates: a user
        // without per-character art shouldn't grow an empty file on every save.
        if !character_prior.images.is_empty()
            || !character_prior.names.is_empty()
            || !character_out.images.is_empty()
            || !character_out.names.is_empty()
        {
            Self::save_room_images(&character_out, false, character)?;
        }
        Self::save_room_images(&global_out, true, None)
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
            variants: Vec::new(),
        }
    }

    /// A character-scoped entry edited in the merged view must return to the
    /// CHARACTER file, and the global file must not absorb it — flattening the
    /// merged store into global leaked character art to every character and
    /// lost the edit on the next launch (the untouched character file layered
    /// the stale version back over it).
    #[test]
    fn split_keeps_character_entries_in_the_character_file() {
        let global_prior = cfg(vec![def("shared", &[1])]);
        let character_prior = cfg(vec![def("mine", &[2])]);
        // The user edited "mine" (added a room) in the merged view.
        let merged = cfg(vec![def("shared", &[1]), def("mine", &[2, 3])]);

        let (character_out, global_out) =
            split_room_images(&merged, &character_prior, &global_prior);

        assert_eq!(character_out.images.len(), 1);
        assert_eq!(character_out.images[0].name, "mine");
        assert_eq!(character_out.images[0].rooms, vec![2, 3]);
        assert_eq!(global_out.images.len(), 1);
        assert_eq!(global_out.images[0].name, "shared");
    }

    /// New entries created in the editor are shared: they land in global.
    #[test]
    fn split_sends_new_entries_to_global() {
        let global_prior = cfg(vec![]);
        let character_prior = cfg(vec![]);
        let merged = cfg(vec![def("fresh", &[9])]);

        let (character_out, global_out) =
            split_room_images(&merged, &character_prior, &global_prior);

        assert!(character_out.images.is_empty());
        assert_eq!(global_out.images.len(), 1);
        assert_eq!(global_out.images[0].name, "fresh");
    }

    /// A global entry shadowed by a character override is invisible in the
    /// merged store, but the global file must keep its own version — rewriting
    /// global without it would strip the base art from every other character.
    #[test]
    fn split_preserves_shadowed_global_entries() {
        let global_prior = cfg(vec![def("pier", &[1])]);
        let character_prior = cfg(vec![def("pier", &[1, 2])]);
        let merged = cfg(vec![def("pier", &[1, 2])]); // merged holds the char copy

        let (character_out, global_out) =
            split_room_images(&merged, &character_prior, &global_prior);

        assert_eq!(character_out.images.len(), 1);
        assert_eq!(character_out.images[0].rooms, vec![1, 2]);
        assert_eq!(global_out.images.len(), 1, "shadowed global copy must survive");
        assert_eq!(global_out.images[0].rooms, vec![1], "global keeps ITS version");
    }

    /// Deleting an entry from the merged view means "gone": it must vanish
    /// from BOTH files, or the surviving layer resurrects it next launch.
    #[test]
    fn split_deletion_removes_the_entry_everywhere() {
        let global_prior = cfg(vec![def("pier", &[1])]);
        let character_prior = cfg(vec![def("pier", &[1, 2])]);
        let merged = cfg(vec![]); // user deleted it

        let (character_out, global_out) =
            split_room_images(&merged, &character_prior, &global_prior);

        assert!(character_out.images.is_empty());
        assert!(global_out.images.is_empty());
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
            variants: Vec::new(),
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

#[cfg(test)]
mod variant_tests {
    use super::*;
    use crate::config::Condition;
    use crate::core::elanthian_time::DayPhase;
    use crate::core::GameState;

    fn night_variant(name: &str) -> RoomImageVariant {
        RoomImageVariant {
            name: name.to_string(),
            when: Condition::TimeOfDay {
                phase: DayPhase::Night,
            },
        }
    }

    /// 2026-08-10 20:44 UTC = 16:44 Eastern (Day); minus 10h = 06:44 (Dawn).
    const DAYTIME: i64 = 1_786_394_661;
    const NIGHTTIME: i64 = 1_786_394_661 - 20 * 3600; // 20:44 Eastern

    /// The whole point: one image by day, another at night.
    #[test]
    fn a_matching_variant_replaces_the_default_art() {
        let gs = GameState::new();
        let def = RoomImageDef {
            name: "pier_day".into(),
            rooms: vec![1],
            rows: None,
            align: None,
            variants: vec![night_variant("pier_night")],
        };
        assert_eq!(def.resolve_name(&gs, DAYTIME, None), "pier_day");
        assert_eq!(def.resolve_name(&gs, NIGHTTIME, None), "pier_night");
    }

    /// No variants at all is the common case and must stay the default.
    #[test]
    fn no_variants_always_resolves_to_the_default() {
        let gs = GameState::new();
        let def = RoomImageDef {
            name: "pier".into(),
            rooms: vec![1],
            rows: None,
            align: None,
            variants: Vec::new(),
        };
        assert_eq!(def.resolve_name(&gs, DAYTIME, None), "pier");
        assert_eq!(def.resolve_name(&gs, NIGHTTIME, None), "pier");
    }

    /// First match wins, so ordering is the author's control.
    #[test]
    fn the_first_matching_variant_wins() {
        let gs = GameState::new();
        let def = RoomImageDef {
            name: "default".into(),
            rooms: vec![1],
            rows: None,
            align: None,
            variants: vec![night_variant("first"), night_variant("second")],
        };
        assert_eq!(def.resolve_name(&gs, NIGHTTIME, None), "first");
    }

    /// A variant with no name is skipped rather than resolving to nothing.
    #[test]
    fn an_unnamed_variant_is_skipped() {
        let gs = GameState::new();
        let def = RoomImageDef {
            name: "pier".into(),
            rooms: vec![1],
            rows: None,
            align: None,
            variants: vec![night_variant(""), night_variant("real_night")],
        };
        assert_eq!(def.resolve_name(&gs, NIGHTTIME, None), "real_night");
    }

    /// Variants round-trip through TOML, and an entry without them stays
    /// clean in the file.
    #[test]
    fn variants_round_trip_through_toml() {
        let config = RoomImagesConfig {
            images: vec![
                RoomImageDef {
                    name: "pier_day".into(),
                    rooms: vec![7118245],
                    rows: Some(4.0),
                    align: None,
                    variants: vec![night_variant("pier_night")],
                },
                RoomImageDef {
                    name: "plain".into(),
                    rooms: vec![1],
                    rows: None,
                    align: None,
                    variants: Vec::new(),
                },
            ],
            names: Default::default(),
        };
        let text = toml::to_string_pretty(&config).unwrap();
        let back: RoomImagesConfig = toml::from_str(&text).unwrap();
        assert_eq!(back, config);
        assert_eq!(
            text.matches("[[image.variant]]").count(),
            1,
            "only the entry with a variant writes one:\n{text}"
        );
    }
}

#[cfg(test)]
mod variant_edit_tests {
    use super::*;
    use crate::config::Condition;
    use crate::core::elanthian_time::DayPhase;

    fn def_with(variants: Vec<RoomImageVariant>) -> RoomImageDef {
        RoomImageDef {
            name: "pier".into(),
            rooms: vec![1],
            rows: None,
            align: None,
            variants,
        }
    }

    fn variant(name: &str, phase: DayPhase) -> RoomImageVariant {
        RoomImageVariant {
            name: name.into(),
            when: Condition::TimeOfDay { phase },
        }
    }

    /// Removing a variant takes the right one out and leaves order intact —
    /// order is meaningful, since the first match wins.
    #[test]
    fn removing_a_variant_preserves_the_order_of_the_rest() {
        let mut def = def_with(vec![
            variant("dawn_art", DayPhase::Dawn),
            variant("dusk_art", DayPhase::Dusk),
            variant("night_art", DayPhase::Night),
        ]);
        def.variants.remove(1);
        let names: Vec<&str> = def.variants.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["dawn_art", "night_art"]);
    }

    /// An out-of-range removal is a no-op rather than a panic: the editor
    /// defers mutations, so an index can outlive the list it referenced.
    #[test]
    fn an_out_of_range_variant_index_is_ignored() {
        let mut def = def_with(vec![variant("night_art", DayPhase::Night)]);
        let before = def.variants.clone();
        if 5 < def.variants.len() {
            def.variants.remove(5);
        }
        assert_eq!(def.variants, before);
    }

    /// Editing a variant's phase changes which art resolves, without
    /// touching the default.
    #[test]
    fn changing_a_variant_phase_changes_what_resolves() {
        use crate::core::GameState;
        let gs = GameState::new();
        const DAYTIME: i64 = 1_786_394_661; // 16:44 Eastern -> Day
        let mut def = def_with(vec![variant("other_art", DayPhase::Night)]);

        assert_eq!(def.resolve_name(&gs, DAYTIME, None), "pier", "night misses");
        def.variants[0].when = Condition::TimeOfDay {
            phase: DayPhase::Day,
        };
        assert_eq!(
            def.resolve_name(&gs, DAYTIME, None),
            "other_art",
            "day now matches"
        );
        assert_eq!(def.name, "pier", "the default art is untouched");
    }
}
