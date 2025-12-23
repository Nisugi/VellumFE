//! Target list widget similar to the players view but with current-target cues.
//!
//! Parses the `<target>` style text from the XML stream and highlights whichever
//! entry is currently underlined.

use super::scrollable_container::ScrollableContainer;
use ratatui::{buffer::Buffer, layout::Rect};

pub struct Targets {
    container: ScrollableContainer,
    count: u32,
    base_title: String,
    count_override: Option<String>,
}

impl Targets {
    pub fn new(title: &str) -> Self {
        let mut container = ScrollableContainer::new(title);
        // Targets widget hides values and percentages by default
        container.set_display_options(false, false);

        Self {
            container,
            count: 0,
            base_title: title.to_string(),
            count_override: None,
        }
    }

    /// Parse targets from formatted game text
    /// Format: "[stu] goblin, <b>[sit] troll</b>, <color ul='true'><b>bandit</b></color>"
    pub fn set_targets_from_text(&mut self, text: &str) {
        self.container.clear();
        self.count = 0;

        if text.is_empty() {
            self.update_title();
            return;
        }

        // Split by comma to get individual targets
        let targets: Vec<&str> = text.split(',').map(|s| s.trim()).collect();

        for (idx, target_str) in targets.iter().enumerate() {
            if target_str.is_empty() {
                continue;
            }

            // Check if this is the current target (has ul='true')
            let is_current = target_str.contains("ul='true'") || target_str.contains("ul=\"true\"");

            // Extract status prefix [xxx] if present
            let status = if let Some(start) = target_str.find('[') {
                if let Some(end) = target_str.find(']') {
                    if end > start {
                        Some(target_str[start..=end].to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Strip all XML tags to get clean target name
            let clean_name = Self::strip_xml_tags(target_str);

            // Remove status prefix from the name if it was at the start
            let clean_name = if let Some(ref s) = status {
                clean_name.trim_start_matches(s).trim().to_string()
            } else {
                clean_name
            };

            if clean_name.is_empty() {
                continue;
            }

            // Add prefix for current target
            let display_name = if is_current {
                format!("► {}", clean_name)
            } else {
                clean_name
            };

            // Create unique ID
            let id = format!("target_{}", idx);

            // Add to container with status as suffix
            self.container.add_or_update_item_full(
                id,
                display_name,
                None,   // no alternate text
                0,      // value (hidden)
                1,      // max (hidden)
                status, // suffix (status like "[sit]")
                None,   // no color override
                None,
            );

            self.count += 1;
        }

        self.update_title();
    }

    /// Strip all XML tags from a string
    fn strip_xml_tags(input: &str) -> String {
        let mut result = String::new();
        let mut in_tag = false;

        for ch in input.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ => {
                    if !in_tag {
                        result.push(ch);
                    }
                }
            }
        }

        result.trim().to_string()
    }

    fn update_title(&mut self) {
        if let Some(ref override_count) = self.count_override {
            let title = if self.base_title.is_empty() {
                String::new()
            } else {
                format!("{} {}", self.base_title, override_count)
            };
            self.container.set_title(title);
        } else if self.base_title.is_empty() {
            self.container.set_title(String::new());
        } else {
            let title = format!("{} [{:02}]", self.base_title, self.count);
            self.container.set_title(title);
        }
    }

    pub fn set_title_with_count(&mut self, base_title: &str, count: Option<&str>) {
        self.base_title = base_title.to_string();
        self.count_override = count.map(|c| c.to_string());
        self.update_title();
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
