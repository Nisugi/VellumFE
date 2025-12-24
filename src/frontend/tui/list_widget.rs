//! Unified list widget for all text-list use cases.
//!
//! Replaces duplicate implementations in SpellsWindow, InventoryWindow, and
//! misuse of ScrollableContainer+ProgressBar in targets, players.
//!
//! This widget properly handles text-only lists without progress bar logic.

use crate::data::{LinkData, TextSegment};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget as RatatuiWidget},
};
use std::collections::VecDeque;

/// Unified list widget for text-only lists (no progress bars).
pub struct ListWidget {
    /// Widget title (optional)
    title: String,

    /// Lines of styled text segments
    lines: Vec<Vec<TextSegment>>,

    /// Current line being built (before finish_line is called)
    current_line: Vec<TextSegment>,

    /// Scroll offset from bottom (0 = live view, showing newest)
    scroll_offset: usize,

    /// Border configuration
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<Color>,
    border_sides: crate::config::BorderSides,

    /// Color configuration
    background_color: Option<Color>,
    transparent_background: bool,
    default_text_color: Option<Color>,

    /// Highlight patterns
    highlight_engine: super::highlight_utils::HighlightEngine,
    replace_enabled: bool,

    /// Click tracking (maintains last N links for coordinate matching)
    recent_links: VecDeque<LinkData>,
    max_recent_links: usize,

    /// Optional word wrapping (for inventory-style widgets)
    word_wrap: bool,
    wrap_width: usize,

    /// Inner dimensions (updated during render)
    inner_width: usize,
    inner_height: usize,
}

impl ListWidget {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            lines: Vec::new(),
            current_line: Vec::new(),
            scroll_offset: 0,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(),
            background_color: None,
            transparent_background: false,
            default_text_color: None,
            highlight_engine: super::highlight_utils::HighlightEngine::new(Vec::new()),
            replace_enabled: false,
            recent_links: VecDeque::new(),
            max_recent_links: 100,
            word_wrap: false,
            wrap_width: 80,
            inner_width: 80,
            inner_height: 20,
        }
    }

    /// Add a line of styled text segments (for SpellsWindow/InventoryWindow style usage)
    pub fn add_line(&mut self, segments: Vec<TextSegment>) {
        if segments.is_empty() {
            // Add empty line as-is (preserves spacing)
            self.lines.push(Vec::new());
        } else {
            // Cache any links in the segments
            for segment in &segments {
                if let Some(ref link_data) = segment.link_data {
                    // Check if we already have this exist_id in the most recent entry
                    let should_append = if let Some(last) = self.recent_links.back_mut() {
                        if last.exist_id == link_data.exist_id {
                            // Append to existing text
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
                        let mut new_link = link_data.clone();
                        new_link.text = segment.text.clone();
                        self.recent_links.push_back(new_link);
                        if self.recent_links.len() > self.max_recent_links {
                            self.recent_links.pop_front();
                        }
                    }
                }
            }

            // Apply highlights before wrapping/adding to buffer
            let highlighted = self
                .highlight_engine
                .apply_highlights_to_segments(&segments, "")
                .unwrap_or(segments);

            // Apply word wrap if enabled
            if self.word_wrap && self.wrap_width > 0 {
                let wrapped_lines = self.wrap_line(highlighted);
                for line in wrapped_lines {
                    self.lines.push(line);
                }
            } else {
                self.lines.push(highlighted);
            }
        }
    }

    /// Add a simple single-color line (convenience for targets/players/dropdown)
    /// This is the pattern used by ScrollableContainer-based widgets
    pub fn add_simple_line(&mut self, text: String, color: Option<String>, link: Option<LinkData>) {
        if text.is_empty() {
            return;
        }

        // Cache link if present
        if let Some(ref link_data) = link {
            // Check for deduplication
            let should_append = if let Some(last) = self.recent_links.back_mut() {
                if last.exist_id == link_data.exist_id {
                    last.text.push_str(&text);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if !should_append {
                let mut new_link = link_data.clone();
                new_link.text = text.clone();
                self.recent_links.push_back(new_link);
                if self.recent_links.len() > self.max_recent_links {
                    self.recent_links.pop_front();
                }
            }
        }

        let segment = TextSegment {
            text,
            fg: color,
            bg: None,
            bold: false,
            span_type: crate::data::SpanType::Normal,
            link_data: link,
        };

        // Apply highlights
        let segments = vec![segment];
        let highlighted = self
            .highlight_engine
            .apply_highlights_to_segments(&segments, "")
            .unwrap_or(segments);

        self.lines.push(highlighted);
    }

    /// Add styled text to current line (for SpellsWindow compatibility)
    pub fn add_text(
        &mut self,
        text: String,
        fg: Option<String>,
        bg: Option<String>,
        bold: bool,
        span_type: crate::data::SpanType,
        link_data: Option<LinkData>,
    ) {
        if text.is_empty() {
            return;
        }

        // Cache link data if present
        if let Some(ref link_data_ref) = link_data {
            let should_append = if let Some(last) = self.recent_links.back_mut() {
                if last.exist_id == link_data_ref.exist_id {
                    last.text.push_str(&text);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if !should_append {
                let mut new_link = link_data_ref.clone();
                new_link.text = text.clone();
                self.recent_links.push_back(new_link);
                if self.recent_links.len() > self.max_recent_links {
                    self.recent_links.pop_front();
                }
            }
        }

        self.current_line.push(TextSegment {
            text,
            fg,
            bg,
            bold,
            span_type,
            link_data,
        });
    }

    /// Finish current line and add to buffer (for SpellsWindow compatibility)
    pub fn finish_line(&mut self) {
        let line = std::mem::take(&mut self.current_line);
        self.add_line(line);
    }

    /// Clear all lines
    pub fn clear(&mut self) {
        self.lines.clear();
        self.current_line.clear();
        self.scroll_offset = 0;
        // Keep link cache - links rarely change
    }

    /// Set title (with optional count)
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    /// Get current title
    pub fn get_title(&self) -> &str {
        &self.title
    }

    /// Get all lines (for text selection)
    pub fn get_lines(&self) -> &[Vec<TextSegment>] {
        &self.lines
    }

    /// Scroll up by N lines
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
        let max_scroll = self.lines.len().saturating_sub(self.inner_height);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    /// Scroll down by N lines
    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// Scroll to bottom (most recent content visible)
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
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

    /// Set border configuration
    pub fn set_border_config(
        &mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color.and_then(|c| Self::parse_color(&c));
    }

    /// Set which border sides to show
    pub fn set_border_sides(&mut self, sides: crate::config::BorderSides) {
        self.border_sides = sides;
    }

    /// Set default text color
    pub fn set_text_color(&mut self, color: Option<String>) {
        self.default_text_color = color.and_then(|c| Self::parse_color(&c));
    }

    /// Set background color
    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color.and_then(|c| {
            let trimmed = c.trim().to_string();
            if trimmed.is_empty() || trimmed == "-" {
                None
            } else {
                Self::parse_color(&trimmed)
            }
        });
    }

    /// Set whether background is transparent
    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    /// Set highlight patterns for this widget
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.highlight_engine.update_if_changed(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.replace_enabled = enabled;
        self.highlight_engine.set_replace_enabled(enabled);
    }

    /// Set word wrap mode (for inventory-style widgets)
    pub fn set_word_wrap(&mut self, enabled: bool) {
        self.word_wrap = enabled;
    }

    /// Update inner dimensions based on window size
    fn update_inner_size(&mut self, width: u16, height: u16) {
        self.inner_width = if self.show_border {
            (width.saturating_sub(2)) as usize
        } else {
            width as usize
        };
        self.inner_height = if self.show_border {
            (height.saturating_sub(2)) as usize
        } else {
            height as usize
        };
        self.wrap_width = self.inner_width;
    }

    /// Wrap a line of styled segments to window width (word-boundary aware)
    /// Copied from InventoryWindow for DRY
    fn wrap_line(&self, segments: Vec<TextSegment>) -> Vec<Vec<TextSegment>> {
        if self.wrap_width == 0 {
            return vec![Vec::new()];
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
                        if current_width + word_buffer_len <= self.wrap_width {
                            // Word fits - add it to current line
                            for word_seg in word_buffer.drain(..) {
                                Self::append_to_line(&mut current_line, word_seg);
                            }
                            current_width += word_buffer_len;
                        } else if word_buffer_len <= self.wrap_width {
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
                                    if current_width >= self.wrap_width {
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
                    if current_width >= self.wrap_width {
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
            if current_width + word_buffer_len <= self.wrap_width {
                // Word fits on current line
                for word_seg in word_buffer {
                    Self::append_to_line(&mut current_line, word_seg);
                }
            } else if word_buffer_len <= self.wrap_width {
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
                        if current_width >= self.wrap_width {
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
            if last_seg.fg == segment.fg
                && last_seg.bg == segment.bg
                && last_seg.bold == segment.bold
                && last_seg.span_type == segment.span_type
                && last_seg.link_data == segment.link_data
            {
                // Merge text into existing segment
                last_seg.text.push_str(&segment.text);
                return;
            }
        }
        // Can't merge - append as new segment
        line.push(segment);
    }

    /// Helper to append a segment to a buffer, merging with last segment if style matches
    fn append_to_buffer(buffer: &mut Vec<TextSegment>, segment: TextSegment) {
        if let Some(last_seg) = buffer.last_mut() {
            if last_seg.fg == segment.fg
                && last_seg.bg == segment.bg
                && last_seg.bold == segment.bold
                && last_seg.span_type == segment.span_type
                && last_seg.link_data == segment.link_data
            {
                last_seg.text.push_str(&segment.text);
                return;
            }
        }
        buffer.push(segment);
    }

    /// Parse a hex color string to ratatui Color
    fn parse_color(hex: &str) -> Option<Color> {
        super::colors::parse_color_to_ratatui(hex)
    }

    /// Render the widget
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.render_with_focus(area, buf, false);
    }

    /// Render the widget with optional focus indicator
    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, _focused: bool) {
        // Update inner size based on area
        self.update_inner_size(area.width, area.height);

        // Clear the area to prevent bleed-through from windows behind
        Clear.render(area, buf);

        // Fill background if not transparent
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

        // Create border block
        let mut block = Block::default();

        if self.show_border {
            let border_color = self.border_color.unwrap_or(Color::White);

            // Apply border sides configuration
            let mut borders = Borders::empty();
            if self.border_sides.top {
                borders |= Borders::TOP;
            }
            if self.border_sides.bottom {
                borders |= Borders::BOTTOM;
            }
            if self.border_sides.left {
                borders |= Borders::LEFT;
            }
            if self.border_sides.right {
                borders |= Borders::RIGHT;
            }

            block = block
                .borders(borders)
                .border_style(Style::default().fg(border_color));

            // Apply border type
            if let Some(ref style) = self.border_style {
                let border_type = match style.as_str() {
                    "double" => BorderType::Double,
                    "rounded" => BorderType::Rounded,
                    "thick" => BorderType::Thick,
                    _ => BorderType::Plain,
                };
                block = block.border_type(border_type);
            }

            if !self.title.is_empty() {
                block = block.title(self.title.as_str());
            }
        }

        let inner = block.inner(area);

        // Calculate visible range (scroll from bottom, like SpellsWindow)
        let total_lines = self.lines.len();
        let visible_start = total_lines.saturating_sub(self.scroll_offset + inner.height as usize);
        let visible_end = total_lines.saturating_sub(self.scroll_offset);

        // Build visible lines
        let mut display_lines = Vec::new();
        for line in self.lines[visible_start..visible_end].iter() {
            let spans: Vec<Span> = line
                .iter()
                .map(|segment| Span::styled(segment.text.clone(), self.apply_style(segment)))
                .collect();
            display_lines.push(Line::from(spans));
        }

        let paragraph = Paragraph::new(display_lines).block(block);

        ratatui::widgets::Widget::render(paragraph, area, buf);
    }

    /// Apply styling to a text segment
    fn apply_style(&self, segment: &TextSegment) -> Style {
        let mut style = Style::default();

        if let Some(ref fg) = segment.fg {
            if let Some(color) = Self::parse_color(fg) {
                style = style.fg(color);
            }
        } else if let Some(default_fg) = self.default_text_color {
            style = style.fg(default_fg);
        }

        if let Some(ref bg) = segment.bg {
            if let Some(color) = Self::parse_color(bg) {
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

    /// Find a link in the recent cache that matches the given word
    /// Returns the LinkData if found, otherwise None
    /// Uses SpellsWindow's multi-pass matching algorithm
    pub fn find_link_by_word(&self, word: &str) -> Option<LinkData> {
        // Search from most recent to oldest
        // First pass: word appears in multi-word link text (HIGHEST priority - prefer complete phrases)
        for link in self.recent_links.iter().rev() {
            let link_text_lower = link.text.to_lowercase();
            let word_lower = word.to_lowercase();

            // Only check multi-word links (2+ words)
            if link_text_lower.split_whitespace().count() > 1 {
                // Check if word appears in the text
                if link_text_lower.split_whitespace().any(|w| w == word_lower) {
                    return Some(link.clone());
                }
            }
        }

        // Second pass: exact noun match for single-word links
        for link in self.recent_links.iter().rev() {
            if link.noun.eq_ignore_ascii_case(word) {
                return Some(link.clone());
            }
        }

        // Third pass: word appears in single-word link text
        for link in self.recent_links.iter().rev() {
            let link_text_lower = link.text.to_lowercase();
            let word_lower = word.to_lowercase();

            if link_text_lower.split_whitespace().count() == 1
                && link_text_lower.split_whitespace().any(|w| w == word_lower)
            {
                return Some(link.clone());
            }
        }

        // No match found
        None
    }

    /// Handle a click at the given coordinates (for targets style usage)
    /// Returns the LinkData if a link was clicked at that position
    /// Note: This is a simple implementation - coordinate-based click handling
    /// is complex and may need refinement based on actual usage
    pub fn handle_click(&self, _x: u16, y: u16, area: Rect) -> Option<LinkData> {
        // Calculate which line was clicked
        let inner_y = if self.show_border {
            y.saturating_sub(area.y + 1)
        } else {
            y.saturating_sub(area.y)
        };

        let total_lines = self.lines.len();
        let visible_start = total_lines.saturating_sub(self.scroll_offset + self.inner_height);
        let clicked_line_idx = visible_start + inner_y as usize;

        if clicked_line_idx >= self.lines.len() {
            return None;
        }

        // Get the clicked line
        let line = &self.lines[clicked_line_idx];

        // Find first link in this line
        for segment in line {
            if let Some(ref link) = segment.link_data {
                return Some(link.clone());
            }
        }

        None
    }
}
