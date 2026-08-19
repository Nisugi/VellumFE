//! Status abbreviation editor popup (TUI): edit the
//! `target_list.status_abbrev` map (full status name -> short tag) shown in
//! the targets and players windows. Unlike the menu keybind editor (a fixed
//! field set) this is a growable map, so rows can be added and deleted.
//!
//! Interaction: Up/Down move the cursor; Tab switches the edited column
//! (Name / Abbrev); typing edits the focused cell; `a` adds a blank row and
//! focuses it; `x` deletes the selected row; Ctrl+S saves via
//! `AppCore::save_config`; Esc cancels. All wiring lives in input.rs, matching
//! the other TUI editors.

use crate::frontend::tui::crossterm_bridge;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, StatefulWidget, Widget},
};

const POPUP_WIDTH: u16 = 48;
const POPUP_HEIGHT: u16 = 22;

/// Which cell of the selected row typing edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Abbrev,
}

/// Result surfaced to input.rs after handling a key.
#[derive(Debug, Clone, PartialEq)]
pub enum StatusAbbrevEditorResult {
    /// Persist the map and close.
    Save,
    /// Close without saving.
    Cancel,
    /// Stay open (redraw).
    None,
}

pub struct StatusAbbrevEditor {
    /// Working rows (name, abbrev), committed on Save. Kept sorted by name on
    /// entry so the list has a stable order.
    rows: Vec<(String, String)>,
    selected: usize,
    column: Column,
    status: String,
}

impl StatusAbbrevEditor {
    /// Seed from the live config map. Rows are sorted by name so the list is
    /// stable across opens (HashMap order is otherwise arbitrary).
    pub fn new(map: &std::collections::HashMap<String, String>) -> Self {
        let mut rows: Vec<(String, String)> =
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            rows,
            selected: 0,
            column: Column::Abbrev,
            status: "Tab: column · type: edit · a: add · x: delete · Ctrl+S: save · Esc: cancel"
                .to_string(),
        }
    }

    /// Rebuild a map from the working rows, dropping blank-name rows and
    /// lowercasing names so render-side `.to_lowercase()` lookups resolve.
    pub fn to_map(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        for (name, abbrev) in &self.rows {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            map.insert(name.to_lowercase(), abbrev.trim().to_string());
        }
        map
    }

    pub fn navigate_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn navigate_down(&mut self) {
        if self.selected + 1 < self.rows.len() {
            self.selected += 1;
        }
    }

    /// Tab: switch the edited column.
    pub fn toggle_column(&mut self) {
        self.column = match self.column {
            Column::Name => Column::Abbrev,
            Column::Abbrev => Column::Name,
        };
    }

    /// Append a blank row and focus its Name cell.
    pub fn add_row(&mut self) {
        self.rows.push((String::new(), String::new()));
        self.selected = self.rows.len() - 1;
        self.column = Column::Name;
        self.status = "New row — type a status name.".to_string();
    }

    /// Delete the selected row.
    pub fn delete_row(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.rows.remove(self.selected);
        if self.selected >= self.rows.len() {
            self.selected = self.rows.len().saturating_sub(1);
        }
        self.status = "Row deleted.".to_string();
    }

    /// Insert a typed character into the focused cell.
    pub fn insert_char(&mut self, c: char) {
        if let Some(row) = self.rows.get_mut(self.selected) {
            let cell = match self.column {
                Column::Name => &mut row.0,
                Column::Abbrev => &mut row.1,
            };
            // Abbrevs are meant to be short; cap at 3 chars like the default map.
            if matches!(self.column, Column::Abbrev) && cell.chars().count() >= 3 {
                return;
            }
            cell.push(c);
        }
    }

    /// Backspace from the focused cell.
    pub fn backspace(&mut self) {
        if let Some(row) = self.rows.get_mut(self.selected) {
            let cell = match self.column {
                Column::Name => &mut row.0,
                Column::Abbrev => &mut row.1,
            };
            cell.pop();
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
    }

    #[cfg(test)]
    pub fn rows(&self) -> &[(String, String)] {
        &self.rows
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, theme: &crate::theme::AppTheme) {
        let width = POPUP_WIDTH.min(area.width);
        let height = POPUP_HEIGHT.min(area.height);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let popup = Rect {
            x,
            y,
            width,
            height,
        };
        Clear.render(popup, buf);

        let bg = crossterm_bridge::to_ratatui_color(theme.browser_background);
        let fg = crossterm_bridge::to_ratatui_color(theme.form_label);
        let focused = crossterm_bridge::to_ratatui_color(theme.form_label_focused);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Status Abbreviations ")
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

        let items: Vec<ListItem> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, (name, abbrev))| {
                let name_shown = if name.is_empty() { "(name)" } else { name };
                let abbrev_shown = if abbrev.is_empty() { "(abbr)" } else { abbrev };
                // Mark the focused cell on the selected row with brackets.
                let (name_cell, abbrev_cell) = if i == self.selected {
                    match self.column {
                        Column::Name => (format!("[{}]", name_shown), abbrev_shown.to_string()),
                        Column::Abbrev => (name_shown.to_string(), format!("[{}]", abbrev_shown)),
                    }
                } else {
                    (name_shown.to_string(), abbrev_shown.to_string())
                };
                let label = format!("{:<20} ▸ {}", name_cell, abbrev_cell);
                ListItem::new(Line::from(Span::styled(label, Style::default().fg(fg))))
            })
            .collect();

        let mut list_state = ListState::default();
        if !self.rows.is_empty() {
            list_state.select(Some(self.selected));
        }

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(crossterm_bridge::to_ratatui_color(
                        theme.browser_item_selected,
                    ))
                    .fg(focused)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        StatefulWidget::render(list, list_area, buf, &mut list_state);

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
    use std::collections::HashMap;

    fn sample() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("stunned".to_string(), "stu".to_string());
        m.insert("dead".to_string(), "ded".to_string());
        m
    }

    #[test]
    fn new_sorts_rows_by_name() {
        let ed = StatusAbbrevEditor::new(&sample());
        assert_eq!(ed.rows()[0].0, "dead");
        assert_eq!(ed.rows()[1].0, "stunned");
    }

    #[test]
    fn typing_edits_abbrev_cell_and_caps_at_three() {
        let mut ed = StatusAbbrevEditor::new(&sample());
        // selected = 0 (dead), column defaults to Abbrev.
        ed.backspace();
        ed.backspace();
        ed.backspace();
        assert_eq!(ed.rows()[0].1, "");
        ed.insert_char('d');
        ed.insert_char('e');
        ed.insert_char('d');
        ed.insert_char('X'); // 4th char rejected
        assert_eq!(ed.rows()[0].1, "ded");
    }

    #[test]
    fn tab_switches_column_to_name() {
        let mut ed = StatusAbbrevEditor::new(&sample());
        ed.toggle_column();
        // Now editing Name of row 0 ("dead").
        ed.insert_char('X');
        assert_eq!(ed.rows()[0].0, "deadX");
    }

    #[test]
    fn add_and_delete_rows() {
        let mut ed = StatusAbbrevEditor::new(&sample());
        ed.add_row();
        assert_eq!(ed.rows().len(), 3);
        // New row focuses Name; type a name + abbrev.
        ed.insert_char('c');
        ed.insert_char('a');
        ed.insert_char('l');
        ed.toggle_column();
        ed.insert_char('c');
        assert_eq!(ed.rows()[2], ("cal".to_string(), "c".to_string()));

        ed.delete_row();
        assert_eq!(ed.rows().len(), 2);
    }

    #[test]
    fn to_map_drops_blank_names_and_lowercases() {
        let mut ed = StatusAbbrevEditor::new(&sample());
        ed.add_row(); // blank name row
        let map = ed.to_map();
        assert_eq!(map.len(), 2, "blank-name row dropped");
        assert!(map.contains_key("dead"));
        assert!(map.contains_key("stunned"));
    }
}
