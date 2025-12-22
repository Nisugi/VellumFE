//! Specialized window widget that mirrors the GemStone inventory panel.
//!
//! Unlike scrolling text buffers, the inventory view replaces its content on
//! each update and keeps a small recent-link cache for click detection.

use crate::data::widget::TextSegment;
use crate::frontend::tui::colors::parse_color_to_ratatui;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::collections::VecDeque;

/// Inventory window widget - displays worn/carried items
/// Content is completely replaced on each update (no appending/scrollback)
pub struct InventoryWindow {
    title: String,
    show_border: bool,
    border_color: Option<Color>,
    text_color: Option<Color>,
    background_color: Option<Color>,
    transparent_background: bool,

    /// Current inventory content (list of styled lines)
    /// No scrollback - completely replaced on each update
    lines: Vec<Vec<TextSegment>>,

    /// Current line being built (before finish_line is called)
    current_line: Vec<TextSegment>,

    /// Scroll offset for navigation
    scroll_offset: usize,

    /// Window dimensions (updated during layout)
    inner_width: usize,
    inner_height: usize,

    /// Enable word wrapping (default false - less clutter for small windows)
    word_wrap: bool,

    /// Highlight engine for pattern matching and styling
    highlight_engine: super::highlight_utils::HighlightEngine,

    /// Recent links cache for deduplication and click detection
    /// Consecutive segments with same exist_id get their text appended
    recent_links: VecDeque<crate::data::LinkData>,
    max_recent_links: usize,
}

impl InventoryWindow {
    pub fn new(title: String) -> Self {
        Self {
            title,
            show_border: true,
            border_color: None,
            text_color: None,
            background_color: None,
            transparent_background: false,
            lines: Vec::new(),
            current_line: Vec::new(),
            scroll_offset: 0,
            inner_width: 80,
            inner_height: 20,
            word_wrap: false, // Default off - less clutter for small windows
            highlight_engine: super::highlight_utils::HighlightEngine::new(Vec::new()),
            recent_links: VecDeque::new(),
            max_recent_links: 100,
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

    /// Clear all content (called when inv stream is pushed)
    pub fn clear(&mut self) {
        self.lines.clear();
        self.current_line.clear();
        self.scroll_offset = 0;
        // Don't clear recent_links - keep for deduplication across updates
    }

    /// Add styled text segment to current line
    pub fn add_segment(&mut self, segment: TextSegment) {
        if segment.text.is_empty() {
            return;
        }

        // Cache link data if present, de-duplicating consecutive segments with same exist_id
        if let Some(ref link_data_ref) = segment.link_data {
            // Check if we already have this exist_id in the most recent entry
            let should_append = if let Some(last) = self.recent_links.back_mut() {
                if last.exist_id == link_data_ref.exist_id {
                    // Append to existing text (same item continuing across segments)
                    last.text.push_str(&segment.text);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if !should_append {
                // New link - create new entry
                let mut new_link = link_data_ref.clone();
                new_link.text = segment.text.clone();
                self.recent_links.push_back(new_link);
                if self.recent_links.len() > self.max_recent_links {
                    self.recent_links.pop_front();
                }
            }
        }

        self.current_line.push(segment);
    }

    /// Finish current line and add to buffer (with wrapping)
    pub fn finish_line(&mut self) {
        let line = std::mem::take(&mut self.current_line);
        if line.is_empty() {
            // Skip empty lines
            return;
        }

        // Apply highlights before wrapping
        let highlighted = self
            .highlight_engine
            .apply_highlights_to_segments(&line, "inv")
            .unwrap_or(line);

        // Wrap the highlighted line to window width
        let wrapped = self.wrap_line(highlighted);
        self.lines.extend(wrapped);
    }

    /// Wrap a line of styled segments to window width (word-boundary aware)
    /// Matches VellumFE's character-by-character processing
    fn wrap_line(&self, segments: Vec<TextSegment>) -> Vec<Vec<TextSegment>> {
        if self.inner_width == 0 {
            return vec![Vec::new()];
        }

        // If word wrap is disabled, return the line as-is (no wrapping)
        if !self.word_wrap {
            return vec![segments];
        }

        let mut wrapped_lines = Vec::new();
        let mut current_line = Vec::new();
        let mut current_width = 0;

        // Track word buffer for smart wrapping
        let mut word_buffer: Vec<TextSegment> = Vec::new();
        let mut word_buffer_len = 0;
        let mut in_word = false;

        for segment in segments {
            for ch in segment.text.chars() {
                let is_whitespace = ch.is_whitespace();

                if is_whitespace {
                    // Flush word buffer if we have one
                    if in_word && !word_buffer.is_empty() {
                        // Check if word fits on current line
                        if current_width + word_buffer_len <= self.inner_width {
                            // Word fits - add it to current line
                            for word_seg in word_buffer.drain(..) {
                                Self::append_to_line(&mut current_line, word_seg);
                            }
                            current_width += word_buffer_len;
                        } else if word_buffer_len <= self.inner_width {
                            // Word doesn't fit on current line, but fits on new line - wrap
                            if !current_line.is_empty() {
                                wrapped_lines.push(std::mem::take(&mut current_line));
                                current_width = 0;
                            }
                            // Add word to new line
                            for word_seg in word_buffer.drain(..) {
                                Self::append_to_line(&mut current_line, word_seg);
                            }
                            current_width += word_buffer_len;
                        } else {
                            // Word is longer than width - must break it mid-word
                            for word_seg in word_buffer.drain(..) {
                                for word_ch in word_seg.text.chars() {
                                    if current_width >= self.inner_width {
                                        wrapped_lines.push(std::mem::take(&mut current_line));
                                        current_width = 0;
                                    }
                                    Self::append_to_line(
                                        &mut current_line,
                                        TextSegment {
                                            text: word_ch.to_string(),
                                            fg: word_seg.fg.clone(),
                                            bg: word_seg.bg.clone(),
                                            bold: word_seg.bold,
                                            span_type: word_seg.span_type,
                                            link_data: word_seg.link_data.clone(),
                                        },
                                    );
                                    current_width += 1;
                                }
                            }
                        }
                        word_buffer_len = 0;
                        in_word = false;
                    }

                    // Add whitespace immediately (don't buffer it)
                    if current_width >= self.inner_width {
                        // Wrap before whitespace
                        wrapped_lines.push(std::mem::take(&mut current_line));
                        current_width = 0;
                        // Don't add whitespace at start of new line
                        continue;
                    }
                    Self::append_to_line(
                        &mut current_line,
                        TextSegment {
                            text: ch.to_string(),
                            fg: segment.fg.clone(),
                            bg: segment.bg.clone(),
                            bold: segment.bold,
                            span_type: segment.span_type,
                            link_data: segment.link_data.clone(),
                        },
                    );
                    current_width += 1;
                } else {
                    // Non-whitespace character - add to word buffer
                    in_word = true;
                    Self::append_to_buffer(
                        &mut word_buffer,
                        TextSegment {
                            text: ch.to_string(),
                            fg: segment.fg.clone(),
                            bg: segment.bg.clone(),
                            bold: segment.bold,
                            span_type: segment.span_type,
                            link_data: segment.link_data.clone(),
                        },
                    );
                    word_buffer_len += 1;
                }
            }
        }

        // Flush remaining word buffer
        if !word_buffer.is_empty() {
            if current_width + word_buffer_len <= self.inner_width {
                // Word fits on current line
                for word_seg in word_buffer {
                    Self::append_to_line(&mut current_line, word_seg);
                }
            } else if word_buffer_len <= self.inner_width {
                // Word needs new line
                if !current_line.is_empty() {
                    wrapped_lines.push(std::mem::take(&mut current_line));
                }
                for word_seg in word_buffer {
                    Self::append_to_line(&mut current_line, word_seg);
                }
            } else {
                // Word is too long - must break it
                for word_seg in word_buffer {
                    for word_ch in word_seg.text.chars() {
                        if current_width >= self.inner_width {
                            wrapped_lines.push(std::mem::take(&mut current_line));
                            current_width = 0;
                        }
                        Self::append_to_line(
                            &mut current_line,
                            TextSegment {
                                text: word_ch.to_string(),
                                fg: word_seg.fg.clone(),
                                bg: word_seg.bg.clone(),
                                bold: word_seg.bold,
                                span_type: word_seg.span_type,
                                link_data: word_seg.link_data.clone(),
                            },
                        );
                        current_width += 1;
                    }
                }
            }
        }

        // Add remaining line if not empty
        if !current_line.is_empty() {
            wrapped_lines.push(current_line);
        }

        // Return at least one empty line if nothing was added
        if wrapped_lines.is_empty() {
            wrapped_lines.push(Vec::new());
        }

        wrapped_lines
    }

    /// Helper to append a segment to a line, merging with last segment if style matches
    fn append_to_line(line: &mut Vec<TextSegment>, segment: TextSegment) {
        if let Some(last_seg) = line.last_mut() {
            // Check if all properties match (including link_data for proper link boundaries)
            let styles_match = last_seg.fg == segment.fg
                && last_seg.bg == segment.bg
                && last_seg.bold == segment.bold
                && last_seg.span_type == segment.span_type
                && last_seg.link_data == segment.link_data;

            if styles_match {
                last_seg.text.push_str(&segment.text);
            } else {
                line.push(segment);
            }
        } else {
            line.push(segment);
        }
    }

    /// Helper to append a segment to buffer, merging with last segment if style matches
    fn append_to_buffer(buffer: &mut Vec<TextSegment>, segment: TextSegment) {
        if let Some(last_seg) = buffer.last_mut() {
            // Check if all properties match (including link_data for proper link boundaries)
            let styles_match = last_seg.fg == segment.fg
                && last_seg.bg == segment.bg
                && last_seg.bold == segment.bold
                && last_seg.span_type == segment.span_type
                && last_seg.link_data == segment.link_data;

            if styles_match {
                last_seg.text.push_str(&segment.text);
            } else {
                buffer.push(segment);
            }
        } else {
            buffer.push(segment);
        }
    }

    /// Update inner dimensions based on window size
    pub fn update_inner_size(&mut self, width: u16, height: u16) {
        let new_width = if self.show_border {
            (width.saturating_sub(2)) as usize
        } else {
            width as usize
        };
        let new_height = if self.show_border {
            (height.saturating_sub(2)) as usize
        } else {
            height as usize
        };

        // If width changed, we need to rewrap all lines
        if new_width != self.inner_width {
            // TODO: Could implement rewrapping here, but for now just update dimensions
            self.inner_width = new_width;
        }
        self.inner_height = new_height;
    }

    /// Scroll up by N lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        let max_scroll = self.lines.len().saturating_sub(self.inner_height);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    /// Scroll down by N lines
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Get all lines (for text selection)
    pub fn get_lines(&self) -> &[Vec<TextSegment>] {
        &self.lines
    }

    /// Get wrapped lines for mouse click detection
    pub fn get_wrapped_lines(&self) -> &Vec<Vec<TextSegment>> {
        &self.lines
    }

    /// Get the start line offset (which line is shown at the top of the visible area)
    /// This is needed for click detection to map visual rows to actual line indices
    pub fn get_start_line(&self) -> usize {
        let total_lines = self.lines.len();
        if total_lines > self.inner_height {
            total_lines
                .saturating_sub(self.inner_height)
                .saturating_sub(self.scroll_offset)
        } else {
            0
        }
    }

    /// Set title
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    /// Set border configuration
    pub fn set_border_config(&mut self, show_border: bool, border_color: Option<String>) {
        self.show_border = show_border;
        self.border_color = border_color.and_then(|hex| parse_hex_color(&hex));
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color.and_then(|hex| parse_hex_color(&hex));
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color.and_then(|hex| {
            let trimmed = hex.trim().to_string();
            if trimmed.is_empty() || trimmed == "-" {
                None
            } else {
                parse_hex_color(&trimmed)
            }
        });
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    /// Render the inventory window
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        if !self.transparent_background {
            if let Some(bg_color) = self.background_color {
                for row in 0..area.height {
                    for col in 0..area.width {
                        let x = area.x + col;
                        let y = area.y + row;
                        if x < buf.area().width && y < buf.area().height {
                            buf[(x, y)].set_bg(bg_color);
                        }
                    }
                }
            }
        }

        // Update inner size
        self.update_inner_size(area.width, area.height);

        // Create border block
        let mut block = Block::default();

        if self.show_border {
            let border_color = self.border_color.unwrap_or(Color::White);

            block = block
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(self.title.as_str());
        }

        // Calculate visible range
        let total_lines = self.lines.len();
        let start_line = if total_lines > self.inner_height {
            total_lines
                .saturating_sub(self.inner_height)
                .saturating_sub(self.scroll_offset)
        } else {
            0
        };
        let end_line = start_line + self.inner_height.min(total_lines);

        // Get visible lines
        let visible_lines: Vec<Line> = self.lines[start_line..end_line.min(total_lines)]
            .iter()
            .map(|segments| {
                let spans: Vec<Span> = segments
                    .iter()
                    .map(|seg| Span::styled(seg.text.clone(), self.apply_style(seg)))
                    .collect();
                Line::from(spans)
            })
            .collect();

        let paragraph = Paragraph::new(visible_lines).block(block);
        use ratatui::widgets::Widget;
        paragraph.render(area, buf);
    }

    fn apply_style(&self, segment: &TextSegment) -> Style {
        let mut style = Style::default();

        if let Some(ref fg) = segment.fg {
            if let Some(color) = parse_hex_color(fg) {
                style = style.fg(color);
            }
        } else if let Some(default_fg) = self.text_color {
            style = style.fg(default_fg);
        }

        if let Some(ref bg) = segment.bg {
            if let Some(color) = parse_hex_color(bg) {
                style = style.bg(color);
            }
        } else if !self.transparent_background {
            if let Some(bg_color) = self.background_color {
                style = style.bg(bg_color);
            }
        }

        if segment.bold {
            style = style.add_modifier(ratatui::style::Modifier::BOLD);
        }

        style
    }

    pub fn render_themed(&mut self, area: Rect, buf: &mut Buffer, _theme: &crate::theme::AppTheme) {
        // For now, just call regular render - theme colors will be applied in future update
        self.render(area, buf);
    }
}

/// Parse a color string to ratatui Color (supports hex and color names)
fn parse_hex_color(input: &str) -> Option<Color> {
    parse_color_to_ratatui(input)
}

