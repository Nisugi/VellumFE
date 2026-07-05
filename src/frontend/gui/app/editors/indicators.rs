//! Indicator template editor: inline table over the merged template list
//! (defaults + custom store), saved back to the indicator template store —
//! the same buffer-and-save-all model the TUI editor uses.

use super::super::VellumGuiApp;
use super::color_field;
use crate::config::{Config, IndicatorTemplateEntry, IndicatorTemplateStore};
use eframe::egui;

pub(in super::super) struct IndicatorTemplatesEditorState {
    entries: Vec<EntryBuffer>,
    error: Option<String>,
}

struct EntryBuffer {
    id: String,
    name: Option<String>,
    title: String,
    icon: String,
    active_color: String,
    inactive_color: String,
    default_status: Option<String>,
    default_color: Option<String>,
    enabled: bool,
}

impl EntryBuffer {
    fn from_entry(entry: &IndicatorTemplateEntry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            title: entry.title.clone().unwrap_or_default(),
            icon: entry.icon.clone().unwrap_or_default(),
            active_color: entry.active_color.clone().unwrap_or_default(),
            inactive_color: entry.inactive_color.clone().unwrap_or_default(),
            default_status: entry.default_status.clone(),
            default_color: entry.default_color.clone(),
            enabled: entry.enabled,
        }
    }

    fn empty() -> Self {
        Self {
            id: String::new(),
            name: None,
            title: String::new(),
            icon: String::new(),
            active_color: String::new(),
            inactive_color: String::new(),
            default_status: None,
            default_color: None,
            enabled: true,
        }
    }

    fn to_entry(&self) -> IndicatorTemplateEntry {
        fn opt(value: &str) -> Option<String> {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        IndicatorTemplateEntry {
            id: self.id.trim().to_string(),
            name: self.name.clone(),
            title: opt(&self.title),
            icon: opt(&self.icon),
            inactive_color: opt(&self.inactive_color),
            active_color: opt(&self.active_color),
            default_status: self.default_status.clone(),
            default_color: self.default_color.clone(),
            enabled: self.enabled,
        }
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_indicator_templates_editor(&mut self) {
        let entries = Config::list_indicator_templates()
            .iter()
            .map(EntryBuffer::from_entry)
            .collect();
        self.indicator_templates_editor = Some(IndicatorTemplatesEditorState {
            entries,
            error: None,
        });
    }

    pub(in super::super) fn render_indicator_templates_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.indicator_templates_editor.take() else {
            return;
        };

        let mut open = true;
        let mut save_request = false;
        let mut remove_index: Option<usize> = None;

        egui::Window::new("Indicator Templates")
            .id(egui::Id::new("gui_indicator_templates"))
            .open(&mut open)
            .default_width(520.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add template").clicked() {
                        state.entries.push(EntryBuffer::empty());
                    }
                    if ui.button("Save all").clicked() {
                        save_request = true;
                    }
                });
                ui.weak("Disabled templates are skipped when building indicator windows.");
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("indicator_templates_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("indicator_templates_grid")
                            .num_columns(7)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("On");
                                ui.strong("Id");
                                ui.strong("Title");
                                ui.strong("Icon");
                                ui.strong("Active");
                                ui.strong("Inactive");
                                ui.label("");
                                ui.end_row();

                                for (index, entry) in state.entries.iter_mut().enumerate() {
                                    ui.checkbox(&mut entry.enabled, "");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut entry.id)
                                            .desired_width(90.0),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut entry.title)
                                            .desired_width(90.0),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut entry.icon)
                                            .desired_width(40.0),
                                    );
                                    color_field(ui, &mut entry.active_color);
                                    color_field(ui, &mut entry.inactive_color);
                                    if ui.small_button("Remove").clicked() {
                                        remove_index = Some(index);
                                    }
                                    ui.end_row();
                                }
                            });
                    });

                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });

        if let Some(index) = remove_index {
            if index < state.entries.len() {
                state.entries.remove(index);
            }
        }

        if save_request {
            let entries: Vec<IndicatorTemplateEntry> = state
                .entries
                .iter()
                .filter(|entry| !entry.id.trim().is_empty())
                .map(EntryBuffer::to_entry)
                .collect();
            if entries.len() < state.entries.len() {
                state.error = Some("Entries without an id were skipped.".to_string());
            } else {
                state.error = None;
            }
            let store = IndicatorTemplateStore { indicators: entries };
            match Config::save_indicator_template_store(&store) {
                Ok(()) => self
                    .app_core
                    .add_system_message("Indicator templates saved."),
                Err(err) => {
                    state.error = Some(format!("Failed to save: {}", err));
                }
            }
        }

        if open {
            self.indicator_templates_editor = Some(state);
        }
    }
}
