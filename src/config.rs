use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use crossterm::event::{KeyCode, KeyModifiers};

// Embed default configuration files at compile time
const DEFAULT_CONFIG: &str = include_str!("../defaults/config.toml");
const DEFAULT_LAYOUT: &str = include_str!("../defaults/layout.toml");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub connection: ConnectionConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub presets: HashMap<String, PresetColor>,
    #[serde(default)]
    pub highlights: HashMap<String, HighlightPattern>,
    #[serde(default)]
    pub keybinds: HashMap<String, KeyBindAction>,
    #[serde(default)]
    pub spell_colors: Vec<SpellColorRange>,
    #[serde(default)]
    pub sound: SoundConfig,
    #[serde(skip)]  // Don't serialize/deserialize this - it's set at runtime
    pub character: Option<String>,  // Character name for character-specific saving
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetColor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptColor {
    pub character: String, // The character to match (e.g., "R", "S", "H", ">")
    pub color: String,     // Hex color
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpellColorRange {
    pub spells: Vec<u32>,  // List of spell IDs (e.g., [101, 107, 120, 140, 150])
    pub color: String,     // Hex color (e.g., "#00ffff")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowDef {
    pub name: String,
    #[serde(default = "default_widget_type")]
    pub widget_type: String,  // "text", "indicator", "progress", "countdown", "injury"
    pub streams: Vec<String>,
    // Explicit positioning and sizing - each window owns its row/col dimensions
    #[serde(default)]
    pub row: u16,         // Starting row position (0-based)
    #[serde(default)]
    pub col: u16,         // Starting column position (0-based)
    #[serde(default = "default_rows")]
    pub rows: u16,        // Height in rows (this window owns these rows)
    #[serde(default = "default_cols")]
    pub cols: u16,        // Width in columns (this window owns these columns)
    // Buffer and display options
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default = "default_show_border")]
    pub show_border: bool,
    #[serde(default)]
    pub border_style: Option<String>,  // "single", "double", "rounded", "thick"
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default)]
    pub border_sides: Option<Vec<String>>,  // ["top", "bottom", "left", "right"] - None means all
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content_align: Option<String>,  // "top-left", "top-right", "bottom-left", "bottom-right", "center" - alignment of content within widget area
    #[serde(default)]
    pub background_color: Option<String>,  // Background color for the entire widget area (works for all widget types)
    #[serde(default)]
    pub bar_color: Option<String>,  // Hex color for progress bar (if widget_type is "progress")
    #[serde(default)]
    pub bar_background_color: Option<String>,  // Background color for progress bar
    #[serde(default = "default_transparent_background")]
    pub transparent_background: bool,  // If true, unfilled portions are transparent
    #[serde(default)]
    pub indicator_colors: Option<Vec<String>>,  // Colors for indicator states [off, on] or [none, 1-6]
    #[serde(default)]
    pub dashboard_layout: Option<String>,  // Dashboard layout: "horizontal", "vertical", "grid_2x2", etc.
    #[serde(default)]
    pub dashboard_indicators: Option<Vec<DashboardIndicatorDef>>,  // List of indicators for dashboard
    #[serde(default)]
    pub dashboard_spacing: Option<u16>,  // Spacing between dashboard indicators
    #[serde(default)]
    pub dashboard_hide_inactive: Option<bool>,  // Hide inactive indicators in dashboard
    #[serde(default)]
    pub visible_count: Option<usize>,  // For scrollable containers: how many items to show
    #[serde(default)]
    pub effect_category: Option<String>,  // For active_effects: "ActiveSpells", "Buffs", "Debuffs", "Cooldowns", "All"
    // Tabbed window configuration
    #[serde(default)]
    pub tabs: Option<Vec<TabConfig>>,  // If set, creates tabbed window (widget_type should be "tabbed")
    #[serde(default)]
    pub tab_bar_position: Option<String>,  // "top" or "bottom"
    #[serde(default)]
    pub tab_active_color: Option<String>,  // Color for active tab
    #[serde(default)]
    pub tab_inactive_color: Option<String>,  // Color for inactive tabs
    #[serde(default)]
    pub tab_unread_color: Option<String>,  // Color for tabs with unread messages
    #[serde(default)]
    pub tab_unread_prefix: Option<String>,  // Prefix for tabs with unread (e.g., "* ")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabConfig {
    pub name: String,    // Tab display name
    pub stream: String,  // Stream to route to this tab
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardIndicatorDef {
    pub id: String,              // e.g., "poisoned", "diseased"
    pub icon: String,            // Unicode icon character
    pub colors: Vec<String>,     // [off_color, on_color]
}

/// Parse border sides configuration into ratatui Borders bitflags
pub fn parse_border_sides(sides: &Option<Vec<String>>) -> ratatui::widgets::Borders {
    use ratatui::widgets::Borders;

    match sides {
        None => Borders::ALL,  // Default: all borders
        Some(list) if list.is_empty() => Borders::NONE,  // Empty list means no borders
        Some(list) => {
            let mut borders = Borders::empty();
            for side in list {
                match side.to_lowercase().as_str() {
                    "top" => borders |= Borders::TOP,
                    "bottom" => borders |= Borders::BOTTOM,
                    "left" => borders |= Borders::LEFT,
                    "right" => borders |= Borders::RIGHT,
                    "all" => return Borders::ALL,
                    "none" => return Borders::NONE,
                    _ => {
                        tracing::warn!("Invalid border side '{}', ignoring", side);
                    }
                }
            }
            // If no valid sides were specified, default to ALL
            if borders.is_empty() {
                Borders::ALL
            } else {
                borders
            }
        }
    }
}

fn default_widget_type() -> String {
    "text".to_string()
}

fn default_rows() -> u16 {
    1
}

fn default_cols() -> u16 {
    1
}

fn default_show_border() -> bool {
    true
}

fn default_transparent_background() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub character: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default)]
    pub show_timestamps: bool,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default = "default_command_echo_color")]
    pub command_echo_color: String,
    #[serde(default)]
    pub prompt_colors: Vec<PromptColor>,
    #[serde(default = "default_countdown_icon")]
    pub countdown_icon: String,  // Unicode character for countdown blocks (e.g., "\u{f0c8}")
    // Text selection settings
    #[serde(default = "default_selection_enabled")]
    pub selection_enabled: bool,
    #[serde(default = "default_selection_respect_window_boundaries")]
    pub selection_respect_window_boundaries: bool,
    #[serde(default = "default_selection_bg_color")]
    pub selection_bg_color: String,
    // Drag and drop settings
    #[serde(default = "default_drag_modifier_key")]
    pub drag_modifier_key: String,  // Modifier key required for drag and drop (e.g., "ctrl", "alt", "shift")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInputConfig {
    #[serde(default = "default_command_input_row")]
    pub row: u16,
    #[serde(default = "default_command_input_col")]
    pub col: u16,
    #[serde(default = "default_command_input_height")]
    pub height: u16,
    #[serde(default = "default_command_input_width")]
    pub width: u16,
    #[serde(default = "default_true")]
    pub show_border: bool,
    pub border_style: Option<String>,  // "single", "double", "rounded", "thick"
    pub border_color: Option<String>,
    pub title: Option<String>,
    pub background_color: Option<String>,  // Background color (transparent if not set)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayoutConfig {
    // Layout is now entirely defined by window positions and sizes
    // No global grid needed
}

/// Represents a saved layout (windows + command input position)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub windows: Vec<WindowDef>,
    #[serde(default = "default_command_input")]
    pub command_input: CommandInputConfig,
}

/// Content alignment within widget area (used when borders are removed)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentAlign {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl ContentAlign {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "top" => ContentAlign::Top,
            "top-right" | "topright" => ContentAlign::TopRight,
            "left" => ContentAlign::Left,
            "center" => ContentAlign::Center,
            "right" => ContentAlign::Right,
            "bottom-left" | "bottomleft" => ContentAlign::BottomLeft,
            "bottom" => ContentAlign::Bottom,
            "bottom-right" | "bottomright" => ContentAlign::BottomRight,
            _ => ContentAlign::TopLeft, // Default
        }
    }

    /// Calculate offset for rendering content within a larger area
    /// Returns (row_offset, col_offset)
    pub fn calculate_offset(&self, content_width: u16, content_height: u16, area_width: u16, area_height: u16) -> (u16, u16) {
        let row_offset = match self {
            ContentAlign::TopLeft | ContentAlign::Top | ContentAlign::TopRight => 0,
            ContentAlign::Left | ContentAlign::Center | ContentAlign::Right => (area_height.saturating_sub(content_height)) / 2,
            ContentAlign::BottomLeft | ContentAlign::Bottom | ContentAlign::BottomRight => area_height.saturating_sub(content_height),
        };

        let col_offset = match self {
            ContentAlign::TopLeft | ContentAlign::Left | ContentAlign::BottomLeft => 0,
            ContentAlign::Top | ContentAlign::Center | ContentAlign::Bottom => (area_width.saturating_sub(content_width)) / 2,
            ContentAlign::TopRight | ContentAlign::Right | ContentAlign::BottomRight => area_width.saturating_sub(content_width),
        };

        (row_offset, col_offset)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightPattern {
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub color_entire_line: bool,  // If true, apply colors to entire line, not just matched text
    #[serde(default, skip_serializing_if = "is_false")]
    pub fast_parse: bool,  // If true, split pattern on | and use Aho-Corasick for literal matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,  // Sound file to play when pattern matches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_volume: Option<f32>,  // Volume override for this sound (0.0 to 1.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundConfig {
    #[serde(default = "default_sound_enabled")]
    pub enabled: bool,
    #[serde(default = "default_sound_volume")]
    pub volume: f32,  // Master volume (0.0 to 1.0)
    #[serde(default = "default_sound_cooldown")]
    pub cooldown_ms: u64,  // Cooldown between same sound plays (milliseconds)
}

fn default_sound_enabled() -> bool {
    true
}

fn default_sound_volume() -> f32 {
    0.7
}

fn default_sound_cooldown() -> u64 {
    500  // 500ms default cooldown
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: default_sound_enabled(),
            volume: default_sound_volume(),
            cooldown_ms: default_sound_cooldown(),
        }
    }
}

// Helper function for serde skip_serializing_if
fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyBindAction {
    Action(String),          // Just an action: "cursor_word_left"
    Macro(MacroAction),      // A macro with text
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroAction {
    pub macro_text: String,  // e.g., "sw\r" for southwest movement
}

/// Actions that can be bound to keys
#[derive(Debug, Clone, PartialEq)]
pub enum KeyAction {
    // Command input actions
    SendCommand,
    CursorLeft,
    CursorRight,
    CursorWordLeft,
    CursorWordRight,
    CursorHome,
    CursorEnd,
    CursorBackspace,
    CursorDelete,

    // History actions
    PreviousCommand,
    NextCommand,
    SendLastCommand,
    SendSecondLastCommand,

    // Window actions
    SwitchCurrentWindow,
    ScrollCurrentWindowUpOne,
    ScrollCurrentWindowDownOne,
    ScrollCurrentWindowUpPage,
    ScrollCurrentWindowDownPage,

    // Search actions (already implemented)
    StartSearch,
    NextSearchMatch,
    PrevSearchMatch,
    ClearSearch,

    // Debug/Performance actions
    TogglePerformanceStats,

    // Macro - send literal text
    SendMacro(String),
}

impl KeyAction {
    pub fn from_str(action: &str) -> Option<Self> {
        match action {
            "send_command" => Some(Self::SendCommand),
            "cursor_left" => Some(Self::CursorLeft),
            "cursor_right" => Some(Self::CursorRight),
            "cursor_word_left" => Some(Self::CursorWordLeft),
            "cursor_word_right" => Some(Self::CursorWordRight),
            "cursor_home" => Some(Self::CursorHome),
            "cursor_end" => Some(Self::CursorEnd),
            "cursor_backspace" => Some(Self::CursorBackspace),
            "cursor_delete" => Some(Self::CursorDelete),
            "previous_command" => Some(Self::PreviousCommand),
            "next_command" => Some(Self::NextCommand),
            "send_last_command" => Some(Self::SendLastCommand),
            "send_second_last_command" => Some(Self::SendSecondLastCommand),
            "switch_current_window" => Some(Self::SwitchCurrentWindow),
            "scroll_current_window_up_one" => Some(Self::ScrollCurrentWindowUpOne),
            "scroll_current_window_down_one" => Some(Self::ScrollCurrentWindowDownOne),
            "scroll_current_window_up_page" => Some(Self::ScrollCurrentWindowUpPage),
            "scroll_current_window_down_page" => Some(Self::ScrollCurrentWindowDownPage),
            "start_search" => Some(Self::StartSearch),
            "next_search_match" => Some(Self::NextSearchMatch),
            "prev_search_match" => Some(Self::PrevSearchMatch),
            "clear_search" => Some(Self::ClearSearch),
            "toggle_performance_stats" => Some(Self::TogglePerformanceStats),
            _ => None,
        }
    }
}

/// Parse a key string like "ctrl+f" or "num_1" into KeyCode and KeyModifiers
pub fn parse_key_string(key_str: &str) -> Option<(KeyCode, KeyModifiers)> {
    // Special case: "num_+" contains a '+' but it's not a modifier separator
    // If the string is exactly a numpad key (no modifiers), handle it first
    if key_str.starts_with("num_") && !key_str.contains("shift+") && !key_str.contains("ctrl+") && !key_str.contains("alt+") {
        let key_code = match key_str {
            "num_0" => KeyCode::Keypad0,
            "num_1" => KeyCode::Keypad1,
            "num_2" => KeyCode::Keypad2,
            "num_3" => KeyCode::Keypad3,
            "num_4" => KeyCode::Keypad4,
            "num_5" => KeyCode::Keypad5,
            "num_6" => KeyCode::Keypad6,
            "num_7" => KeyCode::Keypad7,
            "num_8" => KeyCode::Keypad8,
            "num_9" => KeyCode::Keypad9,
            "num_." => KeyCode::KeypadPeriod,
            "num_+" => KeyCode::KeypadPlus,
            "num_-" => KeyCode::KeypadMinus,
            "num_*" => KeyCode::KeypadMultiply,
            "num_/" => KeyCode::KeypadDivide,
            _ => return None,
        };
        return Some((key_code, KeyModifiers::empty()));
    }

    // For keys with modifiers, we need to carefully parse
    // Split by + but be aware that num_+ contains a literal +
    let parts: Vec<&str> = key_str.split('+').collect();
    let mut modifiers = KeyModifiers::empty();
    let mut key_part = key_str;

    // Parse modifiers
    if parts.len() > 1 {
        for part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                "alt" => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                _ => return None,
            }
        }
        key_part = parts[parts.len() - 1];
    }

    // Parse the actual key
    let key_code = match key_part {
        // Special keys
        "enter" => KeyCode::Enter,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "tab" => KeyCode::Tab,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "page_up" | "pageup" => KeyCode::PageUp,
        "page_down" | "pagedown" => KeyCode::PageDown,

        // Numpad keys (when used with modifiers like shift+num_1)
        "num_0" => KeyCode::Keypad0,
        "num_1" => KeyCode::Keypad1,
        "num_2" => KeyCode::Keypad2,
        "num_3" => KeyCode::Keypad3,
        "num_4" => KeyCode::Keypad4,
        "num_5" => KeyCode::Keypad5,
        "num_6" => KeyCode::Keypad6,
        "num_7" => KeyCode::Keypad7,
        "num_8" => KeyCode::Keypad8,
        "num_9" => KeyCode::Keypad9,
        "num_." => KeyCode::KeypadPeriod,
        "num_+" => KeyCode::KeypadPlus,
        "num_-" => KeyCode::KeypadMinus,
        "num_*" => KeyCode::KeypadMultiply,
        "num_/" => KeyCode::KeypadDivide,

        // Function keys
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),

        // Single character
        s if s.len() == 1 => {
            let ch = s.chars().next().unwrap();
            KeyCode::Char(ch)
        }

        _ => return None,
    };

    Some((key_code, modifiers))
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8000
}

fn default_buffer_size() -> usize {
    1000
}

fn default_command_echo_color() -> String {
    "#ffffff".to_string()
}

/// Get default keybindings (based on ProfanityFE defaults)
pub fn default_keybinds() -> HashMap<String, KeyBindAction> {
    let mut map = HashMap::new();

    // Basic command input
    map.insert("enter".to_string(), KeyBindAction::Action("send_command".to_string()));
    map.insert("left".to_string(), KeyBindAction::Action("cursor_left".to_string()));
    map.insert("right".to_string(), KeyBindAction::Action("cursor_right".to_string()));
    map.insert("ctrl+left".to_string(), KeyBindAction::Action("cursor_word_left".to_string()));
    map.insert("ctrl+right".to_string(), KeyBindAction::Action("cursor_word_right".to_string()));
    map.insert("home".to_string(), KeyBindAction::Action("cursor_home".to_string()));
    map.insert("end".to_string(), KeyBindAction::Action("cursor_end".to_string()));
    map.insert("backspace".to_string(), KeyBindAction::Action("cursor_backspace".to_string()));
    map.insert("delete".to_string(), KeyBindAction::Action("cursor_delete".to_string()));

    // Window management
    map.insert("tab".to_string(), KeyBindAction::Action("switch_current_window".to_string()));
    map.insert("alt+page_up".to_string(), KeyBindAction::Action("scroll_current_window_up_one".to_string()));
    map.insert("alt+page_down".to_string(), KeyBindAction::Action("scroll_current_window_down_one".to_string()));
    map.insert("page_up".to_string(), KeyBindAction::Action("scroll_current_window_up_page".to_string()));
    map.insert("page_down".to_string(), KeyBindAction::Action("scroll_current_window_down_page".to_string()));

    // Command history
    map.insert("up".to_string(), KeyBindAction::Action("previous_command".to_string()));
    map.insert("down".to_string(), KeyBindAction::Action("next_command".to_string()));

    // Search
    map.insert("ctrl+f".to_string(), KeyBindAction::Action("start_search".to_string()));
    map.insert("ctrl+page_up".to_string(), KeyBindAction::Action("prev_search_match".to_string()));
    map.insert("ctrl+page_down".to_string(), KeyBindAction::Action("next_search_match".to_string()));

    // Debug/Performance
    map.insert("f12".to_string(), KeyBindAction::Action("toggle_performance_stats".to_string()));

    // Numpad movement macros
    map.insert("num_1".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "sw\r".to_string() }));
    map.insert("num_2".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "s\r".to_string() }));
    map.insert("num_3".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "se\r".to_string() }));
    map.insert("num_4".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "w\r".to_string() }));
    map.insert("num_5".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "out\r".to_string() }));
    map.insert("num_6".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "e\r".to_string() }));
    map.insert("num_7".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "nw\r".to_string() }));
    map.insert("num_8".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "n\r".to_string() }));
    map.insert("num_9".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "ne\r".to_string() }));
    map.insert("num_0".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "down\r".to_string() }));
    map.insert("num_.".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "up\r".to_string() }));
    map.insert("num_+".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "look\r".to_string() }));
    map.insert("num_-".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "info\r".to_string() }));
    map.insert("num_*".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "exp\r".to_string() }));
    map.insert("num_/".to_string(), KeyBindAction::Macro(MacroAction { macro_text: "health\r".to_string() }));

    // Note: Shift+numpad doesn't work on Windows - the OS doesn't report SHIFT modifier for numpad numeric keys
    // If you want peer keybinds, use alt+numpad or ctrl+numpad instead (those modifiers work with numpad)

    map
}

fn default_countdown_icon() -> String {
    "\u{f0c8}".to_string()  // Nerd Font square icon
}

fn default_selection_enabled() -> bool {
    true
}

fn default_selection_respect_window_boundaries() -> bool {
    true
}

fn default_selection_bg_color() -> String {
    "#4a4a4a".to_string()
}

fn default_drag_modifier_key() -> String {
    "ctrl".to_string()
}

fn default_command_input() -> CommandInputConfig {
    CommandInputConfig {
        row: 0,     // Will be calculated based on terminal height
        col: 0,
        height: 3,
        width: 0,   // Will use full terminal width
        show_border: true,
        border_style: None,
        border_color: None,
        title: None,  // No title by default
        background_color: None,  // Transparent by default
    }
}

fn default_command_input_row() -> u16 {
    0  // Will be calculated dynamically
}

fn default_command_input_col() -> u16 {
    0
}

fn default_command_input_height() -> u16 {
    3
}

fn default_command_input_width() -> u16 {
    0  // 0 means use full terminal width
}

fn default_true() -> bool {
    true
}

fn default_windows() -> Vec<WindowDef> {
    // Default layout using absolute terminal cell positions
    // Assumes typical terminal: ~120 cols x 40 rows
    // Main: top 24 rows, full width
    // Vitals: row 24-29 (two rows of 3-row-tall vitals/countdowns)
    // Thoughts: bottom 10 rows, left 70% (~84 cols)
    // Speech: bottom 10 rows, right 30% (~36 cols)
    vec![
        WindowDef {
            name: "main".to_string(),
            widget_type: "text".to_string(),
            streams: vec!["main".to_string()],
            row: 0,      // Start at top
            col: 0,      // Start at left
            rows: 24,    // 24 rows tall (leave room for 2 rows of vitals)
            cols: 120,   // Full width (will adjust to actual terminal width)
            buffer_size: 10000,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: None,
            title: None,
            content_align: None,
            background_color: None,
            bar_color: None,
            bar_background_color: None,
            transparent_background: true,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        // First row of vitals (row 24-26): Core stats
        WindowDef {
            name: "health".to_string(),
            widget_type: "progress".to_string(),
            streams: vec![],
            row: 24,
            col: 0,
            rows: 3,
            cols: 24,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Health".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#6e0202".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        WindowDef {
            name: "mana".to_string(),
            widget_type: "progress".to_string(),
            streams: vec![],
            row: 24,
            col: 24,
            rows: 3,
            cols: 24,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Mana".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#08086d".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        WindowDef {
            name: "stamina".to_string(),
            widget_type: "progress".to_string(),
            streams: vec![],
            row: 24,
            col: 48,
            rows: 3,
            cols: 24,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Stamina".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#bd7b00".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        WindowDef {
            name: "spirit".to_string(),
            widget_type: "progress".to_string(),
            streams: vec![],
            row: 24,
            col: 72,
            rows: 3,
            cols: 24,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Spirit".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#6e727c".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        WindowDef {
            name: "mindstate".to_string(),
            widget_type: "progress".to_string(),
            streams: vec![],
            row: 24,
            col: 96,
            rows: 3,
            cols: 24,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Mind".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#008b8b".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        // Second row of vitals (row 27-29): Stance, Encumbrance, Countdowns
        WindowDef {
            name: "stance".to_string(),
            widget_type: "progress".to_string(),
            streams: vec![],
            row: 27,
            col: 0,
            rows: 3,
            cols: 20,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Stance".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#000080".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        WindowDef {
            name: "encumlevel".to_string(),
            widget_type: "progress".to_string(),
            streams: vec![],
            row: 27,
            col: 20,
            rows: 3,
            cols: 25,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Encumbrance".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#006400".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        // Countdown timers (row 27-29, right side)
        WindowDef {
            name: "roundtime".to_string(),
            widget_type: "countdown".to_string(),
            streams: vec![],
            row: 27,
            col: 45,
            rows: 3,
            cols: 15,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("RT".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#ff0000".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        WindowDef {
            name: "casttime".to_string(),
            widget_type: "countdown".to_string(),
            streams: vec![],
            row: 27,
            col: 60,
            rows: 3,
            cols: 15,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Cast".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#0000ff".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        WindowDef {
            name: "stuntime".to_string(),
            widget_type: "countdown".to_string(),
            streams: vec![],
            row: 27,
            col: 75,
            rows: 3,
            cols: 15,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Stun".to_string()),
            content_align: None,
            background_color: None,
            bar_color: Some("#ffff00".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        // Text windows (row 30+)
        WindowDef {
            name: "thoughts".to_string(),
            widget_type: "text".to_string(),
            streams: vec!["thoughts".to_string()],
            row: 30,     // Start at row 30 (below vitals)
            col: 0,      // Left side
            rows: 10,    // 10 rows tall
            cols: 84,    // 70% of width
            buffer_size: 500,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: None,
            title: None,
            content_align: None,
            background_color: None,
            bar_color: None,
            bar_background_color: None,
            transparent_background: true,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
        WindowDef {
            name: "speech".to_string(),
            widget_type: "text".to_string(),
            streams: vec!["speech".to_string(), "whisper".to_string()],
            row: 30,     // Start at row 30 (same as thoughts)
            col: 84,     // Start at col 84 (right of thoughts)
            rows: 10,    // 10 rows tall
            cols: 36,    // 30% of width
            buffer_size: 1000,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: None,
            title: None,
            content_align: None,
            background_color: None,
            bar_color: None,
            bar_background_color: None,
            transparent_background: true,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
            visible_count: None,
            effect_category: None,
            tabs: None,
            tab_bar_position: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        },
    ]
}

impl Layout {
    /// Load layout from file (checks autosave, character-specific, then default)
    /// Priority: auto_<character>.toml → <character>.toml → default.toml → embedded default
    pub fn load(character: Option<&str>) -> Result<Self> {
        let layouts_dir = Config::layouts_dir()?;

        // Try character-specific autosave first
        if let Some(char_name) = character {
            let auto_char_path = layouts_dir.join(format!("auto_{}.toml", char_name));
            if auto_char_path.exists() {
                return Self::load_from_file(&auto_char_path);
            }

            let char_path = layouts_dir.join(format!("{}.toml", char_name));
            if char_path.exists() {
                return Self::load_from_file(&char_path);
            }
        }

        // Try generic autosave
        let autosave_path = layouts_dir.join("autosave.toml");
        if autosave_path.exists() {
            return Self::load_from_file(&autosave_path);
        }

        // Try default layout
        let default_path = layouts_dir.join("default.toml");
        if default_path.exists() {
            return Self::load_from_file(&default_path);
        }

        // Fall back to embedded default
        tracing::info!("No layout found, using embedded default");
        let layout: Layout = toml::from_str(DEFAULT_LAYOUT)
            .context("Failed to parse embedded default layout")?;

        // Save it as default for next time
        fs::create_dir_all(&layouts_dir)?;
        fs::write(&default_path, DEFAULT_LAYOUT)
            .context("Failed to write default layout")?;

        Ok(layout)
    }

    pub fn load_from_file(path: &std::path::Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .context(format!("Failed to read layout file: {:?}", path))?;
        let layout: Layout = toml::from_str(&contents)
            .context(format!("Failed to parse layout file: {:?}", path))?;
        Ok(layout)
    }

    /// Save layout to file
    pub fn save(&self, name: &str) -> Result<()> {
        let layout_path = Config::layout_path(name)?;

        if let Some(parent) = layout_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let toml_string = toml::to_string_pretty(&self)
            .context("Failed to serialize layout")?;
        fs::write(&layout_path, toml_string)
            .context("Failed to write layout file")?;

        Ok(())
    }
}

impl Config {
    /// Get a window template by name
    /// Returns a WindowDef with default positioning that can be customized
    pub fn get_window_template(name: &str) -> Option<WindowDef> {
        // Default small window size and position (can be moved/resized by user)
        let default_row = 0;
        let default_col = 0;
        let default_rows = 10;
        let default_cols = 40;

        match name {
            "main" => Some(WindowDef {
                name: "main".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["main".to_string()],
                row: 0,
                col: 0,
                rows: 30,
                cols: 120,
                buffer_size: 10000,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Main".to_string()),
                content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "thoughts" | "thought" => Some(WindowDef {
                name: "thoughts".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["thoughts".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 500,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Thoughts".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "speech" => Some(WindowDef {
                name: "speech".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["speech".to_string(), "whisper".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 1000,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Speech".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "familiar" => Some(WindowDef {
                name: "familiar".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["familiar".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 500,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Familiar".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "room" => Some(WindowDef {
                name: "room".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["room".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 100,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Room".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "logon" | "logons" => Some(WindowDef {
                name: "logons".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["logons".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 500,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Logons".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "death" | "deaths" => Some(WindowDef {
                name: "deaths".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["deaths".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 500,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Deaths".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "arrivals" => Some(WindowDef {
                name: "arrivals".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["arrivals".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 500,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Arrivals".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "ambients" => Some(WindowDef {
                name: "ambients".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["ambients".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 500,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Ambients".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "announcements" => Some(WindowDef {
                name: "announcements".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["announcements".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 500,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Announcements".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "loot" => Some(WindowDef {
                name: "loot".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["loot".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 500,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Loot".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "health" | "hp" => Some(WindowDef {
                name: "health".to_string(),
                widget_type: "progress".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 30,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Health".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#6e0202".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "mana" | "mp" => Some(WindowDef {
                name: "mana".to_string(),
                widget_type: "progress".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 30,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Mana".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#08086d".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "stamina" | "stam" => Some(WindowDef {
                name: "stamina".to_string(),
                widget_type: "progress".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 30,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Stamina".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#bd7b00".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "spirit" => Some(WindowDef {
                name: "spirit".to_string(),
                widget_type: "progress".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 30,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Spirit".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#6e727c".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "mindstate" | "mind" => Some(WindowDef {
                name: "mindState".to_string(),
                widget_type: "progress".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 30,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Mind".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#008b8b".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "encumbrance" | "encum" | "encumlevel" => Some(WindowDef {
                name: "encumlevel".to_string(),
                widget_type: "progress".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 30,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Encumbrance".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#006400".to_string()), // Will change dynamically based on value
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "stance" | "pbarstance" => Some(WindowDef {
                name: "pbarStance".to_string(),
                widget_type: "progress".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 30,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Stance".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#000080".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "bloodpoints" | "blood" | "lblbps" => Some(WindowDef {
                name: "lblBPs".to_string(),
                widget_type: "progress".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 30,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Blood Points".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#4d0085".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "roundtime" | "rt" => Some(WindowDef {
                name: "roundtime".to_string(),
                widget_type: "countdown".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 15,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("RT".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#ff0000".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "casttime" | "cast" => Some(WindowDef {
                name: "casttime".to_string(),
                widget_type: "countdown".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 15,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Cast".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#0000ff".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "stun" | "stuntime" => Some(WindowDef {
                name: "stuntime".to_string(),
                widget_type: "countdown".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 15,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Stun".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#ffff00".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "compass" => Some(WindowDef {
                name: "compass".to_string(),
                widget_type: "compass".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 5,  // 3 rows for grid + 2 for border
                cols: 17, // 4 columns * 4 chars wide + border
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Exits".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "injuries" | "injury_doll" => Some(WindowDef {
                name: "injuries".to_string(),
                widget_type: "injury_doll".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 8,  // 6 rows for body + 2 for border
                cols: 15, // ~13 chars wide + border
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Injuries".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "hands" => Some(WindowDef {
                name: "hands".to_string(),
                widget_type: "hands".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 5,  // 3 rows for hands + 2 for border
                cols: 29, // "L: " + 24 chars + border (2+24+3)
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Hands".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "lefthand" => Some(WindowDef {
                name: "lefthand".to_string(),
                widget_type: "lefthand".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 29, // "L: " + 24 chars + border (2+24+3)
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Left Hand".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "righthand" => Some(WindowDef {
                name: "righthand".to_string(),
                widget_type: "righthand".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 29, // "R: " + 24 chars + border (2+24+3)
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Right Hand".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "spellhand" => Some(WindowDef {
                name: "spellhand".to_string(),
                widget_type: "spellhand".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 29, // "S: " + 24 chars + border (2+24+3)
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Spell".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "poisoned" => Some(WindowDef {
                name: "poisoned".to_string(),
                widget_type: "indicator".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 3,  // Icon + border
                buffer_size: 0,
                show_border: false,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("\u{e231}".to_string()), // Nerd Font poison icon
                content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#00ff00".to_string()]), // off, green
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "diseased" => Some(WindowDef {
                name: "diseased".to_string(),
                widget_type: "indicator".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 3,  // Icon + border
                buffer_size: 0,
                show_border: false,
                border_style: None,
                border_color: None,
            border_sides: None,
                content_align: None,
            background_color: None,
                title: Some("\u{e286}".to_string()), // Nerd Font disease icon
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#8b4513".to_string()]), // off, brownish-red
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "bleeding" => Some(WindowDef {
                name: "bleeding".to_string(),
                widget_type: "indicator".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 3,  // Icon + border
                buffer_size: 0,
                show_border: false,
                border_style: None,
                border_color: None,
            border_sides: None,
                content_align: None,
            background_color: None,
                title: Some("\u{f043}".to_string()), // Nerd Font bleeding icon
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#ff0000".to_string()]), // off, red
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "stunned" => Some(WindowDef {
                name: "stunned".to_string(),
                widget_type: "indicator".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 3,  // Icon + border
                buffer_size: 0,
                show_border: false,
                border_style: None,
                border_color: None,
            border_sides: None,
                content_align: None,
            background_color: None,
                title: Some("\u{f0e7}".to_string()), // Nerd Font stunned icon
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#ffff00".to_string()]), // off, yellow
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "webbed" => Some(WindowDef {
                name: "webbed".to_string(),
                widget_type: "indicator".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 3,  // Icon + border
                buffer_size: 0,
                show_border: false,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("\u{f0bca}".to_string()), // Nerd Font webbed icon
                content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#cccccc".to_string()]), // off, bright grey
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "status_dashboard" => Some(WindowDef {
                name: "status_dashboard".to_string(),
                widget_type: "dashboard".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 15, // ~5 icons * 2 (icon + space) + border
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Status".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: Some("horizontal".to_string()),
                dashboard_indicators: Some(vec![
                    DashboardIndicatorDef {
                        id: "poisoned".to_string(),
                        icon: "\u{e231}".to_string(),
                        colors: vec!["#000000".to_string(), "#00ff00".to_string()],
                    },
                    DashboardIndicatorDef {
                        id: "diseased".to_string(),
                        icon: "\u{e286}".to_string(),
                        colors: vec!["#000000".to_string(), "#8b4513".to_string()],
                    },
                    DashboardIndicatorDef {
                        id: "bleeding".to_string(),
                        icon: "\u{f043}".to_string(),
                        colors: vec!["#000000".to_string(), "#ff0000".to_string()],
                    },
                    DashboardIndicatorDef {
                        id: "stunned".to_string(),
                        icon: "\u{f0e7}".to_string(),
                        colors: vec!["#000000".to_string(), "#ffff00".to_string()],
                    },
                    DashboardIndicatorDef {
                        id: "webbed".to_string(),
                        icon: "\u{f0bca}".to_string(),
                        colors: vec!["#000000".to_string(), "#cccccc".to_string()],
                    },
                ]),
                dashboard_spacing: Some(1),
                dashboard_hide_inactive: Some(true),
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "buffs" => Some(WindowDef {
                name: "buffs".to_string(),
                widget_type: "active_effects".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 7,
                cols: 40,
                buffer_size: 0,
                show_border: true,
                border_style: Some("rounded".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Buffs".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#40FF40".to_string()),
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: Some(5),
                effect_category: Some("Buffs".to_string()),
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "debuffs" => Some(WindowDef {
                name: "debuffs".to_string(),
                widget_type: "active_effects".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 5,
                cols: 40,
                buffer_size: 0,
                show_border: true,
                border_style: Some("rounded".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Debuffs".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#FF4040".to_string()),
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: Some(3),
                effect_category: Some("Debuffs".to_string()),
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "cooldowns" => Some(WindowDef {
                name: "cooldowns".to_string(),
                widget_type: "active_effects".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 5,
                cols: 40,
                buffer_size: 0,
                show_border: true,
                border_style: Some("rounded".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Cooldowns".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#FFB040".to_string()),
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: Some(3),
                effect_category: Some("Cooldowns".to_string()),
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "active_spells" | "spells" => Some(WindowDef {
                name: "active_spells".to_string(),
                widget_type: "active_effects".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 18,
                cols: 40,
                buffer_size: 0,
                show_border: true,
                border_style: Some("rounded".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Active Spells".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#4080FF".to_string()),
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: Some("Active Spells".to_string()),
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "all_effects" | "effects" => Some(WindowDef {
                name: "all_effects".to_string(),
                widget_type: "active_effects".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 12,
                cols: 40,
                buffer_size: 0,
                show_border: true,
                border_style: Some("rounded".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("All Active Effects".to_string()),
            content_align: None,
            background_color: None,
                bar_color: Some("#808080".to_string()),
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: Some(10),
                effect_category: Some("All".to_string()),
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "targets" => Some(WindowDef {
                name: "targets".to_string(),
                widget_type: "targets".to_string(),
                streams: vec!["targetcount".to_string(), "combat".to_string()],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 20,
                buffer_size: 0,
                show_border: true,
                border_style: Some("rounded".to_string()),
                border_color: None,
                border_sides: None,
                title: Some("Targets".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            "players" => Some(WindowDef {
                name: "players".to_string(),
                widget_type: "players".to_string(),
                streams: vec!["playercount".to_string(), "playerlist".to_string()],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 20,
                buffer_size: 0,
                show_border: true,
                border_style: Some("rounded".to_string()),
                border_color: None,
                border_sides: None,
                title: Some("Players".to_string()),
            content_align: None,
            background_color: None,
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: None,
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            }),
            _ => None,
        }
    }

    /// Get list of available window templates
    pub fn available_window_templates() -> Vec<&'static str> {
        vec![
            "main",
            "thoughts",
            "speech",
            "familiar",
            "room",
            "logons",
            "deaths",
            "arrivals",
            "ambients",
            "announcements",
            "loot",
            "health",
            "mana",
            "stamina",
            "spirit",
            "mindstate",
            "encumbrance",
            "stance",
            "bloodpoints",
            "roundtime",
            "casttime",
            "stuntime",
            "compass",
            "injuries",
            "hands",
            "lefthand",
            "righthand",
            "spellhand",
            "poisoned",
            "diseased",
            "bleeding",
            "stunned",
            "webbed",
            "status_dashboard",
            "buffs",
            "debuffs",
            "cooldowns",
            "active_spells",
            "spells",
            "all_effects",
            "effects",
            "targets",
            "players",
        ]
    }

    pub fn load() -> Result<Self> {
        Self::load_with_options(None, 8000)
    }

    /// Load config with command-line options
    /// Checks in order:
    /// 1. ./config/<character>.toml (if character specified)
    /// 2. ./config/default.toml
    /// 3. ~/.vellum-fe/<character>.toml (if character specified)
    /// 4. ~/.vellum-fe/config.toml (fallback)
    pub fn load_with_options(character: Option<&str>, port_override: u16) -> Result<Self> {
        // Build character-specific config path
        let config_path = Self::config_path(character)?;

        // Try to load from ~/.vellum-fe/configs/<character>.toml or default.toml
        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .context(format!("Failed to read config file: {:?}", config_path))?;
            let mut config: Config = toml::from_str(&contents)
                .context(format!("Failed to parse config file: {:?}", config_path))?;

            // Override port from command line
            config.connection.port = port_override;

            // Store character name for later saves
            config.character = character.map(|s| s.to_string());

            // If keybinds is empty, populate with defaults
            if config.keybinds.is_empty() {
                config.keybinds = default_keybinds();
            }

            return Ok(config);
        }

        // No config found - create from embedded defaults
        tracing::info!("No config found, creating from embedded defaults");

        // Parse embedded default config
        let mut config: Config = toml::from_str(DEFAULT_CONFIG)
            .context("Failed to parse embedded default config")?;

        config.connection.port = port_override;
        config.character = character.map(|s| s.to_string());

        // Create directories if needed
        let configs_dir = Self::configs_dir()?;
        let layouts_dir = Self::layouts_dir()?;
        fs::create_dir_all(&configs_dir)?;
        fs::create_dir_all(&layouts_dir)?;

        // Write default config to ~/.vellum-fe/configs/default.toml (if it doesn't exist)
        let default_config_path = configs_dir.join("default.toml");
        if !default_config_path.exists() {
            fs::write(&default_config_path, DEFAULT_CONFIG)
                .context("Failed to write default config")?;
            tracing::info!("Created default config at {:?}", default_config_path);
        }

        // If character was specified, also create character-specific config
        if let Some(char_name) = character {
            let char_config_path = configs_dir.join(format!("{}.toml", char_name));
            fs::write(&char_config_path, DEFAULT_CONFIG)
                .context("Failed to write character-specific config")?;
            tracing::info!("Created character-specific config at {:?}", char_config_path);
        }

        // Write default layout to ~/.vellum-fe/layouts/default.toml (if it doesn't exist)
        let default_layout_path = layouts_dir.join("default.toml");
        if !default_layout_path.exists() {
            fs::write(&default_layout_path, DEFAULT_LAYOUT)
                .context("Failed to write default layout")?;
            tracing::info!("Created default layout at {:?}", default_layout_path);
        }

        Ok(config)
    }

    pub fn save(&self, character: Option<&str>) -> Result<()> {
        // Use provided character name, or fall back to stored character name
        let char_name = character.or(self.character.as_deref());
        let config_path = Self::config_path(char_name)?;

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        fs::write(&config_path, contents)
            .context("Failed to write config file")?;

        Ok(())
    }

    fn config_path(character: Option<&str>) -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        let config_dir = home.join(".vellum-fe").join("configs");

        let filename = if let Some(char_name) = character {
            format!("{}.toml", char_name)
        } else {
            "default.toml".to_string()
        };

        Ok(config_dir.join(filename))
    }

    fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        Ok(home.join(".vellum-fe"))
    }

    fn configs_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        Ok(home.join(".vellum-fe").join("configs"))
    }

    fn layouts_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        Ok(home.join(".vellum-fe").join("layouts"))
    }

    pub fn get_log_path(character: Option<&str>) -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        let log_dir = home.join(".vellum-fe");

        let filename = if let Some(char_name) = character {
            format!("debug_{}.log", char_name)
        } else {
            "debug.log".to_string()
        };

        Ok(log_dir.join(filename))
    }

    /// List all saved layouts
    pub fn list_layouts() -> Result<Vec<String>> {
        let layouts_dir = Self::config_dir()?.join("layouts");

        if !layouts_dir.exists() {
            return Ok(vec![]);
        }

        let mut layouts = vec![];
        for entry in fs::read_dir(layouts_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    layouts.push(name.to_string());
                }
            }
        }

        layouts.sort();
        Ok(layouts)
    }

    pub fn layout_path(name: &str) -> Result<PathBuf> {
        let layouts_dir = Self::layouts_dir()?;
        Ok(layouts_dir.join(format!("{}.toml", name)))
    }

    /// Resolve a spell ID to a color based on configured spell lists
    /// Example: spells = [101, 107, 120, 140, 150]
    pub fn get_spell_color(&self, spell_id: u32) -> Option<String> {
        for spell_config in &self.spell_colors {
            if spell_config.spells.contains(&spell_id) {
                return Some(spell_config.color.clone());
            }
        }
        None
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig {
                host: default_host(),
                port: default_port(),
                character: None,
            },
            ui: UiConfig {
                buffer_size: default_buffer_size(),
                show_timestamps: false,
                layout: LayoutConfig::default(),
                command_echo_color: default_command_echo_color(),
                prompt_colors: vec![
                    PromptColor { character: "R".to_string(), color: "#ff0000".to_string() }, // Red for Roundtime
                    PromptColor { character: "S".to_string(), color: "#ffff00".to_string() }, // Yellow for Stunned
                    PromptColor { character: "H".to_string(), color: "#9370db".to_string() }, // Purple for Hidden
                    PromptColor { character: ">".to_string(), color: "#a9a9a9".to_string() }, // DarkGray default
                ],
                countdown_icon: default_countdown_icon(),
                selection_enabled: default_selection_enabled(),
                selection_respect_window_boundaries: default_selection_respect_window_boundaries(),
                selection_bg_color: default_selection_bg_color(),
                drag_modifier_key: default_drag_modifier_key(),
            },
            presets: {
                let mut map = HashMap::new();
                map.insert("whisper".to_string(), PresetColor { fg: Some("#60b4bf".to_string()), bg: None });
                map.insert("links".to_string(), PresetColor { fg: Some("#477ab3".to_string()), bg: None });
                map.insert("speech".to_string(), PresetColor { fg: Some("#53a684".to_string()), bg: None });
                map.insert("roomName".to_string(), PresetColor { fg: Some("#9BA2B2".to_string()), bg: Some("#395573".to_string()) });
                map.insert("monsterbold".to_string(), PresetColor { fg: Some("#a29900".to_string()), bg: None });
                map.insert("familiar".to_string(), PresetColor { fg: Some("#767339".to_string()), bg: None });
                map.insert("thought".to_string(), PresetColor { fg: Some("#FF8080".to_string()), bg: None });
                map
            },
            highlights: {
                let mut map = HashMap::new();
                // Example: Fast highlight for multiple player names (ultra-fast with Aho-Corasick)
                map.insert("friends".to_string(), HighlightPattern {
                    pattern: "Alice|Bob|Charlie|David|Eve|Frank".to_string(),
                    fg: Some("#ff00ff".to_string()),
                    bg: None,
                    bold: true,
                    color_entire_line: false,
                    fast_parse: true,
                    sound: None,
                    sound_volume: None,
                });
                // Example: Highlight your combat actions in red (partial line, regex)
                map.insert("swing".to_string(), HighlightPattern {
                    pattern: r"You swing.*".to_string(),
                    fg: Some("#ff0000".to_string()),
                    bg: None,
                    bold: true,
                    color_entire_line: false,
                    fast_parse: false,
                    sound: None,
                    sound_volume: None,
                });
                // Example: Highlight damage numbers in yellow (partial line, regex)
                map.insert("damage".to_string(), HighlightPattern {
                    pattern: r"\d+ points? of damage".to_string(),
                    fg: Some("#ffff00".to_string()),
                    bg: None,
                    bold: true,
                    color_entire_line: false,
                    fast_parse: false,
                    sound: None,
                    sound_volume: None,
                });
                // Example: Highlight death messages with bright background (whole line, regex)
                map.insert("death".to_string(), HighlightPattern {
                    pattern: r".*dies.*".to_string(),
                    fg: Some("#ffffff".to_string()),
                    bg: Some("#ff0000".to_string()),
                    bold: true,
                    color_entire_line: true,
                    fast_parse: false,
                    sound: None,
                    sound_volume: None,
                });
                map
            },
            keybinds: default_keybinds(),
            spell_colors: vec![
                // Example spell colors - list commonly used spells from each circle
                // Light blue for Minor Elemental (400 series)
                SpellColorRange {
                    spells: vec![401, 406, 414, 419, 430, 435],
                    color: "#87ceeb".to_string()
                },
                // Dark blue for Major Elemental (500 series)
                SpellColorRange {
                    spells: vec![503, 506, 507, 508, 509, 513, 520, 525, 530, 540],
                    color: "#4169e1".to_string()
                },
                // Purple for Wizard (900 series)
                SpellColorRange {
                    spells: vec![905, 911, 913, 918, 919, 920, 925, 930, 940],
                    color: "#9370db".to_string()
                },
                // Green for Ranger (600 series)
                SpellColorRange {
                    spells: vec![601, 602, 605, 606, 608, 613, 616, 618, 625, 630, 640],
                    color: "#32cd32".to_string()
                },
                // Yellow for Cleric (300 series)
                SpellColorRange {
                    spells: vec![303, 307, 310, 313, 315, 317, 318, 319, 325, 330, 335, 340],
                    color: "#ffd700".to_string()
                },
                // Red for Sorcerer (700 series)
                SpellColorRange {
                    spells: vec![701, 703, 705, 708, 712, 713, 715, 720, 725, 730, 735, 740],
                    color: "#ff4500".to_string()
                },
                // Cyan for Empath (1100 series)
                SpellColorRange {
                    spells: vec![1101, 1107, 1109, 1115, 1120, 1125, 1130, 1140, 1150],
                    color: "#00ffff".to_string()
                },
                // Orange for Bard (1000 series)
                SpellColorRange {
                    spells: vec![1001, 1003, 1006, 1010, 1012, 1019, 1025, 1030, 1035, 1040],
                    color: "#ff8c00".to_string()
                },
                // Pink for Paladin (1600 series)
                SpellColorRange {
                    spells: vec![1601, 1602, 1605, 1610, 1615, 1617, 1618, 1625, 1630, 1635],
                    color: "#ff69b4".to_string()
                },
                // Sky blue for Minor Spirit (100 series)
                SpellColorRange {
                    spells: vec![101, 107, 120, 125, 130, 140, 150, 175],
                    color: "#00bfff".to_string()
                },
            ],
            sound: SoundConfig::default(),
            character: None,  // Set at runtime via load_with_options
        }
    }
}
