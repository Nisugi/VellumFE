use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Clear, Widget},
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
    pub squelch: bool,
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
                squelch: pattern.squelch,
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
        if self.selected_index >= 10 {
            self.selected_index -= 10;
        } else {
            self.selected_index = 0;
        }
        self.adjust_scroll();
    }

    pub fn page_down(&mut self) {
        let filtered = self.filtered_entries();
        if self.selected_index + 10 < filtered.len() {
            self.selected_index += 10;
        } else if !filtered.is_empty() {
            self.selected_index = filtered.len() - 1;
        }
        self.adjust_scroll();
    }

    fn adjust_scroll(&mut self) {
        // Calculate total display rows including category headers
        let filtered = self.filtered_entries();
        let mut total_display_rows = 0;
        let mut last_category: Option<&str> = None;
        let mut selected_display_row = 0;

        for (idx, entry) in filtered.iter().enumerate() {
            let entry_category = entry.category.as_ref().map(|s| s.as_str()).unwrap_or("Uncategorized");

            // Add category header row if category changes
            if last_category != Some(entry_category) {
                total_display_rows += 1;
                last_category = Some(entry_category);
            }

            // Track which display row the selected item is on
            if idx == self.selected_index {
                selected_display_row = total_display_rows;
            }

            total_display_rows += 1;
        }

        let visible_rows = 15; // One less than list_height to account for sticky headers

        // Adjust scroll to keep selected item in view
        if selected_display_row < self.scroll_offset {
            self.scroll_offset = selected_display_row;
        } else if selected_display_row >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected_display_row.saturating_sub(visible_rows - 1);
        }
    }

    pub fn get_selected(&self) -> Option<String> {
        let filtered = self.filtered_entries();
        filtered.get(self.selected_index).map(|e| e.name.clone())
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> bool {
        let popup_width = 70.min(area.width);

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
        let width = 70;
        let height = 20;

        // Center popup initially
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
                    buf[(x + col, y + row)].set_bg(Color::Black);
                }
            }
        }

        // Draw cyan border
        let border_color = Color::Cyan;
        self.draw_border(&Rect { x, y, width, height }, buf, border_color);

        // Title (left-aligned)
        let title = " Highlight Browser ";
        for (i, ch) in title.chars().enumerate() {
            if (x + 1 + i as u16) < (x + width) {
                buf[(x + 1 + i as u16, y)].set_char(ch).set_fg(Color::Cyan).set_bg(Color::Black);
            }
        }

        // Render entries with display_row tracking
        let list_y = y + 1;
        let list_height = 16; // height 20 - 4 (borders + footer)
        let filtered = self.filtered_entries();
        let mut last_category: Option<&str> = None;
        let mut last_rendered_category: Option<&str> = None;
        let mut display_row = 0;
        let mut render_row = 0;
        let visible_start = self.scroll_offset;
        let visible_end = visible_start + list_height;

        for (idx, entry) in filtered.iter().enumerate() {
            let entry_category = entry.category.as_ref().map(|s| s.as_str()).unwrap_or("Uncategorized");

            // Check if we need a category header
            if last_category != Some(entry_category) {
                // Always increment display_row for the header
                if display_row >= visible_start {
                    // Header is in visible range or we're past it
                    if display_row < visible_end && render_row < list_height {
                        // Render the header
                        let current_y = list_y + render_row as u16;
                        let header_text = format!(" ═══ {} ═══", entry_category.to_uppercase());
                        let header_style = ratatui::style::Style::default()
                            .fg(Color::Rgb(255, 215, 0)) // Gold
                            .bg(Color::Black)
                            .add_modifier(Modifier::BOLD);

                        for (i, ch) in header_text.chars().enumerate() {
                            if i < (width - 2) as usize {
                                buf[(x + 1 + i as u16, current_y)].set_char(ch).set_style(header_style);
                            }
                        }

                        // Fill rest of line with spaces
                        for i in header_text.len()..(width - 2) as usize {
                            buf[(x + 1 + i as u16, current_y)].set_char(' ').set_bg(Color::Black);
                        }

                        render_row += 1;
                        last_rendered_category = Some(entry_category);
                    }
                }
                display_row += 1;
                last_category = Some(entry_category);
            }

            // Skip if before visible range
            if display_row < visible_start {
                display_row += 1;
                continue;
            }

            // If this is a new category in the visible area and we haven't rendered its header yet
            if last_rendered_category != Some(entry_category) && render_row < list_height {
                // Render sticky header for this category
                let current_y = list_y + render_row as u16;
                let header_text = format!(" ═══ {} ═══", entry_category.to_uppercase());
                let header_style = ratatui::style::Style::default()
                    .fg(Color::Rgb(255, 215, 0)) // Gold
                    .bg(Color::Black)
                    .add_modifier(Modifier::BOLD);

                for (i, ch) in header_text.chars().enumerate() {
                    if i < (width - 2) as usize {
                        buf[(x + 1 + i as u16, current_y)].set_char(ch).set_style(header_style);
                    }
                }

                // Fill rest of line with spaces
                for i in header_text.len()..(width - 2) as usize {
                    buf[(x + 1 + i as u16, current_y)].set_char(' ').set_bg(Color::Black);
                }

                render_row += 1;
                last_rendered_category = Some(entry_category);
            }

            // Stop if past visible range OR no room for entry
            if display_row >= visible_end || render_row >= list_height {
                break;
            }

            // Render entry row (with 1 col padding from left border)
            let current_y = list_y + render_row as u16;
            let is_selected = idx == self.selected_index;

            // Col 2-4: FG color preview
            if let Some(ref fg_color) = entry.fg {
                let resolved_fg = config.resolve_color(fg_color);
                if let Some(hex_fg) = resolved_fg {
                    if let Some(color) = Self::parse_hex_color(&hex_fg) {
                        for i in 0..3 {
                            buf[(x + 2 + i, current_y)].set_char(' ').set_bg(color);
                        }
                    }
                }
            } else {
                // No color: show [-]
                buf[(x + 3, current_y)].set_char('-').set_fg(Color::Gray).set_bg(Color::Black);
            }

            // Col 7-9: BG color preview
            if let Some(ref bg_color) = entry.bg {
                let resolved_bg = config.resolve_color(bg_color);
                if let Some(hex_bg) = resolved_bg {
                    if let Some(color) = Self::parse_hex_color(&hex_bg) {
                        for i in 0..3 {
                            buf[(x + 7 + i, current_y)].set_char(' ').set_bg(color);
                        }
                    }
                }
            } else {
                buf[(x + 7, current_y)].set_char('[').set_fg(Color::Gray).set_bg(Color::Black);
                buf[(x + 8, current_y)].set_char('-').set_fg(Color::Gray).set_bg(Color::Black);
                buf[(x + 9, current_y)].set_char(']').set_fg(Color::Gray).set_bg(Color::Black);
            }

            // Col 13+: Entry name (cyan normally, gold when selected)
            let name_style = if is_selected {
                ratatui::style::Style::default().fg(Color::Rgb(255, 215, 0)).bg(Color::Black) // Gold when selected
            } else {
                ratatui::style::Style::default().fg(Color::Cyan).bg(Color::Black) // Cyan otherwise
            };

            let sound_indicator = if entry.has_sound { " ♫" } else { "" };
            let squelch_indicator = if entry.squelch { " ✖" } else { "" };
            let name_with_indicators = format!("   {}{}{}", entry.name, sound_indicator, squelch_indicator);
            for (i, ch) in name_with_indicators.chars().enumerate() {
                let col = x + 13 + i as u16;
                if col < x + width - 1 {
                    buf[(col, current_y)].set_char(ch).set_style(name_style);
                }
            }

            display_row += 1;
            render_row += 1;
        }

        // Footer (one line above the bottom border)
        let footer = " Tab/Arrows:Navigate | Enter:Edit | Del:Delete | Esc:Close ";
        let footer_y = y + height - 2;
        let footer_x = x + ((width - footer.len() as u16) / 2);
        for (i, ch) in footer.chars().enumerate() {
            buf[(footer_x + i as u16, footer_y)].set_char(ch).set_fg(Color::White).set_bg(Color::Black);
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
