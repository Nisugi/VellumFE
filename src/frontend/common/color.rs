//! Frontend-agnostic color representation.
//!
//! This module provides UI-agnostic color types that can be converted to
//! TUI-specific (ratatui) or GUI-specific (egui/iced) color representations.

/// Represents a color in RGB format with optional alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    /// Create a new RGB color
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Create a color from hex string (e.g., "#FF5733")
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

        Some(Self::rgb(r, g, b))
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    // ANSI 256-color palette (standard terminal colors)
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const RED: Self = Self::rgb(255, 0, 0);
    pub const GREEN: Self = Self::rgb(0, 255, 0);
    pub const YELLOW: Self = Self::rgb(255, 255, 0);
    pub const BLUE: Self = Self::rgb(0, 0, 255);
    pub const MAGENTA: Self = Self::rgb(255, 0, 255);
    pub const CYAN: Self = Self::rgb(0, 255, 255);
    pub const GRAY: Self = Self::rgb(128, 128, 128);
    pub const DARK_GRAY: Self = Self::rgb(64, 64, 64);
    pub const LIGHT_RED: Self = Self::rgb(255, 128, 128);
    pub const LIGHT_GREEN: Self = Self::rgb(128, 255, 128);
    pub const LIGHT_YELLOW: Self = Self::rgb(255, 255, 128);
    pub const LIGHT_BLUE: Self = Self::rgb(128, 128, 255);
    pub const LIGHT_MAGENTA: Self = Self::rgb(255, 128, 255);
    pub const LIGHT_CYAN: Self = Self::rgb(128, 255, 255);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    // Common UI colors
    pub const TRANSPARENT: Self = Self::rgb(0, 0, 0); // Will be handled specially in rendering
}

/// Named color variants for ease of use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedColor {
    /// Standard ANSI colors
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,

    /// RGB color
    Rgb(u8, u8, u8),

    /// Indexed ANSI 256-color palette (0-255)
    Indexed(u8),

    /// Reset to default terminal color
    Reset,
}

impl NamedColor {
    /// Convert to RGB color
    pub fn to_rgb(&self) -> Color {
        match self {
            NamedColor::Black => Color::BLACK,
            NamedColor::Red => Color::RED,
            NamedColor::Green => Color::GREEN,
            NamedColor::Yellow => Color::YELLOW,
            NamedColor::Blue => Color::BLUE,
            NamedColor::Magenta => Color::MAGENTA,
            NamedColor::Cyan => Color::CYAN,
            NamedColor::Gray => Color::GRAY,
            NamedColor::DarkGray => Color::DARK_GRAY,
            NamedColor::LightRed => Color::LIGHT_RED,
            NamedColor::LightGreen => Color::LIGHT_GREEN,
            NamedColor::LightYellow => Color::LIGHT_YELLOW,
            NamedColor::LightBlue => Color::LIGHT_BLUE,
            NamedColor::LightMagenta => Color::LIGHT_MAGENTA,
            NamedColor::LightCyan => Color::LIGHT_CYAN,
            NamedColor::White => Color::WHITE,
            NamedColor::Rgb(r, g, b) => Color::rgb(*r, *g, *b),
            NamedColor::Indexed(idx) => {
                // ANSI 256-color to RGB approximation
                // This is a simplified conversion - real terminals may vary
                match idx {
                    0..=15 => {
                        // Basic 16 colors
                        match idx {
                            0 => Color::BLACK,
                            1 => Color::RED,
                            2 => Color::GREEN,
                            3 => Color::YELLOW,
                            4 => Color::BLUE,
                            5 => Color::MAGENTA,
                            6 => Color::CYAN,
                            7 => Color::GRAY,
                            8 => Color::DARK_GRAY,
                            9 => Color::LIGHT_RED,
                            10 => Color::LIGHT_GREEN,
                            11 => Color::LIGHT_YELLOW,
                            12 => Color::LIGHT_BLUE,
                            13 => Color::LIGHT_MAGENTA,
                            14 => Color::LIGHT_CYAN,
                            15 => Color::WHITE,
                            _ => Color::BLACK,
                        }
                    }
                    16..=231 => {
                        // 216-color cube (6x6x6)
                        let idx = idx - 16;
                        let r = ((idx / 36) * 51) as u8;
                        let g = (((idx % 36) / 6) * 51) as u8;
                        let b = ((idx % 6) * 51) as u8;
                        Color::rgb(r, g, b)
                    }
                    232..=255 => {
                        // Grayscale ramp
                        let gray = ((idx - 232) * 10 + 8) as u8;
                        Color::rgb(gray, gray, gray)
                    }
                }
            }
            NamedColor::Reset => Color::WHITE, // Default to white
        }
    }
}

impl From<Color> for NamedColor {
    fn from(color: Color) -> Self {
        NamedColor::Rgb(color.r, color.g, color.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // Color struct construction tests
    // ===========================================

    #[test]
    fn test_color_rgb_construction() {
        let color = Color::rgb(100, 150, 200);
        assert_eq!(color.r, 100);
        assert_eq!(color.g, 150);
        assert_eq!(color.b, 200);
    }

    #[test]
    fn test_color_rgb_min_values() {
        let color = Color::rgb(0, 0, 0);
        assert_eq!(color, Color::BLACK);
    }

    #[test]
    fn test_color_rgb_max_values() {
        let color = Color::rgb(255, 255, 255);
        assert_eq!(color, Color::WHITE);
    }

    // ===========================================
    // Color constants tests
    // ===========================================

    #[test]
    fn test_color_constants_basic() {
        assert_eq!(Color::RED, Color::rgb(255, 0, 0));
        assert_eq!(Color::GREEN, Color::rgb(0, 255, 0));
        assert_eq!(Color::BLUE, Color::rgb(0, 0, 255));
    }

    #[test]
    fn test_color_constants_black_white() {
        assert_eq!(Color::BLACK, Color::rgb(0, 0, 0));
        assert_eq!(Color::WHITE, Color::rgb(255, 255, 255));
    }

    #[test]
    fn test_color_constants_secondary() {
        assert_eq!(Color::YELLOW, Color::rgb(255, 255, 0));
        assert_eq!(Color::MAGENTA, Color::rgb(255, 0, 255));
        assert_eq!(Color::CYAN, Color::rgb(0, 255, 255));
    }

    #[test]
    fn test_color_constants_gray() {
        assert_eq!(Color::GRAY, Color::rgb(128, 128, 128));
        assert_eq!(Color::DARK_GRAY, Color::rgb(64, 64, 64));
    }

    #[test]
    fn test_color_constants_light_variants() {
        assert_eq!(Color::LIGHT_RED, Color::rgb(255, 128, 128));
        assert_eq!(Color::LIGHT_GREEN, Color::rgb(128, 255, 128));
        assert_eq!(Color::LIGHT_YELLOW, Color::rgb(255, 255, 128));
        assert_eq!(Color::LIGHT_BLUE, Color::rgb(128, 128, 255));
        assert_eq!(Color::LIGHT_MAGENTA, Color::rgb(255, 128, 255));
        assert_eq!(Color::LIGHT_CYAN, Color::rgb(128, 255, 255));
    }

    #[test]
    fn test_color_transparent() {
        // TRANSPARENT is black (0,0,0) - handled specially in rendering
        assert_eq!(Color::TRANSPARENT, Color::rgb(0, 0, 0));
    }

    // ===========================================
    // Hex conversion tests
    // ===========================================

    #[test]
    fn test_from_hex_with_hash() {
        let color = Color::from_hex("#FF5733").unwrap();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 87);
        assert_eq!(color.b, 51);
    }

    #[test]
    fn test_from_hex_without_hash() {
        let color = Color::from_hex("FF5733").unwrap();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 87);
        assert_eq!(color.b, 51);
    }

    #[test]
    fn test_from_hex_lowercase() {
        let color = Color::from_hex("#ff5733").unwrap();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 87);
        assert_eq!(color.b, 51);
    }

    #[test]
    fn test_from_hex_mixed_case() {
        let color = Color::from_hex("#Ff5733").unwrap();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 87);
        assert_eq!(color.b, 51);
    }

    #[test]
    fn test_from_hex_black() {
        let color = Color::from_hex("#000000").unwrap();
        assert_eq!(color, Color::BLACK);
    }

    #[test]
    fn test_from_hex_white() {
        let color = Color::from_hex("#FFFFFF").unwrap();
        assert_eq!(color, Color::WHITE);
    }

    #[test]
    fn test_from_hex_too_short() {
        assert!(Color::from_hex("#FFF").is_none());
        assert!(Color::from_hex("ABC").is_none());
    }

    #[test]
    fn test_from_hex_too_long() {
        assert!(Color::from_hex("#FF5733FF").is_none());
        assert!(Color::from_hex("FF5733FF").is_none());
    }

    #[test]
    fn test_from_hex_invalid_characters() {
        assert!(Color::from_hex("#GGGGGG").is_none());
        assert!(Color::from_hex("#ZZZZZZ").is_none());
        assert!(Color::from_hex("#12345G").is_none());
    }

    #[test]
    fn test_from_hex_empty() {
        assert!(Color::from_hex("").is_none());
        assert!(Color::from_hex("#").is_none());
    }

    #[test]
    fn test_to_hex_uppercase() {
        let color = Color::rgb(255, 87, 51);
        assert_eq!(color.to_hex(), "#FF5733");
    }

    #[test]
    fn test_to_hex_padded() {
        let color = Color::rgb(1, 2, 3);
        assert_eq!(color.to_hex(), "#010203");
    }

    #[test]
    fn test_to_hex_black() {
        assert_eq!(Color::BLACK.to_hex(), "#000000");
    }

    #[test]
    fn test_to_hex_white() {
        assert_eq!(Color::WHITE.to_hex(), "#FFFFFF");
    }

    #[test]
    fn test_hex_round_trip() {
        let original = Color::rgb(128, 64, 32);
        let hex = original.to_hex();
        let parsed = Color::from_hex(&hex).unwrap();
        assert_eq!(original, parsed);
    }

    // ===========================================
    // NamedColor tests
    // ===========================================

    #[test]
    fn test_named_color_basic() {
        assert_eq!(NamedColor::Black.to_rgb(), Color::BLACK);
        assert_eq!(NamedColor::Red.to_rgb(), Color::RED);
        assert_eq!(NamedColor::Green.to_rgb(), Color::GREEN);
        assert_eq!(NamedColor::Yellow.to_rgb(), Color::YELLOW);
        assert_eq!(NamedColor::Blue.to_rgb(), Color::BLUE);
        assert_eq!(NamedColor::Magenta.to_rgb(), Color::MAGENTA);
        assert_eq!(NamedColor::Cyan.to_rgb(), Color::CYAN);
        assert_eq!(NamedColor::White.to_rgb(), Color::WHITE);
    }

    #[test]
    fn test_named_color_gray() {
        assert_eq!(NamedColor::Gray.to_rgb(), Color::GRAY);
        assert_eq!(NamedColor::DarkGray.to_rgb(), Color::DARK_GRAY);
    }

    #[test]
    fn test_named_color_light_variants() {
        assert_eq!(NamedColor::LightRed.to_rgb(), Color::LIGHT_RED);
        assert_eq!(NamedColor::LightGreen.to_rgb(), Color::LIGHT_GREEN);
        assert_eq!(NamedColor::LightYellow.to_rgb(), Color::LIGHT_YELLOW);
        assert_eq!(NamedColor::LightBlue.to_rgb(), Color::LIGHT_BLUE);
        assert_eq!(NamedColor::LightMagenta.to_rgb(), Color::LIGHT_MAGENTA);
        assert_eq!(NamedColor::LightCyan.to_rgb(), Color::LIGHT_CYAN);
    }

    #[test]
    fn test_named_color_rgb() {
        let color = NamedColor::Rgb(100, 150, 200);
        assert_eq!(color.to_rgb(), Color::rgb(100, 150, 200));
    }

    #[test]
    fn test_named_color_reset() {
        // Reset defaults to white
        assert_eq!(NamedColor::Reset.to_rgb(), Color::WHITE);
    }

    // ===========================================
    // Indexed color tests (ANSI 256-color palette)
    // ===========================================

    #[test]
    fn test_indexed_basic_16_colors() {
        // Test the first 16 ANSI colors (0-15)
        assert_eq!(NamedColor::Indexed(0).to_rgb(), Color::BLACK);
        assert_eq!(NamedColor::Indexed(1).to_rgb(), Color::RED);
        assert_eq!(NamedColor::Indexed(2).to_rgb(), Color::GREEN);
        assert_eq!(NamedColor::Indexed(3).to_rgb(), Color::YELLOW);
        assert_eq!(NamedColor::Indexed(4).to_rgb(), Color::BLUE);
        assert_eq!(NamedColor::Indexed(5).to_rgb(), Color::MAGENTA);
        assert_eq!(NamedColor::Indexed(6).to_rgb(), Color::CYAN);
        assert_eq!(NamedColor::Indexed(7).to_rgb(), Color::GRAY);
    }

    #[test]
    fn test_indexed_bright_colors() {
        // Test bright colors (8-15)
        assert_eq!(NamedColor::Indexed(8).to_rgb(), Color::DARK_GRAY);
        assert_eq!(NamedColor::Indexed(9).to_rgb(), Color::LIGHT_RED);
        assert_eq!(NamedColor::Indexed(10).to_rgb(), Color::LIGHT_GREEN);
        assert_eq!(NamedColor::Indexed(11).to_rgb(), Color::LIGHT_YELLOW);
        assert_eq!(NamedColor::Indexed(12).to_rgb(), Color::LIGHT_BLUE);
        assert_eq!(NamedColor::Indexed(13).to_rgb(), Color::LIGHT_MAGENTA);
        assert_eq!(NamedColor::Indexed(14).to_rgb(), Color::LIGHT_CYAN);
        assert_eq!(NamedColor::Indexed(15).to_rgb(), Color::WHITE);
    }

    #[test]
    fn test_indexed_color_cube_start() {
        // Index 16 is the start of the 6x6x6 color cube (r=0, g=0, b=0)
        let color = NamedColor::Indexed(16).to_rgb();
        assert_eq!(color, Color::rgb(0, 0, 0));
    }

    #[test]
    fn test_indexed_color_cube_end() {
        // Index 231 is the end of the color cube (r=5, g=5, b=5)
        let color = NamedColor::Indexed(231).to_rgb();
        assert_eq!(color, Color::rgb(255, 255, 255));
    }

    #[test]
    fn test_indexed_color_cube_red() {
        // Index 196 = 16 + (5*36) = pure red in color cube
        let color = NamedColor::Indexed(196).to_rgb();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
    }

    #[test]
    fn test_indexed_color_cube_calculation() {
        // Test color cube formula: r = (idx-16)/36, g = ((idx-16)%36)/6, b = (idx-16)%6
        // Index 21 = 16 + 5 => r=0, g=0, b=5 => (0, 0, 255)
        let color = NamedColor::Indexed(21).to_rgb();
        assert_eq!(color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn test_indexed_grayscale_start() {
        // Index 232 starts the grayscale ramp
        let color = NamedColor::Indexed(232).to_rgb();
        // gray = (232 - 232) * 10 + 8 = 8
        assert_eq!(color, Color::rgb(8, 8, 8));
    }

    #[test]
    fn test_indexed_grayscale_end() {
        // Index 255 is the brightest grayscale
        let color = NamedColor::Indexed(255).to_rgb();
        // gray = (255 - 232) * 10 + 8 = 238
        assert_eq!(color, Color::rgb(238, 238, 238));
    }

    #[test]
    fn test_indexed_grayscale_mid() {
        // Test a middle grayscale value
        let color = NamedColor::Indexed(243).to_rgb();
        // gray = (243 - 232) * 10 + 8 = 118
        assert_eq!(color, Color::rgb(118, 118, 118));
    }

    // ===========================================
    // From trait tests
    // ===========================================

    #[test]
    fn test_from_color_to_named_color() {
        let color = Color::rgb(100, 150, 200);
        let named: NamedColor = color.into();
        assert_eq!(named, NamedColor::Rgb(100, 150, 200));
    }

    #[test]
    fn test_from_color_round_trip() {
        let original = Color::rgb(50, 100, 150);
        let named: NamedColor = original.into();
        let back = named.to_rgb();
        assert_eq!(original, back);
    }

    // ===========================================
    // Trait implementation tests
    // ===========================================

    #[test]
    fn test_color_clone() {
        let color = Color::rgb(100, 150, 200);
        let cloned = color.clone();
        assert_eq!(color, cloned);
    }

    #[test]
    fn test_color_copy() {
        let color = Color::rgb(100, 150, 200);
        let copied = color; // Copy happens here
        assert_eq!(color, copied); // Original still accessible
    }

    #[test]
    fn test_color_debug() {
        let color = Color::rgb(255, 0, 0);
        let debug_str = format!("{:?}", color);
        assert!(debug_str.contains("255"));
        assert!(debug_str.contains("0"));
    }

    #[test]
    fn test_color_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Color::RED);
        set.insert(Color::GREEN);
        set.insert(Color::BLUE);
        assert_eq!(set.len(), 3);
        assert!(set.contains(&Color::RED));
    }

    #[test]
    fn test_named_color_clone() {
        let color = NamedColor::Rgb(100, 150, 200);
        let cloned = color.clone();
        assert_eq!(color, cloned);
    }

    #[test]
    fn test_named_color_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(NamedColor::Red);
        set.insert(NamedColor::Green);
        set.insert(NamedColor::Indexed(42));
        assert_eq!(set.len(), 3);
        assert!(set.contains(&NamedColor::Red));
    }
}
