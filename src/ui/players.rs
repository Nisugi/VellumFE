use ratatui::{buffer::Buffer, layout::Rect};
use super::scrollable_container::ScrollableContainer;

/// Widget for displaying players in the room using ScrollableContainer
/// Title shows "Players [XX]" and items show individual player names
pub struct Players {
    container: ScrollableContainer,
    count: u32,
}

impl Players {
    pub fn new(title: impl Into<String>) -> Self {
        let mut container = ScrollableContainer::new(&title.into());
        // Configure for player display
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
        let title = format!("Players [{:02}]", self.count);
        self.container.set_title(title);
    }

    /// Parse and update players from formatted text
    /// Format: "<b>[sit] Player1</b>, <b>Player2</b>, <b>[kne] Deddalus</b>"
    pub fn set_players_from_text(&mut self, text: &str) {
        self.container.clear();

        if text.trim().is_empty() {
            self.count = 0;
            self.update_title();
            return;
        }

        // Parse the player list (comma-separated)
        let parts: Vec<&str> = text.split(',').collect();
        let mut player_index = 0;

        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            let mut status_suffix = None;

            // Remove any XML tags
            let mut clean_text = part.to_string();
            clean_text = clean_text.replace("<b>", "");
            clean_text = clean_text.replace("</b>", "");
            clean_text = clean_text.replace("<color ul='true'>", "");
            clean_text = clean_text.replace("</color>", "");
            clean_text = clean_text.trim().to_string();

            // Check for status prefix [xxx]
            let player_name = if clean_text.starts_with('[') {
                if let Some(end_bracket) = clean_text.find(']') {
                    let status = &clean_text[1..end_bracket];
                    status_suffix = Some(format!("[{}]", status));
                    clean_text[end_bracket + 1..].trim().to_string()
                } else {
                    clean_text
                }
            } else {
                clean_text
            };

            if !player_name.is_empty() {
                // Use a unique ID for each player (index-based since names can repeat)
                let id = format!("player_{}", player_index);

                // Add item with value=0 to hide progress bar, just show text
                self.container.add_or_update_item_full(
                    id,
                    player_name,
                    None,
                    0,  // value
                    1,  // max
                    status_suffix, // Show status on the right
                    None, // no color override for players
                );

                player_index += 1;
            }
        }

        self.count = player_index;
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
