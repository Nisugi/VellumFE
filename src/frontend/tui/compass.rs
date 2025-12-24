//! ASCII compass widget that highlights available exits and respects theme colors.
//!
//! The compass is a simple 7×3 grid of characters. Each cell can be styled with
//! active/inactive colors, and the surrounding chrome inherits border/background
//! settings from the window definition or theme fallback.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Clear, Widget as RatatuiWidget},
};
use std::collections::HashSet;

use super::colors::parse_color_to_ratatui;
use super::crossterm_bridge;

const CONTENT_WIDTH: u16 = 7;
const CONTENT_HEIGHT: u16 = 3;

const POSITIONS: [(u16, u16, &str, &str, &str); 11] = [
    (0, 0, "↑", "up", "up"),
    (2, 0, "↖", "nw", "northwest"),
    (4, 0, "▲", "n", "north"),
    (6, 0, "↗", "ne", "northeast"),
    (2, 1, "◀", "w", "west"),
    (4, 1, "o", "out", "out"),
    (6, 1, "▶", "e", "east"),
    (0, 2, "↓", "down", "down"),
    (2, 2, "↙", "sw", "southwest"),
    (4, 2, "▼", "s", "south"),
    (6, 2, "↘", "se", "southeast"),
];

pub struct Compass {
    label: String,
    directions: HashSet<String>, // normalized to lowercase
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<Color>,
    border_sides: crate::config::BorderSides,
    active_color: Option<Color>,
    inactive_color: Option<Color>,
    content_align: Option<String>,
    background_color: Option<Color>,
    transparent_background: bool,
}

impl Compass {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            directions: HashSet::new(),
            show_border: false,
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(),
            active_color: Some(Color::Green),
            inactive_color: Some(Color::DarkGray),
            content_align: None,
            background_color: None,
            transparent_background: false,
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

    pub fn set_directions(&mut self, directions: Vec<String>) {
        self.directions = directions.into_iter().map(|d| d.to_lowercase()).collect();
    }

    pub fn set_colors(&mut self, active_color: Option<String>, inactive_color: Option<String>) {
        if let Some(color) = active_color.and_then(|c| Self::parse_color(&c)) {
            self.active_color = Some(color);
        }
        if let Some(color) = inactive_color.and_then(|c| Self::parse_color(&c)) {
            self.inactive_color = Some(color);
        }
    }

    pub fn set_content_align(&mut self, content_align: Option<String>) {
        self.content_align = content_align;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = match color {
            Some(ref s) if s.trim() == "-" => None,
            Some(value) => Self::parse_color(value.trim()),
            None => None,
        };
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    fn parse_color(input: &str) -> Option<Color> {
        parse_color_to_ratatui(input)
    }

    fn fill_background(area: Rect, buf: &mut Buffer, color: Color) {
        for row in 0..area.height {
            for col in 0..area.width {
                let x = area.x + col;
                let y = area.y + row;
                if x < buf.area().width && y < buf.area().height {
                    buf[(x, y)].set_bg(color);
                }
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if !self.transparent_background {
            Clear.render(area, buf);
            if let Some(bg_color) = self.background_color {
                Self::fill_background(area, buf, bg_color);
            }
        }

        let mut block = Block::default();
        if !self.transparent_background {
            if let Some(bg_color) = self.background_color {
                block = block.style(Style::default().bg(bg_color));
            }
        }

        if self.show_border {
            let borders = crossterm_bridge::to_ratatui_borders(&self.border_sides);
            block = block.borders(borders);

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

            if let Some(color) = self.border_color {
                block = block.border_style(Style::default().fg(color));
            }

            // Only set title if label is non-empty (avoids empty title affecting layout)
            if !self.label.is_empty() {
                block = block.title(self.label.as_str());
            }
        }

        let inner_area = if self.show_border {
            block.inner(area)
        } else {
            area
        };

        if self.show_border {
            block.render(area, buf);
        }

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        let (row_offset, col_offset) = if let Some(ref align_str) = self.content_align {
            let align = crate::config::ContentAlign::from_str(align_str);
            align.calculate_offset(
                CONTENT_WIDTH,
                CONTENT_HEIGHT,
                inner_area.width,
                inner_area.height,
            )
        } else {
            (0, 0)
        };

        let active_color = self.active_color.unwrap_or(Color::Green);
        let inactive_color = self.inactive_color.unwrap_or(Color::DarkGray);

        for (col, row, glyph, short, long) in POSITIONS.iter() {
            let x = inner_area.x + col + col_offset;
            let y = inner_area.y + row + row_offset;

            if x >= buf.area().width || y >= buf.area().height {
                continue;
            }

            let short_lower = short.to_lowercase();
            let long_lower = long.to_lowercase();
            let is_active =
                self.directions.contains(&short_lower) || self.directions.contains(&long_lower);

            let color = if is_active {
                active_color
            } else {
                inactive_color
            };

            for (i, ch) in glyph.chars().enumerate() {
                let char_x = x + i as u16;
                if char_x < inner_area.x + inner_area.width && y < inner_area.y + inner_area.height
                {
                    buf[(char_x, y)].set_char(ch).set_fg(color);
                    if !self.transparent_background {
                        if let Some(bg_color) = self.background_color {
                            buf[(char_x, y)].set_bg(bg_color);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_and_inactive_colors() {
        let mut compass = Compass::new("Compass");
        compass.set_border_config(false, None, None);
        compass.set_transparent_background(true);
        compass.set_colors(
            Some("#ff0000".to_string()),
            Some("#00ff00".to_string()),
        );
        compass.set_directions(vec!["north".to_string(), "out".to_string()]);

        let area = Rect::new(0, 0, 7, 3);
        let mut buf = Buffer::empty(area);
        compass.render(area, &mut buf);

        assert_eq!(buf[(4, 0)].fg, Color::Rgb(255, 0, 0));
        assert_eq!(buf[(4, 1)].fg, Color::Rgb(255, 0, 0));
        assert_eq!(buf[(6, 1)].fg, Color::Rgb(0, 255, 0));
    }

    #[test]
    fn test_background_fill_when_not_transparent() {
        let mut compass = Compass::new("Compass");
        compass.set_border_config(false, None, None);
        compass.set_background_color(Some("#0000ff".to_string()));

        let area = Rect::new(0, 0, 7, 3);
        let mut buf = Buffer::empty(area);
        compass.render(area, &mut buf);

        assert_eq!(buf[(0, 0)].bg, Color::Rgb(0, 0, 255));
    }

    #[test]
    fn test_content_alignment_offsets_positions() {
        let mut compass = Compass::new("Compass");
        compass.set_border_config(false, None, None);
        compass.set_content_align(Some("center".to_string()));
        compass.set_directions(vec!["north".to_string()]);

        let area = Rect::new(0, 0, 9, 5);
        let mut buf = Buffer::empty(area);
        compass.render(area, &mut buf);

        assert_eq!(buf[(5, 1)].fg, Color::Green);
        assert_eq!(buf[(4, 0)].fg, Color::Reset);
    }
}
