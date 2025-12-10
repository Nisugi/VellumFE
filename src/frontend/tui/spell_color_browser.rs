//! Browser popup for reviewing configured spell color ranges.
//!
//! Shows color previews, associated spell IDs, and integrates with the shared
//! widget traits for navigation/deletion.

use crate::frontend::tui::crossterm_bridge;
use crate::config::SpellColorRange;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Clear, Widget},
};

pub struct SpellColorEntry {
    pub index: usize, // Index in config.spell_colors
    pub spells: Vec<u32>,
    pub bar_color: String,
    pub text_color: String,
    pub bg_color: String,
}

pub struct SpellColorBrowser {
    entries: Vec<SpellColorEntry>,
    selected_index: usize,
    scroll_offset: usize,
    popup_position: (u16, u16),
    pub is_dragging: bool,
    drag_offset: (i16, i16),
}

impl SpellColorBrowser {
    pub fn new(spell_colors: &[SpellColorRange]) -> Self {
        let entries = spell_colors
            .iter()
            .enumerate()
            .map(|(index, sc)| SpellColorEntry {
                index,
                spells: sc.spells.clone(),
                bar_color: sc.bar_color.clone().unwrap_or_else(|| sc.color.clone()),
                text_color: sc
                    .text_color
                    .clone()
                    .unwrap_or_else(|| "#ffffff".to_string()),
                bg_color: sc.bg_color.clone().unwrap_or_else(String::new),
            })
            .collect();

        Self {
            entries,
            selected_index: 0,
            scroll_offset: 0,
            popup_position: (0, 0),
            is_dragging: false,
            drag_offset: (0, 0),
        }
    }

    pub fn navigate_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            // Scroll up if needed
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn navigate_down(&mut self) {
        if self.selected_index < self.entries.len().saturating_sub(1) {
            self.selected_index += 1;
            // Scroll down if needed
            let visible_rows = 15;
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
                    buf.set_string(col, row, " ", Style::default().bg(crossterm_bridge::to_ratatui_color(theme.browser_background)));
                }
            }
        }

        // Draw border
        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.browser_border));

        // Top border
        let top = format!("┌{}┐", "─".repeat(popup_width as usize - 2));
        buf.set_string(popup_col, popup_row, &top, border_style);

        // Title
        buf.set_string(
            popup_col + 2,
            popup_row,
            " Spell Colors ",
            border_style.add_modifier(Modifier::BOLD),
        );

        // Side borders
        for i in 1..popup_height - 1 {
            buf.set_string(popup_col, popup_row + i, "│", border_style);
            buf.set_string(
                popup_col + popup_width - 1,
                popup_row + i,
                "│",
                border_style,
            );
        }

        // Bottom border
        let bottom = format!("└{}┘", "─".repeat(popup_width as usize - 2));
        buf.set_string(
            popup_col,
            popup_row + popup_height - 1,
            &bottom,
            border_style,
        );

        // Render entries
        let visible_rows = popup_height - 4;
        let visible_entries = self
            .entries
            .iter()
            .skip(self.scroll_offset)
            .take(visible_rows as usize);

        let mut y = popup_row + 2;
        for (offset, entry) in visible_entries.enumerate() {
            let is_selected = self.scroll_offset + offset == self.selected_index;
            self.render_entry(
                entry,
                popup_col + 2,
                y,
                popup_width - 4,
                is_selected,
                buf,
                theme,
            );
            y += 1;
        }

        // Status bar
        let total = self.entries.len();
        let current = if total == 0 {
            0
        } else {
            (self.selected_index + 1).min(total)
        };
        let spacer = "                 "; // 17 spaces to align with color browser
        let status = format!(
            " ↑/↓:Nav  Enter:Edit  Del:Del  {} Esc:Close  ({}/{}) ",
            spacer, current, total
        );
        buf.set_string(
            popup_col + 2,
            popup_row + popup_height - 2,
            &status,
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.menu_separator)),
        );
    }

    fn render_entry(
        &self,
        entry: &SpellColorEntry,
        x: u16,
        y: u16,
        width: u16,
        is_selected: bool,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let base_style = if is_selected {
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(super::colors::rgb_to_ratatui_color(100, 149, 237))
                .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
        };

        // Format: bar(3) + 2 spaces + bg(3) + 2 spaces + details
        let mut col = x;
        // Bar color preview: 3 full blocks or " - " if empty/invalid
        if let Some(color) = if !entry.bar_color.is_empty() {
            self.parse_color(&entry.bar_color)
        } else {
            None
        } {
            buf.set_string(
                col,
                y,
                "███",
                Style::default().fg(color).bg(crossterm_bridge::to_ratatui_color(theme.browser_background)),
            );
        } else {
            buf.set_string(col, y, " - ", base_style);
        }
        col += 3;
        buf.set_string(col, y, "  ", base_style);
        col += 2;
        // Background color preview: 3 full blocks or " - " if empty/invalid
        if let Some(color) = if !entry.bg_color.is_empty() {
            self.parse_color(&entry.bg_color)
        } else {
            None
        } {
            buf.set_string(
                col,
                y,
                "███",
                Style::default().fg(color).bg(crossterm_bridge::to_ratatui_color(theme.browser_background)),
            );
        } else {
            buf.set_string(col, y, " - ", base_style);
        }
        col += 3;
        buf.set_string(col, y, "  ", base_style);
        col += 2;

        // Spell IDs (rest of the line)
        let spells_str = entry
            .spells
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let spells_display = format!(" [{}]", spells_str);
        let used_cols = 3 + 2 + 3 + 2;
        let available_width = width.saturating_sub(used_cols as u16) as usize;
        let truncated = if spells_display.len() > available_width {
            format!(
                "{}...",
                &spells_display[..available_width.saturating_sub(3)]
            )
        } else {
            spells_display
        };

        buf.set_string(col, y, &truncated, base_style);
    }

    fn parse_color(&self, hex: &str) -> Option<Color> {
        // Use centralized mode-aware color parser
        super::colors::parse_color_to_ratatui(hex)
    }

    /// Move to next page (alias for page_down)
    pub fn next_page(&mut self) {
        self.page_down();
    }

    /// Move to previous page (alias for page_up)
    pub fn previous_page(&mut self) {
        self.page_up();
    }

    /// Update the list of spell color entries
    pub fn update_items(&mut self, spell_colors: &[SpellColorRange]) {
        self.entries = spell_colors
            .iter()
            .enumerate()
            .map(|(index, sc)| SpellColorEntry {
                index,
                spells: sc.spells.clone(),
                bar_color: sc.bar_color.clone().unwrap_or_else(|| sc.color.clone()),
                text_color: sc
                    .text_color
                    .clone()
                    .unwrap_or_else(|| "#ffffff".to_string()),
                bg_color: sc.bg_color.clone().unwrap_or_else(String::new),
            })
            .collect();

        // Reset selection if out of bounds
        let len = self.entries.len();
        if self.selected_index >= len && len > 0 {
            self.selected_index = len - 1;
        }
    }
}

// Trait implementations for SpellColorBrowser
use super::widget_traits::{Navigable, Selectable};

impl Navigable for SpellColorBrowser {
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

impl Selectable for SpellColorBrowser {
    fn get_selected(&self) -> Option<String> {
        if self.selected_index < self.entries.len() {
            Some(self.entries[self.selected_index].index.to_string())
        } else {
            None
        }
    }

    fn delete_selected(&mut self) -> Option<String> {
        if self.selected_index < self.entries.len() {
            let index = self.entries[self.selected_index].index;
            self.entries.remove(self.selected_index);
            if self.selected_index >= self.entries.len() && self.selected_index > 0 {
                self.selected_index -= 1;
            }
            // Adjust scroll manually (no adjust_scroll method)
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
            Some(index.to_string())
        } else {
            None
        }
    }
}
