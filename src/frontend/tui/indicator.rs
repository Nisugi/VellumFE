//! Minimal widget that lights up whenever a boolean status becomes active.
//!
//! Typically used for stance indicators (kneeling, prone, etc.) where the
//! inactive state should disappear to reduce clutter.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Widget},
};

use super::colors::parse_color_to_ratatui;
use super::crossterm_bridge;

/// Indicator widget for displaying boolean status (on/off)
/// Used for status indicators like "standing", "kneeling", "sitting", etc.
pub struct Indicator {
    label: String,
    active: bool, // true = on, false = off
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: crate::config::BorderSides,
    off_color: String, // Color when inactive
    on_color: String,  // Color when active
    background_color: Option<String>,
    transparent_background: bool,
}

impl Indicator {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            active: false,
            show_border: false, // Indicators typically don't have borders
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(),
            off_color: "#555555".to_string(), // Dark gray when off
            on_color: "#00ff00".to_string(),  // Green when on
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
        self.border_color = border_color;
    }

    pub fn set_border_sides(&mut self, border_sides: crate::config::BorderSides) {
        self.border_sides = border_sides;
    }

    pub fn set_title(&mut self, title: String) {
        self.label = title;
    }

    /// Set the indicator active state
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Set the status string (converts to boolean - any non-empty string is "on")
    pub fn set_status(&mut self, status: &str) {
        self.active = !status.is_empty();
    }

    /// Set custom colors for off and on states
    pub fn set_colors(&mut self, off_color: String, on_color: String) {
        self.off_color = off_color;
        self.on_color = on_color;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        // Handle three-state: None = transparent, Some("-") = transparent, Some(value) = use value
        self.background_color = match color {
            Some(ref s) if s == "-" => None, // "-" means explicitly transparent
            other => other,
        };
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    /// Parse a color string to ratatui Color (supports hex and color names)
    fn parse_color(input: &str) -> Color {
        parse_color_to_ratatui(input).unwrap_or(Color::White)
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 1 || area.height < 1 {
            return;
        }

        // Determine which borders to show
        let borders = if self.show_border {
            crossterm_bridge::to_ratatui_borders(&self.border_sides)
        } else {
            ratatui::widgets::Borders::NONE
        };

        let border_color = self
            .border_color
            .as_ref()
            .map(|c| Self::parse_color(c))
            .unwrap_or(Color::White);

        let inner_area: Rect;

        if self.show_border {
            // Use Block widget for borders
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
            block.render(area, buf);
        } else {
            inner_area = area;
        }

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Fill background if not transparent and color is set
        if !self.transparent_background {
            if let Some(ref color_hex) = self.background_color {
                let bg_color = Self::parse_color(color_hex);
                for row in 0..inner_area.height {
                    for col in 0..inner_area.width {
                        let x = inner_area.x + col;
                        let y = inner_area.y + row;
                        if x < buf.area().width && y < buf.area().height {
                            buf[(x, y)].set_char(' ');
                            buf[(x, y)].set_bg(bg_color);
                        }
                    }
                }
            }
        }

        // If inactive, render nothing (transparent)
        if !self.active {
            return;
        }

        // Get color for active state
        let color = Self::parse_color(&self.on_color);

        // Render the label text with appropriate color
        let display_text = &self.label;

        // Center the text in the available space
        let text_width = display_text.chars().count() as u16;
        let start_col = if text_width <= inner_area.width {
            inner_area.x + (inner_area.width - text_width) / 2
        } else {
            inner_area.x
        };

        // Render each character of the label (vertically centered)
        let y = inner_area.y;
        if y < buf.area().height {
            for (i, c) in display_text.chars().enumerate() {
                let x = start_col + i as u16;
                if x < inner_area.x + inner_area.width && x < buf.area().width {
                    buf[(x, y)].set_char(c);
                    buf[(x, y)].set_fg(color);
                    // Set background if not transparent and color is configured
                    if !self.transparent_background {
                        if let Some(ref color_hex) = self.background_color {
                            let bg_color = Self::parse_color(color_hex);
                            buf[(x, y)].set_bg(bg_color);
                        }
                    }
                }
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}

