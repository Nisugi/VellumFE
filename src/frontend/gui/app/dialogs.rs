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
        // Anchor-grid resolution (combat window etc.); None = flow layout.
        let positioned = dialog.positioned_controls();

        egui::Window::new(title)
            .id(window_id)
            .open(&mut open)
            .collapsible(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                // In anchor-grid mode the labels are drawn positioned inside
                // the canvas below; only the flow fallback lists them here.
                if positioned.is_none() {
                    for label in &dialog.display_labels {
                        ui.label(&label.value);
                    }
                    for bar in &dialog.progress_bars {
                        let mut progress =
                            egui::ProgressBar::new(bar.value.min(100) as f32 / 100.0)
                                .text(bar.text.clone())
                                .corner_radius(self.ui_settings.bar_corner_radius.clamp(0.0, 12.0));
                        if bar.value == 0 {
                            // Suppress egui's minimum-width fill sliver on empty bars.
                            progress = progress.fill(ui.visuals().extreme_bg_color);
                        }
                        ui.add(progress);
                    }
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

                let mut clicked_index: Option<usize> = None;
                let mut dropdown_change: Option<(usize, String)> = None;

                if let Some((controls, (content_w, content_h))) = positioned {
                    // Anchor-grid mode (combat window etc.): controls at
                    // the game's pixel positions inside a fixed canvas.
                    let (canvas_rect, _) = ui.allocate_exact_size(
                        egui::vec2(content_w, content_h),
                        egui::Sense::hover(),
                    );
                    let origin = canvas_rect.min;
                    for control in &controls {
                        let (x, y, w, h) = control.rect;
                        let rect = egui::Rect::from_min_size(
                            origin + egui::vec2(x, y),
                            egui::vec2(w, h),
                        );
                        match control.kind {
                            crate::data::ui_state::PositionedControlKind::Button(index) => {
                                let Some(button) = dialog.buttons.get(index) else {
                                    continue;
                                };
                                let clicked = if button.is_radio {
                                    ui.put(rect, egui::RadioButton::new(button.selected, &button.label))
                                        .clicked()
                                } else {
                                    ui.put(rect, egui::Button::new(&button.label).small())
                                        .clicked()
                                };
                                if clicked {
                                    clicked_index = Some(index);
                                }
                            }
                            crate::data::ui_state::PositionedControlKind::DropDown(index) => {
                                let Some(dropdown) = dialog.dropdowns.get(index) else {
                                    continue;
                                };
                                if let Some(value) = Self::dropdown_combo(
                                    ui,
                                    rect,
                                    &dialog.id,
                                    dropdown,
                                ) {
                                    dropdown_change = Some((index, value));
                                }
                            }
                            crate::data::ui_state::PositionedControlKind::ProgressBar(index) => {
                                let Some(bar) = dialog.progress_bars.get(index) else {
                                    continue;
                                };
                                ui.put(
                                    rect,
                                    egui::ProgressBar::new(bar.value.min(100) as f32 / 100.0)
                                        .text(bar.text.clone()),
                                );
                            }
                            crate::data::ui_state::PositionedControlKind::Label(index) => {
                                let Some(label) = dialog.display_labels.get(index) else {
                                    continue;
                                };
                                // Wrayth right-justifies value columns (justify='6');
                                // we approximate with a left-aligned label placed at
                                // the resolved rect — good enough for the grid, and
                                // wrap is off so rows stay single-line.
                                ui.put(
                                    rect,
                                    egui::Label::new(&label.value)
                                        .truncate(),
                                );
                            }
                            crate::data::ui_state::PositionedControlKind::Skin(_) => {
                                // Backdrop art needs the skin store, which this
                                // popup path doesn't thread; skins are a resident
                                // dialog-panel feature (rendered there). The
                                // numeric bars/text below still show.
                            }
                            crate::data::ui_state::PositionedControlKind::Image(_) => {}
                        }
                    }
                } else {
                    // Flow mode: dropdowns as labeled rows, then buttons.
                    for (index, dropdown) in dialog.dropdowns.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if let Some(tooltip) = &dropdown.tooltip {
                                ui.label(tooltip);
                            }
                            let rect = ui.available_rect_before_wrap();
                            let rect = egui::Rect::from_min_size(
                                rect.min,
                                egui::vec2(160.0_f32.min(rect.width()), 20.0),
                            );
                            if let Some(value) =
                                Self::dropdown_combo(ui, rect, &dialog.id, dropdown)
                            {
                                dropdown_change = Some((index, value));
                            }
                        });
                    }
                    if !dialog.buttons.is_empty() {
                        ui.separator();
                    }
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
                }

                if let Some((index, value)) = dropdown_change {
                    if let Some(dropdown) = dialog.dropdowns.get_mut(index) {
                        dropdown.value = value;
                    }
                    // A selection with a command fires it immediately
                    // (Wrayth semantics: cmd='aim %dDBAim%' sends on pick),
                    // resolved against ALL current control values.
                    let command = dialog
                        .dropdowns
                        .get(index)
                        .map(|d| d.command.clone())
                        .unwrap_or_default();
                    if !command.trim().is_empty() {
                        let resolved = dialog.command_with_placeholders(&command);
                        command_to_send = Some(format!("{}\n", resolved));
                    }
                }
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

    /// Render one `<dropDownBox>` as a ComboBox inside `rect`. Returns the
    /// newly selected VALUE when the user picks a different option.
    fn dropdown_combo(
        ui: &mut egui::Ui,
        rect: egui::Rect,
        dialog_id: &str,
        dropdown: &crate::data::DialogDropDown,
    ) -> Option<String> {
        let selected_text = dropdown
            .options
            .iter()
            .find(|(_, value)| *value == dropdown.value)
            .map(|(text, _)| text.clone())
            .unwrap_or_else(|| dropdown.value.clone());
        let mut picked: Option<String> = None;
        let response = ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            egui::ComboBox::from_id_salt(("dialog_dropdown", dialog_id, &dropdown.id))
                .width(rect.width())
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for (text, value) in &dropdown.options {
                        if ui
                            .selectable_label(*value == dropdown.value, text)
                            .clicked()
                            && *value != dropdown.value
                        {
                            picked = Some(value.clone());
                        }
                    }
                });
        });
        if let Some(tooltip) = &dropdown.tooltip {
            response.response.on_hover_text(tooltip);
        }
        picked
    }
}
