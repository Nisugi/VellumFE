use super::*;

impl TuiFrontend {
    pub fn mouse_to_text_coords(
        &self,
        window_name: &str,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: ratatui::layout::Rect,
    ) -> Option<(usize, usize)> {
        let text_window = self.widget_manager.text_windows.get(window_name)?;
        text_window.mouse_to_text_coords(mouse_col, mouse_row, window_rect)
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

    /// Extract selected text from a text window
    pub fn extract_selection_text(
        &self,
        window_name: &str,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Option<String> {
        let text_window = self.widget_manager.text_windows.get(window_name)?;
        Some(text_window.extract_selection_text(start_line, start_col, end_line, end_col))
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

