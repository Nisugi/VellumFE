//! Map layout engine — Rust port of the mapgen reference implementation.
//!
//! Generates a 2D grid layout for one *location* of the Lich mapdb, live, on
//! area entry (docs/layout-engine-spec.md). Pure functions, no frontend
//! imports: rooms in, layout model out. Deterministic for fixed input; the
//! canonical room iteration order is ascending room id.
//!
//! Pipeline: direction analysis → BFS component placement with grid rips →
//! per-component hill-climb + compaction → interior classification →
//! cluster packing (outdoor sheet) + interior shelf.

pub mod cache;
pub mod classifier;
pub mod direction;
pub mod overrides;
pub mod packer;
pub mod positioner;
pub mod scene;

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::core::mapdb::{Room, RoomTable};

pub use cache::{rooms_content_hash, CacheOutcome, LayoutCache};
pub use classifier::Classification;
pub use overrides::{EdgeAction, EdgeOverride, LocationOverrides, MapOverrides, SheetChoice};
pub use packer::PackInfo;
pub use scene::{build_scene, MapScene, SceneEdgeKind, Sheet};
pub use positioner::{Cell, Group, PackMethod, Violation};

/// A generated layout: every component with internal positions and sheet
/// offsets, plus the interior/outdoor split and packing debug info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub groups: Vec<Group>,
    /// Indices (into `groups`) packed onto the shared outdoor sheet.
    pub outdoor: Vec<usize>,
    /// Indices placed on the separate interiors shelf sheet.
    pub interiors: Vec<usize>,
    pub classification: Classification,
    pub pack_info: PackInfo,
}

/// Run the full pipeline over one location's rooms. `rooms` should already be
/// filtered to the location; order does not matter (they are re-sorted to the
/// canonical ascending-id order).
pub fn generate_layout(rooms: &mut Vec<Room>) -> Layout {
    generate_layout_curated(rooms, &LocationOverrides::default())
}

/// `generate_layout` with the generation-input override subset applied:
/// forced/demoted edge directions patch the direction map before
/// positioning, and classification flips move groups between sheets before
/// packing. Position pins and names are NOT applied here (see
/// `overrides::apply`) so cached layouts stay reusable across those edits.
pub fn generate_layout_curated(rooms: &mut Vec<Room>, curated: &LocationOverrides) -> Layout {
    rooms.sort_by_key(|r| r.id);
    let lookup = RoomTable::new(rooms);
    let mut dirs = direction::DirectionMap::build(&lookup);
    dirs.apply_edge_overrides(&lookup, &curated.edges);

    let mut groups = positioner::position_rooms(&lookup, &dirs);
    let mut classification = classifier::classify(&groups, &lookup);
    classifier::apply_sheet_overrides(&mut classification, &groups, &lookup, &curated.sheets);

    let mut outdoor: Vec<usize> = groups
        .iter()
        .map(|g| g.index)
        .filter(|i| !classification.interior_groups.contains(i))
        .collect();
    let mut interiors: Vec<usize> = groups
        .iter()
        .map(|g| g.index)
        .filter(|i| classification.interior_groups.contains(i))
        .collect();
    // A selection that is entirely interiors skips the split entirely.
    if outdoor.is_empty() {
        outdoor = groups.iter().map(|g| g.index).collect();
        interiors.clear();
    }

    let pack_info = packer::pack_groups(&mut groups, &outdoor, &lookup, &dirs);
    let clusters =
        classifier::interior_clusters(&groups, &classification.interior_groups, &lookup);
    packer::pack_interior_shelf(&mut groups, &interiors, &clusters, &lookup);

    // Building names for interior groups (assigned after shelf packing so the
    // shelf order matches the reference, which sorts unnamed groups).
    for &idx in &interiors {
        groups[idx].name = classifier::building_name(&groups[idx], &lookup);
    }

    Layout {
        groups,
        outdoor,
        interiors,
        classification,
        pack_info,
    }
}

/// Statistical summary matching `docs/layout-fixtures.json` zone entries
/// (produced by the reference's `tools/export-fixtures.mjs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutStats {
    pub rooms: usize,
    pub components: usize,
    pub outdoor_components: usize,
    pub outdoor_rooms: usize,
    pub interior_components: usize,
    pub interior_rooms: usize,
    pub direction_violations: usize,
    pub entrance_rooms: usize,
    pub cell_overlaps: usize,
    pub inter_group_connectors: usize,
    pub connector_len_median: Option<i32>,
    pub connector_len_p90: Option<i32>,
    pub pack_methods: BTreeMap<String, usize>,
    pub primary_image: Option<String>,
}

impl LayoutStats {
    pub fn compute(layout: &Layout, rooms: &[Room]) -> LayoutStats {
        // Final outdoor-sheet cells and component membership per room.
        let mut fin: HashMap<u32, Cell> = HashMap::new();
        let mut comp_of: HashMap<u32, usize> = HashMap::new();
        for &idx in &layout.outdoor {
            let group = &layout.groups[idx];
            for &id in &group.room_ids {
                fin.insert(id, group.final_cell(id));
                comp_of.insert(id, idx);
            }
        }

        let mut cells: HashSet<Cell> = HashSet::new();
        let mut overlaps = 0;
        for c in fin.values() {
            if !cells.insert(*c) {
                overlaps += 1;
            }
        }

        // Inter-group connectors: cross-component wayto pairs on the outdoor
        // sheet, deduped by unordered pair, measured in Chebyshev cells.
        let mut connector_lens: Vec<i32> = Vec::new();
        let mut seen: HashSet<(u32, u32)> = HashSet::new();
        for room in rooms {
            for &target in room.wayto.keys() {
                let (Some(&a), Some(&b)) = (fin.get(&room.id), fin.get(&target)) else {
                    continue;
                };
                if comp_of[&room.id] == comp_of[&target] {
                    continue;
                }
                let key = (room.id.min(target), room.id.max(target));
                if !seen.insert(key) {
                    continue;
                }
                connector_lens.push((a.x - b.x).abs().max((a.y - b.y).abs()));
            }
        }
        connector_lens.sort_unstable();
        let quantile = |f: f64| -> Option<i32> {
            if connector_lens.is_empty() {
                return None;
            }
            let idx = (connector_lens.len() as f64 * f).floor() as usize;
            Some(
                connector_lens
                    .get(idx)
                    .copied()
                    .unwrap_or(*connector_lens.last().unwrap()),
            )
        };

        LayoutStats {
            rooms: rooms.len(),
            components: layout.groups.len(),
            outdoor_components: layout.outdoor.len(),
            outdoor_rooms: layout
                .outdoor
                .iter()
                .map(|&i| layout.groups[i].room_ids.len())
                .sum(),
            interior_components: layout.interiors.len(),
            interior_rooms: layout
                .interiors
                .iter()
                .map(|&i| layout.groups[i].room_ids.len())
                .sum(),
            direction_violations: layout.groups.iter().map(|g| g.violations.len()).sum(),
            entrance_rooms: layout.classification.entrance_room_ids.len(),
            cell_overlaps: overlaps,
            inter_group_connectors: connector_lens.len(),
            connector_len_median: quantile(0.5),
            connector_len_p90: quantile(0.9),
            pack_methods: layout.pack_info.methods.clone(),
            primary_image: layout.pack_info.primary_image.clone(),
        }
    }
}
