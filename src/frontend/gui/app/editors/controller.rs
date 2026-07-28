//! Controller bindings editor (`.controller`): browse/add/edit/delete the
//! `[controller]` table of the global keybinds.toml. Buttons come from a
//! dropdown or by pressing the button on a connected pad ("Capture").
//! Bindings are global — controllers belong to the desk, not a character.

use super::super::VellumGuiApp;
use crate::config::{Config, KeyAction, KeyBindAction, MacroAction, WheelSlice, WHEEL_MIN_SPAN_DEG};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControllerTab {
    Base,
    Shift,
    Wheels,
    Rumble,
    Tuning,
}

const RUMBLE_PATTERNS: [&str; 4] = ["off", "short", "long", "double"];

/// Reserved name of the dynamic portals wheel (mirrors
/// `AppCore::PORTAL_WHEEL_KEY`). It gets a permanent Wheels-tab entry that
/// edits only its button/stick meta; its slices are generated per room.
const PORTAL_WHEEL_KEY: &str = crate::core::AppCore::PORTAL_WHEEL_KEY;

/// Movement-stick choices for the Tuning tab.
const MOVEMENT_STICKS: [&str; 2] = ["left", "right"];
/// Leaf fire-mode choices for the Tuning tab (see `[controller_tuning]`).
const FIRE_MODES: [&str; 3] = ["release", "edge", "retract"];
/// Screen-anchor choices for the reserved Back slice.
const BACK_ANCHORS: [&str; 9] = [
    "up", "down", "left", "right", "up-left", "up-right", "down-left", "down-right",
    // "none" drops the reserved Back seat in folders — ascend with East/B.
    "none",
];

pub(in super::super) struct ControllerEditorState {
    form: Option<ControllerFormState>,
    tab: ControllerTab,
    /// Selected wheel in the Wheels tab: "" = the default wheel.
    wheel_selected: String,
    /// Unsaved working copy of the selected wheel's slices.
    wheel_buffer: Option<Vec<WheelSlice>>,
    wheel_new_name: String,
    wheel_status: Option<String>,
    /// Unsaved working copy of the selected wheel's button/stick meta.
    wheel_meta_buffer: Option<crate::config::WheelMeta>,
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
            wheel_meta_buffer: None,
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
                    ..Default::default()
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

/// The `[controller]` button that opens the wheel `name_key` ("default" =>
/// the bare `controller_wheel` action; any other name => the matching
/// `controller_wheel:<name>`). `[controller]` is the runtime authority, so
/// this is the truth the editor's "Opens with" field should reflect when a
/// wheel's WheelMeta doesn't record its own button.
fn wheel_button_from_binds(config: &Config, name_key: &str) -> Option<String> {
    let wanted = if name_key == "default" {
        "controller_wheel".to_string()
    } else {
        format!("controller_wheel:{}", name_key)
    };
    config.controller_binds.iter().find_map(|(button, action)| match action {
        KeyBindAction::Action(name) if *name == wanted => Some(button.clone()),
        _ => None,
    })
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
        // Re-issuing `.controller` while it is open must not rebuild the
        // state — that would wipe an unsaved wheel buffer or open form.
        // Raise the existing window to the top instead.
        if self.controller_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_controller_editor"));
            return;
        }
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
        let mut tuning_save: Option<crate::config::TuningConfig> = None;
        let mut meta_save: Option<(String, crate::config::WheelMeta)> = None;

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
                    ui.selectable_value(&mut state.tab, ControllerTab::Tuning, "Tuning");
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
                    render_wheels_tab(
                        ui,
                        &mut state,
                        &self.app_core.config,
                        &mut wheel_save,
                        &mut meta_save,
                    );
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

                if state.tab == ControllerTab::Tuning {
                    let mut tuning = self.app_core.config.controller_tuning.clone();
                    let mut changed = false;

                    let combo_row = |ui: &mut egui::Ui,
                                     label: &str,
                                     hover: &str,
                                     value: &mut String,
                                     options: &[&str],
                                     changed: &mut bool| {
                        ui.horizontal(|ui| {
                            ui.label(label).on_hover_text(hover);
                            egui::ComboBox::from_id_salt(format!("tuning_{label}"))
                                .selected_text(value.as_str())
                                .show_ui(ui, |ui| {
                                    for opt in options {
                                        if ui
                                            .selectable_value(value, opt.to_string(), *opt)
                                            .changed()
                                        {
                                            *changed = true;
                                        }
                                    }
                                });
                        });
                    };

                    combo_row(
                        ui,
                        "Movement stick",
                        "Which analog stick walks the eight compass directions. The other \
                         stick aims the wheel and scrolls the story window.",
                        &mut tuning.movement_stick,
                        &MOVEMENT_STICKS,
                        &mut changed,
                    );
                    combo_row(
                        ui,
                        "Back slice anchor",
                        "Screen side the reserved Back slice is pinned to inside a folder. \
                         Back is a real, aimable slice; the ring rotates so its center sits \
                         on this side at every level. 'none' drops the reserved Back seat \
                         entirely — folders then have only their own slices, and you go up \
                         a level with the East/B button.",
                        &mut tuning.back_slice,
                        &BACK_ANCHORS,
                        &mut changed,
                    );

                    // Numeric feel knobs. Ranges are generous guard-rails,
                    // not hard game limits.
                    let slider_row = |ui: &mut egui::Ui,
                                      label: &str,
                                      hover: &str,
                                      value: &mut u32,
                                      range: std::ops::RangeInclusive<u32>,
                                      suffix: &str,
                                      changed: &mut bool| {
                        ui.horizontal(|ui| {
                            ui.label(label).on_hover_text(hover);
                            if ui
                                .add(egui::Slider::new(value, range).suffix(suffix))
                                .changed()
                            {
                                *changed = true;
                            }
                        });
                    };

                    ui.horizontal(|ui| {
                        ui.label("Wheel dead zone").on_hover_text(
                            "Stick deflection needed before a wheel slice registers. \
                             Higher values stop a drifting stick from picking a slice the \
                             instant the wheel opens.",
                        );
                        let mut dz = tuning.deadzone as u32;
                        if ui
                            .add(egui::Slider::new(&mut dz, 0..=95).suffix("%"))
                            .changed()
                        {
                            tuning.deadzone = dz as u8;
                            changed = true;
                        }
                    });
                    slider_row(
                        ui,
                        "Aim dwell",
                        "Hold a leaf slice this long before it commits and arms to fire on \
                         release. Slices you merely sweep across never commit.",
                        &mut tuning.aim_dwell_ms,
                        0..=1000,
                        " ms",
                        &mut changed,
                    );
                    slider_row(
                        ui,
                        "Navigation dwell",
                        "Hold a folder (or the Back slice) this long before it auto-descends \
                         (or auto-ascends). Shared by both navigation moves.",
                        &mut tuning.nav_dwell_ms,
                        0..=1000,
                        " ms",
                        &mut changed,
                    );
                    slider_row(
                        ui,
                        "Fire debounce",
                        "Suppress a repeat fire for this long after one fires, so a noisy \
                         button contact can't double-send.",
                        &mut tuning.fire_debounce_ms,
                        0..=1000,
                        " ms",
                        &mut changed,
                    );
                    slider_row(
                        ui,
                        "Release grace",
                        "After the wheel button comes up the aiming stick is usually still \
                         deflected; over this window movement stays hushed so firing the \
                         wheel doesn't also walk a direction.",
                        &mut tuning.release_grace_ms,
                        0..=300,
                        " ms",
                        &mut changed,
                    );

                    ui.separator();
                    combo_row(
                        ui,
                        "Fire mode",
                        "How a committed leaf slice fires. release: on wheel-button release \
                         (dwell to commit first). edge: the instant the stick crosses the \
                         edge threshold, no dwell — fastest on sparse wheels. retract: dwell \
                         to commit, then fire on a small inward flick (deflection dropping \
                         below its peak) without recentering. Folders always descend on dwell.",
                        &mut tuning.fire_mode,
                        &FIRE_MODES,
                        &mut changed,
                    );
                    // Only the active mode's threshold is worth showing.
                    if tuning.fire_mode == "edge" {
                        ui.horizontal(|ui| {
                            ui.label("Edge threshold").on_hover_text(
                                "Deflection at which a leaf fires in edge mode. Higher means \
                                 you must push the stick nearer the rim before it fires.",
                            );
                            if ui
                                .add(egui::Slider::new(&mut tuning.edge_threshold, 10..=100).suffix("%"))
                                .changed()
                            {
                                changed = true;
                            }
                        });
                    } else if tuning.fire_mode == "retract" {
                        ui.horizontal(|ui| {
                            ui.label("Retract delta").on_hover_text(
                                "How far the stick must pull back from its peak deflection to \
                                 fire in retract mode. Smaller means a lighter inward flick fires.",
                            );
                            if ui
                                .add(egui::Slider::new(&mut tuning.retract_delta, 1..=50).suffix("%"))
                                .changed()
                            {
                                changed = true;
                            }
                        });
                    }

                    ui.separator();
                    ui.weak(
                        "Dwell gates when a slice commits: rest on your target, then release \
                         to fire. Return the stick to center before releasing to cancel.",
                    );
                    if changed {
                        tuning_save = Some(tuning);
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

        if let Some(tuning) = tuning_save {
            match Config::save_controller_tuning(&tuning) {
                Ok(()) => self.app_core.config.controller_tuning = tuning,
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to save tuning config: {}", err)),
            }
        }

        if let Some((name, meta)) = meta_save {
            // Persist the wheel's button/stick meta.
            let mut map = self.app_core.config.controller_wheels_meta.clone();
            map.insert(name.clone(), meta.clone());
            let mut ok = true;
            if let Err(err) = Config::save_controller_wheels_meta(&map) {
                self.app_core
                    .add_system_message(&format!("Failed to save wheel meta: {}", err));
                ok = false;
            }
            // Setting a button also writes the matching [controller] entry
            // (the runtime authority) so the two never silently drift.
            if ok {
                if let Some(button) = meta.button.as_deref() {
                    let action = if name == "default" {
                        "controller_wheel".to_string()
                    } else {
                        format!("controller_wheel:{}", name)
                    };
                    if let Err(err) = Config::save_single_controller_bind(
                        button,
                        &KeyBindAction::Action(action),
                        false,
                    ) {
                        self.app_core
                            .add_system_message(&format!("Failed to bind wheel button: {}", err));
                        ok = false;
                    }
                }
            }
            if ok {
                self.app_core.config.controller_wheels_meta = map;
                self.reload_controller_binds();
                self.app_core.warn_wheel_binding_conflicts();
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
                        // Surface any span problems in what was just saved
                        // (advisory — the runtime still produces a usable
                        // ring). The inline editor advisory is B6.
                        self.app_core.warn_wheel_span_conflicts();
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
    meta_save: &mut Option<(String, crate::config::WheelMeta)>,
) {
    ui.horizontal(|ui| {
        // User-defined wheels, minus "portals": the portals wheel is dynamic
        // (slices built from the room), so it never lives in controller_wheels
        // as an editable slice array — but it still gets a permanent,
        // non-deletable entry below so its button/stick meta is reachable.
        let mut names: Vec<String> = config
            .controller_wheels
            .keys()
            .filter(|name| name.as_str() != PORTAL_WHEEL_KEY)
            .cloned()
            .collect();
        names.sort();
        let select_wheel = |state: &mut ControllerEditorState, name: &str| {
            state.wheel_selected = name.to_string();
            state.wheel_buffer = None;
            state.wheel_meta_buffer = None;
            state.wheel_status = None;
        };
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
                    state.wheel_meta_buffer = None;
                    state.wheel_status = None;
                }
                // Permanent portals entry: button/stick only, no slice list.
                if ui
                    .selectable_label(state.wheel_selected == PORTAL_WHEEL_KEY, "portals (dynamic)")
                    .clicked()
                {
                    select_wheel(state, PORTAL_WHEEL_KEY);
                }
                for name in &names {
                    if ui
                        .selectable_label(state.wheel_selected == *name, name)
                        .clicked()
                    {
                        select_wheel(state, name);
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
            if name == PORTAL_WHEEL_KEY {
                // "portals" is reserved for the dynamic wheel; a static wheel
                // of that name would be shadowed and never open.
                state.wheel_status =
                    Some("'portals' is reserved for the dynamic room wheel.".to_string());
            } else if !name.is_empty() {
                state.wheel_selected = name;
                state.wheel_new_name.clear();
                state.wheel_buffer = Some(Vec::new());
                state.wheel_meta_buffer = Some(crate::config::WheelMeta::default());
                state.wheel_status = None;
            }
        }
    });
    ui.weak(
        "A slice with sub-slices is a folder; leave its command empty.",
    );
    ui.separator();

    // Per-wheel button + aim stick (WheelMeta). Setting the button writes
    // the matching [controller] entry too; the stick overrides the global
    // movement-stick choice while this wheel is open.
    {
        let selected = state.wheel_selected.clone();
        let name_key = if selected.is_empty() { "default" } else { selected.as_str() };
        let meta = state.wheel_meta_buffer.get_or_insert_with(|| {
            let mut meta = config
                .controller_wheels_meta
                .get(name_key)
                .cloned()
                .unwrap_or_default();
            // Back-fill the button from [controller] (the runtime authority)
            // when the meta doesn't record one — e.g. a wheel bound via the
            // old Base-tab action. Keeps "Opens with" showing the real key.
            if meta.button.is_none() {
                meta.button = wheel_button_from_binds(config, name_key);
            }
            meta
        });
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Opens with").on_hover_text(
                "Button that holds-open this wheel. Saving writes the matching \
                 [controller] entry (the runtime binding authority).",
            );
            let cur_button = meta.button.clone().unwrap_or_default();
            egui::ComboBox::from_id_salt("wheel_meta_button")
                .selected_text(if cur_button.is_empty() { "(unset)" } else { cur_button.as_str() })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(meta.button.is_none(), "(unset)").clicked() {
                        meta.button = None;
                        changed = true;
                    }
                    for b in super::super::gamepad::GAMEPAD_BUTTON_NAMES {
                        if ui.selectable_label(meta.button.as_deref() == Some(b), b).clicked() {
                            meta.button = Some(b.to_string());
                            changed = true;
                        }
                    }
                });

            ui.separator();
            ui.label("Aim stick").on_hover_text(
                "Which stick aims this wheel. Overrides the global movement stick \
                 while the wheel is open; if it names the movement stick, walking is \
                 silenced for that wheel. (unset) = the non-movement stick.",
            );
            let cur_stick = meta.stick.clone().unwrap_or_default();
            egui::ComboBox::from_id_salt("wheel_meta_stick")
                .selected_text(if cur_stick.is_empty() { "(unset)" } else { cur_stick.as_str() })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(meta.stick.is_none(), "(unset)").clicked() {
                        meta.stick = None;
                        changed = true;
                    }
                    for s in ["left", "right"] {
                        if ui.selectable_label(meta.stick.as_deref() == Some(s), s).clicked() {
                            meta.stick = Some(s.to_string());
                            changed = true;
                        }
                    }
                });

            ui.separator();
            ui.label("Start").on_hover_text(
                "Ring rotation in degrees (0 = up, clockwise). Rotates the whole \
                 layout so the slices sit where your thumb likes them. 0 = the \
                 first slice at the top.",
            );
            let mut start = meta.start.unwrap_or(0.0);
            if ui
                .add(
                    egui::DragValue::new(&mut start)
                        .speed(1.0)
                        .range(0.0..=359.0)
                        .suffix("°"),
                )
                .changed()
            {
                let norm = start.rem_euclid(360.0);
                meta.start = if norm.abs() < f32::EPSILON { None } else { Some(norm) };
                changed = true;
            }
        });
        if changed {
            *meta_save = Some((name_key.to_string(), meta.clone()));
        }
    }
    ui.separator();

    // The portals wheel has no editable slice list — they are built from the
    // current room each time it opens. Only its button/stick meta (above) is
    // configurable, and that saves on change, so there's no "Save wheel".
    if state.wheel_selected == PORTAL_WHEEL_KEY {
        ui.weak(
            "Slices are generated from the room's portals each time the wheel \
             opens, so there is no slice list to edit here. Set the button and \
             aim stick above; they save automatically.",
        );
        if let Some(status) = &state.wheel_status {
            ui.weak(status);
        }
        return;
    }

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

    // Inner aim floor can't exceed usable travel: it must sit below the
    // fire point (edge threshold, minus the retract delta) with a little
    // headroom, so every slice keeps room between its floor and firing.
    let t = &config.controller_tuning;
    let inner_ceiling = t
        .edge_threshold
        .saturating_sub(t.retract_delta)
        .saturating_sub(5)
        .max(5);

    let mut ops: Vec<WheelOp> = Vec::new();
    egui::ScrollArea::vertical()
        .id_salt("controller_wheel_slices")
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 60.0).max(60.0))
        .show(ui, |ui| {
            render_slice_rows(ui, buffer, &mut Vec::new(), &mut ops, inner_ceiling);
            if ui.button("+ Add slice").clicked() {
                ops.push(WheelOp::AddChild(Vec::new()));
            }
        });
    for op in ops {
        apply_wheel_op(buffer, op);
    }

    // Inline advisory: warn (without blocking) when the current buffer's
    // spans don't fit — same check the load/save-time warner runs.
    let wheel_name = if state.wheel_selected.is_empty() {
        "default"
    } else {
        state.wheel_selected.as_str()
    };
    let span_issues = crate::config::validate_wheel_spans(wheel_name, buffer);

    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Save wheel").clicked() {
            *save_clicked = true;
        }
        if let Some(status) = &state.wheel_status {
            ui.weak(status);
        }
    });
    for issue in &span_issues {
        ui.colored_label(ui.visuals().warn_fg_color, format!("⚠ {}", issue.message()));
    }
}

fn render_slice_rows(
    ui: &mut egui::Ui,
    slices: &mut Vec<WheelSlice>,
    path: &mut Vec<usize>,
    ops: &mut Vec<WheelOp>,
    inner_ceiling: u8,
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

                // Span: 0 = auto (share the remainder). Non-zero is clamped
                // to the minimum so the field can't express an unhittable
                // wedge; the resolver clamps too, this just shows it.
                let mut span = slice.span.unwrap_or(0.0);
                if ui
                    .add(
                        egui::DragValue::new(&mut span)
                            .speed(1.0)
                            .range(0.0..=300.0)
                            .suffix("°")
                            .custom_formatter(|n, _| {
                                if n <= 0.0 { "auto".to_string() } else { format!("{n:.0}°") }
                            }),
                    )
                    .on_hover_text("Wedge width in degrees. auto (0) shares the leftover evenly.")
                    .changed()
                {
                    slice.span = if span <= 0.0 {
                        None
                    } else {
                        Some(span.max(WHEEL_MIN_SPAN_DEG))
                    };
                }

                // Inner: 0 = auto (global dead zone). Non-zero is the aim
                // floor in percent, clamped to a usable ceiling so a slice
                // never demands more throw than it has travel before firing.
                let mut inner = slice.inner.unwrap_or(0) as f32;
                if ui
                    .add(
                        egui::DragValue::new(&mut inner)
                            .speed(1.0)
                            .range(0.0..=inner_ceiling as f32)
                            .custom_formatter(|n, _| {
                                if n <= 0.0 { "auto".to_string() } else { format!("{n:.0}%") }
                            }),
                    )
                    .on_hover_text(
                        "Aim floor: how far the stick must push before this slice registers. \
                         auto (0) uses the global dead zone.",
                    )
                    .changed()
                {
                    slice.inner = if inner <= 0.0 { None } else { Some(inner as u8) };
                }

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
            render_slice_rows(ui, &mut slices[i].slices, path, ops, inner_ceiling);
        }
        path.pop();
    }
}
