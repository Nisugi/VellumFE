//! Window/widget configuration sections of the window context menu.
//!
//! These were the Window Editor's fields (title, streams, feed bindings,
//! per-widget display toggles); they now live directly in the right-click
//! menu and apply live instead of on Save. The view is resolved up front
//! (`window_config_view_for_tab`), rendered by static fns alongside the rest
//! of the context menu, and applied through `apply_window_config_command`.

use super::menus::GuiWindowMenuCommand;
use super::*;
use crate::data::WindowContent;

/// Valid ActiveEffects feed categories (matched exactly by the router).
const EFFECT_CATEGORIES: [&str; 4] = ["ActiveSpells", "Buffs", "Debuffs", "Cooldowns"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FeedKind {
    Countdown,
    Progress,
}

/// Feed binding for countdown/progress widgets. Doubles as the command
/// payload: any field committing sends the whole struct.
#[derive(Clone, Debug)]
pub(super) struct FeedConfig {
    pub(super) kind: FeedKind,
    pub(super) id: String,
    pub(super) label: String,
    /// Bar/fill color override (hex or palette name); empty = default.
    pub(super) color: String,
    /// Progress only: show "value/max" instead of the label.
    pub(super) numbers_only: bool,
    /// Progress only: show just the current value.
    pub(super) current_only: bool,
    /// Countdown only: stay visible at rest showing "label: 0".
    pub(super) show_when_zero: bool,
}

/// Room window section toggles (shared with the TUI via RoomWidgetData).
#[derive(Clone, Debug)]
pub(super) struct RoomSections {
    pub(super) show_desc: bool,
    pub(super) show_objs: bool,
    pub(super) show_players: bool,
    pub(super) show_exits: bool,
}

/// Targets window per-window display options (TargetsWidgetData).
#[derive(Clone, Debug)]
pub(super) struct TargetsConfig {
    pub(super) show_appendages: bool,
    /// "" = follow the global config.
    pub(super) status_position: String,
}

/// gs4_experience field toggles + bar colors (GS4ExperienceWidgetData).
#[derive(Clone, Debug)]
pub(super) struct ExperienceConfig {
    pub(super) show_level: bool,
    pub(super) show_mind_bar: bool,
    pub(super) show_exp_bar: bool,
    pub(super) show_total_exp: bool,
    pub(super) show_ascension_exp: bool,
    pub(super) mind_bar_color: String,
    pub(super) exp_bar_color: String,
}

/// Encumbrance display toggles (EncumbranceWidgetData).
#[derive(Clone, Debug)]
pub(super) struct EncumConfig {
    pub(super) show_bar: bool,
    pub(super) show_label: bool,
}

/// Multi-account card contents (MultiAccountWidgetData).
#[derive(Clone, Debug)]
pub(super) struct MultiAccountConfig {
    /// The widget data verbatim -- the UI edits this copy directly. The old
    /// shape mirrored twelve fields with hand-written copy-in and copy-out
    /// blocks, where a forgotten copy-out meant a checkbox that flipped in
    /// the menu and was silently discarded on the next frame's rebuild.
    pub(super) data: crate::config::MultiAccountWidgetData,
    /// Rows in display order: (row, shown, merge-flag). The merge flag is
    /// the RAW stored membership (`row_merge_flag`), not the positional
    /// render answer -- reading the positional answer here and writing it
    /// back deleted the flag of whichever row sat first whenever any
    /// unrelated option changed.
    pub(super) rows: Vec<(crate::config::CardRow, bool, bool)>,
    /// The effect name filter as one editable comma-separated line.
    pub(super) effect_filter: String,
}

impl MultiAccountConfig {
    pub(super) fn from_data(data: &crate::config::MultiAccountWidgetData) -> Self {
        Self {
            data: data.clone(),
            rows: data
                .ordered_rows()
                .into_iter()
                .map(|(row, shown)| (row, shown, data.row_merge_flag(row)))
                .collect(),
            effect_filter: data.effect_filter.join(", "),
        }
    }

    /// Write the whole view back into the layout's widget data.
    pub(super) fn apply_to(&self, data: &mut crate::config::MultiAccountWidgetData) {
        *data = self.data.clone();
        data.row_order = self.rows.iter().map(|(r, _, _)| r.id().to_string()).collect();
        for (row, shown, merged) in &self.rows {
            data.set_row_shown(*row, *shown);
            data.set_row_merged(*row, *merged);
        }
        data.effect_filter = self
            .effect_filter
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        data.max_effects = data.max_effects.max(1);
    }
}

/// Everything the Window and widget sections of the context menu render,
/// resolved up front so the menu body stays a static fn.
pub(super) struct WindowConfigView {
    /// Internal window name (the stable key commands act on).
    pub(super) window_name: String,
    /// Core window title (the `.rename` target / stream title).
    pub(super) title: String,
    /// Per-window custom title-bar override; "" = automatic title.
    pub(super) custom_title: String,
    /// Position/size lock from the layout def; None = no layout entry.
    pub(super) locked: Option<bool>,
    pub(super) supports_streams: bool,
    /// Comma-joined stream ids (text-list windows).
    pub(super) streams: String,
    pub(super) max_lines: usize,
    /// Streams seen this session as (id, friendly label), for the picker.
    pub(super) seen_streams: Vec<(String, Option<String>)>,
    /// Compact + timestamps (plain text windows only).
    pub(super) supports_compact: bool,
    pub(super) compact: bool,
    pub(super) show_timestamps: bool,
    pub(super) ts_at_start: bool,
    /// TTS opt-in; None when the window doesn't carry text lines.
    pub(super) tts_speak: Option<bool>,
    /// Section title for the widget-specific section; None = no section.
    pub(super) widget_label: Option<&'static str>,
    pub(super) feed: Option<FeedConfig>,
    pub(super) effects_category: Option<String>,
    pub(super) room: Option<RoomSections>,
    pub(super) targets: Option<TargetsConfig>,
    pub(super) experience: Option<ExperienceConfig>,
    pub(super) encum: Option<EncumConfig>,
    pub(super) multiaccount: Option<MultiAccountConfig>,
    /// MiniVitals: the shared vitals options (GuiUiSettings.vitals).
    pub(super) vitals: Option<crate::frontend::gui::persistence::VitalsConfig>,
    /// Widget types with a dedicated editor reachable from the menu.
    pub(super) is_tabbed: bool,
    pub(super) is_hotkeybar: bool,
    pub(super) is_indicator: bool,
    pub(super) is_targets: bool,
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

/// Section title for the widget-specific settings section. None = the
/// widget has no widget-level settings (the section is omitted entirely).
/// Widgets whose settings live purely in the appearance view (map, doll,
/// compass, hand, dashboard) still need the section, so they're named too.
fn widget_section_label(widget_type: &crate::data::WidgetType) -> Option<&'static str> {
    use crate::data::WidgetType as W;
    Some(match widget_type {
        W::Countdown => "Timer",
        W::Progress => "Bar",
        W::ActiveEffects => "Effects",
        W::Room => "Room",
        W::Targets => "Targets",
        W::GS4Experience => "Experience",
        W::Encumbrance => "Encumbrance",
        W::MiniVitals => "Vitals",
        W::TabbedText => "Tabs",
        W::Hotkeybar => "Hotbar",
        W::Indicator => "Indicator",
        W::Map => "Map",
        W::InjuryDoll => "Injury doll",
        W::Compass => "Compass",
        W::Hand => "Hand",
        W::Dashboard => "Dashboard",
        W::MultiAccount => "Characters",
        _ => return None,
    })
}

/// Draft-buffered single-line text field. Edits accumulate in egui temp
/// data while the field has focus; the value is returned once on Enter or
/// focus loss when it differs from `current`. While unfocused the draft
/// tracks `current` so live changes show up.
fn commit_text_field(
    ui: &mut egui::Ui,
    salt: (&str, &str),
    current: &str,
    hint: &str,
    width: f32,
) -> Option<String> {
    let id = ui.id().with(salt);
    let mut draft: String =
        ui.data_mut(|data| data.get_temp(id).unwrap_or_else(|| current.to_string()));
    let response = ui.add(
        egui::TextEdit::singleline(&mut draft)
            .hint_text(hint)
            .desired_width(width),
    );
    let mut committed = None;
    if response.lost_focus() {
        if draft != current {
            committed = Some(draft.clone());
        }
    } else if !response.has_focus() {
        draft = current.to_string();
    }
    ui.data_mut(|data| data.insert_temp(id, draft));
    committed
}

impl VellumGuiApp {
    /// The shared layout definition for `name`, mutable — the write-back
    /// target every live-apply command persists into (the debounced
    /// autosave then writes it to disk).
    fn layout_def_mut(&mut self, name: &str) -> Option<&mut crate::config::WindowDef> {
        self.app_core
            .layout
            .windows
            .iter_mut()
            .find(|window| window.name() == name)
    }

    /// Resolve the Window/widget section state for `tab_key`; None when the
    /// tab no longer maps to a live window.
    pub(super) fn window_config_view_for_tab(&self, tab_key: &TabKey) -> Option<WindowConfigView> {
        let name = self.available_tabs.get(tab_key)?.window_name.clone();
        let window = self.app_core.ui_state.windows.get(&name)?;

        let custom_title = self
            .tab_settings
            .get(tab_key)
            .and_then(|settings| settings.custom_title.clone())
            .unwrap_or_default();

        let def = self
            .app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == name);
        let locked = def.map(|def| def.base().locked);
        let tts_speak = def
            .filter(|def| {
                matches!(
                    def,
                    crate::config::WindowDef::Text { .. }
                        | crate::config::WindowDef::TabbedText { .. }
                        | crate::config::WindowDef::Inventory { .. }
                        | crate::config::WindowDef::Reserve { .. }
                        | crate::config::WindowDef::Spells { .. }
                )
            })
            .map(|def| def.base().tts_speak);

        let mut view = WindowConfigView {
            window_name: name.clone(),
            title: String::new(),
            custom_title,
            locked,
            supports_streams: false,
            streams: String::new(),
            max_lines: 0,
            seen_streams: Vec::new(),
            supports_compact: false,
            compact: false,
            show_timestamps: false,
            ts_at_start: false,
            tts_speak,
            widget_label: widget_section_label(&window.widget_type),
            feed: None,
            effects_category: None,
            room: None,
            targets: None,
            experience: None,
            encum: None,
            multiaccount: None,
            vitals: None,
            is_tabbed: matches!(window.content, WindowContent::TabbedText(_)),
            is_hotkeybar: window.widget_type == crate::data::WidgetType::Hotkeybar,
            is_indicator: window.widget_type == crate::data::WidgetType::Indicator,
            is_targets: matches!(window.content, WindowContent::Targets),
        };

        if let Some(text) = text_content_of(&window.content) {
            view.title = text.title.clone();
            // Spells windows carry TextContent live, but WindowDef::Spells
            // has no stream/buffer fields to persist and the stream router
            // never subscribes Spells content — offering the fields would
            // show edits that neither apply nor survive a restart.
            if !matches!(window.content, WindowContent::Spells(_)) {
                view.streams = text.streams.join(", ");
                view.max_lines = text.max_lines;
                view.supports_streams = true;
                view.seen_streams = self.app_core.message_processor.seen_streams();
            }
            if matches!(window.content, WindowContent::Text(_)) {
                view.supports_compact = true;
                view.compact = text.compact;
                view.show_timestamps = text.show_timestamps;
                view.ts_at_start = matches!(
                    text.timestamp_position,
                    crate::config::TimestampPosition::Start
                );
            }
        } else {
            view.title = Self::tab_key_for_window(&name, window)
                .map(|key| key.default_title())
                .unwrap_or_else(|| name.clone());
        }

        view.feed = match &window.content {
            WindowContent::Countdown(countdown) => Some(FeedConfig {
                kind: FeedKind::Countdown,
                id: countdown.countdown_id.clone(),
                label: countdown.label.clone(),
                color: countdown.color.clone().unwrap_or_default(),
                numbers_only: false,
                current_only: false,
                show_when_zero: countdown.show_when_zero,
            }),
            WindowContent::Progress(progress) => Some(FeedConfig {
                kind: FeedKind::Progress,
                id: progress.progress_id.clone(),
                label: progress.label.clone(),
                color: progress.color.clone().unwrap_or_default(),
                numbers_only: progress.numbers_only,
                current_only: progress.current_only,
                show_when_zero: false,
            }),
            _ => None,
        };
        if let WindowContent::ActiveEffects(effects) = &window.content {
            view.effects_category = Some(effects.category.clone());
        }
        match def {
            Some(crate::config::WindowDef::Room { data, .. }) => {
                view.room = Some(RoomSections {
                    show_desc: data.show_desc,
                    show_objs: data.show_objs,
                    show_players: data.show_players,
                    show_exits: data.show_exits,
                });
            }
            Some(crate::config::WindowDef::Targets { data, .. }) => {
                view.targets = Some(TargetsConfig {
                    show_appendages: data.show_body_part_count,
                    status_position: data.status_position.clone().unwrap_or_default(),
                });
            }
            Some(crate::config::WindowDef::GS4Experience { data, .. }) => {
                view.experience = Some(ExperienceConfig {
                    show_level: data.show_level,
                    show_mind_bar: data.show_mind_bar,
                    show_exp_bar: data.show_exp_bar,
                    show_total_exp: data.show_total_exp,
                    show_ascension_exp: data.show_ascension_exp,
                    mind_bar_color: data.mind_bar_color.clone().unwrap_or_default(),
                    exp_bar_color: data.exp_bar_color.clone().unwrap_or_default(),
                });
            }
            Some(crate::config::WindowDef::MultiAccount { data, .. }) => {
                view.multiaccount = Some(MultiAccountConfig::from_data(data));
            }
            Some(crate::config::WindowDef::Encumbrance { data, .. }) => {
                view.encum = Some(EncumConfig {
                    show_bar: data.show_bar,
                    show_label: data.show_label,
                });
            }
            _ => {}
        }
        if matches!(window.content, WindowContent::MiniVitals) {
            view.vitals = Some(self.ui_settings.vitals.clone());
        }
        Some(view)
    }

    /// Open the window context menu for `name` without a right-click —
    /// used by `.editwindow <name>` and the blank-custom-widget creation
    /// flow (an unconfigured countdown renders as nothing, so drop the user
    /// straight into the menu that configures it).
    pub(super) fn open_window_menu_for_window(&mut self, name: &str) {
        let Some(window) = self.app_core.ui_state.windows.get(name) else {
            self.app_core
                .add_system_message(&format!("Window '{}' not found.", name));
            return;
        };
        let Some(tab_key) = Self::tab_key_for_window(name, window) else {
            return;
        };
        // A detached window lives in its own OS viewport: give it the
        // detached menu, never the attached one — the attached menu's
        // Group/Arrange/Detach actions are meaningless (and grouping a
        // detached leader can orphan the follower from every render
        // surface).
        if self.detached_tabs.contains_key(&tab_key) {
            self.close_all_popup_menus();
            self.window_context_menu = None;
            self.detached_context_menu = Some(super::detached::DetachedMenuState {
                tab_key,
                pos: egui::pos2(24.0, 40.0),
                just_opened: true,
            });
            return;
        }
        let zone = self.zone_for_tab(&tab_key);
        let rect = self
            .main_window_rects
            .get(&tab_key)
            .copied()
            .and_then(Self::rect_from_snapshot)
            .unwrap_or_else(|| {
                egui::Rect::from_min_size(egui::pos2(120.0, 120.0), egui::vec2(320.0, 220.0))
            });
        self.close_all_popup_menus();
        self.window_context_menu = Some(super::menus::GuiWindowMenuRequest {
            tab_key,
            zone,
            position: rect.min + egui::vec2(16.0, 16.0),
            window_rect: rect,
        });
        self.window_context_menu_just_opened = true;
    }

    /// Apply a Window/widget-section command for `tab_key`. Live-apply:
    /// every change hits the running window and the layout def immediately
    /// (the debounced autosave persists it), mirroring what the Window
    /// Editor's Save used to do per field.
    pub(super) fn apply_window_config_command(
        &mut self,
        tab_key: &TabKey,
        command: GuiWindowMenuCommand,
    ) {
        let Some(name) = self
            .available_tabs
            .get(tab_key)
            .map(|tab| tab.window_name.clone())
        else {
            return;
        };
        match command {
            GuiWindowMenuCommand::Rename(title) => {
                let title = title.trim().to_string();
                if title.is_empty() {
                    self.app_core.add_system_message("Title cannot be empty.");
                    return;
                }
                // Rename through the shared dot-command so the layout
                // definition and system messaging match the TUI exactly.
                let _ = self.app_core.send_command(format!(".rename {} {}", name, title));
                self.refresh_available_tabs_if_needed();
                self.app_core.needs_render = true;
            }
            GuiWindowMenuCommand::SetCustomTitle(custom) => {
                let entry = self.tab_settings.entry(tab_key.clone()).or_default();
                entry.custom_title = custom
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetStreams(raw) => {
                let streams = super::editors::parse_streams(&raw);
                if let Some(text) = self
                    .app_core
                    .ui_state
                    .windows
                    .get_mut(&name)
                    .and_then(|window| text_content_mut(&mut window.content))
                {
                    text.streams = streams.clone();
                } else {
                    return;
                }
                // Stream routing reads a cached subscriber map; rebuild it.
                self.app_core
                    .message_processor
                    .update_text_stream_subscribers(&self.app_core.ui_state);
                self.app_core.refresh_bounty_window(&name);
                match self.layout_def_mut(&name)
                {
                    Some(crate::config::WindowDef::Text { data, .. }) => {
                        data.streams = streams;
                    }
                    Some(crate::config::WindowDef::Inventory { data, .. })
                    | Some(crate::config::WindowDef::Reserve { data, .. }) => {
                        data.streams = streams;
                    }
                    _ => {}
                }
                self.app_core.schedule_layout_autosave();
            }
            GuiWindowMenuCommand::SetBufferLines(max_lines) => {
                let max_lines = max_lines.max(1);
                if let Some(text) = self
                    .app_core
                    .ui_state
                    .windows
                    .get_mut(&name)
                    .and_then(|window| text_content_mut(&mut window.content))
                {
                    text.max_lines = max_lines;
                } else {
                    return;
                }
                match self.layout_def_mut(&name)
                {
                    Some(crate::config::WindowDef::Text { data, .. }) => {
                        data.buffer_size = max_lines;
                    }
                    Some(crate::config::WindowDef::Inventory { data, .. })
                    | Some(crate::config::WindowDef::Reserve { data, .. }) => {
                        data.buffer_size = max_lines;
                    }
                    _ => {}
                }
                self.app_core.schedule_layout_autosave();
            }
            GuiWindowMenuCommand::SetCompact(compact) => {
                if let Some(window) = self.app_core.ui_state.windows.get_mut(&name) {
                    if let WindowContent::Text(text) = &mut window.content {
                        text.compact = compact;
                    }
                }
                // Compaction happens at ingestion; re-feed a bounty window
                // from cached data so the toggle applies now.
                self.app_core.refresh_bounty_window(&name);
                if let Some(crate::config::WindowDef::Text { data, .. }) = self.layout_def_mut(&name)
                {
                    data.compact = compact;
                }
                self.app_core.schedule_layout_autosave();
            }
            GuiWindowMenuCommand::SetTimestamps { show, at_start } => {
                let position = if at_start {
                    crate::config::TimestampPosition::Start
                } else {
                    crate::config::TimestampPosition::End
                };
                if let Some(window) = self.app_core.ui_state.windows.get_mut(&name) {
                    if let WindowContent::Text(text) = &mut window.content {
                        text.show_timestamps = show;
                        text.timestamp_position = position.clone();
                    }
                }
                if let Some(crate::config::WindowDef::Text { data, .. }) = self.layout_def_mut(&name)
                {
                    data.show_timestamps = show;
                    data.timestamp_position = Some(position);
                }
                self.app_core.schedule_layout_autosave();
            }
            GuiWindowMenuCommand::SetTtsSpeak(tts_speak) => {
                if let Some(def) = self.layout_def_mut(&name)
                {
                    if def.base().tts_speak != tts_speak {
                        def.base_mut().tts_speak = tts_speak;
                        self.app_core.refresh_tts_windows();
                        self.app_core.schedule_layout_autosave();
                    }
                }
            }
            GuiWindowMenuCommand::SetFeed(feed) => {
                let id = feed.id.trim().to_string();
                let label = feed.label.trim().to_string();
                let color = feed.color.trim().to_string();
                let Some(window) = self.app_core.ui_state.windows.get_mut(&name) else {
                    return;
                };
                match (&mut window.content, feed.kind) {
                    (WindowContent::Countdown(countdown), FeedKind::Countdown) => {
                        countdown.countdown_id = id.clone();
                        countdown.label = label.clone();
                        countdown.color =
                            Some(color.clone()).filter(|value| !value.is_empty());
                        countdown.show_when_zero = feed.show_when_zero;
                    }
                    (WindowContent::Progress(progress), FeedKind::Progress) => {
                        progress.progress_id = id.clone();
                        progress.label = label.clone();
                        progress.color = Some(color.clone()).filter(|value| !value.is_empty());
                        progress.numbers_only = feed.numbers_only;
                        progress.current_only = feed.current_only;
                    }
                    _ => return,
                }
                fn opt(value: &str) -> Option<String> {
                    if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    }
                }
                if let Some(def) = self.layout_def_mut(&name)
                {
                    match (def, feed.kind) {
                        (
                            crate::config::WindowDef::Countdown { data, .. },
                            FeedKind::Countdown,
                        ) => {
                            data.id = opt(&id);
                            data.label = opt(&label);
                            data.color = opt(&color);
                            data.show_when_zero = Some(feed.show_when_zero);
                        }
                        (
                            crate::config::WindowDef::Progress { data, .. },
                            FeedKind::Progress,
                        ) => {
                            data.id = opt(&id);
                            data.label = opt(&label);
                            data.color = opt(&color);
                            data.numbers_only = feed.numbers_only;
                            data.current_only = feed.current_only;
                        }
                        _ => {}
                    }
                }
                self.app_core.schedule_layout_autosave();
            }
            GuiWindowMenuCommand::SetEffectsCategory(category) => {
                if let Some(window) = self.app_core.ui_state.windows.get_mut(&name) {
                    if let WindowContent::ActiveEffects(effects) = &mut window.content {
                        if effects.category != category {
                            effects.category = category.clone();
                            // Old-category effects are stale under the new feed.
                            effects.effects.clear();
                            effects.generation = effects.generation.wrapping_add(1);
                        }
                    }
                }
                if let Some(crate::config::WindowDef::ActiveEffects { data, .. }) = self.layout_def_mut(&name)
                {
                    data.category = category;
                }
                // Effect routing derives implicit streams from categories.
                self.app_core
                    .message_processor
                    .update_text_stream_subscribers(&self.app_core.ui_state);
                self.app_core.schedule_layout_autosave();
            }
            GuiWindowMenuCommand::SetRoomSections(room) => {
                if let Some(crate::config::WindowDef::Room { data, .. }) = self.layout_def_mut(&name)
                {
                    data.show_desc = room.show_desc;
                    data.show_objs = room.show_objs;
                    data.show_players = room.show_players;
                    data.show_exits = room.show_exits;
                    self.app_core.schedule_layout_autosave();
                }
            }
            GuiWindowMenuCommand::SetTargetsConfig(targets) => {
                if let Some(crate::config::WindowDef::Targets { data, .. }) = self.layout_def_mut(&name)
                {
                    data.show_body_part_count = targets.show_appendages;
                    data.status_position = Some(targets.status_position)
                        .filter(|position| !position.is_empty());
                    self.app_core.schedule_layout_autosave();
                }
            }
            GuiWindowMenuCommand::SetExperienceConfig(experience) => {
                if let Some(crate::config::WindowDef::GS4Experience { data, .. }) = self.layout_def_mut(&name)
                {
                    data.show_level = experience.show_level;
                    data.show_mind_bar = experience.show_mind_bar;
                    data.show_exp_bar = experience.show_exp_bar;
                    data.show_total_exp = experience.show_total_exp;
                    data.show_ascension_exp = experience.show_ascension_exp;
                    let opt = |value: &str| {
                        let trimmed = value.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.to_string())
                        }
                    };
                    data.mind_bar_color = opt(&experience.mind_bar_color);
                    data.exp_bar_color = opt(&experience.exp_bar_color);
                    self.app_core.schedule_layout_autosave();
                }
            }
            GuiWindowMenuCommand::MoveMultiAccountRow { row, up } => {
                if let Some(crate::config::WindowDef::MultiAccount { data, .. }) =
                    self.layout_def_mut(&name)
                {
                    data.move_row(row, up);
                    self.app_core.schedule_layout_autosave();
                }
            }
            GuiWindowMenuCommand::SetMultiAccountConfig(ma) => {
                if let Some(crate::config::WindowDef::MultiAccount { data, .. }) =
                    self.layout_def_mut(&name)
                {
                    ma.apply_to(data);
                    self.app_core.schedule_layout_autosave();
                }
            }
            GuiWindowMenuCommand::SetEncumConfig(encum) => {
                if let Some(crate::config::WindowDef::Encumbrance { data, .. }) = self.layout_def_mut(&name)
                {
                    data.show_bar = encum.show_bar;
                    data.show_label = encum.show_label;
                    self.app_core.schedule_layout_autosave();
                }
            }
            GuiWindowMenuCommand::SetVitals(vitals) => {
                // Vitals options live in GuiUiSettings, saved with the
                // per-character GUI layout file (debounced dirty-flag save).
                self.ui_settings.vitals = vitals;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::DeleteWindow => {
                self.delete_custom_window(&name);
            }
            _ => {}
        }
    }

    /// The "Window" section: identity + content routing + lock, with the
    /// rarely-used and destructive bits folded under Advanced.
    /// egui temp-data key for the Delete Window two-step confirmation.
    fn delete_arm_id(window_name: &str) -> egui::Id {
        egui::Id::new(("cfg_delete_armed", window_name))
    }

    /// Reset the Delete Window confirmation for `window_name`. Called from
    /// every menu-open path so the armed state can't leak across menus.
    pub(super) fn disarm_window_delete(ctx: &egui::Context, window_name: &str) {
        ctx.data_mut(|data| data.remove_temp::<bool>(Self::delete_arm_id(window_name)));
    }

    pub(super) fn render_window_section(
        ui: &mut egui::Ui,
        view: &WindowConfigView,
        locked: bool,
    ) -> Option<GuiWindowMenuCommand> {
        let mut command = None;
        let name = view.window_name.as_str();
        ui.horizontal(|ui| {
            ui.label("Title");
            if let Some(title) =
                commit_text_field(ui, (name, "cfg_title"), &view.title, "", 150.0)
            {
                command = Some(GuiWindowMenuCommand::Rename(title));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Title bar text")
                .on_hover_text(
                    "Custom text shown in the window's title bar. Leave blank \
                     for the automatic title (for a grouped window, the joined \
                     member names).",
                );
            if let Some(custom) = commit_text_field(
                ui,
                (name, "cfg_custom_title"),
                &view.custom_title,
                "(automatic)",
                130.0,
            ) {
                command = Some(GuiWindowMenuCommand::SetCustomTitle(Some(custom)));
            }
        });
        if view.supports_streams {
            ui.horizontal(|ui| {
                ui.label("Streams");
                if let Some(streams) = commit_text_field(
                    ui,
                    (name, "cfg_streams"),
                    &view.streams,
                    "main, speech, ...",
                    130.0,
                ) {
                    command = Some(GuiWindowMenuCommand::SetStreams(streams));
                }
                if !view.seen_streams.is_empty() {
                    ui.menu_button("+", |ui| {
                        ui.set_min_width(180.0);
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for (id, label) in &view.seen_streams {
                                let text = match label {
                                    Some(label) => format!("{} ({})", id, label),
                                    None => id.clone(),
                                };
                                if ui.button(text).clicked() {
                                    let mut streams = view.streams.clone();
                                    super::editors::append_stream_id(&mut streams, id);
                                    command =
                                        Some(GuiWindowMenuCommand::SetStreams(streams));
                                    ui.close();
                                }
                            }
                        });
                    })
                    .response
                    .on_hover_text("Add a stream seen this session");
                }
            });
            ui.horizontal(|ui| {
                ui.label("Buffer lines");
                if let Some(raw) = commit_text_field(
                    ui,
                    (name, "cfg_max_lines"),
                    &view.max_lines.to_string(),
                    "",
                    60.0,
                ) {
                    if let Ok(lines) = raw.trim().parse::<usize>() {
                        command = Some(GuiWindowMenuCommand::SetBufferLines(lines));
                    }
                }
            });
        }
        if let Some(tts_speak) = view.tts_speak {
            let mut speak = tts_speak;
            if ui
                .checkbox(&mut speak, "Speak new lines (TTS)")
                .on_hover_text(
                    "Read lines routed to this window aloud. TTS must be \
                     enabled in Settings > Speech.",
                )
                .changed()
            {
                command = Some(GuiWindowMenuCommand::SetTtsSpeak(speak));
            }
        }
        if view.locked.is_some() {
            if ui
                .selectable_label(locked, if locked { "Locked in place" } else { "Lock in place" })
                .on_hover_text(
                    "Locked windows ignore dragging and resizing — position \
                     and size stay put. Arrange ▸ Move Window still works \
                     deliberately.",
                )
                .clicked()
            {
                command = Some(GuiWindowMenuCommand::ToggleLock);
            }
        }
        ui.collapsing("Advanced", |ui| {
            if view.supports_compact {
                let mut compact = view.compact;
                if ui
                    .checkbox(&mut compact, "Compact")
                    .on_hover_text("Condense known content (e.g. bounty text).")
                    .changed()
                {
                    command = Some(GuiWindowMenuCommand::SetCompact(compact));
                }
                let mut show = view.show_timestamps;
                if ui.checkbox(&mut show, "Timestamps").changed() {
                    command = Some(GuiWindowMenuCommand::SetTimestamps {
                        show,
                        at_start: view.ts_at_start,
                    });
                }
                if view.show_timestamps {
                    let mut at_start = view.ts_at_start;
                    if ui
                        .checkbox(&mut at_start, "Timestamp at line start")
                        .on_hover_text("Unchecked: timestamp at the end of the line.")
                        .changed()
                    {
                        command = Some(GuiWindowMenuCommand::SetTimestamps {
                            show: view.show_timestamps,
                            at_start,
                        });
                    }
                }
                ui.separator();
            }
            // Destructive, so double-gated: inside Advanced AND armed by a
            // first click before the real delete row appears. Fixed Id (not
            // ui.id()-derived) so menu-open sites can disarm it; every open
            // path calls disarm_window_delete, so a dismissed confirmation
            // never survives into the next menu.
            let arm_id = Self::delete_arm_id(name);
            let armed: bool = ui.data_mut(|data| data.get_temp(arm_id).unwrap_or(false));
            if !armed {
                if ui
                    .selectable_label(false, "Delete Window…")
                    .on_hover_text(
                        "Remove this window from the layout entirely (not just \
                         hide). Asks for confirmation first.",
                    )
                    .clicked()
                {
                    ui.data_mut(|data| data.insert_temp(arm_id, true));
                }
            } else {
                ui.label("Really delete this window from the layout?");
                ui.horizontal(|ui| {
                    if ui
                        .button(
                            egui::RichText::new("Delete")
                                .color(ui.visuals().error_fg_color),
                        )
                        .clicked()
                    {
                        ui.data_mut(|data| data.insert_temp(arm_id, false));
                        command = Some(GuiWindowMenuCommand::DeleteWindow);
                    }
                    if ui.button("Cancel").clicked() {
                        ui.data_mut(|data| data.insert_temp(arm_id, false));
                    }
                });
            }
        });
        command
    }

    /// The widget-specific config rows (feed bindings, display toggles,
    /// dedicated-editor links). The widget-specific *appearance* controls
    /// (doll/compass/hand art, map zoom) render next to these from the
    /// appearance view; together they form the widget section.
    pub(super) fn render_widget_config_controls(
        ui: &mut egui::Ui,
        view: &WindowConfigView,
    ) -> Option<GuiWindowMenuCommand> {
        let mut command = None;
        let name = view.window_name.as_str();
        if let Some(feed) = &view.feed {
            ui.horizontal(|ui| {
                ui.label(match feed.kind {
                    FeedKind::Countdown => "Timer id",
                    FeedKind::Progress => "Bar id",
                })
                .on_hover_text(match feed.kind {
                    FeedKind::Countdown => {
                        "Timer feed id this widget tracks: roundtime, casttime, \
                         stuntime, a custom [event_patterns] event_type, or an id \
                         a script pushes via <vellumTimer id='...' value='epoch'/>."
                    }
                    FeedKind::Progress => {
                        "Bar feed id this widget tracks: health, mana, stamina, \
                         spirit, encumlevel, mindState, or a custom id pushed by \
                         Lich."
                    }
                });
                if let Some(id) =
                    commit_text_field(ui, (name, "cfg_feed_id"), &feed.id, "", 120.0)
                {
                    let mut next = feed.clone();
                    next.id = id;
                    command = Some(GuiWindowMenuCommand::SetFeed(next));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Label");
                if let Some(label) =
                    commit_text_field(ui, (name, "cfg_feed_label"), &feed.label, "", 120.0)
                {
                    let mut next = feed.clone();
                    next.label = label;
                    command = Some(GuiWindowMenuCommand::SetFeed(next));
                }
            });
            ui.horizontal(|ui| {
                ui.label(match feed.kind {
                    FeedKind::Countdown => "Fill color",
                    FeedKind::Progress => "Bar color",
                });
                if let Some(color) = commit_text_field(
                    ui,
                    (name, "cfg_feed_color"),
                    &feed.color,
                    "#rrggbb or name",
                    110.0,
                ) {
                    let mut next = feed.clone();
                    next.color = color;
                    command = Some(GuiWindowMenuCommand::SetFeed(next));
                }
            });
            if feed.kind == FeedKind::Progress {
                let mut numbers_only = feed.numbers_only;
                if ui
                    .checkbox(&mut numbers_only, "Show value/max")
                    .on_hover_text("Show the numbers instead of the label.")
                    .changed()
                {
                    let mut next = feed.clone();
                    next.numbers_only = numbers_only;
                    command = Some(GuiWindowMenuCommand::SetFeed(next));
                }
                let mut current_only = feed.current_only;
                if ui
                    .checkbox(&mut current_only, "Show value only")
                    .on_hover_text("Show just the current value.")
                    .changed()
                {
                    let mut next = feed.clone();
                    next.current_only = current_only;
                    command = Some(GuiWindowMenuCommand::SetFeed(next));
                }
            }
            if feed.kind == FeedKind::Countdown {
                let mut show_when_zero = feed.show_when_zero;
                if ui
                    .checkbox(&mut show_when_zero, "Stay visible at rest")
                    .on_hover_text(
                        "Keep the timer visible at zero (e.g. \"RT: 0\" with an \
                         empty bar) instead of hiding it.",
                    )
                    .changed()
                {
                    let mut next = feed.clone();
                    next.show_when_zero = show_when_zero;
                    command = Some(GuiWindowMenuCommand::SetFeed(next));
                }
            }
        }
        if let Some(category) = &view.effects_category {
            ui.horizontal(|ui| {
                ui.label("Category");
                egui::ComboBox::from_id_salt((name, "cfg_fx_category"))
                    .selected_text(category.clone())
                    .show_ui(ui, |ui| {
                        for option in EFFECT_CATEGORIES {
                            if ui
                                .selectable_label(category == option, option)
                                .clicked()
                            {
                                command = Some(GuiWindowMenuCommand::SetEffectsCategory(
                                    option.to_string(),
                                ));
                            }
                        }
                    });
            });
        }
        if let Some(room) = &view.room {
            ui.label("Sections");
            let mut next = room.clone();
            let mut changed = false;
            changed |= ui.checkbox(&mut next.show_desc, "Description").changed();
            changed |= ui.checkbox(&mut next.show_objs, "Objects").changed();
            changed |= ui.checkbox(&mut next.show_players, "Players").changed();
            changed |= ui.checkbox(&mut next.show_exits, "Exits").changed();
            if changed {
                command = Some(GuiWindowMenuCommand::SetRoomSections(next));
            }
        }
        if let Some(targets) = &view.targets {
            let mut show_appendages = targets.show_appendages;
            if ui
                .checkbox(&mut show_appendages, "Show filtered appendage count")
                .on_hover_text(
                    "Body parts (arms, tentacles, …) are filtered from the \
                     target list; show how many were hidden.",
                )
                .changed()
            {
                let mut next = targets.clone();
                next.show_appendages = show_appendages;
                command = Some(GuiWindowMenuCommand::SetTargetsConfig(next));
            }
            ui.horizontal(|ui| {
                ui.label("Status position");
                let position_label = |value: &str| match value {
                    "start" => "Before the name",
                    "end" => "After the name",
                    _ => "Global default",
                };
                egui::ComboBox::from_id_salt((name, "cfg_status_position"))
                    .selected_text(position_label(&targets.status_position))
                    .show_ui(ui, |ui| {
                        for option in ["", "start", "end"] {
                            if ui
                                .selectable_label(
                                    targets.status_position == option,
                                    position_label(option),
                                )
                                .clicked()
                            {
                                let mut next = targets.clone();
                                next.status_position = option.to_string();
                                command =
                                    Some(GuiWindowMenuCommand::SetTargetsConfig(next));
                            }
                        }
                    });
            });
        }
        if let Some(experience) = &view.experience {
            ui.label("Fields");
            let mut next = experience.clone();
            let mut changed = false;
            changed |= ui.checkbox(&mut next.show_level, "Level").changed();
            changed |= ui.checkbox(&mut next.show_mind_bar, "Mind state").changed();
            changed |= ui
                .checkbox(&mut next.show_exp_bar, "Experience bar")
                .changed();
            changed |= ui
                .checkbox(&mut next.show_total_exp, "Total exp")
                .on_hover_text("Total absorbed experience (needs the Lich experience feed).")
                .changed();
            changed |= ui
                .checkbox(&mut next.show_ascension_exp, "Ascension exp")
                .on_hover_text("Total ascension experience (needs the Lich experience feed).")
                .changed();
            if changed {
                command = Some(GuiWindowMenuCommand::SetExperienceConfig(next));
            }
            ui.horizontal(|ui| {
                ui.label("Mind bar color");
                if let Some(color) = commit_text_field(
                    ui,
                    (name, "cfg_mind_color"),
                    &experience.mind_bar_color,
                    "#rrggbb or name",
                    100.0,
                ) {
                    let mut next = experience.clone();
                    next.mind_bar_color = color;
                    command = Some(GuiWindowMenuCommand::SetExperienceConfig(next));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Exp bar color");
                if let Some(color) = commit_text_field(
                    ui,
                    (name, "cfg_exp_color"),
                    &experience.exp_bar_color,
                    "#rrggbb or name",
                    100.0,
                ) {
                    let mut next = experience.clone();
                    next.exp_bar_color = color;
                    command = Some(GuiWindowMenuCommand::SetExperienceConfig(next));
                }
            });
        }
        if let Some(ma) = &view.multiaccount {
            ui.label("Card contents");
            let mut next = ma.clone();
            let mut changed = false;
            changed |= ui
                .checkbox(&mut next.data.show_self, "Your own card")
                .changed();
            changed |= ui
                .checkbox(&mut next.data.show_room, "Room id in header")
                .on_hover_text(
                    "The room number, top-right, red when that character is \
                     not in your room. Hover it for the room name.",
                )
                .changed();
            if next.data.show_vitals {
                ui.indent("ma_vitals", |ui| {
                    changed |= ui
                        .checkbox(
                            &mut next.data.show_absolute_vitals,
                            "Vitals as numbers (51/51)",
                        )
                        .changed();
                });
            }
            if next.data.show_effects {
                ui.indent("ma_effects", |ui| {
                    ui.label(RichText::new("Only show effects matching:").small());
                    changed |= ui
                        .text_edit_singleline(&mut next.effect_filter)
                        .on_hover_text(
                            "Comma-separated name fragments, e.g. \"sleep, bind, web\". \
                             Leave empty to show everything.",
                        )
                        .changed();
                    let mut cap = next.data.max_effects as u32;
                    if ui
                        .add(egui::Slider::new(&mut cap, 1..=12).text("Max per card"))
                        .changed()
                    {
                        next.data.max_effects = cap as usize;
                        changed = true;
                    }
                });
            }
            ui.separator();
            // Card columns, the window Group model in miniature: pick how
            // many, weight their widths, then assign each row below. One
            // column is the classic vertical panel; "doll left, info right"
            // is two.
            ui.horizontal(|ui| {
                ui.label("Card columns");
                let current = next.data.column_weights().len();
                for n in 1..=3usize {
                    if ui
                        .selectable_label(current == n, format!("{n}"))
                        .clicked()
                        && current != n
                    {
                        let mut weights = next.data.column_weights();
                        weights.resize(n, 1.0);
                        next.data.card_column_weights = weights;
                        changed = true;
                    }
                }
            });
            if next.data.column_weights().len() > 1 {
                ui.horizontal(|ui| {
                    ui.label("Size weights");
                    let mut weights = next.data.column_weights();
                    let mut edited = false;
                    for weight in weights.iter_mut() {
                        if ui
                            .add(
                                egui::DragValue::new(weight)
                                    .speed(0.05)
                                    .range(0.2..=5.0)
                                    .max_decimals(2),
                            )
                            .changed()
                        {
                            edited = true;
                        }
                    }
                    if edited {
                        next.data.card_column_weights = weights;
                        changed = true;
                    }
                })
                .response
                .on_hover_text(
                    "Relative column widths, like the Group system's size \
                     weights: 1 and 2 makes the second column twice as wide.",
                );
            }
            ui.label(RichText::new("Rows (top to bottom)").small());
            {
                // Same vocabulary as the Group section above: arrows to
                // reorder, a cue on rows living in the line above, a
                // fully-worded share checkbox in an indent, and -- with
                // several columns -- a column picker per row.
                let column_count = next.data.column_weights().len();
                let last = ma.rows.len().saturating_sub(1);
                for (index, (row, shown, merged)) in ma.rows.iter().enumerate() {
                    let row = *row;
                    let shares = index > 0 && *merged;
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(index > 0, egui::Button::new("\u{2B06}").small())
                            .clicked()
                        {
                            command = Some(GuiWindowMenuCommand::MoveMultiAccountRow {
                                row,
                                up: true,
                            });
                        }
                        if ui
                            .add_enabled(index < last, egui::Button::new("\u{2B07}").small())
                            .clicked()
                        {
                            command = Some(GuiWindowMenuCommand::MoveMultiAccountRow {
                                row,
                                up: false,
                            });
                        }
                        if shares {
                            // Visual cue: this row draws on the line opened
                            // by the row above it (Group list convention).
                            // U+00BB, not U+2514 -- the box-drawing corner
                            // is missing from the bundled fonts and rendered
                            // as a tofu square.
                            ui.label(RichText::new("\u{00BB}").weak())
                                .on_hover_text("Shares the line above");
                        }
                        let mut on = *shown;
                        if ui.checkbox(&mut on, row.label()).changed() {
                            if let Some(entry) =
                                next.rows.iter_mut().find(|(r, _, _)| *r == row)
                            {
                                entry.1 = on;
                            }
                            changed = true;
                        }
                        if column_count > 1 {
                            // Right-aligned column picker so labels stay in
                            // a clean left edge.
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let current = next.data.row_column(row);
                                    for col in (0..column_count).rev() {
                                        if ui
                                            .selectable_label(
                                                current == col,
                                                format!("{}", col + 1),
                                            )
                                            .on_hover_text(format!(
                                                "Draw this row in column {}",
                                                col + 1
                                            ))
                                            .clicked()
                                            && current != col
                                        {
                                            next.data.set_row_column(row, col);
                                            changed = true;
                                        }
                                    }
                                },
                            );
                        }
                    });
                    // Big rows (vitals, doll) never squeeze onto a shared
                    // horizontal strip -- putting things BESIDE them is what
                    // the columns above are for -- so no share checkbox.
                    if index > 0 && *shown && !row.full_width() {
                        ui.indent((row.id(), "ma_row_share"), |ui| {
                            let mut share = *merged;
                            if ui
                                .checkbox(&mut share, "Share line with the row above")
                                .on_hover_text(
                                    "Compact rows mix freely on one line \
                                     (within the same column).",
                                )
                                .changed()
                            {
                                if let Some(entry) =
                                    next.rows.iter_mut().find(|(r, _, _)| *r == row)
                                {
                                    entry.2 = share;
                                }
                                changed = true;
                            }
                        });
                    }
                }
            }
            ui.horizontal(|ui| {
                ui.label("Card order");
                for (value, label) in [
                    ("group", "Group"),
                    ("name", "Name"),
                    ("port", "Connected"),
                ] {
                    if ui
                        .selectable_label(next.data.sort_by == value, label)
                        .clicked()
                    {
                        next.data.sort_by = value.to_string();
                        changed = true;
                    }
                }
            });
            changed |= ui
                .add(
                    egui::Slider::new(&mut next.data.card_width, 90.0..=320.0)
                        .text("Card width")
                        .suffix(" px"),
                )
                .changed();
            if changed {
                command = Some(GuiWindowMenuCommand::SetMultiAccountConfig(next));
            }
        }

        if let Some(encum) = &view.encum {
            let mut show_bar = encum.show_bar;
            if ui.checkbox(&mut show_bar, "Level bar").changed() {
                command = Some(GuiWindowMenuCommand::SetEncumConfig(EncumConfig {
                    show_bar,
                    show_label: encum.show_label,
                }));
            }
            let mut show_label = encum.show_label;
            if ui.checkbox(&mut show_label, "Help text").changed() {
                command = Some(GuiWindowMenuCommand::SetEncumConfig(EncumConfig {
                    show_bar: encum.show_bar,
                    show_label,
                }));
            }
        }
        if let Some(vitals) = &view.vitals {
            if let Some(next) = render_vitals_controls(ui, name, vitals) {
                command = Some(GuiWindowMenuCommand::SetVitals(next));
            }
        }
        if view.is_tabbed
            && ui
                .selectable_label(false, "Edit tabs…")
                .on_hover_text("Tab names, streams, and order")
                .clicked()
        {
            command = Some(GuiWindowMenuCommand::EditTabs);
        }
        if view.is_hotkeybar
            && ui
                .selectable_label(false, "Edit hotbars…")
                .on_hover_text("Buttons, commands, conditions, and styling")
                .clicked()
        {
            command = Some(GuiWindowMenuCommand::EditHotbar);
        }
        if view.is_indicator
            && ui
                .selectable_label(false, "Edit indicators…")
                .on_hover_text("Indicator templates: ids, icons, and colors")
                .clicked()
        {
            command = Some(GuiWindowMenuCommand::EditIndicators);
        }
        if view.is_targets
            && ui
                .selectable_label(false, "Global target settings…")
                .on_hover_text(
                    "Colors, truncation, excluded nouns, and status \
                     abbreviations — in Settings ▸ Targets (they apply \
                     everywhere, not just this window).",
                )
                .clicked()
        {
            command = Some(GuiWindowMenuCommand::OpenTargetsSettings);
        }
        command
    }
}

/// Vitals options (MiniVitals windows): orientation, bar sizing/text, and
/// the enabled-bars order. Returns the whole edited config on any change.
fn render_vitals_controls(
    ui: &mut egui::Ui,
    name: &str,
    vitals: &crate::frontend::gui::persistence::VitalsConfig,
) -> Option<crate::frontend::gui::persistence::VitalsConfig> {
    use crate::frontend::gui::persistence::{VitalKind, VitalsOrientation, VitalsTextFormat};
    let mut next: Option<crate::frontend::gui::persistence::VitalsConfig> = None;
    ui.horizontal(|ui| {
        ui.label("Layout");
        egui::ComboBox::from_id_salt((name, "cfg_vitals_orientation"))
            .selected_text(match vitals.orientation {
                VitalsOrientation::Horizontal => "One row",
                VitalsOrientation::Vertical => "Stacked",
            })
            .show_ui(ui, |ui| {
                for (option, label) in [
                    (VitalsOrientation::Horizontal, "One row"),
                    (VitalsOrientation::Vertical, "Stacked"),
                ] {
                    if ui
                        .selectable_label(vitals.orientation == option, label)
                        .clicked()
                    {
                        let mut edited = vitals.clone();
                        edited.orientation = option;
                        next = Some(edited);
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Bar height");
        let mut height = vitals.bar_height;
        if ui
            .add(egui::Slider::new(&mut height, 8.0..=60.0).step_by(1.0))
            .changed()
        {
            let mut edited = vitals.clone();
            edited.bar_height = height;
            next = Some(edited);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Bar text");
        let format_label = |format: VitalsTextFormat| match format {
            VitalsTextFormat::LabelValueMax => "Health: 191/193",
            VitalsTextFormat::LabelPercent => "Health: 99%",
            VitalsTextFormat::ValueMax => "191/193",
            VitalsTextFormat::Percent => "99%",
            VitalsTextFormat::None => "No text",
        };
        egui::ComboBox::from_id_salt((name, "cfg_vitals_text"))
            .selected_text(format_label(vitals.text_format))
            .show_ui(ui, |ui| {
                for format in [
                    VitalsTextFormat::LabelValueMax,
                    VitalsTextFormat::LabelPercent,
                    VitalsTextFormat::ValueMax,
                    VitalsTextFormat::Percent,
                    VitalsTextFormat::None,
                ] {
                    if ui
                        .selectable_label(vitals.text_format == format, format_label(format))
                        .clicked()
                    {
                        let mut edited = vitals.clone();
                        edited.text_format = format;
                        next = Some(edited);
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Depleted color")
            .on_hover_text("Color of the unfilled portion of each bar.");
        let current = vitals.depleted_color.clone().unwrap_or_default();
        if let Some(color) = commit_text_field(
            ui,
            (name, "cfg_vitals_depleted"),
            &current,
            "theme default",
            100.0,
        ) {
            let mut edited = vitals.clone();
            edited.depleted_color =
                Some(color.trim().to_string()).filter(|value| !value.is_empty());
            next = Some(edited);
        }
    });
    ui.label("Bars shown (in display order):");
    // Enabled bars keep their stored order — it IS the display order.
    let bar_count = vitals.bars.len();
    for (index, kind) in vitals.bars.iter().enumerate() {
        ui.horizontal(|ui| {
            let mut enabled = true;
            if ui.checkbox(&mut enabled, kind.label()).changed() {
                let mut edited = vitals.clone();
                edited.bars.remove(index);
                next = Some(edited);
            }
            // Geometric triangles render in the bundled fonts; the ↑/↓
            // arrow block is tofu.
            if ui
                .add_enabled(index > 0, egui::Button::new("▲").small())
                .on_hover_text("Move up")
                .clicked()
            {
                let mut edited = vitals.clone();
                edited.bars.swap(index, index - 1);
                next = Some(edited);
            }
            if ui
                .add_enabled(index + 1 < bar_count, egui::Button::new("▼").small())
                .on_hover_text("Move down")
                .clicked()
            {
                let mut edited = vitals.clone();
                edited.bars.swap(index, index + 1);
                next = Some(edited);
            }
        });
    }
    // Disabled bars: enabling appends at the end of the order.
    for kind in VitalKind::all() {
        if vitals.bars.contains(&kind) {
            continue;
        }
        let mut enabled = false;
        if ui.checkbox(&mut enabled, kind.label()).changed() {
            let mut edited = vitals.clone();
            edited.bars.push(kind);
            next = Some(edited);
        }
    }
    next
}
