use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Widget},
};
use std::collections::HashMap;

/// Layout type for dashboard indicators
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DashboardLayout {
    Horizontal,
    Vertical,
    Grid { rows: usize, cols: usize },
}

/// Individual indicator within a dashboard
#[derive(Debug, Clone)]
pub struct DashboardIndicator {
    pub id: String,          // Identifier (e.g., "poisoned", "diseased")
    pub icon: String,        // Unicode/Nerd Font icon
    pub colors: Vec<String>, // [off_color, on_color]
    pub value: u8,           // 0 = off, 1 = on
}

/// Dashboard widget - container for multiple indicators
pub struct Dashboard {
    label: String,
    indicators: Vec<DashboardIndicator>,
    indicator_map: HashMap<String, usize>, // id -> index in indicators vec
    layout: DashboardLayout,
    spacing: u16,            // Spacing between indicators (in characters)
    hide_inactive: bool,     // Hide indicators when value = 0
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: Option<Vec<String>>,
}

impl Dashboard {
    pub fn new(label: &str, layout: DashboardLayout) -> Self {
        Self {
            label: label.to_string(),
            indicators: Vec::new(),
            indicator_map: HashMap::new(),
            layout,
            spacing: 1, // Default 1 space between indicators
            hide_inactive: true, // Default hide inactive
            show_border: false,
            border_style: None,
            border_color: None,
            border_sides: None,
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

    pub fn set_spacing(&mut self, spacing: u16) {
        self.spacing = spacing;
    }

    pub fn set_hide_inactive(&mut self, hide: bool) {
        self.hide_inactive = hide;
    }

    /// Add an indicator to the dashboard
    pub fn add_indicator(&mut self, id: String, icon: String, colors: Vec<String>) {
        let index = self.indicators.len();
        self.indicators.push(DashboardIndicator {
            id: id.clone(),
            icon,
            colors,
            value: 0,
        });
        self.indicator_map.insert(id, index);
    }

    /// Update an indicator's value by ID
    pub fn set_indicator_value(&mut self, id: &str, value: u8) {
        if let Some(&index) = self.indicator_map.get(id) {
            if let Some(indicator) = self.indicators.get_mut(index) {
                indicator.value = value;
            }
        }
    }

    fn parse_color(hex: &str) -> Color {
        if !hex.starts_with('#') || hex.len() != 7 {
            return Color::White;
        }

        let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(255);

        Color::Rgb(r, g, b)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let mut block = Block::default();

        if self.show_border {
            let borders = crate::config::parse_border_sides(&self.border_sides);
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

            if let Some(ref color_str) = self.border_color {
                let color = Self::parse_color(color_str);
                block = block.border_style(Style::default().fg(color));
            }

            block = block.title(self.label.as_str());
        }

        let inner_area = if self.show_border {
            block.inner(area)
        } else {
            area
        };

        // Render the block first
        if self.show_border {
            block.render(area, buf);
        }

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Fill background
        for row in 0..inner_area.height {
            for col in 0..inner_area.width {
                let x = inner_area.x + col;
                let y = inner_area.y + row;
                // Bounds check against buffer dimensions
                if x < buf.area.right() && y < buf.area.bottom() {
                    buf[(x, y)].set_char(' ');
                    buf[(x, y)].set_bg(Color::Black);
                }
            }
        }

        // Filter out inactive indicators if hide_inactive is true
        let visible_indicators: Vec<&DashboardIndicator> = self.indicators
            .iter()
            .filter(|ind| !self.hide_inactive || ind.value > 0)
            .collect();

        if visible_indicators.is_empty() {
            return;
        }

        // Render indicators based on layout
        match self.layout {
            DashboardLayout::Horizontal => {
                self.render_horizontal(&visible_indicators, inner_area, buf);
            }
            DashboardLayout::Vertical => {
                self.render_vertical(&visible_indicators, inner_area, buf);
            }
            DashboardLayout::Grid { rows, cols } => {
                self.render_grid(&visible_indicators, rows, cols, inner_area, buf);
            }
        }
    }

    fn render_horizontal(&self, indicators: &[&DashboardIndicator], area: Rect, buf: &mut Buffer) {
        let mut x = area.x;
        let y = area.y;

        for indicator in indicators {
            if x >= area.x + area.width {
                break;
            }

            // Get color based on value
            let color_index = (indicator.value as usize).min(indicator.colors.len().saturating_sub(1));
            let color = Self::parse_color(&indicator.colors[color_index]);

            // Render each character of the icon
            for (i, ch) in indicator.icon.chars().enumerate() {
                let col = x + i as u16;
                if col < buf.area.right() && y < buf.area.bottom() {
                    buf[(col, y)].set_char(ch);
                    buf[(col, y)].set_fg(color);
                    buf[(col, y)].set_bg(Color::Black);
                }
            }

            // Move to next position (icon width + spacing)
            x += indicator.icon.chars().count() as u16 + self.spacing;
        }
    }

    fn render_vertical(&self, indicators: &[&DashboardIndicator], area: Rect, buf: &mut Buffer) {
        let x = area.x;
        let mut y = area.y;

        for indicator in indicators {
            if y >= area.y + area.height {
                break;
            }

            // Get color based on value
            let color_index = (indicator.value as usize).min(indicator.colors.len().saturating_sub(1));
            let color = Self::parse_color(&indicator.colors[color_index]);

            // Render each character of the icon
            for (i, ch) in indicator.icon.chars().enumerate() {
                let col = x + i as u16;
                if col < buf.area.right() && y < buf.area.bottom() {
                    buf[(col, y)].set_char(ch);
                    buf[(col, y)].set_fg(color);
                    buf[(col, y)].set_bg(Color::Black);
                }
            }

            // Move to next row (+ spacing)
            y += 1 + self.spacing;
        }
    }

    fn render_grid(&self, indicators: &[&DashboardIndicator], grid_rows: usize, grid_cols: usize, area: Rect, buf: &mut Buffer) {
        let cell_width = if grid_cols > 0 {
            (area.width as usize) / grid_cols
        } else {
            area.width as usize
        };

        let cell_height = if grid_rows > 0 {
            (area.height as usize) / grid_rows
        } else {
            area.height as usize
        };

        for (idx, indicator) in indicators.iter().enumerate() {
            if idx >= grid_rows * grid_cols {
                break; // Don't render more than grid capacity
            }

            let grid_row = idx / grid_cols;
            let grid_col = idx % grid_cols;

            let x = area.x + (grid_col * cell_width) as u16;
            let y = area.y + (grid_row * cell_height) as u16;

            if x >= area.x + area.width || y >= area.y + area.height {
                continue;
            }

            // Get color based on value
            let color_index = (indicator.value as usize).min(indicator.colors.len().saturating_sub(1));
            let color = Self::parse_color(&indicator.colors[color_index]);

            // Render each character of the icon
            for (i, ch) in indicator.icon.chars().enumerate() {
                let col = x + i as u16;
                if col < buf.area.right() && y < buf.area.bottom() {
                    buf[(col, y)].set_char(ch);
                    buf[(col, y)].set_fg(color);
                    buf[(col, y)].set_bg(Color::Black);
                }
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}
