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
    Wheels,
    Rumble,
    Tuning,
}

/// What a controller binding is. `Modifier` declares the button as a member
/// of the held-modifier set (no action of its own, no Value, no Modifier
/// dropdowns); `Macro`/`Action` are ordinary press bindings that may carry
/// up to two modifiers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BindingType {
    Macro,
    Action,
    Modifier,
}

/// The action string a modifier button carries in `[controller]`.
const MODIFIER_ACTION: &str = "controller_modifier";
/// Placeholder shown in the Modifier dropdowns for "no modifier".
const MODIFIER_NONE: &str = "none";

const RUMBLE_PATTERNS: [&str; 4] = ["off", "short", "long", "double"];

/// Reserved name of the dynamic portals wheel (mirrors
/// `AppCore::PORTAL_WHEEL_KEY`). It gets a permanent Wheels-tab entry that
/// edits only its button/stick meta; its slices are generated per room.
const PORTAL_WHEEL_KEY: &str = crate::core::AppCore::PORTAL_WHEEL_KEY;

/// Movement-stick choices for the Tuning tab.
const MOVEMENT_STICKS: [&str; 2] = ["left", "right"];
/// Leaf fire-mode choices for the Tuning tab (see `[controller_tuning]`).
/// Per-slice fire types for the slice-row dropdown (wheel v2). "none" is
/// the dead-zone type; the other three match the old global fire modes.
const SLICE_FIRE_TYPES: [(&str, &str); 4] = [
    ("none", "None"),
    ("release", "Release"),
    ("edge", "Edge"),
    ("retract", "Retract"),
];
const OPPOSING_STICK_ACTIONS: [&str; 2] = ["scroll", "none"];
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
    /// Two-click Delete-wheel confirm: armed by the first click (naming
    /// the opener that will be cleared), executed by the second. Any
    /// wheel switch disarms it.
    wheel_delete_armed: bool,
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
    /// Undo stack for structural wheel edits (§2a): whole-buffer snapshots
    /// plus the wheel meta's `start`, pushed before every structural op.
    /// Bounded; cleared on wheel switch. Field edits (label/command text)
    /// stay outside it by design.
    wheel_undo: Vec<(Vec<WheelSlice>, Option<f32>)>,
    /// Save scope for every write in this editor: `true` = the shared global
    /// controller.toml, `false` = the active character's override file. Load
    /// always merges character over global; this only picks where edits land,
    /// so a class can build its own pad profile without a global pad.
    is_global: bool,
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
            wheel_delete_armed: false,
            wheel_meta_buffer: None,
            wheel_view_mode: WheelViewMode::Visual,
            wheel_designer_path: Vec::new(),
            wheel_selected_slice: None,
            wheel_drag: WheelDesignerDrag::None,
            wheel_undo: Vec::new(),
            is_global: true,
        }
    }
}

/// Cap for the wheel undo stack — behind this, oldest snapshots fall off.
const WHEEL_UNDO_CAP: usize = 50;

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

/// Apply a queued structural op, honoring locks (wheel v2): on a level
/// with any locked slice, deletes go through the position-preserving
/// `ring_delete`, adds split the widest unlocked slice instead of
/// re-splitting the auto pool, and swaps touching a locked slice are
/// refused. Lock-free levels keep the legacy behavior (autos stay auto).
/// Returns a status message when an op is refused; sets `start_dirty`
/// when the wheel's `start` changed and must be saved.
fn apply_wheel_op_v2(
    slices: &mut Vec<WheelSlice>,
    op: WheelOp,
    back_anchor: &str,
    meta: &mut crate::config::WheelMeta,
    start_dirty: &mut bool,
) -> Option<String> {
    // Materialize a locked level and return the op-space start angle (the
    // view's real seat 0 center — meta.start at the top, the anchor
    // rotation in folders).
    fn prep_locked(
        level: &mut Vec<WheelSlice>,
        in_folder: bool,
        back_anchor: &str,
        start: f32,
    ) -> f32 {
        let view =
            super::super::gamepad::WheelView::build(level, in_folder, back_anchor, start);
        materialize_spans(level, &view.layout);
        view.layout
            .seats
            .first()
            .map(|s| s.center_deg())
            .unwrap_or(0.0)
    }
    // Write a top-level start change back to the meta buffer.
    fn store_start(meta: &mut crate::config::WheelMeta, new_start: f32, dirty: &mut bool) {
        let norm = new_start.rem_euclid(360.0);
        let new = if norm.abs() < 1e-4 { None } else { Some(norm) };
        if new != meta.start {
            meta.start = new;
            *dirty = true;
        }
    }

    match op {
        WheelOp::Delete(path) => {
            let (last, parent) = path.split_last()?;
            let at_top = parent.is_empty();
            let start = meta.start.unwrap_or(0.0);
            let level = wheel_slices_at(slices, parent)?;
            if *last >= level.len() {
                return None;
            }
            if level.iter().any(|s| s.locked) {
                let op_start = prep_locked(level, !at_top, back_anchor, start);
                if let Some((new_start, _)) = ring_delete(level, op_start, *last) {
                    if at_top {
                        store_start(meta, new_start, start_dirty);
                    }
                }
            } else {
                level.remove(*last);
            }
            None
        }
        WheelOp::AddChild(path) => {
            // path addresses the slice whose children gain the new entry
            // (an empty path adds a top-level slice).
            let at_top = path.is_empty();
            let start = meta.start.unwrap_or(0.0);
            let level = wheel_slices_at(slices, &path)?;
            if level.iter().any(|s| s.locked) {
                // Splitting the widest unlocked slice keeps every lock (and
                // everything else outside that wedge) exactly in place.
                let op_start = prep_locked(level, !at_top, back_anchor, start);
                let min = crate::config::WHEEL_MIN_SPAN_DEG;
                let candidate = level
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| !s.locked && s.span.unwrap_or(0.0) >= 2.0 * min)
                    .max_by(|a, b| {
                        a.1.span
                            .unwrap_or(0.0)
                            .total_cmp(&b.1.span.unwrap_or(0.0))
                    })
                    .map(|(i, _)| i);
                let Some(i) = candidate else {
                    return Some(
                        "No room for a new slice — unlock or widen one first."
                            .to_string(),
                    );
                };
                // Cut at the wedge's midpoint.
                let leading0 = op_start - level[0].span.unwrap_or(0.0) / 2.0;
                let seat_leading = leading0
                    + level[..i].iter().map(|s| s.span.unwrap_or(0.0)).sum::<f32>();
                let at = seat_leading + level[i].span.unwrap_or(0.0) / 2.0;
                if let Some((new_start, new_idx)) = ring_split(level, op_start, i, at) {
                    level[new_idx].label = "new".to_string();
                    if at_top {
                        store_start(meta, new_start, start_dirty);
                    }
                }
            } else {
                level.push(WheelSlice {
                    label: "new".to_string(),
                    ..Default::default()
                });
            }
            None
        }
        WheelOp::MoveUp(path) => {
            let (last, parent) = path.split_last()?;
            if *last == 0 {
                return None;
            }
            let level = wheel_slices_at(slices, parent)?;
            if *last >= level.len() {
                return None;
            }
            if level[*last].locked || level[*last - 1].locked {
                return Some(
                    "A locked slice holds its seat — unlock it to reorder."
                        .to_string(),
                );
            }
            level.swap(*last, *last - 1);
            None
        }
    }
}

struct ControllerFormState {
    /// Some(key) when editing an existing binding; None when adding. The key
    /// is the canonical bind key (bare button or composite `l2+dpad_down`).
    original_key: Option<String>,
    button: String,
    capture_armed: bool,
    binding_type: BindingType,
    /// The first / second modifier button (or `MODIFIER_NONE`). Ignored when
    /// binding_type is Modifier. Modifier 2 is forced to none while Modifier
    /// 1 is none (no gaps); a button chosen in one is hidden from the other.
    modifier1: String,
    modifier2: String,
    action: String,
    macro_text: String,
    error: Option<String>,
}

impl ControllerFormState {
    fn empty() -> Self {
        Self {
            original_key: None,
            button: String::new(),
            capture_armed: false,
            binding_type: BindingType::Macro,
            modifier1: MODIFIER_NONE.to_string(),
            modifier2: MODIFIER_NONE.to_string(),
            action: String::new(),
            macro_text: String::new(),
            error: None,
        }
    }

    /// Build a form from an existing binding key + action. The key is split
    /// into its button and (up to two) modifiers; a `controller_modifier`
    /// action selects Modifier type.
    fn from_binding(key: &str, action: &KeyBindAction) -> Self {
        let parsed = crate::config::ControllerBindKey::parse(key);
        let (button, mods) = parsed
            .map(|k| (k.button, k.mods))
            .unwrap_or_else(|| (key.to_string(), Vec::new()));
        let (binding_type, action_text, macro_text) = match action {
            KeyBindAction::Action(name) if name == MODIFIER_ACTION => {
                (BindingType::Modifier, String::new(), String::new())
            }
            KeyBindAction::Action(name) => (BindingType::Action, name.clone(), String::new()),
            KeyBindAction::Macro(macro_action) => {
                (BindingType::Macro, String::new(), macro_action.macro_text.clone())
            }
        };
        let modifier1 = mods
            .first()
            .cloned()
            .unwrap_or_else(|| MODIFIER_NONE.to_string());
        let modifier2 = mods
            .get(1)
            .cloned()
            .unwrap_or_else(|| MODIFIER_NONE.to_string());
        Self {
            original_key: Some(key.to_string()),
            button,
            capture_armed: false,
            binding_type,
            modifier1,
            modifier2,
            action: action_text,
            macro_text,
            error: None,
        }
    }

    /// The chosen modifiers as a clean list (dropping `none`, deduped). Only
    /// meaningful for non-Modifier bindings; Modifier bindings never chord.
    fn chosen_modifiers(&self) -> Vec<String> {
        if self.binding_type == BindingType::Modifier {
            return Vec::new();
        }
        let mut out = Vec::new();
        for m in [&self.modifier1, &self.modifier2] {
            if m != MODIFIER_NONE && !m.is_empty() && !out.contains(m) {
                out.push(m.clone());
            }
        }
        out
    }

    /// Build the canonical bind key + action from the form. Validates the
    /// button, the modifier pairing (no gaps, no self-modifier), and the
    /// value for the chosen type.
    fn build_binding(&self) -> Result<(String, KeyBindAction), String> {
        let button = self.button.trim().to_lowercase();
        if button.is_empty() {
            return Err("Pick a button (or press Capture and tap one).".to_string());
        }
        if !super::super::gamepad::GAMEPAD_BUTTON_NAMES.contains(&button.as_str()) {
            return Err(format!("Unknown button '{}'.", button));
        }

        // Modifier declarations never carry a value or modifiers of their own.
        if self.binding_type == BindingType::Modifier {
            return Ok((button, KeyBindAction::Action(MODIFIER_ACTION.to_string())));
        }

        // No gaps: Modifier 1 = none forces Modifier 2 = none.
        if self.modifier1 == MODIFIER_NONE && self.modifier2 != MODIFIER_NONE {
            return Err("Set Modifier 1 before Modifier 2.".to_string());
        }
        let mods = self.chosen_modifiers();
        if mods.iter().any(|m| *m == button) {
            return Err("A button can't be its own modifier.".to_string());
        }

        let action = match self.binding_type {
            BindingType::Macro => {
                if self.macro_text.is_empty() {
                    return Err("Macro text is required (\\r sends enter).".to_string());
                }
                let text = self.macro_text.replace("\\r", "\r").replace("\\n", "\n");
                KeyBindAction::Macro(MacroAction { macro_text: text })
            }
            BindingType::Action => {
                let name = self.action.trim().to_string();
                if name.is_empty() {
                    return Err("Pick an action from the list.".to_string());
                }
                if KeyAction::from_str(&name).is_none() {
                    return Err(format!("Unknown action '{}'.", name));
                }
                // A wheel opens on a held bare button; the runtime never
                // resolves a modifier combo as a wheel opener.
                if name.starts_with("controller_wheel") && !mods.is_empty() {
                    return Err(
                        "A wheel opener can't require modifiers — bind it to a bare button."
                            .to_string(),
                    );
                }
                KeyBindAction::Action(name)
            }
            BindingType::Modifier => unreachable!("handled above"),
        };
        let key = crate::config::ControllerBindKey::new(button, mods).canonical();
        Ok((key, action))
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

/// One `Modifier N` combo row in the Add/Edit form: a `none` sentinel plus
/// every declared-modifier button, minus `exclude` (the button already chosen
/// in the other slot, so `l2+l2` is impossible). Ends the grid row itself.
/// `enabled` disables only the dropdown — the label and the row structure
/// stay on the grid's own `Ui`, so the grid's columns (and thus the left
/// edges of Button / Modifier 1 / Modifier 2) keep lining up.
fn render_modifier_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    pool: &[String],
    exclude: Option<&str>,
    enabled: bool,
) {
    ui.label(label);
    let id = egui::Id::new("controller_modifier_pick").with(label);
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt(id)
            .selected_text(value.as_str())
            .show_ui(ui, |ui| {
                ui.selectable_value(value, MODIFIER_NONE.to_string(), MODIFIER_NONE);
                for name in pool {
                    if exclude == Some(name.as_str()) {
                        continue;
                    }
                    ui.selectable_value(value, name.clone(), name);
                }
            });
    });
    ui.end_row();
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
        is_global: bool,
    ) -> Result<(), String> {
        let (key, action) = form.build_binding()?;

        // Wheel ↔ modifier are mutually exclusive. Block a Modifier
        // declaration on a button already assigned as a wheel button, naming
        // the conflict so the user knows what to clear first.
        if form.binding_type == BindingType::Modifier {
            if let Some(wheel) = self.wheel_key_for_button(&form.button) {
                return Err(format!(
                    "'{}' opens the {} wheel. Clear that wheel button before making it a modifier.",
                    form.button, wheel
                ));
            }
        }

        let character = self.app_core.config.character.clone();
        let character = character.as_deref();

        if let Some(original) = &form.original_key {
            // Remove the old entry when the key changed, or when its scope
            // flipped (the edit lands in the other file, so the original copy
            // must go).
            let was_char_override = self.controller_bind_is_character_override(original);
            let scope_changed = was_char_override == is_global; // override != global
            if *original != key || scope_changed {
                let orig_global = !was_char_override;
                if let Err(err) =
                    Config::delete_single_controller_bind(original, orig_global, character)
                {
                    tracing::warn!("Failed to remove old controller bind '{}': {}", original, err);
                }
            }
        }

        Config::save_single_controller_bind(&key, &action, is_global, character)
            .map_err(|err| format!("Failed to save controller bind: {}", err))?;
        self.reload_controller_binds();
        // Two-way sync: the wheel editor's "Opens with" mirrors whatever
        // this edit did to the touched buttons (assigned a wheel opener,
        // moved it, or replaced one with another action). Only bare
        // buttons can open wheels, so composite keys sync nothing.
        let mut touched: Vec<String> = Vec::new();
        if !key.contains('+') {
            touched.push(key.clone());
        }
        if let Some(original) = &form.original_key {
            if let Some(parsed) = crate::config::ControllerBindKey::parse(original) {
                if parsed.mods.is_empty() && !touched.contains(&parsed.button) {
                    touched.push(parsed.button);
                }
            }
        }
        if !touched.is_empty() {
            self.sync_wheel_meta_for_buttons(&touched, is_global);
        }
        Ok(())
    }

    /// Whether `key` currently lives in the active character's controller
    /// file (as opposed to global) — used to tag rows and route a
    /// re-save/delete at the right scope.
    fn controller_bind_is_character_override(&self, key: &str) -> bool {
        Config::load_character_controller_binds_only(self.app_core.config.character.as_deref())
            .map(|binds| binds.contains_key(key))
            .unwrap_or(false)
    }

    /// The wheel a button opens, as a human label ("default" or the wheel
    /// name), if that button is bound to a `controller_wheel*` action. Used
    /// for the wheel↔modifier exclusivity warning.
    fn wheel_key_for_button(&self, button: &str) -> Option<String> {
        match self.app_core.config.controller_binds.get(button) {
            Some(KeyBindAction::Action(name)) if name == "controller_wheel" => {
                Some("default".to_string())
            }
            Some(KeyBindAction::Action(name)) => name
                .strip_prefix("controller_wheel:")
                .map(|n| n.to_string()),
            _ => None,
        }
    }

    /// Wheel-opener actions offered in the binding dialog's Action
    /// dropdown: the default wheel plus every named wheel (slice arrays ∪
    /// meta entries ∪ the dynamic portals wheel), as
    /// `controller_wheel[:name]`. Selecting one makes the button that
    /// wheel's opener — the same relationship the wheel editor's "Opens
    /// with" edits (see `sync_wheel_meta_for_buttons`).
    fn wheel_action_names(&self) -> Vec<String> {
        let mut names: std::collections::BTreeSet<String> = self
            .app_core
            .config
            .controller_wheels
            .keys()
            .cloned()
            .collect();
        names.extend(self.app_core.config.controller_wheels_meta.keys().cloned());
        // "default" is the bare `controller_wheel` action, not a named form.
        names.remove("default");
        names.insert(PORTAL_WHEEL_KEY.to_string());
        let mut out = vec!["controller_wheel".to_string()];
        out.extend(names.into_iter().map(|n| format!("controller_wheel:{n}")));
        out
    }

    /// Re-point wheel meta after a `[controller]` edit touching `buttons`,
    /// so the binding dialog's Action selection and the wheel editor's
    /// "Opens with" stay two views of one relationship. For each touched
    /// button: a wheel whose meta claims it but that `[controller]` (the
    /// authority) no longer routes there is marked unbound — the wheel
    /// survives, shown as "(unset)", re-assignable; and the wheel the
    /// button now opens records it. Wheels/buttons outside the edit are
    /// never touched (their disagreements stay visible as warnings).
    fn sync_wheel_meta_for_buttons(&mut self, buttons: &[String], is_global: bool) {
        let character = self.app_core.config.character.clone();
        let mut map = self.app_core.config.controller_wheels_meta.clone();
        let mut changed = false;
        for button in buttons {
            // The wheel this button now opens per [controller], as a meta
            // key ("default" for the bare action), if any.
            let now_opens = self.wheel_key_for_button(button);
            for (name, meta) in map.iter_mut() {
                if meta.button.as_deref() == Some(button.as_str())
                    && now_opens.as_deref() != Some(name.as_str())
                {
                    meta.button = None;
                    changed = true;
                }
            }
            if let Some(name) = now_opens {
                let entry = map.entry(name).or_default();
                if entry.button.as_deref() != Some(button.as_str()) {
                    entry.button = Some(button.clone());
                    changed = true;
                }
            }
        }
        if changed {
            match Config::save_controller_wheels_meta(&map, is_global, character.as_deref()) {
                Ok(()) => self.app_core.config.controller_wheels_meta = map,
                Err(err) => tracing::warn!("Failed to sync wheel meta: {}", err),
            }
        }
    }

    /// True when `button` is currently declared as a modifier. Used to block
    /// assigning it as a wheel button (wheel↔modifier exclusivity).
    fn button_is_modifier(&self, button: &str) -> bool {
        matches!(
            self.app_core.config.controller_binds.get(button),
            Some(KeyBindAction::Action(name)) if name == MODIFIER_ACTION
        )
    }

    /// Buttons currently declared as modifiers, sorted canonically — the pool
    /// the form's Modifier dropdowns choose from.
    fn declared_modifier_buttons(&self) -> Vec<String> {
        let mut mods: Vec<String> = self
            .app_core
            .config
            .controller_binds
            .iter()
            .filter_map(|(btn, action)| match action {
                KeyBindAction::Action(name) if name == MODIFIER_ACTION => Some(btn.clone()),
                _ => None,
            })
            .collect();
        mods.sort_by_key(|n| {
            crate::config::CONTROLLER_BUTTON_ORDER
                .iter()
                .position(|b| b == n)
                .unwrap_or(usize::MAX)
        });
        mods
    }

    fn reload_controller_binds(&mut self) {
        let character = self.app_core.config.character.as_deref();
        self.app_core.config.controller_binds =
            Config::load_controller_binds(character).unwrap_or_default();
    }

    pub(in super::super) fn render_controller_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.controller_editor.take() else {
            return;
        };

        let mut open = true;
        let mut open_form: Option<ControllerFormState> = None;
        let mut delete_request: Option<String> = None;
        let mut wheel_save = false;
        let mut wheel_delete: Option<String> = None;
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
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(440.0)
            .default_height(380.0)
            .show(ctx, |ui| {
                if pad_connected {
                    ui.weak("Controller connected. D-pad / South / East are fixed navigation inside interact mode and menus; bindings apply outside them.");
                } else {
                    ui.weak("No controller detected — connect one and it will announce itself.");
                }
                // Save scope: Global (shared) vs this character's override
                // file. Loading always merges character over global, so a
                // class can keep only its diffs here. Character is disabled
                // when no character is active (e.g. pre-login).
                let character = self.app_core.config.character.clone();
                ui.horizontal(|ui| {
                    ui.label("Save to:");
                    ui.selectable_value(&mut state.is_global, true, "Global (all characters)");
                    let char_label = match &character {
                        Some(name) => format!("This character ({name})"),
                        None => "This character".to_string(),
                    };
                    ui.add_enabled_ui(character.is_some(), |ui| {
                        if ui
                            .selectable_label(!state.is_global, char_label)
                            .on_hover_text(
                                "Save edits to this character's controller.toml. \
                                 They override the global file for this character only.",
                            )
                            .clicked()
                        {
                            state.is_global = false;
                        }
                    });
                });
                if character.is_none() {
                    state.is_global = true;
                }
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.tab, ControllerTab::Base, "Bindings");
                    ui.selectable_value(&mut state.tab, ControllerTab::Wheels, "Wheels");
                    ui.selectable_value(&mut state.tab, ControllerTab::Rumble, "Rumble");
                    ui.selectable_value(&mut state.tab, ControllerTab::Tuning, "Tuning");
                    ui.separator();
                    if state.tab == ControllerTab::Base && ui.button("Add binding").clicked() {
                        open_form = Some(ControllerFormState::empty());
                    }
                });
                if state.tab == ControllerTab::Base {
                    ui.weak(
                        "Set a button's Type to Modifier to chord: other bindings can then \
                         require it held (e.g. l2 + dpad_down).",
                    );
                }
                ui.separator();

                if state.tab == ControllerTab::Wheels {
                    render_wheels_tab(
                        ui,
                        &mut state,
                        &self.app_core.config,
                        &mut wheel_save,
                        &mut meta_save,
                        &mut wheel_delete,
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
                        "Opposing stick",
                        "What the non-movement stick does when it is NOT aiming an open \
                         wheel. scroll: scrolls the story window and cycles interact-mode \
                         focus (the classic behaviour). none: no idle action — a stray \
                         nudge does nothing. Wheel aiming works either way.",
                        &mut tuning.opposing_stick,
                        &OPPOSING_STICK_ACTIONS,
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
                    slider_row(
                        ui,
                        "Wheel min open",
                        "A wheel stays up at least this long after the button comes up, so                          a bouncy trigger can't strobe the overlay open and closed.",
                        &mut tuning.wheel_min_open_ms,
                        0..=500,
                        " ms",
                        &mut changed,
                    );
                    ui.horizontal(|ui| {
                        ui.label("Trigger open / close").on_hover_text(
                            "Analog trigger (L2/R2) travel that counts as pressed /                              released for wheel binds. Lower the open value for a worn                              pad that never reports full pull; raise close for a hair                              trigger that idles part-way down. Close always stays below                              open (the gap is the anti-chatter hysteresis).",
                        );
                        if ui
                            .add(egui::Slider::new(&mut tuning.trigger_open_pct, 10..=100).suffix("%"))
                            .changed()
                        {
                            changed = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut tuning.trigger_close_pct, 0..=95).suffix("%"))
                            .changed()
                        {
                            changed = true;
                        }
                        if tuning.trigger_close_pct >= tuning.trigger_open_pct {
                            tuning.trigger_close_pct = tuning.trigger_open_pct.saturating_sub(5);
                            changed = true;
                        }
                    });

                    ui.separator();
                    // Fire type is chosen per slice now (the dropdown in each
                    // slice row of the Wheels tab); a slice without its own
                    // type falls back to release. These parameterize the
                    // edge/retract types wherever a slice uses them.
                    ui.weak(
                        "How a slice fires is set per slice in the Wheels tab \
                         (None / Release / Edge / Retract). The knobs below tune \
                         the edge and retract behaviours for every slice that \
                         uses them.",
                    );
                    ui.horizontal(|ui| {
                        ui.label("Edge threshold").on_hover_text(
                            "Deflection at which an edge-type slice fires. Higher means \
                             you must push the stick nearer the rim before it fires.",
                        );
                        if ui
                            .add(egui::Slider::new(&mut tuning.edge_threshold, 10..=100).suffix("%"))
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Retract delta").on_hover_text(
                            "How far the stick must pull back from its peak deflection to \
                             fire a retract-type slice. Smaller means a lighter inward \
                             flick fires.",
                        );
                        if ui
                            .add(egui::Slider::new(&mut tuning.retract_delta, 1..=50).suffix("%"))
                            .changed()
                        {
                            changed = true;
                        }
                    });

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

                let binds = &self.app_core.config.controller_binds;
                let mut entries: Vec<(&String, &KeyBindAction)> = binds.iter().collect();
                // Sort by canonical modifier order then button so composite
                // combos group under a stable order (bare binds sort first).
                entries.sort_by(|a, b| {
                    let ka = crate::config::ControllerBindKey::parse(a.0);
                    let kb = crate::config::ControllerBindKey::parse(b.0);
                    match (ka, kb) {
                        (Some(ka), Some(kb)) => ka
                            .mods
                            .len()
                            .cmp(&kb.mods.len())
                            .then_with(|| a.0.cmp(b.0)),
                        _ => a.0.cmp(b.0),
                    }
                });
                let row_count = entries.len();

                // Which keys are this character's overrides (vs global), so
                // each row can show a [C]/[G] tag. Loaded once per render, not
                // per row, to keep the file read off the hot path.
                let char_binds = Config::load_character_controller_binds_only(
                    self.app_core.config.character.as_deref(),
                )
                .unwrap_or_default();

                egui::ScrollArea::vertical()
                    .id_salt("controller_editor_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (key, action) in entries {
                            let is_modifier = matches!(
                                action,
                                KeyBindAction::Action(name) if name == MODIFIER_ACTION
                            );
                            ui.horizontal(|ui| {
                                let scope = if char_binds.contains_key(key) {
                                    "[C]"
                                } else {
                                    "[G]"
                                };
                                ui.label(egui::RichText::new(scope).weak().monospace())
                                    .on_hover_text(if scope == "[C]" {
                                        "This character's override"
                                    } else {
                                        "Global (all characters)"
                                    });
                                if ui.small_button("Edit").clicked() {
                                    open_form =
                                        Some(ControllerFormState::from_binding(key, action));
                                }
                                if ui.small_button("Delete").clicked() {
                                    delete_request = Some(key.clone());
                                }
                                // Curate the binding-legend overlay: only
                                // checked rows appear in the HUD. The entry is
                                // the canonical key (bare or composite); a
                                // modifier declaration has nothing to show.
                                if !is_modifier {
                                    let overlay_entry = key.to_string();
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
                                }
                                ui.label(egui::RichText::new(key).monospace().strong());
                                if is_modifier {
                                    ui.weak("(modifier)");
                                } else {
                                    ui.weak(display_action(action));
                                }
                            });
                        }
                        if row_count == 0 {
                            ui.weak("No controller bindings.");
                        }
                    });
            });

        // Every write in this editor targets the same scope the user picked
        // (Global vs the active character); load-time merge is unchanged.
        let scope_global = state.is_global;
        let scope_char = self.app_core.config.character.clone();
        let scope_char = scope_char.as_deref();

        if let Some(rumble) = rumble_save {
            match Config::save_controller_rumble(&rumble, scope_global, scope_char) {
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
            match Config::save_controller_tuning(&tuning, scope_global, scope_char) {
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
            if let Err(err) = Config::save_controller_wheels_meta(&map, scope_global, scope_char) {
                self.app_core
                    .add_system_message(&format!("Failed to save wheel meta: {}", err));
                ok = false;
            }
            // Setting a button also writes the matching [controller] entry
            // (the runtime authority) so the two never silently drift.
            // The inverse holds too: this wheel's action lives on exactly
            // the button meta names — a previous opener's bind (or every
            // opener, when unset) is removed, so changing "Opens with"
            // can never leave two buttons opening one wheel or a dangling
            // `controller_wheel:<name>` reference.
            if ok {
                let action_name = if name == "default" {
                    "controller_wheel".to_string()
                } else {
                    format!("controller_wheel:{}", name)
                };
                let stale: Vec<String> = self
                    .app_core
                    .config
                    .controller_binds
                    .iter()
                    .filter(|(key, bind)| {
                        matches!(bind, KeyBindAction::Action(n) if *n == action_name)
                            && meta.button.as_deref() != Some(key.as_str())
                    })
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in stale {
                    let is_char = self.controller_bind_is_character_override(&key);
                    if let Err(err) =
                        Config::delete_single_controller_bind(&key, !is_char, scope_char)
                    {
                        tracing::warn!("Failed to clear old wheel opener '{}': {}", key, err);
                    }
                }
                if let Some(button) = meta.button.as_deref() {
                    // Wheel ↔ modifier exclusivity: a wheel button can't also
                    // be a modifier. Block and name the conflict.
                    if self.button_is_modifier(button) {
                        self.app_core.add_system_message(&format!(
                            "'{}' is a modifier button. Clear its Modifier type before \
                             assigning it as a wheel button.",
                            button
                        ));
                        ok = false;
                    } else {
                        let action = if name == "default" {
                            "controller_wheel".to_string()
                        } else {
                            format!("controller_wheel:{}", name)
                        };
                        if let Err(err) = Config::save_single_controller_bind(
                            button,
                            &KeyBindAction::Action(action),
                            scope_global,
                            scope_char,
                        ) {
                            self.app_core.add_system_message(&format!(
                                "Failed to bind wheel button: {}",
                                err
                            ));
                            ok = false;
                        }
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
            match Config::save_controller_overlay(&list, scope_global, scope_char) {
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
                match Config::save_controller_wheel_named(name, &buffer, scope_global, scope_char) {
                    Ok(()) => {
                        let ch = self.app_core.config.character.as_deref();
                        self.app_core.config.controller_wheel =
                            Config::load_controller_wheel(ch).unwrap_or_default();
                        self.app_core.config.controller_wheels =
                            Config::load_controller_wheels(ch).unwrap_or_default();
                        self.app_core.push_remote_wheels();
                        // Surface any span problems in what was just saved
                        // (advisory — the runtime still produces a usable
                        // ring). The inline editor advisory is B6.
                        self.app_core.warn_wheel_span_conflicts();
                        state.wheel_status = Some(if buffer.is_empty() && name.is_some() {
                            "Saved (empty wheel — add slices, or use Delete wheel to remove it)."
                                .to_string()
                        } else {
                            "Saved.".to_string()
                        });
                    }
                    Err(err) => state.wheel_status = Some(format!("Save failed: {}", err)),
                }
            }
        }

        if let Some(name) = wheel_delete {
            // Guardrail: clear every [controller] bind that opens this wheel
            // first, so no dangling `controller_wheel:<name>` reference
            // survives the delete. Each bind is removed from whichever file
            // it actually lives in.
            let action_name = format!("controller_wheel:{}", name);
            let openers: Vec<String> = self
                .app_core
                .config
                .controller_binds
                .iter()
                .filter(|(_, bind)| {
                    matches!(bind, KeyBindAction::Action(n) if *n == action_name)
                })
                .map(|(key, _)| key.clone())
                .collect();
            for key in openers {
                let is_char = self.controller_bind_is_character_override(&key);
                if let Err(err) = Config::delete_single_controller_bind(&key, !is_char, scope_char)
                {
                    tracing::warn!("Failed to clear opener bind '{}': {}", key, err);
                }
            }
            match Config::delete_controller_wheel_named(&name, scope_global, scope_char) {
                Ok(()) => {
                    let ch = self.app_core.config.character.as_deref();
                    self.app_core.config.controller_wheels =
                        Config::load_controller_wheels(ch).unwrap_or_default();
                    self.app_core.config.controller_wheels_meta =
                        Config::load_controller_wheels_meta(ch).unwrap_or_default();
                    self.reload_controller_binds();
                    self.app_core.push_remote_wheels();
                    state.wheel_selected.clear();
                    state.wheel_buffer = None;
                    state.wheel_meta_buffer = None;
                    state.wheel_designer_path.clear();
                    state.wheel_selected_slice = None;
                    state.wheel_status = Some(format!("Wheel '{}' deleted.", name));
                }
                Err(err) => state.wheel_status = Some(format!("Delete failed: {}", err)),
            }
        }

        if let Some(key) = delete_request {
            // Delete from whichever file the bind actually lives in, so a
            // per-row Delete works whether it's a global or character bind.
            let is_char = self.controller_bind_is_character_override(&key);
            match Config::delete_single_controller_bind(&key, !is_char, scope_char) {
                Ok(()) => {
                    self.reload_controller_binds();
                    // Deleting a wheel-opener bind unbinds the wheel — the
                    // wheel definition and its slices survive, shown as
                    // "(unset)" and re-assignable from the Wheels tab.
                    if !key.contains('+') {
                        self.sync_wheel_meta_for_buttons(&[key.clone()], scope_global);
                    }
                    self.app_core
                        .add_system_message(&format!("Controller bind '{}' deleted.", key));
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
            let title = if form.original_key.is_some() {
                "Edit Controller Binding"
            } else {
                "Add Controller Binding"
            };
            egui::Window::new(title)
                .id(egui::Id::new("gui_controller_form"))
                .order(egui::Order::Foreground)
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

                            // Modifier dropdowns are hidden for a Modifier
                            // declaration (a modifier button chords nothing of
                            // its own). Shown otherwise, drawing from the pool
                            // of buttons currently declared as modifiers.
                            if form.binding_type != BindingType::Modifier {
                                let mod_pool = self.declared_modifier_buttons();
                                render_modifier_row(ui, "Modifier 1", &mut form.modifier1, &mod_pool, None, true);
                                // Modifier 1 = none forces Modifier 2 = none.
                                if form.modifier1 == MODIFIER_NONE {
                                    form.modifier2 = MODIFIER_NONE.to_string();
                                }
                                let exclude = (form.modifier1 != MODIFIER_NONE)
                                    .then(|| form.modifier1.clone());
                                render_modifier_row(
                                    ui,
                                    "Modifier 2",
                                    &mut form.modifier2,
                                    &mod_pool,
                                    exclude.as_deref(),
                                    form.modifier1 != MODIFIER_NONE,
                                );
                            }

                            ui.label("Type");
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut form.binding_type, BindingType::Macro, "Macro");
                                ui.selectable_value(&mut form.binding_type, BindingType::Action, "Action");
                                ui.selectable_value(&mut form.binding_type, BindingType::Modifier, "Modifier");
                            });
                            ui.end_row();

                            // Value is hidden for a Modifier declaration.
                            match form.binding_type {
                                BindingType::Macro => {
                                    ui.label("Macro text");
                                    ui.text_edit_singleline(&mut form.macro_text);
                                    ui.end_row();
                                }
                                BindingType::Action => {
                                    ui.label("Action");
                                    egui::ComboBox::from_id_salt("controller_action_pick")
                                        .selected_text(if form.action.is_empty() {
                                            "pick..."
                                        } else {
                                            form.action.as_str()
                                        })
                                        .show_ui(ui, |ui| {
                                            for name in KeyAction::controller_action_names() {
                                                ui.selectable_value(
                                                    &mut form.action,
                                                    name.to_string(),
                                                    name,
                                                );
                                            }
                                            // Wheels: selecting one makes this
                                            // button the wheel's opener (the
                                            // wheel editor's "Opens with"
                                            // follows — same relationship).
                                            ui.separator();
                                            ui.weak("Wheels");
                                            for name in self.wheel_action_names() {
                                                ui.selectable_value(
                                                    &mut form.action,
                                                    name.clone(),
                                                    &name,
                                                );
                                            }
                                        });
                                    ui.end_row();
                                }
                                BindingType::Modifier => {}
                            }
                        });
                    match form.binding_type {
                        BindingType::Macro => ui.weak("Use \\r for enter (e.g. \"hide\\r\")."),
                        BindingType::Action => {
                            ui.weak("Actions that work from a pad; anything else, use a Macro.")
                        }
                        BindingType::Modifier => ui.weak(
                            "This button becomes a modifier: hold it while pressing another \
                             bound button to fire that button's modified binding.",
                        ),
                    };

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
                match self.save_controller_bind_from_form(&form, state.is_global) {
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
    delete_request: &mut Option<String>,
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
            state.wheel_delete_armed = false;
            state.wheel_designer_path.clear();
            state.wheel_selected_slice = None;
            state.wheel_drag = WheelDesignerDrag::None;
            state.wheel_undo.clear();
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
                state.wheel_delete_armed = false;
                state.wheel_designer_path.clear();
                state.wheel_selected_slice = None;
            }
        }
        // Delete a named wheel (the default and dynamic-portals entries are
        // permanent). Two clicks: the first arms and names the opener bind
        // that will be cleared, the second executes.
        if !state.wheel_selected.is_empty() && state.wheel_selected != PORTAL_WHEEL_KEY {
            let label = if state.wheel_delete_armed {
                "Confirm delete"
            } else {
                "Delete wheel"
            };
            if ui
                .button(label)
                .on_hover_text(
                    "Delete this wheel: its slices, its meta, and any button \
                     bound to open it. Click twice.",
                )
                .clicked()
            {
                if state.wheel_delete_armed {
                    *delete_request = Some(state.wheel_selected.clone());
                    state.wheel_delete_armed = false;
                } else {
                    state.wheel_delete_armed = true;
                    let opener = wheel_button_from_binds(config, &state.wheel_selected)
                        .or_else(|| {
                            config
                                .controller_wheels_meta
                                .get(&state.wheel_selected)
                                .and_then(|m| m.button.clone())
                        });
                    state.wheel_status = Some(match opener {
                        Some(button) => format!(
                            "'{}' opens with '{}' — that bind will be cleared. Click again to delete.",
                            state.wheel_selected, button
                        ),
                        None => format!(
                            "Delete wheel '{}'? Click again to confirm.",
                            state.wheel_selected
                        ),
                    });
                }
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
    // Wheel↔button validation, inline where the controls live (never in
    // the story window): recomputed from config each frame, so edits from
    // the binding dialog or this tab refresh the warnings in place.
    // [controller] is the source of truth — when a wheel's meta opener
    // disagrees with it, [controller] wins.
    for warning in crate::core::AppCore::wheel_binding_conflicts(config) {
        ui.colored_label(ui.visuals().warn_fg_color, format!("⚠ {}", warning));
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
    let mut undo_clicked = false;
    match state.wheel_view_mode {
        WheelViewMode::Numeric => {
            egui::ScrollArea::vertical()
                .id_salt("controller_wheel_slices")
                .auto_shrink([false, false])
                .max_height((ui.available_height() - 60.0).max(60.0))
                .show(ui, |ui| {
                    render_slice_rows(
                        ui,
                        buffer,
                        &mut Vec::new(),
                        &mut ops,
                        inner_ceiling,
                        &config.controller_tuning.fire_mode,
                    );
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
            undo_clicked = render_wheel_designer(
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
                &config.controller_tuning.fire_mode,
                &mut ops,
                &mut state.wheel_undo,
                &mut state.wheel_status,
            );
        }
    }
    if !ops.is_empty() {
        // §2a: ops mutate after the render pass — bank the pre-op state.
        let meta_start = state.wheel_meta_buffer.as_ref().and_then(|m| m.start);
        state.wheel_undo.push((buffer.clone(), meta_start));
        if state.wheel_undo.len() > WHEEL_UNDO_CAP {
            let drop = state.wheel_undo.len() - WHEEL_UNDO_CAP;
            state.wheel_undo.drain(..drop);
        }
    }
    let mut start_dirty = false;
    let meta = state.wheel_meta_buffer.get_or_insert_with(Default::default);
    for op in ops {
        if let Some(msg) = apply_wheel_op_v2(
            buffer,
            op,
            &config.controller_tuning.back_slice,
            meta,
            &mut start_dirty,
        ) {
            state.wheel_status = Some(msg);
        }
    }
    if start_dirty {
        *meta_save = Some((wheel_name.to_string(), meta.clone()));
    }
    if undo_clicked {
        if let Some((snap_buffer, snap_start)) = state.wheel_undo.pop() {
            *buffer = snap_buffer;
            if meta.start != snap_start {
                meta.start = snap_start;
                *meta_save = Some((wheel_name.to_string(), meta.clone()));
            }
            state.wheel_selected_slice = None;
            state.wheel_drag = WheelDesignerDrag::None;
            if wheel_slices_at(buffer, &state.wheel_designer_path).is_none() {
                state.wheel_designer_path.clear();
            }
        }
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
    default_fire: &str,
) {
    for i in 0..slices.len() {
        path.push(i);
        {
            let slice = &mut slices[i];
            ui.horizontal(|ui| {
                ui.add_space((path.len() - 1) as f32 * 18.0);
                render_slice_fields(ui, slice, inner_ceiling, default_fire);

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
            render_slice_rows(ui, &mut slices[i].slices, path, ops, inner_ceiling, default_fire);
        }
        path.pop();
    }
}

/// The per-slice edit widgets — label, command, type, color, span, inner —
/// shared by the numeric rows and the designer's selected-slice panel so
/// the two edit paths can't diverge. Caller supplies the surrounding row
/// (indent, move/add/delete buttons). `default_fire` is the config's global
/// `fire_mode`, shown as the effective type of a slice without its own.
fn render_slice_fields(
    ui: &mut egui::Ui,
    slice: &mut WheelSlice,
    inner_ceiling: u8,
    default_fire: &str,
) {
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
        let is_dead = slice.is_none_type();
        ui.add_enabled(
            !is_dead,
            egui::TextEdit::singleline(&mut slice.command)
                .desired_width(150.0)
                .hint_text(if is_folder {
                    "(folder)"
                } else if is_dead {
                    "(dead zone)"
                } else {
                    "command"
                }),
        )
        .on_hover_text(
            "Command sent to the game when this slice fires. A slice with \
             sub-slices is a folder — leave its command empty.",
        )
        .on_disabled_hover_text(
            "A None-type slice is a dead zone — it never fires, so it has \
             no command. Change its type to make it live.",
        );
    }
    // Per-slice fire type (wheel v2). Folders always descend on dwell and
    // Back always ascends, so only plain leaves get the dropdown. A slice
    // without its own type shows the global default it inherits.
    if !slice.back && !is_folder {
        let effective = slice
            .fire_type
            .clone()
            .unwrap_or_else(|| default_fire.to_string());
        let shown = SLICE_FIRE_TYPES
            .iter()
            .find(|(v, _)| *v == effective)
            .map(|(_, l)| *l)
            .unwrap_or("Release");
        egui::ComboBox::from_id_salt(ui.id().with("slice_fire_type"))
            .selected_text(shown)
            .width(74.0)
            .show_ui(ui, |ui| {
                for (value, label) in SLICE_FIRE_TYPES {
                    if ui
                        .selectable_label(effective == value, label)
                        .clicked()
                    {
                        slice.fire_type = Some(value.to_string());
                    }
                }
            })
            .response
            .on_hover_text(
                "How this slice fires. None: dead zone — holds its seat but \
                 never aims or fires. Release: dwell to commit, fires when the \
                 wheel button comes up. Edge: fires the instant the stick \
                 crosses the edge threshold, no dwell. Retract: dwell to \
                 commit, then a small inward flick fires. Folders aren't a \
                 type — use +sub to give a slice a child ring.",
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
    default_fire: &str,
    ops: &mut Vec<WheelOp>,
    undo: &mut Vec<(Vec<WheelSlice>, Option<f32>)>,
    status: &mut Option<String>,
) -> bool {
    // A structural edit (delete, move) can strand the path; fall back to
    // the top level rather than a blank canvas.
    if wheel_slices_at(buffer, designer_path).is_none() {
        designer_path.clear();
        *selected_slice = None;
    }

    // §2a undo: snapshot the pre-edit state once per frame; pushed at the
    // end only when a structural edit actually ran this frame. (Edits
    // queued as WheelOps are snapshotted by the caller instead.)
    let undo_snapshot = (buffer.clone(), meta.start);
    let mut mutated = false;
    let mut undo_clicked = false;

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
        // either. With any lock on the level, the two dividers touching
        // seat 0 are also frozen: trading seat 0's width rotates `start`,
        // which would move every locked slice.
        let any_lock = level.iter().any(|s| s.locked);
        let draggable = |b: usize| {
            let structural = if has_ghost { b + 1 < real_len } else { real_len >= 2 };
            structural
                && !level[b].locked
                && !level[(b + 1) % real_len].locked
                && !(any_lock && (b == 0 || (b + 1) % real_len == 0))
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
                        // A locked slice can't be grabbed — unlock first
                        // (F5: direct moves of a locked slice are blocked).
                        if seat < level.len() && !level[seat].locked {
                            grabbed = WheelDesignerDrag::Wedge { slice: seat, target: seat };
                        }
                    }
                }
            }
            // Any grab that mutates (divider materialize now, wedge move
            // on release, floor drag next frames) snapshots pre-edit state.
            if grabbed != WheelDesignerDrag::None {
                mutated = true;
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
    if let WheelDesignerDrag::Wedge { slice, target } = drag {
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let v = pos - center;
                let view = build_view(level, meta);
                if let Some(seat) =
                    super::super::gamepad::seat_index_at_angle(v.x, -v.y, &view.layout)
                {
                    if seat < level.len() {
                        // A move never carries a slice across a lock:
                        // reorders are confined to the run of unlocked
                        // slices around it, so no locked slice shifts.
                        *target = clamp_move_target(level, *slice, seat);
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
        // can't trade width, so those get none; with any lock present,
        // seat 0's two dividers freeze too — see grab_handle)...
        let any_lock = level.iter().any(|s| s.locked);
        let draggable = |b: usize| {
            let structural = if has_ghost { b + 1 < real_len } else { real_len >= 2 };
            structural
                && !level[b].locked
                && !level[(b + 1) % real_len].locked
                && !(any_lock && (b == 0 || (b + 1) % real_len == 0))
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
        // ----- Split / merge gestures (F6): right-click on the ring. Over
        // a divider dot the gesture is merge (remove that divider); over a
        // wedge body it's split (insert a divider at the cursor angle). A
        // ghost preview shows what the right-click would do. -----
        if *drag == WheelDesignerDrag::None {
            // The op-space start: the view's real seat 0 center — identical
            // to meta.start at the top level, and the anchor-derived
            // rotation inside a folder, so cursor angles and ring-op
            // bookkeeping share one coordinate system.
            let op_start = view.layout.seats[0].center_deg();
            // A divider is mergeable if it's a real boundary (not the ghost
            // Back's edge). Lock guards live in ring_merge itself.
            let mergeable = |b: usize| {
                if has_ghost { b + 1 < real_len } else { real_len >= 2 }
            };
            let hover = response.hover_pos().filter(|p| {
                let r = (*p - center).length();
                r >= hub * 0.6 && r <= outer + 12.0
            });
            let hovered_divider = hover.and_then(|pos| {
                let aim = aim_of(pos);
                view.layout
                    .seats
                    .iter()
                    .enumerate()
                    .filter(|(b, _)| mergeable(*b))
                    .map(|(b, seat)| {
                        let angle = (seat.start_deg + seat.span_deg).rem_euclid(360.0);
                        (b, super::super::gamepad::angular_gap(aim, angle))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .filter(|(_, gap)| *gap <= 8.0)
                    .map(|(b, _)| b)
            });
            // Split candidate: the real wedge under the cursor, when both
            // halves would clear MIN_SPAN (the ghost simply doesn't appear
            // when there's no room).
            let split_at = hover.filter(|_| hovered_divider.is_none()).and_then(|pos| {
                let v = pos - center;
                let r = v.length();
                if r < hub || r > outer {
                    return None;
                }
                let seat =
                    super::super::gamepad::seat_index_at_angle(v.x, -v.y, &view.layout)?;
                if seat >= real_len {
                    return None;
                }
                let s = &view.layout.seats[seat];
                let aim = aim_of(pos);
                let off = (aim - s.start_deg).rem_euclid(360.0);
                let min = crate::config::WHEEL_MIN_SPAN_DEG;
                (off >= min && s.span_deg - off >= min).then_some((seat, aim))
            });

            // Ghost previews.
            if let Some(b) = hovered_divider {
                // Merge ghost: thicken the divider that would be removed.
                let seat = &view.layout.seats[b];
                let a = (seat.start_deg + seat.span_deg).to_radians()
                    - std::f32::consts::FRAC_PI_2;
                let dir = egui::vec2(a.cos(), a.sin());
                painter.line_segment(
                    [center + dir * hub, center + dir * outer],
                    egui::Stroke::new(2.5, ui.visuals().warn_fg_color),
                );
                response.clone().on_hover_text(
                    "Right-click to merge: this divider goes away and the \
                     counter-clockwise slice keeps the union.",
                );
            } else if let Some((_, aim)) = split_at {
                // Split ghost: dashed radial line + dashed rim marker at
                // the cut angle.
                let a = aim.to_radians() - std::f32::consts::FRAC_PI_2;
                let dir = egui::vec2(a.cos(), a.sin());
                let line: Vec<egui::Pos2> =
                    vec![center + dir * hub, center + dir * outer];
                painter.extend(egui::Shape::dashed_line(
                    &line,
                    egui::Stroke::new(1.5, ui.visuals().selection.stroke.color),
                    5.0,
                    4.0,
                ));
                let rim: Vec<egui::Pos2> = (-4..=4)
                    .map(|k| {
                        let da = (k as f32 * 1.2).to_radians();
                        center
                            + egui::vec2((a + da).cos(), (a + da).sin()) * (outer + 5.0)
                    })
                    .collect();
                painter.extend(egui::Shape::dashed_line(
                    &rim,
                    egui::Stroke::new(1.5, ui.visuals().selection.stroke.color),
                    3.0,
                    3.0,
                ));
                response.clone().on_hover_text(
                    "Right-click to split this slice at the cursor angle.",
                );
            }

            if response.secondary_clicked() {
                if let Some(b) = hovered_divider {
                    let view = build_view(level, meta);
                    materialize_spans(level, &view.layout);
                    match ring_merge(level, op_start, b) {
                        Ok((new_start, survivor)) => {
                            mutated = true;
                            *selected_slice = Some(survivor);
                            if at_top {
                                let norm = new_start.rem_euclid(360.0);
                                let new =
                                    if norm.abs() < 1e-4 { None } else { Some(norm) };
                                if new != meta.start {
                                    meta.start = new;
                                    *meta_save =
                                        Some((wheel_name.to_string(), meta.clone()));
                                }
                            }
                        }
                        Err(msg) => *status = Some(msg.to_string()),
                    }
                } else if let Some((seat, aim)) = split_at {
                    let view = build_view(level, meta);
                    materialize_spans(level, &view.layout);
                    if let Some((new_start, new_idx)) =
                        ring_split(level, op_start, seat, aim)
                    {
                        mutated = true;
                        *selected_slice = Some(new_idx);
                        if at_top {
                            let norm = new_start.rem_euclid(360.0);
                            let new = if norm.abs() < 1e-4 { None } else { Some(norm) };
                            if new != meta.start {
                                meta.start = new;
                                *meta_save = Some((wheel_name.to_string(), meta.clone()));
                            }
                        }
                    }
                }
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
                "Equalise unlocked slices within each run between locks. \
                 Locked slices keep their exact position and width; a \
                 None-type gap keeps its width too.",
            )
            .clicked()
        {
            let widths: Vec<f32> = build_view(level, meta)
                .layout
                .seats
                .iter()
                .take(level.len())
                .map(|s| s.span_deg)
                .collect();
            even_out_runs(level, &widths);
            mutated = true;
        }
        if ui
            .button("Mirror")
            .on_hover_text(
                "Flip this level left↔right; every slice keeps its width. \
                 Deliberately includes locked slices — the one whole-ring \
                 transform besides ±15° that overrides a lock.",
            )
            .clicked()
        {
            mutated = true;
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
                    mutated = true;
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
                mutated = true;
                level.push(WheelSlice {
                    label: "◂ Back".to_string(),
                    back: true,
                    ..Default::default()
                });
                *selected_slice = Some(level.len() - 1);
            }
        }
        // ±15° rotate (F4b): the top level rotates via the wheel's `start`.
        // Inside a folder the Back anchor owns the rotation (Back stays put
        // across levels), so rotate is disabled there — unless the ring has
        // no anchored Back (back_slice = none, or an explicit Back seat),
        // where the user owns the geometry and `start` applies as usual.
        let can_rotate =
            at_top || back_anchor == "none" || level.iter().any(|s| s.back);
        let mut rotate = 0.0f32;
        let rot_hint = if can_rotate {
            "Rotate the whole ring 15° (adjusts the wheel's Start). \
             Deliberately includes locked slices."
        } else {
            "This folder ring is anchored by its Back slice, which owns \
             the rotation. Add an explicit Back (or set the Back anchor \
             to none in Tuning) to rotate it."
        };
        if ui
            .add_enabled(can_rotate, egui::Button::new("-15°"))
            .on_hover_text(rot_hint)
            .on_disabled_hover_text(rot_hint)
            .clicked()
        {
            rotate = -15.0;
        }
        if ui
            .add_enabled(can_rotate, egui::Button::new("+15°"))
            .on_hover_text(rot_hint)
            .on_disabled_hover_text(rot_hint)
            .clicked()
        {
            rotate = 15.0;
        }
        if rotate != 0.0 {
            mutated = true;
            let norm = (meta.start.unwrap_or(0.0) + rotate).rem_euclid(360.0);
            meta.start = if norm.abs() < 1e-4 { None } else { Some(norm) };
            *meta_save = Some((wheel_name.to_string(), meta.clone()));
        }
        // §2a: undo the most recent structural edit. Ctrl+Z works too while
        // no text field has focus (the canvas is the natural focus here).
        let can_undo = !undo.is_empty();
        if ui
            .add_enabled(can_undo, egui::Button::new("Undo"))
            .on_hover_text(
                "Undo the last structural edit (split, merge, delete, drag, \
                 reorder, Even out, Mirror, ±15°). Ctrl+Z. Label/command \
                 text edits aren't tracked.",
            )
            .clicked()
        {
            undo_clicked = true;
        }
        if can_undo
            && ui.ctx().memory(|m| m.focused().is_none())
            && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z))
        {
            undo_clicked = true;
        }
    });

    // (wheel v2: the four directional axis-mirror buttons are gone — the
    // single Mirror covers flipping, and they confused layout design.)

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
                render_slice_fields(ui, &mut level[i], inner_ceiling, default_fire);
                let mut locked = level[i].locked;
                if ui
                    .checkbox(&mut locked, "lock")
                    .on_hover_text(
                        "Hold this slice in place: freezes its position and \
                         width for this editing session. Neighbours reflow \
                         around it; Even out, deletes, drags, and reorders \
                         never move it. Mirror and ±15° still do (whole-ring \
                         transforms). Locks aren't saved with the wheel.",
                    )
                    .changed()
                {
                    level[i].locked = locked;
                    if locked {
                        level[i].span = Some(resolved_width);
                    }
                }
                // F15: reorder reads the way it looks — < / > shift the
                // slice around the ring, and the selection follows it.
                let is_locked = level[i].locked;
                let n = level.len();
                let ccw_to = if i == 0 { n - 1 } else { i - 1 };
                let cw_to = (i + 1) % n;
                let reorder_hint = if is_locked {
                    "Locked — unlock to move this slice."
                } else {
                    "Shift this slice one seat around the ring. It never \
                     crosses a locked slice."
                };
                if ui
                    .add_enabled(!is_locked && n > 1, egui::Button::new("<").small())
                    .on_hover_text(reorder_hint)
                    .on_disabled_hover_text(reorder_hint)
                    .clicked()
                {
                    let to = clamp_move_target(level, i, ccw_to);
                    if let Some(at) = move_slice(level, i, to) {
                        mutated = true;
                        *selected_slice = Some(at);
                    }
                }
                if ui
                    .add_enabled(!is_locked && n > 1, egui::Button::new(">").small())
                    .on_hover_text(reorder_hint)
                    .on_disabled_hover_text(reorder_hint)
                    .clicked()
                {
                    let to = clamp_move_target(level, i, cw_to);
                    if let Some(at) = move_slice(level, i, to) {
                        mutated = true;
                        *selected_slice = Some(at);
                    }
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
            ui.weak(
                "Click a wedge to edit it · drag a wedge to reorder · \
                 right-click the ring to split a slice · right-click a \
                 divider dot to merge.",
            );
        }
    }
    if ui.button("+ Add slice").clicked() {
        // The new slice lands at the end of this level; select it so its
        // fields open immediately.
        *selected_slice = Some(level.len());
        ops.push(WheelOp::AddChild(designer_path.clone()));
    }

    // §2a: a structural edit ran this frame — bank the pre-edit snapshot.
    if mutated {
        undo.push(undo_snapshot);
        if undo.len() > WHEEL_UNDO_CAP {
            let drop = undo.len() - WHEEL_UNDO_CAP;
            undo.drain(..drop);
        }
    }
    undo_clicked
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

/// Even out the ring (wheel v2, run-based): locked slices are position
/// anchors and None-type slices keep their width, so equalization happens
/// within each maximal run of consecutive unlocked slices between locks —
/// each run's total width is conserved, which is exactly what keeps every
/// locked slice's angular position fixed. With no locks and no None
/// slices this degrades to the legacy "everything back to auto" (all
/// spans cleared), which keeps simple rings simple.
///
/// `widths` are the resolved spans of the real slices (materialized
/// geometry) — required because run totals must be measured, not implied.
fn even_out_runs(level: &mut [WheelSlice], widths: &[f32]) {
    let n = level.len();
    if n == 0 || widths.len() < n {
        return;
    }
    let fixed = |s: &WheelSlice| s.locked || s.is_none_type();
    if !level.iter().any(fixed) {
        for s in level.iter_mut() {
            s.span = None;
        }
        return;
    }
    // Freeze everything to concrete widths, then equalize the free
    // members of each run between locks.
    for (s, w) in level.iter_mut().zip(widths) {
        s.span = Some(*w);
    }
    // Runs are delimited by LOCKED slices (None-type slices belong to a
    // run but keep their width — "a None-type gap is left untouched").
    // Walk circularly from the first locked slice; with no locked slice at
    // all, the whole ring is one run.
    let first_lock = level.iter().position(|s| s.locked);
    let order: Vec<usize> = match first_lock {
        Some(f) => (0..n).map(|k| (f + k) % n).collect(),
        None => (0..n).collect(),
    };
    let mut run: Vec<usize> = Vec::new();
    let mut flush = |run: &mut Vec<usize>, level: &mut [WheelSlice]| {
        let free: Vec<usize> = run
            .iter()
            .copied()
            .filter(|&i| !level[i].is_none_type())
            .collect();
        if !free.is_empty() {
            let total: f32 = run
                .iter()
                .filter(|&&i| !level[i].is_none_type())
                .map(|&i| level[i].span.unwrap_or(0.0))
                .sum();
            let share = total / free.len() as f32;
            for &i in &free {
                level[i].span = Some(share);
            }
        }
        run.clear();
    };
    for &i in &order {
        if level[i].locked {
            flush(&mut run, level);
        } else {
            run.push(i);
        }
    }
    flush(&mut run, level);
}

/// Outcome of a lock-aware slice deletion.
#[derive(Debug, PartialEq)]
enum RingDelete {
    /// The slice was removed; its width went to the unlocked neighbour.
    Removed,
    /// Both neighbours are locked: the seat became a None-type dead zone
    /// holding the exact wedge (F5a) — no locked slice moved.
    BecameNone,
}

/// Delete seat `i` from a fully-materialized ring, honoring locks: the
/// freed width goes to an unlocked neighbour (counter-clockwise/previous
/// preferred), or — with both neighbours locked — the seat converts to a
/// None-type dead zone in place. Every boundary outside the affected
/// wedge(s) is untouched, so locked positions hold by construction.
/// Returns the ring's new `start` alongside what happened; `start` moves
/// only when seat 0's leading edge or width changed (same rule as the
/// divider drag).
fn ring_delete(
    level: &mut Vec<WheelSlice>,
    start: f32,
    i: usize,
) -> Option<(f32, RingDelete)> {
    let n = level.len();
    if i >= n {
        return None;
    }
    if n == 1 {
        level.clear();
        return Some((start, RingDelete::Removed));
    }
    let w = |s: &WheelSlice| s.span.unwrap_or(0.0);
    // Absolute leading edges of every seat (aim convention; resolve_spans
    // centers seat 0 at `start`).
    let leading: Vec<f32> = {
        let mut acc = start - w(&level[0]) / 2.0;
        level
            .iter()
            .map(|s| {
                let l = acc;
                acc += w(s);
                l
            })
            .collect()
    };
    let prev = (i + n - 1) % n;
    let next = (i + 1) % n;
    let freed = w(&level[i]);
    if level[prev].locked && level[next].locked {
        // F5a: both neighbours locked (with n == 2 that's the same slice on
        // both sides) — the wedge becomes a None-type dead zone holding the
        // exact seat; no locked slice moves.
        let seat = &mut level[i];
        *seat = WheelSlice {
            span: seat.span,
            inner: seat.inner,
            fire_type: Some("none".to_string()),
            ..Default::default()
        };
        return Some((start, RingDelete::BecameNone));
    }
    // The unlocked side extends — previous (counter-clockwise) preferred,
    // matching the merge identity rule. prev == next (n == 2) collapses to
    // that one neighbour either way.
    let absorber = if !level[prev].locked { prev } else { next };
    // Growth direction decides whose leading edge survives: prev grows
    // clockwise (keeps its own leading edge), next grows counter-clockwise
    // (its leading edge becomes the deleted seat's).
    let absorber_new_leading = if absorber == prev {
        leading[absorber]
    } else {
        leading[i]
    };
    level[absorber].span = Some(w(&level[absorber]) + freed);
    level.remove(i);
    // Every boundary outside the union is untouched; restore `start` so
    // the NEW seat 0's center sits where its wedge actually is.
    let new_first_leading = if i == 0 {
        // New seat 0 is old slice 1: either it absorbed backwards (its
        // leading edge is old seat 0's) or it kept its own edge.
        if absorber == next { absorber_new_leading } else { leading[1] }
    } else if absorber == 0 {
        // Seat 0 grew (forwards into seat 1's old spot, or backwards
        // around the wrap): its recorded new leading edge is authoritative.
        absorber_new_leading
    } else {
        leading[0]
    };
    let new_start = (new_first_leading + w(&level[0]) / 2.0).rem_euclid(360.0);
    Some((new_start, RingDelete::Removed))
}

/// Split seat `seat` of a fully-materialized ring at the absolute aim
/// angle `at_deg`, inserting the new (empty) slice clockwise of the kept
/// part. Both halves must clear `WHEEL_MIN_SPAN_DEG`; returns the new
/// slice's index (always `seat + 1`) with the ring's `start` adjusted so
/// nothing else moves. None when there's no room or the angle isn't
/// inside the seat.
fn ring_split(
    level: &mut Vec<WheelSlice>,
    start: f32,
    seat: usize,
    at_deg: f32,
) -> Option<(f32, usize)> {
    let n = level.len();
    if seat >= n {
        return None;
    }
    let w = |s: &WheelSlice| s.span.unwrap_or(0.0);
    let leading0 = start - w(&level[0]) / 2.0;
    let seat_leading =
        leading0 + level[..seat].iter().map(w).sum::<f32>();
    let width = w(&level[seat]);
    let off = (at_deg - seat_leading).rem_euclid(360.0);
    if off >= width {
        return None;
    }
    if off < WHEEL_MIN_SPAN_DEG - 1e-3 || width - off < WHEEL_MIN_SPAN_DEG - 1e-3 {
        return None;
    }
    level[seat].span = Some(off);
    level.insert(
        seat + 1,
        WheelSlice {
            span: Some(width - off),
            ..Default::default()
        },
    );
    // Seat 0's width may have changed (seat == 0); recenter start on the
    // shrunk first wedge so every other boundary stays put.
    let new_start = (leading0 + w(&level[0]) / 2.0).rem_euclid(360.0);
    Some((new_start, seat + 1))
}

/// Merge the divider after seat `boundary` on a fully-materialized ring:
/// the two adjacent seats become one wedge (their union). Identity — the
/// counter-clockwise slice (the one listed above in the Numeric list)
/// survives with its label/command/colour/type; the clockwise slice's
/// settings are discarded. Exception: when the survivor-by-direction is
/// locked and the other side isn't, the unlocked side survives instead
/// (a lock's width must not grow as a side effect). Guards: a ring keeps
/// at least 2 slices; both sides locked refuses. Returns (new_start,
/// surviving index) or an error message for the status line.
fn ring_merge(
    level: &mut Vec<WheelSlice>,
    start: f32,
    boundary: usize,
) -> Result<(f32, usize), &'static str> {
    let n = level.len();
    if boundary >= n {
        return Err("no such divider");
    }
    if n <= 2 {
        return Err("a wheel keeps at least 2 slices");
    }
    let a = boundary; // counter-clockwise side of the divider
    let b = (boundary + 1) % n; // clockwise side
    if level[a].locked && level[b].locked {
        return Err("both slices are locked — unlock one to merge");
    }
    // Counter-clockwise survivor unless it's locked and the other isn't.
    let (survivor, absorbed) = if level[a].locked { (b, a) } else { (a, b) };
    let w = |s: &WheelSlice| s.span.unwrap_or(0.0);
    let leading: Vec<f32> = {
        let mut acc = start - w(&level[0]) / 2.0;
        level
            .iter()
            .map(|s| {
                let l = acc;
                acc += w(s);
                l
            })
            .collect()
    };
    // The union wedge always starts at seat a's leading edge.
    let union_leading = leading[a];
    let union_width = w(&level[a]) + w(&level[b]);
    level[survivor].span = Some(union_width);
    level.remove(absorbed);
    let survivor_now = if absorbed < survivor { survivor - 1 } else { survivor };
    // Restore start: find the new seat 0's leading edge.
    let new_first_leading = if survivor_now == 0 {
        // The union owns seat 0 (survivor is first): wrap-merges start at
        // the union's leading edge, in-line merges at seat 0's own edge.
        if a == n - 1 || a == 0 { union_leading } else { leading[0] }
    } else if absorbed == 0 {
        // Old seat 0 was absorbed into the tail (wrap merge, survivor at
        // the end): the new first slice is old slice 1.
        leading[1]
    } else {
        leading[0]
    };
    let new_start = (new_first_leading + w(&level[0]) / 2.0).rem_euclid(360.0);
    Ok((new_start.rem_euclid(360.0), survivor_now))
}

/// Clamp a reorder target so the moved slice never crosses a locked one:
/// every slice that would shift (those strictly between `from` and `to`)
/// must be unlocked. Returns the nearest reachable seat toward `to`.
fn clamp_move_target(level: &[WheelSlice], from: usize, to: usize) -> usize {
    if from >= level.len() || to >= level.len() || from == to {
        return to.min(level.len().saturating_sub(1));
    }
    if to > from {
        let mut reach = from;
        for t in (from + 1)..=to {
            if level[t].locked {
                break;
            }
            reach = t;
        }
        reach
    } else {
        let mut reach = from;
        for t in (to..from).rev() {
            if level[t].locked {
                break;
            }
            reach = t;
        }
        reach
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
mod form_tests {
    use super::*;

    fn wheel_form(button: &str, action: &str, modifier1: &str) -> ControllerFormState {
        let mut form = ControllerFormState::empty();
        form.button = button.to_string();
        form.binding_type = BindingType::Action;
        form.action = action.to_string();
        form.modifier1 = modifier1.to_string();
        form
    }

    /// Wheel openers bind bare buttons only: the runtime's held-wheel scan
    /// never resolves modifier combos, so the form must refuse them.
    #[test]
    fn wheel_opener_rejects_modifiers() {
        let form = wheel_form("r2", "controller_wheel:left2", "l1");
        let err = form.build_binding().unwrap_err();
        assert!(err.contains("bare button"), "unexpected error: {err}");
    }

    /// Both wheel action forms (bare default + named prefix) save from the
    /// Action dropdown on a bare button.
    #[test]
    fn wheel_actions_accepted_on_bare_buttons() {
        for action in ["controller_wheel", "controller_wheel:portals"] {
            let (key, bind) = wheel_form("r2", action, MODIFIER_NONE)
                .build_binding()
                .expect("wheel action on a bare button must validate");
            assert_eq!(key, "r2");
            assert!(
                matches!(&bind, KeyBindAction::Action(name) if name == action),
                "expected wheel action bind for {action}"
            );
        }
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

    /// Absolute (leading, width) pairs of every seat — the geometry two
    /// rings are compared by. Positions are what the lock guarantees.
    fn seat_geometry(level: &[WheelSlice], start: f32) -> Vec<(f32, f32)> {
        let w0 = level[0].span.unwrap();
        let mut acc = (start - w0 / 2.0).rem_euclid(360.0);
        level
            .iter()
            .map(|s| {
                let w = s.span.unwrap();
                let l = acc;
                acc = (acc + w).rem_euclid(360.0);
                (l, w)
            })
            .collect()
    }

    fn locked_named(label: &str, span: f32) -> WheelSlice {
        WheelSlice {
            label: label.to_string(),
            span: Some(span),
            locked: true,
            ..Default::default()
        }
    }

    #[test]
    fn ring_delete_gives_the_wedge_to_the_ccw_neighbour() {
        // a(90) b(90) c(90) d(90), start 0. Delete c: b (counter-clockwise)
        // absorbs; a and d must not move.
        let mut level = vec![
            named("a", Some(90.0)),
            named("b", Some(90.0)),
            named("c", Some(90.0)),
            named("d", Some(90.0)),
        ];
        let before = seat_geometry(&level, 0.0);
        let (start, out) = ring_delete(&mut level, 0.0, 2).unwrap();
        assert_eq!(out, RingDelete::Removed);
        let labels: Vec<&str> = level.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "b", "d"]);
        let after = seat_geometry(&level, start);
        assert_eq!(after[0], before[0], "a untouched");
        assert_eq!(after[1].0, before[1].0, "b's leading edge untouched");
        assert!((after[1].1 - 180.0).abs() < 1e-3, "b holds the union");
        assert_eq!(after[2], before[3], "d untouched");
    }

    #[test]
    fn ring_delete_prefers_unlocked_side_and_start_survives_seat0_edits() {
        // Delete seat 0 (start-centered): the ring must not rotate.
        let mut level = vec![
            named("a", Some(90.0)),
            named("b", Some(90.0)),
            named("c", Some(90.0)),
            named("d", Some(90.0)),
        ];
        let before = seat_geometry(&level, 0.0);
        // d (ccw of a) locked → b absorbs backwards instead.
        level[3].locked = true;
        let (start, _) = ring_delete(&mut level, 0.0, 0).unwrap();
        let labels: Vec<&str> = level.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["b", "c", "d"]);
        let after = seat_geometry(&level, start);
        // b's wedge = union of old a+b, starting at a's old leading edge.
        assert!((gamepad::angular_gap(after[0].0, before[0].0)) < 1e-3, "union starts at a's edge");
        assert!((after[0].1 - 180.0).abs() < 1e-3);
        assert_eq!(after[1], before[2], "c untouched");
        assert_eq!(after[2], before[3], "locked d never moved");
    }

    #[test]
    fn ring_delete_between_two_locks_becomes_a_dead_zone() {
        let mut level = vec![
            locked_named("a", 90.0),
            named("b", Some(90.0)),
            locked_named("c", 90.0),
            named("d", Some(90.0)),
        ];
        let before = seat_geometry(&level, 0.0);
        let (start, out) = ring_delete(&mut level, 0.0, 1).unwrap();
        assert_eq!(out, RingDelete::BecameNone);
        assert_eq!(start, 0.0, "nothing rotated");
        assert_eq!(level.len(), 4, "seat survives as a dead zone");
        assert!(level[1].is_none_type());
        assert!(level[1].command.is_empty() && level[1].label.is_empty());
        assert!(!level[1].locked, "the dead zone itself is unlocked");
        assert_eq!(seat_geometry(&level, start), before, "geometry identical");
    }

    #[test]
    fn ring_split_inserts_after_and_keeps_the_rest_pinned() {
        let mut level = vec![
            named("a", Some(120.0)),
            named("b", Some(120.0)),
            named("c", Some(120.0)),
        ];
        let before = seat_geometry(&level, 0.0);
        // Seat 1 spans 60..180; split at 100°.
        let (start, new_idx) = ring_split(&mut level, 0.0, 1, 100.0).unwrap();
        assert_eq!(new_idx, 2);
        let labels: Vec<&str> = level.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "b", "", "c"]);
        let after = seat_geometry(&level, start);
        assert_eq!(after[0], before[0], "a untouched");
        assert!((after[1].1 - 40.0).abs() < 1e-3, "b keeps the ccw part");
        assert!((after[2].0 - 100.0).abs() < 1e-3, "new slice starts at the cut");
        assert!((after[2].1 - 80.0).abs() < 1e-3);
        assert_eq!(after[3], before[2], "c untouched");

        // No room: a 40° slice can't be split; neither can a cut leaving
        // a sub-30° half.
        let mut tiny = vec![named("a", Some(40.0)), named("b", Some(320.0))];
        assert!(ring_split(&mut tiny, 0.0, 0, 0.0).is_none());
        let mut wide = vec![named("a", Some(320.0)), named("b", Some(40.0))];
        // Seat 0 spans -160..160; a cut at -150 leaves 10° on the ccw side.
        assert!(ring_split(&mut wide, 0.0, 0, 210.0).is_none());
    }

    #[test]
    fn ring_split_seat0_recenters_start() {
        let mut level = vec![named("a", Some(180.0)), named("b", Some(180.0))];
        let before = seat_geometry(&level, 0.0);
        // Seat 0 spans -90..90 (i.e. 270..90 wrapped); cut at 30°.
        let (start, _) = ring_split(&mut level, 0.0, 0, 30.0).unwrap();
        let after = seat_geometry(&level, start);
        assert!((gamepad::angular_gap(after[0].0, before[0].0)) < 1e-3, "a's edge fixed");
        assert!((after[0].1 - 120.0).abs() < 1e-3);
        assert!((gamepad::angular_gap(after[2].0, before[1].0)) < 1e-3, "b untouched");
    }

    #[test]
    fn ring_merge_keeps_the_ccw_slice_and_its_identity() {
        let mut level = vec![
            named("a", Some(90.0)),
            named("b", Some(90.0)),
            named("c", Some(90.0)),
            named("d", Some(90.0)),
        ];
        level[1].command = "keep me".to_string();
        level[2].command = "discard".to_string();
        let before = seat_geometry(&level, 0.0);
        // Divider after b merges b+c → b survives.
        let (start, survivor) = ring_merge(&mut level, 0.0, 1).unwrap();
        assert_eq!(survivor, 1);
        let labels: Vec<&str> = level.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "b", "d"]);
        assert_eq!(level[1].command, "keep me");
        let after = seat_geometry(&level, start);
        assert_eq!(after[0], before[0]);
        assert_eq!(after[1].0, before[1].0);
        assert!((after[1].1 - 180.0).abs() < 1e-3);
        assert_eq!(after[2], before[3]);
    }

    #[test]
    fn ring_merge_unlocked_side_survives_a_locked_ccw() {
        let mut level = vec![
            named("a", Some(90.0)),
            locked_named("b", 90.0),
            named("c", Some(90.0)),
            named("d", Some(90.0)),
        ];
        level[2].command = "the unlocked one".to_string();
        let before = seat_geometry(&level, 0.0);
        let (start, survivor) = ring_merge(&mut level, 0.0, 1).unwrap();
        assert_eq!(level[survivor].command, "the unlocked one");
        let after = seat_geometry(&level, start);
        // The union still spans b+c's old wedge; a and d pinned.
        assert_eq!(after[0], before[0]);
        assert!((after[survivor].0 - before[1].0).abs() < 1e-3);
        assert!((after[survivor].1 - 180.0).abs() < 1e-3);
        assert_eq!(after[2], before[3]);
    }

    #[test]
    fn ring_merge_guards() {
        // Two slices: the last divider can't be removed.
        let mut two = vec![named("a", Some(180.0)), named("b", Some(180.0))];
        assert!(ring_merge(&mut two, 0.0, 0).is_err());
        // Both locked: refused.
        let mut level = vec![
            locked_named("a", 90.0),
            locked_named("b", 90.0),
            named("c", Some(180.0)),
        ];
        assert!(ring_merge(&mut level, 0.0, 0).is_err());
        assert_eq!(level.len(), 3, "refused merge changes nothing");
    }

    #[test]
    fn ring_merge_across_the_wrap() {
        let mut level = vec![
            named("a", Some(90.0)),
            named("b", Some(90.0)),
            named("c", Some(90.0)),
            named("d", Some(90.0)),
        ];
        let before = seat_geometry(&level, 0.0);
        // Divider after d (the wrap divider) merges d+a → d survives.
        let (start, survivor) = ring_merge(&mut level, 0.0, 3).unwrap();
        let labels: Vec<&str> = level.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["b", "c", "d"]);
        assert_eq!(survivor, 2);
        let after = seat_geometry(&level, start);
        assert_eq!(after[0], before[1], "b untouched");
        assert_eq!(after[1], before[2], "c untouched");
        assert!((gamepad::angular_gap(after[2].0, before[3].0)) < 1e-3, "union starts at d's edge");
        assert!((after[2].1 - 180.0).abs() < 1e-3);
    }

    #[test]
    fn clamp_move_target_confines_to_the_unlocked_run() {
        let mut level = vec![
            named("a", None),
            named("b", None),
            locked_named("L", 90.0),
            named("c", None),
            named("d", None),
        ];
        // a can reach b but not cross L.
        assert_eq!(clamp_move_target(&level, 0, 1), 1);
        assert_eq!(clamp_move_target(&level, 0, 4), 1);
        // d can walk back to c but not across L either.
        assert_eq!(clamp_move_target(&level, 4, 3), 3);
        assert_eq!(clamp_move_target(&level, 4, 0), 3);
        // Without locks the full range opens up.
        level[2].locked = false;
        assert_eq!(clamp_move_target(&level, 0, 4), 4);
    }

    #[test]
    fn even_out_runs_equalizes_between_locks_only() {
        // lockA(60) b(50) c(130) lockD(60) e(60), start 0.
        let mut level = vec![
            locked_named("A", 60.0),
            named("b", Some(50.0)),
            named("c", Some(130.0)),
            locked_named("D", 60.0),
            named("e", Some(60.0)),
        ];
        let widths: Vec<f32> = level.iter().map(|s| s.span.unwrap()).collect();
        let before = seat_geometry(&level, 0.0);
        even_out_runs(&mut level, &widths);
        let after = seat_geometry(&level, 0.0);
        // Locks pinned exactly.
        assert_eq!(after[0], before[0]);
        assert_eq!(after[3], before[3]);
        // b/c equalized within their run (180 total → 90 each).
        assert!((level[1].span.unwrap() - 90.0).abs() < 1e-3);
        assert!((level[2].span.unwrap() - 90.0).abs() < 1e-3);
        // e's run is just e: unchanged.
        assert!((level[4].span.unwrap() - 60.0).abs() < 1e-3);
    }

    #[test]
    fn even_out_runs_leaves_none_gaps_and_falls_back_to_auto() {
        // A None gap between two locks keeps its width.
        let mut level = vec![
            locked_named("A", 90.0),
            named("", Some(60.0)),
            locked_named("B", 90.0),
            named("c", Some(70.0)),
            named("d", Some(50.0)),
        ];
        level[1].fire_type = Some("none".to_string());
        let widths: Vec<f32> = level.iter().map(|s| s.span.unwrap()).collect();
        even_out_runs(&mut level, &widths);
        assert!((level[1].span.unwrap() - 60.0).abs() < 1e-3, "gap untouched");
        assert!((level[3].span.unwrap() - 60.0).abs() < 1e-3, "c/d equalized");
        assert!((level[4].span.unwrap() - 60.0).abs() < 1e-3);

        // No locks, no None slices: legacy behaviour — everything to auto.
        let mut plain = vec![named("a", Some(100.0)), named("b", Some(260.0))];
        let widths: Vec<f32> = plain.iter().map(|s| s.span.unwrap()).collect();
        even_out_runs(&mut plain, &widths);
        assert!(plain.iter().all(|s| s.span.is_none()));
    }

    #[test]
    fn even_out_spares_locked_slices() {
        let mut level = vec![slice(Some(120.0)), slice(Some(90.0)), slice(Some(150.0))];
        level[0].locked = true;
        let widths: Vec<f32> = level.iter().map(|s| s.span.unwrap()).collect();
        even_out_runs(&mut level, &widths);
        // The locked slice keeps position AND width; the run between the
        // lock's two sides (90 + 150 = 240) equalizes to 120 each.
        assert_eq!(level[0].span, Some(120.0));
        assert_eq!(level[1].span, Some(120.0));
        assert_eq!(level[2].span, Some(120.0));
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
