use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
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
            let visible_height: usize = 15; // Approximate
            let jump = visible_height.saturating_sub(1).max(1);
            self.selected_index = self.selected_index.saturating_sub(jump);
            self.adjust_scroll();
        }
    }

    pub fn page_down(&mut self) {
        if !self.is_editing() {
            let filtered = self.filtered_items();
            let visible_height: usize = 15; // Approximate
            let jump = visible_height.saturating_sub(1).max(1);
            self.selected_index = (self.selected_index + jump).min(filtered.len().saturating_sub(1));
            self.adjust_scroll();
        }
    }

    fn adjust_scroll(&mut self) {
        let visible_height: usize = 15; // Approximate
        // Ensure selected item is visible
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected_index.saturating_sub(visible_height - 1);
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
        // Calculate popup size
        let popup_width = 70.min(area.width);
        let popup_height = 25.min(area.height);

        // Center popup initially if not yet positioned
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(popup_width)) / 2;
            self.popup_y = (area.height.saturating_sub(popup_height)) / 2;
        }

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Fill background with solid black
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < area.width && y < area.height {
                    buf[(x, y)].set_char(' ').set_bg(Color::Black);
                }
            }
        }

        // Draw border
        let border_color = Color::Cyan;
        self.draw_border(&popup_area, buf, border_color);

        // Draw title
        let title = " Settings Editor ";
        let title_x = popup_area.x + (popup_area.width.saturating_sub(title.len() as u16)) / 2;
        for (i, ch) in title.chars().enumerate() {
            if let Some(x) = title_x.checked_add(i as u16) {
                if x < popup_area.x + popup_area.width {
                    buf[(x, popup_area.y)].set_char(ch).set_fg(Color::Cyan).set_bg(Color::Black);
                }
            }
        }

        // Draw instructions at bottom
        let instructions = if self.is_editing() {
            "Type to edit | Enter: Save | Esc: Cancel"
        } else {
            "↑/↓: Navigate | Enter: Edit/Toggle | PgUp/PgDn: Scroll | Esc: Close"
        };
        let instr_y = popup_area.y + popup_area.height - 1;
        let instr_x = popup_area.x + 2;
        for (i, ch) in instructions.chars().enumerate() {
            if let Some(x) = instr_x.checked_add(i as u16) {
                if x < popup_area.x + popup_area.width - 1 {
                    buf[(x, instr_y)].set_char(ch).set_fg(Color::DarkGray).set_bg(Color::Black);
                }
            }
        }

        // Inner content area
        let content_x = popup_area.x + 1;
        let content_y = popup_area.y + 1;
        let content_width = popup_area.width.saturating_sub(2);
        let content_height = popup_area.height.saturating_sub(3); // Subtract top/bottom borders and instructions

        // Render settings list
        let filtered = self.filtered_items();
        let mut current_category = String::new();
        let mut y = content_y;
        let visible_count = content_height as usize;

        for (display_idx, (original_idx, item)) in filtered.iter().enumerate().skip(self.scroll_offset) {
            if y >= content_y + content_height {
                break;
            }

            // Render category header if changed
            if item.category != current_category {
                current_category = item.category.clone();

                // Render category header
                let header = format!("[{}]", item.category);
                for (i, ch) in header.chars().enumerate() {
                    if let Some(x) = content_x.checked_add(i as u16) {
                        if x < content_x + content_width {
                            let mut cell = buf[(x, y)].clone();
                            cell.set_char(ch).set_fg(Color::Yellow).set_bg(Color::Black);
                            cell.modifier.insert(Modifier::BOLD);
                            buf[(x, y)] = cell;
                        }
                    }
                }
                y += 1;
                if y >= content_y + content_height {
                    break;
                }
            }

            // Determine if this item is selected or being edited
            let is_selected = display_idx == self.selected_index;
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

            // Build display name and value
            let (display_name, display_value) = if is_being_edited {
                (item.display_name.clone(), format!("[{}]", self.edit_buffer))
            } else {
                let value_str = match &item.value {
                    SettingValue::Boolean(b) => {
                        if *b { "[✓]" } else { "[ ]" }
                    }
                    SettingValue::String(s) if item.category == "Presets" || item.category == "Prompts" => {
                        // Format as "7chars 7chars" for presets/prompts
                        let parts: Vec<&str> = s.split_whitespace().collect();
                        let fg = parts.get(0).unwrap_or(&"-");
                        let bg = parts.get(1).unwrap_or(&"-");
                        &format!("{:<7} {:<7}", fg, bg)
                    }
                    _ => &item.value.to_display_string()
                };
                (item.display_name.clone(), value_str.to_string())
            };

            // Format line with padding: "  NAME:<spaces> VALUE"
            let indent = "  ";
            let name_part = if display_name.len() > name_width as usize {
                format!("{}...", &display_name[..name_width.saturating_sub(3) as usize])
            } else {
                format!("{:<width$}", display_name, width = name_width as usize)
            };

            // Style based on state
            let (fg, bg, bold) = if is_being_edited {
                (Color::Green, Color::Black, true)
            } else if is_selected {
                (Color::Black, Color::Cyan, true)
            } else if !item.editable {
                (Color::DarkGray, Color::Black, false)
            } else {
                (Color::White, Color::Black, false)
            };

            // Render indent
            let mut x_pos = content_x;
            for ch in indent.chars() {
                if x_pos < content_x + content_width {
                    let mut cell = buf[(x_pos, y)].clone();
                    cell.set_char(ch).set_fg(fg).set_bg(bg);
                    if bold {
                        cell.modifier.insert(Modifier::BOLD);
                    }
                    buf[(x_pos, y)] = cell;
                    x_pos += 1;
                }
            }

            // Render name part
            for ch in name_part.chars() {
                if x_pos < content_x + content_width {
                    let mut cell = buf[(x_pos, y)].clone();
                    cell.set_char(ch).set_fg(fg).set_bg(bg);
                    if bold {
                        cell.modifier.insert(Modifier::BOLD);
                    }
                    buf[(x_pos, y)] = cell;
                    x_pos += 1;
                }
            }

            // Render colon separator
            if x_pos < content_x + content_width {
                let mut cell = buf[(x_pos, y)].clone();
                cell.set_char(':').set_fg(fg).set_bg(bg);
                if bold {
                    cell.modifier.insert(Modifier::BOLD);
                }
                buf[(x_pos, y)] = cell;
                x_pos += 1;
            }

            // Render space after colon
            if x_pos < content_x + content_width {
                let mut cell = buf[(x_pos, y)].clone();
                cell.set_char(' ').set_fg(fg).set_bg(bg);
                if bold {
                    cell.modifier.insert(Modifier::BOLD);
                }
                buf[(x_pos, y)] = cell;
                x_pos += 1;
            }

            // Render value (up to available space, leaving room for color blocks)
            let value_max_width = content_width.saturating_sub((x_pos - content_x) + 10);
            for (i, ch) in display_value.chars().enumerate() {
                if i >= value_max_width as usize {
                    break;
                }
                if x_pos < content_x + content_width {
                    let mut cell = buf[(x_pos, y)].clone();
                    cell.set_char(ch).set_fg(fg).set_bg(bg);
                    if bold {
                        cell.modifier.insert(Modifier::BOLD);
                    }
                    buf[(x_pos, y)] = cell;
                    x_pos += 1;
                }
            }

            let x_offset = x_pos - content_x;

            // Render color preview blocks for Color and String settings that contain colors
            // (Color settings, Presets, Prompts)
            let should_render_colors = matches!(&item.value, SettingValue::Color(_) | SettingValue::String(_))
                && (item.category == "UI" || item.category == "Presets" || item.category == "Prompts" || item.category == "Spells")
                && !is_being_edited;

            if should_render_colors {
                let value_str = item.value.to_display_string();
                let parts: Vec<&str> = value_str.split_whitespace().collect();

                // For Presets/Prompts: value is "7chars 7chars", so previews start at fixed offset
                let preview_x = if item.category == "Presets" || item.category == "Prompts" {
                    content_x + x_offset + 1  // Right after the formatted value
                } else {
                    content_x + x_offset + 1  // Right after other values
                };

                // Foreground color preview (always render if we have a color)
                if !parts.is_empty() && parts[0] != "-" {
                    let resolved = config.resolve_color(parts[0]);
                    if let Some(hex) = resolved {
                        if let Some(color) = Self::parse_hex_color(&hex) {
                            for i in 0..3 {
                                if let Some(x) = preview_x.checked_add(i) {
                                    if x < content_x + content_width {
                                        buf[(x, y)].set_char('█').set_fg(color).set_bg(bg);
                                    }
                                }
                            }
                        }
                    }
                }

                // Background color preview (always render space for alignment, even if no bg)
                let bg_preview_x = preview_x + 4;  // 3 blocks + 1 space
                if parts.len() > 1 && parts[1] != "-" {
                    let resolved = config.resolve_color(parts[1]);
                    if let Some(hex) = resolved {
                        if let Some(color) = Self::parse_hex_color(&hex) {
                            for i in 0..3 {
                                if let Some(x) = bg_preview_x.checked_add(i) {
                                    if x < content_x + content_width {
                                        buf[(x, y)].set_char('█').set_fg(color).set_bg(bg);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            y += 1;
        }

        // Show scroll indicator if needed
        if filtered.len() > visible_count {
            let scroll_info = format!("{}/{}", self.selected_index + 1, filtered.len());
            let scroll_x = popup_area.x + popup_area.width.saturating_sub(scroll_info.len() as u16 + 2);
            let scroll_y = popup_area.y;
            for (i, ch) in scroll_info.chars().enumerate() {
                if let Some(x) = scroll_x.checked_add(i as u16) {
                    if x < popup_area.x + popup_area.width - 1 {
                        buf[(x, scroll_y)].set_char(ch).set_fg(Color::Cyan).set_bg(Color::Black);
                    }
                }
            }
        }
    }

    fn draw_border(&self, area: &Rect, buf: &mut Buffer, color: Color) {
        // Top and bottom borders
        for x in area.x..area.x + area.width {
            if x < buf.area.width {
                if area.y < buf.area.height {
                    buf[(x, area.y)].set_char('─').set_fg(color).set_bg(Color::Black);
                }
                let bottom_y = area.y + area.height - 1;
                if bottom_y < buf.area.height {
                    buf[(x, bottom_y)].set_char('─').set_fg(color).set_bg(Color::Black);
                }
            }
        }

        // Left and right borders
        for y in area.y..area.y + area.height {
            if y < buf.area.height {
                if area.x < buf.area.width {
                    buf[(area.x, y)].set_char('│').set_fg(color).set_bg(Color::Black);
                }
                let right_x = area.x + area.width - 1;
                if right_x < buf.area.width {
                    buf[(right_x, y)].set_char('│').set_fg(color).set_bg(Color::Black);
                }
            }
        }

        // Corners
        if area.x < buf.area.width && area.y < buf.area.height {
            buf[(area.x, area.y)].set_char('┌').set_fg(color).set_bg(Color::Black);
        }
        let top_right_x = area.x + area.width - 1;
        if top_right_x < buf.area.width && area.y < buf.area.height {
            buf[(top_right_x, area.y)].set_char('┐').set_fg(color).set_bg(Color::Black);
        }
        let bottom_left_y = area.y + area.height - 1;
        if area.x < buf.area.width && bottom_left_y < buf.area.height {
            buf[(area.x, bottom_left_y)].set_char('└').set_fg(color).set_bg(Color::Black);
        }
        let bottom_right_x = area.x + area.width - 1;
        let bottom_right_y = area.y + area.height - 1;
        if bottom_right_x < buf.area.width && bottom_right_y < buf.area.height {
            buf[(bottom_right_x, bottom_right_y)].set_char('┘').set_fg(color).set_bg(Color::Black);
        }
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
