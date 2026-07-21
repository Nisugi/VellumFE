//! Settings editor: edits the shared Config (connection, UI, sound, theme)
//! and saves through `AppCore::save_config`, mirroring the TUI settings
//! editor's field list where the setting applies to the GUI.

use super::super::VellumGuiApp;
use eframe::egui;

const BORDER_STYLES: &[&str] = &[
    "single",
    "double",
    "rounded",
    "thick",
    "quadrant_inside",
    "quadrant_outside",
];

/// Buffered edit state, initialized from Config when the editor opens and
/// applied back on Save.
pub(in super::super) struct SettingsEditorState {
    host: String,
    port: u16,
    character: String,
    buffer_size: usize,
    border_style: String,
    countdown_icon: String,
    min_command_length: usize,
    lich_dir: String,
    mapdb_path: String,
    mapdb_repo: String,
    mapping_mode: bool,
    go2_native_map_clicks: bool,
    sound_enabled: bool,
    sound_volume: f32,
    sound_cooldown_ms: u64,
    active_theme: String,
    theme_names: Vec<String>,
    /// Selected skin; NONE_SKIN sentinel = no skin.
    active_skin: String,
    skin_names: Vec<String>,
    /// Buffer for the "New skin" name field.
    new_skin_name: String,
    /// Inline error from the last "Create" attempt.
    skin_error: Option<String>,
    /// GUI sizing settings; persisted in the per-character GUI layout file,
    /// not config.toml.
    gui_settings: crate::frontend::gui::persistence::GuiUiSettings,
}

/// ComboBox entry meaning "no skin active".
const NONE_SKIN: &str = "(none)";

impl SettingsEditorState {
    fn from_config(
        config: &crate::config::Config,
        theme_names: Vec<String>,
        skin_names: Vec<String>,
        gui_settings: crate::frontend::gui::persistence::GuiUiSettings,
    ) -> Self {
        Self {
            host: config.connection.host.clone(),
            port: config.connection.port,
            character: config.connection.character.clone().unwrap_or_default(),
            lich_dir: config.map.lich_dir.clone().unwrap_or_default(),
            mapdb_path: config.map.mapdb_path.clone().unwrap_or_default(),
            mapdb_repo: config.map.mapdb_repo.clone(),
            mapping_mode: config.map.mapping_mode,
            go2_native_map_clicks: config.go2.native_map_clicks,
            buffer_size: config.ui.buffer_size,
            border_style: config.ui.border_style.clone(),
            countdown_icon: config.ui.countdown_icon.clone(),
            min_command_length: config.ui.min_command_length,
            sound_enabled: config.sound.enabled,
            sound_volume: config.sound.volume,
            sound_cooldown_ms: config.sound.cooldown_ms,
            active_theme: config.active_theme.clone(),
            theme_names,
            active_skin: config
                .active_skin
                .clone()
                .unwrap_or_else(|| NONE_SKIN.to_string()),
            skin_names,
            new_skin_name: String::new(),
            skin_error: None,
            gui_settings,
        }
    }

    fn apply_to_config(&self, config: &mut crate::config::Config) {
        config.connection.host = self.host.clone();
        config.connection.port = self.port;
        config.connection.character = if self.character.trim().is_empty() {
            None
        } else {
            Some(self.character.trim().to_string())
        };
        config.map.lich_dir = match self.lich_dir.trim() {
            "" => None,
            dir => Some(dir.to_string()),
        };
        config.map.mapdb_path = match self.mapdb_path.trim() {
            "" => None,
            path => Some(path.to_string()),
        };
        config.map.mapdb_repo = self.mapdb_repo.trim().to_string();
        config.map.mapping_mode = self.mapping_mode;
        config.go2.native_map_clicks = self.go2_native_map_clicks;
        config.ui.buffer_size = self.buffer_size;
        config.ui.border_style = self.border_style.clone();
        config.ui.countdown_icon = self.countdown_icon.clone();
        config.ui.min_command_length = self.min_command_length;
        config.sound.enabled = self.sound_enabled;
        config.sound.volume = self.sound_volume;
        config.sound.cooldown_ms = self.sound_cooldown_ms;
        config.active_theme = self.active_theme.clone();
        config.active_skin = if self.active_skin == NONE_SKIN {
            None
        } else {
            Some(self.active_skin.clone())
        };
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_settings_editor(&mut self) {
        let mut theme_names: Vec<String> = crate::theme::ThemePresets::all_with_custom(
            self.app_core.config.character.as_deref(),
        )
        .into_keys()
        .collect();
        theme_names.sort();
        self.settings_editor = Some(SettingsEditorState::from_config(
            &self.app_core.config,
            theme_names,
            crate::frontend::gui::skin::list_skins(),
            self.ui_settings.clone(),
        ));
    }

    pub(in super::super) fn render_settings_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.settings_editor.take() else {
            return;
        };

        let mut open = true;
        let mut saved = false;
        let mut cancelled = false;
        // Map download actions fire immediately (they're actions, not
        // settings); the repo text itself still applies on Save.
        let mut map_download_repo: Option<String> = None;
        let mut map_remove_clicked = false;
        let updater_status = self.app_core.map_updater.status.clone();
        let updater_installed = self.app_core.map_updater.installed.clone();
        let updater_in_flight = self.app_core.map_updater.in_flight();
        let saved_target_count = self.app_core.config.go2.saved.len();
        egui::Window::new("Settings")
            .id(egui::Id::new("gui_settings_editor"))
            .open(&mut open)
            .default_width(380.0)
            .collapsible(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("settings_editor_scroll")
                    .show(ui, |ui| {
                        ui.collapsing("Connection", |ui| {
                            ui.label("Connection settings are always character-specific.");
                            egui::Grid::new("settings_connection_grid")
                                .num_columns(2)
                                .show(ui, |ui| {
                                    ui.label("Host");
                                    ui.text_edit_singleline(&mut state.host);
                                    ui.end_row();
                                    ui.label("Port");
                                    ui.add(egui::DragValue::new(&mut state.port));
                                    ui.end_row();
                                    ui.label("Character");
                                    ui.text_edit_singleline(&mut state.character);
                                    ui.end_row();
                                });
                        });

                        ui.collapsing("Map", |ui| {
                            ui.label(
                                "Map data comes from a downloaded release or your Lich install                                  (data/<game>/map-<timestamp>.json); downloaded data wins.",
                            );
                            egui::Grid::new("settings_map_grid").num_columns(2).show(
                                ui,
                                |ui| {
                                    ui.label("Map data repo");
                                    ui.text_edit_singleline(&mut state.mapdb_repo)
                                        .on_hover_text(
                                            "GitHub owner/repo whose releases attach a mapdb.json asset",
                                        );
                                    ui.end_row();
                                    ui.label("Lich folder");
                                    ui.text_edit_singleline(&mut state.lich_dir)
                                        .on_hover_text("e.g. C:/Gemstone/Lich5");
                                    ui.end_row();
                                    ui.label("mapdb file (override)");
                                    ui.text_edit_singleline(&mut state.mapdb_path)
                                        .on_hover_text(
                                            "Optional explicit map-*.json; overrides both sources above",
                                        );
                                    ui.end_row();
                                },
                            );
                            ui.horizontal(|ui| {
                                let can_download =
                                    !updater_in_flight && !state.mapdb_repo.trim().is_empty();
                                let label = if updater_installed.is_some() {
                                    "Check for update"
                                } else {
                                    "Download map data"
                                };
                                if ui.add_enabled(can_download, egui::Button::new(label)).clicked()
                                {
                                    map_download_repo = Some(state.mapdb_repo.trim().to_string());
                                }
                                if updater_installed.is_some()
                                    && ui
                                        .add_enabled(
                                            !updater_in_flight,
                                            egui::Button::new("Remove downloaded"),
                                        )
                                        .on_hover_text(
                                            "Delete downloaded map data and fall back to the Lich folder",
                                        )
                                        .clicked()
                                {
                                    map_remove_clicked = true;
                                }
                            });
                            use crate::core::mapdb_update::UpdateStatus;
                            let status_text = match &updater_status {
                                UpdateStatus::Idle => match &updater_installed {
                                    Some(tag) => format!("Installed: {tag}"),
                                    None => "No downloaded map data".to_string(),
                                },
                                UpdateStatus::Checking => {
                                    "Checking for the latest release...".to_string()
                                }
                                UpdateStatus::Downloading {
                                    tag,
                                    received,
                                    total,
                                } => match total {
                                    Some(total) => format!(
                                        "Downloading {tag}: {:.1} / {:.1} MB",
                                        *received as f64 / 1e6,
                                        *total as f64 / 1e6
                                    ),
                                    None => format!(
                                        "Downloading {tag}: {:.1} MB",
                                        *received as f64 / 1e6
                                    ),
                                },
                                UpdateStatus::UpToDate { tag } => format!("Up to date ({tag})"),
                                UpdateStatus::Updated { tag } => format!("Installed {tag}"),
                                UpdateStatus::Failed(e) => format!("Update failed: {e}"),
                            };
                            if matches!(updater_status, UpdateStatus::Failed(_)) {
                                ui.colored_label(ui.visuals().error_fg_color, status_text);
                            } else {
                                ui.label(egui::RichText::new(status_text).weak());
                            }
                            ui.separator();
                            ui.checkbox(
                                &mut state.mapping_mode,
                                "Cartography mode (sketch unmapped rooms)",
                            )
                            .on_hover_text(
                                "Draw dashed ghost sketches of unmapped rooms as you explore them. \
                                 Off = the map shows only mapdb truth; unmapped interiors hold the \
                                 last mapped room. Sketches are session-only either way.",
                            );
                        });

                        ui.collapsing("Travel", |ui| {
                            ui.label(
                                "Native .go2 walks the map without Lich — travel with                                  .go2 <room|tag|name>, stop with .go2 stop.",
                            );
                            ui.checkbox(
                                &mut state.go2_native_map_clicks,
                                "Map clicks travel natively",
                            )
                            .on_hover_text(
                                "Off = clicks send ;go2 to Lich instead (its go2 knows silvers, day passes, urchins)",
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Saved targets: {saved_target_count} (manage with .go2 save / .go2 targets)"
                                ))
                                .weak(),
                            );
                        });

                        ui.collapsing("UI", |ui| {
                            egui::Grid::new("settings_ui_grid").num_columns(2).show(
                                ui,
                                |ui| {
                                    ui.label("Buffer size");
                                    ui.add(
                                        egui::DragValue::new(&mut state.buffer_size)
                                            .range(100..=100_000),
                                    );
                                    ui.end_row();
                                    ui.label("Border style");
                                    egui::ComboBox::from_id_salt("settings_border_style")
                                        .selected_text(state.border_style.clone())
                                        .show_ui(ui, |ui| {
                                            for style in BORDER_STYLES {
                                                ui.selectable_value(
                                                    &mut state.border_style,
                                                    style.to_string(),
                                                    *style,
                                                );
                                            }
                                        });
                                    ui.end_row();
                                    ui.label("Countdown icon");
                                    ui.text_edit_singleline(&mut state.countdown_icon);
                                    ui.end_row();
                                    ui.label("Min command length");
                                    ui.add(
                                        egui::DragValue::new(&mut state.min_command_length)
                                            .range(0..=10),
                                    );
                                    ui.end_row();
                                },
                            );
                        });

                        ui.collapsing("GUI", |ui| {
                            ui.label(
                                "Sizing applies to the GUI only and is saved per character. \
                                 Ctrl+= / Ctrl+- / Ctrl+0 also adjust zoom anytime.",
                            );
                            egui::Grid::new("settings_gui_grid").num_columns(2).show(
                                ui,
                                |ui| {
                                    ui.label("UI zoom");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut state.gui_settings.zoom_factor,
                                            0.5..=3.0,
                                        )
                                        .step_by(0.05),
                                    );
                                    ui.end_row();
                                    ui.label("Text size");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut state.gui_settings.text_size,
                                            8.0..=32.0,
                                        )
                                        .step_by(0.5),
                                    );
                                    ui.end_row();
                                    ui.label("Title bar size");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut state.gui_settings.title_font_size,
                                            8.0..=40.0,
                                        )
                                        .step_by(0.5),
                                    );
                                    ui.end_row();
                                    ui.label("Effect bar height");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut state.gui_settings.effects_bar_height,
                                            10.0..=60.0,
                                        )
                                        .step_by(1.0),
                                    );
                                    ui.end_row();
                                    ui.label("Density");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut state.gui_settings.density,
                                            0.5..=2.0,
                                        )
                                        .step_by(0.05),
                                    )
                                    .on_hover_text(
                                        "Spacing and padding scale. Lower = denser \
                                         (Wrayth-like), higher = more comfortable.",
                                    );
                                    ui.end_row();
                                    ui.label("Bar corners");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut state.gui_settings.bar_corner_radius,
                                            0.0..=12.0,
                                        )
                                        .step_by(0.5),
                                    )
                                    .on_hover_text(
                                        "Corner radius for all progress bars. \
                                         0 = square (Wrayth-style).",
                                    );
                                    ui.end_row();
                                    ui.label("Bar text contrast");
                                    ui.checkbox(
                                        &mut state.gui_settings.auto_contrast_bar_text,
                                        "Auto light/dark",
                                    )
                                    .on_hover_text(
                                        "Switch bar text to light or dark when its \
                                         configured color would be unreadable against \
                                         the bar fill.",
                                    );
                                    ui.end_row();
                                },
                            );

                            ui.separator();
                            ui.label("Vitals window");
                            egui::Grid::new("settings_vitals_grid").num_columns(2).show(
                                ui,
                                |ui| {
                                    use crate::frontend::gui::persistence::{
                                        VitalsOrientation, VitalsTextFormat,
                                    };
                                    let vitals = &mut state.gui_settings.vitals;
                                    ui.label("Layout");
                                    egui::ComboBox::from_id_salt("settings_vitals_orientation")
                                        .selected_text(match vitals.orientation {
                                            VitalsOrientation::Horizontal => "One row",
                                            VitalsOrientation::Vertical => "Stacked",
                                        })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut vitals.orientation,
                                                VitalsOrientation::Horizontal,
                                                "One row",
                                            );
                                            ui.selectable_value(
                                                &mut vitals.orientation,
                                                VitalsOrientation::Vertical,
                                                "Stacked",
                                            );
                                        });
                                    ui.end_row();
                                    ui.label("Bar height");
                                    ui.add(
                                        egui::Slider::new(&mut vitals.bar_height, 8.0..=60.0)
                                            .step_by(1.0),
                                    );
                                    ui.end_row();
                                    ui.label("Bar text");
                                    egui::ComboBox::from_id_salt("settings_vitals_text")
                                        .selected_text(match vitals.text_format {
                                            VitalsTextFormat::LabelValueMax => "Health: 191/193",
                                            VitalsTextFormat::LabelPercent => "Health: 99%",
                                            VitalsTextFormat::ValueMax => "191/193",
                                            VitalsTextFormat::Percent => "99%",
                                            VitalsTextFormat::None => "No text",
                                        })
                                        .show_ui(ui, |ui| {
                                            for (format, label) in [
                                                (
                                                    VitalsTextFormat::LabelValueMax,
                                                    "Health: 191/193",
                                                ),
                                                (VitalsTextFormat::LabelPercent, "Health: 99%"),
                                                (VitalsTextFormat::ValueMax, "191/193"),
                                                (VitalsTextFormat::Percent, "99%"),
                                                (VitalsTextFormat::None, "No text"),
                                            ] {
                                                ui.selectable_value(
                                                    &mut vitals.text_format,
                                                    format,
                                                    label,
                                                );
                                            }
                                        });
                                    ui.end_row();
                                },
                            );
                            ui.label("Bars shown:");
                            {
                                use crate::frontend::gui::persistence::VitalKind;
                                let bars = &mut state.gui_settings.vitals.bars;
                                for kind in VitalKind::all() {
                                    let mut enabled = bars.contains(&kind);
                                    if ui.checkbox(&mut enabled, kind.label()).changed() {
                                        if enabled {
                                            bars.push(kind);
                                            // Keep display order canonical regardless
                                            // of toggle order.
                                            bars.sort_by_key(|entry| {
                                                VitalKind::all()
                                                    .iter()
                                                    .position(|k| k == entry)
                                                    .unwrap_or(usize::MAX)
                                            });
                                        } else {
                                            bars.retain(|entry| entry != &kind);
                                        }
                                    }
                                }
                            }
                        });

                        ui.collapsing("Sound", |ui| {
                            ui.checkbox(&mut state.sound_enabled, "Sounds enabled");
                            ui.horizontal(|ui| {
                                ui.label("Volume");
                                ui.add(egui::Slider::new(&mut state.sound_volume, 0.0..=1.0));
                            });
                            ui.horizontal(|ui| {
                                ui.label("Cooldown (ms)");
                                ui.add(
                                    egui::DragValue::new(&mut state.sound_cooldown_ms)
                                        .range(0..=10_000),
                                );
                            });
                        });

                        ui.collapsing("Theme", |ui| {
                            egui::ComboBox::from_id_salt("settings_theme")
                                .selected_text(state.active_theme.clone())
                                .show_ui(ui, |ui| {
                                    for name in &state.theme_names {
                                        ui.selectable_value(
                                            &mut state.active_theme,
                                            name.clone(),
                                            name,
                                        );
                                    }
                                });
                        });

                        ui.collapsing("Skin", |ui| {
                            ui.label("Skins add graphics (backgrounds, borders, widget art) on top of the theme.");
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("settings_skin")
                                    .selected_text(state.active_skin.clone())
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut state.active_skin,
                                            NONE_SKIN.to_string(),
                                            NONE_SKIN,
                                        );
                                        for name in &state.skin_names {
                                            ui.selectable_value(
                                                &mut state.active_skin,
                                                name.clone(),
                                                name,
                                            );
                                        }
                                    });
                                if ui
                                    .button("Open skins folder")
                                    .on_hover_text("Skins live in ~/.vellum-fe/skins/<name>/")
                                    .clicked()
                                {
                                    if let Ok(dir) = crate::config::Config::skins_dir() {
                                        let _ = std::fs::create_dir_all(&dir);
                                        if let Err(err) =
                                            crate::platform::open_url(&dir.to_string_lossy())
                                        {
                                            tracing::warn!(
                                                "Failed to open skins folder {}: {}",
                                                dir.display(),
                                                err
                                            );
                                        }
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.new_skin_name)
                                        .hint_text("new skin name")
                                        .desired_width(140.0),
                                );
                                if ui
                                    .button("Create")
                                    .on_hover_text(
                                        "Write a starter skin.toml (all sections commented out) and select it",
                                    )
                                    .clicked()
                                {
                                    match crate::frontend::gui::skin::write_scaffold(
                                        &state.new_skin_name,
                                    ) {
                                        Ok(_) => {
                                            let name = state.new_skin_name.trim().to_string();
                                            state.skin_names.push(name.clone());
                                            state.skin_names.sort();
                                            state.active_skin = name;
                                            state.new_skin_name.clear();
                                            state.skin_error = None;
                                        }
                                        Err(err) => {
                                            state.skin_error = Some(err.to_string());
                                        }
                                    }
                                }
                            });
                            if let Some(error) = &state.skin_error {
                                ui.colored_label(ui.visuals().error_fg_color, error);
                            }
                        });

                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                saved = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancelled = true;
                            }
                        });
                    });
            });

        if let Some(repo) = map_download_repo {
            self.app_core.start_mapdb_download(&repo);
        }
        if map_remove_clicked {
            self.app_core.remove_downloaded_mapdb();
            self.app_core.add_system_message("Downloaded map data removed.");
        }

        if saved {
            state.apply_to_config(&mut self.app_core.config);
            self.app_core.refresh_map_source();
            match self.app_core.save_config() {
                Ok(()) => self.app_core.add_system_message("Settings saved."),
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to save settings: {}", err)),
            }
            // GUI sizing lives in the per-character layout file; force the
            // zoom/title-bar values to re-apply on the next frame.
            self.ui_settings = state.gui_settings.clone();
            self.zoom_applied = false;
            self.applied_title_font_size = None;
            self.applied_density = None;
            self.layout_dirty = true;
            // Theme changes take effect via apply_theme_if_changed next frame.
            self.settings_editor = None;
            return;
        }

        if open && !cancelled {
            self.settings_editor = Some(state);
        }
    }
}
