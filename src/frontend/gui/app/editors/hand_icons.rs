//! Hand-icons editor: status-driven icon states for one hand window
//! (empty hand, held weapon, prepared spell, ...). States reuse the hotbar
//! condition vocabulary and its condition-builder UI; the icon choices come
//! from the shared `hands` image pool. Saves write the window's layout def
//! and persist through the debounced layout autosave.

use super::super::VellumGuiApp;
use crate::config::{EffectCategory, HandIconState, HandSlot, HotbarCondition, NameMatch};
use eframe::egui;

pub(in super::super) struct HandIconsEditorState {
    /// Layout window name being edited (left/right/spell hand window).
    window_name: String,
    working: Vec<HandIconState>,
    dirty: bool,
    error: Option<String>,
}

fn default_state() -> HandIconState {
    HandIconState {
        when: HotbarCondition::HandHolds {
            hand: HandSlot::Right,
            item_type: Some("weapon".to_string()),
            name: None,
            name_match: NameMatch::Contains,
        },
        icon: None,
        text: None,
        icon_color: None,
    }
}

/// Picker label for a state's icon choice.
fn icon_label(icon: &Option<crate::data::IconRef>, pool: &[(String, String)]) -> String {
    match icon {
        None => "Static icon".to_string(),
        Some(crate::data::IconRef::Default) => "Static icon".to_string(),
        Some(crate::data::IconRef::None) => "None (no art)".to_string(),
        Some(crate::data::IconRef::Image { path }) => pool
            .iter()
            .find(|(pool_path, _)| pool_path == path)
            .map(|(_, stem)| stem.clone())
            .unwrap_or_else(|| path.clone()),
        Some(crate::data::IconRef::SheetCell { sheet, cell }) => format!("{sheet} #{cell}"),
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_hand_icons_editor(&mut self, window_name: String) {
        if self
            .hand_icons_editor
            .as_ref()
            .is_some_and(|state| state.window_name == window_name)
        {
            self.raise_editor(egui::Id::new("gui_hand_icons_editor"));
            return;
        }
        let working = self
            .app_core
            .layout
            .windows
            .iter()
            .find(|def| def.name() == window_name)
            .and_then(|def| match def {
                crate::config::WindowDef::Hand { data, .. } => Some(data.states.clone()),
                _ => None,
            })
            .unwrap_or_default();
        self.hand_icons_editor = Some(HandIconsEditorState {
            window_name,
            working,
            dirty: false,
            error: None,
        });
    }

    pub(super) fn render_hand_icons_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.hand_icons_editor.take() else {
            return;
        };
        let mut open = true;
        let mut save_request = false;

        let pool_images: Vec<(String, String)> = crate::config::pool::list_category("hands")
            .into_iter()
            .map(|image| (image.pool_path.clone(), image.stem().to_string()))
            .collect();
        // Effect-name suggestions for the shared condition builder.
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
                            .map(|store| store.effects.iter().map(|e| e.text.clone()).collect())
                            .unwrap_or_default(),
                    )
                })
                .collect();

        egui::Window::new(format!("Hand Icons - {}", state.window_name))
            .id(egui::Id::new("gui_hand_icons_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.weak(
                    "States check top to bottom; the first matching condition drives the \
                     hand's icon and text. No match = the static icon. Icons come from the \
                     'hands' image pool (install more with .jinx).",
                );
                let mut remove: Option<usize> = None;
                let mut swap: Option<(usize, usize)> = None;
                let len = state.working.len();
                for (idx, st) in state.working.iter_mut().enumerate() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong(format!("State {}", idx + 1));
                        if ui
                            .add_enabled(idx > 0, egui::Button::new("⬆").small())
                            .clicked()
                        {
                            swap = Some((idx, idx - 1));
                        }
                        if ui
                            .add_enabled(idx + 1 < len, egui::Button::new("⬇").small())
                            .clicked()
                        {
                            swap = Some((idx, idx + 1));
                        }
                        if ui
                            .small_button("✕")
                            .on_hover_text("Remove this state")
                            .clicked()
                        {
                            remove = Some(idx);
                        }
                    });
                    if super::hotbars::render_condition_group(
                        ui,
                        &format!("hand_state_{idx}"),
                        &mut st.when,
                        0,
                        &suggestions,
                    ) {
                        state.dirty = true;
                    }
                    ui.horizontal(|ui| {
                        ui.label("Icon");
                        egui::ComboBox::from_id_salt(("hand_state_icon", idx))
                            .selected_text(icon_label(&st.icon, &pool_images))
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        matches!(
                                            st.icon,
                                            None | Some(crate::data::IconRef::Default)
                                        ),
                                        "Static icon",
                                    )
                                    .on_hover_text("Keep whatever the hand shows normally")
                                    .clicked()
                                {
                                    st.icon = None;
                                    state.dirty = true;
                                }
                                if ui
                                    .selectable_label(
                                        matches!(st.icon, Some(crate::data::IconRef::None)),
                                        "None (no art)",
                                    )
                                    .clicked()
                                {
                                    st.icon = Some(crate::data::IconRef::None);
                                    state.dirty = true;
                                }
                                for (path, stem) in &pool_images {
                                    let selected = matches!(
                                        &st.icon,
                                        Some(crate::data::IconRef::Image { path: current })
                                            if current == path
                                    );
                                    if ui.selectable_label(selected, stem).clicked() {
                                        st.icon = Some(crate::data::IconRef::Image {
                                            path: path.clone(),
                                        });
                                        state.dirty = true;
                                    }
                                }
                            });
                        ui.label("Text");
                        let mut text = st.text.clone().unwrap_or_default();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut text)
                                    .hint_text("TUI prefix")
                                    .desired_width(70.0),
                            )
                            .on_hover_text(
                                "Text prefix while this state holds — the TUI's icon, and \
                                 the GUI's fallback when no art resolves",
                            )
                            .changed()
                        {
                            st.text = (!text.trim().is_empty()).then(|| text.clone());
                            state.dirty = true;
                        }
                        ui.label("Color");
                        let mut color = st.icon_color.clone().unwrap_or_default();
                        let before = color.clone();
                        super::color_field(ui, &mut color);
                        if color != before {
                            let trimmed = color.trim();
                            st.icon_color = (!trimmed.is_empty()).then(|| trimmed.to_string());
                            state.dirty = true;
                        }
                    });
                }
                if let Some((a, b)) = swap {
                    state.working.swap(a, b);
                    state.dirty = true;
                }
                if let Some(idx) = remove {
                    state.working.remove(idx);
                    state.dirty = true;
                }
                ui.separator();
                if ui.button("+ Add state").clicked() {
                    state.working.push(default_state());
                    state.dirty = true;
                }
                if pool_images.is_empty() {
                    ui.weak("No hand icons in the pool — install with .jinx list / .jinx install");
                }
                // Live preview against the current game state.
                let preview_data = crate::config::HandWidgetData {
                    icon: None,
                    icon_color: None,
                    text_color: None,
                    states: state.working.clone(),
                };
                let now_server = chrono::Utc::now().timestamp()
                    + self.app_core.message_processor.server_time_offset;
                let resolved = crate::core::hotbar::resolve_hand(
                    &preview_data,
                    &self.app_core.game_state,
                    now_server,
                    self.app_core.gameobj_data_cached(),
                );
                ui.horizontal(|ui| {
                    ui.weak("Right now this hand would show:");
                    ui.monospace(icon_label(&resolved.icon, &pool_images));
                    if let Some(text) = &resolved.text {
                        ui.monospace(format!("text '{text}'"));
                    }
                });
                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(state.dirty, egui::Button::new("Save"))
                        .clicked()
                    {
                        save_request = true;
                    }
                    if state.dirty {
                        ui.weak("unsaved changes");
                    }
                });
            });

        if save_request {
            match self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|def| def.name() == state.window_name)
            {
                Some(crate::config::WindowDef::Hand { data, .. }) => {
                    data.states = state.working.clone();
                    self.app_core.schedule_layout_autosave();
                    state.dirty = false;
                    state.error = None;
                }
                _ => {
                    state.error =
                        Some(format!("Window '{}' no longer exists.", state.window_name));
                }
            }
        }

        if open {
            self.hand_icons_editor = Some(state);
        }
    }
}
