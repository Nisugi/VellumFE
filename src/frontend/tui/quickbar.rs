//! Quickbar widget for rendering quickbar entries in a single row.

use crate::data::QuickbarEntry;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Clear, Widget as RatatuiWidget},
};

#[derive(Clone, Debug)]
pub enum QuickbarAction {
    OpenSwitcher,
    ExecuteCommand(String),
    MenuRequest { exist: String, noun: String },
}

#[derive(Clone, Debug)]
struct RenderedItem {
    start: u16,
    end: u16,
    selectable_index: Option<usize>,
    entry_index: Option<usize>,
    is_switcher: bool,
}

pub struct Quickbar {
    entries: Vec<QuickbarEntry>,
    selected_index: usize,
    selectable_count: usize,
    visible_selectable_count: usize,
    rendered_items: Vec<RenderedItem>,

    title: String,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<Color>,
    border_sides: crate::config::BorderSides,
    background_color: Option<Color>,
    transparent_background: bool,
    text_color: Option<Color>,
    selection_fg: Option<Color>,
    selection_bg: Option<Color>,
}

impl Quickbar {
    pub fn new(title: &str) -> Self {
        Self {
            entries: Vec::new(),
            selected_index: 0,
            selectable_count: 1,
            visible_selectable_count: 1,
            rendered_items: Vec::new(),
            title: title.to_string(),
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(),
            background_color: None,
            transparent_background: false,
            text_color: None,
            selection_fg: None,
            selection_bg: None,
        }
    }

    pub fn set_entries(&mut self, entries: Vec<QuickbarEntry>) {
        self.entries = entries;
        self.selectable_count = 1
            + self
                .entries
                .iter()
                .filter(|e| matches!(e, QuickbarEntry::Link { .. } | QuickbarEntry::MenuLink { .. }))
                .count();
        self.visible_selectable_count = self.selectable_count;
        if self.selected_index >= self.selectable_count {
            self.selected_index = self.selectable_count.saturating_sub(1);
        }
    }

    pub fn reset_selection(&mut self) {
        self.selected_index = 0;
    }

    pub fn move_selection(&mut self, delta: i32) {
        let count = self.visible_selectable_count.max(1);
        let mut next = self.selected_index as i32 + delta;
        if next < 0 {
            next = count as i32 - 1;
        } else if next >= count as i32 {
            next = 0;
        }
        self.selected_index = next as usize;
    }

    pub fn activate_selected(&self) -> Option<QuickbarAction> {
        if self.selected_index == 0 {
            return Some(QuickbarAction::OpenSwitcher);
        }

        let mut selectable_index = 0;
        for entry in &self.entries {
            if !matches!(entry, QuickbarEntry::Link { .. } | QuickbarEntry::MenuLink { .. }) {
                continue;
            }
            selectable_index += 1;
            if selectable_index == self.selected_index {
                return Some(Self::entry_action(entry));
            }
        }

        None
    }

    pub fn handle_click(&mut self, x: u16, y: u16, area: Rect) -> Option<QuickbarAction> {
        let inner = self.inner_rect(area);
        if inner.width == 0 || inner.height == 0 {
            return None;
        }
        if y != inner.y {
            return None;
        }
        if x < inner.x || x >= inner.x + inner.width {
            return None;
        }

        let local_x = x - inner.x;
        for item in &self.rendered_items {
            if local_x >= item.start && local_x < item.end {
                if let Some(selectable_index) = item.selectable_index {
                    self.selected_index = selectable_index;
                }
                if item.is_switcher {
                    return Some(QuickbarAction::OpenSwitcher);
                }
                if let Some(entry_index) = item.entry_index {
                    if let Some(entry) = self.entries.get(entry_index) {
                        if matches!(entry, QuickbarEntry::Link { .. } | QuickbarEntry::MenuLink { .. }) {
                            return Some(Self::entry_action(entry));
                        }
                    }
                }
                return None;
            }
        }
        None
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

    pub fn set_selection_colors(&mut self, fg: Option<String>, bg: Option<String>) {
        self.selection_fg = fg.and_then(|c| Self::parse_color(&c));
        self.selection_bg = bg.and_then(|c| Self::parse_color(&c));
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
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

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let (items, visible_selectable) = self.build_layout(inner.width);
        self.rendered_items = items;
        self.visible_selectable_count = visible_selectable.max(1);
        if self.selected_index >= self.visible_selectable_count {
            self.selected_index = self.visible_selectable_count.saturating_sub(1);
        }

        let row_y = inner.y;
        for item in &self.rendered_items {
            let is_selected = focused
                && item.selectable_index.is_some_and(|idx| idx == self.selected_index);
            let style = self.item_style(is_selected);
            let label = if item.is_switcher {
                ">>"
            } else if let Some(entry_index) = item.entry_index {
                Self::entry_label(&self.entries[entry_index])
            } else {
                Self::separator_label()
            };

            self.render_text(
                buf,
                inner.x + item.start,
                row_y,
                label,
                style,
                inner.x + inner.width,
            );
        }
    }

    fn build_layout(&self, inner_width: u16) -> (Vec<RenderedItem>, usize) {
        let mut items = Vec::new();
        let mut cursor = 0u16;
        let mut selectable_index = 0usize;

        let switcher_len = 2u16;
        if cursor + switcher_len <= inner_width {
            items.push(RenderedItem {
                start: cursor,
                end: cursor + switcher_len,
                selectable_index: Some(selectable_index),
                entry_index: None,
                is_switcher: true,
            });
            cursor += switcher_len;
            selectable_index += 1;
        } else {
            return (items, selectable_index);
        }

        if cursor < inner_width {
            cursor += 1;
        }

        for (idx, entry) in self.entries.iter().enumerate() {
            let label = Self::entry_label(entry);
            let label_len = label.chars().count() as u16;
            if label_len == 0 {
                continue;
            }
            if cursor + label_len > inner_width {
                break;
            }

            let is_selectable = matches!(entry, QuickbarEntry::Link { .. } | QuickbarEntry::MenuLink { .. });
            let item_selectable = if is_selectable { Some(selectable_index) } else { None };
            if is_selectable {
                selectable_index += 1;
            }

            items.push(RenderedItem {
                start: cursor,
                end: cursor + label_len,
                selectable_index: item_selectable,
                entry_index: Some(idx),
                is_switcher: false,
            });
            cursor += label_len;
        }

        (items, selectable_index)
    }

    fn entry_label(entry: &QuickbarEntry) -> &str {
        match entry {
            QuickbarEntry::Label { value, .. } => value.as_str(),
            QuickbarEntry::Link { value, .. } => value.as_str(),
            QuickbarEntry::MenuLink { value, .. } => value.as_str(),
            QuickbarEntry::Separator => Self::separator_label(),
        }
    }

    fn separator_label() -> &'static str {
        " | "
    }

    fn entry_action(entry: &QuickbarEntry) -> QuickbarAction {
        match entry {
            QuickbarEntry::Link { cmd, .. } => {
                QuickbarAction::ExecuteCommand(format!("{}\n", cmd))
            }
            QuickbarEntry::MenuLink { exist, noun, .. } => {
                QuickbarAction::MenuRequest {
                    exist: exist.clone(),
                    noun: noun.clone(),
                }
            }
            QuickbarEntry::Label { .. } => QuickbarAction::OpenSwitcher,
            QuickbarEntry::Separator => QuickbarAction::OpenSwitcher,
        }
    }

    fn item_style(&self, selected: bool) -> Style {
        let mut style = Style::default();
        if let Some(fg) = self.text_color {
            style = style.fg(fg);
        }
        if let Some(bg) = self.background_color {
            if !self.transparent_background {
                style = style.bg(bg);
            }
        }
        if selected {
            if let Some(fg) = self.selection_fg {
                style = style.fg(fg);
            }
            if let Some(bg) = self.selection_bg {
                style = style.bg(bg);
            }
            style = style.add_modifier(Modifier::BOLD);
        }
        style
    }

    fn render_text(
        &self,
        buf: &mut Buffer,
        x: u16,
        y: u16,
        text: &str,
        style: Style,
        max_x: u16,
    ) {
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
        self.build_block().inner(area)
    }

    fn parse_color(hex: &str) -> Option<Color> {
        super::colors::parse_color_to_ratatui(hex)
    }

    fn build_block(&self) -> Block<'_> {
        let mut block = Block::default();
        if self.show_border {
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
            block = block.borders(borders);
            if let Some(ref style) = self.border_style {
                let border_type = match style.as_str() {
                    "double" => BorderType::Double,
                    "rounded" => BorderType::Rounded,
                    "thick" => BorderType::Thick,
                    _ => BorderType::Plain,
                };
                block = block.border_type(border_type);
            }
        }
        if !self.title.is_empty() && self.show_border {
            block = block.title(self.title.as_str());
        }
        block
    }
}
