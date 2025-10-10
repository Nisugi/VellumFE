use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};
use std::time::{SystemTime, UNIX_EPOCH};

/// A countdown widget for displaying roundtime, casttime, etc.
pub struct Countdown {
    label: String,
    end_time: u64,  // Unix timestamp when countdown ends
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    bar_color: Option<String>,
    background_color: Option<String>,
    transparent_background: bool,  // If true, empty portion is transparent; if false, use background_color
    icon: char,  // Character to use for countdown blocks
}

impl Countdown {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            end_time: 0,
            show_border: true,
            border_style: None,
            border_color: None,
            bar_color: Some("#00ff00".to_string()),
            background_color: None,
            transparent_background: true, // Transparent by default
            icon: '\u{f0c8}',  // Default to Nerd Font square icon
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

    pub fn set_end_time(&mut self, end_time: u64) {
        self.end_time = end_time;
    }

    /// Get remaining seconds
    fn remaining_seconds(&self) -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        (self.end_time as i64) - (now as i64)
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

        let remaining = self.remaining_seconds().max(0) as u32;

        let bar_color = self.bar_color.as_ref().map(|c| Self::parse_color(c)).unwrap_or(Color::Green);
        let bg_color = self.background_color.as_ref().map(|c| Self::parse_color(c)).unwrap_or(Color::Reset);

        // Clear the bar area
        if !self.transparent_background {
            // Fill with background color if not transparent
            for i in 0..inner_area.width {
                let x = inner_area.x + i;
                buf[(x, inner_area.y)].set_char(' ');
                buf[(x, inner_area.y)].set_bg(bg_color);
            }
        } else {
            // Just clear with spaces, no background
            for i in 0..inner_area.width {
                let x = inner_area.x + i;
                buf[(x, inner_area.y)].set_char(' ');
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
        for (i, c) in remaining_text.chars().enumerate() {
            let x = inner_area.x + i as u16;
            if x < inner_area.x + inner_area.width {
                buf[(x, inner_area.y)].set_char(c);
                buf[(x, inner_area.y)].set_fg(bar_color);
                if !self.transparent_background {
                    buf[(x, inner_area.y)].set_bg(bg_color);
                }
            }
        }

        // Render blocks after the number
        for i in 0..blocks_to_show {
            let pos = text_width + i as u16;
            if pos < inner_area.width {
                let x = inner_area.x + pos;
                buf[(x, inner_area.y)].set_char(self.icon);
                buf[(x, inner_area.y)].set_fg(bar_color);
                if !self.transparent_background {
                    buf[(x, inner_area.y)].set_bg(bg_color);
                }
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}
