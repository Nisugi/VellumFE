use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
};
use std::collections::HashMap;

/// Highlight entry for display in browser
#[derive(Clone)]
pub struct HighlightEntry {
    pub name: String,
    pub pattern: String,
    pub category: Option<String>,
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub has_sound: bool,
}

pub struct HighlightBrowser {
    entries: Vec<HighlightEntry>,
    selected_index: usize,
    scroll_offset: usize,
    category_filter: Option<String>,  // Filter by category

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}

impl HighlightBrowser {
    pub fn new(highlights: &HashMap<String, crate::config::HighlightPattern>) -> Self {
        let mut entries: Vec<HighlightEntry> = highlights
            .iter()
            .map(|(name, pattern)| HighlightEntry {
                name: name.clone(),
                pattern: pattern.pattern.clone(),
                category: pattern.category.clone(),
                fg: pattern.fg.clone(),
                bg: pattern.bg.clone(),
                has_sound: pattern.sound.is_some(),
            })
            .collect();

        // Sort by category, then by name
        entries.sort_by(|a, b| {
            match (&a.category, &b.category) {
                (Some(cat_a), Some(cat_b)) => {
                    cat_a.cmp(cat_b).then_with(|| a.name.cmp(&b.name))
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.name.cmp(&b.name),
            }
        });

        Self {
            entries,
            selected_index: 0,
            scroll_offset: 0,
            category_filter: None,
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn set_category_filter(&mut self, category: Option<String>) {
        self.category_filter = category;
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    fn filtered_entries(&self) -> Vec<&HighlightEntry> {
        if let Some(ref filter) = self.category_filter {
            self.entries
                .iter()
                .filter(|e| e.category.as_ref() == Some(filter))
                .collect()
        } else {
            self.entries.iter().collect()
        }
    }

    pub fn previous(&mut self) {
        let filtered = self.filtered_entries();
        if !filtered.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
        }
    }

    pub fn next(&mut self) {
        let filtered = self.filtered_entries();
        if self.selected_index + 1 < filtered.len() {
            self.selected_index += 1;
            self.adjust_scroll();
        }
    }

    pub fn page_up(&mut self) {
        let visible_height: usize = 20; // Approximate
        let jump = visible_height.saturating_sub(1).max(1);
        self.selected_index = self.selected_index.saturating_sub(jump);
        self.adjust_scroll();
    }

    pub fn page_down(&mut self) {
        let filtered = self.filtered_entries();
        let visible_height: usize = 20; // Approximate
        let jump = visible_height.saturating_sub(1).max(1);
        self.selected_index = (self.selected_index + jump).min(filtered.len().saturating_sub(1));
        self.adjust_scroll();
    }

    fn adjust_scroll(&mut self) {
        let visible_height: usize = 20; // Approximate
        // Ensure selected item is visible
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected_index.saturating_sub(visible_height - 1);
        }
    }

    pub fn get_selected(&self) -> Option<String> {
        let filtered = self.filtered_entries();
        filtered.get(self.selected_index).map(|e| e.name.clone())
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> bool {
        let popup_width = 80.min(area.width);

        // Check if mouse is on title bar
        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > self.popup_x
            && mouse_col < self.popup_x + popup_width - 1;

        if mouse_down && on_title_bar && !self.is_dragging {
            self.is_dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
            return true;
        }

        if self.is_dragging {
            if mouse_down {
                self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
                self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);
                return true;
            } else {
                self.is_dragging = false;
                return true;
            }
        }

        false
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, config: &crate::config::Config) {
        // Calculate popup size
        let popup_width = 80.min(area.width);
        let popup_height = 30.min(area.height);

        // Center popup initially
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(popup_width)) / 2;
            self.popup_y = (area.height.saturating_sub(popup_height)) / 2;
        }

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Fill background with solid black
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < area.width && y < area.height {
                    buf[(x, y)].set_char(' ').set_bg(Color::Black);
                }
            }
        }

        // Draw border
        let border_color = Color::Cyan;
        self.draw_border(&popup_area, buf, border_color);

        // Draw title
        let title = " Highlight Browser ";
        let title_x = popup_area.x + (popup_area.width.saturating_sub(title.len() as u16)) / 2;
        for (i, ch) in title.chars().enumerate() {
            if let Some(x) = title_x.checked_add(i as u16) {
                if x < popup_area.x + popup_area.width {
                    buf[(x, popup_area.y)].set_char(ch).set_fg(Color::Cyan).set_bg(Color::Black);
                }
            }
        }

        // Draw instructions at bottom
        let instructions = "↑/↓: Navigate | Enter: Edit | Delete: Remove | PgUp/PgDn: Scroll | Esc: Close";
        let instr_y = popup_area.y + popup_area.height - 1;
        let instr_x = popup_area.x + 2;
        for (i, ch) in instructions.chars().enumerate() {
            if let Some(x) = instr_x.checked_add(i as u16) {
                if x < popup_area.x + popup_area.width - 1 {
                    buf[(x, instr_y)].set_char(ch).set_fg(Color::DarkGray).set_bg(Color::Black);
                }
            }
        }

        // Inner content area
        let content_x = popup_area.x + 1;
        let content_y = popup_area.y + 1;
        let content_width = popup_area.width.saturating_sub(2);
        let content_height = popup_area.height.saturating_sub(3);

        // Render highlights list
        let filtered = self.filtered_entries();
        let mut current_category = String::new();
        let mut y = content_y;

        for (display_idx, entry) in filtered.iter().enumerate().skip(self.scroll_offset) {
            if y >= content_y + content_height {
                break;
            }

            // Render category header if changed
            let entry_category = entry.category.as_ref().map(|s| s.as_str()).unwrap_or("Uncategorized");
            if entry_category != current_category {
                current_category = entry_category.to_string();

                let header = format!("[{}]", entry_category);
                for (i, ch) in header.chars().enumerate() {
                    if let Some(x) = content_x.checked_add(i as u16) {
                        if x < content_x + content_width {
                            let mut cell = buf[(x, y)].clone();
                            cell.set_char(ch).set_fg(Color::Yellow).set_bg(Color::Black);
                            cell.modifier.insert(Modifier::BOLD);
                            buf[(x, y)] = cell;
                        }
                    }
                }
                y += 1;
                if y >= content_y + content_height {
                    break;
                }
            }

            // Determine if this item is selected
            let is_selected = display_idx == self.selected_index;

            // Build display name with sound indicator
            let sound_indicator = if entry.has_sound { " \u{266B}" } else { "" };
            let name_with_sound = format!("{}{}", entry.name, sound_indicator);

            // Style based on selection
            let (text_fg, text_bg) = if is_selected {
                (Color::Black, Color::Cyan)
            } else {
                (Color::White, Color::Black)
            };

            let mut x_pos = content_x;

            // Render indent
            for ch in "  ".chars() {
                if x_pos < content_x + content_width {
                    buf[(x_pos, y)].set_char(ch).set_fg(text_fg).set_bg(text_bg);
                    x_pos += 1;
                }
            }

            // Render foreground color preview (3 blocks)
            if let Some(ref fg_color) = entry.fg {
                let resolved_fg = config.resolve_color(fg_color);
                if let Some(hex_fg) = resolved_fg {
                    if let Some(color) = Self::parse_hex_color(&hex_fg) {
                        for _ in 0..3 {
                            if x_pos < content_x + content_width {
                                buf[(x_pos, y)].set_char('█').set_fg(color).set_bg(text_bg);
                                x_pos += 1;
                            }
                        }
                    }
                }
            }

            // Space between fg and bg
            if x_pos < content_x + content_width {
                buf[(x_pos, y)].set_char(' ').set_fg(text_fg).set_bg(text_bg);
                x_pos += 1;
            }

            // Render background color preview (3 blocks)
            if let Some(ref bg_color) = entry.bg {
                let resolved_bg = config.resolve_color(bg_color);
                if let Some(hex_bg) = resolved_bg {
                    if let Some(color) = Self::parse_hex_color(&hex_bg) {
                        for _ in 0..3 {
                            if x_pos < content_x + content_width {
                                buf[(x_pos, y)].set_char('█').set_fg(color).set_bg(text_bg);
                                x_pos += 1;
                            }
                        }
                    }
                }
            }

            // Space before name
            if x_pos < content_x + content_width {
                buf[(x_pos, y)].set_char(' ').set_fg(text_fg).set_bg(text_bg);
                x_pos += 1;
            }

            // Render name (no truncation, just fill to end of available space)
            for ch in name_with_sound.chars() {
                if x_pos < content_x + content_width {
                    buf[(x_pos, y)].set_char(ch).set_fg(text_fg).set_bg(text_bg);
                    x_pos += 1;
                } else {
                    break;
                }
            }

            y += 1;
        }

        // Show scroll indicator if needed
        if filtered.len() > content_height as usize {
            let scroll_info = format!("{}/{}", self.selected_index + 1, filtered.len());
            let scroll_x = popup_area.x + popup_area.width.saturating_sub(scroll_info.len() as u16 + 2);
            let scroll_y = popup_area.y;
            for (i, ch) in scroll_info.chars().enumerate() {
                if let Some(x) = scroll_x.checked_add(i as u16) {
                    if x < popup_area.x + popup_area.width - 1 {
                        buf[(x, scroll_y)].set_char(ch).set_fg(Color::Cyan).set_bg(Color::Black);
                    }
                }
            }
        }
    }

    fn draw_border(&self, area: &Rect, buf: &mut Buffer, color: Color) {
        // Top and bottom borders
        for x in area.x..area.x + area.width {
            if x < buf.area.width {
                if area.y < buf.area.height {
                    buf[(x, area.y)].set_char('─').set_fg(color).set_bg(Color::Black);
                }
                let bottom_y = area.y + area.height - 1;
                if bottom_y < buf.area.height {
                    buf[(x, bottom_y)].set_char('─').set_fg(color).set_bg(Color::Black);
                }
            }
        }

        // Left and right borders
        for y in area.y..area.y + area.height {
            if y < buf.area.height {
                if area.x < buf.area.width {
                    buf[(area.x, y)].set_char('│').set_fg(color).set_bg(Color::Black);
                }
                let right_x = area.x + area.width - 1;
                if right_x < buf.area.width {
                    buf[(right_x, y)].set_char('│').set_fg(color).set_bg(Color::Black);
                }
            }
        }

        // Corners
        if area.x < buf.area.width && area.y < buf.area.height {
            buf[(area.x, area.y)].set_char('┌').set_fg(color).set_bg(Color::Black);
        }
        let top_right_x = area.x + area.width - 1;
        if top_right_x < buf.area.width && area.y < buf.area.height {
            buf[(top_right_x, area.y)].set_char('┐').set_fg(color).set_bg(Color::Black);
        }
        let bottom_left_y = area.y + area.height - 1;
        if area.x < buf.area.width && bottom_left_y < buf.area.height {
            buf[(area.x, bottom_left_y)].set_char('└').set_fg(color).set_bg(Color::Black);
        }
        let bottom_right_x = area.x + area.width - 1;
        let bottom_right_y = area.y + area.height - 1;
        if bottom_right_x < buf.area.width && bottom_right_y < buf.area.height {
            buf[(bottom_right_x, bottom_right_y)].set_char('┘').set_fg(color).set_bg(Color::Black);
        }
    }

    fn parse_hex_color(hex: &str) -> Option<Color> {
        if !hex.starts_with('#') || hex.len() != 7 {
            return None;
        }
        let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
        let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
        let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    }
}
