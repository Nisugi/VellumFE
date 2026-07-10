//! Hotkey bar widget: a row/column of command buttons resolved by
//! core::hotbar (state colors, dim, countdown overlays) and clickable
//! via the same hit-testing scheme as the quickbar.

use crate::core::hotbar::ResolvedHotbarButton;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Clear, Widget as RatatuiWidget},
};

#[derive(Clone, Debug)]
struct RenderedItem {
    start: u16,
    end: u16,
    row: u16,
    button_index: usize,
}

pub struct HotkeyBar {
    buttons: Vec<ResolvedHotbarButton>,
    rendered_items: Vec<RenderedItem>,
    vertical: bool,

    title: String,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<Color>,
    border_sides: crate::config::BorderSides,
    background_color: Option<Color>,
    transparent_background: bool,
    text_color: Option<Color>,
}

impl HotkeyBar {
    pub fn new(title: &str) -> Self {
        Self {
            buttons: Vec::new(),
            rendered_items: Vec::new(),
            vertical: false,
            title: title.to_string(),
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(),
            background_color: None,
            transparent_background: false,
            text_color: None,
        }
    }

    pub fn set_buttons(&mut self, buttons: Vec<ResolvedHotbarButton>) {
        self.buttons = buttons;
    }

    pub fn set_vertical(&mut self, vertical: bool) {
        self.vertical = vertical;
    }

    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn set_border_config(&mut self, show: bool, style: Option<String>, color: Option<String>) {
        self.show_border = show;
        self.border_style = style;
        self.border_color = color.and_then(|c| Self::parse_color(&c));
    }

    pub fn set_border_sides(&mut self, sides: crate::config::BorderSides) {
        self.border_sides = sides;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color.and_then(|c| {
            let trimmed = c.trim().to_string();
            if trimmed.is_empty() || trimmed == "-" {
                None
            } else {
                Self::parse_color(&trimmed)
            }
        });
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color.and_then(|c| Self::parse_color(&c));
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    /// Returns the clicked button's command (without newline).
    pub fn handle_click(&mut self, x: u16, y: u16, area: Rect) -> Option<String> {
        let inner = self.inner_rect(area);
        if inner.width == 0 || inner.height == 0 {
            return None;
        }
        if x < inner.x || x >= inner.x + inner.width || y < inner.y {
            return None;
        }

        let local_x = x - inner.x;
        let local_y = y - inner.y;
        for item in &self.rendered_items {
            if local_y == item.row && local_x >= item.start && local_x < item.end {
                return self.buttons.get(item.button_index).map(|b| b.command.clone());
            }
        }
        None
    }

    /// Button face text: "[Label]" or "[Label 12s]" while a countdown runs.
    fn button_text(button: &ResolvedHotbarButton) -> String {
        match button.countdown_secs {
            Some(secs) if secs > 0 => format!("[{} {}s]", button.label, secs),
            _ => format!("[{}]", button.label),
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, _focused: bool) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        Clear.render(area, buf);

        if !self.transparent_background {
            if let Some(bg_color) = self.background_color {
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
        }

        let block = self.build_block_titled();
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        self.rendered_items = self.build_layout(inner.width, inner.height);

        for item in &self.rendered_items {
            let button = &self.buttons[item.button_index];
            let text = Self::button_text(button);
            self.render_text(
                buf,
                inner.x + item.start,
                inner.y + item.row,
                &text,
                self.button_style(button),
                inner.x + inner.width,
            );
        }
    }

    /// Horizontal: buttons flow left-to-right with one-space gaps, wrapping
    /// onto additional inner rows. Vertical: one button per row.
    fn build_layout(&self, inner_width: u16, inner_height: u16) -> Vec<RenderedItem> {
        let mut items = Vec::new();

        if self.vertical {
            for (idx, button) in self.buttons.iter().enumerate() {
                let row = idx as u16;
                if row >= inner_height {
                    break;
                }
                let len = (Self::button_text(button).chars().count() as u16).min(inner_width);
                items.push(RenderedItem {
                    start: 0,
                    end: len,
                    row,
                    button_index: idx,
                });
            }
            return items;
        }

        let mut cursor = 0u16;
        let mut row = 0u16;
        for (idx, button) in self.buttons.iter().enumerate() {
            let len = Self::button_text(button).chars().count() as u16;
            if len == 0 || len > inner_width {
                continue;
            }
            if cursor + len > inner_width {
                row += 1;
                cursor = 0;
            }
            if row >= inner_height {
                break;
            }
            items.push(RenderedItem {
                start: cursor,
                end: cursor + len,
                row,
                button_index: idx,
            });
            cursor += len + 1;
        }
        items
    }

    fn button_style(&self, button: &ResolvedHotbarButton) -> Style {
        let mut style = Style::default();
        if let Some(fg) = button
            .fg
            .as_deref()
            .and_then(Self::parse_color)
            .or(self.text_color)
        {
            style = style.fg(fg);
        }
        if let Some(bg) = button.bg.as_deref().and_then(Self::parse_color) {
            style = style.bg(bg);
        } else if let Some(bg) = self.background_color {
            if !self.transparent_background {
                style = style.bg(bg);
            }
        }
        if button.dim {
            style = style.add_modifier(Modifier::DIM);
        }
        style
    }

    fn render_text(&self, buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style, max_x: u16) {
        let mut cursor = x;
        for ch in text.chars() {
            if cursor >= max_x {
                break;
            }
            if cursor < buf.area().width && y < buf.area().height {
                buf[(cursor, y)]
                    .set_symbol(ch.encode_utf8(&mut [0; 4]))
                    .set_style(style);
            }
            cursor += 1;
        }
    }

    fn inner_rect(&self, area: Rect) -> Rect {
        self.build_block_titled().inner(area)
    }

    fn parse_color(hex: &str) -> Option<Color> {
        super::colors::parse_color_to_ratatui(hex)
    }

    fn build_block_titled(&self) -> Block<'_> {
        let mut block = Block::default();
        if self.show_border {
            let border_color = self.border_color.unwrap_or(Color::White);
            let mut borders = Borders::empty();
            if self.border_sides.top {
                borders |= Borders::TOP;
            }
            if self.border_sides.bottom {
                borders |= Borders::BOTTOM;
            }
            if self.border_sides.left {
                borders |= Borders::LEFT;
            }
            if self.border_sides.right {
                borders |= Borders::RIGHT;
            }
            block = block
                .borders(borders)
                .border_style(Style::default().fg(border_color));
            if let Some(ref style) = self.border_style {
                let border_type = match style.as_str() {
                    "double" => BorderType::Double,
                    "rounded" => BorderType::Rounded,
                    "thick" => BorderType::Thick,
                    _ => BorderType::Plain,
                };
                block = block.border_type(border_type);
            }
            if !self.title.is_empty() {
                block = block.title(self.title.as_str());
            }
        }
        block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(id: &str, label: &str, command: &str) -> ResolvedHotbarButton {
        ResolvedHotbarButton {
            id: id.to_string(),
            label: label.to_string(),
            command: command.to_string(),
            tooltip: None,
            hotkey: None,
            fg: None,
            bg: None,
            dim: false,
            countdown_secs: None,
        }
    }

    #[test]
    fn click_hits_button_and_returns_command() {
        let mut bar = HotkeyBar::new("test");
        bar.set_border_config(false, None, None);
        bar.set_buttons(vec![button("a", "Look", "look"), button("b", "Hide", "hide")]);

        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf, false);

        // Layout: "[Look] [Hide]" -> [Look] at 0..6, [Hide] at 7..13
        assert_eq!(bar.handle_click(1, 0, area).as_deref(), Some("look"));
        assert_eq!(bar.handle_click(8, 0, area).as_deref(), Some("hide"));
        // Gap between buttons hits nothing
        assert_eq!(bar.handle_click(6, 0, area), None);
    }

    #[test]
    fn horizontal_wraps_to_next_row() {
        let mut bar = HotkeyBar::new("test");
        bar.set_border_config(false, None, None);
        bar.set_buttons(vec![
            button("a", "Attack", "attack"),
            button("b", "Defend", "defend"),
        ]);

        // Width fits only one "[Attack]" (8 chars) per row
        let area = Rect::new(0, 0, 10, 2);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf, false);

        assert_eq!(bar.handle_click(0, 0, area).as_deref(), Some("attack"));
        assert_eq!(bar.handle_click(0, 1, area).as_deref(), Some("defend"));
    }

    #[test]
    fn vertical_one_per_row() {
        let mut bar = HotkeyBar::new("test");
        bar.set_border_config(false, None, None);
        bar.set_vertical(true);
        bar.set_buttons(vec![button("a", "Look", "look"), button("b", "Hide", "hide")]);

        let area = Rect::new(0, 0, 20, 3);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf, false);

        assert_eq!(bar.handle_click(0, 0, area).as_deref(), Some("look"));
        assert_eq!(bar.handle_click(0, 1, area).as_deref(), Some("hide"));
        assert_eq!(bar.handle_click(0, 2, area), None);
    }

    #[test]
    fn countdown_suffix_in_button_text() {
        let mut b = button("a", "909", "incant 909");
        assert_eq!(HotkeyBar::button_text(&b), "[909]");
        b.countdown_secs = Some(12);
        assert_eq!(HotkeyBar::button_text(&b), "[909 12s]");
        b.countdown_secs = Some(0);
        assert_eq!(HotkeyBar::button_text(&b), "[909]");
    }
}
