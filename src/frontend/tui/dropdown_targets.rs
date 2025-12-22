//! Dropdown-based target list widget for direct-connect users.
//!
//! Reads target data from GameState.target_list (populated from dDBTarget dropdown)
//! rather than parsing text from the combat stream. This widget is an alternative
//! to the stream-based targets widget for users connecting directly to GemStone IV
//! without Lich's `;targetlist` script.
//!
//! Note: This widget does NOT show creature status (stunned, sitting, etc.) because
//! the dDBTarget dropdown only provides creature names and IDs, not status information.

use super::scrollable_container::ScrollableContainer;
use crate::core::state::TargetListState;
use ratatui::{buffer::Buffer, layout::Rect};

pub struct DropdownTargets {
    container: ScrollableContainer,
    count: u32,
    base_title: String,
    /// Track current target for highlighting
    current_target: String,
    /// Generation counter for change detection
    generation: u64,
}

impl DropdownTargets {
    pub fn new(title: &str) -> Self {
        let mut container = ScrollableContainer::new(title);
        // Dropdown targets widget hides values and percentages (no status info available)
        container.set_display_options(false, false);

        Self {
            container,
            count: 0,
            base_title: title.to_string(),
            current_target: String::new(),
            generation: 0,
        }
    }

    /// Update the widget from TargetListState.
    /// Returns true if the display changed.
    pub fn update_from_state(&mut self, target_list: &TargetListState) -> bool {
        // Quick check: if current_target and creature count match, assume no change
        // (This is a simple optimization; could be more sophisticated with full comparison)
        let new_count = target_list.creatures.len() as u32;
        if self.current_target == target_list.current_target && self.count == new_count {
            return false;
        }

        self.container.clear();
        self.count = 0;
        self.current_target = target_list.current_target.clone();

        for creature in target_list.creatures.iter() {
            // Check if this is the current target
            let is_current = creature.name == target_list.current_target
                || creature.id == target_list.current_target;

            // Add prefix for current target
            let display_name = if is_current {
                format!("► {}", creature.name)
            } else {
                creature.name.clone()
            };

            // Use creature ID as the unique identifier
            let id = creature.id.clone();

            // Add to container (no status suffix available from dropdown)
            self.container.add_or_update_item_full(
                id,
                display_name,
                None,   // no alternate text
                0,      // value (hidden)
                1,      // max (hidden)
                None,   // no suffix (status not available from dropdown)
                None,   // no color override
                None,
            );

            self.count += 1;
        }

        self.generation += 1;
        self.update_title();
        true
    }

    /// Clear all targets
    pub fn clear(&mut self) {
        self.container.clear();
        self.count = 0;
        self.current_target.clear();
        self.generation += 1;
        self.update_title();
    }

    fn update_title(&mut self) {
        if self.base_title.is_empty() {
            self.container.set_title(String::new());
        } else {
            let title = format!("{} [{:02}]", self.base_title, self.count);
            self.container.set_title(title);
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
        self.container.scroll_up(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.container.scroll_down(amount);
    }

    pub fn set_border_config(&mut self, show: bool, style: Option<String>, color: Option<String>) {
        self.container.set_border_config(show, style, color);
    }

    pub fn set_border_sides(&mut self, sides: crate::config::BorderSides) {
        self.container.set_border_sides(sides);
    }

    pub fn set_bar_color(&mut self, color: String) {
        self.container.set_bar_color(color);
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.container.set_background_color(color);
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.container.set_text_color(color);
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.container.set_transparent_background(transparent);
    }

    /// Set highlight patterns for this widget
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.container.set_highlights(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.container.set_replace_enabled(enabled);
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.container.render(area, buf);
    }

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        self.container.render_with_focus(area, buf, focused);
    }
}
