//! Target list widget for tracking creatures in the room.
//!
//! Parses creature data from room objs component (names, IDs, statuses) and uses
//! dDBTarget dropdown to identify which creature is currently targeted.
//! Uses ListWidget for proper text rendering with clickable links.

use crate::data::LinkData;
use ratatui::{buffer::Buffer, layout::Rect};

pub struct Targets {
    widget: super::list_widget::ListWidget,
    count: u32,
    base_title: String,
    /// Track current target for highlighting
    current_target: String,
    /// Generation counter for change detection
    generation: u64,
    /// Cached creature IDs for change detection (comma-joined)
    creature_ids_cache: String,
    /// Color for the target indicator on current target (applied to text color)
    indicator_color: Option<String>,
}

impl Targets {
    pub fn new(title: &str) -> Self {
        Self {
            widget: super::list_widget::ListWidget::new(title),
            count: 0,
            base_title: title.to_string(),
            current_target: String::new(),
            generation: 0,
            creature_ids_cache: String::new(),
            indicator_color: None,
        }
    }

    /// Set the color for the target indicator (►) on current target
    pub fn set_indicator_color(&mut self, color: Option<String>) {
        self.indicator_color = color;
    }

    /// Update the widget from room creatures and current target.
    /// Returns true if the display changed.
    pub fn update_from_state(
        &mut self,
        room_creatures: &[crate::core::state::Creature],
        current_target: &str,
        config: &crate::config::TargetListConfig,
        widget_width: u16,
    ) -> bool {
        // Build new IDs string for comparison (detects room changes even with same count)
        let new_ids: String = room_creatures
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let new_count = room_creatures.len() as u32;

        // Quick check: if IDs, current_target, and count all match, no change
        if self.creature_ids_cache == new_ids
            && self.current_target == current_target
            && self.count == new_count
        {
            return false;
        }

        // Update cache
        self.creature_ids_cache = new_ids;

        tracing::debug!(
            "Targets[{}]::update_from_state - old_count={}, new_count={}, current='{}'",
            self.base_title,
            self.count,
            new_count,
            current_target
        );

        self.widget.clear();
        self.count = 0;
        self.current_target = current_target.to_string();

        for creature in room_creatures.iter() {
            tracing::debug!(
                "Processing creature: name='{}', noun={:?}, id='{}', status={:?}",
                creature.name, creature.noun, creature.id, creature.status
            );

            // Check if this is the current target (compare by ID, not name)
            // current_target is now the ID (e.g., "#209852066") from the parser
            let is_current = creature.id == current_target;

            // Calculate available width (widget width minus borders and padding)
            // Border (2) + margin (2)
            let available_width = widget_width.saturating_sub(4) as usize;

            // Build display text with status based on configuration
            // Always look up abbreviation in config; fallback to truncating to 3 chars
            let status_text = creature.status.as_ref().map(|s| {
                let abbreviated = config.status_abbrev
                    .get(&s.to_lowercase())
                    .cloned()
                    .unwrap_or_else(|| {
                        // No abbreviation defined - truncate to 3 chars
                        if s.len() <= 3 {
                            s.to_string()
                        } else {
                            s.chars().take(3).collect()
                        }
                    });
                format!("[{}]", abbreviated)
            });
            let status_len = status_text.as_ref().map(|s| s.len()).unwrap_or(0);

            // Choose name based on truncation mode and available width
            let base_name = if config.truncation_mode == "noun" && creature.status.is_some() {
                // When truncation_mode is "noun" and there's a status, check if full name + status fits
                let full_len = creature.name.len() + status_len + 1; // +1 for space
                if full_len > available_width {
                    // Use noun instead (either from parser or fallback to last word)
                    creature.noun.as_ref()
                        .map(|n| n.clone())
                        .or_else(|| {
                            creature.name.split_whitespace().last().map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| creature.name.clone())
                } else {
                    creature.name.clone()
                }
            } else {
                // Use full name (ProgressBar will truncate if needed)
                creature.name.clone()
            };

            // Build final display name with status positioned according to config
            let display_name = if let Some(ref status) = status_text {
                if config.status_position == "start" {
                    format!("{} {}", status, base_name)
                } else {
                    // Default: "end"
                    format!("{} {}", base_name, status)
                }
            } else {
                base_name.clone()
            };

            // Use creature ID as the unique identifier
            let id = creature.id.clone();

            // Build LinkData for clickable targeting
            // - exist_id: ID without # prefix (e.g., "209852066")
            // - noun: Use parsed noun or fallback to last word
            // - text: full creature name
            let exist_id = creature.id.trim_start_matches('#').to_string();
            let link_noun = creature.noun.as_ref()
                .map(|n| n.clone())
                .or_else(|| {
                    creature.name.split_whitespace().last().map(|s| s.to_string())
                })
                .unwrap_or_else(|| creature.name.clone());
            let link_data = Some(LinkData {
                exist_id,
                noun: link_noun,
                text: creature.name.clone(),
                coord: None,
            });

            // Apply indicator color to current target for visual distinction
            let item_text_color = if is_current {
                tracing::debug!(
                    "Current target: {} ({}) - applying indicator_color = {:?}",
                    creature.name,
                    creature.id,
                    self.indicator_color
                );
                self.indicator_color.clone()
            } else {
                None
            };

            tracing::debug!(
                "Adding to container: display_name='{}' (is_current={}, avail_width={}, truncation_mode={}, status_pos={})",
                display_name, is_current, available_width, config.truncation_mode, config.status_position
            );

            // Add to widget with link data for click handling
            // NOTE: This is the critical fix - ListWidget doesn't have ProgressBar's
            // ensure_contrast() logic, so item_text_color is preserved exactly!
            self.widget.add_simple_line(display_name, item_text_color, link_data);

            self.count += 1;
        }

        self.generation += 1;
        self.update_title();
        true
    }

    /// Clear all targets
    pub fn clear(&mut self) {
        self.widget.clear();
        self.count = 0;
        self.current_target.clear();
        self.creature_ids_cache.clear();
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

    pub fn set_bar_color(&mut self, _color: String) {
        // No-op: ListWidget doesn't have progress bars
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
    /// Returns the target command to send if a creature was clicked (e.g., "target #209852066").
    pub fn handle_click(&self, y: u16, area: Rect) -> Option<String> {
        // Delegate to ListWidget's click handling (x=0 since ListWidget doesn't use it)
        let link = self.widget.handle_click(0, y, area)?;

        // Return the target command with the creature's ID
        Some(format!("target #{}", link.exist_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::Creature;

    // ===========================================
    // Constructor tests
    // ===========================================

    #[test]
    fn test_new_defaults() {
        let dt = Targets::new("Targets");
        assert_eq!(dt.base_title, "Targets");
        assert_eq!(dt.count, 0);
        assert!(dt.current_target.is_empty());
        assert_eq!(dt.generation, 0);
    }

    // ===========================================
    // Title tests
    // ===========================================

    #[test]
    fn test_set_title() {
        let mut dt = Targets::new("Targets");
        dt.set_title("Creatures");
        assert_eq!(dt.base_title, "Creatures");
    }

    #[test]
    fn test_update_title_with_count() {
        let mut dt = Targets::new("Targets");
        dt.count = 5;
        dt.update_title();
        // Title should include count: "Targets [05]"
    }

    #[test]
    fn test_empty_title() {
        let mut dt = Targets::new("");
        dt.count = 3;
        dt.update_title();
        // Should not panic with empty title
    }

    // ===========================================
    // Generation tests
    // ===========================================

    #[test]
    fn test_get_generation() {
        let dt = Targets::new("Targets");
        assert_eq!(dt.get_generation(), 0);
    }

    #[test]
    fn test_generation_increments_on_update() {
        let mut dt = Targets::new("Targets");
        let creatures = vec![Creature {
            id: "123".to_string(),
            name: "a goblin".to_string(),
            noun: Some("goblin".to_string()),
            status: None,
        }];

        let config = crate::config::TargetListConfig::default();
        dt.update_from_state(&creatures, "123", &config, 30);
        assert_eq!(dt.get_generation(), 1);
    }

    // ===========================================
    // Update from state tests
    // ===========================================

    #[test]
    fn test_update_from_state_empty() {
        let mut dt = Targets::new("Targets");
        let creatures: Vec<Creature> = vec![];
        let config = crate::config::TargetListConfig::default();

        // First update with empty state matches initial widget state, so no change
        let changed = dt.update_from_state(&creatures, "", &config, 30);
        assert!(!changed);
        assert_eq!(dt.count, 0);
    }

    #[test]
    fn test_update_from_state_with_creatures() {
        let mut dt = Targets::new("Targets");
        let creatures = vec![
            Creature {
                id: "1".to_string(),
                name: "a kobold".to_string(),
                noun: Some("kobold".to_string()),
                status: None,
            },
            Creature {
                id: "2".to_string(),
                name: "a goblin".to_string(),
                noun: Some("goblin".to_string()),
                status: None,
            },
        ];
        let config = crate::config::TargetListConfig::default();

        let changed = dt.update_from_state(&creatures, "1", &config, 30);
        assert!(changed);
        assert_eq!(dt.count, 2);
        assert_eq!(dt.current_target, "1");
    }

    #[test]
    fn test_update_from_state_no_change() {
        let mut dt = Targets::new("Targets");
        let creatures = vec![Creature {
            id: "1".to_string(),
            name: "a kobold".to_string(),
            noun: Some("kobold".to_string()),
            status: None,
        }];
        let config = crate::config::TargetListConfig::default();

        dt.update_from_state(&creatures, "1", &config, 30);
        let initial_gen = dt.get_generation();

        // Same state again
        let changed = dt.update_from_state(&creatures, "1", &config, 30);
        assert!(!changed);
        assert_eq!(dt.get_generation(), initial_gen); // Generation unchanged
    }

    #[test]
    fn test_update_from_state_current_target_change() {
        let mut dt = Targets::new("Targets");
        let creatures = vec![
            Creature {
                id: "1".to_string(),
                name: "a kobold".to_string(),
                noun: Some("kobold".to_string()),
                status: None,
            },
            Creature {
                id: "2".to_string(),
                name: "a goblin".to_string(),
                noun: Some("goblin".to_string()),
                status: None,
            },
        ];
        let config = crate::config::TargetListConfig::default();

        dt.update_from_state(&creatures, "1", &config, 30);

        // Change current target
        let changed = dt.update_from_state(&creatures, "2", &config, 30);

        assert!(changed);
        assert_eq!(dt.current_target, "2");
    }

    // ===========================================
    // Clear tests
    // ===========================================

    #[test]
    fn test_clear() {
        let mut dt = Targets::new("Targets");
        let creatures = vec![Creature {
            id: "1".to_string(),
            name: "a kobold".to_string(),
            noun: Some("kobold".to_string()),
            status: None,
        }];
        let config = crate::config::TargetListConfig::default();

        dt.update_from_state(&creatures, "1", &config, 30);
        assert_eq!(dt.count, 1);

        dt.clear();
        assert_eq!(dt.count, 0);
        assert!(dt.current_target.is_empty());
    }

    // ===========================================
    // Scroll tests
    // ===========================================

    #[test]
    fn test_scroll_up() {
        let mut dt = Targets::new("Targets");
        // Just verify it doesn't panic
        dt.scroll_up(5);
    }

    #[test]
    fn test_scroll_down() {
        let mut dt = Targets::new("Targets");
        // Just verify it doesn't panic
        dt.scroll_down(5);
    }
}
