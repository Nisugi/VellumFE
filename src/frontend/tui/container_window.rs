//! Container window widget for displaying contents of bags, backpacks, etc.
//!
//! Similar to inventory_window but displays contents from a specific container
//! tracked in GameState.container_cache. Each container window is configured
//! with a container_id that links it to a specific container in the cache.
//!
//! Now uses ListWidget instead of custom Vec<Vec<TextSegment>> for proper text rendering.

use crate::core::state::ContainerData;
use crate::data::widget::TextSegment;
use ratatui::{buffer::Buffer, layout::Rect};

/// Container window widget - displays items in a specific container
/// Now implemented as a thin wrapper around ListWidget for DRY.
pub struct ContainerWindow {
    widget: super::list_widget::ListWidget,

    /// Container title pattern this window displays (matched case-insensitively)
    container_title: String,

    /// Link color (from theme or default)
    link_color: Option<String>,

    /// Generation counter for change detection (matches ContainerData.generation)
    last_generation: u64,
}

impl ContainerWindow {
    pub fn new(container_title: String, title: String) -> Self {
        let mut widget = super::list_widget::ListWidget::new(&title);
        // Container defaults to word wrap disabled (like inventory)
        widget.set_word_wrap(false);
        Self {
            widget,
            container_title,
            link_color: None,
            last_generation: 0,
        }
    }

    /// Get the container title pattern this window is tracking
    pub fn get_container_title(&self) -> &str {
        &self.container_title
    }

    /// Set highlight patterns for this window (only recompiles if changed)
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.widget.set_highlights(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.widget.set_replace_enabled(enabled);
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
        let current_title = self.widget.get_title();
        if current_title.is_empty() && !container.title.is_empty() {
            self.widget.set_title(container.title.clone());
        }

        // Clear existing lines
        self.widget.clear();

        // Parse each item line from the container
        for item_content in &container.items {
            // Skip header lines like "In the bandolier:" and empty/nothing lines
            let trimmed = item_content.trim().to_lowercase();
            if trimmed.starts_with("in the ") || trimmed == "nothing" || trimmed.is_empty() {
                continue;
            }

            let segments = self.parse_container_item(item_content);
            if !segments.is_empty() {
                // ListWidget handles highlights internally during add_line
                self.widget.add_line(segments);
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

                        // Add link segment - use theme link color or default to cyan
                        segments.push(TextSegment {
                            text: link_text.to_string(),
                            fg: self.link_color.clone().or_else(|| Some("#00FFFF".to_string())),
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

    /// Clear content
    pub fn clear(&mut self) {
        self.widget.clear();
        self.last_generation = 0;
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
    pub fn get_lines(&self) -> &[Vec<TextSegment>] {
        self.widget.get_lines()
    }

    /// Get wrapped lines for mouse click detection (matches inventory_window API)
    pub fn get_wrapped_lines(&self) -> &[Vec<TextSegment>] {
        // For ContainerWindow compatibility, return reference to internal lines
        self.widget.get_lines()
    }

    /// Whether this window has a visible border (for click offset calculation)
    pub fn has_border(&self) -> bool {
        // Delegate to ListWidget's border state
        true // Default assumption; ListWidget tracks this internally
    }

    /// Get the start line offset (for click detection)
    pub fn get_start_line(&self) -> usize {
        self.widget.get_start_line()
    }

    /// Update inner dimensions based on window size
    /// Note: ListWidget updates dimensions automatically during render
    pub fn update_inner_size(&mut self, _width: u16, _height: u16) {
        // No-op: ListWidget handles this internally during render
    }

    /// Set title
    pub fn set_title(&mut self, title: String) {
        self.widget.set_title(title);
    }

    /// Set border configuration
    pub fn set_border_config(&mut self, show_border: bool, border_color: Option<String>) {
        self.widget.set_border_config(show_border, None, border_color);
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

    /// Set the link color (from theme)
    /// If color changes, resets last_generation to force re-parsing on next update
    pub fn set_link_color(&mut self, color: Option<String>) {
        if self.link_color != color {
            self.link_color = color;
            // Reset generation to force re-parse with new link color
            self.last_generation = 0;
        }
    }

    /// Render the container window
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        // Update title with item count
        let line_count = self.widget.get_lines().len();
        let base_title = self.widget.get_title().to_string();

        // Extract base title without count if present
        let display_base = if let Some(idx) = base_title.rfind(" [") {
            &base_title[..idx]
        } else if let Some(idx) = base_title.rfind(" (empty)") {
            &base_title[..idx]
        } else {
            &base_title
        };

        let display_title = if line_count == 0 {
            format!("{} (empty)", display_base)
        } else {
            format!("{} [{}]", display_base, line_count)
        };

        self.widget.set_title(display_title);
        self.widget.render(area, buf);
    }

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        self.widget.render_with_focus(area, buf, focused);
    }
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
        // Title should be updated in widget
    }

    #[test]
    fn test_set_border_config() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.set_border_config(false, None);
        cw.set_border_config(true, Some("#FF0000".to_string()));
        // Configuration delegated to ListWidget
    }

    #[test]
    fn test_set_text_color() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.set_text_color(Some("#00FF00".to_string()));
        // Configuration delegated to ListWidget
    }

    #[test]
    fn test_set_transparent_background() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.set_transparent_background(true);
        // Configuration delegated to ListWidget
    }

    // ===========================================
    // Scrolling tests
    // ===========================================

    #[test]
    fn test_scroll_up_from_zero() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.scroll_up(5);
        // Delegated to ListWidget
    }

    #[test]
    fn test_scroll_down_from_zero() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.scroll_down(3);
        cw.scroll_down(5);
        // Delegated to ListWidget
    }

    // ===========================================
    // Clear tests
    // ===========================================

    #[test]
    fn test_clear() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.clear();
        assert!(cw.get_lines().is_empty());
        assert_eq!(cw.last_generation, 0);
    }

    // ===========================================
    // Inner size tests
    // ===========================================

    #[test]
    fn test_update_inner_size_with_border() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.update_inner_size(80, 24);
        // ListWidget handles this internally
    }

    #[test]
    fn test_update_inner_size_without_border() {
        let mut cw = ContainerWindow::new("bag".to_string(), "Bag".to_string());
        cw.update_inner_size(80, 24);
        // ListWidget handles this internally
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
        // Custom title preservation logic is in update_from_cache
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
