//! XML message processing
//!
//! Handles parsing and routing of XML messages from the game server.
//! Updates GameState and UiState based on incoming messages.

mod buffers;
mod component;
pub use component::SPRITE_COMPONENT;
mod element;
mod flush_line;
mod routing;

use crate::config::{Config, SavedDialogPositions, SpellColorStyle, StreamRoute};
use crate::core::bounty_parser;
use crate::core::GameState;
use crate::data::*;
use crate::parser::ParsedElement;
// std::time unused here

/// Where a line from a stream should go, decided purely from subscription
/// state + the `[streams.routes]` map + the fallback window name. No
/// window-existence checks happen here — delivery walks `candidates` and
/// uses the first window that actually exists (never creating or opening
/// one). The GUI Streams panel reuses this to preview routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    /// A subscribed window handles the stream; orphan routing does not apply.
    Subscribed,
    /// Drop the line silently.
    Discard,
    /// Deliver to the first window in `candidates` that exists.
    Deliver { candidates: Vec<String> },
}

/// Map an injury `<image>`'s `name` to a doll severity level for body part
/// `id`. `Injury1-3` and the nervous system's own `Nsys1-3` prefix are wounds
/// (levels 1-3); `Scar1-3` are scars (4-6); `name == id` (or anything else)
/// means cleared (0). The `Nsys` prefix is the subtle one — the game reports
/// nerve wounds under it, not `Injury`, so omitting it silently dropped every
/// nsys wound to 0 and the doll never showed convulsions.
pub fn injury_name_to_level(id: &str, name: &str) -> u8 {
    if name == id {
        0
    } else if name.starts_with("Injury") || name.starts_with("Nsys") {
        match name.chars().last() {
            Some('1') => 1,
            Some('2') => 2,
            Some('3') => 3,
            _ => 0,
        }
    } else if name.starts_with("Scar") {
        match name.chars().last() {
            Some('1') => 4,
            Some('2') => 5,
            Some('3') => 6,
            _ => 0,
        }
    } else {
        0
    }
}

/// The room component uses this sentinel when Lich has disabled the native
/// room stream.  It is transport metadata, not a description the player
/// should see or retain as room state.
fn room_description_is_disabled(text: &str) -> bool {
    text.trim()
        .eq_ignore_ascii_case("[Room window disabled at this location.]")
}

/// Format the two room identifiers without pretending Lich's navigation UID
/// is its map room number.  Both are useful, but they are different domains.
pub(crate) fn canonical_room_id(
    lich_room_id: Option<&str>,
    nav_room_uid: Option<&str>,
) -> Option<String> {
    let lich = lich_room_id.map(str::trim).filter(|id| !id.is_empty());
    let uid = nav_room_uid.map(str::trim).filter(|id| !id.is_empty());
    match (lich, uid) {
        (Some(room), Some(uid)) if room != uid => Some(format!("{room} (u{uid})")),
        (Some(room), _) => Some(room.to_string()),
        (None, Some(uid)) => Some(format!("u{uid}")),
        (None, None) => None,
    }
}

#[derive(Clone)]
struct PendingRemoteRoomStory {
    identity: String,
    title: Option<String>,
    uid: Option<String>,
    component_lines: Vec<crate::data::widget::StyledLine>,
}

struct NativeRoomCapture {
    /// Header title with brackets and any ` - <room#>` suffix stripped.
    title: String,
    /// Navigation uid from an explicit `(uNNN)` suffix; when absent, the
    /// identity resolves against `current_room_uid` at the prompt (nav can
    /// arrive after the header line).
    uid: Option<String>,
    description: Vec<crate::data::widget::StyledLine>,
    capturing_description: bool,
}

/// Routing precedence for a stream: subscribed window > `routes` entry >
/// `fallback`. Route lookup is case-insensitive (matching the legacy
/// drop-list comparison). A `window:<name>` route lists its window first,
/// then the fallback window, then "main" as the last resort — windows are
/// never auto-created or auto-opened for a route.
pub fn route_for(
    stream_id: &str,
    has_subscriber: bool,
    routes: &std::collections::BTreeMap<String, StreamRoute>,
    fallback: &str,
) -> RouteDecision {
    if has_subscriber {
        return RouteDecision::Subscribed;
    }
    let route = routes
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(stream_id))
        .map(|(_, route)| route);
    let mut candidates: Vec<String> = Vec::new();
    match route {
        Some(StreamRoute::Discard) => return RouteDecision::Discard,
        Some(StreamRoute::Main) => candidates.push("main".to_string()),
        Some(StreamRoute::Window(name)) => {
            candidates.push(name.clone());
            candidates.push(fallback.to_string());
            candidates.push("main".to_string());
        }
        None => {
            // Unrouted stream: existing fallback behavior ("main" as the
            // last resort when the fallback window itself is missing).
            candidates.push(fallback.to_string());
            candidates.push("main".to_string());
        }
    }
    // Order-preserving dedup (e.g. fallback == "main").
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| seen.insert(c.clone()));
    RouteDecision::Deliver { candidates }
}

/// Processes incoming game messages and updates state
pub struct MessageProcessor {
    /// Configuration (for presets, highlights, etc.)
    config: Config,

    /// Prompt character -> resolved color, prebuilt from config.colors.prompt_colors
    /// so prompt rendering doesn't linear-scan the config per character
    prompt_color_map: std::collections::HashMap<char, String>,

    /// Parser for parsing XML content
    parser: crate::parser::XmlParser,

    /// Core highlight engine - applies highlights once during message processing
    highlight_engine: super::highlight_engine::CoreHighlightEngine,

    /// Current text stream (for multi-line messages)
    current_stream: String,

    /// Accumulated styled text for current stream
    current_segments: Vec<TextSegment>,
    /// Extra lines a transform (sorter) generated from the current line;
    /// the flush wrapper re-feeds them through the normal pipeline.
    injected_lines: std::collections::VecDeque<Vec<TextSegment>>,
    /// Item classifier for the sorter transform, lazily resolved through
    /// the data pack. Cleared by `.data reload`.
    sorter_gameobj: Option<std::sync::Arc<crate::core::gameobj_data::GameObjData>>,
    /// Active INVENTORY FULL scan (marked/registered status → registry).
    /// While capturing, reply lines are squelched and parsed; the prompt
    /// finalizes it into `game_state.objects`.
    inv_scan: crate::core::game_objects::inv_scan::InvScan,
    /// Container contents extracted from a main-stream look line during
    /// flush (which lacks `game_state`); drained into the registry by the
    /// caller in `process_element`. (container_id, items)
    pending_container_ingest: Option<(String, Vec<crate::core::game_objects::GameItem>)>,
    /// READY/STOW list rows captured during flush (no `game_state` there);
    /// drained into `game_state.objects` at the prompt. (line_text, item).
    pending_ready_stow: Vec<(String, Option<crate::core::game_objects::GameItem>)>,
    /// Move-feedback events classified during flush (no `game_state` there);
    /// drained into `game_state.move_feedback` at the prompt so the walk
    /// executor sees each one exactly once.
    pending_move_feedback: Vec<(u64, crate::core::move_feedback::MoveFeedback)>,
    /// Message-derived creature-effect events captured during flush (no
    /// game_state in hand there): (exist id, effect name, Some(severity) =
    /// start / None = end, timeout_s). Drained at the prompt.
    pub(crate) pending_creature_effects: Vec<(String, String, Option<u8>, u32)>,
    /// Monotone count of flushed game lines - the stamp on move-feedback
    /// events (Lich's room_count guard generalized): the executor ignores
    /// reactive events whose line predates its last send.
    pub game_line_no: u64,
    /// Raw game lines captured during flush (no `game_state` there); pushed
    /// into `game_state.recent_lines` at the prompt for scripted-edge awaits.
    pending_recent_lines: Vec<String>,
    /// Whether to buffer raw lines at all. Off unless a travel task is
    /// running: awaits are the only consumer, and copying every game line into
    /// a ring for a feature nobody is using is pure waste. `tick_travel`
    /// raises it when travel starts and drops it when travel ends.
    pub capture_recent_lines: bool,
    /// Character-state lines captured during flush (no `game_state` there);
    /// fed into `game_state.character` at the prompt. Society/profession/CHE/
    /// citizenship output from SOCIETY/INFO/PROFILE/CITIZENSHIP.
    pending_character_lines: Vec<String>,
    /// Day-pass description/expiry lines + the line's pass-link id, applied to
    /// `game_state.day_passes` at the prompt IN ORDER (expiry follows desc).
    pending_day_pass_lines: Vec<(String, Option<String>)>,
    /// Silver on hand parsed from a `wealth` line during flush; applied to
    /// `game_state.silver` at the prompt.
    pending_silver: Option<u64>,
    /// Group events captured during flush, with the line's `<a exist noun>`
    /// links. Text says what happened; the links say to whom. Applied to
    /// `game_state.group` at the prompt IN ORDER -- a `group` reply's roster
    /// line and its status sentinel must not be reordered.
    pending_group: Vec<(
        crate::core::group::GroupEvent,
        Vec<crate::core::group::GroupMember>,
    )>,

    /// Track if chunk (since last prompt) has main stream text
    chunk_has_main_text: bool,
    /// Story ("main") text reached REMOTE clients since the last prompt.
    /// Tracked separately from `chunk_has_main_text`, which also arms when
    /// stream text falls back into the local main window because its own
    /// window is missing (headless layouts without thoughts/arrivals
    /// windows). Remote clients route those lines to their own feeds, so
    /// the phone's story must gate its prompt separators on ITS OWN
    /// activity — otherwise every background thought/arrival strands a
    /// lone prompt line in the phone's story.
    remote_chunk_has_story_text: bool,
    /// True while flushing a prompt the remote story feed should NOT
    /// receive (nothing reached it since the last prompt); the flush's
    /// remote tap skips the push. The local main window still shows the
    /// separator — the fallback text landed there.
    suppress_remote_tap: bool,
    /// Familiar-stream text arrived since the last prompt. Drives the prompt
    /// echo into the familiar window (arena-spectate round separators).
    chunk_has_familiar_text: bool,
    /// True only while flushing the internally-built familiar prompt echo,
    /// exempting it from the moved-prompt strip (it is prompt-shaped too).
    emitting_familiar_separator: bool,

    /// Track if chunk (since last prompt) has silent updates
    pub chunk_has_silent_updates: bool,

    /// If true, discard text because no window exists for current stream
    discard_current_stream: bool,

    /// Windows whose layout def opts into TTS (`tts_speak`). Rebuilt by
    /// `AppCore::refresh_tts_windows` on layout load and editor saves.
    tts_windows: std::collections::HashSet<String>,

    /// Indicator ids (uppercase) "claimed" by an indicator template's
    /// condition `states` — a combined indicator owns them, so dashboard
    /// runtime auto-discovery must not add them as separate orphan cells.
    /// Rebuilt by `AppCore::refresh_indicator_templates`.
    claimed_indicator_ids: std::collections::HashSet<String>,

    /// Server time offset for countdown synchronization
    pub server_time_offset: i64,

    /// Buffer for accumulating inventory stream lines (double-buffer system)
    inventory_buffer: Vec<Vec<TextSegment>>,

    /// Buffer for accumulating reserve stream lines (double-buffer system,
    /// same snapshot semantics as inventory)
    reserve_buffer: Vec<Vec<TextSegment>>,

    /// Previous reserve buffer for comparison (avoid unnecessary updates)
    previous_reserve: Vec<Vec<TextSegment>>,

    /// Previous inventory buffer for comparison (avoid unnecessary updates)
    previous_inventory: Vec<Vec<TextSegment>>,

    /// Continuation-following for `<inventoryManager>` (extended feed):
    /// owns request tokens, merges paginated chunks, publishes complete
    /// snapshots. Outbound commands are drained by AppCore's tick.
    pub inv_service: crate::core::inventory_service::InventoryService,

    /// Buffer for accumulating spells stream lines (double-buffer system)
    spells_buffer: Vec<Vec<TextSegment>>,

    /// Previous spells buffer for comparison (avoid unnecessary updates)
    previous_spells: Vec<Vec<TextSegment>>,

    /// Temporary buffer for accumulating segments within current Spells stream line
    spells_line_buffer: Vec<TextSegment>,

    /// Skip the next Spells clearStream (used after _spell_update_links)
    skip_next_spells_clear: bool,

    /// Buffer for accumulating perception stream lines (for perception widget)
    perception_buffer: Vec<Vec<TextSegment>>,

    /// Previous room component values (for change detection to avoid unnecessary processing)
    previous_room_components: std::collections::HashMap<String, String>,

    /// Current room uid from the last `<nav rm=>`, mirrored from
    /// AppCore.nav_room_id. The `sprite` component arrives later in the same
    /// room block but is not handed nav_room_id, so room-art injection reads
    /// the uid from here. None until the first nav tag.
    current_room_uid: Option<u64>,

    /// Identity of the last room movement represented in remote Story.
    last_remote_story_room: Option<String>,

    /// Component-only room fallback, staged until the prompt.  A Lich session
    /// commonly sends an authoritative decorated copy on `main` immediately
    /// after the component stream; waiting lets that native copy win without
    /// duplicating it.  Direct/component-only sessions still get one block at
    /// the prompt.
    pending_remote_room_story: Option<PendingRemoteRoomStory>,

    /// Authoritative native room block currently arriving on `main`.  Its
    /// header is already ordinary Story text; the first following prose line
    /// is retained here so the Room latest-state projection can replace a
    /// disabled component sentinel at the prompt boundary.
    native_room_capture: Option<NativeRoomCapture>,

    /// Room art segments captured from the `sprite` component, held until
    /// the `room desc` component arrives so the mirrored description can
    /// LEAD with them. Sprite precedes the description in the room block, so
    /// writing art straight into room_description would be overwritten.
    /// This is what non-GUI frontends (phone, headless) read.
    pending_room_art: Vec<TextSegment>,

    /// Room uid -> art, rebuilt from room_images.toml at load and on
    /// `.reload`. Empty when the feature is off or nothing is mapped.
    room_image_index: crate::config::room_images::RoomImageIndex,

    squelch_matcher: Option<aho_corasick::AhoCorasick>,
    /// Case-insensitive squelch literals (separate automaton: the
    /// insensitivity flag is builder-wide, so rules opt in individually).
    squelch_matcher_ci: Option<aho_corasick::AhoCorasick>,
    squelch_regexes: Vec<regex::Regex>,

    /// Redirect cache: true if any highlights have redirect_to configured (lazy check optimization)
    has_redirect_highlights: bool,

    /// Aho-Corasick matcher over all fast-parse redirect literals; pattern ids
    /// index into redirect_literal_meta
    redirect_matcher: Option<aho_corasick::AhoCorasick>,
    /// (target window, mode) per fast-parse redirect literal, pattern-id-indexed
    redirect_literal_meta: Vec<(String, crate::config::RedirectMode)>,
    /// Prebuilt (regex, target window, mode) for non-fast redirect patterns
    redirect_regexes: Vec<(regex::Regex, String, crate::config::RedirectMode)>,

    /// Text stream subscribers map: stream_id -> list of window names that subscribe
    /// Built from widget configs at startup and on layout reload
    text_stream_subscribers: std::collections::HashMap<String, Vec<String>>,

    /// Every stream id Lich has pushed this session, mapped to a friendly label
    /// when one is known (from a `<streamWindow title="...">`). Populated as
    /// streams arrive; powers the custom-window authoring "seen this session"
    /// pick-list. Ordered so the picker lists ids deterministically.
    seen_streams: std::collections::BTreeMap<String, Option<String>>,

    /// Newly registered container (for container discovery mode)
    /// Set when a container is first seen, cleared after processing
    pub newly_registered_container: Option<(String, String)>, // (id, title)

    /// Latest Lich WebUI handshake reply (`;ui handshake` -> `<LichWebUI/>`).
    /// Set on parse; the frontend takes it and connects the WebUI bridge.
    pub pending_webui_handshake: Option<crate::data::webui::WebUiHandshake>,

    /// Ordered `<LaunchURL src=.../>` messages from the game, drained by
    /// AppCore each frame. Multiple GOALS replies can land in one frontend
    /// event batch, so this must preserve every reply in wire order.
    pub pending_launch_urls: std::collections::VecDeque<String>,

    /// Pending sounds from highlight processing (to be transferred to GameState)
    pub pending_sounds: Vec<super::highlight_engine::SoundTrigger>,
    /// Custom-status changes from matched highlights, drained by AppCore.
    pub pending_status_actions: Vec<super::highlight_engine::StatusAction>,
    /// Overlay alerts from matched highlights, drained by AppCore into the
    /// core alert state (which owns cooldowns, the concurrent cap, and expiry).
    pub pending_alerts: Vec<super::highlight_engine::AlertTrigger>,
    /// Rumble pattern names from highlight matches, drained by AppCore
    /// into the haptic queue.
    pub pending_rumbles: Vec<String>,

    /// Mapping observations parsed off the main stream (forage sense, ranger
    /// sense). AppCore drains these and attributes them to the current room
    /// uid — the processor has no room context, same split as sounds.
    pub pending_evidence: Vec<super::evidence::Observation>,

    /// A maze route heard from a pathcode NPC ("Your route is: ...").
    /// AppCore attributes it to the maze whose entrance we're standing at
    /// and persists it under that maze's name.
    pub pending_pathcode: Option<Vec<String>>,

    /// Saved dialog positions for persistence across sessions
    pub saved_dialog_positions: SavedDialogPositions,

    /// Buffered bounty data: raw text and parsed compact lines
    /// Updated whenever bounty stream text arrives, regardless of whether a bounty window exists
    bounty_buffer: Option<(String, Vec<String>)>,

    /// Buffered society stream lines for reload
    /// Updated whenever society stream text arrives
    society_buffer: Vec<String>,

    /// Remote client sink for the web frontend sidecar.
    /// None unless `[web] enabled = true` — see core/remote.rs.
    pub remote: Option<super::remote::RemoteSink>,

    /// Dot-commands injected by the feed (`<vellumCmd cmd="..."/>`), waiting
    /// for the frontend to drain them into its dot-command dispatch.
    pub pending_client_commands: Vec<String>,
}

impl MessageProcessor {
    /// Drop character-sheet lines buffered by an earlier transport generation.
    /// They are not authoritative until the prompt commits them, so allowing
    /// them to survive a reconnect could authenticate the wrong Lich session.
    pub(crate) fn discard_pending_character_state(&mut self) {
        self.pending_character_lines.clear();
    }

    pub fn new(mut config: Config, saved_dialog_positions: SavedDialogPositions) -> Self {
        // Routing consults only [streams.routes]; normalize any legacy
        // drop list on our copy in case the caller's config didn't go
        // through Config::load_* (tests, embedders). Idempotent.
        config.streams.migrate_drop_list_to_routes();

        // Create parser with presets from config, resolving palette names to hex values
        let preset_list = config
            .colors
            .presets
            .iter()
            .map(|(id, preset)| {
                // Resolve palette names to actual hex values
                let resolved_fg = preset.fg.as_ref().map(|c| config.resolve_palette_color(c));
                let resolved_bg = preset.bg.as_ref().map(|c| config.resolve_palette_color(c));
                (id.clone(), resolved_fg, resolved_bg)
            })
            .collect();
        let event_patterns = config.event_patterns.clone();
        let parser = crate::parser::XmlParser::with_presets(preset_list, event_patterns);

        // Build highlight engine from config
        let highlights: Vec<_> = config.highlights.values().cloned().collect();
        let mut highlight_engine = super::highlight_engine::CoreHighlightEngine::new(highlights);
        highlight_engine.set_replace_enabled(config.highlight_settings.replace_enabled);

        let prompt_color_map = Self::build_prompt_color_map(&config);

        let mut processor = Self {
            config,
            prompt_color_map,
            parser,
            highlight_engine,
            current_stream: String::from("main"),
            current_segments: Vec::new(),
            injected_lines: std::collections::VecDeque::new(),
            sorter_gameobj: None,
            inv_scan: Default::default(),
            pending_container_ingest: None,
            pending_ready_stow: Vec::new(),
            pending_move_feedback: Vec::new(),
            pending_creature_effects: Vec::new(),
            game_line_no: 0,
            pending_recent_lines: Vec::new(),
            capture_recent_lines: false,
            pending_character_lines: Vec::new(),
            pending_day_pass_lines: Vec::new(),
            pending_silver: None,
            pending_group: Vec::new(),
            remote: None,
            pending_client_commands: Vec::new(),
            chunk_has_main_text: false,
            remote_chunk_has_story_text: false,
            suppress_remote_tap: false,
            chunk_has_familiar_text: false,
            emitting_familiar_separator: false,
            chunk_has_silent_updates: false,
            discard_current_stream: false,
            tts_windows: std::collections::HashSet::new(),
            claimed_indicator_ids: std::collections::HashSet::new(),
            server_time_offset: 0,
            inventory_buffer: Vec::new(),
            previous_inventory: Vec::new(),
            inv_service: crate::core::inventory_service::InventoryService::new(),
            reserve_buffer: Vec::new(),
            previous_reserve: Vec::new(),
            spells_buffer: Vec::new(),
            previous_spells: Vec::new(),
            spells_line_buffer: Vec::new(),
            skip_next_spells_clear: false,
            perception_buffer: Vec::new(),
            previous_room_components: std::collections::HashMap::new(),
            current_room_uid: None,
            last_remote_story_room: None,
            pending_remote_room_story: None,
            native_room_capture: None,
            pending_room_art: Vec::new(),
            room_image_index: Default::default(),
            squelch_matcher: None,
            squelch_matcher_ci: None,
            squelch_regexes: Vec::new(),
            has_redirect_highlights: false,
            redirect_matcher: None,
            redirect_literal_meta: Vec::new(),
            redirect_regexes: Vec::new(),
            text_stream_subscribers: std::collections::HashMap::new(),
            seen_streams: std::collections::BTreeMap::new(),
            newly_registered_container: None,
            pending_webui_handshake: None,
            pending_launch_urls: std::collections::VecDeque::new(),
            pending_sounds: Vec::new(),
            pending_status_actions: Vec::new(),
            pending_alerts: Vec::new(),
            pending_rumbles: Vec::new(),
            pending_evidence: Vec::new(),
            pending_pathcode: None,
            saved_dialog_positions,
            bounty_buffer: None,
            society_buffer: Vec::new(),
        };

        // Initialize squelch patterns from config
        processor.update_squelch_patterns();
        // Initialize redirect cache from config
        processor.update_redirect_cache();
        processor
    }

    /// Build the prompt character color map from config.
    /// Only single-character entries can ever match (the renderer compares
    /// one char at a time); first entry wins for duplicate characters.
    fn build_prompt_color_map(config: &Config) -> std::collections::HashMap<char, String> {
        let mut map = std::collections::HashMap::new();
        for pc in &config.colors.prompt_colors {
            let mut chars = pc.character.chars();
            if let (Some(ch), None) = (chars.next(), chars.next()) {
                if let Some(color) = pc.fg.as_ref().or(pc.color.as_ref()) {
                    map.entry(ch).or_insert_with(|| color.clone());
                }
            }
        }
        map
    }

    /// Resolved color for a prompt character, if configured.
    /// Used by the command echo path in AppCore::send_command.
    pub fn prompt_char_color(&self, ch: char) -> Option<&str> {
        self.prompt_color_map.get(&ch).map(String::as_str)
    }

    /// Take buffered bounty data (raw text, compact lines) if any.
    /// Returns Some((raw_text, compact_lines)) and clears the buffer.
    pub fn take_bounty_buffer(&mut self) -> Option<(String, Vec<String>)> {
        self.bounty_buffer.take()
    }

    /// Take buffered society lines if any.
    /// Returns the lines and clears the buffer.
    pub fn take_society_buffer(&mut self) -> Vec<String> {
        std::mem::take(&mut self.society_buffer)
    }

    /// Refresh internal config, parser presets, and caches after a reload.
    pub fn apply_config(&mut self, mut config: Config) {
        let apply_start = std::time::Instant::now();
        // Same legacy drop-list normalization as `new` — routing consults
        // only [streams.routes].
        config.streams.migrate_drop_list_to_routes();
        crate::config::Config::compile_highlight_patterns(&mut config.highlights);
        tracing::debug!(
            "apply_config: compiled highlight patterns in {:?}",
            apply_start.elapsed()
        );
        self.config = config;
        self.prompt_color_map = Self::build_prompt_color_map(&self.config);

        // Log loaded presets for debugging
        for (id, preset) in &self.config.colors.presets {
            tracing::debug!(
                "Loaded preset '{}': fg={:?}, bg={:?}",
                id,
                preset.fg,
                preset.bg
            );
        }

        // Resolve palette names to hex values when updating presets
        let preset_list = self
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
        self.parser.update_presets(preset_list);
        self.parser
            .update_event_patterns(self.config.event_patterns.clone());

        let cache_start = std::time::Instant::now();
        self.update_squelch_patterns();
        self.update_redirect_cache();
        tracing::debug!(
            "apply_config: updated caches in {:?}",
            cache_start.elapsed()
        );

        // Update highlight engine with new patterns
        self.update_highlights();
        tracing::debug!("apply_config: total elapsed {:?}", apply_start.elapsed());
    }

    /// Update the highlight engine with current config patterns.
    /// Called on startup and when highlights are reloaded.
    pub fn update_highlights(&mut self) {
        let start = std::time::Instant::now();
        let highlights: Vec<_> = self.config.highlights.values().cloned().collect();
        self.highlight_engine.update_patterns(highlights);
        self.highlight_engine
            .set_replace_enabled(self.config.highlight_settings.replace_enabled);
        tracing::debug!("update_highlights: rebuild in {:?}", start.elapsed());
    }

    /// Update only highlight-related configuration and caches.
    pub fn apply_highlights_config(
        &mut self,
        highlights: std::collections::HashMap<String, crate::config::HighlightPattern>,
        highlight_settings: crate::config::HighlightsConfig,
    ) {
        self.config.highlights = highlights;
        self.config.highlight_settings = highlight_settings;
        self.update_squelch_patterns();
        self.update_redirect_cache();
        self.update_highlights();
    }

    /// Skip the next Spells clearStream (used after requesting spell link updates).
    pub fn skip_next_spells_clear(&mut self) {
        self.skip_next_spells_clear = true;
    }

    /// Expand `:grin:`-style emoji shortcodes in the pending line, gated by
    /// the `ui.emoji_shortcodes` toggle. Called from the flush path right
    /// after highlights are applied.
    fn apply_emoji_shortcodes(&mut self) {
        if self.config.ui.emoji_shortcodes {
            super::emoji::apply_to_segments(&mut self.current_segments);
        }
    }

    /// Flush current text to appropriate window
    pub fn flush_current_stream(&mut self, ui_state: &mut UiState) {
        self.flush_current_stream_with_tts(ui_state, None);
    }

    /// Item classifier for the sorter transform, resolved lazily through
    /// the data pack (Lich folder > local store > bundled).
    fn sorter_gameobj(&mut self) -> std::sync::Arc<crate::core::gameobj_data::GameObjData> {
        if self.sorter_gameobj.is_none() {
            let resolved = crate::core::data_pack::resolve(
                &crate::core::data_pack::GAMEOBJ_DATA,
                self.config.map.lich_dir.as_deref(),
            );
            self.sorter_gameobj = Some(std::sync::Arc::new(
                crate::core::gameobj_data::GameObjData::parse(&resolved.content),
            ));
        }
        self.sorter_gameobj.clone().expect("initialized above")
    }

    /// Drop the cached classifier so the next use re-resolves sources
    /// (`.data reload`).
    pub fn reset_gameobj_cache(&mut self) {
        self.sorter_gameobj = None;
    }

    /// Mirror the `.sorter` toggle into the processor's live config
    /// (AppCore owns the persisted copy).
    pub fn set_sorter_enabled(&mut self, enabled: bool) {
        self.config.sorter.enabled = enabled;
    }

    /// Mirror the room-art toggle into the processor's config copy, which is
    /// what the sprite-injection path reads.
    pub fn set_room_images_enabled(&mut self, enabled: bool) {
        self.config.room_images.enabled = enabled;
    }

    /// Mirror the full sorter config (rules/order/labels/format) into the
    /// processor after an editor save.
    pub fn set_sorter_config(&mut self, sorter: crate::config::SorterConfig) {
        self.config.sorter = sorter;
    }

    /// Apply container contents captured from a main-stream look line into
    /// the registry. A look is a full snapshot, so replace (clear + refill).
    /// Registers the container if the `<container>` tag wasn't seen (the
    /// visible look carries the container as its first link; we don't have
    /// its title/target here, so a later `<container>` tag refines those).
    fn drain_pending_container_ingest(&mut self, game_state: &mut crate::core::state::GameState) {
        let Some((container_id, items)) = self.pending_container_ingest.take() else {
            return;
        };
        if game_state.objects.container(&container_id).is_none() {
            game_state
                .objects
                .register_container(container_id.clone(), String::new(), None);
        }
        game_state.objects.clear_container(&container_id);
        for item in items {
            game_state.objects.add_container_item(&container_id, item);
        }
    }

    /// Begin an INVENTORY FULL scan: the caller must send the returned
    /// command to the game. Reply lines are then squelched and parsed into
    /// per-item mark/register status, finalized at the next prompt. Returns
    /// None if a scan is already in flight.
    pub fn start_inventory_scan(&mut self) -> Option<&'static str> {
        if self.inv_scan.is_capturing() {
            return None;
        }
        self.inv_scan.start();
        Some(crate::core::game_objects::inv_scan::INVENTORY_FULL_COMMAND)
    }

    pub fn inventory_scan_in_flight(&self) -> bool {
        self.inv_scan.is_capturing()
    }

    /// Clear inventory cache to force next inventory update to render
    /// Should be called when a new inventory window is added
    pub fn clear_inventory_cache(&mut self) {
        self.previous_inventory.clear();
        tracing::debug!("Cleared inventory cache - next inventory update will render");
    }

    pub fn clear_reserve_cache(&mut self) {
        self.previous_reserve.clear();
        tracing::debug!("Cleared reserve cache - next reserve update will render");
    }

    pub fn set_spells_buffer(&mut self, buffer: Vec<Vec<TextSegment>>) {
        self.spells_buffer = buffer.clone();
        self.previous_spells = buffer;
    }

    pub fn get_spells_buffer(&self) -> &Vec<Vec<TextSegment>> {
        &self.spells_buffer
    }

    /// Populate a Spells window from the buffer
    /// Unlike inventory, spells are sent once at login, so we populate from buffer immediately
    /// Should be called when a new spells window is created
    pub fn populate_spells_window(&self, window_content: &mut crate::data::TextContent) {
        if self.spells_buffer.is_empty() {
            tracing::debug!(
                "Spells buffer is empty - new window will remain empty until data arrives"
            );
            return;
        }

        // Clear existing content
        window_content.lines.clear();

        // Add all buffered lines
        for line_segments in &self.spells_buffer {
            window_content.add_line(StyledLine {
                segments: line_segments.clone(),
                stream: String::from("Spells"),
                timestamp: None,
            });
        }

        tracing::debug!(
            "Populated new spells window from buffer with {} lines",
            window_content.lines.len()
        );
    }
}

#[cfg(test)]
mod tests;
