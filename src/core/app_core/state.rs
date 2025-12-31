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

    // === Dialog Position Persistence ===
    /// Saved dialog positions loaded from widget_state.toml
    /// Updated when dialogs with save='t' are dragged/resized
    pub saved_dialog_positions: SavedDialogPositions,
}

impl AppCore {
    /// Create a new AppCore instance
    pub fn new(config: Config) -> Result<Self> {
        // Load layout from file system
        let layout = Layout::load(config.character.as_deref())?;

        // Load command list
        let cmdlist = CmdList::load().ok();

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
            saved_dialog_positions,
        };

        app.apply_session_cache();
        app.apply_custom_quickbars();

        if let Some((theme_id, _)) = app.apply_layout_theme(layout_theme.as_deref()) {
            app.add_system_message(&format!("Theme switched to: {}", theme_id));
            // Update frontend cache later; AppCore just updates config here.
            // The frontend will refresh during initialization from config.
        }

        Ok(app)
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
                if !window_def.base().visible {
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
            crate::data::WidgetType::Indicator => "indicator",
            crate::data::WidgetType::Room => "room",
            crate::data::WidgetType::Inventory => "inventory",
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
            crate::data::WidgetType::MiniVitals => "minivitals",
            crate::data::WidgetType::Betrayer => "betrayer",
        };
        focusable.contains(kind)
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

            let widget_type = WidgetType::from_str(window_def.widget_type());

            let title = window_def
                .base()
                .title
                .as_deref()
                .unwrap_or(window_def.name());

            let content = match widget_type {
                WidgetType::Text => {
                    let (buffer_size, streams, compact) =
                        if let crate::config::WindowDef::Text { data, .. } = window_def {
                            (data.buffer_size, data.streams.clone(), data.compact)
                        } else {
                            (1000, vec![], false) // fallback
                        };
                    let mut text_content = TextContent::new(title, buffer_size);
                    text_content.streams = streams.clone();
                    text_content.compact = compact;

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
                WidgetType::Inventory => {
                    let mut content = TextContent::new(title, 10000);
                    content.streams = vec!["inv".to_string()];
                    WindowContent::Inventory(content)
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
                    })
                }
                WidgetType::Targets => WindowContent::Targets,
                WidgetType::Players => WindowContent::Players,
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
                }),
                WidgetType::Experience => WindowContent::Experience,
                WidgetType::GS4Experience => WindowContent::GS4Experience,
                WidgetType::Encumbrance => WindowContent::Encumbrance,
                WidgetType::Quickbar => WindowContent::Quickbar,
                WidgetType::MiniVitals => WindowContent::MiniVitals,
                WidgetType::Betrayer => WindowContent::Betrayer,
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
            position.x,
            position.y,
            position.width,
            position.height
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
                let (buffer_size, streams, compact) = if let crate::config::WindowDef::Text { data, .. } = window_def {
                    (data.buffer_size, data.streams.clone(), data.compact)
                } else {
                    (1000, vec![], false) // fallback
                };
                let mut text_content = TextContent::new(title, buffer_size);
                text_content.streams = streams;
                text_content.compact = compact;

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
            WidgetType::Perception => WindowContent::Perception(PerceptionData {
                entries: Vec::new(),
                last_update: 0,
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
                })
            }
            WidgetType::Targets => WindowContent::Targets,
            WidgetType::Players => WindowContent::Players,
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
            WidgetType::MiniVitals => WindowContent::MiniVitals,
            WidgetType::Betrayer => WindowContent::Betrayer,
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
            position.x,
            position.y,
            position.width,
            position.height
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
                position.x,
                position.y,
                position.width,
                position.height
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
            ".containers".to_string(),
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
                span_type: SpanType::System, // system echo; skip highlight transforms
                link_data: None,
            }],
            stream: String::from("main"),
        };

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
        self.add_system_message("");

        // Window locking
        self.add_system_message("WINDOW LOCKING:");
        self.add_system_message("  .lockwindows / .lockall - Toggle lock on all windows (prevent move/resize)");
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
                // Use window's min_cols constraint (default 1) instead of hardcoded 10
                let max_width = terminal_width.saturating_sub(clamped_x);
                let min_width = base.min_cols.unwrap_or(1);
                let clamped_width = ui_pos.width.min(max_width).max(min_width);

                // Ensure height doesn't exceed available space
                // Use window's min_rows constraint (default 1)
                let max_height = terminal_height.saturating_sub(clamped_y);
                let min_height = base.min_rows.unwrap_or(1);
                let clamped_height = ui_pos.height.min(max_height).max(min_height);

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

    /// Process pending window additions from openDialog events.
    /// Called by the frontend each frame with terminal dimensions.
    pub fn process_pending_window_additions(&mut self, terminal_width: u16, terminal_height: u16) {
        // Drain pending additions
        let pending: Vec<String> = self.ui_state.pending_window_additions.drain(..).collect();

        for name in pending {
            // Check if window already exists and is visible
            let already_visible = self
                .layout
                .windows
                .iter()
                .any(|w| w.name() == name && w.base().visible);

            if already_visible {
                // Window exists in layout - just make sure it's in UI state
                if !self.ui_state.windows.contains_key(&name) {
                    // Create UI state for existing layout window
                    if let Some(window_def) = self.layout.windows.iter().find(|w| w.name() == name) {
                        let window_def_clone = window_def.clone();
                        self.add_new_window(&window_def_clone, terminal_width, terminal_height);
                        tracing::info!("Created UI state for existing layout window '{}'", name);
                        self.needs_render = true;
                        self.ui_state.needs_widget_reset = true;
                    }
                }
                continue;
            }

            // Add window to layout from template
            if let Err(e) = self.layout.add_window(&name) {
                tracing::warn!("Failed to auto-add window '{}' from dialog: {}", name, e);
                continue;
            }

            // Get the window definition and create UI state
            if let Some(window_def) = self.layout.windows.iter().find(|w| w.name() == name) {
                let window_def_clone = window_def.clone();
                self.add_new_window(&window_def_clone, terminal_width, terminal_height);
                tracing::info!("Auto-added window '{}' from openDialog", name);
                self.needs_render = true;
                // Signal frontend to rebuild widget caches so new window is rendered
                self.ui_state.needs_widget_reset = true;
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
                x,
                y,
                width: w,
                height: h,
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
        let title_lower = title.to_lowercase();

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
            WidgetType::Perception => WindowContent::Perception(PerceptionData {
                entries: Vec::new(),
                last_update: 0,
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
            }),
            WidgetType::Targets => WindowContent::Targets,
            WidgetType::Players => WindowContent::Players,
            WidgetType::Container => WindowContent::Container {
                container_title: String::new(),
            },
            WidgetType::Experience => WindowContent::Experience,
            WidgetType::GS4Experience => WindowContent::GS4Experience,
            WidgetType::Encumbrance => WindowContent::Encumbrance,
            WidgetType::MiniVitals => WindowContent::MiniVitals,
            WidgetType::Betrayer => WindowContent::Betrayer,
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
            ephemeral: false,
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
                    timestamp_position: None,
                    compact: false,
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
                        timestamp_position: None,
                    compact: false,
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
                if window_width < min_cols {
                    tracing::debug!(
                        "Window '{}': enforcing min_cols={} (was {})",
                        window_def.name(),
                        min_cols,
                        window_width
                    );
                    window_width = min_cols;
                }
            }
            if let Some(max_cols) = window_def.base().max_cols {
                if window_width > max_cols {
                    tracing::debug!(
                        "Window '{}': enforcing max_cols={} (was {})",
                        window_def.name(),
                        max_cols,
                        window_width
                    );
                    window_width = max_cols;
                }
            }
            if let Some(min_rows) = window_def.base().min_rows {
                if window_height < min_rows {
                    tracing::debug!(
                        "Window '{}': enforcing min_rows={} (was {})",
                        window_def.name(),
                        min_rows,
                        window_height
                    );
                    window_height = min_rows;
                }
            }
            if let Some(max_rows) = window_def.base().max_rows {
                if window_height > max_rows {
                    tracing::debug!(
                        "Window '{}': enforcing max_rows={} (was {})",
                        window_def.name(),
                        max_rows,
                        window_height
                    );
                    window_height = max_rows;
                }
            }

            tracing::debug!(
                "Window '{}': pos=({},{}) size={}x{}",
                window_def.name(),
                window_def.base().col,
                window_def.base().row,
                window_width,
                window_height
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

                if base.rows != new_rows {
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
                        base.rows = new_rows;
                    }
                    break;
                }
            }

            // Update ui_state window position height
            if let Some(window) = self.ui_state.windows.get_mut(&name) {
                window.position.height = new_rows;
            }

            // Mark modified but don't show the save reminder for auto-resizes
            self.layout_modified_since_save = true;
            self.needs_render = true;
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


