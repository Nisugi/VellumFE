//! Target list widget for tracking creatures in the room.
//!
//! Parses creature data from room objs component (names, IDs, statuses) and uses
//! dDBTarget dropdown to identify which creature is currently targeted.
//! Uses ListWidget for proper text rendering with clickable links.

use crate::data::LinkData;
use ratatui::{buffer::Buffer, layout::Rect};
use regex::Regex;
use std::sync::OnceLock;

/// Regex for body part nouns that should be filtered out
static BODY_PART_REGEX: OnceLock<Regex> = OnceLock::new();

fn get_body_part_regex() -> &'static Regex {
    BODY_PART_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:arm|appendage|claw|limb|pincer|tentacle)s?$|^(?:palpus|palpi)$")
            .unwrap()
    })
}

/// Check if a creature is a body part (arm, tentacle, etc.)
/// Returns true for body parts except "amaranthine kraken tentacle"
fn is_body_part(creature: &crate::core::state::Creature) -> bool {
    let name_lower = creature.name.to_lowercase();

    // Check noun against body part regex
    if let Some(ref noun) = creature.noun {
        if get_body_part_regex().is_match(noun)
            && !name_lower.contains("amaranthine kraken tentacle")
        {
            return true;
        }
    }

    false
}

/// Check if a creature should be filtered from the targets list
/// Based on Lich's filtering logic for dead/gone, animated, and body parts
/// Returns (should_filter, is_body_part) tuple
fn should_filter_creature(creature: &crate::core::state::Creature) -> (bool, bool) {
    // Check if it's a body part first
    let body_part = is_body_part(creature);

    // Filter dead or gone creatures
    if let Some(ref status) = creature.status {
        let status_lower = status.to_lowercase();
        if status_lower.contains("dead") || status_lower.contains("gone") {
            return (true, body_part);
        }
    }

    let name_lower = creature.name.to_lowercase();

    // Filter "animated" creatures except "animated slush"
    if name_lower.starts_with("animated") && !name_lower.starts_with("animated slush") {
        return (true, body_part);
    }

    // Filter body parts
    if body_part {
        return (true, true);
    }

    (false, false)
}

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
    /// Count of filtered body parts (arms, tentacles, etc.)
    body_part_count: u32,
    /// Whether to show body part count on bottom border
    show_body_part_count: bool,
    /// Border color for rendering body part count (from theme)
    border_color: Option<ratatui::style::Color>,
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
            body_part_count: 0,
            show_body_part_count: false,
            border_color: None,
        }
    }

    /// Set whether to show body part count on bottom border
    pub fn set_show_body_part_count(&mut self, show: bool) {
        self.show_body_part_count = show;
    }

    /// Set the color for the target indicator (►) on current target
    pub fn set_indicator_color(&mut self, color: Option<String>) {
        self.indicator_color = color;
    }

    /// Set the border color (used for body part count display)
    pub fn set_border_color(&mut self, color: Option<String>) {
        self.border_color = color.and_then(|c| super::colors::parse_hex_color(&c).ok());
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
        self.body_part_count = 0;
        self.current_target = current_target.to_string();

        for creature in room_creatures.iter() {
            // Apply Lich-style filtering (dead/gone, animated, body parts)
            let (should_filter, is_body_part) = should_filter_creature(creature);
            if is_body_part {
                self.body_part_count += 1;
            }
            if should_filter {
                tracing::debug!(
                    "Filtering creature: name='{}', noun={:?}, status={:?}, is_body_part={}",
                    creature.name, creature.noun, creature.status, is_body_part
                );
                continue;
            }

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
        self.body_part_count = 0;
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
        self.render_body_part_count(area, buf);
    }

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        self.widget.render_with_focus(area, buf, focused);
        self.render_body_part_count(area, buf);
    }

    /// Render body part count on bottom border if enabled and count > 0
    fn render_body_part_count(&self, area: Rect, buf: &mut Buffer) {
        if !self.show_body_part_count || self.body_part_count == 0 {
            return;
        }

        // Only render if we have enough height for a bottom border
        if area.height < 2 {
            return;
        }

        let text = format!(" Arms: {} ", self.body_part_count);
        let bottom_y = area.y + area.height - 1;

        // Center the text on the bottom border
        let text_len = text.len() as u16;
        if text_len >= area.width {
            return;
        }
        let start_x = area.x + (area.width - text_len) / 2;

        // Write the text using border color (from theme)
        use ratatui::style::{Color, Style};
        let color = self.border_color.unwrap_or(Color::White);
        let style = Style::default().fg(color);
        for (i, ch) in text.chars().enumerate() {
            let x = start_x + i as u16;
            if x < area.x + area.width {
                buf[(x, bottom_y)].set_char(ch).set_style(style);
            }
        }
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
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn buffer_line(buf: &Buffer, y: u16, width: u16) -> String {
        let mut line = String::new();
        for x in 0..width {
            line.push_str(buf[(x, y)].symbol());
        }
        line
    }

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

    // ===========================================
    // Render tests
    // ===========================================

    #[test]
    fn test_render_status_position_start() {
        let mut dt = Targets::new("Targets");
        let mut config = crate::config::TargetListConfig::default();
        config.status_position = "start".to_string();

        let creatures = vec![Creature {
            id: "1".to_string(),
            name: "a goblin".to_string(),
            noun: Some("goblin".to_string()),
            status: Some("stunned".to_string()),
        }];

        dt.update_from_state(&creatures, "1", &config, 30);
        dt.set_border_config(false, None, None);

        let area = Rect::new(0, 0, 30, 1);
        let mut buf = Buffer::empty(area);
        dt.render(area, &mut buf);

        let line = buffer_line(&buf, 0, area.width);
        assert!(line.trim_end().starts_with("[stu] a goblin"));
    }

    #[test]
    fn test_render_status_position_end() {
        let mut dt = Targets::new("Targets");
        let mut config = crate::config::TargetListConfig::default();
        config.status_position = "end".to_string();

        let creatures = vec![Creature {
            id: "1".to_string(),
            name: "a goblin".to_string(),
            noun: Some("goblin".to_string()),
            status: Some("prone".to_string()),
        }];

        dt.update_from_state(&creatures, "1", &config, 30);
        dt.set_border_config(false, None, None);

        let area = Rect::new(0, 0, 30, 1);
        let mut buf = Buffer::empty(area);
        dt.render(area, &mut buf);

        let line = buffer_line(&buf, 0, area.width);
        assert!(line.trim_end().starts_with("a goblin [prn]"));
    }

    #[test]
    fn test_render_uses_noun_when_width_is_limited() {
        let mut dt = Targets::new("Targets");
        let mut config = crate::config::TargetListConfig::default();
        config.truncation_mode = "noun".to_string();

        let creatures = vec![Creature {
            id: "1".to_string(),
            name: "a muddy hog".to_string(),
            noun: Some("hog".to_string()),
            status: Some("stunned".to_string()),
        }];

        dt.update_from_state(&creatures, "1", &config, 12);
        dt.set_border_config(false, None, None);

        let area = Rect::new(0, 0, 30, 1);
        let mut buf = Buffer::empty(area);
        dt.render(area, &mut buf);

        let line = buffer_line(&buf, 0, area.width);
        assert!(line.trim_end().starts_with("hog [stu]"));
    }
}
