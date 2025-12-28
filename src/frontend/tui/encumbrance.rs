//! Encumbrance widget.
//!
//! Displays encumbrance data from the `encum` dialog:
//! - Progress bar (encumlevel)
//! - Descriptive blurb text (encumblurb) - optional
//!
//! Reads data from GameState.encumbrance (populated from dialogData updates).

use crate::config::BorderSides;
use crate::core::state::EncumbranceState;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Widget},
};

/// Encumbrance widget - shows progress bar and optional description
pub struct Encumbrance {
    title: String,
    align: Alignment,
    /// Whether to show the descriptive blurb
    show_label: bool,
    /// Whether to show the title
    show_title: bool,
    /// Whether to show the border
    show_border: bool,
    /// Which border sides to show
    border_sides: BorderSides,
    /// Cached state for rendering
    value: u32,
    text: String,
    blurb: String,
    /// Generation counter for change detection
    generation: u64,
    /// Border color
    border_color: Color,
    /// Text color
    text_color: Color,
    /// Bar color for light encumbrance (0-20)
    color_light: Color,
    /// Bar color for moderate encumbrance (21-50)
    color_moderate: Color,
    /// Bar color for heavy encumbrance (51-80)
    color_heavy: Color,
    /// Bar color for critical encumbrance (81-100)
    color_critical: Color,
    /// Background color (from theme)
    background_color: Option<Color>,
}

impl Encumbrance {
    pub fn new(title: &str, align: &str, show_label: bool) -> Self {
        let alignment = match align.to_lowercase().as_str() {
            "center" | "centre" => Alignment::Center,
            "right" => Alignment::Right,
            _ => Alignment::Left,
        };

        Self {
            title: title.to_string(),
            align: alignment,
            show_label,
            show_title: true,
            show_border: true,
            border_sides: BorderSides::default(),
            value: 0,
            text: String::new(),
            blurb: String::new(),
            generation: 0,
            border_color: Color::White,
            text_color: Color::White,
            color_light: Color::Green,
            color_moderate: Color::Yellow,
            color_heavy: Color::Rgb(255, 165, 0), // Orange
            color_critical: Color::Red,
            background_color: None,
        }
    }

    /// Set the border color
    pub fn set_border_color(&mut self, color: Color) {
        self.border_color = color;
    }

    /// Set the text color
    pub fn set_text_color(&mut self, color: Color) {
        self.text_color = color;
    }

    /// Set the background color (from theme)
    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color.and_then(|c| super::colors::parse_color_to_ratatui(&c));
    }

    /// Set whether to show the label
    pub fn set_show_label(&mut self, show: bool) {
        self.show_label = show;
    }

    /// Set whether to show the title
    pub fn set_show_title(&mut self, show: bool) {
        self.show_title = show;
    }

    /// Set whether to show the border
    pub fn set_show_border(&mut self, show: bool) {
        self.show_border = show;
    }

    /// Set which border sides to show
    pub fn set_border_sides(&mut self, sides: BorderSides) {
        self.border_sides = sides;
    }

    /// Set the light encumbrance color (0-20)
    pub fn set_color_light(&mut self, color: Color) {
        self.color_light = color;
    }

    /// Set the moderate encumbrance color (21-50)
    pub fn set_color_moderate(&mut self, color: Color) {
        self.color_moderate = color;
    }

    /// Set the heavy encumbrance color (51-80)
    pub fn set_color_heavy(&mut self, color: Color) {
        self.color_heavy = color;
    }

    /// Set the critical encumbrance color (81-100)
    pub fn set_color_critical(&mut self, color: Color) {
        self.color_critical = color;
    }

    /// Get the bar color based on current encumbrance value
    fn get_bar_color(&self) -> Color {
        match self.value {
            0..=20 => self.color_light,
            21..=50 => self.color_moderate,
            51..=80 => self.color_heavy,
            _ => self.color_critical,
        }
    }

    /// Update the widget from EncumbranceState.
    /// Returns true if the display changed.
    pub fn update_from_state(&mut self, state: &EncumbranceState) -> bool {
        // Quick check: if generation matches, no update needed
        if self.generation == state.generation {
            return false;
        }

        self.generation = state.generation;
        self.value = state.value;
        self.text = state.text.clone();
        self.blurb = state.blurb.clone();

        true
    }

    /// Render a simple progress bar within a single line
    fn render_bar(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let bar_width = area.width as usize;
        let filled_width = (bar_width as u32 * self.value.min(100) / 100) as usize;

        // Prepare display text, truncate if needed
        let display_text = if self.text.len() > bar_width {
            &self.text[..bar_width]
        } else {
            &self.text
        };

        // Center the text
        let text_start = (bar_width.saturating_sub(display_text.len())) / 2;

        // Get the bar color based on encumbrance level
        let bar_color = self.get_bar_color();

        // Unfilled background: use theme background or transparent (no change)
        let unfilled_bg = self.background_color;

        for col in 0..bar_width {
            let x = area.x + col as u16;
            let y = area.y;

            if x >= buf.area().width || y >= buf.area().height {
                continue;
            }

            let is_filled = col < filled_width;

            // Determine character at this position
            let ch = if col >= text_start && col < text_start + display_text.len() {
                display_text.chars().nth(col - text_start).unwrap_or(' ')
            } else {
                ' '
            };

            if is_filled {
                buf[(x, y)].set_char(ch);
                buf[(x, y)].set_fg(self.text_color);
                buf[(x, y)].set_bg(bar_color);
            } else {
                buf[(x, y)].set_char(ch);
                buf[(x, y)].set_fg(self.text_color);
                // Only set bg if we have a theme background, otherwise leave transparent
                if let Some(bg) = unfilled_bg {
                    buf[(x, y)].set_bg(bg);
                }
            }
        }
    }

    /// Render the encumbrance widget
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Apply background color to full area (including borders) before rendering block
        if let Some(bg_color) = self.background_color {
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(bg_color);
                    }
                }
            }
        }

        // Conditionally create block with border and/or title
        let inner = if self.show_border && self.border_sides.any() {
            let borders = super::crossterm_bridge::to_ratatui_borders(&self.border_sides);
            let mut block = Block::default()
                .borders(borders)
                .border_style(Style::default().fg(self.border_color));
            if self.show_title {
                block = block.title(self.title.as_str());
            }
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // If no data, show placeholder
        if self.text.is_empty() && self.blurb.is_empty() {
            let placeholder = Line::from(Span::styled(
                "(No encumbrance data)",
                Style::default().fg(Color::DarkGray),
            ));
            let placeholder_text = ratatui::widgets::Paragraph::new(placeholder)
                .alignment(self.align);
            placeholder_text.render(inner, buf);
            return;
        }

        let mut current_y = inner.y;

        // Row 1: Progress bar
        if inner.height > 0 {
            let bar_area = Rect {
                x: inner.x,
                y: current_y,
                width: inner.width,
                height: 1,
            };
            self.render_bar(bar_area, buf);
            current_y += 1;
        }

        // Row 2: Blurb text (if enabled and available)
        if self.show_label && current_y < inner.y + inner.height && !self.blurb.is_empty() {
            let blurb_line = Line::from(Span::styled(
                self.blurb.clone(),
                Style::default().fg(self.text_color),
            ));
            let line_area = Rect {
                x: inner.x,
                y: current_y,
                width: inner.width,
                height: 1,
            };
            let para = ratatui::widgets::Paragraph::new(blurb_line).alignment(self.align);
            para.render(line_area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_default_alignment() {
        let enc = Encumbrance::new("Encumbrance", "left", true);
        assert_eq!(enc.title, "Encumbrance");
        assert_eq!(enc.align, Alignment::Left);
        assert!(enc.show_label);
        assert_eq!(enc.generation, 0);
    }

    #[test]
    fn test_new_center_alignment() {
        let enc = Encumbrance::new("Encumbrance", "center", true);
        assert_eq!(enc.align, Alignment::Center);
    }

    #[test]
    fn test_update_from_state_no_change() {
        let mut enc = Encumbrance::new("Encumbrance", "left", true);
        let state = EncumbranceState::default();

        // Default state with generation 0 matches enc.generation 0, so no change
        let changed = enc.update_from_state(&state);
        assert!(!changed);
    }

    #[test]
    fn test_update_from_state_with_change() {
        let mut enc = Encumbrance::new("Encumbrance", "left", true);
        let mut state = EncumbranceState::default();
        state.generation = 1;
        state.value = 50;
        state.text = "Moderate".to_string();
        state.blurb = "You are somewhat encumbered.".to_string();

        let changed = enc.update_from_state(&state);
        assert!(changed);
        assert_eq!(enc.generation, 1);
        assert_eq!(enc.value, 50);
        assert_eq!(enc.text, "Moderate");
        assert_eq!(enc.blurb, "You are somewhat encumbered.");
    }

    #[test]
    fn test_set_border_color() {
        let mut enc = Encumbrance::new("Test", "left", true);
        assert_eq!(enc.border_color, Color::White);

        enc.set_border_color(Color::Red);
        assert_eq!(enc.border_color, Color::Red);
    }

    #[test]
    fn test_set_text_color() {
        let mut enc = Encumbrance::new("Test", "left", true);
        assert_eq!(enc.text_color, Color::White);

        enc.set_text_color(Color::Green);
        assert_eq!(enc.text_color, Color::Green);
    }

    #[test]
    fn test_color_ranges() {
        let mut enc = Encumbrance::new("Test", "left", true);

        // Light (0-20)
        enc.value = 0;
        assert_eq!(enc.get_bar_color(), Color::Green);
        enc.value = 20;
        assert_eq!(enc.get_bar_color(), Color::Green);

        // Moderate (21-50)
        enc.value = 21;
        assert_eq!(enc.get_bar_color(), Color::Yellow);
        enc.value = 50;
        assert_eq!(enc.get_bar_color(), Color::Yellow);

        // Heavy (51-80)
        enc.value = 51;
        assert_eq!(enc.get_bar_color(), Color::Rgb(255, 165, 0));
        enc.value = 80;
        assert_eq!(enc.get_bar_color(), Color::Rgb(255, 165, 0));

        // Critical (81-100)
        enc.value = 81;
        assert_eq!(enc.get_bar_color(), Color::Red);
        enc.value = 100;
        assert_eq!(enc.get_bar_color(), Color::Red);
    }

    #[test]
    fn test_default_colors() {
        let enc = Encumbrance::new("Test", "left", true);
        assert_eq!(enc.color_light, Color::Green);
        assert_eq!(enc.color_moderate, Color::Yellow);
        assert_eq!(enc.color_heavy, Color::Rgb(255, 165, 0));
        assert_eq!(enc.color_critical, Color::Red);
    }
}
