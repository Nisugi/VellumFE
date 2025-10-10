use ratatui::{buffer::Buffer, layout::Rect};
use super::scrollable_container::ScrollableContainer;

pub struct ActiveEffects {
    container: ScrollableContainer,
    effect_category: String,
}

impl ActiveEffects {
    pub fn new(label: &str, effect_category: String) -> Self {
        Self {
            container: ScrollableContainer::new(label),
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

    /// Add or update an active effect with proper formatting: "Name [XX:XX]"
    /// Stores both spell ID and name so user can toggle between them
    pub fn add_or_update_effect(&mut self, id: String, name: String, value: u32, time: String) {
        let duration = Self::format_duration(&time);
        // Pass name as primary text, ID as alternate text
        self.container.add_or_update_item_full(
            id.clone(),  // key
            name,        // primary text (spell name)
            Some(id),    // alternate text (spell ID)
            value,
            100,
            Some(duration)
        );
    }

    pub fn toggle_display(&mut self) {
        self.container.toggle_alternate_text();
    }

    pub fn remove_effect(&mut self, id: &str) {
        self.container.remove_item(id);
    }

    pub fn clear(&mut self) {
        self.container.clear();
    }

    pub fn set_border_config(
        &mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) {
        self.container.set_border_config(show_border, border_style, border_color);
    }

    pub fn set_border_sides(&mut self, border_sides: Option<Vec<String>>) {
        self.container.set_border_sides(border_sides);
    }

    pub fn set_title(&mut self, title: String) {
        self.container.set_title(title);
    }

    pub fn set_bar_color(&mut self, color: String) {
        self.container.set_bar_color(color);
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.container.set_transparent_background(transparent);
    }

    pub fn set_visible_count(&mut self, count: Option<usize>) {
        self.container.set_visible_count(count);
    }

    pub fn scroll_up(&mut self) {
        self.container.scroll_up();
    }

    pub fn scroll_down(&mut self) {
        self.container.scroll_down();
    }

    pub fn get_category(&self) -> &str {
        &self.effect_category
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.container.render(area, buf);
    }

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        self.container.render_with_focus(area, buf, focused);
    }
}
