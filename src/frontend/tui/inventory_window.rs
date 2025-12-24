//! Specialized window widget that mirrors the GemStone inventory panel.
//!
//! Unlike scrolling text buffers, the inventory view replaces its content on
//! each update and keeps a small recent-link cache for click detection.
//!
//! Now implemented as a thin wrapper around ListWidget for DRY.

use crate::data::widget::TextSegment;
use ratatui::{buffer::Buffer, layout::Rect};

/// Inventory window widget - displays worn/carried items
/// Content is completely replaced on each update (no appending/scrollback)
///
/// Now uses ListWidget internally for shared implementation.
pub struct InventoryWindow {
    widget: super::list_widget::ListWidget,
}

impl InventoryWindow {
    pub fn new(title: String) -> Self {
        let mut widget = super::list_widget::ListWidget::new(&title);
        // Inventory defaults to word wrap disabled (less clutter for small windows)
        widget.set_word_wrap(false);
        Self { widget }
    }

    /// Set highlight patterns for this window (only recompiles if changed)
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.widget.set_highlights(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.widget.set_replace_enabled(enabled);
    }

    /// Clear all content (called when inv stream is pushed)
    pub fn clear(&mut self) {
        self.widget.clear();
    }

    /// Add styled text segment to current line
    pub fn add_segment(&mut self, segment: TextSegment) {
        // Delegate to ListWidget's add_text method
        self.widget.add_text(
            segment.text,
            segment.fg,
            segment.bg,
            segment.bold,
            segment.span_type,
            segment.link_data,
        );
    }

    /// Finish current line and add to buffer (with wrapping)
    pub fn finish_line(&mut self) {
        // Delegate to ListWidget (handles highlights and wrapping internally)
        self.widget.finish_line();
    }

    /// Update inner dimensions based on window size
    /// Note: ListWidget updates dimensions automatically during render
    pub fn update_inner_size(&mut self, _width: u16, _height: u16) {
        // No-op: ListWidget handles this internally during render
    }

    /// Scroll up by N lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.widget.scroll_up(lines);
    }

    /// Scroll down by N lines
    pub fn scroll_down(&mut self, lines: usize) {
        self.widget.scroll_down(lines);
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        self.widget.scroll_to_bottom();
    }

    /// Get all lines (for text selection)
    pub fn get_lines(&self) -> &[Vec<TextSegment>] {
        self.widget.get_lines()
    }

    /// Get wrapped lines for mouse click detection
    pub fn get_wrapped_lines(&self) -> &[Vec<TextSegment>] {
        // For InventoryWindow compatibility, return reference to internal lines
        // (word wrapping is handled internally by ListWidget)
        self.widget.get_lines()
    }

    /// Get the start line offset (which line is shown at the top of the visible area)
    /// This is needed for click detection to map visual rows to actual line indices
    pub fn get_start_line(&self) -> usize {
        // Delegate to ListWidget's scroll logic
        self.widget.get_start_line()
    }

    /// Set title
    pub fn set_title(&mut self, title: String) {
        self.widget.set_title(title);
    }

    /// Set border configuration
    pub fn set_border_config(&mut self, show_border: bool, border_color: Option<String>) {
        self.widget.set_border_config(show_border, None, border_color);
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

    /// Render the inventory window
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.widget.render(area, buf);
    }

pub fn render_themed(&mut self, area: Rect, buf: &mut Buffer, _theme: &crate::theme::AppTheme) {
        // For now, just call regular render - theme colors will be applied in future update
        self.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::SpanType;
    use ratatui::buffer::Buffer;

    fn make_segment(text: &str) -> TextSegment {
        TextSegment {
            text: text.to_string(),
            fg: None,
            bg: None,
            bold: false,
            span_type: SpanType::Normal,
            link_data: None,
        }
    }

    #[test]
    fn test_add_segment_and_finish_line() {
        let mut inv = InventoryWindow::new("Inventory".to_string());
        inv.add_segment(make_segment("a "));
        inv.add_segment(make_segment("sword"));
        inv.finish_line();

        let lines = inv.get_lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), 2);
        assert_eq!(lines[0][0].text, "a ");
        assert_eq!(lines[0][1].text, "sword");
    }

    #[test]
    fn test_clear_removes_lines() {
        let mut inv = InventoryWindow::new("Inventory".to_string());
        inv.add_segment(make_segment("a sword"));
        inv.finish_line();
        assert!(!inv.get_lines().is_empty());

        inv.clear();
        assert!(inv.get_lines().is_empty());
    }

    #[test]
    fn test_get_start_line_after_scroll() {
        let mut inv = InventoryWindow::new("Inventory".to_string());
        inv.set_border_config(false, None);

        for _ in 0..5 {
            inv.add_segment(make_segment("item"));
            inv.finish_line();
        }

        let area = Rect::new(0, 0, 10, 3);
        let mut buf = Buffer::empty(area);
        inv.render(area, &mut buf);

        inv.scroll_up(1);
        assert_eq!(inv.get_start_line(), 1);
    }
}

