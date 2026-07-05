//! Native egui editors for configuration (settings, highlights, keybinds,
//! colors). Editors are egui windows that buffer edits and apply them through
//! the shared core config layer, so both frontends stay in sync.

mod colors;
mod highlights;
mod keybinds;
mod settings;
mod themes;

pub(super) use colors::ColorsEditorState;
pub(super) use highlights::HighlightEditorState;
pub(super) use keybinds::KeybindEditorState;
pub(super) use settings::SettingsEditorState;
pub(super) use themes::{ThemeBrowserState, ThemeEditorState};

use super::{theme, VellumGuiApp};
use eframe::egui;

impl VellumGuiApp {
    /// Render whichever editors are open. Called once per frame.
    pub(super) fn render_editors(&mut self, ctx: &eframe::egui::Context) {
        self.render_settings_editor(ctx);
        self.render_highlight_editor(ctx);
        self.render_keybind_editor(ctx);
        self.render_colors_editor(ctx);
        self.render_theme_browser(ctx);
        self.render_theme_editor(ctx);
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
