//! Native egui editors for configuration (settings, highlights, keybinds,
//! colors). Editors are egui windows that buffer edits and apply them through
//! the shared core config layer, so both frontends stay in sync.

mod settings;

pub(super) use settings::SettingsEditorState;

use super::VellumGuiApp;

impl VellumGuiApp {
    /// Render whichever editors are open. Called once per frame.
    pub(super) fn render_editors(&mut self, ctx: &eframe::egui::Context) {
        self.render_settings_editor(ctx);
    }
}
