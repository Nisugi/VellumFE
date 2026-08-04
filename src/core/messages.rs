//! XML message processing
//!
//! Handles parsing and routing of XML messages from the game server.
//! Updates GameState and UiState based on incoming messages.

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
    pending_container_ingest:
        Option<(String, Vec<crate::core::game_objects::GameItem>)>,

    /// Track if chunk (since last prompt) has main stream text
    chunk_has_main_text: bool,

    /// Track if chunk (since last prompt) has silent updates
    pub chunk_has_silent_updates: bool,

    /// If true, discard text because no window exists for current stream
    discard_current_stream: bool,

    /// Windows whose layout def opts into TTS (`tts_speak`). Rebuilt by
    /// `AppCore::refresh_tts_windows` on layout load and editor saves.
    tts_windows: std::collections::HashSet<String>,

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

    squelch_matcher: Option<aho_corasick::AhoCorasick>,
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

    /// Pending sounds from highlight processing (to be transferred to GameState)
    pub pending_sounds: Vec<super::highlight_engine::SoundTrigger>,
    /// Custom-status changes from matched highlights, drained by AppCore.
    pub pending_status_actions: Vec<super::highlight_engine::StatusAction>,
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
    /// Registry entry for a held item from the `<left>`/`<right>` feed;
    /// None for an empty hand (the game sends the literal "Empty").
    fn hand_game_item(
        item: &str,
        link: Option<&crate::data::LinkData>,
    ) -> Option<crate::core::game_objects::GameItem> {
        if item.is_empty() || item.eq_ignore_ascii_case("empty") {
            return None;
        }
        Some(crate::core::game_objects::GameItem::new(
            link.map(|l| l.exist_id.clone()).unwrap_or_default(),
            link.map(|l| l.noun.clone()).unwrap_or_default(),
            item.to_string(),
        ))
    }

    /// Update any countdown windows whose id matches the provided id (case-sensitive).
    /// Falls back to window name for backward compatibility.
    fn update_countdown_by_id(
        &mut self,
        ui_state: &mut crate::data::UiState,
        countdown_id: &str,
        end_time: i64,
    ) {
        for (name, window) in ui_state
            .windows
            .iter_mut()
            .filter(|(_, w)| matches!(w.content, WindowContent::Countdown(_)))
        {
            if let WindowContent::Countdown(ref mut cd) = window.content {
                if cd.countdown_id == countdown_id || name == countdown_id {
                    cd.end_time = end_time;
                }
            }
        }
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
            remote: None,
            pending_client_commands: Vec::new(),
            chunk_has_main_text: false,
            chunk_has_silent_updates: false,
            discard_current_stream: false,
            tts_windows: std::collections::HashSet::new(),
            server_time_offset: 0,
            inventory_buffer: Vec::new(),
            previous_inventory: Vec::new(),
            reserve_buffer: Vec::new(),
            previous_reserve: Vec::new(),
            spells_buffer: Vec::new(),
            previous_spells: Vec::new(),
            spells_line_buffer: Vec::new(),
            skip_next_spells_clear: false,
            perception_buffer: Vec::new(),
            previous_room_components: std::collections::HashMap::new(),
            squelch_matcher: None,
            squelch_regexes: Vec::new(),
            has_redirect_highlights: false,
            redirect_matcher: None,
            redirect_literal_meta: Vec::new(),
            redirect_regexes: Vec::new(),
            text_stream_subscribers: std::collections::HashMap::new(),
            seen_streams: std::collections::BTreeMap::new(),
            newly_registered_container: None,
            pending_webui_handshake: None,
            pending_sounds: Vec::new(),
            pending_status_actions: Vec::new(),
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
                let resolved_fg = preset.fg.as_ref().map(|c| self.config.resolve_palette_color(c));
                let resolved_bg = preset.bg.as_ref().map(|c| self.config.resolve_palette_color(c));
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
        tracing::debug!(
            "apply_config: total elapsed {:?}",
            apply_start.elapsed()
        );
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

    /// Process a parsed XML element and update states
    pub fn process_element(
        &mut self,
        element: &ParsedElement,
        game_state: &mut GameState,
        ui_state: &mut UiState,
        room_components: &mut std::collections::HashMap<String, Vec<Vec<TextSegment>>>,
        current_room_component: &mut Option<String>,
        room_window_dirty: &mut bool,
        nav_room_id: &mut Option<String>,
        lich_room_id: &mut Option<String>,
        room_subtitle: &mut Option<String>,
        mut tts_manager: Option<&mut crate::tts::TtsManager>,
    ) {
        match element {
            ParsedElement::StreamWindow { id, subtitle, title } => {
                self.note_seen_stream(id, title.as_deref());
                // U3: record the stream as a window discovery for AppCore to
                // register as a bound, Hidden-by-default layout entry (the
                // processor can't reach the layout). Replaces the Stream
                // offer.
                ui_state.pending_window_discoveries.push(crate::data::WindowDiscovery {
                    id: id.clone(),
                    title: title.clone().unwrap_or_else(|| id.clone()),
                    kind: crate::data::WindowDiscoveryKind::Stream,
                    save: false,
                });
                self.handle_stream_window(
                    id,
                    subtitle.as_deref(),
                    room_subtitle,
                    room_window_dirty,
                );
            }
            ParsedElement::Component { id, value } => {
                self.handle_component(
                    id,
                    value,
                    game_state,
                    room_components,
                    current_room_component,
                    room_window_dirty,
                );
            }
            ParsedElement::CreatureStatus { id, attrs } => {
                // Standalone <crtrStatus> (outside a room objs component):
                // update the matching room creature's snapshot in place. The
                // component path re-derives flags wholesale, so only known
                // creatures need patching here - an id we haven't seen in
                // room objs yet gets its flags from the next component.
                self.chunk_has_silent_updates = true;
                let hashed_id = format!("#{}", id);
                if let Some(creature) = game_state
                    .room_creatures
                    .iter_mut()
                    .find(|c| c.id == hashed_id)
                {
                    let flags = crate::core::state::CreatureFlags::from_xml_attrs(
                        attrs.iter().map(|(n, v)| (n.as_str(), v.as_str())),
                    );
                    if creature.flags.as_ref() != Some(&flags) {
                        tracing::debug!(
                            "crtrStatus update for {} ({}): {:?}",
                            creature.name,
                            hashed_id,
                            flags
                        );
                        creature.flags = Some(flags);
                        game_state.room_creatures_generation += 1;
                    }
                }
            }
            ParsedElement::AppInfo { character } => {
                self.chunk_has_silent_updates = true;
                // Game feed is authoritative (the headless supervisor's
                // login-derived write-back is the fallback).
                game_state.character_name = Some(character.clone());
                tracing::debug!("Character name from <app>: {}", character);
            }
            ParsedElement::RoomId { id } => {
                *nav_room_id = Some(id.clone());
                *room_window_dirty = true;
                tracing::debug!("Room ID updated: {}", id);
            }
            ParsedElement::RoomMeta { attrs } => {
                self.chunk_has_silent_updates = true;
                if game_state
                    .room_meta
                    .update_from_attrs(attrs.iter().map(|(n, v)| (n.as_str(), v.as_str())))
                {
                    tracing::debug!("roommeta update: {:?}", game_state.room_meta);
                }
            }
            ParsedElement::StreamPush { id } => {
                self.flush_current_stream_with_tts(ui_state, tts_manager.as_deref_mut());
                self.note_seen_stream(id, None);
                self.current_stream = id.clone();

                // Check if any widget subscribes to this stream (using pre-built subscriber map)
                if self.stream_has_target_window(ui_state, id) {
                    // Stream has subscribers - route normally
                    self.discard_current_stream = false;
                } else {
                    // No subscribers - consult the route map / fallback
                    match self.resolve_orphaned_stream(id) {
                        RouteDecision::Discard => {
                            // Routed to discard (or migrated drop-list entry)
                            self.discard_current_stream = true;
                            tracing::debug!(
                                "Stream '{}' has no subscribers and routes to discard, dropping content",
                                id
                            );
                        }
                        decision => {
                            // Will deliver at flush time (first existing
                            // candidate window; never auto-created)
                            self.discard_current_stream = false;
                            tracing::debug!(
                                "Stream '{}' has no subscribers, will deliver per {:?}",
                                id,
                                decision
                            );
                        }
                    }
                }

                // Clear room components when room stream is pushed (only if window exists)
                if id == "room" && !self.discard_current_stream {
                    room_components.clear();
                    *current_room_component = None;
                    self.previous_room_components.clear(); // Clear change detection cache
                    *room_window_dirty = true;
                    tracing::debug!("Room stream pushed - cleared all room components");
                }

                // Clear inventory buffer when inv stream is pushed
                if id == "inv" {
                    self.inventory_buffer.clear();
                    tracing::debug!("Inventory stream pushed - cleared inventory buffer");
                }

                // Clear reserve buffer when reserve stream is pushed (each push
                // is a full snapshot of reserved items, like inv)
                if id == "reserve" {
                    self.reserve_buffer.clear();
                    tracing::debug!("Reserve stream pushed - cleared reserve buffer");
                }

                // Note: perception buffer is NOT cleared on pushStream
                // It's cleared on clearStream (which comes before all entries)
                // This allows entries from multiple push/pop pairs to accumulate
            }
            ParsedElement::StreamPop => {
                self.flush_current_stream_with_tts(ui_state, tts_manager.as_deref_mut());

                // Flush inventory buffer if we're leaving inv stream
                if self.current_stream == "inv" {
                    // Worn items into the registry from the same buffer the
                    // window uses (each line's first <a> link = one worn
                    // item; the "Your worn items are:" header and blank
                    // lines carry no link and are skipped). Runs regardless
                    // of whether an inventory window exists.
                    game_state
                        .objects
                        .set_worn_from_lines(&self.inventory_buffer);
                    self.flush_inventory_buffer(ui_state);
                }

                // Flush reserve buffer if we're leaving reserve stream
                if self.current_stream == "reserve" {
                    self.flush_reserve_buffer(ui_state);
                }

                // Flush spells line buffer if we're leaving Spells stream
                // Each <stream id="Spells">...</stream> block becomes one complete line
                if self.current_stream == "Spells" && !self.spells_line_buffer.is_empty() {
                    let segment_count = self.spells_line_buffer.len();
                    let line_segments = std::mem::take(&mut self.spells_line_buffer);
                    self.spells_buffer.push(line_segments);
                    tracing::debug!(
                        "Flushed Spells line buffer - accumulated {} segments into one line",
                        segment_count
                    );
                }

                // Note: perception buffer is NOT flushed on popStream
                // It accumulates across multiple push/pop pairs and flushes on clearStream

                // Check if stream was routed to a non-main window that actually exists
                // If so, skip the next prompt to avoid duplication in main window
                let stream_window = self.map_stream_to_window(&self.current_stream);

                // Only skip if: (1) maps to non-main AND (2) that window (or a tabbed text tab) exists
                if stream_window != "main"
                    && self.stream_has_target_window(ui_state, &self.current_stream)
                {
                    self.chunk_has_silent_updates = true;
                    tracing::debug!(
                        "Stream '{}' routed to existing '{}' window - will skip next prompt",
                        self.current_stream,
                        stream_window
                    );
                } else if stream_window != "main" {
                    tracing::debug!("Stream '{}' would map to '{}' but window doesn't exist - content went to main, won't skip prompt",
                        self.current_stream, stream_window);
                }

                // Reset discard flag when returning to main stream
                self.discard_current_stream = false;
                self.current_stream = String::from("main");
            }
            ParsedElement::ClearStream { id } => {
                // ClearStream clears the window content for a fresh update
                if id == "percWindow" {
                    // Clear the buffer for new entries
                    self.perception_buffer.clear();
                    // Clear the window content
                    for window in ui_state.windows.values_mut() {
                        if let WindowContent::Perception(ref mut data) = window.content {
                            data.entries.clear();
                            data.last_update = chrono::Utc::now().timestamp();
                            data.generation = data.generation.wrapping_add(1);
                        }
                    }
                    tracing::debug!("ClearStream percWindow - cleared buffer and window");
                } else if id == "Spells" {
                    if self.skip_next_spells_clear {
                        self.skip_next_spells_clear = false;
                        tracing::debug!("ClearStream Spells - skipped one-time clear");
                    } else {
                        // Clear the spells buffer for new data
                        self.spells_buffer.clear();
                        self.spells_line_buffer.clear();
                        self.previous_spells.clear();
                        // Clear the window content
                        for window in ui_state.windows.values_mut() {
                            if let WindowContent::Spells(ref mut content) = window.content {
                                content.lines.clear();
                            }
                        }
                        tracing::debug!("ClearStream Spells - cleared buffer and window(s)");
                    }
                } else if id == "reserve" {
                    // Clear the reserve buffers and window content for a fresh snapshot
                    self.reserve_buffer.clear();
                    self.previous_reserve.clear();
                    for window in ui_state.windows.values_mut() {
                        if let WindowContent::Reserve(ref mut content) = window.content {
                            content.lines.clear();
                        }
                    }
                    tracing::debug!("ClearStream reserve - cleared buffer and window(s)");
                } else {
                    // Generic clearStream handling for text windows
                    // Check if any text window subscribes to this stream and clear it
                    let mut cleared_any = false;
                    for (window_name, window) in ui_state.windows.iter_mut() {
                        if let WindowContent::Text(ref mut content) = window.content {
                            if content.streams.iter().any(|s| s.eq_ignore_ascii_case(id)) {
                                content.lines.clear();
                                content.scroll_offset = 0;
                                content.generation = content.generation.wrapping_add(1);
                                cleared_any = true;
                                tracing::debug!(
                                    "ClearStream '{}' - cleared text window '{}'",
                                    id,
                                    window_name
                                );
                            }
                        }
                    }
                    if !cleared_any {
                        tracing::trace!("ClearStream '{}' - no subscribers found", id);
                    }
                }
            }
            ParsedElement::Prompt { time, text } => {
                // Finish current stream before prompt
                self.flush_current_stream_with_tts(ui_state, tts_manager.as_deref_mut());

                // An INVENTORY FULL scan ends at the prompt: write the
                // collected mark/register statuses into the registry.
                if self.inv_scan.is_capturing() {
                    for (id, status) in self.inv_scan.finish() {
                        game_state.objects.set_status(id, status);
                    }
                }

                // Container contents extracted from a main-stream look line
                // during flush (which lacks game_state) land here.
                self.drain_pending_container_ingest(game_state);

                // Flush perception buffer on prompt (after all entries have accumulated)
                if !self.perception_buffer.is_empty() {
                    self.flush_perception_buffer(ui_state);
                }

                // Flush spells buffer on prompt (after all spells have accumulated)
                if !self.spells_buffer.is_empty() {
                    self.flush_spells_buffer(ui_state);
                    // Mirror the spellbook onto GameState as STYLED lines so
                    // headless/remote clients get the full active-spell list —
                    // with spell coloring and links — without a Spells window.
                    // spells_buffer is already Vec<Vec<TextSegment>>, so this
                    // keeps the styling instead of flattening it. Bump only on
                    // real change.
                    let lines: Vec<crate::data::widget::StyledLine> = self
                        .spells_buffer
                        .iter()
                        .map(|segs| crate::data::widget::StyledLine {
                            segments: segs.clone(),
                            stream: "Spells".to_string(),
                            timestamp: None,
                        })
                        .collect();
                    if game_state.spellbook != lines {
                        game_state.spellbook = lines;
                        game_state.spellbook_generation += 1;
                    }
                }

                // Decide whether to show this prompt based on chunk tracking
                // Skip if: no main text was received since last prompt AND prompt text is unchanged
                // This handles both "silent updates only" and "empty chunk" cases
                // But we always show the prompt if it changed (e.g., "R>" -> ">" when roundtime ends)
                let prompt_changed = text.trim() != game_state.last_prompt.trim();
                let should_skip = !self.chunk_has_main_text && !prompt_changed;

                // Always reset to main stream when a prompt is received
                // (prompts mark the end of a server response, returning control to main)
                self.current_stream = String::from("main");

                if should_skip {
                    // Skip this prompt - no main text since last prompt
                } else if !text.trim().is_empty() {
                    // Store the prompt in game state for command echoes
                    game_state.last_prompt = text.clone();

                    // Render prompt with per-character coloring
                    for ch in text.chars() {
                        let color = self
                            .prompt_color_map
                            .get(&ch)
                            .cloned()
                            .unwrap_or_else(|| "#808080".to_string()); // Default dark gray

                        self.current_segments.push(TextSegment {
                            text: ch.to_string(),
                            fg: Some(color),
                            bg: None,
                            bold: false,
                            mono: false,
                            span_type: SpanType::Normal,
                            link_data: None,
                            custom_emoji: None,
                        });
                    }

                    // Finish prompt line
                    self.flush_current_stream_with_tts(ui_state, tts_manager);
                }

                // Extract server time offset for countdown synchronization
                if let Ok(server_time) = time.parse::<i64>() {
                    let local_time = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_else(|_| {
                            tracing::warn!("System time before UNIX epoch, using 0");
                            std::time::Duration::from_secs(0)
                        })
                        .as_secs() as i64;
                    self.server_time_offset = server_time - local_time;
                    // Update game_time to the prompt's server timestamp
                    game_state.game_time = server_time;
                }

                // Reset chunk tracking for next prompt
                self.chunk_has_main_text = false;
                self.chunk_has_silent_updates = false;

                // Reset discard flag - prompts always return to main stream
                self.discard_current_stream = false;
            }
            ParsedElement::Text {
                content,
                fg_color,
                bg_color,
                bold,
                mono,
                span_type,
                link_data,
                stream,
            } => {
                // Use the stream from the element (inline <stream id="...">) if different from current
                // This handles both <pushStream> (which sets current_stream) and <stream> (inline)
                let effective_stream = if !stream.is_empty()
                    && stream.as_str() != self.current_stream.as_str()
                {
                    tracing::debug!(
                        "Inline stream tag: switching from '{}' to '{}' for this text element",
                        self.current_stream, stream
                    );
                    stream.as_str()
                } else {
                    self.current_stream.as_str()
                };

                // Special handling for inline Spells stream - accumulate segments into line buffer
                // Spells are sent once at login with inline <stream id="Spells"> tags
                // We accumulate segments until the </stream> tag, then flush to buffer
                if effective_stream == "Spells" {
                    self.chunk_has_silent_updates = true;

                    // Map parser SpanType to data layer SpanType
                    use crate::data::SpanType as DataSpanType;
                    use crate::parser::SpanType as ParserSpanType;
                    let data_span_type = match span_type {
                        ParserSpanType::Normal => DataSpanType::Normal,
                        ParserSpanType::Link => DataSpanType::Link,
                        ParserSpanType::Monsterbold => DataSpanType::Monsterbold,
                        ParserSpanType::Spell => DataSpanType::Spell,
                        ParserSpanType::Speech => DataSpanType::Speech,
                    };

                    // Create the text segment
                    let segment = TextSegment {
                        text: content.clone(),
                        fg: fg_color.clone(),
                        bg: bg_color.clone(),
                        bold: *bold,
                        mono: *mono,
                        span_type: data_span_type,
                        link_data: link_data.clone(),
                        custom_emoji: None,
                    };

                    // Accumulate this segment in the current line buffer
                    // It will be flushed to spells_buffer when we see </stream>
                    self.spells_line_buffer.push(segment);
                    tracing::trace!(
                        "Accumulated Spells segment: '{}'",
                        if content.len() > 50 { format!("{}...", &content[..50]) } else { content.to_string() }
                    );
                    return; // Don't add to current_segments
                }

                // Discard text if we're in a discarded stream (e.g., no Spells/inv/room window)
                if self.discard_current_stream {
                    self.chunk_has_silent_updates = true;
                    tracing::debug!(
                        "Discarding text from stream '{}': {:?}",
                        self.current_stream,
                        content.chars().take(50).collect::<String>()
                    );
                    return;
                }

                // Try to extract Lich room ID from room name format: [Name - ID]
                // Example: "[Emberthorn Refuge, Bowery - 33711]"
                if self.current_stream == "main" && content.contains('[') && content.contains(" - ")
                {
                    // Try to match pattern: [...  - NUMBER]
                    if let Some(dash_pos) = content.rfind(" - ") {
                        if let Some(bracket_pos) = content[dash_pos..].find(']') {
                            let id_start = dash_pos + 3; // After " - "
                            let id_end = dash_pos + bracket_pos;
                            if id_start < content.len() && id_end <= content.len() {
                                let potential_id = &content[id_start..id_end].trim();

                                // Check if it's all digits (room ID)
                                if !potential_id.is_empty()
                                    && potential_id.chars().all(|c| c.is_ascii_digit())
                                {
                                    *lich_room_id = Some(potential_id.to_string());
                                    *room_window_dirty = true;
                                    tracing::debug!(
                                        "Extracted Lich room ID from room name: {}",
                                        potential_id
                                    );
                                }
                            }
                        }
                    }
                }

                // Map parser SpanType to data layer SpanType
                use crate::data::SpanType as DataSpanType;
                use crate::parser::SpanType as ParserSpanType;
                let data_span_type = match span_type {
                    ParserSpanType::Normal => DataSpanType::Normal,
                    ParserSpanType::Link => DataSpanType::Link,
                    ParserSpanType::Monsterbold => DataSpanType::Monsterbold,
                    ParserSpanType::Spell => DataSpanType::Spell,
                    ParserSpanType::Speech => DataSpanType::Speech,
                };

                self.current_segments.push(TextSegment {
                    text: content.clone(),
                    fg: fg_color.clone(),
                    bg: bg_color.clone(),
                    bold: *bold,
                    mono: *mono,
                    span_type: data_span_type,
                    link_data: link_data.clone(),
                    custom_emoji: None,
                });
            }
            ParsedElement::RoundTime { value } => {
                // Roundtime is sent as an absolute server timestamp when it ends.
                let end_time_server = *value as i64;
                game_state.roundtime_end = Some(end_time_server);

                // Update countdowns that listen for "roundtime"
                self.update_countdown_by_id(ui_state, "roundtime", end_time_server);
            }
            ParsedElement::CastTime { value } => {
                // Casttime is sent as an absolute server timestamp when it ends.
                let end_time_server = *value as i64;
                game_state.casttime_end = Some(end_time_server);

                // Update countdowns that listen for "casttime"
                self.update_countdown_by_id(ui_state, "casttime", end_time_server);
            }
            ParsedElement::Event {
                event_type,
                action,
                duration,
            } => {
                // Config [event_patterns] regexes matched on game text (stun
                // rounds/recovery, raise dead, ...). The consumer was lost in
                // the Beta 2 rewrite - the parser kept emitting these while
                // nothing fed the stuntime countdown. end_time lives in the
                // server clock domain, like RoundTime/CastTime above.
                let countdown_id = match event_type.as_str() {
                    "stun" => "stuntime",
                    "rt" => "roundtime",
                    "ct" => "casttime",
                    other => other,
                };
                match action {
                    crate::config::EventAction::Set => {
                        if *duration > 0 {
                            let end_time = chrono::Utc::now().timestamp()
                                + self.server_time_offset
                                + *duration as i64;
                            self.update_countdown_by_id(ui_state, countdown_id, end_time);
                        }
                    }
                    crate::config::EventAction::Clear => {
                        self.update_countdown_by_id(ui_state, countdown_id, 0);
                    }
                    // Increment is reserved in the config schema; nothing
                    // emits it yet.
                    crate::config::EventAction::Increment => {}
                }
            }
            ParsedElement::VellumTimer { id, value } => {
                // Script-facing countdown feed (<vellumTimer id=.. value=..>):
                // value is the absolute epoch end time in the server clock
                // domain, like RoundTime/CastTime; 0 or a past time clears.
                self.update_countdown_by_id(ui_state, id, (*value).max(0));
            }
            ParsedElement::VellumCommand { command } => {
                // Feed-driven client commands (Lich scripts). Dot-commands
                // only: the frontends drain this queue into their normal
                // dot-command dispatch, so anything else could round-trip
                // back to the game — refuse it.
                if command.starts_with('.') {
                    self.pending_client_commands.push(command.clone());
                } else {
                    tracing::warn!(
                        "vellumCmd rejected (only dot-commands are allowed): {command}"
                    );
                }
            }
            ParsedElement::LeftHand { item, link } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                game_state.left_hand = if item.is_empty() {
                    None
                } else {
                    Some(item.clone())
                };
                game_state.objects.set_hand(
                    crate::core::game_objects::Hand::Left,
                    Self::hand_game_item(item, link.as_ref()),
                );

                // Update left hand widget if it exists (support legacy and new names)
                for name in ["left", "left_hand"] {
                    if let Some(left_hand_window) =
                        ui_state.get_window_by_type_mut(crate::data::WidgetType::Hand, Some(name))
                    {
                        if let WindowContent::Hand {
                            item: ref mut window_item,
                            link: ref mut window_link,
                        } = left_hand_window.content
                        {
                            let item_changed = *window_item != game_state.left_hand;
                            *window_item = game_state.left_hand.clone();
                            // A refresh that repeats the same item without
                            // exist/noun must not clobber a live link; only
                            // replace it when the item changed or the update
                            // carries one.
                            if link.is_some() || item_changed {
                                *window_link = link.clone();
                            }
                        }
                        break;
                    }
                }
            }
            ParsedElement::RightHand { item, link } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                game_state.right_hand = if item.is_empty() {
                    None
                } else {
                    Some(item.clone())
                };
                game_state.objects.set_hand(
                    crate::core::game_objects::Hand::Right,
                    Self::hand_game_item(item, link.as_ref()),
                );

                // Update right hand widget if it exists (support legacy and new names)
                for name in ["right", "right_hand"] {
                    if let Some(right_hand_window) =
                        ui_state.get_window_by_type_mut(crate::data::WidgetType::Hand, Some(name))
                    {
                        if let WindowContent::Hand {
                            item: ref mut window_item,
                            link: ref mut window_link,
                        } = right_hand_window.content
                        {
                            let item_changed = *window_item != game_state.right_hand;
                            *window_item = game_state.right_hand.clone();
                            // A refresh that repeats the same item without
                            // exist/noun must not clobber a live link; only
                            // replace it when the item changed or the update
                            // carries one.
                            if link.is_some() || item_changed {
                                *window_link = link.clone();
                            }
                        }
                        break;
                    }
                }
            }
            ParsedElement::SpellHand { spell } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                game_state.spell = if spell.is_empty() {
                    None
                } else {
                    Some(spell.clone())
                };

                // Update spell hand widget if it exists (support legacy and new names)
                for name in ["spell", "spell_hand"] {
                    if let Some(spell_hand_window) =
                        ui_state.get_window_by_type_mut(crate::data::WidgetType::Hand, Some(name))
                    {
                        if let WindowContent::Hand { ref mut item, .. } = spell_hand_window.content
                        {
                            *item = game_state.spell.clone();
                        }
                        break;
                    }
                }

                tracing::debug!("Updated spell hand: {:?}", game_state.spell);
            }
            ParsedElement::Compass { directions } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                game_state.compass_dirs = directions.clone();

                // Update compass widget if it exists (singleton)
                if let Some(compass_window) =
                    ui_state.get_window_by_type_mut(crate::data::WidgetType::Compass, None)
                {
                    if let WindowContent::Compass(ref mut compass_data) = compass_window.content {
                        compass_data.directions = directions.clone();
                    }
                }
            }
            ParsedElement::InjuryImage { id, name } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Convert injury name to level: Injury1-3 = 1-3, Scar1-3 = 4-6
                // When name equals body part ID, it means cleared (level 0)
                let level = if name == id {
                    0 // Cleared - name equals body part ID
                } else if name.starts_with("Injury") {
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
                    0 // Unknown injury type - treat as cleared
                };

                // Game state owns injuries (remote clients and windows added
                // mid-session read from here); widget copy below.
                if level == 0 {
                    game_state.injuries.remove(id);
                } else {
                    game_state.injuries.insert(id.clone(), level);
                }

                // Update injury doll widget if it exists (singleton)
                if let Some(injury_window) =
                    ui_state.get_window_by_type_mut(crate::data::WidgetType::InjuryDoll, None)
                {
                    if let WindowContent::InjuryDoll(ref mut injury_data) = injury_window.content {
                        injury_data.set_injury(id.clone(), level);
                        tracing::debug!("Updated injury: {} to level {} ({})", id, level, name);
                    }
                }
            }
            ParsedElement::InjuryPopupData {
                popup_id,
                injuries,
                clear,
            } => {
                self.chunk_has_silent_updates = true;

                // Update the injuries popup if it's active and matches the popup_id
                if let Some(ref mut popup) = ui_state.injuries_popup {
                    if popup.dialog_id == *popup_id {
                        if *clear {
                            popup.injuries.clear();
                            tracing::debug!("Cleared injuries popup: {}", popup_id);
                        } else {
                            for (body_part, name) in injuries {
                                popup.set_injury_from_name(body_part, name);
                                tracing::debug!(
                                    "Updated injuries popup {}: {} -> {}",
                                    popup_id,
                                    body_part,
                                    name
                                );
                            }
                        }
                    }
                }
            }
            ParsedElement::ProgressBar {
                id,
                value,
                max,
                text,
            } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Update progress bar widget(s) whose progress_id matches the incoming id
                for window in ui_state
                    .windows
                    .values_mut()
                    .filter(|w| matches!(w.content, WindowContent::Progress(_)))
                {
                    if let WindowContent::Progress(ref mut data) = window.content {
                        if data.progress_id == *id {
                            data.value = *value; // Store actual values, not percentages
                            data.max = *max;
                            data.label = text.clone();
                        }
                    }
                }

                // Also update vitals if it's a known vital
                // Guard against division by zero when max is 0
                if *max > 0 {
                    match id.as_str() {
                        "health" => game_state.vitals.health = (*value * 100 / *max) as u8,
                        "mana" => game_state.vitals.mana = (*value * 100 / *max) as u8,
                        "stamina" => game_state.vitals.stamina = (*value * 100 / *max) as u8,
                        "spirit" => game_state.vitals.spirit = (*value * 100 / *max) as u8,
                        _ => {}
                    }
                }

                // Update MiniVitals state for minivitals dialog (GS4 and DR)
                // This captures the full text for display options (numbers_only, current_only)
                // Note: DR uses "concentration" instead of "mana"
                match id.as_str() {
                    "health" | "mana" | "concentration" | "stamina" | "spirit" => {
                        game_state.minivitals.update_vital(id, *value, *max, text.clone());
                    }
                    _ => {}
                }

                // Update GS4 experience state for expr dialog elements
                // (the exact-exp attributes on the mindState bar arrive as a
                // separate MindStateExp element right after this one)
                match id.as_str() {
                    "mindState" => {
                        game_state.gs4_experience.update_mind_state(*value, text.clone());
                    }
                    "nextLvlPB" => {
                        game_state.gs4_experience.update_next_level(*value, text.clone());
                    }
                    "encumlevel" => {
                        game_state.encumbrance.update_level(*value, text.clone());
                    }
                    _ => {}
                }
            }
            ParsedElement::MindStateExp {
                field_exp,
                max_field_exp,
                exp,
                ascension_exp,
                until_next,
                fashlonae,
                lumnis,
                rpa,
            } => {
                self.chunk_has_silent_updates = true;
                game_state.gs4_experience.update_exp_attrs(
                    *field_exp,
                    *max_field_exp,
                    *exp,
                    *ascension_exp,
                    *until_next,
                    *fashlonae,
                    *lumnis,
                    *rpa,
                );
            }
            ParsedElement::Label { id, value } => {
                self.chunk_has_silent_updates = true;

                // Update GS4 experience state for expr dialog elements
                if id == "yourLvl" {
                    game_state.gs4_experience.update_level(value.clone());
                }
                // Update encumbrance blurb label
                if id == "encumblurb" {
                    game_state.encumbrance.update_blurb(value.clone());
                }
            }
            ParsedElement::Spell { text } => {
                self.chunk_has_silent_updates = true; // Mark as silent update
                game_state.spell = Some(text.clone());
            }
            ParsedElement::StatusIndicator { id, active } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Update game state. The parser strips the "Icon" prefix
                // but preserves casing (e.g. "BLEEDING"), so match
                // case-insensitively like the indicator widgets below do.
                match id.to_ascii_lowercase().as_str() {
                    "stunned" => game_state.status.stunned = *active,
                    "bleeding" => game_state.status.bleeding = *active,
                    "hidden" => game_state.status.hidden = *active,
                    "invisible" => game_state.status.invisible = *active,
                    "webbed" => game_state.status.webbed = *active,
                    "dead" => game_state.status.dead = *active,
                    "standing" => game_state.status.standing = *active,
                    "kneeling" => game_state.status.kneeling = *active,
                    "sitting" => game_state.status.sitting = *active,
                    "prone" => game_state.status.prone = *active,
                    _ => {}
                }

                // Update Indicator windows whose indicator_id matches
                for (_name, window) in ui_state.windows.iter_mut() {
                    match &mut window.content {
                        crate::data::WindowContent::Indicator(ref mut indicator_data) => {
                            if indicator_data
                                .indicator_id
                                .eq_ignore_ascii_case(id.as_str())
                            {
                                indicator_data.active = *active;
                                tracing::trace!(
                                    "Updated indicator '{}' active={}",
                                    indicator_data.indicator_id,
                                    active
                                );
                            }
                        }
                        crate::data::WindowContent::Dashboard { indicators } => {
                            let mut found = false;
                            for (indicator_id, value) in indicators.iter_mut() {
                                if indicator_id.eq_ignore_ascii_case(id.as_str()) {
                                    *value = if *active { 1 } else { 0 };
                                    found = true;
                                    break;
                                }
                            }
                            if !found {
                                indicators.push((id.clone(), if *active { 1 } else { 0 }));
                            }
                        }
                        _ => {}
                    }
                }
            }
            ParsedElement::QuickbarOpen { id, title } => {
                self.chunk_has_silent_updates = true;

                let entry = ui_state
                    .quickbars
                    .entry(id.clone())
                    .or_insert(QuickbarData {
                        id: id.clone(),
                        title: title.clone(),
                        entries: Vec::new(),
                    });
                if title.is_some() {
                    entry.title = title.clone();
                }
                if !ui_state.quickbar_order.contains(id) {
                    ui_state.quickbar_order.push(id.clone());
                }
                if ui_state.active_quickbar_id.is_none() {
                    ui_state.active_quickbar_id = Some(id.clone());
                }
            }
            ParsedElement::QuickbarEntries { id, clear, entries } => {
                self.chunk_has_silent_updates = true;

                let entry = ui_state
                    .quickbars
                    .entry(id.clone())
                    .or_insert(QuickbarData {
                        id: id.clone(),
                        title: None,
                        entries: Vec::new(),
                    });
                if *clear {
                    entry.entries.clear();
                }
                entry.entries.extend(entries.clone());
                if !ui_state.quickbar_order.contains(id) {
                    ui_state.quickbar_order.push(id.clone());
                }
                if ui_state.active_quickbar_id.is_none() {
                    ui_state.active_quickbar_id = Some(id.clone());
                }
            }
            ParsedElement::QuickbarSwitch { id } => {
                self.chunk_has_silent_updates = true;

                ui_state.active_quickbar_id = Some(id.clone());
                if !ui_state.quickbar_order.contains(id) {
                    ui_state.quickbar_order.push(id.clone());
                }
            }
            ParsedElement::DialogOpen { id, title, save } => {
                self.chunk_has_silent_updates = true;
                tracing::debug!("DialogOpen received: id={}, title={:?}, save={}", id, title, save);

                // U3: dialogs reaching here are non-resident (resident ones
                // are mined into panels). Hidden-until-shown: a dialog the
                // user never showed doesn't pop up, but the store still
                // ingests its data so the window can be shown later.
                if !Self::dialog_should_popup(ui_state, id) {
                    tracing::debug!("DialogOpen suppressed (not shown by user): id={}", id);
                    return;
                }

                // Handle injuries popup for viewing another player's injuries
                // Dialog ID format: "injuries-PLAYERID" (e.g., "injuries-10154507")
                // Title format: "Zoleta's Injuries"
                if id.starts_with("injuries-") {
                    tracing::debug!("DialogOpen creating injuries popup: id={}", id);
                    // Extract player name from title (e.g., "Zoleta's Injuries" -> "Zoleta")
                    let player_name = title
                        .as_ref()
                        .and_then(|t| t.strip_suffix("'s Injuries"))
                        .unwrap_or("Unknown")
                        .to_string();

                    ui_state.injuries_popup = Some(crate::data::InjuriesPopupState::new(
                        id.clone(),
                        player_name,
                    ));
                    return;
                }

                // Map dialog ID to template name (they may differ, e.g., "expr" -> "gs4_experience")
                let template_name = Config::dialog_id_to_template(id);

                // If a widget template exists for this dialog, add it to layout
                // instead of a popup. Queue the DIALOG ID (not the template
                // name) so process_pending_window_additions can tag the created
                // window with its binding — the U2 identity that ties the feed
                // to the placed window regardless of its display name.
                if Config::get_window_template(template_name).is_some() {
                    tracing::debug!("DialogOpen redirected to widget: id={} -> template={}", id, template_name);
                    if !ui_state.pending_window_additions.contains(id) {
                        ui_state.pending_window_additions.push(id.clone());
                    }
                    return;
                }
                tracing::debug!("DialogOpen creating popup: id={}", id);

                // Preserve position from currently open dialog with same ID
                let preserved_pos = ui_state
                    .active_dialog
                    .as_ref()
                    .filter(|d| d.id == *id)
                    .map(|d| (d.position, d.size));

                // Determine position: preserve existing, load from saved, or None (will center)
                let (position, size) = if let Some((pos, sz)) = preserved_pos {
                    (pos, sz)
                } else if *save {
                    // Load from saved positions if save='t' and no current dialog
                    self.saved_dialog_positions
                        .dialogs
                        .get(id)
                        .map(|p| (Some((p.x, p.y)), p.width.zip(p.height)))
                        .unwrap_or((None, None))
                } else {
                    (None, None)
                };

                // No template - show as popup dialog. Seed the store (so
                // re-showing after hide works) preserving any controls the
                // dialog already accumulated, then set the title/geometry.
                {
                    let dialog = ui_state.dialog_slot_mut(id);
                    dialog.title = title.clone();
                    dialog.position = position;
                    dialog.size = size;
                    dialog.save_position = *save;
                }
                ui_state.show_dialog_from_store(id);
                if let Some(dialog) = ui_state.active_dialog.as_mut() {
                    dialog.position = position;
                    dialog.size = size;
                    dialog.save_position = *save;
                }
            }
            ParsedElement::DialogButtons { id, clear, buttons } => {
                self.chunk_has_silent_updates = true;
                // Always INGEST into the store (even for hidden dialogs);
                // policy only gates DISPLAY, synced below.
                let show = Self::dialog_should_popup(ui_state, id);
                let dialog = ui_state.dialog_slot_mut(id);
                if *clear {
                    dialog.buttons.clear();
                }
                // Re-sent controls REPLACE their same-id entry — blind
                // extend piled up duplicate buttons on every dialogData
                // refresh (seen live: combat's target/attack repeating).
                // Id-less buttons still append.
                for button in buttons {
                    let existing = (!button.id.is_empty())
                        .then(|| dialog.buttons.iter_mut().find(|b| b.id == button.id))
                        .flatten();
                    match existing {
                        Some(slot) => *slot = button.clone(),
                        None => dialog.buttons.push(button.clone()),
                    }
                }
                if dialog.selected >= dialog.buttons.len() {
                    dialog.selected = 0;
                }
                self.sync_shown_dialog(ui_state, id, show);
            }
            ParsedElement::DialogDropDowns { id, clear, dropdowns } => {
                self.chunk_has_silent_updates = true;
                let show = Self::dialog_should_popup(ui_state, id);
                let dialog = ui_state.dialog_slot_mut(id);
                if *clear {
                    dialog.dropdowns.clear();
                }
                for dropdown in dropdowns {
                    match dialog.dropdowns.iter_mut().find(|d| d.id == dropdown.id) {
                        Some(slot) => *slot = dropdown.clone(),
                        None => dialog.dropdowns.push(dropdown.clone()),
                    }
                }
                self.sync_shown_dialog(ui_state, id, show);
            }
            ParsedElement::DialogPanelOpen { id, title, save } => {
                self.chunk_has_silent_updates = true;
                // Resident dialogs that already have a dedicated widget
                // (Buffs/Debuffs/Cooldowns/injuries/encum/expr/stance/...)
                // are mined into those panels — don't offer them as generic
                // dialog panels too. Only ids WITHOUT a template become
                // dockable dialog panels (combat, befriend, ...).
                if Config::get_window_template(Config::dialog_id_to_template(id)).is_some() {
                    return;
                }
                // U3: record the resident dialog as a DialogPanel discovery
                // for AppCore to register as a bound, Hidden-by-default
                // dockable-panel layout entry. Replaces the resident Dialog
                // offer. Seed the store title so the panel renders when shown.
                ui_state.pending_window_discoveries.push(crate::data::WindowDiscovery {
                    id: id.clone(),
                    title: title.clone().unwrap_or_else(|| id.clone()),
                    kind: crate::data::WindowDiscoveryKind::DialogPanel,
                    save: *save,
                });
                let dialog = ui_state.dialog_slot_mut(id);
                if dialog.title.is_none() {
                    dialog.title = title.clone();
                }
            }
            ParsedElement::DialogControls {
                id,
                clear,
                links,
                images,
                spinboxes,
                skins,
            } => {
                self.chunk_has_silent_updates = true;
                let show = Self::dialog_should_popup(ui_state, id);
                let dialog = ui_state.dialog_slot_mut(id);
                if *clear {
                    dialog.links.clear();
                    dialog.images.clear();
                    dialog.spinboxes.clear();
                    dialog.skins.clear();
                }
                for skin in skins {
                    match dialog.skins.iter_mut().find(|s| s.id == skin.id) {
                        Some(slot) => *slot = skin.clone(),
                        None => dialog.skins.push(skin.clone()),
                    }
                }
                for link in links {
                    match dialog.links.iter_mut().find(|l| l.id == link.id) {
                        Some(slot) => *slot = link.clone(),
                        None => dialog.links.push(link.clone()),
                    }
                }
                for image in images {
                    match dialog.images.iter_mut().find(|i| i.id == image.id) {
                        Some(slot) => *slot = image.clone(),
                        None => dialog.images.push(image.clone()),
                    }
                }
                for spinbox in spinboxes {
                    match dialog.spinboxes.iter_mut().find(|s| s.id == spinbox.id) {
                        // Preserve a user-edited value across re-sends: only
                        // take the game's value if bounds changed.
                        Some(slot) => {
                            slot.min = spinbox.min;
                            slot.max = spinbox.max;
                            slot.layout = spinbox.layout.clone();
                        }
                        None => dialog.spinboxes.push(spinbox.clone()),
                    }
                }
                self.sync_shown_dialog(ui_state, id, show);
            }
            ParsedElement::DialogFields {
                id,
                clear,
                fields,
                labels,
            } => {
                self.chunk_has_silent_updates = true;
                let show = Self::dialog_should_popup(ui_state, id);
                let dialog = ui_state.dialog_slot_mut(id);
                if *clear {
                    dialog.fields.clear();
                    dialog.labels.clear();
                    // display_labels are the standalone (unpaired) rows, e.g.
                    // a resident panel's positioned label grid; a clear='t'
                    // frame rebuilds them, so drop the old set too.
                    dialog.display_labels.clear();
                    dialog.focused_field = None;
                }

                if !labels.is_empty() {
                    // Separate labels into:
                    // - display_labels: standalone labels (not paired with any field)
                    // - labels: labels that are paired with input fields
                    //
                    // A label is "paired" if its ID is a prefix of a field ID
                    // e.g., "deposit" is paired with "depositAmount"
                    let mut paired_labels = Vec::new();
                    let mut standalone_labels = Vec::new();

                    for label in labels.iter() {
                        let is_paired = fields.iter().any(|field| {
                            field
                                .id
                                .to_lowercase()
                                .starts_with(&label.id.to_lowercase())
                        });

                        let dialog_label = crate::data::DialogLabel {
                            id: label.id.clone(),
                            value: label.value.clone(),
                            layout: label.layout.clone(),
                        };

                        if is_paired {
                            paired_labels.push(dialog_label);
                        } else {
                            standalone_labels.push(dialog_label);
                        }
                    }

                    dialog.labels = paired_labels;
                    dialog.display_labels = standalone_labels;
                }

                let mut focused_index = None;
                let mut new_fields = Vec::new();
                for (idx, field) in fields.iter().enumerate() {
                    if field.focused {
                        focused_index = Some(idx);
                    }
                    let existing = dialog.fields.iter().find(|f| f.id == field.id);
                    // `cursor` is a CHARACTER index, so bound it against the
                    // char count, not the byte length (multibyte-safe).
                    let char_count = field.value.chars().count();
                    let cursor = existing
                        .map(|f| f.cursor.min(char_count))
                        .unwrap_or(char_count);
                    new_fields.push(crate::data::DialogField {
                        id: field.id.clone(),
                        value: field.value.clone(),
                        cursor,
                        enter_button: field.enter_button.clone(),
                        focused: field.focused,
                    });
                }
                if !new_fields.is_empty() {
                    dialog.fields = new_fields;
                }

                let fallback_focus =
                    dialog.focused_field.filter(|idx| *idx < dialog.fields.len());
                let focused_field = focused_index.or(fallback_focus).or_else(|| {
                    if dialog.fields.is_empty() {
                        None
                    } else {
                        Some(0)
                    }
                });

                dialog.focused_field = focused_field;
                for (idx, field) in dialog.fields.iter_mut().enumerate() {
                    field.focused = dialog.focused_field == Some(idx);
                    field.clamp_cursor();
                }
                self.sync_shown_dialog(ui_state, id, show);
            }
            ParsedElement::DialogProgressBars {
                id,
                clear,
                progress_bars,
            } => {
                self.chunk_has_silent_updates = true;
                let show = Self::dialog_should_popup(ui_state, id);
                let dialog = ui_state.dialog_slot_mut(id);
                if *clear {
                    dialog.progress_bars.clear();
                }
                for pb in progress_bars {
                    let bar = crate::data::DialogProgressBar {
                        id: pb.id.clone(),
                        value: pb.value,
                        text: pb.text.clone(),
                        layout: pb.layout.clone(),
                    };
                    match dialog.progress_bars.iter_mut().find(|b| b.id == pb.id) {
                        Some(slot) => *slot = bar,
                        None => dialog.progress_bars.push(bar),
                    }
                }
                self.sync_shown_dialog(ui_state, id, show);
            }
            ParsedElement::DialogLabelList { id, clear, labels } => {
                self.chunk_has_silent_updates = true;

                // Handle BetrayerPanel state updates
                if id == "BetrayerPanel" {
                    if *clear {
                        game_state.betrayer.clear();
                    }
                    // Extract blood points from lblBPs
                    for label in labels.iter() {
                        if label.id == "lblBPs" {
                            game_state.betrayer.update_blood_points(&label.value);
                            break;
                        }
                    }
                    // Extract items from lblitemN labels (keep '!' prefix for active highlighting)
                    let mut items: Vec<String> = Vec::new();
                    for i in 1..=20 {
                        let item_id = format!("lblitem{}", i);
                        if let Some(label) = labels.iter().find(|l| l.id == item_id) {
                            // Keep the raw value including '!' prefix for active item display
                            items.push(label.value.clone());
                        } else {
                            break; // Stop at first missing item
                        }
                    }
                    game_state.betrayer.update_items(items);
                }

                let window_name = id.to_lowercase();
                if let Some(window) = ui_state.windows.get_mut(&window_name) {
                    if let WindowContent::Text(content) = &mut window.content {
                        if *clear {
                            content.lines.clear();
                            content.scroll_offset = 0;
                        }
                        if !labels.is_empty() {
                            let active_color = self
                                .config
                                .ui
                                .betrayer_active_color
                                .as_ref()
                                .map(|value| value.trim())
                                .filter(|value| !value.is_empty() && *value != "-")
                                .map(|value| value.to_string());
                            for label in labels {
                                if id == "BetrayerPanel" && label.value.starts_with('!') {
                                    let mut segments = Vec::new();
                                    segments.push(TextSegment {
                                        text: "!".to_string(),
                                        fg: active_color.clone(),
                                        bg: None,
                                        bold: false,
                                        mono: false,
                                        span_type: SpanType::Normal,
                                        link_data: None,
                                        custom_emoji: None,
                                    });
                                    let rest = label.value[1..].to_string();
                                    if !rest.is_empty() {
                                        segments.push(TextSegment {
                                            text: rest,
                                            fg: None,
                                            bg: None,
                                            bold: false,
                                            mono: false,
                                            span_type: SpanType::Normal,
                                            link_data: None,
                                            custom_emoji: None,
                                        });
                                    }
                                    content.add_line(StyledLine {
                                        segments,
                                        stream: window_name.clone(),
                                        timestamp: None,
                                    });
                                } else {
                                    content.add_line(StyledLine::from_text(label.value.clone()));
                                }
                            }
                        }
                    }
                }
            }
            ParsedElement::CloseDialog { id } => {
                self.chunk_has_silent_updates = true;
                let is_quickbar_id = id == "quick" || id.starts_with("quick-");
                if is_quickbar_id {
                    ui_state.quickbars.remove(id);
                    ui_state.quickbar_order.retain(|entry| entry != id);

                    if ui_state.active_quickbar_id.as_ref() == Some(id) {
                        ui_state.active_quickbar_id = ui_state.quickbar_order.first().cloned();
                    }
                } else if ui_state
                    .injuries_popup
                    .as_ref()
                    .is_some_and(|popup| popup.dialog_id == *id)
                {
                    // Close injuries popup
                    tracing::debug!("Closing injuries popup: {}", id);
                    ui_state.injuries_popup = None;
                } else if ui_state
                    .active_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.id == *id)
                {
                    ui_state.active_dialog = None;
                    if ui_state.input_mode == InputMode::Dialog {
                        ui_state.input_mode = InputMode::Normal;
                    }
                }
            }
            ParsedElement::ClearDialogData { id } => {
                self.chunk_has_silent_updates = true;
                // Handle BetrayerPanel clear
                if id == "BetrayerPanel" {
                    game_state.betrayer.clear();
                }
                // Other dialog clears can be added here as needed
            }
            ParsedElement::ActiveEffect {
                category,
                id,
                value,
                text,
                time,
            } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Find the window for this category (shared mapping).
                let Some(window_name) =
                    crate::data::ActiveEffectsContent::window_name_for_category(category)
                else {
                    return; // Unknown category
                };

                // Derive an absolute expiry now: effects are only re-sent on
                // change, so the remaining-time string goes stale immediately.
                let time_base = if game_state.game_time > 0 {
                    game_state.game_time
                } else {
                    chrono::Utc::now().timestamp()
                };
                let expires_at =
                    crate::data::parse_time_seconds(time).map(|secs| time_base + secs);

                let spell_style = id
                    .parse::<u32>()
                    .ok()
                    .and_then(|spell_id| self.config.get_spell_color_style(spell_id));
                let default_style = SpellColorStyle {
                    bar_color: None,
                    text_color: None,
                };
                let style = spell_style.unwrap_or(default_style);

                // Always store in game state, independent of the local
                // layout: remote clients (and windows added mid-session)
                // need effects even when no effects window exists.
                let store = game_state
                    .effects
                    .entry(category.clone())
                    .or_insert_with(|| crate::data::ActiveEffectsContent {
                        category: category.clone(),
                        effects: Vec::new(),
                        generation: 0,
                    });
                if let Some(effect) = store.effects.iter_mut().find(|e| e.id == *id) {
                    effect.text = text.clone();
                    effect.value = *value;
                    effect.time = time.clone();
                    effect.expires_at = expires_at;
                    effect.bar_color = style.bar_color.clone();
                    effect.text_color = style.text_color.clone();
                } else {
                    store.effects.push(crate::data::ActiveEffect {
                        id: id.clone(),
                        text: text.clone(),
                        value: *value,
                        time: time.clone(),
                        expires_at,
                        bar_color: style.bar_color.clone(),
                        text_color: style.text_color.clone(),
                    });
                }
                store.generation += 1;

                // Update the window content if it exists
                if let Some(window) = ui_state.get_window_mut(window_name) {
                    if let crate::data::WindowContent::ActiveEffects(ref mut effects_content) =
                        window.content
                    {
                        // Find existing effect or add new one
                        if let Some(effect) =
                            effects_content.effects.iter_mut().find(|e| e.id == *id)
                        {
                            // Update existing effect
                            effect.text = text.clone();
                            effect.value = *value;
                            effect.time = time.clone();
                            effect.expires_at = expires_at;
                            effect.bar_color = style.bar_color.clone();
                            effect.text_color = style.text_color.clone();
                        } else {
                            // Add new effect
                            effects_content.effects.push(crate::data::ActiveEffect {
                                id: id.clone(),
                                text: text.clone(),
                                value: *value,
                                time: time.clone(),
                                expires_at,
                                bar_color: style.bar_color.clone(),
                                text_color: style.text_color.clone(),
                            });
                        }
                        effects_content.generation += 1;
                    }
                }
            }
            ParsedElement::ClearActiveEffects { category } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Find the window for this category (shared mapping).
                let Some(window_name) =
                    crate::data::ActiveEffectsContent::window_name_for_category(category)
                else {
                    return; // Unknown category
                };

                // Clear the game-state store too (see ActiveEffect above)
                if let Some(store) = game_state.effects.get_mut(category.as_str()) {
                    store.effects.clear();
                    store.generation += 1;
                }

                // Clear the window content if it exists
                if let Some(window) = ui_state.get_window_mut(window_name) {
                    if let crate::data::WindowContent::ActiveEffects(ref mut effects_content) =
                        window.content
                    {
                        effects_content.effects.clear();
                        effects_content.generation += 1;
                    }
                }
            }
            ParsedElement::TargetList {
                current_target,
                target_ids,  // Store IDs to filter room_creatures
            } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Store current target and targetable IDs from dropdown
                // These IDs filter room_creatures to show only targetable creatures
                // (only bump the generation on real changes - the dropdown is
                // re-sent frequently with identical content)
                if game_state.target_list.current_target != *current_target
                    || game_state.target_list.target_ids != *target_ids
                {
                    game_state.target_list.current_target = current_target.clone();
                    game_state.target_list.target_ids = target_ids.clone();
                    game_state.target_list.generation += 1;
                }

                tracing::debug!(
                    "Updated targets from dropdown: current='{}', {} targetable IDs",
                    current_target,
                    target_ids.len()
                );
            }
            ParsedElement::Container { id, title, target } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Register the container in the registry (target is the
                // game-command id, which differs from the stream id for stow).
                game_state
                    .objects
                    .register_container(id.clone(), title.clone(), target.clone());

                // Signal the sighting for the realize pass (every LOOK IN
                // triggers this): the frontend tick auto-(re)opens the
                // container window when the user has opted it in, and window
                // creation itself skips already-open windows. U3: containers
                // are ephemeral session windows managed via the unified list.
                if !title.is_empty() {
                    self.newly_registered_container = Some((id.clone(), title.clone()));
                    tracing::debug!("Container seen: id='{}', title='{}'", id, title);
                } else {
                    tracing::debug!("Registered container: id='{}', title='{}'", id, title);
                }
            }
            ParsedElement::ClearContainer { id } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Clear container contents
                game_state.objects.clear_container(id);

                tracing::debug!("Cleared container: id='{}'", id);
            }
            ParsedElement::ContainerItem { container_id, content } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Parse the raw <inv> line into a structured GameItem,
                // skipping the container's own header line.
                if let Some(container) = game_state.objects.container(container_id) {
                    let target = container.command_target();
                    if !crate::core::game_objects::parse::is_header_line(content, &target) {
                        if let Some(item) =
                            crate::core::game_objects::parse_anchor(content)
                        {
                            game_state.objects.add_container_item(container_id, item);
                        }
                    }
                } else if let Some(item) =
                    crate::core::game_objects::parse_anchor(content)
                {
                    // Item arrived before the <container> tag; register it
                    // (auto-creates a title-less entry, same as the cache).
                    // Header lines have their own anchor id == container id,
                    // but with no container known yet we can't dedup that;
                    // the header's noun is the container itself, harmless to
                    // include and corrected on the next clear+refill.
                    game_state.objects.add_container_item(container_id, item);
                }

                tracing::trace!("Added item to container '{}': {}", container_id,
                    if content.len() > 50 { format!("{}...", &content[..50]) } else { content.clone() });
            }
            ParsedElement::LichWebUI(handshake) => {
                self.chunk_has_silent_updates = true; // control line, not game text
                tracing::info!(
                    "LichWebUI handshake received: status={} port={}",
                    handshake.status,
                    handshake.port
                );
                self.pending_webui_handshake = Some(handshake.clone());
            }
            ParsedElement::LaunchURL { url } => {
                // Build full URL by prepending play.net base
                let full_url = format!("https://www.play.net{}", url);
                tracing::info!("Launching URL in browser: {}", full_url);

                // Open in default browser
                if let Err(e) = crate::platform::open_url(&full_url) {
                    tracing::error!("Failed to open browser: {}", e);
                }
            }
            _ => {
                // Other elements handled elsewhere or not yet implemented
            }
        }
    }

    /// Handle stream window declaration.
    ///
    /// By default (room_in_main = true), <streamWindow> is treated as a window declaration
    /// tag that does NOT change the current stream context. This allows room text to flow
    /// to the main window (room window uses components, not text).
    ///
    /// When room_in_main = false (legacy mode), <streamWindow id='room'> will push the
    /// stream to "room", causing room text to be discarded. The stream is reset on prompt.
    ///
    /// DragonRealms-specific - GemStone IV doesn't use streamWindow room.
    /// After ingesting a dialogData delta into the store, reflect it into
    /// the visible `active_dialog` if this dialog should be shown. When
    /// first materializing a shown dialog, seed its saved position/size.
    /// Hidden dialogs stay in the store only. If the currently-shown
    /// dialog is a *different* id, leave it be (one popup at a time).
    fn sync_shown_dialog(&self, ui_state: &mut UiState, id: &str, show: bool) {
        if !show {
            return;
        }
        // Don't steal the screen from a different open dialog.
        if ui_state
            .active_dialog
            .as_ref()
            .is_some_and(|d| d.id != id)
        {
            return;
        }
        let first_show = ui_state
            .active_dialog
            .as_ref()
            .map(|d| d.id != id)
            .unwrap_or(true);
        ui_state.show_dialog_from_store(id);
        if first_show {
            if let Some(dialog) = ui_state.active_dialog.as_mut() {
                if let Some(p) = self.saved_dialog_positions.dialogs.get(id) {
                    dialog.position = Some((p.x, p.y));
                    dialog.size = p.width.zip(p.height);
                    dialog.save_position = true;
                }
            }
        }
    }

    /// Whether a dialog's data may be shown as a transient popup. The
    /// always-ingest store keeps every dialog's state regardless; this only
    /// gates the popup. U6: nothing pops up unless the user has SHOWN it via
    /// the Windows list (its id is in `shown_dialog_ids`) — hidden-by-default,
    /// replacing the old blocklist.
    fn dialog_should_popup(ui_state: &UiState, id: &str) -> bool {
        ui_state
            .shown_dialog_ids
            .iter()
            .any(|shown| shown.eq_ignore_ascii_case(id))
    }

    fn handle_stream_window(
        &mut self,
        id: &str,
        subtitle: Option<&str>,
        room_subtitle_out: &mut Option<String>,
        room_window_dirty: &mut bool,
    ) {
        // Decide whether to push the stream
        // For room stream: only push if room_in_main is false (legacy behavior)
        let should_push_stream = if id == "room" {
            !self.config.streams.room_in_main
        } else {
            // Non-room streamWindow tags: keep existing behavior (don't push)
            false
        };

        if should_push_stream {
            self.current_stream = id.to_string();

            // Check stream subscribers for discard logic (case-insensitive lookup)
            if !self.get_stream_subscribers(id).is_empty() {
                self.discard_current_stream = false;
            } else if matches!(self.resolve_orphaned_stream(id), RouteDecision::Discard) {
                self.discard_current_stream = true;
                tracing::debug!("Discarding stream '{}' (routed to discard)", id);
            } else {
                // No subscribers - deliver per route/fallback at flush time
                self.discard_current_stream = false;
                tracing::debug!(
                    "Routing stream '{}' per route map (fallback '{}')",
                    id,
                    self.config.streams.fallback
                );
            }
        }

        // Update room subtitle if this is the room window declaration (always, regardless of push)
        if id == "room" {
            if let Some(subtitle_text) = subtitle {
                // Remove leading " - " if present (matches VellumFE behavior)
                let clean_subtitle = subtitle_text.trim_start_matches(" - ");
                *room_subtitle_out = Some(clean_subtitle.to_string());
                *room_window_dirty = true;
                tracing::debug!(
                    "Room subtitle updated from streamWindow: {} (cleaned from: {})",
                    clean_subtitle,
                    subtitle_text
                );
            }
        }

        // Update main window subtitle if applicable
        if id == "main" {
            if let Some(subtitle_text) = subtitle {
                let clean_subtitle = subtitle_text.trim_start_matches(" - ");
                tracing::debug!("Main window subtitle: {}", clean_subtitle);
                // Could store this somewhere if needed for display
            }
        }
    }

    /// Scan raw component content for `<crtrStatus exist="..." .../>` tags,
    /// keyed by exist id. Component values are captured with embedded tags
    /// intact, so this runs over the same string the creature scan uses.
    fn parse_crtr_status_tags(
        value: &str,
    ) -> std::collections::HashMap<String, crate::core::state::CreatureFlags> {
        let mut map = std::collections::HashMap::new();
        let mut remaining = value;
        while let Some(start) = remaining.find("<crtrStatus") {
            let Some(end_offset) = remaining[start..].find('>') else {
                break;
            };
            let tag = &remaining[start..start + end_offset + 1];
            let attrs = crate::parser::XmlParser::extract_all_attributes(tag);
            let exist = attrs
                .iter()
                .find(|(name, _)| name == "exist")
                .map(|(_, value)| value.clone());
            if let Some(exist) = exist {
                let flags = crate::core::state::CreatureFlags::from_xml_attrs(
                    attrs
                        .iter()
                        .filter(|(name, _)| name != "exist")
                        .map(|(n, v)| (n.as_str(), v.as_str())),
                );
                map.insert(exist, flags);
            }
            remaining = &remaining[start + end_offset + 1..];
        }
        map
    }

    /// Handle component data for room window and exp window (DR)
    fn handle_component(
        &mut self,
        id: &str,
        value: &str,
        game_state: &mut GameState,
        room_components: &mut std::collections::HashMap<String, Vec<Vec<TextSegment>>>,
        current_room_component: &mut Option<String>,
        room_window_dirty: &mut bool,
    ) {
        // Mark ALL components as silent updates (shouldn't trigger prompts in main window)
        // This includes DR experience components (exp Brawling, exp tdp, etc.)
        self.chunk_has_silent_updates = true;

        // Handle DragonRealms experience components (exp Stealth, exp tdp, etc.)
        if let Some(field_name) = id.strip_prefix("exp ") {
            // Register the field order (will be a no-op after first occurrence)
            game_state
                .dr_experience
                .register_field(field_name.to_string());

            // Update the value (only triggers generation bump if changed)
            if game_state
                .dr_experience
                .update_field(field_name, value.to_string())
            {
                tracing::debug!("Exp component updated: {} = {}", field_name, value);
            } else {
                tracing::trace!("Exp component unchanged: {}", field_name);
            }
            return;
        }

        // Only process room-related components for room window updates
        if !id.starts_with("room ") {
            tracing::trace!("Ignoring non-room component: {}", id);
            return;
        }

        // Skip processing if we're discarding the current stream (no window exists)
        if self.discard_current_stream {
            tracing::debug!("Skipping room component {} - no room window exists", id);
            return;
        }

        // Check if component value has changed (avoid unnecessary processing)
        if let Some(previous_value) = self.previous_room_components.get(id) {
            if previous_value == value {
                tracing::trace!("Room component {} unchanged - skipping processing", id);
                return;
            }
            // Debug: log when room objs changes (especially to empty)
            if id == "room objs" {
                tracing::debug!(
                    "Room objs changed: prev_len={}, new_len={}, new_empty={}",
                    previous_value.len(),
                    value.len(),
                    value.is_empty()
                );
            }
        } else if id == "room objs" {
            tracing::debug!("Room objs first seen: len={}, empty={}", value.len(), value.is_empty());
        }

        tracing::debug!(
            "Processing room component: {} (value length: {})",
            id,
            value.len()
        );

        // Store current value for next comparison
        self.previous_room_components
            .insert(id.to_string(), value.to_string());

        // Extract creatures from room objs (for targets widget)
        // Room objs contains items/creatures on ground. Creatures are in bold:
        // <b><pushBold/>a <a exist='ID' noun='...'>name</a><popBold/></b> (status)
        if id == "room objs" {
            let had_objs = !game_state.room_creatures.is_empty();
            game_state.room_creatures.clear();
            // handle_component early-returns on unchanged values, so this
            // block only runs on real changes - the bump is accurate
            game_state.room_creatures_generation += 1;

            // Log when room objs becomes empty (item picked up, etc.)
            if value.is_empty() {
                tracing::debug!("Room objs now empty (previously had creatures: {})", had_objs);
            }

            // Pre-scan for <crtrStatus exist="..." .../> snapshots embedded in
            // the component (the tag precedes each creature's bold name).
            // Keyed by exist id; the tag is self-contained so pairing by id
            // beats positional pairing.
            let crtr_flags = Self::parse_crtr_status_tags(value);

            let mut remaining = value;
            while let Some(bold_start) = remaining.find("<b>") {
                // Find the matching </b>
                if let Some(bold_end_offset) = remaining[bold_start..].find("</b>") {
                    let bold_end = bold_start + bold_end_offset;
                    let bold_section = &remaining[bold_start..bold_end + 4]; // Include </b>

                    // Extract <a exist='...' noun='...'>name</a> within the bold section
                    if let Some(link_start) = bold_section.find("<a ") {
                        if let Some(link_end) = bold_section[link_start..].find("</a>") {
                            let link_tag_end = bold_section[link_start..link_start + link_end]
                                .find('>')
                                .unwrap_or(0);
                            let link_tag = &bold_section[link_start..link_start + link_tag_end];
                            let link_text_start = link_start + link_tag_end + 1;
                            let link_text_end = link_start + link_end;
                            let creature_name = &bold_section[link_text_start..link_text_end];

                            // Extract exist ID from the link tag
                            if let Some(exist_pos) = link_tag.find("exist=") {
                                let after_exist = &link_tag[exist_pos + 6..];
                                if let Some(quote) = after_exist.chars().next() {
                                    if quote == '\'' || quote == '"' {
                                        if let Some(end_quote) = after_exist[1..].find(quote) {
                                            let exist_id = &after_exist[1..=end_quote];

                                            // Extract noun from the link tag (optional)
                                            let noun = if let Some(noun_pos) = link_tag.find("noun=") {
                                                let after_noun = &link_tag[noun_pos + 5..];
                                                if let Some(noun_quote) = after_noun.chars().next() {
                                                    if noun_quote == '\'' || noun_quote == '"' {
                                                        if let Some(noun_end_quote) = after_noun[1..].find(noun_quote) {
                                                            Some(after_noun[1..=noun_end_quote].to_string())
                                                        } else {
                                                            None
                                                        }
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            };

                                            // Check for status after </b>: " (stunned)" or " (dead)"
                                            let after_bold = &remaining[bold_end + 4..];
                                            let status = if after_bold.trim_start().starts_with('(') {
                                                // Extract text between ( and )
                                                after_bold.find('(').and_then(|start| {
                                                    let after_paren = &after_bold[start + 1..];
                                                    after_paren
                                                        .find(')')
                                                        .map(|end| after_paren[..end].to_string())
                                                })
                                            } else {
                                                None
                                            };

                                            // Check if noun should be excluded (configurable filter for non-creatures)
                                            if let Some(ref noun_val) = noun {
                                                if self.config.target_list.excluded_nouns.iter()
                                                    .any(|excluded| excluded.eq_ignore_ascii_case(noun_val)) {
                                                    tracing::debug!(
                                                        "Skipping creature with excluded noun: '{}' (name: '{}')",
                                                        noun_val, creature_name
                                                    );
                                                    remaining = &remaining[bold_end + 4..];
                                                    continue;
                                                }
                                            }

                                            let creature = crate::core::state::Creature {
                                                id: format!("#{}", exist_id),
                                                name: creature_name.to_string(),
                                                noun: noun.clone(),
                                                status: status.clone(),
                                                flags: crtr_flags.get(exist_id).cloned(),
                                            };

                                            tracing::debug!(
                                                "Parsed creature from room objs: name='{}', noun={:?}, id='{}', status={:?}",
                                                creature.name, creature.noun, creature.id, creature.status
                                            );

                                            game_state.room_creatures.push(creature);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    remaining = &remaining[bold_end + 4..];
                } else {
                    break;
                }
            }

            tracing::debug!(
                "Extracted {} creatures from room objs",
                game_state.room_creatures.len()
            );

            // Now extract room objects (non-bold links = items on ground)
            // Strategy: remove all <b>...</b> sections, then parse remaining <a> links
            game_state.room_objects.clear();
            game_state.room_objects_generation += 1;

            // Create a version of the value with bold sections removed
            let mut no_bold = String::new();
            let mut pos = 0usize;
            while pos < value.len() {
                if let Some(bold_start) = value[pos..].find("<b>") {
                    // Add everything before <b>
                    no_bold.push_str(&value[pos..pos + bold_start]);
                    // Find matching </b>
                    if let Some(bold_end) = value[pos + bold_start..].find("</b>") {
                        pos = pos + bold_start + bold_end + 4; // Skip past </b>
                    } else {
                        break;
                    }
                } else {
                    // No more bold sections, add the rest
                    no_bold.push_str(&value[pos..]);
                    break;
                }
            }

            // Now parse <a> links from the non-bold content
            let mut remaining = no_bold.as_str();
            while let Some(link_start) = remaining.find("<a ") {
                if let Some(link_end) = remaining[link_start..].find("</a>") {
                    let link_section = &remaining[link_start..link_start + link_end + 4];

                    // Extract the tag part and text part
                    if let Some(tag_end) = link_section.find('>') {
                        let link_tag = &link_section[..tag_end];
                        let link_text = &link_section[tag_end + 1..link_section.len() - 4]; // Remove </a>

                        // Extract exist ID
                        if let Some(exist_pos) = link_tag.find("exist=") {
                            let after_exist = &link_tag[exist_pos + 6..];
                            if let Some(quote) = after_exist.chars().next() {
                                if quote == '\'' || quote == '"' {
                                    if let Some(end_quote) = after_exist[1..].find(quote) {
                                        let exist_id = &after_exist[1..=end_quote];

                                        // Extract noun
                                        let noun = if let Some(noun_pos) = link_tag.find("noun=") {
                                            let after_noun = &link_tag[noun_pos + 5..];
                                            if let Some(noun_quote) = after_noun.chars().next() {
                                                if noun_quote == '\'' || noun_quote == '"' {
                                                    if let Some(noun_end) = after_noun[1..].find(noun_quote) {
                                                        Some(after_noun[1..=noun_end].to_string())
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };

                                        let room_object = crate::core::state::RoomObject {
                                            id: exist_id.to_string(),
                                            name: link_text.to_string(),
                                            noun,
                                        };

                                        tracing::debug!(
                                            "Parsed room object: name='{}', noun={:?}, id='{}'",
                                            room_object.name, room_object.noun, room_object.id
                                        );

                                        game_state.room_objects.push(room_object);
                                    }
                                }
                            }
                        }
                    }

                    remaining = &remaining[link_start + link_end + 4..];
                } else {
                    break;
                }
            }

            tracing::debug!(
                "Extracted {} room objects from room objs",
                game_state.room_objects.len()
            );

            // Dual-write ground items into the registry (the `floor`/
            // `ground`/`room` foreach targets). Room loot = NOT yours,
            // distinct from at-feet. Consumers still read room_objects.
            let ground: Vec<crate::core::game_objects::GameItem> = game_state
                .room_objects
                .iter()
                .map(|o| {
                    crate::core::game_objects::GameItem::new(
                        o.id.clone(),
                        o.noun.clone().unwrap_or_default(),
                        o.name.clone(),
                    )
                })
                .collect();
            game_state.objects.set_ground(ground);
        }

        // Extract players from room players component
        // Format: "Also here: <a exist='-ID' noun='Name'>Name</a> (prone), a stunned <a exist='...' noun='...'>Name2</a> (prone)"
        if id == "room players" {
            game_state.room_players.clear();
            game_state.room_players_generation += 1;

            let mut remaining = value;

            // Skip "Also here:" prefix if present
            if let Some(pos) = remaining.find(':') {
                remaining = &remaining[pos + 1..];
            }

            // Parse players - separated by commas or end of component
            while let Some(link_start) = remaining.find("<a ") {
                if let Some(link_end) = remaining[link_start..].find("</a>") {
                    let link_section_end = link_start + link_end + 4;
                    let link_section = &remaining[link_start..link_section_end];

                    // Extract exist ID
                    if let Some(exist_pos) = link_section.find("exist=") {
                        let after_exist = &link_section[exist_pos + 6..];
                        if let Some(quote) = after_exist.chars().next() {
                            if quote == '\'' || quote == '"' {
                                if let Some(end_quote) = after_exist[1..].find(quote) {
                                    let exist_id = &after_exist[1..=end_quote];

                                    // Extract player name
                                    if let Some(name_start) = link_section.find('>') {
                                        let name_end = link_section.find("</a>").unwrap();
                                        let player_name = &link_section[name_start + 1..name_end];

                                        // Prefix zone (text before the link):
                                        // may carry titles ("Lord ", "Arena
                                        // Occultist ") and/or the corpse marker
                                        // ("the body of "). Titles are stripped;
                                        // "the body of" sets the dead flag.
                                        let before_link = &remaining[..link_start];
                                        let (dead, primary_status) =
                                            Self::parse_player_prefix(before_link);

                                        // Suffix zone (text after the link, up to
                                        // the next comma that separates players).
                                        // Holds either the brief "(prone)" form or
                                        // the verbose "who is lying down" clause.
                                        let after_link = &remaining[link_section_end..];
                                        let suffix = match after_link.find(',') {
                                            Some(comma) => &after_link[..comma],
                                            None => after_link,
                                        };
                                        let secondary_status =
                                            Self::parse_player_suffix_status(suffix);

                                        let player = crate::core::state::Player {
                                            id: exist_id.to_string(),
                                            name: player_name.to_string(),
                                            primary_status,
                                            secondary_status,
                                            dead,
                                        };

                                        game_state.room_players.push(player);
                                    }
                                }
                            }
                        }
                    }

                    remaining = &remaining[link_section_end..];
                } else {
                    break;
                }
            }

            tracing::debug!(
                "Extracted {} players from room players",
                game_state.room_players.len()
            );
        }

        // If we're starting a new component, finish the current one first
        if current_room_component
            .as_ref()
            .map(|c| c != id)
            .unwrap_or(false)
        {
            // Finish current component
            *current_room_component = None;
        }

        // ALWAYS clear the component buffer when receiving new data (game sends full replacement, not append)
        room_components
            .entry(id.to_string())
            .or_default()
            .clear();
        *current_room_component = Some(id.to_string());
        tracing::debug!("Started/replaced room component: {}", id);

        // Mark room window dirty when component is cleared (even if empty)
        // This ensures the room window updates when items are picked up, etc.
        *room_window_dirty = true;

        // An empty "room desc" component clears the mirrored prose (the parse
        // block below is skipped for empty values, so clear it here).
        if id == "room desc" && value.trim().is_empty() && !game_state.room_description.is_empty() {
            game_state.room_description.clear();
            game_state.room_description_generation += 1;
        }

        // Parse the component value to extract styled segments
        if !value.trim().is_empty() {
            // Save parser state before parsing component (components are self-contained)
            let saved_color_stack = self.parser.color_stack.clone();
            let saved_preset_stack = self.parser.preset_stack.clone();
            let saved_style_stack = self.parser.style_stack.clone();
            let saved_bold_stack = self.parser.bold_stack.clone();
            let saved_link_depth = self.parser.link_depth;
            let saved_spell_depth = self.parser.spell_depth;
            let saved_link_data = self.parser.current_link_data.clone();

            // Clear stacks for component parsing (start with clean state)
            self.parser.color_stack.clear();
            self.parser.preset_stack.clear();
            self.parser.style_stack.clear();
            self.parser.bold_stack.clear();
            self.parser.link_depth = 0;
            self.parser.spell_depth = 0;
            self.parser.current_link_data = None;

            // Parse the component value as XML to get styled elements
            let parsed_elements = self.parser.parse_line(value);

            // Extract text segments from parsed elements
            let mut current_line_segments = Vec::new();

            for element in parsed_elements {
                match element {
                    crate::parser::ParsedElement::Text {
                        content,
                        fg_color,
                        bg_color,
                        bold,
                        span_type,
                        link_data,
                        ..
                    } => {
                        // Map parser SpanType to data layer SpanType
                        use crate::data::SpanType as DataSpanType;
                        use crate::parser::SpanType as ParserSpanType;
                        let data_span_type = match span_type {
                            ParserSpanType::Normal => DataSpanType::Normal,
                            ParserSpanType::Link => DataSpanType::Link,
                            ParserSpanType::Monsterbold => DataSpanType::Monsterbold,
                            ParserSpanType::Spell => DataSpanType::Spell,
                            ParserSpanType::Speech => DataSpanType::Speech,
                            };

                        // Link data is already the correct type from parser
                        let link = link_data.clone();

                        let segment = TextSegment {
                            text: content.clone(),
                            fg: fg_color.clone(),
                            bg: bg_color.clone(),
                            bold,
                            mono: false,
                            span_type: data_span_type,
                            link_data: link.clone(),
                            custom_emoji: None,
                        };

                        // Debug logging for room exits to understand link coloring
                        if id == "room exits" {
                            tracing::debug!(
                                "Room exits segment: text='{}', fg={:?}, span_type={:?}, has_link={}",
                                content,
                                fg_color,
                                data_span_type,
                                link.is_some()
                            );
                        }

                        current_line_segments.push(segment);
                    }
                    _ => {
                        // Ignore other parsed elements (we only care about Text)
                    }
                }
            }

            // Mirror the room description prose onto GameState as STYLED lines
            // so headless/remote clients get the room "look" — with its
            // clickable scenery links and coloring — without a room window.
            // The game sends a full component replacement and handle_component
            // early-returns on unchanged values, so this runs only on real
            // changes — the generation bump stays accurate.
            if id == "room desc" {
                let is_blank = current_line_segments
                    .iter()
                    .all(|s| s.text.trim().is_empty());
                let new_desc: Vec<crate::data::widget::StyledLine> = if is_blank {
                    Vec::new()
                } else {
                    vec![crate::data::widget::StyledLine {
                        segments: current_line_segments.clone(),
                        stream: "room".to_string(),
                        timestamp: None,
                    }]
                };
                if game_state.room_description != new_desc {
                    game_state.room_description = new_desc;
                    game_state.room_description_generation += 1;
                }
            }

            // Add the line if we got any segments
            if !current_line_segments.is_empty() {
                if let Some(buffer) = room_components.get_mut(id) {
                    buffer.push(current_line_segments);
                    *room_window_dirty = true;
                }
            }

            // Restore parser state after parsing component
            self.parser.color_stack = saved_color_stack;
            self.parser.preset_stack = saved_preset_stack;
            self.parser.style_stack = saved_style_stack;
            self.parser.bold_stack = saved_bold_stack;
            self.parser.link_depth = saved_link_depth;
            self.parser.spell_depth = saved_spell_depth;
            self.parser.current_link_data = saved_link_data;
        }
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
    fn drain_pending_container_ingest(
        &mut self,
        game_state: &mut crate::core::state::GameState,
    ) {
        let Some((container_id, items)) = self.pending_container_ingest.take() else {
            return;
        };
        if game_state.objects.container(&container_id).is_none() {
            game_state.objects.register_container(
                container_id.clone(),
                String::new(),
                None,
            );
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

    /// Flush current stream with optional TTS enqueuing. Wrapper drains
    /// any lines a transform injected (sorter categories) through the
    /// same pipeline, so each gets highlights/squelch/TTS individually.
    pub fn flush_current_stream_with_tts(
        &mut self,
        ui_state: &mut UiState,
        mut tts_manager: Option<&mut crate::tts::TtsManager>,
    ) {
        self.flush_one_line(ui_state, tts_manager.as_deref_mut());
        while let Some(next) = self.injected_lines.pop_front() {
            self.current_segments = next;
            self.flush_one_line(ui_state, tts_manager.as_deref_mut());
        }
    }

    /// Flush exactly the pending line (no injected-line draining).
    fn flush_one_line(
        &mut self,
        ui_state: &mut UiState,
        mut tts_manager: Option<&mut crate::tts::TtsManager>,
    ) {
        // Concatenate all segments to get full line text for squelch checking
        let full_text: String = self
            .current_segments
            .iter()
            .map(|seg| seg.text.as_str())
            .collect();

        // Skip leading blank lines - only keep interior blanks (after content starts)
        // This preserves formatting blank lines within output blocks like BOUNTY
        // while filtering noise blank lines before any content appears
        let is_blank_line = full_text.trim().is_empty();
        if is_blank_line && !self.chunk_has_main_text {
            self.current_segments.clear();
            return;
        }

        // Active INVENTORY FULL scan: capture status lines into the scan
        // and squelch the whole reply from the display. The prompt handler
        // finalizes the scan into the registry. Header/footer lines
        // (no link) are captured for the window bound and squelched too,
        // so the reply block doesn't leak into the main window.
        if self.inv_scan.is_capturing() {
            self.inv_scan.ingest_segments(&self.current_segments);
            self.current_segments.clear();
            return;
        }

        // Check if line should be squelched (ignored/filtered)
        // Squelch always takes precedence over redirect
        if self.should_squelch_line(&full_text) {
            tracing::debug!(
                "Line squelched: '{}'",
                if full_text.len() > 80 {
                    format!("{}...", &full_text[..80])
                } else {
                    full_text.clone()
                }
            );
            self.current_segments.clear();
            return; // Discard line completely
        }

        // Mapping evidence capture (forage sense / ranger sense responses on
        // the main stream). Cheap: a few substring checks per line.
        if self.current_stream == "main" {
            if let Some(items) = crate::core::evidence::parse_forage_line(&full_text) {
                self.pending_evidence
                    .push(crate::core::evidence::Observation::Forage(items));
            } else if let Some(data) = crate::core::evidence::parse_sense_line(&full_text) {
                self.pending_evidence
                    .push(crate::core::evidence::Observation::Sense(data));
            } else if let Some(route) =
                crate::core::travel::mazes::parse_pathcode_line(&full_text)
            {
                self.pending_pathcode = Some(route);
            }
        }

        // Sorter: replace a container-look line with categorized lines.
        // The flush wrapper drains the extras; generated lines can't
        // re-trigger (no " you see ").
        if self.current_stream == "main"
            && crate::core::sorter::is_container_look(&full_text)
        {
            // Ingest the container's contents into the registry from the
            // VISIBLE look line — a plain `look in` (and Lich's ;sorter
            // reformat) can deliver contents only as this main-stream
            // prose, not as <inv> paired tags. Buffered here; the caller
            // drains it into game_state.objects (this fn lacks game_state).
            if let Some(pending) = crate::core::sorter::extract_container_items(
                &self.current_segments,
                &full_text,
            ) {
                self.pending_container_ingest = Some(pending);
            }

            // Categorized display transform (only when .sorter is on).
            if self.config.sorter.enabled {
                let data = self.sorter_gameobj();
                if let Some(mut lines) = crate::core::sorter::transform(
                    &self.current_segments,
                    &full_text,
                    &data,
                    &self.config.sorter,
                ) {
                    self.current_segments = lines.remove(0);
                    self.injected_lines.extend(lines);
                }
            }
        }

        // Check for redirect match (after squelch, as squelch takes precedence)
        let redirect_match = self.check_redirect_match(&full_text);

        // Handle redirect by overriding stream (works for both Text and TabbedText windows)
        let original_stream = self.current_stream.clone();
        let mut should_send_to_original = true;

        if let Some((redirect_stream, redirect_mode, _match_len)) = redirect_match {
            tracing::debug!(
                "Line matched redirect pattern -> stream '{}' (mode: {:?})",
                redirect_stream,
                redirect_mode
            );

            // Override stream to redirect target
            self.current_stream = redirect_stream;

            // Determine if we should also send to original stream
            if redirect_mode == crate::config::RedirectMode::RedirectOnly {
                should_send_to_original = false;
            }
        }

        // Apply highlights ONCE here in core, before segments reach any widget.
        // This ensures text arrives at widgets pre-colored.
        let highlight_result = self
            .highlight_engine
            .apply_highlights(&self.current_segments, &self.current_stream);
        self.current_segments = highlight_result.segments;
        let deferred_replacements = highlight_result.deferred_replacements;

        // Expand :grin:-style emoji shortcodes at the same seam as highlight
        // text replacement, so every frontend sees the expanded text. Gated
        // by ui.emoji_shortcodes (mirrors the highlight_settings toggles).
        self.apply_emoji_shortcodes();

        // Queue sounds from highlight processing
        self.pending_sounds.extend(highlight_result.sounds);
        self.pending_status_actions
            .extend(highlight_result.status_actions);
        self.pending_rumbles.extend(highlight_result.rumbles);

        let mut line = StyledLine {
            segments: std::mem::take(&mut self.current_segments),
            stream: self.current_stream.clone(),
            timestamp: None,
        };

        // Track main stream text for prompt skip logic.
        // If a line contains any Speech spans, treat it as speech-only (even with trailing punctuation).
        // If the entire line matched silent_prompt patterns, don't count it as main text.
        if self.current_stream == "main" {
            let has_speech = line
                .segments
                .iter()
                .any(|seg| seg.span_type == SpanType::Speech);
            let has_non_speech_text = line
                .segments
                .iter()
                .any(|seg| seg.span_type != SpanType::Speech && !seg.text.trim().is_empty());

            // Speech also goes to main window, so include it as displayable content
            if (has_non_speech_text || has_speech) && !highlight_result.line_is_silent {
                self.chunk_has_main_text = true;
            }
        }

        // Filter out Speech-typed segments ONLY when on a speech-related stream with no consumer
        // When on main stream, keep Speech segments even if no speech window (main displays full text)
        // This prevents "You say" from being cut off when there's no speech window
        let should_filter_speech = if self.current_stream == "speech" || self.current_stream == "talk" || self.current_stream == "whisper" {
            // On speech stream - check if there's a consumer
            !ui_state.windows.iter().any(|(name, window)| {
                if name == &self.current_stream {
                    return true;
                }
                matches!(&window.content, WindowContent::TabbedText(tabbed) if tabbed.tabs.iter().any(
                    |t| t.definition.streams.iter().any(|s| s == &self.current_stream)
                ))
            })
        } else {
            // On other streams (like main) - never filter Speech segments
            false
        };

        if should_filter_speech {
            let original_count = line.segments.len();
            line.segments
                .retain(|seg| seg.span_type != crate::data::SpanType::Speech);
            if line.segments.len() < original_count {
                tracing::trace!(
                    "Filtered out {} Speech segments on stream '{}' (no consumer window)",
                    original_count - line.segments.len(),
                    self.current_stream
                );
            }
        }

        // If all segments were filtered out, nothing to add
        if line.segments.is_empty() {
            self.current_stream = original_stream; // Restore original stream
            return;
        }

        // Determine target window based on stream (may be redirected stream)
        let _window_name = self.map_stream_to_window(&self.current_stream);

        // Special handling for room stream - room uses components, not text segments
        // Discard text from room stream (room data flows through components only)
        if self.current_stream == "room" {
            tracing::debug!(
                "Discarding text segment from room stream (room uses components, not text)"
            );
            // A redirect may have set current_stream; without the restore the
            // override leaks into every following line of the chunk
            self.current_stream = original_stream;
            return;
        }

        // Remote scrollback tap (web frontend): record the finalized,
        // unwrapped line keyed by stream. Must stay after squelch/speech
        // filtering and the room-stream discard so remote clients see what
        // local windows can see. Mirrors the redirect copy: a redirected
        // line is recorded under both streams when the mode keeps the
        // original.
        if let Some(remote) = self.remote.as_mut() {
            let shared = std::sync::Arc::new(line.clone());
            remote.push_text(&self.current_stream, shared.clone());
            if should_send_to_original && self.current_stream != original_stream {
                remote.push_text(&original_stream, shared);
            }
        }

        // Buffer bounty stream data for later use (e.g., when adding a bounty window later)
        // This happens regardless of whether a bounty window exists
        if self.current_stream.eq_ignore_ascii_case("bounty") {
            // Extract plain text from segments
            let plain_text: String = line.segments.iter().map(|s| s.text.as_str()).collect();

            // Always parse to compact form and buffer both raw and compact
            let compact_lines = if let Some(compact) = bounty_parser::parse_bounty(&plain_text) {
                compact.lines
            } else {
                vec![plain_text.clone()] // Fallback to raw text if parsing fails
            };

            self.bounty_buffer = Some((plain_text, compact_lines));
            tracing::debug!("Buffered bounty data for later use");
            // Continue processing - don't return here, still send to windows
        }

        // Buffer society stream data for reload
        // This happens regardless of whether a society window exists
        if self.current_stream.eq_ignore_ascii_case("society") {
            let plain_text: String = line.segments.iter().map(|s| s.text.as_str()).collect();
            self.society_buffer.push(plain_text);
            tracing::debug!("Buffered society line for reload ({} total)", self.society_buffer.len());
            // Continue processing - don't return here, still send to windows
        }

        // Special handling for inv stream - buffer instead of directly adding to window
        // Inventory updates are sent constantly with same items, so we buffer and compare
        // Inventory stream is always a silent update (shouldn't trigger prompts in main window)
        if self.current_stream == "inv" {
            self.chunk_has_silent_updates = true;
            // Buffer unconditionally: the buffer is the source of truth for
            // both the inventory window (if any) AND the GameObjects
            // registry, which owns worn/carried items regardless of whether
            // a window happens to be open. (Previously this discarded the
            // whole feed when no inventory window existed — a latent bug
            // that left the registry blind to worn items.)
            let num_segments = line.segments.len();
            self.inventory_buffer.push(line.segments);
            tracing::trace!("Buffered inventory line ({} segments)", num_segments);
            self.current_stream = original_stream;
            return;
        }

        // Special handling for reserve stream - buffer instead of directly adding
        // to window, same snapshot-and-compare handling as inv
        if self.current_stream == "reserve" {
            self.chunk_has_silent_updates = true;
            // Check if ANY window has Reserve content type
            if !ui_state
                .windows
                .values()
                .any(|w| matches!(w.content, WindowContent::Reserve(_)))
            {
                tracing::trace!("Discarding reserve stream content - no reserve window exists");
                self.current_stream = original_stream;
                return;
            }
            // Add line to reserve buffer instead of window
            let num_segments = line.segments.len();
            self.reserve_buffer.push(line.segments);
            tracing::trace!("Buffered reserve line ({} segments)", num_segments);
            self.current_stream = original_stream;
            return;
        }

        // Special handling for percWindow stream - buffer for perception widget
        // Perception stream is always a silent update (shouldn't trigger prompts in main window)
        if self.current_stream == "percWindow" {
            self.chunk_has_silent_updates = true;
            // Check if ANY window has Perception content type
            if !ui_state
                .windows
                .values()
                .any(|w| matches!(w.content, WindowContent::Perception(_)))
            {
                tracing::debug!("Discarding percWindow stream content - no perception window exists");
                self.current_stream = original_stream;
                return;
            }

            // Concatenate segments to get full text
            let full_text: String = line.segments.iter().map(|s| s.text.as_str()).collect();

            // Split concatenated entries into individual perception entries
            // The game may send multiple entries in one line like: "Bless  (OM)Auspice  (OM)"
            let split_entries = Self::split_perception_entries(&full_text);

            for entry_text in split_entries {
                // Find link data for this specific entry (if any)
                let entry_name = entry_text.split('(').next().unwrap_or("").trim();
                let link_data = line.segments
                    .iter()
                    .find(|seg| seg.text.trim() == entry_name)
                    .and_then(|seg| seg.link_data.clone());

                // Create a single segment for this entry
                let entry_segment = TextSegment {
                    text: entry_text.clone(),
                    fg: line.segments.first().and_then(|s| s.fg.clone()),
                    bg: line.segments.first().and_then(|s| s.bg.clone()),
                    bold: line.segments.first().map(|s| s.bold).unwrap_or(false),
                    mono: false,
                    span_type: crate::data::SpanType::Normal,
                    link_data,
                    custom_emoji: None,
                };

                self.perception_buffer.push(vec![entry_segment]);
                tracing::debug!("Buffered perception entry: '{}'", entry_text);
            }
            self.current_stream = original_stream;
            return;
        }

        let mut text_added_to_any_window = false;
        let mut tts_handled = false;

        // Route via the prebuilt subscriber index (one O(1) lookup per line)
        // instead of scanning every window's stream list. The index is kept in
        // sync by update_text_stream_subscribers at every window/tab mutation.
        //
        // The map is taken out of self for the loop so subscriber names can be
        // borrowed while &mut self methods run - no per-line Vec/String clones.
        // Nothing inside the loop reads text_stream_subscribers; it is restored
        // immediately after (the loop has no early return, only continue).
        let subscribers_map = std::mem::take(&mut self.text_stream_subscribers);
        let trimmed_stream = self.current_stream.trim();
        let subscriber_names: &[String] = match subscribers_map.get(trimmed_stream) {
            Some(v) => v.as_slice(),
            None => {
                let key = trimmed_stream.to_ascii_lowercase();
                subscribers_map.get(&key).map(|v| v.as_slice()).unwrap_or(&[])
            }
        };

        tracing::trace!(
            "Routing stream '{}' to {} subscriber(s)",
            self.current_stream,
            subscriber_names.len()
        );

        // The line may be MOVED into the last subscriber instead of cloned,
        // but only when the redirect-copy pass after this loop won't reuse it.
        // line_slot is Some until (at most) the last iteration takes it.
        let needed_later = should_send_to_original && self.current_stream != original_stream;
        let mut line_slot = Some(line);

        for (idx, window_name) in subscriber_names.iter().enumerate() {
            let is_last = idx + 1 == subscriber_names.len();
            let Some(window) = ui_state.windows.get_mut(window_name) else {
                continue;
            };
            let mut added_here = false;
            match &mut window.content {
                WindowContent::Text(content) => {
                    // Subscription already verified by the index
                    {
                        let is_compact_bounty = content.compact
                            && self.current_stream.eq_ignore_ascii_case("bounty");
                        // Move instead of clone when nothing after this add
                        // needs the line (compact bounty keeps the clone path:
                        // its parse-failure fallback and TTS-skip semantics
                        // depend on the line surviving)
                        let move_line = is_last
                            && !needed_later
                            && deferred_replacements.is_empty()
                            && !is_compact_bounty;

                        if move_line {
                            // TTS reads the line - enqueue before moving it
                            if !tts_handled {
                                if let Some(tts_mgr) = tts_manager.as_deref_mut() {
                                    self.enqueue_tts(
                                        tts_mgr,
                                        window_name,
                                        line_slot.as_ref().expect("line present until moved"),
                                    );
                                }
                                tts_handled = true;
                            }
                            content.add_line(line_slot.take().expect("line moved at most once"));
                            text_added_to_any_window = true;
                            continue;
                        }

                        let src = line_slot.as_ref().expect("line present until moved");
                        // Apply window-specific replacements if any
                        let final_line = if deferred_replacements.is_empty() {
                            src.clone()
                        } else {
                            StyledLine {
                                segments: super::highlight_engine::apply_deferred_for_window(
                                    &src.segments,
                                    &deferred_replacements,
                                    window_name,
                                ),
                                stream: src.stream.clone(),
                                timestamp: src.timestamp,
                            }
                        };

                        // Check for compact bounty mode
                        if is_compact_bounty {
                            // Extract plain text from segments
                            let plain_text: String = final_line.segments.iter().map(|s| s.text.as_str()).collect();
                            if let Some(compact) = bounty_parser::parse_bounty(&plain_text) {
                                // Clear existing lines and add compact bounty lines
                                content.lines.clear();
                                for text in compact.lines {
                                    content.add_line(StyledLine::from_text_with_stream(text, "bounty"));
                                }
                                // Skip normal add_line - we've handled this specially
                                // (matches prior behavior: no TTS, no
                                // text_added_to_any_window for compact bounty)
                                continue;
                            }
                        }

                        content.add_line(final_line);
                        added_here = true;
                    }
                }
                WindowContent::Inventory(content) | WindowContent::Reserve(content) => {
                    if is_last && !needed_later {
                        if !tts_handled {
                            if let Some(tts_mgr) = tts_manager.as_deref_mut() {
                                self.enqueue_tts(
                                    tts_mgr,
                                    window_name,
                                    line_slot.as_ref().expect("line present until moved"),
                                );
                            }
                            tts_handled = true;
                        }
                        content.add_line(line_slot.take().expect("line moved at most once"));
                        text_added_to_any_window = true;
                        continue;
                    }
                    content.add_line(line_slot.as_ref().expect("line present until moved").clone());
                    added_here = true;
                }
                WindowContent::Spells(content) => {
                    if is_last && !needed_later {
                        if !tts_handled {
                            if let Some(tts_mgr) = tts_manager.as_deref_mut() {
                                self.enqueue_tts(
                                    tts_mgr,
                                    window_name,
                                    line_slot.as_ref().expect("line present until moved"),
                                );
                            }
                            tts_handled = true;
                        }
                        content.add_line(line_slot.take().expect("line moved at most once"));
                        text_added_to_any_window = true;
                        continue;
                    }
                    content.add_line(line_slot.as_ref().expect("line present until moved").clone());
                    added_here = true;
                }
                WindowContent::TabbedText(tab_content) => {
                    // Tabs may match multiple times, so this arm always clones
                    let src = line_slot.as_ref().expect("line present until moved");
                    let active_tab_index = tab_content.active_tab_index;
                    for (tab_index, tab) in tab_content.tabs.iter_mut().enumerate() {
                        if tab
                            .definition
                            .streams
                            .iter()
                            .any(|s| s.trim().eq_ignore_ascii_case(&self.current_stream))
                        {
                            // Apply window-specific replacements if any
                            // Check both parent window name and tab name
                            let final_line = if deferred_replacements.is_empty() {
                                src.clone()
                            } else {
                                // Try window name first, then tab name
                                let mut segments = super::highlight_engine::apply_deferred_for_window(
                                    &src.segments,
                                    &deferred_replacements,
                                    window_name,
                                );
                                // Also check tab name (allows targeting specific tabs)
                                segments = super::highlight_engine::apply_deferred_for_window(
                                    &segments,
                                    &deferred_replacements,
                                    &tab.definition.name,
                                );
                                StyledLine {
                                    segments,
                                    stream: src.stream.clone(),
                                    timestamp: src.timestamp,
                                }
                            };
                            tab.content.add_line(final_line);
                            added_here = true;
                            // Mark tab as unread if it's not the active tab and activity tracking is enabled
                            if tab_index != active_tab_index && !tab.definition.ignore_activity {
                                tab.has_unread = true;
                            }
                        }
                    }
                }
                _ => {}
            }

            if added_here {
                text_added_to_any_window = true;
                if let Some(tts_mgr) = tts_manager.as_deref_mut() {
                    if !tts_handled {
                        self.enqueue_tts(
                            tts_mgr,
                            window_name,
                            line_slot.as_ref().expect("line present until moved"),
                        );
                        tts_handled = true; // Avoid multiple TTS calls for the same line
                    }
                }
            }
        }

        // Restore the subscriber index taken before the loop
        self.text_stream_subscribers = subscribers_map;

        // Orphan routing if no subscribed window handled the stream:
        // [streams.routes] entry (discard / main / window:<name>) else the
        // fallback window
        if !text_added_to_any_window {
            // A move implies text was added, so the line is always present here
            let line = line_slot.as_ref().expect("line present when nothing was added");
            match self.resolve_orphaned_stream(&self.current_stream) {
                // resolve_orphaned_stream passes has_subscriber = false, so
                // Subscribed can't come back; nothing to do if it did.
                RouteDecision::Subscribed => {}
                RouteDecision::Discard => {
                    // Routed to discard - drop silently
                    tracing::trace!(
                        "Dropping line from stream '{}' (routed to discard)",
                        self.current_stream
                    );
                    self.chunk_has_silent_updates = true;
                }
                RouteDecision::Deliver { candidates } => {
                    // The first candidate window that exists receives the
                    // line (into its buffer even while hidden). Windows are
                    // never auto-created or auto-opened here; a missing
                    // window:<name> target falls through to the fallback
                    // window, then "main".
                    let mut delivered = false;
                    for target in &candidates {
                        let Some(window) = ui_state.get_window_mut(target) else {
                            continue;
                        };
                        tracing::trace!(
                            "Stream '{}' has no subscribers, routing to '{}'",
                            self.current_stream,
                            target
                        );
                        // First existing candidate wins; as before, a
                        // non-text window swallows the line.
                        if let WindowContent::Text(ref mut content) = window.content {
                            // Apply window-specific replacements if any
                            let final_line = if deferred_replacements.is_empty() {
                                line.clone()
                            } else {
                                StyledLine {
                                    segments: super::highlight_engine::apply_deferred_for_window(
                                        &line.segments,
                                        &deferred_replacements,
                                        target,
                                    ),
                                    stream: line.stream.clone(),
                                    timestamp: line.timestamp,
                                }
                            };
                            content.add_line(final_line);
                            if let Some(tts_mgr) = tts_manager.as_deref_mut() {
                                self.enqueue_tts(tts_mgr, target, &line);
                            }
                        }
                        delivered = true;
                        break;
                    }
                    if !delivered {
                        // Last resort: any shown subscriber of the story
                        // ("main") stream. The window NAMED "main" can be
                        // hidden with the story feed routed into another
                        // window or a tabbedtext tab — mirror
                        // add_system_message's fallback instead of dropping.
                        'fallback: for (win_name, window) in ui_state.windows.iter_mut() {
                            match &mut window.content {
                                WindowContent::Text(content)
                                    if content
                                        .streams
                                        .iter()
                                        .any(|s| s.eq_ignore_ascii_case("main")) =>
                                {
                                    let final_line = if deferred_replacements.is_empty() {
                                        line.clone()
                                    } else {
                                        StyledLine {
                                            segments:
                                                super::highlight_engine::apply_deferred_for_window(
                                                    &line.segments,
                                                    &deferred_replacements,
                                                    win_name,
                                                ),
                                            stream: line.stream.clone(),
                                            timestamp: line.timestamp,
                                        }
                                    };
                                    content.add_line(final_line);
                                    if let Some(tts_mgr) = tts_manager.as_deref_mut() {
                                        self.enqueue_tts(tts_mgr, win_name, line);
                                    }
                                    delivered = true;
                                    break 'fallback;
                                }
                                WindowContent::TabbedText(tab_content) => {
                                    let active_tab_index = tab_content.active_tab_index;
                                    for (tab_index, tab) in
                                        tab_content.tabs.iter_mut().enumerate()
                                    {
                                        if tab
                                            .definition
                                            .streams
                                            .iter()
                                            .any(|s| s.trim().eq_ignore_ascii_case("main"))
                                        {
                                            tab.content.add_line(line.clone());
                                            if tab_index != active_tab_index
                                                && !tab.definition.ignore_activity
                                            {
                                                tab.has_unread = true;
                                            }
                                            delivered = true;
                                        }
                                    }
                                    if delivered {
                                        break 'fallback;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    if !delivered {
                        tracing::trace!(
                            "No routing candidate exists for stream '{}' (tried {:?}), line dropped",
                            self.current_stream,
                            candidates
                        );
                    }
                }
            }
        }

        // Handle redirect_copy mode: also send to original stream
        if should_send_to_original && self.current_stream != original_stream {
            // needed_later excluded this case from the move above
            let line = line_slot.as_ref().expect("redirect-copy line excluded from move");
            // Restore original stream and route line there too
            self.current_stream = original_stream.clone();
            let original_window_name = self.map_stream_to_window(&self.current_stream);

            tracing::debug!(
                "Redirect mode is Copy - also sending to original stream '{}'",
                self.current_stream
            );

            // Route to original window
            if let Some(window) = ui_state.get_window_mut(&original_window_name) {
                match window.content {
                    WindowContent::Text(ref mut content) => {
                        // Apply window-specific replacements if any
                        let final_line = if deferred_replacements.is_empty() {
                            line.clone()
                        } else {
                            StyledLine {
                                segments: super::highlight_engine::apply_deferred_for_window(
                                    &line.segments,
                                    &deferred_replacements,
                                    &original_window_name,
                                ),
                                stream: line.stream.clone(),
                                timestamp: line.timestamp,
                            }
                        };
                        content.add_line(final_line);
                    }
                    WindowContent::Inventory(ref mut content)
                    | WindowContent::Reserve(ref mut content) => {
                        content.add_line(line.clone());
                    }
                    WindowContent::Spells(ref mut content) => {
                        content.add_line(line.clone());
                    }
                    _ => {}
                }
            } else if original_window_name != "main" {
                // Fallback to main for original stream too
                if let Some(main_window) = ui_state.get_window_mut("main") {
                    if let WindowContent::Text(ref mut content) = main_window.content {
                        // Apply window-specific replacements if any
                        let final_line = if deferred_replacements.is_empty() {
                            line.clone()
                        } else {
                            StyledLine {
                                segments: super::highlight_engine::apply_deferred_for_window(
                                    &line.segments,
                                    &deferred_replacements,
                                    "main",
                                ),
                                stream: line.stream.clone(),
                                timestamp: line.timestamp,
                            }
                        };
                        content.add_line(final_line);
                    }
                }
            }
        } else {
            // Restore original stream even if not copying (cleanup)
            self.current_stream = original_stream;
        }
    }

    /// Flush inventory buffer to window (only if content changed)
    pub fn flush_inventory_buffer(&mut self, ui_state: &mut UiState) {
        // If buffer is empty, nothing to do
        if self.inventory_buffer.is_empty() {
            return;
        }

        // Compare to previous inventory
        let inventory_changed = self.inventory_buffer != self.previous_inventory;

        if inventory_changed {
            tracing::debug!(
                "Inventory changed - updating window ({} lines)",
                self.inventory_buffer.len()
            );

            // Find ALL inventory windows and update them (supports multiple inventory windows)
            let mut updated_count = 0;
            for (name, window) in ui_state.windows.iter_mut() {
                if let WindowContent::Inventory(ref mut content) = window.content {
                    // Clear existing content
                    content.lines.clear();

                    // Add all buffered lines
                    for line_segments in &self.inventory_buffer {
                        content.add_line(StyledLine {
                            segments: line_segments.clone(),
                            stream: String::from("inv"),
                            timestamp: None,
                        });
                    }
                    tracing::debug!(
                        "Updated inventory window '{}' with {} lines",
                        name,
                        content.lines.len()
                    );
                    updated_count += 1;
                }
            }

            if updated_count == 0 {
                // Not an error: the feed is still buffered for the registry
                // even with no inventory window open.
                tracing::trace!("Inventory changed; no window open (buffer kept for registry)");
            } else {
                tracing::debug!("Updated {} inventory window(s)", updated_count);
            }

            // Store as new previous inventory. The buffer is cleared below
            // either way, so swapping avoids deep-cloning every line.
            std::mem::swap(&mut self.previous_inventory, &mut self.inventory_buffer);
        } else {
            tracing::debug!(
                "Inventory unchanged - skipping update ({} lines)",
                self.inventory_buffer.len()
            );
        }

        // Clear buffer for next update
        self.inventory_buffer.clear();
    }

    /// Flush reserve buffer to window (only if content changed)
    pub fn flush_reserve_buffer(&mut self, ui_state: &mut UiState) {
        // If buffer is empty, nothing to do
        if self.reserve_buffer.is_empty() {
            return;
        }

        // Compare to previous reserve snapshot
        let reserve_changed = self.reserve_buffer != self.previous_reserve;

        if reserve_changed {
            tracing::debug!(
                "Reserve changed - updating window ({} lines)",
                self.reserve_buffer.len()
            );

            // Find ALL reserve windows and update them
            let mut updated_count = 0;
            for (name, window) in ui_state.windows.iter_mut() {
                if let WindowContent::Reserve(ref mut content) = window.content {
                    // Clear existing content
                    content.lines.clear();

                    // Add all buffered lines
                    for line_segments in &self.reserve_buffer {
                        content.add_line(StyledLine {
                            segments: line_segments.clone(),
                            stream: String::from("reserve"),
                            timestamp: None,
                        });
                    }
                    tracing::debug!(
                        "Updated reserve window '{}' with {} lines",
                        name,
                        content.lines.len()
                    );
                    updated_count += 1;
                }
            }

            if updated_count == 0 {
                tracing::warn!("No reserve windows found to update!");
            } else {
                tracing::debug!("Updated {} reserve window(s)", updated_count);
            }

            // Store as new previous reserve. The buffer is cleared below
            // either way, so swapping avoids deep-cloning every line.
            std::mem::swap(&mut self.previous_reserve, &mut self.reserve_buffer);
        } else {
            tracing::debug!(
                "Reserve unchanged - skipping update ({} lines)",
                self.reserve_buffer.len()
            );
        }

        // Clear buffer for next update
        self.reserve_buffer.clear();
    }

    /// Flush spells buffer to all Spells windows (only if content changed)
    /// Unlike inventory, spells buffer is NOT cleared after flushing because spells
    /// are sent once at login and must persist for newly created windows
    pub fn flush_spells_buffer(&mut self, ui_state: &mut UiState) {
        // If buffer is empty, nothing to do
        if self.spells_buffer.is_empty() {
            return;
        }

        // Compare to previous spells
        let spells_changed = self.spells_buffer != self.previous_spells;

        if spells_changed {
            tracing::debug!(
                "Spells changed - updating window(s) ({} lines)",
                self.spells_buffer.len()
            );

            // Find ALL spells windows and update them (supports multiple spells windows)
            let mut updated_count = 0;
            for (name, window) in ui_state.windows.iter_mut() {
                if let WindowContent::Spells(ref mut content) = window.content {
                    // Clear existing content
                    content.lines.clear();

                    // Add all buffered lines
                    for line_segments in &self.spells_buffer {
                        content.add_line(StyledLine {
                            segments: line_segments.clone(),
                            stream: String::from("Spells"),
                            timestamp: None,
                        });
                    }
                    tracing::debug!(
                        "Updated spells window '{}' with {} lines",
                        name,
                        content.lines.len()
                    );
                    updated_count += 1;
                }
            }

            if updated_count == 0 {
                tracing::debug!("No spells windows found to update (buffer preserved for future windows)");
            } else {
                tracing::debug!("Updated {} spells window(s)", updated_count);
            }

            // Store as new previous spells
            self.previous_spells = self.spells_buffer.clone();
        } else {
        }

        // NOTE: Unlike inventory, we do NOT clear spells_buffer here
        // Spells are sent once at login and must persist for newly created windows
    }

    /// Flush perception buffer to perception window with parsing and sorting
    pub fn flush_perception_buffer(&mut self, ui_state: &mut UiState) {
        // If buffer is empty, nothing to do
        if self.perception_buffer.is_empty() {
            return;
        }

        tracing::debug!(
            "Flushing perception buffer - {} entries",
            self.perception_buffer.len()
        );

        // Parse each buffered entry into PerceptionEntry
        // Note: Entries are already split during buffering, each buffer item is one entry
        let mut entries: Vec<PerceptionEntry> = Vec::new();

        for line_segments in &self.perception_buffer {
            // Get text from segment (should be a single segment with the entry text)
            let text: String = line_segments
                .iter()
                .map(|seg| seg.text.as_str())
                .collect();

            // Skip empty lines
            if text.trim().is_empty() {
                continue;
            }

            // Get link data from segment
            let link_data = line_segments
                .iter()
                .find_map(|seg| seg.link_data.clone());

            entries.push(Self::parse_perception_entry(&text, link_data));
        }

        // TODO: Get configuration from window definitions when available
        // For now, use default sort direction (descending) and no text replacements
        // This will be enhanced in Phase 5 when integrating with widget manager

        // Sort by weight in descending order (highest weight first)
        entries.sort_by(|a, b| b.weight.cmp(&a.weight));

        tracing::debug!(
            "Parsed {} perception entries (sorted by weight descending)",
            entries.len()
        );

        // Update all perception windows
        let mut updated_count = 0;
        for window in ui_state.windows.values_mut() {
            if let WindowContent::Perception(ref old) = window.content {
                window.content = WindowContent::Perception(PerceptionData {
                    entries: entries.clone(),
                    last_update: chrono::Utc::now().timestamp(),
                    generation: old.generation.wrapping_add(1),
                });
                updated_count += 1;
            }
        }

        if updated_count == 0 {
            tracing::debug!("No perception windows found to update");
        } else {
            tracing::debug!("Updated {} perception window(s)", updated_count);
        }

        // Clear buffer for next update
        self.perception_buffer.clear();
    }

    /// Parse a perception entry from text and extract format/weight
    fn parse_perception_entry(text: &str, link_data: Option<LinkData>) -> PerceptionEntry {
        let text = text.trim();

        // Parse format from parenthetical suffix
        let (name, format) = if let Some(paren_start) = text.rfind('(') {
            let name = text[..paren_start].trim().to_string();
            let suffix = &text[paren_start..];

            let format = if suffix == "(OM)" {
                PerceptionFormat::OngoingMagic
            } else if suffix.contains("Indefinite") || suffix.contains("Cyclic") {
                PerceptionFormat::Indefinite
            } else if suffix.contains("Fading") {
                PerceptionFormat::Fading
            } else if suffix.ends_with("%)") {
                // Extract percentage: "(94%)"
                if let Some(pct_str) = suffix.strip_prefix('(').and_then(|s| s.strip_suffix("%)"))
                {
                    if let Ok(pct) = pct_str.parse::<u8>() {
                        PerceptionFormat::Percentage(pct)
                    } else {
                        PerceptionFormat::Other(suffix.to_string())
                    }
                } else {
                    PerceptionFormat::Other(suffix.to_string())
                }
            } else if suffix.contains("roisaen") || suffix.contains("roisan") {
                // Extract roisaen count: "(82 roisaen)"
                let inner = suffix.trim_start_matches('(').trim_end_matches(')');
                if let Some(num_str) = inner.split_whitespace().next() {
                    if let Ok(num) = num_str.parse::<u32>() {
                        PerceptionFormat::Roisaen(num)
                    } else {
                        PerceptionFormat::Other(suffix.to_string())
                    }
                } else {
                    PerceptionFormat::Other(suffix.to_string())
                }
            } else {
                PerceptionFormat::Other(suffix.to_string())
            };

            (name, format)
        } else {
            (text.to_string(), PerceptionFormat::Other(String::new()))
        };

        // Calculate weight for sorting
        let weight = Self::calculate_weight(&format);

        PerceptionEntry {
            name,
            format,
            raw_text: text.to_string(),
            weight,
            link_data,
        }
    }

    /// Calculate sort weight from perception format
    fn calculate_weight(format: &PerceptionFormat) -> i32 {
        match format {
            PerceptionFormat::OngoingMagic => 2000,
            PerceptionFormat::Indefinite => 1500,
            PerceptionFormat::Fading => 0,
            PerceptionFormat::Percentage(pct) => 3000 + (*pct as i32),
            PerceptionFormat::Roisaen(num) => *num as i32,
            PerceptionFormat::Other(_) => 500,
        }
    }

    /// Split concatenated perception entries into individual entries
    ///
    /// The game sends multiple entries concatenated without separators, like:
    /// "Bless  (OM)Auspice  (OM)Divine Radiance  (OM)"
    /// " Monkey (82 roisaen)" (single entry with leading space)
    ///
    /// This function splits them by detecting duration patterns followed by new entry text.
    fn split_perception_entries(text: &str) -> Vec<String> {
        let text = text.trim();
        if text.is_empty() {
            return Vec::new();
        }

        // Patterns that end an entry (duration/status indicators)
        // After these, a new entry begins (if there's more text)
        let end_patterns = [
            "(OM)",
            "(Indefinite)",
            "(Cyclic)",
            "(Fading)",
            "roisaen)",
            "roisan)",
            "%)",
        ];

        let mut entries = Vec::new();
        let mut remaining = text;

        while !remaining.is_empty() {
            // Find the earliest end pattern
            let mut earliest_end: Option<(usize, usize)> = None; // (pattern_start, pattern_len)

            for pattern in &end_patterns {
                if let Some(pos) = remaining.find(pattern) {
                    let end_pos = pos + pattern.len();
                    match earliest_end {
                        None => earliest_end = Some((pos, end_pos)),
                        Some((_, current_end)) if end_pos < current_end => {
                            earliest_end = Some((pos, end_pos))
                        }
                        _ => {}
                    }
                }
            }

            match earliest_end {
                Some((_, end_pos)) => {
                    // Extract this entry (up to and including the end pattern)
                    let entry = remaining[..end_pos].trim();
                    if !entry.is_empty() {
                        entries.push(entry.to_string());
                    }
                    // Continue with remainder
                    remaining = remaining[end_pos..].trim_start();
                }
                None => {
                    // No end pattern found - treat entire remaining text as one entry
                    let entry = remaining.trim();
                    if !entry.is_empty() {
                        entries.push(entry.to_string());
                    }
                    break;
                }
            }
        }

        entries
    }

    /// Map a verbose "who is <phrase>" posture clause to the canonical status
    /// name used by the status_abbrev config. Returns `None` for phrases we
    /// don't recognize so the caller can fall back to the raw phrase (nothing
    /// is silently dropped). "lying down" is confirmed from live logs; the
    /// rest are the standard GemStone postures.
    fn map_verbose_posture(phrase: &str) -> Option<&'static str> {
        match phrase.trim().to_lowercase().as_str() {
            "lying down" => Some("prone"),
            "sitting" => Some("sitting"),
            "kneeling" => Some("kneeling"),
            "standing" => Some("standing"),
            "stunned" => Some("stunned"),
            "prone" => Some("prone"),
            _ => None,
        }
    }

    /// Parse the prefix zone (text before a player's link) into
    /// `(dead, prepended_status)`.
    ///
    /// The zone can carry a corpse marker ("the body of "), a title
    /// ("Lord ", "Arena Occultist "), and/or the legacy article-gated status
    /// form ("a stunned "). Corpse marker sets `dead`; a bare title must NOT
    /// be mistaken for a status (that was the "Arena Occultist -> [Occ]" bug),
    /// so only the article-gated form yields a prepended status.
    fn parse_player_prefix(text: &str) -> (bool, Option<String>) {
        let trimmed = text.trim();
        // Corpse marker: "the body of" immediately before the link. The game
        // may also prefix a title ("the body of Lord X"); we only need the
        // marker to detect death, titles are ignored either way.
        let dead = trimmed.to_lowercase().contains("the body of");

        // Legacy article-gated prepended status ("a stunned ", "an X ").
        // Only fires when the LAST token is preceded by "a "/"an "; a plain
        // title such as "Arena Occultist" or "Lord" has no article and so
        // yields no status.
        let end = text.trim_end();
        let status = end.rfind(' ').and_then(|space_pos| {
            let word = &end[space_pos + 1..];
            let before = &end[..space_pos];
            if before.ends_with(" a") || before == "a" {
                Some(word.to_string())
            } else if before.ends_with(" an") || before == "an" {
                Some(word.to_string())
            } else {
                None
            }
        });

        (dead, status)
    }

    /// Parse the suffix zone (text after a player's link, already bounded at
    /// the next comma) into an optional status.
    ///
    /// Two forms occur in the same component depending on the player's
    /// brief/verbose setting:
    ///   brief:   " (prone)"          -> Some("prone")
    ///   verbose: " who is lying down" -> Some("prone")  (mapped)
    /// Unknown verbose phrases pass through raw so nothing is dropped; the
    /// abbrev layer downstream truncates/abbreviates them.
    fn parse_player_suffix_status(text: &str) -> Option<String> {
        let trimmed = text.trim();

        // Brief parenthetical form.
        if let Some(rest) = trimmed.strip_prefix('(') {
            if let Some(end_paren) = rest.find(')') {
                let inner = rest[..end_paren].trim();
                if !inner.is_empty() {
                    return Some(inner.to_string());
                }
            }
            return None;
        }

        // Verbose "who is <phrase>" clause.
        if let Some(phrase) = trimmed.strip_prefix("who is ") {
            let phrase = phrase.trim().trim_end_matches('.');
            if phrase.is_empty() {
                return None;
            }
            return Some(
                Self::map_verbose_posture(phrase)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| phrase.to_string()),
            );
        }

        None
    }

    /// Enqueue text for TTS if enabled and configured for this window
    /// Replace the set of windows whose defs opt into TTS.
    pub fn set_tts_windows(&mut self, windows: std::collections::HashSet<String>) {
        self.tts_windows = windows;
    }

    /// Refresh the processor's TTS config snapshot. The processor holds its
    /// own Config copy from construction; without this, enabling TTS in the
    /// settings editor wouldn't take effect until restart (enqueue_tts gates
    /// on the stale copy).
    pub fn set_tts_config(&mut self, tts: crate::config::TtsConfig) {
        self.config.tts = tts;
    }

    fn enqueue_tts(&self, tts_manager: &mut crate::tts::TtsManager, window_name: &str, line: &StyledLine) {
        // Early exit if TTS not enabled
        if !self.config.tts.enabled {
            return;
        }

        // Per-window opt-in from the layout def, with the classic config
        // toggles kept for the three windows they always covered.
        let should_speak = self.tts_windows.contains(window_name)
            || match window_name {
                "thoughts" => self.config.tts.speak_thoughts,
                "speech" => self.config.tts.speak_speech,
                "main" => self.config.tts.speak_main,
                _ => false,
            };

        if !should_speak {
            return;
        }

        // Extract clean text from line segments
        let text: String = line.segments.iter().map(|seg| seg.text.as_str()).collect();

        // Skip empty text
        if text.trim().is_empty() {
            return;
        }

        // Skip prompts (single character lines like ">")
        if text.trim().len() <= 1 {
            tracing::trace!("Skipping TTS for single-character prompt: {:?}", text.trim());
            return;
        }

        // Chronological queue: the manager auto-plays when idle and chains
        // from the utterance-end callback - nothing to trigger here, and
        // new lines never interrupt the one being spoken.
        tts_manager.enqueue(crate::tts::SpeechEntry {
            text,
            source_window: window_name.to_string(),
            priority: crate::tts::Priority::Normal,
            spoken: false,
            repeats: 1,
        });
    }

    /// Map stream ID to window name
    fn map_stream_to_window(&self, stream: &str) -> String {
        match stream {
            "main" => "main",
            "room" => "room",
            "inv" => "inventory",
            "reserve" => "reserve",
            "thoughts" => "thoughts",
            "speech" => "speech",
            "announcements" => "announcements",
            "loot" => "loot",
            "death" => "death",
            "logons" => "logons",
            "familiar" => "familiar",
            "ambients" => "ambients",
            "bounty" => "bounty",
            "Spells" => "spells",
            "percWindow" => "perception",
            _ => "main", // Default to main window
        }
        .to_string()
    }

    /// Determine if a stream is already handled by any window.
    /// Uses the pre-built subscriber map for O(1) lookup.
    fn stream_has_target_window(&self, ui_state: &UiState, stream: &str) -> bool {
        // First check the pre-built subscriber map (text windows, tabbed text, etc.)
        if self.stream_has_subscribers(stream) {
            return true;
        }
        let _ = ui_state;
        false
    }

    /// Determine what to do with an orphaned stream (no subscribers):
    /// `[streams.routes]` entry (discard / main / window:<name>) if present,
    /// else the fallback window. Never returns `RouteDecision::Subscribed`.
    fn resolve_orphaned_stream(&self, stream: &str) -> RouteDecision {
        route_for(
            stream,
            false,
            &self.config.streams.routes,
            &self.config.streams.fallback,
        )
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
            tracing::debug!("Spells buffer is empty - new window will remain empty until data arrives");
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

    /// Update squelch pattern matching infrastructure from config
    pub fn update_squelch_patterns(&mut self) {
        // Collect all squelch patterns
        let squelch_patterns: Vec<_> = self
            .config
            .highlights
            .values()
            .filter(|pattern| pattern.squelch)
            .collect();

        // Build Aho-Corasick for fast_parse patterns
        let mut fast_patterns = Vec::new();
        for pattern in squelch_patterns.iter().filter(|p| p.fast_parse) {
            // Split pattern on | for literal matching
            for literal in pattern.pattern.split('|') {
                let trimmed = literal.trim();
                if !trimmed.is_empty() {
                    fast_patterns.push(trimmed.to_string());
                }
            }
        }

        if !fast_patterns.is_empty() {
            self.squelch_matcher = aho_corasick::AhoCorasickBuilder::new()
                .match_kind(aho_corasick::MatchKind::Standard)
                .build(&fast_patterns)
                .ok();
        } else {
            self.squelch_matcher = None;
        }

        // Compile regex patterns
        self.squelch_regexes = squelch_patterns
            .iter()
            .filter(|p| !p.fast_parse)
            .filter_map(|p| regex::Regex::new(&p.pattern).ok())
            .collect();

        tracing::debug!(
            "Updated squelch patterns: {} fast patterns, {} regex patterns",
            fast_patterns.len(),
            self.squelch_regexes.len()
        );
    }

    /// Rebuild the redirect matchers from config.
    /// Fast-parse literals go into one Aho-Corasick matcher (pattern ids index
    /// redirect_literal_meta); regex patterns are pre-collected. This replaces
    /// re-splitting every pattern on '|' for every line.
    pub fn update_redirect_cache(&mut self) {
        let redirect_patterns: Vec<_> = self
            .config
            .highlights
            .values()
            .filter(|p| p.redirect_to.is_some() && !p.squelch)
            .collect();

        let mut literals = Vec::new();
        self.redirect_literal_meta.clear();
        self.redirect_regexes = Vec::new();

        for pattern in &redirect_patterns {
            let window = pattern
                .redirect_to
                .clone()
                .expect("redirect_to filtered above");
            if pattern.fast_parse {
                let mut saw_literal = false;
                for literal in pattern.pattern.split('|') {
                    let trimmed = literal.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    saw_literal = true;
                    literals.push(trimmed.to_string());
                    self.redirect_literal_meta
                        .push((window.clone(), pattern.redirect_mode.clone()));
                }
                if !saw_literal {
                    tracing::warn!(
                        "Skipping fast-parse redirect with no usable literals: '{}'",
                        pattern.pattern
                    );
                }
            } else if let Some(regex) = &pattern.compiled_regex {
                self.redirect_regexes
                    .push((regex.clone(), window, pattern.redirect_mode.clone()));
            }
        }

        self.redirect_matcher = if literals.is_empty() {
            None
        } else {
            // MatchKind::Standard + find_overlapping_iter reproduces the old
            // "longest contained literal wins" semantics; LeftmostLongest
            // would miss a longer literal starting later in the line
            aho_corasick::AhoCorasickBuilder::new()
                .match_kind(aho_corasick::MatchKind::Standard)
                .build(&literals)
                .ok()
        };

        self.has_redirect_highlights =
            self.redirect_matcher.is_some() || !self.redirect_regexes.is_empty();

        tracing::debug!(
            "Updated redirect cache: {} literals, {} regexes",
            literals.len(),
            self.redirect_regexes.len()
        );
    }

    /// Record that a stream id was seen this session, upgrading its label if a
    /// friendly title arrives later. Called for every pushStream/streamWindow so
    /// the custom-window picker can offer streams Lich actually sent. The `main`
    /// stream is always present and not worth listing, so it is skipped.
    fn note_seen_stream(&mut self, id: &str, title: Option<&str>) {
        let id = id.trim();
        if id.is_empty() || id.eq_ignore_ascii_case("main") {
            return;
        }
        let label = title.map(str::trim).filter(|t| !t.is_empty());
        match self.seen_streams.get_mut(id) {
            Some(existing) => {
                // Only fill in a label; never clobber a known one with None.
                if existing.is_none() {
                    if let Some(label) = label {
                        *existing = Some(label.to_string());
                    }
                }
            }
            None => {
                self.seen_streams
                    .insert(id.to_string(), label.map(str::to_string));
            }
        }
    }

    /// Streams seen this session as `(id, optional friendly label)`, sorted by id.
    /// Consumed by the custom-window authoring panel's "seen this session" list.
    pub fn seen_streams(&self) -> Vec<(String, Option<String>)> {
        self.seen_streams
            .iter()
            .map(|(id, label)| (id.clone(), label.clone()))
            .collect()
    }

    /// Build the text stream subscriber map from widget configurations.
    /// Call this on startup and after layout reload to update routing.
    pub fn update_text_stream_subscribers(&mut self, ui_state: &UiState) {
        let mut subscribers: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        // Keys are canonicalized (trimmed + ascii-lowercased) so routing can do
        // one O(1) lookup per line instead of eq_ignore_ascii_case per window.
        // Dedupe so a window with the same stream on multiple tabs appears once.
        let mut add = |subscribers: &mut std::collections::HashMap<String, Vec<String>>,
                       stream: &str,
                       window_name: &String| {
            let entry = subscribers
                .entry(stream.trim().to_ascii_lowercase())
                .or_default();
            if !entry.contains(window_name) {
                entry.push(window_name.clone());
            }
        };

        for (window_name, window) in &ui_state.windows {
            match &window.content {
                // Text windows have explicit streams field
                WindowContent::Text(content) => {
                    for stream in &content.streams {
                        add(&mut subscribers, stream, window_name);
                    }
                }

                // Tabbed text windows: each tab has its own streams
                WindowContent::TabbedText(tabbed) => {
                    for tab in &tabbed.tabs {
                        for stream in &tab.definition.streams {
                            add(&mut subscribers, stream, window_name);
                        }
                    }
                }

                // Inventory widget uses its streams field (like Text windows)
                WindowContent::Inventory(content) => {
                    for stream in &content.streams {
                        add(&mut subscribers, stream, window_name);
                    }
                }

                // Reserve widget uses its streams field (like Text windows)
                WindowContent::Reserve(content) => {
                    for stream in &content.streams {
                        add(&mut subscribers, stream, window_name);
                    }
                }

                // Spells widget uses its streams field (like Text windows)
                WindowContent::Spells(content) => {
                    for stream in &content.streams {
                        add(&mut subscribers, stream, window_name);
                    }
                }

                // Perception widget implicitly subscribes to "percWindow" stream
                WindowContent::Perception(_) => {
                    add(&mut subscribers, "percWindow", window_name);
                }

                // Hand widgets implicitly subscribe to left/right/spell streams
                WindowContent::Hand { .. } => {
                    // Hand type is determined by window name convention
                    let hand_stream = match window_name.as_str() {
                        "left" | "lefthand" | "left_hand" => Some("left"),
                        "right" | "righthand" | "right_hand" => Some("right"),
                        "spell" | "spellhand" | "spell_hand" => Some("spell"),
                        _ => None,
                    };
                    if let Some(stream) = hand_stream {
                        add(&mut subscribers, stream, window_name);
                    }
                }

                // Targets widget uses component-based approach (GameState.room_creatures)
                // No stream subscription needed
                WindowContent::Targets => {
                    // No-op - component-based widget
                }

                // Players widget uses component-based approach (GameState.room_players)
                // No stream subscription needed
                WindowContent::Players => {
                    // No-op - component-based widget
                }

                // Items widget uses component-based approach (GameState.room_objects)
                // No stream subscription needed
                WindowContent::Items => {
                    // No-op - component-based widget
                }

                // Room widget implicitly subscribes to "room" stream
                WindowContent::Room(_) => {
                    add(&mut subscribers, "room", window_name);
                }

                // ActiveEffects implicitly subscribes to multiple streams
                WindowContent::ActiveEffects(_) => {
                    for stream in &["activespells", "buffs", "debuffs", "cooldowns"] {
                        add(&mut subscribers, stream, window_name);
                    }
                }

                // Other widget types don't subscribe to text streams
                _ => {}
            }
        }

        let stream_count = subscribers.len();
        let total_subscriptions: usize = subscribers.values().map(|v| v.len()).sum();

        self.text_stream_subscribers = subscribers;

        tracing::debug!(
            "Updated text stream subscribers: {} streams, {} total subscriptions",
            stream_count,
            total_subscriptions
        );
    }

    /// Get subscribers for a stream (returns empty vec if none).
    /// Lookup is case-insensitive; map keys are canonical (trimmed, lowercase).
    pub fn get_stream_subscribers(&self, stream: &str) -> &[String] {
        let trimmed = stream.trim();
        // Fast path: already-canonical key needs no allocation
        if let Some(v) = self.text_stream_subscribers.get(trimmed) {
            return v.as_slice();
        }
        self.text_stream_subscribers
            .get(&trimmed.to_ascii_lowercase())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if a stream has any subscribers
    pub fn stream_has_subscribers(&self, stream: &str) -> bool {
        !self.get_stream_subscribers(stream).is_empty()
    }

    /// Check if a line matches a redirect pattern
    /// Returns (redirect_window_name, redirect_mode, match_length) if matched
    /// Squelch patterns are excluded (squelch takes precedence)
    /// Longest match wins when multiple patterns match
    fn check_redirect_match(
        &self,
        text: &str,
    ) -> Option<(String, crate::config::RedirectMode, usize)> {
        // Check if redirects are globally enabled
        if !self.config.highlight_settings.redirect_enabled {
            return None;
        }

        // Lazy check: skip if no redirects configured
        if !self.has_redirect_highlights {
            return None;
        }

        // Longest match wins, across literals and regexes; ties keep the
        // first seen (matching the old strictly-greater comparison)
        let mut best: Option<(&str, &crate::config::RedirectMode, usize)> = None;

        if let Some(matcher) = &self.redirect_matcher {
            for m in matcher.find_overlapping_iter(text) {
                let len = m.end() - m.start();
                if best.as_ref().is_none_or(|(_, _, best_len)| len > *best_len) {
                    let (window, mode) = &self.redirect_literal_meta[m.pattern().as_usize()];
                    best = Some((window, mode, len));
                }
            }
        }

        for (regex, window, mode) in &self.redirect_regexes {
            if let Some(m) = regex.find(text) {
                let len = m.end() - m.start();
                if best.as_ref().is_none_or(|(_, _, best_len)| len > *best_len) {
                    best = Some((window, mode, len));
                }
            }
        }

        best.map(|(window, mode, len)| (window.to_string(), mode.clone(), len))
    }

    /// Check if a line should be squelched (ignored/filtered)
    fn should_squelch_line(&self, text: &str) -> bool {
        // Check Aho-Corasick fast patterns
        if let Some(ref matcher) = self.squelch_matcher {
            if matcher.is_match(text) {
                return true;
            }
        }

        // Check regex patterns
        for regex in &self.squelch_regexes {
            if regex.is_match(text) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // Stream routing precedence (route_for)
    // ===========================================

    fn routes(
        entries: &[(&str, StreamRoute)],
    ) -> std::collections::BTreeMap<String, StreamRoute> {
        entries
            .iter()
            .map(|(id, route)| (id.to_string(), route.clone()))
            .collect()
    }

    fn deliver(candidates: &[&str]) -> RouteDecision {
        RouteDecision::Deliver {
            candidates: candidates.iter().map(|c| c.to_string()).collect(),
        }
    }

    #[test]
    fn route_subscribed_window_always_wins() {
        // Even a discard route loses to a subscribed window.
        let map = routes(&[("speech", StreamRoute::Discard)]);
        assert_eq!(route_for("speech", true, &map, "main"), RouteDecision::Subscribed);
    }

    #[test]
    fn route_discard_drops_orphaned_stream() {
        let map = routes(&[("speech", StreamRoute::Discard)]);
        assert_eq!(route_for("speech", false, &map, "main"), RouteDecision::Discard);
        // Lookup is case-insensitive, matching the legacy drop list.
        assert_eq!(route_for("SPEECH", false, &map, "main"), RouteDecision::Discard);
    }

    #[test]
    fn route_main_delivers_to_main() {
        let map = routes(&[("ooc", StreamRoute::Main)]);
        assert_eq!(route_for("ooc", false, &map, "story"), deliver(&["main"]));
    }

    #[test]
    fn route_window_prefers_window_then_fallback_then_main() {
        let map = routes(&[("bounty", StreamRoute::Window("bounty".to_string()))]);
        // Delivery takes the first candidate window that exists, so a
        // missing "bounty" window falls back to "story", then "main" —
        // never auto-creating or auto-opening anything.
        assert_eq!(
            route_for("bounty", false, &map, "story"),
            deliver(&["bounty", "story", "main"])
        );
        // Duplicates collapse (fallback already "main").
        assert_eq!(
            route_for("bounty", false, &map, "main"),
            deliver(&["bounty", "main"])
        );
    }

    #[test]
    fn route_unrouted_stream_keeps_fallback_behavior() {
        let map = routes(&[("speech", StreamRoute::Discard)]);
        assert_eq!(route_for("bounty", false, &map, "story"), deliver(&["story", "main"]));
        assert_eq!(route_for("bounty", false, &map, "main"), deliver(&["main"]));
        let empty = routes(&[]);
        assert_eq!(route_for("anything", false, &empty, "main"), deliver(&["main"]));
    }

    // ===========================================
    // Helper function to create minimal processor for testing
    // ===========================================

    fn create_test_processor() -> MessageProcessor {
        let config = Config::default();
        MessageProcessor::new(config, SavedDialogPositions::default())
    }

    fn make_redirect_pattern(pattern: &str) -> crate::config::HighlightPattern {
        crate::config::HighlightPattern {
            pattern: pattern.to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: true,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: Some("alerts".to_string()),
            redirect_mode: crate::config::RedirectMode::RedirectOnly,
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            compiled_regex: None,
        }
    }

    // ===========================================
    // GameObjects registry dual-write (migration step 2)
    // ===========================================

    #[test]
    fn inventory_scan_captures_status_then_prompt_writes_registry() {
        use crate::data::widget::{LinkData, TextSegment};
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();

        // Start the scan (the caller would send the returned command).
        assert_eq!(processor.start_inventory_scan(), Some("inventory full"));
        assert!(processor.inventory_scan_in_flight());
        // Starting again while in flight is a no-op.
        assert_eq!(processor.start_inventory_scan(), None);

        // Feed reply lines as segments (what the flush path would pass).
        let link = |id: &str, noun: &str, name: &str| TextSegment {
            text: name.to_string(),
            link_data: Some(LinkData {
                exist_id: id.to_string(),
                noun: noun.to_string(),
                text: name.to_string(),
                coord: None,
            }),
            ..Default::default()
        };
        // header (no link) — captured for the window, no status.
        processor.inv_scan.ingest_segments(&[TextSegment::plain(
            "You are currently wearing:",
        )]);
        processor.inv_scan.ingest_segments(&[
            TextSegment::plain("  some "),
            link("1", "gloves", "triton hide gloves"),
            TextSegment::plain(" with knuckles (registered) (marked)"),
        ]);
        processor.inv_scan.ingest_segments(&[
            TextSegment::plain("  a "),
            link("2", "ring", "plain ring"),
        ]);

        // The prompt finalizes into the registry.
        let prompt = ParsedElement::Prompt {
            time: "0".to_string(),
            text: ">".to_string(),
        };
        processor.process_element(
            &prompt,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );

        assert!(!processor.inventory_scan_in_flight());
        let s1 = game_state.objects.status_of("1").unwrap();
        assert_eq!(s1.registered, Some(true));
        assert_eq!(s1.marked, Some(true));
        let s2 = game_state.objects.status_of("2").unwrap();
        assert_eq!(s2.registered, Some(false), "in reply, no marker = false");
        assert_eq!(s2.marked, Some(false));
    }

    #[test]
    fn discovery_routes_container_signal_bank_popup_stream_queue() {
        // U3: no offer registry. A container sets the newly_registered
        // signal; a dialog (bank) pops up only once the user shows it; a
        // streamWindow pushes a WindowDiscovery for AppCore to bind.
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();

        let feed = [
            ParsedElement::Container {
                id: "77".to_string(),
                title: "Backpack".to_string(),
                target: Some("#77".to_string()),
            },
            ParsedElement::DialogOpen {
                id: "bank".to_string(),
                title: Some("Bank".to_string()),
                save: true,
            },
            ParsedElement::StreamWindow {
                id: "thoughts".to_string(),
                subtitle: None,
                title: Some("Thoughts".to_string()),
            },
        ];
        for element in &feed {
            processor.process_element(
                element,
                &mut game_state,
                &mut ui_state,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }

        // Container → registry + newly-registered signal.
        assert!(game_state.objects.container("77").is_some());
        assert_eq!(
            processor.newly_registered_container,
            Some(("77".to_string(), "Backpack".to_string()))
        );
        // U6: bank does NOT pop up by default (hidden-until-shown —
        // nothing pops unless its id is in shown_dialog_ids).
        assert!(ui_state.active_dialog.is_none());
        // Stream → a WindowDiscovery for AppCore to register.
        let disc = &ui_state.pending_window_discoveries;
        assert!(disc.iter().any(|d| d.id == "thoughts"
            && d.kind == crate::data::WindowDiscoveryKind::Stream));

        // But once the user shows "bank", its re-sent openDialog pops up.
        ui_state.shown_dialog_ids.insert("bank".to_string());
        processor.process_element(
            &ParsedElement::DialogOpen {
                id: "bank".to_string(),
                title: Some("Bank".to_string()),
                save: true,
            },
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
        assert!(ui_state.active_dialog.as_ref().is_some_and(|d| d.id == "bank"));
    }

    #[test]
    fn dialog_popup_gated_on_shown_dialog_ids() {
        // U6: a dialog pops up ONLY if the user has shown it (its id in
        // shown_dialog_ids). Empty set = nothing pops up.
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();
        let open = |id: &str| ParsedElement::DialogOpen {
            id: id.to_string(),
            title: Some(id.to_string()),
            save: false,
        };

        // Not shown → no popup.
        processor.process_element(
            &open("shop"),
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
        assert!(ui_state.active_dialog.is_none());

        // Shown → pops up.
        ui_state.shown_dialog_ids.insert("shop".to_string());
        processor.process_element(
            &open("shop"),
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
        assert!(ui_state.active_dialog.as_ref().is_some_and(|d| d.id == "shop"));
    }

    #[test]
    fn hidden_combat_dialogdata_never_opens_popup() {
        // Real shapes from a 2026-07-28 session log: the combat window is a
        // RESIDENT openDialog (so no DialogOpen is emitted) whose dialogData
        // then arrives both embedded and standalone. The user never showed
        // 'combat', so none of it may create the generic popup.
        let mut parser = crate::parser::XmlParser::new();
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();

        let lines = [
            "<openDialog type='dynamic' id='combat' title='Combat' location='right' target='combat' height='288' resident='true'><dialogData id='combat' clear='t'><image id='unsheathe' name='SwordBtn' cmd='_ready weapon' tooltip='Unsheathe Weapon' echo='ready weapon' align='n' top='3' left='-50' height='29' width='29'/></dialogData></openDialog>",
            "<dialogData id='combat'><progressBar id='pbarStance' value='100' text='defensive (100%)' top='51' width='130' height='16' left='0' align='n' tooltip='Percent of stance contributing to defense'/></dialogData>",
            "<dialogData id='combat'><cmdButton id='cmdDefStance' value='defense' cmd='_stance defensive' tooltip='Assume a Defensive Stance' echo='stance defensive' height='20' width='55' top='70' left='0' align='nw'/><cmdButton id='cmdTarget' value='target' cmd='target random' tooltip='Select a Random Target' height='20' width='55' top='93' left='0' align='nw'/></dialogData>",
        ];
        for line in &lines {
            for element in parser.parse_line(line) {
                processor.process_element(
                    &element,
                    &mut game_state,
                    &mut ui_state,
                    &mut std::collections::HashMap::new(),
                    &mut None,
                    &mut false,
                    &mut None,
                    &mut None,
                    &mut None,
                    None,
                );
            }
        }

        assert!(
            ui_state.active_dialog.is_none(),
            "hidden combat dialogData opened the generic popup: {:?}",
            ui_state.active_dialog.as_ref().map(|d| &d.id)
        );
        // It was recorded as a DialogPanel discovery (Hidden by default).
        let disc = ui_state
            .pending_window_discoveries
            .iter()
            .find(|d| d.id == "combat");
        assert!(
            disc.is_some_and(|d| d.kind == crate::data::WindowDiscoveryKind::DialogPanel),
            "combat should be a DialogPanel discovery"
        );
        // Even hidden, its full state accumulated in the store, so showing
        // it later renders fully formed rather than from deltas.
        let stored = ui_state.dialog_store.get("combat").expect("combat stored");
        assert_eq!(stored.progress_bars.len(), 1, "stance bar stored");
        assert_eq!(stored.buttons.len(), 2, "both stance/target buttons stored");
    }

    #[test]
    fn combat_registers_as_resident_and_ingests_all_controls() {
        // The real login-time combat panel (2026-01 log): resident
        // openDialog + the full set of dialogData chunks. It must register
        // as a RESIDENT dialog offer and accumulate every control type in
        // the store (icons, links, spinbox, buttons, dropdowns, bar).
        let mut parser = crate::parser::XmlParser::new();
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();

        let lines = [
            "<openDialog type='dynamic' id='combat' title='Combat' location='right' target='combat' height='288' resident='true'><dialogData id='combat' clear='t'><image id='unsheathe' name='SwordBtn' cmd='_ready weapon' tooltip='Unsheathe Weapon' align='n' top='3' left='-50' height='29' width='29'/><link id='lnConfigure' value='configure' cmd='_cmbtpl configure dialog' top='30' align='n' left='0'/></dialogData></openDialog>",
            "<dialogData id='combat'><progressBar id='pbarStance' value='100' text='defensive (100%)' top='51' width='130' height='16' left='0' align='n'/></dialogData>",
            "<dialogData id='combat'><cmdButton id='cmdDefStance' value='defense' cmd='_stance defensive' top='70' left='0' align='nw'/><cmdButton id='cmdOffStance' value='offense' cmd='_stance offensive' top='70' left='0' align='ne'/></dialogData>",
            "<dialogData id='combat'><dropDownBox id='dDBStance' value='defensive' cmd='_stance %dDBStance%' content_text='offensive,defensive' content_value='offensive,defensive' top='70' anchor_left='cmdDefStance' anchor_right='cmdOffStance'/></dialogData>",
            "<dialogData id='combat'><upDownEditBox id='uDEQuickstrike' min='-60' max='60' value='-1' top='231' left='0' width='50' height='26'/><cmdButton id='cmdQuickstrike' value='prepare to quickstrike' cmd='quickstrike %uDEQuickstrike%' top='234' left='53'/></dialogData>",
            "<dialogData id='combat'><link id='lnSkin' value='skin' cmd='_skin' top='260' left='0'/><link id='mstrike' value='multistrike' cmd='mstrike'/></dialogData>",
        ];
        for line in &lines {
            for element in parser.parse_line(line) {
                processor.process_element(
                    &element,
                    &mut game_state,
                    &mut ui_state,
                    &mut std::collections::HashMap::new(),
                    &mut None,
                    &mut false,
                    &mut None,
                    &mut None,
                    &mut None,
                    None,
                );
            }
        }

        // Combat is recorded as a DialogPanel discovery (Hidden by default)
        // and never pops up as a transient dialog.
        let disc = ui_state
            .pending_window_discoveries
            .iter()
            .find(|d| d.id == "combat")
            .expect("combat discovery");
        assert_eq!(disc.kind, crate::data::WindowDiscoveryKind::DialogPanel);
        assert!(ui_state.active_dialog.is_none(), "no transient popup");

        // Store accumulated the whole panel.
        let s = ui_state.dialog_store.get("combat").expect("stored");
        assert_eq!(s.images.len(), 1, "sword icon");
        assert_eq!(s.buttons.len(), 3, "defense + offense + quickstrike");
        assert_eq!(s.dropdowns.len(), 1, "stance");
        assert_eq!(s.spinboxes.len(), 1, "quickstrike offset");
        assert_eq!(s.progress_bars.len(), 1, "stance bar");
        assert_eq!(s.links.len(), 3, "configure + skin + multistrike");
        // %id% resolves the spinbox value in a button command.
        assert_eq!(
            s.command_with_placeholders("quickstrike %uDEQuickstrike%"),
            "quickstrike -1"
        );
    }

    #[test]
    fn always_ingest_store_accumulates_hidden_dialog() {
        // The core fix: the game sends combat's definition (here as a batch);
        // combat was never shown so no popup appears, but the store ingests
        // the whole panel so showing it later renders fully formed.
        let mut parser = crate::parser::XmlParser::new();
        let mut processor = create_test_processor();
        let mut app = crate::core::AppCore::new_for_test();

        let lines = [
            "<dialogData id='combat' clear='t'><progressBar id='pbarStance' value='100' text='defensive (100%)' top='51'/></dialogData>",
            "<dialogData id='combat'><cmdButton id='cmdDefStance' value='defense' cmd='_stance defensive' top='70' left='0' align='nw'/><cmdButton id='cmdAttack' value='attack' cmd='attack' top='93' left='55' align='ne'/><dropDownBox id='dDBStance' value='defensive' cmd='_stance %dDBStance%' content_text='offensive,defensive' content_value='offensive,defensive' top='70'/></dialogData>",
        ];
        for line in &lines {
            for element in parser.parse_line(line) {
                processor.process_element(
                    &element,
                    &mut app.game_state,
                    &mut app.ui_state,
                    &mut std::collections::HashMap::new(),
                    &mut None,
                    &mut false,
                    &mut None,
                    &mut None,
                    &mut None,
                    None,
                );
            }
        }

        // Hidden → no transient popup, but fully stored.
        assert!(app.ui_state.active_dialog.is_none());
        let stored = app.ui_state.dialog_store.get("combat").expect("stored");
        assert_eq!(stored.buttons.len(), 2);
        assert_eq!(stored.dropdowns.len(), 1);
        assert_eq!(stored.progress_bars.len(), 1);
    }

    #[test]
    fn shown_dialog_updates_replace_controls_by_id() {
        // The always-ingest store replaces same-id controls (no pile-up) on
        // every dialogData refresh — independent of whether it's shown.
        // Assert against the store (the update-by-id happens there).
        let mut parser = crate::parser::XmlParser::new();
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();

        let chunk = "<dialogData id='combat'><cmdButton id='cmdTarget' value='target' cmd='target random' top='93' left='0'/><cmdButton id='cmdAttack' value='attack' cmd='attack' top='93' left='55'/><dropDownBox id='dDBStance' value='defensive' cmd='_stance %dDBStance%' content_text='offensive,defensive' content_value='offensive,defensive' top='70'/></dialogData>";
        let updated = "<dialogData id='combat'><dropDownBox id='dDBStance' value='offensive' cmd='_stance %dDBStance%' content_text='offensive,defensive' content_value='offensive,defensive' top='70'/></dialogData>";
        for line in [chunk, chunk, updated] {
            for element in parser.parse_line(line) {
                processor.process_element(
                    &element,
                    &mut game_state,
                    &mut ui_state,
                    &mut std::collections::HashMap::new(),
                    &mut None,
                    &mut false,
                    &mut None,
                    &mut None,
                    &mut None,
                    None,
                );
            }
        }

        let dialog = ui_state.dialog_store.get("combat").expect("stored");
        // Re-sent controls replaced their same-id entries, no pile-up
        // (the old extend produced target/attack duplicates live).
        assert_eq!(dialog.buttons.len(), 2, "buttons: {:?}", dialog.buttons);
        assert_eq!(dialog.dropdowns.len(), 1);
        // The refresh updated the dropdown's current value...
        assert_eq!(dialog.dropdowns[0].value, "offensive");
        // ...which %id% substitution resolves in sibling commands.
        assert_eq!(
            dialog.command_with_placeholders("_stance %dDBStance%"),
            "_stance offensive"
        );
    }

    #[test]
    fn container_feed_populates_registry_in_parallel() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();

        // Real look-in-container sequence: <container> then
        // <clearContainer> then header + item <inv> lines.
        let feed = [
            ParsedElement::Container {
                id: "77".to_string(),
                title: "Bandolier".to_string(),
                target: Some("#77".to_string()),
            },
            ParsedElement::ClearContainer { id: "77".to_string() },
            ParsedElement::ContainerItem {
                container_id: "77".to_string(),
                content: r#"In the <a exist="77" noun="bandolier">bandolier</a>:"#
                    .to_string(),
            },
            ParsedElement::ContainerItem {
                container_id: "77".to_string(),
                content: r#" a <a exist="101" noun="crystal">quartz crystal</a>"#
                    .to_string(),
            },
            ParsedElement::ContainerItem {
                container_id: "77".to_string(),
                content: r#" a <a exist="102" noun="sword">short sword</a>"#.to_string(),
            },
        ];
        for element in &feed {
            processor.process_element(
                element,
                &mut game_state,
                &mut ui_state,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }

        // Registry holds the two items, header skipped, ids intact.
        let items = game_state.objects.items_in("77");
        assert_eq!(items.len(), 2, "header excluded, both items kept");
        assert_eq!(items[0].id, "101");
        assert_eq!(items[0].name, "quartz crystal");
        assert_eq!(items[1].id, "102");
    }

    #[test]
    fn stow_container_feed_targets_object_in_registry() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();

        let feed = [
            ParsedElement::Container {
                id: "stow".to_string(),
                title: "My Shroud".to_string(),
                target: Some("#691".to_string()),
            },
            ParsedElement::ClearContainer { id: "stow".to_string() },
            ParsedElement::ContainerItem {
                container_id: "stow".to_string(),
                content: r#"In the <a exist="691" noun="shroud">shroud</a>:"#.to_string(),
            },
            ParsedElement::ContainerItem {
                container_id: "stow".to_string(),
                content: r#" a <a exist="742" noun="feather">disir feather</a>"#
                    .to_string(),
            },
        ];
        for element in &feed {
            processor.process_element(
                element,
                &mut game_state,
                &mut ui_state,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }

        // Header (the shroud object) skipped via command_target, feather
        // kept; the command target is the object id, not "#stow".
        let items = game_state.objects.items_in("stow");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "742");
        assert_eq!(
            game_state.objects.container("stow").unwrap().command_target(),
            "691"
        );
    }

    // ===========================================
    // Active effect expiry derivation
    // ===========================================

    #[test]
    fn test_active_effect_derives_expires_at_from_game_time() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();
        game_state.game_time = 1_000_000;

        let element = ParsedElement::ActiveEffect {
            category: "Buffs".to_string(),
            id: "509".to_string(),
            value: 92,
            text: "Strength of the Bull".to_string(),
            time: "00:01:05".to_string(),
        };
        processor.process_element(
            &element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );

        let store = game_state.effects.get("Buffs").expect("Buffs store");
        assert_eq!(store.effects.len(), 1);
        assert_eq!(store.effects[0].expires_at, Some(1_000_065));

        // Unparseable duration -> no expiry
        let element = ParsedElement::ActiveEffect {
            category: "Buffs".to_string(),
            id: "905".to_string(),
            value: 100,
            text: "Prestidigitation".to_string(),
            time: "Indefinite".to_string(),
        };
        processor.process_element(
            &element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
        let store = game_state.effects.get("Buffs").expect("Buffs store");
        let indef = store.effects.iter().find(|e| e.id == "905").unwrap();
        assert_eq!(indef.expires_at, None);
    }

    // ===========================================
    // seen-streams registry (custom-window authoring source)
    // ===========================================

    #[test]
    fn test_note_seen_stream_records_ids_sorted() {
        let mut processor = create_test_processor();
        processor.note_seen_stream("familiar", None);
        processor.note_seen_stream("bounty", None);
        let seen = processor.seen_streams();
        assert_eq!(
            seen,
            vec![
                ("bounty".to_string(), None),
                ("familiar".to_string(), None),
            ]
        );
    }

    #[test]
    fn test_note_seen_stream_skips_main_and_blank() {
        let mut processor = create_test_processor();
        processor.note_seen_stream("main", None);
        processor.note_seen_stream("MAIN", None);
        processor.note_seen_stream("   ", None);
        assert!(processor.seen_streams().is_empty());
    }

    #[test]
    fn test_note_seen_stream_label_fills_without_clobber() {
        let mut processor = create_test_processor();
        // First seen with no label, then a title arrives -> label fills in.
        processor.note_seen_stream("room", None);
        processor.note_seen_stream("room", Some("Room"));
        assert_eq!(
            processor.seen_streams(),
            vec![("room".to_string(), Some("Room".to_string()))]
        );
        // A later push without a title must not wipe the known label.
        processor.note_seen_stream("room", None);
        assert_eq!(
            processor.seen_streams(),
            vec![("room".to_string(), Some("Room".to_string()))]
        );
    }

    // ===========================================
    // map_stream_to_window tests - core game streams
    // ===========================================

    #[test]
    fn test_map_stream_main() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("main"), "main");
    }

    #[test]
    fn test_map_stream_room() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("room"), "room");
    }

    #[test]
    fn test_map_stream_inventory() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("inv"), "inventory");
    }

    // ===========================================
    // Redirect match tests
    // ===========================================

    #[test]
    fn test_redirect_fast_parse_ignores_empty_literals() {
        let mut config = Config::default();
        config.highlight_settings.redirect_enabled = true;
        config
            .highlights
            .insert("empty_redirect".to_string(), make_redirect_pattern("||"));

        let processor = MessageProcessor::new(config, SavedDialogPositions::default());
        let result = processor.check_redirect_match("anything");
        assert!(result.is_none());
    }

    #[test]
    fn test_redirect_fast_parse_longest_match_wins() {
        let mut config = Config::default();
        config.highlight_settings.redirect_enabled = true;
        config.highlights.insert(
            "longest_redirect".to_string(),
            make_redirect_pattern("a|ab|abc"),
        );

        let processor = MessageProcessor::new(config, SavedDialogPositions::default());
        let result = processor.check_redirect_match("zz abc zz");
        assert!(matches!(
            result,
            Some((_window, crate::config::RedirectMode::RedirectOnly, 3))
        ));
    }

    // ===========================================
    // Emoji shortcode toggle tests
    // ===========================================

    #[test]
    fn test_emoji_shortcodes_applied_when_enabled() {
        let mut processor = create_test_processor();
        assert!(processor.config.ui.emoji_shortcodes, "default must be on");
        processor.current_segments = vec![TextSegment::plain("You :grin: at 12:30:45.")];
        processor.apply_emoji_shortcodes();
        assert_eq!(
            processor.current_segments[0].text,
            "You \u{1F601} at 12:30:45."
        );
    }

    #[test]
    fn test_emoji_shortcodes_toggle_off_passthrough() {
        let mut config = Config::default();
        config.ui.emoji_shortcodes = false;
        let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
        processor.current_segments = vec![TextSegment::plain("You :grin: at :notarealcode:.")];
        processor.apply_emoji_shortcodes();
        assert_eq!(
            processor.current_segments[0].text,
            "You :grin: at :notarealcode:."
        );
    }

    // ===========================================
    // Widget data generation tests
    // ===========================================

    fn process_component(processor: &mut MessageProcessor, game_state: &mut GameState, id: &str, value: &str) {
        let mut room_components = std::collections::HashMap::new();
        let mut current_room_component = None;
        let mut room_dirty = false;
        processor.handle_component(
            id,
            value,
            game_state,
            &mut room_components,
            &mut current_room_component,
            &mut room_dirty,
        );
    }

    #[test]
    fn test_room_component_generations_bump_on_change_only() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();

        let players_v1 = "Also here: <a exist='-123' noun='Bob'>Bob</a>";
        process_component(&mut processor, &mut game_state, "room players", players_v1);
        assert_eq!(game_state.room_players_generation, 1);
        assert_eq!(game_state.room_players.len(), 1);

        // Identical re-send: previous_room_components dedup must skip processing
        process_component(&mut processor, &mut game_state, "room players", players_v1);
        assert_eq!(game_state.room_players_generation, 1, "unchanged component must not bump");

        // Real change bumps again
        process_component(
            &mut processor,
            &mut game_state,
            "room players",
            "Also here: <a exist='-456' noun='Alice'>Alice</a>",
        );
        assert_eq!(game_state.room_players_generation, 2);
    }

    /// Brief mode: a plain living player, no status.
    #[test]
    fn test_room_players_plain_living() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        process_component(
            &mut processor,
            &mut game_state,
            "room players",
            "Also here: <a exist=\"-1\" noun=\"Bob\">Bob</a>",
        );
        assert_eq!(game_state.room_players.len(), 1);
        let p = &game_state.room_players[0];
        assert_eq!(p.name, "Bob");
        assert!(!p.dead);
        assert_eq!(p.primary_status, None);
        assert_eq!(p.secondary_status, None);
    }

    /// Brief mode: parenthetical status "(sitting)".
    #[test]
    fn test_room_players_brief_paren_status() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        process_component(
            &mut processor,
            &mut game_state,
            "room players",
            "Also here: <a exist=\"-1\" noun=\"Kerl\">Kerl</a> (sitting), \
             <a exist=\"-2\" noun=\"Zoleta\">Zoleta</a>",
        );
        assert_eq!(game_state.room_players.len(), 2);
        assert_eq!(game_state.room_players[0].secondary_status.as_deref(), Some("sitting"));
        // The following player must not absorb Kerl's status.
        assert_eq!(game_state.room_players[1].secondary_status, None);
        assert_eq!(game_state.room_players[1].name, "Zoleta");
    }

    /// Verbose mode: "who is lying down" maps to the canonical "prone".
    #[test]
    fn test_room_players_verbose_lying_down_maps_to_prone() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        process_component(
            &mut processor,
            &mut game_state,
            "room players",
            "Also here: <a exist=\"-1\" noun=\"Ruuzakilr\">Ruuzakilr</a> who is lying down, \
             <a exist=\"-2\" noun=\"Torgaben\">Torgaben</a>",
        );
        assert_eq!(game_state.room_players.len(), 2);
        assert_eq!(game_state.room_players[0].secondary_status.as_deref(), Some("prone"));
        assert!(!game_state.room_players[0].dead);
        assert_eq!(game_state.room_players[1].secondary_status, None);
    }

    /// Dead marker: "the body of" sets the dead flag; name stays clean.
    #[test]
    fn test_room_players_dead_body_of() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        process_component(
            &mut processor,
            &mut game_state,
            "room players",
            "Also here: <a exist=\"-1\" noun=\"Braendon\">Braendon</a>, \
             the body of <a exist=\"-2\" noun=\"Regyy\">Regyy</a> (prone)",
        );
        assert_eq!(game_state.room_players.len(), 2);
        assert!(!game_state.room_players[0].dead);
        let regyy = &game_state.room_players[1];
        assert_eq!(regyy.name, "Regyy");
        assert!(regyy.dead, "\"the body of\" must set dead");
        assert_eq!(regyy.secondary_status.as_deref(), Some("prone"));
    }

    /// The stacked case straight from live logs: dead AND verbose posture.
    #[test]
    fn test_room_players_dead_plus_verbose() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        process_component(
            &mut processor,
            &mut game_state,
            "room players",
            "Also here: the body of <a exist=\"-1\" noun=\"Lanthilas\">Lanthilas</a> who is lying down",
        );
        assert_eq!(game_state.room_players.len(), 1);
        let p = &game_state.room_players[0];
        assert_eq!(p.name, "Lanthilas");
        assert!(p.dead);
        assert_eq!(p.secondary_status.as_deref(), Some("prone"));
    }

    /// Title prefixes ("Arena Occultist", "Lord") must NOT become a status
    /// (regression guard for the "-> [Occ]" / "-> [Lord]" bug).
    #[test]
    fn test_room_players_title_prefix_is_not_a_status() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        process_component(
            &mut processor,
            &mut game_state,
            "room players",
            "Also here: Arena Occultist <a exist=\"-1\" noun=\"Sugiin\">Sugiin</a>, \
             Lord <a exist=\"-2\" noun=\"Kazner\">Kazner</a> who is lying down",
        );
        assert_eq!(game_state.room_players.len(), 2);
        let sugiin = &game_state.room_players[0];
        assert_eq!(sugiin.name, "Sugiin");
        assert!(!sugiin.dead);
        assert_eq!(sugiin.primary_status, None, "title must not be a status");
        assert_eq!(sugiin.secondary_status, None);
        // Title + verbose posture together: title dropped, posture kept.
        let kazner = &game_state.room_players[1];
        assert_eq!(kazner.primary_status, None, "title must not be a status");
        assert_eq!(kazner.secondary_status.as_deref(), Some("prone"));
    }

    /// Legacy article-gated prepended status ("a stunned <link>") still works.
    #[test]
    fn test_room_players_article_gated_prepended_status() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        process_component(
            &mut processor,
            &mut game_state,
            "room players",
            "Also here: a stunned <a exist=\"-1\" noun=\"Bob\">Bob</a>",
        );
        assert_eq!(game_state.room_players.len(), 1);
        assert_eq!(game_state.room_players[0].primary_status.as_deref(), Some("stunned"));
    }

    /// Unknown verbose posture passes through raw (nothing silently dropped).
    #[test]
    fn test_room_players_unknown_verbose_passes_through() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        process_component(
            &mut processor,
            &mut game_state,
            "room players",
            "Also here: <a exist=\"-1\" noun=\"Bob\">Bob</a> who is floating serenely",
        );
        assert_eq!(game_state.room_players.len(), 1);
        assert_eq!(
            game_state.room_players[0].secondary_status.as_deref(),
            Some("floating serenely")
        );
    }

    #[test]
    fn test_room_objs_bumps_creature_and_object_generations() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();

        let objs = "You also see <a exist='789' noun='rock'>a rock</a>.";
        process_component(&mut processor, &mut game_state, "room objs", objs);
        assert_eq!(game_state.room_creatures_generation, 1);
        assert_eq!(game_state.room_objects_generation, 1);

        // Identical re-send: no bumps
        process_component(&mut processor, &mut game_state, "room objs", objs);
        assert_eq!(game_state.room_creatures_generation, 1);
        assert_eq!(game_state.room_objects_generation, 1);
    }

    // Flatten a styled line's segments to plaintext for readable assertions.
    fn line_text(line: &crate::data::widget::StyledLine) -> String {
        line.segments.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn test_room_desc_mirrors_styled_lines_with_links_to_game_state() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();

        // Fresh state: no prose, generation 0.
        assert!(game_state.room_description.is_empty());
        assert_eq!(game_state.room_description_generation, 0);

        // A desc component with a scenery link must be mirrored WITH its
        // styling and clickable link intact — not flattened to plaintext.
        let desc = "A mossy <a exist='1' noun='fountain'>marble fountain</a> stands here.";
        process_component(&mut processor, &mut game_state, "room desc", desc);
        assert_eq!(game_state.room_description.len(), 1);
        assert_eq!(
            line_text(&game_state.room_description[0]),
            "A mossy marble fountain stands here.",
            "prose text must be preserved"
        );
        // The clickable scenery link must survive — this is the whole point
        // of carrying styled lines rather than plaintext.
        let has_fountain_link = game_state.room_description[0]
            .segments
            .iter()
            .any(|s| s.link_data.as_ref().is_some_and(|l| l.exist_id == "1"));
        assert!(
            has_fountain_link,
            "the scenery link (exist_id=1) must survive to the phone: {:?}",
            game_state.room_description[0].segments
        );
        assert_eq!(game_state.room_description_generation, 1);
    }

    #[test]
    fn test_room_desc_bumps_only_on_change() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();

        let desc = "A quiet clearing.";
        process_component(&mut processor, &mut game_state, "room desc", desc);
        assert_eq!(game_state.room_description_generation, 1);

        // Identical re-send: the component dedup skips it — no bump.
        process_component(&mut processor, &mut game_state, "room desc", desc);
        assert_eq!(
            game_state.room_description_generation, 1,
            "unchanged room desc must not bump the generation"
        );

        // A real change bumps and replaces.
        process_component(&mut processor, &mut game_state, "room desc", "A dark cave.");
        assert_eq!(game_state.room_description_generation, 2);
        assert_eq!(game_state.room_description.len(), 1);
        assert_eq!(line_text(&game_state.room_description[0]), "A dark cave.");
    }

    #[test]
    fn test_room_desc_clears_on_empty_component() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();

        process_component(&mut processor, &mut game_state, "room desc", "A grand hall.");
        assert_eq!(game_state.room_description.len(), 1);
        assert_eq!(game_state.room_description_generation, 1);

        // An empty desc component clears the mirrored prose and bumps.
        process_component(&mut processor, &mut game_state, "room desc", "");
        assert!(
            game_state.room_description.is_empty(),
            "empty room desc component must clear the mirrored prose"
        );
        assert_eq!(game_state.room_description_generation, 2);
    }

    #[test]
    fn test_spellbook_mirrors_to_game_state_on_prompt_flush() {
        // Drive a real Spells stream + prompt the way the game sends it: each
        // <pushStream id="Spells"> line accumulates, and the prompt flushes the
        // buffer. The spellbook must then be mirrored onto GameState as styled
        // lines (keeping spell coloring/links) for remote clients.
        let mut parser = crate::parser::XmlParser::new();
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::default();

        let lines = [
            "<pushStream id='Spells'/>Elemental Defense III (503)   00:14:59",
            "<popStream/>",
            "<pushStream id='Spells'/>Mana Leech (516)   00:29:42",
            "<popStream/>",
            "<prompt time='1700000000'>&gt;</prompt>",
        ];
        for line in &lines {
            for element in parser.parse_line(line) {
                processor.process_element(
                    &element,
                    &mut game_state,
                    &mut ui_state,
                    &mut std::collections::HashMap::new(),
                    &mut None,
                    &mut false,
                    &mut None,
                    &mut None,
                    &mut None,
                    None,
                );
            }
        }

        assert_eq!(
            game_state.spellbook.len(),
            2,
            "both spell lines must mirror onto GameState, got {:?}",
            game_state.spellbook
        );
        assert!(
            line_text(&game_state.spellbook[0]).contains("Elemental Defense III"),
            "first spell line wrong: {:?}",
            game_state.spellbook
        );
        // The mirrored line must be a real styled line (segments present),
        // not a flattened string — that's what carries spell color/links.
        assert!(
            !game_state.spellbook[0].segments.is_empty(),
            "spellbook line must carry styled segments"
        );
        assert!(
            game_state.spellbook_generation >= 1,
            "spellbook generation must bump on first population"
        );
    }

    // ===========================================
    // <crtrStatus> tests (fixtures captured from a live GST session,
    // via lich-5 PR #1425's spec suite)
    // ===========================================

    #[test]
    fn test_crtr_status_parsed_from_room_objs() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();

        process_component(
            &mut processor,
            &mut game_state,
            "room objs",
            r#"  You notice<crtrStatus exist="607736" hostile="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b>."#,
        );

        assert_eq!(game_state.room_creatures.len(), 1);
        let nymph = &game_state.room_creatures[0];
        assert_eq!(nymph.name, "sea nymph");
        let flags = nymph.flags.as_ref().expect("crtrStatus flags attached");
        assert!(flags.hostile);
        assert!(!flags.dead);
        assert!(flags.statuses.is_empty());
    }

    #[test]
    fn test_crtr_status_two_creatures_one_line() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();

        process_component(
            &mut processor,
            &mut game_state,
            "room objs",
            r#"  You notice<crtrStatus exist="607744" hostile="1"/><b> <pushBold/>a <a exist="607744" noun="worm">carrion worm</a><popBold/></b> and<crtrStatus exist="607736" hostile="1" stunned="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b> (stunned)."#,
        );

        assert_eq!(game_state.room_creatures.len(), 2);
        let worm = &game_state.room_creatures[0];
        let nymph = &game_state.room_creatures[1];
        assert_eq!(worm.id, "#607744");
        assert!(worm.flags.as_ref().unwrap().statuses.is_empty());
        assert_eq!(
            nymph.flags.as_ref().unwrap().statuses,
            vec!["stunned".to_string()]
        );
        assert_eq!(nymph.display_statuses(), vec!["stunned".to_string()]);
    }

    #[test]
    fn test_crtr_status_full_snapshot_reconciles() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();

        process_component(
            &mut processor,
            &mut game_state,
            "room objs",
            r#"  You notice<crtrStatus exist="607736" hostile="1" stunned="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b> (stunned)."#,
        );
        // Dead snapshot: stunned absent means inactive, not unknown
        process_component(
            &mut processor,
            &mut game_state,
            "room objs",
            r#"  You notice<crtrStatus exist="607736" hostile="1" dead="1" prone="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b> (dead)."#,
        );

        let nymph = &game_state.room_creatures[0];
        let flags = nymph.flags.as_ref().unwrap();
        assert!(flags.dead);
        assert_eq!(flags.statuses, vec!["prone".to_string()]);
        assert!(nymph.is_dead());
        // Display leads with dead, then transient statuses
        assert_eq!(
            nymph.display_statuses(),
            vec!["dead".to_string(), "prone".to_string()]
        );
    }

    #[test]
    fn test_crtr_status_flag_zero_means_inactive() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();

        process_component(
            &mut processor,
            &mut game_state,
            "room objs",
            r#"  You notice<crtrStatus exist="999001" hostile="0"/><b> <pushBold/>a <a exist="999001" noun="rabbit">field rabbit</a><popBold/></b>."#,
        );

        let rabbit = &game_state.room_creatures[0];
        let flags = rabbit.flags.as_ref().expect("flags attached even when all inactive");
        assert!(!flags.hostile);
    }

    #[test]
    fn test_crtr_status_maps_immobile_to_immobilized() {
        let flags = crate::core::state::CreatureFlags::from_xml_attrs([
            ("immobile", "1"),
            ("AscensionBoss", "1"),
            ("challenging", "0"),
        ]);
        assert_eq!(flags.statuses, vec!["immobilized".to_string()]);
        assert!(flags.ascension_boss);
        assert!(flags.is_boss());
        assert!(!flags.challenging);
    }

    #[test]
    fn test_crtr_status_standalone_element_updates_existing_creature() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::new();

        process_component(
            &mut processor,
            &mut game_state,
            "room objs",
            r#"  You notice<crtrStatus exist="607736" hostile="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b>."#,
        );
        let generation = game_state.room_creatures_generation;

        // Standalone update (outside a component): patches the known creature
        let element = ParsedElement::CreatureStatus {
            id: "607736".to_string(),
            attrs: vec![
                ("hostile".to_string(), "1".to_string()),
                ("stunned".to_string(), "1".to_string()),
            ],
        };
        processor.process_element(
            &element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );

        let nymph = &game_state.room_creatures[0];
        assert_eq!(
            nymph.flags.as_ref().unwrap().statuses,
            vec!["stunned".to_string()]
        );
        assert_eq!(game_state.room_creatures_generation, generation + 1);

        // Unknown id: no-op, no generation bump
        let element = ParsedElement::CreatureStatus {
            id: "111111".to_string(),
            attrs: vec![("stunned".to_string(), "1".to_string())],
        };
        processor.process_element(
            &element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
        assert_eq!(game_state.room_creatures_generation, generation + 1);
    }

    // ===========================================
    // Stream subscriber index tests
    // ===========================================

    #[test]
    fn test_stream_subscribers_case_insensitive_and_trimmed() {
        let mut processor = create_test_processor();
        let mut ui_state = UiState::new();
        let mut ws = crate::data::window::WindowState::new_text("thoughts", 100);
        if let WindowContent::Text(ref mut c) = ws.content {
            // Config may carry stray whitespace and any casing
            c.streams = vec![" Thoughts ".to_string()];
        }
        ui_state.windows.insert("thoughts".to_string(), ws);
        processor.update_text_stream_subscribers(&ui_state);

        // Lookups match regardless of case/whitespace
        assert_eq!(processor.get_stream_subscribers("thoughts").len(), 1);
        assert_eq!(processor.get_stream_subscribers("THOUGHTS").len(), 1);
        assert_eq!(processor.get_stream_subscribers(" Thoughts ").len(), 1);
        assert!(processor.stream_has_subscribers("tHoUgHtS"));
        assert!(!processor.stream_has_subscribers("speech"));
    }

    #[test]
    fn test_stream_subscribers_dedupe_window() {
        let mut processor = create_test_processor();
        let mut ui_state = UiState::new();
        let mut ws = crate::data::window::WindowState::new_text("combat", 100);
        if let WindowContent::Text(ref mut c) = ws.content {
            // Duplicate stream entries must not double-deliver lines
            c.streams = vec!["combat".to_string(), "Combat".to_string()];
        }
        ui_state.windows.insert("combat".to_string(), ws);
        processor.update_text_stream_subscribers(&ui_state);

        assert_eq!(processor.get_stream_subscribers("combat").len(), 1);
    }

    #[test]
    fn test_event_pattern_feeds_stun_countdown() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::new();
        let mut ws = crate::data::window::WindowState::new_text("stuntime", 10);
        ws.content = WindowContent::Countdown(crate::data::CountdownData {
            end_time: 0,
            label: "Stun".to_string(),
            countdown_id: "stuntime".to_string(),
            color: None,
            show_when_zero: false,
        });
        ui_state.windows.insert("stuntime".to_string(), ws);

        let end_time_of = |ui_state: &UiState| match &ui_state
            .windows
            .get("stuntime")
            .expect("stuntime window")
            .content
        {
            WindowContent::Countdown(cd) => cd.end_time,
            _ => panic!("not a countdown"),
        };

        // Set: end_time lands ~duration seconds from now (server offset 0).
        let set = ParsedElement::Event {
            event_type: "stun".to_string(),
            action: crate::config::EventAction::Set,
            duration: 15,
        };
        processor.process_element(
            &set,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
        let end = end_time_of(&ui_state);
        let now = chrono::Utc::now().timestamp();
        assert!(
            (now + 13..=now + 17).contains(&end),
            "end_time {} not ~now+15",
            end
        );

        // Clear: recovery patterns zero the countdown.
        let clear = ParsedElement::Event {
            event_type: "stun".to_string(),
            action: crate::config::EventAction::Clear,
            duration: 0,
        };
        processor.process_element(
            &clear,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
        assert_eq!(end_time_of(&ui_state), 0);
    }

    #[test]
    fn test_vellum_timer_feeds_countdown_by_id() {
        let mut processor = create_test_processor();
        let mut game_state = GameState::new();
        let mut ui_state = UiState::new();
        let mut ws = crate::data::window::WindowState::new_text("cataclysm", 10);
        ws.content = WindowContent::Countdown(crate::data::CountdownData {
            end_time: 0,
            label: "Cataclysm".to_string(),
            countdown_id: "dark-cataclyst".to_string(),
            color: None,
            show_when_zero: false,
        });
        ui_state.windows.insert("cataclysm".to_string(), ws);

        let end_time_of = |ui_state: &UiState| match &ui_state
            .windows
            .get("cataclysm")
            .expect("countdown window")
            .content
        {
            WindowContent::Countdown(cd) => cd.end_time,
            _ => panic!("not a countdown"),
        };

        let mut process = |processor: &mut MessageProcessor,
                           game_state: &mut GameState,
                           ui_state: &mut UiState,
                           value: i64| {
            let element = ParsedElement::VellumTimer {
                id: "dark-cataclyst".to_string(),
                value,
            };
            processor.process_element(
                &element,
                game_state,
                ui_state,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        };

        process(&mut processor, &mut game_state, &mut ui_state, 1_764_904_999);
        assert_eq!(end_time_of(&ui_state), 1_764_904_999);

        // 0 clears; negative values clamp to cleared instead of going weird.
        process(&mut processor, &mut game_state, &mut ui_state, 0);
        assert_eq!(end_time_of(&ui_state), 0);
        process(&mut processor, &mut game_state, &mut ui_state, -5);
        assert_eq!(end_time_of(&ui_state), 0);
    }

    fn make_text_window(name: &str, streams: &[&str]) -> crate::data::window::WindowState {
        let mut ws = crate::data::window::WindowState::new_text(name, 100);
        if let WindowContent::Text(ref mut c) = ws.content {
            c.streams = streams.iter().map(|s| s.to_string()).collect();
        }
        ws
    }

    fn push_test_segment(processor: &mut MessageProcessor, text: &str) {
        processor.current_segments.push(TextSegment {
            text: text.to_string(),
            fg: None,
            bg: None,
            bold: false,
            mono: false,
            span_type: SpanType::Normal,
            link_data: None,
            custom_emoji: None,
        });
    }

    fn text_line_count(ui_state: &UiState, window: &str) -> usize {
        match &ui_state.windows.get(window).expect("window exists").content {
            WindowContent::Text(c) => c.lines.len(),
            _ => panic!("not a text window"),
        }
    }

    #[test]
    fn test_multi_subscriber_delivery() {
        // Two windows subscribe the same stream: both must receive the line
        // (the last subscriber receives it by move, the rest by clone)
        let mut processor = create_test_processor();
        let mut ui_state = UiState::new();
        ui_state
            .windows
            .insert("alpha".to_string(), make_text_window("alpha", &["thoughts"]));
        ui_state
            .windows
            .insert("beta".to_string(), make_text_window("beta", &["thoughts"]));
        processor.update_text_stream_subscribers(&ui_state);

        processor.current_stream = "thoughts".to_string();
        push_test_segment(&mut processor, "You hear the faint thoughts of someone.");
        processor.flush_current_stream(&mut ui_state);

        assert_eq!(text_line_count(&ui_state, "alpha"), 1);
        assert_eq!(text_line_count(&ui_state, "beta"), 1);
    }

    #[test]
    fn test_redirect_copy_delivers_to_target_and_original() {
        let mut config = Config::default();
        config.highlight_settings.redirect_enabled = true;
        let mut r = make_redirect_pattern("hear");
        r.redirect_to = Some("alerts".to_string());
        r.redirect_mode = crate::config::RedirectMode::RedirectCopy;
        config.highlights.insert("copy_redirect".to_string(), r);

        let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
        let mut ui_state = UiState::new();
        ui_state
            .windows
            .insert("main".to_string(), make_text_window("main", &["main"]));
        ui_state
            .windows
            .insert("alerts".to_string(), make_text_window("alerts", &["alerts"]));
        processor.update_text_stream_subscribers(&ui_state);

        processor.current_stream = "main".to_string();
        push_test_segment(&mut processor, "You hear a noise.");
        processor.flush_current_stream(&mut ui_state);

        // RedirectCopy must deliver to the redirect target AND the original
        assert_eq!(text_line_count(&ui_state, "alerts"), 1);
        assert_eq!(text_line_count(&ui_state, "main"), 1);
    }

    #[test]
    fn test_redirect_to_special_stream_restores_current_stream() {
        // A redirect whose target hits an early-return path (room/inv/
        // percWindow) must still restore current_stream, or the override
        // leaks into every following line of the chunk
        let mut config = Config::default();
        config.highlight_settings.redirect_enabled = true;
        let mut r = make_redirect_pattern("hear");
        r.redirect_to = Some("room".to_string());
        r.redirect_mode = crate::config::RedirectMode::RedirectOnly;
        config.highlights.insert("room_redirect".to_string(), r);

        let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
        let mut ui_state = UiState::new();
        ui_state
            .windows
            .insert("main".to_string(), make_text_window("main", &["main"]));
        processor.update_text_stream_subscribers(&ui_state);

        processor.current_stream = "main".to_string();
        push_test_segment(&mut processor, "You hear a noise.");
        processor.flush_current_stream(&mut ui_state);
        assert_eq!(processor.current_stream, "main");

        // The next (non-matching) line must land in main, not the target
        push_test_segment(&mut processor, "A rat scurries past.");
        processor.flush_current_stream(&mut ui_state);
        assert_eq!(text_line_count(&ui_state, "main"), 1);
    }

    fn make_hand_window(name: &str) -> crate::data::window::WindowState {
        let mut ws = crate::data::window::WindowState::new_text(name, 10);
        ws.widget_type = crate::data::window::WidgetType::Hand;
        ws.content = WindowContent::Hand {
            item: None,
            link: None,
        };
        ws
    }

    fn process_hand_element(
        processor: &mut MessageProcessor,
        game_state: &mut crate::core::state::GameState,
        ui_state: &mut UiState,
        element: &ParsedElement,
    ) {
        processor.process_element(
            element,
            game_state,
            ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    #[test]
    fn test_bare_hand_refresh_keeps_link_for_unchanged_item() {
        let mut processor =
            MessageProcessor::new(Config::default(), SavedDialogPositions::default());
        let mut ui_state = UiState::new();
        let mut game_state = crate::core::state::GameState::new();
        ui_state
            .windows
            .insert("right".to_string(), make_hand_window("right"));
        ui_state.rebuild_widget_index();

        let link = crate::data::LinkData {
            exist_id: "123".to_string(),
            noun: "shard".to_string(),
            text: "jagged nephrite shard".to_string(),
            coord: None,
        };
        process_hand_element(
            &mut processor,
            &mut game_state,
            &mut ui_state,
            &ParsedElement::RightHand {
                item: "jagged nephrite shard".to_string(),
                link: Some(link),
            },
        );

        // A refresh repeating the same item without exist/noun must keep
        // the live link.
        process_hand_element(
            &mut processor,
            &mut game_state,
            &mut ui_state,
            &ParsedElement::RightHand {
                item: "jagged nephrite shard".to_string(),
                link: None,
            },
        );
        match &ui_state.windows.get("right").unwrap().content {
            WindowContent::Hand { item, link } => {
                assert_eq!(item.as_deref(), Some("jagged nephrite shard"));
                assert_eq!(
                    link.as_ref().map(|l| l.exist_id.as_str()),
                    Some("123"),
                    "bare refresh must not clobber the link"
                );
            }
            _ => panic!("not a hand window"),
        }

        // A different item without a link must clear the stale link.
        process_hand_element(
            &mut processor,
            &mut game_state,
            &mut ui_state,
            &ParsedElement::RightHand {
                item: "a wooden club".to_string(),
                link: None,
            },
        );
        match &ui_state.windows.get("right").unwrap().content {
            WindowContent::Hand { link, .. } => {
                assert!(link.is_none(), "stale link must not follow a new item");
            }
            _ => panic!("not a hand window"),
        }

        // Emptying the hand clears both.
        process_hand_element(
            &mut processor,
            &mut game_state,
            &mut ui_state,
            &ParsedElement::RightHand {
                item: String::new(),
                link: None,
            },
        );
        match &ui_state.windows.get("right").unwrap().content {
            WindowContent::Hand { item, link } => {
                assert!(item.is_none());
                assert!(link.is_none());
            }
            _ => panic!("not a hand window"),
        }
    }

    #[test]
    fn test_redirect_longest_match_wins_across_patterns() {
        let mut config = Config::default();
        config.highlight_settings.redirect_enabled = true;
        let mut short = make_redirect_pattern("hits");
        short.redirect_to = Some("short_win".to_string());
        config.highlights.insert("short".to_string(), short);
        let mut long = make_redirect_pattern("hits you");
        long.redirect_to = Some("long_win".to_string());
        config.highlights.insert("long".to_string(), long);

        let processor = MessageProcessor::new(config, SavedDialogPositions::default());
        let result = processor.check_redirect_match("The troll hits you hard!");
        let (window, _, len) = result.expect("should match");
        assert_eq!(window, "long_win");
        assert_eq!(len, 8);
    }

    #[test]
    fn test_redirect_regex_beats_shorter_literal() {
        let mut config = Config::default();
        config.highlight_settings.redirect_enabled = true;
        let mut lit = make_redirect_pattern("troll");
        lit.redirect_to = Some("lit_win".to_string());
        config.highlights.insert("lit".to_string(), lit);
        let mut rx = make_redirect_pattern(r"troll \w+ you");
        rx.fast_parse = false;
        rx.redirect_to = Some("rx_win".to_string());
        rx.compiled_regex = regex::Regex::new(r"troll \w+ you").ok();
        config.highlights.insert("rx".to_string(), rx);

        let processor = MessageProcessor::new(config, SavedDialogPositions::default());
        let result = processor.check_redirect_match("The troll hits you hard!");
        let (window, _, len) = result.expect("should match");
        assert_eq!(window, "rx_win");
        assert_eq!(len, "troll hits you".len());
    }

    #[test]
    fn test_map_stream_thoughts() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("thoughts"), "thoughts");
    }

    #[test]
    fn test_map_stream_speech() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("speech"), "speech");
    }

    // ===========================================
    // map_stream_to_window tests - communication streams
    // ===========================================

    #[test]
    fn test_map_stream_announcements() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("announcements"), "announcements");
    }

    #[test]
    fn test_map_stream_logons() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("logons"), "logons");
    }

    #[test]
    fn test_map_stream_death() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("death"), "death");
    }

    #[test]
    fn test_map_stream_loot() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("loot"), "loot");
    }

    // ===========================================
    // map_stream_to_window tests - misc streams
    // ===========================================

    #[test]
    fn test_map_stream_spells() {
        let processor = create_test_processor();
        // Note: case-sensitive - "Spells" not "spells"
        assert_eq!(processor.map_stream_to_window("Spells"), "spells");
    }

    #[test]
    fn test_map_stream_familiar() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("familiar"), "familiar");
    }

    #[test]
    fn test_map_stream_ambients() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("ambients"), "ambients");
    }

    #[test]
    fn test_map_stream_bounty() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("bounty"), "bounty");
    }

    // ===========================================
    // map_stream_to_window tests - unknown streams default to main
    // ===========================================

    #[test]
    fn test_map_stream_unknown_defaults_to_main() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("unknown_stream"), "main");
    }

    #[test]
    fn test_map_stream_empty_defaults_to_main() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window(""), "main");
    }

    #[test]
    fn test_map_stream_random_text_defaults_to_main() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("xyz123"), "main");
    }

    #[test]
    fn test_map_stream_case_sensitive_spells() {
        let processor = create_test_processor();
        // "spells" (lowercase) should default to main, not "spells" window
        // Only "Spells" (capital S) maps to spells window
        assert_eq!(processor.map_stream_to_window("spells"), "main");
    }

    // ===========================================
    // MessageProcessor construction tests
    // ===========================================

    #[test]
    fn test_new_processor_has_main_stream() {
        let processor = create_test_processor();
        assert_eq!(processor.current_stream, "main");
    }

    #[test]
    fn test_new_processor_segments_empty() {
        let processor = create_test_processor();
        assert!(processor.current_segments.is_empty());
    }

    #[test]
    fn test_new_processor_buffers_empty() {
        let processor = create_test_processor();
        assert!(processor.inventory_buffer.is_empty());
    }

    #[test]
    fn test_new_processor_not_discarding() {
        let processor = create_test_processor();
        assert!(!processor.discard_current_stream);
    }

    #[test]
    fn test_new_processor_server_time_offset_zero() {
        let processor = create_test_processor();
        assert_eq!(processor.server_time_offset, 0);
    }

    // ===========================================
    // clear_inventory_cache tests
    // ===========================================

    #[test]
    fn test_clear_inventory_cache() {
        let mut processor = create_test_processor();
        // Add some fake previous inventory
        processor.previous_inventory = vec![vec![TextSegment {
            text: "test item".to_string(),
            fg: None,
            bg: None,
            bold: false,
            mono: false,
            span_type: SpanType::Normal,
            link_data: None,
            custom_emoji: None,
        }]];
        assert!(!processor.previous_inventory.is_empty());

        // Clear cache
        processor.clear_inventory_cache();
        assert!(processor.previous_inventory.is_empty());
    }

    // ===========================================
    // Reserve stream buffering tests
    // ===========================================

    fn make_reserve_window(name: &str) -> crate::data::window::WindowState {
        let mut ws = crate::data::window::WindowState::new_text(name, 100);
        ws.widget_type = crate::data::window::WidgetType::Reserve;
        let mut content = crate::data::TextContent::new(name.to_string(), 100);
        content.streams = vec!["reserve".to_string()];
        ws.content = WindowContent::Reserve(content);
        ws
    }

    fn reserve_line_count(ui_state: &UiState, window: &str) -> usize {
        match &ui_state.windows.get(window).expect("window exists").content {
            WindowContent::Reserve(c) => c.lines.len(),
            _ => panic!("not a reserve window"),
        }
    }

    #[test]
    fn test_map_stream_reserve() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("reserve"), "reserve");
    }

    #[test]
    fn test_reserve_stream_buffers_then_flushes_snapshot() {
        let mut processor = create_test_processor();
        let mut ui_state = UiState::new();
        ui_state
            .windows
            .insert("reserve".to_string(), make_reserve_window("reserve"));
        processor.update_text_stream_subscribers(&ui_state);

        // Line arrives while in the reserve stream: buffered, not delivered
        processor.current_stream = "reserve".to_string();
        push_test_segment(&mut processor, "a sprig of wild lilac");
        processor.flush_current_stream(&mut ui_state);
        assert_eq!(reserve_line_count(&ui_state, "reserve"), 0);
        assert_eq!(processor.reserve_buffer.len(), 1);

        // Stream pop flushes the snapshot into the window
        processor.flush_reserve_buffer(&mut ui_state);
        assert_eq!(reserve_line_count(&ui_state, "reserve"), 1);
        assert!(processor.reserve_buffer.is_empty());
    }

    #[test]
    fn test_reserve_identical_snapshot_skips_update_changed_replaces() {
        let mut processor = create_test_processor();
        let mut ui_state = UiState::new();
        ui_state
            .windows
            .insert("reserve".to_string(), make_reserve_window("reserve"));
        processor.update_text_stream_subscribers(&ui_state);

        // First snapshot
        processor.current_stream = "reserve".to_string();
        push_test_segment(&mut processor, "a sprig of wild lilac");
        processor.flush_current_stream(&mut ui_state);
        processor.flush_reserve_buffer(&mut ui_state);
        assert_eq!(reserve_line_count(&ui_state, "reserve"), 1);

        // Identical snapshot: dedupe leaves existing content untouched
        processor.current_stream = "reserve".to_string();
        push_test_segment(&mut processor, "a sprig of wild lilac");
        processor.flush_current_stream(&mut ui_state);
        processor.flush_reserve_buffer(&mut ui_state);
        assert_eq!(reserve_line_count(&ui_state, "reserve"), 1);

        // Changed snapshot: content is replaced, not appended
        processor.current_stream = "reserve".to_string();
        push_test_segment(&mut processor, "a blue potion");
        processor.flush_current_stream(&mut ui_state);
        processor.flush_reserve_buffer(&mut ui_state);
        assert_eq!(reserve_line_count(&ui_state, "reserve"), 1);
    }

    #[test]
    fn test_reserve_stream_discarded_without_window() {
        let mut processor = create_test_processor();
        let mut ui_state = UiState::new();
        ui_state
            .windows
            .insert("main".to_string(), make_text_window("main", &["main"]));
        processor.update_text_stream_subscribers(&ui_state);

        processor.current_stream = "reserve".to_string();
        push_test_segment(&mut processor, "a sprig of wild lilac");
        processor.flush_current_stream(&mut ui_state);

        // No reserve window: content dropped, nothing buffered, nothing in main
        assert!(processor.reserve_buffer.is_empty());
        assert_eq!(text_line_count(&ui_state, "main"), 0);
    }

    // ===========================================
    // Stream mapping completeness tests
    // ===========================================

    #[test]
    fn test_all_known_streams_mapped_correctly() {
        let processor = create_test_processor();

        // Test all documented stream -> window mappings
        let expected_mappings = [
            ("main", "main"),
            ("room", "room"),
            ("inv", "inventory"),
            ("thoughts", "thoughts"),
            ("speech", "speech"),
            ("announcements", "announcements"),
            ("loot", "loot"),
            ("death", "death"),
            ("logons", "logons"),
            ("familiar", "familiar"),
            ("ambients", "ambients"),
            ("bounty", "bounty"),
            ("Spells", "spells"),
        ];

        for (stream, expected_window) in expected_mappings {
            assert_eq!(
                processor.map_stream_to_window(stream),
                expected_window,
                "Stream '{}' should map to window '{}'",
                stream,
                expected_window
            );
        }
    }
}
