//! Pack editor (.packs): guided export/import of shareable `.vellumpack`
//! files — the panel form of `.uiexport` / `.uiimport`.
//!
//! Export tab: pack name, per-part checkboxes, optional destination
//! folder (default `~/.vellum-fe/exports/`). Import tab: a dropdown of
//! packs dropped into `~/.vellum-fe/imports/` or a pasted path, a
//! preview of what the pack carries, per-part install checkboxes, and an
//! Install button (backups + hot reload ride the same core path as
//! `.uiimport apply`).

use super::super::VellumGuiApp;
use crate::core::uipack;
use eframe::egui;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum PackEditorTab {
    Export,
    Import,
}

pub(in super::super) struct PackEditorState {
    tab: PackEditorTab,
    // Export
    export_name: String,
    export_parts: Vec<(&'static str, bool)>,
    export_dest: String,
    // Import
    import_choices: Vec<String>,
    import_choice: Option<usize>,
    import_path: String,
    /// The pack path the current preview/install list was built from.
    preview_for: Option<PathBuf>,
    preview: Option<uipack::PackPreview>,
    preview_error: Option<String>,
    install_parts: Vec<(String, bool)>,
    // Deferred actions (run after the window closure releases self)
    do_export: bool,
    do_install: bool,
}

impl PackEditorState {
    fn new(import_choices: Vec<String>) -> Self {
        Self {
            tab: PackEditorTab::Export,
            export_name: String::new(),
            export_parts: uipack::PARTS.iter().map(|p| (*p, true)).collect(),
            export_dest: String::new(),
            import_choices,
            import_choice: None,
            import_path: String::new(),
            preview_for: None,
            preview: None,
            preview_error: None,
            install_parts: Vec::new(),
            do_export: false,
            do_install: false,
        }
    }

    /// The pack the Import tab currently points at: a pasted path wins,
    /// otherwise the dropdown choice from imports/.
    fn import_target(&self, base: &std::path::Path) -> Option<PathBuf> {
        let manual = self.import_path.trim();
        if !manual.is_empty() {
            return Some(PathBuf::from(manual));
        }
        self.import_choice
            .and_then(|i| self.import_choices.get(i))
            .map(|name| base.join("imports").join(format!("{name}.vellumpack")))
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_pack_editor(&mut self) {
        if self.pack_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_pack_editor"));
            return;
        }
        let choices = crate::config::Config::base_dir()
            .map(|base| uipack::list_import_packs(&base))
            .unwrap_or_default();
        self.pack_editor = Some(PackEditorState::new(choices));
    }

    pub(in super::super) fn render_pack_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.pack_editor.take() else {
            return;
        };
        let Ok(base) = crate::config::Config::base_dir() else {
            return;
        };
        let mut open = true;

        egui::Window::new("UI Packs")
            .id(egui::Id::new("gui_pack_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.tab, PackEditorTab::Export, "Export");
                    ui.selectable_value(&mut state.tab, PackEditorTab::Import, "Import");
                });
                ui.separator();
                match state.tab {
                    PackEditorTab::Export => Self::pack_export_tab(ui, &mut state),
                    PackEditorTab::Import => Self::pack_import_tab(ui, &mut state, &base),
                }
            });

        // Actions run after the window borrow ends.
        if state.do_export {
            state.do_export = false;
            let name = state.export_name.trim().to_string();
            let parts: Vec<String> = state
                .export_parts
                .iter()
                .filter(|(_, on)| *on)
                .map(|(p, _)| p.to_string())
                .collect();
            let dest = {
                let dest = state.export_dest.trim();
                (!dest.is_empty()).then(|| PathBuf::from(dest))
            };
            if parts.is_empty() {
                self.app_core
                    .add_system_message("Select at least one part to export.");
            } else {
                let extra = self.gui_layout_pack_entry();
                self.app_core.uiexport_pack(&name, &parts, dest, extra);
            }
        }
        if state.do_install {
            state.do_install = false;
            if let Some(path) = state.import_target(&base) {
                let selected: Vec<String> = state
                    .install_parts
                    .iter()
                    .filter(|(_, on)| *on)
                    .map(|(p, _)| p.clone())
                    .collect();
                if selected.is_empty() {
                    self.app_core
                        .add_system_message("Select at least one part to install.");
                } else if let Some((pack_name, bytes)) =
                    self.app_core.uiimport_apply(&path, Some(&selected))
                {
                    self.install_gui_layout_from_pack(&pack_name, &bytes);
                }
            }
        }

        if open {
            self.pack_editor = Some(state);
        }
    }

    fn pack_export_tab(ui: &mut egui::Ui, state: &mut PackEditorState) {
        egui::Grid::new("pack_export_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Pack name");
                ui.add(
                    egui::TextEdit::singleline(&mut state.export_name)
                        .hint_text("my-ui")
                        .desired_width(200.0),
                );
                ui.end_row();
                ui.label("Save to");
                ui.add(
                    egui::TextEdit::singleline(&mut state.export_dest)
                        .hint_text("(default: ~/.vellum-fe/exports)")
                        .desired_width(260.0),
                );
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.label("Include:");
        for (part, on) in &mut state.export_parts {
            ui.checkbox(on, uipack::part_label(part));
        }
        ui.add_space(6.0);
        let name_ok = uipack::is_valid_pack_name(state.export_name.trim());
        if !state.export_name.trim().is_empty() && !name_ok {
            ui.colored_label(
                egui::Color32::from_rgb(235, 90, 90),
                "Names use letters, digits, '-' and '_' only.",
            );
        }
        if ui
            .add_enabled(name_ok, egui::Button::new("Export pack"))
            .clicked()
        {
            state.do_export = true;
        }
        ui.weak("Connection and account settings are never included.");
    }

    fn pack_import_tab(
        ui: &mut egui::Ui,
        state: &mut PackEditorState,
        base: &std::path::Path,
    ) {
        ui.horizontal(|ui| {
            ui.label("From imports/");
            let selected_text = state
                .import_choice
                .and_then(|i| state.import_choices.get(i))
                .cloned()
                .unwrap_or_else(|| "(choose a pack)".to_string());
            egui::ComboBox::from_id_salt("pack_import_choice")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for (i, name) in state.import_choices.iter().enumerate() {
                        if ui
                            .selectable_label(state.import_choice == Some(i), name)
                            .clicked()
                        {
                            state.import_choice = Some(i);
                            state.import_path.clear();
                        }
                    }
                });
            if ui.small_button("⟳").on_hover_text("Rescan imports/").clicked() {
                state.import_choices = uipack::list_import_packs(base);
                state.import_choice = None;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Or file path");
            ui.add(
                egui::TextEdit::singleline(&mut state.import_path)
                    .hint_text("C:\\path\\to\\pack.vellumpack")
                    .desired_width(280.0),
            );
        });
        ui.weak(format!(
            "Drop packs into {} to see them in the list.",
            base.join("imports").display()
        ));
        ui.separator();

        // (Re)build the preview when the target changes.
        let target = state.import_target(base);
        if target != state.preview_for {
            state.preview_for = target.clone();
            state.preview = None;
            state.preview_error = None;
            state.install_parts.clear();
            if let Some(path) = &target {
                match uipack::preview(path) {
                    Ok(preview) => {
                        state.install_parts = preview
                            .manifest
                            .parts
                            .iter()
                            .map(|p| (p.clone(), true))
                            .collect();
                        state.preview = Some(preview);
                    }
                    Err(err) => state.preview_error = Some(format!("{err:#}")),
                }
            }
        }

        if let Some(err) = &state.preview_error {
            ui.colored_label(egui::Color32::from_rgb(235, 90, 90), err);
            return;
        }
        let Some(preview) = &state.preview else {
            ui.weak("Choose a pack to see what it carries.");
            return;
        };
        let mut summary = format!(
            "VellumFE {} · {} file(s)",
            preview.manifest.version,
            preview.entries.len()
        );
        if let Some(skin) = &preview.manifest.skin {
            summary.push_str(&format!(" · skin '{skin}'"));
        }
        if let Some(theme) = &preview.manifest.theme {
            summary.push_str(&format!(" · theme '{theme}'"));
        }
        ui.label(summary);
        ui.add_space(6.0);
        ui.label("Install:");
        for (part, on) in &mut state.install_parts {
            ui.checkbox(on, uipack::part_label(part));
        }
        ui.add_space(6.0);
        if ui.button("Install selected").clicked() {
            state.do_install = true;
        }
        ui.weak("Replaced files are backed up to ~/.vellum-fe/backups/.");
    }

    /// The GUI's live arrangement as a pack entry (same bytes `.uiexport`
    /// attaches).
    pub(in super::super) fn gui_layout_pack_entry(&mut self) -> Vec<(String, Vec<u8>)> {
        self.build_layout_snapshot()
            .and_then(|layout| serde_json::to_vec_pretty(&layout).ok())
            .map(|bytes| vec![(uipack::GUI_LAYOUT_ENTRY.to_string(), bytes)])
            .unwrap_or_default()
    }

    /// Install a pack's GUI layout bytes as a named checkpoint.
    pub(in super::super) fn install_gui_layout_from_pack(
        &mut self,
        pack_name: &str,
        bytes: &[u8],
    ) {
        match serde_json::from_slice(bytes) {
            Ok(layout) => match crate::frontend::gui::persistence::save_named_layout(
                &layout, pack_name,
            ) {
                Ok(()) => self.app_core.add_system_message(&format!(
                    "GUI layout installed — load it with .loadlayout {pack_name}"
                )),
                Err(err) => self.app_core.add_system_message(&format!(
                    "Pack's GUI layout could not be saved: {err}"
                )),
            },
            Err(err) => self
                .app_core
                .add_system_message(&format!("Pack's GUI layout did not parse: {err}")),
        }
    }
}
