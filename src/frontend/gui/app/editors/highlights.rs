//! Highlight browser + form: add/edit/delete highlight patterns through the
//! shared config layer (`Config::save_single_highlight` /
//! `delete_single_highlight`), hot-reloading the highlight engine via
//! `AppCore::reload_highlights` after every change.

use super::super::{theme, VellumGuiApp};
use crate::config::{Config, HighlightPattern, RedirectMode};
use eframe::egui;

pub(in super::super) struct HighlightEditorState {
    filter: String,
    form: Option<HighlightFormState>,
}

impl HighlightEditorState {
    fn new() -> Self {
        Self {
            filter: String::new(),
            form: None,
        }
    }
}

struct HighlightFormState {
    /// Some(name) when editing an existing highlight; None when adding.
    original_name: Option<String>,
    /// Scope the original lives in (delete-from scope on rename).
    original_is_global: bool,
    name: String,
    pattern: String,
    fg: String,
    bg: String,
    bold: bool,
    color_entire_line: bool,
    fast_parse: bool,
    sound: String,
    sound_volume: String,
    category: String,
    squelch: bool,
    silent_prompt: bool,
    redirect_to: String,
    redirect_copy: bool,
    replace: String,
    stream: String,
    window: String,
    is_global: bool,
    error: Option<String>,
}

impl HighlightFormState {
    fn empty() -> Self {
        Self {
            original_name: None,
            original_is_global: true,
            name: String::new(),
            pattern: String::new(),
            fg: String::new(),
            bg: String::new(),
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: String::new(),
            sound_volume: String::new(),
            category: String::new(),
            squelch: false,
            silent_prompt: false,
            redirect_to: String::new(),
            redirect_copy: false,
            replace: String::new(),
            stream: String::new(),
            window: String::new(),
            is_global: true,
            error: None,
        }
    }

    fn from_pattern(name: &str, pattern: &HighlightPattern, is_global: bool) -> Self {
        Self {
            original_name: Some(name.to_string()),
            original_is_global: is_global,
            name: name.to_string(),
            pattern: pattern.pattern.clone(),
            fg: pattern.fg.clone().unwrap_or_default(),
            bg: pattern.bg.clone().unwrap_or_default(),
            bold: pattern.bold,
            color_entire_line: pattern.color_entire_line,
            fast_parse: pattern.fast_parse,
            sound: pattern.sound.clone().unwrap_or_default(),
            sound_volume: pattern
                .sound_volume
                .map(|volume| volume.to_string())
                .unwrap_or_default(),
            category: pattern.category.clone().unwrap_or_default(),
            squelch: pattern.squelch,
            silent_prompt: pattern.silent_prompt,
            redirect_to: pattern.redirect_to.clone().unwrap_or_default(),
            redirect_copy: pattern.redirect_mode == RedirectMode::RedirectCopy,
            replace: pattern.replace.clone().unwrap_or_default(),
            stream: pattern.stream.clone().unwrap_or_default(),
            window: pattern.window.clone().unwrap_or_default(),
            is_global,
            error: None,
        }
    }

    fn build_pattern(&self) -> Result<(String, HighlightPattern), String> {
        fn opt(value: &str) -> Option<String> {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }

        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("Name is required.".to_string());
        }
        let pattern_text = self.pattern.trim().to_string();
        if pattern_text.is_empty() {
            return Err("Pattern is required.".to_string());
        }
        if !self.fast_parse {
            regex::Regex::new(&pattern_text).map_err(|err| format!("Invalid regex: {}", err))?;
        }
        let sound_volume = match self.sound_volume.trim() {
            "" => None,
            text => Some(
                text.parse::<f32>()
                    .map_err(|_| "Sound volume must be a number between 0 and 1.".to_string())
                    .map(|volume| volume.clamp(0.0, 1.0))?,
            ),
        };

        Ok((
            name,
            HighlightPattern {
                pattern: pattern_text,
                fg: opt(&self.fg),
                bg: opt(&self.bg),
                bold: self.bold,
                color_entire_line: self.color_entire_line,
                fast_parse: self.fast_parse,
                sound: opt(&self.sound),
                sound_volume,
                category: opt(&self.category),
                squelch: self.squelch,
                silent_prompt: self.silent_prompt,
                redirect_to: opt(&self.redirect_to),
                redirect_mode: if self.redirect_copy {
                    RedirectMode::RedirectCopy
                } else {
                    RedirectMode::RedirectOnly
                },
                replace: opt(&self.replace),
                stream: opt(&self.stream),
                window: opt(&self.window),
                compiled_regex: None,
            },
        ))
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_highlight_editor(&mut self, edit_name: Option<&str>) {
        let mut state = HighlightEditorState::new();
        match edit_name {
            Some("") | None => {}
            Some(name) => {
                if let Some(pattern) = self.app_core.config.highlights.get(name) {
                    let is_global = !self.highlight_is_character_override(name);
                    state.form = Some(HighlightFormState::from_pattern(name, pattern, is_global));
                } else {
                    self.app_core
                        .add_system_message(&format!("Highlight '{}' not found.", name));
                }
            }
        }
        self.highlight_editor = Some(state);
    }

    pub(in super::super) fn open_highlight_form_new(&mut self) {
        let mut state = self
            .highlight_editor
            .take()
            .unwrap_or_else(HighlightEditorState::new);
        state.form = Some(HighlightFormState::empty());
        self.highlight_editor = Some(state);
    }

    fn highlight_is_character_override(&self, name: &str) -> bool {
        Config::load_character_highlights_only(self.app_core.config.character.as_deref())
            .map(|highlights| highlights.contains_key(name))
            .unwrap_or(false)
    }

    fn save_highlight_from_form(&mut self, form: &HighlightFormState) -> Result<(), String> {
        let (name, pattern) = form.build_pattern()?;
        let character = self.app_core.config.character.clone();

        // Renamed or re-scoped: remove the old entry from its original scope.
        if let Some(original) = &form.original_name {
            if *original != name || form.original_is_global != form.is_global {
                if let Err(err) = Config::delete_single_highlight(
                    original,
                    form.original_is_global,
                    character.as_deref(),
                ) {
                    tracing::warn!("Failed to remove old highlight '{}': {}", original, err);
                }
            }
        }

        Config::save_single_highlight(&name, &pattern, form.is_global, character.as_deref())
            .map_err(|err| format!("Failed to save highlight: {}", err))?;
        self.app_core.reload_highlights();
        Ok(())
    }

    pub(in super::super) fn render_highlight_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.highlight_editor.take() else {
            return;
        };

        let mut open = true;
        let mut open_form: Option<HighlightFormState> = None;
        let mut delete_request: Option<String> = None;

        egui::Window::new("Highlights")
            .id(egui::Id::new("gui_highlight_browser"))
            .open(&mut open)
            .default_width(460.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut state.filter);
                    if ui.button("Add highlight").clicked() {
                        open_form = Some(HighlightFormState::empty());
                    }
                });
                ui.separator();

                let filter = state.filter.to_lowercase();
                let mut names: Vec<&String> = self
                    .app_core
                    .config
                    .highlights
                    .keys()
                    .filter(|name| filter.is_empty() || name.to_lowercase().contains(&filter))
                    .collect();
                names.sort();

                let row_count = names.len();
                egui::ScrollArea::vertical()
                    .id_salt("highlight_browser_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for name in names {
                            let Some(pattern) = self.app_core.config.highlights.get(name) else {
                                continue;
                            };
                            ui.horizontal(|ui| {
                                if ui.small_button("Edit").clicked() {
                                    let is_global =
                                        !self.highlight_is_character_override(name);
                                    open_form = Some(HighlightFormState::from_pattern(
                                        name, pattern, is_global,
                                    ));
                                }
                                if ui.small_button("Delete").clicked() {
                                    delete_request = Some(name.clone());
                                }
                                let mut sample = egui::RichText::new(name);
                                if let Some(fg) =
                                    pattern.fg.as_deref().and_then(theme::resolve_color)
                                {
                                    sample = sample.color(fg);
                                }
                                if let Some(bg) =
                                    pattern.bg.as_deref().and_then(theme::resolve_color)
                                {
                                    sample = sample.background_color(bg);
                                }
                                if pattern.bold {
                                    sample = sample.strong();
                                }
                                ui.label(sample);
                                if let Some(category) = &pattern.category {
                                    ui.weak(format!("[{}]", category));
                                }
                            });
                        }
                        if row_count == 0 {
                            ui.weak("No highlights match.");
                        }
                    });
            });

        if let Some(name) = delete_request {
            let is_global = !self.highlight_is_character_override(&name);
            let character = self.app_core.config.character.clone();
            match Config::delete_single_highlight(&name, is_global, character.as_deref()) {
                Ok(()) => {
                    self.app_core.reload_highlights();
                    self.app_core
                        .add_system_message(&format!("Highlight '{}' deleted.", name));
                }
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to delete highlight: {}", err)),
            }
        }

        if let Some(form) = open_form {
            state.form = Some(form);
        }

        // Render the form on top of the browser when active.
        if let Some(mut form) = state.form.take() {
            let mut form_open = true;
            let mut submitted = false;
            let mut cancelled = false;
            let title = if form.original_name.is_some() {
                "Edit Highlight"
            } else {
                "Add Highlight"
            };
            egui::Window::new(title)
                .id(egui::Id::new("gui_highlight_form"))
                .open(&mut form_open)
                .default_width(420.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("highlight_form_scroll")
                        .show(ui, |ui| {
                            egui::Grid::new("highlight_form_grid")
                                .num_columns(2)
                                .show(ui, |ui| {
                                    ui.label("Name");
                                    ui.text_edit_singleline(&mut form.name);
                                    ui.end_row();
                                    ui.label("Pattern");
                                    ui.text_edit_singleline(&mut form.pattern);
                                    ui.end_row();
                                    ui.label("Foreground");
                                    ui.horizontal(|ui| {
                                        ui.text_edit_singleline(&mut form.fg);
                                        if let Some(color) = theme::resolve_color(&form.fg) {
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(18.0, 14.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().rect_filled(rect, 2.0, color);
                                        }
                                    });
                                    ui.end_row();
                                    ui.label("Background");
                                    ui.horizontal(|ui| {
                                        ui.text_edit_singleline(&mut form.bg);
                                        if let Some(color) = theme::resolve_color(&form.bg) {
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(18.0, 14.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().rect_filled(rect, 2.0, color);
                                        }
                                    });
                                    ui.end_row();
                                    ui.label("Category");
                                    ui.text_edit_singleline(&mut form.category);
                                    ui.end_row();
                                    ui.label("Sound");
                                    ui.text_edit_singleline(&mut form.sound);
                                    ui.end_row();
                                    ui.label("Sound volume");
                                    ui.text_edit_singleline(&mut form.sound_volume);
                                    ui.end_row();
                                    ui.label("Redirect to");
                                    ui.text_edit_singleline(&mut form.redirect_to);
                                    ui.end_row();
                                    ui.label("Replace");
                                    ui.text_edit_singleline(&mut form.replace);
                                    ui.end_row();
                                    ui.label("Stream");
                                    ui.text_edit_singleline(&mut form.stream);
                                    ui.end_row();
                                    ui.label("Window");
                                    ui.text_edit_singleline(&mut form.window);
                                    ui.end_row();
                                });

                            ui.horizontal_wrapped(|ui| {
                                ui.checkbox(&mut form.bold, "Bold");
                                ui.checkbox(&mut form.color_entire_line, "Entire line");
                                ui.checkbox(&mut form.fast_parse, "Fast parse");
                                ui.checkbox(&mut form.squelch, "Squelch");
                                ui.checkbox(&mut form.silent_prompt, "Silent prompt");
                                ui.checkbox(&mut form.redirect_copy, "Redirect copies");
                                ui.checkbox(&mut form.is_global, "Global (all characters)");
                            });

                            if let Some(error) = &form.error {
                                ui.colored_label(ui.visuals().error_fg_color, error);
                            }

                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui.button("Save").clicked() {
                                    submitted = true;
                                }
                                if ui.button("Cancel").clicked() {
                                    cancelled = true;
                                }
                            });
                        });
                });

            if submitted {
                match self.save_highlight_from_form(&form) {
                    Ok(()) => {
                        self.app_core
                            .add_system_message(&format!("Highlight '{}' saved.", form.name.trim()));
                    }
                    Err(err) => {
                        form.error = Some(err);
                        state.form = Some(form);
                    }
                }
            } else if form_open && !cancelled {
                state.form = Some(form);
            }
        }

        if open {
            self.highlight_editor = Some(state);
        }
    }
}
