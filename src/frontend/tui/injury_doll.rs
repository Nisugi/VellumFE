//! Renders the profanity-style injury doll showing wounds/scars per body part.
//!
//! The widget maps injury levels to configurable colors and can be embedded in
//! any window with optional borders/background alignment tweaks.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Clear, Widget as RatatuiWidget},
};
use std::collections::HashMap;

use super::colors::parse_color_to_ratatui;
use super::crossterm_bridge;

/// Injury doll widget showing body part injuries/scars
/// Layout:
///  👁   👁
///     0    ns
///    /|\
///   o | o  nk
///    / \
///   o   o  bk
pub struct InjuryDoll {
    label: String,
    // Map body part name to injury level (0=none, 1-3=injury, 4-6=scar)
    injuries: HashMap<String, u8>,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<Color>,
    border_sides: crate::config::BorderSides,
    // ProfanityFE injury colors: none, injury1-3, scar1-3
    colors: Vec<String>,
    background_color: Option<Color>,
    content_align: Option<String>,
    transparent_background: bool,
}

impl InjuryDoll {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            injuries: HashMap::new(),
            show_border: false,
            border_style: None,
            border_color: None,
            border_sides: crate::config::BorderSides::default(),
            colors: vec![
                "#333333".to_string(), // 0: none
                "#aa5500".to_string(), // 1: injury 1 (brown)
                "#ff8800".to_string(), // 2: injury 2 (orange)
                "#ff0000".to_string(), // 3: injury 3 (bright red)
                "#999999".to_string(), // 4: scar 1 (light gray)
                "#777777".to_string(), // 5: scar 2 (medium gray)
                "#555555".to_string(), // 6: scar 3 (darker gray)
            ],
            background_color: None,
            content_align: None,
            transparent_background: false, // Default to transparent
        }
    }

    pub fn set_border_config(
        &mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color.and_then(|value| Self::parse_color(&value));
    }

    pub fn set_border_sides(&mut self, border_sides: crate::config::BorderSides) {
        self.border_sides = border_sides;
    }

    pub fn set_title(&mut self, title: String) {
        self.label = title;
    }

    pub fn set_injury(&mut self, body_part: String, level: u8) {
        self.injuries.insert(body_part, level.min(6));
    }

    pub fn set_colors(&mut self, colors: Vec<String>) {
        if colors.len() == 7 {
            self.colors = colors;
        }
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = match color {
            Some(ref s) if s.trim() == "-" => None,
            Some(value) => Self::parse_color(&value),
            None => None,
        };
    }

    pub fn set_content_align(&mut self, align: Option<String>) {
        self.content_align = align;
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    fn parse_color(input: &str) -> Option<Color> {
        parse_color_to_ratatui(input)
    }

    fn fill_background(area: Rect, buf: &mut Buffer, color: Color) {
        for row in 0..area.height {
            for col in 0..area.width {
                let x = area.x + col;
                let y = area.y + row;
                if x < buf.area().width && y < buf.area().height {
                    buf[(x, y)].set_bg(color);
                }
            }
        }
    }

    fn get_injury_color(&self, body_part: &str) -> Color {
        let level = self.injuries.get(body_part).copied().unwrap_or(0);
        let color_hex = &self.colors[level as usize];
        Self::parse_color(color_hex).unwrap_or(Color::White)
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if !self.transparent_background {
            Clear.render(area, buf);
            if let Some(bg_color) = self.background_color {
                Self::fill_background(area, buf, bg_color);
            }
        }

        let mut block = Block::default();
        if !self.transparent_background {
            if let Some(bg_color) = self.background_color {
                block = block.style(Style::default().bg(bg_color));
            }
        }

        if self.show_border {
            let borders = crossterm_bridge::to_ratatui_borders(&self.border_sides);
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

            if let Some(color) = self.border_color {
                block = block.border_style(Style::default().fg(color));
            }

            // Only set title if label is non-empty (avoids empty title affecting layout)
            if !self.label.is_empty() {
                block = block.title(self.label.as_str());
            }
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

        let bg_color = self.background_color;

        // Define all body part positions (col, row, char, body_part_name)
        let positions = [
            // Row 0: Eyes
            (0, 0, '\u{f06e}', "leftEye"), // Nerd Font eye icon
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

        // Render special indicators on the right with text labels: nk, bk, ns
        let text_indicators = [
            (6, 1, "nk", "neck"), // neck - row 1
            (6, 3, "bk", "back"), // back - row 3
            (6, 5, "ns", "nsys"), // nerves - row 5
        ];

        // Calculate content alignment offset based on actual footprint
        let mut content_width = 0u16;
        let mut content_height = 0u16;

        for (col, row, _glyph, _) in positions.iter() {
            // Each glyph is a single character in the grid
            let col_val = *col;
            let row_val = *row;
            content_width = content_width.max(col_val + 1);
            content_height = content_height.max(row_val + 1);
        }
        for (start_col, row, text, _) in text_indicators.iter() {
            let text_width = text.chars().count() as u16;
            let start_col_val = *start_col;
            let row_val = *row;
            content_width = content_width.max(start_col_val + text_width);
            content_height = content_height.max(row_val + 1);
        }

        let (row_offset, col_offset) = if let Some(ref align_str) = self.content_align {
            let align = crate::config::ContentAlign::from_str(align_str);
            align.calculate_offset(
                content_width,
                content_height,
                inner_area.width,
                inner_area.height,
            )
        } else {
            (0, 0) // Default to top-left
        };

        // Render body parts
        for (col, row, ch, body_part) in positions.iter() {
            let x = inner_area.x + col + col_offset;
            let y = inner_area.y + row + row_offset;

            // Bounds check - must be within inner_area AND buffer
            if x >= inner_area.x
                && x < inner_area.x + inner_area.width
                && y >= inner_area.y
                && y < inner_area.y + inner_area.height
                && x < buf.area().width
                && y < buf.area().height
            {
                let color = self.get_injury_color(body_part);
                buf[(x, y)].set_char(*ch);
                buf[(x, y)].set_fg(color);
                if !self.transparent_background {
                    if let Some(bg) = bg_color {
                        buf[(x, y)].set_bg(bg);
                    }
                }
            }
        }

        for (start_col, row, text, body_part) in text_indicators.iter() {
            let color = self.get_injury_color(body_part);

            for (i, ch) in text.chars().enumerate() {
                let x = inner_area.x + start_col + i as u16 + col_offset;
                let y = inner_area.y + row + row_offset;

                // Bounds check - must be within inner_area AND buffer
                if x >= inner_area.x
                    && x < inner_area.x + inner_area.width
                    && y >= inner_area.y
                    && y < inner_area.y + inner_area.height
                    && x < buf.area().width
                    && y < buf.area().height
                {
                    buf[(x, y)].set_char(ch);
                    buf[(x, y)].set_fg(color);
                    if !self.transparent_background {
                        if let Some(bg) = bg_color {
                            buf[(x, y)].set_bg(bg);
                        }
                    }
                }
            }
        }
    }
}

/// Render an injuries popup for viewing another player's injuries
/// Returns the bounding rect of the popup for click detection
pub fn render_injuries_popup(
    popup: &crate::data::InjuriesPopupState,
    screen: Rect,
    buf: &mut Buffer,
    theme: &crate::theme::AppTheme,
) -> Rect {
    // Popup dimensions: injury doll needs 8 cols, 6 rows content + border + title
    // Layout: title bar (1 row) + injury doll (6 rows) + close hint (1 row)
    let popup_width: u16 = 22; // 8 cols content + 2 border + padding
    let popup_height: u16 = 10; // 1 title + 6 content + 1 hint + 2 border

    // Center the popup on screen
    let popup_x = screen.x + (screen.width.saturating_sub(popup_width)) / 2;
    let popup_y = screen.y + (screen.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the popup area
    Clear.render(popup_area, buf);

    // Draw border and background - convert theme colors to ratatui colors
    let bg_color = crossterm_bridge::to_ratatui_color(theme.window_background);
    let border_color = crossterm_bridge::to_ratatui_color(theme.window_border);
    let title_color = crossterm_bridge::to_ratatui_color(theme.window_title);

    // Fill background
    for row in popup_area.y..popup_area.y + popup_area.height {
        for col in popup_area.x..popup_area.x + popup_area.width {
            if col < buf.area().width && row < buf.area().height {
                buf[(col, row)].set_bg(bg_color);
            }
        }
    }

    // Draw border
    let block = Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(format!(" {}'s Injuries ", popup.player_name))
        .title_style(Style::default().fg(title_color).add_modifier(ratatui::style::Modifier::BOLD))
        .style(Style::default().bg(bg_color));

    let inner_area = block.inner(popup_area);
    block.render(popup_area, buf);

    // Render the injury doll in the inner area
    let mut doll = InjuryDoll::new("");
    doll.set_background_color(Some(format!(
        "#{:02x}{:02x}{:02x}",
        bg_color.to_string().as_bytes()[0],
        0,
        0
    )));

    // Copy injuries from popup state to doll
    for (body_part, level) in &popup.injuries {
        doll.set_injury(body_part.clone(), *level);
    }

    // Render doll in upper portion, leaving room for close hint
    let doll_area = Rect::new(
        inner_area.x,
        inner_area.y,
        inner_area.width,
        inner_area.height.saturating_sub(1),
    );

    // Center the injury doll content
    doll.set_content_align(Some("center".to_string()));
    doll.set_transparent_background(false);
    if let Some((r, g, b)) = color_to_rgb(bg_color) {
        doll.set_background_color(Some(format!("#{:02x}{:02x}{:02x}", r, g, b)));
    }
    doll.render(doll_area, buf);

    // Render close hint at bottom
    let hint_y = inner_area.y + inner_area.height.saturating_sub(1);
    let hint_text = "[Esc to close]";
    let hint_x = inner_area.x + (inner_area.width.saturating_sub(hint_text.len() as u16)) / 2;
    let hint_style = Style::default().fg(Color::DarkGray);

    for (i, ch) in hint_text.chars().enumerate() {
        let x = hint_x + i as u16;
        if x < buf.area().width && hint_y < buf.area().height {
            buf[(x, hint_y)].set_char(ch);
            buf[(x, hint_y)].set_style(hint_style);
        }
    }

    popup_area
}

/// Convert ratatui Color to RGB tuple if possible
fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Black => Some((0, 0, 0)),
        Color::White => Some((255, 255, 255)),
        Color::Red => Some((255, 0, 0)),
        Color::Green => Some((0, 255, 0)),
        Color::Blue => Some((0, 0, 255)),
        Color::Yellow => Some((255, 255, 0)),
        Color::Magenta => Some((255, 0, 255)),
        Color::Cyan => Some((0, 255, 255)),
        Color::Gray => Some((128, 128, 128)),
        Color::DarkGray => Some((64, 64, 64)),
        Color::LightRed => Some((255, 128, 128)),
        Color::LightGreen => Some((128, 255, 128)),
        Color::LightBlue => Some((128, 128, 255)),
        Color::LightYellow => Some((255, 255, 128)),
        Color::LightMagenta => Some((255, 128, 255)),
        Color::LightCyan => Some((128, 255, 255)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BorderSides;

    #[test]
    fn content_stays_within_inner_area_with_left_bottom_borders() {
        // Test case: 7 total rows, 10 total cols
        // Borders: left + bottom (no top, no right)
        // Expected: content should NOT overlap bottom border at row 6

        let mut doll = InjuryDoll::new("Injuries");
        doll.set_border_config(true, Some("single".to_string()), Some("#ffffff".to_string()));
        doll.set_border_sides(BorderSides {
            left: true,
            right: false,
            top: false,
            bottom: true,
        });

        let area = Rect::new(0, 0, 10, 7);
        let mut buf = Buffer::empty(area);
        doll.render(area, &mut buf);

        // Check that the bottom border row (row 6) contains border character on left
        // and the content (eyes at row 0) is rendered in the top row of inner area
        assert_eq!(
            buf[(0, 6)].symbol(),
            "└",
            "Bottom-left corner should have border character"
        );

        // Row 0 should have content (left eye at col 1 within inner area)
        // Inner area starts at x=1 (after left border), so left eye at inner col 0 = buf col 1
        let eye_char = buf[(1, 0)].symbol();
        assert!(
            eye_char != " ",
            "Row 0 should have content (eye), got: '{}'",
            eye_char
        );
    }

    #[test]
    fn content_does_not_overflow_into_bottom_border() {
        let mut doll = InjuryDoll::new("Test");
        doll.set_border_config(true, Some("single".to_string()), None);
        doll.set_border_sides(BorderSides {
            left: true,
            right: false,
            top: false,
            bottom: true,
        });

        let area = Rect::new(0, 0, 10, 7);
        let mut buf = Buffer::empty(area);
        doll.render(area, &mut buf);

        // The last row of content (feet at content row 5) should be at buf row 5
        // (inner area is rows 0-5, bottom border is row 6)
        // Feet are at content positions (0, 5) and (4, 5)
        // With inner_area starting at x=1, these would be at buf positions (1, 5) and (5, 5)

        // Row 6 should only have border characters, not content
        // Check columns 1-8 (inner area columns) in row 6 for content
        for col in 1..9 {
            let cell = &buf[(col, 6)];
            let sym = cell.symbol();
            // Should be space or border character, not injury doll content
            assert!(
                sym == " " || sym == "─" || sym == "└",
                "Row 6 (bottom border) should not have injury doll content at col {}, got: '{}'",
                col,
                sym
            );
        }
    }

    #[test]
    fn content_respects_bottom_border_with_larger_widget_no_title() {
        // Test with widget taller than content (8 rows vs 6 rows content)
        // WITH NO TITLE (show_title off) - content should align properly
        let mut doll = InjuryDoll::new(""); // Empty title = show_title off
        doll.set_border_config(true, Some("single".to_string()), None);
        doll.set_border_sides(BorderSides {
            left: true,
            right: false,
            top: false,
            bottom: true,
        });
        // Set bottom alignment - content should align to bottom of inner area
        doll.set_content_align(Some("bottom".to_string()));

        // Widget: 8 rows total, with bottom border = 7 inner rows
        // Content is 6 rows, so with bottom align: row_offset = 7 - 6 = 1
        let area = Rect::new(0, 0, 10, 8);
        let mut buf = Buffer::empty(area);
        doll.render(area, &mut buf);

        // Row 7 is the bottom border - should have border chars, not content
        assert_eq!(
            buf[(0, 7)].symbol(),
            "└",
            "Bottom-left corner should have border character"
        );

        // Check that row 7 doesn't have injury content
        for col in 1..9 {
            let cell = &buf[(col, 7)];
            let sym = cell.symbol();
            assert!(
                sym == " " || sym == "─",
                "Row 7 (bottom border) should not have injury content at col {}, got: '{}'",
                col,
                sym
            );
        }

        // Row 0 should be empty (content aligned to bottom with no title)
        let eye_should_not_be_here = buf[(1, 0)].symbol();
        assert_eq!(
            eye_should_not_be_here, " ",
            "Row 0 should be empty when content is bottom-aligned with no title"
        );

        // Row 1 should have content (eyes) since content shifted down by 1
        let eye_char = buf[(1, 1)].symbol();
        assert!(
            eye_char != " ",
            "Row 1 should have content (eye) when bottom-aligned, got: '{}'",
            eye_char
        );
    }

    #[test]
    fn title_renders_in_content_area_when_no_top_border() {
        // When show_title is ON but top border is OFF,
        // the title should render in the first row of content area
        let mut doll = InjuryDoll::new("Title");
        doll.set_border_config(true, Some("single".to_string()), None);
        doll.set_border_sides(BorderSides {
            left: true,
            right: false,
            top: false,  // No top border
            bottom: true,
        });

        let area = Rect::new(0, 0, 10, 8);
        let mut buf = Buffer::empty(area);
        doll.render(area, &mut buf);

        // Row 0 should have the title "T" (first char) since there's no top border
        let title_char = buf[(1, 0)].symbol();
        assert_eq!(
            title_char, "T",
            "Row 0 should have title character when no top border, got: '{}'",
            title_char
        );
    }
}

