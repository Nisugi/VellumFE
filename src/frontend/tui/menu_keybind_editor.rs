//! Menu keybind editor popup (TUI): edit the 26 fixed `MenuKeybinds`
//! navigation/action keys active while a menu/browser/form has focus. Unlike
//! game keybinds (a browsable key→action map) these are a fixed field set, so
//! this is a scrolling list of one row per `MenuKeybinds::FIELDS` entry.
//!
//! Interaction: Up/Down move the cursor; Enter arms capture (the next key
//! press becomes the selected row's binding); `d` resets the row to its
//! default; `g` toggles Global/character scope; Ctrl+S saves via
//! `Config::save_menu_keybinds` (validated by the shared validator); Esc
//! cancels. All wiring lives in input.rs, matching the other TUI editors.

use crate::config::MenuKeybinds;
use crate::frontend::tui::crossterm_bridge;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Widget},
};

const POPUP_WIDTH: u16 = 56;
const POPUP_HEIGHT: u16 = 24;

/// Result surfaced to input.rs after handling a key.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuKeybindEditorResult {
    /// Persist `working` to the chosen scope and close.
    Save,
    /// Close without saving.
    Cancel,
    /// Stay open (redraw).
    None,
}

pub struct MenuKeybindEditor {
    /// Working copy, committed on Save.
    pub working: MenuKeybinds,
    /// true = global keybinds.toml, false = this character's profile.
    pub is_global: bool,
    selected: usize,
    /// When true, the next key press is captured into the selected row.
    capturing: bool,
    status: String,
}

impl MenuKeybindEditor {
    pub fn new(menu: MenuKeybinds, is_global: bool) -> Self {
        Self {
            working: menu,
            is_global,
            selected: 0,
            capturing: false,
            status: "Enter: capture · d: default · g: scope · Ctrl+S: save · Esc: cancel"
                .to_string(),
        }
    }

    pub fn is_capturing(&self) -> bool {
        self.capturing
    }

    fn field_count() -> usize {
        MenuKeybinds::FIELDS.len()
    }

    pub fn navigate_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn navigate_down(&mut self) {
        if self.selected + 1 < Self::field_count() {
            self.selected += 1;
        }
    }

    /// Arm capture for the selected row (Enter). The next key press is routed
    /// here by input.rs via `apply_captured_key`.
    pub fn arm_capture(&mut self) {
        self.capturing = true;
        self.status = "Press a key for this action…".to_string();
    }

    /// Set the selected row from a captured key string (called by input.rs
    /// with the next key press while `is_capturing()`).
    pub fn apply_captured_key(&mut self, key: String) {
        if let Some(field) = MenuKeybinds::FIELDS.get(self.selected) {
            (field.set)(&mut self.working, key);
        }
        self.capturing = false;
        self.status = "Captured. Ctrl+S to save.".to_string();
    }

    /// Reset the selected row to its shipped default (`d`).
    pub fn reset_selected_to_default(&mut self) {
        if let Some(field) = MenuKeybinds::FIELDS.get(self.selected) {
            let def = MenuKeybinds::default();
            (field.set)(&mut self.working, (field.get)(&def).to_string());
            self.status = format!("{} reset to default.", field.label);
        }
    }

    /// Toggle Global/character scope (`g`).
    pub fn toggle_scope(&mut self) {
        self.is_global = !self.is_global;
        self.status = format!(
            "Scope: {}",
            if self.is_global { "Global" } else { "This character" }
        );
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, theme: &crate::theme::AppTheme) {
        let width = POPUP_WIDTH.min(area.width);
        let height = POPUP_HEIGHT.min(area.height);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let popup = Rect { x, y, width, height };
        Clear.render(popup, buf);

        let bg = crossterm_bridge::to_ratatui_color(theme.browser_background);
        let fg = crossterm_bridge::to_ratatui_color(theme.form_label);
        let focused = crossterm_bridge::to_ratatui_color(theme.form_label_focused);

        let scope = if self.is_global { "[G] Global" } else { "[C] Character" };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Menu Keybinds — {} ", scope))
            .style(Style::default().bg(bg).fg(fg));
        let inner = block.inner(popup);
        block.render(popup, buf);

        // Reserve the last inner row for the status line.
        let list_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(1),
        };

        let mut last_group = "";
        let mut items: Vec<ListItem> = Vec::new();
        // Track which list rows correspond to which field so the selected
        // field maps to the right list line (group headers add rows).
        let mut field_row: Vec<usize> = Vec::new();
        for field in MenuKeybinds::FIELDS {
            if field.group != last_group {
                last_group = field.group;
                items.push(ListItem::new(Line::from(Span::styled(
                    field.group,
                    Style::default().fg(focused).add_modifier(Modifier::BOLD),
                ))));
            }
            field_row.push(items.len());
            let value = (field.get)(&self.working);
            let shown = if value.is_empty() { "(unset)" } else { value };
            let armed = self.capturing
                && field_row.len() - 1 == self.selected;
            let value_text = if armed { "…press a key…" } else { shown };
            let label = format!("  {:<18} {}", field.label, value_text);
            items.push(ListItem::new(Line::from(Span::styled(
                label,
                Style::default().fg(fg),
            ))));
        }

        let mut list_state = ListState::default();
        list_state.select(field_row.get(self.selected).copied());

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(crossterm_bridge::to_ratatui_color(theme.browser_item_selected))
                    .fg(focused)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        ratatui::widgets::StatefulWidget::render(list, list_area, buf, &mut list_state);

        // Status line.
        let status_y = inner.y + inner.height.saturating_sub(1);
        buf.set_string(
            inner.x,
            status_y,
            format!("{:width$}", self.status, width = inner.width as usize),
            Style::default().bg(bg).fg(fg),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_sets_the_selected_field() {
        let mut ed = MenuKeybindEditor::new(MenuKeybinds::default(), true);
        // Move to the 2nd field and capture a new key for it.
        ed.navigate_down();
        assert_eq!(ed.selected, 1);
        ed.arm_capture();
        assert!(ed.is_capturing());
        ed.apply_captured_key("Ctrl+j".to_string());
        assert!(!ed.is_capturing());
        // Field index 1 is navigate_down per FIELDS order.
        let field = &MenuKeybinds::FIELDS[1];
        assert_eq!((field.get)(&ed.working), "Ctrl+j");
    }

    #[test]
    fn reset_restores_field_default() {
        let mut ed = MenuKeybindEditor::new(MenuKeybinds::default(), true);
        ed.arm_capture();
        ed.apply_captured_key("Zzz".to_string());
        assert_eq!((MenuKeybinds::FIELDS[0].get)(&ed.working), "Zzz");
        ed.reset_selected_to_default();
        let def = MenuKeybinds::default();
        assert_eq!(
            (MenuKeybinds::FIELDS[0].get)(&ed.working),
            (MenuKeybinds::FIELDS[0].get)(&def)
        );
    }

    #[test]
    fn scope_toggle_flips_is_global() {
        let mut ed = MenuKeybindEditor::new(MenuKeybinds::default(), true);
        assert!(ed.is_global);
        ed.toggle_scope();
        assert!(!ed.is_global);
    }

    #[test]
    fn navigation_clamps_at_bounds() {
        let mut ed = MenuKeybindEditor::new(MenuKeybinds::default(), true);
        ed.navigate_up(); // already at 0, stays
        assert_eq!(ed.selected, 0);
        for _ in 0..100 {
            ed.navigate_down();
        }
        assert_eq!(ed.selected, MenuKeybinds::FIELDS.len() - 1);
    }
}
