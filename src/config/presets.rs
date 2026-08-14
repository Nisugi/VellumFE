//! Window and indicator template definitions.
//!
//! Contains the built-in window template catalog (`get_window_template`),
//! user-defined template stores persisted to TOML, and the category
//! groupings used by the add-window menus.

use super::*;
use crate::data::geometry::{Col, Height, Row, Width};

/// One condition-driven status icon state, mirroring `HandIconState`: while
/// its `when` holds, its icon/text/color override the template's static
/// defaults. First match wins across the `states` list. This is what lets a
/// status show one icon at rank 1 and another at rank 2 (e.g. an injury
/// threshold), or a different icon per game state generally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusIconState {
    pub when: super::Condition,
    /// GUI icon while the state holds (pool image / sheet cell /
    /// `IconRef::None` for no art). None = keep the resolved default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<crate::data::IconRef>,
    /// TUI text glyph while the state holds (the TUI renders no images).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Color override while the state holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

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
    /// Legacy text-glyph icon (TUI prefix / GUI fallback). Kept for
    /// back-compat; `icon_ref` takes precedence for GUI art.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Pickable ACTIVE GUI icon (pool image / sheet cell): shown when the
    /// indicator is active (game Y, or a matched condition). Resolved before
    /// the legacy `icon` string and the id-keyed skin/pictogram fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_ref: Option<crate::data::IconRef>,
    /// Pickable INACTIVE GUI icon: shown when the indicator is inactive (game
    /// N). None = show NO image while inactive (blank) — inactive art is
    /// opt-in, never a dimmed copy of the active icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive_icon_ref: Option<crate::data::IconRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_color: Option<String>,
    /// Condition-driven icon states (first match wins), overriding the static
    /// icon/colors above while a state holds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<StatusIconState>,
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


/// THE ordered catalog: every built-in window key with its game gating
/// (redesign Phase 6 — one source of truth; formerly a 67-entry parallel
/// name list plus a separate `template_game_type` match). Order is the
/// catalog order the Phase 0 golden fixture pins. User presets from the
/// template stores layer on top in `list_window_templates`.
pub const CATALOG: &[(&str, Option<GameType>)] = &[
    ("health", None),
    ("mana", None),
    ("stamina", None),
    ("spirit", None),
    ("concentration", Some(GameType::DR)),
    ("stance", None),
    ("progress_custom", None),
    ("dashboard", None),
    ("poisoned", None),
    ("bleeding", None),
    ("diseased", None),
    ("stunned", None),
    ("webbed", None),
    ("standing", None),
    ("kneeling", None),
    ("sitting", None),
    ("prone", None),
    ("hidden", None),
    ("invisible", None),
    ("joined", None),
    ("dead", None),
    ("main", None),
    ("thoughts", None),
    ("speech", None),
    ("bestiary", None),
    ("announcements", None),
    ("loot", None),
    ("death", None),
    ("logons", None),
    ("familiar", None),
    ("ambients", None),
    ("bounty", None),
    ("society", None),
    ("text_custom", None),
    ("chat", None),
    ("tabbedtext_custom", None),
    ("targets", None),
    ("creaturefield", Some(GameType::GS4)),
    ("players", None),
    ("items", None),
    ("entity_custom", None),
    ("roundtime", None),
    ("casttime", None),
    ("stuntime", None),
    ("pulse", Some(GameType::GS4)),
    ("countdown_custom", None),
    ("left", None),
    ("right", None),
    ("spell", None),
    ("buffs", None),
    ("debuffs", None),
    ("cooldowns", None),
    ("active_spells", None),
    ("alert_timers", None),
    ("active_effects_custom", None),
    ("inventory", None),
    ("reserve", Some(GameType::GS4)),
    ("room", None),
    ("spells", None),
    ("missingspells", None),
    ("containers", Some(GameType::GS4)),
    ("bestiaryview", Some(GameType::GS4)),
    ("compass", None),
    ("map", None),
    ("injuries", None),
    ("quickbar", None),
    ("hotkeybar", None),
    ("spacer", None),
    ("perception", Some(GameType::DR)),
    ("experience", Some(GameType::DR)),
    ("gs4_experience", Some(GameType::GS4)),
    ("encum", None),
    ("minivitals", Some(GameType::GS4)),
    ("betrayer", Some(GameType::GS4)),
    ("multiaccount", None),
];

impl Config {
    /// Get a window template by name
    /// Returns a WindowDef with default positioning that can be customized
    pub fn get_window_template(name: &str) -> Option<WindowDef> {
        // Create base defaults that all windows share
        let base_defaults = WindowBase {
            name: String::new(), // Will be overridden
            row: Row::new(0),
            col: Col::new(0),
            rows: Height::new(10),
            cols: Width::new(40),
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
            visibility: crate::config::WindowVisibility::Shown,
            binding: None,
            content_align: None,
            tts_speak: false,
            text_size: None,
            font_family: None,
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
                    rows: Height::new(37),
                    cols: Width::new(120),
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
                    rows: Height::new(10),
                    cols: Width::new(80),
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
                    rows: Height::new(20),
                    cols: Width::new(40),
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

            "reserve" => Some(WindowDef::Reserve {
                base: WindowBase {
                    name: "reserve".to_string(),
                    title: Some("Reserve".to_string()),
                    rows: Height::new(20),
                    cols: Width::new(40),
                    min_rows: Some(4),
                    ..base_defaults.clone()
                },
                data: InventoryWidgetData {
                    streams: vec!["reserve".to_string()],
                    buffer_size: 0, // No scrollback (content replaced each snapshot)
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "command_input" => Some(WindowDef::CommandInput {
                base: WindowBase {
                    name: "command_input".to_string(),
                    title: Some("Command Input".to_string()),
                    rows: Height::new(1),
                    cols: Width::new(120),
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
                    rows: Height::new(3),
                    cols: Width::new(120),
                    min_rows: Some(3),
                    max_rows: Some(3),
                    show_border: true,
                    show_title: false,
                    ..base_defaults.clone()
                },
                data: QuickbarWidgetData {},
            }),

            "hotkeybar" => Some(WindowDef::Hotkeybar {
                base: WindowBase {
                    name: "hotkeybar".to_string(),
                    title: Some("Actions".to_string()),
                    rows: Height::new(3),
                    cols: Width::new(60),
                    min_rows: Some(3),
                    max_rows: Some(3),
                    show_border: true,
                    show_title: false,
                    ..base_defaults.clone()
                },
                data: HotkeybarWidgetData {
                    bar: "default".to_string(),
                    orientation: "horizontal".to_string(),
                },
            }),

            "health" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "health".to_string(),
                    title: Some("Health".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(10),
                    cols: Width::new(40),
                    min_rows: Some(4),
                    min_cols: Some(20),
                    ..base_defaults.clone()
                },
                data: PerformanceWidgetData {
                    enabled: true,
                    show_fps: true,
                    show_render_times: true,
                    show_ui_times: true,
                    show_wrap_times: true,
                    show_net: true,
                    show_parse: true,
                    show_events: true,
                    show_cpu: true,
                    show_memory: true,
                    show_lines: true,
                    show_uptime: true,
                    show_spike_log: true,
                    show_per_window: true,
                    sparklines: true,
                },
            }),

            "mana" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "mana".to_string(),
                    title: Some("Mana".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(10),
                    cols: Width::new(40),
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
            "creaturefield" => Some(WindowDef::CreatureField {
                base: WindowBase {
                    name: "creaturefield".to_string(),
                    title: Some("Creature Field".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(16),
                    cols: Width::new(60),
                    min_rows: Some(8),
                    min_cols: Some(30),
                    ..base_defaults.clone()
                },
                data: CreatureFieldWidgetData::default(),
            }),
            "players" => Some(WindowDef::Players {
                base: WindowBase {
                    name: "players".to_string(),
                    title: Some("Players".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(10),
                    cols: Width::new(40),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(10),
                    cols: Width::new(40),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(10),
                    cols: Width::new(40),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(10),
                    min_rows: Some(1),
                    min_cols: Some(1),
                    ..base_defaults.clone()
                },
                data: DashboardWidgetData {
                    layout: default_dashboard_layout(),
                    spacing: default_dashboard_spacing(),
                    // Hide inactive statuses by default so the grid isn't a
                    // wall of dim icons; users can uncheck it in the dashboard
                    // editor. Matches the serde load default (config.rs).
                    hide_inactive: true,
                    indicators: Vec::new(),
                },
            }),

            "poisoned" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "poisoned".to_string(),
                    title: Some("Poisoned".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
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

            // Posture + presence indicators. The game reports these as
            // Icon{STANDING,KNEELING,SITTING,PRONE,HIDDEN,INVISIBLE,JOINED,DEAD}
            // (see core::state::StatusInfo). Shipping them as first-class
            // templates makes their art user-customizable (like the afflictions
            // above) and lets a single combined "posture" indicator drive all
            // four postures via `states` conditions. The GUI already carries a
            // vector pictogram for every one of these ids (status_icons.rs), so
            // they render as art regardless of the text `icon` glyph; the glyph
            // is a TUI-side fallback chosen to be present in the bundled fonts.
            "standing" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "standing".to_string(),
                    title: Some("Standing".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("St".to_string()),
                    indicator_id: Some("STANDING".to_string()),
                    inactive_color: None,
                    active_color: Some("#55b86c".to_string()),
                    default_status: None,
                    default_color: Some("#55b86c".to_string()),
                },
            }),
            "kneeling" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "kneeling".to_string(),
                    title: Some("Kneeling".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("Kn".to_string()),
                    indicator_id: Some("KNEELING".to_string()),
                    inactive_color: None,
                    active_color: Some("#c9a54d".to_string()),
                    default_status: None,
                    default_color: Some("#c9a54d".to_string()),
                },
            }),
            "sitting" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "sitting".to_string(),
                    title: Some("Sitting".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("Si".to_string()),
                    indicator_id: Some("SITTING".to_string()),
                    inactive_color: None,
                    active_color: Some("#c9a54d".to_string()),
                    default_status: None,
                    default_color: Some("#c9a54d".to_string()),
                },
            }),
            "prone" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "prone".to_string(),
                    title: Some("Prone".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("Pr".to_string()),
                    indicator_id: Some("PRONE".to_string()),
                    inactive_color: None,
                    active_color: Some("#d67d3e".to_string()),
                    default_status: None,
                    default_color: Some("#d67d3e".to_string()),
                },
            }),
            "hidden" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "hidden".to_string(),
                    title: Some("Hidden".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("".to_string()), // Nerdfont eye-slash
                    indicator_id: Some("HIDDEN".to_string()),
                    inactive_color: None,
                    active_color: Some("#7a7aa8".to_string()),
                    default_status: None,
                    default_color: Some("#7a7aa8".to_string()),
                },
            }),
            "invisible" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "invisible".to_string(),
                    title: Some("Invisible".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("".to_string()), // Nerdfont ghost
                    indicator_id: Some("INVISIBLE".to_string()),
                    inactive_color: None,
                    active_color: Some("#9a9ac0".to_string()),
                    default_status: None,
                    default_color: Some("#9a9ac0".to_string()),
                },
            }),
            "joined" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "joined".to_string(),
                    title: Some("Joined".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("".to_string()), // Nerdfont users/group
                    indicator_id: Some("JOINED".to_string()),
                    inactive_color: None,
                    active_color: Some("#5aa0d0".to_string()),
                    default_status: None,
                    default_color: Some("#5aa0d0".to_string()),
                },
            }),
            "dead" => Some(WindowDef::Indicator {
                base: WindowBase {
                    name: "dead".to_string(),
                    title: Some("Dead".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(2),
                    cols: Width::new(1),
                    min_rows: Some(2),
                    max_rows: Some(2),
                    min_cols: Some(1),
                    max_cols: Some(1),
                    show_border: false,
                    ..base_defaults.clone()
                },
                data: IndicatorWidgetData {
                    icon: Some("".to_string()), // Nerdfont skull
                    indicator_id: Some("DEAD".to_string()),
                    inactive_color: None,
                    active_color: Some("#cd4d4d".to_string()),
                    default_status: None,
                    default_color: Some("#cd4d4d".to_string()),
                },
            }),

            "spirit" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "spirit".to_string(),
                    title: Some("Spirit".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
                    show_border: true,
                    text_color: Some("#FF0000".to_string()), // Red
                    ..base_defaults.clone()
                },
                data: CountdownWidgetData {
                    id: Some("roundtime".to_string()),
                    label: None,
                    icon: Some(default_countdown_icon().chars().next().unwrap_or('█')),
                    color: None,
                    countdown_background_color: None,
                    show_when_zero: None,
                    count_past_zero: None,
                },
            }),

            "casttime" => Some(WindowDef::Countdown {
                base: WindowBase {
                    name: "casttime".to_string(),
                    title: Some("Cast".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
                    show_border: true,
                    text_color: Some("#00BFFF".to_string()), // Deep sky blue
                    ..base_defaults.clone()
                },
                data: CountdownWidgetData {
                    id: Some("casttime".to_string()),
                    label: None,
                    icon: Some(default_countdown_icon().chars().next().unwrap_or('█')),
                    color: None,
                    countdown_background_color: None,
                    show_when_zero: None,
                    count_past_zero: None,
                },
            }),

            // Extended-feed pulse clock (<pulse min max mana>): counts down
            // to the earliest arrival of the next game pulse. show_when_zero
            // defaults on so "Pulse 0" reads as "armed - pulse imminent"
            // during the min..max window instead of the widget blanking.
            "pulse" => Some(WindowDef::Countdown {
                base: WindowBase {
                    name: "pulse".to_string(),
                    title: Some("Pulse".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
                    show_border: true,
                    text_color: Some("#9370DB".to_string()), // Medium purple
                    ..base_defaults.clone()
                },
                data: CountdownWidgetData {
                    id: Some("pulse".to_string()),
                    label: None,
                    icon: Some(default_countdown_icon().chars().next().unwrap_or('█')),
                    color: None,
                    countdown_background_color: None,
                    show_when_zero: Some(true),
                    // 0 = earliest possible arrival; negative = seconds deep
                    // into the min..max window (bottoms out ~-29).
                    count_past_zero: Some(true),
                },
            }),

            "stuntime" => Some(WindowDef::Countdown {
                base: WindowBase {
                    name: "stuntime".to_string(),
                    title: Some("Stun".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
                    show_border: true,
                    text_color: Some("#FFFF00".to_string()), // Yellow
                    ..base_defaults.clone()
                },
                data: CountdownWidgetData {
                    id: Some("stuntime".to_string()),
                    label: None,
                    icon: Some(default_countdown_icon().chars().next().unwrap_or('█')),
                    color: None,
                    countdown_background_color: None,
                    show_when_zero: None,
                    count_past_zero: None,
                },
            }),

            "countdown_custom" => Some(WindowDef::Countdown {
                base: WindowBase {
                    name: String::new(), // Auto-generated by WindowEditor
                    title: Some("Custom".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: CountdownWidgetData {
                    id: None,
                    label: None,
                    icon: Some(default_countdown_icon().chars().next().unwrap_or('█')),
                    color: None,
                    countdown_background_color: None,
                    show_when_zero: None,
                    count_past_zero: None,
                },
            }),

            "map" => Some(WindowDef::Map {
                base: WindowBase {
                    name: "map".to_string(),
                    title: Some("Map".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(12),
                    cols: Width::new(30),
                    show_border: true,
                    min_rows: Some(5),
                    min_cols: Some(10),
                    ..base_defaults.clone()
                },
                data: MapWidgetData::default(),
            }),

            "compass" => Some(WindowDef::Compass {
                base: WindowBase {
                    name: "compass".to_string(),
                    title: Some("Compass".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(5), // 3 for compass grid + 2 for border
                    cols: Width::new(9), // 7 for compass grid + 2 for border
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(8),  // 6 for injury doll + 2 for border
                    cols: Width::new(10), // 8 for injury doll (5+3 for labels) + 2 for border
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
                    rows: Height::new(10),
                    cols: Width::new(30),
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
                    rows: Height::new(10),
                    cols: Width::new(30),
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
                    rows: Height::new(10),
                    cols: Width::new(30),
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
                    rows: Height::new(10),
                    cols: Width::new(30),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: ActiveEffectsWidgetData {
                    category: "ActiveSpells".to_string(),
                },
            }),

            // Client-authored countdown bars started by alerts. An ordinary
            // effects window on a client-owned category: same bars, same
            // ticking, no new renderer.
            "alert_timers" => Some(WindowDef::ActiveEffects {
                base: WindowBase {
                    name: "alert_timers".to_string(),
                    title: Some("Timers".to_string()),
                    rows: Height::new(10),
                    cols: Width::new(30),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: ActiveEffectsWidgetData {
                    category: crate::core::alert_timers::TIMERS_CATEGORY.to_string(),
                },
            }),

            "active_effects_custom" => Some(WindowDef::ActiveEffects {
                base: WindowBase {
                    name: String::new(), // Auto-generated by WindowEditor
                    title: Some("Custom".to_string()),
                    rows: Height::new(10),
                    cols: Width::new(30),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: HandWidgetData {
                    icon: Some("L:".to_string()),
                    icon_color: None,
                    hand_text_color: None,
                    states: Vec::new(),
                },
            }),

            "right" => Some(WindowDef::Hand {
                base: WindowBase {
                    name: "right".to_string(),
                    title: Some("Right Hand".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: HandWidgetData {
                    icon: Some("R:".to_string()),
                    icon_color: None,
                    hand_text_color: None,
                    states: Vec::new(),
                },
            }),

            "spell" => Some(WindowDef::Hand {
                base: WindowBase {
                    name: "spell".to_string(),
                    title: Some("Spell".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3),
                    cols: Width::new(20),
                    show_border: true,
                    min_rows: Some(3),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: HandWidgetData {
                    icon: Some("S:".to_string()),
                    icon_color: None,
                    hand_text_color: None,
                    states: Vec::new(),
                },
            }),

            // Text window templates for common streams
            "thoughts" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "thoughts".to_string(),
                    title: Some("Thoughts".to_string()),
                    rows: Height::new(10),
                    cols: Width::new(40),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["thoughts".to_string()],
                    buffer_size: 10000,
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
                    rows: Height::new(10),
                    cols: Width::new(40),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["speech".to_string()],
                    buffer_size: 10000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "bestiary" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "bestiary".to_string(),
                    title: Some("Bestiary".to_string()),
                    rows: Height::new(24),
                    cols: Width::new(69),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    // .bestiary output; wordwrap off keeps the 65-col boxes
                    // and tables aligned (the window scrolls horizontally
                    // when narrower).
                    streams: vec!["bestiary".to_string()],
                    buffer_size: 10000,
                    wordwrap: false,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }),

            "announcements" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "announcements".to_string(),
                    title: Some("Announcements".to_string()),
                    rows: Height::new(10),
                    cols: Width::new(50),
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
                    rows: Height::new(10),
                    cols: Width::new(40),
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
                    rows: Height::new(10),
                    cols: Width::new(40),
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
                    rows: Height::new(10),
                    cols: Width::new(40),
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
                    rows: Height::new(10),
                    cols: Width::new(40),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["familiar".to_string()],
                    buffer_size: 10000,
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
                    rows: Height::new(10),
                    cols: Width::new(40),
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
                    rows: Height::new(15),
                    cols: Width::new(50),
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
                    rows: Height::new(15),
                    cols: Width::new(50),
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
                    rows: Height::new(10),
                    cols: Width::new(40),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["custom".to_string()],
                    buffer_size: 10000,
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
                    rows: Height::new(20),
                    cols: Width::new(40),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: SpellsWidgetData {},
            }),

            "chat" => Some(WindowDef::TabbedText {
                base: WindowBase {
                    name: "chat".to_string(),
                    title: Some("Chat".to_string()),
                    rows: Height::new(10),
                    cols: Width::new(60),
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
                    rows: Height::new(10),
                    cols: Width::new(60),
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
                    rows: Height::new(2),
                    cols: Width::new(2),
                    show_border: false, // Spacers never show borders
                    show_title: false, // Spacers never show titles
                    transparent_background: false, // Respects theme background color
                    ..base_defaults
                },
                data: SpacerWidgetData {},
            }),

            "multiaccount" => Some(WindowDef::MultiAccount {
            base: WindowBase {
                name: "multiaccount".to_string(),
                title: Some("Characters".to_string()),
                row: Row::new(0),
                col: Col::new(0),
                rows: Height::new(12),
                cols: Width::new(46),
                min_rows: Some(6),
                min_cols: Some(24),
                ..base_defaults.clone()
            },
            data: MultiAccountWidgetData::default(),
        }),
        "missingspells" => Some(WindowDef::MissingSpells {
                base: WindowBase {
                    name: "missingspells".to_string(),
                    title: Some("Missing Spells".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(8),
                    cols: Width::new(28),
                    min_rows: Some(3),
                    min_cols: Some(14),
                    ..base_defaults.clone()
                },
                data: MissingSpellsWidgetData {},
            }),
            // Managed-inventory container tree (extended feed, .invsync)
            "containers" => Some(WindowDef::Containers {
                base: WindowBase {
                    name: "containers".to_string(),
                    title: Some("Containers".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(20),
                    cols: Width::new(40),
                    min_rows: Some(5),
                    min_cols: Some(20),
                    ..base_defaults.clone()
                },
                data: ContainersWidgetData {},
            }),
            // Bestiary browser (GUI app-style pages over the bundled codex)
            "bestiaryview" => Some(WindowDef::BestiaryView {
                base: WindowBase {
                    name: "bestiaryview".to_string(),
                    title: Some("Bestiary Browser".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(24),
                    cols: Width::new(60),
                    min_rows: Some(8),
                    min_cols: Some(30),
                    ..base_defaults.clone()
                },
                data: BestiaryViewWidgetData {},
            }),
            "perception" => Some(WindowDef::Perception {
                base: WindowBase {
                    name: "perception".to_string(),
                    title: Some("Perceptions".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(20),
                    cols: Width::new(40),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(20),
                    cols: Width::new(35),
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(5),           // 3 default content rows (level, mind, exp) + 2 borders
                    cols: Width::new(30),
                    min_rows: Some(3), // 1 content row + borders (fields are toggleable)
                    max_rows: Some(7), // 5 content rows (level, mind, exp, total, ascension) + borders
                    min_cols: Some(20),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: GS4ExperienceWidgetData {
                    align: "center".to_string(),
                    show_level: true,
                    show_exp_bar: true,
                    show_mind_bar: true,
                    show_total_exp: false,
                    show_ascension_exp: false,
                    mind_bar_color: None,
                    exp_bar_color: None,
                },
            }),

            "encum" => Some(WindowDef::Encumbrance {
                base: WindowBase {
                    name: "encum".to_string(),
                    title: Some("Encumbrance".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(4),           // 1 bar + 1 label + 2 borders = 4 total
                    cols: Width::new(25),
                    min_rows: Some(3), // 1 content row + borders (bar/label are toggleable)
                    max_rows: Some(4), // Maximum with borders + label
                    min_cols: Some(15),
                    show_border: true,
                    ..base_defaults.clone()
                },
                data: EncumbranceWidgetData {
                    align: "left".to_string(),
                    show_label: true,
                    show_bar: true,
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
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(3), // 1 content row + 2 borders = 3 total
                    cols: Width::new(80), // Wide to fit 4 bars
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
                    depleted_color: None,
                    bar_order: default_minivitals_bar_order(),
                },
            }),

            "betrayer" => Some(WindowDef::Betrayer {
                base: WindowBase {
                    name: "betrayer".to_string(),
                    title: Some("Betrayer".to_string()),
                    row: Row::new(0),
                    col: Col::new(0),
                    rows: Height::new(4), // 1 bar + 1 item + 2 borders
                    cols: Width::new(30),
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
                base.rows = Height::new(1);
                base.cols = Width::new(1);
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
        // Built-ins from THE catalog table, then the user preset stores
        // (the never-die layer) appended with case-insensitive dedup.
        let mut templates: Vec<String> =
            CATALOG.iter().map(|(name, _)| name.to_string()).collect();

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
        // Phase 6: gating lives in THE catalog table (user presets and
        // unknown names gate to None, available for both games).
        CATALOG
            .iter()
            .find(|(key, _)| *key == name)
            .and_then(|(_, game)| *game)
    }

    // Phase 6: the dialog/stream id-maps (dialog_id_to_template,
    // id_has_widget_template, stream_id_to_template) moved into
    // core::view_resolver as the dedicated-view tables.

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
                    icon_ref: None,
                    inactive_icon_ref: None,
                    inactive_color: data.inactive_color,
                    active_color: data.active_color,
                    default_status: data.default_status,
                    default_color: data.default_color,
                    states: Vec::new(),
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
        write_atomic(&path, contents)
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
        write_atomic(&path, contents)
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
                            .any(|w| w.name() == *name && w.base().visibility.is_shown())
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
        Self::get_layout_templates_by_category(layout, exclude_essential, false)
    }

    /// Like `get_visible_templates_by_category`, but with `include_hidden`
    /// the filter is presence-in-layout rather than visibility — used by the
    /// edit-window picker so hidden windows stay reachable.
    pub fn get_layout_templates_by_category(
        layout: &crate::config::Layout,
        exclude_essential: bool,
        include_hidden: bool,
    ) -> HashMap<WidgetCategory, Vec<String>> {
        let all_by_category = Self::get_templates_by_category();
        let included = |w: &crate::config::WindowDef| include_hidden || w.base().visibility.is_shown();

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
                        // Include only windows present (and, unless
                        // include_hidden, visible) in the layout
                        layout
                            .windows
                            .iter()
                            .any(|w| w.name() == *name && included(w))
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
                .find(|w| w.widget_type() == "command_input" && included(w))
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
            .filter(|w| w.widget_type() == "spacer" && included(w))
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
        for window in layout.windows.iter().filter(|w| included(w)) {
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
            .filter(|w| w.base().visibility.is_shown())
            .map(|w| w.name().to_string())
            .collect()
    }
}

#[cfg(test)]
mod dialog_template_mapping_tests {
    use super::*;

    // effect_dialog_ids_resolve_to_their_widget_templates moved to
    // core::view_resolver (Phase 6: the mapping lives there now).

    #[test]
    fn standard_status_indicators_all_have_templates() {
        // Every self-status the game reports (core::state::StatusInfo) must be a
        // first-class indicator template so its icon is user-customizable and a
        // combined indicator can drive it. Postures were previously absent,
        // leaving them as uncustomizable runtime-only dashboard cells.
        for name in [
            "poisoned", "bleeding", "diseased", "stunned", "webbed", // afflictions
            "standing", "kneeling", "sitting", "prone", // postures
            "hidden", "invisible", "joined", "dead", // presence
        ] {
            assert!(
                Config::get_window_template(name).is_some(),
                "indicator template '{name}' must exist"
            );
        }

        // And they surface in the customizable indicator-template list.
        let ids: std::collections::HashSet<String> = Config::list_indicator_templates()
            .into_iter()
            .map(|t| t.id.to_ascii_uppercase())
            .collect();
        for id in ["STANDING", "KNEELING", "SITTING", "PRONE", "DEAD", "HIDDEN"] {
            assert!(ids.contains(id), "indicator template list missing '{id}'");
        }
    }
}
