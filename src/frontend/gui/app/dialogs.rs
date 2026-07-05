//! Server dialog rendering (Wrayth `openDialog` -> `DialogState`) as an egui
//! window. Button/radio/close/autosend semantics live on `DialogState` in the
//! data layer and are shared with the TUI.

use super::*;

impl VellumGuiApp {
    pub(super) fn render_server_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.app_core.ui_state.active_dialog.take() else {
            return;
        };

        let mut open = true;
        let mut command_to_send: Option<String> = None;
        let mut close_dialog = false;
        let title = dialog.title.clone().unwrap_or_else(|| "Dialog".to_string());
        let window_id = egui::Id::new(format!("gui_server_dialog_{}", dialog.id));

        egui::Window::new(title)
            .id(window_id)
            .open(&mut open)
            .collapsible(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                for label in &dialog.display_labels {
                    ui.label(&label.value);
                }
                for bar in &dialog.progress_bars {
                    ui.add(
                        egui::ProgressBar::new(bar.value.min(100) as f32 / 100.0)
                            .text(bar.text.clone()),
                    );
                }

                // Input fields, paired positionally with their labels.
                let labels = dialog.labels.clone();
                let mut enter_button: Option<String> = None;
                for (index, field) in dialog.fields.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if let Some(label) = labels.get(index) {
                            ui.label(&label.value);
                        }
                        let response = ui.text_edit_singleline(&mut field.value);
                        if response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            if let Some(button_id) = &field.enter_button {
                                enter_button = Some(button_id.clone());
                            }
                        }
                    });
                }

                if !dialog.buttons.is_empty() {
                    ui.separator();
                }
                let mut clicked_index: Option<usize> = None;
                ui.horizontal_wrapped(|ui| {
                    for (index, button) in dialog.buttons.iter().enumerate() {
                        let clicked = if button.is_radio {
                            ui.radio(button.selected, &button.label).clicked()
                        } else {
                            ui.button(&button.label).clicked()
                        };
                        if clicked {
                            clicked_index = Some(index);
                        }
                    }
                });
                if clicked_index.is_none() {
                    if let Some(button_id) = enter_button {
                        clicked_index = dialog
                            .buttons
                            .iter()
                            .position(|button| button.id == button_id);
                    }
                }

                if let Some(index) = clicked_index {
                    dialog.selected = index;
                    let (cmd, close) = dialog.activate_button(index);
                    command_to_send = cmd;
                    close_dialog = close;
                }
            });

        if let Some(command) = command_to_send {
            self.dispatch_raw_command(command);
        }

        if open && !close_dialog {
            self.app_core.ui_state.active_dialog = Some(dialog);
        } else if self.app_core.ui_state.input_mode == InputMode::Dialog {
            self.app_core.ui_state.input_mode = InputMode::Normal;
        }
    }
}
