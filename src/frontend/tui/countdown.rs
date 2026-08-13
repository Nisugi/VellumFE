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
    icon: char,          // Character to use for countdown blocks
    show_when_zero: bool, // Keep "label 0" visible at rest instead of hiding
    count_past_zero: bool, // Run negative after expiry (window timers like pulse)
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
            show_when_zero: false,
            count_past_zero: false,
        }
    }

    pub fn set_icon(&mut self, icon: char) {
        self.icon = icon;
    }

    /// When true, the timer stays visible at rest showing "label  0" (no
    /// blocks) instead of hiding when it reaches zero.
    pub fn set_show_when_zero(&mut self, show: bool) {
        self.show_when_zero = show;
    }

    /// When true, an expired timer keeps counting (-1, -2, ...) instead of
    /// clamping at 0 - for timers whose expiry is a window, not a moment
    /// (the pulse clock reads 0 at the earliest arrival and runs negative
    /// through the min..max window).
    pub fn set_count_past_zero(&mut self, count: bool) {
        self.count_past_zero = count;
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

    /// Get remaining whole seconds, ceiling-rounded ("time until free").
    ///
    /// Applies server_time_offset to local time to account for clock drift.
    /// Uses millisecond precision for "now" so we don't add up to ~1s of
    /// floor bias on top of the server's whole-second RT/CT timestamp; the
    /// old `.as_secs()` truncation biased "now" earlier, making the timer
    /// read high. Ceiling means a displayed "1" persists until RT actually
    /// clears, then blanks within one ~16ms render frame.
    fn remaining_seconds(&self, server_time_offset: i64) -> i64 {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        Self::remaining_seconds_from(self.end_time, server_time_offset, now_ms)
    }

    /// Pure ceiling math, split out so the boundary behavior is testable
    /// without depending on the wall clock. `end_time` and
    /// `server_time_offset` are whole-second server-domain values;
    /// `now_ms` carries millisecond precision.
    fn remaining_seconds_from(end_time: i64, server_time_offset: i64, now_ms: i64) -> i64 {
        // Lift the whole-second values into the millisecond domain so we
        // don't add floor bias on top of the server's 1s granularity.
        let remaining_ms = end_time * 1000 - (now_ms + server_time_offset * 1000);
        if remaining_ms <= 0 {
            0
        } else {
            (remaining_ms + 999) / 1000 // integer ceiling
        }
    }

    /// Signed variant for count-past-zero timers: positive future values
    /// ceiling like [`Self::remaining_seconds_from`]; overdue values floor
    /// (0 for the first second past expiry, then -1, -2, ...).
    fn remaining_seconds_signed_from(end_time: i64, server_time_offset: i64, now_ms: i64) -> i64 {
        let remaining_ms = end_time * 1000 - (now_ms + server_time_offset * 1000);
        if remaining_ms >= 0 {
            (remaining_ms + 999) / 1000
        } else {
            // Whole seconds overdue: 1..999ms -> 0, 1000..1999ms -> -1, ...
            -((-remaining_ms) / 1000)
        }
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

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let signed = if self.count_past_zero && self.end_time > 0 {
            Self::remaining_seconds_signed_from(self.end_time, server_time_offset, now_ms)
        } else {
            self.remaining_seconds(server_time_offset).max(0)
        };
        let remaining = signed.max(0) as u32;

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

        // If countdown is 0, leave it blank (invisible) unless configured to
        // stay visible — then fall through to render " 0" with no blocks
        // (blocks_to_show computes to 0 for remaining == 0).
        // Count-past-zero timers with a real end time never blank: the
        // negative depth IS the information.
        if remaining == 0 && !self.show_when_zero && !(self.count_past_zero && signed < 0) {
            return;
        }

        // Right-align the number so it doesn't shift when going from 10->9
        // Reserve 2 chars for the number + 1 for space = 3 total
        // Format: " 9 ████████" or "10 ████████" (or "-4 " when overdue)
        let remaining_text = if signed < 0 {
            format!("{:>2} ", signed.max(-99))
        } else {
            format!("{:>2} ", remaining)
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn buffer_line(buf: &Buffer, y: u16, width: u16) -> String {
        let mut line = String::new();
        for x in 0..width {
            line.push_str(buf[(x, y)].symbol());
        }
        line
    }

    fn now_seconds() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn test_remaining_seconds_ceilings_partial() {
        // Deterministic boundary check on the pure ceiling helper.
        // end = 110s, offset 0; sweep "now" across the final two seconds.
        let end = 110;
        // 1001ms remaining -> ceilings to 2.
        assert_eq!(Countdown::remaining_seconds_from(end, 0, 108_999), 2);
        // Exactly 1000ms remaining -> 1.
        assert_eq!(Countdown::remaining_seconds_from(end, 0, 109_000), 1);
        // 1ms remaining -> still 1.
        assert_eq!(Countdown::remaining_seconds_from(end, 0, 109_999), 1);
        // 0ms remaining -> 0.
        assert_eq!(Countdown::remaining_seconds_from(end, 0, 110_000), 0);
        // Past end -> clamped to 0, never negative.
        assert_eq!(Countdown::remaining_seconds_from(end, 0, 200_000), 0);
    }

    #[test]
    fn test_remaining_seconds_applies_server_offset() {
        // Server clock 5s ahead: end 110s, now 100_000ms, offset +5 -> 5s.
        assert_eq!(Countdown::remaining_seconds_from(110, 5, 100_000), 5);
    }

    #[test]
    fn test_render_blank_when_elapsed() {
        let mut countdown = Countdown::new("RT");
        countdown.set_border_config(false, None, None);
        countdown.set_end_time(now_seconds() - 1);

        let area = Rect::new(0, 0, 8, 1);
        let mut buf = Buffer::empty(area);
        let theme = crate::theme::AppTheme::default();
        countdown.render(area, &mut buf, 0, &theme);

        let line = buffer_line(&buf, 0, area.width);
        assert!(line.trim().is_empty());
    }

    #[test]
    fn test_render_shows_blocks_when_active() {
        let mut countdown = Countdown::new("RT");
        countdown.set_border_config(false, None, None);
        countdown.set_icon('#');
        countdown.set_end_time(now_seconds() + 60);

        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        let theme = crate::theme::AppTheme::default();
        countdown.render(area, &mut buf, 0, &theme);

        let line = buffer_line(&buf, 0, area.width);
        assert!(line.contains('#'));
    }

    #[test]
    fn test_render_zero_visible_when_show_when_zero() {
        let mut countdown = Countdown::new("RT");
        countdown.set_border_config(false, None, None);
        countdown.set_show_when_zero(true);
        countdown.set_end_time(now_seconds() - 1); // elapsed => remaining 0

        let area = Rect::new(0, 0, 8, 1);
        let mut buf = Buffer::empty(area);
        let theme = crate::theme::AppTheme::default();
        countdown.render(area, &mut buf, 0, &theme);

        // Shows the right-aligned "0" (no blocks) instead of going blank.
        let line = buffer_line(&buf, 0, area.width);
        assert_eq!(line.trim(), "0", "got: {:?}", line);
    }

    #[test]
    fn test_render_zero_hidden_by_default() {
        // Default (show_when_zero = false) still blanks at rest.
        let mut countdown = Countdown::new("RT");
        countdown.set_border_config(false, None, None);
        countdown.set_end_time(now_seconds() - 1);

        let area = Rect::new(0, 0, 8, 1);
        let mut buf = Buffer::empty(area);
        let theme = crate::theme::AppTheme::default();
        countdown.render(area, &mut buf, 0, &theme);

        assert!(buffer_line(&buf, 0, area.width).trim().is_empty());
    }

    #[test]
    fn test_signed_math_counts_whole_seconds_overdue() {
        // end = 100s. 999ms overdue -> 0; 1000ms -> -1; 29s -> -29.
        assert_eq!(Countdown::remaining_seconds_signed_from(100, 0, 100_999), 0);
        assert_eq!(Countdown::remaining_seconds_signed_from(100, 0, 101_000), -1);
        assert_eq!(Countdown::remaining_seconds_signed_from(100, 0, 129_000), -29);
        // Future values still ceiling like the clamped helper.
        assert_eq!(Countdown::remaining_seconds_signed_from(100, 0, 98_999), 2);
    }

    #[test]
    fn test_render_negative_when_count_past_zero() {
        let mut countdown = Countdown::new("Pulse");
        countdown.set_border_config(false, None, None);
        countdown.set_count_past_zero(true);
        countdown.set_end_time(now_seconds() - 4); // ~4s overdue

        let area = Rect::new(0, 0, 8, 1);
        let mut buf = Buffer::empty(area);
        let theme = crate::theme::AppTheme::default();
        countdown.render(area, &mut buf, 0, &theme);

        let line = buffer_line(&buf, 0, area.width);
        let shown = line.trim();
        assert!(
            shown == "-4" || shown == "-3" || shown == "-5",
            "negative overdue rendered, got {:?}",
            line
        );
    }

    #[test]
    fn test_background_color_fills_area() {
        let mut countdown = Countdown::new("RT");
        countdown.set_border_config(false, None, None);
        countdown.set_background_color(Some("#ff0000".to_string()));
        countdown.set_end_time(now_seconds() - 1);

        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::empty(area);
        let theme = crate::theme::AppTheme::default();
        countdown.render(area, &mut buf, 0, &theme);

        assert_eq!(buf[(0, 0)].bg, Color::Rgb(255, 0, 0));
    }
}
