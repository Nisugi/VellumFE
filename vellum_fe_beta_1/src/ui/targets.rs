use ratatui::{buffer::Buffer, layout::Rect};
use super::scrollable_container::ScrollableContainer;

/// Widget for displaying combat targets using ScrollableContainer
/// Title shows "Targets [XX]" and items show individual targets with status
pub struct Targets {
    container: ScrollableContainer,
    count: u32,
}

impl Targets {
    pub fn new(title: impl Into<String>) -> Self {
        let mut container = ScrollableContainer::new(&title.into());
        // Configure for target display
        container.set_display_options(false, false); // Hide values and percentages

        Self {
            container,
            count: 0,
        }
    }

    pub fn set_count(&mut self, count: u32) {
        self.count = count;
        self.update_title();
    }

    fn update_title(&mut self) {
        let title = format!("Targets [{:02}]", self.count);
        self.container.set_title(title);
    }

    /// Parse and update targets from formatted text
    /// Format: "[stu] goblin, <b>[sit] troll</b>, <color ul='true'><b>bandit</b></color>"
    pub fn set_targets_from_text(&mut self, text: &str) {
        self.container.clear();

        if text.trim().is_empty() {
            self.count = 0;
            self.update_title();
            return;
        }

        // Parse the formatted target list
        let parts: Vec<&str> = text.split(',').collect();
        let mut target_index = 0;

        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            let mut status_suffix = None;
            let is_current_target = part.contains("ul='true'");
            let mut _is_dead = false;

            // Remove all XML tags for parsing
            let mut clean_text = part.to_string();
            clean_text = clean_text.replace("<b>", "");
            clean_text = clean_text.replace("</b>", "");
            clean_text = clean_text.replace("<color ul='true'>", "");
            clean_text = clean_text.replace("</color>", "");
            clean_text = clean_text.trim().to_string();

            // Check for status prefix [xxx]
            let target_name = if clean_text.starts_with('[') {
                if let Some(end_bracket) = clean_text.find(']') {
                    let status = &clean_text[1..end_bracket];
                    status_suffix = Some(format!("[{}]", status));
                    _is_dead = status == "dead";
                    clean_text[end_bracket + 1..].trim().to_string()
                } else {
                    clean_text
                }
            } else {
                clean_text
            };

            if !target_name.is_empty() {
                // Use a unique ID for each target (index-based since names can repeat)
                let id = format!("target_{}", target_index);

                // Add prefix for current target
                let display_name = if is_current_target {
                    format!("► {}", target_name)
                } else {
                    target_name
                };

                // Add item with value=0 to hide progress bar, just show text
                self.container.add_or_update_item_full(
                    id,
                    display_name,
                    None,
                    0,  // value
                    1,  // max
                    status_suffix,
                    None, // no color override for targets
                );

                target_index += 1;
            }
        }

        self.count = target_index;
        self.update_title();
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

    pub fn set_title(&mut self, _title: String) {
        // Title is auto-generated from count
        self.update_title();
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

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.container.render(area, buf);
    }

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.container.render(area, buf);
    }
}
