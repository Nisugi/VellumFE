use crate::config::ColorMode;
use crate::frontend::common::color::parse_color_flexible;
use anyhow::Result;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

// Global color mode - thread-local so it's set once at startup and used everywhere
thread_local! {
    static GLOBAL_COLOR_MODE: Cell<ColorMode> = const { Cell::new(ColorMode::Direct) };
    // Palette lookup: hex color (lowercase, with #) → slot number
    static PALETTE_LOOKUP: RefCell<HashMap<String, u8>> = RefCell::new(HashMap::new());
    // Memoized parse_color_to_ratatui results. Widget renderers call it per
    // segment per frame with a bounded set of config-sourced color strings.
    // Cleared whenever the mode or palette changes (both alter the mapping).
    static COLOR_PARSE_CACHE: RefCell<HashMap<String, Option<ratatui::style::Color>>> =
        RefCell::new(HashMap::new());
}

/// Leak backstop for the parse cache; config colors are a bounded set,
/// so hitting this indicates pathological input (e.g. per-line unique colors)
const COLOR_PARSE_CACHE_MAX: usize = 4096;

fn clear_color_parse_cache() {
    COLOR_PARSE_CACHE.with(|cache| cache.borrow_mut().clear());
}

/// Set the global color mode for all color parsing
/// Call this once at frontend startup with the config value
pub fn set_global_color_mode(mode: ColorMode) {
    GLOBAL_COLOR_MODE.with(|m| m.set(mode));
    clear_color_parse_cache();
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
    // Palette changes alter hex->slot resolution; cached parses are stale
    clear_color_parse_cache();
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

        tracing::info!(
            "Initialized palette lookup with {} color mappings",
            map.len()
        );
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

    PALETTE_LOOKUP.with(|lookup| lookup.borrow().get(&normalized).copied())
}

/// Convert raw RGB values to ratatui Color
///
/// In Direct mode: Returns Color::Rgb for true color terminals
/// In Slot mode: Looks up the color in the palette map first, falls back to nearest slot
pub fn rgb_to_ratatui_color(r: u8, g: u8, b: u8) -> ratatui::style::Color {
    match get_global_color_mode() {
        ColorMode::Direct => ratatui::style::Color::Rgb(r, g, b),
        ColorMode::Slot => {
            // First try palette lookup (user-defined slots via .setpalette)
            let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
            if let Some(slot) = lookup_hex_to_slot(&hex) {
                ratatui::style::Color::Indexed(slot)
            } else {
                // Fall back to nearest slot calculation
                ratatui::style::Color::Indexed(rgb_to_nearest_slot(r, g, b))
            }
        }
        ColorMode::Indexed => {
            // Always use nearest slot in standard 256-color palette
            // No palette lookup - for terminals without OSC4 support
            ratatui::style::Color::Indexed(rgb_to_nearest_slot(r, g, b))
        }
    }
}

/// Parse a hex color string like "#RRGGBB" into ratatui Color
/// Respects global color mode: returns Indexed in Slot mode, Rgb in Direct mode
pub fn parse_hex_color(hex: &str) -> Result<ratatui::style::Color> {
    let hex = hex.trim_start_matches('#');

    if hex.len() != 6 {
        return Err(anyhow::anyhow!("Invalid hex color length"));
    }

    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;

    // Use mode-aware conversion
    Ok(rgb_to_ratatui_color(r, g, b))
}

pub fn color_to_hex_string(color: &crate::frontend::common::Color) -> Option<String> {
    // Color is now a simple RGB struct
    Some(format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b))
}

/// Try to match RGB to standard ANSI colors (slots 0-15)
///
/// Returns Some(slot) if the RGB closely matches a standard ANSI color.
/// This is important for Profanity users who have terminal slots 0-15
/// programmed with custom colors - we want to emit ANSI codes like \e[31m
/// (red = slot 1) so their custom palette applies.
fn rgb_to_ansi_slot(r: u8, g: u8, b: u8) -> Option<u8> {
    // Standard ANSI color definitions (as used in parse_color_flexible)
    // These are the RGB values we convert color names to
    const ANSI_COLORS: [(u8, u8, u8, u8); 16] = [
        (0, 0, 0, 0),        // black
        (205, 0, 0, 1),      // red
        (0, 205, 0, 2),      // green
        (205, 205, 0, 3),    // yellow
        (0, 0, 205, 4),      // blue
        (205, 0, 205, 5),    // magenta
        (0, 205, 205, 6),    // cyan
        (192, 192, 192, 7),  // gray/white (normal)
        (128, 128, 128, 8),  // dark gray (bright black)
        (255, 0, 0, 9),      // bright red
        (0, 255, 0, 10),     // bright green
        (255, 255, 0, 11),   // bright yellow
        (0, 0, 255, 12),     // bright blue
        (255, 0, 255, 13),   // bright magenta
        (0, 255, 255, 14),   // bright cyan
        (255, 255, 255, 15), // bright white
    ];

    // Exact match check
    for (ar, ag, ab, slot) in ANSI_COLORS {
        if r == ar && g == ag && b == ab {
            return Some(slot);
        }
    }

    None
}

/// Convert RGB to nearest xterm 256-color palette slot
///
/// The 256-color palette is organized as:
/// - Slots 0-15: Standard ANSI colors (terminal-dependent)
/// - Slots 16-231: 6x6x6 color cube (216 colors)
/// - Slots 232-255: Grayscale ramp (24 shades)
///
/// This function first tries to match ANSI slots 0-15 (for Profanity users
/// who have custom terminal palettes), then falls back to the color cube.
pub fn rgb_to_nearest_slot(r: u8, g: u8, b: u8) -> u8 {
    // First try ANSI slots 0-15 for exact matches
    // This is critical for Profanity users whose terminal has custom palette
    if let Some(slot) = rgb_to_ansi_slot(r, g, b) {
        return slot;
    }
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

/// Parse a color string to ratatui Color
/// Supports hex codes and standard color names
/// Respects the global color mode setting
pub fn parse_color_to_ratatui(input: &str) -> Option<ratatui::style::Color> {
    // Memoized: renderers call this per segment per frame with a bounded set
    // of config colors. None results are cached too (misses repeat as often).
    if let Some(cached) = COLOR_PARSE_CACHE.with(|cache| cache.borrow().get(input).copied()) {
        return cached;
    }
    // parse_hex_color respects the global color mode automatically
    let parsed = parse_color_flexible(input).and_then(|hex| parse_hex_color(&hex).ok());
    COLOR_PARSE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= COLOR_PARSE_CACHE_MAX {
            cache.clear();
        }
        cache.insert(input.to_string(), parsed);
    });
    parsed
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
        assert_eq!(parse_color_flexible("#ff0000"), Some("#ff0000".to_string()));
        assert_eq!(parse_color_flexible("#00FF00"), Some("#00ff00".to_string()));
    }

    #[test]
    fn parse_color_flexible_hex_without_hash() {
        assert_eq!(parse_color_flexible("ff0000"), Some("#ff0000".to_string()));
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

    #[test]
    fn parse_color_cache_invalidated_by_mode_change() {
        // thread_local state - force a known starting mode
        set_global_color_mode(ColorMode::Direct);
        let direct = parse_color_to_ratatui("#ff8040");
        assert!(matches!(direct, Some(ratatui::style::Color::Rgb(0xff, 0x80, 0x40))));

        // Same input parsed again from cache must still be Rgb
        assert_eq!(parse_color_to_ratatui("#ff8040"), direct);

        // Mode switch clears the cache: same input now resolves to Indexed
        set_global_color_mode(ColorMode::Indexed);
        let indexed = parse_color_to_ratatui("#ff8040");
        assert!(matches!(indexed, Some(ratatui::style::Color::Indexed(_))));

        // Restore for other tests on this thread
        set_global_color_mode(ColorMode::Direct);
    }
}
