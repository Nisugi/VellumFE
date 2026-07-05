//! Theme integration for the GUI.
//!
//! Maps the shared `AppTheme` (themes/ presets + custom themes, selected by
//! `config.active_theme`) onto `egui::Visuals`, and resolves config color
//! strings (hex or names) to egui colors.

use super::*;
use crate::frontend::common::color::parse_color_flexible;
use crate::theme::AppTheme;

pub(super) fn color32(color: crate::frontend::common::Color) -> Color32 {
    Color32::from_rgb(color.r, color.g, color.b)
}

/// Resolve a config color string ("#ff8800", "ff8800", or a name like "red").
pub(super) fn resolve_color(input: &str) -> Option<Color32> {
    parse_color_flexible(input).and_then(|hex| super::widgets::parse_hex_color(&hex))
}

/// Build egui visuals from the shared application theme.
pub(super) fn visuals_from_theme(theme: &AppTheme) -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = color32(theme.background_primary);
    visuals.window_fill = color32(theme.window_background);
    visuals.extreme_bg_color = color32(theme.background_secondary);
    visuals.faint_bg_color = color32(theme.background_secondary);
    visuals.override_text_color = Some(color32(theme.text_primary));
    visuals.hyperlink_color = color32(theme.link_color);
    visuals.selection.bg_fill = color32(theme.selection_background);
    visuals.selection.stroke.color = color32(theme.text_selected);
    visuals.window_stroke.color = color32(theme.window_border);
    visuals.warn_fg_color = color32(theme.status_warning);
    visuals.error_fg_color = color32(theme.status_error);

    visuals.widgets.noninteractive.bg_stroke.color = color32(theme.window_border);
    visuals.widgets.noninteractive.fg_stroke.color = color32(theme.text_primary);
    visuals.widgets.inactive.bg_fill = color32(theme.button_normal);
    visuals.widgets.inactive.weak_bg_fill = color32(theme.button_normal);
    visuals.widgets.inactive.fg_stroke.color = color32(theme.text_primary);
    visuals.widgets.hovered.bg_fill = color32(theme.button_hover);
    visuals.widgets.hovered.weak_bg_fill = color32(theme.button_hover);
    visuals.widgets.active.bg_fill = color32(theme.button_active);
    visuals.widgets.active.weak_bg_fill = color32(theme.button_active);
    visuals.widgets.open.bg_fill = color32(theme.menu_background);
    visuals.widgets.open.weak_bg_fill = color32(theme.menu_background);

    visuals
}

/// Build font definitions for a configured UI font. Returns None for the
/// system default (or when the font can't be loaded), leaving egui's
/// built-in fonts in place.
pub(super) fn font_definitions_from_ref(
    font: &crate::frontend::gui::persistence::FontRef,
) -> Option<egui::FontDefinitions> {
    use crate::frontend::gui::persistence::FontRef;

    let path = match font {
        FontRef::SystemDefault => return None,
        FontRef::Named(name) => {
            tracing::warn!(
                "Named font '{}' is not supported yet; set ui_font to a custom file path instead",
                name
            );
            return None;
        }
        FontRef::Custom(path) => path,
    };

    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!("Failed to load UI font '{}': {}", path, err);
            return None;
        }
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "vellum-custom".to_string(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            list.insert(0, "vellum-custom".to_string());
        }
    }
    Some(fonts)
}

impl VellumGuiApp {
    /// Re-apply visuals when `config.active_theme` changes (startup, .settheme,
    /// layout-driven theme switches).
    pub(super) fn apply_theme_if_changed(&mut self, ctx: &egui::Context) {
        let active = self.app_core.config.active_theme.clone();
        if self.applied_theme_id.as_deref() == Some(active.as_str()) {
            return;
        }

        let presets = crate::theme::ThemePresets::all_with_custom(
            self.app_core.config.character.as_deref(),
        );
        if let Some(theme) = presets.get(&active) {
            ctx.set_visuals(visuals_from_theme(theme));
            self.current_theme = theme.clone();
        } else {
            tracing::warn!("Unknown theme '{}', keeping current visuals", active);
        }
        self.applied_theme_id = Some(active);
    }

    /// Handle `action:settheme:<name>` from dot-commands or menus.
    pub(super) fn apply_theme_by_name(&mut self, name: &str) {
        let presets = crate::theme::ThemePresets::all_with_custom(
            self.app_core.config.character.as_deref(),
        );
        if !presets.contains_key(name) {
            let mut names: Vec<&String> = presets.keys().collect();
            names.sort();
            let list = names
                .iter()
                .map(|n| n.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            self.app_core
                .add_system_message(&format!("Unknown theme '{}'. Available: {}", name, list));
            return;
        }

        self.app_core.config.active_theme = name.to_string();
        if let Err(err) = self
            .app_core
            .config
            .save(self.app_core.config.character.as_deref())
        {
            tracing::warn!("Failed to save config after theme switch: {}", err);
        }
        // Force re-apply on the next frame.
        self.applied_theme_id = None;
        self.app_core
            .add_system_message(&format!("Theme switched to: {}", name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_color_handles_hex_and_names() {
        assert_eq!(
            resolve_color("#ff8800"),
            Some(Color32::from_rgb(0xff, 0x88, 0x00))
        );
        assert_eq!(resolve_color("red"), Some(Color32::from_rgb(205, 0, 0)));
        assert_eq!(resolve_color("notacolor"), None);
        assert_eq!(resolve_color("-"), None);
    }

    #[test]
    fn visuals_reflect_theme_colors() {
        let theme = AppTheme::default();
        let visuals = visuals_from_theme(&theme);
        assert_eq!(visuals.window_fill, color32(theme.window_background));
        assert_eq!(visuals.hyperlink_color, color32(theme.link_color));
        assert_eq!(
            visuals.override_text_color,
            Some(color32(theme.text_primary))
        );
    }
}
