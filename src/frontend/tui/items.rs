//! Items list widget for showing room objects (non-creature items).
//!
//! Parses item data from room objs component (non-bold links) and displays
//! them with clickable links for popup menus and drag-to-inventory.
//! Uses ListWidget for proper text rendering with clickable links.

use crate::data::LinkData;
use ratatui::{buffer::Buffer, layout::Rect};

pub struct Items {
    widget: super::list_widget::ListWidget,
    count: u32,
    base_title: String,
    /// Generation counter for change detection
    generation: u64,
    /// Cached object IDs for change detection (comma-joined)
    object_ids_cache: String,
}

impl Items {
    pub fn new(title: &str) -> Self {
        Self {
            widget: super::list_widget::ListWidget::new(title),
            count: 0,
            base_title: title.to_string(),
            generation: 0,
            object_ids_cache: String::new(),
        }
    }

    /// Update the widget from room objects.
    /// Returns true if the display changed.
    pub fn update_from_state(
        &mut self,
        room_objects: &[crate::core::state::RoomObject],
    ) -> bool {
        // Build cache string for comparison
        let new_object_ids: String = room_objects
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let new_count = room_objects.len() as u32;

        // Quick check: if cache matches, no change needed
        if self.object_ids_cache == new_object_ids && self.count == new_count {
            return false;
        }

        // Update cache
        self.object_ids_cache = new_object_ids;

        self.widget.clear();
        self.count = 0;

        for obj in room_objects.iter() {
            tracing::debug!(
                "Processing room object: name='{}', noun={:?}, id='{}'",
                obj.name, obj.noun, obj.id
            );

            // Build LinkData for clickable interaction
            // - exist_id: ID (e.g., "123456789")
            // - noun: Use parsed noun or fallback to last word
            // - text: full object name
            let link_noun = obj.noun.as_ref()
                .cloned()
                .or_else(|| {
                    obj.name.split_whitespace().last().map(|s| s.to_string())
                })
                .unwrap_or_else(|| obj.name.clone());

            let link_data = Some(LinkData {
                exist_id: obj.id.clone(),
                noun: link_noun,
                text: obj.name.clone(),
                coord: None,
            });

            // Add to widget with link data for click handling
            self.widget.add_simple_line(obj.name.clone(), None, link_data);

            self.count += 1;
        }

        self.generation += 1;
        self.update_title();
        true
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.widget.clear();
        self.count = 0;
        self.object_ids_cache.clear();
        self.generation += 1;
        self.update_title();
    }

    fn update_title(&mut self) {
        if self.base_title.is_empty() {
            self.widget.set_title(String::new());
        } else {
            let title = format!("{} [{:02}]", self.base_title, self.count);
            self.widget.set_title(title);
        }
    }

    pub fn set_title(&mut self, title: &str) {
        self.base_title = title.to_string();
        self.update_title();
    }

    pub fn get_generation(&self) -> u64 {
        self.generation
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.widget.scroll_up(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.widget.scroll_down(amount);
    }

    pub fn set_border_config(&mut self, show: bool, style: Option<String>, color: Option<String>) {
        self.widget.set_border_config(show, style, color);
    }

    pub fn set_border_sides(&mut self, sides: crate::config::BorderSides) {
        self.widget.set_border_sides(sides);
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.widget.set_background_color(color);
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.widget.set_text_color(color);
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.widget.set_transparent_background(transparent);
    }

    /// Set highlight patterns for this widget
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.widget.set_highlights(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.widget.set_replace_enabled(enabled);
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.widget.render(area, buf);
    }

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        self.widget.render_with_focus(area, buf, focused);
    }

    /// Handle a click at the given coordinates.
    /// Returns the link data if an item was clicked (for popup menu).
    pub fn handle_click(&self, y: u16, area: Rect) -> Option<LinkData> {
        // Delegate to ListWidget's click handling
        self.widget.handle_click(0, y, area)
    }

    /// Convert mouse position to text coordinates
    pub fn mouse_to_text_coords(
        &self,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: Rect,
    ) -> Option<(usize, usize)> {
        self.widget.mouse_to_text_coords(mouse_col, mouse_row, window_rect)
    }

    /// Extract text from a selection range
    pub fn extract_selection_text(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        self.widget.extract_selection_text(start_line, start_col, end_line, end_col)
    }

    /// Get wrapped lines for click/drag detection
    pub fn get_wrapped_lines(&self) -> &[Vec<crate::data::widget::TextSegment>] {
        self.widget.get_lines()
    }

    /// Get the start line offset (accounting for scroll position)
    pub fn get_start_line(&self) -> usize {
        self.widget.get_start_line()
    }

    /// Check if widget has border (for coordinate offset calculation)
    pub fn has_border(&self) -> bool {
        // Items widget always has border by default
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::RoomObject;

    #[test]
    fn test_new_defaults() {
        let items = Items::new("Items");
        assert_eq!(items.base_title, "Items");
        assert_eq!(items.count, 0);
        assert_eq!(items.generation, 0);
    }

    #[test]
    fn test_update_from_state_empty() {
        let mut items = Items::new("Items");
        let objects: Vec<RoomObject> = vec![];

        // First update with empty state matches initial widget state, so no change
        let changed = items.update_from_state(&objects);
        assert!(!changed);
        assert_eq!(items.count, 0);
    }

    #[test]
    fn test_update_from_state_with_objects() {
        let mut items = Items::new("Items");
        let objects = vec![
            RoomObject {
                id: "123".to_string(),
                name: "a silver ring".to_string(),
                noun: Some("ring".to_string()),
            },
            RoomObject {
                id: "456".to_string(),
                name: "some gold coins".to_string(),
                noun: Some("coins".to_string()),
            },
        ];

        let changed = items.update_from_state(&objects);
        assert!(changed);
        assert_eq!(items.count, 2);
    }

    #[test]
    fn test_update_from_state_no_change() {
        let mut items = Items::new("Items");
        let objects = vec![RoomObject {
            id: "123".to_string(),
            name: "a silver ring".to_string(),
            noun: Some("ring".to_string()),
        }];

        items.update_from_state(&objects);
        let initial_gen = items.get_generation();

        // Same state again
        let changed = items.update_from_state(&objects);
        assert!(!changed);
        assert_eq!(items.get_generation(), initial_gen);
    }

    #[test]
    fn test_clear() {
        let mut items = Items::new("Items");
        let objects = vec![RoomObject {
            id: "123".to_string(),
            name: "a silver ring".to_string(),
            noun: Some("ring".to_string()),
        }];

        items.update_from_state(&objects);
        assert_eq!(items.count, 1);

        items.clear();
        assert_eq!(items.count, 0);
    }

    #[test]
    fn test_set_title() {
        let mut items = Items::new("Items");
        items.set_title("Ground Objects");
        assert_eq!(items.base_title, "Ground Objects");
    }
}
