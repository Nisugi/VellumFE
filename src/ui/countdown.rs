use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Widget},
};
use std::time::{SystemTime, UNIX_EPOCH};

/// A countdown widget for displaying roundtime, casttime, etc.
pub struct Countdown {
    label: String,
    end_time: u64,  // Unix timestamp when countdown ends
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: Option<Vec<String>>,
    bar_color: Option<String>,
    background_color: Option<String>,
    transparent_background: bool,  // If true, empty portion is transparent; if false, use background_color
    icon: char,  // Character to use for countdown blocks
    content_align: Option<String>,
}

impl Countdown {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            end_time: 0,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: None,
            bar_color: Some("#00ff00".to_string()),
            background_color: None,
            transparent_background: true, // Transparent by default
            icon: '\u{f0c8}',  // Default to Nerd Font square icon
            content_align: None,
        }
    }

    pub fn set_icon(&mut self, icon: char) {
        self.icon = icon;
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
        if bar_color.is_some() {
            self.bar_color = bar_color;
        }
        if background_color.is_some() {
            self.background_color = background_color;
        }
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    pub fn set_content_align(&mut self, align: Option<String>) {
        self.content_align = align;
    }

    pub fn set_end_time(&mut self, end_time: u64) {
        self.end_time = end_time;
    }

    /// Get remaining seconds
    /// Applies server_time_offset to local time to account for clock drift
    fn remaining_seconds(&self, server_time_offset: i64) -> i64 {
        let local_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let adjusted_time = local_time + server_time_offset;
        (self.end_time as i64) - adjusted_time
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

    pub fn render(&self, area: Rect, buf: &mut Buffer, server_time_offset: i64) {
        if area.width < 3 || area.height < 1 {
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

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Calculate content alignment offset (vertical only, like progress bars)
        const CONTENT_HEIGHT: u16 = 1;
        let row_offset = if let Some(ref align_str) = self.content_align {
            let align = crate::config::ContentAlign::from_str(align_str);
            let (offset, _) = align.calculate_offset(inner_area.width, CONTENT_HEIGHT, inner_area.width, inner_area.height);
            offset
        } else {
            0
        };

        let remaining = self.remaining_seconds(server_time_offset).max(0) as u32;

        let bar_color = self.bar_color.as_ref().map(|c| Self::parse_color(c)).unwrap_or(Color::Green);
        let bg_color = self.background_color.as_ref().map(|c| Self::parse_color(c)).unwrap_or(Color::Reset);

        // Clear the bar area
        let y = inner_area.y + row_offset;
        if y < buf.area().height {
            if !self.transparent_background {
                // Fill with background color if not transparent
                for i in 0..inner_area.width {
                    let x = inner_area.x + i;
                    if x < buf.area().width {
                        buf[(x, y)].set_char(' ');
                        buf[(x, y)].set_bg(bg_color);
                    }
                }
            } else {
                // Just clear with spaces, no background
                for i in 0..inner_area.width {
                    let x = inner_area.x + i;
                    if x < buf.area().width {
                        buf[(x, y)].set_char(' ');
                    }
                }
            }
        }

        // If countdown is 0, leave it blank (invisible)
        if remaining == 0 {
            return;
        }

        // Simple block-based countdown:
        // - Max 10 blocks
        // - Show N blocks where N = min(remaining_seconds, 10)
        const MAX_BLOCKS: u32 = 10;
        let blocks_to_show = remaining.min(MAX_BLOCKS);

        // Right-align the number so it doesn't shift when going from 10->9
        // Reserve 2 chars for the number + 1 for space = 3 total
        // Format: " 9 ████████" or "10 ████████"
        let remaining_text = format!("{:>2} ", remaining);
        let text_width = remaining_text.len() as u16; // Always 3 chars

        // Render countdown number on the left (right-aligned within 3 chars)
        let y = inner_area.y + row_offset;
        if y < buf.area().height {
            for (i, c) in remaining_text.chars().enumerate() {
                let x = inner_area.x + i as u16;
                if x < inner_area.x + inner_area.width && x < buf.area().width {
                    buf[(x, y)].set_char(c);
                    buf[(x, y)].set_fg(bar_color);
                    if !self.transparent_background {
                        buf[(x, y)].set_bg(bg_color);
                    }
                }
            }

            // Render blocks after the number
            for i in 0..blocks_to_show {
                let pos = text_width + i as u16;
                if pos < inner_area.width {
                    let x = inner_area.x + pos;
                    if x < buf.area().width {
                        buf[(x, y)].set_char(self.icon);
                        buf[(x, y)].set_fg(bar_color);
                        if !self.transparent_background {
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

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool, server_time_offset: i64) {
        self.render(area, buf, server_time_offset);
    }
}
