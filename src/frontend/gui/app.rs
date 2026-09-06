use super::persistence::{
    list_named_layouts, load_layout, load_named_layout, migrate_legacy_named_layouts, save_layout,
    save_named_layout, FontRef, GuiLayoutFileV1, GuiUiSettings, MainViewportState, TabGroup,
    TabSettings, TabSettingsEntry, ViewportState, ZoneSeparatorStyle,
};
use super::skin;
use super::{TabId, TabKey};
use crate::cmdlist::CmdList;
use crate::config::is_valid_layout_name;
use crate::config::{AppKeybinds, Config, KeyBindAction, TargetListConfig};
use crate::core::AppCore;
use crate::data::{
    InputMode, LinkData, PopupMenu, PopupMenuItem, StyledLine, TabbedTextContent, TextContent,
    TextSegment, WidgetType, WindowContent, WindowState,
};
use crate::network::{LichConnection, RawLogger, ServerMessage};
use anyhow::{anyhow, Context, Result};
use eframe::egui;
use eframe::egui::{Color32, Pos2, Rect, RichText, Vec2, ViewportBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

mod alert_overlay;
mod borders;
mod color_emoji;
mod command_input;
mod custom_emoji_render;
mod detached;
mod dialogs;
mod dock;
pub(crate) mod editors;
#[cfg(feature = "gamepad")]
mod gamepad;
mod global_input;
mod interact;
mod launch;
mod layout_persistence;
mod map_explorer;
mod menus;
mod render_settings;
mod room_sync;
mod search_bar;
mod server_pump;
mod skins;
mod snap;
mod status_icons;
mod tabs;
pub(crate) mod theme;
mod webui_bridge;
mod webui_panel;
pub(crate) mod widgets;
mod window_config;
mod window_manager;
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
/// Title-bar band height bounds, shared by the egui bar override and the
/// skinned sprite band — two different clamps here once let the caption sit
/// on art it shouldn't.
const TITLE_BAR_MIN_HEIGHT: f32 = 12.0;
const TITLE_BAR_MAX_HEIGHT: f32 = 48.0;
/// Idle delay before a dirty layout is flushed to disk. Saves are blocking
/// on the UI thread, so writes must not happen per interaction.
const LAYOUT_SAVE_DEBOUNCE: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
struct GuiTab {
    id: TabId,
    window_name: String,
}

/// Which persisted layout a snapshot is being built for. The one file format
/// serves two purposes with different hidden-window semantics:
/// - `Autosave`: the per-character continuity slot. Hidden windows keep their
///   defs/rects/hidden state so an unhide after restart restores placement.
/// - `Checkpoint`: a named `.savelayout` — an exact, portable copy of the
///   visible arrangement. GUI-hidden windows are stripped entirely so loading
///   on another profile never carries them over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayoutSaveMode {
    Autosave,
    Checkpoint,
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
    /// Creature-card art cache (creaturefield widget); loading happens in
    /// the update loop, renderers only read. None in contexts without a
    /// skin state.
    creature_art: Option<skin::SharedCreatureArt>,
    /// Stage scene (backdrop + scenery props) for the creaturefield
    /// renderer. None everywhere in the game today; the Studio Stage sets
    /// it, and the client's future scenes feature will feed the same slot.
    scene: Option<std::sync::Arc<crate::config::scenes::StageScene>>,
    /// Current command-input buffer, only for command-input windows. Render
    /// paths are `&self`; edits flow back via `CommandInputEcho`.
    command_input_seed: Option<String>,
    /// Untyped suffix of the newest matching history entry.
    command_input_completion: Option<String>,
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
    /// Server "now" for ticking effect bars between refreshes; None when
    /// ui.effect_countdown is off (bars show the server's last snapshot).
    effect_countdown_now: Option<i64>,
    /// This frame's sibling-instance status, for multiaccount windows.
    /// Snapshotted once per frame and shared by Arc: settings are rebuilt per
    /// window, and cloning a six-peer map per window per frame is waste.
    multiaccount_peers:
        std::sync::Arc<std::collections::BTreeMap<u16, crate::core::multiaccount::PeerStatus>>,
}

impl WidgetRenderSettings {
    /// Studio Stage settings: the creature-field renderer reads only
    /// `creature_art`; the rest are the neutral defaults.
    pub(super) fn for_creature_field(
        creature_art: skin::SharedCreatureArt,
        scene: Option<std::sync::Arc<crate::config::scenes::StageScene>>,
    ) -> Self {
        Self {
            text_size: 14.0,
            map_zoom: None,
            font_family: egui::FontFamily::Proportional,
            effects_bar_height: 18.0,
            bar_corner_radius: 3.0,
            auto_contrast_bar_text: false,
            wrap_text: true,
            vitals: super::persistence::VitalsConfig::default(),
            background: None,
            skin_art: None,
            creature_art: Some(creature_art),
            scene,
            command_input_seed: None,
            command_input_completion: None,
            command_input_drag_gutter: false,
            hand_icon_size: 24.0,
            gray_inactive_icons: false,
            gray_icon_overrides: Default::default(),
            doll_grayscale: false,
            effect_countdown_now: None,
            multiaccount_peers: Default::default(),
        }
    }
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
    completion_accepted: bool,
}

impl CommandInputEcho {
    pub(super) fn id() -> egui::Id {
        egui::Id::new("gui_command_input_echo")
    }

    fn is_empty(&self) -> bool {
        self.text.is_none()
            && !self.submit
            && !self.history_prev
            && !self.history_next
            && !self.completion_accepted
    }
}

/// The keys currently bound to the command-input actions, resolved from the
/// keybind config once per frame and read by `render_command_input_widget`.
/// This is what makes send_command / previous_command / next_command /
/// cursor_clear_line honor REBINDS instead of being locked to Enter/↑/↓ —
/// the config is the single source of truth. Defaults (Enter/↑/↓) are always
/// also accepted so a config missing an entry never disables the input.
#[derive(Clone, Default)]
pub(super) struct CommandInputKeys {
    pub submit: Vec<egui::Key>,
    pub history_prev: Vec<egui::Key>,
    pub history_next: Vec<egui::Key>,
    pub clear_line: Vec<(egui::Key, egui::Modifiers)>,
    // Editing actions: bound combos are consumed BEFORE the TextEdit sees
    // them and applied manually (config beats egui built-ins for bound
    // keys). Each op also accepts its combo + Shift as the selection-
    // extending variant, mirroring the TUI.
    pub cursor_left: Vec<(egui::Key, egui::Modifiers)>,
    pub cursor_right: Vec<(egui::Key, egui::Modifiers)>,
    pub cursor_word_left: Vec<(egui::Key, egui::Modifiers)>,
    pub cursor_word_right: Vec<(egui::Key, egui::Modifiers)>,
    pub cursor_home: Vec<(egui::Key, egui::Modifiers)>,
    pub cursor_end: Vec<(egui::Key, egui::Modifiers)>,
    pub cursor_backspace: Vec<(egui::Key, egui::Modifiers)>,
    pub cursor_delete: Vec<(egui::Key, egui::Modifiers)>,
    pub cursor_delete_word: Vec<(egui::Key, egui::Modifiers)>,
    pub select_all: Vec<(egui::Key, egui::Modifiers)>,
    pub copy: Vec<(egui::Key, egui::Modifiers)>,
    pub paste: Vec<(egui::Key, egui::Modifiers)>,
}

impl CommandInputKeys {
    pub(super) fn id() -> egui::Id {
        egui::Id::new("gui_command_input_keys")
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
    NextSearchMatch,
    PrevSearchMatch,
    CloseWindow,
}

#[derive(Clone, Debug)]
enum GlobalDispatchTarget {
    Macro(KeyBindAction),
    Shortcut(AppShortcut),
    /// A keybind Action whose behavior lives in the GUI's own widgets
    /// (command history, tab nav, search, window switch) rather than in
    /// `AppCore`. Carries the action name; `try_gui_command_action` runs it.
    /// These are dispatched globally so a *rebound* key reaches them, but the
    /// focused command-input widget still consumes the default Enter/↑/↓ first.
    GuiCommandAction(String),
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
    MenuRequest {
        exist_id: String,
        noun: String,
    },
    /// Web link: open in the default browser (http/https only).
    OpenUrl(String),
}

#[derive(Clone, Debug)]
pub(super) struct GuiLinkClick {
    pub(super) link_data: LinkData,
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
    /// Dot-command / window-name completion for the input bar (same engine
    /// the TUI model uses; Tab advances it before the history ghost).
    input_completion: crate::frontend::common::CompletionState,
    /// The text our last completion/ghost-accept produced — any divergence
    /// means the user edited and the candidate set is stale.
    input_completion_text: String,
    close_requested: bool,
    detached_tabs: HashMap<TabKey, DetachedWindowState>,
    /// Map Explorer native window (separate OS viewport).
    map_explorer: map_explorer::MapExplorerState,
    /// Watches the other VellumFE instances on this machine. None when the
    /// multi-account display is off or no pairing token is available.
    multiaccount: Option<crate::core::multiaccount::MultiAccountHub>,
    /// Last peer snapshot, refreshed once per frame in update().
    multiaccount_peers:
        std::sync::Arc<std::collections::BTreeMap<u16, crate::core::multiaccount::PeerStatus>>,
    detached_context_menu: Option<DetachedMenuState>,
    /// Which detached tab's viewport hosts the game popup menus. The menu
    /// stack renders inside that OS window (at its local click coords);
    /// None means the root window hosts them.
    popup_menu_host: Option<TabKey>,
    available_tabs: HashMap<TabKey, GuiTab>,
    hidden_tabs: HashSet<TabKey>,
    main_window_rects: HashMap<TabKey, [f32; 4]>,
    /// Persisted edge anchors (snap permanence, P-A1): windows absent here
    /// are free. Solved against the live pane rect every frame at display
    /// time — the solver never writes `main_window_rects`.
    window_anchors: HashMap<TabKey, window_manager::WindowAnchors>,
    /// Per-window size role (P-A3): `Fixed` windows keep their width and
    /// height through every proportional rescale. Persisted beside the
    /// anchors on each rect snapshot entry.
    window_size_roles: HashMap<TabKey, dock::SizeRole>,
    /// This frame's center BASE pane: the center rect as it would be with
    /// no reserved zones open (the space the store's rects live in). The
    /// per-frame P-A3 resolve maps store→current pane from this reference;
    /// gesture writes invert through it. None until the first shell pass.
    center_base_pane: Option<egui::Rect>,
    /// Each zone's pane rect as of its last render pass; the anchor space
    /// for commit-on-detach when anchors are released outside a frame's
    /// solve (context menu).
    last_zone_pane_rects: HashMap<GuiShellZone, Rect>,
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
    /// Pool art graphics (appearance assignments); reloaded when they change.
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
    /// Numpad presses seen this frame, for keybind editors to record.
    ///
    /// Numpad keys arrive through eframe's dedicated channel rather than egui's
    /// event queue, and reading that channel needs `&Frame` — which editors don't
    /// have. `handle_global_input` runs first and stashes them here; an armed editor
    /// drains this later in the same frame.
    frame_numpad_presses: Vec<crate::data::input::KeyEvent>,
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
    /// When the current wheel overlay opened. A release inside the
    /// minimum-open window doesn't close it — a bouncing button contact
    /// would otherwise strobe the overlay open/closed.
    #[cfg(feature = "gamepad")]
    gp_wheel_opened_at: Option<std::time::Instant>,
    /// When the wheel last closed. Movement stays hushed for
    /// [controller_tuning] release_grace_ms afterwards, so releasing the
    /// wheel doesn't also walk a direction.
    #[cfg(feature = "gamepad")]
    gp_wheel_closed_at: Option<std::time::Instant>,
    /// "Spent" latch: set when a wheel fires, cleared only once the aim
    /// stick returns to center. A wheel opened while spent starts in
    /// rearm-until-center, so a still-deflected stick can't instantly
    /// re-aim and re-fire.
    #[cfg(feature = "gamepad")]
    gp_wheel_spent: bool,
    /// Which stick aimed the most recently open wheel (true = right).
    /// The spent latch clears against THIS stick, not the default aim
    /// stick — a wheel whose `stick` override is the movement stick
    /// leaves that stick deflected after close.
    #[cfg(feature = "gamepad")]
    gp_wheel_aim_on_right: bool,
    /// Whether the most recently open wheel aimed with the movement
    /// stick; scopes the release grace to wheels that actually hushed
    /// movement.
    #[cfg(feature = "gamepad")]
    gp_wheel_aim_was_move: bool,
    /// Set when a wheel closes while the aim stick is still deflected: the
    /// stick's normal function (scroll / interact cycle) stays suppressed
    /// until it returns to center once, so releasing the wheel can't also
    /// scroll or cycle from the leftover deflection.
    #[cfg(feature = "gamepad")]
    gp_aim_recenter_needed: bool,
    /// The aim stick has been seen CENTERED at least once since the pad
    /// connected. A stick that reports deflected from the very first frame
    /// is a phantom axis (stale gilrs value after connect/wake) — the live
    /// "aim_y=0.983 forever" stream that pinned the story window at the top
    /// and out-fought the mouse. No scroll until a real center is observed.
    gp_aim_seen_center: bool,
    /// Stale-axis guard for the level-triggered story scroll: the aim
    /// value last seen and when it last changed. A live stick jitters
    /// every few frames; a bit-identical deflected value for seconds is a
    /// frozen driver cache (seen live: aim_y=0.808 for 2,546 straight
    /// frames pinning the story window at the top) and must not scroll.
    #[cfg(feature = "gamepad")]
    gp_aim_prev: (f32, f32),
    #[cfg(feature = "gamepad")]
    gp_aim_last_change: Option<std::time::Instant>,
    #[cfg(feature = "gamepad")]
    gp_aim_stale_logged: bool,
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
    dashboard_editor: Option<editors::DashboardEditorState>,
    jinx_panel: Option<editors::JinxPanelState>,
    /// Chrome for the Skill Goals window (open flag lives in ui_state).
    skill_trainer_panel: editors::SkillTrainerPanelState,
    tab_editor: Option<editors::TabEditorState>,
    custom_windows_editor: Option<editors::CustomWindowsEditorState>,
    known_windows_editor: Option<editors::KnownWindowsEditorState>,
    sorter_editor: Option<editors::SorterEditorState>,
    room_images_editor: Option<editors::RoomImagesEditorState>,
    touch_wheel_editor: Option<editors::TouchWheelEditorState>,
    launcher_editor: Option<editors::LauncherEditorState>,
    doll_calibration: Option<editors::DollCalibrationState>,
    frame_calibration: Option<editors::FrameCalibrationState>,
    creature_calibration: Option<editors::CreatureCalibrationState>,
    scenery_calibration: Option<editors::SceneryCalibrationState>,
    creature_field_editor: Option<editors::CreatureFieldEditorState>,
    pack_editor: Option<editors::PackEditorState>,
    alertpacks_editor: Option<editors::AlertPacksEditorState>,
    /// Editor window Id to raise to the top on the next frame. Set when a
    /// settings command (`.controller`, `.settings`, …) is re-issued while
    /// its editor is already open, so the command surfaces the buried
    /// window instead of silently rebuilding (and wiping) its state.
    pending_editor_raise: Option<egui::Id>,
    search_bar_needs_focus: bool,
    /// Cached search-bar matches for the current target: (target + query
    /// key, content fingerprint, matching line indices).
    search_match_cache: Option<(String, u64, Vec<usize>)>,
    /// Scroll id of the window match-nav last stepped through. The bar
    /// reports position within THIS window, which is not necessarily the
    /// keyboard-focused one (see `step_search_match`).
    search_match_window: Option<String>,
    /// Scroll id of the window the user chose to search in the Find bar.
    /// None = not chosen yet (falls back to the keyboard focus). This is
    /// what decouples "which window am I searching" from window focus, so a
    /// specific tab — thoughts, speech, story — can be searched directly.
    search_target: Option<String>,
    /// Fingerprint of the window set backing `available_tabs`; refresh is
    /// skipped while it is unchanged.
    available_tabs_fingerprint: Option<u64>,
    /// The canvas size the stored window rects are currently anchored to.
    /// Every frame, the loop rescales the store (a pure proportional map —
    /// lossless under composition) from this anchor to the live content size
    /// and re-anchors, so the store is ALWAYS in current-canvas coordinates
    /// by render time: OS resizes track smoothly with no debounce, gestures
    /// write in a consistent space, and a `.savelayout` at any moment records
    /// rects that match its recorded viewport. Loads and `.resize` steer the
    /// system by setting the anchor (file's reference canvas / rect bounding
    /// box) and letting the next frame's apply do the work. None until the
    /// first frame when starting without a persisted layout.
    canonical_canvas: Option<egui::Vec2>,
    /// Live front-to-back stacking order of the main-surface windows, refreshed
    /// each frame from egui's layer order (only `ctx` knows it). The save
    /// snapshot reads this so `visible_tabs` records true z-order instead of an
    /// alphabetical placeholder; back-to-front, i.e. topmost window last.
    current_zorder: Vec<TabKey>,
    /// Stacking order to replay next frame (a layout load carries it in
    /// `visible_tabs`). Applied via `move_to_top` back-to-front, deferred
    /// because restacking needs `ctx`.
    pending_zorder: Option<Vec<TabKey>>,
    /// A single window to raise to the front next frame (switch_current_window
    /// keybind). Deferred like `pending_zorder` because `move_to_top` needs
    /// `ctx`.
    pending_raise_tab: Option<TabKey>,
    /// Search match-navigation cursor (next_search_match / prev_search_match):
    /// the match currently focused, held as an ABSOLUTE line number — the
    /// buffer's `generation` at the moment that line was appended — never a
    /// buffer index. Text buffers are capped ring buffers, so once a window
    /// fills, each new line shifts every existing index down by one; an
    /// index cursor would silently slide onto a different line while the
    /// user pages through matches in a live window. Reset only when the
    /// query or the target window changes. None = not yet stepped.
    search_match_index: Option<u64>,
    /// OS-window geometry to restore for a `.loadlayout` (saved size /
    /// position / maximized), applied in the frame loop via ViewportCommands.
    /// No settle-wait is needed: the per-frame anchor rescale tracks every
    /// intermediate size the OS passes through and lands 1:1 at the target.
    pending_viewport_restore: Option<MainViewportState>,
    /// A legacy `active_skin` found at startup: migrated to a preset on
    /// the first frame (the live-manifest runtime is gone). One-shot.
    startup_skin_migration: Option<String>,
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
    /// Zone window whose size pin is relaxed for the CURRENT press.
    /// Latched when a press starts on/near the window and held until the
    /// mouse releases: a shrink drag moves the grabbed edge away from the
    /// press origin, so re-testing the origin against the current rect
    /// every frame would re-pin the size mid-drag and stall the resize.
    zone_engaged_tab: Option<TabKey>,
    /// True once the CURRENT press has travelled past the click threshold —
    /// sticky for the rest of the press (a drag that pauses, or circles back
    /// near its origin, stays a drag). Cleared on release. Gates the
    /// engagement latch's press_became_drag short-circuit: latch ownership
    /// alone is claimed on a mere press, and treating that as "dragging"
    /// relaxed the size pin on a stationary click — egui re-clamped
    /// content-driven windows (room/targets/spells) to their remembered
    /// desired_size, the rect diverged, and the snap hook popped the grid
    /// on a plain click.
    zone_press_drag_seen: bool,
    /// Sticky per press: set once egui is observed resizing a window this
    /// press (its rendered size diverged from the pinned size), cleared on
    /// release. A resize-edge drag can't be seen through pointer travel —
    /// egui captures/locks the pointer while resizing, so `interact_pos`
    /// freezes and `zone_press_drag_seen` never trips. This flag lets the
    /// size pin relax for a resize the same way `zone_press_drag_seen` does
    /// for a move, so windows can be dragged SMALLER, not only larger.
    zone_resize_active: bool,
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
    /// Bridge events re-emitted by core's pump, forwarded through a
    /// repaint-waking hop like server_rx. Core owns the socket (see
    /// core::app_core::webui); the GUI applies renders to panels.
    webui_rx: Option<mpsc::UnboundedReceiver<crate::webui::WebUiEvent>>,
    /// Pages currently registered on the connected Lich session (GUI-local
    /// mirror for the picker / window-kind logic).
    webui_pages: Vec<crate::data::webui::WebUiPageDescriptor>,
    /// Actions deferred until the handshake/hello completes.
    webui_pending: Vec<WebUiPendingAction>,
    /// True while direct-connected (no Lich): `;ui` commands would reach the
    /// game itself, so the bridge is unavailable.
    is_direct_connection: bool,
    /// Ensures the layout-driven auto-handshake fires once per connect.
    webui_handshake_sent: bool,
    /// Image srcs with a fetch task in flight (dedupes re-queues).
    webui_fetches_inflight: HashSet<String>,

    // --- Reconnect inputs (see `reconnect`) ------------------------------
    /// Retained connection credentials so `.reconnect` can re-establish the
    /// session after a drop. Some = direct mode (re-auths via eAccess); None
    /// = Lich mode (re-attaches to host/port below). Holds the password in
    /// memory exactly as the original connect did.
    reconnect_direct: Option<crate::network::DirectConnectConfig>,
    reconnect_login_key: Option<String>,
    reconnect_host: String,
    reconnect_port: u16,
    /// Feeds the server→UI forwarder that wakes egui. Cloned into each
    /// network task; retaining a clone keeps the forwarder alive and lets a
    /// reconnect spawn a fresh task into the same pipeline.
    network_forward_tx: mpsc::Sender<ServerMessage>,
    /// SSH-launcher progress bridged from the async flow task back to the egui
    /// update loop. The flow (SSH + poll) runs off-thread; each frame we drain
    /// this and surface progress, then attach on Ready. `None` receiver until
    /// the first `.launch`.
    launch_progress_rx: Option<mpsc::UnboundedReceiver<crate::launcher::flow::LaunchProgress>>,
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
        let core_layout_size = (
            initial_width.max(1.0) as u16,
            initial_height.max(1.0) as u16,
        );
        app_core.init_windows(core_layout_size.0, core_layout_size.1);
        // This frontend drains disconnect_requested each frame, so keep-open
        // `.quit` works.
        app_core.detach_quit_supported = true;
        let is_direct_connection = direct.is_some();
        // Core needs the connection mode too: travel's `;go2` fallback can only
        // hand off when there's a Lich listening. The GUI previously kept this
        // to itself, which left the fallback permanently disabled.
        app_core.set_lich_connected(!is_direct_connection);

        let runtime = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

        // Start the web frontend sidecar if enabled (off by default); it
        // runs on this GUI-owned runtime.
        let web_event_rx = if app_core.config.web.should_serve() {
            let _guard = runtime.enter();
            let session_label = app_core
                .config
                .connection
                .character
                .clone()
                .or_else(|| app_core.config.character.clone())
                .unwrap_or_else(|| "default".to_string());
            let (sink, event_rx) = crate::frontend::web::start_with_classic_maps(
                &app_core.config.web,
                session_label,
                app_core.map.classic_maps(),
            );
            app_core.enable_remote(sink);
            Some(event_rx)
        } else {
            None
        };

        // Multi-account status. Rides the same sidecar: every instance on
        // this machine reads one pairing-token file, so no pairing step is
        // needed, and the registry is what makes discovery automatic. The
        // hub requires BOTH the feature flag AND the sidecar actually
        // serving -- the sidecar is what publishes OUR registry entry, and a
        // hub without it would be a silent one-way watcher (sees everyone,
        // seen by no one). should_serve() is implied by `multiaccount` today,
        // but the predicate states the dependency rather than relying on
        // that implication holding forever.
        let multiaccount = if app_core.config.web.multiaccount && app_core.config.web.should_serve()
        {
            let _guard = runtime.enter();
            match crate::config::Config::load_or_create_web_token() {
                Ok(token) => Some(crate::core::multiaccount::MultiAccountHub::start(token)),
                Err(err) => {
                    tracing::warn!("multi-account display disabled (no web token): {err}");
                    None
                }
            }
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

        // Retain everything a later `.reconnect` needs. `server_tx` feeds the
        // forwarder that wakes egui; keep a clone so a reconnect can spawn a
        // fresh network task into the same pipeline (and so the forwarder
        // never sees all senders drop between connections).
        let network_forward_tx = server_tx.clone();
        let reconnect_direct = direct.clone();
        let reconnect_login_key = login_key.clone();
        let reconnect_host = host.clone();
        let reconnect_port = port;

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

        // Named checkpoints moved from per-character dirs into the shared
        // ~/.vellum-fe/layouts/ pool; sweep any stragglers in before the
        // session starts so .loadlayout/.layouts see them.
        let migrated_checkpoints = migrate_legacy_named_layouts();
        if !migrated_checkpoints.is_empty() {
            let names: Vec<&str> = migrated_checkpoints
                .iter()
                .map(|(_, pool_name)| pool_name.as_str())
                .collect();
            app_core.add_system_message(&format!(
                "Moved {} saved layout(s) into the shared layouts folder: {}",
                names.len(),
                names.join(", ")
            ));
        }

        let persisted_layout = load_layout(&layout_profile, &layout_character).ok();
        let available_tabs = Self::collect_available_tabs(&app_core);
        let dock::RestoredLayoutState {
            hidden_tabs,
            main_window_rects,
            window_anchors,
            window_size_roles,
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

        // The live-manifest skin runtime is gone: any active_skin still on
        // record (layout copy or appearance store) is a legacy skin to
        // migrate into a preset on the first frame. Taking it here clears
        // both stores once the layout/appearance persist.
        let mut ui_settings = ui_settings;
        let startup_skin_migration = ui_settings
            .active_skin
            .take()
            .or_else(|| app_core.config.appearance.active_skin.clone());
        let seeded_active_skin = startup_skin_migration.is_some();

        // Set art moved into per-set folders at startup; per-indicator icon
        // overrides name a pool path directly, so rewrite the ones whose art
        // moved or those icons silently go blank.
        let rewrote_icon_paths = ui_settings.status_icons.rewrite_pool_paths();

        // Legacy GUI files stored per-window text size/font/wrap in
        // TabSettings; those now live on the shared layout defs. Migrate
        // once: marking both stores dirty persists the move on both sides.
        let mut tab_settings = tab_settings;
        let (migrated_layout, migrated_gui) =
            Self::migrate_tab_settings_to_layout(&mut tab_settings, &mut app_core.layout, |key| {
                available_tabs.get(key).map(|tab| tab.window_name.clone())
            });
        if migrated_layout {
            app_core.schedule_layout_autosave();
        }

        let command_history = Self::load_command_history(app_core.config.character.as_deref());

        // Anchor the restored rects to the canvas they were saved against:
        // the OS window is restored toward the saved viewport size, but a
        // changed monitor or a maximized-open can land it anywhere, and the
        // first frame's anchor rescale maps the rects onto whatever size
        // actually materializes (identity when they match).
        let canonical_canvas = persisted_layout
            .as_ref()
            .map(|layout| Self::layout_reference_canvas(layout, &main_window_rects));

        // Replay the saved stacking order on the first frame (needs `ctx`).
        // `visible_tabs` is recorded back-to-front; filtered to tabs that
        // actually exist this session so a cross-character load doesn't try to
        // raise a window that isn't here.
        let pending_zorder = persisted_layout
            .as_ref()
            .and_then(Self::dock_snapshot_from_layout)
            .map(|snapshot| {
                snapshot
                    .visible_tabs
                    .into_iter()
                    .filter(|key| available_tabs.contains_key(key))
                    .collect::<Vec<_>>()
            })
            .filter(|order| !order.is_empty());

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
            input_completion: crate::frontend::common::CompletionState::new(),
            input_completion_text: String::new(),
            close_requested: false,
            detached_tabs,
            map_explorer: Default::default(),
            multiaccount,
            multiaccount_peers: Default::default(),
            detached_context_menu: None,
            popup_menu_host: None,
            available_tabs,
            hidden_tabs,
            main_window_rects,
            window_anchors,
            window_size_roles,
            center_base_pane: None,
            last_zone_pane_rects: HashMap::new(),
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
            layout_dirty: migrated_gui || seeded_active_skin || rewrote_icon_paths,
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
            gp_wheel_opened_at: None,
            #[cfg(feature = "gamepad")]
            gp_wheel_closed_at: None,
            #[cfg(feature = "gamepad")]
            gp_wheel_spent: false,
            #[cfg(feature = "gamepad")]
            gp_wheel_aim_on_right: false,
            #[cfg(feature = "gamepad")]
            gp_wheel_aim_was_move: false,
            #[cfg(feature = "gamepad")]
            gp_aim_recenter_needed: false,
            gp_aim_seen_center: false,
            #[cfg(feature = "gamepad")]
            gp_aim_prev: (0.0, 0.0),
            #[cfg(feature = "gamepad")]
            gp_aim_last_change: None,
            #[cfg(feature = "gamepad")]
            gp_aim_stale_logged: false,
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
            frame_numpad_presses: Vec::new(),
            #[cfg(feature = "gamepad")]
            controller_editor: None,
            hotbar_editor: None,
            hand_icons_editor: None,
            colors_editor: None,
            theme_browser: None,
            theme_editor: None,
            indicator_templates_editor: None,
            dashboard_editor: None,
            jinx_panel: None,
            skill_trainer_panel: editors::SkillTrainerPanelState::default(),
            tab_editor: None,
            custom_windows_editor: None,
            known_windows_editor: None,
            sorter_editor: None,
            room_images_editor: None,
            touch_wheel_editor: None,
            launcher_editor: None,
            doll_calibration: None,
            frame_calibration: None,
            creature_calibration: None,
            scenery_calibration: None,
            creature_field_editor: None,
            pack_editor: None,
            alertpacks_editor: None,
            pending_editor_raise: None,
            search_bar_needs_focus: false,
            search_match_cache: None,
            search_match_window: None,
            search_target: None,
            available_tabs_fingerprint: None,
            canonical_canvas,
            current_zorder: Vec::new(),
            pending_zorder,
            pending_raise_tab: None,
            search_match_index: None,
            // Startup already restores the OS window natively; this only
            // serves runtime `.loadlayout`.
            pending_viewport_restore: None,
            startup_skin_migration,
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
            zone_engaged_tab: None,
            zone_press_drag_seen: false,
            zone_resize_active: false,
            zone_snap_drag: None,
            zone_snap_guides: Vec::new(),
            snap_debug: false,
            last_monitor_bounds: None,
            main_viewport_state,
            webui_rx: None,
            webui_pages: Vec::new(),
            webui_pending: Vec::new(),
            is_direct_connection,
            webui_handshake_sent: false,
            webui_fetches_inflight: HashSet::new(),
            reconnect_direct,
            reconnect_login_key,
            reconnect_host,
            reconnect_port,
            network_forward_tx,
            launch_progress_rx: None,
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
                let member_rects: Option<Vec<(String, Rect)>> =
                    ctx.data(|data| data.get_temp(Self::group_member_rects_id(&entry.tab_key)));
                if let Some(member) = member_rects
                    .iter()
                    .flatten()
                    .find_map(|(name, rect)| rect.contains(pointer_pos).then_some(name.as_str()))
                {
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
                    // straight into the context menu that configures it.
                    if template.ends_with("_custom") {
                        self.open_window_menu_for_window(&actual_name);
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
            A::HarmonySkin(name) => self.write_harmony_skin_default(&name),
            A::ReloadSkin => {
                self.skin_state.force_reload();
                self.app_core.add_system_message("Reloading pool art.");
            }
            A::RoomImagesEdit => self.open_room_images_editor(),
            A::AlertPacks => self.open_alertpacks_editor(),
            A::SorterEdit => self.open_sorter_editor(),
            A::CreatureFieldEdit => self.open_creature_field_editor(),
            A::TouchWheelEditor => self.open_touch_wheel_editor(),
            A::Reconnect => self.reconnect(),
            A::Launch(character) => self.start_launch(&character),
            A::LauncherEditor => self.open_launcher_editor(),
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
            A::PerformanceDump => {
                let extra = self.egui_internals_report();
                self.app_core
                    .write_perf_dump(crate::performance::PerfFrontend::Gui, extra);
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
            A::EditStatusAbbrev => {
                // Status abbreviations are global target_list settings; they
                // live in Settings ▸ Targets.
                self.open_settings_editor_at("Targets");
            }
            A::Controller => {
                #[cfg(feature = "gamepad")]
                self.open_controller_editor();
                #[cfg(not(feature = "gamepad"))]
                self.app_core
                    .add_system_message("This build has no gamepad support.");
            }
            A::Hotbars => self.open_hotbar_editor(),
            A::JinxPanel => self.open_jinx_panel(),
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
                // The Window Editor is gone: per-window settings live in the
                // window's right-click menu, and the Windows catalog is the
                // all-windows list.
                Some(name) => {
                    let name = name.to_string();
                    self.open_window_menu_for_window(&name);
                }
                None => self.open_known_windows_editor(),
            },
            A::NextTab => self.cycle_tabbed_tabs(true),
            A::PrevTab => self.cycle_tabbed_tabs(false),
            A::NextUnread => self.goto_unread_tab(),
            A::HideWindow(Some(name)) => {
                // Hide = the Windows-window uncheck (core visibility layer).
                if self.app_core.ui_state.windows.contains_key(&name) {
                    self.core_hide_window_by_name(&name);
                } else {
                    self.app_core
                        .add_system_message(&format!("Window '{}' not found.", name));
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
            A::LoadLayoutToml(name) => {
                // TOML cell layouts are the TUI's format; the GUI's Layouts
                // menu lists its own JSON checkpoints from the same shared
                // folder, so route the request to the matching GUI layout.
                self.handle_ui_action(A::LoadLayout {
                    name: Some(name),
                    keep_skin: false,
                });
            }
            // Layout capability hooks (parity plan D3): same command
            // names as the TUI, GUI-native window-snapshot checkpoints.
            A::SaveLayout(name) => {
                let name = name.unwrap_or_else(|| "default".to_string());
                if !is_valid_layout_name(&name) {
                    self.app_core
                        .add_system_message("Layout names use letters, digits, '-' and '_' only.");
                    return;
                }
                let Some(layout) = self.build_layout_snapshot(LayoutSaveMode::Checkpoint) else {
                    self.app_core
                        .add_system_message("Could not snapshot the current layout.");
                    return;
                };
                match save_named_layout(&layout, &name) {
                    Ok(()) => self.app_core.add_system_message(&format!(
                        "Saved GUI layout '{}'. Load it with .loadlayout {}",
                        name, name
                    )),
                    Err(err) => self
                        .app_core
                        .add_system_message(&format!("Failed to save layout: {}", err)),
                }
            }
            A::LoadLayout { name: None, .. } => {
                self.app_core
                    .add_system_message("Usage: .loadlayout <name> [--keep-skin]");
                self.list_layout_checkpoints();
            }
            A::LoadLayout {
                name: Some(name),
                keep_skin,
            } => {
                match load_named_layout(&name) {
                    Ok(layout) => {
                        self.apply_layout_snapshot(&layout, keep_skin);
                        // Persist the loaded arrangement to the auto-save slot
                        // RIGHT NOW, not just via the 2s debounce. Loading a
                        // layout is a deliberate, infrequent choice, and a user
                        // who X-es or kills the window before the debounce fires
                        // would otherwise lose it — the exact "it never saves my
                        // .loadlayout" report. Also persist the core TOML
                        // (window defs) so a rebuilt window set survives too.
                        self.save_layout_state();
                        self.app_core.autosave_layout();
                        self.layout_dirty = false;
                        self.layout_dirty_since = None;
                        self.app_core.add_system_message(&format!(
                            "Loaded GUI layout '{}'{}.",
                            name,
                            if keep_skin {
                                " (keeping your skin/theme)"
                            } else {
                                ""
                            }
                        ));
                    }
                    Err(err) => {
                        self.app_core
                            .add_system_message(&format!("Failed to load layout: {}", err));
                        self.list_layout_checkpoints();
                    }
                }
            }
            A::ListLayouts => self.list_layout_checkpoints(),
            A::AnchorInfer => self.anchor_infer(),
            A::ResizeLayout(None) => {
                // The GUI tracks the canvas automatically (per-frame anchor
                // rescale), so bare `.resize` keeps only its FILL intent:
                // stretch the arrangement's bounding box out to the full
                // window, absorbing any dead space the user's manual
                // arrangement left. Re-anchoring to the bbox makes the next
                // frame's rescale do exactly that.
                if self.main_window_rects.is_empty() {
                    self.app_core
                        .add_system_message("No positioned windows to refit.");
                } else {
                    self.canonical_canvas =
                        Some(Self::rects_bounding_canvas(&self.main_window_rects));
                    self.app_core
                        .add_system_message("Refitting windows to fill the current size.");
                }
            }
            A::ResizeLayout(Some(name)) => {
                // Geometry-only restore: take the named checkpoint's window
                // positions/sizes (rescaled into the current window) and
                // nothing else — a "make it look arranged like X" that keeps
                // this session's windows, skin, and OS geometry.
                match load_named_layout(&name) {
                    Ok(layout) => {
                        let saved: HashMap<TabKey, [f32; 4]> =
                            Self::dock_snapshot_from_layout(&layout)
                                .map(|snapshot| {
                                    snapshot
                                        .main_window_rects
                                        .into_iter()
                                        .map(|entry| (entry.key, entry.rect))
                                        .collect()
                                })
                                .unwrap_or_default();
                        if saved.is_empty() {
                            self.app_core.add_system_message(&format!(
                                "Layout '{}' has no window geometry to adopt.",
                                name
                            ));
                            return;
                        }
                        let file_ref = Self::layout_reference_canvas(&layout, &saved);
                        let to = self.canonical_canvas.unwrap_or(file_ref);
                        let available = &self.available_tabs;
                        let applied = Self::merge_layout_geometry(
                            &mut self.main_window_rects,
                            &saved,
                            file_ref,
                            to,
                            |key| available.contains_key(key),
                        );
                        if applied > 0 {
                            self.layout_dirty = true;
                            self.app_core.add_system_message(&format!(
                                "Adopted the geometry of layout '{}' for {} window{}.",
                                name,
                                applied,
                                if applied == 1 { "" } else { "s" }
                            ));
                        } else {
                            self.app_core.add_system_message(&format!(
                                "Layout '{}' positions no windows that are open here.",
                                name
                            ));
                        }
                    }
                    Err(err) => {
                        self.app_core
                            .add_system_message(&format!("Failed to load layout: {}", err));
                        self.list_layout_checkpoints();
                    }
                }
            }
            A::SaveSkin(name) => {
                if !is_valid_layout_name(&name) {
                    self.app_core
                        .add_system_message("Skin names use letters, digits, '-' and '_' only.");
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
                let extra = self.gui_layout_pack_entry();
                self.app_core.uiexport_with(&args, extra);
            }
            A::UiImport(args) => {
                if let Some((pack_name, bytes)) = self.app_core.uiimport(&args) {
                    self.install_gui_layout_from_pack(&pack_name, &bytes);
                }
            }
            A::PackEditor => self.open_pack_editor(),
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
            A::EditIndicators => self.open_indicator_templates_editor(),
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
        if link_data.exist_id == crate::data::URL_LINK_SENTINEL {
            return crate::data::is_web_url(&link_data.noun)
                .then(|| GuiLinkDispatch::OpenUrl(link_data.noun.clone()));
        }
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
            GuiLinkDispatch::NetworkCommand(command) => {
                if command.trim_start().starts_with('.') {
                    // Synthetic client links (bestiary tables, etc.) carry
                    // dot-commands; route through dot dispatch, not the game.
                    self.app_core
                        .message_processor
                        .pending_client_commands
                        .push(command.trim().to_string());
                    return;
                }
                command
            }
            GuiLinkDispatch::MenuRequest { exist_id, noun } => {
                self.popup_menu_host = origin;
                self.app_core.request_menu(exist_id, noun, click.click_pos)
            }
            GuiLinkDispatch::OpenUrl(url) => {
                if let Err(err) = crate::platform::open_url(&url) {
                    self.app_core
                        .add_system_message(&format!("Cannot open {}: {}", url, err));
                }
                return;
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
        // Widget forensics: VELLUM_DEBUG_HOVER=1 turns on egui's
        // debug-on-hover with the callstack feature — hovering ANY widget
        // shows the source location that created it. For "what is drawing
        // this?" mysteries; costs nothing when the env var is absent.
        // Both APIs below are #[cfg(debug_assertions)] inside egui itself —
        // they don't exist in release builds, so this whole block is gated.
        #[cfg(debug_assertions)]
        {
            if std::env::var_os("VELLUM_DEBUG_HOVER").is_some() {
                ctx.set_debug_on_hover(true);
            }
            // egui's DebugOptions.show_unaligned defaults ON in debug builds
            // and stamps an orange "Unaligned" warning under widgets with
            // fractionally-sized rects — the phantom strip that haunted the
            // containers and bestiary windows in dev builds. Never wanted.
            if ctx.global_style().debug.show_unaligned {
                ctx.all_styles_mut(|style| style.debug.show_unaligned = false);
            }
        }
        self.app_core.perf_stats.record_frame();
        // "Render" in the GUI is last frame's CPU cost as reported by
        // eframe (App::ui + painting); the first frame has none yet.
        if let Some(cpu_seconds) = frame.info().cpu_usage {
            self.app_core
                .perf_stats
                .record_render_time(std::time::Duration::from_secs_f32(cpu_seconds));
        }
        // Process CPU/RSS (rate-limited to 1 Hz internally) and buffered
        // content totals for the performance monitor.
        self.app_core.perf_stats.sample_sysinfo();

        // Refresh the sibling-instance snapshot once per frame -- render
        // paths are `&self`, so this is the one place it can be taken. The
        // self card is built HERE too: building it in the widget rebuilt it
        // per window per frame, cloning effects/injuries/group each time.
        // Skipped entirely when no multiaccount window is on screen.
        if let Some(hub) = &self.multiaccount {
            let wants_cards = self
                .app_core
                .ui_state
                .windows
                .values()
                .any(|w| matches!(w.content, crate::data::WindowContent::MultiAccount));
            if wants_cards {
                let now_ms = crate::core::multiaccount::hub::now_ms();
                let peers = hub.reap_and_snapshot(now_ms);
                let mut combined = (*peers).clone();
                combined.insert(
                    crate::core::multiaccount::SELF_PORT,
                    crate::core::multiaccount::PeerStatus::from_local(
                        &self.app_core.game_state,
                        self.app_core.config.connection.character.as_deref().or(self
                            .app_core
                            .config
                            .character
                            .as_deref()),
                        self.app_core
                            .nav_room_id
                            .clone()
                            .or_else(|| self.app_core.lich_room_id.clone()),
                        now_ms,
                    ),
                );
                self.multiaccount_peers = std::sync::Arc::new(combined);
            }
        }
        {
            let total_lines: usize = self
                .app_core
                .ui_state
                .windows
                .values()
                .map(|w| match &w.content {
                    crate::data::WindowContent::Text(content)
                    | crate::data::WindowContent::Inventory(content)
                    | crate::data::WindowContent::Reserve(content)
                    | crate::data::WindowContent::Spells(content) => content.lines.len(),
                    crate::data::WindowContent::TabbedText(tabbed) => {
                        tabbed.tabs.iter().map(|tab| tab.content.lines.len()).sum()
                    }
                    _ => 0,
                })
                .sum();
            let window_count = self.app_core.ui_state.windows.len();
            self.app_core
                .perf_stats
                .update_memory_stats(total_lines, window_count);
        }
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
        // Publish the custom-emoji size/spacing knobs for this frame.
        custom_emoji_render::set_geometry(
            self.app_core.config.ui.custom_emoji_size,
            self.app_core.config.ui.custom_emoji_spacing,
        );
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
        // A `.loadlayout` queued an OS-window geometry restore. Send the
        // viewport commands once, then let the rescale below wait for the
        // window to settle at the target size so the rects land 1:1.
        if let Some(viewport) = self.pending_viewport_restore.take() {
            if viewport.maximized {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                let [w, h] = viewport.inner_size;
                if w.is_finite() && h.is_finite() && w > 1.0 && h > 1.0 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
                }
                if let Some([x, y]) = viewport
                    .outer_pos
                    .filter(|pos| pos.iter().all(|v| v.is_finite()))
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
                }
            }
            ctx.request_repaint();
        }
        // Keep the rect store anchored to the live canvas: whenever the
        // content size drifts from the anchor (OS resize, maximize, a load
        // or `.resize` that re-pointed the anchor), apply the pure
        // proportional map and re-anchor. Pure scales compose losslessly, so
        // windows track a drag-resize smoothly frame by frame and return to
        // exact positions when the size comes back; display-time clamping
        // handles tiny canvases without ever writing into the store. A
        // degenerate content rect (minimize, first frames) leaves the anchor
        // alone so the real geometry is still the reference on restore.
        {
            let content = ctx.input(|input| input.content_rect());
            // Zone/role rules: sidebar windows follow their owning edge
            // with fixed width, header/footer mirror on y, Fixed windows
            // keep their size (Niffy's zoom-drift fix; P-A3).
            let tab_zones = &self.tab_zones;
            let size_roles = &self.window_size_roles;
            if Self::track_canvas_anchor_ruled(
                &mut self.canonical_canvas,
                &mut self.main_window_rects,
                content,
                |key| {
                    (
                        tab_zones.get(key).copied().unwrap_or(GuiShellZone::Center),
                        size_roles.get(key).copied().unwrap_or_default(),
                    )
                },
            ) {
                self.layout_dirty = true;
            }
        }
        self.apply_theme_if_changed(&ctx);
        // First frame with a legacy active_skin on record: one-shot
        // conversion to a preset (applies it, so the look carries over).
        if let Some(name) = self.startup_skin_migration.take() {
            self.app_core.add_system_message(&format!(
                "Migrating legacy skin '{name}' — skins are presets now."
            ));
            self.apply_skin_by_name(&name);
        }
        // Core wrote the appearance store outside our funnel (skin-pack
        // install/import): adopt it before the skin declarations below, or
        // this frame's layout save would stomp the new look.
        if self.app_core.appearance_changed_externally {
            self.app_core.appearance_changed_externally = false;
            self.sync_ui_settings_from_appearance();
        }
        // Pool frames referenced by per-window overrides load lazily; tell
        // the skin state which ones are in use before it applies.
        self.skin_state.set_needed_pool_frames(
            self.tab_settings
                .values()
                .filter_map(|settings| settings.skin_frame.clone())
                .chain(self.ui_settings.default_frame.clone())
                // Control faces are pool frames too.
                .chain(self.ui_settings.control_frames.values().cloned()),
        );
        self.skin_state
            .set_control_frames(&self.ui_settings.control_frames);
        self.skin_state
            .set_edge_set(self.ui_settings.edge_set.as_deref());
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
        // Pool dolls bound per-window (doll_set holding a pool path) load
        // as named sets, so bindings work with or without a skin.
        self.skin_state.set_needed_pool_dolls(
            self.app_core
                .layout
                .windows
                .iter()
                .filter_map(|def| match def {
                    crate::config::WindowDef::InjuryDoll { data, .. } => data.doll_set.clone(),
                    _ => None,
                })
                .filter(|binding| binding.contains('/')),
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
        let needed_pool_icons: Vec<String> = hand_state_images.chain(hotbar_images).collect();
        self.skin_state.set_needed_pool_icons(needed_pool_icons);
        self.skin_state.set_grayscale(
            self.ui_settings.status_icons.any_gray(),
            self.ui_settings.doll_grayscale,
        );
        self.skin_state
            .apply_if_changed(&ctx, self.ui_settings.doll_image.as_deref());
        // Creature-card art: resolve + load base sprites for the field's
        // current roster (lazy, negative-cached — a settled room is a few
        // hash lookups). Family comes from the bundled bestiary when the
        // noun maps to exactly one family, feeding the `{family}` tier of
        // the resolve cascade.
        {
            let wanted: Vec<crate::frontend::gui::skin::WantedCreature> = self
                .app_core
                .creature_field
                .units()
                .iter()
                .flat_map(|u| u.members.iter())
                .filter_map(|m| {
                    self.app_core
                        .game_state
                        .room_creatures
                        .iter()
                        .find(|c| &c.id == m)
                })
                .map(|c| {
                    let family = c
                        .noun
                        .as_deref()
                        .and_then(crate::core::creature_cards::family_for_noun);
                    crate::frontend::gui::skin::WantedCreature {
                        name: c.name.clone(),
                        noun: c.noun.clone(),
                        family,
                        prone: c.flags.as_ref().is_some_and(|f| f.has_flag("prone")),
                        injuries: c
                            .flags
                            .as_ref()
                            .map(|f| f.injuries.clone())
                            .unwrap_or_default(),
                    }
                })
                .collect();
            if !wanted.is_empty() {
                self.skin_state.prepare_creature_art(&ctx, &wanted);
            }
        }
        // Scene art for the room-bound stage scene (backdrop + props);
        // per-frame like the Studio's call, settled scenes are hash hits.
        if let Some(scene) = self.app_core.stage_scene.clone() {
            self.skin_state.prepare_scene_art(&ctx, &scene);
        }
        self.apply_ui_sizing(&ctx);
        // Prime the item classifier while &mut self is available; render
        // paths (hotbar/hand conditions) read the immutable cache.
        let _ = self.app_core.gameobj_data();
        // Timed as a pseudo-window so render spikes can tell "message
        // ingestion was slow" apart from "a window painted slow" — the
        // spike attribution lists this alongside real window costs.
        let ingest_start = std::time::Instant::now();
        self.pump_server_messages();
        self.app_core
            .perf_stats
            .record_window_render("(ingest)", ingest_start.elapsed());
        // Feed-injected dot-commands (<vellumCmd> from Lich scripts) run
        // through the same dispatch as typed commands.
        for command in self.app_core.take_pending_client_commands() {
            self.dispatch_command(command);
        }
        // Keep-open `.quit`: drop the connection but keep the app alive.
        // Aborting the task closes the socket (that IS the Lich detach); a
        // killed task sends no ServerMessage::Disconnected, so flip the flag.
        if self.app_core.take_disconnect_request() {
            if let Some(handle) = self.network_handle.take() {
                handle.abort();
            }
            self.app_core.game_state.connected = false;
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
        // Claim this frame's Copy/Cut for an active buffer selection BEFORE
        // any window renders, so which window renders first (zone/tab order,
        // i.e. window positions) can never decide whether Ctrl+C works.
        Self::claim_buffer_copy_event(&ctx);
        // Resolve command-input keybinds for this frame so the input widget's
        // submit/history/clear-line honor rebinds (single source of truth).
        self.stash_command_input_keys(&ctx);
        // Static render fns (text widgets) read per-frame settings from temp
        // data; refresh the split-divider jump-button toggle here.
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("split_jump_button"),
                self.app_core.config.ui.split_jump_button,
            );
            // 0 = left, 1 = center, 2 = right (the default).
            let pos: u8 = match self.app_core.config.ui.split_jump_button_position.as_str() {
                "left" => 0,
                "center" => 1,
                _ => 2,
            };
            data.insert_temp(egui::Id::new("split_jump_button_pos"), pos);
        });

        if self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let detached_before_frame = self.detached_tab_keys();
        let mut reconnect_clicked = false;
        let mut zone_actions = GuiWindowActions::default();
        let mut visible_zone_rects: Vec<(GuiShellZone, Rect)> = Vec::new();
        let mut zone_window_rects: Vec<GuiZoneWindowRect> = Vec::new();

        egui::Panel::top("gui_shell_toolbar")
            .resizable(false)
            .exact_size(30.0)
            // No divider under the header bar — the theme's separator stroke
            // reads as a hard cyan line above the central zone over a dark
            // skin. Let the mesh meet the toolbar directly.
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Flat toolbar: no resting chip background on the zone
                    // toggles / Windows menu, hover highlight only. Scoped to
                    // this row; the dropdown menus keep normal visuals.
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                    ui.heading("VellumFE GUI");
                    ui.separator();
                    // Connected: a plain green status label. Disconnected: the
                    // same slot becomes a clickable Reconnect button (the
                    // status IS the affordance — no separate button).
                    if self.app_core.game_state.connected {
                        ui.label(
                            RichText::new("Connected")
                                .color(theme::color32(self.current_theme.status_success)),
                        );
                    } else if ui
                        .button(
                            RichText::new("Reconnect")
                                .color(theme::color32(self.current_theme.status_error)),
                        )
                        .on_hover_text("Reconnect to the game (.reconnect)")
                        .clicked()
                    {
                        reconnect_clicked = true;
                    }
                    ui.separator();

                    // One "Zones" menu: each row is a show/hide button plus
                    // an Overlay checkbox. NOTHING inside closes the menu
                    // (owner ask: adjust several zones in one visit and
                    // watch them take effect); only clicking outside does —
                    // hence CloseOnClickOutside with no ui.close() calls.
                    egui::containers::menu::MenuButton::new("Zones")
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            ui.set_min_width(200.0);
                            let zone_row = |app: &mut Self,
                                            ui: &mut egui::Ui,
                                            zone: GuiShellZone,
                                            shown: bool,
                                            name: &str| {
                                ui.horizontal(|ui| {
                                    let label =
                                        format!("{} {}", if shown { "Hide" } else { "Show" }, name);
                                    if ui
                                        .add_sized([120.0, 20.0], egui::Button::new(label))
                                        .clicked()
                                    {
                                        match zone {
                                            GuiShellZone::Header => {
                                                app.shell_layout.header_visible =
                                                    !app.shell_layout.header_visible
                                            }
                                            GuiShellZone::Footer => {
                                                app.shell_layout.footer_visible =
                                                    !app.shell_layout.footer_visible
                                            }
                                            GuiShellZone::LeftSidebar => {
                                                app.shell_layout.left_sidebar_collapsed =
                                                    !app.shell_layout.left_sidebar_collapsed
                                            }
                                            GuiShellZone::RightSidebar => {
                                                app.shell_layout.right_sidebar_collapsed =
                                                    !app.shell_layout.right_sidebar_collapsed
                                            }
                                            GuiShellZone::Center => {}
                                        }
                                        app.layout_dirty = true;
                                    }
                                    let mut overlay = app.shell_layout.zone_mode(zone)
                                        == zones::ZoneDisplayMode::Overlay;
                                    if ui
                                        .checkbox(&mut overlay, "Overlay")
                                        .on_hover_text(
                                            "Float this zone over the center like a drawer \
                                         instead of reserving its own space.",
                                        )
                                        .changed()
                                    {
                                        app.shell_layout.set_zone_mode(
                                            zone,
                                            if overlay {
                                                zones::ZoneDisplayMode::Overlay
                                            } else {
                                                zones::ZoneDisplayMode::Reserve
                                            },
                                        );
                                        app.layout_dirty = true;
                                    }
                                });
                                // Backdrop opacity, overlay-only: in Reserve mode
                                // there is nothing behind the zone to reveal, so
                                // the control would be a no-op knob.
                                if app.shell_layout.zone_mode(zone)
                                    == zones::ZoneDisplayMode::Overlay
                                {
                                    ui.horizontal(|ui| {
                                        ui.add_space(8.0);
                                        let mut opacity = app.shell_layout.zone_opacity(zone);
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut opacity, 0.0..=1.0)
                                                    .text("Opacity")
                                                    .fixed_decimals(2),
                                            )
                                            .on_hover_text(
                                                "Backdrop opacity for this drawer. 1.00 is a \
                                             solid panel; lower values let the center pane \
                                             show through so the bar reads as a HUD. \
                                             Clicks are still caught either way.",
                                            )
                                            .changed()
                                        {
                                            app.shell_layout.set_zone_opacity(zone, opacity);
                                            app.layout_dirty = true;
                                        }
                                    });
                                }
                            };
                            zone_row(
                                self,
                                ui,
                                GuiShellZone::Header,
                                self.shell_layout.header_visible,
                                "Header",
                            );
                            zone_row(
                                self,
                                ui,
                                GuiShellZone::Footer,
                                self.shell_layout.footer_visible,
                                "Footer",
                            );
                            zone_row(
                                self,
                                ui,
                                GuiShellZone::LeftSidebar,
                                !self.shell_layout.left_sidebar_collapsed,
                                "Left Bar",
                            );
                            zone_row(
                                self,
                                ui,
                                GuiShellZone::RightSidebar,
                                !self.shell_layout.right_sidebar_collapsed,
                                "Right Bar",
                            );
                        });

                    // "Windows" is the same catalog the floating manager
                    // shows (show/hide + zone + add-window, grouped by
                    // category), inline as a stay-open menu like Zones:
                    // nothing inside closes it, only clicking outside.
                    egui::containers::menu::MenuButton::new("Windows")
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            ui.set_min_width(380.0);
                            ui.set_max_height(520.0);
                            self.known_windows_body(ui);
                        });

                    // "Settings" is the hub for the knobs in config.toml
                    // (the registry-driven Settings window): one row per
                    // section, and clicking a row opens the Settings
                    // window with that section expanded and scrolled into
                    // view. Stay-open like Zones/Windows.
                    egui::containers::menu::MenuButton::new("Settings")
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            ui.set_min_width(160.0);
                            if ui.button("All Settings…").clicked() {
                                self.open_settings_editor();
                            }
                            ui.separator();
                            for section in editors::settings_sections() {
                                if ui.button(section).clicked() {
                                    self.open_settings_editor_at(section);
                                }
                            }
                        });

                    // "Editors" is the hub for authored content — the
                    // standalone editors that manage their own files
                    // (highlights, keybinds, hotbars, …) rather than
                    // config.toml knobs. Same stay-open behavior.
                    egui::containers::menu::MenuButton::new("Editors")
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            ui.set_min_width(180.0);
                            if ui.button("Themes").clicked() {
                                self.open_theme_browser();
                            }
                            if ui.button("Colors").clicked() {
                                self.open_colors_editor();
                            }
                            if ui.button("Highlights").clicked() {
                                self.open_highlight_editor(None);
                            }
                            // Sits with Highlights because that is what a
                            // pack is: highlight rules written by someone
                            // else, with a trust gate on the powers that can
                            // alter game text.
                            if ui.button("Alert Packs").clicked() {
                                self.open_alertpacks_editor();
                            }
                            ui.separator();
                            if ui.button("Keybinds").clicked() {
                                self.open_keybind_editor();
                            }
                            if ui.button("Menu Keybinds").clicked() {
                                self.open_menu_keybind_editor();
                            }
                            #[cfg(feature = "gamepad")]
                            if ui.button("Controller").clicked() {
                                self.open_controller_editor();
                            }
                            if ui.button("Touch Wheel").clicked() {
                                self.open_touch_wheel_editor();
                            }
                            if ui.button("Hotbars").clicked() {
                                self.open_hotbar_editor();
                            }
                            ui.separator();
                            if ui.button("Indicators").clicked() {
                                self.open_indicator_templates_editor();
                            }
                            if ui.button("Streams & Custom Windows").clicked() {
                                self.open_custom_windows_editor();
                            }
                            if ui.button("Sorter").clicked() {
                                self.open_sorter_editor();
                            }
                            if ui.button("Room Images").clicked() {
                                self.open_room_images_editor();
                            }
                            ui.separator();
                            if ui.button("UI Packs").clicked() {
                                self.open_pack_editor();
                            }
                            if ui.button("Asset Manager (Jinx)").clicked() {
                                self.open_jinx_panel();
                            }
                            if ui.button("Launcher").clicked() {
                                self.open_launcher_editor();
                            }
                        });

                    // "Explorer" is a one-shot: open (or surface) the Map
                    // Explorer's native window.
                    if ui.button("Explorer").clicked() {
                        self.map_explorer.open = true;
                    }

                    // Native skill trainer (same panel `.goals` opens).
                    if ui.button("Skill Goals").clicked() {
                        self.open_skill_trainer_panel();
                    }
                });
            });

        let separator_style = self.ui_settings.zone_separators;
        // Overlay-mode zones skip their space-claiming egui panel entirely
        // and render as floating drawers inside the central pass below.
        let header_overlay =
            self.shell_layout.zone_mode(GuiShellZone::Header) == zones::ZoneDisplayMode::Overlay;
        let footer_overlay =
            self.shell_layout.zone_mode(GuiShellZone::Footer) == zones::ZoneDisplayMode::Overlay;
        if self.shell_layout.header_visible && !header_overlay {
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
                let completion = self
                    .app_core
                    .config
                    .ui
                    .history_suggestions
                    .then(|| {
                        crate::frontend::common::find_history_completion(
                            &seed,
                            &self.command_history,
                        )
                    })
                    .flatten();
                // Fixed fallback panel: not a movable window, no grip.
                Self::render_command_input_widget(ui, &seed, completion.as_deref(), false);
            });
        }

        if self.shell_layout.footer_visible && !footer_overlay {
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

                self.shell_layout.sanitize();
                let min_center_width = 220.0;
                let left_on = !self.shell_layout.left_sidebar_collapsed;
                let right_on = !self.shell_layout.right_sidebar_collapsed;
                let left_overlay = self.shell_layout.zone_mode(GuiShellZone::LeftSidebar)
                    == zones::ZoneDisplayMode::Overlay;
                let right_overlay = self.shell_layout.zone_mode(GuiShellZone::RightSidebar)
                    == zones::ZoneDisplayMode::Overlay;
                // Display-only squeeze on narrow windows; the persisted widths
                // stay untouched so the layout springs back when the window
                // grows again (the old math floored collapsed sidebars back to
                // life, inverted the center, and baked the squeeze into the
                // saved layout). Only RESERVED sidebars share space with the
                // center, so only they enter the squeeze; an overlay drawer
                // clamps against the root width alone.
                let (left_reserved_width, right_reserved_width) = zones::squeezed_sidebar_widths(
                    root.width(),
                    min_center_width,
                    if left_on && !left_overlay {
                        self.shell_layout.left_sidebar_width
                    } else {
                        0.0
                    },
                    if right_on && !right_overlay {
                        self.shell_layout.right_sidebar_width
                    } else {
                        0.0
                    },
                );
                let left_width = if left_on && left_overlay {
                    self.shell_layout
                        .left_sidebar_width
                        .min((root.width() - 40.0).max(0.0))
                } else {
                    left_reserved_width
                };
                let right_width = if right_on && right_overlay {
                    self.shell_layout
                        .right_sidebar_width
                        .min((root.width() - 40.0).max(0.0))
                } else {
                    right_reserved_width
                };

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
                // Overlay drawers float above the center, so they never shrink it.
                let center_min_x = match left_rect {
                    Some(rect) if !left_overlay => rect.max.x,
                    _ => root.min.x,
                };
                let center_max_x = match right_rect {
                    Some(rect) if !right_overlay => rect.min.x,
                    _ => root.max.x,
                };
                let center_rect = Rect::from_min_max(
                    Pos2::new(center_min_x, root.min.y),
                    Pos2::new(center_max_x, root.max.y),
                );
                // The center BASE pane: where the store's rects live — the
                // center with no reserved zone open. Root already excludes
                // reserved header/footer (they are egui panels above this
                // pass), so expand back by their heights; width is the full
                // root (sidebars carve from it). The P-A3 resolve maps
                // base→center_rect per frame; identity when they're equal.
                let reserved_header_h = if self.shell_layout.header_visible && !header_overlay {
                    self.shell_layout.header_height
                } else {
                    0.0
                };
                let reserved_footer_h = if self.shell_layout.footer_visible && !footer_overlay {
                    self.shell_layout.footer_height
                } else {
                    0.0
                };
                self.center_base_pane = Some(Rect::from_min_max(
                    Pos2::new(root.min.x, root.min.y - reserved_header_h),
                    Pos2::new(root.max.x, root.max.y + reserved_footer_h),
                ));
                // Overlay header/footer drawers carve their strips out of the
                // full root (their reserved twins are egui panels above this
                // pass and never reach here).
                let header_drawer_rect =
                    (self.shell_layout.header_visible && header_overlay).then(|| {
                        Rect::from_min_max(
                            root.min,
                            Pos2::new(
                                root.max.x,
                                (root.min.y + self.shell_layout.header_height).min(root.max.y),
                            ),
                        )
                    });
                let footer_drawer_rect =
                    (self.shell_layout.footer_visible && footer_overlay).then(|| {
                        Rect::from_min_max(
                            Pos2::new(
                                root.min.x,
                                (root.max.y - self.shell_layout.footer_height).max(root.min.y),
                            ),
                            root.max,
                        )
                    });
                visible_zone_rects.push((GuiShellZone::Center, center_rect));

                let sidebar_divider_stroke =
                    egui::Stroke::new(1.5, ui.visuals().window_stroke.color);
                if separator_style == ZoneSeparatorStyle::Shown {
                    // Overlay drawers draw their own inner-edge divider on the
                    // backdrop layer; a panel-level line would sit underneath.
                    if let (Some(rect), false) = (left_rect, left_overlay) {
                        ui.painter()
                            .vline(rect.max.x, root.y_range(), sidebar_divider_stroke);
                    }
                    if let (Some(rect), false) = (right_rect, right_overlay) {
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
                    if left_overlay {
                        // Backdrop first so the drawer's windows (registered
                        // after, same Foreground order) stack above it.
                        self.render_overlay_backdrop(ui.ctx(), GuiShellZone::LeftSidebar, rect);
                    }
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
                    if left_overlay {
                        // The gutter's persisted Foreground slot can predate the
                        // backdrop's; keep it on top so the grab stays reachable.
                        ui.ctx().move_to_top(egui::LayerId::new(
                            egui::Order::Foreground,
                            egui::Id::new("gui_left_sidebar_splitter"),
                        ));
                    }
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
                    if right_overlay {
                        self.render_overlay_backdrop(ui.ctx(), GuiShellZone::RightSidebar, rect);
                    }
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
                    if right_overlay {
                        ui.ctx().move_to_top(egui::LayerId::new(
                            egui::Order::Foreground,
                            egui::Id::new("gui_right_sidebar_splitter"),
                        ));
                    }
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

                // Overlay header/footer drawers, last so they (and their
                // windows) float over the sidebars too. Their resize handles
                // are Foreground gutter areas like the sidebar splitters —
                // a plain `ui.interact` handle would sit under the backdrop.
                let overlay_band =
                    |app: &mut Self,
                     ui: &mut egui::Ui,
                     zone: GuiShellZone,
                     rect: Rect,
                     visible_zone_rects: &mut Vec<(GuiShellZone, Rect)>,
                     zone_window_rects: &mut Vec<zones::GuiZoneWindowRect>,
                     zone_actions: &mut GuiWindowActions| {
                        visible_zone_rects.push((zone, rect));
                        app.render_overlay_backdrop(ui.ctx(), zone, rect);
                        let is_header = zone == GuiShellZone::Header;
                        let handle = if is_header {
                            Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 10.0), rect.max)
                        } else {
                            Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + 10.0))
                        };
                        let handle_id = egui::Id::new(("gui_overlay_band_handle", zone.label()));
                        let handle_response = egui::Area::new(handle_id)
                            .order(egui::Order::Foreground)
                            .fixed_pos(handle.min)
                            .show(ui.ctx(), |gutter_ui| {
                                gutter_ui
                                    .allocate_exact_size(
                                        handle.size(),
                                        egui::Sense::click_and_drag(),
                                    )
                                    .1
                            })
                            .inner;
                        ui.ctx()
                            .move_to_top(egui::LayerId::new(egui::Order::Foreground, handle_id));
                        if handle_response.hovered() || handle_response.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                        }
                        if handle_response.dragged() {
                            let dy = ui.ctx().input(|i| i.pointer.delta().y);
                            if is_header {
                                app.shell_layout.header_height =
                                    (app.shell_layout.header_height + dy).clamp(96.0, 360.0);
                            } else {
                                app.shell_layout.footer_height =
                                    (app.shell_layout.footer_height - dy).clamp(96.0, 420.0);
                            }
                            app.layout_dirty = true;
                        }
                        zone_actions.merge(app.render_zone_surface(
                            &ctx,
                            &detached_before_frame,
                            zone,
                            rect,
                            zone_window_rects,
                        ));
                    };
                if let Some(rect) = header_drawer_rect {
                    overlay_band(
                        self,
                        ui,
                        GuiShellZone::Header,
                        rect,
                        &mut visible_zone_rects,
                        &mut zone_window_rects,
                        &mut zone_actions,
                    );
                }
                if let Some(rect) = footer_drawer_rect {
                    overlay_band(
                        self,
                        ui,
                        GuiShellZone::Footer,
                        rect,
                        &mut visible_zone_rects,
                        &mut zone_window_rects,
                        &mut zone_actions,
                    );
                }
            });

        let detached_link_clicks = self.render_detached_viewports(&ctx);
        self.render_map_explorer(&ctx);

        let zone_drop_result = self.render_zone_drop_overlay(&ctx, &visible_zone_rects);
        self.render_window_move_overlay(&ctx, &visible_zone_rects);
        self.handle_link_drag_drop(&ctx, &zone_window_rects);

        // All zone surfaces have rendered, so every visible window is
        // registered as an egui layer. If a layout load queued a stacking
        // order, replay it NOW (raising layers that exist — egui resolves the
        // final order at end of pass); otherwise cache the live order for the
        // save snapshot. Never both in one frame: the cache read would still
        // see the pre-raise order and clobber the freshly-applied one.
        if let Some(order) = self.pending_zorder.take() {
            self.apply_stacking_order(&ctx, &order);
        } else if let Some(tab) = self.pending_raise_tab.take() {
            // switch_current_window: raise one window, then let the cache
            // re-read the resulting order next frame (don't clobber it here).
            self.raise_tab_to_front(&ctx, &tab);
        } else {
            self.refresh_zorder_cache(&ctx);
        }

        if reconnect_clicked {
            self.reconnect();
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
        // Alerts sit above the game windows but BELOW menus, popups, and
        // editors: ambiance art must never cover a menu the user just opened.
        self.render_alert_overlay(&ctx);
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
        for event in webui_events {
            // Core owns the socket; forward each interaction through it.
            if let crate::data::webui::WebUiClientMessage::Event { page, cid, value } = event {
                self.app_core.webui_send_event(page, cid, value);
            }
        }

        // Images the panels asked for: /files/ srcs fetch over the bridge's
        // HTTP endpoint (cookie-authed); anything else fails visibly. The
        // endpoint + event sender come from core (the bridge owner).
        for src in Self::take_pending_webui_fetches(&ctx) {
            if self.webui_fetches_inflight.contains(&src) {
                continue;
            }
            match (
                self.app_core.webui_endpoint().cloned(),
                self.app_core.webui_event_sender(),
            ) {
                (Some((host, port, token)), Some(event_tx)) if src.starts_with("/files/") => {
                    self.webui_fetches_inflight.insert(src.clone());
                    crate::webui::fetch_image(
                        self._runtime.handle(),
                        host,
                        port,
                        token,
                        src,
                        event_tx,
                    );
                }
                _ => {
                    let reason = if src.starts_with("/files/") {
                        "not connected to the Lich WebUI".to_string()
                    } else {
                        "external image URLs are not supported yet".to_string()
                    };
                    Self::set_webui_image(&ctx, src, webui_panel::WebUiImageState::Failed(reason));
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
            if echo.completion_accepted {
                self.history_pos = None;
                self.history_draft.clear();
                self.command_cursor_to_end(&ctx);
            } else if echo.history_prev {
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
            cc.egui_ctx.global_style_mut(|style| {
                // Resize hot-zones: stock egui is 3px sides and a 20x20
                // square on each corner, which floats the resize cursor
                // over empty background near a window. The first tighten
                // (2/5) made the band so thin that aiming at a chunky skin
                // frame's visual border missed it and started a body-drag
                // MOVE instead (anchored windows then snap back on release
                // — the reported "grabs the frame skin and drags it away").
                // 5/6 keeps the cursor close to the frame while making the
                // border art actually grabbable.
                style.interaction.resize_grab_radius_side = 5.0;
                style.interaction.resize_grab_radius_corner = 6.0;
            });
            app.set_repaint_context(cc.egui_ctx.clone());
            Ok(Box::new(app))
        }),
    )
    .map_err(|err| anyhow!("Failed to run GUI frontend: {}", err))
}

#[cfg(test)]
mod tests;
