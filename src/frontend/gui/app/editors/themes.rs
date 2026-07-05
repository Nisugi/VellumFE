//! Theme browser (.themes) and theme editor (.edittheme).
//!
//! The browser lists built-in presets plus custom themes from the themes/
//! directory with color swatches and one-click apply. The editor edits every
//! `ThemeData` color as hex and saves as a custom theme file, which becomes
//! immediately selectable (and re-applies live if it is the active theme).

use super::super::{theme, VellumGuiApp};
use super::color_field;
use crate::theme::loader::ThemeData;
use crate::theme::{AppTheme, ThemePresets};
use eframe::egui;

pub(in super::super) struct ThemeBrowserState;

pub(in super::super) struct ThemeEditorState {
    data: ThemeData,
    error: Option<String>,
}

fn swatch(ui: &mut egui::Ui, color: crate::frontend::common::Color) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, theme::color32(color));
}

impl VellumGuiApp {
    pub(in super::super) fn open_theme_browser(&mut self) {
        self.theme_browser = Some(ThemeBrowserState);
    }

    pub(in super::super) fn open_theme_editor(&mut self, base: &AppTheme) {
        self.theme_editor = Some(ThemeEditorState {
            data: ThemeData::from_theme(base),
            error: None,
        });
    }

    pub(in super::super) fn render_theme_browser(&mut self, ctx: &egui::Context) {
        if self.theme_browser.is_none() {
            return;
        }

        let mut open = true;
        let mut apply_request: Option<String> = None;
        let mut edit_request: Option<AppTheme> = None;
        let active = self.app_core.config.active_theme.clone();

        egui::Window::new("Themes")
            .id(egui::Id::new("gui_theme_browser"))
            .open(&mut open)
            .default_width(420.0)
            .default_height(380.0)
            .show(ctx, |ui| {
                ui.weak("Apply switches and persists the theme. Edit opens a copy in the theme editor.");
                ui.separator();

                let presets = ThemePresets::all_with_custom(
                    self.app_core.config.character.as_deref(),
                );
                let mut names: Vec<&String> = presets.keys().collect();
                names.sort();

                egui::ScrollArea::vertical()
                    .id_salt("theme_browser_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for name in names {
                            let Some(preset) = presets.get(name) else {
                                continue;
                            };
                            ui.horizontal(|ui| {
                                if ui.small_button("Apply").clicked() {
                                    apply_request = Some(name.clone());
                                }
                                if ui.small_button("Edit").clicked() {
                                    edit_request = Some(preset.clone());
                                }
                                swatch(ui, preset.background_primary);
                                swatch(ui, preset.window_background);
                                swatch(ui, preset.text_primary);
                                swatch(ui, preset.link_color);
                                swatch(ui, preset.status_success);
                                swatch(ui, preset.status_error);
                                let mut label = egui::RichText::new(name);
                                if *name == active {
                                    label = label.strong();
                                }
                                ui.label(label);
                                if *name == active {
                                    ui.weak("(active)");
                                }
                                if !preset.description.is_empty() {
                                    ui.weak(&preset.description);
                                }
                            });
                        }
                    });
            });

        if let Some(name) = apply_request {
            self.apply_theme_by_name(&name);
        }
        if let Some(base) = edit_request {
            self.open_theme_editor(&base);
        }
        if !open {
            self.theme_browser = None;
        }
    }

    pub(in super::super) fn render_theme_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.theme_editor.take() else {
            return;
        };

        let mut open = true;
        let mut save_request = false;
        let mut save_and_apply = false;

        egui::Window::new("Theme Editor")
            .id(egui::Id::new("gui_theme_editor"))
            .open(&mut open)
            .default_width(420.0)
            .default_height(480.0)
            .show(ctx, |ui| {
                egui::Grid::new("theme_editor_meta")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut state.data.name);
                        ui.end_row();
                        ui.label("Description");
                        ui.text_edit_singleline(&mut state.data.description);
                        ui.end_row();
                    });
                ui.weak("Saved as a custom theme in the themes/ directory. Use a new name to avoid shadowing built-ins.");
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("theme_editor_scroll")
                    .auto_shrink([false, false])
                    .max_height(ui.available_height() - 40.0)
                    .show(ui, |ui| {
                        let data = &mut state.data;
                        let sections: [(&str, Vec<(&str, &mut String)>); 10] = [
                            (
                                "Window",
                                vec![
                                    ("Border", &mut data.window_border),
                                    ("Border (focused)", &mut data.window_border_focused),
                                    ("Background", &mut data.window_background),
                                    ("Title", &mut data.window_title),
                                ],
                            ),
                            (
                                "Text",
                                vec![
                                    ("Primary", &mut data.text_primary),
                                    ("Secondary", &mut data.text_secondary),
                                    ("Disabled", &mut data.text_disabled),
                                    ("Selected", &mut data.text_selected),
                                ],
                            ),
                            (
                                "Background",
                                vec![
                                    ("Primary", &mut data.background_primary),
                                    ("Secondary", &mut data.background_secondary),
                                    ("Selected", &mut data.background_selected),
                                    ("Hover", &mut data.background_hover),
                                ],
                            ),
                            (
                                "Browser",
                                vec![
                                    ("Border", &mut data.browser_border),
                                    ("Title", &mut data.browser_title),
                                    ("Item", &mut data.browser_item_normal),
                                    ("Item (selected)", &mut data.browser_item_selected),
                                    ("Item (focused)", &mut data.browser_item_focused),
                                    ("Background", &mut data.browser_background),
                                    ("Scrollbar", &mut data.browser_scrollbar),
                                ],
                            ),
                            (
                                "Form",
                                vec![
                                    ("Border", &mut data.form_border),
                                    ("Label", &mut data.form_label),
                                    ("Label (focused)", &mut data.form_label_focused),
                                    ("Field background", &mut data.form_field_background),
                                    ("Field text", &mut data.form_field_text),
                                    ("Checkbox (checked)", &mut data.form_checkbox_checked),
                                    ("Checkbox (unchecked)", &mut data.form_checkbox_unchecked),
                                    ("Error", &mut data.form_error),
                                ],
                            ),
                            (
                                "Editor",
                                vec![
                                    ("Border", &mut data.editor_border),
                                    ("Label", &mut data.editor_label),
                                    ("Label (focused)", &mut data.editor_label_focused),
                                    ("Text", &mut data.editor_text),
                                    ("Cursor", &mut data.editor_cursor),
                                    ("Status", &mut data.editor_status),
                                    ("Background", &mut data.editor_background),
                                ],
                            ),
                            (
                                "Menu",
                                vec![
                                    ("Border", &mut data.menu_border),
                                    ("Background", &mut data.menu_background),
                                    ("Item", &mut data.menu_item_normal),
                                    ("Item (selected)", &mut data.menu_item_selected),
                                    ("Item (focused)", &mut data.menu_item_focused),
                                    ("Separator", &mut data.menu_separator),
                                ],
                            ),
                            (
                                "Status",
                                vec![
                                    ("Info", &mut data.status_info),
                                    ("Success", &mut data.status_success),
                                    ("Warning", &mut data.status_warning),
                                    ("Error", &mut data.status_error),
                                    ("Background", &mut data.status_background),
                                ],
                            ),
                            (
                                "Buttons",
                                vec![
                                    ("Normal", &mut data.button_normal),
                                    ("Hover", &mut data.button_hover),
                                    ("Active", &mut data.button_active),
                                    ("Disabled", &mut data.button_disabled),
                                ],
                            ),
                            (
                                "Game",
                                vec![
                                    ("Command echo", &mut data.command_echo),
                                    ("Selection background", &mut data.selection_background),
                                    ("Links", &mut data.link_color),
                                    ("Speech", &mut data.speech_color),
                                    ("Whispers", &mut data.whisper_color),
                                    ("Thoughts", &mut data.thought_color),
                                    ("Injury default", &mut data.injury_default_color),
                                ],
                            ),
                        ];

                        for (section, rows) in sections {
                            ui.collapsing(section, |ui| {
                                egui::Grid::new(format!("theme_grid_{}", section))
                                    .num_columns(2)
                                    .show(ui, |ui| {
                                        for (label, value) in rows {
                                            ui.label(label);
                                            color_field(ui, value);
                                            ui.end_row();
                                        }
                                    });
                            });
                        }
                    });

                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save_request = true;
                    }
                    if ui.button("Save && Apply").clicked() {
                        save_request = true;
                        save_and_apply = true;
                    }
                });
            });

        if save_request {
            match self.save_theme_data(&mut state.data) {
                Ok(name) => {
                    if save_and_apply {
                        self.apply_theme_by_name(&name);
                    } else if name == self.app_core.config.active_theme {
                        // Editing the active theme: re-apply on the next frame.
                        self.applied_theme_id = None;
                    }
                    self.app_core
                        .add_system_message(&format!("Theme '{}' saved.", name));
                    self.theme_editor = Some(state);
                    return;
                }
                Err(err) => {
                    state.error = Some(err);
                    self.theme_editor = Some(state);
                    return;
                }
            }
        }

        if open {
            self.theme_editor = Some(state);
        }
    }

    fn save_theme_data(&mut self, data: &mut ThemeData) -> Result<String, String> {
        let name = data.name.trim().to_string();
        if name.is_empty() {
            return Err("Theme name is required.".to_string());
        }
        data.name = name.clone();
        // Keep the legacy editor-color aliases in sync.
        data.border_color = data.editor_border.clone();
        data.label_color = data.editor_label.clone();
        data.focused_label_color = data.editor_label_focused.clone();
        data.text_color = data.editor_text.clone();

        if data.to_app_theme().is_none() {
            return Err("One or more colors are not valid hex values.".to_string());
        }
        data.save_to_file(self.app_core.config.character.as_deref())
            .map_err(|err| format!("Failed to save theme: {}", err))?;
        Ok(name)
    }
}
