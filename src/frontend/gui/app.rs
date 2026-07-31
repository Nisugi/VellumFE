use super::persistence::{
    is_valid_layout_name, list_named_layouts, load_layout, load_named_layout, save_layout,
    save_named_layout, FontRef, GuiLayoutFileV1, GuiUiSettings, MainViewportState, TabGroup,
    TabSettings, TabSettingsEntry, ViewportState, ZoneSeparatorStyle,
};
use super::skin;
use super::{TabId, TabKey};
use crate::cmdlist::CmdList;
use crate::config::{AppKeybinds, Config, KeyBindAction, TargetListConfig};
use crate::core::AppCore;
use crate::data::{
    InputMode, LinkData, PopupMenu, PopupMenuItem, StyledLine, TabbedTextContent, TextContent,
    TextSegment,
    WidgetType, WindowContent, WindowState,
};
use crate::network::{LichConnection, RawLogger, ServerMessage};
use anyhow::{anyhow, Context, Result};
use eframe::egui;
use eframe::egui::{Color32, Pos2, Rect, RichText, Vec2, ViewportBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

mod borders;
mod color_emoji;
mod detached;
mod map_explorer;
mod dialogs;
mod dock;
mod editors;
#[cfg(feature = "gamepad")]
mod gamepad;
mod interact;
mod menus;
mod snap;
mod status_icons;
mod theme;
mod webui_panel;
mod widgets;
mod zones;

use detached::{DetachedMenuState, DetachedWindowState};
use dock::{DockStateSnapshot, MainWindowRectSnapshot};
use menus::GuiWindowMenuRequest;
use zones::{
    GuiShellZone, GuiWindowMoveState, GuiZoneDragState, GuiZoneWindowRect, PendingZoneSnapshot,
    ShellLayoutSnapshot, TabZoneSnapshot,
};

const INITIAL_LAYOUT_WIDTH: u16 = 160;
const INITIAL_LAYOUT_HEIGHT: u16 = 50;
const MAX_RENDERED_LINES: usize = 10_000;
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

/// Resolved per-window sizing values passed into content renderers.
#[derive(Clone, Debug)]
pub(super) struct WidgetRenderSettings {
    /// Effective text size for this window (per-tab override or global).
    text_size: f32,
    /// Mini map zoom override (px per cell).
    map_zoom: Option<f32>,
    /// Effective font family for this window's proportional text.
    font_family: egui::FontFamily,
    /// Height of one active-effect bar row.
    effects_bar_height: f32,
    /// Corner radius for progress bars; 0 = square.
    bar_corner_radius: f32,
    /// Swap bar text to light/dark when the configured color is unreadable
    /// against the fill.
    auto_contrast_bar_text: bool,
    /// Wrap long lines at the window edge; false = one row per line with
    /// horizontal scrolling (useful for inventory/container lists).
    wrap_text: bool,
    /// Vitals window layout and bar selection (global config).
    vitals: super::persistence::VitalsConfig,
    /// Skin background image for this window, if the active skin defines
    /// one. Resolved here so detached viewports can paint it too.
    background: Option<skin::ResolvedBackground>,
    /// Widget sprite art from the active skin (status icons, compass,
    /// injury doll); None = draw the built-in vector graphics.
    skin_art: Option<std::sync::Arc<skin::SkinWidgetArt>>,
    /// Current command-input buffer, only for command-input windows. Render
    /// paths are `&self`; edits flow back via `CommandInputEcho`.
    command_input_seed: Option<String>,
    /// Command-input windows with a hidden title bar show a small grip
    /// gutter: the TextEdit owns every drag in the body, so without it the
    /// window would have no drag surface at all.
    command_input_drag_gutter: bool,
    /// Hand widget icon box size in points (ui_settings.hand_icon_size).
    hand_icon_size: f32,
    /// Inactive status icons render their grayscale twin instead of the
    /// alpha dim: the global toggle plus per-indicator exceptions
    /// (ui_settings.status_icons.gray_inactive / gray_overrides).
    gray_inactive_icons: bool,
    gray_icon_overrides: std::collections::HashMap<String, bool>,
    /// Doll art renders its grayscale twins (ui_settings.doll_grayscale).
    doll_grayscale: bool,
}

/// Stable widget id for the command-input TextEdit, wherever it renders
/// (docked window, detached viewport, or the fallback bottom panel). Focus
/// routing and cursor placement key off this id.
pub(super) const COMMAND_INPUT_EDIT_ID: &str = "gui_command_input_edit";

/// Outcome of rendering the command-input widget inside a `&self` render
/// path: buffer edits and key events are stashed in egui temp data and
/// drained once per frame by the app update loop, which owns the state.
#[derive(Clone, Default)]
pub(super) struct CommandInputEcho {
    /// New buffer contents, when edited this frame.
    text: Option<String>,
    submit: bool,
    history_prev: bool,
    history_next: bool,
}

impl CommandInputEcho {
    pub(super) fn id() -> egui::Id {
        egui::Id::new("gui_command_input_echo")
    }

    fn is_empty(&self) -> bool {
        self.text.is_none() && !self.submit && !self.history_prev && !self.history_next
    }
}

impl WidgetRenderSettings {
    /// The proportional font for this window's text.
    fn font_id(&self) -> egui::FontId {
        egui::FontId {
            size: self.text_size,
            family: self.font_family.clone(),
        }
    }
}

/// Per-frame interactions collected while rendering zone surfaces.
/// Window management commands (move/hide/detach/etc.) do not flow through
/// here; they are applied via `apply_window_menu_command`.
#[derive(Default)]
struct GuiWindowActions {
    link_clicks: Vec<GuiLinkClick>,
    window_menu_request: Option<GuiWindowMenuRequest>,
    /// WebUI windows whose title-bar close button was clicked this frame
    /// (window names); the app removes them and unsubscribes their pages.
    webui_closes: Vec<String>,
}

impl GuiWindowActions {
    fn merge(&mut self, other: GuiWindowActions) {
        self.link_clicks.extend(other.link_clicks);
        if let Some(request) = other.window_menu_request {
            self.window_menu_request = Some(request);
        }
        self.webui_closes.extend(other.webui_closes);
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

#[derive(Clone, Debug)]
struct GuiLinkClick {
    link_data: LinkData,
    click_pos: (u16, u16),
}

pub struct VellumGuiApp {
    app_core: AppCore,
    _runtime: tokio::runtime::Runtime,
    command_tx: mpsc::UnboundedSender<String>,
    server_rx: mpsc::Receiver<ServerMessage>,
    /// Commands typed on remote web clients (empty when web is disabled).
    remote_rx: mpsc::UnboundedReceiver<crate::core::remote::RemoteEvent>,
    network_handle: Option<tokio::task::JoinHandle<()>>,
    command_input: String,
    /// Input-bar history, newest first (same file and semantics as the
    /// TUI: ~/.vellum-fe/<profile>/history.txt, deduped, capped).
    command_history: std::collections::VecDeque<String>,
    /// Some(i) while browsing history with the arrow keys.
    history_pos: Option<usize>,
    /// The in-progress text stashed when browsing starts.
    history_draft: String,
    close_requested: bool,
    detached_tabs: HashMap<TabKey, DetachedWindowState>,
    /// Map Explorer native window (separate OS viewport).
    map_explorer: map_explorer::MapExplorerState,
    detached_context_menu: Option<DetachedMenuState>,
    /// Which detached tab's viewport hosts the game popup menus. The menu
    /// stack renders inside that OS window (at its local click coords);
    /// None means the root window hosts them.
    popup_menu_host: Option<TabKey>,
    available_tabs: HashMap<TabKey, GuiTab>,
    hidden_tabs: HashSet<TabKey>,
    main_window_rects: HashMap<TabKey, [f32; 4]>,
    /// Legacy sidebar stacks: desired empty space above each docked
    /// window. Read once by `bake_sidebar_stack`, which converts the
    /// stack into free-placement rects and drains these entries.
    sidebar_gap_above: HashMap<TabKey, f32>,
    /// Sidebars whose windows are free-placement rects. A zone missing
    /// here bakes its legacy gap stack on its first render pass; the set
    /// persists in the layout snapshot so a bake can never re-run on a
    /// freely rearranged sidebar.
    migrated_sidebar_zones: HashSet<GuiShellZone>,
    last_center_window_rects: HashMap<TabKey, [f32; 4]>,
    tab_zones: HashMap<TabKey, GuiShellZone>,
    /// Zone prefs for windows that aren't live tabs yet (hidden / never
    /// added), keyed by window name; seeds tab_zones on materialize.
    pending_zones: HashMap<String, GuiShellZone>,
    no_title_tabs: HashSet<TabKey>,
    shell_layout: ShellLayoutSnapshot,
    layout_profile: String,
    layout_character: String,
    /// Dimensions passed to `AppCore::init_windows`; new core windows
    /// (containers, dialog-driven additions) are positioned in this space.
    core_layout_size: (u16, u16),
    layout_dirty: bool,
    layout_dirty_since: Option<Instant>,
    applied_theme_id: Option<String>,
    current_theme: crate::theme::AppTheme,
    /// Active skin graphics (ui_settings.active_skin); reloaded when it changes.
    skin_state: skin::SkinState,
    ui_font: FontRef,
    fonts_applied: bool,
    /// Named font families actually registered with egui; a per-tab font
    /// that failed to load is absent and falls back to Proportional
    /// (an unbound FontFamily::Name panics inside egui).
    registered_font_families: HashSet<String>,
    /// Families passed to `ctx.set_fonts` this frame. egui only installs new
    /// font definitions at the next `begin_pass`, so these must not enter
    /// `registered_font_families` until the following frame — using a family
    /// in the same frame it was registered panics inside epaint.
    pending_font_families: Option<HashSet<String>>,
    /// Numpad keybind names last pushed to eframe via `set_numpad_capture_keys`;
    /// `None` until the first sync so startup always pushes the initial set.
    numpad_capture_keys: Option<HashSet<String>>,
    /// Gamepad context; None when init failed or the feature is disabled.
    #[cfg(feature = "gamepad")]
    gamepad: Option<gilrs::Gilrs>,
    /// Left-stick compass sector currently deflected (0=n..7=nw); None at
    /// center. Movement sends on sector *change* with hysteresis.
    #[cfg(feature = "gamepad")]
    gp_stick_sector: Option<usize>,
    /// Right-stick four-way direction currently deflected (interact-mode
    /// cycling); None at center. Steps on direction *change*.
    #[cfg(feature = "gamepad")]
    gp_right_dir: Option<gamepad::FourWay>,
    /// Radial wheel state while the wheel button is held: which named
    /// wheel, the folder path descended so far, and the aimed slice.
    /// Firing happens on release.
    #[cfg(feature = "gamepad")]
    gp_wheel: Option<gamepad::WheelUi>,
    /// A leaf already fired during this hold of the wheel button; the
    /// wheel stays closed (and release fires nothing) until a fresh hold,
    /// so one hold never fires twice.
    #[cfg(feature = "gamepad")]
    gp_wheel_fired: bool,
    /// When the wheel last dispatched a command; a repeat fire inside
    /// [controller_tuning] fire_debounce_ms is suppressed.
    #[cfg(feature = "gamepad")]
    gp_wheel_last_fire: Option<std::time::Instant>,
    /// Set when a wheel closes while the aim stick is still deflected: the
    /// stick's normal function (scroll / interact cycle) stays suppressed
    /// until it returns to center once, so releasing the wheel can't also
    /// scroll or cycle from the leftover deflection.
    #[cfg(feature = "gamepad")]
    gp_aim_recenter_needed: bool,
    /// Binding-legend overlay visibility (controller_overlay toggles it).
    #[cfg(feature = "gamepad")]
    gp_overlay: bool,
    /// Live rumble effects: gilrs stops an effect when dropped, so each
    /// stays here until its expiry.
    #[cfg(feature = "gamepad")]
    gp_rumble: Vec<(gilrs::ff::Effect, std::time::Instant)>,
    ui_settings: GuiUiSettings,
    tab_settings: HashMap<TabKey, TabSettings>,
    /// Windows locked together; each group renders as one window in the
    /// leader's (first member's) slot.
    tab_groups: Vec<TabGroup>,
    /// Zoom factor pushed to egui at startup; afterwards egui owns it
    /// (Ctrl+= / Ctrl+- / Ctrl+0) and we persist changes back.
    zoom_applied: bool,
    /// Login music is armed when the first server data arrives — the
    /// connection actually being established — not when the window opens.
    startup_music_pending: bool,
    /// Deadline for delayed startup music ([sound] startup_music_delay_ms,
    /// counted from first server data); None once played or when off. The
    /// player is !Send, so the frame loop fires this instead of a timer
    /// thread — same reasoning as the TUI runtime's deferred deadline.
    startup_music_at: Option<std::time::Instant>,
    /// Title font size currently applied to the egui style; None forces
    /// a re-apply on the next frame.
    applied_title_font_size: Option<f32>,
    /// Spacing density currently applied to the egui style.
    applied_density: Option<f32>,
    /// Window frame corner radius currently applied to the egui visuals;
    /// also reset after a theme switch, which rebuilds the visuals.
    applied_window_corner_radius: Option<f32>,
    settings_editor: Option<editors::SettingsEditorState>,
    highlight_editor: Option<editors::HighlightEditorState>,
    keybind_editor: Option<editors::KeybindEditorState>,
    menu_keybind_editor: Option<editors::MenuKeybindEditorState>,
    #[cfg(feature = "gamepad")]
    controller_editor: Option<editors::ControllerEditorState>,
    hotbar_editor: Option<editors::HotbarEditorState>,
    hand_icons_editor: Option<editors::HandIconsEditorState>,
    colors_editor: Option<editors::ColorsEditorState>,
    theme_browser: Option<editors::ThemeBrowserState>,
    theme_editor: Option<editors::ThemeEditorState>,
    indicator_templates_editor: Option<editors::IndicatorTemplatesEditorState>,
    window_editor: Option<editors::WindowEditorState>,
    custom_windows_editor: Option<editors::CustomWindowsEditorState>,
    known_windows_editor: Option<editors::KnownWindowsEditorState>,
    sorter_editor: Option<editors::SorterEditorState>,
    doll_calibration: Option<editors::DollCalibrationState>,
    /// Editor window Id to raise to the top on the next frame. Set when a
    /// settings command (`.controller`, `.settings`, …) is re-issued while
    /// its editor is already open, so the command surfaces the buried
    /// window instead of silently rebuilding (and wiping) its state.
    pending_editor_raise: Option<egui::Id>,
    search_bar_needs_focus: bool,
    /// Cached search-bar match count: (lowercased query, content fingerprint, count).
    search_match_cache: Option<(String, u64, usize)>,
    /// Fingerprint of the window set backing `available_tabs`; refresh is
    /// skipped while it is unchanged.
    available_tabs_fingerprint: Option<u64>,
    command_input_id: Option<egui::Id>,
    repaint_ctx: std::sync::Arc<std::sync::Mutex<Option<egui::Context>>>,
    layout_save_tx: Option<std::sync::mpsc::Sender<GuiLayoutFileV1>>,
    layout_save_worker: Option<std::thread::JoinHandle<()>>,
    window_context_menu: Option<GuiWindowMenuRequest>,
    /// Move mode (right-click menu → Move Window): the window follows the
    /// cursor until a click places it or Esc cancels.
    window_move_state: Option<GuiWindowMoveState>,
    /// True on the frame the window context menu was opened. The opening
    /// right-click is still "a click" that frame, and near screen edges the
    /// menu area gets shifted to stay on screen, putting the click position
    /// outside the menu rect — without this guard the close-on-click-outside
    /// check would dismiss the menu on the same frame it appeared.
    window_context_menu_just_opened: bool,
    zone_drag_state: Option<GuiZoneDragState>,
    hand_resize_tab: Option<TabKey>,
    /// Zone window whose size pin is relaxed for the CURRENT press.
    /// Latched when a press starts on/near the window and held until the
    /// mouse releases: a shrink drag moves the grabbed edge away from the
    /// press origin, so re-testing the origin against the current rect
    /// every frame would re-pin the size mid-drag and stall the resize.
    zone_engaged_tab: Option<TabKey>,
    /// Pointer-true rect of the zone window being dragged/resized, so
    /// snapping stays escapable (see `snap.rs`); None outside a drag.
    zone_snap_drag: Option<snap::ZoneSnapDrag>,
    /// Snaps engaged this frame, drawn as guides by the owning zone's pass.
    zone_snap_guides: Vec<snap::SnapGuide>,
    /// `.snapdebug`: per-frame snap trace into vellum-fe.log. Runtime
    /// toggle, deliberately not persisted.
    snap_debug: bool,
    last_monitor_bounds: Option<[f32; 4]>,
    /// Latest main OS window geometry, persisted so the next launch opens
    /// at the same size (per-window rects are saved against this geometry).
    main_viewport_state: Option<MainViewportState>,
    /// Lich WebUI bridge socket (Some while a session's WebUI is connected).
    webui_bridge: Option<crate::webui::WebUiHandle>,
    /// Bridge events, forwarded through a repaint-waking hop like server_rx.
    webui_rx: Option<mpsc::UnboundedReceiver<crate::webui::WebUiEvent>>,
    /// Pages currently registered on the connected Lich session.
    webui_pages: Vec<crate::data::webui::WebUiPageDescriptor>,
    /// Actions deferred until the handshake/hello completes.
    webui_pending: Vec<WebUiPendingAction>,
    /// True while direct-connected (no Lich): `;ui` commands would reach the
    /// game itself, so the bridge is unavailable.
    is_direct_connection: bool,
    /// Ensures the layout-driven auto-handshake fires once per connect.
    webui_handshake_sent: bool,
    /// (host, port, auth token) of the connected WebUI server, for /files/
    /// image fetches. Host comes from the handshake url (loopback for a
    /// local Lich, a LAN address for a containerized one). The token is
    /// script-level power: never log it.
    webui_endpoint: Option<(String, u16, String)>,
    /// Raw bridge event sender; image fetch tasks report through it.
    webui_event_tx: Option<mpsc::UnboundedSender<crate::webui::WebUiEvent>>,
    /// Image srcs with a fetch task in flight (dedupes re-queues).
    webui_fetches_inflight: HashSet<String>,
}

/// What to do once the Lich WebUI bridge says hello.
#[derive(Clone, Debug, PartialEq)]
enum WebUiPendingAction {
    /// Open the page-picker popup menu.
    Picker,
    /// Subscribe and open a panel for this page id.
    Open(String),
}

impl VellumGuiApp {
    pub fn new(
        mut app_core: AppCore,
        direct: Option<crate::network::DirectConnectConfig>,
        login_key: Option<String>,
        initial_width: f32,
        initial_height: f32,
    ) -> Result<Self> {
        let core_layout_size = (initial_width.max(1.0) as u16, initial_height.max(1.0) as u16);
        app_core.init_windows(core_layout_size.0, core_layout_size.1);
        let is_direct_connection = direct.is_some();

        let runtime = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

        // Start the web frontend sidecar if enabled (off by default); it
        // runs on this GUI-owned runtime.
        let web_event_rx = if app_core.config.web.enabled {
            let _guard = runtime.enter();
            let session_label = app_core
                .config
                .connection
                .character
                .clone()
                .or_else(|| app_core.config.character.clone())
                .unwrap_or_else(|| "default".to_string());
            let (sink, event_rx) =
                crate::frontend::web::start(&app_core.config.web, session_label);
            app_core.enable_remote(sink);
            Some(event_rx)
        } else {
            None
        };

        let (server_tx, mut network_rx) =
            mpsc::channel::<ServerMessage>(crate::network::SERVER_CHANNEL_CAPACITY);
        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();

        // Forward server messages through an intermediary that wakes the egui
        // event loop, so the idle repaint interval can stay slow without
        // adding latency to incoming game text.
        let repaint_ctx: std::sync::Arc<std::sync::Mutex<Option<egui::Context>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let (forward_tx, server_rx) =
            mpsc::channel::<ServerMessage>(crate::network::SERVER_CHANNEL_CAPACITY);
        let waker_ctx = std::sync::Arc::clone(&repaint_ctx);
        runtime.spawn(async move {
            while let Some(message) = network_rx.recv().await {
                if forward_tx.send(message).await.is_err() {
                    break;
                }
                if let Some(ctx) = waker_ctx.lock().ok().and_then(|slot| slot.clone()) {
                    ctx.request_repaint();
                }
            }
        });

        // Same waking hop for remote web-client commands: forward them and
        // wake the event loop so phone input isn't stuck waiting for the
        // next idle repaint. With web disabled the sender drops immediately
        // and the receiver just sits empty.
        let (remote_forward_tx, remote_rx) =
            mpsc::unbounded_channel::<crate::core::remote::RemoteEvent>();
        if let Some(mut event_rx) = web_event_rx {
            let waker_ctx = std::sync::Arc::clone(&repaint_ctx);
            runtime.spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    if remote_forward_tx.send(event).is_err() {
                        break;
                    }
                    if let Some(ctx) = waker_ctx.lock().ok().and_then(|slot| slot.clone()) {
                        ctx.request_repaint();
                    }
                }
            });
        }

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

        // Layout writer thread: disk I/O for debounced saves happens off the
        // UI thread; writes stay sequential because one worker owns them.
        let (layout_save_tx, layout_save_rx) = std::sync::mpsc::channel::<GuiLayoutFileV1>();
        let worker_profile = layout_profile.clone();
        let worker_character = layout_character.clone();
        let layout_save_worker = std::thread::spawn(move || {
            while let Ok(layout) = layout_save_rx.recv() {
                Self::write_layout_now(&layout, &worker_profile, &worker_character);
            }
        });

        let persisted_layout = load_layout(&layout_profile, &layout_character).ok();
        let available_tabs = Self::collect_available_tabs(&app_core);
        let dock::RestoredLayoutState {
            hidden_tabs,
            main_window_rects,
            sidebar_gap_above,
            migrated_sidebar_zones,
            tab_zones,
            pending_zones,
            no_title_tabs,
            shell_layout,
            tab_groups,
            detached_tabs,
            ui_font,
            ui_settings,
            tab_settings,
            main_viewport: main_viewport_state,
        } = Self::restore_layout_state(persisted_layout.as_ref(), &available_tabs);

        // The active skin lives in the layout now; GUI files from before
        // that (or fresh characters) seed it from the config mirror once.
        let mut ui_settings = ui_settings;
        let mut seeded_active_skin = false;
        if ui_settings.active_skin.is_none() && app_core.config.active_skin.is_some() {
            ui_settings.active_skin = app_core.config.active_skin.clone();
            seeded_active_skin = true;
        }

        // Legacy GUI files stored per-window text size/font/wrap in
        // TabSettings; those now live on the shared layout defs. Migrate
        // once: marking both stores dirty persists the move on both sides.
        let mut tab_settings = tab_settings;
        let (migrated_layout, migrated_gui) = Self::migrate_tab_settings_to_layout(
            &mut tab_settings,
            &mut app_core.layout,
            |key| available_tabs.get(key).map(|tab| tab.window_name.clone()),
        );
        if migrated_layout {
            app_core.schedule_layout_autosave();
        }

        let command_history =
            Self::load_command_history(app_core.config.character.as_deref());

        // Login music plays when the game connection is established (first
        // server data), not when the login screen opens — the frame loop
        // arms the deadline on first receive.
        let startup_music_pending =
            app_core.config.sound.startup_music && app_core.sound_player.is_some();

        Ok(Self {
            app_core,
            _runtime: runtime,
            command_tx,
            server_rx,
            remote_rx,
            network_handle: Some(network_handle),
            command_input: String::new(),
            command_history,
            history_pos: None,
            history_draft: String::new(),
            close_requested: false,
            detached_tabs,
            map_explorer: Default::default(),
            detached_context_menu: None,
            popup_menu_host: None,
            available_tabs,
            hidden_tabs,
            main_window_rects,
            sidebar_gap_above,
            migrated_sidebar_zones,
            last_center_window_rects: HashMap::new(),
            tab_zones,
            pending_zones,
            no_title_tabs,
            shell_layout,
            layout_profile,
            layout_character,
            core_layout_size,
            // Migration emptied legacy TabSettings fields (and may have
            // seeded the layout's active_skin from config); rewrite the
            // GUI file so both stick.
            layout_dirty: migrated_gui || seeded_active_skin,
            layout_dirty_since: None,
            applied_theme_id: None,
            current_theme: crate::theme::AppTheme::default(),
            skin_state: skin::SkinState::default(),
            ui_font,
            fonts_applied: false,
            registered_font_families: HashSet::new(),
            pending_font_families: None,
            numpad_capture_keys: None,
            #[cfg(feature = "gamepad")]
            gamepad: gilrs::Gilrs::new()
                .inspect_err(|e| tracing::warn!("gamepad init failed: {}", e))
                .ok(),
            #[cfg(feature = "gamepad")]
            gp_stick_sector: None,
            #[cfg(feature = "gamepad")]
            gp_right_dir: None,
            #[cfg(feature = "gamepad")]
            gp_wheel: None,
            #[cfg(feature = "gamepad")]
            gp_wheel_fired: false,
            #[cfg(feature = "gamepad")]
            gp_wheel_last_fire: None,
            #[cfg(feature = "gamepad")]
            gp_aim_recenter_needed: false,
            #[cfg(feature = "gamepad")]
            gp_overlay: false,
            #[cfg(feature = "gamepad")]
            gp_rumble: Vec::new(),
            ui_settings,
            tab_settings,
            tab_groups,
            zoom_applied: false,
            startup_music_pending,
            startup_music_at: None,
            applied_title_font_size: None,
            applied_density: None,
            applied_window_corner_radius: None,
            settings_editor: None,
            highlight_editor: None,
            keybind_editor: None,
            menu_keybind_editor: None,
            #[cfg(feature = "gamepad")]
            controller_editor: None,
            hotbar_editor: None,
            hand_icons_editor: None,
            colors_editor: None,
            theme_browser: None,
            theme_editor: None,
            indicator_templates_editor: None,
            window_editor: None,
            custom_windows_editor: None,
            known_windows_editor: None,
            sorter_editor: None,
            doll_calibration: None,
            pending_editor_raise: None,
            search_bar_needs_focus: false,
            search_match_cache: None,
            available_tabs_fingerprint: None,
            // Fixed id: the TextEdit uses it wherever it renders, so focus
            // routing and cursor placement survive docking moves.
            command_input_id: Some(egui::Id::new(COMMAND_INPUT_EDIT_ID)),
            repaint_ctx,
            layout_save_tx: Some(layout_save_tx),
            layout_save_worker: Some(layout_save_worker),
            window_context_menu: None,
            window_move_state: None,
            window_context_menu_just_opened: false,
            zone_drag_state: None,
            hand_resize_tab: None,
            zone_engaged_tab: None,
            zone_snap_drag: None,
            zone_snap_guides: Vec::new(),
            snap_debug: false,
            last_monitor_bounds: None,
            main_viewport_state,
            webui_bridge: None,
            webui_rx: None,
            webui_pages: Vec::new(),
            webui_pending: Vec::new(),
            is_direct_connection,
            webui_handshake_sent: false,
            webui_endpoint: None,
            webui_event_tx: None,
            webui_fetches_inflight: HashSet::new(),
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
        // Canonical windows first so they claim singleton keys (Targets,
        // Room, …) ahead of user-added "custom-*" duplicates, which fall
        // back to name-keyed tabs below instead of hijacking the slot.
        keys.sort_by_key(|name| (name.starts_with("custom-"), name.clone()));

        let mut tabs = HashMap::new();
        for name in keys {
            let Some(window) = app_core.ui_state.windows.get(&name) else {
                continue;
            };

            let Some(mut tab_key) = Self::tab_key_for_window(&name, window) else {
                continue;
            };
            // A second window of a singleton type would silently lose the
            // entry race and never get a tab (invisible, unlisted). Key
            // extras by window name so every window stays reachable.
            if tabs.contains_key(&tab_key) {
                tab_key = TabKey::WindowByName { id: name.clone() };
            }

            // The main story window keeps its canonical title regardless of the
            // layout's window name (legacy layouts call it "main"/"primary").
            let title = if tab_key == TabKey::TextMain {
                tab_key.default_title()
            } else {
                window.name.clone()
            };
            tabs.entry(tab_key.clone()).or_insert_with(|| GuiTab {
                id: TabId::with_title(tab_key, title),
                window_name: name.clone(),
            });
        }

        tabs
    }

    fn tab_key_for_window(name: &str, window: &WindowState) -> Option<TabKey> {
        let key = match window.widget_type {
            WidgetType::Spacer => return None,
            WidgetType::CommandInput => TabKey::CommandInput,
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
            WidgetType::MiniVitals => TabKey::Vitals,
            WidgetType::Progress => {
                // Legacy layouts use a Progress-typed window named "vitals"
                // for the multi-bar cluster; standalone bars (stance, single
                // health/mana bars) each get their own tab.
                if name.eq_ignore_ascii_case("vitals") || name.eq_ignore_ascii_case("minivitals") {
                    TabKey::Vitals
                } else {
                    TabKey::ProgressBar {
                        id: name.to_string(),
                    }
                }
            }
            WidgetType::Countdown => TabKey::Countdown {
                id: name.to_string(),
            },
            WidgetType::Compass => TabKey::Compass,
            WidgetType::Map => TabKey::Map,
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
            WidgetType::WebUi => {
                // Key on the bound page id, not the window name, so a WebUI
                // panel never shares TabByName's keyspace with text windows.
                let page = match &window.content {
                    WindowContent::WebUi(content) => content.page_id.clone(),
                    _ => name.to_string(),
                };
                TabKey::WebUi { page }
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
            | WindowContent::Reserve(content)
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

    /// Order-independent hash of everything tab identity derives from:
    /// window key, display title, widget type, and main-stream status.
    /// Allocation-free, so the per-frame no-change path stays cheap.
    fn available_tabs_fingerprint(app_core: &AppCore) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut acc = 0u64;
        for (name, window) in &app_core.ui_state.windows {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            name.hash(&mut hasher);
            window.name.hash(&mut hasher);
            std::mem::discriminant(&window.widget_type).hash(&mut hasher);
            Self::is_main_stream_window(name, window).hash(&mut hasher);
            acc = acc.wrapping_add(hasher.finish());
        }
        acc
    }

    fn refresh_available_tabs_if_needed(&mut self) {
        let fingerprint = Self::available_tabs_fingerprint(&self.app_core);
        if self.available_tabs_fingerprint == Some(fingerprint) {
            return;
        }
        self.available_tabs_fingerprint = Some(fingerprint);

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
        for (key, tab) in &self.available_tabs {
            if !self.tab_zones.contains_key(key) {
                // A pending zone pref (Windows-window dropdown set while
                // the window was hidden) beats the widget default.
                let zone = self
                    .pending_zones
                    .get(&tab.window_name)
                    .copied()
                    .unwrap_or_else(|| Self::default_zone_for_tab_key(key));
                self.tab_zones.insert(key.clone(), zone);
            }
        }
        self.prune_detached_tabs();
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
                        timestamp: None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// One flowing line like "Also here: Fraemen, Zugu" with each entry a
    /// clickable link. Fallback for when the styled room component is
    /// missing (e.g. state fed by Lich rather than the game stream).
    fn room_line_from_links(
        label: &str,
        entries: impl Iterator<Item = (String, Option<crate::data::LinkData>)>,
    ) -> Vec<StyledLine> {
        let mut segments = vec![TextSegment {
            text: label.to_string(),
            ..Default::default()
        }];
        let mut first = true;
        for (text, link_data) in entries {
            if !first {
                segments.push(TextSegment {
                    text: ", ".to_string(),
                    ..Default::default()
                });
            }
            first = false;
            let span_type = if link_data.is_some() {
                crate::data::SpanType::Link
            } else {
                crate::data::SpanType::Normal
            };
            segments.push(TextSegment {
                text,
                span_type,
                link_data,
                ..Default::default()
            });
        }
        if first {
            return Vec::new();
        }
        vec![StyledLine {
            segments,
            stream: "room".to_string(),
            timestamp: None,
        }]
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
        // Prefer the styled component text verbatim (natural "Obvious
        // paths:" / "Also here:" phrasing, monsterbold creatures, links);
        // synthesize an equivalent line from game state only when absent.
        let mut exits = Self::room_component_lines(self.app_core.room_components.get("room exits"));
        if exits.is_empty() {
            exits = Self::room_line_from_links(
                "Obvious exits: ",
                self.app_core.game_state.exits.iter().map(|dir| {
                    (
                        dir.clone(),
                        Some(crate::data::LinkData {
                            exist_id: "_direct_".to_string(),
                            noun: dir.clone(),
                            text: dir.clone(),
                            coord: None,
                        }),
                    )
                }),
            );
        }
        let mut players =
            Self::room_component_lines(self.app_core.room_components.get("room players"));
        if players.is_empty() {
            players = Self::room_line_from_links(
                "Also here: ",
                self.app_core.game_state.room_players.iter().map(|player| {
                    (
                        player.name.clone(),
                        Some(crate::data::LinkData {
                            exist_id: player.id.clone(),
                            noun: player.name.clone(),
                            text: player.name.clone(),
                            coord: None,
                        }),
                    )
                }),
            );
        }
        let mut objects =
            Self::room_component_lines(self.app_core.room_components.get("room objs"));
        if objects.is_empty() {
            objects = Self::room_line_from_links(
                "You also see ",
                self.app_core.game_state.room_objects.iter().map(|object| {
                    (
                        object.name.clone(),
                        Some(crate::data::LinkData {
                            exist_id: object.id.clone(),
                            noun: object.noun.clone().unwrap_or_else(|| object.name.clone()),
                            text: object.name.clone(),
                            coord: None,
                        }),
                    )
                }),
            );
        }

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

    /// Will the command-input tab render somewhere this frame — a zone
    /// surface or a detached viewport? When not, the shell shows the fixed
    /// bottom panel instead so typing is always possible.
    fn command_input_tab_rendered(&self) -> bool {
        let key = TabKey::CommandInput;
        if !self.available_tabs.contains_key(&key) || self.hidden_tabs.contains(&key) {
            return false;
        }
        if self.detached_tabs.contains_key(&key) {
            return true;
        }
        match self.zone_for_tab(&key) {
            GuiShellZone::Header => self.shell_layout.header_visible,
            GuiShellZone::Footer => self.shell_layout.footer_visible,
            GuiShellZone::LeftSidebar => !self.shell_layout.left_sidebar_collapsed,
            GuiShellZone::RightSidebar => !self.shell_layout.right_sidebar_collapsed,
            _ => true,
        }
    }

    fn hide_tab(&mut self, key: TabKey) {
        if self.hidden_tabs.insert(key) {
            self.prune_detached_tabs();
            self.layout_dirty = true;
        }
    }

    fn restore_tab(&mut self, key: TabKey) {
        if self.hidden_tabs.remove(&key) {
            self.layout_dirty = true;
        }
    }

    /// Find the live tab whose window matches `window_name` (bridges the
    /// core known-windows list, keyed by name, to the GUI zone system,
    /// keyed by TabKey). None for windows that aren't currently a live tab.
    pub(super) fn find_tab_key_by_name(&self, window_name: &str) -> Option<TabKey> {
        self.available_tabs
            .iter()
            .find(|(_, tab)| tab.window_name == window_name)
            .map(|(key, _)| key.clone())
    }


    /// Drop group members that no longer exist, groups that shrink below
    /// two members, and duplicate memberships (first group wins).
    fn sanitize_tab_groups(
        groups: Vec<TabGroup>,
        available_tabs: &HashMap<TabKey, GuiTab>,
    ) -> Vec<TabGroup> {
        let mut seen: HashSet<TabKey> = HashSet::new();
        groups
            .into_iter()
            .filter_map(|mut group| {
                group
                    .members
                    .retain(|key| available_tabs.contains_key(key) && seen.insert(key.clone()));
                (group.members.len() >= 2).then_some(group)
            })
            .collect()
    }

    /// The group a tab belongs to, if any.
    fn group_for_tab(&self, key: &TabKey) -> Option<&TabGroup> {
        self.tab_groups
            .iter()
            .find(|group| group.members.contains(key))
    }

    /// True when this tab is in a group but is not its leader (first member):
    /// such tabs render inside the leader's window, never on their own.
    fn is_grouped_follower(&self, key: &TabKey) -> bool {
        self.group_for_tab(key)
            .is_some_and(|group| group.members.first() != Some(key))
    }

    /// Remove a tab from its group, dissolving groups left with one member.
    fn ungroup_tab(&mut self, key: &TabKey) {
        if self.group_for_tab(key).is_none() {
            return;
        }
        for group in &mut self.tab_groups {
            group.members.retain(|member| member != key);
            group.merged.retain(|member| member != key);
            group.end_anchored.retain(|member| member != key);
        }
        self.tab_groups.retain(|group| group.members.len() >= 2);
        self.layout_dirty = true;
    }

    /// Add `other` to `leader`'s group (creating one if needed) and move it
    /// into the leader's zone so the group renders on one surface.
    fn group_tabs(&mut self, leader: &TabKey, other: TabKey) {
        if leader == &other {
            return;
        }
        self.ungroup_tab(&other);
        let leader_zone = self.zone_for_tab(leader);
        if let Some(index) = self
            .tab_groups
            .iter()
            .position(|group| group.members.contains(leader))
        {
            self.tab_groups[index].members.push(other.clone());
        } else {
            self.tab_groups.push(TabGroup {
                members: vec![leader.clone(), other.clone()],
                horizontal: false,
                merged: Vec::new(),
                end_anchored: Vec::new(),
            });
        }
        self.tab_zones.insert(other, leader_zone);
        self.layout_dirty = true;
    }

    /// The shared layout definition backing a tab, when one exists.
    fn layout_def_for_tab(&self, key: &TabKey) -> Option<&crate::config::WindowDef> {
        let window_name = &self.available_tabs.get(key)?.window_name;
        self.app_core
            .layout
            .windows
            .iter()
            .find(|window| window.name() == *window_name)
    }

    /// Mutate the shared layout def backing a tab and mark layout.toml
    /// modified (the same mechanism the Window Editor uses). Returns false
    /// (and changes nothing) when the tab has no layout definition.
    fn with_layout_def_for_tab(
        &mut self,
        key: &TabKey,
        mutate: impl FnOnce(&mut crate::config::WindowDef),
    ) -> bool {
        let Some(window_name) = self
            .available_tabs
            .get(key)
            .map(|tab| tab.window_name.clone())
        else {
            return false;
        };
        let Some(def) = self
            .app_core
            .layout
            .windows
            .iter_mut()
            .find(|window| window.name() == window_name)
        else {
            return false;
        };
        mutate(def);
        self.app_core.schedule_layout_autosave();
        true
    }

    /// Per-window text size override: the shared layout def first, then the
    /// legacy per-tab GUI setting (pre-migration files).
    fn text_size_override_for_tab(&self, key: &TabKey) -> Option<f32> {
        self.layout_def_for_tab(key)
            .and_then(|def| def.base().text_size)
            .or_else(|| self.tab_settings.get(key).and_then(|s| s.text_size))
    }

    /// Effective text size for a window: per-window override or the global size.
    fn effective_text_size(&self, key: &TabKey) -> f32 {
        self.text_size_override_for_tab(key)
            .unwrap_or(self.ui_settings.text_size)
            .clamp(6.0, 72.0)
    }

    /// Per-window font: the shared layout def's family name first, then the
    /// legacy per-tab GUI setting (which can also be a custom font file).
    fn font_ref_for_tab(&self, key: &TabKey) -> Option<FontRef> {
        if let Some(family) = self
            .layout_def_for_tab(key)
            .and_then(|def| def.base().font_family.clone())
        {
            return Some(FontRef::Named(family));
        }
        self.tab_settings
            .get(key)
            .map(|settings| settings.font_primary.clone())
    }

    /// Effective proportional font family for a window: the per-window font
    /// (registered as a named family during font setup) or egui's default.
    fn effective_font_family(&self, key: &TabKey) -> egui::FontFamily {
        self.font_ref_for_tab(key)
            .as_ref()
            .and_then(theme::font_ref_key)
            .filter(|font_key| self.registered_font_families.contains(font_key))
            .map(|font_key| egui::FontFamily::Name(font_key.into()))
            .unwrap_or(egui::FontFamily::Proportional)
    }

    /// Word wrap stored on the shared layout def, for widget types whose def
    /// carries a `wordwrap` field. None = this def has no such field.
    fn def_wordwrap_for_tab(&self, key: &TabKey) -> Option<bool> {
        match self.layout_def_for_tab(key)? {
            crate::config::WindowDef::Text { data, .. } => Some(data.wordwrap),
            crate::config::WindowDef::Inventory { data, .. }
            | crate::config::WindowDef::Reserve { data, .. } => Some(data.wordwrap),
            _ => None,
        }
    }

    /// Effective wrap for a window: the def's wordwrap when it has one, else
    /// the legacy per-tab GUI setting, else on.
    fn effective_wrap_text(&self, key: &TabKey) -> bool {
        self.def_wordwrap_for_tab(key).unwrap_or_else(|| {
            self.tab_settings
                .get(key)
                .map(|settings| settings.wrap_text)
                .unwrap_or(true)
        })
    }

    /// One-time migration: move legacy per-tab GUI appearance settings
    /// (text size, font, wrap) onto the shared layout window defs, where
    /// they now live. A value already present on a def wins; migrated
    /// TabSettings fields reset to their defaults either way, so a second
    /// run is a no-op. `FontRef::Custom` (a font file path, not a family
    /// name) stays in TabSettings and keeps working via the read fallback.
    /// Returns (layout_changed, gui_settings_changed).
    fn migrate_tab_settings_to_layout(
        tab_settings: &mut HashMap<TabKey, TabSettings>,
        layout: &mut crate::config::Layout,
        window_name_of: impl Fn(&TabKey) -> Option<String>,
    ) -> (bool, bool) {
        use crate::config::WindowDef;

        let mut layout_changed = false;
        let mut gui_changed = false;
        for (key, settings) in tab_settings.iter_mut() {
            let Some(name) = window_name_of(key) else {
                continue;
            };
            let Some(def) = layout.windows.iter_mut().find(|w| w.name() == name) else {
                continue;
            };
            if let Some(size) = settings.text_size.take() {
                if def.base().text_size.is_none() {
                    def.base_mut().text_size = Some(size);
                    layout_changed = true;
                }
                gui_changed = true;
            }
            if matches!(settings.font_primary, FontRef::Named(_)) {
                let font = std::mem::take(&mut settings.font_primary);
                if let FontRef::Named(family) = font {
                    if def.base().font_family.is_none() {
                        def.base_mut().font_family = Some(family);
                        layout_changed = true;
                    }
                }
                gui_changed = true;
            }
            if !settings.wrap_text {
                // wrap_text=false is the only migratable signal (true is the
                // default); only defs with a wordwrap field can hold it.
                let target = match def {
                    WindowDef::Text { data, .. } => Some(&mut data.wordwrap),
                    WindowDef::Inventory { data, .. } | WindowDef::Reserve { data, .. } => {
                        Some(&mut data.wordwrap)
                    }
                    _ => None,
                };
                if let Some(wordwrap) = target {
                    if *wordwrap {
                        *wordwrap = false;
                        layout_changed = true;
                    }
                    settings.wrap_text = true;
                    gui_changed = true;
                }
            }
        }
        (layout_changed, gui_changed)
    }

    /// Resolve the sizing values a window's content renderer needs.
    fn widget_render_settings(&self, key: &TabKey) -> WidgetRenderSettings {
        WidgetRenderSettings {
            text_size: self.effective_text_size(key),
            map_zoom: self.tab_settings.get(key).and_then(|s| s.map_zoom),
            font_family: self.effective_font_family(key),
            effects_bar_height: self.ui_settings.effects_bar_height.clamp(10.0, 60.0),
            bar_corner_radius: self.ui_settings.bar_corner_radius.clamp(0.0, 12.0),
            auto_contrast_bar_text: self.ui_settings.auto_contrast_bar_text,
            wrap_text: self.effective_wrap_text(key),
            vitals: self.ui_settings.vitals.clone(),
            background: self.available_tabs.get(key).and_then(|tab| {
                self.skin_state.background_for_with_override(
                    &tab.window_name,
                    self.tab_settings
                        .get(key)
                        .and_then(|settings| settings.background_image.as_deref())
                        .or(self.ui_settings.default_background.as_deref()),
                )
            }),
            skin_art: self.skin_state.widget_art(),
            command_input_seed: self
                .available_tabs
                .get(key)
                .and_then(|tab| self.app_core.ui_state.windows.get(&tab.window_name))
                .filter(|window| window.widget_type == WidgetType::CommandInput)
                .map(|_| self.command_input.clone()),
            command_input_drag_gutter: self
                .available_tabs
                .get(key)
                .and_then(|tab| self.app_core.ui_state.windows.get(&tab.window_name))
                .is_some_and(|window| window.widget_type == WidgetType::CommandInput)
                && self.title_bar_hidden(key),
            hand_icon_size: self.ui_settings.hand_icon_size.clamp(16.0, 48.0),
            gray_inactive_icons: self.ui_settings.status_icons.gray_inactive,
            gray_icon_overrides: self.ui_settings.status_icons.gray_overrides.clone(),
            doll_grayscale: self.ui_settings.doll_grayscale,
        }
    }

    /// Display title for a docked window: grouped leaders show all member
    /// titles joined; everything else shows its own title.
    fn window_display_title(&self, tab: &GuiTab) -> String {
        match self.group_for_tab(&tab.id.key) {
            Some(group) if group.members.first() == Some(&tab.id.key) => group
                .members
                .iter()
                .filter_map(|key| self.available_tabs.get(key))
                .map(|member| member.id.title.as_str())
                .collect::<Vec<_>>()
                .join(" + "),
            _ => tab.id.title.clone(),
        }
    }

    /// Render a window's content, or — when the window leads a group — all
    /// member contents split along the group's orientation.
    fn render_window_or_group_content(
        &self,
        ui: &mut egui::Ui,
        tab: &GuiTab,
    ) -> Option<GuiLinkClick> {
        let members: Vec<GuiTab> = match self.group_for_tab(&tab.id.key) {
            Some(group) => group
                .members
                .iter()
                .filter(|key| !self.hidden_tabs.contains(*key))
                .filter(|key| !self.detached_tabs.contains_key(*key))
                .filter_map(|key| self.available_tabs.get(key).cloned())
                .collect(),
            None => Vec::new(),
        };
        if members.len() < 2 {
            return Self::render_window_content(
                &self.app_core,
                ui,
                tab,
                self.widget_render_settings(&tab.id.key),
            );
        }
        let (horizontal, merged, end_anchored) = self
            .group_for_tab(&tab.id.key)
            .map(|group| {
                (
                    group.horizontal,
                    group.merged.clone(),
                    group.end_anchored.clone(),
                )
            })
            .unwrap_or_default();

        // Partition members into slots along the group axis: a merged
        // member joins its predecessor's slot and stacks along the
        // perpendicular axis (a column of a side-by-side group holds a
        // vertical stack; a row of a stacked group holds a side-by-side
        // run). The first member always opens a slot.
        let mut slots: Vec<Vec<GuiTab>> = Vec::new();
        for member in members {
            if !slots.is_empty() && merged.contains(&member.id.key) {
                slots.last_mut().expect("slots checked non-empty").push(member);
            } else {
                slots.push(vec![member]);
            }
        }

        let mut clicked = None;
        // Each member's screen rect, recorded so window-level drag-and-drop
        // can resolve drops to the member under the pointer instead of the
        // whole group window (e.g. left vs right hand in a hand group).
        let mut member_rects: Vec<(String, Rect)> = Vec::new();
        if horizontal {
            ui.columns(slots.len(), |columns| {
                for (column, slot) in columns.iter_mut().zip(slots.iter()) {
                    let anchored = end_anchored.contains(&slot[0].id.key);
                    self.render_group_stack(
                        column,
                        slot,
                        anchored,
                        &mut member_rects,
                        &mut clicked,
                    );
                }
            });
        } else {
            let gap = ui.spacing().item_spacing.y;
            let bar_height = ui.spacing().interact_size.y.max(16.0);
            // A row slot is as tall as its tallest fixed member; any
            // flexible member makes the whole row flexible.
            let slot_heights: Vec<Option<f32>> = slots
                .iter()
                .map(|slot| {
                    slot.iter()
                        .map(|member| self.member_natural_height(gap, bar_height, member))
                        .try_fold(0.0f32, |tallest, natural| {
                            natural.map(|height| tallest.max(height))
                        })
                })
                .collect();
            let fixed_total: f32 = slot_heights.iter().flatten().sum();
            let flexible_count = slot_heights.iter().filter(|h| h.is_none()).count() as f32;
            let total_gap = gap * (slots.len() as f32 - 1.0);
            let flex_height = if flexible_count > 0.0 {
                ((ui.available_height() - total_gap - fixed_total) / flexible_count)
                    .max(24.0)
            } else {
                0.0
            };
            let width = ui.available_width().max(1.0);
            for (slot, natural) in slots.iter().zip(&slot_heights) {
                let each_height = natural.unwrap_or(flex_height);
                if let [member] = slot.as_slice() {
                    let block = ui.push_id(&member.id.key, |ui| {
                        ui.allocate_ui(Vec2::new(width, each_height), |ui| {
                            ui.set_min_size(Vec2::new(width, each_height));
                            ui.set_max_height(each_height);
                            if let Some(click) = Self::render_window_content(
                                &self.app_core,
                                ui,
                                member,
                                self.widget_render_settings(&member.id.key),
                            ) {
                                clicked = Some(click);
                            }
                        })
                    });
                    member_rects.push((member.window_name.clone(), block.inner.response.rect));
                } else {
                    ui.allocate_ui(Vec2::new(width, each_height), |ui| {
                        ui.set_min_size(Vec2::new(width, each_height));
                        ui.set_max_height(each_height);
                        ui.columns(slot.len(), |columns| {
                            for (column, member) in columns.iter_mut().zip(slot.iter()) {
                                member_rects
                                    .push((member.window_name.clone(), column.max_rect()));
                                column.push_id(&member.id.key, |ui| {
                                    if let Some(click) = Self::render_window_content(
                                        &self.app_core,
                                        ui,
                                        member,
                                        self.widget_render_settings(&member.id.key),
                                    ) {
                                        clicked = Some(click);
                                    }
                                });
                            }
                        });
                    });
                }
            }
        }
        ui.ctx().data_mut(|data| {
            data.insert_temp(Self::group_member_rects_id(&tab.id.key), member_rects);
        });
        clicked
    }

    /// egui temp-data key for a group leader's per-member screen rects,
    /// refreshed every frame the group renders.
    fn group_member_rects_id(leader: &TabKey) -> egui::Id {
        egui::Id::new("gui_group_member_rects").with(leader)
    }

    /// Natural (fixed) height of a group member, when it has one. Compact
    /// widgets (bars, timers, hands) only ever draw one row, so they get
    /// exactly that; the leftover splits among flexible members (doll,
    /// text, ...) instead of equal N-way shares that leave dead space
    /// under each bar. None = flexible.
    fn member_natural_height(&self, gap: f32, bar_height: f32, member: &GuiTab) -> Option<f32> {
        match self
            .app_core
            .ui_state
            .windows
            .get(&member.window_name)
            .map(|window| &window.content)
        {
            Some(
                WindowContent::Progress(_)
                | WindowContent::Countdown(_)
                | WindowContent::Hand { .. },
            ) => Some(bar_height),
            Some(WindowContent::Betrayer)
                if self.app_core.game_state.betrayer.items.is_empty() =>
            {
                Some(bar_height)
            }
            Some(WindowContent::Encumbrance) => {
                let (show_bar, show_label) =
                    Self::encumbrance_flags(&self.app_core, &member.window_name);
                let rows = (show_bar as u32 + show_label as u32).max(1) as f32;
                Some(bar_height * rows + gap * (rows - 1.0))
            }
            Some(WindowContent::GS4Experience) => {
                let (level, mind, exp_bar, total, ascension) =
                    Self::gs4_experience_flags(&self.app_core, &member.window_name);
                let rows = ([level, mind, exp_bar, total, ascension]
                    .into_iter()
                    .filter(|on| *on)
                    .count()
                    .max(1)) as f32;
                Some(bar_height * rows + gap * (rows - 1.0))
            }
            Some(WindowContent::MiniVitals) => {
                use crate::frontend::gui::persistence::VitalsOrientation;
                let vitals = &self.ui_settings.vitals;
                let row = vitals.bar_height.clamp(8.0, 60.0);
                match vitals.orientation {
                    VitalsOrientation::Horizontal => Some(row),
                    VitalsOrientation::Vertical => {
                        let count = vitals.bars.len().max(1) as f32;
                        Some(row * count + gap * (count - 1.0))
                    }
                }
            }
            _ => None,
        }
    }

    /// Render one group slot's members stacked vertically (a column of a
    /// side-by-side group, or the whole body of a stacked group's slot).
    /// Fixed members get their natural height and the leftover splits
    /// among flexible ones; when every member is fixed, the leftover pads
    /// the bottom — or the top when the slot is end-anchored, so a bar
    /// stack can hug the column's bottom edge.
    fn render_group_stack(
        &self,
        ui: &mut egui::Ui,
        members: &[GuiTab],
        end_anchored: bool,
        member_rects: &mut Vec<(String, Rect)>,
        clicked: &mut Option<GuiLinkClick>,
    ) {
        let gap = ui.spacing().item_spacing.y;
        let bar_height = ui.spacing().interact_size.y.max(16.0);
        let natural_heights: Vec<Option<f32>> = members
            .iter()
            .map(|member| self.member_natural_height(gap, bar_height, member))
            .collect();
        let fixed_total: f32 = natural_heights.iter().flatten().sum();
        let flexible_count = natural_heights.iter().filter(|h| h.is_none()).count() as f32;
        let total_gap = gap * (members.len() as f32 - 1.0);
        let flex_height = if flexible_count > 0.0 {
            ((ui.available_height() - total_gap - fixed_total) / flexible_count).max(24.0)
        } else {
            0.0
        };
        if end_anchored && flexible_count == 0.0 {
            let leftover = ui.available_height() - total_gap - fixed_total;
            if leftover > 0.0 {
                ui.add_space(leftover);
            }
        }
        let width = ui.available_width().max(1.0);
        for (member, natural) in members.iter().zip(&natural_heights) {
            let each_height = natural.unwrap_or(flex_height);
            let block = ui.push_id(&member.id.key, |ui| {
                ui.allocate_ui(Vec2::new(width, each_height), |ui| {
                    ui.set_min_size(Vec2::new(width, each_height));
                    ui.set_max_height(each_height);
                    if let Some(click) = Self::render_window_content(
                        &self.app_core,
                        ui,
                        member,
                        self.widget_render_settings(&member.id.key),
                    ) {
                        *clicked = Some(click);
                    }
                })
            });
            member_rects.push((member.window_name.clone(), block.inner.response.rect));
        }
    }

    /// Set the active skin in the layout (its home — checkpoints carry it)
    /// and mirror it into config for the web doll endpoint and the
    /// non-GUI frontends.
    fn set_active_skin(&mut self, skin: Option<String>) {
        self.ui_settings.active_skin = skin.clone();
        self.layout_dirty = true;
        if self.app_core.config.active_skin != skin {
            self.app_core.config.active_skin = skin;
            self.save_config_after_skin_change();
        }
    }

    /// Bake the current live appearance — doll, compass set, status icon
    /// art, pool frames in use, per-window backgrounds — into
    /// `global/skins/<name>/skin.toml`, referencing pool paths (the image
    /// resolver falls back to the pool, so nothing is copied). The live
    /// state doesn't change: skins are a publish format, not a
    /// prerequisite. Sheet-cell icon overrides can't be expressed in a
    /// skin manifest and stay as layout overrides.
    fn compile_appearance_to_skin(&self, name: &str) -> anyhow::Result<()> {
        use toml_edit::{value, Array, DocumentMut, Item, Table};

        let mut doc = DocumentMut::new();
        let mut meta = Table::new();
        meta.insert("name", value(name));
        meta.insert(
            "description",
            value("Compiled from the live appearance (.saveskin)"),
        );
        doc.insert("meta", Item::Table(meta));

        // Status icons: the active pool set, then Image overrides on top.
        let mut icon_entries: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        if let Some(set) = &self.ui_settings.status_icons.set {
            for image in crate::config::pool::list_category("statusicons") {
                if let Some((prefix, glyph)) = image.stem().split_once('_') {
                    if prefix.eq_ignore_ascii_case(set) && !glyph.is_empty() {
                        icon_entries
                            .insert(glyph.to_ascii_lowercase(), image.pool_path.clone());
                    }
                }
            }
        }
        for (id, icon) in &self.ui_settings.status_icons.overrides {
            if let crate::data::IconRef::Image { path } = icon {
                icon_entries.insert(id.to_ascii_lowercase(), path.clone());
            }
        }
        if !icon_entries.is_empty() {
            let mut icons = Table::new();
            for (id, path) in &icon_entries {
                icons.insert(id, value(path));
            }
            doc.insert("icons", Item::Table(icons));
        }

        // Compass set (only meaningful with a rose).
        if let Some(set) = &self.ui_settings.compass_set {
            let mut entries: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();
            for image in crate::config::pool::list_category("compass") {
                if let Some((prefix, role)) = image.stem().split_once('_') {
                    if prefix.eq_ignore_ascii_case(set) && !role.is_empty() {
                        entries.insert(role.to_ascii_lowercase(), image.pool_path.clone());
                    }
                }
            }
            if let Some(rose) = entries.get("rose").cloned() {
                let mut compass = Table::new();
                compass.insert("rose", value(rose));
                for (role, path) in &entries {
                    if role != "rose" {
                        compass.insert(role, value(path));
                    }
                }
                doc.insert("compass", Item::Table(compass));
            }
        }

        // Injury doll: pool image + its sidecar calibration.
        if let Some(image) = &self.ui_settings.doll_image {
            let mut doll = Table::new();
            doll.insert("base", value(image));
            doc.insert("injury_doll", Item::Table(doll));
            let abs = crate::config::Config::global_images_dir()?.join(image);
            if let Some(sidecar) =
                crate::config::pool::read_sidecar::<crate::config::pool::DollSidecar>(&abs)
            {
                let rounded = |v: f32, places: f64| (v as f64 * places).round() / places;
                let mut anchors = Table::new();
                let mut keys: Vec<&String> = sidecar.anchors.keys().collect();
                keys.sort();
                for key in keys {
                    let [x, y] = sidecar.anchors[key];
                    let mut pair = Array::new();
                    pair.push(rounded(x, 10_000.0));
                    pair.push(rounded(y, 10_000.0));
                    anchors.insert(key, value(pair));
                }
                let mut dots = Table::new();
                dots.insert("wound_color", value(sidecar.dots.wound_color.as_str()));
                dots.insert("scar_color", value(sidecar.dots.scar_color.as_str()));
                dots.insert("opacity", value(rounded(sidecar.dots.opacity, 100.0)));
                dots.insert("diameter", value(rounded(sidecar.dots.diameter, 1_000.0)));
                let doll = doc["injury_doll"].as_table_mut().expect("just inserted");
                doll.insert("anchors", Item::Table(anchors));
                doll.insert("dots", Item::Table(dots));
            }
        }

        // Pool frames any window override references -> [frames.<stem>],
        // plus the global default frame (Settings > GUI).
        let mut wanted_frames: Vec<String> = self
            .tab_settings
            .values()
            .filter_map(|settings| settings.skin_frame.clone())
            .chain(self.ui_settings.default_frame.clone())
            .map(|frame| frame.to_ascii_lowercase())
            .filter(|frame| frame != "none")
            .collect();
        wanted_frames.sort();
        wanted_frames.dedup();
        if !wanted_frames.is_empty() {
            let mut frames = Table::new();
            frames.set_implicit(true);
            for image in crate::config::pool::list_category("frames") {
                let stem = image.stem().to_ascii_lowercase();
                if !wanted_frames.contains(&stem) {
                    continue;
                }
                let Some(sidecar) = crate::config::pool::read_sidecar::<
                    crate::config::pool::FrameSidecar,
                >(&image.abs_path) else {
                    continue;
                };
                let mut entry = Table::new();
                entry.insert("image", value(&image.pool_path));
                let mut slice = Array::new();
                for inset in sidecar.slice.insets() {
                    slice.push(inset as f64);
                }
                entry.insert("slice", value(slice));
                entry.insert("scale", value(sidecar.effective_scale() as f64));
                frames.insert(&stem, Item::Table(entry));
            }
            if !frames.is_empty() {
                doc.insert("frames", Item::Table(frames));
            }
        }

        // Per-window backgrounds -> [window.<name>.background]; the global
        // default background bakes as the skin's [window.default] entry
        // (the manifest-wide fallback window_field consults).
        let mut backgrounds: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        if let Some(background) = &self.ui_settings.default_background {
            if !background.eq_ignore_ascii_case("none") {
                backgrounds.insert("default".to_string(), background.clone());
            }
        }
        for (key, settings) in &self.tab_settings {
            let Some(background) = &settings.background_image else {
                continue;
            };
            if background.eq_ignore_ascii_case("none") {
                continue;
            }
            if let Some(tab) = self.available_tabs.get(key) {
                backgrounds.insert(tab.window_name.clone(), background.clone());
            }
        }
        if !backgrounds.is_empty() {
            let mut windows = Table::new();
            windows.set_implicit(true);
            for (window_name, path) in &backgrounds {
                let mut background = Table::new();
                background.insert("image", value(path));
                let mut per_window = Table::new();
                per_window.set_implicit(true);
                per_window.insert("background", Item::Table(background));
                windows.insert(window_name, Item::Table(per_window));
            }
            doc.insert("window", Item::Table(windows));
        }

        let root = crate::config::Config::skins_dir()?.join(name);
        std::fs::create_dir_all(&root)?;
        crate::config::write_atomic(&root.join("skin.toml"), doc.to_string())?;
        Ok(())
    }

    /// Set the injury doll override (pool-relative path), persisted in the
    /// layout and mirrored to config for the web doll endpoint. The doll
    /// switches next frame via `SkinState::apply_if_changed`.
    pub(super) fn set_doll_image(&mut self, image: Option<String>) {
        self.ui_settings.doll_image = image.clone();
        self.layout_dirty = true;
        if self.app_core.config.doll_image != image {
            self.app_core.config.doll_image = image;
            self.save_config_after_skin_change();
        }
    }

    /// Handle `action:setskin:<name>` from dot-commands or menus. "none"
    /// (or "off") disables the active skin. The switch itself happens next
    /// frame via `SkinState::apply_if_changed`.
    fn apply_skin_by_name(&mut self, name: &str) {
        if name.eq_ignore_ascii_case("none") || name.eq_ignore_ascii_case("off") {
            self.set_active_skin(None);
            self.app_core.add_system_message("Skin disabled.");
            return;
        }
        match crate::config::skins::load_manifest(name) {
            Ok(_) => {
                self.set_active_skin(Some(name.to_string()));
                self.app_core
                    .add_system_message(&format!("Skin switched to: {}", name));
            }
            Err(err) => {
                let available = crate::config::skins::list_skins();
                if available.is_empty() {
                    self.app_core.add_system_message(&format!(
                        "Cannot load skin '{}': {}. No skins installed; create one under ~/.vellum-fe/global/skins/<name>/skin.toml",
                        name, err
                    ));
                } else {
                    self.app_core.add_system_message(&format!(
                        "Cannot load skin '{}': {}. Available: {}",
                        name,
                        err,
                        available.join(", ")
                    ));
                }
            }
        }
    }

    /// Handle `action:skins`: list installed skins in the main window.
    fn list_skins_to_window(&mut self) {
        let available = crate::config::skins::list_skins();
        if available.is_empty() {
            self.app_core.add_system_message(
                "No skins installed. Create one under ~/.vellum-fe/global/skins/<name>/skin.toml",
            );
            return;
        }
        let active = self.ui_settings.active_skin.clone();
        self.app_core.add_system_message("Installed skins:");
        for name in available {
            let marker = if active.as_deref() == Some(name.as_str()) {
                " (active)"
            } else {
                ""
            };
            self.app_core
                .add_system_message(&format!("  {}{}", name, marker));
        }
        self.app_core
            .add_system_message("Use .setskin <name> to activate, .setskin none to disable.");
    }

    /// Handle `action:makeskin:<name>`: write the starter skin and tell the
    /// user how to proceed. Does not activate it — a fresh scaffold is all
    /// comments, so activating it would visibly do nothing.
    fn make_skin_scaffold(&mut self, name: &str) {
        match crate::config::skins::write_scaffold(name) {
            Ok(path) => {
                self.app_core.add_system_message(&format!(
                    "Created skin '{}' at {}",
                    name,
                    path.display()
                ));
                self.app_core.add_system_message(
                    "Edit skin.toml (sections are commented out), add images, then .setskin to activate.",
                );
            }
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Cannot create skin '{}': {}", name, err));
            }
        }
    }

    fn save_config_after_skin_change(&mut self) {
        if let Err(err) = self
            .app_core
            .config
            .save(self.app_core.config.character.as_deref())
        {
            tracing::warn!("Failed to save config after skin switch: {}", err);
        }
    }

    /// Adjust a docked window's frame when the active skin draws this
    /// window's border: drop the stroke (the nine-slice replaces it) and
    /// widen the inner margin so content clears the border art.
    /// Which sides of the skin's nine-slice frame draw for this window,
    /// as [top, right, bottom, left]. The layout def's border settings
    /// drive it — Border off (or style "none") hides the whole frame,
    /// per-side toggles hide individual rails (their corners collapse and
    /// the surviving rails extend to the window edge). Windows without a
    /// layout def draw all four.
    pub(super) fn skin_border_sides_for_tab(&self, key: &TabKey) -> [bool; 4] {
        let Some(def) = self.layout_def_for_tab(key) else {
            return [true; 4];
        };
        let base = def.base();
        if !base.show_border || base.border_style.eq_ignore_ascii_case("none") {
            return [false; 4];
        }
        let sides = &base.border_sides;
        [sides.top, sides.right, sides.bottom, sides.left]
    }

    /// The skin border this tab draws, honoring the per-window frame
    /// override (Appearance > Skin frame) stored in its tab settings,
    /// then the global default frame (Settings > GUI).
    pub(super) fn skin_border_for_tab(&self, key: &TabKey) -> Option<skin::ResolvedBorder> {
        let tab = self.available_tabs.get(key)?;
        let frame_override = self
            .tab_settings
            .get(key)
            .and_then(|settings| settings.skin_frame.as_deref())
            .or(self.ui_settings.default_frame.as_deref());
        self.skin_state
            .border_for_with_override(&tab.window_name, frame_override)
    }

    fn apply_skin_border_to_frame(
        &self,
        key: &TabKey,
        sides: [bool; 4],
        frame: &mut egui::Frame,
    ) {
        let Some(border) = self.skin_border_for_tab(key) else {
            return;
        };
        if sides == [false; 4] {
            return;
        }
        frame.stroke = egui::Stroke::NONE;
        // Square corners whenever skin art frames the window: a rounded
        // background fill would show through (or clip) the art's corners.
        frame.corner_radius = egui::CornerRadius::ZERO;
        let side = |inset: f32| (inset * border.scale).ceil().clamp(0.0, 127.0) as i8;
        let margin = &mut frame.inner_margin;
        if sides[0] {
            margin.top = margin.top.max(side(border.slice[0]));
        }
        if sides[1] {
            margin.right = margin.right.max(side(border.slice[1]));
        }
        if sides[2] {
            margin.bottom = margin.bottom.max(side(border.slice[2]));
        }
        if sides[3] {
            margin.left = margin.left.max(side(border.slice[3]));
        }
    }

    /// Paint the skin's nine-slice border over a rendered window, on the
    /// window's own layer so it moves and stacks with the window.
    fn paint_skin_border(
        &self,
        ctx: &egui::Context,
        key: &TabKey,
        sides: [bool; 4],
        response: &egui::Response,
    ) {
        if sides == [false; 4] {
            return;
        }
        if let Some(border) = self.skin_border_for_tab(key) {
            skin::paint_nine_slice(
                &ctx.layer_painter(response.layer_id),
                response.rect,
                &border,
                sides,
            );
        }
    }

    /// Per-window position/size lock: locked windows ignore drag and
    /// resize gestures in every zone; the deliberate Arrange ▸ Move Window
    /// menu action still works. THE flag is the shared layout's
    /// `WindowBase::locked` — the same one `.lockwindows`,
    /// `.lockwindow <name>`, and the TUI write — so global and per-window
    /// locks are one system across both frontends.
    pub(super) fn window_locked(&self, key: &TabKey) -> bool {
        self.available_tabs.get(key).is_some_and(|tab| {
            self.app_core
                .layout
                .windows
                .iter()
                .find(|window| window.name() == tab.window_name)
                .is_some_and(|window| window.base().locked)
        })
    }

    /// Per-window frame corner radius override (context menu); None follows
    /// the global `ui_settings.window_corner_radius` already baked into the
    /// window frame style.
    pub(super) fn corner_radius_override_for_tab(&self, key: &TabKey) -> Option<f32> {
        self.tab_settings
            .get(key)
            .and_then(|settings| settings.corner_radius)
    }

    /// Effective title bar height for a game window: per-window override,
    /// else the global setting. 0 means "auto" in both layers; None =
    /// derive from the title font (egui's default behavior).
    pub(super) fn title_bar_height_for_tab(&self, key: &TabKey) -> Option<f32> {
        let height = self
            .tab_settings
            .get(key)
            .and_then(|settings| settings.title_bar_height)
            .unwrap_or(self.ui_settings.title_bar_height);
        (height > 0.0).then(|| height.clamp(12.0, 32.0))
    }

    /// Effective title text alignment for a game window.
    pub(super) fn title_align_for_tab(&self, key: &TabKey) -> egui::Align {
        let align = self
            .tab_settings
            .get(key)
            .and_then(|settings| settings.title_bar_align.as_deref())
            .unwrap_or(&self.ui_settings.title_bar_align);
        match align {
            "left" => egui::Align::Min,
            "right" => egui::Align::Max,
            _ => egui::Align::Center,
        }
    }

    /// Apply the resolved title bar height and alignment to a game-window
    /// builder. Editor and dialog windows keep egui's standard chrome.
    pub(super) fn style_window_title_bar<'a>(
        &self,
        key: &TabKey,
        mut window: egui::Window<'a>,
    ) -> egui::Window<'a> {
        window = window.title_align(self.title_align_for_tab(key));
        if let Some(height) = self.title_bar_height_for_tab(key) {
            window = window.title_bar_height(height);
        }
        window
    }

    /// Accent (border) color for a window. Precedence: the per-window GUI
    /// accent (context menu), else the shared layout definition's
    /// border_color — so a border color set in the window editor or the
    /// TUI finally shows here too — else the theme frame (None).
    fn accent_color_for_tab(&self, key: &TabKey) -> Option<Color32> {
        if let Some(accent) = self
            .tab_settings
            .get(key)
            .and_then(|settings| settings.accent_color.as_deref())
            .and_then(widgets::parse_hex_color)
        {
            return Some(accent);
        }
        let window_name = &self.available_tabs.get(key)?.window_name;
        let base = self
            .app_core
            .layout
            .windows
            .iter()
            .find(|window| window.name() == *window_name)?
            .base();
        if let Some(color) = match base.border_color.as_deref() {
            None | Some("-") | Some("") => None,
            Some(color) => widgets::parse_hex_color(color),
        } {
            return Some(color);
        }
        // colors.toml ui.border_color, only when actually changed from the
        // built-in default (extracted defaults fall through to the theme).
        self.app_core
            .config
            .colors
            .ui
            .user_border_color()
            .and_then(widgets::parse_hex_color)
    }

    /// Apply zoom and title-bar sizing. Zoom is pushed to egui once at
    /// startup; afterwards egui owns it (Ctrl+= / Ctrl+- / Ctrl+0 via
    /// zoom_with_keyboard) and changes are persisted back into settings.
    /// Title bar height follows the Heading text style, so resizing titles
    /// is a style update; `docked_inner_size_for_outer` stays in sync
    /// because it resolves Heading from the same style.
    fn apply_ui_sizing(&mut self, ctx: &egui::Context) {
        if !self.zoom_applied {
            self.zoom_applied = true;
            ctx.options_mut(|options| options.zoom_with_keyboard = true);
            let zoom = self.ui_settings.zoom_factor.clamp(0.5, 3.0);
            if (ctx.zoom_factor() - zoom).abs() > 0.001 {
                ctx.set_zoom_factor(zoom);
            }
        } else {
            let zoom = ctx.zoom_factor();
            if (zoom - self.ui_settings.zoom_factor).abs() > 0.001 {
                self.ui_settings.zoom_factor = zoom;
                self.layout_dirty = true;
            }
        }

        let title_size = self.ui_settings.title_font_size.clamp(8.0, 40.0);
        let density = self.ui_settings.density.clamp(0.5, 2.0);
        let window_radius = self.ui_settings.window_corner_radius.clamp(0.0, 12.0);
        if self.applied_title_font_size != Some(title_size)
            || self.applied_density != Some(density)
            || self.applied_window_corner_radius != Some(window_radius)
        {
            self.applied_title_font_size = Some(title_size);
            self.applied_density = Some(density);
            self.applied_window_corner_radius = Some(window_radius);
            ctx.global_style_mut(|style| {
                if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Heading) {
                    font.size = title_size;
                }
                style.visuals.window_corner_radius =
                    egui::CornerRadius::same(window_radius.round() as u8);
                // Scale spacing from egui's defaults (not the current values,
                // so repeated applies don't compound).
                let defaults = egui::style::Spacing::default();
                style.spacing.item_spacing = defaults.item_spacing * density;
                style.spacing.button_padding = defaults.button_padding * density;
                style.spacing.window_margin = defaults.window_margin * density;
                style.spacing.menu_margin = defaults.menu_margin * density;
                style.spacing.interact_size = defaults.interact_size * density;
            });
        }
    }

    /// Assemble the persistable layout snapshot. Returns None when the dock
    /// snapshot fails to serialize (never persist a null layout).
    fn build_layout_snapshot(&mut self) -> Option<GuiLayoutFileV1> {
        let mut layout = GuiLayoutFileV1::new(&self.layout_profile, &self.layout_character);

        let mut hidden_tabs: Vec<TabKey> = self.hidden_tabs.iter().cloned().collect();
        hidden_tabs.sort_by_key(|key| key.short_id());
        layout.hidden_tabs = hidden_tabs;
        layout.ui_font = self.ui_font.clone();
        layout.ui_settings = self.ui_settings.clone();
        layout.tab_settings = {
            let mut entries: Vec<TabSettingsEntry> = self
                .tab_settings
                .iter()
                .map(|(key, settings)| TabSettingsEntry {
                    key: key.clone(),
                    settings: settings.clone(),
                })
                .collect();
            entries.sort_by_key(|entry| entry.key.short_id());
            entries
        };

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
                        gap_above: self
                            .sidebar_gap_above
                            .get(key)
                            .copied()
                            .filter(|value| value.is_finite() && *value > 0.0)
                            .unwrap_or(0.0),
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
            tab_groups: Self::sanitize_tab_groups(self.tab_groups.clone(), &self.available_tabs),
            // Stable order: GuiShellZone::all() filtered, not HashSet order.
            free_sidebar_zones: GuiShellZone::all()
                .into_iter()
                .filter(|zone| self.migrated_sidebar_zones.contains(zone))
                .collect(),
            pending_zones: {
                let mut entries: Vec<PendingZoneSnapshot> = self
                    .pending_zones
                    .iter()
                    .map(|(window, zone)| PendingZoneSnapshot {
                        window: window.clone(),
                        zone: *zone,
                    })
                    .collect();
                entries.sort_by(|a, b| a.window.cmp(&b.window));
                entries
            },
        };
        layout.dock_state_json = match serde_json::to_value(snapshot) {
            Ok(value) => value,
            Err(err) => {
                // Persisting a null snapshot would wipe the saved window layout;
                // keep the existing file instead.
                tracing::error!("Failed to serialize GUI dock layout; skipping save: {}", err);
                return None;
            }
        };
        layout.detached_viewports = self
            .detached_tabs
            .iter()
            .map(|(key, state)| (key.short_id(), state.current.clone()))
            .collect();
        layout.main_viewport = self.main_viewport_state.clone();
        layout.touch();
        Some(layout)
    }

    /// Record the main OS window's current geometry. Not marked layout-dirty:
    /// it rides along with the next save (including the on-exit flush), so
    /// pure moves/resizes of the OS window don't churn the writer thread.
    fn capture_main_viewport(&mut self, ctx: &egui::Context) {
        let (inner_rect, outer_rect, maximized) = ctx.input(|i| {
            let viewport = i.viewport();
            (
                viewport.inner_rect,
                viewport.outer_rect,
                viewport.maximized.unwrap_or(false),
            )
        });
        let Some(inner_rect) = inner_rect else {
            return;
        };
        if !inner_rect.is_finite() || inner_rect.width() < 1.0 || inner_rect.height() < 1.0 {
            return;
        }
        if maximized {
            // Keep the last un-maximized geometry as the restore size.
            match &mut self.main_viewport_state {
                Some(state) => state.maximized = true,
                None => {
                    self.main_viewport_state = Some(MainViewportState {
                        outer_pos: None,
                        inner_size: [inner_rect.width(), inner_rect.height()],
                        maximized: true,
                    });
                }
            }
        } else {
            self.main_viewport_state = Some(MainViewportState {
                outer_pos: outer_rect
                    .filter(|rect| rect.is_finite())
                    .map(|rect| [rect.min.x, rect.min.y]),
                inner_size: [inner_rect.width(), inner_rect.height()],
                maximized: false,
            });
        }
    }

    /// Persist the layout. Serialization happens here on the UI thread (it is
    /// cheap once debounced); the disk I/O (backup copy + temp write + rename)
    /// runs on the writer thread. Falls back to a synchronous write when the
    /// worker is gone (shutdown path).
    fn save_layout_state(&mut self) {
        let Some(layout) = self.build_layout_snapshot() else {
            return;
        };
        match &self.layout_save_tx {
            Some(tx) => {
                if let Err(send_error) = tx.send(layout) {
                    Self::write_layout_now(
                        &send_error.0,
                        &self.layout_profile,
                        &self.layout_character,
                    );
                }
            }
            None => {
                Self::write_layout_now(&layout, &self.layout_profile, &self.layout_character)
            }
        }
    }

    fn write_layout_now(layout: &GuiLayoutFileV1, profile: &str, character: &str) {
        if let Err(err) = save_layout(layout, profile, character) {
            tracing::warn!("Failed to save GUI layout: {}", err);
        }
    }

    /// Apply a saved layout snapshot to the live app — the runtime half of
    /// `.loadlayout`. Reuses the constructor's reconciliation, so tabs the
    /// file doesn't know keep working and saved tabs missing this session
    /// are dropped. The main OS window geometry is deliberately left alone:
    /// only the arrangement inside it (and detached windows) changes.
    fn apply_layout_snapshot(&mut self, layout: &GuiLayoutFileV1) {
        let restored = Self::restore_layout_state(Some(layout), &self.available_tabs);
        tracing::info!(
            "Applying GUI layout snapshot: {} window rects, {} zone assignments",
            restored.main_window_rects.len(),
            restored.tab_zones.len()
        );
        self.hidden_tabs = restored.hidden_tabs;
        self.main_window_rects = restored.main_window_rects;
        self.sidebar_gap_above = restored.sidebar_gap_above;
        self.migrated_sidebar_zones = restored.migrated_sidebar_zones;
        self.last_center_window_rects.clear();
        self.zone_snap_drag = None;
        self.zone_snap_guides.clear();
        self.tab_zones = restored.tab_zones;
        self.pending_zones = restored.pending_zones;
        self.no_title_tabs = restored.no_title_tabs;
        self.shell_layout = restored.shell_layout;
        self.tab_groups = restored.tab_groups;
        self.detached_tabs = restored.detached_tabs;
        self.ui_font = restored.ui_font;
        let previous_skin = self.ui_settings.active_skin.take();
        self.ui_settings = restored.ui_settings;
        // Skins ride with checkpoints. One without a skin recorded (incl.
        // checkpoints from before skins lived in the layout) keeps the
        // current skin instead of clearing it — .setskin none is the
        // explicit off switch. apply_if_changed swaps the art next frame.
        if self.ui_settings.active_skin.is_none() {
            self.ui_settings.active_skin = previous_skin;
        }
        if self.app_core.config.active_skin != self.ui_settings.active_skin {
            self.app_core.config.active_skin = self.ui_settings.active_skin.clone();
            self.save_config_after_skin_change();
        }
        self.tab_settings = restored.tab_settings;
        // Checkpoints can predate the move of per-window text size/font/wrap
        // onto the layout defs; migrate them the same way startup does.
        let available_tabs = &self.available_tabs;
        let (migrated_layout, _) = Self::migrate_tab_settings_to_layout(
            &mut self.tab_settings,
            &mut self.app_core.layout,
            |key| available_tabs.get(key).map(|tab| tab.window_name.clone()),
        );
        if migrated_layout {
            self.app_core.schedule_layout_autosave();
        }
        // Lazy appliers pick up the new font/zoom/density next frame.
        self.fonts_applied = false;
        self.zoom_applied = false;
        self.applied_title_font_size = None;
        self.applied_density = None;
        self.applied_window_corner_radius = None;
        // The live autosave slot now reflects the loaded arrangement; the
        // checkpoint itself is only written by an explicit .savelayout.
        self.layout_dirty = true;
    }

    fn list_layout_checkpoints(&mut self) {
        let names = list_named_layouts(&self.layout_profile, &self.layout_character);
        if names.is_empty() {
            self.app_core.add_system_message(
                "No saved GUI layouts. Save the current arrangement with .savelayout <name>",
            );
        } else {
            self.app_core
                .add_system_message(&format!("Saved GUI layouts: {}", names.join(", ")));
        }
    }

    fn pump_server_messages(&mut self) {
        // Commands from remote web clients run the same dispatch path as
        // the local input bar.
        while let Ok(event) = self.remote_rx.try_recv() {
            match event {
                crate::core::remote::RemoteEvent::Command(text) => {
                    tracing::debug!("remote command: '{}'", text);
                    self.record_command_history(&text);
                    self.dispatch_command(text);
                }
                crate::core::remote::RemoteEvent::LinkTap {
                    client_id,
                    request_id,
                    exist_id,
                    noun,
                    text,
                    coord,
                } => {
                    // Resolved exactly like a local click: <d>/coord links
                    // become direct commands, plain links a _menu request
                    // tagged to route back to this client.
                    let link = crate::data::LinkData {
                        exist_id,
                        noun,
                        text,
                        coord,
                    };
                    if let Some(cmd) = self.app_core.resolve_link_activation(
                        &link,
                        crate::core::remote::MenuOrigin::Remote {
                            client_id,
                            request_id,
                        },
                    ) {
                        self.app_core
                            .perf_stats
                            .record_bytes_sent((cmd.len() + 1) as u64);
                        let _ = self.command_tx.send(cmd);
                    }
                }
                crate::core::remote::RemoteEvent::MacroSave {
                    group,
                    label,
                    command,
                    color,
                    confirm,
                    insert,
                    options,
                    original,
                } => {
                    let button = crate::config::MacroButton {
                        label,
                        command: Some(command).filter(|c| !c.is_empty()),
                        color,
                        confirm,
                        insert,
                        options,
                        ..Default::default()
                    };
                    self.app_core.apply_macro_save(group, button, original);
                }
                crate::core::remote::RemoteEvent::MacroDelete { group, label } => {
                    self.app_core.apply_macro_delete(group, label);
                }
                crate::core::remote::RemoteEvent::Notice(message) => {
                    self.app_core.add_system_message(&message);
                }
                crate::core::remote::RemoteEvent::ConfigGet {
                    client_id,
                    request_id,
                    file,
                } => {
                    self.app_core
                        .handle_remote_config_get(client_id, request_id, file);
                }
                crate::core::remote::RemoteEvent::ConfigPut {
                    client_id,
                    request_id,
                    file,
                    content,
                } => {
                    self.app_core
                        .handle_remote_config_put(client_id, request_id, file, content);
                }
                crate::core::remote::RemoteEvent::HighlightsGet {
                    client_id,
                    request_id,
                    scope,
                } => {
                    self.app_core
                        .handle_remote_highlights_get(client_id, request_id, scope);
                }
                crate::core::remote::RemoteEvent::HighlightPut {
                    client_id,
                    request_id,
                    scope,
                    name,
                    rule,
                } => {
                    self.app_core
                        .handle_remote_highlight_put(client_id, request_id, scope, name, rule);
                }
                crate::core::remote::RemoteEvent::SettingsGet {
                    client_id,
                    request_id,
                } => {
                    self.app_core
                        .handle_remote_settings_get(client_id, request_id);
                }
                crate::core::remote::RemoteEvent::SettingsPut {
                    client_id,
                    request_id,
                    key,
                    value,
                    scope,
                    clear,
                } => {
                    self.app_core.handle_remote_settings_put(
                        client_id, request_id, key, value, scope, clear,
                    );
                }
                crate::core::remote::RemoteEvent::StreamsGet {
                    client_id,
                    request_id,
                } => {
                    self.app_core
                        .handle_remote_streams_get(client_id, request_id);
                }
                crate::core::remote::RemoteEvent::StreamsPut {
                    client_id,
                    request_id,
                    stream,
                    target,
                } => {
                    self.app_core
                        .handle_remote_streams_put(client_id, request_id, stream, target);
                }
                crate::core::remote::RemoteEvent::ColorsGet {
                    client_id,
                    request_id,
                    scope,
                } => {
                    self.app_core
                        .handle_remote_colors_get(client_id, request_id, scope);
                }
                crate::core::remote::RemoteEvent::ColorsPut {
                    client_id,
                    request_id,
                    scope,
                    colors,
                } => {
                    self.app_core
                        .handle_remote_colors_put(client_id, request_id, scope, colors);
                }
                crate::core::remote::RemoteEvent::MapLocations {
                    client_id,
                    request_id,
                } => {
                    self.app_core
                        .handle_remote_map_locations(client_id, request_id);
                }
                crate::core::remote::RemoteEvent::MapView {
                    client_id,
                    request_id,
                    location,
                } => {
                    self.app_core
                        .handle_remote_map_view(client_id, request_id, location);
                }
                crate::core::remote::RemoteEvent::HighlightDelete {
                    client_id,
                    request_id,
                    scope,
                    name,
                } => {
                    self.app_core
                        .handle_remote_highlight_delete(client_id, request_id, scope, name);
                }
                crate::core::remote::RemoteEvent::SessionConnect { .. }
                | crate::core::remote::RemoteEvent::SessionDisconnect => {
                    // Sidecar sessions are owned by this local UI; the web
                    // client shouldn't offer these (session_control is
                    // false), but answer stray requests politely.
                    self.app_core.add_system_message(
                        "Session control is only available in headless mode.",
                    );
                }
                crate::core::remote::RemoteEvent::Macro { id } => {
                    // Resolve the id against config; the command runs the
                    // same dispatch as typed input (echo, dot-commands).
                    match self.app_core.config.macros.resolve(&id).map(String::from) {
                        Some(command) => {
                            tracing::debug!("remote macro '{}': '{}'", id, command);
                            self.dispatch_command(command);
                        }
                        None => tracing::warn!(
                            "remote macro id '{}' did not resolve (stale client?)",
                            id
                        ),
                    }
                }
                crate::core::remote::RemoteEvent::WheelPick { key, path } => {
                    // Resolved against config (or the dynamic portals
                    // wheel) like macros; same dispatch as typed input.
                    match self.app_core.wheel_pick_command(&key, &path) {
                        Some(command) => {
                            tracing::debug!(
                                "remote wheel pick '{}' {:?}: '{}'",
                                key,
                                path,
                                command
                            );
                            self.dispatch_command(command);
                        }
                        None => tracing::warn!(
                            "remote wheel pick '{}' {:?} did not resolve (stale client?)",
                            key,
                            path
                        ),
                    }
                }
            }
        }

        // Drain map worker results (mapdb load, layout generation), the
        // mapdb release updater, and the walk executor.
        self.app_core.poll_map();
        // Commands the walk executor queued go out through the same path as
        // typed commands (echo, ghost-room labels, network).
        for command in self.app_core.take_outbound() {
            self.dispatch_command(command);
        }

        let mut received_text = false;
        while let Ok(message) = self.server_rx.try_recv() {
            match message {
                ServerMessage::Text(line) => {
                    // First data from the game = connection established:
                    // time the login music from here.
                    if self.startup_music_pending {
                        self.startup_music_pending = false;
                        self.startup_music_at = Some(
                            std::time::Instant::now()
                                + std::time::Duration::from_millis(
                                    self.app_core.config.sound.startup_music_delay_ms,
                                ),
                        );
                    }
                    self.app_core
                        .perf_stats
                        .record_bytes_received((line.len() + 1) as u64);
                    if let Err(err) = self.app_core.process_server_data(&line) {
                        self.app_core
                            .add_system_message(&format!("GUI parse error: {}", err));
                    }
                    self.app_core.needs_render = true;
                    received_text = true;
                }
                ServerMessage::Connected => {
                    self.app_core.game_state.connected = true;
                    self.app_core.needs_render = true;
                    // Layout has saved WebUI panels: bring them back up
                    // automatically (Lich proxy connections only - a direct
                    // connection has no Lich to answer the handshake).
                    if !self.is_direct_connection
                        && !self.webui_handshake_sent
                        && self.has_webui_windows()
                    {
                        self.request_webui_handshake();
                    }
                }
                ServerMessage::Disconnected => {
                    self.app_core.game_state.connected = false;
                    self.app_core.needs_render = true;
                }
            }
        }

        // Post-processing the TUI runtime also performs after server data:
        // content-driven resizes, plus realizing game-offered windows
        // (containers whose offer the user has Shown, openDialog-templated
        // widgets like stance/inventory/experience).
        if received_text {
            self.app_core.adjust_content_driven_windows();
            let (layout_width, layout_height) = self.core_layout_size;
            self.app_core
                .realize_offered_windows(layout_width, layout_height);

            // A `;ui handshake` reply arrived on the game stream: connect
            // (or reconnect) the WebUI bridge with the fresh port + token.
            if let Some(handshake) = self
                .app_core
                .message_processor
                .pending_webui_handshake
                .take()
            {
                self.handle_webui_handshake(handshake);
            }
        }

        self.pump_webui_events();

        // Flush coalesced state deltas to web clients once per batch
        // (no-op unless [web] is enabled)
        self.app_core.flush_remote_state();

        // Send commands queued by dialog-panel widgets this frame
        // (they render from an immutable AppCore borrow).
        let panel_commands: Vec<String> =
            self.app_core.ui_state.pending_panel_commands.borrow_mut().drain(..).collect();
        for command in panel_commands {
            self.dispatch_raw_command(command);
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

    // ==================== Lich WebUI bridge ====================

    fn has_webui_windows(&self) -> bool {
        self.app_core
            .ui_state
            .windows
            .values()
            .any(|w| matches!(w.content, WindowContent::WebUi(_)))
    }

    /// Asks Lich for the WebUI endpoint. The reply comes back on the game
    /// stream as one `<LichWebUI .../>` line (handled in pump_server_messages).
    fn request_webui_handshake(&mut self) {
        if self.is_direct_connection {
            self.app_core.add_system_message(
                "The Lich WebUI needs a Lich proxy connection (direct connections bypass Lich).",
            );
            self.webui_pending.clear();
            return;
        }
        self.webui_handshake_sent = true;
        // Accounted raw send (byte counters, no dot-command re-interception),
        // matching every other outbound line.
        self.dispatch_raw_command(";ui handshake".to_string());
    }

    fn handle_webui_handshake(&mut self, handshake: crate::data::webui::WebUiHandshake) {
        match handshake.status.as_str() {
            "ok" => {}
            "disabled" => {
                self.app_core.add_system_message(
                    "Lich WebUI is disabled. Run ;ui on (persists), then .webui again.",
                );
                self.webui_pending.clear();
                return;
            }
            other => {
                self.app_core.add_system_message(&format!(
                    "Lich WebUI is not running (status: {}). Check ;ui status in Lich.",
                    other
                ));
                self.webui_pending.clear();
                return;
            }
        }
        let Some(token) = handshake.token().map(String::from) else {
            self.app_core
                .add_system_message("Lich WebUI handshake had no auth token; cannot connect.");
            return;
        };

        // Replace any prior bridge (Lich restarts change port and token).
        // Dial the address the handshake url advertises — a containerized
        // Lich hands out its reachable LAN address there, not loopback.
        let (host, port) = handshake.endpoint();
        self.webui_bridge = None;
        self.webui_rx = None;
        self.webui_endpoint = Some((host.clone(), port, token.clone()));
        self.webui_fetches_inflight.clear();

        // Same waking hop as server messages: forward bridge events and
        // repaint so panel updates aren't stuck waiting for an idle frame.
        let (event_tx, mut raw_rx) = mpsc::unbounded_channel::<crate::webui::WebUiEvent>();
        self.webui_event_tx = Some(event_tx.clone());
        let (forward_tx, forward_rx) = mpsc::unbounded_channel::<crate::webui::WebUiEvent>();
        let waker_ctx = std::sync::Arc::clone(&self.repaint_ctx);
        self._runtime.spawn(async move {
            while let Some(event) = raw_rx.recv().await {
                if forward_tx.send(event).is_err() {
                    break;
                }
                if let Some(ctx) = waker_ctx.lock().ok().and_then(|slot| slot.clone()) {
                    ctx.request_repaint();
                }
            }
        });

        let handle =
            crate::webui::start(self._runtime.handle(), host.clone(), port, token, event_tx);
        self.webui_bridge = Some(handle);
        self.webui_rx = Some(forward_rx);
        tracing::info!("WebUI bridge connecting to {}:{}", host, port);
    }

    /// Applies bridge events to panel windows. Called once per frame.
    fn pump_webui_events(&mut self) {
        let Some(rx) = self.webui_rx.as_mut() else {
            return;
        };
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        for event in events {
            match event {
                crate::webui::WebUiEvent::Hello { session, pages, .. } => {
                    self.webui_pages = pages;
                    self.refresh_webui_window_kinds();
                    self.set_webui_windows_connected(true);
                    // Fresh connection: failed images are worth retrying, and
                    // Loading entries are orphans of the previous socket.
                    if let Some(ctx) = self.repaint_ctx.lock().ok().and_then(|slot| slot.clone()) {
                        Self::clear_stale_webui_images(&ctx);
                    }
                    self.app_core.add_system_message(&format!(
                        "Lich WebUI connected ({} - {} page{}).",
                        session.name,
                        self.webui_pages.len(),
                        if self.webui_pages.len() == 1 { "" } else { "s" }
                    ));
                    // Re-subscribe every panel window (fresh socket has no
                    // subscriptions; renders re-arrive and clear stale trees).
                    let pages: Vec<String> = self
                        .app_core
                        .ui_state
                        .windows
                        .values()
                        .filter_map(|w| match &w.content {
                            WindowContent::WebUi(content) => Some(content.page_id.clone()),
                            _ => None,
                        })
                        .collect();
                    if let Some(bridge) = &self.webui_bridge {
                        for page in pages {
                            bridge.subscribe(&page);
                        }
                    }
                    let pending = std::mem::take(&mut self.webui_pending);
                    for action in pending {
                        match action {
                            WebUiPendingAction::Picker => self.open_webui_picker(),
                            WebUiPendingAction::Open(page) => self.open_webui_page(&page),
                        }
                    }
                }
                crate::webui::WebUiEvent::Pages(pages) => {
                    // A page we host may have just re-registered (script
                    // restart): subscribe again so it resumes.
                    let hosted_ended: Vec<String> = self
                        .app_core
                        .ui_state
                        .windows
                        .values()
                        .filter_map(|w| match &w.content {
                            WindowContent::WebUi(content) if content.ended.is_some() => {
                                Some(content.page_id.clone())
                            }
                            _ => None,
                        })
                        .collect();
                    if let Some(bridge) = &self.webui_bridge {
                        for page in hosted_ended {
                            if pages.iter().any(|p| p.id == page) {
                                bridge.subscribe(&page);
                            }
                        }
                    }
                    // Transient pages registered while we're connected open
                    // themselves: ";bigshot setup" pops its dialog without a
                    // .webui round-trip (and the window auto-closes on save,
                    // completing the dialog lifecycle). Declared panels stay
                    // opt-in - dock once via .webui, the layout brings them
                    // back - so a script restart never resurrects a panel
                    // window the user closed on purpose. Dialogs (a settings
                    // popup, e.g. map/settings) are on-demand supplemental
                    // pages: they open via the image_map right-click popup,
                    // never just because the script registered them. The
                    // hello baseline never auto-opens (connecting must not
                    // spawn windows).
                    let fresh: Vec<String> = pages
                        .iter()
                        .filter(|p| !matches!(p.kind.as_deref(), Some("panel") | Some("dialog")))
                        .filter(|p| !self.webui_pages.iter().any(|k| k.id == p.id))
                        .map(|p| p.id.clone())
                        .collect();
                    self.webui_pages = pages;
                    self.refresh_webui_window_kinds();
                    for page in fresh {
                        self.open_webui_page(&page);
                    }
                }
                crate::webui::WebUiEvent::Render { page, seq, tree } => {
                    self.apply_webui_render(&page, seq, tree);
                }
                crate::webui::WebUiEvent::PageClosed { page } => {
                    // Declared panels (kind: "panel") stay docked with a
                    // notice and resume when the script re-registers, so a
                    // crash or ;repository restart never silently removes a
                    // docked widget. Everything else (settings dialogs,
                    // pages with no kind hint) closes with its script.
                    let is_panel = self.app_core.ui_state.windows.values().any(|w| {
                        matches!(&w.content, WindowContent::WebUi(c)
                            if c.page_id == page && c.kind.as_deref() == Some("panel"))
                    });
                    if is_panel {
                        self.with_webui_window(&page, |content| {
                            content.ended = Some("The owning script exited.".to_string());
                        });
                    } else {
                        let names: Vec<String> = self
                            .app_core
                            .ui_state
                            .windows
                            .values()
                            .filter(|w| {
                                matches!(&w.content, WindowContent::WebUi(c)
                                    if c.page_id == page)
                            })
                            .map(|w| w.name.clone())
                            .collect();
                        if !names.is_empty() {
                            for name in &names {
                                self.app_core.remove_window(name);
                                self.app_core.layout.windows.retain(|w| w.name() != *name);
                            }
                            self.app_core.schedule_layout_autosave();
                            tracing::info!(
                                "WebUI page '{}' ended; closed transient panel",
                                page
                            );
                        }
                    }
                }
                crate::webui::WebUiEvent::Notice { level, text } => {
                    self.app_core
                        .add_system_message(&format!("[WebUI {}] {}", level, text));
                }
                crate::webui::WebUiEvent::Disconnected {
                    gave_up,
                    never_connected,
                } => {
                    self.set_webui_windows_connected(false);
                    if gave_up {
                        if never_connected {
                            // The endpoint refused every attempt: almost
                            // always the advertised host isn't reachable
                            // from this machine (WebUI bound to localhost
                            // on the Lich box, firewall, stale address).
                            let endpoint = self
                                .webui_endpoint
                                .as_ref()
                                .map(|(host, port, _)| format!("{}:{}", host, port))
                                .unwrap_or_else(|| "the advertised address".to_string());
                            self.app_core.add_system_message(&format!(
                                "Lich WebUI unreachable at {} (every attempt refused). \
                                 If ';ui' says it is running, the WebUI is likely only \
                                 listening on the Lich machine's localhost or its \
                                 firewall blocks the port - check that http://{}/ opens \
                                 in a browser on THIS machine, then run .webui again.",
                                endpoint, endpoint
                            ));
                        } else {
                            self.app_core.add_system_message(
                                "Lich WebUI connection lost (Lich restarted?). Run .webui to reconnect.",
                            );
                        }
                        self.webui_bridge = None;
                        self.webui_rx = None;
                        self.webui_endpoint = None;
                        self.webui_event_tx = None;
                        self.webui_handshake_sent = false;
                        break;
                    }
                }
                crate::webui::WebUiEvent::ImageFetched { src, data } => {
                    self.webui_fetches_inflight.remove(&src);
                    let Some(ctx) = self.repaint_ctx.lock().ok().and_then(|slot| slot.clone())
                    else {
                        continue;
                    };
                    let state = match data {
                        Ok(bytes) => Self::decode_webui_image(&ctx, &src, &bytes),
                        Err(err) => {
                            tracing::warn!("WebUI image '{}' fetch failed: {}", src, err);
                            webui_panel::WebUiImageState::Failed(err)
                        }
                    };
                    Self::set_webui_image(&ctx, src, state);
                    self.app_core.needs_render = true;
                }
            }
        }
    }

    fn apply_webui_render(&mut self, page: &str, seq: u64, tree: crate::data::webui::WebUiNode) {
        let mut applied = false;
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::WebUi(content) = &mut window.content {
                if content.page_id == page {
                    // Drop stale out-of-order renders (reconnect replays can
                    // race a live push).
                    if seq != 0 && seq <= content.seq && content.tree.is_some() {
                        return;
                    }
                    content.tree = Some(tree);
                    content.seq = seq;
                    content.generation = content.generation.wrapping_add(1);
                    content.connected = true;
                    content.ended = None;
                    applied = true;
                    break;
                }
            }
        }
        if applied {
            self.app_core.needs_render = true;
        } else {
            tracing::debug!("WebUI render for unhosted page '{}' ignored", page);
        }
    }

    fn with_webui_window(
        &mut self,
        page: &str,
        apply: impl FnOnce(&mut crate::data::webui::WebUiPanelContent),
    ) {
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::WebUi(content) = &mut window.content {
                if content.page_id == page {
                    apply(content);
                    self.app_core.needs_render = true;
                    return;
                }
            }
        }
    }

    fn set_webui_windows_connected(&mut self, connected: bool) {
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::WebUi(content) = &mut window.content {
                content.connected = connected;
            }
        }
        self.app_core.needs_render = true;
    }

    /// Popup menu of the session's registered pages (like `.addwindow`).
    fn open_webui_picker(&mut self) {
        if self.webui_pages.is_empty() {
            self.app_core.add_system_message(
                "No WebUI pages are registered. Start a script that opens one (e.g. ;webui-demo).",
            );
            return;
        }
        let items: Vec<PopupMenuItem> = self
            .webui_pages
            .iter()
            .map(|page| {
                let text = if page.title.is_empty() {
                    page.id.clone()
                } else {
                    format!("{} ({})", page.title, page.id)
                };
                PopupMenuItem {
                    text,
                    // Menu commands that miss every internal prefix are sent
                    // to the game raw (Wrayth menu semantics), so this must
                    // be the action string, not the ".webui" dot-command.
                    command: format!("action:webui:open:{}", page.id),
                    disabled: false,
                }
            })
            .collect();
        self.close_all_popup_menus();
        self.app_core.ui_state.popup_menu = Some(PopupMenu::new(items, (8, 4)));
        self.app_core.ui_state.input_mode = InputMode::Menu;
    }

    /// Refreshes each hosted window's remembered page kind from the current
    /// registry. Windows restored from a saved layout start with no kind;
    /// this is what re-teaches them before their page can end.
    fn refresh_webui_window_kinds(&mut self) {
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::WebUi(content) = &mut window.content {
                if let Some(descriptor) =
                    self.webui_pages.iter().find(|p| p.id == content.page_id)
                {
                    if descriptor.kind.is_some() {
                        content.kind = descriptor.kind.clone();
                    }
                }
            }
        }
    }

    /// Creates (or focuses) the panel window for a page and subscribes.
    fn open_webui_page(&mut self, page_id: &str) {
        let descriptor = self.webui_pages.iter().find(|p| p.id == page_id);
        let title = descriptor
            .map(|d| {
                if d.title.is_empty() {
                    d.id.clone()
                } else {
                    d.title.clone()
                }
            })
            .unwrap_or_else(|| page_id.to_string());
        let size = descriptor.and_then(|d| d.size);
        let kind = descriptor.and_then(|d| d.kind.clone());

        let name = self
            .app_core
            .add_webui_window(page_id, &title, size, kind.clone());
        self.with_webui_window(page_id, |content| {
            content.connected = true;
            if kind.is_some() {
                content.kind = kind.clone();
            }
        });
        if let Some(bridge) = &self.webui_bridge {
            bridge.subscribe(page_id);
        }
        self.layout_dirty = true;
        tracing::info!("WebUI panel '{}' opened for page '{}'", name, page_id);
    }

    /// Closes one WebUI window from its title-bar close button: removes the
    /// window and its layout entry, and unsubscribes the page so the server
    /// sees the viewer leave (scripts with a window watchdog - ;map - treat
    /// that as the window closing, matching the GTK/browser lifecycle).
    fn close_webui_window(&mut self, name: &str) {
        let page_id = self.app_core.ui_state.windows.get(name).and_then(|w| {
            match &w.content {
                WindowContent::WebUi(content) => Some(content.page_id.clone()),
                _ => None,
            }
        });
        let Some(page_id) = page_id else { return };
        self.app_core.remove_window(name);
        self.app_core.layout.windows.retain(|w| w.name() != name);
        self.app_core.schedule_layout_autosave();
        self.layout_dirty = true;
        // Only unsubscribe when no other window still shows the page.
        let still_hosted = self.app_core.ui_state.windows.values().any(|w| {
            matches!(&w.content, WindowContent::WebUi(c) if c.page_id == page_id)
        });
        if !still_hosted {
            if let Some(bridge) = &self.webui_bridge {
                bridge.send(crate::data::webui::WebUiClientMessage::Unsubscribe {
                    page: page_id.clone(),
                });
            }
        }
        tracing::info!(
            "WebUI panel '{}' closed by user (page '{}', unsubscribed: {})",
            name,
            page_id,
            !still_hosted
        );
    }

    /// `.webui` action entry points (returns true when handled).
    fn handle_webui_action(&mut self, action: &str) -> bool {
        if action == "action:webui" {
            if self.webui_bridge.is_some() && !self.webui_pages.is_empty() {
                self.open_webui_picker();
            } else {
                self.webui_pending.push(WebUiPendingAction::Picker);
                self.app_core
                    .add_system_message("Requesting WebUI handshake from Lich...");
                self.request_webui_handshake();
            }
            return true;
        }
        if action == "action:webui:off" {
            self.webui_bridge = None;
            self.webui_rx = None;
            self.webui_pages.clear();
            self.webui_pending.clear();
            self.webui_handshake_sent = false;
            self.webui_endpoint = None;
            self.webui_event_tx = None;
            self.webui_fetches_inflight.clear();
            self.set_webui_windows_connected(false);
            self.app_core
                .add_system_message("Lich WebUI bridge disconnected.");
            return true;
        }
        if let Some(page) = action.strip_prefix("action:webui:open:") {
            let page = page.to_string();
            if self.webui_bridge.is_some() {
                self.open_webui_page(&page);
            } else {
                self.webui_pending.push(WebUiPendingAction::Open(page));
                self.app_core
                    .add_system_message("Requesting WebUI handshake from Lich...");
                self.request_webui_handshake();
            }
            return true;
        }
        false
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
            // Esc cancels an active .go2 trip from anywhere in the GUI. Gated
            // on the same text-capture modes as macro dispatch so an editor
            // that owns the keyboard keeps its Esc semantics.
            if key_press.key_event.code == crate::data::input::KeyCode::Esc
                && key_press.key_event.modifiers == crate::data::input::KeyModifiers::NONE
                && !suppress_macro_dispatch
                && self.app_core.travel.is_traveling()
            {
                self.app_core.stop_travel();
                consumed_keyboard_input = true;
                ctx.input_mut(|input| {
                    if let Some(logical_key) = key_press.logical_key {
                        input.consume_key(key_press.modifiers, logical_key);
                    }
                    if let Some(physical_key) = key_press.physical_key {
                        input.consume_key(key_press.modifiers, physical_key);
                    }
                });
                continue;
            }

            // Interact mode and popup-menu keyboard navigation take plain
            // arrows/enter/escape before keybinds or text input see them.
            // Window-move and window-context-menu keep their own Esc
            // semantics (handled later this frame).
            if !suppress_macro_dispatch
                && self.window_move_state.is_none()
                && self.window_context_menu.is_none()
                && self.handle_modal_nav_key(&key_press.key_event, ctx)
            {
                consumed_keyboard_input = true;
                ctx.input_mut(|input| {
                    if let Some(logical_key) = key_press.logical_key {
                        input.consume_key(key_press.modifiers, logical_key);
                    }
                    if let Some(physical_key) = key_press.physical_key {
                        input.consume_key(key_press.modifiers, physical_key);
                    }
                });
                continue;
            }

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
            self.execute_global_dispatch_target(target, ctx);

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
                if !numpad_key.pressed || numpad_key.repeat {
                    return None;
                }

                // keybind_name() is Some only for events eframe consumed (egui
                // never saw them), so dispatch can't double-act with text input.
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

    /// Tell eframe which numpad keys actually have bindings, so unbound keys
    /// keep their native behavior (typing digits, NumpadEnter submitting text).
    /// Cheap when nothing changed; call every frame so edits from the keybind
    /// editor and dot-commands are picked up wherever they happen.
    #[cfg(not(target_arch = "wasm32"))]
    fn sync_numpad_capture_keys(&mut self, frame: &mut eframe::Frame) {
        let keys = self.bound_numpad_capture_keys();
        if self.numpad_capture_keys.as_ref() != Some(&keys) {
            frame.set_numpad_capture_keys(Some(keys.clone()));
            self.numpad_capture_keys = Some(keys);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn sync_numpad_capture_keys(&mut self, _frame: &mut eframe::Frame) {}

    /// Numpad keybind names ("num_1", "num_plus", …) with a user binding or an
    /// app shortcut, i.e. the keys `handle_global_input` can actually dispatch.
    fn bound_numpad_capture_keys(&self) -> HashSet<String> {
        let mut keys: HashSet<String> = self
            .app_core
            .keybind_map
            .keys()
            .filter_map(|event| Self::frontend_code_to_numpad_binding_name(event.code))
            .map(str::to_string)
            .collect();

        let app_keybinds = &self.app_core.config.app_keybinds;
        for binding in [
            &app_keybinds.quit,
            &app_keybinds.start_search,
            &app_keybinds.close_window,
        ] {
            if let Some((code, _)) = crate::config::parse_key_string(binding) {
                if let Some(name) = Self::frontend_code_to_numpad_binding_name(code) {
                    keys.insert(name.to_string());
                }
            }
        }
        keys
    }

    fn frontend_code_to_numpad_binding_name(
        code: crate::data::input::KeyCode,
    ) -> Option<&'static str> {
        let name = match code {
            crate::data::input::KeyCode::Keypad0 => "num_0",
            crate::data::input::KeyCode::Keypad1 => "num_1",
            crate::data::input::KeyCode::Keypad2 => "num_2",
            crate::data::input::KeyCode::Keypad3 => "num_3",
            crate::data::input::KeyCode::Keypad4 => "num_4",
            crate::data::input::KeyCode::Keypad5 => "num_5",
            crate::data::input::KeyCode::Keypad6 => "num_6",
            crate::data::input::KeyCode::Keypad7 => "num_7",
            crate::data::input::KeyCode::Keypad8 => "num_8",
            crate::data::input::KeyCode::Keypad9 => "num_9",
            crate::data::input::KeyCode::KeypadPlus => "num_plus",
            crate::data::input::KeyCode::KeypadMinus => "num_minus",
            crate::data::input::KeyCode::KeypadMultiply => "num_multiply",
            crate::data::input::KeyCode::KeypadDivide => "num_divide",
            crate::data::input::KeyCode::KeypadEnter => "num_enter",
            crate::data::input::KeyCode::KeypadPeriod => "num_decimal",
            _ => return None,
        };
        Some(name)
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
            match keybind_map.get(&key_event) {
                Some(binding @ KeyBindAction::Macro(_)) => {
                    return Some(GlobalDispatchTarget::Macro(binding.clone()));
                }
                // Action bindings the core can execute without frontend help
                // (mode/system toggles, TTS). Cursor/scroll/search/tab actions
                // stay with the GUI's own widgets — dispatching those here
                // would steal keys like Esc (clear_search) from every editor.
                Some(binding @ KeyBindAction::Action(name)) => {
                    if crate::config::KeyAction::from_str(name)
                        .is_some_and(Self::is_core_global_action)
                    {
                        return Some(GlobalDispatchTarget::Macro(binding.clone()));
                    }
                }
                None => {}
            }
        }

        Self::app_shortcut_for_key(key_event, app_keybinds).map(GlobalDispatchTarget::Shortcut)
    }

    /// Action keybinds that execute fully inside AppCore, safe to dispatch
    /// globally in the GUI (everything else is widget-level and handled by
    /// the GUI's own input paths).
    fn is_core_global_action(action: crate::config::KeyAction) -> bool {
        use crate::config::KeyAction;
        matches!(
            action,
            KeyAction::InteractMode
                | KeyAction::StopTravel
                | KeyAction::ToggleSounds
                | KeyAction::TogglePerformanceStats
                | KeyAction::TtsNext
                | KeyAction::TtsPrevious
                | KeyAction::TtsNextUnread
                | KeyAction::TtsStop
                | KeyAction::TtsMuteToggle
                | KeyAction::TtsIncreaseRate
                | KeyAction::TtsDecreaseRate
                | KeyAction::TtsIncreaseVolume
                | KeyAction::TtsDecreaseVolume
        )
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
        matches!(
            self.app_core.ui_state.input_mode,
            InputMode::KeybindForm | InputMode::Search
        )
    }

    fn execute_global_dispatch_target(
        &mut self,
        target: GlobalDispatchTarget,
        ctx: &egui::Context,
    ) {
        match target {
            GlobalDispatchTarget::Macro(action) => self.execute_macro_keybind(&action, ctx),
            GlobalDispatchTarget::Shortcut(shortcut) => self.execute_app_shortcut(shortcut),
        }
    }

    /// Scroll actions bound to keys or controller buttons, applied to the
    /// GUI's own scroll model — the core implementations move
    /// `content.scroll_offset`, which only the TUI renders. v1 targets the
    /// "main" story window. Returns true when the action was a scroll.
    fn try_gui_scroll_action(&mut self, action: &KeyBindAction, ctx: &egui::Context) -> bool {
        let KeyBindAction::Action(name) = action else {
            return false;
        };
        let scroll_id = "main";
        let view_h: f32 = ctx
            .data_mut(|d| d.get_temp(egui::Id::new(("text_scroll_view_h", scroll_id))))
            .unwrap_or(400.0);
        // (kind, value): 0 = relative px, 1 = home, 2 = end
        let request: (u8, f32) = match name.as_str() {
            "scroll_current_window_up_page" => (0, -(view_h * 0.85)),
            "scroll_current_window_down_page" => (0, view_h * 0.85),
            "scroll_current_window_up_one" => (0, -48.0),
            "scroll_current_window_down_one" => (0, 48.0),
            "scroll_current_window_home" => (1, 0.0),
            "scroll_current_window_end" => (2, 0.0),
            _ => return false,
        };
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(("text_scroll_pending", scroll_id)), request)
        });
        ctx.request_repaint();
        true
    }

    fn execute_macro_keybind(&mut self, action: &KeyBindAction, ctx: &egui::Context) {
        if self.try_gui_scroll_action(action, ctx) {
            return;
        }
        #[cfg(feature = "gamepad")]
        if matches!(action, KeyBindAction::Action(name) if name == "controller_overlay") {
            self.gp_overlay = !self.gp_overlay;
            ctx.request_repaint();
            return;
        }
        match self.app_core.execute_keybind_action(action) {
            Ok(outcomes) => {
                for outcome in outcomes {
                    match outcome {
                        crate::data::CommandOutcome::Game(outbound) => {
                            if Self::should_send_to_network(&outbound) {
                                self.app_core
                                    .perf_stats
                                    .record_bytes_sent((outbound.len() + 1) as u64);
                                let _ = self.command_tx.send(outbound);
                            }
                        }
                        crate::data::CommandOutcome::Handled => {}
                        // A macro bound to a dot-command that opens an
                        // editor: perform it here.
                        crate::data::CommandOutcome::Ui(ui) => self.handle_ui_action(ui),
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
                self.search_bar_needs_focus = true;
            }
            AppShortcut::CloseWindow => self.handle_close_window_shortcut(),
        }
    }

    fn handle_close_window_shortcut(&mut self) {
        // Move mode owns Esc: the move overlay cancels and restores the
        // window's original position later this frame.
        if self.window_move_state.is_some() {
            return;
        }
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
        self.record_command_history(&input);
        self.history_pos = None;
        self.history_draft.clear();
        self.dispatch_command(input);
    }

    const MAX_COMMAND_HISTORY: usize = 100;

    fn history_path_for(character: Option<&str>) -> Option<std::path::PathBuf> {
        crate::config::Config::history_path(character).ok()
    }

    /// Load history from the shared per-profile file (newest first, same
    /// format the TUI reads and writes).
    fn load_command_history(character: Option<&str>) -> std::collections::VecDeque<String> {
        let mut history = std::collections::VecDeque::new();
        let Some(path) = Self::history_path_for(character) else {
            return history;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return history;
        };
        for line in text.lines() {
            if !line.trim().is_empty() {
                history.push_back(line.to_string());
                if history.len() >= Self::MAX_COMMAND_HISTORY {
                    break;
                }
            }
        }
        history
    }

    /// Record a submitted command: min-length and consecutive-dedupe rules
    /// matching the TUI's input model, then persist.
    fn record_command_history(&mut self, command: &str) {
        let command = command.trim_end();
        if command.is_empty() || command.len() < self.app_core.config.ui.min_command_length {
            return;
        }
        if self.command_history.front().map(String::as_str) == Some(command) {
            return;
        }
        self.command_history.push_front(command.to_string());
        self.command_history.truncate(Self::MAX_COMMAND_HISTORY);
        if let Some(path) = Self::history_path_for(self.app_core.config.character.as_deref()) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let joined: String = self
                .command_history
                .iter()
                .map(|c| format!("{c}\n"))
                .collect();
            let _ = std::fs::write(path, joined);
        }
    }

    /// Up arrow: step back through history (stashing the in-progress text
    /// on entry).
    fn history_previous(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        let next = match self.history_pos {
            None => {
                self.history_draft = std::mem::take(&mut self.command_input);
                0
            }
            Some(i) if i + 1 < self.command_history.len() => i + 1,
            Some(i) => i,
        };
        self.history_pos = Some(next);
        self.command_input = self.command_history[next].clone();
    }

    /// Down arrow: step toward newest; at the newest entry (or when not
    /// browsing) clear the input so it's ready for fresh typing.
    fn history_next(&mut self) {
        match self.history_pos {
            Some(0) | None => {
                self.history_pos = None;
                self.command_input.clear();
                self.history_draft.clear();
            }
            Some(i) => {
                self.history_pos = Some(i - 1);
                self.command_input = self.command_history[i - 1].clone();
            }
        }
    }

    /// Put the caret at the end of the input after programmatic text swaps.
    fn command_cursor_to_end(&self, ctx: &egui::Context) {
        let Some(id) = self.command_input_id else {
            return;
        };
        if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
            let ccursor = egui::text::CCursor::new(self.command_input.chars().count());
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
            state.store(ctx, id);
        }
    }

    /// Run a command through the shared core path (echo, dot-commands,
    /// quit interception). Used by the local input bar and by commands
    /// arriving from remote web clients.
    fn dispatch_command(&mut self, command: String) {
        let command = command.trim_end().to_string();
        if command.is_empty() {
            return;
        }

        match self.app_core.send_command(command) {
            Ok(crate::data::CommandOutcome::Ui(action)) => self.handle_ui_action(action),
            Ok(crate::data::CommandOutcome::Handled) => {}
            Ok(crate::data::CommandOutcome::Game(outbound)) => {
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

    /// Give the server-message forwarder a context so incoming game text
    /// wakes the event loop immediately.
    fn set_repaint_context(&self, ctx: egui::Context) {
        if let Ok(mut slot) = self.repaint_ctx.lock() {
            *slot = Some(ctx);
        }
    }

    /// True while any countdown window is actively ticking.
    fn any_countdown_running(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() as i64)
            .unwrap_or(0);
        let adjusted = now + self.app_core.server_time_offset;
        self.app_core
            .ui_state
            .windows
            .values()
            .any(|window| match &window.content {
                WindowContent::Countdown(countdown) => countdown.end_time > adjusted,
                _ => false,
            })
    }

    /// Floating search bar shown while in Search mode (Ctrl+F). Matching
    /// segments highlight via the theme selection color in text windows.
    fn render_search_bar(&mut self, ctx: &egui::Context) {
        if self.app_core.ui_state.input_mode != InputMode::Search {
            return;
        }

        // Count matching lines across visible text content, including the
        // active tab of tabbed windows (read-only pass before the window
        // closure takes mutable borrows). The scan is cached: buffer
        // generations only move when content changes, so an idle search bar
        // costs a fingerprint pass instead of a full-buffer rescan per frame.
        let query = self
            .app_core
            .ui_state
            .search_input
            .trim()
            .to_ascii_lowercase();
        let match_count = if query.is_empty() {
            0
        } else {
            let contents = || {
                self.app_core
                    .ui_state
                    .windows
                    .values()
                    .filter_map(|window| match &window.content {
                        WindowContent::Text(content)
                        | WindowContent::Inventory(content)
                        | WindowContent::Reserve(content)
                        | WindowContent::Spells(content) => Some(content),
                        WindowContent::TabbedText(tabbed) => tabbed
                            .tabs
                            .get(tabbed.active_tab_index)
                            .map(|tab| &tab.content),
                        _ => None,
                    })
            };
            // Order-independent content fingerprint (windows is a HashMap).
            // Active tab indices are mixed in so switching tabs invalidates
            // the cache even when two tabs share generation and length.
            let tab_switch_salt: u64 = self
                .app_core
                .ui_state
                .windows
                .values()
                .filter_map(|window| match &window.content {
                    WindowContent::TabbedText(tabbed) => Some(tabbed.active_tab_index as u64),
                    _ => None,
                })
                .fold(0u64, |acc, index| {
                    acc.wrapping_add(index.wrapping_mul(0x517c_c1b7_2722_0a95))
                });
            let fingerprint = contents().fold(tab_switch_salt, |acc, content| {
                acc.wrapping_add(content.generation)
                    .wrapping_add((content.lines.len() as u64).wrapping_mul(0x9e37_79b9))
            });
            match &self.search_match_cache {
                Some((cached_query, cached_fingerprint, cached_count))
                    if *cached_query == query && *cached_fingerprint == fingerprint =>
                {
                    *cached_count
                }
                _ => {
                    let count = contents()
                        .flat_map(|content| content.lines.iter())
                        .filter(|line| {
                            line.segments.iter().any(|segment| {
                                Self::find_ascii_ci(&segment.text, &query, 0).is_some()
                            })
                        })
                        .count();
                    self.search_match_cache = Some((query.clone(), fingerprint, count));
                    count
                }
            }
        };

        let mut close = false;
        egui::Window::new("gui_search_bar")
            .id(egui::Id::new("gui_search_bar"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 36.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Find:");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.app_core.ui_state.search_input)
                            .desired_width(200.0),
                    );
                    if self.search_bar_needs_focus {
                        response.request_focus();
                        self.search_bar_needs_focus = false;
                    }
                    if query.is_empty() {
                        ui.weak("type to highlight matches");
                    } else {
                        ui.weak(format!("{} matching lines", match_count));
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });

        if close {
            self.app_core.clear_search_mode();
        }
    }

    fn drag_modifier_from_config(key: &str) -> egui::Modifiers {
        match key.trim().to_ascii_lowercase().as_str() {
            "alt" => egui::Modifiers::ALT,
            "shift" => egui::Modifiers::SHIFT,
            _ => egui::Modifiers::CTRL,
        }
    }

    /// Item drag-and-drop: floating hint while dragging, and window-level
    /// drop resolution mirroring the TUI `_drag` protocol. Link-level drop
    /// targets consume the payload during rendering, so this fallback only
    /// fires for drops on window bodies or empty space.
    fn handle_link_drag_drop(
        &mut self,
        ctx: &egui::Context,
        zone_window_rects: &[GuiZoneWindowRect],
    ) {
        if !egui::DragAndDrop::has_any_payload(ctx) {
            return;
        }
        let pointer = ctx.input(|input| {
            input
                .pointer
                .interact_pos()
                .or_else(|| input.pointer.latest_pos())
        });

        if let (Some(payload), Some(pointer_pos)) =
            (egui::DragAndDrop::payload::<LinkData>(ctx), pointer)
        {
            let name = if payload.text.trim().is_empty() {
                payload.noun.clone()
            } else {
                payload.text.clone()
            };
            egui::Area::new(egui::Id::new("gui_link_drag_hint"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pointer_pos + Vec2::new(14.0, 14.0))
                .interactable(false)
                .show(ctx, |ui| {
                    ui.label(format!("Dragging: {}", name));
                });
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }

        if !ctx.input(|input| input.pointer.any_released()) {
            return;
        }
        let Some(payload) = egui::DragAndDrop::take_payload::<LinkData>(ctx) else {
            return;
        };
        let Some(pointer_pos) = pointer else {
            return;
        };

        // Later-rendered windows draw on top; prefer them for the hit test.
        let mut target: Option<String> = None;
        for entry in zone_window_rects.iter().rev() {
            if !entry.rect.contains(pointer_pos) {
                continue;
            }
            // Grouped windows resolve to the member under the pointer
            // (a hand group is one window but two drop targets).
            if self.group_for_tab(&entry.tab_key).is_some() {
                let member_rects: Option<Vec<(String, Rect)>> = ctx
                    .data(|data| data.get_temp(Self::group_member_rects_id(&entry.tab_key)));
                if let Some(member) = member_rects.iter().flatten().find_map(|(name, rect)| {
                    rect.contains(pointer_pos).then_some(name.as_str())
                }) {
                    target = Some(self.drag_drop_target_for_window(member));
                    break;
                }
            }
            let Some(window_name) = self
                .available_tabs
                .get(&entry.tab_key)
                .map(|tab| tab.window_name.clone())
            else {
                continue;
            };
            target = Some(self.drag_drop_target_for_window(&window_name));
            break;
        }

        let target = target.unwrap_or_else(|| "drop".to_string());
        let command = format!("_drag #{} {}", payload.exist_id, target);
        self.dispatch_raw_command(command);
    }

    /// The `_drag` protocol target a drop on this window's body maps to.
    fn drag_drop_target_for_window(&self, window_name: &str) -> String {
        let Some(window) = self.app_core.ui_state.windows.get(window_name) else {
            return "drop".to_string();
        };
        let name_lower = window_name.to_ascii_lowercase();
        match &window.content {
            WindowContent::Hand { .. } if name_lower.contains("left") => "left".to_string(),
            WindowContent::Hand { .. } if name_lower.contains("right") => "right".to_string(),
            WindowContent::Inventory(_) => "wear".to_string(),
            WindowContent::Container { container_title } => {
                match self
                    .app_core
                    .game_state
                    .objects
                    .find_container(container_title)
                {
                    // command_target is stow-correct (plain id = "#stow").
                    Some(container) => format!("#{}", container.command_target()),
                    None => "drop".to_string(),
                }
            }
            _ => "drop".to_string(),
        }
    }

    /// Add a window from a layout template (menu `__ADD__<template>` path).
    /// The new window is picked up as a dock tab on the next frame by
    /// refresh_available_tabs_if_needed.
    fn add_window_from_template(&mut self, template: &str) {
        match self.app_core.layout.add_window(template) {
            Ok(_) => {
                // Templates with auto-generated names (spacers, custom tabbed
                // windows) end up as the last layout entry.
                let window_def = self
                    .app_core
                    .layout
                    .get_window(template)
                    .cloned()
                    .or_else(|| self.app_core.layout.windows.last().cloned());
                if let Some(window_def) = window_def {
                    let actual_name = window_def.name().to_string();
                    self.app_core.add_new_window(
                        &window_def,
                        INITIAL_LAYOUT_WIDTH,
                        INITIAL_LAYOUT_HEIGHT,
                    );
                    self.app_core.schedule_layout_autosave();
                    self.app_core
                        .add_system_message(&format!("Window '{}' added.", actual_name));
                    // Blank custom widgets start unconfigured (e.g. a countdown
                    // with no feed id renders as nothing) — drop the user
                    // straight into the editor, like the TUI does.
                    if template.ends_with("_custom") {
                        self.open_window_editor(Some(&actual_name));
                    }
                } else {
                    self.app_core.add_system_message(&format!(
                        "Window '{}' added but its definition could not be retrieved.",
                        template
                    ));
                }
            }
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Failed to add window: {}", err));
            }
        }
    }

    fn switch_tabbed_tab(&mut self, window_name: &str, index: usize) {
        if let Some(window) = self.app_core.ui_state.windows.get_mut(window_name) {
            if let WindowContent::TabbedText(tabbed) = &mut window.content {
                if index < tabbed.tabs.len() {
                    tabbed.active_tab_index = index;
                    tabbed.tabs[index].has_unread = false;
                    self.app_core.needs_render = true;
                }
            }
        }
    }

    /// Cycle or jump tabs on tabbedtext windows. Applies to every tabbedtext
    /// window (there is usually exactly one).
    fn cycle_tabbed_tabs(&mut self, forward: bool) {
        let mut any = false;
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::TabbedText(tabbed) = &mut window.content {
                let count = tabbed.tabs.len();
                if count == 0 {
                    continue;
                }
                let next = if forward {
                    (tabbed.active_tab_index + 1) % count
                } else {
                    (tabbed.active_tab_index + count - 1) % count
                };
                tabbed.active_tab_index = next;
                tabbed.tabs[next].has_unread = false;
                any = true;
            }
        }
        if any {
            self.app_core.needs_render = true;
        } else {
            self.app_core
                .add_system_message("No tabbed windows to cycle.");
        }
    }

    fn goto_unread_tab(&mut self) {
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::TabbedText(tabbed) = &mut window.content {
                if let Some(index) = tabbed.tabs.iter().position(|tab| tab.has_unread) {
                    tabbed.active_tab_index = index;
                    tabbed.tabs[index].has_unread = false;
                    self.app_core.needs_render = true;
                    return;
                }
            }
        }
        self.app_core.add_system_message("No unread tabs.");
    }

    /// Handle `action:zone:<zone>:<op>` from `.header`/`.footer`/`.leftbar`/
    /// `.rightbar` — show, hide, or toggle a shell zone. Macroable via
    /// keybinds and hotbar buttons like any other dot-command.
    fn handle_zone_action(&mut self, rest: &str) -> bool {
        let Some((zone, op)) = rest.split_once(':') else {
            return false;
        };
        let shown_now = match zone {
            "header" => self.shell_layout.header_visible,
            "footer" => self.shell_layout.footer_visible,
            "leftbar" => !self.shell_layout.left_sidebar_collapsed,
            "rightbar" => !self.shell_layout.right_sidebar_collapsed,
            _ => return false,
        };
        let shown = match op {
            "on" => true,
            "off" => false,
            "toggle" => !shown_now,
            _ => return false,
        };
        if shown != shown_now {
            match zone {
                "header" => self.shell_layout.header_visible = shown,
                "footer" => self.shell_layout.footer_visible = shown,
                "leftbar" => self.shell_layout.left_sidebar_collapsed = !shown,
                "rightbar" => self.shell_layout.right_sidebar_collapsed = !shown,
                _ => unreachable!(),
            }
            self.layout_dirty = true;
        }
        true
    }

    /// Dispatch an `action:*` string from a popup-menu item (menu items
    /// carry strings). The typed path is [`Self::handle_ui_action`]; this
    /// is the single string bridge into it. Returns false only for
    /// unparseable strings — a menu-wiring bug.
    fn handle_action_string(&mut self, action: &str) -> bool {
        match crate::data::UiAction::parse(action) {
            Some(action) => {
                self.handle_ui_action(action);
                true
            }
            None => false,
        }
    }

    /// Perform a [`UiAction`] in the GUI. The match is EXHAUSTIVE on
    /// purpose: adding a UiAction variant forces every frontend to decide
    /// — implement it or answer with a redirect — so actions can never
    /// silently die again (see the dot-command parity audit).
    fn handle_ui_action(&mut self, action: crate::data::UiAction) {
        use crate::data::UiAction as A;
        match action {
            A::WindowList => {
                // Core renders the list; round-trip through the command.
                let _ = self.app_core.send_command(".windows".to_string());
            }
            A::SetTheme(name) => self.apply_theme_by_name(&name),
            A::SetSkin(name) => self.apply_skin_by_name(&name),
            A::Skins => self.list_skins_to_window(),
            A::MakeSkin(name) => self.make_skin_scaffold(&name),
            A::ReloadSkin => match self.ui_settings.active_skin.clone() {
                Some(name) => {
                    self.skin_state.force_reload();
                    self.app_core
                        .add_system_message(&format!("Reloading skin '{}'.", name));
                }
                None => {
                    self.app_core
                        .add_system_message("No skin active. Use .setskin <name> first.");
                }
            },
            A::SorterEdit => self.open_sorter_editor(),
            A::SnapDebug => {
                self.snap_debug = !self.snap_debug;
                self.app_core.add_system_message(if self.snap_debug {
                    "Snap debug trace ON: drag/resize center windows, then read \
                     ~/.vellum-fe/vellum-fe.log (lines tagged 'snapdbg'). \
                     Toggle off with .snapdebug."
                } else {
                    "Snap debug trace off."
                });
            }
            A::Settings => self.open_settings_editor(),
            A::Highlights => self.open_highlight_editor(None),
            A::AddHighlight => {
                self.open_highlight_editor(None);
                self.open_highlight_form_new();
            }
            A::EditHighlight(name) => match name.as_deref() {
                Some(name) => self.open_highlight_editor(Some(name)),
                None => self.open_highlight_editor(None),
            },
            A::Keybinds => self.open_keybind_editor(),
            A::MenuKeybinds => self.open_menu_keybind_editor(),
            A::Controller => {
                #[cfg(feature = "gamepad")]
                self.open_controller_editor();
                #[cfg(not(feature = "gamepad"))]
                self.app_core
                    .add_system_message("This build has no gamepad support.");
            }
            A::Hotbars => self.open_hotbar_editor(),
            A::AddKeybind => {
                self.open_keybind_editor();
                self.open_keybind_form_new();
            }
            A::Colors => self.open_colors_editor(),
            A::AddColor => self.open_palette_form_new(),
            A::UiColors => self.open_ui_colors_editor(),
            A::SpellColors => self.open_spell_colors_editor(),
            A::AddSpellColor => self.open_spell_form_new(),
            A::Themes => self.open_theme_browser(),
            A::EditTheme => {
                let base = self.current_theme.clone();
                self.open_theme_editor(&base);
            }
            A::EditWindow(name) => match name.as_deref() {
                Some(name) => self.open_window_editor(Some(name)),
                None => self.open_window_editor(None),
            },
            A::NextTab => self.cycle_tabbed_tabs(true),
            A::PrevTab => self.cycle_tabbed_tabs(false),
            A::NextUnread => self.goto_unread_tab(),
            A::HideWindow(Some(name)) => {
                let key = self
                    .app_core
                    .ui_state
                    .windows
                    .get(&name)
                    .and_then(|window| Self::tab_key_for_window(&name, window));
                match key {
                    Some(key) => self.hide_tab(key),
                    None => self
                        .app_core
                        .add_system_message(&format!("Window '{}' not found.", name)),
                }
            }
            // Bare `.hidewindow` (no name) asks for a picker: the Windows
            // manager IS the show/hide picker here.
            A::HideWindow(None) => self.open_known_windows_editor(),
            // `.streams` and the Streams & Custom Windows panel are the
            // same surface; the TUI stream-menu actions land there too.
            A::Streams
            | A::CustomWindows
            | A::StreamActions(_)
            | A::StreamPickWindow(_)
            | A::StreamRoute { .. }
            | A::StreamSubscribe { .. }
            | A::StreamNewWindow(_) => self.open_custom_windows_editor(),
            A::Zone { zone, op } => {
                let _ = self.handle_zone_action(&format!("{}:{}", zone.as_str(), op.as_str()));
            }
            A::SetPalette | A::ResetPalette => {
                self.app_core.add_system_message(
                    "Terminal palette commands do not apply to the GUI; use .themes instead.",
                );
            }
            A::LoadLayoutToml(_) => {
                // This action comes from the Layouts menu, which lists TUI TOML
                // layouts — those don't apply here. GUI checkpoints are the
                // .savelayout/.loadlayout commands below.
                self.app_core.add_system_message(
                    "TOML layouts are a TUI feature. In the GUI, use .savelayout <name> and .loadlayout <name> for named layouts.",
                );
            }
            // Layout capability hooks (parity plan D3): same command
            // names as the TUI, GUI-native window-snapshot checkpoints.
            A::SaveLayout(name) => {
                let name = name.unwrap_or_else(|| "default".to_string());
                if !is_valid_layout_name(&name) {
                    self.app_core.add_system_message(
                        "Layout names use letters, digits, '-' and '_' only.",
                    );
                    return;
                }
                let Some(layout) = self.build_layout_snapshot() else {
                    self.app_core
                        .add_system_message("Could not snapshot the current layout.");
                    return;
                };
                match save_named_layout(
                    &layout,
                    &self.layout_profile,
                    &self.layout_character,
                    &name,
                ) {
                    Ok(()) => self.app_core.add_system_message(&format!(
                        "Saved GUI layout '{}'. Load it with .loadlayout {}",
                        name, name
                    )),
                    Err(err) => self
                        .app_core
                        .add_system_message(&format!("Failed to save layout: {}", err)),
                }
            }
            A::LoadLayout(None) => {
                self.app_core
                    .add_system_message("Usage: .loadlayout <name>");
                self.list_layout_checkpoints();
            }
            A::LoadLayout(Some(name)) => {
                match load_named_layout(&self.layout_profile, &self.layout_character, &name) {
                    Ok(layout) => {
                        self.apply_layout_snapshot(&layout);
                        self.app_core
                            .add_system_message(&format!("Loaded GUI layout '{}'.", name));
                    }
                    Err(err) => {
                        self.app_core
                            .add_system_message(&format!("Failed to load layout: {}", err));
                        self.list_layout_checkpoints();
                    }
                }
            }
            A::ListLayouts => self.list_layout_checkpoints(),
            A::ResizeLayout => {
                self.app_core.add_system_message(
                    ".resize refits the TUI's cell layout; the GUI has no cell grid.",
                );
            }
            A::SaveSkin(name) => {
                if !is_valid_layout_name(&name) {
                    self.app_core.add_system_message(
                        "Skin names use letters, digits, '-' and '_' only.",
                    );
                    return;
                }
                match self.compile_appearance_to_skin(&name) {
                    Ok(()) => self.app_core.add_system_message(&format!(
                        "Saved skin '{}' from the current appearance. Activate it with .setskin {}",
                        name, name
                    )),
                    Err(err) => self
                        .app_core
                        .add_system_message(&format!("Failed to save skin: {}", err)),
                }
            }
            // UI packs ride the core commands with the live GUI layout
            // attached (export) / installed (import).
            A::UiExport(args) => {
                let extra = self
                    .build_layout_snapshot()
                    .and_then(|layout| serde_json::to_vec_pretty(&layout).ok())
                    .map(|bytes| {
                        vec![(
                            crate::core::uipack::GUI_LAYOUT_ENTRY.to_string(),
                            bytes,
                        )]
                    })
                    .unwrap_or_default();
                self.app_core.uiexport_with(&args, extra);
            }
            A::UiImport(args) => {
                if let Some((pack_name, bytes)) = self.app_core.uiimport(&args) {
                    match serde_json::from_slice(&bytes) {
                        Ok(layout) => match save_named_layout(
                            &layout,
                            &self.layout_profile,
                            &self.layout_character,
                            &pack_name,
                        ) {
                            Ok(()) => self.app_core.add_system_message(&format!(
                                "GUI layout installed — load it with .loadlayout {pack_name}"
                            )),
                            Err(err) => self.app_core.add_system_message(&format!(
                                "Pack's GUI layout could not be saved: {err}"
                            )),
                        },
                        Err(err) => self.app_core.add_system_message(&format!(
                            "Pack's GUI layout did not parse: {err}"
                        )),
                    }
                }
            }
            A::WebUiPicker => {
                let _ = self.handle_webui_action("action:webui");
            }
            A::WebUiOff => {
                let _ = self.handle_webui_action("action:webui:off");
            }
            A::WebUiOpen(page) => {
                let _ = self.handle_webui_action(&format!("action:webui:open:{page}"));
            }
            A::KnownWindows => self.open_known_windows_editor(),
            A::AddWindowPicker => {
                let mut items = self.app_core.build_add_window_menu();
                // Surface the custom-window authoring panel at the top of the
                // Add Widget menu (GUI-local; the shared core menu builder stays
                // untouched). The show/hide list lives under Windows > Show/Hide.
                items.insert(
                    0,
                    PopupMenuItem {
                        text: "Streams & Custom Windows…".to_string(),
                        command: "action:customwindows".to_string(),
                        disabled: false,
                    },
                );
                if items.is_empty() {
                    self.app_core
                        .add_system_message("No window templates available to add.");
                } else {
                    self.close_all_popup_menus();
                    self.app_core.ui_state.popup_menu = Some(PopupMenu::new(items, (8, 4)));
                    self.app_core.ui_state.input_mode = InputMode::Menu;
                }
            }
            // TUI-menu-only actions the GUI's own menus never emit; keep
            // them meaningful if one ever arrives.
            A::CreateWindow(_) | A::ShowWindow(_) => {
                self.open_known_windows_editor();
                self.app_core.add_system_message(
                    "Use the Windows manager to add and show windows in the GUI.",
                );
            }
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

    /// `origin` names the detached tab whose viewport the click came from
    /// (None for the root window); a resulting popup menu renders there.
    fn handle_link_click(&mut self, click: GuiLinkClick, origin: Option<TabKey>) {
        if click.link_data.exist_id == Self::QUICKBAR_SWITCH_SENTINEL {
            self.app_core.ui_state.active_quickbar_id = Some(click.link_data.noun.clone());
            return;
        }
        if click.link_data.exist_id == Self::TABBED_SWITCH_SENTINEL {
            if let Some((window_name, index)) = click.link_data.noun.split_once('|') {
                if let Ok(index) = index.parse::<usize>() {
                    let window_name = window_name.to_string();
                    self.switch_tabbed_tab(&window_name, index);
                }
            }
            return;
        }
        if click.link_data.exist_id == Self::LINK_DROP_SENTINEL {
            if let Some((dragged, target)) = click.link_data.noun.split_once('|') {
                if !dragged.is_empty() && !target.is_empty() && dragged != target {
                    let command = format!("_drag #{} #{}", dragged, target);
                    self.dispatch_raw_command(command);
                }
            }
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
                self.popup_menu_host = origin;
                self.app_core.request_menu(exist_id, noun, click.click_pos)
            }
        };
        // Direct links carrying a dot command (e.g. the map's native ".go2")
        // are client commands, not game text.
        if outbound.starts_with('.') {
            self.dispatch_command(outbound);
        } else {
            self.dispatch_raw_command(outbound);
        }
    }
}

impl eframe::App for VellumGuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.app_core.perf_stats.record_frame();
        self.capture_main_viewport(&ctx);
        // Fire delayed startup music once its deadline passes; ask egui for
        // a frame at the deadline so a slow idle repaint can't stretch the
        // configured delay.
        if let Some(at) = self.startup_music_at {
            let now = std::time::Instant::now();
            if now >= at {
                self.startup_music_at = None;
                if let Some(ref player) = self.app_core.sound_player {
                    if let Err(e) = player.play_from_sounds_dir("wizard_music", None) {
                        tracing::debug!("Startup music not available: {e}");
                    }
                }
            } else {
                ctx.request_repaint_after(at - now);
            }
        }
        // Publish the color-emoji toggle for this frame's text painters.
        color_emoji::set_enabled(self.app_core.config.ui.color_emoji);
        // Publish the configured item-drag modifier for link renderers.
        ctx.data_mut(|data| {
            data.insert_temp(
                Self::drag_modifier_data_id(),
                Self::drag_modifier_from_config(&self.app_core.config.ui.drag_modifier_key),
            );
        });
        // While an item drag is in flight, sweeping the pointer across text
        // must not select it.
        let dragging_item = egui::DragAndDrop::has_any_payload(&ctx);
        ctx.global_style_mut(|style| style.interaction.selectable_labels = !dragging_item);
        // Families set last frame are installed by now (set_fonts only takes
        // effect at the next begin_pass), so it is safe for widgets to use them.
        if let Some(families) = self.pending_font_families.take() {
            self.registered_font_families = families;
        }
        if !self.fonts_applied {
            self.fonts_applied = true;
            let mut window_fonts: Vec<FontRef> = self
                .tab_settings
                .values()
                .map(|settings| settings.font_primary.clone())
                .collect();
            // Fonts assigned on shared layout defs need registering too.
            window_fonts.extend(
                self.app_core
                    .layout
                    .windows
                    .iter()
                    .filter_map(|window| window.base().font_family.clone().map(FontRef::Named)),
            );
            let fonts = theme::build_font_definitions(&self.ui_font, &window_fonts);
            self.pending_font_families = Some(
                fonts
                    .families
                    .keys()
                    .filter_map(|family| match family {
                        egui::FontFamily::Name(name) => Some(name.to_string()),
                        _ => None,
                    })
                    .collect(),
            );
            ctx.set_fonts(fonts);
            // The new families become usable next frame; make sure it happens
            // promptly instead of waiting for the idle repaint tick.
            ctx.request_repaint();
        }
        self.apply_theme_if_changed(&ctx);
        // Pool frames referenced by per-window overrides load lazily; tell
        // the skin state which ones are in use before it applies.
        self.skin_state.set_needed_pool_frames(
            self.tab_settings
                .values()
                .filter_map(|settings| settings.skin_frame.clone())
                .chain(self.ui_settings.default_frame.clone()),
        );
        self.skin_state.set_status_icon_config(
            self.ui_settings.status_icons.set.as_deref(),
            &self.ui_settings.status_icons.overrides,
        );
        self.skin_state
            .set_compass_set(self.ui_settings.compass_set.as_deref());
        self.skin_state.set_needed_pool_backgrounds(
            self.tab_settings
                .values()
                .filter_map(|settings| settings.background_image.clone())
                .chain(self.ui_settings.default_background.clone()),
        );
        // Pool images named by hand-widget icon states and hotbar button
        // icons load with the skin (declared loads, like frames).
        let hand_state_images = self
            .app_core
            .layout
            .windows
            .iter()
            .filter_map(|def| match def {
                crate::config::WindowDef::Hand { data, .. } => Some(&data.states),
                _ => None,
            })
            .flatten()
            .filter_map(|state| match &state.icon {
                Some(crate::data::IconRef::Image { path }) => Some(path.clone()),
                _ => None,
            });
        let hotbar_images = self
            .app_core
            .config
            .hotbars
            .bars
            .iter()
            .flat_map(|bar| &bar.buttons)
            .flat_map(|button| {
                button
                    .icon
                    .iter()
                    .chain(button.default_style.iter().filter_map(|s| s.icon.as_ref()))
                    .chain(button.states.iter().filter_map(|s| s.style.icon.as_ref()))
            })
            .filter_map(|icon| match &icon.icon {
                crate::data::IconRef::Image { path } => Some(path.clone()),
                _ => None,
            });
        let needed_pool_icons: Vec<String> =
            hand_state_images.chain(hotbar_images).collect();
        self.skin_state.set_needed_pool_icons(needed_pool_icons);
        self.skin_state.set_grayscale(
            self.ui_settings.status_icons.any_gray(),
            self.ui_settings.doll_grayscale,
        );
        self.skin_state.apply_if_changed(
            &ctx,
            self.ui_settings.active_skin.as_deref(),
            self.ui_settings.doll_image.as_deref(),
        );
        self.apply_ui_sizing(&ctx);
        // Prime the item classifier while &mut self is available; render
        // paths (hotbar/hand conditions) read the immutable cache.
        let _ = self.app_core.gameobj_data();
        self.pump_server_messages();
        // Feed-injected dot-commands (<vellumCmd> from Lich scripts) run
        // through the same dispatch as typed commands.
        for command in self.app_core.take_pending_client_commands() {
            self.dispatch_command(command);
        }
        // Keep painting while the map worker, mapdb download, or walk
        // executor is busy so results and progress appear without waiting
        // for user input or game text (travel needs ticks for RT waits).
        if self.app_core.map.has_pending()
            || self.app_core.map_updater.in_flight()
            || self.app_core.travel.is_traveling()
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
        self.sync_room_windows_from_components();
        self.refresh_available_tabs_if_needed();
        let monitor_bounds = Self::monitor_bounds_from_ctx(&ctx);
        self.last_monitor_bounds = Some(monitor_bounds);

        if self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.sync_numpad_capture_keys(frame);
        #[cfg(feature = "gamepad")]
        self.poll_gamepad(&ctx);
        self.handle_global_input(&ctx, frame);

        if self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let detached_before_frame = self.detached_tab_keys();
        let mut open_windows_manager = false;
        let mut zone_actions = GuiWindowActions::default();
        let mut visible_zone_rects: Vec<(GuiShellZone, Rect)> = Vec::new();
        let mut zone_window_rects: Vec<GuiZoneWindowRect> = Vec::new();

        egui::Panel::top("gui_shell_toolbar")
            .resizable(false)
            .exact_size(30.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Flat toolbar: no resting chip background on the zone
                    // toggles / Windows menu, hover highlight only. Scoped to
                    // this row; the dropdown menus keep normal visuals.
                    ui.visuals_mut().widgets.inactive.weak_bg_fill =
                        egui::Color32::TRANSPARENT;
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

                    // U6: the "Windows" button opens the single Windows
                    // manager (show/hide + zone + add-window, grouped by
                    // category) instead of an inline menu.
                    if ui.button("Windows").clicked() {
                        open_windows_manager = true;
                    }
                });
            });

        let separator_style = self.ui_settings.zone_separators;
        if self.shell_layout.header_visible {
            egui::Panel::top("gui_shell_header")
                .resizable(false)
                .exact_size(self.shell_layout.header_height)
                .show_separator_line(separator_style == ZoneSeparatorStyle::Shown)
                .frame(
                    egui::Frame::default()
                        .inner_margin(egui::Margin::ZERO)
                        .outer_margin(egui::Margin::ZERO),
                )
                .show(ui, |ui| {
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
                            if separator_style == ZoneSeparatorStyle::Hover {
                                ui.painter().hline(
                                    header_zone_rect.x_range(),
                                    header_zone_rect.max.y - 0.75,
                                    egui::Stroke::new(1.5, ui.visuals().window_stroke.color),
                                );
                            }
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

        // The command input is a normal dockable window now
        // (TabKey::CommandInput). This fixed panel appears only when no such
        // tab actually renders this frame — missing window def, hidden tab,
        // or the tab parked in a collapsed/hidden shell zone — so the input
        // can never be lost.
        if !self.command_input_tab_rendered() {
            egui::Panel::bottom("gui_command_input").show(ui, |ui| {
                let seed = self.command_input.clone();
                // Fixed fallback panel: not a movable window, no grip.
                Self::render_command_input_widget(ui, &seed, false);
            });
        }

        if self.shell_layout.footer_visible {
            egui::Panel::bottom("gui_shell_footer")
                .resizable(false)
                .exact_size(self.shell_layout.footer_height)
                .show_separator_line(separator_style == ZoneSeparatorStyle::Shown)
                .frame(
                    egui::Frame::default()
                        .inner_margin(egui::Margin::ZERO)
                        .outer_margin(egui::Margin::ZERO),
                )
                .show(ui, |ui| {
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
                            if separator_style == ZoneSeparatorStyle::Hover {
                                ui.painter().hline(
                                    footer_zone_rect.x_range(),
                                    footer_zone_rect.min.y + 0.75,
                                    egui::Stroke::new(1.5, ui.visuals().window_stroke.color),
                                );
                            }
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
            .show(ui, |ui| {
            let root = ui.max_rect();
            if !root.is_finite() || root.width() <= 24.0 || root.height() <= 24.0 {
                return;
            }

            self.shell_layout.sanitize(root.width());
            let min_center_width = 220.0;
            let left_width = if self.shell_layout.left_sidebar_collapsed {
                0.0
            } else {
                self.shell_layout.left_sidebar_width
            };
            let right_width = if self.shell_layout.right_sidebar_collapsed {
                0.0
            } else {
                self.shell_layout.right_sidebar_width
            };
            // Display-only squeeze on narrow windows; the persisted widths
            // stay untouched so the layout springs back when the window
            // grows again (the old math floored collapsed sidebars back to
            // life, inverted the center, and baked the squeeze into the
            // saved layout).
            let (left_width, right_width) = zones::squeezed_sidebar_widths(
                root.width(),
                min_center_width,
                left_width,
                right_width,
            );

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
            if separator_style == ZoneSeparatorStyle::Shown {
                if let Some(rect) = left_rect {
                    ui.painter()
                        .vline(rect.max.x, root.y_range(), sidebar_divider_stroke);
                }
                if let Some(rect) = right_rect {
                    ui.painter()
                        .vline(rect.min.x, root.y_range(), sidebar_divider_stroke);
                }
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
                // D5 gutter: an always-on-top strip owned by the zone, so
                // the grab survives windows parked flush on the boundary
                // (free-placement sidebars have no per-window width band).
                let splitter_response =
                    egui::Area::new(egui::Id::new("gui_left_sidebar_splitter"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(splitter.min)
                        .show(ui.ctx(), |gutter_ui| {
                            gutter_ui
                                .allocate_exact_size(
                                    splitter.size(),
                                    egui::Sense::click_and_drag(),
                                )
                                .1
                        })
                        .inner;
                if splitter_response.hovered() || splitter_response.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    if separator_style == ZoneSeparatorStyle::Hover {
                        ui.painter()
                            .vline(rect.max.x, root.y_range(), sidebar_divider_stroke);
                    }
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
                // D5 gutter — see the left-sidebar twin above.
                let splitter_response =
                    egui::Area::new(egui::Id::new("gui_right_sidebar_splitter"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(splitter.min)
                        .show(ui.ctx(), |gutter_ui| {
                            gutter_ui
                                .allocate_exact_size(
                                    splitter.size(),
                                    egui::Sense::click_and_drag(),
                                )
                                .1
                        })
                        .inner;
                if splitter_response.hovered() || splitter_response.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    if separator_style == ZoneSeparatorStyle::Hover {
                        ui.painter()
                            .vline(rect.min.x, root.y_range(), sidebar_divider_stroke);
                    }
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
        });

        let detached_link_clicks = self.render_detached_viewports(&ctx);
        self.render_map_explorer(&ctx);

        let zone_drop_result = self.render_zone_drop_overlay(&ctx, &visible_zone_rects);
        self.render_window_move_overlay(&ctx, &visible_zone_rects);
        self.handle_link_drag_drop(&ctx, &zone_window_rects);

        if open_windows_manager {
            self.open_known_windows_editor();
        }
        if let Some(drop_result) = zone_drop_result {
            self.apply_zone_drop(drop_result, &visible_zone_rects);
        }
        if let Some(request) = zone_actions.window_menu_request {
            // While a window is in Move mode the pointer belongs to placement.
            if self.window_move_state.is_none() {
                self.close_all_popup_menus();
                self.window_context_menu = Some(request);
                self.window_context_menu_just_opened = true;
            }
        }
        for name in std::mem::take(&mut zone_actions.webui_closes) {
            self.close_webui_window(&name);
        }
        for click in zone_actions.link_clicks {
            self.handle_link_click(click, None);
        }
        for (origin, click) in detached_link_clicks {
            self.handle_link_click(click, Some(origin));
        }
        self.render_window_context_popup(&ctx);
        self.render_popup_menus(&ctx);
        self.render_interact_overlay(&ctx);
        #[cfg(feature = "gamepad")]
        self.render_controller_wheel(&ctx);
        #[cfg(feature = "gamepad")]
        self.render_controller_overlay(&ctx);
        self.render_injuries_popup(&ctx);
        self.render_editors(&ctx);
        self.render_server_dialog(&ctx);
        self.render_search_bar(&ctx);

        // Interactions queued by WebUI panels during this frame go out over
        // the bridge socket (button clicks, input submits, row clicks).
        let webui_events = Self::take_pending_webui_events(&ctx);
        if !webui_events.is_empty() {
            if let Some(bridge) = &self.webui_bridge {
                for event in webui_events {
                    bridge.send(event);
                }
            }
        }

        // Images the panels asked for: /files/ srcs fetch over the bridge's
        // HTTP endpoint (cookie-authed); anything else fails visibly.
        for src in Self::take_pending_webui_fetches(&ctx) {
            if self.webui_fetches_inflight.contains(&src) {
                continue;
            }
            match (&self.webui_endpoint, &self.webui_event_tx) {
                (Some((host, port, token)), Some(event_tx)) if src.starts_with("/files/") => {
                    self.webui_fetches_inflight.insert(src.clone());
                    crate::webui::fetch_image(
                        self._runtime.handle(),
                        host.clone(),
                        *port,
                        token.clone(),
                        src,
                        event_tx.clone(),
                    );
                }
                _ => {
                    let reason = if src.starts_with("/files/") {
                        "not connected to the Lich WebUI".to_string()
                    } else {
                        "external image URLs are not supported yet".to_string()
                    };
                    Self::set_webui_image(
                        &ctx,
                        src,
                        webui_panel::WebUiImageState::Failed(reason),
                    );
                }
            }
        }

        // Pages an image_map right-click asked to open (popup:).
        for page in Self::take_pending_webui_page_opens(&ctx) {
            if !page.is_empty() {
                self.open_webui_page(&page);
            }
        }
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
        // Same debounce for the core TOML layout (WindowDef data: streams,
        // added/removed windows). Previously only written on exit, so a
        // crash lost window-def edits; this mirrors the TUI's autosave tick.
        self.app_core.tick_layout_autosave();

        // Drain the command-input echo (see render_command_input_widget):
        // the widget renders inside &self paths, so buffer edits and
        // history/submit events arrive here once per frame.
        let echo: Option<CommandInputEcho> = ctx.data_mut(|data| {
            let value = data.get_temp(CommandInputEcho::id());
            if value.is_some() {
                data.remove::<CommandInputEcho>(CommandInputEcho::id());
            }
            value
        });
        if let Some(echo) = echo {
            if let Some(text) = echo.text {
                self.command_input = text;
            }
            if echo.history_prev {
                self.history_previous();
                self.command_cursor_to_end(&ctx);
            } else if echo.history_next {
                self.history_next();
                self.command_cursor_to_end(&ctx);
            }
            if echo.submit {
                self.submit_command();
            }
        }

        // Focus-follows rule: any click that no text widget captured returns
        // keyboard focus to the command input, so the player can always type
        // without hunting for the input bar. Editors, dialogs, and the search
        // bar keep focus while their fields are in use; keybind capture is
        // exempt so the captured key doesn't also type into the input.
        if let Some(input_id) = self.command_input_id {
            let nothing_focused = ctx.memory(|memory| memory.focused().is_none());
            if nothing_focused
                && !self.keybind_capture_armed()
                && !self.menu_keybind_capture_armed()
                && !self.hotbar_capture_armed()
            {
                ctx.memory_mut(|memory| memory.request_focus(input_id));
            }
        }

        // Input events and incoming server data (via the forwarder task) wake
        // the loop immediately; the periodic repaint only drives countdown
        // ticks and background polling, so idle CPU stays near zero.
        let repaint_after = if self.any_countdown_running() {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(500)
        };
        ctx.request_repaint_after(repaint_after);
    }

    fn on_exit(&mut self) {
        // Stop the async writer first (drop the sender, drain the queue) so
        // the final synchronous save below can never interleave with a
        // queued write.
        self.layout_save_tx = None;
        if let Some(worker) = self.layout_save_worker.take() {
            let _ = worker.join();
        }
        // Flush any debounced layout changes while the app is still intact.
        self.save_layout_state();
        // Persist the config layout (WindowDef data: streams, feed ids,
        // added/removed windows) and session cache. Without this, closing
        // the window with the X button silently discarded every window-def
        // edit — only the `quit` command path saved them.
        self.app_core.save_on_quit();
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
    // Restore the last session's OS window geometry. Opening at a smaller
    // default size would clamp the saved per-window rects (which were laid
    // out against the old geometry) on the first frames.
    let (profile_id, character_id) = VellumGuiApp::resolve_layout_ids(&app_core.config);
    let persisted_layout = load_layout(&profile_id, &character_id).ok();
    // The saved geometry is in egui points measured while the persisted UI
    // zoom was active. egui-winit multiplies ViewportBuilder sizes by the
    // *current* zoom factor, but the main window is created before the
    // first frame applies the persisted zoom (it is still 1.0 here), so
    // pre-scale ourselves. Without this, a zoomed-out UI grows by 1/zoom
    // on every restart (and a zoomed-in one shrinks).
    let saved_zoom = persisted_layout
        .as_ref()
        .map(|layout| layout.ui_settings.zoom_factor.clamp(0.5, 3.0))
        .unwrap_or(1.0);
    let saved_viewport = persisted_layout.and_then(|layout| layout.main_viewport);
    let mut viewport = ViewportBuilder::default().with_title(window_title.clone());
    match saved_viewport {
        Some(saved) => {
            viewport = viewport.with_inner_size([
                saved.inner_size[0] * saved_zoom,
                saved.inner_size[1] * saved_zoom,
            ]);
            if let Some(pos) = saved.outer_pos {
                viewport = viewport.with_position([pos[0] * saved_zoom, pos[1] * saved_zoom]);
            }
            if saved.maximized {
                viewport = viewport.with_maximized(true);
            }
        }
        None => {
            viewport = viewport.with_inner_size([1200.0, 800.0]);
        }
    }
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
        Box::new(move |cc| {
            // Virtualized text windows intentionally re-address screen rects
            // to different (content-stable) widget ids as they scroll; egui's
            // debug-build id-instability lint paints red warning boxes over
            // exactly that pattern, so opt out. Release builds compile the
            // lint out entirely.
            #[cfg(debug_assertions)]
            cc.egui_ctx.global_style_mut(|style| {
                style.debug.warn_if_rect_changes_id = false;
            });
            app.set_repaint_context(cc.egui_ctx.clone());
            Ok(Box::new(app))
        }),
    )
    .map_err(|err| anyhow!("Failed to run GUI frontend: {}", err))
}

#[cfg(test)]
mod tests {
    use super::widgets::parse_hex_color;
    use super::{
        AppShortcut, FontRef, GlobalDispatchTarget, GuiLinkDispatch, TabKey, TabSettings,
        VellumGuiApp,
    };
    use crate::config::{AppKeybinds, Config, KeyBindAction, MacroAction, TargetListConfig};
    use crate::core::state::{Creature, Player};
    use crate::data::{LinkData, SpanType, TextSegment};
    use crate::data::input::{KeyCode, KeyEvent, KeyModifiers};
    use eframe::egui::{Color32, Pos2};
    use std::collections::HashMap;

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
    fn test_global_dispatch_fires_core_action_binds() {
        // f6 = "interact_mode" (and the TTS F-keys) are Action binds, not
        // macros — they must dispatch globally in the GUI.
        let key_event = KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE);
        let mut keybind_map = HashMap::new();
        keybind_map.insert(
            key_event,
            KeyBindAction::Action("interact_mode".to_string()),
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
    fn test_global_dispatch_leaves_widget_actions_to_the_gui() {
        // esc = "clear_search" is widget-level: dispatching it globally
        // would steal Esc from every editor and popup.
        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut keybind_map = HashMap::new();
        keybind_map.insert(
            key_event,
            KeyBindAction::Action("clear_search".to_string()),
        );

        let target = VellumGuiApp::resolve_global_dispatch_target(
            key_event,
            &keybind_map,
            &AppKeybinds::default(),
            false,
        );
        assert!(!matches!(target, Some(GlobalDispatchTarget::Macro(_))));
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
    fn test_room_line_from_links_builds_one_labeled_line() {
        let lines = VellumGuiApp::room_line_from_links(
            "Obvious exits: ",
            ["north", "south"].iter().map(|dir| {
                (
                    dir.to_string(),
                    Some(crate::data::LinkData {
                        exist_id: "_direct_".to_string(),
                        noun: dir.to_string(),
                        text: dir.to_string(),
                        coord: None,
                    }),
                )
            }),
        );

        assert_eq!(lines.len(), 1);
        let texts: Vec<&str> = lines[0]
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(texts, vec!["Obvious exits: ", "north", ", ", "south"]);
        assert!(lines[0].segments[1].link_data.is_some());
        assert_eq!(
            lines[0].segments[1].span_type,
            crate::data::SpanType::Link
        );
        assert!(lines[0].segments[2].link_data.is_none());
    }

    #[test]
    fn test_room_line_from_links_empty_entries_yield_no_line() {
        let lines = VellumGuiApp::room_line_from_links("Also here: ", std::iter::empty());
        assert!(lines.is_empty());
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
        let cfg = TargetListConfig::default();
        let creature = Creature {
            name: "a goblin".to_string(),
            noun: Some("goblin".to_string()),
            id: "#101".to_string(),
            status: Some("stunned".to_string()),
            flags: None,
        };

        let start = VellumGuiApp::format_target_line(&creature, &cfg, "start");
        assert_eq!(start, "[stu] a goblin");

        let end = VellumGuiApp::format_target_line(&creature, &cfg, "end");
        assert_eq!(end, "a goblin [stu]");
    }

    #[test]
    fn test_format_target_line_joins_crtr_statuses() {
        let cfg = TargetListConfig::default();
        let creature = Creature {
            name: "a sea nymph".to_string(),
            noun: Some("nymph".to_string()),
            id: "#607736".to_string(),
            // Structured flags beat the legacy text status
            status: Some("stunned".to_string()),
            flags: Some(crate::core::state::CreatureFlags {
                statuses: vec!["stunned".to_string(), "prone".to_string()],
                hostile: true,
                ..Default::default()
            }),
        };

        let line = VellumGuiApp::format_target_line(&creature, &cfg, &cfg.status_position);
        assert_eq!(line, "a sea nymph [stu,prn]");
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
    fn test_is_valid_target_filters_dead_and_excluded_nouns() {
        // Filtering is now canonical on Creature::is_valid_target; the GUI
        // routes through it. Default excluded_nouns = ["arm", "coal"].
        let cfg = TargetListConfig::default();
        let dead_creature = Creature {
            name: "a dead goblin".to_string(),
            noun: Some("goblin".to_string()),
            id: "#1".to_string(),
            status: Some("dead".to_string()),
            flags: None,
        };
        let body_part_creature = Creature {
            name: "an arm".to_string(),
            noun: Some("arm".to_string()),
            id: "#2".to_string(),
            status: None,
            flags: None,
        };

        assert!(!dead_creature.is_valid_target(&cfg.excluded_nouns));
        assert!(!body_part_creature.is_valid_target(&cfg.excluded_nouns));
    }

    #[test]
    fn test_is_valid_target_keeps_live_creatures() {
        let cfg = TargetListConfig::default();
        let live_creature = Creature {
            name: "a forest troll".to_string(),
            noun: Some("troll".to_string()),
            id: "#3".to_string(),
            status: Some("stunned".to_string()),
            flags: None,
        };

        assert!(live_creature.is_valid_target(&cfg.excluded_nouns));
    }

    fn migration_test_layout() -> crate::config::Layout {
        use crate::config::{BorderSides, CompassWidgetData, TextWidgetData, WindowBase, WindowDef};
        let base = |name: &str| WindowBase {
            name: name.to_string(),
            row: crate::data::geometry::Row::new(0),
            col: crate::data::geometry::Col::new(0),
            rows: crate::data::geometry::Height::new(10),
            cols: crate::data::geometry::Width::new(40),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: true,
            title: None,
            title_position: "top-left".to_string(),
            background_color: None,
            text_color: None,
            transparent_background: false,
            locked: false,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            visibility: crate::config::WindowVisibility::Shown,
            binding: None,
            content_align: None,
            tts_speak: false,
            text_size: None,
            font_family: None,
        };
        crate::config::Layout {
            windows: vec![
                WindowDef::Text {
                    base: base("main"),
                    data: TextWidgetData {
                        streams: vec!["main".to_string()],
                        buffer_size: 100,
                        wordwrap: true,
                        show_timestamps: false,
                        timestamp_position: None,
                        compact: false,
                    },
                },
                WindowDef::Compass {
                    base: base("compass"),
                    data: CompassWidgetData {
                        active_color: None,
                        inactive_color: None,
                    },
                },
            ],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
        }
    }

    fn migration_name_of(key: &TabKey) -> Option<String> {
        match key {
            TabKey::TextMain => Some("main".to_string()),
            TabKey::Compass => Some("compass".to_string()),
            _ => None,
        }
    }

    #[test]
    fn test_migrate_tab_settings_moves_values_onto_defs() {
        let mut layout = migration_test_layout();
        let mut settings = HashMap::new();
        settings.insert(
            TabKey::TextMain,
            TabSettings {
                text_size: Some(16.0),
                font_primary: FontRef::Named("Fira Code".to_string()),
                wrap_text: false,
                ..Default::default()
            },
        );
        // Compass has no wordwrap field: wrap stays a legacy GUI setting.
        settings.insert(
            TabKey::Compass,
            TabSettings {
                wrap_text: false,
                ..Default::default()
            },
        );

        let (layout_changed, gui_changed) = VellumGuiApp::migrate_tab_settings_to_layout(
            &mut settings,
            &mut layout,
            migration_name_of,
        );
        assert!(layout_changed);
        assert!(gui_changed);

        assert_eq!(layout.windows[0].base().text_size, Some(16.0));
        assert_eq!(
            layout.windows[0].base().font_family.as_deref(),
            Some("Fira Code")
        );
        let crate::config::WindowDef::Text { data, .. } = &layout.windows[0] else {
            panic!("main window should be a text def");
        };
        assert!(!data.wordwrap);

        let migrated = settings.get(&TabKey::TextMain).unwrap();
        assert_eq!(migrated.text_size, None);
        assert!(matches!(migrated.font_primary, FontRef::SystemDefault));
        assert!(migrated.wrap_text);

        // Compass: nothing moved onto the def; legacy wrap preserved.
        assert_eq!(layout.windows[1].base().text_size, None);
        assert!(!settings.get(&TabKey::Compass).unwrap().wrap_text);

        // Idempotent: a second run changes nothing.
        let (layout_changed, gui_changed) = VellumGuiApp::migrate_tab_settings_to_layout(
            &mut settings,
            &mut layout,
            migration_name_of,
        );
        assert!(!layout_changed);
        assert!(!gui_changed);
    }

    #[test]
    fn test_migrate_tab_settings_existing_def_value_wins() {
        let mut layout = migration_test_layout();
        layout.windows[0].base_mut().text_size = Some(20.0);
        layout.windows[0].base_mut().font_family = Some("Consolas".to_string());

        let mut settings = HashMap::new();
        settings.insert(
            TabKey::TextMain,
            TabSettings {
                text_size: Some(16.0),
                font_primary: FontRef::Named("Fira Code".to_string()),
                ..Default::default()
            },
        );

        let (layout_changed, gui_changed) = VellumGuiApp::migrate_tab_settings_to_layout(
            &mut settings,
            &mut layout,
            migration_name_of,
        );
        assert!(!layout_changed);
        // The legacy fields still empty out so they stop shadowing the def.
        assert!(gui_changed);
        assert_eq!(layout.windows[0].base().text_size, Some(20.0));
        assert_eq!(
            layout.windows[0].base().font_family.as_deref(),
            Some("Consolas")
        );
        let migrated = settings.get(&TabKey::TextMain).unwrap();
        assert_eq!(migrated.text_size, None);
        assert!(matches!(migrated.font_primary, FontRef::SystemDefault));
    }

    #[test]
    fn test_migrate_tab_settings_ignores_unmapped_tabs() {
        let mut layout = migration_test_layout();
        let mut settings = HashMap::new();
        settings.insert(
            TabKey::Vitals,
            TabSettings {
                text_size: Some(16.0),
                ..Default::default()
            },
        );

        let (layout_changed, gui_changed) = VellumGuiApp::migrate_tab_settings_to_layout(
            &mut settings,
            &mut layout,
            migration_name_of,
        );
        assert!(!layout_changed);
        assert!(!gui_changed);
        assert_eq!(settings.get(&TabKey::Vitals).unwrap().text_size, Some(16.0));
    }
}
