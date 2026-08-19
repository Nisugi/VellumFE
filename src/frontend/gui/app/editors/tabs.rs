//! Tab Editor: the tabbedtext window's tab list (names, streams, order,
//! per-tab quiet/timestamps). A dedicated popout in the dashboard/hand-icons
//! editor family — a grid-shaped editing surface launched from the window's
//! right-click menu ("Edit tabs…"). Buffered: changes apply on Save.

use super::custom_windows::append_stream_id;
use crate::data::WindowContent;
use eframe::egui;

use super::super::VellumGuiApp;

pub(in super::super) struct TabEditorState {
    /// Internal name of the tabbedtext window being edited.
    window: String,
    tabs: Vec<TabBuffer>,
    error: Option<String>,
}

/// One editable tab row. Keeps the original config tab so fields this
/// editor doesn't surface (position, colors) survive a save.
struct TabBuffer {
    name: String,
    streams: String,
    ignore_activity: bool,
    show_timestamps: bool,
    original: Option<crate::config::TabbedTextTab>,
}

impl TabBuffer {
    fn from_config(tab: &crate::config::TabbedTextTab) -> Self {
        Self {
            name: tab.name.clone(),
            streams: tab.get_streams().join(", "),
            ignore_activity: tab.ignore_activity.unwrap_or(false),
            show_timestamps: tab.show_timestamps.unwrap_or(false),
            original: Some(tab.clone()),
        }
    }

    fn empty() -> Self {
        Self {
            name: String::new(),
            streams: String::new(),
            ignore_activity: false,
            show_timestamps: false,
            original: None,
        }
    }

    fn to_config(&self) -> crate::config::TabbedTextTab {
        let mut tab = self.original.clone().unwrap_or_default();
        tab.name = self.name.trim().to_string();
        tab.stream = None; // legacy single-stream field superseded below
        tab.streams = super::custom_windows::parse_streams(&self.streams);
        tab.ignore_activity = if self.ignore_activity {
            Some(true)
        } else {
            None
        };
        tab.show_timestamps = if self.show_timestamps {
            Some(true)
        } else {
            None
        };
        tab
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_tab_editor(&mut self, window_name: String) {
        // Re-opening for the same window raises the existing editor instead
        // of rebuilding (and wiping) its unsaved state.
        if let Some(state) = &self.tab_editor {
            if state.window == window_name {
                self.raise_editor(egui::Id::new("gui_tab_editor"));
                return;
            }
        }
        let Some(crate::config::WindowDef::TabbedText { data, .. }) = self
            .app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == window_name)
        else {
            self.app_core.add_system_message(&format!(
                "Window '{}' has no tabbed layout definition.",
                window_name
            ));
            return;
        };
        let tabs = data.tabs.iter().map(TabBuffer::from_config).collect();
        self.tab_editor = Some(TabEditorState {
            window: window_name,
            tabs,
            error: None,
        });
    }

    fn apply_tab_editor(&mut self, state: &TabEditorState) -> Result<(), String> {
        if state.tabs.is_empty() {
            return Err("A tabbed window needs at least one tab.".to_string());
        }
        if state.tabs.iter().any(|tab| tab.name.trim().is_empty()) {
            return Err("Every tab needs a name.".to_string());
        }
        let new_tabs: Vec<crate::config::TabbedTextTab> =
            state.tabs.iter().map(TabBuffer::to_config).collect();
        let Some(crate::config::WindowDef::TabbedText { data, .. }) = self
            .app_core
            .layout
            .windows
            .iter_mut()
            .find(|w| w.name() == state.window)
        else {
            return Err(format!(
                "Window '{}' has no tabbed layout definition.",
                state.window
            ));
        };
        data.tabs = new_tabs;
        // Rebuild live tabs (and the stream routing index) from the def.
        self.app_core.sync_tabbed_window_tabs(&state.window);
        self.app_core.schedule_layout_autosave();
        self.app_core.needs_render = true;
        Ok(())
    }

    pub(in super::super) fn render_tab_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.tab_editor.take() else {
            return;
        };
        // The window may have been deleted or renamed since opening.
        if !self
            .app_core
            .ui_state
            .windows
            .get(&state.window)
            .map(|window| matches!(window.content, WindowContent::TabbedText(_)))
            .unwrap_or(false)
        {
            return;
        }

        let mut open = true;
        let mut save_request = false;
        // Streams seen this session, for the per-tab "+" picker.
        let seen_streams = self.app_core.message_processor.seen_streams();
        // The full stream universe for the checklist: the persisted
        // discovery registry (well-known + everything this character's
        // sessions ever declared) plus streams seen this session, with the
        // friendly title when one is known. Sorted by label.
        let known_streams: Vec<(String, String)> = {
            let mut keys: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut list: Vec<(String, String)> = Vec::new();
            for binding in &self.app_core.window_registry.bindings {
                if binding.kind == "stream" && keys.insert(binding.id.to_ascii_lowercase()) {
                    list.push((binding.id.clone(), binding.title.clone()));
                }
            }
            for (id, label) in &seen_streams {
                if keys.insert(id.to_ascii_lowercase()) {
                    list.push((id.clone(), label.clone().unwrap_or_else(|| id.clone())));
                }
            }
            list.sort_by(|a, b| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase()));
            list
        };

        let title = self
            .app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == state.window)
            .and_then(|w| w.base().title.clone())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| state.window.clone());
        egui::Window::new("Tab Editor")
            .id(egui::Id::new("gui_tab_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            // Wide enough for the grid (Name + Streams + Quiet/TS + reorder
            // + Remove) so the Name field isn't squeezed to a few chars.
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(format!("Tabs of '{}'", title)).strong());
                ui.separator();
                let tabs = &mut state.tabs;
                let mut remove_index: Option<usize> = None;
                let mut move_op: Option<(usize, bool)> = None; // (index, up)
                let tab_count = tabs.len();
                egui::Grid::new("tab_editor_grid")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Name");
                        ui.strong("Streams");
                        ui.strong("Quiet");
                        ui.strong("TS");
                        ui.label("");
                        ui.end_row();
                        for (index, tab) in tabs.iter_mut().enumerate() {
                            ui.add(
                                egui::TextEdit::singleline(&mut tab.name)
                                    .desired_width(120.0)
                                    .clip_text(false),
                            );
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut tab.streams)
                                        .desired_width(210.0)
                                        .hint_text("stream ids, comma-separated"),
                                );
                                // Pick a seen stream to add to THIS tab.
                                if !seen_streams.is_empty() {
                                    ui.menu_button("+", |ui| {
                                        ui.set_min_width(180.0);
                                        for (id, label) in &seen_streams {
                                            let text = match label {
                                                Some(label) => format!("{} ({})", id, label),
                                                None => id.clone(),
                                            };
                                            if ui.button(text).clicked() {
                                                append_stream_id(&mut tab.streams, id);
                                                ui.close();
                                            }
                                        }
                                    })
                                    .response
                                    .on_hover_text("Add a seen stream to this tab");
                                }
                            });
                            ui.checkbox(&mut tab.ignore_activity, "")
                                .on_hover_text("Don't mark this tab unread on activity.");
                            ui.checkbox(&mut tab.show_timestamps, "")
                                .on_hover_text("Show per-line timestamps on this tab.");
                            ui.horizontal(|ui| {
                                // Geometric triangles render in the bundled
                                // fonts; the ↑/↓ arrow block is tofu.
                                if ui
                                    .add_enabled(index > 0, egui::Button::new("▲").small())
                                    .on_hover_text("Move up")
                                    .clicked()
                                {
                                    move_op = Some((index, true));
                                }
                                if ui
                                    .add_enabled(
                                        index + 1 < tab_count,
                                        egui::Button::new("▼").small(),
                                    )
                                    .on_hover_text("Move down")
                                    .clicked()
                                {
                                    move_op = Some((index, false));
                                }
                                if ui.small_button("Remove").clicked() {
                                    remove_index = Some(index);
                                }
                            });
                            ui.end_row();
                        }
                    });
                // Known-stream activation list: every stream the client
                // knows, one checkbox each. Ticking gives the stream its own
                // tab named after it; unticking removes it from EVERY tab,
                // and a tab left with no streams goes away. The grid above
                // stays for renaming, multi-stream tabs, and ordering.
                if !known_streams.is_empty() {
                    let mut toggled: Option<(String, String, bool)> = None;
                    let active: std::collections::HashSet<String> = tabs
                        .iter()
                        .flat_map(|tab| tab.streams.split(','))
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect();
                    ui.add_space(4.0);
                    ui.strong("Known streams");
                    ui.weak(
                        "Tick to give a stream its own tab; untick to remove it \
                         from every tab.",
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("tab_editor_known_streams")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for (id, label) in &known_streams {
                                    let mut on = active.contains(&id.to_ascii_lowercase());
                                    let text = if label.eq_ignore_ascii_case(id) {
                                        label.clone()
                                    } else {
                                        format!("{label} ({id})")
                                    };
                                    if ui.checkbox(&mut on, text).changed() {
                                        toggled = Some((id.clone(), label.clone(), on));
                                    }
                                }
                            });
                        });
                    if let Some((id, label, on)) = toggled {
                        if on {
                            tabs.push(TabBuffer {
                                name: label,
                                streams: id,
                                ignore_activity: false,
                                show_timestamps: false,
                                original: None,
                            });
                        } else {
                            let lower = id.to_ascii_lowercase();
                            // Drop a tab only when THIS untick emptied it.
                            // Rows that were already stream-less (a freshly
                            // added tab mid-setup, a placeholder) must
                            // survive an unrelated stream toggle.
                            tabs.retain_mut(|tab| {
                                let had_stream = tab
                                    .streams
                                    .split(',')
                                    .any(|s| s.trim().to_ascii_lowercase() == lower);
                                if !had_stream {
                                    return true;
                                }
                                tab.streams = tab
                                    .streams
                                    .split(',')
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty() && s.to_ascii_lowercase() != lower)
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                !tab.streams.trim().is_empty()
                            });
                            if tabs.is_empty() {
                                tabs.push(TabBuffer::empty());
                            }
                        }
                    }
                }
                if ui.button("Add tab").clicked() {
                    tabs.push(TabBuffer::empty());
                }
                ui.weak("Per-tab comma-separated stream ids. Changes apply on Save.");
                if let Some((index, up)) = move_op {
                    let target = if up { index - 1 } else { index + 1 };
                    tabs.swap(index, target);
                }
                if let Some(index) = remove_index {
                    if tabs.len() > 1 {
                        tabs.remove(index);
                    }
                }
                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.separator();
                if ui.button("Save").clicked() {
                    save_request = true;
                }
            });

        if save_request {
            match self.apply_tab_editor(&state) {
                Ok(()) => state.error = None,
                Err(err) => state.error = Some(err),
            }
        }

        if open {
            self.tab_editor = Some(state);
        }
    }
}
