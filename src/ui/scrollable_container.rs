use ratatui::{buffer::Buffer, layout::Rect, widgets::Block, widgets::Widget};
use super::progress_bar::ProgressBar;
use std::collections::HashMap;

#[derive(Clone)]
pub struct ScrollableItem {
    pub id: String,
    pub text: String,
    pub alternate_text: Option<String>,  // Alternative text to display (e.g., spell ID vs spell name)
    pub value: u32,
    pub max: u32,
    pub suffix: Option<String>,  // Optional suffix to pin to right edge (e.g., "[XX:XX]")
    pub color: Option<String>,   // Optional color override for this item (hex format)
}

pub struct ScrollableContainer {
    label: String,
    items: HashMap<String, ScrollableItem>,
    item_order: Vec<String>,
    scroll_offset: usize,
    visible_count: Option<usize>,  // None = use full available height
    last_available_height: usize,  // Track last render height for scrolling
    show_alternate_text: bool,  // Toggle between text and alternate_text
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: Option<Vec<String>>,  // Which borders to show
    bar_color: String,
    transparent_background: bool,
    show_values: bool,
    show_percentage: bool,
}

impl ScrollableContainer {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            items: HashMap::new(),
            item_order: Vec::new(),
            scroll_offset: 0,
            visible_count: None,  // Default to using full available height
            last_available_height: 10,  // Default assumption
            show_alternate_text: false,  // Default to showing primary text
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: None,  // Default: all borders
            bar_color: "#808080".to_string(),
            transparent_background: true,
            show_values: false,
            show_percentage: false,
        }
    }

    pub fn toggle_alternate_text(&mut self) {
        self.show_alternate_text = !self.show_alternate_text;
    }

    pub fn set_visible_count(&mut self, count: Option<usize>) {
        tracing::debug!("ScrollableContainer '{}': set_visible_count to {:?}", self.label, count);
        self.visible_count = count;
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    pub fn scroll_down(&mut self) {
        let visible = self.visible_count.unwrap_or(self.last_available_height);
        let max_scroll = self.items.len().saturating_sub(visible);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    pub fn add_or_update_item(&mut self, id: String, text: String, value: u32, max: u32) {
        self.add_or_update_item_full(id, text, None, value, max, None, None);
    }

    pub fn add_or_update_item_with_suffix(&mut self, id: String, text: String, value: u32, max: u32, suffix: Option<String>) {
        self.add_or_update_item_full(id, text, None, value, max, suffix, None);
    }

    pub fn add_or_update_item_full(&mut self, id: String, text: String, alternate_text: Option<String>, value: u32, max: u32, suffix: Option<String>, color: Option<String>) {
        let item = ScrollableItem {
            id: id.clone(),
            text,
            alternate_text,
            value,
            max,
            suffix,
            color,
        };

        // Add to order list if new
        if !self.items.contains_key(&id) {
            self.item_order.push(id.clone());
        }

        self.items.insert(id, item);
    }

    pub fn remove_item(&mut self, id: &str) {
        self.items.remove(id);
        self.item_order.retain(|item_id| item_id != id);
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.item_order.clear();
        self.scroll_offset = 0;
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

    pub fn set_border_sides(&mut self, border_sides: Option<Vec<String>>) {
        self.border_sides = border_sides;
    }

    pub fn set_title(&mut self, title: String) {
        self.label = title;
    }

    pub fn set_bar_color(&mut self, color: String) {
        self.bar_color = color;
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    pub fn set_display_options(&mut self, show_values: bool, show_percentage: bool) {
        self.show_values = show_values;
        self.show_percentage = show_percentage;
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        if area.width < 3 || area.height < 1 {
            return;
        }

        // Determine which borders to show
        let borders = if self.show_border {
            crate::config::parse_border_sides(&self.border_sides)
        } else {
            ratatui::widgets::Borders::NONE
        };

        // Trust that border_color is set by window manager from config resolution
        let border_color = self.border_color.as_ref()
            .map(|c| ProgressBar::parse_color(c))
            .unwrap_or(ratatui::style::Color::Reset);  // Fallback to terminal default (should never happen)

        // Check if we have partial borders (not all four sides)
        let has_top = borders.contains(ratatui::widgets::Borders::TOP);
        let has_bottom = borders.contains(ratatui::widgets::Borders::BOTTOM);
        let has_left = borders.contains(ratatui::widgets::Borders::LEFT);
        let has_right = borders.contains(ratatui::widgets::Borders::RIGHT);
        let is_partial_borders = self.show_border && !(has_top && has_bottom && has_left && has_right);

        let inner_area: Rect;

        if is_partial_borders {
            // For partial borders, we'll manually render them
            // Calculate inner area manually
            let top_offset = if has_top { 1 } else { 0 };
            let bottom_offset = if has_bottom { 1 } else { 0 };
            let left_offset = if has_left { 1 } else { 0 };
            let right_offset = if has_right { 1 } else { 0 };

            inner_area = Rect {
                x: area.x + left_offset,
                y: area.y + top_offset,
                width: area.width.saturating_sub(left_offset + right_offset),
                height: area.height.saturating_sub(top_offset + bottom_offset),
            };

            // Render top border with title if enabled
            if has_top {
                // Draw top border line
                for x in area.x..area.x + area.width {
                    if x < buf.area().width {
                        let ch = if x == area.x && has_left {
                            '┌'  // Top-left corner
                        } else if x == area.x + area.width - 1 && has_right {
                            '┐'  // Top-right corner
                        } else if x > area.x && has_left && x == area.x + left_offset - 1 {
                            '┬'  // T-junction (shouldn't happen with this logic)
                        } else {
                            '─'  // Horizontal line
                        };
                        buf[(x, area.y)].set_char(ch);
                        buf[(x, area.y)].set_fg(border_color);
                    }
                }

                // Render title on top border
                let title_text = format!(" {} ", self.label);
                let title_start = area.x + 2;  // Start 2 chars from left edge
                for (i, ch) in title_text.chars().enumerate() {
                    let x = title_start + i as u16;
                    if x < area.x + area.width - 1 {
                        buf[(x, area.y)].set_char(ch);
                        buf[(x, area.y)].set_fg(border_color);
                    }
                }
            }

            // Render bottom border if enabled
            if has_bottom {
                let bottom_y = area.y + area.height - 1;
                for x in area.x..area.x + area.width {
                    if x < buf.area().width && bottom_y < buf.area().height {
                        let ch = if x == area.x && has_left {
                            '└'  // Bottom-left corner
                        } else if x == area.x + area.width - 1 && has_right {
                            '┘'  // Bottom-right corner
                        } else {
                            '─'  // Horizontal line
                        };
                        buf[(x, bottom_y)].set_char(ch);
                        buf[(x, bottom_y)].set_fg(border_color);
                    }
                }
            }

            // We'll render left/right borders per content row later
        } else if self.show_border {
            // Use Block widget for all borders or no borders
            let mut block = Block::default().borders(borders);

            if let Some(ref style) = self.border_style {
                use ratatui::widgets::BorderType;
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

            block = block.border_style(ratatui::style::Style::default().fg(border_color));
            block = block.title(self.label.as_str());

            inner_area = block.inner(area);
            block.render(area, buf);
        } else {
            inner_area = area;
        }

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Calculate how many items we can display
        let available_height = inner_area.height as usize;
        self.last_available_height = available_height;  // Store for scroll calculations
        let display_count = self.visible_count.unwrap_or(available_height).min(available_height);

        // Get the slice of items to display
        let start_idx = self.scroll_offset;
        let end_idx = (start_idx + display_count).min(self.item_order.len());

        // Render each visible item as a progress bar
        for (i, item_id) in self.item_order[start_idx..end_idx].iter().enumerate() {
            if let Some(item) = self.items.get(item_id) {
                // Choose which text to display (primary or alternate)
                let source_text = if self.show_alternate_text {
                    item.alternate_text.as_ref().unwrap_or(&item.text)
                } else {
                    &item.text
                };

                // Format text with suffix pinned to right edge
                let display_text = if let Some(ref suffix) = item.suffix {
                    let available_width = inner_area.width as usize;
                    let suffix_len = suffix.chars().count();

                    if available_width < suffix_len + 1 {
                        // Too narrow to show anything meaningful, just show truncated suffix
                        suffix.chars().take(available_width).collect()
                    } else if available_width <= suffix_len + 1 {
                        // Just barely enough for suffix, truncate text completely
                        suffix.clone()
                    } else {
                        // We have room for text + suffix
                        // Reserve space for suffix + " " (space before suffix)
                        let reserved = suffix_len + 1;
                        let text_space = available_width - reserved;

                        // Determine text (no separator, just text and time)
                        let truncated_text = if source_text.chars().count() > text_space {
                            // Text is too long, truncate without ellipsis
                            source_text.chars().take(text_space).collect()
                        } else {
                            // Text fits completely
                            source_text.clone()
                        };

                        // Calculate padding to push suffix to right edge
                        // We want: "text<padding>suffix" where padding is at least 1 space
                        let text_len = truncated_text.chars().count();
                        let min_spacing = 1;  // At least 1 space between text and suffix
                        let current_len = text_len + min_spacing + suffix_len;

                        if current_len <= available_width {
                            // We have room for at least 1 space
                            let padding = available_width - text_len - suffix_len;
                            format!("{}{}{}", truncated_text, " ".repeat(padding), suffix)
                        } else {
                            // No room for spacing, just concatenate (shouldn't happen with correct text truncation)
                            format!("{}{}", truncated_text, suffix)
                        }
                    }
                } else {
                    source_text.clone()
                };

                // Create a progress bar for this item
                let mut pb = ProgressBar::new("");
                pb.set_value(item.value, item.max);
                pb.set_value_with_text(item.value, item.max, Some(display_text));

                // Use item-specific color if available, otherwise use default bar_color
                let bar_color = item.color.as_ref().unwrap_or(&self.bar_color).clone();
                pb.set_colors(Some(bar_color), None);
                pb.set_transparent_background(self.transparent_background);
                pb.set_border_config(false, None, None);
                pb.set_display_options(self.show_values, self.show_percentage);
                pb.set_text_alignment(crate::ui::TextAlignment::Left);  // Left-align for effect lists

                // Render this progress bar in its row
                let row_area = Rect {
                    x: inner_area.x,
                    y: inner_area.y + i as u16,
                    width: inner_area.width,
                    height: 1,
                };

                pb.render(row_area, buf);
            }
        }

        // If we have partial borders with left/right enabled, render them on all content rows
        if is_partial_borders && (has_left || has_right) {
            for y in inner_area.y..inner_area.y + inner_area.height {
                if y < buf.area().height {
                    // Render left border
                    if has_left && area.x < buf.area().width {
                        buf[(area.x, y)].set_char('│');
                        buf[(area.x, y)].set_fg(border_color);
                    }
                    // Render right border
                    if has_right {
                        let right_x = area.x + area.width - 1;
                        if right_x < buf.area().width {
                            buf[(right_x, y)].set_char('│');
                            buf[(right_x, y)].set_fg(border_color);
                        }
                    }
                }
            }
        }

        // TODO: Add scroll indicators
        // Show "↑" at top if scroll_offset > 0
        // Show "↓" at bottom if end_idx < item_order.len()
    }

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}
