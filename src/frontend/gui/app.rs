use super::persistence::{load_layout, save_layout, GuiLayoutFileV1, ViewportState};
use super::{TabId, TabKey};
use crate::cmdlist::CmdList;
use crate::config::{AppKeybinds, Config, KeyBindAction};
use crate::core::AppCore;
use crate::data::{
    InputMode, LinkData, PopupMenu, SpanType, StyledLine, TabbedTextContent, TextContent,
    TextSegment, WidgetType, WindowContent, WindowState,
};
use crate::network::{LichConnection, RawLogger, ServerMessage};
use anyhow::{anyhow, Context, Result};
use eframe::egui;
use eframe::egui::{Color32, Pos2, Rect, RichText, Vec2, ViewportBuilder};
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{DockArea, DockState, Surface, SurfaceIndex, TabViewer};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::mpsc;

const INITIAL_LAYOUT_WIDTH: u16 = 160;
const INITIAL_LAYOUT_HEIGHT: u16 = 50;
const DEFAULT_FONT_SIZE: f32 = 14.0;
const MAX_RENDERED_LINES: usize = 2000;
const MIN_VISIBLE_VIEWPORT_PX: f32 = 48.0;
const MIN_VIEWPORT_WIDTH: f32 = 180.0;
const MIN_VIEWPORT_HEIGHT: f32 = 120.0;

#[derive(Clone, Debug)]
struct GuiTab {
    id: TabId,
    window_name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct DockStateSnapshot {
    visible_tabs: Vec<TabKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppShortcut {
    Quit,
    StartSearch,
    CloseWindow,
}

#[derive(Clone, Debug)]
enum GlobalDispatchTarget {
    Macro(KeyBindAction),
    Shortcut(AppShortcut),
}

#[derive(Clone, Copy, Debug)]
struct GuiKeyPress {
    key_event: crate::frontend::common::KeyEvent,
    logical_key: Option<egui::Key>,
    physical_key: Option<egui::Key>,
    modifiers: egui::Modifiers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GuiLinkDispatch {
    NetworkCommand(String),
    MenuRequest { exist_id: String, noun: String },
}

#[derive(Clone, Copy, Debug)]
enum GuiMenuLayer {
    Main,
    Submenu,
    Nested,
    Deep,
}

#[derive(Clone, Debug)]
struct GuiLinkClick {
    link_data: LinkData,
    click_pos: (u16, u16),
}

pub struct VellumGuiApp {
    app_core: AppCore,
    _runtime: tokio::runtime::Runtime,
    command_tx: mpsc::UnboundedSender<String>,
    server_rx: mpsc::UnboundedReceiver<ServerMessage>,
    network_handle: Option<tokio::task::JoinHandle<()>>,
    command_input: String,
    close_requested: bool,
    dock_state: Option<DockState<GuiTab>>,
    available_tabs: HashMap<TabKey, GuiTab>,
    hidden_tabs: HashSet<TabKey>,
    layout_profile: String,
    layout_character: String,
    layout_dirty: bool,
    pending_detached_viewports: Vec<ViewportState>,
    last_monitor_bounds: Option<[f32; 4]>,
}

impl VellumGuiApp {
    pub fn new(
        mut app_core: AppCore,
        login_key: Option<String>,
        initial_width: f32,
        initial_height: f32,
    ) -> Result<Self> {
        app_core.init_windows(
            initial_width.max(1.0) as u16,
            initial_height.max(1.0) as u16,
        );

        let runtime = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let (server_tx, server_rx) = mpsc::unbounded_channel::<ServerMessage>();
        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();

        let host = app_core.config.connection.host.clone();
        let port = app_core.config.connection.port;

        let raw_logger = match RawLogger::new(&app_core.config) {
            Ok(logger) => logger,
            Err(err) => {
                tracing::error!("Failed to initialize raw logger: {}", err);
                None
            }
        };

        let network_handle = runtime.spawn(async move {
            if let Err(err) =
                LichConnection::start(&host, port, login_key, server_tx, command_rx, raw_logger)
                    .await
            {
                tracing::error!("GUI network connection error: {}", err);
            }
        });

        let (layout_profile, layout_character) = Self::resolve_layout_ids(&app_core.config);
        let persisted_layout = load_layout(&layout_profile, &layout_character).ok();

        let available_tabs = Self::collect_available_tabs(&app_core);
        let mut hidden_tabs: HashSet<TabKey> = persisted_layout
            .as_ref()
            .map(|layout| layout.hidden_tabs.iter().cloned().collect())
            .unwrap_or_default();
        hidden_tabs.retain(|key| available_tabs.contains_key(key));

        let snapshot = persisted_layout
            .as_ref()
            .and_then(|layout| Self::dock_snapshot_from_layout(layout));

        let detached_viewports = persisted_layout
            .as_ref()
            .map(|layout| {
                Self::detached_viewports_from_layout(layout, &available_tabs, &hidden_tabs)
            })
            .unwrap_or_default();
        let detached_tab_keys: HashSet<TabKey> = detached_viewports
            .iter()
            .map(|viewport| viewport.tab.clone())
            .collect();

        let visible_tabs = Self::build_visible_tabs(
            &available_tabs,
            &hidden_tabs,
            &detached_tab_keys,
            snapshot.as_ref(),
        );
        let mut dock_state = if visible_tabs.is_empty() && detached_viewports.is_empty() {
            None
        } else {
            Some(DockState::new(visible_tabs))
        };
        if let Some(dock_state) = &mut dock_state {
            let initial_bounds = [0.0, 0.0, initial_width.max(1.0), initial_height.max(1.0)];
            Self::attach_detached_windows(
                dock_state,
                &available_tabs,
                &detached_viewports,
                Some(initial_bounds),
            );
        }

        Ok(Self {
            app_core,
            _runtime: runtime,
            command_tx,
            server_rx,
            network_handle: Some(network_handle),
            command_input: String::new(),
            close_requested: false,
            dock_state,
            available_tabs,
            hidden_tabs,
            layout_profile,
            layout_character,
            layout_dirty: false,
            pending_detached_viewports: detached_viewports,
            last_monitor_bounds: None,
        })
    }

    fn resolve_layout_ids(config: &Config) -> (String, String) {
        let profile_id = config
            .character
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let character_id = config
            .connection
            .character
            .clone()
            .or_else(|| config.character.clone())
            .unwrap_or_else(|| "default".to_string());
        (profile_id, character_id)
    }

    fn collect_available_tabs(app_core: &AppCore) -> HashMap<TabKey, GuiTab> {
        let mut keys: Vec<String> = app_core.ui_state.windows.keys().cloned().collect();
        keys.sort();

        let mut tabs = HashMap::new();
        for name in keys {
            let Some(window) = app_core.ui_state.windows.get(&name) else {
                continue;
            };

            let Some(tab_key) = Self::tab_key_for_window(&name, window) else {
                continue;
            };

            tabs.entry(tab_key.clone()).or_insert_with(|| GuiTab {
                id: TabId::with_title(tab_key, window.name.clone()),
                window_name: name.clone(),
            });
        }

        tabs
    }

    fn tab_key_for_window(name: &str, window: &WindowState) -> Option<TabKey> {
        let key = match window.widget_type {
            WidgetType::CommandInput | WidgetType::Spacer => return None,
            WidgetType::Text | WidgetType::TabbedText => {
                if Self::is_main_stream_window(name, window) {
                    TabKey::TextMain
                } else {
                    TabKey::TextByName {
                        id: name.to_string(),
                    }
                }
            }
            WidgetType::Inventory => TabKey::Inventory {
                id: name.to_string(),
            },
            WidgetType::ActiveEffects => TabKey::ActiveEffects {
                id: name.to_string(),
            },
            WidgetType::Quickbar => TabKey::Quickbar {
                id: name.to_string(),
            },
            WidgetType::MiniVitals | WidgetType::Progress => TabKey::Vitals,
            WidgetType::Countdown => TabKey::Countdown,
            WidgetType::Compass => TabKey::Compass,
            WidgetType::Indicator => TabKey::Indicators,
            WidgetType::Targets => TabKey::Targets,
            WidgetType::Players => TabKey::Players,
            WidgetType::Room => TabKey::Room,
            WidgetType::Experience | WidgetType::GS4Experience => TabKey::Experience,
            WidgetType::InjuryDoll => TabKey::InjuryDoll,
            WidgetType::Dashboard => TabKey::Dashboard,
            WidgetType::Encumbrance => TabKey::Encumbrance,
            WidgetType::Perception => TabKey::Perception,
            WidgetType::Hand => {
                let lower = name.to_ascii_lowercase();
                if lower.contains("left") {
                    TabKey::LeftHand
                } else if lower.contains("right") {
                    TabKey::RightHand
                } else {
                    TabKey::SpellHand
                }
            }
            _ => TabKey::TextByName {
                id: name.to_string(),
            },
        };

        Some(key)
    }

    fn is_main_stream_window(name: &str, window: &WindowState) -> bool {
        if name.eq_ignore_ascii_case("main") {
            return true;
        }

        match &window.content {
            WindowContent::Text(content)
            | WindowContent::Inventory(content)
            | WindowContent::Spells(content) => content
                .streams
                .iter()
                .any(|stream| stream.eq_ignore_ascii_case("main")),
            WindowContent::TabbedText(tabbed) => Self::find_main_tab(tabbed).is_some(),
            _ => false,
        }
    }

    fn find_main_tab(tabbed: &TabbedTextContent) -> Option<&crate::data::TabState> {
        tabbed.tabs.iter().find(|tab| {
            tab.definition
                .streams
                .iter()
                .any(|stream| stream.eq_ignore_ascii_case("main"))
        })
    }

    fn dock_snapshot_from_layout(layout: &GuiLayoutFileV1) -> Option<DockStateSnapshot> {
        if layout.dock_state_json.is_null() {
            return None;
        }
        serde_json::from_value(layout.dock_state_json.clone()).ok()
    }

    fn build_visible_tabs(
        available_tabs: &HashMap<TabKey, GuiTab>,
        hidden_tabs: &HashSet<TabKey>,
        detached_tabs: &HashSet<TabKey>,
        snapshot: Option<&DockStateSnapshot>,
    ) -> Vec<GuiTab> {
        let mut visible = Vec::new();
        let mut seen = HashSet::new();

        if let Some(snapshot) = snapshot {
            for key in &snapshot.visible_tabs {
                if hidden_tabs.contains(key) || detached_tabs.contains(key) {
                    continue;
                }
                if let Some(tab) = available_tabs.get(key) {
                    visible.push(tab.clone());
                    seen.insert(key.clone());
                }
            }
        }

        let mut fallback_tabs: Vec<GuiTab> = available_tabs
            .iter()
            .filter_map(|(key, tab)| {
                if hidden_tabs.contains(key) || detached_tabs.contains(key) || seen.contains(key) {
                    None
                } else {
                    Some(tab.clone())
                }
            })
            .collect();
        fallback_tabs.sort_by_key(|tab| tab.id.title.to_ascii_lowercase());

        visible.extend(fallback_tabs);
        visible
    }

    fn detached_viewports_from_layout(
        layout: &GuiLayoutFileV1,
        available_tabs: &HashMap<TabKey, GuiTab>,
        hidden_tabs: &HashSet<TabKey>,
    ) -> Vec<ViewportState> {
        let mut entries: Vec<(&String, &ViewportState)> =
            layout.detached_viewports.iter().collect();
        entries.sort_by(|(left, _), (right, _)| left.cmp(right));

        let mut detached = Vec::new();
        let mut seen = HashSet::new();
        for (_, state) in entries {
            if hidden_tabs.contains(&state.tab) || !available_tabs.contains_key(&state.tab) {
                continue;
            }
            if seen.insert(state.tab.clone()) {
                detached.push(state.clone());
            }
        }
        detached
    }

    fn current_main_surface_tab_keys(&self) -> Vec<TabKey> {
        if let Some(dock_state) = &self.dock_state {
            let mut visible = Vec::new();
            let mut seen = HashSet::new();
            for ((surface, _), tab) in dock_state.iter_all_tabs() {
                if surface.is_main() && seen.insert(tab.id.key.clone()) {
                    visible.push(tab.id.key.clone());
                }
            }
            if !visible.is_empty() {
                return visible;
            }
        }

        let mut visible: Vec<(String, TabKey)> = self
            .available_tabs
            .iter()
            .filter_map(|(key, tab)| {
                if self.hidden_tabs.contains(key) {
                    None
                } else {
                    Some((tab.id.title.clone(), key.clone()))
                }
            })
            .collect();
        visible.sort_by_key(|(title, _)| title.to_ascii_lowercase());
        visible.into_iter().map(|(_, key)| key).collect()
    }

    fn collect_detached_tab_keys(dock_state: &DockState<GuiTab>) -> HashSet<TabKey> {
        let mut detached = HashSet::new();
        for ((surface, _), tab) in dock_state.iter_all_tabs() {
            if !surface.is_main() {
                detached.insert(tab.id.key.clone());
            }
        }
        detached
    }

    fn sanitize_viewport_state(
        state: &ViewportState,
        monitor_bounds: Option<[f32; 4]>,
    ) -> ViewportState {
        let mut viewport = state.clone();
        viewport.outer_size_px[0] = viewport.outer_size_px[0].max(MIN_VIEWPORT_WIDTH);
        viewport.outer_size_px[1] = viewport.outer_size_px[1].max(MIN_VIEWPORT_HEIGHT);
        if let Some(bounds) = monitor_bounds {
            viewport.clamp_to_bounds(bounds, MIN_VISIBLE_VIEWPORT_PX);
        }
        viewport
    }

    fn apply_viewport_to_surface(
        dock_state: &mut DockState<GuiTab>,
        surface: SurfaceIndex,
        viewport: &ViewportState,
        monitor_bounds: Option<[f32; 4]>,
    ) {
        let viewport = Self::sanitize_viewport_state(viewport, monitor_bounds);
        if let Some(window_state) = dock_state.get_window_state_mut(surface) {
            window_state
                .set_position(Pos2::new(
                    viewport.outer_pos_px[0],
                    viewport.outer_pos_px[1],
                ))
                .set_size(Vec2::new(
                    viewport.outer_size_px[0],
                    viewport.outer_size_px[1],
                ));
        }
    }

    fn attach_detached_windows(
        dock_state: &mut DockState<GuiTab>,
        available_tabs: &HashMap<TabKey, GuiTab>,
        detached_viewports: &[ViewportState],
        monitor_bounds: Option<[f32; 4]>,
    ) {
        let mut attached = HashSet::new();
        for viewport in detached_viewports {
            if !attached.insert(viewport.tab.clone()) {
                continue;
            }
            let Some(tab) = available_tabs.get(&viewport.tab).cloned() else {
                continue;
            };
            let surface = dock_state.add_window(vec![tab]);
            Self::apply_viewport_to_surface(dock_state, surface, viewport, monitor_bounds);
        }
    }

    fn collect_detached_viewports_for_save(
        dock_state: &mut DockState<GuiTab>,
        monitor_bounds: Option<[f32; 4]>,
    ) -> HashMap<String, ViewportState> {
        let mut detached = HashMap::new();
        let surface_count = dock_state.surfaces_count();

        for raw_index in 1..surface_count {
            let surface = SurfaceIndex(raw_index);
            let tabs: Vec<TabKey> = match dock_state.get_surface(surface) {
                Some(Surface::Window(tree, _)) => tree
                    .iter()
                    .flat_map(|node| node.iter_tabs())
                    .map(|tab| tab.id.key.clone())
                    .collect(),
                _ => Vec::new(),
            };
            if tabs.is_empty() {
                continue;
            }

            let rect = dock_state
                .get_window_state(surface)
                .map(|state| state.rect())
                .unwrap_or(Rect::NOTHING);
            let fallback = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(600.0, 400.0));
            let safe_rect =
                if rect.is_finite() && rect.width().is_finite() && rect.height().is_finite() {
                    rect
                } else {
                    fallback
                };

            for tab_key in tabs {
                let mut viewport = ViewportState::new(
                    tab_key.clone(),
                    [safe_rect.min.x, safe_rect.min.y],
                    [safe_rect.width(), safe_rect.height()],
                );
                if let Some(bounds) = monitor_bounds {
                    viewport.clamp_to_bounds(bounds, MIN_VISIBLE_VIEWPORT_PX);
                }
                let id = format!("vp_surface{}_{}", raw_index, tab_key.short_id());
                detached.insert(id, viewport);
            }
        }

        detached
    }

    fn rebuild_dock_state(&mut self) {
        let detached_viewports = self
            .dock_state
            .as_mut()
            .map(|dock_state| {
                Self::collect_detached_viewports_for_save(dock_state, self.last_monitor_bounds)
            })
            .unwrap_or_default();
        let detached_viewports: Vec<ViewportState> = detached_viewports.into_values().collect();
        let detached_keys: HashSet<TabKey> = detached_viewports
            .iter()
            .map(|viewport| viewport.tab.clone())
            .collect();

        let visible_tabs = Self::build_visible_tabs(
            &self.available_tabs,
            &self.hidden_tabs,
            &detached_keys,
            None,
        );
        self.dock_state = if visible_tabs.is_empty() && detached_viewports.is_empty() {
            None
        } else {
            let mut dock_state = DockState::new(visible_tabs);
            Self::attach_detached_windows(
                &mut dock_state,
                &self.available_tabs,
                &detached_viewports,
                self.last_monitor_bounds,
            );
            Some(dock_state)
        };
    }

    fn refresh_available_tabs_if_needed(&mut self) {
        let refreshed = Self::collect_available_tabs(&self.app_core);
        if refreshed.len() == self.available_tabs.len()
            && refreshed
                .keys()
                .all(|key| self.available_tabs.contains_key(key))
        {
            return;
        }

        self.available_tabs = refreshed;
        self.hidden_tabs
            .retain(|key| self.available_tabs.contains_key(key));
        self.rebuild_dock_state();
        self.layout_dirty = true;
    }

    fn hide_tab(&mut self, key: TabKey) {
        if self.hidden_tabs.insert(key) {
            self.rebuild_dock_state();
            self.layout_dirty = true;
        }
    }

    fn restore_tab(&mut self, key: TabKey) {
        if self.hidden_tabs.remove(&key) {
            self.rebuild_dock_state();
            self.layout_dirty = true;
        }
    }

    fn hidden_tabs_for_menu(&self) -> Vec<(TabKey, String)> {
        let mut entries: Vec<(TabKey, String)> = self
            .hidden_tabs
            .iter()
            .filter_map(|key| {
                self.available_tabs
                    .get(key)
                    .map(|tab| (key.clone(), tab.id.title.clone()))
            })
            .collect();
        entries.sort_by_key(|(_, title)| title.to_ascii_lowercase());
        entries
    }

    fn monitor_bounds_from_ctx(ctx: &egui::Context) -> [f32; 4] {
        ctx.input(|input| {
            if let (Some(outer_rect), Some(monitor_size)) =
                (input.viewport().outer_rect, input.viewport().monitor_size)
            {
                [
                    outer_rect.min.x,
                    outer_rect.min.y,
                    monitor_size.x.max(1.0),
                    monitor_size.y.max(1.0),
                ]
            } else {
                let screen = input.screen_rect();
                [screen.min.x, screen.min.y, screen.width(), screen.height()]
            }
        })
    }

    fn apply_pending_detached_viewports(&mut self, monitor_bounds: [f32; 4]) {
        if self.pending_detached_viewports.is_empty() {
            return;
        }
        if let Some(dock_state) = &mut self.dock_state {
            for viewport in &self.pending_detached_viewports {
                let mut target_surface = None;
                for ((surface, _), tab) in dock_state.iter_all_tabs() {
                    if !surface.is_main() && tab.id.key == viewport.tab {
                        target_surface = Some(surface);
                        break;
                    }
                }
                if let Some(surface) = target_surface {
                    Self::apply_viewport_to_surface(
                        dock_state,
                        surface,
                        viewport,
                        Some(monitor_bounds),
                    );
                }
            }
        }
        self.pending_detached_viewports.clear();
        self.layout_dirty = true;
    }

    fn hide_removed_detached_tabs(&mut self, detached_before_frame: &HashSet<TabKey>) {
        if detached_before_frame.is_empty() {
            return;
        }

        let detached_after_frame = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .unwrap_or_default();
        let all_tabs_after: HashSet<TabKey> = self
            .dock_state
            .as_ref()
            .map(|dock_state| {
                dock_state
                    .iter_all_tabs()
                    .map(|(_, tab)| tab.id.key.clone())
                    .collect()
            })
            .unwrap_or_default();

        for key in detached_before_frame {
            if detached_after_frame.contains(key) || all_tabs_after.contains(key) {
                continue;
            }
            self.hide_tab(key.clone());
        }
    }

    fn save_layout_state(&mut self) {
        let mut layout = GuiLayoutFileV1::new(&self.layout_profile, &self.layout_character);

        let mut hidden_tabs: Vec<TabKey> = self.hidden_tabs.iter().cloned().collect();
        hidden_tabs.sort_by_key(|key| key.short_id());
        layout.hidden_tabs = hidden_tabs;

        let snapshot = DockStateSnapshot {
            visible_tabs: self.current_main_surface_tab_keys(),
        };
        layout.dock_state_json = serde_json::to_value(snapshot).unwrap_or(serde_json::Value::Null);
        if let Some(dock_state) = &mut self.dock_state {
            layout.detached_viewports =
                Self::collect_detached_viewports_for_save(dock_state, self.last_monitor_bounds);
        }
        layout.touch();

        if let Err(err) = save_layout(&layout, &self.layout_profile, &self.layout_character) {
            tracing::warn!("Failed to save GUI layout: {}", err);
        }
    }

    fn pump_server_messages(&mut self) {
        while let Ok(message) = self.server_rx.try_recv() {
            match message {
                ServerMessage::Text(line) => {
                    self.app_core
                        .perf_stats
                        .record_bytes_received((line.len() + 1) as u64);
                    if let Err(err) = self.app_core.process_server_data(&line) {
                        self.app_core
                            .add_system_message(&format!("GUI parse error: {}", err));
                    }
                    self.app_core.needs_render = true;
                }
                ServerMessage::Connected => {
                    self.app_core.game_state.connected = true;
                    self.app_core.needs_render = true;
                }
                ServerMessage::Disconnected => {
                    self.app_core.game_state.connected = false;
                    self.app_core.needs_render = true;
                }
            }
        }
    }

    fn handle_global_input(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let mut key_presses = Self::collect_numpad_key_events(frame);
        key_presses.extend(Self::collect_pressed_key_events(ctx));
        if key_presses.is_empty() {
            return;
        }

        let suppress_macro_dispatch = self.should_suppress_macro_dispatch();
        let mut consumed_keyboard_input = false;

        for key_press in key_presses {
            let target = Self::resolve_global_dispatch_target(
                key_press.key_event,
                &self.app_core.keybind_map,
                &self.app_core.config.app_keybinds,
                suppress_macro_dispatch,
            );
            let Some(target) = target else {
                continue;
            };

            consumed_keyboard_input = true;
            self.execute_global_dispatch_target(target);

            ctx.input_mut(|input| {
                if let Some(logical_key) = key_press.logical_key {
                    input.consume_key(key_press.modifiers, logical_key);
                }
                if let Some(physical_key) = key_press.physical_key {
                    input.consume_key(key_press.modifiers, physical_key);
                }
            });
        }

        if consumed_keyboard_input {
            // Remove keyboard/text events so focused text widgets don't also process consumed keys.
            ctx.input_mut(|input| {
                input.raw.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Key { .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                            | egui::Event::Copy
                            | egui::Event::Cut
                    )
                });
            });
        }
    }

    fn collect_pressed_key_events(ctx: &egui::Context) -> Vec<GuiKeyPress> {
        ctx.input(|input| {
            input
                .raw
                .events
                .iter()
                .filter_map(|event| {
                    let egui::Event::Key {
                        key,
                        physical_key,
                        pressed,
                        repeat,
                        modifiers,
                    } = event
                    else {
                        return None;
                    };

                    if !pressed || *repeat {
                        return None;
                    }

                    let key_event = Self::egui_key_to_frontend_event(*key, *modifiers)?;
                    Some(GuiKeyPress {
                        key_event,
                        logical_key: Some(*key),
                        physical_key: *physical_key,
                        modifiers: *modifiers,
                    })
                })
                .collect()
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn collect_numpad_key_events(frame: &eframe::Frame) -> Vec<GuiKeyPress> {
        frame
            .numpad_keys()
            .iter()
            .filter_map(|numpad_key| {
                if !numpad_key.pressed || numpad_key.numlock_on {
                    return None;
                }

                let code = Self::numpad_binding_name_to_frontend_code(numpad_key.keybind_name()?)?;
                let modifiers = numpad_key.modifiers;
                let key_event = crate::frontend::common::KeyEvent::new(
                    code,
                    Self::egui_modifiers_to_frontend(modifiers),
                );

                Some(GuiKeyPress {
                    key_event,
                    logical_key: None,
                    physical_key: None,
                    modifiers,
                })
            })
            .collect()
    }

    #[cfg(target_arch = "wasm32")]
    fn collect_numpad_key_events(_frame: &eframe::Frame) -> Vec<GuiKeyPress> {
        Vec::new()
    }

    fn numpad_binding_name_to_frontend_code(
        binding: &str,
    ) -> Option<crate::frontend::common::KeyCode> {
        let code = match binding {
            "num_0" => crate::frontend::common::KeyCode::Keypad0,
            "num_1" => crate::frontend::common::KeyCode::Keypad1,
            "num_2" => crate::frontend::common::KeyCode::Keypad2,
            "num_3" => crate::frontend::common::KeyCode::Keypad3,
            "num_4" => crate::frontend::common::KeyCode::Keypad4,
            "num_5" => crate::frontend::common::KeyCode::Keypad5,
            "num_6" => crate::frontend::common::KeyCode::Keypad6,
            "num_7" => crate::frontend::common::KeyCode::Keypad7,
            "num_8" => crate::frontend::common::KeyCode::Keypad8,
            "num_9" => crate::frontend::common::KeyCode::Keypad9,
            "num_plus" => crate::frontend::common::KeyCode::KeypadPlus,
            "num_minus" => crate::frontend::common::KeyCode::KeypadMinus,
            "num_multiply" => crate::frontend::common::KeyCode::KeypadMultiply,
            "num_divide" => crate::frontend::common::KeyCode::KeypadDivide,
            "num_enter" => crate::frontend::common::KeyCode::KeypadEnter,
            "num_decimal" => crate::frontend::common::KeyCode::KeypadPeriod,
            _ => return None,
        };
        Some(code)
    }

    fn resolve_global_dispatch_target(
        key_event: crate::frontend::common::KeyEvent,
        keybind_map: &HashMap<crate::frontend::common::KeyEvent, KeyBindAction>,
        app_keybinds: &AppKeybinds,
        suppress_macro_dispatch: bool,
    ) -> Option<GlobalDispatchTarget> {
        if !suppress_macro_dispatch {
            if let Some(KeyBindAction::Macro(_)) = keybind_map.get(&key_event) {
                return keybind_map
                    .get(&key_event)
                    .cloned()
                    .map(GlobalDispatchTarget::Macro);
            }
        }

        Self::app_shortcut_for_key(key_event, app_keybinds).map(GlobalDispatchTarget::Shortcut)
    }

    fn app_shortcut_for_key(
        key_event: crate::frontend::common::KeyEvent,
        app_keybinds: &AppKeybinds,
    ) -> Option<AppShortcut> {
        if Self::binding_matches_key_event(&app_keybinds.quit, key_event) {
            return Some(AppShortcut::Quit);
        }
        if Self::binding_matches_key_event(&app_keybinds.start_search, key_event) {
            return Some(AppShortcut::StartSearch);
        }
        if Self::binding_matches_key_event(&app_keybinds.close_window, key_event) {
            return Some(AppShortcut::CloseWindow);
        }
        None
    }

    fn binding_matches_key_event(
        binding: &str,
        key_event: crate::frontend::common::KeyEvent,
    ) -> bool {
        crate::config::parse_key_string(binding)
            .map(|(code, modifiers)| crate::frontend::common::KeyEvent::new(code, modifiers))
            .is_some_and(|candidate| candidate == key_event)
    }

    fn should_suppress_macro_dispatch(&self) -> bool {
        self.app_core.ui_state.input_mode == InputMode::KeybindForm
    }

    fn execute_global_dispatch_target(&mut self, target: GlobalDispatchTarget) {
        match target {
            GlobalDispatchTarget::Macro(action) => self.execute_macro_keybind(&action),
            GlobalDispatchTarget::Shortcut(shortcut) => self.execute_app_shortcut(shortcut),
        }
    }

    fn execute_macro_keybind(&mut self, action: &KeyBindAction) {
        match self.app_core.execute_keybind_action(action) {
            Ok(commands) => {
                for outbound in commands {
                    if Self::should_send_to_network(&outbound) {
                        self.app_core
                            .perf_stats
                            .record_bytes_sent((outbound.len() + 1) as u64);
                        let _ = self.command_tx.send(outbound);
                    }
                }
            }
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Keybind error: {}", err));
            }
        }

        if !self.app_core.running {
            self.close_requested = true;
        }
    }

    fn execute_app_shortcut(&mut self, shortcut: AppShortcut) {
        match shortcut {
            AppShortcut::Quit => {
                self.app_core.quit();
                self.close_requested = true;
            }
            AppShortcut::StartSearch => {
                self.app_core.start_search_mode();
            }
            AppShortcut::CloseWindow => self.handle_close_window_shortcut(),
        }
    }

    fn handle_close_window_shortcut(&mut self) {
        if self.app_core.ui_state.input_mode == InputMode::Search {
            self.app_core.clear_search_mode();
            return;
        }

        if !matches!(self.app_core.ui_state.input_mode, InputMode::Normal) {
            self.app_core.ui_state.input_mode = InputMode::Normal;
            self.app_core.ui_state.popup_menu = None;
            self.app_core.ui_state.submenu = None;
            self.app_core.ui_state.nested_submenu = None;
            self.app_core.ui_state.deep_submenu = None;
            self.app_core.ui_state.active_dialog = None;
            self.app_core.needs_render = true;
        }
    }

    fn egui_key_to_frontend_event(
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) -> Option<crate::frontend::common::KeyEvent> {
        let code = Self::egui_key_to_frontend_code(key, modifiers)?;
        let modifiers = Self::egui_modifiers_to_frontend(modifiers);
        Some(crate::frontend::common::KeyEvent::new(code, modifiers))
    }

    fn egui_modifiers_to_frontend(
        modifiers: egui::Modifiers,
    ) -> crate::frontend::common::KeyModifiers {
        crate::frontend::common::KeyModifiers {
            ctrl: modifiers.ctrl || modifiers.command,
            shift: modifiers.shift,
            alt: modifiers.alt,
        }
    }

    fn egui_key_to_frontend_code(
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) -> Option<crate::frontend::common::KeyCode> {
        let code = match key {
            egui::Key::ArrowDown => crate::frontend::common::KeyCode::Down,
            egui::Key::ArrowLeft => crate::frontend::common::KeyCode::Left,
            egui::Key::ArrowRight => crate::frontend::common::KeyCode::Right,
            egui::Key::ArrowUp => crate::frontend::common::KeyCode::Up,
            egui::Key::Escape => crate::frontend::common::KeyCode::Esc,
            egui::Key::Tab => {
                if modifiers.shift {
                    crate::frontend::common::KeyCode::BackTab
                } else {
                    crate::frontend::common::KeyCode::Tab
                }
            }
            egui::Key::Backspace => crate::frontend::common::KeyCode::Backspace,
            egui::Key::Enter => crate::frontend::common::KeyCode::Enter,
            egui::Key::Space => crate::frontend::common::KeyCode::Char(' '),
            egui::Key::Insert => crate::frontend::common::KeyCode::Insert,
            egui::Key::Delete => crate::frontend::common::KeyCode::Delete,
            egui::Key::Home => crate::frontend::common::KeyCode::Home,
            egui::Key::End => crate::frontend::common::KeyCode::End,
            egui::Key::PageUp => crate::frontend::common::KeyCode::PageUp,
            egui::Key::PageDown => crate::frontend::common::KeyCode::PageDown,
            egui::Key::Num0 => crate::frontend::common::KeyCode::Char('0'),
            egui::Key::Num1 => crate::frontend::common::KeyCode::Char('1'),
            egui::Key::Num2 => crate::frontend::common::KeyCode::Char('2'),
            egui::Key::Num3 => crate::frontend::common::KeyCode::Char('3'),
            egui::Key::Num4 => crate::frontend::common::KeyCode::Char('4'),
            egui::Key::Num5 => crate::frontend::common::KeyCode::Char('5'),
            egui::Key::Num6 => crate::frontend::common::KeyCode::Char('6'),
            egui::Key::Num7 => crate::frontend::common::KeyCode::Char('7'),
            egui::Key::Num8 => crate::frontend::common::KeyCode::Char('8'),
            egui::Key::Num9 => crate::frontend::common::KeyCode::Char('9'),
            egui::Key::A => crate::frontend::common::KeyCode::Char('a'),
            egui::Key::B => crate::frontend::common::KeyCode::Char('b'),
            egui::Key::C => crate::frontend::common::KeyCode::Char('c'),
            egui::Key::D => crate::frontend::common::KeyCode::Char('d'),
            egui::Key::E => crate::frontend::common::KeyCode::Char('e'),
            egui::Key::F => crate::frontend::common::KeyCode::Char('f'),
            egui::Key::G => crate::frontend::common::KeyCode::Char('g'),
            egui::Key::H => crate::frontend::common::KeyCode::Char('h'),
            egui::Key::I => crate::frontend::common::KeyCode::Char('i'),
            egui::Key::J => crate::frontend::common::KeyCode::Char('j'),
            egui::Key::K => crate::frontend::common::KeyCode::Char('k'),
            egui::Key::L => crate::frontend::common::KeyCode::Char('l'),
            egui::Key::M => crate::frontend::common::KeyCode::Char('m'),
            egui::Key::N => crate::frontend::common::KeyCode::Char('n'),
            egui::Key::O => crate::frontend::common::KeyCode::Char('o'),
            egui::Key::P => crate::frontend::common::KeyCode::Char('p'),
            egui::Key::Q => crate::frontend::common::KeyCode::Char('q'),
            egui::Key::R => crate::frontend::common::KeyCode::Char('r'),
            egui::Key::S => crate::frontend::common::KeyCode::Char('s'),
            egui::Key::T => crate::frontend::common::KeyCode::Char('t'),
            egui::Key::U => crate::frontend::common::KeyCode::Char('u'),
            egui::Key::V => crate::frontend::common::KeyCode::Char('v'),
            egui::Key::W => crate::frontend::common::KeyCode::Char('w'),
            egui::Key::X => crate::frontend::common::KeyCode::Char('x'),
            egui::Key::Y => crate::frontend::common::KeyCode::Char('y'),
            egui::Key::Z => crate::frontend::common::KeyCode::Char('z'),
            egui::Key::F1 => crate::frontend::common::KeyCode::F(1),
            egui::Key::F2 => crate::frontend::common::KeyCode::F(2),
            egui::Key::F3 => crate::frontend::common::KeyCode::F(3),
            egui::Key::F4 => crate::frontend::common::KeyCode::F(4),
            egui::Key::F5 => crate::frontend::common::KeyCode::F(5),
            egui::Key::F6 => crate::frontend::common::KeyCode::F(6),
            egui::Key::F7 => crate::frontend::common::KeyCode::F(7),
            egui::Key::F8 => crate::frontend::common::KeyCode::F(8),
            egui::Key::F9 => crate::frontend::common::KeyCode::F(9),
            egui::Key::F10 => crate::frontend::common::KeyCode::F(10),
            egui::Key::F11 => crate::frontend::common::KeyCode::F(11),
            egui::Key::F12 => crate::frontend::common::KeyCode::F(12),
            _ => return None,
        };
        Some(code)
    }

    fn submit_command(&mut self) {
        let input = std::mem::take(&mut self.command_input);
        let command = input.trim_end().to_string();
        if command.is_empty() {
            return;
        }

        match self.app_core.send_command(command) {
            Ok(outbound) => {
                if Self::should_send_to_network(&outbound) {
                    self.app_core
                        .perf_stats
                        .record_bytes_sent((outbound.len() + 1) as u64);
                    let _ = self.command_tx.send(outbound);
                }
            }
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Command error: {}", err));
            }
        }

        if !self.app_core.running {
            self.close_requested = true;
        }
    }

    fn should_send_to_network(command: &str) -> bool {
        !command.is_empty()
            && !command.starts_with("__")
            && !command.starts_with("action:")
            && !command.starts_with("menu:")
    }

    fn dispatch_raw_command(&mut self, command: String) {
        let outbound = command.trim_end_matches(['\r', '\n']).to_string();
        if outbound.trim().is_empty() {
            return;
        }

        self.app_core
            .perf_stats
            .record_bytes_sent((outbound.len() + 1) as u64);
        let _ = self.command_tx.send(outbound);
    }

    fn resolve_link_dispatch(
        link_data: &LinkData,
        cmdlist: Option<&CmdList>,
    ) -> Option<GuiLinkDispatch> {
        if link_data.exist_id == "_direct_" {
            let command = if !link_data.noun.trim().is_empty() {
                link_data.noun.trim().to_string()
            } else {
                link_data.text.trim().to_string()
            };
            if command.is_empty() {
                None
            } else {
                Some(GuiLinkDispatch::NetworkCommand(command))
            }
        } else if let Some(coord) = link_data.coord.as_deref() {
            if let Some(entry) = cmdlist.and_then(|list| list.get(coord)) {
                Some(GuiLinkDispatch::NetworkCommand(
                    CmdList::substitute_command(
                        &entry.command,
                        &link_data.noun,
                        &link_data.exist_id,
                        None,
                    ),
                ))
            } else if !link_data.exist_id.trim().is_empty() {
                Some(GuiLinkDispatch::MenuRequest {
                    exist_id: link_data.exist_id.clone(),
                    noun: link_data.noun.clone(),
                })
            } else {
                None
            }
        } else {
            Some(GuiLinkDispatch::MenuRequest {
                exist_id: link_data.exist_id.clone(),
                noun: link_data.noun.clone(),
            })
        }
    }

    fn click_pos_to_grid(pos: Pos2) -> (u16, u16) {
        let x = pos.x.clamp(0.0, u16::MAX as f32) as u16;
        let y = pos.y.clamp(0.0, u16::MAX as f32) as u16;
        (x, y)
    }

    fn handle_link_click(&mut self, click: GuiLinkClick) {
        let dispatch =
            Self::resolve_link_dispatch(&click.link_data, self.app_core.cmdlist.as_ref());
        let Some(dispatch) = dispatch else {
            tracing::warn!(
                "Unable to resolve GUI link click for exist_id='{}' noun='{}' coord={:?}",
                click.link_data.exist_id,
                click.link_data.noun,
                click.link_data.coord
            );
            return;
        };

        let outbound = match dispatch {
            GuiLinkDispatch::NetworkCommand(command) => command,
            GuiLinkDispatch::MenuRequest { exist_id, noun } => {
                self.app_core.request_menu(exist_id, noun, click.click_pos)
            }
        };
        self.dispatch_raw_command(outbound);
    }

    fn close_all_popup_menus(&mut self) {
        self.app_core.ui_state.popup_menu = None;
        self.app_core.ui_state.submenu = None;
        self.app_core.ui_state.nested_submenu = None;
        self.app_core.ui_state.deep_submenu = None;
    }

    fn render_menu_layer(
        ctx: &egui::Context,
        layer: GuiMenuLayer,
        menu: &PopupMenu,
    ) -> Option<String> {
        let layer_id = match layer {
            GuiMenuLayer::Main => "gui_popup_menu_main",
            GuiMenuLayer::Submenu => "gui_popup_menu_submenu",
            GuiMenuLayer::Nested => "gui_popup_menu_nested",
            GuiMenuLayer::Deep => "gui_popup_menu_deep",
        };

        let mut clicked_command: Option<String> = None;
        let pos = Pos2::new(menu.position.0 as f32, menu.position.1 as f32);
        egui::Area::new(egui::Id::new(layer_id))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    for item in menu.get_items() {
                        let button = egui::Button::new(item.text.as_str());
                        let response = ui.add_enabled(!item.disabled, button);
                        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            clicked_command = Some(item.command.clone());
                        }
                    }
                });
            });

        clicked_command
    }

    fn handle_popup_menu_command(&mut self, command: String) {
        if let Some(category) = command.strip_prefix("__SUBMENU__") {
            if let Some(items) = self.app_core.menu_categories.get(category).cloned() {
                let parent_pos = self
                    .app_core
                    .ui_state
                    .popup_menu
                    .as_ref()
                    .map(|menu| menu.get_position())
                    .unwrap_or((40, 12));
                self.app_core.ui_state.submenu = Some(PopupMenu::new(
                    items,
                    (parent_pos.0.saturating_add(24), parent_pos.1),
                ));
                self.app_core.ui_state.input_mode = InputMode::Menu;
            } else {
                tracing::warn!("Missing GUI menu category: {}", category);
            }
            return;
        }

        self.dispatch_raw_command(command);
        self.close_all_popup_menus();
        self.app_core.ui_state.input_mode = InputMode::Normal;
    }

    fn render_popup_menus(&mut self, ctx: &egui::Context) {
        let main = self.app_core.ui_state.popup_menu.clone();
        let submenu = self.app_core.ui_state.submenu.clone();
        let nested = self.app_core.ui_state.nested_submenu.clone();
        let deep = self.app_core.ui_state.deep_submenu.clone();

        let mut clicked_command = None;

        if let Some(menu) = &main {
            clicked_command = Self::render_menu_layer(ctx, GuiMenuLayer::Main, menu);
        }
        if clicked_command.is_none() {
            if let Some(menu) = &submenu {
                clicked_command = Self::render_menu_layer(ctx, GuiMenuLayer::Submenu, menu);
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = &nested {
                clicked_command = Self::render_menu_layer(ctx, GuiMenuLayer::Nested, menu);
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = &deep {
                clicked_command = Self::render_menu_layer(ctx, GuiMenuLayer::Deep, menu);
            }
        }

        if let Some(command) = clicked_command {
            self.handle_popup_menu_command(command);
        }
    }

    fn segment_to_rich_text(
        segment: &TextSegment,
        visuals: &egui::Visuals,
        is_link: bool,
    ) -> RichText {
        let foreground = segment
            .fg
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| {
                if is_link {
                    visuals.hyperlink_color
                } else {
                    visuals.text_color()
                }
            });
        let background = segment
            .bg
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(Color32::TRANSPARENT);

        let mut rich = RichText::new(segment.text.as_str())
            .size(DEFAULT_FONT_SIZE + if segment.bold { 0.5 } else { 0.0 })
            .color(foreground)
            .background_color(background);

        if segment.bold {
            rich = rich.strong();
        }
        if segment.mono {
            rich = rich.monospace();
        }
        if is_link {
            rich = rich.underline();
        }
        rich
    }

    fn render_styled_line(
        ui: &mut egui::Ui,
        line: &StyledLine,
        visuals: &egui::Visuals,
    ) -> Option<GuiLinkClick> {
        let mut clicked_link = None;

        ui.scope(|ui| {
            // Each styled segment is rendered as a separate widget. Keep inter-widget spacing at
            // zero so highlights/links don't introduce artificial spaces around punctuation.
            ui.spacing_mut().item_spacing.x = 0.0;

            ui.horizontal_wrapped(|ui| {
                for segment in &line.segments {
                    if segment.text.is_empty() {
                        continue;
                    }

                    let is_link =
                        matches!(segment.span_type, SpanType::Link) && segment.link_data.is_some();
                    let rich = Self::segment_to_rich_text(segment, visuals, is_link);

                    if is_link {
                        let response = ui
                            .add(egui::Label::new(rich).sense(egui::Sense::click()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.clicked() && clicked_link.is_none() {
                            if let Some(link_data) = segment.link_data.clone() {
                                let pointer_pos = response
                                    .interact_pointer_pos()
                                    .or_else(|| ui.ctx().pointer_latest_pos())
                                    .unwrap_or(Pos2::ZERO);
                                clicked_link = Some(GuiLinkClick {
                                    link_data,
                                    click_pos: Self::click_pos_to_grid(pointer_pos),
                                });
                            }
                        }
                    } else {
                        ui.label(rich);
                    }
                }
            });
        });

        clicked_link
    }

    fn render_text_content(
        ui: &mut egui::Ui,
        content: &TextContent,
        scroll_id: &str,
    ) -> Option<GuiLinkClick> {
        let visuals = ui.visuals().clone();
        let mut clicked_link = None;
        let start = content.lines.len().saturating_sub(MAX_RENDERED_LINES);

        egui::ScrollArea::vertical()
            .id_salt(format!("text_scroll_{}", scroll_id))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for line in content.lines.iter().skip(start) {
                    if let Some(link) = Self::render_styled_line(ui, line, &visuals) {
                        clicked_link = Some(link);
                    }
                }
            });
        clicked_link
    }

    fn render_room_description(
        ui: &mut egui::Ui,
        lines: &[StyledLine],
        scroll_id: &str,
    ) -> Option<GuiLinkClick> {
        let visuals = ui.visuals().clone();
        let mut clicked_link = None;

        egui::ScrollArea::vertical()
            .id_salt(format!("room_scroll_{}", scroll_id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for line in lines {
                    if let Some(link) = Self::render_styled_line(ui, line, &visuals) {
                        clicked_link = Some(link);
                    }
                }
            });

        clicked_link
    }

    fn render_window_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        tab: &GuiTab,
    ) -> Option<GuiLinkClick> {
        let Some(window) = app_core.ui_state.windows.get(&tab.window_name) else {
            ui.label("This tab's source window is no longer available.");
            return None;
        };

        match &window.content {
            WindowContent::Text(content)
            | WindowContent::Inventory(content)
            | WindowContent::Spells(content) => {
                Self::render_text_content(ui, content, &tab.window_name)
            }
            WindowContent::TabbedText(tabbed) => {
                if let Some(active) = tabbed.tabs.get(tabbed.active_tab_index) {
                    ui.label(
                        RichText::new(format!("Active tab: {}", active.definition.name)).italics(),
                    );
                    ui.separator();
                    Self::render_text_content(ui, &active.content, &tab.window_name)
                } else {
                    ui.label("No active tab content.");
                    None
                }
            }
            WindowContent::Room(room) => {
                ui.heading(&room.name);
                ui.separator();
                let clicked_link =
                    Self::render_room_description(ui, &room.description, &tab.window_name);
                if !room.exits.is_empty() {
                    ui.separator();
                    ui.label(format!("Exits: {}", room.exits.join(", ")));
                }
                clicked_link
            }
            _ => {
                ui.label("Widget rendering for this tab is scheduled for later GUI milestones.");
                ui.label(format!(
                    "Window: {} ({:?})",
                    window.name, window.widget_type
                ));
                None
            }
        }
    }
}

struct GuiDockTabViewer<'a> {
    app_core: &'a AppCore,
    closed_tabs: Vec<TabKey>,
    link_clicks: Vec<GuiLinkClick>,
}

impl<'a> GuiDockTabViewer<'a> {
    fn new(app_core: &'a AppCore) -> Self {
        Self {
            app_core,
            closed_tabs: Vec::new(),
            link_clicks: Vec::new(),
        }
    }

    fn take_closed_tabs(&mut self) -> Vec<TabKey> {
        std::mem::take(&mut self.closed_tabs)
    }

    fn take_link_clicks(&mut self) -> Vec<GuiLinkClick> {
        std::mem::take(&mut self.link_clicks)
    }
}

impl TabViewer for GuiDockTabViewer<'_> {
    type Tab = GuiTab;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        if let Some(click) = VellumGuiApp::render_window_content(self.app_core, ui, tab) {
            self.link_clicks.push(click);
        }
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.id.title.clone().into()
    }

    fn is_closeable(&self, _tab: &Self::Tab) -> bool {
        true
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> OnCloseResponse {
        self.closed_tabs.push(tab.id.key.clone());
        OnCloseResponse::Close
    }
}

impl eframe::App for VellumGuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.pump_server_messages();
        self.refresh_available_tabs_if_needed();
        let monitor_bounds = Self::monitor_bounds_from_ctx(&ctx);
        self.last_monitor_bounds = Some(monitor_bounds);
        self.apply_pending_detached_viewports(monitor_bounds);

        if self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.handle_global_input(&ctx, frame);

        if self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let mut restore_key: Option<TabKey> = None;

        egui::TopBottomPanel::top("gui_header").show(&ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("VellumFE GUI");
                let connection_text = if self.app_core.game_state.connected {
                    RichText::new("Connected").color(Color32::from_rgb(0x3a, 0xc5, 0x6d))
                } else {
                    RichText::new("Disconnected").color(Color32::from_rgb(0xd9, 0x55, 0x55))
                };
                ui.separator();
                ui.label(connection_text);
                ui.separator();

                ui.menu_button("Hidden Tabs", |ui| {
                    let hidden = self.hidden_tabs_for_menu();
                    if hidden.is_empty() {
                        ui.label("No hidden tabs");
                    } else {
                        for (key, title) in hidden {
                            if ui.button(title).clicked() {
                                restore_key = Some(key);
                                ui.close_menu();
                            }
                        }
                    }
                });
            });
        });

        if let Some(key) = restore_key {
            self.restore_tab(key);
        }

        let detached_before_frame = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .unwrap_or_default();
        let mut closed_tabs = Vec::new();
        let mut link_clicks = Vec::new();
        egui::CentralPanel::default().show(&ctx, |ui| {
            if let Some(dock_state) = &mut self.dock_state {
                let mut viewer = GuiDockTabViewer::new(&self.app_core);
                DockArea::new(dock_state).show_inside(ui, &mut viewer);
                link_clicks = viewer.take_link_clicks();
                closed_tabs = viewer.take_closed_tabs();
            } else {
                ui.heading("No visible tabs");
                ui.label("Use Hidden Tabs to restore one or more tabs.");
            }
        });

        for key in closed_tabs {
            self.hide_tab(key);
        }
        self.hide_removed_detached_tabs(&detached_before_frame);
        for click in link_clicks {
            self.handle_link_click(click);
        }
        self.render_popup_menus(&ctx);

        egui::TopBottomPanel::bottom("gui_command_input").show(&ctx, |ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.command_input)
                    .hint_text("Enter command...")
                    .desired_width(f32::INFINITY),
            );

            let pressed_enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if response.lost_focus() && pressed_enter {
                self.submit_command();
                response.request_focus();
            }
        });
        if ctx.input(|i| i.pointer.any_released()) {
            self.layout_dirty = true;
        }

        if self.layout_dirty {
            self.save_layout_state();
            self.layout_dirty = false;
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl Drop for VellumGuiApp {
    fn drop(&mut self) {
        if let Some(handle) = self.network_handle.take() {
            handle.abort();
        }

        self.save_layout_state();
    }
}

pub fn run_native_gui(app_core: AppCore, login_key: Option<String>) -> Result<()> {
    let viewport = ViewportBuilder::default().with_inner_size([1200.0, 800.0]);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let app = VellumGuiApp::new(
        app_core,
        login_key,
        INITIAL_LAYOUT_WIDTH as f32,
        INITIAL_LAYOUT_HEIGHT as f32,
    )?;

    eframe::run_native(
        "VellumFE GUI",
        options,
        Box::new(move |_cc| Ok(Box::new(app))),
    )
    .map_err(|err| anyhow!("Failed to run GUI frontend: {}", err))
}

fn parse_hex_color(input: &str) -> Option<Color32> {
    let hex = input.strip_prefix('#').unwrap_or(input);
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::{
        parse_hex_color, AppShortcut, DockStateSnapshot, GlobalDispatchTarget, GuiLinkDispatch,
        GuiTab, VellumGuiApp,
    };
    use crate::config::{AppKeybinds, Config, KeyBindAction, MacroAction};
    use crate::data::LinkData;
    use crate::frontend::common::{KeyCode, KeyEvent, KeyModifiers};
    use crate::frontend::gui::{GuiLayoutFileV1, TabId, TabKey, ViewportState};
    use eframe::egui::{Color32, Pos2, Vec2};
    use egui_dock::DockState;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn test_parse_hex_color_with_hash() {
        assert_eq!(
            parse_hex_color("#FF00AA"),
            Some(Color32::from_rgb(255, 0, 170))
        );
    }

    #[test]
    fn test_parse_hex_color_without_hash() {
        assert_eq!(
            parse_hex_color("00FF00"),
            Some(Color32::from_rgb(0, 255, 0))
        );
    }

    #[test]
    fn test_parse_hex_color_invalid_input() {
        assert_eq!(parse_hex_color("#XYZ"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn test_resolve_layout_ids_prefers_connection_character() {
        let mut config = Config::default();
        config.character = Some("profile_a".to_string());
        config.connection.character = Some("Nisugi".to_string());

        let (profile, character) = VellumGuiApp::resolve_layout_ids(&config);
        assert_eq!(profile, "profile_a");
        assert_eq!(character, "Nisugi");
    }

    #[test]
    fn test_dock_state_snapshot_round_trip() {
        let snapshot = DockStateSnapshot {
            visible_tabs: vec![TabKey::TextMain, TabKey::Vitals],
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: DockStateSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.visible_tabs.len(), 2);
        assert_eq!(parsed.visible_tabs[0], TabKey::TextMain);
        assert_eq!(parsed.visible_tabs[1], TabKey::Vitals);
    }

    #[test]
    fn test_detached_viewports_from_layout_filters_invalid_entries() {
        let mut available_tabs = HashMap::new();
        available_tabs.insert(
            TabKey::Vitals,
            GuiTab {
                id: TabId::new(TabKey::Vitals),
                window_name: "vitals".to_string(),
            },
        );
        available_tabs.insert(
            TabKey::Room,
            GuiTab {
                id: TabId::new(TabKey::Room),
                window_name: "room".to_string(),
            },
        );

        let mut layout = GuiLayoutFileV1::new("profile", "character");
        layout.detached_viewports.insert(
            "b_vitals".to_string(),
            ViewportState::new(TabKey::Vitals, [100.0, 100.0], [400.0, 300.0]),
        );
        layout.detached_viewports.insert(
            "a_vitals".to_string(),
            ViewportState::new(TabKey::Vitals, [200.0, 200.0], [500.0, 400.0]),
        );
        layout.detached_viewports.insert(
            "room_hidden".to_string(),
            ViewportState::new(TabKey::Room, [100.0, 100.0], [400.0, 300.0]),
        );
        layout.detached_viewports.insert(
            "missing_tab".to_string(),
            ViewportState::new(TabKey::Compass, [100.0, 100.0], [400.0, 300.0]),
        );

        let hidden_tabs = HashSet::from([TabKey::Room]);
        let detached =
            VellumGuiApp::detached_viewports_from_layout(&layout, &available_tabs, &hidden_tabs);

        assert_eq!(detached.len(), 1);
        assert_eq!(detached[0].tab, TabKey::Vitals);
        assert_eq!(detached[0].outer_pos_px, [200.0, 200.0]);
    }

    #[test]
    fn test_sanitize_viewport_state_clamps_and_enforces_min_size() {
        let viewport = ViewportState::new(TabKey::Vitals, [-500.0, -500.0], [20.0, 30.0]);
        let sanitized =
            VellumGuiApp::sanitize_viewport_state(&viewport, Some([0.0, 0.0, 1920.0, 1080.0]));

        assert!(sanitized.outer_size_px[0] >= super::MIN_VIEWPORT_WIDTH);
        assert!(sanitized.outer_size_px[1] >= super::MIN_VIEWPORT_HEIGHT);

        let min_x = 0.0 - sanitized.outer_size_px[0] + super::MIN_VISIBLE_VIEWPORT_PX;
        let max_x = 1920.0 - super::MIN_VISIBLE_VIEWPORT_PX;
        let min_y = 0.0 - sanitized.outer_size_px[1] + super::MIN_VISIBLE_VIEWPORT_PX;
        let max_y = 1080.0 - super::MIN_VISIBLE_VIEWPORT_PX;
        assert!(sanitized.outer_pos_px[0] >= min_x - 0.01);
        assert!(sanitized.outer_pos_px[0] <= max_x + 0.01);
        assert!(sanitized.outer_pos_px[1] >= min_y - 0.01);
        assert!(sanitized.outer_pos_px[1] <= max_y + 0.01);
    }

    #[test]
    fn test_collect_detached_viewports_for_save_includes_window_tabs() {
        let mut dock_state = DockState::new(vec![GuiTab {
            id: TabId::new(TabKey::TextMain),
            window_name: "main".to_string(),
        }]);
        let detached_surface = dock_state.add_window(vec![GuiTab {
            id: TabId::new(TabKey::Vitals),
            window_name: "vitals".to_string(),
        }]);
        dock_state
            .get_window_state_mut(detached_surface)
            .expect("detached surface should have a window state")
            .set_position(Pos2::new(250.0, 120.0))
            .set_size(Vec2::new(640.0, 480.0));

        let detached = VellumGuiApp::collect_detached_viewports_for_save(&mut dock_state, None);
        assert_eq!(detached.len(), 1);
        let saved = detached.values().next().expect("detached viewport entry");
        assert_eq!(saved.tab, TabKey::Vitals);
        // `egui_dock::WindowState` reports `Rect::NOTHING` until first rendered frame,
        // so collection falls back to a safe default rectangle in headless unit tests.
        assert_eq!(saved.outer_pos_px, [100.0, 100.0]);
        assert_eq!(saved.outer_size_px, [600.0, 400.0]);
    }

    #[test]
    fn test_global_dispatch_prefers_macro_over_shortcut() {
        let key_event = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CTRL);
        let mut keybind_map = HashMap::new();
        keybind_map.insert(
            key_event,
            KeyBindAction::Macro(MacroAction {
                macro_text: "look\r".to_string(),
            }),
        );

        let target = VellumGuiApp::resolve_global_dispatch_target(
            key_event,
            &keybind_map,
            &AppKeybinds::default(),
            false,
        );
        assert!(matches!(target, Some(GlobalDispatchTarget::Macro(_))));
    }

    #[test]
    fn test_global_dispatch_uses_shortcut_when_macro_capture_active() {
        let key_event = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CTRL);
        let mut keybind_map = HashMap::new();
        keybind_map.insert(
            key_event,
            KeyBindAction::Macro(MacroAction {
                macro_text: "look\r".to_string(),
            }),
        );

        let target = VellumGuiApp::resolve_global_dispatch_target(
            key_event,
            &keybind_map,
            &AppKeybinds::default(),
            true,
        );
        assert!(matches!(
            target,
            Some(GlobalDispatchTarget::Shortcut(AppShortcut::StartSearch))
        ));
    }

    #[test]
    fn test_global_dispatch_suppresses_macro_without_shortcut() {
        let key_event = KeyEvent::new(KeyCode::Keypad1, KeyModifiers::NONE);
        let mut keybind_map = HashMap::new();
        keybind_map.insert(
            key_event,
            KeyBindAction::Macro(MacroAction {
                macro_text: "sw\r".to_string(),
            }),
        );

        let target = VellumGuiApp::resolve_global_dispatch_target(
            key_event,
            &keybind_map,
            &AppKeybinds::default(),
            true,
        );
        assert!(target.is_none());
    }

    #[test]
    fn test_egui_num_key_maps_to_keypad_event() {
        let event = VellumGuiApp::egui_key_to_frontend_event(
            eframe::egui::Key::Num1,
            eframe::egui::Modifiers::default(),
        )
        .expect("Num1 should map to a frontend key event");
        assert_eq!(event.code, KeyCode::Char('1'));
        assert_eq!(event.modifiers, KeyModifiers::NONE);
    }

    #[test]
    fn test_numpad_binding_name_maps_to_keypad_codes() {
        assert_eq!(
            VellumGuiApp::numpad_binding_name_to_frontend_code("num_1"),
            Some(KeyCode::Keypad1)
        );
        assert_eq!(
            VellumGuiApp::numpad_binding_name_to_frontend_code("num_plus"),
            Some(KeyCode::KeypadPlus)
        );
        assert_eq!(
            VellumGuiApp::numpad_binding_name_to_frontend_code("num_decimal"),
            Some(KeyCode::KeypadPeriod)
        );
        assert_eq!(
            VellumGuiApp::numpad_binding_name_to_frontend_code("unknown"),
            None
        );
    }

    #[test]
    fn test_resolve_link_dispatch_direct_cmd_prefers_noun() {
        let link = LinkData {
            exist_id: "_direct_".to_string(),
            noun: "get coin".to_string(),
            text: "GET COIN".to_string(),
            coord: None,
        };

        let dispatch = VellumGuiApp::resolve_link_dispatch(&link, None);
        assert_eq!(
            dispatch,
            Some(GuiLinkDispatch::NetworkCommand("get coin".to_string()))
        );
    }

    #[test]
    fn test_resolve_link_dispatch_direct_cmd_falls_back_to_text() {
        let link = LinkData {
            exist_id: "_direct_".to_string(),
            noun: String::new(),
            text: "SKILLS BASE".to_string(),
            coord: None,
        };

        let dispatch = VellumGuiApp::resolve_link_dispatch(&link, None);
        assert_eq!(
            dispatch,
            Some(GuiLinkDispatch::NetworkCommand("SKILLS BASE".to_string()))
        );
    }

    #[test]
    fn test_resolve_link_dispatch_menu_request_for_regular_link() {
        let link = LinkData {
            exist_id: "12345".to_string(),
            noun: "sword".to_string(),
            text: "a rusty sword".to_string(),
            coord: None,
        };

        let dispatch = VellumGuiApp::resolve_link_dispatch(&link, None);
        assert_eq!(
            dispatch,
            Some(GuiLinkDispatch::MenuRequest {
                exist_id: "12345".to_string(),
                noun: "sword".to_string(),
            })
        );
    }

    #[test]
    fn test_resolve_link_dispatch_coord_without_cmdlist_falls_back_to_menu() {
        let link = LinkData {
            exist_id: "12345".to_string(),
            noun: "sword".to_string(),
            text: "a rusty sword".to_string(),
            coord: Some("2524,2061".to_string()),
        };

        let dispatch = VellumGuiApp::resolve_link_dispatch(&link, None);
        assert_eq!(
            dispatch,
            Some(GuiLinkDispatch::MenuRequest {
                exist_id: "12345".to_string(),
                noun: "sword".to_string(),
            })
        );
    }

    #[test]
    fn test_click_pos_to_grid_clamps_values() {
        let pos = Pos2::new(-10.0, 999999.0);
        let (x, y) = VellumGuiApp::click_pos_to_grid(pos);
        assert_eq!(x, 0);
        assert_eq!(y, u16::MAX);
    }
}
