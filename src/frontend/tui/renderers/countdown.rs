//! TUI renderer for Countdown widget data.

use crate::frontend::common::widget_data::CountdownData;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

/// Parse a hex color string to ratatui Color
fn parse_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Color::White;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);

    Color::Rgb(r, g, b)
}

/// Render a countdown timer using TUI (ratatui) primitives
pub fn render_countdown(data: &CountdownData, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 1 {
        return;
    }

    let border_color = data
        .border
        .border_color
        .as_ref()
        .map(|c| parse_color(c))
        .unwrap_or(Color::White);

    // Determine background color (no theme available here)
    let bg_color = if data.transparent_background {
        None
    } else {
        data.background_color.as_ref().map(|c| parse_color(c))
    };

    let inner_area: Rect;

    if data.border.show_border {
        // Use Block widget for borders
        let mut block = Block::default().borders(Borders::ALL);

        if let Some(ref style) = data.border.border_style {
            let border_type = match style.as_str() {
                "double" => BorderType::Double,
                "rounded" => BorderType::Rounded,
                "thick" => BorderType::Thick,
                _ => BorderType::Plain,
            };
            block = block.border_type(border_type);
        }

        block = block.border_style(Style::default().fg(border_color));
        if let Some(bg) = bg_color {
            block = block.style(Style::default().bg(bg));
        }
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

    let text_color = data
        .text_color
        .as_ref()
        .map(|c| parse_color(c))
        .unwrap_or(Color::White);

    // Determine background color
    let bg_color = if !data.transparent_background {
        data.background_color.as_ref().map(|c| parse_color(c))
    } else {
        None
    };

    // Clear the bar area with appropriate background
    let y = inner_area.y;
    if y < buf.area().height {
        for i in 0..inner_area.width {
            let x = inner_area.x + i;
            if x < buf.area().width {
                buf[(x, y)].set_char(' ');
                if let Some(bg) = bg_color {
                    buf[(x, y)].set_bg(bg);
                }
            }
        }
    }

    // If countdown is 0, leave it blank (invisible)
    if data.remaining_seconds == 0 {
        return;
    }

    // Render countdown number on the left (right-aligned within 3 chars)
    let text_width = data.display_text.len() as u16; // Always 3 chars
    if y < buf.area().height {
        for (i, c) in data.display_text.chars().enumerate() {
            let x = inner_area.x + i as u16;
            if x < inner_area.x + inner_area.width && x < buf.area().width {
                buf[(x, y)].set_char(c);
                buf[(x, y)].set_fg(text_color);
                if let Some(bg) = bg_color {
                    buf[(x, y)].set_bg(bg);
                }
            }
        }

        // Render blocks after the number
        for i in 0..data.blocks_to_show {
            let pos = text_width + i as u16;
            if pos < inner_area.width {
                let x = inner_area.x + pos;
                if x < buf.area().width {
                    buf[(x, y)].set_char(data.icon);
                    buf[(x, y)].set_fg(text_color);
                    if let Some(bg) = bg_color {
                        buf[(x, y)].set_bg(bg);
                    }
                }
            }
        }
    }
}
