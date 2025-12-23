//! Wrapper around `ScrollableContainer` for displaying active spell/effect rows.
//!
//! Adds minor formatting (duration strings, color handling) and exposes
//! convenience helpers for toggling alternate text.

use super::scrollable_container::ScrollableContainer;
use ratatui::{buffer::Buffer, layout::Rect};

/// Widget that lists buffs/debuffs for a particular category.
pub struct ActiveEffects {
    container: ScrollableContainer,
    effect_category: String, // "spell", "disease", etc.
}

impl ActiveEffects {
    pub fn new(label: &str, effect_category: String) -> Self {
        let mut container = ScrollableContainer::new(label);
        // ActiveEffects hides values and percentages by default
        container.set_display_options(false, false);

        Self {
            container,
            effect_category,
        }
    }

    /// Format time from "HH:MM:SS" to "[HH:MM]" or "[MM:SS]"
    fn format_duration(time_str: &str) -> String {
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() != 3 {
            return "[??:??]".to_string();
        }

        let hours: u32 = parts[0].parse().unwrap_or(0);
        let minutes: u32 = parts[1].parse().unwrap_or(0);
        let seconds: u32 = parts[2].parse().unwrap_or(0);

        if hours > 0 {
            // Show HH:MM when >= 1 hour
            format!("[{:02}:{:02}]", hours, minutes)
        } else {
            // Show MM:SS when < 1 hour
            format!("[{:02}:{:02}]", minutes, seconds)
        }
    }

    pub fn add_or_update_effect(
        &mut self,
        id: String,
        name: String,
        value: u32,
        time: String,
        bar_color: Option<String>,
        text_color: Option<String>,
    ) {
        let duration_str = Self::format_duration(&time);

        self.container.add_or_update_item_full(
            id.clone(),
            name,
            Some(id), // alternate text is the ID (for toggle)
            value,
            100, // max value for effects
            Some(duration_str),
            bar_color,
            text_color,
            None, // no link data
        );
    }

    pub fn remove_effect(&mut self, id: &str) {
        self.container.remove_item(id);
    }

    pub fn clear(&mut self) {
        self.container.clear();
    }

    pub fn toggle_display(&mut self) {
        self.container.toggle_alternate_text();
    }

    pub fn get_category(&self) -> &str {
        &self.effect_category
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.container.scroll_up(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.container.scroll_down(amount);
    }

    pub fn scroll_position(&self) -> usize {
        self.container.scroll_position()
    }

    pub fn restore_scroll_position(&mut self, offset: usize) {
        self.container.restore_scroll_position(offset);
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

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.container.set_transparent_background(transparent);
    }

    pub fn set_title(&mut self, title: String) {
        self.container.set_title(title);
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.container.set_text_color(color);
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.container.set_background_color(color);
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
