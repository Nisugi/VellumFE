//! Window and indicator template definitions.
//!
//! Contains the built-in window template catalog (`get_window_template`),
//! user-defined template stores persisted to TOML, and the category
//! groupings used by the add-window menus.

use super::*;

/// Globally available indicator template definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndicatorTemplateEntry {
    /// Unique indicator id (case-preserved, e.g., "POISONED")
    pub id: String,
    /// Optional template name (defaults to lowercased id when omitted)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional display title
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_color: Option<String>,
    /// Enabled flag; if false, this template is skipped on load
    #[serde(
        default = "default_template_enabled",
        skip_serializing_if = "is_enabled_default"
    )]
    pub enabled: bool,
}

impl IndicatorTemplateEntry {
    /// Key used for template lookup (stable even if id casing differs)
    pub fn key(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.id.to_lowercase())
    }

    /// Title shown to users; falls back to id
    pub fn title_or_id(&self) -> String {
        self.title.clone().unwrap_or_else(|| self.id.clone())
    }
}

fn default_template_enabled() -> bool {
    true
}

fn is_enabled_default(value: &bool) -> bool {
    *value
}

/// TOML file wrapper for indicator templates
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndicatorTemplateStore {
    #[serde(default)]
    pub indicators: Vec<IndicatorTemplateEntry>,
}

/// Generic window template definition stored globally
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowTemplateEntry {
    /// Template name (used as the template key)
    pub name: String,
    /// Widget type this template represents (e.g., "progress", "countdown", "text")
    pub widget_type: String,
    /// Full window definition to clone when instantiating
    pub window: WindowDef,
    /// Enabled flag; if false, this template is skipped on load
    #[serde(
        default = "default_template_enabled",
        skip_serializing_if = "is_enabled_default"
    )]
    pub enabled: bool,
}

/// TOML file wrapper for window templates
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowTemplateStore {
    #[serde(default)]
    pub templates: Vec<WindowTemplateEntry>,
}


impl Config {
    /// Get a window template by name
    /// Returns a WindowDef with default positioning that can be customized
    pub fn get_window_template(name: &str) -> Option<WindowDef> {
        // Create base defaults that all windows share
        let base_defaults = WindowBase {
            name: String::new(), // Will be overridden
            row: 0,
            col: 0,
            rows: 10,
            cols: 40,
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: true,
            title: None, // Will be overridden
            title_position: default_title_position(),
            background_color: None,
            text_color: None,
            transparent_background: false,
            locked: false,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            visible: true,
            content_align: None,
        };
        // Prefer user-defined window templates (global store)
        if let Some(custom) = Self::get_custom_window_template(name) {
            return Some(custom);
        }

        // Prefer user-defined indicator templates (global store)
        if let Some(custom) = Self::get_custom_indicator_template(name, &base_defaults) {
            return Some(custom);
        }

        match name {
            "main" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "main".to_string(),
                    title: Some("Story".to_string()),
                    rows: 37,
                    cols: 120,
                    locked: true,
                    ..base_defaults
                },
                data: TextWidgetData {
                    streams: vec!["main".to_string()],
                    buffer_size: 10000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "room" => Some(WindowDef::Room {
                base: WindowBase {
                    name: "room".to_string(),
                    title: Some("Room".to_string()),
                    rows: 10,
                    cols: 80,
                    min_rows: Some(5),
                    ..base_defaults.clone()
                },
                data: RoomWidgetData {
                    buffer_size: 0,
                    show_desc: true,
                    show_objs: true,
                    show_players: true,
                    show_exits: true,
                    show_name: false,
                },
            }),

            "inventory" => Some(WindowDef::Inventory {
                base: WindowBase {
                    name: "inventory".to_string(),
                    title: Some("Inventory".to_string()),
                    rows: 20,
                    cols: 40,
                    min_rows: Some(4),
                    ..base_defaults.clone()
                },
                data: InventoryWidgetData {
                    streams: vec!["inv".to_string()],
                    buffer_size: 0, // No scrollback for inventory (content replaced each update)
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "command_input" => Some(WindowDef::CommandInput {
                base: WindowBase {
                    name: "command_input".to_string(),
                    title: Some("Command Input".to_string()),
                    rows: 1,
                    cols: 120,
                    min_rows: Some(1),
                    max_rows: Some(1),
                    locked: true,
                    ..base_defaults.clone()
                },
                data: CommandInputWidgetData::default(),
            }),

            "quickbar" => Some(WindowDef::Quickbar {
                base: WindowBase {
                    name: "quickbar".to_string(),
                    title: Some("Quickbar".to_string()),
                    rows: 3,
                    cols: 120,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    show_border: true,
                    show_title: false,
                    ..base_defaults.clone()
                },
                data: QuickbarWidgetData {},
            }),

            "health" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "health".to_string(),
                    title: Some("Health".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                    id: Some("health".to_string()),
                    label: Some("Health".to_string()),
                    color: Some("#6e0202".to_string()), // Dark red
                    numbers_only: false,
                    current_only: false,
                },
            }),
            "performance" => Some(WindowDef::Performance {
                base: WindowBase {
                    name: "performance".to_string(),
                    title: Some("Performance Stats".to_string()),
                    row: 0,
                    col: 0,
                    rows: 10,
                    cols: 40,
                    min_rows: Some(4),
                    min_cols: Some(20),
                    ..base_defaults.clone()
                },
                data: PerformanceWidgetData {
                    enabled: true,
                    show_fps: false,
                    show_frame_times: false,
                    show_render_times: false,
                    show_ui_times: false,
                    show_wrap_times: false,
                    show_net: false,
                    show_parse: false,
                    show_events: false,
                    show_memory: false,
                    show_lines: false,
                    show_uptime: true,
                    show_jitter: false,
                    show_frame_spikes: false,
                    show_event_lag: false,
                    show_memory_delta: false,
                },
            }),

            "mana" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "mana".to_string(),
                    title: Some("Mana".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                    id: Some("mana".to_string()),
                    label: Some("Mana".to_string()),
                    color: Some("#08086d".to_string()), // Dark blue
                    numbers_only: false,
                    current_only: false,
                },
            }),

            "stamina" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "stamina".to_string(),
                    title: Some("Stamina".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                    id: Some("stamina".to_string()),
                    label: Some("Stamina".to_string()),
                    color: Some("#bd7b00".to_string()), // Orange
                    numbers_only: false,
                    current_only: false,
                },
            }),
            "targets" => Some(WindowDef::Targets {
                base: WindowBase {
                    name: "targets".to_string(),
                    title: Some("Targets".to_string()),
                    row: 0,
                    col: 0,
                    rows: 10,
                    cols: 40,
                    min_rows: Some(4),
                    min_cols: Some(20),
                    ..base_defaults.clone()
                },
                data: TargetsWidgetData {
                    entity_id: default_target_entity_id(),
                    show_body_part_count: false,
                    status_position: None,
                },
            }),
            "players" => Some(WindowDef::Players {
                base: WindowBase {
                    name: "players".to_string(),
                    title: Some("Players".to_string()),
                    row: 0,
                    col: 0,
                    rows: 10,
                    cols: 40,
                    min_rows: Some(4),
                    min_cols: Some(20),
                    ..base_defaults.clone()
                },
                data: PlayersWidgetData {
                    entity_id: default_player_entity_id(),
                },
            }),
            "items" => Some(WindowDef::Items {
                base: WindowBase {
                    name: "items".to_string(),
                    title: Some("Items".to_string()),
                    row: 0,
                    col: 0,
                    rows: 10,
                    cols: 40,
                    min_rows: Some(4),
                    min_cols: Some(20),
                    ..base_defaults.clone()
                },
                data: ItemsWidgetData {
                    entity_id: default_items_entity_id(),
                },
            }),

            "entity_custom" => Some(WindowDef::Targets {
                base: WindowBase {
                    name: String::new(), // Auto-generated by WindowEditor
                    title: Some("Custom".to_string()),
                    row: 0,
                    col: 0,
                    rows: 10,
                    cols: 40,
                    min_rows: Some(4),
                    min_cols: Some(20),
                    ..base_defaults.clone()
                },
                data: TargetsWidgetData {
                    entity_id: String::new(),
                    show_body_part_count: false,
                    status_position: None,
                },
            }),

            "dashboard" => Some(WindowDef::Dashboard {
                base: WindowBase {
                    name: "dashboard".to_string(),
                    title: Some("Dashboard".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 10,
                    min_rows: Some(1),
                    min_cols: Some(1),
                    ..base_defaults.clone()
                },
                data: DashboardWidgetData {
                    layout: default_dashboard_layout(),
                    spacing: default_dashboard_spacing(),
                    hide_inactive: default_dashboard_hide_inactive(),
                    indicators: Vec::new(),
                },
            }),

            "poisoned" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "poisoned".to_string(),
                    title: Some("Poisoned".to_string()),
                    row: 0,
                    col: 0,
                    rows: 2,
                    cols: 1,
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    // Skull and crossbones
                    icon: Some("".to_string()),
                    indicator_id: Some("POISONED".to_string()),
                    inactive_color: None,
                    active_color: Some("#00ff00".to_string()),
                    default_status: None,
                    default_color: Some("#00ff00".to_string()),
                },
            }),
            "bleeding" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "bleeding".to_string(),
                    title: Some("Bleeding".to_string()),
                    row: 0,
                    col: 0,
                    rows: 2,
                    cols: 1,
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("".to_string()), // Nerdfont bleeding icon
                    indicator_id: Some("BLEEDING".to_string()),
                    inactive_color: None,
                    active_color: Some("#ff0000".to_string()),
                    default_status: None,
                    default_color: Some("#ff0000".to_string()),
                },
            }),
            "diseased" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "diseased".to_string(),
                    title: Some("Diseased".to_string()),
                    row: 0,
                    col: 0,
                    rows: 2,
                    cols: 1,
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("".to_string()), // Nerdfont diseased icon
                    indicator_id: Some("DISEASED".to_string()),
                    inactive_color: None,
                    active_color: Some("#8b4513".to_string()),
                    default_status: None,
                    default_color: Some("#8b4513".to_string()),
                },
            }),
            "stunned" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "stunned".to_string(),
                    title: Some("Stunned".to_string()),
                    row: 0,
                    col: 0,
                    rows: 2,
                    cols: 1,
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("󱐌".to_string()), // Lightning bolt
                    indicator_id: Some("STUNNED".to_string()),
                    inactive_color: None,
                    active_color: Some("#ffff00".to_string()),
                    default_status: None,
                    default_color: Some("#ffff00".to_string()),
                },
            }),
            "webbed" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "webbed".to_string(),
                    title: Some("Webbed".to_string()),
                    row: 0,
                    col: 0,
                    rows: 2,
                    cols: 1,
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("󰯊".to_string()), // Nerdfont web icon
                    indicator_id: Some("WEBBED".to_string()),
                    inactive_color: None,
                    active_color: Some("#cccccc".to_string()),
                    default_status: None,
                    default_color: Some("#cccccc".to_string()),
                },
            }),

            "spirit" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "spirit".to_string(),
                    title: Some("Spirit".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                    id: Some("spirit".to_string()),
                    label: Some("Spirit".to_string()),
                    color: Some("#6e727c".to_string()), // Gray
                    numbers_only: false,
                    current_only: false,
                },
            }),

            // DR-specific: Concentration bar (4th vital in DragonRealms)
            "concentration" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "concentration".to_string(),
                    title: Some("Concentration".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                    id: Some("concentration".to_string()),
                    label: Some("Conc".to_string()), // Short label for narrow bars
                    color: Some("#00a0a0".to_string()), // Cyan/teal
                    numbers_only: false,
                    current_only: false,
                },
            }),

            "stance" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "stance".to_string(),
                    title: Some("Stance".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                    id: Some("pbarStance".to_string()),
                    label: Some("Stance".to_string()),
                    color: Some("#000080".to_string()), // Navy
                    numbers_only: false,
                    current_only: false,
                },
            }),

"progress_custom" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: String::new(), // Auto-generated by WindowEditor
                    title: Some("Custom".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                id: None,
                    label: None,
                    color: None,
                    numbers_only: false,
                    current_only: false,
                },
            }),

            "roundtime" => Some(WindowDef::Countdown {
                base: WindowBase {
                    name: "roundtime".to_string(),
                    title: Some("RT".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    text_color: Some("#FF0000".to_string()), // Red
                    ..base_defaults.clone()
                },
                data: CountdownWidgetData {
                    id: Some("roundtime".to_string()),
                    label: None,
                    icon: Some(default_countdown_icon().chars().next().unwrap_or('█')),
                    color: None,
                    background_color: None,
                },
            }),

            "casttime" => Some(WindowDef::Countdown {
                base: WindowBase {
                    name: "casttime".to_string(),
                    title: Some("Cast".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    text_color: Some("#00BFFF".to_string()), // Deep sky blue
                    ..base_defaults.clone()
                },
                data: CountdownWidgetData {
                    id: Some("casttime".to_string()),
                    label: None,
                    icon: Some(default_countdown_icon().chars().next().unwrap_or('█')),
                    color: None,
                    background_color: None,
                },
            }),

            "stuntime" => Some(WindowDef::Countdown {
                base: WindowBase {
                    name: "stuntime".to_string(),
                    title: Some("Stun".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    text_color: Some("#FFFF00".to_string()), // Yellow
                    ..base_defaults.clone()
                },
                data: CountdownWidgetData {
                    id: Some("stuntime".to_string()),
                    label: None,
                    icon: Some(default_countdown_icon().chars().next().unwrap_or('█')),
                    color: None,
                    background_color: None,
                },
            }),

            "countdown_custom" => Some(WindowDef::Countdown {
                base: WindowBase {
                    name: String::new(), // Auto-generated by WindowEditor
                    title: Some("Custom".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: CountdownWidgetData {
                    id: None,
                    label: None,
                    icon: Some(default_countdown_icon().chars().next().unwrap_or('█')),
                    color: None,
                    background_color: None,
                },
            }),

            "compass" => Some(WindowDef::Compass {
                base: WindowBase {
                    name: "compass".to_string(),
                    title: Some("Compass".to_string()),
                    row: 0,
                    col: 0,
                    rows: 5, // 3 for compass grid + 2 for border
                    cols: 9, // 7 for compass grid + 2 for border
                    show_border: true,
                    min_rows: Some(3),
                    min_cols: Some(7),
                    content_align: Some("center".to_string()),
                    ..base_defaults.clone()
                },
                data: CompassWidgetData {
                    active_color: Some("#00FF00".to_string()),   // Green
                    inactive_color: Some("#333333".to_string()), // Dark gray
                },
            }),

            "injuries" | "injury_doll" => Some(WindowDef::InjuryDoll {
                base: WindowBase {
                    name: "injuries".to_string(),
                    title: Some("Injuries".to_string()),
                    row: 0,
                    col: 0,
                    rows: 8,  // 6 for injury doll + 2 for border
                    cols: 10, // 8 for injury doll (5+3 for labels) + 2 for border
                    show_border: true,
                    min_rows: Some(6),
                    min_cols: Some(8),
                    content_align: Some("center".to_string()),
                    ..base_defaults.clone()
                },
                data: InjuryDollWidgetData {
                    injury_default_color: None,
                    injury1_color: Some("#aa5500".to_string()), // Brown
                    injury2_color: Some("#ff8800".to_string()), // Orange
                    injury3_color: Some("#ff0000".to_string()), // Bright red
                    scar1_color: Some("#999999".to_string()),   // Light gray
                    scar2_color: Some("#777777".to_string()),   // Medium gray
                    scar3_color: Some("#555555".to_string()),   // Darker gray
                },
            }),

            "buffs" => Some(WindowDef::ActiveEffects {
                base: WindowBase {
                    name: "buffs".to_string(),
                    title: Some("Buffs".to_string()),
                    rows: 10,
                    cols: 30,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: ActiveEffectsWidgetData {
                    category: "Buffs".to_string(),
                },
            }),

            "debuffs" => Some(WindowDef::ActiveEffects {
                base: WindowBase {
                    name: "debuffs".to_string(),
                    title: Some("Debuffs".to_string()),
                    rows: 10,
                    cols: 30,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: ActiveEffectsWidgetData {
                    category: "Debuffs".to_string(),
                },
            }),

            "cooldowns" => Some(WindowDef::ActiveEffects {
                base: WindowBase {
                    name: "cooldowns".to_string(),
                    title: Some("Cooldowns".to_string()),
                    rows: 10,
                    cols: 30,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: ActiveEffectsWidgetData {
                    category: "Cooldowns".to_string(),
                },
            }),

            "active_spells" => Some(WindowDef::ActiveEffects {
                base: WindowBase {
                    name: "active_spells".to_string(),
                    title: Some("Active Spells".to_string()),
                    rows: 10,
                    cols: 30,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: ActiveEffectsWidgetData {
                    category: "ActiveSpells".to_string(),
                },
            }),

            "active_effects_custom" => Some(WindowDef::ActiveEffects {
                base: WindowBase {
                    name: String::new(), // Auto-generated by WindowEditor
                    title: Some("Custom".to_string()),
                    rows: 10,
                    cols: 30,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: ActiveEffectsWidgetData {
                    category: String::new(),
                },
            }),

            "left" => Some(WindowDef::Hand {
                base: WindowBase {
                    name: "left".to_string(),
                    title: Some("Left Hand".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: HandWidgetData {
                    icon: Some("L:".to_string()),
                    icon_color: None,
                    text_color: None,
                },
            }),

            "right" => Some(WindowDef::Hand {
                base: WindowBase {
                    name: "right".to_string(),
                    title: Some("Right Hand".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: HandWidgetData {
                    icon: Some("R:".to_string()),
                    icon_color: None,
                    text_color: None,
                },
            }),

            "spell" => Some(WindowDef::Hand {
                base: WindowBase {
                    name: "spell".to_string(),
                    title: Some("Spell".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: HandWidgetData {
                    icon: Some("S:".to_string()),
                    icon_color: None,
                    text_color: None,
                },
            }),

            // Text window templates for common streams
            "thoughts" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "thoughts".to_string(),
                    title: Some("Thoughts".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["thoughts".to_string()],
                    buffer_size: 1000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "speech" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "speech".to_string(),
                    title: Some("Speech".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["speech".to_string()],
                    buffer_size: 1000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "announcements" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "announcements".to_string(),
                    title: Some("Announcements".to_string()),
                    rows: 10,
                    cols: 50,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["announcements".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "loot" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "loot".to_string(),
                    title: Some("Loot".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["loot".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "death" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "death".to_string(),
                    title: Some("Death".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["death".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "logons" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "logons".to_string(),
                    title: Some("Logons".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["logons".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "familiar" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "familiar".to_string(),
                    title: Some("Familiar".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["familiar".to_string()],
                    buffer_size: 1000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "ambients" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "ambients".to_string(),
                    title: Some("Ambients".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["ambients".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "bounty" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "bounty".to_string(),
                    title: Some("Bounties".to_string()),
                    rows: 15,
                    cols: 50,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["bounty".to_string()],
                    buffer_size: 10, // Small buffer - content is cleared and replaced by clearStream
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "society" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "society".to_string(),
                    title: Some("Society Tasks".to_string()),
                    rows: 15,
                    cols: 50,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["society".to_string()],
                    buffer_size: 10, // Small buffer - content is cleared and replaced by clearStream
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

"text_custom" => Some(WindowDef::Text {
                base: WindowBase {
                    name: String::new(),
                    title: None,
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["custom".to_string()],
                    buffer_size: 1000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "spells" => Some(WindowDef::Spells {
                base: WindowBase {
                    name: "spells".to_string(),
                    title: Some("Spells".to_string()),
                    rows: 20,
                    cols: 40,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: SpellsWidgetData {},
            }),

            "chat" => Some(WindowDef::TabbedText {
                base: WindowBase {
                    name: "chat".to_string(),
                    title: Some("Chat".to_string()),
                    rows: 10,
                    cols: 60,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TabbedTextWidgetData {
                    tabs: vec![
                        TabbedTextTab {
                            name: "Thoughts".to_string(),
                            stream: None,
                            streams: vec!["thoughts".to_string()],
                            show_timestamps: None,
                            ignore_activity: Some(false),
                            timestamp_position: None,
                        },
                        TabbedTextTab {
                            name: "Speech".to_string(),
                            stream: None,
                            streams: vec!["speech".to_string()],
                            show_timestamps: None,
                            ignore_activity: Some(false),
                            timestamp_position: None,
                        },
                        TabbedTextTab {
                            name: "Announcements".to_string(),
                            stream: None,
                            streams: vec!["announcements".to_string()],
                            show_timestamps: None,
                            ignore_activity: Some(false),
                            timestamp_position: None,
                        },
                        TabbedTextTab {
                            name: "Loot".to_string(),
                            stream: None,
                            streams: vec!["loot".to_string()],
                            show_timestamps: None,
                            ignore_activity: Some(false),
                            timestamp_position: None,
                        },
                        TabbedTextTab {
                            name: "Ambients".to_string(),
                            stream: None,
                            streams: vec!["ambients".to_string()],
                            show_timestamps: None,
                            ignore_activity: Some(false),
                            timestamp_position: None,
                        },
                    ],
                    buffer_size: 5000,
                    tab_bar_position: "top".to_string(),
                    tab_separator: true,
                    tab_active_color: None,
                    tab_inactive_color: None,
                    tab_unread_color: None,
                    tab_unread_prefix: None,
                },
            }),
            "tabbedtext_custom" => Some(WindowDef::TabbedText {
                base: WindowBase {
                    name: String::new(),
                    title: None,
                    rows: 10,
                    cols: 60,
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TabbedTextWidgetData {
                    tabs: vec![TabbedTextTab {
                        name: "Main".to_string(),
                        stream: None,
                        streams: vec!["main".to_string()],
                        show_timestamps: None, // Per-tab setting, no global default
                        ignore_activity: Some(false),
                        timestamp_position: None,
                    }],
                    buffer_size: 5000,
                    tab_bar_position: "top".to_string(),
                    tab_separator: true,
                    tab_active_color: None,
                    tab_inactive_color: None,
                    tab_unread_color: None,
                    tab_unread_prefix: None,
                },
            }),

            "spacer" => Some(WindowDef::Spacer {
                base: WindowBase {
                    name: String::new(), // Will be set by caller with auto-generated name
                    rows: 2,
                    cols: 2,
                    show_border: false, // Spacers never show borders
                    show_title: false, // Spacers never show titles
                    transparent_background: false, // Respects theme background color
                    ..base_defaults
                },
                data: SpacerWidgetData {},
            }),

            "perception" => Some(WindowDef::Perception {
                base: WindowBase {
                    name: "perception".to_string(),
                    title: Some("Perceptions".to_string()),
                    row: 0,
                    col: 0,
                    rows: 20,
                    cols: 40,
                    min_rows: Some(5),
                    min_cols: Some(20),
                    ..base_defaults.clone()
                },
                data: PerceptionWidgetData {
                    stream: "percWindow".to_string(),
                    buffer_size: 100,
                    sort_direction: SortDirection::Descending,
                    text_replacements: vec![],
                    use_short_spell_names: false,
                },
            }),

            // DR-specific: Experience window (skill training status)
            "experience" => Some(WindowDef::Experience {
                base: WindowBase {
                    name: "experience".to_string(),
                    title: Some("Experience".to_string()),
                    row: 0,
                    col: 0,
                    rows: 20,
                    cols: 35,
                    min_rows: Some(5),
                    min_cols: Some(20),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: ExperienceWidgetData {
                    align: "left".to_string(),
                },
            }),

            "gs4_experience" => Some(WindowDef::GS4Experience {
                base: WindowBase {
                    name: "gs4_experience".to_string(),
                    title: Some("Experience".to_string()),
                    row: 0,
                    col: 0,
                    rows: 5,           // 3 content (level, mind, exp) + 2 borders = 5 total
                    cols: 30,
                    min_rows: Some(5), // Minimum with all content + borders
                    max_rows: Some(5), // Maximum with all content + borders
                    min_cols: Some(20),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: GS4ExperienceWidgetData {
                    align: "center".to_string(),
                    show_level: true,
                    show_exp_bar: true,
                    mind_bar_color: None,
                    exp_bar_color: None,
                },
            }),

            "encum" => Some(WindowDef::Encumbrance {
                base: WindowBase {
                    name: "encum".to_string(),
                    title: Some("Encumbrance".to_string()),
                    row: 0,
                    col: 0,
                    rows: 4,           // 1 bar + 1 label + 2 borders = 4 total
                    cols: 25,
                    min_rows: Some(4), // Minimum with borders + label
                    max_rows: Some(4), // Maximum with borders + label
                    min_cols: Some(15),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: EncumbranceWidgetData {
                    align: "left".to_string(),
                    show_label: true,
                    color_light: None,
                    color_moderate: None,
                    color_heavy: None,
                    color_critical: None,
                },
            }),

            "minivitals" => Some(WindowDef::MiniVitals {
                base: WindowBase {
                    name: "minivitals".to_string(),
                    title: None, // No title shown (like Wrayth Stats)
                    row: 0,
                    col: 0,
                    rows: 3, // 1 content row + 2 borders = 3 total
                    cols: 80, // Wide to fit 4 bars
                    min_rows: Some(3),
                    max_rows: Some(3),
                    min_cols: Some(40),
                    show_border: true, // Borders enabled by default
                    ..base_defaults
                },
                data: MiniVitalsWidgetData {
                    numbers_only: false,
                    current_only: false,
                    health_color: None,
                    mana_color: None,
                    stamina_color: None,
                    spirit_color: None,
                    concentration_color: None,
                    bar_order: default_minivitals_bar_order(),
                },
            }),

            "betrayer" => Some(WindowDef::Betrayer {
                base: WindowBase {
                    name: "betrayer".to_string(),
                    title: Some("Betrayer".to_string()),
                    row: 0,
                    col: 0,
                    rows: 4, // 1 bar + 1 item + 2 borders
                    cols: 30,
                    min_rows: Some(3), // bar + borders (when show_items=false)
                    max_rows: Some(12), // Allow growth for more items
                    min_cols: Some(20),
                    show_border: true,
                    ..base_defaults
                },
                data: BetrayerWidgetData {
                    show_items: true,
                    bar_color: None, // Default to #8b0000 in widget
                },
            }),

            _ => None,
        }
    }

    /// Resolve a user-defined indicator template by name
    fn get_custom_indicator_template(name: &str, base_defaults: &WindowBase) -> Option<WindowDef> {
        let store = Self::load_indicator_template_store().ok()?;
        store
            .indicators
            .iter()
            .find(|tpl| tpl.enabled && tpl.key().eq_ignore_ascii_case(name))
            .map(|tpl| {
                let mut base = base_defaults.clone();
                base.name = tpl.key();
                base.title = Some(tpl.title_or_id());
                base.rows = 1;
                base.cols = 1;
                base.min_rows = Some(1);
                base.max_rows = Some(1);
                base.min_cols = Some(1);
                base.max_cols = Some(1);

                WindowDef::Indicator {
                    base,
                    data: IndicatorWidgetData {
                        icon: tpl.icon.clone(),
                        indicator_id: Some(tpl.id.clone()),
                        inactive_color: tpl.inactive_color.clone(),
                        active_color: tpl.active_color.clone(),
                        default_status: tpl.default_status.clone(),
                        default_color: tpl.default_color.clone(),
                    },
                }
            })
    }

    /// Resolve a user-defined window template by name (non-indicator)
    fn get_custom_window_template(name: &str) -> Option<WindowDef> {
        let store = Self::load_window_template_store().ok()?;
        store
            .templates
            .iter()
            .find(|tpl| tpl.enabled && tpl.name.eq_ignore_ascii_case(name))
            .map(|tpl| {
                // Ensure the stored window name matches the template name
                let mut window = tpl.window.clone();
                window.base_mut().name = tpl.name.clone();
                window
            })
    }

    /// Get list of all available window templates
    /// Returns all windows that can be added via .menu
    pub fn list_window_templates() -> Vec<String> {
        let mut templates = vec![
            // Progress bars
            "health".to_string(),
            "mana".to_string(),
            "stamina".to_string(),
            "spirit".to_string(),
            "concentration".to_string(), // DR-specific
            "stance".to_string(),
            "progress_custom".to_string(),
            "dashboard".to_string(),
            "poisoned".to_string(),
            "bleeding".to_string(),
            "diseased".to_string(),
            "stunned".to_string(),
            "webbed".to_string(),
            // Text windows
            "main".to_string(),
            "thoughts".to_string(),
            "speech".to_string(),
            "announcements".to_string(),
            "loot".to_string(),
            "death".to_string(),
            "logons".to_string(),
            "familiar".to_string(),
            "ambients".to_string(),
            "bounty".to_string(),
            "society".to_string(),
            "text_custom".to_string(),
            // Tabbed text windows
            "chat".to_string(),
            "tabbedtext_custom".to_string(),
            // Entity
            "targets".to_string(),
            "players".to_string(),
            "items".to_string(),
            "entity_custom".to_string(),
            // Countdowns
            "roundtime".to_string(),
            "casttime".to_string(),
            "stuntime".to_string(),
            "countdown_custom".to_string(),
            // Hands
            "left".to_string(),
            "right".to_string(),
            "spell".to_string(),
            // Active Effects
            "buffs".to_string(),
            "debuffs".to_string(),
            "cooldowns".to_string(),
            "active_spells".to_string(),
            "active_effects_custom".to_string(),
            // Other
            "inventory".to_string(),
            "room".to_string(),
            "spells".to_string(),
            "compass".to_string(),
            "injuries".to_string(),
            "quickbar".to_string(),
            "spacer".to_string(),
            // "performance" removed - now overlay-only via F12
            "perception".to_string(),
            "experience".to_string(),     // DR-specific
            "gs4_experience".to_string(), // GS4-specific
            "encum".to_string(),          // Available for both games
            "minivitals".to_string(),     // GS4-specific
            "betrayer".to_string(),       // GS4-specific
            // command_input is NOT in this list - it's always present and can't be added/removed
        ];

        // Add enabled global window templates
        if let Ok(store) = Self::load_window_template_store() {
            for tpl in store.templates {
                if !tpl.enabled {
                    continue;
                }
                let key = tpl.name.to_lowercase();
                if !templates.iter().any(|t| t.to_lowercase() == key) {
                    templates.push(tpl.name);
                }
            }
        }

        if let Ok(store) = Self::load_indicator_template_store() {
            for tpl in store.indicators {
                let key = tpl.key();
                if !tpl.enabled {
                    continue;
                }
                if !templates.iter().any(|t| t.eq_ignore_ascii_case(&key)) {
                    templates.push(key);
                }
            }
        }

        templates
    }

    /// Get the game type requirement for a template
    /// Returns None if template is available for all games
    pub fn template_game_type(name: &str) -> Option<GameType> {
        match name {
            // DR-specific templates
            "experience" | "concentration" | "perception" => Some(GameType::DR),
            // GS4-specific templates
            "gs4_experience" | "betrayer" | "minivitals" => Some(GameType::GS4),
            // All others (including encum) available for both games
            _ => None,
        }
    }

    /// Map dialog ID to template name when they differ.
    /// Most dialogs use the same ID as the template, but some have special mappings.
    pub fn dialog_id_to_template(dialog_id: &str) -> &str {
        match dialog_id {
            // GS4 expr dialog -> gs4_experience template
            "expr" => "gs4_experience",
            // Most dialogs use the same ID as template
            _ => dialog_id,
        }
    }

    /// List window templates filtered by game type
    pub fn list_window_templates_for_game(game: Option<GameType>) -> Vec<String> {
        Self::list_window_templates()
            .into_iter()
            .filter(|name| match Self::template_game_type(name) {
                None => true, // Available for all games
                Some(required_game) => game == Some(required_game),
            })
            .collect()
    }

    /// Return all indicator templates (built-in + user-defined), deduplicated by id
    pub fn list_indicator_templates() -> Vec<IndicatorTemplateEntry> {
        let mut templates = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for name in Self::list_window_templates() {
            if let Some(WindowDef::Indicator { base, data }) = Self::get_window_template(&name) {
                // Skip legacy placeholder
                if base.name == "indicator_custom" {
                    continue;
                }

                let id = data
                    .indicator_id
                    .clone()
                    .unwrap_or_else(|| base.name.clone());
                let key = id.to_lowercase();
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);

                templates.push(IndicatorTemplateEntry {
                    id,
                    name: Some(base.name),
                    title: base.title.clone(),
                    icon: data.icon,
                    inactive_color: data.inactive_color,
                    active_color: data.active_color,
                    default_status: data.default_status,
                    default_color: data.default_color,
                    enabled: true,
                });
            }
        }

        templates.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
        templates
    }

    /// Load indicator templates from the global store file
    pub fn load_indicator_template_store() -> Result<IndicatorTemplateStore> {
        let path = Self::indicator_templates_path()?;
        if !path.exists() {
            return Ok(IndicatorTemplateStore::default());
        }

        let contents =
            fs::read_to_string(&path).context(format!("Failed to read indicator templates at {:?}", path))?;
        let mut store: IndicatorTemplateStore = toml::from_str(&contents)
            .context(format!("Failed to parse indicator templates at {:?}", path))?;

        // Deduplicate by key (case-insensitive)
        let mut seen = std::collections::HashSet::new();
        store.indicators.retain(|tpl| {
            let key = tpl.key().to_lowercase();
            if seen.contains(&key) {
                false
            } else {
                seen.insert(key);
                true
            }
        });

        Ok(store)
    }

    /// Save indicator templates to the global store file
    pub fn save_indicator_template_store(store: &IndicatorTemplateStore) -> Result<()> {
        let path = Self::indicator_templates_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut sorted = store.clone();
        sorted
            .indicators
            .sort_by(|a, b| a.key().to_lowercase().cmp(&b.key().to_lowercase()));

        let contents =
            toml::to_string_pretty(&sorted).context("Failed to serialize indicator templates")?;
        fs::write(&path, contents)
            .context(format!("Failed to write indicator templates to {:?}", path))?;
        Ok(())
    }

    /// Path to the shared indicator template store
    pub fn indicator_templates_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("indicator_templates.toml"))
    }

    /// Load window templates from the global store file
    pub fn load_window_template_store() -> Result<WindowTemplateStore> {
        let path = Self::window_templates_path()?;
        if !path.exists() {
            return Ok(WindowTemplateStore::default());
        }

        let contents =
            fs::read_to_string(&path).context(format!("Failed to read window templates at {:?}", path))?;
        let mut store: WindowTemplateStore = toml::from_str(&contents)
            .context(format!("Failed to parse window templates at {:?}", path))?;

        // Deduplicate by name (case-insensitive) keeping first occurrence
        let mut seen = std::collections::HashSet::new();
        store.templates.retain(|tpl| {
            let key = tpl.name.to_lowercase();
            if seen.contains(&key) {
                false
            } else {
                seen.insert(key);
                true
            }
        });

        Ok(store)
    }

    /// Save window templates to the global store file
    pub fn save_window_template_store(store: &WindowTemplateStore) -> Result<()> {
        let path = Self::window_templates_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut sorted = store.clone();
        sorted
            .templates
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let contents =
            toml::to_string_pretty(&sorted).context("Failed to serialize window templates")?;
        fs::write(&path, contents)
            .context(format!("Failed to write window templates to {:?}", path))?;
        Ok(())
    }

    /// Path to the shared window template store
    pub fn window_templates_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("window_templates.toml"))
    }

    /// Upsert a window definition into the global window template store
    /// Enabled is always true on save; users can disable manually in the TOML.
    pub fn upsert_window_template(window: &WindowDef) -> Result<()> {
        let mut store = Self::load_window_template_store().unwrap_or_default();
        let key = window.name().to_lowercase();

        store
            .templates
            .retain(|tpl| tpl.name.to_lowercase() != key);

        store.templates.push(WindowTemplateEntry {
            name: window.name().to_string(),
            widget_type: window.widget_type().to_string(),
            window: window.clone(),
            enabled: true,
        });

        Self::save_window_template_store(&store)
    }

    /// True if a global window template exists with this name (case-insensitive)
    pub fn window_template_exists(name: &str) -> bool {
        if let Ok(store) = Self::load_window_template_store() {
            return store
                .templates
                .iter()
                .any(|tpl| tpl.name.eq_ignore_ascii_case(name));
        }
        false
    }

    /// Get templates grouped by widget category
    pub fn get_templates_by_category() -> HashMap<WidgetCategory, Vec<String>> {
        let mut categories: HashMap<WidgetCategory, Vec<String>> = HashMap::new();

        for template_name in Self::list_window_templates() {
            if let Some(template) = Self::get_window_template(&template_name) {
                let category = WidgetCategory::from_widget_type(template.widget_type());
                categories
                    .entry(category)
                    .or_default()
                    .push(template_name);
            }
        }

        categories
    }

    /// Get addable templates by category (excluding visible windows and wrong game type)
    pub fn get_addable_templates_by_category(
        layout: &crate::config::Layout,
        game_type: Option<GameType>,
    ) -> HashMap<WidgetCategory, Vec<String>> {
        let all_by_category = Self::get_templates_by_category();

        all_by_category
            .into_iter()
            .map(|(category, templates)| {
                let available: Vec<String> = templates
                    .into_iter()
                    .filter(|name| {
                        // Filter by game type first
                        match Self::template_game_type(name) {
                            None => true, // Available for all games
                            Some(required_game) => game_type == Some(required_game),
                        }
                    })
                    .filter(|name| {
                        // Then filter out already visible windows
                        !layout
                            .windows
                            .iter()
                            .any(|w| w.name() == *name && w.base().visible)
                    })
                    .collect();
                (category, available)
            })
            .filter(|(category, templates)| {
                !templates.is_empty() || matches!(category, WidgetCategory::Status)
            })
            .collect()
    }

    /// Get visible windows by category (for Hide/Edit menus)
    /// Returns only categories that have visible windows (excludes essential windows like main/command_input for hide menu)
    pub fn get_visible_templates_by_category(
        layout: &crate::config::Layout,
        exclude_essential: bool,
    ) -> HashMap<WidgetCategory, Vec<String>> {
        let all_by_category = Self::get_templates_by_category();

        let mut visible_by_category: HashMap<WidgetCategory, Vec<String>> = all_by_category
            .into_iter()
            .map(|(category, templates)| {
                let visible: Vec<String> = templates
                    .into_iter()
                    .filter(|name| {
                        // Skip essential windows for hide menu
                        if exclude_essential && (*name == "main" || *name == "command_input") {
                            return false;
                        }
                        // Include only visible windows
                        layout
                            .windows
                            .iter()
                            .any(|w| w.name() == *name && w.base().visible)
                    })
                    .collect();
                (category, visible)
            })
            .filter(|(category, templates)| {
                !templates.is_empty()
                    || (!exclude_essential && matches!(category, WidgetCategory::Status))
            })
            .collect();

        // Special-case command_input: always present, not addable, not hideable, but editable
        if !exclude_essential {
            if let Some(cmd) = layout
                .windows
                .iter()
                .find(|w| w.widget_type() == "command_input" && w.base().visible)
            {
                visible_by_category
                    .entry(WidgetCategory::Other)
                    .or_default()
                    .push(cmd.name().to_string());
            }
        }

        // Special-case spacers: dynamically named (spacer_1, spacer_2, etc.), not in templates
        for spacer in layout
            .windows
            .iter()
            .filter(|w| w.widget_type() == "spacer" && w.base().visible)
        {
            visible_by_category
                .entry(WidgetCategory::Other)
                .or_default()
                .push(spacer.name().to_string());
        }

        // Include custom windows (created via .addwindow) that aren't in templates
        // These have names like "custom-text-1", "custom-tabbedtext-2", etc.
        let all_templates: std::collections::HashSet<String> =
            Self::list_window_templates().into_iter().collect();
        for window in layout.windows.iter().filter(|w| w.base().visible) {
            let name = window.name().to_string();
            // Skip if already in templates or is essential window we're excluding
            if all_templates.contains(&name) {
                continue;
            }
            if exclude_essential && (name == "main" || name == "command_input") {
                continue;
            }
            // Skip spacers (already handled above) and command_input (handled above)
            if window.widget_type() == "spacer" || window.widget_type() == "command_input" {
                continue;
            }
            // Add custom window to appropriate category
            let category = WidgetCategory::from_widget_type(window.widget_type());
            let entry = visible_by_category.entry(category).or_default();
            if !entry.contains(&name) {
                entry.push(name);
            }
        }

        visible_by_category
    }

    /// Get list of visible windows in a layout
    pub fn list_visible_windows(layout: &crate::config::Layout) -> Vec<String> {
        layout
            .windows
            .iter()
            .filter(|w| w.base().visible)
            .map(|w| w.name().to_string())
            .collect()
    }
}
