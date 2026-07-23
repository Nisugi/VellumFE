//! Native egui editors for configuration (settings, highlights, keybinds,
//! colors). Editors are egui windows that buffer edits and apply them through
//! the shared core config layer, so both frontends stay in sync.

mod colors;
mod custom_windows;
mod doll_calibration;
mod highlights;
mod hotbars;
mod indicators;
mod keybinds;
mod settings;
mod themes;
mod windows;

pub(super) use colors::ColorsEditorState;
pub(super) use custom_windows::CustomWindowsEditorState;
pub(super) use doll_calibration::DollCalibrationState;
pub(super) use highlights::HighlightEditorState;
pub(super) use hotbars::HotbarEditorState;
pub(super) use indicators::IndicatorTemplatesEditorState;
pub(super) use keybinds::KeybindEditorState;
pub(super) use settings::SettingsEditorState;
pub(super) use themes::{ThemeBrowserState, ThemeEditorState};
pub(super) use windows::WindowEditorState;

use super::{theme, VellumGuiApp};
use eframe::egui;

impl VellumGuiApp {
    /// Render whichever editors are open. Called once per frame.
    pub(super) fn render_editors(&mut self, ctx: &eframe::egui::Context) {
        self.render_settings_editor(ctx);
        self.render_highlight_editor(ctx);
        self.render_keybind_editor(ctx);
        self.render_hotbar_editor(ctx);
        self.render_colors_editor(ctx);
        self.render_theme_browser(ctx);
        self.render_theme_editor(ctx);
        self.render_indicator_templates_editor(ctx);
        self.render_window_editor(ctx);
        self.render_custom_windows_editor(ctx);
        self.render_doll_calibration(ctx);
    }
}

/// Hex/name text field with a live swatch and an egui color picker.
fn color_field(ui: &mut egui::Ui, value: &mut String) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(110.0));
        if let Some(color) = theme::resolve_color(value) {
            let mut rgb = [color.r(), color.g(), color.b()];
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                *value = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            }
        } else if !value.trim().is_empty() {
            ui.weak("?");
        }
    });
}
