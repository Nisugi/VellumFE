//! Simple countdown timer widget that mirrors Profanity's RT/CT bars.
//!
//! Displays a numeric timer plus up to ten block glyphs so the user can gauge
//! duration at a glance.

use crate::frontend::tui::colors::parse_color_to_ratatui;
use crate::frontend::tui::crossterm_bridge;
use crate::frontend::tui::title_position::{self, TitlePosition};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use std::time::{SystemTime, UNIX_EPOCH};

/// A countdown widget for displaying roundtime, casttime, stuntime, etc.
pub struct Countdown {
    label: String,
    end_time: i64, // Unix timestamp when countdown ends
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: crate::config::BorderSides,
    title_position: TitlePosition,
    text_color: Option<String>,
    background_color: Option<String>,
    transparent_background: bool,
    icon: char, // Character to use for countdown blocks
}

impl Countdown {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            end_time: 0,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(),
            title_position: TitlePosition::TopLeft,
            text_color: None,
            background_color: None,
            transparent_background: false,
            icon: '█', // Default to filled block
        }
    }

    pub fn set_icon(&mut self, icon: char) {
        self.icon = icon;
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

    pub fn set_border_sides(&mut self, sides: crate::config::BorderSides) {
        self.border_sides = sides;
    }

    pub fn set_title_position(&mut self, position: String) {
        self.title_position = TitlePosition::from_str(&position);
    }

    pub fn set_title(&mut self, title: String) {
        self.label = title;
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color;
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    pub fn set_end_time(&mut self, end_time: i64) {
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
        self.end_time - adjusted_time
    }

    /// Parse a color string to ratatui Color (supports hex and color names)
    fn parse_color_opt(input: &str) -> Option<Color> {
        parse_color_to_ratatui(input)
    }

    pub fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        server_time_offset: i64,
        theme: &crate::theme::AppTheme,
    ) {
        if area.width < 3 || area.height < 1 {
            return;
        }

        // Determine background color - use theme background if not transparent
        let bg_color = if self.transparent_background {
            None
        } else if let Some(ref color) = self.background_color {
            Some(
                Self::parse_color_opt(color)
                    .unwrap_or_else(|| crossterm_bridge::to_ratatui_color(theme.window_background)),
            )
        } else {
            Some(crossterm_bridge::to_ratatui_color(theme.window_background))
        };

        let border_color = self
            .border_color
            .as_ref()
            .and_then(|c| Self::parse_color_opt(c))
            .unwrap_or_else(|| crossterm_bridge::to_ratatui_color(theme.window_border));

        // Fill the full area with background to avoid bleed-through when transparent is false
        if let Some(bg) = bg_color {
            for row in 0..area.height {
                for col in 0..area.width {
                    let x = area.x + col;
                    let y = area.y + row;
                    if x < buf.area().width && y < buf.area().height {
                        buf[(x, y)].set_bg(bg);
                    }
                }
            }
        }

        // Build border parameters
        let borders = crossterm_bridge::to_ratatui_borders(&self.border_sides);
        let border_type = match self.border_style.as_deref() {
            Some("double") => ratatui::widgets::BorderType::Double,
            Some("rounded") => ratatui::widgets::BorderType::Rounded,
            Some("thick") => ratatui::widgets::BorderType::Thick,
            Some("quadrant_inside") => ratatui::widgets::BorderType::QuadrantInside,
            Some("quadrant_outside") => ratatui::widgets::BorderType::QuadrantOutside,
            _ => ratatui::widgets::BorderType::Plain,
        };
        let border_style = Style::default()
            .fg(border_color)
            .bg(bg_color.unwrap_or(Color::Reset));

        // Render border/title respecting sides; obtain inner area
        let inner_area = title_position::render_block_with_title(
            area,
            buf,
            self.show_border,
            borders,
            &self.border_sides,
            border_type,
            border_style,
            &self.label,
            self.title_position,
        );

        // If inner area collapsed to zero, keep borders visible but skip content
        // (previously fell back to full area which overwrote borders)
        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        let remaining = self.remaining_seconds(server_time_offset).max(0) as u32;

        let text_color = self
            .text_color
            .as_ref()
            .and_then(|c| Self::parse_color_opt(c))
            .unwrap_or(Color::White);

        // Clear the bar area with appropriate background
        let y = inner_area.y;
        if y < buf.area().height {
            for i in 0..inner_area.width {
                let x = inner_area.x + i;
                if x < buf.area().width {
                    buf[(x, y)].set_char(' ');
                    if let Some(bg) = bg_color {
                        buf[(x, y)].set_bg(bg);
                    }
                }
            }
        }

        // If countdown is 0, leave it blank (invisible)
        if remaining == 0 {
            return;
        }

        // Right-align the number so it doesn't shift when going from 10->9
        // Reserve 2 chars for the number + 1 for space = 3 total
        // Format: " 9 ████████" or "10 ████████"
        let remaining_text = format!("{:>2} ", remaining);
        let text_width = remaining_text.len() as u16; // Always 3 chars

        // Dynamic block-based countdown - adapts to widget width
        // Calculate max blocks based on available space after the number
        let max_blocks = if inner_area.width > text_width {
            (inner_area.width - text_width) as u32
        } else {
            0
        };
        let blocks_to_show = remaining.min(max_blocks);

        // Render countdown number on the left (right-aligned within 3 chars)
        let y = inner_area.y;
        if y < buf.area().height {
            for (i, c) in remaining_text.chars().enumerate() {
                let x = inner_area.x + i as u16;
                if x < inner_area.x + inner_area.width && x < buf.area().width {
                    buf[(x, y)].set_char(c);
                    buf[(x, y)].set_fg(text_color);
                    if let Some(bg) = bg_color {
                        buf[(x, y)].set_bg(bg);
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
                        buf[(x, y)].set_fg(text_color);
                        if let Some(bg) = bg_color {
                            buf[(x, y)].set_bg(bg);
                        }
                    }
                }
            }
        }
    }
}

