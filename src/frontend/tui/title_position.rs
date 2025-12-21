use crate::config::BorderSides;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    symbols,
    widgets::{Block, BorderType, Borders, Widget},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitlePosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl TitlePosition {
    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "top-center" => Self::TopCenter,
            "top-right" => Self::TopRight,
            "bottom-left" => Self::BottomLeft,
            "bottom-center" => Self::BottomCenter,
            "bottom-right" => Self::BottomRight,
            _ => Self::TopLeft,
        }
    }

    pub fn offset(width: u16, title_len: u16, position: TitlePosition) -> u16 {
        match position {
            TitlePosition::TopLeft | TitlePosition::BottomLeft => 0,
            TitlePosition::TopCenter | TitlePosition::BottomCenter => {
                width.saturating_sub(title_len) / 2
            }
            TitlePosition::TopRight | TitlePosition::BottomRight => width.saturating_sub(title_len),
        }
    }
}

/// Render a border block and overlay a custom-positioned title while keeping the border drawn behind it.
/// Returns the inner area (like `Block::inner`).
pub fn render_block_with_title(
    area: Rect,
    buf: &mut Buffer,
    show_border: bool,
    borders: Borders,
    border_sides: &BorderSides,
    border_type: BorderType,
    border_style: Style,
    title: &str,
    title_position: TitlePosition,
) -> Rect {
    // Compute inner area first so callers always get consistent geometry
    let block = Block::default()
        .borders(borders)
        .border_type(border_type)
        .border_style(border_style);

    let inner = if show_border { block.inner(area) } else { area };

    if show_border {
        block.render(area, buf);
    }

    // Don't render title if no border (e.g., TextWindow inside TabbedTextWindow)
    // or if title is empty or area is zero
    if !show_border || title.is_empty() || area.width == 0 || area.height == 0 {
        return inner;
    }

    let left_pad: u16 = if border_sides.left { 1 } else { 0 };
    let right_pad: u16 = if border_sides.right { 1 } else { 0 };
    let available = area
        .width
        .saturating_sub(left_pad.saturating_add(right_pad));

    if available == 0 {
        return inner;
    }

    let title_len = title.chars().count() as u16;
    let trimmed_len = title_len.min(available);
    let offset = TitlePosition::offset(available, trimmed_len, title_position);
    let start_x = area.x + left_pad + offset.min(available.saturating_sub(trimmed_len));
    let title_chars: Vec<char> = title.chars().take(trimmed_len as usize).collect();
    let border_color = border_style.fg.unwrap_or(Color::White);

    let (title_y, line_char) = match title_position {
        TitlePosition::TopLeft | TitlePosition::TopCenter | TitlePosition::TopRight => (
            area.y,
            if border_sides.top {
                Some(match border_type {
                    BorderType::Rounded => symbols::border::ROUNDED.horizontal_top,
                    BorderType::Double => symbols::border::DOUBLE.horizontal_top,
                    BorderType::Thick => symbols::border::THICK.horizontal_top,
                    BorderType::QuadrantInside | BorderType::QuadrantOutside => {
                        symbols::border::QUADRANT_INSIDE.horizontal_top
                    }
                    _ => symbols::border::PLAIN.horizontal_top,
                })
            } else {
                None
            },
        ),
        TitlePosition::BottomLeft
        | TitlePosition::BottomCenter
        | TitlePosition::BottomRight => (
            area.y.saturating_add(area.height.saturating_sub(1)),
            if border_sides.bottom {
                Some(match border_type {
                    BorderType::Rounded => symbols::border::ROUNDED.horizontal_bottom,
                    BorderType::Double => symbols::border::DOUBLE.horizontal_bottom,
                    BorderType::Thick => symbols::border::THICK.horizontal_bottom,
                    BorderType::QuadrantInside | BorderType::QuadrantOutside => {
                        symbols::border::QUADRANT_INSIDE.horizontal_bottom
                    }
                    _ => symbols::border::PLAIN.horizontal_bottom,
                })
            } else {
                None
            },
        ),
    };

    // Always clear/redraw the title area to prevent old characters from persisting
    // when title length changes (e.g., [64] -> [126])
    let start_line_x = area.x + left_pad;
    let end_line_x = area
        .x
        .saturating_add(area.width.saturating_sub(right_pad.saturating_add(1)));
    if end_line_x >= start_line_x {
        let clear_char = if let Some(lc) = line_char {
            lc.chars().next().unwrap_or(' ')
        } else {
            ' ' // Use space if no border
        };
        for x in start_line_x..=end_line_x {
            buf[(x, title_y)]
                .set_char(clear_char)
                .set_style(Style::default().fg(border_color));
        }
    }

    for (idx, ch) in title_chars.into_iter().enumerate() {
        let x = start_x + idx as u16;
        if x < area.x + area.width {
            buf[(x, title_y)]
                .set_char(ch)
                .set_style(Style::default().fg(border_color).add_modifier(border_style.add_modifier));
        }
    }

    inner
}
