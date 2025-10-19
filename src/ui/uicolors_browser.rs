use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use tui_textarea::TextArea;
use crate::config::ColorConfig;

#[derive(Clone, Debug)]
pub enum UIColorEntryType {
    UIColor,
    Preset,
    PromptColor,
}

#[derive(Clone, Debug)]
pub struct UIColorEntry {
    pub name: String,
    pub category: String,
    pub fg_color: Option<String>,
    pub bg_color: Option<String>,
    #[allow(dead_code)]
    pub entry_type: UIColorEntryType,
}

/// Inline color editor popup (52x9) for editing individual color entries
pub struct UIColorEditor {
    asset_name: String,
    color_fg: TextArea<'static>,
    color_bg: TextArea<'static>,
    focused_field: usize, // 0 = fg, 1 = bg
    popup_x: u16,
    popup_y: u16,
    pub dragging: bool,
    drag_offset_x: u16,
    drag_offset_y: u16,
}

impl UIColorEditor {
    pub fn new(entry: &UIColorEntry) -> Self {
        let mut color_fg = TextArea::default();
        if let Some(fg) = &entry.fg_color {
            color_fg.insert_str(fg);
        }

        let mut color_bg = TextArea::default();
        if let Some(bg) = &entry.bg_color {
            color_bg.insert_str(bg);
        }

        Self {
            asset_name: entry.name.clone(),
            color_fg,
            color_bg,
            focused_field: 0,
            popup_x: 0,  // Will be centered on first render
            popup_y: 0,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<(Option<String>, Option<String>)> {
        match key.code {
            KeyCode::Esc => return Some((None, None)), // Cancel
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Save
                let fg = self.color_fg.lines()[0].trim();
                let bg = self.color_bg.lines()[0].trim();
                return Some((
                    if fg.is_empty() { None } else { Some(fg.to_string()) },
                    if bg.is_empty() { None } else { Some(bg.to_string()) },
                ));
            }
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                // Shift+Tab: previous field
                self.focused_field = if self.focused_field == 0 { 1 } else { 0 };
            }
            KeyCode::Tab => {
                // Tab: next field
                self.focused_field = if self.focused_field == 1 { 0 } else { 1 };
            }
            KeyCode::Up => {
                self.focused_field = if self.focused_field == 0 { 1 } else { 0 };
            }
            KeyCode::Down => {
                self.focused_field = if self.focused_field == 1 { 0 } else { 1 };
            }
            _ => {
                // Pass to active TextArea
                let active_field = if self.focused_field == 0 {
                    &mut self.color_fg
                } else {
                    &mut self.color_bg
                };
                // Pass the full key event to the TextArea
                active_field.input(key);
            }
        }
        None
    }

    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) {
        const POPUP_WIDTH: u16 = 52;

        if !mouse_down {
            self.dragging = false;
            return;
        }

        // Title bar detection: top border row, excluding corners
        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > self.popup_x
            && mouse_col < self.popup_x + POPUP_WIDTH.saturating_sub(1);

        if on_title_bar && !self.dragging {
            self.dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
        }

        if self.dragging {
            self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
            self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);

            // Keep within bounds
            self.popup_x = self.popup_x.min(area.width.saturating_sub(POPUP_WIDTH));
            self.popup_y = self.popup_y.min(area.height.saturating_sub(9));
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        const POPUP_WIDTH: u16 = 52;
        const POPUP_HEIGHT: u16 = 9;

        // Center on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(POPUP_WIDTH)) / 2;
            self.popup_y = (area.height.saturating_sub(POPUP_HEIGHT)) / 2;
        }

        let x = self.popup_x;
        let y = self.popup_y;
        let width = POPUP_WIDTH;
        let height = POPUP_HEIGHT;

        // Black background
        for row in 0..height {
            for col in 0..width {
                if x + col < area.width && y + row < area.height {
                    buf.get_mut(x + col, y + row)
                        .set_char(' ')
                        .set_style(Style::default().bg(Color::Black));
                }
            }
        }

        // Cyan border
        let border_style = Style::default().fg(Color::Cyan);

        // Top border
        buf.get_mut(x, y).set_char('┌').set_style(border_style);
        for col in 1..width - 1 {
            buf.get_mut(x + col, y).set_char('─').set_style(border_style);
        }
        buf.get_mut(x + width - 1, y).set_char('┐').set_style(border_style);

        // Bottom border
        buf.get_mut(x, y + height - 1).set_char('└').set_style(border_style);
        for col in 1..width - 1 {
            buf.get_mut(x + col, y + height - 1).set_char('─').set_style(border_style);
        }
        buf.get_mut(x + width - 1, y + height - 1).set_char('┘').set_style(border_style);

        // Side borders
        for row in 1..height - 1 {
            buf.get_mut(x, y + row).set_char('│').set_style(border_style);
            buf.get_mut(x + width - 1, y + row).set_char('│').set_style(border_style);
        }

        // Title (left-aligned)
        let title = " Edit UI Color ";
        for (i, ch) in title.chars().enumerate() {
            buf.get_mut(x + 1 + i as u16, y)
                .set_char(ch)
                .set_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        }

        // Row 2: Asset: <name>
        let asset_label = format!("  Asset: {}", self.asset_name);
        for (i, ch) in asset_label.chars().enumerate() {
            if i < (width - 2) as usize {
                buf.get_mut(x + 1 + i as u16, y + 2)
                    .set_char(ch)
                    .set_style(Style::default().fg(Color::Cyan).bg(Color::Black));
            }
        }

        // Row 4: Color: <6 spaces> <10 char textarea> <1 space> <2 char preview>
        let color_label = "  Color:";
        let color_label_style = if self.focused_field == 0 {
            Style::default().fg(Color::Rgb(255, 215, 0)).bg(Color::Black) // Gold when focused
        } else {
            Style::default().fg(Color::Cyan).bg(Color::Black) // Cyan normally
        };
        for (i, ch) in color_label.chars().enumerate() {
            buf.get_mut(x + 1 + i as u16, y + 4)
                .set_char(ch)
                .set_style(color_label_style);
        }

        // 6 spaces after "Color:"
        let textarea_start = x + 1 + 8 + 6; // "  Color:" = 8 chars, + 6 spaces = col 15

        // Color FG textarea (10 chars, dark maroon, no focus highlight)
        let fg_style = Style::default().fg(Color::White).bg(Color::Rgb(64, 0, 0));
        let fg_text = self.color_fg.lines()[0].clone();
        for i in 0..10 {
            let ch = fg_text.chars().nth(i).unwrap_or(' ');
            buf.get_mut(textarea_start + i as u16, y + 4)
                .set_char(ch)
                .set_style(fg_style);
        }

        // 1 space gap
        buf.get_mut(textarea_start + 10, y + 4)
            .set_char(' ')
            .set_style(Style::default().bg(Color::Black));

        // FG color preview (2 chars)
        let fg_color = self.parse_color(&fg_text);
        for i in 0..2 {
            buf.get_mut(textarea_start + 11 + i, y + 4)
                .set_char(' ')
                .set_style(Style::default().bg(fg_color));
        }

        // Row 5: Background: <6 spaces> <10 char textarea> <1 space> <2 char preview>
        let bg_label = "  Background:";
        let bg_label_style = if self.focused_field == 1 {
            Style::default().fg(Color::Rgb(255, 215, 0)).bg(Color::Black) // Gold when focused
        } else {
            Style::default().fg(Color::Cyan).bg(Color::Black) // Cyan normally
        };
        for (i, ch) in bg_label.chars().enumerate() {
            buf.get_mut(x + 1 + i as u16, y + 5)
                .set_char(ch)
                .set_style(bg_label_style);
        }

        // 6 spaces after "Background:" (which is 13 chars, so textarea starts at col 20)
        let bg_textarea_start = x + 1 + 13 + 1; // "  Background:" = 13 chars, + 1 spaces = col 15

        // Color BG textarea (10 chars, dark maroon, no focus highlight)
        let bg_style = Style::default().fg(Color::White).bg(Color::Rgb(64, 0, 0));
        let bg_text = self.color_bg.lines()[0].clone();
        for i in 0..10 {
            let ch = bg_text.chars().nth(i).unwrap_or(' ');
            buf.get_mut(bg_textarea_start + i as u16, y + 5)
                .set_char(ch)
                .set_style(bg_style);
        }

        // 1 space gap
        buf.get_mut(bg_textarea_start + 10, y + 5)
            .set_char(' ')
            .set_style(Style::default().bg(Color::Black));

        // BG color preview (2 chars)
        let bg_color = self.parse_color(&bg_text);
        for i in 0..2 {
            buf.get_mut(bg_textarea_start + 11 + i, y + 5)
                .set_char(' ')
                .set_style(Style::default().bg(bg_color));
        }

        // Footer (one line above the bottom border)
        let footer = " Ctrl+Enter:Save | Tab:Next Field | Esc:Cancel ";
        let footer_x = x + (width - footer.len() as u16) / 2;
        for (i, ch) in footer.chars().enumerate() {
            buf.get_mut(footer_x + i as u16, y + height - 2)
                .set_char(ch)
                .set_style(Style::default().fg(Color::White).bg(Color::Black));
        }
    }

    fn parse_color(&self, color_str: &str) -> Color {
        if color_str.is_empty() {
            return Color::Black;
        }
        if color_str.starts_with('#') && color_str.len() >= 7 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&color_str[1..3], 16),
                u8::from_str_radix(&color_str[3..5], 16),
                u8::from_str_radix(&color_str[5..7], 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        }
        Color::Black
    }
}

pub struct UIColorsBrowser {
    entries: Vec<UIColorEntry>,
    selected_index: usize,
    scroll_offset: usize,
    popup_x: u16,
    popup_y: u16,
    pub dragging: bool,
    drag_offset_x: u16,
    drag_offset_y: u16,
    width: u16,
    height: u16,
    pub editor: Option<UIColorEditor>, // Active color editor popup
}

impl UIColorsBrowser {
    pub fn new(colors: &ColorConfig) -> Self {
        let mut entries = Vec::new();

        // Add UI colors
        entries.push(UIColorEntry {
            name: "Background".to_string(),
            category: "UI".to_string(),
            fg_color: Some(colors.ui.background_color.clone()),
            bg_color: None,
            entry_type: UIColorEntryType::UIColor,
        });
        entries.push(UIColorEntry {
            name: "Border".to_string(),
            category: "UI".to_string(),
            fg_color: Some(colors.ui.border_color.clone()),
            bg_color: None,
            entry_type: UIColorEntryType::UIColor,
        });
        entries.push(UIColorEntry {
            name: "Command Echo".to_string(),
            category: "UI".to_string(),
            fg_color: Some(colors.ui.command_echo_color.clone()),
            bg_color: None,
            entry_type: UIColorEntryType::UIColor,
        });
        entries.push(UIColorEntry {
            name: "Focused Border".to_string(),
            category: "UI".to_string(),
            fg_color: Some(colors.ui.focused_border_color.clone()),
            bg_color: None,
            entry_type: UIColorEntryType::UIColor,
        });
        entries.push(UIColorEntry {
            name: "Text".to_string(),
            category: "UI".to_string(),
            fg_color: Some(colors.ui.text_color.clone()),
            bg_color: None,
            entry_type: UIColorEntryType::UIColor,
        });
        entries.push(UIColorEntry {
            name: "Text Selection".to_string(),
            category: "UI".to_string(),
            fg_color: Some(colors.ui.selection_bg_color.clone()),
            bg_color: None,
            entry_type: UIColorEntryType::UIColor,
        });
        entries.push(UIColorEntry {
            name: "Textarea Background".to_string(),
            category: "UI".to_string(),
            fg_color: Some(colors.ui.textarea_background.clone()),
            bg_color: None,
            entry_type: UIColorEntryType::UIColor,
        });

        // Add presets (sorted by name)
        let mut preset_names: Vec<_> = colors.presets.keys().cloned().collect();
        preset_names.sort();
        for preset_name in preset_names {
            if let Some(preset) = colors.presets.get(&preset_name) {
                entries.push(UIColorEntry {
                    name: preset_name.clone(),
                    category: "PRESETS".to_string(),
                    fg_color: preset.fg.clone(),
                    bg_color: preset.bg.clone(),
                    entry_type: UIColorEntryType::Preset,
                });
            }
        }

        // Add prompt colors (sorted by character)
        let mut prompt_chars: Vec<_> = colors.prompt_colors.iter().map(|p| p.character.clone()).collect();
        prompt_chars.sort();
        for ch in prompt_chars {
            if let Some(prompt) = colors.prompt_colors.iter().find(|p| p.character == ch) {
                entries.push(UIColorEntry {
                    name: format!("Prompt ({})", ch),
                    category: "PROMPT".to_string(),
                    fg_color: prompt.color.clone(),
                    bg_color: None,
                    entry_type: UIColorEntryType::PromptColor,
                });
            }
        }

        Self {
            entries,
            selected_index: 0,
            scroll_offset: 0,
            popup_x: 0,  // Will be centered on first render
            popup_y: 0,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            width: 70,
            height: 20,
            editor: None,
        }
    }

    pub fn previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
        }
    }

    pub fn next(&mut self) {
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
        // Calculate total display rows including category headers
        let mut total_display_rows = 0;
        let mut last_category: Option<&str> = None;
        let mut selected_display_row = 0;

        for (idx, entry) in self.entries.iter().enumerate() {
            // Add category header row if category changes
            if last_category != Some(entry.category.as_str()) {
                total_display_rows += 1;
                last_category = Some(&entry.category);
            }

            // Track which display row the selected item is on
            if idx == self.selected_index {
                selected_display_row = total_display_rows;
            }

            total_display_rows += 1;
        }

        let visible_rows = 15; // One less than list_height to account for sticky headers

        // Adjust scroll to keep selected item in view
        if selected_display_row < self.scroll_offset {
            self.scroll_offset = selected_display_row;
        } else if selected_display_row >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected_display_row.saturating_sub(visible_rows - 1);
        }
    }

    pub fn open_editor(&mut self) {
        if let Some(entry) = self.entries.get(self.selected_index) {
            self.editor = Some(UIColorEditor::new(entry));
        }
    }

    pub fn close_editor(&mut self) {
        self.editor = None;
    }

    pub fn save_editor(&mut self) -> Option<(String, String, Option<String>, Option<String>)> {
        // Returns (category, name, fg, bg)
        if let Some(entry) = self.entries.get(self.selected_index) {
            let category = entry.category.clone();
            let name = entry.name.clone();
            self.editor = None;
            return Some((category, name, entry.fg_color.clone(), entry.bg_color.clone()));
        }
        None
    }

    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) {
        if !mouse_down {
            self.dragging = false;
            return;
        }

        // Title bar detection: top border row, excluding corners
        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > self.popup_x
            && mouse_col < self.popup_x + self.width.saturating_sub(1);

        if on_title_bar && !self.dragging {
            self.dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
        }

        if self.dragging {
            self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
            self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);

            // Keep within bounds
            self.popup_x = self.popup_x.min(area.width.saturating_sub(self.width));
            self.popup_y = self.popup_y.min(area.height.saturating_sub(self.height));
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        // Center on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(self.width)) / 2;
            self.popup_y = (area.height.saturating_sub(self.height)) / 2;
        }

        let x = self.popup_x;
        let y = self.popup_y;

        // Draw black background
        for row in 0..self.height {
            for col in 0..self.width {
                if x + col < area.width && y + row < area.height {
                    buf.get_mut(x + col, y + row)
                        .set_char(' ')
                        .set_style(Style::default().bg(Color::Black));
                }
            }
        }

        // Draw cyan border
        let border_style = Style::default().fg(Color::Cyan);

        // Top border
        buf.get_mut(x, y).set_char('┌').set_style(border_style);
        for col in 1..self.width - 1 {
            buf.get_mut(x + col, y).set_char('─').set_style(border_style);
        }
        buf.get_mut(x + self.width - 1, y).set_char('┐').set_style(border_style);

        // Title (left-aligned)
        let title = " UI Colors Browser ";
        for (i, ch) in title.chars().enumerate() {
            buf.get_mut(x + 1 + i as u16, y)
                .set_char(ch)
                .set_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        }

        // Side borders
        for row in 1..self.height - 1 {
            buf.get_mut(x, y + row).set_char('│').set_style(border_style);
            buf.get_mut(x + self.width - 1, y + row).set_char('│').set_style(border_style);
        }

        // Bottom border
        buf.get_mut(x, y + self.height - 1).set_char('└').set_style(border_style);
        for col in 1..self.width - 1 {
            buf.get_mut(x + col, y + self.height - 1).set_char('─').set_style(border_style);
        }
        buf.get_mut(x + self.width - 1, y + self.height - 1).set_char('┘').set_style(border_style);

        // Render entries with display_row tracking (like color_palette_browser)
        let list_y = y + 1;
        let list_height = 16; // height 20 - 4 (borders + footer)
        let mut last_category: Option<&str> = None;
        let mut last_rendered_category: Option<&str> = None;  // Track what we've rendered in visible area
        let mut display_row = 0;
        let mut render_row = 0;
        let visible_start = self.scroll_offset;
        let visible_end = visible_start + list_height;

        for (idx, entry) in self.entries.iter().enumerate() {
            // Check if we need a category header
            if last_category != Some(entry.category.as_str()) {
                // Always increment display_row for the header
                if display_row >= visible_start {
                    // Header is in visible range or we're past it
                    if display_row < visible_end && render_row < list_height {
                        // Render the header
                        let current_y = list_y + render_row as u16;
                        let header_text = format!(" ═══ {} ═══", entry.category);
                        let header_style = Style::default()
                            .fg(Color::Rgb(255, 215, 0)) // Gold
                            .bg(Color::Black)
                            .add_modifier(Modifier::BOLD);

                        for (i, ch) in header_text.chars().enumerate() {
                            if i < (self.width - 2) as usize {
                                buf.get_mut(x + 1 + i as u16, current_y)
                                    .set_char(ch)
                                    .set_style(header_style);
                            }
                        }

                        // Fill rest of line with spaces
                        for i in header_text.len()..(self.width - 2) as usize {
                            buf.get_mut(x + 1 + i as u16, current_y)
                                .set_char(' ')
                                .set_style(Style::default().bg(Color::Black));
                        }

                        render_row += 1;
                        last_rendered_category = Some(&entry.category);
                    }
                }
                display_row += 1;
                last_category = Some(&entry.category);
            }

            // Skip if before visible range
            if display_row < visible_start {
                display_row += 1;
                continue;
            }

            // If this is a new category in the visible area and we haven't rendered its header yet
            if last_rendered_category != Some(entry.category.as_str()) && render_row < list_height {
                // Render sticky header for this category
                let current_y = list_y + render_row as u16;
                let header_text = format!(" ═══ {} ═══", entry.category);
                let header_style = Style::default()
                    .fg(Color::Rgb(255, 215, 0)) // Gold
                    .bg(Color::Black)
                    .add_modifier(Modifier::BOLD);

                for (i, ch) in header_text.chars().enumerate() {
                    if i < (self.width - 2) as usize {
                        buf.get_mut(x + 1 + i as u16, current_y)
                            .set_char(ch)
                            .set_style(header_style);
                    }
                }

                // Fill rest of line with spaces
                for i in header_text.len()..(self.width - 2) as usize {
                    buf.get_mut(x + 1 + i as u16, current_y)
                        .set_char(' ')
                        .set_style(Style::default().bg(Color::Black));
                }

                render_row += 1;
                last_rendered_category = Some(&entry.category);
            }

            // Stop if past visible range OR no room for entry
            if display_row >= visible_end || render_row >= list_height {
                break;
            }

            // Render entry row (with 1 col padding from left border)
            let current_y = list_y + render_row as u16;
            let is_selected = idx == self.selected_index;

            // Col 2-4: FG color preview [P]
            if let Some(fg) = &entry.fg_color {
                let fg_color = self.parse_color(fg);
                for i in 0..3 {
                    buf.get_mut(x + 2 + i, current_y)
                        .set_char(' ')
                        .set_style(Style::default().bg(fg_color));
                }
            } else {
                // No color: show [-]
                buf.get_mut(x + 3, current_y).set_char('-').set_style(Style::default().fg(Color::Gray).bg(Color::Black));
            }

            // Col 7-9: BG color preview [P] or [-]
            if let Some(bg) = &entry.bg_color {
                let bg_color = self.parse_color(bg);
                for i in 0..3 {
                    buf.get_mut(x + 7 + i, current_y)
                        .set_char(' ')
                        .set_style(Style::default().bg(bg_color));
                }
            } else {
                buf.get_mut(x + 8, current_y).set_char('-').set_style(Style::default().fg(Color::Gray).bg(Color::Black));
            }

            // Col 13+: Entry name
            let name_style = if is_selected {
                Style::default().fg(Color::Rgb(255, 215, 0)).bg(Color::Black) // Gold when selected
            } else {
                Style::default().fg(Color::Cyan).bg(Color::Black) // Cyan otherwise
            };

            let name_with_space = format!("   {}", entry.name);
            for (i, ch) in name_with_space.chars().enumerate() {
                let col = x + 13 + i as u16;
                if col < x + self.width - 1 {
                    buf.get_mut(col, current_y)
                        .set_char(ch)
                        .set_style(name_style);
                }
            }

            display_row += 1;
            render_row += 1;
        }

        // Footer (one line above the bottom border)
        let footer = " Tab/Arrows:Navigate | Enter/Space:Edit | Ctrl+S:Save | Esc:Close ";
        let footer_y = y + self.height - 2;
        let footer_x = x + ((self.width - footer.len() as u16) / 2);
        for (i, ch) in footer.chars().enumerate() {
            buf.get_mut(footer_x + i as u16, footer_y)
                .set_char(ch)
                .set_style(Style::default().fg(Color::White).bg(Color::Black));
        }
    }

    fn parse_color(&self, color_str: &str) -> Color {
        if color_str.is_empty() {
            return Color::Black;
        }
        if color_str.starts_with('#') && color_str.len() >= 7 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&color_str[1..3], 16),
                u8::from_str_radix(&color_str[3..5], 16),
                u8::from_str_radix(&color_str[5..7], 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        }
        Color::Black
    }
}
