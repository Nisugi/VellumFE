///! Widget Management - Cache and sync all TUI widgets
///!
///! This module manages the lifecycle of all TUI widgets, including:
///! - Widget caches (HashMaps of widget instances)
///! - Sync methods (updating widgets from AppCore state)
///! - Widget initialization and updates

use std::collections::HashMap;

/// Widget manager handles all widget caches and synchronization
pub struct WidgetManager {
    /// Cache of TextWindow widgets per window name
    pub text_windows: HashMap<String, super::text_window::TextWindow>,
    /// Cache of CommandInput widgets per window name
    pub command_inputs: HashMap<String, super::command_input::CommandInput>,
    /// Cache of RoomWindow widgets per window name
    pub room_windows: HashMap<String, super::room_window::RoomWindow>,
    /// Cache of InventoryWindow widgets per window name
    pub inventory_windows: HashMap<String, super::inventory_window::InventoryWindow>,
    /// Cache of SpellsWindow widgets per window name
    pub spells_windows: HashMap<String, super::spells_window::SpellsWindow>,
    /// Cache of ProgressBar widgets per window name
    pub progress_bars: HashMap<String, super::progress_bar::ProgressBar>,
    /// Cache of Countdown widgets per window name
    pub countdowns: HashMap<String, super::countdown::Countdown>,
    /// Cache of ActiveEffects widgets per window name
    pub active_effects_windows: HashMap<String, super::active_effects::ActiveEffects>,
    /// Cache of Hand widgets per window name
    pub hand_widgets: HashMap<String, super::hand::Hand>,
    /// Cache of Spacer widgets per window name
    pub spacer_widgets: HashMap<String, super::spacer::Spacer>,
    /// Cache of Indicator widgets per window name
    pub indicator_widgets: HashMap<String, super::indicator::Indicator>,
    /// Cache of Targets widgets per window name
    pub targets_widgets: HashMap<String, super::targets::Targets>,
    /// Cache of DropdownTargets widgets per window name (for direct-connect users)
    pub dropdown_targets_widgets: HashMap<String, super::dropdown_targets::DropdownTargets>,
    /// Cache of Players widgets per window name
    pub players_widgets: HashMap<String, super::players::Players>,
    /// Cache of ContainerWindow widgets per window name
    pub container_widgets: HashMap<String, super::container_window::ContainerWindow>,
    /// Cache of Dashboard widgets per window name
    pub dashboard_widgets: HashMap<String, super::dashboard::Dashboard>,
    /// Cache of TabbedTextWindow widgets per window name
    pub tabbed_text_windows: HashMap<String, super::tabbed_text_window::TabbedTextWindow>,
    /// Cache of Compass widgets per window name
    pub compass_widgets: HashMap<String, super::compass::Compass>,
    /// Cache of InjuryDoll widgets per window name
    pub injury_doll_widgets: HashMap<String, super::injury_doll::InjuryDoll>,
    /// Cache of Performance widgets per window name
    pub performance_widgets: HashMap<String, super::performance_stats::PerformanceStatsWidget>,
    /// Cache of Perception widgets per window name
    pub perception_windows: HashMap<String, super::perception::PerceptionWindow>,
    /// Cache of Experience widgets per window name (DR skill training)
    pub experience_widgets: HashMap<String, super::experience::Experience>,
    /// Track last synced generation per text window to know what's new
    /// Using generation instead of line count to handle buffer rotation at max_lines
    pub last_synced_generation: HashMap<String, u64>,
}

impl WidgetManager {
    /// Create a new widget manager with empty caches
    pub fn new() -> Self {
        Self {
            text_windows: HashMap::new(),
            command_inputs: HashMap::new(),
            room_windows: HashMap::new(),
            inventory_windows: HashMap::new(),
            spells_windows: HashMap::new(),
            progress_bars: HashMap::new(),
            countdowns: HashMap::new(),
            active_effects_windows: HashMap::new(),
            hand_widgets: HashMap::new(),
            spacer_widgets: HashMap::new(),
            indicator_widgets: HashMap::new(),
            targets_widgets: HashMap::new(),
            dropdown_targets_widgets: HashMap::new(),
            players_widgets: HashMap::new(),
            container_widgets: HashMap::new(),
            dashboard_widgets: HashMap::new(),
            tabbed_text_windows: HashMap::new(),
            compass_widgets: HashMap::new(),
            injury_doll_widgets: HashMap::new(),
            performance_widgets: HashMap::new(),
            perception_windows: HashMap::new(),
            experience_widgets: HashMap::new(),
            last_synced_generation: HashMap::new(),
        }
    }

    /// Clear all widget caches - call after layout reload to reset state
    pub fn clear(&mut self) {
        self.text_windows.clear();
        self.command_inputs.clear();
        self.room_windows.clear();
        self.inventory_windows.clear();
        self.spells_windows.clear();
        self.progress_bars.clear();
        self.countdowns.clear();
        self.active_effects_windows.clear();
        self.hand_widgets.clear();
        self.spacer_widgets.clear();
        self.indicator_widgets.clear();
        self.targets_widgets.clear();
        self.dropdown_targets_widgets.clear();
        self.players_widgets.clear();
        self.container_widgets.clear();
        self.dashboard_widgets.clear();
        self.tabbed_text_windows.clear();
        self.compass_widgets.clear();
        self.injury_doll_widgets.clear();
        self.performance_widgets.clear();
        self.perception_windows.clear();
        self.experience_widgets.clear();
        self.last_synced_generation.clear();
    }
}

impl Default for WidgetManager {
    fn default() -> Self {
        Self::new()
    }
}
