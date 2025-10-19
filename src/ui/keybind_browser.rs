use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Clear, Widget},
};
use std::collections::HashMap;

/// Keybind entry for display in browser
#[derive(Clone)]
pub struct KeybindEntry {
    pub key_combo: String,
    pub action_type: String,  // "Action" or "Macro"
    pub action_value: String,
}

pub struct KeybindBrowser {
    entries: Vec<KeybindEntry>,
    selected_index: usize,
    scroll_offset: usize,
    num_sections: usize,  // Number of section headers (for scroll calculation)

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}

impl KeybindBrowser {
    pub fn new(keybinds: &HashMap<String, crate::config::KeyBindAction>) -> Self {
        let mut entries: Vec<KeybindEntry> = keybinds
            .iter()
            .map(|(key_combo, action)| {
                let (action_type, action_value) = match action {
                    crate::config::KeyBindAction::Action(a) => {
                        ("Action".to_string(), a.clone())
                    }
                    crate::config::KeyBindAction::Macro(m) => {
                        // Escape control characters for display
                        let escaped = m.macro_text
                            .replace('\r', "\\r")
                            .replace('\n', "\\n")
                            .replace('\t', "\\t");
                        ("Macro".to_string(), escaped)
                    }
                };
                KeybindEntry {
                    key_combo: key_combo.clone(),
                    action_type,
                    action_value,
                }
            })
            .collect();

        // Sort by action type (Actions first, then Macros), then by key combo
        entries.sort_by(|a, b| {
            a.action_type.cmp(&b.action_type)
                .then_with(|| a.key_combo.cmp(&b.key_combo))
        });

        // Count sections (how many unique action types)
        let mut num_sections = 0;
        let mut last_type: Option<&str> = None;
        for entry in &entries {
            if last_type != Some(entry.action_type.as_str()) {
                num_sections += 1;
                last_type = Some(&entry.action_type);
            }
        }

        Self {
            entries,
            selected_index: 0,
            scroll_offset: 0,
            num_sections,
            popup_x: 10,
            popup_y: 2,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn previous(&mut self) {
        if !self.entries.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
        }
    }

    pub fn next(&mut self) {
        if self.selected_index + 1 < self.entries.len() {
            self.selected_index += 1;
            self.adjust_scroll();
        }
    }

    pub fn page_up(&mut self) {
        if self.selected_index >= 10 {
            self.selected_index -= 10;
        } else {
            self.selected_index = 0;
        }
        self.adjust_scroll();
    }

    pub fn page_down(&mut self) {
        if self.selected_index + 10 < self.entries.len() {
            self.selected_index += 10;
        } else if !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        }
        self.adjust_scroll();
    }

    fn adjust_scroll(&mut self) {
        // Track display rows (including section headers)
        let mut total_display_rows = 0;
        let mut last_section: Option<&str> = None;
        let mut selected_display_row = 0;

        for (idx, entry) in self.entries.iter().enumerate() {
            let entry_section = &entry.action_type;

            // Add section header if needed
            if last_section.as_deref() != Some(entry_section) {
                total_display_rows += 1;
                last_section = Some(entry_section);
            }

            if idx == self.selected_index {
                selected_display_row = total_display_rows;
            }

            total_display_rows += 1;
        }

        let visible_rows = 15; // One less than list_height for sticky headers

        if selected_display_row < self.scroll_offset {
            self.scroll_offset = selected_display_row;
        } else if selected_display_row >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected_display_row.saturating_sub(visible_rows - 1);
        }
    }

    pub fn get_selected(&self) -> Option<String> {
        self.entries.get(self.selected_index).map(|e| e.key_combo.clone())
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> bool {
        let popup_width = 70;

        // Check if mouse is on title bar
        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > self.popup_x
            && mouse_col < self.popup_x + popup_width - 1;

        if mouse_down && on_title_bar && !self.is_dragging {
            // Start dragging
            self.is_dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
            return true;
        }

        if self.is_dragging {
            if mouse_down {
                // Continue dragging
                self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
                self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);
                return true;
            } else {
                // Stop dragging
                self.is_dragging = false;
                return true;
            }
        }

        false
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let width = 70;
        let height = 20;

        let x = self.popup_x;
        let y = self.popup_y;

        // Clear the popup area to prevent bleed-through
        let popup_area = Rect { x, y, width, height };
        Clear.render(popup_area, buf);

        // Draw black background
        for row in 0..height {
            for col in 0..width {
                if x + col < area.width && y + row < area.height {
                    buf[(x + col, y + row)].set_bg(Color::Black);
                }
            }
        }

        // Draw border
        self.draw_border(x, y, width, height, buf);

        // Title (left-aligned on top border)
        let title = format!(" Keybinds ({}) ", self.entries.len());
        for (i, ch) in title.chars().enumerate() {
            if (x + 1 + i as u16) < (x + width) {
                buf[(x + 1 + i as u16, y)].set_char(ch).set_fg(Color::Cyan).set_bg(Color::Black);
            }
        }

        // Footer (off border at row 18)
        let footer = "↑/↓:Nav PgUp/PgDn:Page Enter:Edit Del:Remove Esc:Close";
        let footer_y = y + 18;
        let footer_x = x + 2;
        for (i, ch) in footer.chars().enumerate() {
            if (footer_x + i as u16) < (x + width - 2) {
                buf[(footer_x + i as u16, footer_y)].set_char(ch).set_fg(Color::White).set_bg(Color::Black);
            }
        }

        // Render entries with sticky headers
        if self.entries.is_empty() {
            let msg = "No keybinds configured";
            let msg_x = x + (width.saturating_sub(msg.len() as u16)) / 2;
            let msg_y = y + 10;
            for (i, ch) in msg.chars().enumerate() {
                buf[(msg_x + i as u16, msg_y)].set_char(ch).set_fg(Color::DarkGray).set_bg(Color::Black);
            }
            return;
        }

        let list_y = y + 1;
        let list_height = 16; // height 20 - 4 (borders + footer)
        let mut last_section: Option<&str> = None;
        let mut last_rendered_section: Option<&str> = None;
        let mut display_row = 0;
        let mut render_row = 0;
        let visible_start = self.scroll_offset;
        let visible_end = visible_start + list_height;

        for (idx, entry) in self.entries.iter().enumerate() {
            let entry_section = &entry.action_type;

            // Check if we need a section header
            if last_section.as_deref() != Some(entry_section) {
                // Always increment display_row for the header
                if display_row >= visible_start {
                    // Render header if in visible range AND we have room
                    if display_row < visible_end && render_row < list_height {
                        let current_y = list_y + render_row as u16;
                        let header_text = if entry_section == "Action" {
                            " ═══ ACTIONS ═══"
                        } else {
                            " ═══ MACROS ═══"
                        };
                        let header_style = Style::default().fg(Color::Rgb(255, 215, 0)).bg(Color::Black).add_modifier(Modifier::BOLD);
                        for (i, ch) in header_text.chars().enumerate() {
                            if (x + 1 + i as u16) < (x + width - 1) {
                                buf[(x + 1 + i as u16, current_y)].set_char(ch).set_style(header_style);
                            }
                        }
                        render_row += 1;
                        last_rendered_section = Some(entry_section);
                    }
                }
                display_row += 1;
                last_section = Some(entry_section);
            }

            // Skip if before visible range
            if display_row < visible_start {
                display_row += 1;
                continue;
            }

            // If this is a new section in the visible area and we haven't rendered its header yet (sticky header)
            if last_rendered_section.as_deref() != Some(entry_section) && render_row < list_height {
                let current_y = list_y + render_row as u16;
                let header_text = if entry_section == "Action" {
                    " ═══ ACTIONS ═══"
                } else {
                    " ═══ MACROS ═══"
                };
                let header_style = Style::default().fg(Color::Rgb(255, 215, 0)).bg(Color::Black).add_modifier(Modifier::BOLD);
                for (i, ch) in header_text.chars().enumerate() {
                    if (x + 1 + i as u16) < (x + width - 1) {
                        buf[(x + 1 + i as u16, current_y)].set_char(ch).set_style(header_style);
                    }
                }
                render_row += 1;
                last_rendered_section = Some(entry_section);
            }

            // Stop if past visible range OR no room for entry
            if display_row >= visible_end || render_row >= list_height {
                break;
            }

            let is_selected = idx == self.selected_index;
            let current_y = list_y + render_row as u16;

            // Format: [key_combo] type: value
            let entry_text = format!("[{}] {}: {}", entry.key_combo, entry.action_type, entry.action_value);
            let available_width = (width - 2) as usize; // Account for padding
            let display_text = if entry_text.len() > available_width {
                format!("{}...", &entry_text[..available_width.saturating_sub(3)])
            } else {
                entry_text
            };

            let entry_color = if is_selected { Color::Rgb(255, 215, 0) } else { Color::Cyan };

            // Render entry text
            for (i, ch) in display_text.chars().enumerate() {
                if (x + 1 + i as u16) < (x + width - 1) {
                    buf[(x + 1 + i as u16, current_y)].set_char(ch).set_fg(entry_color).set_bg(Color::Black);
                }
            }

            display_row += 1;
            render_row += 1;
        }
    }

    fn draw_border(&self, x: u16, y: u16, width: u16, height: u16, buf: &mut Buffer) {
        let border_style = Style::default().fg(Color::Cyan);

        // Top border
        buf[(x, y)].set_char('┌').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y)].set_char('┐').set_style(border_style);

        // Side borders
        for row in 1..height - 1 {
            buf[(x, y + row)].set_char('│').set_style(border_style);
            buf[(x + width - 1, y + row)].set_char('│').set_style(border_style);
        }

        // Bottom border
        buf[(x, y + height - 1)].set_char('└').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y + height - 1)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y + height - 1)].set_char('┘').set_style(border_style);
    }
}
