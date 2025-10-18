use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// A popup menu for context actions
pub struct PopupMenu {
    items: Vec<MenuItem>,
    selected_index: usize,
    position: (u16, u16), // (x, y) position for the popup
}

#[derive(Clone)]
pub struct MenuItem {
    pub text: String,
    pub command: String,
}

impl PopupMenu {
    pub fn new(items: Vec<MenuItem>, position: (u16, u16)) -> Self {
        Self {
            items,
            selected_index: 0,
            position,
        }
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.items.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.items.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.items.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn get_selected_command(&self) -> Option<String> {
        self.items.get(self.selected_index).map(|item| item.command.clone())
    }

    pub fn get_items(&self) -> &[MenuItem] {
        &self.items
    }

    pub fn get_position(&self) -> (u16, u16) {
        self.position
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected_index
    }

    /// Check if a mouse click at (x, y) hits a menu item
    /// Returns the index of the clicked item if any
    pub fn check_click(&self, x: u16, y: u16, area: Rect) -> Option<usize> {
        // Check if click is within the menu area
        if x < area.x || x >= area.x + area.width || y < area.y || y >= area.y + area.height {
            return None;
        }

        // Calculate which item was clicked (accounting for border and title)
        let relative_y = (y - area.y) as usize;

        // Border takes 1 row at top and bottom
        // Title takes 0 rows (it's in the border)
        if relative_y == 0 || relative_y >= area.height as usize - 1 {
            return None; // Clicked on border
        }

        let item_index = relative_y - 1; // Subtract top border

        if item_index < self.items.len() {
            Some(item_index)
        } else {
            None
        }
    }

    /// Render the popup menu
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Calculate menu size based on items
        let max_width = self.items.iter()
            .map(|item| item.text.len())
            .max()
            .unwrap_or(20)
            .min(60); // Cap at 60 chars wide

        let menu_width = (max_width + 4) as u16; // Add padding for borders and spacing
        let menu_height = (self.items.len() + 2) as u16; // Items + borders

        // Position the menu, ensuring it fits on screen
        let x = self.position.0.min(area.width.saturating_sub(menu_width));
        let y = self.position.1.min(area.height.saturating_sub(menu_height));

        let menu_rect = Rect {
            x,
            y,
            width: menu_width.min(area.width),
            height: menu_height.min(area.height),
        };

        // Calculate inner width for padding
        let inner_width = menu_width.saturating_sub(2) as usize; // Subtract borders

        // Build menu lines
        let lines: Vec<Line> = self.items.iter().enumerate().map(|(idx, item)| {
            let style = if idx == self.selected_index {
                Style::default().fg(Color::Black).bg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan).bg(Color::Black)
            };

            // Pad the text to fill the entire width
            let text = format!(" {:<width$} ", item.text, width = inner_width.saturating_sub(2));
            Line::from(vec![
                Span::styled(text, style)
            ])
        }).collect();

        // Clear the background area first to ensure solid background
        for y in menu_rect.y..menu_rect.y + menu_rect.height {
            for x in menu_rect.x..menu_rect.x + menu_rect.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(" ");
                    cell.set_style(Style::default().bg(Color::Black).fg(Color::Black));
                }
            }
        }

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title("Menu")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .style(Style::default().bg(Color::Black))
            );

        paragraph.render(menu_rect, buf);
    }
}
