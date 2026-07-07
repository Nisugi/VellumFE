//! Window editor: rename windows and edit stream routing / scrollback for
//! text windows. Geometry, borders, and colors are dock/theme concerns in the
//! GUI, so only content-level properties are exposed here.

use super::super::VellumGuiApp;
use super::color_field;
use crate::data::WindowContent;
use eframe::egui;

pub(in super::super) struct WindowEditorState {
    /// None = window picker; Some = editing that window.
    selected: Option<String>,
    title: String,
    streams: String,
    max_lines: String,
    supports_streams: bool,
    /// Some when the window is a countdown or progress widget: its feed id
    /// and label are editable (the id decides which timer/bar updates it).
    feed: Option<FeedFields>,
    /// Compact mode toggle (plain text windows only).
    supports_compact: bool,
    compact: bool,
    /// Some for ActiveEffects windows: which effect category feeds it.
    effects_category: Option<String>,
    /// Some for tabbed-text windows: the editable tab list.
    tabs: Option<Vec<TabBuffer>>,
    error: Option<String>,
}

/// One editable tab row. Keeps the original config tab so fields this
/// editor doesn't surface (timestamps, position) survive a save.
struct TabBuffer {
    name: String,
    streams: String,
    ignore_activity: bool,
    original: Option<crate::config::TabbedTextTab>,
}

impl TabBuffer {
    fn from_config(tab: &crate::config::TabbedTextTab) -> Self {
        Self {
            name: tab.name.clone(),
            streams: tab.get_streams().join(", "),
            ignore_activity: tab.ignore_activity.unwrap_or(false),
            original: Some(tab.clone()),
        }
    }

    fn empty() -> Self {
        Self {
            name: String::new(),
            streams: String::new(),
            ignore_activity: false,
            original: None,
        }
    }

    fn to_config(&self) -> crate::config::TabbedTextTab {
        let mut tab = self.original.clone().unwrap_or_default();
        tab.name = self.name.trim().to_string();
        tab.stream = None; // legacy single-stream field superseded below
        tab.streams = self
            .streams
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        tab.ignore_activity = if self.ignore_activity { Some(true) } else { None };
        tab
    }
}

/// Editable feed binding for countdown/progress widgets.
struct FeedFields {
    kind: FeedKind,
    id: String,
    label: String,
    /// Bar/fill color override (hex or palette name); empty = default.
    color: String,
    /// Progress only: show "value/max" instead of the label.
    numbers_only: bool,
    /// Progress only: show just the current value.
    current_only: bool,
}

/// Valid ActiveEffects feed categories (matched exactly by the router).
const EFFECT_CATEGORIES: [&str; 4] = ["ActiveSpells", "Buffs", "Debuffs", "Cooldowns"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum FeedKind {
    Countdown,
    Progress,
}

impl WindowEditorState {
    fn picker() -> Self {
        Self {
            selected: None,
            title: String::new(),
            streams: String::new(),
            max_lines: String::new(),
            supports_streams: false,
            feed: None,
            supports_compact: false,
            compact: false,
            effects_category: None,
            tabs: None,
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
        state.feed = None;
        state.supports_compact = false;
        state.compact = false;
        state.effects_category = None;
        state.tabs = None;
        // Tabbed windows: edit the tab list from the layout definition (the
        // canonical home of per-tab config; live tabs sync from it on save).
        if matches!(window.content, WindowContent::TabbedText(_)) {
            if let Some(crate::config::WindowDef::TabbedText { data, .. }) = self
                .app_core
                .layout
                .windows
                .iter()
                .find(|w| w.name() == name)
            {
                state.tabs = Some(data.tabs.iter().map(TabBuffer::from_config).collect());
            }
        }
        if let Some(text) = text_content_of(&window.content) {
            state.title = text.title.clone();
            state.streams = text.streams.join(", ");
            state.max_lines = text.max_lines.to_string();
            state.supports_streams = true;
            // Compact mode (ingest-time transform) applies to plain text
            // windows only, matching the TUI editor.
            if matches!(window.content, WindowContent::Text(_)) {
                state.supports_compact = true;
                state.compact = text.compact;
            }
        } else {
            // Fall back to the tab title for non-text widgets.
            state.title = Self::tab_key_for_window(name, window)
                .map(|key| key.default_title())
                .unwrap_or_else(|| name.to_string());
            state.streams = String::new();
            state.max_lines = String::new();
            state.supports_streams = false;
            // Countdown/progress widgets: expose the feed binding so custom
            // timers/bars can actually be pointed at an update source.
            state.feed = match &window.content {
                WindowContent::Countdown(countdown) => Some(FeedFields {
                    kind: FeedKind::Countdown,
                    id: countdown.countdown_id.clone(),
                    label: countdown.label.clone(),
                    color: countdown.color.clone().unwrap_or_default(),
                    numbers_only: false,
                    current_only: false,
                }),
                WindowContent::Progress(progress) => Some(FeedFields {
                    kind: FeedKind::Progress,
                    id: progress.progress_id.clone(),
                    label: progress.label.clone(),
                    color: progress.color.clone().unwrap_or_default(),
                    numbers_only: progress.numbers_only,
                    current_only: progress.current_only,
                }),
                _ => None,
            };
            if let WindowContent::ActiveEffects(effects) = &window.content {
                state.effects_category = Some(effects.category.clone());
            }
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
            text.streams = streams.clone();
            text.max_lines = max_lines;
            if state.supports_compact {
                text.compact = state.compact;
            }
            // Stream routing reads a cached subscriber map; rebuild it.
            self.app_core
                .message_processor
                .update_text_stream_subscribers(&self.app_core.ui_state);
            // Persist content settings to the layout definition too (streams
            // previously mutated live state only and were lost on restart).
            match self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            {
                Some(crate::config::WindowDef::Text { data, .. }) => {
                    data.streams = streams;
                    data.buffer_size = max_lines;
                    data.compact = state.compact;
                }
                Some(crate::config::WindowDef::Inventory { data, .. }) => {
                    data.streams = streams;
                    data.buffer_size = max_lines;
                }
                _ => {}
            }
            self.app_core.layout_modified_since_save = true;
        }

        if let Some(feed) = &state.feed {
            let id = feed.id.trim().to_string();
            let label = feed.label.trim().to_string();

            // Live content: countdown/progress updates match on this id
            // directly (no cache), so the change takes effect immediately.
            let Some(window) = self.app_core.ui_state.windows.get_mut(name) else {
                return Err(format!("Window '{}' no longer exists.", name));
            };
            match (&mut window.content, feed.kind) {
                (WindowContent::Countdown(countdown), FeedKind::Countdown) => {
                    countdown.countdown_id = id.clone();
                    countdown.label = label.clone();
                    let color = feed.color.trim();
                    countdown.color = if color.is_empty() {
                        None
                    } else {
                        Some(color.to_string())
                    };
                }
                (WindowContent::Progress(progress), FeedKind::Progress) => {
                    progress.progress_id = id.clone();
                    progress.label = label.clone();
                    let color = feed.color.trim();
                    progress.color = if color.is_empty() {
                        None
                    } else {
                        Some(color.to_string())
                    };
                    progress.numbers_only = feed.numbers_only;
                    progress.current_only = feed.current_only;
                }
                _ => {
                    return Err(format!(
                        "Window '{}' is no longer a countdown/progress widget.",
                        name
                    ));
                }
            }

            // Layout definition: persist the binding so it survives a save.
            fn opt(value: &str) -> Option<String> {
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                }
            }
            if let Some(def) = self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            {
                match (def, feed.kind) {
                    (crate::config::WindowDef::Countdown { data, .. }, FeedKind::Countdown) => {
                        data.id = opt(&id);
                        data.label = opt(&label);
                        data.color = opt(feed.color.trim());
                    }
                    (crate::config::WindowDef::Progress { data, .. }, FeedKind::Progress) => {
                        data.id = opt(&id);
                        data.label = opt(&label);
                        data.color = opt(feed.color.trim());
                        data.numbers_only = feed.numbers_only;
                        data.current_only = feed.current_only;
                    }
                    _ => {}
                }
            }
            self.app_core.layout_modified_since_save = true;
        }

        if let Some(category) = &state.effects_category {
            let Some(window) = self.app_core.ui_state.windows.get_mut(name) else {
                return Err(format!("Window '{}' no longer exists.", name));
            };
            if let WindowContent::ActiveEffects(effects) = &mut window.content {
                if effects.category != *category {
                    effects.category = category.clone();
                    // Old-category effects are stale under the new feed.
                    effects.effects.clear();
                    effects.generation = effects.generation.wrapping_add(1);
                }
            }
            if let Some(crate::config::WindowDef::ActiveEffects { data, .. }) = self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            {
                data.category = category.clone();
            }
            // Effect routing derives implicit streams from window categories.
            self.app_core
                .message_processor
                .update_text_stream_subscribers(&self.app_core.ui_state);
            self.app_core.layout_modified_since_save = true;
        }

        if let Some(tabs) = &state.tabs {
            if tabs.is_empty() {
                return Err("A tabbed window needs at least one tab.".to_string());
            }
            if tabs.iter().any(|tab| tab.name.trim().is_empty()) {
                return Err("Every tab needs a name.".to_string());
            }
            let new_tabs: Vec<crate::config::TabbedTextTab> =
                tabs.iter().map(TabBuffer::to_config).collect();
            let Some(crate::config::WindowDef::TabbedText { data, .. }) = self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            else {
                return Err(format!(
                    "Window '{}' has no tabbed layout definition.",
                    name
                ));
            };
            data.tabs = new_tabs;
            // Rebuild live tabs (and the stream routing index) from the def.
            self.app_core.sync_tabbed_window_tabs(name);
            self.app_core.layout_modified_since_save = true;
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
        let mut delete_request = false;

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
                        if state.supports_compact {
                            ui.label("Compact");
                            ui.checkbox(&mut state.compact, "condense known content");
                            ui.end_row();
                        }
                        if let Some(feed) = state.feed.as_mut() {
                            ui.label(match feed.kind {
                                FeedKind::Countdown => "Countdown id",
                                FeedKind::Progress => "Bar id",
                            });
                            ui.text_edit_singleline(&mut feed.id);
                            ui.end_row();
                            ui.label("Label");
                            ui.text_edit_singleline(&mut feed.label);
                            ui.end_row();
                            ui.label(match feed.kind {
                                FeedKind::Countdown => "Fill color",
                                FeedKind::Progress => "Bar color",
                            });
                            color_field(ui, &mut feed.color);
                            ui.end_row();
                            if feed.kind == FeedKind::Progress {
                                ui.label("Display");
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut feed.numbers_only, "value/max")
                                        .on_hover_text(
                                            "Show the numbers instead of the label.",
                                        );
                                    ui.checkbox(&mut feed.current_only, "value only")
                                        .on_hover_text("Show just the current value.");
                                });
                                ui.end_row();
                            }
                        }
                        if let Some(category) = state.effects_category.as_mut() {
                            ui.label("Category");
                            egui::ComboBox::from_id_salt("window_editor_fx_category")
                                .selected_text(category.clone())
                                .show_ui(ui, |ui| {
                                    for option in EFFECT_CATEGORIES {
                                        ui.selectable_value(
                                            category,
                                            option.to_string(),
                                            option,
                                        );
                                    }
                                });
                            ui.end_row();
                        }
                    });
                if state.supports_streams {
                    ui.weak("Comma-separated stream ids (e.g. main, speech, thoughts).");
                }
                if let Some(feed) = &state.feed {
                    ui.weak(match feed.kind {
                        FeedKind::Countdown => {
                            "Timer feed id this widget tracks: roundtime, casttime, \
                             stuntime, or a custom id pushed by Lich."
                        }
                        FeedKind::Progress => {
                            "Bar feed id this widget tracks: health, mana, stamina, \
                             spirit, encumlevel, mindState, or a custom id pushed by Lich."
                        }
                    });
                }

                if let Some(tabs) = state.tabs.as_mut() {
                    ui.separator();
                    ui.strong("Tabs");
                    let mut remove_index: Option<usize> = None;
                    let mut move_op: Option<(usize, bool)> = None; // (index, up)
                    let tab_count = tabs.len();
                    egui::Grid::new("window_editor_tabs_grid")
                        .num_columns(4)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Name");
                            ui.strong("Streams");
                            ui.strong("Quiet");
                            ui.label("");
                            ui.end_row();
                            for (index, tab) in tabs.iter_mut().enumerate() {
                                ui.add(
                                    egui::TextEdit::singleline(&mut tab.name)
                                        .desired_width(90.0),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut tab.streams)
                                        .desired_width(160.0),
                                );
                                ui.checkbox(&mut tab.ignore_activity, "")
                                    .on_hover_text("Don't mark this tab unread on activity.");
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(index > 0, egui::Button::new("↑").small())
                                        .clicked()
                                    {
                                        move_op = Some((index, true));
                                    }
                                    if ui
                                        .add_enabled(
                                            index + 1 < tab_count,
                                            egui::Button::new("↓").small(),
                                        )
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(
                                egui::RichText::new("Delete Window")
                                    .color(ui.visuals().error_fg_color),
                            )
                            .on_hover_text(
                                "Remove this window from the layout entirely (not just hide).",
                            )
                            .clicked()
                        {
                            delete_request = true;
                        }
                    });
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

        if delete_request {
            if let Some(name) = state.selected.clone() {
                let locked = self
                    .app_core
                    .layout
                    .windows
                    .iter()
                    .find(|w| w.name() == name)
                    .map(|w| w.base().locked)
                    .unwrap_or(false);
                if locked {
                    state.error = Some(format!(
                        "Window '{}' is locked; unlock it before deleting.",
                        name
                    ));
                } else {
                    self.delete_custom_window(&name);
                    state = WindowEditorState::picker();
                }
            }
        }

        if open {
            self.window_editor = Some(state);
        }
    }
}
