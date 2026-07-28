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

/// How the Wheels tab presents the slice buffer: the drag-and-drop canvas
/// (default) or the classic numeric row list (power/fallback path). Both
/// edit the same buffer, so switching modes never loses work.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WheelViewMode {
    Visual,
    Numeric,
}

/// Live pointer drag on the designer canvas. `start_dirty` on a divider
/// drag records that the ring's `start` was rotated to keep the other
/// dividers pinned, so the wheel meta must be saved when the drag ends.
#[derive(Clone, Copy, PartialEq)]
enum WheelDesignerDrag {
    None,
    Divider { boundary: usize, start_dirty: bool },
    /// Radial drag of one slice's aim floor (its `inner`).
    InnerArc { slice: usize },
    /// Body drag of a whole wedge to another position around the ring;
    /// the move applies on release (`target` tracks the seat under the
    /// pointer, highlighted while dragging).
    Wedge { slice: usize, target: usize },
}

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
    /// Visual designer canvas vs numeric row list.
    wheel_view_mode: WheelViewMode,
    /// Designer: folder level being edited, as indices into the buffer
    /// tree (empty = top level). The canvas draws this level's ring.
    wheel_designer_path: Vec<usize>,
    /// Designer: index of the selected slice within the current level.
    wheel_selected_slice: Option<usize>,
    /// Designer: drag in progress on the canvas.
    wheel_drag: WheelDesignerDrag,
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
            wheel_view_mode: WheelViewMode::Visual,
            wheel_designer_path: Vec::new(),
            wheel_selected_slice: None,
            wheel_drag: WheelDesignerDrag::None,
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
        let mut rumble_test: Option<(f32, u32, u32, u32)> = None;
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
                        .on_hover_text(
                            "Master switch for all controller vibration, \
                             including rumbles fired by highlight rules.",
                        )
                        .changed();
                    // Dropdown options: off + built-ins + user patterns.
                    let mut options: Vec<String> =
                        RUMBLE_PATTERNS.iter().map(|s| s.to_string()).collect();
                    options.extend(rumble.patterns.iter().map(|p| p.name.clone()));
                    let pattern_row = |ui: &mut egui::Ui,
                                       label: &str,
                                       hover: &str,
                                       options: &[String],
                                       value: &mut String,
                                       changed: &mut bool| {
                        ui.horizontal(|ui| {
                            ui.label(label).on_hover_text(hover);
                            egui::ComboBox::from_id_salt(format!("rumble_{label}"))
                                .selected_text(value.as_str())
                                .show_ui(ui, |ui| {
                                    for pattern in options {
                                        if ui
                                            .selectable_value(
                                                value,
                                                pattern.clone(),
                                                pattern,
                                            )
                                            .changed()
                                        {
                                            *changed = true;
                                        }
                                    }
                                })
                                .response
                                .on_hover_text(hover);
                        });
                    };
                    pattern_row(
                        ui,
                        "Roundtime ends",
                        "Buzz when roundtime or casttime finishes — hands \
                         are free again.",
                        &options,
                        &mut rumble.roundtime_end,
                        &mut changed,
                    );
                    pattern_row(
                        ui,
                        "Stunned",
                        "Buzz the moment the character becomes stunned.",
                        &options,
                        &mut rumble.stunned,
                        &mut changed,
                    );
                    pattern_row(
                        ui,
                        "Death",
                        "Buzz when the character dies.",
                        &options,
                        &mut rumble.death,
                        &mut changed,
                    );
                    ui.weak("short = light tap · long = strong buzz · double = two pulses");

                    ui.separator();
                    ui.label("Custom patterns").on_hover_text(
                        "Your own vibration patterns. They appear in the \
                         dropdowns above and in the Highlights editor, so \
                         any highlight rule can buzz the pad. Built-in \
                         names (short/long/double) win on collision.",
                    );
                    let mut remove: Option<usize> = None;
                    for (i, pattern) in rumble.patterns.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut pattern.name)
                                        .hint_text("name")
                                        .desired_width(90.0),
                                )
                                .on_hover_text(
                                    "Name to select this pattern by \
                                     (event rows, highlight rules).",
                                )
                                .changed();
                            changed |= ui
                                .add(
                                    egui::Slider::new(&mut pattern.strength, 0.05..=1.0)
                                        .show_value(false),
                                )
                                .on_hover_text("Vibration strength.")
                                .changed();
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut pattern.pulse_ms)
                                        .range(20..=2000)
                                        .suffix(" ms"),
                                )
                                .on_hover_text("Length of each buzz.")
                                .changed();
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut pattern.pulses)
                                        .range(1..=8)
                                        .prefix("x"),
                                )
                                .on_hover_text("Number of buzzes.")
                                .changed();
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut pattern.gap_ms)
                                        .range(0..=2000)
                                        .suffix(" ms gap"),
                                )
                                .on_hover_text("Silence between buzzes.")
                                .changed();
                            if ui
                                .button("Test")
                                .on_hover_text("Play this pattern on the pad now.")
                                .clicked()
                            {
                                rumble_test = Some((
                                    pattern.strength.clamp(0.05, 1.0),
                                    pattern.pulse_ms.clamp(20, 2000),
                                    pattern.pulses.clamp(1, 8),
                                    pattern.gap_ms.min(2000),
                                ));
                            }
                            if ui
                                .button("X")
                                .on_hover_text("Delete this pattern.")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        rumble.patterns.remove(i);
                        changed = true;
                    }
                    if ui
                        .button("+ Add pattern")
                        .on_hover_text("Add a new custom vibration pattern.")
                        .clicked()
                    {
                        let name = format!("custom-{}", rumble.patterns.len() + 1);
                        rumble.patterns.push(crate::config::RumblePattern {
                            name,
                            ..Default::default()
                        });
                        changed = true;
                    }
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
                            // Hover help on the widget too — most people
                            // point at the control, not its label.
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
                                })
                                .response
                                .on_hover_text(hover);
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
                                .on_hover_text(hover)
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
                            .on_hover_text(
                                "Stick deflection needed before a wheel slice \
                                 registers. Higher values stop a drifting stick \
                                 from picking a slice the instant the wheel opens.",
                            )
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
        if let Some(resolved) = rumble_test {
            // Uses the row's live values, so Test previews unsaved edits.
            self.play_rumble_resolved(resolved);
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
            state.wheel_designer_path.clear();
            state.wheel_selected_slice = None;
            state.wheel_drag = WheelDesignerDrag::None;
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
                    select_wheel(state, "");
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
                state.wheel_designer_path.clear();
                state.wheel_selected_slice = None;
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
    let wheel_name = if selected.is_empty() { "default" } else { selected.as_str() };
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

    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.wheel_view_mode, WheelViewMode::Visual, "Visual")
            .on_hover_text("Drag-and-drop wheel canvas.");
        ui.selectable_value(&mut state.wheel_view_mode, WheelViewMode::Numeric, "Numeric")
            .on_hover_text("Every slice as an editable row; exact numbers.");
    });

    let mut ops: Vec<WheelOp> = Vec::new();
    match state.wheel_view_mode {
        WheelViewMode::Numeric => {
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
        }
        WheelViewMode::Visual => {
            let global_dz =
                (config.controller_tuning.deadzone as f32 / 100.0).clamp(0.0, 0.99);
            let meta = state
                .wheel_meta_buffer
                .get_or_insert_with(Default::default);
            render_wheel_designer(
                ui,
                wheel_name,
                buffer,
                &mut state.wheel_designer_path,
                &mut state.wheel_selected_slice,
                &mut state.wheel_drag,
                meta,
                meta_save,
                &config.controller_tuning.back_slice,
                global_dz,
                inner_ceiling,
                &mut ops,
            );
        }
    }
    for op in ops {
        apply_wheel_op(buffer, op);
    }

    // Inline advisory: warn (without blocking) when the current buffer's
    // spans don't fit — same check the load/save-time warner runs.
    let span_issues = crate::config::validate_wheel_spans(wheel_name, buffer);

    ui.separator();
    ui.horizontal(|ui| {
        if ui
            .button("Save wheel")
            .on_hover_text(
                "Write this wheel's slices to keybinds.toml. Until then edits \
                 live only in this editor.",
            )
            .clicked()
        {
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
            let slice = &mut slices[i];
            ui.horizontal(|ui| {
                ui.add_space((path.len() - 1) as f32 * 18.0);
                render_slice_fields(ui, slice, inner_ceiling);

                if ui.small_button("^").on_hover_text("Move up").clicked() {
                    ops.push(WheelOp::MoveUp(path.clone()));
                }
                if !slice.back
                    && ui
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

/// The per-slice edit widgets — label, command, color, span, inner —
/// shared by the numeric rows and the designer's selected-slice panel so
/// the two edit paths can't diverge. Caller supplies the surrounding row
/// (indent, move/add/delete buttons).
fn render_slice_fields(ui: &mut egui::Ui, slice: &mut WheelSlice, inner_ceiling: u8) {
    let is_folder = slice.is_folder();
    ui.add(
        egui::TextEdit::singleline(&mut slice.label)
            .desired_width(100.0)
            .hint_text("label"),
    )
    .on_hover_text("Label: the text drawn on this wedge.");
    if slice.back {
        // A Back slice never fires a command — dwelling it goes up a
        // level — so there is nothing to type here.
        ui.add_sized(
            [150.0, 18.0],
            egui::Label::new(egui::RichText::new("(goes up one level)").weak()),
        )
        .on_hover_text(
            "This is the Back slice: dwelling it ascends to the parent \
             ring. It has no command; everything else — width, color, \
             floor, position — edits like any other slice.",
        );
    } else {
        ui.add(
            egui::TextEdit::singleline(&mut slice.command)
                .desired_width(150.0)
                .hint_text(if is_folder { "(folder)" } else { "command" }),
        )
        .on_hover_text(
            "Command sent to the game when this slice fires. A slice with \
             sub-slices is a folder — leave its command empty.",
        );
    }
    let mut color = slice.color.clone().unwrap_or_default();
    super::color_field(ui, &mut color);
    slice.color = if color.trim().is_empty() {
        None
    } else {
        Some(color)
    };

    // Span: 0 = auto (share the remainder). Non-zero is clamped
    // to the minimum so the field can't express an unhittable
    // wedge; the resolver clamps too, this just shows it. A locked
    // slice's width is frozen — the field disables with the drags.
    let mut span = slice.span.unwrap_or(0.0);
    if ui
        .add_enabled(
            !slice.locked,
            egui::DragValue::new(&mut span)
                .speed(1.0)
                .range(0.0..=300.0)
                .suffix("°")
                .custom_formatter(|n, _| {
                    if n <= 0.0 { "auto".to_string() } else { format!("{n:.0}°") }
                }),
        )
        .on_hover_text("Wedge width in degrees. auto (0) shares the leftover evenly.")
        .on_disabled_hover_text("Width is locked — untick lock to resize.")
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
}

/// The Visual designer: the current folder level drawn as a wheel with the
/// live renderer's own painter (`paint_wheel_ring` — the preview can't
/// drift from the real thing), a breadcrumb naming the level, click on a
/// wedge to select it, and the selected slice's fields underneath. Edits
/// go to the same buffer the numeric rows use.
///
/// Dragging a divider trades width between its two seats
/// (`apply_divider_drag`); the first drag frame freezes every auto span at
/// its resolved width so the whole ring edits as concrete numbers. A trade
/// touching seat 0 rotates the ring's `start` to keep the other dividers
/// pinned — that lands in the wheel meta and is saved when the drag ends.
#[allow(clippy::too_many_arguments)]
fn render_wheel_designer(
    ui: &mut egui::Ui,
    wheel_name: &str,
    buffer: &mut Vec<WheelSlice>,
    designer_path: &mut Vec<usize>,
    selected_slice: &mut Option<usize>,
    drag: &mut WheelDesignerDrag,
    meta: &mut crate::config::WheelMeta,
    meta_save: &mut Option<(String, crate::config::WheelMeta)>,
    back_anchor: &str,
    global_deadzone: f32,
    inner_ceiling: u8,
    ops: &mut Vec<WheelOp>,
) {
    // A structural edit (delete, move) can strand the path; fall back to
    // the top level rather than a blank canvas.
    if wheel_slices_at(buffer, designer_path).is_none() {
        designer_path.clear();
        *selected_slice = None;
    }

    // Breadcrumb: wheel name, then each folder down to the shown level.
    let crumbs: Vec<String> = {
        let mut crumbs = vec![wheel_name.to_string()];
        let mut level: &[WheelSlice] = buffer;
        for &i in designer_path.iter() {
            let Some(slice) = level.get(i) else { break };
            crumbs.push(slice.label.clone());
            level = &slice.slices;
        }
        crumbs
    };
    ui.horizontal(|ui| {
        let mut jump: Option<usize> = None;
        for (i, crumb) in crumbs.iter().enumerate() {
            if i > 0 {
                ui.weak("▸");
            }
            if i + 1 == crumbs.len() {
                ui.strong(crumb);
            } else if ui.link(crumb).on_hover_text("Go back to this level").clicked() {
                jump = Some(i);
            }
        }
        if let Some(i) = jump {
            designer_path.truncate(i);
            *selected_slice = None;
        }
    });

    let at_top = designer_path.is_empty();
    let level = wheel_slices_at(buffer, designer_path)
        .expect("designer path resolves after the fallback above");
    // The display ring, built exactly as the runtime builds it: inside a
    // folder the reserved Back seat is appended as a read-only ghost and
    // the whole ring rotates to land it on its anchor, so the designer
    // shows the geometry the wheel will actually have (`back_slice =
    // "none"` folders have no ghost, also like the runtime).
    let build_view = |level: &[WheelSlice], meta: &crate::config::WheelMeta| {
        super::super::gamepad::WheelView::build(
            level,
            !at_top,
            back_anchor,
            meta.start.unwrap_or(0.0),
        )
    };

    // Canvas: full available width, wheel centered; leave room below for
    // the selected-slice panel and the Save row.
    const PANEL_RESERVE: f32 = 150.0;
    let avail = ui.available_size();
    let side = avail
        .x
        .min((avail.y - PANEL_RESERVE).max(200.0))
        .clamp(200.0, 440.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::new(avail.x.max(side), side),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter().with_clip_rect(rect);
    let center = rect.center();
    let outer = side / 2.0 - 8.0;
    let hub = (outer * 0.22).clamp(28.0, 46.0);
    // Same label placement as the live wheel: 34px inside the rim.
    let label_radius = outer - 34.0;
    // Pointer position → aim-convention degrees (0 = up, clockwise).
    let aim_of = |pos: egui::Pos2| {
        let v = pos - center;
        v.x.atan2(-v.y).to_degrees().rem_euclid(360.0)
    };

    // What a grab at `pos` would take hold of: a draggable divider (grab
    // priority), else the aimed wedge's floor arc — the slice's own
    // `inner`, or the global dead-zone radius when it has none (that's the
    // affordance for creating one). Shared by the drag start and the
    // hover-cursor hint so they can't disagree.
    let grab_handle = |level: &[WheelSlice],
                       meta: &crate::config::WheelMeta,
                       pos: egui::Pos2|
     -> WheelDesignerDrag {
        let v = pos - center;
        let r = v.length();
        if level.is_empty() || r < hub * 0.6 || r > outer + 12.0 {
            return WheelDesignerDrag::None;
        }
        let view = build_view(level, meta);
        let real_len = level.len();
        let has_ghost = view.layout.len() > real_len;
        let aim = aim_of(pos);
        // Nearest draggable divider (the shared edge after each seat)
        // within grab range. The ghost Back seat's width is the runtime's
        // to decide, so neither of its edges is draggable — and a locked
        // slice's width can't be traded, so neither of its edges is
        // either.
        let draggable = |b: usize| {
            let structural = if has_ghost { b + 1 < real_len } else { real_len >= 2 };
            structural && !level[b].locked && !level[(b + 1) % real_len].locked
        };
        let nearest = view
            .layout
            .seats
            .iter()
            .enumerate()
            .filter(|(b, _)| draggable(*b))
            .map(|(b, seat)| {
                let angle = (seat.start_deg + seat.span_deg).rem_euclid(360.0);
                (b, super::super::gamepad::angular_gap(aim, angle))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, gap)| *gap <= 8.0);
        if let Some((boundary, _)) = nearest {
            return WheelDesignerDrag::Divider { boundary, start_dirty: false };
        }
        if let Some(seat) = super::super::gamepad::seat_index_at_angle(v.x, -v.y, &view.layout)
        {
            // Aim floors belong to real slices only.
            if seat < real_len {
                let floor = level[seat]
                    .inner
                    .map(|p| p as f32 / 100.0)
                    .unwrap_or(global_deadzone);
                let floor_r = hub + (outer - hub) * floor;
                if (r - floor_r).abs() <= 10.0 {
                    return WheelDesignerDrag::InnerArc { slice: seat };
                }
            }
        }
        WheelDesignerDrag::None
    };

    // ----- Drag lifecycle (before painting, so the ring drawn this frame
    // already reflects the pointer). -----
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut grabbed = grab_handle(level, meta, pos);
            if let WheelDesignerDrag::Divider { .. } = grabbed {
                // Freeze the real slices to concrete widths so the drag
                // edits predictable numbers (auto seats would otherwise
                // re-split under the pointer). A ghost Back stays auto —
                // its width is the remainder, exactly as at runtime.
                let view = build_view(level, meta);
                materialize_spans(level, &view.layout);
            }
            // Nothing grabbable under the pointer: a drag on a wedge's
            // body moves the whole slice to another position (applied on
            // release). The ghost Back isn't movable — its seat is the
            // runtime's.
            if grabbed == WheelDesignerDrag::None {
                let v = pos - center;
                let r = v.length();
                if r >= hub && r <= outer {
                    let view = build_view(level, meta);
                    if let Some(seat) =
                        super::super::gamepad::seat_index_at_angle(v.x, -v.y, &view.layout)
                    {
                        if seat < level.len() {
                            grabbed = WheelDesignerDrag::Wedge { slice: seat, target: seat };
                        }
                    }
                }
            }
            *drag = grabbed;
        }
    }
    if let WheelDesignerDrag::Divider { boundary, start_dirty } = drag {
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let view = build_view(level, meta);
                if let Some(seat0) = view.layout.seats.first() {
                    // The ring's effective rotation: seat 0's center — the
                    // wheel's `start` at the top level, the Back-anchor
                    // rotation inside a folder.
                    let old_start = seat0.center_deg();
                    let mut widths: Vec<f32> =
                        view.layout.seats.iter().map(|s| s.span_deg).collect();
                    // Snap to the compass points (45° multiples) when the
                    // pointer is close; Shift holds the drag free.
                    let mut target = aim_of(pos);
                    if !ui.input(|i| i.modifiers.shift) {
                        let compass = ((target / 45.0).round() * 45.0).rem_euclid(360.0);
                        if super::super::gamepad::angular_gap(target, compass) <= 4.0 {
                            target = compass;
                        }
                    }
                    let new_start =
                        apply_divider_drag(&mut widths, old_start, *boundary, target);
                    for (slice, w) in level.iter_mut().zip(&widths) {
                        slice.span = Some(*w);
                    }
                    // A trade touching seat 0 rotates the ring to keep the
                    // other dividers pinned. At the top level that rotation
                    // is real state (the wheel's `start`). In folders it is
                    // discarded: an anchored folder re-derives its rotation
                    // from the Back anchor on rebuild, which cancels the
                    // shift exactly; a `back_slice = "none"` folder shares
                    // `start` with every other level, so its two seat-0
                    // dividers track at half speed rather than rotating the
                    // whole wheel from inside one folder.
                    if at_top && (new_start - old_start).abs() > 1e-4 {
                        let norm = new_start.rem_euclid(360.0);
                        meta.start = if norm.abs() < 1e-4 { None } else { Some(norm) };
                        *start_dirty = true;
                    }
                }
            }
        }
    }
    if let WheelDesignerDrag::InnerArc { slice } = drag {
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let r = (pos - center).length();
                if let Some(s) = level.get_mut(*slice) {
                    s.inner =
                        inner_from_radius(r, hub, outer, inner_ceiling, global_deadzone);
                }
            }
        }
    }
    if let WheelDesignerDrag::Wedge { target, .. } = drag {
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let v = pos - center;
                let view = build_view(level, meta);
                if let Some(seat) =
                    super::super::gamepad::seat_index_at_angle(v.x, -v.y, &view.layout)
                {
                    if seat < level.len() {
                        *target = seat;
                    }
                }
            }
        }
    }
    if response.drag_stopped() {
        if let WheelDesignerDrag::Divider { start_dirty: true, .. } = drag {
            // Spans save with the Save button, but `start` lives in the
            // wheel meta, which saves on change — flush the rotation now.
            *meta_save = Some((wheel_name.to_string(), meta.clone()));
        }
        if let WheelDesignerDrag::Wedge { slice, target } = *drag {
            if let Some(at) = move_slice(level, slice, target) {
                *selected_slice = Some(at);
            }
        }
        *drag = WheelDesignerDrag::None;
    }

    // Cursor affordances: grabbing during a live drag, a grab hand over
    // anything a drag would take hold of — a divider, a floor arc, or a
    // real wedge body (which drags to reorder).
    if *drag != WheelDesignerDrag::None {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if let Some(pos) = response.hover_pos() {
        let on_handle = grab_handle(level, meta, pos) != WheelDesignerDrag::None;
        let on_wedge = {
            let v = pos - center;
            let r = v.length();
            r >= hub
                && r <= outer
                && super::super::gamepad::seat_index_at_angle(v.x, -v.y, &build_view(level, meta).layout)
                    .is_some_and(|s| s < level.len())
        };
        if on_handle || on_wedge {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    let bg = ui.visuals().window_fill.gamma_multiply(0.92);
    painter.circle_filled(center, outer, bg);
    painter.circle_stroke(
        center,
        outer,
        egui::Stroke::new(1.0, ui.visuals().window_stroke.color),
    );

    if level.is_empty() {
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            "no slices yet — + Add slice below",
            egui::FontId::proportional(13.0),
            ui.visuals().weak_text_color(),
        );
    } else {
        let view = build_view(level, meta);
        super::super::gamepad::paint_wheel_ring(
            &painter,
            ui.visuals(),
            center,
            outer,
            hub,
            label_radius,
            &view.slices,
            &view.layout,
            *selected_slice,
            global_deadzone,
        );
        painter.circle_filled(center, hub, bg);
        // Hub count only when the ring has room — on a cramped canvas it
        // collides with the labels.
        if side >= 300.0 {
            let count = if level.len() == 1 {
                "1 slice".to_string()
            } else {
                format!("{} slices", level.len())
            };
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                count,
                egui::FontId::proportional(12.0),
                ui.visuals().weak_text_color(),
            );
        }

        // ----- Designer-only overlays (the live wheel draws none of
        // these): affordances for what the invisible hit zones grab. -----
        let real_len = level.len();
        let has_ghost = view.layout.len() > real_len;
        let weak = ui.visuals().weak_text_color();
        let handle_fill = ui.visuals().extreme_bg_color;

        // Dashed guide ring at the global dead zone — the default aim
        // floor every slice without its own `inner` sits on.
        let dz_r = hub + (outer - hub) * global_deadzone;
        let guide: Vec<egui::Pos2> = (0..=72)
            .map(|k| {
                let a = k as f32 / 72.0 * std::f32::consts::TAU;
                center + egui::vec2(a.cos(), a.sin()) * dz_r
            })
            .collect();
        painter.extend(egui::Shape::dashed_line(
            &guide,
            egui::Stroke::new(1.0, weak.gamma_multiply(0.5)),
            4.0,
            6.0,
        ));

        // A dot on the rim end of every draggable divider (the ghost
        // Back's edges are the runtime's, and a locked slice's edges
        // can't trade width, so those get none)...
        let draggable = |b: usize| {
            let structural = if has_ghost { b + 1 < real_len } else { real_len >= 2 };
            structural && !level[b].locked && !level[(b + 1) % real_len].locked
        };
        for (b, seat) in view.layout.seats.iter().enumerate() {
            if !draggable(b) {
                continue;
            }
            let a = (seat.start_deg + seat.span_deg).to_radians()
                - std::f32::consts::FRAC_PI_2;
            let pos = center + egui::vec2(a.cos(), a.sin()) * outer;
            painter.circle(
                pos,
                4.0,
                handle_fill,
                egui::Stroke::new(1.5, ui.visuals().window_stroke.color),
            );
        }
        // ...and one on each real slice's floor arc — its own `inner`
        // (warn-colored, like the arc), or the default floor, where
        // dragging creates an override.
        for (i, slice) in level.iter().enumerate() {
            let seat = &view.layout.seats[i];
            let floor = slice
                .inner
                .map(|p| p as f32 / 100.0)
                .unwrap_or(global_deadzone);
            let r_f = hub + (outer - hub) * floor;
            let a = seat.center_deg().to_radians() - std::f32::consts::FRAC_PI_2;
            let pos = center + egui::vec2(a.cos(), a.sin()) * r_f;
            let ring = if slice.inner.is_some() {
                ui.visuals().warn_fg_color
            } else {
                ui.visuals().window_stroke.color
            };
            painter.circle(pos, 3.0, handle_fill, egui::Stroke::new(1.5, ring));
        }

        // Width (and floor) annotation under each label, room permitting —
        // the ghost Back gets its width too, so the whole ring reads.
        if side >= 300.0 {
            for (i, seat) in view.layout.seats.iter().enumerate() {
                let a = seat.center_deg().to_radians() - std::f32::consts::FRAC_PI_2;
                let pos = center + egui::vec2(a.cos(), a.sin()) * label_radius
                    + egui::vec2(0.0, 13.0);
                let mut text = format!("{:.0}°", seat.span_deg);
                if let Some(p) = view.slices.get(i).and_then(|s| s.inner) {
                    text.push_str(&format!(" · in {p}%"));
                }
                if level.get(i).is_some_and(|s| s.locked) {
                    text.push_str(" · lock");
                }
                painter.text(
                    pos,
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(10.0),
                    weak,
                );
            }
        }

        // Wedge-move feedback: outline the seat the dragged slice will
        // land on.
        if let WheelDesignerDrag::Wedge { slice, target } = *drag {
            if slice != target {
                if let Some(seat) = view.layout.seats.get(target) {
                    let a0 = seat.start_deg.to_radians() - std::f32::consts::FRAC_PI_2;
                    let a1 = (seat.start_deg + seat.span_deg).to_radians()
                        - std::f32::consts::FRAC_PI_2;
                    let pts: Vec<egui::Pos2> = (0..=16)
                        .map(|k| {
                            let a = a0 + (a1 - a0) * k as f32 / 16.0;
                            center + egui::vec2(a.cos(), a.sin()) * (outer + 4.0)
                        })
                        .collect();
                    painter.add(egui::Shape::line(
                        pts,
                        egui::Stroke::new(3.0, ui.visuals().selection.stroke.color),
                    ));
                }
            }
        }

        // Click on a wedge selects it; the hub, the rim's outside, and the
        // ghost Back seat clear the selection.
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let v = pos - center;
                let r = v.length();
                *selected_slice = if r >= hub && r <= outer {
                    super::super::gamepad::seat_index_at_angle(v.x, -v.y, &view.layout)
                        .filter(|&s| s < level.len())
                } else {
                    None
                };
            }
        }
        // Double-click descends into a folder wedge; on the Back ghost it
        // ascends — the wheel's own navigation, mirrored.
        if response.double_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let v = pos - center;
                let r = v.length();
                if r >= hub && r <= outer {
                    match super::super::gamepad::seat_index_at_angle(v.x, -v.y, &view.layout)
                    {
                        // Explicit Back first — like the runtime, back
                        // wins over folder-ness.
                        Some(s) if s < level.len() && level[s].back => {
                            designer_path.pop();
                            *selected_slice = None;
                        }
                        Some(s) if s < level.len() && level[s].is_folder() => {
                            designer_path.push(s);
                            *selected_slice = None;
                        }
                        Some(s) if s >= level.len() => {
                            designer_path.pop();
                            *selected_slice = None;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Ring tools: whole-level operations.
    ui.horizontal(|ui| {
        if ui
            .button("Even out")
            .on_hover_text(
                "Give every unlocked slice an even share of the leftover. \
                 Locked slices keep their exact width.",
            )
            .clicked()
        {
            even_out_unlocked(level);
        }
        if ui
            .button("Mirror")
            .on_hover_text("Flip this level left↔right; every slice keeps its width.")
            .clicked()
        {
            mirror_slices(level);
            if at_top {
                // Seat 0's center mirrors too: start → −start.
                let flipped = (-meta.start.unwrap_or(0.0)).rem_euclid(360.0);
                let new = if flipped.abs() < 1e-4 { None } else { Some(flipped) };
                if new != meta.start {
                    meta.start = new;
                    *meta_save = Some((wheel_name.to_string(), meta.clone()));
                }
            }
            // Indices moved: slice 0 kept its seat, the tail reversed.
            if let Some(sel) = *selected_slice {
                if sel != 0 && sel < level.len() {
                    *selected_slice = Some(level.len() - sel);
                }
            }
        }
        // Add/Remove Back — a folder level only (the top ring has no
        // parent to ascend to). An explicit Back replaces the synthesized
        // ghost with a real, movable, resizable seat.
        if !at_top {
            let has_back = level.iter().any(|s| s.back);
            if has_back {
                if ui
                    .button("Remove Back")
                    .on_hover_text(
                        "Delete the explicit Back slice. The auto Back \
                         reappears at the anchor from the Tuning tab.",
                    )
                    .clicked()
                {
                    level.retain(|s| !s.back);
                    *selected_slice = None;
                }
            } else if ui
                .button("Add Back")
                .on_hover_text(
                    "Add a real Back slice you can move, resize, and \
                     color. It replaces the auto Back ghost; dwelling it \
                     still ascends a level.",
                )
                .clicked()
            {
                level.push(WheelSlice {
                    label: "◂ Back".to_string(),
                    back: true,
                    ..Default::default()
                });
                *selected_slice = Some(level.len() - 1);
            }
        }
        if at_top {
            let mut rotate = 0.0f32;
            if ui
                .button("-15°")
                .on_hover_text(
                    "Rotate the whole ring 15° counter-clockwise (adjusts the \
                     wheel's Start).",
                )
                .clicked()
            {
                rotate = -15.0;
            }
            if ui
                .button("+15°")
                .on_hover_text(
                    "Rotate the whole ring 15° clockwise (adjusts the wheel's \
                     Start).",
                )
                .clicked()
            {
                rotate = 15.0;
            }
            if rotate != 0.0 {
                let norm = (meta.start.unwrap_or(0.0) + rotate).rem_euclid(360.0);
                meta.start = if norm.abs() < 1e-4 { None } else { Some(norm) };
                *meta_save = Some((wheel_name.to_string(), meta.clone()));
            }
        }
    });

    // Directed mirrors (top level only): keep the named half, replace the
    // other with its reflection — the reference designer's mirror.
    if at_top && !level.is_empty() {
        ui.horizontal(|ui| {
            let mirrors = [
                // ▸ not →: the basic Arrows block (U+2190..) is missing from
                // the bundled fallback fonts and renders as tofu.
                ("Left ▸ right", MirrorKeep::Left),
                ("Right ▸ left", MirrorKeep::Right),
                ("Top ▸ bottom", MirrorKeep::Top),
                ("Bottom ▸ top", MirrorKeep::Bottom),
            ];
            for (label, keep) in mirrors {
                if ui
                    .button(label)
                    .on_hover_text(
                        "Keep that half of the ring and replace the opposite half \
                         with its mirror image (slices are cloned). A slice \
                         crossing the axis isn't cut — it becomes symmetric \
                         about it.",
                    )
                    .clicked()
                {
                    let view = build_view(level, meta);
                    if let Some((mirrored, new_start)) =
                        mirror_half(level, &view.layout, keep)
                    {
                        *level = mirrored;
                        let norm = new_start.rem_euclid(360.0);
                        let new = if norm.abs() < 1e-4 { None } else { Some(norm) };
                        if new != meta.start {
                            meta.start = new;
                            *meta_save = Some((wheel_name.to_string(), meta.clone()));
                        }
                        *selected_slice = None;
                    }
                }
            }
        });
    }

    // Selected-slice panel: the shared field widgets plus the structural
    // buttons the numeric rows offer.
    if selected_slice.is_some_and(|i| i >= level.len()) {
        *selected_slice = None;
    }
    match *selected_slice {
        Some(i) => {
            let resolved_width = build_view(level, meta).layout.seats[i].span_deg;
            let mut slice_path = designer_path.clone();
            slice_path.push(i);
            ui.horizontal(|ui| {
                render_slice_fields(ui, &mut level[i], inner_ceiling);
                let mut locked = level[i].locked;
                if ui
                    .checkbox(&mut locked, "lock")
                    .on_hover_text(
                        "Lock this slice's width: freezes it at its current \
                         degrees and Even out leaves it alone. Unlocking keeps \
                         the width; Even out returns it to the automatic share.",
                    )
                    .changed()
                {
                    level[i].locked = locked;
                    if locked {
                        level[i].span = Some(resolved_width);
                    }
                }
                if ui.small_button("^").on_hover_text("Move up").clicked() {
                    ops.push(WheelOp::MoveUp(slice_path.clone()));
                }
                if !level[i].back
                    && ui
                        .small_button("+sub")
                        .on_hover_text("Add a child slice (makes this a folder)")
                        .clicked()
                {
                    ops.push(WheelOp::AddChild(slice_path.clone()));
                }
                if ui.small_button("X").on_hover_text("Delete").clicked() {
                    ops.push(WheelOp::Delete(slice_path.clone()));
                    *selected_slice = None;
                }
            });
        }
        None => {
            ui.weak("Click a wedge to edit it · drag a wedge to reorder.");
        }
    }
    if ui.button("+ Add slice").clicked() {
        // The new slice lands at the end of this level; select it so its
        // fields open immediately.
        *selected_slice = Some(level.len());
        ops.push(WheelOp::AddChild(designer_path.clone()));
    }
}

/// Freeze every real slice at this level to its resolved width in the
/// display layout (auto seats become explicit spans). Run once when a
/// divider drag starts, so the drag trades concrete numbers across the
/// whole ring instead of fighting the auto re-split; the numeric list
/// shows the same frozen values live. The zip stops at the real slices,
/// so a ghost Back seat stays auto — its width remains the remainder,
/// exactly as at runtime — and the ring's geometry is unchanged by the
/// freeze itself.
fn materialize_spans(
    level: &mut [WheelSlice],
    layout: &super::super::gamepad::ResolvedLayout,
) {
    for (slice, seat) in level.iter_mut().zip(&layout.seats) {
        slice.span = Some(seat.span_deg);
    }
}

/// Even out the ring, honoring locks: every unlocked slice returns to the
/// automatic even split of whatever the locked slices leave over. (The
/// reference designer's rule — "evening out only respaces the gaps
/// between locked slices" — expressed in span-list form.)
fn even_out_unlocked(level: &mut [WheelSlice]) {
    for s in level.iter_mut() {
        if !s.locked {
            s.span = None;
        }
    }
}

/// Move the slice at `from` so it occupies the seat at `to`, shifting the
/// slices between them by one. Widths, floors, colors, locks, and folder
/// contents travel with the moved slice. Returns the slice's new index, or
/// None when the move is a no-op or out of range. The single reorder used
/// by the designer's drag-to-arrange so the behavior is unit-testable.
fn move_slice(level: &mut Vec<WheelSlice>, from: usize, to: usize) -> Option<usize> {
    if from == to || from >= level.len() {
        return None;
    }
    let moved = level.remove(from);
    let at = to.min(level.len());
    level.insert(at, moved);
    Some(at)
}

/// Mirror a ring left↔right (angle → −angle). Slice 0 keeps its seat —
/// its center sits on the start axis, which the caller flips separately
/// at the top level — and the clockwise tail becomes the
/// counter-clockwise tail, so everything after slice 0 reverses in place.
/// Widths and floors travel with their slices; applying twice is the
/// identity.
fn mirror_slices(level: &mut [WheelSlice]) {
    if level.len() > 1 {
        level[1..].reverse();
    }
}

/// Which half of the ring a directed mirror keeps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MirrorKeep {
    Left,
    Right,
    Top,
    Bottom,
}

impl MirrorKeep {
    /// The two angles (aim convention) where the mirror axis meets the
    /// ring: the vertical axis (0/180) for left/right keeps, the
    /// horizontal one (90/270) for top/bottom.
    fn axis_points(self) -> [f32; 2] {
        match self {
            MirrorKeep::Left | MirrorKeep::Right => [0.0, 180.0],
            MirrorKeep::Top | MirrorKeep::Bottom => [90.0, 270.0],
        }
    }

    /// Reflect an angle across the mirror axis.
    fn reflect(self, deg: f32) -> f32 {
        match self {
            MirrorKeep::Left | MirrorKeep::Right => (-deg).rem_euclid(360.0),
            MirrorKeep::Top | MirrorKeep::Bottom => (180.0 - deg).rem_euclid(360.0),
        }
    }

    /// Is an angle strictly inside the kept half (the axis itself is
    /// neither side — axis-crossing slices are handled separately)?
    fn keeps(self, deg: f32) -> bool {
        let d = deg.rem_euclid(360.0);
        match self {
            MirrorKeep::Right => d > 0.0 && d < 180.0,
            MirrorKeep::Left => d > 180.0 && d < 360.0,
            MirrorKeep::Top => !(90.0..=270.0).contains(&d),
            MirrorKeep::Bottom => d > 90.0 && d < 270.0,
        }
    }
}

/// The reference designer's directed mirror: keep the named half and
/// replace the opposite half with its reflection. Kept slices project
/// mirrored clones (labels, commands, folders and all) onto the other
/// side; a slice whose wedge crosses the axis isn't cut — it recenters ON
/// the axis, symmetric about it; the rest are dropped. The result is
/// scaled uniformly to close the ring (which preserves the symmetry) and
/// anchored at the axis, so the mirror is exact after normalization.
/// Returns the new slice list (all spans explicit) plus the ring's new
/// `start`; None only for an empty level.
fn mirror_half(
    level: &[WheelSlice],
    layout: &super::super::gamepad::ResolvedLayout,
    keep: MirrorKeep,
) -> Option<(Vec<WheelSlice>, f32)> {
    use super::super::gamepad::angular_gap;
    let [p0, p1] = keep.axis_points();

    // The surviving seats as (center, width, slice), pre-normalization.
    let mut seats: Vec<(f32, f32, WheelSlice)> = Vec::new();
    for (slice, seat) in level.iter().zip(&layout.seats) {
        let center = seat.center_deg().rem_euclid(360.0);
        let w = seat.span_deg;
        let g0 = angular_gap(center, p0);
        let g1 = angular_gap(center, p1);
        if g0 < w / 2.0 || g1 < w / 2.0 {
            // Crosses the axis: becomes symmetric about the nearer point.
            let p = if g0 <= g1 { p0 } else { p1 };
            seats.push((p, w, slice.clone()));
        } else if keep.keeps(center) {
            seats.push((center, w, slice.clone()));
            seats.push((keep.reflect(center), w, slice.clone()));
        }
    }
    if seats.is_empty() {
        return None;
    }

    // Lay the symmetric set back out: sort clockwise from the axis and
    // anchor there — a seat centered on the axis keeps its center on it,
    // otherwise a boundary sits exactly on it. Uniform scaling keeps the
    // palindrome (and thus the symmetry) intact.
    seats.sort_by(|a, b| {
        (a.0 - p0)
            .rem_euclid(360.0)
            .total_cmp(&(b.0 - p0).rem_euclid(360.0))
    });
    let total: f32 = seats.iter().map(|s| s.1).sum();
    let scale = 360.0 / total;
    let first_on_axis = angular_gap(seats[0].0, p0) < 1e-3;
    let mut edge = if first_on_axis {
        p0 - seats[0].1 * scale / 2.0
    } else {
        p0
    };
    let mut out = Vec::with_capacity(seats.len());
    let mut start = 0.0;
    for (i, (_, w, mut slice)) in seats.into_iter().enumerate() {
        let w = w * scale;
        if i == 0 {
            start = edge + w / 2.0;
        }
        slice.span = Some(w);
        out.push(slice);
        edge += w;
    }
    Some((out, start.rem_euclid(360.0)))
}

/// A dragged floor radius → the slice's `inner` value: percent of full
/// deflection from hub to rim, clamped to 5..=`ceiling` (the same ceiling
/// the numeric field enforces). `None` — use the global dead zone — when
/// the floor isn't deeper than the default, so dragging the arc back down
/// erases the override instead of freezing it at the dead-zone value.
fn inner_from_radius(
    r: f32,
    hub: f32,
    outer: f32,
    ceiling: u8,
    global_deadzone: f32,
) -> Option<u8> {
    if outer <= hub {
        return None;
    }
    let frac = ((r - hub) / (outer - hub)).clamp(0.0, 1.0);
    let pct = (frac * 100.0).round().clamp(5.0, ceiling as f32);
    if pct <= global_deadzone * 100.0 + 1e-3 {
        None
    } else {
        Some(pct as u8)
    }
}

/// Move the divider between seat `boundary` and the next seat (wrapping)
/// toward the absolute aim angle `new_deg`, trading width between the two
/// so the ring stays exactly 360. Both seats are clamped at
/// `WHEEL_MIN_SPAN_DEG`, which also caps how far the divider can travel.
///
/// `widths` must be fully materialized (see `materialize_spans`). Returns
/// the ring's start angle after the drag: `resolve_spans` centers seat 0
/// at `start`, so a trade that changes seat 0's width would shift every
/// boundary by half the delta — rotating `start` by `d/2` cancels that,
/// leaving the dragged divider tracking the pointer and every other
/// divider pinned. Trades not touching seat 0 return `start_deg`
/// unchanged.
fn apply_divider_drag(
    widths: &mut [f32],
    start_deg: f32,
    boundary: usize,
    new_deg: f32,
) -> f32 {
    let n = widths.len();
    if n < 2 || boundary >= n {
        return start_deg;
    }
    let next = (boundary + 1) % n;

    // Current absolute angle of the dragged divider (aim convention).
    let leading = start_deg - widths[0] / 2.0;
    let current = leading + widths[..=boundary].iter().sum::<f32>();

    // Shortest signed way from the divider to the pointer, then clamped so
    // neither traded seat drops below the minimum span.
    let mut d = (new_deg - current).rem_euclid(360.0);
    if d > 180.0 {
        d -= 360.0;
    }
    d = d.clamp(
        -(widths[boundary] - WHEEL_MIN_SPAN_DEG).max(0.0),
        (widths[next] - WHEEL_MIN_SPAN_DEG).max(0.0),
    );

    widths[boundary] += d;
    widths[next] -= d;
    if boundary == 0 || next == 0 {
        start_deg + d / 2.0
    } else {
        start_deg
    }
}

#[cfg(test)]
mod designer_tests {
    use super::super::super::gamepad;
    use super::*;

    /// Absolute divider angles implied by widths + start (aim convention,
    /// normalized), for asserting which boundaries moved.
    fn divider_angles(widths: &[f32], start_deg: f32) -> Vec<f32> {
        let leading = start_deg - widths[0] / 2.0;
        let mut cum = 0.0;
        widths
            .iter()
            .map(|w| {
                cum += w;
                (leading + cum).rem_euclid(360.0)
            })
            .collect()
    }

    fn slice(span: Option<f32>) -> WheelSlice {
        WheelSlice { span, ..Default::default() }
    }

    fn labeled(name: &str) -> WheelSlice {
        WheelSlice { label: name.to_string(), ..Default::default() }
    }

    #[test]
    fn move_slice_carries_the_slice_and_reports_its_new_index() {
        let mut level = vec![labeled("a"), labeled("b"), labeled("c"), labeled("d")];
        // Drag "a" (0) onto seat 2: b, c shift left, a lands at 2.
        assert_eq!(move_slice(&mut level, 0, 2), Some(2));
        let order: Vec<&str> = level.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(order, vec!["b", "c", "a", "d"]);
    }

    #[test]
    fn move_slice_backwards_and_noop_and_bounds() {
        let mut level = vec![labeled("a"), labeled("b"), labeled("c")];
        assert_eq!(move_slice(&mut level, 2, 0), Some(0));
        let order: Vec<&str> = level.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(order, vec!["c", "a", "b"]);
        // Same seat is a no-op; out-of-range is rejected.
        assert_eq!(move_slice(&mut level, 1, 1), None);
        assert_eq!(move_slice(&mut level, 9, 0), None);
    }

    #[test]
    fn move_slice_preserves_an_explicit_back_flag() {
        let mut back = labeled("◂ Back");
        back.back = true;
        let mut level = vec![labeled("a"), back, labeled("b")];
        // Move Back from the middle to the front.
        assert_eq!(move_slice(&mut level, 1, 0), Some(0));
        assert!(level[0].back, "back flag rides along");
        assert_eq!(level[0].label, "◂ Back");
    }

    #[test]
    fn materialize_freezes_autos_at_resolved_widths() {
        let mut level = vec![slice(None), slice(None), slice(Some(120.0)), slice(None)];
        let spans: Vec<Option<f32>> = level.iter().map(|s| s.span).collect();
        let layout = gamepad::resolve_spans(&spans, 0.0);
        materialize_spans(&mut level, &layout);
        let spans: Vec<f32> = level.iter().map(|s| s.span.unwrap()).collect();
        assert_eq!(spans, vec![80.0, 80.0, 120.0, 80.0]);
        assert!((spans.iter().sum::<f32>() - 360.0).abs() < 1e-3);
    }

    #[test]
    fn folder_materialize_leaves_the_ghost_back_auto() {
        // A 3-slice folder ring displays 4 seats (ghost Back appended).
        let mut level = vec![slice(None), slice(None), slice(None)];
        let view = gamepad::WheelView::build(&level, true, "down", 0.0);
        assert_eq!(view.layout.len(), 4);
        materialize_spans(&mut level, &view.layout);
        let spans: Vec<f32> = level.iter().map(|s| s.span.unwrap()).collect();
        assert_eq!(spans, vec![90.0, 90.0, 90.0]);
        // The freeze must not move anything: rebuilding reproduces the
        // identical ring, Back still owning the remainder at its anchor.
        let after = gamepad::WheelView::build(&level, true, "down", 0.0);
        assert_eq!(after.layout.seats, view.layout.seats);
    }

    #[test]
    fn drag_trades_width_between_the_two_seats_only() {
        // Even 4-ring, start 0: dividers at 45/135/225/315. Drag the one
        // after seat 1 (135°) to 150°.
        let mut w = vec![90.0, 90.0, 90.0, 90.0];
        let start = apply_divider_drag(&mut w, 0.0, 1, 150.0);
        assert_eq!(start, 0.0);
        assert_eq!(w, vec![90.0, 105.0, 75.0, 90.0]);
        // Only the dragged divider moved.
        assert_eq!(divider_angles(&w, start), vec![45.0, 150.0, 225.0, 315.0]);
    }

    #[test]
    fn drag_clamps_at_min_span_both_directions() {
        // Shrinking the next seat stops at the 30° floor...
        let mut w = vec![90.0, 90.0, 90.0, 90.0];
        apply_divider_drag(&mut w, 0.0, 1, 210.0);
        assert_eq!(w, vec![90.0, 150.0, 30.0, 90.0]);
        // ...and shrinking the dragged seat stops there too.
        let mut w = vec![90.0, 90.0, 90.0, 90.0];
        apply_divider_drag(&mut w, 0.0, 1, 60.0);
        assert_eq!(w, vec![90.0, 30.0, 150.0, 90.0]);
    }

    #[test]
    fn seat_zero_trades_rotate_start_to_pin_other_dividers() {
        // Wrap divider (after the last seat, at 315°) dragged clockwise to
        // 325°: last seat grows, seat 0 shrinks, start absorbs half the
        // delta so dividers 0..2 stay put.
        let mut w = vec![90.0, 90.0, 90.0, 90.0];
        let start = apply_divider_drag(&mut w, 0.0, 3, 325.0);
        assert!((start - 5.0).abs() < 1e-4);
        assert_eq!(w, vec![80.0, 90.0, 90.0, 100.0]);
        let angles = divider_angles(&w, start);
        assert!((angles[0] - 45.0).abs() < 1e-3);
        assert!((angles[1] - 135.0).abs() < 1e-3);
        assert!((angles[2] - 225.0).abs() < 1e-3);
        assert!((angles[3] - 325.0).abs() < 1e-3);

        // Divider after seat 0 (45°) dragged to 55°: seat 0 grows.
        let mut w = vec![90.0, 90.0, 90.0, 90.0];
        let start = apply_divider_drag(&mut w, 0.0, 0, 55.0);
        assert!((start - 5.0).abs() < 1e-4);
        assert_eq!(w, vec![100.0, 80.0, 90.0, 90.0]);
        let angles = divider_angles(&w, start);
        assert!((angles[0] - 55.0).abs() < 1e-3);
        assert!((angles[1] - 135.0).abs() < 1e-3);
        assert!((angles[3] - 315.0).abs() < 1e-3);
    }

    #[test]
    fn drag_takes_the_shortest_way_across_the_wrap() {
        // Widths [80,90,90,100] with start 35 put the wrap divider at
        // 355°; dragging it to 5° must move +10° clockwise, not −350°.
        let mut w = vec![80.0, 90.0, 90.0, 100.0];
        let start = apply_divider_drag(&mut w, 35.0, 3, 5.0);
        assert_eq!(w, vec![70.0, 90.0, 90.0, 110.0]);
        assert!((start - 40.0).abs() < 1e-4);
    }

    fn named(label: &str, span: Option<f32>) -> WheelSlice {
        WheelSlice {
            label: label.to_string(),
            span,
            ..Default::default()
        }
    }

    #[test]
    fn directed_mirror_keeps_left_and_projects_it_right() {
        // Even 4-ring a(0) b(90) c(180) d(270), keep Left: a and c cross
        // the axis and recenter on it, d projects a clone to 90, b (right
        // half) is replaced by it.
        let level = vec![named("a", None), named("b", None), named("c", None), named("d", None)];
        let layout = gamepad::resolve_spans(&[None, None, None, None], 0.0);
        let (out, start) = mirror_half(&level, &layout, MirrorKeep::Left).unwrap();
        let labels: Vec<&str> = out.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "d", "c", "d"]);
        assert!(start.abs() < 1e-3);
        for s in &out {
            assert!((s.span.unwrap() - 90.0).abs() < 1e-3);
        }
    }

    #[test]
    fn directed_mirror_rescales_and_stays_symmetric() {
        // Widths [60,120,90,90] from start 0 → centers 0/90/195/285.
        // Keep Right: a crosses the top axis point, c (center 195) crosses
        // the bottom one and recenters at 180, b is kept and cloned to the
        // left, d is replaced. Total 390 → uniform rescale closes the ring
        // without breaking the symmetry.
        let level = vec![
            named("a", Some(60.0)),
            named("b", Some(120.0)),
            named("c", Some(90.0)),
            named("d", Some(90.0)),
        ];
        let layout =
            gamepad::resolve_spans(&[Some(60.0), Some(120.0), Some(90.0), Some(90.0)], 0.0);
        let (out, start) = mirror_half(&level, &layout, MirrorKeep::Right).unwrap();
        let labels: Vec<&str> = out.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "b", "c", "b"]);
        assert!(start.abs() < 1e-3);
        let spans: Vec<f32> = out.iter().map(|s| s.span.unwrap()).collect();
        assert!((spans.iter().sum::<f32>() - 360.0).abs() < 1e-2);
        // The two b wedges mirror each other exactly.
        assert!((spans[1] - spans[3]).abs() < 1e-3);
        // Proportions kept: b is still twice a's width... (120/60 = 2).
        assert!((spans[1] / spans[0] - 2.0).abs() < 1e-3);
    }

    #[test]
    fn directed_mirror_single_slice_and_empty() {
        // A one-slice ring covers the axis and survives unchanged.
        let level = vec![named("solo", None)];
        let layout = gamepad::resolve_spans(&[None], 0.0);
        let (out, start) = mirror_half(&level, &layout, MirrorKeep::Top).unwrap();
        assert_eq!(out.len(), 1);
        assert!((out[0].span.unwrap() - 360.0).abs() < 1e-3);
        assert!(start.abs() < 1e-3 || (start - 90.0).abs() < 1e-3);
        // Empty rings have nothing to mirror.
        let layout = gamepad::resolve_spans(&[], 0.0);
        assert!(mirror_half(&[], &layout, MirrorKeep::Left).is_none());
    }

    #[test]
    fn even_out_spares_locked_slices() {
        let mut level = vec![slice(Some(120.0)), slice(Some(90.0)), slice(Some(150.0))];
        level[0].locked = true;
        even_out_unlocked(&mut level);
        // The locked width survives; the rest go back to auto and split
        // the leftover evenly when resolved.
        assert_eq!(level[0].span, Some(120.0));
        assert_eq!(level[1].span, None);
        assert_eq!(level[2].span, None);
        let spans: Vec<Option<f32>> = level.iter().map(|s| s.span).collect();
        let layout = gamepad::resolve_spans(&spans, 0.0);
        assert_eq!(layout.seats[1].span_deg, 120.0);
        assert_eq!(layout.seats[2].span_deg, 120.0);
    }

    #[test]
    fn mirror_keeps_slice_zero_and_reverses_the_tail() {
        let mk = |label: &str, span| WheelSlice {
            label: label.to_string(),
            span,
            ..Default::default()
        };
        let mut level = vec![
            mk("a", Some(100.0)),
            mk("b", None),
            mk("c", Some(50.0)),
            mk("d", None),
        ];
        mirror_slices(&mut level);
        let labels: Vec<&str> = level.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "d", "c", "b"]);
        // Widths travel with their slices.
        assert_eq!(level[2].span, Some(50.0));
        // Mirroring twice is the identity.
        mirror_slices(&mut level);
        let labels: Vec<&str> = level.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn inner_radius_maps_hub_to_rim_onto_percent() {
        // Ring from r=40 to r=240: 60% of the way out with dead zone 35%.
        assert_eq!(inner_from_radius(160.0, 40.0, 240.0, 70, 0.35), Some(60));
        // Not deeper than the default floor → clears the override.
        assert_eq!(inner_from_radius(100.0, 40.0, 240.0, 70, 0.35), None);
        // Clamped to the same ceiling the numeric field enforces.
        assert_eq!(inner_from_radius(239.0, 40.0, 240.0, 70, 0.35), Some(70));
        // Tiny throws floor at 5% (kept when the dead zone is lower).
        assert_eq!(inner_from_radius(41.0, 40.0, 240.0, 70, 0.03), Some(5));
        // Degenerate geometry never produces a floor.
        assert_eq!(inner_from_radius(50.0, 40.0, 40.0, 70, 0.35), None);
    }

    #[test]
    fn degenerate_rings_are_untouched() {
        let mut w = vec![360.0];
        assert_eq!(apply_divider_drag(&mut w, 0.0, 0, 90.0), 0.0);
        assert_eq!(w, vec![360.0]);
        let mut w = vec![180.0, 180.0];
        assert_eq!(apply_divider_drag(&mut w, 0.0, 5, 90.0), 0.0);
        assert_eq!(w, vec![180.0, 180.0]);
    }
}
