//! Generic scrollable list container for windows such as hands/effects.
//!
//! Provides ordering, optional alternate text, and per-row progress bars, which
//! makes it a handy building block for several specialized widgets.

use crate::data::LinkData;
use crate::frontend::tui::crossterm_bridge;
use super::progress_bar::ProgressBar;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::{Block, Borders, Clear, Widget as RatatuiWidget},
};
use std::collections::HashMap;

#[derive(Clone)]
pub struct ScrollableItem {
    pub id: String,
    pub text: String,
    pub alternate_text: Option<String>, // Alternative text to display (e.g., spell ID vs spell name)
    pub value: u32,
    pub max: u32,
    pub suffix: Option<String>, // Optional suffix to pin to right edge (e.g., "[XX:XX]")
    pub color: Option<String>,  // Optional color override for this item (hex format)
    pub text_color: Option<String>,
    pub link_data: Option<LinkData>, // Link metadata for clickable items
}

pub struct ScrollableContainer {
    label: String,
    items: HashMap<String, ScrollableItem>,
    item_order: Vec<String>,
    scroll_offset: usize,
    visible_count: Option<usize>, // None = use full available height
    last_available_height: usize, // Track last render height for scrolling
    show_alternate_text: bool,    // Toggle between text and alternate_text
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<Color>,
    border_sides: crate::config::BorderSides, // Which borders to show
    bar_color: String,
    transparent_background: bool,
    text_color: Option<String>,
    background_color_hex: Option<String>,
    background_color: Option<Color>,
    show_values: bool,
    show_percentage: bool,
    /// Highlight engine for pattern matching and styling
    highlight_engine: super::highlight_utils::HighlightEngine,
}

impl ScrollableContainer {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            items: HashMap::new(),
            item_order: Vec::new(),
            scroll_offset: 0,
            visible_count: None,        // Default to using full available height
            last_available_height: 10,  // Default assumption
            show_alternate_text: false, // Default to showing primary text
            show_border: true,
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(), // Default: all borders
            bar_color: "#808080".to_string(),
            transparent_background: false,
            text_color: None,
            background_color_hex: None,
            background_color: None,
            show_values: false,
            show_percentage: false,
            highlight_engine: super::highlight_utils::HighlightEngine::new(Vec::new()),
        }
    }

    /// Set highlight patterns for this container (only recompiles if changed)
    pub fn set_highlights(&mut self, highlights: Vec<crate::config::HighlightPattern>) {
        self.highlight_engine.update_if_changed(highlights);
    }

    /// Set whether text replacement is enabled for highlights
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.highlight_engine.set_replace_enabled(enabled);
    }

    pub fn toggle_alternate_text(&mut self) {
        self.show_alternate_text = !self.show_alternate_text;
    }

    pub fn set_visible_count(&mut self, count: Option<usize>) {
        self.visible_count = count;
    }

    fn max_scroll_offset(&self) -> usize {
        if self.item_order.is_empty() {
            return 0;
        }

        let visible = self
            .visible_count
            .unwrap_or(self.last_available_height)
            .max(1);
        self.item_order.len().saturating_sub(visible)
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        let max_scroll = self.max_scroll_offset();
        self.scroll_offset = self.scroll_offset.saturating_add(amount).min(max_scroll);
    }

    pub fn scroll_position(&self) -> usize {
        self.scroll_offset
    }

    pub fn restore_scroll_position(&mut self, desired_offset: usize) {
        if self.item_order.is_empty() {
            self.scroll_offset = 0;
            return;
        }

        let max_scroll = self.max_scroll_offset();
        self.scroll_offset = desired_offset.min(max_scroll);
    }

    pub fn add_or_update_item(&mut self, id: String, text: String, value: u32, max: u32) {
        self.add_or_update_item_full(id, text, None, value, max, None, None, None, None);
    }

    pub fn add_or_update_item_with_suffix(
        &mut self,
        id: String,
        text: String,
        value: u32,
        max: u32,
        suffix: Option<String>,
    ) {
        self.add_or_update_item_full(id, text, None, value, max, suffix, None, None, None);
    }

    pub fn add_or_update_item_full(
        &mut self,
        id: String,
        text: String,
        alternate_text: Option<String>,
        value: u32,
        max: u32,
        suffix: Option<String>,
        color: Option<String>,
        text_color: Option<String>,
        link_data: Option<LinkData>,
    ) {
        let item = ScrollableItem {
            id: id.clone(),
            text,
            alternate_text,
            value,
            max,
            suffix,
            color,
            text_color,
            link_data,
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
        self.border_color = border_color.and_then(|c| Self::parse_color(&c));
    }

    pub fn set_border_sides(&mut self, border_sides: crate::config::BorderSides) {
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

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        let normalized = color.and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() || trimmed == "-" {
                None
            } else {
                Some(trimmed)
            }
        });

        self.background_color_hex = normalized.clone();
        self.background_color = normalized
            .as_ref()
            .and_then(|value| Self::parse_color(value));
    }

    pub fn set_display_options(&mut self, show_values: bool, show_percentage: bool) {
        self.show_values = show_values;
        self.show_percentage = show_percentage;
    }

    /// Parse a hex color string to ratatui Color
    fn parse_color(hex: &str) -> Option<Color> {
        // Use centralized mode-aware color parser
        super::colors::parse_color_to_ratatui(hex)
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        if area.width < 3 || area.height < 1 {
            return;
        }

        // Clear the area to prevent bleed-through from windows behind
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

        // Determine which borders to show
        let borders = if self.show_border {
            crossterm_bridge::to_ratatui_borders(&self.border_sides)
        } else {
            Borders::NONE
        };

        // Trust that border_color is set by window manager from config resolution
        let border_color = self.border_color.unwrap_or(Color::Reset); // Fallback to terminal default

        let inner_area: Rect;

        if self.show_border {
            // Use Block widget for borders
            let mut block = Block::default().borders(borders);

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

            block = block.border_style(ratatui::style::Style::default().fg(border_color));
            // Only set title if label is non-empty (avoids empty title affecting layout)
            if !self.label.is_empty() {
                block = block.title(self.label.as_str());
            }

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
        self.last_available_height = available_height; // Store for scroll calculations
        let display_count = self
            .visible_count
            .unwrap_or(available_height)
            .min(available_height);

        let max_scroll = self.max_scroll_offset();
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }

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
                        let padding = available_width - text_len - suffix_len;
                        format!("{}{}{}", truncated_text, " ".repeat(padding), suffix)
                    }
                } else {
                    source_text.clone()
                };

                // Create a progress bar for this item
                let mut pb = ProgressBar::new("");
                pb.set_value_with_text(item.value, item.max, Some(display_text));
                pb.set_text_align_left(true);

                // Use item-specific color if provided, otherwise default bar color
                let bar_fill = item.color.clone().or_else(|| Some(self.bar_color.clone()));
                pb.set_colors(bar_fill, None);
                if !self.transparent_background {
                    pb.set_background_color(self.background_color_hex.clone());
                } else {
                    pb.set_background_color(None);
                }

                // Check for highlight match on the item text
                let highlight_color = self.highlight_engine.get_first_match_color(source_text);

                tracing::trace!(
                    "ScrollableContainer render: text='{}', highlight={:?}, item_text_color={:?}, container_text_color={:?}",
                    source_text,
                    highlight_color,
                    item.text_color,
                    self.text_color
                );

                let row_text_color = highlight_color
                    .or_else(|| item.text_color.clone())
                    .or_else(|| self.text_color.clone());

                pb.set_text_color(row_text_color);

                pb.set_transparent_background(self.transparent_background);
                pb.set_border_config(
                    false,
                    None,
                    None,
                    crate::config::BorderSides {
                        top: false,
                        bottom: false,
                        left: false,
                        right: false,
                    },
                ); // No borders on individual items

                // Calculate the area for this item (single row)
                let item_area = Rect {
                    x: inner_area.x,
                    y: inner_area.y + i as u16,
                    width: inner_area.width,
                    height: 1,
                };

                pb.render(item_area, buf);
            }
        }
    }

    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, _focused: bool) {
        // For now, focus doesn't change rendering
        self.render(area, buf);
    }

    /// Get the item at a specific y coordinate within the given area.
    /// Returns None if the y coordinate is outside the content area or no item exists there.
    pub fn get_item_at_y(&self, y: u16, area: Rect) -> Option<&ScrollableItem> {
        // Calculate inner area (accounting for borders)
        let inner_y = if self.show_border {
            area.y + 1 // Skip top border
        } else {
            area.y
        };

        let inner_height = if self.show_border {
            area.height.saturating_sub(2) // Subtract top and bottom borders
        } else {
            area.height
        };

        // Check if y is within the content area
        if y < inner_y || y >= inner_y + inner_height {
            return None;
        }

        // Calculate which row was clicked (relative to inner area)
        let relative_row = (y - inner_y) as usize;

        // Add scroll offset to get the actual item index
        let item_index = self.scroll_offset + relative_row;

        // Get the item ID at this index
        let item_id = self.item_order.get(item_index)?;

        // Return the item
        self.items.get(item_id)
    }

    /// Convert mouse position to text coordinates
    pub fn mouse_to_text_coords(
        &self,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: Rect,
    ) -> Option<(usize, usize)> {
        let border_offset = if self.show_border { 1 } else { 0 };

        // Bounds check within content area
        if mouse_col < window_rect.x + border_offset
            || mouse_col >= window_rect.x + window_rect.width - border_offset
            || mouse_row < window_rect.y + border_offset
            || mouse_row >= window_rect.y + window_rect.height - border_offset
        {
            return None;
        }

        let line_idx = self.scroll_offset + (mouse_row - window_rect.y - border_offset) as usize;
        let col_offset = (mouse_col - window_rect.x - border_offset) as usize;

        Some((line_idx, col_offset))
    }

    /// Extract text from a selection range
    pub fn extract_selection_text(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        let mut result = String::new();

        for line_idx in start_line..=end_line.min(self.item_order.len().saturating_sub(1)) {
            if line_idx >= self.item_order.len() {
                break;
            }

            let item_id = &self.item_order[line_idx];
            let item = match self.items.get(item_id) {
                Some(i) => i,
                None => continue,
            };

            let line_text = if self.show_alternate_text {
                item.alternate_text.as_deref().unwrap_or(&item.text)
            } else {
                &item.text
            };
            let line_len = line_text.chars().count();

            if start_line == end_line {
                // Single line selection
                let start = start_col.min(line_len);
                let end = end_col.min(line_len);
                if start < end {
                    result.push_str(&line_text.chars().skip(start).take(end - start).collect::<String>());
                }
            } else if line_idx == start_line {
                // First line of multi-line selection
                let start = start_col.min(line_len);
                result.push_str(&line_text.chars().skip(start).collect::<String>());
                result.push('\n');
            } else if line_idx == end_line {
                // Last line of multi-line selection
                let end = end_col.min(line_len);
                result.push_str(&line_text.chars().take(end).collect::<String>());
            } else {
                // Middle lines - take all
                result.push_str(line_text);
                result.push('\n');
            }
        }

        result
    }
}
