//! Buffered-window flushes (inventory, reserve, spells, perception) and
//! the perception/player text parsing they rely on.

use super::*;

impl MessageProcessor {
    /// Commit one complete inventory snapshot and update open inventory windows.
    pub fn flush_inventory_buffer(&mut self, game_state: &mut GameState, ui_state: &mut UiState) {
        // Every inv push/pop pair is a whole replacement. Mirror the exact
        // styled buffer even when it is empty so remote state cannot retain a
        // stale inventory after the game reports none.
        game_state.inventory = self
            .inventory_buffer
            .iter()
            .map(|segments| StyledLine {
                segments: segments.clone(),
                stream: String::from("inv"),
                timestamp: None,
            })
            .collect();

        // Compare to the previous complete snapshot. An empty buffer is still
        // authoritative and must clear any stale inventory window.
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

                    // Add the complete current snapshot
                    for line in &game_state.inventory {
                        content.add_line(line.clone());
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
                tracing::debug!(
                    "No spells windows found to update (buffer preserved for future windows)"
                );
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
            let text: String = line_segments.iter().map(|seg| seg.text.as_str()).collect();

            // Skip empty lines
            if text.trim().is_empty() {
                continue;
            }

            // Get link data from segment
            let link_data = line_segments.iter().find_map(|seg| seg.link_data.clone());

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
    pub(super) fn parse_perception_entry(
        text: &str,
        link_data: Option<LinkData>,
    ) -> PerceptionEntry {
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
                if let Some(pct_str) = suffix.strip_prefix('(').and_then(|s| s.strip_suffix("%)")) {
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
    pub(super) fn calculate_weight(format: &PerceptionFormat) -> i32 {
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
    pub(super) fn split_perception_entries(text: &str) -> Vec<String> {
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
    pub(super) fn map_verbose_posture(phrase: &str) -> Option<&'static str> {
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
    pub(super) fn parse_player_prefix(text: &str) -> (bool, Option<String>) {
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
    pub(super) fn parse_player_suffix_status(text: &str) -> Option<String> {
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
}
