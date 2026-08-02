//! Color configuration: palette, presets, prompt/spell colors, UI colors.
//!
//! Backs colors.toml (`ColorConfig`) plus the palette-resolution and
//! theme-lookup helpers on `Config`.

use super::*;

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
    #[serde(default = "default_system_message_color")]
    pub system_message_color: String, // Client feedback lines (.theme, [jinx], errors)
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

impl UiColors {
    /// The user's chosen value, or None when it still equals the built-in
    /// default (or the "-" unset sentinel). Extracted colors.toml files
    /// carry the concrete defaults, so equality with the default is
    /// indistinguishable from "never touched" — those fall through to the
    /// theme layer instead of overriding it.
    fn user_value<'a>(value: &'a str, default: &str) -> Option<&'a str> {
        if value == default || value == "-" || value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    pub fn user_border_color(&self) -> Option<&str> {
        Self::user_value(&self.border_color, &super::default_border_color_default())
    }

    pub fn user_focused_border_color(&self) -> Option<&str> {
        Self::user_value(
            &self.focused_border_color,
            &super::default_focused_border_color(),
        )
    }

    pub fn user_text_color(&self) -> Option<&str> {
        Self::user_value(&self.text_color, &super::default_text_color_default())
    }

    pub fn user_background_color(&self) -> Option<&str> {
        Self::user_value(&self.background_color, &super::default_background_color())
    }
}

/// Stored harmony-generation recipe (seed, scheme, controls, pins).
/// Generation is deterministic from this tuple, so a generated look stays
/// reproducible and re-tunable instead of frozen as opaque hex. Written by
/// `.harmony` / the GUI Generate tab; scheme is a `core::harmony::Scheme`
/// name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarmonyRecipe {
    pub seed: String,
    pub background: String,
    pub scheme: String,
    #[serde(default = "harmony_default_variance")]
    pub variance: f64,
    #[serde(default = "harmony_default_min_contrast")]
    pub min_contrast: f64,
    #[serde(default = "harmony_default_separation")]
    pub separation: f64,
    #[serde(default = "harmony_default_room_spread")]
    pub room_title_spread: f64,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub pins: HashMap<String, String>,
}

fn harmony_default_variance() -> f64 {
    1.0
}
fn harmony_default_min_contrast() -> f64 {
    4.5
}
fn harmony_default_separation() -> f64 {
    0.09
}
fn harmony_default_room_spread() -> f64 {
    2.5
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
    /// Last harmony-generation recipe, if the presets were ever generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harmony: Option<HarmonyRecipe>,
}

impl Default for UiColors {
    fn default() -> Self {
        Self {
            command_echo_color: default_command_echo_color(),
            system_message_color: default_system_message_color(),
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
                harmony: None,
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
                harmony: None,
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

        // Character harmony recipe overrides the global one, if present
        if char_colors.harmony.is_some() {
            colors.harmony = char_colors.harmony;
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
        write_atomic(&colors_path, contents).context("Failed to write colors.toml")?;
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
        write_atomic(&colors_path, contents).context("Failed to write global colors.toml")?;
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

    /// Apply generated preset colors in-memory. Named palette entries that a
    /// preset references are recolored in place (so slot-mode terminals and
    /// every other consumer of the name update together); presets whose value
    /// is a literal hex get the new hex directly. When two roles share one
    /// palette entry (links and commands both ship pointing at "Link"), the
    /// first role keeps the shared entry and later roles switch their preset
    /// to a literal hex so both generated colors survive.
    ///
    /// Returns the palette-entry recolors performed as `(name, hex)` — the
    /// character-scope shadow logic in `persist_generated_presets` needs them.
    pub fn apply_generated_presets(
        &mut self,
        colors: &[(String, String)],
        room_bg: &str,
    ) -> Vec<(String, String)> {
        let mut claimed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut palette_updates: Vec<(String, String)> = Vec::new();
        let mut apply_one = |cfg: &mut Self, role: &str, hex: &str, is_bg: bool| {
            let current = cfg.presets.get(role).and_then(|preset| {
                if is_bg {
                    preset.bg.clone()
                } else {
                    preset.fg.clone()
                }
            });
            let mut resolved_via_palette = false;
            if let Some(name) = current.as_deref().filter(|v| !v.starts_with('#')) {
                let key = name.to_lowercase();
                if let Some(entry) = cfg
                    .color_palette
                    .iter_mut()
                    .find(|entry| entry.name.to_lowercase() == key)
                {
                    if claimed.insert(key) {
                        entry.color = hex.to_string();
                        palette_updates.push((entry.name.clone(), hex.to_string()));
                        resolved_via_palette = true;
                    }
                }
            }
            if !resolved_via_palette {
                let preset = cfg.presets.entry(role.to_string()).or_insert(PresetColor {
                    fg: None,
                    bg: None,
                });
                if is_bg {
                    preset.bg = Some(hex.to_string());
                } else {
                    preset.fg = Some(hex.to_string());
                }
            }
        };
        for (role, hex) in colors {
            apply_one(self, role, hex, false);
        }
        apply_one(self, "roomName", room_bg, true);
        palette_updates
    }

    /// Persist a generated color set (and its recipe) honoring the global +
    /// character scope split. The full set lands in the global file; the
    /// character file is touched only where it already shadows the result —
    /// a non-empty character palette replaces the whole global list (so its
    /// matching entries are recolored and missing ones added), and per-key
    /// character preset overrides win over global (so those keys get the hex
    /// directly). Both writes are atomic with .bak backups.
    pub fn persist_generated_presets(
        colors: &[(String, String)],
        room_bg: &str,
        recipe: &HarmonyRecipe,
        character: Option<&str>,
    ) -> Result<()> {
        let mut global = Self::load_common_colors()?;
        let palette_updates = global.apply_generated_presets(colors, room_bg);
        global.harmony = Some(recipe.clone());
        global.save_common()?;

        let mut chara = Self::load_character_colors_only(character)?;
        let mut char_dirty = false;

        // A non-empty character palette shadows the ENTIRE global list, so
        // the recolors above would be invisible: mirror them here, adding
        // entries the character list lacks (keeping the name indirection).
        if !chara.color_palette.is_empty() {
            for (name, hex) in &palette_updates {
                let key = name.to_lowercase();
                match chara
                    .color_palette
                    .iter_mut()
                    .find(|entry| entry.name.to_lowercase() == key)
                {
                    Some(entry) => entry.color = hex.clone(),
                    None => chara
                        .color_palette
                        .push(PaletteColor::new(name, hex, "presets")),
                }
            }
            char_dirty = true;
        }

        // Character preset keys override global per-key: give literal-hex
        // (or empty) overrides the generated hex. Name-valued overrides
        // resolve through the palette, which the updates above already cover.
        let mut override_role = |chara: &mut Self, role: &str, hex: &str, is_bg: bool| {
            if let Some(preset) = chara.presets.get_mut(role) {
                let value = if is_bg { &mut preset.bg } else { &mut preset.fg };
                if !value.as_deref().is_some_and(|v| !v.starts_with('#')) {
                    *value = Some(hex.to_string());
                    return true;
                }
            }
            false
        };
        for (role, hex) in colors {
            char_dirty |= override_role(&mut chara, role, hex, false);
        }
        char_dirty |= override_role(&mut chara, "roomName", room_bg, true);

        if char_dirty {
            chara.save(character)?;
        }
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

#[cfg(test)]
mod harmony_apply_tests {
    use super::*;

    fn generated() -> Vec<(String, String)> {
        [
            ("links", "#111111"),
            ("commands", "#222222"),
            ("speech", "#333333"),
            ("roomName", "#444444"),
            ("target_indicator", "#bb2211"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
    }

    fn palette_hex(cfg: &ColorConfig, name: &str) -> String {
        cfg.color_palette
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("palette entry '{name}' missing"))
            .color
            .clone()
    }

    #[test]
    fn named_palette_entries_are_recolored_in_place() {
        // The shipped defaults: presets.speech -> palette "Speech" etc.
        let mut cfg = ColorConfig::default();
        cfg.apply_generated_presets(&generated(), "#0a0a14");
        assert_eq!(palette_hex(&cfg, "Speech"), "#333333");
        assert_eq!(palette_hex(&cfg, "Room Name"), "#444444");
        assert_eq!(palette_hex(&cfg, "Room Name BG"), "#0a0a14");
        // Name indirection preserved: the preset still points at the name.
        assert_eq!(cfg.presets["speech"].fg.as_deref(), Some("Speech"));
        assert_eq!(cfg.presets["roomName"].bg.as_deref(), Some("Room Name BG"));
    }

    #[test]
    fn shared_palette_entry_first_role_wins_later_roles_go_literal() {
        // links and commands both ship pointing at palette "Link"; the two
        // generated colors must BOTH survive.
        let mut cfg = ColorConfig::default();
        cfg.apply_generated_presets(&generated(), "#0a0a14");
        assert_eq!(palette_hex(&cfg, "Link"), "#111111", "links claims the entry");
        assert_eq!(
            cfg.presets["commands"].fg.as_deref(),
            Some("#222222"),
            "commands switches to a literal hex"
        );
    }

    #[test]
    fn literal_hex_presets_are_overwritten_directly() {
        // target_indicator ships as a literal (8-digit) hex, not a name.
        let mut cfg = ColorConfig::default();
        cfg.apply_generated_presets(&generated(), "#0a0a14");
        assert_eq!(cfg.presets["target_indicator"].fg.as_deref(), Some("#bb2211"));
    }

    #[test]
    fn missing_preset_keys_are_created() {
        let mut cfg = ColorConfig::default();
        cfg.presets.remove("speech");
        cfg.apply_generated_presets(&generated(), "#0a0a14");
        assert_eq!(cfg.presets["speech"].fg.as_deref(), Some("#333333"));
    }

    #[test]
    fn recipe_survives_a_toml_round_trip() {
        let mut cfg = ColorConfig::default();
        cfg.harmony = Some(HarmonyRecipe {
            seed: "#bf616a".into(),
            background: "#2e3440".into(),
            scheme: "triadic".into(),
            variance: 1.4,
            min_contrast: 7.0,
            separation: 0.04,
            room_title_spread: 2.5,
            pins: std::iter::once(("speech".to_string(), "#12ab34".to_string())).collect(),
        });
        let text = toml::to_string_pretty(&cfg).expect("serialize with recipe");
        let back: ColorConfig = toml::from_str(&text).expect("parse back");
        let recipe = back.harmony.expect("recipe kept");
        assert_eq!(recipe.seed, "#bf616a");
        assert_eq!(recipe.scheme, "triadic");
        assert_eq!(recipe.pins["speech"], "#12ab34");
        // And absent stays absent (no empty [harmony] table emitted).
        let cfg2 = ColorConfig::default();
        assert!(!toml::to_string_pretty(&cfg2).unwrap().contains("[harmony]"));
    }
}

#[cfg(test)]
mod ui_color_layer_tests {
    use super::UiColors;

    #[test]
    fn defaults_and_sentinels_are_not_user_values() {
        let ui = UiColors::default();
        assert_eq!(ui.user_border_color(), None);
        assert_eq!(ui.user_focused_border_color(), None);
        assert_eq!(ui.user_text_color(), None);
        assert_eq!(ui.user_background_color(), None);
    }

    #[test]
    fn changed_focused_border_is_a_user_value() {
        let mut ui = UiColors::default();
        ui.focused_border_color = "#ff00ff".to_string();
        assert_eq!(ui.user_focused_border_color(), Some("#ff00ff"));
        ui.focused_border_color = "-".to_string();
        assert_eq!(ui.user_focused_border_color(), None);
        ui.focused_border_color = String::new();
        assert_eq!(ui.user_focused_border_color(), None);
    }

    #[test]
    fn changed_values_are_user_values() {
        let mut ui = UiColors::default();
        ui.border_color = "#123456".to_string();
        ui.background_color = "#000011".to_string();
        assert_eq!(ui.user_border_color(), Some("#123456"));
        assert_eq!(ui.user_background_color(), Some("#000011"));
        ui.border_color = "-".to_string();
        assert_eq!(ui.user_border_color(), None);
    }
}
