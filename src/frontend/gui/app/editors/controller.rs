//! Controller bindings editor (`.controller`): browse/add/edit/delete the
//! `[controller]` table of the global keybinds.toml. Buttons come from a
//! dropdown or by pressing the button on a connected pad ("Capture").
//! Bindings are global — controllers belong to the desk, not a character.

use super::super::VellumGuiApp;
use crate::config::{Config, KeyAction, KeyBindAction, MacroAction, WheelSlice};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControllerTab {
    Base,
    Shift,
    Wheels,
    Rumble,
}

const RUMBLE_PATTERNS: [&str; 4] = ["off", "short", "long", "double"];

pub(in super::super) struct ControllerEditorState {
    form: Option<ControllerFormState>,
    tab: ControllerTab,
    /// Selected wheel in the Wheels tab: "" = the default wheel.
    wheel_selected: String,
    /// Unsaved working copy of the selected wheel's slices.
    wheel_buffer: Option<Vec<WheelSlice>>,
    wheel_new_name: String,
    wheel_status: Option<String>,
}

impl ControllerEditorState {
    fn new() -> Self {
        Self {
            form: None,
            tab: ControllerTab::Base,
            wheel_selected: String::new(),
            wheel_buffer: None,
            wheel_new_name: String::new(),
            wheel_status: None,
        }
    }

    fn shift_layer(&self) -> bool {
        self.tab == ControllerTab::Shift
    }
}

/// Structural edit collected while rendering the slice tree (applied
/// after the pass; in-place text/color edits mutate the buffer directly).
enum WheelOp {
    Delete(Vec<usize>),
    AddChild(Vec<usize>),
    MoveUp(Vec<usize>),
}

fn wheel_slices_at<'a>(slices: &'a mut Vec<WheelSlice>, path: &[usize]) -> Option<&'a mut Vec<WheelSlice>> {
    let mut level = slices;
    for &index in path {
        level = &mut level.get_mut(index)?.slices;
    }
    Some(level)
}

fn apply_wheel_op(slices: &mut Vec<WheelSlice>, op: WheelOp) {
    match op {
        WheelOp::Delete(path) => {
            let (last, parent) = match path.split_last() {
                Some(split) => split,
                None => return,
            };
            if let Some(level) = wheel_slices_at(slices, parent) {
                if *last < level.len() {
                    level.remove(*last);
                }
            }
        }
        WheelOp::AddChild(path) => {
            // path addresses the slice whose children gain the new entry
            // (an empty path adds a top-level slice).
            if let Some(level) = wheel_slices_at(slices, &path) {
                level.push(WheelSlice {
                    label: "new".to_string(),
                    command: String::new(),
                    color: None,
                    slices: Vec::new(),
                });
            }
        }
        WheelOp::MoveUp(path) => {
            let (last, parent) = match path.split_last() {
                Some(split) => split,
                None => return,
            };
            if *last == 0 {
                return;
            }
            if let Some(level) = wheel_slices_at(slices, parent) {
                if *last < level.len() {
                    level.swap(*last, *last - 1);
                }
            }
        }
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

    fn save_controller_bind_from_form(
        &mut self,
        form: &ControllerFormState,
        shift: bool,
    ) -> Result<(), String> {
        let (button, action) = form.build_binding()?;

        if let Some(original) = &form.original_button {
            if *original != button {
                if let Err(err) = Config::delete_single_controller_bind(original, shift) {
                    tracing::warn!("Failed to remove old controller bind '{}': {}", original, err);
                }
            }
        }

        Config::save_single_controller_bind(&button, &action, shift)
            .map_err(|err| format!("Failed to save controller bind: {}", err))?;
        self.reload_controller_binds();
        Ok(())
    }

    fn reload_controller_binds(&mut self) {
        self.app_core.config.controller_binds =
            Config::load_controller_binds().unwrap_or_default();
        self.app_core.config.controller_shift_binds =
            Config::load_controller_binds_layer(true).unwrap_or_default();
    }

    pub(in super::super) fn render_controller_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.controller_editor.take() else {
            return;
        };

        let mut open = true;
        let mut open_form: Option<ControllerFormState> = None;
        let mut delete_request: Option<String> = None;
        let mut wheel_save = false;
        let mut overlay_toggle: Option<String> = None;
        let mut rumble_save: Option<crate::config::RumbleConfig> = None;

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
                    ui.selectable_value(&mut state.tab, ControllerTab::Base, "Base");
                    ui.selectable_value(&mut state.tab, ControllerTab::Shift, "Shift layer");
                    ui.selectable_value(&mut state.tab, ControllerTab::Wheels, "Wheels");
                    ui.selectable_value(&mut state.tab, ControllerTab::Rumble, "Rumble");
                    ui.separator();
                    if matches!(state.tab, ControllerTab::Base | ControllerTab::Shift)
                        && ui.button("Add binding").clicked()
                    {
                        open_form = Some(ControllerFormState::empty());
                    }
                });
                if state.tab == ControllerTab::Shift {
                    ui.weak("Bindings while the shift button (bind one to controller_shift) is held.");
                }
                ui.separator();

                if state.tab == ControllerTab::Wheels {
                    render_wheels_tab(ui, &mut state, &self.app_core.config, &mut wheel_save);
                    return;
                }
                if state.tab == ControllerTab::Rumble {
                    let mut rumble = self.app_core.config.controller_rumble.clone();
                    let mut changed = ui
                        .checkbox(&mut rumble.enabled, "Rumble on game events")
                        .changed();
                    let pattern_row = |ui: &mut egui::Ui,
                                       label: &str,
                                       value: &mut String,
                                       changed: &mut bool| {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            egui::ComboBox::from_id_salt(format!("rumble_{label}"))
                                .selected_text(value.as_str())
                                .show_ui(ui, |ui| {
                                    for pattern in RUMBLE_PATTERNS {
                                        if ui
                                            .selectable_value(
                                                value,
                                                pattern.to_string(),
                                                pattern,
                                            )
                                            .changed()
                                        {
                                            *changed = true;
                                        }
                                    }
                                });
                        });
                    };
                    pattern_row(ui, "Roundtime ends", &mut rumble.roundtime_end, &mut changed);
                    pattern_row(ui, "Stunned", &mut rumble.stunned, &mut changed);
                    pattern_row(ui, "Death", &mut rumble.death, &mut changed);
                    ui.weak("short = light tap · long = strong buzz · double = two pulses");
                    if changed {
                        rumble_save = Some(rumble);
                    }
                    return;
                }

                let binds = if state.shift_layer() {
                    &self.app_core.config.controller_shift_binds
                } else {
                    &self.app_core.config.controller_binds
                };
                let mut entries: Vec<(&String, &KeyBindAction)> = binds.iter().collect();
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
                                // Curate the binding-legend overlay: only
                                // checked rows appear in the HUD.
                                let overlay_entry = if state.shift_layer() {
                                    format!("shift/{}", button)
                                } else {
                                    button.to_string()
                                };
                                let mut in_overlay = self
                                    .app_core
                                    .config
                                    .controller_overlay
                                    .contains(&overlay_entry);
                                if ui
                                    .checkbox(&mut in_overlay, "HUD")
                                    .on_hover_text(
                                        "Show this binding in the overlay legend \
                                         (controller_overlay toggles it; Select by default)",
                                    )
                                    .changed()
                                {
                                    overlay_toggle = Some(overlay_entry);
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

        if let Some(rumble) = rumble_save {
            match Config::save_controller_rumble(&rumble) {
                Ok(()) => self.app_core.config.controller_rumble = rumble,
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to save rumble config: {}", err)),
            }
        }

        if let Some(entry) = overlay_toggle {
            let mut list = self.app_core.config.controller_overlay.clone();
            match list.iter().position(|e| *e == entry) {
                Some(index) => {
                    list.remove(index);
                }
                None => list.push(entry),
            }
            match Config::save_controller_overlay(&list) {
                Ok(()) => self.app_core.config.controller_overlay = list,
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to save overlay list: {}", err)),
            }
        }

        if wheel_save {
            if let Some(buffer) = state.wheel_buffer.clone() {
                let name = (!state.wheel_selected.is_empty())
                    .then_some(state.wheel_selected.as_str());
                match Config::save_controller_wheel_named(name, &buffer) {
                    Ok(()) => {
                        self.app_core.config.controller_wheel =
                            Config::load_controller_wheel().unwrap_or_default();
                        self.app_core.config.controller_wheels =
                            Config::load_controller_wheels().unwrap_or_default();
                        self.app_core.push_remote_wheels();
                        state.wheel_status = Some(if buffer.is_empty() && name.is_some() {
                            "Wheel deleted (no slices).".to_string()
                        } else {
                            "Saved.".to_string()
                        });
                    }
                    Err(err) => state.wheel_status = Some(format!("Save failed: {}", err)),
                }
            }
        }

        if let Some(button) = delete_request {
            match Config::delete_single_controller_bind(&button, state.shift_layer()) {
                Ok(()) => {
                    self.reload_controller_binds();
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
                match self.save_controller_bind_from_form(&form, state.shift_layer()) {
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

/// Wheels tab: pick or create a wheel, edit its slice tree (labels,
/// commands, colors, folders), save the whole wheel at once. Saving a
/// named wheel with no slices deletes it.
fn render_wheels_tab(
    ui: &mut egui::Ui,
    state: &mut ControllerEditorState,
    config: &Config,
    save_clicked: &mut bool,
) {
    ui.horizontal(|ui| {
        let mut names: Vec<String> = config.controller_wheels.keys().cloned().collect();
        names.sort();
        egui::ComboBox::from_id_salt("controller_wheel_pick")
            .selected_text(if state.wheel_selected.is_empty() {
                "default"
            } else {
                state.wheel_selected.as_str()
            })
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.wheel_selected.is_empty(), "default")
                    .clicked()
                {
                    state.wheel_selected.clear();
                    state.wheel_buffer = None;
                    state.wheel_status = None;
                }
                for name in &names {
                    if ui
                        .selectable_label(state.wheel_selected == *name, name)
                        .clicked()
                    {
                        state.wheel_selected = name.clone();
                        state.wheel_buffer = None;
                        state.wheel_status = None;
                    }
                }
            });
        ui.add(
            egui::TextEdit::singleline(&mut state.wheel_new_name)
                .desired_width(110.0)
                .hint_text("new wheel name"),
        );
        if ui.button("New wheel").clicked() {
            let name = state.wheel_new_name.trim().to_string();
            if !name.is_empty() {
                state.wheel_selected = name;
                state.wheel_new_name.clear();
                state.wheel_buffer = Some(Vec::new());
                state.wheel_status = None;
            }
        }
    });
    ui.weak(
        "Bind a button to controller_wheel (default) or controller_wheel:<name>. \
         A slice with sub-slices is a folder; leave its command empty.",
    );
    ui.separator();

    let selected = state.wheel_selected.clone();
    let buffer = state.wheel_buffer.get_or_insert_with(|| {
        if selected.is_empty() {
            config.controller_wheel.clone()
        } else {
            config
                .controller_wheels
                .get(&selected)
                .cloned()
                .unwrap_or_default()
        }
    });

    let mut ops: Vec<WheelOp> = Vec::new();
    egui::ScrollArea::vertical()
        .id_salt("controller_wheel_slices")
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 40.0).max(60.0))
        .show(ui, |ui| {
            render_slice_rows(ui, buffer, &mut Vec::new(), &mut ops);
            if ui.button("+ Add slice").clicked() {
                ops.push(WheelOp::AddChild(Vec::new()));
            }
        });
    for op in ops {
        apply_wheel_op(buffer, op);
    }

    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Save wheel").clicked() {
            *save_clicked = true;
        }
        if let Some(status) = &state.wheel_status {
            ui.weak(status);
        }
    });
}

fn render_slice_rows(
    ui: &mut egui::Ui,
    slices: &mut Vec<WheelSlice>,
    path: &mut Vec<usize>,
    ops: &mut Vec<WheelOp>,
) {
    for i in 0..slices.len() {
        path.push(i);
        {
            let is_folder = slices[i].is_folder();
            let slice = &mut slices[i];
            ui.horizontal(|ui| {
                ui.add_space((path.len() - 1) as f32 * 18.0);
                ui.add(
                    egui::TextEdit::singleline(&mut slice.label)
                        .desired_width(100.0)
                        .hint_text("label"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut slice.command)
                        .desired_width(150.0)
                        .hint_text(if is_folder { "(folder)" } else { "command" }),
                );
                let mut color = slice.color.clone().unwrap_or_default();
                super::color_field(ui, &mut color);
                slice.color = if color.trim().is_empty() {
                    None
                } else {
                    Some(color)
                };
                if ui.small_button("^").on_hover_text("Move up").clicked() {
                    ops.push(WheelOp::MoveUp(path.clone()));
                }
                if ui
                    .small_button("+sub")
                    .on_hover_text("Add a child slice (makes this a folder)")
                    .clicked()
                {
                    ops.push(WheelOp::AddChild(path.clone()));
                }
                if ui.small_button("X").on_hover_text("Delete").clicked() {
                    ops.push(WheelOp::Delete(path.clone()));
                }
            });
        }
        if !slices[i].slices.is_empty() {
            render_slice_rows(ui, &mut slices[i].slices, path, ops);
        }
        path.pop();
    }
}
