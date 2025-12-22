//! Perception window widget - displays sorted spell/buff/debuff entries
//!
//! Parses percWindow stream data and displays entries sorted by weight.

use crate::data::widget::{PerceptionEntry, SpanType, TextSegment};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget as RatatuiWidget},
};

/// Perception window widget for displaying sorted perception entries
pub struct PerceptionWindow {
    title: String,
    show_border: bool,
    border_color: Option<Color>,
    text_color: Option<Color>,
    background_color: Option<Color>,
    entries: Vec<PerceptionEntry>,
    scroll_offset: usize,
    /// Highlight engine for pattern matching and styling
    highlight_engine: super::highlight_utils::HighlightEngine,
}

impl PerceptionWindow {
    /// Create a new perception window with the given title
    pub fn new(title: String) -> Self {
        Self {
            title,
            show_border: true,
            border_color: None,
            text_color: None,
            background_color: None,
            entries: Vec::new(),
            scroll_offset: 0,
            highlight_engine: super::highlight_utils::HighlightEngine::new(Vec::new()),
        }
    }

    /// Set highlight patterns for this window
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.highlight_engine = super::highlight_utils::HighlightEngine::new(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.highlight_engine.set_replace_enabled(enabled);
    }

    /// Update the perception entries (already sorted by weight)
    pub fn set_entries(&mut self, entries: Vec<PerceptionEntry>) {
        self.entries = entries;
    }

    /// Set whether to show the window border
    pub fn set_show_border(&mut self, show: bool) {
        self.show_border = show;
    }

    /// Set the border color
    pub fn set_border_color(&mut self, color: Option<String>) {
        self.border_color = color.and_then(|c| super::colors::parse_color_to_ratatui(&c));
    }

    /// Set the text color
    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color.and_then(|c| super::colors::parse_color_to_ratatui(&c));
    }

    /// Set the background color
    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color.and_then(|c| super::colors::parse_color_to_ratatui(&c));
    }

    /// Get the current scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Set the scroll offset
    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self) {
        if self.scroll_offset < self.entries.len().saturating_sub(1) {
            self.scroll_offset += 1;
        }
    }

    /// Render the perception window to the given area
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear area
        Clear.render(area, buf);

        // Create block with optional border
        let mut block = Block::default();
        if self.show_border {
            block = block.borders(Borders::ALL).title(self.title.as_str());
            if let Some(color) = self.border_color {
                block = block.border_style(Style::default().fg(color));
            }
        }

        let inner = block.inner(area);
        block.render(area, buf);

        // Apply background color
        if let Some(bg_color) = self.background_color {
            for y in inner.top()..inner.bottom() {
                for x in inner.left()..inner.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(bg_color);
                    }
                }
            }
        }

        // Format entries as lines with highlight support
        let lines: Vec<Line> = self
            .entries
            .iter()
            .map(|entry| {
                // Create a TextSegment from the raw text
                let base_segment = TextSegment {
                    text: entry.raw_text.clone(),
                    fg: None,
                    bg: None,
                    bold: false,
                    span_type: SpanType::Normal,
                    link_data: entry.link_data.clone(),
                };

                // Apply highlights
                let segments = self
                    .highlight_engine
                    .apply_highlights_to_segments(&[base_segment.clone()], "perception")
                    .unwrap_or_else(|| vec![base_segment]);

                // Convert segments to spans
                let spans: Vec<Span> = segments
                    .iter()
                    .map(|segment| Span::styled(segment.text.clone(), self.apply_style(segment)))
                    .collect();

                Line::from(spans)
            })
            .collect();

        // Render paragraph with scrolling
        let paragraph = Paragraph::new(lines).scroll((self.scroll_offset as u16, 0));

        paragraph.render(inner, buf);
    }

    /// Apply styling to a text segment, respecting highlights and defaults
    fn apply_style(&self, segment: &TextSegment) -> Style {
        let mut style = Style::default();

        // Foreground color: segment override > window default
        if let Some(ref fg) = segment.fg {
            if let Some(color) = super::colors::parse_color_to_ratatui(fg) {
                style = style.fg(color);
            }
        } else if let Some(default_fg) = self.text_color {
            style = style.fg(default_fg);
        }

        // Background color: segment override > window default
        if let Some(ref bg) = segment.bg {
            if let Some(color) = super::colors::parse_color_to_ratatui(bg) {
                style = style.bg(color);
            }
        } else if let Some(bg_color) = self.background_color {
            style = style.bg(bg_color);
        }

        // Bold modifier
        if segment.bold {
            style = style.add_modifier(Modifier::BOLD);
        }

        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::{PerceptionEntry, PerceptionFormat};

    #[test]
    fn test_new_perception_window() {
        let window = PerceptionWindow::new("Perceptions".to_string());
        assert_eq!(window.title, "Perceptions");
        assert!(window.show_border);
        assert!(window.entries.is_empty());
        assert_eq!(window.scroll_offset, 0);
    }

    #[test]
    fn test_set_entries() {
        let mut window = PerceptionWindow::new("Test".to_string());
        let entries = vec![
            PerceptionEntry {
                name: "Bless".to_string(),
                format: PerceptionFormat::Percentage(94),
                raw_text: "Bless (94%)".to_string(),
                weight: 3094,
                link_data: None,
            },
            PerceptionEntry {
                name: "Elemental Focus".to_string(),
                format: PerceptionFormat::OngoingMagic,
                raw_text: "Elemental Focus (OM)".to_string(),
                weight: 2000,
                link_data: None,
            },
        ];

        window.set_entries(entries.clone());
        assert_eq!(window.entries.len(), 2);
        assert_eq!(window.entries[0].name, "Bless");
        assert_eq!(window.entries[1].name, "Elemental Focus");
    }

    #[test]
    fn test_scroll_functions() {
        let mut window = PerceptionWindow::new("Test".to_string());
        let entries = vec![
            PerceptionEntry {
                name: "Entry1".to_string(),
                format: PerceptionFormat::Other(String::new()),
                raw_text: "Entry1".to_string(),
                weight: 100,
                link_data: None,
            },
            PerceptionEntry {
                name: "Entry2".to_string(),
                format: PerceptionFormat::Other(String::new()),
                raw_text: "Entry2".to_string(),
                weight: 90,
                link_data: None,
            },
            PerceptionEntry {
                name: "Entry3".to_string(),
                format: PerceptionFormat::Other(String::new()),
                raw_text: "Entry3".to_string(),
                weight: 80,
                link_data: None,
            },
        ];

        window.set_entries(entries);

        // Test scrolling
        assert_eq!(window.scroll_offset(), 0);

        window.scroll_down();
        assert_eq!(window.scroll_offset(), 1);

        window.scroll_down();
        assert_eq!(window.scroll_offset(), 2);

        window.scroll_up();
        assert_eq!(window.scroll_offset(), 1);

        window.scroll_up();
        assert_eq!(window.scroll_offset(), 0);

        // Test saturating at 0
        window.scroll_up();
        assert_eq!(window.scroll_offset(), 0);
    }

    #[test]
    fn test_set_colors() {
        let mut window = PerceptionWindow::new("Test".to_string());

        window.set_text_color(Some("#00FF00".to_string()));
        assert!(window.text_color.is_some());

        window.set_border_color(Some("#FF0000".to_string()));
        assert!(window.border_color.is_some());

        window.set_background_color(Some("#000000".to_string()));
        assert!(window.background_color.is_some());
    }

    #[test]
    fn test_set_show_border() {
        let mut window = PerceptionWindow::new("Test".to_string());
        assert!(window.show_border);

        window.set_show_border(false);
        assert!(!window.show_border);

        window.set_show_border(true);
        assert!(window.show_border);
    }
}
