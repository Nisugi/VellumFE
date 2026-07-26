//! Controller bindings editor (`.controller`): browse/add/edit/delete the
//! `[controller]` table of the global keybinds.toml. Buttons come from a
//! dropdown or by pressing the button on a connected pad ("Capture").
//! Bindings are global — controllers belong to the desk, not a character.

use super::super::VellumGuiApp;
use crate::config::{Config, KeyAction, KeyBindAction, MacroAction};
use eframe::egui;

pub(in super::super) struct ControllerEditorState {
    form: Option<ControllerFormState>,
}

impl ControllerEditorState {
    fn new() -> Self {
        Self { form: None }
    }
}

struct ControllerFormState {
    /// Some(button) when editing an existing binding; None when adding.
    original_button: Option<String>,
    button: String,
    capture_armed: bool,
    is_macro: bool,
    action: String,
    macro_text: String,
    error: Option<String>,
}

impl ControllerFormState {
    fn empty() -> Self {
        Self {
            original_button: None,
            button: String::new(),
            capture_armed: false,
            is_macro: true,
            action: String::new(),
            macro_text: String::new(),
            error: None,
        }
    }

    fn from_binding(button: &str, action: &KeyBindAction) -> Self {
        let (is_macro, action_text, macro_text) = match action {
            KeyBindAction::Action(name) => (false, name.clone(), String::new()),
            KeyBindAction::Macro(macro_action) => {
                (true, String::new(), macro_action.macro_text.clone())
            }
        };
        Self {
            original_button: Some(button.to_string()),
            button: button.to_string(),
            capture_armed: false,
            is_macro,
            action: action_text,
            macro_text,
            error: None,
        }
    }

    fn build_binding(&self) -> Result<(String, KeyBindAction), String> {
        let button = self.button.trim().to_lowercase();
        if button.is_empty() {
            return Err("Pick a button (or press Capture and tap one).".to_string());
        }
        if !super::super::gamepad::GAMEPAD_BUTTON_NAMES.contains(&button.as_str()) {
            return Err(format!("Unknown button '{}'.", button));
        }
        let action = if self.is_macro {
            if self.macro_text.is_empty() {
                return Err("Macro text is required (\\r sends enter).".to_string());
            }
            let text = self.macro_text.replace("\\r", "\r").replace("\\n", "\n");
            KeyBindAction::Macro(MacroAction { macro_text: text })
        } else {
            let name = self.action.trim().to_string();
            if name.is_empty() {
                return Err("Pick an action from the list.".to_string());
            }
            if KeyAction::from_str(&name).is_none() {
                return Err(format!("Unknown action '{}'.", name));
            }
            KeyBindAction::Action(name)
        };
        Ok((button, action))
    }
}

fn display_action(action: &KeyBindAction) -> String {
    match action {
        KeyBindAction::Action(name) => name.clone(),
        KeyBindAction::Macro(macro_action) => format!(
            "macro: {}",
            macro_action.macro_text.replace('\r', "\\r").replace('\n', "\\n")
        ),
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_controller_editor(&mut self) {
        self.controller_editor = Some(ControllerEditorState::new());
    }

    /// Route a pressed controller button into the form's capture field.
    /// Returns true when the press was consumed (capture was armed).
    pub(in super::super) fn controller_editor_capture(&mut self, name: &str) -> bool {
        if let Some(form) = self
            .controller_editor
            .as_mut()
            .and_then(|state| state.form.as_mut())
        {
            if form.capture_armed {
                form.button = name.to_string();
                form.capture_armed = false;
                return true;
            }
        }
        false
    }

    fn save_controller_bind_from_form(&mut self, form: &ControllerFormState) -> Result<(), String> {
        let (button, action) = form.build_binding()?;

        if let Some(original) = &form.original_button {
            if *original != button {
                if let Err(err) = Config::delete_single_controller_bind(original) {
                    tracing::warn!("Failed to remove old controller bind '{}': {}", original, err);
                }
            }
        }

        Config::save_single_controller_bind(&button, &action)
            .map_err(|err| format!("Failed to save controller bind: {}", err))?;
        self.app_core.config.controller_binds =
            Config::load_controller_binds().unwrap_or_default();
        Ok(())
    }

    pub(in super::super) fn render_controller_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.controller_editor.take() else {
            return;
        };

        let mut open = true;
        let mut open_form: Option<ControllerFormState> = None;
        let mut delete_request: Option<String> = None;

        let pad_connected = self
            .gamepad
            .as_ref()
            .is_some_and(|g| g.gamepads().next().is_some());

        egui::Window::new("Controller")
            .id(egui::Id::new("gui_controller_editor"))
            .open(&mut open)
            .default_width(440.0)
            .default_height(380.0)
            .show(ctx, |ui| {
                if pad_connected {
                    ui.weak("Controller connected. D-pad / South / East are fixed navigation inside interact mode and menus; bindings apply outside them.");
                } else {
                    ui.weak("No controller detected — connect one and it will announce itself.");
                }
                ui.horizontal(|ui| {
                    if ui.button("Add binding").clicked() {
                        open_form = Some(ControllerFormState::empty());
                    }
                });
                ui.separator();

                let mut entries: Vec<(&String, &KeyBindAction)> =
                    self.app_core.config.controller_binds.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                let row_count = entries.len();

                egui::ScrollArea::vertical()
                    .id_salt("controller_editor_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (button, action) in entries {
                            ui.horizontal(|ui| {
                                if ui.small_button("Edit").clicked() {
                                    open_form =
                                        Some(ControllerFormState::from_binding(button, action));
                                }
                                if ui.small_button("Delete").clicked() {
                                    delete_request = Some(button.clone());
                                }
                                ui.label(egui::RichText::new(button).monospace().strong());
                                ui.weak(display_action(action));
                            });
                        }
                        if row_count == 0 {
                            ui.weak("No controller bindings.");
                        }
                    });
            });

        if let Some(button) = delete_request {
            match Config::delete_single_controller_bind(&button) {
                Ok(()) => {
                    self.app_core.config.controller_binds =
                        Config::load_controller_binds().unwrap_or_default();
                    self.app_core
                        .add_system_message(&format!("Controller bind '{}' deleted.", button));
                }
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to delete controller bind: {}", err)),
            }
        }

        if let Some(form) = open_form {
            state.form = Some(form);
        }

        if let Some(mut form) = state.form.take() {
            let mut form_open = true;
            let mut submitted = false;
            let mut cancelled = false;
            let title = if form.original_button.is_some() {
                "Edit Controller Binding"
            } else {
                "Add Controller Binding"
            };
            egui::Window::new(title)
                .id(egui::Id::new("gui_controller_form"))
                .open(&mut form_open)
                .default_width(380.0)
                .show(ctx, |ui| {
                    egui::Grid::new("controller_form_grid")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Button");
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("controller_button_pick")
                                    .selected_text(if form.button.is_empty() {
                                        "pick..."
                                    } else {
                                        form.button.as_str()
                                    })
                                    .show_ui(ui, |ui| {
                                        for name in super::super::gamepad::GAMEPAD_BUTTON_NAMES {
                                            ui.selectable_value(
                                                &mut form.button,
                                                name.to_string(),
                                                name,
                                            );
                                        }
                                    });
                                let capture_label = if form.capture_armed {
                                    "Press a button..."
                                } else {
                                    "Capture"
                                };
                                if ui
                                    .add_enabled(pad_connected, egui::Button::new(capture_label))
                                    .clicked()
                                {
                                    form.capture_armed = !form.capture_armed;
                                }
                            });
                            ui.end_row();
                            ui.label("Type");
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut form.is_macro, true, "Macro");
                                ui.selectable_value(&mut form.is_macro, false, "Action");
                            });
                            ui.end_row();
                            if form.is_macro {
                                ui.label("Macro text");
                                ui.text_edit_singleline(&mut form.macro_text);
                                ui.end_row();
                            } else {
                                ui.label("Action");
                                egui::ComboBox::from_id_salt("controller_action_pick")
                                    .selected_text(if form.action.is_empty() {
                                        "pick..."
                                    } else {
                                        form.action.as_str()
                                    })
                                    .show_ui(ui, |ui| {
                                        for name in KeyAction::CONTROLLER_ACTION_NAMES {
                                            ui.selectable_value(
                                                &mut form.action,
                                                name.to_string(),
                                                *name,
                                            );
                                        }
                                    });
                                ui.end_row();
                            }
                        });
                    if form.is_macro {
                        ui.weak("Use \\r for enter (e.g. \"hide\\r\").");
                    } else {
                        ui.weak("Actions that work from a pad; anything else, use a Macro.");
                    }

                    if let Some(error) = &form.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            submitted = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancelled = true;
                        }
                    });
                });

            if submitted {
                match self.save_controller_bind_from_form(&form) {
                    Ok(()) => {
                        self.app_core.add_system_message(&format!(
                            "Controller bind '{}' saved.",
                            form.button.trim()
                        ));
                    }
                    Err(err) => {
                        form.error = Some(err);
                        state.form = Some(form);
                    }
                }
            } else if form_open && !cancelled {
                state.form = Some(form);
            }
        }

        if open {
            self.controller_editor = Some(state);
        }
    }
}
