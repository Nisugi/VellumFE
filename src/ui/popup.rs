use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
};

/// Shared popup state that encapsulates drag handling and positioning.
#[derive(Debug, Clone)]
pub struct PopupState {
    pub x: u16,
    pub y: u16,
    is_dragging: bool,
    drag_offset_x: u16,
    drag_offset_y: u16,
}

impl PopupState {
    pub fn new(x: u16, y: u16) -> Self {
        Self {
            x,
            y,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn rect(&self, width: u16, height: u16) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width,
            height,
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    pub fn handle_mouse(
        &mut self,
        mouse_col: u16,
        mouse_row: u16,
        mouse_down: bool,
        bounds: Rect,
        size: (u16, u16),
    ) -> bool {
        let (width, height) = size;
        let hit_title = self.is_on_title_bar(mouse_col, mouse_row, width);

        if mouse_down && hit_title && !self.is_dragging {
            self.is_dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.x);
            self.drag_offset_y = mouse_row.saturating_sub(self.y);
            return true;
        }

        if self.is_dragging {
            if mouse_down {
                let new_x = mouse_col.saturating_sub(self.drag_offset_x);
                let new_y = mouse_row.saturating_sub(self.drag_offset_y);
                self.x = clamp_axis(new_x, bounds.x, bounds.width, width);
                self.y = clamp_axis(new_y, bounds.y, bounds.height, height);
                return true;
            } else {
                self.is_dragging = false;
                return true;
            }
        }

        false
    }

    fn is_on_title_bar(&self, col: u16, row: u16, width: u16) -> bool {
        if width <= 1 {
            return false;
        }

        row == self.y && col > self.x && col < self.x.saturating_add(width).saturating_sub(1)
    }
}

fn clamp_axis(position: u16, origin: u16, bound: u16, size: u16) -> u16 {
    if bound <= size {
        origin
    } else {
        let min = origin as u32;
        let available = (bound as u32).saturating_sub(size as u32);
        let max = min + available;
        let value = position as u32;
        let clamped = value.max(min).min(max);
        clamped as u16
    }
}

/// Draw the popup frame (background + border + title).
pub fn render_popup_frame(
    buf: &mut Buffer,
    state: &PopupState,
    bounds: Rect,
    width: u16,
    height: u16,
    title: &str,
    background_style: Style,
    border_style: Style,
    title_style: Style,
) {
    if width == 0 || height == 0 {
        return;
    }

    let rect = state.rect(width, height);

    for row in rect.y..rect.y.saturating_add(height) {
        if row >= bounds.y.saturating_add(bounds.height) {
            break;
        }

        for col in rect.x..rect.x.saturating_add(width) {
            if col >= bounds.x.saturating_add(bounds.width) {
                break;
            }
            buf.set_string(col, row, " ", background_style);
        }
    }

    let horizontal = if width > 2 {
        "-".repeat((width - 2) as usize)
    } else {
        String::new()
    };

    buf.set_string(rect.x, rect.y, "+", border_style);
    if !horizontal.is_empty() {
        buf.set_string(rect.x + 1, rect.y, &horizontal, border_style);
    }
    if width > 1 {
        buf.set_string(rect.x + width - 1, rect.y, "+", border_style);
    }

    for row in 1..height.saturating_sub(1) {
        buf.set_string(rect.x, rect.y + row, "|", border_style);
        if width > 1 {
            buf.set_string(rect.x + width - 1, rect.y + row, "|", border_style);
        }
    }

    if height > 1 {
        buf.set_string(rect.x, rect.y + height - 1, "+", border_style);
        if !horizontal.is_empty() {
            buf.set_string(rect.x + 1, rect.y + height - 1, &horizontal, border_style);
        }
        if width > 1 {
            buf.set_string(rect.x + width - 1, rect.y + height - 1, "+", border_style);
        }
    }

    if width > 4 {
        let title_x = rect.x + 2;
        let max_len = (width - 4) as usize;
        let show_title = if title.len() > max_len {
            &title[..max_len]
        } else {
            title
        };
        buf.set_string(title_x, rect.y, show_title, title_style);
    }
}
