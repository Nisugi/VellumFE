//! Keybind browser + form: add/edit/delete key bindings through the shared
//! config layer (`Config::save_single_keybind` / `delete_single_keybind`),
//! rebuilding the live keybind map after every change. The form captures key
//! combos from real key presses (macro dispatch is suppressed while armed via
//! `InputMode::KeybindForm`).

use super::super::VellumGuiApp;
use crate::config::{Config, KeyAction, KeyBindAction, MacroAction};
use crate::data::InputMode;
use eframe::egui;

pub(in super::super) struct KeybindEditorState {
    filter: String,
    form: Option<KeybindFormState>,
}

impl KeybindEditorState {
    fn new() -> Self {
        Self {
            filter: String::new(),
            form: None,
        }
    }
}

struct KeybindFormState {
    /// Some(key) when editing an existing binding; None when adding.
    original_key: Option<String>,
    original_is_global: bool,
    key: String,
    capture_armed: bool,
    is_macro: bool,
    action: String,
    macro_text: String,
    is_global: bool,
    error: Option<String>,
}

impl KeybindFormState {
    fn empty() -> Self {
        Self {
            original_key: None,
            original_is_global: true,
            key: String::new(),
            capture_armed: false,
            is_macro: true,
            action: String::new(),
            macro_text: String::new(),
            is_global: true,
            error: None,
        }
    }

    fn from_binding(key: &str, action: &KeyBindAction, is_global: bool) -> Self {
        let (is_macro, action_text, macro_text) = match action {
            KeyBindAction::Action(name) => (false, name.clone(), String::new()),
            KeyBindAction::Macro(macro_action) => {
                (true, String::new(), macro_action.macro_text.clone())
            }
        };
        Self {
            original_key: Some(key.to_string()),
            original_is_global: is_global,
            key: key.to_string(),
            capture_armed: false,
            is_macro,
            action: action_text,
            macro_text,
            is_global,
            error: None,
        }
    }

    fn build_binding(&self) -> Result<(String, KeyBindAction), String> {
        let key = self.key.trim().to_lowercase();
        if key.is_empty() {
            return Err("Key combo is required (e.g. ctrl+f, num_1, f5).".to_string());
        }
        if crate::config::parse_key_string(&key).is_none() {
            return Err(format!("Unrecognized key combo '{}'.", key));
        }
        let action = if self.is_macro {
            if self.macro_text.is_empty() {
                return Err("Macro text is required (\\r sends enter).".to_string());
            }
            // Store literal \r/\n escapes the way keybinds.toml expects.
            let text = self.macro_text.replace("\\r", "\r").replace("\\n", "\n");
            KeyBindAction::Macro(MacroAction { macro_text: text })
        } else {
            let name = self.action.trim().to_string();
            if KeyAction::from_str(&name).is_none() {
                return Err(format!("Unknown action '{}'.", name));
            }
            KeyBindAction::Action(name)
        };
        Ok((key, action))
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
    pub(in super::super) fn open_keybind_editor(&mut self) {
        self.keybind_editor = Some(KeybindEditorState::new());
    }

    pub(in super::super) fn open_keybind_form_new(&mut self) {
        let mut state = self
            .keybind_editor
            .take()
            .unwrap_or_else(KeybindEditorState::new);
        state.form = Some(KeybindFormState::empty());
        self.keybind_editor = Some(state);
    }

    fn keybind_is_character_override(&self, key: &str) -> bool {
        Config::load_character_keybinds_only(self.app_core.config.character.as_deref())
            .map(|keybinds| keybinds.contains_key(key))
            .unwrap_or(false)
    }

    fn save_keybind_from_form(&mut self, form: &KeybindFormState) -> Result<(), String> {
        let (key, action) = form.build_binding()?;
        let character = self.app_core.config.character.clone();

        if let Some(original) = &form.original_key {
            if *original != key || form.original_is_global != form.is_global {
                if let Err(err) = Config::delete_single_keybind(
                    original,
                    form.original_is_global,
                    character.as_deref(),
                ) {
                    tracing::warn!("Failed to remove old keybind '{}': {}", original, err);
                }
            }
        }

        Config::save_single_keybind(&key, &action, form.is_global, character.as_deref())
            .map_err(|err| format!("Failed to save keybind: {}", err))?;
        self.app_core.reload_keybinds();
        self.app_core.rebuild_keybind_map();
        Ok(())
    }

    pub(in super::super) fn render_keybind_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.keybind_editor.take() else {
            return;
        };

        let mut open = true;
        let mut open_form: Option<KeybindFormState> = None;
        let mut delete_request: Option<String> = None;

        egui::Window::new("Keybinds")
            .id(egui::Id::new("gui_keybind_browser"))
            .open(&mut open)
            .default_width(440.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut state.filter);
                    if ui.button("Add keybind").clicked() {
                        open_form = Some(KeybindFormState::empty());
                    }
                });
                ui.separator();

                let filter = state.filter.to_lowercase();
                let mut entries: Vec<(&String, &KeyBindAction)> = self
                    .app_core
                    .config
                    .keybinds
                    .iter()
                    .filter(|(key, action)| {
                        filter.is_empty()
                            || key.to_lowercase().contains(&filter)
                            || display_action(action).to_lowercase().contains(&filter)
                    })
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));

                let row_count = entries.len();
                egui::ScrollArea::vertical()
                    .id_salt("keybind_browser_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (key, action) in entries {
                            ui.horizontal(|ui| {
                                if ui.small_button("Edit").clicked() {
                                    let is_global = !self.keybind_is_character_override(key);
                                    open_form =
                                        Some(KeybindFormState::from_binding(key, action, is_global));
                                }
                                if ui.small_button("Delete").clicked() {
                                    delete_request = Some(key.clone());
                                }
                                ui.label(egui::RichText::new(key).monospace().strong());
                                ui.weak(display_action(action));
                            });
                        }
                        if row_count == 0 {
                            ui.weak("No keybinds match.");
                        }
                    });
            });

        if let Some(key) = delete_request {
            let is_global = !self.keybind_is_character_override(&key);
            let character = self.app_core.config.character.clone();
            match Config::delete_single_keybind(&key, is_global, character.as_deref()) {
                Ok(()) => {
                    self.app_core.reload_keybinds();
                    self.app_core.rebuild_keybind_map();
                    self.app_core
                        .add_system_message(&format!("Keybind '{}' deleted.", key));
                }
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to delete keybind: {}", err)),
            }
        }

        if let Some(form) = open_form {
            state.form = Some(form);
        }

        if let Some(mut form) = state.form.take() {
            // While capture is armed, suppress macro dispatch via the existing
            // KeybindForm input mode and grab the next key press.
            if form.capture_armed {
                self.app_core.ui_state.input_mode = InputMode::KeybindForm;
                if let Some(press) = Self::collect_pressed_key_events(ctx)
                    .into_iter()
                    .next()
                {
                    form.key = crate::core::menu_actions::key_event_to_string(press.key_event);
                    form.capture_armed = false;
                    self.app_core.ui_state.input_mode = InputMode::Normal;
                }
            }

            let mut form_open = true;
            let mut submitted = false;
            let mut cancelled = false;
            let title = if form.original_key.is_some() {
                "Edit Keybind"
            } else {
                "Add Keybind"
            };
            egui::Window::new(title)
                .id(egui::Id::new("gui_keybind_form"))
                .open(&mut form_open)
                .default_width(380.0)
                .show(ctx, |ui| {
                    egui::Grid::new("keybind_form_grid")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Key combo");
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut form.key);
                                let capture_label = if form.capture_armed {
                                    "Press a key..."
                                } else {
                                    "Capture"
                                };
                                if ui.button(capture_label).clicked() {
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
                                ui.text_edit_singleline(&mut form.action);
                                ui.end_row();
                            }
                        });
                    if form.is_macro {
                        ui.weak("Use \\r for enter (e.g. \"sw\\r\" to walk southwest).");
                    } else {
                        ui.weak("Action name, e.g. cursor_word_left, next_tab, toggle_sounds.");
                    }
                    ui.checkbox(&mut form.is_global, "Global (all characters)");

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
                match self.save_keybind_from_form(&form) {
                    Ok(()) => {
                        self.app_core
                            .add_system_message(&format!("Keybind '{}' saved.", form.key.trim()));
                        self.app_core.ui_state.input_mode = InputMode::Normal;
                    }
                    Err(err) => {
                        form.error = Some(err);
                        state.form = Some(form);
                    }
                }
            } else if form_open && !cancelled {
                state.form = Some(form);
            } else {
                // Form closed: make sure macro dispatch is re-enabled.
                self.app_core.ui_state.input_mode = InputMode::Normal;
            }
        }

        if open {
            self.keybind_editor = Some(state);
        }
    }
}
