//! Scrollable popup that lists every configured keybinding.
//!
//! Provides paging/dragging behavior plus columnar rendering so users can
//! quickly audit key combos, differentiate actions vs macros, and pick entries
//! to edit/delete.

use crate::frontend::tui::crossterm_bridge;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Clear, Widget},
};
use std::collections::HashMap;

/// Keybind entry for display in browser
#[derive(Clone)]
pub struct KeybindEntry {
    pub key_combo: String,
    pub action_type: String, // "Action" or "Macro"
    pub action_value: String,
    pub is_global: bool,     // true = from global/, false = from character profile
}

/// Scrollable inventory of current keybinding entries with optional drag handle.
pub struct KeybindBrowser {
    entries: Vec<KeybindEntry>,
    selected_index: usize,
    scroll_offset: usize,
    num_sections: usize, // Number of section headers (for scroll calculation)

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}

impl KeybindBrowser {
    /// Create browser from keybinds with source tracking
    /// global_keybinds: keybinds from global/keybinds.toml
    /// character_keybinds: keybinds from profiles/{char}/keybinds.toml
    pub fn new_with_source(
        global_keybinds: &HashMap<String, crate::config::KeyBindAction>,
        character_keybinds: &HashMap<String, crate::config::KeyBindAction>,
    ) -> Self {
        let mut entries: Vec<KeybindEntry> = Vec::new();

        // Add global keybinds (mark as is_global = true)
        for (key_combo, action) in global_keybinds {
            // Skip if overridden by character keybind
            if character_keybinds.contains_key(key_combo) {
                continue;
            }
            let (action_type, action_value) = Self::format_action(action);
            entries.push(KeybindEntry {
                key_combo: key_combo.clone(),
                action_type,
                action_value,
                is_global: true,
            });
        }

        // Add character keybinds (mark as is_global = false)
        for (key_combo, action) in character_keybinds {
            let (action_type, action_value) = Self::format_action(action);
            entries.push(KeybindEntry {
                key_combo: key_combo.clone(),
                action_type,
                action_value,
                is_global: false,
            });
        }

        Self::from_entries(entries)
    }

    /// Legacy constructor - treats all keybinds as character-specific
    pub fn new(keybinds: &HashMap<String, crate::config::KeyBindAction>) -> Self {
        let entries: Vec<KeybindEntry> = keybinds
            .iter()
            .map(|(key_combo, action)| {
                let (action_type, action_value) = Self::format_action(action);
                KeybindEntry {
                    key_combo: key_combo.clone(),
                    action_type,
                    action_value,
                    is_global: false, // Legacy: assume character-specific
                }
            })
            .collect();

        Self::from_entries(entries)
    }

    fn format_action(action: &crate::config::KeyBindAction) -> (String, String) {
        match action {
            crate::config::KeyBindAction::Action(a) => ("Action".to_string(), a.clone()),
            crate::config::KeyBindAction::Macro(m) => {
                // Escape control characters for display
                let escaped = m
                    .macro_text
                    .replace('\r', "\\r")
                    .replace('\n', "\\n")
                    .replace('\t', "\\t");
                ("Macro".to_string(), escaped)
            }
        }
    }

    fn from_entries(mut entries: Vec<KeybindEntry>) -> Self {

        // Sort by action type (Actions first, then Macros), then by key combo
        entries.sort_by(|a, b| {
            a.action_type
                .cmp(&b.action_type)
                .then_with(|| a.key_combo.cmp(&b.key_combo))
        });

        // Count sections (how many unique action types)
        let mut num_sections = 0;
        let mut last_type: Option<&str> = None;
        for entry in &entries {
            if last_type != Some(entry.action_type.as_str()) {
                num_sections += 1;
                last_type = Some(&entry.action_type);
            }
        }

        Self {
            entries,
            selected_index: 0,
            scroll_offset: 0,
            num_sections,
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn navigate_up(&mut self) {
        if !self.entries.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
        }
    }

    pub fn navigate_down(&mut self) {
        if self.selected_index + 1 < self.entries.len() {
            self.selected_index += 1;
            self.adjust_scroll();
        }
    }

    pub fn page_up(&mut self) {
        if self.selected_index >= 10 {
            self.selected_index -= 10;
        } else {
            self.selected_index = 0;
        }
        self.adjust_scroll();
    }

    pub fn page_down(&mut self) {
        if self.selected_index + 10 < self.entries.len() {
            self.selected_index += 10;
        } else if !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        }
        self.adjust_scroll();
    }

    fn adjust_scroll(&mut self) {
        // Track display rows (including section headers)
        let mut total_display_rows = 0;
        let mut last_section: Option<&str> = None;
        let mut selected_display_row = 0;

        for (idx, entry) in self.entries.iter().enumerate() {
            let entry_section = &entry.action_type;

            // Add section header if needed
            if last_section != Some(entry_section) {
                total_display_rows += 1;
                last_section = Some(entry_section);
            }

            if idx == self.selected_index {
                selected_display_row = total_display_rows;
            }

            total_display_rows += 1;
        }

        let visible_rows = 15; // One less than list_height for sticky headers

        if selected_display_row < self.scroll_offset {
            self.scroll_offset = selected_display_row;
        } else if selected_display_row >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected_display_row.saturating_sub(visible_rows - 1);
        }
    }

    pub fn get_selected(&self) -> Option<String> {
        self.entries
            .get(self.selected_index)
            .map(|e| e.key_combo.clone())
    }

    /// Get the selected entry (full data)
    pub fn get_selected_entry(&self) -> Option<&KeybindEntry> {
        self.entries.get(self.selected_index)
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(
        &mut self,
        mouse_col: u16,
        mouse_row: u16,
        mouse_down: bool,
        _area: Rect,
    ) -> bool {
        let popup_width = 70;

        // Check if mouse is on title bar
        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > self.popup_x
            && mouse_col < self.popup_x + popup_width - 1;

        if mouse_down && on_title_bar && !self.is_dragging {
            // Start dragging
            self.is_dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
            return true;
        }

        if self.is_dragging {
            if mouse_down {
                // Continue dragging
                self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
                self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);
                return true;
            } else {
                // Stop dragging
                self.is_dragging = false;
                return true;
            }
        }

        false
    }

    pub fn render(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        _config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        let width = 70;
        let height = 20;

        // Center on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(width)) / 2;
            self.popup_y = (area.height.saturating_sub(height)) / 2;
        }

        let x = self.popup_x;
        let y = self.popup_y;

        // Clear the popup area to prevent bleed-through
        let popup_area = Rect {
            x,
            y,
            width,
            height,
        };
        Clear.render(popup_area, buf);

        // Draw background
        for row in 0..height {
            for col in 0..width {
                if x + col < area.width && y + row < area.height {
                    buf[(x + col, y + row)].set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
        }

        // Draw border
        self.draw_border(x, y, width, height, buf, theme);

        // Title (left-aligned on top border)
        let title = format!(" Keybinds ({}) ", self.entries.len());
        for (i, ch) in title.chars().enumerate() {
            if (x + 1 + i as u16) < (x + width) {
                buf[(x + 1 + i as u16, y)]
                    .set_char(ch)
                    .set_fg(crossterm_bridge::to_ratatui_color(theme.browser_item_normal))
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
            }
        }

        // Footer (off border at row 18)
        let footer = "↑/↓:Nav PgUp/PgDn:Page Enter:Edit Del:Remove Esc:Close";
        let footer_y = y + 18;
        let footer_x = x + 2;
        for (i, ch) in footer.chars().enumerate() {
            if (footer_x + i as u16) < (x + width - 2) {
                buf[(footer_x + i as u16, footer_y)]
                    .set_char(ch)
                    .set_fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
            }
        }

        // Render entries with sticky headers
        if self.entries.is_empty() {
            let msg = "No keybinds configured";
            let msg_x = x + (width.saturating_sub(msg.len() as u16)) / 2;
            let msg_y = y + 10;
            for (i, ch) in msg.chars().enumerate() {
                buf[(msg_x + i as u16, msg_y)]
                    .set_char(ch)
                    .set_fg(crossterm_bridge::to_ratatui_color(theme.text_disabled))
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
            }
            return;
        }

        let list_y = y + 1;
        let list_height = 16; // height 20 - 4 (borders + footer)
        let mut last_section: Option<&str> = None;
        let mut last_rendered_section: Option<&str> = None;
        let mut display_row = 0;
        let mut render_row = 0;
        let visible_start = self.scroll_offset;
        let visible_end = visible_start + list_height;

        for (idx, entry) in self.entries.iter().enumerate() {
            let entry_section = &entry.action_type;

            // Check if we need a section header
            if last_section != Some(entry_section) {
                // Always increment display_row for the header
                if display_row >= visible_start {
                    // Render header if in visible range AND we have room
                    if display_row < visible_end && render_row < list_height {
                        let current_y = list_y + render_row as u16;
                        let header_text = if entry_section == "Action" {
                            " ═══ ACTIONS ═══"
                        } else {
                            " ═══ MACROS ═══"
                        };
                        let header_style = Style::default()
                            .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                            .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                            .add_modifier(Modifier::BOLD);
                        for (i, ch) in header_text.chars().enumerate() {
                            if (x + 1 + i as u16) < (x + width - 1) {
                                buf[(x + 1 + i as u16, current_y)]
                                    .set_char(ch)
                                    .set_style(header_style);
                            }
                        }
                        render_row += 1;
                        last_rendered_section = Some(entry_section);
                    }
                }
                display_row += 1;
                last_section = Some(entry_section);
            }

            // Skip if before visible range
            if display_row < visible_start {
                display_row += 1;
                continue;
            }

            // If this is a new section in the visible area and we haven't rendered its header yet (sticky header)
            if last_rendered_section != Some(entry_section) && render_row < list_height {
                let current_y = list_y + render_row as u16;
                let header_text = if entry_section == "Action" {
                    " ═══ ACTIONS ═══"
                } else {
                    " ═══ MACROS ═══"
                };
                let header_style = Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                    .add_modifier(Modifier::BOLD);
                for (i, ch) in header_text.chars().enumerate() {
                    if (x + 1 + i as u16) < (x + width - 1) {
                        buf[(x + 1 + i as u16, current_y)]
                            .set_char(ch)
                            .set_style(header_style);
                    }
                }
                render_row += 1;
                last_rendered_section = Some(entry_section);
            }

            // Stop if past visible range OR no room for entry
            if display_row >= visible_end || render_row >= list_height {
                break;
            }

            let is_selected = idx == self.selected_index;
            let current_y = list_y + render_row as u16;

            // Format as 4 columns: Scope (4) | Key (17 chars) | Type (10 chars) | Value (remaining)
            let scope_width = 4;  // "[G] " or "[C] "
            let key_width = 17;
            let type_width = 10;
            let value_start = scope_width + key_width + type_width;
            let value_width = (width as usize).saturating_sub(value_start + 4); // -4 for borders and padding

            // Scope indicator [G] or [C]
            let scope_text = if entry.is_global { "[G] " } else { "[C] " };

            // Truncate or pad key combo
            let key_text = if entry.key_combo.len() > key_width {
                format!("{}...", &entry.key_combo[..key_width.saturating_sub(3)])
            } else {
                format!("{:<width$}", entry.key_combo, width = key_width)
            };

            // Type column (Action/Macro)
            let type_text = format!("{:<width$}", entry.action_type, width = type_width);

            // Truncate value if needed
            let value_text = if entry.action_value.len() > value_width {
                format!(
                    "{}...",
                    &entry.action_value[..value_width.saturating_sub(3)]
                )
            } else {
                entry.action_value.clone()
            };

            let entry_color = crossterm_bridge::to_ratatui_color(if is_selected  {
                theme.browser_item_focused
            } else {
                theme.browser_item_normal
            });

            // Scope indicator color (dimmer for global)
            let scope_color = crossterm_bridge::to_ratatui_color(if is_selected {
                theme.browser_item_focused
            } else if entry.is_global {
                theme.text_disabled
            } else {
                theme.browser_item_normal
            });

            // Render scope column [G]/[C]
            let scope_x = x + 2;
            for (i, ch) in scope_text.chars().enumerate() {
                if (scope_x + i as u16) < (x + width - 1) {
                    buf[(scope_x + i as u16, current_y)]
                        .set_char(ch)
                        .set_fg(scope_color)
                        .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }

            // Render key combo column
            let key_x = scope_x + scope_width as u16;
            for (i, ch) in key_text.chars().enumerate() {
                if (key_x + i as u16) < (x + width - 1) {
                    buf[(key_x + i as u16, current_y)]
                        .set_char(ch)
                        .set_fg(entry_color)
                        .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }

            // Render type column
            let type_x = key_x + key_width as u16;
            for (i, ch) in type_text.chars().enumerate() {
                if (type_x + i as u16) < (x + width - 1) {
                    buf[(type_x + i as u16, current_y)]
                        .set_char(ch)
                        .set_fg(entry_color)
                        .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }

            // Render value column
            let value_x = type_x + type_width as u16;
            for (i, ch) in value_text.chars().enumerate() {
                if (value_x + i as u16) < (x + width - 1) {
                    buf[(value_x + i as u16, current_y)]
                        .set_char(ch)
                        .set_fg(entry_color)
                        .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }

            display_row += 1;
            render_row += 1;
        }
    }

    fn draw_border(
        &self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.browser_border));

        // Top border
        buf[(x, y)].set_char('┌').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y)]
            .set_char('┐')
            .set_style(border_style);

        // Side borders
        for row in 1..height - 1 {
            buf[(x, y + row)].set_char('│').set_style(border_style);
            buf[(x + width - 1, y + row)]
                .set_char('│')
                .set_style(border_style);
        }

        // Bottom border
        buf[(x, y + height - 1)]
            .set_char('└')
            .set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y + height - 1)]
                .set_char('─')
                .set_style(border_style);
        }
        buf[(x + width - 1, y + height - 1)]
            .set_char('┘')
            .set_style(border_style);
    }

    /// Move to next page (alias for page_down)
    pub fn next_page(&mut self) {
        self.page_down();
    }

    /// Move to previous page (alias for page_up)
    pub fn previous_page(&mut self) {
        self.page_up();
    }

    /// Toggle the filter state
    pub fn toggle_filter(&mut self) {
        // Placeholder for filter toggle functionality
        // Would need to be implemented based on browser's filter requirements
    }

    /// Update the list of keybind entries with source tracking
    ///
    /// # Arguments
    /// * `global_keybinds` - Keybinds from global/keybinds.toml
    /// * `character_keybinds` - Keybinds from profiles/{char}/keybinds.toml (overrides global)
    pub fn update_items_with_source(
        &mut self,
        global_keybinds: &std::collections::HashMap<String, crate::config::KeyBindAction>,
        character_keybinds: &std::collections::HashMap<String, crate::config::KeyBindAction>,
    ) {
        self.entries.clear();

        // Add global keybinds first (will be shown as [G])
        for (key, action) in global_keybinds {
            // Skip if overridden by character keybind
            if character_keybinds.contains_key(key) {
                continue;
            }
            self.entries.push(KeybindEntry {
                key_combo: key.clone(),
                action_type: action.type_name().to_string(),
                action_value: action.display_value(),
                is_global: true,
            });
        }

        // Add character keybinds (will be shown as [C])
        for (key, action) in character_keybinds {
            self.entries.push(KeybindEntry {
                key_combo: key.clone(),
                action_type: action.type_name().to_string(),
                action_value: action.display_value(),
                is_global: false,
            });
        }

        // Sort by key combo
        self.entries.sort_by(|a, b| a.key_combo.cmp(&b.key_combo));

        // Reset selection if out of bounds
        if self.selected_index >= self.entries.len() && !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        }
    }

    /// Update the list of keybind entries (legacy - all marked as global)
    ///
    /// Prefer `update_items_with_source` for proper [G]/[C] indicators.
    #[allow(dead_code)]
    pub fn update_items(&mut self, keybinds: &std::collections::HashMap<String, crate::config::KeyBindAction>) {
        self.entries.clear();
        for (key, action) in keybinds {
            self.entries.push(KeybindEntry {
                key_combo: key.clone(),
                action_type: action.type_name().to_string(),
                action_value: action.display_value(),
                is_global: true, // Default to global when source unknown
            });
        }
        // Sort by key combo
        self.entries.sort_by(|a, b| a.key_combo.cmp(&b.key_combo));

        // Reset selection if out of bounds
        if self.selected_index >= self.entries.len() && !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        }
    }
}

// Trait implementations for KeybindBrowser
use super::widget_traits::{Navigable, Selectable};

impl Navigable for KeybindBrowser {
    fn navigate_up(&mut self) {
        self.navigate_up();
    }

    fn navigate_down(&mut self) {
        self.navigate_down();
    }

    fn page_up(&mut self) {
        self.page_up();
    }

    fn page_down(&mut self) {
        self.page_down();
    }
}

impl Selectable for KeybindBrowser {
    fn get_selected(&self) -> Option<String> {
        self.entries
            .get(self.selected_index)
            .map(|e| e.key_combo.clone())
    }

    fn delete_selected(&mut self) -> Option<String> {
        let combo = self.get_selected()?;
        self.entries.retain(|e| e.key_combo != combo);
        if self.selected_index >= self.entries.len() && self.selected_index > 0 {
            self.selected_index = self.entries.len() - 1;
        }
        self.adjust_scroll();
        Some(combo)
    }
}
