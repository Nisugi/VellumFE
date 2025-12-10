//! TUI renderer for Indicator widget data.

use crate::frontend::common::widget_data::IndicatorData;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Widget},
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

/// Render an indicator using TUI (ratatui) primitives
pub fn render_indicator(data: &IndicatorData, area: Rect, buf: &mut Buffer) {
    if area.width < 1 || area.height < 1 {
        return;
    }

    // Resolve background color once for border + fill
    let resolved_bg = if !data.transparent_background {
        data.background_color.as_ref().map(|c| parse_color(c))
    } else {
        None
    };

    // Pre-fill the entire area so borders also sit on the correct background
    if let Some(bg_color) = resolved_bg {
        for row in 0..area.height {
            for col in 0..area.width {
                let x = area.x + col;
                let y = area.y + row;
                if x < buf.area().width && y < buf.area().height {
                    buf[(x, y)].set_char(' ');
                    buf[(x, y)].set_bg(bg_color);
                }
            }
        }
    }

    // Determine which borders to show
    let borders = if data.border.show_border {
        // For now, always show all borders when border is enabled
        // TODO: Support BorderSides configuration
        ratatui::widgets::Borders::ALL
    } else {
        ratatui::widgets::Borders::NONE
    };

    let border_color = data
        .border
        .border_color
        .as_ref()
        .map(|c| parse_color(c))
        .unwrap_or(Color::White);

    let inner_area: Rect;

    if data.border.show_border {
        // Use Block widget for borders
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
        if let Some(bg_color) = resolved_bg {
            block = block.style(Style::default().bg(bg_color));
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

    // Fill background if not transparent and color is set
    if let Some(bg_color) = resolved_bg {
        for row in 0..inner_area.height {
            for col in 0..inner_area.width {
                let x = inner_area.x + col;
                let y = inner_area.y + row;
                if x < buf.area().width && y < buf.area().height {
                    buf[(x, y)].set_char(' ');
                    buf[(x, y)].set_bg(bg_color);
                }
            }
        }
    }

    // If inactive, render nothing (transparent)
    if !data.active {
        return;
    }

    // Get color for active state
    let color = parse_color(&data.on_color);

    // Render the label text with appropriate color
    let display_text = &data.label;

    // Center the text in the available space
    let text_width = display_text.chars().count() as u16;
    let start_col = if text_width <= inner_area.width {
        inner_area.x + (inner_area.width - text_width) / 2
    } else {
        inner_area.x
    };

    // Render each character of the label (vertically centered)
    let y = inner_area.y;
    if y < buf.area().height {
        for (i, c) in display_text.chars().enumerate() {
            let x = start_col + i as u16;
            if x < inner_area.x + inner_area.width && x < buf.area().width {
                buf[(x, y)].set_char(c);
                buf[(x, y)].set_fg(color);
                // Set background if not transparent and color is configured
                if let Some(bg_color) = resolved_bg {
                    buf[(x, y)].set_bg(bg_color);
                }
            }
        }
    }
}
