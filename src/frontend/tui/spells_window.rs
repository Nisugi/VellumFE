//! Window dedicated to showing spell listings and clickable spell links.
//!
//! This widget behaves similarly to the inventory window but retains a separate
//! link cache tailored to `<spell>` stream updates.
//!
//! Now implemented as a thin wrapper around ListWidget for DRY.

use crate::data::{LinkData, SpanType};
use ratatui::{buffer::Buffer, layout::Rect};

/// Spells window widget - displays known spells with clickable links
/// Content is completely replaced on each update (no buffer, no scrolling history)
///
/// Now uses ListWidget internally for shared implementation.
pub struct SpellsWindow {
    widget: super::list_widget::ListWidget,
}

impl SpellsWindow {
    pub fn new(title: String) -> Self {
        Self {
            widget: super::list_widget::ListWidget::new(&title),
        }
    }

    /// Set highlight patterns for this window (only recompiles if changed)
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.widget.set_highlights(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.widget.set_replace_enabled(enabled);
    }

    /// Clear all content (called when clearStream is received)
    pub fn clear(&mut self) {
        self.widget.clear();
    }

    /// Add styled text to current line
    pub fn add_text(
        &mut self,
        text: String,
        fg: Option<String>,
        bg: Option<String>,
        bold: bool,
        span_type: SpanType,
        link_data: Option<LinkData>,
    ) {
        self.widget
            .add_text(text, fg, bg, bold, span_type, link_data);
    }

    /// Finish current line and add to buffer (no wrapping - spells content is pre-formatted)
    pub fn finish_line(&mut self) {
        self.widget.finish_line();
    }

    /// Scroll up by N lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.widget.scroll_up(lines);
    }

    /// Scroll down by N lines
    pub fn scroll_down(&mut self, lines: usize) {
        self.widget.scroll_down(lines);
    }

    pub fn set_border_config(
        &mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) {
        self.widget
            .set_border_config(show_border, border_style, border_color);
    }

    pub fn set_title(&mut self, title: String) {
        self.widget.set_title(title);
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.widget.set_text_color(color);
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.widget.set_background_color(color);
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.widget.set_transparent_background(transparent);
    }

    /// Handle a click at the given coordinates.
    /// Returns the LinkData if a spell link was clicked.
    pub fn handle_click(&self, x: u16, y: u16, area: Rect) -> Option<LinkData> {
        self.widget.handle_click(x, y, area)
    }

    /// Render the spells window
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.widget.render(area, buf);
    }

    /// Convert mouse position to text coordinates
    pub fn mouse_to_text_coords(
        &self,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: Rect,
    ) -> Option<(usize, usize)> {
        self.widget
            .mouse_to_text_coords(mouse_col, mouse_row, window_rect)
    }

    /// Extract text from a selection range
    pub fn extract_selection_text(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        self.widget
            .extract_selection_text(start_line, start_col, end_line, end_col)
    }
}

#[cfg(test)]
impl SpellsWindow {
    fn get_lines(&self) -> &[Vec<crate::data::TextSegment>] {
        self.widget.get_lines()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    fn link(exist_id: &str, noun: &str, text: &str) -> LinkData {
        LinkData {
            exist_id: exist_id.to_string(),
            noun: noun.to_string(),
            text: text.to_string(),
            coord: None,
        }
    }

    #[test]
    fn test_add_text_and_finish_line() {
        let mut spells = SpellsWindow::new("Spells".to_string());
        spells.add_text(
            "You know ".to_string(),
            None,
            None,
            false,
            SpanType::Normal,
            None,
        );
        spells.add_text(
            "Fireball".to_string(),
            None,
            None,
            false,
            SpanType::Link,
            Some(link("101", "fireball", "Fireball")),
        );
        spells.finish_line();

        let lines = spells.get_lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), 2);
        assert_eq!(lines[0][0].text, "You know ");
        assert_eq!(lines[0][1].text, "Fireball");
    }

    #[test]
    fn test_clear_removes_lines() {
        let mut spells = SpellsWindow::new("Spells".to_string());
        spells.add_text(
            "Fireball".to_string(),
            None,
            None,
            false,
            SpanType::Link,
            Some(link("101", "fireball", "Fireball")),
        );
        spells.finish_line();
        assert!(!spells.get_lines().is_empty());

        spells.clear();
        assert!(spells.get_lines().is_empty());
    }

    #[test]
    fn test_handle_click_returns_link_data() {
        let mut spells = SpellsWindow::new("Spells".to_string());
        spells.set_border_config(false, None, None);
        spells.add_text(
            "Fireball".to_string(),
            None,
            None,
            false,
            SpanType::Link,
            Some(link("101", "fireball", "Fireball")),
        );
        spells.finish_line();

        let area = Rect::new(0, 0, 20, 3);
        let mut buf = Buffer::empty(area);
        spells.render(area, &mut buf);

        let clicked = spells.handle_click(0, 0, area);
        assert!(clicked.is_some());
        let clicked = clicked.unwrap();
        assert_eq!(clicked.exist_id, "101");
        assert_eq!(clicked.text, "Fireball");
    }
}
