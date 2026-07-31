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
    /// Indicator id being typed into the "add grayscale exception" row.
    new_gray_id: String,
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
    /// Pickable default GUI icon; edited in the Phase-D1 states UI. Carried
    /// through save so a plain field edit never wipes it.
    icon_ref: Option<crate::data::IconRef>,
    /// Condition-driven icon states; edited in the Phase-D1 states UI, carried
    /// through save.
    states: Vec<crate::config::StatusIconState>,
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
            icon_ref: entry.icon_ref.clone(),
            states: entry.states.clone(),
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
            icon_ref: None,
            states: Vec::new(),
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
            icon_ref: self.icon_ref.clone(),
            inactive_color: opt(&self.inactive_color),
            active_color: opt(&self.active_color),
            default_status: self.default_status.clone(),
            default_color: self.default_color.clone(),
            states: self.states.clone(),
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
            new_gray_id: String::new(),
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
        // Effect-name suggestions for the shared condition builder (states).
        let suggestions: std::collections::HashMap<&'static str, Vec<String>> =
            crate::config::EffectCategory::ALL
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
        let mut sorted_gray_overrides: Vec<(String, bool)> = self
            .ui_settings
            .status_icons
            .gray_overrides
            .iter()
            .map(|(id, on)| (id.clone(), *on))
            .collect();
        sorted_gray_overrides.sort_by(|a, b| a.0.cmp(&b.0));
        // (id, Some(on)) = upsert; (id, None) = back to the global toggle.
        let mut gray_override_changes: Vec<(String, Option<bool>)> = Vec::new();

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
                // Per-indicator exceptions to the grayscale toggle.
                for (id, on) in &sorted_gray_overrides {
                    ui.horizontal(|ui| {
                        ui.monospace(id);
                        let mut on = *on;
                        egui::ComboBox::from_id_salt(format!("statusicon_gray_{id}"))
                            .width(140.0)
                            .selected_text(if on { "Grayscale" } else { "Alpha dim" })
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(on, "Grayscale").clicked() && !on {
                                    on = true;
                                    gray_override_changes.push((id.clone(), Some(true)));
                                }
                                if ui.selectable_label(!on, "Alpha dim").clicked() && on {
                                    on = false;
                                    gray_override_changes.push((id.clone(), Some(false)));
                                }
                            });
                        if ui
                            .small_button("✕")
                            .on_hover_text("Follow the global toggle again")
                            .clicked()
                        {
                            gray_override_changes.push((id.clone(), None));
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_gray_id)
                            .hint_text("indicator id for a grayscale exception")
                            .desired_width(160.0),
                    );
                    if ui.button("Add grayscale exception").clicked()
                        && !state.new_gray_id.trim().is_empty()
                    {
                        // Exceptions start as the opposite of the global
                        // toggle — that's why you'd add one.
                        gray_override_changes.push((
                            state.new_gray_id.trim().to_ascii_uppercase(),
                            Some(!current_gray),
                        ));
                        state.new_gray_id.clear();
                    }
                });
                for (id, icon) in &sorted_overrides {
                    ui.horizontal(|ui| {
                        ui.monospace(id);
                        match super::icon_ref_picker(
                            ui,
                            format!("statusicon_override_{id}"),
                            Some(icon),
                            &pool_images,
                            &sheets,
                            None,
                            Some("Default"),
                            Some("None (hidden)"),
                        ) {
                            Some(super::IconRefPick::Ref(picked)) => {
                                override_changes.push((id.clone(), Some(picked)));
                            }
                            Some(super::IconRefPick::Unset) | None => {}
                        }
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

                ui.separator();
                ui.strong("Per-template icons & states");
                ui.weak(
                    "A pickable default icon and condition-driven states per template. \
                     States check top to bottom; the first match drives the icon/color \
                     (e.g. an injury rank). Icons come from the 'statusicons' pool.",
                );
                for entry in state.entries.iter_mut() {
                    let heading = if entry.id.trim().is_empty() {
                        "(unnamed)".to_string()
                    } else {
                        entry.id.trim().to_string()
                    };
                    egui::CollapsingHeader::new(heading)
                        .id_salt(format!("status_states_{}", entry.id))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Default icon");
                                match super::icon_ref_picker(
                                    ui,
                                    format!("status_default_icon_{}", entry.id),
                                    entry.icon_ref.as_ref(),
                                    &pool_images,
                                    &sheets,
                                    Some("Default (by id)"),
                                    None,
                                    Some("None (no art)"),
                                ) {
                                    Some(super::IconRefPick::Unset) => entry.icon_ref = None,
                                    Some(super::IconRefPick::Ref(picked)) => {
                                        entry.icon_ref = Some(picked)
                                    }
                                    None => {}
                                }
                                if let Some(crate::data::IconRef::SheetCell { cell, .. }) =
                                    &mut entry.icon_ref
                                {
                                    let mut value = (*cell).max(1);
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut value)
                                                .range(1..=9999)
                                                .prefix("#"),
                                        )
                                        .changed()
                                    {
                                        *cell = value;
                                    }
                                }
                            });
                            Self::render_status_states(
                                ui,
                                &entry.id,
                                &mut entry.states,
                                &pool_images,
                                &sheets,
                                &suggestions,
                            );
                        });
                }

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
        for (id, change) in gray_override_changes {
            match change {
                Some(on) => {
                    self.ui_settings.status_icons.gray_overrides.insert(id, on);
                }
                None => {
                    self.ui_settings.status_icons.gray_overrides.remove(&id);
                }
            }
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
                Ok(()) => {
                    // Refresh the render-loop cache so new icons/states show
                    // without a restart.
                    self.app_core.refresh_indicator_templates();
                    self.app_core
                        .add_system_message("Indicator templates saved.");
                }
                Err(err) => {
                    state.error = Some(format!("Failed to save: {}", err));
                }
            }
        }

        if open {
            self.indicator_templates_editor = Some(state);
        }
    }

    /// Condition-driven status icon states for one template. Mirrors the
    /// hand-icon states editor: each state gets the shared condition builder,
    /// an IconRef picker (statusicons pool / sheet cell / none), a TUI text
    /// glyph, and a color; first match wins. Reorder + remove + add.
    fn render_status_states(
        ui: &mut egui::Ui,
        entry_id: &str,
        states: &mut Vec<crate::config::StatusIconState>,
        pool_images: &[(String, String)],
        sheets: &[String],
        suggestions: &std::collections::HashMap<&'static str, Vec<String>>,
    ) {
        let mut remove: Option<usize> = None;
        let mut swap: Option<(usize, usize)> = None;
        let len = states.len();
        for (idx, st) in states.iter_mut().enumerate() {
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
            super::hotbars::render_condition_group(
                ui,
                &format!("status_state_{entry_id}_{idx}"),
                &mut st.when,
                0,
                suggestions,
            );
            ui.horizontal(|ui| {
                ui.label("Icon");
                match super::icon_ref_picker(
                    ui,
                    format!("status_state_icon_{entry_id}_{idx}"),
                    st.icon.as_ref(),
                    pool_images,
                    sheets,
                    Some("Template default"),
                    None,
                    Some("None (no art)"),
                ) {
                    Some(super::IconRefPick::Unset) => st.icon = None,
                    Some(super::IconRefPick::Ref(picked)) => st.icon = Some(picked),
                    None => {}
                }
                if let Some(crate::data::IconRef::SheetCell { cell, .. }) = &mut st.icon {
                    let mut value = (*cell).max(1);
                    if ui
                        .add(egui::DragValue::new(&mut value).range(1..=9999).prefix("#"))
                        .changed()
                    {
                        *cell = value;
                    }
                }
                ui.label("Text");
                let mut text = st.text.clone().unwrap_or_default();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut text)
                            .hint_text("TUI glyph")
                            .desired_width(60.0),
                    )
                    .changed()
                {
                    st.text = (!text.trim().is_empty()).then(|| text.clone());
                }
                ui.label("Color");
                let mut color = st.color.clone().unwrap_or_default();
                let before = color.clone();
                color_field(ui, &mut color);
                if color != before {
                    let trimmed = color.trim();
                    st.color = (!trimmed.is_empty()).then(|| trimmed.to_string());
                }
            });
        }
        if let Some((a, b)) = swap {
            states.swap(a, b);
        }
        if let Some(idx) = remove {
            states.remove(idx);
        }
        if ui.button("+ Add state").clicked() {
            states.push(crate::config::StatusIconState {
                when: crate::config::Condition::Injury {
                    area: "neck".to_string(),
                    cmp: crate::config::Cmp::Ge,
                    level: 1,
                },
                icon: None,
                text: None,
                color: None,
            });
        }
    }
}
