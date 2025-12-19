//! Configuration loader/writer plus strongly typed settings structures.
//!
//! This module deserializes every TOML blob we ship (config, highlights,
//! keybinds, colors, layouts, etc.), exposes helpers for resolving per-character
//! overrides, and persists edits that come from the UI.

use anyhow::{Context, Result};
use crate::frontend::common::{KeyCode, KeyModifiers};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub mod menu_keybind_validator;
mod highlights;
mod keybinds;

pub use highlights::{EventAction, EventPattern, HighlightPattern, RedirectMode};
pub use keybinds::{
    parse_key_string, GlobalKeybinds, KeyAction, KeyBindAction, MacroAction, MenuKeybinds,
};

// Embed default configuration files at compile time
const DEFAULT_CONFIG: &str = include_str!("../defaults/config.toml");
const DEFAULT_COLORS: &str = include_str!("../defaults/colors.toml");
const DEFAULT_HIGHLIGHTS: &str = include_str!("../defaults/highlights.toml");
const DEFAULT_KEYBINDS: &str = include_str!("../defaults/keybinds.toml");
const DEFAULT_CMDLIST: &str = include_str!("../defaults/cmdlist1.xml");

// Embed entire directories - automatically includes all files
static LAYOUTS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/defaults/layouts");
static SOUNDS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/defaults/sounds");

// Keep embedded default layout for fallback
const LAYOUT_DEFAULT: &str = include_str!("../defaults/layouts/layout.toml");

/// Widget category for organizing windows in menus
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WidgetCategory {
    ActiveEffects,
    Countdown,
    Entity,
    Hand,
    Other,
    ProgressBar,
    Status,
    TextWindow,
}

impl WidgetCategory {
    pub fn display_name(&self) -> &str {
        match self {
            Self::ActiveEffects => "Active Effects",
            Self::Countdown => "Countdowns",
            Self::Entity => "Entities",
            Self::Hand => "Hands",
            Self::Other => "Other",
            Self::ProgressBar => "Progress Bars",
            Self::Status => "Status",
            Self::TextWindow => "Text Windows",
        }
    }

    pub fn from_widget_type(widget_type: &str) -> Self {
        match widget_type {
            "countdown" => Self::Countdown,
            "hand" => Self::Hand,
            "active_effects" => Self::ActiveEffects,
            "indicator" | "dashboard" => Self::Status,
            "progress" => Self::ProgressBar,
            "text" | "tabbedtext" => Self::TextWindow,
            "targets" | "players" => Self::Entity,
            _ => Self::Other,
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
    /// True color RGB (24-bit) - requires modern terminal
    #[default]
    Direct,
    /// 256-color palette indices - for legacy terminals or SSH sessions
    Slot,
}

impl std::fmt::Display for ColorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColorMode::Direct => write!(f, "direct"),
            ColorMode::Slot => write!(f, "slot"),
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
    #[serde(skip)] // Loaded from [global] section of keybinds.toml
    pub global_keybinds: GlobalKeybinds,
    #[serde(default)]
    pub sound: SoundConfig,
    #[serde(default)]
    pub tts: TtsConfig,
    #[serde(default)]
    pub event_patterns: HashMap<String, EventPattern>,
    #[serde(default)]
    pub layout_mappings: Vec<LayoutMapping>,
    #[serde(skip)] // Don't serialize/deserialize this - it's set at runtime
    pub character: Option<String>, // Character name for character-specific saving
    #[serde(skip)] // Loaded from separate colors.toml file (includes color_palette)
    pub colors: ColorConfig, // All color configuration (presets, prompt_colors, ui colors, spell colors, color_palette)
    #[serde(default)] // Use defaults for menu keybinds
    pub menu_keybinds: MenuKeybinds, // Keybinds for menu system (browsers, forms, editors)
    #[serde(default = "default_theme_name")] // Default to "dark" theme
    pub active_theme: String, // Currently active theme name
}

/// Terminal size range to layout mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutMapping {
    pub min_width: u16,
    pub min_height: u16,
    pub max_width: u16,
    pub max_height: u16,
    pub layout: String, // Layout name (e.g., "compact1", "half_screen")
}

impl LayoutMapping {
    /// Check if terminal size matches this mapping
    pub fn matches(&self, width: u16, height: u16) -> bool {
        width >= self.min_width
            && width <= self.max_width
            && height >= self.min_height
            && height <= self.max_height
    }
}

/// Named color in the user's palette
///
/// Each color can optionally be assigned a terminal palette slot (16-231)
/// for use with `.setpalette` command in Slot color mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaletteColor {
    pub name: String,
    pub color: String,    // Hex color code
    pub category: String, // Color family: "red", "blue", "green", etc.
    #[serde(default)]
    pub favorite: bool,
    /// Terminal palette slot (16-231) for .setpalette command
    /// Slots 0-15 are standard ANSI colors and should be avoided
    /// Slots 16-231 are the 6x6x6 color cube
    /// Slots 232-255 are the grayscale ramp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u8>,
}

impl PaletteColor {
    pub fn new(name: &str, color: &str, category: &str) -> Self {
        Self {
            name: name.to_string(),
            color: color.to_string(),
            category: category.to_string(),
            favorite: false,
            slot: None,
        }
    }

    /// Create a palette color with a specific terminal slot assignment
    pub fn with_slot(name: &str, color: &str, category: &str, slot: u8) -> Self {
        Self {
            name: name.to_string(),
            color: color.to_string(),
            category: category.to_string(),
            favorite: false,
            slot: Some(slot),
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    // Legacy field for backwards compatibility - maps to fg if present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpellColorRange {
    pub spells: Vec<u32>, // List of spell IDs (e.g., [101, 107, 120, 140, 150])
    #[serde(default)]
    pub color: String, // Legacy field: bar color (for backward compatibility)
    #[serde(default)]
    pub bar_color: Option<String>, // Progress bar fill color (e.g., "#00ffff")
    #[serde(default)]
    pub text_color: Option<String>, // Text color on filled portion (default: white)
    #[serde(default)]
    pub bg_color: Option<String>, // Background/unfilled portion color (default: black)
}

#[derive(Debug, Clone)]
pub struct SpellColorStyle {
    pub bar_color: Option<String>,
    pub text_color: Option<String>,
}

impl SpellColorRange {
    pub fn style(&self) -> SpellColorStyle {
        let bar_color = self
            .bar_color
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                let legacy = self.color.trim();
                if legacy.is_empty() {
                    None
                } else {
                    Some(self.color.clone())
                }
            });

        let text_color = self.text_color.clone().filter(|s| !s.trim().is_empty());

        SpellColorStyle {
            bar_color,
            text_color,
        }
    }
}

/// UI color configuration - global defaults for all widgets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiColors {
    #[serde(default = "default_command_echo_color")]
    pub command_echo_color: String,
    #[serde(default = "default_border_color_default")]
    pub border_color: String, // Default border color for all widgets
    #[serde(default = "default_focused_border_color")]
    pub focused_border_color: String, // Border color for focused/active windows
    #[serde(default = "default_text_color_default")]
    pub text_color: String, // Default text color for all widgets
    #[serde(default = "default_background_color")]
    pub background_color: String, // Default background color for all widgets
    #[serde(default = "default_selection_bg_color")]
    pub selection_bg_color: String, // Text selection background color
    #[serde(default = "default_textarea_background")]
    pub textarea_background: String, // Background color for input textareas in forms/browsers
}

/// Color configuration - separate file (colors.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    #[serde(default)]
    pub presets: HashMap<String, PresetColor>,
    #[serde(default)]
    pub prompt_colors: Vec<PromptColor>,
    #[serde(default)]
    pub ui: UiColors,
    // Spell colors are managed by .addspellcolor/.spellcolors but stored here
    #[serde(default)]
    pub spell_colors: Vec<SpellColorRange>,
    // Color palette for .colors browser
    #[serde(default)]
    pub color_palette: Vec<PaletteColor>,
}

impl Default for UiColors {
    fn default() -> Self {
        Self {
            command_echo_color: default_command_echo_color(),
            border_color: default_border_color_default(),
            focused_border_color: default_focused_border_color(),
            text_color: default_text_color_default(),
            background_color: default_background_color(),
            selection_bg_color: default_selection_bg_color(),
            textarea_background: default_textarea_background(),
        }
    }
}

impl Default for ColorConfig {
    fn default() -> Self {
        // Parse from embedded default colors.toml
        toml::from_str(DEFAULT_COLORS).unwrap_or_else(|e| {
            eprintln!("Failed to parse embedded colors.toml: {}", e);
            Self {
                presets: HashMap::new(),
                prompt_colors: Vec::new(),
                ui: UiColors::default(),
                spell_colors: Vec::new(),
                color_palette: Vec::new(),
            }
        })
    }
}

impl ColorConfig {
    /// Load colors from colors.toml for a character
    pub fn load(character: Option<&str>) -> Result<Self> {
        let colors_path = Config::colors_path(character)?;

        if colors_path.exists() {
            let contents =
                fs::read_to_string(&colors_path).context("Failed to read colors.toml")?;
            let mut colors: ColorConfig =
                toml::from_str(&contents).context("Failed to parse colors.toml")?;

            // Merge defaults for missing color_palette (for backward compatibility)
            if colors.color_palette.is_empty() {
                let defaults = Self::default();
                colors.color_palette = defaults.color_palette;
            }

            Ok(colors)
        } else {
            // Return default if file doesn't exist (will be created by extract_defaults)
            Ok(Self::default())
        }
    }

    /// Save colors to colors.toml for a character
    pub fn save(&self, character: Option<&str>) -> Result<()> {
        let colors_path = Config::colors_path(character)?;
        let contents = toml::to_string_pretty(self).context("Failed to serialize colors")?;
        fs::write(&colors_path, contents).context("Failed to write colors.toml")?;
        Ok(())
    }
}

/// Helper functions for loading/saving highlights and keybinds
impl Config {
    /// Resolve a color name from the palette, or return the original string if it's already a hex code
    ///
    /// # Examples
    /// - Input: "Primary Blue" (if in palette) → Output: "#0066cc"
    /// - Input: "#ff0000" → Output: "#ff0000" (pass-through)
    /// - Input: "Unknown Name" → Output: "Unknown Name" (pass-through)
    pub fn resolve_palette_color(&self, input: &str) -> String {
        let trimmed = input.trim();

        // If it's already a hex code (starts with #), return as-is
        if trimmed.starts_with('#') {
            return trimmed.to_string();
        }

        // Try to find matching color in palette (case-insensitive)
        let input_lower = trimmed.to_lowercase();
        for palette_color in &self.colors.color_palette {
            if palette_color.name.to_lowercase() == input_lower {
                return palette_color.color.clone();
            }
        }

        // Not found in palette - return original input
        trimmed.to_string()
    }

}

/// Border sides configuration - which borders to show
/// Serializes to/from array of strings in TOML: ["left", "right", "top", "bottom"]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "Vec<String>", into = "Vec<String>")]
pub struct BorderSides {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl Default for BorderSides {
    fn default() -> Self {
        Self {
            top: true,
            bottom: true,
            left: true,
            right: true,
        }
    }
}

// Convert from TOML array format ["left", "right"] to BorderSides struct
impl From<Vec<String>> for BorderSides {
    fn from(sides: Vec<String>) -> Self {
        let mut border = Self {
            top: false,
            bottom: false,
            left: false,
            right: false,
        };

        for side in sides {
            match side.to_lowercase().as_str() {
                "top" => border.top = true,
                "bottom" => border.bottom = true,
                "left" => border.left = true,
                "right" => border.right = true,
                _ => {} // Ignore unknown sides
            }
        }

        border
    }
}

// Convert from BorderSides struct to TOML array format
impl From<BorderSides> for Vec<String> {
    fn from(border: BorderSides) -> Self {
        let mut sides = Vec::new();
        if border.top {
            sides.push("top".to_string());
        }
        if border.bottom {
            sides.push("bottom".to_string());
        }
        if border.left {
            sides.push("left".to_string());
        }
        if border.right {
            sides.push("right".to_string());
        }
        sides
    }
}

impl BorderSides {
    /// True if any side is enabled
    pub fn any(&self) -> bool {
        self.top || self.bottom || self.left || self.right
    }
}

impl WindowBase {
    fn horizontal_border_units_for(show: bool, sides: &BorderSides) -> u16 {
        if !show {
            return 0;
        }
        (sides.top as u16) + (sides.bottom as u16)
    }

    fn vertical_border_units_for(show: bool, sides: &BorderSides) -> u16 {
        if !show {
            return 0;
        }
        (sides.left as u16) + (sides.right as u16)
    }

    /// Number of rows consumed by borders (top + bottom)
    pub fn horizontal_border_units(&self) -> u16 {
        Self::horizontal_border_units_for(self.show_border, &self.border_sides)
    }

    /// Number of columns consumed by borders (left + right)
    pub fn vertical_border_units(&self) -> u16 {
        Self::vertical_border_units_for(self.show_border, &self.border_sides)
    }

    /// Rows available for the widget's interior content
    pub fn content_rows(&self) -> u16 {
        self.rows.saturating_sub(self.horizontal_border_units())
    }

    /// Columns available for the widget's interior content
    pub fn content_cols(&self) -> u16 {
        self.cols.saturating_sub(self.vertical_border_units())
    }

    /// Apply new border visibility/sides while keeping interior size the same.
    pub fn apply_border_configuration(&mut self, show_border: bool, border_sides: BorderSides) {
        let prev_horizontal = self.horizontal_border_units();
        let prev_vertical = self.vertical_border_units();

        let mut content_rows = self.rows.saturating_sub(prev_horizontal);
        if content_rows == 0 {
            content_rows = 1;
        }
        let mut content_cols = self.cols.saturating_sub(prev_vertical);
        if content_cols == 0 {
            content_cols = 1;
        }

        self.show_border = show_border && border_sides.any();
        self.border_sides = border_sides;

        let new_horizontal =
            Self::horizontal_border_units_for(self.show_border, &self.border_sides);
        let new_vertical = Self::vertical_border_units_for(self.show_border, &self.border_sides);

        self.rows = content_rows + new_horizontal;
        self.cols = content_cols + new_vertical;

        if let Some(min_rows) = self.min_rows {
            if self.rows < min_rows {
                self.rows = min_rows;
            }
        } else if self.rows == 0 {
            self.rows = 1;
        }
        if let Some(max_rows) = self.max_rows {
            if self.rows > max_rows {
                self.rows = max_rows;
            }
        }

        if let Some(min_cols) = self.min_cols {
            if self.cols < min_cols {
                self.cols = min_cols;
            }
        } else if self.cols == 0 {
            self.cols = 1;
        }
        if let Some(max_cols) = self.max_cols {
            if self.cols > max_cols {
                self.cols = max_cols;
            }
        }
    }
}

/// Base configuration shared by ALL widget types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowBase {
    pub name: String,
    #[serde(default)]
    pub row: u16,
    #[serde(default)]
    pub col: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_show_border")]
    pub show_border: bool,
    #[serde(default = "default_border_style")]
    pub border_style: String, // "single", "double", "rounded", "thick", "plain"
    #[serde(default)]
    pub border_sides: BorderSides,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default = "default_show_title")]
    pub show_title: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default = "default_title_position")]
    pub title_position: String,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub text_color: Option<String>,
    #[serde(default = "default_transparent_background")]
    pub transparent_background: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub min_rows: Option<u16>,
    #[serde(default)]
    pub max_rows: Option<u16>,
    #[serde(default)]
    pub min_cols: Option<u16>,
    #[serde(default)]
    pub max_cols: Option<u16>,
    /// Whether this window is currently visible (defaults to true for backwards compatibility)
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Content alignment within widget area
    #[serde(default)]
    pub content_align: Option<String>,
}

/// Text widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextWidgetData {
    #[serde(default)]
    pub streams: Vec<String>,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default = "default_true")]
    pub wordwrap: bool,
    #[serde(default)]
    pub show_timestamps: bool,
}

/// Room widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomWidgetData {
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,

    /// Component visibility toggles (default: all true)
    #[serde(default = "default_true")]
    pub show_desc: bool,

    #[serde(default = "default_true")]
    pub show_objs: bool,

    #[serde(default = "default_true")]
    pub show_players: bool,

    #[serde(default = "default_true")]
    pub show_exits: bool,

    /// Display the room name within the window content (useful when borders are hidden)
    #[serde(default = "default_false")]
    pub show_name: bool,
}

/// Command input widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CommandInputWidgetData {
    #[serde(default)]
    pub text_color: Option<String>,
    #[serde(default)]
    pub cursor_color: Option<String>,
    #[serde(default)]
    pub cursor_background_color: Option<String>,
    #[serde(default)]
    pub prompt_icon: Option<String>,
    #[serde(default)]
    pub prompt_icon_color: Option<String>,
}

/// Inventory widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryWidgetData {
    #[serde(default)]
    pub streams: Vec<String>,
    #[serde(default)]
    pub buffer_size: usize,
    #[serde(default = "default_true")]
    pub wordwrap: bool,
    #[serde(default)]
    pub show_timestamps: bool,
}

/// TabbedText widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabbedTextWidgetData {
    #[serde(default)]
    pub tabs: Vec<TabbedTextTab>,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default = "default_tab_bar_position")]
    pub tab_bar_position: String,
    #[serde(default)]
    pub tab_separator: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_active_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_inactive_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_unread_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_unread_prefix: Option<String>,
}

fn default_tab_bar_position() -> String {
    "top".to_string()
}

fn default_title_position() -> String {
    "top-left".to_string()
}

/// Tab configuration for TabbedText widget
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabbedTextTab {
    pub name: String,
    /// Single stream (for compatibility) - converts to streams array
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    /// Multiple streams (preferred) - if both set, this takes precedence
    #[serde(default)]
    pub streams: Vec<String>,
    /// Show timestamps (overrides ui.show_timestamps if Some)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_timestamps: Option<bool>,
    /// Ignore activity/unread indicators for this tab
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_activity: Option<bool>,
}

impl TabbedTextTab {
    /// Get the list of streams for this tab
    /// Handles both `stream` (singular) and `streams` (plural) fields
    pub fn get_streams(&self) -> Vec<String> {
        if !self.streams.is_empty() {
            self.streams.clone()
        } else if let Some(stream) = &self.stream {
            vec![stream.clone()]
        } else {
            vec![]
        }
    }
}

/// Progress bar widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressWidgetData {
    /// Progress feed identifier (XML progressBar id); case-sensitive
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub numbers_only: bool,
    /// When true, show only the current value (no label, no max)
    #[serde(default)]
    pub current_only: bool,
}

/// Countdown timer widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountdownWidgetData {
    /// Countdown feed identifier (XML id), case-sensitive
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<char>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub background_color: Option<String>,
}

/// Compass widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompassWidgetData {
    #[serde(default)]
    pub active_color: Option<String>, // Color for available exits (default: green)
    #[serde(default)]
    pub inactive_color: Option<String>, // Color for unavailable exits (default: dark gray)
}

/// Injury doll widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InjuryDollWidgetData {
    #[serde(default)]
    pub injury_default_color: Option<String>, // Level 0: none (default: #333333)
    #[serde(default)]
    pub injury1_color: Option<String>, // Level 1: injury 1 (default: #aa5500)
    #[serde(default)]
    pub injury2_color: Option<String>, // Level 2: injury 2 (default: #ff8800)
    #[serde(default)]
    pub injury3_color: Option<String>, // Level 3: injury 3 (default: #ff0000)
    #[serde(default)]
    pub scar1_color: Option<String>, // Level 4: scar 1 (default: #999999)
    #[serde(default)]
    pub scar2_color: Option<String>, // Level 5: scar 2 (default: #777777)
    #[serde(default)]
    pub scar3_color: Option<String>, // Level 6: scar 3 (default: #555555)
}

/// Indicator widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndicatorWidgetData {
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub indicator_id: Option<String>,
    #[serde(default = "default_indicator_inactive_color")]
    pub inactive_color: Option<String>,
    #[serde(default = "default_indicator_active_color")]
    pub active_color: Option<String>,
    #[serde(default)]
    pub default_status: Option<String>, // legacy
    #[serde(default)]
    pub default_color: Option<String>,  // legacy
}

/// Dashboard widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardWidgetData {
    /// Layout direction: "horizontal", "vertical", or "grid:RxC"
    #[serde(default = "default_dashboard_layout", rename = "dashboard_layout")]
    pub layout: String,
    /// Spacing between indicators (characters)
    #[serde(default = "default_dashboard_spacing", rename = "dashboard_spacing")]
    pub spacing: u16,
    /// Hide inactive indicators (value = 0)
    #[serde(
        default = "default_dashboard_hide_inactive",
        rename = "dashboard_hide_inactive"
    )]
    pub hide_inactive: bool,
    /// Indicator definitions (id/icon/colors)
    #[serde(default, rename = "dashboard_indicators")]
    pub indicators: Vec<DashboardIndicatorDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardIndicatorDef {
    pub id: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub colors: Vec<String>,
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

fn default_dashboard_layout() -> String {
    "horizontal".to_string()
}

fn default_dashboard_spacing() -> u16 {
    1
}

fn default_dashboard_hide_inactive() -> bool {
    false
}

pub(crate) fn default_target_entity_id() -> String {
    "targetcount".to_string()
}

pub(crate) fn default_player_entity_id() -> String {
    "playercount".to_string()
}

fn default_indicator_active_color() -> Option<String> {
    Some("#00ff00".to_string())
}

fn default_indicator_inactive_color() -> Option<String> {
    Some("#555555".to_string())
}

/// Hand widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandWidgetData {
    /// Optional icon prefix (e.g., "L:", "R:", "S:")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Icon color (falls back to window/text color if None)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    /// Text color override (also overrides link color if set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
}

/// Active effects widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveEffectsWidgetData {
    pub category: String, // "Buffs", "Debuffs", "Cooldowns", "ActiveSpells"
}

/// Performance widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceWidgetData {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub show_fps: bool,
    #[serde(default = "default_true")]
    pub show_frame_times: bool,
    #[serde(default = "default_true")]
    pub show_render_times: bool,
    #[serde(default = "default_true")]
    pub show_ui_times: bool,
    #[serde(default = "default_true")]
    pub show_wrap_times: bool,
    #[serde(default = "default_true")]
    pub show_net: bool,
    #[serde(default = "default_true")]
    pub show_parse: bool,
    #[serde(default = "default_true")]
    pub show_events: bool,
    #[serde(default = "default_true")]
    pub show_memory: bool,
    #[serde(default = "default_true")]
    pub show_lines: bool,
    #[serde(default = "default_true")]
    pub show_uptime: bool,
    #[serde(default = "default_true")]
    pub show_jitter: bool,
    #[serde(default = "default_true")]
    pub show_frame_spikes: bool,
    #[serde(default = "default_true")]
    pub show_event_lag: bool,
    #[serde(default = "default_true")]
    pub show_memory_delta: bool,
}

/// Targets widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetsWidgetData {
    #[serde(default = "default_target_entity_id")]
    pub entity_id: String,
}

/// Players widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayersWidgetData {
    #[serde(default = "default_player_entity_id")]
    pub entity_id: String,
}

/// Spacer widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpacerWidgetData {
    // No extra fields currently
}

/// Spells window widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpellsWidgetData {
    // No extra fields currently - uses "spells" stream
}

/// Window definition - enum with widget-specific variants
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "widget_type")]
pub enum WindowDef {
    #[serde(rename = "text")]
    Text {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: TextWidgetData,
    },

    #[serde(rename = "tabbedtext")]
    TabbedText {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: TabbedTextWidgetData,
    },

    #[serde(rename = "room")]
    Room {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: RoomWidgetData,
    },

    #[serde(rename = "inventory")]
    Inventory {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: InventoryWidgetData,
    },

    #[serde(rename = "command_input")]
    CommandInput {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: CommandInputWidgetData,
    },

    #[serde(rename = "progress")]
    Progress {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: ProgressWidgetData,
    },

    #[serde(rename = "countdown")]
    Countdown {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: CountdownWidgetData,
    },

    #[serde(rename = "compass")]
    Compass {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: CompassWidgetData,
    },

    #[serde(rename = "injury_doll")]
    InjuryDoll {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: InjuryDollWidgetData,
    },

    #[serde(rename = "indicator")]
    Indicator {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: IndicatorWidgetData,
    },

    #[serde(rename = "dashboard")]
    Dashboard {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: DashboardWidgetData,
    },

    #[serde(rename = "hand")]
    Hand {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: HandWidgetData,
    },

    #[serde(rename = "active_effects")]
    ActiveEffects {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: ActiveEffectsWidgetData,
    },
    #[serde(rename = "performance")]
    Performance {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: PerformanceWidgetData,
    },

    #[serde(rename = "targets")]
    Targets {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: TargetsWidgetData,
    },

    #[serde(rename = "players")]
    Players {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: PlayersWidgetData,
    },

    #[serde(rename = "spacer")]
    Spacer {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: SpacerWidgetData,
    },

    #[serde(rename = "spells")]
    Spells {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: SpellsWidgetData,
    },
}

impl WindowDef {
    /// Get the window name
    pub fn name(&self) -> &str {
        match self {
            WindowDef::Text { base, .. } => &base.name,
            WindowDef::TabbedText { base, .. } => &base.name,
            WindowDef::Room { base, .. } => &base.name,
            WindowDef::Inventory { base, .. } => &base.name,
            WindowDef::CommandInput { base, .. } => &base.name,
            WindowDef::Progress { base, .. } => &base.name,
            WindowDef::Countdown { base, .. } => &base.name,
            WindowDef::Compass { base, .. } => &base.name,
            WindowDef::Indicator { base, .. } => &base.name,
            WindowDef::Dashboard { base, .. } => &base.name,
            WindowDef::InjuryDoll { base, .. } => &base.name,
            WindowDef::Hand { base, .. } => &base.name,
            WindowDef::ActiveEffects { base, .. } => &base.name,
            WindowDef::Performance { base, .. } => &base.name,
            WindowDef::Targets { base, .. } => &base.name,
            WindowDef::Players { base, .. } => &base.name,
            WindowDef::Spacer { base, .. } => &base.name,
            WindowDef::Spells { base, .. } => &base.name,
        }
    }

    /// Get the widget type as a string
    pub fn widget_type(&self) -> &str {
        match self {
            WindowDef::Text { .. } => "text",
            WindowDef::TabbedText { .. } => "tabbedtext",
            WindowDef::Room { .. } => "room",
            WindowDef::Inventory { .. } => "inventory",
            WindowDef::CommandInput { .. } => "command_input",
            WindowDef::Progress { .. } => "progress",
            WindowDef::Countdown { .. } => "countdown",
            WindowDef::Compass { .. } => "compass",
            WindowDef::Indicator { .. } => "indicator",
            WindowDef::Dashboard { .. } => "dashboard",
            WindowDef::InjuryDoll { .. } => "injury_doll",
            WindowDef::Hand { .. } => "hand",
            WindowDef::ActiveEffects { .. } => "active_effects",
            WindowDef::Performance { .. } => "performance",
            WindowDef::Targets { .. } => "targets",
            WindowDef::Players { .. } => "players",
            WindowDef::Spacer { .. } => "spacer",
            WindowDef::Spells { .. } => "spells",
        }
    }

    /// Get a reference to the base configuration
    pub fn base(&self) -> &WindowBase {
        match self {
            WindowDef::Text { base, .. } => base,
            WindowDef::TabbedText { base, .. } => base,
            WindowDef::Room { base, .. } => base,
            WindowDef::Inventory { base, .. } => base,
            WindowDef::CommandInput { base, .. } => base,
            WindowDef::Progress { base, .. } => base,
            WindowDef::Countdown { base, .. } => base,
            WindowDef::Compass { base, .. } => base,
            WindowDef::Indicator { base, .. } => base,
            WindowDef::Dashboard { base, .. } => base,
            WindowDef::InjuryDoll { base, .. } => base,
            WindowDef::Hand { base, .. } => base,
            WindowDef::ActiveEffects { base, .. } => base,
            WindowDef::Performance { base, .. } => base,
            WindowDef::Targets { base, .. } => base,
            WindowDef::Players { base, .. } => base,
            WindowDef::Spacer { base, .. } => base,
            WindowDef::Spells { base, .. } => base,
        }
    }

    /// Get a mutable reference to the base configuration
    pub fn base_mut(&mut self) -> &mut WindowBase {
        match self {
            WindowDef::Text { base, .. } => base,
            WindowDef::TabbedText { base, .. } => base,
            WindowDef::Room { base, .. } => base,
            WindowDef::Inventory { base, .. } => base,
            WindowDef::CommandInput { base, .. } => base,
            WindowDef::Progress { base, .. } => base,
            WindowDef::Countdown { base, .. } => base,
            WindowDef::Compass { base, .. } => base,
            WindowDef::Indicator { base, .. } => base,
            WindowDef::Dashboard { base, .. } => base,
            WindowDef::InjuryDoll { base, .. } => base,
            WindowDef::Hand { base, .. } => base,
            WindowDef::ActiveEffects { base, .. } => base,
            WindowDef::Performance { base, .. } => base,
            WindowDef::Targets { base, .. } => base,
            WindowDef::Players { base, .. } => base,
            WindowDef::Spacer { base, .. } => base,
            WindowDef::Spells { base, .. } => base,
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

fn default_show_title() -> bool {
    true
}

fn default_transparent_background() -> bool {
    false
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
    #[serde(default = "default_border_style")]
    pub border_style: String, // Default border style: "single", "double", "rounded", "thick", "none"
    #[serde(default = "default_countdown_icon")]
    pub countdown_icon: String, // Unicode character for countdown blocks (e.g., "\u{f0c8}")
    #[serde(default = "default_poll_timeout_ms")]
    pub poll_timeout_ms: u64, // Event poll timeout in milliseconds (lower = higher FPS, higher CPU)
    // Startup music settings
    #[serde(default = "default_startup_music")]
    pub startup_music: bool, // Play startup music on connection
    #[serde(default = "default_startup_music_file")]
    pub startup_music_file: String, // Sound file to play on startup (without extension)
    // Text selection settings
    #[serde(default = "default_selection_enabled")]
    pub selection_enabled: bool,
    #[serde(default = "default_selection_respect_window_boundaries")]
    pub selection_respect_window_boundaries: bool,
    /// Automatically copy mouse selection to clipboard on mouse-up
    #[serde(default = "default_selection_auto_copy")]
    pub selection_auto_copy: bool,
    // Drag and drop settings
    #[serde(default = "default_drag_modifier_key")]
    pub drag_modifier_key: String, // Modifier key required for drag and drop (e.g., "ctrl", "alt", "shift")
    // Command history settings
    #[serde(default = "default_min_command_length")]
    pub min_command_length: usize, // Minimum command length to save to history (commands shorter than this are not saved)
    // Performance stats settings
    #[serde(default = "default_performance_stats_enabled")]
    pub performance_stats_enabled: bool, // Global toggle for performance overlay
    #[serde(default = "default_perf_stats_x")]
    pub perf_stats_x: u16,
    #[serde(default = "default_perf_stats_y")]
    pub perf_stats_y: u16,
    #[serde(default = "default_perf_stats_width")]
    pub perf_stats_width: u16,
    #[serde(default = "default_perf_stats_height")]
    pub perf_stats_height: u16,
    // Squelch/ignore settings
    #[serde(default = "default_ignores_enabled")]
    pub ignores_enabled: bool, // Global toggle for squelch patterns (can disable without deleting patterns)
    // Color rendering mode
    #[serde(default)]
    pub color_mode: ColorMode, // "direct" (true color) or "slot" (256-color palette)
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            buffer_size: default_buffer_size(),
            show_timestamps: false,
            layout: LayoutConfig::default(),
            border_style: default_border_style(),
            countdown_icon: default_countdown_icon(),
            poll_timeout_ms: default_poll_timeout_ms(),
            startup_music: default_startup_music(),
            startup_music_file: default_startup_music_file(),
            selection_enabled: default_selection_enabled(),
            selection_respect_window_boundaries: default_selection_respect_window_boundaries(),
            selection_auto_copy: default_selection_auto_copy(),
            drag_modifier_key: default_drag_modifier_key(),
            min_command_length: default_min_command_length(),
            performance_stats_enabled: default_performance_stats_enabled(),
            perf_stats_x: default_perf_stats_x(),
            perf_stats_y: default_perf_stats_y(),
            perf_stats_width: default_perf_stats_width(),
            perf_stats_height: default_perf_stats_height(),
            ignores_enabled: default_ignores_enabled(),
            color_mode: ColorMode::default(),
        }
    }
}

// CommandInputConfig removed - command_input is now a regular window in the windows array

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayoutConfig {
    // Layout is now entirely defined by window positions and sizes
    // No global grid needed
}

/// Represents a saved layout (windows only - command_input is just another window)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub windows: Vec<WindowDef>,
    #[serde(default)]
    pub terminal_width: Option<u16>, // Designed terminal width (for resize calculations)
    #[serde(default)]
    pub terminal_height: Option<u16>, // Designed terminal height (for resize calculations)
    #[serde(default)]
    pub base_layout: Option<String>, // Reference to base layout (for auto layouts)
    #[serde(default)]
    pub theme: Option<String>, // Theme applied when this layout was saved
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
            "top-left" | "topleft" => ContentAlign::TopLeft,
            "top" | "top-center" | "topcenter" => ContentAlign::Top,
            "top-right" | "topright" => ContentAlign::TopRight,
            "left" | "center-left" | "centerleft" => ContentAlign::Left,
            "center" => ContentAlign::Center,
            "right" | "center-right" | "centerright" => ContentAlign::Right,
            "bottom-left" | "bottomleft" => ContentAlign::BottomLeft,
            "bottom" | "bottom-center" | "bottomcenter" => ContentAlign::Bottom,
            "bottom-right" | "bottomright" => ContentAlign::BottomRight,
            _ => ContentAlign::TopLeft, // Default
        }
    }

    /// Calculate offset for rendering content within a larger area
    /// Returns (row_offset, col_offset)
    pub fn calculate_offset(
        &self,
        content_width: u16,
        content_height: u16,
        area_width: u16,
        area_height: u16,
    ) -> (u16, u16) {
        let row_offset = match self {
            ContentAlign::TopLeft | ContentAlign::Top | ContentAlign::TopRight => 0,
            ContentAlign::Left | ContentAlign::Center | ContentAlign::Right => {
                (area_height.saturating_sub(content_height)) / 2
            }
            ContentAlign::BottomLeft | ContentAlign::Bottom | ContentAlign::BottomRight => {
                area_height.saturating_sub(content_height)
            }
        };

        let col_offset = match self {
            ContentAlign::TopLeft | ContentAlign::Left | ContentAlign::BottomLeft => 0,
            ContentAlign::Top | ContentAlign::Center | ContentAlign::Bottom => {
                (area_width.saturating_sub(content_width)) / 2
            }
            ContentAlign::TopRight | ContentAlign::Right | ContentAlign::BottomRight => {
                area_width.saturating_sub(content_width)
            }
        };

        (row_offset, col_offset)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundConfig {
    #[serde(default = "default_sound_enabled")]
    pub enabled: bool,
    #[serde(default = "default_sound_volume")]
    pub volume: f32, // Master volume (0.0 to 1.0)
    #[serde(default = "default_sound_cooldown")]
    pub cooldown_ms: u64, // Cooldown between same sound plays (milliseconds)
    #[serde(default)]
    pub disabled: bool, // Skip sound system initialization entirely (for systems without audio)
}

fn default_sound_enabled() -> bool {
    true
}

fn default_sound_volume() -> f32 {
    0.7
}

fn default_sound_cooldown() -> u64 {
    500 // 500ms default cooldown
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: default_sound_enabled(),
            volume: default_sound_volume(),
            cooldown_ms: default_sound_cooldown(),
            disabled: false,
        }
    }
}

/// Text-to-Speech Configuration
///
/// Controls accessibility features for visually impaired users.
/// When disabled (default), has zero performance impact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    #[serde(default = "default_tts_enabled")]
    pub enabled: bool,
    #[serde(default = "default_tts_voice")]
    pub voice: Option<String>, // Voice name (None = system default)
    #[serde(default = "default_tts_rate")]
    pub rate: f32, // Speech rate (0.5 to 2.0, 1.0 = normal)
    #[serde(default = "default_tts_volume")]
    pub volume: f32, // Volume (0.0 to 1.0)
    #[serde(default = "default_tts_speak_thoughts")]
    pub speak_thoughts: bool, // Automatically speak thought window
    #[serde(default = "default_tts_speak_whispers")]
    pub speak_whispers: bool, // Automatically speak whispers
    #[serde(default = "default_tts_speak_main")]
    pub speak_main: bool, // Automatically speak main window
}

fn default_tts_enabled() -> bool {
    false // Disabled by default (opt-in)
}

fn default_tts_voice() -> Option<String> {
    None // Use system default voice
}

fn default_tts_rate() -> f32 {
    1.0 // Normal speech rate
}

fn default_tts_volume() -> f32 {
    1.0 // Full volume
}

fn default_tts_speak_thoughts() -> bool {
    true // Thoughts are high priority for screen reader users
}

fn default_tts_speak_whispers() -> bool {
    true // Whispers are important communications
}

fn default_tts_speak_main() -> bool {
    false // Main window can be overwhelming, off by default
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: default_tts_enabled(),
            voice: default_tts_voice(),
            rate: default_tts_rate(),
            volume: default_tts_volume(),
            speak_thoughts: default_tts_speak_thoughts(),
            speak_whispers: default_tts_speak_whispers(),
            speak_main: default_tts_speak_main(),
        }
    }
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

fn default_poll_timeout_ms() -> u64 {
    16 // 16ms = ~60 FPS, 8ms = ~120 FPS, 4ms = ~240 FPS
}

fn default_background_color() -> String {
    "-".to_string() // transparent/no background
}

fn default_startup_music() -> bool {
    true // Enable by default - nostalgic easter egg
}

fn default_startup_music_file() -> String {
    "wizard_music".to_string() // Default to wizard_music for nostalgia
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

fn default_ignores_enabled() -> bool {
    true
}

// default_command_input* functions removed - command_input is now in windows array

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_theme_name() -> String {
    "dark".to_string()
}

fn default_windows() -> Vec<WindowDef> {
    // Default layout: just main text window and command input
    // Users can add more windows via .addwindow command
    vec![
        Config::get_window_template("main").expect("main template should exist"),
        Config::get_window_template("command_input").expect("command_input template should exist"),
    ]
}

impl Layout {
    /// Load layout from file using new profile-based structure
    /// Priority: ~/.vellum-fe/{character}/layout.toml → ~/.vellum-fe/layouts/layout.toml → embedded
    pub fn load(character: Option<&str>) -> Result<Self> {
        let (layout, _base_name) = Self::load_with_terminal_size(character, None)?;
        Ok(layout)
    }

    /// Load layout with terminal size for auto-selection
    /// Returns (layout, base_layout_name) where base_layout_name is the source layout file name (without .toml)
    ///
    /// New structure:
    /// 1. ~/.vellum-fe/{character}/layout.toml (auto-save from exit)
    /// 2. ~/.vellum-fe/default/layouts/default.toml (shared default)
    /// 3. Embedded default
    pub fn load_with_terminal_size(
        character: Option<&str>,
        terminal_size: Option<(u16, u16)>,
    ) -> Result<(Self, Option<String>)> {
        let profile_dir = Config::profile_dir(character)?;
        let default_profile_dir = Config::profile_dir(None)?; // ~/.vellum-fe/default/
        let _shared_layouts_dir = Config::layouts_dir()?; // ~/.vellum-fe/layouts/ (templates only)

        // 1. Try character auto-save layout: ~/.vellum-fe/{character}/layout.toml
        let auto_layout_path = profile_dir.join("layout.toml");
        if auto_layout_path.exists() {
            tracing::info!("Loading auto-save layout from {:?}", auto_layout_path);
            let mut layout = Self::load_from_file(&auto_layout_path)?;
            let base_name = layout
                .base_layout
                .clone()
                .unwrap_or_else(|| "default".to_string());

            // Check if we need to scale from base layout
            if let Some((curr_width, curr_height)) = terminal_size {
                if let (Some(layout_width), Some(layout_height)) =
                    (layout.terminal_width, layout.terminal_height)
                {
                    if curr_width != layout_width || curr_height != layout_height {
                        tracing::info!(
                            "Terminal size changed from {}x{} to {}x{}, scaling current layout (preserving user customizations like spacers)",
                            layout_width,
                            layout_height,
                            curr_width,
                            curr_height
                        );

                        // DO NOT load base layout - it would overwrite user customizations!
                        // The current layout (with spacers and other customizations) is the correct baseline
                        // Scale the CURRENT layout to the new terminal size
                        layout.scale_to_terminal_size(curr_width, curr_height);
                    }
                }
            }

            return Ok((layout, Some(base_name)));
        }

        // 2. Try default profile auto-save layout: ~/.vellum-fe/default/layout.toml
        let default_path = default_profile_dir.join("layout.toml");
        if default_path.exists() {
            tracing::info!(
                "Loading default profile auto-save layout from {:?}",
                default_path
            );
            let layout = Self::load_from_file(&default_path)?;
            return Ok((layout, Some("layout".to_string())));
        }

        // 3. Fall back to embedded default (should have been extracted by extract_defaults())
        tracing::warn!(
            "No layout found, using embedded default (this should have been extracted!)"
        );
        let layout: Layout =
            toml::from_str(LAYOUT_DEFAULT).context("Failed to parse embedded default layout")?;

        Ok((layout, Some("layout".to_string())))
    }

    /// Scale all windows proportionally to fit new terminal size
    pub fn scale_to_terminal_size(&mut self, new_width: u16, new_height: u16) {
        let base_width = self.terminal_width.unwrap_or(new_width);
        let base_height = self.terminal_height.unwrap_or(new_height);

        if base_width == 0 || base_height == 0 {
            tracing::warn!(
                "Invalid base terminal size ({}x{}), skipping scale",
                base_width,
                base_height
            );
            return;
        }

        let scale_x = new_width as f32 / base_width as f32;
        let scale_y = new_height as f32 / base_height as f32;

        tracing::info!(
            "Scaling layout from {}x{} to {}x{} (scale: {:.2}x, {:.2}y)",
            base_width,
            base_height,
            new_width,
            new_height,
            scale_x,
            scale_y
        );

        for window in &mut self.windows {
            // Capture name and type before mutable borrow
            let window_name = window.name().to_string();
            let window_type = window.widget_type().to_string();

            let base = window.base_mut();
            let old_col = base.col;
            let old_row = base.row;
            let old_cols = base.cols;
            let old_rows = base.rows;

            base.col = (base.col as f32 * scale_x).round() as u16;
            base.row = (base.row as f32 * scale_y).round() as u16;
            base.cols = (base.cols as f32 * scale_x).round() as u16;
            base.rows = (base.rows as f32 * scale_y).round() as u16;

            // Ensure minimum sizes
            if base.cols < 1 {
                base.cols = 1;
            }
            if base.rows < 1 {
                base.rows = 1;
            }

            // Respect min/max constraints if set
            if let Some(min_cols) = base.min_cols {
                if base.cols < min_cols {
                    base.cols = min_cols;
                }
            }
            if let Some(max_cols) = base.max_cols {
                if base.cols > max_cols {
                    base.cols = max_cols;
                }
            }
            if let Some(min_rows) = base.min_rows {
                if base.rows < min_rows {
                    base.rows = min_rows;
                }
            }
            if let Some(max_rows) = base.max_rows {
                if base.rows > max_rows {
                    base.rows = max_rows;
                }
            }

            tracing::debug!(
                "  {} [{}]: pos {}x{} -> {}x{}, size {}x{} -> {}x{}",
                window_name,
                window_type,
                old_col,
                old_row,
                base.col,
                base.row,
                old_cols,
                old_rows,
                base.cols,
                base.rows
            );
        }

        // Update terminal size to new size
        self.terminal_width = Some(new_width);
        self.terminal_height = Some(new_height);
    }

    pub fn load_from_file(path: &std::path::Path) -> Result<Self> {
        let contents =
            fs::read_to_string(path).context(format!("Failed to read layout file: {:?}", path))?;
        let mut layout: Layout = toml::from_str(&contents)
            .context(format!("Failed to parse layout file: {:?}", path))?;

        // Debug: Log what terminal size was loaded
        tracing::debug!(
            "Loaded layout from {:?}: terminal_width={:?}, terminal_height={:?}",
            path,
            layout.terminal_width,
            layout.terminal_height
        );

        // Migration: Ensure command_input exists in windows array with valid values
        if let Some(idx) = layout
            .windows
            .iter()
            .position(|w| w.widget_type() == "command_input")
        {
            // Command input exists but might have invalid values (cols=0, rows=0, etc)
            let cmd_input_base = layout.windows[idx].base_mut();
            if cmd_input_base.cols == 0 || cmd_input_base.rows == 0 {
                tracing::warn!(
                    "Command input has invalid size ({}x{}), fixing with defaults",
                    cmd_input_base.rows,
                    cmd_input_base.cols
                );
                // Get defaults from default_windows()
                if let Some(default_cmd) = default_windows()
                    .into_iter()
                    .find(|w| w.widget_type() == "command_input")
                {
                    let default_base = default_cmd.base();
                    cmd_input_base.row = default_base.row;
                    cmd_input_base.col = default_base.col;
                    cmd_input_base.rows = default_base.rows;
                    cmd_input_base.cols = default_base.cols;
                }
            }
        } else {
            // Command input doesn't exist - add it
            if let Some(cmd_input) = default_windows()
                .into_iter()
                .find(|w| w.widget_type() == "command_input")
            {
                tracing::info!("Migrating command_input to windows array");
                layout.windows.push(cmd_input);
            }
        }

        Ok(layout)
    }

    /// Save layout to file
    /// If force_terminal_size is true, always update terminal_width/height to terminal_size
    /// Save layout to shared layouts directory (.savelayout command)
    /// Saves to: ~/.vellum-fe/default/layouts/{name}.toml
    /// Normalize windows before saving - convert None colors back to "-" to preserve transparency
    fn normalize_windows_for_save(&mut self) {
        // Sort windows: spacers last, others maintain relative order
        // This prevents spacers from appearing first in TOML and overlapping during resize
        self.windows.sort_by_key(|w| {
            if w.widget_type() == "spacer" {
                1 // Spacers go last
            } else {
                0 // All other windows maintain order
            }
        });

        for window in &mut self.windows {
            // Convert None to Some("-") for color fields to preserve transparency setting
            let normalize = |field: &mut Option<String>| {
                if field.is_none() {
                    *field = Some("-".to_string());
                }
            };

            let base = window.base_mut();
            normalize(&mut base.background_color);
            normalize(&mut base.border_color);
            normalize(&mut base.text_color);
        }
    }

    pub fn save(
        &mut self,
        name: &str,
        terminal_size: Option<(u16, u16)>,
        force_terminal_size: bool,
    ) -> Result<()> {
        // Capture terminal size for layout baseline
        if force_terminal_size {
            // Force update terminal size (used by .resize to match resized widgets)
            if let Some((width, height)) = terminal_size {
                tracing::info!(
                    "Forcing layout terminal size to {}x{} (was {:?}x{:?})",
                    width,
                    height,
                    self.terminal_width,
                    self.terminal_height
                );
                self.terminal_width = Some(width);
                self.terminal_height = Some(height);
            }
        } else if self.terminal_width.is_none() || self.terminal_height.is_none() {
            // Only set if not already set
            if let Some((width, height)) = terminal_size {
                self.terminal_width = Some(width);
                self.terminal_height = Some(height);
                tracing::info!(
                    "Set layout terminal size to {}x{} (was not previously set)",
                    width,
                    height
                );
            }
        } else {
            tracing::debug!(
                "Preserving existing layout terminal size: {}x{} (not overwriting with current terminal size)",
                self.terminal_width.unwrap(), self.terminal_height.unwrap()
            );
        }

        // Normalize windows before saving (convert None colors to "-")
        self.normalize_windows_for_save();

        // Save to shared layouts directory: ~/.vellum-fe/default/layouts/{name}.toml
        let layouts_dir = Config::layouts_dir()?;
        fs::create_dir_all(&layouts_dir)?;

        let layout_path = layouts_dir.join(format!("{}.toml", name));
        let toml_string = toml::to_string_pretty(&self).context("Failed to serialize layout")?;
        fs::write(&layout_path, toml_string).context("Failed to write layout file")?;

        tracing::info!("Saved layout '{}' to {:?}", name, layout_path);
        Ok(())
    }

    /// Save as character auto-save layout (on exit/resize)
    /// Saves to: ~/.vellum-fe/{character}/layout.toml
    pub fn save_auto(
        &mut self,
        character: &str,
        base_layout_name: &str,
        terminal_size: Option<(u16, u16)>,
    ) -> Result<()> {
        // Set base_layout reference
        self.base_layout = Some(base_layout_name.to_string());

        // Always update terminal size for auto layouts
        if let Some((width, height)) = terminal_size {
            self.terminal_width = Some(width);
            self.terminal_height = Some(height);
        }

        // Normalize windows before saving (convert None colors to "-")
        self.normalize_windows_for_save();

        // Save to character profile: ~/.vellum-fe/{character}/layout.toml
        let profile_dir = Config::profile_dir(Some(character))?;
        fs::create_dir_all(&profile_dir)?;

        let layout_path = profile_dir.join("layout.toml");
        let toml_string =
            toml::to_string_pretty(&self).context("Failed to serialize auto layout")?;
        fs::write(&layout_path, toml_string).context("Failed to write auto layout file")?;

        tracing::info!(
            "Saved auto layout for {} to {:?} (base: {}, terminal: {:?}x{:?})",
            character,
            layout_path,
            base_layout_name,
            self.terminal_width,
            self.terminal_height
        );

        Ok(())
    }

    /// Validate layout and print results to stdout
    /// Returns Ok(()) if valid (with warnings OK), Err if fatal errors found
    pub fn validate_and_print(&self) -> Result<()> {
        println!("✓ Layout loaded successfully");
        println!("  {} windows defined", self.windows.len());

        // Basic validation checks
        let mut errors = 0;
        let mut warnings = 0;

        for window in &self.windows {
            let name = window.name();
            let base = window.base();

            // Check for zero dimensions
            if base.rows == 0 {
                eprintln!("✗ Error: Window '{}' has zero height", name);
                errors += 1;
            }
            if base.cols == 0 {
                eprintln!("✗ Error: Window '{}' has zero width", name);
                errors += 1;
            }

            // Check for empty names
            if name.is_empty() {
                eprintln!("✗ Error: Window has empty name");
                errors += 1;
            }

            // Warn about very small windows
            if base.rows == 1 && base.cols < 10 {
                eprintln!(
                    "⚠ Warning: Window '{}' is very small ({}x{})",
                    name, base.cols, base.rows
                );
                warnings += 1;
            }
        }

        // Summary
        if errors == 0 && warnings == 0 {
            println!("✓ Layout is valid with no issues");
        } else {
            if errors > 0 {
                eprintln!("\n✗ Found {} error(s)", errors);
            }
            if warnings > 0 {
                println!("⚠ Found {} warning(s)", warnings);
            }
        }

        if errors > 0 {
            anyhow::bail!("Layout validation failed with {} error(s)", errors);
        }

        Ok(())
    }

    /// Get a window from the layout by name
    pub fn get_window(&self, name: &str) -> Option<&WindowDef> {
        self.windows.iter().find(|w| w.name() == name)
    }

    /// Add a window to the layout (from template or make visible if exists)
    /// Generate a unique spacer widget name based on existing spacers in layout
    /// Uses max number + 1 algorithm, checking ALL widgets including hidden ones
    /// Pattern: spacer_1, spacer_2, spacer_3, etc.
    pub fn generate_spacer_name(&self) -> String {
        let max_number = self
            .windows
            .iter()
            .filter_map(|w| {
                // Only consider spacer widgets
                match w {
                    WindowDef::Spacer { base, .. } => {
                        // Extract number from name like "spacer_5"
                        if let Some(num_str) = base.name.strip_prefix("spacer_") {
                            num_str.parse::<u32>().ok()
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .max()
            .unwrap_or(0);

        format!("spacer_{}", max_number + 1)
    }

    pub fn add_window(&mut self, name: &str) -> Result<()> {
        // Check if window already exists in layout
        if let Some(existing) = self.windows.iter_mut().find(|w| w.name() == name) {
            // Just make it visible
            existing.base_mut().visible = true;
            tracing::info!("Window '{}' already exists, setting visible=true", name);
            return Ok(());
        }

        // Get template
        let mut window_def = Config::get_window_template(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown window template: {}", name))?;

        // Special handling for spacer: auto-generate unique name
        if name == "spacer" {
            let auto_name = self.generate_spacer_name();
            window_def.base_mut().name = auto_name.clone();
            tracing::info!("Auto-generated spacer name: {}", auto_name);
        }

        // Set visible
        window_def.base_mut().visible = true;

        // Add to layout
        self.windows.push(window_def);
        tracing::info!("Added window '{}' from template", name);
        Ok(())
    }

    /// Hide a window (set visible = false)
    pub fn hide_window(&mut self, name: &str) -> Result<()> {
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.name() == name)
            .ok_or_else(|| anyhow::anyhow!("Window not found: {}", name))?;

        window.base_mut().visible = false;
        tracing::info!("Window '{}' hidden (visible=false)", name);
        Ok(())
    }

    /// Remove window from layout if it matches the default template
    /// (keeps layout file minimal by not saving unmodified windows)
    pub fn remove_window_if_default(&mut self, name: &str) {
        if let Some(template) = Config::get_window_template(name) {
            self.windows.retain(|w| {
                if w.name() == name {
                    // Compare window to template - if identical, remove (return false to filter out)
                    // If different, keep (return true)
                    w != &template
                } else {
                    true
                }
            });
        }
    }
}

impl Config {
    /// Find the appropriate layout for a given terminal size
    /// Returns the layout name if a matching mapping is found
    pub fn find_layout_for_size(&self, width: u16, height: u16) -> Option<String> {
        for mapping in &self.layout_mappings {
            if mapping.matches(width, height) {
                tracing::info!(
                    "Found layout mapping for {}x{}: '{}' (range: {}x{} to {}x{})",
                    width,
                    height,
                    mapping.layout,
                    mapping.min_width,
                    mapping.min_height,
                    mapping.max_width,
                    mapping.max_height
                );
                return Some(mapping.layout.clone());
            }
        }
        tracing::debug!(
            "No layout mapping found for terminal size {}x{}",
            width,
            height
        );
        None
    }

    /// Resolve a color name to a hex code
    /// If the input is already a hex code, return it unchanged
    /// If it's a color name, look it up in the palette
    /// Returns None if the color name is not found
    pub fn resolve_color(&self, color_input: &str) -> Option<String> {
        // If it's already a hex code, return it
        if color_input.starts_with('#') && color_input.len() == 7 {
            return Some(color_input.to_string());
        }

        // If it's "none" or empty, return None
        if color_input.is_empty() || color_input.eq_ignore_ascii_case("none") || color_input == "-"
        {
            return None;
        }

        // Look up in palette
        let color_lower = color_input.to_lowercase();
        for palette_color in &self.colors.color_palette {
            if palette_color.name.to_lowercase() == color_lower {
                return Some(palette_color.color.clone());
            }
        }

        // Not found - return the input as-is (might be a hex code without #, or invalid)
        // Let the caller handle validation
        Some(color_input.to_string())
    }

    /// Get the currently active theme
    /// Returns the theme specified by active_theme, or the default dark theme if not found
    pub fn get_theme(&self) -> crate::theme::AppTheme {
        crate::theme::ThemePresets::all_with_custom(self.character.as_deref())
            .get(&self.active_theme)
            .cloned()
            .unwrap_or_else(crate::theme::ThemePresets::dark)
    }

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
        let ui_defaults = UiConfig::default();

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

            "health" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "health".to_string(),
                    title: Some("Health".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(1),
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
                    min_rows: Some(1),
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
                    min_rows: Some(1),
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

            "entity_custom" => Some(WindowDef::Targets {
                base: WindowBase {
                    name: "entity_custom".to_string(),
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
                },
            }),

            "dashboard" => Some(WindowDef::Dashboard {
                base: WindowBase {
                    name: "dashboard".to_string(),
                    title: Some("Dashboard".to_string()),
                    row: 0,
                    col: 0,
                    rows: 5,
                    cols: 40,
                    min_rows: Some(3),
                    min_cols: Some(10),
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
                    min_rows: Some(1),
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

            "encumlevel" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "encumlevel".to_string(),
                    title: Some("Encumbrance".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(1),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                    id: Some("encumlevel".to_string()),
                    label: Some("Encumbrance".to_string()),
                    color: Some("#006400".to_string()), // Dark green
                    numbers_only: false,
                    current_only: false,
                },
            }),

            "pbarStance" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "pbarStance".to_string(),
                    title: Some("Stance".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(1),
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

            "mindState" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "mindState".to_string(),
                    title: Some("Mind".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(1),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                    id: Some("mindState".to_string()),
                    label: Some("Mind".to_string()),
                    color: Some("#008b8b".to_string()), // Cyan/teal
                    numbers_only: false,
                    current_only: false,
                },
            }),

            "lblBPs" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "lblBPs".to_string(),
                    title: Some("Blood Points".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(1),
                    max_rows: Some(3),
                    ..base_defaults.clone()
                },
                data: ProgressWidgetData {
                    id: Some("lblBPs".to_string()),
                    label: Some("Blood Points".to_string()),
                    color: Some("#8B0000".to_string()), // Dark red
                    numbers_only: false,
                    current_only: false,
                },
            }),

            "progress_custom" => Some(WindowDef::Progress {
                base: WindowBase {
                    name: "progress_custom".to_string(),
                    title: Some("Custom".to_string()),
                    row: 0,
                    col: 0,
                    rows: 3,
                    cols: 20,
                    show_border: true,
                    min_rows: Some(1),
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
                    name: "countdown_custom".to_string(),
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
                    max_rows: Some(5),
                    min_cols: Some(7),
                    max_cols: Some(9),
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
                    max_rows: Some(8),
                    min_cols: Some(8),
                    max_cols: Some(10),
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
                    text_color: Some("#00FF00".to_string()), // Green
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
                    text_color: Some("#FF0000".to_string()), // Red
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
                    text_color: Some("#FFA500".to_string()), // Orange
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
                    text_color: Some("#00BFFF".to_string()), // Deep sky blue
                    ..base_defaults.clone()
                },
                data: ActiveEffectsWidgetData {
                    category: "ActiveSpells".to_string(),
                },
            }),

            "active_effects_custom" => Some(WindowDef::ActiveEffects {
                base: WindowBase {
                    name: "active_effects_custom".to_string(),
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
                    min_rows: Some(1),
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
                    min_rows: Some(1),
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
                    min_rows: Some(1),
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
                    text_color: Some("#00CED1".to_string()), // Dark turquoise
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["thoughts".to_string()],
                    buffer_size: 1000,
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "speech" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "speech".to_string(),
                    title: Some("Speech".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    text_color: Some("#FFD700".to_string()), // Gold
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["speech".to_string()],
                    buffer_size: 1000,
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "announcements" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "announcements".to_string(),
                    title: Some("Announcements".to_string()),
                    rows: 10,
                    cols: 50,
                    show_border: true,
                    text_color: Some("#FF1493".to_string()), // Deep pink
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["announcements".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "loot" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "loot".to_string(),
                    title: Some("Loot".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    text_color: Some("#FFD700".to_string()), // Gold
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["loot".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "death" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "death".to_string(),
                    title: Some("Death".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    text_color: Some("#DC143C".to_string()), // Crimson
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["death".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "logons" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "logons".to_string(),
                    title: Some("Logons".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    text_color: Some("#87CEEB".to_string()), // Sky blue
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["logons".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "familiar" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "familiar".to_string(),
                    title: Some("Familiar".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    text_color: Some("#9370DB".to_string()), // Medium purple
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["familiar".to_string()],
                    buffer_size: 1000,
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "ambients" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "ambients".to_string(),
                    title: Some("Ambients".to_string()),
                    rows: 10,
                    cols: 40,
                    show_border: true,
                    text_color: Some("#B0C4DE".to_string()), // Light steel blue
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["ambients".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "bounty" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "bounty".to_string(),
                    title: Some("Bounties".to_string()),
                    rows: 15,
                    cols: 50,
                    show_border: true,
                    text_color: Some("#FFD700".to_string()), // Gold
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["bounty".to_string()],
                    buffer_size: 0, // VellumFE uses 0 - content is cleared and replaced
                    wordwrap: true,
                    show_timestamps: false,
                },
            }),

            "society" => Some(WindowDef::Text {
                base: WindowBase {
                    name: "society".to_string(),
                    title: Some("Society".to_string()),
                    rows: 10,
                    cols: 50,
                    show_border: true,
                    text_color: Some("#9370DB".to_string()), // Medium purple
                    ..base_defaults.clone()
                },
                data: TextWidgetData {
                    streams: vec!["society".to_string()],
                    buffer_size: 500,
                    wordwrap: true,
                    show_timestamps: false,
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
                },
            }),

            "spells" => Some(WindowDef::Spells {
                base: WindowBase {
                    name: "spells".to_string(),
                    title: Some("Spells".to_string()),
                    rows: 20,
                    cols: 40,
                    show_border: true,
                    text_color: Some("#9370DB".to_string()), // Medium purple
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
                        },
                        TabbedTextTab {
                            name: "Speech".to_string(),
                            stream: None,
                            streams: vec!["speech".to_string()],
                            show_timestamps: None,
                            ignore_activity: Some(false),
                        },
                        TabbedTextTab {
                            name: "Announcements".to_string(),
                            stream: None,
                            streams: vec!["announcements".to_string()],
                            show_timestamps: None,
                            ignore_activity: Some(false),
                        },
                        TabbedTextTab {
                            name: "Loot".to_string(),
                            stream: None,
                            streams: vec!["loot".to_string()],
                            show_timestamps: None,
                            ignore_activity: Some(false),
                        },
                        TabbedTextTab {
                            name: "Ambients".to_string(),
                            stream: None,
                            streams: vec!["ambients".to_string()],
                            show_timestamps: None,
                            ignore_activity: Some(false),
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
                        show_timestamps: Some(ui_defaults.show_timestamps),
                        ignore_activity: Some(false),
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
            "encumlevel".to_string(),
            "pbarStance".to_string(),
            "mindState".to_string(),
            "lblBPs".to_string(),
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
            "spacer".to_string(),
            "performance".to_string(),
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

    /// Get list of available window templates (excluding command_input which is always present)
    /// DEPRECATED: Use list_window_templates() instead
    pub fn available_window_templates() -> Vec<String> {
        Self::list_window_templates()
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

    /// Get addable templates by category (excluding visible windows)
    pub fn get_addable_templates_by_category(
        layout: &crate::config::Layout,
    ) -> HashMap<WidgetCategory, Vec<String>> {
        let all_by_category = Self::get_templates_by_category();

        all_by_category
            .into_iter()
            .map(|(category, templates)| {
                let available: Vec<String> = templates
                    .into_iter()
                    .filter(|name| {
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

    pub fn load() -> Result<Self> {
        Self::load_with_options(None, 8000)
    }

    /// Load config from a custom file path
    /// This loads the main config.toml from the specified path,
    /// but still loads colors, highlights, and keybinds from standard locations
    pub fn load_from_path(
        path: &std::path::Path,
        character: Option<&str>,
        port_override: u16,
    ) -> Result<Self> {
        // Ensure defaults are extracted
        Self::extract_defaults(character)?;

        // Load config from custom path
        let contents =
            fs::read_to_string(path).context(format!("Failed to read config file: {:?}", path))?;
        let mut config: Config = toml::from_str(&contents)
            .context(format!("Failed to parse config file: {:?}", path))?;

        // Override port from command line
        config.connection.port = port_override;

        // Store character name for later saves
        config.character = character.map(|s| s.to_string());

        // Load from separate files (from standard locations)
        config.colors = ColorConfig::load(character)?;
        config.highlights = Self::load_highlights(character)?;
        config.keybinds = Self::load_keybinds(character)?;
        config.global_keybinds = Self::load_global_keybinds(character)?;

        // Validate and auto-fix menu keybinds
        let validation = menu_keybind_validator::validate_menu_keybinds(&config.menu_keybinds);
        if validation.has_errors() {
            tracing::warn!(
                "Menu keybind validation found {} errors",
                validation.errors().len()
            );
            for error in validation.errors() {
                tracing::warn!("  {}", error.message());
            }

            // Auto-fix critical issues
            let fixed = menu_keybind_validator::auto_fix_menu_keybinds(
                &mut config.menu_keybinds,
                &validation.issues,
            );
            if fixed > 0 {
                tracing::info!("Auto-fixed {} menu keybind issues", fixed);
            }
        }
        if validation.has_warnings() {
            for warning in validation.warnings() {
                tracing::warn!("Menu keybind warning: {}", warning.message());
            }
        }

        Ok(config)
    }

    /// Load config with command-line options
    /// Checks in order:
    /// 1. ./config/<character>.toml (if character specified)
    /// 2. ./config/default.toml
    /// 3. ~/.vellum-fe/<character>.toml (if character specified)
    /// 4. ~/.vellum-fe/config.toml (fallback)
    /// Extract default files on first run
    /// Creates shared directories and profile-specific files
    ///
    /// Global resources:
    /// - ~/.vellum-fe/global/cmdlist1.xml
    /// - ~/.vellum-fe/global/sounds/wizard_music.mp3
    /// - ~/.vellum-fe/global/sounds/README.md
    ///
    /// Shared layouts:
    /// - ~/.vellum-fe/layouts/layout.toml
    /// - ~/.vellum-fe/layouts/none.toml
    /// - ~/.vellum-fe/layouts/sidebar.toml
    ///
    /// Profile-specific (default or character):
    /// - ~/.vellum-fe/profiles/{profile}/config.toml
    /// - ~/.vellum-fe/profiles/{profile}/history.txt (empty)
    fn extract_defaults(character: Option<&str>) -> Result<()> {
        // Create shared layouts directory and extract all embedded layouts
        let layouts_dir = Self::layouts_dir()?;
        fs::create_dir_all(&layouts_dir)?;

        // Automatically extract all files from embedded layouts directory
        for file in LAYOUTS_DIR.files() {
            let filename = file
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .context("Invalid layout filename")?;
            let layout_path = layouts_dir.join(filename);

            if !layout_path.exists() {
                let content = file
                    .contents_utf8()
                    .context(format!("Failed to read embedded layout {}", filename))?;
                fs::write(&layout_path, content)
                    .context(format!("Failed to write layouts/{}", filename))?;
                tracing::info!("Extracted layout {} to {:?}", filename, layout_path);
            }
        }

        // Create shared sounds directory and extract all embedded sounds
        let sounds_dir = Self::sounds_dir()?;
        fs::create_dir_all(&sounds_dir)?;

        // Automatically extract all files from embedded sounds directory
        for file in SOUNDS_DIR.files() {
            let filename = file
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .context("Invalid sound filename")?;
            let sound_path = sounds_dir.join(filename);

            if !sound_path.exists() {
                let content = file.contents();
                fs::write(&sound_path, content)
                    .context(format!("Failed to write sounds/{}", filename))?;
                tracing::info!("Extracted sound file {} to {:?}", filename, sound_path);
            }
        }

        // Extract cmdlist1.xml to global directory (only once)
        let global_dir = Self::global_dir()?;
        fs::create_dir_all(&global_dir)?;

        let cmdlist_path = Self::cmdlist_path()?;
        if !cmdlist_path.exists() {
            fs::write(&cmdlist_path, DEFAULT_CMDLIST).context("Failed to write cmdlist1.xml")?;
            tracing::info!("Extracted cmdlist1.xml to {:?}", cmdlist_path);
        }

        // Create profile directory
        let profile = Self::profile_dir(character)?;
        fs::create_dir_all(&profile)?;
        tracing::info!("Created profile directory: {:?}", profile);

        // Extract config.toml to profile (if it doesn't exist)
        let config_path = profile.join("config.toml");
        if !config_path.exists() {
            fs::write(&config_path, DEFAULT_CONFIG).context("Failed to write config.toml")?;
            tracing::info!("Extracted config.toml to {:?}", config_path);
        }

        // Extract colors.toml to profile (if it doesn't exist)
        let colors_path = profile.join("colors.toml");
        if !colors_path.exists() {
            fs::write(&colors_path, DEFAULT_COLORS).context("Failed to write colors.toml")?;
            tracing::info!("Extracted colors.toml to {:?}", colors_path);
        }

        // Extract highlights.toml to profile (if it doesn't exist)
        let highlights_path = profile.join("highlights.toml");
        if !highlights_path.exists() {
            fs::write(&highlights_path, DEFAULT_HIGHLIGHTS)
                .context("Failed to write highlights.toml")?;
            tracing::info!("Extracted highlights.toml to {:?}", highlights_path);
        }

        // Extract keybinds.toml to profile (if it doesn't exist)
        let keybinds_path = profile.join("keybinds.toml");
        if !keybinds_path.exists() {
            fs::write(&keybinds_path, DEFAULT_KEYBINDS).context("Failed to write keybinds.toml")?;
            tracing::info!("Extracted keybinds.toml to {:?}", keybinds_path);
        }

        // Create empty history.txt in profile (if it doesn't exist)
        let history_path = profile.join("history.txt");
        if !history_path.exists() {
            fs::write(&history_path, "").context("Failed to create history.txt")?;
            tracing::info!("Created empty history.txt at {:?}", history_path);
        }

        Ok(())
    }

    pub fn load_with_options(character: Option<&str>, port_override: u16) -> Result<Self> {
        // Extract defaults on first run (idempotent - only creates missing files)
        Self::extract_defaults(character)?;

        // Build character-specific config path
        let config_path = Self::config_path(character)?;

        // Load config from profile
        let contents = fs::read_to_string(&config_path)
            .context(format!("Failed to read config file: {:?}", config_path))?;
        let mut config: Config = toml::from_str(&contents)
            .context(format!("Failed to parse config file: {:?}", config_path))?;

        // Override port from command line
        config.connection.port = port_override;

        // Store character name for later saves
        config.character = character.map(|s| s.to_string());

        // Load from separate files
        config.colors = ColorConfig::load(character)?;
        config.highlights = Self::load_highlights(character)?;
        config.keybinds = Self::load_keybinds(character)?;
        config.global_keybinds = Self::load_global_keybinds(character)?;
        config.menu_keybinds = Self::load_menu_keybinds(character)?;

        // Validate and auto-fix menu keybinds
        let validation = menu_keybind_validator::validate_menu_keybinds(&config.menu_keybinds);
        if validation.has_errors() {
            tracing::warn!(
                "Menu keybind validation found {} errors",
                validation.errors().len()
            );
            for error in validation.errors() {
                tracing::warn!("  {}", error.message());
            }

            // Auto-fix critical issues
            let fixed = menu_keybind_validator::auto_fix_menu_keybinds(
                &mut config.menu_keybinds,
                &validation.issues,
            );
            if fixed > 0 {
                tracing::info!("Auto-fixed {} menu keybind issues", fixed);
            }
        }
        if validation.has_warnings() {
            for warning in validation.warnings() {
                tracing::warn!("Menu keybind warning: {}", warning.message());
            }
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

        // Save main config (without highlights, keybinds, colors, color_palette - those are skipped)
        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&config_path, contents).context("Failed to write config file")?;

        // Save to separate files
        self.colors.save(char_name)?;
        self.save_highlights(char_name)?;
        self.save_keybinds(char_name)?;

        Ok(())
    }

    /// Expose base directory path (~/.vellum-fe) for other systems (e.g., direct auth).
    pub fn base_dir() -> Result<PathBuf> {
        Self::config_dir()
    }

    /// Get the profile directory for a character (or "default" if none)
    /// Returns: ~/.vellum-fe/profiles/{character}/ or ~/.vellum-fe/profiles/default/
    fn profile_dir(character: Option<&str>) -> Result<PathBuf> {
        let profile_name = character.unwrap_or("default");
        Ok(Self::config_dir()?.join("profiles").join(profile_name))
    }

    /// Get the base vellum-fe directory (~/.vellum-fe/)
    /// Can be overridden with VELLUM_FE_DIR environment variable
    fn config_dir() -> Result<PathBuf> {
        // Check for custom directory from environment variable
        if let Ok(custom_dir) = std::env::var("VELLUM_FE_DIR") {
            return Ok(PathBuf::from(custom_dir));
        }

        // Default to ~/.vellum-fe
        let home = dirs::home_dir().context("Could not find home directory")?;
        Ok(home.join(".vellum-fe"))
    }

    /// Get path to config.toml for a character
    /// Returns: ~/.vellum-fe/{character}/config.toml or ~/.vellum-fe/default/config.toml
    pub fn config_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("config.toml"))
    }

    /// Get path to colors.toml for a character
    /// Returns: ~/.vellum-fe/{character}/colors.toml
    pub fn colors_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("colors.toml"))
    }

    /// Get the shared layouts directory (where .savelayout saves to)
    /// Returns: ~/.vellum-fe/layouts/
    fn layouts_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("layouts"))
    }

    /// Get the shared highlights directory (where .savehighlights saves to)
    /// Returns: ~/.vellum-fe/highlights/
    fn highlights_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("highlights"))
    }

    /// Get the shared keybinds directory (where .savekeybinds saves to)
    /// Returns: ~/.vellum-fe/keybinds/
    fn keybinds_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("keybinds"))
    }

    /// Get the global directory (for all shared resources)
    /// Returns: ~/.vellum-fe/global/
    fn global_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("global"))
    }

    /// Get the shared sounds directory
    /// Returns: ~/.vellum-fe/global/sounds/
    pub fn sounds_dir() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("sounds"))
    }

    /// Get path to common (global) highlights file
    /// Returns: ~/.vellum-fe/global/highlights.toml
    pub fn common_highlights_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("highlights.toml"))
    }

    /// Get path to debug log for a character
    /// Returns: ~/.vellum-fe/{character}/debug.log
    pub fn get_log_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("debug.log"))
    }

    /// Get path to command history for a character
    /// Returns: ~/.vellum-fe/{character}/history.txt
    pub fn history_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("history.txt"))
    }

    /// Get path to widget_state.toml for a character
    /// Returns: ~/.vellum-fe/{character}/widget_state.toml
    pub fn widget_state_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("widget_state.toml"))
    }

    /// Get path to cmdlist1.xml (single source of truth)
    /// Returns: ~/.vellum-fe/global/cmdlist1.xml
    pub fn cmdlist_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("cmdlist1.xml"))
    }

    /// Get path to highlights.toml for a character
    /// Returns: ~/.vellum-fe/{character}/highlights.toml
    pub fn highlights_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("highlights.toml"))
    }

    /// Get path to keybinds.toml for a character
    /// Returns: ~/.vellum-fe/{character}/keybinds.toml
    pub fn keybinds_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("keybinds.toml"))
    }

    /// Get path to auto-saved layout.toml for a character
    /// Returns: ~/.vellum-fe/{character}/layout.toml
    pub fn auto_layout_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("layout.toml"))
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


    /// List all saved keybind profiles
    pub fn list_saved_keybinds() -> Result<Vec<String>> {
        let keybinds_dir = Self::keybinds_dir()?;

        if !keybinds_dir.exists() {
            return Ok(vec![]);
        }

        let mut profiles = vec![];
        for entry in fs::read_dir(keybinds_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    profiles.push(name.to_string());
                }
            }
        }

        profiles.sort();
        Ok(profiles)
    }

    /// Save current keybinds to a named profile
    /// Returns path to saved keybinds
    pub fn save_keybinds_as(&self, name: &str) -> Result<PathBuf> {
        let keybinds_dir = Self::keybinds_dir()?;
        fs::create_dir_all(&keybinds_dir)?;

        let keybinds_path = keybinds_dir.join(format!("{}.toml", name));
        let contents =
            toml::to_string_pretty(&self.keybinds).context("Failed to serialize keybinds")?;
        fs::write(&keybinds_path, contents).context("Failed to write keybinds profile")?;

        Ok(keybinds_path)
    }

    /// Load keybinds from a named profile
    pub fn load_keybinds_from(name: &str) -> Result<HashMap<String, KeyBindAction>> {
        let keybinds_dir = Self::keybinds_dir()?;
        let keybinds_path = keybinds_dir.join(format!("{}.toml", name));

        if !keybinds_path.exists() {
            return Err(anyhow::anyhow!("Keybind profile '{}' not found", name));
        }

        let contents =
            fs::read_to_string(&keybinds_path).context("Failed to read keybinds profile")?;
        let keybinds: HashMap<String, KeyBindAction> =
            toml::from_str(&contents).context("Failed to parse keybinds profile")?;

        Ok(keybinds)
    }

    /// Resolve a spell ID to configured styling (bar/text colors)
    pub fn get_spell_color_style(&self, spell_id: u32) -> Option<SpellColorStyle> {
        for spell_config in &self.colors.spell_colors {
            if spell_config.spells.contains(&spell_id) {
                return Some(spell_config.style());
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
                border_style: default_border_style(),
                countdown_icon: default_countdown_icon(),
                poll_timeout_ms: default_poll_timeout_ms(),
                startup_music: default_startup_music(),
                startup_music_file: default_startup_music_file(),
                selection_enabled: default_selection_enabled(),
                selection_respect_window_boundaries: default_selection_respect_window_boundaries(),
                selection_auto_copy: default_selection_auto_copy(),
                drag_modifier_key: default_drag_modifier_key(),
                min_command_length: default_min_command_length(),
                performance_stats_enabled: default_performance_stats_enabled(),
                perf_stats_x: default_perf_stats_x(),
                perf_stats_y: default_perf_stats_y(),
                perf_stats_width: default_perf_stats_width(),
                perf_stats_height: default_perf_stats_height(),
                ignores_enabled: default_ignores_enabled(),
                color_mode: ColorMode::default(),
            },
            highlights: HashMap::new(),     // Loaded from highlights.toml
            keybinds: HashMap::new(),       // Loaded from keybinds.toml
            global_keybinds: GlobalKeybinds::default(), // Loaded from [global] section of keybinds.toml
            colors: ColorConfig::default(), // Loaded from colors.toml
            sound: SoundConfig::default(),
            tts: TtsConfig::default(),
            event_patterns: HashMap::new(), // Empty by default - user adds via config
            layout_mappings: Vec::new(),    // Empty by default - user adds via config
            character: None,                // Set at runtime via load_with_options
            menu_keybinds: MenuKeybinds::default(),
            active_theme: default_theme_name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spacer_template_exists() {
        // RED: Spacer template should exist and be retrievable
        let template = Config::get_window_template("spacer");
        assert!(template.is_some(), "Spacer template should exist");
    }

    #[test]
    fn test_spacer_template_returns_spacer_widget() {
        // RED: Template should return Spacer widget type
        let template = Config::get_window_template("spacer");
        assert!(template.is_some());

        match template.unwrap() {
            WindowDef::Spacer { .. } => {
                // Expected
            }
            _ => {
                panic!("Expected WindowDef::Spacer variant");
            }
        }
    }

    #[test]
    fn test_spacer_template_widget_type() {
        // RED: widget_type() should return "spacer"
        let template = Config::get_window_template("spacer").expect("Spacer template exists");
        assert_eq!(template.widget_type(), "spacer");
    }

    #[test]
    fn test_spacer_template_defaults() {
        // GREEN: Spacer template should have sensible defaults
        let template = Config::get_window_template("spacer").expect("Spacer template exists");

        if let WindowDef::Spacer { base, .. } = template {
            // Name should be empty (will be set by caller)
            assert_eq!(base.name, "");

            // Dimensions - minimal 2x2 spacer
            assert_eq!(base.rows, 2);
            assert_eq!(base.cols, 2);

            // Spacer should NOT show borders
            assert!(!base.show_border);

            // Spacer should NOT show title
            assert!(!base.show_title);

            // Should NOT be transparent (respects theme background color)
            assert!(!base.transparent_background);

            // Should be visible
            assert!(base.visible);
        } else {
            panic!("Expected WindowDef::Spacer variant");
        }
    }

    #[test]
    fn test_spacer_in_templates_list() {
        // RED: Spacer should be in the list of available templates
        let templates = Config::list_window_templates();
        assert!(
            templates.contains(&"spacer".to_string()),
            "Spacer should be in available templates list"
        );
    }

    #[test]
    fn test_spacer_widget_category() {
        // RED: Spacer should be categorized as "Other"
        let category = WidgetCategory::from_widget_type("spacer");
        assert_eq!(category, WidgetCategory::Other);
    }

    #[test]
    fn test_spacer_in_templates_by_category() {
        // RED: Spacer should appear in templates by category under "Other"
        let by_category = Config::get_templates_by_category();

        if let Some(other_templates) = by_category.get(&WidgetCategory::Other) {
            assert!(
                other_templates.contains(&"spacer".to_string()),
                "Spacer should be in Other category"
            );
        } else {
            panic!("Other category should exist");
        }
    }

    #[test]
    fn test_spacer_data_structure() {
        // RED: SpacerWidgetData should be valid
        let template = Config::get_window_template("spacer").expect("Spacer template exists");

        if let WindowDef::Spacer { data, .. } = template {
            // Should construct without issues
            let _data = SpacerWidgetData {};
            assert_eq!(data, SpacerWidgetData {});
        } else {
            panic!("Expected WindowDef::Spacer variant");
        }
    }

    #[test]
    fn test_spacer_toml_serialization() {
        // RED: Spacer widget should serialize to TOML
        let spacer = WindowDef::Spacer {
            base: WindowBase {
                name: "spacer_1".to_string(),
                row: 2,
                col: 5,
                rows: 3,
                cols: 8,
                show_border: false,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: SpacerWidgetData {},
        };

        let layout = Layout {
            windows: vec![spacer],
            terminal_width: Some(200),
            terminal_height: Some(50),
            base_layout: None,
            theme: None,
        };

        // Should serialize without error
        let toml_str = toml::to_string_pretty(&layout).expect("Failed to serialize layout");
        assert!(!toml_str.is_empty());
        assert!(toml_str.contains("spacer_1"));
    }

    #[test]
    fn test_spacer_toml_deserialization() {
        // RED: Spacer widget should deserialize from TOML
        let toml_str = r#"
terminal_width = 200
terminal_height = 50

[[windows]]
widget_type = "spacer"
name = "spacer_1"
row = 2
col = 5
rows = 3
cols = 8
show_border = false
show_title = false
transparent_background = false
visible = true
"#;

        let layout: Layout = toml::from_str(toml_str).expect("Failed to deserialize layout");
        assert_eq!(layout.windows.len(), 1);
        assert_eq!(layout.windows[0].widget_type(), "spacer");
        assert_eq!(layout.windows[0].name(), "spacer_1");
    }

    #[test]
    fn test_spacer_toml_round_trip() {
        // RED: Layout with spacer should survive serialize/deserialize round-trip
        let spacer = WindowDef::Spacer {
            base: WindowBase {
                name: "spacer_2".to_string(),
                row: 5,
                col: 10,
                rows: 4,
                cols: 6,
                show_border: false,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: SpacerWidgetData {},
        };

        let original_layout = Layout {
            windows: vec![spacer],
            terminal_width: Some(240),
            terminal_height: Some(60),
            base_layout: Some("default".to_string()),
            theme: Some("classic".to_string()),
        };

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&original_layout).expect("Failed to serialize");

        // Deserialize back
        let restored_layout: Layout = toml::from_str(&toml_str).expect("Failed to deserialize");

        // Verify structure
        assert_eq!(restored_layout.windows.len(), 1);
        assert_eq!(restored_layout.terminal_width, Some(240));
        assert_eq!(restored_layout.terminal_height, Some(60));
        assert_eq!(restored_layout.base_layout, Some("default".to_string()));
        assert_eq!(restored_layout.theme, Some("classic".to_string()));

        // Verify spacer properties
        assert_eq!(restored_layout.windows[0].widget_type(), "spacer");
        assert_eq!(restored_layout.windows[0].name(), "spacer_2");
        let base = restored_layout.windows[0].base();
        assert_eq!(base.row, 5);
        assert_eq!(base.col, 10);
        assert_eq!(base.rows, 4);
        assert_eq!(base.cols, 6);
        assert!(!base.show_border);
        assert!(!base.show_title);
        assert!(!base.transparent_background);
        assert!(base.visible);
    }

    #[test]
    fn test_multiple_spacers_toml_round_trip() {
        // RED: Layout with multiple spacers should preserve all of them
        let spacer1 = WindowDef::Spacer {
            base: WindowBase {
                name: "spacer_1".to_string(),
                row: 0,
                col: 0,
                rows: 2,
                cols: 5,
                show_border: false,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: SpacerWidgetData {},
        };

        let spacer2 = WindowDef::Spacer {
            base: WindowBase {
                name: "spacer_2".to_string(),
                row: 10,
                col: 20,
                rows: 3,
                cols: 8,
                show_border: false,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: SpacerWidgetData {},
        };

        let original_layout = Layout {
            windows: vec![spacer1, spacer2],
            terminal_width: Some(200),
            terminal_height: Some(50),
            base_layout: None,
            theme: None,
        };

        // Serialize and deserialize
        let toml_str = toml::to_string_pretty(&original_layout).expect("Failed to serialize");
        let restored_layout: Layout = toml::from_str(&toml_str).expect("Failed to deserialize");

        // Verify both spacers are present
        assert_eq!(restored_layout.windows.len(), 2);
        assert_eq!(restored_layout.windows[0].name(), "spacer_1");
        assert_eq!(restored_layout.windows[1].name(), "spacer_2");
        assert_eq!(restored_layout.windows[0].base().row, 0);
        assert_eq!(restored_layout.windows[1].base().row, 10);
    }

    #[test]
    fn test_hidden_spacer_toml_round_trip() {
        // RED: Hidden spacers should persist in layout files
        let visible_spacer = WindowDef::Spacer {
            base: WindowBase {
                name: "spacer_1".to_string(),
                row: 0,
                col: 0,
                rows: 2,
                cols: 5,
                show_border: false,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: SpacerWidgetData {},
        };

        let hidden_spacer = WindowDef::Spacer {
            base: WindowBase {
                name: "spacer_2".to_string(),
                row: 5,
                col: 10,
                rows: 2,
                cols: 5,
                show_border: false,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
                background_color: None,
                text_color: None,
                transparent_background: false,
                locked: false,
                min_rows: None,
                max_rows: None,
                min_cols: None,
                max_cols: None,
                visible: false,
                content_align: None,  // Hidden!
                title_position: "top-left".to_string(),
            },
            data: SpacerWidgetData {},
        };

        let original_layout = Layout {
            windows: vec![visible_spacer, hidden_spacer],
            terminal_width: Some(200),
            terminal_height: Some(50),
            base_layout: None,
            theme: None,
        };

        // Serialize and deserialize
        let toml_str = toml::to_string_pretty(&original_layout).expect("Failed to serialize");
        let restored_layout: Layout = toml::from_str(&toml_str).expect("Failed to deserialize");

        // Verify both spacers are present, including hidden one
        assert_eq!(restored_layout.windows.len(), 2);
        assert_eq!(restored_layout.windows[0].name(), "spacer_1");
        assert_eq!(restored_layout.windows[1].name(), "spacer_2");

        // Verify visibility state is preserved
        assert!(restored_layout.windows[0].base().visible);
        assert!(!restored_layout.windows[1].base().visible);
    }

    #[test]
    fn test_spacer_resize_scales_proportionally() {
        // RED: Spacers should scale proportionally during resize
        // Create layout: Widget A (0,0 10x10) - spacer (10,0 5x10) - Widget B (15,0 10x10)
        let widget_a = WindowDef::Text {
            base: WindowBase {
                name: "widget_a".to_string(),
                row: 0,
                col: 0,
                rows: 10,
                cols: 10,
                show_border: true,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: TextWidgetData {
                streams: vec!["main".to_string()],
                buffer_size: 1000,
                wordwrap: true,
                show_timestamps: false,
            },
        };

        let spacer = WindowDef::Spacer {
            base: WindowBase {
                name: "spacer_1".to_string(),
                row: 0,
                col: 10,
                rows: 10,
                cols: 5,
                show_border: false,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: SpacerWidgetData {},
        };

        let widget_b = WindowDef::Text {
            base: WindowBase {
                name: "widget_b".to_string(),
                row: 0,
                col: 15,
                rows: 10,
                cols: 10,
                show_border: true,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: TextWidgetData {
                streams: vec!["main".to_string()],
                buffer_size: 1000,
                wordwrap: true,
                show_timestamps: false,
            },
        };

        let mut layout = Layout {
            windows: vec![widget_a, spacer, widget_b],
            terminal_width: Some(50),
            terminal_height: Some(20),
            base_layout: None,
            theme: None,
        };

        // Resize to 100x40 (2x scale)
        layout.scale_to_terminal_size(100, 40);

        // Verify spacer scaled proportionally
        let spacer_base = layout.windows[1].base();
        assert_eq!(spacer_base.col, 20); // 10 * 2 = 20
        assert_eq!(spacer_base.cols, 10); // 5 * 2 = 10
        assert_eq!(spacer_base.row, 0); // 0 * 2 = 0
        assert_eq!(spacer_base.rows, 20); // 10 * 2 = 20
    }

    #[test]
    fn test_spacer_maintains_gap_after_resize() {
        // RED: Spacer should maintain gap between widgets after resize
        // Setup: Widget A at col 0 (10 wide), spacer at col 10 (5 wide), Widget B at col 15 (10 wide)
        let widget_a = WindowDef::Text {
            base: WindowBase {
                name: "a".to_string(),
                row: 0,
                col: 0,
                rows: 10,
                cols: 10,
                show_border: true,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: TextWidgetData {
                streams: vec!["main".to_string()],
                buffer_size: 1000,
                wordwrap: true,
                show_timestamps: false,
            },
        };

        let spacer = WindowDef::Spacer {
            base: WindowBase {
                name: "spacer_1".to_string(),
                row: 0,
                col: 10,
                rows: 10,
                cols: 5,
                show_border: false,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: SpacerWidgetData {},
        };

        let widget_b = WindowDef::Text {
            base: WindowBase {
                name: "b".to_string(),
                row: 0,
                col: 15,
                rows: 10,
                cols: 10,
                show_border: true,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: TextWidgetData {
                streams: vec!["main".to_string()],
                buffer_size: 1000,
                wordwrap: true,
                show_timestamps: false,
            },
        };

        let mut layout = Layout {
            windows: vec![widget_a, spacer, widget_b],
            terminal_width: Some(50),
            terminal_height: Some(20),
            base_layout: None,
            theme: None,
        };

        // Verify gap before resize: A ends at 10, spacer starts at 10, B starts at 15
        assert_eq!(layout.windows[0].base().col + layout.windows[0].base().cols, 10); // A: 0+10
        assert_eq!(layout.windows[1].base().col, 10); // Spacer starts at 10
        assert_eq!(layout.windows[2].base().col, 15); // B starts at 15

        // Resize to 100x40 (2x scale)
        layout.scale_to_terminal_size(100, 40);

        // After resize: Gap should still exist, proportionally
        // A at 0, 20 wide -> ends at 20
        // Spacer at 20, 10 wide -> covers 20-30
        // B at 30, 20 wide -> starts at 30
        let a_end = layout.windows[0].base().col + layout.windows[0].base().cols;
        let spacer_start = layout.windows[1].base().col;
        let b_start = layout.windows[2].base().col;

        // Gap maintained: A-end == spacer-start, spacer-end == B-start
        assert_eq!(a_end, spacer_start);
        assert_eq!(
            spacer_start + layout.windows[1].base().cols,
            b_start
        );
    }

    #[test]
    fn test_spacer_no_widget_collision_after_resize() {
        // RED: Spacers should prevent widget collisions after resize
        // Setup: Simple 2-widget layout separated by spacer
        let widget_a = WindowDef::Text {
            base: WindowBase {
                name: "main".to_string(),
                row: 0,
                col: 0,
                rows: 20,
                cols: 30,
                show_border: true,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: true,
                title: Some("Main".to_string()),
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
                title_position: "top-left".to_string(),
            },
            data: TextWidgetData {
                streams: vec!["main".to_string()],
                buffer_size: 5000,
                wordwrap: true,
                show_timestamps: false,
            },
        };

        let spacer = WindowDef::Spacer {
            base: WindowBase {
                name: "spacer_1".to_string(),
                row: 0,
                col: 30,
                rows: 20,
                cols: 2,
                show_border: false,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: false,
                title: None,
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
                title_position: "top-left".to_string(),
            },
            data: SpacerWidgetData {},
        };

        let widget_b = WindowDef::Text {
            base: WindowBase {
                name: "status".to_string(),
                row: 0,
                col: 32,
                rows: 20,
                cols: 20,
                show_border: true,
                border_style: "single".to_string(),
                border_sides: BorderSides::default(),
                border_color: None,
                show_title: true,
                title: Some("Status".to_string()),
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
                title_position: "top-left".to_string(),
            },
            data: TextWidgetData {
                streams: vec!["status".to_string()],
                buffer_size: 100,
                wordwrap: true,
                show_timestamps: false,
            },
        };

        let mut layout = Layout {
            windows: vec![widget_a, spacer, widget_b],
            terminal_width: Some(100),
            terminal_height: Some(25),
            base_layout: None,
            theme: None,
        };

        // Verify initial no overlap
        let a_end = layout.windows[0].base().col + layout.windows[0].base().cols;
        let spacer_start = layout.windows[1].base().col;
        assert_eq!(a_end, spacer_start, "Initial state: A should end where spacer starts");

        // Resize to 200x50 (2x scale)
        layout.scale_to_terminal_size(200, 50);

        // Verify no collision after resize
        let a_end = layout.windows[0].base().col + layout.windows[0].base().cols;
        let spacer_start = layout.windows[1].base().col;
        let spacer_end = layout.windows[1].base().col + layout.windows[1].base().cols;
        let b_start = layout.windows[2].base().col;

        // Should maintain separation
        assert!(a_end <= spacer_start, "A should not overlap spacer");
        assert!(spacer_end <= b_start, "Spacer should not overlap B");
    }
}
