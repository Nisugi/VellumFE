//! Simple spacer widget to reserve layout space or tint gaps.
//!
//! Used heavily by dynamic layouts to enforce padding without drawing chrome.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
};

/// Spacer widget - fills area with optional background color for layout spacing
/// Never shows borders or title, just optional background fill
pub struct Spacer {
    background_color: Option<String>,
    transparent: bool,
}

impl Spacer {
    pub fn new() -> Self {
        Self {
            background_color: None,
            transparent: true,
        }
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        // Handle three-state: None = transparent, Some("-") = transparent, Some(value) = use value
        self.background_color = match color {
            Some(ref s) if s == "-" => None, // "-" means explicitly transparent
            other => other,
        };
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent = transparent;
    }

    pub fn set_border_config(
        &mut self,
        _show_border: bool,
        _border_style: Option<String>,
        _border_color: Option<String>,
    ) {
        // Intentionally no-op: spacers never render borders
    }

    pub fn set_border_sides(&mut self, _sides: crate::config::BorderSides) {
        // Intentionally no-op: spacers never render borders
    }

    pub fn set_title(&mut self, _title: String) {
        // No-op: spacers have no title
    }

    /// Parse a hex color string to ratatui Color
    fn parse_color(hex: &str) -> Color {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return Color::DarkGray;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128);

        Color::Rgb(r, g, b)
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.transparent {
            return;
        }

        // If user provided a background, use it; otherwise use a subtle default
        let color = self
            .background_color
            .as_ref()
            .map(|c| Self::parse_color(c))
            .unwrap_or(Color::DarkGray);

        // Fill each row with background spaces
        for y in area.y..area.y + area.height {
            if y < buf.area().height {
                for x in area.x..area.x + area.width {
                    if x < buf.area().width {
                        buf[(x, y)].set_char(' ');
                        buf[(x, y)].set_bg(color);
                    }
                }
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}
