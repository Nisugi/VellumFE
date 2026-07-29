//! Window editor: rename windows and edit stream routing / scrollback for
//! text windows. Geometry, borders, and colors are dock/theme concerns in the
//! GUI, so only content-level properties are exposed here.
//!
//! Widget-shaped settings that used to live in the Settings window are edited
//! here instead, next to the window they configure: the vitals bar options
//! (GuiUiSettings, per-character GUI layout file) and the global
//! `target_list.*` config settings (settings registry).

use super::super::VellumGuiApp;
use super::color_field;
use super::custom_windows::append_stream_id;
use crate::config::registry::{self, SettingKind, SettingValue};
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
    /// Timestamp rendering (plain text windows only).
    show_timestamps: bool,
    /// Timestamp at the start of the line (false = end).
    ts_at_start: bool,
    /// Some for ActiveEffects windows: which effect category feeds it.
    effects_category: Option<String>,
    /// Some for Room windows: section visibility toggles. The room-name
    /// heading is always shown in the GUI (the def's show_name flag drives
    /// the TUI border title instead).
    room: Option<RoomFields>,
    /// Some for Targets windows: per-window display options.
    targets: Option<TargetFields>,
    /// Some for Targets windows: drafts of the GLOBAL `target_list.*`
    /// settings (config.toml via the settings registry), surfaced here so
    /// every target knob lives in one place. Changes apply immediately and
    /// persist per key at character scope.
    targets_global: Option<TargetGlobalFields>,
    /// Some for MiniVitals windows: the shared vitals options from the
    /// per-character GUI layout file (GuiUiSettings.vitals). Buffered here
    /// and written back on Save with the same dirty-flag mechanism the
    /// Settings editor used before this section moved here.
    vitals: Option<crate::frontend::gui::persistence::VitalsConfig>,
    /// Some for gs4_experience windows: per-field display toggles.
    experience: Option<ExperienceFields>,
    /// Some for encum windows: bar/blurb display toggles.
    encum: Option<EncumFields>,
    /// Lock flag from the layout definition; None when the window has no
    /// layout def. Locked windows can't be deleted.
    locked: Option<bool>,
    /// TTS opt-in from the layout def; None when the window has no def or
    /// doesn't carry text lines.
    tts_speak: Option<bool>,
    /// Some for tabbed-text windows: the editable tab list.
    tabs: Option<Vec<TabBuffer>>,
    /// True for injury-doll windows: surfaces the skin calibrator launcher.
    is_injury_doll: bool,
    error: Option<String>,
}

/// Room window section toggles (shared with the TUI editor via RoomWidgetData).
struct RoomFields {
    show_desc: bool,
    show_objs: bool,
    show_players: bool,
    show_exits: bool,
}

/// gs4_experience field toggles + bar colors (shared with the TUI via
/// GS4ExperienceWidgetData). Colors are edited as free text (empty = None,
/// falling back to the theme/default), matching the color_field idiom.
struct ExperienceFields {
    show_level: bool,
    show_mind_bar: bool,
    show_exp_bar: bool,
    show_total_exp: bool,
    show_ascension_exp: bool,
    mind_bar_color: String,
    exp_bar_color: String,
}

/// Encumbrance display toggles (shared via EncumbranceWidgetData).
struct EncumFields {
    show_bar: bool,
    show_label: bool,
}

/// Targets window display options (shared via TargetsWidgetData).
struct TargetFields {
    /// Show the count of filtered body parts (arms, tentacles, …).
    show_appendages: bool,
    /// Per-window status position override: "" = follow global config.
    status_position: String,
}

/// Drafts for the global `target_list.*` registry settings. Each field
/// mirrors one registry key; lists buffer as one entry per line.
struct TargetGlobalFields {
    status_position: String,
    truncation_mode: String,
    excluded_nouns: String,
    boss_color: String,
    challenging_color: String,
}

/// Current value of a registry setting as draft text (lists join one entry
/// per line, matching the Settings editor's list buffers).
fn registry_draft(config: &crate::config::Config, key: &str) -> String {
    match registry::find(key).map(|def| (def.get)(config)) {
        Some(SettingValue::Text(v)) => v,
        Some(SettingValue::List(v)) => v.join("\n"),
        _ => String::new(),
    }
}

/// One editable tab row. Keeps the original config tab so fields this
/// editor doesn't surface (timestamps, position) survive a save.
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
        tab.streams = self
            .streams
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        tab.ignore_activity = if self.ignore_activity { Some(true) } else { None };
        tab.show_timestamps = if self.show_timestamps { Some(true) } else { None };
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
            show_timestamps: false,
            ts_at_start: false,
            effects_category: None,
            room: None,
            targets: None,
            targets_global: None,
            vitals: None,
            experience: None,
            encum: None,
            locked: None,
            tts_speak: None,
            tabs: None,
            is_injury_doll: false,
            error: None,
        }
    }
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

impl VellumGuiApp {
    pub(in super::super) fn open_window_editor(&mut self, window_name: Option<&str>) {
        // Already open: if a specific window is named, load it into the live
        // editor; otherwise just raise the existing picker. Rebuilding would
        // discard any in-progress edits.
        if let Some(mut state) = self.window_editor.take() {
            if let Some(name) = window_name {
                if !self.load_window_into_editor(&mut state, name) {
                    self.app_core
                        .add_system_message(&format!("Window '{}' not found.", name));
                }
            }
            self.window_editor = Some(state);
            self.raise_editor(egui::Id::new("gui_window_editor"));
            return;
        }
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
        state.show_timestamps = false;
        state.ts_at_start = false;
        state.effects_category = None;
        state.room = None;
        state.targets = None;
        state.targets_global = None;
        state.vitals = None;
        state.experience = None;
        state.encum = None;
        state.locked = None;
        state.tts_speak = None;
        state.tabs = None;
        state.is_injury_doll = matches!(window.content, WindowContent::InjuryDoll(_));
        // Def-backed options (lock flag, room/targets display settings).
        if let Some(def) = self
            .app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == name)
        {
            state.locked = Some(def.base().locked);
            // Only text-carrying windows receive lines the TTS queue can
            // speak, so only they get the checkbox.
            if matches!(
                def,
                crate::config::WindowDef::Text { .. }
                    | crate::config::WindowDef::TabbedText { .. }
                    | crate::config::WindowDef::Inventory { .. }
                    | crate::config::WindowDef::Reserve { .. }
                    | crate::config::WindowDef::Spells { .. }
            ) {
                state.tts_speak = Some(def.base().tts_speak);
            }
            match def {
                crate::config::WindowDef::Room { data, .. } => {
                    state.room = Some(RoomFields {
                        show_desc: data.show_desc,
                        show_objs: data.show_objs,
                        show_players: data.show_players,
                        show_exits: data.show_exits,
                    });
                }
                crate::config::WindowDef::Targets { data, .. } => {
                    state.targets = Some(TargetFields {
                        show_appendages: data.show_body_part_count,
                        status_position: data.status_position.clone().unwrap_or_default(),
                    });
                }
                crate::config::WindowDef::GS4Experience { data, .. } => {
                    state.experience = Some(ExperienceFields {
                        show_level: data.show_level,
                        show_mind_bar: data.show_mind_bar,
                        show_exp_bar: data.show_exp_bar,
                        show_total_exp: data.show_total_exp,
                        show_ascension_exp: data.show_ascension_exp,
                        mind_bar_color: data.mind_bar_color.clone().unwrap_or_default(),
                        exp_bar_color: data.exp_bar_color.clone().unwrap_or_default(),
                    });
                }
                crate::config::WindowDef::Encumbrance { data, .. } => {
                    state.encum = Some(EncumFields {
                        show_bar: data.show_bar,
                        show_label: data.show_label,
                    });
                }
                _ => {}
            }
        }
        // Targets windows also edit the global target_list.* settings here
        // (moved from the Settings window's Targets category).
        if matches!(window.content, WindowContent::Targets) {
            let config = &self.app_core.config;
            state.targets_global = Some(TargetGlobalFields {
                status_position: registry_draft(config, "target_list.status_position"),
                truncation_mode: registry_draft(config, "target_list.truncation_mode"),
                excluded_nouns: registry_draft(config, "target_list.excluded_nouns"),
                boss_color: registry_draft(config, "target_list.boss_color"),
                challenging_color: registry_draft(config, "target_list.challenging_color"),
            });
        }
        // The vitals window's bar options live in GuiUiSettings (moved from
        // the Settings window's GUI section).
        if matches!(window.content, WindowContent::MiniVitals) {
            state.vitals = Some(self.ui_settings.vitals.clone());
        }
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
            // Compact mode (ingest-time transform) and timestamps apply to
            // plain text windows only, matching the TUI editor.
            if matches!(window.content, WindowContent::Text(_)) {
                state.supports_compact = true;
                state.compact = text.compact;
                state.show_timestamps = text.show_timestamps;
                state.ts_at_start = matches!(
                    text.timestamp_position,
                    crate::config::TimestampPosition::Start
                );
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
                text.show_timestamps = state.show_timestamps;
                text.timestamp_position = if state.ts_at_start {
                    crate::config::TimestampPosition::Start
                } else {
                    crate::config::TimestampPosition::End
                };
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
                    data.show_timestamps = state.show_timestamps;
                    data.timestamp_position = Some(if state.ts_at_start {
                        crate::config::TimestampPosition::Start
                    } else {
                        crate::config::TimestampPosition::End
                    });
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

        if let Some(room) = &state.room {
            if let Some(crate::config::WindowDef::Room { data, .. }) = self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            {
                data.show_desc = room.show_desc;
                data.show_objs = room.show_objs;
                data.show_players = room.show_players;
                data.show_exits = room.show_exits;
                self.app_core.layout_modified_since_save = true;
            }
        }

        if let Some(targets) = &state.targets {
            if let Some(crate::config::WindowDef::Targets { data, .. }) = self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            {
                data.show_body_part_count = targets.show_appendages;
                data.status_position = Some(targets.status_position.clone())
                    .filter(|position| !position.is_empty());
                self.app_core.layout_modified_since_save = true;
            }
        }

        if let Some(experience) = &state.experience {
            if let Some(crate::config::WindowDef::GS4Experience { data, .. }) = self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            {
                data.show_level = experience.show_level;
                data.show_mind_bar = experience.show_mind_bar;
                data.show_exp_bar = experience.show_exp_bar;
                data.show_total_exp = experience.show_total_exp;
                data.show_ascension_exp = experience.show_ascension_exp;
                let opt = |s: &str| {
                    let t = s.trim();
                    if t.is_empty() { None } else { Some(t.to_string()) }
                };
                data.mind_bar_color = opt(&experience.mind_bar_color);
                data.exp_bar_color = opt(&experience.exp_bar_color);
                self.app_core.layout_modified_since_save = true;
            }
        }

        if let Some(encum) = &state.encum {
            if let Some(crate::config::WindowDef::Encumbrance { data, .. }) = self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            {
                data.show_bar = encum.show_bar;
                data.show_label = encum.show_label;
                self.app_core.layout_modified_since_save = true;
            }
        }

        if let Some(tts_speak) = state.tts_speak {
            if let Some(def) = self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            {
                if def.base().tts_speak != tts_speak {
                    def.base_mut().tts_speak = tts_speak;
                    self.app_core.refresh_tts_windows();
                    self.app_core.layout_modified_since_save = true;
                }
            }
        }

        if let Some(locked) = state.locked {
            if let Some(def) = self
                .app_core
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == name)
            {
                if def.base().locked != locked {
                    def.base_mut().locked = locked;
                    self.app_core.layout_modified_since_save = true;
                }
            }
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

        if let Some(vitals) = &state.vitals {
            // Vitals options live in GuiUiSettings, saved with the
            // per-character GUI layout file — same storage and debounced
            // dirty-flag save the Settings editor used for this section.
            self.ui_settings.vitals = vitals.clone();
            self.layout_dirty = true;
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
        // A stream id clicked in the "seen this session" list, to append to
        // the Streams field after the UI closure.
        let mut append_stream: Option<String> = None;
        // Global target_list.* keys edited this frame; applied and persisted
        // after the UI closure (the closure only borrows self immutably).
        let mut changed_global: Vec<&'static str> = Vec::new();
        // Snapshot outside the closure: the closure borrows self immutably.
        // Available for single-stream text windows AND tabbed windows (each
        // tab picks streams from it).
        let seen_streams = if state.supports_streams || state.tabs.is_some() {
            self.app_core.message_processor.seen_streams()
        } else {
            Vec::new()
        };
        // Injury-doll windows launch the skin calibrator from here too; it
        // works against the *loaded* skin, so it needs saved doll base art
        // (same gate as the Settings > Appearance button).
        let mut calibrate_doll_clicked = false;
        let can_calibrate_doll = self
            .skin_state
            .widget_art()
            .is_some_and(|art| art.doll_base.is_some());
        // Appearance overrides (title bar, text size, font, accent, wrap)
        // live on the window's tab, not in the buffered editor state; the
        // Appearance section applies them immediately, like the same
        // controls in the window's right-click menu.
        let appearance_tab = state.selected.as_ref().and_then(|name| {
            self.app_core
                .ui_state
                .windows
                .get(name)
                .and_then(|window| Self::tab_key_for_window(name, window))
        });
        let appearance_view = appearance_tab
            .as_ref()
            .map(|key| self.appearance_view_for_tab(key));
        let mut appearance_command = None;

        egui::Window::new("Window Editor")
            .id(egui::Id::new("gui_window_editor"))
            .open(&mut open)
            .default_width(380.0)
            .show(ctx, |ui| {
                if state.selected.is_none() {
                    ui.weak("Pick a window to edit.");
                    ui.separator();
                    let mut names: Vec<(String, String, bool)> = self
                        .app_core
                        .ui_state
                        .windows
                        .iter()
                        .map(|(name, window)| {
                            let hidden = Self::tab_key_for_window(name, window)
                                .is_some_and(|key| self.hidden_tabs.contains(&key));
                            (name.clone(), format!("{:?}", window.widget_type), hidden)
                        })
                        .collect();
                    names.sort();
                    egui::ScrollArea::vertical()
                        .id_salt("window_editor_picker")
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for (name, widget_type, hidden) in names {
                                ui.horizontal(|ui| {
                                    if ui.small_button("Edit").clicked() {
                                        load_request = Some(name.clone());
                                    }
                                    ui.label(&name);
                                    ui.weak(widget_type);
                                    if hidden {
                                        ui.weak("(hidden)");
                                    }
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
                        if let Some(locked) = state.locked.as_mut() {
                            ui.label("Locked");
                            ui.checkbox(locked, "protect from delete")
                                .on_hover_text(
                                    "A locked window can't be deleted until unlocked.",
                                );
                            ui.end_row();
                        }
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
                            ui.label("Timestamps");
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut state.show_timestamps, "show");
                                if state.show_timestamps {
                                    ui.checkbox(&mut state.ts_at_start, "at line start")
                                        .on_hover_text(
                                            "Unchecked: timestamp at the end of the line.",
                                        );
                                }
                            });
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
                        if let Some(room) = state.room.as_mut() {
                            ui.label("Sections");
                            ui.vertical(|ui| {
                                ui.checkbox(&mut room.show_desc, "description");
                                ui.checkbox(&mut room.show_objs, "objects");
                                ui.checkbox(&mut room.show_players, "players");
                                ui.checkbox(&mut room.show_exits, "exits");
                            });
                            ui.end_row();
                        }
                        if let Some(tts_speak) = state.tts_speak.as_mut() {
                            ui.label("Speech");
                            ui.checkbox(tts_speak, "speak new lines (TTS)")
                                .on_hover_text(
                                    "Read lines routed to this window aloud. TTS must \
                                     be enabled in Settings > Speech.",
                                );
                            ui.end_row();
                        }
                        if let Some(experience) = state.experience.as_mut() {
                            ui.label("Fields");
                            ui.vertical(|ui| {
                                ui.checkbox(&mut experience.show_level, "level");
                                ui.checkbox(&mut experience.show_mind_bar, "mind state");
                                ui.checkbox(&mut experience.show_exp_bar, "experience bar");
                                ui.checkbox(&mut experience.show_total_exp, "total exp")
                                    .on_hover_text(
                                        "Total absorbed experience (needs the Lich \
                                         experience feed).",
                                    );
                                ui.checkbox(&mut experience.show_ascension_exp, "ascension exp")
                                    .on_hover_text(
                                        "Total ascension experience (needs the Lich \
                                         experience feed).",
                                    );
                            });
                            ui.end_row();
                            ui.label("Mind bar color");
                            super::color_field(ui, &mut experience.mind_bar_color);
                            ui.end_row();
                            ui.label("Exp bar color");
                            super::color_field(ui, &mut experience.exp_bar_color);
                            ui.end_row();
                        }
                        if let Some(encum) = state.encum.as_mut() {
                            ui.label("Fields");
                            ui.vertical(|ui| {
                                ui.checkbox(&mut encum.show_bar, "level bar");
                                ui.checkbox(&mut encum.show_label, "help text");
                            });
                            ui.end_row();
                        }
                        if let Some(targets) = state.targets.as_mut() {
                            ui.label("Appendages");
                            ui.checkbox(&mut targets.show_appendages, "show filtered count")
                                .on_hover_text(
                                    "Body parts (arms, tentacles, …) are filtered from \
                                     the target list; show how many were hidden.",
                                );
                            ui.end_row();
                            ui.label("Status position");
                            let position_label = |value: &str| match value {
                                "start" => "Before the name",
                                "end" => "After the name",
                                _ => "Global default",
                            };
                            egui::ComboBox::from_id_salt("window_editor_status_position")
                                .selected_text(position_label(&targets.status_position))
                                .show_ui(ui, |ui| {
                                    for option in ["", "start", "end"] {
                                        ui.selectable_value(
                                            &mut targets.status_position,
                                            option.to_string(),
                                            position_label(option),
                                        );
                                    }
                                });
                            ui.end_row();
                        }
                    });

                // Vitals bar options (moved from Settings > GUI). Buffered
                // like every other field here; written back on Save.
                if let Some(vitals) = state.vitals.as_mut() {
                    use crate::frontend::gui::persistence::{
                        VitalKind, VitalsOrientation, VitalsTextFormat,
                    };
                    ui.separator();
                    ui.strong("Vitals");
                    ui.weak("Saved per character with the GUI layout.");
                    egui::Grid::new("window_editor_vitals_grid")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Layout");
                            egui::ComboBox::from_id_salt("window_editor_vitals_orientation")
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
                            egui::ComboBox::from_id_salt("window_editor_vitals_text")
                                .selected_text(match vitals.text_format {
                                    VitalsTextFormat::LabelValueMax => "Health: 191/193",
                                    VitalsTextFormat::LabelPercent => "Health: 99%",
                                    VitalsTextFormat::ValueMax => "191/193",
                                    VitalsTextFormat::Percent => "99%",
                                    VitalsTextFormat::None => "No text",
                                })
                                .show_ui(ui, |ui| {
                                    for (format, label) in [
                                        (VitalsTextFormat::LabelValueMax, "Health: 191/193"),
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
                        });
                    ui.label("Bars shown:");
                    let bars = &mut vitals.bars;
                    for kind in VitalKind::all() {
                        let mut enabled = bars.contains(&kind);
                        if ui.checkbox(&mut enabled, kind.label()).changed() {
                            if enabled {
                                bars.push(kind);
                                // Keep display order canonical regardless of
                                // toggle order.
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

                // Global targets settings (moved from Settings > Targets).
                // These edit config.target_list through the settings
                // registry and apply/persist on change, not on Save.
                if let Some(globals) = state.targets_global.as_mut() {
                    ui.separator();
                    ui.strong("Targets (global)");
                    ui.weak(
                        "These apply to target parsing/display everywhere, \
                         not just this window.",
                    );
                    let input_height = ui.spacing().interact_size.y;
                    egui::Grid::new("window_editor_targets_global_grid")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (key, draft) in [
                                (
                                    "target_list.status_position",
                                    &mut globals.status_position,
                                ),
                                (
                                    "target_list.truncation_mode",
                                    &mut globals.truncation_mode,
                                ),
                            ] {
                                let Some(def) = registry::find(key) else {
                                    continue;
                                };
                                ui.label(def.label).on_hover_text(def.description);
                                if let SettingKind::Enum { options } = &def.kind {
                                    let mut changed = false;
                                    egui::ComboBox::from_id_salt((
                                        "window_editor_targets_global",
                                        key,
                                    ))
                                    .selected_text(draft.clone())
                                    .show_ui(ui, |ui| {
                                        for option in *options {
                                            changed |= ui
                                                .selectable_value(
                                                    draft,
                                                    option.to_string(),
                                                    *option,
                                                )
                                                .changed();
                                        }
                                    });
                                    if changed {
                                        changed_global.push(key);
                                    }
                                }
                                ui.end_row();
                            }
                            {
                                let key = "target_list.excluded_nouns";
                                if let Some(def) = registry::find(key) {
                                    ui.label(def.label).on_hover_text(def.description);
                                    if ui
                                        .add_sized(
                                            [260.0, input_height * 3.6],
                                            egui::TextEdit::multiline(
                                                &mut globals.excluded_nouns,
                                            )
                                            .desired_rows(3),
                                        )
                                        .on_hover_text("one entry per line")
                                        .changed()
                                    {
                                        changed_global.push(key);
                                    }
                                    ui.end_row();
                                }
                            }
                            for (key, draft) in [
                                ("target_list.boss_color", &mut globals.boss_color),
                                (
                                    "target_list.challenging_color",
                                    &mut globals.challenging_color,
                                ),
                            ] {
                                let Some(def) = registry::find(key) else {
                                    continue;
                                };
                                ui.label(def.label).on_hover_text(def.description);
                                if ui
                                    .add_sized(
                                        [260.0, input_height],
                                        egui::TextEdit::singleline(draft),
                                    )
                                    .on_hover_text("empty = unset")
                                    .changed()
                                {
                                    changed_global.push(key);
                                }
                                ui.end_row();
                            }
                        });
                }

                if state.supports_streams {
                    ui.weak("Comma-separated stream ids (e.g. main, speech, thoughts).");
                    if !seen_streams.is_empty() {
                        ui.weak("Seen this session (click to add):");
                        egui::ScrollArea::vertical()
                            .id_salt("window_editor_seen_scroll")
                            .max_height(70.0)
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    for (id, label) in &seen_streams {
                                        let text = match label {
                                            Some(label) => format!("{} ({})", id, label),
                                            None => id.clone(),
                                        };
                                        if ui.small_button(text).clicked() {
                                            append_stream = Some(id.clone());
                                        }
                                    }
                                });
                            });
                    }
                }
                if let Some(feed) = &state.feed {
                    ui.weak(match feed.kind {
                        FeedKind::Countdown => {
                            "Timer feed id this widget tracks: roundtime, casttime, \
                             stuntime, a custom [event_patterns] event_type, or an id \
                             a script pushes via <vellumTimer id='...' value='epoch'/>."
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
                                        .desired_width(130.0),
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

                if let Some(view) = &appearance_view {
                    ui.separator();
                    ui.collapsing("Appearance", |ui| {
                        ui.weak("Applies immediately; also in the window's right-click menu.");
                        if let Some(command) = Self::render_appearance_controls(ui, view) {
                            appearance_command = Some(command);
                        }
                    });
                }

                if state.is_injury_doll {
                    ui.separator();
                    if ui
                        .add_enabled(
                            can_calibrate_doll,
                            egui::Button::new("Calibrate injury doll…"),
                        )
                        .on_hover_text(
                            "Click each body part on the skin's doll image to \
                             place its wound dot",
                        )
                        .on_disabled_hover_text(
                            "Needs an active (saved) skin with base art under \
                             [injury_doll] in its skin.toml",
                        )
                        .clicked()
                    {
                        calibrate_doll_clicked = true;
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

        if let Some(id) = append_stream {
            append_stream_id(&mut state.streams, &id);
        }

        if calibrate_doll_clicked {
            self.open_doll_calibration();
        }

        if let (Some(key), Some(command)) = (appearance_tab.as_ref(), appearance_command) {
            self.apply_appearance_command(key, command);
        }

        // Apply the global targets edits: write through the registry setter
        // and persist each changed key sparsely at character scope (the GUI
        // default elsewhere).
        if !changed_global.is_empty() {
            if let Some(globals) = &state.targets_global {
                changed_global.sort_unstable();
                changed_global.dedup();
                let character = self.app_core.config.character.clone();
                for key in changed_global {
                    let Some(def) = registry::find(key) else {
                        continue;
                    };
                    let value = match key {
                        "target_list.status_position" => {
                            SettingValue::Text(globals.status_position.clone())
                        }
                        "target_list.truncation_mode" => {
                            SettingValue::Text(globals.truncation_mode.clone())
                        }
                        "target_list.excluded_nouns" => SettingValue::List(
                            globals
                                .excluded_nouns
                                .lines()
                                .map(str::trim)
                                .filter(|line| !line.is_empty())
                                .map(str::to_string)
                                .collect(),
                        ),
                        "target_list.boss_color" => {
                            SettingValue::Text(globals.boss_color.trim().to_string())
                        }
                        "target_list.challenging_color" => {
                            SettingValue::Text(globals.challenging_color.trim().to_string())
                        }
                        _ => continue,
                    };
                    if let Err(err) = (def.set)(&mut self.app_core.config, &value) {
                        state.error = Some(format!("{key}: {err}"));
                        continue;
                    }
                    if let Err(err) = self.app_core.config.save_single_setting(
                        key,
                        false,
                        character.as_deref(),
                    ) {
                        state.error = Some(format!("{key}: {err}"));
                    }
                }
                self.app_core.needs_render = true;
            }
        }

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
