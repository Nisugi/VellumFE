//! GUI indicator icon editor: image-first. Each indicator gets a default
//! image icon plus condition-driven image states (first match wins), so one
//! indicator can show different icons under different game conditions. This
//! is the GUI's job — images and conditions. The TUI's text glyph, title,
//! and active/inactive colors are edited in the TUI's own indicator editor;
//! this editor never shows them, but carries them through save untouched so
//! the two editors share one template store without clobbering each other.

use super::super::VellumGuiApp;
use crate::config::{Config, IndicatorTemplateEntry, IndicatorTemplateStore};
use eframe::egui;

pub(in super::super) struct IndicatorTemplatesEditorState {
    entries: Vec<EntryBuffer>,
    /// Index of the indicator shown in the right-hand editor panel.
    selected: usize,
    /// Free-text id for the "add indicator" row.
    new_id: String,
    /// Indicator id being typed into the "add icon override" row.
    new_override_id: String,
    /// Indicator id being typed into the "add grayscale exception" row.
    new_gray_id: String,
    error: Option<String>,
}

/// A working copy of one template. GUI-editable fields (id, enabled, the
/// default image `icon_ref`, and the condition-driven image `states`) are
/// surfaced in the editor; the TUI-only fields (title, text glyph, colors,
/// default_status/color) are held verbatim and written back on save so the
/// TUI editor's work is never lost.
struct EntryBuffer {
    id: String,
    enabled: bool,
    /// GUI: ACTIVE (Y) image icon (pool image / sheet cell).
    icon_ref: Option<crate::data::IconRef>,
    /// GUI: INACTIVE (N) image icon; None = show nothing while inactive.
    inactive_icon_ref: Option<crate::data::IconRef>,
    /// GUI: condition-driven image states (first match wins).
    states: Vec<crate::config::StatusIconState>,
    // --- TUI-owned, carried through untouched ---
    name: Option<String>,
    title: Option<String>,
    icon: Option<String>,
    active_color: Option<String>,
    inactive_color: Option<String>,
    default_status: Option<String>,
    default_color: Option<String>,
}

impl EntryBuffer {
    fn from_entry(entry: &IndicatorTemplateEntry) -> Self {
        Self {
            id: entry.id.clone(),
            enabled: entry.enabled,
            icon_ref: entry.icon_ref.clone(),
            inactive_icon_ref: entry.inactive_icon_ref.clone(),
            states: entry.states.clone(),
            name: entry.name.clone(),
            title: entry.title.clone(),
            icon: entry.icon.clone(),
            active_color: entry.active_color.clone(),
            inactive_color: entry.inactive_color.clone(),
            default_status: entry.default_status.clone(),
            default_color: entry.default_color.clone(),
        }
    }

    fn empty(id: &str) -> Self {
        Self {
            id: id.trim().to_string(),
            enabled: true,
            icon_ref: None,
            inactive_icon_ref: None,
            states: Vec::new(),
            name: None,
            title: None,
            icon: None,
            active_color: None,
            inactive_color: None,
            default_status: None,
            default_color: None,
        }
    }

    fn to_entry(&self) -> IndicatorTemplateEntry {
        IndicatorTemplateEntry {
            id: self.id.trim().to_string(),
            name: self.name.clone(),
            title: self.title.clone(),
            icon: self.icon.clone(),
            icon_ref: self.icon_ref.clone(),
            inactive_icon_ref: self.inactive_icon_ref.clone(),
            inactive_color: self.inactive_color.clone(),
            active_color: self.active_color.clone(),
            default_status: self.default_status.clone(),
            default_color: self.default_color.clone(),
            states: self.states.clone(),
            enabled: self.enabled,
        }
    }

    /// Label for the left-hand list: the id, or a placeholder for a brand-new
    /// unnamed row.
    fn list_label(&self) -> String {
        if self.id.trim().is_empty() {
            "(unnamed)".to_string()
        } else {
            self.id.trim().to_string()
        }
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_indicator_templates_editor(&mut self) {
        if self.indicator_templates_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_indicator_templates"));
            return;
        }
        let entries: Vec<EntryBuffer> = Config::list_indicator_templates()
            .iter()
            .map(EntryBuffer::from_entry)
            .collect();
        self.indicator_templates_editor = Some(IndicatorTemplatesEditorState {
            entries,
            selected: 0,
            new_id: String::new(),
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

        // Keep the selection in range (entries can shrink via Remove).
        if state.selected >= state.entries.len() {
            state.selected = state.entries.len().saturating_sub(1);
        }

        // Mirror the working editor windows (keybinds/colors): a fixed
        // default height and ONE outer scroll area with
        // auto_shrink([false,false]). That fills exactly default_height and
        // never grows — the earlier per-pane scroll areas let egui's window
        // resize logic balloon the frame to full screen height.
        egui::Window::new("Indicator Icons")
            .id(egui::Id::new("gui_indicator_templates"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(640.0)
            .default_height(480.0)
            .show(ctx, |ui| {
                if ui.button("Save all").clicked() {
                    save_request = true;
                }
                ui.weak(
                    "Give each status an image icon, and add conditions that swap the \
                     icon by game state. Text glyphs and colors are the TUI's — edit \
                     those with .indicators in the TUI.",
                );
                ui.separator();

                // Two panes: the indicator list (left) and the image/condition
                // editor for the selected one (right). The whole two-pane row
                // lives inside a FIXED-SIZE allocation, so no matter how tall
                // the inner content or scroll state gets, the row reports
                // exactly PANE_HEIGHT back to the window — the window can never
                // grow itself from this content (the earlier bug: content
                // height fed the resizable window's remembered size and it
                // ballooned to full screen). Each pane scrolls within the box.
                const PANE_HEIGHT: f32 = 320.0;
                let pane_size = egui::vec2(ui.available_width(), PANE_HEIGHT);
                ui.allocate_ui(pane_size, |ui| {
                ui.horizontal_top(|ui| {
                    // Left: indicator list + add row.
                    ui.vertical(|ui| {
                        ui.set_width(200.0);
                        ui.strong("Indicators");
                        egui::ScrollArea::vertical()
                            .id_salt("indicator_list_scroll")
                            .max_height(PANE_HEIGHT)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for index in 0..state.entries.len() {
                                    let (label, enabled) = {
                                        let e = &state.entries[index];
                                        (e.list_label(), e.enabled)
                                    };
                                    ui.horizontal(|ui| {
                                        let mut on = enabled;
                                        if ui
                                            .checkbox(&mut on, "")
                                            .on_hover_text(
                                                "Enabled: disabled indicators are skipped \
                                                 when building indicator windows.",
                                            )
                                            .changed()
                                        {
                                            state.entries[index].enabled = on;
                                        }
                                        if ui
                                            .selectable_label(state.selected == index, label)
                                            .clicked()
                                        {
                                            state.selected = index;
                                        }
                                    });
                                }
                            });
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut state.new_id)
                                    .hint_text("new id")
                                    .desired_width(120.0),
                            );
                            if ui.button("Add").clicked() {
                                let id = state.new_id.trim().to_ascii_uppercase();
                                if id.is_empty() {
                                    state.error = Some("Enter an indicator id.".to_string());
                                } else if state
                                    .entries
                                    .iter()
                                    .any(|e| e.id.eq_ignore_ascii_case(&id))
                                {
                                    state.error = Some(format!("'{id}' already exists."));
                                } else {
                                    state.entries.push(EntryBuffer::empty(&id));
                                    state.selected = state.entries.len() - 1;
                                    state.new_id.clear();
                                    state.error = None;
                                }
                            }
                        });
                    });

                    ui.separator();

                    // Right: the selected indicator's image editor.
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("indicator_editor_scroll")
                            .max_height(PANE_HEIGHT)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if let Some(entry) = state.entries.get_mut(state.selected) {
                                    ui.horizontal(|ui| {
                                        ui.strong("Id");
                                        ui.add(
                                            egui::TextEdit::singleline(&mut entry.id)
                                                .desired_width(160.0),
                                        );
                                        if ui.button("Remove indicator").clicked() {
                                            remove_index = Some(state.selected);
                                        }
                                    });
                                    ui.separator();
                                    // Active (Y) icon: shown when the game
                                    // reports this indicator active. "Default
                                    // (by id)" falls back to the built-in
                                    // pictogram / skin sprite named for the id.
                                    Self::render_icon_picker_row(
                                        ui,
                                        &format!("status_active_icon_{}", entry.id),
                                        "Active icon (Y)",
                                        &mut entry.icon_ref,
                                        Some("Default (by id)"),
                                        Some("None (no art)"),
                                        &pool_images,
                                        &sheets,
                                        art.as_deref(),
                                    );
                                    // Inactive (N) icon: shown when the game
                                    // reports this indicator inactive. Defaults
                                    // to no image — inactive art is opt-in, not
                                    // a dimmed copy of the active icon.
                                    Self::render_icon_picker_row(
                                        ui,
                                        &format!("status_inactive_icon_{}", entry.id),
                                        "Inactive icon (N)",
                                        &mut entry.inactive_icon_ref,
                                        Some("None (blank)"),
                                        None,
                                        &pool_images,
                                        &sheets,
                                        art.as_deref(),
                                    );
                                    ui.weak(
                                        "Active shows when the game reports this status on \
                                         (Y); inactive when off (N). Inactive is blank unless \
                                         you set an image. Conditions below override both.",
                                    );
                                    ui.separator();
                                    ui.strong("Conditions (first match wins)");
                                    ui.weak(
                                        "Each condition that matches the game state can \
                                         show a different image. Checked top to bottom; \
                                         the first match's icon is used.",
                                    );
                                    Self::render_status_states(
                                        ui,
                                        &entry.id,
                                        &mut entry.states,
                                        &pool_images,
                                        &sheets,
                                        art.as_deref(),
                                        &suggestions,
                                    );
                                } else {
                                    ui.weak("Add or select an indicator to edit its icon.");
                                }
                            });
                    });
                });
                }); // close the fixed-size allocate_ui around the two panes

                if let Some(error) = &state.error {
                    ui.separator();
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }

                // Global image-art controls (GUI-only): the icon set, the
                // inactive-grayscale toggle, and per-id icon/gray overrides.
                ui.separator();
                egui::CollapsingHeader::new("Global icon art")
                    .id_salt("indicator_global_art")
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("indicator_global_art_scroll")
                            .max_height(180.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Icon set");
                                    let selected = current_set.as_deref().unwrap_or("None");
                                    egui::ComboBox::from_id_salt("statusicon_set")
                                        .selected_text(selected)
                                        .show_ui(ui, |ui| {
                                            if ui
                                                .selectable_label(current_set.is_none(), "None")
                                                .clicked()
                                            {
                                                set_change = Some(None);
                                            }
                                            for set in &icon_sets {
                                                let is_current =
                                                    current_set.as_deref() == Some(set.as_str());
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
                                        "Inactive statuses show a desaturated copy of their \
                                         icon instead of fading it. Grayscale copies are \
                                         built only while this is on.",
                                    )
                                    .changed()
                                {
                                    gray_change = Some(gray);
                                }
                                for (id, on) in &sorted_gray_overrides {
                                    ui.horizontal(|ui| {
                                        ui.monospace(id);
                                        let mut on = *on;
                                        egui::ComboBox::from_id_salt(format!(
                                            "statusicon_gray_{id}"
                                        ))
                                        .width(140.0)
                                        .selected_text(if on { "Grayscale" } else { "Alpha dim" })
                                        .show_ui(ui, |ui| {
                                            if ui.selectable_label(on, "Grayscale").clicked()
                                                && !on
                                            {
                                                on = true;
                                                gray_override_changes
                                                    .push((id.clone(), Some(true)));
                                            }
                                            if ui.selectable_label(!on, "Alpha dim").clicked()
                                                && on
                                            {
                                                on = false;
                                                gray_override_changes
                                                    .push((id.clone(), Some(false)));
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
                                            .hint_text("id for a grayscale exception")
                                            .desired_width(160.0),
                                    );
                                    if ui.button("Add grayscale exception").clicked()
                                        && !state.new_gray_id.trim().is_empty()
                                    {
                                        gray_override_changes.push((
                                            state.new_gray_id.trim().to_ascii_uppercase(),
                                            Some(!current_gray),
                                        ));
                                        state.new_gray_id.clear();
                                    }
                                });
                                ui.separator();
                                ui.weak("Per-id icon overrides");
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
                                        if let crate::data::IconRef::SheetCell { sheet, cell } =
                                            icon
                                        {
                                            let max = art
                                                .as_ref()
                                                .and_then(|a| a.sheet_cell_count(sheet))
                                                .unwrap_or(u32::MAX)
                                                .max(1);
                                            let mut value = *cell;
                                            if ui
                                                .add(
                                                    egui::DragValue::new(&mut value)
                                                        .range(1..=max)
                                                        .prefix("#"),
                                                )
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
                                            .hint_text("id (e.g. STUNNED)")
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
                            });
                    });
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

    /// One labelled image-icon picker row: the IconRef combo, an inline
    /// `#cell` spinner for sheet refs, and a collapsing visual cell grid.
    /// Shared by the Active (Y) and Inactive (N) icon rows.
    #[allow(clippy::too_many_arguments)]
    fn render_icon_picker_row(
        ui: &mut egui::Ui,
        id_salt: &str,
        label: &str,
        icon: &mut Option<crate::data::IconRef>,
        unset_label: Option<&str>,
        none_label: Option<&str>,
        pool_images: &[(String, String)],
        sheets: &[String],
        art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) {
        ui.horizontal(|ui| {
            ui.label(label);
            match super::icon_ref_picker(
                ui,
                format!("{id_salt}_picker"),
                icon.as_ref(),
                pool_images,
                sheets,
                unset_label,
                None,
                none_label,
            ) {
                Some(super::IconRefPick::Unset) => *icon = None,
                Some(super::IconRefPick::Ref(picked)) => *icon = Some(picked),
                None => {}
            }
            if let Some(crate::data::IconRef::SheetCell { cell, .. }) = icon {
                let mut value = (*cell).max(1);
                if ui
                    .add(egui::DragValue::new(&mut value).range(1..=9999).prefix("#"))
                    .changed()
                {
                    *cell = value;
                }
            }
        });
        // Visual cell grid for sheet refs — click a sprite instead of typing
        // its number (same picker as the hotbar editor).
        if let (Some(art), Some(crate::data::IconRef::SheetCell { sheet, cell })) = (art, icon) {
            if let Some(count) = art.sheet_cell_count(sheet) {
                egui::CollapsingHeader::new("Pick cell from sheet")
                    .id_salt(format!("{id_salt}_grid_header"))
                    .show(ui, |ui| {
                        super::sheet_cell_grid(ui, id_salt, sheet, cell, art, count, false);
                    });
            }
        }
    }

    /// Condition-driven status icon states for one template (GUI: image
    /// only). Each state gets the shared condition builder and an IconRef
    /// picker (statusicons pool / sheet cell / template-default / none);
    /// first match wins. Reorder + remove + add. The state's TUI text glyph
    /// and color are not shown here (TUI editor's job) and are preserved
    /// untouched — this fn never writes `st.text`/`st.color`.
    fn render_status_states(
        ui: &mut egui::Ui,
        entry_id: &str,
        states: &mut Vec<crate::config::StatusIconState>,
        pool_images: &[(String, String)],
        sheets: &[String],
        art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
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
            });
            // Visual cell grid for this state's sheet icon (same picker as
            // the default icon and the hotbar editor).
            if let (Some(art), Some(crate::data::IconRef::SheetCell { sheet, cell })) =
                (art, &mut st.icon)
            {
                if let Some(count) = art.sheet_cell_count(sheet) {
                    egui::CollapsingHeader::new("Pick cell from sheet")
                        .id_salt(format!("status_state_grid_{entry_id}_{idx}"))
                        .show(ui, |ui| {
                            super::sheet_cell_grid(
                                ui,
                                &format!("status_state_{entry_id}_{idx}"),
                                sheet,
                                cell,
                                art,
                                count,
                                false,
                            );
                        });
                }
            }
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
