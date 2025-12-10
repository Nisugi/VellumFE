//! Window state - Layout and content management
//!
//! Windows are the containers for widgets. They have position, size, and content.

use super::widget::*;

/// Window state - combines layout position with content
#[derive(Clone, Debug)]
pub struct WindowState {
    pub name: String,
    pub widget_type: WidgetType,
    pub content: WindowContent,
    pub position: WindowPosition,
    pub visible: bool,
    pub focused: bool,
    pub content_align: Option<String>,
}

/// Types of widgets that can be displayed
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WidgetType {
    Text,
    TabbedText,
    Progress,
    Countdown,
    Compass,
    Indicator,
    Room,
    Inventory,
    CommandInput,
    Dashboard,
    InjuryDoll,
    Hand,
    ActiveEffects,
    Targets,
    Players,
    Map,
    Spells,
    Spacer,
    Performance,
}

// helper maybe not needed currently

/// Window content - what the window displays
#[derive(Clone, Debug)]
pub enum WindowContent {
    Text(TextContent),
    TabbedText(TabbedTextContent),
    Progress(ProgressData),
    Countdown(CountdownData),
    Compass(CompassData),
    InjuryDoll(InjuryDollData),
    Indicator(IndicatorData),
    Room(RoomContent),
    Inventory(TextContent),
    CommandInput {
        text: String,
        cursor: usize,
        history: Vec<String>,
        history_index: Option<usize>,
    },
    Hand {
        item: Option<String>,
        link: Option<LinkData>,
    },
    Spells(TextContent), // Spells window - similar to Inventory but with link caching
    ActiveEffects(ActiveEffectsContent), // Active effects (buffs, debuffs, cooldowns, active spells)
    Targets {
        targets_text: String, // Raw text from game (XML formatted)
        count: Option<String>, // Raw count string from targetcount stream (e.g., "[03]")
        entity_id: String, // Stream id for counts (defaults to targetcount)
    },
    Players {
        players_text: String, // Raw text from game (XML formatted)
        count: Option<String>, // Raw count string from playercount stream
        entity_id: String, // Stream id for counts (defaults to playercount)
    },
    Dashboard {
        indicators: Vec<(String, u8)>, // (id, value) pairs
    },
    Performance,
    Empty, // For spacers or not-yet-implemented widgets
}

/// Window position and size
#[derive(Clone, Debug)]
pub struct WindowPosition {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl WindowState {
    pub fn new_text(name: impl Into<String>, max_lines: usize) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            widget_type: WidgetType::Text,
            content: WindowContent::Text(TextContent::new(name, max_lines)),
            position: WindowPosition {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
            visible: true,
            focused: false,
            content_align: None,
        }
    }

    pub fn new_command_input(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            widget_type: WidgetType::CommandInput,
            content: WindowContent::CommandInput {
                text: String::new(),
                cursor: 0,
                history: Vec::new(),
                history_index: None,
            },
            position: WindowPosition {
                x: 0,
                y: 23,
                width: 80,
                height: 1,
            },
            visible: true,
            focused: false,
            content_align: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // WindowPosition tests
    // ===========================================

    #[test]
    fn test_window_position_fields() {
        let pos = WindowPosition {
            x: 10,
            y: 20,
            width: 80,
            height: 24,
        };
        assert_eq!(pos.x, 10);
        assert_eq!(pos.y, 20);
        assert_eq!(pos.width, 80);
        assert_eq!(pos.height, 24);
    }

    #[test]
    fn test_window_position_clone() {
        let pos = WindowPosition {
            x: 5,
            y: 10,
            width: 40,
            height: 20,
        };
        let cloned = pos.clone();
        assert_eq!(cloned.x, pos.x);
        assert_eq!(cloned.y, pos.y);
        assert_eq!(cloned.width, pos.width);
        assert_eq!(cloned.height, pos.height);
    }

    #[test]
    fn test_window_position_zero_size() {
        let pos = WindowPosition {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert_eq!(pos.width, 0);
        assert_eq!(pos.height, 0);
    }

    // ===========================================
    // WidgetType tests
    // ===========================================

    #[test]
    fn test_widget_type_equality() {
        assert_eq!(WidgetType::Text, WidgetType::Text);
        assert_ne!(WidgetType::Text, WidgetType::Progress);
        assert_ne!(WidgetType::Compass, WidgetType::Room);
    }

    #[test]
    fn test_widget_type_clone() {
        let widget_type = WidgetType::Inventory;
        let cloned = widget_type.clone();
        assert_eq!(widget_type, cloned);
    }

    #[test]
    fn test_widget_type_all_variants_distinct() {
        let variants = vec![
            WidgetType::Text,
            WidgetType::TabbedText,
            WidgetType::Progress,
            WidgetType::Countdown,
            WidgetType::Compass,
            WidgetType::Indicator,
            WidgetType::Room,
            WidgetType::Inventory,
            WidgetType::CommandInput,
            WidgetType::Dashboard,
            WidgetType::InjuryDoll,
            WidgetType::Hand,
            WidgetType::ActiveEffects,
            WidgetType::Targets,
            WidgetType::Players,
            WidgetType::Map,
            WidgetType::Spells,
            WidgetType::Spacer,
            WidgetType::Performance,
        ];

        // All variants should be distinct
        for i in 0..variants.len() {
            for j in i + 1..variants.len() {
                assert_ne!(variants[i], variants[j]);
            }
        }
    }

    #[test]
    fn test_widget_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(WidgetType::Text);
        set.insert(WidgetType::Progress);
        set.insert(WidgetType::Compass);
        assert_eq!(set.len(), 3);
        assert!(set.contains(&WidgetType::Text));
    }

    // ===========================================
    // WindowState::new_text tests
    // ===========================================

    #[test]
    fn test_new_text_window_name() {
        let window = WindowState::new_text("main", 1000);
        assert_eq!(window.name, "main");
    }

    #[test]
    fn test_new_text_window_widget_type() {
        let window = WindowState::new_text("test", 100);
        assert_eq!(window.widget_type, WidgetType::Text);
    }

    #[test]
    fn test_new_text_window_visible() {
        let window = WindowState::new_text("test", 100);
        assert!(window.visible);
    }

    #[test]
    fn test_new_text_window_not_focused() {
        let window = WindowState::new_text("test", 100);
        assert!(!window.focused);
    }

    #[test]
    fn test_new_text_window_default_position() {
        let window = WindowState::new_text("test", 100);
        assert_eq!(window.position.x, 0);
        assert_eq!(window.position.y, 0);
        assert_eq!(window.position.width, 80);
        assert_eq!(window.position.height, 24);
    }

    #[test]
    fn test_new_text_window_content_align_none() {
        let window = WindowState::new_text("test", 100);
        assert!(window.content_align.is_none());
    }

    #[test]
    fn test_new_text_window_content_is_text() {
        let window = WindowState::new_text("test", 100);
        match window.content {
            WindowContent::Text(_) => {} // Expected
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_new_text_window_with_string() {
        let window = WindowState::new_text(String::from("story"), 500);
        assert_eq!(window.name, "story");
    }

    // ===========================================
    // WindowState::new_command_input tests
    // ===========================================

    #[test]
    fn test_new_command_input_name() {
        let window = WindowState::new_command_input("command");
        assert_eq!(window.name, "command");
    }

    #[test]
    fn test_new_command_input_widget_type() {
        let window = WindowState::new_command_input("input");
        assert_eq!(window.widget_type, WidgetType::CommandInput);
    }

    #[test]
    fn test_new_command_input_visible() {
        let window = WindowState::new_command_input("input");
        assert!(window.visible);
    }

    #[test]
    fn test_new_command_input_not_focused() {
        let window = WindowState::new_command_input("input");
        assert!(!window.focused);
    }

    #[test]
    fn test_new_command_input_position() {
        let window = WindowState::new_command_input("input");
        assert_eq!(window.position.x, 0);
        assert_eq!(window.position.y, 23);
        assert_eq!(window.position.width, 80);
        assert_eq!(window.position.height, 1);
    }

    #[test]
    fn test_new_command_input_content() {
        let window = WindowState::new_command_input("input");
        match window.content {
            WindowContent::CommandInput {
                text,
                cursor,
                history,
                history_index,
            } => {
                assert!(text.is_empty());
                assert_eq!(cursor, 0);
                assert!(history.is_empty());
                assert!(history_index.is_none());
            }
            _ => panic!("Expected CommandInput content"),
        }
    }

    // ===========================================
    // WindowContent tests
    // ===========================================

    #[test]
    fn test_window_content_empty() {
        let content = WindowContent::Empty;
        match content {
            WindowContent::Empty => {} // Expected
            _ => panic!("Expected Empty content"),
        }
    }

    #[test]
    fn test_window_content_performance() {
        let content = WindowContent::Performance;
        match content {
            WindowContent::Performance => {} // Expected
            _ => panic!("Expected Performance content"),
        }
    }

    #[test]
    fn test_window_content_dashboard() {
        let content = WindowContent::Dashboard {
            indicators: vec![("health".to_string(), 100), ("mana".to_string(), 50)],
        };
        match content {
            WindowContent::Dashboard { indicators } => {
                assert_eq!(indicators.len(), 2);
                assert_eq!(indicators[0].0, "health");
                assert_eq!(indicators[0].1, 100);
            }
            _ => panic!("Expected Dashboard content"),
        }
    }

    #[test]
    fn test_window_content_hand() {
        let content = WindowContent::Hand {
            item: Some("rusty sword".to_string()),
            link: None,
        };
        match content {
            WindowContent::Hand { item, link } => {
                assert_eq!(item, Some("rusty sword".to_string()));
                assert!(link.is_none());
            }
            _ => panic!("Expected Hand content"),
        }
    }

    #[test]
    fn test_window_content_targets() {
        let content = WindowContent::Targets {
            targets_text: "<target>Orc</target>".to_string(),
            count: None,
            entity_id: "targetcount".to_string(),
        };
        match content {
            WindowContent::Targets { targets_text, .. } => {
                assert!(targets_text.contains("Orc"));
            }
            _ => panic!("Expected Targets content"),
        }
    }

    #[test]
    fn test_window_content_players() {
        let content = WindowContent::Players {
            players_text: "<player>Warrior</player>".to_string(),
            count: None,
            entity_id: "playercount".to_string(),
        };
        match content {
            WindowContent::Players { players_text, .. } => {
                assert!(players_text.contains("Warrior"));
            }
            _ => panic!("Expected Players content"),
        }
    }

    // ===========================================
    // WindowState clone tests
    // ===========================================

    #[test]
    fn test_window_state_clone() {
        let window = WindowState::new_text("main", 1000);
        let cloned = window.clone();
        assert_eq!(cloned.name, window.name);
        assert_eq!(cloned.widget_type, window.widget_type);
        assert_eq!(cloned.visible, window.visible);
        assert_eq!(cloned.focused, window.focused);
    }

    #[test]
    fn test_window_state_debug() {
        let window = WindowState::new_text("test", 100);
        let debug_str = format!("{:?}", window);
        assert!(debug_str.contains("WindowState"));
        assert!(debug_str.contains("test"));
    }
}
