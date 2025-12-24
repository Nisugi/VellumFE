//! Dialog popup rendering and hit-testing for dynamic openDialog payloads.

use crate::data::DialogState;
use crate::frontend::tui::crossterm_bridge;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

pub struct DialogLayout {
    pub area: Rect,
    pub field_areas: Vec<Rect>,
    pub button_areas: Vec<Rect>,
}

fn display_label(button: &crate::data::DialogButton) -> String {
    if button.is_radio {
        let marker = if button.selected { "x" } else { " " };
        format!("[{}] {}", marker, button.label)
    } else {
        button.label.clone()
    }
}

fn field_label(dialog: &DialogState, index: usize) -> String {
    dialog
        .labels
        .get(index)
        .map(|label| label.value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| dialog.fields[index].id.clone())
}

fn field_line_text(dialog: &DialogState, index: usize, include_cursor: bool) -> String {
    let label = field_label(dialog, index);
    let field = &dialog.fields[index];
    let mut value = field.value.clone();
    if include_cursor {
        let cursor = field.cursor.min(value.len());
        value.insert(cursor, '|');
    }
    if label.is_empty() {
        value
    } else {
        format!("{}: {}", label, value)
    }
}

pub fn compute_dialog_layout(screen: Rect, dialog: &DialogState) -> DialogLayout {
    let title = dialog.title.as_deref().unwrap_or("Dialog");
    let field_count = dialog.fields.len();
    let max_label_len = dialog
        .buttons
        .iter()
        .map(|button| display_label(button).len())
        .max()
        .unwrap_or(4);
    let max_field_len = dialog
        .fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let cursor_extra = if dialog.focused_field == Some(idx) { 1 } else { 0 };
            let label_len = field_label(dialog, idx).len();
            let value_len = field.value.len() + cursor_extra;
            if label_len == 0 {
                value_len
            } else {
                label_len + 2 + value_len
            }
        })
        .max()
        .unwrap_or(0);
    let content_width = max_label_len
        .max(max_field_len)
        .max(title.len())
        .max(4);
    let mut width = (content_width + 4) as u16;
    if screen.width > 0 {
        width = width.min(screen.width);
    }

    let total_lines = (field_count + dialog.buttons.len()).max(1);
    let mut height = (total_lines + 2) as u16;
    if screen.height > 0 {
        height = height.min(screen.height);
    }

    let x = screen
        .x
        .saturating_add(screen.width.saturating_sub(width) / 2);
    let y = screen
        .y
        .saturating_add(screen.height.saturating_sub(height) / 2);

    let area = Rect { x, y, width, height };

    let mut field_areas = Vec::new();
    for (idx, _) in dialog.fields.iter().enumerate() {
        let line_y = y.saturating_add(1 + idx as u16);
        if line_y < y.saturating_add(height.saturating_sub(1)) {
            field_areas.push(Rect {
                x: x.saturating_add(1),
                y: line_y,
                width: width.saturating_sub(2),
                height: 1,
            });
        }
    }

    let mut button_areas = Vec::new();
    for (idx, _) in dialog.buttons.iter().enumerate() {
        let line_y = y.saturating_add(1 + field_count as u16 + idx as u16);
        if line_y < y.saturating_add(height.saturating_sub(1)) {
            button_areas.push(Rect {
                x: x.saturating_add(1),
                y: line_y,
                width: width.saturating_sub(2),
                height: 1,
            });
        }
    }

    DialogLayout {
        area,
        field_areas,
        button_areas,
    }
}

pub fn hit_test_button(layout: &DialogLayout, x: u16, y: u16) -> Option<usize> {
    layout.button_areas.iter().position(|area| {
        x >= area.x
            && x < area.x.saturating_add(area.width)
            && y >= area.y
            && y < area.y.saturating_add(area.height)
    })
}

pub fn hit_test_field(layout: &DialogLayout, x: u16, y: u16) -> Option<usize> {
    layout.field_areas.iter().position(|area| {
        x >= area.x
            && x < area.x.saturating_add(area.width)
            && y >= area.y
            && y < area.y.saturating_add(area.height)
    })
}

pub fn render_dialog(
    dialog: &DialogState,
    screen: Rect,
    buf: &mut Buffer,
    theme: &crate::theme::AppTheme,
) {
    let layout = compute_dialog_layout(screen, dialog);
    let title = dialog.title.as_deref().unwrap_or("Dialog");

    Clear.render(layout.area, buf);

    let mut lines = Vec::new();
    if dialog.fields.is_empty() && dialog.buttons.is_empty() {
        lines.push(Line::from(Span::raw(" ")));
    } else {
        for (idx, _field) in dialog.fields.iter().enumerate() {
            let is_focused = dialog.focused_field == Some(idx);
            let style = if is_focused {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.menu_item_selected))
                    .bg(crossterm_bridge::to_ratatui_color(theme.menu_item_focused))
            } else {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.menu_item_normal))
                    .bg(crossterm_bridge::to_ratatui_color(theme.menu_background))
            };
            let line_text = field_line_text(dialog, idx, is_focused);
            let line = Line::from(vec![
                Span::raw(" "),
                Span::styled(line_text, style),
                Span::raw(" "),
            ]);
            lines.push(line);
        }

        for (idx, button) in dialog.buttons.iter().enumerate() {
            let style = if idx == dialog.selected {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.menu_item_selected))
                    .bg(crossterm_bridge::to_ratatui_color(theme.menu_item_focused))
            } else {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.menu_item_normal))
                    .bg(crossterm_bridge::to_ratatui_color(theme.menu_background))
            };

            let line = Line::from(vec![
                Span::raw(" "),
                Span::styled(display_label(button), style),
                Span::raw(" "),
            ]);
            lines.push(line);
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(crossterm_bridge::to_ratatui_color(theme.menu_border)))
        .style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.menu_background)));

    let paragraph = Paragraph::new(lines).block(block);
    paragraph.render(layout.area, buf);
}
