//! Core text window widget that powers most streams in the TUI.
//!
//! Responsible for buffering, wrapping, highlighting, search, and selection
//! logic in a way that mirrors Profanity/Vellum's behavior.

use crate::frontend::tui::{crossterm_bridge, title_position::{self, TitlePosition}};
use crate::config::HighlightPattern;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph, Widget},
};
use regex::Regex;
use std::collections::VecDeque;

// Per-character style info for layering
#[derive(Clone, Copy)]
struct CharStyle {
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
    span_type: SpanType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanType {
    Normal,      // Regular text
    Link,        // <a> tag from parser
    Monsterbold, // <preset id="monsterbold"> from parser
    Spell,       // <spell> tag from parser
    Speech,      // <preset id="speech"> from parser
    System,      // Client/system messages; skip highlights/replacements
}

/// Link metadata for clickable game objects
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkData {
    pub exist_id: String,      // Unique ID for this game object
    pub noun: String,          // The noun/name of the object
    pub text: String,          // The actual displayed text (e.g., "nacreous pearl bangle")
    pub coord: Option<String>, // Optional coord for direct commands (e.g., "2524,1864" for movement)
}

#[derive(Clone)]
pub struct StyledText {
    pub content: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub span_type: SpanType,         // Semantic type for priority layering
    pub link_data: Option<LinkData>, // Link metadata if span_type is Link
}

// One display line (post-wrapping) with multiple styled spans
#[derive(Clone)]
struct WrappedLine {
    spans: Vec<(String, Style, SpanType, Option<LinkData>)>,
}

// One logical line (before wrapping) - stores original styled content
#[derive(Clone)]
struct LogicalLine {
    spans: Vec<(String, Style, SpanType, Option<LinkData>)>,
}

// Match location: (line_index, start_char, end_char)
#[derive(Clone, Debug)]
struct SearchMatch {
    line_idx: usize, // Index in wrapped_lines
    start: usize,    // Character offset in the line text
    end: usize,      // Character offset (exclusive)
}

struct SearchState {
    regex: Regex,
    matches: Vec<SearchMatch>,
    current_match_idx: usize, // Which match is currently selected
}

// Manual Clone implementation because SearchState contains Regex (not Clone)
// Also skip highlight_regexes and fast_matcher which contain Regex/AhoCorasick
pub struct TextWindow {
    // Store original logical lines (for re-wrapping)
    logical_lines: VecDeque<LogicalLine>,
    // Cached wrapped lines (invalidated when width changes)
    wrapped_lines: VecDeque<WrappedLine>,
    // Wrap timing samples (drained after render to feed performance stats)
    wrap_samples: Vec<std::time::Duration>,
    // Accumulate styled chunks for current logical line
    current_line_spans: Vec<(String, Style, SpanType, Option<LinkData>)>,
    max_lines: usize,
    scroll_offset: usize, // Lines back from end when at bottom (0 = live view)
    scroll_position: Option<usize>, // Absolute line position when scrolled back (None = following live)
    last_visible_height: usize,     // Track the visible height from last render
    last_render_range: Option<(usize, usize)>, // Track last (start_line, end_line) to detect content changes
    title: String,
    title_position: TitlePosition,
    last_width: u16,
    needs_rewrap: bool, // Flag to trigger re-wrapping
    // Border configuration
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: crate::config::BorderSides,
    background_color: Option<Color>,
    default_text_color: Option<Color>,
    content_align: Option<String>,
    // Search functionality
    search_state: Option<SearchState>,
    // Highlight patterns
    highlights: Vec<HighlightPattern>,
    // Precompiled highlight regexes (parallel to highlights vec, only for non-fast_parse)
    highlight_regexes: Vec<Option<Regex>>,
    // Aho-Corasick matcher for fast_parse patterns
    fast_matcher: Option<AhoCorasick>,
    // Maps Aho-Corasick match index -> highlight index
    // Link toggling
    links_enabled: bool,
    fast_pattern_map: Vec<usize>,
    // Recent links cache for click detection
    recent_links: VecDeque<LinkData>,
    max_recent_links: usize,
    // Timestamp configuration
    show_timestamps: bool,
}

impl Clone for TextWindow {
    fn clone(&self) -> Self {
        Self {
            logical_lines: self.logical_lines.clone(),
            wrapped_lines: self.wrapped_lines.clone(),
            wrap_samples: Vec::new(),
            current_line_spans: self.current_line_spans.clone(),
            max_lines: self.max_lines,
            scroll_offset: self.scroll_offset,
            scroll_position: self.scroll_position,
            last_visible_height: self.last_visible_height,
            last_render_range: self.last_render_range,
            title: self.title.clone(),
            title_position: self.title_position,
            last_width: self.last_width,
            needs_rewrap: self.needs_rewrap,
            show_border: self.show_border,
            border_style: self.border_style.clone(),
            border_color: self.border_color.clone(),
            border_sides: self.border_sides.clone(),
            background_color: self.background_color,
            default_text_color: self.default_text_color,
            content_align: self.content_align.clone(),
            // Skip search_state (contains Regex which doesn't implement Clone)
            search_state: None,
            highlights: self.highlights.clone(),
            // Skip highlight_regexes (contains Regex)
            highlight_regexes: vec![],
            // Skip fast_matcher (AhoCorasick doesn't implement Clone)
            fast_matcher: None,
            links_enabled: self.links_enabled,
            fast_pattern_map: self.fast_pattern_map.clone(),
            recent_links: self.recent_links.clone(),
            max_recent_links: self.max_recent_links,
            show_timestamps: self.show_timestamps,
        }
    }
}

impl TextWindow {
    pub fn new(title: impl Into<String>, max_lines: usize) -> Self {
        Self {
            logical_lines: VecDeque::with_capacity(max_lines),
            wrapped_lines: VecDeque::with_capacity(max_lines * 2), // More space for wrapped
            wrap_samples: Vec::new(),
            current_line_spans: Vec::new(),
            max_lines,
            scroll_offset: 0,
            title: title.into(),
            title_position: TitlePosition::TopLeft,
            last_width: 0,
            needs_rewrap: false,
            last_render_range: None,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(),
            background_color: None,
            default_text_color: None,
            content_align: None,
            scroll_position: None,         // Start in live view mode
            last_visible_height: 20,       // Reasonable default
            search_state: None,            // No active search
            highlights: Vec::new(),        // No highlights by default
            highlight_regexes: Vec::new(), // No precompiled regexes by default
            fast_matcher: None,            // No Aho-Corasick matcher by default
            fast_pattern_map: Vec::new(),  // No fast pattern mapping by default
            recent_links: VecDeque::new(), // No recent links yet
            max_recent_links: 100,         // Keep last 100 links
            show_timestamps: false,        // Timestamps off by default
            links_enabled: true,           // Links enabled by default
        }
    }

    pub fn set_highlights(&mut self, highlights: Vec<HighlightPattern>) {
        // Separate fast_parse patterns from regex patterns
        let mut fast_patterns: Vec<String> = Vec::new();
        let mut fast_map: Vec<usize> = Vec::new();

        // Build regex list and collect fast_parse patterns
        self.highlight_regexes = highlights
            .iter()
            .enumerate()
            .map(|(i, h)| {
                if h.fast_parse {
                    // Split pattern on | and add to Aho-Corasick
                    for literal in h.pattern.split('|') {
                        let literal = literal.trim();
                        if !literal.is_empty() {
                            fast_patterns.push(literal.to_string());
                            fast_map.push(i); // Map this pattern back to highlight index
                        }
                    }
                    None // Don't compile as regex
                } else {
                    // Regular regex pattern
                    Regex::new(&h.pattern).ok()
                }
            })
            .collect();

        // Build Aho-Corasick matcher for fast_parse patterns with whole-word matching only
        if !fast_patterns.is_empty() {
            self.fast_matcher = AhoCorasickBuilder::new()
                .match_kind(MatchKind::Standard) // Standard matching
                .build(&fast_patterns)
                .ok();
            self.fast_pattern_map = fast_map;
        } else {
            self.fast_matcher = None;
            self.fast_pattern_map = Vec::new();
        }

        self.highlights = highlights;
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

    pub fn set_border_sides(&mut self, border_sides: crate::config::BorderSides) {
        self.border_sides = border_sides;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = Self::parse_color_setting(color);
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.default_text_color = Self::parse_color_setting(color);
    }

    fn parse_color_setting(color: Option<String>) -> Option<Color> {
        color.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed == "-" {
                None
            } else {
                Self::parse_hex_color(trimmed)
            }
        })
    }

    pub fn set_content_align(&mut self, align: Option<String>) {
        self.content_align = align;
    }

    /// Update the window title
    pub fn set_title(&mut self, title: String) {
        self.title = title;
        self.needs_rewrap = true;
    }

    pub fn set_title_position(&mut self, position: TitlePosition) {
        self.title_position = position;
    }

    pub fn set_show_timestamps(&mut self, show: bool) {
        self.show_timestamps = show;
    }

    pub fn toggle_links(&mut self) {
        self.links_enabled = !self.links_enabled;
    }

    pub fn get_links_enabled(&self) -> bool {
        self.links_enabled
    }

    pub fn has_border(&self) -> bool {
        self.show_border
    }

    /// Format current time as timestamp (e.g., "[7:08 AM]")
    fn format_timestamp() -> String {
        use chrono::Local;
        let now = Local::now();
        format!(" [{}]", now.format("%l:%M %p").to_string().trim())
    }

    pub fn add_text(&mut self, styled: StyledText) {
        let mut style = Style::default();
        if let Some(fg) = styled.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = styled.bg {
            style = style.bg(bg);
        }
        if styled.bold {
            style = style.add_modifier(Modifier::BOLD);
        }

        // Only process links if links_enabled is true
        let link_data = if self.links_enabled {
            // Cache link data if present, accumulating text for the same exist_id
            if let Some(ref link_data) = styled.link_data {
                // Check if we already have this exist_id in the most recent entry
                let should_append = if let Some(last) = self.recent_links.back_mut() {
                    if last.exist_id == link_data.exist_id {
                        // Append to existing text (no debug log for appends - too spammy)
                        last.text.push_str(&styled.content);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !should_append {
                    // New link - create new entry with this content as the text
                    let mut new_link = link_data.clone();
                    new_link.text = styled.content.clone();
                    self.recent_links.push_back(new_link);
                    if self.recent_links.len() > self.max_recent_links {
                        self.recent_links.pop_front();
                    }
                }
            }
            styled.link_data.clone()
        } else {
            // Links disabled - don't cache or include link data
            None
        };

        // Add this styled chunk to current line with semantic type and link metadata
        self.current_line_spans
            .push((styled.content, style, styled.span_type, link_data));
    }

    pub fn finish_line(&mut self, _width: u16) {
        if self.current_line_spans.is_empty() {
            return;
        }

        // Apply highlights before storing/wrapping
        self.apply_highlights();

        // Add timestamp if enabled (before storing/wrapping)
        if self.show_timestamps {
            let timestamp = Self::format_timestamp();
            let timestamp_style = Style::default().fg(Color::DarkGray);
            self.current_line_spans
                .push((timestamp, timestamp_style, SpanType::Normal, None));
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

        let wrap_start = std::time::Instant::now();
        let wrapped = self.wrap_styled_spans(&self.current_line_spans, actual_width as usize);
        let wrap_duration = wrap_start.elapsed();
        self.wrap_samples.push(wrap_duration);

        // Add wrapped lines to the END
        for line in wrapped {
            self.wrapped_lines.push_back(line);
        }

        self.current_line_spans.clear();
    }

    /// Apply highlight patterns to current line spans with proper priority layering
    fn apply_highlights(&mut self) {
        // Skip highlight transforms for pure system lines
        if self
            .current_line_spans
            .iter()
            .all(|(_, _, span_type, _)| matches!(span_type, SpanType::System))
        {
            return;
        }

        if self.highlights.is_empty() {
            return;
        }

        // STEP 1: Build character-by-character style and link maps from current spans
        let mut char_styles: Vec<CharStyle> = Vec::new();
        let mut char_links: Vec<Option<LinkData>> = Vec::new();
        for (content, style, span_type, link) in &self.current_line_spans {
            for _ in content.chars() {
                char_styles.push(CharStyle {
                    fg: style.fg,
                    bg: style.bg,
                    bold: style.add_modifier.contains(Modifier::BOLD),
                    span_type: *span_type,
                });
                char_links.push(link.clone());
            }
        }

        if char_styles.is_empty() {
            return;
        }

        // STEP 2: Build full text for pattern matching
        let mut full_text: String = self
            .current_line_spans
            .iter()
            .map(|(content, _, _, _)| content.as_str())
            .collect();

        // STEP 3: Find all highlight matches (both Aho-Corasick and regex)
        #[derive(Clone)]
        struct MatchInfo {
            start_byte: usize,
            end_byte: usize,
            fg: Option<Color>,
            bg: Option<Color>,
            bold: bool,
            color_entire_line: bool,
            replace: Option<String>,
        }

        let mut matches: Vec<MatchInfo> = Vec::new();

        // Try Aho-Corasick fast patterns (with word boundary checking)
        if let Some(ref matcher) = self.fast_matcher {
            for mat in matcher.find_iter(&full_text) {
                let start = mat.start();
                let end = mat.end();
                let bytes = full_text.as_bytes();

                let is_word_start = start == 0 || {
                    bytes.get(start - 1).is_none_or(|&b| {
                        let c = b as char;
                        !c.is_alphanumeric() && c != '_'
                    })
                };

                let is_word_end = end >= bytes.len() || {
                    bytes.get(end).is_none_or(|&b| {
                        let c = b as char;
                        !c.is_alphanumeric() && c != '_'
                    })
                };

                if is_word_start && is_word_end {
                    if let Some(&highlight_idx) =
                        self.fast_pattern_map.get(mat.pattern().as_usize())
                    {
                        if let Some(highlight) = self.highlights.get(highlight_idx) {
                            let fg = highlight.fg.as_ref().and_then(|h| Self::parse_hex_color(h));
                            let bg = highlight.bg.as_ref().and_then(|h| Self::parse_hex_color(h));
                            matches.push(MatchInfo {
                                start_byte: start,
                                end_byte: end,
                                fg,
                                bg,
                                bold: highlight.bold,
                                color_entire_line: highlight.color_entire_line,
                                replace: highlight.replace.clone(),
                            });
                        }
                    }
                }
            }
        }

        // Try regex patterns
        for (i, highlight) in self.highlights.iter().enumerate() {
            if highlight.fast_parse {
                continue; // Already handled by Aho-Corasick
            }

            if let Some(Some(regex)) = self.highlight_regexes.get(i) {
                for m in regex.find_iter(&full_text) {
                    let fg = highlight.fg.as_ref().and_then(|h| Self::parse_hex_color(h));
                    let bg = highlight.bg.as_ref().and_then(|h| Self::parse_hex_color(h));
                    matches.push(MatchInfo {
                        start_byte: m.start(),
                        end_byte: m.end(),
                        fg,
                        bg,
                        bold: highlight.bold,
                        color_entire_line: highlight.color_entire_line,
                        replace: highlight.replace.clone(),
                    });
                }
            }
        }

        if !matches.is_empty() {
            use std::collections::HashMap;

            // Map byte offsets to char indices
            let mut byte_to_char: HashMap<usize, usize> = HashMap::new();
            for (idx, (byte, _ch)) in full_text.char_indices().enumerate() {
                byte_to_char.insert(byte, idx);
            }

            let full_text_chars: Vec<char> = full_text.chars().collect();
            matches.sort_by_key(|m| m.start_byte);

            let mut new_text = String::new();
            let mut new_styles: Vec<CharStyle> = Vec::new();
            let mut new_links: Vec<Option<LinkData>> = Vec::new();
            let mut new_match_ranges: Vec<(usize, usize, MatchInfo)> = Vec::new();

            let mut last_char_idx = 0usize;
            for m in matches {
                let start_char = *byte_to_char.get(&m.start_byte).unwrap_or(&last_char_idx);
                let end_char = *byte_to_char.get(&m.end_byte).unwrap_or(&full_text_chars.len());
                if start_char < last_char_idx {
                    continue; // overlapping; skip
                }

                // Copy untouched region
                for i in last_char_idx..start_char {
                    new_text.push(full_text_chars[i]);
                    new_styles.push(char_styles.get(i).cloned().unwrap_or(CharStyle {
                        fg: None,
                        bg: None,
                        bold: false,
                        span_type: SpanType::Normal,
                    }));
                    new_links.push(char_links.get(i).cloned().unwrap_or(None));
                }

                let new_start = new_styles.len();

                // Replacement or original segment
                if let Some(ref repl) = m.replace {
                    let base_style = char_styles
                        .get(start_char)
                        .cloned()
                        .unwrap_or(CharStyle {
                            fg: None,
                            bg: None,
                            bold: false,
                            span_type: SpanType::Normal,
                        });
                    for ch in repl.chars() {
                        new_text.push(ch);
                        new_styles.push(base_style);
                        new_links.push(None);
                    }
                } else {
                    for i in start_char..end_char {
                        new_text.push(full_text_chars[i]);
                        new_styles.push(char_styles.get(i).cloned().unwrap_or(CharStyle {
                            fg: None,
                            bg: None,
                            bold: false,
                            span_type: SpanType::Normal,
                        }));
                        new_links.push(char_links.get(i).cloned().unwrap_or(None));
                    }
                }

                let new_end = new_styles.len();
                new_match_ranges.push((
                    new_start,
                    new_end,
                    MatchInfo {
                        start_byte: m.start_byte,
                        end_byte: m.end_byte,
                        fg: m.fg,
                        bg: m.bg,
                        bold: m.bold,
                        color_entire_line: m.color_entire_line,
                        replace: m.replace,
                    },
                ));

                last_char_idx = end_char;
            }

            // Tail
            for i in last_char_idx..full_text_chars.len() {
                new_text.push(full_text_chars[i]);
                new_styles.push(char_styles.get(i).cloned().unwrap_or(CharStyle {
                    fg: None,
                    bg: None,
                    bold: false,
                    span_type: SpanType::Normal,
                }));
                new_links.push(char_links.get(i).cloned().unwrap_or(None));
            }

            // Apply highlight styling on rewritten text
            for (start, end, info) in &new_match_ranges {
                if info.color_entire_line {
                    for cs in new_styles.iter_mut() {
                        if cs.span_type == SpanType::Link || cs.span_type == SpanType::Monsterbold {
                            if let Some(color) = info.bg {
                                cs.bg = Some(color);
                            }
                        } else {
                            if let Some(color) = info.fg {
                                cs.fg = Some(color);
                            }
                            if let Some(color) = info.bg {
                                cs.bg = Some(color);
                            }
                            if info.bold {
                                cs.bold = true;
                            }
                        }
                    }
                    break; // only first whole-line match applies
                } else {
                    for idx in *start..(*end).min(new_styles.len()) {
                        if let Some(color) = info.fg {
                            new_styles[idx].fg = Some(color);
                        }
                        if let Some(color) = info.bg {
                            new_styles[idx].bg = Some(color);
                        }
                        if info.bold {
                            new_styles[idx].bold = true;
                        }
                    }
                }
            }

            char_styles = new_styles;
            char_links = new_links;
            full_text = new_text;
        }

        // STEP 5: Reconstruct spans from char_styles with proper splitting
        let mut new_spans: Vec<(String, Style, SpanType, Option<LinkData>)> = Vec::new();
        let full_text_chars: Vec<char> = full_text.chars().collect();

        let mut i = 0;
        while i < char_styles.len() {
            let current_style = char_styles[i];
            let current_link = char_links.get(i).cloned().unwrap_or(None);
            let mut content = String::new();
            content.push(full_text_chars[i]);

            // Extend span while style matches
            i += 1;
            while i < char_styles.len() {
                let next_style = char_styles[i];
                let next_link = char_links.get(i).cloned().unwrap_or(None);
                if next_style.fg == current_style.fg
                    && next_style.bg == current_style.bg
                    && next_style.bold == current_style.bold
                    && next_style.span_type == current_style.span_type
                    && next_link == current_link
                {
                    content.push(full_text_chars[i]);
                    i += 1;
                } else {
                    break;
                }
            }

            // Build Style
            let mut style = Style::default();
            if let Some(fg) = current_style.fg {
                style = style.fg(fg);
            }
            if let Some(bg) = current_style.bg {
                style = style.bg(bg);
            }
            if current_style.bold {
                style = style.add_modifier(Modifier::BOLD);
            }

            new_spans.push((content, style, current_style.span_type, current_link));
        }

        // Replace current_line_spans with new layered spans
        self.current_line_spans = new_spans;
    }

    // Wrap a series of styled spans into multiple display lines
    fn wrap_styled_spans(
        &self,
        spans: &[(String, Style, SpanType, Option<LinkData>)],
        width: usize,
    ) -> Vec<WrappedLine> {
        if width == 0 {
            return vec![];
        }

        let mut result = Vec::new();
        let mut current_line_spans: Vec<(String, Style, SpanType, Option<LinkData>)> = Vec::new();
        let mut current_line_len = 0;

        // Track word buffer for smart wrapping
        let mut word_buffer: Vec<(String, Style, SpanType, Option<LinkData>)> = Vec::new();
        let mut word_buffer_len = 0;
        let mut in_word = false;

        for (text, style, span_type, link) in spans {
            for ch in text.chars() {
                let is_whitespace = ch.is_whitespace();

                if is_whitespace {
                    // Flush word buffer if we have one
                    if in_word && !word_buffer.is_empty() {
                        // Check if word fits on current line
                        if current_line_len + word_buffer_len <= width {
                            // Word fits - add it to current line
                            for (word_text, word_style, word_type, word_link) in
                                word_buffer.drain(..)
                            {
                                Self::append_to_line(
                                    &mut current_line_spans,
                                    word_text,
                                    word_style,
                                    word_type,
                                    word_link,
                                );
                            }
                            current_line_len += word_buffer_len;
                        } else if word_buffer_len <= width {
                            // Word doesn't fit on current line, but fits on new line - wrap
                            if !current_line_spans.is_empty() {
                                result.push(WrappedLine {
                                    spans: current_line_spans.clone(),
                                });
                                current_line_spans.clear();
                                current_line_len = 0;
                            }
                            // Add word to new line
                            for (word_text, word_style, word_type, word_link) in
                                word_buffer.drain(..)
                            {
                                Self::append_to_line(
                                    &mut current_line_spans,
                                    word_text,
                                    word_style,
                                    word_type,
                                    word_link,
                                );
                            }
                            current_line_len += word_buffer_len;
                        } else {
                            // Word is longer than width - must break it mid-word
                            for (word_text, word_style, word_type, word_link) in
                                word_buffer.drain(..)
                            {
                                for word_ch in word_text.chars() {
                                    if current_line_len >= width {
                                        result.push(WrappedLine {
                                            spans: current_line_spans.clone(),
                                        });
                                        current_line_spans.clear();
                                        current_line_len = 0;
                                    }
                                    Self::append_to_line(
                                        &mut current_line_spans,
                                        word_ch.to_string(),
                                        word_style,
                                        word_type,
                                        word_link.clone(),
                                    );
                                    current_line_len += 1;
                                }
                            }
                        }
                        word_buffer_len = 0;
                        in_word = false;
                    }

                    // Add whitespace immediately (don't buffer it)
                    if current_line_len >= width {
                        // Wrap before whitespace
                        result.push(WrappedLine {
                            spans: current_line_spans.clone(),
                        });
                        current_line_spans.clear();
                        current_line_len = 0;
                        // Don't add whitespace at start of new line
                        continue;
                    }
                    Self::append_to_line(
                        &mut current_line_spans,
                        ch.to_string(),
                        *style,
                        *span_type,
                        link.clone(),
                    );
                    current_line_len += 1;
                } else {
                    // Non-whitespace character - add to word buffer
                    in_word = true;
                    Self::append_to_buffer(
                        &mut word_buffer,
                        ch.to_string(),
                        *style,
                        *span_type,
                        link.clone(),
                    );
                    word_buffer_len += 1;
                }
            }
        }

        // Flush remaining word buffer
        if !word_buffer.is_empty() {
            if current_line_len + word_buffer_len <= width {
                // Word fits on current line
                for (word_text, word_style, word_type, word_link) in word_buffer {
                    Self::append_to_line(
                        &mut current_line_spans,
                        word_text,
                        word_style,
                        word_type,
                        word_link,
                    );
                }
            } else if word_buffer_len <= width {
                // Word needs new line
                if !current_line_spans.is_empty() {
                    result.push(WrappedLine {
                        spans: current_line_spans.clone(),
                    });
                    current_line_spans.clear();
                }
                for (word_text, word_style, word_type, word_link) in word_buffer {
                    Self::append_to_line(
                        &mut current_line_spans,
                        word_text,
                        word_style,
                        word_type,
                        word_link,
                    );
                }
            } else {
                // Word is too long - must break it
                for (word_text, word_style, word_type, word_link) in word_buffer {
                    for word_ch in word_text.chars() {
                        if current_line_len >= width {
                            result.push(WrappedLine {
                                spans: current_line_spans.clone(),
                            });
                            current_line_spans.clear();
                            current_line_len = 0;
                        }
                        Self::append_to_line(
                            &mut current_line_spans,
                            word_ch.to_string(),
                            word_style,
                            word_type,
                            word_link.clone(),
                        );
                        current_line_len += 1;
                    }
                }
            }
        }

        // Push any remaining content
        if !current_line_spans.is_empty() {
            result.push(WrappedLine {
                spans: current_line_spans,
            });
        }

        if result.is_empty() {
            // Return at least one empty line
            result.push(WrappedLine { spans: vec![] });
        }

        result
    }

    // Helper to append text to a span list, merging with last span if style matches
    fn append_to_line(
        spans: &mut Vec<(String, Style, SpanType, Option<LinkData>)>,
        text: String,
        style: Style,
        span_type: SpanType,
        link: Option<LinkData>,
    ) {
        if let Some((last_text, last_style, last_type, last_link)) = spans.last_mut() {
            if *last_style == style && *last_type == span_type && *last_link == link {
                last_text.push_str(&text);
            } else {
                spans.push((text, style, span_type, link));
            }
        } else {
            spans.push((text, style, span_type, link));
        }
    }

    // Helper to append text to buffer, merging with last entry if style matches
    fn append_to_buffer(
        buffer: &mut Vec<(String, Style, SpanType, Option<LinkData>)>,
        text: String,
        style: Style,
        span_type: SpanType,
        link: Option<LinkData>,
    ) {
        if let Some((last_text, last_style, last_type, last_link)) = buffer.last_mut() {
            if *last_style == style && *last_type == span_type && *last_link == link {
                last_text.push_str(&text);
            } else {
                buffer.push((text, style, span_type, link));
            }
        } else {
            buffer.push((text, style, span_type, link));
        }
    }

    pub fn update_inner_width(&mut self, width: u16) {
        self.last_width = width;
        // Note: No rewrapping needed - lines are already character-wrapped at exact width
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
            let line_text: String = wrapped_line
                .spans
                .iter()
                .map(|(text, _, _, _)| text.as_str())
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

    /// Drain collected wrap timing samples (used by performance stats)
    pub fn take_wrap_samples(&mut self) -> Vec<std::time::Duration> {
        std::mem::take(&mut self.wrap_samples)
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
        self.search_state
            .as_ref()
            .map(|state| (state.current_match_idx + 1, state.matches.len()))
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
    ) -> Vec<Span<'_>> {
        // Build the full line text to know character positions
        let _full_text: String = wrapped
            .spans
            .iter()
            .map(|(text, _, _, _)| text.as_str())
            .collect();

        // Collect all character positions that should be highlighted
        let mut highlight_ranges: Vec<(usize, usize, bool)> = Vec::new(); // (start, end, is_current)

        for m in line_matches {
            let is_current = current_match.is_some_and(|cm| {
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

        for (text, style, _span_type, _link) in &wrapped.spans {
            let base_style = self.style_with_defaults(*style);
            let text_len = text.len();
            let span_start = char_pos;
            let span_end = char_pos + text_len;

            let mut current_pos = span_start;

            // Check for highlights that overlap this span
            while highlight_idx < highlight_ranges.len()
                && highlight_ranges[highlight_idx].0 < span_end
            {
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
                    result_spans.push(Span::styled(substr.to_string(), base_style));
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
                        Style::default()
                            .bg(Color::Yellow)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().bg(Color::DarkGray).fg(Color::White)
                    };

                    let highlight_style = self.style_with_defaults(highlight_style);
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
                result_spans.push(Span::styled(substr.to_string(), base_style));
            }

            char_pos = span_end;
        }

        result_spans
    }

    /// Create spans for a line with text selection highlighting
    fn create_spans_with_selection(
        &self,
        wrapped: &WrappedLine,
        line_idx: usize,
        selection_state: Option<&crate::selection::SelectionState>,
        selection_bg: Option<Color>,
        window_index: usize,
    ) -> Vec<Span<'_>> {
        // If no selection or no background color, render normally
        let selection = match (selection_state, selection_bg) {
            (Some(sel), Some(bg)) if sel.active => (sel, bg),
            _ => {
                return wrapped
                    .spans
                    .iter()
                    .map(|(text, style, _span_type, _link)| {
                        let resolved = self.style_with_defaults(*style);
                        Span::styled(text.clone(), resolved)
                    })
                    .collect();
            }
        };

        let (sel, bg_color) = selection;

        // Reconstruct spans, applying selection background where needed
        let mut result_spans = Vec::new();
        let mut char_pos = 0;

        for (text, style, _span_type, _link) in &wrapped.spans {
            let text_chars: Vec<char> = text.chars().collect();
            let text_len = text_chars.len(); // Character count, not byte count
            let span_start = char_pos;
            let span_end = char_pos + text_len;

            let mut current_pos = span_start;
            let mut char_idx = 0;

            // Process each character to check if it's selected
            while char_idx < text_chars.len() {
                let char_col = current_pos;
                let is_selected = sel.contains(window_index, line_idx, char_col);

                // Collect consecutive characters with same selection state
                let mut chunk = String::new();
                chunk.push(text_chars[char_idx]);
                char_idx += 1;
                current_pos += 1;

                while char_idx < text_chars.len() {
                    let next_col = current_pos;
                    let next_selected = sel.contains(window_index, line_idx, next_col);

                    if next_selected == is_selected {
                        chunk.push(text_chars[char_idx]);
                        char_idx += 1;
                        current_pos += 1;
                    } else {
                        break;
                    }
                }

                // Create span with appropriate style
                let span_style = if is_selected {
                    style.bg(bg_color)
                } else {
                    *style
                };

                let span_style = self.style_with_defaults(span_style);
                result_spans.push(Span::styled(chunk, span_style));
            }

            char_pos = span_end;
        }

        result_spans
    }

    /// Convert a relative row position (visible row in window) to absolute line index in wrapped_lines buffer
    /// This accounts for scroll position to map visible coordinates to buffer coordinates
    pub fn relative_row_to_absolute_line(&self, rel_row: usize, visible_height: usize) -> usize {
        let total_lines = self.wrapped_lines.len();

        // Calculate start_line using same logic as render_with_focus
        let start_line = if let Some(pos) = self.scroll_position {
            // Scrolled back - use absolute position (frozen view)
            pos
        } else {
            // Live view mode - show the last visible_height lines
            let end = total_lines.saturating_sub(self.scroll_offset);
            end.saturating_sub(visible_height)
        };

        // Add relative row to start_line to get absolute line index
        start_line + rel_row
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

    /// Get the wrapped lines for text selection/extraction
    /// Returns a reference to the line segments
    pub fn get_lines(&self) -> Vec<LineSegments> {
        self.wrapped_lines
            .iter()
            .map(|line| LineSegments {
                segments: line
                    .spans
                    .iter()
                    .map(|(text, style, span_type, link)| {
                        let resolved = self.style_with_defaults(*style);
                        TextSegment {
                            text: text.clone(),
                            fg: resolved.fg,
                            bg: resolved.bg,
                            bold: resolved.add_modifier.contains(Modifier::BOLD),
                            span_type: *span_type,
                            link_data: link.clone(),
                        }
                    })
                    .collect(),
            })
            .collect()
    }

    /// Get the last N wrapped lines for saving to widget state
    /// Returns lines as Vec of Vec<TextSegment> (line segments)
    pub fn get_lines_for_save(&self, max: usize) -> Vec<Vec<TextSegment>> {
        let start_idx = self.wrapped_lines.len().saturating_sub(max);
        self.wrapped_lines
            .iter()
            .skip(start_idx)
            .map(|line| {
                line.spans
                    .iter()
                    .map(|(text, style, span_type, link)| {
                        let resolved = self.style_with_defaults(*style);
                        TextSegment {
                            text: text.clone(),
                            fg: resolved.fg,
                            bg: resolved.bg,
                            bold: resolved.add_modifier.contains(Modifier::BOLD),
                            span_type: *span_type,
                            link_data: link.clone(),
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// Find a link in the recent cache that matches the given word
    /// Returns the LinkData if found, otherwise None
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
                    tracing::debug!(
                        "Found multi-word text match: '{}' in text='{}' -> noun='{}' exist_id='{}'",
                        word,
                        link.text,
                        link.noun,
                        link.exist_id
                    );
                    return Some(link.clone());
                }
            }
        }

        // Second pass: exact noun match for single-word links
        for link in self.recent_links.iter().rev() {
            if link.noun.eq_ignore_ascii_case(word) {
                tracing::debug!(
                    "Found exact noun match: '{}' -> noun='{}' exist_id='{}' text='{}'",
                    word,
                    link.noun,
                    link.exist_id,
                    link.text
                );
                return Some(link.clone());
            }
        }

        tracing::debug!("No link found for word '{}'", word);
        None
    }

    /// Convert mouse position to text coordinates (line index, column index)
    /// Returns None if position is outside content area
    pub fn mouse_to_text_coords(
        &self,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: ratatui::layout::Rect,
    ) -> Option<(usize, usize)> {
        let border_offset = if self.has_border() { 1 } else { 0 };

        // Bounds check within content area
        if mouse_col < window_rect.x + border_offset
            || mouse_col >= window_rect.x + window_rect.width - border_offset
            || mouse_row < window_rect.y + border_offset
            || mouse_row >= window_rect.y + window_rect.height - border_offset
        {
            return None;
        }

        let visible_height = (window_rect.height.saturating_sub(2 * border_offset)) as usize;
        let total_lines = self.wrapped_lines.len();

        // Calculate start_line (same logic as render)
        let start_line = if let Some(pos) = self.scroll_position {
            pos
        } else {
            let end = total_lines.saturating_sub(self.scroll_offset);
            end.saturating_sub(visible_height)
        };

        let line_idx = start_line + (mouse_row - window_rect.y - border_offset) as usize;
        let col_offset = (mouse_col - window_rect.x - border_offset) as usize;

        // Line index might be beyond visible lines
        if line_idx >= total_lines {
            return None;
        }

        Some((line_idx, col_offset))
    }

    /// Extract text from a selection range
    /// Returns the selected text as a String
    pub fn extract_selection_text(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        let mut result = String::new();

        if start_line == end_line {
            // Single line selection
            if let Some(wrapped) = self.wrapped_lines.get(start_line) {
                let line_text: String = wrapped
                    .spans
                    .iter()
                    .map(|(text, _, _, _)| text.as_str())
                    .collect();
                let chars: Vec<char> = line_text.chars().collect();
                let actual_end = end_col.min(chars.len());
                if start_col < chars.len() {
                    result.push_str(&chars[start_col..actual_end].iter().collect::<String>());
                }
            }
        } else {
            // Multi-line selection
            for line_idx in start_line..=end_line.min(self.wrapped_lines.len().saturating_sub(1)) {
                if let Some(wrapped) = self.wrapped_lines.get(line_idx) {
                    let line_text: String = wrapped
                        .spans
                        .iter()
                        .map(|(text, _, _, _)| text.as_str())
                        .collect();
                    let chars: Vec<char> = line_text.chars().collect();

                    if line_idx == start_line {
                        // First line - from start_col to end
                        if start_col < chars.len() {
                            result.push_str(&chars[start_col..].iter().collect::<String>());
                        }
                    } else if line_idx == end_line {
                        // Last line - from beginning to end_col
                        let actual_end = end_col.min(chars.len());
                        result.push_str(&chars[..actual_end].iter().collect::<String>());
                    } else {
                        // Middle line - entire line
                        result.push_str(&line_text);
                    }

                    // Add newline between lines (but not after last line)
                    if line_idx < end_line {
                        result.push('\n');
                    }
                }
            }
        }

        result
    }

    /// Get visible line information for click detection
    /// Returns (start_line_index, visible_lines)
    pub fn get_visible_lines_info(&self, visible_height: usize) -> (usize, Vec<LineSegments>) {
        let total_lines = self.wrapped_lines.len();

        // Calculate which lines are visible based on scroll mode
        let (start_line, end_line) = if let Some(pos) = self.scroll_position {
            // Scrolled back - use absolute position
            let start = pos;
            let end = (pos + visible_height).min(total_lines);
            (start, end)
        } else {
            // Live view mode - show the last visible_height lines
            let end = total_lines.saturating_sub(self.scroll_offset);
            let start = end.saturating_sub(visible_height);
            (start, end)
        };

        // Collect visible lines
        let visible_lines: Vec<LineSegments> = (start_line..end_line)
            .filter_map(|idx| {
                self.wrapped_lines.get(idx).map(|line| LineSegments {
                    segments: line
                        .spans
                        .iter()
                        .map(|(text, style, span_type, link)| {
                            let resolved = self.style_with_defaults(*style);
                            TextSegment {
                                text: text.clone(),
                                fg: resolved.fg,
                                bg: resolved.bg,
                                bold: resolved.add_modifier.contains(Modifier::BOLD),
                                span_type: *span_type,
                                link_data: link.clone(),
                            }
                        })
                        .collect(),
                })
            })
            .collect();

        (start_line, visible_lines)
    }
}

/// A line of text with multiple styled segments (for text selection)
pub struct LineSegments {
    pub segments: Vec<TextSegment>,
}

/// A segment of styled text within a line
#[derive(Clone)]
pub struct TextSegment {
    pub text: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub span_type: SpanType,
    pub link_data: Option<LinkData>,
}

impl TextWindow {
    /// Render the window with optional focus indicator and selection highlighting
    pub fn render_with_focus(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        focused: bool,
        selection_state: Option<&crate::selection::SelectionState>,
        selection_bg_color: &str,
        window_index: usize,
        theme: &crate::theme::AppTheme,
    ) {
        // Clear the area to prevent bleed-through from windows behind
        Clear.render(area, buf);

        // Paint background before any content/borders so empty windows still show theme colors
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

        // Pre-compute border geometry so we can rewrap to the actual inner width
        let borders = crossterm_bridge::to_ratatui_borders(&self.border_sides);
        let border_type = match self.border_style.as_deref() {
            Some("double") => BorderType::Double,
            Some("rounded") => BorderType::Rounded,
            Some("thick") => BorderType::Thick,
            Some("quadrant_inside") => BorderType::QuadrantInside,
            Some("quadrant_outside") => BorderType::QuadrantOutside,
            _ => BorderType::Plain,
        };
        let inner_area = if self.show_border {
            Block::default()
                .borders(borders)
                .border_type(border_type)
                .inner(area)
        } else {
            area
        };

        // Update width based on inner area and re-wrap if needed
        self.set_width(inner_area.width);
        if self.needs_rewrap {
            self.rewrap_all();
        }

        let total_lines = self.wrapped_lines.len();
        let visible_height = inner_area.height as usize;
        self.last_visible_height = visible_height; // Save for scroll calculations

        let title = if let Some(pos) = self.scroll_position {
            let lines_from_end = total_lines.saturating_sub(pos);
            format!("{} [{}]", self.title, lines_from_end)
        } else if self.scroll_offset > 0 {
            format!("{} [{}]", self.title, self.scroll_offset)
        } else {
            self.title.clone()
        };

        let mut border_style = Style::default();
        if let Some(ref color_hex) = self.border_color {
            if let Some(color) = Self::parse_hex_color(color_hex) {
                border_style = border_style.fg(color);
            }
        }

        if focused {
            border_style = border_style
                .fg(crossterm_bridge::to_ratatui_color(theme.window_border_focused))
                .add_modifier(Modifier::BOLD);
        }

        // Draw border/title for the current window state
        let inner_area = title_position::render_block_with_title(
            area,
            buf,
            self.show_border,
            borders,
            &self.border_sides,
            border_type,
            border_style,
            &title,
            self.title_position,
        );

        if total_lines == 0 {
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

        // Parse selection background color
        let selection_bg = Self::parse_hex_color(selection_bg_color);

        // Collect lines from buffer (oldest to newest order)
        let mut display_lines: Vec<Line> = Vec::new();
        for idx in start_line..end_line {
            if let Some(wrapped) = self.wrapped_lines.get(idx) {
                // Check if this line has search matches
                let line_matches: Vec<&SearchMatch> = self
                    .search_state
                    .as_ref()
                    .map(|state| state.matches.iter().filter(|m| m.line_idx == idx).collect())
                    .unwrap_or_default();

                let current_match = self
                    .search_state
                    .as_ref()
                    .and_then(|state| state.matches.get(state.current_match_idx));

                let spans: Vec<Span> = if line_matches.is_empty() {
                    // No search matches - check for selection
                    self.create_spans_with_selection(
                        wrapped,
                        idx,
                        selection_state,
                        selection_bg,
                        window_index,
                    )
                } else {
                    // Has matches - need to highlight them
                    self.create_highlighted_spans(wrapped, &line_matches, current_match)
                };

                display_lines.push(Line::from(spans));
            }
        }

        // Lines are already in the correct order (oldest at top, newest at bottom)
        // No need to reverse!

        // Calculate content alignment offset if specified
        // Only apply centering when content is LESS than window height
        // Once content fills the window, behave normally (top-aligned scrolling)
        let row_offset = if let Some(ref align_str) = self.content_align {
            let content_height = display_lines.len() as u16;
            // Only apply alignment if content is shorter than window
            if content_height < inner_area.height {
                let align = crate::config::ContentAlign::from_str(align_str);
                let (offset, _) = align.calculate_offset(
                    inner_area.width,
                    content_height,
                    inner_area.width,
                    inner_area.height,
                );
                offset
            } else {
                // Content fills or exceeds window - use default top alignment
                0
            }
        } else {
            0
        };

        // Apply row offset by padding top with empty lines
        let mut padded_lines = display_lines;
        if row_offset > 0 {
            let empty_lines: Vec<Line> = (0..row_offset).map(|_| Line::from("")).collect();
            padded_lines.splice(0..0, empty_lines);
        }

        // Render content inside the inner area (border already drawn above)
        let paragraph = Paragraph::new(padded_lines);
        paragraph.render(inner_area, buf);
    }

    fn fallback_text_color(&self) -> Color {
        self.default_text_color.unwrap_or(Color::Gray)
    }

    fn style_with_defaults(&self, mut style: Style) -> Style {
        if matches!(style.fg, None | Some(Color::Reset)) {
            style = style.fg(self.fallback_text_color());
        }

        if matches!(style.bg, None | Some(Color::Reset)) {
            if let Some(bg) = self.background_color {
                style = style.bg(bg);
            }
        }

        style
    }

    fn parse_hex_color(hex: &str) -> Option<Color> {
        // Use centralized mode-aware color parser
        super::colors::parse_color_to_ratatui(hex)
    }

    /// Clear all text from the buffer
    pub fn clear(&mut self) {
        self.logical_lines.clear();
        self.current_line_spans.clear();
        self.scroll_offset = 0;
        self.wrapped_lines.clear();
    }
}

impl Widget for &mut TextWindow {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // No selection highlighting for basic Widget trait render
        // Use default dark theme (this trait doesn't allow passing theme)
        let theme = crate::theme::ThemePresets::dark();
        self.render_with_focus(area, buf, false, None, "#4a4a4a", 0, &theme);
    }
}
