use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    buffer::Buffer,
};
use std::collections::HashMap;
use crate::ui::{TextSegment, SpanType, LinkData};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderStyleType {
    Single,
    Double,
    Rounded,
    Thick,
    None,
}

/// Room window widget - displays room information with component buffering
/// Components: room desc, room objs, room players, room exits, sprite
pub struct RoomWindow {
    title: String,
    show_border: bool,
    border_style: BorderStyleType,
    border_color: Option<Color>,

    /// Component buffers (id -> styled lines)
    /// Components: "room desc", "room objs", "room players", "room exits", "sprite"
    components: HashMap<String, Vec<Vec<TextSegment>>>,

    /// Current line being built for a component
    current_component_id: Option<String>,
    current_line: Vec<TextSegment>,

    /// Cached wrapped lines for rendering and click detection
    wrapped_lines: Vec<Vec<TextSegment>>,
    needs_rewrap: bool,

    /// Scroll offset
    scroll_offset: usize,

    /// Window dimensions (updated during layout)
    inner_width: usize,
    inner_height: usize,
}

impl RoomWindow {
    pub fn new(title: String) -> Self {
        Self {
            title,
            show_border: true,
            border_style: BorderStyleType::Single,
            border_color: None,
            components: HashMap::new(),
            current_component_id: None,
            current_line: Vec::new(),
            wrapped_lines: Vec::new(),
            needs_rewrap: true,
            scroll_offset: 0,
            inner_width: 80,
            inner_height: 20,
        }
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

        // Initialize component buffer if it doesn't exist
        self.components.entry(id).or_insert_with(Vec::new).clear();
    }

    /// Add styled text to current component's current line
    pub fn add_text(&mut self, styled: crate::ui::StyledText) {
        if styled.content.is_empty() {
            return;
        }

        self.current_line.push(TextSegment {
            text: styled.content,
            fg: styled.fg,
            bg: styled.bg,
            bold: styled.bold,
            span_type: styled.span_type,
            link_data: styled.link_data,
        });
    }

    /// Finish current line and add to current component buffer
    /// Note: We don't wrap here - let Ratatui's Paragraph widget handle wrapping
    pub fn finish_line(&mut self) {
        if let Some(ref component_id) = self.current_component_id {
            let line = std::mem::take(&mut self.current_line);

            if let Some(buffer) = self.components.get_mut(component_id) {
                buffer.push(line);
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

    /// Wrap a line of styled segments to window width
    fn wrap_line(&self, segments: Vec<TextSegment>) -> Vec<Vec<TextSegment>> {
        let mut wrapped_lines = Vec::new();
        let mut current_line = Vec::new();
        let mut current_width = 0;

        for segment in segments {
            let chars: Vec<char> = segment.text.chars().collect();
            let mut char_idx = 0;

            while char_idx < chars.len() {
                let remaining = self.inner_width.saturating_sub(current_width);

                if remaining == 0 {
                    // Line is full, wrap
                    wrapped_lines.push(std::mem::take(&mut current_line));
                    current_width = 0;
                }

                let chars_to_take = remaining.min(chars.len() - char_idx);
                if chars_to_take > 0 {
                    let text: String = chars[char_idx..char_idx + chars_to_take].iter().collect();
                    current_line.push(TextSegment {
                        text,
                        fg: segment.fg,
                        bg: segment.bg,
                        bold: segment.bold,
                        span_type: segment.span_type,
                        link_data: segment.link_data.clone(),
                    });
                    current_width += chars_to_take;
                    char_idx += chars_to_take;
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
        let total_lines = self.get_total_lines();
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

    /// Get total line count across all components
    fn get_total_lines(&self) -> usize {
        let mut total = 0;

        // Display in order: desc, objs, players, exits (skip sprite)
        for comp_id in &["room desc", "room objs", "room players", "room exits"] {
            if let Some(lines) = self.components.get(*comp_id) {
                total += lines.len();
            }
        }

        total
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

    /// Get wrapped lines for click detection
    pub fn get_wrapped_lines(&self) -> &Vec<Vec<TextSegment>> {
        &self.wrapped_lines
    }

    /// Get all lines combined (for text selection)
    /// Layout: desc+objs on one line, players on next line, exits on next line
    pub fn get_combined_lines(&self) -> Vec<Vec<TextSegment>> {
        let mut all_lines: Vec<Vec<TextSegment>> = Vec::new();

        // Combine desc + objs on same line
        let mut desc_and_objs_line = Vec::new();

        // Add room desc segments
        if let Some(desc_lines) = self.components.get("room desc") {
            for line in desc_lines {
                for segment in line {
                    desc_and_objs_line.push(TextSegment {
                        text: segment.text.clone(),
                        fg: segment.fg,
                        bg: segment.bg,
                        bold: segment.bold,
                        span_type: segment.span_type,
                        link_data: segment.link_data.clone(),
                    });
                }
            }
        }

        // Append room objs segments to same line
        if let Some(objs_lines) = self.components.get("room objs") {
            for line in objs_lines {
                for segment in line {
                    desc_and_objs_line.push(TextSegment {
                        text: segment.text.clone(),
                        fg: segment.fg,
                        bg: segment.bg,
                        bold: segment.bold,
                        span_type: segment.span_type,
                        link_data: segment.link_data.clone(),
                    });
                }
            }
        }

        // Only add the combined line if it's not empty
        if !desc_and_objs_line.is_empty() {
            all_lines.push(desc_and_objs_line);
        }

        // Add room players on own line (skip if empty)
        if let Some(players_lines) = self.components.get("room players") {
            if !players_lines.is_empty() && !players_lines.iter().all(|line| line.is_empty()) {
                for line in players_lines {
                    let mut new_line = Vec::new();
                    for segment in line {
                        new_line.push(TextSegment {
                            text: segment.text.clone(),
                            fg: segment.fg,
                            bg: segment.bg,
                            bold: segment.bold,
                            span_type: segment.span_type,
                            link_data: segment.link_data.clone(),
                        });
                    }
                    all_lines.push(new_line);
                }
            }
        }

        // Add room exits on own line
        if let Some(exits_lines) = self.components.get("room exits") {
            for line in exits_lines {
                let mut new_line = Vec::new();
                for segment in line {
                    new_line.push(TextSegment {
                        text: segment.text.clone(),
                        fg: segment.fg,
                        bg: segment.bg,
                        bold: segment.bold,
                        span_type: segment.span_type,
                        link_data: segment.link_data.clone(),
                    });
                }
                all_lines.push(new_line);
            }
        }

        all_lines
    }

    /// Set border visibility
    pub fn set_show_border(&mut self, show: bool) {
        self.show_border = show;
    }

    /// Set border style
    pub fn set_border_style(&mut self, style: BorderStyleType) {
        self.border_style = style;
    }

    /// Set border color
    pub fn set_border_color(&mut self, color: Option<Color>) {
        self.border_color = color;
    }

    /// Set title
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    /// Render the room window
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        // Rewrap if needed
        if self.needs_rewrap {
            self.rewrap_all();
        }

        // Create border block
        let mut block = Block::default();

        if self.show_border {
            block = block.borders(Borders::ALL).border_style(
                Style::default().fg(self.border_color.unwrap_or(Color::White))
            );

            // Apply border type
            block = match self.border_style {
                BorderStyleType::Single => block.border_type(ratatui::widgets::BorderType::Plain),
                BorderStyleType::Double => block.border_type(ratatui::widgets::BorderType::Double),
                BorderStyleType::Rounded => block.border_type(ratatui::widgets::BorderType::Rounded),
                BorderStyleType::Thick => block.border_type(ratatui::widgets::BorderType::Thick),
                BorderStyleType::None => block.borders(Borders::NONE),
            };

            if !self.title.is_empty() {
                block = block.title(self.title.clone());
            }
        }

        let inner = block.inner(area);

        // Use pre-wrapped lines
        let total_lines = self.wrapped_lines.len();

        // Calculate visible range
        let visible_start = total_lines.saturating_sub(self.scroll_offset + inner.height as usize);
        let visible_end = total_lines.saturating_sub(self.scroll_offset);

        // Build visible lines from wrapped_lines
        let mut display_lines = Vec::new();
        for line in self.wrapped_lines[visible_start..visible_end].iter() {
            let mut spans = Vec::new();
            for segment in line {
                let mut style = Style::default();
                if let Some(fg) = segment.fg {
                    style = style.fg(fg);
                }
                if let Some(bg) = segment.bg {
                    style = style.bg(bg);
                }
                if segment.bold {
                    style = style.add_modifier(ratatui::style::Modifier::BOLD);
                }
                spans.push(Span::styled(segment.text.clone(), style));
            }
            display_lines.push(Line::from(spans));
        }

        // Don't use Paragraph wrapping - we already wrapped
        let paragraph = Paragraph::new(display_lines)
            .block(block);

        ratatui::widgets::Widget::render(paragraph, area, buf);
    }
}
