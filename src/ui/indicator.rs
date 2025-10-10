use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};

/// An indicator widget for displaying boolean status or multi-level states
/// Used for injuries (0-6), status indicators (on/off), compass directions, etc.
pub struct Indicator {
    label: String,
    value: u8,  // 0 = off/none, 1-6 = injury/scar levels, or 0-1 for boolean
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: Option<Vec<String>>,
    // Colors for different states (index by value)
    // For injuries: [none, injury1, injury2, injury3, scar1, scar2, scar3]
    // For boolean: [off, on]
    colors: Vec<String>,
}

impl Indicator {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            value: 0,
            show_border: false,  // Indicators typically don't have borders
            border_style: None,
            border_color: None,
            border_sides: None,
            colors: vec![
                "#555555".to_string(),  // 0: none/off (dark gray)
                "#9BA2B2".to_string(),  // 1: injury 1 (light gray)
                "#a29900".to_string(),  // 2: injury 2 (yellow)
                "#bf4d80".to_string(),  // 3: injury 3 (red)
                "#60b4bf".to_string(),  // 4: scar 1 (cyan)
                "#477ab3".to_string(),  // 5: scar 2 (blue)
                "#7e62b3".to_string(),  // 6: scar 3 (purple)
            ],
        }
    }

    pub fn with_border_config(
        mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) -> Self {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color;
        self
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

    pub fn set_border_sides(&mut self, border_sides: Option<Vec<String>>) {
        self.border_sides = border_sides;
    }

    pub fn set_title(&mut self, title: String) {
        self.label = title;
    }

    /// Set the indicator value (0 = off/none, 1-6 for injuries/scars, 1 for boolean on)
    pub fn set_value(&mut self, value: u8) {
        self.value = value;
    }

    /// Set custom colors for each state
    pub fn set_colors(&mut self, colors: Vec<String>) {
        self.colors = colors;
    }

    /// Parse a hex color string to ratatui Color
    fn parse_color(hex: &str) -> Color {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return Color::White;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);

        Color::Rgb(r, g, b)
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 1 || area.height < 1 {
            return;
        }

        let mut block = Block::default();

        if self.show_border {
            let borders = crate::config::parse_border_sides(&self.border_sides);
            block = block.borders(borders);

            if let Some(ref style) = self.border_style {
                use ratatui::widgets::BorderType;
                let border_type = match style.as_str() {
                    "double" => BorderType::Double,
                    "rounded" => BorderType::Rounded,
                    "thick" => BorderType::Thick,
                    _ => BorderType::Plain,
                };
                block = block.border_type(border_type);
            }

            if let Some(ref color_str) = self.border_color {
                let color = Self::parse_color(color_str);
                block = block.border_style(Style::default().fg(color));
            }

            block = block.title(self.label.as_str());
        }

        let inner_area = if self.show_border {
            block.inner(area)
        } else {
            area
        };

        // Render the block first
        if self.show_border {
            block.render(area, buf);
        }

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Get color for current value
        let color_index = (self.value as usize).min(self.colors.len().saturating_sub(1));
        let color = Self::parse_color(&self.colors[color_index]);

        // Render the label text with appropriate color
        // If value is 0 and we're hiding inactive indicators, don't render
        let display_text = if self.value == 0 {
            // For inactive indicators, show dimmed text or nothing based on config
            // For now, show dimmed
            &self.label
        } else {
            &self.label
        };

        // Center the text in the available space
        let text_width = display_text.len() as u16;
        let start_col = if text_width <= inner_area.width {
            inner_area.x + (inner_area.width - text_width) / 2
        } else {
            inner_area.x
        };

        // Render each character of the label
        for (i, c) in display_text.chars().enumerate() {
            let x = start_col + i as u16;
            if x < inner_area.x + inner_area.width && inner_area.y < area.y + area.height {
                buf[(x, inner_area.y)].set_char(c);
                buf[(x, inner_area.y)].set_fg(color);
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}
