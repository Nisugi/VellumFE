use ratatui::{buffer::Buffer, layout::Rect, widgets::Block, widgets::Borders, widgets::Widget};
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
        self.add_or_update_item_full(id, text, None, value, max, None);
    }

    pub fn add_or_update_item_with_suffix(&mut self, id: String, text: String, value: u32, max: u32, suffix: Option<String>) {
        self.add_or_update_item_full(id, text, None, value, max, suffix);
    }

    pub fn add_or_update_item_full(&mut self, id: String, text: String, alternate_text: Option<String>, value: u32, max: u32, suffix: Option<String>) {
        let item = ScrollableItem {
            id: id.clone(),
            text,
            alternate_text,
            value,
            max,
            suffix,
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

        let mut block = Block::default();

        if self.show_border {
            let borders = crate::config::parse_border_sides(&self.border_sides);
            block = block.borders(borders);

            if let Some(ref style) = self.border_style {
                use ratatui::widgets::BorderType;
                let border_type = match style.as_str() {
                    "double" => BorderType::Double,
                    "rounded" => BorderType::Rounded,
                    "thick" => BorderType::Thick,
                    _ => BorderType::Plain,
                };
                block = block.border_type(border_type);
            }

            if let Some(ref color_str) = self.border_color {
                let color = ProgressBar::parse_color(color_str);
                block = block.border_style(ratatui::style::Style::default().fg(color));
            }

            block = block.title(self.label.as_str());
        }

        let inner_area = if self.show_border {
            block.inner(area)
        } else {
            area
        };

        if self.show_border {
            block.render(area, buf);
        }

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Calculate how many items we can display
        let available_height = inner_area.height as usize;
        self.last_available_height = available_height;  // Store for scroll calculations
        let display_count = self.visible_count.unwrap_or(available_height).min(available_height);
        tracing::debug!("ScrollableContainer '{}': available_height={}, visible_count={:?}, display_count={}",
            self.label, available_height, self.visible_count, display_count);

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
                    } else if available_width <= suffix_len + 2 {
                        // Just enough for suffix, show "…[XX:XX]"
                        format!("…{}", suffix)
                    } else {
                        // We have room for text + suffix
                        // Reserve space for suffix + " " (space before suffix)
                        let reserved = suffix_len + 1;
                        let text_space = available_width - reserved;

                        // Determine text (no separator, just text and time)
                        let truncated_text = if source_text.chars().count() > text_space {
                            // Text is too long, truncate with ellipsis
                            let text: String = source_text.chars().take(text_space.saturating_sub(1)).collect();
                            format!("{}…", text)
                        } else {
                            // Text fits completely
                            source_text.clone()
                        };

                        // Calculate padding to push suffix to right edge
                        let current_len = truncated_text.chars().count() + 1 + suffix_len;
                        let padding = available_width.saturating_sub(current_len);

                        // Format: "text<padding> suffix"
                        if padding > 0 {
                            format!("{}{} {}", truncated_text, " ".repeat(padding), suffix)
                        } else {
                            format!("{} {}", truncated_text, suffix)
                        }
                    }
                } else {
                    source_text.clone()
                };

                // Create a progress bar for this item
                let mut pb = ProgressBar::new("");
                pb.set_value(item.value, item.max);
                pb.set_value_with_text(item.value, item.max, Some(display_text));

                pb.set_colors(Some(self.bar_color.clone()), None);
                pb.set_transparent_background(self.transparent_background);
                pb.set_border_config(false, None, None);
                pb.set_display_options(self.show_values, self.show_percentage);

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

        // TODO: Add scroll indicators
        // Show "↑" at top if scroll_offset > 0
        // Show "↓" at bottom if end_idx < item_order.len()
    }

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}
