use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardIndicatorDef {
    pub id: String,              // e.g., "poisoned", "diseased"
    pub icon: String,            // Unicode icon character
    pub colors: Vec<String>,     // [off_color, on_color]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBind {
    pub key: String,
    pub action: String,
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

fn default_countdown_icon() -> String {
    "\u{f0c8}".to_string()  // Nerd Font square icon
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
            title: None,
            bar_color: None,
            bar_background_color: None,
            transparent_background: true,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("Health".to_string()),
            bar_color: Some("#6e0202".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("Mana".to_string()),
            bar_color: Some("#08086d".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("Stamina".to_string()),
            bar_color: Some("#bd7b00".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("Spirit".to_string()),
            bar_color: Some("#6e727c".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("Mind".to_string()),
            bar_color: Some("#008b8b".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("Stance".to_string()),
            bar_color: Some("#000080".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("Encumbrance".to_string()),
            bar_color: Some("#006400".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("RT".to_string()),
            bar_color: Some("#ff0000".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("Cast".to_string()),
            bar_color: Some("#0000ff".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: Some("Stun".to_string()),
            bar_color: Some("#ffff00".to_string()),
            bar_background_color: Some("#000000".to_string()),
            transparent_background: false,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: None,
            bar_color: None,
            bar_background_color: None,
            transparent_background: true,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
            title: None,
            bar_color: None,
            bar_background_color: None,
            transparent_background: true,
            indicator_colors: None,
            dashboard_layout: None,
            dashboard_indicators: None,
            dashboard_spacing: None,
            dashboard_hide_inactive: None,
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
                title: Some("Main".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Thoughts".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Speech".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Familiar".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Room".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Logons".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Deaths".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Arrivals".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Ambients".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Announcements".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Loot".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Health".to_string()),
                bar_color: Some("#6e0202".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Mana".to_string()),
                bar_color: Some("#08086d".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Stamina".to_string()),
                bar_color: Some("#bd7b00".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Spirit".to_string()),
                bar_color: Some("#6e727c".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Mind".to_string()),
                bar_color: Some("#008b8b".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Encumbrance".to_string()),
                bar_color: Some("#006400".to_string()), // Will change dynamically based on value
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Stance".to_string()),
                bar_color: Some("#000080".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Blood Points".to_string()),
                bar_color: Some("#4d0085".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("RT".to_string()),
                bar_color: Some("#ff0000".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Cast".to_string()),
                bar_color: Some("#0000ff".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Stun".to_string()),
                bar_color: Some("#ffff00".to_string()),
                bar_background_color: Some("#000000".to_string()),
                transparent_background: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Exits".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Injuries".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Hands".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Left Hand".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Right Hand".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("Spell".to_string()),
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("\u{e231}".to_string()), // Nerd Font poison icon
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#00ff00".to_string()]), // off, green
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("\u{e286}".to_string()), // Nerd Font disease icon
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#8b4513".to_string()]), // off, brownish-red
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("\u{f043}".to_string()), // Nerd Font bleeding icon
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#ff0000".to_string()]), // off, red
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("\u{f0e7}".to_string()), // Nerd Font stunned icon
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#ffff00".to_string()]), // off, yellow
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
                title: Some("\u{f0bca}".to_string()), // Nerd Font webbed icon
                bar_color: None,
                bar_background_color: None,
                transparent_background: true,
                indicator_colors: Some(vec!["#000000".to_string(), "#cccccc".to_string()]), // off, bright grey
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
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
        ]
    }

    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .context("Failed to read config file")?;
            let config: Config = toml::from_str(&contents)
                .context("Failed to parse config file")?;
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
                HighlightPattern {
                    pattern: r"You swing.*".to_string(),
                    fg: Some("#ff0000".to_string()),
                    bg: None,
                    bold: true,
                },
            ],
            keybinds: vec![],
        }
    }
}
