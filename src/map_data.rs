use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapCoordinate {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapConnector {
    pub x: i32,
    pub y: i32,
    #[serde(rename = "type")]
    pub connector_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapContext {
    pub display_name: String,
    pub description: String,
    pub rooms: HashMap<String, MapCoordinate>,
    #[serde(default)]
    pub connectors: Vec<MapConnector>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapData {
    pub contexts: HashMap<String, MapContext>,
}

/// Simplified mapdb room structure (only the fields we need)
#[derive(Debug, Clone, Deserialize)]
pub struct MapDbRoom {
    pub id: i32,  // Room IDs can be negative
    #[serde(default)]
    pub uid: Vec<i32>,
    #[serde(default)]
    pub wayto: HashMap<String, String>,
}

/// Full mapdb data
pub struct MapDb {
    rooms: HashMap<String, MapDbRoom>, // Keyed by UID as string
    id_to_uid: HashMap<i32, i32>,      // Map room ID to UID (both IDs and UIDs can be negative for instanced/special areas)
}

impl MapData {
    /// Load map data from embedded defaults
    pub fn load_default() -> Result<Self, serde_json::Error> {
        let data = include_str!("../defaults/map_coordinates.json");
        serde_json::from_str(data)
    }

    /// Get the context ID for a given room ID
    pub fn get_context_for_room(&self, room_id: &str) -> Option<String> {
        for (context_id, context) in &self.contexts {
            if context.rooms.contains_key(room_id) {
                return Some(context_id.clone());
            }
        }
        None
    }

    /// Get coordinate for a specific room
    pub fn get_coordinate(&self, room_id: &str) -> Option<&MapCoordinate> {
        for context in self.contexts.values() {
            if let Some(coord) = context.rooms.get(room_id) {
                return Some(coord);
            }
        }
        None
    }

    /// Get all rooms in a specific context
    pub fn get_context(&self, context_id: &str) -> Option<&MapContext> {
        self.contexts.get(context_id)
    }
}

impl MapDb {
    /// Load mapdb from file (runtime, not embedded due to 38MB size)
    pub fn load_default() -> Result<Self, Box<dyn std::error::Error>> {
        // Try to load from working directory first, then from defaults/
        let paths = vec![
            std::path::PathBuf::from("mapdb.json"),
            std::path::PathBuf::from("defaults/mapdb.json"),
        ];

        let data = paths
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "mapdb.json not found in working directory or defaults/"))?;

        let rooms_vec: Vec<MapDbRoom> = serde_json::from_str(&data)?;

        // Index by UID and build ID-to-UID mapping
        let mut rooms = HashMap::new();
        let mut id_to_uid = HashMap::new();

        let total_rooms = rooms_vec.len();
        let mut skipped_ids = Vec::new();

        for room in rooms_vec {
            if let Some(&uid) = room.uid.first() {
                id_to_uid.insert(room.id, uid);
                rooms.insert(uid.to_string(), room);
            } else {
                skipped_ids.push(room.id);
            }
        }

        tracing::info!("Loaded mapdb with {} rooms ({} with UIDs, {} without)", total_rooms, rooms.len(), skipped_ids.len());
        if skipped_ids.len() < 50 {
            tracing::debug!("Rooms without UIDs: {:?}", skipped_ids);
        } else {
            tracing::debug!("First 10 rooms without UIDs: {:?}", &skipped_ids[..10]);
        }

        Ok(MapDb { rooms, id_to_uid })
    }

    /// Get room by UID
    pub fn get_room(&self, uid: &str) -> Option<&MapDbRoom> {
        self.rooms.get(uid)
    }

    /// Convert room ID to UID
    pub fn get_uid_for_id(&self, id_str: &str) -> Option<i32> {
        let id = id_str.parse::<i32>().ok()?;
        self.id_to_uid.get(&id).copied()
    }

    /// Check if a room has exits to rooms outside the given context
    /// Returns Vec of (direction, destination_uid) for portal exits
    pub fn get_portal_exits(&self, room_uid: &str, context_rooms: &HashMap<String, MapCoordinate>) -> Vec<(String, String)> {
        let mut portals = Vec::new();

        if let Some(room) = self.get_room(room_uid) {
            for (dest_uid, direction) in &room.wayto {
                // Check if destination is NOT in the current context
                if !context_rooms.contains_key(dest_uid) {
                    // Filter to only cardinal/ordinal directions (not scripts)
                    let dir_lower = direction.to_lowercase();
                    if matches!(dir_lower.as_str(),
                        "north" | "south" | "east" | "west" |
                        "northeast" | "northwest" | "southeast" | "southwest" |
                        "up" | "down" | "out" | "n" | "s" | "e" | "w" |
                        "ne" | "nw" | "se" | "sw" | "u" | "d")
                    {
                        portals.push((direction.clone(), dest_uid.clone()));
                    } else if direction.starts_with("go ") || direction.starts_with("climb ") {
                        // Also include "go door", "climb stairs", etc.
                        portals.push((direction.clone(), dest_uid.clone()));
                    }
                }
            }
        }

        portals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_default() {
        let map_data = MapData::load_default().expect("Failed to load map data");
        assert!(map_data.contexts.contains_key("wehnimers_town_square"));
        assert!(map_data.contexts.contains_key("wehnimers_moot_hall"));
    }

    #[test]
    fn test_get_context_for_room() {
        let map_data = MapData::load_default().unwrap();

        // Room 228 should be in Town Square context
        assert_eq!(
            map_data.get_context_for_room("228"),
            Some("wehnimers_town_square".to_string())
        );

        // Room 387 should be in Moot Hall context
        assert_eq!(
            map_data.get_context_for_room("387"),
            Some("wehnimers_moot_hall".to_string())
        );
    }

    #[test]
    fn test_get_coordinate() {
        let map_data = MapData::load_default().unwrap();

        // Room 228 is at origin (0, 0, 0)
        let coord = map_data.get_coordinate("228").expect("Room 228 not found");
        assert_eq!(coord.x, 0);
        assert_eq!(coord.y, 0);
        assert_eq!(coord.z, 0);
    }
}
