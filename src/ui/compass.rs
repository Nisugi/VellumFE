use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType},
};
use std::collections::HashSet;

/// Compass widget showing available exits in a 4x3 grid
/// Layout:
///   U    NW  N   NE
///   D    W   O   E
///   OUT  SW  S   SE
pub struct Compass {
    label: String,
    directions: HashSet<String>, // Active directions
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: Option<Vec<String>>,
    active_color: Option<String>,   // Color for available exits
    inactive_color: Option<String>, // Color for unavailable exits
    content_align: Option<String>,  // Content alignment within widget area
    background_color: Option<String>, // Background color for entire widget area
}

impl Compass {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            directions: HashSet::new(),
            show_border: false,
            border_style: None,
            border_color: None,
            border_sides: None,
            active_color: Some("#00ff00".to_string()),   // Green for available
            inactive_color: Some("#333333".to_string()), // Dark gray for unavailable
            content_align: None,  // Default to top-left
            background_color: None,
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

    pub fn set_directions(&mut self, directions: Vec<String>) {
        // tracing::debug!("Compass: Setting directions: {:?}", directions);  // Commented out - too spammy
        self.directions = directions.into_iter().collect();
    }

    pub fn set_colors(&mut self, active_color: Option<String>, inactive_color: Option<String>) {
        if let Some(color) = active_color {
            self.active_color = Some(color);
        }
        if let Some(color) = inactive_color {
            self.inactive_color = Some(color);
        }
    }

    pub fn set_content_align(&mut self, content_align: Option<String>) {
        self.content_align = content_align;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color;
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
        // Create block for border
        let mut block = Block::default();

        if self.show_border {
            let borders = crate::config::parse_border_sides(&self.border_sides);
            block = block.borders(borders);

            if let Some(ref style) = self.border_style {
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
            use ratatui::widgets::Widget;
            block.render(area, buf);
        }

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // Calculate content alignment offset
        // Compass content is 7 cols x 3 rows
        const CONTENT_WIDTH: u16 = 7;
        const CONTENT_HEIGHT: u16 = 3;

        let (row_offset, col_offset) = if let Some(ref align_str) = self.content_align {
            let align = crate::config::ContentAlign::from_str(align_str);
            align.calculate_offset(CONTENT_WIDTH, CONTENT_HEIGHT, inner_area.width, inner_area.height)
        } else {
            (0, 0) // Default to top-left
        };

        let active_color = self.active_color.as_ref()
            .map(|c| Self::parse_color(c))
            .unwrap_or(Color::Green);
        let inactive_color = self.inactive_color.as_ref()
            .map(|c| Self::parse_color(c))
            .unwrap_or(Color::DarkGray);

        // Fixed position layout with 1 char per direction:
        //   ↑ · ↖ ▲ ↗
        //   · · ◀ o ▶
        //   ↓ · ↙ ▼ ↘
        // Each direction is 1 char, with 1 space between groups

        // Fill background if explicitly set
        if let Some(ref color_hex) = self.background_color {
            let bg_color = Self::parse_color(color_hex);
            for row in 0..inner_area.height {
                for col in 0..inner_area.width {
                    let x = inner_area.x + col;
                    let y = inner_area.y + row;
                    if x < buf.area().width && y < buf.area().height {
                        buf[(x, y)].set_char(' ');
                        buf[(x, y)].set_bg(bg_color);
                    }
                }
            }
        }

        // Define positions for each direction (col, row, label, short_form, long_form)
        // Using arrow icons like ProfanityFE
        // Consistent 2-space gaps throughout (1 char + 1 space = 2 between each)
        //   ↑ · ↖ ▲ ↗
        //   · · ◀ o ▶
        //   ↓ · ↙ ▼ ↘
        let positions = [
            // Row 0
            (0, 0, "↑", Some("up"), Some("up")),
            (2, 0, "↖", Some("nw"), Some("northwest")),
            (4, 0, "▲", Some("n"), Some("north")),
            (6, 0, "↗", Some("ne"), Some("northeast")),
            // Row 1 (middle row - out is in the center of compass)
            (2, 1, "◀", Some("w"), Some("west")),
            (4, 1, "o", Some("out"), Some("out")),
            (6, 1, "▶", Some("e"), Some("east")),
            // Row 2
            (0, 2, "↓", Some("down"), Some("down")),
            (2, 2, "↙", Some("sw"), Some("southwest")),
            (4, 2, "▼", Some("s"), Some("south")),
            (6, 2, "↘", Some("se"), Some("southeast")),
        ];

        for (col, row, dir_label, short_form, long_form) in positions.iter() {
            let x = inner_area.x + col + col_offset;
            let y = inner_area.y + row + row_offset;

            // Skip if position is out of bounds (need to check before any buffer access)
            if x >= buf.area().width || y >= buf.area().height {
                continue;
            }

            // Check if this direction is active
            let is_active = if short_form.is_some() && long_form.is_some() {
                let short = short_form.unwrap();
                let long = long_form.unwrap();
                // Check both short and long forms
                self.directions.contains(short)
                    || self.directions.contains(long)
                    || self.directions.contains(&short.to_lowercase())
                    || self.directions.contains(&long.to_lowercase())
            } else {
                true // Center marker is always visible (when no forms specified)
            };

            let color = if is_active { active_color } else { inactive_color };

            // Render the direction label at its fixed position (1 char each)
            for (i, c) in dir_label.chars().enumerate() {
                let char_x = x + i as u16;
                // Ensure both x and y are within bounds
                if char_x < inner_area.x + inner_area.width && y < inner_area.y + inner_area.height {
                    buf[(char_x, y)].set_char(c);
                    buf[(char_x, y)].set_fg(color);
                    // Only set background if explicitly configured
                    if let Some(ref color_hex) = self.background_color {
                        let bg_color = Self::parse_color(color_hex);
                        buf[(char_x, y)].set_bg(bg_color);
                    }
                }
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}
