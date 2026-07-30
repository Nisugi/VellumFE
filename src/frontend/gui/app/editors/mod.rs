//! Native egui editors for configuration (settings, highlights, keybinds,
//! colors). Editors are egui windows that buffer edits and apply them through
//! the shared core config layer, so both frontends stay in sync.

mod colors;
#[cfg(feature = "gamepad")]
mod controller;
mod custom_windows;
mod doll_calibration;
mod hand_icons;
mod highlights;
mod hotbars;
mod indicators;
mod keybinds;
mod known_windows;
mod menu_keybinds;
mod settings;
mod themes;
mod windows;

pub(super) use colors::ColorsEditorState;
#[cfg(feature = "gamepad")]
pub(super) use controller::ControllerEditorState;
pub(super) use custom_windows::CustomWindowsEditorState;
pub(super) use doll_calibration::DollCalibrationState;
pub(super) use hand_icons::HandIconsEditorState;
pub(super) use highlights::HighlightEditorState;
pub(super) use hotbars::HotbarEditorState;
pub(super) use indicators::IndicatorTemplatesEditorState;
pub(super) use keybinds::KeybindEditorState;
pub(super) use known_windows::KnownWindowsEditorState;
pub(super) use menu_keybinds::MenuKeybindEditorState;
pub(super) use settings::SettingsEditorState;
pub(super) use themes::{ThemeBrowserState, ThemeEditorState};
pub(super) use windows::WindowEditorState;

use super::{theme, VellumGuiApp};
use eframe::egui;

impl VellumGuiApp {
    /// Request that the editor window with `window_id` be raised to the top
    /// on the next frame. Called by the `open_*` methods when their editor
    /// is already open, so re-issuing a settings command (`.controller`,
    /// `.settings`, …) surfaces the buried window instead of rebuilding —
    /// and wiping — its unsaved state. `window_id` is the same `Id` passed
    /// to the editor window's `.id(...)`.
    pub(in super::super) fn raise_editor(&mut self, window_id: egui::Id) {
        self.pending_editor_raise = Some(window_id);
    }

    /// Render whichever editors are open. Called once per frame.
    pub(super) fn render_editors(&mut self, ctx: &eframe::egui::Context) {
        // Raise a re-requested editor before rendering it, so it draws on
        // top this frame. Editor windows render in the foreground order
        // (above game windows, which stay in Middle), keyed by their
        // `.id(...)`.
        if let Some(window_id) = self.pending_editor_raise.take() {
            ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, window_id));
            ctx.request_repaint();
        }
        self.render_settings_editor(ctx);
        self.render_highlight_editor(ctx);
        self.render_keybind_editor(ctx);
        self.render_menu_keybind_editor(ctx);
        #[cfg(feature = "gamepad")]
        self.render_controller_editor(ctx);
        self.render_hotbar_editor(ctx);
        self.render_colors_editor(ctx);
        self.render_theme_browser(ctx);
        self.render_theme_editor(ctx);
        self.render_indicator_templates_editor(ctx);
        self.render_window_editor(ctx);
        self.render_hand_icons_editor(ctx);
        self.render_custom_windows_editor(ctx);
        self.render_known_windows_editor(ctx);
        self.render_doll_calibration(ctx);
    }
}

/// Click-to-pick color swatch plus a hex/name text field. The swatch is
/// always shown — even when the field is empty — so picking a color never
/// requires typing a code; the picker writes hex back into the field.
fn color_field(ui: &mut egui::Ui, value: &mut String) {
    ui.horizontal(|ui| {
        theme::color_picker_swatch(ui, value);
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(110.0)
                .hint_text("color"),
        )
        .on_hover_text(
            "Optional: type a hex value like #b0503a or a palette name \
             instead of using the picker.",
        );
        if theme::resolve_color(value).is_none() && !value.trim().is_empty() {
            ui.weak("?").on_hover_text("Unknown color name.");
        }
    });
}
