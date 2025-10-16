use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Widget},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

/// A progress bar widget for displaying vitals, spell durations, etc.
pub struct ProgressBar {
    label: String,
    current: u32,
    max: u32,
    custom_text: Option<String>,  // Custom text to display instead of values (e.g., "clear as a bell")
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: Option<Vec<String>>,  // Which borders to show: ["top", "bottom", "left", "right"]
    bar_color: Option<String>,
    background_color: Option<String>,
    transparent_background: bool,  // If true, unfilled portion is transparent; if false, use background_color
    text_color: Option<String>,  // Text color for bar content (default: white on filled, gray on empty)
    show_percentage: bool,
    show_values: bool,
    text_alignment: TextAlignment,  // How to align the text (left, center, right)
    content_align: Option<String>,  // Alignment of bar within widget area (top, center, bottom, etc.)
    numbers_only: bool,  // If true, strip words from text (e.g., "health 325/326" -> "325/326")
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
            border_sides: None,  // Default: all borders
            bar_color: Some("#00ff00".to_string()), // Green by default
            background_color: None,
            transparent_background: true, // Transparent by default
            text_color: None,  // Default: white on filled, gray on empty
            show_percentage: true,
            show_values: true,
            text_alignment: TextAlignment::Center,  // Center by default (for vitals)
            content_align: None,
            numbers_only: false,  // Default: show full text
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

    pub fn set_text_alignment(&mut self, alignment: TextAlignment) {
        self.text_alignment = alignment;
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color;
    }

    pub fn set_content_align(&mut self, align: Option<String>) {
        self.content_align = align;
    }

    pub fn set_numbers_only(&mut self, numbers_only: bool) {
        self.numbers_only = numbers_only;
    }

    /// Parse a hex color string to ratatui Color
    pub fn parse_color(hex: &str) -> Color {
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
        // Only enforce minimum width if we have a border (needs space for border chars)
        if (self.show_border && area.width < 3) || area.height < 1 {
            return;
        }

        // Without border, allow even 1-char width for very narrow windows
        if !self.show_border && area.width == 0 {
            return;
        }

        // Determine which borders to show
        let borders = if self.show_border {
            crate::config::parse_border_sides(&self.border_sides)
        } else {
            ratatui::widgets::Borders::NONE
        };

        let border_color = self.border_color.as_ref()
            .map(|c| Self::parse_color(c))
            .unwrap_or(Color::White);

        // Check if we only have left/right borders (no top/bottom)
        let only_horizontal_borders = self.show_border &&
            (borders.contains(ratatui::widgets::Borders::LEFT) || borders.contains(ratatui::widgets::Borders::RIGHT)) &&
            !borders.contains(ratatui::widgets::Borders::TOP) &&
            !borders.contains(ratatui::widgets::Borders::BOTTOM);

        let inner_area: Rect;

        if only_horizontal_borders {
            // For left/right only borders, we'll manually render them on the content row
            // Don't use Block widget at all - just calculate the inner area manually
            let has_left = borders.contains(ratatui::widgets::Borders::LEFT);
            let has_right = borders.contains(ratatui::widgets::Borders::RIGHT);
            let border_width = (if has_left { 1 } else { 0 }) + (if has_right { 1 } else { 0 });

            inner_area = Rect {
                x: area.x + (if has_left { 1 } else { 0 }),
                y: area.y,
                width: area.width.saturating_sub(border_width),
                height: area.height,
            };
            // We'll render the borders later after we know which row has content
        } else if self.show_border {
            // Use Block widget for all other border combinations
            let mut block = Block::default().borders(borders);

            // Apply border style
            if let Some(ref style) = self.border_style {
                use ratatui::widgets::BorderType;
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
            block = block.title(self.label.as_str());

            inner_area = block.inner(area);
            block.render(area, buf);
        } else {
            inner_area = area;
        }

        // Now render the progress bar content
        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Calculate content alignment offset
        // Progress bar content is 1 row high, fills width
        const CONTENT_HEIGHT: u16 = 1;
        let row_offset = if let Some(ref align_str) = self.content_align {
            let align = crate::config::ContentAlign::from_str(align_str);
            let (offset, _) = align.calculate_offset(inner_area.width, CONTENT_HEIGHT, inner_area.width, inner_area.height);
            offset
        } else {
            0
        };

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

        // Strip words if numbers_only is enabled (e.g., "health 325/326" -> "325/326", "blood 100" -> "100")
        let display_text = if self.numbers_only {
            // Find the first digit and take everything from there
            if let Some(pos) = display_text.find(|c: char| c.is_ascii_digit()) {
                display_text[pos..].to_string()
            } else {
                // No digits found, return as-is
                display_text
            }
        } else {
            display_text
        };

        let text_width = display_text.len() as u16;

        // ProfanityFE-style: Use background colors on text, not bar characters
        // The bar IS the colored background behind the text

        let bar_color = self.bar_color.as_ref().map(|c| Self::parse_color(c)).unwrap_or(Color::Green);
        let bg_color = self.background_color.as_ref().map(|c| Self::parse_color(c)).unwrap_or(Color::Reset);

        // Calculate split point based on percentage
        let split_position = ((percentage as f64 / 100.0) * available_width as f64) as u16;

        if text_width > 0 {
            // Truncate text if it's too wide for available space
            let (final_text, final_text_width) = if text_width > available_width {
                // Text is too wide, truncate from left by removing complete words
                // For "health 325/326" we want to keep "325/326" not "ealth 325/326" or "health 3"
                if available_width > 0 {
                    // Split into words and remove from the left until it fits
                    let words: Vec<&str> = display_text.split_whitespace().collect();
                    let mut result = String::new();

                    // Try progressively removing words from the left
                    for i in 0..words.len() {
                        let candidate = words[i..].join(" ");
                        let candidate_width = candidate.chars().count() as u16;
                        if candidate_width <= available_width {
                            result = candidate;
                            break;
                        }
                    }

                    // If even the last word is too long, truncate it from the left
                    if result.is_empty() && !words.is_empty() {
                        let last_word = words[words.len() - 1];
                        let char_count = last_word.chars().count();
                        let skip_count = char_count.saturating_sub(available_width as usize);
                        result = last_word.chars().skip(skip_count).collect();
                    }

                    let result_width = result.chars().count() as u16;
                    (result, result_width)
                } else {
                    (String::new(), 0)
                }
            } else {
                // Text fits, use as-is
                (display_text.clone(), text_width)
            };

            if final_text_width > 0 && final_text_width <= available_width {
                // Calculate text position based on alignment
                let text_start_x = match self.text_alignment {
                    TextAlignment::Left => inner_area.x,
                    TextAlignment::Center => inner_area.x + (available_width.saturating_sub(final_text_width)) / 2,
                    TextAlignment::Right => inner_area.x + available_width.saturating_sub(final_text_width),
                };

                // First pass: Fill the background
                let y = inner_area.y + row_offset;
                if y < buf.area().height {
                    for i in 0..available_width {
                        let x = inner_area.x + i;
                        if x < buf.area().width {
                            buf[(x, y)].set_char(' ');
                            if i < split_position {
                                // Filled portion - use bar color as background
                                buf[(x, y)].set_bg(bar_color);
                            } else if !self.transparent_background {
                                // Empty portion - use background color only if not transparent
                                buf[(x, y)].set_bg(bg_color);
                            }
                            // If transparent_background is true, don't set background for empty portion
                        }
                    }
                }

                // Second pass: Render text on top with appropriate colors
                let y = inner_area.y + row_offset;
                if y < buf.area().height {
                    for (i, c) in final_text.chars().enumerate() {
                        let x = text_start_x + i as u16;
                        if x < inner_area.x + inner_area.width && x < buf.area().width {
                            let char_position = x - inner_area.x;

                            if char_position < split_position {
                                // On filled portion: configurable text color (default white) on colored background
                                buf[(x, y)].set_char(c);
                                let text_fg = self.text_color.as_ref()
                                    .map(|c| Self::parse_color(c))
                                    .unwrap_or(Color::White);
                                buf[(x, y)].set_fg(text_fg);
                                buf[(x, y)].set_bg(bar_color);
                            } else {
                                // On empty portion: configurable text color (default gray), background depends on transparent_background
                                buf[(x, y)].set_char(c);
                                let text_fg = self.text_color.as_ref()
                                    .map(|c| Self::parse_color(c))
                                    .unwrap_or(Color::DarkGray);
                                buf[(x, y)].set_fg(text_fg);
                                if !self.transparent_background {
                                    buf[(x, y)].set_bg(bg_color);
                                }
                            }
                        }
                    }
                }
            }
        } else if available_width > 0 {
            // No text - just show the colored bar
            let y = inner_area.y + row_offset;
            if y < buf.area().height {
                for i in 0..available_width {
                    let x = inner_area.x + i;
                    if x < buf.area().width {
                        buf[(x, y)].set_char(' ');
                        if i < split_position {
                            buf[(x, y)].set_bg(bar_color);
                        } else if !self.transparent_background {
                            buf[(x, y)].set_bg(bg_color);
                        }
                    }
                }
            }
        }

        // If we have left/right only borders, render them manually on the content row
        if only_horizontal_borders {
            let content_y = inner_area.y + row_offset;
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
        // Progress bars don't really have focus behavior, just render normally
        self.render(area, buf);
    }
}
