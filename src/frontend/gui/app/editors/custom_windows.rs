//! Streams panel (formerly "Custom Windows").
//!
//! One table over every known game text stream — routed streams, streams any
//! window definition subscribes to, and streams Lich has actually sent this
//! session — showing the EFFECTIVE destination of each (mirroring the
//! `route_for` precedence: subscribed window > `[streams.routes]` entry >
//! fallback window) and letting the user change it in place: move the stream
//! between text windows, route it to Main/Discard, reset it to the fallback,
//! or spin up a new custom window subscribed to it.
//!
//! The authoring surface the panel started as is still here: a "custom
//! window" is a plain text widget subscribed to one or more Lich XML stream
//! ids, and this panel creates, edits, and deletes them (the routing engine
//! dispatches any stream id to whichever window subscribes to it — see
//! `MessageProcessor::update_text_stream_subscribers`).

use super::super::VellumGuiApp;
use crate::config::StreamRoute;
use crate::core::messages::{route_for, RouteDecision};
use crate::data::WindowContent;
use eframe::egui;
use std::collections::BTreeMap;

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
        | WindowContent::Reserve(text)
        | WindowContent::Spells(text) => Some(text),
        _ => None,
    }
}

fn text_content_mut(content: &mut WindowContent) -> Option<&mut crate::data::TextContent> {
    match content {
        WindowContent::Text(text)
        | WindowContent::Inventory(text)
        | WindowContent::Reserve(text)
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

/// The effective destination of a stream, shaped for the "Goes to" column.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GoesTo {
    /// One or more windows subscribe; the first is shown, extras counted.
    Subscribed { first: String, extra: usize },
    /// Orphaned with a `main` route.
    RouteMain,
    /// Orphaned with a `discard` route.
    RouteDiscard,
    /// Orphaned with a `window:<name>` route (shown whether or not the
    /// window currently exists — delivery falls back gracefully either way).
    RouteWindow(String),
    /// No subscriber and no route: the fallback window receives it.
    Fallback,
}

/// Compute the effective destination of `stream_id`, mirroring the routing
/// precedence exactly by asking `route_for`. `RouteDecision::Deliver`
/// flattens "explicit route" vs "fallback" into candidate windows, which the
/// panel must keep apart for display — that disambiguation consults the
/// route entry the same way `route_for` does (case-insensitive).
fn goes_to(
    subscribers: &[String],
    stream_id: &str,
    routes: &BTreeMap<String, StreamRoute>,
    fallback: &str,
) -> GoesTo {
    match route_for(stream_id, !subscribers.is_empty(), routes, fallback) {
        RouteDecision::Subscribed => GoesTo::Subscribed {
            first: subscribers[0].clone(),
            extra: subscribers.len() - 1,
        },
        RouteDecision::Discard => GoesTo::RouteDiscard,
        RouteDecision::Deliver { .. } => {
            let route = routes
                .iter()
                .find(|(id, _)| id.eq_ignore_ascii_case(stream_id))
                .map(|(_, route)| route);
            match route {
                Some(StreamRoute::Main) => GoesTo::RouteMain,
                Some(StreamRoute::Window(name)) => GoesTo::RouteWindow(name.clone()),
                // A discard route never reaches Deliver without a subscriber,
                // and Subscribed was handled above; treat both as fallback.
                Some(StreamRoute::Discard) | None => GoesTo::Fallback,
            }
        }
    }
}

/// Human label for a destination ("thoughts", "thoughts +2", "Main",
/// "Discard", "fallback (main)").
fn goes_to_label(dest: &GoesTo, fallback: &str) -> String {
    match dest {
        GoesTo::Subscribed { first, extra: 0 } => first.clone(),
        GoesTo::Subscribed { first, extra } => format!("{} +{}", first, extra),
        GoesTo::RouteMain => "Main".to_string(),
        GoesTo::RouteDiscard => "Discard".to_string(),
        GoesTo::RouteWindow(name) => name.clone(),
        GoesTo::Fallback => format!("fallback ({})", fallback),
    }
}

/// Union stream ids from several sources: trim, drop empties, de-duplicate
/// case-insensitively (first spelling wins), sort alphabetically.
fn merge_stream_ids<'a>(sources: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in sources {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        if !out.iter().any(|s| s.eq_ignore_ascii_case(id)) {
            out.push(id.to_string());
        }
    }
    out.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    out
}

/// What the user picked in a row's "Goes to" combo.
enum RouteTarget {
    /// Subscribe the named text window to the stream (moving it from any
    /// other text window) — the same action the Window Editor performs.
    Window(String),
    /// Orphan the stream and route it to main.
    Main,
    /// Orphan the stream and discard it.
    Discard,
    /// Orphan the stream and clear its route (fallback window applies).
    Fallback,
    /// Pre-fill the "new custom window" draft with the stream.
    NewWindow,
}

/// One row of the streams table, snapshotted for display.
struct StreamRow {
    id: String,
    /// Friendly label from the seen-streams registry, if any (hover text).
    label: Option<String>,
    dest: GoesTo,
    /// The route entry even when a subscription overrides it (orphan policy).
    route: Option<StreamRoute>,
    /// False when a non-Text window subscribes (tabbed tab, implicit widget
    /// stream, …): this panel only edits plain text windows' stream lists,
    /// so such bindings are changed in the Window Editor instead.
    editable: bool,
}

impl VellumGuiApp {
    pub(in super::super) fn open_custom_windows_editor(&mut self) {
        if self.custom_windows_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_custom_windows"));
            return;
        }
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
        self.app_core.schedule_layout_autosave();

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
        // Persist to the layout definition too, or the edit is lost on
        // restart (same fix the Window Editor carries).
        self.app_core
            .sync_text_streams_to_layout(name, streams.to_vec(), max_lines);
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

    // The stream-routing mutations (subscribe, orphan, route, layout
    // sync) live on AppCore (core/app_core/streams.rs), shared with the
    // TUI's `.streams` menu; only the panel plumbing here is
    // GUI-specific.

    /// Apply a "Goes to" combo choice for `stream`. `NewWindow` is handled in
    /// the render loop (it only pre-fills the draft).
    fn apply_route_target(&mut self, stream: &str, target: RouteTarget) -> Result<(), String> {
        match target {
            RouteTarget::Window(name) => {
                // Move the subscription; any route entry stays as the orphan
                // policy for when this window goes away (subscriptions win).
                self.app_core
                    .remove_stream_from_text_windows(stream, Some(&name));
                self.app_core.add_stream_to_text_window(&name, stream)?;
            }
            RouteTarget::Main => {
                self.app_core.remove_stream_from_text_windows(stream, None);
                self.app_core
                    .set_stream_route(stream, Some(StreamRoute::Main))?;
            }
            RouteTarget::Discard => {
                self.app_core.remove_stream_from_text_windows(stream, None);
                self.app_core
                    .set_stream_route(stream, Some(StreamRoute::Discard))?;
            }
            RouteTarget::Fallback => {
                self.app_core.remove_stream_from_text_windows(stream, None);
                self.app_core.set_stream_route(stream, None)?;
            }
            RouteTarget::NewWindow => {}
        }
        Ok(())
    }

    /// Build the streams table: the union of routed streams, streams any
    /// window definition subscribes to, and streams seen this session.
    fn build_stream_rows(&self) -> Vec<StreamRow> {
        let routes = &self.app_core.config.streams.routes;
        let fallback = &self.app_core.config.streams.fallback;
        let seen = self.app_core.message_processor.seen_streams();

        // Streams from layout window definitions (text-widget kinds).
        let mut layout_streams: Vec<String> = Vec::new();
        for def in &self.app_core.layout.windows {
            match def {
                crate::config::WindowDef::Text { data, .. } => {
                    layout_streams.extend(data.streams.iter().cloned());
                }
                crate::config::WindowDef::Inventory { data, .. }
                | crate::config::WindowDef::Reserve { data, .. } => {
                    layout_streams.extend(data.streams.iter().cloned());
                }
                crate::config::WindowDef::TabbedText { data, .. } => {
                    for tab in &data.tabs {
                        layout_streams.extend(tab.get_streams());
                    }
                }
                _ => {}
            }
        }

        let ids = merge_stream_ids(
            routes
                .keys()
                .map(String::as_str)
                .chain(layout_streams.iter().map(String::as_str))
                .chain(seen.iter().map(|(id, _)| id.as_str())),
        );

        ids.into_iter()
            .map(|id| {
                let subscribers = self
                    .app_core
                    .message_processor
                    .get_stream_subscribers(&id)
                    .to_vec();
                let editable = subscribers.iter().all(|name| {
                    matches!(
                        self.app_core.ui_state.windows.get(name).map(|w| &w.content),
                        Some(WindowContent::Text(_))
                    )
                });
                let dest = goes_to(&subscribers, &id, routes, fallback);
                let route = routes
                    .iter()
                    .find(|(rid, _)| rid.eq_ignore_ascii_case(&id))
                    .map(|(_, route)| route.clone());
                let label = seen
                    .iter()
                    .find(|(sid, _)| sid.eq_ignore_ascii_case(&id))
                    .and_then(|(_, label)| label.clone());
                StreamRow {
                    id,
                    label,
                    dest,
                    route,
                    editable,
                }
            })
            .collect()
    }

    /// Delete a window entirely (not just hide) — remove it from the live
    /// UI and from the layout so it does not reappear, then rebuild routing.
    /// Shared by the Custom Windows panel and the Window Editor.
    pub(in super::super) fn delete_custom_window(&mut self, name: &str) {
        // Forget the GUI-side per-tab state BEFORE removing the window from
        // the core: find_tab_key_by_name resolves through available_tabs,
        // which is derived from the core window list. Deleting a grouped
        // window otherwise left the group intact, so surviving members
        // stayed grouped followers — checked and un-re-addable in Windows.
        if let Some(key) = self.find_tab_key_by_name(name) {
            self.forget_tab_state(&key, name);
        }
        self.app_core.remove_window(name);
        self.app_core.layout.windows.retain(|w| w.name() != name);
        self.app_core.schedule_layout_autosave();
        self.app_core
            .add_system_message(&format!("Custom window '{}' deleted.", name));
    }

    pub(in super::super) fn render_custom_windows_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.custom_windows_editor.take() else {
            return;
        };

        let rows = self.list_custom_windows();
        let seen_streams = self.app_core.message_processor.seen_streams();
        let stream_rows = self.build_stream_rows();
        let fallback = self.app_core.config.streams.fallback.clone();
        // Window choices for the "Goes to" combos: the plain text windows
        // this panel can (re)subscribe.
        let window_names: Vec<String> = rows.iter().map(|row| row.name.clone()).collect();

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
        // A "Goes to" combo choice: (stream_id, target).
        let mut route_request: Option<(String, RouteTarget)> = None;

        egui::Window::new("Streams")
            .id(egui::Id::new("gui_custom_windows"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(460.0)
            .default_height(460.0)
            .show(ctx, |ui| {
                ui.weak(
                    "Every game text stream and where it goes. Streams nobody \
                     claims go to the fallback window.",
                );
                ui.horizontal(|ui| {
                    ui.label("Fallback window:");
                    ui.strong(&fallback);
                    ui.weak("(change in Settings > Streams)");
                });
                ui.separator();

                // ---- All known streams ----
                ui.strong("All known streams");
                if stream_rows.is_empty() {
                    ui.weak(
                        "No streams known yet. Ids appear here from routes, \
                         window subscriptions, and live traffic.",
                    );
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("streams_table_scroll")
                        .max_height(200.0)
                        .show(ui, |ui| {
                            egui::Grid::new("streams_table_grid")
                                .num_columns(3)
                                .striped(true)
                                .show(ui, |ui| {
                                    for row in &stream_rows {
                                        render_stream_row(
                                            ui,
                                            row,
                                            &window_names,
                                            &fallback,
                                            &mut route_request,
                                        );
                                        ui.end_row();
                                    }
                                });
                        });
                }

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
        if let Some((id, target)) = route_request {
            match target {
                RouteTarget::NewWindow => {
                    // Pre-fill the creation draft below; the user names the
                    // window and clicks Create.
                    append_stream_id(&mut state.new_streams, &id);
                    state.error = None;
                }
                target => match self.apply_route_target(&id, target) {
                    Ok(()) => state.error = None,
                    Err(err) => state.error = Some(err),
                },
            }
        }

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

/// One table row: stream id | "Goes to" combo | annotation. Emits the chosen
/// retarget into `route_request` (applied after the UI closure).
fn render_stream_row(
    ui: &mut egui::Ui,
    row: &StreamRow,
    window_names: &[String],
    fallback: &str,
    route_request: &mut Option<(String, RouteTarget)>,
) {
    // Column 1: stream id (hover shows the friendly label Lich sent, if any).
    let id_response = ui.label(&row.id);
    if let Some(label) = &row.label {
        id_response.on_hover_text(label);
    }

    // Column 2: effective destination combo.
    let selected = match &row.dest {
        // The fallback destination is deliberately weak: nothing claims
        // this stream, it merely lands in the default window.
        GoesTo::Fallback => egui::RichText::new(goes_to_label(&row.dest, fallback)).weak(),
        dest => egui::RichText::new(goes_to_label(dest, fallback)),
    };
    let combo = ui.add_enabled_ui(row.editable, |ui| {
        egui::ComboBox::from_id_salt(("stream_goes_to", &row.id))
            .width(170.0)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for name in window_names {
                    let is_current = matches!(
                        &row.dest,
                        GoesTo::Subscribed { first, .. } if first == name
                    );
                    if ui.selectable_label(is_current, name).clicked() && !is_current {
                        *route_request =
                            Some((row.id.clone(), RouteTarget::Window(name.clone())));
                    }
                }
                if !window_names.is_empty() {
                    ui.separator();
                }
                let is_main = row.dest == GoesTo::RouteMain;
                if ui.selectable_label(is_main, "Main").clicked() && !is_main {
                    *route_request = Some((row.id.clone(), RouteTarget::Main));
                }
                let is_discard = row.dest == GoesTo::RouteDiscard;
                if ui.selectable_label(is_discard, "Discard").clicked() && !is_discard {
                    *route_request = Some((row.id.clone(), RouteTarget::Discard));
                }
                let is_fallback = row.dest == GoesTo::Fallback;
                if ui
                    .selectable_label(is_fallback, format!("(fallback: {})", fallback))
                    .clicked()
                    && !is_fallback
                {
                    *route_request = Some((row.id.clone(), RouteTarget::Fallback));
                }
                ui.separator();
                if ui.selectable_label(false, "New window…").clicked() {
                    *route_request = Some((row.id.clone(), RouteTarget::NewWindow));
                }
            });
    });
    if !row.editable {
        combo.response.on_hover_text(
            "A tabbed or built-in widget subscribes to this stream; \
             change that binding in the Window Editor.",
        );
    }

    // Column 3: the orphan policy when a subscription currently overrides it.
    match (&row.dest, &row.route) {
        (GoesTo::Subscribed { .. }, Some(route)) => {
            ui.weak(format!("route: {}", route)).on_hover_text(
                "Where this stream goes if the subscribing window is removed.",
            );
        }
        _ if !row.editable => {
            ui.weak("via Window Editor");
        }
        _ => {
            ui.label("");
        }
    }
}

/// Append a stream id to a comma-separated field if not already present.
/// Shared with the Window Editor's seen-streams picker.
pub(super) fn append_stream_id(field: &mut String, id: &str) {
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

    // ---- Streams table helpers ----------------------------------------

    fn routes(entries: &[(&str, StreamRoute)]) -> BTreeMap<String, StreamRoute> {
        entries
            .iter()
            .map(|(id, route)| (id.to_string(), route.clone()))
            .collect()
    }

    fn subs(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn goes_to_subscribed_window_beats_any_route() {
        let map = routes(&[("bounty", StreamRoute::Discard)]);
        let dest = goes_to(&subs(&["bounty_win"]), "bounty", &map, "main");
        assert_eq!(
            dest,
            GoesTo::Subscribed {
                first: "bounty_win".to_string(),
                extra: 0
            }
        );
        assert_eq!(goes_to_label(&dest, "main"), "bounty_win");
    }

    #[test]
    fn goes_to_counts_extra_subscribers() {
        let dest = goes_to(&subs(&["a", "b", "c"]), "notes", &routes(&[]), "main");
        assert_eq!(goes_to_label(&dest, "main"), "a +2");
    }

    #[test]
    fn goes_to_shows_routes_case_insensitively() {
        let map = routes(&[
            ("speech", StreamRoute::Discard),
            ("ooc", StreamRoute::Main),
            ("bounty", StreamRoute::Window("bounty_win".to_string())),
        ]);
        assert_eq!(goes_to(&[], "SPEECH", &map, "main"), GoesTo::RouteDiscard);
        assert_eq!(goes_to(&[], "ooc", &map, "main"), GoesTo::RouteMain);
        let dest = goes_to(&[], "Bounty", &map, "main");
        assert_eq!(dest, GoesTo::RouteWindow("bounty_win".to_string()));
        assert_eq!(goes_to_label(&dest, "main"), "bounty_win");
        assert_eq!(goes_to_label(&GoesTo::RouteDiscard, "main"), "Discard");
        assert_eq!(goes_to_label(&GoesTo::RouteMain, "main"), "Main");
    }

    #[test]
    fn goes_to_unrouted_stream_is_fallback() {
        let dest = goes_to(&[], "bounty", &routes(&[]), "story");
        assert_eq!(dest, GoesTo::Fallback);
        assert_eq!(goes_to_label(&dest, "story"), "fallback (story)");
    }

    #[test]
    fn merge_stream_ids_unions_dedups_and_sorts() {
        let merged = merge_stream_ids(
            ["bounty", "Speech", "", " notes ", "speech", "atmospherics"]
                .into_iter(),
        );
        assert_eq!(merged, vec!["atmospherics", "bounty", "notes", "Speech"]);
    }
}
