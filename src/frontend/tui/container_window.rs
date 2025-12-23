//! Container window widget for displaying contents of bags, backpacks, etc.
//!
//! Similar to inventory_window but displays contents from a specific container
//! tracked in GameState.container_cache. Each container window is configured
//! with a container_id that links it to a specific container in the cache.

use crate::core::state::ContainerData;
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

/// Container window widget - displays items in a specific container
pub struct ContainerWindow {
    /// Container title pattern this window displays (matched case-insensitively)
    container_title: String,
    /// Display title (falls back to container title from cache)
    title: String,
    show_border: bool,
    border_color: Option<Color>,
    text_color: Option<Color>,
    background_color: Option<Color>,
    transparent_background: bool,

    /// Current container content (parsed from raw lines)
    lines: Vec<Vec<TextSegment>>,

    /// Scroll offset for navigation
    scroll_offset: usize,

    /// Window dimensions (updated during layout)
    inner_width: usize,
    inner_height: usize,

    /// Generation counter for change detection (matches ContainerData.generation)
    last_generation: u64,

    /// Highlight engine for pattern matching and styling
    highlight_engine: super::highlight_utils::HighlightEngine,

    /// Recent links cache for click detection
    recent_links: VecDeque<crate::data::LinkData>,
    max_recent_links: usize,
}

impl ContainerWindow {
    pub fn new(container_title: String, title: String) -> Self {
        Self {
            container_title,
            title,
            show_border: true,
            border_color: None,
            text_color: None,
            background_color: None,
            transparent_background: false,
            lines: Vec::new(),
            scroll_offset: 0,
            inner_width: 80,
            inner_height: 20,
            last_generation: 0,
            highlight_engine: super::highlight_utils::HighlightEngine::new(Vec::new()),
            recent_links: VecDeque::new(),
            max_recent_links: 100,
        }
    }

    /// Get the container title pattern this window is tracking
    pub fn get_container_title(&self) -> &str {
        &self.container_title
    }

    /// Set highlight patterns for this window (only recompiles if changed)
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.highlight_engine.update_if_changed(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.highlight_engine.set_replace_enabled(enabled);
    }

    /// Update from ContainerData if generation changed.
    /// Returns true if content was updated.
    pub fn update_from_cache(&mut self, container: &ContainerData) -> bool {
        // Skip if generation hasn't changed
        if container.generation == self.last_generation {
            return false;
        }

        self.last_generation = container.generation;

        // Update title from container if we don't have a custom title
        if self.title.is_empty() && !container.title.is_empty() {
            self.title = container.title.clone();
        }

        // Clear existing lines
        self.lines.clear();
        self.scroll_offset = 0;

        // Parse each item line from the container
        for item_content in &container.items {
            let segments = self.parse_container_item(item_content);
            if !segments.is_empty() {
                // Apply highlights
                let highlighted = self
                    .highlight_engine
                    .apply_highlights_to_segments(&segments, "container")
                    .unwrap_or(segments);
                self.lines.push(highlighted);
            }
        }

        true
    }

    /// Parse a container item string (with potential XML/links) into TextSegments
    fn parse_container_item(&mut self, content: &str) -> Vec<TextSegment> {
        let mut segments = Vec::new();
        let mut current_pos = 0;
        let content_len = content.len();

        while current_pos < content_len {
            // Look for <a tag
            if let Some(a_start) = content[current_pos..].find("<a ") {
                let a_start_abs = current_pos + a_start;

                // Add text before the link
                if a_start > 0 {
                    let text = &content[current_pos..a_start_abs];
                    if !text.is_empty() {
                        segments.push(TextSegment {
                            text: text.to_string(),
                            fg: None,
                            bg: None,
                            bold: false,
                            span_type: crate::data::SpanType::Normal,
                            link_data: None,
                        });
                    }
                }

                // Parse the <a> tag
                if let Some(tag_end) = content[a_start_abs..].find('>') {
                    let tag_end_abs = a_start_abs + tag_end;
                    let tag = &content[a_start_abs..=tag_end_abs];

                    // Extract attributes
                    let exist_id = Self::extract_attribute(tag, "exist")
                        .unwrap_or_default();
                    let noun = Self::extract_attribute(tag, "noun")
                        .unwrap_or_default();

                    // Find closing </a>
                    if let Some(close_start) = content[tag_end_abs + 1..].find("</a>") {
                        let close_start_abs = tag_end_abs + 1 + close_start;
                        let link_text = &content[tag_end_abs + 1..close_start_abs];

                        // Create link data
                        let link_data = crate::data::LinkData {
                            exist_id,
                            noun,
                            text: link_text.to_string(),
                            coord: None,
                        };

                        // Cache link for click detection
                        self.cache_link(&link_data);

                        // Add link segment
                        segments.push(TextSegment {
                            text: link_text.to_string(),
                            fg: Some("#00FFFF".to_string()), // Cyan for links
                            bg: None,
                            bold: false,
                            span_type: crate::data::SpanType::Link,
                            link_data: Some(link_data),
                        });

                        current_pos = close_start_abs + 4; // Skip past </a>
                        continue;
                    }
                }

                // If we couldn't parse the link, skip the <a and continue
                current_pos = a_start_abs + 2;
            } else {
                // No more links - add remaining text
                let text = &content[current_pos..];
                if !text.is_empty() {
                    segments.push(TextSegment {
                        text: text.to_string(),
                        fg: None,
                        bg: None,
                        bold: false,
                        span_type: crate::data::SpanType::Normal,
                        link_data: None,
                    });
                }
                break;
            }
        }

        segments
    }

    /// Extract an attribute value from an XML tag
    fn extract_attribute(tag: &str, attr_name: &str) -> Option<String> {
        // Look for attr="value" or attr='value'
        let pattern1 = format!("{}=\"", attr_name);
        let pattern2 = format!("{}='", attr_name);

        if let Some(start) = tag.find(&pattern1) {
            let value_start = start + pattern1.len();
            if let Some(end) = tag[value_start..].find('"') {
                return Some(tag[value_start..value_start + end].to_string());
            }
        }

        if let Some(start) = tag.find(&pattern2) {
            let value_start = start + pattern2.len();
            if let Some(end) = tag[value_start..].find('\'') {
                return Some(tag[value_start..value_start + end].to_string());
            }
        }

        None
    }

    /// Cache a link for click detection
    fn cache_link(&mut self, link_data: &crate::data::LinkData) {
        // Check if we already have this exist_id
        if let Some(last) = self.recent_links.back() {
            if last.exist_id == link_data.exist_id {
                return; // Don't duplicate
            }
        }

        self.recent_links.push_back(link_data.clone());
        if self.recent_links.len() > self.max_recent_links {
            self.recent_links.pop_front();
        }
    }

    /// Clear content
    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll_offset = 0;
        self.last_generation = 0;
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

    /// Get the start line offset (for click detection)
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

    /// Render the container window
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

        // Create border block with title
        let display_title = if self.lines.is_empty() {
            format!("{} (empty)", self.title)
        } else {
            format!("{} [{}]", self.title, self.lines.len())
        };

        let mut block = Block::default();

        if self.show_border {
            let border_color = self.border_color.unwrap_or(Color::White);

            block = block
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(display_title.as_str());
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

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, _focused: bool) {
        // For now, just call regular render - focus styling can be added later
        self.render(area, buf);
    }
}

/// Parse a color string to ratatui Color
fn parse_hex_color(input: &str) -> Option<Color> {
    parse_color_to_ratatui(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // Constructor tests
    // ===========================================

    #[test]
    fn test_new_defaults() {
        let cw = ContainerWindow::new("my_bag".to_string(), "My Bag".to_string());
        assert_eq!(cw.container_title, "my_bag");
        assert_eq!(cw.title, "My Bag");
        assert!(cw.show_border);
        assert!(cw.lines.is_empty());
        assert_eq!(cw.scroll_offset, 0);
        assert_eq!(cw.last_generation, 0);
    }

    #[test]
    fn test_get_container_title() {
        let cw = ContainerWindow::new("backpack".to_string(), "Backpack".to_string());
        assert_eq!(cw.get_container_title(), "backpack");
    }

    // ===========================================
    // Configuration tests
    // ===========================================

    #[test]
    fn test_set_title() {
        let mut cw = ContainerWindow::new("bag".to_string(), "".to_string());
        cw.set_title("New Title".to_string());
        assert_eq!(cw.title, "New Title");
    }

    #[test]
    fn test_set_border_config() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.set_border_config(false, None);
        assert!(!cw.show_border);
        assert!(cw.border_color.is_none());

        cw.set_border_config(true, Some("#FF0000".to_string()));
        assert!(cw.show_border);
        assert!(cw.border_color.is_some());
    }

    #[test]
    fn test_set_text_color() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        assert!(cw.text_color.is_none());

        cw.set_text_color(Some("#00FF00".to_string()));
        assert!(cw.text_color.is_some());
    }

    #[test]
    fn test_set_transparent_background() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        assert!(!cw.transparent_background);

        cw.set_transparent_background(true);
        assert!(cw.transparent_background);
    }

    // ===========================================
    // Scrolling tests
    // ===========================================

    #[test]
    fn test_scroll_up_from_zero() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        assert_eq!(cw.scroll_offset, 0);

        cw.scroll_up(5);
        // With no lines, scroll should be capped at 0
        assert_eq!(cw.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_down_from_zero() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.scroll_offset = 5;

        cw.scroll_down(3);
        assert_eq!(cw.scroll_offset, 2);

        cw.scroll_down(5);
        assert_eq!(cw.scroll_offset, 0); // Saturates at 0
    }

    // ===========================================
    // Clear tests
    // ===========================================

    #[test]
    fn test_clear() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.scroll_offset = 10;
        cw.last_generation = 5;

        cw.clear();
        assert!(cw.lines.is_empty());
        assert_eq!(cw.scroll_offset, 0);
        assert_eq!(cw.last_generation, 0);
    }

    // ===========================================
    // Inner size tests
    // ===========================================

    #[test]
    fn test_update_inner_size_with_border() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.show_border = true;

        cw.update_inner_size(80, 24);
        assert_eq!(cw.inner_width, 78); // 80 - 2 for borders
        assert_eq!(cw.inner_height, 22); // 24 - 2 for borders
    }

    #[test]
    fn test_update_inner_size_without_border() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.show_border = false;

        cw.update_inner_size(80, 24);
        assert_eq!(cw.inner_width, 80);
        assert_eq!(cw.inner_height, 24);
    }

    // ===========================================
    // Extract attribute tests
    // ===========================================

    #[test]
    fn test_extract_attribute_double_quotes() {
        let tag = r#"<a exist="12345" noun="sword">"#;
        let result = ContainerWindow::extract_attribute(tag, "exist");
        assert_eq!(result, Some("12345".to_string()));
    }

    #[test]
    fn test_extract_attribute_single_quotes() {
        let tag = r#"<a exist='67890' noun='dagger'>"#;
        let result = ContainerWindow::extract_attribute(tag, "noun");
        assert_eq!(result, Some("dagger".to_string()));
    }

    #[test]
    fn test_extract_attribute_missing() {
        let tag = r#"<a exist="12345">"#;
        let result = ContainerWindow::extract_attribute(tag, "noun");
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_attribute_empty_value() {
        let tag = r#"<a exist="" noun="test">"#;
        let result = ContainerWindow::extract_attribute(tag, "exist");
        assert_eq!(result, Some("".to_string()));
    }

    // ===========================================
    // Parse container item tests
    // ===========================================

    #[test]
    fn test_parse_container_item_plain_text() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        let segments = cw.parse_container_item("a simple sword");

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "a simple sword");
        assert!(segments[0].link_data.is_none());
    }

    #[test]
    fn test_parse_container_item_with_link() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        let segments = cw.parse_container_item(r#"a <a exist="123" noun="sword">gleaming sword</a>"#);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "a ");
        assert!(segments[0].link_data.is_none());

        assert_eq!(segments[1].text, "gleaming sword");
        assert!(segments[1].link_data.is_some());
        let link = segments[1].link_data.as_ref().unwrap();
        assert_eq!(link.exist_id, "123");
        assert_eq!(link.noun, "sword");
    }

    #[test]
    fn test_parse_container_item_multiple_links() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        let segments = cw.parse_container_item(
            r#"<a exist="1" noun="sword">sword</a> and <a exist="2" noun="shield">shield</a>"#,
        );

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "sword");
        assert!(segments[0].link_data.is_some());

        assert_eq!(segments[1].text, " and ");
        assert!(segments[1].link_data.is_none());

        assert_eq!(segments[2].text, "shield");
        assert!(segments[2].link_data.is_some());
    }

    #[test]
    fn test_parse_container_item_empty_string() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        let segments = cw.parse_container_item("");
        assert!(segments.is_empty());
    }

    // ===========================================
    // Update from cache tests
    // ===========================================

    #[test]
    fn test_update_from_cache_no_change() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        let container = ContainerData {
            id: "1".to_string(),
            title: "Bag".to_string(),
            items: vec![],
            generation: 0,
        };

        // First call with generation 0 should return false (matches default)
        let changed = cw.update_from_cache(&container);
        assert!(!changed);
    }

    #[test]
    fn test_update_from_cache_with_change() {
        let mut cw = ContainerWindow::new("bag".to_string(), "".to_string());
        let container = ContainerData {
            id: "2".to_string(),
            title: "My Bag".to_string(),
            items: vec!["an item".to_string()],
            generation: 1,
        };

        let changed = cw.update_from_cache(&container);
        assert!(changed);
        assert_eq!(cw.title, "My Bag"); // Title updated from container
        assert_eq!(cw.lines.len(), 1);
        assert_eq!(cw.last_generation, 1);
    }

    #[test]
    fn test_update_from_cache_preserves_custom_title() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Custom Title".to_string());
        let container = ContainerData {
            id: "3".to_string(),
            title: "Container Title".to_string(),
            items: vec![],
            generation: 1,
        };

        cw.update_from_cache(&container);
        assert_eq!(cw.title, "Custom Title"); // Custom title preserved
    }

    // ===========================================
    // Get lines tests
    // ===========================================

    #[test]
    fn test_get_lines_empty() {
        let cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        assert!(cw.get_lines().is_empty());
    }

    #[test]
    fn test_get_start_line_empty() {
        let cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        assert_eq!(cw.get_start_line(), 0);
    }
}
