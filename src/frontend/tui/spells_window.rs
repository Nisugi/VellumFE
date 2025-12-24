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
        self.widget.add_text(text, fg, bg, bold, span_type, link_data);
    }

    /// Finish current line and add to buffer (no wrapping - spells content is pre-formatted)
    pub fn finish_line(&mut self) {
        self.widget.finish_line();
    }

    /// Find a link in the recent cache that matches the given word
    /// Returns the LinkData if found, otherwise None
    pub fn find_link_by_word(&self, word: &str) -> Option<LinkData> {
        self.widget.find_link_by_word(word)
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

    /// Get all lines (for text selection)
    pub fn get_lines(&self) -> &[Vec<crate::data::TextSegment>] {
        self.widget.get_lines()
    }

    pub fn set_border_config(
        &mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) {
        self.widget.set_border_config(show_border, border_style, border_color);
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

    pub fn render_themed(&mut self, area: Rect, buf: &mut Buffer, _theme: &crate::theme::AppTheme) {
        // For now, just call regular render - theme colors will be applied in future update
        self.render(area, buf);
    }
}

