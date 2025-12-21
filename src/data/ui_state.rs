//! UI State - Focus, selection, and interaction state
//!
//! This module contains UI state that is independent of rendering.
//! Both TUI and GUI frontends read from these structures.

use super::window::WindowState;
use crate::data::LinkData;
use crate::selection::SelectionState;
use std::collections::HashMap;

/// Application UI state
#[derive(Clone, Debug)]
pub struct UiState {
    /// All windows in the application
    pub windows: HashMap<String, WindowState>,

    /// Widget type index - cached mapping of widget types to window names
    /// Rebuilt when windows are added/removed
    widget_type_index: HashMap<super::window::WidgetType, Vec<String>>,

    /// Currently focused window name
    pub focused_window: Option<String>,

    /// Current input mode
    pub input_mode: InputMode,

    /// Search input (when in Search mode)
    pub search_input: String,
    pub search_cursor: usize,

    /// Popup menu state (main menu or level 1)
    pub popup_menu: Option<PopupMenu>,

    /// Submenu (level 2) - shown when clicking category in popup_menu
    pub submenu: Option<PopupMenu>,

    /// Nested submenu (level 3) - shown when clicking subcategory in submenu
    pub nested_submenu: Option<PopupMenu>,

    /// Deep submenu (level 4) - shown when clicking item in nested_submenu
    pub deep_submenu: Option<PopupMenu>,

    /// Status bar text
    pub status_text: String,

    /// Mouse drag state for window resize/move
    pub mouse_drag: Option<MouseDragState>,

    /// Text selection state
    pub selection_state: Option<SelectionState>,

    /// Mouse position when drag started (for detecting drag vs click)
    pub selection_drag_start: Option<(u16, u16)>,

    /// Link drag state (Ctrl+drag from link)
    pub link_drag_state: Option<LinkDragState>,

    /// Pending link click (released without drag = send _menu)
    pub pending_link_click: Option<PendingLinkClick>,
}

/// Mouse drag state for window operations
#[derive(Clone, Debug)]
pub struct MouseDragState {
    pub operation: DragOperation,
    pub window_name: String,
    pub start_pos: (u16, u16),
    pub original_window_pos: (u16, u16, u16, u16), // x, y, width, height
}

/// Type of mouse drag operation
#[derive(Clone, Debug, PartialEq)]
pub enum DragOperation {
    Move,
    ResizeRight,
    ResizeBottom,
    ResizeBottomRight,
}

/// Link drag state (Ctrl+drag on a link)
#[derive(Clone, Debug)]
pub struct LinkDragState {
    pub link_data: LinkData,
    pub start_pos: (u16, u16),
    pub current_pos: (u16, u16),
}

/// Pending link click (mouse down on link, waiting for mouse up to send _menu)
#[derive(Clone, Debug)]
pub struct PendingLinkClick {
    pub link_data: LinkData,
    pub click_pos: (u16, u16),
}

/// Input mode for the application
#[derive(Clone, Debug, PartialEq)]
pub enum InputMode {
    /// Normal command input
    Normal,
    /// Vi-style navigation mode
    Navigation,
    /// Scrolling through history
    History,
    /// Search mode (Ctrl+F)
    Search,
    /// Popup menu is active (Tab/Shift+Tab navigation)
    Menu,
    /// Window editor is open
    WindowEditor,
    /// Highlight browser is open
    HighlightBrowser,
    /// Highlight form is open (create/edit highlight)
    HighlightForm,
    /// Keybind browser is open
    KeybindBrowser,
    /// Keybind form is open (create/edit keybind)
    KeybindForm,
    /// Color palette browser is open
    ColorPaletteBrowser,
    /// Color form is open (create/edit palette color)
    ColorForm,
    /// UI colors browser is open
    UIColorsBrowser,
    /// Spell colors browser is open
    SpellColorsBrowser,
    /// Spell color form is open (create/edit spell color)
    SpellColorForm,
    /// Theme browser is open
    ThemeBrowser,
    /// Theme editor is open (create/edit theme)
    ThemeEditor,
    /// Settings editor is open
    SettingsEditor,
    /// Indicator template editor is open
    IndicatorTemplateEditor,
}

/// Popup menu state
#[derive(Clone, Debug)]
pub struct PopupMenu {
    pub items: Vec<PopupMenuItem>,
    pub selected: usize,
    pub position: (u16, u16), // x, y position
}

/// A single popup menu item
#[derive(Clone, Debug)]
pub struct PopupMenuItem {
    pub text: String,
    pub command: String,
    pub disabled: bool,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            widget_type_index: HashMap::new(),
            focused_window: None,
            input_mode: InputMode::Normal,
            search_input: String::new(),
            search_cursor: 0,
            popup_menu: None,
            submenu: None,
            nested_submenu: None,
            deep_submenu: None,
            status_text: String::from("Ready"),
            mouse_drag: None,
            selection_state: None,
            selection_drag_start: None,
            link_drag_state: None,
            pending_link_click: None,
        }
    }

    /// Get a window by name
    pub fn get_window(&self, name: &str) -> Option<&WindowState> {
        self.windows.get(name)
    }

    /// Get a mutable window by name
    pub fn get_window_mut(&mut self, name: &str) -> Option<&mut WindowState> {
        self.windows.get_mut(name)
    }

    /// Add or update a window
    pub fn set_window(&mut self, name: String, window: WindowState) {
        self.windows.insert(name, window);
        self.rebuild_widget_index();
    }

    /// Remove a window by name
    pub fn remove_window(&mut self, name: &str) -> Option<WindowState> {
        let result = self.windows.remove(name);
        if result.is_some() {
            self.rebuild_widget_index();
        }
        result
    }

    /// Rebuild the widget type index cache
    /// Called whenever windows are added/removed
    pub fn rebuild_widget_index(&mut self) {
        self.widget_type_index.clear();
        for (name, window) in &self.windows {
            self.widget_type_index
                .entry(window.widget_type.clone())
                .or_default()
                .push(name.clone());
        }
    }

    /// Get a window by widget type and optional name
    /// For singletons (Compass, InjuryDoll): pass None for name
    /// For multi-instance (Countdown, Text, etc): pass Some(name) to specify which one
    pub fn get_window_by_type(
        &self,
        widget_type: super::window::WidgetType,
        name: Option<&str>,
    ) -> Option<&WindowState> {
        let candidates = self.widget_type_index.get(&widget_type)?;

        match name {
            Some(specific_name) => {
                // Multi-instance: find the specific named window
                self.windows.get(specific_name)
            }
            None => {
                // Singleton: return the first (only) window of this type
                candidates.first().and_then(|n| self.windows.get(n))
            }
        }
    }

    /// Get a mutable window by widget type and optional name
    /// For singletons (Compass, InjuryDoll): pass None for name
    /// For multi-instance (Countdown, Text, etc): pass Some(name) to specify which one
    pub fn get_window_by_type_mut(
        &mut self,
        widget_type: super::window::WidgetType,
        name: Option<&str>,
    ) -> Option<&mut WindowState> {
        let candidates = self.widget_type_index.get(&widget_type)?;

        match name {
            Some(specific_name) => {
                // Multi-instance: find the specific named window
                self.windows.get_mut(specific_name)
            }
            None => {
                // Singleton: return the first (only) window of this type
                let window_name = candidates.first()?.clone();
                self.windows.get_mut(&window_name)
            }
        }
    }

    /// Set the focused window
    pub fn set_focus(&mut self, name: Option<String>) {
        // Clear old focus
        if let Some(old_name) = &self.focused_window {
            if let Some(window) = self.windows.get_mut(old_name) {
                window.focused = false;
            }
        }

        // Set new focus
        if let Some(new_name) = &name {
            if let Some(window) = self.windows.get_mut(new_name) {
                window.focused = true;
            }
        }

        self.focused_window = name;
    }

    /// Get the currently focused window
    pub fn focused_window(&self) -> Option<&WindowState> {
        self.focused_window
            .as_ref()
            .and_then(|name| self.windows.get(name))
    }

    /// Get the currently focused window mutably
    pub fn focused_window_mut(&mut self) -> Option<&mut WindowState> {
        let name = self.focused_window.clone();
        name.as_ref().and_then(|n| self.windows.get_mut(n))
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl PopupMenu {
    pub fn new(items: Vec<PopupMenuItem>, position: (u16, u16)) -> Self {
        Self {
            items,
            selected: 0,
            position,
        }
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.items.is_empty() {
            self.selected = if self.selected == 0 {
                self.items.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    pub fn selected_item(&self) -> Option<&PopupMenuItem> {
        self.items.get(self.selected)
    }

    pub fn get_selected(&self) -> Option<&PopupMenuItem> {
        self.items.get(self.selected)
    }

    pub fn get_items(&self) -> &[PopupMenuItem] {
        &self.items
    }

    pub fn get_position(&self) -> (u16, u16) {
        self.position
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected
    }

    /// Check if a mouse click at (x, y) hits a menu item
    /// Returns the index of the clicked item if any
    ///
    /// # Arguments
    /// * `area` - Tuple of (x, y, width, height) representing the menu area
    pub fn check_click(&self, x: u16, y: u16, area: (u16, u16, u16, u16)) -> Option<usize> {
        let (area_x, area_y, area_width, area_height) = area;

        // Check if click is within the menu area
        if x < area_x || x >= area_x + area_width || y < area_y || y >= area_y + area_height {
            return None;
        }

        // Calculate which item was clicked (accounting for border and title)
        let relative_y = (y - area_y) as usize;

        // Border takes 1 row at top and bottom
        if relative_y == 0 || relative_y >= area_height as usize - 1 {
            return None; // Clicked on border
        }

        let item_index = relative_y - 1; // Subtract top border

        if item_index < self.items.len() {
            Some(item_index)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== UiState Tests ====================

    #[test]
    fn test_ui_state_new() {
        let state = UiState::new();
        assert!(state.windows.is_empty());
        assert!(state.focused_window.is_none());
        assert_eq!(state.input_mode, InputMode::Normal);
        assert!(state.search_input.is_empty());
        assert_eq!(state.search_cursor, 0);
        assert!(state.popup_menu.is_none());
        assert!(state.submenu.is_none());
        assert!(state.nested_submenu.is_none());
        assert_eq!(state.status_text, "Ready");
        assert!(state.mouse_drag.is_none());
        assert!(state.selection_state.is_none());
    }

    #[test]
    fn test_ui_state_default() {
        let state = UiState::default();
        assert!(state.windows.is_empty());
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_ui_state_get_nonexistent_window() {
        let state = UiState::new();
        assert!(state.get_window("nonexistent").is_none());
    }

    #[test]
    fn test_ui_state_focused_window_none() {
        let state = UiState::new();
        assert!(state.focused_window().is_none());
    }

    // ==================== InputMode Tests ====================

    #[test]
    fn test_input_mode_equality() {
        assert_eq!(InputMode::Normal, InputMode::Normal);
        assert_ne!(InputMode::Normal, InputMode::Navigation);
        assert_ne!(InputMode::History, InputMode::Search);
    }

    #[test]
    fn test_input_mode_clone() {
        let mode = InputMode::WindowEditor;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_input_mode_debug() {
        let debug_str = format!("{:?}", InputMode::HighlightBrowser);
        assert!(debug_str.contains("HighlightBrowser"));
    }

    #[test]
    fn test_all_input_modes_distinct() {
        let modes = vec![
            InputMode::Normal,
            InputMode::Navigation,
            InputMode::History,
            InputMode::Search,
            InputMode::Menu,
            InputMode::WindowEditor,
            InputMode::HighlightBrowser,
            InputMode::HighlightForm,
            InputMode::KeybindBrowser,
            InputMode::KeybindForm,
            InputMode::ColorPaletteBrowser,
            InputMode::ColorForm,
            InputMode::UIColorsBrowser,
            InputMode::SpellColorsBrowser,
            InputMode::SpellColorForm,
            InputMode::ThemeBrowser,
            InputMode::ThemeEditor,
            InputMode::SettingsEditor,
        ];

        // All modes should be distinct
        for i in 0..modes.len() {
            for j in i + 1..modes.len() {
                assert_ne!(modes[i], modes[j]);
            }
        }
    }

    // ==================== DragOperation Tests ====================

    #[test]
    fn test_drag_operation_equality() {
        assert_eq!(DragOperation::Move, DragOperation::Move);
        assert_ne!(DragOperation::Move, DragOperation::ResizeRight);
        assert_ne!(DragOperation::ResizeBottom, DragOperation::ResizeBottomRight);
    }

    #[test]
    fn test_drag_operation_clone() {
        let op = DragOperation::ResizeBottomRight;
        let cloned = op.clone();
        assert_eq!(op, cloned);
    }

    #[test]
    fn test_drag_operation_debug() {
        let debug_str = format!("{:?}", DragOperation::Move);
        assert!(debug_str.contains("Move"));
    }

    // ==================== PopupMenuItem Tests ====================

    #[test]
    fn test_popup_menu_item_creation() {
        let item = PopupMenuItem {
            text: "Look".to_string(),
            command: "look".to_string(),
            disabled: false,
        };
        assert_eq!(item.text, "Look");
        assert_eq!(item.command, "look");
        assert!(!item.disabled);
    }

    #[test]
    fn test_popup_menu_item_disabled() {
        let item = PopupMenuItem {
            text: "Disabled Action".to_string(),
            command: "disabled".to_string(),
            disabled: true,
        };
        assert!(item.disabled);
    }

    #[test]
    fn test_popup_menu_item_clone() {
        let item = PopupMenuItem {
            text: "Get".to_string(),
            command: "get".to_string(),
            disabled: false,
        };
        let cloned = item.clone();
        assert_eq!(cloned.text, item.text);
        assert_eq!(cloned.command, item.command);
        assert_eq!(cloned.disabled, item.disabled);
    }

    // ==================== PopupMenu Tests ====================

    fn create_test_menu() -> PopupMenu {
        let items = vec![
            PopupMenuItem {
                text: "Look".to_string(),
                command: "look".to_string(),
                disabled: false,
            },
            PopupMenuItem {
                text: "Get".to_string(),
                command: "get".to_string(),
                disabled: false,
            },
            PopupMenuItem {
                text: "Drop".to_string(),
                command: "drop".to_string(),
                disabled: false,
            },
        ];
        PopupMenu::new(items, (10, 20))
    }

    #[test]
    fn test_popup_menu_new() {
        let menu = create_test_menu();
        assert_eq!(menu.items.len(), 3);
        assert_eq!(menu.selected, 0);
        assert_eq!(menu.position, (10, 20));
    }

    #[test]
    fn test_popup_menu_empty() {
        let menu = PopupMenu::new(vec![], (0, 0));
        assert!(menu.items.is_empty());
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_select_next() {
        let mut menu = create_test_menu();
        assert_eq!(menu.selected, 0);

        menu.select_next();
        assert_eq!(menu.selected, 1);

        menu.select_next();
        assert_eq!(menu.selected, 2);

        // Should wrap around
        menu.select_next();
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_select_next_empty() {
        let mut menu = PopupMenu::new(vec![], (0, 0));
        menu.select_next(); // Should not panic
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_select_prev() {
        let mut menu = create_test_menu();
        assert_eq!(menu.selected, 0);

        // Should wrap to last item
        menu.select_prev();
        assert_eq!(menu.selected, 2);

        menu.select_prev();
        assert_eq!(menu.selected, 1);

        menu.select_prev();
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_select_prev_empty() {
        let mut menu = PopupMenu::new(vec![], (0, 0));
        menu.select_prev(); // Should not panic
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_selected_item() {
        let menu = create_test_menu();
        let item = menu.selected_item().unwrap();
        assert_eq!(item.text, "Look");
    }

    #[test]
    fn test_popup_menu_selected_item_after_navigation() {
        let mut menu = create_test_menu();
        menu.select_next();
        let item = menu.selected_item().unwrap();
        assert_eq!(item.text, "Get");
    }

    #[test]
    fn test_popup_menu_selected_item_empty() {
        let menu = PopupMenu::new(vec![], (0, 0));
        assert!(menu.selected_item().is_none());
    }

    #[test]
    fn test_popup_menu_get_selected() {
        let menu = create_test_menu();
        let item = menu.get_selected().unwrap();
        assert_eq!(item.command, "look");
    }

    #[test]
    fn test_popup_menu_get_items() {
        let menu = create_test_menu();
        let items = menu.get_items();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "Look");
        assert_eq!(items[1].text, "Get");
        assert_eq!(items[2].text, "Drop");
    }

    #[test]
    fn test_popup_menu_get_position() {
        let menu = create_test_menu();
        assert_eq!(menu.get_position(), (10, 20));
    }

    #[test]
    fn test_popup_menu_get_selected_index() {
        let mut menu = create_test_menu();
        assert_eq!(menu.get_selected_index(), 0);

        menu.select_next();
        assert_eq!(menu.get_selected_index(), 1);
    }

    // ==================== PopupMenu::check_click Tests ====================

    #[test]
    fn test_check_click_outside_left() {
        let menu = create_test_menu();
        // Area starts at x=10, click at x=5 is outside
        let result = menu.check_click(5, 22, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_outside_right() {
        let menu = create_test_menu();
        // Area is x=10 to x=30 (10+20), click at x=35 is outside
        let result = menu.check_click(35, 22, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_outside_top() {
        let menu = create_test_menu();
        // Area starts at y=20, click at y=15 is outside
        let result = menu.check_click(15, 15, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_outside_bottom() {
        let menu = create_test_menu();
        // Area is y=20 to y=25 (20+5), click at y=30 is outside
        let result = menu.check_click(15, 30, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_on_top_border() {
        let menu = create_test_menu();
        // y=20 is the top border (relative_y=0)
        let result = menu.check_click(15, 20, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_on_bottom_border() {
        let menu = create_test_menu();
        // y=24 is the bottom border (area_height-1 = 4)
        let result = menu.check_click(15, 24, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_first_item() {
        let menu = create_test_menu();
        // y=21 is the first item (relative_y=1, item_index=0)
        let result = menu.check_click(15, 21, (10, 20, 20, 5));
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_check_click_second_item() {
        let menu = create_test_menu();
        // y=22 is the second item (relative_y=2, item_index=1)
        let result = menu.check_click(15, 22, (10, 20, 20, 5));
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_check_click_third_item() {
        let menu = create_test_menu();
        // y=23 is the third item (relative_y=3, item_index=2)
        let result = menu.check_click(15, 23, (10, 20, 20, 5));
        assert_eq!(result, Some(2));
    }

    #[test]
    fn test_check_click_beyond_items() {
        // Menu with only 2 items, but area has room for more
        let items = vec![
            PopupMenuItem {
                text: "A".to_string(),
                command: "a".to_string(),
                disabled: false,
            },
            PopupMenuItem {
                text: "B".to_string(),
                command: "b".to_string(),
                disabled: false,
            },
        ];
        let menu = PopupMenu::new(items, (0, 0));

        // Click on what would be item 3 (but menu only has 2 items)
        // Area height = 6, so relative_y=3 gives item_index=2
        let result = menu.check_click(5, 3, (0, 0, 20, 6));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_at_area_boundary() {
        let menu = create_test_menu();
        // Click at the exact right edge (x=29, just inside x=10+20-1)
        let result = menu.check_click(29, 21, (10, 20, 20, 5));
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_check_click_at_area_corner() {
        let menu = create_test_menu();
        // Click at top-left corner (border)
        let result = menu.check_click(10, 20, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    // ==================== MouseDragState Tests ====================

    #[test]
    fn test_mouse_drag_state_creation() {
        let drag = MouseDragState {
            operation: DragOperation::Move,
            window_name: "main".to_string(),
            start_pos: (100, 200),
            original_window_pos: (10, 20, 80, 40),
        };
        assert_eq!(drag.operation, DragOperation::Move);
        assert_eq!(drag.window_name, "main");
        assert_eq!(drag.start_pos, (100, 200));
        assert_eq!(drag.original_window_pos, (10, 20, 80, 40));
    }

    #[test]
    fn test_mouse_drag_state_clone() {
        let drag = MouseDragState {
            operation: DragOperation::ResizeRight,
            window_name: "story".to_string(),
            start_pos: (50, 60),
            original_window_pos: (0, 0, 100, 50),
        };
        let cloned = drag.clone();
        assert_eq!(cloned.operation, drag.operation);
        assert_eq!(cloned.window_name, drag.window_name);
    }

    // ==================== PopupMenu Clone Tests ====================

    #[test]
    fn test_popup_menu_clone() {
        let mut menu = create_test_menu();
        menu.select_next();

        let cloned = menu.clone();
        assert_eq!(cloned.items.len(), menu.items.len());
        assert_eq!(cloned.selected, menu.selected);
        assert_eq!(cloned.position, menu.position);
    }

    // ==================== UiState Clone Tests ====================

    #[test]
    fn test_ui_state_clone() {
        let state = UiState::new();
        let cloned = state.clone();
        assert_eq!(cloned.input_mode, state.input_mode);
        assert_eq!(cloned.status_text, state.status_text);
    }
}
