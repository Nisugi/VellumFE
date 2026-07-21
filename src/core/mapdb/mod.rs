//! The mapdb — canonical room database shared by the layout engine
//! (per-location room lists) and the pathing engine (the full wayto graph).
//!
//! Grew out of `layout_engine/mapdb.rs`; the layout-facing API is unchanged
//! (`rooms(location)`, uid/id → location lookups over *mappable* rooms),
//! while pathing sees every room through `room(id)` / `ids_of_uid` /
//! `room_ids_with_tag`, including location-less rooms and virtual
//! urchin-hideout routing nodes that the map never draws.

pub mod model;

use std::collections::{BTreeMap, HashMap};

pub use model::{
    is_proc_command, rooms_for_location, rooms_from_array, Room, RoomTable, TimeTo,
};

/// Rooms carrying this tag are player-shop warrens — hundreds of
/// near-identical rooms that dwarf their town on the map.
const PLAYERSHOP_TAG: &str = "meta:playershop";
/// Appended to the town's location to form the warren's own pseudo-location
/// ("Mist Harbor (Player Shops)"). It gets its own browsable layout with the
/// usual outdoor/interiors split, and the town map stays readable. Pathing
/// is untouched — the graph keeps every edge; only map grouping changes.
pub const PLAYERSHOP_LOCATION_SUFFIX: &str = " (Player Shops)";

/// Where a room lives inside `MapDb`.
#[derive(Debug, Clone)]
enum Slot {
    /// In a location's mappable room list.
    Placed { location: String, index: usize },
    /// Routable but not mappable: no location, or an urchin hideout.
    Unplaced { index: usize },
}

pub struct MapDb {
    /// Mappable rooms by location, ascending id — what the layout engine
    /// consumes.
    locations: BTreeMap<String, Vec<Room>>,
    /// Rooms the map never draws but the pathing graph still contains.
    unplaced: Vec<Room>,
    slots: HashMap<u32, Slot>,
    /// uid → ids of every room carrying it, in mapdb order (instanced areas
    /// share uids). The *last placed* id is the map-resolution answer,
    /// matching the pre-split behavior.
    ids_of_uid: HashMap<i64, Vec<u32>>,
    /// tag → mappable+unplaced room ids ("bank" → every bank teller).
    ids_of_tag: HashMap<String, Vec<u32>>,
    /// title → mappable room ids carrying it. Titles repeat heavily
    /// ("[A Dark Tunnel]"); consumed only by the uid-less current-room
    /// fallback, which must disambiguate before trusting a hit.
    ids_of_title: HashMap<String, Vec<u32>>,
}

impl MapDb {
    /// Parse a full mapdb JSON array (Lich's `data/<GAME>/map-<ts>.json`).
    pub fn load(path: &std::path::Path) -> std::io::Result<MapDb> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn from_json(json: &str) -> serde_json::Result<MapDb> {
        let db: Vec<serde_json::Value> = serde_json::from_str(json)?;

        let mut locations: BTreeMap<String, Vec<Room>> = Default::default();
        let mut unplaced: Vec<Room> = Vec::new();
        let mut slots = HashMap::new();
        let mut ids_of_uid: HashMap<i64, Vec<u32>> = HashMap::new();
        let mut ids_of_tag: HashMap<String, Vec<u32>> = HashMap::new();
        let mut ids_of_title: HashMap<String, Vec<u32>> = HashMap::new();

        for value in &db {
            let Some(mut room) = Room::from_json(value) else {
                continue;
            };
            // Player-shop warrens split into their own pseudo-location so
            // the town layout isn't dominated by them. Un-located tagged
            // rooms stay unplaced as usual.
            if room.location.is_some() && room.tags.iter().any(|t| t == PLAYERSHOP_TAG) {
                let town = room.location.take().expect("checked above");
                room.location = Some(format!("{town}{PLAYERSHOP_LOCATION_SUFFIX}"));
            }
            for &uid in &room.uid {
                let ids = ids_of_uid.entry(uid).or_default();
                if !ids.contains(&room.id) {
                    ids.push(room.id);
                }
            }
            for tag in &room.tags {
                ids_of_tag.entry(tag.clone()).or_default().push(room.id);
            }
            // Urchin hideouts are teleport routing nodes, not places; rooms
            // without a location can't be laid out. Both stay routable.
            let mappable = !room.is_urchin_hideout() && room.location.is_some();
            if mappable {
                let location = room.location.clone().expect("checked above");
                let id = room.id;
                for title in &room.title {
                    let ids = ids_of_title.entry(title.clone()).or_default();
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                let rooms = locations.entry(location.clone()).or_default();
                rooms.push(room);
                slots.insert(
                    id,
                    Slot::Placed {
                        location,
                        index: rooms.len() - 1,
                    },
                );
            } else {
                slots.insert(
                    room.id,
                    Slot::Unplaced {
                        index: unplaced.len(),
                    },
                );
                unplaced.push(room);
            }
        }
        // Canonical ascending-id order per location — then reindex the slots
        // the sort just invalidated.
        for rooms in locations.values_mut() {
            rooms.sort_by_key(|r| r.id);
            for (index, room) in rooms.iter().enumerate() {
                if let Some(Slot::Placed { index: slot_index, .. }) = slots.get_mut(&room.id) {
                    *slot_index = index;
                }
            }
        }
        Ok(MapDb {
            locations,
            unplaced,
            slots,
            ids_of_uid,
            ids_of_tag,
            ids_of_title,
        })
    }

    /// Any room by id — placed or not. The pathing graph's node lookup.
    pub fn room(&self, id: u32) -> Option<&Room> {
        match self.slots.get(&id)? {
            Slot::Placed { location, index } => self.locations.get(location)?.get(*index),
            Slot::Unplaced { index } => self.unplaced.get(*index),
        }
    }

    pub fn room_count(&self) -> usize {
        self.slots.len()
    }

    pub fn locations(&self) -> impl Iterator<Item = &str> {
        self.locations.keys().map(String::as_str)
    }

    /// Mappable rooms of one location, in canonical ascending-id order.
    pub fn rooms(&self, location: &str) -> Option<&[Room]> {
        self.locations.get(location).map(Vec::as_slice)
    }

    /// Every room id carrying this game uid, in mapdb order.
    pub fn ids_of_uid(&self, uid: i64) -> &[u32] {
        self.ids_of_uid.get(&uid).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Ids of every room tagged `tag` (`.go2 bank` targets).
    pub fn room_ids_with_tag(&self, tag: &str) -> &[u32] {
        self.ids_of_tag.get(tag).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Ids of every *mappable* room whose title list contains `title`
    /// verbatim. The uid-less current-room fallback's candidate pool.
    pub fn room_ids_with_title(&self, title: &str) -> &[u32] {
        self.ids_of_title.get(title).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The *mappable* Lich room id carrying this game uid (`<nav rm='…'/>`
    /// reports uids; layouts speak room ids). Last placed room wins,
    /// matching the pre-split lookup's insert order.
    pub fn room_id_of_uid(&self, uid: i64) -> Option<u32> {
        self.ids_of_uid(uid)
            .iter()
            .rev()
            .copied()
            .find(|id| matches!(self.slots.get(id), Some(Slot::Placed { .. })))
    }

    pub fn location_of_room_id(&self, id: u32) -> Option<&str> {
        match self.slots.get(&id)? {
            Slot::Placed { location, .. } => Some(location.as_str()),
            Slot::Unplaced { .. } => None,
        }
    }

    pub fn location_of_uid(&self, uid: i64) -> Option<&str> {
        self.location_of_room_id(self.room_id_of_uid(uid)?)
    }
}

/// Newest `map-<timestamp>.json` in Lich's per-game data directory
/// (`<lich>/data/GSIV` for prime, `GST` for test).
pub fn find_latest_mapdb(game_data_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut best: Option<(u64, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(game_data_dir).ok()? {
        let path = entry.ok()?.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_owned(),
            None => continue,
        };
        let Some(ts) = name
            .strip_prefix("map-")
            .and_then(|rest| rest.strip_suffix(".json"))
            .and_then(|ts| ts.parse::<u64>().ok())
        else {
            continue;
        };
        if best.as_ref().map(|(t, _)| ts > *t).unwrap_or(true) {
            best = Some((ts, path));
        }
    }
    best.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
        {"id": 369, "uid": [731009], "location": "Mist Harbor",
         "title": ["[East Row, Fel Road]"], "tags": ["bank"],
         "wayto": {"370": "north"}, "timeto": {"370": 0.2},
         "paths": "Obvious paths: north"},
        {"id": 370, "uid": [731010], "location": "Mist Harbor",
         "title": ["[East Row, North]"],
         "wayto": {"369": "south"}, "timeto": {"369": 0.2},
         "paths": "Obvious paths: south"},
        {"id": 30708, "uid": [900001], "location": "Wehnimer's Landing",
         "title": ["[Wehnimer's Landing - Urchin Hideout]"],
         "wayto": {"369": "urchin guide east row"}, "timeto": {},
         "paths": "Obvious exits: bwahaha"},
        {"id": 50000, "uid": [731009],
         "title": ["[An Instanced Copy]"], "wayto": {}, "paths": ""}
    ]"#;

    #[test]
    fn placed_and_unplaced_rooms_split_but_all_stay_reachable() {
        let db = MapDb::from_json(SAMPLE).unwrap();
        assert_eq!(db.room_count(), 4);
        // Layout view: only mappable rooms, per location.
        assert_eq!(db.rooms("Mist Harbor").unwrap().len(), 2);
        assert!(db.rooms("Wehnimer's Landing").is_none(), "urchin hideout never maps");
        // Pathing view: everything resolves by id, including the hideout and
        // the location-less instance.
        assert!(db.room(30708).is_some());
        assert!(db.room(50000).is_some());
        assert_eq!(db.location_of_room_id(30708), None);
    }

    #[test]
    fn uid_lookups_prefer_placed_rooms_and_expose_all_ids() {
        let db = MapDb::from_json(SAMPLE).unwrap();
        // 731009 is carried by placed 369 and unplaced 50000: the map
        // resolution answer is the placed room.
        assert_eq!(db.room_id_of_uid(731009), Some(369));
        assert_eq!(db.location_of_uid(731009), Some("Mist Harbor"));
        assert_eq!(db.ids_of_uid(731009), &[369, 50000]);
        assert_eq!(db.room_ids_with_tag("bank"), &[369]);
        assert_eq!(db.room_ids_with_tag("nope"), &[] as &[u32]);
    }

    #[test]
    fn playershop_rooms_split_into_their_own_pseudo_location() {
        let json = r#"[
            {"id": 1, "uid": [100], "location": "Mist Harbor",
             "title": ["[East Row, Fel Road]"],
             "wayto": {"2": "go shop"}, "timeto": {"2": 0.2},
             "paths": "Obvious paths: none"},
            {"id": 2, "uid": [200], "location": "Mist Harbor",
             "title": ["[Sivalis' General Store]"], "tags": ["meta:playershop"],
             "wayto": {"1": "out", "3": "north"}, "timeto": {"1": 0.2, "3": 0.2},
             "paths": "Obvious exits: out, north"},
            {"id": 3, "uid": [300], "location": "Mist Harbor",
             "title": ["[Ryain's General Store]"], "tags": ["meta:playershop"],
             "wayto": {"2": "south"}, "timeto": {"2": 0.2},
             "paths": "Obvious exits: south"},
            {"id": 4, "uid": [400],
             "title": ["[A Locationless Shop]"], "tags": ["meta:playershop"],
             "wayto": {}, "paths": ""}
        ]"#;
        let db = MapDb::from_json(json).unwrap();
        // The warren is its own location; the town keeps only untagged rooms.
        assert_eq!(db.rooms("Mist Harbor").unwrap().len(), 1);
        assert_eq!(
            db.rooms("Mist Harbor (Player Shops)").unwrap().len(),
            2,
            "tagged rooms move to the pseudo-location"
        );
        assert_eq!(
            db.location_of_room_id(2),
            Some("Mist Harbor (Player Shops)")
        );
        // Pathing still sees every room and edge.
        assert!(db.room(2).unwrap().wayto.contains_key(&1));
        assert_eq!(db.room_ids_with_tag("meta:playershop"), &[2, 3, 4]);
        // Un-located tagged rooms stay unplaced, not invented into a location.
        assert_eq!(db.location_of_room_id(4), None);
    }
}
