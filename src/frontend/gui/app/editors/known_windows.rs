//! The **Windows** window — the single manager for every window the client
//! knows about. Opened from the top-bar "Windows" button. U6: merges the
//! old zone menu + the show/hide checkbox panel into one window.
//!
//! Each row: a show/hide checkbox (drives core WindowVisibility — Hidden
//! also suppresses game auto-spawn/popup), a Zone selector (placement), and
//! rows are grouped into collapsible WidgetCategory sections. An "Add
//! Custom" section at the top adds a new custom window via the editor.

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
    shown: bool,
    /// Its live tab + zone, when it has one (shown windows do).
    zone: Option<GuiShellZone>,
    ephemeral: bool,
}

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

        // Resolve rows: every known window + its category + (zone if live).
        let mut rows: Vec<Row> = self
            .app_core
            .enumerate_known_windows()
            .into_iter()
            .map(|k| {
                let zone = self
                    .find_tab_key_by_name(&k.name)
                    .map(|key| self.zone_for_tab(&key));
                Row {
                    category: WidgetCategory::from_widget_type(&k.widget_type),
                    name: k.name,
                    title: k.title,
                    shown: k.shown,
                    zone,
                    ephemeral: k.ephemeral,
                }
            })
            .collect();
        rows.sort_by(|a, b| a.title.to_ascii_lowercase().cmp(&b.title.to_ascii_lowercase()));

        // Add-window templates grouped by category (for the Add section).
        let add_groups = self.app_core.addable_window_templates();

        // Actions chosen this frame (applied after the closure so it doesn't
        // borrow self mutably while rendering).
        let mut toggle: Option<(String, bool)> = None;
        let mut zone_change: Option<(String, GuiShellZone)> = None;
        let mut add_template: Option<String> = None;

        egui::Window::new("Windows")
            .id(egui::Id::new("gui_known_windows"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(360.0)
            .default_height(480.0)
            .show(ctx, |ui| {
                ui.label(
                    "Every window the client knows about. Tick to show, untick \
                     to hide. Hidden windows stay hidden even when the game \
                     re-sends them.",
                );

                // Add Custom / Add Window section.
                ui.menu_button("➕ Add window…", |ui| {
                    if add_groups.is_empty() {
                        ui.label("All windows already added");
                    }
                    for (category, entries) in &add_groups {
                        ui.menu_button(category, |ui| {
                            for (template_name, display_name) in entries {
                                if ui.button(display_name).clicked() {
                                    add_template = Some(template_name.clone());
                                    ui.close();
                                }
                            }
                        });
                    }
                });
                ui.separator();

                if rows.is_empty() {
                    ui.weak("No windows yet — they appear as the game sends them.");
                    return;
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Group rows by widget category, collapsible.
                    for category in WidgetCategory::ALL {
                        let group: Vec<&Row> =
                            rows.iter().filter(|r| r.category == category).collect();
                        if group.is_empty() {
                            continue;
                        }
                        egui::CollapsingHeader::new(category.display_name())
                            .id_salt(category.display_name())
                            .default_open(true)
                            .show(ui, |ui| {
                                for row in group {
                                    ui.horizontal(|ui| {
                                        let mut checked = row.shown;
                                        let mut label = row.title.clone();
                                        if row.ephemeral {
                                            label.push_str("  (session)");
                                        }
                                        if ui.checkbox(&mut checked, label).changed() {
                                            toggle = Some((row.name.clone(), checked));
                                        }
                                        // Zone selector only for windows that
                                        // are live (have a tab); hidden ones
                                        // get one once shown.
                                        if let Some(zone) = row.zone {
                                            ui.menu_button(
                                                format!("Zone: {}", zone.label()),
                                                |ui| {
                                                    for target in GuiShellZone::all() {
                                                        let cur = target == zone;
                                                        if ui
                                                            .selectable_label(
                                                                cur,
                                                                target.label(),
                                                            )
                                                            .clicked()
                                                            && !cur
                                                        {
                                                            zone_change = Some((
                                                                row.name.clone(),
                                                                target,
                                                            ));
                                                            ui.close();
                                                        }
                                                    }
                                                },
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
        }
        if let Some(template) = add_template {
            self.add_window_from_template(&template);
        }
        if !open {
            self.known_windows_editor = None;
        }
    }
}
