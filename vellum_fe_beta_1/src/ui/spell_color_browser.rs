use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Clear, Widget},
};
use crate::config::SpellColorRange;

pub struct SpellColorEntry {
    pub index: usize,        // Index in config.spell_colors
    pub spells: Vec<u32>,
    pub bar_color: String,
    #[allow(unused)]
    pub text_color: String,
    pub bg_color: String,
}

pub struct SpellColorBrowser {
    entries: Vec<SpellColorEntry>,
    selected_index: usize,
    scroll_offset: usize,
    popup_position: (u16, u16),
    #[allow(unused)]
    pub is_dragging: bool,
    #[allow(unused)]
    drag_offset: (i16, i16),
}

impl SpellColorBrowser {
    pub fn new(spell_colors: &[SpellColorRange]) -> Self {
        let entries = spell_colors.iter().enumerate().map(|(index, sc)| {
            SpellColorEntry {
                index,
                spells: sc.spells.clone(),
                bar_color: sc.bar_color.clone().unwrap_or_else(|| sc.color.clone()),
                text_color: sc.text_color.clone().unwrap_or_else(|| "#ffffff".to_string()),
                bg_color: sc.bg_color.clone().unwrap_or_else(|| String::new()),
            }
        }).collect();

        Self {
            entries,
            selected_index: 0,
            scroll_offset: 0,
            popup_position: (0, 0),
            is_dragging: false,
            drag_offset: (0, 0),
        }
    }

    pub fn previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            // Scroll up if needed
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn next(&mut self) {
        if self.selected_index < self.entries.len().saturating_sub(1) {
            self.selected_index += 1;
            // Scroll down if needed
            let visible_rows = 15; // Max visible entries
            if self.selected_index >= self.scroll_offset + visible_rows {
                self.scroll_offset = self.selected_index - visible_rows + 1;
            }
        }
    }

    pub fn page_up(&mut self) {
        let page_size = 15;
        self.selected_index = self.selected_index.saturating_sub(page_size);
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    pub fn page_down(&mut self) {
        let page_size = 15;
        let max_index = self.entries.len().saturating_sub(1);
        self.selected_index = (self.selected_index + page_size).min(max_index);
        let visible_rows = 15;
        if self.selected_index >= self.scroll_offset + visible_rows {
            self.scroll_offset = self.selected_index - visible_rows + 1;
        }
    }

    pub fn get_selected(&self) -> Option<usize> {
        if self.selected_index < self.entries.len() {
            Some(self.entries[self.selected_index].index)
        } else {
            None
        }
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> bool {
        let (col, row) = (event.column, event.row);
        let (popup_col, popup_row) = self.popup_position;
        let popup_width = 70;
        let popup_height = 20;

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if clicking in title bar (for dragging)
                if row == popup_row
                    && col > popup_col
                    && col < popup_col + popup_width - 1
                {
                    self.is_dragging = true;
                    self.drag_offset = (col as i16 - popup_col as i16, row as i16 - popup_row as i16);
                    return true;
                }

                // Check if clicking on an entry
                if row > popup_row + 1 && row < popup_row + popup_height - 2
                    && col > popup_col && col < popup_col + popup_width - 1
                {
                    let clicked_index = (row - popup_row - 2) as usize + self.scroll_offset;
                    if clicked_index < self.entries.len() {
                        self.selected_index = clicked_index;
                        return true;
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.is_dragging {
                    let new_col = (col as i16 - self.drag_offset.0).max(0) as u16;
                    let new_row = (row as i16 - self.drag_offset.1).max(0) as u16;
                    self.popup_position = (
                        new_col.min(area.width.saturating_sub(popup_width)),
                        new_row.min(area.height.saturating_sub(popup_height)),
                    );
                    return true;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.is_dragging {
                    self.is_dragging = false;
                    return true;
                }
            }
            MouseEventKind::ScrollUp => {
                if row >= popup_row && row < popup_row + popup_height
                    && col >= popup_col && col < popup_col + popup_width
                {
                    self.previous();
                    return true;
                }
            }
            MouseEventKind::ScrollDown => {
                if row >= popup_row && row < popup_row + popup_height
                    && col >= popup_col && col < popup_col + popup_width
                {
                    self.next();
                    return true;
                }
            }
            _ => {}
        }

        false
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let popup_width = 70;
        let popup_height = 20;

        // Center on first render
        if self.popup_position == (0, 0) {
            let centered_x = (area.width.saturating_sub(popup_width)) / 2;
            let centered_y = (area.height.saturating_sub(popup_height)) / 2;
            self.popup_position = (centered_x, centered_y);
        }

        let (popup_col, popup_row) = self.popup_position;

        // Clear the popup area to prevent bleed-through
        let popup_area = Rect {
            x: popup_col,
            y: popup_row,
            width: popup_width,
            height: popup_height,
        };
        Clear.render(popup_area, buf);

        // Draw black background
        for row in popup_row..popup_row + popup_height {
            for col in popup_col..popup_col + popup_width {
                if col < area.width && row < area.height {
                    buf.set_string(col, row, " ", Style::default().bg(Color::Black));
                }
            }
        }

        // Draw border
        let border_style = Style::default().fg(Color::Cyan);

        // Top border
        let top = format!("┌{}┐", "─".repeat(popup_width as usize - 2));
        buf.set_string(popup_col, popup_row, &top, border_style);

        // Title
        buf.set_string(popup_col + 2, popup_row, " Spell Colors ", border_style.add_modifier(Modifier::BOLD));

        // Side borders
        for i in 1..popup_height - 1 {
            buf.set_string(popup_col, popup_row + i, "│", border_style);
            buf.set_string(popup_col + popup_width - 1, popup_row + i, "│", border_style);
        }

        // Bottom border
        let bottom = format!("└{}┘", "─".repeat(popup_width as usize - 2));
        buf.set_string(popup_col, popup_row + popup_height - 1, &bottom, border_style);

        // Render entries
        let visible_rows = popup_height - 4; // Leave room for borders and status
        let visible_entries = self.entries.iter()
            .skip(self.scroll_offset)
            .take(visible_rows as usize);

        let mut y = popup_row + 2;
        for (offset, entry) in visible_entries.enumerate() {
            let is_selected = self.scroll_offset + offset == self.selected_index;
            self.render_entry(entry, popup_col + 2, y, popup_width - 4, is_selected, buf);
            y += 1;
        }

        // Status bar
        let total = self.entries.len();
        let current = if total == 0 { 0 } else { (self.selected_index + 1).min(total) };
        let spacer = "                 "; // 17 spaces to align with color browser
        let status = format!(" ↑/↓:Nav  Enter:Edit  Del:Del  {} Esc:Close  ({}/{}) ", spacer, current, total);
        buf.set_string(popup_col + 2, popup_row + popup_height - 2, &status, Style::default().fg(Color::Gray));
    }

    fn render_entry(&self, entry: &SpellColorEntry, x: u16, y: u16, width: u16, is_selected: bool, buf: &mut Buffer) {
        let base_style = if is_selected {
            Style::default().fg(Color::Yellow).bg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(100, 149, 237)).bg(Color::Black)
        };

        // Format: bar(3) + 2 spaces + bg(3) + 2 spaces + details
        let mut col = x;
        // Bar color preview: 3 full blocks or " - " if empty/invalid
        if let Some(color) = if !entry.bar_color.is_empty() { self.parse_color(&entry.bar_color) } else { None } {
            buf.set_string(col, y, "███", Style::default().fg(color).bg(Color::Black));
        } else {
            buf.set_string(col, y, " - ", base_style);
        }
        col += 3;
        buf.set_string(col, y, "  ", base_style); // 2 spaces between previews
        col += 2;
        // Background color preview: 3 full blocks or " - " if empty/invalid
        if let Some(color) = if !entry.bg_color.is_empty() { self.parse_color(&entry.bg_color) } else { None } {
            buf.set_string(col, y, "███", Style::default().fg(color).bg(Color::Black));
        } else {
            buf.set_string(col, y, " - ", base_style);
        }
        col += 3;
        buf.set_string(col, y, "  ", base_style); // 2 spaces before details
        col += 2;

        // Spell IDs (rest of the line)
        let spells_str = entry.spells.iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let spells_display = format!(" [{}]", spells_str);
        let used_cols = 3 + 2 + 3 + 2; // previews + spaces = 10
        let available_width = width.saturating_sub(used_cols as u16) as usize;
        let truncated = if spells_display.len() > available_width as usize {
            format!("{}...", &spells_display[..available_width.saturating_sub(3) as usize])
        } else {
            spells_display
        };

        buf.set_string(col, y, &truncated, base_style);
    }

    fn parse_color(&self, hex: &str) -> Option<Color> {
        if hex.len() != 7 || !hex.starts_with('#') {
            return None;
        }
        let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
        let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
        let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    }
}


