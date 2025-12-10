use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use crate::config::PaletteColor;

/// Color picker dropdown widget with tab completion
/// Shows recent/favorite colors, allows typing for filtering
pub struct ColorPicker {
    colors: Vec<PaletteColor>,
    recent_colors: Vec<String>,  // Recently used color names
    filter: String,  // Current filter text
    selected_index: usize,
    visible: bool,
    max_visible: usize,  // Max colors to show in dropdown (10-15)
}

impl ColorPicker {
    pub fn new(palette: Vec<PaletteColor>) -> Self {
        Self {
            colors: palette,
            recent_colors: Vec::new(),
            filter: String::new(),
            selected_index: 0,
            visible: false,
            max_visible: 12,
        }
    }

    /// Set the filter text (what user is typing)
    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter.to_lowercase();
        self.selected_index = 0;
    }

    /// Get filtered colors (favorites first, then matching filter)
    pub fn filtered_colors(&self) -> Vec<&PaletteColor> {
        if self.filter.is_empty() {
            // Show recent colors first, then favorites
            let mut result = Vec::new();

            // Add recent colors
            for recent in &self.recent_colors {
                if let Some(color) = self.colors.iter().find(|c| c.name == *recent) {
                    result.push(color);
                    if result.len() >= self.max_visible {
                        return result;
                    }
                }
            }

            // Add favorites not already in recent
            for color in &self.colors {
                if color.favorite && !self.recent_colors.contains(&color.name) {
                    result.push(color);
                    if result.len() >= self.max_visible {
                        return result;
                    }
                }
            }

            // Fill remaining with other colors
            for color in &self.colors {
                if !color.favorite && !self.recent_colors.contains(&color.name) {
                    result.push(color);
                    if result.len() >= self.max_visible {
                        break;
                    }
                }
            }

            result
        } else {
            // Filter by name
            self.colors
                .iter()
                .filter(|c| c.name.to_lowercase().contains(&self.filter))
                .take(self.max_visible)
                .collect()
        }
    }

    /// Get the currently selected color
    pub fn get_selected(&self) -> Option<&PaletteColor> {
        let filtered = self.filtered_colors();
        filtered.get(self.selected_index).copied()
    }

    /// Get the currently selected color name
    pub fn get_selected_name(&self) -> Option<String> {
        self.get_selected().map(|c| c.name.clone())
    }

    /// Tab completion - complete to next matching color
    pub fn tab_complete(&mut self) -> Option<String> {
        let filtered = self.filtered_colors();
        if filtered.is_empty() {
            return None;
        }

        // Move to next match
        self.selected_index = (self.selected_index + 1) % filtered.len();
        self.get_selected_name()
    }

    /// Navigate up in the list
    pub fn previous(&mut self) {
        let filtered = self.filtered_colors();
        if !filtered.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Navigate down in the list
    pub fn next(&mut self) {
        let filtered = self.filtered_colors();
        if self.selected_index + 1 < filtered.len() {
            self.selected_index += 1;
        }
    }

    /// Mark a color as recently used
    pub fn mark_recent(&mut self, color_name: &str) {
        // Remove if already in recent
        self.recent_colors.retain(|c| c != color_name);
        // Add to front
        self.recent_colors.insert(0, color_name.to_string());
        // Keep only last 10
        self.recent_colors.truncate(10);
    }

    /// Show the dropdown
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the dropdown
    pub fn hide(&mut self) {
        self.visible = false;
        self.filter.clear();
        self.selected_index = 0;
    }

    /// Check if dropdown is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Render the dropdown below a text field
    /// x, y = position of text field
    /// width = width of dropdown
    pub fn render(&self, x: u16, y: u16, width: u16, buf: &mut Buffer, area: Rect) {
        if !self.visible {
            return;
        }

        let filtered = self.filtered_colors();
        if filtered.is_empty() {
            return;
        }

        let dropdown_height = (filtered.len() as u16).min(self.max_visible as u16) + 2; // +2 for border
        let dropdown_y = (y + 1).min(area.height.saturating_sub(dropdown_height));
        let dropdown_width = width.max(30);

        let dropdown_area = Rect {
            x: x.min(area.width.saturating_sub(dropdown_width)),
            y: dropdown_y,
            width: dropdown_width.min(area.width.saturating_sub(x)),
            height: dropdown_height.min(area.height.saturating_sub(dropdown_y)),
        };

        // Draw solid black background
        for dy in 0..dropdown_area.height {
            for dx in 0..dropdown_area.width {
                let px = dropdown_area.x + dx;
                let py = dropdown_area.y + dy;
                if let Some(cell) = buf.cell_mut((px, py)) {
                    cell.set_char(' ');
                    cell.set_bg(Color::Black);
                }
            }
        }

        // Draw border
        let border_style = Style::default().fg(Color::Cyan);
        self.draw_border(dropdown_area, buf, border_style);

        // Draw colors
        let list_area = Rect {
            x: dropdown_area.x + 1,
            y: dropdown_area.y + 1,
            width: dropdown_area.width.saturating_sub(2),
            height: dropdown_area.height.saturating_sub(2),
        };

        for (i, color) in filtered.iter().take(list_area.height as usize).enumerate() {
            let line_y = list_area.y + i as u16;
            let is_selected = i == self.selected_index;

            // Format: "[color_preview] name"
            let preview = "███";
            let _line = format!("{} {}", preview, color.name);

            // Parse the color for preview
            let preview_color = Self::parse_hex_color(&color.color).unwrap_or(Color::White);

            let bg_color = if is_selected {
                Color::DarkGray
            } else {
                Color::Black
            };

            let fg_color = if is_selected {
                Color::White
            } else {
                Color::Gray
            };

            // Render color preview
            for (j, ch) in preview.chars().enumerate() {
                let px = list_area.x + j as u16;
                if let Some(cell) = buf.cell_mut((px, line_y)) {
                    cell.set_char(ch);
                    cell.set_fg(preview_color);
                    cell.set_bg(bg_color);
                }
            }

            // Render name
            let name_start = list_area.x + preview.len() as u16;
            for (j, ch) in color.name.chars().enumerate() {
                let px = name_start + j as u16;
                if px >= list_area.x + list_area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((px, line_y)) {
                    cell.set_char(ch);
                    cell.set_fg(fg_color);
                    cell.set_bg(bg_color);
                    if is_selected {
                        cell.set_style(Style::default().add_modifier(Modifier::BOLD));
                    }
                }
            }

            // Fill rest of line
            for px in (name_start + color.name.len() as u16)..(list_area.x + list_area.width) {
                if let Some(cell) = buf.cell_mut((px, line_y)) {
                    cell.set_char(' ');
                    cell.set_bg(bg_color);
                }
            }
        }
    }

    fn draw_border(&self, area: Rect, buf: &mut Buffer, style: Style) {
        // Top border
        for x in 0..area.width {
            if let Some(cell) = buf.cell_mut((area.x + x, area.y)) {
                if x == 0 {
                    cell.set_char('┌');
                } else if x == area.width - 1 {
                    cell.set_char('┐');
                } else {
                    cell.set_char('─');
                }
                cell.set_style(style);
            }
        }

        // Bottom border
        for x in 0..area.width {
            if let Some(cell) = buf.cell_mut((area.x + x, area.y + area.height - 1)) {
                if x == 0 {
                    cell.set_char('└');
                } else if x == area.width - 1 {
                    cell.set_char('┘');
                } else {
                    cell.set_char('─');
                }
                cell.set_style(style);
            }
        }

        // Left border
        for y in 1..area.height - 1 {
            if let Some(cell) = buf.cell_mut((area.x, area.y + y)) {
                cell.set_char('│');
                cell.set_style(style);
            }
        }

        // Right border
        for y in 1..area.height - 1 {
            if let Some(cell) = buf.cell_mut((area.x + area.width - 1, area.y + y)) {
                cell.set_char('│');
                cell.set_style(style);
            }
        }
    }

    /// Parse hex color string to ratatui Color
    fn parse_hex_color(hex: &str) -> Option<Color> {
        if hex.starts_with('#') && hex.len() == 7 {
            let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
            let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
            let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        } else {
            None
        }
    }
}
