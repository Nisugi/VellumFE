//! Map membership — which map every mappable room belongs to.
//!
//! Replaces raw mapdb-`location` bucketing when curated data is available:
//! curated base maps ([`crate::core::curated_maps`]) claim their uid
//! rosters, satellites ([`crate::core::satellites`]) claim the remainder,
//! and every map key — curated slug or `sat-*` — is an opaque string to
//! everything downstream (generation, cache, overrides, switching).
//!
//! Tiny satellites (below the min-rooms threshold) are not minted as maps:
//! standing in a one-room closet resolves to the base map its portal opens
//! from, so the street stays on screen — the closet is annotation material,
//! not a map.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::core::curated_maps::CuratedMaps;
use crate::core::mapdb::MapDb;
use crate::core::satellites::{Satellite, SatelliteIndex, DEFAULT_MIN_ROOMS};

#[derive(Debug, Clone, Default)]
pub struct Membership {
    /// room id → map key, for every mappable room that resolves to a map.
    map_of_room: HashMap<u32, String>,
    /// map key → member room ids, ascending. Curated slugs and satellite
    /// keys share one namespace.
    rooms_of_map: BTreeMap<String, Vec<u32>>,
    /// map key → display name.
    names: HashMap<String, String>,
    /// The decomposition itself, kept for portals/annotations/explorer.
    pub satellites: SatelliteIndex,
    /// Curated slugs, for "is this a base map" checks.
    curated_keys: std::collections::HashSet<String>,
}

impl Membership {
    pub fn map_of_room(&self, id: u32) -> Option<&str> {
        self.map_of_room.get(&id).map(String::as_str)
    }

    pub fn rooms_of_map(&self, key: &str) -> Option<&[u32]> {
        self.rooms_of_map.get(key).map(Vec::as_slice)
    }

    pub fn display_name<'a>(&'a self, key: &'a str) -> &'a str {
        self.names.get(key).map(String::as_str).unwrap_or(key)
    }

    pub fn is_curated(&self, key: &str) -> bool {
        self.curated_keys.contains(key)
    }

    /// Every map key with its display name and size, curated maps first
    /// (each group sorted by name) — the explorer's listing.
    pub fn list_maps(&self) -> Vec<(String, String, usize, bool)> {
        let mut out: Vec<(String, String, usize, bool)> = self
            .rooms_of_map
            .iter()
            .map(|(key, rooms)| {
                (
                    key.clone(),
                    self.display_name(key).to_string(),
                    rooms.len(),
                    self.is_curated(key),
                )
            })
            .collect();
        out.sort_by(|a, b| (!a.3, a.1.to_lowercase()).cmp(&(!b.3, b.1.to_lowercase())));
        out
    }

    /// Build the full membership from the mapdb and curated rosters.
    pub fn build(db: &MapDb, curated: &CuratedMaps) -> Arc<Membership> {
        let satellites = SatelliteIndex::build(db, curated, DEFAULT_MIN_ROOMS);
        let uid_index = curated.uid_index();

        let mut map_of_room: HashMap<u32, String> = HashMap::new();
        let mut rooms_of_map: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        let mut names: HashMap<String, String> = HashMap::new();
        let mut curated_keys = std::collections::HashSet::new();

        // Curated coverage first: a room is in a base map when any of its
        // uids is on that map's roster (same rule the decomposition used).
        let locations: Vec<String> = db.locations().map(str::to_owned).collect();
        for location in &locations {
            for room in db.rooms(location).unwrap_or(&[]) {
                if let Some(&slug) = room.uid.iter().find_map(|uid| uid_index.get(uid)) {
                    map_of_room.insert(room.id, slug.to_string());
                    rooms_of_map
                        .entry(slug.to_string())
                        .or_default()
                        .push(room.id);
                }
            }
        }
        // Every curated map registers — including EMPTY rosters. A user-
        // created map (or Purgatory) starts with no rooms and must still
        // appear in the picker and as a move target; from_saga drops empty
        // layouts, so empties here are user-authored, not Simu noise.
        for (slug, map) in &curated.maps {
            rooms_of_map.entry(slug.clone()).or_default();
            names.insert(slug.clone(), map.name.clone());
            curated_keys.insert(slug.clone());
        }

        // Satellites claim the remainder. Tiny ones resolve to the base map
        // behind their first base portal instead of minting a map; a tiny
        // component with no base portal still gets its map (better a silly
        // one-room map than nowhere to stand).
        for satellite in satellites.satellites.values() {
            let tiny_home = satellite
                .tiny
                .then(|| satellite.portals.iter().find_map(|p| p.base_map.clone()))
                .flatten();
            match tiny_home {
                Some(base) => {
                    for &id in &satellite.room_ids {
                        map_of_room.insert(id, base.clone());
                        // NOT added to rooms_of_map: the base layout doesn't
                        // place the closet; the view just holds the street.
                    }
                }
                None => {
                    for &id in &satellite.room_ids {
                        map_of_room.insert(id, satellite.key.clone());
                    }
                    rooms_of_map.insert(satellite.key.clone(), satellite.room_ids.clone());
                    names.insert(satellite.key.clone(), satellite.name.clone());
                }
            }
        }

        for rooms in rooms_of_map.values_mut() {
            rooms.sort_unstable();
        }

        Arc::new(Membership {
            map_of_room,
            rooms_of_map,
            names,
            satellites,
            curated_keys,
        })
    }

    /// The satellite whose portals sit on this base-map room, if any —
    /// the map renderer's "there is an enterable place here" marker.
    pub fn satellite_at_base_room(&self, room_id: u32) -> Option<&Satellite> {
        self.satellites
            .satellites
            .values()
            .find(|s| s.portals.iter().any(|p| p.outside == room_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::curated_maps::CuratedMaps;

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
    fn rooms_resolve_to_curated_satellite_or_tiny_home() {
        let db = MapDb::from_json(WORLD).unwrap();
        let m = Membership::build(&db, &curated());

        // Covered street rooms → the curated map.
        assert_eq!(m.map_of_room(1), Some("town"));
        assert_eq!(m.rooms_of_map("town"), Some(&[1u32, 2][..]));
        assert!(m.is_curated("town"));
        assert_eq!(m.display_name("town"), "Town");

        // The well is a real satellite map.
        assert_eq!(m.map_of_room(10), Some("sat-200"));
        assert_eq!(m.rooms_of_map("sat-200"), Some(&[10u32, 11][..]));
        assert!(!m.is_curated("sat-200"));

        // The closet is tiny: it resolves to the base map behind its portal
        // and mints no map of its own.
        assert_eq!(m.map_of_room(20), Some("town"));
        assert!(m.rooms_of_map("town").unwrap().iter().all(|&id| id != 20));
        assert!(m.rooms_of_map("sat-300").is_none());

        // The uncovered wilderness is its own satellite map.
        assert_eq!(m.map_of_room(30), Some("sat-400"));
        assert_eq!(m.display_name("sat-400"), "Wilds");
    }

    #[test]
    fn listing_puts_curated_maps_first() {
        let db = MapDb::from_json(WORLD).unwrap();
        let m = Membership::build(&db, &curated());
        let list = m.list_maps();
        assert_eq!(list.first().map(|e| e.0.as_str()), Some("town"));
        assert!(
            list.iter().skip(1).all(|e| !e.3),
            "satellites after curated"
        );
    }

    #[test]
    fn base_room_annotations_find_their_satellite() {
        let db = MapDb::from_json(WORLD).unwrap();
        let m = Membership::build(&db, &curated());
        // Room 1 (Town Square) is the portal room for both the well and the
        // closet; any of them is a valid annotation source.
        let sat = m.satellite_at_base_room(1).expect("square has satellites");
        assert!(sat.portals.iter().any(|p| p.outside == 1));
        assert!(m.satellite_at_base_room(2).is_none());
    }
}
