use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType},
};

/// Hands widget showing left/right/spell hand contents
/// Layout: Icon: text (up to 24 chars)
/// L: item name here
/// R: another item
/// S: spell prepared
pub struct Hands {
    label: String,
    left_hand: String,
    right_hand: String,
    spell_hand: String,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: Option<Vec<String>>,
    text_color: Option<String>,
}

impl Hands {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            left_hand: String::new(),
            right_hand: String::new(),
            spell_hand: String::new(),
            show_border: false,
            border_style: None,
            border_color: None,
            border_sides: None,
            text_color: Some("#ffffff".to_string()),
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

    pub fn set_left_hand(&mut self, item: String) {
        // Truncate to 24 characters
        self.left_hand = if item.len() > 24 {
            item.chars().take(24).collect()
        } else {
            item
        };
    }

    pub fn set_right_hand(&mut self, item: String) {
        // Truncate to 24 characters
        self.right_hand = if item.len() > 24 {
            item.chars().take(24).collect()
        } else {
            item
        };
    }

    pub fn set_spell_hand(&mut self, spell: String) {
        // Truncate to 24 characters
        self.spell_hand = if spell.len() > 24 {
            spell.chars().take(24).collect()
        } else {
            spell
        };
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color;
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

        let text_color = self.text_color
            .as_ref()
            .map(|c| Self::parse_color(c))
            .unwrap_or(Color::White);

        // Render the three hands
        // Row 0: L: left_hand_text
        // Row 1: R: right_hand_text
        // Row 2: S: spell_hand_text

        let hands = [
            (0, "L:", &self.left_hand),
            (1, "R:", &self.right_hand),
            (2, "S:", &self.spell_hand),
        ];

        for (row, icon, text) in hands.iter() {
            if *row >= inner_area.height {
                break;
            }

            let y = inner_area.y + row;

            // Render icon (2 chars: "L:")
            for (i, ch) in icon.chars().enumerate() {
                let x = inner_area.x + i as u16;
                if x < inner_area.x + inner_area.width {
                    buf[(x, y)].set_char(ch);
                    buf[(x, y)].set_fg(text_color);
                    buf[(x, y)].set_bg(Color::Black);
                }
            }

            // Render text starting at column 3 (after "L: ")
            let start_col = 3;
            for (i, ch) in text.chars().enumerate() {
                let x = inner_area.x + start_col + i as u16;
                if x < inner_area.x + inner_area.width {
                    buf[(x, y)].set_char(ch);
                    buf[(x, y)].set_fg(text_color);
                    buf[(x, y)].set_bg(Color::Black);
                }
            }
        }
    }

    pub fn render_with_focus(&self, area: Rect, buf: &mut Buffer, _focused: bool) {
        self.render(area, buf);
    }
}
