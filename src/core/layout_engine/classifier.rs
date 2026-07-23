//! Interior classification — port of `interior-classifier.js` (spec §6).
//!
//! Primary signal: indoor rooms print "Obvious exits", outdoor rooms print
//! "Obvious paths" (99.5% coverage); a strict majority decides. Fallbacks: a
//! literal `out` exit leaving the component, weatherless rooms, and
//! propagation (a component reachable only through interiors is interior).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::core::mapdb::{Room, RoomTable};
use super::positioner::Group;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrance {
    pub outdoor_room_id: u32,
    pub interior_room_id: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    pub interior_groups: HashSet<usize>,
    /// interior group index → doorway edges into it.
    pub entrances: HashMap<usize, Vec<Entrance>>,
    /// Outdoor rooms that host a doorway (get door markers).
    pub entrance_room_ids: HashSet<u32>,
}

pub fn classify(groups: &[Group], lookup: &RoomTable) -> Classification {
    let mut component_of: HashMap<u32, usize> = HashMap::new();
    for group in groups {
        for &id in &group.room_ids {
            component_of.insert(id, group.index);
        }
    }

    let mut interior: HashSet<usize> = HashSet::new();
    for group in groups {
        if is_interior_component(group, &component_of, lookup) {
            interior.insert(group.index);
        }
    }

    // Propagate interiority to a fixed point: rooms behind a second door
    // inside a building form their own component with no `out` of their own.
    //
    // Propagation exists to fill in for components with NO paths/exits
    // signal of their own — it must never overrule a decisive outdoor
    // majority. Player-shop boutique streets are the canonical case: 17
    // rooms all printing "Obvious paths", reachable only through the shops
    // they serve, are still streets.
    let mut decisive_outdoor: HashSet<usize> = HashSet::new();
    for group in groups {
        let mut indoor = 0usize;
        let mut outdoor = 0usize;
        for &room_id in &group.room_ids {
            if let Some(room) = lookup.get(room_id) {
                match room_sense(room) {
                    Sense::Indoor => indoor += 1,
                    Sense::Outdoor => outdoor += 1,
                    Sense::Unknown => {}
                }
            }
        }
        if outdoor > indoor {
            decisive_outdoor.insert(group.index);
        }
    }
    let mut neighbor_sets: HashMap<usize, HashSet<usize>> = HashMap::new();
    for group in groups {
        let mut neighbors = HashSet::new();
        for &room_id in &group.room_ids {
            let Some(room) = lookup.get(room_id) else {
                continue;
            };
            for &target_id in room.wayto.keys() {
                if let Some(&target_group) = component_of.get(&target_id) {
                    if target_group != group.index {
                        neighbors.insert(target_group);
                    }
                }
            }
        }
        neighbor_sets.insert(group.index, neighbors);
    }
    let mut changed = true;
    while changed {
        changed = false;
        for group in groups {
            if interior.contains(&group.index) || decisive_outdoor.contains(&group.index) {
                continue;
            }
            let neighbors = &neighbor_sets[&group.index];
            if neighbors.is_empty() {
                continue;
            }
            if neighbors.iter().all(|n| interior.contains(n)) {
                interior.insert(group.index);
                changed = true;
            }
        }
    }

    let (entrances, entrance_room_ids) = compute_entrances(groups, lookup, &interior);

    Classification {
        interior_groups: interior,
        entrances,
        entrance_room_ids,
    }
}

/// Entrances: every edge from an outdoor room into an interior component.
/// Factored out so classification overrides can recompute after flipping
/// groups between sheets.
fn compute_entrances(
    groups: &[Group],
    lookup: &RoomTable,
    interior: &HashSet<usize>,
) -> (HashMap<usize, Vec<Entrance>>, HashSet<u32>) {
    let mut component_of: HashMap<u32, usize> = HashMap::new();
    for group in groups {
        for &id in &group.room_ids {
            component_of.insert(id, group.index);
        }
    }
    let mut entrances: HashMap<usize, Vec<Entrance>> = HashMap::new();
    let mut entrance_room_ids: HashSet<u32> = HashSet::new();
    for group in groups {
        if interior.contains(&group.index) {
            continue;
        }
        for &room_id in &group.room_ids {
            let Some(room) = lookup.get(room_id) else {
                continue;
            };
            for &target_id in room.wayto.keys() {
                let Some(&target_group) = component_of.get(&target_id) else {
                    continue;
                };
                if !interior.contains(&target_group) {
                    continue;
                }
                entrances.entry(target_group).or_default().push(Entrance {
                    outdoor_room_id: room_id,
                    interior_room_id: target_id,
                });
                entrance_room_ids.insert(room_id);
            }
        }
    }
    (entrances, entrance_room_ids)
}

/// Apply classification overrides: flip the listed groups (keyed by anchor)
/// onto the chosen sheet and recompute the doorway markers. Orphaned anchors
/// are skipped silently.
pub fn apply_sheet_overrides(
    classification: &mut Classification,
    groups: &[Group],
    lookup: &RoomTable,
    sheets: &HashMap<i64, super::overrides::SheetChoice>,
) {
    if sheets.is_empty() {
        return;
    }
    let mut by_anchor: HashMap<i64, usize> = HashMap::new();
    for group in groups {
        by_anchor.insert(super::overrides::group_anchor_key(group, lookup), group.index);
    }
    for (&anchor, &choice) in sheets {
        let Some(&idx) = by_anchor.get(&anchor) else {
            continue;
        };
        match choice {
            super::overrides::SheetChoice::Outdoor => {
                classification.interior_groups.remove(&idx);
            }
            super::overrides::SheetChoice::Interior => {
                classification.interior_groups.insert(idx);
            }
        }
    }
    let (entrances, entrance_room_ids) =
        compute_entrances(groups, lookup, &classification.interior_groups);
    classification.entrances = entrances;
    classification.entrance_room_ids = entrance_room_ids;
}

/// Recompute doorway markers from the current interior set — for callers
/// that move groups between sheets after classification (the packer's
/// try-inline pass), mirroring what `apply_sheet_overrides` does for
/// curated flips.
pub fn recompute_entrances(
    classification: &mut Classification,
    groups: &[Group],
    lookup: &RoomTable,
) {
    let (entrances, entrance_room_ids) =
        compute_entrances(groups, lookup, &classification.interior_groups);
    classification.entrances = entrances;
    classification.entrance_room_ids = entrance_room_ids;
}

#[derive(PartialEq)]
enum Sense {
    Indoor,
    Outdoor,
    Unknown,
}

/// "Obvious exits" = indoor, "Obvious paths" = outdoor.
fn room_sense(room: &Room) -> Sense {
    let paths = room.paths.to_lowercase();
    if paths.contains("obvious exits") {
        Sense::Indoor
    } else if paths.contains("obvious paths") {
        Sense::Outdoor
    } else {
        Sense::Unknown
    }
}

fn is_interior_component(
    group: &Group,
    component_of: &HashMap<u32, usize>,
    lookup: &RoomTable,
) -> bool {
    let mut indoor = 0usize;
    let mut outdoor = 0usize;
    for &room_id in &group.room_ids {
        if let Some(room) = lookup.get(room_id) {
            match room_sense(room) {
                Sense::Indoor => indoor += 1,
                Sense::Outdoor => outdoor += 1,
                Sense::Unknown => {}
            }
        }
    }
    if indoor != outdoor && indoor + outdoor > 0 {
        return indoor > outdoor;
    }

    // No usable paths data — structural fallbacks.
    let mut weatherless = 0usize;
    for &room_id in &group.room_ids {
        let Some(room) = lookup.get(room_id) else {
            continue;
        };
        for (&target_id, way) in &room.wayto {
            if way.trim().to_lowercase() != "out" {
                continue;
            }
            // `out` is only a doorway when it LEAVES this component. An `out`
            // that stays inside means the component contains its own outdoors
            // (grottos off a beach) — not a building.
            if component_of.get(&target_id) != Some(&group.index) {
                return true;
            }
        }
        if room.climate.as_deref() == Some("none") && room.terrain.as_deref() == Some("none") {
            weatherless += 1;
        }
    }
    weatherless > 0 && weatherless == group.room_ids.len()
}

/// A component this large is a zone (catacombs, sewers, castle floors),
/// not a room of somebody's building: it gets its own cluster and never
/// welds its neighbors (Wehnimer's underground touches half the town's
/// cellars — without this line every shop with a trapdoor merges into one
/// monster "building"). Spec §10 lists proper large-interior splitting as
/// future work.
pub const ZONE_COMPONENT_ROOMS: usize = 50;

/// Interior clusters: interior groups connected by ANY wayto edge between
/// interior rooms form one walkable interior space (one building) — "go
/// arch" joins as surely as "north". Only edges that lead outdoors (or into
/// a zone-sized component) separate. Returns interior group index → cluster
/// id (the smallest group index in the cluster), so ids are stable for a
/// given mapdb build.
pub fn interior_clusters(
    groups: &[Group],
    interior: &HashSet<usize>,
    lookup: &RoomTable,
) -> HashMap<usize, usize> {
    let is_zone = |idx: usize| groups[idx].room_ids.len() > ZONE_COMPONENT_ROOMS;
    let mut group_of: HashMap<u32, usize> = HashMap::new();
    for group in groups {
        if interior.contains(&group.index) {
            for &id in &group.room_ids {
                group_of.insert(id, group.index);
            }
        }
    }

    // Union-find over interior group indices.
    let mut parent: HashMap<usize, usize> = interior.iter().map(|&g| (g, g)).collect();
    fn find(parent: &mut HashMap<usize, usize>, mut g: usize) -> usize {
        while parent[&g] != g {
            let up = parent[&parent[&g]];
            parent.insert(g, up);
            g = up;
        }
        g
    }
    for group in groups {
        if !interior.contains(&group.index) || is_zone(group.index) {
            continue;
        }
        for &room_id in &group.room_ids {
            let Some(room) = lookup.get(room_id) else {
                continue;
            };
            for &target_id in room.wayto.keys() {
                let Some(&other) = group_of.get(&target_id) else {
                    continue; // outdoors or outside the selection: a boundary
                };
                if other == group.index || is_zone(other) {
                    continue; // a zone is a neighbor, not a wing
                }
                let a = find(&mut parent, group.index);
                let b = find(&mut parent, other);
                if a != b {
                    // Root at the smaller index so cluster ids are canonical.
                    let (lo, hi) = (a.min(b), a.max(b));
                    parent.insert(hi, lo);
                }
            }
        }
    }

    let keys: Vec<usize> = parent.keys().copied().collect();
    keys.into_iter()
        .map(|g| {
            let root = find(&mut parent, g);
            (g, root)
        })
        .collect()
}

/// "[Hamehela's Magic Shoppe]" / "[Manor House, Foyer]" → building name: the
/// most common bracketed prefix among the group's room titles.
pub fn building_name(group: &Group, lookup: &RoomTable) -> Option<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for &room_id in &group.room_ids {
        let title = lookup
            .get(room_id)
            .and_then(|r| r.title.first())
            .map(String::as_str)
            .unwrap_or("");
        let Some(rest) = title.strip_prefix('[') else {
            continue;
        };
        let end = rest.find([',', ']']).unwrap_or(rest.len());
        let name = rest[..end].trim();
        if name.is_empty() {
            continue;
        }
        if let Some(entry) = counts.iter_mut().find(|(n, _)| n == name) {
            entry.1 += 1;
        } else {
            counts.push((name.to_owned(), 1));
        }
    }
    let mut best: Option<(String, usize)> = None;
    for (name, count) in counts {
        if best.as_ref().map(|(_, c)| count > *c).unwrap_or(true) {
            best = Some((name, count));
        }
    }
    best.map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn room(id: u32, wayto: &[(u32, &str)]) -> Room {
        Room {
            id,
            uid: vec![7_000_000 + id as i64],
            location: Some("Test".into()),
            title: vec![],
            description: Vec::new(),
            wayto: wayto
                .iter()
                .map(|&(t, cmd)| (t, cmd.to_string()))
                .collect::<BTreeMap<_, _>>(),
            timeto: BTreeMap::new(),
            dirto: BTreeMap::new(),
            tags: Vec::new(),
            paths: String::new(),
            climate: None,
            terrain: None,
            image: None,
            image_coords: None,
        }
    }

    fn group(index: usize, room_ids: &[u32]) -> Group {
        Group {
            index,
            room_ids: room_ids.to_vec(),
            positions: room_ids
                .iter()
                .enumerate()
                .map(|(i, &id)| {
                    (
                        id,
                        crate::core::layout_engine::Cell {
                            x: i as i32,
                            y: 0,
                        },
                    )
                })
                .collect(),
            violations: vec![],
            base_offset: None,
            packing: None,
            name: None,
        }
    }

    /// The bank: 3850+3672 are one directional component, 3670 hangs off
    /// 3672 via "go arch" (directionless, so its own component), and 3672
    /// leads outside via "out" to 3669. All three interior rooms are ONE
    /// cluster; the outdoor room never joins.
    #[test]
    fn go_arch_joins_a_building_but_out_does_not() {
        let rooms = vec![
            room(3669, &[(3672, "go bank")]),               // outdoors
            room(3670, &[(3672, "go arch")]),               // teller cage
            room(3672, &[(3850, "north"), (3670, "go arch"), (3669, "out")]),
            room(3850, &[(3672, "south")]),
        ];
        let lookup = RoomTable::new(&rooms);
        let groups = vec![
            group(0, &[3672, 3850]), // directional pair
            group(1, &[3670]),       // reached only via "go arch"
            group(2, &[3669]),       // the street outside
        ];
        let interior: HashSet<usize> = [0usize, 1].into_iter().collect();

        let clusters = interior_clusters(&groups, &interior, &lookup);
        assert_eq!(clusters.len(), 2, "both interior groups get a cluster id");
        assert_eq!(
            clusters[&0], clusters[&1],
            "'go arch' must join the bank's sub-groups into one building"
        );
        assert!(
            !clusters.contains_key(&2),
            "the outdoor group is not part of any interior cluster"
        );
    }
}
