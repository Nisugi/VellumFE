//! The top-level dispatch for parsed XML elements: `process_element`
//! updates game/UI state for every ParsedElement variant, plus the hand
//! and countdown helpers it leans on.

use super::*;

impl MessageProcessor {
    /// Stage a completed room component snapshot as a fallback for remote
    /// Story.  Publication waits for the prompt because Lich normally follows
    /// this projection with the same room as authoritative decorated `main`
    /// text; publishing here would show both copies.
    fn stage_remote_room_story(
        &mut self,
        game_state: &GameState,
        room_components: &std::collections::HashMap<String, Vec<Vec<TextSegment>>>,
        nav_room_id: Option<&str>,
        room_subtitle: Option<&str>,
    ) {
        if self.remote.is_none() {
            return;
        }

        let title = game_state
            .room_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .or(room_subtitle.filter(|subtitle| !subtitle.trim().is_empty()));
        let uid = nav_room_id.filter(|id| !id.trim().is_empty());
        let Some(identity) = uid
            .map(|id| format!("id:{}", id.trim()))
            .or_else(|| title.map(|title| format!("title:{}", title.trim())))
        else {
            return;
        };

        if self.last_remote_story_room.as_deref() == Some(identity.as_str()) {
            return;
        }

        const COMPONENT_ORDER: [&str; 4] = ["room desc", "room objs", "room players", "room exits"];
        if !COMPONENT_ORDER
            .iter()
            .any(|id| room_components.contains_key(*id))
        {
            return;
        }

        let title = title
            .map(str::trim)
            .map(|title| title.trim_start_matches('-').trim())
            .map(|title| title.trim_start_matches('[').trim_end_matches(']').trim())
            .filter(|title| !title.is_empty())
            .map(str::to_string);
        let mut component_lines = Vec::new();
        for component_id in COMPONENT_ORDER {
            if let Some(lines) = room_components.get(component_id) {
                component_lines.extend(lines.iter().filter_map(|segments| {
                    let text: String = segments
                        .iter()
                        .map(|segment| segment.text.as_str())
                        .collect();
                    (!segments.is_empty() && !super::room_description_is_disabled(&text)).then(|| {
                        let mut segments = segments.clone();
                        super::flush_line::style_player_title_prefixes(&mut segments);
                        crate::data::widget::StyledLine {
                            segments,
                            stream: "main".to_string(),
                            timestamp: None,
                        }
                    })
                }));
            }
        }

        self.pending_remote_room_story = Some(super::PendingRemoteRoomStory {
            identity,
            title,
            uid: uid.map(|id| id.trim().to_string()),
            component_lines,
        });
    }

    /// Complete native-room capture and publish a staged component fallback
    /// only when no matching native block arrived before the prompt.
    fn finalize_remote_room_story(&mut self, game_state: &mut GameState) {
        let native = self.native_room_capture.take();
        if let Some(new_description) = native
            .as_ref()
            .map(|capture| capture.description.clone())
            .filter(|description| !description.is_empty())
        {
            if game_state.room_description != new_description {
                game_state.room_description = new_description;
                game_state.room_description_generation += 1;
            }
        }

        let Some(pending) = self.pending_remote_room_story.take() else {
            if let Some(native) = native {
                self.last_remote_story_room = Some(self.native_capture_identity(&native));
            }
            return;
        };
        // A native block matches the staged fallback when their identities
        // agree — by uid (explicit, or the navigation uid current at this
        // prompt) or, failing that, by title. Either match means the native
        // block already told the Story about this room.
        if native.as_ref().is_some_and(|capture| {
            self.native_capture_identity(capture) == pending.identity
                || pending.title.as_deref() == Some(capture.title.as_str())
        }) {
            self.last_remote_story_room = Some(pending.identity);
            return;
        }

        if self.last_remote_story_room.as_deref() == Some(pending.identity.as_str()) {
            return;
        }

        let display_id = super::canonical_room_id(None, pending.uid.as_deref());
        let header = match (pending.title.as_deref(), display_id.as_deref()) {
            (Some(title), Some(id)) => format!("[{title} - {id}]"),
            (Some(title), None) => format!("[{title}]"),
            (None, Some(id)) => format!("[Room {id}]"),
            (None, None) => "[Room]".to_string(),
        };
        let room_name_preset = self.config.colors.presets.get("roomName");
        let header_fg = room_name_preset
            .and_then(|preset| preset.fg.as_ref())
            .map(|color| self.config.resolve_palette_color(color));
        let header_bg = room_name_preset
            .and_then(|preset| preset.bg.as_ref())
            .map(|color| self.config.resolve_palette_color(color));
        let header = crate::data::widget::StyledLine {
            segments: vec![TextSegment {
                text: header,
                fg: header_fg,
                bg: header_bg,
                bold: true,
                ..Default::default()
            }],
            stream: "main".to_string(),
            timestamp: None,
        };

        if let Some(remote) = self.remote.as_mut() {
            for line in std::iter::once(header).chain(pending.component_lines.into_iter()) {
                remote.push_text("main", std::sync::Arc::new(line));
            }
        }
        self.last_remote_story_room = Some(pending.identity);
        self.remote_chunk_has_story_text = true;
    }

    /// Parse the decorated room header used on the ordinary `main` stream:
    /// `[Title - ROOM] (uUID)` (Lich), `[Title - ROOM]` (uid display off),
    /// or `[Title]` (the game's own header / Lich with room numbers off).
    /// Returns `(title, had_room_number, explicit_uid)`. Numberless,
    /// uid-less brackets are ambiguous (chat prefixes look identical), so
    /// the caller must gate those on a matching staged room title.
    fn native_room_header_parts(text: &str) -> Option<(String, bool, Option<String>)> {
        let text = text.trim();
        let bracketed = text.strip_prefix('[')?;
        let close = bracketed.find(']')?;
        let inside = bracketed.get(..close)?;
        let (title, numbered) = match inside.rsplit_once(" - ") {
            Some((title, room_id))
                if !room_id.is_empty() && room_id.chars().all(|ch| ch.is_ascii_digit()) =>
            {
                (title, true)
            }
            _ => (inside, false),
        };
        let suffix = bracketed.get(close + 1..)?.trim();
        let uid = suffix
            .strip_prefix("(u")
            .and_then(|rest| rest.strip_suffix(')'))
            .filter(|uid| !uid.is_empty() && uid.chars().all(|ch| ch.is_ascii_digit()))
            .map(str::to_string);
        Some((title.trim().to_string(), numbered, uid))
    }

    /// The dedup identity a native capture resolves to at the prompt:
    /// explicit uid, else the navigation uid current at the prompt, else
    /// the title.
    fn native_capture_identity(&self, capture: &super::NativeRoomCapture) -> String {
        capture
            .uid
            .clone()
            .or_else(|| self.current_room_uid.map(|uid| uid.to_string()))
            .map(|uid| format!("id:{uid}"))
            .unwrap_or_else(|| format!("title:{}", capture.title))
    }

    /// Observe finalized main-stream lines so the ordinary decorated Lich
    /// room block can suppress the component fallback and refresh Room prose.
    pub(super) fn observe_native_room_line(&mut self, line: &crate::data::widget::StyledLine) {
        if self.current_stream != "main" {
            return;
        }
        let text: String = line
            .segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect();
        // Room art may precede the styled header in its own segment. Inspect
        // each segment rather than requiring the concatenated line to begin
        // with `[` so the header remains recognizable with art enabled.
        if let Some((title, uid)) = line.segments.iter().find_map(|segment| {
            let (title, numbered, uid) = Self::native_room_header_parts(&segment.text)?;
            // A bare `[Title]` is only trusted as a room header when it
            // matches the room title staged from the component projection;
            // any other bracketed text (chat prefixes etc.) is ignored.
            let recognized = numbered
                || uid.is_some()
                || self
                    .pending_remote_room_story
                    .as_ref()
                    .and_then(|pending| pending.title.as_deref())
                    .is_some_and(|pending_title| pending_title == title);
            recognized.then_some((title, uid))
        }) {
            self.native_room_capture = Some(super::NativeRoomCapture {
                title,
                uid,
                description: Vec::new(),
                capturing_description: true,
            });
            return;
        }

        let Some(capture) = self.native_room_capture.as_mut() else {
            return;
        };
        if !capture.capturing_description || text.trim().is_empty() {
            return;
        }
        let trimmed = text.trim();
        if trimmed.starts_with("You also see ")
            || trimmed.starts_with("Also here:")
            || trimmed.starts_with("Obvious exits:")
            || trimmed.starts_with("Obvious paths:")
        {
            capture.capturing_description = false;
            return;
        }
        if !super::room_description_is_disabled(trimmed) {
            let mut description = line.clone();
            description.stream = "room".to_string();
            capture.description.push(description);
        }
    }

    /// Registry entry for a held item from the `<left>`/`<right>` feed;
    /// None for an empty hand (the game sends the literal "Empty").
    pub(super) fn hand_game_item(
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
    pub(super) fn update_countdown_by_id(
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
            ParsedElement::StreamWindow {
                id,
                subtitle,
                title,
            } => {
                self.note_seen_stream(id, title.as_deref());
                // U3: record the stream as a window discovery for AppCore to
                // register as a bound, Hidden-by-default layout entry (the
                // processor can't reach the layout). Replaces the Stream
                // offer.
                ui_state
                    .pending_window_discoveries
                    .push(crate::data::WindowDiscovery {
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
                // A changed navigation UID begins a new room identity. The
                // decorated Lich room number arrives later on `main`; keeping
                // the previous room's number through this interval would
                // briefly publish a false pair such as `228 (u7121)`.
                if nav_room_id.as_deref() != Some(id.as_str()) {
                    *lich_room_id = None;
                }
                *nav_room_id = Some(id.clone());
                // Mirror onto the processor so the `sprite` component (which
                // arrives later in the same room block, and does not receive
                // nav_room_id) can look up this room's art.
                self.current_room_uid = id.parse::<u64>().ok();
                *room_window_dirty = true;
                // A <nav> tag is the universal "you moved" signal (Lich's
                // room_count increment). Push it for the walk executor even
                // if the room can't be resolved to a mapdb id — arrival
                // detection then never hangs on an unmapped room (§12).
                self.game_line_no += 1;
                game_state.nav_count += 1;
                game_state.move_feedback.push_back((
                    self.game_line_no,
                    crate::core::move_feedback::MoveFeedback::NavArrived,
                ));
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
            ParsedElement::StreamPop | ParsedElement::StreamPopForced => {
                // A forced pop means the prompt force-closed a stream whose
                // popStream was eaten upstream: per-stream buffers may be
                // truncated and must not commit as authoritative snapshots.
                let torn = matches!(element, ParsedElement::StreamPopForced);
                self.flush_current_stream_with_tts(ui_state, tts_manager.as_deref_mut());

                if self.current_stream == "room" {
                    self.stage_remote_room_story(
                        game_state,
                        room_components,
                        nav_room_id.as_deref(),
                        room_subtitle.as_deref(),
                    );
                }

                // Flush inventory buffer if we're leaving inv stream
                if self.current_stream == "inv" && torn {
                    // Truncated snapshot: keep the previous complete
                    // inventory instead of committing a torn one.
                    tracing::warn!(
                        "inv stream force-closed by prompt - discarding torn snapshot ({} lines)",
                        self.inventory_buffer.len()
                    );
                    self.inventory_buffer.clear();
                } else if self.current_stream == "inv" {
                    // Worn items into the registry from the same buffer the
                    // window uses (each line's first <a> link = one worn
                    // item; the "Your worn items are:" header and blank
                    // lines carry no link and are skipped). Runs regardless
                    // of whether an inventory window exists.
                    game_state
                        .objects
                        .set_worn_from_lines(&self.inventory_buffer);
                    self.flush_inventory_buffer(game_state, ui_state);
                }

                // Flush reserve buffer if we're leaving reserve stream
                if self.current_stream == "reserve" {
                    if torn {
                        self.reserve_buffer.clear();
                    } else {
                        self.flush_reserve_buffer(ui_state);
                    }
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
            ParsedElement::StreamResume { id } => {
                // A pop uncovered an enclosing redirect (parser stream
                // stack): re-route to it like a push, but WITHOUT the
                // arrival side effects — the enclosing stream's content is
                // mid-accumulation, so clearing room/inv/reserve buffers
                // here would destroy it.
                self.note_seen_stream(id, None);
                self.current_stream = id.clone();
                if self.stream_has_target_window(ui_state, id) {
                    self.discard_current_stream = false;
                } else {
                    self.discard_current_stream =
                        matches!(self.resolve_orphaned_stream(id), RouteDecision::Discard);
                }
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

                // At most one background viewitem probe dispatches per
                // prompt (Saga's pacing); commands ride take_outbound.
                self.inv_service
                    .on_prompt(chrono::Utc::now().timestamp_millis().max(0) as u64);

                // An INVENTORY FULL scan ends at the prompt: write the
                // collected mark/register statuses into the registry.
                if self.inv_scan.is_capturing() {
                    for (id, status) in self.inv_scan.finish() {
                        game_state.objects.set_status(id, status);
                    }
                }

                // READY/STOW list rows captured during flush feed the
                // ready/stow state now that game_state is in hand.
                for (text, link) in self.pending_ready_stow.drain(..) {
                    game_state.objects.parse_ready_stow_line(&text, link);
                }

                // Move-feedback events captured during flush queue for the
                // walk executor (drained by tick_travel).
                game_state
                    .move_feedback
                    .extend(self.pending_move_feedback.drain(..));
                // Creature-effect events captured during flush apply now
                // that game_state is in hand (starts re-arm, ends remove).
                if !self.pending_creature_effects.is_empty() {
                    let now_server = chrono::Utc::now().timestamp() + self.server_time_offset;
                    for (exist, name, severity, timeout_s) in
                        self.pending_creature_effects.drain(..).collect::<Vec<_>>()
                    {
                        game_state.apply_creature_effect_event(
                            &exist, &name, severity, timeout_s, now_server,
                        );
                    }
                }
                game_state.game_line_no = self.game_line_no;

                // Raw lines for scripted-edge awaits. A bounded ring, not a
                // queue: an await must see lines that arrived before it armed,
                // and several steps may match the same line.
                for line in self.pending_recent_lines.drain(..) {
                    game_state.push_recent_line(&line);
                }

                // Character-state lines feed the parser in order (the PROFILE
                // house parse is stateful).
                for line in self.pending_character_lines.drain(..) {
                    game_state.character.parse_line(&line);
                }
                // Day-pass lines feed the cache in order (expiry follows the
                // description, keyed by the same pass id).
                for (line, pass_id) in self.pending_day_pass_lines.drain(..) {
                    game_state.day_passes.observe(&line, pass_id.as_deref());
                }
                if let Some(silver) = self.pending_silver.take() {
                    game_state.silver = Some(silver);
                    game_state.silver_line_no = self.game_line_no;
                }

                // Group events apply in order: a `group` reply stages its
                // roster on the "You are leading/grouped with" line and
                // commits on the status sentinel, so the two must not be
                // reordered. The staging cursor is local to this drain, so a
                // reply split across prompts cannot leave the roster
                // half-applied -- it just fails to commit and stays
                // unconfirmed, which the display reports honestly.
                if !self.pending_group.is_empty() {
                    let mut roster_pending = None;
                    for (event, members) in self.pending_group.drain(..) {
                        crate::core::group::apply_event(
                            &mut game_state.group,
                            &event,
                            &members,
                            &mut roster_pending,
                        );
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

                // Lich sends its native decorated room block after the room
                // component projection.  Resolve the two at their shared
                // prompt boundary: native wins; component-only connections
                // receive the staged fallback exactly once.
                self.finalize_remote_room_story(game_state);

                // Decide whether to show this prompt based on chunk tracking
                // Skip if: no main text was received since last prompt AND prompt text is unchanged
                // This handles both "silent updates only" and "empty chunk" cases
                // But we always show the prompt if it changed (e.g., "R>" -> ">" when roundtime ends)
                let prompt_changed = text.trim() != game_state.last_prompt.trim();
                let should_skip = !self.chunk_has_main_text && !prompt_changed;
                // Remote clients gate the separator on THEIR story feed's
                // activity: stream text that fell back into the local main
                // window (headless layout without thoughts/arrivals windows)
                // arms chunk_has_main_text, but the phone routed those lines
                // to their own feeds — pushing this prompt would strand a
                // lone separator in the phone's story per background line.
                let show_remote = self.remote_chunk_has_story_text || prompt_changed;

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
                            inline_image: None,
                        });
                    }

                    // Finish prompt line. The remote tap is suppressed when
                    // the phone's story feed saw nothing this chunk (the
                    // local main window still shows the separator — the
                    // fallback text landed there).
                    self.suppress_remote_tap = !show_remote;
                    self.flush_current_stream_with_tts(ui_state, tts_manager);
                    self.suppress_remote_tap = false;
                }

                // Echo the prompt into the familiar window as a separator
                // (arena-spectate parity with Wrayth's main view). Fires only
                // when familiar text arrived since the last prompt, and
                // independently of the main window's prompt dedupe above.
                // Without a familiar window the spectate text fell back into
                // main, whose own prompt logic above already covers it —
                // echoing too would double the separator there.
                if self.chunk_has_familiar_text {
                    // (Prompt lines a redirect script moved into the stream
                    // as plain text are dropped at flush time — this echo is
                    // the single styled separator.)
                    if !text.trim().is_empty()
                        && self.stream_has_target_window(ui_state, "familiar")
                    {
                        let original_stream =
                            std::mem::replace(&mut self.current_stream, "familiar".to_string());
                        for ch in text.chars() {
                            let color = self
                                .prompt_color_map
                                .get(&ch)
                                .cloned()
                                .unwrap_or_else(|| "#808080".to_string());
                            self.current_segments.push(TextSegment {
                                text: ch.to_string(),
                                fg: Some(color),
                                bg: None,
                                bold: false,
                                mono: false,
                                span_type: SpanType::Normal,
                                link_data: None,
                                custom_emoji: None,
                                inline_image: None,
                            });
                        }
                        // A bare separator: no TTS. The flag exempts this
                        // internally-built line from the moved-prompt strip
                        // (it is itself prompt-shaped).
                        self.emitting_familiar_separator = true;
                        self.flush_current_stream_with_tts(ui_state, None);
                        self.emitting_familiar_separator = false;
                        self.current_stream = original_stream;
                    }
                    // Reset AFTER the echo: the echoed line flows through the
                    // same flush tracking and would re-arm the flag, echoing
                    // a stray separator on the next idle prompt.
                    self.chunk_has_familiar_text = false;
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
                    // (through the setter so the local receipt stamp that
                    // keeps RT flowing between lines is taken too).
                    game_state.update_game_time(server_time);
                }

                // Reset chunk tracking for next prompt
                self.chunk_has_main_text = false;
                self.remote_chunk_has_story_text = false;
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
                let effective_stream =
                    if !stream.is_empty() && stream.as_str() != self.current_stream.as_str() {
                        tracing::debug!(
                            "Inline stream tag: switching from '{}' to '{}' for this text element",
                            self.current_stream,
                            stream
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
                        inline_image: None,
                    };

                    // Accumulate this segment in the current line buffer
                    // It will be flushed to spells_buffer when we see </stream>
                    self.spells_line_buffer.push(segment);
                    tracing::trace!(
                        "Accumulated Spells segment: '{}'",
                        if content.len() > 50 {
                            format!("{}...", &content[..50])
                        } else {
                            content.to_string()
                        }
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
                    inline_image: None,
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
            ParsedElement::AimTime { value } => {
                // Aimed-shot timer (AimTimerDialog): absolute server end
                // time like casttime; 0 (dialog closed) clears.
                self.update_countdown_by_id(ui_state, "aimtime", *value as i64);
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
                    tracing::warn!("vellumCmd rejected (only dot-commands are allowed): {command}");
                }
            }
            ParsedElement::RoomPicture { id } => {
                // The game says "this room has picture N"; the wire carries
                // only the number. Resolution order:
                //   1. the user's own pool (images/inline/<id>.png) — always
                //      wins, so installed art overrides the download
                //   2. GemStone's art, downloaded from play.net, but ONLY
                //      when the user opted in
                // Unknown ids and the near-universal 0 clear the slot, so a
                // room without a picture never shows the previous room's.
                let mut art = (*id != 0)
                    .then(|| id.to_string())
                    .filter(|name| crate::core::inline_image::contains(name));

                if art.is_none() && *id != 0 && self.config.game_art.enabled {
                    let picture = *id;
                    let downloaded = crate::core::game_art::pool_name(picture);
                    if crate::core::inline_image::contains(&downloaded) {
                        art = Some(downloaded);
                    } else if crate::core::game_art::claim_fetch(picture) {
                        // Off the feed thread: a room render must never wait
                        // on the network. The picture appears on the next
                        // visit (or the next room change) once cached.
                        std::thread::spawn(move || {
                            match crate::core::game_art::fetch_blocking(picture) {
                                Ok(_) => {}
                                Err(err @ crate::core::game_art::FetchError::Missing(_)) => {
                                    // The server said this id has no art — remember
                                    // it so the id is not requested again.
                                    tracing::debug!("game art {picture}: {}", err.reason());
                                    crate::core::game_art::mark_missing(picture);
                                }
                                Err(err) => {
                                    // Transient (network/disk) — claim_fetch already
                                    // stops retries this session; next session tries
                                    // again. Recording it would kill the art forever.
                                    tracing::debug!(
                                        "game art {picture} (will retry next session): {}",
                                        err.reason()
                                    );
                                }
                            }
                        });
                    }
                }
                // Emit the picture into the STORY line, which is where
                // Wrayth shows it: floated left with the room name and
                // description wrapping beside it. `<resource>` arrives just
                // before the room name, so pushing a segment here puts the
                // image at the head of that line — the same float the
                // <vellumImg> path produces.
                if let Some(name) = &art {
                    self.current_segments.push(TextSegment {
                        text: format!("[img:{name}]"),
                        inline_image: Some(crate::data::InlineImage {
                            name: name.clone(),
                            rows: crate::config::room_images::DEFAULT_ROOM_IMAGE_ROWS,
                            align: crate::data::FloatAlign::Left,
                        }),
                        ..Default::default()
                    });
                }
                if game_state.story_picture != art {
                    game_state.story_picture = art;
                    *room_window_dirty = true;
                }
            }
            ParsedElement::VellumImage { src, rows, align } => {
                // Script-facing inline image. The segment keeps a readable
                // `[img:name]` fallback in `text` so the TUI (and any
                // frontend that can't resolve the art) shows something
                // rather than a blank, exactly like custom emoji.
                self.current_segments.push(TextSegment {
                    text: format!("[img:{src}]"),
                    inline_image: Some(crate::data::InlineImage {
                        name: src.clone(),
                        rows: *rows,
                        align: *align,
                    }),
                    ..Default::default()
                });
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

                let level = injury_name_to_level(id, name);

                // Game state owns injuries (remote clients and windows added
                // mid-session read from here); widget copy below.
                if level == 0 {
                    game_state.injuries.remove(id);
                } else {
                    game_state.injuries.insert(id.clone(), level);
                }

                // Update EVERY injury doll window — per-window doll sets
                // mean several dolls can render the same wound data, and the
                // old singleton lookup left all but the first one stale.
                for window in ui_state.windows.values_mut() {
                    if let WindowContent::InjuryDoll(ref mut injury_data) = window.content {
                        injury_data.set_injury(id.clone(), level);
                    }
                }
                tracing::debug!("Updated injury: {} to level {} ({})", id, level, name);
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
                        game_state
                            .minivitals
                            .update_vital(id, *value, *max, text.clone());
                    }
                    _ => {}
                }

                // Update GS4 experience state for expr dialog elements
                // (the exact-exp attributes on the mindState bar arrive as a
                // separate MindStateExp element right after this one)
                match id.as_str() {
                    "mindState" => {
                        game_state
                            .gs4_experience
                            .update_mind_state(*value, text.clone());
                    }
                    "nextLvlPB" => {
                        game_state
                            .gs4_experience
                            .update_next_level(*value, text.clone());
                    }
                    "encumlevel" => {
                        game_state.encumbrance.update_level(*value, text.clone());
                    }
                    // The stance bar renders into a window widget above, but
                    // it also belongs in game state: headless and remote
                    // clients have no stance window to read it from.
                    "pbarStance" => {
                        game_state.stance.update(*value, text);
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
                // Training points + conversion rates ride the same dialog.
                game_state.gs4_experience.update_tp_label(id, value);
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

                // Store every indicator the game sends, whatever its id.
                // `set` normalizes case and the "Icon" prefix, so the parser's
                // casing does not matter here. Previously this was a fixed
                // match that silently dropped JOINED, POISONED, DISEASED and
                // anything new Simu added.
                game_state.status.set(id, *active);

                // JOINED going off is the one authoritative "you are in no
                // group" signal the feed gives us -- more reliable than
                // waiting for a leave message that may never arrive (death,
                // linkdeath). Clear the roster on the falling edge.
                if id.eq_ignore_ascii_case("joined") {
                    if *active {
                        // We are in a group but the roster is not known --
                        // being ADDED by someone else produces this
                        // indicator with no message naming the members. Mark
                        // it unconfirmed so the display says so, and so a
                        // watcher can tell "grouped, roster pending" from
                        // "not grouped".
                        if !game_state.group.is_grouped() {
                            game_state.group.mark_joined_unconfirmed();
                        }
                    } else {
                        game_state.group.clear();
                    }
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
                            // Auto-discover a new id ONLY if no indicator
                            // template already claims it via a condition state
                            // (a combined indicator owns it; adding the raw id
                            // too would double it up as an orphan cell).
                            if !found
                                && !self
                                    .claimed_indicator_ids
                                    .contains(&id.to_ascii_uppercase())
                            {
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
            ParsedElement::DialogOpen {
                id,
                title,
                save,
                location,
            } => {
                self.chunk_has_silent_updates = true;
                tracing::debug!(
                    "DialogOpen received: id={}, title={:?}, save={}",
                    id,
                    title,
                    save
                );

                // U3: dialogs reaching here are non-resident (resident ones
                // are mined into panels). Hidden-until-shown: a dialog the
                // user never showed doesn't pop up, but the store still
                // ingests its data so the window can be shown later.
                // EXCEPTION: the detach/save='false' utility-popup class
                // (bugDialogBox, alert boxes) is a direct response to the
                // user's own command — it pops without opt-in (live-test
                // report: bug dialogs never appeared for anyone).
                let utility_popup = location.as_deref() == Some("detach") && !save;
                if !utility_popup && !Self::dialog_should_popup(ui_state, id) {
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

                // A dialog id claimed by a dedicated catalog view becomes a
                // layout widget instead of a popup (redesign Phase 4:
                // claims_dialog is the single must-agree guard). Queue the
                // DIALOG ID (not the view key) so
                // process_pending_window_additions can tag the created
                // window with its binding — the U2 identity that ties the
                // feed to the placed window regardless of display name.
                if crate::core::local_catalog::claims_dialog(id) {
                    tracing::debug!("DialogOpen redirected to claimed widget: id={}", id);
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
            ParsedElement::DialogDropDowns {
                id,
                clear,
                dropdowns,
            } => {
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
                // Resident dialogs claimed by a dedicated view
                // (Buffs/Debuffs/Cooldowns/injuries/encum/expr/stance/...)
                // are mined into those widgets — don't offer them as generic
                // dialog panels too. Same single guard as the DialogOpen
                // redirect (redesign Phase 4), so the two paths can never
                // disagree.
                if crate::core::local_catalog::claims_dialog(id) {
                    return;
                }
                // U3: record the resident dialog as a DialogPanel discovery
                // for AppCore to register as a bound, Hidden-by-default
                // dockable-panel layout entry. Replaces the resident Dialog
                // offer. Seed the store title so the panel renders when shown.
                ui_state
                    .pending_window_discoveries
                    .push(crate::data::WindowDiscovery {
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
                            justify: label.justify,
                        };

                        if is_paired {
                            paired_labels.push(dialog_label);
                        } else {
                            standalone_labels.push(dialog_label);
                        }
                    }

                    // Paired labels belong to an input dialog that re-sends its
                    // full set, so replace them. But standalone (panel) labels
                    // arrive in PARTIAL updates — a resident panel re-sends only
                    // the rows that changed (UberBar's update frame carries a
                    // handful of values, not the whole grid). Merge those by id
                    // so the label column and untouched values survive, instead
                    // of being wiped to just the few in this frame.
                    if !paired_labels.is_empty() {
                        dialog.labels = paired_labels;
                    }
                    for label in standalone_labels {
                        match dialog.display_labels.iter_mut().find(|l| l.id == label.id) {
                            // Preserve a prior layout if this partial update
                            // omitted it (updates often carry value-only).
                            Some(slot) => {
                                slot.value = label.value;
                                if label.layout.is_some() {
                                    slot.layout = label.layout;
                                }
                                if label.justify.is_some() {
                                    slot.justify = label.justify;
                                }
                            }
                            None => dialog.display_labels.push(label),
                        }
                    }
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

                let fallback_focus = dialog
                    .focused_field
                    .filter(|idx| *idx < dialog.fields.len());
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

                // Stance arrives by either route depending on how the server
                // frames the dialog, so mirror it into game state from both.
                // Everything else here is dialog-slot rendering only.
                for pb in progress_bars {
                    if pb.id == "pbarStance" {
                        game_state.stance.update(pb.value, &pb.text);
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
                                        inline_image: None,
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
                                            inline_image: None,
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
                // Redesign Phase 4d: a window this session SHOWED via an
                // expose verb is dismissed by the matching closeDialog
                // (bank sends one on leaving ×3,911). Queued — hiding a
                // layout window needs the layout-capable tick. The game
                // also closes never-opened ids defensively
                // (withdraw/deposit); the drain no-ops those.
                if ui_state.expose_shown_ids.contains(id) {
                    ui_state.pending_expose_closes.push(id.clone());
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
                let expires_at = crate::data::parse_time_seconds(time).map(|secs| time_base + secs);

                // Remember the feed's display name so the missing-spells
                // window can label effects the static table doesn't know
                // even after they drop.
                if matches!(category.as_str(), "ActiveSpells" | "Buffs") {
                    if let Ok(number) = id.parse::<u16>() {
                        game_state
                            .spell_names_seen
                            .entry(number)
                            .or_insert_with(|| text.clone());
                    }
                }
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
            ParsedElement::ObjectivesUpdate { action, entries } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                let store = &mut game_state.objectives;
                let changed = if action == "full-refresh" {
                    if store.objectives != *entries {
                        store.objectives = entries.clone();
                        true
                    } else {
                        false // Feed re-sends the full list on login; don't churn
                    }
                } else {
                    // Unknown incremental action: upsert by id so a future
                    // partial-update variant degrades gracefully.
                    let mut any = false;
                    for entry in entries {
                        match store.objectives.iter_mut().find(|o| o.id == entry.id) {
                            Some(slot) if *slot != *entry => {
                                *slot = entry.clone();
                                any = true;
                            }
                            Some(_) => {}
                            None => {
                                store.objectives.push(entry.clone());
                                any = true;
                            }
                        }
                    }
                    any
                };
                if changed {
                    store.generation += 1;
                }
            }
            ParsedElement::TargetList {
                current_target,
                target_ids, // Store IDs to filter room_creatures
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
            ParsedElement::ContainerItem {
                container_id,
                content,
            } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Parse the raw <inv> line into a structured GameItem,
                // skipping the container's own header line.
                if let Some(container) = game_state.objects.container(container_id) {
                    let target = container.command_target();
                    if !crate::core::game_objects::parse::is_header_line(content, &target) {
                        if let Some(item) = crate::core::game_objects::parse_anchor(content) {
                            game_state.objects.add_container_item(container_id, item);
                        }
                    }
                } else if let Some(item) = crate::core::game_objects::parse_anchor(content) {
                    // Item arrived before the <container> tag; register it
                    // (auto-creates a title-less entry, same as the cache).
                    // Header lines have their own anchor id == container id,
                    // but with no container known yet we can't dedup that;
                    // the header's noun is the container itself, harmless to
                    // include and corrected on the next clear+refill.
                    game_state.objects.add_container_item(container_id, item);
                }

                tracing::trace!(
                    "Added item to container '{}': {}",
                    container_id,
                    if content.len() > 50 {
                        format!("{}...", &content[..50])
                    } else {
                        content.clone()
                    }
                );
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
                // Routed by AppCore each frame: the native skill trainer
                // claims it when armed (user just sent GOALS); anything else
                // opens in the system browser as before.
                self.chunk_has_silent_updates = true;
                tracing::info!("LaunchURL received: {}", url);
                self.pending_launch_url = Some(url.clone());
            }
            ParsedElement::WindowHints { id, attrs } => {
                // Always-ingest, like the dialog store: the latest
                // declaration's placement attrs win, available whenever
                // the window materializes (redesign Phase 3e).
                ui_state.window_hints.insert(id.clone(), attrs.clone());
                // A dialog's declared width/height feeds the anchor grid's
                // vertical compass (bank: openDialog height='130' — the
                // e/w rows center against it, align='s' bottoms against
                // it). EXISTING slots only: hints also fire for stream/
                // container ids, which must not conjure phantom dialog
                // slots. Ordering holds because the openDialog block's
                // inner dialogData elements (which create the slot) are
                // pushed before the trailing WindowHints element.
                if let Some(dialog) = ui_state.dialog_store.get_mut(id) {
                    let dim = |name: &str| {
                        attrs
                            .iter()
                            .find(|(k, _)| k == name)
                            .and_then(|(_, v)| v.parse::<f32>().ok())
                            .unwrap_or(0.0)
                    };
                    let (w, h) = (dim("width"), dim("height"));
                    if w > 1.0 || h > 1.0 {
                        dialog.declared_size = Some((w, h));
                    }
                }
            }
            ParsedElement::Expose { kind, id } => {
                // Redesign Phase 4d: expose = show. The processor can't
                // reach the layout, so queue for the frontend tick
                // (realize_offered_windows). Containers keep their own
                // sighting/opt-in flow for now.
                self.chunk_has_silent_updates = true;
                if kind != "container" {
                    ui_state.pending_exposes.push((kind.clone(), id.clone()));
                } else {
                    // `<exposeContainer>` is the wire's own "a container just
                    // opened" - authoritative where prose isn't: flavored
                    // containers answer `open` with custom verbiage ("You
                    // carefully lift the rune-covered flap...") that no
                    // "You open" pattern matches, and the day-pass preamble
                    // sat out its full 12s response timeout on one (live
                    // Loci Workshop stall). Feed the typed event directly.
                    self.game_line_no += 1;
                    game_state.move_feedback.push_back((
                        self.game_line_no,
                        crate::core::move_feedback::MoveFeedback::ContainerOpened,
                    ));
                }
            }
            ParsedElement::Pulse { mana, min, max } => {
                self.chunk_has_silent_updates = true;
                game_state.pulse_count += 1;
                game_state.next_pulse_mana = *mana;
                // min/max bound the seconds until the NEXT pulse. Anchor
                // both ends in the server clock domain, like RT/CT, so the
                // countdown widget's offset math applies uniformly.
                let now_server = chrono::Utc::now().timestamp() + self.server_time_offset;
                game_state.pulse_next_earliest = Some(now_server + *min as i64);
                game_state.pulse_next_latest = Some(now_server + *max as i64);
                self.update_countdown_by_id(ui_state, "pulse", now_server + *min as i64);
            }
            ParsedElement::InventoryManager {
                token,
                room,
                root: _,
                after: _,
                state,
                items,
                continuations,
            } => {
                self.chunk_has_silent_updates = true;
                let parsed: Vec<_> = items
                    .iter()
                    .filter_map(|attrs| {
                        let item = crate::core::state::ManagedInventoryItem::from_attrs(attrs);
                        if item.is_none() {
                            tracing::warn!(
                                "inventoryManager item missing id/loc, dropped: {:?}",
                                attrs
                            );
                        }
                        item
                    })
                    .collect();
                let cursors: Vec<(String, String)> = continuations
                    .iter()
                    .filter_map(|attrs| {
                        let get = |name: &str| {
                            attrs
                                .iter()
                                .find(|(k, _)| k == name)
                                .map(|(_, v)| v.clone())
                        };
                        Some((get("root")?, get("last")?))
                    })
                    .collect();
                let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                use crate::core::inventory_service::ResponseOutcome;
                match self.inv_service.on_response(
                    token,
                    room,
                    state.as_deref(),
                    parsed.clone(),
                    &cursors,
                    now_ms,
                ) {
                    ResponseOutcome::Publish(snapshot) => {
                        tracing::debug!(
                            "inventoryManager snapshot complete: room={} items={}",
                            snapshot.room,
                            snapshot.items.len()
                        );
                        // Background-enrich container open/closed state with
                        // paced viewitem probes (one per prompt).
                        self.inv_service.queue_container_probes(&snapshot);
                        game_state.managed_inventory = Some(snapshot);
                    }
                    ResponseOutcome::Absorbed | ResponseOutcome::Failed => {
                        // Chunk merged into the in-progress load (or the load
                        // restarted); nothing published yet.
                    }
                    ResponseOutcome::Foreign => {
                        // Not a token we issued (e.g. a manual test request).
                        // Preserve the pre-service behavior: publish what
                        // arrived, incomplete when paginated.
                        let generation = game_state
                            .managed_inventory
                            .as_ref()
                            .map(|s| s.generation + 1)
                            .unwrap_or(1);
                        game_state.managed_inventory =
                            Some(crate::core::state::ManagedInventoryState {
                                token: token.clone(),
                                room: room.clone(),
                                items: parsed,
                                complete: cursors.is_empty(),
                                generation,
                            });
                    }
                }
            }
            ParsedElement::WorldEvent {
                realm,
                expires_min,
                text,
            } => {
                self.chunk_has_silent_updates = true;
                let now = chrono::Utc::now().timestamp();
                game_state
                    .world_events
                    .retain(|e| e.expires_at.is_none_or(|t| t > now));
                game_state
                    .world_events
                    .push(crate::core::state::WorldEventState {
                        realm: realm.clone(),
                        text: text.clone(),
                        expires_at: expires_min.map(|m| now + 60 * m as i64),
                    });
            }
            ParsedElement::PantheonStatus { value } => {
                self.chunk_has_silent_updates = true;
                game_state.pantheon_value = Some(*value);
            }
            ParsedElement::InventoryViewItem(resp) => {
                self.chunk_has_silent_updates = true;
                use crate::core::inventory_service::ViewItemOutcome;
                // The envelope's closed attribute is authoritative container
                // state whichever path answered - apply it either way.
                let apply_closed = |game_state: &mut GameState, exist: &str, closed: bool| {
                    if let Some(snapshot) = game_state.managed_inventory.as_mut() {
                        if let Some(item) = snapshot.items.iter_mut().find(|i| i.id == exist) {
                            let was_closed = item.is_closed();
                            if closed && !was_closed {
                                item.flags.push("closed".to_string());
                            } else if !closed && was_closed {
                                item.flags.retain(|f| f != "closed");
                            }
                            if was_closed != closed {
                                snapshot.generation += 1;
                            }
                        }
                    }
                };
                match self.inv_service.on_viewitem(
                    &resp.token,
                    &resp.exist,
                    resp.state.as_deref(),
                    resp.closed_attr,
                    &resp.results,
                ) {
                    ViewItemOutcome::Probe(verdict) => {
                        apply_closed(game_state, &verdict.exist, verdict.closed);
                    }
                    ViewItemOutcome::Detail {
                        exist,
                        closed,
                        results,
                    } => {
                        apply_closed(game_state, &exist, closed);
                        let name = game_state
                            .managed_inventory
                            .as_ref()
                            .and_then(|s| s.items.iter().find(|i| i.id == exist))
                            .map(|i| i.name.clone())
                            .unwrap_or_else(|| format!("#{exist}"));
                        let generation = game_state
                            .viewed_item
                            .as_ref()
                            .map(|v| v.generation + 1)
                            .unwrap_or(1);
                        game_state.viewed_item = Some(crate::core::state::ViewedItem {
                            exist,
                            name,
                            results,
                            generation,
                        });
                    }
                    ViewItemOutcome::Ignored => {}
                }
            }
            _ => {
                // Other elements handled elsewhere or not yet implemented
            }
        }
    }
}
