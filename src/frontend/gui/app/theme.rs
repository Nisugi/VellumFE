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

/// Lazily-loaded system font database, shared by name resolution and the
/// per-window font picker. Scanning system font dirs is done once.
fn system_font_db() -> &'static fontdb::Database {
    static DB: std::sync::OnceLock<fontdb::Database> = std::sync::OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        tracing::info!("Loaded {} system font faces", db.len());
        db
    })
}

/// Sorted, de-duplicated system font family names for the font picker.
pub(super) fn system_font_families() -> &'static [String] {
    static FAMILIES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FAMILIES.get_or_init(|| {
        let db = system_font_db();
        let mut families: Vec<String> = db
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .collect();
        families.sort();
        families.dedup();
        families
    })
}

/// Load raw font data for a font reference. Named fonts resolve through the
/// system font database; custom fonts read from the given file path.
fn font_data_from_ref(
    font: &crate::frontend::gui::persistence::FontRef,
) -> Option<egui::FontData> {
    use crate::frontend::gui::persistence::FontRef;

    match font {
        FontRef::SystemDefault => None,
        FontRef::Named(name) => {
            let db = system_font_db();
            let id = db.query(&fontdb::Query {
                families: &[fontdb::Family::Name(name)],
                ..Default::default()
            })?;
            let (source, index) = db.face_source(id)?;
            let bytes = match source {
                fontdb::Source::Binary(data) | fontdb::Source::SharedFile(_, data) => {
                    data.as_ref().as_ref().to_vec()
                }
                fontdb::Source::File(path) => match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        tracing::warn!(
                            "Failed to read font '{}' from {}: {}",
                            name,
                            path.display(),
                            err
                        );
                        return None;
                    }
                },
            };
            let mut data = egui::FontData::from_owned(bytes);
            data.index = index;
            Some(data)
        }
        FontRef::Custom(path) => match std::fs::read(path) {
            Ok(bytes) => Some(egui::FontData::from_owned(bytes)),
            Err(err) => {
                tracing::warn!("Failed to load font file '{}': {}", path, err);
                None
            }
        },
    }
}

/// Registration key for a font reference inside `FontDefinitions`; None for
/// the system default (nothing to register).
pub(super) fn font_ref_key(font: &crate::frontend::gui::persistence::FontRef) -> Option<String> {
    use crate::frontend::gui::persistence::FontRef;
    match font {
        FontRef::SystemDefault => None,
        FontRef::Named(name) => Some(format!("vellum-named:{}", name)),
        FontRef::Custom(path) => Some(format!("vellum-file:{}", path)),
    }
}

/// Build the full font definitions: egui's built-ins, the app-wide UI font
/// (prepended to the default families), and every per-window font registered
/// as its own named family (falling back to the proportional stack for
/// missing glyphs).
pub(super) fn build_font_definitions(
    ui_font: &crate::frontend::gui::persistence::FontRef,
    window_fonts: &[crate::frontend::gui::persistence::FontRef],
) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();

    if let Some(data) = font_data_from_ref(ui_font) {
        fonts
            .font_data
            .insert("vellum-custom".to_string(), std::sync::Arc::new(data));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.insert(0, "vellum-custom".to_string());
            }
        }
    }

    for font in window_fonts {
        let Some(key) = font_ref_key(font) else {
            continue;
        };
        let family = egui::FontFamily::Name(key.clone().into());
        if fonts.families.contains_key(&family) {
            continue;
        }
        let Some(data) = font_data_from_ref(font) else {
            tracing::warn!("Window font {:?} could not be loaded; using default", font);
            continue;
        };
        fonts
            .font_data
            .insert(key.clone(), std::sync::Arc::new(data));
        let mut list = vec![key];
        if let Some(fallbacks) = fonts.families.get(&egui::FontFamily::Proportional) {
            list.extend(fallbacks.iter().cloned());
        }
        fonts.families.insert(family, list);
    }

    fonts
}

impl VellumGuiApp {
    /// Re-apply visuals when `config.active_theme` changes (startup, .settheme,
    /// layout-driven theme switches).
    pub(super) fn apply_theme_if_changed(&mut self, ctx: &egui::Context) {
        if self.applied_theme_id.as_deref() == Some(self.app_core.config.active_theme.as_str()) {
            return;
        }
        let active = self.app_core.config.active_theme.clone();

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
