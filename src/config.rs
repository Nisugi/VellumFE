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
mod highlights;
mod keybinds;
mod layout;
mod settings;
mod templates;
mod widgets;
mod window_def;

pub use colors::{
    ColorConfig, PaletteColor, PresetColor, PromptColor, SpellColorRange, SpellColorStyle,
    UiColors,
};
pub use highlights::{EventAction, EventPattern, HighlightPattern, RedirectMode};
pub use keybinds::{
    parse_key_string, AppKeybinds, KeyAction, KeyBindAction, MacroAction, MenuKeybinds,
};
pub use layout::{ContentAlign, Layout, LayoutConfig, LayoutMapping};
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

impl Config {
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
