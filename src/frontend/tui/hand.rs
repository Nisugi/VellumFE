//! Displays the left/right/spell hand contents using configurable glyphs.
//!
//! Handles truncated item names, optional icons, and partial border rendering so
//! the widget can slot into dense HUD layouts.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear, Widget},
};

use super::colors::parse_color_to_ratatui;
use super::crossterm_bridge;

/// Individual hand widget for left/right/spell hand
/// Shows icon + text for a single hand (e.g., "L: item name")
pub struct Hand {
    label: String,
    hand_type: HandType,
    content: String,
    icon: String, // Configurable icon (e.g., "L:", "R:", "S:")
    icon_color: Option<Color>,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<Color>,
    border_sides: crate::config::BorderSides,
    text_color: Option<Color>,
    content_highlight_color: Option<Color>,
    background_color: Option<Color>,
    transparent_background: bool,
    link_data: Option<crate::data::LinkData>,
}

#[derive(Debug, Clone, Copy)]
pub enum HandType {
    Left,
    Right,
    Spell,
}

impl Hand {
    pub fn new(label: &str, hand_type: HandType) -> Self {
        let default_icon = match hand_type {
            HandType::Left => "L:",
            HandType::Right => "R:",
            HandType::Spell => "S:",
        };

        Self {
            label: label.to_string(),
            hand_type,
            content: String::new(),
            icon: default_icon.to_string(),
            icon_color: None,
            show_border: false,
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(),
            text_color: None, // Will use global default
            content_highlight_color: None,
            background_color: None,
            transparent_background: false, // Default to transparent
            link_data: None,
        }
    }

    pub fn set_border_config(
        &mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color.and_then(|c| Self::parse_color(&c));
    }

    pub fn set_border_sides(&mut self, border_sides: crate::config::BorderSides) {
        self.border_sides = border_sides;
    }

    pub fn set_title(&mut self, title: String) {
        self.label = title;
    }

    pub fn set_icon(&mut self, icon: String) {
        self.icon = icon;
    }

    pub fn set_icon_color(&mut self, color: Option<String>) {
        self.icon_color = color.and_then(|c| Self::parse_color(&c));
    }

    pub fn set_content(&mut self, content: String) {
        // Truncate to 24 characters
        self.content = if content.chars().count() > 24 {
            content.chars().take(24).collect()
        } else {
            content
        };
    }

    pub fn set_link_data(&mut self, link: Option<crate::data::LinkData>) {
        self.link_data = link;
    }

    pub fn link_data(&self) -> Option<crate::data::LinkData> {
        self.link_data.clone()
    }

    pub fn has_border(&self) -> bool {
        self.show_border
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color.and_then(|c| Self::parse_color(&c));
    }

    pub fn set_content_highlight_color(&mut self, color: Option<String>) {
        self.content_highlight_color = color.and_then(|c| Self::parse_color(&c));
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = match color {
            Some(ref s) if s == "-" => None,
            Some(value) => Self::parse_color(&value),
            None => None,
        };
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    /// Parse a color string to ratatui Color (supports hex and color names)
    fn parse_color(input: &str) -> Option<Color> {
        parse_color_to_ratatui(input)
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        if !self.transparent_background {
            let bg_color = self.background_color.unwrap_or(Color::Reset);
            for row in 0..area.height {
                for col in 0..area.width {
                    let x = area.x + col;
                    let y = area.y + row;
                    if x < buf.area().width && y < buf.area().height {
                        buf[(x, y)].set_bg(bg_color);
                    }
                }
            }
        }

        // Determine which borders to show
        let borders = if self.show_border {
            crossterm_bridge::to_ratatui_borders(&self.border_sides)
        } else {
            Borders::NONE
        };

        let border_color = self.border_color.unwrap_or(Color::White);

        // Check if we only have left/right borders (no top/bottom)
        let only_horizontal_borders = self.show_border
            && (borders.contains(ratatui::widgets::Borders::LEFT)
                || borders.contains(ratatui::widgets::Borders::RIGHT))
            && !borders.contains(ratatui::widgets::Borders::TOP)
            && !borders.contains(ratatui::widgets::Borders::BOTTOM);

        let inner_area: Rect;

        if only_horizontal_borders {
            // For left/right only borders, we'll manually render them on the content row
            let has_left = borders.contains(ratatui::widgets::Borders::LEFT);
            let has_right = borders.contains(ratatui::widgets::Borders::RIGHT);
            let border_width = (if has_left { 1 } else { 0 }) + (if has_right { 1 } else { 0 });

            inner_area = Rect {
                x: area.x + (if has_left { 1 } else { 0 }),
                y: area.y,
                width: area.width.saturating_sub(border_width),
                height: area.height,
            };
            // We'll render the borders later after content
        } else if self.show_border {
            // Use Block widget for all other border combinations
            let mut block = Block::default().borders(borders);

            if let Some(ref style) = self.border_style {
                let border_type = match style.as_str() {
                    "double" => BorderType::Double,
                    "rounded" => BorderType::Rounded,
                    "thick" => BorderType::Thick,
                    "quadrant_inside" => BorderType::QuadrantInside,
                    "quadrant_outside" => BorderType::QuadrantOutside,
                    _ => BorderType::Plain,
                };
                block = block.border_type(border_type);
            }

            block = block.border_style(Style::default().fg(border_color));

            // Only set title if label is non-empty (avoids empty title affecting layout)
            if !self.label.is_empty() {
                block = block.title(self.label.as_str());
            }

            inner_area = block.inner(area);
            use ratatui::widgets::Widget;
            block.render(area, buf);
        } else {
            inner_area = area;
        }

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Fill entire area with background color if not transparent
        let fill_bg = if self.transparent_background {
            None
        } else {
            Some(self.background_color.unwrap_or(Color::Reset))
        };

        // Trust that text_color is always set by window manager from config resolution
        let base_text_color = self.text_color.unwrap_or(Color::Reset);
        let icon_color = self.icon_color.unwrap_or(base_text_color);
        let content_color = self.content_highlight_color.unwrap_or(base_text_color);

        let y = inner_area.y;

        // Render icon using configurable icon field
        for (i, ch) in self.icon.chars().enumerate() {
            let x = inner_area.x + i as u16;
            if x < inner_area.x + inner_area.width && x < buf.area().width && y < buf.area().height
            {
                buf[(x, y)].set_char(ch);
                buf[(x, y)].set_fg(icon_color);
                if let Some(bg_color) = fill_bg {
                    buf[(x, y)].set_bg(bg_color);
                }
            }
        }

        // Render content after icon (+ 1 space)
        let start_col = self.icon.chars().count() as u16 + 1;
        for (i, ch) in self.content.chars().enumerate() {
            let x = inner_area.x + start_col + i as u16;
            if x < inner_area.x + inner_area.width && x < buf.area().width && y < buf.area().height
            {
                buf[(x, y)].set_char(ch);
                buf[(x, y)].set_fg(content_color);
                if let Some(bg_color) = fill_bg {
                    buf[(x, y)].set_bg(bg_color);
                }
            }
        }

        // If we have left/right only borders, render them manually on the content row
        if only_horizontal_borders {
            let content_y = inner_area.y; // Hand widgets always render at y=0 of inner area
            if content_y < buf.area().height {
                let has_left = borders.contains(ratatui::widgets::Borders::LEFT);
                let has_right = borders.contains(ratatui::widgets::Borders::RIGHT);

                // Render left border
                if has_left && area.x < buf.area().width {
                    buf[(area.x, content_y)].set_char('│');
                    buf[(area.x, content_y)].set_fg(border_color);
                }
                // Render right border
                if has_right {
                    let right_x = area.x + area.width.saturating_sub(1);
                    if right_x < buf.area().width {
                        buf[(right_x, content_y)].set_char('│');
                        buf[(right_x, content_y)].set_fg(border_color);
                    }
                }
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }

    /// Convert mouse position to text coordinates
    /// For Hand widget, line is always 0, col is relative to content start
    pub fn mouse_to_text_coords(
        &self,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: Rect,
    ) -> Option<(usize, usize)> {
        // Check if click is within the window area
        if mouse_row < window_rect.y || mouse_row >= window_rect.y + window_rect.height {
            return None;
        }
        if mouse_col < window_rect.x || mouse_col >= window_rect.x + window_rect.width {
            return None;
        }

        // Hand is a single-line widget
        let line = 0;
        let col = (mouse_col - window_rect.x) as usize;
        Some((line, col))
    }

    /// Extract text from a selection range
    /// For Hand widget, returns icon + content as a single line
    pub fn extract_selection_text(
        &self,
        _start_line: usize,
        start_col: usize,
        _end_line: usize,
        end_col: usize,
    ) -> String {
        // Build the full display line: icon + space + content
        let full_text = format!("{} {}", self.icon, self.content);

        // Extract the selected portion
        let chars: Vec<char> = full_text.chars().collect();
        let start = start_col.min(chars.len());
        let end = end_col.min(chars.len());

        if start >= end {
            return String::new();
        }

        chars[start..end].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    fn buffer_line(buf: &Buffer, y: u16, width: u16) -> String {
        let mut line = String::new();
        for x in 0..width {
            line.push_str(buf[(x, y)].symbol());
        }
        line
    }

    #[test]
    fn test_render_includes_icon_and_content() {
        let mut hand = Hand::new("Left", HandType::Left);
        hand.set_content("short sword".to_string());
        hand.set_transparent_background(true);

        let area = Rect::new(0, 0, 30, 1);
        let mut buf = Buffer::empty(area);
        hand.render(area, &mut buf);

        let line = buffer_line(&buf, 0, area.width);
        assert!(line.contains("L:"));
        assert!(line.contains("short sword"));
    }

    #[test]
    fn test_content_truncation() {
        let mut hand = Hand::new("Left", HandType::Left);
        hand.set_content("abcdefghijklmnopqrstuvwxyz".to_string());
        hand.set_transparent_background(true);

        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        hand.render(area, &mut buf);

        let line = buffer_line(&buf, 0, area.width);
        assert!(!line.contains("y"));
        assert!(!line.contains("z"));
    }

    #[test]
    fn test_left_right_borders_rendered() {
        let mut hand = Hand::new("Left", HandType::Left);
        hand.set_border_config(true, None, None);
        hand.set_border_sides(crate::config::BorderSides {
            left: true,
            right: true,
            top: false,
            bottom: false,
        });
        hand.set_transparent_background(true);

        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        hand.render(area, &mut buf);

        assert_eq!(buf[(0, 0)].symbol(), "│");
        assert_eq!(buf[(9, 0)].symbol(), "│");
    }
}
