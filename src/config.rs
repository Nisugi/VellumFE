use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use crossterm::event::{KeyCode, KeyModifiers};
use include_dir::{include_dir, Dir};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub connection: ConnectionConfig,
    pub ui: UiConfig,
    #[serde(skip)]  // Loaded from separate highlights.toml file
    pub highlights: HashMap<String, HighlightPattern>,
    #[serde(skip)]  // Loaded from separate keybinds.toml file
    pub keybinds: HashMap<String, KeyBindAction>,
    #[serde(default)]
    pub sound: SoundConfig,
    #[serde(default)]
    pub event_patterns: HashMap<String, EventPattern>,
    #[serde(default)]
    pub layout_mappings: Vec<LayoutMapping>,
    #[serde(skip)]  // Don't serialize/deserialize this - it's set at runtime
    pub character: Option<String>,  // Character name for character-specific saving
    #[serde(skip)]  // Loaded from separate colors.toml file (includes color_palette)
    pub colors: ColorConfig,  // All color configuration (presets, prompt_colors, ui colors, spell colors, color_palette)
}

/// Terminal size range to layout mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutMapping {
    pub min_width: u16,
    pub min_height: u16,
    pub max_width: u16,
    pub max_height: u16,
    pub layout: String,  // Layout name (e.g., "compact1", "half_screen")
}

impl LayoutMapping {
    /// Check if terminal size matches this mapping
    pub fn matches(&self, width: u16, height: u16) -> bool {
        width >= self.min_width && width <= self.max_width &&
        height >= self.min_height && height <= self.max_height
    }
}

/// Named color in the user's palette
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaletteColor {
    pub name: String,
    pub color: String,  // Hex color code
    pub category: String,  // Color family: "red", "blue", "green", etc.
    #[serde(default)]
    pub favorite: bool,
}

impl PaletteColor {
    pub fn new(name: &str, color: &str, category: &str) -> Self {
        Self {
            name: name.to_string(),
            color: color.to_string(),
            category: category.to_string(),
            favorite: false,
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
    pub spells: Vec<u32>,           // List of spell IDs (e.g., [101, 107, 120, 140, 150])
    #[serde(default)]
    pub color: String,              // Legacy field: bar color (for backward compatibility)
    #[serde(default)]
    pub bar_color: Option<String>,  // Progress bar fill color (e.g., "#00ffff")
    #[serde(default)]
    pub text_color: Option<String>, // Text color on filled portion (default: white)
    #[serde(default)]
    pub bg_color: Option<String>,   // Background/unfilled portion color (default: black)
}

impl SpellColorRange {
    /// Get the effective bar color (prefer bar_color, fall back to color for backward compatibility)
    pub fn get_bar_color(&self) -> &str {
        self.bar_color.as_deref().unwrap_or(&self.color)
    }

    /// Get the effective text color (default to white if not set)
    pub fn get_text_color(&self) -> &str {
        self.text_color.as_deref().unwrap_or("#ffffff")
    }

    /// Get the effective background color (default to black if not set)
    pub fn get_bg_color(&self) -> &str {
        self.bg_color.as_deref().unwrap_or("#000000")
    }
}

/// UI color configuration - global defaults for all widgets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiColors {
    #[serde(default = "default_command_echo_color")]
    pub command_echo_color: String,
    #[serde(default = "default_border_color_default")]
    pub border_color: String,  // Default border color for all widgets
    #[serde(default = "default_focused_border_color")]
    pub focused_border_color: String,  // Border color for focused/active windows
    #[serde(default = "default_text_color_default")]
    pub text_color: String,    // Default text color for all widgets
    #[serde(default = "default_background_color")]
    pub background_color: String,  // Default background color for all widgets
    #[serde(default = "default_selection_bg_color")]
    pub selection_bg_color: String,  // Text selection background color
    #[serde(default = "default_textarea_background")]
    pub textarea_background: String,  // Background color for input textareas in forms/browsers
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
            let contents = fs::read_to_string(&colors_path)
                .context("Failed to read colors.toml")?;
            let mut colors: ColorConfig = toml::from_str(&contents)
                .context("Failed to parse colors.toml")?;

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
        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize colors")?;
        fs::write(&colors_path, contents)
            .context("Failed to write colors.toml")?;
        Ok(())
    }
}

/// Helper functions for loading/saving highlights and keybinds
impl Config {
    /// Load highlights from highlights.toml for a character
    /// Load common highlights shared across all characters
    /// Returns: HashMap of common highlights (empty if file doesn't exist)
    pub fn load_common_highlights() -> Result<HashMap<String, HighlightPattern>> {
        let common_path = Self::common_highlights_path()?;

        if common_path.exists() {
            let contents = fs::read_to_string(&common_path)
                .context("Failed to read common highlights.toml")?;
            let highlights: HashMap<String, HighlightPattern> = toml::from_str(&contents)
                .context("Failed to parse common highlights.toml")?;
            Ok(highlights)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Load highlights for a character, merging common highlights with character-specific ones
    /// Character-specific highlights override common highlights with the same name
    pub fn load_highlights(character: Option<&str>) -> Result<HashMap<String, HighlightPattern>> {
        // Start with common highlights
        let mut highlights = Self::load_common_highlights().unwrap_or_default();

        // Load character-specific highlights
        let highlights_path = Self::highlights_path(character)?;
        if highlights_path.exists() {
            let contents = fs::read_to_string(&highlights_path)
                .context("Failed to read highlights.toml")?;
            let char_highlights: HashMap<String, HighlightPattern> = toml::from_str(&contents)
                .context("Failed to parse highlights.toml")?;

            // Merge: character-specific overrides common
            highlights.extend(char_highlights);
        } else {
            // If no character-specific file, merge with defaults
            let defaults: HashMap<String, HighlightPattern> = toml::from_str(DEFAULT_HIGHLIGHTS)
                .unwrap_or_default();
            highlights.extend(defaults);
        }

        Ok(highlights)
    }

    /// Save highlights to highlights.toml for a character
    fn save_highlights(&self, character: Option<&str>) -> Result<()> {
        let highlights_path = Self::highlights_path(character)?;
        let contents = toml::to_string_pretty(&self.highlights)
            .context("Failed to serialize highlights")?;
        fs::write(&highlights_path, contents)
            .context("Failed to write highlights.toml")?;
        Ok(())
    }

    /// Save a single highlight to common highlights file
    pub fn save_common_highlight(name: &str, pattern: &HighlightPattern) -> Result<()> {
        let common_path = Self::common_highlights_path()?;

        // Load existing common highlights
        let mut common_highlights = Self::load_common_highlights().unwrap_or_default();

        // Add/update the highlight
        common_highlights.insert(name.to_string(), pattern.clone());

        // Save back to file
        let contents = toml::to_string_pretty(&common_highlights)
            .context("Failed to serialize common highlights")?;
        fs::write(&common_path, contents)
            .context("Failed to write common highlights.toml")?;

        Ok(())
    }

    /// Load keybinds from keybinds.toml for a character
    pub fn load_keybinds(character: Option<&str>) -> Result<HashMap<String, KeyBindAction>> {
        let keybinds_path = Self::keybinds_path(character)?;

        if keybinds_path.exists() {
            let contents = fs::read_to_string(&keybinds_path)
                .context("Failed to read keybinds.toml")?;
            let keybinds: HashMap<String, KeyBindAction> = toml::from_str(&contents)
                .context("Failed to parse keybinds.toml")?;
            Ok(keybinds)
        } else {
            // Return defaults from embedded file
            Ok(toml::from_str(DEFAULT_KEYBINDS).unwrap_or_else(|_| default_keybinds()))
        }
    }

    /// Save keybinds to keybinds.toml for a character
    fn save_keybinds(&self, character: Option<&str>) -> Result<()> {
        let keybinds_path = Self::keybinds_path(character)?;
        let contents = toml::to_string_pretty(&self.keybinds)
            .context("Failed to serialize keybinds")?;
        fs::write(&keybinds_path, contents)
            .context("Failed to write keybinds.toml")?;
        Ok(())
    }
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
    #[serde(default, alias = "bar_color")]
    pub bar_fill: Option<String>,  // Filled portion color for progress bar (if widget_type is "progress")
    #[serde(default, alias = "bar_background_color")]
    pub bar_background: Option<String>,  // Unfilled portion color for progress bar
    #[serde(default = "default_transparent_background")]
    pub transparent_background: bool,  // If true, unfilled portions are transparent
    #[serde(default)]
    pub locked: bool,  // If true, window cannot be moved or resized with mouse
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
    pub effect_category: Option<String>,  // For active_effects: "ActiveSpells", "Buffs", "Debuffs", "Cooldowns"
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
    // Hand widget configuration
    #[serde(default)]
    pub hand_icon: Option<String>,  // Icon for hand widgets (e.g., "L:", "R:", "S:")
    #[serde(default)]
    pub text_color: Option<String>,  // Text color for hand widgets and progress bars
    // Countdown widget configuration
    #[serde(default)]
    pub countdown_icon: Option<String>,  // Icon for countdown widgets (overrides global default)
    // Compass widget configuration
    #[serde(default)]
    pub compass_active_color: Option<String>,    // Color for available exits (default: #00ff00)
    #[serde(default)]
    pub compass_inactive_color: Option<String>,  // Color for unavailable exits (default: #333333)
    // Timestamp configuration
    #[serde(default)]
    pub show_timestamps: Option<bool>,  // Show timestamps at end of lines (e.g., [7:08 AM])
    // Layout resizing constraints
    #[serde(default)]
    pub min_rows: Option<u16>,  // Minimum height (enforced during resize)
    #[serde(default)]
    pub max_rows: Option<u16>,  // Maximum height (enforced during resize)
    #[serde(default)]
    pub min_cols: Option<u16>,  // Minimum width (enforced during resize)
    #[serde(default)]
    pub max_cols: Option<u16>,  // Maximum width (enforced during resize)
    // Progress bar display options
    #[serde(default = "default_false")]
    pub numbers_only: bool,  // For progress bars: strip words, show only numbers (e.g., "health 325/326" -> "325/326")
    #[serde(default)]
    pub progress_id: Option<String>,  // ID for progress bar updates (e.g., "health", "mana", "stance")
    #[serde(default)]
    pub countdown_id: Option<String>,  // ID for countdown updates (e.g., "roundtime", "casttime", "stuntime")
    #[serde(default)]
    pub effect_default_color: Option<String>,  // Default color for effects without explicit color
    // Injury doll color configuration
    #[serde(default)]
    pub injury_default_color: Option<String>,  // Default/none injury color (index 0)
    #[serde(default)]
    pub injury1_color: Option<String>,  // Injury level 1 color
    #[serde(default)]
    pub injury2_color: Option<String>,  // Injury level 2 color
    #[serde(default)]
    pub injury3_color: Option<String>,  // Injury level 3 color
    #[serde(default)]
    pub scar1_color: Option<String>,  // Scar level 1 color
    #[serde(default)]
    pub scar2_color: Option<String>,  // Scar level 2 color
    #[serde(default)]
    pub scar3_color: Option<String>,  // Scar level 3 color
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabConfig {
    pub name: String,    // Tab display name
    pub stream: String,  // Stream to route to this tab
    #[serde(default)]
    pub show_timestamps: Option<bool>,  // Show timestamps at end of lines for this tab
}

impl Default for WindowDef {
    fn default() -> Self {
        Self {
            name: "new_window".to_string(),
            widget_type: default_widget_type(),
            streams: Vec::new(),
            row: 0,
            col: 0,
            rows: default_rows(),
            cols: default_cols(),
            buffer_size: default_buffer_size(),
            show_border: default_show_border(),
            border_style: None,
            border_color: None,
            border_sides: None,
            title: None,
            content_align: None,
            background_color: None,
            bar_fill: None,
            bar_background: None,
            text_color: None,
            transparent_background: default_transparent_background(),
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            show_timestamps: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            progress_id: None,
            countdown_id: None,
            effect_default_color: None,
            injury_default_color: None,
            injury1_color: None,
            injury2_color: None,
            injury3_color: None,
            scar1_color: None,
            scar2_color: None,
            scar3_color: None,
        }
    }
}

impl WindowDef {
    /// Resolve an optional string field with three-state logic:
    /// - None = use provided default
    /// - Some("-") = explicitly empty (return None)
    /// - Some(value) = use value
    pub fn resolve_optional_string(field: &Option<String>, default: &str) -> Option<String> {
        match field {
            None => Some(default.to_string()),  // Use default
            Some(s) if s == "-" => None,        // Explicitly empty
            Some(s) => Some(s.clone()),         // Use value
        }
    }

    /// Get the effective border color (with global default fallback)
    pub fn get_border_color(&self, colors: &ColorConfig) -> Option<String> {
        Self::resolve_optional_string(&self.border_color, &colors.ui.border_color)
    }

    /// Get the effective text color (with global default fallback)
    pub fn get_text_color(&self, colors: &ColorConfig) -> Option<String> {
        Self::resolve_optional_string(&self.text_color, &colors.ui.text_color)
    }

    /// Get the effective border style (with global default fallback)
    pub fn get_border_style(&self, ui_config: &UiConfig) -> Option<String> {
        Self::resolve_optional_string(&self.border_style, &ui_config.border_style)
    }

    /// Get the effective background color (with global default fallback)
    pub fn get_background_color(&self, colors: &ColorConfig) -> Option<String> {
        Self::resolve_optional_string(&self.background_color, &colors.ui.background_color)
    }
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
    #[serde(default = "default_border_style")]
    pub border_style: String,  // Default border style: "single", "double", "rounded", "thick", "none"
    #[serde(default = "default_countdown_icon")]
    pub countdown_icon: String,  // Unicode character for countdown blocks (e.g., "\u{f0c8}")
    #[serde(default = "default_poll_timeout_ms")]
    pub poll_timeout_ms: u64,  // Event poll timeout in milliseconds (lower = higher FPS, higher CPU)
    // Startup music settings
    #[serde(default = "default_startup_music")]
    pub startup_music: bool,  // Play startup music on connection
    #[serde(default = "default_startup_music_file")]
    pub startup_music_file: String,  // Sound file to play on startup (without extension)
    // Text selection settings
    #[serde(default = "default_selection_enabled")]
    pub selection_enabled: bool,
    #[serde(default = "default_selection_respect_window_boundaries")]
    pub selection_respect_window_boundaries: bool,
    // Drag and drop settings
    #[serde(default = "default_drag_modifier_key")]
    pub drag_modifier_key: String,  // Modifier key required for drag and drop (e.g., "ctrl", "alt", "shift")
    // Command history settings
    #[serde(default = "default_min_command_length")]
    pub min_command_length: usize,  // Minimum command length to save to history (commands shorter than this are not saved)
    // Performance stats settings
    #[serde(default = "default_perf_stats_x")]
    pub perf_stats_x: u16,
    #[serde(default = "default_perf_stats_y")]
    pub perf_stats_y: u16,
    #[serde(default = "default_perf_stats_width")]
    pub perf_stats_width: u16,
    #[serde(default = "default_perf_stats_height")]
    pub perf_stats_height: u16,
    // Ignore/Squelch settings
    #[serde(default = "default_ignores_enabled")]
    pub ignores_enabled: bool,  // Global toggle for squelch patterns (default: true)
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
    pub terminal_width: Option<u16>,   // Designed terminal width (for resize calculations)
    #[serde(default)]
    pub terminal_height: Option<u16>,  // Designed terminal height (for resize calculations)
    #[serde(default)]
    pub base_layout: Option<String>,   // Reference to base layout (for auto layouts)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_position: Option<crate::terminal_position::TerminalPosition>,  // Terminal window position/size (autosave only)
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub squelch: bool,  // If true, discard the entire line when pattern matches (ignores)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,  // Sound file to play when pattern matches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_volume: Option<f32>,  // Volume override for this sound (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,  // Category for grouping highlights (e.g., "Combat", "Healing", "Death")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPattern {
    pub pattern: String,           // Regex pattern to match
    pub event_type: String,        // Event type: "stun", "webbed", "prone", etc.
    pub action: EventAction,       // Action to perform: set/clear/increment
    #[serde(default)]
    pub duration: u32,             // Duration in seconds (0 = don't change)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_capture: Option<usize>,  // Regex capture group for duration (1-based)
    #[serde(default = "default_duration_multiplier")]
    pub duration_multiplier: f32,  // Multiply captured duration (e.g., 5.0 for rounds->seconds)
    #[serde(default = "default_enabled")]
    pub enabled: bool,             // Can disable without deleting
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventAction {
    Set,       // Set state/timer (e.g., start stun countdown)
    Clear,     // Clear state/timer (e.g., recover from stun)
    Increment, // Add to existing value (future use)
}

impl Default for EventAction {
    fn default() -> Self {
        EventAction::Set
    }
}

fn default_duration_multiplier() -> f32 { 1.0 }
fn default_enabled() -> bool { true }

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
        "insert" => KeyCode::Insert,
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

fn default_background_color() -> String {
    "-".to_string() // transparent/no background
}

fn default_startup_music() -> bool {
    true  // Enable by default - nostalgic easter egg
}

fn default_startup_music_file() -> String {
    "wizard_music".to_string()  // Default to wizard_music for nostalgia
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

fn default_poll_timeout_ms() -> u64 {
    16  // 16ms = ~60 FPS, 8ms = ~120 FPS, 4ms = ~240 FPS
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
    0  // Calculated dynamically: terminal_width - 35
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
            content_align: Some("top-left".to_string()),
            background_color: None,
            bar_fill: None,
            bar_background: None,
            text_color: None,
            transparent_background: true,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            bar_fill: Some("#6e0202".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            bar_fill: Some("#08086d".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            bar_fill: Some("#bd7b00".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            bar_fill: Some("#6e727c".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            bar_fill: Some("#008b8b".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            bar_fill: Some("#000080".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            bar_fill: Some("#006400".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
        },
        // Countdown timers (row 27-29, right side)
        WindowDef {
            name: "roundtime".to_string(),
            widget_type: "countdown".to_string(),
            streams: vec![],
            row: 27,
            col: 45,
            rows: 3,
            cols: 10,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("RT".to_string()),
            content_align: None,
            background_color: None,
            bar_fill: Some("#ff0000".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
        },
        WindowDef {
            name: "casttime".to_string(),
            widget_type: "countdown".to_string(),
            streams: vec![],
            row: 27,
            col: 60,
            rows: 3,
            cols: 10,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Cast".to_string()),
            content_align: None,
            background_color: None,
            bar_fill: Some("#0000ff".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
        },
        WindowDef {
            name: "stuntime".to_string(),
            widget_type: "countdown".to_string(),
            streams: vec![],
            row: 27,
            col: 75,
            rows: 3,
            cols: 10,
            buffer_size: 0,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: None,
            title: Some("Stun".to_string()),
            content_align: None,
            background_color: None,
            bar_fill: Some("#ffff00".to_string()),
            bar_background: Some("#000000".to_string()),
            text_color: None,
            transparent_background: false,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("top-left".to_string()),
            background_color: None,
            bar_fill: None,
            bar_background: None,
            text_color: None,
            transparent_background: true,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("top-left".to_string()),
            background_color: None,
            bar_fill: None,
            bar_background: None,
            text_color: None,
            transparent_background: true,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
        },
        // Command input (bottom of screen)
        WindowDef {
            name: "command_input".to_string(),
            widget_type: "command_input".to_string(),
            streams: vec![],
            row: 40,     // Bottom of screen (will be calculated dynamically)
            col: 0,
            rows: 3,     // 3 rows tall (with border)
            cols: 120,   // Full width (will use actual terminal width)
            buffer_size: 0,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: None,
            title: None,
            content_align: None,
            background_color: None,
            bar_fill: None,
            bar_background: None,
            text_color: None,
            transparent_background: true,
            locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: Some(1),  // Command input needs at least 1 row
            max_rows: Some(5),  // Don't let it get too tall
            min_cols: Some(20), // Needs some width to be usable
            max_cols: None,     // Can be full width
            numbers_only: false,
            ..Default::default()
        },
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
    pub fn load_with_terminal_size(character: Option<&str>, terminal_size: Option<(u16, u16)>) -> Result<(Self, Option<String>)> {
        let profile_dir = Config::profile_dir(character)?;
        let shared_layouts_dir = Config::layouts_dir()?;  // ~/.vellum-fe/default/layouts/

        // 1. Try character auto-save layout: ~/.vellum-fe/{character}/layout.toml
        let auto_layout_path = profile_dir.join("layout.toml");
        if auto_layout_path.exists() {
            tracing::info!("Loading auto-save layout from {:?}", auto_layout_path);
            let mut layout = Self::load_from_file(&auto_layout_path)?;
            let base_name = layout.base_layout.clone().unwrap_or_else(|| "default".to_string());

            // Check if we need to scale from base layout
            if let Some((curr_width, curr_height)) = terminal_size {
                if let (Some(layout_width), Some(layout_height)) = (layout.terminal_width, layout.terminal_height) {
                    if curr_width != layout_width || curr_height != layout_height {
                        tracing::info!("Terminal size changed from {}x{} to {}x{}, scaling from base layout",
                            layout_width, layout_height, curr_width, curr_height);

                        // Try to load base layout for accurate scaling
                        let base_path = shared_layouts_dir.join(format!("{}.toml", base_name));
                        if base_path.exists() {
                            match Self::load_from_file(&base_path) {
                                Ok(base_layout) => {
                                    tracing::info!("Scaling from base layout '{}'", base_name);
                                    layout = base_layout;
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to load base layout '{}': {}, using auto layout as base", base_name, e);
                                }
                            }
                        }

                        // Scale to current terminal size
                        layout.scale_to_terminal_size(curr_width, curr_height);
                    }
                }
            }

            return Ok((layout, Some(base_name)));
        }

        // 2. Try shared default layout: ~/.vellum-fe/layouts/layout.toml
        let default_path = shared_layouts_dir.join("layout.toml");
        if default_path.exists() {
            tracing::info!("Loading shared default layout from {:?}", default_path);
            let layout = Self::load_from_file(&default_path)?;
            return Ok((layout, Some("layout".to_string())));
        }

        // 3. Fall back to embedded default (should have been extracted by extract_defaults())
        tracing::warn!("No layout found, using embedded default (this should have been extracted!)");
        let layout: Layout = toml::from_str(LAYOUT_DEFAULT)
            .context("Failed to parse embedded default layout")?;

        Ok((layout, Some("layout".to_string())))
    }

    /// Scale all windows proportionally to fit new terminal size
    pub fn scale_to_terminal_size(&mut self, new_width: u16, new_height: u16) {
        let base_width = self.terminal_width.unwrap_or(new_width);
        let base_height = self.terminal_height.unwrap_or(new_height);

        if base_width == 0 || base_height == 0 {
            tracing::warn!("Invalid base terminal size ({}x{}), skipping scale", base_width, base_height);
            return;
        }

        let scale_x = new_width as f32 / base_width as f32;
        let scale_y = new_height as f32 / base_height as f32;

        tracing::info!("Scaling layout from {}x{} to {}x{} (scale: {:.2}x, {:.2}y)",
            base_width, base_height, new_width, new_height, scale_x, scale_y);

        for window in &mut self.windows {
            let old_col = window.col;
            let old_row = window.row;
            let old_cols = window.cols;
            let old_rows = window.rows;

            window.col = (window.col as f32 * scale_x).round() as u16;
            window.row = (window.row as f32 * scale_y).round() as u16;
            window.cols = (window.cols as f32 * scale_x).round() as u16;
            window.rows = (window.rows as f32 * scale_y).round() as u16;

            // Ensure minimum sizes
            if window.cols < 1 { window.cols = 1; }
            if window.rows < 1 { window.rows = 1; }

            // Respect min/max constraints if set
            if let Some(min_cols) = window.min_cols {
                if window.cols < min_cols {
                    window.cols = min_cols;
                }
            }
            if let Some(max_cols) = window.max_cols {
                if window.cols > max_cols {
                    window.cols = max_cols;
                }
            }
            if let Some(min_rows) = window.min_rows {
                if window.rows < min_rows {
                    window.rows = min_rows;
                }
            }
            if let Some(max_rows) = window.max_rows {
                if window.rows > max_rows {
                    window.rows = max_rows;
                }
            }

            tracing::debug!("  {} [{}]: pos {}x{} -> {}x{}, size {}x{} -> {}x{}",
                window.name, window.widget_type,
                old_col, old_row, window.col, window.row,
                old_cols, old_rows, window.cols, window.rows);
        }

        // Update terminal size to new size
        self.terminal_width = Some(new_width);
        self.terminal_height = Some(new_height);
    }

    pub fn load_from_file(path: &std::path::Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .context(format!("Failed to read layout file: {:?}", path))?;
        let mut layout: Layout = toml::from_str(&contents)
            .context(format!("Failed to parse layout file: {:?}", path))?;

        // Debug: Log what terminal size was loaded
        tracing::debug!(
            "Loaded layout from {:?}: terminal_width={:?}, terminal_height={:?}",
            path, layout.terminal_width, layout.terminal_height
        );

        // Migration: Ensure command_input exists in windows array with valid values
        if let Some(idx) = layout.windows.iter().position(|w| w.widget_type == "command_input") {
            // Command input exists but might have invalid values (cols=0, rows=0, etc)
            let cmd_input = &mut layout.windows[idx];
            if cmd_input.cols == 0 || cmd_input.rows == 0 {
                tracing::warn!("Command input has invalid size ({}x{}), fixing with defaults", cmd_input.rows, cmd_input.cols);
                // Get defaults from default_windows()
                if let Some(default_cmd) = default_windows().into_iter().find(|w| w.widget_type == "command_input") {
                    cmd_input.row = default_cmd.row;
                    cmd_input.col = default_cmd.col;
                    cmd_input.rows = default_cmd.rows;
                    cmd_input.cols = default_cmd.cols;
                }
            }
        } else {
            // Command input doesn't exist - add it
            if let Some(cmd_input) = default_windows().into_iter().find(|w| w.widget_type == "command_input") {
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
        for window in &mut self.windows {
            // Convert None to Some("-") for color fields to preserve transparency setting
            let normalize = |field: &mut Option<String>| {
                if field.is_none() {
                    *field = Some("-".to_string());
                }
            };

            normalize(&mut window.background_color);
            normalize(&mut window.border_color);
            normalize(&mut window.bar_fill);
            normalize(&mut window.bar_background);
            normalize(&mut window.text_color);
            normalize(&mut window.tab_active_color);
            normalize(&mut window.tab_inactive_color);
            normalize(&mut window.tab_unread_color);
            normalize(&mut window.compass_active_color);
            normalize(&mut window.compass_inactive_color);
        }
    }

    pub fn save(&mut self, name: &str, terminal_size: Option<(u16, u16)>, force_terminal_size: bool) -> Result<()> {
        // Capture terminal size for layout baseline
        if force_terminal_size {
            // Force update terminal size (used by .resize to match resized widgets)
            if let Some((width, height)) = terminal_size {
                tracing::info!("Forcing layout terminal size to {}x{} (was {:?}x{:?})",
                    width, height, self.terminal_width, self.terminal_height);
                self.terminal_width = Some(width);
                self.terminal_height = Some(height);
            }
        } else if self.terminal_width.is_none() || self.terminal_height.is_none() {
            // Only set if not already set
            if let Some((width, height)) = terminal_size {
                self.terminal_width = Some(width);
                self.terminal_height = Some(height);
                tracing::info!("Set layout terminal size to {}x{} (was not previously set)", width, height);
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
        let toml_string = toml::to_string_pretty(&self)
            .context("Failed to serialize layout")?;
        fs::write(&layout_path, toml_string)
            .context("Failed to write layout file")?;

        tracing::info!("Saved layout '{}' to {:?}", name, layout_path);
        Ok(())
    }

    /// Save as character auto-save layout (on exit/resize)
    /// Saves to: ~/.vellum-fe/{character}/layout.toml
    pub fn save_auto(&mut self, character: &str, base_layout_name: &str, terminal_size: Option<(u16, u16)>) -> Result<()> {
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
        let toml_string = toml::to_string_pretty(&self)
            .context("Failed to serialize auto layout")?;
        fs::write(&layout_path, toml_string)
            .context("Failed to write auto layout file")?;

        tracing::info!("Saved auto layout for {} to {:?} (base: {}, terminal: {:?}x{:?})",
            character, layout_path, base_layout_name, self.terminal_width, self.terminal_height);

        Ok(())
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
                    width, height, mapping.layout,
                    mapping.min_width, mapping.min_height,
                    mapping.max_width, mapping.max_height
                );
                return Some(mapping.layout.clone());
            }
        }
        tracing::debug!("No layout mapping found for terminal size {}x{}", width, height);
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
        if color_input.is_empty() || color_input.eq_ignore_ascii_case("none") || color_input == "-" {
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
                content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "inventory" | "inv" => Some(WindowDef {
                name: "inventory".to_string(),
                widget_type: "inventory".to_string(),
                streams: vec!["inv".to_string()],
                row: default_row,
                col: default_col,
                rows: 20,
                cols: 60,
                buffer_size: 0, // Not used by inventory widget
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Inventory".to_string()),
            content_align: Some("top".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "room" => Some(WindowDef {
                name: "room".to_string(),
                widget_type: "room".to_string(),
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "local_map" => Some(WindowDef {
                name: "map".to_string(),
                widget_type: "map".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 25,
                cols: 40,
                buffer_size: 0,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Local Map".to_string()),
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "spells" => Some(WindowDef {
                name: "spells".to_string(),
                widget_type: "spells".to_string(),
                streams: vec!["Spells".to_string()],
                row: default_row,
                col: default_col,
                rows: 25,
                cols: 40,
                buffer_size: 0, // Not used by spells widget
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Spells".to_string()),
            content_align: Some("top".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            content_align: Some("center".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "bounty" | "bounties" => Some(WindowDef {
                name: "bounty".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["bounty".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 0,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Bounties".to_string()),
            content_align: None,
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "spells" => Some(WindowDef {
                name: "spells".to_string(),
                widget_type: "text".to_string(),
                streams: vec!["Spells".to_string()],
                row: default_row,
                col: default_col,
                rows: default_rows,
                cols: default_cols,
                buffer_size: 10000,
                show_border: true,
                border_style: None,
                border_color: None,
            border_sides: None,
                title: Some("Spells".to_string()),
            content_align: None,
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#6e0202".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#08086d".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#bd7b00".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#6e727c".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#008b8b".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#006400".to_string()), // Will change dynamically based on value
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#000080".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#4d0085".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "roundtime" | "rt" => Some(WindowDef {
                name: "roundtime".to_string(),
                widget_type: "countdown".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 10,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("RT".to_string()),
            content_align: None,
            background_color: None,
                bar_fill: Some("#ff0000".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "casttime" | "cast" => Some(WindowDef {
                name: "casttime".to_string(),
                widget_type: "countdown".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 10,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Cast".to_string()),
            content_align: None,
            background_color: None,
                bar_fill: Some("#0000ff".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "stun" | "stuntime" => Some(WindowDef {
                name: "stuntime".to_string(),
                widget_type: "countdown".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,
                cols: 10,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Stun".to_string()),
            content_align: None,
            background_color: None,
                bar_fill: Some("#ffff00".to_string()),
                bar_background: Some("#000000".to_string()),
                text_color: None,
                transparent_background: false,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "compass" => Some(WindowDef {
                name: "compass".to_string(),
                widget_type: "compass".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 5,  // 3 rows for grid + 2 for border
                cols: 10,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Exits".to_string()),
            content_align: Some("center-left".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "injuries" | "injury_doll" => Some(WindowDef {
                name: "injuries".to_string(),
                widget_type: "injury_doll".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 8,  // 6 rows for body + 2 for border
                cols: 10,
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Injuries".to_string()),
            content_align: Some("center-left".to_string()),
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "lefthand" => Some(WindowDef {
                name: "lefthand".to_string(),
                widget_type: "lefthand".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 20, // Hand item display
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Left Hand".to_string()),
            content_align: None,
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
                hand_icon: Some("L:".to_string()),
                countdown_icon: None,
                compass_active_color: None,
                compass_inactive_color: None,
                min_rows: None,
                max_rows: None,
                min_cols: None,
                max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "righthand" => Some(WindowDef {
                name: "righthand".to_string(),
                widget_type: "righthand".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 20, // Hand item display
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Right Hand".to_string()),
            content_align: None,
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
                hand_icon: Some("R:".to_string()),
                countdown_icon: None,
                compass_active_color: None,
                compass_inactive_color: None,
                min_rows: None,
                max_rows: None,
                min_cols: None,
                max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "spellhand" => Some(WindowDef {
                name: "spellhand".to_string(),
                widget_type: "spellhand".to_string(),
                streams: vec![],
                row: default_row,
                col: default_col,
                rows: 3,  // 1 row + 2 for border
                cols: 20, // Hand item display
                buffer_size: 0,
                show_border: true,
                border_style: Some("single".to_string()),
                border_color: None,
            border_sides: None,
                title: Some("Spell".to_string()),
            content_align: None,
            background_color: None,
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
                hand_icon: Some("S:".to_string()),
                countdown_icon: None,
                compass_active_color: None,
                compass_inactive_color: None,
                min_rows: None,
                max_rows: None,
                min_cols: None,
                max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: Some(vec!["-".to_string(), "#00ff00".to_string()]), // off (transparent), green (on)
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: Some(vec!["-".to_string(), "#8b4513".to_string()]), // off (transparent), brownish-red (on)
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: Some(vec!["-".to_string(), "#ff0000".to_string()]), // off (transparent), red (on)
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: Some(vec!["-".to_string(), "#ffff00".to_string()]), // off (transparent), yellow (on)
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: Some(vec!["-".to_string(), "#cccccc".to_string()]), // off (transparent), bright grey (on)
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: None,
                dashboard_layout: Some("horizontal".to_string()),
                dashboard_indicators: Some(vec![
                    DashboardIndicatorDef {
                        id: "poisoned".to_string(),
                        icon: "\u{e231}".to_string(),
                        colors: vec!["-".to_string(), "#00ff00".to_string()],
                    },
                    DashboardIndicatorDef {
                        id: "diseased".to_string(),
                        icon: "\u{e286}".to_string(),
                        colors: vec!["-".to_string(), "#8b4513".to_string()],
                    },
                    DashboardIndicatorDef {
                        id: "bleeding".to_string(),
                        icon: "\u{f043}".to_string(),
                        colors: vec!["-".to_string(), "#ff0000".to_string()],
                    },
                    DashboardIndicatorDef {
                        id: "stunned".to_string(),
                        icon: "\u{f0e7}".to_string(),
                        colors: vec!["-".to_string(), "#ffff00".to_string()],
                    },
                    DashboardIndicatorDef {
                        id: "webbed".to_string(),
                        icon: "\u{f0bca}".to_string(),
                        colors: vec!["-".to_string(), "#cccccc".to_string()],
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#40FF40".to_string()),
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,  // Auto-adjust to window height
                effect_category: Some("Buffs".to_string()),
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#FF4040".to_string()),
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,  // Auto-adjust to window height
                effect_category: Some("Debuffs".to_string()),
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#FFB040".to_string()),
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,  // Auto-adjust to window height
                effect_category: Some("Cooldowns".to_string()),
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
                bar_fill: Some("#4080FF".to_string()),
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
                indicator_colors: None,
                dashboard_layout: None,
                dashboard_indicators: None,
                dashboard_spacing: None,
                dashboard_hide_inactive: None,
                visible_count: None,
                effect_category: Some("ActiveSpells".to_string()),
                tabs: None,
                tab_bar_position: None,
                tab_active_color: None,
                tab_inactive_color: None,
                tab_unread_color: None,
                tab_unread_prefix: None,
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "targets" => Some(WindowDef {
                name: "targets".to_string(),
                widget_type: "entity".to_string(),
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
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
            }),
            "players" => Some(WindowDef {
                name: "players".to_string(),
                widget_type: "entity".to_string(),
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
                bar_fill: None,
                bar_background: None,
                text_color: None,
                transparent_background: true,
                locked: false,
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
            hand_icon: None,
            countdown_icon: None,
            compass_active_color: None,
            compass_inactive_color: None,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            numbers_only: false,
            ..Default::default()
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
            "inventory",
            "room",
            "logons",
            "deaths",
            "arrivals",
            "ambients",
            "announcements",
            "loot",
            "bounty",
            "spells",
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
    /// Extract default files on first run
    /// Creates shared directories and profile-specific files
    ///
    /// Shared:
    /// - ~/.vellum-fe/layouts/layout.toml
    /// - ~/.vellum-fe/layouts/none.toml
    /// - ~/.vellum-fe/layouts/sidebar.toml
    /// - ~/.vellum-fe/sounds/wizard_music.mp3
    /// - ~/.vellum-fe/sounds/README.md
    /// - ~/.vellum-fe/cmdlist1.xml
    ///
    /// Profile-specific (default or character):
    /// - ~/.vellum-fe/{profile}/config.toml
    /// - ~/.vellum-fe/{profile}/history.txt (empty)
    fn extract_defaults(character: Option<&str>) -> Result<()> {
        // Create shared layouts directory and extract all embedded layouts
        let layouts_dir = Self::layouts_dir()?;
        fs::create_dir_all(&layouts_dir)?;

        // Automatically extract all files from embedded layouts directory
        for file in LAYOUTS_DIR.files() {
            let filename = file.path().file_name()
                .and_then(|n| n.to_str())
                .context("Invalid layout filename")?;
            let layout_path = layouts_dir.join(filename);

            if !layout_path.exists() {
                let content = file.contents_utf8()
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
            let filename = file.path().file_name()
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

        // Extract cmdlist1.xml to shared location (only once)
        let cmdlist_path = Self::cmdlist_path()?;
        if !cmdlist_path.exists() {
            fs::write(&cmdlist_path, DEFAULT_CMDLIST)
                .context("Failed to write cmdlist1.xml")?;
            tracing::info!("Extracted cmdlist1.xml to {:?}", cmdlist_path);
        }

        // Create profile directory
        let profile = Self::profile_dir(character)?;
        fs::create_dir_all(&profile)?;
        tracing::info!("Created profile directory: {:?}", profile);

        // Extract config.toml to profile (if it doesn't exist)
        let config_path = profile.join("config.toml");
        if !config_path.exists() {
            fs::write(&config_path, DEFAULT_CONFIG)
                .context("Failed to write config.toml")?;
            tracing::info!("Extracted config.toml to {:?}", config_path);
        }

        // Extract colors.toml to profile (if it doesn't exist)
        let colors_path = profile.join("colors.toml");
        if !colors_path.exists() {
            fs::write(&colors_path, DEFAULT_COLORS)
                .context("Failed to write colors.toml")?;
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
            fs::write(&keybinds_path, DEFAULT_KEYBINDS)
                .context("Failed to write keybinds.toml")?;
            tracing::info!("Extracted keybinds.toml to {:?}", keybinds_path);
        }

        // Create empty history.txt in profile (if it doesn't exist)
        let history_path = profile.join("history.txt");
        if !history_path.exists() {
            fs::write(&history_path, "")
                .context("Failed to create history.txt")?;
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
        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        fs::write(&config_path, contents)
            .context("Failed to write config file")?;

        // Save to separate files
        self.colors.save(char_name)?;
        self.save_highlights(char_name)?;
        self.save_keybinds(char_name)?;

        Ok(())
    }

    /// Get the profile directory for a character (or "default" if none)
    /// Returns: ~/.vellum-fe/profiles/{character}/ or ~/.vellum-fe/profiles/default/
    fn profile_dir(character: Option<&str>) -> Result<PathBuf> {
        let profile_name = character.unwrap_or("default");
        Ok(Self::config_dir()?.join("profiles").join(profile_name))
    }

    /// Get the base vellum-fe directory (~/.vellum-fe/)
    pub fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        Ok(home.join(".vellum-fe"))
    }

    /// Get path to config.toml for a character
    /// Returns: ~/.vellum-fe/profiles/{character}/config.toml or ~/.vellum-fe/profiles/default/config.toml
    pub fn config_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("config.toml"))
    }

    /// Get path to colors.toml for a character
    /// Returns: ~/.vellum-fe/profiles/{character}/colors.toml
    pub fn colors_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("colors.toml"))
    }

    /// Get the shared layouts directory (where .savelayout saves to)
    /// Returns: ~/.vellum-fe/layouts/
    fn layouts_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("layouts"))
    }

    /// Get the shared highlights directory (where .savehighlights saves to)
    /// Returns: ~/.vellum-fe/presets/highlights/
    fn highlights_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("presets").join("highlights"))
    }

    /// Get the shared keybinds directory (where .savekeybinds saves to)
    /// Returns: ~/.vellum-fe/presets/keybinds/
    fn keybinds_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("presets").join("keybinds"))
    }

    /// Get the shared sounds directory
    /// Returns: ~/.vellum-fe/global/sounds/
    pub fn sounds_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("global").join("sounds"))
    }

    /// Get path to debug log for a character
    /// Returns: ~/.vellum-fe/profiles/{character}/debug.log
    pub fn get_log_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("debug.log"))
    }

    /// Get path to command history for a character
    /// Returns: ~/.vellum-fe/profiles/{character}/history.txt
    pub fn history_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("history.txt"))
    }

    /// Get path to cmdlist1.xml (single source of truth)
    /// Returns: ~/.vellum-fe/global/cmdlist1.xml
    pub fn cmdlist_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("global").join("cmdlist1.xml"))
    }

    /// Get path to highlights.toml for a character
    /// Returns: ~/.vellum-fe/profiles/{character}/highlights.toml
    pub fn highlights_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("highlights.toml"))
    }

    /// Get path to global highlights shared across all characters
    /// Returns: ~/.vellum-fe/global/highlights.toml
    pub fn common_highlights_path() -> Result<PathBuf> {
        let global_dir = Self::config_dir()?.join("global");
        if !global_dir.exists() {
            fs::create_dir_all(&global_dir)?;
        }
        Ok(global_dir.join("highlights.toml"))
    }

    /// Get path to keybinds.toml for a character
    /// Returns: ~/.vellum-fe/profiles/{character}/keybinds.toml
    pub fn keybinds_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("keybinds.toml"))
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

    /// List all saved highlight profiles
    pub fn list_saved_highlights() -> Result<Vec<String>> {
        let highlights_dir = Self::highlights_dir()?;

        if !highlights_dir.exists() {
            return Ok(vec![]);
        }

        let mut profiles = vec![];
        for entry in fs::read_dir(highlights_dir)? {
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

    /// Save current highlights to a named profile
    /// Returns path to saved highlights
    pub fn save_highlights_as(&self, name: &str) -> Result<PathBuf> {
        let highlights_dir = Self::highlights_dir()?;
        fs::create_dir_all(&highlights_dir)?;

        let highlights_path = highlights_dir.join(format!("{}.toml", name));
        let contents = toml::to_string_pretty(&self.highlights)
            .context("Failed to serialize highlights")?;
        fs::write(&highlights_path, contents)
            .context("Failed to write highlights profile")?;

        Ok(highlights_path)
    }

    /// Load highlights from a named profile
    pub fn load_highlights_from(name: &str) -> Result<HashMap<String, HighlightPattern>> {
        let highlights_dir = Self::highlights_dir()?;
        let highlights_path = highlights_dir.join(format!("{}.toml", name));

        if !highlights_path.exists() {
            return Err(anyhow::anyhow!("Highlight profile '{}' not found", name));
        }

        let contents = fs::read_to_string(&highlights_path)
            .context("Failed to read highlights profile")?;
        let highlights: HashMap<String, HighlightPattern> = toml::from_str(&contents)
            .context("Failed to parse highlights profile")?;

        Ok(highlights)
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
        let contents = toml::to_string_pretty(&self.keybinds)
            .context("Failed to serialize keybinds")?;
        fs::write(&keybinds_path, contents)
            .context("Failed to write keybinds profile")?;

        Ok(keybinds_path)
    }

    /// Load keybinds from a named profile
    pub fn load_keybinds_from(name: &str) -> Result<HashMap<String, KeyBindAction>> {
        let keybinds_dir = Self::keybinds_dir()?;
        let keybinds_path = keybinds_dir.join(format!("{}.toml", name));

        if !keybinds_path.exists() {
            return Err(anyhow::anyhow!("Keybind profile '{}' not found", name));
        }

        let contents = fs::read_to_string(&keybinds_path)
            .context("Failed to read keybinds profile")?;
        let keybinds: HashMap<String, KeyBindAction> = toml::from_str(&contents)
            .context("Failed to parse keybinds profile")?;

        Ok(keybinds)
    }

    /// Resolve a spell ID to a color based on configured spell lists
    /// Example: spells = [101, 107, 120, 140, 150]
    pub fn get_spell_color(&self, spell_id: u32) -> Option<String> {
        for spell_config in &self.colors.spell_colors {
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
                border_style: default_border_style(),
                countdown_icon: default_countdown_icon(),
                poll_timeout_ms: default_poll_timeout_ms(),
                startup_music: default_startup_music(),
                startup_music_file: default_startup_music_file(),
                selection_enabled: default_selection_enabled(),
                selection_respect_window_boundaries: default_selection_respect_window_boundaries(),
                drag_modifier_key: default_drag_modifier_key(),
                min_command_length: default_min_command_length(),
                perf_stats_x: default_perf_stats_x(),
                perf_stats_y: default_perf_stats_y(),
                perf_stats_width: default_perf_stats_width(),
                perf_stats_height: default_perf_stats_height(),
                ignores_enabled: default_ignores_enabled(),
            },
            highlights: HashMap::new(),  // Loaded from highlights.toml
            keybinds: HashMap::new(),  // Loaded from keybinds.toml
            colors: ColorConfig::default(),  // Loaded from colors.toml
            sound: SoundConfig::default(),
            event_patterns: HashMap::new(),  // Empty by default - user adds via config
            layout_mappings: Vec::new(),  // Empty by default - user adds via config
            character: None,  // Set at runtime via load_with_options
        }
    }
}
