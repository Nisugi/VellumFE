//! Popup form for creating or editing keybind definitions from the TUI.
//!
//! Handles both action-style bindings (predefined commands) and macro text,
//! validates combinations, and integrates with the shared widget traits so the
//! broader UI can drive it uniformly.

use crate::frontend::tui::crossterm_bridge;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Clear, Widget},
};
use tui_textarea::TextArea;

/// Result of keybind form interaction
#[derive(Debug, Clone)]
pub enum KeybindFormResult {
    Save {
        key_combo: String,
        action_type: KeybindActionType,
        value: String,
    },
    Delete {
        key_combo: String,
    },
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeybindActionType {
    Action, // Built-in action
    Macro,  // Macro text
}

/// Action sections for keybind browser navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSection {
    CommandInput,
    CommandHistory,
    WindowScrolling,
    TabNavigation,
    Search,
    Clipboard,
    TTS,
    SystemToggles,
    Meta,
}

/// Keybind management form widget
pub struct KeybindFormWidget {
    key_combo: TextArea<'static>,
    action_type: KeybindActionType,
    action_dropdown_index: usize, // Index in AVAILABLE_ACTIONS
    macro_text: TextArea<'static>,

    focused_field: usize, // 0=action_type_action, 1=action_type_macro, 2=key_combo, 3=action/macro field
    status_message: String,
    key_combo_error: Option<String>,
    mode: FormMode,

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}

#[derive(Debug, Clone, PartialEq)]
enum FormMode {
    Create,
    Edit { original_key: String },
}

// Available built-in actions
const AVAILABLE_ACTIONS: &[&str] = &[
    "send_command",
    "cursor_left",
    "cursor_right",
    "cursor_word_left",
    "cursor_word_right",
    "cursor_home",
    "cursor_end",
    "cursor_backspace",
    "cursor_delete",
    "cursor_delete_word",
    "cursor_clear_line",
    "switch_current_window",
    "scroll_current_window_up_one",
    "scroll_current_window_down_one",
    "scroll_current_window_up_page",
    "scroll_current_window_down_page",
    "scroll_current_window_home",
    "scroll_current_window_end",
    "previous_command",
    "next_command",
    "start_search",
    "prev_search_match",
    "next_search_match",
    "toggle_performance_stats",
];

impl KeybindFormWidget {
    pub fn new() -> Self {
        let mut key_combo = TextArea::default();
        key_combo.set_placeholder_text("e.g., ctrl+e, f5, alt+shift+a");

        let mut macro_text = TextArea::default();
        macro_text.set_placeholder_text("e.g., run left\\r");

        Self {
            key_combo,
            action_type: KeybindActionType::Action,
            action_dropdown_index: 0,
            macro_text,
            focused_field: 0,
            status_message: String::new(),
            key_combo_error: None,
            mode: FormMode::Create,
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn new_edit(key_combo: String, action_type: KeybindActionType, value: String) -> Self {
        let mut form = Self::new();
        form.key_combo.insert_str(&key_combo);
        form.action_type = action_type.clone();

        match action_type {
            KeybindActionType::Action => {
                // Find action in list
                if let Some(idx) = AVAILABLE_ACTIONS.iter().position(|&a| a == value) {
                    form.action_dropdown_index = idx;
                }
            }
            KeybindActionType::Macro => {
                form.macro_text.insert_str(&value);
            }
        }

        form.mode = FormMode::Edit {
            original_key: key_combo,
        };
        form
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<KeybindFormResult> {
        // Note: All navigation keys (Tab, Shift+Tab, Esc, Space, Up, Down, Ctrl+S, Ctrl+D, Ctrl+A)
        // are now routed via MenuAction in mod.rs. This method only handles text input.

        // Pass to text inputs
        let rt_key = crate::frontend::tui::textarea_bridge::to_textarea_event(key);

        let _handled = match self.focused_field {
            2 => {
                // Field 2: Key Combo
                let result = self.key_combo.input(rt_key);
                self.validate_key_combo();
                result
            }
            3 if self.action_type == KeybindActionType::Macro => {
                // Field 3: Macro text (only when macro type is selected)
                self.macro_text.input(rt_key)
            }
            _ => false,
        };
        None
    }

    /// Handle MenuAction (called from mod.rs input routing)
    pub fn handle_action(&mut self, action: crate::core::menu_actions::MenuAction) -> Option<KeybindFormResult> {
        use crate::core::menu_actions::MenuAction;

        match action {
            MenuAction::NavigateUp => {
                // Up arrow - navigate to previous field
                self.previous_field();
                None
            }
            MenuAction::NavigateDown => {
                // Down arrow - navigate to next field
                self.next_field();
                None
            }
            MenuAction::CycleBackward => {
                // Left arrow - cycle dropdown backward (when on action dropdown)
                if self.focused_field == 3 && self.action_type == KeybindActionType::Action {
                    self.action_dropdown_index = self.action_dropdown_index.saturating_sub(1);
                }
                None
            }
            MenuAction::CycleForward => {
                // Right arrow - cycle dropdown forward (when on action dropdown)
                if self.focused_field == 3 && self.action_type == KeybindActionType::Action {
                    self.action_dropdown_index =
                        (self.action_dropdown_index + 1).min(AVAILABLE_ACTIONS.len() - 1);
                }
                None
            }
            MenuAction::Select | MenuAction::Toggle => {
                // Enter/Space - toggle radio buttons for action type selection
                match self.focused_field {
                    0 => {
                        self.action_type = KeybindActionType::Action;
                    }
                    1 => {
                        self.action_type = KeybindActionType::Macro;
                    }
                    _ => {}
                }
                None
            }
            MenuAction::Save => {
                // Ctrl+S - save the form
                self.save_internal()
            }
            MenuAction::Delete => {
                // Delete key or Ctrl+D - delete keybind (only in edit mode)
                if matches!(self.mode, FormMode::Edit { .. }) {
                    self.try_delete()
                } else {
                    None
                }
            }
            _ => None
        }
    }

    fn validate_key_combo(&mut self) {
        let combo = self.key_combo.lines()[0].as_str();
        if combo.is_empty() {
            self.key_combo_error = None;
            return;
        }

        // Basic validation - check if it looks like a valid key combo
        // Valid formats: "a", "ctrl+a", "alt+shift+f5", etc.
        let parts: Vec<&str> = combo.split('+').collect();
        let mut has_key = false;

        for part in &parts {
            let normalized = part.trim().to_lowercase();
            if matches!(
                normalized.as_str(),
                "a" | "b"
                    | "c"
                    | "d"
                    | "e"
                    | "f"
                    | "g"
                    | "h"
                    | "i"
                    | "j"
                    | "k"
                    | "l"
                    | "m"
                    | "n"
                    | "o"
                    | "p"
                    | "q"
                    | "r"
                    | "s"
                    | "t"
                    | "u"
                    | "v"
                    | "w"
                    | "x"
                    | "y"
                    | "z"
                    | "f1"
                    | "f2"
                    | "f3"
                    | "f4"
                    | "f5"
                    | "f6"
                    | "f7"
                    | "f8"
                    | "f9"
                    | "f10"
                    | "f11"
                    | "f12"
                    | "enter"
                    | "space"
                    | "tab"
                    | "esc"
                    | "backspace"
                    | "delete"
                    | "home"
                    | "end"
                    | "page_up"
                    | "page_down"
                    | "up"
                    | "down"
                    | "left"
                    | "right"
                    | "num_0"
                    | "num_1"
                    | "num_2"
                    | "num_3"
                    | "num_4"
                    | "num_5"
                    | "num_6"
                    | "num_7"
                    | "num_8"
                    | "num_9"
                    | "num_."
                    | "num_+"
                    | "num_-"
                    | "num_*"
                    | "num_/"
            ) {
                has_key = true;
            } else if !matches!(normalized.as_str(), "ctrl" | "alt" | "shift") {
                self.key_combo_error = Some(format!("Invalid key: '{}'", part));
                return;
            }
        }

        if !has_key {
            self.key_combo_error = Some("Must specify a key (not just modifiers)".to_string());
        } else {
            self.key_combo_error = None;
        }
    }

    fn save_internal(&mut self) -> Option<KeybindFormResult> {
        self.validate_key_combo();

        let key_combo = self.key_combo.lines()[0].to_string();

        if key_combo.is_empty() {
            self.status_message = "Key combo cannot be empty".to_string();
            return None;
        }

        if self.key_combo_error.is_some() {
            self.status_message = "Fix validation errors before saving".to_string();
            return None;
        }

        let value = match self.action_type {
            KeybindActionType::Action => AVAILABLE_ACTIONS[self.action_dropdown_index].to_string(),
            KeybindActionType::Macro => {
                let text = self.macro_text.lines()[0].to_string();
                if text.is_empty() {
                    self.status_message = "Macro text cannot be empty".to_string();
                    return None;
                }
                text
            }
        };

        Some(KeybindFormResult::Save {
            key_combo,
            action_type: self.action_type.clone(),
            value,
        })
    }

    fn try_delete(&self) -> Option<KeybindFormResult> {
        if let FormMode::Edit { ref original_key } = self.mode {
            Some(KeybindFormResult::Delete {
                key_combo: original_key.clone(),
            })
        } else {
            None
        }
    }

    pub fn render(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        let width = 52;
        let height = 9;

        // Center on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(width)) / 2;
            self.popup_y = (area.height.saturating_sub(height)) / 2;
        }

        let x = self.popup_x;
        let y = self.popup_y;

        // Clear the popup area to prevent bleed-through
        let popup_area = Rect {
            x,
            y,
            width,
            height,
        };
        Clear.render(popup_area, buf);

        // Draw black background
        for row in 0..height {
            for col in 0..width {
                if x + col < area.width && y + row < area.height {
                    buf[(x + col, y + row)].set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
        }

        // Draw cyan border
        self.draw_border(x, y, width, height, buf, theme);

        // Title (left-aligned on top border)
        let title = match self.mode {
            FormMode::Create => " Add Keybind ",
            FormMode::Edit { .. } => " Edit Keybind ",
        };
        for (i, ch) in title.chars().enumerate() {
            if (x + 1 + i as u16) < (x + width) {
                buf[(x + 1 + i as u16, y)]
                    .set_char(ch)
                    .set_fg(crossterm_bridge::to_ratatui_color(theme.form_label))
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
            }
        }

        // Render fields
        self.render_fields(x, y, width, buf, config, theme);

        // Footer (centered at row 7)
        let footer = "Ctrl+s:Save Ctrl+D:Delete Esc:Cancel";
        let footer_y = y + 7;
        let footer_x = x + (width.saturating_sub(footer.len() as u16)) / 2;
        for (i, ch) in footer.chars().enumerate() {
            if (footer_x + i as u16) < (x + width) {
                buf[(footer_x + i as u16, footer_y)]
                    .set_char(ch)
                    .set_fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
            }
        }
    }

    fn draw_border(
        &self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label));

        // Top border
        buf[(x, y)].set_char('┌').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y)]
            .set_char('┐')
            .set_style(border_style);

        // Side borders
        for row in 1..height - 1 {
            buf[(x, y + row)].set_char('│').set_style(border_style);
            buf[(x + width - 1, y + row)]
                .set_char('│')
                .set_style(border_style);
        }

        // Bottom border
        buf[(x, y + height - 1)]
            .set_char('└')
            .set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y + height - 1)]
                .set_char('─')
                .set_style(border_style);
        }
        buf[(x + width - 1, y + height - 1)]
            .set_char('┘')
            .set_style(border_style);
    }

    fn render_fields(
        &mut self,
        x: u16,
        y: u16,
        _width: u16,
        buf: &mut Buffer,
        config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        let mut current_y = y + 2;

        // Parse textarea background color from config
        let textarea_bg = if config.colors.ui.textarea_background == "-" {
            Color::Reset
        } else if let Some(color) = Self::parse_hex_color(&config.colors.ui.textarea_background) {
            color
        } else {
            Color::Reset // Fallback to terminal default
        };

        // Row 2: Type (radio buttons) - Fields 0 and 1
        let type_label_color = crossterm_bridge::to_ratatui_color(if self.focused_field == 0 || self.focused_field == 1  {
            theme.form_label_focused
        } else {
            theme.form_label
        });
        let type_label = "Type:";
        for (i, ch) in type_label.chars().enumerate() {
            buf[(x + 2 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(type_label_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Action radio button (Field 0)
        let action_selected = self.action_type == KeybindActionType::Action;
        let action_color = crossterm_bridge::to_ratatui_color(if self.focused_field == 0  {
            theme.form_label_focused
        } else {
            theme.form_label
        });
        let action_text = if action_selected {
            "[X] Action"
        } else {
            "[ ] Action"
        };
        for (i, ch) in action_text.chars().enumerate() {
            buf[(x + 8 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(action_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Macro radio button (Field 1)
        let macro_selected = self.action_type == KeybindActionType::Macro;
        let macro_color = crossterm_bridge::to_ratatui_color(if self.focused_field == 1  {
            theme.form_label_focused
        } else {
            theme.form_label
        });
        let macro_text = if macro_selected {
            "[X] Macro"
        } else {
            "[ ] Macro"
        };
        for (i, ch) in macro_text.chars().enumerate() {
            buf[(x + 23 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(macro_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
        current_y += 1;

        let focused_field = self.focused_field;

        // Row 3: Key Combo (Field 2) - 1 col spacing, 37 col width
        let key_input_start = x + 2 + 10 + 1; // "Key Combo:" (10) + 1 space
        Self::render_text_row(
            focused_field,
            2,
            "Key Combo:",
            &mut self.key_combo,
            "ctrl+e, f5, alt+shift+a",
            x + 2,
            current_y,
            key_input_start,
            37,
            textarea_bg,
            buf,
            theme,
        );
        current_y += 2;

        // Row 5: Action dropdown or Macro text (4 col spacing, 37 col width)
        match self.action_type {
            KeybindActionType::Action => {
                let action_input_start = x + 2 + 7 + 4; // "Action:" (7) + 4 spaces
                self.render_action_dropdown(
                    x + 2,
                    current_y,
                    action_input_start,
                    37,
                    textarea_bg,
                    buf,
                    theme,
                );
            }
            KeybindActionType::Macro => {
                let macro_input_start = x + 2 + 11 + 4; // "Macro Text:" (11) + 4 spaces
                Self::render_text_row(
                    focused_field,
                    3,
                    "Macro Text:",
                    &mut self.macro_text,
                    "run left\\r",
                    x + 2,
                    current_y,
                    macro_input_start,
                    37,
                    textarea_bg,
                    buf,
                    theme,
                );
            }
        }
    }

    fn render_text_row(
        _focused_field: usize,
        field_id: usize,
        label: &str,
        textarea: &mut TextArea,
        _hint: &str,
        x: u16,
        y: u16,
        input_x: u16,
        input_width: u16,
        bg: Color,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let focused = _focused_field == field_id;
        let label_color = crossterm_bridge::to_ratatui_color(if focused  {
            theme.form_label_focused
        } else {
            theme.form_label
        });

        // Render label
        for (i, ch) in label.chars().enumerate() {
            buf[(x + i as u16, y)]
                .set_char(ch)
                .set_fg(label_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Create rect for the TextArea widget
        let textarea_rect = Rect {
            x: input_x,
            y,
            width: input_width,
            height: 1,
        };

        // Set block style for the textarea (no border, just background)
        let block =
            ratatui::widgets::Block::default().style(ratatui::style::Style::default().bg(bg));

        textarea.set_block(block);

        // Set text style
        textarea.set_style(
            ratatui::style::Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                .bg(bg),
        );

        // Render the TextArea widget - it handles cursor positioning and scrolling automatically
        textarea.render(textarea_rect, buf);
    }

    fn render_action_dropdown(
        &self,
        x: u16,
        y: u16,
        input_x: u16,
        input_width: u16,
        _bg: Color,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let focused = self.focused_field == 3;
        let label_color = crossterm_bridge::to_ratatui_color(if focused  {
            theme.form_label_focused
        } else {
            theme.form_label
        });

        // Render label
        let label = "Action:";
        for (i, ch) in label.chars().enumerate() {
            buf[(x + i as u16, y)]
                .set_char(ch)
                .set_fg(label_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Get current value from dropdown index
        let current_value = AVAILABLE_ACTIONS[self.action_dropdown_index];

        // Render current value (highlight if focused, no background)
        let value_color = crossterm_bridge::to_ratatui_color(if focused  {
            theme.form_label_focused
        } else {
            theme.text_disabled
        });
        for (i, ch) in current_value.chars().enumerate().take(input_width as usize) {
            buf[(input_x + i as u16, y)]
                .set_char(ch)
                .set_fg(value_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
    }

    /// Handle mouse events for dragging
    pub fn handle_mouse(&mut self, col: u16, row: u16, pressed: bool, terminal_area: Rect) -> bool {
        let popup_width = 52;
        let popup_height = 9;

        // Check if click is on title bar (top border, excluding corners)
        let on_title_bar =
            row == self.popup_y && col > self.popup_x && col < self.popup_x + popup_width - 1;

        if pressed {
            if on_title_bar && !self.is_dragging {
                // Start dragging
                self.is_dragging = true;
                self.drag_offset_x = col.saturating_sub(self.popup_x);
                self.drag_offset_y = row.saturating_sub(self.popup_y);
                return true;
            } else if self.is_dragging {
                // Continue dragging
                let new_x = col.saturating_sub(self.drag_offset_x);
                let new_y = row.saturating_sub(self.drag_offset_y);

                // Clamp to terminal bounds
                self.popup_x = new_x.min(terminal_area.width.saturating_sub(popup_width));
                self.popup_y = new_y.min(terminal_area.height.saturating_sub(popup_height));
                return true;
            }
        } else {
            // Mouse released
            if self.is_dragging {
                self.is_dragging = false;
                return true;
            }
        }

        false
    }

    /// Parse hex color string to ratatui Color
    fn parse_hex_color(hex: &str) -> Option<Color> {
        // Use centralized mode-aware color parser
        super::colors::parse_color_to_ratatui(hex)
    }

    /// Navigate to a specific action section (for keybind browser integration)
    /// This is a placeholder - actual section navigation would need to be implemented
    /// based on how the keybind browser groups actions
    pub fn go_to_section(&mut self, _section: ActionSection) {
        // Placeholder: In a full implementation, this would:
        // 1. Filter the action dropdown to only show actions in this section
        // 2. Update focused_field to the action dropdown
        // 3. Set action_dropdown_index to the first action in the section

        // For now, just focus on the action dropdown
        self.focused_field = 3;
        self.action_type = KeybindActionType::Action;
    }

    /// Get the current action section
    /// This is a placeholder that returns a default section
    pub fn get_current_section(&self) -> ActionSection {
        // Placeholder: In a full implementation, this would determine which section
        // the currently selected action belongs to
        ActionSection::CommandInput
    }
}

// Trait implementations for KeybindFormWidget
use super::widget_traits::{Cyclable, FieldNavigable, TextEditable, Toggleable};

impl TextEditable for KeybindFormWidget {
    fn get_focused_field(&self) -> Option<&TextArea<'static>> {
        match self.focused_field {
            2 => Some(&self.key_combo),
            3 if self.action_type == KeybindActionType::Macro => Some(&self.macro_text),
            _ => None,
        }
    }

    fn get_focused_field_mut(&mut self) -> Option<&mut TextArea<'static>> {
        match self.focused_field {
            2 => Some(&mut self.key_combo),
            3 if self.action_type == KeybindActionType::Macro => Some(&mut self.macro_text),
            _ => None,
        }
    }
}

impl FieldNavigable for KeybindFormWidget {
    fn next_field(&mut self) {
        self.focused_field = (self.focused_field + 1) % 4;
    }

    fn previous_field(&mut self) {
        self.focused_field = if self.focused_field == 0 {
            3
        } else {
            self.focused_field - 1
        };
    }

    fn field_count(&self) -> usize {
        4
    }

    fn current_field(&self) -> usize {
        self.focused_field
    }
}

// Implement Saveable trait for uniform form interface
impl super::widget_traits::Saveable for KeybindFormWidget {
    type SaveResult = KeybindFormResult;

    fn try_save(&mut self) -> Option<Self::SaveResult> {
        // Delegate to internal save logic
        self.save_internal()
    }
}

impl Toggleable for KeybindFormWidget {
    fn toggle_focused(&mut self) -> Option<bool> {
        match self.focused_field {
            0 => {
                self.action_type = KeybindActionType::Action;
                Some(true)
            }
            1 => {
                self.action_type = KeybindActionType::Macro;
                Some(false)
            }
            _ => None,
        }
    }
}

impl Cyclable for KeybindFormWidget {
    fn cycle_forward(&mut self) {
        if self.focused_field == 3 && self.action_type == KeybindActionType::Action {
            self.action_dropdown_index =
                (self.action_dropdown_index + 1).min(AVAILABLE_ACTIONS.len() - 1);
        }
    }

    fn cycle_backward(&mut self) {
        if self.focused_field == 3 && self.action_type == KeybindActionType::Action {
            self.action_dropdown_index = self.action_dropdown_index.saturating_sub(1);
        }
    }
}
