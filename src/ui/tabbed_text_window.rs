use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Tabs, Widget},
};
use super::{StyledText, TextWindow};

#[derive(Clone, Debug)]
pub enum TabBarPosition {
    Top,
    Bottom,
}

struct TabInfo {
    name: String,
    stream: String,
    window: TextWindow,
    has_unread: bool,
    unread_count: usize,
}

pub struct TabbedTextWindow {
    tabs: Vec<TabInfo>,
    active_tab_index: usize,
    tab_bar_position: TabBarPosition,
    // Border and styling
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    title: String,
    // Background
    transparent_background: bool,
    background_color: Option<String>,
    // Tab styling
    tab_active_color: Option<String>,
    tab_inactive_color: Option<String>,
    tab_unread_color: Option<String>,
    tab_unread_prefix: String,
}

impl TabbedTextWindow {
    /// Create a new empty tabbed window (tabs can be added later)
    pub fn new(
        title: impl Into<String>,
        tab_bar_position: TabBarPosition,
    ) -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_index: 0,
            tab_bar_position,
            show_border: true,
            border_style: None,
            border_color: None,
            title: title.into(),
            transparent_background: false,
            background_color: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: "* ".to_string(),
        }
    }

    /// Create a new tabbed window with initial tabs
    pub fn with_tabs(
        title: impl Into<String>,
        tabs: Vec<(String, String)>, // (tab_name, stream_name)
        max_lines_per_tab: usize,
    ) -> Self {
        let tab_infos: Vec<TabInfo> = tabs
            .into_iter()
            .map(|(name, stream)| TabInfo {
                name: name.clone(),
                stream: stream.clone(),
                window: TextWindow::new(name.clone(), max_lines_per_tab),
                has_unread: false,
                unread_count: 0,
            })
            .collect();

        Self {
            tabs: tab_infos,
            active_tab_index: 0,
            tab_bar_position: TabBarPosition::Top,
            show_border: true,
            border_style: None,
            border_color: None,
            title: title.into(),
            transparent_background: false,
            background_color: None,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: "* ".to_string(),
        }
    }

    pub fn with_border_config(
        mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) -> Self {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color;
        self
    }

    pub fn with_tab_bar_position(mut self, position: TabBarPosition) -> Self {
        self.tab_bar_position = position;
        self
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

    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn has_border(&self) -> bool {
        self.show_border
    }

    pub fn set_border_config(
        &mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color;
    }

    pub fn set_tab_bar_position(&mut self, position: TabBarPosition) {
        self.tab_bar_position = position;
    }

    pub fn set_tab_active_color(&mut self, color: String) {
        self.tab_active_color = Some(color);
    }

    pub fn set_tab_inactive_color(&mut self, color: String) {
        self.tab_inactive_color = Some(color);
    }

    pub fn set_tab_unread_color(&mut self, color: String) {
        self.tab_unread_color = Some(color);
    }

    pub fn set_unread_prefix(&mut self, prefix: String) {
        self.tab_unread_prefix = prefix;
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color;
    }

    /// Add a new tab dynamically
    pub fn add_tab(&mut self, name: String, stream: String, max_lines: usize) {
        self.tabs.push(TabInfo {
            name: name.clone(),
            stream,
            window: TextWindow::new(name, max_lines),
            has_unread: false,
            unread_count: 0,
        });
    }

    /// Add text to a specific stream (will route to correct tab and set unread if needed)
    pub fn add_text_to_stream(&mut self, stream: &str, styled: StyledText) {
        if let Some((idx, tab)) = self.tabs.iter_mut().enumerate().find(|(_, t)| t.stream == stream) {
            tab.window.add_text(styled);

            // Set unread flag if this is not the active tab
            if idx != self.active_tab_index {
                tab.has_unread = true;
                tab.unread_count += 1;
            }
        }
    }

    /// Finish line for a specific stream
    pub fn finish_line_for_stream(&mut self, stream: &str, width: u16) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.stream == stream) {
            tab.window.finish_line(width);
        }
    }

    /// Get the active tab's stream name
    pub fn get_active_stream(&self) -> Option<&str> {
        self.tabs.get(self.active_tab_index).map(|t| t.stream.as_str())
    }

    /// Get all stream names for this tabbed window
    pub fn get_all_streams(&self) -> Vec<String> {
        self.tabs.iter().map(|t| t.stream.clone()).collect()
    }

    /// Switch to a specific tab by index
    pub fn switch_to_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            // Clear unread flag
            self.tabs[index].has_unread = false;
            self.tabs[index].unread_count = 0;
        }
    }

    /// Switch to tab by name
    pub fn switch_to_tab_by_name(&mut self, name: &str) {
        if let Some(idx) = self.tabs.iter().position(|t| t.name == name) {
            self.switch_to_tab(idx);
        }
    }

    /// Remove a tab by name
    pub fn remove_tab(&mut self, name: &str) -> bool {
        if self.tabs.len() <= 1 {
            return false; // Can't remove last tab
        }

        if let Some(idx) = self.tabs.iter().position(|t| t.name == name) {
            self.tabs.remove(idx);

            // Adjust active index if needed
            if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len() - 1;
            }

            true
        } else {
            false
        }
    }

    /// Rename a tab
    pub fn rename_tab(&mut self, old_name: &str, new_name: String) -> bool {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.name == old_name) {
            tab.name = new_name.clone();
            tab.window.set_title(new_name);
            true
        } else {
            false
        }
    }

    /// Get list of current tab names
    pub fn get_tab_names(&self) -> Vec<String> {
        self.tabs.iter().map(|t| t.name.clone()).collect()
    }

    /// Reorder tabs to match a given order of tab names
    pub fn reorder_tabs(&mut self, new_order: &[String]) {
        let mut new_tabs = Vec::new();

        // Build new tab list in the specified order
        for name in new_order {
            if let Some(pos) = self.tabs.iter().position(|t| &t.name == name) {
                new_tabs.push(self.tabs.remove(pos));
            }
        }

        // Add any remaining tabs that weren't in the new order (shouldn't happen, but be safe)
        new_tabs.append(&mut self.tabs);

        self.tabs = new_tabs;

        // Adjust active index if needed
        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len().saturating_sub(1);
        }
    }

    /// Scroll the active tab
    pub fn scroll_up(&mut self, lines: usize) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.scroll_up(lines);
        }
    }

    pub fn scroll_down(&mut self, lines: usize) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.scroll_down(lines);
        }
    }

    /// Update inner width for all tabs
    pub fn update_inner_width(&mut self, width: u16) {
        for tab in &mut self.tabs {
            tab.window.update_inner_width(width);
        }
    }

    /// Parse color string (hex or named)
    fn parse_color(color_str: &str) -> Option<Color> {
        if color_str.starts_with('#') && color_str.len() == 7 {
            // Hex color
            let r = u8::from_str_radix(&color_str[1..3], 16).ok()?;
            let g = u8::from_str_radix(&color_str[3..5], 16).ok()?;
            let b = u8::from_str_radix(&color_str[5..7], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        } else {
            // Named color
            match color_str.to_lowercase().as_str() {
                "black" => Some(Color::Black),
                "red" => Some(Color::Red),
                "green" => Some(Color::Green),
                "yellow" => Some(Color::Yellow),
                "blue" => Some(Color::Blue),
                "magenta" => Some(Color::Magenta),
                "cyan" => Some(Color::Cyan),
                "gray" | "grey" => Some(Color::Gray),
                "white" => Some(Color::White),
                "darkgray" | "darkgrey" => Some(Color::DarkGray),
                _ => None,
            }
        }
    }

    /// Render the tab bar
    fn render_tab_bar(&self, area: Rect, buf: &mut Buffer) {
        let tab_titles: Vec<Line> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(idx, tab)| {
                if idx == self.active_tab_index {
                    // Active tab: bold + highlight color
                    let color = self
                        .tab_active_color
                        .as_ref()
                        .and_then(|c| Self::parse_color(c))
                        .unwrap_or(Color::Yellow);

                    Line::from(Span::styled(
                        tab.name.clone(),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ))
                } else if tab.has_unread {
                    // Inactive with unread: indicator prefix + color
                    let color = self
                        .tab_unread_color
                        .as_ref()
                        .and_then(|c| Self::parse_color(c))
                        .unwrap_or(Color::White);

                    Line::from(Span::styled(
                        format!("{}{}", self.tab_unread_prefix, tab.name),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ))
                } else {
                    // Inactive, no unread: dim
                    let color = self
                        .tab_inactive_color
                        .as_ref()
                        .and_then(|c| Self::parse_color(c))
                        .unwrap_or(Color::DarkGray);

                    Line::from(Span::styled(
                        tab.name.clone(),
                        Style::default().fg(color),
                    ))
                }
            })
            .collect();

        let tabs_widget = Tabs::new(tab_titles)
            .select(self.active_tab_index)
            .divider("|");

        tabs_widget.render(area, buf);
    }

    /// Render with focus indicator
    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        // Create border block
        let mut block = Block::default();

        if self.show_border {
            let borders = if focused {
                Borders::ALL
            } else {
                Borders::ALL
            };
            block = block.borders(borders);

            // Apply border style
            if let Some(style_str) = &self.border_style {
                let border_type = match style_str.as_str() {
                    "double" => BorderType::Double,
                    "rounded" => BorderType::Rounded,
                    "thick" => BorderType::Thick,
                    "quadrant_inside" => BorderType::QuadrantInside,
                    "quadrant_outside" => BorderType::QuadrantOutside,
                    _ => BorderType::Plain,
                };
                block = block.border_type(border_type);
            }

            // Apply border color
            if let Some(color_str) = &self.border_color {
                if let Some(color) = Self::parse_color(color_str) {
                    let border_style = if focused {
                        Style::default().fg(color).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(color)
                    };
                    block = block.border_style(border_style);
                }
            } else if focused {
                block = block.border_style(Style::default().add_modifier(Modifier::BOLD));
            }

            // Only set title when borders are shown
            block = block.title(self.title.clone());
        }

        let inner = block.inner(area);
        block.render(area, buf);

        // Calculate tab bar and content areas
        let (tab_bar_area, content_area) = match self.tab_bar_position {
            TabBarPosition::Top => {
                let tab_bar = Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: 1,
                };
                let content = Rect {
                    x: inner.x,
                    y: inner.y + 1,
                    width: inner.width,
                    height: inner.height.saturating_sub(1),
                };
                (tab_bar, content)
            }
            TabBarPosition::Bottom => {
                let content = Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: inner.height.saturating_sub(1),
                };
                let tab_bar = Rect {
                    x: inner.x,
                    y: inner.y + content.height,
                    width: inner.width,
                    height: 1,
                };
                (tab_bar, content)
            }
        };

        // Fill background if not transparent
        if !self.transparent_background {
            let bg_color = if let Some(ref color_str) = self.background_color {
                Self::parse_color(color_str).unwrap_or(Color::Black)
            } else {
                Color::Black
            };

            // Fill with space characters and background color
            for y in inner.y..inner.y + inner.height {
                for x in inner.x..inner.x + inner.width {
                    if x < buf.area.width && y < buf.area.height {
                        buf.get_mut(x, y).set_char(' ').set_bg(bg_color);
                    }
                }
            }
        }

        // Render tab bar
        self.render_tab_bar(tab_bar_area, buf);

        // Render active tab's content
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.window.render(content_area, buf);
        }
    }

    /// Calculate which tab was clicked based on X position in tab bar
    pub fn get_tab_at_position(&self, x: u16, tab_bar_rect: Rect) -> Option<usize> {
        if x < tab_bar_rect.x || x >= tab_bar_rect.x + tab_bar_rect.width {
            return None;
        }

        // Calculate actual tab widths based on text content
        // Each tab is: " TabName " (1 space before, 1 space after)
        // Plus divider "|" between tabs (1 char)
        let relative_x = x - tab_bar_rect.x;
        let mut current_x = 0u16;

        for (idx, tab) in self.tabs.iter().enumerate() {
            // Calculate the display text for this tab
            let display_text = if idx == self.active_tab_index {
                tab.name.clone()
            } else if tab.has_unread {
                format!("{}{}", self.tab_unread_prefix, tab.name)
            } else {
                tab.name.clone()
            };

            // Tab width = 1 space + text + 1 space = text.len() + 2
            let tab_width = (display_text.len() + 2) as u16;

            // Add divider width if not the first tab (1 char for "|")
            let divider_width = if idx > 0 { 1 } else { 0 };
            let total_width = divider_width + tab_width;

            // Check if click is within this tab's bounds
            if relative_x >= current_x && relative_x < current_x + total_width {
                return Some(idx);
            }

            current_x += total_width;
        }

        None
    }

    /// Get tab bar rect for mouse detection
    pub fn get_tab_bar_rect(&self, window_rect: Rect) -> Rect {
        // Use the same logic as render_with_focus to calculate inner area
        let mut block = Block::default();
        if self.show_border {
            block = block.borders(Borders::ALL);
            // Only set title when borders are shown (matching render_with_focus)
            block = block.title(self.title.clone());
        }
        let inner = block.inner(window_rect);

        match self.tab_bar_position {
            TabBarPosition::Top => Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: 1,
            },
            TabBarPosition::Bottom => Rect {
                x: inner.x,
                y: inner.y + inner.height.saturating_sub(1),
                width: inner.width,
                height: 1,
            },
        }
    }
}
