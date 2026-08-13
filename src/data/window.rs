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
    /// If true, this window is ephemeral (session-only, not saved to layout)
    pub ephemeral: bool,
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
    Reserve,
    CommandInput,
    Dashboard,
    InjuryDoll,
    Hand,
    ActiveEffects,
    Targets, // New component-based (default)
    Players,
    Items, // Room objects (non-creatures)
    Spells,
    Spacer,
    Performance,
    Perception,
    Container,
    Experience,
    GS4Experience,
    Encumbrance,
    Quickbar,
    Hotkeybar,
    MiniVitals,
    Betrayer,
    /// Lich WebUI page rendered natively from its JSON component tree
    WebUi,
    /// Auto-generated location map (mini map)
    Map,
    /// A resident game dialog panel (combat, befriend, ...) rendered from
    /// the accumulated dialog store by its id.
    DialogPanel,
    /// Watched spells currently NOT active in ActiveSpells/Buffs.
    /// Reads from GameState (character.watched_spells + effects).
    MissingSpells,
    /// Status cards for every other VellumFE instance on this machine,
    /// clustered by group. Reads from the MultiAccountHub on the GUI app,
    /// not from GameState.
    MultiAccount,
}

impl WidgetType {
    /// Parse a widget type string to WidgetType enum
    ///
    /// Returns Text as the default for unknown widget types.
    pub fn from_str(s: &str) -> Self {
        Self::try_from_str(s).unwrap_or(WidgetType::Text)
    }

    /// Try to parse a widget type string to WidgetType enum
    ///
    /// Returns None for unknown widget types.
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "text" => Some(WidgetType::Text),
            "tabbedtext" => Some(WidgetType::TabbedText),
            "progress" => Some(WidgetType::Progress),
            "countdown" => Some(WidgetType::Countdown),
            "compass" => Some(WidgetType::Compass),
            "injury_doll" | "injuries" => Some(WidgetType::InjuryDoll),
            "indicator" => Some(WidgetType::Indicator),
            "room" => Some(WidgetType::Room),
            "inventory" => Some(WidgetType::Inventory),
            "reserve" => Some(WidgetType::Reserve),
            "command_input" | "commandinput" => Some(WidgetType::CommandInput),
            "dashboard" => Some(WidgetType::Dashboard),
            "hand" => Some(WidgetType::Hand),
            "active_effects" => Some(WidgetType::ActiveEffects),
            "targets" => Some(WidgetType::Targets), // Component-based (default)
            "players" => Some(WidgetType::Players),
            "missingspells" => Some(WidgetType::MissingSpells),
            "multiaccount" => Some(WidgetType::MultiAccount),
            "items" => Some(WidgetType::Items),
            "spells" => Some(WidgetType::Spells),
            "perception" => Some(WidgetType::Perception),
            "performance" => Some(WidgetType::Performance),
            "spacer" => Some(WidgetType::Spacer),
            "container" => Some(WidgetType::Container),
            "experience" => Some(WidgetType::Experience),
            "gs4_experience" => Some(WidgetType::GS4Experience),
            "encum" => Some(WidgetType::Encumbrance),
            "quickbar" => Some(WidgetType::Quickbar),
            "hotkeybar" => Some(WidgetType::Hotkeybar),
            "minivitals" => Some(WidgetType::MiniVitals),
            "betrayer" => Some(WidgetType::Betrayer),
            "webui" | "lichui" => Some(WidgetType::WebUi),
            "map" => Some(WidgetType::Map),
            "dialogpanel" | "dialog_panel" => Some(WidgetType::DialogPanel),
            _ => None,
        }
    }

    /// List of valid widget type names for help messages
    pub const VALID_TYPES: &'static [&'static str] = &[
        "text",
        "tabbedtext",
        "progress",
        "countdown",
        "compass",
        "injury_doll",
        "indicator",
        "room",
        "inventory",
        "reserve",
        "command_input",
        "dashboard",
        "hand",
        "active_effects",
        "targets",
        "players",
        "spells",
        "missingspells",
        "multiaccount",
        "perception",
        "performance",
        "spacer",
        "container",
        "experience",
        "gs4_experience",
        "encum",
        "quickbar",
        "hotkeybar",
        "minivitals",
        "betrayer",
        "webui",
        "map",
        "dialogpanel",
    ];
}

/// Window content - what the window displays
#[derive(Clone, Debug)]
pub enum WindowContent {
    Text(TextContent),
    Map(MapData),
    TabbedText(TabbedTextContent),
    Progress(ProgressData),
    Countdown(CountdownData),
    Compass(CompassData),
    InjuryDoll(InjuryDollData),
    Indicator(IndicatorData),
    Room(RoomContent),
    Inventory(TextContent),
    /// Reserved-items window - same snapshot semantics as Inventory but fed
    /// by the `reserve` stream
    Reserve(TextContent),
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
    /// Component-based target list (room objs)
    /// Reads from GameState.room_creatures
    Targets,
    /// Component-based players list (room players)
    /// Reads from GameState.room_players
    Players,
    /// Component-based items list (room objs, non-creatures)
    /// Reads from GameState.room_objects
    Items,
    /// Multi-account status cards: one per sibling VellumFE instance on
    /// this machine, grouped by `core::multiaccount::cluster_peers`. Carries
    /// no data -- the peer table lives on the GUI app (MultiAccountHub is
    /// neither Clone nor Debug) and arrives via WidgetRenderSettings.
    MultiAccount,
    /// Missing-spells watchlist (watched spells not currently active).
    /// Reads from GameState.character.watched_spells + effects.
    MissingSpells,
    Dashboard {
        indicators: Vec<(String, u8)>, // (id, value) pairs
    },
    Performance,
    Perception(PerceptionData), // Perception window - sorted spells/buffs
    /// Container window - displays contents of a specific container
    Container {
        container_title: String, // Title of the container to display (e.g., "Bandolier")
    },
    /// Experience window - displays DR skill/experience components
    /// Reads from GameState.dr_experience (no data stored here)
    Experience,
    /// GS4 Experience window - displays level, mind state, experience
    /// Reads from GameState.gs4_experience (no data stored here)
    GS4Experience,
    /// Encumbrance window - displays progress bar + optional label
    /// Reads from GameState.encumbrance (no data stored here)
    Encumbrance,
    Quickbar,
    /// Hotkey bar - buttons resolved each frame from config.hotbars +
    /// GameState by core::hotbar::resolve_bar; carries only its bar binding
    Hotkeybar {
        bar: String, // Name of the bar in hotbars.toml
    },
    /// MiniVitals window - displays health, mana, stamina, spirit as horizontal bars
    /// Reads from GameState.vitals (no data stored here)
    MiniVitals,
    /// Betrayer window - displays blood points progress bar and item list
    /// Reads from GameState.betrayer (no data stored here)
    Betrayer,
    /// Lich WebUI panel - carries its page binding and latest component tree
    WebUi(super::webui::WebUiPanelContent),
    /// Resident dialog panel (combat, befriend, ...) - carries which dialog
    /// id it renders; content comes from ui_state.dialog_store by that id.
    DialogPanel {
        dialog_id: String,
    },
    Empty, // For spacers or not-yet-implemented widgets
}

/// Window position and size
#[derive(Clone, Debug)]
pub struct WindowPosition {
    pub x: super::geometry::Col,
    pub y: super::geometry::Row,
    pub width: super::geometry::Width,
    pub height: super::geometry::Height,
}

/// Compute the new window position for a mouse drag, given the position at
/// drag-start (`orig`), the accumulated pointer delta (`dx`, `dy`), the
/// window's minimum size, and the terminal size.
///
/// Pure geometry extracted from the TUI mouse-drag handler so it can be unit
/// tested (and so a future geometry newtype has one place to touch instead of
/// four inline branches). Behavior mirrors the original exactly:
/// - Move: shift the top-left by the delta, floored at (0,0), then clamp so the
///   window stays fully on screen (its size is unchanged, so max origin =
///   term - size, saturating).
/// - Resize*: grow/shrink the dragged edge(s) by the delta, floored at the
///   window's min size, then clamp to the terminal edge (max extent =
///   term - origin, saturating). The origin never moves on a resize.
pub fn apply_window_drag(
    op: crate::data::ui_state::DragOperation,
    orig: WindowPosition,
    dx: i32,
    dy: i32,
    min_width: u16,
    min_height: u16,
    term_width: u16,
    term_height: u16,
) -> WindowPosition {
    use crate::data::ui_state::DragOperation;

    use super::geometry::{Col, Height, Row, Width};

    let mut pos = orig.clone();
    match op {
        DragOperation::Move => {
            let new_x = Col::from_i32_saturating(orig.x.get() as i32 + dx);
            let new_y = Row::from_i32_saturating(orig.y.get() as i32 + dy);
            // Window size is unchanged on a move, so the max origin keeps the
            // window fully on screen: term - size (saturating).
            let max_x = Col::new(term_width) - orig.width;
            let max_y = Row::new(term_height) - orig.height;
            pos.x = new_x.min(max_x);
            pos.y = new_y.min(max_y);
        }
        DragOperation::ResizeRight => {
            let new_width =
                Width::from_i32_saturating(orig.width.get() as i32 + dx).max(Width::new(min_width));
            // The origin never moves on a resize, so max extent = term - origin.
            let max_width = Width::new(term_width) - Width::new(orig.x.get());
            pos.width = new_width.min(max_width);
        }
        DragOperation::ResizeBottom => {
            let new_height = Height::from_i32_saturating(orig.height.get() as i32 + dy)
                .max(Height::new(min_height));
            let max_height = Height::new(term_height) - Height::new(orig.y.get());
            pos.height = new_height.min(max_height);
        }
        DragOperation::ResizeBottomRight => {
            let new_width =
                Width::from_i32_saturating(orig.width.get() as i32 + dx).max(Width::new(min_width));
            let new_height = Height::from_i32_saturating(orig.height.get() as i32 + dy)
                .max(Height::new(min_height));
            let max_width = Width::new(term_width) - Width::new(orig.x.get());
            let max_height = Height::new(term_height) - Height::new(orig.y.get());
            pos.width = new_width.min(max_width);
            pos.height = new_height.min(max_height);
        }
    }
    pos
}

impl WindowState {
    pub fn new_text(name: impl Into<String>, max_lines: usize) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            widget_type: WidgetType::Text,
            content: WindowContent::Text(TextContent::new(name, max_lines)),
            position: WindowPosition {
                x: super::geometry::Col::new(0),
                y: super::geometry::Row::new(0),
                width: super::geometry::Width::new(80),
                height: super::geometry::Height::new(24),
            },
            visible: true,
            focused: false,
            content_align: None,
            ephemeral: false,
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
                x: super::geometry::Col::new(0),
                y: super::geometry::Row::new(23),
                width: super::geometry::Width::new(80),
                height: super::geometry::Height::new(1),
            },
            visible: true,
            focused: false,
            content_align: None,
            ephemeral: false,
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
        let pos = pos(10, 20, 80, 24);
        assert_eq!(pos.x.get(), 10);
        assert_eq!(pos.y.get(), 20);
        assert_eq!(pos.width.get(), 80);
        assert_eq!(pos.height.get(), 24);
    }

    #[test]
    fn test_window_position_clone() {
        let pos = pos(5, 10, 40, 20);
        let cloned = pos.clone();
        assert_eq!(cloned.x, pos.x);
        assert_eq!(cloned.y, pos.y);
        assert_eq!(cloned.width, pos.width);
        assert_eq!(cloned.height, pos.height);
    }

    #[test]
    fn test_window_position_zero_size() {
        let pos = pos(0, 0, 0, 0);
        assert_eq!(pos.width.get(), 0);
        assert_eq!(pos.height.get(), 0);
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
            WidgetType::Items,
            WidgetType::Spells,
            WidgetType::Spacer,
            WidgetType::Performance,
            WidgetType::Perception,
            WidgetType::Quickbar,
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
        assert_eq!(window.position.x.get(), 0);
        assert_eq!(window.position.y.get(), 0);
        assert_eq!(window.position.width.get(), 80);
        assert_eq!(window.position.height.get(), 24);
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
        assert_eq!(window.position.x.get(), 0);
        assert_eq!(window.position.y.get(), 23);
        assert_eq!(window.position.width.get(), 80);
        assert_eq!(window.position.height.get(), 1);
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
        let content = WindowContent::Targets;
        assert!(matches!(content, WindowContent::Targets));
    }

    #[test]
    fn test_window_content_players() {
        let content = WindowContent::Players;
        assert!(matches!(content, WindowContent::Players));
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

    // ========== apply_window_drag characterization ==========
    use crate::data::ui_state::DragOperation;

    fn pos(x: u16, y: u16, w: u16, h: u16) -> WindowPosition {
        use crate::data::geometry::{Col, Height, Row, Width};
        WindowPosition {
            x: Col::new(x),
            y: Row::new(y),
            width: Width::new(w),
            height: Height::new(h),
        }
    }

    /// Move shifts the origin by the delta; size is unchanged.
    #[test]
    fn apply_window_drag_move_shifts_origin() {
        let out = apply_window_drag(DragOperation::Move, pos(10, 5, 20, 8), 3, -2, 5, 3, 100, 40);
        assert_eq!(
            (out.x.get(), out.y.get(), out.width.get(), out.height.get()),
            (13, 3, 20, 8)
        );
    }

    /// Move floors the origin at (0,0) — dragging past the top-left edge.
    #[test]
    fn apply_window_drag_move_floors_at_origin() {
        let out = apply_window_drag(DragOperation::Move, pos(2, 2, 20, 8), -10, -10, 5, 3, 100, 40);
        assert_eq!((out.x.get(), out.y.get()), (0, 0));
    }

    /// Move clamps so the window stays fully on screen (max origin = term-size).
    #[test]
    fn apply_window_drag_move_clamps_to_stay_onscreen() {
        // term 100x40, window 20x8 -> max origin (80, 32). Drag far right/down.
        let out =
            apply_window_drag(DragOperation::Move, pos(50, 20, 20, 8), 1000, 1000, 5, 3, 100, 40);
        assert_eq!((out.x.get(), out.y.get()), (80, 32));
    }

    /// ResizeRight grows the width by dx; origin and height untouched.
    #[test]
    fn apply_window_drag_resize_right_grows_width() {
        let out =
            apply_window_drag(DragOperation::ResizeRight, pos(10, 5, 20, 8), 15, 0, 5, 3, 100, 40);
        assert_eq!(
            (out.x.get(), out.y.get(), out.width.get(), out.height.get()),
            (10, 5, 35, 8)
        );
    }

    /// ResizeRight floors the width at the window minimum.
    #[test]
    fn apply_window_drag_resize_right_floors_at_min_width() {
        let out = apply_window_drag(
            DragOperation::ResizeRight,
            pos(10, 5, 20, 8),
            -100,
            0,
            5, // min_width
            3,
            100,
            40,
        );
        assert_eq!(out.width.get(), 5);
    }

    /// ResizeRight clamps the width to the terminal edge (max = term - x).
    #[test]
    fn apply_window_drag_resize_right_clamps_to_terminal_edge() {
        // x=90 in a 100-wide terminal -> max width 10.
        let out =
            apply_window_drag(DragOperation::ResizeRight, pos(90, 5, 5, 8), 1000, 0, 3, 3, 100, 40);
        assert_eq!(out.width.get(), 10);
    }

    /// ResizeBottom grows height by dy, floored at min, clamped to term-y.
    #[test]
    fn apply_window_drag_resize_bottom() {
        let grow =
            apply_window_drag(DragOperation::ResizeBottom, pos(0, 10, 20, 8), 0, 12, 5, 3, 100, 40);
        assert_eq!(grow.height.get(), 20);
        // Clamp: y=35 in 40-tall term -> max height 5.
        let clamp = apply_window_drag(
            DragOperation::ResizeBottom,
            pos(0, 35, 20, 3),
            0,
            1000,
            3,
            3,
            100,
            40,
        );
        assert_eq!(clamp.height.get(), 5);
    }

    /// ResizeBottomRight applies both width and height changes independently.
    #[test]
    fn apply_window_drag_resize_bottom_right() {
        let out = apply_window_drag(
            DragOperation::ResizeBottomRight,
            pos(10, 5, 20, 8),
            10,
            6,
            5,
            3,
            100,
            40,
        );
        assert_eq!(
            (out.x.get(), out.y.get(), out.width.get(), out.height.get()),
            (10, 5, 30, 14)
        );
    }
}
