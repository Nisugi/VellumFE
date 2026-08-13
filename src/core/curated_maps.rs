//! Curated base-map membership — which uids belong to which hand-drawn map.
//!
//! The official Saga client ships prebaked layouts whose real value is
//! *editorial*: someone chose exactly which rooms form "the town of
//! Wehnimer's Landing" (street spine only, no interiors). We import only
//! that membership — the uid roster per map — and discard every coordinate;
//! our own layout engine places the rooms. Nothing of Saga's placement data
//! is copied or redistributed.
//!
//! The membership ships WITH the app: `defaults/curated_maps.toml` is
//! embedded at build time and is the data every user gets — no Saga
//! install is ever required at runtime. The `extract-curated-maps` CLI
//! subcommand is a maintainer tool: run it against a local Saga install
//! to refresh the committed file when Simu bakes more maps. A user-side
//! `global/data/curated_maps.toml`, when present, overrides the embedded
//! data (power users can maintain their own rosters).
//!
//! Everything *not* in a curated map becomes satellite-map material
//! (connected components of the remaining graph) — see the membership
//! engine that consumes `map_of_uid`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Snapshot format version (ours, not Saga's).
pub const SNAPSHOT_VERSION: u32 = 1;

/// One curated map: a display name and its uid roster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CuratedMap {
    pub name: String,
    /// Real server uids (room-bracket numbers), sorted, deduped.
    pub uids: Vec<i64>,
}

/// The full curated membership set, keyed by map slug
/// ("wehnimers-landing-town"). Slugs are stable across Saga updates and
/// double as our map keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CuratedMaps {
    pub version: u32,
    /// Saga's layoutVersion the snapshot was extracted from, for staleness
    /// display only — never gates loading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_layout_version: Option<u64>,
    pub maps: BTreeMap<String, CuratedMap>,
}

impl CuratedMaps {
    /// uid → map slug. A uid claimed by several maps (shared boundary rooms)
    /// resolves to the first slug in BTreeMap order — deterministic, and
    /// coverage (the thing satellites are computed against) is unaffected.
    pub fn uid_index(&self) -> HashMap<i64, &str> {
        let mut index: HashMap<i64, &str> = HashMap::new();
        for (slug, map) in &self.maps {
            for &uid in &map.uids {
                index.entry(uid).or_insert(slug.as_str());
            }
        }
        index
    }

    /// Total distinct uids across every curated map (the coverage set size).
    pub fn coverage_len(&self) -> usize {
        let mut all: BTreeSet<i64> = BTreeSet::new();
        for map in self.maps.values() {
            all.extend(map.uids.iter().copied());
        }
        all.len()
    }

    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }

    /// Parse Saga's `layouts.json` text, keeping membership only.
    ///
    /// Expected shape: `{"layoutVersion": N, "layouts": {"<slug>||<meta>":
    /// {"pos": [[uid, x, y], ...], ...}, ...}}`. `pos` coordinates are
    /// dropped on the floor here by design.
    pub fn from_saga_layouts_json(text: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct SagaFile {
            #[serde(rename = "layoutVersion")]
            layout_version: Option<u64>,
            layouts: BTreeMap<String, SagaLayout>,
        }
        #[derive(Deserialize)]
        struct SagaLayout {
            #[serde(default)]
            pos: Vec<Vec<serde_json::Number>>,
        }

        let file: SagaFile =
            serde_json::from_str(text).context("Saga layouts.json did not parse")?;
        let mut maps: BTreeMap<String, CuratedMap> = BTreeMap::new();
        for (key, layout) in file.layouts {
            let slug = key.split("||").next().unwrap_or(&key).to_string();
            let entry = maps.entry(slug.clone()).or_insert_with(|| CuratedMap {
                name: display_name_from_slug(&slug),
                uids: Vec::new(),
            });
            for pos in &layout.pos {
                if let Some(uid) = pos.first().and_then(|n| n.as_i64()) {
                    entry.uids.push(uid);
                }
            }
        }
        for map in maps.values_mut() {
            map.uids.sort_unstable();
            map.uids.dedup();
        }
        maps.retain(|_, m| !m.uids.is_empty());
        Ok(Self {
            version: SNAPSHOT_VERSION,
            source_layout_version: file.layout_version,
            maps,
        })
    }

    /// The membership shipped with this build (defaults/curated_maps.toml).
    pub fn embedded() -> Result<Self> {
        Self::from_toml(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/defaults/curated_maps.toml"
        )))
    }

    /// Load a snapshot TOML.
    pub fn from_toml(text: &str) -> Result<Self> {
        toml::from_str(text).context("curated_maps.toml did not parse")
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("curated maps did not serialize")
    }

    /// Merge a freshly extracted set over this snapshot: extracted maps
    /// replace same-slug entries wholesale (Simu's roster wins for maps they
    /// still ship); snapshot-only slugs are kept (a map they retired doesn't
    /// yank the rug from users mid-session).
    pub fn merge_from(&mut self, extracted: CuratedMaps) {
        self.source_layout_version = extracted.source_layout_version.or(self.source_layout_version);
        for (slug, map) in extracted.maps {
            self.maps.insert(slug, map);
        }
    }
}

/// "wehnimers-landing-coastal-cliffs" → "Wehnimers Landing Coastal Cliffs".
/// Purely a default; names are override material, never identity.
fn display_name_from_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Locate a Saga install's `layouts.json`. Order: explicit dir argument,
/// `SAGA_RESOURCES_DIR` env, then the stock install path per platform.
/// Returns the layouts.json path only if it actually exists.
pub fn find_saga_layouts(explicit_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = explicit_dir {
        candidates.push(dir.to_path_buf());
    }
    if let Ok(dir) = std::env::var("SAGA_RESOURCES_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    #[cfg(target_os = "windows")]
    candidates.push(PathBuf::from(r"C:\Program Files\Saga\resources"));
    #[cfg(target_os = "macos")]
    candidates.push(PathBuf::from(
        "/Applications/Saga.app/Contents/Resources/resources",
    ));

    for dir in candidates {
        let path = dir.join("map-data").join("prime").join("layouts.json");
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Extract membership straight from a Saga layouts.json on disk.
pub fn extract_from_saga(layouts_json: &Path) -> Result<CuratedMaps> {
    let text = std::fs::read_to_string(layouts_json)
        .with_context(|| format!("could not read {layouts_json:?}"))?;
    CuratedMaps::from_saga_layouts_json(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "layoutVersion": 61,
        "layouts": {
            "town-a||i:1|x:2": {"pos": [[100, 0, 0], [101, 1, 0], [101, 1, 0]]},
            "town-a-annex||i:9": {"pos": [[200, 5, 5]]},
            "empty-map||i:0": {"pos": []}
        }
    }"#;

    #[test]
    fn extracts_membership_and_drops_coordinates() {
        let maps = CuratedMaps::from_saga_layouts_json(SAMPLE).unwrap();
        assert_eq!(maps.source_layout_version, Some(61));
        assert_eq!(maps.maps.len(), 2, "empty layouts are dropped");
        let town = &maps.maps["town-a"];
        assert_eq!(town.uids, vec![100, 101], "sorted and deduped");
        assert_eq!(town.name, "Town A");
        assert_eq!(maps.coverage_len(), 3);
    }

    #[test]
    fn uid_index_is_deterministic_on_shared_uids() {
        let mut maps = CuratedMaps::from_saga_layouts_json(SAMPLE).unwrap();
        // Claim uid 100 from a second map; first slug in BTreeMap order wins.
        maps.maps.get_mut("town-a-annex").unwrap().uids.push(100);
        let index = maps.uid_index();
        assert_eq!(index[&100], "town-a");
        assert_eq!(index[&200], "town-a-annex");
    }

    #[test]
    fn embedded_rosters_parse_and_cover_the_world() {
        let maps = CuratedMaps::embedded().expect("shipped curated_maps.toml must parse");
        assert!(maps.maps.len() >= 100, "expected the full curated set, got {}", maps.maps.len());
        assert!(maps.coverage_len() >= 10_000);
        assert!(maps.maps.contains_key("wehnimers-landing-town"));
    }

    #[test]
    fn toml_round_trip() {
        let maps = CuratedMaps::from_saga_layouts_json(SAMPLE).unwrap();
        let text = maps.to_toml().unwrap();
        let back = CuratedMaps::from_toml(&text).unwrap();
        assert_eq!(maps, back);
    }

    #[test]
    fn merge_replaces_shipped_maps_and_keeps_retired_ones() {
        let mut snapshot = CuratedMaps::from_saga_layouts_json(SAMPLE).unwrap();
        let fresh = CuratedMaps::from_saga_layouts_json(
            r#"{"layoutVersion": 62, "layouts": {"town-a||i:1": {"pos": [[100,0,0],[102,2,0]]}}}"#,
        )
        .unwrap();
        snapshot.merge_from(fresh);
        assert_eq!(snapshot.source_layout_version, Some(62));
        assert_eq!(snapshot.maps["town-a"].uids, vec![100, 102], "roster replaced");
        assert!(snapshot.maps.contains_key("town-a-annex"), "retired map kept");
    }
}
