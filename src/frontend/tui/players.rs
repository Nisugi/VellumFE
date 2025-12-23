//! Players list widget using room players component data.
//!
//! Displays players with dual status support (prepended + appended) and clickable names.
//! Uses ListWidget for proper text rendering with clickable links.

use crate::data::LinkData;
use ratatui::{buffer::Buffer, layout::Rect};

pub struct Players {
    widget: super::list_widget::ListWidget,
    count: u32,
    base_title: String,
    /// Generation counter for change detection
    generation: u64,
    /// Cached player IDs for change detection (comma-joined)
    player_ids_cache: String,
}

impl Players {
    pub fn new(title: &str) -> Self {
        Self {
            widget: super::list_widget::ListWidget::new(title),
            count: 0,
            base_title: title.to_string(),
            generation: 0,
            player_ids_cache: String::new(),
        }
    }

    /// Update from room players with dual status support
    /// Returns true if the display changed
    pub fn update_from_state(
        &mut self,
        room_players: &[crate::core::state::Player],
        config: &crate::config::TargetListConfig,
    ) -> bool {
        // Build IDs string for change detection
        let new_ids: String = room_players
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let new_count = room_players.len() as u32;

        // Quick check: if IDs and count match, no change
        if self.player_ids_cache == new_ids && self.count == new_count {
            return false;
        }

        self.player_ids_cache = new_ids;
        self.widget.clear();
        self.count = 0;

        tracing::debug!(
            "Players[{}]::update_from_state - {} players",
            self.base_title,
            room_players.len()
        );

        for player in room_players.iter() {
            // Build status display with abbreviations
            let mut status_parts = Vec::new();

            // Primary status (prepended, e.g., "stunned" from "a stunned Player")
            if let Some(ref primary) = player.primary_status {
                let abbrev = config.status_abbrev
                    .get(&primary.to_lowercase())
                    .cloned()
                    .unwrap_or_else(|| {
                        if primary.len() <= 3 { primary.to_string() }
                        else { primary.chars().take(3).collect() }
                    });
                status_parts.push(format!("[{}]", abbrev));
            }

            // Secondary status (appended, e.g., "prone" from "Player (prone)")
            if let Some(ref secondary) = player.secondary_status {
                let abbrev = config.status_abbrev
                    .get(&secondary.to_lowercase())
                    .cloned()
                    .unwrap_or_else(|| {
                        if secondary.len() <= 3 { secondary.to_string() }
                        else { secondary.chars().take(3).collect() }
                    });
                status_parts.push(format!("[{}]", abbrev));
            }

            // Build display name with status position from config
            let display_name = if status_parts.is_empty() {
                player.name.clone()
            } else if config.status_position == "start" {
                format!("{} {}", status_parts.join(" "), player.name)
            } else {
                // Default: "end"
                format!("{} {}", player.name, status_parts.join(" "))
            };

            // Create clickable link
            let link_data = Some(LinkData {
                exist_id: player.id.clone(),
                noun: player.name.clone(),
                text: player.name.clone(),
                coord: None,
            });

            self.widget.add_simple_line(display_name, None, link_data);
            self.count += 1;
        }

        self.update_title();
        self.generation += 1;
        true
    }

    fn update_title(&mut self) {
        let title = if self.base_title.is_empty() {
            String::new()
        } else {
            format!("{} [{:02}]", self.base_title, self.count)
        };
        self.widget.set_title(title);
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
    /// Returns a LinkData if a player was clicked (can be used for targeting/interacting).
    pub fn handle_click(&self, y: u16, area: Rect) -> Option<LinkData> {
        // Delegate to ListWidget's click handling (x=0 since ListWidget doesn't use it)
        self.widget.handle_click(0, y, area)
    }
}
