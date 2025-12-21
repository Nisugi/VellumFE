//! XML message processing
//!
//! Handles parsing and routing of XML messages from the game server.
//! Updates GameState and UiState based on incoming messages.

use crate::config::{Config, SpellColorStyle};
use crate::core::GameState;
use crate::data::*;
use crate::parser::ParsedElement;
// std::time unused here

/// Processes incoming game messages and updates state
pub struct MessageProcessor {
    /// Configuration (for presets, highlights, etc.)
    config: Config,

    /// Parser for parsing XML content
    parser: crate::parser::XmlParser,

    /// Current text stream (for multi-line messages)
    current_stream: String,

    /// Accumulated styled text for current stream
    current_segments: Vec<TextSegment>,

    /// Track if chunk (since last prompt) has main stream text
    chunk_has_main_text: bool,

    /// Track if chunk (since last prompt) has silent updates
    pub chunk_has_silent_updates: bool,

    /// If true, discard text because no window exists for current stream
    discard_current_stream: bool,

    /// Server time offset for countdown synchronization
    pub server_time_offset: i64,

    /// Buffer for accumulating inventory stream lines (double-buffer system)
    inventory_buffer: Vec<Vec<TextSegment>>,

    /// Previous inventory buffer for comparison (avoid unnecessary updates)
    previous_inventory: Vec<Vec<TextSegment>>,

    /// Buffer for accumulating combat stream lines (for targets widget)
    combat_buffer: Vec<Vec<TextSegment>>,

    /// Buffer for accumulating playerlist stream lines (for players widget)
    playerlist_buffer: Vec<Vec<TextSegment>>,

    /// Buffer for accumulating perception stream lines (for perception widget)
    perception_buffer: Vec<Vec<TextSegment>>,

    /// Previous room component values (for change detection to avoid unnecessary processing)
    previous_room_components: std::collections::HashMap<String, String>,

    /// Squelch/ignore system for filtering unwanted lines
    squelch_enabled: bool,
    squelch_matcher: Option<aho_corasick::AhoCorasick>,
    squelch_regexes: Vec<regex::Regex>,

    /// Redirect cache: true if any highlights have redirect_to configured (lazy check optimization)
    has_redirect_highlights: bool,
}

impl MessageProcessor {
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
    pub fn new(config: Config) -> Self {
        // Create parser with presets from config
        let preset_list = config
            .colors
            .presets
            .iter()
            .map(|(id, preset)| (id.clone(), preset.fg.clone(), preset.bg.clone()))
            .collect();
        let event_patterns = config.event_patterns.clone();
        let parser = crate::parser::XmlParser::with_presets(preset_list, event_patterns);

        let mut processor = Self {
            config,
            parser,
            current_stream: String::from("main"),
            current_segments: Vec::new(),
            chunk_has_main_text: false,
            chunk_has_silent_updates: false,
            discard_current_stream: false,
            server_time_offset: 0,
            inventory_buffer: Vec::new(),
            previous_inventory: Vec::new(),
            combat_buffer: Vec::new(),
            playerlist_buffer: Vec::new(),
            perception_buffer: Vec::new(),
            previous_room_components: std::collections::HashMap::new(),
            squelch_enabled: false,
            squelch_matcher: None,
            squelch_regexes: Vec::new(),
            has_redirect_highlights: false,
        };

        // Initialize squelch patterns from config
        processor.update_squelch_patterns();
        // Initialize redirect cache from config
        processor.update_redirect_cache();
        processor
    }

    /// Refresh internal config, parser presets, and caches after a reload.
    pub fn apply_config(&mut self, mut config: Config) {
        crate::config::Config::compile_highlight_patterns(&mut config.highlights);
        self.config = config;

        let preset_list = self
            .config
            .colors
            .presets
            .iter()
            .map(|(id, preset)| (id.clone(), preset.fg.clone(), preset.bg.clone()))
            .collect();
        self.parser.update_presets(preset_list);
        self.parser
            .update_event_patterns(self.config.event_patterns.clone());

        self.update_squelch_patterns();
        self.update_redirect_cache();
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
            ParsedElement::StreamWindow { id, subtitle } => {
                self.handle_stream_window(
                    id,
                    subtitle.as_deref(),
                    ui_state,
                    room_subtitle,
                    room_window_dirty,
                );
            }
            ParsedElement::Component { id, value } => {
                self.handle_component(
                    id,
                    value,
                    room_components,
                    current_room_component,
                    room_window_dirty,
                );
            }
            ParsedElement::RoomId { id } => {
                *nav_room_id = Some(id.clone());
                *room_window_dirty = true;
                tracing::debug!("Room ID updated: {}", id);
            }
            ParsedElement::StreamPush { id } => {
                self.flush_current_stream_with_tts(ui_state, tts_manager.as_deref_mut());
                self.current_stream = id.clone();

                // Check if this is a stream that should be discarded when window doesn't exist
                // Streams like spells, bounty, room should be discarded
                // Speech/talk/whisper should be dropped to avoid duplicate main stream lines
                let should_discard_if_no_window = matches!(
                    id.as_str(),
                    "spell" | "bounty" | "room" | "speech" | "talk" | "whisper"
                );

                // Check if any window exists for this stream (direct or tabbed)
                if should_discard_if_no_window && !self.stream_has_target_window(ui_state, id) {
                    self.discard_current_stream = true;
                    tracing::debug!("No window exists for stream '{}', discarding content", id);
                } else {
                    self.discard_current_stream = false;
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

                // Clear combat buffer when combat stream is pushed
                if id == "combat" {
                    self.combat_buffer.clear();
                    tracing::debug!("Combat stream pushed - cleared combat buffer");
                }

                // Clear playerlist buffer when playerlist stream is pushed
                if id == "playerlist" {
                    self.playerlist_buffer.clear();
                    tracing::debug!("Playerlist stream pushed - cleared playerlist buffer");
                }

                // Note: perception buffer is NOT cleared on pushStream
                // It's cleared on clearStream (which comes before all entries)
                // This allows entries from multiple push/pop pairs to accumulate
            }
            ParsedElement::StreamPop => {
                self.flush_current_stream_with_tts(ui_state, tts_manager.as_deref_mut());

                // Flush inventory buffer if we're leaving inv stream
                if self.current_stream == "inv" {
                    self.flush_inventory_buffer(ui_state);
                }

                // Flush combat buffer if we're leaving combat stream
                if self.current_stream == "combat" {
                    self.flush_combat_buffer(ui_state);
                }

                // Flush playerlist buffer if we're leaving playerlist stream
                if self.current_stream == "playerlist" {
                    self.flush_playerlist_buffer(ui_state);
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
                        }
                    }
                    tracing::debug!("ClearStream percWindow - cleared buffer and window");
                }
                // Other streams can be handled here as needed
            }
            ParsedElement::Prompt { time, text } => {
                // Finish current stream before prompt
                self.flush_current_stream_with_tts(ui_state, tts_manager.as_deref_mut());

                // Flush perception buffer on prompt (after all entries have accumulated)
                if !self.perception_buffer.is_empty() {
                    self.flush_perception_buffer(ui_state);
                }

                // Decide whether to show this prompt based on chunk tracking
                // Skip if: chunk had ONLY silent updates (no main text)
                let should_skip = self.chunk_has_silent_updates && !self.chunk_has_main_text;

                if should_skip {
                    tracing::debug!("Skipping prompt '{}' - chunk had only silent updates", text);
                } else if !text.trim().is_empty() {
                    // Store the prompt in game state for command echoes
                    game_state.last_prompt = text.clone();

                    // Reset to main stream
                    self.current_stream = String::from("main");

                    // Render prompt with per-character coloring
                    for ch in text.chars() {
                        let char_str = ch.to_string();

                        // Find color for this character in prompt_colors config
                        let color = self
                            .config
                            .colors
                            .prompt_colors
                            .iter()
                            .find(|pc| pc.character == char_str)
                            .and_then(|pc| {
                                // Prefer fg, fallback to color (legacy)
                                pc.fg.as_ref().or(pc.color.as_ref()).cloned()
                            })
                            .unwrap_or_else(|| "#808080".to_string()); // Default dark gray

                        self.current_segments.push(TextSegment {
                            text: char_str,
                            fg: Some(color),
                            bg: None,
                            bold: false,
                            span_type: SpanType::Normal,
                            link_data: None,
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
                span_type,
                link_data,
                ..
            } => {
                // Debug: log perception stream text elements
                if self.current_stream == "percWindow" {
                    tracing::debug!(
                        "Text element on percWindow stream: '{}'",
                        if content.len() > 50 { format!("{}...", &content[..50]) } else { content.to_string() }
                    );
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
                    ParserSpanType::System => DataSpanType::Normal, // system echoes treated as normal for data layer
                };

                self.current_segments.push(TextSegment {
                    text: content.clone(),
                    fg: fg_color.clone(),
                    bg: bg_color.clone(),
                    bold: *bold,
                    span_type: data_span_type,
                    link_data: link_data.clone(),
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
            ParsedElement::LeftHand { item, link } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                game_state.left_hand = if item.is_empty() {
                    None
                } else {
                    Some(item.clone())
                };

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
                            *window_item = game_state.left_hand.clone();
                            *window_link = link.clone();
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
                            *window_item = game_state.right_hand.clone();
                            *window_link = link.clone();
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
                match id.as_str() {
                    "health" => game_state.vitals.health = (*value * 100 / *max) as u8,
                    "mana" => game_state.vitals.mana = (*value * 100 / *max) as u8,
                    "stamina" => game_state.vitals.stamina = (*value * 100 / *max) as u8,
                    "spirit" => game_state.vitals.spirit = (*value * 100 / *max) as u8,
                    _ => {}
                }
            }
            ParsedElement::Spell { text } => {
                self.chunk_has_silent_updates = true; // Mark as silent update
                game_state.spell = Some(text.clone());
            }
            ParsedElement::StatusIndicator { id, active } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Update game state (legacy)
                match id.as_str() {
                    "stunned" => game_state.status.stunned = *active,
                    "bleeding" => game_state.status.bleeding = *active,
                    "hidden" => game_state.status.hidden = *active,
                    "invisible" => game_state.status.invisible = *active,
                    "webbed" => game_state.status.webbed = *active,
                    "dead" => game_state.status.dead = *active,
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
            ParsedElement::ActiveEffect {
                category,
                id,
                value,
                text,
                time,
            } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Find the window for this category
                let window_name = match category.as_str() {
                    "Buffs" => "buffs",
                    "Debuffs" => "debuffs",
                    "Cooldowns" => "cooldowns",
                    "ActiveSpells" => "active_spells",
                    _ => return, // Unknown category
                };

                // Update the window content if it exists
                if let Some(window) = ui_state.get_window_mut(window_name) {
                    if let crate::data::WindowContent::ActiveEffects(ref mut effects_content) =
                        window.content
                    {
                        let spell_style = id
                            .parse::<u32>()
                            .ok()
                            .and_then(|spell_id| self.config.get_spell_color_style(spell_id));
                        let default_style = SpellColorStyle {
                            bar_color: None,
                            text_color: None,
                        };
                        let style = spell_style.unwrap_or(default_style);

                        // Find existing effect or add new one
                        if let Some(effect) =
                            effects_content.effects.iter_mut().find(|e| e.id == *id)
                        {
                            // Update existing effect
                            effect.text = text.clone();
                            effect.value = *value;
                            effect.time = time.clone();
                            effect.bar_color = style.bar_color.clone();
                            effect.text_color = style.text_color.clone();
                        } else {
                            // Add new effect
                            effects_content.effects.push(crate::data::ActiveEffect {
                                id: id.clone(),
                                text: text.clone(),
                                value: *value,
                                time: time.clone(),
                                bar_color: style.bar_color.clone(),
                                text_color: style.text_color.clone(),
                            });
                        }
                    }
                }
            }
            ParsedElement::ClearActiveEffects { category } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Find the window for this category
                let window_name = match category.as_str() {
                    "Buffs" => "buffs",
                    "Debuffs" => "debuffs",
                    "Cooldowns" => "cooldowns",
                    "ActiveSpells" => "active_spells",
                    _ => return, // Unknown category
                };

                // Clear the window content if it exists
                if let Some(window) = ui_state.get_window_mut(window_name) {
                    if let crate::data::WindowContent::ActiveEffects(ref mut effects_content) =
                        window.content
                    {
                        effects_content.effects.clear();
                    }
                }
            }
            _ => {
                // Other elements handled elsewhere or not yet implemented
            }
        }
    }

    /// Handle stream window (DO NOT auto-create windows!)
    fn handle_stream_window(
        &mut self,
        id: &str,
        subtitle: Option<&str>,
        ui_state: &mut UiState,
        room_subtitle_out: &mut Option<String>,
        room_window_dirty: &mut bool,
    ) {
        // Push the stream (streamWindow acts like pushStream)
        self.current_stream = id.to_string();

        // Check if a window exists for this stream
        // For inv and Spells streams, check by content type (allows any window name)
        // For other streams, check by mapped window name
        let has_target_window = match id {
            "inv" => {
                // Check if ANY window has Inventory content type
                ui_state
                    .windows
                    .values()
                    .any(|w| matches!(w.content, crate::data::WindowContent::Inventory(_)))
            }
            "Spells" => {
                // Check if ANY window has Spells content type
                ui_state
                    .windows
                    .values()
                    .any(|w| matches!(w.content, crate::data::WindowContent::Spells(_)))
            }
            _ => {
                // For other streams, check by mapped window name
                let window_name = self.map_stream_to_window(id);
                ui_state.get_window(&window_name).is_some()
            }
        };

        if !has_target_window {
            self.discard_current_stream = true;
            tracing::debug!("No window exists for stream '{}', discarding content", id);
        } else {
            self.discard_current_stream = false;
        }

        // Update room subtitle if this is the room window AND window exists
        if id == "room" && !self.discard_current_stream {
            if let Some(subtitle_text) = subtitle {
                // Remove leading " - " if present (matches VellumFE behavior)
                let clean_subtitle = subtitle_text.trim_start_matches(" - ");
                *room_subtitle_out = Some(clean_subtitle.to_string());
                *room_window_dirty = true;
                tracing::debug!(
                    "Room subtitle updated: {} (cleaned from: {})",
                    clean_subtitle,
                    subtitle_text
                );
            }
        }
    }

    /// Handle component data for room window
    fn handle_component(
        &mut self,
        id: &str,
        value: &str,
        room_components: &mut std::collections::HashMap<String, Vec<Vec<TextSegment>>>,
        current_room_component: &mut Option<String>,
        room_window_dirty: &mut bool,
    ) {
        // Only handle room-related components
        if !id.starts_with("room ") {
            tracing::trace!("Ignoring non-room component: {}", id);
            return;
        }

        // Skip processing if we're discarding the current stream (no window exists)
        if self.discard_current_stream {
            tracing::debug!("Skipping room component {} - no room window exists", id);
            return;
        }

        // Mark as silent update (room components shouldn't trigger prompts in main window)
        self.chunk_has_silent_updates = true;

        // Check if component value has changed (avoid unnecessary processing)
        if let Some(previous_value) = self.previous_room_components.get(id) {
            if previous_value == value {
                tracing::trace!("Room component {} unchanged - skipping processing", id);
                return;
            }
        }

        tracing::debug!(
            "Processing room component: {} (value length: {})",
            id,
            value.len()
        );

        // Store current value for next comparison
        self.previous_room_components
            .insert(id.to_string(), value.to_string());

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
                            ParserSpanType::System => DataSpanType::Normal,
                        };

                        // Link data is already the correct type from parser
                        let link = link_data.clone();

                        let segment = TextSegment {
                            text: content.clone(),
                            fg: fg_color.clone(),
                            bg: bg_color.clone(),
                            bold,
                            span_type: data_span_type,
                            link_data: link.clone(),
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

    /// Flush current text to appropriate window
    pub fn flush_current_stream(&mut self, ui_state: &mut UiState) {
        self.flush_current_stream_with_tts(ui_state, None);
    }

    /// Flush current stream with optional TTS enqueuing
    pub fn flush_current_stream_with_tts(
        &mut self,
        ui_state: &mut UiState,
        mut tts_manager: Option<&mut crate::tts::TtsManager>,
    ) {
        // Debug: log perception stream flushes
        if self.current_stream == "percWindow" {
            tracing::debug!(
                "flush_current_stream_with_tts called for percWindow, segments.len={}",
                self.current_segments.len()
            );
        }

        if self.current_segments.is_empty() {
            return;
        }

        // Concatenate all segments to get full line text for squelch checking
        let full_text: String = self
            .current_segments
            .iter()
            .map(|seg| seg.text.as_str())
            .collect();

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

        let mut line = StyledLine {
            segments: std::mem::take(&mut self.current_segments),
            stream: self.current_stream.clone(),
        };

        // Track main stream text for prompt skip logic.
        // If a line contains any Speech spans, treat it as speech-only (even with trailing punctuation).
        if self.current_stream == "main" {
            let has_speech = line
                .segments
                .iter()
                .any(|seg| seg.span_type == SpanType::Speech);
            let has_non_speech_text = line
                .segments
                .iter()
                .any(|seg| seg.span_type != SpanType::Speech && !seg.text.trim().is_empty());

            if has_non_speech_text && !has_speech {
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
            return;
        }

        // Special handling for inv stream - buffer instead of directly adding to window
        // Inventory updates are sent constantly with same items, so we buffer and compare
        // Inventory stream is always a silent update (shouldn't trigger prompts in main window)
        if self.current_stream == "inv" {
            self.chunk_has_silent_updates = true;
            // Check if ANY window has Inventory content type
            if !ui_state
                .windows
                .values()
                .any(|w| matches!(w.content, WindowContent::Inventory(_)))
            {
                tracing::trace!("Discarding inv stream content - no inventory window exists");
                return;
            }
            // Add line to inventory buffer instead of window
            let num_segments = line.segments.len();
            self.inventory_buffer.push(line.segments);
            tracing::trace!("Buffered inventory line ({} segments)", num_segments);
            return;
        }

        // Special handling for combat stream - buffer for targets widget
        // Combat stream is always a silent update (shouldn't trigger prompts in main window)
        if self.current_stream == "combat" {
            self.chunk_has_silent_updates = true;
            // Check if ANY window has Targets content type
            if !ui_state
                .windows
                .values()
                .any(|w| matches!(w.content, WindowContent::Targets { .. }))
            {
                tracing::trace!("Discarding combat stream content - no targets window exists");
                return;
            }
            // Add line to combat buffer instead of window
            let num_segments = line.segments.len();
            self.combat_buffer.push(line.segments);
            tracing::trace!("Buffered combat line ({} segments)", num_segments);
            return;
        }

        // Special handling for playerlist stream - buffer for players widget
        // Playerlist stream is always a silent update (shouldn't trigger prompts in main window)
        if self.current_stream == "playerlist" {
            self.chunk_has_silent_updates = true;
            // Check if ANY window has Players content type
            if !ui_state
                .windows
                .values()
                .any(|w| matches!(w.content, WindowContent::Players { .. }))
            {
                tracing::trace!("Discarding playerlist stream content - no players window exists");
                return;
            }
            // Add line to playerlist buffer instead of window
            let num_segments = line.segments.len();
            self.playerlist_buffer.push(line.segments);
            tracing::trace!("Buffered playerlist line ({} segments)", num_segments);
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
                    span_type: crate::data::SpanType::Normal,
                    link_data,
                };

                self.perception_buffer.push(vec![entry_segment]);
                tracing::debug!("Buffered perception entry: '{}'", entry_text);
            }
            return;
        }

        // Special handling for targetcount stream - update targets window titles/counts
        if ui_state
            .windows
            .values()
            .filter_map(|w| {
                if let WindowContent::Targets { entity_id, .. } = &w.content {
                    Some(entity_id)
                } else {
                    None
                }
            })
            .any(|id| id == &self.current_stream)
        {
            self.chunk_has_silent_updates = true;
            let count_text: String = line
                .segments
                .iter()
                .map(|seg| seg.text.as_str())
                .collect::<String>()
                .trim()
                .to_string();

            for (_name, window) in ui_state.windows.iter_mut() {
                if let WindowContent::Targets { count, entity_id, .. } = &mut window.content {
                    if *entity_id != self.current_stream {
                        continue;
                    }
                    *count = Some(count_text.clone());
                }
            }
            return;
        }

        // Special handling for playercount stream - update players window titles/counts
        if ui_state
            .windows
            .values()
            .filter_map(|w| {
                if let WindowContent::Players { entity_id, .. } = &w.content {
                    Some(entity_id)
                } else {
                    None
                }
            })
            .any(|id| id == &self.current_stream)
        {
            self.chunk_has_silent_updates = true;
            let count_text: String = line
                .segments
                .iter()
                .map(|seg| seg.text.as_str())
                .collect::<String>()
                .trim()
                .to_string();

            for (_name, window) in ui_state.windows.iter_mut() {
                if let WindowContent::Players { count, entity_id, .. } = &mut window.content {
                    if *entity_id != self.current_stream {
                        continue;
                    }
                    *count = Some(count_text.clone());
                }
            }
            return;
        }

        let mut text_added_to_any_window = false;
        let mut tts_handled = false;

        // Iterate over all windows to find interested parties
        for (window_name, window) in ui_state.windows.iter_mut() {
            let mut added_here = false;
            match &mut window.content {
                WindowContent::Text(content) => {
                    // Check if this text window listens to the current stream
                    if content.streams.iter().any(|s| s.eq_ignore_ascii_case(&self.current_stream)) {
                        content.add_line(line.clone());
                        added_here = true;
                    }
                }
                WindowContent::Inventory(content) => {
                    let mapped_name = self.map_stream_to_window(&self.current_stream);
                    if mapped_name == "inventory" {
                        content.add_line(line.clone());
                        added_here = true;
                    }
                }
                WindowContent::Spells(content) => {
                    let mapped_name = self.map_stream_to_window(&self.current_stream);
                    if mapped_name == "spells" {
                        content.add_line(line.clone());
                        added_here = true;
                    }
                }
                WindowContent::TabbedText(tab_content) => {
                    let active_tab_index = tab_content.active_tab_index;
                    for (tab_index, tab) in tab_content.tabs.iter_mut().enumerate() {
                        if tab
                            .definition
                            .streams
                            .iter()
                            .any(|s| s.trim().eq_ignore_ascii_case(&self.current_stream))
                        {
                            tab.content.add_line(line.clone());
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
                        self.enqueue_tts(tts_mgr, window_name, &line);
                        tts_handled = true; // Avoid multiple TTS calls for the same line
                    }
                }
            }
        }

        // Fallback to main window if no other window handled the stream
        // Exception: speech/talk/whisper streams send duplicate lines (one for dedicated window, one for main)
        // If no dedicated window exists, drop the line instead of falling back (main gets its own copy)
        if !text_added_to_any_window {
            let is_duplicate_stream = matches!(
                self.current_stream.to_lowercase().as_str(),
                "speech" | "talk" | "whisper"
            );

            if is_duplicate_stream {
                tracing::trace!(
                    "Dropping line from stream '{}' (no dedicated window, main gets duplicate copy)",
                    self.current_stream
                );
            } else {
                tracing::trace!(
                    "Window for stream '{}' not found, routing content to main window",
                    self.current_stream
                );
                if let Some(main_window) = ui_state.get_window_mut("main") {
                    if let WindowContent::Text(ref mut content) = main_window.content {
                        content.add_line(line.clone());
                        if let Some(tts_mgr) = tts_manager.as_deref_mut() {
                            self.enqueue_tts(tts_mgr, "main", &line);
                        }
                    }
                }
            }
        }

        // Handle redirect_copy mode: also send to original stream
        if should_send_to_original && self.current_stream != original_stream {
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
                        content.add_line(line.clone());
                    }
                    WindowContent::Inventory(ref mut content) => {
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
                        content.add_line(line.clone());
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
                tracing::warn!("No inventory windows found to update!");
            } else {
                tracing::debug!("Updated {} inventory window(s)", updated_count);
            }

            // Store as new previous inventory
            self.previous_inventory = self.inventory_buffer.clone();
        } else {
            tracing::debug!(
                "Inventory unchanged - skipping update ({} lines)",
                self.inventory_buffer.len()
            );
        }

        // Clear buffer for next update
        self.inventory_buffer.clear();
    }

    /// Flush combat buffer to targets window
    pub fn flush_combat_buffer(&mut self, ui_state: &mut UiState) {
        // If buffer is empty, nothing to do
        if self.combat_buffer.is_empty() {
            return;
        }

        // Concatenate all text segments into a single string
        let mut full_text = String::new();
        for line_segments in &self.combat_buffer {
            for segment in line_segments {
                full_text.push_str(&segment.text);
            }
        }

        tracing::debug!(
            "Flushing combat buffer - {} lines, {} chars total",
            self.combat_buffer.len(),
            full_text.len()
        );

        // Find ALL targets windows and update them (supports multiple targets windows)
        let mut updated_count = 0;
        for (name, window) in ui_state.windows.iter_mut() {
            if let WindowContent::Targets {
                ref mut targets_text,
                ..
            } = window.content
            {
                *targets_text = full_text.clone();
                tracing::debug!(
                    "Updated targets window '{}' with {} chars",
                    name,
                    targets_text.len()
                );
                updated_count += 1;
            }
        }

        if updated_count == 0 {
            tracing::debug!("No targets windows found to update");
        } else {
            tracing::debug!("Updated {} targets window(s)", updated_count);
        }

        // Clear buffer for next update
        self.combat_buffer.clear();
    }

    /// Flush playerlist buffer to players window
    pub fn flush_playerlist_buffer(&mut self, ui_state: &mut UiState) {
        // If buffer is empty, nothing to do
        if self.playerlist_buffer.is_empty() {
            return;
        }

        // Concatenate all text segments into a single string
        let mut full_text = String::new();
        for line_segments in &self.playerlist_buffer {
            for segment in line_segments {
                full_text.push_str(&segment.text);
            }
        }

        tracing::debug!(
            "Flushing playerlist buffer - {} lines, {} chars total",
            self.playerlist_buffer.len(),
            full_text.len()
        );

        // Find ALL players windows and update them (supports multiple players windows)
        let mut updated_count = 0;
        for (name, window) in ui_state.windows.iter_mut() {
            if let WindowContent::Players {
                ref mut players_text,
                ..
            } = window.content
            {
                *players_text = full_text.clone();
                tracing::debug!(
                    "Updated players window '{}' with {} chars",
                    name,
                    players_text.len()
                );
                updated_count += 1;
            }
        }

        if updated_count == 0 {
            tracing::debug!("No players windows found to update");
        } else {
            tracing::debug!("Updated {} players window(s)", updated_count);
        }

        // Clear buffer for next update
        self.playerlist_buffer.clear();
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
            if matches!(window.content, WindowContent::Perception(_)) {
                window.content = WindowContent::Perception(PerceptionData {
                    entries: entries.clone(),
                    last_update: chrono::Utc::now().timestamp(),
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

    /// Enqueue text for TTS if enabled and configured for this window
    fn enqueue_tts(&self, tts_manager: &mut crate::tts::TtsManager, window_name: &str, line: &StyledLine) {
        // Early exit if TTS not enabled
        if !self.config.tts.enabled {
            return;
        }

        // Check if this window should be spoken based on config
        let should_speak = match window_name {
            "thoughts" => self.config.tts.speak_thoughts,
            "speech" => self.config.tts.speak_whispers, // Whispers go to speech window
            "main" => self.config.tts.speak_main,
            _ => false, // Don't speak other windows by default
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

        // Determine priority based on window
        let priority = match window_name {
            "thoughts" => crate::tts::Priority::High, // Thoughts are important
            "speech" => crate::tts::Priority::High,   // Whispers are important
            "main" => crate::tts::Priority::Normal,   // Regular game text
            _ => crate::tts::Priority::Normal,
        };

        // Enqueue speech entry
        tts_manager.enqueue(crate::tts::SpeechEntry {
            text,
            source_window: window_name.to_string(),
            priority,
            spoken: false,
        });

        // Auto-speak the next item in queue (if not currently speaking)
        // This ensures new text gets spoken immediately
        if let Err(e) = tts_manager.speak_next() {
            tracing::warn!("Failed to speak TTS entry: {}", e);
        }
    }

    /// Map stream ID to window name
    fn map_stream_to_window(&self, stream: &str) -> String {
        match stream {
            "main" => "main",
            "room" => "room",
            "inv" => "inventory",
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
            "combat" => "targets",
            "playerlist" => "players",
            "percWindow" => "perception",
            _ => "main", // Default to main window
        }
        .to_string()
    }

    /// Determine if a stream is already handled by any window (direct text window or tabbed text)
    fn stream_has_target_window(&self, ui_state: &UiState, stream: &str) -> bool {
        let mapped = self.map_stream_to_window(stream);

        // Direct window match
        if ui_state.get_window(&mapped).is_some() {
            return true;
        }

        // Any tabbed text window that subscribes to this stream
        ui_state.windows.values().any(|w| {
            if let WindowContent::TabbedText(tab_content) = &w.content {
                tab_content.tabs.iter().any(|tab| {
                    tab.definition
                        .streams
                        .iter()
                        .any(|s| s.trim().eq_ignore_ascii_case(stream))
                })
            } else {
                false
            }
        })
    }

    /// Clear inventory cache to force next inventory update to render
    /// Should be called when a new inventory window is added
    pub fn clear_inventory_cache(&mut self) {
        self.previous_inventory.clear();
        tracing::debug!("Cleared inventory cache - next inventory update will render");
    }

    /// Update squelch pattern matching infrastructure from config
    pub fn update_squelch_patterns(&mut self) {
        self.squelch_enabled = self.config.ui.ignores_enabled;

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
            "Updated squelch patterns: {} fast patterns, {} regex patterns, enabled={}",
            fast_patterns.len(),
            self.squelch_regexes.len(),
            self.squelch_enabled
        );
    }

    /// Update the redirect cache (lazy check optimization)
    pub fn update_redirect_cache(&mut self) {
        self.has_redirect_highlights = self
            .config
            .highlights
            .values()
            .any(|pattern| pattern.redirect_to.is_some());

        tracing::debug!(
            "Updated redirect cache: has_redirect_highlights={}",
            self.has_redirect_highlights
        );
    }

    /// Check if a line matches a redirect pattern
    /// Returns (redirect_window_name, redirect_mode, match_length) if matched
    /// Squelch patterns are excluded (squelch takes precedence)
    /// Longest match wins when multiple patterns match
    fn check_redirect_match(
        &self,
        text: &str,
    ) -> Option<(String, crate::config::RedirectMode, usize)> {
        // Lazy check: skip if no redirects configured
        if !self.has_redirect_highlights {
            return None;
        }

        let mut best_match: Option<(String, crate::config::RedirectMode, usize)> = None;

        // Check all highlight patterns with redirects configured
        for pattern in self.config.highlights.values() {
            // Skip if no redirect or if squelched (squelch takes precedence)
            if pattern.redirect_to.is_none() || pattern.squelch {
                continue;
            }

            let redirect_window = pattern.redirect_to.as_ref().expect("redirect_to checked above");

            // Check if pattern matches
            let match_len = if pattern.fast_parse {
                // Check literal substring match (split on |)
                pattern
                    .pattern
                    .split('|')
                    .filter_map(|literal| {
                        let trimmed = literal.trim();
                        if text.contains(trimmed) {
                            Some(trimmed.len())
                        } else {
                            None
                        }
                    })
                    .next()
            } else {
                // Check regex match
                if let Some(ref regex) = pattern.compiled_regex {
                    regex.find(text).map(|m| m.end() - m.start())
                } else {
                    None
                }
            };

            // Update best match if this match is longer
            if let Some(len) = match_len {
                let is_better = best_match.as_ref().map_or(true, |(_, _, best_len)| len > *best_len);
                if is_better {
                    best_match = Some((
                        redirect_window.clone(),
                        pattern.redirect_mode.clone(),
                        len,
                    ));
                }
            }
        }

        best_match
    }

    /// Check if a line should be squelched (ignored/filtered)
    fn should_squelch_line(&self, text: &str) -> bool {
        if !self.squelch_enabled {
            return false;
        }

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
    // Helper function to create minimal processor for testing
    // ===========================================

    fn create_test_processor() -> MessageProcessor {
        let config = Config::default();
        MessageProcessor::new(config)
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

    // ===========================================
    // map_stream_to_window tests - combat streams
    // ===========================================

    #[test]
    fn test_map_stream_combat() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("combat"), "targets");
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

    #[test]
    fn test_map_stream_playerlist() {
        let processor = create_test_processor();
        assert_eq!(processor.map_stream_to_window("playerlist"), "players");
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
        assert!(processor.combat_buffer.is_empty());
        assert!(processor.playerlist_buffer.is_empty());
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

    #[test]
    fn test_new_processor_squelch_enabled_by_default() {
        let processor = create_test_processor();
        // Squelch enabled by default (ignores_enabled = true in config)
        assert!(processor.squelch_enabled);
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
            span_type: SpanType::Normal,
            link_data: None,
        }]];
        assert!(!processor.previous_inventory.is_empty());

        // Clear cache
        processor.clear_inventory_cache();
        assert!(processor.previous_inventory.is_empty());
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
            ("combat", "targets"),
            ("playerlist", "players"),
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
