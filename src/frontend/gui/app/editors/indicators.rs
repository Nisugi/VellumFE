//! Indicator template editor: inline table over the merged template list
//! (defaults + custom store), saved back to the indicator template store —
//! the same buffer-and-save-all model the TUI editor uses.

use super::super::VellumGuiApp;
use super::color_field;
use crate::config::{Config, IndicatorTemplateEntry, IndicatorTemplateStore};
use eframe::egui;

pub(in super::super) struct IndicatorTemplatesEditorState {
    entries: Vec<EntryBuffer>,
    /// Indicator id being typed into the "add icon override" row.
    new_override_id: String,
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
        if self.indicator_templates_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_indicator_templates"));
            return;
        }
        let entries = Config::list_indicator_templates()
            .iter()
            .map(EntryBuffer::from_entry)
            .collect();
        self.indicator_templates_editor = Some(IndicatorTemplatesEditorState {
            entries,
            new_override_id: String::new(),
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

        // GUI icon art inputs, gathered up front; changes collected in the
        // closure and applied to ui_settings afterwards.
        let icon_sets = crate::config::pool::set_names("statusicons");
        let pool_images: Vec<(String, String)> =
            crate::config::pool::list_category("statusicons")
                .iter()
                .map(|image| (image.pool_path.clone(), image.stem().to_string()))
                .collect();
        let art = self.skin_state.widget_art();
        let sheets: Vec<String> = art.as_ref().map(|a| a.sheet_names()).unwrap_or_default();
        let current_set = self.ui_settings.status_icons.set.clone();
        let mut sorted_overrides: Vec<(String, crate::data::IconRef)> = self
            .ui_settings
            .status_icons
            .overrides
            .iter()
            .map(|(id, icon)| (id.clone(), icon.clone()))
            .collect();
        sorted_overrides.sort_by(|a, b| a.0.cmp(&b.0));
        let mut set_change: Option<Option<String>> = None;
        // (id, Some(new)) = upsert; (id, None) = remove.
        let mut override_changes: Vec<(String, Option<crate::data::IconRef>)> = Vec::new();
        let current_gray = self.ui_settings.status_icons.gray_inactive;
        let mut gray_change: Option<bool> = None;

        egui::Window::new("Indicator Templates")
            .id(egui::Id::new("gui_indicator_templates"))
            .order(egui::Order::Foreground)
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

                ui.separator();
                ui.strong("GUI icon art");
                ui.weak(
                    "Pool sets and per-indicator icons; the Icon column above is the TUI glyph.",
                );
                ui.horizontal(|ui| {
                    ui.label("Icon set");
                    let selected = current_set.as_deref().unwrap_or("None");
                    egui::ComboBox::from_id_salt("statusicon_set")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(current_set.is_none(), "None").clicked() {
                                set_change = Some(None);
                            }
                            for set in &icon_sets {
                                let is_current = current_set.as_deref() == Some(set.as_str());
                                if ui.selectable_label(is_current, set).clicked() {
                                    set_change = Some(Some(set.clone()));
                                }
                            }
                        });
                    if icon_sets.is_empty() {
                        ui.weak("(no sets in the pool — install with .jinx)");
                    }
                });
                let mut gray = current_gray;
                if ui
                    .checkbox(&mut gray, "Grayscale when inactive")
                    .on_hover_text(
                        "Inactive statuses show a desaturated copy of their icon instead of \
                         fading it. Grayscale copies are built only while this is on.",
                    )
                    .changed()
                {
                    gray_change = Some(gray);
                }
                for (id, icon) in &sorted_overrides {
                    ui.horizontal(|ui| {
                        ui.monospace(id);
                        let label = match icon {
                            crate::data::IconRef::Default => "Default".to_string(),
                            crate::data::IconRef::None => "None (hidden)".to_string(),
                            crate::data::IconRef::Image { path } => path
                                .rsplit_once('/')
                                .map(|(_, file)| file.to_string())
                                .unwrap_or_else(|| path.clone()),
                            crate::data::IconRef::SheetCell { sheet, cell } => {
                                format!("{sheet} #{cell}")
                            }
                        };
                        egui::ComboBox::from_id_salt(("statusicon_override", id))
                            .selected_text(label)
                            .show_ui(ui, |ui| {
                                if ui.button("Default").clicked() {
                                    override_changes
                                        .push((id.clone(), Some(crate::data::IconRef::Default)));
                                }
                                if ui
                                    .button("None (hidden)")
                                    .on_hover_text(
                                        "Suppress this icon's art; the widget shows its \
                                         built-in fallback",
                                    )
                                    .clicked()
                                {
                                    override_changes
                                        .push((id.clone(), Some(crate::data::IconRef::None)));
                                }
                                for (path, stem) in &pool_images {
                                    if ui.button(stem).clicked() {
                                        override_changes.push((
                                            id.clone(),
                                            Some(crate::data::IconRef::Image {
                                                path: path.clone(),
                                            }),
                                        ));
                                    }
                                }
                                for sheet in &sheets {
                                    if ui.button(format!("sheet: {sheet}")).clicked() {
                                        override_changes.push((
                                            id.clone(),
                                            Some(crate::data::IconRef::SheetCell {
                                                sheet: sheet.clone(),
                                                cell: 1,
                                            }),
                                        ));
                                    }
                                }
                            });
                        if let crate::data::IconRef::SheetCell { sheet, cell } = icon {
                            let max = art
                                .as_ref()
                                .and_then(|a| a.sheet_cell_count(sheet))
                                .unwrap_or(u32::MAX)
                                .max(1);
                            let mut value = *cell;
                            if ui
                                .add(egui::DragValue::new(&mut value).range(1..=max).prefix("#"))
                                .changed()
                            {
                                override_changes.push((
                                    id.clone(),
                                    Some(crate::data::IconRef::SheetCell {
                                        sheet: sheet.clone(),
                                        cell: value,
                                    }),
                                ));
                            }
                        }
                        if ui.small_button("✕").clicked() {
                            override_changes.push((id.clone(), None));
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_override_id)
                            .hint_text("indicator id (e.g. STUNNED)")
                            .desired_width(160.0),
                    );
                    if ui.button("Add icon override").clicked()
                        && !state.new_override_id.trim().is_empty()
                    {
                        override_changes.push((
                            state.new_override_id.trim().to_ascii_uppercase(),
                            Some(crate::data::IconRef::Default),
                        ));
                        state.new_override_id.clear();
                    }
                });

                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });

        if let Some(set) = set_change {
            self.ui_settings.status_icons.set = set;
            self.layout_dirty = true;
        }
        if let Some(gray) = gray_change {
            self.ui_settings.status_icons.gray_inactive = gray;
            self.layout_dirty = true;
        }
        for (id, change) in override_changes {
            match change {
                Some(icon) => {
                    self.ui_settings.status_icons.overrides.insert(id, icon);
                }
                None => {
                    self.ui_settings.status_icons.overrides.remove(&id);
                }
            }
            self.layout_dirty = true;
        }

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
