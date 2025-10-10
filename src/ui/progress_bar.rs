use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};

/// A progress bar widget for displaying vitals, spell durations, etc.
pub struct ProgressBar {
    label: String,
    current: u32,
    max: u32,
    custom_text: Option<String>,  // Custom text to display instead of values (e.g., "clear as a bell")
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    bar_color: Option<String>,
    background_color: Option<String>,
    transparent_background: bool,  // If true, unfilled portion is transparent; if false, use background_color
    show_percentage: bool,
    show_values: bool,
}

impl ProgressBar {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            current: 0,
            max: 100,
            custom_text: None,
            show_border: true,
            border_style: None,
            border_color: None,
            bar_color: Some("#00ff00".to_string()), // Green by default
            background_color: None,
            transparent_background: true, // Transparent by default
            show_percentage: true,
            show_values: true,
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

    pub fn set_title(&mut self, title: String) {
        self.label = title;
    }

    pub fn set_colors(&mut self, bar_color: Option<String>, background_color: Option<String>) {
        // Only update if provided (don't replace with None)
        if bar_color.is_some() {
            self.bar_color = bar_color;
        }
        if background_color.is_some() {
            self.background_color = background_color;
        }
    }

    pub fn set_value(&mut self, current: u32, max: u32) {
        self.current = current;
        self.max = max;
        self.custom_text = None;  // Clear custom text when setting values directly
    }

    pub fn set_value_with_text(&mut self, current: u32, max: u32, custom_text: Option<String>) {
        self.current = current;
        self.max = max;
        self.custom_text = custom_text;
    }

    pub fn set_display_options(&mut self, show_percentage: bool, show_values: bool) {
        self.show_percentage = show_percentage;
        self.show_values = show_values;
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
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
        if area.width < 3 || area.height < 1 {
            return;
        }

        let mut block = Block::default();

        if self.show_border {
            block = block.borders(Borders::ALL);

            // Apply border style
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

            // Apply border color
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

        // Now render the progress bar content
        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Calculate percentage
        let percentage = if self.max > 0 {
            (self.current as f64 / self.max as f64 * 100.0) as u32
        } else {
            0
        };

        // Build the display text with progressive simplification based on available space
        let available_width = inner_area.width;

        // If custom text is set, use it instead of values
        let display_text = if let Some(ref custom) = self.custom_text {
            // For custom text (like "clear as a bell" or "defensive" for stance),
            // just show the custom text as-is without any label prefix
            custom.clone()
        } else {
            // Build text options from most detailed to least detailed
            // Never prepend label - if needed, it should be in the border title
            let mut text_options = Vec::new();

            // Option 1: Values and percentage
            if self.show_values && self.show_percentage {
                text_options.push(format!("{}/{} ({}%)", self.current, self.max, percentage));
            }

            // Option 2: Just values
            if self.show_values {
                text_options.push(format!("{}/{}", self.current, self.max));
            }

            // Option 3: Just percentage
            if self.show_percentage {
                text_options.push(format!("{}%", percentage));
            }

            // Option 4: Just current value
            text_options.push(format!("{}", self.current));

            // Pick the first option that fits
            text_options.iter()
                .find(|text| text.len() as u16 <= available_width)
                .cloned()
                .unwrap_or_default()
        };

        let text_width = display_text.len() as u16;

        // ProfanityFE-style: Use background colors on text, not bar characters
        // The bar IS the colored background behind the text

        let bar_color = self.bar_color.as_ref().map(|c| Self::parse_color(c)).unwrap_or(Color::Green);
        let bg_color = self.background_color.as_ref().map(|c| Self::parse_color(c)).unwrap_or(Color::Reset);

        // Calculate split point based on percentage
        let split_position = ((percentage as f64 / 100.0) * available_width as f64) as u16;

        if text_width > 0 && text_width <= available_width {
            // Center the text
            let text_start_x = inner_area.x + (available_width.saturating_sub(text_width)) / 2;

            // First pass: Fill the background
            for i in 0..available_width {
                let x = inner_area.x + i;
                buf[(x, inner_area.y)].set_char(' ');
                if i < split_position {
                    // Filled portion - use bar color as background
                    buf[(x, inner_area.y)].set_bg(bar_color);
                } else if !self.transparent_background {
                    // Empty portion - use background color only if not transparent
                    buf[(x, inner_area.y)].set_bg(bg_color);
                }
                // If transparent_background is true, don't set background for empty portion
            }

            // Second pass: Render text on top with appropriate colors
            for (i, c) in display_text.chars().enumerate() {
                let x = text_start_x + i as u16;
                if x < inner_area.x + inner_area.width {
                    let char_position = x - inner_area.x;

                    if char_position < split_position {
                        // On filled portion: white text on colored background
                        buf[(x, inner_area.y)].set_char(c);
                        buf[(x, inner_area.y)].set_fg(Color::White);
                        buf[(x, inner_area.y)].set_bg(bar_color);
                    } else {
                        // On empty portion: gray text, background depends on transparent_background
                        buf[(x, inner_area.y)].set_char(c);
                        buf[(x, inner_area.y)].set_fg(Color::DarkGray);
                        if !self.transparent_background {
                            buf[(x, inner_area.y)].set_bg(bg_color);
                        }
                    }
                }
            }
        } else if available_width > 0 {
            // No text or text too wide - just show the colored bar
            for i in 0..available_width {
                let x = inner_area.x + i;
                buf[(x, inner_area.y)].set_char(' ');
                if i < split_position {
                    buf[(x, inner_area.y)].set_bg(bar_color);
                } else if !self.transparent_background {
                    buf[(x, inner_area.y)].set_bg(bg_color);
                }
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        // Progress bars don't really have focus behavior, just render normally
        self.render(area, buf);
    }
}
