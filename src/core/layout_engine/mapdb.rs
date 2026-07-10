//! Mapdb room model for the layout engine.
//!
//! Rooms come from the Lich mapdb JSON array. Only the fields the layout
//! pipeline reads are kept (docs/layout-engine-spec.md §2). `wayto`/`dirto`
//! keys are parsed to numeric ids and stored in a BTreeMap so iteration is
//! ascending-numeric, matching JS object key order for integer-like keys.

use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Room {
    pub id: u32,
    /// Game uids — the stable identity across mapdb builds; `uid[0]` is used.
    pub uid: Vec<i64>,
    pub location: Option<String>,
    pub title: Vec<String>,
    /// Movement commands keyed by destination room id.
    pub wayto: BTreeMap<u32, String>,
    /// Hand-curated direction overrides keyed by destination room id.
    pub dirto: BTreeMap<u32, String>,
    /// `"Obvious exits: …"` (indoor) / `"Obvious paths: …"` (outdoor).
    /// Arrays in the JSON are joined with `,` (JS `String(array)` semantics).
    pub paths: String,
    pub climate: Option<String>,
    pub terrain: Option<String>,
    /// Hand-drawn overlay anchor: image filename + `[x1,y1,x2,y2]` pixel rect.
    pub image: Option<String>,
    pub image_coords: Option<[f64; 4]>,
}

impl Room {
    pub fn from_json(value: &Value) -> Option<Room> {
        let obj = value.as_object()?;
        let id = obj.get("id")?.as_u64()? as u32;

        let uid = obj
            .get("uid")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default();

        let title = obj
            .get("title")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        let image_coords = obj
            .get("image_coords")
            .and_then(Value::as_array)
            .and_then(|a| {
                if a.len() == 4 {
                    let mut coords = [0.0; 4];
                    for (i, v) in a.iter().enumerate() {
                        coords[i] = v.as_f64()?;
                    }
                    Some(coords)
                } else {
                    None
                }
            });

        Some(Room {
            id,
            uid,
            location: string_field(obj.get("location")),
            title,
            wayto: id_keyed_strings(obj.get("wayto")),
            dirto: id_keyed_strings(obj.get("dirto")),
            paths: paths_string(obj.get("paths")),
            climate: string_field(obj.get("climate")),
            terrain: string_field(obj.get("terrain")),
            image: string_field(obj.get("image")),
            image_coords,
        })
    }
}

fn string_field(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_owned)
}

/// `{ "1234": "north", ... }` → numeric-keyed map. Non-numeric keys and
/// non-string values are dropped (the reference lookup fails on them too).
fn id_keyed_strings(value: Option<&Value>) -> BTreeMap<u32, String> {
    let mut map = BTreeMap::new();
    if let Some(Value::Object(obj)) = value {
        for (key, val) in obj {
            if let (Ok(id), Some(s)) = (key.parse::<u32>(), val.as_str()) {
                map.insert(id, s.to_owned());
            }
        }
    }
    map
}

/// JS reads `String(room.paths)`: arrays stringify to comma-joined elements.
fn paths_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(","),
        _ => String::new(),
    }
}

/// Parse a mapdb JSON array (entries may be `null`), keeping rooms in the
/// selected location, sorted by ascending room id (the spec's canonical
/// iteration order).
pub fn rooms_for_location(db_json: &str, location: &str) -> serde_json::Result<Vec<Room>> {
    let db: Vec<Value> = serde_json::from_str(db_json)?;
    let mut rooms: Vec<Room> = db
        .iter()
        .filter_map(Room::from_json)
        .filter(|r| r.location.as_deref() == Some(location))
        .collect();
    rooms.sort_by_key(|r| r.id);
    Ok(rooms)
}

/// Parse a pre-filtered room array (e.g. a test fixture), sorted by id.
pub fn rooms_from_array(json: &str) -> serde_json::Result<Vec<Room>> {
    let entries: Vec<Value> = serde_json::from_str(json)?;
    let mut rooms: Vec<Room> = entries.iter().filter_map(Room::from_json).collect();
    rooms.sort_by_key(|r| r.id);
    Ok(rooms)
}

/// The whole mapdb, indexed for layout generation: rooms grouped by location
/// plus uid/id → location lookups (uids are the stable identity the game
/// stream reports; ids are Lich's build-local numbering).
pub struct MapDb {
    locations: std::collections::BTreeMap<String, Vec<Room>>,
    location_of_id: std::collections::HashMap<u32, String>,
    location_of_uid: std::collections::HashMap<i64, String>,
}

impl MapDb {
    /// Parse a full mapdb JSON array (Lich's `data/<GAME>/map-<ts>.json`).
    /// Rooms without a `location` cannot be laid out and are dropped.
    pub fn load(path: &std::path::Path) -> std::io::Result<MapDb> {
        let json = std::fs::read_to_string(path)?;
        let db: Vec<Value> = serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut locations: std::collections::BTreeMap<String, Vec<Room>> = Default::default();
        let mut location_of_id = std::collections::HashMap::new();
        let mut location_of_uid = std::collections::HashMap::new();
        for value in &db {
            let Some(room) = Room::from_json(value) else {
                continue;
            };
            let Some(location) = room.location.clone() else {
                continue;
            };
            location_of_id.insert(room.id, location.clone());
            for &uid in &room.uid {
                location_of_uid.insert(uid, location.clone());
            }
            locations.entry(location).or_default().push(room);
        }
        for rooms in locations.values_mut() {
            rooms.sort_by_key(|r| r.id);
        }
        Ok(MapDb {
            locations,
            location_of_id,
            location_of_uid,
        })
    }

    pub fn locations(&self) -> impl Iterator<Item = &str> {
        self.locations.keys().map(String::as_str)
    }

    /// Rooms of one location, in canonical ascending-id order.
    pub fn rooms(&self, location: &str) -> Option<&[Room]> {
        self.locations.get(location).map(Vec::as_slice)
    }

    pub fn location_of_uid(&self, uid: i64) -> Option<&str> {
        self.location_of_uid.get(&uid).map(String::as_str)
    }

    pub fn location_of_room_id(&self, id: u32) -> Option<&str> {
        self.location_of_id.get(&id).map(String::as_str)
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

/// Room list plus an id → index lookup, the shape every pipeline stage takes.
pub struct RoomTable<'a> {
    rooms: &'a [Room],
    by_id: std::collections::HashMap<u32, usize>,
}

impl<'a> RoomTable<'a> {
    pub fn new(rooms: &'a [Room]) -> Self {
        let by_id = rooms.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
        RoomTable { rooms, by_id }
    }

    pub fn get(&self, id: u32) -> Option<&'a Room> {
        self.by_id.get(&id).map(|&i| &self.rooms[i])
    }

    pub fn contains(&self, id: u32) -> bool {
        self.by_id.contains_key(&id)
    }

    pub fn rooms(&self) -> &'a [Room] {
        self.rooms
    }
}
