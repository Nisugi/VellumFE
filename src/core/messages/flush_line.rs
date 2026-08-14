//! The per-line flush pipeline: highlights, squelch, redirects, sorter
//! transforms, and TTS enqueueing for each completed line.

use super::*;

impl MessageProcessor {
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

    /// Buffer the group events in a flushed chunk, each with the links that
    /// appear on its own line.
    ///
    /// A flush can carry several game lines, and the segments carry no line
    /// boundaries, so this walks the segments accumulating a per-line link
    /// list and cuts it at every newline in the segment text. Attributing all
    /// of a chunk's links to all of its events would merge unrelated joins.
    fn buffer_group_events(&mut self, full_text: &str) {
        use crate::core::group::{classify_line, GroupMember};

        // Classify FIRST -- it is pure &str work. The gate upstream is a
        // substring check, so any room description mentioning a "group" of
        // creatures lands here; building per-line member vecs (three String
        // clones per link) before knowing whether anything matched paid that
        // allocation on every such line.
        let events: Vec<(usize, crate::core::group::GroupEvent)> = full_text
            .lines()
            .enumerate()
            .filter_map(|(idx, line)| classify_line(line).map(|e| (idx, e)))
            .collect();
        if events.is_empty() {
            return;
        }

        // Links, bucketed by which line of the chunk they appeared on.
        let mut per_line: Vec<Vec<GroupMember>> = Vec::new();
        let mut current: Vec<GroupMember> = Vec::new();
        for seg in &self.current_segments {
            if let Some(l) = &seg.link_data {
                // Skip the synthetic link sentinels (`_direct_`, `_url_`);
                // only real game objects carry an exist id.
                if !l.exist_id.is_empty() && !l.exist_id.starts_with('_') {
                    current.push(GroupMember {
                        id: l.exist_id.clone(),
                        noun: l.noun.clone(),
                        name: l.text.clone(),
                    });
                }
            }
            // A segment's text may contain several newlines; each ends a line.
            for _ in 0..seg.text.matches('\n').count() {
                per_line.push(std::mem::take(&mut current));
            }
        }
        // Trailing text with no final newline is still a line.
        if !current.is_empty() {
            per_line.push(current);
        }

        for (idx, event) in events {
            let members = per_line
                .get_mut(idx)
                .map(std::mem::take)
                .unwrap_or_default();
            self.pending_group.push((event, members));
        }
    }

    /// Flush exactly the pending line (no injected-line draining).
    pub(super) fn flush_one_line(
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

        // Move feedback: classify the line into a typed recovery event (hands
        // full, closed door, fell, hard/soft move failure, …) for the walk
        // executor. Buffered here (no game_state); drained at the prompt into
        // game_state.move_feedback so each event fires exactly once. The
        // aho-corasick matcher is cheap on non-matching lines.
        self.game_line_no += 1;
        if let Some(fb) = crate::core::move_feedback::classify_line(&full_text) {
            self.pending_move_feedback.push((self.game_line_no, fb));
        }

        // Raw line for scripted-edge `Await` steps. Unlike the typed feedback
        // above we can't pre-classify these: the patterns live in mapdb data.
        // Only buffered while a scripted edge is actually awaiting, so the
        // common case costs one bool check.
        if self.capture_recent_lines {
            self.pending_recent_lines.push(full_text.clone());
        }

        // Character state (society/profession/CHE/citizenship). Buffer the line
        // for the prompt handler to feed into game_state.character IN ORDER —
        // the PROFILE house parse is stateful across lines. Cheap prefix gate.
        if crate::core::character_state::line_is_character_state(&full_text) {
            self.pending_character_lines.push(full_text.clone());
        }

        // Silver on hand (from `wealth`/`wealth quiet`) — drives go2 funding.
        if full_text.starts_with("You have ") && full_text.contains("silver with you") {
            if let Some(silver) = crate::core::character_state::parse_wealth_line(&full_text) {
                self.pending_silver = Some(silver);
            }
        }

        // Group membership. The text says WHAT happened; the line's
        // `<a exist noun>` links say TO WHOM, so both travel together. Keyed
        // on exist id rather than name, since two players can share a name.
        // Buffered here (no game_state) and applied at the prompt in order —
        // a `group` reply's roster line must land before its status sentinel.
        //
        // One flush can hold SEVERAL game lines (nothing flushes on an
        // embedded newline), so links are attributed per line by walking the
        // segments and tracking newlines. Attributing every link in the chunk
        // to every event would put Carol in Bob's join.
        // ONE gate, owned by the group module. A hand-copied duplicate here
        // once drifted from it and silently killed every hand-holding event
        // in production -- the classifier's own tests kept passing because
        // they never crossed this line.
        if crate::core::group::might_be_group_line(&full_text) {
            self.buffer_group_events(&full_text);
        }

        // READY/STOW list rows: observe (don't squelch — the player asked for
        // the list). Buffer the flat text + the first item link; the prompt
        // handler feeds them into game_state.objects' ready/stow state, which
        // drives the hands stow cascade (P2). Cheap: a couple of prefix checks
        // gate the work, so ordinary lines pay almost nothing.
        if crate::core::game_objects::ready_stow::line_is_ready_stow(&full_text) {
            let link = self.current_segments.iter().find_map(|seg| {
                seg.link_data.as_ref().map(|l| {
                    crate::core::game_objects::GameItem::new(
                        l.exist_id.clone(),
                        l.noun.clone(),
                        l.text.clone(),
                    )
                })
            });
            self.pending_ready_stow.push((full_text.clone(), link));
        }

        // Chronomage day-pass description / expiry lines (from `look`ing at a
        // pass) — feed the day-pass cache that gates day-pass travel. The
        // description/EXPIRED lines carry a `noun="pass"` link whose exist-id
        // keys the pass; the expiry line has no link. Buffer with the pass id
        // for the prompt handler to apply IN ORDER (expiry follows description).
        if crate::core::day_pass::line_is_day_pass(&full_text) {
            let pass_id = self.current_segments.iter().find_map(|seg| {
                seg.link_data
                    .as_ref()
                    .filter(|l| l.noun == "pass")
                    .map(|l| l.exist_id.clone())
            });
            self.pending_day_pass_lines.push((full_text.clone(), pass_id));
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
        self.pending_alerts.extend(highlight_result.alerts);

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
                    inline_image: None,
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
                                segments: crate::core::highlight_engine::apply_deferred_for_window(
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
                                let mut segments = crate::core::highlight_engine::apply_deferred_for_window(
                                    &src.segments,
                                    &deferred_replacements,
                                    window_name,
                                );
                                // Also check tab name (allows targeting specific tabs)
                                segments = crate::core::highlight_engine::apply_deferred_for_window(
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
                                    segments: crate::core::highlight_engine::apply_deferred_for_window(
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
                                                crate::core::highlight_engine::apply_deferred_for_window(
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
                                segments: crate::core::highlight_engine::apply_deferred_for_window(
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
                                segments: crate::core::highlight_engine::apply_deferred_for_window(
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

    /// Enqueue text for TTS if enabled and configured for this window
    /// Replace the set of windows whose defs opt into TTS.
    pub fn set_tts_windows(&mut self, windows: std::collections::HashSet<String>) {
        self.tts_windows = windows;
    }

    /// Replace the set of indicator ids claimed by template condition states.
    /// Dashboard runtime auto-discovery skips these (uppercase-keyed).
    pub fn set_claimed_indicator_ids(
        &mut self,
        ids: std::collections::HashSet<String>,
    ) {
        self.claimed_indicator_ids = ids;
    }

    /// Refresh the processor's TTS config snapshot. The processor holds its
    /// own Config copy from construction; without this, enabling TTS in the
    /// settings editor wouldn't take effect until restart (enqueue_tts gates
    /// on the stale copy).
    pub fn set_tts_config(&mut self, tts: crate::config::TtsConfig) {
        self.config.tts = tts;
    }

    pub(super) fn enqueue_tts(&self, tts_manager: &mut crate::tts::TtsManager, window_name: &str, line: &StyledLine) {
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

}
