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

    /// Advance the walk executor against the latest world state. Called
    /// after every processed network line and once per frontend frame (the
    /// frame tick covers time-based waits like roundtime when the game is
    /// quiet).
    pub fn tick_travel(&mut self) {
        if !self.travel.is_traveling() {
            // Not walking: don't let feedback accumulate unboundedly.
            self.game_state.move_feedback.clear();
            return;
        }
        let Some(db) = self.map.mapdb().cloned() else {
            return;
        };
        // Drain the move-feedback queue for this tick (edge-triggered events,
        // each consumed exactly once — §09).
        let feedback: Vec<crate::core::move_feedback::MoveFeedback> =
            self.game_state.move_feedback.drain(..).collect();
        // Active spell numbers for scripted-edge checkspell branches.
        let active_spells: Vec<u16> = self
            .game_state
            .effects
            .get("ActiveSpells")
            .map(|content| {
                content
                    .effects
                    .iter()
                    .filter_map(|e| e.id.trim().parse::<u16>().ok())
                    .collect()
            })
            .unwrap_or_default();

        // Assemble the hands stow inputs into OWNED locals, so `ctx` doesn't
        // hold a borrow of self.game_state across the &mut self.travel tick.
        // Resolve the configured weaponsack/lootsack names to container
        // command-ids, gather the other tracked containers as last-resort
        // stow targets, and classify each hand's item as a weapon.
        use crate::core::game_objects::Hand;
        // gameobj_data() takes &mut self (lazy load), so resolve it before we
        // hold the immutable objects borrow.
        let gameobj_data = self.gameobj_data();
        let objects = &self.game_state.objects;
        let resolve_bag = |name: &str| -> Option<String> {
            if name.trim().is_empty() {
                return None;
            }
            objects.find_container(name).map(|c| c.command_target())
        };
        let weaponsack = resolve_bag(&self.config.go2.weaponsack);
        let lootsack = resolve_bag(&self.config.go2.lootsack);
        let reserved: std::collections::HashSet<&str> = weaponsack
            .as_deref()
            .into_iter()
            .chain(lootsack.as_deref())
            .collect();
        let other_containers: Vec<String> = objects
            .containers()
            .map(|c| c.command_target())
            .filter(|id| !reserved.contains(id.as_str()))
            .collect();
        let is_weapon = |item: Option<&crate::core::game_objects::GameItem>| -> bool {
            match item {
                Some(i) => gameobj_data.is_type(&i.name, &i.noun, "weapon"),
                None => false,
            }
        };
        // Confluence landmark scan reads ground loot + linked room scenery
        // (Lich's `GameObj.loot`): the tranquility point and pit appear as
        // room objects. Collect their nouns/names while we hold `objects`.
        let loot_nouns: Vec<String> = objects
            .ground()
            .iter()
            .chain(objects.room_desc().iter())
            .map(|item| item.name.clone())
            .collect();
        let left_hand = objects.hand(Hand::Left).cloned();
        let right_hand = objects.hand(Hand::Right).cloned();
        let left_is_weapon = is_weapon(left_hand.as_ref());
        let right_is_weapon = is_weapon(right_hand.as_ref());
        let ready_stow = objects.ready_stow().clone();
        let hands = crate::core::travel::executor::StashInputs {
            left_hand: left_hand.as_ref(),
            right_hand: right_hand.as_ref(),
            ready_stow: &ready_stow,
            weaponsack: weaponsack.as_deref(),
            lootsack: lootsack.as_deref(),
            other_containers: &other_containers,
            // Bandolier-bag resolution (Lich's find_bandolier_bag) is a
            // multi-container "swirling mist" look-scan we don't run yet; the
            // retrieval command is ported (rub #bag), but live bag lookup is a
            // follow-up. Ethereal items need no resolution (retrieved by noun).
            left_bandolier: None,
            right_bandolier: None,
            left_is_weapon,
            right_is_weapon,
        };

        // Live compass exits (XMLData.room_exits) for the Confluence explorer.
        let compass_dirs: Vec<String> = self.game_state.compass_dirs.clone();

        let ctx = crate::core::travel::TravelContext {
            db: &db,
            current_room: self.map.current_room_id,
            dead: self.game_state.status.dead,
            muckled: self.game_state.status.stunned || self.game_state.status.webbed,
            standing: self.game_state.status.standing,
            sitting: self.game_state.status.sitting,
            kneeling: self.game_state.status.kneeling,
            active_spells: &active_spells,
            rt_remaining: self.game_state.roundtime_remaining() as f64,
            now_ms: self.travel.now_ms(),
            pathcodes: &self.config.go2.pathcodes,
            hands: Some(hands),
            feedback: &feedback,
            // The fallback is a Lich-only bandaid: gated on the setting AND a
            // non-direct connection (a direct connection has no Lich to hand
            // off to). webui_available() is our "connected via Lich" proxy.
            lich_fallback: self.config.go2.lich_fallback && self.webui_available(),
            funding: Some(crate::core::travel::executor::FundingInputs {
                silver: self.game_state.silver,
                get_silvers: self.config.go2.get_silvers,
                get_return_trip: self.config.go2.get_return_trip_silvers,
            }),
            at_pinefar_depository: self
                .game_state
                .room_name
                .as_deref()
                .is_some_and(|t| t.contains("Pinefar, Depository")),
            // Confluence explorer's live view of the shifting maze: the
            // current room's compass exits and ground-loot nouns (the
            // tranquility point / pit landmarks live in ground + room_desc).
            compass_dirs: &compass_dirs,
            loot_nouns: &loot_nouns,
        };
        let events = self.travel.tick(ctx);
        for event in events {
            match event {
                crate::core::travel::TravelEvent::Status(text) => {
                    self.add_system_message(&format!("[go2] {text}"));
                }
                crate::core::travel::TravelEvent::Arrived {
                    destination,
                    seconds,
                } => {
                    self.add_system_message(&format!(
                        "[go2] arrived at room {destination} - travel time {}",
                        crate::core::travel::format_eta(seconds)
                    ));
                }
                crate::core::travel::TravelEvent::Failed(reason) => {
                    self.add_system_message(&format!("[go2] {reason}"));
                }
                crate::core::travel::TravelEvent::LichFallback { destination } => {
                    // Native travel can't cross this edge; hand off to Lich.
                    // Stop the native task and send `;go2 <dest>` — Lich walks
                    // the rest. (The event only fires on a Lich connection.)
                    self.travel.stop();
                    self.queue_timed_command(
                        std::time::Duration::ZERO,
                        format!(";go2 {destination}"),
                    );
                }
                crate::core::travel::TravelEvent::Send(_) => unreachable!("queued by the service"),
            }
        }
    }

    /// Advance the `.foreach` runner. Called from the same two places as
    /// `tick_travel` (per network line + per frontend frame).
    pub fn tick_foreach(&mut self) {
        if !self.foreach.is_running() {
            return;
        }
        let ctx = crate::core::foreach::ForeachContext {
            rt_remaining: self.game_state.roundtime_remaining() as f64,
            now_ms: self.foreach.now_ms(),
            dead: self.game_state.status.dead,
        };
        let events = self.foreach.tick(&ctx);
        for event in events {
            match event {
                crate::core::foreach::ForeachEvent::Status(text) => {
                    self.add_system_message(&format!("[foreach] {text}"));
                }
                crate::core::foreach::ForeachEvent::Done { items } => {
                    self.add_system_message(&format!(
                        "[foreach] done - {items} item{} processed.",
                        if items == 1 { "" } else { "s" }
                    ));
                }
                crate::core::foreach::ForeachEvent::Failed(reason) => {
                    self.add_system_message(&format!("[foreach] {reason}"));
                }
                crate::core::foreach::ForeachEvent::Send(_) => {
                    unreachable!("queued by the service")
                }
            }
        }
    }

    /// Commands automation wants sent to the game; frontends drain this
    /// through the same path as typed commands. Includes macro sleep
    /// segments whose pause has elapsed.
    pub fn take_outbound(&mut self) -> Vec<String> {
        let mut commands = self.travel.take_outbound();
        commands.extend(self.foreach.take_outbound());
        let now = std::time::Instant::now();
        let mut i = 0;
        while i < self.timed_commands.len() {
            if self.timed_commands[i].0 <= now {
                commands.push(self.timed_commands.remove(i).1);
            } else {
                i += 1;
            }
        }
        commands
    }

    /// Queue a command to go out after a pause (macro sleep segments).
    pub fn queue_timed_command(&mut self, delay: std::time::Duration, command: String) {
        self.timed_commands
            .push((std::time::Instant::now() + delay, command));
    }

    /// Plan and begin a trip to a mapdb room id.
    pub fn start_travel(&mut self, destination: u32) {
        // Lease gate: a different automation root (e.g. a running foreach)
        // must be stopped first; a go2-owned chain retargets as always.
        if let Some(owner) = self.automation_blocked_by("go2") {
            self.add_system_message(&format!(
                "[go2] {} is driving - .stop to cancel it first.",
                owner.desc
            ));
            return;
        }
        // Sync the gated-travel routing flags before planning (Lich's
        // $go2_use_seeking / UserVars.mapdb_use_portmasters globals).
        // Seeking only takes effect for a Voln Master, so its toggle is gated
        // on can_seek(); portmasters are open to anyone with the silver.
        crate::core::pathing::transpile::set_use_seeking(
            self.config.go2.use_seeking,
            self.game_state.character.can_seek(),
        );
        crate::core::pathing::transpile::set_use_portmasters(self.config.go2.use_portmasters);
        // Urchins: valid only when enabled AND access hasn't expired AND not
        // hidden/invisible (Lich's combined urchin timeto gate). Also lets
        // dijkstra route through the urchin-hideout hubs this trip.
        let now_epoch = chrono::Utc::now().timestamp();
        crate::core::pathing::transpile::set_urchins_valid(
            self.config.go2.use_urchins
                && self.game_state.character.urchins_valid(
                    now_epoch,
                    self.game_state.status.hidden,
                    self.game_state.status.invisible,
                ),
        );
        let Some(db) = self.map.mapdb().cloned() else {
            self.add_system_message(
                "[go2] map database not loaded - configure it in Settings > Map",
            );
            return;
        };
        let Some(current) = self.map.current_room_id else {
            self.add_system_message(
                "[go2] your current room hasn't resolved against the mapdb yet (see .room)",
            );
            return;
        };
        if current == destination {
            self.add_system_message("[go2] you're already here...");
            return;
        }
        if db.room(destination).is_none() {
            self.add_system_message(&format!("[go2] room {destination} is not in the mapdb"));
            return;
        }
        match crate::core::travel::TravelTask::start(
            &db,
            current,
            destination,
            self.travel.now_ms(),
        ) {
            Ok(task) => {
                let eta = task.eta_seconds(&db, current);
                let title = db
                    .room(destination)
                    .and_then(|r| r.title.first().cloned())
                    .unwrap_or_default();
                self.add_system_message(&format!(
                    "[go2] -> {title} ({destination}): {} rooms, ETA {}",
                    task.rooms_total(),
                    crate::core::travel::format_eta(eta)
                ));
                self.travel.last_start_room = Some(current);
                self.travel.set_task(task);
                // Fire the first move now instead of on the next frame.
                self.tick_travel();
            }
            Err(reason) => {
                self.add_system_message(&format!("[go2] {reason}"));
            }
        }
    }

    /// Cancel the active trip (`.go2 stop`, Esc).
    pub fn stop_travel(&mut self) {
        if self.travel.stop() {
            self.add_system_message("[go2] travel stopped.");
        } else {
            self.add_system_message("[go2] not traveling.");
        }
    }

    /// Check the given GitHub repo for a mapdb release and download it if
    /// it's new. Progress lands in `map_updater.status`.
    pub fn start_mapdb_download(&mut self, repo: &str) {
        let repo = repo.trim();
        if repo.is_empty() {
            return;
        }
        self.map_updater.start(repo.to_owned());
    }

    /// Delete all downloaded mapdb versions and fall back to the Lich folder.
    pub fn remove_downloaded_mapdb(&mut self) {
        self.map_updater.remove_downloaded();
        self.refresh_map_source();
    }

    /// Push the latest stream-reported room identifiers into the map service.
    /// `nav_room_id` carries the game uid; `lich_room_id` the Lich room id.
    /// Title and obvious exits ride along — unmapped rooms are sketched as
    /// ghosts from exactly this data.
    fn sync_map_room(&mut self) {
        let uid = self
            .nav_room_id
            .as_deref()
            .and_then(|s| s.trim().parse::<i64>().ok());
        let lich_id = self
            .lich_room_id
            .as_deref()
            .and_then(|s| s.trim().parse::<u32>().ok());
        // Plain-text "room desc" for the uid-less content fallback; lines
        // are joined with a space to mirror the single-string mapdb form.
        let description = self
            .room_components
            .get("room desc")
            .map(|lines| {
                lines
                    .iter()
                    .map(|segments| {
                        segments
                            .iter()
                            .map(|seg| seg.text.as_str())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let snapshot = crate::core::ghost_rooms::RoomSnapshot {
            title: self.game_state.room_name.clone(),
            exits: self.game_state.exits.clone(),
            description,
        };
        self.map.note_room(uid, lich_id, snapshot);
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

    /// Build runtime keybind map from config for fast O(1) lookups
    /// Converts string-based keybinds (e.g., "num_0", "Ctrl+s") to KeyEvent structs

    /// Rebuild the keybind map (call after config changes)

    // ===========================================================================================
    // Window Scrolling Methods
    // ===========================================================================================

    /// Scroll the currently focused window up by one line
    pub fn scroll_current_window_up_one(&mut self) {
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    content.scroll_up(1);
                    self.needs_render = true;
                }
            }
        }
    }

    /// Scroll the currently focused window down by one line
    pub fn scroll_current_window_down_one(&mut self) {
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    content.scroll_down(1);
                    self.needs_render = true;
                }
            }
        }
    }

    /// Scroll the currently focused window up by one page
    pub fn scroll_current_window_up_page(&mut self) {
        tracing::debug!("scroll_current_window_up_page called, focused_window={:?}", self.ui_state.focused_window);
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                tracing::debug!("Found window '{}', widget_type={:?}", window_name, window.widget_type);
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    // Use a reasonable page size (20 lines)
                    let old_offset = content.scroll_offset;
                    content.scroll_up(20);
                    tracing::info!("Scrolled '{}' up: {} -> {}", window_name, old_offset, content.scroll_offset);
                    self.needs_render = true;
                } else {
                    tracing::debug!("Window '{}' content is not Text type", window_name);
                }
            } else {
                tracing::warn!("Focused window '{}' not found in windows map", window_name);
            }
        } else {
            tracing::warn!("No focused window set for scrolling");
        }
    }

    /// Scroll the currently focused window down by one page
    pub fn scroll_current_window_down_page(&mut self) {
        tracing::debug!("scroll_current_window_down_page called, focused_window={:?}", self.ui_state.focused_window);
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                tracing::debug!("Found window '{}', widget_type={:?}", window_name, window.widget_type);
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    // Use a reasonable page size (20 lines)
                    let old_offset = content.scroll_offset;
                    content.scroll_down(20);
                    tracing::info!("Scrolled '{}' down: {} -> {}", window_name, old_offset, content.scroll_offset);
                    self.needs_render = true;
                } else {
                    tracing::debug!("Window '{}' content is not Text type", window_name);
                }
            } else {
                tracing::warn!("Focused window '{}' not found in windows map", window_name);
            }
        } else {
            tracing::warn!("No focused window set for scrolling");
        }
    }

    /// Scroll the currently focused window to the top (oldest content)
    pub fn scroll_current_window_home(&mut self) {
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    content.scroll_to_top();
                    self.needs_render = true;
                }
            }
        }
    }

    /// Scroll the currently focused window to the bottom (newest content)
    pub fn scroll_current_window_end(&mut self) {
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    content.scroll_to_bottom();
                    self.needs_render = true;
                }
            }
        }
    }

    /// Cycle to the next scrollable text window
    /// Uses focus configuration (types + optional order) to choose focusable windows.
    pub fn cycle_focused_window(&mut self) {
        let focus_order = self.build_focus_order();
        if focus_order.is_empty() {
            return;
        }

        let current_idx = self
            .ui_state
            .focused_window
            .as_ref()
            .and_then(|name| focus_order.iter().position(|n| n == name))
            .unwrap_or(usize::MAX);

        let next_idx = if current_idx == usize::MAX {
            0
        } else {
            (current_idx + 1) % focus_order.len()
        };
        let next_name = focus_order[next_idx].clone();

        self.ui_state.set_focus(Some(next_name.clone()));
        self.add_system_message(&format!("Focused window: {}", next_name));
        self.needs_render = true;
        tracing::debug!("Cycled focused window to '{}'", next_name);
    }

    /// Cycle focus backwards through the focus order.
    pub fn cycle_focused_window_reverse(&mut self) {
        let focus_order = self.build_focus_order();
        if focus_order.is_empty() {
            return;
        }

        let current_idx = self
            .ui_state
            .focused_window
            .as_ref()
            .and_then(|name| focus_order.iter().position(|n| n == name))
            .unwrap_or(0);

        let prev_idx = if current_idx == 0 {
            focus_order.len() - 1
        } else {
            current_idx - 1
        };
        let prev_name = focus_order[prev_idx].clone();

        self.ui_state.set_focus(Some(prev_name.clone()));
        self.needs_render = true;
        tracing::debug!("Cycled focused window to '{}' (reverse)", prev_name);
    }

    fn build_focus_order(&self) -> Vec<String> {
        let focus_config = &self.config.ui.focus;
        let mut focusable = std::collections::HashSet::new();
        if !focus_config.types.is_empty() {
            for entry in &focus_config.types {
                focusable.insert(entry.trim().to_lowercase());
            }
        }
        let mut excluded = std::collections::HashSet::new();
        for entry in &focus_config.exclude {
            let trimmed = entry.trim();
            if !trimmed.is_empty() {
                excluded.insert(trimmed.to_lowercase());
            }
        }

        let mut names = Vec::new();

        if !focus_config.order.is_empty() {
            for name in &focus_config.order {
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if excluded.contains(&trimmed.to_lowercase()) {
                    continue;
                }
                if let Some(window) = self.ui_state.windows.get(trimmed) {
                    if !window.visible {
                        continue;
                    }
                    if Self::is_focusable_widget(&window.widget_type, &focusable) {
                        names.push(trimmed.to_string());
                    }
                }
            }
        } else {
            for window_def in &self.layout.windows {
                if !window_def.base().visibility.is_shown() {
                    continue;
                }
                let name = window_def.name();
                if excluded.contains(&name.to_lowercase()) {
                    continue;
                }
                if let Some(window) = self.ui_state.windows.get(name) {
                    if Self::is_focusable_widget(&window.widget_type, &focusable) {
                        names.push(name.to_string());
                    }
                }
            }
        }

        for (name, window) in &self.ui_state.windows {
            if !window.visible {
                continue;
            }
            if excluded.contains(&name.to_lowercase()) {
                continue;
            }
            if names.contains(name) {
                continue;
            }
            if Self::is_focusable_widget(&window.widget_type, &focusable) {
                names.push(name.clone());
            }
        }

        names
    }

    fn is_focusable_widget(
        widget_type: &crate::data::WidgetType,
        focusable: &std::collections::HashSet<String>,
    ) -> bool {
        if focusable.is_empty() {
            return !matches!(widget_type, crate::data::WidgetType::CommandInput);
        }
        let kind = match widget_type {
            crate::data::WidgetType::Text => "text",
            crate::data::WidgetType::TabbedText => "tabbedtext",
            crate::data::WidgetType::Progress => "progress",
            crate::data::WidgetType::Countdown => "countdown",
            crate::data::WidgetType::Compass => "compass",
            crate::data::WidgetType::Map => "map",
            crate::data::WidgetType::Indicator => "indicator",
            crate::data::WidgetType::Room => "room",
            crate::data::WidgetType::Inventory => "inventory",
            crate::data::WidgetType::Reserve => "reserve",
            crate::data::WidgetType::CommandInput => "command_input",
            crate::data::WidgetType::Dashboard => "dashboard",
            crate::data::WidgetType::InjuryDoll => "injury_doll",
            crate::data::WidgetType::Hand => "hand",
            crate::data::WidgetType::ActiveEffects => "active_effects",
            crate::data::WidgetType::Targets => "targets",
            crate::data::WidgetType::Players => "players",
            crate::data::WidgetType::Spells => "spells",
            crate::data::WidgetType::Spacer => "spacer",
            crate::data::WidgetType::Performance => "performance",
            crate::data::WidgetType::Perception => "perception",
            crate::data::WidgetType::Container => "container",
            crate::data::WidgetType::Experience => "experience",
            crate::data::WidgetType::GS4Experience => "gs4_experience",
            crate::data::WidgetType::Encumbrance => "encum",
            crate::data::WidgetType::Quickbar => "quickbar",
            crate::data::WidgetType::Hotkeybar => "hotkeybar",
            crate::data::WidgetType::MiniVitals => "minivitals",
            crate::data::WidgetType::Betrayer => "betrayer",
            crate::data::WidgetType::Items => "items",
            crate::data::WidgetType::WebUi => "webui",
            crate::data::WidgetType::DialogPanel => "dialogpanel",
        };
        focusable.contains(kind)
    }

    // ===========================================================================================
    // Keybind Action Execution
    // ===========================================================================================

    /// Execute a keybind action (called when a bound key is pressed)
    /// Returns a list of commands to send to the server (for macros)

    /// Execute a KeyAction (dispatch to the appropriate method)

    /// Attach the remote client sink (web frontend sidecar).
    /// Called by the runtime after it spawns the web server task.
    pub fn enable_remote(&mut self, mut sink: crate::core::remote::RemoteSink) {
        sink.set_macros(&self.config.macros);
        sink.set_wheels(&self.config);
        self.message_processor.remote = Some(sink);
    }

    /// Re-publish radial-wheel definitions to remote clients after the
    /// wheel config changed (keybinds reload, desktop wheel editor).
    /// No-op when web is disabled.
    pub fn push_remote_wheels(&mut self) {
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_wheels(&self.config);
        }
    }

    /// Surface per-wheel `button` conflicts against the `[controller]`
    /// table (the runtime binding authority). Two classes are reported:
    ///   - a wheel's `button` disagrees with the button actually bound to
    ///     its `controller_wheel[:name]` action — `[controller]` wins, so
    ///     the note tells the user which button really opens the wheel;
    ///   - two wheels claim the same `button` — only one can win.
    /// Called after controller config (re)loads. Silent when clean.
    pub fn warn_wheel_binding_conflicts(&mut self) {
        use crate::config::KeyBindAction;
        // button -> wheel key ("" = default) from [controller].
        let mut bound: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (button, action) in &self.config.controller_binds {
            if let KeyBindAction::Action(name) = action {
                let key = if name == "controller_wheel" {
                    Some(String::new())
                } else {
                    name.strip_prefix("controller_wheel:").map(str::to_string)
                };
                if let Some(key) = key {
                    bound.insert(button.clone(), key);
                }
            }
        }

        let mut warnings: Vec<String> = Vec::new();
        // Meta button vs [controller] authority.
        let mut claimed: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (name, meta) in &self.config.controller_wheels_meta {
            let Some(button) = meta.button.as_deref() else {
                continue;
            };
            // Wheel key as [controller] would encode it ("default" wheel
            // binds the bare controller_wheel action = key "").
            let wheel_key = if name == "default" { "" } else { name.as_str() };
            match bound.get(button) {
                Some(k) if k == wheel_key => {} // agrees
                Some(other) => {
                    let other_label = if other.is_empty() { "default" } else { other };
                    warnings.push(format!(
                        "Wheel '{}' lists button '{}', but [controller] binds '{}' to the '{}' wheel — [controller] wins.",
                        name, button, button, other_label
                    ));
                }
                None => warnings.push(format!(
                    "Wheel '{}' lists button '{}', but nothing in [controller] opens it — bind '{}' to 'controller_wheel:{}'.",
                    name, button, button, name
                )),
            }
            if let Some(prev) = claimed.insert(button.to_string(), name.clone()) {
                warnings.push(format!(
                    "Wheels '{}' and '{}' both claim button '{}' — only one can open on it.",
                    prev, name, button
                ));
            }
        }

        for w in warnings {
            self.add_system_message(&w);
        }
    }

    /// Surface `span` problems across every configured wheel (default +
    /// named, recursing into folders): spans that sum over 360°, resolve
    /// below the minimum width, or leave a ring unable to close. Advisory
    /// only — the runtime resolver always produces a usable ring by
    /// clamping and scaling; these tell the user their numbers were
    /// adjusted. Called alongside `warn_wheel_binding_conflicts` on load
    /// and editor save. The dynamic portals wheel carries no spans, so it
    /// is skipped.
    pub fn warn_wheel_span_conflicts(&mut self) {
        use crate::config::validate_wheel_spans;
        let mut issues = validate_wheel_spans("default", &self.config.controller_wheel);
        for (name, slices) in &self.config.controller_wheels {
            if name == Self::PORTAL_WHEEL_KEY {
                continue;
            }
            issues.extend(validate_wheel_spans(name, slices));
        }
        for issue in issues {
            self.add_system_message(&issue.message());
        }
    }

    /// Reserved dynamic wheel name: slices are built from the current
    /// room's portal list at open time instead of TOML.
    pub const PORTAL_WHEEL_KEY: &str = "portals";

    /// Slices for a wheel key: the dynamic portals wheel first
    /// (shadowing any static wheel of that name), else the static
    /// config lookup. Owned — dynamic slices have no home in config.
    pub fn wheel_slices(&self, key: &str, path: &[usize]) -> Option<Vec<crate::config::WheelSlice>> {
        if key == Self::PORTAL_WHEEL_KEY {
            if !path.is_empty() {
                return None; // flat wheel: portals have no folders
            }
            let slices: Vec<crate::config::WheelSlice> = self
                .portal_candidate_list()
                .into_iter()
                .map(|c| crate::config::WheelSlice {
                    // The wedge shows the movement label (verb-stripped for a
                    // plain "go gate" -> "gate"); a StringProc edge's label is
                    // already the movement (e.g. "climb footpath"). The pick
                    // runs c.command (a .go2 <id> for proc edges).
                    label: c
                        .label
                        .split_once(' ')
                        .map(|(_, rest)| rest.to_string())
                        .unwrap_or_else(|| c.label.clone()),
                    command: c.command,
                    // Dynamic slices carry no span/inner/color: the portals
                    // ring stays evenly spaced with the global dead zone.
                    ..Default::default()
                })
                .collect();
            return (!slices.is_empty()).then_some(slices);
        }
        self.config.wheel_level_slices(key, path).cloned()
    }

    /// Resolve a wheel pick (remote clients): the dynamic portals wheel by
    /// index, else static config. `<target_id>`/`<target_noun>` resolve
    /// against the host's interact focus so a phone combat wheel casts at
    /// the selected creature; a placeholder with nothing focused yields
    /// None (the pick is dropped, never sent literally) — mirroring the
    /// GUI wheel and bound interact macros. The GUI's own release-fire
    /// substitutes in wheel_fire, so it doesn't route through here.
    pub fn wheel_pick_command(&self, key: &str, path: &[usize]) -> Option<String> {
        let raw = if key == Self::PORTAL_WHEEL_KEY {
            let (&leaf, folders) = path.split_last()?;
            if !folders.is_empty() {
                return None;
            }
            self.portal_commands().into_iter().nth(leaf)?
        } else {
            self.config.wheel_pick_command(key, path)?
        };
        self.substitute_interact_placeholders(raw)
    }

    /// Declare that this runtime accepts session control (Connect /
    /// Disconnect) from web clients. Only the headless runtime does.
    pub fn set_remote_session_control(&mut self, enabled: bool) {
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_session_control(enabled);
        }
    }

    /// Push a session status change to remote clients (headless supervisor
    /// state transitions). No-op when web is disabled.
    pub fn set_remote_session_state(&mut self, info: crate::core::remote::RemoteSessionInfo) {
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_session_state(info);
        }
    }

    /// Broadcast a highlight-triggered sound to remote clients, which play
    /// it via the browser (used by the headless runtime where there is no
    /// native audio device). No-op when web is disabled.
    pub fn push_remote_sound(&mut self, file: &str, volume: Option<f32>) {
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_sound(file, volume);
        }
    }

    /// Create or edit a phone-authored macro button. The edit lands in the
    /// macros-local.toml overlay (the hand-written macros.toml is never
    /// rewritten), then the merged set is re-published to every client.
    pub fn apply_macro_save(
        &mut self,
        group: Option<String>,
        button: crate::config::MacroButton,
        original: Option<(Option<String>, String)>,
    ) {
        let has_command = !button.command.as_deref().unwrap_or("").trim().is_empty();
        if button.label.trim().is_empty() || (!has_command && button.options.is_empty()) {
            self.add_system_message(
                "Macro not saved: a label plus a command (or menu options) are required",
            );
            return;
        }
        let label = button.label.clone();
        self.config.macros_local.upsert_button(
            group.as_deref(),
            button,
            original
                .as_ref()
                .map(|(group, label)| (group.as_deref(), label.as_str())),
        );
        self.persist_and_push_macros(&format!("Saved macro '{}'", label));
    }

    /// Delete a phone-authored macro button. Buttons from the hand-written
    /// macros.toml are not deletable remotely.
    pub fn apply_macro_delete(&mut self, group: Option<String>, label: String) {
        if self.config.macros_local.delete_button(group.as_deref(), &label) {
            self.persist_and_push_macros(&format!("Deleted macro '{}'", label));
        } else {
            self.add_system_message(&format!(
                "Macro '{}' is defined in macros.toml and can only be edited there",
                label
            ));
        }
    }

    fn persist_and_push_macros(&mut self, message: &str) {
        if let Err(e) = self
            .config
            .macros_local
            .save_local(self.config.character.as_deref())
        {
            self.add_system_message(&format!("Failed to save macros-local.toml: {e:#}"));
            return;
        }
        let base = crate::config::MacrosConfig::load_base(self.config.character.as_deref())
            .unwrap_or_default();
        self.config.macros =
            crate::config::MacrosConfig::merge(base, self.config.macros_local.clone());
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_macros(&self.config.macros);
        }
        self.add_system_message(message);
    }

    /// Flush coalesced game-state deltas to remote clients. Called once
    /// per message batch by the frontend loop; no-op when web is disabled.
    pub fn flush_remote_state(&mut self) {
        if self.message_processor.remote.is_none() {
            return;
        }
        let mut snap = crate::core::remote::RemoteStateSnapshot::from_game_state(
            &self.game_state,
            &self.config.target_list.excluded_nouns,
        );
        // Room number lives on AppCore (nav tag in direct mode; extracted
        // from the room name under Lich), not GameState.
        if snap.room_id.is_none() {
            snap.room_id = self
                .nav_room_id
                .clone()
                .or_else(|| self.lich_room_id.clone());
        }
        // Portal resolution needs the map service, which lives here.
        snap.portals = self.portal_commands();
        // Real sessions rarely set game_state.room_name/exits; fall back
        // the same way the room widget does (see gui sync_room_windows):
        // subtitle from <streamWindow> for the name, compass for exits.
        if snap.room_name.as_deref().is_none_or(|n| n.trim().is_empty()) {
            snap.room_name = self.room_subtitle.as_ref().map(|subtitle| {
                subtitle
                    .trim()
                    .trim_start_matches('-')
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string()
            });
        }
        if snap.exits.is_empty() {
            snap.exits = self.game_state.compass_dirs.clone();
        }
        if snap.character.is_none() {
            // connection.character comes from config.toml; config.character
            // is the CLI --character/--profile name.
            snap.character = self
                .config
                .connection
                .character
                .clone()
                .or_else(|| self.config.character.clone());
        }
        // The map lives on AppCore, not GameState: overlay the drawable
        // scene + position for the phone's map view.
        let (map_scene, map_state) = self.build_remote_map();
        snap.map_scene = map_scene;
        snap.map_state = map_state;
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.flush_state(snap);
        }
    }

    /// The phone map's wire data: the same sheet + building filter the
    /// desktop mini map draws, cached until the drawn view changes, plus
    /// the small per-step position/ghost state.
    fn build_remote_map(
        &mut self,
    ) -> (
        crate::core::remote::RemoteMapSceneRef,
        crate::core::remote::RemoteMapState,
    ) {
        use crate::core::layout_engine::Sheet;
        use crate::core::remote::{
            RemoteGhostEdge, RemoteGhostNode, RemoteMapSceneRef, RemoteMapState,
        };

        let map = &self.map;
        let mut state = RemoteMapState::default();
        let Some(scene) = map.current_scene() else {
            self.remote_map_cache = None;
            return (RemoteMapSceneRef::default(), state);
        };

        let current = map.current_room_id;
        let (sheet, center, filter) = match current.and_then(|id| scene.room(id)) {
            Some((sheet, room)) => (
                sheet,
                Some(room.cell),
                (sheet == Sheet::Interiors).then(|| scene.cluster_groups(room.group)),
            ),
            None => (Sheet::Outdoor, None, None),
        };

        // Ghost sketch overlay (session-only unmapped interiors); rendered
        // only in cartography mode — everyday play shows mapdb truth.
        let overlay = (self.config.map.mapping_mode && !map.ghosts().is_empty()).then(|| {
            crate::core::ghost_rooms::build_overlay(map.ghosts(), scene, sheet, filter.as_ref())
        });
        let ghost_cell = map
            .current_ghost
            .and_then(|uid| overlay.as_ref()?.cell_of(uid));

        state.available = true;
        state.location = map.current_location.clone();
        state.room = current;
        state.cell = ghost_cell.or(center).map(|c| [c.x, c.y]);
        state.in_ghost = ghost_cell.is_some();
        state.travel = self.travel.task().and_then(|task| {
            let db = map.mapdb()?;
            let from = current?;
            Some(crate::core::remote::RemoteTravelStatus {
                dest: task.destination,
                done: task.rooms_total().saturating_sub(task.rooms_remaining()),
                total: task.rooms_total(),
                eta: crate::core::travel::format_eta(task.eta_seconds(db, from)),
            })
        });
        if let Some(overlay) = &overlay {
            let current_ghost = map.current_ghost;
            state.ghosts = overlay
                .nodes
                .iter()
                .map(|n| RemoteGhostNode {
                    x: n.cell.x,
                    y: n.cell.y,
                    cur: current_ghost == Some(n.uid),
                })
                .collect();
            state.ghost_edges = overlay
                .edges
                .iter()
                .map(|e| RemoteGhostEdge {
                    x1: e.a.x,
                    y1: e.a.y,
                    x2: e.b.x,
                    y2: e.b.y,
                    l: e.label.clone(),
                })
                .collect();
        }

        // Scene: rebuild only when the drawn view changes (location/sheet/
        // building or a layout regeneration — the Arc pointer covers all).
        let cluster_key = filter
            .as_ref()
            .map(|set| set.iter().min().copied().unwrap_or(0));
        let key = (std::sync::Arc::as_ptr(scene) as usize, sheet, cluster_key);
        if let Some((cached_key, cached)) = &self.remote_map_cache {
            if *cached_key == key {
                return (RemoteMapSceneRef(Some(cached.clone())), state);
            }
        }

        let wire = std::sync::Arc::new(wire_map_scene(scene, sheet, filter.as_ref()));
        self.remote_map_cache = Some((key, wire.clone()));
        (RemoteMapSceneRef(Some(wire)), state)
    }

    /// Location list for a phone's map picker.
    pub fn handle_remote_map_locations(&mut self, client_id: u64, request_id: u64) {
        let locations: Vec<String> = self
            .map
            .mapdb()
            .map(|db| db.locations().map(str::to_owned).collect())
            .unwrap_or_default();
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_map_locations(client_id, request_id, locations);
        }
    }

    /// A phone wants to browse another location's map. Layout generation is
    /// async: reply now when the scene is cached, otherwise queue and let
    /// `poll_map` answer when the worker finishes.
    pub fn handle_remote_map_view(&mut self, client_id: u64, request_id: u64, location: String) {
        let known = self
            .map
            .mapdb()
            .map(|db| db.rooms(&location).is_some())
            .unwrap_or(false);
        if !known {
            if let Some(remote) = self.message_processor.remote.as_mut() {
                remote.push_map_browse(
                    client_id,
                    request_id,
                    location.clone(),
                    None,
                    Some(format!("'{location}' is not in the map database")),
                );
            }
            return;
        }
        self.map.request_location(&location);
        self.pending_map_views.push((client_id, request_id, location));
        self.service_pending_map_views();
    }

    /// Answer browse requests whose layouts have finished generating.
    fn service_pending_map_views(&mut self) {
        if self.pending_map_views.is_empty() {
            return;
        }
        let mut still_pending = Vec::new();
        for (client_id, request_id, location) in std::mem::take(&mut self.pending_map_views) {
            let Some(scene) = self.map.scene_for(&location) else {
                still_pending.push((client_id, request_id, location));
                continue;
            };
            // Browse the outdoor sheet; interior-only locations fall back
            // to their interiors shelf.
            let sheet = if scene.outdoor.rooms.is_empty() {
                crate::core::layout_engine::Sheet::Interiors
            } else {
                crate::core::layout_engine::Sheet::Outdoor
            };
            let wire = std::sync::Arc::new(wire_map_scene(scene, sheet, None));
            if let Some(remote) = self.message_processor.remote.as_mut() {
                remote.push_map_browse(client_id, request_id, location, Some(wire), None);
            }
        }
        self.pending_map_views = still_pending;
    }

    /// Poll TTS events from callback channel and handle them
    /// Should be called in the main event loop to enable auto-play
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

    /// Initialize windows based on current layout
    pub fn init_windows(&mut self, terminal_width: u16, terminal_height: u16) {
        // Preserve command history from existing command_input window
        let preserved_history: Option<Vec<String>> = self
            .ui_state
            .windows
            .get("command_input")
            .and_then(|w| {
                if let WindowContent::CommandInput { history, .. } = &w.content {
                    Some(history.clone())
                } else {
                    None
                }
            });

        // Calculate window positions from layout
        let positions = self.calculate_window_positions(terminal_width, terminal_height);

        // Log all widget types being loaded for debugging
        let widget_types: Vec<_> = self.layout.windows.iter()
            .map(|w| format!("{}:{}", w.name(), w.widget_type()))
            .collect();
        tracing::info!("init_windows: Loading {} windows: {:?}", widget_types.len(), widget_types);

        // Create windows based on layout (only visible ones)
        for window_def in &self.layout.windows {
            // Skip hidden windows (except command_input under the TUI
            // force-show rule — the TUI has no fallback input bar).
            if !window_def.base().visibility.is_shown() {
                let force = self.force_show_command_input
                    && window_def.widget_type() == "command_input";
                if !force {
                    tracing::debug!("Skipping hidden window '{}' during init", window_def.name());
                    continue;
                }
            }

            let position = positions
                .get(window_def.name())
                .cloned()
                .unwrap_or(WindowPosition {
                    x: crate::data::geometry::Col::new(0),
                    y: crate::data::geometry::Row::new(0),
                    width: crate::data::geometry::Width::new(80),
                    height: crate::data::geometry::Height::new(24),
                });

            let widget_type = WidgetType::from_str(window_def.widget_type());

            let title = window_def
                .base()
                .title
                .as_deref()
                .unwrap_or(window_def.name());

            let content = match widget_type {
                WidgetType::Text => {
                    let (buffer_size, streams, compact, show_ts, ts_pos) =
                        if let crate::config::WindowDef::Text { data, .. } = window_def {
                            (
                                data.buffer_size,
                                data.streams.clone(),
                                data.compact,
                                data.show_timestamps,
                                data.timestamp_position
                                    .unwrap_or(self.config.ui.timestamp_position),
                            )
                        } else {
                            (1000, vec![], false, false, self.config.ui.timestamp_position)
                        };
                    let mut text_content = TextContent::new(title, buffer_size);
                    text_content.streams = streams.clone();
                    text_content.compact = compact;
                    text_content.show_timestamps = show_ts;
                    text_content.timestamp_position = ts_pos;

                    // Pre-populate bounty window with cached data on reload
                    if window_def.name().eq_ignore_ascii_case("bounty") && self.game_state.bounty.has_data() {
                        let lines = if compact {
                            &self.game_state.bounty.compact_lines
                        } else {
                            std::slice::from_ref(&self.game_state.bounty.raw_text)
                        };
                        for line_text in lines {
                            text_content.add_line(crate::data::widget::StyledLine::from_text_with_stream(
                                line_text.clone(),
                                "bounty",
                            ));
                        }
                        tracing::info!("Pre-populated bounty window with {} cached lines", lines.len());
                    }

                    // Pre-populate society window with cached data on reload
                    if streams.iter().any(|s| s.eq_ignore_ascii_case("society")) && self.game_state.society.has_data() {
                        for line_text in &self.game_state.society.lines {
                            text_content.add_line(crate::data::widget::StyledLine::from_text_with_stream(
                                line_text.clone(),
                                "society",
                            ));
                        }
                        tracing::info!("Pre-populated society window with {} cached lines", self.game_state.society.lines.len());
                    }

                    WindowContent::Text(text_content)
                }
                WidgetType::TabbedText => {
                    // Extract tab definitions and buffer size from window def
                    if let crate::config::WindowDef::TabbedText { data, .. } = window_def {
                        let global_ts_pos = self.config.ui.timestamp_position;
                        let tabs: Vec<(String, Vec<String>, bool, bool, crate::config::TimestampPosition)> = data
                            .tabs
                            .iter()
                            .map(|tab| {
                                // show_timestamps defaults to false if not explicitly set per-tab
                                let show_ts = tab.show_timestamps.unwrap_or(false);
                                let ignore = tab.ignore_activity.unwrap_or(false);
                                let ts_pos = tab.timestamp_position.unwrap_or(global_ts_pos);
                                (tab.name.clone(), tab.get_streams(), show_ts, ignore, ts_pos)
                            })
                            .collect();
                        WindowContent::TabbedText(crate::data::TabbedTextContent::new(
                            tabs,
                            data.buffer_size,
                        ))
                    } else {
                        // Fallback, though this path should ideally not be taken if config is valid
                        WindowContent::TabbedText(crate::data::TabbedTextContent::new(
                            vec![(
                                "Default".to_string(),
                                vec!["main".to_string()],
                                false, // show_timestamps defaults to false
                                false,
                                crate::config::TimestampPosition::End,
                            )],
                            1000,
                        ))
                    }
                }
                WidgetType::CommandInput => WindowContent::CommandInput {
                    text: String::new(),
                    cursor: 0,
                    history: Vec::new(),
                    history_index: None,
                },
                WidgetType::Progress => {
                    let (label, progress_id, color, numbers_only, current_only) =
                        if let crate::config::WindowDef::Progress { data, .. } = window_def {
                            (
                                data.label.clone().unwrap_or_else(|| title.to_string()),
                                data.id
                                    .clone()
                                    .unwrap_or_else(|| window_def.name().to_string()),
                                data.color.clone(),
                                data.numbers_only,
                                data.current_only,
                            )
                        } else {
                            (
                                title.to_string(),
                                window_def.name().to_string(),
                                None,
                                false,
                                false,
                            )
                        };
                    WindowContent::Progress(ProgressData {
                        value: 100,
                        max: 100,
                        label,
                        color,
                        progress_id,
                        numbers_only,
                        current_only,
                    })
                }
                WidgetType::Countdown => {
                    let (label, countdown_id, color, show_when_zero) =
                        if let crate::config::WindowDef::Countdown { data, .. } = window_def {
                            (
                                data.label
                                    .clone()
                                    .unwrap_or_else(|| title.to_string()),
                                data.id
                                    .clone()
                                    .unwrap_or_else(|| window_def.name().to_string()),
                                data.color.clone(),
                                data.show_when_zero.unwrap_or(false),
                            )
                        } else {
                            (title.to_string(), window_def.name().to_string(), None, false)
                        };

                    WindowContent::Countdown(CountdownData {
                        end_time: 0,
                        label,
                        countdown_id,
                        color,
                        show_when_zero,
                    })
                }
                WidgetType::Map => WindowContent::Map(crate::data::MapData::default()),
            WidgetType::Compass => WindowContent::Compass(CompassData {
                    directions: Vec::new(),
                }),
                WidgetType::InjuryDoll => WindowContent::InjuryDoll(InjuryDollData::new()),
                WidgetType::Indicator => {
                    let (indicator_id, active_color) =
                        if let crate::config::WindowDef::Indicator { data, .. } = window_def {
                            (
                                data.indicator_id
                                    .clone()
                                    .unwrap_or_else(|| window_def.name().to_string()),
                                data.active_color.clone(),
                            )
                        } else {
                            (window_def.name().to_string(), None)
                        };
                    WindowContent::Indicator(IndicatorData {
                        indicator_id,
                        active: false,
                        color: active_color,
                    })
                }
                WidgetType::Performance => WindowContent::Performance,
                WidgetType::Hand => WindowContent::Hand {
                    item: None,
                    link: None,
                },
                WidgetType::Room => WindowContent::Room(RoomContent {
                    name: String::new(),
                    description: Vec::new(),
                    exits: Vec::new(),
                    players: Vec::new(),
                    objects: Vec::new(),
                }),
                WidgetType::Inventory => {
                    let mut content = TextContent::new(title, 10000);
                    content.streams = vec!["inv".to_string()];
                    WindowContent::Inventory(content)
                }
                WidgetType::Reserve => {
                    let mut content = TextContent::new(title, 10000);
                    content.streams = vec!["reserve".to_string()];
                    WindowContent::Reserve(content)
                }
                WidgetType::Spells => {
                    let mut content = TextContent::new(title, 10000);
                    content.streams = vec!["Spells".to_string()];
                    tracing::debug!("init_windows: Creating Spells window '{}' with streams={:?}", title, content.streams);
                    WindowContent::Spells(content)
                }
                WidgetType::ActiveEffects => {
                    // Extract category from window def
                    let category =
                        if let crate::config::WindowDef::ActiveEffects { data, .. } = window_def {
                            data.category.clone()
                        } else {
                            "Unknown".to_string()
                        };
                    WindowContent::ActiveEffects(crate::data::ActiveEffectsContent {
                        category,
                        effects: Vec::new(),
                        generation: 0,
                    })
                }
                WidgetType::Targets => WindowContent::Targets,
                WidgetType::Players => WindowContent::Players,
                WidgetType::Items => WindowContent::Items,
                WidgetType::Container => {
                    // Get container_title from window def if available
                    let container_title = if let crate::config::WindowDef::Container { data, .. } = window_def {
                        data.container_title.clone()
                    } else {
                        String::new()
                    };
                    WindowContent::Container { container_title }
                }
                WidgetType::Dashboard => WindowContent::Dashboard {
                    indicators: Vec::new(),
                },
                WidgetType::Perception => WindowContent::Perception(PerceptionData {
                    entries: Vec::new(),
                    last_update: 0,
                    generation: 0,
                }),
                WidgetType::Experience => WindowContent::Experience,
                WidgetType::GS4Experience => WindowContent::GS4Experience,
                WidgetType::Encumbrance => WindowContent::Encumbrance,
                WidgetType::Quickbar => WindowContent::Quickbar,
                WidgetType::Hotkeybar => {
                    let bar = if let crate::config::WindowDef::Hotkeybar { data, .. } = window_def {
                        data.bar.clone()
                    } else {
                        String::new()
                    };
                    WindowContent::Hotkeybar { bar }
                }
                WidgetType::MiniVitals => WindowContent::MiniVitals,
                WidgetType::Betrayer => WindowContent::Betrayer,
                WidgetType::WebUi => {
                    let page = if let crate::config::WindowDef::WebUi { data, .. } = window_def {
                        data.page.clone()
                    } else {
                        String::new()
                    };
                    WindowContent::WebUi(crate::data::webui::WebUiPanelContent::new(page, title))
                }
                // A resident dialog panel (combat, UberBar) renders from the
                // dialog store by its bound id — see add_new_window's twin arm.
                WidgetType::DialogPanel => {
                    let dialog_id = match window_def {
                        crate::config::WindowDef::DialogPanel { data, .. }
                            if !data.dialog_id.is_empty() =>
                        {
                            data.dialog_id.clone()
                        }
                        _ => window_def
                            .base()
                            .binding
                            .as_ref()
                            .map(|b| b.id().to_string())
                            .unwrap_or_default(),
                    };
                    WindowContent::DialogPanel { dialog_id }
                }
                _ => WindowContent::Empty,
            };

            let window = WindowState {
                name: window_def.name().to_string(),
                widget_type,
                content,
                position,
                visible: true,
                content_align: window_def.base().content_align.clone(),
                focused: false,
                ephemeral: false,
            };

            self.ui_state
                .set_window(window_def.name().to_string(), window);
        }

        // Set default focused window to "main" if it exists (enables scrolling with PageUp/PageDown)
        if self.ui_state.focused_window.is_none() {
            if self.ui_state.windows.contains_key("main") {
                self.ui_state.set_focus(Some("main".to_string()));
                tracing::debug!("Set default focused window to 'main'");
            } else if let Some(first_name) = self.ui_state.windows.keys().next().cloned() {
                // Fall back to first window if main doesn't exist
                self.ui_state.set_focus(Some(first_name.clone()));
                tracing::debug!("Set default focused window to '{}'", first_name);
            }
        }

        // Update text stream subscriber map for routing (uses widget stream configs)
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);

        // Populate all spells windows from buffer (spells are sent once at login)
        for window in self.ui_state.windows.values_mut() {
            if let WindowContent::Spells(ref mut content) = window.content {
                self.message_processor.populate_spells_window(content);
            }
        }

        // Restore preserved command history
        if let Some(history) = preserved_history {
            if let Some(window) = self.ui_state.windows.get_mut("command_input") {
                if let WindowContent::CommandInput {
                    history: ref mut h, ..
                } = window.content
                {
                    *h = history;
                }
            }
        }

        self.needs_render = true;
    }

    /// Add a single new window without destroying existing ones
    ///
    /// Uses absolute positioning from window definition with optional delta-based scaling.
    pub fn add_new_window(
        &mut self,
        window_def: &crate::config::WindowDef,
        _terminal_width: u16,
        _terminal_height: u16,
    ) {
        tracing::info!(
            "add_new_window: '{}' ({})",
            window_def.name(),
            window_def.widget_type()
        );

        // Use exact position from window definition
        let base = window_def.base();
        let position = WindowPosition {
            x: base.col,
            y: base.row,
            width: base.cols,
            height: base.rows,
        };

        tracing::debug!(
            "Window '{}' will be created at exact pos=({},{}) size={}x{}",
            window_def.name(),
            position.x.get(),
            position.y.get(),
            position.width.get(),
            position.height.get()
        );

        let is_room_window = window_def.widget_type() == "room";

        let widget_type = WidgetType::from_str(window_def.widget_type());

        let title = window_def
            .base()
            .title
            .as_deref()
            .unwrap_or("");

        let content = match widget_type {
            WidgetType::Text => {
                let (buffer_size, streams, compact, show_ts, ts_pos) =
                    if let crate::config::WindowDef::Text { data, .. } = window_def {
                        (
                            data.buffer_size,
                            data.streams.clone(),
                            data.compact,
                            data.show_timestamps,
                            data.timestamp_position
                                .unwrap_or(self.config.ui.timestamp_position),
                        )
                    } else {
                        (1000, vec![], false, false, self.config.ui.timestamp_position)
                    };
                let mut text_content = TextContent::new(title, buffer_size);
                text_content.streams = streams;
                text_content.compact = compact;
                text_content.show_timestamps = show_ts;
                text_content.timestamp_position = ts_pos;

                // For bounty windows: pre-populate with buffered bounty data if available
                if window_def.name().eq_ignore_ascii_case("bounty") && self.game_state.bounty.has_data() {
                    // Use compact lines if window is in compact mode, otherwise raw text
                    let lines = if compact {
                        &self.game_state.bounty.compact_lines
                    } else {
                        // For non-compact, use raw text as single line
                        std::slice::from_ref(&self.game_state.bounty.raw_text)
                    };

                    for line_text in lines {
                        text_content.add_line(crate::data::widget::StyledLine::from_text_with_stream(
                            line_text.clone(),
                            "bounty",
                        ));
                    }
                    tracing::info!(
                        "Pre-populated bounty window with {} buffered lines",
                        lines.len()
                    );
                }

                WindowContent::Text(text_content)
            }
            WidgetType::TabbedText => {
                // Extract tab definitions and buffer size from window def
                if let crate::config::WindowDef::TabbedText { data, .. } = window_def {
                    let global_ts_pos = self.config.ui.timestamp_position;
                    let tabs: Vec<(String, Vec<String>, bool, bool, crate::config::TimestampPosition)> = data
                        .tabs
                        .iter()
                        .map(|tab| {
                            // show_timestamps defaults to false if not explicitly set per-tab
                            let show_ts = tab.show_timestamps.unwrap_or(false);
                            let ignore = tab.ignore_activity.unwrap_or(false);
                            let ts_pos = tab.timestamp_position.unwrap_or(global_ts_pos);
                            (tab.name.clone(), tab.get_streams(), show_ts, ignore, ts_pos)
                        })
                        .collect();
                    WindowContent::TabbedText(crate::data::TabbedTextContent::new(
                        tabs,
                        data.buffer_size,
                    ))
                } else {
                    // Fallback if window_def is wrong type
                    WindowContent::TabbedText(crate::data::TabbedTextContent::new(
                        vec![(
                            "Default".to_string(),
                            vec!["main".to_string()],
                            false, // show_timestamps defaults to false
                            false,
                            crate::config::TimestampPosition::End,
                        )],
                        5000,
                    ))
                }
            }
            WidgetType::CommandInput => WindowContent::CommandInput {
                text: String::new(),
                cursor: 0,
                history: Vec::new(),
                history_index: None,
            },
            WidgetType::Progress => {
                let (label, progress_id, color, numbers_only, current_only) =
                    if let crate::config::WindowDef::Progress { data, .. } = window_def {
                        (
                            data.label.clone().unwrap_or_else(|| title.to_string()),
                            data.id
                                .clone()
                                .unwrap_or_else(|| window_def.name().to_string()),
                            data.color.clone(),
                            data.numbers_only,
                            data.current_only,
                        )
                    } else {
                        (
                            title.to_string(),
                            window_def.name().to_string(),
                            None,
                            false,
                            false,
                        )
                    };
                WindowContent::Progress(ProgressData {
                    value: 100,
                    max: 100,
                    label,
                    color,
                    progress_id,
                    numbers_only,
                    current_only,
                })
            }
            WidgetType::Countdown => {
                let (label, countdown_id, color, show_when_zero) =
                    if let crate::config::WindowDef::Countdown { data, .. } = window_def {
                        (
                            data.label.clone().unwrap_or_else(|| title.to_string()),
                            data.id
                                .clone()
                                .unwrap_or_else(|| window_def.name().to_string()),
                            data.color.clone(),
                            data.show_when_zero.unwrap_or(false),
                        )
                    } else {
                        (title.to_string(), window_def.name().to_string(), None, false)
                    };
                WindowContent::Countdown(CountdownData {
                    end_time: 0,
                    label,
                    countdown_id,
                    color,
                    show_when_zero,
                })
            }
            WidgetType::Map => WindowContent::Map(crate::data::MapData::default()),
            WidgetType::Compass => WindowContent::Compass(CompassData {
                directions: Vec::new(),
            }),
            WidgetType::InjuryDoll => WindowContent::InjuryDoll(InjuryDollData::new()),
            WidgetType::Indicator => {
                let (indicator_id, active_color) =
                    if let crate::config::WindowDef::Indicator { data, .. } = window_def {
                        (
                            data.indicator_id
                                .clone()
                                .unwrap_or_else(|| window_def.name().to_string()),
                            data.active_color.clone(),
                        )
                    } else {
                        (window_def.name().to_string(), None)
                    };
                WindowContent::Indicator(IndicatorData {
                    indicator_id,
                    active: false,
                    color: active_color,
                })
            }
            WidgetType::Perception => WindowContent::Perception(PerceptionData {
                entries: Vec::new(),
                last_update: 0,
                generation: 0,
            }),
            WidgetType::Performance => WindowContent::Performance,
            WidgetType::Hand => WindowContent::Hand {
                item: None,
                link: None,
            },
            WidgetType::Room => WindowContent::Room(RoomContent {
                name: String::new(),
                description: Vec::new(),
                exits: Vec::new(),
                players: Vec::new(),
                objects: Vec::new(),
            }),
            WidgetType::Inventory => {
                let mut content = TextContent::new(title, 0);
                content.streams = vec!["inv".to_string()];
                WindowContent::Inventory(content)
            }
            WidgetType::Reserve => {
                let mut content = TextContent::new(title, 0);
                content.streams = vec!["reserve".to_string()];
                WindowContent::Reserve(content)
            }
            WidgetType::Spells => {
                let mut content = TextContent::new(title, 0);
                content.streams = vec!["Spells".to_string()];
                WindowContent::Spells(content)
            }
            WidgetType::ActiveEffects => {
                // Extract category from window def
                let category =
                    if let crate::config::WindowDef::ActiveEffects { data, .. } = window_def {
                        data.category.clone()
                    } else {
                        "Unknown".to_string()
                    };
                WindowContent::ActiveEffects(crate::data::ActiveEffectsContent {
                    category,
                    effects: Vec::new(),
                    generation: 0,
                })
            }
            WidgetType::Targets => WindowContent::Targets,
            WidgetType::Players => WindowContent::Players,
            WidgetType::Items => WindowContent::Items,
            WidgetType::Container => {
                // Get container_title from window def if available
                let container_title = if let crate::config::WindowDef::Container { data, .. } = window_def {
                    data.container_title.clone()
                } else {
                    String::new()
                };
                WindowContent::Container { container_title }
            }
            WidgetType::Dashboard => WindowContent::Dashboard {
                indicators: Vec::new(),
            },
            WidgetType::Experience => WindowContent::Experience,
            WidgetType::GS4Experience => WindowContent::GS4Experience,
            WidgetType::Encumbrance => WindowContent::Encumbrance,
            WidgetType::Quickbar => WindowContent::Quickbar,
            WidgetType::Hotkeybar => {
                let bar = if let crate::config::WindowDef::Hotkeybar { data, .. } = window_def {
                    data.bar.clone()
                } else {
                    String::new()
                };
                WindowContent::Hotkeybar { bar }
            }
            WidgetType::MiniVitals => WindowContent::MiniVitals,
            WidgetType::Betrayer => WindowContent::Betrayer,
            WidgetType::WebUi => {
                let page = if let crate::config::WindowDef::WebUi { data, .. } = window_def {
                    data.page.clone()
                } else {
                    String::new()
                };
                WindowContent::WebUi(crate::data::webui::WebUiPanelContent::new(page, title))
            }
            // A resident dialog panel (combat, UberBar) renders from the
            // dialog store by its bound id. Without this arm the window fell
            // through to Empty and rendered blank even though the store held
            // its bars/labels/skins.
            WidgetType::DialogPanel => {
                let dialog_id = match window_def {
                    crate::config::WindowDef::DialogPanel { data, .. }
                        if !data.dialog_id.is_empty() =>
                    {
                        data.dialog_id.clone()
                    }
                    // Fall back to the binding id (the discovery sets both, but
                    // a hand-authored panel might only carry the binding).
                    _ => window_def
                        .base()
                        .binding
                        .as_ref()
                        .map(|b| b.id().to_string())
                        .unwrap_or_default(),
                };
                WindowContent::DialogPanel { dialog_id }
            }
            _ => WindowContent::Empty,
        };

        let window = WindowState {
            name: window_def.name().to_string(),
            widget_type,
            content,
            position: position.clone(),
            visible: true,
            content_align: window_def.base().content_align.clone(),
            focused: false,
            ephemeral: false,
        };

        self.ui_state
            .set_window(window_def.name().to_string(), window);
        self.needs_render = true;

        // Clear inventory cache if this is an inventory window to force initial render
        if window_def.widget_type() == "inventory" {
            self.message_processor.clear_inventory_cache();
        }

        // Same for reserve windows - force the next reserve update to render
        if window_def.widget_type() == "reserve" {
            self.message_processor.clear_reserve_cache();
        }

        // Populate spells window from buffer if this is a spells window
        // Spells are sent once at login, so we populate immediately from buffer
        if window_def.widget_type() == "spells" {
            if let Some(window) = self.ui_state.windows.get_mut(window_def.name()) {
                if let WindowContent::Spells(ref mut content) = window.content {
                    self.message_processor.populate_spells_window(content);
                }
            }
        }

        // Set dirty flag for room windows to trigger sync in TUI frontend
        if is_room_window {
            self.room_window_dirty = true;
        }

        tracing::info!(
            "Created new window '{}' at ({}, {}) size {}x{}",
            window_def.name(),
            position.x.get(),
            position.y.get(),
            position.width.get(),
            position.height.get()
        );

        // Update text stream subscriber map (new window may have stream subscriptions)
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);
    }

    /// Update an existing window's position without destroying content
    /// Update an existing window's position from window definition (uses exact positions, no scaling)
    ///
    /// This is called when editing a window via the window editor. It applies the exact
    /// position from the window definition to the UI state without any scaling.
    pub fn update_window_position(
        &mut self,
        window_def: &crate::config::WindowDef,
        _terminal_width: u16,
        _terminal_height: u16,
    ) {
        let base = window_def.base();
        let position = WindowPosition {
            x: base.col,
            y: base.row,
            width: base.cols,
            height: base.rows,
        };

        if let Some(window_state) = self.ui_state.windows.get_mut(window_def.name()) {
            window_state.position = position.clone();
            self.needs_render = true;
            tracing::info!(
                "Updated window '{}' to EXACT position ({}, {}) size {}x{}",
                window_def.name(),
                position.x.get(),
                position.y.get(),
                position.width.get(),
                position.height.get()
            );
        }
    }

    /// Sync tabbed window tabs from layout definition.
    /// Called after window editor saves changes to a TabbedText window.
    /// Returns true if structural changes occurred (requiring widget cache reset).
    pub fn sync_tabbed_window_tabs(&mut self, window_name: &str) -> bool {
        // Find the layout definition
        let window_def = self.layout.windows.iter().find(|w| w.name() == window_name);
        let Some(crate::config::WindowDef::TabbedText { data, base: _ }) = window_def else {
            return false;
        };

        // Get the TabbedTextContent from ui_state
        let Some(window) = self.ui_state.windows.get_mut(window_name) else {
            return false;
        };
        let crate::data::WindowContent::TabbedText(tabbed_content) = &mut window.content else {
            return false;
        };

        // Build new tab definitions from layout
        let global_ts_pos = self.config.ui.timestamp_position;
        let new_tabs: Vec<_> = data
            .tabs
            .iter()
            .map(|tab| {
                let show_ts = tab.show_timestamps.unwrap_or(false);
                let ignore = tab.ignore_activity.unwrap_or(false);
                let ts_pos = tab.timestamp_position.unwrap_or(global_ts_pos);
                (tab.name.clone(), tab.get_streams(), show_ts, ignore, ts_pos)
            })
            .collect();

        // Update and return whether structural change occurred
        let changed = tabbed_content.update_tabs(new_tabs, data.buffer_size);
        if changed {
            tracing::info!("Updated tabs for window '{}'", window_name);
            // Tab streams changed - keep the routing index in sync
            self.message_processor
                .update_text_stream_subscribers(&self.ui_state);
        }
        changed
    }

    /// Remove a window from UI state
    pub fn remove_window(&mut self, name: &str) {
        self.ui_state.remove_window(name);
        self.needs_render = true;
        tracing::info!("Removed window '{}'", name);

        // Update text stream subscriber map (removed window may have had stream subscriptions)
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);
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

    /// Save current layout
    pub fn save_layout(&mut self, name: &str, terminal_width: u16, terminal_height: u16) {
        tracing::info!("========== SAVE LAYOUT: '{}' START ==========", name);
        tracing::info!(
            "Current terminal size: {}x{}",
            terminal_width,
            terminal_height
        );
        tracing::info!("Layout has {} windows defined", self.layout.windows.len());
        tracing::info!(
            "UI state has {} windows rendered",
            self.ui_state.windows.len()
        );

        // IMPORTANT: Capture actual window positions from UI state before saving
        // (user may have moved/resized windows with mouse)
        for window_def in &mut self.layout.windows {
            let window_name = window_def.name().to_string();
            let base = window_def.base();

            tracing::debug!(
                "Window '{}' BEFORE capture: pos=({},{}) size={}x{}",
                window_name,
                base.col.get(),
                base.row.get(),
                base.cols.get(),
                base.rows.get()
            );

            if let Some(window_state) = self.ui_state.windows.get(&window_name) {
                let ui_pos = &window_state.position;
                tracing::info!(
                    "Window '{}' - Capturing from UI state: pos=({},{}) size={}x{}",
                    window_name,
                    ui_pos.x.get(),
                    ui_pos.y.get(),
                    ui_pos.width.get(),
                    ui_pos.height.get()
                );

                // Clamp window position and size to terminal boundaries before saving
                let clamped_x = ui_pos.x.get().min(terminal_width.saturating_sub(1));
                let clamped_y = ui_pos.y.get().min(terminal_height.saturating_sub(1));

                // Ensure width doesn't exceed available space
                // Use window's min_cols constraint (default 1) instead of hardcoded 10
                let max_width = terminal_width.saturating_sub(clamped_x);
                let min_width = base.min_cols.unwrap_or(1);
                let clamped_width = ui_pos.width.get().min(max_width).max(min_width);

                // Ensure height doesn't exceed available space
                // Use window's min_rows constraint (default 1)
                let max_height = terminal_height.saturating_sub(clamped_y);
                let min_height = base.min_rows.unwrap_or(1);
                let clamped_height = ui_pos.height.get().min(max_height).max(min_height);

                if clamped_x != ui_pos.x.get()
                    || clamped_y != ui_pos.y.get()
                    || clamped_width != ui_pos.width.get()
                    || clamped_height != ui_pos.height.get()
                {
                    tracing::warn!(
                        "Window '{}' clamped: ({},{} {}x{}) -> ({},{} {}x{}) to fit terminal {}x{}",
                        window_name,
                        ui_pos.x.get(),
                        ui_pos.y.get(),
                        ui_pos.width.get(),
                        ui_pos.height.get(),
                        clamped_x,
                        clamped_y,
                        clamped_width,
                        clamped_height,
                        terminal_width,
                        terminal_height
                    );
                }

                let base = window_def.base_mut();
                base.row = crate::data::geometry::Row::new(clamped_y);
                base.col = crate::data::geometry::Col::new(clamped_x);
                base.rows = crate::data::geometry::Height::new(clamped_height);
                base.cols = crate::data::geometry::Width::new(clamped_width);

                tracing::debug!(
                    "Window '{}' AFTER capture: pos=({},{}) size={}x{}",
                    window_name,
                    base.col.get(),
                    base.row.get(),
                    base.cols.get(),
                    base.rows.get()
                );
            } else {
                tracing::warn!(
                    "Window '{}' is in layout but NOT in ui_state! Cannot capture position.",
                    window_name
                );
            }
        }

        let layout_path = match Config::layout_path(name) {
            Ok(path) => path,
            Err(e) => {
                tracing::error!("Failed to get layout path for '{}': {}", name, e);
                self.add_system_message(&format!("Failed to get layout path: {}", e));
                return;
            }
        };

        tracing::info!("Saving layout to: {}", layout_path.display());

        // Pass actual terminal size with force=true so it always updates to current terminal size
        self.layout.theme = Some(self.config.active_theme.clone());
        match self
            .layout
            .save(name, Some((terminal_width, terminal_height)), true)
        {
            Ok(_) => {
                tracing::info!(
                    "Layout '{}' saved successfully to {}",
                    name,
                    layout_path.display()
                );
                tracing::info!("========== SAVE LAYOUT: '{}' SUCCESS ==========", name);
                self.add_system_message(&format!("Layout saved as '{}'", name));
                // Clear modified flag and update base layout name
                self.layout_modified_since_save = false;
                self.base_layout_name = Some(name.to_string());
                // Mirror the just-saved arrangement into the auto-save slot
                // startup reads, so the save sticks even if this session
                // ends without a clean quit.
                self.autosave_layout();
            }
            Err(e) => {
                tracing::error!("Failed to save layout '{}': {}", name, e);
                tracing::info!("========== SAVE LAYOUT: '{}' FAILED ==========", name);
                self.add_system_message(&format!("Failed to save layout: {}", e));
            }
        }
    }

    /// Load a saved layout and update window positions/configs
    ///
    /// Loads layout at exact positions specified in file.
    /// Use .resize command for delta-based proportional scaling after loading.

    /// Resize all windows proportionally based on current terminal size (VellumFE algorithm)
    ///
    /// This command resets to the baseline layout and applies delta-based proportional distribution.
    /// This is the ONLY place (besides initial load) that should perform scaling operations.

    /// Helper to get minimum widget size based on widget type (from VellumFE)


    /// Apply proportional height resize (from VellumFE apply_height_resize)
    /// Adapted for WindowDef enum structure

    /// Apply proportional width resize (from VellumFE apply_width_resize)
    /// Adapted for WindowDef enum structure
    /// baseline_rows: Vec of (name, baseline_row, baseline_rows) for grouping windows by original row

    /// Sync layout WindowDefs to ui_state WindowStates without destroying content
    ///
    /// Uses exact positions from layout file.
    /// Use .resize command for delta-based proportional scaling.

    /// Load a saved layout with terminal size for immediate reinitialization

    /// List all saved layouts

    /// Resize layout using delta-based proportional distribution
    /// This method is called by the .resize command and requires manual invocation

    /// Wrapper for resize command - gets terminal size from layout

    /// List all windows
    pub(super) fn list_windows(&mut self) {
        let window_count = self.ui_state.windows.len();

        // Collect window info first to avoid borrow checker issues
        let mut window_info = Vec::new();
        for (name, window) in &self.ui_state.windows {
            let pos = &window.position;
            let visible = if window.visible { "visible" } else { "hidden" };
            window_info.push(format!(
                "  {} - {}x{} at ({},{}) - {} - {}",
                name,
                pos.width.get(),
                pos.height.get(),
                pos.x.get(),
                pos.y.get(),
                visible,
                format!("{:?}", window.widget_type)
            ));
        }

        // Now add all messages
        self.add_system_message(&format!("=== Windows ({}) ===", window_count));
        for info in window_info {
            self.add_system_message(&info);
        }
    }

    /// Push a Text def's content settings (streams, buffer, compact,
    /// timestamps) onto the live window, rebuild stream routing, and re-feed
    /// bounty data. Editors that only replace the layout def otherwise leave
    /// the live window on its old settings until it is recreated.
    pub fn apply_text_content_settings(&mut self, def: &crate::config::WindowDef) {
        let crate::config::WindowDef::Text { data, .. } = def else {
            return;
        };
        let Some(window) = self.ui_state.windows.get_mut(def.name()) else {
            return;
        };
        let WindowContent::Text(text) = &mut window.content else {
            return;
        };
        text.streams = data.streams.clone();
        text.max_lines = data.buffer_size;
        text.compact = data.compact;
        text.show_timestamps = data.show_timestamps;
        if let Some(pos) = data.timestamp_position {
            text.timestamp_position = pos;
        }
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);
        self.refresh_bounty_window(def.name());
    }

    /// Rebuild a bounty-fed text window's lines from the cached bounty data,
    /// honoring its current compact flag. Compaction is applied at line
    /// ingestion, so toggling the flag otherwise only affects the NEXT
    /// bounty update — which made the editor's condense checkbox look inert
    /// until the window was closed and reopened.
    pub fn refresh_bounty_window(&mut self, name: &str) {
        if !self.game_state.bounty.has_data() {
            return;
        }
        let Some(window) = self.ui_state.windows.get_mut(name) else {
            return;
        };
        let WindowContent::Text(text) = &mut window.content else {
            return;
        };
        // Only rebuild windows fed solely by the bounty stream: mixed-stream
        // history can't be reconstructed from the bounty cache.
        let bounty_only = text.streams.len() == 1
            && text.streams[0].eq_ignore_ascii_case("bounty");
        if !bounty_only {
            return;
        }
        let lines: Vec<String> = if text.compact {
            self.game_state.bounty.compact_lines.clone()
        } else {
            vec![self.game_state.bounty.raw_text.clone()]
        };
        text.lines.clear();
        for line_text in lines {
            text.add_line(crate::data::widget::StyledLine::from_text_with_stream(
                line_text, "bounty",
            ));
        }
    }

    /// True if a shown window other than `excluding` carries the "main"
    /// stream — a text window subscribed to it, or a tabbedtext with a
    /// subscribed tab. The story feed must always have a live subscriber;
    /// hide_window gates on this instead of hard-protecting the window
    /// NAMED "main" (the feed may live in a tabbedtext tab instead).
    fn main_stream_has_subscriber_excluding(&self, excluding: &str) -> bool {
        self.ui_state.windows.iter().any(|(win_name, window)| {
            if win_name == excluding {
                return false;
            }
            Self::window_subscribes_to_main(&window.content)
        })
    }

    fn window_subscribes_to_main(content: &crate::data::WindowContent) -> bool {
        match content {
            crate::data::WindowContent::Text(text) => {
                text.streams.iter().any(|s| s.eq_ignore_ascii_case("main"))
            }
            crate::data::WindowContent::TabbedText(tabbed) => tabbed.tabs.iter().any(|tab| {
                tab.definition
                    .streams
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case("main"))
            }),
            _ => false,
        }
    }

    /// Hide a window (keep in layout for persistence, remove from UI)
    pub fn hide_window(&mut self, name: &str) {
        // Main-stream invariant: hiding the last shown subscriber of the
        // story feed would silently eat all main text.
        let hides_main_subscriber = self
            .ui_state
            .windows
            .get(name)
            .map(|w| Self::window_subscribes_to_main(&w.content))
            .unwrap_or(false);
        if hides_main_subscriber && !self.main_stream_has_subscriber_excluding(name) {
            self.add_system_message(
                "Cannot hide the only window showing the story (main) feed. \
                 Add the main stream to another window first.",
            );
            return;
        }

        // Find ALL windows with this name and mark as hidden (handles duplicates)
        let mut found_count = 0;
        let mut is_command_input = false;
        for window_def in self.layout.windows.iter_mut() {
            if window_def.name() == name && window_def.base().visibility.is_shown() {
                window_def.base_mut().visibility = crate::config::WindowVisibility::Hidden;
                is_command_input |= window_def.widget_type() == "command_input";
                found_count += 1;
            }
        }

        if found_count > 0 {
            // TUI force-show: persist the hidden flag (so the GUI honors
            // it) but keep the input line on screen — the TUI has no
            // fallback bar and would otherwise leave the user typing blind.
            if is_command_input && self.force_show_command_input {
                self.add_system_message(
                    "Command input hidden in the layout (GUI shows its fallback bar); \
                     the TUI keeps it visible.",
                );
                self.mark_layout_modified();
                self.needs_render = true;
                return;
            }
            // Remove from UI state (but keep in layout!)
            self.ui_state.remove_window(name);

            let msg = if found_count > 1 {
                format!(
                    "Window '{}' hidden ({} duplicates removed)",
                    name, found_count
                )
            } else {
                format!("Window '{}' hidden", name)
            };
            self.add_system_message(&msg);
            self.mark_layout_modified();
            self.needs_render = true;
            tracing::info!(
                "Hid {} instance(s) of window '{}' - template(s) preserved in layout",
                found_count,
                name
            );
        } else {
            self.add_system_message(&format!("Window '{}' not found or already hidden", name));
        }
    }

    /// Show a window (unhide it - restore from layout template)
    pub fn show_window(&mut self, name: &str, terminal_width: u16, terminal_height: u16) {
        // Use Layout's add_window() which handles both:
        // 1. Existing windows (just marks visible)
        // 2. New windows (creates from template and adds to layout)
        if let Err(e) = self.layout.add_window(name) {
            self.add_system_message(&format!("Failed to add window '{}': {}", name, e));
            return;
        }

        // Get the window definition (now guaranteed to exist)
        let window_def_clone = self
            .layout
            .windows
            .iter()
            .find(|w| w.name() == name)
            .expect("Window should exist after add_window")
            .clone();

        // Create in UI state from layout template
        self.add_new_window(&window_def_clone, terminal_width, terminal_height);

        self.add_system_message(&format!("Window '{}' shown", name));
        self.mark_layout_modified();
        self.needs_render = true;
        tracing::info!("Showed window '{}' - added to layout and UI state", name);
    }

    /// Create any window definitions this layout lacks, from a saved layout's
    /// captured defs. Used by the GUI `.loadlayout`: a named layout saved on
    /// one character carries the full window definitions, so loading it into a
    /// fresh profile (which only has the default windows) recreates the missing
    /// windows before the arrangement is reconciled. Windows already present
    /// are left untouched — their live content (buffered text, etc.) survives.
    /// Returns the names actually created.
    pub fn materialize_missing_windows(
        &mut self,
        defs: &[crate::config::WindowDef],
        terminal_width: u16,
        terminal_height: u16,
    ) -> Vec<String> {
        let mut created = Vec::new();
        for def in defs {
            let name = def.name().to_string();
            if self.ui_state.windows.contains_key(&name) {
                continue;
            }
            // Keep the layout's def list authoritative so a later .savelayout
            // (or autosave) re-persists the window; add_new_window only writes
            // ui_state.
            if !self.layout.windows.iter().any(|w| w.name() == name) {
                self.layout.windows.push(def.clone());
            }
            self.add_new_window(def, terminal_width, terminal_height);
            created.push(name);
        }
        if !created.is_empty() {
            self.needs_render = true;
        }
        created
    }

    /// Process pending window additions from openDialog events.
    /// Called by the frontend each frame with terminal dimensions.
    /// Whether a layout window equivalent to `template_name` already exists,
    /// regardless of its display name. Dialog-driven singleton widgets
    /// (experience/stance/encum/minivitals/injuries/buffs/…) get placed by
    /// the user under an auto-generated `custom-*` name, so a bare
    /// `w.name() == template_name` check misses them and the game re-adds a
    /// duplicate on every dialog re-send. Match on the template's WIDGET
    /// TYPE instead — plus the distinguishing data field for the two types
    /// that legitimately allow multiple instances (Progress `id`,
    /// ActiveEffects `category`), so a Buffs window doesn't shadow Debuffs
    /// and a stance bar doesn't shadow an unrelated progress bar.
    fn layout_has_equivalent_window(&self, template_name: &str) -> bool {
        self.layout_equivalent_window_name(template_name).is_some()
    }

    /// The NAME of an existing layout window equivalent to `template_name`
    /// (see layout_has_equivalent_window for the identity rules), or None.
    fn layout_equivalent_window_name(&self, template_name: &str) -> Option<String> {
        use crate::config::WindowDef;
        let template = crate::core::local_catalog::seed(template_name)?;
        let tmpl_type = template.widget_type();
        self.layout
            .windows
            .iter()
            .find(|w| {
                if w.widget_type() != tmpl_type {
                    return false;
                }
                match (&template, *w) {
                    // Disambiguate the shared types by their identity field.
                    (WindowDef::Progress { data: t, .. }, WindowDef::Progress { data: w, .. }) => {
                        t.id == w.id
                    }
                    (
                        WindowDef::ActiveEffects { data: t, .. },
                        WindowDef::ActiveEffects { data: w, .. },
                    ) => t.category.eq_ignore_ascii_case(&w.category),
                    // All other singleton types: one per layout, type is enough.
                    _ => true,
                }
            })
            .map(|w| w.name().to_string())
    }

    pub fn process_pending_window_additions(&mut self, terminal_width: u16, terminal_height: u16) {
        use crate::config::WindowBinding;
        // Drain pending additions. As of U2 these are DIALOG IDS (e.g.
        // "expr", "stance"), not template names — so we can bind the created
        // window to its game feed.
        let pending: Vec<String> = self.ui_state.pending_window_additions.drain(..).collect();

        for dialog_id in pending {
            // The claimed view's seed key, via the resolver (Phase 4).
            let template_name =
                Self::seed_template_for(&WindowBinding::Dialog(dialog_id.clone()));

            // Already have a window bound to this feed? The game only ever
            // needs one home per feed to create — refresh flows to all bound
            // windows via the normal data path, so just ensure UI state
            // exists for any shown bound window and move on (no duplicate).
            if self.layout.has_window_bound_to(&dialog_id) {
                let bound_shown: Vec<String> = self
                    .layout
                    .windows
                    .iter()
                    .filter(|w| {
                        w.base().binding.as_ref().is_some_and(|b| b.id() == dialog_id)
                            && w.base().visibility.is_shown()
                    })
                    .map(|w| w.name().to_string())
                    .collect();
                for name in bound_shown {
                    if !self.ui_state.windows.contains_key(&name) {
                        if let Some(def) = self.layout.windows.iter().find(|w| w.name() == name) {
                            let def = def.clone();
                            self.add_new_window(&def, terminal_width, terminal_height);
                            self.needs_render = true;
                            self.ui_state.needs_widget_reset = true;
                        }
                    }
                }
                continue;
            }

            // No bound window yet. A user may have an EQUIVALENT widget placed
            // under a renamed custom-* name (U0) — adopt it by tagging the
            // binding, so future feeds resolve by id and we never duplicate.
            if let Some(existing_name) = self.layout_equivalent_window_name(&template_name) {
                if let Some(def) = self
                    .layout
                    .windows
                    .iter_mut()
                    .find(|w| w.name() == existing_name)
                {
                    def.base_mut().binding = Some(WindowBinding::Dialog(dialog_id.clone()));
                }
                // Ensure UI state if it's shown.
                let shown = self
                    .layout
                    .windows
                    .iter()
                    .find(|w| w.name() == existing_name)
                    .map(|w| w.base().visibility.is_shown())
                    .unwrap_or(false);
                if shown && !self.ui_state.windows.contains_key(&existing_name) {
                    if let Some(def) = self.layout.windows.iter().find(|w| w.name() == existing_name)
                    {
                        let def = def.clone();
                        self.add_new_window(&def, terminal_width, terminal_height);
                        self.needs_render = true;
                        self.ui_state.needs_widget_reset = true;
                    }
                }
                continue;
            }

            // Genuinely new: add the templated window, bound to this feed.
            // (U2a keeps the current visible-spawn behavior; U2b gates on
            // visibility so a hidden binding suppresses the auto-spawn.)
            if let Err(e) = self.layout.add_window(&template_name) {
                tracing::warn!("Failed to auto-add window '{}': {}", template_name, e);
                continue;
            }
            let created = self
                .layout
                .windows
                .iter()
                .rev()
                .find(|w| w.widget_type() == template_name || w.name() == template_name)
                .or_else(|| self.layout.windows.last())
                .map(|w| w.name().to_string());
            if let Some(name) = created {
                if let Some(def) = self.layout.windows.iter_mut().find(|w| w.name() == name) {
                    def.base_mut().binding = Some(WindowBinding::Dialog(dialog_id.clone()));
                }
                if let Some(def) = self.layout.windows.iter().find(|w| w.name() == name) {
                    let def = def.clone();
                    self.add_new_window(&def, terminal_width, terminal_height);
                    tracing::info!("Auto-added bound window '{}' from openDialog '{}'", name, dialog_id);
                    self.needs_render = true;
                    self.ui_state.needs_widget_reset = true;
                }
            }
        }
    }

    /// Delete a window (legacy - use hide_window instead)
    pub(super) fn delete_window(&mut self, name: &str) {
        // For backwards compatibility, redirect to hide
        self.hide_window(name);
    }

    /// Permanently delete a window from the layout, but STASH its full def in
    /// `layout.deleted_windows` so it can be restored later. This is the honest
    /// "delete" (distinct from hide): the window leaves the Windows menu and
    /// stops rendering, yet a custom window — the only record of a moved
    /// command_input or a user-authored window that `+ Custom window` can't
    /// recreate — is never actually lost. Returns true if a window was deleted.
    pub fn delete_and_stash_window(&mut self, name: &str) -> bool {
        // Remove the live UI window.
        self.remove_window(name);
        // Pull the def out of the layout and stash it (newest last). If a def
        // with this name is already stashed, replace it (a re-delete after a
        // restore keeps one copy).
        let Some(pos) = self.layout.windows.iter().position(|w| w.name() == name) else {
            return false;
        };
        let def = self.layout.windows.remove(pos);
        // A dialog-bound window carries an entry in the popup allow-set
        // (shown_dialog_ids) whenever it was shown. hide_window clears that,
        // but delete must too — otherwise the id lingers and the next
        // dialogData the game sends re-pops the dialog as a bare popup
        // (titled "Dialog"), resurrecting a window the user deleted.
        if let Some(crate::config::WindowBinding::Dialog(id)) = def.base().binding.clone() {
            self.ui_state.shown_dialog_ids.remove(&id);
            if self
                .ui_state
                .active_dialog
                .as_ref()
                .is_some_and(|d| d.id == id)
            {
                self.ui_state.active_dialog = None;
            }
        }
        self.layout.deleted_windows.retain(|w| w.name() != name);
        self.layout.deleted_windows.push(def);
        self.mark_layout_modified();
        self.schedule_layout_autosave();
        true
    }

    /// Deleted windows the user can restore, newest first, as
    /// `(name, display_title)`: restore by the stable `name`, show the human
    /// `title` (falling back to the name when no title was set).
    pub fn deleted_windows_for_restore(&self) -> Vec<(String, String)> {
        self.layout
            .deleted_windows
            .iter()
            .rev()
            .map(|w| {
                let name = w.name().to_string();
                let title = w
                    .base()
                    .title
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| name.clone());
                (name, title)
            })
            .collect()
    }

    /// Just the internal names of restorable deleted windows, newest first.
    /// (Kept for tests / callers that only need the key.)
    pub fn deleted_window_names(&self) -> Vec<String> {
        self.layout
            .deleted_windows
            .iter()
            .rev()
            .map(|w| w.name().to_string())
            .collect()
    }

    /// Restore a previously deleted window by name: move its def out of the
    /// stash and back into the layout (Shown), then materialize the live
    /// window. Returns true if it was restored. If a live window with the same
    /// name now exists (the name was reused), the restore is refused so it
    /// can't clobber the current window.
    pub fn restore_deleted_window(&mut self, name: &str, width: u16, height: u16) -> bool {
        if self.layout.windows.iter().any(|w| w.name() == name) {
            self.add_system_message(&format!(
                "Can't restore '{name}': a window with that name already exists."
            ));
            return false;
        }
        let Some(pos) = self
            .layout
            .deleted_windows
            .iter()
            .position(|w| w.name() == name)
        else {
            return false;
        };
        let mut def = self.layout.deleted_windows.remove(pos);
        // A restored window comes back visible (Ephemeral would not persist).
        def.base_mut().visibility = crate::config::WindowVisibility::Shown;
        self.layout.windows.push(def);
        self.mark_layout_modified();
        // Rebuild the live windows so the restored def gets a UI window.
        self.init_windows(width, height);
        self.schedule_layout_autosave();
        self.add_system_message(&format!("Restored window '{name}'."));
        true
    }

    /// Create an ephemeral container window at screen center (or saved position if available)
    pub fn create_ephemeral_container_window(
        &mut self,
        container_title: &str,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        use crate::data::{WidgetType, WindowContent, WindowPosition, WindowState};

        // Use simple lowercase name for internal tracking (e.g., "bandolier")
        let window_name = container_title.replace(' ', "_").to_lowercase();

        // Skip if already exists
        if self.ui_state.windows.contains_key(&window_name) {
            tracing::debug!(
                "Container window '{}' already exists, skipping creation",
                window_name
            );
            return;
        }

        // Check for saved position, otherwise center with reasonable defaults
        let (x, y, w, h) = if let Some(saved) = self.saved_dialog_positions.containers.get(&window_name) {
            let width = saved.width.unwrap_or(40);
            let height = saved.height.unwrap_or(15);
            // Clamp to terminal bounds
            let x = saved.x.min(terminal_width.saturating_sub(width));
            let y = saved.y.min(terminal_height.saturating_sub(height));
            tracing::debug!("Using saved position for container '{}': ({}, {}) {}x{}", window_name, x, y, width, height);
            (x, y, width, height)
        } else {
            // Redesign Phase 3e: one placement policy, honoring the
            // declaration's own hints when the game sent any. (Container
            // hints are keyed by container id; only the title reaches
            // here, so containers ride the kind default until the id is
            // plumbed through — the panels below DO consume hints.)
            crate::core::placement::ephemeral_placement(
                None,
                (40, 15),
                crate::core::placement::PlacementAnchor::Center,
                (terminal_width, terminal_height),
            )
        };

        let window = WindowState {
            name: window_name.clone(),
            widget_type: WidgetType::Container,
            content: WindowContent::Container {
                container_title: container_title.to_string(),
            },
            position: WindowPosition {
                x: crate::data::geometry::Col::new(x),
                y: crate::data::geometry::Row::new(y),
                width: crate::data::geometry::Width::new(w),
                height: crate::data::geometry::Height::new(h),
            },
            visible: true,
            focused: false,
            content_align: None,
            ephemeral: true,
        };

        self.ui_state.set_window(window_name.clone(), window);
        self.ui_state.ephemeral_windows.insert(window_name);
        self.add_system_message(&format!("Created container window: {}", container_title));
        self.needs_render = true;

        tracing::info!(
            "Created ephemeral container window for '{}' at ({}, {})",
            container_title,
            x,
            y
        );
    }

    /// Create an ephemeral dockable panel window for a resident dialog
    /// (combat, befriend, ...). Positioned like an ephemeral container
    /// window; content renders from ui_state.dialog_store by `dialog_id`.
    pub fn create_dialog_panel_window(
        &mut self,
        dialog_id: &str,
        title: &str,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        use crate::data::{WidgetType, WindowContent, WindowPosition, WindowState};

        let window_name = format!("panel_{}", dialog_id.replace(' ', "_").to_lowercase());
        if self.ui_state.windows.contains_key(&window_name) {
            return;
        }

        // Redesign Phase 3e: seed rect from the single placement policy —
        // the dialog's own declaration hints (location/width/height from
        // openDialog, captured as WindowHints) win over the tall-narrow
        // kind default (26x20, right edge — combat is ~190x288 px).
        let (hx, hy, w, h) = crate::core::placement::ephemeral_placement(
            self.ui_state.window_hints.get(dialog_id).map(|v| v.as_slice()),
            (26, 20),
            crate::core::placement::PlacementAnchor::RightEdge,
            (terminal_width, terminal_height),
        );
        // A saved per-id position still beats the hint (user geometry is
        // always first in the placement precedence).
        let (x, y) = if let Some(saved) = self.saved_dialog_positions.dialogs.get(dialog_id) {
            (
                saved.x.min(terminal_width.saturating_sub(w)),
                saved.y.min(terminal_height.saturating_sub(h)),
            )
        } else {
            (hx, hy)
        };

        let window = WindowState {
            name: window_name.clone(),
            widget_type: WidgetType::DialogPanel,
            content: WindowContent::DialogPanel {
                dialog_id: dialog_id.to_string(),
            },
            position: WindowPosition {
                x: crate::data::geometry::Col::new(x),
                y: crate::data::geometry::Row::new(y),
                width: crate::data::geometry::Width::new(w),
                height: crate::data::geometry::Height::new(h),
            },
            visible: true,
            focused: false,
            content_align: None,
            ephemeral: true,
        };
        self.ui_state.set_window(window_name.clone(), window);
        self.ui_state.ephemeral_windows.insert(window_name);
        self.add_system_message(&format!("Opened {} panel", title));
        self.needs_render = true;
    }

    /// Apply a user show/hide choice from the "known windows" list: record
    /// the policy on the offer and create or close the corresponding
    /// window. Currently wires container offers to ephemeral container
    /// windows; dialog/stream offers record policy for now (their window
    /// wiring lands as those consumption paths migrate).
    /// U3: show or hide a known window by NAME (from enumerate_known_windows),
    /// with no offer registry. Dispatches on where the window lives:
    /// - a persistent LAYOUT window (incl. bound streams / dialog panels):
    ///   flip visibility via show_window / hide_window.
    /// - a session-only EPHEMERAL window (container, ad-hoc panel): create
    ///   or remove the runtime window.
    /// - the bank-style POPUP: materialize from / clear active_dialog.
    pub fn set_known_window_shown(
        &mut self,
        name: &str,
        shown: bool,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        // Persistent layout window? (streams, dialog panels, plain widgets)
        if let Some(win) = self.layout.windows.iter().find(|w| w.name() == name) {
            // Keep the dialog-popup allow-set in sync — but ONLY for dialogs
            // that render as a transient popup (bank/shop). A DialogPanel
            // widget (combat, UberBar) renders the dialog store IN THE PANEL;
            // adding its id here would ALSO pop it up as an active_dialog,
            // producing a duplicate (an empty panel + a populated popup, or
            // vice-versa). So a panel-bound dialog must stay out of the set.
            let is_dialog_panel =
                matches!(win, crate::config::WindowDef::DialogPanel { .. });
            if let Some(crate::config::WindowBinding::Dialog(id)) = win.base().binding.clone() {
                if !is_dialog_panel {
                    if shown {
                        self.ui_state.shown_dialog_ids.insert(id);
                    } else {
                        self.ui_state.shown_dialog_ids.remove(&id);
                    }
                }
            }
            if shown {
                self.show_window(name, terminal_width, terminal_height);
            } else {
                self.hide_window(name);
            }
            return;
        }

        // Ephemeral runtime window already present (container/panel): just
        // toggle its presence.
        if let Some(win) = self.ui_state.windows.get(name) {
            // A container also drops out of the session "shown" set so it
            // doesn't re-open on the next sighting.
            let container_title = match &win.content {
                crate::data::WindowContent::Container { container_title } => {
                    Some(container_title.clone())
                }
                _ => None,
            };
            if !shown {
                self.ui_state.remove_window(name);
                self.ui_state.ephemeral_windows.remove(name);
                if let Some(t) = container_title {
                    self.ui_state.shown_container_titles.remove(&t);
                }
                self.needs_render = true;
            }
            // (Re-showing an already-present ephemeral window is a no-op.)
            return;
        }

        // Not yet materialized. Conjure when shown, MOST-SPECIFIC FIRST.
        if shown {
            // A real widget template ALWAYS wins. It is the least ambiguous
            // meaning of a name, so it must beat both the generic dialog panel
            // and the container branches below. This matters because a
            // deleted-then-reshown widget whose id the game ALSO feeds as a
            // resident dialog (minivitals, expr, encum, Buffs, injuries,
            // stance, status indicators, ...) leaves an entry in the always-on
            // dialog store; without template-first, that store entry would
            // resurrect the widget as a bare `panel_<id>` instead of the real
            // widget. (A future container title colliding with a template name
            // would be the same trap one branch down — template-first closes
            // both.) show_window adds the def from the template + materializes.
            if crate::core::local_catalog::seed(name).is_some() {
                self.show_window(name, terminal_width, terminal_height);
                return;
            }
            // A remembered binding from discovery memory
            // (window_registry.toml): conjure the bound PERSISTENT window
            // exactly as a live discovery would, then show it. This
            // outranks the ephemeral dialog-store branch below — live-test
            // finding (Nisugi, bank): delete + reshow used to fall to the
            // store branch and produce a different, session-only
            // `panel_<id>` window instead of the persistent bound one the
            // first Show created.
            if let Some(entry) = self
                .window_registry
                .bindings
                .iter()
                .find(|b| b.id == name)
                .cloned()
            {
                let binding = match entry.kind.as_str() {
                    "stream" => Some(crate::config::WindowBinding::Stream(entry.id.clone())),
                    "dialog" => Some(crate::config::WindowBinding::Dialog(entry.id.clone())),
                    _ => None,
                };
                if let Some(binding) = binding {
                    let template = Self::seed_template_for(&binding);
                    if let Some(win_name) =
                        self.layout.register_discovered_window(binding, &template)
                    {
                        if !entry.title.is_empty() {
                            if let Some(def) = self
                                .layout
                                .windows
                                .iter_mut()
                                .find(|w| w.name() == win_name)
                            {
                                def.base_mut().title = Some(entry.title.clone());
                            }
                        }
                        self.apply_declared_size_hint(&win_name, &entry.id);
                        self.mark_layout_modified();
                        self.show_window(&win_name, terminal_width, terminal_height);
                        self.needs_render = true;
                    }
                    return;
                }
            }
            // A dialog-store entry the registry does NOT remember (rare:
            // discoveries record into the registry) → the legacy GENERIC
            // ephemeral panel rendered from the store by id.
            if self.ui_state.dialog_store.contains_key(name) {
                self.create_dialog_panel_window(name, name, terminal_width, terminal_height);
                self.needs_render = true;
                return;
            }
            // A sighted registry container (window name is title-derived) →
            // remember the opt-in and open it.
            let container_title = self
                .game_state
                .objects
                .containers()
                .find(|c| c.title.replace(' ', "_").to_lowercase() == name)
                .map(|c| c.title.clone());
            if let Some(title) = container_title {
                self.ui_state.shown_container_titles.insert(title.clone());
                self.create_ephemeral_container_window(&title, terminal_width, terminal_height);
                self.needs_render = true;
            }
        }
    }

    /// Realize game-offered windows after a batch of server messages, once
    /// terminal dimensions are known (called from every frontend's tick).
    /// Replaces the old all-or-nothing container discovery mode: a sighted
    /// container auto-(re)opens only if its offer policy says Shown, and
    /// openDialog-templated widgets queued by the message processor get
    /// added to the layout.
    pub fn realize_offered_windows(&mut self, terminal_width: u16, terminal_height: u16) {
        // Drain game-window discoveries the message processor observed into
        // the layout (it can't reach the layout itself). U3: streams and
        // resident dialog panels become bound, Hidden-by-default layout
        // entries — known forever, not auto-shown. Idempotent per binding.
        let discoveries: Vec<crate::data::WindowDiscovery> =
            self.ui_state.pending_window_discoveries.drain(..).collect();
        for d in discoveries {
            self.register_window_discovery(d);
        }

        // Redesign Phase 4d — expose = show. Rules (owner-decided, wire
        // verified): a KNOWN window's Show flag is the permission — Hidden
        // blocks the expose (Hidden already means "suppress game
        // auto-spawn", the U3 unified rule); an id arriving via expose for
        // the FIRST time registers bound and shows (default allowed). A
        // popup currently active under this id stays the popup path's
        // business (bank: openDialog popup + exposeDialog ride together;
        // U5's persistent bank row remains deferred).
        let exposes: Vec<(String, String)> =
            self.ui_state.pending_exposes.drain(..).collect();
        for (kind, id) in exposes {
            if self
                .ui_state
                .active_dialog
                .as_ref()
                .is_some_and(|dialog| dialog.id == id)
            {
                continue;
            }
            if self.layout.has_window_bound_to(&id) {
                let target = self
                    .layout
                    .windows
                    .iter()
                    .find(|w| w.base().binding.as_ref().is_some_and(|b| b.id() == id))
                    .map(|w| (w.name().to_string(), w.base().visibility));
                if let Some((name, visibility)) = target {
                    use crate::config::WindowVisibility;
                    if visibility == WindowVisibility::Hidden {
                        tracing::debug!("expose {kind} {id}: blocked (user hid the window)");
                        continue;
                    }
                    if !self.ui_state.windows.contains_key(&name) {
                        self.show_window(&name, terminal_width, terminal_height);
                        self.needs_render = true;
                    }
                    self.ui_state.expose_shown_ids.insert(id);
                }
                continue;
            }
            // First arrival via expose. Streams (charprofile-class, the
            // wire-verified case) register bound and show — the expose
            // default. Unknown DIALOG ids stay with the popup machinery
            // for now: bank's exposeDialog rides beside its popup flow,
            // and registering it as a panel would duplicate the popup
            // (its persistent hidden-unless-exposed row is U5's save-attr
            // work).
            if kind == "stream" {
                use crate::config::WindowBinding;
                let binding = WindowBinding::Stream(id.clone());
                let template = Self::seed_template_for(&binding);
                if let Some(name) =
                    self.layout.register_discovered_window(binding, &template)
                {
                    self.apply_declared_size_hint(&name, &id);
                    self.mark_layout_modified();
                    self.show_window(&name, terminal_width, terminal_height);
                    self.ui_state.expose_shown_ids.insert(id);
                    self.needs_render = true;
                }
            } else {
                tracing::debug!("expose {kind} {id}: unbound dialog left to the popup path");
            }
        }

        // The matching dismissals: dematerialize exactly the windows an
        // expose showed this session — WITHOUT flipping their persisted
        // visibility to Hidden (Hidden is the user's block lever; a game
        // dismissal must not eat the NEXT expose — bank re-exposes every
        // visit). Never-opened ids the game closes defensively
        // (withdraw/deposit) no-op here.
        let closes: Vec<String> = self.ui_state.pending_expose_closes.drain(..).collect();
        for id in closes {
            if !self.ui_state.expose_shown_ids.remove(&id) {
                continue;
            }
            let name = self
                .layout
                .windows
                .iter()
                .find(|w| w.base().binding.as_ref().is_some_and(|b| b.id() == id))
                .map(|w| w.name().to_string());
            if let Some(name) = name {
                self.ui_state.remove_window(&name);
                self.needs_render = true;
            }
        }

        if let Some((_id, title)) = self.message_processor.newly_registered_container.take() {
            // U3: a sighted container (re)opens only if the user opted it in
            // this session (via the Windows list). Ephemeral, wiped on relog.
            if self.ui_state.shown_container_titles.contains(&title) {
                self.create_ephemeral_container_window(&title, terminal_width, terminal_height);
            }
        }
        self.process_pending_window_additions(terminal_width, terminal_height);
    }

    /// Register a game-window discovery into the layout as a bound entry.
    /// Streams and resident dialog panels become persistent Hidden layout
    /// windows (known forever); hidden-until-shown is the universal
    /// default. No-op if a window is already bound to this id.
    /// Apply the game's DECLARED size (openDialog/streamWindow
    /// width/height px, captured as WindowHints) to a newly created bound
    /// window. CREATION TIME ONLY — the precedence is saved local
    /// geometry → declared size → default, and applying only at creation
    /// means user resizes and saved layouts always win afterward and a
    /// re-sent hint can never clobber them. Generic views only: dedicated
    /// widgets (expr, minivitals, …) keep their curated sizes — the
    /// binding is the game's, the presentation is ours.
    /// The declared (width, height) px for a game id from THIS session's
    /// hints; components <= 1 are treated as unset.
    fn declared_size_from_hints(&self, game_id: &str) -> Option<(f32, f32)> {
        let hints = self.ui_state.window_hints.get(game_id)?;
        let dim = |name: &str| {
            hints
                .iter()
                .find(|(k, _)| k == name)
                .and_then(|(_, v)| v.parse::<f32>().ok())
                .filter(|v| *v > 1.0)
                .unwrap_or(0.0)
        };
        let (w, h) = (dim("width"), dim("height"));
        (w > 1.0 || h > 1.0).then_some((w, h))
    }

    fn apply_declared_size_hint(&mut self, window_name: &str, game_id: &str) {
        // Session hints first; the cross-session registry memory second
        // (a fresh session's conjure still gets the declared shape).
        let declared = self.declared_size_from_hints(game_id).or_else(|| {
            self.window_registry
                .bindings
                .iter()
                .find(|b| b.id == game_id)
                .and_then(|b| b.declared_size)
        });
        let Some((wpx, hpx)) = declared else {
            return;
        };
        let (w, h) = (
            (wpx > 1.0).then_some(wpx),
            (hpx > 1.0).then_some(hpx),
        );
        let Some(def) = self
            .layout
            .windows
            .iter_mut()
            .find(|def| def.name() == window_name)
        else {
            return;
        };
        if !matches!(def.widget_type(), "text" | "dialogpanel" | "container") {
            return;
        }
        let base = def.base_mut();
        if let Some(hpx) = h {
            // Content px → cells (~16px rows) + title-bar row.
            let rows = ((hpx / 16.0).ceil() as u16 + 1).clamp(3, 80);
            base.rows = crate::data::geometry::Height::new(rows);
        }
        if let Some(wpx) = w {
            let cols = ((wpx / 8.0).ceil() as u16 + 2).clamp(12, 240);
            base.cols = crate::data::geometry::Width::new(cols);
        }
        self.mark_layout_modified();
    }

    /// The seed key a bound window is created from, via the presentation
    /// resolver (redesign Phase 3): a dedicated view's widget template,
    /// or the generic view for the binding's kind.
    fn seed_template_for(binding: &crate::config::WindowBinding) -> String {
        use crate::core::view_resolver::resolve_view;
        use crate::data::view_kind::ViewKind;
        match resolve_view(binding, None) {
            ViewKind::Dedicated(key) => key,
            ViewKind::Text => "text_custom".to_string(),
            ViewKind::DialogPanel => "dialogpanel".to_string(),
            ViewKind::Container => "container".to_string(),
        }
    }

    fn register_window_discovery(&mut self, d: crate::data::WindowDiscovery) {
        use crate::config::{WindowBinding, WindowVisibility, WindowDef};
        use crate::data::WindowDiscoveryKind;

        // Discovery memory (Phase 1b): every sighting is recorded — even
        // for ids that already have a bound window (a re-declaration can
        // carry a better title) and for popup dialogs that never become
        // layout windows. The write is deferred to the frontend-driven
        // autosave tick (same driver as the layout), so pure-core tests
        // never touch the filesystem and a failed write never disturbs
        // the session.
        let registry_kind = match d.kind {
            WindowDiscoveryKind::Stream => "stream",
            WindowDiscoveryKind::DialogPanel | WindowDiscoveryKind::DialogPopup => "dialog",
        };
        if self.window_registry.record(registry_kind, &d.id, &d.title) {
            self.window_registry_dirty = true;
        }
        if let Some((w, h)) = self.declared_size_from_hints(&d.id) {
            if self.window_registry.record_declared_size(&d.id, (w, h)) {
                self.window_registry_dirty = true;
            }
        }

        // Windows the user conjured from the catalog THIS session aren't
        // binding-tagged until the next load (backfill runs at load time),
        // so the game's own declaration would create a bound DUPLICATE
        // under the same name (fresh-profile live test: two 'Room' rows +
        // a GUI widget-id clash). Backfill is idempotent and cheap — tag now
        // so has_window_bound_to adopts instead of duplicating.
        if crate::config::Layout::backfill_bindings(&mut self.layout) > 0 {
            self.mark_layout_modified();
        }

        if self.layout.has_window_bound_to(&d.id) {
            return;
        }

        // ADOPT an existing window instead of creating a duplicate:
        // - a stream whose id a text/inventory window already subscribes to
        //   (the default layout ships thoughts/speech/society/inv/... windows
        //   that predate binding — tag them so the discovery doesn't make a
        //   second "thoughts" beside the shipped "Thoughts").
        if d.kind == WindowDiscoveryKind::Stream {
            // A single-stream window already showing this id: ADOPT it (tag
            // the binding) so it becomes the one true home for the stream.
            let single = self.layout.windows.iter_mut().find(|w| match w {
                WindowDef::Text { data, .. } => data.streams.iter().any(|s| s == &d.id),
                WindowDef::Inventory { data, .. } | WindowDef::Reserve { data, .. } => {
                    data.streams.iter().any(|s| s == &d.id)
                }
                _ => false,
            });
            if let Some(w) = single {
                if w.base().binding.is_none() {
                    w.base_mut().binding = Some(WindowBinding::Stream(d.id.clone()));
                    self.mark_layout_modified();
                }
                return;
            }
            // A MULTI-stream window (tabbedtext) already routes this stream
            // through a tab: don't create a duplicate, and don't bind the
            // whole window (it carries many streams). The tab handles it.
            let in_tab = self.layout.windows.iter().any(|w| match w {
                WindowDef::TabbedText { data, .. } => data.tabs.iter().any(|t| {
                    t.streams.iter().any(|s| s == &d.id)
                        || t.stream.as_deref() == Some(d.id.as_str())
                }),
                _ => false,
            });
            if in_tab {
                return;
            }
        }

        // Pick the seed view + binding for this discovery kind through the
        // presentation resolver (redesign Phase 3: the discovery base
        // comes from resolve_view, not scattered id-maps — a dedicated
        // view resolves to its widget's seed, everything else to the
        // generic view for its kind).
        use crate::core::view_resolver::resolve_view;
        use crate::data::view_kind::ViewKind;
        let binding = match d.kind {
            WindowDiscoveryKind::Stream => WindowBinding::Stream(d.id.clone()),
            WindowDiscoveryKind::DialogPanel => WindowBinding::Dialog(d.id.clone()),
            // Popups (bank) aren't layout widgets; they're handled by the
            // active_dialog popup path. Skip layout registration for now
            // (U5 gives bank a first-class row).
            WindowDiscoveryKind::DialogPopup => return,
        };
        let template = Self::seed_template_for(&binding);

        if let Some(name) = self.layout.register_discovered_window(binding, &template) {
            // A new discovery changes the layout — mark it so the autosave
            // (or .savelayout) persists it, making the window known forever.
            self.mark_layout_modified();
            // Size from the game's own declaration when it sent one.
            self.apply_declared_size_hint(&name, &d.id);
            // Set a friendly title + Shown/Hidden default.
            if let Some(def) = self.layout.windows.iter_mut().find(|w| w.name() == name) {
                if !d.title.is_empty() {
                    def.base_mut().title = Some(d.title.clone());
                }
                // Freshly discovered windows are Hidden (U3:
                // hidden-by-default); this is where a future policy
                // (e.g. resident streams shown) would flip it.
                def.base_mut().visibility = WindowVisibility::Hidden;
                // Wire the widget to its game feed by id.
                match (d.kind, def) {
                    // A stream text window subscribes to the stream id.
                    (
                        WindowDiscoveryKind::Stream,
                        crate::config::WindowDef::Text { data, .. },
                    ) => {
                        if !data.streams.contains(&d.id) {
                            data.streams.push(d.id.clone());
                        }
                    }
                    // A dialog panel renders from the dialog store by id.
                    (
                        WindowDiscoveryKind::DialogPanel,
                        crate::config::WindowDef::DialogPanel { data, .. },
                    ) => {
                        data.dialog_id = d.id.clone();
                    }
                    _ => {}
                }
            }
        }
    }

    /// Close all ephemeral container windows
    pub fn close_all_ephemeral_windows(&mut self) {
        let names: Vec<_> = self.ui_state.ephemeral_windows.iter().cloned().collect();
        let count = names.len();

        for name in names {
            self.ui_state.remove_window(&name);
        }
        self.ui_state.ephemeral_windows.clear();

        if count > 0 {
            self.add_system_message(&format!("Closed {} container window(s)", count));
            self.needs_render = true;
        } else {
            self.add_system_message("No container windows to close");
        }
    }

    /// Close ephemeral container window by title (case-insensitive partial match)
    pub fn close_ephemeral_window_by_title(&mut self, title: &str) {
        // Window names are built as lowercase-with-underscores (see
        // create_ephemeral_container_window), so normalize the needle the
        // same way or multi-word titles like "My Pack" never match.
        let title_lower = title.to_lowercase().replace(' ', "_");

        // Find matching ephemeral windows
        let matches: Vec<_> = self
            .ui_state
            .ephemeral_windows
            .iter()
            .filter(|name| name.to_lowercase().contains(&title_lower))
            .cloned()
            .collect();

        if matches.is_empty() {
            self.add_system_message(&format!("No container window matching '{}'", title));
            return;
        }

        for name in &matches {
            self.ui_state.remove_window(name);
            self.ui_state.ephemeral_windows.remove(name);
        }

        self.add_system_message(&format!("Closed {} container window(s)", matches.len()));
        self.needs_render = true;
    }

    /// Add a new window
    pub(super) fn add_window(
        &mut self,
        name: &str,
        widget_type_str: &str,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) {
        use crate::config::WindowDef;
        use crate::data::{
            CompassData, CountdownData, IndicatorData, PerceptionData, ProgressData, RoomContent,
            TextContent, WidgetType, WindowContent, WindowPosition, WindowState,
        };

        // Check if window already exists
        if self.ui_state.windows.contains_key(name) {
            self.add_system_message(&format!("Window '{}' already exists", name));
            return;
        }

        // Parse widget type
        let widget_type = match WidgetType::try_from_str(widget_type_str) {
            Some(wt) => wt,
            None => {
                self.add_system_message(&format!("Unknown widget type: {}", widget_type_str));
                self.add_system_message(&format!("Valid types: {}", WidgetType::VALID_TYPES.join(", ")));
                return;
            }
        };

        // Create window content based on type
        let content = match widget_type {
            WidgetType::Text => WindowContent::Text(TextContent::new(name, 1000)),
            WidgetType::Progress => WindowContent::Progress(ProgressData {
                value: 100,
                max: 100,
                label: name.to_string(),
                color: None,
                progress_id: name.to_string(),
                numbers_only: false,
                current_only: false,
            }),
            WidgetType::Countdown => WindowContent::Countdown(CountdownData {
                end_time: 0,
                label: name.to_string(),
                countdown_id: name.to_string(),
                color: None,
                show_when_zero: false,
            }),
            WidgetType::Map => WindowContent::Map(crate::data::MapData::default()),
            WidgetType::Compass => WindowContent::Compass(CompassData {
                directions: Vec::new(),
            }),
            WidgetType::InjuryDoll => WindowContent::InjuryDoll(InjuryDollData::new()),
            WidgetType::Hand => WindowContent::Hand {
                item: None,
                link: None,
            },
            WidgetType::Room => WindowContent::Room(RoomContent {
                name: String::new(),
                description: Vec::new(),
                exits: Vec::new(),
                players: Vec::new(),
                objects: Vec::new(),
            }),
            WidgetType::Indicator => WindowContent::Indicator(IndicatorData {
                indicator_id: name.to_string(),
                active: false,
                color: None,
            }),
            WidgetType::Performance => WindowContent::Performance,
            WidgetType::Perception => WindowContent::Perception(PerceptionData {
                entries: Vec::new(),
                last_update: 0,
                generation: 0,
            }),
            WidgetType::CommandInput => WindowContent::CommandInput {
                text: String::new(),
                cursor: 0,
                history: Vec::new(),
                history_index: None,
            },
            WidgetType::Inventory => {
                let mut content = TextContent::new(name, 0);
                content.streams = vec!["inv".to_string()];
                WindowContent::Inventory(content)
            }
            WidgetType::Reserve => {
                let mut content = TextContent::new(name, 0);
                content.streams = vec!["reserve".to_string()];
                WindowContent::Reserve(content)
            }
            WidgetType::Spells => {
                let mut content = TextContent::new(name, 0);
                content.streams = vec!["Spells".to_string()];
                WindowContent::Spells(content)
            }
            WidgetType::Dashboard => WindowContent::Dashboard {
                indicators: Vec::new(),
            },
            WidgetType::ActiveEffects => WindowContent::ActiveEffects(crate::data::ActiveEffectsContent {
                category: "Unknown".to_string(),
                effects: Vec::new(),
                generation: 0,
            }),
            WidgetType::Targets => WindowContent::Targets,
            WidgetType::Players => WindowContent::Players,
            WidgetType::Items => WindowContent::Items,
            WidgetType::Container => WindowContent::Container {
                container_title: String::new(),
            },
            WidgetType::Experience => WindowContent::Experience,
            WidgetType::GS4Experience => WindowContent::GS4Experience,
            WidgetType::Encumbrance => WindowContent::Encumbrance,
            WidgetType::MiniVitals => WindowContent::MiniVitals,
            WidgetType::Betrayer => WindowContent::Betrayer,
            // A dot-command-created hotkeybar binds to the bar with the
            // same name as the window
            WidgetType::Hotkeybar => WindowContent::Hotkeybar {
                bar: name.to_string(),
            },
            WidgetType::WebUi => WindowContent::WebUi(
                crate::data::webui::WebUiPanelContent::new(name, name),
            ),
            // Name-based creation path: bind the panel to the window name.
            WidgetType::DialogPanel => WindowContent::DialogPanel {
                dialog_id: name.to_string(),
            },
            _ => WindowContent::Empty,
        };

        if widget_type == WidgetType::Performance {
            // Restart peaks/spike log so they describe this viewing session.
            self.perf_stats.reset_peaks();
        }

        // Create window state
        let window = WindowState {
            name: name.to_string(),
            widget_type: widget_type.clone(),
            content,
            position: WindowPosition {
                x: crate::data::geometry::Col::new(x),
                y: crate::data::geometry::Row::new(y),
                width: crate::data::geometry::Width::new(width),
                height: crate::data::geometry::Height::new(height),
            },
            visible: true,
            content_align: None,
            focused: false,
            ephemeral: false,
        };

        // Add to UI state
        self.ui_state.set_window(name.to_string(), window);

        // Create window definition for layout
        use crate::config::{BorderSides, TextWidgetData, WindowBase};

        let base = WindowBase {
            name: name.to_string(),
            row: crate::data::geometry::Row::new(y),
            col: crate::data::geometry::Col::new(x),
            rows: crate::data::geometry::Height::new(height),
            cols: crate::data::geometry::Width::new(width),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: true,
            title: Some(name.to_string()),
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

        // Persist the window with its REAL widget type. Previously only
        // text/room/command_input/webui were handled and every other type fell
        // back to WindowDef::Text, so progress/countdown/compass/indicator/hand
        // windows reloaded as empty text boxes (and landed in the wrong resize
        // bucket). WindowDef::blank builds the correct variant for each type.
        //
        // `widget_type_str` was already validated by WidgetType::try_from_str
        // near the top of this function, so blank() cannot return None here;
        // fall back to a plain text def defensively rather than panicking.
        let fallback_base = base.clone();
        let window_def = WindowDef::blank(widget_type_str, base).unwrap_or_else(|| {
            WindowDef::Text {
                base: fallback_base,
                data: TextWidgetData {
                    streams: vec![],
                    buffer_size: 10_000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }
        });

        // Add to layout at the front (so new windows appear on top)
        self.layout.windows.insert(0, window_def);

        self.add_system_message(&format!(
            "Window '{}' added ({}x{} at {},{}) - type: {}",
            name, width, height, x, y, widget_type_str
        ));
        self.needs_render = true;

        // Update text stream subscriber map (new window may have stream subscriptions)
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);

        // Clear inventory cache if this is an inventory window to force initial render
        if widget_type == WidgetType::Inventory {
            self.message_processor.clear_inventory_cache();
        }

        // Populate spells window from buffer if this is a spells window
        // Spells are sent once at login, so we populate immediately from buffer
        if widget_type == WidgetType::Spells {
            if let Some(window) = self.ui_state.windows.get_mut(name) {
                if let WindowContent::Spells(ref mut content) = window.content {
                    self.message_processor.populate_spells_window(content);
                }
            }
        }
    }

    /// Create (or reuse) a window bound to a Lich WebUI page. Returns the
    /// window name (`webui:<page_id>`). The size hint is the descriptor's
    /// preferred content size in CSS pixels, mapped to core layout cells.
    pub fn add_webui_window(
        &mut self,
        page_id: &str,
        title: &str,
        size_hint: Option<[f32; 2]>,
        kind: Option<String>,
    ) -> String {
        use crate::data::{WidgetType, WindowContent, WindowPosition, WindowState};

        let name = format!("webui:{}", page_id);
        if self.ui_state.windows.contains_key(&name) {
            return name;
        }

        // Rough CSS-px -> layout-cell mapping (8x16 px cells), floored to a
        // usable minimum so tiny hints still get a visible window.
        let (width, height) = match size_hint {
            Some([w, h]) => (
                ((w / 8.0).ceil() as u16).clamp(20, 120),
                ((h / 16.0).ceil() as u16).clamp(4, 60),
            ),
            None => (40, 12),
        };

        let mut content = crate::data::webui::WebUiPanelContent::new(page_id, title);
        content.kind = kind;
        let window = WindowState {
            name: name.clone(),
            widget_type: WidgetType::WebUi,
            content: WindowContent::WebUi(content),
            position: WindowPosition {
                x: crate::data::geometry::Col::new(0),
                y: crate::data::geometry::Row::new(0),
                width: crate::data::geometry::Width::new(width),
                height: crate::data::geometry::Height::new(height),
            },
            visible: true,
            content_align: None,
            focused: false,
            ephemeral: false,
        };
        self.ui_state.set_window(name.clone(), window);

        let base = crate::config::WindowBase {
            name: name.clone(),
            row: crate::data::geometry::Row::new(0),
            col: crate::data::geometry::Col::new(0),
            rows: crate::data::geometry::Height::new(height),
            cols: crate::data::geometry::Width::new(width),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: crate::config::BorderSides::default(),
            border_color: None,
            show_title: true,
            title: Some(title.to_string()),
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
        self.layout.windows.insert(
            0,
            crate::config::WindowDef::WebUi {
                base,
                data: crate::config::WebUiWidgetData {
                    page: page_id.to_string(),
                },
            },
        );
        self.needs_render = true;
        name
    }

    /// Rename a window's title
    pub(super) fn rename_window(&mut self, window_name: &str, new_title: &str) {
        // Update in layout definition
        if let Some(window_def) = self
            .layout
            .windows
            .iter_mut()
            .find(|w| w.name() == window_name)
        {
            window_def.base_mut().title = Some(new_title.to_string());
            self.add_system_message(&format!(
                "Window '{}' renamed to '{}'",
                window_name, new_title
            ));
            self.needs_render = true;
        } else {
            self.add_system_message(&format!("Window '{}' not found", window_name));
        }
    }

    /// Set window border style and color
    pub(super) fn set_window_border(&mut self, window_name: &str, style: &str, color: Option<String>) {
        if let Some(window_def) = self
            .layout
            .windows
            .iter_mut()
            .find(|w| w.name() == window_name)
        {
            use crate::config::BorderSides;

            let style_lower = style.to_lowercase();
            let (new_show, new_sides) = match style_lower.as_str() {
                "none" => (false, window_def.base().border_sides.clone()),
                "all" => (true, BorderSides::default()),
                "top" => (
                    true,
                    BorderSides {
                        top: true,
                        bottom: false,
                        left: false,
                        right: false,
                    },
                ),
                "bottom" => (
                    true,
                    BorderSides {
                        top: false,
                        bottom: true,
                        left: false,
                        right: false,
                    },
                ),
                "left" => (
                    true,
                    BorderSides {
                        top: false,
                        bottom: false,
                        left: true,
                        right: false,
                    },
                ),
                "right" => (
                    true,
                    BorderSides {
                        top: false,
                        bottom: false,
                        left: false,
                        right: true,
                    },
                ),
                _ => {
                    self.add_system_message(&format!("Unknown border style: {}", style));
                    return;
                }
            };

            window_def
                .base_mut()
                .apply_border_configuration(new_show, new_sides);

            // Set border color if provided
            if let Some(c) = color {
                window_def.base_mut().border_color = Some(c);
            }

            // Recalculate and update window positions since rows/cols changed
            let width = self.layout.terminal_width.unwrap_or(80);
            let height = self.layout.terminal_height.unwrap_or(24);
            let positions = self.calculate_window_positions(width, height);
            for (name, position) in positions {
                if let Some(window) = self.ui_state.get_window_mut(&name) {
                    window.position = position;
                }
            }

            self.add_system_message(&format!("Border updated for window '{}'", window_name));
            self.mark_layout_modified();
            self.ui_state.needs_widget_reset = true;
            self.needs_render = true;
        } else {
            self.add_system_message(&format!("Window '{}' not found", window_name));
        }
    }

    /// Toggle transparent_background for all windows in the current layout.
    pub(super) fn toggle_transparent_background_all(&mut self) {
        if self.layout.windows.is_empty() {
            self.add_system_message("No windows found in layout");
            return;
        }

        let enable = self
            .layout
            .windows
            .iter()
            .any(|w| !w.base().transparent_background);

        for window_def in &mut self.layout.windows {
            window_def.base_mut().transparent_background = enable;
        }

        let status = if enable { "enabled" } else { "disabled" };
        self.add_system_message(&format!(
            "Background transparency {} for all windows",
            status
        ));
        self.needs_render = true;
    }

    /// Handle terminal resize
    pub fn resize(&mut self, width: u16, height: u16) {
        // Recalculate all window positions
        let positions = self.calculate_window_positions(width, height);

        // Update all window positions
        for (name, position) in positions {
            if let Some(window) = self.ui_state.get_window_mut(&name) {
                window.position = position;
            }
        }

        self.needs_render = true;
    }

    /// Calculate window positions based on layout and terminal size
    fn calculate_window_positions(
        &self,
        _width: u16,
        _height: u16,
    ) -> HashMap<String, WindowPosition> {
        let mut positions = HashMap::new();

        // Use exact layout file values (row, col, rows, cols) without any scaling
        // Windows may be offscreen if terminal is smaller than saved layout size
        // User can manually run .resize if they want to redistribute windows

        for window_def in &self.layout.windows {
            // Use exact position and size from layout
            let mut window_width = window_def.base().cols;
            let mut window_height = window_def.base().rows;

            // Apply min/max constraints from window settings
            if let Some(min_cols) = window_def.base().min_cols {
                if window_width.get() < min_cols {
                    tracing::debug!(
                        "Window '{}': enforcing min_cols={} (was {})",
                        window_def.name(),
                        min_cols,
                        window_width.get()
                    );
                    window_width = crate::data::geometry::Width::new(min_cols);
                }
            }
            if let Some(max_cols) = window_def.base().max_cols {
                if window_width.get() > max_cols {
                    tracing::debug!(
                        "Window '{}': enforcing max_cols={} (was {})",
                        window_def.name(),
                        max_cols,
                        window_width.get()
                    );
                    window_width = crate::data::geometry::Width::new(max_cols);
                }
            }
            if let Some(min_rows) = window_def.base().min_rows {
                if window_height.get() < min_rows {
                    tracing::debug!(
                        "Window '{}': enforcing min_rows={} (was {})",
                        window_def.name(),
                        min_rows,
                        window_height.get()
                    );
                    window_height = crate::data::geometry::Height::new(min_rows);
                }
            }
            if let Some(max_rows) = window_def.base().max_rows {
                if window_height.get() > max_rows {
                    tracing::debug!(
                        "Window '{}': enforcing max_rows={} (was {})",
                        window_def.name(),
                        max_rows,
                        window_height.get()
                    );
                    window_height = crate::data::geometry::Height::new(max_rows);
                }
            }

            tracing::debug!(
                "Window '{}': pos=({},{}) size={}x{}",
                window_def.name(),
                window_def.base().col.get(),
                window_def.base().row.get(),
                window_width.get(),
                window_height.get()
            );

            positions.insert(
                window_def.name().to_string(),
                WindowPosition {
                    x: window_def.base().col,
                    y: window_def.base().row,
                    width: window_width,
                    height: window_height,
                },
            );
        }

        positions
    }

    /// Build main menu for .menu command
    pub(super) fn build_main_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Colors >".to_string(),
                command: "__SUBMENU__colors".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Highlights >".to_string(),
                command: "__SUBMENU__highlights".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Keybinds >".to_string(),
                command: "__SUBMENU__keybinds".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Layouts >".to_string(),
                command: "__SUBMENU__layouts".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Settings".to_string(),
                command: ".settings".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Streams".to_string(),
                command: ".streams".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Windows >".to_string(),
                command: "__SUBMENU__windows".to_string(),
                disabled: false,
            },
            // First-class entry to the indicator template builder — reachable
            // even when every indicator is already placed (the Add/Edit
            // submenus' "Editor" leaf disappears once none are left to add).
            crate::data::ui_state::PopupMenuItem {
                text: "Indicators".to_string(),
                command: ".indicators".to_string(),
                disabled: false,
            },
            // Standalone editor for the target_list.status_abbrev map (shown
            // in the targets & players windows). Uses the action: command form
            // rather than a dot-command. In the GUI this opens the Window
            // Editor's Targets section where the same map is edited.
            crate::data::ui_state::PopupMenuItem {
                text: "Status Abbrevs".to_string(),
                command: "action:editstatusabbrev".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build colors submenu
    fn build_colors_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Add".to_string(),
                command: ".addcolor".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Browse".to_string(),
                command: ".colors".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Spells".to_string(),
                command: ".spellcolors".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Themes".to_string(),
                command: ".themes".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build highlights submenu
    pub(super) fn build_highlights_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Add".to_string(),
                command: ".addhighlight".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Browse".to_string(),
                command: ".highlights".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build keybinds submenu
    fn build_keybinds_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Add".to_string(),
                command: ".addkeybind".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Browse".to_string(),
                command: ".keybinds".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build themes submenu
    fn build_themes_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Browse themes".to_string(),
                command: ".themes".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Edit theme".to_string(),
                command: ".edittheme".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build windows submenu. U6: "Show/Hide windows" is the primary
    /// manager (every known window, toggle each); Add creates new ones;
    /// Edit tweaks geometry/settings. ("Hide window" is subsumed by the
    /// Show/Hide list — you untick a row there.)
    pub fn build_windows_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Show/Hide windows >".to_string(),
                command: "menu:knownwindows".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Add window >".to_string(),
                command: "menu:addwindow".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Edit window >".to_string(),
                command: "menu:editwindow".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build layouts submenu
    pub fn build_layouts_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let mut items = Vec::new();

        // Get list of saved layouts
        match Config::list_layouts() {
            Ok(mut layouts) => {
                // Sort alphabetically for predictability
                layouts.sort();
                let page_size = 10;
                let mut page = 0;
                let mut count = 0;
                for layout_name in layouts {
                    if count > 0 && count % page_size == 0 {
                        page += 1;
                    }
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: if page == 0 {
                            layout_name.clone()
                        } else {
                            format!("{} (p{})", layout_name, page + 1)
                        },
                        command: format!("action:loadlayout:{}", layout_name),
                        disabled: false,
                    });
                    count += 1;
                }
                if items.is_empty() {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: "No layouts found".to_string(),
                        command: String::new(),
                        disabled: true,
                    });
                }
            }
            Err(err) => {
                // If we can't load layouts, show a disabled message with reason
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: format!("No layouts: {}", err),
                    command: String::new(),
                    disabled: true,
                });
            }
        }

        // Add a close entry for accessibility
        items.push(crate::data::ui_state::PopupMenuItem {
            text: "Close menu".to_string(),
            command: String::new(),
            disabled: true,
        });

        items
    }

    /// Build submenu based on category name
    pub fn build_submenu(&self, category: &str) -> Vec<crate::data::ui_state::PopupMenuItem> {
        match category {
            "colors" => self.build_colors_submenu(),
            "highlights" => self.build_highlights_submenu(),
            "keybinds" => self.build_keybinds_submenu(),
            "layouts" => self.build_layouts_submenu(),
            "themes" => self.build_themes_submenu(),
            "windows" => self.build_windows_submenu(),
            "knownwindows" => self.build_known_windows_menu(),
            _ => Vec::new(),
        }
    }

    /// Handle menu response from server
    fn handle_menu_response(&mut self, counter: &str, coords: &[(String, Option<String>)]) {
        // Look up the pending request
        let pending = match self.pending_menu_requests.remove(counter) {
            Some(p) => p,
            None => {
                tracing::warn!("Received menu response for unknown counter: {}", counter);
                return;
            }
        };

        tracing::info!(
            "Menu response for exist_id {} (noun: {}): {} coords",
            pending.exist_id,
            pending.noun,
            coords.len()
        );

        // Check if cmdlist is loaded
        let cmdlist = match &self.cmdlist {
            Some(list) => list,
            None => {
                tracing::warn!("Context menu received but cmdlist not loaded");
                self.answer_remote_menu_empty(&pending);
                return;
            }
        };

        // Group menu items by category
        let mut categories: HashMap<String, Vec<crate::data::ui_state::PopupMenuItem>> =
            HashMap::new();

        for (coord, secondary_noun) in coords {
            if let Some(cmd) = coord.strip_prefix("__direct__:") {
                let menu_text = secondary_noun
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(cmd)
                    .to_string();
                categories
                    .entry("0".to_string())
                    .or_default()
                    .push(crate::data::ui_state::PopupMenuItem {
                        text: menu_text,
                        command: cmd.to_string(),
                        disabled: false,
                    });
                continue;
            }

            if let Some(entry) = cmdlist.get(coord) {
                // Skip _dialog commands
                if entry.command.starts_with("_dialog") {
                    continue;
                }

                // Build menu text (remove @ and # placeholders, substitute %)
                let menu_text = Self::format_menu_text(&entry.menu, secondary_noun.as_deref());

                // Build command with placeholders substituted
                let command = CmdList::substitute_command(
                    &entry.command,
                    &pending.noun,
                    &pending.exist_id,
                    secondary_noun.as_deref(),
                );

                let category = if entry.menu_cat.is_empty() {
                    "0".to_string()
                } else {
                    entry.menu_cat.clone()
                };

                categories.entry(category).or_default().push(
                    crate::data::ui_state::PopupMenuItem {
                        text: menu_text,
                        command,
                        disabled: false,
                    },
                );
            }
        }

        if categories.is_empty() {
            tracing::warn!("No menu items available for this object");
            self.answer_remote_menu_empty(&pending);
            return;
        }

        // Build final menu with categories
        let mut menu_items = Vec::new();
        let mut sorted_cats: Vec<_> = categories.keys().cloned().collect();

        // Sort categories, but keep "0" at the end
        sorted_cats.sort_by(|a, b| {
            if a == "0" {
                std::cmp::Ordering::Greater
            } else if b == "0" {
                std::cmp::Ordering::Less
            } else {
                a.cmp(b)
            }
        });

        // Route the response to its origin. A remote client gets a flat
        // list (submenu categories become disabled section headers, since
        // a phone bottom sheet has no nested menus); a pick comes back as
        // an ordinary cmd. The local popup path below stays unchanged.
        if let crate::core::remote::MenuOrigin::Remote {
            client_id,
            request_id,
        } = pending.origin
        {
            let mut items = Vec::new();
            for cat in &sorted_cats {
                let cat_items = categories.get(cat).unwrap();
                if cat.contains('_') && cat != "0" {
                    items.push(crate::core::remote::RemoteMenuItem {
                        text: Self::format_category_label(cat),
                        command: String::new(),
                        disabled: true,
                    });
                }
                items.extend(cat_items.iter().map(|item| {
                    crate::core::remote::RemoteMenuItem {
                        text: item.text.clone(),
                        command: item.command.clone(),
                        disabled: item.disabled,
                    }
                }));
            }
            if let Some(remote) = self.message_processor.remote.as_mut() {
                remote.push_menu(client_id, request_id, pending.noun.clone(), items);
            }
            return;
        }

        // Add items to menu
        for cat in &sorted_cats {
            let items = categories.get(cat).unwrap();

            // Categories with _ become submenus (except "0")
            if cat.contains('_') && cat != "0" {
                // Cache submenu items
                self.menu_categories.insert(cat.clone(), items.clone());

                // Add submenu entry to main menu
                let cat_name = Self::format_category_label(cat);
                menu_items.push(crate::data::ui_state::PopupMenuItem {
                    text: format!("{} >", cat_name),
                    command: format!("__SUBMENU__{}", cat),
                    disabled: false,
                });
            } else {
                // Add items directly to main menu
                menu_items.extend(items.clone());
            }
        }

        // Create popup menu at last click position (or centered)
        let position = self.last_link_click_pos.unwrap_or((40, 12));

        self.ui_state.popup_menu =
            Some(crate::data::ui_state::PopupMenu::new(menu_items, position));
        self.ui_state.input_mode = crate::data::ui_state::InputMode::Menu;

        tracing::info!(
            "Created context menu with {} items",
            self.ui_state.popup_menu.as_ref().unwrap().get_items().len()
        );
    }

    /// When a menu request from a remote client can't produce items, still
    /// answer with an empty menu — otherwise the client's sheet waits
    /// forever. Local origins need nothing (no popup was opened).
    fn answer_remote_menu_empty(&mut self, pending: &PendingMenuRequest) {
        if let crate::core::remote::MenuOrigin::Remote {
            client_id,
            request_id,
        } = pending.origin
        {
            if let Some(remote) = self.message_processor.remote.as_mut() {
                remote.push_menu(client_id, request_id, pending.noun.clone(), Vec::new());
            }
        }
    }

    fn format_category_label(cat: &str) -> String {
        let mut label = cat.split('_').nth(1).unwrap_or(cat).replace('-', " ");
        if label.is_empty() {
            label = cat.to_string();
        }

        if label.is_empty() {
            return "Other".to_string();
        }

        let mut chars = label.chars();
        let first = chars.next().unwrap();
        let mut output = String::new();
        for c in first.to_uppercase() {
            output.push(c);
        }
        output.push_str(chars.as_str());
        output
    }

    /// Format menu text by removing @ and # placeholders and substituting %
    fn format_menu_text(menu: &str, secondary_noun: Option<&str>) -> String {
        let mut text = menu.to_string();

        // Substitute % with secondary noun
        if let Some(sec_noun) = secondary_noun {
            text = text.replace('%', sec_noun);
        }

        // Find first @ or #
        if let Some(pos) = text.find(['@', '#']) {
            let remaining = text[pos + 1..].trim();
            if remaining.is_empty() {
                // Placeholder at end - truncate
                text[..pos].trim_end().to_string()
            } else {
                // Placeholder in middle - remove it but keep rest
                let before = text[..pos].trim_end();
                let after = text[pos + 1..].trim_start();
                if before.is_empty() {
                    after.to_string()
                } else {
                    format!("{} {}", before, after)
                }
            }
        } else {
            text
        }
    }

    /// Request context menu for a link (local popup origin)
    /// Returns the _menu command to send to the server
    pub fn request_menu(
        &mut self,
        exist_id: String,
        noun: String,
        click_pos: (u16, u16),
    ) -> String {
        // Store click position for menu placement
        self.last_link_click_pos = Some(click_pos);
        self.request_menu_from(exist_id, noun, crate::core::remote::MenuOrigin::Local)
    }

    /// Resolve a link activation the way a local click does (mirrors the
    /// dispatch in frontend/tui/input.rs): `<d>` tags send their noun/text
    /// as a direct command, links with a coord resolve through cmdlist to
    /// a direct command (exits, default actions), and only plain links
    /// issue a `_menu` request (tagged with `origin` so the response
    /// routes back). Returns the command to send upstream, if any.
    pub fn resolve_link_activation(
        &mut self,
        link: &crate::data::LinkData,
        origin: crate::core::remote::MenuOrigin,
    ) -> Option<String> {
        if link.exist_id == crate::data::URL_LINK_SENTINEL {
            // Web link: frontends open the URL on their own side (browser on
            // desktop, window.open on the phone). Never a game command, and
            // never a _menu request for a fake exist id.
            tracing::debug!("URL link activation reached core (frontend opens it): {}", link.noun);
            return None;
        }

        if link.exist_id == "_direct_" {
            // <d> tag: the noun (cmd attribute) or text IS the command
            let command = if !link.noun.is_empty() {
                &link.noun
            } else {
                &link.text
            };
            tracing::info!("Executing <d> direct command: {}", command);
            return Some(format!("{}\n", command));
        }

        if let Some(ref coord) = link.coord {
            // Coord link: look up the default action in cmdlist and send
            // it directly - no menu round-trip (e.g. exits move you)
            let Some(ref cmdlist) = self.cmdlist else {
                tracing::warn!("Cmdlist not loaded - cannot resolve coord {}", coord);
                return None;
            };
            let Some(entry) = cmdlist.get(coord) else {
                tracing::warn!("Coord {} not found in cmdlist for '{}'", coord, link.text);
                return None;
            };
            let command = CmdList::substitute_command(
                &entry.command,
                &link.noun,
                &link.exist_id,
                None,
            );
            tracing::info!(
                "Executing cmdlist command for '{}' (coord: {}): {}",
                link.text,
                coord,
                command.trim()
            );
            return Some(format!("{}\n", command));
        }

        // Plain link: context menu round-trip
        Some(self.request_menu_from(link.exist_id.clone(), link.noun.clone(), origin))
    }

    /// Request context menu for a link on behalf of an origin (local UI or
    /// a remote web client). The `<menu>` response routes back to the
    /// origin in handle_menu_response.
    pub fn request_menu_from(
        &mut self,
        exist_id: String,
        noun: String,
        origin: crate::core::remote::MenuOrigin,
    ) -> String {
        // Increment counter
        self.menu_request_counter += 1;
        let counter = self.menu_request_counter;

        // Store pending request
        self.pending_menu_requests.insert(
            counter.to_string(),
            PendingMenuRequest {
                exist_id: exist_id.clone(),
                noun,
                origin,
            },
        );

        // Return command to send to server
        format!("_menu #{} {}\n", exist_id, counter)
    }

    /// How long the layout must be stable before the debounced autosave fires.
    pub const LAYOUT_AUTOSAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(3);

    /// Mark the layout changed and (re)arm the debounced autosave. Every
    /// mutation site routes through this or sets the flag via it, so window
    /// moves/resizes/edits persist a few seconds later instead of only on a
    /// clean quit.
    pub fn schedule_layout_autosave(&mut self) {
        self.layout_modified_since_save = true;
        self.layout_autosave_pending = Some(std::time::Instant::now());
    }

    /// Debounce driver: frontends call this from their event loop. Writes the
    /// profile auto-save slot once the layout has been stable for
    /// LAYOUT_AUTOSAVE_DEBOUNCE.
    pub fn tick_layout_autosave(&mut self) {
        // Discovery-memory flush rides the same driver (no debounce —
        // registrations are rare, one-shot, and tiny).
        if std::mem::take(&mut self.window_registry_dirty) {
            if let Err(e) = Config::save_window_registry(
                self.config.character.as_deref(),
                &self.window_registry,
            ) {
                tracing::warn!("could not write window_registry.toml: {e:#}");
            }
        }
        // Persist character state when it actually changed (rare — the parser
        // only bumps the generation on a real value change). No debounce: the
        // write is tiny and infrequent, same as the registry above.
        if self.game_state.character.generation != self.character_state_saved_gen {
            self.persist_character_state();
        }
        if let Some(changed_at) = self.layout_autosave_pending {
            if changed_at.elapsed() >= Self::LAYOUT_AUTOSAVE_DEBOUNCE {
                self.autosave_layout();
            }
        }
    }

    /// Mark layout as modified and show reminder (once per session)
    pub fn mark_layout_modified(&mut self) {
        self.schedule_layout_autosave();

        // Show reminder once per session
        if !self.save_reminder_shown {
            self.add_system_message(
                "Tip: Use .savelayout <name> to preserve changes as a reusable template",
            );
            self.save_reminder_shown = true;
        }
    }

    /// Adjust window rows for content-driven widgets (like Betrayer)
    /// Called after message processing when content count may have changed
    pub fn adjust_content_driven_windows(&mut self) {
        // Collect changes first to avoid borrow issues
        let mut changes: Vec<(String, u16)> = Vec::new();

        for window_def in &self.layout.windows {
            if let crate::config::WindowDef::Betrayer { base, data } = window_def {
                let bar_rows = 1u16;
                let item_rows = if data.show_items {
                    self.game_state.betrayer.items.len().max(1) as u16
                } else {
                    0
                };
                let border_rows = base.horizontal_border_units();
                let ideal_rows = bar_rows + item_rows + border_rows;

                // Clamp to min/max
                let new_rows = ideal_rows
                    .max(base.min_rows.unwrap_or(1))
                    .min(base.max_rows.unwrap_or(u16::MAX));

                if base.rows.get() != new_rows {
                    changes.push((base.name.clone(), new_rows));
                }
            }
        }

        // Apply changes to both layout and ui_state
        for (name, new_rows) in changes {
            // Update layout
            for window_def in &mut self.layout.windows {
                if window_def.name() == name {
                    if let crate::config::WindowDef::Betrayer { base, .. } = window_def {
                        base.rows = crate::data::geometry::Height::new(new_rows);
                    }
                    break;
                }
            }

            // Update ui_state window position height
            if let Some(window) = self.ui_state.windows.get_mut(&name) {
                window.position.height = crate::data::geometry::Height::new(new_rows);
            }

            // Mark modified but don't show the save reminder for auto-resizes
            self.schedule_layout_autosave();
            self.needs_render = true;
        }
    }

    /// Write the current layout to the profile auto-save slot
    /// (~/.vellum-fe/profiles/{character}/layout.toml) — the file startup
    /// reads. Called on quit, after .savelayout/.loadlayout, and by the
    /// debounced tick_layout_autosave, so layout changes stick even if the
    /// session ends without reaching the quit path (console X, crash).
    pub fn autosave_layout(&mut self) {
        self.layout_autosave_pending = None;
        let profile = self
            .config
            .character
            .clone()
            .unwrap_or_else(|| "default".to_string());

        let terminal_size = self
            .layout
            .terminal_width
            .and_then(|w| self.layout.terminal_height.map(|h| (w, h)));

        let base_layout_name = self
            .base_layout_name
            .clone()
            .or_else(|| self.layout.base_layout.clone())
            .unwrap_or_else(|| "default".to_string());

        self.layout.theme = Some(self.config.active_theme.clone());
        if let Err(e) = self
            .layout
            .save_auto(&profile, &base_layout_name, terminal_size)
        {
            tracing::warn!("Failed to autosave layout: {}", e);
        } else {
            tracing::info!(
                "Layout autosaved to profile '{}' (base: {}, terminal: {:?})",
                profile,
                base_layout_name,
                terminal_size
            );
        }
    }

    /// Save settings (layout, session cache) without exiting.
    /// Called by quit() and when intercepting game "quit" command.
    pub fn save_on_quit(&mut self) {
        // Show reminder if layout was modified
        if self.layout_modified_since_save {
            self.add_system_message(
                "Layout modified - use .savelayout <name> to create reusable template",
            );
        }

        // Autosave to profile layout.toml ("default" profile when no character is set)
        self.autosave_layout();

        let allowed_ids = self.allowed_quickbar_ids();
        let quickbars: HashMap<String, QuickbarData> = self
            .ui_state
            .quickbars
            .iter()
            .filter(|(id, _)| allowed_ids.contains(*id))
            .map(|(id, data)| (id.clone(), data.clone()))
            .collect();
        let quickbar_order: Vec<String> = self
            .ui_state
            .quickbar_order
            .iter()
            .filter(|id| allowed_ids.contains(*id))
            .cloned()
            .collect();
        let active_quickbar_id = self
            .ui_state
            .active_quickbar_id
            .as_ref()
            .and_then(|id| if allowed_ids.contains(id) { Some(id.clone()) } else { None });

        let character = (self.game_state.character != Default::default())
            .then(|| self.game_state.character.clone());
        let cache = crate::session_cache::SessionCache {
            quickbars,
            quickbar_order,
            active_quickbar_id,
            character,
        };
        if let Err(err) = crate::session_cache::save(self.config.character.as_deref(), &cache) {
            tracing::warn!("Failed to save session cache: {}", err);
        }
    }

    /// Persist just the character state into the session cache, preserving the
    /// rest (quickbars). Called by the autosave tick when the state changed.
    fn persist_character_state(&mut self) {
        let character = self.config.character.as_deref();
        // Merge into the existing cache so we don't clobber quickbars.
        let mut cache = crate::session_cache::load(character).unwrap_or_default();
        cache.character = (self.game_state.character != Default::default())
            .then(|| self.game_state.character.clone());
        match crate::session_cache::save(character, &cache) {
            Ok(()) => self.character_state_saved_gen = self.game_state.character.generation,
            Err(e) => tracing::warn!("could not persist character state: {e:#}"),
        }
    }

    /// Quit the application
    pub fn quit(&mut self) {
        self.save_on_quit();
        self.running = false;
    }

    /// Save configuration to disk
    pub fn save_config(&mut self) -> Result<()> {
        self.config.save(self.config.character.as_deref())?;
        // Update squelch patterns after config save (in case highlights changed)
        self.message_processor.update_squelch_patterns();
        // Update redirect cache after config save (in case highlights changed)
        self.message_processor.update_redirect_cache();
        Ok(())
    }

    // ===========================================================================================
    // Config Reload Methods
    // ===========================================================================================

    /// Reload all configuration from disk
    pub fn reload_all(&mut self) {
        self.add_system_message("Reloading all configuration...");
        self.reload_highlights();
        self.reload_keybinds();
        self.reload_hotbars();
        self.reload_settings();
        self.reload_colors();
        self.reload_layout();
        let emoji_count = crate::core::custom_emoji::reload();
        self.add_system_message(&format!(
            "All configuration reloaded ({emoji_count} custom emoji)"
        ));
    }

    /// Reload highlights from disk
    pub fn reload_highlights(&mut self) {
        tracing::debug!("reload_highlights: start");
        match crate::config::Config::load_highlights(self.config.character.as_deref()) {
            Ok(highlights) => {
                self.config.highlights = highlights;
                crate::config::Config::compile_highlight_patterns(&mut self.config.highlights);
                self.message_processor.apply_config(self.config.clone());
                tracing::debug!("reload_highlights: apply_config done");
                self.add_system_message("Highlights reloaded");
                tracing::debug!("reload_highlights: system message queued");
                let has_perception_window = self.ui_state.windows.values().any(|window| {
                    matches!(window.content, crate::data::WindowContent::Perception(_))
                });
                let is_dr_game = self
                    .config
                    .connection
                    .game
                    .as_deref()
                    .map(|game| game.to_ascii_lowercase().starts_with("dr"))
                    .unwrap_or(false);
                if has_perception_window && is_dr_game {
                    tracing::debug!("reload_highlights: before reload_spell_abbrevs");
                    match crate::spell_abbrevs::reload_spell_abbrevs() {
                        Ok(()) => {
                            self.add_system_message("Spell abbreviations reloaded");
                            tracing::debug!("reload_highlights: spell abbrevs reloaded");
                        }
                        Err(e) => self.add_system_message(&format!(
                            "Failed to reload spell abbreviations: {}",
                            e
                        )),
                    }
                    tracing::debug!("reload_highlights: after reload_spell_abbrevs");
                } else {
                    tracing::debug!(
                        "reload_highlights: skipping spell abbrevs (perception_window={}, dr_game={})",
                        has_perception_window,
                        is_dr_game
                    );
                }
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload highlights: {}", e));
            }
        }
        tracing::debug!("reload_highlights: end");
    }

    /// Reload keybinds from disk
    pub fn reload_keybinds(&mut self) {
        match crate::config::Config::load_keybinds(self.config.character.as_deref()) {
            Ok(keybinds) => {
                self.config.keybinds = keybinds;
                let character = self.config.character.clone();
                let character = character.as_deref();
                crate::config::Config::migrate_controller_shift_layers(character);
                self.config.controller_binds =
                    crate::config::Config::load_controller_binds(character).unwrap_or_default();
                self.config.controller_wheel =
                    crate::config::Config::load_controller_wheel(character).unwrap_or_default();
                self.config.controller_wheels =
                    crate::config::Config::load_controller_wheels(character).unwrap_or_default();
                self.config.controller_wheels_meta =
                    crate::config::Config::load_controller_wheels_meta(character)
                        .unwrap_or_default();
                self.config.touch_wheel =
                    crate::config::Config::load_touch_wheel(character).unwrap_or_default();
                self.config.controller_overlay =
                    crate::config::Config::load_controller_overlay(character).unwrap_or_default();
                self.config.controller_rumble =
                    crate::config::Config::load_controller_rumble(character).unwrap_or_default();
                self.config.controller_tuning =
                    crate::config::Config::load_controller_tuning(character).unwrap_or_default();
                // Rebuild keybind map for O(1) lookups (re-merges hotbar keys)
                self.rebuild_keybind_map();
                // Web clients render the wheel from a shipped copy.
                self.push_remote_wheels();
                self.warn_wheel_binding_conflicts();
                self.warn_wheel_span_conflicts();
                self.add_system_message("Keybinds reloaded");
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload keybinds: {}", e));
            }
        }
    }

    /// Reload hotbars from disk and re-register their hotkeys
    pub fn reload_hotbars(&mut self) {
        match crate::config::Config::load_hotbars(self.config.character.as_deref()) {
            Ok(hotbars) => {
                self.config.hotbars = hotbars;
                self.rebuild_keybind_map();
                for conflict in &self.hotbar_key_conflicts.clone() {
                    self.add_system_message(&format!(
                        "Hotbar key '{}' ({}:{}) not registered - already bound by {}",
                        conflict.key, conflict.bar, conflict.button, conflict.conflicts_with
                    ));
                }
                self.add_system_message("Hotbars reloaded");
                self.needs_render = true;
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload hotbars: {}", e));
            }
        }
    }

    /// Reload settings (UI, connection, sound) from disk
    pub fn reload_settings(&mut self) {
        let config_path = match crate::config::Config::config_path(self.config.character.as_deref()) {
            Ok(path) => path,
            Err(e) => {
                self.add_system_message(&format!("Failed to get config path: {}", e));
                return;
            }
        };

        match std::fs::read_to_string(&config_path) {
            Ok(contents) => {
                match toml::from_str::<crate::config::Config>(&contents) {
                    Ok(new_config) => {
                        // Update only the settings sections, preserve character name and runtime state
                        self.config.connection = new_config.connection;
                        self.config.ui = new_config.ui;
                        self.config.sound = new_config.sound;
                        self.config.event_patterns = new_config.event_patterns;
                        self.parser
                            .update_event_patterns(self.config.event_patterns.clone());
                        self.message_processor.apply_config(self.config.clone());
                        self.add_system_message("Settings reloaded");
                    }
                    Err(e) => {
                        self.add_system_message(&format!("Failed to parse config: {}", e));
                    }
                }
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to read config file: {}", e));
            }
        }
    }

    /// Reload colors (presets, spell colors, prompt colors, UI colors) from disk
    pub fn reload_colors(&mut self) {
        match crate::config::ColorConfig::load(self.config.character.as_deref()) {
            Ok(colors) => {
                self.config.colors = colors;
                // Update parser with new presets - resolve palette names to hex values
                let presets: Vec<(String, Option<String>, Option<String>)> = self
                    .config
                    .colors
                    .presets
                    .iter()
                    .map(|(id, preset)| {
                        let resolved_fg = preset
                            .fg
                            .as_ref()
                            .map(|c| self.config.resolve_palette_color(c));
                        let resolved_bg = preset
                            .bg
                            .as_ref()
                            .map(|c| self.config.resolve_palette_color(c));
                        (id.clone(), resolved_fg, resolved_bg)
                    })
                    .collect();
                self.parser.update_presets(presets);
                self.message_processor.apply_config(self.config.clone());
                self.add_system_message("Colors reloaded");
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload colors: {}", e));
            }
        }
    }

    /// Reload layout from the auto-saved layout.toml file
    ///
    /// This reloads the character's layout from ~/.vellum-fe/{character}/layout.toml
    /// using the current terminal size stored in the layout.
    pub fn reload_layout(&mut self) {
        let layout_path =
            match crate::config::Config::auto_layout_path(self.config.character.as_deref()) {
                Ok(path) => path,
                Err(e) => {
                    self.add_system_message(&format!("Failed to get layout path: {}", e));
                    return;
                }
            };

        if !layout_path.exists() {
            self.add_system_message("No auto-saved layout found");
            self.add_system_message("Use .savelayout to save the current layout first");
            return;
        }

        match crate::config::Layout::load_from_file(&layout_path) {
            Ok(new_layout) => {
                // Get terminal size from current layout (use current if available)
                let width = self.layout.terminal_width.unwrap_or(80);
                let height = self.layout.terminal_height.unwrap_or(24);

                // Apply theme if specified
                self.apply_layout_theme(new_layout.theme.as_deref());

                // Update layout and baseline
                self.layout = new_layout.clone();
                self.baseline_layout = Some(new_layout);

                // Clear modified flag
                self.layout_modified_since_save = false;

                // Reinitialize windows with current terminal size
                self.init_windows(width, height);
                self.needs_render = true;

                // Signal frontend to reset widget caches
                self.ui_state.needs_widget_reset = true;

                self.add_system_message("Layout reloaded from disk");
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload layout: {}", e));
            }
        }
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

    // ========== Menu Building Methods ==========

    /// Build the top-level "Add Window" menu showing widget categories
    pub fn build_add_window_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let categories_map = crate::core::local_catalog::addable_by_category(&self.layout, self.game_type());

        // Sort categories for consistent display
        let mut categories: Vec<_> = categories_map.into_iter().collect();
        categories.sort_by_key(|(cat, _)| cat.clone());

        categories
            .into_iter()
            .map(
                |(category, _templates)| crate::data::ui_state::PopupMenuItem {
                    text: category.display_name().to_string(),
                    command: format!("__SUBMENU_ADD__{:?}", category),
                    disabled: false,
                },
            )
            .collect()
    }

    /// Addable window templates grouped by category, as
    /// `(category display name, [(template name, display name)])`, for
    /// frontends that render native menus instead of the popup-menu stack.
    pub fn addable_window_templates(&self) -> Vec<(String, Vec<(String, String)>)> {
        let categories_map = crate::core::local_catalog::addable_by_category(
            &self.layout,
            self.game_type(),
        );
        let mut categories: Vec<_> = categories_map.into_iter().collect();
        categories.sort_by_key(|(category, _)| category.clone());
        categories
            .into_iter()
            .map(|(category, templates)| {
                let mut entries: Vec<(String, String)> = templates
                    .into_iter()
                    .filter(|name| {
                        self.layout
                            .get_window(name)
                            .map(|w| !w.base().visibility.is_shown())
                            .unwrap_or(true)
                    })
                    .map(|name| {
                        let display = self.get_window_display_name(&name);
                        (name, display)
                    })
                    .collect();
                entries.sort_by(|a, b| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase()));
                (category.display_name().to_string(), entries)
            })
            .filter(|(_, entries)| !entries.is_empty())
            .collect()
    }

    /// Build category submenu showing available windows of that type
    pub fn build_add_window_category_menu(
        &self,
        category: &crate::config::WidgetCategory,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let categories_map = crate::core::local_catalog::addable_by_category(&self.layout, self.game_type());

        if let Some(templates) = categories_map.get(category) {
            // Filter out templates already present in the layout (so they disappear once added)
            let available_templates: Vec<_> = templates
                .iter()
                .filter(|name| {
                    self.layout
                        .get_window(name)
                        .map(|w| !w.base().visibility.is_shown())
                        .unwrap_or(true)
                })
                .collect();

            // Special handling for Status: dashboard + Indicators submenu
            if matches!(category, crate::config::WidgetCategory::Status) {
                let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
                if available_templates.iter().any(|t| *t == "dashboard") {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: "Dashboard".to_string(),
                        command: "__ADD__dashboard".to_string(),
                        disabled: false,
                    });
                }
                // Indicators submenu (only if any indicator templates are available)
                let available_owned: Vec<String> =
                    available_templates.iter().map(|s| s.to_string()).collect();
                if !self.build_indicator_add_menu(&available_owned).is_empty() {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: "Indicators >".to_string(),
                        command: "__SUBMENU_INDICATORS".to_string(),
                        disabled: false,
                    });
                }
                return items;
            }

            let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();

            // Custom template entry (derive widget type from the first available template)
            // Skip for Hands to match the fixed submenu (left/right/spell only) and Other category per design.
            let allow_custom = !matches!(category, crate::config::WidgetCategory::Hand)
                && !matches!(category, crate::config::WidgetCategory::Other);
            let has_explicit_custom = available_templates
                .iter()
                .any(|name| name.ends_with("_custom"));
            if allow_custom && !has_explicit_custom {
                if let Some(first) = available_templates.first() {
                    if let Some(widget_type) = crate::core::local_catalog::seed(first)
                        .map(|t| t.widget_type().to_string())
                    {
                        items.push(crate::data::ui_state::PopupMenuItem {
                            text: "Custom (blank)".to_string(),
                            command: format!("__ADD_CUSTOM__{}", widget_type),
                            disabled: false,
                        });
                    }
                }
            }

            items.extend(available_templates.into_iter().map(|name| {
                crate::data::ui_state::PopupMenuItem {
                    text: self.get_window_display_name(name),
                    command: format!("__ADD__{}", name),
                    disabled: false,
                }
            }));

            items
        } else {
            vec![]
        }
    }

    /// U3: the unified list of every window the client knows about, from
    /// the layout (persistent, possibly game-bound) plus session-only
    /// ephemeral windows (containers, dialog panels). This replaces the
    /// separate offer registry as the source for the Windows list.
    pub fn enumerate_known_windows(&self) -> Vec<crate::core::known_windows::KnownWindow> {
        use crate::config::WindowBinding;
        use crate::core::known_windows::{KnownWindow, KnownWindowKind};

        let mut out: Vec<KnownWindow> = Vec::new();

        // Persistent layout windows. Bound ones are game-discovered
        // dialogs/streams; unbound ones are template/custom widgets.
        // Nothing is unlisted: "main" is just the story window (hideable
        // while another window carries the main stream), and command_input
        // is hideable in the GUI (fallback bottom bar) while the TUI
        // force-shows it.
        for w in &self.layout.windows {
            let base = w.base();
            let name = base.name.clone();
            let kind = match &base.binding {
                Some(WindowBinding::Stream(_)) => KnownWindowKind::Stream,
                Some(WindowBinding::Dialog(_)) => KnownWindowKind::Dialog,
                Some(WindowBinding::Container(_)) => KnownWindowKind::Container,
                None => KnownWindowKind::Layout,
            };
            out.push(KnownWindow {
                name: name.clone(),
                title: base.title.clone().unwrap_or(name),
                kind,
                widget_type: w.widget_type().to_string(),
                shown: base.visibility.is_shown(),
                ephemeral: false,
            });
        }

        // Session-only ephemeral windows (containers, dialog panels) —
        // these live in ui_state, not the layout.
        for name in &self.ui_state.ephemeral_windows {
            let Some(win) = self.ui_state.windows.get(name) else {
                continue;
            };
            let (kind, wt) = match win.widget_type {
                crate::data::WidgetType::Container => {
                    (KnownWindowKind::Container, "container")
                }
                crate::data::WidgetType::DialogPanel => {
                    (KnownWindowKind::Dialog, "dialogpanel")
                }
                _ => (KnownWindowKind::Layout, "text"),
            };
            let title = match &win.content {
                crate::data::WindowContent::Container { container_title } => {
                    container_title.clone()
                }
                _ => name.clone(),
            };
            out.push(KnownWindow {
                name: name.clone(),
                title,
                kind,
                widget_type: wt.to_string(),
                shown: win.visible,
                ephemeral: true,
            });
        }

        // Sighted-but-not-open containers from the GameObjects registry, so
        // the user can opt one in the first time. The toggle key is the
        // ephemeral window name a container would get.
        for container in self.game_state.objects.containers() {
            if container.title.is_empty() {
                continue;
            }
            let win_name = container.title.replace(' ', "_").to_lowercase();
            if self.ui_state.windows.contains_key(&win_name) {
                continue; // already listed above as an open ephemeral window
            }
            out.push(KnownWindow {
                name: win_name,
                title: container.title.clone(),
                kind: KnownWindowKind::Container,
                widget_type: "container".to_string(),
                shown: false,
                ephemeral: true,
            });
        }

        // Full catalog: every template for this game type is a row even
        // before it exists in the layout — ticking one conjures it via
        // set_known_window_shown. Seed templates (`*_custom`) and spacers
        // are creation flows, not windows, so they stay out; command_input
        // has no template and is covered by the layout pass above.
        let existing: std::collections::HashSet<String> =
            out.iter().map(|k| k.name.to_ascii_lowercase()).collect();
        for template_name in
            crate::core::local_catalog::creatable_for_game(self.game_type())
        {
            if template_name == "spacer" || template_name.ends_with("_custom") {
                continue;
            }
            if existing.contains(&template_name.to_ascii_lowercase()) {
                continue;
            }
            let Some(template) = crate::core::local_catalog::seed(&template_name)
            else {
                continue;
            };
            out.push(KnownWindow {
                title: template
                    .base()
                    .title
                    .clone()
                    .unwrap_or_else(|| template_name.clone()),
                name: template_name,
                kind: KnownWindowKind::Layout,
                widget_type: template.widget_type().to_string(),
                shown: false,
                ephemeral: false,
            });
        }

        // Discovery memory (redesign Phase 3): bindings this character has
        // seen in past sessions (or the well-known seeds) that no row
        // above covers — so "Bounty" is addable in a FRESH layout before
        // the game re-declares it. Strict union: dedicated-view ids stay
        // owned by the template rows above (including their game-type
        // gating), bound layout windows already listed, and name
        // collisions defer to the existing row. Ticking one conjures a
        // bound window exactly as a live discovery would.
        let existing: std::collections::HashSet<String> =
            out.iter().map(|k| k.name.to_ascii_lowercase()).collect();
        for entry in &self.window_registry.bindings {
            let (binding, kind, widget_type) = match entry.kind.as_str() {
                "stream" => (
                    crate::config::WindowBinding::Stream(entry.id.clone()),
                    KnownWindowKind::Stream,
                    "text",
                ),
                "dialog" => (
                    crate::config::WindowBinding::Dialog(entry.id.clone()),
                    KnownWindowKind::Dialog,
                    "dialogpanel",
                ),
                _ => continue,
            };
            if self.layout.has_window_bound_to(&entry.id) {
                continue;
            }
            if crate::core::view_resolver::resolve_view(&binding, None)
                .dedicated_key()
                .is_some()
            {
                continue;
            }
            if existing.contains(&entry.id.to_ascii_lowercase()) {
                continue;
            }
            out.push(KnownWindow {
                name: entry.id.clone(),
                title: if entry.title.is_empty() {
                    entry.id.clone()
                } else {
                    entry.title.clone()
                },
                kind,
                widget_type: widget_type.to_string(),
                shown: false,
                ephemeral: false,
            });
        }

        out
    }

    /// Build the unified Windows list menu: the FULL window catalog (every
    /// template + layout + ephemeral runtime), each row `[x]`/`[ ]` for its
    /// shown state, grouped under disabled category-header rows. Selecting
    /// a row emits `__TOGGLE_WINDOW__<name>` to flip it (ticking a
    /// never-added template conjures it). The GUI has its own Windows
    /// window; this menu is the TUI's view of the same catalog.
    pub fn build_known_windows_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        use crate::config::WidgetCategory;
        let known = self.enumerate_known_windows();
        if known.is_empty() {
            return vec![crate::data::ui_state::PopupMenuItem {
                text: "(no windows known yet)".to_string(),
                command: String::new(),
                disabled: true,
            }];
        }
        let mut items = Vec::new();
        for category in WidgetCategory::ALL {
            let mut group: Vec<_> = known
                .iter()
                .filter(|k| WidgetCategory::from_widget_type(&k.widget_type) == category)
                .collect();
            if group.is_empty() {
                continue;
            }
            group.sort_by(|a, b| a.title.to_ascii_lowercase().cmp(&b.title.to_ascii_lowercase()));
            items.push(crate::data::ui_state::PopupMenuItem {
                text: format!("── {} ──", category.display_name()),
                command: String::new(),
                disabled: true,
            });
            for k in group {
                let mark = if k.shown { "[x]" } else { "[ ]" };
                let session = if k.ephemeral { " (session)" } else { "" };
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: format!("{} {}{}", mark, k.title, session),
                    command: format!("__TOGGLE_WINDOW__{}", k.name),
                    disabled: false,
                });
            }
        }
        items
    }

    /// U3: toggle a known window's shown state by NAME (from the unified
    /// Windows list). Flips shown↔hidden via set_known_window_shown.
    pub fn toggle_known_window(&mut self, name: &str) {
        let currently_shown = self
            .enumerate_known_windows()
            .iter()
            .find(|k| k.name == name)
            .map(|k| k.shown)
            .unwrap_or(false);
        let (w, h) = (
            self.layout.terminal_width.unwrap_or(80),
            self.layout.terminal_height.unwrap_or(24),
        );
        self.set_known_window_shown(name, !currently_shown, w, h);
    }


    pub fn build_hide_window_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let categories_map = crate::core::local_catalog::visible_by_category(&self.layout, true);

        // Sort categories for consistent display
        let mut categories: Vec<_> = categories_map.into_iter().collect();
        categories.sort_by_key(|(cat, _)| cat.clone());

        categories
            .into_iter()
            .map(
                |(category, _templates)| crate::data::ui_state::PopupMenuItem {
                    text: category.display_name().to_string(),
                    command: format!("__SUBMENU_HIDE__{:?}", category),
                    disabled: false,
                },
            )
            .collect()
    }

    /// Build category submenu for hiding windows
    pub fn build_hide_window_category_menu(
        &self,
        category: &crate::config::WidgetCategory,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let categories_map =
            crate::core::local_catalog::visible_by_category(&self.layout, true);

        if let Some(templates) = categories_map.get(category) {
            // Special handling for Status: Dashboard item + Indicators submenu
            if matches!(category, crate::config::WidgetCategory::Status) {
                let dashboards: Vec<String> = templates
                    .iter()
                    .filter(|name| *name == "dashboard")
                    .cloned()
                    .collect();
                let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
                for name in dashboards {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: self.get_window_display_name(&name),
                        command: format!("__HIDE__{}", name),
                        disabled: false,
                    });
                }
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: "Indicators >".to_string(),
                    command: "__SUBMENU_HIDE_INDICATORS".to_string(),
                    disabled: false,
                });
                return items;
            }

            templates
                .iter()
                .map(|name| crate::data::ui_state::PopupMenuItem {
                    text: self.get_window_display_name(name),
                    command: format!("__HIDE__{}", name),
                    disabled: false,
                })
                .collect()
        } else {
            vec![]
        }
    }

    /// Build indicator submenu for Status -> Indicators
    pub fn build_indicator_add_menu(
        &self,
        available_templates: &[String],
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let available: std::collections::HashSet<String> = available_templates
            .iter()
            .map(|s| s.to_lowercase())
            .collect();

        let mut templates: Vec<_> = crate::config::Config::list_indicator_templates()
            .into_iter()
            .filter(|tpl| available.contains(&tpl.key().to_lowercase()))
            .collect();

        let desired_order = ["bleeding", "diseased", "poisoned", "stunned", "webbed"];
        let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();

        for desired in &desired_order {
            if let Some(idx) = templates.iter().position(|t| {
                t.key().eq_ignore_ascii_case(desired) || t.id.eq_ignore_ascii_case(desired)
            }) {
                let tpl = templates.remove(idx);
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: tpl.title_or_id(),
                    command: format!("__ADD__{}", tpl.key()),
                    disabled: false,
                });
            }
        }

        // Append remaining templates alphabetically
        templates.sort_by(|a, b| a.title_or_id().to_lowercase().cmp(&b.title_or_id().to_lowercase()));
        for tpl in templates {
            items.push(crate::data::ui_state::PopupMenuItem {
                text: tpl.title_or_id(),
                command: format!("__ADD__{}", tpl.key()),
                disabled: false,
            });
        }

        // Always include the template editor entry at the bottom
        items.push(crate::data::ui_state::PopupMenuItem {
            text: "Editor".to_string(),
            command: "__INDICATOR_EDITOR".to_string(),
            disabled: false,
        });

        items
    }

    /// Indicator submenu for Hide
    pub fn build_indicator_hide_menu(
        &self,
        indicator_names: &[String],
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let desired_order = ["bleeding", "diseased", "poisoned", "stunned", "webbed"];
        let title_lookup: std::collections::HashMap<String, String> =
            crate::config::Config::list_indicator_templates()
                .into_iter()
                .map(|tpl| (tpl.key().to_lowercase(), tpl.title_or_id()))
                .collect();

        let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();

        for desired in &desired_order {
            for name in indicator_names {
                if name.eq_ignore_ascii_case(desired) {
                    let key = name.to_lowercase();
                    if used.insert(key.clone()) {
                        let text = title_lookup
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| self.get_window_display_name(name));
                        items.push(crate::data::ui_state::PopupMenuItem {
                            text,
                            command: format!("__HIDE__{}", name),
                            disabled: false,
                        });
                    }
                }
            }
        }
        // Append remaining indicators not in desired order
        let mut remaining: Vec<String> = indicator_names.iter().cloned().collect();
        remaining.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        for name in remaining {
            let key = name.to_lowercase();
            if used.insert(key.clone()) {
                let text = title_lookup
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| self.get_window_display_name(&name));
                items.push(crate::data::ui_state::PopupMenuItem {
                    text,
                    command: format!("__HIDE__{}", name),
                    disabled: false,
                });
            }
        }

        items
    }

    /// Indicator submenu for Edit
    pub fn build_indicator_edit_menu(
        &self,
        indicator_names: &[String],
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let desired_order = ["bleeding", "diseased", "poisoned", "stunned", "webbed"];
        let title_lookup: std::collections::HashMap<String, String> =
            crate::config::Config::list_indicator_templates()
                .into_iter()
                .map(|tpl| (tpl.key().to_lowercase(), tpl.title_or_id()))
                .collect();

        let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();

        for desired in &desired_order {
            for name in indicator_names {
                if name.eq_ignore_ascii_case(desired) {
                    let key = name.to_lowercase();
                    if used.insert(key.clone()) {
                        let text = title_lookup
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| self.get_window_display_name(name));
                        items.push(crate::data::ui_state::PopupMenuItem {
                            text,
                            command: format!("__EDIT__{}", name),
                            disabled: false,
                        });
                    }
                }
            }
        }
        let mut remaining: Vec<String> = indicator_names.iter().cloned().collect();
        remaining.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        for name in remaining {
            let key = name.to_lowercase();
            if used.insert(key.clone()) {
                let text = title_lookup
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| self.get_window_display_name(&name));
                items.push(crate::data::ui_state::PopupMenuItem {
                    text,
                    command: format!("__EDIT__{}", name),
                    disabled: false,
                });
            }
        }

        // Append editor entry at the bottom
        items.push(crate::data::ui_state::PopupMenuItem {
            text: "Editor".to_string(),
            command: "__INDICATOR_EDITOR".to_string(),
            disabled: false,
        });

        items
    }

    /// Build "Edit Window" menu showing widget categories (only categories with visible windows)
    pub fn build_edit_window_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        // include_hidden: hidden windows stay editable from the picker.
        let categories_map =
            crate::core::local_catalog::layout_windows_by_category(&self.layout, false, true);

        // Sort categories for consistent display
        let mut categories: Vec<_> = categories_map.into_iter().collect();
        categories.sort_by_key(|(cat, _)| cat.clone());

        categories
            .into_iter()
            .map(
                |(category, _templates)| crate::data::ui_state::PopupMenuItem {
                    text: category.display_name().to_string(),
                    command: format!("__SUBMENU_EDIT__{:?}", category),
                    disabled: false,
                },
            )
            .collect()
    }

    /// Build category submenu for editing windows
    pub fn build_edit_window_category_menu(
        &self,
        category: &crate::config::WidgetCategory,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        // include_hidden: hidden windows stay editable from the picker.
        let categories_map =
            crate::core::local_catalog::layout_windows_by_category(&self.layout, false, true);

        if let Some(templates) = categories_map.get(category) {
            // Special handling for Status: Dashboard + Indicators submenu
            if matches!(category, crate::config::WidgetCategory::Status) {
                let dashboards: Vec<String> = templates
                    .iter()
                    .filter(|name| *name == "dashboard")
                    .cloned()
                    .collect();
                let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
                for name in dashboards {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: self.edit_menu_entry_text(&name),
                        command: format!("__EDIT__{}", name),
                        disabled: false,
                    });
                }
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: "Indicators >".to_string(),
                    command: "__SUBMENU_EDIT_INDICATORS".to_string(),
                    disabled: false,
                });
                return items;
            }

            templates
                .iter()
                .map(|name| crate::data::ui_state::PopupMenuItem {
                    text: self.edit_menu_entry_text(name),
                    command: format!("__EDIT__{}", name),
                    disabled: false,
                })
                .collect()
        } else {
            vec![]
        }
    }

    /// Display text for an edit-menu entry; hidden windows are tagged so the
    /// picker makes their state obvious.
    fn edit_menu_entry_text(&self, name: &str) -> String {
        let display = self.get_window_display_name(name);
        let hidden = self
            .layout
            .get_window(name)
            .is_some_and(|w| !w.base().visibility.is_shown());
        if hidden {
            format!("{} (hidden)", display)
        } else {
            display
        }
    }

    /// Get display name for a window (uses title from template, or falls back to name)
    pub fn get_window_display_name(&self, name: &str) -> String {
        crate::core::local_catalog::seed(name)
            .and_then(|t| t.base().title.clone())
            .unwrap_or_else(|| name.to_string())
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
