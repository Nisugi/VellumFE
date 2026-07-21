//! Human curation overrides (spec §8): a sparse, uid-keyed diff applied
//! AFTER generation. Cached layouts stay pristine; overrides survive mapdb
//! renumbering because keys are game uids (Lich ids only as a fallback for
//! uid-less rooms — uids are ≥ 7 digits, so the key spaces never collide).
//! Orphaned overrides (anchor no longer resolves) are skipped silently: the
//! solver's placement shows.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::direction::Dir;
use crate::core::mapdb::RoomTable;
use super::positioner::{Cell, Group};
use super::Layout;

/// What to do with the edge between two rooms (keyed by uid pair).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeAction {
    /// Don't draw the edge at all (presentation only).
    Hide,
    /// Draw the edge dashed but leave geometry alone (presentation only) —
    /// for "go well"-style passages the solver placed correctly where a
    /// solid line would overstate the connection. Unlike `Connector`, the
    /// edge keeps anchoring positioning, so the layout does not change.
    Dash,
    /// Treat as a directionless passage: no geometry constraint, drawn
    /// dashed. Un-welds rooms the solver placed adjacent on bad data.
    Connector,
    /// Force this direction from `a` to `b` (`b` to `a` gets the opposite).
    /// Applied before positioning, so the rooms lay out accordingly.
    Direction(Dir),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeOverride {
    /// Room keys (uid, fallback id), canonically `a < b`.
    pub a: i64,
    pub b: i64,
    pub action: EdgeAction,
}

/// Force a group onto a sheet regardless of what the classifier decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetChoice {
    Outdoor,
    Interior,
}

/// All override state, one section per location.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MapOverrides {
    #[serde(default)]
    pub locations: HashMap<String, LocationOverrides>,
}

/// Layer a personal overrides section on top of a community base, at use
/// time — the personal file on disk never absorbs community data. Personal
/// wins per key, except group offsets which ADD: both layers are deltas
/// from the clean layout, so dragging composes with the community placement
/// and a personal net-zero re-exposes it.
pub fn merge_location(
    community: Option<&LocationOverrides>,
    personal: Option<&LocationOverrides>,
) -> LocationOverrides {
    let mut merged = community.cloned().unwrap_or_default();
    let Some(personal) = personal else {
        return merged;
    };
    for (&anchor, &delta) in &personal.group_offsets {
        let cur = merged.group_offsets.entry(anchor).or_default();
        cur.x += delta.x;
        cur.y += delta.y;
        if cur.x == 0 && cur.y == 0 {
            merged.group_offsets.remove(&anchor);
        }
    }
    for (&k, &v) in &personal.room_pins {
        merged.room_pins.insert(k, v);
    }
    for (&k, v) in &personal.names {
        merged.names.insert(k, v.clone());
    }
    for edge in &personal.edges {
        match merged
            .edges
            .iter_mut()
            .find(|e| (e.a, e.b) == (edge.a, edge.b))
        {
            Some(existing) => *existing = *edge,
            None => merged.edges.push(*edge),
        }
    }
    for (&k, &v) in &personal.sheets {
        merged.sheets.insert(k, v);
    }
    merged
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocationOverrides {
    /// Group offset delta (cells), keyed by the group's anchor
    /// (lowest room uid in the group; fallback lowest room id).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub group_offsets: HashMap<i64, Cell>,
    /// Room position pin, relative to its group's frame (the group's
    /// internal coordinates), keyed by room uid (fallback id).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub room_pins: HashMap<i64, Cell>,
    /// Display names, keyed by group anchor.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub names: HashMap<i64, String>,
    /// Edge overrides, keyed by canonical room-key pair.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeOverride>,
    /// Classification overrides, keyed by group anchor.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub sheets: HashMap<i64, SheetChoice>,
}

impl LocationOverrides {
    pub fn is_empty(&self) -> bool {
        self.group_offsets.is_empty()
            && self.room_pins.is_empty()
            && self.names.is_empty()
            && self.edges.is_empty()
            && self.sheets.is_empty()
    }

    /// The subset that changes GENERATION (not just presentation): edge and
    /// classification overrides feed positioning and packing, so the cache
    /// entry records a hash of them and regenerates when it moves. Position
    /// pins and names stay out — they apply after loading. `Dash` edges are
    /// pure styling and stay out too, so dashing a line is a cache hit, not
    /// a re-layout. (`Hide` is also presentation-only but stays in: dropping
    /// it would shift every existing user's curated hashes for no gain.)
    pub fn generation_subset(&self) -> LocationOverrides {
        LocationOverrides {
            edges: self
                .edges
                .iter()
                .filter(|e| e.action != EdgeAction::Dash)
                .cloned()
                .collect(),
            sheets: self.sheets.clone(),
            ..Default::default()
        }
    }

    /// Stable hash of the generation subset (0 when empty).
    pub fn curated_hash(&self) -> u64 {
        if self.edges.is_empty() && self.sheets.is_empty() {
            return 0;
        }
        let mut edges = self.edges.clone();
        edges.sort_by_key(|e| (e.a, e.b));
        let mut sheets: Vec<(i64, SheetChoice)> =
            self.sheets.iter().map(|(&k, &v)| (k, v)).collect();
        sheets.sort_by_key(|&(k, _)| k);
        // Debug formatting is stable for these plain enums/ints.
        let repr = format!("{edges:?}|{sheets:?}");
        super::cache::fnv1a_64(repr.as_bytes())
    }
}

/// Room key → room id for this mapdb build, for resolving edge overrides.
pub fn room_key_index(lookup: &RoomTable) -> HashMap<i64, u32> {
    lookup
        .rooms()
        .iter()
        .map(|r| (r.uid.first().copied().unwrap_or(r.id as i64), r.id))
        .collect()
}

/// Canonical edge-override key order.
pub fn edge_pair(k1: i64, k2: i64) -> (i64, i64) {
    (k1.min(k2), k1.max(k2))
}

/// The stable identity of a group across mapdb builds: its lowest room uid,
/// or (for uid-less groups) its lowest room id.
pub fn group_anchor_key(group: &Group, lookup: &RoomTable) -> i64 {
    group
        .room_ids
        .iter()
        .filter_map(|&id| lookup.get(id).and_then(|r| r.uid.first().copied()))
        .min()
        .unwrap_or_else(|| group.room_ids.iter().copied().min().unwrap_or(0) as i64)
}

/// A room's override key: its uid, or its id when it has none.
pub fn room_key(id: u32, lookup: &RoomTable) -> i64 {
    lookup
        .get(id)
        .and_then(|r| r.uid.first().copied())
        .unwrap_or(id as i64)
}

/// Apply a location's overrides to a freshly generated (or cache-loaded)
/// layout. Group offsets shift the whole group's frame; room pins replace a
/// room's internal position; names replace group names.
pub fn apply(layout: &mut Layout, lookup: &RoomTable, ov: &LocationOverrides) {
    if ov.is_empty() {
        return;
    }
    // Anchor → group index, resolved against this build's groups.
    let mut by_anchor: HashMap<i64, usize> = HashMap::new();
    for group in &layout.groups {
        by_anchor.insert(group_anchor_key(group, lookup), group.index);
    }

    for (&anchor, &delta) in &ov.group_offsets {
        let Some(&idx) = by_anchor.get(&anchor) else {
            continue; // orphaned: mapdb no longer has this group
        };
        if let Some(off) = &mut layout.groups[idx].base_offset {
            off.x += delta.x;
            off.y += delta.y;
        }
    }

    for (&anchor, name) in &ov.names {
        if let Some(&idx) = by_anchor.get(&anchor) {
            layout.groups[idx].name = Some(name.clone());
        }
    }

    if !ov.room_pins.is_empty() {
        // room key → (group, room id) for every placed room.
        let mut keys: HashMap<i64, (usize, u32)> = HashMap::new();
        for group in &layout.groups {
            for &id in &group.room_ids {
                keys.insert(room_key(id, lookup), (group.index, id));
            }
        }
        for (&key, &pin) in &ov.room_pins {
            let Some(&(idx, id)) = keys.get(&key) else {
                continue;
            };
            layout.groups[idx].positions.insert(id, pin);
        }
    }
}

pub fn load(path: &Path) -> MapOverrides {
    match std::fs::read_to_string(path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
            tracing::warn!("map overrides unreadable ({e}); starting fresh");
            MapOverrides::default()
        }),
        Err(_) => MapOverrides::default(),
    }
}

pub fn save(path: &Path, overrides: &MapOverrides) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(overrides)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_layers_personal_over_community() {
        let mut community = LocationOverrides::default();
        community.group_offsets.insert(100, Cell { x: 2, y: 0 });
        community.room_pins.insert(200, Cell { x: 1, y: 1 });
        community.names.insert(100, "Community Name".into());
        community.edges.push(EdgeOverride {
            a: 1,
            b: 2,
            action: EdgeAction::Hide,
        });
        community.sheets.insert(100, SheetChoice::Outdoor);

        let mut personal = LocationOverrides::default();
        // Offsets add (both are deltas from the clean layout).
        personal.group_offsets.insert(100, Cell { x: -1, y: 3 });
        // Everything else: personal wins per key.
        personal.room_pins.insert(200, Cell { x: 5, y: 5 });
        personal.edges.push(EdgeOverride {
            a: 1,
            b: 2,
            action: EdgeAction::Dash,
        });
        personal.edges.push(EdgeOverride {
            a: 3,
            b: 4,
            action: EdgeAction::Connector,
        });

        let merged = merge_location(Some(&community), Some(&personal));
        assert_eq!(merged.group_offsets[&100], Cell { x: 1, y: 3 });
        assert_eq!(merged.room_pins[&200], Cell { x: 5, y: 5 });
        assert_eq!(merged.names[&100], "Community Name");
        assert_eq!(merged.edges.len(), 2);
        assert_eq!(
            merged.edges.iter().find(|e| (e.a, e.b) == (1, 2)).unwrap().action,
            EdgeAction::Dash
        );
        assert_eq!(merged.sheets[&100], SheetChoice::Outdoor);

        // A personal offset that cancels the community one removes the entry.
        let mut cancel = LocationOverrides::default();
        cancel.group_offsets.insert(100, Cell { x: -2, y: 0 });
        let merged = merge_location(Some(&community), Some(&cancel));
        assert!(!merged.group_offsets.contains_key(&100));

        // No community layer: personal passes through unchanged.
        let merged = merge_location(None, Some(&personal));
        assert_eq!(merged.group_offsets[&100], Cell { x: -1, y: 3 });
    }

    #[test]
    fn empty_overrides_are_a_noop_shape() {
        let ov = LocationOverrides::default();
        assert!(ov.is_empty());
    }

    /// Dash is styling, not a generation input: adding one must not move the
    /// curated hash (else dashing a line needlessly re-lays-out the zone).
    #[test]
    fn dash_edges_stay_out_of_the_generation_hash() {
        let mut ov = LocationOverrides::default();
        ov.edges.push(EdgeOverride {
            a: 100,
            b: 200,
            action: EdgeAction::Connector,
        });
        let baseline = ov.generation_subset().curated_hash();

        ov.edges.push(EdgeOverride {
            a: 300,
            b: 400,
            action: EdgeAction::Dash,
        });
        let subset = ov.generation_subset();
        assert_eq!(subset.edges.len(), 1, "Dash filtered out of the subset");
        assert_eq!(subset.curated_hash(), baseline, "hash unmoved by Dash");

        // A location curated ONLY with dashes hashes as uncurated.
        let only_dash = LocationOverrides {
            edges: vec![EdgeOverride {
                a: 1,
                b: 2,
                action: EdgeAction::Dash,
            }],
            ..Default::default()
        };
        assert_eq!(only_dash.generation_subset().curated_hash(), 0);
        assert!(!only_dash.is_empty(), "but it still persists to disk");
    }
}
