//! Menu keybind editor: the 26 fixed `MenuKeybinds` navigation/action keys
//! (arrow nav, select/cancel/save, clipboard, reorder, list mgmt) that are
//! active only while a menu/browser/form has focus. Unlike game keybinds
//! (a browsable key→action map), these are a fixed field set, so the editor is
//! a form of one row per field driven by `MenuKeybinds::FIELDS`. Persists via
//! `Config::save_menu_keybinds` and re-validates with the shared validator.

use super::super::VellumGuiApp;
use crate::config::{Config, MenuKeybinds};
use crate::config::menu_keybind_validator::validate_menu_keybinds;
use crate::data::InputMode;
use eframe::egui;

pub(in super::super) struct MenuKeybindEditorState {
    /// Working copy edited in place; committed to disk on Save.
    working: MenuKeybinds,
    /// true = write global/keybinds.toml, false = this character's profile.
    is_global: bool,
    /// Index into `MenuKeybinds::FIELDS` currently capturing a key press,
    /// or None. Capture arms `InputMode::KeybindForm` to suppress dispatch.
    capturing: Option<usize>,
}

impl MenuKeybindEditorState {
    fn new(menu: MenuKeybinds) -> Self {
        Self {
            working: menu,
            is_global: true,
            capturing: None,
        }
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_menu_keybind_editor(&mut self) {
        if self.menu_keybind_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_menu_keybind_editor"));
            return;
        }
        // Seed the working copy from the live merged config.
        self.menu_keybind_editor = Some(MenuKeybindEditorState::new(
            self.app_core.config.menu_keybinds.clone(),
        ));
    }

    /// The capture flow must not let the focus-follows rule steal focus.
    pub(in super::super) fn menu_keybind_capture_armed(&self) -> bool {
        self.menu_keybind_editor
            .as_ref()
            .is_some_and(|s| s.capturing.is_some())
    }

    pub(in super::super) fn render_menu_keybind_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.menu_keybind_editor.take() else {
            return;
        };

        // While a row is capturing, grab the next key press into that field.
        if let Some(idx) = state.capturing {
            self.app_core.ui_state.input_mode = InputMode::KeybindForm;
            // Numpad presses come from eframe's channel via `handle_global_input`;
            // see the keybind editor for why they take priority.
            let captured = self.frame_numpad_presses.first().copied().or_else(|| {
                Self::collect_pressed_key_events(ctx)
                    .into_iter()
                    .next()
                    .map(|press| press.key_event)
            });

            if let Some(key_event) = captured {
                let key = crate::core::menu_actions::key_event_to_string(key_event);
                if let Some(field) = MenuKeybinds::FIELDS.get(idx) {
                    (field.set)(&mut state.working, key);
                }
                state.capturing = None;
                self.app_core.ui_state.input_mode = InputMode::Normal;
            }
        }

        let mut open = true;
        let mut save_request = false;
        let mut reset_all = false;

        egui::Window::new("Menu Keybinds")
            .id(egui::Id::new("gui_menu_keybind_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(420.0)
            .default_height(520.0)
            .show(ctx, |ui| {
                ui.label(
                    "Keys used while a menu, browser, or form has focus. \
                     These are separate from game keybinds.",
                );
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.is_global, "Global (all characters)")
                        .on_hover_text(
                            "On: save to the shared keybinds file. Off: save to \
                             this character's profile.",
                        );
                    if ui.button("Reset all to defaults").clicked() {
                        reset_all = true;
                    }
                });

                // Surface validation inline so missing/duplicate binds are
                // visible before the user leaves the editor.
                let validation = validate_menu_keybinds(&state.working);
                for issue in &validation.issues {
                    let color = match issue.severity() {
                        crate::config::menu_keybind_validator::ValidationSeverity::Error => {
                            ui.visuals().error_fg_color
                        }
                        crate::config::menu_keybind_validator::ValidationSeverity::Warning => {
                            ui.visuals().warn_fg_color
                        }
                    };
                    ui.colored_label(color, issue.message());
                }
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("menu_keybind_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut last_group = "";
                        egui::Grid::new("menu_keybind_grid")
                            .num_columns(3)
                            .striped(true)
                            .show(ui, |ui| {
                                for (i, field) in MenuKeybinds::FIELDS.iter().enumerate() {
                                    if field.group != last_group {
                                        last_group = field.group;
                                        ui.label(egui::RichText::new(field.group).strong());
                                        ui.label("");
                                        ui.label("");
                                        ui.end_row();
                                    }
                                    ui.label(field.label);
                                    let current = (field.get)(&state.working);
                                    let armed = state.capturing == Some(i);
                                    let key_text = if armed {
                                        "Press a key…".to_string()
                                    } else if current.is_empty() {
                                        "(unset)".to_string()
                                    } else {
                                        current.to_string()
                                    };
                                    ui.label(egui::RichText::new(key_text).monospace());
                                    ui.horizontal(|ui| {
                                        let label = if armed { "…" } else { "Capture" };
                                        if ui.small_button(label).clicked() {
                                            state.capturing =
                                                if armed { None } else { Some(i) };
                                        }
                                        if ui.small_button("Default").clicked() {
                                            let def = MenuKeybinds::default();
                                            (field.set)(
                                                &mut state.working,
                                                (field.get)(&def).to_string(),
                                            );
                                        }
                                    });
                                    ui.end_row();
                                }
                            });
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save_request = true;
                    }
                    if ui.button("Revert").clicked() {
                        // Re-seed from the live config, dropping edits.
                        state.working = self.app_core.config.menu_keybinds.clone();
                        state.capturing = None;
                    }
                });
            });

        if reset_all {
            state.working = MenuKeybinds::default();
            state.capturing = None;
        }

        if save_request {
            let character = self.app_core.config.character.clone();
            match Config::save_menu_keybinds(
                &state.working,
                state.is_global,
                character.as_deref(),
            ) {
                Ok(()) => {
                    // Reflect the save in the live config so menus use it now.
                    self.app_core.config.menu_keybinds = state.working.clone();
                    self.app_core.add_system_message("Menu keybinds saved.");
                }
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to save menu keybinds: {}", err)),
            }
        }

        if open {
            self.menu_keybind_editor = Some(state);
        }
    }
}
