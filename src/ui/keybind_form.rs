use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget as RatatuiWidget},
};
use tui_textarea::TextArea;

/// Result of keybind form interaction
#[derive(Debug, Clone)]
pub enum KeybindFormResult {
    Save { key_combo: String, action_type: KeybindActionType, value: String },
    Delete { key_combo: String },
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeybindActionType {
    Action,  // Built-in action
    Macro,   // Macro text
}

/// Keybind management form widget
pub struct KeybindFormWidget {
    key_combo: TextArea<'static>,
    action_type: KeybindActionType,
    action_dropdown_index: usize,  // Index in AVAILABLE_ACTIONS
    macro_text: TextArea<'static>,

    focused_field: usize,  // 0=key_combo, 1=action_type_action, 2=action_type_macro, 3=action/macro field, 4=save, 5=cancel, 6=delete
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
            popup_x: 10,
            popup_y: 2,
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

        form.mode = FormMode::Edit { original_key: key_combo };
        form
    }

    pub fn handle_key(&mut self, key: ratatui::crossterm::event::KeyEvent) -> Option<KeybindFormResult> {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Tab => {
                // Tab through fields (with wraparound)
                let max_field = if matches!(self.mode, FormMode::Edit { .. }) { 6 } else { 5 };

                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Tab: go backwards with wraparound
                    if self.focused_field == 0 {
                        self.focused_field = max_field;
                    } else {
                        self.focused_field -= 1;
                    }
                } else {
                    // Tab: go forwards with wraparound
                    if self.focused_field >= max_field {
                        self.focused_field = 0;
                    } else {
                        self.focused_field += 1;
                    }
                }
                None
            }
            KeyCode::Esc => Some(KeybindFormResult::Cancel),
            KeyCode::Char(' ') if self.focused_field == 1 => {
                // Toggle to Action type
                self.action_type = KeybindActionType::Action;
                None
            }
            KeyCode::Char(' ') if self.focused_field == 2 => {
                // Toggle to Macro type
                self.action_type = KeybindActionType::Macro;
                None
            }
            KeyCode::Up if self.focused_field == 3 && self.action_type == KeybindActionType::Action => {
                // Scroll action dropdown up
                self.action_dropdown_index = self.action_dropdown_index.saturating_sub(1);
                None
            }
            KeyCode::Down if self.focused_field == 3 && self.action_type == KeybindActionType::Action => {
                // Scroll action dropdown down
                self.action_dropdown_index = (self.action_dropdown_index + 1).min(AVAILABLE_ACTIONS.len() - 1);
                None
            }
            KeyCode::Enter if self.focused_field == 4 => {
                // Save button
                self.try_save()
            }
            KeyCode::Enter if self.focused_field == 5 => {
                // Cancel button
                Some(KeybindFormResult::Cancel)
            }
            KeyCode::Enter if self.focused_field == 6 => {
                // Delete button
                self.try_delete()
            }
            _ => {
                // Pass to text inputs
                use tui_textarea::Input;
                let input: Input = key.into();

                let _handled = match self.focused_field {
                    0 => {
                        let result = self.key_combo.input(input.clone());
                        self.validate_key_combo();
                        result
                    }
                    3 if self.action_type == KeybindActionType::Macro => {
                        self.macro_text.input(input.clone())
                    }
                    _ => false,
                };
                None
            }
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
                "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" |
                "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z" |
                "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12" |
                "enter" | "space" | "tab" | "esc" | "backspace" | "delete" | "home" | "end" |
                "page_up" | "page_down" | "up" | "down" | "left" | "right" |
                "num_0" | "num_1" | "num_2" | "num_3" | "num_4" | "num_5" |
                "num_6" | "num_7" | "num_8" | "num_9" | "num_." | "num_+" | "num_-" | "num_*" | "num_/"
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

    fn try_save(&mut self) -> Option<KeybindFormResult> {
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
            KeybindActionType::Action => {
                AVAILABLE_ACTIONS[self.action_dropdown_index].to_string()
            }
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

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let popup_width = 80;
        let popup_height = 25;

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width.min(area.width.saturating_sub(self.popup_x)),
            height: popup_height.min(area.height.saturating_sub(self.popup_y)),
        };

        // Draw solid black background
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(' ');
                    cell.set_bg(Color::Black);
                }
            }
        }

        // Draw popup border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(match self.mode {
                FormMode::Create => " Add Keybind ",
                FormMode::Edit { .. } => " Edit Keybind ",
            });
        block.render(popup_area, buf);

        // Content area (inside borders)
        let content = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 2,
            width: popup_area.width.saturating_sub(4),
            height: popup_area.height.saturating_sub(4),
        };

        let mut y = content.y;

        // Title
        let title = match self.mode {
            FormMode::Create => "Create a new keybind",
            FormMode::Edit { .. } => "Edit keybind",
        };
        let title_para = Paragraph::new(title).style(Style::default().add_modifier(Modifier::BOLD));
        title_para.render(Rect { x: content.x, y, width: content.width, height: 1 }, buf);
        y += 2;

        // Key combo field
        Self::render_text_field(0, self.focused_field, "Key Combo:", &mut self.key_combo, content.x, y, content.width, buf);
        y += 3;

        // Show error if any
        if let Some(ref error) = self.key_combo_error {
            let error_para = Paragraph::new(error.as_str()).style(Style::default().fg(Color::Red));
            error_para.render(Rect { x: content.x + 12, y, width: content.width.saturating_sub(12), height: 1 }, buf);
            y += 1;
        }
        y += 1;

        // Action type radio buttons
        let action_label = Paragraph::new("Type:");
        action_label.render(Rect { x: content.x, y, width: 12, height: 1 }, buf);

        let action_radio = if self.action_type == KeybindActionType::Action { "[•] Action" } else { "[ ] Action" };
        let action_style = if self.focused_field == 1 {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let action_para = Paragraph::new(action_radio).style(action_style);
        action_para.render(Rect { x: content.x + 12, y, width: 15, height: 1 }, buf);

        let macro_radio = if self.action_type == KeybindActionType::Macro { "[•] Macro" } else { "[ ] Macro" };
        let macro_style = if self.focused_field == 2 {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let macro_para = Paragraph::new(macro_radio).style(macro_style);
        macro_para.render(Rect { x: content.x + 28, y, width: 15, height: 1 }, buf);
        y += 2;

        // Action dropdown or macro text field
        match self.action_type {
            KeybindActionType::Action => {
                let label = Paragraph::new("Action:");
                label.render(Rect { x: content.x, y, width: 12, height: 1 }, buf);

                // Show current action and scroll info
                let current_action = AVAILABLE_ACTIONS[self.action_dropdown_index];
                let action_text = format!("{} ({}/{})", current_action, self.action_dropdown_index + 1, AVAILABLE_ACTIONS.len());
                let action_style = if self.focused_field == 3 {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let action_para = Paragraph::new(action_text).style(action_style);
                action_para.render(Rect { x: content.x + 12, y, width: content.width.saturating_sub(12), height: 1 }, buf);
                y += 1;

                if self.focused_field == 3 {
                    let help = Paragraph::new("↑↓ to scroll through actions").style(Style::default().fg(Color::DarkGray));
                    help.render(Rect { x: content.x + 12, y, width: content.width.saturating_sub(12), height: 1 }, buf);
                }
                y += 2;
            }
            KeybindActionType::Macro => {
                Self::render_text_field(3, self.focused_field, "Macro Text:", &mut self.macro_text, content.x, y, content.width, buf);
                y += 3;

                let help = Paragraph::new("Use \\r for Enter (e.g., \"run left\\r\")").style(Style::default().fg(Color::DarkGray));
                help.render(Rect { x: content.x + 12, y, width: content.width.saturating_sub(12), height: 1 }, buf);
                y += 2;
            }
        }

        // Status message
        if !self.status_message.is_empty() {
            let status_para = Paragraph::new(self.status_message.as_str()).style(Style::default().fg(Color::Yellow));
            status_para.render(Rect { x: content.x, y, width: content.width, height: 1 }, buf);
            y += 2;
        }

        // Buttons
        self.render_buttons(content.x, y, buf);
    }

    fn render_text_field(
        field_id: usize,
        focused_field: usize,
        label: &str,
        textarea: &mut TextArea,
        x: u16,
        y: u16,
        width: u16,
        buf: &mut Buffer,
    ) {
        // Label
        let label_para = Paragraph::new(label);
        label_para.render(Rect { x, y, width: 12, height: 1 }, buf);

        // Input area - TextArea needs height 3 minimum (border + text + cursor)
        let input_area = Rect {
            x: x + 12,
            y,
            width: width.saturating_sub(12),
            height: 3,
        };

        let border_style = if focused_field == field_id {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        textarea.set_block(Block::default().borders(Borders::ALL).border_style(border_style));

        RatatuiWidget::render(&*textarea, input_area, buf);
    }

    fn render_buttons(&self, x: u16, y: u16, buf: &mut Buffer) {
        let (save_text, save_style) = if self.focused_field == 4 {
            ("[ SAVE ]", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD))
        } else {
            ("[ Save ]", Style::default().fg(Color::Green))
        };

        let (cancel_text, cancel_style) = if self.focused_field == 5 {
            ("[ CANCEL ]", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD))
        } else {
            ("[ Cancel ]", Style::default().fg(Color::Red))
        };

        let save_para = Paragraph::new(save_text).style(save_style);
        save_para.render(Rect { x, y, width: 10, height: 1 }, buf);

        let cancel_para = Paragraph::new(cancel_text).style(cancel_style);
        cancel_para.render(Rect { x: x + 11, y, width: 12, height: 1 }, buf);

        // Delete button only in edit mode
        if matches!(self.mode, FormMode::Edit { .. }) {
            let (delete_text, delete_style) = if self.focused_field == 6 {
                ("[ DELETE ]", Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD))
            } else {
                ("[ Delete ]", Style::default().fg(Color::Magenta))
            };

            let delete_para = Paragraph::new(delete_text).style(delete_style);
            delete_para.render(Rect { x: x + 24, y, width: 12, height: 1 }, buf);
        }
    }

    /// Handle mouse events for dragging
    pub fn handle_mouse(&mut self, col: u16, row: u16, pressed: bool, terminal_area: Rect) -> bool {
        let popup_width = 80;
        let popup_height = 25;

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width.min(terminal_area.width.saturating_sub(self.popup_x)),
            height: popup_height.min(terminal_area.height.saturating_sub(self.popup_y)),
        };

        // Check if click is on title bar (top border, excluding corners)
        let on_title_bar = row == popup_area.y
            && col > popup_area.x
            && col < popup_area.x + popup_area.width - 1;

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
}
