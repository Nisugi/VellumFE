//! Betrayer widget.
//!
//! Displays Betrayer panel data from the `BetrayerPanel` dialog:
//! - Progress bar showing blood points (lblBPs)
//! - Optional list of items contributing to the blood pool (lblitemX)
//!
//! GS4-specific widget.
//! Reads data from GameState.betrayer (populated from dialogData updates).

use crate::core::state::BetrayerState;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

/// Betrayer widget - shows blood points progress bar and item list
pub struct Betrayer {
    title: String,
    /// Whether to show border
    show_border: bool,
    /// Whether to show item list
    show_items: bool,
    /// Cached state for rendering
    value: u32,
    text: String,
    items: Vec<String>,
    /// Generation counter for change detection
    generation: u64,
    /// Border color
    border_color: Color,
    /// Text color
    text_color: Color,
    /// Bar fill color (default: dark red #8b0000)
    bar_color: Color,
    /// Background color (from theme)
    background_color: Option<Color>,
    /// Active item color (items with '!' prefix) - default: #ff4040
    active_color: Color,
}

impl Betrayer {
    pub fn new(title: &str, show_border: bool) -> Self {
        Self {
            title: title.to_string(),
            show_border,
            show_items: true,
            value: 0,
            text: String::new(),
            items: Vec::new(),
            generation: 0,
            border_color: Color::White,
            text_color: Color::White,
            bar_color: Color::Rgb(139, 0, 0), // #8b0000 dark red
            background_color: None,
            active_color: Color::Rgb(255, 64, 64), // #ff4040 red for active items
        }
    }

    /// Set the border color
    pub fn set_border_color(&mut self, color: Color) {
        self.border_color = color;
    }

    /// Set the text color
    pub fn set_text_color(&mut self, color: Color) {
        self.text_color = color;
    }

    /// Set whether to show the border
    pub fn set_show_border(&mut self, show: bool) {
        self.show_border = show;
    }

    /// Set whether to show the item list
    pub fn set_show_items(&mut self, show: bool) {
        self.show_items = show;
    }

    /// Set the bar fill color
    pub fn set_bar_color(&mut self, color: Color) {
        self.bar_color = color;
    }

    /// Set the background color (from theme)
    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color.and_then(|c| super::colors::parse_color_to_ratatui(&c));
    }

    /// Set the active item color (for items with '!' prefix)
    pub fn set_active_color(&mut self, color: Color) {
        self.active_color = color;
    }

    /// Update the widget from BetrayerState.
    /// Returns true if the display changed.
    pub fn update_from_state(&mut self, state: &BetrayerState) -> bool {
        // Quick check: if generation matches, no update needed
        if self.generation == state.generation {
            return false;
        }

        self.generation = state.generation;
        self.value = state.value;
        self.text = state.text.clone();
        self.items = state.items.clone();

        true
    }

    /// Calculate ideal content rows (excluding borders)
    pub fn ideal_content_rows(&self) -> u16 {
        let bar_row = 1;
        let item_rows = if self.show_items {
            self.items.len().max(1) // At least 1 row for items section
        } else {
            0
        };
        (bar_row + item_rows) as u16
    }

    /// Render a progress bar within a single line
    fn render_bar(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let bar_width = area.width as usize;
        let filled_width = (bar_width as u32 * self.value.min(100) / 100) as usize;

        // Prepare display text, truncate if needed
        let display_text = if self.text.len() > bar_width {
            &self.text[..bar_width]
        } else {
            &self.text
        };

        // Center the text
        let text_start = (bar_width.saturating_sub(display_text.len())) / 2;

        // Unfilled background: use theme background or transparent (no change)
        let unfilled_bg = self.background_color;

        for col in 0..bar_width {
            let x = area.x + col as u16;
            let y = area.y;

            if x >= buf.area().width || y >= buf.area().height {
                continue;
            }

            let is_filled = col < filled_width;

            // Determine character at this position
            let ch = if col >= text_start && col < text_start + display_text.len() {
                display_text.chars().nth(col - text_start).unwrap_or(' ')
            } else {
                ' '
            };

            if is_filled {
                buf[(x, y)].set_char(ch);
                buf[(x, y)].set_fg(self.text_color);
                buf[(x, y)].set_bg(self.bar_color);
            } else {
                buf[(x, y)].set_char(ch);
                buf[(x, y)].set_fg(self.text_color);
                // Only set bg if we have a theme background, otherwise leave transparent
                if let Some(bg) = unfilled_bg {
                    buf[(x, y)].set_bg(bg);
                }
            }
        }
    }

    /// Render the betrayer widget
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Apply background color to full area (including borders) before rendering block
        if let Some(bg_color) = self.background_color {
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(bg_color);
                    }
                }
            }
        }

        let inner = if self.show_border {
            let block = Block::default()
                .title(self.title.as_str())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.border_color));
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // If no data, show placeholder
        if self.text.is_empty() && self.items.is_empty() {
            let placeholder = Line::from(Span::styled(
                "(No blood pool data)",
                Style::default().fg(Color::DarkGray),
            ));
            let placeholder_text = ratatui::widgets::Paragraph::new(placeholder);
            placeholder_text.render(inner, buf);
            return;
        }

        let mut current_y = inner.y;

        // Row 1: Progress bar
        if inner.height > 0 {
            let bar_area = Rect {
                x: inner.x,
                y: current_y,
                width: inner.width,
                height: 1,
            };
            self.render_bar(bar_area, buf);
            current_y += 1;
        }

        // Remaining rows: Item list (if enabled)
        if self.show_items && current_y < inner.y + inner.height {
            let available_rows = (inner.y + inner.height).saturating_sub(current_y) as usize;

            for (i, item) in self.items.iter().take(available_rows).enumerate() {
                let item_y = current_y + i as u16;
                if item_y >= inner.y + inner.height {
                    break;
                }

                // Check if item is active (has '!' prefix) - only the '!' is in active_color
                let is_active = item.starts_with('!');

                // Truncate item text if needed
                let display_text = if item.len() > inner.width as usize {
                    &item[..inner.width as usize]
                } else {
                    item
                };

                // Build line with spans: red '!' + normal rest, or just normal text
                let item_line = if is_active && display_text.len() > 1 {
                    Line::from(vec![
                        Span::styled("!", Style::default().fg(self.active_color)),
                        Span::styled(
                            display_text[1..].to_string(),
                            Style::default().fg(self.text_color),
                        ),
                    ])
                } else if is_active {
                    // Just "!" with nothing after
                    Line::from(Span::styled("!", Style::default().fg(self.active_color)))
                } else {
                    Line::from(Span::styled(
                        display_text.to_string(),
                        Style::default().fg(self.text_color),
                    ))
                };

                let line_area = Rect {
                    x: inner.x,
                    y: item_y,
                    width: inner.width,
                    height: 1,
                };
                let para = ratatui::widgets::Paragraph::new(item_line);
                para.render(line_area, buf);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_default() {
        let betrayer = Betrayer::new("Blood Pool", true);
        assert_eq!(betrayer.title, "Blood Pool");
        assert!(betrayer.show_border);
        assert!(betrayer.show_items);
        assert_eq!(betrayer.value, 0);
        assert_eq!(betrayer.generation, 0);
        assert_eq!(betrayer.bar_color, Color::Rgb(139, 0, 0));
        assert_eq!(betrayer.active_color, Color::Rgb(255, 64, 64));
    }

    #[test]
    fn test_update_from_state_no_change() {
        let mut betrayer = Betrayer::new("Blood Pool", true);
        let state = BetrayerState::default();

        // Default state with generation 0 matches betrayer.generation 0, so no change
        let changed = betrayer.update_from_state(&state);
        assert!(!changed);
    }

    #[test]
    fn test_update_from_state_with_change() {
        let mut betrayer = Betrayer::new("Blood Pool", true);
        let mut state = BetrayerState::default();
        state.generation = 1;
        state.value = 75;
        state.text = "Blood Points: 75".to_string();
        state.items = vec!["a patchwork dwarf skin backpack".to_string()];

        let changed = betrayer.update_from_state(&state);
        assert!(changed);
        assert_eq!(betrayer.generation, 1);
        assert_eq!(betrayer.value, 75);
        assert_eq!(betrayer.text, "Blood Points: 75");
        assert_eq!(betrayer.items.len(), 1);
    }

    #[test]
    fn test_ideal_content_rows() {
        let mut betrayer = Betrayer::new("Blood Pool", true);

        // No items, show_items=true -> 1 bar + 1 placeholder = 2
        assert_eq!(betrayer.ideal_content_rows(), 2);

        // Add 3 items -> 1 bar + 3 items = 4
        betrayer.items = vec![
            "item1".to_string(),
            "item2".to_string(),
            "item3".to_string(),
        ];
        assert_eq!(betrayer.ideal_content_rows(), 4);

        // show_items=false -> just 1 bar row
        betrayer.set_show_items(false);
        assert_eq!(betrayer.ideal_content_rows(), 1);
    }

    #[test]
    fn test_set_colors() {
        let mut betrayer = Betrayer::new("Blood Pool", true);

        betrayer.set_border_color(Color::Cyan);
        assert_eq!(betrayer.border_color, Color::Cyan);

        betrayer.set_text_color(Color::Green);
        assert_eq!(betrayer.text_color, Color::Green);

        betrayer.set_bar_color(Color::Red);
        assert_eq!(betrayer.bar_color, Color::Red);
    }

    #[test]
    fn test_set_show_items() {
        let mut betrayer = Betrayer::new("Blood Pool", true);
        assert!(betrayer.show_items);

        betrayer.set_show_items(false);
        assert!(!betrayer.show_items);
    }

    #[test]
    fn test_set_show_border() {
        let mut betrayer = Betrayer::new("Blood Pool", true);
        assert!(betrayer.show_border);

        betrayer.set_show_border(false);
        assert!(!betrayer.show_border);
    }

    #[test]
    fn test_set_active_color() {
        let mut betrayer = Betrayer::new("Blood Pool", true);
        assert_eq!(betrayer.active_color, Color::Rgb(255, 64, 64));

        betrayer.set_active_color(Color::Rgb(255, 0, 0));
        assert_eq!(betrayer.active_color, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn test_active_items_with_prefix() {
        let mut betrayer = Betrayer::new("Blood Pool", true);
        let mut state = BetrayerState::default();
        state.generation = 1;
        state.value = 100;
        state.text = "Blood Points: 100".to_string();
        // Mix of active (with '!') and inactive items
        state.items = vec![
            "!active item".to_string(),
            "inactive item".to_string(),
        ];

        betrayer.update_from_state(&state);
        assert_eq!(betrayer.items.len(), 2);
        // Items are stored with '!' prefix intact
        assert!(betrayer.items[0].starts_with('!'));
        assert!(!betrayer.items[1].starts_with('!'));
    }
}
