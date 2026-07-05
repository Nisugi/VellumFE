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
mod highlights;
mod keybinds;
mod settings;
mod templates;
mod widgets;
mod window_def;

pub use highlights::{EventAction, EventPattern, HighlightPattern, RedirectMode};
pub use keybinds::{
    parse_key_string, AppKeybinds, KeyAction, KeyBindAction, MacroAction, MenuKeybinds,
};
pub use settings::{
    ConnectionConfig, FocusConfig, HighlightsConfig, LoggingConfig, SoundConfig, StreamsConfig,
    TargetListConfig, TtsConfig, UiConfig,
};
pub use templates::{IndicatorTemplateEntry, IndicatorTemplateStore};
pub use widgets::{
    apply_compiled_text_replacements, compile_text_replacements, default_minivitals_bar_order,
    ActiveEffectsWidgetData, BetrayerWidgetData, BorderSides, CommandInputWidgetData,
    CompassWidgetData, CompiledTextReplacement, ContainerWidgetData, CountdownWidgetData,
    DashboardIndicatorDef, DashboardWidgetData, EncumbranceWidgetData, ExperienceWidgetData,
    GS4ExperienceWidgetData, HandWidgetData, IndicatorWidgetData, InjuryDollWidgetData,
    InventoryWidgetData, ItemsWidgetData, MiniVitalsWidgetData, PerceptionWidgetData,
    PerformanceWidgetData, PlayersWidgetData, ProgressWidgetData, QuickbarDefinition,
    QuickbarEntryConfig, QuickbarWidgetData, QuickbarsConfig, RoomWidgetData, SortDirection,
    SpacerWidgetData, SpellsWidgetData, TabbedTextTab, TabbedTextWidgetData, TargetsWidgetData,
    TextReplacement, TextWidgetData, WindowBase,
};
pub use window_def::WindowDef;

// Embed default configuration files at compile time
// Files are under defaults/globals/ to mirror the user's ~/.vellum-fe/global/ structure
const DEFAULT_CONFIG: &str = include_str!("../defaults/globals/config.toml");
const DEFAULT_COLORS: &str = include_str!("../defaults/globals/colors.toml");
const DEFAULT_HIGHLIGHTS: &str = include_str!("../defaults/globals/highlights.toml");
const DEFAULT_KEYBINDS: &str = include_str!("../defaults/globals/keybinds.toml");
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
            "targets" | "players" | "items" => Self::Entity,
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
    #[serde(default)] // Use defaults for stream routing
    pub streams: StreamsConfig, // Stream routing configuration (drop list, fallback)
    #[serde(default, rename = "highlights")] // [highlights] section in config.toml
    pub highlight_settings: HighlightsConfig, // Highlight system toggles (sounds, replace, redirect, coloring)
    #[serde(default)]
    pub quickbars: QuickbarsConfig, // Custom quickbar definitions and defaults
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
    /// Load colors from colors.toml for a character (with merge from global)
    pub fn load(character: Option<&str>) -> Result<Self> {
        // Try to load with merge (global + character)
        Self::load_with_merge(character)
    }

    /// Load common (global) colors from global/colors.toml
    pub fn load_common_colors() -> Result<Self> {
        let colors_path = Config::common_colors_path()?;

        if colors_path.exists() {
            tracing::info!("Loading common colors from: {:?}", colors_path);
            let contents =
                fs::read_to_string(&colors_path).context("Failed to read global colors.toml")?;
            let mut colors: ColorConfig =
                toml::from_str(&contents).context("Failed to parse global colors.toml")?;

            // Merge defaults for missing presets
            let defaults = Self::default();
            for (key, preset) in defaults.presets {
                colors.presets.entry(key).or_insert(preset);
            }

            // Merge defaults for missing color_palette
            if colors.color_palette.is_empty() {
                colors.color_palette = defaults.color_palette;
            }

            Ok(colors)
        } else {
            tracing::info!(
                "Global colors.toml not found at {:?}, using defaults",
                colors_path
            );
            Ok(Self::default())
        }
    }

    /// Load ONLY character-specific colors (no merge with global)
    /// Used for source tracking in UI to distinguish [G] vs [C] colors
    pub fn load_character_colors_only(character: Option<&str>) -> Result<Self> {
        let colors_path = Config::colors_path(character)?;

        if colors_path.exists() {
            tracing::debug!("Loading character colors from: {:?}", colors_path);
            let contents =
                fs::read_to_string(&colors_path).context("Failed to read character colors.toml")?;
            let colors: ColorConfig =
                toml::from_str(&contents).context("Failed to parse character colors.toml")?;
            Ok(colors)
        } else {
            // Return empty config if no character-specific file
            Ok(Self {
                presets: HashMap::new(),
                prompt_colors: Vec::new(),
                ui: UiColors::default(),
                spell_colors: Vec::new(),
                color_palette: Vec::new(),
            })
        }
    }

    /// Load with merge: global first, character overrides
    pub fn load_with_merge(character: Option<&str>) -> Result<Self> {
        // Start with global colors
        let mut colors = Self::load_common_colors()?;

        // Load character-specific colors
        let char_colors = Self::load_character_colors_only(character)?;

        // Merge character presets (override global)
        for (key, preset) in char_colors.presets {
            colors.presets.insert(key, preset);
        }

        // Merge character prompt_colors (replace entire list if not empty)
        if !char_colors.prompt_colors.is_empty() {
            colors.prompt_colors = char_colors.prompt_colors;
        }

        // Merge character UI colors (only override non-default values)
        // For simplicity, we'll check if they differ from defaults
        let default_ui = UiColors::default();
        if char_colors.ui.command_echo_color != default_ui.command_echo_color {
            colors.ui.command_echo_color = char_colors.ui.command_echo_color;
        }
        if char_colors.ui.border_color != default_ui.border_color {
            colors.ui.border_color = char_colors.ui.border_color;
        }
        if char_colors.ui.focused_border_color != default_ui.focused_border_color {
            colors.ui.focused_border_color = char_colors.ui.focused_border_color;
        }
        if char_colors.ui.text_color != default_ui.text_color {
            colors.ui.text_color = char_colors.ui.text_color;
        }
        if char_colors.ui.background_color != default_ui.background_color {
            colors.ui.background_color = char_colors.ui.background_color;
        }
        if char_colors.ui.selection_bg_color != default_ui.selection_bg_color {
            colors.ui.selection_bg_color = char_colors.ui.selection_bg_color;
        }
        if char_colors.ui.textarea_background != default_ui.textarea_background {
            colors.ui.textarea_background = char_colors.ui.textarea_background;
        }

        // Merge character spell_colors (replace entire list if not empty)
        if !char_colors.spell_colors.is_empty() {
            colors.spell_colors = char_colors.spell_colors;
        }

        // Merge character color_palette (replace entire list if not empty)
        if !char_colors.color_palette.is_empty() {
            colors.color_palette = char_colors.color_palette;
        }

        tracing::debug!(
            "Loaded merged colors: {} presets, {} palette colors",
            colors.presets.len(),
            colors.color_palette.len()
        );

        Ok(colors)
    }

    /// Save colors to colors.toml for a character
    pub fn save(&self, character: Option<&str>) -> Result<()> {
        let colors_path = Config::colors_path(character)?;
        let contents = toml::to_string_pretty(self).context("Failed to serialize colors")?;
        fs::write(&colors_path, contents).context("Failed to write colors.toml")?;
        Ok(())
    }

    /// Save colors to global colors.toml
    pub fn save_common(&self) -> Result<()> {
        let colors_path = Config::common_colors_path()?;

        // Ensure global directory exists
        if let Some(parent) = colors_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create global directory: {:?}", parent))?;
        }

        let contents = toml::to_string_pretty(self).context("Failed to serialize colors")?;
        fs::write(&colors_path, contents).context("Failed to write global colors.toml")?;
        tracing::info!("Saved colors to global file: {:?}", colors_path);
        Ok(())
    }

    /// Save a single palette color to the appropriate file based on scope
    pub fn save_single_palette_color(
        color: &PaletteColor,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        if is_global {
            Self::save_common_palette_color(color)
        } else {
            Self::save_character_palette_color(color, character)
        }
    }

    /// Save a single palette color to global colors.toml
    fn save_common_palette_color(color: &PaletteColor) -> Result<()> {
        let mut colors = Self::load_common_colors()?;

        // Find and update or add the color
        if let Some(existing) = colors.color_palette.iter_mut().find(|c| c.name == color.name) {
            *existing = color.clone();
        } else {
            colors.color_palette.push(color.clone());
        }

        colors.save_common()?;
        tracing::info!("Saved palette color '{}' to global colors", color.name);
        Ok(())
    }

    /// Save a single palette color to character colors.toml
    fn save_character_palette_color(color: &PaletteColor, character: Option<&str>) -> Result<()> {
        let mut colors = Self::load_character_colors_only(character)?;

        // Find and update or add the color
        if let Some(existing) = colors.color_palette.iter_mut().find(|c| c.name == color.name) {
            *existing = color.clone();
        } else {
            colors.color_palette.push(color.clone());
        }

        colors.save(character)?;
        tracing::info!("Saved palette color '{}' to character colors", color.name);
        Ok(())
    }

    /// Delete a single palette color from the appropriate file based on scope
    pub fn delete_single_palette_color(
        name: &str,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        if is_global {
            Self::delete_common_palette_color(name)
        } else {
            Self::delete_character_palette_color(name, character)
        }
    }

    /// Delete a single palette color from global colors.toml
    fn delete_common_palette_color(name: &str) -> Result<()> {
        let mut colors = Self::load_common_colors()?;
        let original_len = colors.color_palette.len();
        colors.color_palette.retain(|c| c.name != name);

        if colors.color_palette.len() < original_len {
            colors.save_common()?;
            tracing::info!("Deleted palette color '{}' from global colors", name);
        }
        Ok(())
    }

    /// Delete a single palette color from character colors.toml
    fn delete_character_palette_color(name: &str, character: Option<&str>) -> Result<()> {
        let mut colors = Self::load_character_colors_only(character)?;
        let original_len = colors.color_palette.len();
        colors.color_palette.retain(|c| c.name != name);

        if colors.color_palette.len() < original_len {
            colors.save(character)?;
            tracing::info!("Deleted palette color '{}' from character colors", name);
        }
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

/// Saved dialog position for persistence across sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogPosition {
    pub x: u16,
    pub y: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u16>,
}

/// TOML file wrapper for saved dialog positions (widget_state.toml)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedDialogPositions {
    #[serde(default)]
    pub dialogs: HashMap<String, DialogPosition>,
    /// Saved positions for ephemeral container windows (keyed by container title)
    #[serde(default)]
    pub containers: HashMap<String, DialogPosition>,
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
        "targets".to_string(),
        "players".to_string(),
        "items".to_string(),
        "inventory".to_string(),
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

fn default_open_dialog_blocklist() -> Vec<String> {
    vec![
        "combat".to_string(),
        "injuries".to_string(),
        "stance".to_string(),
        "befriend".to_string(),
        "espMasterDialog".to_string(),
        "espMasterData".to_string(),
        "Buffs".to_string(),
        "Debuffs".to_string(),
        "Cooldowns".to_string(),
        "mapMaster".to_string(),
        "encum".to_string(),
        "minivitals".to_string(),
        "expr".to_string(),
        "Active Spells".to_string(),
    ]
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

fn default_items_entity_id() -> String {
    "items".to_string()
}

fn default_true() -> bool {
    true
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

        for window in &mut layout.windows {
            if window.widget_type() == "targets" {
                let base = window.base_mut();
                if base.name == "dd_targets" {
                    tracing::info!("Renaming legacy targets window 'dd_targets' -> 'targets'");
                    base.name = "targets".to_string();
                }
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

    /// Generate a unique widget name for any widget type
    /// Uses max number + 1 algorithm, checking ALL widgets with matching prefix
    /// Pattern: custom-{widgettype}-1, custom-{widgettype}-2, etc.
    /// Example: custom-tabbedtext-1, custom-text-2, custom-progress-1
    pub fn generate_widget_name(&self, widget_type: &str) -> String {
        // Normalize widget type: lowercase and strip _custom suffix
        // This ensures "tabbedtext_custom" → "custom-tabbedtext-1" (not "custom-tabbedtext_custom-1")
        let lowercase = widget_type.to_lowercase();
        let normalized_type = lowercase
            .strip_suffix("_custom")
            .unwrap_or(&lowercase);
        let prefix = format!("custom-{}-", normalized_type);

        let max_number = self
            .windows
            .iter()
            .filter_map(|w| {
                let name = w.name();
                // Extract number from name like "custom-text-5"
                if let Some(num_str) = name.strip_prefix(&prefix) {
                    num_str.parse::<u32>().ok()
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);

        format!("custom-{}-{}", normalized_type, max_number + 1)
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

        // Auto-generate unique name for templates with empty names
        // This includes spacers and custom widgets (tabbedtext_custom, text_custom, etc.)
        if window_def.base().name.is_empty() {
            let auto_name = if name == "spacer" {
                self.generate_spacer_name()
            } else {
                self.generate_widget_name(name)
            };
            window_def.base_mut().name = auto_name.clone();
            tracing::info!("Auto-generated window name: {} for template '{}'", auto_name, name);
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


    pub fn load() -> Result<Self> {
        Self::load_with_options(None, None)
    }

    /// Load config from a custom file path
    /// This loads the main config.toml from the specified path,
    /// but still loads colors, highlights, and keybinds from standard locations
    pub fn load_from_path(
        path: &std::path::Path,
        character: Option<&str>,
        port_override: Option<u16>,
    ) -> Result<Self> {
        // Ensure defaults are extracted
        Self::extract_defaults(character)?;

        // Load config from custom path
        let contents =
            fs::read_to_string(path).context(format!("Failed to read config file: {:?}", path))?;
        let mut config: Config = toml::from_str(&contents)
            .context(format!("Failed to parse config file: {:?}", path))?;

        // Override port from command line (if specified)
        if let Some(port) = port_override {
            config.connection.port = port;
        }

        // Store character name for later saves
        config.character = character.map(|s| s.to_string());

        // Load from separate files (from standard locations)
        config.colors = ColorConfig::load(character)?;
        config.highlights = Self::load_highlights(character)?;
        config.keybinds = Self::load_keybinds(character)?;
        config.app_keybinds = Self::load_app_keybinds(character)?;

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
    /// Global resources (shared by all characters):
    /// - ~/.vellum-fe/global/cmdlist1.xml
    /// - ~/.vellum-fe/global/keybinds.toml (default keybinds, char overrides in profile)
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
    /// Note: keybinds.toml in profile is optional (for character-specific overrides)
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

        let spell_abbrev_path = Self::spell_abbrev_path()?;
        if !spell_abbrev_path.exists() {
            fs::write(&spell_abbrev_path, DEFAULT_SPELL_ABBREVS)
                .context("Failed to write spell_abbrev.toml")?;
            tracing::info!(
                "Extracted spell_abbrev.toml to {:?}",
                spell_abbrev_path
            );
        }

        // Extract documented templates to global/templates directory
        // These preserve all comments and examples for user reference
        let templates_dir = global_dir.join("templates");
        fs::create_dir_all(&templates_dir)?;

        let template_config_path = templates_dir.join("config_template.toml");
        if !template_config_path.exists() {
            fs::write(&template_config_path, DEFAULT_CONFIG_TEMPLATE)
                .context("Failed to write config_template.toml")?;
            tracing::info!(
                "Extracted documented config_template.toml to {:?}",
                template_config_path
            );
        }

        let template_layout_path = templates_dir.join("layout_template.toml");
        if !template_layout_path.exists() {
            fs::write(&template_layout_path, DEFAULT_LAYOUT_TEMPLATE)
                .context("Failed to write layout_template.toml")?;
            tracing::info!(
                "Extracted documented layout_template.toml to {:?}",
                template_layout_path
            );
        }

        // Create profile directory
        let profile = Self::profile_dir(character)?;
        fs::create_dir_all(&profile)?;
        tracing::info!("Created profile directory: {:?}", profile);

        // Extract config.toml to global directory (shared defaults for all characters)
        // Character-specific overrides can still be added to profile/config.toml
        let config_path = Self::common_config_path()?;
        if !config_path.exists() {
            fs::write(&config_path, DEFAULT_CONFIG).context("Failed to write config.toml")?;
            tracing::info!("Extracted config.toml to {:?}", config_path);
        }

        // Extract colors.toml to global directory (shared across all characters)
        // Character-specific overrides can still be added to profile/colors.toml
        let colors_path = Self::common_colors_path()?;
        if !colors_path.exists() {
            fs::write(&colors_path, DEFAULT_COLORS).context("Failed to write colors.toml")?;
            tracing::info!("Extracted colors.toml to {:?}", colors_path);
        }

        // Extract highlights.toml to global directory (shared across all characters)
        // Character-specific overrides can still be added to profile/highlights.toml
        let highlights_path = Self::common_highlights_path()?;
        if !highlights_path.exists() {
            fs::write(&highlights_path, DEFAULT_HIGHLIGHTS)
                .context("Failed to write highlights.toml")?;
            tracing::info!("Extracted highlights.toml to {:?}", highlights_path);
        }

        // Extract keybinds.toml to global directory (shared across all characters)
        // Character-specific overrides can still be added to profile/keybinds.toml
        let keybinds_path = Self::common_keybinds_path()?;
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

    /// Load common (global) config defaults
    /// Returns: Config from ~/.vellum-fe/global/config.toml, or defaults if not found
    pub fn load_common_config() -> Result<Self> {
        let global_path = Self::common_config_path()?;
        if global_path.exists() {
            let contents = fs::read_to_string(&global_path)
                .context(format!("Failed to read global config: {:?}", global_path))?;
            toml::from_str(&contents)
                .context(format!("Failed to parse global config: {:?}", global_path))
        } else {
            // Return default config if no global file exists
            Ok(Self::default())
        }
    }

    /// Load ONLY character-specific config (no merge with global)
    /// Returns: Config from ~/.vellum-fe/profiles/{char}/config.toml, or None if not found
    pub fn load_character_config_only(character: Option<&str>) -> Result<Option<Self>> {
        let config_path = Self::config_path(character)?;
        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .context(format!("Failed to read character config: {:?}", config_path))?;
            let config: Config = toml::from_str(&contents)
                .context(format!("Failed to parse character config: {:?}", config_path))?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Merge character config overrides onto self
    /// NOTE: connection section ALWAYS comes from character config (never merged)
    pub fn merge_with(&mut self, character_config: Config) {
        // Connection ALWAYS comes from character (credentials, host, game)
        self.connection = character_config.connection;

        // Other sections: character overrides global if non-default
        // For now, character config completely overrides these sections if present
        // UI settings
        self.ui = character_config.ui;

        // Sound settings
        self.sound = character_config.sound;

        // TTS settings
        self.tts = character_config.tts;

        // Target list settings
        self.target_list = character_config.target_list;

        // Logging settings
        self.logging = character_config.logging;

        // Event patterns: merge (character extends global)
        for (key, pattern) in character_config.event_patterns {
            self.event_patterns.insert(key, pattern);
        }

        // Layout mappings: character replaces global if provided
        if !character_config.layout_mappings.is_empty() {
            self.layout_mappings = character_config.layout_mappings;
        }

        // Menu keybinds: character overrides global
        self.menu_keybinds = character_config.menu_keybinds;

        // Active theme: character overrides global
        self.active_theme = character_config.active_theme;

        // Streams config: character overrides global
        self.streams = character_config.streams;

        // Highlight settings: character overrides global
        self.highlight_settings = character_config.highlight_settings;

        // Quickbars: character overrides global
        self.quickbars = character_config.quickbars;
    }

    pub fn load_with_options(character: Option<&str>, port_override: Option<u16>) -> Result<Self> {
        // Extract defaults on first run (idempotent - only creates missing files)
        Self::extract_defaults(character)?;

        // Load global config first (defaults for all characters)
        let mut config = Self::load_common_config()?;

        // Load character-specific config and merge (character overrides global)
        if let Some(char_config) = Self::load_character_config_only(character)? {
            config.merge_with(char_config);
        }
        // If no character config exists, we use global config with default connection

        // Override port from command line (if specified)
        if let Some(port) = port_override {
            config.connection.port = port;
        }

        // Store character name for later saves
        config.character = character.map(|s| s.to_string());

        // Load from separate files (these already have global/character merge logic)
        config.colors = ColorConfig::load(character)?;
        config.highlights = Self::load_highlights(character)?;
        config.keybinds = Self::load_keybinds(character)?;
        config.app_keybinds = Self::load_app_keybinds(character)?;
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

    /// Save config to global config.toml
    pub fn save_common(&self) -> Result<()> {
        let config_path = Self::common_config_path()?;

        // Ensure global directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create global directory: {:?}", parent))?;
        }

        // Save main config
        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&config_path, contents).context("Failed to write global config file")?;
        tracing::info!("Saved config to global file: {:?}", config_path);
        Ok(())
    }

    /// Save a single setting to the appropriate file based on scope
    /// NOTE: Connection settings MUST always go to character config (never global)
    pub fn save_single_setting(
        &self,
        key: &str,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        // Connection settings are ALWAYS character-specific
        let actual_is_global = if key.starts_with("connection.") {
            false
        } else {
            is_global
        };

        if actual_is_global {
            self.save_setting_to_global(key)
        } else {
            self.save_setting_to_character(key, character)
        }
    }

    /// Save a specific setting to global config
    fn save_setting_to_global(&self, key: &str) -> Result<()> {
        // Load current global config
        let mut global_config = Self::load_common_config()?;

        // Update the specific setting
        Self::copy_setting(&mut global_config, self, key);

        // Save global config
        global_config.save_common()
    }

    /// Save a specific setting to character config
    fn save_setting_to_character(&self, key: &str, character: Option<&str>) -> Result<()> {
        // Load current character config (or create new if doesn't exist)
        let mut char_config = Self::load_character_config_only(character)?
            .unwrap_or_else(Self::default);

        // Update the specific setting
        Self::copy_setting(&mut char_config, self, key);

        // Save to character config path
        let config_path = Self::config_path(character)?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(&char_config).context("Failed to serialize config")?;
        fs::write(&config_path, contents).context("Failed to write character config file")?;
        tracing::info!("Saved setting '{}' to character config: {:?}", key, config_path);
        Ok(())
    }

    /// Copy a specific setting from source to destination config
    fn copy_setting(dest: &mut Config, src: &Config, key: &str) {
        match key {
            // Connection settings
            "connection.host" => dest.connection.host = src.connection.host.clone(),
            "connection.port" => dest.connection.port = src.connection.port,
            "connection.character" => dest.connection.character = src.connection.character.clone(),
            "connection.account" => dest.connection.account = src.connection.account.clone(),
            "connection.password" => dest.connection.password = src.connection.password.clone(),
            "connection.game" => dest.connection.game = src.connection.game.clone(),

            // UI settings
            "ui.buffer_size" => dest.ui.buffer_size = src.ui.buffer_size,
            "ui.border_style" => dest.ui.border_style = src.ui.border_style.clone(),
            "ui.countdown_icon" => dest.ui.countdown_icon = src.ui.countdown_icon.clone(),
            "ui.selection_enabled" => dest.ui.selection_enabled = src.ui.selection_enabled,
            "ui.selection_respect_window_boundaries" => {
                dest.ui.selection_respect_window_boundaries = src.ui.selection_respect_window_boundaries
            }
            "ui.selection_auto_copy" => dest.ui.selection_auto_copy = src.ui.selection_auto_copy,
            "ui.drag_modifier_key" => dest.ui.drag_modifier_key = src.ui.drag_modifier_key.clone(),
            "ui.min_command_length" => dest.ui.min_command_length = src.ui.min_command_length,

            // Sound settings
            "sound.enabled" => dest.sound.enabled = src.sound.enabled,
            "sound.volume" => dest.sound.volume = src.sound.volume,
            "sound.cooldown_ms" => dest.sound.cooldown_ms = src.sound.cooldown_ms,

            // Theme settings
            "active_theme" => dest.active_theme = src.active_theme.clone(),

            _ => {
                tracing::warn!("Unknown setting key for copy: {}", key);
            }
        }
    }

    /// Expose base directory path (~/.vellum-fe) for other systems (e.g., direct auth).
    pub fn base_dir() -> Result<PathBuf> {
        Self::config_dir()
    }

    /// Get the profile directory for a character (or "default" if none)
    /// Returns: ~/.vellum-fe/profiles/{character}/ or ~/.vellum-fe/profiles/default/
    pub(crate) fn profile_dir(character: Option<&str>) -> Result<PathBuf> {
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

    /// Get path to common (global) keybinds file
    /// Returns: ~/.vellum-fe/global/keybinds.toml
    pub fn common_keybinds_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("keybinds.toml"))
    }

    /// Get path to common (global) colors file
    /// Returns: ~/.vellum-fe/global/colors.toml
    pub fn common_colors_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("colors.toml"))
    }

    /// Get path to common (global) config file
    /// Returns: ~/.vellum-fe/global/config.toml
    pub fn common_config_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("config.toml"))
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

    /// Load saved dialog positions from widget_state.toml for a character
    pub fn load_dialog_positions(character: Option<&str>) -> Result<SavedDialogPositions> {
        let path = Self::widget_state_path(character)?;
        if !path.exists() {
            return Ok(SavedDialogPositions::default());
        }

        let contents = fs::read_to_string(&path)
            .context(format!("Failed to read widget state at {:?}", path))?;
        let positions: SavedDialogPositions = toml::from_str(&contents)
            .context(format!("Failed to parse widget state at {:?}", path))?;

        Ok(positions)
    }

    /// Save dialog positions to widget_state.toml for a character
    pub fn save_dialog_positions(
        character: Option<&str>,
        positions: &SavedDialogPositions,
    ) -> Result<()> {
        let path = Self::widget_state_path(character)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(positions)
            .context("Failed to serialize dialog positions")?;
        fs::write(&path, contents)
            .context(format!("Failed to write widget state to {:?}", path))?;
        Ok(())
    }

    /// Get path to cmdlist1.xml (single source of truth)
    /// Returns: ~/.vellum-fe/global/cmdlist1.xml
    pub fn cmdlist_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("cmdlist1.xml"))
    }

    /// Get path to spell abbreviations (perception window)
    /// Returns: ~/.vellum-fe/global/spell_abbrev.toml
    pub fn spell_abbrev_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("spell_abbrev.toml"))
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
                account: None,
                password: None,
                game: None,
            },
            ui: UiConfig {
                buffer_size: default_buffer_size(),
                layout: LayoutConfig::default(),
                border_style: default_border_style(),
                countdown_icon: default_countdown_icon(),
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
                perf_show_fps: true,
                perf_show_frame_times: false,
                perf_show_render_times: true,
                perf_show_ui_times: true,
                perf_show_wrap_times: true,
                perf_show_net: true,
                perf_show_parse: true,
                perf_show_events: true,
                perf_show_memory: true,
                perf_show_lines: true,
                perf_show_uptime: true,
                perf_show_jitter: false,
                perf_show_frame_spikes: false,
                perf_show_event_lag: false,
                perf_show_memory_delta: true,
                color_mode: ColorMode::default(),
                timestamp_position: TimestampPosition::default(),
                command_echo: default_command_echo(),
                betrayer_active_color: default_betrayer_active_color(),
                open_dialog_blocklist: default_open_dialog_blocklist(),
                focus: FocusConfig::default(),
                terminal_title: String::new(),
            },
            highlights: HashMap::new(),     // Loaded from highlights.toml
            keybinds: HashMap::new(),       // Loaded from keybinds.toml
            app_keybinds: AppKeybinds::default(), // Loaded from [app] section of keybinds.toml
            colors: ColorConfig::default(), // Loaded from colors.toml
            sound: SoundConfig::default(),
            tts: TtsConfig::default(),
            target_list: TargetListConfig::default(),
            logging: LoggingConfig::default(),
            streams: StreamsConfig::default(), // Stream routing config
            highlight_settings: HighlightsConfig::default(), // Highlight system toggles
            quickbars: QuickbarsConfig::default(),
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
                timestamp_position: None,
                    compact: false,
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
                timestamp_position: None,
                    compact: false,
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
                timestamp_position: None,
                    compact: false,
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
                timestamp_position: None,
                    compact: false,
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
                timestamp_position: None,
                    compact: false,
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
                timestamp_position: None,
                    compact: false,
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
