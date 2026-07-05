//! Window editor: rename windows and edit stream routing / scrollback for
//! text windows. Geometry, borders, and colors are dock/theme concerns in the
//! GUI, so only content-level properties are exposed here.

use super::super::VellumGuiApp;
use crate::data::WindowContent;
use eframe::egui;

pub(in super::super) struct WindowEditorState {
    /// None = window picker; Some = editing that window.
    selected: Option<String>,
    title: String,
    streams: String,
    max_lines: String,
    supports_streams: bool,
    error: Option<String>,
}

impl WindowEditorState {
    fn picker() -> Self {
        Self {
            selected: None,
            title: String::new(),
            streams: String::new(),
            max_lines: String::new(),
            supports_streams: false,
            error: None,
        }
    }
}

fn text_content_of(content: &WindowContent) -> Option<&crate::data::TextContent> {
    match content {
        WindowContent::Text(text)
        | WindowContent::Inventory(text)
        | WindowContent::Spells(text) => Some(text),
        _ => None,
    }
}

fn text_content_mut(content: &mut WindowContent) -> Option<&mut crate::data::TextContent> {
    match content {
        WindowContent::Text(text)
        | WindowContent::Inventory(text)
        | WindowContent::Spells(text) => Some(text),
        _ => None,
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_window_editor(&mut self, window_name: Option<&str>) {
        let mut state = WindowEditorState::picker();
        if let Some(name) = window_name {
            if self.load_window_into_editor(&mut state, name) {
                // loaded
            } else {
                self.app_core
                    .add_system_message(&format!("Window '{}' not found.", name));
            }
        }
        self.window_editor = Some(state);
    }

    fn load_window_into_editor(&self, state: &mut WindowEditorState, name: &str) -> bool {
        let Some(window) = self.app_core.ui_state.windows.get(name) else {
            return false;
        };
        state.selected = Some(name.to_string());
        state.error = None;
        if let Some(text) = text_content_of(&window.content) {
            state.title = text.title.clone();
            state.streams = text.streams.join(", ");
            state.max_lines = text.max_lines.to_string();
            state.supports_streams = true;
        } else {
            // Fall back to the tab title for non-text widgets.
            state.title = Self::tab_key_for_window(name, window)
                .map(|key| key.default_title())
                .unwrap_or_else(|| name.to_string());
            state.streams = String::new();
            state.max_lines = String::new();
            state.supports_streams = false;
        }
        true
    }

    fn apply_window_editor(&mut self, state: &WindowEditorState) -> Result<(), String> {
        let Some(name) = &state.selected else {
            return Err("No window selected.".to_string());
        };
        let title = state.title.trim().to_string();
        if title.is_empty() {
            return Err("Title is required.".to_string());
        }

        if state.supports_streams {
            let streams: Vec<String> = state
                .streams
                .split(',')
                .map(|stream| stream.trim().to_string())
                .filter(|stream| !stream.is_empty())
                .collect();
            let max_lines: usize = state
                .max_lines
                .trim()
                .parse()
                .map_err(|_| "Buffer lines must be a number.".to_string())?;
            if max_lines == 0 {
                return Err("Buffer lines must be at least 1.".to_string());
            }

            let Some(window) = self.app_core.ui_state.windows.get_mut(name) else {
                return Err(format!("Window '{}' no longer exists.", name));
            };
            let Some(text) = text_content_mut(&mut window.content) else {
                return Err(format!("Window '{}' is not a text window.", name));
            };
            text.streams = streams;
            text.max_lines = max_lines;
            // Stream routing reads a cached subscriber map; rebuild it.
            self.app_core
                .message_processor
                .update_text_stream_subscribers(&self.app_core.ui_state);
        }

        // Rename through the shared dot-command so the layout definition and
        // system messaging behave exactly like the TUI.
        let _ = self
            .app_core
            .send_command(format!(".rename {} {}", name, title));
        self.refresh_available_tabs_if_needed();
        self.app_core.needs_render = true;
        Ok(())
    }

    pub(in super::super) fn render_window_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.window_editor.take() else {
            return;
        };

        let mut open = true;
        let mut load_request: Option<String> = None;
        let mut save_request = false;

        egui::Window::new("Window Editor")
            .id(egui::Id::new("gui_window_editor"))
            .open(&mut open)
            .default_width(380.0)
            .show(ctx, |ui| {
                if state.selected.is_none() {
                    ui.weak("Pick a window to edit.");
                    ui.separator();
                    let mut names: Vec<(String, String)> = self
                        .app_core
                        .ui_state
                        .windows
                        .iter()
                        .map(|(name, window)| {
                            (name.clone(), format!("{:?}", window.widget_type))
                        })
                        .collect();
                    names.sort();
                    egui::ScrollArea::vertical()
                        .id_salt("window_editor_picker")
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for (name, widget_type) in names {
                                ui.horizontal(|ui| {
                                    if ui.small_button("Edit").clicked() {
                                        load_request = Some(name.clone());
                                    }
                                    ui.label(&name);
                                    ui.weak(widget_type);
                                });
                            }
                        });
                    return;
                }

                let window_name = state.selected.clone().unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!("Editing '{}'", window_name)).strong(),
                );
                ui.separator();
                egui::Grid::new("window_editor_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Title");
                        ui.text_edit_singleline(&mut state.title);
                        ui.end_row();
                        if state.supports_streams {
                            ui.label("Streams");
                            ui.text_edit_singleline(&mut state.streams);
                            ui.end_row();
                            ui.label("Buffer lines");
                            ui.text_edit_singleline(&mut state.max_lines);
                            ui.end_row();
                        }
                    });
                if state.supports_streams {
                    ui.weak("Comma-separated stream ids (e.g. main, speech, thoughts).");
                }

                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save_request = true;
                    }
                    if ui.button("Back").clicked() {
                        load_request = Some(String::new());
                    }
                });
            });

        match load_request.as_deref() {
            Some("") => {
                state = WindowEditorState::picker();
            }
            Some(name) => {
                let name = name.to_string();
                if !self.load_window_into_editor(&mut state, &name) {
                    state.error = Some(format!("Window '{}' not found.", name));
                }
            }
            None => {}
        }

        if save_request {
            match self.apply_window_editor(&state) {
                Ok(()) => state.error = None,
                Err(err) => state.error = Some(err),
            }
        }

        if open {
            self.window_editor = Some(state);
        }
    }
}
