use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;

#[derive(Debug, Clone, Deserialize)]
struct MapDbRoom {
    id: i32,
    #[serde(default)]
    uid: Vec<i32>,
    #[serde(default)]
    title: Vec<String>,
    #[serde(default)]
    wayto: HashMap<String, String>,
    #[serde(default)]
    paths: Vec<String>,
}

fn is_outdoor_room(room: &MapDbRoom) -> bool {
    // Check if paths contains "Obvious paths" (outdoor) vs "Obvious exits" (indoor)
    room.paths.iter().any(|p| p.to_lowercase().contains("obvious paths"))
}

fn is_indoor_room(room: &MapDbRoom) -> bool {
    // Check if paths contains "Obvious exits" (indoor)
    room.paths.iter().any(|p| p.to_lowercase().contains("obvious exits"))
}

#[derive(Debug, Clone, Serialize)]
struct Coordinate {
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Debug, Clone, Serialize)]
struct MapContext {
    display_name: String,
    description: String,
    rooms: HashMap<String, Coordinate>,
}

#[derive(Debug, Clone, Serialize)]
struct MapData {
    contexts: HashMap<String, MapContext>,
}

fn direction_to_delta(dir: &str) -> Option<(i32, i32, i32)> {
    match dir.to_lowercase().as_str() {
        "north" | "n" => Some((0, -1, 0)),
        "south" | "s" => Some((0, 1, 0)),
        "east" | "e" => Some((1, 0, 0)),
        "west" | "w" => Some((-1, 0, 0)),
        "northeast" | "ne" => Some((1, -1, 0)),
        "northwest" | "nw" => Some((-1, -1, 0)),
        "southeast" | "se" => Some((1, 1, 0)),
        "southwest" | "sw" => Some((-1, 1, 0)),
        "up" | "u" => Some((0, 0, 1)),
        "down" | "d" => Some((0, 0, -1)),
        _ => None,
    }
}

fn reverse_direction(dir: &str) -> Option<String> {
    match dir.to_lowercase().as_str() {
        "north" | "n" => Some("south".to_string()),
        "south" | "s" => Some("north".to_string()),
        "east" | "e" => Some("west".to_string()),
        "west" | "w" => Some("east".to_string()),
        "northeast" | "ne" => Some("southwest".to_string()),
        "northwest" | "nw" => Some("southeast".to_string()),
        "southeast" | "se" => Some("northwest".to_string()),
        "southwest" | "sw" => Some("northeast".to_string()),
        "up" | "u" => Some("down".to_string()),
        "down" | "d" => Some("up".to_string()),
        _ => None,
    }
}

fn generate_coordinates_by_context(
    mapdb: &[MapDbRoom],
    start_uid: i32,
    max_depth: usize,
) -> HashMap<String, HashMap<String, Coordinate>> {
    // Returns: context_name -> (room_uid -> coordinate)
    // Index rooms by UID for fast lookup
    let mut uid_to_room: HashMap<i32, &MapDbRoom> = HashMap::new();
    let mut id_to_uid: HashMap<i32, i32> = HashMap::new();

    for room in mapdb {
        if let Some(&uid) = room.uid.first() {
            uid_to_room.insert(uid, room);
            id_to_uid.insert(room.id, uid);
        }
    }

    // Context storage: context_name -> (room_uid -> coordinate)
    let mut contexts: HashMap<String, HashMap<String, Coordinate>> = HashMap::new();

    // Track which context each room belongs to
    let mut room_to_context: HashMap<i32, String> = HashMap::new();

    // Global visited set (across all contexts)
    let mut visited: HashSet<i32> = HashSet::new();

    // Queue: (uid, coordinate, depth, context_name)
    let mut queue: VecDeque<(i32, Coordinate, usize, String)> = VecDeque::new();

    // Building context counter
    let mut building_counter = 1;

    // Determine starting context
    let start_room = uid_to_room.get(&start_uid).expect("Start UID not found");
    let start_context = if is_outdoor_room(start_room) {
        "outdoor".to_string()
    } else {
        format!("building_{}", building_counter)
    };

    // Initialize start room
    contexts.insert(start_context.clone(), HashMap::new());
    contexts.get_mut(&start_context).unwrap().insert(
        start_uid.to_string(),
        Coordinate { x: 0, y: 0, z: 0 }
    );
    room_to_context.insert(start_uid, start_context.clone());
    queue.push_back((start_uid, Coordinate { x: 0, y: 0, z: 0 }, 0, start_context));
    visited.insert(start_uid);

    while let Some((current_uid, current_coord, depth, current_context)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        let Some(current_room) = uid_to_room.get(&current_uid) else {
            continue;
        };

        let current_is_outdoor = is_outdoor_room(current_room);

        // Process all exits
        for (dest_id_str, direction) in &current_room.wayto {
            // Parse destination ID
            let Ok(dest_id) = dest_id_str.parse::<i32>() else {
                continue;
            };

            // Get destination UID
            let Some(&dest_uid) = id_to_uid.get(&dest_id) else {
                continue;
            };

            // Skip if already visited
            if visited.contains(&dest_uid) {
                continue;
            }

            // Get destination room
            let Some(dest_room) = uid_to_room.get(&dest_uid) else {
                continue;
            };

            let dest_is_outdoor = is_outdoor_room(dest_room);

            // Calculate new coordinate based on direction
            let Some((dx, dy, dz)) = direction_to_delta(direction) else {
                // Skip non-cardinal directions (go door, climb stairs, etc.)
                continue;
            };

            // VERIFY BIDIRECTIONAL CONSISTENCY
            let current_id_str = current_room.id.to_string();
            if let Some(reverse_dir) = dest_room.wayto.get(&current_id_str) {
                if let Some(expected_reverse) = reverse_direction(direction) {
                    if reverse_dir != &expected_reverse {
                        eprintln!(
                            "Warning: Bidirectional mismatch! UID {} '{}' to UID {} (ID {}), but return is '{}' (expected '{}'). Skipping.",
                            current_uid, direction, dest_uid, dest_id, reverse_dir, expected_reverse
                        );
                        continue;
                    }
                }
            }

            // DETERMINE DESTINATION CONTEXT
            let dest_context = if current_is_outdoor && !dest_is_outdoor {
                // Outdoor → Indoor: Create new building context
                building_counter += 1;
                let building_name = if let Some(title) = dest_room.title.first() {
                    // Try to extract building name from title
                    let clean_title = title.trim_matches(|c| c == '[' || c == ']');
                    format!("building_{}_{}", building_counter, clean_title.replace(", ", "_").replace(" ", "_").to_lowercase())
                } else {
                    format!("building_{}", building_counter)
                };

                // Ensure context exists
                if !contexts.contains_key(&building_name) {
                    contexts.insert(building_name.clone(), HashMap::new());
                }

                println!("  → New building context: {}", building_name);
                building_name
            } else if !current_is_outdoor && dest_is_outdoor {
                // Indoor → Outdoor: Back to outdoor context
                "outdoor".to_string()
            } else {
                // Same type: stay in current context
                current_context.clone()
            };

            // Get or create coordinates for this context
            let context_coords = contexts.get_mut(&dest_context).unwrap();

            // Calculate coordinate (relative to current context if same, origin if new context)
            let new_coord = if dest_context == current_context {
                // Same context: offset from current position
                Coordinate {
                    x: current_coord.x + dx,
                    y: current_coord.y + dy,
                    z: current_coord.z + dz,
                }
            } else {
                // New context: start at origin
                Coordinate { x: 0, y: 0, z: 0 }
            };

            // Check for coordinate conflicts within this context
            let conflict = context_coords.values().any(|c| {
                c.x == new_coord.x && c.y == new_coord.y && c.z == new_coord.z
            });

            if conflict {
                eprintln!(
                    "Warning: Coordinate conflict at ({}, {}, {}) for room UID {} in context {}",
                    new_coord.x, new_coord.y, new_coord.z, dest_uid, dest_context
                );
                // Try to place nearby
                let mut resolved = false;
                for attempt in 0..8 {
                    let adjusted = Coordinate {
                        x: new_coord.x + (attempt % 3 - 1),
                        y: new_coord.y + (attempt / 3 - 1),
                        z: new_coord.z,
                    };

                    let adjusted_conflict = context_coords.values().any(|c| {
                        c.x == adjusted.x && c.y == adjusted.y && c.z == adjusted.z
                    });

                    if !adjusted_conflict {
                        let (adj_x, adj_y, adj_z) = (adjusted.x, adjusted.y, adjusted.z);
                        context_coords.insert(dest_uid.to_string(), adjusted.clone());
                        room_to_context.insert(dest_uid, dest_context.clone());
                        queue.push_back((dest_uid, adjusted, depth + 1, dest_context.clone()));
                        visited.insert(dest_uid);
                        resolved = true;
                        eprintln!("  Resolved: placed at ({}, {}, {})", adj_x, adj_y, adj_z);
                        break;
                    }
                }

                if !resolved {
                    eprintln!("  Could not resolve conflict, skipping room");
                }
                continue;
            }

            let (coord_x, coord_y, coord_z) = (new_coord.x, new_coord.y, new_coord.z);
            context_coords.insert(dest_uid.to_string(), new_coord.clone());
            room_to_context.insert(dest_uid, dest_context.clone());
            queue.push_back((dest_uid, new_coord, depth + 1, dest_context.clone()));
            visited.insert(dest_uid);

            // Print progress
            if let Some(title) = dest_room.title.first() {
                println!(
                    "Mapped: UID {} @ ({:3}, {:3}, {:2}) [{}] - {}",
                    dest_uid, coord_x, coord_y, coord_z, dest_context, title
                );
            }
        }
    }

    contexts
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 5 {
        eprintln!("Usage: map_generator <mapdb.json> <start_uid> <max_depth> <context_name> [output.json]");
        eprintln!("Example: map_generator mapdb.json 7120 10 \"wehnimers_landing\" map_coords.json");
        std::process::exit(1);
    }

    let mapdb_path = &args[1];
    let start_uid: i32 = args[2].parse().expect("Invalid start UID");
    let max_depth: usize = args[3].parse().expect("Invalid max depth");
    let context_name = &args[4];
    let output_path = args.get(5).map(|s| s.as_str()).unwrap_or("map_coordinates.json");

    println!("Loading mapdb from: {}", mapdb_path);
    let mapdb_content = fs::read_to_string(mapdb_path).expect("Failed to read mapdb");
    let mapdb: Vec<MapDbRoom> = serde_json::from_str(&mapdb_content).expect("Failed to parse mapdb");
    println!("Loaded {} rooms", mapdb.len());

    println!("\nGenerating coordinates starting from UID {} with max depth {}...", start_uid, max_depth);
    let context_map = generate_coordinates_by_context(&mapdb, start_uid, max_depth);

    let total_rooms: usize = context_map.values().map(|rooms| rooms.len()).sum();
    println!("\nGenerated {} room coordinates across {} contexts", total_rooms, context_map.len());

    // Create map data structure with proper context names
    let mut contexts = HashMap::new();
    for (context_id, rooms) in context_map {
        let display_name = if context_id == "outdoor" {
            format!("{} - Outdoor", context_name.replace('_', " "))
        } else {
            // Extract building name from context_id
            context_id.replace('_', " ").to_string()
        };

        contexts.insert(
            context_id.clone(),
            MapContext {
                display_name,
                description: format!("Auto-generated from UID {}", start_uid),
                rooms,
            },
        );
    }

    let map_data = MapData { contexts };

    // Write output
    let json = serde_json::to_string_pretty(&map_data).expect("Failed to serialize");
    fs::write(output_path, json).expect("Failed to write output file");
    println!("\nWrote map coordinates to: {}", output_path);
}
