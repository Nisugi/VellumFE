use super::*;

impl TuiFrontend {
    /// Convert mouse coordinates to text coordinates for a window.
    /// Works with text windows, tabbed text windows, and other text-containing widgets.
    pub fn mouse_to_text_coords(
        &self,
        window_name: &str,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: ratatui::layout::Rect,
    ) -> Option<(usize, usize)> {
        // Check text windows first
        if let Some(text_window) = self.widget_manager.text_windows.get(window_name) {
            return text_window.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        // Check tabbed text windows (use active tab's text window)
        if let Some(tabbed) = self.widget_manager.tabbed_text_windows.get(window_name) {
            return tabbed.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        // Check room windows
        if let Some(room) = self.widget_manager.room_windows.get(window_name) {
            return room.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        // Check inventory windows
        if let Some(inv) = self.widget_manager.inventory_windows.get(window_name) {
            return inv.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        // Check container windows
        if let Some(container) = self.widget_manager.container_widgets.get(window_name) {
            return container.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        // Check spells windows
        if let Some(spells) = self.widget_manager.spells_windows.get(window_name) {
            return spells.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        // Check active effects windows
        if let Some(effects) = self.widget_manager.active_effects_windows.get(window_name) {
            return effects.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        // Check targets windows
        if let Some(targets) = self.widget_manager.targets_widgets.get(window_name) {
            return targets.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        // Check players windows
        if let Some(players) = self.widget_manager.players_widgets.get(window_name) {
            return players.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        // Check hand widgets
        if let Some(hand) = self.widget_manager.hand_widgets.get(window_name) {
            return hand.mouse_to_text_coords(mouse_col, mouse_row, window_rect);
        }

        None
    }

    /// Handle a tab click for a tabbed text window; returns Some(new_index) if a tab was activated.
    pub fn handle_tabbed_click(
        &mut self,
        window_name: &str,
        window_rect: ratatui::layout::Rect,
        mouse_col: u16,
        mouse_row: u16,
    ) -> Option<usize> {
        if let Some(tabbed_window) = self.widget_manager.tabbed_text_windows.get_mut(window_name) {
            if tabbed_window.handle_mouse_click(window_rect, mouse_col, mouse_row) {
                return Some(tabbed_window.get_active_tab_index());
            }
        }
        None
    }

    /// Extract selected text from any text-supporting window
    pub fn extract_selection_text(
        &self,
        window_name: &str,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Option<String> {
        // Check text windows first
        if let Some(text_window) = self.widget_manager.text_windows.get(window_name) {
            return Some(text_window.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        // Check tabbed text windows
        if let Some(tabbed) = self.widget_manager.tabbed_text_windows.get(window_name) {
            return Some(tabbed.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        // Check room windows
        if let Some(room) = self.widget_manager.room_windows.get(window_name) {
            return Some(room.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        // Check inventory windows
        if let Some(inv) = self.widget_manager.inventory_windows.get(window_name) {
            return Some(inv.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        // Check container windows
        if let Some(container) = self.widget_manager.container_widgets.get(window_name) {
            return Some(container.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        // Check spells windows
        if let Some(spells) = self.widget_manager.spells_windows.get(window_name) {
            return Some(spells.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        // Check active effects windows
        if let Some(effects) = self.widget_manager.active_effects_windows.get(window_name) {
            return Some(effects.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        // Check targets windows
        if let Some(targets) = self.widget_manager.targets_widgets.get(window_name) {
            return Some(targets.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        // Check players windows
        if let Some(players) = self.widget_manager.players_widgets.get(window_name) {
            return Some(players.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        // Check hand widgets
        if let Some(hand) = self.widget_manager.hand_widgets.get(window_name) {
            return Some(hand.extract_selection_text(start_line, start_col, end_line, end_col));
        }

        None
    }

    /// Ensure a command input widget exists (should be called during init)
    pub fn execute_search(
        &mut self,
        window_name: &str,
        pattern: &str,
    ) -> Result<usize, regex::Error> {
        if let Some(text_window) = self.widget_manager.text_windows.get_mut(window_name) {
            // Make search case-insensitive by prepending (?i) unless user already specified flags
            let case_insensitive_pattern = if pattern.starts_with("(?") {
                pattern.to_string()
            } else {
                format!("(?i){}", pattern)
            };
            text_window.start_search(&case_insensitive_pattern)
        } else {
            Ok(0)
        }
    }

    /// Go to next search match
    pub fn next_search_match(&mut self, window_name: &str) -> bool {
        if let Some(text_window) = self.widget_manager.text_windows.get_mut(window_name) {
            text_window.next_match()
        } else {
            false
        }
    }

    /// Go to previous search match
    pub fn prev_search_match(&mut self, window_name: &str) -> bool {
        if let Some(text_window) = self.widget_manager.text_windows.get_mut(window_name) {
            text_window.prev_match()
        } else {
            false
        }
    }

    /// Clear search from all text windows
    pub fn clear_all_searches(&mut self) {
        for text_window in self.widget_manager.text_windows.values_mut() {
            text_window.clear_search();
        }
    }

    /// Get search info from a window (current match, total matches)
    pub fn get_search_info(&self, window_name: &str) -> Option<(usize, usize)> {
        self.widget_manager.text_windows
            .get(window_name)
            .and_then(|tw| tw.search_info())
    }
}

