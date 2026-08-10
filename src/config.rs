//! Configuration loader/writer plus strongly typed settings structures.
//!
//! This module deserializes every TOML blob we ship (config, highlights,
//! keybinds, colors, layouts, etc.), exposes helpers for resolving per-character
//! overrides, and persists edits that come from the UI.

use anyhow::{Context, Result};
use crate::data::input::{KeyCode, KeyModifiers};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub mod menu_keybind_validator;
pub mod wrayth_import;
mod colors;
mod conditions;
mod highlights;
mod hotbars;
mod io;
pub mod room_images;
mod macros;
mod paths;
pub mod pool;
pub mod profiles;
pub mod skins;
mod defaults_refresh;
mod keybinds;
mod layout;
pub mod registry;
mod settings;
mod sparse;
mod presets;
mod widgets;
mod window_registry;
mod window_def;

pub use colors::{
    ColorConfig, HarmonyRecipe, PaletteColor, PresetColor, PromptColor, SpellColorRange,
    SpellColorStyle, UiColors,
};
pub use highlights::{
    highlight_web_fields, EventAction, EventPattern, HighlightPattern, RedirectMode,
};
pub use conditions::{
    Cmp, Condition, EffectCategory, HandSlot, NameMatch, VitalKind, VitalUnit, INJURY_AREAS,
};
pub use hotbars::{
    GradientDir, HotbarButton, HotbarButtonState, HotbarCountdownSource, HotbarDef, HotbarIcon,
    HotbarStyle, HotbarsConfig, IconMode,
};
pub use keybinds::{
    keyboard_dead_action_reason, parse_key_string, reserved_combo_conflict,
    touch_wheel_action_catalog, validate_wheel_spans, AppKeybinds,
    ControllerBindKey, KeyAction, KeyBindAction, MacroAction, MenuKeybindField, MenuKeybinds,
    RumbleConfig, RumblePattern, TuningConfig, WheelMeta, WheelSlice, WheelSpanIssue,
    CONTROLLER_BUTTON_ORDER, TOUCH_WHEEL_CLIENT_ACTIONS, WHEEL_MIN_SPAN_DEG,
};
pub use layout::{ContentAlign, Layout, LayoutConfig};
pub use macros::{MacroButton, MacroGroup, MacroOption, MacrosConfig};
pub use paths::{is_valid_layout_name, write_atomic, DialogPosition, SavedDialogPositions};
pub use window_registry::{RegistryBinding, WindowRegistry};
#[cfg(test)]
pub use paths::VELLUM_FE_DIR_TEST_LOCK;
pub use settings::{
    ConnectionConfig, FocusConfig, Go2Config, HighlightsConfig, LoggingConfig, MapConfig,
    RoomImagesSettings, SorterConfig, SorterRule, SoundConfig, StreamRoute, StreamsConfig, TargetListConfig,
    TtsConfig, TtsSubstitution, UiConfig, WebConfig,
};
pub use presets::{IndicatorTemplateEntry, IndicatorTemplateStore, StatusIconState};
pub use widgets::{
    apply_compiled_text_replacements, compile_text_replacements, default_minivitals_bar_order,
    DEFAULT_INJURY_PALETTE,
    ActiveEffectsWidgetData, BetrayerWidgetData, BorderSides, CommandInputWidgetData,
    CompassWidgetData, CompiledTextReplacement, ContainerWidgetData, CountdownWidgetData,
    DashboardIndicatorDef, DashboardLayout, DashboardWidgetData, DialogPanelWidgetData,
    EncumbranceWidgetData,
    ExperienceWidgetData,
    GS4ExperienceWidgetData, HandIconState, HandWidgetData, HotkeybarWidgetData, IndicatorWidgetData,
    InjuryDollWidgetData, InventoryWidgetData, ItemsWidgetData, MapWidgetData,
    MiniVitalsWidgetData, MissingSpellsWidgetData, PerceptionWidgetData,
    PerformanceWidgetData, PlayersWidgetData, ProgressWidgetData, QuickbarDefinition,
    QuickbarEntryConfig, QuickbarWidgetData, QuickbarsConfig, RoomWidgetData, SortDirection,
    SpacerWidgetData, SpellsWidgetData, TabbedTextTab, TabbedTextWidgetData, TargetsWidgetData,
    TextReplacement, TextWidgetData, WebUiWidgetData, WindowBase, WindowBinding, WindowVisibility,
};
pub use window_def::WindowDef;

// Embed default configuration files at compile time
// Files are under defaults/globals/ to mirror the user's ~/.vellum-fe/global/ structure
const DEFAULT_CONFIG: &str = include_str!("../defaults/globals/config.toml");
const DEFAULT_COLORS: &str = include_str!("../defaults/globals/colors.toml");
const DEFAULT_HIGHLIGHTS: &str = include_str!("../defaults/globals/highlights.toml");
const DEFAULT_KEYBINDS: &str = include_str!("../defaults/globals/keybinds.toml");
const DEFAULT_CONTROLLER: &str = include_str!("../defaults/globals/controller.toml");
const DEFAULT_HOTBARS: &str = include_str!("../defaults/globals/hotbars.toml");
const DEFAULT_MACROS: &str = include_str!("../defaults/globals/macros.toml");
const DEFAULT_CMDLIST: &str = include_str!("../defaults/globals/cmdlist1.xml");
const DEFAULT_SPELL_ABBREVS: &str = include_str!("../defaults/globals/spell_abbrev.toml");
const DEFAULT_LAYOUT_TEMPLATE: &str = include_str!("../defaults/globals/templates/layout_template.toml");
const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../defaults/globals/templates/config_template.toml");

// Embed entire directories - automatically includes all files
static LAYOUTS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/defaults/globals/layouts");
static SOUNDS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/defaults/globals/sounds");

// Keep embedded default layout for fallback
const LAYOUT_DEFAULT: &str = include_str!("../defaults/globals/layouts/layout.toml");

/// Widget category for organizing windows in menus
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WidgetCategory {
    ActiveEffects,
    Character,
    Container,
    Countdown,
    Dialog,
    Entity,
    Hand,
    Hotbars,
    Navigation,
    Other,
    ProgressBar,
    Status,
    TextWindow,
}

impl WidgetCategory {
    /// All categories in a stable display order.
    pub const ALL: [WidgetCategory; 13] = [
        WidgetCategory::Status,
        WidgetCategory::ProgressBar,
        WidgetCategory::Countdown,
        WidgetCategory::ActiveEffects,
        WidgetCategory::Entity,
        WidgetCategory::Hand,
        WidgetCategory::TextWindow,
        WidgetCategory::Character,
        WidgetCategory::Navigation,
        WidgetCategory::Hotbars,
        WidgetCategory::Container,
        WidgetCategory::Dialog,
        WidgetCategory::Other,
    ];

    pub fn display_name(&self) -> &str {
        match self {
            Self::ActiveEffects => "Active Effects",
            Self::Character => "Character",
            Self::Container => "Containers",
            Self::Countdown => "Countdowns",
            Self::Dialog => "Dialogs",
            Self::Entity => "Entities",
            Self::Hand => "Hands",
            Self::Hotbars => "Hotbars",
            Self::Navigation => "Navigation",
            Self::Other => "Other",
            Self::ProgressBar => "Progress Bars",
            Self::Status => "Status",
            Self::TextWindow => "Text Windows",
        }
    }

    /// Parse a category from its variant name (the Debug form used in
    /// `__SUBMENU_ADD__*` menu commands).
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "ActiveEffects" => Some(Self::ActiveEffects),
            "Character" => Some(Self::Character),
            "Container" => Some(Self::Container),
            "Countdown" => Some(Self::Countdown),
            "Dialog" => Some(Self::Dialog),
            "Entity" => Some(Self::Entity),
            "Hand" => Some(Self::Hand),
            "Hotbars" => Some(Self::Hotbars),
            "Navigation" => Some(Self::Navigation),
            "Other" => Some(Self::Other),
            "ProgressBar" => Some(Self::ProgressBar),
            "Status" => Some(Self::Status),
            "TextWindow" => Some(Self::TextWindow),
            _ => None,
        }
    }

    pub fn from_widget_type(widget_type: &str) -> Self {
        match widget_type {
            "countdown" => Self::Countdown,
            "hand" => Self::Hand,
            "active_effects" => Self::ActiveEffects,
            "indicator" | "dashboard" => Self::Status,
            "progress" | "minivitals" => Self::ProgressBar,
            "text" | "tabbedtext" => Self::TextWindow,
            "targets" | "players" | "items" => Self::Entity,
            "inventory" | "spells" | "missingspells" | "injury_doll" | "experience"
            | "gs4_experience" | "encum" | "reserve" | "perception" => Self::Character,
            "room" | "compass" | "map" => Self::Navigation,
            "quickbar" | "hotkeybar" => Self::Hotbars,
            "container" => Self::Container,
            "dialogpanel" | "betrayer" => Self::Dialog,
            _ => Self::Other,
        }
    }
}

/// Game type for filtering game-specific features and templates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameType {
    /// GemStone IV (prime, platinum, shattered, test)
    GS4,
    /// DragonRealms (dr, drplatinum, drfallen, drtest)
    DR,
}

impl GameType {
    /// Determine game type from game string (e.g., "prime", "dr", "drplatinum")
    /// Defaults to GS4 when game string is None or unknown (most common case)
    pub fn from_game_string(game: Option<&str>) -> Option<Self> {
        match game {
            Some(g) if g.to_ascii_lowercase().starts_with("dr") => Some(GameType::DR),
            // Default to GS4 for all other cases (including None)
            // This ensures GS4-specific templates show when connecting via Lich without --game
            _ => Some(GameType::GS4),
        }
    }
}

/// Color rendering mode for terminal compatibility
///
/// VellumFE supports two color modes:
/// - `Direct` (default): Sends RGB values using true color escape sequences (ESC[38;2;R;G;Bm)
/// - `Slot`: Sends 256-color palette indices (ESC[38;5;Nm), optionally customizable via `.setpalette`
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    /// True color RGB (24-bit) - ESC[38;2;R;G;Bm
    /// Requires modern terminal with true color support.
    #[default]
    Direct,
    /// 256-color with custom palette - ESC[38;5;Nm + OSC4
    /// Use with .setpalette to reprogram terminal palette slots.
    /// Requires terminal with OSC4 support (most modern terminals).
    Slot,
    /// 256-color with standard palette - ESC[38;5;Nm only
    /// Uses default xterm 256-color palette, finds closest match.
    /// For terminals without true color OR OSC4 support.
    Indexed,
}

impl std::fmt::Display for ColorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColorMode::Direct => write!(f, "direct"),
            ColorMode::Slot => write!(f, "slot"),
            ColorMode::Indexed => write!(f, "indexed"),
        }
    }
}

/// Position of timestamps relative to line content
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TimestampPosition {
    /// Append timestamp at end of line (default, e.g., "text [7:08 AM]")
    #[default]
    End,
    /// Prepend timestamp at start of line (e.g., "[7:08 AM] text")
    Start,
}

impl std::fmt::Display for TimestampPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimestampPosition::End => write!(f, "end"),
            TimestampPosition::Start => write!(f, "start"),
        }
    }
}

/// Top-level configuration object aggregated from multiple TOML files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub connection: ConnectionConfig,
    pub ui: UiConfig,
    #[serde(skip)] // Loaded from separate highlights.toml file
    pub highlights: HashMap<String, HighlightPattern>,
    #[serde(skip)] // Loaded from separate keybinds.toml file
    pub keybinds: HashMap<String, KeyBindAction>,
    // Loaded from [controller] of controller.toml. Keys are canonical
    // controller bind keys: a bare button (`south`) or a composite modifier
    // combo (`l2+dpad_down`). Buttons declared `controller_modifier` form the
    // held-modifier set that composite keys resolve against. (Replaced the
    // old split base/[controller_shift] banks; legacy configs auto-migrate.)
    #[serde(skip)]
    pub controller_binds: HashMap<String, KeyBindAction>,
    #[serde(skip)] // Loaded from [[controller_wheel]] (default radial wheel)
    pub controller_wheel: Vec<WheelSlice>,
    #[serde(skip)] // Loaded from [controller_wheels.<name>] (named radial wheels)
    pub controller_wheels: HashMap<String, Vec<WheelSlice>>,
    /// The phone's touch radial wheel (long-press navigation). Loaded from
    /// touch_wheel.toml (per-character, roams); shipped to the phone as the
    /// named "touch" wheel and editable from both frontends. Slices carry a
    /// `client` action (open a window, focus input) or a game `command`.
    #[serde(skip)]
    pub touch_wheel: Vec<WheelSlice>,
    #[serde(skip)] // Loaded from [controller_wheels_meta.<name>] (per-wheel button/stick)
    pub controller_wheels_meta: HashMap<String, WheelMeta>,
    #[serde(skip)] // Loaded from [controller_overlay] (curated HUD legend entries)
    pub controller_overlay: Vec<String>,
    #[serde(skip)] // Loaded from [controller_rumble] (haptic event map)
    pub controller_rumble: RumbleConfig,
    #[serde(skip)] // Loaded from [controller_tuning] (input-feel tuning)
    pub controller_tuning: TuningConfig,
    #[serde(skip)] // Loaded from separate hotbars.toml file
    pub hotbars: HotbarsConfig,
    #[serde(skip)] // Loaded from [app] section of keybinds.toml
    pub app_keybinds: AppKeybinds,
    #[serde(default)]
    pub sound: SoundConfig,
    #[serde(default)]
    pub tts: TtsConfig,
    #[serde(default)]
    pub target_list: TargetListConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub event_patterns: HashMap<String, EventPattern>,
    #[serde(skip)] // Don't serialize/deserialize this - it's set at runtime
    pub character: Option<String>, // Character name for character-specific saving
    #[serde(skip)] // Loaded from separate colors.toml file (includes color_palette)
    pub colors: ColorConfig, // All color configuration (presets, prompt_colors, ui colors, spell colors, color_palette)
    #[serde(default)] // Use defaults for menu keybinds
    pub menu_keybinds: MenuKeybinds, // Keybinds for menu system (browsers, forms, editors)
    #[serde(default = "default_theme_name")] // Default to "dark" theme
    pub active_theme: String, // Currently active theme name
    // Empty string reads as None: a profile clearing a skin the global layer
    // sets must be able to STATE "no skin" (TOML has no null), or the sparse
    // save writes nothing and the next load re-inherits the global value.
    #[serde(default, deserialize_with = "empty_string_as_none", skip_serializing_if = "Option::is_none")]
    pub active_skin: Option<String>, // Active GUI skin (dir name under ~/.vellum-fe/global/skins/); None = plain theme colors. In the GUI this mirrors ui_settings.active_skin in the layout file (web doll + non-GUI frontends read it here)
    #[serde(default, deserialize_with = "empty_string_as_none", skip_serializing_if = "Option::is_none")]
    pub doll_image: Option<String>, // Injury doll image override (pool-relative, "dolls/x.png"); mirrors ui_settings.doll_image like active_skin (web doll endpoint reads it here)
    #[serde(default)] // Use defaults for stream routing
    pub streams: StreamsConfig, // Stream routing configuration (drop list, fallback)
    #[serde(default)] // `[sorter]` — categorized container looks (.sorter)
    pub sorter: SorterConfig,
    #[serde(default)] // `[room_images]` — room art by uid (.roomimages)
    pub room_images: RoomImagesSettings,
    #[serde(default, rename = "highlights")] // [highlights] section in config.toml
    pub highlight_settings: HighlightsConfig, // Highlight system toggles (sounds, replace, redirect, coloring)
    #[serde(default)]
    pub quickbars: QuickbarsConfig, // Custom quickbar definitions and defaults
    #[serde(default)]
    pub web: WebConfig, // Embedded web server for the mobile web frontend
    #[serde(default)]
    pub map: MapConfig, // Mapdb discovery for the mini map / map explorer
    #[serde(default)]
    pub go2: Go2Config, // Native travel: saved targets, travel options
    #[serde(skip)] // Merged view of macros.toml + macros-local.toml
    pub macros: MacrosConfig, // Macro buttons for the web frontend
    #[serde(skip)] // Phone-edited overlay, persisted to macros-local.toml
    pub macros_local: MacrosConfig,
}


/// Deserialize an optional string, treating "" as None. The sparse save
/// writes an empty string to record a deliberately cleared Option, since
/// TOML cannot express null and an omitted key means "inherit".
fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Ok(Option::<String>::deserialize(deserializer)?.filter(|s| !s.is_empty()))
}

fn default_title_position() -> String {
    "top-left".to_string()
}

fn default_focus_types() -> Vec<String> {
    vec!["text".to_string(), "tabbedtext".to_string()]
}

fn default_focus_exclude() -> Vec<String> {
    // Exclude all non-text widget types from focus by default
    vec![
        "quickbar".to_string(),
        "hotkeybar".to_string(),
        "targets".to_string(),
        "players".to_string(),
        "items".to_string(),
        "inventory".to_string(),
        "reserve".to_string(),
        "spells".to_string(),
        "progress".to_string(),
        "countdown".to_string(),
        "compass".to_string(),
        "indicator".to_string(),
        "room".to_string(),
        "dashboard".to_string(),
        "injury_doll".to_string(),
        "hand".to_string(),
        "active_effects".to_string(),
        "spacer".to_string(),
        "performance".to_string(),
        "perception".to_string(),
        "container".to_string(),
        "experience".to_string(),
        "gs4_experience".to_string(),
        "encum".to_string(),
        "minivitals".to_string(),
        "betrayer".to_string(),
    ]
}

fn default_betrayer_active_color() -> Option<String> {
    Some("#ff4040".to_string())
}

fn default_dashboard_layout() -> String {
    "horizontal".to_string()
}

fn default_dashboard_spacing() -> u16 {
    1
}

fn default_dashboard_hide_inactive() -> bool {
    // Hide inactive statuses by default so a dashboard reads as "what's wrong
    // right now" rather than a wall of dim icons. Applies to new dashboards and
    // to any saved layout that omits the field. Users can uncheck it per
    // dashboard in the editor.
    true
}

pub(crate) fn default_target_entity_id() -> String {
    "targetcount".to_string()
}

pub(crate) fn default_player_entity_id() -> String {
    "playercount".to_string()
}

fn default_items_entity_id() -> String {
    "items".to_string()
}

fn default_true() -> bool {
    true
}

fn default_rows() -> crate::data::geometry::Height {
    crate::data::geometry::Height::new(1)
}

fn default_cols() -> crate::data::geometry::Width {
    crate::data::geometry::Width::new(1)
}

fn default_show_border() -> bool {
    true
}

fn default_show_title() -> bool {
    true
}

fn default_transparent_background() -> bool {
    false
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8000
}

fn default_buffer_size() -> usize {
    10_000
}

fn default_command_echo_color() -> String {
    "#ffffff".to_string()
}

fn default_system_message_color() -> String {
    "#8fbc8f".to_string() // muted green; full #00ff00 reads as shouting
}

fn default_border_color_default() -> String {
    "#00ffff".to_string() // cyan
}

fn default_focused_border_color() -> String {
    "#ffff00".to_string() // yellow
}

fn default_text_color_default() -> String {
    "#ffffff".to_string() // white
}

fn default_border_style() -> String {
    "single".to_string()
}

fn default_countdown_icon() -> String {
    "\u{f0c8}".to_string() // Nerd Font square icon
}

fn default_background_color() -> String {
    "-".to_string() // transparent/no background
}


fn default_selection_enabled() -> bool {
    true
}

fn default_selection_respect_window_boundaries() -> bool {
    true
}

fn default_selection_auto_copy() -> bool {
    true
}

fn default_selection_bg_color() -> String {
    "#4a4a4a".to_string()
}

fn default_textarea_background() -> String {
    "-".to_string() // No background color (terminal default)
}

fn default_drag_modifier_key() -> String {
    "ctrl".to_string()
}

fn default_min_command_length() -> usize {
    3
}

fn default_command_echo() -> bool {
    true
}

fn default_perf_stats_x() -> u16 {
    0 // Calculated dynamically: terminal_width - 35
}

fn default_perf_stats_y() -> u16 {
    0
}

fn default_perf_stats_width() -> u16 {
    35
}

fn default_perf_stats_height() -> u16 {
    23
}

fn default_performance_stats_enabled() -> bool {
    false // Start disabled by default
}


// default_command_input* functions removed - command_input is now in windows array

fn default_false() -> bool {
    false
}

fn default_theme_name() -> String {
    "dark".to_string()
}

#[cfg(test)]
mod spacer_tests;
