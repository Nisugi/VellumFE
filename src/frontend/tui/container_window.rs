//! Container window widget for displaying contents of bags, backpacks, etc.
//!
//! Similar to inventory_window but displays contents from a specific
//! container in the GameObjects registry (`game_state.objects`). Each
//! container window is bound to a container title; the frontend resolves
//! it to a `Container` each frame and feeds it here.
//!
//! Uses ListWidget for rendering. Items arrive pre-parsed as `GameItem`s
//! from the registry's single anchor parser — this widget no longer parses
//! `<a>` tags itself (that duplicate parser was deleted in the registry
//! migration).

use crate::core::game_objects::Container;
use crate::data::widget::TextSegment;
use ratatui::{buffer::Buffer, layout::Rect};

/// Container window widget - displays items in a specific container
/// Now implemented as a thin wrapper around ListWidget for DRY.
pub struct ContainerWindow {
    widget: super::list_widget::ListWidget,

    /// Container title pattern this window displays (matched case-insensitively)

    /// Link color (from theme or default)
    link_color: Option<String>,

    /// Generation counter for change detection (matches ContainerData.generation)
    last_generation: u64,
}

impl ContainerWindow {
    pub fn new(title: String) -> Self {
        let mut widget = super::list_widget::ListWidget::new(&title);
        // Container defaults to word wrap disabled (like inventory)
        widget.set_word_wrap(false);
        Self {
            widget,
            link_color: None,
            last_generation: 0,
        }
    }

    /// Set highlight patterns for this window (only recompiles if changed)
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.widget.set_highlights(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.widget.set_replace_enabled(enabled);
    }

    /// Update from a registry `Container` if its generation changed.
    /// Returns true if content was updated.
    ///
    /// Items are already parsed (`GameItem { id, noun, name }`) — one
    /// clickable line each, styled with the link color. The header line is
    /// already excluded upstream by the registry's ingest.
    pub fn update_from_cache(&mut self, container: &Container) -> bool {
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

        // One clickable line per registry item.
        for item in &container.items {
            let segment = TextSegment {
                text: item.name.clone(),
                fg: self
                    .link_color
                    .clone()
                    .or_else(|| Some("#00FFFF".to_string())),
                bg: None,
                bold: false,
                mono: false,
                span_type: crate::data::SpanType::Link,
                link_data: Some(crate::data::LinkData {
                    exist_id: item.id.clone(),
                    noun: item.noun.clone(),
                    text: item.name.clone(),
                    coord: None,
                }),
            };
            // ListWidget handles highlights internally during add_line.
            self.widget.add_line(vec![segment]);
        }

        true
    }

    /// Scroll up by N lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.widget.scroll_up(lines);
    }

    /// Scroll down by N lines
    pub fn scroll_down(&mut self, lines: usize) {
        self.widget.scroll_down(lines);
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

    /// Set title
    pub fn set_title(&mut self, title: String) {
        self.widget.set_title(title);
    }

    /// Set border configuration
    pub fn set_border_config(&mut self, show_border: bool, border_color: Option<String>) {
        self.widget
            .set_border_config(show_border, None, border_color);
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

    /// Convert mouse position to text coordinates
    pub fn mouse_to_text_coords(
        &self,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: Rect,
    ) -> Option<(usize, usize)> {
        self.widget
            .mouse_to_text_coords(mouse_col, mouse_row, window_rect)
    }

    /// Extract text from a selection range
    pub fn extract_selection_text(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        self.widget
            .extract_selection_text(start_line, start_col, end_line, end_col)
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
        let cw = ContainerWindow::new("My Bag".to_string());
        assert_eq!(cw.last_generation, 0);
    }

    // ===========================================
    // Configuration tests
    // ===========================================

    #[test]
    fn test_set_title() {
        let mut cw = ContainerWindow::new("".to_string());
        cw.set_title("New Title".to_string());
        // Title should be updated in widget
    }

    #[test]
    fn test_set_border_config() {
        let mut cw = ContainerWindow::new("Bag".to_string());
        cw.set_border_config(false, None);
        cw.set_border_config(true, Some("#FF0000".to_string()));
        // Configuration delegated to ListWidget
    }

    #[test]
    fn test_set_text_color() {
        let mut cw = ContainerWindow::new("Bag".to_string());
        cw.set_text_color(Some("#00FF00".to_string()));
        // Configuration delegated to ListWidget
    }

    #[test]
    fn test_set_transparent_background() {
        let mut cw = ContainerWindow::new("Bag".to_string());
        cw.set_transparent_background(true);
        // Configuration delegated to ListWidget
    }

    // ===========================================
    // Scrolling tests
    // ===========================================

    #[test]
    fn test_scroll_up_from_zero() {
        let mut cw = ContainerWindow::new("Bag".to_string());
        cw.scroll_up(5);
        // Delegated to ListWidget
    }

    #[test]
    fn test_scroll_down_from_zero() {
        let mut cw = ContainerWindow::new("Bag".to_string());
        cw.scroll_down(3);
        cw.scroll_down(5);
        // Delegated to ListWidget
    }

    // ===========================================
    // Clear tests
    // ===========================================

    // ===========================================
    // Update-from-registry tests
    // (anchor parsing now lives in game_objects::parse, tested there)
    // ===========================================

    use crate::core::game_objects::{Container, GameItem};

    fn container(id: &str, title: &str, gen: u64, items: Vec<GameItem>) -> Container {
        Container {
            id: id.to_string(),
            title: title.to_string(),
            title_lower: title.to_lowercase(),
            items,
            generation: gen,
            ..Default::default()
        }
    }

    #[test]
    fn test_update_from_cache_no_change() {
        let mut cw = ContainerWindow::new("Bag".to_string());
        // generation 0 matches the widget default -> no update.
        let c = container("1", "Bag", 0, vec![]);
        assert!(!cw.update_from_cache(&c));
    }

    #[test]
    fn test_update_from_cache_builds_clickable_items() {
        let mut cw = ContainerWindow::new("".to_string());
        let c = container(
            "2",
            "My Bag",
            1,
            vec![
                GameItem::new("101", "sword", "gleaming sword"),
                GameItem::new("102", "shield", "battered shield"),
            ],
        );
        assert!(cw.update_from_cache(&c));
        assert_eq!(cw.last_generation, 1);

        // One clickable line per registry item, name + link intact.
        let lines = cw.widget.get_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0][0].text, "gleaming sword");
        let link = lines[0][0].link_data.as_ref().unwrap();
        assert_eq!(link.exist_id, "101");
        assert_eq!(link.noun, "sword");
        assert_eq!(lines[1][0].link_data.as_ref().unwrap().exist_id, "102");
    }

    #[test]
    fn test_update_from_cache_preserves_custom_title() {
        let mut cw = ContainerWindow::new("Custom Title".to_string());
        let c = container("3", "Container Title", 1, vec![]);
        cw.update_from_cache(&c);
        // A non-empty custom title is not overwritten by the container's.
        assert_eq!(cw.widget.get_title(), "Custom Title");
    }

    // ===========================================
    // Get lines tests
    // ===========================================

    #[test]
    fn test_get_start_line_empty() {
        let cw = ContainerWindow::new("Bag".to_string());
        assert_eq!(cw.get_start_line(), 0);
    }
}
