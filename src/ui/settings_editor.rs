use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Clear, Widget},
};

#[derive(Debug, Clone)]
pub enum SettingValue {
    String(String),
    Number(i64),
    Float(f64),
    Boolean(bool),
    Color(String),  // Hex color
    Enum(String, Vec<String>),  // (current, options)
}

impl SettingValue {
    pub fn to_display_string(&self) -> String {
        match self {
            SettingValue::String(s) => s.clone(),
            SettingValue::Number(n) => n.to_string(),
            SettingValue::Float(f) => format!("{:.2}", f),
            SettingValue::Boolean(b) => if *b { "true".to_string() } else { "false".to_string() },
            SettingValue::Color(c) => c.clone(),
            SettingValue::Enum(current, _) => current.clone(),
        }
    }

    pub fn to_config_string(&self) -> String {
        match self {
            SettingValue::String(s) => format!("\"{}\"", s),
            SettingValue::Number(n) => n.to_string(),
            SettingValue::Float(f) => f.to_string(),
            SettingValue::Boolean(b) => b.to_string(),
            SettingValue::Color(c) => format!("\"{}\"", c),
            SettingValue::Enum(current, _) => format!("\"{}\"", current),
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
    pub editable: bool,  // Some settings might be read-only
    pub name_width: Option<u16>,  // Custom width for name column (None = auto)
}

pub struct SettingsEditor {
    items: Vec<SettingItem>,
    selected_index: usize,
    scroll_offset: usize,
    editing_index: Option<usize>,
    edit_buffer: String,
    category_filter: Option<String>,

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}

impl SettingsEditor {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
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

    pub fn with_items(items: Vec<SettingItem>) -> Self {
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

    pub fn add_item(&mut self, item: SettingItem) {
        self.items.push(item);
    }

    fn filtered_items(&self) -> Vec<(usize, &SettingItem)> {
        if let Some(ref filter) = self.category_filter {
            self.items
                .iter()
                .enumerate()
                .filter(|(_, item)| &item.category == filter)
                .collect()
        } else {
            self.items.iter().enumerate().collect()
        }
    }

    pub fn previous(&mut self) {
        if !self.is_editing() {
            let filtered = self.filtered_items();
            if !filtered.is_empty() && self.selected_index > 0 {
                self.selected_index -= 1;
                self.adjust_scroll();
            }
        }
    }

    pub fn next(&mut self) {
        if !self.is_editing() {
            let filtered = self.filtered_items();
            if self.selected_index + 1 < filtered.len() {
                self.selected_index += 1;
                self.adjust_scroll();
            }
        }
    }

    pub fn page_up(&mut self) {
        if !self.is_editing() {
            let visible_height: usize = 16; // 70x20 window: 20 - 4 (borders + footer) = 16
            let jump = visible_height.saturating_sub(1).max(1);
            self.selected_index = self.selected_index.saturating_sub(jump);
            self.adjust_scroll();
        }
    }

    pub fn page_down(&mut self) {
        if !self.is_editing() {
            let filtered = self.filtered_items();
            let visible_height: usize = 16; // 70x20 window: 20 - 4 (borders + footer) = 16
            let jump = visible_height.saturating_sub(1).max(1);
            self.selected_index = (self.selected_index + jump).min(filtered.len().saturating_sub(1));
            self.adjust_scroll();
        }
    }

    fn adjust_scroll(&mut self) {
        // Calculate total display rows including category headers
        let filtered = self.filtered_items();
        let mut total_display_rows = 0;
        let mut last_category: Option<&str> = None;
        let mut selected_display_row = 0;

        for (idx, (_original_idx, item)) in filtered.iter().enumerate() {
            // Add category header row if category changes
            if last_category.as_deref() != Some(&item.category) {
                total_display_rows += 1;
                last_category = Some(&item.category);
            }

            // Track which display row the selected item is on
            if idx == self.selected_index {
                selected_display_row = total_display_rows;
            }

            total_display_rows += 1;
        }

        let visible_rows = 16; // 70x20 window: 20 - 4 (borders + footer) = 16

        // Adjust scroll to keep selected item in view
        if selected_display_row < self.scroll_offset {
            self.scroll_offset = selected_display_row;
        } else if selected_display_row >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected_display_row.saturating_sub(visible_rows - 1);
        }
    }

    pub fn is_editing(&self) -> bool {
        self.editing_index.is_some()
    }

    pub fn start_edit(&mut self) {
        let filtered = self.filtered_items();
        if let Some((original_idx, item)) = filtered.get(self.selected_index) {
            if item.editable {
                // For booleans, toggle immediately instead of entering edit mode
                if matches!(item.value, SettingValue::Boolean(_)) {
                    return; // Don't enter edit mode for booleans
                }

                let idx = *original_idx;
                let buffer = item.value.to_display_string();
                self.editing_index = Some(idx);
                self.edit_buffer = buffer;
            }
        }
    }

    /// Toggle a boolean setting (updates the value and returns the index and new value)
    pub fn toggle_boolean(&mut self) -> Option<(usize, bool)> {
        let filtered = self.filtered_items();
        if let Some((original_idx, _item)) = filtered.get(self.selected_index) {
            let idx = *original_idx;
            if let Some(item) = self.items.get_mut(idx) {
                if item.editable {
                    if let SettingValue::Boolean(current) = item.value {
                        let new_value = !current;
                        item.value = SettingValue::Boolean(new_value);
                        return Some((idx, new_value));
                    }
                }
            }
        }
        None
    }

    /// Cycle enum to next value (updates the value and returns the index and new value)
    pub fn cycle_enum_next(&mut self) -> Option<(usize, String)> {
        let filtered = self.filtered_items();
        if let Some((original_idx, _item)) = filtered.get(self.selected_index) {
            let idx = *original_idx;
            if let Some(item) = self.items.get_mut(idx) {
                if item.editable {
                    if let SettingValue::Enum(current, options) = &item.value {
                        if let Some(current_idx) = options.iter().position(|o| o == current) {
                            // Don't wrap around - stop at last option
                            let next_idx = (current_idx + 1).min(options.len() - 1);
                            let new_value = options[next_idx].clone();
                            item.value = SettingValue::Enum(new_value.clone(), options.clone());
                            return Some((idx, new_value));
                        }
                    }
                }
            }
        }
        None
    }

    /// Cycle enum to previous value (updates the value and returns the index and new value)
    pub fn cycle_enum_prev(&mut self) -> Option<(usize, String)> {
        let filtered = self.filtered_items();
        if let Some((original_idx, _item)) = filtered.get(self.selected_index) {
            let idx = *original_idx;
            if let Some(item) = self.items.get_mut(idx) {
                if item.editable {
                    if let SettingValue::Enum(current, options) = &item.value {
                        if let Some(current_idx) = options.iter().position(|o| o == current) {
                            // Don't wrap around - stop at first option (use saturating_sub)
                            let prev_idx = current_idx.saturating_sub(1);
                            let new_value = options[prev_idx].clone();
                            item.value = SettingValue::Enum(new_value.clone(), options.clone());
                            return Some((idx, new_value));
                        }
                    }
                }
            }
        }
        None
    }

    /// Check if the currently selected item is an Enum
    pub fn is_selected_enum(&self) -> bool {
        let filtered = self.filtered_items();
        if let Some((_original_idx, item)) = filtered.get(self.selected_index) {
            matches!(item.value, SettingValue::Enum(_, _))
        } else {
            false
        }
    }

    pub fn cancel_edit(&mut self) {
        self.editing_index = None;
        self.edit_buffer.clear();
    }

    pub fn finish_edit(&mut self) -> Option<(usize, String)> {
        if let Some(idx) = self.editing_index {
            let new_value = self.edit_buffer.clone();
            self.editing_index = None;
            self.edit_buffer.clear();
            return Some((idx, new_value));
        }
        None
    }

    pub fn handle_edit_input(&mut self, c: char) {
        if self.is_editing() {
            self.edit_buffer.push(c);
        }
    }

    pub fn handle_edit_backspace(&mut self) {
        if self.is_editing() {
            self.edit_buffer.pop();
        }
    }

    pub fn get_item(&self, idx: usize) -> Option<&SettingItem> {
        self.items.get(idx)
    }

    pub fn get_item_mut(&mut self, idx: usize) -> Option<&mut SettingItem> {
        self.items.get_mut(idx)
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn set_selected_index(&mut self, index: usize) {
        self.selected_index = index;
    }

    pub fn get_scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    /// Handle mouse events for dragging the popup
    /// Returns true if the mouse event was handled
    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> bool {
        let popup_width = 70.min(area.width);
        let _popup_height = 25.min(area.height);

        // Check if mouse is on title bar (top border, excluding corners)
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
                // Continue dragging - update position
                self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
                self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);
                return true;
            } else {
                // Release - stop dragging
                self.is_dragging = false;
                return true;
            }
        }

        false
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, config: &crate::config::Config) {
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
        let popup_area = Rect { x, y, width, height };
        Clear.render(popup_area, buf);

        // Draw black background
        for row in 0..height {
            for col in 0..width {
                if x + col < area.width && y + row < area.height {
                    buf[(x + col, y + row)].set_bg(Color::Black);
                }
            }
        }

        // Draw cyan border
        self.draw_border(x, y, width, height, buf);

        // Title (left-aligned on top border)
        let title = " Settings Editor ";
        for (i, ch) in title.chars().enumerate() {
            if (x + 1 + i as u16) < (x + width) {
                buf[(x + 1 + i as u16, y)].set_char(ch).set_fg(Color::Cyan).set_bg(Color::Black);
            }
        }

        // Footer (off border at row 18)
        let instructions = if self.is_editing() {
            "Ctrl+S:Save Esc:Cancel"
        } else {
            "↑/↓:Nav/Cycle Tab:Nav Enter:Edit/Toggle Esc:Close"
        };
        let footer_y = y + 18;
        let footer_x = x + 2;
        for (i, ch) in instructions.chars().enumerate() {
            if (footer_x + i as u16) < (x + width - 2) {
                buf[(footer_x + i as u16, footer_y)].set_char(ch).set_fg(Color::White).set_bg(Color::Black);
            }
        }

        // List area
        let list_y = y + 1;
        let list_height = 16; // height 20 - 4 (borders + footer)
        let list_x = x + 1;
        let list_width = width - 2;

        // Render settings list with sticky category headers
        let filtered = self.filtered_items();
        let mut last_category: Option<&str> = None;
        let mut last_rendered_category: Option<&str> = None;
        let mut display_row = 0;
        let mut render_row = 0;
        let visible_start = self.scroll_offset;
        let visible_end = visible_start + list_height as usize;

        for (abs_idx, (original_idx, item)) in filtered.iter().enumerate() {
            let item_category = &item.category;

            // Check if we need a category header
            if last_category.as_deref() != Some(item_category) {
                // Always increment display_row for the header
                if display_row >= visible_start {
                    // Render header if in visible range AND we have room
                    if display_row < visible_end && render_row < list_height as usize {
                        let current_y = list_y + render_row as u16;
                        let header = format!(" ═══ {} ═══", item_category.to_uppercase());
                        let header_style = Style::default().fg(Color::Yellow).bg(Color::Black).add_modifier(Modifier::BOLD);
                        for (i, ch) in header.chars().enumerate() {
                            if (list_x + i as u16) < (list_x + list_width) {
                                buf[(list_x + i as u16, current_y)].set_char(ch).set_style(header_style);
                            }
                        }
                        render_row += 1;
                        last_rendered_category = Some(item_category);
                    }
                }
                display_row += 1;
                last_category = Some(item_category);
            }

            // Skip if before visible range
            if display_row < visible_start {
                display_row += 1;
                continue;
            }

            // If this is a new category in the visible area and we haven't rendered its header yet (sticky header)
            if last_rendered_category.as_deref() != Some(item_category) && render_row < list_height as usize {
                let current_y = list_y + render_row as u16;
                let header = format!(" ═══ {} ═══", item_category.to_uppercase());
                let header_style = Style::default().fg(Color::Yellow).bg(Color::Black).add_modifier(Modifier::BOLD);
                for (i, ch) in header.chars().enumerate() {
                    if (list_x + i as u16) < (list_x + list_width) {
                        buf[(list_x + i as u16, current_y)].set_char(ch).set_style(header_style);
                    }
                }
                render_row += 1;
                last_rendered_category = Some(item_category);
            }

            // Stop if past visible range OR no room for entry
            if display_row >= visible_end || render_row >= list_height as usize {
                break;
            }

            let current_y = list_y + render_row as u16;

            // Determine if this item is selected or being edited
            let is_selected = abs_idx == self.selected_index;
            let is_being_edited = self.editing_index == Some(*original_idx);

            // Determine name column width based on category or item setting
            let name_width = if let Some(width) = item.name_width {
                width
            } else {
                // Default widths per category
                match item.category.as_str() {
                    "Presets" => 20,
                    "Prompts" => 20,  // Changed from 16 to align with other sections
                    "Spells" => 40,
                    _ => 20,  // Connection, UI, Sound
                }
            };

            // Style guide format: "  Label:              [value in textarea_background]"
            let textarea_bg = Self::parse_hex_color(&config.colors.ui.textarea_background).unwrap_or(Color::Rgb(64, 0, 0));
            let label_color = if is_selected { Color::Rgb(255, 215, 0) } else { Color::Cyan };

            // Render label with indent (2 spaces)
            let label = format!("  {}:", item.display_name);
            let mut x_pos = list_x;
            for ch in label.chars() {
                if x_pos < list_x + list_width {
                    buf[(x_pos, current_y)].set_char(ch).set_fg(label_color).set_bg(Color::Black);
                    x_pos += 1;
                }
            }

            // Calculate input field position (aligned at column 22 from list_x)
            let input_x = list_x + 22;
            let input_width = 44; // Fits in 70-width popup with margins

            // Render based on value type
            match &item.value {
                SettingValue::Boolean(value) => {
                    // Render checkbox at input position
                    let checkbox = if *value { "[✓]" } else { "[ ]" };
                    for (i, ch) in checkbox.chars().enumerate() {
                        if (input_x + i as u16) < (list_x + list_width) {
                            buf[(input_x + i as u16, current_y)].set_char(ch).set_fg(label_color).set_bg(Color::Black);
                        }
                    }
                }
                _ => {
                    // Render textarea with textarea_background color
                    let display_value = if is_being_edited {
                        &self.edit_buffer
                    } else {
                        match &item.value {
                            SettingValue::Enum(current, _) => current,
                            _ => &item.value.to_display_string()
                        }
                    };

                    // Fill background with textarea_background
                    for i in 0..input_width {
                        if input_x + i < list_x + list_width {
                            buf[(input_x + i, current_y)].set_bg(textarea_bg);
                        }
                    }

                    // Render text value
                    let text_color = if is_being_edited { Color::Green } else { Color::White };
                    for (i, ch) in display_value.chars().enumerate() {
                        if i >= input_width as usize {
                            break;
                        }
                        if (input_x + i as u16) < (list_x + list_width) {
                            buf[(input_x + i as u16, current_y)].set_char(ch).set_fg(text_color).set_bg(textarea_bg);
                        }
                    }
                }
            }

            display_row += 1;
            render_row += 1;
        }
    }

    fn draw_border(&self, x: u16, y: u16, width: u16, height: u16, buf: &mut Buffer) {
        let border_style = Style::default().fg(Color::Cyan);

        // Top border
        buf[(x, y)].set_char('┌').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y)].set_char('┐').set_style(border_style);

        // Side borders
        for row in 1..height - 1 {
            buf[(x, y + row)].set_char('│').set_style(border_style);
            buf[(x + width - 1, y + row)].set_char('│').set_style(border_style);
        }

        // Bottom border
        buf[(x, y + height - 1)].set_char('└').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y + height - 1)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y + height - 1)].set_char('┘').set_style(border_style);
    }

    fn parse_hex_color(hex: &str) -> Option<Color> {
        if !hex.starts_with('#') || hex.len() != 7 {
            return None;
        }
        let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
        let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
        let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    }
}
