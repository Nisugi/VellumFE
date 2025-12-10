//! Popup browser that lists every built-in/custom theme.
//!
//! Provides deletion for custom entries plus navigation hints that mirror other
//! list dialogs.

use crate::theme::{AppTheme, ThemePresets};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget},
};

/// Browser for viewing and selecting application themes
pub struct ThemeBrowser {
    themes: Vec<(String, AppTheme)>, // (theme_id, theme)
    custom_theme_ids: std::collections::HashSet<String>, // Track which themes are custom
    selected_index: usize,
    scroll_offset: usize,
    active_theme_id: String, // Currently active theme to highlight

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}

impl ThemeBrowser {
    pub fn new(active_theme_id: String, config_base: Option<&str>) -> Self {
        // Load built-in themes
        let builtin_themes = ThemePresets::all();

        // Load custom themes
        let custom_themes = ThemePresets::load_custom_themes(config_base);
        let custom_theme_ids: std::collections::HashSet<String> =
            custom_themes.keys().cloned().collect();

        // Merge and sort all themes
        let mut themes: Vec<(String, AppTheme)> = builtin_themes
            .into_iter()
            .chain(custom_themes)
            .collect();
        themes.sort_by(|a, b| a.0.cmp(&b.0));

        // Find the index of the active theme
        let selected_index = themes
            .iter()
            .position(|(id, _)| id == &active_theme_id)
            .unwrap_or(0);

        Self {
            themes,
            custom_theme_ids,
            selected_index,
            scroll_offset: 0,
            active_theme_id,
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn navigate_up(&mut self) {
        if !self.themes.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
        }
    }

    pub fn navigate_down(&mut self) {
        if self.selected_index + 1 < self.themes.len() {
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
        if self.selected_index + 10 < self.themes.len() {
            self.selected_index += 10;
        } else if !self.themes.is_empty() {
            self.selected_index = self.themes.len() - 1;
        }
        self.adjust_scroll();
    }

    fn adjust_scroll(&mut self) {
        const VISIBLE_ITEMS: usize = 20;

        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + VISIBLE_ITEMS {
            self.scroll_offset = self.selected_index - VISIBLE_ITEMS + 1;
        }
    }

    pub fn get_selected_theme_id(&self) -> Option<String> {
        self.themes
            .get(self.selected_index)
            .map(|(id, _)| id.clone())
    }

    pub fn get_selected_theme(&self) -> Option<&AppTheme> {
        self.themes.get(self.selected_index).map(|(_, theme)| theme)
    }

    pub fn is_selected_custom(&self) -> bool {
        self.get_selected_theme_id()
            .map(|id| self.custom_theme_ids.contains(&id))
            .unwrap_or(false)
    }

    /// Delete a custom theme from disk and refresh the list
    pub fn delete_selected_custom(&mut self, config_base: Option<&str>) -> anyhow::Result<()> {
        use std::fs;
        use std::path::PathBuf;

        // Get selected theme ID and verify it's custom
        let theme_id = self
            .get_selected_theme_id()
            .ok_or_else(|| anyhow::anyhow!("No theme selected"))?;

        if !self.custom_theme_ids.contains(&theme_id) {
            return Err(anyhow::anyhow!("Cannot delete built-in theme"));
        }

        // Determine themes directory path
        let themes_dir = if let Some(base) = config_base {
            PathBuf::from(base).join("themes")
        } else {
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
            home.join(".vellum-fe").join("themes")
        };

        // Sanitize filename (same logic as save)
        let filename = theme_id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();

        let filepath = themes_dir.join(format!("{}.toml", filename));

        // Delete the file
        fs::remove_file(&filepath)?;

        // Remove from custom_theme_ids
        self.custom_theme_ids.remove(&theme_id);

        // Remove from themes list
        self.themes.retain(|(id, _)| id != &theme_id);

        // Adjust selected_index if necessary
        if self.selected_index >= self.themes.len() && !self.themes.is_empty() {
            self.selected_index = self.themes.len() - 1;
        }
        self.adjust_scroll();

        Ok(())
    }

    /// Move to next page (alias for page_down)
    pub fn next_page(&mut self) {
        self.page_down();
    }

    /// Move to previous page (alias for page_up)
    pub fn previous_page(&mut self) {
        self.page_up();
    }

    /// Get the selected theme ID
    pub fn get_selected(&self) -> Option<String> {
        if self.themes.is_empty() || self.selected_index >= self.themes.len() {
            return None;
        }
        Some(self.themes[self.selected_index].0.clone())
    }
}

impl Widget for &ThemeBrowser {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate popup dimensions
        let width = 80.min(area.width.saturating_sub(4));
        let height = 25.min(area.height.saturating_sub(2));

        let popup_x = if self.popup_x > 0 {
            self.popup_x.min(area.width.saturating_sub(width))
        } else {
            (area.width.saturating_sub(width)) / 2
        };

        let popup_y = if self.popup_y > 0 {
            self.popup_y.min(area.height.saturating_sub(height))
        } else {
            (area.height.saturating_sub(height)) / 2
        };

        let popup_area = Rect {
            x: area.x + popup_x,
            y: area.y + popup_y,
            width,
            height,
        };

        // Clear the background
        Clear.render(popup_area, buf);

        // Create the popup block
        let block = Block::default()
            .title(" Themes ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // Render theme list
        const VISIBLE_ITEMS: usize = 20;
        let visible_themes: Vec<_> = self
            .themes
            .iter()
            .skip(self.scroll_offset)
            .take(VISIBLE_ITEMS.min(inner.height as usize))
            .enumerate()
            .collect();

        for (i, (theme_id, theme)) in visible_themes {
            let item_index = self.scroll_offset + i;
            let is_selected = item_index == self.selected_index;
            let is_active = theme_id == &self.active_theme_id;

            // Determine item style
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            // Create the line with theme info
            let prefix = if is_active { "● " } else { "  " };
            let is_custom = self.custom_theme_ids.contains(theme_id);
            let custom_badge = if is_custom { " [Custom]" } else { "" };
            let name_part = format!("{}{}{}", prefix, theme.name, custom_badge);
            let description = &theme.description;

            // Format: "● Name [Custom] - Description"
            let line_text = format!("{:<28} - {}", name_part, description);

            let line = Line::from(vec![Span::styled(line_text, style)]);

            // Render the line
            let line_area = Rect {
                x: inner.x,
                y: inner.y + i as u16,
                width: inner.width,
                height: 1,
            };

            Paragraph::new(line).render(line_area, buf);
        }

        // Render help text at the bottom
        let help_y = popup_area.y + popup_area.height - 2;
        let help_text = Line::from(vec![
            Span::styled(" ↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(": Navigate  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(": Select  "),
            Span::styled("D", Style::default().fg(Color::Yellow)),
            Span::raw(": Delete Custom  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Cancel "),
        ]);

        let help_area = Rect {
            x: popup_area.x + 2,
            y: help_y,
            width: popup_area.width - 4,
            height: 1,
        };

        Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .render(help_area, buf);

        // Render scroll indicator if needed
        if self.themes.len() > VISIBLE_ITEMS {
            let total = self.themes.len();
            let visible_end = (self.scroll_offset + VISIBLE_ITEMS).min(total);
            let scroll_text = format!(" {}-{}/{} ", self.scroll_offset + 1, visible_end, total);

            let scroll_area = Rect {
                x: popup_area.x + popup_area.width - scroll_text.len() as u16 - 1,
                y: popup_area.y,
                width: scroll_text.len() as u16,
                height: 1,
            };

            Paragraph::new(scroll_text)
                .style(Style::default().fg(Color::Yellow))
                .render(scroll_area, buf);
        }
    }
}
