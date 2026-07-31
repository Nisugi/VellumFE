//! Dashboard editor: per-window pop-out for a dashboard widget. Edits the
//! window's layout def — which status ids it shows, plus layout / spacing /
//! hide-inactive. Icons and colors come from the status templates (edited in
//! the Indicator Templates editor), so this editor only manages the id list
//! and layout; each entry's legacy per-entry icon/colors (TUI back-compat)
//! are preserved untouched. Saves write the layout def and persist through
//! the debounced layout autosave.

use super::super::VellumGuiApp;
use crate::config::{DashboardIndicatorDef, DashboardWidgetData};
use eframe::egui;

pub(in super::super) struct DashboardEditorState {
    /// Layout window name being edited.
    window_name: String,
    working: DashboardWidgetData,
    /// Id typed into the "add status" row.
    new_id: String,
    dirty: bool,
    error: Option<String>,
}

/// Layout options offered in the combo, as (config string, display label).
const LAYOUT_CHOICES: &[(&str, &str)] = &[
    ("horizontal", "Horizontal"),
    ("vertical", "Vertical"),
    ("flow", "Flow (wrap)"),
    ("grid:2x2", "Grid 2x2"),
    ("grid:3x3", "Grid 3x3"),
];

impl VellumGuiApp {
    pub(in super::super) fn open_dashboard_editor(&mut self, window_name: String) {
        if self
            .dashboard_editor
            .as_ref()
            .is_some_and(|state| state.window_name == window_name)
        {
            self.raise_editor(egui::Id::new("gui_dashboard_editor"));
            return;
        }
        let working = self
            .app_core
            .layout
            .windows
            .iter()
            .find(|def| def.name() == window_name)
            .and_then(|def| match def {
                crate::config::WindowDef::Dashboard { data, .. } => Some(data.clone()),
                _ => None,
            });
        let Some(working) = working else {
            self.app_core
                .add_system_message(&format!("'{window_name}' is not a dashboard window."));
            return;
        };
        self.dashboard_editor = Some(DashboardEditorState {
            window_name,
            working,
            new_id: String::new(),
            dirty: false,
            error: None,
        });
    }

    pub(super) fn render_dashboard_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.dashboard_editor.take() else {
            return;
        };
        let mut open = true;
        let mut save_request = false;

        // Known status ids the user can pick from (built-ins + custom
        // templates); free text is still allowed for the open set.
        let known_ids: Vec<String> = self
            .app_core
            .indicator_templates
            .values()
            .map(|entry| entry.id.clone())
            .collect();
        let mut known_ids = known_ids;
        known_ids.sort();

        egui::Window::new(format!("Dashboard - {}", state.window_name))
            .id(egui::Id::new("gui_dashboard_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Layout");
                    let current = state.working.layout.clone();
                    let current_label = LAYOUT_CHOICES
                        .iter()
                        .find(|(value, _)| *value == current)
                        .map(|(_, label)| *label)
                        .unwrap_or(current.as_str());
                    egui::ComboBox::from_id_salt("dashboard_layout")
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            for (value, label) in LAYOUT_CHOICES {
                                if ui
                                    .selectable_label(current == *value, *label)
                                    .clicked()
                                {
                                    state.working.layout = value.to_string();
                                    state.dirty = true;
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Spacing");
                    if ui
                        .add(egui::DragValue::new(&mut state.working.spacing).range(0..=8))
                        .changed()
                    {
                        state.dirty = true;
                    }
                    if ui
                        .checkbox(&mut state.working.hide_inactive, "Hide inactive")
                        .changed()
                    {
                        state.dirty = true;
                    }
                });

                ui.separator();
                ui.strong("Statuses");
                ui.weak(
                    "Which statuses this dashboard shows. Icons and colors come from the \
                     status templates (.settings → Indicator Templates).",
                );

                let mut remove: Option<usize> = None;
                let mut swap: Option<(usize, usize)> = None;
                let len = state.working.indicators.len();
                for (idx, ind) in state.working.indicators.iter().enumerate() {
                    ui.horizontal(|ui| {
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
                        if ui.small_button("✕").clicked() {
                            remove = Some(idx);
                        }
                        ui.monospace(&ind.id);
                    });
                }
                if let Some((a, b)) = swap {
                    state.working.indicators.swap(a, b);
                    state.dirty = true;
                }
                if let Some(idx) = remove {
                    state.working.indicators.remove(idx);
                    state.dirty = true;
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_id)
                            .hint_text("status id")
                            .desired_width(140.0),
                    );
                    egui::ComboBox::from_id_salt("dashboard_add_known")
                        .selected_text("Known…")
                        .show_ui(ui, |ui| {
                            for id in &known_ids {
                                if ui.selectable_label(false, id).clicked() {
                                    state.new_id = id.clone();
                                }
                            }
                        });
                    if ui.button("Add").clicked() {
                        let id = state.new_id.trim().to_string();
                        if id.is_empty() {
                            state.error = Some("Enter a status id.".to_string());
                        } else if state
                            .working
                            .indicators
                            .iter()
                            .any(|ind| ind.id.eq_ignore_ascii_case(&id))
                        {
                            state.error = Some(format!("'{id}' is already listed."));
                        } else {
                            state.working.indicators.push(DashboardIndicatorDef {
                                id,
                                icon: String::new(),
                                colors: Vec::new(),
                            });
                            state.new_id.clear();
                            state.error = None;
                            state.dirty = true;
                        }
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
                Some(crate::config::WindowDef::Dashboard { data, .. }) => {
                    *data = state.working.clone();
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
            self.dashboard_editor = Some(state);
        }
    }
}
