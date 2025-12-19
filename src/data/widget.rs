//! Widget data structures - State for all widget types
//!
//! These are pure data structures with NO rendering logic.
//! Frontends read from these to render appropriately.

use std::collections::VecDeque;

/// Styled text content for text-based widgets
#[derive(Clone, Debug)]
pub struct TextContent {
    /// Wrapped lines ready for display
    pub lines: VecDeque<StyledLine>,
    /// Scroll offset from bottom (0 = live view, showing newest)
    pub scroll_offset: usize,
    /// Maximum lines to keep in buffer
    pub max_lines: usize,
    /// Title for the window
    pub title: String,
    /// Generation counter - increments on every add_line call
    /// Used to detect changes even when line count stays constant (at max_lines)
    pub generation: u64,
    /// Stream IDs this window listens to (e.g., ["thoughts"], ["main"], ["combat"])
    /// Used for routing incoming game text to the correct window
    pub streams: Vec<String>,
}

/// A single display line with styled segments
#[derive(Clone, Debug)]
pub struct StyledLine {
    pub segments: Vec<TextSegment>,
    /// The stream this line originated from (e.g., "death", "thoughts", "main")
    /// Used for stream-filtered highlights
    pub stream: String,
}

/// A segment of text with styling
#[derive(Clone, Debug, PartialEq)]
pub struct TextSegment {
    pub text: String,
    pub fg: Option<String>, // Hex color "#RRGGBB"
    pub bg: Option<String>, // Hex color "#RRGGBB"
    pub bold: bool,
    pub span_type: SpanType, // Semantic type for priority layering
    pub link_data: Option<LinkData>,
}

/// Semantic type of text span (for highlight priority)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanType {
    Normal,      // Regular text
    Link,        // <a> tag from parser (clickable game objects)
    Monsterbold, // <preset id="monsterbold"> from parser (monsters)
    Spell,       // <spell> tag from parser (spells)
    Speech,      // <preset id="speech"> from parser (player speech)
    System,      // Client/system messages; skip highlight transforms
}

/// Link metadata for clickable text
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkData {
    pub exist_id: String,
    pub noun: String,
    pub text: String,
    pub coord: Option<String>, // Optional coord for direct commands (e.g., "2524,1864" for movement)
}

/// Progress bar state
#[derive(Clone, Debug)]
pub struct ProgressData {
    pub value: u32,            // Current value (actual value, not percentage)
    pub max: u32,              // Maximum value (actual max, not percentage)
    pub label: String,         // Display label
    pub color: Option<String>, // Hex color override (or custom text like "clear as a bell")
    pub progress_id: String,   // Feed id (XML progressBar id), case-sensitive
}

/// Countdown timer state
#[derive(Clone, Debug)]
pub struct CountdownData {
    pub end_time: i64, // Unix timestamp when timer expires
    pub label: String, // Display label
    pub countdown_id: String, // Feed id (XML event id), case-sensitive
}

/// Compass directions
#[derive(Clone, Debug)]
pub struct CompassData {
    pub directions: Vec<String>, // Available exits: "n", "s", "e", "w", etc.
}

/// Injury doll state
#[derive(Clone, Debug)]
pub struct InjuryDollData {
    pub injuries: std::collections::HashMap<String, u8>, // body_part -> level (0-6)
                                                         // Injury levels: 0=none, 1-3=injury levels, 4-6=scar levels
}

impl InjuryDollData {
    pub fn new() -> Self {
        Self {
            injuries: std::collections::HashMap::new(),
        }
    }

    pub fn set_injury(&mut self, body_part: String, level: u8) {
        self.injuries.insert(body_part, level.min(6));
    }

    pub fn get_injury(&self, body_part: &str) -> u8 {
        self.injuries.get(body_part).copied().unwrap_or(0)
    }

    pub fn clear_all(&mut self) {
        self.injuries.clear();
    }
}

/// Status indicator state
#[derive(Clone, Debug)]
pub struct IndicatorData {
    pub indicator_id: String,  // Feed id, e.g., "kneeling", "hidden"
    pub active: bool,          // Whether indicator is on
    pub color: Option<String>, // Optional color override
}

/// Room description content
#[derive(Clone, Debug)]
pub struct RoomContent {
    pub name: String,
    pub description: Vec<StyledLine>,
    pub exits: Vec<String>,
    pub players: Vec<String>,
    pub objects: Vec<String>,
}

/// Active effect (buff/debuff/cooldown/active spell)
#[derive(Clone, Debug)]
pub struct ActiveEffect {
    pub id: String,   // Unique identifier
    pub text: String, // Display text (e.g., "Fasthr's Reward")
    pub value: u32,   // Progress/percentage (0-100)
    pub time: String, // Time remaining (e.g., "03:06:54")
    pub bar_color: Option<String>,
    pub text_color: Option<String>,
}

/// Active effects content (for buffs, debuffs, cooldowns, active spells)
#[derive(Clone, Debug)]
pub struct ActiveEffectsContent {
    pub category: String, // "Buffs", "Debuffs", "Cooldowns", "ActiveSpells"
    pub effects: Vec<ActiveEffect>,
}

/// Tab definition for tabbed text window
#[derive(Clone, Debug)]
pub struct TabDefinition {
    pub name: String,   // Display name of tab
    pub streams: Vec<String>, // Stream IDs this tab listens to
    pub show_timestamps: bool, // Whether to render timestamps for this tab
    pub ignore_activity: bool, // Skip unread indicators/counts
}

/// Holds the state for a single tab, including its definition and content.
#[derive(Clone, Debug)]
pub struct TabState {
    pub definition: TabDefinition,
    pub content: TextContent,
}

/// Tabbed text window content
#[derive(Clone, Debug)]
pub struct TabbedTextContent {
    pub tabs: Vec<TabState>,
    pub active_tab_index: usize,
}

impl TabbedTextContent {
    pub fn new(
        tabs: Vec<(String, Vec<String>, bool, bool)>,
        max_lines_per_tab: usize,
    ) -> Self {
        let tabs = tabs
            .into_iter()
            .map(|(name, streams, show_timestamps, ignore_activity)| {
                let definition = TabDefinition {
                    name: name.clone(),
                    streams,
                    show_timestamps,
                    ignore_activity,
                };
                let content = TextContent::new(name, max_lines_per_tab);
                TabState { definition, content }
            })
            .collect();
        Self {
            tabs,
            active_tab_index: 0,
        }
    }
}

impl TextContent {
    pub fn new(title: impl Into<String>, max_lines: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max_lines),
            scroll_offset: 0,
            max_lines,
            title: title.into(),
            generation: 0,
            streams: vec![],  // Default to empty - will be set during window creation
        }
    }

    pub fn add_line(&mut self, line: StyledLine) {
        self.lines.push_back(line);
        if self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
        // Increment generation counter on every add_line call
        // This allows frontend to detect changes even when line count stays constant
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn scroll_up(&mut self, amount: usize) {
        let max_scroll = self.lines.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_to_top(&mut self) {
        let max_scroll = self.lines.len().saturating_sub(1);
        self.scroll_offset = max_scroll;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }
}

impl StyledLine {
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            segments: vec![TextSegment {
                text: text.into(),
                fg: None,
                bg: None,
                bold: false,
                span_type: SpanType::Normal,
                link_data: None,
            }],
            stream: String::from("main"),
        }
    }

    /// Create a StyledLine with a specific stream
    pub fn from_text_with_stream(text: impl Into<String>, stream: impl Into<String>) -> Self {
        Self {
            segments: vec![TextSegment {
                text: text.into(),
                fg: None,
                bg: None,
                bold: false,
                span_type: SpanType::Normal,
                link_data: None,
            }],
            stream: stream.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== TextContent Tests ====================

    #[test]
    fn test_text_content_new() {
        let content = TextContent::new("Main", 1000);

        assert_eq!(content.title, "Main");
        assert_eq!(content.max_lines, 1000);
        assert_eq!(content.scroll_offset, 0);
        assert_eq!(content.generation, 0);
        assert!(content.lines.is_empty());
    }

    #[test]
    fn test_text_content_add_line() {
        let mut content = TextContent::new("Test", 100);

        content.add_line(StyledLine::from_text("Hello"));
        assert_eq!(content.lines.len(), 1);
        assert_eq!(content.generation, 1);

        content.add_line(StyledLine::from_text("World"));
        assert_eq!(content.lines.len(), 2);
        assert_eq!(content.generation, 2);
    }

    #[test]
    fn test_text_content_max_lines_limit() {
        let mut content = TextContent::new("Test", 3);

        content.add_line(StyledLine::from_text("Line 1"));
        content.add_line(StyledLine::from_text("Line 2"));
        content.add_line(StyledLine::from_text("Line 3"));
        assert_eq!(content.lines.len(), 3);

        // Adding a 4th line should remove the oldest
        content.add_line(StyledLine::from_text("Line 4"));
        assert_eq!(content.lines.len(), 3);

        // First line should now be "Line 2"
        assert_eq!(content.lines[0].segments[0].text, "Line 2");
        assert_eq!(content.lines[2].segments[0].text, "Line 4");
    }

    #[test]
    fn test_text_content_generation_increments() {
        let mut content = TextContent::new("Test", 5);

        for i in 0..10 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
            assert_eq!(content.generation, (i + 1) as u64);
        }
    }

    #[test]
    fn test_text_content_scroll_up() {
        let mut content = TextContent::new("Test", 100);
        for i in 0..20 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
        }

        assert_eq!(content.scroll_offset, 0);

        content.scroll_up(5);
        assert_eq!(content.scroll_offset, 5);

        content.scroll_up(5);
        assert_eq!(content.scroll_offset, 10);

        // Scroll beyond max should clamp
        content.scroll_up(100);
        assert_eq!(content.scroll_offset, 19); // max is lines.len() - 1
    }

    #[test]
    fn test_text_content_scroll_down() {
        let mut content = TextContent::new("Test", 100);
        for i in 0..20 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
        }

        content.scroll_offset = 15;

        content.scroll_down(5);
        assert_eq!(content.scroll_offset, 10);

        content.scroll_down(5);
        assert_eq!(content.scroll_offset, 5);

        // Scroll below 0 should clamp to 0
        content.scroll_down(100);
        assert_eq!(content.scroll_offset, 0);
    }

    #[test]
    fn test_text_content_scroll_to_top() {
        let mut content = TextContent::new("Test", 100);
        for i in 0..20 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
        }

        content.scroll_to_top();
        assert_eq!(content.scroll_offset, 19); // lines.len() - 1
    }

    #[test]
    fn test_text_content_scroll_to_bottom() {
        let mut content = TextContent::new("Test", 100);
        for i in 0..20 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
        }
        content.scroll_offset = 15;

        content.scroll_to_bottom();
        assert_eq!(content.scroll_offset, 0);
    }

    // ==================== StyledLine Tests ====================

    #[test]
    fn test_styled_line_from_text() {
        let line = StyledLine::from_text("Hello, world!");

        assert_eq!(line.segments.len(), 1);
        assert_eq!(line.segments[0].text, "Hello, world!");
        assert_eq!(line.segments[0].fg, None);
        assert_eq!(line.segments[0].bg, None);
        assert!(!line.segments[0].bold);
        assert_eq!(line.segments[0].span_type, SpanType::Normal);
        assert!(line.segments[0].link_data.is_none());
    }

    // ==================== TextSegment Tests ====================

    #[test]
    fn test_text_segment_with_link() {
        let segment = TextSegment {
            text: "a rusty sword".to_string(),
            fg: Some("#477ab3".to_string()),
            bg: None,
            bold: false,
            span_type: SpanType::Link,
            link_data: Some(LinkData {
                exist_id: "12345".to_string(),
                noun: "sword".to_string(),
                text: "a rusty sword".to_string(),
                coord: None,
            }),
        };

        assert_eq!(segment.span_type, SpanType::Link);
        let link = segment.link_data.as_ref().unwrap();
        assert_eq!(link.exist_id, "12345");
        assert_eq!(link.noun, "sword");
    }

    #[test]
    fn test_text_segment_equality() {
        let seg1 = TextSegment {
            text: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            span_type: SpanType::Monsterbold,
            link_data: None,
        };

        let seg2 = TextSegment {
            text: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            span_type: SpanType::Monsterbold,
            link_data: None,
        };

        let seg3 = TextSegment {
            text: "different".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            span_type: SpanType::Monsterbold,
            link_data: None,
        };

        assert_eq!(seg1, seg2);
        assert_ne!(seg1, seg3);
    }

    // ==================== LinkData Tests ====================

    #[test]
    fn test_link_data_gs4_style() {
        let link = LinkData {
            exist_id: "67890".to_string(),
            noun: "chest".to_string(),
            text: "an iron chest".to_string(),
            coord: Some("1234,5678".to_string()),
        };

        assert_eq!(link.exist_id, "67890");
        assert_eq!(link.noun, "chest");
        assert_eq!(link.text, "an iron chest");
        assert_eq!(link.coord, Some("1234,5678".to_string()));
    }

    #[test]
    fn test_link_data_dr_style() {
        // DragonRealms uses _direct_ marker with cmd in noun
        let link = LinkData {
            exist_id: "_direct_".to_string(),
            noun: "get #8735861 in #8735860 in watery portal".to_string(),
            text: "Some arzumodine cloth".to_string(),
            coord: None,
        };

        assert_eq!(link.exist_id, "_direct_");
        assert!(link.noun.contains("#8735861"));
        assert_eq!(link.coord, None);
    }

    #[test]
    fn test_link_data_equality() {
        let link1 = LinkData {
            exist_id: "123".to_string(),
            noun: "sword".to_string(),
            text: "a sword".to_string(),
            coord: None,
        };

        let link2 = LinkData {
            exist_id: "123".to_string(),
            noun: "sword".to_string(),
            text: "a sword".to_string(),
            coord: None,
        };

        let link3 = LinkData {
            exist_id: "456".to_string(),
            noun: "sword".to_string(),
            text: "a sword".to_string(),
            coord: None,
        };

        assert_eq!(link1, link2);
        assert_ne!(link1, link3);
    }

    // ==================== SpanType Tests ====================

    #[test]
    fn test_span_type_variants() {
        assert_eq!(SpanType::Normal, SpanType::Normal);
        assert_ne!(SpanType::Normal, SpanType::Link);
        assert_ne!(SpanType::Link, SpanType::Monsterbold);
        assert_ne!(SpanType::Monsterbold, SpanType::Spell);
        assert_ne!(SpanType::Spell, SpanType::Speech);
    }

    // ==================== InjuryDollData Tests ====================

    #[test]
    fn test_injury_doll_new() {
        let doll = InjuryDollData::new();
        assert!(doll.injuries.is_empty());
    }

    #[test]
    fn test_injury_doll_set_get() {
        let mut doll = InjuryDollData::new();

        doll.set_injury("head".to_string(), 2);
        assert_eq!(doll.get_injury("head"), 2);

        doll.set_injury("leftArm".to_string(), 5);
        assert_eq!(doll.get_injury("leftArm"), 5);

        // Non-existent body part returns 0
        assert_eq!(doll.get_injury("nonexistent"), 0);
    }

    #[test]
    fn test_injury_doll_level_clamped() {
        let mut doll = InjuryDollData::new();

        // Level should be clamped to max 6
        doll.set_injury("head".to_string(), 10);
        assert_eq!(doll.get_injury("head"), 6);
    }

    #[test]
    fn test_injury_doll_clear_all() {
        let mut doll = InjuryDollData::new();

        doll.set_injury("head".to_string(), 2);
        doll.set_injury("chest".to_string(), 3);
        doll.set_injury("leftArm".to_string(), 1);

        assert_eq!(doll.injuries.len(), 3);

        doll.clear_all();
        assert!(doll.injuries.is_empty());
        assert_eq!(doll.get_injury("head"), 0);
    }

    // ==================== TabbedTextContent Tests ====================

    #[test]
    fn test_tabbed_text_content_new() {
        let tabs = vec![
            ("Main".to_string(), vec!["main".to_string()], false, false),
            (
                "Combat".to_string(),
                vec!["combat".to_string(), "death".to_string()],
                true,
                true,
            ),
        ];

        let content = TabbedTextContent::new(tabs, 1000);

        assert_eq!(content.tabs.len(), 2);
        assert_eq!(content.active_tab_index, 0);

        assert_eq!(content.tabs[0].definition.name, "Main");
        assert_eq!(content.tabs[0].definition.streams, vec!["main"]);
        assert!(!content.tabs[0].definition.show_timestamps);
        assert!(!content.tabs[0].definition.ignore_activity);

        assert_eq!(content.tabs[1].definition.name, "Combat");
        assert_eq!(content.tabs[1].definition.streams, vec!["combat", "death"]);
        assert!(content.tabs[1].definition.show_timestamps);
        assert!(content.tabs[1].definition.ignore_activity);
    }

    // ==================== ProgressData Tests ====================

    #[test]
    fn test_progress_data() {
        let progress = ProgressData {
            value: 75,
            max: 100,
            label: "Health".to_string(),
            color: Some("#00FF00".to_string()),
            progress_id: "health".to_string(),
        };

        assert_eq!(progress.value, 75);
        assert_eq!(progress.max, 100);
        assert_eq!(progress.label, "Health");
        assert_eq!(progress.color, Some("#00FF00".to_string()));
        assert_eq!(progress.progress_id, "health");
    }

    // ==================== CompassData Tests ====================

    #[test]
    fn test_compass_data() {
        let compass = CompassData {
            directions: vec!["n".to_string(), "e".to_string(), "out".to_string()],
        };

        assert_eq!(compass.directions.len(), 3);
        assert!(compass.directions.contains(&"n".to_string()));
        assert!(compass.directions.contains(&"e".to_string()));
        assert!(compass.directions.contains(&"out".to_string()));
    }

    // ==================== ActiveEffect Tests ====================

    #[test]
    fn test_active_effect() {
        let effect = ActiveEffect {
            id: "115".to_string(),
            text: "Fasthr's Reward".to_string(),
            value: 74,
            time: "03:06:54".to_string(),
            bar_color: Some("#00FF00".to_string()),
            text_color: None,
        };

        assert_eq!(effect.id, "115");
        assert_eq!(effect.text, "Fasthr's Reward");
        assert_eq!(effect.value, 74);
        assert_eq!(effect.time, "03:06:54");
    }
}
