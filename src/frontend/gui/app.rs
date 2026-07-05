use super::persistence::{load_layout, save_layout, FontRef, GuiLayoutFileV1, ViewportState};
use super::{TabId, TabKey};
use crate::cmdlist::CmdList;
use crate::config::{AppKeybinds, Config, KeyBindAction, TargetListConfig};
use crate::core::AppCore;
use crate::data::{
    InputMode, LinkData, PopupMenu, StyledLine, TabbedTextContent, TextContent, TextSegment,
    WidgetType, WindowContent, WindowState,
};
use crate::network::{LichConnection, RawLogger, ServerMessage};
use anyhow::{anyhow, Context, Result};
use eframe::egui;
use eframe::egui::{Color32, Pos2, Rect, RichText, Vec2, ViewportBuilder};
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{DockArea, DockState, Surface, SurfaceIndex, TabViewer};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

mod editors;
mod theme;
mod widgets;

const INITIAL_LAYOUT_WIDTH: u16 = 160;
const INITIAL_LAYOUT_HEIGHT: u16 = 50;
const DEFAULT_FONT_SIZE: f32 = 14.0;
const MAX_RENDERED_LINES: usize = 2000;
const MIN_VISIBLE_VIEWPORT_PX: f32 = 48.0;
const MIN_VIEWPORT_WIDTH: f32 = 180.0;
const MIN_VIEWPORT_HEIGHT: f32 = 120.0;
const MIN_DOCKED_WINDOW_HEIGHT: f32 = 24.0;
/// Idle delay before a dirty layout is flushed to disk. Saves are blocking
/// on the UI thread, so writes must not happen per interaction.
const LAYOUT_SAVE_DEBOUNCE: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
struct GuiTab {
    id: TabId,
    window_name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct DockStateSnapshot {
    visible_tabs: Vec<TabKey>,
    #[serde(default)]
    main_window_rects: Vec<MainWindowRectSnapshot>,
    #[serde(default)]
    tab_zones: Vec<TabZoneSnapshot>,
    #[serde(default)]
    no_title_tabs: Vec<TabKey>,
    #[serde(default)]
    shell_layout: ShellLayoutSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MainWindowRectSnapshot {
    key: TabKey,
    /// [x, y, width, height] in points
    rect: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GuiShellZone {
    Header,
    Footer,
    LeftSidebar,
    Center,
    RightSidebar,
}

impl GuiShellZone {
    fn label(self) -> &'static str {
        match self {
            GuiShellZone::Header => "Header",
            GuiShellZone::Footer => "Footer",
            GuiShellZone::LeftSidebar => "Left Bar",
            GuiShellZone::Center => "Center",
            GuiShellZone::RightSidebar => "Right Bar",
        }
    }

    fn id_fragment(self) -> &'static str {
        match self {
            GuiShellZone::Header => "header",
            GuiShellZone::Footer => "footer",
            GuiShellZone::LeftSidebar => "left",
            GuiShellZone::Center => "center",
            GuiShellZone::RightSidebar => "right",
        }
    }

    fn all() -> [GuiShellZone; 5] {
        [
            GuiShellZone::Header,
            GuiShellZone::Footer,
            GuiShellZone::LeftSidebar,
            GuiShellZone::Center,
            GuiShellZone::RightSidebar,
        ]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TabZoneSnapshot {
    key: TabKey,
    zone: GuiShellZone,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct ShellLayoutSnapshot {
    header_height: f32,
    footer_height: f32,
    left_sidebar_width: f32,
    right_sidebar_width: f32,
    #[serde(default = "serde_default_true")]
    header_visible: bool,
    #[serde(default = "serde_default_true")]
    footer_visible: bool,
    left_sidebar_collapsed: bool,
    right_sidebar_collapsed: bool,
}

const fn serde_default_true() -> bool {
    true
}

impl Default for ShellLayoutSnapshot {
    fn default() -> Self {
        Self {
            header_height: 140.0,
            footer_height: 180.0,
            left_sidebar_width: 300.0,
            right_sidebar_width: 300.0,
            // Default to a center-only shell; users can enable regions from the toolbar.
            header_visible: false,
            footer_visible: false,
            left_sidebar_collapsed: true,
            right_sidebar_collapsed: true,
        }
    }
}

impl ShellLayoutSnapshot {
    fn sanitize(&mut self, center_width: f32) {
        self.header_height = self.header_height.clamp(96.0, 360.0);
        self.footer_height = self.footer_height.clamp(96.0, 420.0);
        self.left_sidebar_width = self.left_sidebar_width.clamp(220.0, 700.0);
        self.right_sidebar_width = self.right_sidebar_width.clamp(220.0, 700.0);

        let max_sidebar_width = ((center_width - 220.0).max(220.0) * 0.45).max(220.0);
        self.left_sidebar_width = self.left_sidebar_width.min(max_sidebar_width);
        self.right_sidebar_width = self.right_sidebar_width.min(max_sidebar_width);
    }
}

/// Per-frame interactions collected while rendering zone surfaces.
/// Window management commands (move/hide/detach/etc.) do not flow through
/// here; they are applied via `apply_window_menu_command`.
#[derive(Default)]
struct GuiWindowActions {
    link_clicks: Vec<GuiLinkClick>,
    window_menu_request: Option<GuiWindowMenuRequest>,
}

impl GuiWindowActions {
    fn merge(&mut self, other: GuiWindowActions) {
        self.link_clicks.extend(other.link_clicks);
        if let Some(request) = other.window_menu_request {
            self.window_menu_request = Some(request);
        }
    }
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
    key_event: crate::data::input::KeyEvent,
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
struct GuiMenuCommand {
    layer: GuiMenuLayer,
    command: String,
}

#[derive(Clone, Debug)]
struct GuiLinkClick {
    link_data: LinkData,
    click_pos: (u16, u16),
}

#[derive(Clone, Debug)]
struct GuiWindowMenuRequest {
    tab_key: TabKey,
    zone: GuiShellZone,
    allow_reorder: bool,
    title_bar_hidden: bool,
    position: Pos2,
}

#[derive(Clone, Copy, Debug)]
enum GuiWindowMenuCommand {
    Hide,
    Eject,
    ToggleTitleBar,
    MoveUp,
    MoveDown,
    MoveTo(GuiShellZone),
}

#[derive(Clone, Debug)]
struct GuiZoneDragState {
    tab_key: TabKey,
    from_zone: GuiShellZone,
    pointer_pos: Pos2,
}

#[derive(Clone, Debug)]
struct GuiZoneWindowRect {
    zone: GuiShellZone,
    tab_key: TabKey,
    rect: Rect,
}

#[derive(Clone, Debug)]
struct GuiZoneDropResult {
    tab_key: TabKey,
    target_zone: GuiShellZone,
    insert_before: Option<TabKey>,
}

pub struct VellumGuiApp {
    app_core: AppCore,
    _runtime: tokio::runtime::Runtime,
    command_tx: mpsc::UnboundedSender<String>,
    server_rx: mpsc::Receiver<ServerMessage>,
    network_handle: Option<tokio::task::JoinHandle<()>>,
    command_input: String,
    close_requested: bool,
    dock_state: Option<DockState<GuiTab>>,
    available_tabs: HashMap<TabKey, GuiTab>,
    hidden_tabs: HashSet<TabKey>,
    main_window_rects: HashMap<TabKey, [f32; 4]>,
    last_center_window_rects: HashMap<TabKey, [f32; 4]>,
    tab_zones: HashMap<TabKey, GuiShellZone>,
    no_title_tabs: HashSet<TabKey>,
    shell_layout: ShellLayoutSnapshot,
    layout_profile: String,
    layout_character: String,
    layout_dirty: bool,
    layout_dirty_since: Option<Instant>,
    applied_theme_id: Option<String>,
    current_theme: crate::theme::AppTheme,
    ui_font: FontRef,
    fonts_applied: bool,
    settings_editor: Option<editors::SettingsEditorState>,
    highlight_editor: Option<editors::HighlightEditorState>,
    window_context_menu: Option<GuiWindowMenuRequest>,
    zone_drag_state: Option<GuiZoneDragState>,
    hand_resize_tab: Option<TabKey>,
    pending_detached_viewports: Vec<ViewportState>,
    last_monitor_bounds: Option<[f32; 4]>,
}

impl VellumGuiApp {
    pub fn new(
        mut app_core: AppCore,
        direct: Option<crate::network::DirectConnectConfig>,
        login_key: Option<String>,
        initial_width: f32,
        initial_height: f32,
    ) -> Result<Self> {
        app_core.init_windows(
            initial_width.max(1.0) as u16,
            initial_height.max(1.0) as u16,
        );

        let runtime = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let (server_tx, server_rx) =
            mpsc::channel::<ServerMessage>(crate::network::SERVER_CHANNEL_CAPACITY);
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

        let network_handle = match direct {
            Some(cfg) => runtime.spawn(async move {
                if let Err(err) =
                    crate::network::DirectConnection::start(cfg, server_tx, command_rx, raw_logger)
                        .await
                {
                    tracing::error!("GUI network connection error: {}", err);
                }
            }),
            None => runtime.spawn(async move {
                if let Err(err) =
                    LichConnection::start(&host, port, login_key, server_tx, command_rx, raw_logger)
                        .await
                {
                    tracing::error!("GUI network connection error: {}", err);
                }
            }),
        };

        let (layout_profile, layout_character) = Self::resolve_layout_ids(&app_core.config);
        let persisted_layout = load_layout(&layout_profile, &layout_character).ok();
        let ui_font = persisted_layout
            .as_ref()
            .map(|layout| layout.ui_font.clone())
            .unwrap_or_default();

        let available_tabs = Self::collect_available_tabs(&app_core);
        let mut hidden_tabs: HashSet<TabKey> = persisted_layout
            .as_ref()
            .map(|layout| layout.hidden_tabs.iter().cloned().collect())
            .unwrap_or_default();
        hidden_tabs.retain(|key| available_tabs.contains_key(key));
        let snapshot = persisted_layout
            .as_ref()
            .and_then(|layout| Self::dock_snapshot_from_layout(layout));
        let mut main_window_rects = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .main_window_rects
                    .iter()
                    .filter(|entry| available_tabs.contains_key(&entry.key))
                    .map(|entry| (entry.key.clone(), entry.rect))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        main_window_rects.retain(|key, _| available_tabs.contains_key(key));
        let mut tab_zones = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .tab_zones
                    .iter()
                    .filter(|entry| available_tabs.contains_key(&entry.key))
                    .map(|entry| (entry.key.clone(), entry.zone))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        tab_zones.retain(|key, _| available_tabs.contains_key(key));
        let mut no_title_tabs: HashSet<TabKey> = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .no_title_tabs
                    .iter()
                    .filter(|key| available_tabs.contains_key(*key))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        no_title_tabs.retain(|key| available_tabs.contains_key(key));
        for key in available_tabs.keys() {
            tab_zones
                .entry(key.clone())
                .or_insert_with(|| Self::default_zone_for_tab_key(key));
        }
        let mut shell_layout = snapshot
            .as_ref()
            .map(|snapshot| snapshot.shell_layout.clone())
            .unwrap_or_default();
        shell_layout.sanitize(initial_width.max(1.0));

        let detached_viewports = persisted_layout
            .as_ref()
            .map(|layout| {
                Self::detached_viewports_from_layout(layout, &available_tabs, &hidden_tabs)
            })
            .unwrap_or_default();
        let mut dock_state = if detached_viewports.is_empty() {
            None
        } else {
            Some(DockState::new(Vec::new()))
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
            main_window_rects,
            last_center_window_rects: HashMap::new(),
            tab_zones,
            no_title_tabs,
            shell_layout,
            layout_profile,
            layout_character,
            layout_dirty: false,
            layout_dirty_since: None,
            applied_theme_id: None,
            current_theme: crate::theme::AppTheme::default(),
            ui_font,
            fonts_applied: false,
            settings_editor: None,
            highlight_editor: None,
            window_context_menu: None,
            zone_drag_state: None,
            hand_resize_tab: None,
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

    fn default_zone_for_tab_key(tab_key: &TabKey) -> GuiShellZone {
        match tab_key {
            TabKey::LeftHand | TabKey::RightHand | TabKey::SpellHand => GuiShellZone::Header,
            TabKey::Compass
            | TabKey::Quickbar { .. }
            | TabKey::Indicators
            | TabKey::Vitals
            | TabKey::Countdown
            | TabKey::Dashboard
            | TabKey::Encumbrance
            | TabKey::Experience
            | TabKey::Perception
            | TabKey::InjuryDoll => GuiShellZone::Footer,
            _ => GuiShellZone::Center,
        }
    }

    fn zone_for_tab(&self, key: &TabKey) -> GuiShellZone {
        self.tab_zones
            .get(key)
            .copied()
            .unwrap_or_else(|| Self::default_zone_for_tab_key(key))
    }

    fn target_docked_height(&self, zone: GuiShellZone) -> Option<f32> {
        match zone {
            GuiShellZone::Header => Some(
                (self.shell_layout.header_height - 12.0).max(MIN_DOCKED_WINDOW_HEIGHT),
            ),
            GuiShellZone::Footer => Some(
                (self.shell_layout.footer_height - 12.0).max(MIN_DOCKED_WINDOW_HEIGHT),
            ),
            _ => None,
        }
    }

    fn is_compact_center_widget(widget_type: &WidgetType) -> bool {
        matches!(
            widget_type,
            WidgetType::Hand
                | WidgetType::MiniVitals
                | WidgetType::Progress
                | WidgetType::Compass
                | WidgetType::Indicator
                | WidgetType::Countdown
        )
    }

    fn min_window_height_for_zone(zone: GuiShellZone, window: &WindowState) -> f32 {
        if matches!(zone, GuiShellZone::Header | GuiShellZone::Footer) {
            MIN_DOCKED_WINDOW_HEIGHT
        } else if zone == GuiShellZone::Center && Self::is_compact_center_widget(&window.widget_type)
        {
            MIN_DOCKED_WINDOW_HEIGHT
        } else {
            90.0
        }
    }

    fn set_tab_zone(&mut self, key: TabKey, zone: GuiShellZone) {
        let current = self.zone_for_tab(&key);
        if current != zone {
            self.tab_zones.insert(key.clone(), zone);
            if let Some(target_height) = self.target_docked_height(zone) {
                let entry = self
                    .main_window_rects
                    .entry(key.clone())
                    .or_insert([16.0, 16.0, 240.0, target_height]);
                entry[3] = target_height;
            }
            if matches!(zone, GuiShellZone::LeftSidebar | GuiShellZone::RightSidebar) {
                let entry = self
                    .main_window_rects
                    .entry(key.clone())
                    .or_insert([16.0, 16.0, 240.0, 240.0]);
                entry[3] = entry[3].clamp(120.0, 420.0);
            }
            self.layout_dirty = true;
        }
    }

    fn apply_zone_drop(&mut self, drop_result: GuiZoneDropResult) {
        let GuiZoneDropResult {
            tab_key,
            target_zone,
            insert_before,
        } = drop_result;

        self.set_tab_zone(tab_key.clone(), target_zone);
        if matches!(target_zone, GuiShellZone::Center) {
            // Restore last center geometry if available so moves out/in of header/footer
            // do not inherit docked coordinates.
            if let Some(snapshot) = self.last_center_window_rects.get(&tab_key).copied() {
                self.main_window_rects.insert(tab_key, snapshot);
            } else {
                // Never rendered in center this session: the stored rect holds
                // synthetic docked coordinates. Drop it so the center renderer
                // assigns its default fallback rect instead.
                self.main_window_rects.remove(&tab_key);
            }
            self.layout_dirty = true;
            // Center windows are freely positioned/resized; do not normalize their order
            // into synthetic y offsets or they will collapse toward the top-left.
            return;
        }

        let detached_tabs = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .unwrap_or_default();
        let mut ordered: Vec<TabKey> = self
            .zone_surface_tabs(&detached_tabs, target_zone)
            .into_iter()
            .map(|tab| tab.id.key)
            .collect();
        let Some(existing_idx) = ordered.iter().position(|candidate| candidate == &tab_key) else {
            return;
        };
        ordered.remove(existing_idx);
        let insert_idx = insert_before
            .as_ref()
            .and_then(|before_key| ordered.iter().position(|candidate| candidate == before_key))
            .unwrap_or(ordered.len());
        ordered.insert(insert_idx, tab_key);
        self.persist_zone_order(&ordered);
    }

    fn title_bar_hidden(&self, key: &TabKey) -> bool {
        self.no_title_tabs.contains(key)
    }

    fn toggle_title_bar(&mut self, key: TabKey) {
        if self.no_title_tabs.contains(&key) {
            self.no_title_tabs.remove(&key);
        } else {
            self.no_title_tabs.insert(key);
        }
        self.layout_dirty = true;
    }

    fn persist_zone_order(&mut self, ordered: &[TabKey]) {
        let mut y = 16.0f32;
        for key in ordered {
            let rect = self
                .main_window_rects
                .entry(key.clone())
                .or_insert([16.0, y, 220.0, 140.0]);
            rect[1] = y;
            y += 10.0;
        }
        self.layout_dirty = true;
    }

    fn move_tab_within_zone(&mut self, key: &TabKey, zone: GuiShellZone, move_up: bool) {
        let detached_tabs = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .unwrap_or_default();
        let mut ordered: Vec<TabKey> = self
            .zone_surface_tabs(&detached_tabs, zone)
            .into_iter()
            .map(|tab| tab.id.key)
            .collect();
        let Some(current_idx) = ordered.iter().position(|candidate| candidate == key) else {
            return;
        };
        let target_idx = if move_up {
            current_idx.checked_sub(1)
        } else if current_idx + 1 < ordered.len() {
            Some(current_idx + 1)
        } else {
            None
        };
        if let Some(target_idx) = target_idx {
            ordered.swap(current_idx, target_idx);
            self.persist_zone_order(&ordered);
        }
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

    fn rect_to_snapshot(rect: Rect) -> [f32; 4] {
        [rect.min.x, rect.min.y, rect.width(), rect.height()]
    }

    fn rect_from_snapshot(raw: [f32; 4]) -> Option<Rect> {
        if !raw.iter().all(|value| value.is_finite()) {
            return None;
        }
        let width = raw[2].max(120.0);
        let height = raw[3].max(MIN_DOCKED_WINDOW_HEIGHT);
        Some(Rect::from_min_size(
            Pos2::new(raw[0], raw[1]),
            Vec2::new(width, height),
        ))
    }

    fn clamp_main_window_rect(rect: Rect, bounds: Rect) -> Rect {
        if !rect.is_finite() || !bounds.is_finite() {
            return rect;
        }

        let bounds_w = bounds.width().max(1.0);
        let bounds_h = bounds.height().max(1.0);
        let min_w = 120.0_f32.min(bounds_w);
        let min_h = MIN_DOCKED_WINDOW_HEIGHT.min(bounds_h);
        let width = rect.width().clamp(min_w, bounds_w);
        let height = rect.height().clamp(min_h, bounds_h);
        let min_x = bounds.left();
        let max_x = bounds.right() - width;
        let min_y = bounds.top();
        let max_y = bounds.bottom() - height;
        let x = rect.min.x.clamp(min_x, max_x);
        let y = rect.min.y.clamp(min_y, max_y);
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height))
    }

    fn track_main_window_rect(&mut self, key: &TabKey, rect: Rect, bounds: Rect) {
        if !rect.is_finite() || !bounds.is_finite() {
            return;
        }
        let clamped = Self::clamp_main_window_rect(rect, bounds);
        if !clamped.is_finite() {
            return;
        }
        let snapshot = Self::rect_to_snapshot(clamped);
        let changed = self
            .main_window_rects
            .get(key)
            .map(|existing| {
                let dx = (existing[0] - snapshot[0]).abs();
                let dy = (existing[1] - snapshot[1]).abs();
                let dw = (existing[2] - snapshot[2]).abs();
                let dh = (existing[3] - snapshot[3]).abs();
                dx > 0.5 || dy > 0.5 || dw > 0.5 || dh > 0.5
            })
            .unwrap_or(true);
        if changed {
            self.main_window_rects.insert(key.clone(), snapshot);
            self.layout_dirty = true;
        }
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
        let detached_tabs = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .unwrap_or_default();

        if let Some(dock_state) = &self.dock_state {
            let mut visible = Vec::new();
            let mut seen = HashSet::new();
            for ((surface, _), tab) in dock_state.iter_all_tabs() {
                if surface.is_main()
                    && !detached_tabs.contains(&tab.id.key)
                    && seen.insert(tab.id.key.clone())
                {
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
                if self.hidden_tabs.contains(key) || detached_tabs.contains(key) {
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
        if !viewport.outer_pos_px[0].is_finite() {
            viewport.outer_pos_px[0] = 120.0;
        }
        if !viewport.outer_pos_px[1].is_finite() {
            viewport.outer_pos_px[1] = 120.0;
        }
        if !viewport.outer_size_px[0].is_finite() {
            viewport.outer_size_px[0] = 640.0;
        }
        if !viewport.outer_size_px[1].is_finite() {
            viewport.outer_size_px[1] = 480.0;
        }

        viewport.outer_size_px[0] = viewport.outer_size_px[0].max(MIN_VIEWPORT_WIDTH);
        viewport.outer_size_px[1] = viewport.outer_size_px[1].max(MIN_VIEWPORT_HEIGHT);
        if let Some(bounds) = monitor_bounds.filter(|bounds| bounds.iter().all(|v| v.is_finite())) {
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
        if !viewport.outer_pos_px.iter().all(|value| value.is_finite())
            || !viewport.outer_size_px.iter().all(|value| value.is_finite())
        {
            return;
        }
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
        let detached_viewports: Vec<ViewportState> = detached_viewports
            .into_values()
            .filter(|viewport| {
                !self.hidden_tabs.contains(&viewport.tab)
                    && self.available_tabs.contains_key(&viewport.tab)
            })
            .collect();
        self.dock_state = if detached_viewports.is_empty() {
            None
        } else {
            let mut dock_state = DockState::new(Vec::new());
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
            && refreshed.iter().all(|(key, refreshed_tab)| {
                self.available_tabs
                    .get(key)
                    .map(|tab| {
                        tab.window_name == refreshed_tab.window_name
                            && tab.id.title == refreshed_tab.id.title
                    })
                    .unwrap_or(false)
            })
        {
            return;
        }

        self.available_tabs = refreshed;
        self.hidden_tabs
            .retain(|key| self.available_tabs.contains_key(key));
        self.main_window_rects
            .retain(|key, _| self.available_tabs.contains_key(key));
        self.tab_zones
            .retain(|key, _| self.available_tabs.contains_key(key));
        self.no_title_tabs
            .retain(|key| self.available_tabs.contains_key(key));
        for key in self.available_tabs.keys() {
            self.tab_zones
                .entry(key.clone())
                .or_insert_with(|| Self::default_zone_for_tab_key(key));
        }
        self.rebuild_dock_state();
        self.layout_dirty = true;
    }

    fn room_component_lines(component: Option<&Vec<Vec<TextSegment>>>) -> Vec<StyledLine> {
        component
            .map(|lines| {
                lines
                    .iter()
                    .map(|segments| StyledLine {
                        segments: segments.clone(),
                        stream: "room".to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn room_component_entries(component: Option<&Vec<Vec<TextSegment>>>) -> Vec<String> {
        component
            .map(|lines| {
                lines
                    .iter()
                    .map(|segments| {
                        segments
                            .iter()
                            .map(|segment| segment.text.as_str())
                            .collect::<String>()
                            .trim()
                            .to_string()
                    })
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn sync_room_windows_from_components(&mut self) {
        if !self.app_core.room_window_dirty {
            return;
        }

        let room_name = self
            .app_core
            .game_state
            .room_name
            .as_ref()
            .filter(|name| !name.trim().is_empty())
            .cloned()
            .or_else(|| self.app_core.room_subtitle.clone())
            .unwrap_or_default();
        let description =
            Self::room_component_lines(self.app_core.room_components.get("room desc"));
        let exits = if self.app_core.game_state.exits.is_empty() {
            Self::room_component_entries(self.app_core.room_components.get("room exits"))
        } else {
            self.app_core.game_state.exits.clone()
        };
        let players = if self.app_core.game_state.room_players.is_empty() {
            Self::room_component_entries(self.app_core.room_components.get("room players"))
        } else {
            self.app_core
                .game_state
                .room_players
                .iter()
                .map(|player| player.name.clone())
                .collect()
        };
        let objects = if self.app_core.game_state.room_objects.is_empty() {
            Self::room_component_entries(self.app_core.room_components.get("room objs"))
        } else {
            self.app_core
                .game_state
                .room_objects
                .iter()
                .map(|object| object.name.clone())
                .collect()
        };

        for window in self.app_core.ui_state.windows.values_mut() {
            let WindowContent::Room(room) = &mut window.content else {
                continue;
            };
            room.name = room_name.clone();
            room.description = description.clone();
            room.exits = exits.clone();
            room.players = players.clone();
            room.objects = objects.clone();
        }

        self.app_core.room_window_dirty = false;
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

    fn windows_for_menu(&self) -> Vec<(TabKey, String, bool, bool, GuiShellZone)> {
        let detached_tabs = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .unwrap_or_default();
        let mut entries: Vec<(TabKey, String, bool, bool, GuiShellZone)> = self
            .available_tabs
            .iter()
            .map(|(key, tab)| {
                let hidden = self.hidden_tabs.contains(key);
                let detached = detached_tabs.contains(key);
                let zone = self.zone_for_tab(key);
                (key.clone(), tab.id.title.clone(), hidden, detached, zone)
            })
            .collect();
        entries.sort_by_key(|(_, title, _, _, _)| title.to_ascii_lowercase());
        entries
    }

    fn zone_surface_tabs(&self, detached_tabs: &HashSet<TabKey>, zone: GuiShellZone) -> Vec<GuiTab> {
        let mut tabs: Vec<(i32, i32, String, GuiTab)> = self
            .available_tabs
            .iter()
            .filter_map(|(key, tab)| {
                if self.hidden_tabs.contains(key)
                    || detached_tabs.contains(key)
                    || self.zone_for_tab(key) != zone
                {
                    return None;
                }
                let window = self.app_core.ui_state.windows.get(&tab.window_name)?;
                let saved_y = self
                    .main_window_rects
                    .get(key)
                    .and_then(|rect| rect.get(1).copied())
                    .filter(|v| v.is_finite())
                    .unwrap_or(window.position.y as f32);
                let saved_x = self
                    .main_window_rects
                    .get(key)
                    .and_then(|rect| rect.get(0).copied())
                    .filter(|v| v.is_finite())
                    .unwrap_or(window.position.x as f32);
                Some((
                    saved_y.round() as i32,
                    saved_x.round() as i32,
                    tab.id.title.to_ascii_lowercase(),
                    tab.clone(),
                ))
            })
            .collect();
        tabs.sort_by_key(|(y, x, title, _)| (*y, *x, title.clone()));
        tabs.into_iter().map(|(_, _, _, tab)| tab).collect()
    }

    fn main_surface_bounds(&self, tabs: &[GuiTab]) -> (f32, f32) {
        let mut max_col = 0f32;
        let mut max_row = 0f32;
        for tab in tabs {
            let Some(window) = self.app_core.ui_state.windows.get(&tab.window_name) else {
                continue;
            };
            max_col = max_col.max((window.position.x + window.position.width).max(1) as f32);
            max_row = max_row.max((window.position.y + window.position.height).max(1) as f32);
        }
        (max_col.max(1.0), max_row.max(1.0))
    }

    fn docked_inner_size_for_outer(
        ctx: &egui::Context,
        outer_size: Vec2,
        include_title_bar: bool,
    ) -> Vec2 {
        let style = ctx.global_style();
        let window_frame = egui::Frame::window(&style).shadow(egui::epaint::Shadow::NONE);
        let mut margins = window_frame.total_margin().sum();
        if include_title_bar {
            let title_font = egui::TextStyle::Heading.resolve(&style);
            let title_bar_inner_height = ctx
                .fonts_mut(|fonts| fonts.row_height(&title_font))
                .max(style.spacing.interact_size.y);
            let title_bar_height_with_margin =
                title_bar_inner_height + window_frame.inner_margin.sum().y;
            let title_content_spacing = window_frame.stroke.width;
            margins += Vec2::new(0.0, title_bar_height_with_margin + title_content_spacing);
        }
        Vec2::new(
            (outer_size.x - margins.x).max(1.0),
            (outer_size.y - margins.y).max(1.0),
        )
    }

    fn tab_window_rect(
        root_rect: Rect,
        layout_bounds: (f32, f32),
        window: &WindowState,
    ) -> Option<Rect> {
        if !root_rect.is_finite() {
            return None;
        }
        let (max_col, max_row) = layout_bounds;
        if max_col <= 0.0 || max_row <= 0.0 {
            return None;
        }

        let left = root_rect.left() + (window.position.x as f32 / max_col) * root_rect.width();
        let top = root_rect.top() + (window.position.y as f32 / max_row) * root_rect.height();
        let width = ((window.position.width as f32 / max_col) * root_rect.width()).max(120.0);
        let height = ((window.position.height as f32 / max_row) * root_rect.height())
            .max(MIN_DOCKED_WINDOW_HEIGHT);
        if !left.is_finite() || !top.is_finite() || !width.is_finite() || !height.is_finite() {
            return None;
        }
        let rect = Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, height));
        let clipped = rect.intersect(root_rect);
        if !clipped.is_finite() {
            return None;
        }
        if clipped.width() < 60.0 || clipped.height() < MIN_DOCKED_WINDOW_HEIGHT {
            None
        } else {
            Some(clipped)
        }
    }

    fn detach_tab(&mut self, key: TabKey) {
        let Some(tab) = self.available_tabs.get(&key).cloned() else {
            return;
        };
        let already_detached = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .is_some_and(|detached| detached.contains(&key));
        if already_detached {
            return;
        }

        let mut dock_state = self
            .dock_state
            .take()
            .unwrap_or_else(|| DockState::new(Vec::new()));
        let surface = dock_state.add_window(vec![tab]);
        let bounds = self
            .last_monitor_bounds
            .unwrap_or([0.0, 0.0, 1200.0, 800.0]);
        let viewport = ViewportState::new(
            key,
            [bounds[0] + 120.0, bounds[1] + 120.0],
            [
                bounds[2].min(640.0).max(320.0),
                bounds[3].min(480.0).max(240.0),
            ],
        );
        Self::apply_viewport_to_surface(&mut dock_state, surface, &viewport, Some(bounds));
        self.dock_state = Some(dock_state);
        self.layout_dirty = true;
    }

    fn monitor_bounds_from_ctx(ctx: &egui::Context) -> [f32; 4] {
        ctx.input(|input| {
            if let (Some(outer_rect), Some(monitor_size)) =
                (input.viewport().outer_rect, input.viewport().monitor_size)
            {
                let bounds = [
                    outer_rect.min.x,
                    outer_rect.min.y,
                    monitor_size.x.max(1.0),
                    monitor_size.y.max(1.0),
                ];
                if bounds.iter().all(|value| value.is_finite()) {
                    return bounds;
                }
            }

            let content = input.content_rect();
            let content_bounds = [
                content.min.x,
                content.min.y,
                content.width().max(1.0),
                content.height().max(1.0),
            ];
            if content_bounds.iter().all(|value| value.is_finite()) {
                content_bounds
            } else {
                [0.0, 0.0, 1920.0, 1080.0]
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
        layout.ui_font = self.ui_font.clone();

        let snapshot = DockStateSnapshot {
            visible_tabs: self.current_main_surface_tab_keys(),
            main_window_rects: {
                let mut rects: Vec<MainWindowRectSnapshot> = self
                    .main_window_rects
                    .iter()
                    .filter(|(key, _)| self.available_tabs.contains_key(*key))
                    .map(|(key, rect)| MainWindowRectSnapshot {
                        key: key.clone(),
                        rect: *rect,
                    })
                    .collect();
                rects.sort_by_key(|entry| entry.key.short_id());
                rects
            },
            tab_zones: {
                let mut zones: Vec<TabZoneSnapshot> = self
                    .tab_zones
                    .iter()
                    .filter(|(key, _)| self.available_tabs.contains_key(*key))
                    .map(|(key, zone)| TabZoneSnapshot {
                        key: key.clone(),
                        zone: *zone,
                    })
                    .collect();
                zones.sort_by_key(|entry| entry.key.short_id());
                zones
            },
            no_title_tabs: {
                let mut keys: Vec<TabKey> = self
                    .no_title_tabs
                    .iter()
                    .filter(|key| self.available_tabs.contains_key(*key))
                    .cloned()
                    .collect();
                keys.sort_by_key(|key| key.short_id());
                keys
            },
            shell_layout: self.shell_layout.clone(),
        };
        layout.dock_state_json = match serde_json::to_value(snapshot) {
            Ok(value) => value,
            Err(err) => {
                // Persisting a null snapshot would wipe the saved window layout;
                // keep the existing file instead.
                tracing::error!("Failed to serialize GUI dock layout; skipping save: {}", err);
                return;
            }
        };
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

        // Play sounds queued by highlight processing.
        for sound in self.app_core.game_state.drain_sound_queue() {
            if let Some(ref player) = self.app_core.sound_player {
                if let Err(err) = player.play_from_sounds_dir(&sound.file, sound.volume) {
                    tracing::warn!("Failed to play sound '{}': {}", sound.file, err);
                }
            }
        }

        // Poll TTS callback events for auto-play.
        self.app_core.poll_tts_events();
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
                let key_event = crate::data::input::KeyEvent::new(
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
    ) -> Option<crate::data::input::KeyCode> {
        let code = match binding {
            "num_0" => crate::data::input::KeyCode::Keypad0,
            "num_1" => crate::data::input::KeyCode::Keypad1,
            "num_2" => crate::data::input::KeyCode::Keypad2,
            "num_3" => crate::data::input::KeyCode::Keypad3,
            "num_4" => crate::data::input::KeyCode::Keypad4,
            "num_5" => crate::data::input::KeyCode::Keypad5,
            "num_6" => crate::data::input::KeyCode::Keypad6,
            "num_7" => crate::data::input::KeyCode::Keypad7,
            "num_8" => crate::data::input::KeyCode::Keypad8,
            "num_9" => crate::data::input::KeyCode::Keypad9,
            "num_plus" => crate::data::input::KeyCode::KeypadPlus,
            "num_minus" => crate::data::input::KeyCode::KeypadMinus,
            "num_multiply" => crate::data::input::KeyCode::KeypadMultiply,
            "num_divide" => crate::data::input::KeyCode::KeypadDivide,
            "num_enter" => crate::data::input::KeyCode::KeypadEnter,
            "num_decimal" => crate::data::input::KeyCode::KeypadPeriod,
            _ => return None,
        };
        Some(code)
    }

    fn resolve_global_dispatch_target(
        key_event: crate::data::input::KeyEvent,
        keybind_map: &HashMap<crate::data::input::KeyEvent, KeyBindAction>,
        app_keybinds: &AppKeybinds,
        suppress_macro_dispatch: bool,
    ) -> Option<GlobalDispatchTarget> {
        if !suppress_macro_dispatch {
            if let Some(binding @ KeyBindAction::Macro(_)) = keybind_map.get(&key_event) {
                return Some(GlobalDispatchTarget::Macro(binding.clone()));
            }
        }

        Self::app_shortcut_for_key(key_event, app_keybinds).map(GlobalDispatchTarget::Shortcut)
    }

    fn app_shortcut_for_key(
        key_event: crate::data::input::KeyEvent,
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
        key_event: crate::data::input::KeyEvent,
    ) -> bool {
        crate::config::parse_key_string(binding)
            .map(|(code, modifiers)| crate::data::input::KeyEvent::new(code, modifiers))
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
        if self.window_context_menu.is_some() {
            self.window_context_menu = None;
            return;
        }
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
    ) -> Option<crate::data::input::KeyEvent> {
        let code = Self::egui_key_to_frontend_code(key, modifiers)?;
        let modifiers = Self::egui_modifiers_to_frontend(modifiers);
        Some(crate::data::input::KeyEvent::new(code, modifiers))
    }

    fn egui_modifiers_to_frontend(
        modifiers: egui::Modifiers,
    ) -> crate::data::input::KeyModifiers {
        crate::data::input::KeyModifiers {
            ctrl: modifiers.ctrl || modifiers.command,
            shift: modifiers.shift,
            alt: modifiers.alt,
        }
    }

    fn egui_key_to_frontend_code(
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) -> Option<crate::data::input::KeyCode> {
        let code = match key {
            egui::Key::ArrowDown => crate::data::input::KeyCode::Down,
            egui::Key::ArrowLeft => crate::data::input::KeyCode::Left,
            egui::Key::ArrowRight => crate::data::input::KeyCode::Right,
            egui::Key::ArrowUp => crate::data::input::KeyCode::Up,
            egui::Key::Escape => crate::data::input::KeyCode::Esc,
            egui::Key::Tab => {
                if modifiers.shift {
                    crate::data::input::KeyCode::BackTab
                } else {
                    crate::data::input::KeyCode::Tab
                }
            }
            egui::Key::Backspace => crate::data::input::KeyCode::Backspace,
            egui::Key::Enter => crate::data::input::KeyCode::Enter,
            egui::Key::Space => crate::data::input::KeyCode::Char(' '),
            egui::Key::Insert => crate::data::input::KeyCode::Insert,
            egui::Key::Delete => crate::data::input::KeyCode::Delete,
            egui::Key::Home => crate::data::input::KeyCode::Home,
            egui::Key::End => crate::data::input::KeyCode::End,
            egui::Key::PageUp => crate::data::input::KeyCode::PageUp,
            egui::Key::PageDown => crate::data::input::KeyCode::PageDown,
            egui::Key::Num0 => crate::data::input::KeyCode::Char('0'),
            egui::Key::Num1 => crate::data::input::KeyCode::Char('1'),
            egui::Key::Num2 => crate::data::input::KeyCode::Char('2'),
            egui::Key::Num3 => crate::data::input::KeyCode::Char('3'),
            egui::Key::Num4 => crate::data::input::KeyCode::Char('4'),
            egui::Key::Num5 => crate::data::input::KeyCode::Char('5'),
            egui::Key::Num6 => crate::data::input::KeyCode::Char('6'),
            egui::Key::Num7 => crate::data::input::KeyCode::Char('7'),
            egui::Key::Num8 => crate::data::input::KeyCode::Char('8'),
            egui::Key::Num9 => crate::data::input::KeyCode::Char('9'),
            egui::Key::A => crate::data::input::KeyCode::Char('a'),
            egui::Key::B => crate::data::input::KeyCode::Char('b'),
            egui::Key::C => crate::data::input::KeyCode::Char('c'),
            egui::Key::D => crate::data::input::KeyCode::Char('d'),
            egui::Key::E => crate::data::input::KeyCode::Char('e'),
            egui::Key::F => crate::data::input::KeyCode::Char('f'),
            egui::Key::G => crate::data::input::KeyCode::Char('g'),
            egui::Key::H => crate::data::input::KeyCode::Char('h'),
            egui::Key::I => crate::data::input::KeyCode::Char('i'),
            egui::Key::J => crate::data::input::KeyCode::Char('j'),
            egui::Key::K => crate::data::input::KeyCode::Char('k'),
            egui::Key::L => crate::data::input::KeyCode::Char('l'),
            egui::Key::M => crate::data::input::KeyCode::Char('m'),
            egui::Key::N => crate::data::input::KeyCode::Char('n'),
            egui::Key::O => crate::data::input::KeyCode::Char('o'),
            egui::Key::P => crate::data::input::KeyCode::Char('p'),
            egui::Key::Q => crate::data::input::KeyCode::Char('q'),
            egui::Key::R => crate::data::input::KeyCode::Char('r'),
            egui::Key::S => crate::data::input::KeyCode::Char('s'),
            egui::Key::T => crate::data::input::KeyCode::Char('t'),
            egui::Key::U => crate::data::input::KeyCode::Char('u'),
            egui::Key::V => crate::data::input::KeyCode::Char('v'),
            egui::Key::W => crate::data::input::KeyCode::Char('w'),
            egui::Key::X => crate::data::input::KeyCode::Char('x'),
            egui::Key::Y => crate::data::input::KeyCode::Char('y'),
            egui::Key::Z => crate::data::input::KeyCode::Char('z'),
            egui::Key::F1 => crate::data::input::KeyCode::F(1),
            egui::Key::F2 => crate::data::input::KeyCode::F(2),
            egui::Key::F3 => crate::data::input::KeyCode::F(3),
            egui::Key::F4 => crate::data::input::KeyCode::F(4),
            egui::Key::F5 => crate::data::input::KeyCode::F(5),
            egui::Key::F6 => crate::data::input::KeyCode::F(6),
            egui::Key::F7 => crate::data::input::KeyCode::F(7),
            egui::Key::F8 => crate::data::input::KeyCode::F(8),
            egui::Key::F9 => crate::data::input::KeyCode::F(9),
            egui::Key::F10 => crate::data::input::KeyCode::F(10),
            egui::Key::F11 => crate::data::input::KeyCode::F(11),
            egui::Key::F12 => crate::data::input::KeyCode::F(12),
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
                if outbound.starts_with("action:") {
                    if !self.handle_action_string(&outbound) {
                        self.app_core.add_system_message(&format!(
                            "GUI action not implemented yet: {}",
                            outbound
                        ));
                    }
                } else if Self::should_send_to_network(&outbound) {
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

    /// Dispatch an `action:*` string from a dot-command or menu item.
    /// Returns false when the action has no GUI handler yet.
    fn handle_action_string(&mut self, action: &str) -> bool {
        if action == "action:windows" || action == "action:listwindows" {
            let _ = self.app_core.send_command(".windows".to_string());
            return true;
        }
        if let Some(name) = action.strip_prefix("action:settheme:") {
            let name = name.to_string();
            self.apply_theme_by_name(&name);
            return true;
        }
        if action == "action:settings" {
            self.open_settings_editor();
            return true;
        }
        if action == "action:highlights" {
            self.open_highlight_editor(None);
            return true;
        }
        if action == "action:addhighlight" {
            self.open_highlight_editor(None);
            self.open_highlight_form_new();
            return true;
        }
        if let Some(name) = action.strip_prefix("action:edithighlight") {
            let name = name.strip_prefix(':').unwrap_or("").to_string();
            if name.is_empty() {
                self.open_highlight_editor(None);
            } else {
                self.open_highlight_editor(Some(&name));
            }
            return true;
        }
        false
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
        if click.link_data.exist_id == Self::QUICKBAR_SWITCH_SENTINEL {
            self.app_core.ui_state.active_quickbar_id = Some(click.link_data.noun.clone());
            return;
        }
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

    fn apply_window_menu_command(
        &mut self,
        request: &GuiWindowMenuRequest,
        command: GuiWindowMenuCommand,
    ) {
        match command {
            GuiWindowMenuCommand::Hide => self.hide_tab(request.tab_key.clone()),
            GuiWindowMenuCommand::Eject => self.detach_tab(request.tab_key.clone()),
            GuiWindowMenuCommand::ToggleTitleBar => self.toggle_title_bar(request.tab_key.clone()),
            GuiWindowMenuCommand::MoveUp => {
                if request.allow_reorder {
                    self.move_tab_within_zone(&request.tab_key, request.zone, true);
                }
            }
            GuiWindowMenuCommand::MoveDown => {
                if request.allow_reorder {
                    self.move_tab_within_zone(&request.tab_key, request.zone, false);
                }
            }
            GuiWindowMenuCommand::MoveTo(target) => {
                if target != request.zone {
                    self.set_tab_zone(request.tab_key.clone(), target);
                }
            }
        }
    }

    fn render_window_context_popup(&mut self, ctx: &egui::Context) {
        let Some(request) = self.window_context_menu.clone() else {
            return;
        };

        let mut selected_command: Option<GuiWindowMenuCommand> = None;
        let area_response = egui::Area::new(egui::Id::new("gui_window_context_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(request.position)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    selected_command = Self::render_window_context_menu(
                        ui,
                        request.zone,
                        request.allow_reorder,
                        request.title_bar_hidden,
                    );
                });
            });

        if let Some(command) = selected_command {
            self.apply_window_menu_command(&request, command);
            self.window_context_menu = None;
            return;
        }

        let menu_rect = area_response.response.rect;
        let should_close = ctx.input(|input| {
            input.pointer.any_click()
                && input
                    .pointer
                    .latest_pos()
                    .map(|pos| !menu_rect.contains(pos))
                    .unwrap_or(false)
        });
        if should_close {
            self.window_context_menu = None;
        }
    }

    fn render_menu_layer(
        ctx: &egui::Context,
        layer: GuiMenuLayer,
        menu: &PopupMenu,
    ) -> (Option<GuiMenuCommand>, Option<Rect>) {
        let layer_id = match layer {
            GuiMenuLayer::Main => "gui_popup_menu_main",
            GuiMenuLayer::Submenu => "gui_popup_menu_submenu",
            GuiMenuLayer::Nested => "gui_popup_menu_nested",
            GuiMenuLayer::Deep => "gui_popup_menu_deep",
        };

        let mut clicked_command: Option<String> = None;
        let pos = Pos2::new(menu.position.0 as f32, menu.position.1 as f32);
        let area_response = egui::Area::new(egui::Id::new(layer_id))
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
        let layer_rect = area_response.response.rect;

        (
            clicked_command.map(|command| GuiMenuCommand { layer, command }),
            if layer_rect.is_finite() {
                Some(layer_rect)
            } else {
                None
            },
        )
    }

    fn should_close_popup_menus_on_outside_click(
        any_click: bool,
        pointer_pos: Option<Pos2>,
        menu_rects: &[Rect],
    ) -> bool {
        if !any_click || menu_rects.is_empty() {
            return false;
        }
        let Some(pointer_pos) = pointer_pos else {
            return false;
        };

        menu_rects.iter().all(|rect| !rect.contains(pointer_pos))
    }

    fn open_child_menu_for_layer(
        &mut self,
        layer: GuiMenuLayer,
        items: Vec<crate::data::ui_state::PopupMenuItem>,
    ) {
        if items.is_empty() {
            return;
        }

        let parent_pos = match layer {
            GuiMenuLayer::Main => self
                .app_core
                .ui_state
                .popup_menu
                .as_ref()
                .map(|menu| menu.get_position()),
            GuiMenuLayer::Submenu => self
                .app_core
                .ui_state
                .submenu
                .as_ref()
                .map(|menu| menu.get_position()),
            GuiMenuLayer::Nested => self
                .app_core
                .ui_state
                .nested_submenu
                .as_ref()
                .map(|menu| menu.get_position()),
            GuiMenuLayer::Deep => self
                .app_core
                .ui_state
                .deep_submenu
                .as_ref()
                .map(|menu| menu.get_position()),
        }
        .unwrap_or((40, 12));

        let child = PopupMenu::new(items, (parent_pos.0.saturating_add(24), parent_pos.1));
        match layer {
            GuiMenuLayer::Main => {
                self.app_core.ui_state.submenu = Some(child);
                self.app_core.ui_state.nested_submenu = None;
                self.app_core.ui_state.deep_submenu = None;
            }
            GuiMenuLayer::Submenu => {
                self.app_core.ui_state.nested_submenu = Some(child);
                self.app_core.ui_state.deep_submenu = None;
            }
            GuiMenuLayer::Nested | GuiMenuLayer::Deep => {
                self.app_core.ui_state.deep_submenu = Some(child)
            }
        }
        self.app_core.ui_state.input_mode = InputMode::Menu;
    }

    fn handle_popup_menu_command(&mut self, menu_command: GuiMenuCommand) {
        let command = menu_command.command;

        if let Some(category) = command.strip_prefix("__SUBMENU__") {
            if let Some(items) = self.app_core.menu_categories.get(category).cloned() {
                self.open_child_menu_for_layer(menu_command.layer, items);
            } else {
                tracing::warn!("Missing GUI menu category: {}", category);
            }
            return;
        }

        if let Some(submenu) = command.strip_prefix("menu:") {
            let items = self.app_core.build_submenu(submenu);
            if items.is_empty() {
                self.app_core.add_system_message(&format!(
                    "Menu '{}' is not available in GUI yet.",
                    submenu
                ));
                self.close_all_popup_menus();
                self.app_core.ui_state.input_mode = InputMode::Normal;
            } else {
                self.open_child_menu_for_layer(menu_command.layer, items);
            }
            return;
        }

        if command.starts_with("action:") {
            if !self.handle_action_string(&command) {
                self.app_core
                    .add_system_message(&format!("GUI action not implemented yet: {}", command));
            }
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            return;
        }

        self.dispatch_raw_command(command);
        self.close_all_popup_menus();
        self.app_core.ui_state.input_mode = InputMode::Normal;
    }

    fn render_popup_menus(&mut self, ctx: &egui::Context) {
        let main = self.app_core.ui_state.popup_menu.as_ref();
        let submenu = self.app_core.ui_state.submenu.as_ref();
        let nested = self.app_core.ui_state.nested_submenu.as_ref();
        let deep = self.app_core.ui_state.deep_submenu.as_ref();

        let mut clicked_command: Option<GuiMenuCommand> = None;
        let mut menu_rects: Vec<Rect> = Vec::new();

        if let Some(menu) = main {
            let (command, rect) = Self::render_menu_layer(ctx, GuiMenuLayer::Main, menu);
            clicked_command = command;
            if let Some(rect) = rect {
                menu_rects.push(rect);
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = submenu {
                let (command, rect) = Self::render_menu_layer(ctx, GuiMenuLayer::Submenu, menu);
                clicked_command = command;
                if let Some(rect) = rect {
                    menu_rects.push(rect);
                }
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = nested {
                let (command, rect) = Self::render_menu_layer(ctx, GuiMenuLayer::Nested, menu);
                clicked_command = command;
                if let Some(rect) = rect {
                    menu_rects.push(rect);
                }
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = deep {
                let (command, rect) = Self::render_menu_layer(ctx, GuiMenuLayer::Deep, menu);
                clicked_command = command;
                if let Some(rect) = rect {
                    menu_rects.push(rect);
                }
            }
        }

        if let Some(command) = clicked_command {
            self.handle_popup_menu_command(command);
            return;
        }

        let should_close = ctx.input(|input| {
            Self::should_close_popup_menus_on_outside_click(
                input.pointer.any_click(),
                input.pointer.latest_pos(),
                &menu_rects,
            )
        });
        if should_close {
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
        }
    }

    fn render_window_context_menu(
        ui: &mut egui::Ui,
        zone: GuiShellZone,
        allow_reorder: bool,
        title_bar_hidden: bool,
    ) -> Option<GuiWindowMenuCommand> {
        if ui.button("Hide").clicked() {
            return Some(GuiWindowMenuCommand::Hide);
        }
        if ui.button("Eject").clicked() {
            return Some(GuiWindowMenuCommand::Eject);
        }
        if ui
            .button(if title_bar_hidden {
                "Show Title Bar"
            } else {
                "Hide Title Bar"
            })
            .clicked()
        {
            return Some(GuiWindowMenuCommand::ToggleTitleBar);
        }
        if allow_reorder {
            ui.separator();
            if ui.button("Move Up").clicked() {
                return Some(GuiWindowMenuCommand::MoveUp);
            }
            if ui.button("Move Down").clicked() {
                return Some(GuiWindowMenuCommand::MoveDown);
            }
        }
        ui.separator();
        ui.label("Move to");
        for target in GuiShellZone::all() {
            let is_current = target == zone;
            let label = if is_current {
                format!("{} (current)", target.label())
            } else {
                target.label().to_string()
            };
            if ui.selectable_label(is_current, label).clicked() {
                return Some(GuiWindowMenuCommand::MoveTo(target));
            }
        }
        None
    }

    fn zone_drag_pointer_for_rect(ctx: &egui::Context, window_rect: Rect) -> Option<Pos2> {
        ctx.input(|i| {
            if !i.modifiers.alt || !i.pointer.button_down(egui::PointerButton::Primary) {
                return None;
            }
            let pointer_pos = i.pointer.interact_pos().or(i.pointer.latest_pos())?;
            if !window_rect.contains(pointer_pos) || i.pointer.delta().length_sq() <= f32::EPSILON {
                return None;
            }
            Some(pointer_pos)
        })
    }

    fn zone_drop_insert_before(
        zone: GuiShellZone,
        pointer_pos: Pos2,
        window_rects: &[GuiZoneWindowRect],
        dragged_tab: &TabKey,
    ) -> Option<TabKey> {
        if matches!(zone, GuiShellZone::Center) {
            return None;
        }
        for window in window_rects
            .iter()
            .filter(|window| window.zone == zone && window.tab_key != *dragged_tab)
        {
            let should_insert_before = match zone {
                GuiShellZone::LeftSidebar | GuiShellZone::RightSidebar => {
                    pointer_pos.y < window.rect.center().y
                }
                GuiShellZone::Header | GuiShellZone::Footer => pointer_pos.x < window.rect.center().x,
                GuiShellZone::Center => false,
            };
            if should_insert_before {
                return Some(window.tab_key.clone());
            }
        }
        None
    }

    fn zone_for_pointer(
        zone_rects: &[(GuiShellZone, Rect)],
        pointer_pos: Pos2,
    ) -> Option<GuiShellZone> {
        zone_rects
            .iter()
            .find_map(|(zone, rect)| rect.contains(pointer_pos).then_some(*zone))
    }

    fn render_zone_drop_overlay(
        &mut self,
        ctx: &egui::Context,
        zone_rects: &[(GuiShellZone, Rect)],
        window_rects: &[GuiZoneWindowRect],
    ) -> Option<GuiZoneDropResult> {
        let mut drag = self.zone_drag_state.clone()?;
        let pointer_pos = ctx
            .input(|i| i.pointer.interact_pos().or(i.pointer.latest_pos()))
            .unwrap_or(drag.pointer_pos);
        drag.pointer_pos = pointer_pos;
        self.zone_drag_state = Some(drag.clone());
        if !ctx.input(|i| i.modifiers.alt) {
            self.zone_drag_state = None;
            return None;
        }

        let hovered_zone = Self::zone_for_pointer(zone_rects, pointer_pos);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("gui_zone_drop_overlay"),
        ));
        for (zone, rect) in zone_rects {
            let tint = if Some(*zone) == hovered_zone {
                Color32::from_rgba_unmultiplied(70, 130, 220, 48)
            } else {
                Color32::from_rgba_unmultiplied(35, 35, 35, 24)
            };
            painter.rect_filled(*rect, 0.0, tint);
        }

        let drop_hint = hovered_zone
            .map(|zone| {
                if zone == drag.from_zone {
                    format!("Reorder in {}", zone.label())
                } else {
                    format!("Drop to {}", zone.label())
                }
            })
            .unwrap_or_else(|| "Release to cancel move".to_string());
        egui::Area::new(egui::Id::new("gui_zone_drop_hint"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer_pos + Vec2::new(16.0, 16.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.label(drop_hint);
            });

        let pointer_released = ctx.input(|i| i.pointer.any_released());
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if pointer_released || !pointer_down {
            self.zone_drag_state = None;
            if let Some(target_zone) = hovered_zone {
                let insert_before = Self::zone_drop_insert_before(
                    target_zone,
                    pointer_pos,
                    window_rects,
                    &drag.tab_key,
                );
                if target_zone == drag.from_zone
                    && insert_before.is_none()
                    && matches!(target_zone, GuiShellZone::Center)
                {
                    return None;
                }
                return Some(GuiZoneDropResult {
                    tab_key: drag.tab_key,
                    target_zone,
                    insert_before,
                });
            }
        }
        None
    }

    fn render_zone_surface(
        &mut self,
        ctx: &egui::Context,
        detached_tabs: &HashSet<TabKey>,
        zone: GuiShellZone,
        root_rect: Rect,
        zone_window_rects: &mut Vec<GuiZoneWindowRect>,
    ) -> GuiWindowActions {
        let mut actions = GuiWindowActions::default();
        let primary_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        if !primary_down {
            self.hand_resize_tab = None;
        }
        if !root_rect.is_finite() || root_rect.width() <= 24.0 || root_rect.height() <= 24.0 {
            return actions;
        }

        let tabs = self.zone_surface_tabs(detached_tabs, zone);
        if tabs.is_empty() {
            return actions;
        }
        let layout_bounds = self.main_surface_bounds(&tabs);
        let is_sidebar = matches!(zone, GuiShellZone::LeftSidebar | GuiShellZone::RightSidebar);
        let secondary_click_pos = ctx.input(|input| {
            if input.pointer.secondary_clicked() {
                input.pointer.interact_pos()
            } else {
                None
            }
        });

        if is_sidebar {
            let margin = 0.0;
            let gap = 4.0;
            let min_slot_height = 120.0;
            let default_slot_height = 240.0;
            let slot_width = (root_rect.width() - margin * 2.0).max(120.0);
            let mut y = root_rect.min.y + margin;
            let tab_count = tabs.len();

            for (idx, tab) in tabs.into_iter().enumerate() {
                if y >= root_rect.max.y - margin {
                    break;
                }
                let remaining_tabs = tab_count.saturating_sub(idx + 1);
                let min_remaining_height = remaining_tabs as f32 * (min_slot_height + gap);
                let max_height_here = (root_rect.max.y - margin - y - min_remaining_height).max(min_slot_height);
                let desired_height = self
                    .main_window_rects
                    .get(&tab.id.key)
                    .map(|rect| rect[3])
                    .filter(|v| v.is_finite())
                    .unwrap_or(default_slot_height);
                let slot_height = desired_height.clamp(min_slot_height, max_height_here);
                let slot_bottom = (y + slot_height).min(root_rect.max.y - margin - min_remaining_height);
                let slot_rect = Rect::from_min_max(
                    Pos2::new(root_rect.min.x + margin, y),
                    Pos2::new(root_rect.min.x + margin + slot_width, slot_bottom),
                );
                y = slot_bottom + gap;
                if slot_rect.height() < 44.0 {
                    continue;
                }

                let mut clicked_link = None;
                let mut resize_delta_y = 0.0f32;
                let title_bar_hidden = self.title_bar_hidden(&tab.id.key);
                let window_id = egui::Id::new(format!(
                    "gui_zone_{}_window_{}",
                    zone.id_fragment(),
                    tab.id.key.short_id()
                ));
                if let Some(inner) = egui::Window::new(tab.id.title.clone())
                    .id(window_id)
                    .fixed_pos(slot_rect.min)
                    .fixed_size(Self::docked_inner_size_for_outer(
                        ctx,
                        slot_rect.size(),
                        !title_bar_hidden,
                    ))
                    .resizable(false)
                    .movable(false)
                    .title_bar(!title_bar_hidden)
                    .collapsible(false)
                    .frame(
                        egui::Frame::window(ctx.global_style().as_ref())
                            .outer_margin(egui::Margin::ZERO)
                            .shadow(egui::epaint::Shadow::NONE),
                    )
                    .constrain_to(root_rect)
                    .show(ctx, |ui| {
                        ui.push_id(tab.id.key.short_id(), |ui| {
                            let clicked = Self::render_window_content(&self.app_core, ui, &tab);
                            ui.separator();
                            let handle_response = ui.allocate_response(
                                Vec2::new(ui.available_width().max(1.0), 6.0),
                                egui::Sense::click_and_drag(),
                            );
                            if handle_response.hovered() || handle_response.dragged() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                            }
                            if handle_response.dragged() {
                                resize_delta_y += ui.ctx().input(|i| i.pointer.delta().y);
                            }
                            clicked
                        })
                        .inner
                    })
                {
                    clicked_link = inner.inner.flatten();
                    zone_window_rects.push(GuiZoneWindowRect {
                        zone,
                        tab_key: tab.id.key.clone(),
                        rect: inner.response.rect,
                    });
                    if let Some(pointer_pos) = secondary_click_pos {
                        if inner.response.rect.contains(pointer_pos) {
                            actions.window_menu_request = Some(GuiWindowMenuRequest {
                                tab_key: tab.id.key.clone(),
                                zone,
                                allow_reorder: true,
                                title_bar_hidden,
                                position: pointer_pos,
                            });
                        }
                    }
                    if self.zone_drag_state.is_none() {
                        if let Some(pointer_pos) =
                            Self::zone_drag_pointer_for_rect(ctx, inner.response.rect)
                        {
                            self.zone_drag_state = Some(GuiZoneDragState {
                                tab_key: tab.id.key.clone(),
                                from_zone: zone,
                                pointer_pos,
                            });
                        }
                    }
                }

                if let Some(click) = clicked_link {
                    actions.link_clicks.push(click);
                }
                if resize_delta_y.abs() > 0.0 {
                    let resized_height =
                        (slot_rect.height() + resize_delta_y).clamp(min_slot_height, max_height_here);
                    let entry = self
                        .main_window_rects
                        .entry(tab.id.key.clone())
                        .or_insert([slot_rect.min.x, slot_rect.min.y, slot_rect.width(), resized_height]);
                    entry[3] = resized_height;
                    self.layout_dirty = true;
                }
            }

            return actions;
        }

        let window_bounds = if zone == GuiShellZone::Center {
            root_rect.shrink(1.0)
        } else {
            root_rect
        };
        if !window_bounds.is_finite() || window_bounds.width() <= 8.0 || window_bounds.height() <= 8.0 {
            return actions;
        }

        let mut occupied_rects: Vec<Rect> = Vec::new();
        for tab in tabs {
            let Some(window) = self.app_core.ui_state.windows.get(&tab.window_name) else {
                continue;
            };
            let min_window_height = Self::min_window_height_for_zone(zone, window);
            let min_window_size = Vec2::new(
                120.0_f32.min(window_bounds.width().max(1.0)),
                min_window_height.min(window_bounds.height().max(1.0)),
            );
            // Keep a little vertical headroom in header/footer so windows can be repositioned
            // instead of filling the entire zone and snapping back to the top.
            let max_window_height = if matches!(zone, GuiShellZone::Header | GuiShellZone::Footer) {
                (window_bounds.height() - 12.0).max(min_window_size.y)
            } else {
                window_bounds.height().max(min_window_size.y)
            };
            let max_window_size = Vec2::new(
                window_bounds.width().max(min_window_size.x),
                max_window_height,
            );
            let fallback_rect =
                Self::tab_window_rect(window_bounds, layout_bounds, window).unwrap_or_else(|| {
                    Rect::from_min_size(
                        Pos2::new(window_bounds.min.x + 8.0, window_bounds.min.y + 8.0),
                        Vec2::new(
                            (window_bounds.width() - 16.0).max(min_window_size.x),
                            (window_bounds.height() - 16.0).max(min_window_size.y),
                        ),
                    )
                });
            let initial_rect = self
                .main_window_rects
                .get(&tab.id.key)
                .copied()
                .and_then(Self::rect_from_snapshot)
                .map(|rect| Self::clamp_main_window_rect(rect, window_bounds))
                .unwrap_or(fallback_rect);
            if !initial_rect.is_finite() {
                continue;
            }

            let mut clicked_link = None;
            let mut hand_resize_delta_x = 0.0f32;
            let title_bar_hidden = self.title_bar_hidden(&tab.id.key);
            let is_hand_widget = matches!(window.content, WindowContent::Hand { .. });
            let hand_resize_handle_width = 10.0f32;
            let pointer_over_hand_resize_handle = if is_hand_widget && primary_down {
                let handle_rect = Rect::from_min_max(
                    Pos2::new(initial_rect.max.x - hand_resize_handle_width, initial_rect.min.y),
                    initial_rect.max,
                );
                ctx.input(|i| {
                    i.pointer
                        .interact_pos()
                        .or(i.pointer.latest_pos())
                        .is_some_and(|pos| handle_rect.contains(pos))
                })
            } else {
                false
            };
            if is_hand_widget
                && primary_down
                && pointer_over_hand_resize_handle
                && self.hand_resize_tab.is_none()
            {
                self.hand_resize_tab = Some(tab.id.key.clone());
            }
            let hand_resize_active = is_hand_widget
                && primary_down
                && self
                    .hand_resize_tab
                    .as_ref()
                    .is_some_and(|key| key == &tab.id.key);
            let window_id = egui::Id::new(format!(
                "gui_zone_{}_window_{}",
                zone.id_fragment(),
                tab.id.key.short_id()
            ));
            let docked_window_frame = egui::Frame::window(ctx.global_style().as_ref())
                .outer_margin(egui::Margin::ZERO)
                .shadow(egui::epaint::Shadow::NONE);
            let mut window_builder = egui::Window::new(tab.id.title.clone())
                .id(window_id)
                .default_size(if zone == GuiShellZone::Center {
                    initial_rect.size()
                } else {
                    Self::docked_inner_size_for_outer(ctx, initial_rect.size(), !title_bar_hidden)
                })
                .min_size(min_window_size)
                .max_size(max_window_size)
                .resizable(true)
                .movable(!ctx.input(|i| i.modifiers.alt) && !hand_resize_active)
                .title_bar(!title_bar_hidden)
                .collapsible(false)
                .constrain_to(window_bounds)
                .frame(docked_window_frame);
            if is_hand_widget {
                let fixed_inner_size = if zone == GuiShellZone::Center {
                    initial_rect.size()
                } else {
                    Self::docked_inner_size_for_outer(ctx, initial_rect.size(), !title_bar_hidden)
                };
                window_builder = window_builder.fixed_size(fixed_inner_size).resizable(false);
            }
            let is_compact_center_widget =
                zone == GuiShellZone::Center && Self::is_compact_center_widget(&window.widget_type);
            if zone == GuiShellZone::Center && !is_compact_center_widget {
                // Prevent content-driven growth by making the window scroll instead of expanding.
                window_builder = window_builder.scroll([true, true]);
            }
            window_builder = if zone == GuiShellZone::Center {
                window_builder.current_pos(initial_rect.min)
            } else {
                window_builder.default_pos(initial_rect.min)
            };
            if let Some(inner) = window_builder.show(ctx, |ui| {
                    ui.push_id(tab.id.key.short_id(), |ui| {
                        Self::render_window_content(&self.app_core, ui, &tab)
                    })
                    .inner
                }) {
                if is_hand_widget {
                    let handle_rect = Rect::from_min_max(
                        Pos2::new(
                            inner.response.rect.max.x - hand_resize_handle_width,
                            inner.response.rect.min.y,
                        ),
                        inner.response.rect.max,
                    );
                    if hand_resize_active
                        || ctx.input(|i| {
                            i.pointer
                                .interact_pos()
                                .or(i.pointer.latest_pos())
                                .is_some_and(|pos| handle_rect.contains(pos))
                        })
                    {
                        ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    if hand_resize_active {
                        hand_resize_delta_x += ctx.input(|i| i.pointer.delta().x);
                    }
                }
                let center_rect_changed = zone == GuiShellZone::Center
                    && ((inner.response.rect.min - initial_rect.min).length_sq() > 0.25
                        || (inner.response.rect.size() - initial_rect.size()).length_sq() > 0.25);
                let should_track_rect = zone != GuiShellZone::Center || center_rect_changed;
                if should_track_rect {
                    self.track_main_window_rect(&tab.id.key, inner.response.rect, window_bounds);
                }
                if zone == GuiShellZone::Center {
                    let clamped = Self::clamp_main_window_rect(inner.response.rect, window_bounds);
                    if clamped.is_finite() {
                        self.last_center_window_rects
                            .insert(tab.id.key.clone(), Self::rect_to_snapshot(clamped));
                    }
                }
                clicked_link = inner.inner.flatten();
                zone_window_rects.push(GuiZoneWindowRect {
                    zone,
                    tab_key: tab.id.key.clone(),
                    rect: inner.response.rect,
                });
                if let Some(pointer_pos) = secondary_click_pos {
                    if inner.response.rect.contains(pointer_pos) {
                        actions.window_menu_request = Some(GuiWindowMenuRequest {
                            tab_key: tab.id.key.clone(),
                            zone,
                            allow_reorder: false,
                            title_bar_hidden,
                            position: pointer_pos,
                        });
                    }
                }
                if is_hand_widget && hand_resize_delta_x.abs() > 0.0 {
                    let resized_width =
                        (inner.response.rect.width() + hand_resize_delta_x).clamp(min_window_size.x, max_window_size.x);
                    let entry = self.main_window_rects.entry(tab.id.key.clone()).or_insert([
                        inner.response.rect.min.x,
                        inner.response.rect.min.y,
                        inner.response.rect.width(),
                        inner.response.rect.height(),
                    ]);
                    entry[2] = resized_width;
                    self.layout_dirty = true;
                }
                occupied_rects.push(inner.response.rect);
                if self.zone_drag_state.is_none() {
                    if let Some(pointer_pos) =
                        Self::zone_drag_pointer_for_rect(ctx, inner.response.rect)
                    {
                        self.zone_drag_state = Some(GuiZoneDragState {
                            tab_key: tab.id.key.clone(),
                            from_zone: zone,
                            pointer_pos,
                        });
                    }
                }
            }
            if let Some(click) = clicked_link {
                actions.link_clicks.push(click);
            }
        }

        actions
    }

    fn render_detached_window_host(
        &mut self,
        ui: &mut egui::Ui,
    ) -> (Vec<TabKey>, Vec<GuiLinkClick>) {
        let mut closed_tabs = Vec::new();
        let mut link_clicks = Vec::new();
        let Some(dock_state) = &mut self.dock_state else {
            return (closed_tabs, link_clicks);
        };

        let max_rect = ui.max_rect();
        if !max_rect.is_finite() {
            return (closed_tabs, link_clicks);
        }
        let host_rect = Rect::from_min_size(max_rect.min, Vec2::new(1.0, 1.0));
        ui.allocate_ui_at_rect(host_rect, |ui| {
            let mut viewer = GuiDockTabViewer::new(&self.app_core);
            DockArea::new(dock_state).show_inside(ui, &mut viewer);
            closed_tabs = viewer.take_closed_tabs();
            link_clicks = viewer.take_link_clicks();
        });

        (closed_tabs, link_clicks)
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
        self.app_core.perf_stats.record_frame();
        if !self.fonts_applied {
            self.fonts_applied = true;
            if let Some(fonts) = theme::font_definitions_from_ref(&self.ui_font) {
                ctx.set_fonts(fonts);
            }
        }
        self.apply_theme_if_changed(&ctx);
        self.pump_server_messages();
        self.sync_room_windows_from_components();
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

        let detached_before_frame = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .unwrap_or_default();
        let mut visibility_toggles: Vec<TabKey> = Vec::new();
        let mut zone_assignments: Vec<(TabKey, GuiShellZone)> = Vec::new();
        let mut zone_actions = GuiWindowActions::default();
        let mut closed_tabs = Vec::new();
        let mut detached_link_clicks = Vec::new();
        let mut visible_zone_rects: Vec<(GuiShellZone, Rect)> = Vec::new();
        let mut zone_window_rects: Vec<GuiZoneWindowRect> = Vec::new();

        egui::TopBottomPanel::top("gui_shell_toolbar")
            .resizable(false)
            .exact_height(30.0)
            .show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("VellumFE GUI");
                    let connection_text = if self.app_core.game_state.connected {
                        RichText::new("Connected")
                            .color(theme::color32(self.current_theme.status_success))
                    } else {
                        RichText::new("Disconnected")
                            .color(theme::color32(self.current_theme.status_error))
                    };
                    ui.separator();
                    ui.label(connection_text);
                    ui.separator();

                    if ui
                        .small_button(if self.shell_layout.header_visible {
                            "Hide Header"
                        } else {
                            "Show Header"
                        })
                        .clicked()
                    {
                        self.shell_layout.header_visible = !self.shell_layout.header_visible;
                        self.layout_dirty = true;
                    }
                    if ui
                        .small_button(if self.shell_layout.footer_visible {
                            "Hide Footer"
                        } else {
                            "Show Footer"
                        })
                        .clicked()
                    {
                        self.shell_layout.footer_visible = !self.shell_layout.footer_visible;
                        self.layout_dirty = true;
                    }
                    if ui
                        .small_button(if self.shell_layout.left_sidebar_collapsed {
                            "Show Left Bar"
                        } else {
                            "Hide Left Bar"
                        })
                        .clicked()
                    {
                        self.shell_layout.left_sidebar_collapsed =
                            !self.shell_layout.left_sidebar_collapsed;
                        self.layout_dirty = true;
                    }
                    if ui
                        .small_button(if self.shell_layout.right_sidebar_collapsed {
                            "Show Right Bar"
                        } else {
                            "Hide Right Bar"
                        })
                        .clicked()
                    {
                        self.shell_layout.right_sidebar_collapsed =
                            !self.shell_layout.right_sidebar_collapsed;
                        self.layout_dirty = true;
                    }

                    ui.menu_button("Windows", |ui| {
                        let windows = self.windows_for_menu();
                        if windows.is_empty() {
                            ui.label("No windows available");
                            return;
                        }

                        for (key, title, is_hidden, is_detached, zone) in windows {
                            ui.horizontal(|ui| {
                                let mut visible = !is_hidden;
                                let mut label = title.clone();
                                if is_detached {
                                    label.push_str(" (detached)");
                                }
                                if ui.checkbox(&mut visible, label).changed() {
                                    visibility_toggles.push(key.clone());
                                }

                                ui.menu_button(format!("Zone: {}", zone.label()), |ui| {
                                    for target in GuiShellZone::all() {
                                        let is_current = target == zone;
                                        let target_label = if is_current {
                                            format!("{} (current)", target.label())
                                        } else {
                                            target.label().to_string()
                                        };
                                        if ui.selectable_label(is_current, target_label).clicked() {
                                            zone_assignments.push((key.clone(), target));
                                            ui.close_menu();
                                        }
                                    }
                                });
                            });
                        }
                    });
                });
            });

        if self.shell_layout.header_visible {
            egui::TopBottomPanel::top("gui_shell_header")
                .resizable(false)
                .exact_height(self.shell_layout.header_height)
                .frame(
                    egui::Frame::default()
                        .inner_margin(egui::Margin::ZERO)
                        .outer_margin(egui::Margin::ZERO),
                )
                .show(&ctx, |ui| {
                    let header_zone_rect = ui.max_rect();
                    visible_zone_rects.push((GuiShellZone::Header, header_zone_rect));
                    let header_handle_h = 10.0;
                    let header_handle_rect = if header_zone_rect.height() > header_handle_h {
                        Some(Rect::from_min_max(
                            Pos2::new(
                                header_zone_rect.min.x,
                                header_zone_rect.max.y - header_handle_h,
                            ),
                            header_zone_rect.max,
                        ))
                    } else {
                        None
                    };
                    zone_actions.merge(self.render_zone_surface(
                        &ctx,
                        &detached_before_frame,
                        GuiShellZone::Header,
                        header_zone_rect,
                        &mut zone_window_rects,
                    ));

                    if let Some(handle_rect) = header_handle_rect {
                        let handle_response = ui.interact(
                            handle_rect,
                            egui::Id::new("gui_header_resize_handle"),
                            egui::Sense::click_and_drag(),
                        );
                        if handle_response.hovered() || handle_response.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                        }
                        if handle_response.dragged() {
                            let dy = ui.ctx().input(|i| i.pointer.delta().y);
                            self.shell_layout.header_height =
                                (self.shell_layout.header_height + dy).clamp(96.0, 360.0);
                            self.layout_dirty = true;
                        }
                    }
                });
        }

        egui::TopBottomPanel::bottom("gui_command_input").show(&ctx, |ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.command_input)
                    .hint_text("Enter command...")
                    .desired_width(ui.available_width()),
            );

            let pressed_enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if response.lost_focus() && pressed_enter {
                self.submit_command();
                response.request_focus();
            }
        });

        if self.shell_layout.footer_visible {
            egui::TopBottomPanel::bottom("gui_shell_footer")
                .resizable(false)
                .exact_height(self.shell_layout.footer_height)
                .frame(
                    egui::Frame::default()
                        .inner_margin(egui::Margin::ZERO)
                        .outer_margin(egui::Margin::ZERO),
                )
                .show(&ctx, |ui| {
                    let footer_zone_rect = ui.max_rect();
                    visible_zone_rects.push((GuiShellZone::Footer, footer_zone_rect));
                    let footer_handle_h = 10.0;
                    let footer_handle_rect = if footer_zone_rect.height() > footer_handle_h {
                        Some(Rect::from_min_max(
                            footer_zone_rect.min,
                            Pos2::new(
                                footer_zone_rect.max.x,
                                footer_zone_rect.min.y + footer_handle_h,
                            ),
                        ))
                    } else {
                        None
                    };
                    zone_actions.merge(self.render_zone_surface(
                        &ctx,
                        &detached_before_frame,
                        GuiShellZone::Footer,
                        footer_zone_rect,
                        &mut zone_window_rects,
                    ));

                    if let Some(handle_rect) = footer_handle_rect {
                        let handle_response = ui.interact(
                            handle_rect,
                            egui::Id::new("gui_footer_resize_handle"),
                            egui::Sense::click_and_drag(),
                        );
                        if handle_response.hovered() || handle_response.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                        }
                        if handle_response.dragged() {
                            let dy = ui.ctx().input(|i| i.pointer.delta().y);
                            self.shell_layout.footer_height =
                                (self.shell_layout.footer_height - dy).clamp(96.0, 420.0);
                            self.layout_dirty = true;
                        }
                    }
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .inner_margin(egui::Margin::ZERO)
                    .outer_margin(egui::Margin::ZERO),
            )
            .show(&ctx, |ui| {
            let root = ui.max_rect();
            if !root.is_finite() || root.width() <= 24.0 || root.height() <= 24.0 {
                return;
            }

            self.shell_layout.sanitize(root.width());
            let min_center_width = 220.0;
            let mut left_width = if self.shell_layout.left_sidebar_collapsed {
                0.0
            } else {
                self.shell_layout.left_sidebar_width
            };
            let mut right_width = if self.shell_layout.right_sidebar_collapsed {
                0.0
            } else {
                self.shell_layout.right_sidebar_width
            };
            if left_width + right_width > (root.width() - min_center_width).max(0.0) {
                let overflow = left_width + right_width - (root.width() - min_center_width).max(0.0);
                let shrink_left = (overflow * 0.5).min(left_width.max(0.0));
                left_width = (left_width - shrink_left).max(220.0);
                right_width = (right_width - (overflow - shrink_left)).max(220.0);
            }
            if !self.shell_layout.left_sidebar_collapsed
                && (self.shell_layout.left_sidebar_width - left_width).abs() > 0.5
            {
                self.shell_layout.left_sidebar_width = left_width;
                self.layout_dirty = true;
            }
            if !self.shell_layout.right_sidebar_collapsed
                && (self.shell_layout.right_sidebar_width - right_width).abs() > 0.5
            {
                self.shell_layout.right_sidebar_width = right_width;
                self.layout_dirty = true;
            }

            let left_rect = if left_width > 0.0 {
                Some(Rect::from_min_max(
                    root.min,
                    Pos2::new(root.min.x + left_width, root.max.y),
                ))
            } else {
                None
            };
            let right_rect = if right_width > 0.0 {
                Some(Rect::from_min_max(
                    Pos2::new(root.max.x - right_width, root.min.y),
                    root.max,
                ))
            } else {
                None
            };
            let center_min_x = left_rect.map(|rect| rect.max.x).unwrap_or(root.min.x);
            let center_max_x = right_rect.map(|rect| rect.min.x).unwrap_or(root.max.x);
            let center_rect = Rect::from_min_max(
                Pos2::new(center_min_x, root.min.y),
                Pos2::new(center_max_x, root.max.y),
            );
            visible_zone_rects.push((GuiShellZone::Center, center_rect));

            let sidebar_divider_stroke = egui::Stroke::new(
                1.5,
                ui.visuals().window_stroke.color,
            );
            if let Some(rect) = left_rect {
                ui.painter()
                    .vline(rect.max.x, root.y_range(), sidebar_divider_stroke);
            }
            if let Some(rect) = right_rect {
                ui.painter()
                    .vline(rect.min.x, root.y_range(), sidebar_divider_stroke);
            }

            zone_actions.merge(self.render_zone_surface(
                &ctx,
                &detached_before_frame,
                GuiShellZone::Center,
                center_rect,
                &mut zone_window_rects,
            ));

            if let Some(rect) = left_rect {
                visible_zone_rects.push((GuiShellZone::LeftSidebar, rect));
                let splitter = Rect::from_min_max(
                    Pos2::new(rect.max.x - 6.0, rect.min.y),
                    Pos2::new(rect.max.x + 6.0, rect.max.y),
                );
                let splitter_response = ui.interact(
                    splitter,
                    egui::Id::new("gui_left_sidebar_splitter"),
                    egui::Sense::click_and_drag(),
                );
                if splitter_response.hovered() || splitter_response.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if splitter_response.dragged() {
                    let dx = ui.ctx().input(|i| i.pointer.delta().x);
                    self.shell_layout.left_sidebar_width =
                        (self.shell_layout.left_sidebar_width + dx).clamp(220.0, 700.0);
                    self.layout_dirty = true;
                }
                zone_actions.merge(self.render_zone_surface(
                    &ctx,
                    &detached_before_frame,
                    GuiShellZone::LeftSidebar,
                    rect,
                    &mut zone_window_rects,
                ));
            }

            if let Some(rect) = right_rect {
                visible_zone_rects.push((GuiShellZone::RightSidebar, rect));
                let splitter = Rect::from_min_max(
                    Pos2::new(rect.min.x - 6.0, rect.min.y),
                    Pos2::new(rect.min.x + 6.0, rect.max.y),
                );
                let splitter_response = ui.interact(
                    splitter,
                    egui::Id::new("gui_right_sidebar_splitter"),
                    egui::Sense::click_and_drag(),
                );
                if splitter_response.hovered() || splitter_response.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if splitter_response.dragged() {
                    let dx = ui.ctx().input(|i| i.pointer.delta().x);
                    self.shell_layout.right_sidebar_width =
                        (self.shell_layout.right_sidebar_width - dx).clamp(220.0, 700.0);
                    self.layout_dirty = true;
                }
                zone_actions.merge(self.render_zone_surface(
                    &ctx,
                    &detached_before_frame,
                    GuiShellZone::RightSidebar,
                    rect,
                    &mut zone_window_rects,
                ));
            }
            (closed_tabs, detached_link_clicks) = self.render_detached_window_host(ui);
        });

        let zone_drop_result =
            self.render_zone_drop_overlay(&ctx, &visible_zone_rects, &zone_window_rects);

        for key in visibility_toggles {
            if self.hidden_tabs.contains(&key) {
                self.restore_tab(key);
            } else {
                self.hide_tab(key);
            }
        }
        for (key, zone) in zone_assignments {
            self.set_tab_zone(key, zone);
        }
        if let Some(drop_result) = zone_drop_result {
            self.apply_zone_drop(drop_result);
        }
        if let Some(request) = zone_actions.window_menu_request {
            self.close_all_popup_menus();
            self.window_context_menu = Some(request);
        }
        let mut link_clicks = zone_actions.link_clicks;
        link_clicks.extend(detached_link_clicks);

        for key in closed_tabs {
            self.hide_tab(key);
        }
        self.hide_removed_detached_tabs(&detached_before_frame);
        for click in link_clicks {
            self.handle_link_click(click);
        }
        self.render_window_context_popup(&ctx);
        self.render_popup_menus(&ctx);
        self.render_injuries_popup(&ctx);
        self.render_editors(&ctx);
        // Layout mutations mark `layout_dirty` at their call sites; debounce the
        // blocking disk write until the layout has been stable for a while. Any
        // still-pending save is flushed on shutdown.
        if self.layout_dirty {
            self.layout_dirty = false;
            self.layout_dirty_since = Some(Instant::now());
        }
        if let Some(dirty_since) = self.layout_dirty_since {
            if dirty_since.elapsed() >= LAYOUT_SAVE_DEBOUNCE {
                self.save_layout_state();
                self.layout_dirty_since = None;
            }
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }

    fn on_exit(&mut self) {
        // Flush any debounced layout changes while the app is still intact,
        // rather than during Drop teardown.
        self.save_layout_state();
    }
}

impl Drop for VellumGuiApp {
    fn drop(&mut self) {
        if let Some(handle) = self.network_handle.take() {
            handle.abort();
        }
    }
}

pub fn run_native_gui(
    app_core: AppCore,
    direct: Option<crate::network::DirectConnectConfig>,
    login_key: Option<String>,
) -> Result<()> {
    let window_title = app_core
        .config
        .connection
        .character
        .as_deref()
        .or(app_core.config.character.as_deref())
        .map(|character| format!("VellumFE - {}", character))
        .unwrap_or_else(|| "VellumFE".to_string());
    let viewport = ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_title(window_title.clone());
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let app = VellumGuiApp::new(
        app_core,
        direct,
        login_key,
        INITIAL_LAYOUT_WIDTH as f32,
        INITIAL_LAYOUT_HEIGHT as f32,
    )?;

    eframe::run_native(
        &window_title,
        options,
        Box::new(move |_cc| Ok(Box::new(app))),
    )
    .map_err(|err| anyhow!("Failed to run GUI frontend: {}", err))
}

#[cfg(test)]
mod tests {
    use super::widgets::parse_hex_color;
    use super::{
        AppShortcut, DockStateSnapshot, GlobalDispatchTarget, GuiLinkDispatch, GuiTab,
        ShellLayoutSnapshot, VellumGuiApp,
    };
    use crate::config::{AppKeybinds, Config, KeyBindAction, MacroAction, TargetListConfig};
    use crate::core::state::{Creature, Player};
    use crate::data::{LinkData, SpanType, TextSegment};
    use crate::data::input::{KeyCode, KeyEvent, KeyModifiers};
    use crate::frontend::gui::{GuiLayoutFileV1, TabId, TabKey, ViewportState};
    use eframe::egui::{Color32, Pos2, Rect, Vec2};
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
            main_window_rects: Vec::new(),
            tab_zones: Vec::new(),
            no_title_tabs: Vec::new(),
            shell_layout: ShellLayoutSnapshot::default(),
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
    fn test_segment_has_clickable_link_for_monsterbold_link_segment() {
        let segment = TextSegment {
            text: "goblin".to_string(),
            fg: Some("#00ff00".to_string()),
            bg: None,
            bold: true,
            mono: false,
            span_type: SpanType::Monsterbold,
            link_data: Some(LinkData {
                exist_id: "12345".to_string(),
                noun: "goblin".to_string(),
                text: "goblin".to_string(),
                coord: None,
            }),
        };

        assert!(VellumGuiApp::segment_has_clickable_link(&segment));
    }

    #[test]
    fn test_segment_has_clickable_link_false_without_link_data() {
        let segment = TextSegment {
            text: "plain text".to_string(),
            fg: None,
            bg: None,
            bold: false,
            mono: false,
            span_type: SpanType::Link,
            link_data: None,
        };

        assert!(!VellumGuiApp::segment_has_clickable_link(&segment));
    }

    #[test]
    fn test_click_pos_to_grid_clamps_values() {
        let pos = Pos2::new(-10.0, 999999.0);
        let (x, y) = VellumGuiApp::click_pos_to_grid(pos);
        assert_eq!(x, 0);
        assert_eq!(y, u16::MAX);
    }

    #[test]
    fn test_default_zone_for_tab_key_assignments() {
        assert_eq!(
            VellumGuiApp::default_zone_for_tab_key(&TabKey::LeftHand),
            super::GuiShellZone::Header
        );
        assert_eq!(
            VellumGuiApp::default_zone_for_tab_key(&TabKey::Compass),
            super::GuiShellZone::Footer
        );
        assert_eq!(
            VellumGuiApp::default_zone_for_tab_key(&TabKey::TextMain),
            super::GuiShellZone::Center
        );
    }

    #[test]
    fn test_zone_for_pointer_returns_matching_zone() {
        let zone_rects = vec![
            (
                super::GuiShellZone::Header,
                Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(400.0, 100.0)),
            ),
            (
                super::GuiShellZone::Center,
                Rect::from_min_max(Pos2::new(0.0, 100.0), Pos2::new(400.0, 400.0)),
            ),
        ];

        let zone = VellumGuiApp::zone_for_pointer(&zone_rects, Pos2::new(80.0, 40.0));
        assert_eq!(zone, Some(super::GuiShellZone::Header));
    }

    #[test]
    fn test_zone_for_pointer_returns_none_outside_rects() {
        let zone_rects = vec![(
            super::GuiShellZone::Center,
            Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 300.0)),
        )];

        let zone = VellumGuiApp::zone_for_pointer(&zone_rects, Pos2::new(50.0, 50.0));
        assert_eq!(zone, None);
    }

    #[test]
    fn test_zone_drop_insert_before_uses_header_x_axis() {
        let window_rects = vec![
            super::GuiZoneWindowRect {
                zone: super::GuiShellZone::Header,
                tab_key: TabKey::Compass,
                rect: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 60.0)),
            },
            super::GuiZoneWindowRect {
                zone: super::GuiShellZone::Header,
                tab_key: TabKey::Room,
                rect: Rect::from_min_max(Pos2::new(120.0, 0.0), Pos2::new(220.0, 60.0)),
            },
        ];

        // x=130 is left of Room's center (170) but right of Compass's (50):
        // insert before Room. A y-axis mixup would return None (y=30 is at
        // both windows' center line), so this pins the axis choice too.
        let before = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::Header,
            Pos2::new(130.0, 30.0),
            &window_rects,
            &TabKey::TextMain,
        );
        assert_eq!(before, Some(TabKey::Room));

        // Past the last window's center: append at end (None).
        let after_last = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::Header,
            Pos2::new(180.0, 30.0),
            &window_rects,
            &TabKey::TextMain,
        );
        assert_eq!(after_last, None);
    }

    #[test]
    fn test_zone_drop_insert_before_uses_sidebar_y_axis() {
        let window_rects = vec![
            super::GuiZoneWindowRect {
                zone: super::GuiShellZone::LeftSidebar,
                tab_key: TabKey::Targets,
                rect: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(220.0, 120.0)),
            },
            super::GuiZoneWindowRect {
                zone: super::GuiShellZone::LeftSidebar,
                tab_key: TabKey::Players,
                rect: Rect::from_min_max(Pos2::new(0.0, 130.0), Pos2::new(220.0, 250.0)),
            },
        ];

        // y=100 is above Players' center (190) but below Targets' (60):
        // insert before Players. An x-axis mixup would return Some(Targets)
        // (x=80 is left of both centers), so this pins the axis choice too.
        let before = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::LeftSidebar,
            Pos2::new(80.0, 100.0),
            &window_rects,
            &TabKey::TextMain,
        );
        assert_eq!(before, Some(TabKey::Players));

        // Past the last window's center: append at end (None).
        let after_last = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::LeftSidebar,
            Pos2::new(80.0, 210.0),
            &window_rects,
            &TabKey::TextMain,
        );
        assert_eq!(after_last, None);
    }

    #[test]
    fn test_zone_drop_insert_before_ignores_center_zone() {
        let window_rects = vec![super::GuiZoneWindowRect {
            zone: super::GuiShellZone::Center,
            tab_key: TabKey::TextMain,
            rect: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(220.0, 120.0)),
        }];

        let before = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::Center,
            Pos2::new(40.0, 40.0),
            &window_rects,
            &TabKey::Room,
        );
        assert_eq!(before, None);
    }

    #[test]
    fn test_should_close_popup_menus_on_outside_click_true() {
        let menu_rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(220.0, 180.0));
        let should_close = VellumGuiApp::should_close_popup_menus_on_outside_click(
            true,
            Some(Pos2::new(50.0, 50.0)),
            &[menu_rect],
        );
        assert!(should_close);
    }

    #[test]
    fn test_should_close_popup_menus_on_outside_click_false_for_inside_click() {
        let menu_rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(220.0, 180.0));
        let should_close = VellumGuiApp::should_close_popup_menus_on_outside_click(
            true,
            Some(Pos2::new(150.0, 120.0)),
            &[menu_rect],
        );
        assert!(!should_close);
    }

    #[test]
    fn test_should_close_popup_menus_on_outside_click_false_without_click() {
        let menu_rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(220.0, 180.0));
        let should_close = VellumGuiApp::should_close_popup_menus_on_outside_click(
            false,
            Some(Pos2::new(50.0, 50.0)),
            &[menu_rect],
        );
        assert!(!should_close);
    }

    #[test]
    fn test_status_abbreviation_prefers_config_value() {
        let mut cfg = TargetListConfig::default();
        cfg.status_abbrev
            .insert("weirdstatus".to_string(), "wiz".to_string());

        let abbreviated = VellumGuiApp::status_abbreviation("weirdstatus", &cfg);
        assert_eq!(abbreviated, "wiz");
    }

    #[test]
    fn test_status_abbreviation_falls_back_to_first_three_chars() {
        let cfg = TargetListConfig::default();

        let abbreviated = VellumGuiApp::status_abbreviation("awkward", &cfg);
        assert_eq!(abbreviated, "awk");
    }

    #[test]
    fn test_normalize_entity_id_strips_hash_prefix() {
        assert_eq!(VellumGuiApp::normalize_entity_id("#12345"), "12345");
        assert_eq!(VellumGuiApp::normalize_entity_id("12345"), "12345");
    }

    #[test]
    fn test_room_component_entries_trims_and_filters_empty() {
        let component = vec![
            vec![TextSegment::plain("  north  ")],
            vec![TextSegment::plain(""), TextSegment::plain(" ")],
            vec![TextSegment::plain("south")],
        ];

        let entries = VellumGuiApp::room_component_entries(Some(&component));

        assert_eq!(entries, vec!["north".to_string(), "south".to_string()]);
    }

    #[test]
    fn test_room_component_lines_preserve_segments_and_set_stream() {
        let component = vec![vec![TextSegment::plain("Room text")]];

        let lines = VellumGuiApp::room_component_lines(Some(&component));

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].stream, "room");
        assert_eq!(lines[0].segments[0].text, "Room text");
    }

    #[test]
    fn test_format_target_line_respects_status_position() {
        let mut cfg = TargetListConfig::default();
        let creature = Creature {
            name: "a goblin".to_string(),
            noun: Some("goblin".to_string()),
            id: "#101".to_string(),
            status: Some("stunned".to_string()),
        };

        cfg.status_position = "start".to_string();
        let start = VellumGuiApp::format_target_line(&creature, &cfg);
        assert_eq!(start, "[stu] a goblin");

        cfg.status_position = "end".to_string();
        let end = VellumGuiApp::format_target_line(&creature, &cfg);
        assert_eq!(end, "a goblin [stu]");
    }

    #[test]
    fn test_format_player_line_includes_both_statuses() {
        let mut cfg = TargetListConfig::default();
        cfg.status_position = "start".to_string();
        let player = Player {
            name: "Nisugi".to_string(),
            id: "-42".to_string(),
            primary_status: Some("stunned".to_string()),
            secondary_status: Some("prone".to_string()),
        };

        let start = VellumGuiApp::format_player_line(&player, &cfg);
        assert_eq!(start, "[stu] [prn] Nisugi");

        cfg.status_position = "end".to_string();
        let end = VellumGuiApp::format_player_line(&player, &cfg);
        assert_eq!(end, "Nisugi [stu] [prn]");
    }

    #[test]
    fn test_should_filter_target_creature_filters_dead_and_excluded_nouns() {
        let cfg = TargetListConfig::default();
        let dead_creature = Creature {
            name: "a dead goblin".to_string(),
            noun: Some("goblin".to_string()),
            id: "#1".to_string(),
            status: Some("dead".to_string()),
        };
        let body_part_creature = Creature {
            name: "an arm".to_string(),
            noun: Some("arm".to_string()),
            id: "#2".to_string(),
            status: None,
        };

        assert!(VellumGuiApp::should_filter_target_creature(
            &dead_creature,
            &cfg
        ));
        assert!(VellumGuiApp::should_filter_target_creature(
            &body_part_creature,
            &cfg
        ));
    }

    #[test]
    fn test_should_filter_target_creature_keeps_live_creatures() {
        let cfg = TargetListConfig::default();
        let live_creature = Creature {
            name: "a forest troll".to_string(),
            noun: Some("troll".to_string()),
            id: "#3".to_string(),
            status: Some("stunned".to_string()),
        };

        assert!(!VellumGuiApp::should_filter_target_creature(
            &live_creature,
            &cfg
        ));
    }

}
