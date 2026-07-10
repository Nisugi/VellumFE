//! Hotbar editor: bar management, button CRUD + reorder, and the structured
//! condition/state builder — everything hotbars.toml expresses, built from
//! dropdowns (no expression syntax). Edits buffer in a working copy and are
//! written via `Config::save_hotbar`, then `reload_hotbars()` re-merges
//! hotkeys and refreshes conflicts.

use super::super::VellumGuiApp;
use super::color_field;
use crate::config::{
    Config, EffectCategory, HotbarButton, HotbarButtonState, HotbarCmp, HotbarCondition,
    HotbarCountdownSource, HotbarDef, HotbarStyle, NameMatch, VitalKind, VitalUnit,
};
use crate::data::InputMode;
use eframe::egui;

const INDICATOR_IDS: &[&str] = &[
    "standing",
    "kneeling",
    "sitting",
    "prone",
    "stunned",
    "bleeding",
    "hidden",
    "invisible",
    "webbed",
    "joined",
    "dead",
];

const CMPS: &[HotbarCmp] = &[
    HotbarCmp::Lt,
    HotbarCmp::Le,
    HotbarCmp::Gt,
    HotbarCmp::Ge,
];

const VITALS: &[VitalKind] = &[
    VitalKind::Health,
    VitalKind::Mana,
    VitalKind::Stamina,
    VitalKind::Spirit,
];

fn vital_name(v: VitalKind) -> &'static str {
    match v {
        VitalKind::Health => "health",
        VitalKind::Mana => "mana",
        VitalKind::Stamina => "stamina",
        VitalKind::Spirit => "spirit",
    }
}

fn category_name(c: EffectCategory) -> &'static str {
    match c {
        EffectCategory::Buffs => "Buffs",
        EffectCategory::Debuffs => "Debuffs",
        EffectCategory::Cooldowns => "Cooldowns",
        EffectCategory::ActiveSpells => "Active Spells",
    }
}

/// Human-readable label for a leaf condition kind (combo entries).
const LEAF_KINDS: &[&str] = &[
    "Effect active",
    "Effect inactive",
    "Effect time remaining",
    "Roundtime active",
    "Casttime active",
    "Indicator",
    "Vital",
];

fn leaf_kind_index(cond: &HotbarCondition) -> usize {
    match cond {
        HotbarCondition::EffectActive { .. } => 0,
        HotbarCondition::EffectInactive { .. } => 1,
        HotbarCondition::EffectTime { .. } => 2,
        HotbarCondition::RtActive => 3,
        HotbarCondition::CtActive => 4,
        HotbarCondition::Indicator { .. } => 5,
        HotbarCondition::Vital { .. } => 6,
        HotbarCondition::All { .. } | HotbarCondition::Any { .. } => 0,
    }
}

fn default_leaf(kind: usize) -> HotbarCondition {
    match kind {
        0 => HotbarCondition::EffectActive {
            category: EffectCategory::Buffs,
            name: String::new(),
            name_match: NameMatch::Exact,
        },
        1 => HotbarCondition::EffectInactive {
            category: EffectCategory::Buffs,
            name: String::new(),
            name_match: NameMatch::Exact,
        },
        2 => HotbarCondition::EffectTime {
            category: EffectCategory::Buffs,
            name: String::new(),
            name_match: NameMatch::Exact,
            cmp: HotbarCmp::Lt,
            seconds: 60,
        },
        3 => HotbarCondition::RtActive,
        4 => HotbarCondition::CtActive,
        5 => HotbarCondition::Indicator {
            id: "hidden".to_string(),
            active: true,
        },
        _ => HotbarCondition::Vital {
            vital: VitalKind::Stamina,
            cmp: HotbarCmp::Lt,
            value: 25,
            unit: VitalUnit::Percent,
        },
    }
}

pub(in super::super) struct HotbarEditorState {
    /// Working copy of the bar being edited; None until a bar is selected.
    working: Option<HotbarDef>,
    /// Scope the working bar saves to.
    is_global: bool,
    /// Name the working copy was loaded under (rename = delete + save).
    original_name: Option<String>,
    dirty: bool,
    selected_button: Option<usize>,
    new_bar_name: String,
    hotkey_capture_armed: bool,
    error: Option<String>,
}

impl HotbarEditorState {
    fn new() -> Self {
        Self {
            working: None,
            is_global: true,
            original_name: None,
            dirty: false,
            selected_button: None,
            new_bar_name: String::new(),
            hotkey_capture_armed: false,
            error: None,
        }
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_hotbar_editor(&mut self) {
        self.hotbar_editor = Some(HotbarEditorState::new());
    }

    /// True while the hotbar form is waiting to capture a hotkey press.
    pub(in super::super) fn hotbar_capture_armed(&self) -> bool {
        self.hotbar_editor
            .as_ref()
            .is_some_and(|state| state.hotkey_capture_armed)
    }

    /// What already owns this key, if anything: "keybinds.toml" or
    /// "bar:button". Ignores the button being edited itself.
    fn hotkey_conflict_owner(&self, key: &str, own_bar: &str, own_button: &str) -> Option<String> {
        let (code, modifiers) = crate::config::parse_key_string(key)?;
        let key_event = crate::data::input::KeyEvent { code, modifiers };

        for existing in self.app_core.config.keybinds.keys() {
            if let Some((code, modifiers)) = crate::config::parse_key_string(existing) {
                let existing_event = crate::data::input::KeyEvent { code, modifiers };
                if existing_event == key_event {
                    return Some("keybinds.toml".to_string());
                }
            }
        }
        for bar in &self.app_core.config.hotbars.bars {
            for button in &bar.buttons {
                if bar.name == own_bar && button.id == own_button {
                    continue;
                }
                if let Some(hotkey) = &button.hotkey {
                    if let Some((code, modifiers)) = crate::config::parse_key_string(hotkey) {
                        let existing_event = crate::data::input::KeyEvent { code, modifiers };
                        if existing_event == key_event {
                            return Some(format!("{}:{}", bar.name, button.id));
                        }
                    }
                }
            }
        }
        None
    }

    pub(in super::super) fn render_hotbar_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.hotbar_editor.take() else {
            return;
        };

        // Hotkey capture: suppress macro dispatch and grab the next press.
        if state.hotkey_capture_armed {
            self.app_core.ui_state.input_mode = InputMode::KeybindForm;
            if let Some(press) = Self::collect_pressed_key_events(ctx).into_iter().next() {
                let key = crate::core::menu_actions::key_event_to_string(press.key_event);
                if let (Some(working), Some(idx)) = (&mut state.working, state.selected_button) {
                    if let Some(button) = working.buttons.get_mut(idx) {
                        button.hotkey = Some(key);
                        state.dirty = true;
                    }
                }
                state.hotkey_capture_armed = false;
                self.app_core.ui_state.input_mode = InputMode::Normal;
            }
        }

        let mut open = true;
        let mut load_bar: Option<(HotbarDef, bool)> = None;
        let mut delete_bar: Option<String> = None;
        let mut save_requested = false;

        egui::Window::new("Hotbars")
            .id(egui::Id::new("gui_hotbar_editor"))
            .open(&mut open)
            .default_width(720.0)
            .default_height(520.0)
            .show(ctx, |ui| {
                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(190.0);
                        ui.strong("Bars");
                        ui.separator();
                        let character = self.app_core.config.character.clone();
                        let bars: Vec<HotbarDef> =
                            self.app_core.config.hotbars.bars.clone();
                        egui::ScrollArea::vertical()
                            .id_salt("hotbar_bars_scroll")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height() - 70.0)
                            .show(ui, |ui| {
                                for bar in &bars {
                                    let (in_global, in_character) =
                                        Config::hotbar_scope(&bar.name, character.as_deref());
                                    let scope = match (in_global, in_character) {
                                        (_, true) => "[C]",
                                        (true, false) => "[G]",
                                        _ => "[?]", // embedded default, not yet on disk
                                    };
                                    let selected = state.working.is_some()
                                        && state.original_name.as_deref() == Some(&bar.name);
                                    let label = format!("{} {}", scope, bar.name);
                                    if ui.selectable_label(selected, label).clicked() {
                                        load_bar = Some((bar.clone(), !in_character));
                                    }
                                }
                                if bars.is_empty() {
                                    ui.weak("No bars defined.");
                                }
                            });
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut state.new_bar_name)
                                    .hint_text("new bar name")
                                    .desired_width(100.0),
                            );
                            if ui.button("Add").clicked() {
                                let name = state.new_bar_name.trim().to_string();
                                if name.is_empty() {
                                    state.error = Some("Bar name is required.".to_string());
                                } else if self.app_core.config.hotbars.find_bar(&name).is_some() {
                                    state.error =
                                        Some(format!("Bar '{}' already exists.", name));
                                } else {
                                    load_bar = Some((
                                        HotbarDef {
                                            name,
                                            title: None,
                                            buttons: Vec::new(),
                                        },
                                        true,
                                    ));
                                    state.new_bar_name.clear();
                                    state.error = None;
                                }
                            }
                        });
                        if let Some(name) = state.original_name.clone() {
                            if ui.button("Delete selected bar").clicked() {
                                delete_bar = Some(name);
                            }
                        }
                    });

                    ui.separator();

                    ui.vertical(|ui| {
                    let Some(working) = &mut state.working else {
                        ui.weak("Select a bar on the left, or add a new one.");
                        return;
                    };

                    ui.horizontal(|ui| {
                        ui.strong(format!("Bar: {}", working.name));
                        ui.label("Title:");
                        let mut title = working.title.clone().unwrap_or_default();
                        if ui
                            .add(egui::TextEdit::singleline(&mut title).desired_width(140.0))
                            .changed()
                        {
                            working.title =
                                (!title.trim().is_empty()).then(|| title.clone());
                            state.dirty = true;
                        }
                        ui.checkbox(&mut state.is_global, "Global (all characters)");
                        if ui
                            .add_enabled(state.dirty, egui::Button::new("Save bar"))
                            .clicked()
                        {
                            save_requested = true;
                        }
                        if state.dirty {
                            ui.weak("unsaved changes");
                        }
                    });

                    // Live preview against the current game state
                    let now_server = chrono::Utc::now().timestamp()
                        + self.app_core.message_processor.server_time_offset;
                    let preview = crate::core::hotbar::resolve_bar(
                        working,
                        &self.app_core.game_state,
                        now_server,
                    );
                    if !preview.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.weak("Preview:");
                            for b in &preview {
                                let text = match b.countdown_secs {
                                    Some(s) if s > 0 => format!("{}  {}s", b.label, s),
                                    _ => b.label.clone(),
                                };
                                let mut rich = egui::RichText::new(text);
                                if b.dim {
                                    rich = rich.color(ui.visuals().weak_text_color());
                                } else if let Some(fg) = b
                                    .fg
                                    .as_deref()
                                    .and_then(super::super::widgets::parse_hex_color)
                                {
                                    rich = rich.color(fg);
                                }
                                let mut btn = egui::Button::new(rich);
                                if !b.dim {
                                    if let Some(bg) = b
                                        .bg
                                        .as_deref()
                                        .and_then(super::super::widgets::parse_hex_color)
                                    {
                                        btn = btn.fill(bg);
                                    }
                                }
                                let _ = ui.add(btn);
                            }
                        });
                    }
                    ui.separator();

                    // Button list with reorder / add / delete / duplicate
                    ui.horizontal(|ui| {
                        ui.strong("Buttons");
                        if ui.button("Add button").clicked() {
                            let n = working.buttons.len() + 1;
                            let mut id = format!("button{}", n);
                            while working.buttons.iter().any(|b| b.id == id) {
                                id.push('x');
                            }
                            working.buttons.push(HotbarButton {
                                id,
                                label: "New".to_string(),
                                command: String::new(),
                                hotkey: None,
                                tooltip: None,
                                category: None,
                                countdown: None,
                                states: Vec::new(),
                                default_style: None,
                            });
                            state.selected_button = Some(working.buttons.len() - 1);
                            state.dirty = true;
                        }
                    });

                    let conflicts: Vec<(String, String)> = self
                        .app_core
                        .hotbar_key_conflicts
                        .iter()
                        .map(|c| (c.bar.clone(), c.button.clone()))
                        .collect();

                    let mut move_up: Option<usize> = None;
                    let mut move_down: Option<usize> = None;
                    let mut delete_button: Option<usize> = None;
                    let mut duplicate_button: Option<usize> = None;

                    egui::ScrollArea::vertical()
                        .id_salt("hotbar_buttons_scroll")
                        .auto_shrink([false, false])
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for (idx, button) in working.buttons.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    if ui.small_button("^").clicked() {
                                        move_up = Some(idx);
                                    }
                                    if ui.small_button("v").clicked() {
                                        move_down = Some(idx);
                                    }
                                    if ui.small_button("Dup").clicked() {
                                        duplicate_button = Some(idx);
                                    }
                                    if ui.small_button("Del").clicked() {
                                        delete_button = Some(idx);
                                    }
                                    let selected = state.selected_button == Some(idx);
                                    let mut label = format!(
                                        "{}  ({})",
                                        button.label, button.command
                                    );
                                    if let Some(hotkey) = &button.hotkey {
                                        label.push_str(&format!("  [{}]", hotkey));
                                    }
                                    if conflicts.contains(&(
                                        working.name.clone(),
                                        button.id.clone(),
                                    )) {
                                        label.push_str("  (key conflict)");
                                    }
                                    if ui.selectable_label(selected, label).clicked() {
                                        state.selected_button = Some(idx);
                                        state.hotkey_capture_armed = false;
                                    }
                                });
                            }
                            if working.buttons.is_empty() {
                                ui.weak("No buttons yet.");
                            }
                        });

                    if let Some(idx) = move_up {
                        if idx > 0 {
                            working.buttons.swap(idx, idx - 1);
                            state.selected_button = Some(idx - 1);
                            state.dirty = true;
                        }
                    }
                    if let Some(idx) = move_down {
                        if idx + 1 < working.buttons.len() {
                            working.buttons.swap(idx, idx + 1);
                            state.selected_button = Some(idx + 1);
                            state.dirty = true;
                        }
                    }
                    if let Some(idx) = duplicate_button {
                        let mut copy = working.buttons[idx].clone();
                        copy.id = format!("{}_copy", copy.id);
                        while working.buttons.iter().any(|b| b.id == copy.id) {
                            copy.id.push('x');
                        }
                        copy.hotkey = None; // duplicating the key would always conflict
                        working.buttons.insert(idx + 1, copy);
                        state.selected_button = Some(idx + 1);
                        state.dirty = true;
                    }
                    if let Some(idx) = delete_button {
                        working.buttons.remove(idx);
                        state.selected_button = None;
                        state.dirty = true;
                    }

                    ui.separator();

                    // Button form
                    let Some(button_idx) = state.selected_button else {
                        ui.weak("Select a button to edit it.");
                        return;
                    };
                    let bar_name = working.name.clone();
                    // Collect effect-name suggestions before borrowing the button
                    let suggestions: std::collections::HashMap<&'static str, Vec<String>> =
                        EffectCategory::ALL
                            .iter()
                            .map(|c| {
                                (
                                    c.state_key(),
                                    self.app_core
                                        .game_state
                                        .effects
                                        .get(c.state_key())
                                        .map(|store| {
                                            store
                                                .effects
                                                .iter()
                                                .map(|e| e.text.clone())
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                )
                            })
                            .collect();
                    let conflict_owner_lookup =
                        |app: &Self, key: &str, own_button: &str| -> Option<String> {
                            app.hotkey_conflict_owner(key, &bar_name, own_button)
                        };
                    let conflict_owner = {
                        let button = &working.buttons[button_idx];
                        button
                            .hotkey
                            .as_deref()
                            .filter(|k| !k.is_empty())
                            .and_then(|k| conflict_owner_lookup(self, k, &button.id))
                    };

                    let Some(button) = working.buttons.get_mut(button_idx) else {
                        return;
                    };

                    egui::ScrollArea::vertical()
                        .id_salt("hotbar_button_form_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let mut changed = false;
                            egui::Grid::new("hotbar_button_grid")
                                .num_columns(2)
                                .show(ui, |ui| {
                                    ui.label("Label");
                                    changed |= ui
                                        .text_edit_singleline(&mut button.label)
                                        .changed();
                                    ui.end_row();

                                    ui.label("Command");
                                    changed |= ui
                                        .text_edit_singleline(&mut button.command)
                                        .changed();
                                    ui.end_row();

                                    ui.label("Tooltip");
                                    let mut tooltip =
                                        button.tooltip.clone().unwrap_or_default();
                                    if ui.text_edit_singleline(&mut tooltip).changed() {
                                        button.tooltip = (!tooltip.trim().is_empty())
                                            .then(|| tooltip.clone());
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Category");
                                    let mut category =
                                        button.category.clone().unwrap_or_default();
                                    if ui.text_edit_singleline(&mut category).changed() {
                                        button.category = (!category.trim().is_empty())
                                            .then(|| category.clone());
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Hotkey");
                                    ui.horizontal(|ui| {
                                        let mut hotkey =
                                            button.hotkey.clone().unwrap_or_default();
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(&mut hotkey)
                                                    .desired_width(110.0),
                                            )
                                            .changed()
                                        {
                                            button.hotkey = (!hotkey.trim().is_empty())
                                                .then(|| hotkey.trim().to_lowercase());
                                            changed = true;
                                        }
                                        let capture_label = if state.hotkey_capture_armed {
                                            "Press a key..."
                                        } else {
                                            "Capture"
                                        };
                                        if ui.button(capture_label).clicked() {
                                            state.hotkey_capture_armed =
                                                !state.hotkey_capture_armed;
                                        }
                                        if button.hotkey.is_some()
                                            && ui.small_button("Clear").clicked()
                                        {
                                            button.hotkey = None;
                                            changed = true;
                                        }
                                    });
                                    ui.end_row();
                                });

                            if let Some(owner) = &conflict_owner {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    format!(
                                        "Key is already bound by {} - it wins over this button.",
                                        owner
                                    ),
                                );
                            }

                            ui.separator();
                            changed |= render_countdown_editor(ui, button, &suggestions);
                            ui.separator();
                            changed |= render_states_editor(ui, button, &suggestions);

                            if changed {
                                state.dirty = true;
                            }
                        });
                    });
                });
            });

        if let Some(name) = delete_bar {
            let character = self.app_core.config.character.clone();
            let (in_global, in_character) = Config::hotbar_scope(&name, character.as_deref());
            let mut result = Ok(());
            if in_character {
                result = Config::delete_hotbar(&name, false, character.as_deref());
            }
            if result.is_ok() && in_global {
                result = Config::delete_hotbar(&name, true, character.as_deref());
            }
            match result {
                Ok(()) => {
                    self.app_core.reload_hotbars();
                    self.app_core
                        .add_system_message(&format!("Hotbar '{}' deleted.", name));
                    state.working = None;
                    state.original_name = None;
                    state.selected_button = None;
                    state.dirty = false;
                }
                Err(err) => state.error = Some(format!("Failed to delete bar: {}", err)),
            }
        }

        if let Some((bar, is_global)) = load_bar {
            state.working = Some(bar.clone());
            state.original_name = self
                .app_core
                .config
                .hotbars
                .find_bar(&bar.name)
                .map(|b| b.name.clone());
            state.is_global = is_global;
            state.selected_button = None;
            state.dirty = state.original_name.is_none(); // new bars start dirty
            state.error = None;
            state.hotkey_capture_armed = false;
        }

        if save_requested {
            if let Some(working) = &state.working {
                let character = self.app_core.config.character.clone();
                match Config::save_hotbar(working, state.is_global, character.as_deref()) {
                    Ok(()) => {
                        self.app_core.reload_hotbars();
                        self.app_core
                            .add_system_message(&format!("Hotbar '{}' saved.", working.name));
                        state.original_name = Some(working.name.clone());
                        state.dirty = false;
                        state.error = None;
                    }
                    Err(err) => state.error = Some(format!("Failed to save bar: {}", err)),
                }
            }
        }

        if open {
            self.hotbar_editor = Some(state);
        } else {
            // Editor closed: re-enable macro dispatch if capture was armed
            if state.hotkey_capture_armed {
                self.app_core.ui_state.input_mode = InputMode::Normal;
            }
        }
    }
}

/// Countdown source section of the button form. Returns true when edited.
fn render_countdown_editor(
    ui: &mut egui::Ui,
    button: &mut HotbarButton,
    suggestions: &std::collections::HashMap<&'static str, Vec<String>>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.strong("Countdown overlay");
        let current = match &button.countdown {
            None => "None",
            Some(HotbarCountdownSource::Roundtime) => "Roundtime",
            Some(HotbarCountdownSource::Casttime) => "Casttime",
            Some(HotbarCountdownSource::Effect { .. }) => "Effect",
        };
        egui::ComboBox::from_id_salt("hotbar_countdown_source")
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current == "None", "None").clicked() {
                    button.countdown = None;
                    changed = true;
                }
                if ui
                    .selectable_label(current == "Roundtime", "Roundtime")
                    .clicked()
                {
                    button.countdown = Some(HotbarCountdownSource::Roundtime);
                    changed = true;
                }
                if ui
                    .selectable_label(current == "Casttime", "Casttime")
                    .clicked()
                {
                    button.countdown = Some(HotbarCountdownSource::Casttime);
                    changed = true;
                }
                if ui.selectable_label(current == "Effect", "Effect").clicked() {
                    button.countdown = Some(HotbarCountdownSource::Effect {
                        category: EffectCategory::Cooldowns,
                        name: String::new(),
                        name_match: NameMatch::Exact,
                    });
                    changed = true;
                }
            });
    });

    if let Some(HotbarCountdownSource::Effect {
        category,
        name,
        name_match,
    }) = &mut button.countdown
    {
        ui.horizontal(|ui| {
            changed |= category_combo(ui, "hotbar_countdown_cat", category);
            changed |= effect_name_field(ui, "hotbar_countdown_name", name, category, suggestions);
            changed |= match_combo(ui, "hotbar_countdown_match", name_match);
        });
    }
    changed
}

/// States section: ordered condition->style cards. Returns true when edited.
fn render_states_editor(
    ui: &mut egui::Ui,
    button: &mut HotbarButton,
    suggestions: &std::collections::HashMap<&'static str, Vec<String>>,
) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.strong("States");
        ui.weak("(first matching state styles the button)");
        if ui.button("Add state").clicked() {
            button.states.push(HotbarButtonState {
                when: HotbarCondition::All {
                    conditions: vec![default_leaf(3)], // RT active
                },
                style: HotbarStyle {
                    dim: true,
                    ..Default::default()
                },
            });
            changed = true;
        }
    });

    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    let mut delete_state: Option<usize> = None;

    for (idx, hb_state) in button.states.iter_mut().enumerate() {
        let frame = egui::Frame::group(ui.style());
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("State {}", idx + 1));
                if ui.small_button("^").clicked() {
                    move_up = Some(idx);
                }
                if ui.small_button("v").clicked() {
                    move_down = Some(idx);
                }
                if ui.small_button("Del").clicked() {
                    delete_state = Some(idx);
                }
            });
            changed |= render_condition_group(
                ui,
                &format!("state{}_cond", idx),
                &mut hb_state.when,
                0,
                suggestions,
            );
            ui.label("Style while active:");
            changed |= render_style_editor(ui, &format!("state{}_style", idx), &mut hb_state.style);
        });
    }

    if let Some(idx) = move_up {
        if idx > 0 {
            button.states.swap(idx, idx - 1);
            changed = true;
        }
    }
    if let Some(idx) = move_down {
        if idx + 1 < button.states.len() {
            button.states.swap(idx, idx + 1);
            changed = true;
        }
    }
    if let Some(idx) = delete_state {
        button.states.remove(idx);
        changed = true;
    }

    // Default style (no state matched)
    ui.separator();
    let mut has_default = button.default_style.is_some();
    if ui
        .checkbox(&mut has_default, "Default style (when no state matches)")
        .changed()
    {
        button.default_style = has_default.then(HotbarStyle::default);
        changed = true;
    }
    if let Some(style) = &mut button.default_style {
        changed |= render_style_editor(ui, "default_style", style);
    }

    changed
}

/// Condition group editor. Groups may nest one level deep (editors enforce);
/// deeper hand-authored trees still render read-only as a summary.
fn render_condition_group(
    ui: &mut egui::Ui,
    id: &str,
    cond: &mut HotbarCondition,
    depth: usize,
    suggestions: &std::collections::HashMap<&'static str, Vec<String>>,
) -> bool {
    let mut changed = false;

    // Normalize a bare leaf at the root into a group so the UI is uniform
    if depth == 0
        && !matches!(
            cond,
            HotbarCondition::All { .. } | HotbarCondition::Any { .. }
        )
    {
        let leaf = cond.clone();
        *cond = HotbarCondition::All {
            conditions: vec![leaf],
        };
        changed = true;
    }

    let is_all = matches!(cond, HotbarCondition::All { .. });
    ui.horizontal(|ui| {
        ui.label(if depth == 0 { "When" } else { "Group:" });
        let mut all_selected = is_all;
        egui::ComboBox::from_id_salt(format!("{}_grouptype", id))
            .selected_text(if all_selected { "all of" } else { "any of" })
            .show_ui(ui, |ui| {
                if ui.selectable_value(&mut all_selected, true, "all of").clicked()
                    || ui
                        .selectable_value(&mut all_selected, false, "any of")
                        .clicked()
                {
                    if all_selected != is_all {
                        let conditions = match cond {
                            HotbarCondition::All { conditions }
                            | HotbarCondition::Any { conditions } => std::mem::take(conditions),
                            _ => vec![],
                        };
                        *cond = if all_selected {
                            HotbarCondition::All { conditions }
                        } else {
                            HotbarCondition::Any { conditions }
                        };
                        changed = true;
                    }
                }
            });
        if ui.small_button("+ condition").clicked() {
            if let HotbarCondition::All { conditions } | HotbarCondition::Any { conditions } = cond
            {
                conditions.push(default_leaf(3));
                changed = true;
            }
        }
        if depth == 0 && ui.small_button("+ group").clicked() {
            if let HotbarCondition::All { conditions } | HotbarCondition::Any { conditions } = cond
            {
                conditions.push(HotbarCondition::Any {
                    conditions: vec![default_leaf(3)],
                });
                changed = true;
            }
        }
    });

    let (HotbarCondition::All { conditions } | HotbarCondition::Any { conditions }) = cond else {
        return changed;
    };

    let mut delete_idx: Option<usize> = None;
    for (idx, child) in conditions.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add_space(12.0 * (depth as f32 + 1.0));
            if ui.small_button("x").clicked() {
                delete_idx = Some(idx);
            }
            match child {
                HotbarCondition::All { .. } | HotbarCondition::Any { .. } => {
                    if depth == 0 {
                        ui.vertical(|ui| {
                            changed |= render_condition_group(
                                ui,
                                &format!("{}_g{}", id, idx),
                                child,
                                depth + 1,
                                suggestions,
                            );
                        });
                    } else {
                        // Deeper nesting is file-authored only; keep intact
                        ui.weak("(nested group - edit in hotbars.toml)");
                    }
                }
                leaf => {
                    changed |= render_leaf_condition(
                        ui,
                        &format!("{}_l{}", id, idx),
                        leaf,
                        suggestions,
                    );
                }
            }
        });
    }
    if let Some(idx) = delete_idx {
        conditions.remove(idx);
        changed = true;
    }
    changed
}

/// One leaf condition row: kind combo plus that kind's fields.
fn render_leaf_condition(
    ui: &mut egui::Ui,
    id: &str,
    cond: &mut HotbarCondition,
    suggestions: &std::collections::HashMap<&'static str, Vec<String>>,
) -> bool {
    let mut changed = false;
    let current_kind = leaf_kind_index(cond);
    egui::ComboBox::from_id_salt(format!("{}_kind", id))
        .selected_text(LEAF_KINDS[current_kind])
        .width(150.0)
        .show_ui(ui, |ui| {
            for (kind, label) in LEAF_KINDS.iter().enumerate() {
                if ui.selectable_label(kind == current_kind, *label).clicked()
                    && kind != current_kind
                {
                    *cond = default_leaf(kind);
                    changed = true;
                }
            }
        });

    match cond {
        HotbarCondition::EffectActive {
            category,
            name,
            name_match,
        }
        | HotbarCondition::EffectInactive {
            category,
            name,
            name_match,
        } => {
            changed |= category_combo(ui, &format!("{}_cat", id), category);
            changed |= effect_name_field(ui, &format!("{}_name", id), name, category, suggestions);
            changed |= match_combo(ui, &format!("{}_match", id), name_match);
        }
        HotbarCondition::EffectTime {
            category,
            name,
            name_match,
            cmp,
            seconds,
        } => {
            changed |= category_combo(ui, &format!("{}_cat", id), category);
            changed |= effect_name_field(ui, &format!("{}_name", id), name, category, suggestions);
            changed |= match_combo(ui, &format!("{}_match", id), name_match);
            changed |= cmp_combo(ui, &format!("{}_cmp", id), cmp);
            changed |= ui
                .add(egui::DragValue::new(seconds).range(0..=86_400).suffix("s"))
                .changed();
        }
        HotbarCondition::Indicator { id: ind_id, active } => {
            egui::ComboBox::from_id_salt(format!("{}_ind", id))
                .selected_text(ind_id.as_str())
                .show_ui(ui, |ui| {
                    for candidate in INDICATOR_IDS {
                        if ui
                            .selectable_label(ind_id == candidate, *candidate)
                            .clicked()
                        {
                            *ind_id = candidate.to_string();
                            changed = true;
                        }
                    }
                });
            changed |= ui.checkbox(active, "active").changed();
        }
        HotbarCondition::Vital {
            vital,
            cmp,
            value,
            unit,
        } => {
            egui::ComboBox::from_id_salt(format!("{}_vital", id))
                .selected_text(vital_name(*vital))
                .show_ui(ui, |ui| {
                    for candidate in VITALS {
                        if ui
                            .selectable_label(vital == candidate, vital_name(*candidate))
                            .clicked()
                        {
                            *vital = *candidate;
                            changed = true;
                        }
                    }
                });
            changed |= cmp_combo(ui, &format!("{}_cmp", id), cmp);
            changed |= ui
                .add(egui::DragValue::new(value).range(0..=100_000))
                .changed();
            let unit_label = match unit {
                VitalUnit::Percent => "%",
                VitalUnit::Absolute => "abs",
            };
            egui::ComboBox::from_id_salt(format!("{}_unit", id))
                .selected_text(unit_label)
                .width(60.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(matches!(unit, VitalUnit::Percent), "%")
                        .clicked()
                    {
                        *unit = VitalUnit::Percent;
                        changed = true;
                    }
                    if ui
                        .selectable_label(matches!(unit, VitalUnit::Absolute), "abs")
                        .clicked()
                    {
                        *unit = VitalUnit::Absolute;
                        changed = true;
                    }
                });
        }
        HotbarCondition::RtActive | HotbarCondition::CtActive => {}
        HotbarCondition::All { .. } | HotbarCondition::Any { .. } => {}
    }
    changed
}

fn category_combo(ui: &mut egui::Ui, id: &str, category: &mut EffectCategory) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id.to_string())
        .selected_text(category_name(*category))
        .width(110.0)
        .show_ui(ui, |ui| {
            for candidate in EffectCategory::ALL {
                if ui
                    .selectable_label(*category == candidate, category_name(candidate))
                    .clicked()
                {
                    *category = candidate;
                    changed = true;
                }
            }
        });
    changed
}

fn effect_name_field(
    ui: &mut egui::Ui,
    id: &str,
    name: &mut String,
    category: &EffectCategory,
    suggestions: &std::collections::HashMap<&'static str, Vec<String>>,
) -> bool {
    let mut changed = ui
        .add(egui::TextEdit::singleline(name).desired_width(150.0))
        .changed();
    let known = suggestions
        .get(category.state_key())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    if !known.is_empty() {
        egui::ComboBox::from_id_salt(format!("{}_suggest", id))
            .selected_text("...")
            .width(30.0)
            .show_ui(ui, |ui| {
                for candidate in known {
                    if ui.selectable_label(false, candidate).clicked() {
                        *name = candidate.clone();
                        changed = true;
                    }
                }
            });
    }
    changed
}

fn match_combo(ui: &mut egui::Ui, id: &str, name_match: &mut NameMatch) -> bool {
    let mut changed = false;
    let label = match name_match {
        NameMatch::Exact => "exact",
        NameMatch::Contains => "contains",
    };
    egui::ComboBox::from_id_salt(id.to_string())
        .selected_text(label)
        .width(90.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(matches!(name_match, NameMatch::Exact), "exact")
                .clicked()
            {
                *name_match = NameMatch::Exact;
                changed = true;
            }
            if ui
                .selectable_label(matches!(name_match, NameMatch::Contains), "contains")
                .clicked()
            {
                *name_match = NameMatch::Contains;
                changed = true;
            }
        });
    changed
}

fn cmp_combo(ui: &mut egui::Ui, id: &str, cmp: &mut HotbarCmp) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id.to_string())
        .selected_text(cmp.symbol())
        .width(50.0)
        .show_ui(ui, |ui| {
            for candidate in CMPS {
                if ui
                    .selectable_label(cmp == candidate, candidate.symbol())
                    .clicked()
                {
                    *cmp = *candidate;
                    changed = true;
                }
            }
        });
    changed
}

/// Style fields: label override, fg/bg colors, dim. Returns true when edited.
fn render_style_editor(ui: &mut egui::Ui, id: &str, style: &mut HotbarStyle) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Label:");
        let mut label = style.label.clone().unwrap_or_default();
        if ui
            .add(
                egui::TextEdit::singleline(&mut label)
                    .hint_text("(keep)")
                    .desired_width(90.0),
            )
            .changed()
        {
            style.label = (!label.trim().is_empty()).then(|| label.clone());
            changed = true;
        }

        ui.label("Fg:");
        let mut fg = style.fg.clone().unwrap_or_default();
        let before = fg.clone();
        color_field(ui, &mut fg);
        if fg != before {
            style.fg = (!fg.trim().is_empty()).then(|| fg.clone());
            changed = true;
        }

        ui.label("Bg:");
        let mut bg = style.bg.clone().unwrap_or_default();
        let before = bg.clone();
        color_field(ui, &mut bg);
        if bg != before {
            style.bg = (!bg.trim().is_empty()).then(|| bg.clone());
            changed = true;
        }

        changed |= ui.checkbox(&mut style.dim, "dim").changed();
    });
    let _ = id;
    changed
}
