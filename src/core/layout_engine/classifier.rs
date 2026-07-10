//! Interior classification — port of `interior-classifier.js` (spec §6).
//!
//! Primary signal: indoor rooms print "Obvious exits", outdoor rooms print
//! "Obvious paths" (99.5% coverage); a strict majority decides. Fallbacks: a
//! literal `out` exit leaving the component, weatherless rooms, and
//! propagation (a component reachable only through interiors is interior).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::mapdb::{Room, RoomTable};
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
            if interior.contains(&group.index) {
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

    // Entrances: every edge from an outdoor room into an interior component.
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

    Classification {
        interior_groups: interior,
        entrances,
        entrance_room_ids,
    }
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
