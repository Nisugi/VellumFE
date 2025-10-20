use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

pub struct Spacer {
    background_color: Option<String>,
    transparent: bool,
}

impl Spacer {
    pub fn new(background_color: Option<String>, transparent: bool) -> Self {
        Self { background_color, transparent }
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        // Handle three-state: None = transparent, Some("-") = transparent, Some(value) = use value
        self.background_color = match color {
            Some(ref s) if s == "-" => None,  // "-" means explicitly transparent
            other => other,
        };
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent = transparent;
    }

    pub fn set_border_config(&mut self, _show_border: bool, _border_style: Option<String>, _border_color: Option<String>) {
        // Intentionally no-op: spacers never render borders
    }

    pub fn set_title(&mut self, _title: String) {
        // No-op: spacers have no title
    }

    pub fn set_border_sides(&mut self, _sides: Option<Vec<String>>) {
        // No-op for spacer
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.transparent {
            return;
        }

        // If user provided a background, use it; otherwise use a subtle default so it's visible initially.
        let color = self
            .background_color
            .as_deref()
            .and_then(Self::parse_hex_color)
            .unwrap_or(Color::DarkGray);

        let style = Style::default().bg(color);
        for y in area.y..area.y + area.height {
            // Fill each row with background spaces
            buf.set_stringn(area.x, y, " ".repeat(area.width as usize), area.width as usize, style);
        }
    }

    fn parse_hex_color(hex: &str) -> Option<Color> {
        let s = hex.trim();
        let h = if let Some(stripped) = s.strip_prefix('#') { stripped } else { s };
        if h.len() != 6 { return None; }
        u32::from_str_radix(h, 16).ok().map(|rgb| {
            let r = ((rgb >> 16) & 0xff) as u8;
            let g = ((rgb >> 8) & 0xff) as u8;
            let b = (rgb & 0xff) as u8;
            Color::Rgb(r, g, b)
        })
    }
}

