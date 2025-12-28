use super::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RedirectMode {
    /// Only send to redirect window (remove from original window)
    #[serde(rename = "redirect_only")]
    RedirectOnly,
    /// Send to both original window and redirect window (duplicate)
    #[serde(rename = "redirect_copy")]
    RedirectCopy,
}

impl Default for RedirectMode {
    fn default() -> Self {
        RedirectMode::RedirectOnly
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
    pub color_entire_line: bool, // If true, apply colors to entire line, not just matched text
    #[serde(default, skip_serializing_if = "is_false")]
    pub fast_parse: bool, // If true, split pattern on | and use Aho-Corasick for literal matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>, // Sound file to play when pattern matches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_volume: Option<f32>, // Volume override for this sound (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>, // Category for grouping highlights (e.g., "Combat", "Healing", "Death")
    #[serde(default, skip_serializing_if = "is_false")]
    pub squelch: bool, // If true, completely hide lines matching this pattern (ignore/filter)
    #[serde(default, skip_serializing_if = "is_false")]
    pub silent_prompt: bool, // If true, lines matching don't trigger prompt display (prompt suppressed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_to: Option<String>, // Window name to redirect matching lines to
    #[serde(default)]
    pub redirect_mode: RedirectMode, // How to handle redirect: only or copy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>, // If set, replace matched text with this string (supports $1, $2 capture groups)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>, // If set, only apply this highlight to lines from this stream (e.g., "death", "thoughts")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>, // If set with replace, only apply replacement in this window (colors apply everywhere)

    // Performance optimization: cache compiled regex (not serialized)
    #[serde(skip)]
    pub compiled_regex: Option<regex::Regex>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPattern {
    pub pattern: String,     // Regex pattern to match
    pub event_type: String,  // Event type: "stun", "webbed", "prone", etc.
    pub action: EventAction, // Action to perform: set/clear/increment
    #[serde(default)]
    pub duration: u32, // Duration in seconds (0 = don't change)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_capture: Option<usize>, // Regex capture group for duration (1-based)
    #[serde(default = "default_duration_multiplier")]
    pub duration_multiplier: f32, // Multiply captured duration (e.g., 5.0 for rounds->seconds)
    #[serde(default = "default_enabled")]
    pub enabled: bool, // Can disable without deleting
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum EventAction {
    #[default]
    Set,       // Set state/timer (e.g., start stun countdown)
    Clear,     // Clear state/timer (e.g., recover from stun)
    Increment, // Add to existing value (future use)
}

fn default_duration_multiplier() -> f32 {
    1.0
}
fn default_enabled() -> bool {
    true
}

fn is_false(b: &bool) -> bool {
    !b
}

impl Config {
    /// Load common (global) highlights that apply to all characters
    /// Returns: HashMap of global highlights, or empty if file doesn't exist
    pub fn load_common_highlights() -> Result<HashMap<String, HighlightPattern>> {
        let path = Self::common_highlights_path()?;

        if !path.exists() {
            return Ok(HashMap::new());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common highlights: {:?}", path))?;

        let highlights: HashMap<String, HighlightPattern> = toml::from_str(&contents)
            .context("Failed to parse common highlights TOML")?;

        Ok(highlights)
    }

    /// Load highlights for a character, merging global + character-specific
    /// Character-specific highlights override global ones with the same name
    pub fn load_highlights(character: Option<&str>) -> Result<HashMap<String, HighlightPattern>> {
        // Start with global/common highlights
        let mut highlights = Self::load_common_highlights()?;

        // Load character-specific highlights
        let highlights_path = Self::highlights_path(character)?;

        if highlights_path.exists() {
            let contents =
                fs::read_to_string(&highlights_path).context("Failed to read highlights.toml")?;
            let character_highlights: HashMap<String, HighlightPattern> =
                toml::from_str(&contents).context("Failed to parse highlights.toml")?;

            // Character highlights override global (HashMap::extend)
            highlights.extend(character_highlights);
        } else if highlights.is_empty() {
            // No global and no character highlights - use embedded defaults
            highlights = toml::from_str(DEFAULT_HIGHLIGHTS).unwrap_or_default();
        }

        // Compile all regex patterns for performance
        Self::compile_highlight_patterns(&mut highlights);

        Ok(highlights)
    }

    /// Compile regex patterns for all highlights (performance optimization)
    pub fn compile_highlight_patterns(highlights: &mut HashMap<String, HighlightPattern>) {
        for (name, pattern) in highlights.iter_mut() {
            if !pattern.fast_parse {
                // Only compile regex for non-fast_parse patterns
                match regex::Regex::new(&pattern.pattern) {
                    Ok(regex) => {
                        pattern.compiled_regex = Some(regex);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to compile regex for highlight '{}': {}", name, e);
                        pattern.compiled_regex = None;
                    }
                }
            }
        }
    }

    /// Save highlights to highlights.toml for a character
    pub(crate) fn save_highlights(&self, character: Option<&str>) -> Result<()> {
        let highlights_path = Self::highlights_path(character)?;
        let contents =
            toml::to_string_pretty(&self.highlights).context("Failed to serialize highlights")?;
        fs::write(&highlights_path, contents).context("Failed to write highlights.toml")?;
        Ok(())
    }

    /// Save a highlight to common (global) highlights file
    /// This makes the highlight available to all characters
    pub fn save_common_highlight(name: &str, pattern: &HighlightPattern) -> Result<()> {
        // Ensure global directory exists
        let global_dir = Self::global_dir()?;
        fs::create_dir_all(&global_dir)
            .with_context(|| format!("Failed to create global directory: {:?}", global_dir))?;

        // Load existing common highlights
        let mut highlights = Self::load_common_highlights()?;

        // Add or update the pattern
        highlights.insert(name.to_string(), pattern.clone());

        // Write back to file
        let path = Self::common_highlights_path()?;
        let toml = toml::to_string_pretty(&highlights)
            .context("Failed to serialize common highlights")?;

        fs::write(&path, toml)
            .with_context(|| format!("Failed to write common highlights: {:?}", path))?;

        Ok(())
    }

    /// Delete a highlight from common (global) highlights file
    pub fn delete_common_highlight(name: &str) -> Result<()> {
        let mut highlights = Self::load_common_highlights()?;
        highlights.remove(name);

        let path = Self::common_highlights_path()?;
        let toml = toml::to_string_pretty(&highlights)
            .context("Failed to serialize common highlights")?;

        fs::write(&path, toml)
            .with_context(|| format!("Failed to write common highlights: {:?}", path))?;

        Ok(())
    }

    /// Load ONLY character-specific highlights (no merge with global)
    /// Used for source tracking in UI to distinguish [G] vs [C] highlights
    pub fn load_character_highlights_only(
        character: Option<&str>,
    ) -> Result<HashMap<String, HighlightPattern>> {
        let highlights_path = Self::highlights_path(character)?;

        if !highlights_path.exists() {
            return Ok(HashMap::new());
        }

        let contents =
            fs::read_to_string(&highlights_path).context("Failed to read highlights.toml")?;
        let highlights: HashMap<String, HighlightPattern> =
            toml::from_str(&contents).context("Failed to parse highlights.toml")?;

        Ok(highlights)
    }

    /// Save a single highlight to the appropriate file based on scope
    /// is_global = true: saves to global/highlights.toml
    /// is_global = false: saves to profiles/{char}/highlights.toml
    pub fn save_single_highlight(
        name: &str,
        pattern: &HighlightPattern,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        if is_global {
            Self::save_common_highlight(name, pattern)
        } else {
            Self::save_character_highlight(name, pattern, character)
        }
    }

    /// Save a single highlight to character-specific highlights file
    fn save_character_highlight(
        name: &str,
        pattern: &HighlightPattern,
        character: Option<&str>,
    ) -> Result<()> {
        let highlights_path = Self::highlights_path(character)?;

        // Ensure parent directory exists
        if let Some(parent) = highlights_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        // Load existing character highlights
        let mut highlights = Self::load_character_highlights_only(character)?;

        // Add or update the pattern
        highlights.insert(name.to_string(), pattern.clone());

        // Write back to file
        let toml = toml::to_string_pretty(&highlights)
            .context("Failed to serialize character highlights")?;

        fs::write(&highlights_path, toml)
            .with_context(|| format!("Failed to write highlights: {:?}", highlights_path))?;

        tracing::info!(
            "Saved highlight '{}' to character highlights file: {:?}",
            name,
            highlights_path
        );

        Ok(())
    }

    /// Delete a single highlight from the appropriate file based on scope
    /// is_global = true: deletes from global/highlights.toml
    /// is_global = false: deletes from profiles/{char}/highlights.toml
    pub fn delete_single_highlight(
        name: &str,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        if is_global {
            Self::delete_common_highlight(name)
        } else {
            Self::delete_character_highlight(name, character)
        }
    }

    /// Delete a single highlight from character-specific highlights file
    fn delete_character_highlight(name: &str, character: Option<&str>) -> Result<()> {
        let highlights_path = Self::highlights_path(character)?;

        if !highlights_path.exists() {
            tracing::warn!(
                "Cannot delete highlight '{}' - file does not exist: {:?}",
                name,
                highlights_path
            );
            return Ok(());
        }

        let mut highlights = Self::load_character_highlights_only(character)?;

        if highlights.remove(name).is_some() {
            let toml = toml::to_string_pretty(&highlights)
                .context("Failed to serialize character highlights")?;

            fs::write(&highlights_path, toml)
                .with_context(|| format!("Failed to write highlights: {:?}", highlights_path))?;

            tracing::info!(
                "Deleted highlight '{}' from character highlights file: {:?}",
                name,
                highlights_path
            );
        }

        Ok(())
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
        let contents =
            toml::to_string_pretty(&self.highlights).context("Failed to serialize highlights")?;
        fs::write(&highlights_path, contents).context("Failed to write highlights profile")?;

        Ok(highlights_path)
    }

    /// Load highlights from a named profile
    pub fn load_highlights_from(name: &str) -> Result<HashMap<String, HighlightPattern>> {
        let highlights_dir = Self::highlights_dir()?;
        let highlights_path = highlights_dir.join(format!("{}.toml", name));

        if !highlights_path.exists() {
            return Err(anyhow::anyhow!("Highlight profile '{}' not found", name));
        }

        let contents =
            fs::read_to_string(&highlights_path).context("Failed to read highlights profile")?;
        let highlights: HashMap<String, HighlightPattern> =
            toml::from_str(&contents).context("Failed to parse highlights profile")?;

        Ok(highlights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // RedirectMode tests
    // ===========================================

    #[test]
    fn test_redirect_mode_default() {
        let mode = RedirectMode::default();
        assert_eq!(mode, RedirectMode::RedirectOnly);
    }

    #[test]
    fn test_redirect_mode_equality() {
        assert_eq!(RedirectMode::RedirectOnly, RedirectMode::RedirectOnly);
        assert_eq!(RedirectMode::RedirectCopy, RedirectMode::RedirectCopy);
        assert_ne!(RedirectMode::RedirectOnly, RedirectMode::RedirectCopy);
    }

    #[test]
    fn test_redirect_mode_clone() {
        let mode = RedirectMode::RedirectCopy;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    // ===========================================
    // HighlightPattern tests
    // ===========================================

    #[test]
    fn test_highlight_pattern_basic() {
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        };

        assert_eq!(pattern.pattern, "test");
        assert_eq!(pattern.fg, Some("#FF0000".to_string()));
        assert!(pattern.bg.is_none());
        assert!(!pattern.bold);
    }

    #[test]
    fn test_highlight_pattern_with_all_options() {
        let pattern = HighlightPattern {
            pattern: r"\d+ damage".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: Some("#330000".to_string()),
            bold: true,
            color_entire_line: true,
            fast_parse: false,
            sound: Some("damage.wav".to_string()),
            sound_volume: Some(0.8),
            category: Some("Combat".to_string()),
            squelch: false,
            silent_prompt: false,
            redirect_to: Some("combat".to_string()),
            redirect_mode: RedirectMode::RedirectCopy,
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        };

        assert!(pattern.bold);
        assert!(pattern.color_entire_line);
        assert_eq!(pattern.sound, Some("damage.wav".to_string()));
        assert_eq!(pattern.sound_volume, Some(0.8));
        assert_eq!(pattern.category, Some("Combat".to_string()));
        assert_eq!(pattern.redirect_to, Some("combat".to_string()));
        assert_eq!(pattern.redirect_mode, RedirectMode::RedirectCopy);
    }

    #[test]
    fn test_highlight_pattern_squelch() {
        let pattern = HighlightPattern {
            pattern: "spam message".to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            category: Some("Ignore".to_string()),
            squelch: true,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        };

        assert!(pattern.squelch);
    }

    #[test]
    fn test_highlight_pattern_fast_parse() {
        let pattern = HighlightPattern {
            pattern: "word1|word2|word3".to_string(),
            fg: Some("#00FF00".to_string()),
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: true, // Uses Aho-Corasick
            sound: None,
            sound_volume: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        };

        assert!(pattern.fast_parse);
    }

    #[test]
    fn test_highlight_pattern_clone() {
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            category: Some("Test".to_string()),
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        };

        let cloned = pattern.clone();
        assert_eq!(cloned.pattern, pattern.pattern);
        assert_eq!(cloned.fg, pattern.fg);
        assert_eq!(cloned.bold, pattern.bold);
        assert_eq!(cloned.category, pattern.category);
    }

    // ===========================================
    // EventAction tests
    // ===========================================

    #[test]
    fn test_event_action_default() {
        let action = EventAction::default();
        assert!(matches!(action, EventAction::Set));
    }

    #[test]
    fn test_event_action_variants() {
        let set = EventAction::Set;
        let clear = EventAction::Clear;
        let increment = EventAction::Increment;

        assert!(matches!(set, EventAction::Set));
        assert!(matches!(clear, EventAction::Clear));
        assert!(matches!(increment, EventAction::Increment));
    }

    #[test]
    fn test_event_action_clone() {
        let action = EventAction::Clear;
        let cloned = action.clone();
        assert!(matches!(cloned, EventAction::Clear));
    }

    // ===========================================
    // EventPattern tests
    // ===========================================

    #[test]
    fn test_event_pattern_basic() {
        let pattern = EventPattern {
            pattern: r"You are stunned for (\d+) seconds".to_string(),
            event_type: "stun".to_string(),
            action: EventAction::Set,
            duration: 0,
            duration_capture: Some(1),
            duration_multiplier: 1.0,
            enabled: true,
        };

        assert_eq!(pattern.event_type, "stun");
        assert!(matches!(pattern.action, EventAction::Set));
        assert_eq!(pattern.duration_capture, Some(1));
        assert!(pattern.enabled);
    }

    #[test]
    fn test_event_pattern_with_fixed_duration() {
        let pattern = EventPattern {
            pattern: "You fall prone".to_string(),
            event_type: "prone".to_string(),
            action: EventAction::Set,
            duration: 3,
            duration_capture: None,
            duration_multiplier: 1.0,
            enabled: true,
        };

        assert_eq!(pattern.duration, 3);
        assert!(pattern.duration_capture.is_none());
    }

    #[test]
    fn test_event_pattern_with_multiplier() {
        let pattern = EventPattern {
            pattern: r"Webbed for (\d+) rounds".to_string(),
            event_type: "webbed".to_string(),
            action: EventAction::Set,
            duration: 0,
            duration_capture: Some(1),
            duration_multiplier: 5.0, // Convert rounds to seconds
            enabled: true,
        };

        assert_eq!(pattern.duration_multiplier, 5.0);
    }

    #[test]
    fn test_event_pattern_disabled() {
        let pattern = EventPattern {
            pattern: "test".to_string(),
            event_type: "test".to_string(),
            action: EventAction::Set,
            duration: 0,
            duration_capture: None,
            duration_multiplier: 1.0,
            enabled: false,
        };

        assert!(!pattern.enabled);
    }

    #[test]
    fn test_event_pattern_clear_action() {
        let pattern = EventPattern {
            pattern: "You recover from the stun".to_string(),
            event_type: "stun".to_string(),
            action: EventAction::Clear,
            duration: 0,
            duration_capture: None,
            duration_multiplier: 1.0,
            enabled: true,
        };

        assert!(matches!(pattern.action, EventAction::Clear));
    }

    // ===========================================
    // Helper function tests
    // ===========================================

    #[test]
    fn test_is_false_helper() {
        assert!(is_false(&false));
        assert!(!is_false(&true));
    }

    #[test]
    fn test_default_duration_multiplier() {
        assert_eq!(default_duration_multiplier(), 1.0);
    }

    #[test]
    fn test_default_enabled() {
        assert!(default_enabled());
    }

    // ===========================================
    // Serialization tests (via TOML)
    // ===========================================

    #[test]
    fn test_highlight_pattern_serialization() {
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        };

        let toml_str = toml::to_string(&pattern).unwrap();
        assert!(toml_str.contains("pattern = \"test\""));
        assert!(toml_str.contains("fg = \"#FF0000\""));
        assert!(toml_str.contains("bold = true"));
        // Options with false should be skipped (skip_serializing_if = "is_false")
    }

    #[test]
    fn test_highlight_pattern_deserialization() {
        let toml_str = r##"
            pattern = "damage"
            fg = "#FF0000"
            bold = true
        "##;

        let pattern: HighlightPattern = toml::from_str(toml_str).unwrap();
        assert_eq!(pattern.pattern, "damage");
        assert_eq!(pattern.fg, Some("#FF0000".to_string()));
        assert!(pattern.bold);
        assert!(!pattern.squelch); // Default
        assert!(!pattern.fast_parse); // Default
    }

    #[test]
    fn test_redirect_mode_serialization() {
        // TOML can't serialize bare enums - test it as part of a HighlightPattern
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: Some("combat".to_string()),
            redirect_mode: RedirectMode::RedirectCopy,
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        };
        let toml_str = toml::to_string(&pattern).unwrap();
        assert!(toml_str.contains("redirect_mode = \"redirect_copy\""));
    }

    #[test]
    fn test_event_pattern_serialization() {
        let pattern = EventPattern {
            pattern: "stunned".to_string(),
            event_type: "stun".to_string(),
            action: EventAction::Set,
            duration: 5,
            duration_capture: None,
            duration_multiplier: 1.0,
            enabled: true,
        };

        let toml_str = toml::to_string(&pattern).unwrap();
        assert!(toml_str.contains("pattern = \"stunned\""));
        assert!(toml_str.contains("event_type = \"stun\""));
    }

    // ===========================================
    // Debug trait tests
    // ===========================================

    #[test]
    fn test_redirect_mode_debug() {
        let mode = RedirectMode::RedirectOnly;
        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("RedirectOnly"));
    }

    #[test]
    fn test_highlight_pattern_debug() {
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        };

        let debug_str = format!("{:?}", pattern);
        assert!(debug_str.contains("HighlightPattern"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_event_action_debug() {
        let action = EventAction::Increment;
        let debug_str = format!("{:?}", action);
        assert!(debug_str.contains("Increment"));
    }
}
