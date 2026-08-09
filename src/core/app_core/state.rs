//! Core application logic - Pure business logic without UI coupling
//!
//! AppCore manages game state, configuration, and message processing.
//! It has NO knowledge of rendering - all state is stored in data structures
//! that frontends read from.

use crate::cmdlist::CmdList;
use crate::config::{Config, Layout, SavedDialogPositions};
use crate::core::{GameState, MessageProcessor};
use crate::data::*;
use crate::parser::{ParsedElement, XmlParser};
use crate::performance::PerformanceStats;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

mod focus;
mod menus;
mod persistence;
mod remote;
mod travel_ticks;
mod window_lifecycle;
mod windows;

/// Pending menu request for correlation
#[derive(Clone, Debug)]
pub struct PendingMenuRequest {
    pub exist_id: String,
    pub noun: String,
    /// Who asked: the local UI, or a remote web client. The `<menu>`
    /// response routes back to this origin.
    pub origin: crate::core::remote::MenuOrigin,
}

/// Core application state - frontend-agnostic
pub struct AppCore {
    // === Configuration ===
    /// Application configuration (presets, highlights, keybinds, etc.)
    pub config: Config,

    /// Current window layout definition
    pub layout: Layout,

    /// Baseline layout for proportional resizing
    pub baseline_layout: Option<Layout>,

    // === State ===
    /// Game session state (connection, character, room, vitals, etc.)
    pub game_state: GameState,

    /// UI state (windows, focus, input, popups, etc.)
    pub ui_state: UiState,

    // === Message Processing ===
    /// XML parser for GemStone IV protocol
    pub parser: XmlParser,

    /// Message processor (routes parsed elements to state updates)
    pub message_processor: MessageProcessor,

    // === Stream Management ===
    /// Current active stream ID (where text is being routed)
    pub current_stream: String,

    /// If true, discard text because no window exists for stream
    pub discard_current_stream: bool,

    /// Buffer for accumulating multi-line stream content
    pub stream_buffer: String,

    // === Timing ===
    /// Server time offset (server_time - local_time) for countdown calculations
    pub server_time_offset: i64,

    // === Optional Features ===
    /// Command list for context menus (None if failed to load)
    pub cmdlist: Option<CmdList>,

    /// Menu request counter for correlating menu responses
    pub menu_request_counter: u32,

    /// Pending menu requests (counter -> PendingMenuRequest)
    pub pending_menu_requests: HashMap<String, PendingMenuRequest>,

    /// Cached menu categories for submenus (category_name -> items)
    pub menu_categories: HashMap<String, Vec<crate::data::ui_state::PopupMenuItem>>,

    /// Position of last link click (for menu positioning)
    pub last_link_click_pos: Option<(u16, u16)>,

    /// Performance statistics tracking
    pub perf_stats: PerformanceStats,

    /// Whether to show performance stats
    pub show_perf_stats: bool,

    /// Sound player for highlight sounds
    pub sound_player: Option<crate::sound::SoundPlayer>,

    /// Text-to-Speech manager for accessibility
    pub tts_manager: crate::tts::TtsManager,

    /// Queued haptic (rumble) events for frontends to drain (haptics.rs)
    pub pending_haptics: Vec<super::HapticEvent>,
    /// Last-seen state for haptic transition detection
    pub(crate) haptic_prev: super::HapticSnapshot,
    /// Cooldown clock for highlight-driven rumble (haptics.rs)
    pub(crate) last_highlight_rumble: Option<std::time::Instant>,

    // === Navigation State ===
    /// Navigation room ID from <nav rm='...'/>
    /// Live map state: mapdb, generated layouts, current-room tracking.
    pub map: crate::core::map_service::MapService,
    /// Downloads released mapdbs from GitHub (Settings > Map).
    pub map_updater: crate::core::mapdb_update::MapDbUpdater,
    /// The asset manager (`.jinx`): off-thread install/update against
    /// federated repos, polled each frame like `map_updater`.
    pub jinx_worker: crate::core::jinx::worker::JinxWorker,
    /// Auto-clear deadlines for highlight-set custom statuses (UPPERCASE
    /// id -> when it switches back off).
    pub custom_status_expiries: std::collections::HashMap<String, std::time::Instant>,
    /// Cached indicator templates keyed by UPPERCASE id, rebuilt from disk on
    /// load and after the template editor saves. Status icon resolution
    /// (indicator windows + dashboards) reads this per frame; the underlying
    /// `Config::list_indicator_templates()` does file IO, so it must not run
    /// in the render loop.
    pub indicator_templates: std::collections::HashMap<String, crate::config::IndicatorTemplateEntry>,
    /// Latest Jinx catalog (all installable assets across repos), delivered by
    /// the worker's `Catalog` request and read by the GUI Assets panel. None
    /// until first fetched; the panel triggers a refresh on open.
    pub jinx_catalog: Option<Vec<crate::core::jinx::worker::CatalogEntry>>,
    /// One-shot: emit the "game data is stale" login nudge on the first game
    /// text of the session. Set true at construction, cleared after firing.
    jinx_nudge_pending: bool,
    /// Native go2: the walk executor and its outbound command queue.
    pub travel: crate::core::travel::TravelService,
    /// A `.go2` waiting on a `urchin status` refresh: (destination, deadline).
    /// When urchin travel is enabled but the cached access is stale, go2 sends
    /// `urchin status` and defers planning until the reply parses (Lich's
    /// `update_urchin_expire`), or the deadline passes. Drained per tick.
    pending_urchin_refresh: Option<(u32, std::time::Instant)>,
    /// A `.go2` waiting on the Chronomage day-pass sack scan: (destination,
    /// deadline, pass ids being `look`ed at). Lich's `mapdb_find_day_pass`
    /// sweep — the cache must learn what's held BEFORE routing so a held pair
    /// routes at 0.8. Empty ids = the one-time contents probe (open + look in).
    /// The bool records whether the sack was ALREADY open ("That is already
    /// open" seen) — then the scan doesn't close it (the user keeps it open).
    pending_day_pass_scan: Option<(u32, std::time::Instant, Vec<String>, bool)>,
    /// The day-pass sack contents probe has run this session (the container
    /// stream keeps contents fresh after the first open).
    day_pass_sack_probed: bool,
    /// Macro sleep segments (`look\rs2\rhide`): commands waiting out
    /// their pause, drained by take_outbound once due (insertion order
    /// preserved among same-tick due commands).
    timed_commands: Vec<(std::time::Instant, String)>,
    /// Cache for the wire-format map scene sent to web clients, keyed by
    /// (scene Arc pointer, sheet, building cluster) so a rebuild only
    /// happens when the drawn view actually changes.
    remote_map_cache: Option<(
        (usize, crate::core::layout_engine::Sheet, Option<usize>),
        std::sync::Arc<crate::core::remote::RemoteMapScene>,
    )>,
    /// Map revision as of the last remote flush; lets poll_map push a
    /// freshly generated layout to phones without waiting for game text.
    last_remote_map_revision: u64,
    /// Browse requests waiting on async layout generation:
    /// (client_id, request_id, location).
    pending_map_views: Vec<(u64, u64, String)>,

    /// Session-only mapping observations (forage sense, ranger sense),
    /// keyed by room uid. Dies on relog by design — see core::evidence.
    pub evidence: crate::core::evidence::EvidenceStore,

    pub nav_room_id: Option<String>,

    /// Lich room ID extracted from room display
    pub lich_room_id: Option<String>,

    /// Throttled doll variant/hidden rules from the active skin, resolved
    /// per remote flush so phone clients get the active variant name and
    /// suppressed parts pushed instead of evaluating conditions in JS.
    pub doll_rules: crate::core::doll_rules::DollRulesCache,

    /// Room subtitle (e.g., " - Emberthorn Refuge, Bowery")
    pub room_subtitle: Option<String>,

    /// Room component buffers (id -> lines of segments)
    /// Components: "room desc", "room objs", "room players", "room exits"
    pub room_components: HashMap<String, Vec<Vec<TextSegment>>>,

    /// Current room component being built
    pub current_room_component: Option<String>,

    /// Flag indicating room window needs sync
    pub room_window_dirty: bool,

    // === Runtime Flags ===
    /// Application running flag
    pub running: bool,

    /// Dirty flag - true if state changed and needs re-render
    pub needs_render: bool,

    /// Track if current chunk has main stream text
    pub chunk_has_main_text: bool,

    /// Track if current chunk has silent updates (vitals, buffs, etc.)
    pub chunk_has_silent_updates: bool,

    /// Track if layout has been modified since last .savelayout
    pub layout_modified_since_save: bool,

    /// When the layout last changed; drives the debounced autosave
    /// (tick_layout_autosave). None = nothing pending.
    pub layout_autosave_pending: Option<std::time::Instant>,

    /// Track if save reminder has been shown this session
    pub save_reminder_shown: bool,

    /// TUI-only: materialize the command_input window even when the
    /// layout marks it hidden (the TUI has no fallback input bar; the
    /// GUI shows its fixed bottom panel instead). The hidden flag itself
    /// is preserved so the GUI preference survives TUI sessions.
    pub force_show_command_input: bool,

    /// Set by `.reconnect` (via `UiAction::Reconnect`); the frontend runtime
    /// owns the network channels, so it drains this once per tick and
    /// re-establishes the connection. Core can't reconnect itself — it has no
    /// handle to the socket task — so this is the hand-off point.
    pub reconnect_requested: bool,

    /// Set by `.quit` when `ui.keep_open_on_quit` applies: the frontend
    /// runtime drains this once per tick and closes the network connection
    /// WITHOUT exiting the app (scrollback stays; `.reconnect`/`.launch`
    /// resume, a second `.quit` or `.exit` closes the window).
    pub disconnect_requested: bool,

    /// Whether the running frontend honors `disconnect_requested` (set at
    /// startup by the desktop TUI/GUI runtimes). The headless/web runtime
    /// doesn't — its `.quit` keeps today's semantics — and without this gate
    /// a keep-open `.quit` there would set a flag nobody drains and become a
    /// no-op.
    pub detach_quit_supported: bool,

    /// Set by a `.launch <character>` in a frontend whose runtime loop owns the
    /// network (the TUI). Core can't SSH or attach itself, so it stashes the
    /// character name here and the runtime drains it once per tick, runs the
    /// SSH-launcher flow, and attaches. `None` = no pending launch.
    pub launch_requested: Option<String>,

    /// Base layout name for autosave reference
    pub base_layout_name: Option<String>,

    // === Keybind Runtime Cache ===
    /// Runtime keybind map for fast O(1) lookups (KeyEvent -> KeyBindAction)
    /// Built from config.keybinds at startup and on config reload,
    /// then merged with hotbar button hotkeys (as Macro entries)
    pub keybind_map: HashMap<crate::data::input::KeyEvent, crate::config::KeyBindAction>,

    /// Hotbar hotkeys that lost a conflict with an existing binding
    /// (keybinds.toml or an earlier hotbar button). Editors surface these.
    pub hotbar_key_conflicts: Vec<crate::core::app_core::keybinds::HotbarKeyConflict>,

    /// Item classifier from the data pack, built on first use.
    /// `.data reload` drops it so the next use re-resolves sources.
    pub gameobj_data: Option<std::sync::Arc<crate::core::gameobj_data::GameObjData>>,

    /// `.foreach` batch runner (automation lease root when active).
    pub foreach: crate::core::foreach::ForeachService,

    // === Dialog Position Persistence ===
    /// Saved dialog positions loaded from widget_state.toml
    /// Updated when dialogs with save='t' are dragged/resized
    pub saved_dialog_positions: SavedDialogPositions,

    /// Discovery memory (window_registry.toml): every dialog/stream
    /// binding this character has ever seen, so windows stay addable in
    /// fresh layouts before the game re-declares them. Dark in Phase 1 —
    /// recorded here, consumed by the Phase 3 Windows-list union.
    pub window_registry: crate::config::WindowRegistry,
    /// Unflushed registry changes; written by `tick_layout_autosave`.
    window_registry_dirty: bool,
    /// Character state changed since the last persist; flushed to the session
    /// cache by `tick_layout_autosave` (rare, so no debounce — same as the
    /// registry). The generation last written, to detect real changes.
    character_state_saved_gen: u64,

    // === Lich WebUI bridge (owned in core so BOTH the GUI and the phone
    // render the same trees; see core::app_core::webui) ===
    /// The live bridge socket to Lich's WebUI server. None until a handshake
    /// starts it; the frontend supplies its tokio Handle to `start_webui`.
    pub(crate) webui_bridge: Option<crate::webui::WebUiHandle>,
    /// Raw bridge events from the socket task, drained each tick by
    /// `pump_webui`. None until the bridge starts.
    pub(crate) webui_rx: Option<tokio::sync::mpsc::UnboundedReceiver<crate::webui::WebUiEvent>>,
    /// The sender half, cloned into `fetch_image` calls so results return
    /// on the same channel `pump_webui` drains.
    pub(crate) webui_event_tx:
        Option<tokio::sync::mpsc::UnboundedSender<crate::webui::WebUiEvent>>,
    /// (host, port, token) for `/files/` image fetches; set at handshake.
    pub(crate) webui_endpoint: Option<(String, u16, String)>,
    /// True once a `;ui handshake` has been dispatched this session, so it
    /// isn't re-sent every tick.
    pub(crate) webui_handshake_sent: bool,
    /// Registered pages from the last `hello`/`pages` envelope (mirrored to
    /// the phone; the GUI reads them for its page picker).
    pub(crate) webui_pages: Vec<crate::data::webui::WebUiPageDescriptor>,
    /// Whether Lich WebUI is reachable this session (only when Lich-attached;
    /// a direct eAccess connection has no Lich, so no WebUI). Advertised to
    /// the phone so it shows the WebUI affordance only when usable.
    pub(crate) webui_available: bool,
    /// GUI re-emit channel: `pump_webui` forwards every bridge event here so
    /// the GUI can do its GUI-side handling (image textures, window kinds)
    /// while core owns the socket. None in headless/TUI (no local renderer).
    pub(crate) webui_gui_tx:
        Option<tokio::sync::mpsc::UnboundedSender<crate::webui::WebUiEvent>>,
    /// Raw game commands core queued for the frontend to send (the WebUI
    /// `;ui handshake` — core has no game socket). Drained each tick.
    pub(crate) webui_pending_raw: Vec<String>,
    /// Pages any client has subscribed to; replayed on a fresh socket's Hello
    /// so renders resume after a reconnect.
    pub(crate) webui_subscribed: std::collections::HashSet<String>,
}

impl AppCore {
    /// Item classifier (gameobj-data.xml), built on first use from the
    /// data pack: Lich folder > local store > bundled snapshot.
    pub fn gameobj_data(&mut self) -> std::sync::Arc<crate::core::gameobj_data::GameObjData> {
        if self.gameobj_data.is_none() {
            let resolved = crate::core::data_pack::resolve(
                &crate::core::data_pack::GAMEOBJ_DATA,
                self.config.map.lich_dir.as_deref(),
            );
            let data = crate::core::gameobj_data::GameObjData::parse(&resolved.content);
            tracing::info!(
                "gameobj-data loaded from {}: {} types, {} sellable, {} skipped regexes",
                resolved.source.label(),
                data.type_count(),
                data.sellable_count(),
                data.skipped.len()
            );
            self.gameobj_data = Some(std::sync::Arc::new(data));
        }
        self.gameobj_data
            .clone()
            .expect("gameobj_data initialized above")
    }

    /// Cached item classifier for immutable contexts (widget rendering).
    /// None until `gameobj_data()` has built it — the frontends prime it
    /// once per frame from their mutable phase, so render paths can rely
    /// on it after the first frame.
    pub fn gameobj_data_cached(&self) -> Option<&crate::core::gameobj_data::GameObjData> {
        self.gameobj_data.as_deref()
    }

    /// Drop and rebuild the item classifier from the data pack, in both
    /// AppCore and the message processor (the sorter's copy). Returns the
    /// reloaded type count. Shared by `.data reload` and Settings > Data.
    pub fn reload_data_pack(&mut self) -> usize {
        self.gameobj_data = None;
        self.message_processor.reset_gameobj_cache();
        self.gameobj_data().type_count()
    }

    /// Create a new AppCore instance
    /// Disk-free constructor for unit tests: default config, empty layout,
    /// no cmdlist/sound, TTS disabled. Never touches VELLUM_FE_DIR.
    #[cfg(test)]
    pub(crate) fn new_for_test() -> Self {
        let config = Config::default();
        let layout = Layout {
            windows: Vec::new(),
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };
        let saved_dialog_positions: crate::config::SavedDialogPositions = Default::default();
        let message_processor =
            MessageProcessor::new(config.clone(), saved_dialog_positions.clone());
        let parser = XmlParser::with_presets(Vec::new(), config.event_patterns.clone());
        let tts_manager = crate::tts::TtsManager::new(false, 1.0, 1.0);
        let keybind_map = Self::build_keybind_map(&config);
        let temp = std::env::temp_dir().join("vellum-fe-test");

        Self {
            config,
            map: crate::core::map_service::MapService::new(
                temp.join("cache"),
                temp.join("map_overrides.json"),
            ),
            map_updater: crate::core::mapdb_update::MapDbUpdater::new(temp.join("mapdb")),
            jinx_worker: crate::core::jinx::worker::JinxWorker::new(None),
            custom_status_expiries: std::collections::HashMap::new(),
            indicator_templates: std::collections::HashMap::new(),
            jinx_catalog: None,
            jinx_nudge_pending: true,
            travel: Default::default(),
            pending_urchin_refresh: None,
            pending_day_pass_scan: None,
            day_pass_sack_probed: false,
            timed_commands: Vec::new(),
            remote_map_cache: None,
            last_remote_map_revision: 0,
            pending_map_views: Vec::new(),
            layout: layout.clone(),
            baseline_layout: Some(layout),
            game_state: GameState::new(),
            ui_state: UiState::new(),
            parser,
            message_processor,
            current_stream: String::from("main"),
            discard_current_stream: false,
            stream_buffer: String::new(),
            server_time_offset: 0,
            cmdlist: None,
            menu_request_counter: 0,
            pending_menu_requests: HashMap::new(),
            menu_categories: HashMap::new(),
            last_link_click_pos: None,
            perf_stats: PerformanceStats::new(),
            show_perf_stats: false,
            sound_player: None,
            tts_manager,
            pending_haptics: Vec::new(),
            haptic_prev: Default::default(),
            last_highlight_rumble: None,
            evidence: crate::core::evidence::EvidenceStore::default(),
            nav_room_id: None,
            lich_room_id: None,
            doll_rules: Default::default(),
            room_subtitle: None,
            room_components: HashMap::new(),
            current_room_component: None,
            room_window_dirty: false,
            running: true,
            needs_render: true,
            chunk_has_main_text: false,
            chunk_has_silent_updates: false,
            layout_modified_since_save: false,
            layout_autosave_pending: None,
            save_reminder_shown: false,
            force_show_command_input: false,
            reconnect_requested: false,
            disconnect_requested: false,
            detach_quit_supported: false,
            launch_requested: None,
            base_layout_name: None,
            keybind_map,
            hotbar_key_conflicts: Vec::new(),
            gameobj_data: None,
            foreach: Default::default(),
            saved_dialog_positions,
            window_registry: Default::default(),
            window_registry_dirty: false,
            character_state_saved_gen: 0,
            webui_bridge: None,
            webui_rx: None,
            webui_event_tx: None,
            webui_endpoint: None,
            webui_handshake_sent: false,
            webui_pages: Vec::new(),
            webui_available: false,
            webui_gui_tx: None,
            webui_pending_raw: Vec::new(),
            webui_subscribed: std::collections::HashSet::new(),
        }
    }

    pub fn new(config: Config) -> Result<Self> {
        // Load layout from file system
        let layout = Layout::load(config.character.as_deref())?;

        // Load command list
        let cmdlist = CmdList::load()
            .inspect_err(|e| tracing::warn!("Failed to load command list: {}", e))
            .ok();

        // Scan ~/.vellum-fe/emoji/ for custom emoji so `:name:` shortcodes
        // resolve from the first line. Cheap and non-fatal when the dir is
        // absent; rescanned on `.reload`.
        let custom_emoji_count = crate::core::custom_emoji::reload();
        if custom_emoji_count > 0 {
            tracing::info!("Loaded {custom_emoji_count} custom emoji");
        }

        // Load saved dialog positions from widget_state.toml
        let saved_dialog_positions = Config::load_dialog_positions(config.character.as_deref())
            .unwrap_or_default();

        // Discovery memory: load (missing/corrupt = empty), then seed the
        // well-known feeds on first run so a fresh character's registry
        // starts useful. The constructor never writes; a seeded registry
        // is marked dirty and the frontend-driven autosave tick flushes
        // it (keeps constructor-only tests off the filesystem).
        let mut window_registry = Config::load_window_registry(config.character.as_deref());
        let window_registry_dirty = window_registry.seed_well_known();

        // Create message processor (shares saved_dialog_positions reference)
        let message_processor = MessageProcessor::new(config.clone(), saved_dialog_positions.clone());

        // Convert presets from config to parser format, resolving palette names to hex values
        let preset_list: Vec<(String, Option<String>, Option<String>)> = config
            .colors
            .presets
            .iter()
            .map(|(id, preset)| {
                let resolved_fg = preset.fg.as_ref().map(|c| config.resolve_palette_color(c));
                let resolved_bg = preset.bg.as_ref().map(|c| config.resolve_palette_color(c));
                (id.clone(), resolved_fg, resolved_bg)
            })
            .collect();

        // Create parser with presets and event patterns
        let parser = XmlParser::with_presets(preset_list, config.event_patterns.clone());

        // Initialize sound player (if sound feature is enabled)
        // If enabled = false, skips audio device initialization entirely
        let sound_player = crate::sound::SoundPlayer::new(
            config.sound.enabled,
            config.sound.volume,
            config.sound.cooldown_ms,
        )
        .inspect_err(|e| {
            // Err is the normal path when sound is disabled - only warn if enabled
            if config.sound.enabled {
                tracing::warn!("Failed to initialize sound player: {}", e);
            }
        })
        .ok();
        if sound_player.is_some() {
            tracing::debug!("Sound player initialized");
            // Ensure sounds directory exists
            if let Err(e) = crate::sound::ensure_sounds_directory() {
                tracing::warn!("Failed to create sounds directory: {}", e);
            }
        }

        // Initialize TTS manager (respects config.tts.enabled)
        let tts_manager = crate::tts::TtsManager::new(
            config.tts.enabled,
            config.tts.rate,
            config.tts.volume
        );
        if config.tts.enabled {
            tracing::info!("TTS enabled - accessibility features active");
        }

        // Build the runtime keybind map from config, then merge hotbar
        // button hotkeys (existing bindings win; conflicts surfaced below)
        let mut keybind_map = Self::build_keybind_map(&config);
        let hotbar_key_conflicts = Self::merge_hotbar_hotkeys(&mut keybind_map, &config.hotbars);

        let layout_theme = layout.theme.clone();
        let map_base = Config::base_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let map_cache_dir = map_base.join("cache").join("layouts");
        let map_overrides_path = map_base.join("map_overrides.json");

        let mut app = Self {
            config,
            map: crate::core::map_service::MapService::new(map_cache_dir, map_overrides_path),
            map_updater: crate::core::mapdb_update::MapDbUpdater::new(
                crate::core::mapdb_update::download_dir(&map_base),
            ),
            jinx_worker: crate::core::jinx::worker::JinxWorker::new(None),
            custom_status_expiries: std::collections::HashMap::new(),
            indicator_templates: std::collections::HashMap::new(),
            jinx_catalog: None,
            jinx_nudge_pending: true,
            travel: Default::default(),
            pending_urchin_refresh: None,
            pending_day_pass_scan: None,
            day_pass_sack_probed: false,
            timed_commands: Vec::new(),
            remote_map_cache: None,
            last_remote_map_revision: 0,
            pending_map_views: Vec::new(),
            layout: layout.clone(),
            baseline_layout: Some(layout),
            game_state: GameState::new(),
            ui_state: UiState::new(),
            parser,
            message_processor,
            current_stream: String::from("main"),
            discard_current_stream: false,
            stream_buffer: String::new(),
            server_time_offset: 0,
            cmdlist,
            menu_request_counter: 0,
            pending_menu_requests: HashMap::new(),
            menu_categories: HashMap::new(),
            last_link_click_pos: None,
            perf_stats: PerformanceStats::new(),
            show_perf_stats: false,
            sound_player,
            tts_manager,
            pending_haptics: Vec::new(),
            haptic_prev: Default::default(),
            last_highlight_rumble: None,
            evidence: crate::core::evidence::EvidenceStore::default(),
            nav_room_id: None,
            lich_room_id: None,
            doll_rules: Default::default(),
            room_subtitle: None,
            room_components: HashMap::new(),
            current_room_component: None,
            room_window_dirty: false,
            running: true,
            needs_render: true,
            chunk_has_main_text: false,
            chunk_has_silent_updates: false,
            layout_modified_since_save: false,
            layout_autosave_pending: None,
            save_reminder_shown: false,
            force_show_command_input: false,
            reconnect_requested: false,
            disconnect_requested: false,
            detach_quit_supported: false,
            launch_requested: None,
            base_layout_name: None,
            keybind_map,
            hotbar_key_conflicts,
            gameobj_data: None,
            foreach: Default::default(),
            saved_dialog_positions,
            window_registry,
            window_registry_dirty,
            character_state_saved_gen: 0,
            webui_bridge: None,
            webui_rx: None,
            webui_event_tx: None,
            webui_endpoint: None,
            webui_handshake_sent: false,
            webui_pages: Vec::new(),
            webui_available: false,
            webui_gui_tx: None,
            webui_pending_raw: Vec::new(),
            webui_subscribed: std::collections::HashSet::new(),
        };

        for conflict in &app.hotbar_key_conflicts.clone() {
            app.add_system_message(&format!(
                "Hotbar key '{}' ({}:{}) not registered - already bound by {}",
                conflict.key, conflict.bar, conflict.button, conflict.conflicts_with
            ));
        }

        for entry in app.layout.unknown_windows.clone() {
            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let widget_type = entry
                .get("widget_type")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            app.add_system_message(&format!(
                "Layout window '{}' skipped: widget type '{}' not supported by this build (kept in layout.toml)",
                name, widget_type
            ));
        }

        app.apply_session_cache();
        app.apply_custom_quickbars();
        app.refresh_tts_windows();
        app.refresh_indicator_templates();
        app.apply_tts_settings();

        if let Some((theme_id, _)) = app.apply_layout_theme(layout_theme.as_deref()) {
            app.add_system_message(&format!("Theme switched to: {}", theme_id));
            // Update frontend cache later; AppCore just updates config here.
            // The frontend will refresh during initialization from config.
        }

        app.refresh_map_source();

        Ok(app)
    }

    /// Rebuild the cached indicator-template map (UPPERCASE id -> entry) from
    /// disk. Call at startup and after the indicator-template editor saves —
    /// the render loop reads the cache, never the file.
    pub fn refresh_indicator_templates(&mut self) {
        self.indicator_templates = crate::config::Config::list_indicator_templates()
            .into_iter()
            .map(|entry| (entry.id.to_ascii_uppercase(), entry))
            .collect();

        // Ids "claimed" by a template's condition states — a combined indicator
        // (e.g. one POSTURE template with when=Standing/Kneeling/... states)
        // owns those raw ids, so the dashboard's runtime auto-discovery must
        // NOT also add them as separate orphan cells. Stored uppercase to match
        // the parser's Icon-stripped, case-preserved ids.
        let claimed: std::collections::HashSet<String> = self
            .indicator_templates
            .values()
            .flat_map(|tpl| {
                let mut ids = Vec::new();
                for state in &tpl.states {
                    state.when.referenced_indicator_ids(&mut ids);
                }
                ids
            })
            .map(|id| id.to_ascii_uppercase())
            .collect();
        self.message_processor.set_claimed_indicator_ids(claimed);
    }

    /// Look up a status template by id (case-insensitive) from the cache.
    pub fn indicator_template(
        &self,
        id: &str,
    ) -> Option<&crate::config::IndicatorTemplateEntry> {
        self.indicator_templates.get(&id.to_ascii_uppercase())
    }

    /// Whether some indicator template's condition `states` reference this id
    /// (case-insensitive) — i.e. a combined indicator "claims" it, so a raw
    /// dashboard cell for the id should not be auto-added. Mirrors the claimed
    /// set the message processor uses for the server-indicator path.
    pub fn indicator_id_is_claimed(&self, id: &str) -> bool {
        let target = id.to_ascii_uppercase();
        self.indicator_templates.values().any(|tpl| {
            let mut ids = Vec::new();
            for state in &tpl.states {
                state.when.referenced_indicator_ids(&mut ids);
            }
            ids.iter().any(|rid| rid.to_ascii_uppercase() == target)
        })
    }

    /// Rebuild the message processor's set of TTS-opted windows from the
    /// layout. Call after layout load and whenever a window's tts_speak
    /// flag or name changes.
    pub fn refresh_tts_windows(&mut self) {
        let windows: std::collections::HashSet<String> = self
            .layout
            .windows
            .iter()
            .filter(|def| def.base().tts_speak)
            .map(|def| def.name().to_string())
            .collect();
        self.message_processor.set_tts_windows(windows);
    }

    /// Push the config's TTS settings (enabled, rate, volume, voice,
    /// filters) into the live manager. Call at startup and after the
    /// settings editor saves.
    pub fn apply_tts_settings(&mut self) {
        // The message processor gates enqueue on its own config copy;
        // keep it in sync or runtime changes wait for a restart.
        self.message_processor
            .set_tts_config(self.config.tts.clone());
        let tts = &self.config.tts;
        self.tts_manager.set_enabled(tts.enabled);
        let _ = self.tts_manager.set_rate(tts.rate);
        let _ = self.tts_manager.set_volume(tts.volume);
        self.tts_manager.set_voice_by_name(tts.voice.clone());
        let substitutions: Vec<(String, String)> = tts
            .substitutions
            .iter()
            .map(|sub| (sub.pattern.clone(), sub.replacement.clone()))
            .collect();
        self.tts_manager.set_filters(&tts.gags, &substitutions);
    }

    /// Reconcile the live `SoundPlayer` with `config.sound`.
    ///
    /// Without this the sound config was write-only: the keybind toggle and the
    /// settings editor mutated `config.sound` and saved to disk, but the running
    /// player kept its construction-time fields, so changes did nothing until a
    /// restart. Because `SoundPlayer::new(enabled = false, ..)` returns `Err`
    /// (audio device init is skipped when disabled), a player that started
    /// disabled is `None` and cannot be re-enabled by a setter — it must be
    /// reconstructed. Call this after any change to `config.sound`.
    pub fn apply_sound_settings(&mut self) {
        let sound = self.config.sound.clone();
        match self.sound_player.as_mut() {
            Some(player) => {
                if sound.enabled {
                    // Live player exists and stays enabled: push the new knobs.
                    player.set_enabled(true);
                    player.set_volume(sound.volume);
                    player.set_cooldown_ms(sound.cooldown_ms);
                } else {
                    // Drop the player so the audio device is released; a later
                    // enable reconstructs it.
                    self.sound_player = None;
                    tracing::debug!("Sound player disabled and released");
                }
            }
            None if sound.enabled => {
                // Enabling from a disabled/None state: build a fresh player.
                match crate::sound::SoundPlayer::new(true, sound.volume, sound.cooldown_ms) {
                    Ok(player) => {
                        self.sound_player = Some(player);
                        tracing::debug!("Sound player initialized on enable");
                        if let Err(e) = crate::sound::ensure_sounds_directory() {
                            tracing::warn!("Failed to create sounds directory: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to initialize sound player on enable: {}", e);
                        self.add_system_message(
                            "Could not enable sound: no audio device available",
                        );
                    }
                }
            }
            None => {
                // Already disabled and no player — nothing to do.
            }
        }
    }

    /// Resolve the mapdb source from config and (re)start the load when it
    /// changes. Called at startup, after the settings editor saves, and when
    /// the updater installs a fresh download.
    pub fn refresh_map_source(&mut self) {
        let base = Config::base_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let source = crate::core::map_service::resolve_source(
            self.config.map.mapdb_path.as_deref(),
            self.config.map.lich_dir.as_deref(),
            self.config.connection.game.as_deref(),
            &crate::core::mapdb_update::download_dir(&base),
        );
        self.map.ensure_db(source);
    }

    /// Drain the map worker and the mapdb updater; a freshly installed
    /// download is picked up immediately. Frontends call this once per frame.
    pub fn poll_map(&mut self) {
        self.map.poll();
        if self.map_updater.poll() {
            self.refresh_map_source();
        }
        // Announce download completion everywhere — on phones there is no
        // settings panel to watch, only the game text.
        if let Some(status) = self.map_updater.take_finished() {
            use crate::core::mapdb_update::UpdateStatus;
            let text = match status {
                UpdateStatus::Updated { tag } => format!("map data {tag} installed"),
                UpdateStatus::UpToDate { tag } => format!("map data already up to date ({tag})"),
                UpdateStatus::Failed(e) => format!("map download failed: {e}"),
                _ => "map update finished".to_string(),
            };
            self.add_system_message(&format!("[map] {text}"));
        }
        self.tick_urchin_refresh();
        self.tick_day_pass_scan();
        self.tick_travel();
        self.tick_foreach();
        self.poll_jinx();
        // Auto-clear expired highlight-set custom statuses.
        self.tick_custom_statuses();
        // Browse replies waiting on the layout worker.
        self.service_pending_map_views();
        // A layout that finished generating between game lines still needs
        // to reach phones; the flush is diff-based so this is cheap.
        if self.message_processor.remote.is_some()
            && self.map.revision != self.last_remote_map_revision
        {
            self.last_remote_map_revision = self.map.revision;
            self.flush_remote_state();
        }
    }

    /// Apply queued custom-status changes from matched highlights: flip any
    /// indicator/dashboard entry whose id matches, and track auto-clear
    /// deadlines. Statuses ride the exact indicator machinery the server's
    /// IconXXX updates use, so icons, grayscale, and TUI glyphs all apply.
    pub fn apply_pending_status_actions(&mut self) {
        let actions: Vec<_> = self
            .message_processor
            .pending_status_actions
            .drain(..)
            .collect();
        for action in actions {
            if let Some((id, duration)) = action.set {
                self.set_custom_status(&id, true);
                match duration {
                    Some(secs) if secs > 0.0 => {
                        self.custom_status_expiries.insert(
                            id.to_ascii_uppercase(),
                            std::time::Instant::now()
                                + std::time::Duration::from_secs_f32(secs),
                        );
                    }
                    _ => {
                        self.custom_status_expiries
                            .remove(&id.to_ascii_uppercase());
                    }
                }
            }
            if let Some(id) = action.clear {
                self.set_custom_status(&id, false);
                self.custom_status_expiries.remove(&id.to_ascii_uppercase());
            }
        }
    }

    /// Deactivate custom statuses whose duration ran out. Called once per
    /// frame alongside the other pollers.
    pub fn tick_custom_statuses(&mut self) {
        if self.custom_status_expiries.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let expired: Vec<String> = self
            .custom_status_expiries
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            self.custom_status_expiries.remove(&id);
            self.set_custom_status(&id, false);
        }
    }

    /// Flip every indicator/dashboard entry whose id matches (the same
    /// update the server's status indicators perform).
    fn set_custom_status(&mut self, id: &str, active: bool) {
        let claimed = self.indicator_id_is_claimed(id);
        for window in self.ui_state.windows.values_mut() {
            match &mut window.content {
                crate::data::WindowContent::Indicator(ref mut indicator) => {
                    if indicator.indicator_id.eq_ignore_ascii_case(id) {
                        indicator.active = active;
                    }
                }
                crate::data::WindowContent::Dashboard { indicators } => {
                    let mut found = false;
                    for (indicator_id, value) in indicators.iter_mut() {
                        if indicator_id.eq_ignore_ascii_case(id) {
                            *value = if active { 1 } else { 0 };
                            found = true;
                            break;
                        }
                    }
                    // Same claim guard as the server-indicator path: don't
                    // auto-add an id a combined indicator template owns.
                    if !found && active && !claimed {
                        indicators.push((id.to_string(), 1));
                    }
                }
                _ => {}
            }
        }
        self.needs_render = true;
    }

    /// Drain the asset-manager worker: print each line to the game text and
    /// apply any post-install effect. Called once per frame from `poll_map`,
    /// alongside the map worker it mirrors.
    pub fn poll_jinx(&mut self) {
        let updates = self.jinx_worker.poll();
        for update in updates {
            self.add_system_message(&update.line);
            if let Some(effect) = update.effect {
                self.apply_jinx_effect(effect);
            }
        }
    }

    /// Apply a post-install side effect on the main thread (reloads touch
    /// `AppCore` and can't run on the worker). Reloads that already exist run
    /// live; kinds whose reload plumbing isn't built yet say so plainly rather
    /// than silently leaving a stale in-memory copy.
    fn apply_jinx_effect(&mut self, effect: crate::core::jinx::worker::Effect) {
        use crate::core::jinx::worker::Effect;
        match effect {
            Effect::Installed { name, kind } => match name.as_str() {
                // gameobj-data.xml re-resolves live: drop the cache and the
                // next classify() reads the freshly installed global/data copy.
                "gameobj-data.xml" => {
                    let types = self.reload_data_pack();
                    self.add_system_message(&format!(
                        "[jinx] gameobj classifier reloaded ({types} types)"
                    ));
                }
                // effect-list.xml re-reads live: spell_table prefers the
                // freshly installed global/data copy and swaps its table.
                "effect-list.xml" => {
                    let count = crate::core::spell_table::reload();
                    self.add_system_message(&format!(
                        "[jinx] spell table reloaded ({count} spells)"
                    ));
                }
                // mapdb.json landed in the map dir; resolve_source now
                // recognizes a plain mapdb.json (below any versioned release),
                // so re-resolving the source loads it live.
                "mapdb.json" => {
                    self.refresh_map_source();
                    self.add_system_message("[jinx] map database reloaded");
                }
                _ => match kind.as_str() {
                    // A skin's files land under skins/<name>/; list_skins and
                    // load_manifest read that dir live, so the new skin is
                    // immediately selectable. Activation stays user-driven
                    // (accessibility-first: never auto-restyle). Suggest the
                    // exact .setskin command, using the skin's dir name (the
                    // archive extension stripped).
                    "skin" => {
                        let skin_name = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
                        self.add_system_message(&format!(
                            "[jinx] skin installed — activate with .setskin {skin_name}"
                        ));
                    }
                    "iconmap" | "image" | "icon" => self.add_system_message(&format!(
                        "[jinx] {name} installed to the icon pool"
                    )),
                    // A doll base image lands in the doll pool; a skin points
                    // its [injury_doll] base at it (paths may be absolute).
                    "doll" => self.add_system_message(&format!(
                        "[jinx] {name} installed to the doll pool"
                    )),
                    _ => {
                        tracing::info!("jinx installed {name} ({kind}); no reload hook");
                    }
                },
            },
            // Stash the catalog for the GUI Assets panel to read; no core
            // side effect (the panel renders it and drives install/update).
            Effect::Catalog(entries) => {
                self.jinx_catalog = Some(entries);
            }
        }
    }

    fn apply_custom_quickbars(&mut self) {
        use crate::config::{QuickbarEntryConfig, QuickbarDefinition};
        use crate::data::{QuickbarData, QuickbarEntry};

        fn is_quickbar_id(id: &str) -> bool {
            let trimmed = id.trim();
            trimmed == "quick" || trimmed.starts_with("quick-")
        }

        fn normalize_title(title: &Option<String>) -> Option<String> {
            title
                .as_ref()
                .map(|t| t.trim())
                .filter(|t| !t.is_empty())
                .map(|t| t.to_string())
        }

        fn insert_quickbar(
            state: &mut crate::data::UiState,
            def: &QuickbarDefinition,
        ) {
            let id = def.id.trim();
            if id.is_empty() {
                return;
            }

            if !is_quickbar_id(id) {
                tracing::warn!("Skipping custom quickbar with invalid id '{}'", id);
                return;
            }

            let mut entries = Vec::new();
            for (index, entry) in def.entries.iter().enumerate() {
                match entry {
                    QuickbarEntryConfig::Link { label, command, echo } => {
                        let value = label.trim();
                        let cmd = command.trim();
                        if value.is_empty() || cmd.is_empty() {
                            continue;
                        }
                        entries.push(QuickbarEntry::Link {
                            id: format!("custom-{}", index + 1),
                            value: value.to_string(),
                            cmd: cmd.to_string(),
                            echo: echo.clone().filter(|s| !s.trim().is_empty()),
                        });
                    }
                    QuickbarEntryConfig::MenuLink { label, exist, noun } => {
                        let value = label.trim();
                        let exist_id = exist.trim();
                        let noun_value = noun.trim();
                        if value.is_empty() || exist_id.is_empty() || noun_value.is_empty() {
                            continue;
                        }
                        entries.push(QuickbarEntry::MenuLink {
                            id: format!("custom-menu-{}", index + 1),
                            value: value.to_string(),
                            exist: exist_id.to_string(),
                            noun: noun_value.to_string(),
                        });
                    }
                    QuickbarEntryConfig::Separator => {
                        entries.push(QuickbarEntry::Separator);
                    }
                }
            }

            let data = QuickbarData {
                id: id.to_string(),
                title: normalize_title(&def.title),
                entries,
            };
            state.quickbars.insert(id.to_string(), data);
            if !state.quickbar_order.contains(&id.to_string()) {
                state.quickbar_order.push(id.to_string());
            }
        }

        if self.config.quickbars.custom.is_empty() && self.config.quickbars.default.is_none() {
            return;
        }

        for def in &self.config.quickbars.custom {
            insert_quickbar(&mut self.ui_state, def);
        }

        if let Some(default_id) = self.config.quickbars.default.as_ref() {
            let trimmed = default_id.trim();
            if is_quickbar_id(trimmed) {
                if self.ui_state.quickbars.contains_key(trimmed) {
                    self.ui_state.active_quickbar_id = Some(trimmed.to_string());
                } else {
                    tracing::warn!(
                        "Quickbar default '{}' not found in custom quickbars",
                        trimmed
                    );
                }
            } else if !trimmed.is_empty() {
                tracing::warn!(
                    "Quickbar default '{}' is not a valid quickbar id",
                    trimmed
                );
            }
        }
    }

    fn apply_session_cache(&mut self) {
        let Some(cache) = crate::session_cache::load(self.config.character.as_deref()) else {
            return;
        };

        // Warm-start the character state (society/house/profession/citizenship)
        // so the travel gates work immediately on connect. The live feed stays
        // authoritative — any SOCIETY/INFO/PROFILE output or a resign/join/step
        // event overwrites this via the parser.
        if let Some(character) = cache.character.clone() {
            self.game_state.character = character;
        }

        if !cache.quickbars.is_empty() {
            let allowed_ids = self.allowed_quickbar_ids();
            let quickbars: HashMap<String, QuickbarData> = cache
                .quickbars
                .iter()
                .filter(|(id, _)| allowed_ids.contains(*id))
                .map(|(id, data)| (id.clone(), data.clone()))
                .collect();
            let quickbar_order: Vec<String> = cache
                .quickbar_order
                .iter()
                .filter(|id| allowed_ids.contains(*id))
                .cloned()
                .collect();
            let active_quickbar_id = cache
                .active_quickbar_id
                .as_ref()
                .and_then(|id| if allowed_ids.contains(id) { Some(id.clone()) } else { None });

            self.ui_state.quickbars = quickbars;
            self.ui_state.quickbar_order = quickbar_order;
            self.ui_state.active_quickbar_id = active_quickbar_id;

            if self.ui_state.quickbar_order.is_empty() {
                let mut ids: Vec<String> = self.ui_state.quickbars.keys().cloned().collect();
                ids.sort();
                self.ui_state.quickbar_order = ids;
            } else {
                for id in self.ui_state.quickbars.keys() {
                    if !self.ui_state.quickbar_order.contains(id) {
                        self.ui_state.quickbar_order.push(id.clone());
                    }
                }
            }

            if let Some(active_id) = self.ui_state.active_quickbar_id.as_ref() {
                if !self.ui_state.quickbars.contains_key(active_id) {
                    self.ui_state.active_quickbar_id = None;
                }
            }
        }

    }

    fn allowed_quickbar_ids(&self) -> HashSet<String> {
        let mut ids = HashSet::new();
        ids.insert("quick".to_string());
        ids.insert("quick-combat".to_string());
        ids.insert("quick-simu".to_string());

        for def in &self.config.quickbars.custom {
            let id = def.id.trim();
            if !id.is_empty() {
                ids.insert(id.to_string());
            }
        }

        if let Some(default_id) = self.config.quickbars.default.as_ref() {
            let id = default_id.trim();
            if !id.is_empty() {
                ids.insert(id.to_string());
            }
        }

        ids
    }

    /// Poll TTS events from callback channel and handle them.
    /// Should be called in the main event loop to enable auto-play.
    pub fn poll_tts_events(&mut self) {
        use std::sync::mpsc::TryRecvError;

        loop {
            match self.tts_manager.try_recv_event() {
                Ok(event) => {
                    match event {
                        crate::tts::TtsEvent::UtteranceEnded => {
                            // Chains the next unread queue entry (auto-play).
                            self.tts_manager.handle_utterance_ended();
                        }
                        crate::tts::TtsEvent::UtteranceStarted => {
                            tracing::debug!("Utterance started");
                        }
                        crate::tts::TtsEvent::UtteranceStopped => {
                            self.tts_manager.handle_utterance_stopped();
                        }
                    }
                }
                Err(TryRecvError::Empty) => {
                    // No more events to process
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    tracing::error!("TTS event channel disconnected");
                    break;
                }
            }
        }
        // Watchdog: drains the queue even when the platform never delivers
        // utterance-end callbacks (observed on Windows).
        self.tts_manager.pump();
    }

    /// Process incoming XML data from server
    pub fn process_server_data(&mut self, data: &str) -> Result<()> {
        // First game text of the session = a good moment for the one-shot
        // game-data staleness nudge (every frontend funnels through here).
        if self.jinx_nudge_pending {
            self.jinx_nudge_pending = false;
            self.emit_stale_data_nudge();
        }
        // Parse timing lives here so every frontend gets it for free —
        // runtimes must not also time this call (double counting).
        let parse_start = std::time::Instant::now();
        let result = self.process_server_data_inner(data);
        self.perf_stats.record_parse(parse_start.elapsed());
        result
    }

    /// Emit a once-per-session reminder when installed game data is old (or was
    /// installed before timestamping). Silent when nothing is stale or nothing
    /// is tracked. Threshold: 30 days. Cheap: reads jinx-installed.toml once.
    fn emit_stale_data_nudge(&mut self) {
        const STALE_DAYS: i64 = 30;
        let Ok(db) = crate::core::jinx::metadata::InstalledDb::load() else {
            return;
        };
        // Only game-data assets drive the nudge (effect-list/gameobj/mapdb) —
        // art staleness isn't worth nagging about.
        let now = chrono::Utc::now().timestamp();
        let mut stale = 0;
        let mut untracked = 0;
        for asset in db.assets.values().filter(|a| a.kind == "data") {
            match asset.last_updated {
                Some(ts) if (now - ts) / 86_400 >= STALE_DAYS => stale += 1,
                Some(_) => {}
                None => untracked += 1,
            }
        }
        if stale + untracked == 0 {
            return;
        }
        let n = stale + untracked;
        self.add_system_message(&format!(
            "[jinx] {n} game-data file{} may be out of date — run .jinx auto-update to refresh (or .jinx gui)",
            if n == 1 { "" } else { "s" }
        ));
    }

    fn process_server_data_inner(&mut self, data: &str) -> Result<()> {
        // Handle empty input (blank line from server) - "".lines() yields nothing!
        // Network reads line-by-line, so blank lines arrive as empty strings.
        // We must handle this explicitly since Rust's lines() returns an empty iterator for "".
        if data.is_empty() {
            // Parser already handles empty input: returns vec![Text { content: "" }]
            let elements = self.parser.parse_line(data);
            for element in elements {
                self.process_element(&element)?;
            }
            self.message_processor
                .flush_current_stream_with_tts(&mut self.ui_state, Some(&mut self.tts_manager));

            // Transfer pending sounds from MessageProcessor to GameState
            for sound in self.message_processor.pending_sounds.drain(..) {
                self.game_state.queue_sound(sound);
            }
            // Highlight-driven rumble joins the haptic queue (cooldown inside).
            self.queue_highlight_rumbles();
            // Highlight-driven custom statuses flip their indicators.
            self.apply_pending_status_actions();

            // Attribute mapping observations to the current room uid
            if !self.message_processor.pending_evidence.is_empty() {
                let uid = self
                    .nav_room_id
                    .as_deref()
                    .and_then(|s| s.trim().parse::<i64>().ok())
                    .filter(|&u| u != 0);
                for obs in self.message_processor.pending_evidence.drain(..) {
                    if let Some(uid) = uid {
                        self.evidence.record(
                            uid,
                            self.game_state.room_name.clone(),
                            obs,
                            self.game_state.game_time,
                        );
                    }
                }
            }

            // A pathcode NPC spoke a route: persist it for the maze whose
            // entrance we're standing at (works mid-.go2 or asked by hand).
            if let Some(route) = self.message_processor.pending_pathcode.take() {
                let maze = self
                    .map
                    .current_room_id
                    .and_then(crate::core::travel::mazes::maze_at_entrance);
                if let Some(maze) = maze {
                    let steps = route.len();
                    self.config.go2.pathcodes.insert(maze.name.clone(), route);
                    if let Err(e) = self.save_config() {
                        tracing::warn!("pathcode save failed: {e}");
                    }
                    self.add_system_message(&format!(
                        "[go2] pathcode for {} captured ({steps} steps)",
                        maze.name
                    ));
                } else {
                    tracing::debug!("pathcode heard away from any maze entrance; ignored");
                }
            }

            // Transfer bounty buffer to GameState if any
            if let Some((raw_text, compact_lines)) = self.message_processor.take_bounty_buffer() {
                self.game_state.bounty.update(raw_text, compact_lines);
            }

            // Transfer society buffer to GameState if any
            let society_lines = self.message_processor.take_society_buffer();
            if !society_lines.is_empty() {
                self.game_state.society.update(society_lines);
            }

            return Ok(());
        }

        // Parse XML line by line
        for line in data.lines() {
            let elements = self.parser.parse_line(line);
            if !elements.is_empty() {
                self.perf_stats
                    .record_elements_parsed(elements.len() as u64);
            }

            // Process each element
            for element in elements {
                self.process_element(&element)?;
            }

            // Finish the current line after processing all elements from this network line
            // This ensures newlines from the game are preserved (like VellumFE does)
            self.message_processor
                .flush_current_stream_with_tts(&mut self.ui_state, Some(&mut self.tts_manager));

            // Transfer pending sounds from MessageProcessor to GameState
            for sound in self.message_processor.pending_sounds.drain(..) {
                self.game_state.queue_sound(sound);
            }
            // Highlight-driven rumble joins the haptic queue (cooldown inside).
            self.queue_highlight_rumbles();
            // Highlight-driven custom statuses flip their indicators.
            self.apply_pending_status_actions();

            // Attribute mapping observations to the current room uid
            if !self.message_processor.pending_evidence.is_empty() {
                let uid = self
                    .nav_room_id
                    .as_deref()
                    .and_then(|s| s.trim().parse::<i64>().ok())
                    .filter(|&u| u != 0);
                for obs in self.message_processor.pending_evidence.drain(..) {
                    if let Some(uid) = uid {
                        self.evidence.record(
                            uid,
                            self.game_state.room_name.clone(),
                            obs,
                            self.game_state.game_time,
                        );
                    }
                }
            }

            // A pathcode NPC spoke a route: persist it for the maze whose
            // entrance we're standing at (works mid-.go2 or asked by hand).
            if let Some(route) = self.message_processor.pending_pathcode.take() {
                let maze = self
                    .map
                    .current_room_id
                    .and_then(crate::core::travel::mazes::maze_at_entrance);
                if let Some(maze) = maze {
                    let steps = route.len();
                    self.config.go2.pathcodes.insert(maze.name.clone(), route);
                    if let Err(e) = self.save_config() {
                        tracing::warn!("pathcode save failed: {e}");
                    }
                    self.add_system_message(&format!(
                        "[go2] pathcode for {} captured ({steps} steps)",
                        maze.name
                    ));
                } else {
                    tracing::debug!("pathcode heard away from any maze entrance; ignored");
                }
            }

            // Transfer bounty buffer to GameState if any
            if let Some((raw_text, compact_lines)) = self.message_processor.take_bounty_buffer() {
                self.game_state.bounty.update(raw_text, compact_lines);
            }

            // Transfer society buffer to GameState if any
            let society_lines = self.message_processor.take_society_buffer();
            if !society_lines.is_empty() {
                self.game_state.society.update(society_lines);
            }
        }

        self.sync_map_room();
        // Automation reacts to whatever this line changed (room, RT,
        // status); the per-frame tick covers pure time-based waits.
        self.tick_travel();
        self.tick_foreach();

        Ok(())
    }

    /// Seed default quickbars when attaching without login bursts.
    /// Intended for non-direct connections where login-only data is missing.
    pub fn seed_default_quickbars_if_empty(&mut self) {
        let has_quick = self.ui_state.quickbars.contains_key("quick");
        let has_quick_combat = self.ui_state.quickbars.contains_key("quick-combat");
        let has_quick_simu = self.ui_state.quickbars.contains_key("quick-simu");
        if has_quick && has_quick_combat && has_quick_simu {
            return;
        }

        let quickbar_lines = [
            (
                "quick",
                "<openDialog id=\"quick\" location=\"quickBar\" title=\"main  \"><dialogData id=\"quick\" clear=\"true\"><link id=\"2\" value=\"look\" cmd=\"look\" echo=\"look\"/><sep/><menuLink id=\"3\" value=\"roleplay...\" exist=\"qlinkrp\" noun=\"\" width=\"\" left=\"\"/><menuLink id=\"18\" value=\"actions...\" exist=\"qlinkmech\" noun=\"\" width=\"\" left=\"\"/><link id=\"4\" value=\"search\" cmd=\"search\" echo=\"search\"/><sep/><link id=\"5\" value=\"inventory\" cmd=\"inven\" echo=\"inventory\"/><sep/><link id=\"6\" value=\"character sheet\" cmd=\"_info character\" echo=\"info\"/><sep/><link id=\"7\" value=\"skill goals\" cmd=\"goals\"/><sep/><link id=\"13\" value=\"directions\" cmd=\"dir\" echo=\"directions\"/><sep/><sep/><link id=\"19\" value=\"get assistance\" cmd=\"assist\" echo=\"assist\"/><sep/><link id=\"17\" value=\"society\" cmd=\"society\" echo=\"society\"/><sep/><link id=\"21\" value=\"SimuCoins\" cmd=\"simucoin\" echo=\"simucoin\"/><sep/></dialogData></openDialog>",
            ),
            (
                "quick-combat",
                "<openDialog id=\"quick-combat\" location=\"quickBar\" title=\"combat\"><dialogData id=\"quick-combat\" clear=\"true\"><link id=\"2\" value=\"look\" cmd=\"look\" echo=\"look\"/><sep/><link id=\"3\" value=\"attack\" cmd=\"attack\" echo=\"attack\"/><sep/><link id=\"4\" value=\"ambush\" cmd=\"ambush\" echo=\"ambush\"/><sep/><link id=\"5\" value=\"aim\" cmd=\"aim\" echo=\"aim\"/><sep/><link id=\"6\" value=\"target\" cmd=\"target\" echo=\"target\"/><sep/><link id=\"7\" value=\"fire\" cmd=\"fire\" echo=\"fire\"/><sep/><link id=\"8\" value=\"multistrike\" cmd=\"mstrike\" echo=\"mstrike\"/><sep/><link id=\"9\" value=\"targeted multistrike\" cmd=\"mstrike target\" echo=\"mstrike target\"/><sep/><link id=\"8\" value=\"maneuvers\" cmd=\"cman\" echo=\"cman\"/></dialogData></openDialog>",
            ),
            (
                "quick-simu",
                "<openDialog id=\"quick-simu\" location=\"quickBar\" title=\"information\"><dialogData id=\"quick-simu\" clear=\"true\"><link id=\"1\" value=\"policy\" cmd=\"policy\" echo=\"policy\"/><sep/><link id=\"2\" value=\"news\" cmd=\"url:/gs4/news.asp\"/><sep/><link id=\"3\" value=\"calendar\" cmd=\"url:/gs4/events/\"/><sep/><link id=\"4\" value=\"documentation\" cmd=\"url:/gs4/info/\"/><sep/><link id=\"5\" value=\"premium\" cmd=\"premium\" echo=\"premium\"/><sep/><link id=\"6\" value=\"platinum\" cmd=\"url:/gs4/platinum/\"/><sep/><link id=\"7\" value=\"maps\" cmd=\"url:/bounce/redirect.asp?URL=https://gswiki.play.net/Category:World\"/><sep/><link id=\"8\" value=\"Discord\" cmd=\"url:/bounce/redirect.asp?URL=https://discord.gg/gs4\"/><sep/><link id=\"9\" value=\"version notes\" cmd=\"url:/gs4/play/wrayth/notes.asp\"/><sep/><link id=\"10\" value=\"SimuCoins Store\" cmd=\"url:/bounce/redirect.asp?URL=http://store.play.net/store/purchase/GS\"/></dialogData></openDialog>",
            ),
        ];

        for (id, line) in quickbar_lines {
            if self.ui_state.quickbars.contains_key(id) {
                continue;
            }
            if let Err(e) = self.process_server_data(line) {
                tracing::warn!("Failed to seed default quickbar line: {}", e);
            }
        }
    }

    /// Process a single parsed XML element
    fn process_element(&mut self, element: &ParsedElement) -> Result<()> {
        // Handle MenuResponse specially (needs access to cmdlist and menu state)
        if let ParsedElement::MenuResponse { id, coords } = element {
            self.message_processor.chunk_has_silent_updates = true; // Mark as silent update
            self.handle_menu_response(id, coords);
            self.needs_render = true;
            return Ok(());
        }

        // Update game state and UI state via message processor
        self.message_processor.process_element(
            element,
            &mut self.game_state,
            &mut self.ui_state,
            &mut self.room_components,
            &mut self.current_room_component,
            &mut self.room_window_dirty,
            &mut self.nav_room_id,
            &mut self.lich_room_id,
            &mut self.room_subtitle,
            Some(&mut self.tts_manager),
        );

        // Mark that we need to render
        self.needs_render = true;

        Ok(())
    }

    /// Send command to server

    /// Handle dot commands (local client commands)

    /// Get list of available dot commands for tab completion
    pub fn get_available_commands(&self) -> Vec<String> {
        vec![
            // Application commands
            ".quit".to_string(),
            ".q".to_string(),
            ".help".to_string(),
            ".h".to_string(),
            ".?".to_string(),
            ".reload".to_string(),
            // Layout commands
            ".savelayout".to_string(),
            ".loadlayout".to_string(),
            ".layouts".to_string(),
            ".resize".to_string(),
            // UI pack sharing
            ".uiexport".to_string(),
            ".uiimport".to_string(),
            // Window management
            ".windows".to_string(),
            ".deletewindow".to_string(),
            ".delwindow".to_string(),
            ".addwindow".to_string(),
            ".rename".to_string(),
            ".border".to_string(),
            ".editwindow".to_string(),
            ".editwin".to_string(),
            ".hidewindow".to_string(),
            ".hidewin".to_string(),
            // Highlight commands
            ".highlights".to_string(),
            ".hl".to_string(),
            ".addhighlight".to_string(),
            ".addhl".to_string(),
            ".edithighlight".to_string(),
            ".edithl".to_string(),
            ".testline".to_string(),
            ".savehighlights".to_string(),
            ".savehl".to_string(),
            ".loadhighlights".to_string(),
            ".loadhl".to_string(),
            ".highlightprofiles".to_string(),
            ".hlprofiles".to_string(),
            // Keybind commands
            ".keybinds".to_string(),
            ".kb".to_string(),
            ".addkeybind".to_string(),
            ".addkey".to_string(),
            ".savekeybinds".to_string(),
            ".savekb".to_string(),
            ".loadkeybinds".to_string(),
            ".loadkb".to_string(),
            ".keybindprofiles".to_string(),
            ".kbprofiles".to_string(),
            // Color commands
            ".colors".to_string(),
            ".colorpalette".to_string(),
            ".addcolor".to_string(),
            ".createcolor".to_string(),
            ".uicolors".to_string(),
            ".spellcolors".to_string(),
            ".addspellcolor".to_string(),
            ".newspellcolor".to_string(),
            ".setpalette".to_string(),
            ".resetpalette".to_string(),
            // Theme commands
            ".themes".to_string(),
            ".settheme".to_string(),
            ".theme".to_string(),
            ".edittheme".to_string(),
            // Skin commands (GUI)
            ".skins".to_string(),
            ".setskin".to_string(),
            ".skin".to_string(),
            ".makeskin".to_string(),
            ".reloadskin".to_string(),
            // Tab navigation
            ".nexttab".to_string(),
            ".prevtab".to_string(),
            ".gonew".to_string(),
            ".nextunread".to_string(),
            // Settings
            ".settings".to_string(),
            // Toggles
            ".toggletransparency".to_string(),
            ".transparency".to_string(),
            // Window locking (toggle)
            ".lockwindows".to_string(),
            ".lockall".to_string(),
            // Containers
            ".hidecontainers".to_string(),
            // Menu system
            ".menu".to_string(),
        ]
    }

    /// Get list of window names for tab completion
    pub fn get_window_names(&self) -> Vec<String> {
        self.layout
            .windows
            .iter()
            .map(|w| w.name().to_string())
            .collect()
    }

    /// Get the current game type from config
    pub fn game_type(&self) -> Option<crate::config::GameType> {
        crate::config::GameType::from_game_string(self.config.connection.game.as_deref())
    }

    /// Generate a unique spacer widget name based on existing spacers in layout
    /// Uses max number + 1 algorithm, checking ALL widgets including hidden ones
    /// Pattern: spacer_1, spacer_2, spacer_3, etc.
    pub fn generate_spacer_name(layout: &Layout) -> String {
        let max_number = layout
            .windows
            .iter()
            .filter_map(|w| {
                // Only consider spacer widgets
                match w {
                    crate::config::WindowDef::Spacer { base, .. } => {
                        // Extract number from name like "spacer_5"
                        if let Some(num_str) = base.name.strip_prefix("spacer_") {
                            num_str.parse::<u32>().ok()
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .max()
            .unwrap_or(0);

        format!("spacer_{}", max_number + 1)
    }

    /// Feed-injected dot-commands (`<vellumCmd cmd=".."/>`, emitted by Lich
    /// scripts) waiting for the frontend's normal dot-command dispatch.
    /// Drained once per frame/tick by each frontend.
    pub fn take_pending_client_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.message_processor.pending_client_commands)
    }

    /// Consume a pending `.reconnect` request (see `reconnect_requested`).
    /// Returns true at most once per request; the frontend runtime acts on it.
    pub fn take_reconnect_request(&mut self) -> bool {
        std::mem::take(&mut self.reconnect_requested)
    }

    /// Consume a pending keep-open `.quit` request (see
    /// `disconnect_requested`). Returns true at most once per request; the
    /// frontend runtime closes the connection but keeps the app running.
    pub fn take_disconnect_request(&mut self) -> bool {
        std::mem::take(&mut self.disconnect_requested)
    }

    /// Consume a pending `.launch <character>` request (see `launch_requested`).
    /// Returns the character name at most once per request; the frontend
    /// runtime then runs the SSH-launcher flow and attaches.
    pub fn take_launch_request(&mut self) -> Option<String> {
        std::mem::take(&mut self.launch_requested)
    }

    /// Add a system message to a window that receives the "main" stream.
    /// First tries window named "main", then looks for any window subscribed to "main" stream.
    pub fn add_system_message(&mut self, message: &str) {
        use crate::data::{SpanType, StyledLine, TextSegment, WindowContent};

        let line = StyledLine {
            segments: vec![TextSegment {
                text: message.to_string(),
                fg: Some(self.config.colors.ui.system_message_color.clone()),
                bg: None,
                bold: true,
                // Client output (.jinx tables, .layouts, errors) renders in
                // the window's mono font so structured info reads aligned
                // and stands apart from the game feed. The TUI is mono
                // regardless; the GUI switches fonts per segment.
                mono: true,
                span_type: SpanType::System, // system echo; skip highlight transforms
                link_data: None,
                custom_emoji: None,
            }],
            stream: String::from("main"),
            timestamp: None,
        };

        // System messages bypass the message pipeline, so mirror them to
        // remote clients explicitly (dot-command feedback, errors, ...)
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_text("main", std::sync::Arc::new(line.clone()));
        }

        // First try window named "main" (backward compatibility)
        if let Some(main_window) = self.ui_state.get_window_mut("main") {
            if let WindowContent::Text(ref mut content) = main_window.content {
                content.add_line(line);
                self.needs_render = true;
                return;
            }
        }

        // Otherwise, find any window subscribed to "main" stream
        // Check Text windows
        for window in self.ui_state.windows.values_mut() {
            match &mut window.content {
                WindowContent::Text(ref mut content) => {
                    if content.streams.iter().any(|s| s.eq_ignore_ascii_case("main")) {
                        content.add_line(line);
                        self.needs_render = true;
                        return;
                    }
                }
                WindowContent::TabbedText(ref mut content) => {
                    // Find tab subscribed to "main" stream
                    for tab in content.tabs.iter_mut() {
                        if tab.definition.streams.iter().any(|s| s.eq_ignore_ascii_case("main")) {
                            tab.content.add_line(line);
                            self.needs_render = true;
                            return;
                        }
                    }
                }
                _ => {}
            }
        }

        // No window found - log warning
        tracing::warn!("No window found subscribed to 'main' stream for system message: {}", message);
    }

    /// Inject a test line through the complete pipeline (parser → message processor → UI)
    /// This simulates receiving a line from the game server for testing highlights and squelch
    pub(super) fn inject_test_line(&mut self, text: &str) {
        // Parse the line as if it came from the game
        let elements = self.parser.parse_line(text);

        tracing::info!("[TESTLINE] Injecting test line: '{}'", text);
        tracing::debug!("[TESTLINE] Parsed {} elements", elements.len());

        // Process each element through the message processor
        for element in elements {
            if let Err(e) = self.process_element(&element) {
                tracing::error!("[TESTLINE] Failed to process element: {}", e);
            }
        }

        // Flush any accumulated segments to ensure the line is rendered
        self.message_processor.flush_current_stream(&mut self.ui_state);

        self.add_system_message(&format!("[TEST] Injected: {}", text));
        self.needs_render = true;
    }

    /// Show help for dot commands. Rendered from the command help table
    /// (command_help.rs) — the single source the dispatcher tripwire
    /// keeps in sync with the real command set, so this can no longer
    /// drift the way the hand-written list did.
    pub(super) fn show_help(&mut self) {
        for line in super::command_help::render_help_lines() {
            self.add_system_message(&line);
        }
    }

    /// Show version information
    pub(super) fn show_version(&mut self) {
        let version = env!("CARGO_PKG_VERSION");
        self.add_system_message(&format!("VellumFE v{}", version));
    }

    /// Start search mode (Ctrl+F)
    pub fn start_search_mode(&mut self) {
        self.ui_state.input_mode = crate::data::ui_state::InputMode::Search;
        self.ui_state.search_input.clear();
        self.ui_state.search_cursor = 0;
        self.needs_render = true;
    }

    /// Get the focused window name (or "main" as default)
    pub fn get_focused_window_name(&self) -> String {
        self.ui_state
            .focused_window
            .clone()
            .unwrap_or_else(|| "main".to_string())
    }

    /// Clear search mode
    pub fn clear_search_mode(&mut self) {
        // Exit search mode
        if self.ui_state.input_mode == crate::data::ui_state::InputMode::Search {
            self.ui_state.input_mode = crate::data::ui_state::InputMode::Normal;
        }

        self.ui_state.search_input.clear();
        self.ui_state.search_cursor = 0;
        self.needs_render = true;
    }


}

/// Project one sheet of a generated scene into the phone wire format,
/// optionally filtered to a building's groups (the current-view push) or
/// unfiltered (location browsing).
fn wire_map_scene(
    scene: &crate::core::layout_engine::MapScene,
    sheet: crate::core::layout_engine::Sheet,
    filter: Option<&std::collections::HashSet<usize>>,
) -> crate::core::remote::RemoteMapScene {
    use crate::core::layout_engine::{SceneEdgeKind, Sheet};
    use crate::core::remote::{RemoteMapEdge, RemoteMapLabel, RemoteMapRoom, RemoteMapScene};

    let pass = |group: usize| filter.map_or(true, |set| set.contains(&group));
    let sheet_scene = scene.sheet(sheet);
    RemoteMapScene {
        location: scene.location.clone(),
        sheet: match sheet {
            Sheet::Outdoor => "outdoor".to_string(),
            Sheet::Interiors => "interiors".to_string(),
        },
        rooms: sheet_scene
            .rooms
            .iter()
            .filter(|r| pass(r.group))
            .map(|r| RemoteMapRoom {
                i: r.id,
                x: r.cell.x,
                y: r.cell.y,
                e: r.entrance,
            })
            .collect(),
        edges: sheet_scene
            .edges
            .iter()
            .filter(|e| pass(e.group))
            .map(|e| {
                let stub = e.kind == SceneEdgeKind::Stub;
                RemoteMapEdge {
                    x1: e.a.x,
                    y1: e.a.y,
                    x2: e.b.x,
                    y2: e.b.y,
                    k: match e.kind {
                        SceneEdgeKind::Directional => 0,
                        SceneEdgeKind::Connector => 1,
                        SceneEdgeKind::Stub => 2,
                    },
                    l: e.label.clone(),
                    ar: stub.then_some(e.a_room),
                    br: stub.then_some(e.b_room),
                }
            })
            .collect(),
        labels: sheet_scene
            .labels
            .iter()
            .filter(|l| pass(l.group))
            .map(|l| RemoteMapLabel {
                x: l.cell.x,
                y: l.cell.y,
                t: l.text.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Layout, WindowBase, WindowDef, SpacerWidgetData, BorderSides};

    #[test]
    fn reconnect_request_flag_is_consumed_exactly_once() {
        let mut core = AppCore::new_for_test();
        // Fresh core has no pending request.
        assert!(!core.take_reconnect_request());
        // The frontend dispatcher sets this on UiAction::Reconnect.
        core.reconnect_requested = true;
        // First drain sees it, then clears it — the runtime reconnects once.
        assert!(core.take_reconnect_request());
        assert!(!core.take_reconnect_request());
    }

    // Test helper to create a minimal WindowBase
    fn test_window_base(name: &str) -> WindowBase {
        WindowBase {
            name: name.to_string(),
            row: crate::data::geometry::Row::new(0),
            col: crate::data::geometry::Col::new(0),
            rows: crate::data::geometry::Height::new(2),
            cols: crate::data::geometry::Width::new(5),
            show_border: false,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: false,
            title: None,
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
            title_position: "top-left".to_string(),
        }
    }

    #[test]
    fn test_edit_picker_reaches_hidden_windows() {
        // A hidden spacer must appear in the edit picker's template map when
        // include_hidden is set, and stay out of the visible-only map.
        let mut base = test_window_base("spacer_1");
        base.visibility = crate::config::WindowVisibility::Hidden;
        let layout = Layout {
            windows: vec![WindowDef::Spacer {
                base,
                data: SpacerWidgetData {},
            }],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };

        let with_hidden =
            crate::core::local_catalog::layout_windows_by_category(&layout, false, true);
        assert!(with_hidden
            .get(&crate::config::WidgetCategory::Other)
            .is_some_and(|names| names.iter().any(|n| n == "spacer_1")));

        let visible_only =
            crate::core::local_catalog::visible_by_category(&layout, false);
        assert!(!visible_only
            .get(&crate::config::WidgetCategory::Other)
            .is_some_and(|names| names.iter().any(|n| n == "spacer_1")));
    }

    #[test]
    fn test_generate_spacer_name_empty_layout() {
        // RED: With no spacers, should return spacer_1
        let layout = Layout {
            windows: vec![],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_1");
    }

    #[test]
    fn test_generate_spacer_name_single_spacer() {
        // RED: With one spacer_1, should return spacer_2
        let spacer1 = WindowDef::Spacer {
            base: test_window_base("spacer_1"),
            data: SpacerWidgetData {},
        };
        let layout = Layout {
            windows: vec![spacer1],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_2");
    }

    #[test]
    fn test_generate_spacer_name_multiple_spacers() {
        // RED: With spacer_1, spacer_2, spacer_3, should return spacer_4
        let spacer1 = WindowDef::Spacer {
            base: test_window_base("spacer_1"),
            data: SpacerWidgetData {},
        };
        let spacer2 = WindowDef::Spacer {
            base: test_window_base("spacer_2"),
            data: SpacerWidgetData {},
        };
        let spacer3 = WindowDef::Spacer {
            base: test_window_base("spacer_3"),
            data: SpacerWidgetData {},
        };
        let layout = Layout {
            windows: vec![spacer1, spacer2, spacer3],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_4");
    }

    #[test]
    fn test_generate_spacer_name_with_gaps() {
        // RED: With spacer_1 and spacer_3 (gap at 2), should return spacer_4 (max + 1)
        let spacer1 = WindowDef::Spacer {
            base: test_window_base("spacer_1"),
            data: SpacerWidgetData {},
        };
        let spacer3 = WindowDef::Spacer {
            base: test_window_base("spacer_3"),
            data: SpacerWidgetData {},
        };
        let layout = Layout {
            windows: vec![spacer1, spacer3],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_4");
    }

    #[test]
    fn test_format_category_label_standard() {
        assert_eq!(AppCore::format_category_label("cat_tools"), "Tools");
    }

    #[test]
    fn test_format_category_label_single_char() {
        assert_eq!(AppCore::format_category_label("x"), "X");
    }

    #[test]
    fn test_format_category_label_empty() {
        assert_eq!(AppCore::format_category_label(""), "Other");
    }

    #[test]
    fn test_generate_spacer_name_ignores_non_spacers() {
        // RED: Non-spacer widgets should be ignored
        let text_widget = WindowDef::Text {
            base: test_window_base("main"),
            data: crate::config::TextWidgetData {
                streams: vec!["main".to_string()],
                buffer_size: 1000,
                wordwrap: true,
                show_timestamps: false,
                timestamp_position: None,
                compact: false,
            },
        };
        let spacer1 = WindowDef::Spacer {
            base: test_window_base("spacer_1"),
            data: SpacerWidgetData {},
        };
        let layout = Layout {
            windows: vec![text_widget, spacer1],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_2");
    }

    #[test]
    fn test_generate_spacer_name_with_hidden_spacers() {
        // RED: Hidden spacers should be considered (widgets can be hidden, not deleted)
        let mut visible_base = test_window_base("spacer_1");
        visible_base.visibility = crate::config::WindowVisibility::Shown;

        let mut hidden_base = test_window_base("spacer_2");
        hidden_base.visibility = crate::config::WindowVisibility::Hidden;

        let visible_spacer = WindowDef::Spacer {
            base: visible_base,
            data: SpacerWidgetData {},
        };
        let hidden_spacer = WindowDef::Spacer {
            base: hidden_base,
            data: SpacerWidgetData {},
        };
        let layout = Layout {
            windows: vec![visible_spacer, hidden_spacer],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_3");
    }

    #[test]
    fn test_generate_spacer_name_non_sequential() {
        // RED: With spacer_2, spacer_5 (max is 5), should return spacer_6
        let spacer2 = WindowDef::Spacer {
            base: test_window_base("spacer_2"),
            data: SpacerWidgetData {},
        };
        let spacer5 = WindowDef::Spacer {
            base: test_window_base("spacer_5"),
            data: SpacerWidgetData {},
        };
        let layout = Layout {
            windows: vec![spacer2, spacer5],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_6");
    }

    #[test]
    fn test_generate_spacer_name_large_numbers() {
        // RED: Should handle large numbers correctly
        let spacer99 = WindowDef::Spacer {
            base: test_window_base("spacer_99"),
            data: SpacerWidgetData {},
        };
        let layout = Layout {
            windows: vec![spacer99],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_100");
    }

    // ========== calculate_window_positions characterization ==========
    // This is the load/init positioning pass: it copies each window's EXACT
    // col/row (no scaling — deliberately, so windows may sit offscreen) and
    // clamps width/height to any min/max constraints. Pin that contract before
    // a geometry newtype touches it.

    fn positioned_text_def(
        name: &str,
        col: u16,
        row: u16,
        cols: u16,
        rows: u16,
    ) -> WindowDef {
        let mut base = test_window_base(name);
        base.col = crate::data::geometry::Col::new(col);
        base.row = crate::data::geometry::Row::new(row);
        base.cols = crate::data::geometry::Width::new(cols);
        base.rows = crate::data::geometry::Height::new(rows);
        WindowDef::Text {
            base,
            data: crate::config::TextWidgetData {
                streams: vec![],
                buffer_size: 1000,
                wordwrap: true,
                show_timestamps: false,
                timestamp_position: None,
                compact: false,
            },
        }
    }

    fn core_with_layout(windows: Vec<WindowDef>) -> AppCore {
        let mut core = AppCore::new_for_test();
        core.layout = Layout {
            windows,
            terminal_width: Some(80),
            terminal_height: Some(24),
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };
        core
    }

    #[test]
    fn delete_and_stash_then_restore_roundtrips_the_exact_def() {
        let mut core = core_with_layout(vec![
            positioned_text_def("main", 0, 0, 40, 24),
            positioned_text_def("my_notes", 5, 5, 20, 8),
        ]);
        core.init_windows(80, 24);

        // Delete the custom window: gone from the layout, stashed.
        assert!(core.delete_and_stash_window("my_notes"));
        assert!(!core.layout.windows.iter().any(|w| w.name() == "my_notes"));
        assert!(!core.ui_state.windows.contains_key("my_notes"));
        assert_eq!(core.deleted_window_names(), vec!["my_notes".to_string()]);

        // Restore it: back in the layout with its exact geometry, live again.
        assert!(core.restore_deleted_window("my_notes", 80, 24));
        let def = core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == "my_notes")
            .expect("restored def present");
        assert_eq!(def.base().col.get(), 5);
        assert_eq!(def.base().row.get(), 5);
        assert_eq!(def.base().cols.get(), 20);
        assert!(def.base().visibility.is_shown());
        assert!(core.ui_state.windows.contains_key("my_notes"));
        // Stash is now empty.
        assert!(core.deleted_window_names().is_empty());
    }

    #[test]
    fn deleted_windows_for_restore_shows_title_not_internal_id() {
        // A custom window with an opaque id but a human title.
        let mut def = positioned_text_def("custom-text-1", 1, 1, 10, 5);
        def.base_mut().title = Some("Consumables".into());
        let mut core = core_with_layout(vec![def]);
        core.init_windows(80, 24);
        core.delete_and_stash_window("custom-text-1");

        let entries = core.deleted_windows_for_restore();
        assert_eq!(entries.len(), 1);
        let (name, title) = &entries[0];
        assert_eq!(name, "custom-text-1", "restore key is the stable id");
        assert_eq!(title, "Consumables", "menu shows the human title");

        // A titleless deleted window falls back to its name.
        core.delete_and_stash_window("custom-text-1"); // already stashed; re-add a bare one
        let mut bare = positioned_text_def("scratch-2", 0, 0, 5, 5);
        bare.base_mut().title = None;
        core.layout.windows.push(bare);
        core.init_windows(80, 24);
        core.delete_and_stash_window("scratch-2");
        let scratch = core
            .deleted_windows_for_restore()
            .into_iter()
            .find(|(n, _)| n == "scratch-2")
            .unwrap();
        assert_eq!(scratch.1, "scratch-2", "no title -> falls back to name");
    }

    #[test]
    fn re_deleting_after_restore_keeps_one_stash_copy() {
        let mut core = core_with_layout(vec![positioned_text_def("notes", 1, 1, 10, 5)]);
        core.init_windows(80, 24);
        core.delete_and_stash_window("notes");
        core.restore_deleted_window("notes", 80, 24);
        core.delete_and_stash_window("notes");
        // Only one stashed copy, not two.
        assert_eq!(core.deleted_window_names(), vec!["notes".to_string()]);
    }

    #[test]
    fn restore_refuses_when_name_is_reused_by_a_live_window() {
        let mut core = core_with_layout(vec![positioned_text_def("notes", 1, 1, 10, 5)]);
        core.init_windows(80, 24);
        core.delete_and_stash_window("notes");
        // A new window reuses the name.
        core.layout.windows.push(positioned_text_def("notes", 0, 0, 5, 5));
        core.init_windows(80, 24);
        // Restore is refused (won't clobber the live one); the stash keeps it.
        assert!(!core.restore_deleted_window("notes", 80, 24));
        assert_eq!(core.deleted_window_names(), vec!["notes".to_string()]);
    }

    #[test]
    fn deleted_windows_persist_through_layout_serialization() {
        let mut core = core_with_layout(vec![positioned_text_def("gone", 2, 2, 12, 6)]);
        core.init_windows(80, 24);
        core.delete_and_stash_window("gone");
        // Serialize + reparse the layout: the stash survives.
        let toml = toml::to_string(&core.layout).expect("serialize layout");
        assert!(toml.contains("deleted_windows"), "stash must serialize");
        let reparsed: Layout = toml::from_str(&toml).expect("reparse layout");
        assert_eq!(reparsed.deleted_windows.len(), 1);
        assert_eq!(reparsed.deleted_windows[0].name(), "gone");
    }

    /// Positions and sizes pass through exactly (no scaling), even when the
    /// window extends beyond the given terminal size.
    #[test]
    fn calculate_window_positions_uses_exact_values() {
        let core = core_with_layout(vec![
            positioned_text_def("a", 3, 5, 40, 10),
            positioned_text_def("offscreen", 100, 50, 20, 8), // beyond 80x24
        ]);
        let positions = core.calculate_window_positions(80, 24);

        let a = &positions["a"];
        assert_eq!(
            (a.x.get(), a.y.get(), a.width.get(), a.height.get()),
            (3, 5, 40, 10)
        );
        // Deliberately NOT clamped to the terminal — offscreen is allowed.
        let off = &positions["offscreen"];
        assert_eq!(
            (off.x.get(), off.y.get(), off.width.get(), off.height.get()),
            (100, 50, 20, 8)
        );
    }

    /// min/max constraints clamp the size (never the position).
    #[test]
    fn calculate_window_positions_applies_min_max_constraints() {
        let mut narrow = positioned_text_def("narrow", 0, 0, 4, 30);
        narrow.base_mut().min_cols = Some(10); // widen up to min
        narrow.base_mut().max_rows = Some(20); // cap height
        let core = core_with_layout(vec![narrow]);

        let p = &core.calculate_window_positions(80, 24)["narrow"];
        assert_eq!(p.x.get(), 0); // position untouched
        assert_eq!(p.y.get(), 0);
        assert_eq!(p.width.get(), 10); // 4 raised to min_cols
        assert_eq!(p.height.get(), 20); // 30 capped at max_rows
    }

    #[test]
    fn known_windows_menu_reflects_state_and_toggle_flips_it() {
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};
        let mut core = core_with_layout(vec![]);
        core.layout.terminal_width = Some(80);
        core.layout.terminal_height = Some(24);

        // Discover a stream → bound, Hidden layout entry named "thoughts".
        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
        core.realize_offered_windows(80, 24);

        // Fresh discovery: hidden → "[ ]" and a __TOGGLE_WINDOW__ command.
        let menu = core.build_known_windows_menu();
        let row = menu
            .iter()
            .find(|i| i.command == "__TOGGLE_WINDOW__thoughts")
            .unwrap();
        assert!(row.text.starts_with("[ ]"), "row: {}", row.text);
        assert!(row.text.contains("Thoughts"));

        // Toggle shows it (creates UI state).
        core.toggle_known_window("thoughts");
        assert!(core.ui_state.windows.contains_key("thoughts"));
        let menu = core.build_known_windows_menu();
        let row = menu
            .iter()
            .find(|i| i.command == "__TOGGLE_WINDOW__thoughts")
            .unwrap();
        assert!(row.text.starts_with("[x]"), "row: {}", row.text);

        // Toggle again hides it.
        core.toggle_known_window("thoughts");
        assert!(!core.ui_state.windows.contains_key("thoughts"));
    }

    fn renamed_widget(display_name: &str, template_name: &str) -> WindowDef {
        // A widget the user placed via the Windows list: built from a
        // template (so category/id fields are set) but the editor renamed
        // it to a custom-* display name, losing the template name.
        let mut def = crate::core::local_catalog::seed(template_name)
            .unwrap_or_else(|| panic!("no template '{}'", template_name));
        def.base_mut().name = display_name.to_string();
        def
    }

    #[test]
    fn dialog_readd_does_not_duplicate_a_renamed_singleton_widget() {
        // The bug: game re-sends the expr dialog; the user's placed widget
        // is "custom-gs4_experience-1", so the old exact-name check missed
        // it and spawned a duplicate on every re-send. U2: the pending
        // queue carries the DIALOG ID ("expr"); the equivalent renamed
        // widget gets ADOPTED (binding tagged) so re-sends resolve by id.
        let mut core = core_with_layout(vec![renamed_widget(
            "custom-gs4_experience-1",
            "gs4_experience",
        )]);
        assert_eq!(core.layout.windows.len(), 1);

        // Simulate several dialog re-sends (expr -> gs4_experience template).
        for _ in 0..3 {
            core.ui_state.pending_window_additions.push("expr".to_string());
            core.process_pending_window_additions(80, 24);
        }

        // Still exactly one gs4_experience window — no duplicate spawned...
        let count = core
            .layout
            .windows
            .iter()
            .filter(|w| w.widget_type() == "gs4_experience")
            .count();
        assert_eq!(count, 1, "duplicate gs4_experience window spawned");
        // ...and it was adopted: now bound to "expr".
        assert!(
            core.layout.has_window_bound_to("expr"),
            "the renamed widget should have been adopted and bound to expr"
        );
    }

    #[test]
    fn first_sight_creates_a_bound_window() {
        // No existing widget: the first expr feed creates a gs4_experience
        // window bound to "expr", and a re-send doesn't duplicate it.
        let mut core = core_with_layout(vec![]);
        core.ui_state.pending_window_additions.push("expr".to_string());
        core.process_pending_window_additions(80, 24);

        assert!(core.layout.has_window_bound_to("expr"));
        assert_eq!(
            core.layout.windows.iter().filter(|w| w.widget_type() == "gs4_experience").count(),
            1
        );

        // Re-send: still one.
        core.ui_state.pending_window_additions.push("expr".to_string());
        core.process_pending_window_additions(80, 24);
        assert_eq!(
            core.layout.windows.iter().filter(|w| w.widget_type() == "gs4_experience").count(),
            1
        );
    }

    #[test]
    fn one_feed_delivers_to_multiple_bound_windows() {
        // Nisugi's rule: 3 windows bound to "expr" all count as "exists"
        // (no new spawn) and windows_bound_to lists all of them for delivery.
        let mut core = core_with_layout(vec![]);
        for i in 0..3 {
            let mut def = crate::core::local_catalog::seed("gs4_experience").unwrap();
            def.base_mut().name = format!("xp{}", i);
            def.base_mut().binding =
                Some(crate::config::WindowBinding::Dialog("expr".to_string()));
            core.layout.windows.push(def);
        }
        // A feed for expr must NOT spawn a 4th window.
        core.ui_state.pending_window_additions.push("expr".to_string());
        core.process_pending_window_additions(80, 24);
        assert_eq!(core.layout.windows.len(), 3, "should not create a 4th");
        // All three are addressable for delivery.
        assert_eq!(core.layout.windows_bound_to("expr").len(), 3);
    }

    #[test]
    fn set_known_window_shown_flips_layout_visibility() {
        use crate::config::WindowVisibility;
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};
        let mut core = core_with_layout(vec![]);
        core.layout.terminal_width = Some(80);
        core.layout.terminal_height = Some(24);

        // Discover a stream (bound, Hidden layout entry).
        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
        core.realize_offered_windows(80, 24);
        let vis = |c: &AppCore| {
            c.layout
                .windows
                .iter()
                .find(|w| w.name() == "thoughts")
                .unwrap()
                .base()
                .visibility
        };
        assert_eq!(vis(&core), WindowVisibility::Hidden);

        // Show it by name → visibility flips to Shown + UI state created.
        core.set_known_window_shown("thoughts", true, 80, 24);
        assert_eq!(vis(&core), WindowVisibility::Shown);
        assert!(core.ui_state.windows.contains_key("thoughts"));

        // Hide it → back to Hidden, removed from UI state.
        core.set_known_window_shown("thoughts", false, 80, 24);
        assert_eq!(vis(&core), WindowVisibility::Hidden);
        assert!(!core.ui_state.windows.contains_key("thoughts"));
    }

    #[test]
    fn showing_a_dialog_window_syncs_shown_dialog_ids() {
        // U6: showing/hiding a dialog-bound window flips its id in
        // shown_dialog_ids, which the message processor's popup gate reads.
        use crate::config::WindowBinding;
        let mut core = core_with_layout(vec![]);
        core.layout.terminal_width = Some(80);
        core.layout.terminal_height = Some(24);
        let mut bank = crate::core::local_catalog::seed("stance").unwrap();
        bank.base_mut().name = "bank".to_string();
        bank.base_mut().binding = Some(WindowBinding::Dialog("bank".to_string()));
        bank.base_mut().visibility = crate::config::WindowVisibility::Hidden;
        core.layout.windows.push(bank);

        assert!(!core.ui_state.shown_dialog_ids.contains("bank"));
        core.set_known_window_shown("bank", true, 80, 24);
        assert!(core.ui_state.shown_dialog_ids.contains("bank"));
        core.set_known_window_shown("bank", false, 80, 24);
        assert!(!core.ui_state.shown_dialog_ids.contains("bank"));
    }

    #[test]
    fn showing_a_dialog_panel_does_not_pop_it_up_as_a_dialog() {
        // UberBar bug: a DialogPanel-bound dialog renders IN THE PANEL. Showing
        // it must NOT add its id to shown_dialog_ids, or every dialogData frame
        // would ALSO fire an active_dialog popup — a duplicate window (empty
        // panel + populated popup). Only true popup dialogs (bank) join the set.
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};
        let mut core = core_with_layout(vec![]);
        core.layout.terminal_width = Some(80);
        core.layout.terminal_height = Some(24);

        // Register UberBar the way the game does: a DialogPanel discovery.
        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "UberBar".to_string(),
            title: "Nisugi's Uberbar".to_string(),
            kind: WindowDiscoveryKind::DialogPanel,
            save: false,
        });
        core.realize_offered_windows(80, 24);
        assert!(
            core.layout.windows.iter().any(|w| matches!(
                w,
                crate::config::WindowDef::DialogPanel { .. }
            )),
            "the discovery should have created a DialogPanel window"
        );

        core.set_known_window_shown("UberBar", true, 80, 24);
        assert!(
            !core.ui_state.shown_dialog_ids.contains("UberBar"),
            "a DialogPanel must not join the popup allow-set (that causes the duplicate window)"
        );

        // The runtime window must carry DialogPanel content bound to the id —
        // NOT WindowContent::Empty (the blank-panel bug: add_new_window had no
        // DialogPanel arm, so the shown panel rendered nothing).
        let win = core
            .ui_state
            .windows
            .get("UberBar")
            .expect("shown UberBar has a runtime window");
        match &win.content {
            crate::data::WindowContent::DialogPanel { dialog_id } => {
                assert_eq!(dialog_id, "UberBar", "panel content bound to the dialog id");
            }
            other => panic!("expected DialogPanel content, got {:?}", other),
        }
    }

    #[test]
    fn deleting_a_shown_dialog_window_clears_the_popup_allow_set() {
        // Rysk's bug: show a dialog-bound window (seeds shown_dialog_ids),
        // then DELETE it. Delete must scrub the id from the popup allow-set
        // and drop any live popup — otherwise the next dialogData the game
        // sends re-pops the deleted dialog as a bare "Dialog" popup.
        use crate::config::WindowBinding;
        let mut core = core_with_layout(vec![]);
        core.layout.terminal_width = Some(80);
        core.layout.terminal_height = Some(24);
        let mut win = crate::core::local_catalog::seed("stance").unwrap();
        win.base_mut().name = "activespells".to_string();
        win.base_mut().binding = Some(WindowBinding::Dialog("activespells".to_string()));
        win.base_mut().visibility = crate::config::WindowVisibility::Hidden;
        core.layout.windows.push(win);

        core.set_known_window_shown("activespells", true, 80, 24);
        assert!(core.ui_state.shown_dialog_ids.contains("activespells"));
        // Simulate a live popup for this id.
        core.ui_state.active_dialog = Some(crate::data::DialogState::empty(
            "activespells".to_string(),
            Some("Dialog".to_string()),
        ));

        assert!(core.delete_and_stash_window("activespells"));
        assert!(
            !core.ui_state.shown_dialog_ids.contains("activespells"),
            "delete must remove the dialog id from the popup allow-set"
        );
        assert!(
            core.ui_state.active_dialog.is_none(),
            "delete must close a popup that was showing the deleted dialog"
        );
    }

    #[test]
    fn rediscovery_of_a_persisted_window_is_idempotent() {
        // U4: after a persisted discovered window reloads (simulated: a
        // bound Hidden layout entry already present), the game re-announcing
        // it must NOT create a duplicate, and must NOT force it visible.
        use crate::config::{WindowBinding, WindowVisibility};
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};
        let mut core = core_with_layout(vec![]);
        // Simulate a reloaded layout: combat already bound + Hidden.
        let mut combat = crate::core::local_catalog::seed("stance").unwrap();
        combat.base_mut().name = "combat".to_string();
        combat.base_mut().binding = Some(WindowBinding::Dialog("combat".to_string()));
        combat.base_mut().visibility = WindowVisibility::Hidden;
        core.layout.windows.push(combat);
        assert_eq!(core.layout.windows.len(), 1);

        // The game re-announces combat this session.
        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "combat".to_string(),
            title: "Combat".to_string(),
            kind: WindowDiscoveryKind::DialogPanel,
            save: false,
        });
        core.realize_offered_windows(80, 24);

        // No duplicate; still Hidden.
        assert_eq!(core.layout.windows_bound_to("combat").len(), 1);
        assert_eq!(
            core.layout.windows.iter().find(|w| w.name() == "combat").unwrap().base().visibility,
            WindowVisibility::Hidden
        );
    }

    #[test]
    fn stream_discovery_adopts_existing_subscriber_no_duplicate() {
        use crate::config::{WindowBinding, WindowDef};
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};

        // A single-stream text window already subscribes to "thoughts"
        // (like the default layout's thoughts window, unbound).
        let mut thoughts = crate::core::local_catalog::seed("text_custom").unwrap();
        thoughts.base_mut().name = "Thoughts".to_string();
        if let WindowDef::Text { data, .. } = &mut thoughts {
            data.streams.push("thoughts".to_string());
        }
        let mut core = core_with_layout(vec![thoughts]);

        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
        core.realize_offered_windows(80, 24);

        // No duplicate — the existing window was adopted (bound), not cloned.
        assert_eq!(core.layout.windows.len(), 1, "no duplicate thoughts window");
        assert_eq!(
            core.layout.windows[0].base().binding,
            Some(WindowBinding::Stream("thoughts".to_string()))
        );
    }

    #[test]
    fn stream_discovery_skips_when_a_tab_already_routes_it() {
        use crate::config::WindowDef;
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};

        // A tabbedtext window has a tab subscribing to "thoughts".
        let mut tabbed = crate::core::local_catalog::seed("tabbedtext_custom").unwrap();
        tabbed.base_mut().name = "chat".to_string();
        if let WindowDef::TabbedText { data, .. } = &mut tabbed {
            data.tabs.push(crate::config::TabbedTextTab {
                name: "Thoughts".to_string(),
                stream: Some("thoughts".to_string()),
                streams: vec!["thoughts".to_string()],
                ..Default::default()
            });
        }
        let mut core = core_with_layout(vec![tabbed]);

        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
        core.realize_offered_windows(80, 24);

        // No new window: the tab already routes it (whole tabbed window not
        // bound, since it carries many streams).
        assert_eq!(core.layout.windows.len(), 1, "no duplicate for tab-routed stream");
        assert!(core.layout.windows[0].base().binding.is_none());
    }

    #[test]
    fn window_discoveries_register_as_bound_hidden_layout_entries() {
        use crate::config::{WindowBinding, WindowVisibility};
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};
        let mut core = core_with_layout(vec![]);

        // A stream and a resident dialog panel are discovered.
        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "combat".to_string(),
            title: "Combat".to_string(),
            kind: WindowDiscoveryKind::DialogPanel,
            save: false,
        });
        core.realize_offered_windows(80, 24);

        // Both became bound, Hidden layout entries (known forever, not shown).
        assert!(core.layout.has_window_bound_to("thoughts"));
        assert!(core.layout.has_window_bound_to("combat"));
        for id in ["thoughts", "combat"] {
            let w = core
                .layout
                .windows
                .iter()
                .find(|w| w.base().binding.as_ref().is_some_and(|b| b.id() == id))
                .unwrap();
            assert_eq!(w.base().visibility, WindowVisibility::Hidden, "{id} hidden");
        }
        // The stream window subscribes to its stream id.
        let stream_win = core
            .layout
            .windows
            .iter()
            .find(|w| w.base().binding == Some(WindowBinding::Stream("thoughts".to_string())))
            .unwrap();
        if let crate::config::WindowDef::Text { data, .. } = stream_win {
            assert!(data.streams.contains(&"thoughts".to_string()));
        } else {
            panic!("stream discovery should be a text window");
        }

        // Idempotent: re-discovering doesn't add duplicates.
        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
        core.realize_offered_windows(80, 24);
        assert_eq!(core.layout.windows_bound_to("thoughts").len(), 1);
    }

    #[test]
    fn spells_stream_discovery_creates_a_spells_widget_not_text() {
        use crate::config::WindowBinding;
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};
        let mut core = core_with_layout(vec![]);

        // The game declares its spellbook window via <streamWindow id="Spells">.
        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "Spells".to_string(),
            title: "Spells".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
        core.realize_offered_windows(80, 24);

        // It must be the dedicated spells widget (whose buffer-replay pipeline
        // populates it), NOT a generic text window that would render empty.
        let win = core
            .layout
            .windows
            .iter()
            .find(|w| w.base().binding == Some(WindowBinding::Stream("Spells".to_string())))
            .expect("Spells stream should register a bound window");
        assert!(
            matches!(win, crate::config::WindowDef::Spells { .. }),
            "Spells stream discovery must produce a spells widget, got {:?}",
            win.widget_type()
        );
    }

    #[test]
    fn widget_backed_streams_discover_their_widget_not_text() {
        use crate::config::{WindowBinding, WindowDef};
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};

        // Each of these stream ids has a dedicated widget; auto-discovery must
        // produce that widget, not a generic (empty) text window.
        let cases: &[(&str, fn(&WindowDef) -> bool)] = &[
            ("Spells", |w| matches!(w, WindowDef::Spells { .. })),
            ("inv", |w| matches!(w, WindowDef::Inventory { .. })),
            ("reserve", |w| matches!(w, WindowDef::Reserve { .. })),
            ("room", |w| matches!(w, WindowDef::Room { .. })),
        ];

        for (id, is_expected) in cases {
            let mut core = core_with_layout(vec![]);
            core.ui_state.pending_window_discoveries.push(WindowDiscovery {
                id: id.to_string(),
                title: id.to_string(),
                kind: WindowDiscoveryKind::Stream,
                save: false,
            });
            core.realize_offered_windows(80, 24);
            let win = core
                .layout
                .windows
                .iter()
                .find(|w| w.base().binding == Some(WindowBinding::Stream(id.to_string())))
                .unwrap_or_else(|| panic!("stream '{id}' should register a bound window"));
            assert!(
                is_expected(win),
                "stream '{id}' must discover its widget, got {:?}",
                win.widget_type()
            );
        }
    }

    #[test]
    fn plain_text_streams_still_discover_a_text_window() {
        use crate::config::{WindowBinding, WindowDef};
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};
        // A stream with no dedicated widget stays a text window.
        let mut core = core_with_layout(vec![]);
        core.ui_state.pending_window_discoveries.push(WindowDiscovery {
            id: "custom_feed".to_string(),
            title: "Custom".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
        core.realize_offered_windows(80, 24);
        let win = core
            .layout
            .windows
            .iter()
            .find(|w| w.base().binding == Some(WindowBinding::Stream("custom_feed".to_string())))
            .expect("stream should register a window");
        assert!(matches!(win, WindowDef::Text { .. }));
    }

    #[test]
    fn enumerate_known_windows_covers_layout_and_ephemeral() {
        use crate::core::known_windows::KnownWindowKind;
        // A bound (discovered) hidden dialog window, an unbound plain
        // widget, and the un-hideable essentials.
        let mut core = core_with_layout(vec![]);
        let mut combat = crate::core::local_catalog::seed("stance").unwrap();
        combat.base_mut().name = "combat".to_string();
        combat.base_mut().title = Some("Combat".to_string());
        combat.base_mut().binding =
            Some(crate::config::WindowBinding::Dialog("combat".to_string()));
        combat.base_mut().visibility = crate::config::WindowVisibility::Hidden;
        core.layout.windows.push(combat);
        core.layout.windows.push(positioned_text_def("main", 0, 0, 40, 10)); // essential
        core.layout.windows.push(positioned_text_def("my_notes", 0, 0, 20, 5)); // plain

        let known = core.enumerate_known_windows();
        // "main" is listed like any other window (hideable under the
        // main-stream invariant — see hide_window).
        let main = known.iter().find(|k| k.name == "main").expect("main listed");
        assert!(main.shown);
        // The bound combat window is classified as a Dialog, hidden.
        let combat = known.iter().find(|k| k.name == "combat").expect("combat listed");
        assert_eq!(combat.kind, KnownWindowKind::Dialog);
        assert!(!combat.shown);
        assert_eq!(combat.title, "Combat");
        // The unbound widget is a plain Layout window.
        let notes = known.iter().find(|k| k.name == "my_notes").expect("notes listed");
        assert_eq!(notes.kind, KnownWindowKind::Layout);
        assert!(notes.shown);
    }

    /// Full-catalog rows: every template is listed even before it exists
    /// in the layout; seed templates and spacers stay out; a layout entry
    /// wins over its template row (no duplicates, live state preserved).
    #[test]
    fn enumerate_known_windows_lists_full_template_catalog() {
        let core = core_with_layout(vec![positioned_text_def("thoughts", 0, 0, 10, 5)]);
        let known = core.enumerate_known_windows();

        // Never-added template → unchecked row.
        let compass = known.iter().find(|k| k.name == "compass").expect("compass listed");
        assert!(!compass.shown);
        assert!(!compass.ephemeral);
        // main appears as a template row even though the layout lacks it.
        assert!(known.iter().any(|k| k.name == "main"));
        // Creation seeds are flows, not windows.
        assert!(!known.iter().any(|k| k.name.ends_with("_custom")));
        assert!(!known.iter().any(|k| k.name == "spacer"));
        // Layout entry dedups its template row and keeps live state.
        let thoughts: Vec<_> = known.iter().filter(|k| k.name == "thoughts").collect();
        assert_eq!(thoughts.len(), 1);
        assert!(thoughts[0].shown);
    }

    /// Ticking a catalog row whose template isn't in the layout yet
    /// conjures it: added to the layout shown + materialized in ui_state.
    #[test]
    fn set_known_window_shown_conjures_template_not_in_layout() {
        let mut core = core_with_layout(vec![]);
        assert!(core.layout.get_window("compass").is_none());
        core.set_known_window_shown("compass", true, 80, 24);
        assert!(core
            .layout
            .get_window("compass")
            .map(|w| w.base().visibility.is_shown())
            .unwrap_or(false));
        assert!(core.ui_state.windows.contains_key("compass"));
    }

    /// Regression: deleting a widget-backed window whose id the game ALSO
    /// feeds as a resident dialog (minivitals, expr, encum, Buffs, ...) and
    /// then re-showing it must restore its real WIDGET, not conjure a generic
    /// `panel_<id>` dialog panel.
    ///
    /// Repro (Rysk/Crinbar): minivitals owns the `minivitals` MiniVitals
    /// widget template, but the game streams `<dialogData id='minivitals'>`
    /// every vitals tick, which always accumulates into `dialog_store`. After
    /// deleting the window, `dialog_store.contains_key("minivitals")` was true,
    /// so `set_known_window_shown` built `panel_minivitals` instead of the
    /// widget. `set_known_window_shown` now checks the real widget template
    /// FIRST, ahead of the dialog-store and container conjure branches, so a
    /// widget-backed id can never be resurrected as a generic panel.
    #[test]
    fn reshowing_deleted_widget_backed_dialog_restores_widget_not_panel() {
        let minivitals_def = crate::core::local_catalog::seed("minivitals")
            .expect("minivitals template exists");
        let mut core = core_with_layout(vec![minivitals_def]);
        core.init_windows(80, 24);
        assert!(core.ui_state.windows.contains_key("minivitals"));

        // 1. Delete the widget window (stashed for restore).
        assert!(core.delete_and_stash_window("minivitals"));
        assert!(!core.ui_state.windows.contains_key("minivitals"));

        // 2. Game keeps streaming resident minivitals dialogData → dialog_store
        //    fills even though no window is bound to it.
        core.inject_test_line(
            "<dialogData id='minivitals'><progressBar id='mana' value='94' text='mana 386/407' left='76.7%' top='0%' width='23.3%' height='100%'/></dialogData>",
        );
        assert!(
            core.ui_state.dialog_store.contains_key("minivitals"),
            "resident dialogData should accumulate in the store"
        );

        // 3. Re-show minivitals from the Windows list.
        core.set_known_window_shown("minivitals", true, 80, 24);

        // The REAL widget is restored; no generic panel is conjured.
        assert!(
            core.ui_state.windows.contains_key("minivitals"),
            "minivitals widget must be restored"
        );
        assert!(
            !core.ui_state.windows.contains_key("panel_minivitals"),
            "no generic panel_minivitals may be created"
        );
        assert!(
            core.layout.windows.iter().any(|w| w.name() == "minivitals"
                && matches!(w, crate::config::WindowDef::MiniVitals { .. })),
            "restored window is the MiniVitals widget, not a DialogPanel"
        );
    }

    /// Bug #1: a named GUI layout saved on one character carries the full
    /// window defs; loading it into a profile that only has the default
    /// windows must recreate the missing ones (in both the layout def list
    /// and ui_state) while leaving existing windows untouched.
    #[test]
    fn materialize_missing_windows_creates_only_the_absent() {
        let mut core = core_with_layout(vec![positioned_text_def("story", 0, 0, 40, 10)]);
        core.init_windows(80, 24);
        assert!(core.ui_state.windows.contains_key("story"));

        let saved_defs = vec![
            positioned_text_def("story", 0, 0, 40, 10), // already present
            positioned_text_def("room", 40, 0, 20, 8),  // missing
            positioned_text_def("map", 60, 0, 20, 8),   // missing
        ];
        let created = core.materialize_missing_windows(&saved_defs, 80, 24);

        // Only the two absent windows are created, in order.
        assert_eq!(created, vec!["room".to_string(), "map".to_string()]);
        // Both live in ui_state AND the authoritative layout def list.
        for name in ["room", "map"] {
            assert!(core.ui_state.windows.contains_key(name), "{name} in ui_state");
            assert!(
                core.layout.windows.iter().any(|w| w.name() == name),
                "{name} in layout defs"
            );
        }
        // The pre-existing window is not duplicated.
        assert_eq!(
            core.layout.windows.iter().filter(|w| w.name() == "story").count(),
            1
        );
    }

    /// A text window subscribed to the main stream.
    fn main_text_def(name: &str) -> WindowDef {
        let mut def = positioned_text_def(name, 0, 0, 40, 10);
        if let WindowDef::Text { data, .. } = &mut def {
            data.streams = vec!["main".to_string()];
        }
        def
    }

    /// The story feed must always have a shown subscriber: hiding the
    /// last main-stream window is refused; with a second subscriber the
    /// window named "main" hides like any other.
    #[test]
    fn hide_window_gates_on_main_stream_invariant() {
        let mut core = core_with_layout(vec![main_text_def("main")]);
        core.init_windows(80, 24);

        // Sole subscriber → refused, still shown.
        core.hide_window("main");
        assert!(core.ui_state.windows.contains_key("main"));
        assert!(core.layout.get_window("main").unwrap().base().visibility.is_shown());

        // A second subscriber makes main hideable.
        let second = main_text_def("story_tab");
        core.layout.windows.push(second.clone());
        core.add_new_window(&second, 80, 24);
        core.hide_window("main");
        assert!(!core.ui_state.windows.contains_key("main"));
        assert!(!core.layout.get_window("main").unwrap().base().visibility.is_shown());

        // Now story_tab is the last subscriber → refused in turn.
        core.hide_window("story_tab");
        assert!(core.ui_state.windows.contains_key("story_tab"));
    }

    /// TUI force-show: a hidden command_input still materializes at init,
    /// and hiding it persists the layout flag without dropping the UI
    /// window. Without the flag (GUI), it hides like any other window.
    #[test]
    fn command_input_hidden_flag_vs_tui_force_show() {
        let cmd = {
            let mut base = test_window_base("command_input");
            base.visibility = crate::config::WindowVisibility::Hidden;
            WindowDef::CommandInput {
                base,
                data: crate::config::CommandInputWidgetData::default(),
            }
        };
        // GUI mode (no force): hidden stays out of ui_state.
        let mut core = core_with_layout(vec![cmd.clone()]);
        core.init_windows(80, 24);
        assert!(!core.ui_state.windows.contains_key("command_input"));

        // TUI mode: force-show materializes it despite the hidden flag.
        let mut core = core_with_layout(vec![cmd]);
        core.force_show_command_input = true;
        core.init_windows(80, 24);
        assert!(core.ui_state.windows.contains_key("command_input"));

        // Hiding under force-show flips the layout flag but keeps the UI.
        core.layout.windows[0].base_mut().visibility = crate::config::WindowVisibility::Shown;
        core.hide_window("command_input");
        assert!(!core.layout.get_window("command_input").unwrap().base().visibility.is_shown());
        assert!(core.ui_state.windows.contains_key("command_input"));
    }

    #[test]
    fn dialog_readd_disambiguates_active_effects_by_category() {
        // Buffs and Debuffs share the ActiveEffects widget type. Having a
        // Buffs window must NOT suppress auto-adding Debuffs.
        let buffs = renamed_widget("custom-buffs", "buffs");
        let mut core = core_with_layout(vec![buffs]);

        // Buffs re-send: recognized, no add.
        core.ui_state.pending_window_additions.push("buffs".to_string());
        core.process_pending_window_additions(80, 24);
        assert_eq!(
            core.layout.windows.iter().filter(|w| w.widget_type() == "active_effects").count(),
            1
        );

        // Debuffs first sight: NOT shadowed by Buffs → added.
        core.ui_state.pending_window_additions.push("debuffs".to_string());
        core.process_pending_window_additions(80, 24);
        assert_eq!(
            core.layout.windows.iter().filter(|w| w.widget_type() == "active_effects").count(),
            2,
            "debuffs was wrongly suppressed by the buffs window"
        );
    }

    #[test]
    fn container_show_hide_and_sighting_via_session_set() {
        // U3: containers are ephemeral session windows. A sighted container
        // auto-(re)opens only if the user opted it in (shown_container_titles);
        // showing/hiding by name adds/removes it. Multi-word titles work.
        let mut core = AppCore::new_for_test();
        core.layout.terminal_width = Some(80);
        core.layout.terminal_height = Some(24);
        // The registry knows the container (so it's listable), title has a space.
        core.game_state.objects.register_container(
            "268435466".to_string(),
            "My Pack".to_string(),
            Some("#268435466".to_string()),
        );

        // Sighted while not opted in → no window.
        core.message_processor.newly_registered_container =
            Some(("268435466".to_string(), "My Pack".to_string()));
        core.realize_offered_windows(80, 24);
        assert!(!core.ui_state.windows.contains_key("my_pack"));

        // Show it by (window) name → opted in + window created.
        core.set_known_window_shown("my_pack", true, 80, 24);
        assert!(core.ui_state.windows.contains_key("my_pack"));
        assert!(core.ui_state.shown_container_titles.contains("My Pack"));

        // Hide it → window closes, opt-in cleared (multi-word title works).
        core.set_known_window_shown("my_pack", false, 80, 24);
        assert!(!core.ui_state.windows.contains_key("my_pack"));
        assert!(!core.ui_state.shown_container_titles.contains("My Pack"));

        // Opt in, then a re-sight re-opens it automatically.
        core.ui_state.shown_container_titles.insert("My Pack".to_string());
        core.message_processor.newly_registered_container =
            Some(("268435466".to_string(), "My Pack".to_string()));
        core.realize_offered_windows(80, 24);
        assert!(core.ui_state.windows.contains_key("my_pack"));
    }

    #[test]
    fn discovery_burst_on_backfilled_layout_creates_zero_windows() {
        // Redesign Phase 2 gate: after the load-time binding backfill, a
        // login burst re-declaring every feed the layout already hosts
        // must create NOTHING — binding identity short-circuits before
        // adoption even runs.
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};
        let mut windows: Vec<WindowDef> = ["thoughts", "inventory", "buffs", "injuries"]
            .iter()
            .map(|name| crate::core::local_catalog::seed(name).expect(name))
            .collect();
        let mut layout = crate::config::Layout {
            windows: std::mem::take(&mut windows),
            terminal_width: Some(80),
            terminal_height: Some(24),
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
            deleted_windows: Vec::new(),
        };
        assert!(crate::config::Layout::backfill_bindings(&mut layout) > 0);
        let mut core = core_with_layout(std::mem::take(&mut layout.windows));

        let before = core.layout.windows.len();
        let bindings_before: Vec<_> = core
            .layout
            .windows
            .iter()
            .map(|w| w.base().binding.clone())
            .collect();
        for (id, kind) in [
            ("thoughts", WindowDiscoveryKind::Stream),
            ("inv", WindowDiscoveryKind::Stream),
            ("Buffs", WindowDiscoveryKind::DialogPanel),
            ("injuries", WindowDiscoveryKind::DialogPanel),
        ] {
            core.ui_state.pending_window_discoveries.push(WindowDiscovery {
                id: id.to_string(),
                title: id.to_string(),
                kind,
                save: false,
            });
        }
        core.realize_offered_windows(80, 24);

        assert_eq!(core.layout.windows.len(), before, "zero windows created");
        let bindings_after: Vec<_> = core
            .layout
            .windows
            .iter()
            .map(|w| w.base().binding.clone())
            .collect();
        assert_eq!(bindings_after, bindings_before, "bindings untouched");
    }

    #[test]
    fn registry_bindings_join_known_windows_and_conjure_bound_windows() {
        // Redesign Phase 3: discovery memory joins the Windows-list union
        // — a feed seen in a PAST session is re-addable in a fresh layout
        // before the game re-declares it.
        use crate::core::known_windows::KnownWindowKind;
        let mut core = core_with_layout(vec![]);
        core.window_registry.record("stream", "voln", "Voln");
        core.window_registry.record("dialog", "combat", "Combat");
        // Dedicated-view ids stay owned by their template rows (with the
        // template pass's game gating) — no duplicate registry row.
        core.window_registry.record("stream", "inv", "Inventory");

        let known = core.enumerate_known_windows();
        let row = |name: &str| known.iter().find(|k| k.name == name);
        let voln = row("voln").expect("registry stream row");
        assert_eq!(voln.kind, KnownWindowKind::Stream);
        assert!(!voln.shown);
        let combat = row("combat").expect("registry dialog row");
        assert_eq!(combat.kind, KnownWindowKind::Dialog);
        assert!(row("inv").is_none(), "dedicated view owned by the template row");

        // Ticking the rows conjures bound windows exactly as a live
        // discovery would, and shows them.
        core.set_known_window_shown("voln", true, 80, 24);
        let win = core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == "voln")
            .expect("conjured layout window");
        assert_eq!(
            win.base().binding,
            Some(crate::config::WindowBinding::Stream("voln".into()))
        );
        assert!(win.base().visibility.is_shown());
        assert_eq!(win.widget_type(), "text");

        core.set_known_window_shown("combat", true, 80, 24);
        let win = core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == "combat")
            .expect("conjured dialog panel");
        assert_eq!(
            win.base().binding,
            Some(crate::config::WindowBinding::Dialog("combat".into()))
        );
        assert_eq!(win.widget_type(), "dialogpanel");

        // Re-enumerating lists them as layout rows now, not registry rows.
        let known = core.enumerate_known_windows();
        assert_eq!(
            known.iter().filter(|k| k.name == "voln").count(),
            1,
            "no duplicate row after conjuring"
        );
    }

    #[test]
    fn expose_lifecycle_show_dismiss_reshow_and_user_block() {
        // Redesign Phase 4d gate: expose = show; closeDialog dismisses
        // without eating the NEXT expose; the user's Hidden is the block.
        let mut core = core_with_layout(vec![]);

        // 1. First arrival via exposeStream: registers bound and SHOWS
        //    (the expose default), unlike plain discoveries (hidden).
        core.ui_state
            .pending_exposes
            .push(("stream".to_string(), "charprofile".to_string()));
        core.realize_offered_windows(80, 24);
        let vis = core
            .layout
            .windows
            .iter()
            .find(|w| {
                w.base().binding.as_ref().is_some_and(|b| b.id() == "charprofile")
            })
            .map(|w| (w.name().to_string(), w.base().visibility))
            .expect("expose registered a bound window");
        assert!(vis.1.is_shown(), "expose default is SHOWN");
        assert!(
            core.ui_state.windows.contains_key(&vis.0),
            "and the window materialized"
        );

        // 2. The matching closeDialog dismisses the DISPLAY only: the
        //    runtime window goes, the persisted visibility does not flip
        //    to Hidden (a game dismissal is not a user block).
        core.ui_state
            .pending_expose_closes
            .push("charprofile".to_string());
        core.ui_state.expose_shown_ids.insert("charprofile".to_string());
        core.realize_offered_windows(80, 24);
        assert!(!core.ui_state.windows.contains_key(&vis.0), "dematerialized");
        let still_shown = core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == vis.0)
            .unwrap()
            .base()
            .visibility
            .is_shown();
        assert!(still_shown, "persisted visibility untouched by game close");

        // 3. Re-expose re-materializes (the walk-back-into-the-bank flow).
        core.ui_state
            .pending_exposes
            .push(("stream".to_string(), "charprofile".to_string()));
        core.realize_offered_windows(80, 24);
        assert!(core.ui_state.windows.contains_key(&vis.0), "re-shown");

        // 4. The user hides it: that IS the block — the next expose no-ops.
        core.set_known_window_shown(&vis.0, false, 80, 24);
        core.ui_state
            .pending_exposes
            .push(("stream".to_string(), "charprofile".to_string()));
        core.realize_offered_windows(80, 24);
        assert!(
            !core.ui_state.windows.contains_key(&vis.0),
            "expose blocked by the user's Hidden"
        );

        // 5. Defensive closes of never-opened ids (withdraw/deposit) no-op.
        core.ui_state
            .pending_expose_closes
            .push("withdraw".to_string());
        core.realize_offered_windows(80, 24);
    }

    #[test]
    fn declared_size_hint_shapes_new_windows_but_never_dedicated_views() {
        // Owner rule: every window respects the game's declared size at
        // creation; saved/user geometry wins afterward (creation-time-only
        // application), and dedicated views keep their curated sizes.
        let mut core = core_with_layout(vec![]);
        core.ui_state.window_hints.insert(
            "charprofile".to_string(),
            vec![
                ("location".to_string(), "force-center".to_string()),
                ("height".to_string(), "320".to_string()),
                ("width".to_string(), "400".to_string()),
            ],
        );
        core.ui_state
            .pending_exposes
            .push(("stream".to_string(), "charprofile".to_string()));
        core.realize_offered_windows(120, 60);
        let def = core
            .layout
            .windows
            .iter()
            .find(|w| {
                w.base().binding.as_ref().is_some_and(|b| b.id() == "charprofile")
            })
            .expect("expose registered");
        assert_eq!(def.base().rows.get(), 320 / 16 + 1, "declared height in cells");
        assert_eq!(def.base().cols.get(), 400 / 8 + 2, "declared width in cells");

        // A dedicated view (inventory via its claimed stream) keeps its
        // template size even when the game hints something else.
        core.ui_state.window_hints.insert(
            "inv".to_string(),
            vec![("height".to_string(), "2100".to_string())],
        );
        core.ui_state.pending_window_discoveries.push(crate::data::WindowDiscovery {
            id: "inv".to_string(),
            title: "Inventory".to_string(),
            kind: crate::data::WindowDiscoveryKind::Stream,
            save: false,
        });
        core.realize_offered_windows(120, 60);
        let inv = core
            .layout
            .windows
            .iter()
            .find(|w| w.base().binding.as_ref().is_some_and(|b| b.id() == "inv"))
            .expect("inv discovered");
        assert_eq!(inv.widget_type(), "inventory");
        let template_rows = crate::core::local_catalog::seed("inventory")
            .unwrap()
            .base()
            .rows
            .get();
        assert_eq!(inv.base().rows.get(), template_rows, "dedicated view untouched");
    }
}
