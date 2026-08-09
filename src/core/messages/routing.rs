//! Stream-to-window routing: stream window handling, the stream→window
//! map, orphaned-stream resolution, seen-stream registry, subscriber
//! tracking, and squelch/redirect matching.

use super::*;

impl MessageProcessor {
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
    pub(super) fn handle_stream_window(
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

    /// Map stream ID to window name
    pub(super) fn map_stream_to_window(&self, stream: &str) -> String {
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
    pub(super) fn stream_has_target_window(&self, ui_state: &UiState, stream: &str) -> bool {
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
    pub(super) fn resolve_orphaned_stream(&self, stream: &str) -> RouteDecision {
        route_for(
            stream,
            false,
            &self.config.streams.routes,
            &self.config.streams.fallback,
        )
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
    pub(super) fn note_seen_stream(&mut self, id: &str, title: Option<&str>) {
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
    pub(super) fn check_redirect_match(
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
    pub(super) fn should_squelch_line(&self, text: &str) -> bool {
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
