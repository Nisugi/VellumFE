//! TUI renderer for Hand widget data.

use crate::frontend::common::widget_data::HandData;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear, Widget},
};

/// Parse a hex color string to ratatui Color
fn parse_color(hex: &str) -> Option<Color> {
    // Use centralized mode-aware color parser
    super::super::colors::parse_color_to_ratatui(hex)
}

/// Render a hand widget using TUI (ratatui) primitives
pub fn render_hand(data: &HandData, area: Rect, buf: &mut Buffer) {
    Clear.render(area, buf);

    if !data.transparent_background {
        let bg_color = data
            .background_color
            .as_ref()
            .and_then(|c| parse_color(c))
            .unwrap_or(Color::Reset);

        for row in 0..area.height {
            for col in 0..area.width {
                let x = area.x + col;
                let y = area.y + row;
                if x < buf.area().width && y < buf.area().height {
                    buf[(x, y)].set_bg(bg_color);
                }
            }
        }
    }

    // Determine which borders to show
    let borders = if data.border.show_border {
        crossterm_bridge::to_ratatui_borders(&data.border_sides)
    } else {
        Borders::NONE
    };

    let border_color = data
        .border
        .border_color
        .as_ref()
        .and_then(|c| parse_color(c))
        .unwrap_or(Color::White);

    // Check if we only have left/right borders (no top/bottom)
    let only_horizontal_borders = data.border.show_border
        && (borders.contains(Borders::LEFT) || borders.contains(Borders::RIGHT))
        && !borders.contains(Borders::TOP)
        && !borders.contains(Borders::BOTTOM);

    let inner_area: Rect;

    if only_horizontal_borders {
        // For left/right only borders, we'll manually render them on the content row
        let has_left = borders.contains(Borders::LEFT);
        let has_right = borders.contains(Borders::RIGHT);
        let border_width = (if has_left { 1 } else { 0 }) + (if has_right { 1 } else { 0 });

        inner_area = Rect {
            x: area.x + (if has_left { 1 } else { 0 }),
            y: area.y,
            width: area.width.saturating_sub(border_width),
            height: area.height,
        };
    } else if data.border.show_border {
        // Use Block widget for all other border combinations
        let mut block = Block::default().borders(borders);

        if let Some(ref style) = data.border.border_style {
            let border_type = match style.as_str() {
                "double" => BorderType::Double,
                "rounded" => BorderType::Rounded,
                "thick" => BorderType::Thick,
                "quadrant_inside" => BorderType::QuadrantInside,
                "quadrant_outside" => BorderType::QuadrantOutside,
                _ => BorderType::Plain,
            };
            block = block.border_type(border_type);
        }

        block = block.border_style(Style::default().fg(border_color));
        // Only set title if label is non-empty (avoids empty title affecting layout)
        if !data.label.is_empty() {
            block = block.title(data.label.as_str());
        }

        inner_area = block.inner(area);
        block.render(area, buf);
    } else {
        inner_area = area;
    }

    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }

    // Fill entire area with background color if not transparent
    let fill_bg = if data.transparent_background {
        None
    } else {
        data.background_color
            .as_ref()
            .and_then(|c| parse_color(c))
    };

    let base_text_color = data
        .text_color
        .as_ref()
        .and_then(|c| parse_color(c))
        .unwrap_or(Color::Reset);

    let content_color = data
        .content_highlight_color
        .as_ref()
        .and_then(|c| parse_color(c))
        .unwrap_or(base_text_color);

    let y = inner_area.y;

    // Render icon using configurable icon field
    for (i, ch) in data.icon.chars().enumerate() {
        let x = inner_area.x + i as u16;
        if x < inner_area.x + inner_area.width && x < buf.area().width && y < buf.area().height {
            buf[(x, y)].set_char(ch);
            buf[(x, y)].set_fg(base_text_color);
            if let Some(bg_color) = fill_bg {
                buf[(x, y)].set_bg(bg_color);
            }
        }
    }

    // Render content after icon (+ 1 space)
    let start_col = data.icon.chars().count() as u16 + 1;
    for (i, ch) in data.content.chars().enumerate() {
        let x = inner_area.x + start_col + i as u16;
        if x < inner_area.x + inner_area.width && x < buf.area().width && y < buf.area().height {
            buf[(x, y)].set_char(ch);
            buf[(x, y)].set_fg(content_color);
            if let Some(bg_color) = fill_bg {
                buf[(x, y)].set_bg(bg_color);
            }
        }
    }

    // If we have left/right only borders, render them manually on the content row
    if only_horizontal_borders {
        let content_y = inner_area.y;
        if content_y < buf.area().height {
            let has_left = borders.contains(Borders::LEFT);
            let has_right = borders.contains(Borders::RIGHT);

            // Render left border
            if has_left && area.x < buf.area().width {
                buf[(area.x, content_y)].set_char('│');
                buf[(area.x, content_y)].set_fg(border_color);
            }
            // Render right border
            if has_right {
                let right_x = area.x + area.width.saturating_sub(1);
                if right_x < buf.area().width {
                    buf[(right_x, content_y)].set_char('│');
                    buf[(right_x, content_y)].set_fg(border_color);
                }
            }
        }
    }
}
