//! Frontend-agnostic widget data structures.
//!
//! These structs contain only the calculated/prepared data needed to render widgets,
//! with no rendering logic or frontend-specific types (like ratatui's Buffer/Rect).
//!
//! The data preparation logic is shared across all frontends (TUI, GUI),
//! while each frontend has its own renderer that consumes this data.

/// Border configuration for widgets
#[derive(Debug, Clone, PartialEq)]
pub struct BorderConfig {
    pub show_border: bool,
    pub border_style: Option<String>,
    pub border_color: Option<String>,
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            show_border: false,
            border_style: None,
            border_color: None,
        }
    }
}

/// Color configuration (hex strings like "#FF0000")
#[derive(Debug, Clone, PartialEq)]
pub struct ColorConfig {
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub transparent_background: bool,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            foreground: None,
            background: None,
            transparent_background: false,
        }
    }
}

/// Data for a progress bar widget
#[derive(Debug, Clone)]
pub struct ProgressBarData {
    pub label: String,
    pub current: u32,
    pub max: u32,
    pub custom_text: Option<String>,
    pub percentage: u32,
    pub display_text: String,
    pub border: BorderConfig,
    pub bar_fill_color: Option<String>,
    pub bar_background_color: Option<String>,
    pub text_color: Option<String>,
    pub window_background_color: Option<String>,
    pub transparent_background: bool,
}

impl ProgressBarData {
    /// Create progress bar data from raw values
    pub fn new(label: String, current: u32, max: u32, custom_text: Option<String>) -> Self {
        let percentage = if max > 0 {
            ((current as f64 / max as f64) * 100.0) as u32
        } else {
            0
        };

        let display_text = custom_text
            .clone()
            .unwrap_or_else(|| format!("{}/{}", current, max));

        Self {
            label,
            current,
            max,
            custom_text,
            percentage,
            display_text,
            border: BorderConfig::default(),
            bar_fill_color: Some("#00ff00".to_string()), // Green default
            bar_background_color: None,
            text_color: Some("#ffffff".to_string()), // White default
            window_background_color: None,
            transparent_background: false,
        }
    }

    pub fn with_border(mut self, border: BorderConfig) -> Self {
        self.border = border;
        self
    }

    pub fn with_colors(
        mut self,
        bar_fill: Option<String>,
        bar_background: Option<String>,
    ) -> Self {
        self.bar_fill_color = bar_fill;
        self.bar_background_color = bar_background;
        self
    }
}

/// Data for an indicator widget (boolean on/off display)
#[derive(Debug, Clone)]
pub struct IndicatorData {
    pub label: String,
    pub active: bool,
    pub border: BorderConfig,
    pub off_color: String,
    pub on_color: String,
    pub background_color: Option<String>,
    pub transparent_background: bool,
}

impl IndicatorData {
    pub fn new(label: String, active: bool) -> Self {
        Self {
            label,
            active,
            border: BorderConfig::default(),
            off_color: "#555555".to_string(), // Dark gray
            on_color: "#00ff00".to_string(),   // Green
            background_color: None,
            transparent_background: false,
        }
    }

    pub fn with_colors(mut self, off_color: String, on_color: String) -> Self {
        self.off_color = off_color;
        self.on_color = on_color;
        self
    }
}

/// Data for a countdown timer widget
#[derive(Debug, Clone)]
pub struct CountdownData {
    pub label: String,
    pub remaining_seconds: u32,
    pub display_text: String,
    pub blocks_to_show: u32,
    pub icon: char,
    pub border: BorderConfig,
    pub text_color: Option<String>,
    pub background_color: Option<String>,
    pub transparent_background: bool,
}

impl CountdownData {
    /// Create countdown data from remaining seconds and available width
    pub fn new(
        label: String,
        remaining_seconds: u32,
        available_width: u16,
        icon: char,
    ) -> Self {
        // Right-align the number so it doesn't shift when going from 10->9
        // Format: " 9 " or "10 " (always 3 chars)
        let display_text = format!("{:>2} ", remaining_seconds);
        let text_width = 3u16; // Always 3 chars

        // Calculate max blocks based on available space after the number
        let max_blocks = if available_width > text_width {
            (available_width - text_width) as u32
        } else {
            0
        };
        let blocks_to_show = remaining_seconds.min(max_blocks);

        Self {
            label,
            remaining_seconds,
            display_text,
            blocks_to_show,
            icon,
            border: BorderConfig::default(),
            text_color: Some("#ffffff".to_string()), // White default
            background_color: None,
            transparent_background: false,
        }
    }

    pub fn with_border(mut self, border: BorderConfig) -> Self {
        self.border = border;
        self
    }
}

/// Hand type for hand widget
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandType {
    Left,
    Right,
    Spell,
}

/// Data for a hand widget (displays item in left/right/spell hand)
#[derive(Debug, Clone)]
pub struct HandData {
    pub label: String,
    pub hand_type: HandType,
    pub content: String,
    pub icon: String,
    pub border: BorderConfig,
    pub border_sides: crate::config::BorderSides,
    pub text_color: Option<String>,
    pub content_highlight_color: Option<String>,
    pub background_color: Option<String>,
    pub transparent_background: bool,
}

impl HandData {
    pub fn new(label: String, hand_type: HandType, content: String) -> Self {
        let default_icon = match hand_type {
            HandType::Left => "L:",
            HandType::Right => "R:",
            HandType::Spell => "S:",
        };

        // Truncate content to 24 characters
        let truncated_content = if content.chars().count() > 24 {
            content.chars().take(24).collect()
        } else {
            content
        };

        Self {
            label,
            hand_type,
            content: truncated_content,
            icon: default_icon.to_string(),
            border: BorderConfig::default(),
            border_sides: crate::config::BorderSides::default(),
            text_color: None,
            content_highlight_color: None,
            background_color: None,
            transparent_background: false,
        }
    }

    pub fn with_icon(mut self, icon: String) -> Self {
        self.icon = icon;
        self
    }

    pub fn with_colors(
        mut self,
        text_color: Option<String>,
        content_highlight: Option<String>,
    ) -> Self {
        self.text_color = text_color;
        self.content_highlight_color = content_highlight;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // BorderConfig tests
    // ===========================================

    #[test]
    fn test_border_config_default() {
        let config = BorderConfig::default();
        assert!(!config.show_border);
        assert!(config.border_style.is_none());
        assert!(config.border_color.is_none());
    }

    #[test]
    fn test_border_config_with_values() {
        let config = BorderConfig {
            show_border: true,
            border_style: Some("rounded".to_string()),
            border_color: Some("#FF0000".to_string()),
        };
        assert!(config.show_border);
        assert_eq!(config.border_style, Some("rounded".to_string()));
        assert_eq!(config.border_color, Some("#FF0000".to_string()));
    }

    #[test]
    fn test_border_config_equality() {
        let config1 = BorderConfig::default();
        let config2 = BorderConfig::default();
        assert_eq!(config1, config2);
    }

    #[test]
    fn test_border_config_clone() {
        let config = BorderConfig {
            show_border: true,
            border_style: Some("double".to_string()),
            border_color: None,
        };
        let cloned = config.clone();
        assert_eq!(config.show_border, cloned.show_border);
        assert_eq!(config.border_style, cloned.border_style);
    }

    // ===========================================
    // ColorConfig tests
    // ===========================================

    #[test]
    fn test_color_config_default() {
        let config = ColorConfig::default();
        assert!(config.foreground.is_none());
        assert!(config.background.is_none());
        assert!(!config.transparent_background);
    }

    #[test]
    fn test_color_config_with_values() {
        let config = ColorConfig {
            foreground: Some("#FFFFFF".to_string()),
            background: Some("#000000".to_string()),
            transparent_background: false,
        };
        assert_eq!(config.foreground, Some("#FFFFFF".to_string()));
        assert_eq!(config.background, Some("#000000".to_string()));
        assert!(!config.transparent_background);
    }

    #[test]
    fn test_color_config_equality() {
        let config1 = ColorConfig::default();
        let config2 = ColorConfig::default();
        assert_eq!(config1, config2);
    }

    // ===========================================
    // ProgressBarData tests
    // ===========================================

    #[test]
    fn test_progress_bar_new() {
        let data = ProgressBarData::new("Health".to_string(), 75, 100, None);
        assert_eq!(data.label, "Health");
        assert_eq!(data.current, 75);
        assert_eq!(data.max, 100);
        assert_eq!(data.percentage, 75);
        assert_eq!(data.display_text, "75/100");
    }

    #[test]
    fn test_progress_bar_percentage_calculation() {
        let data = ProgressBarData::new("Test".to_string(), 50, 200, None);
        assert_eq!(data.percentage, 25); // 50/200 = 25%
    }

    #[test]
    fn test_progress_bar_percentage_zero_max() {
        let data = ProgressBarData::new("Test".to_string(), 50, 0, None);
        assert_eq!(data.percentage, 0); // Avoid division by zero
    }

    #[test]
    fn test_progress_bar_percentage_full() {
        let data = ProgressBarData::new("Test".to_string(), 100, 100, None);
        assert_eq!(data.percentage, 100);
    }

    #[test]
    fn test_progress_bar_percentage_over() {
        let data = ProgressBarData::new("Test".to_string(), 150, 100, None);
        assert_eq!(data.percentage, 150); // Can exceed 100%
    }

    #[test]
    fn test_progress_bar_custom_text() {
        let data = ProgressBarData::new("Health".to_string(), 75, 100, Some("HP: 75%".to_string()));
        assert_eq!(data.display_text, "HP: 75%");
        assert_eq!(data.custom_text, Some("HP: 75%".to_string()));
    }

    #[test]
    fn test_progress_bar_default_colors() {
        let data = ProgressBarData::new("Test".to_string(), 50, 100, None);
        assert_eq!(data.bar_fill_color, Some("#00ff00".to_string())); // Green
        assert_eq!(data.text_color, Some("#ffffff".to_string())); // White
        assert!(data.bar_background_color.is_none());
    }

    #[test]
    fn test_progress_bar_with_border() {
        let border = BorderConfig {
            show_border: true,
            border_style: Some("rounded".to_string()),
            border_color: None,
        };
        let data = ProgressBarData::new("Test".to_string(), 50, 100, None).with_border(border);
        assert!(data.border.show_border);
    }

    #[test]
    fn test_progress_bar_with_colors() {
        let data = ProgressBarData::new("Test".to_string(), 50, 100, None)
            .with_colors(Some("#FF0000".to_string()), Some("#333333".to_string()));
        assert_eq!(data.bar_fill_color, Some("#FF0000".to_string()));
        assert_eq!(data.bar_background_color, Some("#333333".to_string()));
    }

    #[test]
    fn test_progress_bar_transparent_background() {
        let data = ProgressBarData::new("Test".to_string(), 50, 100, None);
        assert!(!data.transparent_background);
    }

    // ===========================================
    // IndicatorData tests
    // ===========================================

    #[test]
    fn test_indicator_new_inactive() {
        let data = IndicatorData::new("Stun".to_string(), false);
        assert_eq!(data.label, "Stun");
        assert!(!data.active);
    }

    #[test]
    fn test_indicator_new_active() {
        let data = IndicatorData::new("Poison".to_string(), true);
        assert_eq!(data.label, "Poison");
        assert!(data.active);
    }

    #[test]
    fn test_indicator_default_colors() {
        let data = IndicatorData::new("Test".to_string(), false);
        assert_eq!(data.off_color, "#555555"); // Dark gray
        assert_eq!(data.on_color, "#00ff00");   // Green
    }

    #[test]
    fn test_indicator_with_colors() {
        let data = IndicatorData::new("Test".to_string(), true)
            .with_colors("#333333".to_string(), "#FF0000".to_string());
        assert_eq!(data.off_color, "#333333");
        assert_eq!(data.on_color, "#FF0000");
    }

    #[test]
    fn test_indicator_transparent_background() {
        let data = IndicatorData::new("Test".to_string(), false);
        assert!(!data.transparent_background);
        assert!(data.background_color.is_none());
    }

    #[test]
    fn test_indicator_default_border() {
        let data = IndicatorData::new("Test".to_string(), false);
        assert!(!data.border.show_border);
    }

    // ===========================================
    // CountdownData tests
    // ===========================================

    #[test]
    fn test_countdown_new() {
        let data = CountdownData::new("RT".to_string(), 5, 20, '█');
        assert_eq!(data.label, "RT");
        assert_eq!(data.remaining_seconds, 5);
        assert_eq!(data.icon, '█');
    }

    #[test]
    fn test_countdown_display_text_format() {
        let data = CountdownData::new("RT".to_string(), 5, 20, '█');
        // Format is right-aligned with space: " 5 "
        assert_eq!(data.display_text, " 5 ");
    }

    #[test]
    fn test_countdown_display_text_double_digit() {
        let data = CountdownData::new("RT".to_string(), 12, 20, '█');
        assert_eq!(data.display_text, "12 ");
    }

    #[test]
    fn test_countdown_blocks_calculation() {
        let data = CountdownData::new("RT".to_string(), 5, 20, '█');
        // Available width = 20, text_width = 3, max_blocks = 17
        // blocks_to_show = min(5, 17) = 5
        assert_eq!(data.blocks_to_show, 5);
    }

    #[test]
    fn test_countdown_blocks_limited_by_width() {
        let data = CountdownData::new("RT".to_string(), 100, 10, '█');
        // Available width = 10, text_width = 3, max_blocks = 7
        // blocks_to_show = min(100, 7) = 7
        assert_eq!(data.blocks_to_show, 7);
    }

    #[test]
    fn test_countdown_blocks_zero_width() {
        let data = CountdownData::new("RT".to_string(), 5, 2, '█');
        // Available width = 2, text_width = 3, so no room for blocks
        assert_eq!(data.blocks_to_show, 0);
    }

    #[test]
    fn test_countdown_with_border() {
        let border = BorderConfig {
            show_border: true,
            border_style: None,
            border_color: Some("#FFFFFF".to_string()),
        };
        let data = CountdownData::new("RT".to_string(), 5, 20, '█').with_border(border);
        assert!(data.border.show_border);
        assert_eq!(data.border.border_color, Some("#FFFFFF".to_string()));
    }

    #[test]
    fn test_countdown_default_colors() {
        let data = CountdownData::new("RT".to_string(), 5, 20, '█');
        assert_eq!(data.text_color, Some("#ffffff".to_string()));
        assert!(data.background_color.is_none());
        assert!(!data.transparent_background);
    }

    // ===========================================
    // HandType tests
    // ===========================================

    #[test]
    fn test_hand_type_variants() {
        assert_eq!(HandType::Left, HandType::Left);
        assert_eq!(HandType::Right, HandType::Right);
        assert_eq!(HandType::Spell, HandType::Spell);
    }

    #[test]
    fn test_hand_type_inequality() {
        assert_ne!(HandType::Left, HandType::Right);
        assert_ne!(HandType::Left, HandType::Spell);
        assert_ne!(HandType::Right, HandType::Spell);
    }

    #[test]
    fn test_hand_type_clone() {
        let hand = HandType::Left;
        let cloned = hand;
        assert_eq!(hand, cloned);
    }

    // ===========================================
    // HandData tests
    // ===========================================

    #[test]
    fn test_hand_data_left() {
        let data = HandData::new("Left Hand".to_string(), HandType::Left, "sword".to_string());
        assert_eq!(data.label, "Left Hand");
        assert_eq!(data.hand_type, HandType::Left);
        assert_eq!(data.content, "sword");
        assert_eq!(data.icon, "L:");
    }

    #[test]
    fn test_hand_data_right() {
        let data = HandData::new("Right Hand".to_string(), HandType::Right, "shield".to_string());
        assert_eq!(data.hand_type, HandType::Right);
        assert_eq!(data.icon, "R:");
    }

    #[test]
    fn test_hand_data_spell() {
        let data = HandData::new("Spell".to_string(), HandType::Spell, "fireball".to_string());
        assert_eq!(data.hand_type, HandType::Spell);
        assert_eq!(data.icon, "S:");
    }

    #[test]
    fn test_hand_data_truncates_long_content() {
        let long_content = "a".repeat(50);
        let data = HandData::new("Hand".to_string(), HandType::Left, long_content);
        assert_eq!(data.content.chars().count(), 24);
    }

    #[test]
    fn test_hand_data_short_content_unchanged() {
        let short_content = "short item";
        let data = HandData::new("Hand".to_string(), HandType::Left, short_content.to_string());
        assert_eq!(data.content, "short item");
    }

    #[test]
    fn test_hand_data_with_icon() {
        let data = HandData::new("Hand".to_string(), HandType::Left, "item".to_string())
            .with_icon("🗡:".to_string());
        assert_eq!(data.icon, "🗡:");
    }

    #[test]
    fn test_hand_data_with_colors() {
        let data = HandData::new("Hand".to_string(), HandType::Left, "item".to_string())
            .with_colors(Some("#FFFFFF".to_string()), Some("#00FF00".to_string()));
        assert_eq!(data.text_color, Some("#FFFFFF".to_string()));
        assert_eq!(data.content_highlight_color, Some("#00FF00".to_string()));
    }

    #[test]
    fn test_hand_data_default_colors() {
        let data = HandData::new("Hand".to_string(), HandType::Left, "item".to_string());
        assert!(data.text_color.is_none());
        assert!(data.content_highlight_color.is_none());
        assert!(data.background_color.is_none());
        assert!(!data.transparent_background);
    }

    #[test]
    fn test_hand_data_default_border() {
        let data = HandData::new("Hand".to_string(), HandType::Left, "item".to_string());
        assert!(!data.border.show_border);
    }

    // ===========================================
    // Clone and Debug tests
    // ===========================================

    #[test]
    fn test_progress_bar_clone() {
        let data = ProgressBarData::new("Health".to_string(), 75, 100, None);
        let cloned = data.clone();
        assert_eq!(cloned.label, data.label);
        assert_eq!(cloned.current, data.current);
        assert_eq!(cloned.percentage, data.percentage);
    }

    #[test]
    fn test_indicator_clone() {
        let data = IndicatorData::new("Test".to_string(), true);
        let cloned = data.clone();
        assert_eq!(cloned.label, data.label);
        assert_eq!(cloned.active, data.active);
    }

    #[test]
    fn test_countdown_clone() {
        let data = CountdownData::new("RT".to_string(), 5, 20, '█');
        let cloned = data.clone();
        assert_eq!(cloned.remaining_seconds, data.remaining_seconds);
        assert_eq!(cloned.blocks_to_show, data.blocks_to_show);
    }

    #[test]
    fn test_hand_data_clone() {
        let data = HandData::new("Hand".to_string(), HandType::Left, "item".to_string());
        let cloned = data.clone();
        assert_eq!(cloned.label, data.label);
        assert_eq!(cloned.hand_type, data.hand_type);
    }

    #[test]
    fn test_progress_bar_debug() {
        let data = ProgressBarData::new("Health".to_string(), 75, 100, None);
        let debug_str = format!("{:?}", data);
        assert!(debug_str.contains("ProgressBarData"));
        assert!(debug_str.contains("Health"));
    }

    #[test]
    fn test_indicator_debug() {
        let data = IndicatorData::new("Stun".to_string(), true);
        let debug_str = format!("{:?}", data);
        assert!(debug_str.contains("IndicatorData"));
        assert!(debug_str.contains("Stun"));
    }
}

