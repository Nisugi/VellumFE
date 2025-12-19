//! Core application logic - Pure business logic without UI coupling
//!
//! AppCore manages game state, configuration, and message processing.
//! It has NO knowledge of rendering - all state is stored in data structures
//! that frontends read from.

use crate::cmdlist::CmdList;
use crate::config::{Config, Layout};
use crate::core::{GameState, MessageProcessor};
use crate::data::*;
use crate::parser::{ParsedElement, XmlParser};
use crate::performance::PerformanceStats;
use anyhow::Result;
use std::collections::HashMap;

/// Pending menu request for correlation
#[derive(Clone, Debug)]
pub struct PendingMenuRequest {
    pub exist_id: String,
    pub noun: String,
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

    // === Navigation State ===
    /// Navigation room ID from <nav rm='...'/>
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
    /// Built from config.keybinds at startup and on config reload
    pub keybind_map: HashMap<crate::frontend::common::KeyEvent, crate::config::KeyBindAction>,
}

impl AppCore {
    fn available_themes_message(theme_presets: &HashMap<String, crate::theme::AppTheme>) -> String {
        let mut names: Vec<_> = theme_presets.keys().cloned().collect();
        names.sort();
        format!("Available themes: {}", names.join(", "))
    }


    /// Create a new AppCore instance
    pub fn new(config: Config) -> Result<Self> {
        // Load layout from file system
        let layout = Layout::load(config.character.as_deref())?;

        // Load command list
        let cmdlist = CmdList::load().ok();

        // Create message processor
        let message_processor = MessageProcessor::new(config.clone());

        // Convert presets from config to parser format
        let preset_list: Vec<(String, Option<String>, Option<String>)> = config
            .colors
            .presets
            .iter()
            .map(|(id, preset)| (id.clone(), preset.fg.clone(), preset.bg.clone()))
            .collect();

        // Create parser with presets and event patterns
        let parser = XmlParser::with_presets(preset_list, config.event_patterns.clone());

        // Initialize sound player (if sound feature is enabled)
        // Pass disabled flag to skip audio device initialization on systems without audio hardware
        let sound_player = crate::sound::SoundPlayer::new(
            config.sound.enabled,
            config.sound.volume,
            config.sound.cooldown_ms,
            config.sound.disabled,
        ).ok();
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

        // Build the runtime keybind map from config
        let keybind_map = Self::build_keybind_map(&config);

        let layout_theme = layout.theme.clone();
        let mut app = Self {
            config,
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
        };

        if let Some((theme_id, _)) = app.apply_layout_theme(layout_theme.as_deref()) {
            app.add_system_message(&format!("Theme switched to: {}", theme_id));
            // Update frontend cache later; AppCore just updates config here.
            // The frontend will refresh during initialization from config.
        }

        Ok(app)
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
    /// Only cycles through text and tabbedtext windows since other types don't scroll
    pub fn cycle_focused_window(&mut self) {
        // Get list of scrollable window names (text and tabbedtext only)
        let scrollable_names: Vec<String> = self
            .ui_state
            .windows
            .iter()
            .filter(|(_, w)| {
                matches!(
                    w.widget_type,
                    crate::data::WidgetType::Text | crate::data::WidgetType::TabbedText
                )
            })
            .map(|(name, _)| name.clone())
            .collect();

        if scrollable_names.is_empty() {
            return;
        }

        // Find current index
        let current_idx = self
            .ui_state
            .focused_window
            .as_ref()
            .and_then(|name| scrollable_names.iter().position(|n| n == name))
            .unwrap_or(0);

        // Cycle to next
        let next_idx = (current_idx + 1) % scrollable_names.len();
        let next_name = scrollable_names[next_idx].clone();

        self.ui_state.set_focus(Some(next_name.clone()));
        self.add_system_message(&format!("Focused window: {}", next_name));
        self.needs_render = true;
        tracing::debug!("Cycled focused window to '{}'", next_name);
    }

    // ===========================================================================================
    // Keybind Action Execution
    // ===========================================================================================

    /// Execute a keybind action (called when a bound key is pressed)
    /// Returns a list of commands to send to the server (for macros)

    /// Execute a KeyAction (dispatch to the appropriate method)

    /// Poll TTS events from callback channel and handle them
    /// Should be called in the main event loop to enable auto-play
    pub fn poll_tts_events(&mut self) {
        use std::sync::mpsc::TryRecvError;

        loop {
            match self.tts_manager.try_recv_event() {
                Ok(event) => {
                    match event {
                        crate::tts::TtsEvent::UtteranceEnded => {
                            tracing::debug!("Utterance ended");
                        }
                        crate::tts::TtsEvent::UtteranceStarted => {
                            tracing::debug!("Utterance started");
                        }
                        crate::tts::TtsEvent::UtteranceStopped => {
                            tracing::debug!("Utterance stopped");
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
    }

    /// Initialize windows based on current layout
    pub fn init_windows(&mut self, terminal_width: u16, terminal_height: u16) {
        // Calculate window positions from layout
        let positions = self.calculate_window_positions(terminal_width, terminal_height);

        // Create windows based on layout (only visible ones)
        for window_def in &self.layout.windows {
            // Skip hidden windows
            if !window_def.base().visible {
                tracing::debug!("Skipping hidden window '{}' during init", window_def.name());
                continue;
            }

            let position = positions
                .get(window_def.name())
                .cloned()
                .unwrap_or(WindowPosition {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                });

            let widget_type = match window_def.widget_type() {
                "text" => WidgetType::Text,
                "tabbedtext" => WidgetType::TabbedText,
                "progress" => WidgetType::Progress,
                "countdown" => WidgetType::Countdown,
                "compass" => WidgetType::Compass,
                "injury_doll" | "injuries" => WidgetType::InjuryDoll,
                "indicator" => WidgetType::Indicator,
                "room" => WidgetType::Room,
                "inventory" => WidgetType::Inventory,
                "command_input" | "commandinput" => WidgetType::CommandInput, // Support both for backward compatibility
                "dashboard" => WidgetType::Dashboard,
                "hand" => WidgetType::Hand,
                "active_effects" => WidgetType::ActiveEffects,
                "targets" => WidgetType::Targets,
                "players" => WidgetType::Players,
                "spells" => WidgetType::Spells,
                "performance" => WidgetType::Performance,
                _ => WidgetType::Text,
            };

            let title = window_def
                .base()
                .title
                .as_deref()
                .unwrap_or(window_def.name());

            let content = match widget_type {
                WidgetType::Text => {
                    let buffer_size =
                        if let crate::config::WindowDef::Text { data, .. } = window_def {
                            data.buffer_size
                        } else {
                            1000 // fallback
                        };
                    WindowContent::Text(TextContent::new(title, buffer_size))
                }
                WidgetType::TabbedText => {
                    // Extract tab definitions and buffer size from window def
                    if let crate::config::WindowDef::TabbedText { data, .. } = window_def {
                        let tabs: Vec<(String, Vec<String>, bool, bool)> = data
                            .tabs
                            .iter()
                            .map(|tab| {
                                let show_ts = tab
                                    .show_timestamps
                                    .unwrap_or(self.config.ui.show_timestamps);
                                let ignore = tab.ignore_activity.unwrap_or(false);
                                (tab.name.clone(), tab.get_streams(), show_ts, ignore)
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
                                self.config.ui.show_timestamps,
                                false,
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
                WidgetType::Progress => WindowContent::Progress(ProgressData {
                    value: 100,
                    max: 100,
                    label: if let crate::config::WindowDef::Progress { data, .. } = window_def {
                        data.label.clone().unwrap_or_else(|| title.to_string())
                    } else {
                        title.to_string()
                    },
                    color: None,
                    progress_id: if let crate::config::WindowDef::Progress { data, .. } = window_def
                    {
                        data.id
                            .clone()
                            .unwrap_or_else(|| window_def.name().to_string())
                    } else {
                        window_def.name().to_string()
                    },
                }),
                WidgetType::Countdown => {
                    let (label, countdown_id) = if let crate::config::WindowDef::Countdown { data, .. } =
                        window_def
                    {
                        (
                            data.label
                                .clone()
                                .unwrap_or_else(|| title.to_string()),
                            data.id
                                .clone()
                                .unwrap_or_else(|| window_def.name().to_string()),
                        )
                    } else {
                        (title.to_string(), window_def.name().to_string())
                    };

                    WindowContent::Countdown(CountdownData {
                        end_time: 0,
                        label,
                        countdown_id,
                    })
                }
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
                WidgetType::Inventory => WindowContent::Inventory(TextContent::new(title, 10000)),
                WidgetType::Spells => WindowContent::Spells(TextContent::new(title, 10000)),
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
                    })
                }
                WidgetType::Targets => {
                    let entity_id = if let crate::config::WindowDef::Targets { data, .. } =
                        window_def
                    {
                        data.entity_id.clone()
                    } else {
                        crate::config::default_target_entity_id()
                    };
                    WindowContent::Targets {
                        targets_text: String::new(),
                        count: None,
                        entity_id,
                    }
                }
                WidgetType::Players => {
                    let entity_id = if let crate::config::WindowDef::Players { data, .. } =
                        window_def
                    {
                        data.entity_id.clone()
                    } else {
                        crate::config::default_player_entity_id()
                    };
                    WindowContent::Players {
                        players_text: String::new(),
                        count: None,
                        entity_id,
                    }
                }
                WidgetType::Dashboard => WindowContent::Dashboard {
                    indicators: Vec::new(),
                },
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
            position.x,
            position.y,
            position.width,
            position.height
        );

        let is_room_window = window_def.widget_type() == "room";

        let widget_type = match window_def.widget_type() {
            "text" => WidgetType::Text,
            "tabbedtext" => WidgetType::TabbedText,
            "progress" => WidgetType::Progress,
            "countdown" => WidgetType::Countdown,
            "compass" => WidgetType::Compass,
            "injury_doll" | "injuries" => WidgetType::InjuryDoll,
            "indicator" => WidgetType::Indicator,
            "room" => WidgetType::Room,
            "inventory" => WidgetType::Inventory,
            "command_input" | "commandinput" => WidgetType::CommandInput,
            "dashboard" => WidgetType::Dashboard,
            "hand" => WidgetType::Hand,
            "active_effects" => WidgetType::ActiveEffects,
            "targets" => WidgetType::Targets,
            "players" => WidgetType::Players,
            "spells" => WidgetType::Spells,
            "performance" => WidgetType::Performance,
            _ => WidgetType::Text,
        };

        let title = window_def
            .base()
            .title
            .as_deref()
            .unwrap_or("");

        let content = match widget_type {
            WidgetType::Text => {
                let buffer_size = if let crate::config::WindowDef::Text { data, .. } = window_def {
                    data.buffer_size
                } else {
                    1000 // fallback
                };
                WindowContent::Text(TextContent::new(title, buffer_size))
            }
            WidgetType::TabbedText => {
                // Extract tab definitions and buffer size from window def
                if let crate::config::WindowDef::TabbedText { data, .. } = window_def {
                    let tabs: Vec<(String, Vec<String>, bool, bool)> = data
                        .tabs
                        .iter()
                        .map(|tab| {
                            let show_ts =
                                tab.show_timestamps.unwrap_or(self.config.ui.show_timestamps);
                            let ignore = tab.ignore_activity.unwrap_or(false);
                            (tab.name.clone(), tab.get_streams(), show_ts, ignore)
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
                            self.config.ui.show_timestamps,
                            false,
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
            WidgetType::Progress => WindowContent::Progress(ProgressData {
                value: 100,
                max: 100,
                label: title.to_string(),
                color: None,
                progress_id: if let crate::config::WindowDef::Progress { data, .. } = window_def {
                    data.id
                        .clone()
                        .unwrap_or_else(|| window_def.name().to_string())
                } else {
                    window_def.name().to_string()
                },
            }),
            WidgetType::Countdown => WindowContent::Countdown(CountdownData {
                end_time: 0,
                label: if let crate::config::WindowDef::Countdown { data, .. } = window_def {
                    data.label.clone().unwrap_or_else(|| title.to_string())
                } else {
                    title.to_string()
                },
                countdown_id: if let crate::config::WindowDef::Countdown { data, .. } =
                    window_def
                {
                    data.id
                        .clone()
                        .unwrap_or_else(|| window_def.name().to_string())
                } else {
                    window_def.name().to_string()
                },
            }),
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
            WidgetType::Inventory => WindowContent::Inventory(TextContent::new(title, 0)),
            WidgetType::Spells => WindowContent::Spells(TextContent::new(title, 0)),
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
                })
            }
            WidgetType::Targets => WindowContent::Targets {
                targets_text: String::new(),
                count: None,
                entity_id: crate::config::default_target_entity_id(),
            },
            WidgetType::Players => WindowContent::Players {
                players_text: String::new(),
                count: None,
                entity_id: crate::config::default_player_entity_id(),
            },
            WidgetType::Dashboard => WindowContent::Dashboard {
                indicators: Vec::new(),
            },
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
        };

        self.ui_state
            .set_window(window_def.name().to_string(), window);
        self.needs_render = true;

        // Clear inventory cache if this is an inventory window to force initial render
        if window_def.widget_type() == "inventory" {
            self.message_processor.clear_inventory_cache();
        }

        // Set dirty flag for room windows to trigger sync in TUI frontend
        if is_room_window {
            self.room_window_dirty = true;
        }

        tracing::info!(
            "Created new window '{}' at ({}, {}) size {}x{}",
            window_def.name(),
            position.x,
            position.y,
            position.width,
            position.height
        );
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
                position.x,
                position.y,
                position.width,
                position.height
            );
        }
    }

    /// Remove a window from UI state
    pub fn remove_window(&mut self, name: &str) {
        self.ui_state.remove_window(name);
        self.needs_render = true;
        tracing::info!("Removed window '{}'", name);
    }

    /// Process incoming XML data from server
    pub fn process_server_data(&mut self, data: &str) -> Result<()> {
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
        }

        Ok(())
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
            // Window management
            ".windows".to_string(),
            ".deletewindow".to_string(),
            ".delwindow".to_string(),
            ".addwindow".to_string(),
            ".rename".to_string(),
            ".border".to_string(),
            ".editwindow".to_string(),
            ".editwin".to_string(),
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
            // Theme commands
            ".themes".to_string(),
            ".settheme".to_string(),
            ".theme".to_string(),
            ".edittheme".to_string(),
            // Tab navigation
            ".nexttab".to_string(),
            ".prevtab".to_string(),
            ".gonew".to_string(),
            ".nextunread".to_string(),
            // Settings
            ".settings".to_string(),
            // Toggles
            ".toggleignores".to_string(),
            ".ignores".to_string(),
            // Window locking
            ".lockwindows".to_string(),
            ".lockall".to_string(),
            ".unlockwindows".to_string(),
            ".unlockall".to_string(),
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

    /// List all loaded highlights
    pub(super) fn list_highlights(&mut self) {
        let count = self.config.highlights.len();

        // Collect all highlight info first to avoid borrow checker issues
        let mut lines = vec![format!("=== Highlights ({}) ===", count)];

        for (name, pattern) in &self.config.highlights {
            let mut info = format!("  {} - pattern: '{}'", name, pattern.pattern);
            if let Some(ref fg) = pattern.fg {
                info.push_str(&format!(" fg:{}", fg));
            }
            if let Some(ref bg) = pattern.bg {
                info.push_str(&format!(" bg:{}", bg));
            }
            if pattern.bold {
                info.push_str(" bold");
            }
            lines.push(info);
        }

        // Add all messages
        for line in lines {
            self.add_system_message(&line);
        }
    }

    /// Add a system message to the main window
    pub fn add_system_message(&mut self, message: &str) {
        use crate::data::{SpanType, StyledLine, TextSegment, WindowContent};

        if let Some(main_window) = self.ui_state.get_window_mut("main") {
            if let WindowContent::Text(ref mut content) = main_window.content {
                let line = StyledLine {
                    segments: vec![TextSegment {
                        text: message.to_string(),
                        fg: Some("#00ff00".to_string()),
                        bg: None,
                        bold: true,
                        span_type: SpanType::System, // system echo; skip highlight transforms
                        link_data: None,
                    }],
                    stream: String::from("main"),
                };
                content.add_line(line);
                self.needs_render = true;
            }
        }
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
        self.add_system_message("  .menu                   - Open main menu");
        self.add_system_message("  .settings               - Open settings editor");
        self.add_system_message("  .reload [category]      - Reload config from disk (highlights|keybinds|settings|colors)");
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
        self.add_system_message("");

        // Tab navigation
        self.add_system_message("TAB NAVIGATION:");
        self.add_system_message("  .nexttab                - Switch to next tab");
        self.add_system_message("  .prevtab                - Switch to previous tab");
        self.add_system_message("  .gonew / .nextunread    - Jump to next tab with unread messages");
        self.add_system_message("");

        // Toggles
        self.add_system_message("TOGGLES:");
        self.add_system_message("  .toggleignores          - Toggle squelch patterns on/off");
        self.add_system_message("  .ignores                - Alias for .toggleignores");
        self.add_system_message("");

        // Window locking
        self.add_system_message("WINDOW LOCKING:");
        self.add_system_message("  .lockwindows / .lockall - Lock all windows (prevent move/resize)");
        self.add_system_message("  .unlockwindows / .unlockall - Unlock all windows");
        self.add_system_message("");

        self.add_system_message("Type the command name for more details. Example: .help windows");
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
                base.col,
                base.row,
                base.cols,
                base.rows
            );

            if let Some(window_state) = self.ui_state.windows.get(&window_name) {
                let ui_pos = &window_state.position;
                tracing::info!(
                    "Window '{}' - Capturing from UI state: pos=({},{}) size={}x{}",
                    window_name,
                    ui_pos.x,
                    ui_pos.y,
                    ui_pos.width,
                    ui_pos.height
                );

                // Clamp window position and size to terminal boundaries before saving
                let clamped_x = ui_pos.x.min(terminal_width.saturating_sub(1));
                let clamped_y = ui_pos.y.min(terminal_height.saturating_sub(1));

                // Ensure width doesn't exceed available space
                let max_width = terminal_width.saturating_sub(clamped_x);
                let clamped_width = ui_pos.width.min(max_width).max(10);

                // Ensure height doesn't exceed available space
                let max_height = terminal_height.saturating_sub(clamped_y);
                let clamped_height = ui_pos.height.min(max_height).max(1);

                if clamped_x != ui_pos.x
                    || clamped_y != ui_pos.y
                    || clamped_width != ui_pos.width
                    || clamped_height != ui_pos.height
                {
                    tracing::warn!(
                        "Window '{}' clamped: ({},{} {}x{}) -> ({},{} {}x{}) to fit terminal {}x{}",
                        window_name,
                        ui_pos.x,
                        ui_pos.y,
                        ui_pos.width,
                        ui_pos.height,
                        clamped_x,
                        clamped_y,
                        clamped_width,
                        clamped_height,
                        terminal_width,
                        terminal_height
                    );
                }

                let base = window_def.base_mut();
                base.row = clamped_y;
                base.col = clamped_x;
                base.rows = clamped_height;
                base.cols = clamped_width;

                tracing::debug!(
                    "Window '{}' AFTER capture: pos=({},{}) size={}x{}",
                    window_name,
                    base.col,
                    base.row,
                    base.cols,
                    base.rows
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
                pos.width,
                pos.height,
                pos.x,
                pos.y,
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
            if window_def.name() == name && window_def.base().visible {
                window_def.base_mut().visible = false;
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

    /// Delete a window (legacy - use hide_window instead)
    pub(super) fn delete_window(&mut self, name: &str) {
        // For backwards compatibility, redirect to hide
        self.hide_window(name);
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
            CompassData, CountdownData, IndicatorData, ProgressData, RoomContent, TextContent,
            WidgetType, WindowContent, WindowPosition, WindowState,
        };

        // Check if window already exists
        if self.ui_state.windows.contains_key(name) {
            self.add_system_message(&format!("Window '{}' already exists", name));
            return;
        }

        // Parse widget type
        let widget_type = match widget_type_str.to_lowercase().as_str() {
            "text" => WidgetType::Text,
            "progress" => WidgetType::Progress,
            "countdown" => WidgetType::Countdown,
            "compass" => WidgetType::Compass,
            "injury_doll" | "injuries" => WidgetType::InjuryDoll,
            "hand" => WidgetType::Hand,
            "room" => WidgetType::Room,
            "indicator" => WidgetType::Indicator,
            "performance" => WidgetType::Performance,
            "command_input" | "commandinput" => WidgetType::CommandInput,
            _ => {
                self.add_system_message(&format!("Unknown widget type: {}", widget_type_str));
                self.add_system_message("Types: text, progress, countdown, compass, injury_doll, hand, room, indicator, performance, command_input");
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
            }),
            WidgetType::Countdown => WindowContent::Countdown(CountdownData {
                end_time: 0,
                label: name.to_string(),
                countdown_id: name.to_string(),
            }),
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
            WidgetType::CommandInput => WindowContent::CommandInput {
                text: String::new(),
                cursor: 0,
                history: Vec::new(),
                history_index: None,
            },
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
                x,
                y,
                width,
                height,
            },
            visible: true,
            content_align: None,
            focused: false,
        };

        // Add to UI state
        self.ui_state.set_window(name.to_string(), window);

        // Create window definition for layout
        use crate::config::{
            BorderSides, CommandInputWidgetData, RoomWidgetData, TextWidgetData, WindowBase,
        };

        let base = WindowBase {
            name: name.to_string(),
            row: y,
            col: x,
            rows: height,
            cols: width,
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
            visible: true,
            content_align: None,
        };

        let window_def = match widget_type_str.to_lowercase().as_str() {
            "text" => WindowDef::Text {
                base,
                data: TextWidgetData {
                    streams: vec![],
                    buffer_size: 1000,
                    wordwrap: true,
                    show_timestamps: false,
                },
            },
            "room" => WindowDef::Room {
                base,
                data: RoomWidgetData {
                    buffer_size: 0,
                    show_desc: true,
                    show_objs: true,
                    show_players: true,
                    show_exits: true,
                    show_name: false,
                },
            },
            "command_input" | "commandinput" => WindowDef::CommandInput {
                base,
                data: CommandInputWidgetData::default(),
            },
            _ => {
                // Default to text window for unknown types
                WindowDef::Text {
                    base,
                    data: TextWidgetData {
                        streams: vec![],
                        buffer_size: 1000,
                        wordwrap: true,
                        show_timestamps: false,
                    },
                }
            }
        };

        // Add to layout at the front (so new windows appear on top)
        self.layout.windows.insert(0, window_def);

        self.add_system_message(&format!(
            "Window '{}' added ({}x{} at {},{}) - type: {}",
            name, width, height, x, y, widget_type_str
        ));
        self.needs_render = true;
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

            self.add_system_message(&format!("Border updated for window '{}'", window_name));
            self.needs_render = true;
        } else {
            self.add_system_message(&format!("Window '{}' not found", window_name));
        }
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
        width: u16,
        height: u16,
    ) -> HashMap<String, WindowPosition> {
        let mut positions = HashMap::new();

        // Use layout file values directly (row, col, rows, cols from layout)
        // Scale if terminal size differs from layout's expected terminal size
        let layout_width = self.layout.terminal_width.unwrap_or(width) as f32;
        let layout_height = self.layout.terminal_height.unwrap_or(height) as f32;
        let actual_width = width as f32;
        let actual_height = height as f32;

        // Calculate scale factors (don't scale if layout size is 0 or terminal size matches)
        let scale_x = if layout_width > 0.0 && (layout_width - actual_width).abs() > 1.0 {
            actual_width / layout_width
        } else {
            1.0
        };
        let scale_y = if layout_height > 0.0 && (layout_height - actual_height).abs() > 1.0 {
            actual_height / layout_height
        } else {
            1.0
        };

        tracing::debug!(
            "Layout terminal size: {}x{}, actual: {}x{}, scale: {:.2}x{:.2}",
            layout_width,
            layout_height,
            actual_width,
            actual_height,
            scale_x,
            scale_y
        );

        for window_def in &self.layout.windows {
            // Scale window position and size
            let scaled_x = (window_def.base().col as f32 * scale_x) as u16;
            let scaled_y = (window_def.base().row as f32 * scale_y) as u16;
            let mut scaled_width = (window_def.base().cols as f32 * scale_x).max(1.0) as u16;
            let mut scaled_height = (window_def.base().rows as f32 * scale_y).max(1.0) as u16;

            // Apply min/max constraints from window settings
            if let Some(min_cols) = window_def.base().min_cols {
                if scaled_width < min_cols {
                    tracing::debug!(
                        "Window '{}': enforcing min_cols={} (was {})",
                        window_def.name(),
                        min_cols,
                        scaled_width
                    );
                    scaled_width = min_cols;
                }
            }
            if let Some(max_cols) = window_def.base().max_cols {
                if scaled_width > max_cols {
                    tracing::debug!(
                        "Window '{}': enforcing max_cols={} (was {})",
                        window_def.name(),
                        max_cols,
                        scaled_width
                    );
                    scaled_width = max_cols;
                }
            }
            if let Some(min_rows) = window_def.base().min_rows {
                if scaled_height < min_rows {
                    tracing::debug!(
                        "Window '{}': enforcing min_rows={} (was {})",
                        window_def.name(),
                        min_rows,
                        scaled_height
                    );
                    scaled_height = min_rows;
                }
            }
            if let Some(max_rows) = window_def.base().max_rows {
                if scaled_height > max_rows {
                    tracing::debug!(
                        "Window '{}': enforcing max_rows={} (was {})",
                        window_def.name(),
                        max_rows,
                        scaled_height
                    );
                    scaled_height = max_rows;
                }
            }

            tracing::debug!(
                "Window '{}': layout pos=({},{}) size={}x{}, scaled pos=({},{}) size={}x{}",
                window_def.name(),
                window_def.base().col,
                window_def.base().row,
                window_def.base().cols,
                window_def.base().rows,
                scaled_x,
                scaled_y,
                scaled_width,
                scaled_height
            );

            positions.insert(
                window_def.name().to_string(),
                WindowPosition {
                    x: scaled_x,
                    y: scaled_y,
                    width: scaled_width,
                    height: scaled_height,
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
                command: ".addwindow".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Edit window >".to_string(),
                command: ".editwindow".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Edit Performance".to_string(),
                command: "action:editperformance".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Hide window >".to_string(),
                command: ".hidewindow".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "List windows >".to_string(),
                command: ".windows".to_string(),
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
                return;
            }
        };

        // Group menu items by category
        let mut categories: HashMap<String, Vec<crate::data::ui_state::PopupMenuItem>> =
            HashMap::new();

        for (coord, secondary_noun) in coords {
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

        // Add items to menu
        for cat in &sorted_cats {
            let items = categories.get(cat).unwrap();

            // Categories with _ become submenus (except "0")
            if cat.contains('_') && cat != "0" {
                // Cache submenu items
                self.menu_categories.insert(cat.clone(), items.clone());

                // Add submenu entry to main menu
                let cat_name = cat.split('_').nth(1).unwrap_or(cat).replace('-', " ");
                let cat_name = cat_name
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default()
                    + &cat_name[1..];
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

    /// Request context menu for a link
    /// Returns the _menu command to send to the server
    pub fn request_menu(
        &mut self,
        exist_id: String,
        noun: String,
        click_pos: (u16, u16),
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
            },
        );

        // Store click position for menu placement
        self.last_link_click_pos = Some(click_pos);

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

    /// Quit the application
    pub fn quit(&mut self) {
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
        self.reload_settings();
        self.reload_colors();
        self.reload_layout();
        self.add_system_message("All configuration reloaded");
    }

    /// Reload highlights from disk
    pub fn reload_highlights(&mut self) {
        match crate::config::Config::load_highlights(self.config.character.as_deref()) {
            Ok(highlights) => {
                self.config.highlights = highlights;
                // Rebuild message processor with new highlights
                self.message_processor = crate::core::MessageProcessor::new(self.config.clone());
                self.add_system_message("Highlights reloaded");
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload highlights: {}", e));
            }
        }
    }

    /// Reload keybinds from disk
    pub fn reload_keybinds(&mut self) {
        match crate::config::Config::load_keybinds(self.config.character.as_deref()) {
            Ok(keybinds) => {
                self.config.keybinds = keybinds;
                // Rebuild keybind map for O(1) lookups
                self.keybind_map = Self::build_keybind_map(&self.config);
                self.add_system_message("Keybinds reloaded");
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload keybinds: {}", e));
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
                        self.config.layout_mappings = new_config.layout_mappings;
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
                // Update parser with new presets
                let presets: Vec<(String, Option<String>, Option<String>)> = self
                    .config
                    .colors
                    .presets
                    .iter()
                    .map(|(id, p)| (id.clone(), p.fg.clone(), p.bg.clone()))
                    .collect();
                self.parser.update_presets(presets);
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
        let categories_map = crate::config::Config::get_addable_templates_by_category(&self.layout);

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

    /// Build category submenu showing available windows of that type
    pub fn build_add_window_category_menu(
        &self,
        category: &crate::config::WidgetCategory,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let categories_map = crate::config::Config::get_addable_templates_by_category(&self.layout);

        if let Some(templates) = categories_map.get(category) {
            // Filter out templates already present in the layout (so they disappear once added)
            let available_templates: Vec<_> = templates
                .iter()
                .filter(|name| {
                    self.layout
                        .get_window(name)
                        .map(|w| !w.base().visible)
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

    /// Build "Hide Window" menu showing widget categories (only categories with visible windows)
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
        let categories_map = crate::config::Config::get_visible_templates_by_category(&self.layout, false);

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
        let categories_map = crate::config::Config::get_visible_templates_by_category(&self.layout, false);

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
                        text: self.get_window_display_name(&name),
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
                    text: self.get_window_display_name(name),
                    command: format!("__EDIT__{}", name),
                    disabled: false,
                })
                .collect()
        } else {
            vec![]
        }
    }

    /// Get display name for a window (uses title from template, or falls back to name)
    pub fn get_window_display_name(&self, name: &str) -> String {
        crate::config::Config::get_window_template(name)
            .and_then(|t| t.base().title.clone())
            .unwrap_or_else(|| name.to_string())
    }

    /// Check if text matches any highlight patterns with sounds and play them
    pub fn check_sound_triggers(&self, text: &str) {
        if let Some(ref sound_player) = self.sound_player {
            for pattern in self.config.highlights.values() {
                // Skip if no sound configured for this pattern
                if pattern.sound.is_none() {
                    continue;
                }

                let matches = if pattern.fast_parse {
                    // Fast parse: check if any of the pipe-separated patterns are in the text
                    pattern.pattern.split('|').any(|p| text.contains(p.trim()))
                } else {
                    // Regex parse
                    if let Ok(regex) = regex::Regex::new(&pattern.pattern) {
                        regex.is_match(text)
                    } else {
                        false
                    }
                };

                if matches {
                    if let Some(ref sound_file) = pattern.sound {
                        // Play the sound
                        if let Err(e) =
                            sound_player.play_from_sounds_dir(sound_file, pattern.sound_volume)
                        {
                            tracing::warn!("Failed to play sound '{}': {}", sound_file, e);
                        }
                    }
                }
            }
        }
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
            row: 0,
            col: 0,
            rows: 2,
            cols: 5,
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
            visible: true,
            content_align: None,
            title_position: "top-left".to_string(),
        }
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
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_4");
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
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_2");
    }

    #[test]
    fn test_generate_spacer_name_with_hidden_spacers() {
        // RED: Hidden spacers should be considered (widgets can be hidden, not deleted)
        let mut visible_base = test_window_base("spacer_1");
        visible_base.visible = true;

        let mut hidden_base = test_window_base("spacer_2");
        hidden_base.visible = false;

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
        };

        let name = AppCore::generate_spacer_name(&layout);
        assert_eq!(name, "spacer_100");
    }
}


