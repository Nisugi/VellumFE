//! Window dedicated to showing spell listings and clickable spell links.
//!
//! This widget behaves similarly to the inventory window but retains a separate
//! link cache tailored to `<spell>` stream updates.

use crate::data::{LinkData, SpanType, TextSegment};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget as RatatuiWidget},
};
use std::collections::VecDeque;

/// Spells window widget - displays known spells with clickable links
/// Content is completely replaced on each update (no buffer, no scrolling history)
pub struct SpellsWindow {
    title: String,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<Color>,
    text_color: Option<Color>,
    background_color: Option<Color>,
    transparent_background: bool,

    /// Current spell content (list of styled lines)
    lines: Vec<Vec<TextSegment>>,

    /// Current line being built (before finish_line is called)
    current_line: Vec<TextSegment>,

    /// Scroll offset
    scroll_offset: usize,

    /// Window dimensions (updated during layout)
    inner_width: usize,
    inner_height: usize,

    /// Highlight engine for pattern matching and styling
    highlight_engine: super::highlight_utils::HighlightEngine,

    /// Recent links cache for click detection
    recent_links: VecDeque<LinkData>,
    max_recent_links: usize,
}

impl SpellsWindow {
    pub fn new(title: String) -> Self {
        Self {
            title,
            show_border: true,
            border_style: None,
            border_color: None,
            text_color: None,
            background_color: None,
            transparent_background: false,
            lines: Vec::new(),
            current_line: Vec::new(),
            scroll_offset: 0,
            inner_width: 80,
            inner_height: 20,
            highlight_engine: super::highlight_utils::HighlightEngine::new(Vec::new()),
            recent_links: VecDeque::new(),
            max_recent_links: 100,
        }
    }

    /// Set highlight patterns for this window (only recompiles if changed)
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.highlight_engine.update_if_changed(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.highlight_engine.set_replace_enabled(enabled);
    }

    /// Clear all content (called when clearStream is received)
    pub fn clear(&mut self) {
        self.lines.clear();
        self.current_line.clear();
        self.scroll_offset = 0;
        // Keep link cache - links rarely change
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
            // Apply highlights before adding to buffer
            let highlighted = self
                .highlight_engine
                .apply_highlights_to_segments(&line, "spell")
                .unwrap_or(line);
            self.lines.push(highlighted);
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

        // Third pass: word appears in single-word link text
        for link in self.recent_links.iter().rev() {
            let link_text_lower = link.text.to_lowercase();
            let word_lower = word.to_lowercase();

            if link_text_lower.split_whitespace().count() == 1
                && link_text_lower.split_whitespace().any(|w| w == word_lower) {
                    tracing::debug!(
                        "Found single-word text match: '{}' -> text='{}' noun='{}' exist_id='{}'",
                        word,
                        link.text,
                        link.noun,
                        link.exist_id
                    );
                    return Some(link.clone());
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

    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color.and_then(|c| Self::parse_color(&c));
    }

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

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    /// Parse a hex color string to ratatui Color
    fn parse_color(hex: &str) -> Option<Color> {
        // Use centralized mode-aware color parser
        super::colors::parse_color_to_ratatui(hex)
    }

    /// Render the spells window
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear the area to prevent bleed-through from windows behind
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

        // Create border block
        let mut block = Block::default();

        if self.show_border {
            let border_color = self.border_color.unwrap_or(Color::White);
            block = block
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color));

            // Apply border type
            if let Some(ref style) = self.border_style {
                use ratatui::widgets::BorderType;
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

        // Calculate visible range
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

    fn apply_style(&self, segment: &TextSegment) -> Style {
        let mut style = Style::default();

        if let Some(ref fg) = segment.fg {
            if let Some(color) = Self::parse_color(fg) {
                style = style.fg(color);
            }
        } else if let Some(default_fg) = self.text_color {
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

    pub fn render_themed(&self, area: Rect, buf: &mut Buffer, _theme: &crate::theme::AppTheme) {
        // For now, just call regular render - theme colors will be applied in future update
        self.render(area, buf);
    }
}

