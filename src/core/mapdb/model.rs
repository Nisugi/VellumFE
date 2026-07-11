//! The mapdb Room model — full parse of Lich's room JSON.
//!
//! Shared by the layout engine (which reads the spatial fields, spec §2) and
//! the pathing engine (which reads `wayto`/`timeto`/`tags`). `wayto`/`dirto`/
//! `timeto` keys are parsed to numeric ids and stored in BTreeMaps so
//! iteration is ascending-numeric, matching JS object key order for
//! integer-like keys.

use std::collections::BTreeMap;

use serde_json::Value;

/// A `timeto` edge weight: seconds, or an embedded-Ruby StringProc source
/// (`";e …"`) that Lich evaluates at path time. VellumFE never executes
/// Ruby — procs are matched by the transpiler or the edge is unroutable.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeTo {
    Seconds(f64),
    Proc(String),
}

#[derive(Debug, Clone)]
pub struct Room {
    pub id: u32,
    /// Game uids — the stable identity across mapdb builds; `uid[0]` is used.
    pub uid: Vec<i64>,
    pub location: Option<String>,
    pub title: Vec<String>,
    pub description: Vec<String>,
    /// Movement commands keyed by destination room id. Commands starting
    /// with `";e "` are StringProc source (see `is_proc_command`).
    pub wayto: BTreeMap<u32, String>,
    /// Edge costs in seconds keyed by destination room id. A wayto edge
    /// without a timeto entry is NOT routable (Lich skips it in dijkstra).
    pub timeto: BTreeMap<u32, TimeTo>,
    /// Hand-curated direction overrides keyed by destination room id.
    pub dirto: BTreeMap<u32, String>,
    /// Search/routing tags ("bank", "furrier", …).
    pub tags: Vec<String>,
    /// `"Obvious exits: …"` (indoor) / `"Obvious paths: …"` (outdoor).
    /// Arrays in the JSON are joined with `,` (JS `String(array)` semantics).
    pub paths: String,
    pub climate: Option<String>,
    pub terrain: Option<String>,
    /// Hand-drawn overlay anchor: image filename + `[x1,y1,x2,y2]` pixel rect.
    pub image: Option<String>,
    pub image_coords: Option<[f64; 4]>,
}

/// A wayto command that is embedded Ruby, not a typeable game command.
pub fn is_proc_command(command: &str) -> bool {
    command.starts_with(";e ") || command == ";e"
}

impl Room {
    /// Virtual mapdb rooms that model the urchin-guide teleport network
    /// (one per town, titled "[Town - Urchin Hideout]", waytos to half the
    /// district). They are routing data, not places — never mapped, and
    /// excluded from v1 pathing (urchin guides cost silver and are off by
    /// default).
    pub fn is_urchin_hideout(&self) -> bool {
        self.title
            .iter()
            .any(|t| t.to_lowercase().contains("urchin hideout"))
    }

    pub fn from_json(value: &Value) -> Option<Room> {
        let obj = value.as_object()?;
        let id = obj.get("id")?.as_u64()? as u32;

        let uid = obj
            .get("uid")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default();

        Some(Room {
            id,
            uid,
            location: string_field(obj.get("location")),
            title: string_array(obj.get("title")),
            description: string_array(obj.get("description")),
            wayto: id_keyed_strings(obj.get("wayto")),
            timeto: id_keyed_timeto(obj.get("timeto")),
            dirto: id_keyed_strings(obj.get("dirto")),
            tags: string_array(obj.get("tags")),
            paths: paths_string(obj.get("paths")),
            climate: string_field(obj.get("climate")),
            terrain: string_field(obj.get("terrain")),
            image: string_field(obj.get("image")),
            image_coords: obj.get("image_coords").and_then(Value::as_array).and_then(
                |a| {
                    if a.len() == 4 {
                        let mut coords = [0.0; 4];
                        for (i, v) in a.iter().enumerate() {
                            coords[i] = v.as_f64()?;
                        }
                        Some(coords)
                    } else {
                        None
                    }
                },
            ),
        })
    }
}

fn string_field(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_owned)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
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

/// `{ "1234": 0.2, "5678": ";e …" }` → numeric-keyed edge costs. Lich
/// serializes StringProc costs as `";e …"` strings.
fn id_keyed_timeto(value: Option<&Value>) -> BTreeMap<u32, TimeTo> {
    let mut map = BTreeMap::new();
    if let Some(Value::Object(obj)) = value {
        for (key, val) in obj {
            let Ok(id) = key.parse::<u32>() else { continue };
            match val {
                Value::Number(n) => {
                    if let Some(seconds) = n.as_f64() {
                        map.insert(id, TimeTo::Seconds(seconds));
                    }
                }
                Value::String(s) => {
                    map.insert(id, TimeTo::Proc(s.clone()));
                }
                _ => {}
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

/// Room list plus an id → index lookup, the shape every layout pipeline
/// stage takes.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urchin_hideouts_are_recognized() {
        let hideout: Value = serde_json::json!({
            "id": 30708,
            "title": ["[Wehnimer's Landing - Urchin Hideout]"],
            "location": "Wehnimer's Landing",
            "wayto": {"222": "urchin guide premiumhall nonpremium"},
            "paths": ["Obvious exits: bwahaha"]
        });
        assert!(Room::from_json(&hideout).unwrap().is_urchin_hideout());

        let shop: Value = serde_json::json!({
            "id": 3672,
            "title": ["[First Elanith Bank, Teller]"],
            "location": "Wehnimer's Landing",
            "wayto": {"3669": "out"}
        });
        assert!(!Room::from_json(&shop).unwrap().is_urchin_hideout());
    }

    #[test]
    fn full_room_fields_parse() {
        let room: Value = serde_json::json!({
            "id": 3672,
            "uid": [7150105],
            "title": ["[First Elanith Bank, Teller]"],
            "description": ["Guarded by an alert-looking..."],
            "location": "Wehnimer's Landing",
            "tags": ["bank", "deposit"],
            "wayto": {"3669": "out", "9999": ";e fput 'go window'"},
            "timeto": {"3669": 0.2, "9999": ";e Map.estimate_time_to(1)"},
            "paths": "Obvious exits: out"
        });
        let room = Room::from_json(&room).unwrap();
        assert_eq!(room.tags, vec!["bank", "deposit"]);
        assert_eq!(room.description.len(), 1);
        assert_eq!(room.timeto[&3669], TimeTo::Seconds(0.2));
        assert!(matches!(room.timeto[&9999], TimeTo::Proc(_)));
        assert!(!is_proc_command(&room.wayto[&3669]));
        assert!(is_proc_command(&room.wayto[&9999]));
    }
}
