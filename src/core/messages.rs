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

    /// Core highlight engine - applies highlights once during message processing
    highlight_engine: super::highlight_engine::CoreHighlightEngine,

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

    /// Buffer for accumulating perception stream lines (for perception widget)
    perception_buffer: Vec<Vec<TextSegment>>,

    /// Previous room component values (for change detection to avoid unnecessary processing)
    previous_room_components: std::collections::HashMap<String, String>,

    squelch_matcher: Option<aho_corasick::AhoCorasick>,
    squelch_regexes: Vec<regex::Regex>,

    /// Redirect cache: true if any highlights have redirect_to configured (lazy check optimization)
    has_redirect_highlights: bool,

    /// Warn-once cache for empty fast-parse redirect patterns
    warned_empty_redirect_patterns: std::cell::RefCell<std::collections::HashSet<String>>,

    /// Text stream subscribers map: stream_id -> list of window names that subscribe
    /// Built from widget configs at startup and on layout reload
    text_stream_subscribers: std::collections::HashMap<String, Vec<String>>,

    /// Newly registered container (for container discovery mode)
    /// Set when a container is first seen, cleared after processing
    pub newly_registered_container: Option<(String, String)>, // (id, title)

    /// Pending sounds from highlight processing (to be transferred to GameState)
    pub pending_sounds: Vec<super::highlight_engine::SoundTrigger>,
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

        // Build highlight engine from config
        let highlights: Vec<_> = config.highlights.values().cloned().collect();
        let mut highlight_engine = super::highlight_engine::CoreHighlightEngine::new(highlights);
        highlight_engine.set_replace_enabled(config.highlight_settings.replace_enabled);

        let mut processor = Self {
            config,
            parser,
            highlight_engine,
            current_stream: String::from("main"),
            current_segments: Vec::new(),
            chunk_has_main_text: false,
            chunk_has_silent_updates: false,
            discard_current_stream: false,
            server_time_offset: 0,
            inventory_buffer: Vec::new(),
            previous_inventory: Vec::new(),
            perception_buffer: Vec::new(),
            previous_room_components: std::collections::HashMap::new(),
            squelch_matcher: None,
            squelch_regexes: Vec::new(),
            has_redirect_highlights: false,
            warned_empty_redirect_patterns: std::cell::RefCell::new(std::collections::HashSet::new()),
            text_stream_subscribers: std::collections::HashMap::new(),
            newly_registered_container: None,
            pending_sounds: Vec::new(),
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

        // Log loaded presets for debugging
        for (id, preset) in &self.config.colors.presets {
            tracing::debug!(
                "Loaded preset '{}': fg={:?}, bg={:?}",
                id,
                preset.fg,
                preset.bg
            );
        }

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
        self.warned_empty_redirect_patterns.borrow_mut().clear();

        // Update highlight engine with new patterns
        self.update_highlights();
    }

    /// Update the highlight engine with current config patterns.
    /// Called on startup and when highlights are reloaded.
    pub fn update_highlights(&mut self) {
        let highlights: Vec<_> = self.config.highlights.values().cloned().collect();
        self.highlight_engine.update_patterns(highlights);
        self.highlight_engine
            .set_replace_enabled(self.config.highlight_settings.replace_enabled);
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
                    game_state,
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

                // Check if any widget subscribes to this stream (using pre-built subscriber map)
                if self.stream_has_target_window(ui_state, id) {
                    // Stream has subscribers - route normally
                    self.discard_current_stream = false;
                } else {
                    // No subscribers - check drop list vs fallback routing
                    match self.resolve_orphaned_stream(id) {
                        None => {
                            // Stream is in drop_unsubscribed list - discard content
                            self.discard_current_stream = true;
                            tracing::debug!(
                                "Stream '{}' has no subscribers and is in drop list, discarding content",
                                id
                            );
                        }
                        Some(fallback) => {
                            // Not in drop list - will route to fallback window later
                            self.discard_current_stream = false;
                            tracing::debug!(
                                "Stream '{}' has no subscribers, will route to fallback '{}'",
                                id,
                                fallback
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
                // Skip if: no main text was received since last prompt
                // This handles both "silent updates only" and "empty chunk" cases
                let should_skip = !self.chunk_has_main_text;

                // Always reset to main stream when a prompt is received
                // (prompts mark the end of a server response, returning control to main)
                self.current_stream = String::from("main");

                if should_skip {
                    tracing::debug!("Skipping prompt '{}' - no main text since last prompt", text);
                } else if !text.trim().is_empty() {
                    // Store the prompt in game state for command echoes
                    game_state.last_prompt = text.clone();

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
            ParsedElement::TargetList {
                current_target,
                targets: _,    // Ignore - creature list comes from room objs
                target_ids: _, // Ignore - creature list comes from room objs
            } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Dropdown only tells us which creature is currently targeted
                // Creature list comes from room objs component
                game_state.target_list.current_target = current_target.clone();

                tracing::debug!(
                    "Updated current target from dropdown: '{}'",
                    current_target
                );
            }
            ParsedElement::Container { id, title, .. } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Register container in cache
                game_state.container_cache.register_container(id.clone(), title.clone());

                // Signal container for discovery mode (every LOOK IN triggers this)
                // The runtime will check if a window already exists before creating
                if !title.is_empty() {
                    self.newly_registered_container = Some((id.clone(), title.clone()));
                    tracing::debug!(
                        "Container seen: id='{}', title='{}' (signaling for discovery)",
                        id,
                        title
                    );
                } else {
                    tracing::debug!("Registered container: id='{}', title='{}'", id, title);
                }
            }
            ParsedElement::ClearContainer { id } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Clear container contents
                game_state.container_cache.clear_container(id);

                tracing::debug!("Cleared container: id='{}'", id);
            }
            ParsedElement::ContainerItem { container_id, content } => {
                self.chunk_has_silent_updates = true; // Mark as silent update

                // Add item to container
                game_state.container_cache.add_item(container_id, content.clone());

                tracing::trace!("Added item to container '{}': {}", container_id,
                    if content.len() > 50 { format!("{}...", &content[..50]) } else { content.clone() });
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
    fn handle_stream_window(
        &mut self,
        id: &str,
        subtitle: Option<&str>,
        ui_state: &mut UiState,
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

            // Check stream subscribers for discard logic
            if let Some(subscribers) = self.text_stream_subscribers.get(id) {
                if !subscribers.is_empty() {
                    self.discard_current_stream = false;
                } else if self.config.streams.drop_unsubscribed.contains(&id.to_string()) {
                    self.discard_current_stream = true;
                    tracing::debug!("Discarding stream '{}' (in drop_unsubscribed list)", id);
                } else {
                    // Route to fallback
                    self.discard_current_stream = false;
                    tracing::debug!(
                        "Routing stream '{}' to fallback '{}'",
                        id,
                        self.config.streams.fallback
                    );
                }
            } else {
                // No subscribers map entry - check drop list
                if self.config.streams.drop_unsubscribed.contains(&id.to_string()) {
                    self.discard_current_stream = true;
                } else {
                    self.discard_current_stream = false;
                }
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
                .exp_components
                .register_field(field_name.to_string());

            // Update the value (only triggers generation bump if changed)
            if game_state
                .exp_components
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
        }

        tracing::debug!(
            "Processing room component: {} (value length: {})",
            id,
            value.len()
        );

        // Store current value for next comparison
        self.previous_room_components
            .insert(id.to_string(), value.to_string());

        // Extract creatures from room objs (for dropdown_targets widget)
        // Creatures are in bold: <b><pushBold/>a <a exist='ID' noun='...'>name</a><popBold/></b> (status)
        if id == "room objs" {
            game_state.room_creatures.clear();

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
                                                let after_paren = &after_bold[after_bold.find('(').unwrap() + 1..];
                                                after_paren
                                                    .find(')')
                                                    .map(|end| after_paren[..end].to_string())
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
        }

        // Extract players from room players component
        // Format: "Also here: <a exist='-ID' noun='Name'>Name</a> (prone), a stunned <a exist='...' noun='...'>Name2</a> (prone)"
        if id == "room players" {
            game_state.room_players.clear();

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

                                        // Parse prepended status (e.g., "a stunned")
                                        let before_link = &remaining[..link_start];
                                        let primary_status = Self::parse_prepended_status(before_link);

                                        // Parse appended status (e.g., "(prone)")
                                        let after_link = &remaining[link_section_end..];
                                        let secondary_status = Self::parse_appended_status(after_link);

                                        let player = crate::core::state::Player {
                                            id: exist_id.to_string(),
                                            name: player_name.to_string(),
                                            primary_status,
                                            secondary_status,
                                        };

                                        tracing::debug!(
                                            "Parsed player from room players: name='{}', id='{}', primary={:?}, secondary={:?}",
                                            player.name, player.id, player.primary_status, player.secondary_status
                                        );

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

        // Apply highlights ONCE here in core, before segments reach any widget.
        // This ensures text arrives at widgets pre-colored.
        let highlight_result = self
            .highlight_engine
            .apply_highlights(&self.current_segments, &self.current_stream);
        self.current_segments = highlight_result.segments;
        let deferred_replacements = highlight_result.deferred_replacements;

        // Queue sounds from highlight processing
        self.pending_sounds.extend(highlight_result.sounds);

        let mut line = StyledLine {
            segments: std::mem::take(&mut self.current_segments),
            stream: self.current_stream.clone(),
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

            if has_non_speech_text && !has_speech && !highlight_result.line_is_silent {
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

        let mut text_added_to_any_window = false;
        let mut tts_handled = false;

        // Iterate over all windows to find interested parties
        for (window_name, window) in ui_state.windows.iter_mut() {
            let mut added_here = false;
            match &mut window.content {
                WindowContent::Text(content) => {
                    // Check if this text window listens to the current stream
                    if content.streams.iter().any(|s| s.eq_ignore_ascii_case(&self.current_stream)) {
                        // Apply window-specific replacements if any
                        let final_line = if deferred_replacements.is_empty() {
                            line.clone()
                        } else {
                            StyledLine {
                                segments: super::highlight_engine::apply_deferred_for_window(
                                    &line.segments,
                                    &deferred_replacements,
                                    window_name,
                                ),
                                stream: line.stream.clone(),
                            }
                        };
                        content.add_line(final_line);
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
                            // Apply window-specific replacements if any
                            // Check both parent window name and tab name
                            let final_line = if deferred_replacements.is_empty() {
                                line.clone()
                            } else {
                                // Try window name first, then tab name
                                let mut segments = super::highlight_engine::apply_deferred_for_window(
                                    &line.segments,
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
                                    stream: line.stream.clone(),
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
                        self.enqueue_tts(tts_mgr, window_name, &line);
                        tts_handled = true; // Avoid multiple TTS calls for the same line
                    }
                }
            }
        }

        // Fallback routing if no window handled the stream
        // Uses config.streams settings: drop_unsubscribed list and fallback window
        if !text_added_to_any_window {
            match self.resolve_orphaned_stream(&self.current_stream) {
                None => {
                    // Stream is in drop list - discard silently
                    tracing::trace!(
                        "Dropping line from stream '{}' (in drop_unsubscribed list)",
                        self.current_stream
                    );
                    self.chunk_has_silent_updates = true;
                }
                Some(fallback_window) => {
                    // Route to fallback window (defaults to "main")
                    tracing::trace!(
                        "Stream '{}' has no subscribers, routing to fallback '{}'",
                        self.current_stream,
                        fallback_window
                    );
                    if let Some(fallback) = ui_state.get_window_mut(&fallback_window) {
                        if let WindowContent::Text(ref mut content) = fallback.content {
                            // Apply window-specific replacements if any
                            let final_line = if deferred_replacements.is_empty() {
                                line.clone()
                            } else {
                                StyledLine {
                                    segments: super::highlight_engine::apply_deferred_for_window(
                                        &line.segments,
                                        &deferred_replacements,
                                        &fallback_window,
                                    ),
                                    stream: line.stream.clone(),
                                }
                            };
                            content.add_line(final_line);
                            if let Some(tts_mgr) = tts_manager.as_deref_mut() {
                                self.enqueue_tts(tts_mgr, &fallback_window, &line);
                            }
                        }
                    } else if fallback_window != "main" {
                        // Fallback window doesn't exist, try main as last resort
                        tracing::trace!(
                            "Fallback window '{}' not found, routing to main",
                            fallback_window
                        );
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
                                    }
                                };
                                content.add_line(final_line);
                                if let Some(tts_mgr) = tts_manager.as_deref_mut() {
                                    self.enqueue_tts(tts_mgr, "main", &line);
                                }
                            }
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
                            }
                        };
                        content.add_line(final_line);
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

    /// Parse prepended status from text before player link
    /// Format: "a stunned " -> Some("stunned")
    /// Format: "an invisible " -> Some("invisible")
    fn parse_prepended_status(text: &str) -> Option<String> {
        let trimmed = text.trim_end();
        if let Some(space_pos) = trimmed.rfind(' ') {
            let potential_status = &trimmed[space_pos + 1..];
            // Check if there's an article ("a " or "an ") before the status
            if space_pos >= 2 {
                let article_check = &trimmed[space_pos - 2..space_pos];
                if article_check == "a " {
                    return Some(potential_status.to_string());
                }
            }
            if space_pos >= 3 {
                let article_check = &trimmed[space_pos - 3..space_pos];
                if article_check == "an " {
                    return Some(potential_status.to_string());
                }
            }
        }
        None
    }

    /// Parse appended status from text after player link
    /// Format: " (prone), " -> Some("prone")
    /// Format: " (sitting)" -> Some("sitting")
    fn parse_appended_status(text: &str) -> Option<String> {
        let trimmed = text.trim_start();
        if trimmed.starts_with('(') {
            if let Some(end_paren) = trimmed.find(')') {
                return Some(trimmed[1..end_paren].to_string());
            }
        }
        None
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
            "speech" => self.config.tts.speak_speech,
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

    /// Determine what to do with an orphaned stream (no subscribers).
    /// Returns: Some(window_name) to route to, or None to discard.
    fn resolve_orphaned_stream(&self, stream: &str) -> Option<String> {
        // Check if stream is in the drop list
        if self.config.streams.drop_unsubscribed.iter().any(|s| s.eq_ignore_ascii_case(stream)) {
            tracing::debug!("Stream '{}' is in drop_unsubscribed list, discarding", stream);
            return None;
        }

        // Return the fallback window (defaults to "main")
        Some(self.config.streams.fallback.clone())
    }

    /// Clear inventory cache to force next inventory update to render
    /// Should be called when a new inventory window is added
    pub fn clear_inventory_cache(&mut self) {
        self.previous_inventory.clear();
        tracing::debug!("Cleared inventory cache - next inventory update will render");
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

    /// Build the text stream subscriber map from widget configurations.
    /// Call this on startup and after layout reload to update routing.
    pub fn update_text_stream_subscribers(&mut self, ui_state: &UiState) {
        let mut subscribers: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for (window_name, window) in &ui_state.windows {
            match &window.content {
                // Text windows have explicit streams field
                WindowContent::Text(content) => {
                    for stream in &content.streams {
                        subscribers
                            .entry(stream.clone())
                            .or_default()
                            .push(window_name.clone());
                    }
                }

                // Tabbed text windows: each tab has its own streams
                WindowContent::TabbedText(tabbed) => {
                    for tab in &tabbed.tabs {
                        for stream in &tab.definition.streams {
                            subscribers
                                .entry(stream.clone())
                                .or_default()
                                .push(window_name.clone());
                        }
                    }
                }

                // Inventory widget implicitly subscribes to "inv" stream
                WindowContent::Inventory(_) => {
                    subscribers
                        .entry("inv".to_string())
                        .or_default()
                        .push(window_name.clone());
                }

                // Spells widget implicitly subscribes to "Spells" stream
                WindowContent::Spells(_) => {
                    subscribers
                        .entry("Spells".to_string())
                        .or_default()
                        .push(window_name.clone());
                }

                // Perception widget implicitly subscribes to "percWindow" stream
                WindowContent::Perception(_) => {
                    subscribers
                        .entry("percWindow".to_string())
                        .or_default()
                        .push(window_name.clone());
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
                        subscribers
                            .entry(stream.to_string())
                            .or_default()
                            .push(window_name.clone());
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

                // Room widget implicitly subscribes to "room" stream
                WindowContent::Room(_) => {
                    subscribers
                        .entry("room".to_string())
                        .or_default()
                        .push(window_name.clone());
                }

                // ActiveEffects implicitly subscribes to multiple streams
                WindowContent::ActiveEffects(_) => {
                    for stream in &["activespells", "buffs", "debuffs", "cooldowns"] {
                        subscribers
                            .entry(stream.to_string())
                            .or_default()
                            .push(window_name.clone());
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

    /// Get subscribers for a stream (returns empty vec if none)
    pub fn get_stream_subscribers(&self, stream: &str) -> &[String] {
        self.text_stream_subscribers
            .get(stream)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if a stream has any subscribers
    pub fn stream_has_subscribers(&self, stream: &str) -> bool {
        self.text_stream_subscribers
            .get(stream)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
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
                let mut saw_literal = false;
                let mut longest_match: Option<usize> = None;

                for literal in pattern.pattern.split('|') {
                    let trimmed = literal.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    saw_literal = true;

                    if text.contains(trimmed) {
                        let len = trimmed.len();
                        let should_replace = longest_match.map_or(true, |best| len > best);
                        if should_replace {
                            longest_match = Some(len);
                        }
                    }
                }

                if !saw_literal {
                    if self
                        .warned_empty_redirect_patterns
                        .borrow_mut()
                        .insert(pattern.pattern.clone())
                    {
                        tracing::warn!(
                            "Skipping fast-parse redirect with no usable literals: '{}'",
                            pattern.pattern
                        );
                    }
                    None
                } else {
                    longest_match
                }
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
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: Some("alerts".to_string()),
            redirect_mode: crate::config::RedirectMode::RedirectOnly,
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        }
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

        let mut processor = MessageProcessor::new(config);
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

        let mut processor = MessageProcessor::new(config);
        let result = processor.check_redirect_match("zz abc zz");
        assert!(matches!(
            result,
            Some((_window, crate::config::RedirectMode::RedirectOnly, 3))
        ));
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
