use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
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
        // Account for section headers in visible rows
        // Each section header takes 1 line, so available lines = 20 - num_sections
        let list_height: usize = 20;
        let available_rows = list_height.saturating_sub(self.num_sections);

        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if available_rows > 0 && self.selected_index >= self.scroll_offset + available_rows {
            self.scroll_offset = self.selected_index.saturating_sub(available_rows - 1);
        }
    }

    pub fn get_selected(&self) -> Option<String> {
        self.entries.get(self.selected_index).map(|e| e.key_combo.clone())
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> bool {
        let popup_width = 80.min(area.width);

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

        // Draw border
        let border_style = Style::default().fg(Color::Cyan);
        self.draw_border(popup_area, buf, border_style);

        // Draw title
        let title = format!(" Keybinds ({}) ", self.entries.len());
        let title_x = popup_area.x + 2;
        if title_x < popup_area.x + popup_area.width {
            for (i, ch) in title.chars().enumerate() {
                let x = title_x + i as u16;
                if x >= popup_area.x + popup_area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, popup_area.y)) {
                    cell.set_char(ch);
                    cell.set_fg(Color::Yellow);
                    cell.set_bg(Color::Black);
                }
            }
        }

        // Draw help text
        let help = " ↑/↓:Navigate  Enter:Edit  Del:Remove  Esc:Close ";
        let help_x = popup_area.x + popup_area.width.saturating_sub(help.len() as u16 + 1);
        if help_x > popup_area.x {
            for (i, ch) in help.chars().enumerate() {
                let x = help_x + i as u16;
                if x >= popup_area.x + popup_area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, popup_area.y + popup_area.height - 1)) {
                    cell.set_char(ch);
                    cell.set_fg(Color::Gray);
                    cell.set_bg(Color::Black);
                }
            }
        }

        // Draw keybinds list
        let list_area = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 2,
            width: popup_area.width.saturating_sub(4),
            height: popup_area.height.saturating_sub(4),
        };

        if self.entries.is_empty() {
            // Show "No keybinds" message
            let msg = "No keybinds configured";
            let x = list_area.x + (list_area.width.saturating_sub(msg.len() as u16)) / 2;
            let y = list_area.y + list_area.height / 2;
            for (i, ch) in msg.chars().enumerate() {
                if let Some(cell) = buf.cell_mut((x + i as u16, y)) {
                    cell.set_char(ch);
                    cell.set_fg(Color::Gray);
                    cell.set_bg(Color::Black);
                }
            }
            return;
        }

        // Column widths
        let key_col_width = 18;
        let type_col_width = 8;
        let value_col_start = key_col_width + type_col_width;

        // Track current display position
        let mut display_index = 0;
        let mut current_y = list_area.y;
        let mut last_type: Option<&str> = None;

        // Iterate through entries and render with section headers
        for (abs_idx, entry) in self.entries.iter().enumerate() {
            // Check if we need a section header
            if last_type.is_none() || last_type != Some(entry.action_type.as_str()) {
                // Skip section header if we're still scrolled past it
                if display_index >= self.scroll_offset {
                    if current_y >= list_area.y + list_area.height {
                        break;
                    }

                    // Draw section header
                    let header = if entry.action_type == "Action" {
                        "═══ ACTIONS ═══"
                    } else {
                        "═══ MACROS ═══"
                    };

                    let header_style = Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD);

                    for (i, ch) in header.chars().enumerate() {
                        let x = list_area.x + i as u16;
                        if x >= list_area.x + list_area.width {
                            break;
                        }
                        if let Some(cell) = buf.cell_mut((x, current_y)) {
                            cell.set_char(ch);
                            cell.set_style(header_style);
                        }
                    }
                    current_y += 1;
                }
                display_index += 1;
                last_type = Some(&entry.action_type);
            }

            // Check if this entry should be displayed
            if display_index < self.scroll_offset {
                display_index += 1;
                continue;
            }

            if current_y >= list_area.y + list_area.height {
                break;
            }

            let is_selected = abs_idx == self.selected_index;

            // Format with columns: [key_combo] type  value
            let key_part = format!("[{}]", entry.key_combo);
            let key_padded = format!("{:<width$}", key_part, width = key_col_width);

            let type_padded = format!("{:<width$}", entry.action_type, width = type_col_width);

            // Calculate available space for value
            let available_value_width = list_area.width.saturating_sub(value_col_start as u16) as usize;
            let value_truncated = if entry.action_value.len() > available_value_width {
                format!("{}...", &entry.action_value[..available_value_width.saturating_sub(3)])
            } else {
                entry.action_value.clone()
            };

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White).bg(Color::Black)
            };

            // Render key column
            for (i, ch) in key_padded.chars().take(key_col_width).enumerate() {
                let x = list_area.x + i as u16;
                if let Some(cell) = buf.cell_mut((x, current_y)) {
                    cell.set_char(ch);
                    cell.set_style(style);
                }
            }

            // Render type column
            for (i, ch) in type_padded.chars().take(type_col_width).enumerate() {
                let x = list_area.x + key_col_width as u16 + i as u16;
                if let Some(cell) = buf.cell_mut((x, current_y)) {
                    cell.set_char(ch);
                    cell.set_style(style);
                }
            }

            // Render value column
            for (i, ch) in value_truncated.chars().enumerate() {
                let x = list_area.x + value_col_start as u16 + i as u16;
                if x >= list_area.x + list_area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, current_y)) {
                    cell.set_char(ch);
                    cell.set_style(style);
                }
            }

            // Fill rest of line with background if selected
            if is_selected {
                for x in (list_area.x + value_col_start as u16 + value_truncated.len() as u16)..(list_area.x + list_area.width) {
                    if let Some(cell) = buf.cell_mut((x, current_y)) {
                        cell.set_char(' ');
                        cell.set_bg(Color::Cyan);
                    }
                }
            }

            current_y += 1;
            display_index += 1;
        }
    }

    fn draw_border(&self, area: Rect, buf: &mut Buffer, style: Style) {
        // Top border
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                if x == area.x {
                    cell.set_char('┌');
                } else if x == area.x + area.width - 1 {
                    cell.set_char('┐');
                } else {
                    cell.set_char('─');
                }
                cell.set_style(style);
            }
        }

        // Bottom border
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, area.y + area.height - 1)) {
                if x == area.x {
                    cell.set_char('└');
                } else if x == area.x + area.width - 1 {
                    cell.set_char('┘');
                } else {
                    cell.set_char('─');
                }
                cell.set_style(style);
            }
        }

        // Left border
        for y in area.y + 1..area.y + area.height - 1 {
            if let Some(cell) = buf.cell_mut((area.x, y)) {
                cell.set_char('│');
                cell.set_style(style);
            }
        }

        // Right border
        for y in area.y + 1..area.y + area.height - 1 {
            if let Some(cell) = buf.cell_mut((area.x + area.width - 1, y)) {
                cell.set_char('│');
                cell.set_style(style);
            }
        }
    }
}
