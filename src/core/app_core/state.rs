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

mod alerts;
mod custom_status;
mod focus;
mod menus;
mod persistence;
mod remote;
mod stage_scene;
mod streams;
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

    /// Set when core wrote `config.appearance` outside the GUI's own
    /// funnel (skin-pack install/import) — the GUI must copy the store
    /// into its `ui_settings` next frame or its layout save would stomp
    /// the new look. The GUI clears it after syncing.
    pub appearance_changed_externally: bool,

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

    /// Creature-field placement (the creaturefield widget's solver state).
    /// Synced from `game_state.room_creatures` by `sync_creature_field`,
    /// event-driven on the roster generation — never per frame.
    pub creature_field: crate::core::creature_cards::solver::CreatureField,
    /// Roster generation the field was last synced against.
    pub creature_field_synced_gen: u64,
    /// Creature-field stage scene state; documented in `state/stage_scene.rs`.
    pub stage_scene: Option<std::sync::Arc<crate::config::scenes::StageScene>>,
    pub stage_scene_name: Option<String>,
    pub default_stage_scene: Option<std::sync::Arc<crate::config::scenes::StageScene>>,
    pub(crate) default_scene_name: Option<String>,
    pub field_overrides: crate::config::creature_field::FieldOverrides,
    pub(crate) scene_pick_inputs: Option<(Option<i64>, Option<String>, Option<String>)>,
    #[allow(clippy::type_complexity)]
    pub(crate) field_params_inputs: Option<(Option<String>, crate::config::creature_field::FieldOverrides)>,
    /// Game commands core logic queued outside the typed-command path (e.g.
    /// target cycling): the keybind-action dispatch returns no outcomes, so
    /// core-initiated sends ride the same per-frame `take_outbound` drain as
    /// travel/foreach automation.
    pub(crate) queued_game_commands: Vec<String>,

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
    /// Native skill trainer: off-thread fetch/submit of the play.net web
    /// skill manager, polled each frame like the jinx worker.
    pub skill_trainer_worker: crate::core::skill_trainer::SkillTrainerWorker,
    /// Ordered presentation targets for GOALS replies. The game returns one
    /// LaunchURL per command, in command order, so preserving that order is
    /// what keeps simultaneous browser tabs and native clients isolated.
    pub(super) pending_goals_launches:
        std::collections::VecDeque<super::skill_goals::PendingGoalsLaunch>,
    /// Presentation target prepared while resolving one outbound GOALS
    /// command. It moves into `pending_goals_launches` only after the owning
    /// frontend confirms that exact command reached its network queue.
    pub(super) staged_goals_launch: Option<super::skill_goals::GoalsLaunchTarget>,
    /// Auto-clear deadlines for highlight-set custom statuses (UPPERCASE
    /// id -> when it switches back off).
    pub custom_status_expiries: std::collections::HashMap<String, std::time::Instant>,
    /// Live overlay alerts + their discipline (cooldowns, concurrent cap).
    /// Core-owned so detached viewports can't double-fire it and the phone
    /// bridge can push it; frontends only ever read `alerts.active()`.
    pub alerts: crate::core::alerts::AlertState,
    /// Installed alert packs, cached in memory so per-room re-arming never
    /// touches the disk (`reload_highlights` does, and would be far too
    /// expensive to run every time the player walks through a door).
    pub alert_packs: Vec<crate::config::AlertPack>,
    /// Enable/approval record for those packs.
    pub alertpack_approvals: crate::config::AlertPackApprovals,
    /// Room scope the pack set was last armed for. Re-arming compares against
    /// this, so moving between rooms in the same area rebuilds nothing.
    pub last_pack_scope: Option<crate::config::RoomScope>,
    /// Whether the pack cache has been populated from disk this session.
    pub alert_packs_loaded: bool,
    /// Cached indicator templates keyed by UPPERCASE id, rebuilt from disk on
    /// load and after the template editor saves. Status icon resolution
    /// (indicator windows + dashboards) reads this per frame; the underlying
    /// `Config::list_indicator_templates()` does file IO, so it must not run
    /// in the render loop.
    pub indicator_templates:
        std::collections::HashMap<String, crate::config::IndicatorTemplateEntry>,
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
    pending_day_pass_scan: Option<(u32, std::time::Instant, Vec<String>)>,
    /// The scan is holding the sack open across its rounds: (sack id, was it
    /// ALREADY open - "That is already open" seen). One `open` for the whole
    /// scan, one `close` at the true end (skipped when the user keeps the
    /// sack open) - the old round-by-round close/open churned the sack live.
    day_pass_scan_open: Option<(String, bool)>,
    /// The day-pass sack contents probe has run this session (the container
    /// stream keeps contents fresh after the first open).
    day_pass_sack_probed: bool,
    /// Macro sleep segments (`look\rs2\rhide`): commands waiting out
    /// their pause, drained by take_outbound once due (insertion order
    /// preserved among same-tick due commands).
    timed_commands: Vec<(std::time::Instant, String)>,
    /// Verified item moves (`_drag`, extended feed): one at a time,
    /// confirmed against hand events, drained by take_outbound.
    pub item_mover: crate::core::item_mover::ItemMover,
    /// Token of the last managed-inventory snapshot announced to the user
    /// (keyed by token, not generation, so probe flag updates stay quiet).
    last_announced_inv_token: String,
    /// Generation of the last `.viewitem` detail echoed to main.
    last_announced_view_generation: u64,
    /// User-invoked hands stow/retrieve (`.emptyhands`/`.fillhands`) - the
    /// same StashTask the travel executor uses, run standalone.
    pub(crate) hand_stash: Option<crate::core::travel::stash::StashTask>,
    /// What the last `.emptyhands` stowed (LIFO), replayed by `.fillhands`.
    pub(crate) hand_stash_stack: Vec<crate::core::travel::stash::Stowed>,
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

    /// Room art mappings (room_images.toml), loaded lazily by the
    /// `.roomimages` command and the editor. The processor holds the
    /// room-major index it actually looks up per room change; this is the
    /// editable image-major store.
    pub room_images: Option<crate::config::room_images::RoomImagesConfig>,

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
    pub(crate) webui_event_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::webui::WebUiEvent>>,
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
    /// Whether this session is proxied through Lich (vs. a direct eAccess
    /// connection). Distinct from `webui_available`: WebUI is an optional Lich
    /// feature, while this is purely "is there a Lich to send `;` commands to".
    /// Travel's `;go2` fallback gates on THIS, not on WebUI reachability.
    pub(crate) lich_connected: bool,
    /// GUI re-emit channel: `pump_webui` forwards every bridge event here so
    /// the GUI can do its GUI-side handling (image textures, window kinds)
    /// while core owns the socket. None in headless/TUI (no local renderer).
    pub(crate) webui_gui_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::webui::WebUiEvent>>,
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
            skill_trainer_worker: Default::default(),
            pending_goals_launches: std::collections::VecDeque::new(),
            staged_goals_launch: None,
            custom_status_expiries: std::collections::HashMap::new(),
            alerts: crate::core::alerts::AlertState::new(),
            alert_packs: Vec::new(),
            alertpack_approvals: Default::default(),
            last_pack_scope: None,
            alert_packs_loaded: false,
            indicator_templates: std::collections::HashMap::new(),
            jinx_catalog: None,
            jinx_nudge_pending: true,
            travel: Default::default(),
            pending_urchin_refresh: None,
            pending_day_pass_scan: None,
            day_pass_scan_open: None,
            day_pass_sack_probed: false,
            timed_commands: Vec::new(),
            item_mover: crate::core::item_mover::ItemMover::new(),
            last_announced_inv_token: String::new(),
            last_announced_view_generation: 0,
            hand_stash: None,
            hand_stash_stack: Vec::new(),
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
            appearance_changed_externally: false,
            server_time_offset: 0,
            cmdlist: None,
            menu_request_counter: 0,
            pending_menu_requests: HashMap::new(),
            menu_categories: HashMap::new(),
            last_link_click_pos: None,
            creature_field: Default::default(),
            creature_field_synced_gen: 0,
            stage_scene: None,
            stage_scene_name: None,
            default_stage_scene: None,
            default_scene_name: None,
            field_overrides: crate::config::creature_field::FieldOverrides::load(),
            scene_pick_inputs: None, field_params_inputs: None,
            queued_game_commands: Vec::new(),
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
            room_images: None,
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
            lich_connected: false,
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

        // Same for inline image art (<vellumImg src=..>), scanned from the
        // shared image pool.
        let inline_image_count = crate::core::inline_image::reload();
        if inline_image_count > 0 {
            tracing::info!("Loaded {inline_image_count} inline images");
        }

        // Room art mappings (uid -> image), indexed room-major for the
        // per-room-change lookup.
        let room_images = Config::load_room_images(config.character.as_deref()).unwrap_or_default();
        let room_image_index = crate::config::room_images::RoomImageIndex::build(&room_images);
        if !room_image_index.is_empty() {
            tracing::info!("Loaded room art for {} rooms", room_image_index.len());
        }

        // Load saved dialog positions from widget_state.toml
        let saved_dialog_positions =
            Config::load_dialog_positions(config.character.as_deref()).unwrap_or_default();

        // Discovery memory: load (missing/corrupt = empty), then seed the
        // well-known feeds on first run so a fresh character's registry
        // starts useful. The constructor never writes; a seeded registry
        // is marked dirty and the frontend-driven autosave tick flushes
        // it (keeps constructor-only tests off the filesystem).
        let mut window_registry = Config::load_window_registry(config.character.as_deref());
        let window_registry_dirty = window_registry.seed_well_known();

        // Create message processor (shares saved_dialog_positions reference)
        let mut message_processor =
            MessageProcessor::new(config.clone(), saved_dialog_positions.clone());
        message_processor.set_room_image_index(room_image_index);

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
        let tts_manager =
            crate::tts::TtsManager::new(config.tts.enabled, config.tts.rate, config.tts.volume);
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
            skill_trainer_worker: Default::default(),
            pending_goals_launches: std::collections::VecDeque::new(),
            staged_goals_launch: None,
            custom_status_expiries: std::collections::HashMap::new(),
            alerts: crate::core::alerts::AlertState::new(),
            alert_packs: Vec::new(),
            alertpack_approvals: Default::default(),
            last_pack_scope: None,
            alert_packs_loaded: false,
            indicator_templates: std::collections::HashMap::new(),
            jinx_catalog: None,
            jinx_nudge_pending: true,
            travel: Default::default(),
            pending_urchin_refresh: None,
            pending_day_pass_scan: None,
            day_pass_scan_open: None,
            day_pass_sack_probed: false,
            timed_commands: Vec::new(),
            item_mover: crate::core::item_mover::ItemMover::new(),
            last_announced_inv_token: String::new(),
            last_announced_view_generation: 0,
            hand_stash: None,
            hand_stash_stack: Vec::new(),
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
            appearance_changed_externally: false,
            server_time_offset: 0,
            cmdlist,
            menu_request_counter: 0,
            pending_menu_requests: HashMap::new(),
            menu_categories: HashMap::new(),
            last_link_click_pos: None,
            creature_field: Default::default(),
            creature_field_synced_gen: 0,
            stage_scene: None,
            stage_scene_name: None,
            default_stage_scene: None,
            default_scene_name: None,
            field_overrides: crate::config::creature_field::FieldOverrides::load(),
            scene_pick_inputs: None, field_params_inputs: None,
            queued_game_commands: Vec::new(),
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
            room_images: None,
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
            lich_connected: false,
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
    pub fn indicator_template(&self, id: &str) -> Option<&crate::config::IndicatorTemplateEntry> {
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
        self.refresh_curated_maps();
        let base = Config::base_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let source = crate::core::map_service::resolve_source_resolution(
            self.config.map.mapdb_path.as_deref(),
            self.config.map.lich_dir.as_deref(),
            self.config.connection.game.as_deref(),
            &crate::core::mapdb_update::download_dir(&base),
        );
        let previous_warning = self.map.classic_maps_warning().map(str::to_owned);
        self.map.ensure_db(source);
        let current_warning = self.map.classic_maps_warning().map(str::to_owned);
        if current_warning != previous_warning {
            if let Some(warning) = current_warning {
                self.add_system_message(&format!("Classic map art disabled: {warning}"));
            }
        }
    }

    /// Load curated base-map membership: the rosters embedded in the build
    /// (defaults/curated_maps.toml — every user has them, no external
    /// install involved), overridden by a user-maintained
    /// `global/data/curated_maps.toml` when one exists. `set_curated`
    /// no-ops on identical data, so calling this on every source refresh
    /// is cheap in steady state.
    fn refresh_curated_maps(&mut self) {
        use crate::core::curated_maps;
        let user_file = Config::global_data_dir()
            .ok()
            .map(|dir| dir.join("curated_maps.toml"))
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| match curated_maps::CuratedMaps::from_toml(&text) {
                Ok(snapshot) => Some(snapshot),
                Err(e) => {
                    tracing::warn!("curated_maps.toml unreadable, using built-in: {e}");
                    None
                }
            });
        let curated = match user_file {
            Some(user) => user,
            None => match curated_maps::CuratedMaps::embedded() {
                Ok(embedded) => embedded,
                Err(e) => {
                    tracing::error!("embedded curated_maps.toml unreadable: {e}");
                    return;
                }
            },
        };
        if !curated.is_empty() {
            self.map.set_curated(curated);
        }
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
        self.tick_hand_stash();
        self.poll_jinx();
        self.poll_skill_trainer();
        // Auto-clear expired highlight-set custom statuses.
        self.tick_custom_statuses();
        // Expire message-derived creature effects (the timeout safety net —
        // a missed end message can never leave a stale bleed) and keep the
        // derived statuses merged across roster rebuilds.
        {
            let now_server =
                chrono::Utc::now().timestamp() + self.message_processor.server_time_offset;
            self.game_state.tick_creature_effects(now_server);
        }
        self.tick_stage_scene();
        // Creature-field roster diff (no-op while the generation matches).
        crate::core::creature_cards::sync_field(
            &mut self.creature_field,
            &mut self.creature_field_synced_gen,
            &self.game_state,
            &self.config.target_list.excluded_nouns,
        );
        // Arm/disarm area-scoped packs for wherever we are now. Gated on the
        // scope actually changing, so this is a cheap comparison on the
        // overwhelming majority of frames.
        self.rearm_alert_packs();
        // Edge-detect condition-gated alerts, then retire expired ones.
        // Evaluation runs before expiry so an alert firing this frame gets
        // its full duration rather than being aged by a stale tick.
        self.tick_alert_conditions();
        self.tick_alerts();
        // Client timers have no server to remove them; core reaps its own.
        self.tick_alert_timers();
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

    fn apply_custom_quickbars(&mut self) {
        use crate::config::{QuickbarDefinition, QuickbarEntryConfig};
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

        fn insert_quickbar(state: &mut crate::data::UiState, def: &QuickbarDefinition) {
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
                    QuickbarEntryConfig::Link {
                        label,
                        command,
                        echo,
                    } => {
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
                tracing::warn!("Quickbar default '{}' is not a valid quickbar id", trimmed);
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
            let active_quickbar_id = cache.active_quickbar_id.as_ref().and_then(|id| {
                if allowed_ids.contains(id) {
                    Some(id.clone())
                } else {
                    None
                }
            });

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
            // Overlay alerts admitted through the same drain seam.
            self.apply_pending_alerts();

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
            // Overlay alerts admitted through the same drain seam.
            self.apply_pending_alerts();

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
            ".exportskin".to_string(),
            ".importskin".to_string(),
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
                inline_image: None,
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
                    if content
                        .streams
                        .iter()
                        .any(|s| s.eq_ignore_ascii_case("main"))
                    {
                        content.add_line(line);
                        self.needs_render = true;
                        return;
                    }
                }
                WindowContent::TabbedText(ref mut content) => {
                    // Find tab subscribed to "main" stream
                    for tab in content.tabs.iter_mut() {
                        if tab
                            .definition
                            .streams
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case("main"))
                        {
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
        tracing::warn!(
            "No window found subscribed to 'main' stream for system message: {}",
            message
        );
    }

    /// Deliver a client-generated line to every window subscribed to a
    /// dedicated stream (like `inspect`). Unlike add_system_message there
    /// is no fallback to main - a dedicated stream with no subscriber
    /// drops the line, matching game streams. Returns whether anyone got it.
    pub fn add_stream_message(&mut self, stream: &str, message: &str) -> bool {
        let fg = Some(self.config.colors.ui.system_message_color.clone());
        self.add_stream_line(stream, message, fg, None, false)
    }

    /// Like [`Self::add_stream_message`] with explicit styling (banner
    /// lines with background bands, bold headers).
    pub fn add_stream_line(
        &mut self,
        stream: &str,
        message: &str,
        fg: Option<String>,
        bg: Option<String>,
        bold: bool,
    ) -> bool {
        use crate::data::{SpanType, StyledLine, TextSegment, WindowContent};
        let line = StyledLine {
            segments: vec![TextSegment {
                text: message.to_string(),
                fg,
                bg,
                bold,
                mono: false,
                span_type: SpanType::System,
                link_data: None,
                custom_emoji: None,
                inline_image: None,
            }],
            stream: stream.to_string(),
            timestamp: None,
        };
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_text(stream, std::sync::Arc::new(line.clone()));
        }
        let mut delivered = false;
        for window in self.ui_state.windows.values_mut() {
            match &mut window.content {
                WindowContent::Text(ref mut content) => {
                    if content
                        .streams
                        .iter()
                        .any(|s| s.eq_ignore_ascii_case(stream))
                    {
                        content.add_line(line.clone());
                        delivered = true;
                    }
                }
                WindowContent::TabbedText(ref mut content) => {
                    for tab in content.tabs.iter_mut() {
                        if tab
                            .definition
                            .streams
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case(stream))
                        {
                            tab.content.add_line(line.clone());
                            delivered = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if delivered {
            self.needs_render = true;
        }
        delivered
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
        self.message_processor
            .flush_current_stream(&mut self.ui_state);

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
                // Stubs and dot pairs both need their room ids on the wire
                // (stub labels; dot-pair color hashing).
                let wants_rooms = matches!(e.kind, SceneEdgeKind::Stub | SceneEdgeKind::DotPair);
                RemoteMapEdge {
                    x1: e.a.x,
                    y1: e.a.y,
                    x2: e.b.x,
                    y2: e.b.y,
                    k: match e.kind {
                        SceneEdgeKind::Directional => 0,
                        SceneEdgeKind::Connector | SceneEdgeKind::ForcedDash => 1,
                        SceneEdgeKind::Stub => 2,
                        SceneEdgeKind::DotPair => 3,
                    },
                    l: e.label.clone(),
                    ar: wants_rooms.then_some(e.a_room),
                    br: wants_rooms.then_some(e.b_room),
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
mod tests;
