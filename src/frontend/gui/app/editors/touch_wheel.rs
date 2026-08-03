//! The touch-wheel editor: edit the phone's long-press radial wheel from
//! the desktop. Peer to the phone's own editor — both write the same
//! `[touch_wheel]` config (character scope, roams) and the change is
//! re-broadcast to any connected phone. Slices are the top-level ring; each
//! is a client action (open a panel, focus input) or a game command. Nested
//! folders load and save unchanged but aren't edited here (kept focused).

use crate::config::{Config, WheelSlice, TOUCH_WHEEL_CLIENT_ACTIONS};
use crate::frontend::gui::app::VellumGuiApp;
use eframe::egui;

pub(in super::super) struct TouchWheelEditorState {
    /// Working copy of the top-level ring.
    draft: Vec<WheelSlice>,
    status: Option<String>,
}

/// Deferred row mutation so the render loop never edits what it iterates.
enum RowOp {
    MoveUp(usize),
    MoveDown(usize),
    Remove(usize),
}

/// A slice's editor "kind": a client action, a game command, or a folder
/// (folders are shown read-only here).
fn slice_kind(slice: &WheelSlice) -> &'static str {
    if !slice.slices.is_empty() {
        "folder"
    } else if slice.client.is_some() {
        "client"
    } else {
        "command"
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_touch_wheel_editor(&mut self) {
        if self.touch_wheel_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_touch_wheel_editor"));
            return;
        }
        // Seed from the loaded config; if unset, start from an empty ring
        // (the phone falls back to its built-in default until saved).
        self.touch_wheel_editor = Some(TouchWheelEditorState {
            draft: self.app_core.config.touch_wheel.clone(),
            status: None,
        });
    }

    pub(in super::super) fn render_touch_wheel_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.touch_wheel_editor.take() else {
            return;
        };
        let mut open = true;
        let mut pending_op: Option<RowOp> = None;
        let mut do_add = false;
        let mut do_save = false;

        egui::Window::new("Touch wheel")
            .id(egui::Id::new("gui_touch_wheel_editor"))
            .open(&mut open)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(
                    "The phone's long-press radial wheel. Each slice opens a \
                     panel, focuses the input, or runs a game command. Saved \
                     to this character and applied to any connected phone.",
                );
                ui.separator();

                if state.draft.is_empty() {
                    ui.label("No slices yet — add one below.");
                }

                for (i, slice) in state.draft.iter_mut().enumerate() {
                    let kind = slice_kind(slice);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut slice.label)
                                .hint_text("Label")
                                .desired_width(90.0),
                        );

                        if kind == "folder" {
                            ui.label(egui::RichText::new("folder (nested)").italics().weak());
                        } else {
                            // Kind toggle: client action vs game command.
                            let mut is_client = slice.client.is_some();
                            egui::ComboBox::from_id_salt(("tw_kind", i))
                                .selected_text(if is_client { "Open / focus" } else { "Game command" })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut is_client, true, "Open / focus");
                                    ui.selectable_value(&mut is_client, false, "Game command");
                                });
                            // Apply a kind switch: move the value to the right field.
                            if is_client && slice.client.is_none() {
                                slice.client = Some(TOUCH_WHEEL_CLIENT_ACTIONS[0].0.to_string());
                                slice.command.clear();
                            } else if !is_client && slice.client.is_some() {
                                slice.client = None;
                            }

                            if let Some(action) = slice.client.as_mut() {
                                // Client-action dropdown from the shared catalog.
                                let current_label = TOUCH_WHEEL_CLIENT_ACTIONS
                                    .iter()
                                    .find(|(a, _)| a == action)
                                    .map(|(_, l)| *l)
                                    .unwrap_or(action.as_str());
                                egui::ComboBox::from_id_salt(("tw_action", i))
                                    .selected_text(current_label)
                                    .show_ui(ui, |ui| {
                                        for (a, label) in TOUCH_WHEEL_CLIENT_ACTIONS {
                                            ui.selectable_value(action, a.to_string(), *label);
                                        }
                                    });
                            } else {
                                ui.add(
                                    egui::TextEdit::singleline(&mut slice.command)
                                        .hint_text("e.g. look")
                                        .desired_width(140.0),
                                );
                            }
                        }

                        // Reorder + delete.
                        if ui.add_enabled(i > 0, egui::Button::new("↑")).clicked() {
                            pending_op = Some(RowOp::MoveUp(i));
                        }
                        if ui
                            .add_enabled(i + 1 < 999, egui::Button::new("↓"))
                            .clicked()
                        {
                            pending_op = Some(RowOp::MoveDown(i));
                        }
                        if ui.button("✕").clicked() {
                            pending_op = Some(RowOp::Remove(i));
                        }
                    });
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("+ Add slice").clicked() {
                        do_add = true;
                    }
                    if ui.button("Save").clicked() {
                        do_save = true;
                    }
                    if let Some(status) = &state.status {
                        ui.label(status);
                    }
                });
            });

        // Apply deferred mutations after the iteration.
        match pending_op {
            Some(RowOp::MoveUp(i)) if i > 0 => state.draft.swap(i, i - 1),
            Some(RowOp::MoveDown(i)) if i + 1 < state.draft.len() => state.draft.swap(i, i + 1),
            Some(RowOp::Remove(i)) if i < state.draft.len() => {
                state.draft.remove(i);
            }
            _ => {}
        }
        if do_add {
            state.draft.push(WheelSlice {
                label: String::new(),
                client: Some(TOUCH_WHEEL_CLIENT_ACTIONS[0].0.to_string()),
                ..Default::default()
            });
        }
        if do_save {
            // Drop blank-label rows (an empty slice can't be aimed anyway).
            let slices: Vec<WheelSlice> = state
                .draft
                .iter()
                .filter(|s| !s.label.trim().is_empty())
                .cloned()
                .collect();
            let character = self.app_core.config.character.clone();
            match Config::save_touch_wheel(&slices, character.is_none(), character.as_deref()) {
                Ok(()) => {
                    // Hot-reload + re-broadcast so a connected phone updates live.
                    self.app_core.config.touch_wheel =
                        Config::load_touch_wheel(character.as_deref()).unwrap_or_default();
                    self.app_core.push_remote_wheels();
                    state.status = Some("Saved.".to_string());
                }
                Err(e) => state.status = Some(format!("Save failed: {e}")),
            }
        }

        if open {
            self.touch_wheel_editor = Some(state);
        }
    }
}
