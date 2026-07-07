//! Custom Windows authoring panel.
//!
//! A "custom window" is a plain text widget subscribed to one or more Lich XML
//! stream ids. The routing engine already dispatches any stream id to whichever
//! window subscribes to it (see `MessageProcessor::update_text_stream_subscribers`),
//! so this panel is purely an authoring surface: it lets the user create such a
//! window, name it, choose the stream id(s) it listens to — including picking
//! from streams Lich has actually sent this session — and edit or delete it
//! later. This is the discoverable front door that both frontends previously
//! lacked (custom windows were an emergent use of the text widget + streams
//! field).

use super::super::VellumGuiApp;
use crate::data::WindowContent;
use eframe::egui;

/// The template used to spawn a fresh, auto-named custom text window.
const CUSTOM_TEXT_TEMPLATE: &str = "text_custom";

pub(in super::super) struct CustomWindowsEditorState {
    /// Draft for the "new window" row.
    new_title: String,
    new_streams: String,
    /// Draft edits for the currently expanded existing window, keyed by name.
    editing: Option<EditBuffer>,
    error: Option<String>,
}

struct EditBuffer {
    name: String,
    title: String,
    streams: String,
    max_lines: String,
}

impl CustomWindowsEditorState {
    fn new() -> Self {
        Self {
            new_title: String::new(),
            new_streams: String::new(),
            editing: None,
            error: None,
        }
    }
}

/// One existing custom (text-backed) window, snapshotted for display.
struct CustomWindowRow {
    name: String,
    title: String,
    streams: String,
    max_lines: usize,
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

/// Parse a comma-separated streams string into a trimmed, de-duplicated list.
fn parse_streams(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for stream in raw.split(',') {
        let stream = stream.trim();
        if stream.is_empty() {
            continue;
        }
        if !out.iter().any(|s| s.eq_ignore_ascii_case(stream)) {
            out.push(stream.to_string());
        }
    }
    out
}

impl VellumGuiApp {
    pub(in super::super) fn open_custom_windows_editor(&mut self) {
        self.custom_windows_editor = Some(CustomWindowsEditorState::new());
    }

    /// Snapshot the text-backed windows that look like custom windows, i.e.
    /// plain `Text` widgets. Built-in text windows (main, thoughts, …) are still
    /// listed so the user can retarget them, but the created-here flow only ever
    /// produces `Text` widgets.
    fn list_custom_windows(&self) -> Vec<CustomWindowRow> {
        let mut rows: Vec<CustomWindowRow> = self
            .app_core
            .ui_state
            .windows
            .iter()
            .filter_map(|(name, window)| {
                // Only plain Text widgets are user-authorable custom windows.
                if !matches!(window.content, WindowContent::Text(_)) {
                    return None;
                }
                let text = text_content_of(&window.content)?;
                Some(CustomWindowRow {
                    name: name.clone(),
                    title: text.title.clone(),
                    streams: text.streams.join(", "),
                    max_lines: text.max_lines,
                })
            })
            .collect();
        rows.sort_by_key(|row| row.name.to_ascii_lowercase());
        rows
    }

    /// Create a new custom text window, then apply the drafted title + streams.
    fn create_custom_window(&mut self, title: &str, streams: &[String]) -> Result<String, String> {
        // Spawn an auto-named text window via the shared layout path.
        self.app_core
            .layout
            .add_window(CUSTOM_TEXT_TEMPLATE)
            .map_err(|err| format!("Failed to create window: {}", err))?;
        // The auto-named window is the last layout entry.
        let window_def = self
            .app_core
            .layout
            .windows
            .last()
            .cloned()
            .ok_or_else(|| "New window definition could not be retrieved.".to_string())?;
        let name = window_def.name().to_string();
        self.app_core.add_new_window(
            &window_def,
            super::super::INITIAL_LAYOUT_WIDTH,
            super::super::INITIAL_LAYOUT_HEIGHT,
        );
        self.app_core.layout_modified_since_save = true;

        self.apply_streams_and_title(&name, title, streams, None)?;
        Ok(name)
    }

    /// Write streams (+ optional buffer size) and title onto an existing window,
    /// then rebuild the routing cache. Title goes through the shared `.rename`
    /// dot-command so the layout definition and system messaging match the TUI.
    fn apply_streams_and_title(
        &mut self,
        name: &str,
        title: &str,
        streams: &[String],
        max_lines: Option<usize>,
    ) -> Result<(), String> {
        {
            let window = self
                .app_core
                .ui_state
                .windows
                .get_mut(name)
                .ok_or_else(|| format!("Window '{}' no longer exists.", name))?;
            let text = text_content_mut(&mut window.content)
                .ok_or_else(|| format!("Window '{}' is not a text window.", name))?;
            text.streams = streams.to_vec();
            if let Some(max_lines) = max_lines {
                text.max_lines = max_lines;
            }
        }
        // Routing reads a cached subscriber map; rebuild it so the new streams
        // take effect immediately.
        self.app_core
            .message_processor
            .update_text_stream_subscribers(&self.app_core.ui_state);

        let title = title.trim();
        if !title.is_empty() {
            let _ = self
                .app_core
                .send_command(format!(".rename {} {}", name, title));
        }
        self.refresh_available_tabs_if_needed();
        self.app_core.needs_render = true;
        Ok(())
    }

    /// Delete a custom window entirely (not just hide) — remove it from the live
    /// UI and from the layout so it does not reappear, then rebuild routing.
    fn delete_custom_window(&mut self, name: &str) {
        self.app_core.remove_window(name);
        self.app_core.layout.windows.retain(|w| w.name() != name);
        self.app_core.layout_modified_since_save = true;
        self.app_core
            .add_system_message(&format!("Custom window '{}' deleted.", name));
    }

    pub(in super::super) fn render_custom_windows_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.custom_windows_editor.take() else {
            return;
        };

        let rows = self.list_custom_windows();
        let seen_streams = self.app_core.message_processor.seen_streams();

        let mut open = true;
        // Deferred requests, applied after the UI closure to avoid borrowing self
        // mutably during rendering.
        let mut create_request = false;
        let mut save_request: Option<EditBuffer> = None;
        let mut delete_request: Option<String> = None;
        let mut expand_request: Option<Option<EditBuffer>> = None;
        // A stream id clicked in the "seen this session" list to append to
        // whichever draft is active: (append_to_new, stream_id).
        let mut append_stream: Option<(bool, String)> = None;

        egui::Window::new("Custom Windows")
            .id(egui::Id::new("gui_custom_windows"))
            .open(&mut open)
            .default_width(460.0)
            .default_height(460.0)
            .show(ctx, |ui| {
                ui.weak(
                    "Custom windows are text windows fed by Lich stream ids. \
                     Lich can push any content to a stream id; a window listening \
                     to that id displays it.",
                );
                ui.separator();

                // ---- New custom window ----
                ui.strong("New custom window");
                egui::Grid::new("custom_window_new_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Title");
                        ui.text_edit_singleline(&mut state.new_title);
                        ui.end_row();
                        ui.label("Stream ids");
                        ui.text_edit_singleline(&mut state.new_streams);
                        ui.end_row();
                    });
                ui.horizontal(|ui| {
                    let enabled = !state.new_streams.trim().is_empty();
                    if ui
                        .add_enabled(enabled, egui::Button::new("Create"))
                        .on_hover_text("Create a text window subscribed to these stream ids.")
                        .clicked()
                    {
                        create_request = true;
                    }
                    ui.weak("Comma-separated, e.g. bounty, notes");
                });

                ui.separator();

                // ---- Seen this session ----
                ui.strong("Streams seen this session");
                if seen_streams.is_empty() {
                    ui.weak(
                        "No custom streams observed yet. Ids appear here as Lich pushes them.",
                    );
                } else {
                    ui.weak("Click to add to the draft above (or the open editor below).");
                    egui::ScrollArea::vertical()
                        .id_salt("custom_windows_seen_scroll")
                        .max_height(90.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for (id, label) in &seen_streams {
                                    let text = match label {
                                        Some(label) => format!("{} ({})", id, label),
                                        None => id.clone(),
                                    };
                                    if ui.small_button(text).clicked() {
                                        // Route to the open editor if one is active,
                                        // otherwise to the new-window draft.
                                        let to_new = state.editing.is_none();
                                        append_stream = Some((to_new, id.clone()));
                                    }
                                }
                            });
                        });
                }

                ui.separator();

                // ---- Existing text windows ----
                ui.strong("Existing text windows");
                egui::ScrollArea::vertical()
                    .id_salt("custom_windows_existing_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for row in &rows {
                            let is_editing = state
                                .editing
                                .as_ref()
                                .is_some_and(|edit| edit.name == row.name);

                            ui.horizontal(|ui| {
                                if ui.small_button(if is_editing { "▼" } else { "▶" }).clicked() {
                                    expand_request = Some(if is_editing {
                                        None
                                    } else {
                                        Some(EditBuffer {
                                            name: row.name.clone(),
                                            title: row.title.clone(),
                                            streams: row.streams.clone(),
                                            max_lines: row.max_lines.to_string(),
                                        })
                                    });
                                }
                                ui.strong(&row.name);
                                if row.streams.is_empty() {
                                    ui.weak("(no streams)");
                                } else {
                                    ui.weak(&row.streams);
                                }
                            });

                            if is_editing {
                                if let Some(edit) = state.editing.as_mut() {
                                    ui.indent("custom_window_edit_indent", |ui| {
                                        egui::Grid::new(("custom_window_edit_grid", &row.name))
                                            .num_columns(2)
                                            .show(ui, |ui| {
                                                ui.label("Title");
                                                ui.text_edit_singleline(&mut edit.title);
                                                ui.end_row();
                                                ui.label("Stream ids");
                                                ui.text_edit_singleline(&mut edit.streams);
                                                ui.end_row();
                                                ui.label("Buffer lines");
                                                ui.text_edit_singleline(&mut edit.max_lines);
                                                ui.end_row();
                                            });
                                        ui.horizontal(|ui| {
                                            if ui.button("Save").clicked() {
                                                save_request = Some(EditBuffer {
                                                    name: edit.name.clone(),
                                                    title: edit.title.clone(),
                                                    streams: edit.streams.clone(),
                                                    max_lines: edit.max_lines.clone(),
                                                });
                                            }
                                            if ui.button("Delete").clicked() {
                                                delete_request = Some(edit.name.clone());
                                            }
                                        });
                                    });
                                }
                            }
                        }
                        if rows.is_empty() {
                            ui.weak("No text windows in this layout.");
                        }
                    });

                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });

        // ---- Apply deferred requests ----
        if let Some((to_new, id)) = append_stream {
            let target = if to_new {
                Some(&mut state.new_streams)
            } else {
                state.editing.as_mut().map(|edit| &mut edit.streams)
            };
            if let Some(field) = target {
                append_stream_id(field, &id);
            }
        }

        if let Some(expand) = expand_request {
            state.editing = expand;
        }

        if create_request {
            let title = state.new_title.trim().to_string();
            let streams = parse_streams(&state.new_streams);
            if streams.is_empty() {
                state.error = Some("Enter at least one stream id.".to_string());
            } else {
                match self.create_custom_window(&title, &streams) {
                    Ok(name) => {
                        self.app_core
                            .add_system_message(&format!("Custom window '{}' created.", name));
                        state.new_title.clear();
                        state.new_streams.clear();
                        state.error = None;
                    }
                    Err(err) => state.error = Some(err),
                }
            }
        }

        if let Some(edit) = save_request {
            let streams = parse_streams(&edit.streams);
            let max_lines = edit.max_lines.trim().parse::<usize>();
            match max_lines {
                Ok(0) | Err(_) => {
                    state.error = Some("Buffer lines must be a positive number.".to_string());
                }
                Ok(max_lines) => {
                    match self.apply_streams_and_title(
                        &edit.name,
                        &edit.title,
                        &streams,
                        Some(max_lines),
                    ) {
                        Ok(()) => {
                            state.error = None;
                            state.editing = None;
                        }
                        Err(err) => state.error = Some(err),
                    }
                }
            }
        }

        if let Some(name) = delete_request {
            self.delete_custom_window(&name);
            if state
                .editing
                .as_ref()
                .is_some_and(|edit| edit.name == name)
            {
                state.editing = None;
            }
            state.error = None;
        }

        if open {
            self.custom_windows_editor = Some(state);
        }
    }
}

/// Append a stream id to a comma-separated field if not already present.
fn append_stream_id(field: &mut String, id: &str) {
    let already = field
        .split(',')
        .any(|s| s.trim().eq_ignore_ascii_case(id));
    if already {
        return;
    }
    let trimmed = field.trim_end().trim_end_matches(',');
    if trimmed.is_empty() {
        *field = id.to_string();
    } else {
        *field = format!("{}, {}", trimmed, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_streams_trims_dedups_drops_empty() {
        let got = parse_streams(" bounty , notes ,, Bounty ,");
        assert_eq!(got, vec!["bounty".to_string(), "notes".to_string()]);
    }

    #[test]
    fn test_append_stream_id_into_empty() {
        let mut field = String::new();
        append_stream_id(&mut field, "bounty");
        assert_eq!(field, "bounty");
    }

    #[test]
    fn test_append_stream_id_appends_with_separator() {
        let mut field = "bounty".to_string();
        append_stream_id(&mut field, "notes");
        assert_eq!(field, "bounty, notes");
    }

    #[test]
    fn test_append_stream_id_skips_duplicate_case_insensitive() {
        let mut field = "Bounty, notes".to_string();
        append_stream_id(&mut field, "bounty");
        assert_eq!(field, "Bounty, notes");
    }

    #[test]
    fn test_append_stream_id_handles_trailing_comma() {
        let mut field = "bounty, ".to_string();
        append_stream_id(&mut field, "notes");
        assert_eq!(field, "bounty, notes");
    }
}
