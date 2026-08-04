//! The **Windows** window — the single manager for every window the client
//! can have. Opened from the top-bar "Windows" button.
//!
//! Catalog model: the list is the FULL universe (every template for this
//! game + layout windows + session containers/dialogs), grouped into
//! collapsible categories that spawn collapsed. Each row: a show/hide
//! checkbox (ticking a never-added template conjures it; hiding drives
//! core WindowVisibility, which also suppresses game auto-spawn) and a
//! zone dropdown (live windows move immediately; hidden ones store a
//! pending zone applied when they materialize — default Center-ish per
//! widget). "Custom window…" creates blank custom widgets; stock windows
//! are already rows.

use crate::config::WidgetCategory;
use crate::frontend::gui::app::zones::GuiShellZone;
use crate::frontend::gui::app::VellumGuiApp;

/// Minimal editor state: just an open flag (the data lives on the layout /
/// ui_state, so there's nothing to buffer).
pub(in super::super) struct KnownWindowsEditorState;

/// One resolved row for rendering.
struct Row {
    name: String,
    title: String,
    category: WidgetCategory,
    widget_type: String,
    shown: bool,
    /// Zone shown in the dropdown: the live tab's zone, else the stored
    /// pending preference, else the widget-type default.
    zone_display: GuiShellZone,
    ephemeral: bool,
}

/// One window group resolved for the Groups section: the joined member
/// titles, the member window names (render order), and the leader TabKey
/// used to dissolve the whole group.
struct GroupView {
    title: String,
    member_names: Vec<String>,
    leader: crate::frontend::gui::TabKey,
}

/// Blank custom widgets offered by "Custom window…", label → seed template
/// (the `*_custom` templates excluded from the catalog rows).
const CUSTOM_SEEDS: &[(&str, &str)] = &[
    ("Text", "text_custom"),
    ("Tabbed text", "tabbedtext_custom"),
    ("Progress bar", "progress_custom"),
    ("Countdown", "countdown_custom"),
    ("Entity list", "entity_custom"),
    ("Active effects", "active_effects_custom"),
];

impl VellumGuiApp {
    pub(in super::super) fn open_known_windows_editor(&mut self) {
        if self.known_windows_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_known_windows"));
            return;
        }
        self.known_windows_editor = Some(KnownWindowsEditorState);
    }

    pub(in super::super) fn render_known_windows_editor(&mut self, ctx: &egui::Context) {
        if self.known_windows_editor.is_none() {
            return;
        }
        let mut open = true;

        // Resolve rows: the full catalog + live zone / pending pref.
        let mut rows: Vec<Row> = self
            .app_core
            .enumerate_known_windows()
            .into_iter()
            .map(|k| {
                let zone_display = self
                    .find_tab_key_by_name(&k.name)
                    .map(|key| self.zone_for_tab(&key))
                    .or_else(|| self.pending_zones.get(&k.name).copied())
                    .unwrap_or_else(|| Self::default_zone_for_widget_type(&k.widget_type));
                Row {
                    category: WidgetCategory::from_widget_type(&k.widget_type),
                    name: k.name,
                    title: k.title,
                    widget_type: k.widget_type,
                    shown: k.shown,
                    zone_display,
                    ephemeral: k.ephemeral,
                }
            })
            .collect();
        rows.sort_by(|a, b| a.title.to_ascii_lowercase().cmp(&b.title.to_ascii_lowercase()));

        // Window groups (locked-together windows rendered as one). Resolve
        // each group to its member window names in render order so the Groups
        // section can nest them; grouped names are excluded from the flat
        // category list below so a window never appears twice.
        let groups: Vec<GroupView> = self
            .tab_groups
            .iter()
            .filter_map(|group| {
                let members: Vec<String> = group
                    .members
                    .iter()
                    .filter_map(|key| {
                        self.available_tabs
                            .get(key)
                            .map(|tab| tab.window_name.clone())
                    })
                    .collect();
                // A leader key we can hand back to dissolve the whole group.
                let leader = group.members.first().cloned();
                (members.len() >= 2 && leader.is_some()).then(|| GroupView {
                    title: members.join(" + "),
                    member_names: members,
                    leader: leader.unwrap(),
                })
            })
            .collect();
        let grouped_names: std::collections::HashSet<String> = groups
            .iter()
            .flat_map(|g| g.member_names.iter().cloned())
            .collect();

        // Actions chosen this frame (applied after the closure so it doesn't
        // borrow self mutably while rendering).
        let mut toggle: Option<(String, bool)> = None;
        let mut zone_change: Option<(String, GuiShellZone)> = None;
        let mut add_template: Option<String> = None;
        let mut ungroup: Option<crate::frontend::gui::TabKey> = None;
        let mut restore_deleted: Option<String> = None;

        // Windows the user deleted (not hidden) that can be restored:
        // (internal name, display title).
        let deleted_windows = self.app_core.deleted_windows_for_restore();

        egui::Window::new("Windows")
            .id(egui::Id::new("gui_known_windows"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(380.0)
            .default_height(480.0)
            .show(ctx, |ui| {
                ui.label(
                    "Every window the client can have. Tick to show, untick \
                     to hide — hidden windows stay hidden even when the game \
                     re-sends them. The zone dropdown places a window; set it \
                     on a hidden window and it appears there when shown.",
                );

                ui.horizontal(|ui| {
                    ui.menu_button("➕ Custom window…", |ui| {
                        for (label, template) in CUSTOM_SEEDS {
                            if ui.button(*label).clicked() {
                                add_template = Some((*template).to_string());
                                ui.close();
                            }
                        }
                    });
                    // Restore a window the user deleted (distinct from hide):
                    // its exact def — position, streams, widget type — comes
                    // back. Only shown when something was deleted.
                    if !deleted_windows.is_empty() {
                        ui.menu_button("↩ Restore deleted…", |ui| {
                            for (name, title) in &deleted_windows {
                                // Show the human title; restore by the stable id.
                                if ui.button(title).clicked() {
                                    restore_deleted = Some(name.clone());
                                    ui.close();
                                }
                            }
                        });
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Groups first: clustered windows, nested under their
                    // group, so the parent/child relationship is visible and
                    // they don't scatter across the category list below.
                    if !groups.is_empty() {
                        egui::CollapsingHeader::new("Groups")
                            .id_salt("known_windows_groups")
                            .default_open(true)
                            .show(ui, |ui| {
                                for group in &groups {
                                    ui.horizontal(|ui| {
                                        ui.strong(&group.title);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .button("Ungroup")
                                                    .on_hover_text(
                                                        "Break this group into standalone windows.",
                                                    )
                                                    .clicked()
                                                {
                                                    ungroup = Some(group.leader.clone());
                                                }
                                            },
                                        );
                                    });
                                    ui.indent(("group_members", &group.title), |ui| {
                                        for name in &group.member_names {
                                            if let Some(row) =
                                                rows.iter().find(|r| &r.name == name)
                                            {
                                                Self::known_window_row(
                                                    ui,
                                                    row,
                                                    &mut toggle,
                                                    &mut zone_change,
                                                );
                                            }
                                        }
                                    });
                                }
                            });
                    }

                    for category in WidgetCategory::ALL {
                        let group: Vec<&Row> = rows
                            .iter()
                            .filter(|r| r.category == category)
                            // Grouped windows live in the Groups section only.
                            .filter(|r| !grouped_names.contains(&r.name))
                            .collect();
                        if group.is_empty() {
                            continue;
                        }
                        egui::CollapsingHeader::new(category.display_name())
                            .id_salt(category.display_name())
                            .default_open(false)
                            .show(ui, |ui| {
                                // Status: dozens of indicators go into their
                                // own nested collapse so the dashboard row
                                // isn't buried.
                                let (indicators, plain): (Vec<&&Row>, Vec<&&Row>) =
                                    group.iter().partition(|r| {
                                        category == WidgetCategory::Status
                                            && r.widget_type == "indicator"
                                    });
                                for row in plain {
                                    Self::known_window_row(
                                        ui,
                                        row,
                                        &mut toggle,
                                        &mut zone_change,
                                    );
                                }
                                if !indicators.is_empty() {
                                    egui::CollapsingHeader::new("Indicators")
                                        .id_salt("known_windows_indicators")
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            for row in indicators {
                                                Self::known_window_row(
                                                    ui,
                                                    row,
                                                    &mut toggle,
                                                    &mut zone_change,
                                                );
                                            }
                                        });
                                }
                            });
                    }
                });
            });

        // Apply deferred actions.
        let (w, h) = self.core_layout_size;
        if let Some((name, shown)) = toggle {
            self.app_core.set_known_window_shown(&name, shown, w, h);
        }
        if let Some((name, zone)) = zone_change {
            if let Some(key) = self.find_tab_key_by_name(&name) {
                self.set_tab_zone(key, zone);
            }
            // Remember the preference either way: it survives hide/show and
            // seeds the tab when a hidden window materializes.
            self.pending_zones.insert(name, zone);
            self.layout_dirty = true;
        }
        if let Some(template) = add_template {
            self.add_window_from_template(&template);
        }
        if let Some(name) = restore_deleted {
            self.app_core.restore_deleted_window(&name, w, h);
            self.layout_dirty = true;
        }
        if let Some(leader) = ungroup {
            self.dissolve_group_of(&leader);
        }
        if !open {
            self.known_windows_editor = None;
        }
    }

    /// One catalog row: show/hide checkbox + zone dropdown.
    fn known_window_row(
        ui: &mut egui::Ui,
        row: &Row,
        toggle: &mut Option<(String, bool)>,
        zone_change: &mut Option<(String, GuiShellZone)>,
    ) {
        ui.horizontal(|ui| {
            let mut checked = row.shown;
            let mut label = row.title.clone();
            if row.ephemeral {
                label.push_str("  (session)");
            }
            if ui.checkbox(&mut checked, label).changed() {
                *toggle = Some((row.name.clone(), checked));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt(("known_windows_zone", &row.name))
                    .selected_text(row.zone_display.label())
                    .show_ui(ui, |ui| {
                        for target in GuiShellZone::all() {
                            if ui
                                .selectable_label(target == row.zone_display, target.label())
                                .clicked()
                                && target != row.zone_display
                            {
                                *zone_change = Some((row.name.clone(), target));
                            }
                        }
                    });
            });
        });
    }
}
