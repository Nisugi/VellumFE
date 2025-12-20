//! Room window implementation with per-component visibility toggles.
//!
//! Buffers each component separately so that toggling desc/objs/players/exits
//! only requires rewrapping, not reparsing, and maintains a dedicated scrollback.

use crate::frontend::tui::colors::parse_color_to_ratatui;
use crate::frontend::tui::crossterm_bridge;
use crate::{
    config,
    data::widget::{SpanType, TextSegment},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph},
};
use std::collections::HashMap;

/// Room window widget - displays room information with component buffering
/// Components: room desc, room objs, room players, room exits
pub struct RoomWindow {
    title: String,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<Color>,
    border_sides: config::BorderSides,
    background_color: Option<Color>,
    default_text_color: Option<Color>,
    title_fg_hex: Option<String>,
    title_bg_hex: Option<String>,
    show_inline_title: bool,

    /// Component buffers (id -> styled lines)
    /// Components: "room desc", "room objs", "room players", "room exits"
    components: HashMap<String, Vec<Vec<TextSegment>>>,

    /// Component visibility toggles (default: all enabled)
    /// Allows users to show/hide specific room components
    component_visibility: HashMap<String, bool>,

    /// Current line being built for a component
    current_component_id: Option<String>,
    current_line: Vec<TextSegment>,

    /// Cached wrapped lines for rendering
    wrapped_lines: Vec<Vec<TextSegment>>,
    needs_rewrap: bool,

    /// Scroll offset
    scroll_offset: usize,

    /// Window dimensions (updated during layout)
    inner_width: usize,
    inner_height: usize,

    /// Highlight engine for pattern matching and styling
    highlight_engine: super::highlight_utils::HighlightEngine,
}

impl RoomWindow {
    pub fn new(title: String) -> Self {
        // Initialize with all components visible by default
        let mut component_visibility = HashMap::new();
        component_visibility.insert("room desc".to_string(), true);
        component_visibility.insert("room objs".to_string(), true);
        component_visibility.insert("room players".to_string(), true);
        component_visibility.insert("room exits".to_string(), true);

        Self {
            title,
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: config::BorderSides::default(),
            background_color: None,
            default_text_color: None,
            title_fg_hex: None,
            title_bg_hex: None,
            show_inline_title: false,
            components: HashMap::new(),
            component_visibility,
            current_component_id: None,
            current_line: Vec::new(),
            wrapped_lines: Vec::new(),
            needs_rewrap: true,
            scroll_offset: 0,
            inner_width: 80,
            inner_height: 20,
            highlight_engine: super::highlight_utils::HighlightEngine::new(Vec::new()),
        }
    }

    /// Set highlight patterns for this window
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.highlight_engine = super::highlight_utils::HighlightEngine::new(highlights);
        self.needs_rewrap = true; // Re-process content with new highlights
    }

    /// Clear all component buffers (called when room stream is pushed)
    pub fn clear_all_components(&mut self) {
        self.components.clear();
        self.current_component_id = None;
        self.current_line.clear();
        self.scroll_offset = 0;
        self.needs_rewrap = true;
    }

    /// Start building a new component
    pub fn start_component(&mut self, id: String) {
        // Finish any pending component first
        if self.current_component_id.is_some() {
            self.finish_component();
        }

        self.current_component_id = Some(id.clone());
        self.current_line.clear();

        // Initialize component buffer if it doesn't exist, or clear it
        self.components.entry(id).or_default().clear();
    }

    /// Add styled text segment to current component's current line
    pub fn add_segment(&mut self, segment: TextSegment) {
        if segment.text.is_empty() {
            return;
        }
        self.current_line.push(segment);
    }

    /// Finish current line and add to current component buffer
    pub fn finish_line(&mut self) {
        if let Some(ref component_id) = self.current_component_id {
            if !self.current_line.is_empty() {
                let line = std::mem::take(&mut self.current_line);
                if let Some(buffer) = self.components.get_mut(component_id) {
                    buffer.push(line);
                }
            }
        }
    }

    /// Finish building current component
    pub fn finish_component(&mut self) {
        // Finish any pending line
        if !self.current_line.is_empty() {
            self.finish_line();
        }
        self.current_component_id = None;
        self.needs_rewrap = true;
    }

    /// Wrap a line of styled segments to window width (word-aware wrapping)
    /// Matches VellumFE's character-by-character processing to properly split spans
    fn wrap_line(&self, segments: Vec<TextSegment>) -> Vec<Vec<TextSegment>> {
        if self.inner_width == 0 {
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

        if new_width != self.inner_width || new_height != self.inner_height {
            self.needs_rewrap = true;
        }
        self.inner_width = new_width;
        self.inner_height = new_height;
    }

    /// Scroll up by N lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        let total_lines = self.wrapped_lines.len();
        let max_scroll = total_lines.saturating_sub(self.inner_height);
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

    /// Get all lines combined for wrapping
    /// Layout: desc+objs on one line, players on next line, exits on next line
    fn get_combined_lines(&self) -> Vec<Vec<TextSegment>> {
        let mut all_lines: Vec<Vec<TextSegment>> = Vec::new();

        if self.show_inline_title && !self.title.is_empty() {
            let title_segment = vec![TextSegment {
                text: self.title.clone(),
                fg: self.title_fg_hex.clone(),
                bg: self.title_bg_hex.clone(),
                bold: true,
                span_type: SpanType::Normal,
                link_data: None,
            }];
            // Apply highlights to title
            let highlighted = self
                .highlight_engine
                .apply_highlights_to_segments(&title_segment, "room")
                .unwrap_or(title_segment);
            all_lines.push(highlighted);
        }

        // Combine desc + objs on same line (only if visible)
        let mut desc_and_objs_line = Vec::new();

        // Add room desc segments (if visible)
        if self.is_component_visible("room desc") {
            if let Some(desc_lines) = self.components.get("room desc") {
                for line in desc_lines {
                    desc_and_objs_line.extend(line.clone());
                }
            }
        }

        // Append room objs segments to same line (if visible)
        if self.is_component_visible("room objs") {
            if let Some(objs_lines) = self.components.get("room objs") {
                for line in objs_lines {
                    desc_and_objs_line.extend(line.clone());
                }
            }
        }

        // Only add the combined line if it's not empty
        if !desc_and_objs_line.is_empty() {
            // Apply highlights to desc+objs line
            let highlighted = self
                .highlight_engine
                .apply_highlights_to_segments(&desc_and_objs_line, "room")
                .unwrap_or(desc_and_objs_line);
            all_lines.push(highlighted);
        }

        // Add room players on own line (skip if empty or not visible)
        if self.is_component_visible("room players") {
            if let Some(players_lines) = self.components.get("room players") {
                if !players_lines.is_empty() && !players_lines.iter().all(|line| line.is_empty()) {
                    for line in players_lines {
                        if !line.is_empty() {
                            // Apply highlights to players line
                            let highlighted = self
                                .highlight_engine
                                .apply_highlights_to_segments(line, "room")
                                .unwrap_or_else(|| line.clone());
                            all_lines.push(highlighted);
                        }
                    }
                }
            }
        }

        // Add room exits on own line (skip if empty or not visible)
        if self.is_component_visible("room exits") {
            if let Some(exits_lines) = self.components.get("room exits") {
                if !exits_lines.is_empty() && !exits_lines.iter().all(|line| line.is_empty()) {
                    for line in exits_lines {
                        if !line.is_empty() {
                            // Apply highlights to exits line
                            let highlighted = self
                                .highlight_engine
                                .apply_highlights_to_segments(line, "room")
                                .unwrap_or_else(|| line.clone());
                            all_lines.push(highlighted);
                        }
                    }
                }
            }
        }

        all_lines
    }

    /// Rewrap all combined lines and cache them
    fn rewrap_all(&mut self) {
        self.wrapped_lines.clear();

        let combined = self.get_combined_lines();
        for line in combined {
            let wrapped = self.wrap_line(line);
            self.wrapped_lines.extend(wrapped);
        }

        self.needs_rewrap = false;
    }

    /// Set title
    pub fn set_title(&mut self, title: String) {
        self.title = title;
        self.needs_rewrap = true;
    }

    pub fn set_title_colors(&mut self, fg_hex: Option<String>, bg_hex: Option<String>) {
        self.title_fg_hex = fg_hex;
        self.title_bg_hex = bg_hex;
        self.needs_rewrap = true;
    }

    /// Toggle inline room name rendering
    pub fn set_show_name(&mut self, show: bool) {
        if self.show_inline_title != show {
            self.show_inline_title = show;
            self.needs_rewrap = true;
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
        self.border_color = Self::parse_color_setting(border_color);
    }

    pub fn set_border_sides(&mut self, sides: config::BorderSides) {
        self.border_sides = sides;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = Self::parse_color_setting(color);
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.default_text_color = Self::parse_color_setting(color);
    }

    /// Check if a component is visible
    pub fn is_component_visible(&self, component_id: &str) -> bool {
        self.component_visibility
            .get(component_id)
            .copied()
            .unwrap_or(true)
    }

    /// Set component visibility
    /// Components: "room desc", "room objs", "room players", "room exits"
    pub fn set_component_visible(&mut self, component_id: &str, visible: bool) {
        let entry = self
            .component_visibility
            .entry(component_id.to_string())
            .or_insert(true);
        if *entry != visible {
            *entry = visible;
            self.needs_rewrap = true; // Trigger rewrap since content changed
        }
    }

    /// Get all component visibility states
    pub fn get_component_visibility(&self) -> HashMap<String, bool> {
        self.component_visibility.clone()
    }

    /// Set all component visibility states at once
    pub fn set_all_component_visibility(&mut self, visibility: HashMap<String, bool>) {
        self.component_visibility = visibility;
        self.needs_rewrap = true;
    }

    /// Get the wrapped lines for click detection and other operations
    pub fn get_wrapped_lines(&self) -> &Vec<Vec<TextSegment>> {
        &self.wrapped_lines
    }

    /// Get the start line offset (which line is shown at the top of the visible area)
    /// This is needed for click detection to map visual rows to actual wrapped line indices
    pub fn get_start_line(&self) -> usize {
        let total_lines = self.wrapped_lines.len();
        if total_lines > self.inner_height {
            total_lines
                .saturating_sub(self.inner_height)
                .saturating_sub(self.scroll_offset)
        } else {
            0
        }
    }

    /// Render the room window
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

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

        // Update inner size
        self.update_inner_size(area.width, area.height);

        // Rewrap if needed
        if self.needs_rewrap {
            self.rewrap_all();
        }

        // Create border block
        let mut block = if self.show_border {
            let borders = crossterm_bridge::to_ratatui_borders(&self.border_sides);

            // Use styled title with roomName preset if provided
            let title_line = if self.title_fg_hex.is_some() || self.title_bg_hex.is_some() {
                let mut style = Style::default();
                if let Some(ref fg) = self.title_fg_hex {
                    if let Some(color) = parse_color_to_ratatui(fg) {
                        style = style.fg(color);
                    }
                }
                if let Some(ref bg) = self.title_bg_hex {
                    if let Some(color) = parse_color_to_ratatui(bg) {
                        style = style.bg(color);
                    }
                }
                Line::from(Span::styled(self.title.clone(), style.add_modifier(Modifier::BOLD)))
            } else {
                Line::from(self.title.as_str())
            };

            Block::default().title(title_line).borders(borders)
        } else {
            Block::default()
        };

        if self.show_border {
            if let Some(ref style_name) = self.border_style {
                let border_type = match style_name.as_str() {
                    "double" => BorderType::Double,
                    "rounded" => BorderType::Rounded,
                    "thick" => BorderType::Thick,
                    "quadrant_inside" => BorderType::QuadrantInside,
                    "quadrant_outside" => BorderType::QuadrantOutside,
                    _ => BorderType::Plain,
                };
                block = block.border_type(border_type);
            }

            let border_color = self.border_color.unwrap_or(Color::White);
            block = block.border_style(Style::default().fg(border_color));
        }

        // Calculate visible range
        let total_lines = self.wrapped_lines.len();
        let start_line = if total_lines > self.inner_height {
            total_lines
                .saturating_sub(self.inner_height)
                .saturating_sub(self.scroll_offset)
        } else {
            0
        };
        let end_line = start_line + self.inner_height.min(total_lines);

        // Get visible lines
        let visible_lines: Vec<Line> = self.wrapped_lines[start_line..end_line.min(total_lines)]
            .iter()
            .map(|segments| {
                let spans: Vec<Span> = segments
                    .iter()
                    .map(|seg| Span::styled(seg.text.clone(), self.segment_style(seg)))
                    .collect();
                Line::from(spans)
            })
            .collect();

        let paragraph = Paragraph::new(visible_lines).block(block);
        use ratatui::widgets::Widget;
        paragraph.render(area, buf);
    }

    fn segment_style(&self, segment: &TextSegment) -> Style {
        let mut style = Style::default();

        if let Some(ref fg) = segment.fg {
            if let Some(color) = parse_color_to_ratatui(fg) {
                style = style.fg(color);
            }
        } else if let Some(default_fg) = self.default_text_color {
            style = style.fg(default_fg);
        }

        if let Some(ref bg) = segment.bg {
            if let Some(color) = parse_color_to_ratatui(bg) {
                style = style.bg(color);
            }
        } else if let Some(bg_color) = self.background_color {
            style = style.bg(bg_color);
        }

        if segment.bold {
            style = style.add_modifier(Modifier::BOLD);
        }

        style
    }

    pub fn render_themed(&mut self, area: Rect, buf: &mut Buffer, _theme: &crate::theme::AppTheme) {
        self.render(area, buf);
    }

    fn parse_color_setting(color: Option<String>) -> Option<Color> {
        color.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed == "-" {
                None
            } else {
                parse_color_to_ratatui(trimmed)
            }
        })
    }
}
