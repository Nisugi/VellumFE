//! Comprehensive UI color browser/editor covering presets and prompts.
//!
//! Exposes inline swatches plus an embedded mini editor for tweaking fg/bg
//! values inline without leaving the popup.

use crate::frontend::tui::crossterm_bridge;
use crate::config::ColorConfig;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Clear, Widget},
};
use tui_textarea::TextArea;

#[derive(Clone, Debug)]
pub enum UIColorEntryType {
    UIColor,
    Preset,
    PromptColor,
}

#[derive(Clone, Debug)]
pub enum UIColorEditorResult {
    Save {
        fg: Option<String>,
        bg: Option<String>,
    },
    Cancel,
}

#[derive(Clone, Debug)]
pub struct UIColorEntry {
    pub name: String,
    pub category: String,
    pub fg_color: Option<String>,
    pub bg_color: Option<String>,
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
    textarea_bg_color: Color,
}

impl UIColorEditor {
    pub fn new(entry: &UIColorEntry, textarea_bg: &str) -> Self {
        let mut color_fg = TextArea::default();
        if let Some(fg) = &entry.fg_color {
            color_fg.insert_str(fg);
        }

        let mut color_bg = TextArea::default();
        if let Some(bg) = &entry.bg_color {
            color_bg.insert_str(bg);
        }

        // Parse textarea background color
        let textarea_bg_color = if textarea_bg == "-" {
            Color::Reset
        } else {
            Self::parse_color_static(textarea_bg)
        };

        Self {
            asset_name: entry.name.clone(),
            color_fg,
            color_bg,
            focused_field: 0,
            popup_x: 0,
            popup_y: 0,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            textarea_bg_color,
        }
    }

    fn parse_color_static(color_str: &str) -> Color {
        if color_str.is_empty() {
            return Color::Reset;
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
        Color::Reset
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<UIColorEditorResult> {
        match key.code {
            KeyCode::Esc => return Some(UIColorEditorResult::Cancel),
            KeyCode::Char('s') | KeyCode::Char('S')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                // Save
                let fg = self.color_fg.lines()[0].trim();
                let bg = self.color_bg.lines()[0].trim();
                return Some(UIColorEditorResult::Save {
                    fg: if fg.is_empty() {
                        None
                    } else {
                        Some(fg.to_string())
                    },
                    bg: if bg.is_empty() {
                        None
                    } else {
                        Some(bg.to_string())
                    },
                });
            }
            KeyCode::Tab => {
                // Tab: next field
                self.focused_field = if self.focused_field == 1 { 0 } else { 1 };
            }
            KeyCode::BackTab => {
                // Shift+Tab: previous field
                self.focused_field = if self.focused_field == 0 { 1 } else { 0 };
            }
            KeyCode::Up => {
                self.focused_field = if self.focused_field == 0 { 1 } else { 0 };
            }
            KeyCode::Down => {
                self.focused_field = if self.focused_field == 1 { 0 } else { 1 };
            }
            _ => {
                // Pass to active TextArea (convert KeyEvent for tui-textarea compatibility)
                let rt_key = crate::frontend::tui::textarea_bridge::to_textarea_event(key);
                let active_field = if self.focused_field == 0 {
                    &mut self.color_fg
                } else {
                    &mut self.color_bg
                };
                active_field.input(rt_key);
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

    pub fn render(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        _config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
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

        // Clear the popup area to prevent bleed-through
        let popup_area = Rect {
            x,
            y,
            width,
            height,
        };
        Clear.render(popup_area, buf);

        // Black background
        for row in 0..height {
            for col in 0..width {
                if x + col < area.width && y + row < area.height {
                    if let Some(cell) = buf.cell_mut((x + col, y + row)) {
                        cell.set_char(' ');
                        cell.set_style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.browser_background)));
                    }
                }
            }
        }

        // Cyan border
        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.browser_border));

        // Top border
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_char('┌').set_style(border_style);
        }
        for col in 1..width - 1 {
            if let Some(cell) = buf.cell_mut((x + col, y)) {
                cell.set_char('─').set_style(border_style);
            }
        }
        if let Some(cell) = buf.cell_mut((x + width - 1, y)) {
            cell.set_char('┐').set_style(border_style);
        }

        // Bottom border
        if let Some(cell) = buf.cell_mut((x, y + height - 1)) {
            cell.set_char('└').set_style(border_style);
        }
        for col in 1..width - 1 {
            if let Some(cell) = buf.cell_mut((x + col, y + height - 1)) {
                cell.set_char('─').set_style(border_style);
            }
        }
        if let Some(cell) = buf.cell_mut((x + width - 1, y + height - 1)) {
            cell.set_char('┘').set_style(border_style);
        }

        // Side borders
        for row in 1..height - 1 {
            if let Some(cell) = buf.cell_mut((x, y + row)) {
                cell.set_char('│').set_style(border_style);
            }
            if let Some(cell) = buf.cell_mut((x + width - 1, y + row)) {
                cell.set_char('│').set_style(border_style);
            }
        }

        // Title (left-aligned)
        let title = " Edit UI Color ";
        for (i, ch) in title.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((x + 1 + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(
                    Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.browser_border))
                        .add_modifier(Modifier::BOLD),
                );
            }
        }

        // Row 2: Asset: <name>
        let asset_label = format!("  Asset: {}", self.asset_name);
        for (i, ch) in asset_label.chars().enumerate() {
            if i < (width - 2) as usize {
                if let Some(cell) = buf.cell_mut((x + 1 + i as u16, y + 2)) {
                    cell.set_char(ch);
                    cell.set_style(
                        Style::default()
                            .fg(crossterm_bridge::to_ratatui_color(theme.browser_border))
                            .bg(crossterm_bridge::to_ratatui_color(theme.browser_background)),
                    );
                }
            }
        }

        // Row 4: Color: <textarea> <preview>
        let color_label = "  Color:";
        let color_label_style = if self.focused_field == 0 {
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                .bg(crossterm_bridge::to_ratatui_color(theme.browser_background)) // Gold when focused
        } else {
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.browser_border))
                .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
        };
        for (i, ch) in color_label.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((x + 1 + i as u16, y + 4)) {
                cell.set_char(ch);
                cell.set_style(color_label_style);
            }
        }

        let textarea_start = x + 1 + 8 + 6; // "  Color:" = 8 chars, + 6 spaces = col 15

        // Color FG textarea (10 chars)
        let fg_style = Style::default()
            .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
            .bg(self.textarea_bg_color);
        let fg_text = self.color_fg.lines()[0].clone();
        for i in 0..10 {
            let ch = fg_text.chars().nth(i).unwrap_or(' ');
            if let Some(cell) = buf.cell_mut((textarea_start + i as u16, y + 4)) {
                cell.set_char(ch);
                cell.set_style(fg_style);
            }
        }

        // 1 space gap
        if let Some(cell) = buf.cell_mut((textarea_start + 10, y + 4)) {
            cell.set_char(' ');
            cell.set_style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.browser_background)));
        }

        // FG color preview (2 chars)
        let fg_color = self.parse_color(&fg_text);
        for i in 0..2 {
            if let Some(cell) = buf.cell_mut((textarea_start + 11 + i, y + 4)) {
                cell.set_char(' ');
                cell.set_style(Style::default().bg(fg_color));
            }
        }

        // Row 5: Background: <textarea> <preview>
        let bg_label = "  Background:";
        let bg_label_style = if self.focused_field == 1 {
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
        } else {
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.browser_border))
                .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
        };
        for (i, ch) in bg_label.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((x + 1 + i as u16, y + 5)) {
                cell.set_char(ch);
                cell.set_style(bg_label_style);
            }
        }

        let bg_textarea_start = x + 1 + 13 + 1; // "  Background:" = 13 chars, + 1 space = col 15

        // Color BG textarea (10 chars)
        let bg_style = Style::default()
            .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
            .bg(self.textarea_bg_color);
        let bg_text = self.color_bg.lines()[0].clone();
        for i in 0..10 {
            let ch = bg_text.chars().nth(i).unwrap_or(' ');
            if let Some(cell) = buf.cell_mut((bg_textarea_start + i as u16, y + 5)) {
                cell.set_char(ch);
                cell.set_style(bg_style);
            }
        }

        // 1 space gap
        if let Some(cell) = buf.cell_mut((bg_textarea_start + 10, y + 5)) {
            cell.set_char(' ');
            cell.set_style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.browser_background)));
        }

        // BG color preview (2 chars)
        let bg_color = self.parse_color(&bg_text);
        for i in 0..2 {
            if let Some(cell) = buf.cell_mut((bg_textarea_start + 11 + i, y + 5)) {
                cell.set_char(' ');
                cell.set_style(Style::default().bg(bg_color));
            }
        }

        // Footer (one line above the bottom border)
        let footer = " Ctrl+s:Save | Tab:Next Field | Esc:Cancel ";
        let footer_x = x + (width - footer.len() as u16) / 2;
        for (i, ch) in footer.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((footer_x + i as u16, y + height - 2)) {
                cell.set_char(ch);
                cell.set_style(
                    Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                        .bg(crossterm_bridge::to_ratatui_color(theme.browser_background)),
                );
            }
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
    pub editor: Option<UIColorEditor>,
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
        let mut prompt_chars: Vec<_> = colors
            .prompt_colors
            .iter()
            .map(|p| p.character.clone())
            .collect();
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
            popup_x: 0,
            popup_y: 0,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            width: 70,
            height: 20,
            editor: None,
        }
    }

    pub fn navigate_up(&mut self) {
        if self.selected_index > 0 {
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

        let visible_rows = 15;

        // Adjust scroll to keep selected item in view
        if selected_display_row < self.scroll_offset {
            self.scroll_offset = selected_display_row;
        } else if selected_display_row >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected_display_row.saturating_sub(visible_rows - 1);
        }
    }

    pub fn open_editor(&mut self, textarea_bg: &str) {
        if let Some(entry) = self.entries.get(self.selected_index) {
            self.editor = Some(UIColorEditor::new(entry, textarea_bg));
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
            return Some((
                category,
                name,
                entry.fg_color.clone(),
                entry.bg_color.clone(),
            ));
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

    pub fn render(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        _config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        // Center on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(self.width)) / 2;
            self.popup_y = (area.height.saturating_sub(self.height)) / 2;
        }

        let x = self.popup_x;
        let y = self.popup_y;

        // Clear the popup area to prevent bleed-through
        let popup_area = Rect {
            x,
            y,
            width: self.width,
            height: self.height,
        };
        Clear.render(popup_area, buf);

        // Draw black background
        for row in 0..self.height {
            for col in 0..self.width {
                if x + col < area.width && y + row < area.height {
                    if let Some(cell) = buf.cell_mut((x + col, y + row)) {
                        cell.set_char(' ');
                        cell.set_style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.browser_background)));
                    }
                }
            }
        }

        // Draw cyan border
        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.browser_border));

        // Top border
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_char('┌').set_style(border_style);
        }
        for col in 1..self.width - 1 {
            if let Some(cell) = buf.cell_mut((x + col, y)) {
                cell.set_char('─').set_style(border_style);
            }
        }
        if let Some(cell) = buf.cell_mut((x + self.width - 1, y)) {
            cell.set_char('┐').set_style(border_style);
        }

        // Title
        let title = " UI Colors Browser ";
        for (i, ch) in title.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((x + 1 + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(
                    Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.browser_border))
                        .add_modifier(Modifier::BOLD),
                );
            }
        }

        // Side borders
        for row in 1..self.height - 1 {
            if let Some(cell) = buf.cell_mut((x, y + row)) {
                cell.set_char('│').set_style(border_style);
            }
            if let Some(cell) = buf.cell_mut((x + self.width - 1, y + row)) {
                cell.set_char('│').set_style(border_style);
            }
        }

        // Bottom border
        if let Some(cell) = buf.cell_mut((x, y + self.height - 1)) {
            cell.set_char('└').set_style(border_style);
        }
        for col in 1..self.width - 1 {
            if let Some(cell) = buf.cell_mut((x + col, y + self.height - 1)) {
                cell.set_char('─').set_style(border_style);
            }
        }
        if let Some(cell) = buf.cell_mut((x + self.width - 1, y + self.height - 1)) {
            cell.set_char('┘').set_style(border_style);
        }

        // Render entries with display_row tracking
        let list_y = y + 1;
        let list_height = 16;
        let mut last_category: Option<&str> = None;
        let mut last_rendered_category: Option<&str> = None;
        let mut display_row = 0;
        let mut render_row = 0;
        let visible_start = self.scroll_offset;
        let visible_end = visible_start + list_height;

        for (idx, entry) in self.entries.iter().enumerate() {
            // Check if we need a category header
            if last_category != Some(entry.category.as_str()) {
                if display_row >= visible_start
                    && display_row < visible_end && render_row < list_height {
                        // Render the header
                        let current_y = list_y + render_row as u16;
                        let header_text = format!(" ═══ {} ═══", entry.category);
                        let header_style = Style::default()
                            .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                            .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                            .add_modifier(Modifier::BOLD);

                        for (i, ch) in header_text.chars().enumerate() {
                            if i < (self.width - 2) as usize {
                                if let Some(cell) = buf.cell_mut((x + 1 + i as u16, current_y)) {
                                    cell.set_char(ch);
                                    cell.set_style(header_style);
                                }
                            }
                        }

                        // Fill rest of line
                        for i in header_text.len()..(self.width - 2) as usize {
                            if let Some(cell) = buf.cell_mut((x + 1 + i as u16, current_y)) {
                                cell.set_char(' ');
                                cell.set_style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.browser_background)));
                            }
                        }

                        render_row += 1;
                        last_rendered_category = Some(&entry.category);
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
                let current_y = list_y + render_row as u16;
                let header_text = format!(" ═══ {} ═══", entry.category);
                let header_style = Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
                    .add_modifier(Modifier::BOLD);

                for (i, ch) in header_text.chars().enumerate() {
                    if i < (self.width - 2) as usize {
                        if let Some(cell) = buf.cell_mut((x + 1 + i as u16, current_y)) {
                            cell.set_char(ch);
                            cell.set_style(header_style);
                        }
                    }
                }

                for i in header_text.len()..(self.width - 2) as usize {
                    if let Some(cell) = buf.cell_mut((x + 1 + i as u16, current_y)) {
                        cell.set_char(' ');
                        cell.set_style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.browser_background)));
                    }
                }

                render_row += 1;
                last_rendered_category = Some(&entry.category);
            }

            // Stop if past visible range OR no room for entry
            if display_row >= visible_end || render_row >= list_height {
                break;
            }

            // Render entry row
            let current_y = list_y + render_row as u16;
            let is_selected = idx == self.selected_index;

            // Col 2-4: FG color preview
            if let Some(fg) = &entry.fg_color {
                let fg_color = self.parse_color(fg);
                for i in 0..3 {
                    if let Some(cell) = buf.cell_mut((x + 2 + i, current_y)) {
                        cell.set_char(' ');
                        cell.set_style(Style::default().bg(fg_color));
                    }
                }
            } else if let Some(cell) = buf.cell_mut((x + 3, current_y)) {
                cell.set_char('-').set_style(
                    Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.menu_separator))
                        .bg(crossterm_bridge::to_ratatui_color(theme.browser_background)),
                );
            }

            // Col 7-9: BG color preview
            if let Some(bg) = &entry.bg_color {
                let bg_color = self.parse_color(bg);
                for i in 0..3 {
                    if let Some(cell) = buf.cell_mut((x + 7 + i, current_y)) {
                        cell.set_char(' ');
                        cell.set_style(Style::default().bg(bg_color));
                    }
                }
            } else if let Some(cell) = buf.cell_mut((x + 8, current_y)) {
                cell.set_char('-').set_style(
                    Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.menu_separator))
                        .bg(crossterm_bridge::to_ratatui_color(theme.browser_background)),
                );
            }

            // Col 13+: Entry name
            let name_style = if is_selected {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.browser_item_focused))
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
            } else {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.browser_border))
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_background))
            };

            let name_with_space = format!("   {}", entry.name);
            for (i, ch) in name_with_space.chars().enumerate() {
                let col = x + 13 + i as u16;
                if col < x + self.width - 1 {
                    if let Some(cell) = buf.cell_mut((col, current_y)) {
                        cell.set_char(ch);
                        cell.set_style(name_style);
                    }
                }
            }

            display_row += 1;
            render_row += 1;
        }

        // Footer
        let footer = " Tab/Arrows:Navigate | Enter/Space:Edit | Ctrl+s:Save | Esc:Close ";
        let footer_y = y + self.height - 2;
        let footer_x = x + ((self.width - footer.len() as u16) / 2);
        for (i, ch) in footer.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((footer_x + i as u16, footer_y)) {
                cell.set_char(ch);
                cell.set_style(
                    Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                        .bg(crossterm_bridge::to_ratatui_color(theme.browser_background)),
                );
            }
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

    /// Move to next page (alias for page_down)
    pub fn next_page(&mut self) {
        self.page_down();
    }

    /// Move to previous page (alias for page_up)
    pub fn previous_page(&mut self) {
        self.page_up();
    }
}

// Trait implementations for UIColorsBrowser
use super::widget_traits::{Navigable, Selectable};

impl Navigable for UIColorsBrowser {
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

impl Selectable for UIColorsBrowser {
    fn get_selected(&self) -> Option<String> {
        self.entries
            .get(self.selected_index)
            .map(|e| e.name.clone())
    }

    fn delete_selected(&mut self) -> Option<String> {
        // UI colors can't be deleted, only edited
        None
    }
}
