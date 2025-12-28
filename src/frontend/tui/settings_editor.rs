//! In-terminal settings editor that spans categories and value types.
//!
//! Provides table-style navigation, inline editing, and trait-based controls so
//! it matches the ergonomic expectations set by other popups.

use crate::frontend::tui::crossterm_bridge;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Clear, Widget},
};

#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    String(String),
    Number(i64),
    Float(f64),
    Boolean(bool),
    Color(String),
    Enum(String, Vec<String>), // (current_value, all_options)
}

impl SettingValue {
    pub fn to_display_string(&self) -> String {
        match self {
            SettingValue::String(s) => s.clone(),
            SettingValue::Number(n) => n.to_string(),
            SettingValue::Float(f) => format!("{:.2}", f),
            SettingValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
            SettingValue::Color(c) => c.clone(),
            SettingValue::Enum(val, _) => val.clone(),
        }
    }

    pub fn parse_from_string(&self, s: &str) -> Option<SettingValue> {
        match self {
            SettingValue::String(_) => Some(SettingValue::String(s.to_string())),
            SettingValue::Number(_) => s.parse::<i64>().ok().map(SettingValue::Number),
            SettingValue::Float(_) => s.parse::<f64>().ok().map(SettingValue::Float),
            SettingValue::Boolean(_) => match s.to_lowercase().as_str() {
                "true" | "t" | "1" | "yes" | "y" => Some(SettingValue::Boolean(true)),
                "false" | "f" | "0" | "no" | "n" => Some(SettingValue::Boolean(false)),
                _ => None,
            },
            SettingValue::Color(_) => {
                // Basic hex color validation
                if s.starts_with('#') && s.len() == 7 {
                    Some(SettingValue::Color(s.to_string()))
                } else {
                    Some(SettingValue::Color(s.to_string()))
                }
            }
            SettingValue::Enum(_, options) => {
                if options.contains(&s.to_string()) {
                    Some(SettingValue::Enum(s.to_string(), options.clone()))
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingItem {
    pub category: String,
    pub key: String,
    pub display_name: String,
    pub value: SettingValue,
    pub description: Option<String>,
    pub editable: bool,
    pub name_width: Option<u16>, // Custom width for name column
    pub is_global: bool,         // true = from global config, false = character override
}

pub struct SettingsEditor {
    items: Vec<SettingItem>,
    selected_index: usize,
    scroll_offset: usize,
    editing_index: Option<usize>,
    edit_buffer: String,
    category_filter: Option<String>,

    // Popup dragging
    popup_x: u16,
    popup_y: u16,
    pub is_dragging: bool,
    drag_offset_x: u16,
    drag_offset_y: u16,
}

impl SettingsEditor {
    pub fn new(items: Vec<SettingItem>) -> Self {
        Self {
            items,
            selected_index: 0,
            scroll_offset: 0,
            editing_index: None,
            edit_buffer: String::new(),
            category_filter: None,
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn set_category_filter(&mut self, category: Option<String>) {
        self.category_filter = category;
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    fn filtered_items(&self) -> Vec<(usize, &SettingItem)> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if let Some(ref filter) = self.category_filter {
                    &item.category == filter
                } else {
                    true
                }
            })
            .collect()
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
        let filtered = self.filtered_items();
        if self.selected_index + 10 < filtered.len() {
            self.selected_index += 10;
        } else if !filtered.is_empty() {
            self.selected_index = filtered.len() - 1;
        }
        self.adjust_scroll();
    }

    fn adjust_scroll(&mut self) {
        let filtered = self.filtered_items();
        let mut total_display_rows = 0;
        let mut last_category: Option<&str> = None;
        let mut selected_display_row = 0;

        for (idx, (_, item)) in filtered.iter().enumerate() {
            // Add section header row if category changes
            if last_category != Some(item.category.as_str()) {
                total_display_rows += 1;
                last_category = Some(&item.category);
            }

            // Track which display row the selected item is on
            if idx == self.selected_index {
                selected_display_row = total_display_rows;
            }

            total_display_rows += 1;
        }

        let visible_rows = 15;

        // Adjust scroll to keep selected item in view
        if selected_display_row < self.scroll_offset {
            self.scroll_offset = selected_display_row;
        } else if selected_display_row >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected_display_row.saturating_sub(visible_rows - 1);
        }
    }

    pub fn get_selected(&self) -> Option<&SettingItem> {
        let filtered = self.filtered_items();
        filtered.get(self.selected_index).map(|(_, item)| *item)
    }

    pub fn get_selected_mut(&mut self) -> Option<&mut SettingItem> {
        // Find the absolute index without borrowing items
        let selected_idx = self.selected_index;
        let category_filter = self.category_filter.clone();

        let abs_idx = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if let Some(ref filter) = category_filter {
                    &item.category == filter
                } else {
                    true
                }
            })
            .nth(selected_idx)
            .map(|(idx, _)| idx);

        if let Some(idx) = abs_idx {
            self.items.get_mut(idx)
        } else {
            None
        }
    }

    pub fn start_editing(&mut self) {
        // Get item data without borrowing self
        let (editable, is_boolean, value_str, abs_idx) = {
            let filtered = self.filtered_items();
            if let Some((abs_idx, item)) = filtered.get(self.selected_index) {
                (
                    item.editable,
                    matches!(item.value, SettingValue::Boolean(_)),
                    item.value.to_display_string(),
                    *abs_idx,
                )
            } else {
                return;
            }
        };

        if !editable {
            return;
        }

        // Don't enter edit mode for booleans - they toggle directly
        if is_boolean {
            self.toggle_boolean();
            return;
        }

        // Start editing
        self.editing_index = Some(abs_idx);
        self.edit_buffer = value_str;
    }

    pub fn stop_editing(&mut self, save: bool) {
        if let Some(editing_idx) = self.editing_index {
            if save {
                if let Some(item) = self.items.get_mut(editing_idx) {
                    if let Some(new_value) = item.value.parse_from_string(&self.edit_buffer) {
                        item.value = new_value;
                    }
                }
            }
            self.editing_index = None;
            self.edit_buffer.clear();
        }
    }

    pub fn is_editing(&self) -> bool {
        self.editing_index.is_some()
    }

    fn toggle_boolean(&mut self) {
        if let Some(item) = self.get_selected_mut() {
            if let SettingValue::Boolean(ref mut val) = item.value {
                *val = !*val;
            }
        }
    }

    /// Toggle scope between global and character for the selected setting
    /// NOTE: Connection settings CANNOT be toggled - they are always character-specific
    pub fn toggle_scope(&mut self) {
        if let Some(item) = self.get_selected_mut() {
            // Connection settings are ALWAYS character-specific (never allow global)
            if item.key.starts_with("connection.") {
                // Cannot toggle - connection is always character-specific
                return;
            }
            item.is_global = !item.is_global;
        }
    }

    /// Get the scope of the selected setting
    pub fn get_selected_is_global(&self) -> Option<bool> {
        self.get_selected().map(|item| item.is_global)
    }

    /// Get the key and is_global of the selected setting
    pub fn get_selected_key_and_scope(&self) -> Option<(String, bool)> {
        self.get_selected().map(|item| (item.key.clone(), item.is_global))
    }

    /// Get an iterator over all settings items
    pub fn all_items(&self) -> impl Iterator<Item = &SettingItem> {
        self.items.iter()
    }

    /// Apply all setting values from the editor back to a Config
    pub fn apply_to_config(&self, config: &mut crate::config::Config) {
        for item in &self.items {
            match item.key.as_str() {
                // Connection settings
                "connection.host" => {
                    if let SettingValue::String(ref v) = item.value {
                        config.connection.host = v.clone();
                    }
                }
                "connection.port" => {
                    if let SettingValue::Number(v) = item.value {
                        config.connection.port = v as u16;
                    }
                }
                "connection.character" => {
                    if let SettingValue::String(ref v) = item.value {
                        config.connection.character = Some(v.clone());
                    }
                }

                // UI settings
                "ui.buffer_size" => {
                    if let SettingValue::Number(v) = item.value {
                        config.ui.buffer_size = v as usize;
                    }
                }
                "ui.border_style" => {
                    if let SettingValue::Enum(ref v, _) = item.value {
                        config.ui.border_style = v.clone();
                    }
                }
                "ui.countdown_icon" => {
                    if let SettingValue::String(ref v) = item.value {
                        config.ui.countdown_icon = v.clone();
                    }
                }
                "ui.selection_enabled" => {
                    if let SettingValue::Boolean(v) = item.value {
                        config.ui.selection_enabled = v;
                    }
                }
                "ui.selection_respect_window_boundaries" => {
                    if let SettingValue::Boolean(v) = item.value {
                        config.ui.selection_respect_window_boundaries = v;
                    }
                }
                "ui.selection_auto_copy" => {
                    if let SettingValue::Boolean(v) = item.value {
                        config.ui.selection_auto_copy = v;
                    }
                }
                "ui.drag_modifier_key" => {
                    if let SettingValue::Enum(ref v, _) = item.value {
                        config.ui.drag_modifier_key = v.clone();
                    }
                }
                "ui.min_command_length" => {
                    if let SettingValue::Number(v) = item.value {
                        config.ui.min_command_length = v as usize;
                    }
                }

                // Sound settings
                "sound.enabled" => {
                    if let SettingValue::Boolean(v) = item.value {
                        config.sound.enabled = v;
                    }
                }
                "sound.volume" => {
                    if let SettingValue::Float(v) = item.value {
                        config.sound.volume = v as f32;
                    }
                }
                "sound.cooldown_ms" => {
                    if let SettingValue::Number(v) = item.value {
                        config.sound.cooldown_ms = v as u64;
                    }
                }

                // Theme settings
                "active_theme" => {
                    if let SettingValue::String(ref v) = item.value {
                        config.active_theme = v.clone();
                    }
                }

                _ => {}
            }
        }
    }

    fn cycle_enum(&mut self, forward: bool) {
        if let Some(item) = self.get_selected_mut() {
            if let SettingValue::Enum(ref mut current, ref options) = item.value {
                if let Some(current_idx) = options.iter().position(|o| o == current) {
                    let new_idx = if forward {
                        (current_idx + 1) % options.len()
                    } else if current_idx == 0 {
                        options.len() - 1
                    } else {
                        current_idx - 1
                    };
                    *current = options[new_idx].clone();
                }
            }
        }
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> bool {
        if self.is_editing() {
            match key.code {
                KeyCode::Esc => {
                    self.stop_editing(false);
                    true
                }
                KeyCode::Enter => {
                    self.stop_editing(true);
                    true
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Select all
                    true
                }
                KeyCode::Backspace => {
                    self.edit_buffer.pop();
                    true
                }
                KeyCode::Char(c) => {
                    self.edit_buffer.push(c);
                    true
                }
                _ => true,
            }
        } else {
            // Not editing - handle navigation and actions
            match key.code {
                KeyCode::Up => {
                    self.navigate_up();
                    true
                }
                KeyCode::Down => {
                    self.navigate_down();
                    true
                }
                KeyCode::PageUp => {
                    self.page_up();
                    true
                }
                KeyCode::PageDown => {
                    self.page_down();
                    true
                }
                KeyCode::Enter => {
                    self.start_editing();
                    true
                }
                KeyCode::Char(' ') => {
                    // Toggle boolean or start editing enum
                    if let Some(item) = self.get_selected() {
                        if matches!(item.value, SettingValue::Boolean(_)) {
                            self.toggle_boolean();
                            return true;
                        } else if matches!(item.value, SettingValue::Enum(_, _)) {
                            self.cycle_enum(true);
                            return true;
                        }
                    }
                    false
                }
                KeyCode::Left => {
                    // Cycle enum backward
                    if let Some(item) = self.get_selected() {
                        if matches!(item.value, SettingValue::Enum(_, _)) {
                            self.cycle_enum(false);
                            return true;
                        }
                    }
                    false
                }
                KeyCode::Right => {
                    // Cycle enum forward
                    if let Some(item) = self.get_selected() {
                        if matches!(item.value, SettingValue::Enum(_, _)) {
                            self.cycle_enum(true);
                            return true;
                        }
                    }
                    false
                }
                KeyCode::Char('g') => {
                    // Toggle scope (global/character)
                    self.toggle_scope();
                    true
                }
                _ => false,
            }
        }
    }

    pub fn handle_mouse(
        &mut self,
        mouse_col: u16,
        mouse_row: u16,
        mouse_down: bool,
        area: Rect,
    ) -> bool {
        let popup_width = 70.min(area.width);

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
        config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        let textarea_bg = if config.colors.ui.textarea_background == "-" {
            Color::Reset
        } else if let Ok(color) = Self::parse_hex_color(&config.colors.ui.textarea_background) {
            color
        } else {
            Color::Reset
        };
        let popup_width = 70;
        let popup_height = 20;

        // Center on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(popup_width)) / 2;
            self.popup_y = (area.height.saturating_sub(popup_height)) / 2;
        }

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width.min(area.width.saturating_sub(self.popup_x)),
            height: popup_height.min(area.height.saturating_sub(self.popup_y)),
        };

        // Clear the popup area
        Clear.render(popup_area, buf);

        // Draw solid black background
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(' ');
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
        }

        // Draw border
        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label));
        self.draw_border(popup_area, buf, border_style);

        // Draw title
        let title = if let Some(ref filter) = self.category_filter {
            format!(" Settings - {} ", filter)
        } else {
            " Settings ".to_string()
        };
        let title_x = popup_area.x + 2;
        if title_x < popup_area.x + popup_area.width {
            for (i, ch) in title.chars().enumerate() {
                let x = title_x + i as u16;
                if x >= popup_area.x + popup_area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, popup_area.y)) {
                    cell.set_char(ch);
                    cell.set_fg(super::colors::rgb_to_ratatui_color(100, 149, 237));
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
        }

        // Draw help text
        let filtered = self.filtered_items();
        let total = filtered.len();
        let current = if total == 0 {
            0
        } else {
            (self.selected_index + 1).min(total)
        };
        let help = format!(
            " ↑↓:Nav  Enter:Edit  Space:Toggle  g:Scope  ^S:Save  Esc:Close  ({}/{}) ",
            current, total
        );
        let help_x = popup_area.x + popup_area.width.saturating_sub(help.len() as u16 + 1);
        let start_x = if help_x > popup_area.x + 1 {
            help_x
        } else {
            popup_area.x + 1
        };
        let help_y = popup_area.y + popup_area.height.saturating_sub(2);
        if start_x < popup_area.x + popup_area.width && help_y < popup_area.y + popup_area.height {
            for (i, ch) in help.chars().enumerate() {
                let x = start_x + i as u16;
                if x >= popup_area.x + popup_area.width - 1 {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, help_y)) {
                    cell.set_char(ch);
                    cell.set_fg(crossterm_bridge::to_ratatui_color(theme.text_disabled));
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
        }

        // Draw settings list
        let list_area = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 1,
            width: popup_area.width.saturating_sub(4),
            height: popup_area.height.saturating_sub(4),
        };

        if filtered.is_empty() {
            // Show "No settings" message
            let msg = "No settings available";
            let x = list_area.x + (list_area.width.saturating_sub(msg.len() as u16)) / 2;
            let y = list_area.y + list_area.height / 2;
            for (i, ch) in msg.chars().enumerate() {
                if let Some(cell) = buf.cell_mut((x + i as u16, y)) {
                    cell.set_char(ch);
                    cell.set_fg(crossterm_bridge::to_ratatui_color(theme.text_disabled));
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
            return;
        }

        // Track categories for section headers
        let mut last_category: Option<&str> = None;
        let mut last_rendered_category: Option<&str> = None;
        let mut display_row = 0;
        let mut render_row = 0;
        let visible_start = self.scroll_offset;
        let visible_end = visible_start + list_area.height as usize;

        for (rel_idx, (abs_idx, item)) in filtered.iter().enumerate() {
            // Check if we need a category header
            if last_category != Some(item.category.as_str()) {
                // Always increment display_row for the header
                if display_row >= visible_start {
                    // Header is in visible range or we're past it
                    if display_row < visible_end && render_row < list_area.height as usize {
                        // Render the header
                        let y = list_area.y + render_row as u16;
                        let header = format!("═══ {} ═══", item.category.to_uppercase());
                        let header_style = Style::default()
                            .fg(crossterm_bridge::to_ratatui_color(theme.form_label_focused))
                            .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                            .add_modifier(Modifier::BOLD);

                        for (i, ch) in header.chars().enumerate() {
                            let x = list_area.x + i as u16;
                            if x >= list_area.x + list_area.width {
                                break;
                            }
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.set_char(ch);
                                cell.set_style(header_style);
                            }
                        }
                        render_row += 1;
                        last_rendered_category = Some(&item.category);
                    }
                }
                display_row += 1;
                last_category = Some(&item.category);
            }

            // Skip if before visible range
            if display_row < visible_start {
                display_row += 1;
                continue;
            }

            // If this is a new category in the visible area and we haven't rendered its header yet
            if last_rendered_category != Some(item.category.as_str())
                && render_row < list_area.height as usize
            {
                // Render sticky header for this category
                let y = list_area.y + render_row as u16;
                let header = format!("═══ {} ═══", item.category.to_uppercase());
                let header_style = Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.form_label_focused))
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                    .add_modifier(Modifier::BOLD);

                for (i, ch) in header.chars().enumerate() {
                    let x = list_area.x + i as u16;
                    if x >= list_area.x + list_area.width {
                        break;
                    }
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_char(ch);
                        cell.set_style(header_style);
                    }
                }
                render_row += 1;
                last_rendered_category = Some(&item.category);
            }

            // Stop if past visible range
            if display_row >= visible_end || render_row >= list_area.height as usize {
                break;
            }

            let y = list_area.y + render_row as u16;
            let is_selected = rel_idx == self.selected_index;
            let is_editing = self.editing_index == Some(*abs_idx);

            self.render_setting_item(
                item,
                is_selected,
                is_editing,
                list_area.x,
                y,
                list_area.width,
                buf,
                textarea_bg,
                theme,
            );

            display_row += 1;
            render_row += 1;
        }
    }

    fn render_setting_item(
        &self,
        item: &SettingItem,
        is_selected: bool,
        is_editing: bool,
        x: u16,
        y: u16,
        width: u16,
        buf: &mut Buffer,
        textarea_bg: Color,
        theme: &crate::theme::AppTheme,
    ) {
        // Render [G] or [C] scope indicator first
        let scope_indicator = if item.is_global { "[G]" } else { "[C]" };
        let scope_style = Style::default()
            .fg(if item.is_global {
                super::colors::rgb_to_ratatui_color(100, 200, 100) // Green for global
            } else {
                super::colors::rgb_to_ratatui_color(200, 150, 100) // Orange for character
            })
            .bg(crossterm_bridge::to_ratatui_color(theme.browser_background));

        for (i, ch) in scope_indicator.chars().enumerate() {
            let px = x + i as u16;
            if let Some(cell) = buf.cell_mut((px, y)) {
                cell.set_char(ch);
                cell.set_style(scope_style);
            }
        }

        // Adjust positions for the rest (scope indicator is 4 chars including space)
        let name_x = x + 4; // "[G] " or "[C] "
        let name_width = item.name_width.unwrap_or(25);
        let adjusted_name_width = name_width.saturating_sub(4);
        let value_width = width.saturating_sub(name_width + 3);

        // Render name
        let name_style = if is_selected {
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.form_label_focused))
                .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(super::colors::rgb_to_ratatui_color(100, 149, 237))
                .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
        };

        // Add space after scope indicator
        if let Some(cell) = buf.cell_mut((x + 3, y)) {
            cell.set_char(' ');
            cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        for (i, ch) in item
            .display_name
            .chars()
            .take(adjusted_name_width as usize)
            .enumerate()
        {
            let px = name_x + i as u16;
            if let Some(cell) = buf.cell_mut((px, y)) {
                cell.set_char(ch);
                cell.set_style(name_style);
            }
        }

        // Fill remaining name space
        let rendered_name_len = item.display_name.len().min(adjusted_name_width as usize);
        for i in rendered_name_len..(adjusted_name_width as usize) {
            let px = name_x + i as u16;
            if let Some(cell) = buf.cell_mut((px, y)) {
                cell.set_char(' ');
                cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
            }
        }

        // Render separator
        let sep_x = x + name_width;
        if let Some(cell) = buf.cell_mut((sep_x, y)) {
            cell.set_char(':');
            cell.set_fg(crossterm_bridge::to_ratatui_color(theme.text_disabled));
            cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
        if let Some(cell) = buf.cell_mut((sep_x + 1, y)) {
            cell.set_char(' ');
            cell.set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Render value
        let value_x = sep_x + 2;
        let value_text = if is_editing {
            &self.edit_buffer
        } else {
            &item.value.to_display_string()
        };

        let value_bg = if is_editing {
            textarea_bg
        } else {
            crossterm_bridge::to_ratatui_color(theme.browser_background)
        };
        let value_fg = crossterm_bridge::to_ratatui_color(if is_editing {
            theme.form_label
        } else if is_selected {
            theme.form_label_focused
        } else {
            theme.text_primary
        });

        let value_style = Style::default().fg(value_fg).bg(value_bg);

        for (i, ch) in value_text.chars().take(value_width as usize).enumerate() {
            let px = value_x + i as u16;
            if px >= x + width {
                break;
            }
            if let Some(cell) = buf.cell_mut((px, y)) {
                cell.set_char(ch);
                cell.set_style(value_style);
            }
        }

        // Fill remaining value space
        for i in value_text.len()..(value_width as usize) {
            let px = value_x + i as u16;
            if px >= x + width {
                break;
            }
            if let Some(cell) = buf.cell_mut((px, y)) {
                cell.set_char(' ');
                cell.set_bg(value_bg);
            }
        }

        // Show cursor if editing
        if is_editing {
            let cursor_x = value_x + self.edit_buffer.len() as u16;
            if cursor_x < x + width {
                if let Some(cell) = buf.cell_mut((cursor_x, y)) {
                    cell.set_fg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                    cell.set_bg(crossterm_bridge::to_ratatui_color(theme.text_primary));
                }
            }
        }
    }

    fn draw_border(&self, area: Rect, buf: &mut Buffer, style: Style) {
        // Top border
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                if x == area.x {
                    cell.set_char('┌');
                } else if x == area.x + area.width - 1 {
                    cell.set_char('┐');
                } else {
                    cell.set_char('─');
                }
                cell.set_style(style);
            }
        }

        // Bottom border
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, area.y + area.height - 1)) {
                if x == area.x {
                    cell.set_char('└');
                } else if x == area.x + area.width - 1 {
                    cell.set_char('┘');
                } else {
                    cell.set_char('─');
                }
                cell.set_style(style);
            }
        }

        // Left border
        for y in area.y + 1..area.y + area.height - 1 {
            if let Some(cell) = buf.cell_mut((area.x, y)) {
                cell.set_char('│');
                cell.set_style(style);
            }
        }

        // Right border
        for y in area.y + 1..area.y + area.height - 1 {
            if let Some(cell) = buf.cell_mut((area.x + area.width - 1, y)) {
                cell.set_char('│');
                cell.set_style(style);
            }
        }
    }

    fn parse_hex_color(hex: &str) -> Result<Color, ()> {
        if hex.len() != 7 || !hex.starts_with('#') {
            return Err(());
        }
        let r = u8::from_str_radix(&hex[1..3], 16).map_err(|_| ())?;
        let g = u8::from_str_radix(&hex[3..5], 16).map_err(|_| ())?;
        let b = u8::from_str_radix(&hex[5..7], 16).map_err(|_| ())?;
        Ok(Color::Rgb(r, g, b))
    }

    /// Move to next page (alias for page_down)
    pub fn next_page(&mut self) {
        self.page_down();
    }

    /// Move to previous page (alias for page_up)
    pub fn previous_page(&mut self) {
        self.page_up();
    }
}

// Trait implementations for SettingsEditor
use super::widget_traits::{Cyclable, Navigable, Toggleable};

impl Navigable for SettingsEditor {
    fn navigate_up(&mut self) {
        let filtered = self.filtered_items();
        if !filtered.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
        }
    }

    fn navigate_down(&mut self) {
        let filtered = self.filtered_items();
        if self.selected_index + 1 < filtered.len() {
            self.selected_index += 1;
            self.adjust_scroll();
        }
    }

    fn page_up(&mut self) {
        self.page_up();
    }

    fn page_down(&mut self) {
        self.page_down();
    }
}

impl Toggleable for SettingsEditor {
    fn toggle_focused(&mut self) -> Option<bool> {
        // Check if current item is a Boolean
        if let Some(item) = self.get_selected() {
            if matches!(item.value, SettingValue::Boolean(_)) {
                self.toggle_boolean();
                // Get the new value
                if let Some(item) = self.get_selected() {
                    if let SettingValue::Boolean(val) = item.value {
                        return Some(val);
                    }
                }
            }
        }
        None
    }
}

impl Cyclable for SettingsEditor {
    fn cycle_forward(&mut self) {
        // Check if current item is an Enum
        if let Some(item) = self.get_selected() {
            if matches!(item.value, SettingValue::Enum(_, _)) {
                self.cycle_enum(true);
            }
        }
    }

    fn cycle_backward(&mut self) {
        // Check if current item is an Enum
        if let Some(item) = self.get_selected() {
            if matches!(item.value, SettingValue::Enum(_, _)) {
                self.cycle_enum(false);
            }
        }
    }
}

// Note: TextEditable trait not implemented - SettingsEditor uses a String edit_buffer
// rather than TextArea fields. Clipboard operations are handled internally via handle_input().
