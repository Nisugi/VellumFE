//! GUI Theme Integration - Convert AppTheme to egui::Visuals
//!
//! This module provides conversion functions to integrate VellumFE's comprehensive
//! theme system (30+ themes, 90+ colors) with egui's Visuals styling system.

use crate::theme::AppTheme;
use eframe::egui::{self, Color32};

/// Convert AppTheme to egui::Visuals
///
/// Maps VellumFE's 90+ theme colors to egui's ~40 visual properties.
/// Based on TUI's proven theme system with support for light/dark modes,
/// color filters, and extensive widget customization.
pub fn app_theme_to_visuals(theme: &AppTheme) -> egui::Visuals {
    let mut visuals = egui::Visuals::dark(); // Start with dark base

    // Override widget colors
    visuals.widgets.noninteractive.bg_fill = color_to_color32(&theme.background_primary);
    visuals.widgets.noninteractive.weak_bg_fill = color_to_color32(&theme.background_secondary);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(
        1.0,
        color_to_color32(&theme.window_border),
    );
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(
        1.0,
        color_to_color32(&theme.text_primary),
    );

    visuals.widgets.inactive.bg_fill = color_to_color32(&theme.background_primary);
    visuals.widgets.inactive.weak_bg_fill = color_to_color32(&theme.background_secondary);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(
        1.0,
        color_to_color32(&theme.window_border),
    );
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(
        1.0,
        color_to_color32(&theme.text_primary),
    );

    visuals.widgets.hovered.bg_fill = color_to_color32(&theme.background_hover);
    visuals.widgets.hovered.weak_bg_fill = color_to_color32(&theme.background_hover);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(
        1.5,
        color_to_color32(&theme.button_hover),
    );
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(
        1.5,
        color_to_color32(&theme.text_selected),
    );

    visuals.widgets.active.bg_fill = color_to_color32(&theme.button_active);
    visuals.widgets.active.weak_bg_fill = color_to_color32(&theme.button_active);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(
        2.0,
        color_to_color32(&theme.button_active),
    );
    visuals.widgets.active.fg_stroke = egui::Stroke::new(
        2.0,
        color_to_color32(&theme.text_selected),
    );

    visuals.widgets.open.bg_fill = color_to_color32(&theme.button_active);
    visuals.widgets.open.weak_bg_fill = color_to_color32(&theme.button_active);
    visuals.widgets.open.bg_stroke = egui::Stroke::new(
        1.5,
        color_to_color32(&theme.window_border),
    );
    visuals.widgets.open.fg_stroke = egui::Stroke::new(
        1.5,
        color_to_color32(&theme.text_selected),
    );

    // Selection colors
    visuals.selection.bg_fill = color_to_color32(&theme.background_selected);
    visuals.selection.stroke = egui::Stroke::new(
        1.0,
        color_to_color32(&theme.text_selected),
    );

    // Hyperlink colors
    visuals.hyperlink_color = color_to_color32(&theme.link_color);

    // Window styling
    visuals.window_fill = color_to_color32(&theme.window_background);
    visuals.window_stroke = egui::Stroke::new(
        1.0,
        color_to_color32(&theme.window_border),
    );
    visuals.window_shadow = egui::epaint::Shadow::NONE;

    // Panel styling
    visuals.panel_fill = color_to_color32(&theme.menu_background);

    // Popup styling
    visuals.popup_shadow = egui::epaint::Shadow::NONE;

    // Extreme backgrounds (tooltips, error messages)
    visuals.extreme_bg_color = color_to_color32(&theme.background_hover);

    // Code/mono text
    visuals.code_bg_color = color_to_color32(&theme.background_secondary);

    // Warning colors
    visuals.warn_fg_color = color_to_color32(&theme.status_warning);
    visuals.error_fg_color = color_to_color32(&theme.status_error);

    // Window highlight for focused window (disable the default lime green highlight)
    // The title bar of focused windows uses the selection color in egui
    visuals.window_highlight_topmost = false;

    visuals
}

/// Convert frontend::common::Color to egui::Color32
fn color_to_color32(color: &crate::frontend::common::Color) -> Color32 {
    Color32::from_rgb(color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemePresets;

    #[test]
    fn test_app_theme_to_visuals() {
        let theme = ThemePresets::dracula();
        let visuals = app_theme_to_visuals(&theme);

        // Verify basic properties are set
        assert_ne!(visuals.widgets.noninteractive.bg_fill, Color32::TRANSPARENT);
        assert_ne!(visuals.window_fill, Color32::TRANSPARENT);
    }

    #[test]
    fn test_color_conversion() {
        use crate::frontend::common::Color;

        assert_eq!(color_to_color32(&Color::BLACK), Color32::BLACK);
        assert_eq!(color_to_color32(&Color::WHITE), Color32::WHITE);
        assert_eq!(
            color_to_color32(&Color::rgb(100, 150, 200)),
            Color32::from_rgb(100, 150, 200)
        );
    }

    #[test]
    fn test_multiple_themes() {
        // Verify conversion works for different theme presets
        let themes = vec![
            ThemePresets::dracula(),
            ThemePresets::gruvbox_dark(),
            ThemePresets::nord(),
            ThemePresets::tokyo_night(),
        ];

        for theme in themes {
            let visuals = app_theme_to_visuals(&theme);
            // Should not panic and should produce valid visuals
            assert_ne!(visuals.widgets.noninteractive.bg_fill, Color32::TRANSPARENT);
        }
    }
}
