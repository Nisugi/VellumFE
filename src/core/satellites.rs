//! Satellite maps — the automatic remainder of curated membership.
//!
//! Curated base maps ([`crate::core::curated_maps`]) claim the hand-drawn
//! spine of the world (streets, major outdoor areas). Everything mappable
//! that is *not* claimed decomposes into connected components of the
//! remaining room graph, and each component is a satellite map: the town
//! well, a bank interior, a treehouse — or an entire hunting ground Saga
//! has never baked. Nothing here is authored; when the curated set grows,
//! the decomposition just recomputes and satellites shrink or vanish.
//!
//! Identity is stable across recomputes: a satellite's key is derived from
//! the smallest game uid among its members (fallback: smallest room id),
//! so layout caches and overrides keyed by it survive mapdb rebuilds.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::core::curated_maps::CuratedMaps;
use crate::core::mapdb::MapDb;

/// Components with fewer mappable rooms than this default get `tiny: true` —
/// consumers annotate the portal room on the base map instead of minting a
/// one-room map.
pub const DEFAULT_MIN_ROOMS: usize = 2;

/// One boundary crossing between a satellite and the world outside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Portal {
    /// Room id inside the satellite.
    pub inside: u32,
    /// Room id on the other side of the edge.
    pub outside: u32,
    /// Curated map slug the outside room belongs to, when it is covered.
    /// `None` means the outside room is another satellite's member (the two
    /// components touch only through covered rooms otherwise, so this is
    /// rare but possible via one-way edges).
    pub base_map: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Satellite {
    /// Stable key: `sat-<min uid>` or `satid-<min room id>`.
    pub key: String,
    /// Auto display name — override material, never identity.
    pub name: String,
    /// Member room ids, ascending.
    pub room_ids: Vec<u32>,
    pub portals: Vec<Portal>,
    /// Below the min-rooms threshold: annotate, don't map.
    pub tiny: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SatelliteIndex {
    pub satellites: BTreeMap<String, Satellite>,
    map_of_room: HashMap<u32, String>,
}

impl SatelliteIndex {
    /// Satellite key containing this mappable room, if any.
    pub fn satellite_of_room(&self, id: u32) -> Option<&str> {
        self.map_of_room.get(&id).map(String::as_str)
    }

    pub fn get(&self, key: &str) -> Option<&Satellite> {
        self.satellites.get(key)
    }

    /// Decompose the un-curated remainder of the mappable world.
    pub fn build(db: &MapDb, curated: &CuratedMaps, min_rooms: usize) -> Self {
        let uid_index = curated.uid_index();
        let covered_map_of = |id: u32| -> Option<&str> {
            let room = db.room(id)?;
            room.uid.iter().find_map(|uid| uid_index.get(uid).copied())
        };

        // All mappable room ids, plus an undirected adjacency over them.
        // wayto is directed; a satellite reachable only via a one-way "go
        // trapdoor" still belongs to the component, so edges count both ways.
        let mut mappable: Vec<u32> = Vec::new();
        let mut adjacency: HashMap<u32, Vec<u32>> = HashMap::new();
        let locations: Vec<&str> = db.locations().collect();
        for location in &locations {
            for room in db.rooms(location).unwrap_or(&[]) {
                mappable.push(room.id);
                for &target in room.wayto.keys() {
                    if db.location_of_room_id(target).is_some() {
                        adjacency.entry(room.id).or_default().push(target);
                        adjacency.entry(target).or_default().push(room.id);
                    }
                }
            }
        }
        mappable.sort_unstable();

        let uncovered: HashSet<u32> = mappable
            .iter()
            .copied()
            .filter(|&id| covered_map_of(id).is_none())
            .collect();

        // BFS components over uncovered rooms, seeded in ascending id order
        // so member lists and traversal are deterministic.
        let mut visited: HashSet<u32> = HashSet::new();
        let mut components: Vec<Vec<u32>> = Vec::new();
        for &seed in &mappable {
            if !uncovered.contains(&seed) || visited.contains(&seed) {
                continue;
            }
            let mut members = Vec::new();
            let mut queue = std::collections::VecDeque::from([seed]);
            visited.insert(seed);
            while let Some(id) = queue.pop_front() {
                members.push(id);
                let mut neighbors: Vec<u32> = adjacency
                    .get(&id)
                    .map(|n| n.iter().copied().collect())
                    .unwrap_or_default();
                neighbors.sort_unstable();
                for next in neighbors {
                    if uncovered.contains(&next) && visited.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
            members.sort_unstable();
            components.push(members);
        }

        let mut satellites: BTreeMap<String, Satellite> = BTreeMap::new();
        let mut map_of_room: HashMap<u32, String> = HashMap::new();
        for members in components {
            let member_set: HashSet<u32> = members.iter().copied().collect();

            let mut portals: Vec<Portal> = Vec::new();
            let mut seen: HashSet<(u32, u32)> = HashSet::new();
            for &id in &members {
                for &other in adjacency.get(&id).map(Vec::as_slice).unwrap_or(&[]) {
                    if member_set.contains(&other) || !seen.insert((id, other)) {
                        continue;
                    }
                    portals.push(Portal {
                        inside: id,
                        outside: other,
                        base_map: covered_map_of(other).map(str::to_string),
                    });
                }
            }
            portals.sort_by_key(|p| (p.inside, p.outside));

            let key = satellite_key(db, &members);
            let name = satellite_name(db, &members);
            for &id in &members {
                map_of_room.insert(id, key.clone());
            }
            let tiny = members.len() < min_rooms;
            satellites.insert(
                key.clone(),
                Satellite {
                    key,
                    name,
                    room_ids: members,
                    portals,
                    tiny,
                },
            );
        }

        Self {
            satellites,
            map_of_room,
        }
    }
}

/// `sat-<min uid>` over every member uid; uid-less components fall back to
/// `satid-<min room id>`. Min-uid is the most durable anchor: if Simu later
/// bakes part of the component, the smallest uid usually remains inside.
fn satellite_key(db: &MapDb, members: &[u32]) -> String {
    let min_uid = members
        .iter()
        .filter_map(|&id| db.room(id))
        .flat_map(|room| room.uid.iter().copied())
        .min();
    match min_uid {
        Some(uid) => format!("sat-{uid}"),
        None => format!("satid-{}", members.first().copied().unwrap_or(0)),
    }
}

/// Dominant mapdb location of the members, suffixed with the lowest-id
/// room's title when the component is only part of that location:
/// "Wehnimer's Landing: Town Well". A component that IS its whole location
/// (an area Saga hasn't baked at all) just keeps the location name.
fn satellite_name(db: &MapDb, members: &[u32]) -> String {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for &id in members {
        if let Some(location) = db.location_of_room_id(id) {
            *counts.entry(location).or_default() += 1;
        }
    }
    let Some((&location, _)) = counts
        .iter()
        .max_by_key(|(loc, n)| (**n, std::cmp::Reverse(*loc)))
    else {
        return format!("Satellite {}", members.first().copied().unwrap_or(0));
    };
    let location_total = db.rooms(location).map(<[_]>::len).unwrap_or(0);
    if members.len() >= location_total {
        return location.to_string();
    }
    let title = members
        .first()
        .and_then(|&id| db.room(id))
        .and_then(|room| room.title.first())
        .map(|t| t.trim_matches(['[', ']']).to_string())
        .unwrap_or_default();
    if title.is_empty() {
        location.to_string()
    } else {
        format!("{location}: {title}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::curated_maps::CuratedMaps;

    /// A street of two covered rooms (uids 100, 101); a two-room well off
    /// the street (200, 201); a one-room closet (300); and an entire
    /// uncovered location (400, 401) not touching anything covered.
    const WORLD: &str = r#"[
        {"id": 1, "uid": [100], "location": "Town",
         "title": ["[Town, Square]"],
         "wayto": {"2": "east", "10": "go well", "20": "go closet"},
         "timeto": {"2": 0.2, "10": 0.2, "20": 0.2}, "paths": ""},
        {"id": 2, "uid": [101], "location": "Town",
         "title": ["[Town, East]"], "wayto": {"1": "west"},
         "timeto": {"1": 0.2}, "paths": ""},
        {"id": 10, "uid": [200], "location": "Town",
         "title": ["[Town, Well Top]"], "wayto": {"1": "out", "11": "down"},
         "timeto": {"1": 0.2, "11": 0.2}, "paths": ""},
        {"id": 11, "uid": [201], "location": "Town",
         "title": ["[Town, Well Bottom]"], "wayto": {"10": "up"},
         "timeto": {"10": 0.2}, "paths": ""},
        {"id": 20, "uid": [300], "location": "Town",
         "title": ["[Town, Closet]"], "wayto": {"1": "out"},
         "timeto": {"1": 0.2}, "paths": ""},
        {"id": 30, "uid": [400], "location": "Wilds",
         "title": ["[Wilds, Edge]"], "wayto": {"31": "north"},
         "timeto": {"31": 0.2}, "paths": ""},
        {"id": 31, "uid": [401], "location": "Wilds",
         "title": ["[Wilds, Deep]"], "wayto": {"30": "south"},
         "timeto": {"30": 0.2}, "paths": ""}
    ]"#;

    fn curated() -> CuratedMaps {
        CuratedMaps::from_saga_layouts_json(
            r#"{"layoutVersion": 1, "layouts":
                {"town||i:1": {"pos": [[100, 0, 0], [101, 1, 0]]}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn decomposes_remainder_into_components() {
        let db = MapDb::from_json(WORLD).unwrap();
        let index = SatelliteIndex::build(&db, &curated(), DEFAULT_MIN_ROOMS);
        assert_eq!(index.satellites.len(), 3, "well, closet, wilds");

        let well = index.get("sat-200").expect("well satellite");
        assert_eq!(well.room_ids, vec![10, 11]);
        assert!(!well.tiny);
        assert_eq!(well.name, "Town: Town, Well Top");
        assert_eq!(
            well.portals,
            vec![Portal {
                inside: 10,
                outside: 1,
                base_map: Some("town".into())
            }]
        );

        let closet = index.get("sat-300").expect("closet satellite");
        assert!(closet.tiny, "one room is below the threshold");

        // The whole uncovered location keeps its own name and has no portals
        // (nothing connects it to a covered room).
        let wilds = index.get("sat-400").expect("wilds satellite");
        assert_eq!(wilds.name, "Wilds");
        assert!(wilds.portals.is_empty());
    }

    #[test]
    fn room_lookup_and_one_way_edges() {
        // Reverse the well entrance to one-way (only street -> well): the
        // well must still form a component, discovered via the undirected
        // adjacency.
        let world = WORLD.replace(
            r#""wayto": {"1": "out", "11": "down"}"#,
            r#""wayto": {"11": "down"}"#,
        );
        let db = MapDb::from_json(&world).unwrap();
        let index = SatelliteIndex::build(&db, &curated(), DEFAULT_MIN_ROOMS);
        assert_eq!(index.satellite_of_room(11), Some("sat-200"));
        assert_eq!(
            index.satellite_of_room(1),
            None,
            "covered rooms belong to base maps"
        );
        let well = index.get("sat-200").unwrap();
        assert_eq!(
            well.portals.len(),
            1,
            "inbound-only edge still yields a portal"
        );
    }

    /// Real-world decomposition report. Ignored by default; run with
    /// `VELLUM_MAPDB=<map-*.json> SAGA_RESOURCES_DIR=<dir> cargo test
    /// real_world_decomposition -- --ignored --nocapture` to see how the
    /// live world actually splits.
    #[test]
    #[ignore]
    fn real_world_decomposition_report() {
        let Ok(mapdb_path) = std::env::var("VELLUM_MAPDB") else {
            eprintln!("set VELLUM_MAPDB to a Lich map-*.json");
            return;
        };
        let layouts =
            crate::core::curated_maps::find_saga_layouts(None).expect("no Saga install found");
        let curated = crate::core::curated_maps::extract_from_saga(&layouts).unwrap();
        let db = MapDb::load(std::path::Path::new(&mapdb_path)).unwrap();
        let index = SatelliteIndex::build(&db, &curated, DEFAULT_MIN_ROOMS);

        let total: usize = index.satellites.values().map(|s| s.room_ids.len()).sum();
        let tiny = index.satellites.values().filter(|s| s.tiny).count();
        let with_portals = index
            .satellites
            .values()
            .filter(|s| s.portals.iter().any(|p| p.base_map.is_some()))
            .count();
        println!(
            "curated: {} maps / {} uids; satellites: {} ({} tiny, {} portal into a base map), {} rooms total",
            curated.maps.len(),
            curated.coverage_len(),
            index.satellites.len(),
            tiny,
            with_portals,
            total,
        );
        let mut biggest: Vec<&Satellite> = index.satellites.values().collect();
        biggest.sort_by_key(|s| std::cmp::Reverse(s.room_ids.len()));
        for sat in biggest.iter().take(25) {
            println!(
                "  {:<12} {:>5} rooms  {:>3} portals  {}",
                sat.key,
                sat.room_ids.len(),
                sat.portals.len(),
                sat.name
            );
        }
        let sizes = [1usize, 2, 5, 10, 50, 200, usize::MAX];
        let mut last = 0usize;
        for &cap in &sizes {
            let n = index
                .satellites
                .values()
                .filter(|s| s.room_ids.len() > last && s.room_ids.len() <= cap)
                .count();
            println!(
                "  size {:>4}..{:<10} {n}",
                last + 1,
                if cap == usize::MAX {
                    "up".into()
                } else {
                    cap.to_string()
                }
            );
            last = cap;
        }
    }

    #[test]
    fn coverage_growth_shrinks_satellites_but_keeps_keys_stable() {
        let db = MapDb::from_json(WORLD).unwrap();
        // Simu bakes the well top (uid 200) into town: the satellite shrinks
        // to the bottom room but keeps... a NEW key (min uid moved). This
        // documents the acceptable churn: keys only change when the old
        // min-uid room itself gets baked.
        let grown = CuratedMaps::from_saga_layouts_json(
            r#"{"layoutVersion": 2, "layouts":
                {"town||i:1": {"pos": [[100,0,0],[101,1,0],[200,2,0]]}}}"#,
        )
        .unwrap();
        let index = SatelliteIndex::build(&db, &grown, DEFAULT_MIN_ROOMS);
        let bottom = index.get("sat-201").expect("shrunken well");
        assert_eq!(bottom.room_ids, vec![11]);
        assert!(bottom.tiny);
        assert_eq!(bottom.portals[0].base_map.as_deref(), Some("town"));
    }
}
