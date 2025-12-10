use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget as RatatuiWidget},
    buffer::Buffer,
};
use crate::ui::{TextSegment, SpanType, LinkData};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderStyleType {
    Single,
    Double,
    Rounded,
    Thick,
    None,
}

/// Spells window widget - displays known spells with clickable links
/// Content is completely replaced on each update (no scrolling, no buffer)
pub struct SpellsWindow {
    title: String,
    show_border: bool,
    border_style: BorderStyleType,
    border_color: Option<String>,

    /// Current spell content (list of styled lines)
    lines: Vec<Vec<TextSegment>>,

    /// Current line being built (before finish_line is called)
    current_line: Vec<TextSegment>,

    /// Scroll offset
    scroll_offset: usize,

    /// Window dimensions (updated during layout)
    inner_width: usize,
    inner_height: usize,

    /// Recent links cache for click detection
    recent_links: VecDeque<LinkData>,
    max_recent_links: usize,
}

impl SpellsWindow {
    pub fn new(title: String) -> Self {
        Self {
            title,
            show_border: true,
            border_style: BorderStyleType::Single,
            border_color: None,
            lines: Vec::new(),
            current_line: Vec::new(),
            scroll_offset: 0,
            inner_width: 80,
            inner_height: 20,
            recent_links: VecDeque::new(),
            max_recent_links: 100,
        }
    }

    /// Clear all content (called when Spells stream is pushed)
    pub fn clear(&mut self) {
        self.lines.clear();
        self.current_line.clear();
        self.scroll_offset = 0;
    }

    /// Add styled text to current line
    pub fn add_text(&mut self, text: String, fg: Option<Color>, bg: Option<Color>, bold: bool, span_type: SpanType, link_data: Option<LinkData>) {
        if text.is_empty() {
            return;
        }

        // Cache link data if present, accumulating text for the same exist_id
        if let Some(ref link_data_ref) = link_data {
            // Check if we already have this exist_id in the most recent entry
            let should_append = if let Some(last) = self.recent_links.back_mut() {
                if last.exist_id == link_data_ref.exist_id {
                    // Append to existing text
                    last.text.push_str(&text);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if !should_append {
                // New link - create new entry with this content as the text
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

    /// Finish current line and add to buffer (no wrapping - spells content is pre-formatted)
    pub fn finish_line(&mut self) {
        let line = std::mem::take(&mut self.current_line);
        if line.is_empty() {
            // Add empty line as-is (preserves spacing in spell list)
            self.lines.push(Vec::new());
        } else {
            self.lines.push(line);
        }
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
                    tracing::debug!("Found multi-word text match: '{}' in text='{}' -> noun='{}' exist_id='{}'",
                        word, link.text, link.noun, link.exist_id);
                    return Some(link.clone());
                }
            }
        }

        // Second pass: exact noun match for single-word links
        for link in self.recent_links.iter().rev() {
            if link.noun.eq_ignore_ascii_case(word) {
                tracing::debug!("Found exact noun match: '{}' -> noun='{}' exist_id='{}' text='{}'",
                    word, link.noun, link.exist_id, link.text);
                return Some(link.clone());
            }
        }

        // Third pass: word appears in single-word link text
        for link in self.recent_links.iter().rev() {
            let link_text_lower = link.text.to_lowercase();
            let word_lower = word.to_lowercase();

            if link_text_lower.split_whitespace().count() == 1 {
                if link_text_lower.split_whitespace().any(|w| w == word_lower) {
                    tracing::debug!("Found single-word text match: '{}' -> text='{}' noun='{}' exist_id='{}'",
                        word, link.text, link.noun, link.exist_id);
                    return Some(link.clone());
                }
            }
        }

        // No match found
        None
    }

    /// Update inner dimensions based on window size
    pub fn update_inner_size(&mut self, width: u16, height: u16) {
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

    /// Get all lines (for text selection)
    pub fn get_lines(&self) -> &[Vec<TextSegment>] {
        &self.lines
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
    pub fn set_border_color(&mut self, color: Option<String>) {
        self.border_color = color;
    }

    /// Parse a hex color string to ratatui Color
    fn parse_color(hex: &str) -> Color {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return Color::White;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);

        Color::Rgb(r, g, b)
    }

    /// Set title
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    /// Render the spells window
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear the area to prevent bleed-through from windows behind
        Clear.render(area, buf);

        // Create border block
        let mut block = Block::default();

        if self.show_border {
            let border_color = self.border_color.as_ref()
                .map(|c| Self::parse_color(c))
                .unwrap_or(Color::White);
            block = block.borders(Borders::ALL).border_style(
                Style::default().fg(border_color)
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

        // Calculate visible range
        let total_lines = self.lines.len();
        let visible_start = total_lines.saturating_sub(self.scroll_offset + inner.height as usize);
        let visible_end = total_lines.saturating_sub(self.scroll_offset);

        // Build visible lines
        let mut display_lines = Vec::new();
        for line in self.lines[visible_start..visible_end].iter() {
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

        let paragraph = Paragraph::new(display_lines)
            .block(block);

        ratatui::widgets::Widget::render(paragraph, area, buf);
    }
}
