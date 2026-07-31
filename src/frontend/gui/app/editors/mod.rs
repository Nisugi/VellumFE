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
mod sorter;
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
pub(super) use sorter::SorterEditorState;
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
        self.render_sorter_editor(ctx);
        self.render_doll_calibration(ctx);
    }
}

/// A pick made in the shared [`icon_ref_picker`].
pub(super) enum IconRefPick {
    /// The caller's "unset/inherit" row (maps to `Option::None` refs).
    Unset,
    /// A concrete reference (pool image, sheet cell, `Default`, `None`).
    Ref(crate::data::IconRef),
}

/// Display label for an `IconRef` choice against a pool listing.
pub(super) fn icon_ref_label(
    current: Option<&crate::data::IconRef>,
    pool_images: &[(String, String)],
    unset_label: &str,
) -> String {
    match current {
        None => unset_label.to_string(),
        Some(crate::data::IconRef::Default) => "Default".to_string(),
        Some(crate::data::IconRef::None) => "None (no art)".to_string(),
        Some(crate::data::IconRef::Image { path }) => pool_images
            .iter()
            .find(|(pool_path, _)| pool_path == path)
            .map(|(_, stem)| stem.clone())
            // Stale reference (image removed): show the raw path.
            .unwrap_or_else(|| path.clone()),
        Some(crate::data::IconRef::SheetCell { sheet, cell }) => format!("{sheet} #{cell}"),
    }
}

/// Shared IconRef source picker: one combo offering the caller's semantic
/// rows (unset/default/none, each present only when labeled), standalone
/// pool images, and the active skin's sheets (picked at cell 1 — callers
/// show a cell spinner/grid for refinement). Returns the pick on the frame
/// a row is clicked; the caller applies it to its own storage.
#[allow(clippy::too_many_arguments)]
pub(super) fn icon_ref_picker(
    ui: &mut egui::Ui,
    id_salt: String,
    current: Option<&crate::data::IconRef>,
    pool_images: &[(String, String)],
    sheet_names: &[String],
    unset_label: Option<&str>,
    default_label: Option<&str>,
    none_label: Option<&str>,
) -> Option<IconRefPick> {
    let mut picked = None;
    let selected = icon_ref_label(current, pool_images, unset_label.unwrap_or("(unset)"));
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            if let Some(label) = unset_label {
                if ui.selectable_label(current.is_none(), label).clicked() {
                    picked = Some(IconRefPick::Unset);
                }
            }
            if let Some(label) = default_label {
                let is = matches!(current, Some(crate::data::IconRef::Default));
                if ui.selectable_label(is, label).clicked() {
                    picked = Some(IconRefPick::Ref(crate::data::IconRef::Default));
                }
            }
            if let Some(label) = none_label {
                let is = matches!(current, Some(crate::data::IconRef::None));
                if ui.selectable_label(is, label).clicked() {
                    picked = Some(IconRefPick::Ref(crate::data::IconRef::None));
                }
            }
            for (path, stem) in pool_images {
                let is = matches!(
                    current,
                    Some(crate::data::IconRef::Image { path: p }) if p == path
                );
                if ui.selectable_label(is, stem).clicked() {
                    picked = Some(IconRefPick::Ref(crate::data::IconRef::Image {
                        path: path.clone(),
                    }));
                }
            }
            for sheet in sheet_names {
                let is = matches!(
                    current,
                    Some(crate::data::IconRef::SheetCell { sheet: s, .. }) if s == sheet
                );
                if ui
                    .selectable_label(is, format!("sheet: {sheet}"))
                    .clicked()
                {
                    picked = Some(IconRefPick::Ref(crate::data::IconRef::SheetCell {
                        sheet: sheet.clone(),
                        cell: 1,
                    }));
                }
            }
            if pool_images.is_empty() && sheet_names.is_empty() {
                ui.weak("no pool images or sheets available");
            }
        });
    picked
}

/// Pool images of one category as (pool-relative path, display stem) rows
/// for [`icon_ref_picker`].
pub(super) fn pool_picker_rows(category: &str) -> Vec<(String, String)> {
    crate::config::pool::list_category(category)
        .into_iter()
        .map(|image| (image.pool_path.clone(), image.stem().to_string()))
        .collect()
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
