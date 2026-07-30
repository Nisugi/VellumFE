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

/// Click-to-pick swatch for a config color string: shows the current color
/// (name or hex) and opens egui's color picker on click, writing `#rrggbb`
/// back into `value`. Pair with a text field for those who want to type a
/// name; nobody is required to know color codes. Returns true when the
/// picker changed the value.
pub(super) fn color_picker_swatch(ui: &mut egui::Ui, value: &mut String) -> bool {
    let current = resolve_color(value).unwrap_or(Color32::GRAY);
    let mut rgb = [current.r(), current.g(), current.b()];
    let response = ui
        .color_edit_button_srgb(&mut rgb)
        .on_hover_text("Pick a color");
    if response.changed() {
        *value = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        true
    } else {
        false
    }
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
    // Button fills deliberately keep egui's neutral dark defaults: the
    // theme's button_* colors are TUI accent colors, and using them as
    // fills turns every button into a bright solid chip.
    visuals.widgets.inactive.fg_stroke.color = color32(theme.text_primary);
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
            if let Err(err) = validate_font_bytes(&bytes, index) {
                tracing::warn!("Font '{}' is not usable ({}); keeping default", name, err);
                return None;
            }
            let mut data = egui::FontData::from_owned(bytes);
            data.index = index;
            Some(data)
        }
        FontRef::Custom(path) => match std::fs::read(path) {
            Ok(bytes) => {
                if let Err(err) = validate_font_bytes(&bytes, 0) {
                    tracing::warn!("Font file '{}' is not usable ({}); keeping default", path, err);
                    return None;
                }
                Some(egui::FontData::from_owned(bytes))
            }
            Err(err) => {
                tracing::warn!("Failed to load font file '{}': {}", path, err);
                None
            }
        },
    }
}

/// Reject font data egui cannot render. epaint parses fonts with skrifa and
/// panics the frame on failure, so anything skrifa refuses here must never
/// reach `FontDefinitions`. fontdb enumerates with ttf-parser, which accepts
/// faces skrifa does not — the picker can list fonts that would crash us.
fn validate_font_bytes(bytes: &[u8], index: u32) -> Result<(), skrifa::raw::ReadError> {
    skrifa::FontRef::from_index(bytes, index).map(|_| ())
}

/// Bundled fallback fonts (name, bytes) appended to the end of every font
/// family. NotoSansSymbols2 covers arrows/geometric shapes/misc symbols;
/// NotoEmoji (monochrome) covers emoji. Both are OFL-licensed — see
/// assets/fonts/OFL.txt. Order matters: symbols before emoji.
const BUILTIN_FALLBACK_FONTS: &[(&str, &[u8])] = &[
    (
        "vellum-fallback-symbols",
        include_bytes!("../../../../assets/fonts/NotoSansSymbols2-Regular.ttf"),
    ),
    (
        "vellum-fallback-emoji",
        include_bytes!("../../../../assets/fonts/NotoEmoji-VariableFont_wght.ttf"),
    ),
];

/// Append the bundled symbol/emoji fallback fonts to the END of every font
/// family (default and custom alike), registering their data once. Call this
/// after any code that builds or adds font families so no font selection can
/// produce tofu for arrows/symbols/emoji. Append-only and idempotent: it
/// never replaces or reorders the fonts already in a family.
pub(super) fn append_builtin_fallbacks(fonts: &mut egui::FontDefinitions) {
    for (name, bytes) in BUILTIN_FALLBACK_FONTS {
        fonts
            .font_data
            .entry((*name).to_string())
            .or_insert_with(|| std::sync::Arc::new(egui::FontData::from_static(bytes)));
    }
    for list in fonts.families.values_mut() {
        for (name, _) in BUILTIN_FALLBACK_FONTS {
            list.retain(|font| font != name);
            list.push((*name).to_string());
        }
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

    // Last step so the bundled symbol/emoji fallbacks land at the end of
    // every family built above (defaults, ui font, and per-window families).
    append_builtin_fallbacks(&mut fonts);

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
            // set_visuals rebuilds Visuals wholesale; force the ui_settings
            // window radius to re-apply over it next frame.
            self.applied_window_corner_radius = None;
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
    fn bundled_fallback_fonts_parse_with_skrifa() {
        // epaint panics the frame on unparseable font data; make sure the
        // bundled fallbacks pass the same skrifa validation gate.
        for (name, bytes) in BUILTIN_FALLBACK_FONTS {
            assert!(
                validate_font_bytes(bytes, 0).is_ok(),
                "bundled fallback font '{}' failed skrifa validation",
                name
            );
        }
    }

    #[test]
    fn builtin_fallbacks_appended_to_end_of_every_family() {
        let mut fonts = egui::FontDefinitions::default();
        // A custom family like the per-window ones built from font refs.
        fonts.families.insert(
            egui::FontFamily::Name("vellum-named:Test Font".into()),
            vec!["some-user-font".to_string()],
        );

        append_builtin_fallbacks(&mut fonts);
        // Idempotent: a second run must not duplicate names or data.
        append_builtin_fallbacks(&mut fonts);

        // Font data registered exactly once per fallback (font_data is a map,
        // so presence means exactly one entry under that name).
        for (name, _) in BUILTIN_FALLBACK_FONTS {
            assert!(
                fonts.font_data.contains_key(*name),
                "fallback '{}' missing from font_data",
                name
            );
        }

        for (family, list) in &fonts.families {
            assert!(
                list.len() >= 2,
                "family {:?} has too few fonts: {:?}",
                family,
                list
            );
            // Both fallbacks present exactly once, as the last two entries,
            // symbols before emoji.
            for (name, _) in BUILTIN_FALLBACK_FONTS {
                assert_eq!(
                    list.iter().filter(|font| font == name).count(),
                    1,
                    "family {:?} should list '{}' exactly once: {:?}",
                    family,
                    name,
                    list
                );
            }
            assert_eq!(
                &list[list.len() - 2..],
                &[
                    "vellum-fallback-symbols".to_string(),
                    "vellum-fallback-emoji".to_string()
                ],
                "family {:?} must end with the bundled fallbacks: {:?}",
                family,
                list
            );
        }

        // The custom family's own font is still first — append-only.
        let custom = fonts
            .families
            .get(&egui::FontFamily::Name("vellum-named:Test Font".into()))
            .unwrap();
        assert_eq!(custom[0], "some-user-font");
    }

    #[test]
    fn build_font_definitions_ends_every_family_with_fallbacks() {
        use crate::frontend::gui::persistence::FontRef;
        // SystemDefault avoids touching the system font database in tests.
        let fonts = build_font_definitions(&FontRef::SystemDefault, &[]);
        for (family, list) in &fonts.families {
            assert_eq!(
                list.last().map(String::as_str),
                Some("vellum-fallback-emoji"),
                "family {:?} does not end with the emoji fallback: {:?}",
                family,
                list
            );
            assert_eq!(
                list.get(list.len() - 2).map(String::as_str),
                Some("vellum-fallback-symbols"),
                "family {:?} missing the symbols fallback before emoji: {:?}",
                family,
                list
            );
        }
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
        // Button fills must stay at egui's neutral defaults, not theme accents.
        let defaults = egui::Visuals::dark();
        assert_eq!(
            visuals.widgets.inactive.bg_fill,
            defaults.widgets.inactive.bg_fill
        );
        assert_eq!(
            visuals.widgets.inactive.weak_bg_fill,
            defaults.widgets.inactive.weak_bg_fill
        );
        assert_eq!(
            visuals.widgets.hovered.bg_fill,
            defaults.widgets.hovered.bg_fill
        );
        assert_eq!(
            visuals.widgets.active.bg_fill,
            defaults.widgets.active.bg_fill
        );
    }
}
