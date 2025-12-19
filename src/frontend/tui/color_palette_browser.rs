//! Popup browser that lists palette colors grouped by category.
//!
//! Provides filtering, scrolling, and drag handling so it behaves like the
//! other management popups.

use crate::frontend::tui::colors::parse_color_to_ratatui;
use crate::frontend::tui::crossterm_bridge;
use crate::config::PaletteColor;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Clear, Widget},
};

/// Browser for viewing and managing color palette
pub struct ColorPaletteBrowser {
    colors: Vec<PaletteColor>,
    selected_index: usize,
    scroll_offset: usize,
    filter: String, // Filter by name or category

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}

impl ColorPaletteBrowser {
    pub fn new(colors: Vec<PaletteColor>) -> Self {
        // Sort by category, then by name
        let mut sorted_colors = colors;
        sorted_colors.sort_by(|a, b| {
            a.category
                .cmp(&b.category)
                .then_with(|| a.name.cmp(&b.name))
        });

        Self {
            colors: sorted_colors,
            selected_index: 0,
            scroll_offset: 0,
            filter: String::new(),
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter.to_lowercase();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn filtered_colors(&self) -> Vec<&PaletteColor> {
        if self.filter.is_empty() {
            self.colors.iter().collect()
        } else {
            self.colors
                .iter()
                .filter(|c| {
                    c.name.to_lowercase().contains(&self.filter)
                        || c.category.to_lowercase().contains(&self.filter)
                })
                .collect()
        }
    }

    pub fn get_colors(&self) -> &Vec<PaletteColor> {
        &self.colors
    }

    pub fn navigate_up(&mut self) {
        let filtered = self.filtered_colors();
        if !filtered.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
        }
    }

    pub fn navigate_down(&mut self) {
        let filtered = self.filtered_colors();
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
        let filtered = self.filtered_colors();
        if self.selected_index + 10 < filtered.len() {
            self.selected_index += 10;
        } else if !filtered.is_empty() {
            self.selected_index = filtered.len() - 1;
        }
        self.adjust_scroll();
    }

    fn adjust_scroll(&mut self) {
        // Calculate total display rows including section headers
        let filtered = self.filtered_colors();
        let mut total_display_rows = 0;
        let mut last_category: Option<&str> = None;
        let mut selected_display_row = 0;

        for (idx, color) in filtered.iter().enumerate() {
            // Add section header row if category changes
            if last_category != Some(&color.category) {
                total_display_rows += 1;
                last_category = Some(&color.category);
            }

            // Track which display row the selected item is on
            if idx == self.selected_index {
                selected_display_row = total_display_rows;
            }

            total_display_rows += 1;
        }

        let visible_rows = 15;

        // Adjust scroll to keep selected item in view
        if selected_display_row < self.scroll_offset {
            self.scroll_offset = selected_display_row;
        } else if selected_display_row >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected_display_row.saturating_sub(visible_rows - 1);
        }
    }

    pub fn get_selected(&self) -> Option<String> {
        let filtered = self.filtered_colors();
        filtered.get(self.selected_index).map(|c| c.name.clone())
    }

    pub fn get_selected_color(&self) -> Option<&PaletteColor> {
        let filtered = self.filtered_colors();
        filtered.get(self.selected_index).copied()
    }

    pub fn toggle_favorite(&mut self) {
        if let Some(color) = self.get_selected_color() {
            let name = color.name.clone();
            if let Some(c) = self.colors.iter_mut().find(|c| c.name == name) {
                c.favorite = !c.favorite;
            }
        }
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(
        &mut self,
        mouse_col: u16,
        mouse_row: u16,
        mouse_down: bool,
        area: Rect,
    ) -> bool {
        let popup_width = 70.min(area.width);

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

    pub fn render(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        _config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        let popup_width = 70;
        let popup_height = 20;

        // Center on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(popup_width)) / 2;
            self.popup_y = (area.height.saturating_sub(popup_height)) / 2;
        }

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width.min(area.width.saturating_sub(self.popup_x)),
            height: popup_height.min(area.height.saturating_sub(self.popup_y)),
        };

        // Clear the popup area to prevent bleed-through
        Clear.render(popup_area, buf);

        // Draw solid black background
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(' ');
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
        }

        // Draw border
        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.browser_border));
        self.draw_border(popup_area, buf, border_style);

        // Draw title
        let title = if self.filter.is_empty() {
            format!(" Color Palette ({}) ", self.colors.len())
        } else {
            format!(
                " Color Palette ({}/{}) - Filter: {} ",
                self.filtered_colors().len(),
                self.colors.len(),
                self.filter
            )
        };
        let title_x = popup_area.x + 2;
        if title_x < popup_area.x + popup_area.width {
            for (i, ch) in title.chars().enumerate() {
                let x = title_x + i as u16;
                if x >= popup_area.x + popup_area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, popup_area.y)) {
                    cell.set_char(ch);
                    cell.set_fg(super::colors::rgb_to_ratatui_color(100, 149, 237));
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
        }

        // Draw help text with selection count
        let total = self.filtered_colors().len();
        let current = if total == 0 {
            0
        } else {
            (self.selected_index + 1).min(total)
        };
        let help = format!(
            " ↑/↓:Nav  Enter:Edit  Del:Del  F:Fav  /:Filter  Esc:Close  ({}/{}) ",
            current, total
        );
        let help_x = popup_area.x + popup_area.width.saturating_sub(help.len() as u16 + 1);
        let start_x = if help_x > popup_area.x + 1 {
            help_x
        } else {
            popup_area.x + 1
        };
        let help_y = popup_area.y + popup_area.height.saturating_sub(2);
        if start_x < popup_area.x + popup_area.width && help_y < popup_area.y + popup_area.height {
            for (i, ch) in help.chars().enumerate() {
                let x = start_x + i as u16;
                if x >= popup_area.x + popup_area.width - 1 {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, help_y)) {
                    cell.set_char(ch);
                    cell.set_fg(crossterm_bridge::to_ratatui_color(theme.menu_separator));
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
        }

        // Draw colors list
        let list_area = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 1,
            width: popup_area.width.saturating_sub(4),
            height: popup_area.height.saturating_sub(4),
        };

        let filtered = self.filtered_colors();
        if filtered.is_empty() {
            // Show "No colors" message
            let msg = if self.filter.is_empty() {
                "No colors in palette"
            } else {
                "No colors match filter"
            };
            let x = list_area.x + (list_area.width.saturating_sub(msg.len() as u16)) / 2;
            let y = list_area.y + list_area.height / 2;
            for (i, ch) in msg.chars().enumerate() {
                if let Some(cell) = buf.cell_mut((x + i as u16, y)) {
                    cell.set_char(ch);
                    cell.set_fg(crossterm_bridge::to_ratatui_color(theme.menu_separator));
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
            return;
        }

        // Track categories for section headers
        let mut last_category: Option<&str> = None;
        let mut last_rendered_category: Option<&str> = None;
        let mut display_row = 0;
        let mut render_row = 0;
        let visible_start = self.scroll_offset;
        let visible_end = visible_start + list_area.height as usize;

        for (abs_idx, color) in filtered.iter().enumerate() {
            // Check if we need a category header
            if last_category != Some(&color.category) {
                // Always increment display_row for the header
                if display_row >= visible_start {
                    // Header is in visible range or we're past it
                    if display_row < visible_end && render_row < list_area.height as usize {
                        // Render the header
                        let y = list_area.y + render_row as u16;
                        let header = format!("═══ {} ═══", color.category.to_uppercase());
                        let header_style = Style::default()
                            .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                            .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                            .add_modifier(Modifier::BOLD);

                        for (i, ch) in header.chars().enumerate() {
                            let x = list_area.x + i as u16;
                            if x >= list_area.x + list_area.width {
                                break;
                            }
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.set_char(ch);
                                cell.set_style(header_style);
                            }
                        }
                        render_row += 1;
                        last_rendered_category = Some(&color.category);
                    }
                }
                display_row += 1;
                last_category = Some(&color.category);
            }

            // Skip if before visible range
            if display_row < visible_start {
                display_row += 1;
                continue;
            }

            // If this is a new category in the visible area and we haven't rendered its header yet
            if last_rendered_category != Some(&color.category)
                && render_row < list_area.height as usize
            {
                // Render sticky header for this category
                let y = list_area.y + render_row as u16;
                let header = format!("═══ {} ═══", color.category.to_uppercase());
                let header_style = Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                    .add_modifier(Modifier::BOLD);

                for (i, ch) in header.chars().enumerate() {
                    let x = list_area.x + i as u16;
                    if x >= list_area.x + list_area.width {
                        break;
                    }
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_char(ch);
                        cell.set_style(header_style);
                    }
                }
                render_row += 1;
                last_rendered_category = Some(&color.category);
            }

            // Stop if past visible range
            if display_row >= visible_end || render_row >= list_area.height as usize {
                break;
            }

            let y = list_area.y + render_row as u16;

            let is_selected = abs_idx == self.selected_index;

            // Format: preview(3) + 3 spaces + fav + 3 spaces + name + color code + slot
            let preview = "███"; // 3-character preview swatch (full blocks)
            let fav_char = if color.favorite { '*' } else { ' ' };
            let slot_str = color.slot.map_or(String::new(), |s| format!(" [{}]", s));
            let content = format!("   {}   {:<18} {}{}", fav_char, color.name, color.color, slot_str);
            // Parse the color for preview
            let preview_color = Self::parse_hex_color(&color.color).unwrap_or(Color::White);

            let style = if is_selected {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(super::colors::rgb_to_ratatui_color(100, 149, 237))
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
            };

            // Render color preview with actual color
            for (i, ch) in preview.chars().enumerate() {
                let x = list_area.x + i as u16;
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(ch);
                    cell.set_fg(preview_color);
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }

            // Render rest of line (after preview)
            let preview_cols = preview.chars().count() as u16;
            for (i, ch) in content.chars().enumerate() {
                let x = list_area.x + preview_cols + i as u16;
                if x >= list_area.x + list_area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(ch);
                    cell.set_style(style);
                }
            }

            display_row += 1;
            render_row += 1;
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

    /// Parse a color string to ratatui Color (supports hex and color names)
    fn parse_hex_color(input: &str) -> Option<Color> {
        parse_color_to_ratatui(input)
    }

    /// Move to next page (alias for page_down)
    pub fn next_page(&mut self) {
        if self.colors.is_empty() {
            return;
        }
        let max_index = self.colors.len() - 1;
        self.selected_index = (self.selected_index + 10).min(max_index);
    }

    /// Move to previous page (alias for page_up)
    pub fn previous_page(&mut self) {
        if self.colors.is_empty() {
            return;
        }
        self.selected_index = self.selected_index.saturating_sub(10);
    }

    /// Update the list of color palette entries
    pub fn update_items(&mut self, palette: Vec<crate::config::PaletteColor>) {
        self.colors = palette;
        // Sort by name
        self.colors.sort_by(|a, b| a.name.cmp(&b.name));

        // Reset selection if out of bounds
        if self.selected_index >= self.colors.len() && !self.colors.is_empty() {
            self.selected_index = self.colors.len() - 1;
        }
    }
}

// Trait implementations for ColorPaletteBrowser
use super::widget_traits::{Navigable, Selectable};

impl Navigable for ColorPaletteBrowser {
    fn navigate_up(&mut self) {
        self.navigate_up();
    }

    fn navigate_down(&mut self) {
        self.navigate_down();
    }

    fn page_up(&mut self) {
        self.page_up();
    }

    fn page_down(&mut self) {
        self.page_down();
    }
}

impl Selectable for ColorPaletteBrowser {
    fn get_selected(&self) -> Option<String> {
        self.get_selected()
    }

    fn delete_selected(&mut self) -> Option<String> {
        let name = self.get_selected()?;
        self.colors.retain(|c| c.name != name);
        let filtered = self.filtered_colors();
        if self.selected_index >= filtered.len() && self.selected_index > 0 {
            self.selected_index = filtered.len() - 1;
        }
        self.adjust_scroll();
        Some(name)
    }
}
