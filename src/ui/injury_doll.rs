use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, BorderType},
};
use std::collections::HashMap;

/// Injury doll widget showing body part injuries/scars
/// Layout:
///  👁   👁
///      0      ns
///     /|\
///    o | o   nk
///     / \
///    o   o   bk
pub struct InjuryDoll {
    label: String,
    // Map body part name to injury level (0=none, 1-3=injury, 4-6=scar)
    injuries: HashMap<String, u8>,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: Option<Vec<String>>,
    // ProfanityFE injury colors: none, injury1-3, scar1-3
    colors: Vec<String>,
}

impl InjuryDoll {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            injuries: HashMap::new(),
            show_border: false,
            border_style: None,
            border_color: None,
            border_sides: None,
            colors: vec![
                "#333333".to_string(),  // 0: none
                "#aa5500".to_string(),  // 1: injury 1 (brown)
                "#ff8800".to_string(),  // 2: injury 2 (orange)
                "#ff0000".to_string(),  // 3: injury 3 (bright red)
                "#999999".to_string(),  // 4: scar 1 (light gray)
                "#777777".to_string(),  // 5: scar 2 (medium gray)
                "#555555".to_string(),  // 6: scar 3 (darker gray)
            ],
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

    pub fn set_injury(&mut self, body_part: String, level: u8) {
        tracing::debug!("InjuryDoll: Setting {} to level {}", body_part, level);
        self.injuries.insert(body_part, level.min(6));
    }

    pub fn set_colors(&mut self, colors: Vec<String>) {
        if colors.len() == 7 {
            self.colors = colors;
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

    fn get_injury_color(&self, body_part: &str) -> Color {
        let level = self.injuries.get(body_part).copied().unwrap_or(0);
        let color_hex = &self.colors[level as usize];
        Self::parse_color(color_hex)
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

        // Fill entire area with black background
        for row in 0..inner_area.height {
            for col in 0..inner_area.width {
                let x = inner_area.x + col;
                let y = inner_area.y + row;
                buf[(x, y)].set_char(' ');
                buf[(x, y)].set_bg(Color::Black);
            }
        }

        // Layout with fixed character positions (shifted left to align with eyes):
        // 👁   👁
        //     0      nk
        //    /|\
        //   o | o   bk
        //    / \
        //   o   o   ns

        // Define all body part positions (col, row, char, body_part_name)
        let positions = [
            // Row 0: Eyes
            (0, 0, '\u{f06e}', "leftEye"),   // Nerd Font eye icon
            (4, 0, '\u{f06e}', "rightEye"),
            // Row 1: Head
            (2, 1, '0', "head"),
            // Row 2: Arms/Chest
            (1, 2, '/', "leftArm"),
            (2, 2, '|', "chest"),
            (3, 2, '\\', "rightArm"),
            // Row 3: Hands/Abdomen
            (0, 3, 'o', "leftHand"),
            (2, 3, '|', "abdomen"),
            (4, 3, 'o', "rightHand"),
            // Row 4: Leg tops
            (1, 4, '/', "leftLeg"),
            (3, 4, '\\', "rightLeg"),
            // Row 5: Leg bottoms (same body parts, just visual continuation)
            (0, 5, 'o', "leftLeg"),
            (4, 5, 'o', "rightLeg"),
        ];

        // Render body parts
        for (col, row, ch, body_part) in positions.iter() {
            let x = inner_area.x + col;
            let y = inner_area.y + row;

            // Bounds check
            if x < inner_area.x + inner_area.width && y < inner_area.y + inner_area.height {
                let color = self.get_injury_color(body_part);
                buf[(x, y)].set_char(*ch);
                buf[(x, y)].set_fg(color);
                buf[(x, y)].set_bg(Color::Black);
            }
        }

        // Render special indicators on the right with text labels: nk, bk, ns (reordered)
        // Neck on row 1, Back on row 3, Nerves on row 5
        let text_indicators = [
            (6, 1, "nk", "neck"),   // neck - row 1
            (6, 3, "bk", "back"),   // back - row 3
            (6, 5, "ns", "nsys"),   // nerves - row 5
        ];

        for (start_col, row, text, body_part) in text_indicators.iter() {
            let color = self.get_injury_color(body_part);

            for (i, ch) in text.chars().enumerate() {
                let x = inner_area.x + start_col + i as u16;
                let y = inner_area.y + row;

                if x < inner_area.x + inner_area.width && y < inner_area.y + inner_area.height {
                    buf[(x, y)].set_char(ch);
                    buf[(x, y)].set_fg(color);
                    buf[(x, y)].set_bg(Color::Black);
                }
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}
