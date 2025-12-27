//! GemStone IV experience widget.
//!
//! Displays GS4 experience data from the `expr` dialog:
//! - Level text (yourLvl label)
//! - Mind state progress bar (mindState)
//! - Experience progress bar (nextLvlPB)
//!
//! Reads data from GameState.gs4_experience (populated from dialogData updates).

use crate::core::state::GS4ExperienceState;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

/// GS4 Experience widget - shows level, mind state, and exp progress
pub struct GS4Experience {
    title: String,
    align: Alignment,
    /// Cached state for rendering
    level_text: String,
    mind_value: u32,
    mind_text: String,
    next_level_value: u32,
    next_level_text: String,
    /// Generation counter for change detection
    generation: u64,
    /// Border color
    border_color: Color,
    /// Text color
    text_color: Color,
    /// Mind bar fill color
    mind_bar_color: Color,
    /// Exp bar fill color (None = use theme background, for max-level users)
    exp_bar_color: Option<Color>,
    /// Whether to show the level label (yourLvl)
    show_level: bool,
    /// Whether to show the exp bar (nextLvlPB)
    show_exp_bar: bool,
    /// Background color (from theme)
    background_color: Option<Color>,
}

impl GS4Experience {
    pub fn new(title: &str, align: &str) -> Self {
        let alignment = match align.to_lowercase().as_str() {
            "center" | "centre" => Alignment::Center,
            "right" => Alignment::Right,
            _ => Alignment::Left,
        };

        Self {
            title: title.to_string(),
            align: alignment,
            level_text: String::new(),
            mind_value: 0,
            mind_text: String::new(),
            next_level_value: 0,
            next_level_text: String::new(),
            generation: 0,
            border_color: Color::White,
            text_color: Color::White,
            mind_bar_color: Color::Cyan,
            exp_bar_color: None, // Default to theme background for max-level users
            show_level: true,
            show_exp_bar: true,
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

    /// Set whether to show the level label
    pub fn set_show_level(&mut self, show: bool) {
        self.show_level = show;
    }

    /// Set whether to show the exp bar
    pub fn set_show_exp_bar(&mut self, show: bool) {
        self.show_exp_bar = show;
    }

    /// Set the background color (from theme)
    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color.and_then(|c| super::colors::parse_color_to_ratatui(&c));
    }

    /// Set the mind bar color
    pub fn set_mind_bar_color(&mut self, color: Color) {
        self.mind_bar_color = color;
    }

    /// Set the exp bar color (None = use theme background)
    pub fn set_exp_bar_color(&mut self, color: Option<Color>) {
        self.exp_bar_color = color;
    }

    /// Update the widget from GS4ExperienceState.
    /// Returns true if the display changed.
    pub fn update_from_state(&mut self, state: &GS4ExperienceState) -> bool {
        // Quick check: if generation matches, no update needed
        if self.generation == state.generation {
            return false;
        }

        self.generation = state.generation;
        self.level_text = state.level_text.clone();
        self.mind_value = state.mind_state_value;
        self.mind_text = state.mind_state_text.clone();
        self.next_level_value = state.next_level_value;
        self.next_level_text = state.next_level_text.clone();

        true
    }

    /// Render a simple progress bar within a single line
    /// fill_color: Some(color) = use that color for filled portion, None = use theme background
    fn render_bar(
        &self,
        area: Rect,
        buf: &mut Buffer,
        value: u32,
        text: &str,
        fill_color: Option<Color>,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let bar_width = area.width as usize;
        let filled_width = (bar_width as u32 * value.min(100) / 100) as usize;

        // Prepare display text, truncate if needed
        let display_text = if text.len() > bar_width {
            &text[..bar_width]
        } else {
            text
        };

        // Center the text
        let text_start = (bar_width.saturating_sub(display_text.len())) / 2;

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
                if let Some(fc) = fill_color {
                    // Has fill color - use it with black text for contrast
                    buf[(x, y)].set_fg(Color::Black);
                    buf[(x, y)].set_bg(fc);
                } else {
                    // No fill color (e.g., exp bar for max-level users) - use theme background
                    buf[(x, y)].set_fg(self.text_color);
                    if let Some(bg) = self.background_color {
                        buf[(x, y)].set_bg(bg);
                    }
                }
            } else {
                buf[(x, y)].set_char(ch);
                buf[(x, y)].set_fg(self.text_color);
                // Only set bg if we have a theme background, otherwise leave transparent
                if let Some(bg) = self.background_color {
                    buf[(x, y)].set_bg(bg);
                }
            }
        }
    }

    /// Render the experience widget
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

        // Create block with border and title
        let block = Block::default()
            .title(self.title.as_str())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.border_color));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // If no data, show placeholder
        if self.level_text.is_empty() && self.mind_text.is_empty() && self.next_level_text.is_empty() {
            let placeholder = Line::from(Span::styled(
                "(No experience data)",
                Style::default().fg(Color::DarkGray),
            ));
            let placeholder_text = ratatui::widgets::Paragraph::new(placeholder)
                .alignment(self.align);
            placeholder_text.render(inner, buf);
            return;
        }

        let mut current_y = inner.y;

        // Row 1: Level text (if show_level enabled)
        if self.show_level && inner.height > 0 && !self.level_text.is_empty() {
            let level_line = Line::from(Span::styled(
                self.level_text.clone(),
                Style::default().fg(self.text_color),
            ));
            let line_area = Rect {
                x: inner.x,
                y: current_y,
                width: inner.width,
                height: 1,
            };
            let para = ratatui::widgets::Paragraph::new(level_line).alignment(self.align);
            para.render(line_area, buf);
            current_y += 1;
        }

        // Row 2: Mind state bar
        if current_y < inner.y + inner.height && !self.mind_text.is_empty() {
            let bar_area = Rect {
                x: inner.x,
                y: current_y,
                width: inner.width,
                height: 1,
            };
            self.render_bar(bar_area, buf, self.mind_value, &self.mind_text, Some(self.mind_bar_color));
            current_y += 1;
        }

        // Row 3: Exp progress bar (if show_exp_bar enabled)
        if self.show_exp_bar && current_y < inner.y + inner.height && !self.next_level_text.is_empty() {
            let bar_area = Rect {
                x: inner.x,
                y: current_y,
                width: inner.width,
                height: 1,
            };
            self.render_bar(bar_area, buf, self.next_level_value, &self.next_level_text, self.exp_bar_color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_default_alignment() {
        let exp = GS4Experience::new("Experience", "left");
        assert_eq!(exp.title, "Experience");
        assert_eq!(exp.align, Alignment::Left);
        assert_eq!(exp.generation, 0);
    }

    #[test]
    fn test_new_center_alignment() {
        let exp = GS4Experience::new("Experience", "center");
        assert_eq!(exp.align, Alignment::Center);
    }

    #[test]
    fn test_new_centre_alignment() {
        let exp = GS4Experience::new("Experience", "centre");
        assert_eq!(exp.align, Alignment::Center);
    }

    #[test]
    fn test_new_right_alignment() {
        let exp = GS4Experience::new("Experience", "right");
        assert_eq!(exp.align, Alignment::Right);
    }

    #[test]
    fn test_update_from_state_no_change() {
        let mut exp = GS4Experience::new("Experience", "left");
        let state = GS4ExperienceState::default();

        // Default state with generation 0 matches exp.generation 0, so no change
        let changed = exp.update_from_state(&state);
        assert!(!changed);
    }

    #[test]
    fn test_update_from_state_with_change() {
        let mut exp = GS4Experience::new("Experience", "left");
        let mut state = GS4ExperienceState::default();
        state.generation = 1;
        state.level_text = "Level 100".to_string();
        state.mind_state_value = 50;
        state.mind_state_text = "clear as a bell".to_string();

        let changed = exp.update_from_state(&state);
        assert!(changed);
        assert_eq!(exp.generation, 1);
        assert_eq!(exp.level_text, "Level 100");
        assert_eq!(exp.mind_value, 50);
        assert_eq!(exp.mind_text, "clear as a bell");
    }

    #[test]
    fn test_update_from_state_caches_generation() {
        let mut exp = GS4Experience::new("Experience", "left");
        let mut state = GS4ExperienceState::default();
        state.generation = 5;

        exp.update_from_state(&state);
        assert_eq!(exp.generation, 5);

        // Same generation again - no update
        let changed = exp.update_from_state(&state);
        assert!(!changed);
    }

    #[test]
    fn test_set_border_color() {
        let mut exp = GS4Experience::new("Test", "left");
        assert_eq!(exp.border_color, Color::White);

        exp.set_border_color(Color::Red);
        assert_eq!(exp.border_color, Color::Red);
    }

    #[test]
    fn test_set_text_color() {
        let mut exp = GS4Experience::new("Test", "left");
        assert_eq!(exp.text_color, Color::White);

        exp.set_text_color(Color::Green);
        assert_eq!(exp.text_color, Color::Green);
    }
}
