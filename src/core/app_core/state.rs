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

    /// Track if save reminder has been shown this session
    pub save_reminder_shown: bool,

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
            save_reminder_shown: false,
            base_layout_name: None,
            keybind_map,
            hotbar_key_conflicts: Vec::new(),
            gameobj_data: None,
            foreach: Default::default(),
            saved_dialog_positions,
        }
    }

    pub fn new(config: Config) -> Result<Self> {
        // Load layout from file system
        let layout = Layout::load(config.character.as_deref())?;

        // Load command list
        let cmdlist = CmdList::load()
            .inspect_err(|e| tracing::warn!("Failed to load command list: {}", e))
            .ok();

        // Load saved dialog positions from widget_state.toml
        let saved_dialog_positions = Config::load_dialog_positions(config.character.as_deref())
            .unwrap_or_default();

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
            save_reminder_shown: false,
            base_layout_name: None,
            keybind_map,
            hotbar_key_conflicts,
            gameobj_data: None,
            foreach: Default::default(),
            saved_dialog_positions,
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
        app.apply_tts_settings();

        if let Some((theme_id, _)) = app.apply_layout_theme(layout_theme.as_deref()) {
            app.add_system_message(&format!("Theme switched to: {}", theme_id));
            // Update frontend cache later; AppCore just updates config here.
            // The frontend will refresh during initialization from config.
        }

        app.refresh_map_source();

        Ok(app)
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

    /// Advance the walk executor against the latest world state. Called
    /// after every processed network line and once per frontend frame (the
    /// frame tick covers time-based waits like roundtime when the game is
    /// quiet).
    pub fn tick_travel(&mut self) {
        if !self.travel.is_traveling() {
            return;
        }
        let Some(db) = self.map.mapdb().cloned() else {
            return;
        };
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
                .portal_commands()
                .into_iter()
                .map(|command| crate::config::WheelSlice {
                    // "go gate" reads as "gate" on the wedge; the full
                    // command still shows in the hub.
                    label: command
                        .split_once(' ')
                        .map(|(_, rest)| rest.to_string())
                        .unwrap_or_else(|| command.clone()),
                    command,
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
        let mut snap =
            crate::core::remote::RemoteStateSnapshot::from_game_state(&self.game_state);
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
            // Skip hidden windows
            if !window_def.base().visibility.is_shown() {
                tracing::debug!("Skipping hidden window '{}' during init", window_def.name());
                continue;
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
                    let (label, countdown_id, color) =
                        if let crate::config::WindowDef::Countdown { data, .. } = window_def {
                            (
                                data.label
                                    .clone()
                                    .unwrap_or_else(|| title.to_string()),
                                data.id
                                    .clone()
                                    .unwrap_or_else(|| window_def.name().to_string()),
                                data.color.clone(),
                            )
                        } else {
                            (title.to_string(), window_def.name().to_string(), None)
                        };

                    WindowContent::Countdown(CountdownData {
                        end_time: 0,
                        label,
                        countdown_id,
                        color,
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
                WidgetType::Performance => {
                    if let crate::config::WindowDef::Performance { data, .. } = window_def {
                        self.perf_stats.apply_enabled_from(data);
                    }
                    WindowContent::Performance
                }
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
                let (label, countdown_id, color) =
                    if let crate::config::WindowDef::Countdown { data, .. } = window_def {
                        (
                            data.label.clone().unwrap_or_else(|| title.to_string()),
                            data.id
                                .clone()
                                .unwrap_or_else(|| window_def.name().to_string()),
                            data.color.clone(),
                        )
                    } else {
                        (title.to_string(), window_def.name().to_string(), None)
                    };
                WindowContent::Countdown(CountdownData {
                    end_time: 0,
                    label,
                    countdown_id,
                    color,
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
            WidgetType::Performance => {
                if let crate::config::WindowDef::Performance { data, .. } = window_def {
                    self.perf_stats.apply_enabled_from(data);
                }
                WindowContent::Performance
            }
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

    /// Add a system message to a window that receives the "main" stream.
    /// First tries window named "main", then looks for any window subscribed to "main" stream.
    pub fn add_system_message(&mut self, message: &str) {
        use crate::data::{SpanType, StyledLine, TextSegment, WindowContent};

        let line = StyledLine {
            segments: vec![TextSegment {
                text: message.to_string(),
                fg: Some("#00ff00".to_string()),
                bg: None,
                bold: true,
                mono: false,
                span_type: SpanType::System, // system echo; skip highlight transforms
                link_data: None,
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

    /// Show help for dot commands
    pub(super) fn show_help(&mut self) {
        self.add_system_message("=== VellumFE Dot Commands ===");
        self.add_system_message("");

        // Application commands
        self.add_system_message("APPLICATION:");
        self.add_system_message("  .quit / .q              - Exit VellumFE");
        self.add_system_message("  .help / .h / .?         - Show this help");
        self.add_system_message("  .version / .ver         - Show version info");
        self.add_system_message("  .menu                   - Open main menu");
        self.add_system_message("  .settings               - Open settings editor");
        self.add_system_message("  .reload [category]      - Reload config from disk (highlights|keybinds|hotbars|settings|colors)");
        self.add_system_message("  .room                   - Show how the current room resolved against the mapdb");
        self.add_system_message("  .mapdb [download|remove|repo <r>] - Manage downloaded map data (status by default)");
        self.add_system_message("  .data [status|reload]   - Shared game-data assets: source + age (Lich folder > local > bundled)");
        self.add_system_message("  .go2 <target>           - Travel there (room id, uid, tag, saved name, or text search)");
        self.add_system_message("  .go2 stop|status        - Cancel / show the active trip");
        self.add_system_message("  .go2 save <name> [id]   - Save a target (.go2 targets lists, .go2 back returns)");
        self.add_system_message("  .sorter [on|off]        - Categorize 'look in container' output by item type");
        self.add_system_message("  .foreach ... in <bag>; cmd; cmd - Batch commands over matching container items (.foreach for usage)");
        self.add_system_message("  .stop                   - Stop whatever automation is driving (go2 trip, foreach run)");
        self.add_system_message("");

        // Layout commands
        self.add_system_message("LAYOUTS:");
        self.add_system_message("  .savelayout [name]      - Save current layout (default: 'default')");
        self.add_system_message("  .loadlayout [name]      - Load a saved layout");
        self.add_system_message("  .layouts                - List available layouts");
        self.add_system_message("  .resize                 - Resize layout to current terminal");
        self.add_system_message("");

        // Window management
        self.add_system_message("WINDOWS:");
        self.add_system_message("  .windows                - List all windows");
        self.add_system_message("  .addwindow              - Open widget type picker");
        self.add_system_message("  .addwindow <name> <type> <x> <y> <w> [h] - Add window manually");
        self.add_system_message("  .deletewindow <name>    - Delete a window");
        self.add_system_message("  .delwindow <name>       - Alias for .deletewindow");
        self.add_system_message("  .hidewindow [name]      - Hide window (or open picker)");
        self.add_system_message("  .hidewin [name]         - Alias for .hidewindow");
        self.add_system_message("  .editwindow [name]      - Edit window (or open picker)");
        self.add_system_message("  .editwin [name]         - Alias for .editwindow");
        self.add_system_message("  .rename <win> <title>   - Rename window title");
        self.add_system_message("  .border <win> <style> [color] - Set window border");
        self.add_system_message("    Styles: all, none, top, bottom, left, right");
        self.add_system_message("");

        // Highlights
        self.add_system_message("HIGHLIGHTS:");
        self.add_system_message("  .highlights / .hl       - Open highlights browser");
        self.add_system_message("  .addhighlight / .addhl  - Create new highlight");
        self.add_system_message("  .edithighlight <name>   - Edit existing highlight");
        self.add_system_message("  .edithl <name>          - Alias for .edithighlight");
        self.add_system_message("  .savehighlights [name]  - Save highlights as profile (default: 'default')");
        self.add_system_message("  .loadhighlights [name]  - Load highlights from profile");
        self.add_system_message("  .highlightprofiles      - List saved highlight profiles");
        self.add_system_message("");

        // Testing
        self.add_system_message("TESTING:");
        self.add_system_message("  .testline <text>        - Test highlights/squelch with fake game line");
        self.add_system_message("");

        // Keybinds
        self.add_system_message("KEYBINDS:");
        self.add_system_message("  .keybinds / .kb         - Open keybinds browser");
        self.add_system_message("  .addkeybind / .addkey   - Create new keybind");
        self.add_system_message("  .savekeybinds [name]    - Save keybinds as profile (default: 'default')");
        self.add_system_message("  .loadkeybinds <name>    - Load keybinds from profile");
        self.add_system_message("  .keybindprofiles        - List saved keybind profiles");
        self.add_system_message("");

        // Hotbars
        self.add_system_message("HOTBARS:");
        self.add_system_message("  .hotbars / .hotbar      - Open hotbar editor (bars of command buttons)");
        self.add_system_message("    Add a bar to a layout with a 'hotkeybar' window (.addwindow)");
        self.add_system_message("");

        // Colors
        self.add_system_message("COLORS:");
        self.add_system_message("  .colors / .colorpalette - Open color palette browser");
        self.add_system_message("  .addcolor / .createcolor - Create new palette color");
        self.add_system_message("  .uicolors               - Open UI colors browser");
        self.add_system_message("  .spellcolors            - Open spell colors browser");
        self.add_system_message("  .addspellcolor          - Create new spell color");
        self.add_system_message("  .newspellcolor          - Alias for .addspellcolor");
        self.add_system_message("  .setpalette             - Load palette colors into terminal");
        self.add_system_message("  .resetpalette           - Reset terminal palette to defaults");
        self.add_system_message("");

        // Themes
        self.add_system_message("THEMES:");
        self.add_system_message("  .themes                 - Open themes browser");
        self.add_system_message("  .settheme <name>        - Switch to a theme");
        self.add_system_message("  .theme <name>           - Alias for .settheme");
        self.add_system_message("  .edittheme              - Edit current theme");
        self.add_system_message("  .skins                  - List installed GUI skins");
        self.add_system_message("  .setskin <name>         - Activate a GUI skin (.setskin none to disable)");
        self.add_system_message("  .skin <name>            - Alias for .setskin");
        self.add_system_message("  .makeskin <name>        - Create a starter skin to edit");
        self.add_system_message("  .reloadskin             - Reload the active skin's images");
        self.add_system_message("");

        // Sharing
        self.add_system_message("SHARING:");
        self.add_system_message("  .uiexport <name> [parts]- Export layout/highlights/keybinds/hotbars/colors/macros/skin as a shareable pack");
        self.add_system_message("  .uiimport <name|file>   - Preview a shared UI pack; add 'apply' to install (with backups)");
        self.add_system_message("");

        // Tab navigation
        self.add_system_message("TAB NAVIGATION:");
        self.add_system_message("  .nexttab                - Switch to next tab");
        self.add_system_message("  .prevtab                - Switch to previous tab");
        self.add_system_message("  .gonew / .nextunread    - Jump to next tab with unread messages");
        self.add_system_message("");

        // Toggles
        self.add_system_message("TOGGLES:");
        self.add_system_message("");

        // Window locking
        self.add_system_message("WINDOW LOCKING:");
        self.add_system_message("  .lockwindows / .lockall - Toggle lock on all windows (prevent move/resize)");
        self.add_system_message("");

        self.add_system_message("Type the command name for more details. Example: .help windows");
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

    /// Hide a window (keep in layout for persistence, remove from UI)
    pub fn hide_window(&mut self, name: &str) {
        if name == "main" {
            self.add_system_message("Cannot hide main window");
            return;
        }

        // Find ALL windows with this name and mark as hidden (handles duplicates)
        let mut found_count = 0;
        for window_def in self.layout.windows.iter_mut() {
            if window_def.name() == name && window_def.base().visibility.is_shown() {
                window_def.base_mut().visibility = crate::config::WindowVisibility::Hidden;
                found_count += 1;
            }
        }

        if found_count > 0 {
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
        let template = crate::config::Config::get_window_template(template_name)?;
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
            let template_name = crate::config::Config::dialog_id_to_template(&dialog_id).to_string();

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
            let (w, h) = (40u16, 15u16);
            let x = terminal_width.saturating_sub(w) / 2;
            let y = terminal_height.saturating_sub(h) / 2;
            (x, y, w, h)
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

        // Panels are tall and narrow (combat is ~190x288 px → ~24x18 cells).
        let (w, h) = (26u16, 20u16);
        let (x, y) = if let Some(saved) = self.saved_dialog_positions.dialogs.get(dialog_id) {
            (
                saved.x.min(terminal_width.saturating_sub(w)),
                saved.y.min(terminal_height.saturating_sub(h)),
            )
        } else {
            // Default toward the right edge, the game's usual hint.
            (terminal_width.saturating_sub(w + 1), 1)
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
            // Keep the dialog-popup allow-set in sync: a dialog-bound window
            // being shown/hidden decides whether its openDialog may pop up.
            if let Some(crate::config::WindowBinding::Dialog(id)) = win.base().binding.clone() {
                if shown {
                    self.ui_state.shown_dialog_ids.insert(id);
                } else {
                    self.ui_state.shown_dialog_ids.remove(&id);
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

        // Not yet materialized. Three possibilities to conjure when shown:
        if shown {
            // A dialog-store entry → a dialog/panel we can show.
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
    /// windows (known forever); the visibility default respects the config
    /// blocklist. No-op if a window is already bound to this id.
    fn register_window_discovery(&mut self, d: crate::data::WindowDiscovery) {
        use crate::config::{WindowBinding, WindowVisibility};
        use crate::data::WindowDiscoveryKind;

        if self.layout.has_window_bound_to(&d.id) {
            return;
        }

        // Pick the template + binding for this discovery kind.
        let (binding, template) = match d.kind {
            WindowDiscoveryKind::Stream => {
                // Streams bind to a blank text window that subscribes to
                // the id ("text_custom" is the addable blank-text template).
                (WindowBinding::Stream(d.id.clone()), "text_custom")
            }
            WindowDiscoveryKind::DialogPanel => {
                (WindowBinding::Dialog(d.id.clone()), "dialogpanel")
            }
            // Popups (bank) aren't layout widgets; they're handled by the
            // active_dialog popup path. Skip layout registration for now
            // (U5 gives bank a first-class row).
            WindowDiscoveryKind::DialogPopup => return,
        };

        if let Some(name) = self.layout.register_discovered_window(binding, template) {
            // A new discovery changes the layout — mark it so the autosave
            // (or .savelayout) persists it, making the window known forever.
            self.mark_layout_modified();
            // Set a friendly title + Shown/Hidden default.
            if let Some(def) = self.layout.windows.iter_mut().find(|w| w.name() == name) {
                if !d.title.is_empty() {
                    def.base_mut().title = Some(d.title.clone());
                }
                // Blocklisted → stay Hidden (already the register default);
                // otherwise a freshly discovered window is Hidden too (U3:
                // hidden-by-default), but this is where a future policy
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
            _ => WindowContent::Empty,
        };

        if widget_type == WidgetType::Performance {
            let cfg = crate::config::PerformanceWidgetData {
                enabled: true,
                show_fps: true,
                show_frame_times: true,
                show_render_times: true,
                show_ui_times: true,
                show_wrap_times: true,
                show_net: true,
                show_parse: true,
                show_events: true,
                show_memory: true,
                show_lines: true,
                show_uptime: true,
                show_jitter: true,
                show_frame_spikes: true,
                show_event_lag: true,
                show_memory_delta: true,
            };
            self.perf_stats.apply_enabled_from(&cfg);
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

    /// Build windows submenu
    pub fn build_windows_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
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
            // "Edit Performance" removed - now use right-click on overlay to toggle metrics
            crate::data::ui_state::PopupMenuItem {
                text: "Hide window >".to_string(),
                command: "menu:hidewindow".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Show/Hide windows >".to_string(),
                command: "menu:knownwindows".to_string(),
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

    /// Mark layout as modified and show reminder (once per session)
    pub fn mark_layout_modified(&mut self) {
        self.layout_modified_since_save = true;

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
            self.layout_modified_since_save = true;
            self.needs_render = true;
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

        // Autosave to character-specific layout.toml (if character is set)
        if let Some(ref character) = self.config.character {
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
                .save_auto(character, &base_layout_name, terminal_size)
            {
                tracing::warn!("Failed to autosave layout on quit: {}", e);
            } else {
                tracing::info!(
                    "Layout autosaved to character profile '{}' (base: {}, terminal: {:?})",
                    character,
                    base_layout_name,
                    terminal_size
                );
            }
        } else {
            // No character set - save to default profile: ~/.vellum-fe/default/layout.toml
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
                .save_auto("default", &base_layout_name, terminal_size)
            {
                tracing::warn!("Failed to autosave layout on quit: {}", e);
            } else {
                tracing::info!(
                    "Layout autosaved to default profile (base: {}, terminal: {:?})",
                    base_layout_name,
                    terminal_size
                );
            }
        }

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

        let cache = crate::session_cache::SessionCache {
            quickbars,
            quickbar_order,
            active_quickbar_id,
        };
        if let Err(err) = crate::session_cache::save(self.config.character.as_deref(), &cache) {
            tracing::warn!("Failed to save session cache: {}", err);
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
        self.add_system_message("All configuration reloaded");
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
                self.config.controller_binds =
                    crate::config::Config::load_controller_binds(character).unwrap_or_default();
                self.config.controller_shift_binds =
                    crate::config::Config::load_controller_binds_layer(true, character)
                        .unwrap_or_default();
                self.config.controller_wheel =
                    crate::config::Config::load_controller_wheel(character).unwrap_or_default();
                self.config.controller_wheels =
                    crate::config::Config::load_controller_wheels(character).unwrap_or_default();
                self.config.controller_wheels_meta =
                    crate::config::Config::load_controller_wheels_meta(character)
                        .unwrap_or_default();
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
        let categories_map = crate::config::Config::get_addable_templates_by_category(&self.layout, self.game_type());

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
        let categories_map = crate::config::Config::get_addable_templates_by_category(
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
        let categories_map = crate::config::Config::get_addable_templates_by_category(&self.layout, self.game_type());

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
                    if let Some(widget_type) = crate::config::Config::get_window_template(first)
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
        // dialogs/streams; unbound ones are template/custom widgets. Skip
        // the essentials that can't be hidden (main stream, command input).
        for w in &self.layout.windows {
            let base = w.base();
            let name = base.name.clone();
            if name == "main" || w.widget_type() == "command_input" {
                continue;
            }
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
            let kind = match win.widget_type {
                crate::data::WidgetType::Container => KnownWindowKind::Container,
                crate::data::WidgetType::DialogPanel => KnownWindowKind::Dialog,
                _ => KnownWindowKind::Layout,
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
                shown: false,
                ephemeral: true,
            });
        }

        out
    }

    /// Build the unified Windows list menu: every known window (from the
    /// layout + ephemeral runtime), each row `[x]`/`[ ]` for its shown
    /// state, grouped by kind. Selecting a row emits `__TOGGLE_WINDOW__<name>`
    /// to flip it. U3: reads enumerate_known_windows — no offer registry.
    pub fn build_known_windows_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        use crate::core::known_windows::KnownWindowKind;
        let known = self.enumerate_known_windows();
        if known.is_empty() {
            return vec![crate::data::ui_state::PopupMenuItem {
                text: "(no windows known yet)".to_string(),
                command: String::new(),
                disabled: true,
            }];
        }
        let mut items = Vec::new();
        for kind in KnownWindowKind::MENU_ORDER {
            let mut group: Vec<_> = known.iter().filter(|k| k.kind == kind).collect();
            if group.is_empty() {
                continue;
            }
            group.sort_by(|a, b| a.title.cmp(&b.title));
            for k in group {
                let mark = if k.shown { "[x]" } else { "[ ]" };
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: format!("{} {} ({})", mark, k.title, kind.label()),
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
        let categories_map = crate::config::Config::get_visible_templates_by_category(&self.layout, true);

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
            crate::config::Config::get_visible_templates_by_category(&self.layout, true);

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
            crate::config::Config::get_layout_templates_by_category(&self.layout, false, true);

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
            crate::config::Config::get_layout_templates_by_category(&self.layout, false, true);

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
        crate::config::Config::get_window_template(name)
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
        };

        let with_hidden =
            crate::config::Config::get_layout_templates_by_category(&layout, false, true);
        assert!(with_hidden
            .get(&crate::config::WidgetCategory::Other)
            .is_some_and(|names| names.iter().any(|n| n == "spacer_1")));

        let visible_only =
            crate::config::Config::get_visible_templates_by_category(&layout, false);
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
        };
        core
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
        let mut def = crate::config::Config::get_window_template(template_name)
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
            let mut def = crate::config::Config::get_window_template("gs4_experience").unwrap();
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
        let mut bank = crate::config::Config::get_window_template("stance").unwrap();
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
    fn rediscovery_of_a_persisted_window_is_idempotent() {
        // U4: after a persisted discovered window reloads (simulated: a
        // bound Hidden layout entry already present), the game re-announcing
        // it must NOT create a duplicate, and must NOT force it visible.
        use crate::config::{WindowBinding, WindowVisibility};
        use crate::data::{WindowDiscovery, WindowDiscoveryKind};
        let mut core = core_with_layout(vec![]);
        // Simulate a reloaded layout: combat already bound + Hidden.
        let mut combat = crate::config::Config::get_window_template("stance").unwrap();
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
    fn enumerate_known_windows_covers_layout_and_ephemeral() {
        use crate::core::known_windows::KnownWindowKind;
        // A bound (discovered) hidden dialog window, an unbound plain
        // widget, and the un-hideable essentials.
        let mut core = core_with_layout(vec![]);
        let mut combat = crate::config::Config::get_window_template("stance").unwrap();
        combat.base_mut().name = "combat".to_string();
        combat.base_mut().title = Some("Combat".to_string());
        combat.base_mut().binding =
            Some(crate::config::WindowBinding::Dialog("combat".to_string()));
        combat.base_mut().visibility = crate::config::WindowVisibility::Hidden;
        core.layout.windows.push(combat);
        core.layout.windows.push(positioned_text_def("main", 0, 0, 40, 10)); // essential
        core.layout.windows.push(positioned_text_def("my_notes", 0, 0, 20, 5)); // plain

        let known = core.enumerate_known_windows();
        // "main" is filtered out (essential).
        assert!(!known.iter().any(|k| k.name == "main"));
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
}


