use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};
use std::collections::VecDeque;
use regex::Regex;

#[derive(Clone)]
pub struct StyledText {
    pub content: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

// One display line (post-wrapping) with multiple styled spans
#[derive(Clone)]
struct WrappedLine {
    spans: Vec<(String, Style)>,
}

// One logical line (before wrapping) - stores original styled content
#[derive(Clone)]
struct LogicalLine {
    spans: Vec<(String, Style)>,
}

// Match location: (line_index, start_char, end_char)
#[derive(Clone, Debug)]
struct SearchMatch {
    line_idx: usize,      // Index in wrapped_lines
    start: usize,         // Character offset in the line text
    end: usize,           // Character offset (exclusive)
}

struct SearchState {
    regex: Regex,
    matches: Vec<SearchMatch>,
    current_match_idx: usize,  // Which match is currently selected
}

pub struct TextWindow {
    // Store original logical lines (for re-wrapping)
    logical_lines: VecDeque<LogicalLine>,
    // Cached wrapped lines (invalidated when width changes)
    wrapped_lines: VecDeque<WrappedLine>,
    // Accumulate styled chunks for current logical line
    current_line_spans: Vec<(String, Style)>,
    max_lines: usize,
    scroll_offset: usize,  // Lines back from end when at bottom (0 = live view)
    scroll_position: Option<usize>,  // Absolute line position when scrolled back (None = following live)
    last_visible_height: usize,  // Track the visible height from last render
    title: String,
    last_width: u16,
    needs_rewrap: bool, // Flag to trigger re-wrapping
    // Border configuration
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: Option<Vec<String>>,
    // Search functionality
    search_state: Option<SearchState>,
}

impl TextWindow {
    pub fn new(title: impl Into<String>, max_lines: usize) -> Self {
        Self {
            logical_lines: VecDeque::with_capacity(max_lines),
            wrapped_lines: VecDeque::with_capacity(max_lines * 2), // More space for wrapped
            current_line_spans: Vec::new(),
            max_lines,
            scroll_offset: 0,
            title: title.into(),
            last_width: 0,
            needs_rewrap: false,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: None,
            scroll_position: None,  // Start in live view mode
            last_visible_height: 20,  // Reasonable default
            search_state: None,  // No active search
        }
    }

    pub fn with_border_config(
        mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) -> Self {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color;
        self
    }

    /// Update border configuration on an existing window
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

    pub fn set_border_sides(&mut self, border_sides: Option<Vec<String>>) {
        self.border_sides = border_sides;
    }

    /// Update the window title
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn add_text(&mut self, styled: StyledText) {
        let style = Style::default()
            .fg(styled.fg.unwrap_or(Color::Gray))
            .bg(styled.bg.unwrap_or(Color::Reset))
            .add_modifier(if styled.bold {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });

        // Add this styled chunk to current line
        self.current_line_spans.push((styled.content, style));
    }

    pub fn finish_line(&mut self, _width: u16) {
        if self.current_line_spans.is_empty() {
            return;
        }

        // Store the original logical line
        let logical_line = LogicalLine {
            spans: self.current_line_spans.clone(),
        };
        self.logical_lines.push_back(logical_line);

        // Remove oldest logical line if we exceed buffer
        if self.logical_lines.len() > self.max_lines {
            self.logical_lines.pop_front();
        }

        // Wrap this logical line and add to wrapped cache
        let actual_width = if self.last_width > 0 {
            self.last_width
        } else {
            80 // Fallback
        };

        let wrapped = self.wrap_styled_spans(&self.current_line_spans, actual_width as usize);

        // Add wrapped lines to the END
        for line in wrapped {
            self.wrapped_lines.push_back(line);
        }

        self.current_line_spans.clear();
    }

    // Wrap a series of styled spans into multiple display lines
    fn wrap_styled_spans(&self, spans: &[(String, Style)], width: usize) -> Vec<WrappedLine> {
        if width == 0 {
            return vec![];
        }

        let mut result = Vec::new();
        let mut current_line_spans: Vec<(String, Style)> = Vec::new();
        let mut current_line_len = 0;

        for (text, style) in spans {
            // Process text character by character to preserve exact spacing
            let mut chars = text.chars().peekable();
            let mut word_buffer = String::new();

            while let Some(ch) = chars.next() {
                if ch.is_whitespace() {
                    // Flush word buffer if we have one
                    if !word_buffer.is_empty() {
                        let word_len = word_buffer.len();

                        // Check if we need to wrap before adding this word
                        if current_line_len > 0 && current_line_len + 1 + word_len > width {
                            // Wrap to new line
                            result.push(WrappedLine {
                                spans: current_line_spans.clone(),
                            });
                            current_line_spans.clear();
                            current_line_len = 0;
                        }

                        // Add word to current line
                        if current_line_len > 0 {
                            // Check if word starts with punctuation (don't add space before punctuation)
                            let needs_space = !word_buffer.starts_with(|c: char| c.is_ascii_punctuation());

                            // Append to last span if same style
                            if let Some((last_text, last_style)) = current_line_spans.last_mut() {
                                if last_style == style {
                                    if needs_space {
                                        last_text.push(' ');
                                    }
                                    last_text.push_str(&word_buffer);
                                } else {
                                    if needs_space {
                                        current_line_spans.push((format!(" {}", word_buffer), *style));
                                    } else {
                                        current_line_spans.push((word_buffer.clone(), *style));
                                    }
                                }
                            } else {
                                if needs_space {
                                    current_line_spans.push((format!(" {}", word_buffer), *style));
                                } else {
                                    current_line_spans.push((word_buffer.clone(), *style));
                                }
                            }
                            current_line_len += if needs_space { 1 } else { 0 } + word_len;
                        } else {
                            current_line_spans.push((word_buffer.clone(), *style));
                            current_line_len += word_len;
                        }
                        word_buffer.clear();
                    }
                    // Skip trailing whitespace (don't add to output)
                } else {
                    word_buffer.push(ch);
                }
            }

            // Flush any remaining word
            if !word_buffer.is_empty() {
                let word_len = word_buffer.len();
                let needs_space = !word_buffer.starts_with(|c: char| c.is_ascii_punctuation());
                let space_len = if needs_space { 1 } else { 0 };

                if current_line_len > 0 && current_line_len + space_len + word_len > width {
                    result.push(WrappedLine {
                        spans: current_line_spans.clone(),
                    });
                    current_line_spans.clear();
                    current_line_len = 0;
                }

                if current_line_len > 0 {
                    if let Some((last_text, last_style)) = current_line_spans.last_mut() {
                        if last_style == style {
                            if needs_space {
                                last_text.push(' ');
                            }
                            last_text.push_str(&word_buffer);
                        } else {
                            if needs_space {
                                current_line_spans.push((format!(" {}", word_buffer), *style));
                            } else {
                                current_line_spans.push((word_buffer.clone(), *style));
                            }
                        }
                    } else {
                        if needs_space {
                            current_line_spans.push((format!(" {}", word_buffer), *style));
                        } else {
                            current_line_spans.push((word_buffer.clone(), *style));
                        }
                    }
                    current_line_len += space_len + word_len;
                } else {
                    current_line_spans.push((word_buffer.clone(), *style));
                    current_line_len += word_len;
                }
            }
        }

        // Push any remaining content
        if !current_line_spans.is_empty() {
            result.push(WrappedLine {
                spans: current_line_spans,
            });
        }

        // Handle empty line case
        if result.is_empty() {
            result.push(WrappedLine {
                spans: vec![("".to_string(), Style::default())],
            });
        }

        result
    }

    pub fn scroll_up(&mut self, amount: usize) {
        // Scrolling up = viewing older lines
        let total_lines = self.wrapped_lines.len();

        if let Some(pos) = self.scroll_position {
            // Already scrolled - move the absolute position up (to older lines)
            self.scroll_position = Some(pos.saturating_sub(amount));
        } else {
            // First scroll up from live view - convert to absolute position
            // We're currently viewing the last last_visible_height lines
            // The view starts at (total_lines - visible_height)
            let current_start = total_lines.saturating_sub(self.last_visible_height);
            // Scroll up means move the start position back
            self.scroll_position = Some(current_start.saturating_sub(amount));
        }
    }

    pub fn scroll_down(&mut self, amount: usize) {
        // Scrolling down = viewing newer lines
        let total_lines = self.wrapped_lines.len();

        if let Some(pos) = self.scroll_position {
            let new_pos = pos.saturating_add(amount);

            // Check if we've scrolled back to the bottom (within visible_height of end)
            let bottom_threshold = total_lines.saturating_sub(self.last_visible_height);
            if new_pos >= bottom_threshold {
                // Return to live view mode
                self.scroll_position = None;
                self.scroll_offset = 0;
            } else {
                self.scroll_position = Some(new_pos);
            }
        } else {
            // Already in live view, just decrease offset (shouldn't normally happen)
            self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        }
    }

    pub fn set_width(&mut self, width: u16) {
        if width == self.last_width || width == 0 {
            return;
        }

        self.last_width = width;
        self.needs_rewrap = true; // Mark that we need to re-wrap all lines
    }

    /// Start a new search with the given regex pattern
    /// Returns Ok(match_count) or Err(regex_error)
    pub fn start_search(&mut self, pattern: &str) -> Result<usize, regex::Error> {
        let regex = Regex::new(pattern)?;

        // Search through all wrapped lines
        let mut matches = Vec::new();

        for (line_idx, wrapped_line) in self.wrapped_lines.iter().enumerate() {
            // Combine all spans into a single text string for searching
            let line_text: String = wrapped_line.spans.iter()
                .map(|(text, _)| text.as_str())
                .collect();

            // Find all matches in this line
            for mat in regex.find_iter(&line_text) {
                matches.push(SearchMatch {
                    line_idx,
                    start: mat.start(),
                    end: mat.end(),
                });
            }
        }

        let match_count = matches.len();

        if !matches.is_empty() {
            self.search_state = Some(SearchState {
                regex,
                matches,
                current_match_idx: 0,
            });

            // Scroll to first match
            self.scroll_to_match(0);
        } else {
            self.search_state = None;
        }

        Ok(match_count)
    }

    /// Clear the current search
    pub fn clear_search(&mut self) {
        self.search_state = None;
    }

    /// Get the number of wrapped lines (for memory tracking)
    pub fn wrapped_line_count(&self) -> usize {
        self.wrapped_lines.len()
    }

    /// Jump to the next match
    pub fn next_match(&mut self) -> bool {
        let new_idx = if let Some(state) = &mut self.search_state {
            if !state.matches.is_empty() {
                state.current_match_idx = (state.current_match_idx + 1) % state.matches.len();
                Some(state.current_match_idx)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(idx) = new_idx {
            self.scroll_to_match(idx);
            true
        } else {
            false
        }
    }

    /// Jump to the previous match
    pub fn prev_match(&mut self) -> bool {
        let new_idx = if let Some(state) = &mut self.search_state {
            if !state.matches.is_empty() {
                if state.current_match_idx == 0 {
                    state.current_match_idx = state.matches.len() - 1;
                } else {
                    state.current_match_idx -= 1;
                }
                Some(state.current_match_idx)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(idx) = new_idx {
            self.scroll_to_match(idx);
            true
        } else {
            false
        }
    }

    /// Get search info for display: (current_idx, total_matches)
    pub fn search_info(&self) -> Option<(usize, usize)> {
        self.search_state.as_ref().map(|state| {
            (state.current_match_idx + 1, state.matches.len())
        })
    }

    /// Scroll to show a specific match
    fn scroll_to_match(&mut self, match_idx: usize) {
        if let Some(state) = &self.search_state {
            if let Some(m) = state.matches.get(match_idx) {
                // Set scroll position to show this line
                // Try to center the match in the view
                let target_line = m.line_idx;
                let offset = self.last_visible_height / 2;
                let scroll_pos = target_line.saturating_sub(offset);

                self.scroll_position = Some(scroll_pos);
            }
        }
    }

    /// Create spans for a line with highlighted search matches
    fn create_highlighted_spans(
        &self,
        wrapped: &WrappedLine,
        line_matches: &[&SearchMatch],
        current_match: Option<&SearchMatch>,
    ) -> Vec<Span> {
        // Build the full line text to know character positions
        let full_text: String = wrapped.spans.iter()
            .map(|(text, _)| text.as_str())
            .collect();

        // Collect all character positions that should be highlighted
        let mut highlight_ranges: Vec<(usize, usize, bool)> = Vec::new();  // (start, end, is_current)

        for m in line_matches {
            let is_current = current_match.map_or(false, |cm| {
                cm.line_idx == m.line_idx && cm.start == m.start && cm.end == m.end
            });
            highlight_ranges.push((m.start, m.end, is_current));
        }

        // Sort ranges by start position
        highlight_ranges.sort_by_key(|(start, _, _)| *start);

        // Reconstruct spans, splitting where highlights occur
        let mut result_spans = Vec::new();
        let mut char_pos = 0;
        let mut highlight_idx = 0;

        for (text, style) in &wrapped.spans {
            let text_len = text.len();
            let span_start = char_pos;
            let span_end = char_pos + text_len;

            let mut current_pos = span_start;

            // Check for highlights that overlap this span
            while highlight_idx < highlight_ranges.len() && highlight_ranges[highlight_idx].0 < span_end {
                let (hl_start, hl_end, is_current) = highlight_ranges[highlight_idx];

                if hl_end <= span_start {
                    // Highlight is before this span
                    highlight_idx += 1;
                    continue;
                }

                // Add non-highlighted part before the match
                if current_pos < hl_start && hl_start >= span_start {
                    let offset = current_pos - span_start;
                    let length = hl_start - current_pos;
                    let substr = &text[offset..offset + length];
                    result_spans.push(Span::styled(substr.to_string(), *style));
                    current_pos = hl_start;
                }

                // Add highlighted part
                if current_pos < hl_end && current_pos >= span_start {
                    let offset = current_pos - span_start;
                    let end_pos = hl_end.min(span_end);
                    let length = end_pos - current_pos;
                    let substr = &text[offset..offset + length];

                    // Use different colors for current match vs other matches
                    let highlight_style = if is_current {
                        Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().bg(Color::DarkGray).fg(Color::White)
                    };

                    result_spans.push(Span::styled(substr.to_string(), highlight_style));
                    current_pos = end_pos;
                }

                if hl_end <= span_end {
                    highlight_idx += 1;
                }

                if current_pos >= span_end {
                    break;
                }
            }

            // Add remaining non-highlighted part
            if current_pos < span_end {
                let offset = current_pos - span_start;
                let substr = &text[offset..];
                result_spans.push(Span::styled(substr.to_string(), *style));
            }

            char_pos = span_end;
        }

        result_spans
    }

    /// Re-wrap all logical lines with the current width
    fn rewrap_all(&mut self) {
        self.wrapped_lines.clear();

        let width = if self.last_width > 0 {
            self.last_width as usize
        } else {
            80
        };

        // Wrap each logical line
        for logical_line in &self.logical_lines {
            let wrapped = self.wrap_styled_spans(&logical_line.spans, width);
            for line in wrapped {
                self.wrapped_lines.push_back(line);
            }
        }

        self.needs_rewrap = false;
    }
}

impl TextWindow {
    /// Render the window with optional focus indicator
    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        // Update width for wrapping
        let inner_width = area.width.saturating_sub(2); // Account for borders
        self.set_width(inner_width);

        // Re-wrap all lines if width changed
        if self.needs_rewrap {
            self.rewrap_all();
        }

        // Build visible lines for display
        // Buffer storage: wrapped_lines[0] = oldest, wrapped_lines[end] = newest
        // Display: oldest at top, newest at bottom (standard chat/log view)
        // scroll_offset = how many lines back from the end we're viewing
        // scroll_offset=0 means viewing the bottom (live, newest lines)
        // scroll_offset>0 means scrolled back to view older lines

        let visible_height = area.height.saturating_sub(2) as usize; // Account for borders
        self.last_visible_height = visible_height;  // Save for scroll calculations
        let total_lines = self.wrapped_lines.len();

        if total_lines == 0 {
            // No lines to display
            let borders = crate::config::parse_border_sides(&self.border_sides);
            let paragraph = Paragraph::new(vec![])
                .block(
                    if focused {
                        Block::default()
                            .title(self.title.as_str())
                            .borders(borders)
                            .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    } else {
                        Block::default()
                            .title(self.title.as_str())
                            .borders(borders)
                    }
                );
            paragraph.render(area, buf);
            return;
        }

        // Calculate which lines to display based on scroll mode
        let (start_line, end_line) = if let Some(pos) = self.scroll_position {
            // Scrolled back - use absolute position (frozen view)
            // Display visible_height lines starting from scroll_position
            let start = pos;
            let end = (pos + visible_height).min(total_lines);
            (start, end)
        } else {
            // Live view mode - show the last visible_height lines
            // Example: 100 total lines, visible_height=20, scroll_offset=0
            //   -> show lines 80-99 (the last 20)
            let end = total_lines.saturating_sub(self.scroll_offset);
            let start = end.saturating_sub(visible_height);
            (start, end)
        };

        // Collect lines from buffer (oldest to newest order)
        let mut display_lines: Vec<Line> = Vec::new();
        for idx in start_line..end_line {
            if let Some(wrapped) = self.wrapped_lines.get(idx) {
                // Check if this line has search matches
                let line_matches: Vec<&SearchMatch> = self.search_state.as_ref()
                    .map(|state| {
                        state.matches.iter()
                            .filter(|m| m.line_idx == idx)
                            .collect()
                    })
                    .unwrap_or_default();

                let current_match = self.search_state.as_ref()
                    .and_then(|state| state.matches.get(state.current_match_idx));

                let spans: Vec<Span> = if line_matches.is_empty() {
                    // No matches on this line - render normally
                    wrapped.spans.iter()
                        .map(|(text, style)| Span::styled(text.clone(), *style))
                        .collect()
                } else {
                    // Has matches - need to highlight them
                    self.create_highlighted_spans(wrapped, &line_matches, current_match)
                };

                display_lines.push(Line::from(spans));
            }
        }

        // Lines are already in the correct order (oldest at top, newest at bottom)
        // No need to reverse!

        // Build block with focus indicator and scroll position
        // Show scroll indicator when not in live view
        let title = if let Some(pos) = self.scroll_position {
            let lines_from_end = total_lines.saturating_sub(pos);
            format!("{} [↑{}]", self.title, lines_from_end)
        } else if self.scroll_offset > 0 {
            format!("{} [↑{}]", self.title, self.scroll_offset)
        } else {
            self.title.clone()
        };

        // Create block based on border configuration
        let mut block = if self.show_border {
            let borders = crate::config::parse_border_sides(&self.border_sides);
            Block::default()
                .title(title.as_str())
                .borders(borders)
        } else {
            Block::default() // No borders
        };

        // Apply border style if specified
        if let Some(ref style_name) = self.border_style {
            let border_type = match style_name.as_str() {
                "double" => BorderType::Double,
                "rounded" => BorderType::Rounded,
                "thick" => BorderType::Thick,
                _ => BorderType::Plain, // "single" or default
            };
            block = block.border_type(border_type);
        }

        // Apply border color
        let mut border_style = Style::default();
        if let Some(ref color_hex) = self.border_color {
            if let Some(color) = Self::parse_hex_color(color_hex) {
                border_style = border_style.fg(color);
            }
        }

        // Override with focus color if focused
        if focused {
            border_style = border_style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
        }

        if self.show_border {
            block = block.border_style(border_style);
        }

        let paragraph = Paragraph::new(display_lines).block(block);
        paragraph.render(area, buf);
    }

    fn parse_hex_color(hex: &str) -> Option<Color> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    }
}

impl Widget for &mut TextWindow {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_with_focus(area, buf, false);
    }
}
