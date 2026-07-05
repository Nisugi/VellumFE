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
    sound_enabled: bool,
    sound_volume: f32,
    sound_cooldown_ms: u64,
    active_theme: String,
    theme_names: Vec<String>,
}

impl SettingsEditorState {
    fn from_config(config: &crate::config::Config, theme_names: Vec<String>) -> Self {
        Self {
            host: config.connection.host.clone(),
            port: config.connection.port,
            character: config.connection.character.clone().unwrap_or_default(),
            buffer_size: config.ui.buffer_size,
            border_style: config.ui.border_style.clone(),
            countdown_icon: config.ui.countdown_icon.clone(),
            min_command_length: config.ui.min_command_length,
            sound_enabled: config.sound.enabled,
            sound_volume: config.sound.volume,
            sound_cooldown_ms: config.sound.cooldown_ms,
            active_theme: config.active_theme.clone(),
            theme_names,
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
        config.ui.buffer_size = self.buffer_size;
        config.ui.border_style = self.border_style.clone();
        config.ui.countdown_icon = self.countdown_icon.clone();
        config.ui.min_command_length = self.min_command_length;
        config.sound.enabled = self.sound_enabled;
        config.sound.volume = self.sound_volume;
        config.sound.cooldown_ms = self.sound_cooldown_ms;
        config.active_theme = self.active_theme.clone();
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
        ));
    }

    pub(in super::super) fn render_settings_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.settings_editor.take() else {
            return;
        };

        let mut open = true;
        let mut saved = false;
        let mut cancelled = false;
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

        if saved {
            state.apply_to_config(&mut self.app_core.config);
            match self.app_core.save_config() {
                Ok(()) => self.app_core.add_system_message("Settings saved."),
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to save settings: {}", err)),
            }
            // Theme changes take effect via apply_theme_if_changed next frame.
            self.settings_editor = None;
            return;
        }

        if open && !cancelled {
            self.settings_editor = Some(state);
        }
    }
}
