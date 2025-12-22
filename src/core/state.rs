//! Game state management
//!
//! Tracks the current state of the game session: connection status,
//! character info, room state, inventory, etc.

use std::collections::HashMap;

/// How often to recalculate lag estimate (in seconds of game time)
const LAG_CHECK_INTERVAL_SECS: i64 = 30;

/// Game session state
#[derive(Clone, Debug)]
pub struct GameState {
    /// Connection status
    pub connected: bool,

    /// Character name
    pub character_name: Option<String>,

    /// Current room ID
    pub room_id: Option<String>,

    /// Current room name
    pub room_name: Option<String>,

    /// Available exits from current room
    pub exits: Vec<String>,

    /// Game server time from last prompt (Unix timestamp)
    /// This is the authoritative time source for roundtime/casttime comparisons
    pub game_time: i64,

    /// Roundtime end timestamp (Unix time from game server)
    pub roundtime_end: Option<i64>,

    /// Casttime end timestamp (Unix time from game server)
    pub casttime_end: Option<i64>,

    /// Current spell being prepared
    pub spell: Option<String>,

    /// Active game streams (tags like "inv", "assess", etc.)
    pub active_streams: HashMap<String, bool>,

    /// Player status indicators
    pub status: StatusInfo,

    /// Vitals (health, mana, etc.)
    pub vitals: Vitals,

    /// Inventory items
    pub inventory: Vec<String>,

    /// Current left hand item
    pub left_hand: Option<String>,

    /// Current right hand item
    pub right_hand: Option<String>,

    /// Active effects/buffs
    pub active_effects: Vec<String>,

    /// Compass directions
    pub compass_dirs: Vec<String>,

    /// Last prompt text (for command echoes)
    pub last_prompt: String,

    /// Target list from dDBTarget dropdown (for direct-connect users)
    pub target_list: TargetListState,

    /// Container cache for bag/container contents
    pub container_cache: ContainerCache,

    /// Estimated lag between system time and game server time (in milliseconds)
    /// Positive = system clock ahead of game, Negative = game ahead of system
    /// Recalculated periodically (every LAG_CHECK_INTERVAL_SECS)
    pub estimated_lag_ms: Option<i64>,

    /// Game time when we last calculated lag (for throttling)
    last_lag_check_time: i64,
}

/// Player status information
#[derive(Clone, Debug, Default)]
pub struct StatusInfo {
    pub standing: bool,
    pub kneeling: bool,
    pub sitting: bool,
    pub prone: bool,
    pub stunned: bool,
    pub bleeding: bool,
    pub hidden: bool,
    pub invisible: bool,
    pub webbed: bool,
    pub joined: bool,
    pub dead: bool,
}

/// Player vitals
#[derive(Clone, Debug)]
pub struct Vitals {
    pub health: u8,
    pub mana: u8,
    pub stamina: u8,
    pub spirit: u8,
}

/// Target list state from dDBTarget dropdown (for direct-connect users)
#[derive(Clone, Debug, Default)]
pub struct TargetListState {
    /// Currently selected target name
    pub current_target: String,
    /// List of creatures in the room
    pub creatures: Vec<Creature>,
}

/// A creature in the target list
#[derive(Clone, Debug)]
pub struct Creature {
    /// Creature display name
    pub name: String,
    /// Creature ID (e.g., "#146101714")
    pub id: String,
}

/// Container cache for inventory containers (bags, backpacks, etc.)
#[derive(Clone, Debug, Default)]
pub struct ContainerCache {
    /// Map of container ID to container data
    pub containers: HashMap<String, ContainerData>,
}

/// Data for a single container
#[derive(Clone, Debug)]
pub struct ContainerData {
    /// Container ID
    pub id: String,
    /// Container title (e.g., "Bandolier")
    pub title: String,
    /// Items in the container (raw content lines with links preserved)
    pub items: Vec<String>,
    /// Generation counter for change detection
    pub generation: u64,
}

impl TargetListState {
    /// Update the target list with new data from dDBTarget dropdown.
    /// Compares new list with existing, removing creatures that disappeared
    /// and adding new ones.
    pub fn update(&mut self, current_target: String, names: Vec<String>, ids: Vec<String>) {
        self.current_target = current_target;

        // Build new creature list from paired names/ids
        let mut new_creatures = Vec::new();
        for (name, id) in names.into_iter().zip(ids.into_iter()) {
            // Skip the "none" placeholder entry
            if name.to_lowercase() == "none" || id.to_lowercase().contains("help") {
                continue;
            }
            new_creatures.push(Creature { name, id });
        }

        self.creatures = new_creatures;
    }

    /// Clear the target list
    pub fn clear(&mut self) {
        self.current_target.clear();
        self.creatures.clear();
    }
}

impl ContainerCache {
    /// Register a new container or update its metadata
    pub fn register_container(&mut self, id: String, title: String) {
        let entry = self.containers.entry(id.clone()).or_insert_with(|| ContainerData {
            id,
            title: title.clone(),
            items: Vec::new(),
            generation: 0,
        });
        // Update title if it changed
        if entry.title != title {
            entry.title = title;
            entry.generation += 1;
        }
    }

    /// Clear all items in a container (called on clearContainer tag)
    pub fn clear_container(&mut self, id: &str) {
        if let Some(container) = self.containers.get_mut(id) {
            container.items.clear();
            container.generation += 1;
        }
    }

    /// Add an item to a container
    pub fn add_item(&mut self, container_id: &str, content: String) {
        if let Some(container) = self.containers.get_mut(container_id) {
            container.items.push(content);
            container.generation += 1;
        } else {
            // Container not registered yet - create it with unknown title
            let container = ContainerData {
                id: container_id.to_string(),
                title: String::new(),
                items: vec![content],
                generation: 1,
            };
            self.containers.insert(container_id.to_string(), container);
        }
    }

    /// Get a container by ID
    pub fn get(&self, id: &str) -> Option<&ContainerData> {
        self.containers.get(id)
    }

    /// Get all known container IDs
    pub fn container_ids(&self) -> Vec<String> {
        self.containers.keys().cloned().collect()
    }

    /// Find a container by its title (case-insensitive partial match)
    /// Returns the first matching container's data
    pub fn find_by_title(&self, title: &str) -> Option<&ContainerData> {
        let title_lower = title.to_lowercase();
        // First try exact match (case-insensitive)
        for container in self.containers.values() {
            if container.title.to_lowercase() == title_lower {
                return Some(container);
            }
        }
        // Then try partial match
        for container in self.containers.values() {
            if container.title.to_lowercase().contains(&title_lower) {
                return Some(container);
            }
        }
        None
    }

    /// Get all known containers sorted by title
    pub fn list_containers(&self) -> Vec<&ContainerData> {
        let mut containers: Vec<_> = self.containers.values().collect();
        containers.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        containers
    }
}

impl GameState {
    pub fn new() -> Self {
        Self {
            connected: false,
            character_name: None,
            room_id: None,
            room_name: None,
            exits: Vec::new(),
            game_time: 0,
            roundtime_end: None,
            casttime_end: None,
            spell: None,
            active_streams: HashMap::new(),
            status: StatusInfo::default(),
            vitals: Vitals::default(),
            inventory: Vec::new(),
            left_hand: None,
            right_hand: None,
            active_effects: Vec::new(),
            compass_dirs: Vec::new(),
            last_prompt: String::from(">"), // Default prompt
            target_list: TargetListState::default(),
            container_cache: ContainerCache::default(),
            estimated_lag_ms: None,
            last_lag_check_time: 0,
        }
    }

    /// Update game time from prompt timestamp.
    /// Also periodically recalculates estimated lag (every 30 seconds of game time).
    pub fn update_game_time(&mut self, prompt_time: i64) {
        self.game_time = prompt_time;

        // Periodically calculate lag (every LAG_CHECK_INTERVAL_SECS)
        if prompt_time - self.last_lag_check_time >= LAG_CHECK_INTERVAL_SECS {
            let system_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;

            // Convert game time to milliseconds for comparison
            let game_time_ms = prompt_time * 1000;

            // Positive lag = system ahead, Negative = game ahead
            self.estimated_lag_ms = Some(system_time - game_time_ms);
            self.last_lag_check_time = prompt_time;
        }
    }

    /// Check if currently in roundtime.
    /// Compares against game server time, not system time.
    pub fn in_roundtime(&self) -> bool {
        if let Some(end_time) = self.roundtime_end {
            self.game_time < end_time
        } else {
            false
        }
    }

    /// Check if currently in casttime.
    /// Compares against game server time, not system time.
    pub fn in_casttime(&self) -> bool {
        if let Some(end_time) = self.casttime_end {
            self.game_time < end_time
        } else {
            false
        }
    }

    /// Get remaining roundtime in seconds (0 if not in roundtime)
    pub fn roundtime_remaining(&self) -> i64 {
        if let Some(end_time) = self.roundtime_end {
            (end_time - self.game_time).max(0)
        } else {
            0
        }
    }

    /// Get remaining casttime in seconds (0 if not in casttime)
    pub fn casttime_remaining(&self) -> i64 {
        if let Some(end_time) = self.casttime_end {
            (end_time - self.game_time).max(0)
        } else {
            0
        }
    }

    /// Get estimated lag in milliseconds, if available
    pub fn lag_ms(&self) -> Option<i64> {
        self.estimated_lag_ms
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            health: 100,
            mana: 100,
            stamina: 100,
            spirit: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== GameState tests ==========

    #[test]
    fn test_game_state_new() {
        let state = GameState::new();
        assert!(!state.connected);
        assert!(state.character_name.is_none());
        assert!(state.room_id.is_none());
        assert!(state.room_name.is_none());
        assert!(state.exits.is_empty());
        assert_eq!(state.game_time, 0);
        assert!(state.roundtime_end.is_none());
        assert!(state.casttime_end.is_none());
        assert!(state.spell.is_none());
        assert!(state.active_streams.is_empty());
        assert!(state.inventory.is_empty());
        assert!(state.left_hand.is_none());
        assert!(state.right_hand.is_none());
        assert!(state.active_effects.is_empty());
        assert!(state.compass_dirs.is_empty());
        assert_eq!(state.last_prompt, ">");
        assert!(state.estimated_lag_ms.is_none());
    }

    #[test]
    fn test_game_state_default() {
        let state = GameState::default();
        assert!(!state.connected);
        assert_eq!(state.last_prompt, ">");
        assert_eq!(state.game_time, 0);
    }

    #[test]
    fn test_game_state_vitals_default() {
        let state = GameState::new();
        assert_eq!(state.vitals.health, 100);
        assert_eq!(state.vitals.mana, 100);
        assert_eq!(state.vitals.stamina, 100);
        assert_eq!(state.vitals.spirit, 100);
    }

    #[test]
    fn test_game_state_status_default() {
        let state = GameState::new();
        assert!(!state.status.standing);
        assert!(!state.status.kneeling);
        assert!(!state.status.sitting);
        assert!(!state.status.prone);
        assert!(!state.status.stunned);
        assert!(!state.status.bleeding);
        assert!(!state.status.hidden);
        assert!(!state.status.invisible);
        assert!(!state.status.webbed);
        assert!(!state.status.joined);
        assert!(!state.status.dead);
    }

    // ========== Game Time tests ==========

    #[test]
    fn test_update_game_time() {
        let mut state = GameState::new();
        let game_time = 1764905000;

        state.update_game_time(game_time);

        assert_eq!(state.game_time, game_time);
    }

    #[test]
    fn test_update_game_time_calculates_lag_on_first_call() {
        let mut state = GameState::new();
        let game_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        state.update_game_time(game_time);

        // Should calculate lag on first call (since last_lag_check_time is 0)
        assert!(state.estimated_lag_ms.is_some());
        assert_eq!(state.last_lag_check_time, game_time);
    }

    #[test]
    fn test_update_game_time_throttles_lag_calculation() {
        let mut state = GameState::new();
        let base_time = 1764905000i64;

        // First update - should calculate lag
        state.update_game_time(base_time);
        let first_lag = state.estimated_lag_ms;
        assert!(first_lag.is_some());

        // Update 10 seconds later - should NOT recalculate (< 30 sec threshold)
        state.update_game_time(base_time + 10);
        assert_eq!(state.estimated_lag_ms, first_lag);
        assert_eq!(state.last_lag_check_time, base_time); // Still the original check time

        // Update 35 seconds later - SHOULD recalculate (> 30 sec threshold)
        state.update_game_time(base_time + 35);
        assert_eq!(state.last_lag_check_time, base_time + 35);
    }

    // ========== Roundtime tests (using game time) ==========

    #[test]
    fn test_game_state_in_roundtime_none() {
        let state = GameState::new();
        assert!(!state.in_roundtime());
    }

    #[test]
    fn test_game_state_in_roundtime_future() {
        let mut state = GameState::new();
        let game_time = 1764905000;

        // Simulate: game time is 1764905000, roundtime ends at 1764905005 (5 sec RT)
        state.game_time = game_time;
        state.roundtime_end = Some(game_time + 5);

        assert!(state.in_roundtime());
    }

    #[test]
    fn test_game_state_in_roundtime_past() {
        let mut state = GameState::new();
        let game_time = 1764905010;

        // Simulate: game time is 1764905010, roundtime ended at 1764905005
        state.game_time = game_time;
        state.roundtime_end = Some(1764905005);

        assert!(!state.in_roundtime());
    }

    #[test]
    fn test_roundtime_remaining() {
        let mut state = GameState::new();
        state.game_time = 1764905000;
        state.roundtime_end = Some(1764905005);

        assert_eq!(state.roundtime_remaining(), 5);
    }

    #[test]
    fn test_roundtime_remaining_expired() {
        let mut state = GameState::new();
        state.game_time = 1764905010;
        state.roundtime_end = Some(1764905005);

        assert_eq!(state.roundtime_remaining(), 0); // Clamped to 0
    }

    #[test]
    fn test_roundtime_remaining_none() {
        let state = GameState::new();
        assert_eq!(state.roundtime_remaining(), 0);
    }

    // ========== Casttime tests (using game time) ==========

    #[test]
    fn test_game_state_in_casttime_none() {
        let state = GameState::new();
        assert!(!state.in_casttime());
    }

    #[test]
    fn test_game_state_in_casttime_future() {
        let mut state = GameState::new();
        let game_time = 1764905000;

        // Simulate: game time is 1764905000, casttime ends at 1764905003 (3 sec cast)
        state.game_time = game_time;
        state.casttime_end = Some(game_time + 3);

        assert!(state.in_casttime());
    }

    #[test]
    fn test_game_state_in_casttime_past() {
        let mut state = GameState::new();
        let game_time = 1764905010;

        // Simulate: game time is 1764905010, casttime ended at 1764905003
        state.game_time = game_time;
        state.casttime_end = Some(1764905003);

        assert!(!state.in_casttime());
    }

    #[test]
    fn test_casttime_remaining() {
        let mut state = GameState::new();
        state.game_time = 1764905000;
        state.casttime_end = Some(1764905003);

        assert_eq!(state.casttime_remaining(), 3);
    }

    #[test]
    fn test_casttime_remaining_expired() {
        let mut state = GameState::new();
        state.game_time = 1764905010;
        state.casttime_end = Some(1764905003);

        assert_eq!(state.casttime_remaining(), 0);
    }

    // ========== Lag tests ==========

    #[test]
    fn test_lag_ms_initially_none() {
        let state = GameState::new();
        assert!(state.lag_ms().is_none());
    }

    #[test]
    fn test_lag_ms_after_update() {
        let mut state = GameState::new();
        let game_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        state.update_game_time(game_time);

        // Lag should be calculated and be relatively small (within a few hundred ms)
        let lag = state.lag_ms().expect("lag should be calculated");
        // Allow for some system timing variance (within 5 seconds = 5000ms)
        assert!(lag.abs() < 5000, "lag {} ms is unexpectedly large", lag);
    }

    // ========== Clone and other tests ==========

    #[test]
    fn test_game_state_clone() {
        let mut state = GameState::new();
        state.connected = true;
        state.character_name = Some("TestChar".to_string());
        state.exits.push("north".to_string());
        state.vitals.health = 75;
        state.game_time = 1764905000;

        let cloned = state.clone();
        assert!(cloned.connected);
        assert_eq!(cloned.character_name, Some("TestChar".to_string()));
        assert_eq!(cloned.exits.len(), 1);
        assert_eq!(cloned.vitals.health, 75);
        assert_eq!(cloned.game_time, 1764905000);
    }

    #[test]
    fn test_game_state_active_streams() {
        let mut state = GameState::new();
        state.active_streams.insert("inv".to_string(), true);
        state.active_streams.insert("assess".to_string(), false);

        assert_eq!(state.active_streams.get("inv"), Some(&true));
        assert_eq!(state.active_streams.get("assess"), Some(&false));
        assert_eq!(state.active_streams.get("unknown"), None);
    }

    // ========== StatusInfo tests ==========

    #[test]
    fn test_status_info_default() {
        let status = StatusInfo::default();
        assert!(!status.standing);
        assert!(!status.kneeling);
        assert!(!status.sitting);
        assert!(!status.prone);
        assert!(!status.stunned);
        assert!(!status.bleeding);
        assert!(!status.hidden);
        assert!(!status.invisible);
        assert!(!status.webbed);
        assert!(!status.joined);
        assert!(!status.dead);
    }

    #[test]
    fn test_status_info_clone() {
        let mut status = StatusInfo::default();
        status.standing = true;
        status.hidden = true;

        let cloned = status.clone();
        assert!(cloned.standing);
        assert!(cloned.hidden);
        assert!(!cloned.dead);
    }

    // ========== Vitals tests ==========

    #[test]
    fn test_vitals_default() {
        let vitals = Vitals::default();
        assert_eq!(vitals.health, 100);
        assert_eq!(vitals.mana, 100);
        assert_eq!(vitals.stamina, 100);
        assert_eq!(vitals.spirit, 100);
    }

    #[test]
    fn test_vitals_clone() {
        let mut vitals = Vitals::default();
        vitals.health = 50;
        vitals.mana = 75;

        let cloned = vitals.clone();
        assert_eq!(cloned.health, 50);
        assert_eq!(cloned.mana, 75);
        assert_eq!(cloned.stamina, 100);
        assert_eq!(cloned.spirit, 100);
    }

    #[test]
    fn test_vitals_boundary_values() {
        let mut vitals = Vitals::default();
        vitals.health = 0;
        vitals.mana = 255; // u8 max

        assert_eq!(vitals.health, 0);
        assert_eq!(vitals.mana, 255);
    }

    // ========== Debug trait tests ==========

    #[test]
    fn test_game_state_debug() {
        let state = GameState::new();
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("GameState"));
        assert!(debug_str.contains("connected"));
        assert!(debug_str.contains("game_time"));
    }

    #[test]
    fn test_status_info_debug() {
        let status = StatusInfo::default();
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("StatusInfo"));
        assert!(debug_str.contains("standing"));
    }

    #[test]
    fn test_vitals_debug() {
        let vitals = Vitals::default();
        let debug_str = format!("{:?}", vitals);
        assert!(debug_str.contains("Vitals"));
        assert!(debug_str.contains("health"));
    }
}
