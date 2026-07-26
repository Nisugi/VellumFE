//! Pure window color precedence resolution, shared by frontends.
//!
//! Precedence for per-window colors: the window definition, else a
//! colors.toml ui.* value the user actually changed (defaults fall
//! through — see `UiColors::user_*`), else the theme.
//!
//! Color-name resolution ("red" -> "#cd0000") is supplied by the caller as
//! a `parse_color` function: the canonical name table lives in
//! `frontend/common/color.rs` (`parse_color_flexible`), which core must not
//! import (see tests/architecture.rs). Frontends pass it in; tests can pass
//! a minimal parser. Theme colors are plain RGB structs (theme.rs is a root
//! module), formatted here as lowercase `#rrggbb` hex strings.

use crate::config::{UiColors, WindowBase};
use crate::theme::AppTheme;

/// Resolved per-window colors as hex strings (lowercase `#rrggbb`), or
/// `None` where nothing should be painted (e.g. transparent background).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowColors {
    pub border: Option<String>,
    pub background: Option<String>,
    pub text: Option<String>,
}

/// Format an RGB triple as a lowercase hex string (matches the TUI's
/// `color_to_hex_string` output for theme colors).
fn rgb_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Normalize an optional window-def color string: empty and the "-" unset
/// sentinel mean "not set"; names resolve through `parse_color`; anything
/// unrecognized passes through untouched (renderers decide what to do).
pub fn normalize_color(
    opt: &Option<String>,
    parse_color: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    opt.as_ref().and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == "-" {
            None
        } else {
            parse_color(trimmed).or_else(|| Some(trimmed.to_string()))
        }
    })
}

/// Per-window color precedence: the window definition, else a colors.toml
/// ui.* value the user actually changed, else the theme.
pub fn resolve_window_colors(
    base: &WindowBase,
    ui: &UiColors,
    theme: &AppTheme,
    parse_color: impl Fn(&str) -> Option<String>,
) -> WindowColors {
    let ui_layer = |value: Option<&str>| value.and_then(|s| parse_color(s));

    let border = normalize_color(&base.border_color, &parse_color)
        .or_else(|| ui_layer(ui.user_border_color()))
        .or_else(|| {
            Some(rgb_hex(
                theme.window_border.r,
                theme.window_border.g,
                theme.window_border.b,
            ))
        });
    let background = if base.transparent_background {
        None
    } else {
        normalize_color(&base.background_color, &parse_color)
            .or_else(|| ui_layer(ui.user_background_color()))
            .or_else(|| {
                Some(rgb_hex(
                    theme.window_background.r,
                    theme.window_background.g,
                    theme.window_background.b,
                ))
            })
    };
    let text = normalize_color(&base.text_color, &parse_color)
        .or_else(|| ui_layer(ui.user_text_color()))
        .or_else(|| {
            Some(rgb_hex(
                theme.text_primary.r,
                theme.text_primary.g,
                theme.text_primary.b,
            ))
        });

    WindowColors {
        border,
        background,
        text,
    }
}

/// Focused-border precedence: a colors.toml ui.focused_border_color the
/// user actually changed, else the theme's window_border_focused. Window
/// definitions have no per-window focused color, so there is no def layer.
pub fn resolve_focused_border_color(
    ui: &UiColors,
    theme: &AppTheme,
    parse_color: impl Fn(&str) -> Option<String>,
) -> String {
    ui.user_focused_border_color()
        .and_then(|s| parse_color(s))
        .unwrap_or_else(|| {
            rgb_hex(
                theme.window_border_focused.r,
                theme.window_border_focused.g,
                theme.window_border_focused.b,
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal stand-in for parse_color_flexible: hex passes through
    /// (lowercased, '#' added), one name resolves, everything else misses.
    fn test_parse(input: &str) -> Option<String> {
        let trimmed = input.trim();
        if trimmed.is_empty() || trimmed == "-" {
            return None;
        }
        let hex = trimmed.trim_start_matches('#');
        if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(format!("#{}", hex.to_lowercase()));
        }
        (trimmed.eq_ignore_ascii_case("red")).then(|| "#cd0000".to_string())
    }

    /// WindowBase has no Default; build a minimal one through serde so
    /// every field takes its usual layout.toml default.
    fn base_with(border: Option<&str>, background: Option<&str>, text: Option<&str>) -> WindowBase {
        let mut base: WindowBase = toml::from_str("name = \"test\"").expect("minimal WindowBase");
        base.border_color = border.map(String::from);
        base.background_color = background.map(String::from);
        base.text_color = text.map(String::from);
        base
    }

    #[test]
    fn window_def_wins_over_user_and_theme() {
        let base = base_with(Some("#112233"), Some("red"), Some("#445566"));
        let mut ui = UiColors::default();
        ui.border_color = "#999999".to_string();
        let theme = AppTheme::default();

        let colors = resolve_window_colors(&base, &ui, &theme, test_parse);
        assert_eq!(colors.border.as_deref(), Some("#112233"));
        // Names in defs resolve through the parser
        assert_eq!(colors.background.as_deref(), Some("#cd0000"));
        assert_eq!(colors.text.as_deref(), Some("#445566"));
    }

    #[test]
    fn user_ui_wins_over_theme_when_def_unset() {
        let base = base_with(None, None, None);
        let mut ui = UiColors::default();
        ui.border_color = "#abcdef".to_string();
        ui.text_color = "red".to_string();
        let theme = AppTheme::default();

        let colors = resolve_window_colors(&base, &ui, &theme, test_parse);
        assert_eq!(colors.border.as_deref(), Some("#abcdef"));
        assert_eq!(colors.text.as_deref(), Some("#cd0000"));
        // background untouched by user -> theme
        assert_eq!(
            colors.background,
            Some(rgb_hex(
                theme.window_background.r,
                theme.window_background.g,
                theme.window_background.b
            ))
        );
    }

    #[test]
    fn defaults_fall_through_to_theme() {
        let base = base_with(None, None, None);
        let ui = UiColors::default();
        let theme = AppTheme::default();

        let colors = resolve_window_colors(&base, &ui, &theme, test_parse);
        assert_eq!(
            colors.border,
            Some(rgb_hex(
                theme.window_border.r,
                theme.window_border.g,
                theme.window_border.b
            ))
        );
        assert_eq!(
            colors.text,
            Some(rgb_hex(
                theme.text_primary.r,
                theme.text_primary.g,
                theme.text_primary.b
            ))
        );
    }

    #[test]
    fn transparent_background_is_none() {
        let mut base = base_with(None, Some("#112233"), None);
        base.transparent_background = true;
        let colors =
            resolve_window_colors(&base, &UiColors::default(), &AppTheme::default(), test_parse);
        assert_eq!(colors.background, None);
    }

    #[test]
    fn def_sentinels_fall_through() {
        // Empty string and "-" in a def are "not set", not overrides
        let base = base_with(Some("-"), Some(""), None);
        let mut ui = UiColors::default();
        ui.border_color = "#abcdef".to_string();
        let theme = AppTheme::default();
        let colors = resolve_window_colors(&base, &ui, &theme, test_parse);
        assert_eq!(colors.border.as_deref(), Some("#abcdef"));
        assert!(colors.background.is_some()); // theme layer
    }

    #[test]
    fn normalize_color_passes_unknown_through() {
        assert_eq!(
            normalize_color(&Some("mystery".to_string()), test_parse),
            Some("mystery".to_string())
        );
        assert_eq!(normalize_color(&Some("-".to_string()), test_parse), None);
        assert_eq!(normalize_color(&None, test_parse), None);
    }

    #[test]
    fn focused_border_user_wins_over_theme() {
        let mut ui = UiColors::default();
        ui.focused_border_color = "#123abc".to_string();
        let theme = AppTheme::default();
        assert_eq!(
            resolve_focused_border_color(&ui, &theme, test_parse),
            "#123abc"
        );
    }

    #[test]
    fn focused_border_defaults_to_theme() {
        let ui = UiColors::default();
        let theme = AppTheme::default();
        let expected = rgb_hex(
            theme.window_border_focused.r,
            theme.window_border_focused.g,
            theme.window_border_focused.b,
        );
        assert_eq!(resolve_focused_border_color(&ui, &theme, test_parse), expected);
    }
}
