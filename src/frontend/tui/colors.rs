use anyhow::Result;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use crate::config::ColorMode;

// Global color mode - thread-local so it's set once at startup and used everywhere
thread_local! {
    static GLOBAL_COLOR_MODE: Cell<ColorMode> = const { Cell::new(ColorMode::Direct) };
    // Palette lookup: hex color (lowercase, with #) → slot number
    static PALETTE_LOOKUP: RefCell<HashMap<String, u8>> = RefCell::new(HashMap::new());
}

/// Set the global color mode for all color parsing
/// Call this once at frontend startup with the config value
pub fn set_global_color_mode(mode: ColorMode) {
    GLOBAL_COLOR_MODE.with(|m| m.set(mode));
    tracing::info!("Global color mode set to {:?}", mode);
}

/// Get the current global color mode
pub fn get_global_color_mode() -> ColorMode {
    GLOBAL_COLOR_MODE.with(|m| m.get())
}

/// Initialize the palette lookup from config
///
/// This builds a HashMap from hex color → slot number for all palette colors
/// that have a slot assignment. Call this once at startup when color_mode is Slot.
pub fn init_palette_lookup(palette: &[crate::config::PaletteColor]) {
    PALETTE_LOOKUP.with(|lookup| {
        let mut map = lookup.borrow_mut();
        map.clear();

        for color in palette {
            if let Some(slot) = color.slot {
                // Normalize hex to lowercase with #
                let hex = color.color.trim();
                let normalized = if hex.starts_with('#') {
                    hex.to_lowercase()
                } else {
                    format!("#{}", hex.to_lowercase())
                };
                map.insert(normalized, slot);
            }
        }

        tracing::info!("Initialized palette lookup with {} color mappings", map.len());
    });
}

/// Look up hex color in palette, returning slot number if found
fn lookup_hex_to_slot(hex: &str) -> Option<u8> {
    // Normalize hex to lowercase with #
    let normalized = if hex.starts_with('#') {
        hex.to_lowercase()
    } else {
        format!("#{}", hex.to_lowercase())
    };

    PALETTE_LOOKUP.with(|lookup| {
        lookup.borrow().get(&normalized).copied()
    })
}

/// Convert raw RGB values to ratatui Color
///
/// In Direct mode: Returns Color::Rgb for true color terminals
/// In Slot mode: Looks up the color in the palette map first, falls back to nearest slot
pub fn rgb_to_ratatui_color(r: u8, g: u8, b: u8) -> ratatui::style::Color {
    match get_global_color_mode() {
        ColorMode::Direct => ratatui::style::Color::Rgb(r, g, b),
        ColorMode::Slot => {
            // First try palette lookup
            let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
            if let Some(slot) = lookup_hex_to_slot(&hex) {
                ratatui::style::Color::Indexed(slot)
            } else {
                // Fall back to nearest slot calculation
                ratatui::style::Color::Indexed(rgb_to_nearest_slot(r, g, b))
            }
        }
    }
}

/// Parse a hex color string like "#RRGGBB" into ratatui Color
/// Always returns Color::Rgb for maximum compatibility - use parse_hex_color_with_mode for explicit control
pub fn parse_hex_color(hex: &str) -> Result<ratatui::style::Color> {
    let hex = hex.trim_start_matches('#');

    if hex.len() != 6 {
        return Err(anyhow::anyhow!("Invalid hex color length"));
    }

    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;

    // Always return RGB - Slot mode conversion happens at render time via crossterm_bridge
    Ok(ratatui::style::Color::Rgb(r, g, b))
}

/// Parse a hex color string with color mode awareness
///
/// In Direct mode, returns Color::Rgb(r, g, b) for true color terminals.
/// In Slot mode, returns Color::Indexed(n) using nearest 256-color match.
pub fn parse_hex_color_with_mode(hex: &str, mode: ColorMode) -> Result<ratatui::style::Color> {
    let hex = hex.trim_start_matches('#');

    if hex.len() != 6 {
        return Err(anyhow::anyhow!("Invalid hex color length"));
    }

    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;

    match mode {
        ColorMode::Direct => Ok(ratatui::style::Color::Rgb(r, g, b)),
        ColorMode::Slot => Ok(ratatui::style::Color::Indexed(rgb_to_nearest_slot(r, g, b))),
    }
}

pub fn color_to_hex_string(color: &crate::frontend::common::Color) -> Option<String> {
    // Color is now a simple RGB struct
    Some(format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b))
}

// OLD functions no longer needed after Phase 2 refactoring
#[allow(dead_code)]
pub(crate) fn _old_color_to_hex_string(color: &ratatui::style::Color) -> Option<String> {
    _old_color_to_rgb(color).map(|(r, g, b)| format!("#{:02x}{:02x}{:02x}", r, g, b))
}

#[allow(dead_code)]
pub(crate) fn _old_color_to_rgb(color: &ratatui::style::Color) -> Option<(u8, u8, u8)> {
    use ratatui::style::Color;

    match color {
        Color::Rgb(r, g, b) => Some((*r, *g, *b)),
        Color::Indexed(index) => Some(indexed_color_to_rgb(*index)),
        Color::Reset => None,
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((205, 0, 0)),
        Color::Green => Some((0, 205, 0)),
        Color::Yellow => Some((205, 205, 0)),
        Color::Blue => Some((0, 0, 205)),
        Color::Magenta => Some((205, 0, 205)),
        Color::Cyan => Some((0, 205, 205)),
        Color::Gray => Some((192, 192, 192)),
        Color::DarkGray => Some((128, 128, 128)),
        Color::LightRed => Some((255, 102, 102)),
        Color::LightGreen => Some((144, 238, 144)),
        Color::LightYellow => Some((255, 255, 102)),
        Color::LightBlue => Some((173, 216, 230)),
        Color::LightMagenta => Some((255, 119, 255)),
        Color::LightCyan => Some((224, 255, 255)),
        Color::White => Some((255, 255, 255)),
    }
}

fn indexed_color_to_rgb(index: u8) -> (u8, u8, u8) {
    const STANDARD_COLORS: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (128, 0, 0),
        (0, 128, 0),
        (128, 128, 0),
        (0, 0, 128),
        (128, 0, 128),
        (0, 128, 128),
        (192, 192, 192),
        (128, 128, 128),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (0, 0, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];

    if index < 16 {
        return STANDARD_COLORS[index as usize];
    }

    if index <= 231 {
        let level = index as usize - 16;
        let r = level / 36;
        let g = (level % 36) / 6;
        let b = level % 6;
        let levels = [0, 95, 135, 175, 215, 255];
        return (levels[r], levels[g], levels[b]);
    }

    // Grayscale ramp
    let gray = 8 + (index.saturating_sub(232)) * 10;
    (gray, gray, gray)
}

/// Convert RGB to nearest xterm 256-color palette slot
///
/// The 256-color palette is organized as:
/// - Slots 0-15: Standard ANSI colors (terminal-dependent)
/// - Slots 16-231: 6x6x6 color cube (216 colors)
/// - Slots 232-255: Grayscale ramp (24 shades)
///
/// This function maps RGB values to the nearest color cube or grayscale slot,
/// avoiding ANSI slots 0-15 which may vary by terminal theme.
pub fn rgb_to_nearest_slot(r: u8, g: u8, b: u8) -> u8 {
    // Check if grayscale (r == g == b)
    if r == g && g == b {
        // Use grayscale ramp for pure grays
        if r < 8 {
            return 16; // Near black - use darkest color cube entry
        }
        if r > 248 {
            return 231; // Near white - use lightest color cube entry
        }
        // Grayscale ramp: slots 232-255 represent grays from 8 to 238
        // Formula: gray = 8 + (index - 232) * 10
        // Inverse: index = 232 + (gray - 8) / 10
        return (232 + (r - 8) / 10).min(255);
    }

    // Map to 6x6x6 color cube (slots 16-231)
    // Each axis has 6 levels: 0, 95, 135, 175, 215, 255
    let to_6 = |v: u8| -> u8 {
        match v {
            0..=47 => 0,
            48..=114 => 1,
            115..=154 => 2,
            155..=194 => 3,
            195..=234 => 4,
            _ => 5,
        }
    };

    // Color cube formula: 16 + 36*r + 6*g + b
    16 + 36 * to_6(r) + 6 * to_6(g) + to_6(b)
}

pub fn blend_colors_hex(
    base: &crate::frontend::common::Color,
    target: &crate::frontend::common::Color,
    ratio: f32,
) -> Option<String> {
    // Color is now a simple RGB struct
    let (br, bg, bb) = (base.r, base.g, base.b);
    let (tr, tg, tb) = (target.r, target.g, target.b);
    let ratio = ratio.clamp(0.0, 1.0);
    let blend = |b: u8, t: u8| -> u8 {
        (b as f32 + (t as f32 - b as f32) * ratio)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        blend(br, tr),
        blend(bg, tg),
        blend(bb, tb)
    ))
}

pub(crate) fn normalize_color(opt: &Option<String>) -> Option<String> {
    opt.as_ref().and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == "-" {
            None
        } else {
            // Try to resolve color names to hex codes
            parse_color_flexible(trimmed).or_else(|| Some(trimmed.to_string()))
        }
    })
}

/// Parse a color string that can be:
/// - A hex code: "#RRGGBB" or "RRGGBB"
/// - A standard color name: "red", "blue", "green", etc.
/// Returns the color as a hex string if successful
pub fn parse_color_flexible(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return None;
    }

    // Try hex code first
    let hex_input = trimmed.trim_start_matches('#');
    if hex_input.len() == 6 && hex_input.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(format!("#{}", hex_input.to_lowercase()));
    }

    // Try standard color names (case-insensitive)
    let color_lower = trimmed.to_lowercase();
    let rgb = match color_lower.as_str() {
        // Basic ANSI colors
        "black" => Some((0, 0, 0)),
        "red" => Some((205, 0, 0)),
        "green" => Some((0, 205, 0)),
        "yellow" => Some((205, 205, 0)),
        "blue" => Some((0, 0, 205)),
        "magenta" | "purple" => Some((205, 0, 205)),
        "cyan" => Some((0, 205, 205)),
        "gray" | "grey" => Some((192, 192, 192)),
        "white" => Some((255, 255, 255)),

        // Light variants
        "darkgray" | "darkgrey" | "dark_gray" | "dark_grey" => Some((128, 128, 128)),
        "lightred" | "light_red" => Some((255, 102, 102)),
        "lightgreen" | "light_green" | "lime" => Some((144, 238, 144)),
        "lightyellow" | "light_yellow" => Some((255, 255, 102)),
        "lightblue" | "light_blue" => Some((173, 216, 230)),
        "lightmagenta" | "light_magenta" | "pink" => Some((255, 119, 255)),
        "lightcyan" | "light_cyan" => Some((224, 255, 255)),

        // Extended web colors
        "orange" => Some((255, 165, 0)),
        "brown" => Some((165, 42, 42)),
        "maroon" => Some((128, 0, 0)),
        "olive" => Some((128, 128, 0)),
        "navy" => Some((0, 0, 128)),
        "teal" => Some((0, 128, 128)),
        "aqua" => Some((0, 255, 255)),
        "fuchsia" => Some((255, 0, 255)),
        "silver" => Some((192, 192, 192)),
        "gold" => Some((255, 215, 0)),
        "coral" => Some((255, 127, 80)),
        "salmon" => Some((250, 128, 114)),
        "violet" => Some((238, 130, 238)),
        "indigo" => Some((75, 0, 130)),
        "crimson" => Some((220, 20, 60)),
        "turquoise" => Some((64, 224, 208)),
        "tan" => Some((210, 180, 140)),
        "khaki" => Some((240, 230, 140)),
        "beige" => Some((245, 245, 220)),
        "ivory" => Some((255, 255, 240)),
        "azure" => Some((240, 255, 255)),
        "lavender" => Some((230, 230, 250)),
        "plum" => Some((221, 160, 221)),
        "orchid" => Some((218, 112, 214)),
        "peru" => Some((205, 133, 63)),
        "sienna" => Some((160, 82, 45)),
        "chocolate" => Some((210, 105, 30)),
        "tomato" => Some((255, 99, 71)),
        "firebrick" => Some((178, 34, 34)),
        "darkred" | "dark_red" => Some((139, 0, 0)),
        "darkgreen" | "dark_green" => Some((0, 100, 0)),
        "darkblue" | "dark_blue" => Some((0, 0, 139)),
        "darkcyan" | "dark_cyan" => Some((0, 139, 139)),
        "darkmagenta" | "dark_magenta" => Some((139, 0, 139)),
        "darkorange" | "dark_orange" => Some((255, 140, 0)),
        "darkviolet" | "dark_violet" => Some((148, 0, 211)),
        "deeppink" | "deep_pink" => Some((255, 20, 147)),
        "deepskyblue" | "deep_sky_blue" => Some((0, 191, 255)),
        "dodgerblue" | "dodger_blue" => Some((30, 144, 255)),
        "forestgreen" | "forest_green" => Some((34, 139, 34)),
        "hotpink" | "hot_pink" => Some((255, 105, 180)),
        "limegreen" | "lime_green" => Some((50, 205, 50)),
        "mediumblue" | "medium_blue" => Some((0, 0, 205)),
        "mediumvioletred" | "medium_violet_red" => Some((199, 21, 133)),
        "midnightblue" | "midnight_blue" => Some((25, 25, 112)),
        "royalblue" | "royal_blue" => Some((65, 105, 225)),
        "seagreen" | "sea_green" => Some((46, 139, 87)),
        "skyblue" | "sky_blue" => Some((135, 206, 235)),
        "slateblue" | "slate_blue" => Some((106, 90, 205)),
        "slategray" | "slate_gray" | "slategrey" | "slate_grey" => Some((112, 128, 144)),
        "springgreen" | "spring_green" => Some((0, 255, 127)),
        "steelblue" | "steel_blue" => Some((70, 130, 180)),
        "yellowgreen" | "yellow_green" => Some((154, 205, 50)),
        _ => None,
    };

    rgb.map(|(r, g, b)| format!("#{:02x}{:02x}{:02x}", r, g, b))
}

/// Parse a color string to ratatui Color
/// Supports hex codes and standard color names
/// Respects the global color mode setting
pub fn parse_color_to_ratatui(input: &str) -> Option<ratatui::style::Color> {
    // parse_hex_color now respects global color mode automatically
    parse_color_flexible(input).and_then(|hex| parse_hex_color(&hex).ok())
}

/// Parse a color string to ratatui Color with color mode awareness
/// In Direct mode: returns Color::Rgb for true color terminals
/// In Slot mode: returns Color::Indexed for 256-color terminals (like macOS Terminal.app)
pub fn parse_color_to_ratatui_with_mode(input: &str, mode: ColorMode) -> Option<ratatui::style::Color> {
    parse_color_flexible(input).and_then(|hex| parse_hex_color_with_mode(&hex, mode).ok())
}

#[derive(Clone)]
pub struct WindowColors {
    pub border: Option<String>,
    pub background: Option<String>,
    pub text: Option<String>,
}

pub fn resolve_window_colors(
    base: &crate::config::WindowBase,
    theme: &crate::theme::AppTheme,
) -> WindowColors {
    let border =
        normalize_color(&base.border_color).or_else(|| color_to_hex_string(&theme.window_border));
    let background = if base.transparent_background {
        None
    } else {
        normalize_color(&base.background_color)
            .or_else(|| color_to_hex_string(&theme.window_background))
    };
    let text =
        normalize_color(&base.text_color).or_else(|| color_to_hex_string(&theme.text_primary));

    WindowColors {
        border,
        background,
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_flexible_hex_with_hash() {
        assert_eq!(
            parse_color_flexible("#ff0000"),
            Some("#ff0000".to_string())
        );
        assert_eq!(
            parse_color_flexible("#00FF00"),
            Some("#00ff00".to_string())
        );
    }

    #[test]
    fn parse_color_flexible_hex_without_hash() {
        assert_eq!(
            parse_color_flexible("ff0000"),
            Some("#ff0000".to_string())
        );
    }

    #[test]
    fn parse_color_flexible_basic_colors() {
        assert_eq!(parse_color_flexible("red"), Some("#cd0000".to_string()));
        assert_eq!(parse_color_flexible("RED"), Some("#cd0000".to_string()));
        assert_eq!(parse_color_flexible("blue"), Some("#0000cd".to_string()));
        assert_eq!(parse_color_flexible("green"), Some("#00cd00".to_string()));
        assert_eq!(parse_color_flexible("white"), Some("#ffffff".to_string()));
        assert_eq!(parse_color_flexible("black"), Some("#000000".to_string()));
    }

    #[test]
    fn parse_color_flexible_extended_colors() {
        assert_eq!(parse_color_flexible("orange"), Some("#ffa500".to_string()));
        assert_eq!(parse_color_flexible("pink"), Some("#ff77ff".to_string()));
        assert_eq!(parse_color_flexible("gold"), Some("#ffd700".to_string()));
        assert_eq!(parse_color_flexible("navy"), Some("#000080".to_string()));
        assert_eq!(parse_color_flexible("teal"), Some("#008080".to_string()));
    }

    #[test]
    fn parse_color_flexible_light_variants() {
        assert!(parse_color_flexible("lightred").is_some());
        assert!(parse_color_flexible("light_red").is_some());
        assert!(parse_color_flexible("LightRed").is_some());
    }

    #[test]
    fn parse_color_flexible_empty_and_dash() {
        assert_eq!(parse_color_flexible(""), None);
        assert_eq!(parse_color_flexible("-"), None);
        assert_eq!(parse_color_flexible("  "), None);
    }

    #[test]
    fn parse_color_flexible_invalid_returns_none() {
        assert_eq!(parse_color_flexible("notacolor"), None);
        assert_eq!(parse_color_flexible("invalid"), None);
    }

    #[test]
    fn normalize_color_resolves_names() {
        let red = Some("red".to_string());
        assert_eq!(normalize_color(&red), Some("#cd0000".to_string()));

        let hex = Some("#ff0000".to_string());
        assert_eq!(normalize_color(&hex), Some("#ff0000".to_string()));
    }

    #[test]
    fn normalize_color_handles_none_and_empty() {
        assert_eq!(normalize_color(&None), None);
        assert_eq!(normalize_color(&Some("".to_string())), None);
        assert_eq!(normalize_color(&Some("-".to_string())), None);
    }

    #[test]
    fn parse_color_to_ratatui_works() {
        let color = parse_color_to_ratatui("red");
        assert!(color.is_some());

        let color = parse_color_to_ratatui("#ff0000");
        assert!(color.is_some());
    }
}
