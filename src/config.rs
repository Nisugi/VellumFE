use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crossterm::event::{KeyCode, KeyModifiers};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub connection: ConnectionConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub presets: Vec<PresetColor>,
    #[serde(default)]
    pub highlights: Vec<HighlightPattern>,
    #[serde(default)]
    pub keybinds: Vec<KeyBind>,
    #[serde(default)]
    pub spell_colors: Vec<SpellColorRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetColor {
    pub id: String,
    pub fg: Option<String>,
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
    #[serde(default = "default_windows")]
    pub windows: Vec<WindowDef>,
    #[serde(default = "default_mouse_mode_toggle_key")]
    pub mouse_mode_toggle_key: String,  // Key to toggle mouse mode (e.g., "F12")
    #[serde(default = "default_countdown_icon")]
    pub countdown_icon: String,  // Unicode character for countdown blocks (e.g., "\u{f0c8}")
    #[serde(default = "default_command_input")]
    pub command_input: CommandInputConfig,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayoutConfig {
    // Layout is now entirely defined by window positions and sizes
    // No global grid needed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightPattern {
    pub pattern: String,
    pub fg: Option<String>,
    pub bg: Option<String>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub color_entire_line: bool,  // If true, apply colors to entire line, not just matched text
    #[serde(default)]
    pub fast_parse: bool,  // If true, split pattern on | and use Aho-Corasick for literal matching
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBind {
    pub key: String,           // e.g., "ctrl+f", "num_1", "alt+page_up"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>, // e.g., "cursor_word_left", "send_command"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macro_text: Option<String>, // e.g., "sw\r" for southwest movement
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

fn default_mouse_mode_toggle_key() -> String {
    "F11".to_string()
}

/// Get default keybindings (based on ProfanityFE defaults)
pub fn default_keybinds() -> Vec<KeyBind> {
    vec![
        // Basic command input
        KeyBind { key: "enter".to_string(), action: Some("send_command".to_string()), macro_text: None },
        KeyBind { key: "left".to_string(), action: Some("cursor_left".to_string()), macro_text: None },
        KeyBind { key: "right".to_string(), action: Some("cursor_right".to_string()), macro_text: None },
        KeyBind { key: "ctrl+left".to_string(), action: Some("cursor_word_left".to_string()), macro_text: None },
        KeyBind { key: "ctrl+right".to_string(), action: Some("cursor_word_right".to_string()), macro_text: None },
        KeyBind { key: "home".to_string(), action: Some("cursor_home".to_string()), macro_text: None },
        KeyBind { key: "end".to_string(), action: Some("cursor_end".to_string()), macro_text: None },
        KeyBind { key: "backspace".to_string(), action: Some("cursor_backspace".to_string()), macro_text: None },
        KeyBind { key: "delete".to_string(), action: Some("cursor_delete".to_string()), macro_text: None },

        // Window management
        KeyBind { key: "tab".to_string(), action: Some("switch_current_window".to_string()), macro_text: None },
        KeyBind { key: "alt+page_up".to_string(), action: Some("scroll_current_window_up_one".to_string()), macro_text: None },
        KeyBind { key: "alt+page_down".to_string(), action: Some("scroll_current_window_down_one".to_string()), macro_text: None },
        KeyBind { key: "page_up".to_string(), action: Some("scroll_current_window_up_page".to_string()), macro_text: None },
        KeyBind { key: "page_down".to_string(), action: Some("scroll_current_window_down_page".to_string()), macro_text: None },

        // Command history
        KeyBind { key: "up".to_string(), action: Some("previous_command".to_string()), macro_text: None },
        KeyBind { key: "down".to_string(), action: Some("next_command".to_string()), macro_text: None },

        // Search (already implemented)
        KeyBind { key: "ctrl+f".to_string(), action: Some("start_search".to_string()), macro_text: None },
        KeyBind { key: "ctrl+page_up".to_string(), action: Some("prev_search_match".to_string()), macro_text: None },
        KeyBind { key: "ctrl+page_down".to_string(), action: Some("next_search_match".to_string()), macro_text: None },

        // Debug/Performance
        KeyBind { key: "f12".to_string(), action: Some("toggle_performance_stats".to_string()), macro_text: None },

        // Numpad movement macros (no \r needed - network module adds \n automatically)
        KeyBind { key: "num_1".to_string(), action: None, macro_text: Some("sw".to_string()) },
        KeyBind { key: "num_2".to_string(), action: None, macro_text: Some("s".to_string()) },
        KeyBind { key: "num_3".to_string(), action: None, macro_text: Some("se".to_string()) },
        KeyBind { key: "num_4".to_string(), action: None, macro_text: Some("w".to_string()) },
        KeyBind { key: "num_5".to_string(), action: None, macro_text: Some("out".to_string()) },
        KeyBind { key: "num_6".to_string(), action: None, macro_text: Some("e".to_string()) },
        KeyBind { key: "num_7".to_string(), action: None, macro_text: Some("nw".to_string()) },
        KeyBind { key: "num_8".to_string(), action: None, macro_text: Some("n".to_string()) },
        KeyBind { key: "num_9".to_string(), action: None, macro_text: Some("ne".to_string()) },
        KeyBind { key: "num_0".to_string(), action: None, macro_text: Some("down".to_string()) },
        KeyBind { key: "num_.".to_string(), action: None, macro_text: Some("up".to_string()) },
        KeyBind { key: "num_+".to_string(), action: None, macro_text: Some("look".to_string()) },
        KeyBind { key: "num_-".to_string(), action: None, macro_text: Some("info".to_string()) },
        KeyBind { key: "num_*".to_string(), action: None, macro_text: Some("exp".to_string()) },
        KeyBind { key: "num_/".to_string(), action: None, macro_text: Some("health".to_string()) },

        // Note: Shift+numpad doesn't work on Windows - the OS doesn't report SHIFT modifier for numpad numeric keys
        // If you want peer keybinds, use alt+numpad or ctrl+numpad instead (those modifiers work with numpad)
    ]
}

fn default_countdown_icon() -> String {
    "\u{f0c8}".to_string()  // Nerd Font square icon
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
        title: Some("Command".to_string()),
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
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .context("Failed to read config file")?;
            let mut config: Config = toml::from_str(&contents)
                .context("Failed to parse config file")?;

            // If keybinds is empty, populate with defaults and save
            if config.keybinds.is_empty() {
                config.keybinds = default_keybinds();
                config.save()?;
            }

            Ok(config)
        } else {
            // Create default config
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

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

    fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        Ok(home.join(".profanity-rs").join("config.toml"))
    }

    fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        Ok(home.join(".profanity-rs"))
    }

    /// Save just the window layout to a named file
    pub fn save_layout(&self, name: &str) -> Result<()> {
        let layout_path = Self::layout_path(name)?;

        // Ensure directory exists
        if let Some(parent) = layout_path.parent() {
            fs::create_dir_all(parent)?;
        }

        #[derive(serde::Serialize)]
        struct LayoutData {
            windows: Vec<WindowDef>,
        }

        let layout_config = LayoutData {
            windows: self.ui.windows.clone(),
        };

        let contents = toml::to_string_pretty(&layout_config)
            .context("Failed to serialize layout")?;
        fs::write(&layout_path, contents)
            .context("Failed to write layout file")?;

        Ok(())
    }

    /// Load window layout from a named file
    pub fn load_layout(&mut self, name: &str) -> Result<()> {
        let layout_path = Self::layout_path(name)?;

        if !layout_path.exists() {
            anyhow::bail!("Layout '{}' not found", name);
        }

        let contents = fs::read_to_string(&layout_path)
            .context("Failed to read layout file")?;

        #[derive(serde::Deserialize)]
        struct LayoutData {
            windows: Vec<WindowDef>,
        }

        let layout: LayoutData = toml::from_str(&contents)
            .context("Failed to parse layout file")?;

        self.ui.windows = layout.windows;
        Ok(())
    }

    /// Save autosave layout (current window positions)
    pub fn autosave_layout(&self) -> Result<()> {
        self.save_layout("autosave")
    }

    /// Load autosave layout if it exists
    pub fn load_autosave_layout(&mut self) -> Result<bool> {
        let layout_path = Self::layout_path("autosave")?;
        if layout_path.exists() {
            self.load_layout("autosave")?;
            Ok(true)
        } else {
            Ok(false)
        }
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

    fn layout_path(name: &str) -> Result<PathBuf> {
        let config_dir = Self::config_dir()?;
        Ok(config_dir.join("layouts").join(format!("{}.toml", name)))
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
                windows: default_windows(),
                mouse_mode_toggle_key: default_mouse_mode_toggle_key(),
                countdown_icon: default_countdown_icon(),
                command_input: default_command_input(),
            },
            presets: vec![
                PresetColor { id: "whisper".to_string(), fg: Some("#60b4bf".to_string()), bg: None },
                PresetColor { id: "links".to_string(), fg: Some("#477ab3".to_string()), bg: None },
                PresetColor { id: "speech".to_string(), fg: Some("#53a684".to_string()), bg: None },
                PresetColor { id: "roomName".to_string(), fg: Some("#9BA2B2".to_string()), bg: Some("#395573".to_string()) },
                PresetColor { id: "monsterbold".to_string(), fg: Some("#a29900".to_string()), bg: None },
                PresetColor { id: "familiar".to_string(), fg: Some("#767339".to_string()), bg: None },
                PresetColor { id: "thought".to_string(), fg: Some("#FF8080".to_string()), bg: None },
            ],
            highlights: vec![
                // Example: Fast highlight for multiple player names (ultra-fast with Aho-Corasick)
                HighlightPattern {
                    pattern: "Alice|Bob|Charlie|David|Eve|Frank".to_string(),
                    fg: Some("#ff00ff".to_string()),
                    bg: None,
                    bold: true,
                    color_entire_line: false,
                    fast_parse: true,  // Enables Aho-Corasick for blazing speed
                },
                // Example: Highlight your combat actions in red (partial line, regex)
                HighlightPattern {
                    pattern: r"You swing.*".to_string(),
                    fg: Some("#ff0000".to_string()),
                    bg: None,
                    bold: true,
                    color_entire_line: false,
                    fast_parse: false,
                },
                // Example: Highlight damage numbers in yellow (partial line, regex)
                HighlightPattern {
                    pattern: r"\d+ points? of damage".to_string(),
                    fg: Some("#ffff00".to_string()),
                    bg: None,
                    bold: true,
                    color_entire_line: false,
                    fast_parse: false,
                },
                // Example: Highlight death messages with bright background (whole line, regex)
                HighlightPattern {
                    pattern: r".*dies.*".to_string(),
                    fg: Some("#ffffff".to_string()),
                    bg: Some("#ff0000".to_string()),
                    bold: true,
                    color_entire_line: true,
                    fast_parse: false,
                },
            ],
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
        }
    }
}
