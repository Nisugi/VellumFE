//! Multi-tab wrapper around `TextWindow` for stream multiplexing.
//!
//! Handles unread counts, tab bar placement, and themed chrome while delegating
//! actual text rendering to the existing `TextWindow`.

use super::text_window::TextWindow;
use super::title_position::{self, TitlePosition};
use crate::selection::SelectionState;
use crate::theme::AppTheme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType},
};
use std::time::Duration;

use super::crossterm_bridge;

#[derive(Clone, Debug, PartialEq)]
pub enum TabBarPosition {
    Top,
    Bottom,
}

impl TabBarPosition {
    pub(crate) fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "bottom" => Self::Bottom,
            _ => Self::Top,
        }
    }
}

struct TabInfo {
    name: String,
    streams: Vec<String>,
    window: TextWindow,
    has_unread: bool,
    unread_count: usize,
    ignore_activity: bool,
}

pub struct TabbedTextWindow {
    tabs: Vec<TabInfo>,
    active_tab_index: usize,
    tab_bar_position: TabBarPosition,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: crate::config::BorderSides,
    title: String,
    transparent_background: bool,
    background_color: Option<String>,
    content_text_color: Option<String>,
    tab_active_color: Option<String>,
    tab_inactive_color: Option<String>,
    tab_unread_color: Option<String>,
    tab_unread_prefix: String,
    show_tab_separator: bool,
    title_position: TitlePosition,
}

impl TabbedTextWindow {
    pub fn new(title: &str, tab_bar_position: TabBarPosition) -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_index: 0,
            tab_bar_position,
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: Some("#808080".to_string()),
            border_sides: crate::config::BorderSides::default(),
            title: title.to_string(),
            transparent_background: false,
            background_color: None,
            content_text_color: None,
            tab_active_color: Some("#FFFF00".to_string()), // Yellow
            tab_inactive_color: Some("#808080".to_string()), // Gray
            tab_unread_color: Some("#FFFFFF".to_string()), // White
            tab_unread_prefix: "* ".to_string(),
            show_tab_separator: false,
            title_position: TitlePosition::TopLeft,
        }
    }

    // Note: set_highlights and set_replace_enabled removed - highlights now applied in core

    pub fn with_tabs(
        title: &str,
        tabs: Vec<(String, Vec<String>, bool, bool)>,
        max_lines_per_tab: usize,
    ) -> Self {
        let mut window = Self::new(title, TabBarPosition::Top);
        for (name, streams, show_timestamps, ignore_activity) in tabs {
            window.add_tab(
                name,
                streams,
                max_lines_per_tab,
                show_timestamps,
                ignore_activity,
            );
        }
        window
    }

    pub fn with_border_config(
        mut self,
        show: bool,
        style: Option<String>,
        color: Option<String>,
    ) -> Self {
        self.show_border = show;
        self.border_style = style;
        self.border_color = color;
        self
    }

    pub fn with_tab_bar_position(mut self, position: TabBarPosition) -> Self {
        self.set_tab_bar_position(position);
        self
    }

    pub fn with_title_position(mut self, position: TitlePosition) -> Self {
        self.title_position = position;
        self
    }

    pub fn set_tab_bar_position(&mut self, position: TabBarPosition) {
        self.tab_bar_position = position;
    }

    pub fn with_tab_colors(
        mut self,
        active: Option<String>,
        inactive: Option<String>,
        unread: Option<String>,
    ) -> Self {
        self.tab_active_color = active;
        self.tab_inactive_color = inactive;
        self.tab_unread_color = unread;
        self
    }

    pub fn with_unread_prefix(mut self, prefix: String) -> Self {
        self.tab_unread_prefix = prefix;
        self
    }

    pub fn set_tab_colors(
        &mut self,
        active: Option<String>,
        inactive: Option<String>,
        unread: Option<String>,
    ) {
        self.tab_active_color = active;
        self.tab_inactive_color = inactive;
        self.tab_unread_color = unread;
    }

    pub fn set_unread_prefix(&mut self, prefix: String) {
        self.tab_unread_prefix = prefix;
    }

    pub fn set_title_position(&mut self, position: TitlePosition) {
        self.title_position = position;
    }
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn set_tab_separator(&mut self, show: bool) {
        self.show_tab_separator = show;
    }

    pub fn set_content_align(&mut self, align: Option<String>) {
        for tab in &mut self.tabs {
            tab.window.set_content_align(align.clone());
        }
    }

    pub fn set_tab_ignore_activity(&mut self, index: usize, ignore: bool) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.ignore_activity = ignore;
            if ignore {
                tab.has_unread = false;
                tab.unread_count = 0;
            }
        }
    }

    /// Handle a mouse click; returns true if it activated a tab.
    pub fn handle_mouse_click(
        &mut self,
        window_rect: ratatui::layout::Rect,
        mouse_col: u16,
        mouse_row: u16,
    ) -> bool {
        let tab_bar = self.tab_bar_rect(window_rect);
        if mouse_col < tab_bar.x
            || mouse_col >= tab_bar.x + tab_bar.width
            || mouse_row != tab_bar.y
        {
            return false;
        }

        if let Some(idx) = self.get_tab_at_position(mouse_col, tab_bar) {
            self.switch_to_tab(idx);
            return true;
        }

        false
    }

    pub fn add_tab(
        &mut self,
        name: String,
        streams: Vec<String>,
        max_lines: usize,
        show_timestamps: bool,
        ignore_activity: bool,
    ) {
        let mut window = TextWindow::new(&name, max_lines);
        window.set_show_timestamps(show_timestamps);
        window.set_border_config(false, None, None); // Tabs don't have their own borders
        // Hide per-tab title; the tab bar already shows the tab label
        window.set_title(String::new());
        window.set_background_color(self.background_color.clone());
        window.set_text_color(self.content_text_color.clone());

        self.tabs.push(TabInfo {
            name,
            streams,
            window,
            has_unread: false,
            unread_count: 0,
            ignore_activity,
        });
    }

    pub fn remove_tab(&mut self, name: &str) -> bool {
        // Can't remove the last tab
        if self.tabs.len() <= 1 {
            return false;
        }

        if let Some(idx) = self.tabs.iter().position(|t| t.name == name) {
            self.tabs.remove(idx);

            // Adjust active index if needed
            if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len().saturating_sub(1);
            }
            true
        } else {
            false
        }
    }

    pub fn rename_tab(&mut self, old_name: &str, new_name: String) -> bool {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.name == old_name) {
            tab.name = new_name;
            true
        } else {
            false
        }
    }

    pub fn switch_to_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            // Clear unread status
            if let Some(tab) = self.tabs.get_mut(index) {
                tab.has_unread = false;
                tab.unread_count = 0;
            }
        }
    }

    pub fn switch_to_tab_by_name(&mut self, name: &str) {
        if let Some(idx) = self.tabs.iter().position(|t| t.name == name) {
            self.switch_to_tab(idx);
        }
    }

    pub fn get_tab_names(&self) -> Vec<String> {
        self.tabs.iter().map(|t| t.name.clone()).collect()
    }

    pub fn get_tab_window_mut(&mut self, index: usize) -> Option<&mut TextWindow> {
        self.tabs.get_mut(index).map(|ti| &mut ti.window)
    }

    /// Drain wrap timing samples across all tabs for performance stats
    pub fn take_wrap_samples(&mut self) -> Vec<Duration> {
        let mut samples = Vec::new();
        for tab in self.tabs.iter_mut() {
            samples.extend(tab.window.take_wrap_samples());
        }
        samples
    }

    /// Switch to the next tab (wraps around to first tab)
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            let next_index = (self.active_tab_index + 1) % self.tabs.len();
            self.switch_to_tab(next_index);
        }
    }

    /// Switch to the previous tab (wraps around to last tab)
    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            let prev_index = if self.active_tab_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab_index - 1
            };
            self.switch_to_tab(prev_index);
        }
    }

    /// Switch to the next tab with unread messages
    /// Returns true if found and switched, false if no unread tabs
    pub fn next_tab_with_unread(&mut self) -> bool {
        if self.tabs.is_empty() {
            return false;
        }

        // Search for next unread tab starting from current position
        let start_index = (self.active_tab_index + 1) % self.tabs.len();

        // First pass: from current+1 to end
        for i in start_index..self.tabs.len() {
            if self.tabs[i].has_unread {
                self.switch_to_tab(i);
                return true;
            }
        }

        // Second pass: from beginning to current (wrap around)
        for i in 0..start_index {
            if self.tabs[i].has_unread {
                self.switch_to_tab(i);
                return true;
            }
        }

        false
    }

    /// Get current active tab index
    pub fn get_active_tab_index(&self) -> usize {
        self.active_tab_index
    }

    /// Check if any tabs have unread messages
    pub fn has_unread_tabs(&self) -> bool {
        self.tabs.iter().any(|t| t.has_unread)
    }

    /// Total wrapped line count across all tabs (for memory tracking)
    pub fn total_wrapped_line_count(&self) -> usize {
        self.tabs
            .iter()
            .map(|t| t.window.wrapped_line_count())
            .sum()
    }

    pub fn reorder_tabs(&mut self, new_order: &[String]) {
        let active_name = self.tabs.get(self.active_tab_index).map(|t| t.name.clone());
        let mut new_tabs = Vec::new();
        for name in new_order {
            if let Some(idx) = self.tabs.iter().position(|t| &t.name == name) {
                new_tabs.push(self.tabs.remove(idx));
            }
        }
        // Add any tabs that weren't in the new order
        new_tabs.append(&mut self.tabs);
        self.tabs = new_tabs;

        // Reset active index to previously active tab if it still exists
        if let Some(active_name) = active_name {
            if let Some(new_idx) = self.tabs.iter().position(|t| t.name == active_name) {
                self.active_tab_index = new_idx;
            } else {
                self.active_tab_index = 0;
            }
        } else {
            self.active_tab_index = 0;
        }
    }

    /// Mark a tab as having unread content, incrementing its counter by `count`
    pub fn mark_tab_unread(&mut self, index: usize, count: usize) {
        if let Some(tab) = self.tabs.get_mut(index) {
            if count > 0 && !tab.ignore_activity {
                tab.has_unread = true;
                tab.unread_count = tab.unread_count.saturating_add(count);
            }
        }
    }

    pub fn add_text_to_stream(&mut self, stream: &str, styled: super::text_window::StyledText) {
        for (idx, tab) in self.tabs.iter_mut().enumerate() {
            if tab.streams.contains(&stream.to_string()) {
                tab.window.add_text(styled.clone());

                // Mark as unread if not active tab
                if idx != self.active_tab_index && !tab.ignore_activity {
                    tab.has_unread = true;
                    tab.unread_count += 1;
                }
            }
        }
    }

    pub fn finish_line_for_stream(&mut self, stream: &str, width: u16) {
        for tab in &mut self.tabs {
            if tab.streams.contains(&stream.to_string()) {
                tab.window.finish_line(width);
            }
        }
    }

    pub fn add_text_to_tab(&mut self, tab_name: &str, styled: super::text_window::StyledText) {
        if let Some((idx, tab)) = self
            .tabs
            .iter_mut()
            .enumerate()
            .find(|(_, t)| t.name == tab_name)
        {
            tab.window.add_text(styled);

            // Mark as unread if not active tab
            if idx != self.active_tab_index && !tab.ignore_activity {
                tab.has_unread = true;
                tab.unread_count += 1;
            }
        }
    }

    pub fn finish_line_for_tab(&mut self, tab_name: &str, width: u16) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.name == tab_name) {
            tab.window.finish_line(width);
        }
    }

    pub fn get_all_streams(&self) -> Vec<String> {
        self.tabs.iter().flat_map(|t| t.streams.clone()).collect()
    }

    pub fn clear_stream(&mut self, stream: &str) {
        for tab in &mut self.tabs {
            if tab.streams.contains(&stream.to_string()) {
                tab.window.clear();
                tab.has_unread = false;
                tab.unread_count = 0;
            }
        }
    }

    pub fn scroll_up(&mut self, amount: usize) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.scroll_up(amount);
        }
    }

    pub fn scroll_down(&mut self, amount: usize) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.scroll_down(amount);
        }
    }

    pub fn start_search(&mut self, pattern: &str) -> Result<usize, regex::Error> {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.start_search(pattern)
        } else {
            Ok(0)
        }
    }

    pub fn clear_search(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.clear_search();
        }
    }

    pub fn next_match(&mut self) -> bool {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.next_match()
        } else {
            false
        }
    }

    pub fn prev_match(&mut self) -> bool {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.prev_match()
        } else {
            false
        }
    }

    pub fn search_info(&self) -> Option<(usize, usize)> {
        self.tabs
            .get(self.active_tab_index)
            .and_then(|tab| tab.window.search_info())
    }

    pub fn set_border_config(&mut self, show: bool, style: Option<String>, color: Option<String>) {
        self.show_border = show;
        self.border_style = style;
        self.border_color = color;
    }

    pub fn set_border_sides(&mut self, sides: crate::config::BorderSides) {
        self.border_sides = sides;
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color;
    }

    pub fn apply_window_colors(
        &mut self,
        text_color: Option<String>,
        background_color: Option<String>,
    ) {
        self.content_text_color = text_color.clone();
        self.background_color = background_color.clone();

        for tab in &mut self.tabs {
            tab.window.set_text_color(text_color.clone());
            tab.window.set_background_color(background_color.clone());
        }
    }

    pub fn get_tab_at_position(&self, x: u16, tab_bar_rect: Rect) -> Option<usize> {
        let mut curr_x = tab_bar_rect.x;

        for (idx, tab) in self.tabs.iter().enumerate() {
            if curr_x >= tab_bar_rect.right() {
                break;
            }
            let tab_text = if idx == self.active_tab_index {
                tab.name.clone()
            } else if tab.has_unread {
                format!("{}{}", self.tab_unread_prefix, tab.name)
            } else {
                tab.name.clone()
            };

            let tab_width = tab_text.chars().count() as u16;
            let divider_width = if idx < self.tabs.len() - 1 { 3 } else { 0 }; // " | "

            if x >= curr_x && x < curr_x + tab_width + divider_width {
                return Some(idx);
            }

            curr_x += tab_width + divider_width;
        }

        None
    }

    /// Compute the tab bar and content areas for a given outer window rect
    fn tab_bar_rect(&self, outer: Rect) -> Rect {
        // Match render() logic for border handling
        let inner_area = if self.show_border {
            let mut block = Block::default();
            let border_type = match self
                .border_style
                .as_deref()
                .unwrap_or("single")
                .to_lowercase()
                .as_str()
            {
                "double" => BorderType::Double,
                "rounded" => BorderType::Rounded,
                "thick" => BorderType::Thick,
                "single" => BorderType::Plain, // closest available thin border
                _ => BorderType::Plain,
            };
            let borders = crossterm_bridge::to_ratatui_borders(&self.border_sides);
            block = block.borders(borders).border_type(border_type);
            block.inner(outer)
        } else {
            outer
        };

        match self.tab_bar_position {
            TabBarPosition::Top => Rect {
                x: inner_area.x,
                y: inner_area.y,
                width: inner_area.width,
                height: 1,
            },
            TabBarPosition::Bottom => Rect {
                x: inner_area.x,
                y: inner_area.y + inner_area.height.saturating_sub(1),
                width: inner_area.width,
                height: 1,
            },
        }
    }

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

    pub fn render_with_focus(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        focused: bool,
        selection_state: Option<&SelectionState>,
        selection_bg_color: &str,
        window_index: usize,
        theme: &AppTheme,
    ) {
        if self.tabs.is_empty() {
            return;
        }

        let border_type = match self.border_style.as_deref() {
            Some("double") => BorderType::Double,
            Some("rounded") => BorderType::Rounded,
            Some("thick") => BorderType::Thick,
            Some("single") => BorderType::Plain, // closest available thin border
            _ => BorderType::Plain,
        };

        let borders = crossterm_bridge::to_ratatui_borders(&self.border_sides);

        let mut border_style = Style::default();
        if let Some(ref color_str) = self.border_color {
            let color = Self::parse_color(color_str);
            border_style = border_style.fg(color);
        }

        if focused {
            border_style = border_style
                .fg(crossterm_bridge::to_ratatui_color(theme.window_border_focused))
                .add_modifier(Modifier::BOLD);
        }

        let bg_color = if self.transparent_background {
            None
        } else if let Some(ref bg_hex) = self.background_color {
            Some(Self::parse_color(bg_hex))
        } else {
            Some(crossterm_bridge::to_ratatui_color(theme.window_background))
        };
        if let Some(bg) = bg_color {
            border_style = border_style.bg(bg);
        }

        let inner_area = title_position::render_block_with_title(
            area,
            buf,
            self.show_border,
            borders,
            &self.border_sides,
            border_type,
            border_style,
            &self.title,
            self.title_position,
        );

        // Split inner area for tab bar and content
        let (tab_bar_area, mut content_area) = match self.tab_bar_position {
            TabBarPosition::Top => {
                let tab_bar = Rect {
                    x: inner_area.x,
                    y: inner_area.y,
                    width: inner_area.width,
                    height: 1,
                };
                let content = Rect {
                    x: inner_area.x,
                    y: inner_area.y + 1,
                    width: inner_area.width,
                    height: inner_area.height.saturating_sub(1),
                };
                (tab_bar, content)
            }
            TabBarPosition::Bottom => {
                let content = Rect {
                    x: inner_area.x,
                    y: inner_area.y,
                    width: inner_area.width,
                    height: inner_area.height.saturating_sub(1),
                };
                let tab_bar = Rect {
                    x: inner_area.x,
                    y: inner_area.y + content.height,
                    width: inner_area.width,
                    height: 1,
                };
                (tab_bar, content)
            }
        };

        // Optional separator between tab bar and content
        let mut separator_area: Option<Rect> = None;
        if self.show_tab_separator && content_area.height > 0 {
            match self.tab_bar_position {
                TabBarPosition::Top => {
                    separator_area = Some(Rect {
                        x: content_area.x,
                        y: content_area.y,
                        width: content_area.width,
                        height: 1,
                    });
                    content_area.y = content_area.y.saturating_add(1);
                    content_area.height = content_area.height.saturating_sub(1);
                }
                TabBarPosition::Bottom => {
                    separator_area = Some(Rect {
                        x: content_area.x,
                        y: content_area
                            .y
                            .saturating_add(content_area.height.saturating_sub(1)),
                        width: content_area.width,
                        height: 1,
                    });
                    content_area.height = content_area.height.saturating_sub(1);
                }
            }
        }

        // Paint background for tab bar and content (use theme fallback when needed)
        let bg_color = if self.transparent_background {
            None
        } else if let Some(ref bg_hex) = self.background_color {
            Some(Self::parse_color(bg_hex))
        } else {
            Some(crossterm_bridge::to_ratatui_color(theme.window_background))
        };
        if let Some(bg) = bg_color {
            // Tab bar row
            for dx in 0..tab_bar_area.width {
                let x = tab_bar_area.x + dx;
                let y = tab_bar_area.y;
                if x < buf.area().width && y < buf.area().height {
                    buf[(x, y)].set_bg(bg);
                }
            }

            // Content area
            for dx in 0..content_area.width {
                for dy in 0..content_area.height {
                    let x = content_area.x + dx;
                    let y = content_area.y + dy;
                    if x < buf.area().width && y < buf.area().height {
                        buf[(x, y)].set_bg(bg);
                    }
                }
            }

            if let Some(sep) = separator_area {
                for dx in 0..sep.width {
                    let x = sep.x + dx;
                    let y = sep.y;
                    if x < buf.area().width && y < buf.area().height {
                        buf[(x, y)].set_bg(bg);
                    }
                }
            }
        }

        // Render tab bar
        self.render_tab_bar(tab_bar_area, buf);

        // Render separator line if enabled and space remains
        // Include scroll indicator [N] if the active tab is scrolled
        if let Some(sep) = separator_area {
            if sep.width > 0 && sep.y < buf.area().height {
                let sep_color = self
                    .border_color
                    .as_ref()
                    .map(|c| Self::parse_color(c))
                    .or_else(|| {
                        self.tab_inactive_color
                            .as_ref()
                            .map(|c| Self::parse_color(c))
                    })
                    .unwrap_or(Color::DarkGray);

                // Check if active tab has scroll indicator
                let scroll_indicator = self
                    .tabs
                    .get(self.active_tab_index)
                    .and_then(|tab| tab.window.get_scroll_indicator())
                    .map(|n| format!("[{}]", n));

                // Draw separator line
                for dx in 0..sep.width {
                    let x = sep.x + dx;
                    if x < buf.area().width {
                        buf[(x, sep.y)]
                            .set_char('-')
                            .set_style(Style::default().fg(sep_color));
                    }
                }

                // Draw scroll indicator on left side of separator if scrolled
                if let Some(indicator) = scroll_indicator {
                    for (i, ch) in indicator.chars().enumerate() {
                        let x = sep.x + i as u16;
                        if x < sep.x + sep.width && x < buf.area().width {
                            buf[(x, sep.y)]
                                .set_char(ch)
                                .set_style(Style::default().fg(sep_color));
                        }
                    }
                }
            }
        }

        // Render active tab content
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.render_with_focus(
                content_area,
                buf,
                focused,
                selection_state,
                selection_bg_color,
                window_index,
                theme,
            );
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let theme = crate::theme::ThemePresets::dark();
        self.render_with_focus(area, buf, false, None, "#4a4a4a", 0, &theme);
    }

    fn render_tab_bar(&self, area: Rect, buf: &mut Buffer) {
        let active_color = self
            .tab_active_color
            .as_ref()
            .map(|c| Self::parse_color(c))
            .unwrap_or(Color::Yellow);
        let inactive_color = self
            .tab_inactive_color
            .as_ref()
            .map(|c| Self::parse_color(c))
            .unwrap_or(Color::DarkGray);
        let unread_color = self
            .tab_unread_color
            .as_ref()
            .map(|c| Self::parse_color(c))
            .unwrap_or(Color::White);

        let mut x = area.x;
        let divider = " | ";

        for (idx, tab) in self.tabs.iter().enumerate() {
            if x >= area.right() {
                break;
            }

            // Determine tab text and style
            let ignore_activity = tab.ignore_activity;
            let (raw_text, style) = if idx == self.active_tab_index {
                (
                    tab.name.clone(),
                    Style::default()
                        .fg(active_color)
                        .add_modifier(Modifier::BOLD),
                )
            } else if tab.has_unread && !ignore_activity {
                (
                    format!("{}{}", self.tab_unread_prefix, tab.name),
                    Style::default().fg(unread_color),
                )
            } else {
                (tab.name.clone(), Style::default().fg(inactive_color))
            };

            // Compute available width for this tab (leave space for divider if needed)
            let remaining = area.right().saturating_sub(x);
            if remaining == 0 {
                break;
            }
            let max_label_width = if idx < self.tabs.len() - 1 && remaining > divider.len() as u16 {
                remaining.saturating_sub(divider.len() as u16)
            } else {
                remaining
            };

            // Truncate with ellipsis if needed
            let tab_text = if raw_text.chars().count() as u16 > max_label_width {
                if max_label_width == 0 {
                    String::new()
                } else if max_label_width == 1 {
                    "…".to_string()
                } else if max_label_width == 2 {
                    "…".repeat(2)
                } else {
                    let take_len = max_label_width.saturating_sub(1) as usize;
                    let mut truncated = String::new();
                    for ch in raw_text.chars().take(take_len) {
                        truncated.push(ch);
                    }
                    truncated.push('…');
                    truncated
                }
            } else {
                raw_text
            };

            // Render tab text
            for ch in tab_text.chars() {
                if x >= area.right() {
                    break;
                }
                buf[(x, area.y)].set_char(ch).set_style(style);
                x += 1;
            }

            // Render divider if not last tab
            if idx < self.tabs.len() - 1 && x < area.right() {
                for ch in divider.chars() {
                    if x >= area.right() {
                        break;
                    }
                    buf[(x, area.y)]
                        .set_char(ch)
                        .set_style(Style::default().fg(inactive_color));
                    x += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::SpanType;
    use crate::frontend::tui::text_window::StyledText;

    fn styled_text(text: &str) -> StyledText {
        StyledText {
            content: text.to_string(),
            fg: None,
            bg: None,
            bold: false,
            span_type: SpanType::Normal,
            link_data: None,
        }
    }

    fn make_window() -> TabbedTextWindow {
        let mut window = TabbedTextWindow::new("Tabs", TabBarPosition::Top);
        window.add_tab(
            "Main".to_string(),
            vec!["main".to_string()],
            100,
            false,
            false,
        );
        window.add_tab(
            "Thoughts".to_string(),
            vec!["thoughts".to_string()],
            100,
            false,
            false,
        );
        window
    }

    #[test]
    fn test_add_tab_and_get_names() {
        let window = make_window();
        let names = window.get_tab_names();
        assert_eq!(names, vec!["Main".to_string(), "Thoughts".to_string()]);
    }

    #[test]
    fn test_add_text_marks_unread_for_inactive_tab() {
        let mut window = make_window();
        assert!(!window.has_unread_tabs());

        window.add_text_to_stream("thoughts", styled_text("Hello"));
        assert!(window.has_unread_tabs());

        window.switch_to_tab(1);
        assert!(!window.has_unread_tabs());
    }

    #[test]
    fn test_handle_mouse_click_switches_tab() {
        let mut window = make_window();
        let rect = Rect::new(0, 0, 20, 5);

        // Tab bar is inside border, first tab starts at x=1, y=1 with divider " | "
        let clicked = window.handle_mouse_click(rect, 8, 1);
        assert!(clicked);
        assert_eq!(window.get_active_tab_index(), 1);
    }

    #[test]
    fn test_remove_tab_disallows_last() {
        let mut window = TabbedTextWindow::new("Tabs", TabBarPosition::Top);
        window.add_tab(
            "Main".to_string(),
            vec!["main".to_string()],
            100,
            false,
            false,
        );
        assert!(!window.remove_tab("Main"));

        window.add_tab(
            "Other".to_string(),
            vec!["other".to_string()],
            100,
            false,
            false,
        );
        assert!(window.remove_tab("Main"));
        assert_eq!(window.get_tab_names(), vec!["Other".to_string()]);
        assert_eq!(window.get_active_tab_index(), 0);
    }

    #[test]
    fn test_reorder_tabs_preserves_active_by_name() {
        let mut window = make_window();
        window.add_tab(
            "Other".to_string(),
            vec!["other".to_string()],
            100,
            false,
            false,
        );
        window.switch_to_tab(2); // "Other"

        window.reorder_tabs(&["Other".to_string(), "Main".to_string()]);
        let names = window.get_tab_names();
        let active = window.get_active_tab_index();
        assert_eq!(names[active], "Other");
        assert_eq!(names, vec!["Other".to_string(), "Main".to_string(), "Thoughts".to_string()]);
    }
}

