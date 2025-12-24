//! Shared popup menu used for contextual actions in the TUI.
//!
//! Provides keyboard navigation, click hit-testing, and theme-aware rendering.

use crate::frontend::tui::crossterm_bridge;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// A menu item with display text and command to execute
#[derive(Clone, Debug)]
pub struct MenuItem {
    pub text: String,
}

/// Popup menu widget for navigable menus
pub struct PopupMenu {
    items: Vec<MenuItem>,
    selected: usize,
    position: (u16, u16), // (col, row)
}

impl PopupMenu {
    /// Create a new PopupMenu with a specific selected index
    pub fn with_selected(items: Vec<MenuItem>, position: (u16, u16), selected: usize) -> Self {
        Self {
            items,
            selected,
            position,
        }
    }

    /// Render the menu at its position
    pub fn render(&self, area: Rect, buf: &mut Buffer, theme: &crate::theme::AppTheme) {
        // Calculate menu dimensions
        let max_width = self
            .items
            .iter()
            .map(|item| item.text.len())
            .max()
            .unwrap_or(20)
            .min(60);

        let width = (max_width + 4) as u16; // +4 for borders and padding
        let height = (self.items.len() + 2) as u16; // +2 for borders

        // Position the menu
        let x = self.position.0.min(area.width.saturating_sub(width));
        let y = self.position.1.min(area.height.saturating_sub(height));

        let menu_rect = Rect {
            x,
            y,
            width,
            height,
        };

        // Clear the area behind the menu
        Clear.render(menu_rect, buf);

        // Build menu lines
        let mut lines = Vec::new();
        for (idx, item) in self.items.iter().enumerate() {
            let style = if idx == self.selected {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                    .bg(crossterm_bridge::to_ratatui_color(theme.form_label_focused))
            } else {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
            };

            let line = Line::from(vec![
                Span::raw(" "),
                Span::styled(item.text.clone(), style),
                Span::raw(" "),
            ]);
            lines.push(line);
        }

        // Create block with border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crossterm_bridge::to_ratatui_color(theme.menu_border)))
            .style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.browser_background)));

        let paragraph = Paragraph::new(lines).block(block);

        ratatui::widgets::Widget::render(paragraph, menu_rect, buf);
    }
}
