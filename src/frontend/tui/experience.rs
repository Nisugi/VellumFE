//! DragonRealms experience widget.
//!
//! Displays skill/experience components from `<component id='exp XXX'>` tags.
//! Reads data from GameState.exp_components (populated at login and updated on changes).

use crate::core::state::ExpComponentState;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

pub struct Experience {
    title: String,
    align: Alignment,
    /// Cached lines for rendering
    lines: Vec<Line<'static>>,
    /// Generation counter for change detection
    generation: u64,
    /// Border color
    border_color: Color,
    /// Text color
    text_color: Color,
}

impl Experience {
    pub fn new(title: &str, align: &str) -> Self {
        let alignment = match align.to_lowercase().as_str() {
            "center" | "centre" => Alignment::Center,
            "right" => Alignment::Right,
            _ => Alignment::Left,
        };

        Self {
            title: title.to_string(),
            align: alignment,
            lines: Vec::new(),
            generation: 0,
            border_color: Color::White,
            text_color: Color::White,
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

    /// Update the widget from ExpComponentState.
    /// Returns true if the display changed.
    pub fn update_from_state(&mut self, exp_state: &ExpComponentState) -> bool {
        // Quick check: if generation matches, no update needed
        if self.generation == exp_state.generation {
            return false;
        }

        self.generation = exp_state.generation;
        self.lines.clear();

        // Get fields with values in order
        for (field_name, value) in exp_state.fields_with_values() {
            // Create a line: "FieldName: value"
            let line = Line::from(vec![
                Span::styled(
                    format!("{}: ", field_name),
                    Style::default().fg(self.text_color),
                ),
                Span::styled(value.to_string(), Style::default().fg(self.text_color)),
            ]);
            self.lines.push(line);
        }

        true
    }

    /// Render the experience widget
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Create block with border and title
        let block = Block::default()
            .title(self.title.as_str())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.border_color));

        // If no data, show a placeholder message
        let lines: Vec<Line> = if self.lines.is_empty() {
            vec![Line::from(Span::styled(
                "(No experience data)",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            self.lines.clone()
        };

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(self.align);

        paragraph.render(area, buf);
    }
}
