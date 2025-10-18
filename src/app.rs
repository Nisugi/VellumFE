use crate::cmdlist::CmdList;
use crate::config::{Config, KeyAction, KeyBindAction, Layout, parse_key_string};
use crate::network::{LichConnection, ServerMessage};
use crate::parser::{ParsedElement, XmlParser};
use crate::performance::PerformanceStats;
use crate::selection::SelectionState;
use crate::sound::SoundPlayer;
use crate::ui::{CommandInput, PerformanceStatsWidget, SpanType, StyledText, UiLayout, Widget, WindowManager, WindowConfig};
use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    Terminal,
};
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};
use rand::Rng;

/// Debouncer for terminal resize events to prevent excessive layout recalculations
struct ResizeDebouncer {
    last_resize_time: std::time::Instant,
    debounce_duration: std::time::Duration,
    pending_size: Option<(u16, u16)>, // (width, height)
}

impl ResizeDebouncer {
    fn new(debounce_ms: u64) -> Self {
        Self {
            last_resize_time: std::time::Instant::now(),
            debounce_duration: std::time::Duration::from_millis(debounce_ms),
            pending_size: None,
        }
    }

    /// Check if a resize event should be processed or debounced
    /// Returns Some(size) if the resize should be processed now, None if it should be debounced
    fn check_resize(&mut self, width: u16, height: u16) -> Option<(u16, u16)> {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_resize_time);

        if elapsed >= self.debounce_duration {
            // Enough time has passed, process this resize
            self.last_resize_time = now;
            self.pending_size = None;
            Some((width, height))
        } else {
            // Still within debounce period, store pending size
            self.pending_size = Some((width, height));
            None
        }
    }

    /// Check if there's a pending resize that should be processed
    fn check_pending(&mut self) -> Option<(u16, u16)> {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_resize_time);

        if elapsed >= self.debounce_duration {
            if let Some(size) = self.pending_size.take() {
                self.last_resize_time = now;
                return Some(size);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum InputMode {
    Normal,   // Normal text input to command window
    Search,   // Search mode (typing search query)
    HighlightForm,  // Highlight management form
    KeybindForm,  // Keybind management form
    SettingsEditor,  // Settings editor form
    HighlightBrowser,  // Highlight browser
    KeybindBrowser,  // Keybind browser
    ColorPaletteBrowser,  // Color palette browser
    ColorBrowserFilter,  // Color palette browser filter input
    ColorForm,  // Color palette editor form
    SpellColorBrowser,  // Spell color browser
    SpellColorForm,  // Spell color editor form
    WindowEditor,  // Window configuration editor
}

pub struct App {
    config: Config,
    layout: Layout,
    window_manager: WindowManager,
    command_input: CommandInput,
    search_input: CommandInput,  // Separate input for search
    parser: XmlParser,
    running: bool,
    prompt_shown: bool, // Track if we've shown a prompt since last real text
    current_stream: String, // Track which stream we're currently writing to
    discard_current_stream: bool, // If true, discard text because no window exists for current stream
    skip_next_prompt: bool, // Skip the next prompt (after returning from a non-main stream)
    server_time_offset: i64, // Offset between server time and local time (server_time - local_time) - used for countdown calculations to avoid clock drift
    focused_window_index: usize, // Index of currently focused window for scrolling
    resize_state: Option<ResizeState>, // Track active resize operation
    move_state: Option<MoveState>, // Track active window move operation
    input_mode: InputMode,  // Track current input mode
    keybind_map: HashMap<(KeyCode, KeyModifiers), KeyAction>,  // Parsed keybindings
    perf_stats: PerformanceStats,  // Performance statistics
    show_perf_stats: bool,  // Whether to show performance stats window
    stream_buffer: String,  // Buffer for accumulating stream text (used for combat/playerlist)
    sound_player: Option<SoundPlayer>,  // Sound player (None if initialization failed)
    highlight_form: Option<crate::ui::HighlightFormWidget>,  // Highlight form (None when not shown)
    keybind_form: Option<crate::ui::KeybindFormWidget>,  // Keybind form (None when not shown)
    selection_state: Option<SelectionState>,  // Current text selection (None when no selection)
    selection_drag_start: Option<(u16, u16)>,  // Mouse position when drag started (for detecting drag vs click)
    cmdlist: Option<CmdList>,  // Command list for context menus (None if failed to load)
    menu_request_counter: u32,  // Counter for menu request correlation IDs
    pending_menu_requests: HashMap<String, PendingMenuRequest>,  // counter -> (exist_id, noun)
    popup_menu: Option<crate::ui::PopupMenu>,  // Active main popup menu (None when no menu shown)
    submenu: Option<crate::ui::PopupMenu>,  // Active submenu (None when no submenu shown)
    nested_submenu: Option<crate::ui::PopupMenu>,  // Active nested submenu (third level)
    last_link_click_pos: Option<(u16, u16)>,  // Position of last link click (for menu positioning)
    menu_categories: HashMap<String, Vec<crate::ui::MenuItem>>,  // Cached categories for submenus
    mouse_menus_enabled: bool,  // Disable mouse-driven menus to avoid freezes
    drag_state: Option<DragState>,  // Active drag operation (None when not dragging)
    window_editor: crate::ui::WindowEditor,  // Window configuration editor
    settings_editor: Option<crate::ui::SettingsEditor>,  // Settings editor (None when not shown)
    highlight_browser: Option<crate::ui::HighlightBrowser>,  // Highlight browser (None when not shown)
    keybind_browser: Option<crate::ui::KeybindBrowser>,  // Keybind browser (None when not shown)
    color_palette_browser: Option<crate::ui::ColorPaletteBrowser>,  // Color palette browser (None when not shown)
    color_form: Option<crate::ui::ColorForm>,  // Color editor form (None when not shown)
    spell_color_browser: Option<crate::ui::SpellColorBrowser>,  // Spell color browser (None when not shown)
    spell_color_form: Option<crate::ui::SpellColorFormWidget>,  // Spell color editor form (None when not shown)
    baseline_snapshot: Option<(u16, u16)>,  // Baseline terminal size (width, height) for proportional resizing
    baseline_layout: Option<Layout>,  // Baseline layout (original widget positions/sizes) for proportional resizing
    resize_debouncer: ResizeDebouncer,  // Debouncer for terminal resize events (300ms default)
    shown_bounds_warning: bool,  // Track if we've shown the out-of-bounds warning
}

/// Drag and drop state
#[derive(Debug, Clone)]
struct DragState {
    link_data: crate::ui::LinkData,  // What we're dragging
    start_pos: (u16, u16),           // Where drag started (col, row)
    current_pos: (u16, u16),         // Current mouse position (col, row)
}

/// Pending menu request information
#[derive(Debug, Clone)]
struct PendingMenuRequest {
    exist_id: String,
    noun: String,
}

#[derive(Debug, Clone)]
struct ResizeState {
    window_index: usize,
    edge: ResizeEdge,
    start_mouse_pos: (u16, u16), // (col, row) where drag started
}

#[derive(Debug, Clone)]
struct MoveState {
    window_index: usize,
    start_mouse_pos: (u16, u16), // (col, row) where drag started
}

#[derive(Debug, Clone, Copy)]
enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        // Load layout (separate from config)
        // Get terminal size for layout auto-selection
        let terminal_size = crossterm::terminal::size().ok();

        // Priority: auto_<character>.toml → <character>.toml → layout mapping → default.toml → embedded default
        let layout = Layout::load_with_terminal_size(config.character.as_deref(), terminal_size)?;
        info!("Loaded layout with {} windows", layout.windows.len());

        // Clone layout as baseline for resize calculations (before any modifications)
        let baseline_layout = layout.clone();

        // Convert config presets to parser format
        let presets: Vec<(String, Option<String>, Option<String>)> = config
            .presets
            .iter()
            .map(|(id, p)| (id.clone(), p.fg.clone(), p.bg.clone()))
            .collect();

        debug!("Loaded {} prompt color mappings:", config.ui.prompt_colors.len());
        for pc in &config.ui.prompt_colors {
            let fg = pc.fg.as_ref().or(pc.color.as_ref()).map(|s| s.as_str()).unwrap_or("none");
            let bg = pc.bg.as_ref().map(|s| s.as_str()).unwrap_or("none");
            debug!("  '{}' -> fg: {}, bg: {}", pc.character, fg, bg);
        }

        // Convert window configs from layout
        let countdown_icon = Some(config.ui.countdown_icon.clone());
        let window_configs: Vec<WindowConfig> = layout
            .windows
            .iter()
            .map(|w| WindowConfig {
                name: w.name.clone(),
                widget_type: w.widget_type.clone(),
                streams: w.streams.clone(),
                row: w.row,
                col: w.col,
                rows: w.rows,
                cols: w.cols,
                buffer_size: w.buffer_size,
                show_border: w.show_border,
                border_style: w.border_style.clone(),
                border_color: w.border_color.clone(),
                border_sides: w.border_sides.clone(),
                title: w.title.clone(),
                content_align: w.content_align.clone(),
                background_color: w.background_color.clone(),
                bar_color: w.bar_color.clone(),
                bar_background_color: w.bar_background_color.clone(),
                text_color: w.text_color.clone(),
                transparent_background: w.transparent_background,
                countdown_icon: countdown_icon.clone(),
                indicator_colors: w.indicator_colors.clone(),
                dashboard_layout: w.dashboard_layout.clone(),
                dashboard_indicators: w.dashboard_indicators.clone(),
                dashboard_spacing: w.dashboard_spacing,
                dashboard_hide_inactive: w.dashboard_hide_inactive,
                visible_count: w.visible_count,
                effect_category: w.effect_category.clone(),
                tabs: w.tabs.clone(),
                tab_bar_position: w.tab_bar_position.clone(),
                tab_active_color: w.tab_active_color.clone(),
                tab_inactive_color: w.tab_inactive_color.clone(),
                tab_unread_color: w.tab_unread_color.clone(),
                tab_unread_prefix: w.tab_unread_prefix.clone(),
                hand_icon: w.hand_icon.clone(),
                compass_active_color: w.compass_active_color.clone(),
                compass_inactive_color: w.compass_inactive_color.clone(),
                numbers_only: Some(w.numbers_only),
                injury_default_color: w.injury_default_color.clone(),
                injury1_color: w.injury1_color.clone(),
                injury2_color: w.injury2_color.clone(),
                injury3_color: w.injury3_color.clone(),
                scar1_color: w.scar1_color.clone(),
                scar2_color: w.scar2_color.clone(),
                scar3_color: w.scar3_color.clone(),
            })
            .collect();

        debug!("Creating {} windows:", window_configs.len());
        for wc in &window_configs {
            debug!("  '{}' ({}) - streams: {:?}, pos: ({},{}) size: ({}x{}), buffer: {}",
                wc.name, wc.widget_type, wc.streams, wc.row, wc.col, wc.rows, wc.cols, wc.buffer_size);
        }

        // Build keybind map
        let mut keybind_map = HashMap::new();
        for (key_str, keybind_action) in &config.keybinds {
            if let Some((key_code, modifiers)) = parse_key_string(key_str) {
                let action = match keybind_action {
                    KeyBindAction::Action(action_str) => {
                        KeyAction::from_str(action_str)
                    }
                    KeyBindAction::Macro(macro_action) => {
                        Some(KeyAction::SendMacro(macro_action.macro_text.clone()))
                    }
                };

                if let Some(action) = action {
                    keybind_map.insert((key_code, modifiers), action);
                } else {
                    tracing::warn!("Invalid keybind: {} -> {:?}", key_str, keybind_action);
                }
            } else {
                tracing::warn!("Could not parse key string: {}", key_str);
            }
        }

        debug!("Loaded {} keybindings", keybind_map.len());

        // Find command_input from windows array
        let cmd_window = layout.windows.iter()
            .find(|w| w.widget_type == "command_input")
            .expect("command_input must exist in windows array (layout migration should ensure this)");

        // Create command input with config from WindowDef
        let mut command_input = CommandInput::new(100);
        command_input.set_min_command_length(config.ui.min_command_length);
        command_input.set_border_config(
            cmd_window.show_border,
            cmd_window.border_style.clone(),
            cmd_window.border_color.clone(),
        );
        if let Some(ref title) = cmd_window.title {
            command_input.set_title(title.clone());
        }
        command_input.set_background_color(cmd_window.background_color.clone());

        // Initialize sound player
        let sound_player = match SoundPlayer::new(
            config.sound.enabled,
            config.sound.volume,
            config.sound.cooldown_ms,
        ) {
            Ok(player) => {
                // Ensure sounds directory exists
                if let Err(e) = crate::sound::ensure_sounds_directory() {
                    tracing::warn!("Failed to create sounds directory: {}", e);
                }
                Some(player)
            }
            Err(e) => {
                tracing::warn!("Failed to initialize sound player: {}", e);
                None
            }
        };

        // Load command history
        if let Err(e) = command_input.load_history(config.character.as_deref()) {
            tracing::warn!("Failed to load command history: {}", e);
        }

        // Load command list for context menus
        let cmdlist = match CmdList::load() {
            Ok(list) => {
                tracing::info!("Command list loaded successfully");
                Some(list)
            }
            Err(e) => {
                tracing::warn!("Failed to load command list: {}. Context menus will not be available.", e);
                None
            }
        };

        Ok(Self {
            window_manager: WindowManager::new(
                window_configs,
                config.highlights.clone(),
                config.ui.countdown_icon.clone(),
            ),
            command_input,
            search_input: CommandInput::new(50),  // Smaller history for search
            parser: XmlParser::with_presets(presets, config.event_patterns.clone()),
            keybind_map,
            config,
            layout,
            running: true,
            prompt_shown: false,
            current_stream: "main".to_string(),
            discard_current_stream: false,
            skip_next_prompt: false,
            server_time_offset: 0, // No offset until first prompt
            focused_window_index: 0, // Start with first window focused
            resize_state: None, // No active resize initially
            move_state: None, // No active move initially
            input_mode: InputMode::Normal,  // Start in normal mode
            perf_stats: PerformanceStats::new(),  // Initialize performance stats
            show_perf_stats: false,  // Hidden by default
            stream_buffer: String::new(),  // Initialize empty stream buffer
            sound_player,  // Sound player (may be None if initialization failed)
            highlight_form: None,  // No form shown initially
            keybind_form: None,  // No form shown initially
            selection_state: None,  // No selection initially
            selection_drag_start: None,  // No drag initially
            cmdlist,  // Command list (may be None if failed to load)
            menu_request_counter: 0,  // Start counter at 0
            pending_menu_requests: HashMap::new(),  // No pending requests initially
            popup_menu: None,  // No popup menu initially
            submenu: None,  // No submenu initially
            nested_submenu: None,  // No nested submenu initially
            last_link_click_pos: None,  // No last click position initially
            menu_categories: HashMap::new(),  // No cached categories initially
            mouse_menus_enabled: false,  // Keyboard-only menus by default
            drag_state: None,  // No drag operation initially
            window_editor: crate::ui::WindowEditor::new(),  // Initialize window editor
            settings_editor: None,  // Settings editor not shown initially
            highlight_browser: None,  // Highlight browser not shown initially
            keybind_browser: None,  // Keybind browser not shown initially
            color_palette_browser: None,  // Color palette browser not shown initially
            color_form: None,  // Color form not shown initially
            spell_color_browser: None,  // Spell color browser not shown initially
            spell_color_form: None,  // Spell color form not shown initially
            baseline_snapshot: None,  // No baseline snapshot initially (will be set after layout load)
            baseline_layout: Some(baseline_layout),  // Store baseline layout for resize calculations
            resize_debouncer: ResizeDebouncer::new(300),  // 300ms debounce for terminal resize
            shown_bounds_warning: false,  // Haven't shown warning yet
        })
    }

    /// Check if layout fits terminal and warn if not
    pub fn check_and_auto_resize(&mut self) -> Result<()> {
        // Check if layout has designed terminal size
        if let (Some(layout_w), Some(layout_h)) = (self.layout.terminal_width, self.layout.terminal_height) {
            // Get current terminal size
            let (current_w, current_h) = crossterm::terminal::size()
                .context("Failed to get terminal size")?;

            // Check if terminal size doesn't match layout
            if current_w != layout_w || current_h != layout_h {
                let message = if current_w < layout_w || current_h < layout_h {
                    format!(
                        "⚠️  Terminal {}x{} smaller than layout {}x{} - some widgets hidden. Use .resize to adapt.",
                        current_w, current_h, layout_w, layout_h
                    )
                } else {
                    format!(
                        "Terminal {}x{} larger than layout {}x{} - use .resize to expand layout.",
                        current_w, current_h, layout_w, layout_h
                    )
                };

                tracing::warn!("{}", message);
                self.add_system_message(&message);
            }
        }

        Ok(())
    }

    

    /// Check if text matches any highlight patterns with sounds and play them
    fn check_sound_triggers(&mut self, text: &str) {
        if let Some(ref sound_player) = self.sound_player {
            for (_name, pattern) in &self.config.highlights {
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
                        if let Err(e) = sound_player.play_from_sounds_dir(sound_file, pattern.sound_volume) {
                            tracing::warn!("Failed to play sound '{}': {}", sound_file, e);
                        }
                    }
                }
            }
        }
    }

    /// Add text to the appropriate window/tab for the current stream
    fn add_text_to_current_stream(&mut self, text: StyledText) {
        let stream = self.current_stream.clone();

        // Find which window this stream maps to
        let window_name = self.window_manager
            .stream_map
            .get(&stream)
            .cloned()
            .unwrap_or_else(|| "main".to_string());

        // Get the window
        if let Some(widget) = self.window_manager.get_window(&window_name) {
            match widget {
                Widget::Tabbed(tabbed) => {
                    // Route to specific tab based on stream
                    tabbed.add_text_to_stream(&stream, text);
                }
                Widget::Text(text_window) => {
                    text_window.add_text(text);
                }
                _ => {
                    // Other widget types don't support text
                }
            }
        }
    }

    /// Finish the current line in the appropriate window/tab
    fn finish_current_line(&mut self, inner_width: u16) {
        let stream = self.current_stream.clone();

        // Find which window this stream maps to
        let window_name = self.window_manager
            .stream_map
            .get(&stream)
            .cloned()
            .unwrap_or_else(|| "main".to_string());

        // Get the window
        if let Some(widget) = self.window_manager.get_window(&window_name) {
            match widget {
                Widget::Tabbed(tabbed) => {
                    // Finish line for specific tab based on stream
                    tabbed.finish_line_for_stream(&stream, inner_width);
                }
                Widget::Text(text_window) => {
                    text_window.finish_line(inner_width);
                }
                _ => {
                    // Other widget types don't support text
                }
            }
        }
    }

    /// Get the focused window for scrolling
    fn get_focused_window(&mut self) -> Option<&mut Widget> {
        let window_names = self.window_manager.get_window_names();
        if self.focused_window_index < window_names.len() {
            let name = &window_names[self.focused_window_index];
            self.window_manager.get_window(name)
        } else {
            None
        }
    }

    /// Cycle to next window
    fn cycle_focused_window(&mut self) {
        let window_count = self.window_manager.get_window_names().len();
        if window_count > 0 {
            self.focused_window_index = (self.focused_window_index + 1) % window_count;
            debug!("Focused window index: {}", self.focused_window_index);
        }
    }

    /// Check if a mouse position is on a resize border
    /// Returns (window_index, edge) if on a border
    fn check_resize_border(
        &self,
        mouse_col: u16,
        mouse_row: u16,
        window_layouts: &HashMap<String, ratatui::layout::Rect>,
    ) -> Option<(usize, ResizeEdge)> {
        let window_names = self.window_manager.get_window_names();

        for (idx, name) in window_names.iter().enumerate() {
            if let Some(rect) = window_layouts.get(name) {
                // Check corners for top edge resizing (leave middle for title bar dragging)
                // Only resize from top edge at the corners (first and last column)
                if mouse_row == rect.y {
                    if mouse_col == rect.x || mouse_col == rect.x + rect.width.saturating_sub(1) {
                        return Some((idx, ResizeEdge::Top));
                    }
                    // Middle of top border is for moving, not resizing
                }

                // Check if mouse is on bottom border (last row of window)
                if mouse_row == rect.y + rect.height.saturating_sub(1)
                    && mouse_col >= rect.x
                    && mouse_col < rect.x + rect.width
                {
                    return Some((idx, ResizeEdge::Bottom));
                }

                // Check if mouse is on left border (but not top/bottom corners to avoid conflict)
                if mouse_col == rect.x
                    && mouse_row > rect.y
                    && mouse_row < rect.y + rect.height.saturating_sub(1)
                {
                    return Some((idx, ResizeEdge::Left));
                }

                // Check if mouse is on right border (but not top/bottom corners)
                if mouse_col == rect.x + rect.width.saturating_sub(1)
                    && mouse_row > rect.y
                    && mouse_row < rect.y + rect.height.saturating_sub(1)
                {
                    return Some((idx, ResizeEdge::Right));
                }
            }
        }

        None
    }

    /// Check if the mouse is on a window's title bar (top border, but not corners)
    /// Returns the window index if on a title bar
    fn check_title_bar(
        &mut self,
        mouse_col: u16,
        mouse_row: u16,
        window_layouts: &HashMap<String, ratatui::layout::Rect>,
    ) -> Option<usize> {
        let window_names = self.window_manager.get_window_names();

        for (idx, name) in window_names.iter().enumerate() {
            if let Some(rect) = window_layouts.get(name) {
                // Skip title bar detection for borderless tabbed windows
                // (the top row contains tabs, not a title bar)
                if let Some(widget) = self.window_manager.get_window(name) {
                    if let Widget::Tabbed(tabbed) = widget {
                        if !tabbed.has_border() {
                            continue;
                        }
                    }
                }

                // Check if on top border but not in the corners (leave 1 cell margin on each side)
                if mouse_row == rect.y
                    && mouse_col > rect.x
                    && mouse_col < rect.x + rect.width.saturating_sub(1)
                {
                    return Some(idx);
                }
            }
        }
        None
    }

    /// Convert mouse screen coordinates to text position (window_idx, line, col)
    /// Returns None if mouse is not over a text window
    fn mouse_to_text_position(
        &self,
        mouse_col: u16,
        mouse_row: u16,
        window_layouts: &HashMap<String, ratatui::layout::Rect>,
    ) -> Option<(usize, usize, usize)> {
        let window_names = self.window_manager.get_window_names();

        for (idx, name) in window_names.iter().enumerate() {
            if let Some(rect) = window_layouts.get(name) {
                // Check if mouse is within window bounds
                if mouse_col >= rect.x
                    && mouse_col < rect.x + rect.width
                    && mouse_row >= rect.y
                    && mouse_row < rect.y + rect.height
                {
                    // Only handle text windows and tabbed windows for selection
                    if let Some(widget) = self.window_manager.get_window_const(name) {
                        match widget {
                            Widget::Text(_) | Widget::Tabbed(_) => {
                                // Convert to window-relative coordinates
                                // Account for border (if present)
                                let has_border = self.layout.windows.get(idx)
                                    .map(|w| w.show_border)
                                    .unwrap_or(false);

                                let (rel_col, rel_row) = if has_border {
                                    // Border takes 1 cell on each side
                                    (
                                        (mouse_col.saturating_sub(rect.x + 1)) as usize,
                                        (mouse_row.saturating_sub(rect.y + 1)) as usize,
                                    )
                                } else {
                                    (
                                        (mouse_col - rect.x) as usize,
                                        (mouse_row - rect.y) as usize,
                                    )
                                };

                                // Convert relative row to absolute line index (accounting for scrolling)
                                match widget {
                                    Widget::Text(text_window) => {
                                        let visible_height = if has_border {
                                            rect.height.saturating_sub(2) as usize
                                        } else {
                                            rect.height as usize
                                        };
                                        let absolute_line = text_window.relative_row_to_absolute_line(rel_row, visible_height);
                                        return Some((idx, absolute_line, rel_col));
                                    }
                                    Widget::Tabbed(tabbed) => {
                                        // For tabbed windows, get the active tab's text window
                                        if let Some(active_window) = tabbed.get_active_window() {
                                            // Get tab bar position from config
                                            let tab_bar_at_top = self.layout.windows.get(idx)
                                                .and_then(|w| w.tab_bar_position.as_ref())
                                                .map(|pos| pos == "top")
                                                .unwrap_or(true); // default to top

                                            // Account for tab bar height (usually 1 row at top or bottom)
                                            let tab_bar_height: usize = 1;

                                            // Account for inner TextWindow border (always present)
                                            let inner_border_height = if active_window.has_border() { 1 } else { 0 };

                                            // Calculate total offset from rel_row (which is after outer border)
                                            // Structure: [outer border (has_border)] + [tab bar] + [inner border] + [content]
                                            let mut offset = 0;
                                            if tab_bar_at_top {
                                                offset += tab_bar_height; // Tab bar
                                            }
                                            offset += inner_border_height; // Inner TextWindow border

                                            let adjusted_row = rel_row.saturating_sub(offset);

                                            // Calculate visible height: total height - outer borders - tab bar - inner borders
                                            let outer_border_height = if has_border { 2 } else { 0 };
                                            let inner_border_total = if active_window.has_border() { 2 } else { 0 };
                                            let visible_height = rect.height
                                                .saturating_sub(outer_border_height as u16)
                                                .saturating_sub(tab_bar_height as u16)
                                                .saturating_sub(inner_border_total as u16) as usize;

                                            let absolute_line = active_window.relative_row_to_absolute_line(adjusted_row, visible_height);
                                            return Some((idx, absolute_line, rel_col));
                                        }
                                    }
                                    _ => {}
                                }

                                return Some((idx, rel_row, rel_col));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        None
    }

    /// Copy the currently selected text to clipboard
    fn copy_selection_to_clipboard(&mut self, _window_layouts: &HashMap<String, ratatui::layout::Rect>) {
        let selection = match &self.selection_state {
            Some(s) if s.active => s,
            _ => return,
        };

        // Get the window being selected
        let window_names = self.window_manager.get_window_names();
        if selection.start.window_index >= window_names.len() {
            return;
        }

        let window_name = &window_names[selection.start.window_index];

        // Get the text window (either standalone or from active tab)
        let text_window = match self.window_manager.get_window(window_name) {
            Some(Widget::Text(text_window)) => text_window,
            Some(Widget::Tabbed(tabbed)) => {
                // For tabbed windows, get the active tab's text window
                match tabbed.get_active_window_mut() {
                    Some(active_window) => active_window,
                    None => return,
                }
            }
            _ => return,
        };

        let widget = text_window;

        // Extract selected text
        let (start, end) = selection.normalized_range();
        let mut selected_text = String::new();

        // Get the visible lines from the text window
        let lines = widget.get_lines();

        for line_idx in start.line..=end.line.min(lines.len().saturating_sub(1)) {
            if line_idx >= lines.len() {
                break;
            }

            let line = &lines[line_idx];

            // Determine which columns to include from this line
            let (start_col, end_col) = if line_idx == start.line && line_idx == end.line {
                // Same line - use both bounds
                (start.col, end.col)
            } else if line_idx == start.line {
                // First line - from start_col to end
                (start.col, usize::MAX)
            } else if line_idx == end.line {
                // Last line - from beginning to end_col
                (0, end.col)
            } else {
                // Middle lines - full line
                (0, usize::MAX)
            };

            // Extract text from the line
            let mut col_offset = 0;
            for segment in &line.segments {
                let segment_end = col_offset + segment.text.len();

                if col_offset >= end_col {
                    break;
                }

                if segment_end > start_col {
                    // This segment overlaps with selection
                    let seg_start = start_col.saturating_sub(col_offset);
                    let seg_end = (end_col.saturating_sub(col_offset)).min(segment.text.len());

                    if seg_start < segment.text.len() {
                        let chars: Vec<char> = segment.text.chars().collect();
                        let selected_chars: String = chars[seg_start..seg_end.min(chars.len())].iter().collect();
                        selected_text.push_str(&selected_chars);
                    }
                }

                col_offset = segment_end;
            }

            // Add newline except for last line
            if line_idx < end.line {
                selected_text.push('\n');
            }
        }

        // Copy to clipboard
        if !selected_text.is_empty() {
            match arboard::Clipboard::new() {
                Ok(mut clipboard) => {
                    if let Err(e) = clipboard.set_text(&selected_text) {
                        debug!("Failed to copy to clipboard: {}", e);
                    } else {
                        debug!("Copied {} characters to clipboard", selected_text.len());
                    }
                }
                Err(e) => {
                    debug!("Failed to access clipboard: {}", e);
                }
            }
        }
    }

    // [removed] legacy proportional resizing (v1)
    /* fn apply_proportional_resize(&mut self, width_delta: i32, height_delta: i32) {
        use std::collections::HashSet;

        tracing::debug!("=== PROPORTIONAL RESIZE ===");
        tracing::debug!("Width delta: {:+}, Height delta: {:+}", width_delta, height_delta);

        // Helper function to get hard-coded minimum size for widget type
        let get_widget_min_size = |widget_type: &str| -> (u16, u16) {
            match widget_type {
                "progress" | "countdown" | "indicator" | "hands" | "hand" => (10, 1),
                "compass" => (13, 5),
                "injury_doll" => (20, 10),
                "dashboard" => (15, 3),
                "command_input" => (20, 1),
                _ => (5, 3), // text, tabbed, etc.
            }
        };

        // Categorize widgets by scaling behavior
        let mut static_both = HashSet::new();
        let mut static_height = HashSet::new();

        for window in &self.layout.windows {
            match window.widget_type.as_str() {
                "compass" | "injury_doll" | "dashboard" | "indicator" => {
                    static_both.insert(window.name.clone());
                }
                "progress" | "countdown" | "hands" | "hand" | "lefthand" | "righthand" | "spellhand" | "command_input" => {
                    static_height.insert(window.name.clone());
                }
                _ => {} // Fully scalable
            }
        }

        // HEIGHT PROCESSING: Process column-by-column (left to right)
        if height_delta != 0 {
            tracing::debug!("--- HEIGHT SCALING ---");

            // Track which widgets have already received height adjustment
            let mut height_applied = HashSet::new();

            // Build list of all scalable widgets with their column ranges
            let mut scalable_widgets: Vec<(String, u16, u16, u16, u16)> = Vec::new();
            for window in &self.layout.windows {
                if static_both.contains(&window.name) || static_height.contains(&window.name) {
                    continue; // Skip static-height widgets for now
                }
                scalable_widgets.push((
                    window.name.clone(),
                    window.row,
                    window.rows,
                    window.col,
                    window.cols,
                ));
            }

            // Process widgets in column groups (widgets whose column ranges overlap)
            while !scalable_widgets.is_empty() {
                // Take the first unprocessed widget as the anchor for this column group
                let anchor = scalable_widgets.remove(0);
                let (anchor_name, anchor_row, anchor_rows, anchor_col, anchor_cols) = anchor;

                if height_applied.contains(&anchor_name) {
                    continue;
                }

                let anchor_col_end = anchor_col + anchor_cols;
                tracing::debug!("Processing column stack anchored by '{}' (col {}-{})", anchor_name, anchor_col, anchor_col_end);

                // Find all widgets whose columns overlap with the anchor's column range
                let mut widgets_in_col = vec![(anchor_name.clone(), anchor_row, anchor_rows, anchor_cols)];

                scalable_widgets.retain(|(name, row, rows, col, cols)| {
                    let col_end = *col + *cols;
                    // Check if column ranges overlap
                    let overlaps = *col < anchor_col_end && col_end > anchor_col;

                    if overlaps && !height_applied.contains(name) {
                        tracing::debug!("  - Found overlapping widget '{}' (col {}-{})", name, col, col_end);
                        widgets_in_col.push((name.clone(), *row, *rows, *cols));
                        false // Remove from scalable_widgets
                    } else {
                        true // Keep in scalable_widgets
                    }
                });

            // Process each column group
            {
                // Sort by row (top to bottom)
                widgets_in_col.sort_by_key(|(_, row, _, _)| *row);

                // Calculate total scalable height in this column (include ALL widgets, even already-processed ones)
                let total_scalable_height: u16 = widgets_in_col.iter()
                    .map(|(_, _, rows, _)| *rows)
                    .sum();

                if total_scalable_height == 0 {
                    continue;
                }

                // Distribute height delta proportionally
                let mut adjustments: Vec<(String, i32)> = Vec::new();
                let mut leftover = 0i32;

                for (name, _row, rows, _cols) in &widgets_in_col {
                    // Calculate proportional share based on ALL widgets in column
                    let proportion = *rows as f64 / total_scalable_height as f64;
                    let share = (proportion * height_delta as f64).floor() as i32;
                    leftover += ((proportion * height_delta as f64) - share as f64).round() as i32;

                    // But only apply it if this widget hasn't been processed yet
                    if !height_applied.contains(name) {
                        adjustments.push((name.clone(), share));
                    } else {
                        // Widget already processed - discard its share to keep math consistent
                        tracing::debug!("  - Discarding share for already-processed widget '{}'", name);
                    }
                }

                // Give leftover to largest widget
                if leftover != 0 && !adjustments.is_empty() {
                    let largest_idx = widgets_in_col.iter()
                        .enumerate()
                        .filter(|(_, (name, _, _, _))| !height_applied.contains(name))
                        .max_by_key(|(_, (_, _, rows, _))| *rows)
                        .map(|(idx, _)| idx)
                        .unwrap_or(0);

                    if let Some((name, _,  _, _)) = widgets_in_col.get(largest_idx) {
                        if let Some(adj) = adjustments.iter_mut().find(|(n, _)| n == name) {
                            adj.1 += leftover;
                        }
                    }
                }

                // Apply adjustments with cascade
                let mut current_row = 0u16;
                for (idx, (name, orig_row, orig_rows, _cols)) in widgets_in_col.iter().enumerate() {
                    if height_applied.contains(name) {
                        continue;
                    }

                    let adjustment = adjustments.iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, adj)| *adj)
                        .unwrap_or(0);

                    // Get min/max constraints
                    let window = self.layout.windows.iter().find(|w| &w.name == name).unwrap();
                    let (_, min_rows) = get_widget_min_size(&window.widget_type);
                    let min_constraint = window.min_rows.unwrap_or(min_rows);
                    let max_constraint = window.max_rows;

                    // Calculate new height
                    let mut new_rows = (*orig_rows as i32 + adjustment).max(min_constraint as i32) as u16;
                    if let Some(max) = max_constraint {
                        new_rows = new_rows.min(max);
                    }

                    // Calculate new row position (cascade from previous widget)
                    let new_row = if idx == 0 {
                        *orig_row  // First widget keeps original row
                    } else {
                        current_row  // Cascade from previous widget
                    };

                    // Apply to layout
                    if let Some(window) = self.layout.windows.iter_mut().find(|w| &w.name == name) {
                        window.row = new_row;
                        window.rows = new_rows;
                        height_applied.insert(name.clone());
                    }

                    current_row = new_row + new_rows;
                }
            }
            } // End while loop

            // Static-height widgets: shift row by full height_delta
            // Exception: command_input stays anchored to bottom
            let new_height = if let Some(ref baseline) = self.baseline_layout {
                if let (Some(baseline_width), Some(baseline_height)) = (baseline.terminal_width, baseline.terminal_height) {
                    (baseline_height as i32 + height_delta).max(0) as u16
                } else {
                    // Baseline has no terminal size - shouldn't happen
                    0
                }
            } else {
                // No baseline layout - shouldn't happen
                0
            };

            for window in self.layout.windows.iter_mut() {
                if window.widget_type == "command_input" {
                    // Anchor command_input to bottom of terminal
                    let old_row = window.row;
                    window.row = new_height.saturating_sub(window.rows);
                    tracing::debug!(
                        "Command input: baseline_height={}, height_delta={:+}, new_height={}, rows={}, old_row={}, new_row={}",
                        if let Some(ref bl) = self.baseline_layout {
                            bl.terminal_height.unwrap_or(0)
                        } else { 0 },
                        height_delta,
                        new_height,
                        window.rows,
                        old_row,
                        window.row
                    );
                } else if static_both.contains(&window.name) || static_height.contains(&window.name) {
                    window.row = (window.row as i32 + height_delta).max(0) as u16;
                }
            }
        }

        // WIDTH PROCESSING: Process row-by-row (top to bottom)
        if width_delta != 0 {
            tracing::debug!("--- WIDTH SCALING ---");

            // Track which widgets have already received width adjustment
            let mut width_applied = HashSet::new();

            // Find max row to iterate through
            let max_row = self.layout.windows.iter()
                .map(|w| w.row + w.rows)
                .max()
                .unwrap_or(0);

            // Process each row
            for current_row in 0..max_row {
                // Find all widgets at this row (excluding already-processed widgets)
                let mut widgets_at_row: Vec<(String, String, u16, u16, u16, u16)> = Vec::new();
                for window in &self.layout.windows {
                    // Skip widgets that have already been resized in a previous row
                    if width_applied.contains(&window.name) {
                        continue;
                    }

                    let widget_row_start = window.row;
                    let widget_row_end = window.row + window.rows;
                    if current_row >= widget_row_start && current_row < widget_row_end {
                        let is_static = static_both.contains(&window.name);
                        widgets_at_row.push((
                            window.name.clone(),
                            window.widget_type.clone(),
                            window.row,
                            window.col,
                            window.rows,
                            window.cols,
                        ));
                    }
                }

                if widgets_at_row.is_empty() {
                    continue;
                }

                // Sort by column (left to right)
                widgets_at_row.sort_by_key(|(_, _, _, col, _, _)| *col);

                tracing::debug!("--- Row {} ---", current_row);
                for (name, widget_type, row, col, rows, cols) in &widgets_at_row {
                    let is_static = if static_both.contains(name) { "STATIC" } else { "scalable" };
                    tracing::debug!("  {} ({}) @ col={} width={} [{}]", name, widget_type, col, cols, is_static);
                }

                // Calculate total scalable width: only count outermost (non-embedded) widgets
                // Embedded widgets (like countdown timers inside command_input) are completely ignored
                let mut total_scalable_width: u16 = 0;

                // First pass: identify all embedded widgets (widgets entirely contained within others)
                let mut embedded_widgets = HashSet::new();
                for (i, (name_i, _, _, col_i, _, cols_i)) in widgets_at_row.iter().enumerate() {
                    let col_i_end = *col_i + *cols_i;
                    for (j, (name_j, _, _, col_j, _, cols_j)) in widgets_at_row.iter().enumerate() {
                        if i == j {
                            continue;
                        }
                        let col_j_end = *col_j + *cols_j;
                        // If widget j is entirely inside widget i's horizontal space, mark it as embedded
                        if *col_j >= *col_i && col_j_end <= col_i_end {
                            embedded_widgets.insert(name_j.clone());
                            tracing::debug!("    [width calc] '{}' is embedded inside '{}', will be ignored", name_j, name_i);
                        }
                    }
                }

                // Second pass: sum widths of non-embedded, non-static widgets
                for (name, _, _, col, _, cols) in widgets_at_row.iter() {
                    if static_both.contains(name) {
                        tracing::debug!("    [width calc] '{}' @ col={} width={} - STATIC, ignored", name, col, cols);
                        continue;
                    }
                    if embedded_widgets.contains(name) {
                        tracing::debug!("    [width calc] '{}' @ col={} width={} - EMBEDDED, ignored", name, col, cols);
                        continue;
                    }
                    total_scalable_width += *cols;
                    tracing::debug!("    [width calc] '{}' @ col={} width={} - OUTERMOST, counted (total now: {})", name, col, cols, total_scalable_width);
                }

                tracing::debug!("  Total scalable width: {} (from {} outermost widgets)", total_scalable_width, widgets_at_row.iter().filter(|(n, _, _, _, _, _)| !static_both.contains(n) && !embedded_widgets.contains(n)).count());

                if total_scalable_width == 0 {
                    tracing::debug!("  No scalable widgets in this row, skipping");
                    continue;
                }

                // Distribute width delta proportionally with max_cols redistribution
                let mut adjustments: Vec<(String, i32)> = Vec::new();
                let mut leftover = 0i32;
                let mut redistribution_pool = 0i32;  // Unused delta from capped widgets

                // First pass: calculate initial proportional shares
                for (name, widget_type, _row, _col, _rows, cols) in &widgets_at_row {
                    if static_both.contains(name) {
                        continue; // Skip static widgets entirely
                    }

                    // Calculate proportional share based on ALL scalable widgets in row
                    let proportion = *cols as f64 / total_scalable_width as f64;
                    let share = (proportion * width_delta as f64).floor() as i32;
                    leftover += ((proportion * width_delta as f64) - share as f64).round() as i32;

                    // But only apply it if this widget hasn't been processed yet
                    if !width_applied.contains(name) {
                        adjustments.push((name.clone(), share));
                    } else {
                        // Widget already processed - discard its share to keep math consistent
                        tracing::debug!("  - Discarding width share for already-processed widget '{}'", name);
                    }
                }

                // Second pass: check for max_cols constraints and collect unused delta
                let mut capped_widgets = HashSet::new();
                for (name, adjustment) in &adjustments {
                    let window = self.layout.windows.iter().find(|w| &w.name == name).unwrap();
                    if let Some(max_cols) = window.max_cols {
                        let current_cols = window.cols;
                        let target_cols = (current_cols as i32 + adjustment).max(0) as u16;

                        if target_cols > max_cols {
                            // Widget would exceed max - calculate unused delta
                            let actual_adjustment = (max_cols as i32 - current_cols as i32).max(0);
                            let unused = adjustment - actual_adjustment;
                            redistribution_pool += unused;
                            capped_widgets.insert(name.clone());
                            tracing::debug!("    '{}' capped at max_cols={}, returning {} to pool", name, max_cols, unused);
                        }
                    }
                }

                // Third pass: redistribute unused delta to uncapped widgets
                if redistribution_pool > 0 && adjustments.len() > capped_widgets.len() {
                    let uncapped_total: u16 = widgets_at_row.iter()
                        .filter(|(name, _, _, _, _, _)| {
                            !static_both.contains(name)
                            && !width_applied.contains(name)
                            && !capped_widgets.contains(name)
                        })
                        .map(|(_, _, _, _, _, cols)| *cols)
                        .sum();

                    if uncapped_total > 0 {
                        tracing::debug!("    Redistributing {} to uncapped widgets (total width: {})", redistribution_pool, uncapped_total);

                        for (name, adjustment) in adjustments.iter_mut() {
                            if !capped_widgets.contains(name) {
                                let window = self.layout.windows.iter().find(|w| &w.name == name).unwrap();
                                let proportion = window.cols as f64 / uncapped_total as f64;
                                let extra_share = (proportion * redistribution_pool as f64).floor() as i32;
                                *adjustment += extra_share;
                                tracing::debug!("    '{}' receives +{} from redistribution (total: {})", name, extra_share, adjustment);
                            }
                        }
                    }
                }

                // Give leftover to largest uncapped widget
                if leftover != 0 && !adjustments.is_empty() {
                    let largest_idx = widgets_at_row.iter()
                        .enumerate()
                        .filter(|(_, (name, _, _, _, _, _))| {
                            !static_both.contains(name)
                            && !width_applied.contains(name)
                            && !capped_widgets.contains(name)
                        })
                        .max_by_key(|(_, (_, _, _, _, _, cols))| *cols)
                        .map(|(idx, _)| idx);

                    if let Some(largest_idx) = largest_idx {
                        if let Some((name, _, _, _, _, _)) = widgets_at_row.get(largest_idx) {
                            if let Some(adj) = adjustments.iter_mut().find(|(n, _)| n == name) {
                                adj.1 += leftover;
                            }
                        }
                    }
                }

                // Apply adjustments with cascade
                let mut current_col = 0u16;
                let mut previous_original_col = 0u16;
                let mut previous_original_width = 0u16;
                let mut first_widget_end = 0u16;  // Track end of first widget for overlap detection

                for (idx, (name, widget_type, _row, orig_col, _rows, orig_cols)) in widgets_at_row.iter().enumerate() {
                    // Skip static widgets entirely - they keep original position and size
                    if static_both.contains(name) {
                        // Static widget - track its position for cascade but don't modify it
                        previous_original_col = *orig_col;
                        previous_original_width = *orig_cols;
                        current_col = *orig_col + *orig_cols;
                        continue;
                    }

                    if width_applied.contains(name) {
                        // Already applied - use current width from layout
                        let current_width = self.layout.windows.iter()
                            .find(|w| &w.name == name)
                            .map(|w| w.cols)
                            .unwrap_or(*orig_cols);
                        current_col += current_width;
                        continue;
                    }

                    let adjustment = adjustments.iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, adj)| *adj)
                        .unwrap_or(0);

                    // Get min/max constraints
                    let window = self.layout.windows.iter().find(|w| &w.name == name).unwrap();
                    let (min_cols, _) = get_widget_min_size(&window.widget_type);
                    let min_constraint = window.min_cols.unwrap_or(min_cols);
                    let max_constraint = window.max_cols;

                    // Calculate new width
                    let mut new_cols = (*orig_cols as i32 + adjustment).max(min_constraint as i32) as u16;
                    if let Some(max) = max_constraint {
                        new_cols = new_cols.min(max);
                    }

                    // Track the end of the first processed widget for overlap detection
                    if idx == 0 {
                        first_widget_end = *orig_col + *orig_cols;
                    }

                    // Check if this widget was originally positioned within the first widget's space
                    // This handles cases like hands widgets inside main window
                    let overlaps_first = idx > 0 && *orig_col < first_widget_end;

                    // Also check if overlapping with immediate previous widget
                    let overlaps_previous = if idx == 0 {
                        false
                    } else {
                        *orig_col < previous_original_col + previous_original_width
                    };

                    // Calculate new column position
                    let new_col = if idx == 0 || overlaps_previous || overlaps_first {
                        *orig_col  // First or overlapping widget keeps original column
                    } else {
                        // Cascade with gap preservation
                        let original_gap = orig_col.saturating_sub(previous_original_col + previous_original_width);
                        current_col + original_gap
                    };

                    // Safety check: don't allow widgets to go off-screen or have negative width
                    // We don't have direct access to terminal width here, so we'll just ensure
                    // widgets don't have invalid positions/sizes
                    let safe_col = new_col;
                    let safe_cols = new_cols;

                    // Apply to layout
                    if let Some(window) = self.layout.windows.iter_mut().find(|w| &w.name == name) {
                        tracing::debug!("    {} resized: col {} -> {}, width {} -> {} (adjustment: {:+})",
                            name, *orig_col, safe_col, *orig_cols, safe_cols, adjustment);
                        window.col = safe_col;
                        window.cols = safe_cols;
                        width_applied.insert(name.clone());
                    }

                    previous_original_col = *orig_col;
                    previous_original_width = *orig_cols;
                    current_col = safe_col + safe_cols;
                }
            }
        }

        tracing::debug!("Proportional resize complete");
    } */

    // Validation helpers (safe, no UI side effects). Used by CLI/tests.
    pub fn set_layout_for_validation(&mut self, layout: Layout, baseline: (u16, u16)) {
        self.layout = layout.clone();
        self.baseline_layout = Some(layout);
        if let Some(ref mut bl) = self.baseline_layout {
            bl.terminal_width = Some(baseline.0);
            bl.terminal_height = Some(baseline.1);
        }
    }

    pub fn reset_layout_to_baseline(&mut self) {
        if let Some(ref bl) = self.baseline_layout {
            self.layout = bl.clone();
        }
    }

    pub fn current_layout(&self) -> &Layout {
        &self.layout
    }

    // Minimum widget sizes by type (fallback when WindowDef doesn't specify min)
    fn widget_min_size(&self, widget_type: &str) -> (u16, u16) {
        match widget_type {
            "progress" | "countdown" | "indicator" | "hands" | "hand" => (10, 1),
            "compass" => (13, 5),
            "injury_doll" => (20, 10),
            "dashboard" => (15, 3),
            "command_input" => (20, 1),
            _ => (5, 3), // text, tabbed, etc.
        }
    }

    // Height-only proportional pass extracted for readability
    fn apply_height_resize(
        &mut self,
        height_delta: i32,
        static_both: &std::collections::HashSet<String>,
        static_height: &std::collections::HashSet<String>,
    ) {
        use std::collections::HashSet;
        if height_delta == 0 { return; }

        tracing::debug!("--- HEIGHT SCALING (extracted) ---");

        let mut height_applied = HashSet::new();

        // Build list of all scalable widgets with their column ranges
        let mut scalable_widgets: Vec<(String, u16, u16, u16, u16)> = Vec::new();
        for window in &self.layout.windows {
            if static_both.contains(&window.name) || static_height.contains(&window.name) {
                continue;
            }
            scalable_widgets.push((window.name.clone(), window.row, window.rows, window.col, window.cols));
        }

        while !scalable_widgets.is_empty() {
            let anchor = scalable_widgets.remove(0);
            let (anchor_name, anchor_row, anchor_rows, anchor_col, anchor_cols) = anchor;
            if height_applied.contains(&anchor_name) { continue; }

            let anchor_col_end = anchor_col + anchor_cols;
            tracing::debug!("Processing column stack anchored by '{}' (col {}-{})", anchor_name, anchor_col, anchor_col_end);

            let mut widgets_in_col = vec![(anchor_name.clone(), anchor_row, anchor_rows, anchor_cols)];
            scalable_widgets.retain(|(name, row, rows, col, cols)| {
                let col_end = *col + *cols;
                let overlaps = *col < anchor_col_end && col_end > anchor_col;
                if overlaps && !height_applied.contains(name) {
                    widgets_in_col.push((name.clone(), *row, *rows, *cols));
                    false
                } else { true }
            });

            // Sort by row and distribute
            widgets_in_col.sort_by_key(|(_, row, _, _)| *row);
            let total_scalable_height: u16 = widgets_in_col.iter().map(|(_, _, rows, _)| *rows).sum();
            if total_scalable_height == 0 { continue; }

            let mut adjustments: Vec<(String, i32)> = Vec::new();
            let mut leftover = 0i32;
            for (name, _row, rows, _cols) in &widgets_in_col {
                let proportion = *rows as f64 / total_scalable_height as f64;
                let share = (proportion * height_delta as f64).floor() as i32;
                leftover += ((proportion * height_delta as f64) - share as f64).round() as i32;
                if !height_applied.contains(name) { adjustments.push((name.clone(), share)); }
            }
            if leftover != 0 && !adjustments.is_empty() {
                if let Some((name, _, _, _)) = widgets_in_col.iter()
                    .enumerate()
                    .filter(|(_, (n, _, _, _))| !height_applied.contains(n))
                    .max_by_key(|(_, (_, _, rows, _))| *rows)
                    .map(|(_, t)| t) {
                    if let Some(adj) = adjustments.iter_mut().find(|(n, _)| n == name) { adj.1 += leftover; }
                }
            }

            let mut current_row = 0u16;
            for (idx, (name, orig_row, orig_rows, _cols)) in widgets_in_col.iter().enumerate() {
                if height_applied.contains(name) { continue; }
                let adjustment = adjustments.iter().find(|(n, _)| n == name).map(|(_, a)| *a).unwrap_or(0);
                let window = self.layout.windows.iter().find(|w| &w.name == name).unwrap();
                let (_, min_rows) = self.widget_min_size(&window.widget_type);
                let min_constraint = window.min_rows.unwrap_or(min_rows);
                let max_constraint = window.max_rows;
                let mut new_rows = (*orig_rows as i32 + adjustment).max(min_constraint as i32) as u16;
                if let Some(max) = max_constraint { new_rows = new_rows.min(max); }
                let new_row = if idx == 0 { *orig_row } else { current_row };
                if let Some(w) = self.layout.windows.iter_mut().find(|w| &w.name == name) {
                    w.row = new_row; w.rows = new_rows; height_applied.insert(name.clone());
                }
                current_row = new_row + new_rows;
            }
        }

        // Anchor command_input to bottom; build continuous top stack of statics (rows starting at 0, contiguous, overlapping horizontally)
        let new_height = if let Some(ref baseline) = self.baseline_layout {
            if let (Some(_bw), Some(bh)) = (baseline.terminal_width, baseline.terminal_height) { (bh as i32 + height_delta).max(0) as u16 } else { 0 }
        } else { 0 };

        // Snapshot baseline rows to avoid mixing updates while computing the stack
        let baseline_rows: Vec<u16> = self.layout.windows.iter().map(|w| w.row).collect();

        // Collect static-height windows by baseline row (exclude command_input)
        use std::collections::BTreeMap;
        let mut statics_by_row: BTreeMap<u16, Vec<(u16, u16, usize)>> = BTreeMap::new();
        for (i, w) in self.layout.windows.iter().enumerate() {
            if w.widget_type == "command_input" { continue; }
            if static_both.contains(&w.name) || static_height.contains(&w.name) {
                let start = w.col;
                let end = w.col.saturating_add(w.cols);
                statics_by_row.entry(baseline_rows[i]).or_default().push((start, end, i));
            }
        }

        // Build the top stack: start with row 0 statics; each next row keeps only statics overlapping with previous row's stack spans; stop on gaps
        use std::collections::HashSet as _HashSetAlias; // avoid shadowing
        let mut stack_indices: _HashSetAlias<usize> = _HashSetAlias::new();
        let prev_spans: Vec<(u16, u16, usize)> = statics_by_row.get(&0).cloned().unwrap_or_default();
        for (_, i) in prev_spans.iter().map(|(_, _, idx)| ((), *idx)) { stack_indices.insert(i); }

        if !prev_spans.is_empty() {
            for (_row, _spans) in statics_by_row.iter().filter(|(r, _)| **r > 0) {
                // Only allow contiguous rows: if this row is not exactly prev_row + 1, break the chain
                let _prev_row = prev_spans.first().map(|_| prev_spans[0]).map(|_| () );
                // Compute expected next row as last processed row + 1 by tracking last_row separately
            }
        }

        // Implement contiguous rows with tracking
        let mut current_row_opt = Some(0u16);
        let mut last_spans = prev_spans;
        while let Some(current_row) = current_row_opt {
            let next_row = current_row.saturating_add(1);
            if let Some(candidates) = statics_by_row.get(&next_row) {
                let mut next_spans: Vec<(u16, u16, usize)> = Vec::new();
                for (s, e, idx) in candidates.iter().copied() {
                    // overlap with any last_spans
                    let overlaps = last_spans.iter().any(|(ps, pe, _)| s < *pe && e > *ps);
                    if overlaps {
                        next_spans.push((s, e, idx));
                        stack_indices.insert(idx);
                    }
                }
                if next_spans.is_empty() {
                    break;
                } else {
                    last_spans = next_spans;
                    current_row_opt = Some(next_row);
                }
            } else {
                break;
            }
        }

        // Apply anchoring: command_input to bottom; top-stack statics remain at their baseline rows; others shift by delta
        for (i, window) in self.layout.windows.iter_mut().enumerate() {
            if window.widget_type == "command_input" {
                let old_row = window.row;
                window.row = new_height.saturating_sub(window.rows);
                tracing::debug!("Command input anchored: old_row={}, new_row={}", old_row, window.row);
                continue;
            }
            if static_both.contains(&window.name) || static_height.contains(&window.name) {
                let baseline_row = baseline_rows[i];
                if stack_indices.contains(&i) {
                    window.row = baseline_row.min(new_height.saturating_sub(window.rows));
                } else {
                    window.row = (baseline_row as i32 + height_delta).max(0) as u16;
                }
            }
        }
    }

    // Width-only proportional pass extracted for readability
    fn apply_width_resize(
        &mut self,
        width_delta: i32,
        static_both: &std::collections::HashSet<String>,
    ) {
        use std::collections::HashSet;
        if width_delta == 0 { return; }

        tracing::debug!("--- WIDTH SCALING (extracted) ---");

        let mut width_applied = HashSet::new();
        let max_row = self.layout.windows.iter().map(|w| w.row + w.rows).max().unwrap_or(0);
        for current_row in 0..max_row {
            let mut widgets_at_row: Vec<(String, String, u16, u16, u16, u16)> = Vec::new();
            for window in &self.layout.windows {
                if width_applied.contains(&window.name) { continue; }
                if current_row >= window.row && current_row < window.row + window.rows {
                    widgets_at_row.push((window.name.clone(), window.widget_type.clone(), window.row, window.col, window.rows, window.cols));
                }
            }
            if widgets_at_row.is_empty() { continue; }
            widgets_at_row.sort_by_key(|(_, _, _, col, _, _)| *col);

            let mut total_scalable_width: u16 = 0;
            let mut embedded_widgets = HashSet::new();
            for (i, (_name_i, _, _, col_i, _, cols_i)) in widgets_at_row.iter().enumerate() {
                let col_i_end = *col_i + *cols_i;
                for (j, (name_j, _, _, col_j, _, cols_j)) in widgets_at_row.iter().enumerate() {
                    if i == j { continue; }
                    let col_j_end = *col_j + *cols_j;
                    if *col_j >= *col_i && col_j_end <= col_i_end { embedded_widgets.insert(name_j.clone()); }
                }
            }
            for (name, _, _, _col, _, cols) in widgets_at_row.iter() {
                if static_both.contains(name) || embedded_widgets.contains(name) { continue; }
                total_scalable_width += *cols;
            }
            if total_scalable_width == 0 { continue; }

            let mut adjustments: Vec<(String, i32)> = Vec::new();
            let mut _leftover = 0i32;
            let mut redistribution_pool = 0i32;
            for (name, _wt, _row, _col, _rows, cols) in &widgets_at_row {
                if static_both.contains(name) { continue; }
                let proportion = *cols as f64 / total_scalable_width as f64;
                let share = (proportion * width_delta as f64).floor() as i32;
                _leftover += ((proportion * width_delta as f64) - share as f64).round() as i32;
                if !width_applied.contains(name) { adjustments.push((name.clone(), share)); }
            }

            let mut capped_widgets = HashSet::new();
            for (name, adjustment) in &adjustments {
                let window = self.layout.windows.iter().find(|w| &w.name == name).unwrap();
                if let Some(max_cols) = window.max_cols {
                    let current_cols = window.cols;
                    let target_cols = (current_cols as i32 + adjustment).max(0) as u16;
                    if target_cols > max_cols {
                        let actual_adjustment = (max_cols as i32 - current_cols as i32).max(0);
                        let unused = adjustment - actual_adjustment;
                        redistribution_pool += unused;
                        capped_widgets.insert(name.clone());
                    }
                }
            }
            if redistribution_pool != 0 {
                let recipients: Vec<_> = adjustments.iter().map(|(n, _)| n.clone()).filter(|n| !capped_widgets.contains(n)).collect();
                let recip_count = recipients.len() as i32;
                if recip_count > 0 {
                    let each = redistribution_pool / recip_count;
                    let mut remainder = redistribution_pool % recip_count;
                    for (name, adj) in &mut adjustments {
                        if !capped_widgets.contains(name) {
                            *adj += each;
                            if remainder != 0 { *adj += remainder.signum(); remainder -= remainder.signum(); }
                        }
                    }
                }
            }

            let mut previous_original_col = 0u16;
            let mut previous_original_width = 0u16;
            let mut current_col = 0u16;
            let mut first_widget_end = 0u16;
            for (idx, (name, _wt, _row, orig_col, _rows, orig_cols)) in widgets_at_row.iter().enumerate() {
                if static_both.contains(name) {
                    previous_original_col = *orig_col; previous_original_width = *orig_cols; current_col = *orig_col + *orig_cols; continue;
                }
                if width_applied.contains(name) {
                    let current_width = self.layout.windows.iter().find(|w| &w.name == name).map(|w| w.cols).unwrap_or(*orig_cols);
                    current_col += current_width; continue;
                }
                let adjustment = adjustments.iter().find(|(n, _)| n == name).map(|(_, a)| *a).unwrap_or(0);
                let window = self.layout.windows.iter().find(|w| &w.name == name).unwrap();
                let (min_cols, _) = self.widget_min_size(&window.widget_type);
                let min_constraint = window.min_cols.unwrap_or(min_cols);
                let max_constraint = window.max_cols;
                let mut new_cols = (*orig_cols as i32 + adjustment).max(min_constraint as i32) as u16;
                if let Some(max) = max_constraint { new_cols = new_cols.min(max); }
                if idx == 0 { first_widget_end = *orig_col + *orig_cols; }
                let overlaps_first = idx > 0 && *orig_col < first_widget_end;
                let overlaps_previous = if idx == 0 { false } else { *orig_col < previous_original_col + previous_original_width };
                let new_col = if idx == 0 || overlaps_previous || overlaps_first { *orig_col } else { let original_gap = orig_col.saturating_sub(previous_original_col + previous_original_width); current_col + original_gap };
                if let Some(w) = self.layout.windows.iter_mut().find(|w| &w.name == name) { w.col = new_col; w.cols = new_cols; width_applied.insert(name.clone()); }
                previous_original_col = *orig_col; previous_original_width = *orig_cols; current_col = new_col + new_cols;
            }
        }

        // No left/right anchoring for now (reverted per request). Width behavior unchanged.
    }

    /// New wrapper that delegates to extracted height/width passes
    pub fn apply_proportional_resize2(&mut self, width_delta: i32, height_delta: i32) {
        use std::collections::HashSet;
        tracing::debug!("=== PROPORTIONAL RESIZE (v2) ===");
        tracing::debug!("Width delta: {:+}, Height delta: {:+}", width_delta, height_delta);
        let mut static_both = HashSet::new();
        let mut static_height = HashSet::new();
        for window in &self.layout.windows {
            match window.widget_type.as_str() {
                "compass" | "injury_doll" | "dashboard" | "indicator" => { static_both.insert(window.name.clone()); }
                "progress" | "countdown" | "hands" | "hand" | "lefthand" | "righthand" | "spellhand" | "command_input" => { static_height.insert(window.name.clone()); }
                _ => {}
            }
        }
        self.apply_height_resize(height_delta, &static_both, &static_height);
        self.apply_width_resize(width_delta, &static_both);
        tracing::debug!("Proportional resize complete (v2)");
    }

    /// Update window manager configs from current config
    fn update_window_manager_config(&mut self) {
        let countdown_icon = Some(self.config.ui.countdown_icon.clone());
        let window_configs: Vec<WindowConfig> = self.layout
            .windows
            .iter()
            .map(|w| WindowConfig {
                name: w.name.clone(),
                widget_type: w.widget_type.clone(),
                streams: w.streams.clone(),
                row: w.row,
                col: w.col,
                rows: w.rows,
                cols: w.cols,
                buffer_size: w.buffer_size,
                show_border: w.show_border,
                border_style: w.border_style.clone(),
                border_color: w.border_color.clone(),
                border_sides: w.border_sides.clone(),
                title: w.title.clone(),
                content_align: w.content_align.clone(),
                background_color: w.background_color.clone(),
                bar_color: w.bar_color.clone(),
                bar_background_color: w.bar_background_color.clone(),
                text_color: w.text_color.clone(),
                transparent_background: w.transparent_background,
                countdown_icon: countdown_icon.clone(),
                indicator_colors: w.indicator_colors.clone(),
                dashboard_layout: w.dashboard_layout.clone(),
                dashboard_indicators: w.dashboard_indicators.clone(),
                dashboard_spacing: w.dashboard_spacing,
                dashboard_hide_inactive: w.dashboard_hide_inactive,
                visible_count: w.visible_count,
                effect_category: w.effect_category.clone(),
                tabs: w.tabs.clone(),
                tab_bar_position: w.tab_bar_position.clone(),
                tab_active_color: w.tab_active_color.clone(),
                tab_inactive_color: w.tab_inactive_color.clone(),
                tab_unread_color: w.tab_unread_color.clone(),
                tab_unread_prefix: w.tab_unread_prefix.clone(),
                hand_icon: w.hand_icon.clone(),
                compass_active_color: w.compass_active_color.clone(),
                compass_inactive_color: w.compass_inactive_color.clone(),
                numbers_only: Some(w.numbers_only),
                injury_default_color: w.injury_default_color.clone(),
                injury1_color: w.injury1_color.clone(),
                injury2_color: w.injury2_color.clone(),
                injury3_color: w.injury3_color.clone(),
                scar1_color: w.scar1_color.clone(),
                scar2_color: w.scar2_color.clone(),
                scar3_color: w.scar3_color.clone(),
            })
            .collect();

        self.window_manager.update_config(window_configs);
    }

    /// Update command input config from current layout
    fn update_command_input_config(&mut self) {
        // Find command_input from windows array
        if let Some(cmd_window) = self.layout.windows.iter()
            .find(|w| w.widget_type == "command_input") {

            self.command_input.set_border_config(
                cmd_window.show_border,
                cmd_window.border_style.clone(),
                cmd_window.border_color.clone(),
            );

            if let Some(ref title) = cmd_window.title {
                self.command_input.set_title(title.clone());
            } else {
                self.command_input.set_title(String::new());
            }

            self.command_input.set_background_color(cmd_window.background_color.clone());

            tracing::debug!("Updated command_input config from layout");
        } else {
            tracing::warn!("No command_input found in layout windows");
        }
    }

    /// Resize a window based on mouse drag (independent - no adjacent window adjustment)
    fn resize_window(&mut self, window_index: usize, edge: ResizeEdge, delta_rows: i16, delta_cols: i16) {
        let window_names = self.window_manager.get_window_names();
        if window_index >= window_names.len() {
            return;
        }

        let window_name = window_names[window_index].clone();

        // Get terminal size for bounds checking
        let (term_width, term_height) = if let Ok(size) = crossterm::terminal::size() {
            (size.0, size.1)
        } else {
            return; // Can't get terminal size, skip resize
        };

        // Find and update only this window - other windows stay independent
        for window_def in &mut self.layout.windows {
            if window_def.name == window_name {
                match edge {
                    ResizeEdge::Top => {
                        // Moving top edge: adjust position and height
                        let mut new_row = (window_def.row as i16 + delta_rows).max(0) as u16;
                        let row_change = new_row as i16 - window_def.row as i16;
                        let new_rows_raw = (window_def.rows as i16 - row_change).max(1) as u16;

                        // Terminal-bound max rows given new_row
                        let term_max_rows = term_height.saturating_sub(new_row);

                        // Apply WindowDef min/max constraints (fall back to terminal bounds)
                        let min_rows = window_def.min_rows.unwrap_or(1);
                        let max_rows_cfg = window_def.max_rows.unwrap_or(u16::MAX);
                        let max_rows_allowed = term_max_rows.min(max_rows_cfg);

                        // Clamp rows within [min_rows, max_rows_allowed], but if min_rows > max_allowed, cap at max_allowed
                        let mut clamped_rows = new_rows_raw;
                        if max_rows_allowed >= min_rows {
                            clamped_rows = clamped_rows.clamp(min_rows, max_rows_allowed);
                        } else {
                            clamped_rows = max_rows_allowed;
                        }

                        // Keep bottom edge fixed when clamping size
                        let bottom = window_def.row.saturating_add(window_def.rows);
                        let adjusted_row = bottom.saturating_sub(clamped_rows);
                        // Ensure adjusted_row is within screen
                        new_row = adjusted_row.min(term_height.saturating_sub(1));

                        debug!(
                            "Resizing {} top: row {} -> {}, rows {} -> {} (term_max: {}, cfg_min: {:?}, cfg_max: {:?})",
                            window_name, window_def.row, new_row, window_def.rows, clamped_rows, term_max_rows, window_def.min_rows, window_def.max_rows
                        );
                        window_def.row = new_row;
                        window_def.rows = clamped_rows;
                    }
                    ResizeEdge::Bottom => {
                        let new_rows_raw = (window_def.rows as i16 + delta_rows).max(1) as u16;

                        // Terminal-bound max rows at current row
                        let term_max_rows = term_height.saturating_sub(window_def.row);

                        // Apply WindowDef min/max constraints
                        let min_rows = window_def.min_rows.unwrap_or(1);
                        let max_rows_cfg = window_def.max_rows.unwrap_or(u16::MAX);
                        let max_rows_allowed = term_max_rows.min(max_rows_cfg);

                        let clamped_rows = if max_rows_allowed >= min_rows {
                            new_rows_raw.clamp(min_rows, max_rows_allowed)
                        } else {
                            max_rows_allowed
                        };

                        debug!(
                            "Resizing {} bottom: {} -> {} rows (term_max: {}, cfg_min: {:?}, cfg_max: {:?})",
                            window_name, window_def.rows, clamped_rows, term_max_rows, window_def.min_rows, window_def.max_rows
                        );
                        window_def.rows = clamped_rows;
                    }
                    ResizeEdge::Left => {
                        // Moving left edge: adjust position and width
                        let mut new_col = (window_def.col as i16 + delta_cols).max(0) as u16;
                        let col_change = new_col as i16 - window_def.col as i16;
                        let new_cols_raw = (window_def.cols as i16 - col_change).max(1) as u16;

                        // Terminal-bound max cols given new_col
                        let term_max_cols = term_width.saturating_sub(new_col);

                        // Apply WindowDef min/max constraints
                        let min_cols = window_def.min_cols.unwrap_or(1);
                        let max_cols_cfg = window_def.max_cols.unwrap_or(u16::MAX);
                        let max_cols_allowed = term_max_cols.min(max_cols_cfg);

                        let clamped_cols = if max_cols_allowed >= min_cols {
                            new_cols_raw.clamp(min_cols, max_cols_allowed)
                        } else {
                            max_cols_allowed
                        };

                        // Keep right edge fixed when clamping size
                        let right = window_def.col.saturating_add(window_def.cols);
                        let adjusted_col = right.saturating_sub(clamped_cols);
                        new_col = adjusted_col.min(term_width.saturating_sub(1));

                        debug!(
                            "Resizing {} left: col {} -> {}, cols {} -> {} (term_max: {}, cfg_min: {:?}, cfg_max: {:?})",
                            window_name, window_def.col, new_col, window_def.cols, clamped_cols, term_max_cols, window_def.min_cols, window_def.max_cols
                        );
                        window_def.col = new_col;
                        window_def.cols = clamped_cols;
                    }
                    ResizeEdge::Right => {
                        let new_cols_raw = (window_def.cols as i16 + delta_cols).max(1) as u16;

                        // Terminal-bound max cols at current col
                        let term_max_cols = term_width.saturating_sub(window_def.col);

                        // Apply WindowDef min/max constraints
                        let min_cols = window_def.min_cols.unwrap_or(1);
                        let max_cols_cfg = window_def.max_cols.unwrap_or(u16::MAX);
                        let max_cols_allowed = term_max_cols.min(max_cols_cfg);

                        let clamped_cols = if max_cols_allowed >= min_cols {
                            new_cols_raw.clamp(min_cols, max_cols_allowed)
                        } else {
                            max_cols_allowed
                        };

                        debug!(
                            "Resizing {} right: {} -> {} cols (term_max: {}, cfg_min: {:?}, cfg_max: {:?})",
                            window_name, window_def.cols, clamped_cols, term_max_cols, window_def.min_cols, window_def.max_cols
                        );
                        window_def.cols = clamped_cols;
                    }
                }
                break;
            }
        }

        // Update the window manager with new config
        self.update_window_manager_config();
    }

    fn move_window(&mut self, window_index: usize, delta_cols: i16, delta_rows: i16) {
        let window_names = self.window_manager.get_window_names();
        if window_index >= window_names.len() {
            return;
        }

        let window_name = window_names[window_index].clone();

        // Get terminal size for bounds checking
        let (term_width, term_height) = if let Ok(size) = crossterm::terminal::size() {
            (size.0, size.1)
        } else {
            return; // Can't get terminal size, skip move
        };

        // Find and update only this window's position
        for window_def in &mut self.layout.windows {
            if window_def.name == window_name {
                // Update position, ensuring we don't go negative or beyond terminal bounds
                let new_row = (window_def.row as i16 + delta_rows).max(0) as u16;
                let new_col = (window_def.col as i16 + delta_cols).max(0) as u16;

                // Ensure the window doesn't go outside terminal bounds
                // Keep at least 1 row/col visible
                let max_row = term_height.saturating_sub(window_def.rows).max(0);
                let max_col = term_width.saturating_sub(window_def.cols).max(0);

                let bounded_row = new_row.min(max_row);
                let bounded_col = new_col.min(max_col);

                debug!("Moving {}: row {} -> {} (max: {}), col {} -> {} (max: {})",
                    window_name, window_def.row, bounded_row, max_row, window_def.col, bounded_col, max_col);

                window_def.row = bounded_row;
                window_def.col = bounded_col;
                break;
            }
        }

        // Update the window manager with new config
        self.update_window_manager_config();
    }

    /// Handle local dot commands
    fn handle_dot_command(&mut self, command: &str, command_tx: Option<&mpsc::UnboundedSender<String>>) {
        let parts: Vec<&str> = command[1..].split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "menu" => {
                self.open_main_menu();
            }
            "quit" | "q" => {
                self.running = false;
            }
            "savelayout" => {
                let name = parts.get(1).unwrap_or(&"default");
                let terminal_size = crossterm::terminal::size().ok();
                // Don't force terminal size for manual saves (preserve baseline)
                match self.layout.save(name, terminal_size, false) {
                    Ok(_) => self.add_system_message(&format!("Layout saved as '{}'", name)),
                    Err(e) => self.add_system_message(&format!("Failed to save layout: {}", e)),
                }
            }
            "loadlayout" => {
                let name = parts.get(1).unwrap_or(&"default");
                let layout_path = match Config::layout_path(name) {
                    Ok(path) => path,
                    Err(e) => {
                        self.add_system_message(&format!("Failed to get layout path: {}", e));
                        return;
                    }
                };
                match Layout::load_from_file(&layout_path) {
                    Ok(new_layout) => {
                        self.layout = new_layout.clone();
                        self.baseline_layout = Some(new_layout);  // Store as new baseline
                        self.add_system_message(&format!("Layout '{}' loaded", name));
                        self.update_window_manager_config();
                        self.update_command_input_config();

                        // Log if layout has terminal size info
                        if let (Some(w), Some(h)) = (self.layout.terminal_width, self.layout.terminal_height) {
                            tracing::info!("Loaded layout with designed terminal size: {}x{}", w, h);
                        } else {
                            tracing::warn!("Layout has no terminal size - .resize will use .baseline snapshot if set");
                        }
                    }
                    Err(e) => self.add_system_message(&format!("Failed to load layout: {}", e)),
                }
            }
            "layouts" => {
                match Config::list_layouts() {
                    Ok(layouts) => {
                        if layouts.is_empty() {
                            self.add_system_message("No saved layouts");
                        } else {
                            self.add_system_message(&format!("Saved layouts: {}", layouts.join(", ")));
                        }
                    }
                    Err(e) => self.add_system_message(&format!("Failed to list layouts: {}", e)),
                }
            }
            "createwindow" | "createwin" => {
                if parts.len() < 2 {
                    let templates = Config::available_window_templates();
                    self.add_system_message(&format!("Usage: .createwindow <name>"));
                    self.add_system_message(&format!("Available: {}", templates.join(", ")));
                    return;
                }

                let window_name = parts[1];

                // Check if window already exists
                if self.layout.windows.iter().any(|w| w.name == window_name) {
                    self.add_system_message(&format!("Window '{}' already exists", window_name));
                    return;
                }

                // Get template
                if let Some(window_def) = Config::get_window_template(window_name) {
                    // Prevent creating duplicate command_input
                    if window_def.widget_type == "command_input" {
                        if self.layout.windows.iter().any(|w| w.widget_type == "command_input") {
                            self.add_system_message("Cannot create duplicate command_input - one already exists");
                            return;
                        }
                    }

                    let actual_name = window_def.name.clone();
                    self.layout.windows.push(window_def);
                    self.update_window_manager_config();

                    // If template name differs from actual window name, inform user
                    if actual_name != window_name {
                        self.add_system_message(&format!("Created window '{}' (use name '{}' for commands)", window_name, actual_name));
                    } else {
                        self.add_system_message(&format!("Created window '{}' - use mouse to move/resize", window_name));
                    }
                } else {
                    let templates = Config::available_window_templates();
                    self.add_system_message(&format!("Unknown window type: {}", window_name));
                    self.add_system_message(&format!("Available: {}", templates.join(", ")));
                }
            }
            "customwindow" | "customwin" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .customwindow <name> <stream1,stream2,...>");
                    self.add_system_message("Example: .customwindow combat combat,death");
                    self.add_system_message("Creates a custom window with specified streams");
                    return;
                }

                let window_name = parts[1];
                let streams_str = parts[2];

                // Check if window already exists
                if self.layout.windows.iter().any(|w| w.name == window_name) {
                    self.add_system_message(&format!("Window '{}' already exists", window_name));
                    return;
                }

                // Parse comma-separated streams
                let streams: Vec<String> = streams_str.split(',').map(|s| s.trim().to_string()).collect();

                if streams.is_empty() {
                    self.add_system_message("Error: At least one stream required");
                    return;
                }

                // Create custom window
                use crate::config::WindowDef;
                let window_def = WindowDef {
                    name: window_name.to_string(),
                    widget_type: "text".to_string(),
                    streams,
                    row: 0,
                    col: 0,
                    rows: 10,
                    cols: 40,
                    buffer_size: 1000,
                    show_border: true,
                    border_style: Some("single".to_string()),
                    border_color: None,
                    border_sides: None,
                    title: Some(window_name.to_string()),
                    content_align: None,
            background_color: None,
                    bar_color: None,
                    bar_background_color: None,
                    text_color: None,
                    transparent_background: true,
                    locked: false,
                    indicator_colors: None,
                    dashboard_layout: None,
                    dashboard_indicators: None,
                    dashboard_spacing: None,
                    dashboard_hide_inactive: None,
                    visible_count: None,
                    effect_category: None,
                    tabs: None,
                    tab_bar_position: None,
                    tab_active_color: None,
                    tab_inactive_color: None,
                    tab_unread_color: None,
                    tab_unread_prefix: None,
                    hand_icon: None,
                    countdown_icon: None,
                    compass_active_color: None,
                    compass_inactive_color: None,
                    min_rows: None,
                    max_rows: None,
                    min_cols: None,
                    max_cols: None,
                    numbers_only: false,
                    progress_id: None,
                    countdown_id: None,
                    effect_default_color: None,
                    ..Default::default()
                };

                self.layout.windows.push(window_def);
                self.update_window_manager_config();
                self.add_system_message(&format!("Created custom window '{}' - use mouse to move/resize", window_name));
            }
            "createtabbed" | "tabbedwindow" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .createtabbed <name> <tab1:stream1,tab2:stream2,...>");
                    self.add_system_message("Example: .createtabbed chat Speech:speech,Thoughts:thoughts,Whisper:whisper");
                    self.add_system_message("Creates a tabbed window with specified tabs");
                    return;
                }

                let window_name = parts[1];
                let tabs_str = parts[2];

                // Check if window already exists
                if self.layout.windows.iter().any(|w| w.name == window_name) {
                    self.add_system_message(&format!("Window '{}' already exists", window_name));
                    return;
                }

                // Parse tab definitions: "TabName:stream,TabName2:stream2"
                use crate::config::{WindowDef, TabConfig};
                let mut tabs = Vec::new();
                for tab_def in tabs_str.split(',') {
                    let tab_parts: Vec<&str> = tab_def.split(':').collect();
                    if tab_parts.len() != 2 {
                        self.add_system_message(&format!("Invalid tab format: '{}' (expected name:stream)", tab_def));
                        return;
                    }
                    tabs.push(TabConfig {
                        name: tab_parts[0].trim().to_string(),
                        stream: tab_parts[1].trim().to_string(),
                    });
                }

                if tabs.is_empty() {
                    self.add_system_message("Error: At least one tab required");
                    return;
                }

                let window_def = WindowDef {
                    name: window_name.to_string(),
                    widget_type: "tabbed".to_string(),
                    streams: vec![],  // Tabs handle their own streams
                    row: 0,
                    col: 0,
                    rows: 20,
                    cols: 60,
                    buffer_size: 5000,
                    show_border: true,
                    border_style: Some("rounded".to_string()),
                    border_color: None,
                    border_sides: None,
                    title: Some(window_name.to_string()),
                    content_align: None,
            background_color: None,
                    bar_color: None,
                    bar_background_color: None,
                    text_color: None,
                    transparent_background: true,
                    locked: false,
                    indicator_colors: None,
                    dashboard_layout: None,
                    dashboard_indicators: None,
                    dashboard_spacing: None,
                    dashboard_hide_inactive: None,
                    visible_count: None,
                    effect_category: None,
                    tabs: Some(tabs.clone()),
                    tab_bar_position: Some("top".to_string()),
                    tab_active_color: Some("#ffff00".to_string()),
                    tab_inactive_color: Some("#808080".to_string()),
                    tab_unread_color: Some("#ffffff".to_string()),
                    tab_unread_prefix: Some("* ".to_string()),
                    hand_icon: None,
                    countdown_icon: None,
                    compass_active_color: None,
                    compass_inactive_color: None,
                    min_rows: None,
                    max_rows: None,
                    min_cols: None,
                    max_cols: None,
                    numbers_only: false,
                    progress_id: None,
                    countdown_id: None,
                    effect_default_color: None,
                    ..Default::default()
                };

                self.layout.windows.push(window_def);
                self.update_window_manager_config();

                let tab_names: Vec<String> = tabs.iter().map(|t| t.name.clone()).collect();
                self.add_system_message(&format!("Created tabbed window '{}' with tabs: {}", window_name, tab_names.join(", ")));
                self.add_system_message("Use mouse to move/resize, click tabs to switch");
            }
            "addtab" => {
                if parts.len() < 4 {
                    self.add_system_message("Usage: .addtab <window> <tab_name> <stream>");
                    self.add_system_message("Example: .addtab chat LNet logons");
                    return;
                }

                let window_name = parts[1];
                let tab_name = parts[2];
                let stream_name = parts[3];

                // Find the window
                if let Some(window_def) = self.layout.windows.iter_mut().find(|w| w.name == window_name) {
                    if window_def.widget_type != "tabbed" {
                        self.add_system_message(&format!("Window '{}' is not a tabbed window", window_name));
                        return;
                    }

                    // Initialize tabs vec if needed
                    if window_def.tabs.is_none() {
                        window_def.tabs = Some(Vec::new());
                    }

                    // Check if tab already exists
                    if let Some(ref tabs) = window_def.tabs {
                        if tabs.iter().any(|t| t.name == tab_name) {
                            self.add_system_message(&format!("Tab '{}' already exists in window '{}'", tab_name, window_name));
                            return;
                        }
                    }

                    // Add the tab
                    use crate::config::TabConfig;
                    if let Some(ref mut tabs) = window_def.tabs {
                        tabs.push(TabConfig {
                            name: tab_name.to_string(),
                            stream: stream_name.to_string(),
                        });
                    }

                    self.update_window_manager_config();
                    self.add_system_message(&format!("Added tab '{}' (stream: {}) to window '{}'", tab_name, stream_name, window_name));
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "removetab" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .removetab <window> <tab_name>");
                    self.add_system_message("Example: .removetab chat LNet");
                    return;
                }

                let window_name = parts[1];
                let tab_name = parts[2];

                // Find the window
                if let Some(window_def) = self.layout.windows.iter_mut().find(|w| w.name == window_name) {
                    if window_def.widget_type != "tabbed" {
                        self.add_system_message(&format!("Window '{}' is not a tabbed window", window_name));
                        return;
                    }

                    if let Some(ref mut tabs) = window_def.tabs {
                        let initial_len = tabs.len();
                        if initial_len <= 1 {
                            self.add_system_message("Cannot remove last tab from window");
                            return;
                        }

                        tabs.retain(|t| t.name != tab_name);

                        if tabs.len() < initial_len {
                            self.update_window_manager_config();
                            self.add_system_message(&format!("Removed tab '{}' from window '{}'", tab_name, window_name));
                        } else {
                            self.add_system_message(&format!("Tab '{}' not found in window '{}'", tab_name, window_name));
                        }
                    } else {
                        self.add_system_message(&format!("Window '{}' has no tabs", window_name));
                    }
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "switchtab" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .switchtab <window> <tab_name|index>");
                    self.add_system_message("Example: .switchtab chat Speech  OR  .switchtab chat 0");
                    return;
                }

                let window_name = parts[1];
                let tab_identifier = parts[2];

                // Find the window widget
                if let Some(widget) = self.window_manager.get_window(window_name) {
                    if let Widget::Tabbed(tabbed) = widget {
                        // Try parsing as index first
                        if let Ok(index) = tab_identifier.parse::<usize>() {
                            tabbed.switch_to_tab(index);
                            self.add_system_message(&format!("Switched to tab #{} in window '{}'", index, window_name));
                        } else {
                            // Try by name
                            tabbed.switch_to_tab_by_name(tab_identifier);
                            self.add_system_message(&format!("Switched to tab '{}' in window '{}'", tab_identifier, window_name));
                        }
                    } else {
                        self.add_system_message(&format!("Window '{}' is not a tabbed window", window_name));
                    }
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "tabcolors" | "settabcolors" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .tabcolors <window> <active_color> [unread_color] [inactive_color]");
                    self.add_system_message("Example: .tabcolors chat #ffff00 #ffffff #808080");
                    self.add_system_message("Sets colors for active tab, unread tabs, and inactive tabs");
                    return;
                }

                let window_name = parts[1];
                let active_color = parts[2];
                let unread_color = parts.get(3).copied();
                let inactive_color = parts.get(4).copied();

                // Find the window config
                if let Some(window_def) = self.layout.windows.iter_mut().find(|w| w.name == window_name) {
                    if window_def.widget_type != "tabbed" {
                        self.add_system_message(&format!("Window '{}' is not a tabbed window", window_name));
                        return;
                    }

                    // Update colors in config
                    window_def.tab_active_color = Some(active_color.to_string());
                    if let Some(color) = unread_color {
                        window_def.tab_unread_color = Some(color.to_string());
                    }
                    if let Some(color) = inactive_color {
                        window_def.tab_inactive_color = Some(color.to_string());
                    }

                    // Update the widget
                    if let Some(widget) = self.window_manager.get_window(window_name) {
                        if let Widget::Tabbed(tabbed) = widget {
                            tabbed.set_tab_active_color(active_color.to_string());
                            if let Some(color) = unread_color {
                                tabbed.set_tab_unread_color(color.to_string());
                            }
                            if let Some(color) = inactive_color {
                                tabbed.set_tab_inactive_color(color.to_string());
                            }
                        }
                    }

                    let mut msg = format!("Set tab active color to {} for window '{}'", active_color, window_name);
                    if let Some(color) = unread_color {
                        msg.push_str(&format!(", unread to {}", color));
                    }
                    if let Some(color) = inactive_color {
                        msg.push_str(&format!(", inactive to {}", color));
                    }
                    self.add_system_message(&msg);
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "movetab" | "reordertab" => {
                if parts.len() < 4 {
                    self.add_system_message("Usage: .movetab <window> <tab_name> <new_position>");
                    self.add_system_message("Example: .movetab chat Speech 0");
                    self.add_system_message("Moves tab to new position (0-based index)");
                    return;
                }

                let window_name = parts[1];
                let tab_name = parts[2];
                let new_position: usize = match parts[3].parse() {
                    Ok(pos) => pos,
                    Err(_) => {
                        self.add_system_message("Error: Position must be a number");
                        return;
                    }
                };

                // Find the window config
                if let Some(window_def) = self.layout.windows.iter_mut().find(|w| w.name == window_name) {
                    if window_def.widget_type != "tabbed" {
                        self.add_system_message(&format!("Window '{}' is not a tabbed window", window_name));
                        return;
                    }

                    if let Some(ref mut tabs) = window_def.tabs {
                        // Find the tab by name
                        if let Some(current_index) = tabs.iter().position(|t| t.name == tab_name) {
                            let tab_count = tabs.len();
                            if new_position >= tab_count {
                                self.add_system_message(&format!("Error: Position {} is out of range (0-{})", new_position, tab_count - 1));
                                return;
                            }

                            // Remove tab from current position and insert at new position
                            let tab = tabs.remove(current_index);
                            tabs.insert(new_position, tab);

                            // Update window manager
                            self.update_window_manager_config();
                            self.add_system_message(&format!("Moved tab '{}' to position {} in window '{}'", tab_name, new_position, window_name));
                        } else {
                            self.add_system_message(&format!("Tab '{}' not found in window '{}'", tab_name, window_name));
                        }
                    } else {
                        self.add_system_message(&format!("Window '{}' has no tabs", window_name));
                    }
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "deletewindow" | "deletewin" => {
                if parts.len() < 2 {
                    self.add_system_message("Usage: .deletewindow <name>");
                    return;
                }

                let window_name = parts[1];

                // Prevent deletion of command_input
                if window_name == "command_input" {
                    self.add_system_message("Cannot delete command_input - it is required for the application");
                    return;
                }

                let initial_len = self.layout.windows.len();
                self.layout.windows.retain(|w| w.name != window_name);

                if self.layout.windows.len() < initial_len {
                    self.update_window_manager_config();
                    self.add_system_message(&format!("Deleted window '{}'", window_name));

                    // Adjust focused window index if needed
                    if self.focused_window_index >= self.layout.windows.len() && self.focused_window_index > 0 {
                        self.focused_window_index = self.layout.windows.len() - 1;
                    }
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "togglespellid" | "toggleeffectid" => {
                if parts.len() < 2 {
                    self.add_system_message("Usage: .togglespellid <window_name>");
                    self.add_system_message("Toggles between spell name and spell ID for active effects windows");
                    return;
                }

                let window_name = parts[1];
                if let Some(window) = self.window_manager.get_window(window_name) {
                    window.toggle_effect_display();
                    self.add_system_message(&format!("Toggled display for '{}'", window_name));
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "windows" | "listwindows" => {
                let windows: Vec<String> = self.layout.windows.iter().map(|w| w.name.clone()).collect();
                if windows.is_empty() {
                    self.add_system_message("No windows");
                } else {
                    self.add_system_message(&format!("Windows: {}", windows.join(", ")));
                }
            }
            "templates" | "availablewindows" => {
                let templates = Config::available_window_templates();
                self.add_system_message(&format!("Available window templates: {}", templates.join(", ")));
            }
            "editwindow" | "editwin" => {
                // Get list of window names
                let window_names: Vec<String> = self.layout.windows.iter().map(|w| w.name.clone()).collect();

                if window_names.is_empty() {
                    self.add_system_message("No windows to edit");
                    return;
                }

                // Optional: window name specified
                let selected_window = parts.get(1).map(|s| s.to_string());

                // Open window editor
                self.window_editor.open_for_window(window_names, selected_window);
                self.input_mode = InputMode::WindowEditor;
                self.add_system_message("Window editor opened - select window to edit");
            }
            "addwindow" | "newwindow" => {
                // Open window editor for new window
                self.window_editor.open_for_new_window();
                self.input_mode = InputMode::WindowEditor;
                self.add_system_message("Window editor opened - select widget type");
            }
            "editinput" | "editcommandbox" => {
                // Find command_input from windows array
                let window_def = self.layout.windows.iter()
                    .find(|w| w.widget_type == "command_input")
                    .cloned()
                    .expect("command_input must exist in windows array");
                self.window_editor.load_window(window_def);
                self.input_mode = InputMode::WindowEditor;
                self.add_system_message("Editing command input box");
            }
            "lockwindows" | "lockall" => {
                // Lock all windows
                let mut count = 0;
                for window in &mut self.layout.windows {
                    if !window.locked {
                        window.locked = true;
                        count += 1;
                    }
                }
                self.add_system_message(&format!("Locked {} window(s) - cannot be moved or resized", count));
                self.add_system_message("Remember to .savelayout to persist locked state!");
            }
            "unlockwindows" | "unlockall" => {
                // Unlock all windows
                let mut count = 0;
                for window in &mut self.layout.windows {
                    if window.locked {
                        window.locked = false;
                        count += 1;
                    }
                }
                self.add_system_message(&format!("Unlocked {} window(s) - can now be moved or resized", count));
                self.add_system_message("Remember to .savelayout to persist unlocked state!");
            }
            "indicatoron" => {
                // Force all status indicators on for testing
                let indicators = ["poisoned", "diseased", "bleeding", "stunned", "webbed"];
                for name in &indicators {
                    if let Some(window) = self.window_manager.get_window(name) {
                        window.set_indicator(1);
                    }
                    // Also update dashboards
                    self.window_manager.update_dashboard_indicator(name, 1);
                }
                self.add_system_message("Forced all status indicators ON");
            }
            "indicatoroff" => {
                // Force all status indicators off for testing
                let indicators = ["poisoned", "diseased", "bleeding", "stunned", "webbed"];
                for name in &indicators {
                    if let Some(window) = self.window_manager.get_window(name) {
                        window.set_indicator(0);
                    }
                    // Also update dashboards
                    self.window_manager.update_dashboard_indicator(name, 0);
                }
                self.add_system_message("Forced all status indicators OFF");
            }
            "randominjuries" | "randinjuries" => {
                // Randomly assign injuries/scars to the injury doll for testing
                let body_parts = ["head", "neck", "rightArm", "leftArm", "rightHand", "leftHand",
                                 "chest", "abdomen", "back", "rightLeg", "leftLeg", "rightEye", "leftEye"];
                let mut rng = rand::thread_rng();

                // Random number of injuries (3-8)
                let num_injuries = rng.gen_range(3..=8);

                for _ in 0..num_injuries {
                    let part = body_parts[rng.gen_range(0..body_parts.len())];
                    let is_scar = rng.gen_bool(0.3); // 30% chance of being a scar
                    // Levels 1-3 are wounds, 4-6 are scars
                    let level = if is_scar {
                        rng.gen_range(4..=6)
                    } else {
                        rng.gen_range(1..=3)
                    };

                    if let Some(window) = self.window_manager.get_window("injuries") {
                        window.set_injury(part.to_string(), level);
                    }
                }
                self.add_system_message(&format!("Randomized {} injuries/scars", num_injuries));
            }
            "randomcompass" | "randcompass" => {
                // Randomly assign compass directions for testing
                let directions = ["n", "ne", "e", "se", "s", "sw", "w", "nw", "out"];
                let mut rng = rand::thread_rng();
                let mut active_dirs = Vec::new();

                // Random number of exits (2-6)
                let num_exits = rng.gen_range(2..=6);

                for _ in 0..num_exits {
                    let dir = directions[rng.gen_range(0..directions.len())];
                    if !active_dirs.contains(&dir) {
                        active_dirs.push(dir);
                    }
                }

                if let Some(window) = self.window_manager.get_window("compass") {
                    window.set_compass_directions(active_dirs.iter().map(|s| s.to_string()).collect());
                }
                self.add_system_message(&format!("Randomized {} compass exits", active_dirs.len()));
            }
            "randomprogress" | "randprog" => {
                // Randomly set all progress bars for testing
                let mut rng = rand::thread_rng();

                // Health: max 350
                let health_max = 350;
                let health_current = rng.gen_range(50..=health_max);
                if let Some(window) = self.window_manager.get_window("health") {
                    window.set_progress(health_current, health_max);
                    debug!("Set health to {}/{}", health_current, health_max);
                } else {
                    debug!("No window found for 'health'");
                }

                // Mana: max 580
                let mana_max = 580;
                let mana_current = rng.gen_range(50..=mana_max);
                if let Some(window) = self.window_manager.get_window("mana") {
                    window.set_progress(mana_current, mana_max);
                }

                // Stamina: max 250
                let stamina_max = 250;
                let stamina_current = rng.gen_range(30..=stamina_max);
                if let Some(window) = self.window_manager.get_window("stamina") {
                    window.set_progress(stamina_current, stamina_max);
                }

                // Spirit: max 13
                let spirit_max = 13;
                let spirit_current = rng.gen_range(1..=spirit_max);
                if let Some(window) = self.window_manager.get_window("spirit") {
                    window.set_progress(spirit_current, spirit_max);
                }

                // Blood Points: max 100 (try multiple possible names)
                let blood_max = 100;
                let blood_current = rng.gen_range(0..=blood_max);
                let blood_names = ["bloodpoints", "lblBPs", "blood"];
                for name in &blood_names {
                    if let Some(window) = self.window_manager.get_window(name) {
                        window.set_progress(blood_current, blood_max);
                        break;
                    }
                }

                // Mind: max 100 (try multiple possible names)
                let mind_max = 100;
                let mind_current = rng.gen_range(20..=mind_max);
                let mind_names = ["mindstate", "mind"];
                for name in &mind_names {
                    if let Some(window) = self.window_manager.get_window(name) {
                        window.set_progress(mind_current, mind_max);
                        break;
                    }
                }

                // Encumbrance: max 100, but text shows "overloaded" not the max
                let encum_value = rng.gen_range(0..=100);
                let encum_names = ["encumlevel", "encumbrance", "encum"];
                for name in &encum_names {
                    if let Some(window) = self.window_manager.get_window(name) {
                        window.set_progress(encum_value, 100);
                        break;
                    }
                }

                // Stance: max 100, text shows stance name (defensive/guarded/neutral/forward/advance/offensive)
                let stance_value = rng.gen_range(0..=100);
                let stance_text = Self::stance_percentage_to_text(stance_value);
                let stance_names = ["stance", "pbarStance"];
                for name in &stance_names {
                    if let Some(window) = self.window_manager.get_window(name) {
                        window.set_progress_with_text(stance_value, 100, Some(stance_text.clone()));
                        break;
                    }
                }

                self.add_system_message("Randomized all progress bars");
            }
            "randomcountdowns" | "randcountdowns" => {
                // Randomly set countdown timers (15-25 seconds each)
                use std::time::{SystemTime, UNIX_EPOCH};
                let mut rng = rand::thread_rng();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                // Roundtime: 15-25 seconds
                let rt_seconds = rng.gen_range(15..=25);
                if let Some(window) = self.window_manager.get_window("roundtime") {
                    window.set_countdown(now + rt_seconds);
                }

                // Casttime: 15-25 seconds
                let cast_seconds = rng.gen_range(15..=25);
                if let Some(window) = self.window_manager.get_window("casttime") {
                    window.set_countdown(now + cast_seconds);
                }

                // Stuntime: 15-25 seconds
                let stun_seconds = rng.gen_range(15..=25);
                if let Some(window) = self.window_manager.get_window("stuntime") {
                    window.set_countdown(now + stun_seconds);
                }

                self.add_system_message(&format!("Randomized countdowns: RT={}s, Cast={}s, Stun={}s",
                    rt_seconds, cast_seconds, stun_seconds));
            }
            "rename" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .rename <window> <new title>");
                    self.add_system_message("Example: .rename loot My Loot Window");
                    return;
                }

                let window_name = parts[1];
                // Join the rest of the parts as the title (allows spaces)
                let new_title = parts[2..].join(" ");

                // Find and update the window
                let mut found = false;
                for window_def in &mut self.layout.windows {
                    if window_def.name == window_name {
                        window_def.title = Some(new_title.clone());
                        found = true;
                        break;
                    }
                }

                if found {
                    self.update_window_manager_config();
                    self.add_system_message(&format!("Renamed '{}' to '{}'", window_name, new_title));
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "border" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .border <window> <style> [color] [sides...]");
                    self.add_system_message("Styles: single, double, rounded, thick, none");
                    self.add_system_message("Sides: top, bottom, left, right, all, none (default: all)");
                    self.add_system_message("Example: .border main rounded #00ff00 top bottom");
                    return;
                }

                let window_name = parts[1];
                let style = parts[2];

                // Parse color and sides - color is a hex string starting with #
                let mut color: Option<String> = None;
                let mut sides: Vec<String> = Vec::new();

                for i in 3..parts.len() {
                    if parts[i].starts_with('#') {
                        color = Some(parts[i].to_string());
                    } else {
                        sides.push(parts[i].to_string());
                    }
                }

                let border_sides = if sides.is_empty() {
                    None  // Default to all sides
                } else {
                    Some(sides)
                };

                // Validate style
                let valid_styles = vec!["single", "double", "rounded", "thick", "none"];
                if !valid_styles.contains(&style) {
                    self.add_system_message(&format!("Invalid style: {}", style));
                    self.add_system_message("Valid styles: single, double, rounded, thick, none");
                    return;
                }

                // Find and update the window
                let mut found = false;
                for window_def in &mut self.layout.windows {
                    if window_def.name == window_name {
                        if style == "none" {
                            window_def.show_border = false;
                            window_def.border_style = None;
                        } else {
                            window_def.show_border = true;
                            window_def.border_style = Some(style.to_string());
                        }

                        if let Some(ref c) = color {
                            window_def.border_color = Some(c.clone());
                        }

                        window_def.border_sides = border_sides.clone();

                        found = true;
                        break;
                    }
                }

                if found {
                    self.update_window_manager_config();
                    let sides_str = border_sides.as_ref()
                        .map(|s| format!(" [{}]", s.join(", ")))
                        .unwrap_or_default();

                    if style == "none" {
                        self.add_system_message(&format!("Removed border from '{}'", window_name));
                    } else if let Some(ref c) = color {
                        self.add_system_message(&format!("Set '{}' border to {} ({}){}", window_name, style, c, sides_str));
                    } else {
                        self.add_system_message(&format!("Set '{}' border to {}{}", window_name, style, sides_str));
                    }
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "contentalign" | "align" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .contentalign <window> <alignment>");
                    self.add_system_message("Alignments: top-left, top-right, bottom-left, bottom-right, center");
                    self.add_system_message("Example: .contentalign compass bottom-left");
                    return;
                }

                let window_name = parts[1];
                let alignment = parts[2];

                // Validate alignment
                let valid_alignments = ["top-left", "top", "top-right", "left", "center", "right", "bottom-left", "bottom", "bottom-right"];
                if !valid_alignments.contains(&alignment) {
                    self.add_system_message(&format!("Invalid alignment '{}'. Valid: top-left, top, top-right, left, center, right, bottom-left, bottom, bottom-right", alignment));
                    return;
                }

                // Find and update the window definition
                let mut found = false;
                for window_def in &mut self.layout.windows {
                    if window_def.name == window_name {
                        window_def.content_align = Some(alignment.to_string());
                        found = true;
                        break;
                    }
                }

                if found {
                    // Update the running widget directly without recreating everything
                    if let Some(widget) = self.window_manager.get_window(window_name) {
                        widget.set_content_align(Some(alignment.to_string()));
                        self.add_system_message(&format!("Set '{}' content alignment to {}", window_name, alignment));
                    } else {
                        self.add_system_message(&format!("Window '{}' not found in window manager", window_name));
                    }
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "setprogress" | "setprog" => {
                if parts.len() < 4 {
                    self.add_system_message("Usage: .setprogress <window> <current> <max>");
                    self.add_system_message("Example: .setprogress health 150 200");
                    return;
                }

                let window_name = parts[1];
                let current = parts[2].parse::<u32>();
                let max = parts[3].parse::<u32>();

                if current.is_err() || max.is_err() {
                    self.add_system_message("Error: current and max must be numbers");
                    return;
                }

                let current = current.unwrap();
                let max = max.unwrap();

                if let Some(window) = self.window_manager.get_window(window_name) {
                    window.set_progress(current, max);
                    self.add_system_message(&format!("Set '{}' to {}/{}", window_name, current, max));
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "setcountdown" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .setcountdown <window> <seconds>");
                    self.add_system_message("Example: .setcountdown roundtime 5");
                    return;
                }

                let window_name = parts[1];
                let seconds = parts[2].parse::<u64>();

                if seconds.is_err() {
                    self.add_system_message("Error: seconds must be a number");
                    return;
                }

                let seconds = seconds.unwrap();

                // Calculate end time (current time + seconds)
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let end_time = now + seconds;

                debug!("Looking for countdown window: '{}', end_time: {}, now: {}", window_name, end_time, now);

                if let Some(window) = self.window_manager.get_window(window_name) {
                    debug!("Found window '{}', calling set_countdown", window_name);
                    window.set_countdown(end_time);
                    self.add_system_message(&format!("Set '{}' countdown to {} seconds", window_name, seconds));
                } else {
                    debug!("Window '{}' not found!", window_name);
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "setbarcolor" | "barcolor" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .setbarcolor <window> <color> [bg_color]");
                    self.add_system_message("Example: .setbarcolor health #6e0202 #2a0101");
                    self.add_system_message("Colors should be hex format: #RRGGBB");
                    return;
                }

                let window_name = parts[1];
                let bar_color = parts[2];
                let bg_color = parts.get(3).copied();

                // Validate hex color format
                if !bar_color.starts_with('#') || bar_color.len() != 7 {
                    self.add_system_message("Error: Color must be in hex format: #RRGGBB");
                    return;
                }

                if let Some(bg) = bg_color {
                    if !bg.starts_with('#') || bg.len() != 7 {
                        self.add_system_message("Error: Background color must be in hex format: #RRGGBB");
                        return;
                    }
                }

                // Update the config
                let mut found = false;
                for window_def in &mut self.layout.windows {
                    if window_def.name == window_name {
                        window_def.bar_color = Some(bar_color.to_string());
                        window_def.bar_background_color = bg_color.map(|s| s.to_string());
                        found = true;
                        break;
                    }
                }

                if found {
                    // Update the actual widget's colors immediately
                    if let Some(window) = self.window_manager.get_window(window_name) {
                        window.set_bar_colors(Some(bar_color.to_string()), bg_color.map(|s| s.to_string()));
                        if let Some(bg) = bg_color {
                            self.add_system_message(&format!("Set '{}' colors to {} / {}", window_name, bar_color, bg));
                        } else {
                            self.add_system_message(&format!("Set '{}' bar color to {}", window_name, bar_color));
                        }
                    } else {
                        self.add_system_message(&format!("Window '{}' not found in manager", window_name));
                    }
                } else {
                    self.add_system_message(&format!("Window '{}' not found in config", window_name));
                }
            }
            "addhighlight" | "addhl" => {
                // Open highlight form in Create mode
                let form = crate::ui::HighlightFormWidget::new();

                self.highlight_form = Some(form);
                self.input_mode = InputMode::HighlightForm;
                self.add_system_message("Opening highlight form (Tab to navigate, Esc to cancel)");
            }
            "edithighlight" | "edithl" => {
                if parts.len() < 2 {
                    self.add_system_message("Usage: .edithighlight <name>");
                    return;
                }

                let name = parts[1];

                if let Some(pattern) = self.config.highlights.get(name) {
                    let form = crate::ui::HighlightFormWidget::new_edit(name.to_string(), pattern);
                    self.highlight_form = Some(form);
                    self.input_mode = InputMode::HighlightForm;
                    self.add_system_message(&format!("Editing highlight '{}' (Tab to navigate, Esc to cancel)", name));
                } else {
                    self.add_system_message(&format!("Highlight '{}' not found", name));
                }
            }
            "deletehighlight" | "delhl" => {
                if parts.len() < 2 {
                    self.add_system_message("Usage: .deletehighlight <name>");
                    return;
                }

                let name = parts[1];

                if self.config.highlights.remove(name).is_some() {
                    if let Err(e) = self.config.save(None) {
                        self.add_system_message(&format!("Failed to save config: {}", e));
                    } else {
                        self.window_manager.update_highlights(self.config.highlights.clone());
                        self.add_system_message(&format!("Highlight '{}' deleted", name));
                    }
                } else {
                    self.add_system_message(&format!("Highlight '{}' not found", name));
                }
            }
            "listhighlights" | "listhl" | "highlights" => {
                // Open highlight browser
                self.open_highlight_browser();
            }
            "testhighlight" | "testhl" => {
                if parts.len() < 3 {
                    self.add_system_message("Usage: .testhighlight <name> <text to test>");
                    self.add_system_message("Example: .testhighlight combat_swing You swing a sword at the kobold!");
                    return;
                }

                let name = parts[1];
                // Join remaining parts as the test text
                let test_text = parts[2..].join(" ");

                // Clone the pattern to avoid borrowing issues
                if let Some(pattern_config) = self.config.highlights.get(name).cloned() {
                    // Test the pattern
                    match regex::Regex::new(&pattern_config.pattern) {
                        Ok(re) => {
                            if let Some(matched) = re.find(&test_text) {
                                self.add_system_message(&format!("✓ Match found in highlight '{}'", name));
                                self.add_system_message(&format!("  Pattern: {}", pattern_config.pattern));
                                self.add_system_message(&format!("  Matched: \"{}\"", matched.as_str()));
                                self.add_system_message(&format!("  Position: {} to {}", matched.start(), matched.end()));

                                // Show styling that would be applied
                                let mut style_info = Vec::new();
                                if let Some(ref fg) = pattern_config.fg {
                                    style_info.push(format!("fg={}", fg));
                                }
                                if let Some(ref bg) = pattern_config.bg {
                                    style_info.push(format!("bg={}", bg));
                                }
                                if pattern_config.bold {
                                    style_info.push("bold".to_string());
                                }
                                if pattern_config.color_entire_line {
                                    style_info.push("entire line".to_string());
                                }
                                if !style_info.is_empty() {
                                    self.add_system_message(&format!("  Styling: {}", style_info.join(", ")));
                                }
                            } else {
                                self.add_system_message(&format!("✗ No match in highlight '{}'", name));
                                self.add_system_message(&format!("  Pattern: {}", pattern_config.pattern));
                                self.add_system_message(&format!("  Test text: \"{}\"", test_text));
                            }
                        }
                        Err(e) => {
                            self.add_system_message(&format!("✗ Invalid regex in highlight '{}': {}", name, e));
                        }
                    }
                } else {
                    self.add_system_message(&format!("Highlight '{}' not found", name));
                    self.add_system_message("Use .listhighlights to see all highlights");
                }
            }
            "addkeybind" | "addkey" => {
                // Open keybind form in Create mode
                let form = crate::ui::KeybindFormWidget::new();
                self.keybind_form = Some(form);
                self.input_mode = InputMode::KeybindForm;
                self.add_system_message("Opening keybind form (Tab to navigate, Esc to cancel)");
            }
            "editkeybind" | "editkey" => {
                if parts.len() < 2 {
                    self.add_system_message("Usage: .editkeybind <key_combo>");
                    self.add_system_message("Example: .editkeybind ctrl+e");
                    return;
                }

                let key_combo = parts[1];

                if let Some(keybind_action) = self.config.keybinds.get(key_combo) {
                    use crate::config::KeyBindAction;
                    use crate::ui::KeybindActionType;

                    let (action_type, value) = match keybind_action {
                        KeyBindAction::Action(action_str) => (KeybindActionType::Action, action_str.clone()),
                        KeyBindAction::Macro(macro_action) => (KeybindActionType::Macro, macro_action.macro_text.clone()),
                    };

                    let form = crate::ui::KeybindFormWidget::new_edit(key_combo.to_string(), action_type, value);
                    self.keybind_form = Some(form);
                    self.input_mode = InputMode::KeybindForm;
                    self.add_system_message(&format!("Editing keybind '{}' (Tab to navigate, Esc to cancel)", key_combo));
                } else {
                    self.add_system_message(&format!("Keybind '{}' not found", key_combo));
                }
            }
            "deletekeybind" | "delkey" => {
                if parts.len() < 2 {
                    self.add_system_message("Usage: .deletekeybind <key_combo>");
                    self.add_system_message("Example: .deletekeybind ctrl+e");
                    return;
                }

                let key_combo = parts[1];

                if self.config.keybinds.remove(key_combo).is_some() {
                    if let Err(e) = self.config.save(None) {
                        self.add_system_message(&format!("Failed to delete keybind: {}", e));
                    } else {
                        self.rebuild_keybind_map();
                        self.add_system_message(&format!("Keybind '{}' deleted", key_combo));
                    }
                } else {
                    self.add_system_message(&format!("Keybind '{}' not found", key_combo));
                }
            }
            "listkeybinds" | "listkeys" | "keybinds" => {
                // Open keybind browser
                self.open_keybind_browser();
            }
            "settings" | "config" => {
                // Open settings editor with all config values
                self.open_settings_editor();
            }
            "colors" | "palette" | "colorpalette" => {
                // Open color palette browser
                self.open_color_palette_browser();
            }
            "addcolor" | "newcolor" | "createcolor" => {
                // Open color editor form for creating new color
                self.open_color_form_create();
            }
            "addspellcolor" | "newspellcolor" => {
                // Open spell color form in Create mode
                let form = crate::ui::SpellColorFormWidget::new();
                self.spell_color_form = Some(form);
                self.input_mode = InputMode::SpellColorForm;
                self.add_system_message("Opening spell color form (Tab to navigate, Ctrl+S to save, Esc to cancel)");
            }
            "spellcolors" => {
                // Open spell color browser
                let browser = crate::ui::SpellColorBrowser::new(&self.config.spell_colors);
                self.spell_color_browser = Some(browser);
                self.input_mode = InputMode::SpellColorBrowser;
                self.add_system_message("Opening spell color browser (↑/↓ to navigate, Enter to edit, Del to delete)");
            }
            "testmenu" => {
                // Test command: .testmenu <exist_id> [noun]
                // Example: .testmenu 12345 pendant
                if parts.len() < 2 {
                    self.add_system_message("Usage: .testmenu <exist_id> [noun]");
                    return;
                }

                let exist_id = parts[1].to_string();
                let noun = parts.get(2).map(|s| s.to_string()).unwrap_or_else(|| "item".to_string());

                // Generate counter and store pending request
                self.menu_request_counter += 1;
                let counter = self.menu_request_counter.to_string();

                self.pending_menu_requests.insert(counter.clone(), PendingMenuRequest {
                    exist_id: exist_id.clone(),
                    noun: noun.clone(),
                });

                // Send _menu command to server
                let menu_cmd = format!("_menu #{} {}", exist_id, counter);
                self.add_system_message(&format!("Requesting context menu for #{} (counter: {})", exist_id, counter));

                if let Some(tx) = command_tx {
                    let _ = tx.send(menu_cmd);
                    tracing::debug!("Sent menu request for exist_id {} with counter {}", exist_id, counter);
                } else {
                    self.add_system_message("Error: Cannot send menu request (no command channel)");
                }
            }
            "baseline" => {
                // Capture current terminal size as baseline for proportional resizing
                if let Ok(size) = crossterm::terminal::size() {
                    self.baseline_snapshot = Some((size.0, size.1));
                    self.add_system_message(&format!("Baseline captured: {}x{}", size.0, size.1));
                    tracing::info!("Baseline snapshot set to {}x{}", size.0, size.1);
                } else {
                    self.add_system_message("Failed to get terminal size");
                }
            }
            "resize" => {
                // Get baseline layout (original widget positions/sizes)
                let baseline_layout = if let Some(ref bl) = self.baseline_layout {
                    bl.clone()
                } else {
                    self.add_system_message("No baseline layout - cannot resize");
                    return;
                };

                // Get baseline terminal size from baseline layout
                let baseline = if let (Some(w), Some(h)) = (baseline_layout.terminal_width, baseline_layout.terminal_height) {
                    tracing::info!("Using baseline layout terminal size: {}x{}", w, h);
                    (w, h)
                } else if let Some(snapshot) = self.baseline_snapshot {
                    tracing::info!("Using baseline snapshot: {}x{}", snapshot.0, snapshot.1);
                    snapshot
                } else {
                    self.add_system_message("No baseline terminal size - cannot resize");
                    return;
                };

                if let Ok(current_size) = crossterm::terminal::size() {
                    let width_delta = current_size.0 as i32 - baseline.0 as i32;
                    let height_delta = current_size.1 as i32 - baseline.1 as i32;

                    tracing::info!(
                        "Resize: baseline={}x{}, current={}x{}, delta=({:+}, {:+})",
                        baseline.0, baseline.1, current_size.0, current_size.1, width_delta, height_delta
                    );

                    self.add_system_message(&format!(
                        "Resizing from {}x{} to {}x{} (delta: {:+}x{:+})",
                        baseline.0, baseline.1,
                        current_size.0, current_size.1,
                        width_delta, height_delta
                    ));

                    // Replace current layout with baseline layout (resets to original positions/sizes)
                    self.layout = baseline_layout;

                    // Apply proportional resize to layout (now working from baseline)
                    self.apply_proportional_resize2(width_delta, height_delta);

                    // Update window manager with new layout
                    self.update_window_manager_config();

                    // Autosave resized layout (force update terminal size to match resized widgets)
                    let autosave_name = if let Some(ref char_name) = self.config.character {
                        format!("auto_{}", char_name)
                    } else {
                        "autosave".to_string()
                    };

                    if let Err(e) = self.layout.save(&autosave_name, Some(current_size), true) {
                        tracing::error!("Failed to autosave resized layout: {}", e);
                        self.add_system_message(&format!("Resize complete (autosave failed: {})", e));
                    } else {
                        tracing::info!("Resized layout autosaved as {}", autosave_name);
                        self.add_system_message("Resize complete - layout autosaved");
                    }
                } else {
                    self.add_system_message("Failed to get current terminal size");
                }
            }
            _ => {
                self.add_system_message(&format!("Unknown command: .{}", parts[0]));
            }
        }
    }

    /// Open the main popup menu centered on the terminal
    fn open_main_menu(&mut self) {
        // Build main menu items (only top-level entries and settings)
        let mut items: Vec<crate::ui::MenuItem> = Vec::new();

        items.push(crate::ui::MenuItem { text: "Colors >".to_string(), command: "__SUBMENU__colors".to_string() });
        items.push(crate::ui::MenuItem { text: "Highlights >".to_string(), command: "__SUBMENU__highlights".to_string() });
        items.push(crate::ui::MenuItem { text: "Keybinds >".to_string(), command: "__SUBMENU__keybinds".to_string() });
        items.push(crate::ui::MenuItem { text: "Settings".to_string(), command: ".settings".to_string() });
        items.push(crate::ui::MenuItem { text: "Windows >".to_string(), command: "__SUBMENU__windows".to_string() });

        // Define submenu contents
        self.menu_categories.insert(
            "colors".to_string(),
            vec![
                crate::ui::MenuItem { text: "Add color".to_string(), command: ".addcolor".to_string() },
                crate::ui::MenuItem { text: "Browse colors".to_string(), command: ".colors".to_string() },
                crate::ui::MenuItem { text: "Add spellcolor".to_string(), command: ".addspellcolor".to_string() },
                crate::ui::MenuItem { text: "Browse spellcolors".to_string(), command: ".spellcolors".to_string() },
            ]
        );

        self.menu_categories.insert(
            "highlights".to_string(),
            vec![
                crate::ui::MenuItem { text: "Add highlight".to_string(), command: ".addhighlight".to_string() },
                crate::ui::MenuItem { text: "Browse highlights".to_string(), command: ".highlights".to_string() },
            ]
        );

        self.menu_categories.insert(
            "keybinds".to_string(),
            vec![
                crate::ui::MenuItem { text: "Add keybind".to_string(), command: ".addkeybind".to_string() },
                crate::ui::MenuItem { text: "Browse keybinds".to_string(), command: ".keybinds".to_string() },
            ]
        );

        self.menu_categories.insert(
            "windows".to_string(),
            vec![
                crate::ui::MenuItem { text: "Add window".to_string(), command: ".addwindow".to_string() },
                crate::ui::MenuItem { text: "Browse windows".to_string(), command: ".editwindow".to_string() },
            ]
        );

        // Calculate centered position based on terminal size and menu size
        let (term_w, term_h) = match crossterm::terminal::size() {
            Ok(sz) => (sz.0, sz.1),
            Err(_) => (80, 24),
        };

        let max_width = items.iter().map(|i| i.text.len()).max().unwrap_or(20).min(60) as u16;
        let menu_width = max_width + 4; // padding + borders
        let menu_height = (items.len() as u16) + 2; // items + borders

        let pos_x = term_w.saturating_sub(menu_width) / 2;
        let pos_y = term_h.saturating_sub(menu_height) / 2;

        self.popup_menu = Some(crate::ui::PopupMenu::new(items, (pos_x, pos_y)));
        self.submenu = None;
        self.nested_submenu = None;
    }

    /// Request a context menu for a game object
    fn request_menu(&mut self, exist_id: &str, noun: &str, command_tx: Option<&mpsc::UnboundedSender<String>>) -> Result<()> {
        // Generate counter and store pending request
        self.menu_request_counter += 1;
        let counter = self.menu_request_counter.to_string();

        self.pending_menu_requests.insert(counter.clone(), PendingMenuRequest {
            exist_id: exist_id.to_string(),
            noun: noun.to_string(),
        });

        // Send _menu command to server
        let menu_cmd = format!("_menu #{} {}", exist_id, counter);
        tracing::debug!("Requesting context menu for #{} (noun: {}, counter: {})", exist_id, noun, counter);

        if let Some(tx) = command_tx {
            tx.send(menu_cmd)?;
        } else {
            self.add_system_message("Error: Cannot send menu request (no command channel)");
        }

        Ok(())
    }

    /// Handle menu response from server
    fn handle_menu_response(&mut self, counter: &str, coords: &[(String, Option<String>)]) {
        // Look up the pending request
        let pending = self.pending_menu_requests.remove(counter);

        if pending.is_none() {
            tracing::warn!("Received menu response for unknown counter: {}", counter);
            return;
        }

        let pending = pending.unwrap();
        tracing::info!("Menu response for exist_id {} (noun: {}): {} coords",
            pending.exist_id, pending.noun, coords.len());

        // Look up each coord in cmdlist and build menu items
        if self.cmdlist.is_none() {
            self.add_system_message("Context menu received but cmdlist not loaded");
            return;
        }

        // Group menu items by category
        let mut categories: HashMap<String, Vec<crate::ui::MenuItem>> = HashMap::new();

        if let Some(ref cmdlist) = self.cmdlist {
            for (coord, secondary_noun) in coords.iter() {
                if let Some(entry) = cmdlist.get(coord) {
                    // Skip _dialog commands for now (until dialog box is implemented)
                    if entry.command.starts_with("_dialog") {
                        tracing::debug!("Skipping _dialog command: {}", entry.command);
                        continue;
                    }

                    // For menu display: handle special characters
                    // # and @ are removed/truncated
                    // % is substituted with secondary noun (if present)
                    let menu_text = {
                        let mut text = entry.menu.clone();

                        // First, substitute % with secondary noun (if present)
                        if let Some(ref sec_noun) = secondary_noun {
                            text = text.replace("%", sec_noun);
                        }

                        // Then find first occurrence of # or @
                        let mut special_char_pos = None;
                        for (pos, ch) in text.char_indices() {
                            if ch == '#' || ch == '@' {
                                special_char_pos = Some(pos);
                                break;
                            }
                        }

                        if let Some(pos) = special_char_pos {
                            let after_special = pos + 1;

                            // Check if there's meaningful text after the special character
                            let remaining = text[after_special..].trim();

                            if remaining.is_empty() {
                                // Special char is at the end - truncate at it
                                text[..pos].trim_end().to_string()
                            } else {
                                // Special char is in the middle - remove it but keep the rest
                                // Keep one space between words
                                let before = text[..pos].trim_end();
                                let after = text[after_special..].trim_start();

                                if before.is_empty() {
                                    after.to_string()
                                } else if after.is_empty() {
                                    before.to_string()
                                } else {
                                    format!("{} {}", before, after)
                                }
                            }
                        } else {
                            // No # or @ found - return text (% already substituted)
                            text
                        }
                    };

                    let command = crate::cmdlist::CmdList::substitute_command(
                        &entry.command,
                        &pending.noun,
                        &pending.exist_id,
                        secondary_noun.as_deref(), // Pass secondary noun for % substitution
                    );

                    let category = if entry.menu_cat.is_empty() {
                        "0".to_string()
                    } else {
                        entry.menu_cat.clone()
                    };

                    categories.entry(category).or_insert_with(Vec::new).push(crate::ui::MenuItem {
                        text: menu_text,
                        command,
                    });
                } else {
                    tracing::debug!("Coord {} not found in cmdlist", coord);
                }
            }
        }

        if categories.is_empty() {
            self.add_system_message("No menu items available for this object");
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

        // Detect parent-child category relationships
        // Example: "5_roleplay" is parent of "5_roleplay-swear"
        // Note: Hyphens indicate subcategories, not underscores
        let mut child_categories: std::collections::HashSet<String> = std::collections::HashSet::new();
        for cat in &sorted_cats {
            if cat.contains('_') && cat != "0" {
                // Check if this contains a hyphen (subcategory indicator)
                if cat.contains('-') {
                    // Extract potential parent by removing hyphen and everything after
                    if let Some(hyphen_pos) = cat.find('-') {
                        let potential_parent = &cat[..hyphen_pos];
                        if sorted_cats.iter().any(|c| c == potential_parent) {
                            child_categories.insert(cat.clone());
                        }
                    }
                }
            }
        }

        // Build menu items for parent categories (including their child submenus)
        for cat in &sorted_cats {
            // Skip if this is a child category (it will be added under its parent)
            if child_categories.contains(cat) {
                continue;
            }

            let items = categories.get(cat).unwrap();

            // Categories with _ in the name become submenus (e.g., "5_roleplay", "6_combat maneuvers")
            // Exception: category "0" never becomes a submenu
            if cat.contains('_') && cat != "0" {
                // Build submenu items for this parent category
                let mut submenu_items = items.clone();

                // Add child category submenus
                for child_cat in &sorted_cats {
                    if child_categories.contains(child_cat) {
                        // Check if this child belongs to current parent
                        // Child format: "5_roleplay-swear", parent format: "5_roleplay"
                        if let Some(hyphen_pos) = child_cat.find('-') {
                            let parent = &child_cat[..hyphen_pos];
                            if parent == cat {
                                let child_name = Self::format_subcategory_name(child_cat, cat);
                                submenu_items.push(crate::ui::MenuItem {
                                    text: format!("{} >", child_name),
                                    command: format!("__SUBMENU__{}", child_cat),
                                });
                            }
                        }
                    }
                }

                // Cache the parent category with its items AND child submenus
                self.menu_categories.insert(cat.clone(), submenu_items.clone());

                // Add parent to main menu
                let cat_name = Self::format_category_name(cat);
                menu_items.push(crate::ui::MenuItem {
                    text: format!("{} >", cat_name),
                    command: format!("__SUBMENU__{}", cat),
                });
            } else {
                // Add items directly to main menu
                menu_items.extend(items.clone());
            }
        }

        // Cache all child categories (they already have their items from earlier)
        for cat in &sorted_cats {
            if child_categories.contains(cat) {
                let items = categories.get(cat).unwrap();
                self.menu_categories.insert(cat.clone(), items.clone());
            }
        }

        if menu_items.is_empty() {
            self.add_system_message("No menu items available for this object");
            return;
        }

        // Create popup menu at last click position
        let position = self.last_link_click_pos.unwrap_or((0, 0));
        let item_count = menu_items.len();
        self.popup_menu = Some(crate::ui::PopupMenu::new(menu_items, position));
        tracing::info!("Created popup menu with {} items at {:?}", item_count, position);
    }

    /// Format a category code into a display name
    /// Handle event pattern matches (stun, webbed, etc.)
    fn handle_event(&mut self, event_type: &str, action: crate::config::EventAction, duration: u32) {
        use crate::config::EventAction;

        match event_type {
            "stun" => {
                match action {
                    EventAction::Set => {
                        if duration > 0 {
                            // Find stuntime countdown widget and set it
                            if let Some(window) = self.window_manager.get_window("stuntime") {
                                // Calculate end time: current time + duration
                                let end_time = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs() + duration as u64;

                                window.set_countdown(end_time);
                                tracing::debug!("Set stun countdown to {}s (end_time: {})", duration, end_time);
                            } else {
                                tracing::warn!("Stun event matched but no 'stuntime' window found");
                            }
                        }
                    }
                    EventAction::Clear => {
                        // Clear stun countdown
                        if let Some(window) = self.window_manager.get_window("stuntime") {
                            window.set_countdown(0);
                            tracing::debug!("Cleared stun countdown");
                        }
                    }
                    _ => {}
                }
            }

            // Future: Handle other event types (webbed, prone, etc.)
            "webbed" | "prone" | "kneeling" | "sitting" | "hidden" | "invisible" | "silenced" => {
                tracing::debug!("Event type '{}' matched but not yet implemented", event_type);
                // TODO: Set indicator state when indicator support is added
            }

            _ => {
                tracing::debug!("Unknown event type: {}", event_type);
            }
        }
    }

    fn format_category_name(cat: &str) -> String {
        // Category format: "0", "1", "5_roleplay", "6_combat maneuvers", "9_challenge", etc.
        if let Some((_num, name)) = cat.split_once('_') {
            // Handle names with spaces (e.g., "combat maneuvers")
            // Return lowercase (no capitalization)
            name.to_string()
        } else {
            // Just a number - use generic names (lowercase)
            match cat {
                "0" => "general".to_string(),
                "1" => "basic".to_string(),
                "2" => "item".to_string(),
                "3" => "social".to_string(),
                "4" => "combat".to_string(),
                "5" => "roleplay".to_string(),
                "6" => "religious".to_string(),
                "7" => "magic".to_string(),
                "8" => "healing".to_string(),
                "9" => "challenge".to_string(),
                _ => format!("category {}", cat),
            }
        }
    }

    /// Format a subcategory name by extracting the part after the parent
    /// Example: "5_roleplay-swear" with parent "5_roleplay" -> "swear"
    fn format_subcategory_name(child: &str, parent: &str) -> String {
        if let Some(suffix) = child.strip_prefix(&format!("{}-", parent)) {
            suffix.to_string()
        } else {
            Self::format_category_name(child)
        }
    }

    /// Calculate menu rectangle for a popup menu
    /// Returns the actual Rect that will be used for rendering
    fn calculate_menu_rect(menu: &crate::ui::PopupMenu, terminal_area: Rect) -> Rect {
        let max_width = menu.get_items().iter()
            .map(|item| item.text.len())
            .max()
            .unwrap_or(20)
            .min(60);
        let menu_width = (max_width + 4) as u16;
        let menu_height = (menu.get_items().len() + 2) as u16;
        let position = menu.get_position();

        // Ensure menu fits within terminal area
        let x = position.0.min(terminal_area.width.saturating_sub(menu_width));
        let y = position.1.min(terminal_area.height.saturating_sub(menu_height));

        Rect {
            x,
            y,
            width: menu_width.min(terminal_area.width),
            height: menu_height.min(terminal_area.height),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        // Check minimum terminal size BEFORE entering alternate screen
        const MIN_WIDTH: u16 = 80;
        const MIN_HEIGHT: u16 = 24;

        // Get terminal size without entering alternate screen yet
        let size = crossterm::terminal::size()?;
        if size.0 < MIN_WIDTH || size.1 < MIN_HEIGHT {
            eprintln!("\n╔════════════════════════════════════════════════════════════╗");
            eprintln!("║              TERMINAL SIZE TOO SMALL                       ║");
            eprintln!("╠════════════════════════════════════════════════════════════╣");
            eprintln!("║ VellumFE requires a minimum terminal size to run.         ║");
            eprintln!("║                                                            ║");
            eprintln!("║ Current size: {}x{} (width x height)", size.0, size.1);
            eprintln!("║ Required:     {}x{} (minimum)", MIN_WIDTH, MIN_HEIGHT);
            eprintln!("║                                                            ║");
            eprintln!("║ Please resize your terminal window and try again.         ║");
            eprintln!("╚════════════════════════════════════════════════════════════╝\n");

            return Err(anyhow::anyhow!("Terminal size {}x{} is below minimum {}x{}",
                size.0, size.1, MIN_WIDTH, MIN_HEIGHT));
        }

        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        tracing::info!("Terminal size: {}x{}", size.0, size.1);

        // Capture baseline for proportional resizing
        self.baseline_snapshot = Some((size.0, size.1));
        tracing::info!("Captured baseline terminal size: {}x{}", size.0, size.1);

        // Set up signal handler for Ctrl+C and terminal close
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
        }).expect("Error setting Ctrl+C handler");

        // Connect to Lich
        let (server_tx, mut server_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();

        // Spawn connection task
        let host = self.config.connection.host.clone();
        let port = self.config.connection.port;
        tokio::spawn(async move {
            if let Err(e) = LichConnection::start(&host, port, server_tx, command_rx).await {
                tracing::error!("Connection error: {}", e);
            }
        });

        // Main event loop
        while self.running && running.load(Ordering::SeqCst) {
            // Update window widths based on terminal size
            let terminal_size = terminal.size()?;
            let terminal_rect = ratatui::layout::Rect::new(0, 0, terminal_size.width, terminal_size.height);

            // Get command_input position from windows array
            let (cmd_row, cmd_col, cmd_height, cmd_width) = self.layout.windows.iter()
                .find(|w| w.widget_type == "command_input")
                .map(|w| (w.row, w.col, w.rows, w.cols))
                .expect("command_input must exist in windows array");

            let layout = UiLayout::calculate(terminal_rect, cmd_row, cmd_col, cmd_height, cmd_width);

            // Calculate window layouts using proportional sizing
            let layout_calc_start = std::time::Instant::now();
            let mut window_layouts = self.window_manager.calculate_layout(layout.main_area);

            // Add command_input to window_layouts for mouse operations
            window_layouts.insert(
                "command_input".to_string(),
                ratatui::layout::Rect::new(cmd_col, cmd_row, cmd_width, cmd_height)
            );

            self.window_manager.update_widths(&window_layouts);
            let layout_calc_duration = layout_calc_start.elapsed();
            if layout_calc_duration.as_millis() > 100 {
                tracing::warn!("Layout calculation took {}ms - possible freeze!", layout_calc_duration.as_millis());
            }

            // Draw UI and track render time
            let render_start = std::time::Instant::now();
            terminal.draw(|f| {
                let ui_render_start = std::time::Instant::now();

                // Get command_input position from windows array
                let (cmd_row, cmd_col, cmd_height, cmd_width) = self.layout.windows.iter()
                    .find(|w| w.widget_type == "command_input")
                    .map(|w| (w.row, w.col, w.rows, w.cols))
                    .expect("command_input must exist in windows array");

                let layout = UiLayout::calculate(f.area(), cmd_row, cmd_col, cmd_height, cmd_width);
                let mut window_layouts = self.window_manager.calculate_layout(layout.main_area);

                // Add command_input to window_layouts for mouse operations (with bounds checking)
                let terminal_area = f.area();
                let cmd_input_rect = if cmd_row < terminal_area.height && cmd_col < terminal_area.width {
                    // Clip command_input to terminal bounds
                    let clipped_height = cmd_height.min(terminal_area.height.saturating_sub(cmd_row));
                    let clipped_width = cmd_width.min(terminal_area.width.saturating_sub(cmd_col));

                    if clipped_height > 0 && clipped_width > 0 {
                        Some(ratatui::layout::Rect::new(cmd_col, cmd_row, clipped_width, clipped_height))
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(rect) = cmd_input_rect {
                    // Debug logging removed - was logging on every frame (~60 FPS)
                    window_layouts.insert("command_input".to_string(), rect);
                }

                // Render all windows in order with focus indicator
                let window_names = self.window_manager.get_window_names();
                let terminal_area = f.area();
                let mut out_of_bounds_count = 0;

                for (idx, name) in window_names.iter().enumerate() {
                    if let Some(rect) = window_layouts.get(name) {
                        // Layer 2: Bounds checking safety net
                        // Skip widgets that start beyond terminal bounds
                        if rect.y >= terminal_area.height || rect.x >= terminal_area.width {
                            tracing::warn!(
                                "Skipping out-of-bounds widget '{}' at {}x{} (terminal: {}x{})",
                                name, rect.x, rect.y, terminal_area.width, terminal_area.height
                            );
                            out_of_bounds_count += 1;
                            continue;
                        }

                        // Clip widgets that extend beyond terminal bounds
                        let clipped_rect = if rect.y + rect.height > terminal_area.height || rect.x + rect.width > terminal_area.width {
                            let clipped_height = rect.height.min(terminal_area.height.saturating_sub(rect.y));
                            let clipped_width = rect.width.min(terminal_area.width.saturating_sub(rect.x));

                            if clipped_height > 0 && clipped_width > 0 {
                                tracing::warn!(
                                    "Clipping widget '{}' from {}x{} to {}x{} (terminal: {}x{})",
                                    name, rect.width, rect.height, clipped_width, clipped_height,
                                    terminal_area.width, terminal_area.height
                                );
                                ratatui::layout::Rect::new(rect.x, rect.y, clipped_width, clipped_height)
                            } else {
                                tracing::warn!("Skipping widget '{}' - would be clipped to zero size", name);
                                out_of_bounds_count += 1;
                                continue;
                            }
                        } else {
                            *rect
                        };

                        if let Some(window) = self.window_manager.get_window(name) {
                            let focused = idx == self.focused_window_index;
                            window.render_with_focus(
                                clipped_rect,
                                f.buffer_mut(),
                                focused,
                                self.server_time_offset,
                                self.selection_state.as_ref(),
                                &self.config.ui.selection_bg_color,
                                idx,
                            );
                        }
                    }
                }

                // Layer 3: User feedback (show once)
                if out_of_bounds_count > 0 && !self.shown_bounds_warning {
                    tracing::error!(
                        "{} widgets out of bounds! Terminal: {}x{}, use .resize to adapt layout",
                        out_of_bounds_count, terminal_area.width, terminal_area.height
                    );
                    self.shown_bounds_warning = true;
                }

                // Render performance stats if enabled
                if self.show_perf_stats {
                    // Use config values, but calculate X dynamically if set to 0 (right-align)
                    let x = if self.config.ui.perf_stats_x == 0 {
                        f.area().width.saturating_sub(self.config.ui.perf_stats_width)
                    } else {
                        self.config.ui.perf_stats_x
                    };

                    let perf_rect = Rect {
                        x,
                        y: self.config.ui.perf_stats_y,
                        width: self.config.ui.perf_stats_width,
                        height: self.config.ui.perf_stats_height,
                    };
                    let perf_widget = PerformanceStatsWidget::new();
                    perf_widget.render(perf_rect, f.buffer_mut(), &self.perf_stats);
                }

                // Render input based on mode (only if command_input is in bounds)
                if let Some(cmd_input_rect) = window_layouts.get("command_input").copied() {
                    match self.input_mode {
                        InputMode::Search => {
                            // Render search input with prompt
                            self.render_search_input(cmd_input_rect, f.buffer_mut());
                        }
                        InputMode::HighlightForm | InputMode::KeybindForm | InputMode::SettingsEditor | InputMode::HighlightBrowser | InputMode::KeybindBrowser | InputMode::WindowEditor | InputMode::ColorPaletteBrowser | InputMode::ColorForm | InputMode::SpellColorBrowser | InputMode::SpellColorForm => {
                            // Hide command input when form/browser/editor is open
                        }
                        _ => {
                            self.command_input.render(cmd_input_rect, f.buffer_mut());
                        }
                    }
                } else {
                    // Command input is out of bounds - skip rendering but you can still type commands
                    tracing::debug!("Command input is out of bounds - not rendering");
                }

                // Render highlight form as popup (if open)
                if let Some(ref mut form) = self.highlight_form {
                    form.render(f.area(), f.buffer_mut(), &self.config);
                }

                // Render keybind form as popup (if open)
                if let Some(ref mut form) = self.keybind_form {
                    form.render(f.area(), f.buffer_mut());
                }

                // Render settings editor as popup (if open)
                if let Some(ref mut editor) = self.settings_editor {
                    editor.render(f.area(), f.buffer_mut(), &self.config);
                }

                // Render highlight browser as popup (if open)
                if let Some(ref mut browser) = self.highlight_browser {
                    browser.render(f.area(), f.buffer_mut(), &self.config);
                }

                // Render keybind browser as popup (if open)
                if let Some(ref mut browser) = self.keybind_browser {
                    browser.render(f.area(), f.buffer_mut());
                }

                // Render color palette browser as popup (if open)
                if let Some(ref browser) = self.color_palette_browser {
                    browser.render(f.area(), f.buffer_mut());
                }

                // Render color form as popup (if open)
                if let Some(ref mut form) = self.color_form {
                    form.render(f.area(), f.buffer_mut());
                }

                // Render spell color browser as popup (if open)
                if let Some(ref browser) = self.spell_color_browser {
                    browser.render(f.area(), f.buffer_mut());
                }

                // Render spell color form as popup (if open)
                if let Some(ref mut form) = self.spell_color_form {
                    form.render(f.area(), f.buffer_mut());
                }

                // UI color editor removed

                // Render window editor as popup (if open)
                if self.input_mode == InputMode::WindowEditor {
                    self.window_editor.render(f.area(), f.buffer_mut(), &self.config);
                }

                // Render popup menu (if open)
                // Render main popup menu
                if let Some(ref menu) = self.popup_menu {
                    menu.render(f.area(), f.buffer_mut());
                }

                // Render submenu on top of main menu
                if let Some(ref submenu) = self.submenu {
                    submenu.render(f.area(), f.buffer_mut());
                }

                // Render nested submenu on top of everything
                if let Some(ref nested_submenu) = self.nested_submenu {
                    nested_submenu.render(f.area(), f.buffer_mut());
                }

                // Record UI render time
                let ui_render_duration = ui_render_start.elapsed();
                self.perf_stats.record_ui_render_time(ui_render_duration);
            })?;

            // Record total render time
            let render_duration = render_start.elapsed();
            self.perf_stats.record_render_time(render_duration);

            // Record frame completion
            self.perf_stats.record_frame();

            // Handle events with timeout (configurable via poll_timeout_ms setting)
            if event::poll(std::time::Duration::from_millis(self.config.ui.poll_timeout_ms))? {
                let event_start = std::time::Instant::now();
                match event::read()? {
                    Event::Key(key) => {
                        // Only handle key press events, not release or repeat
                        if key.kind == KeyEventKind::Press {
                            self.handle_key_event(key.code, key.modifiers, &command_tx)?;
                        }
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse_event(mouse, &window_layouts, &command_tx)?;
                    }
                    Event::Resize(width, height) => {
                        const MIN_WIDTH: u16 = 80;
                        const MIN_HEIGHT: u16 = 24;

                        // Check debouncer - only process resize if enough time has passed
                        if let Some((w, h)) = self.resize_debouncer.check_resize(width, height) {
                            tracing::debug!("Terminal resized to {}x{} (debounced)", w, h);

                            if w < MIN_WIDTH || h < MIN_HEIGHT {
                                tracing::warn!(
                                    "Terminal size {}x{} is below minimum {}x{}. Layout may not display correctly.",
                                    w, h, MIN_WIDTH, MIN_HEIGHT
                                );
                                let warning_msg = format!(
                                    "⚠ Warning: Terminal size {}x{} is below minimum {}x{}. Please resize for optimal display.",
                                    w, h, MIN_WIDTH, MIN_HEIGHT
                                );
                                self.add_system_message(&warning_msg);
                            }
                            // Layout recalculation happens automatically on next frame
                        } else {
                            tracing::trace!("Resize to {}x{} debounced (waiting for resize to finish)", width, height);
                        }
                    }
                    _ => {}
                }
                let event_duration = event_start.elapsed();
                self.perf_stats.record_event_process_time(event_duration);
            }

            // Check for pending resize (if debounce period has passed)
            if let Some((width, height)) = self.resize_debouncer.check_pending() {
                tracing::debug!("Processing pending resize to {}x{}", width, height);
                const MIN_WIDTH: u16 = 80;
                const MIN_HEIGHT: u16 = 24;
                if width < MIN_WIDTH || height < MIN_HEIGHT {
                    let warning_msg = format!(
                        "⚠ Warning: Terminal size {}x{} is below minimum {}x{}. Please resize for optimal display.",
                        width, height, MIN_WIDTH, MIN_HEIGHT
                    );
                    self.add_system_message(&warning_msg);
                }
                // Layout recalculation happens automatically on next frame
            }

            // Handle server messages
            let msg_start = std::time::Instant::now();
            let mut msg_count = 0;
            while let Ok(msg) = server_rx.try_recv() {
                self.handle_server_message(msg);
                msg_count += 1;
            }
            let msg_duration = msg_start.elapsed();
            if msg_duration.as_millis() > 100 {
                tracing::warn!("Message processing took {}ms ({} messages) - possible freeze!",
                    msg_duration.as_millis(), msg_count);
            }

            // Update memory stats periodically (count total lines buffered)
            let window_names = self.window_manager.get_window_names();
            let mut total_lines = 0;
            for name in &window_names {
                if let Some(window) = self.window_manager.get_window(name) {
                    total_lines += window.line_count();
                }
            }
            let window_count = self.layout.windows.len();
            self.perf_stats.update_memory_stats(total_lines, window_count);
        }

        // Autosave config before exiting
        if let Err(e) = self.config.save(None) {
            tracing::error!("Failed to autosave config: {}", e);
        } else {
            tracing::info!("Config autosaved");
        }

        // Note: Layout autosave removed - layouts are now saved after .resize instead
        // This prevents corrupting the designed terminal size on exit

        // Save command history
        if let Err(e) = self.command_input.save_history(self.config.character.as_deref()) {
            tracing::error!("Failed to save command history: {}", e);
        } else {
            tracing::info!("Command history saved");
        }

        // Cleanup terminal
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
        terminal.show_cursor()?;

        Ok(())
    }

    fn handle_key_event(
        &mut self,
        key: KeyCode,
        modifiers: KeyModifiers,
        command_tx: &mpsc::UnboundedSender<String>,
    ) -> Result<()> {
        // Handle popup menu keys first (highest priority)
        // Check nested submenu first, then submenu, then main menu
        if self.nested_submenu.is_some() {
            match key {
                KeyCode::Esc | KeyCode::Left => {
                    // Close nested submenu, keep submenu and main menu
                    self.nested_submenu = None;
                    return Ok(());
                }
                KeyCode::Up => {
                    if let Some(ref mut menu) = self.nested_submenu {
                        menu.select_previous();
                    }
                    return Ok(());
                }
                KeyCode::Down => {
                    if let Some(ref mut menu) = self.nested_submenu {
                        menu.select_next();
                    }
                    return Ok(());
                }
                KeyCode::Enter => {
                    if let Some(ref menu) = self.nested_submenu {
                        if let Some(command) = menu.get_selected_command() {
                            debug!("Nested submenu item selected via Enter: {}", command);
                            if command.starts_with('.') {
                                self.handle_dot_command(&command, Some(command_tx));
                            } else {
                                command_tx.send(command)?;
                            }
                            self.popup_menu = None;
                            self.submenu = None;
                            self.nested_submenu = None;
                        }
                    }
                    return Ok(());
                }
                _ => {
                    // Other keys close all menus
                    self.popup_menu = None;
                    self.submenu = None;
                    self.nested_submenu = None;
                }
            }
        } else if self.submenu.is_some() {
            match key {
                KeyCode::Esc | KeyCode::Left => {
                    // Close submenu, keep main menu
                    self.submenu = None;
                    return Ok(());
                }
                KeyCode::Up => {
                    if let Some(ref mut menu) = self.submenu {
                        menu.select_previous();
                    }
                    return Ok(());
                }
                KeyCode::Down => {
                    if let Some(ref mut menu) = self.submenu {
                        menu.select_next();
                    }
                    return Ok(());
                }
                KeyCode::Enter | KeyCode::Right => {
                    if let Some(ref menu) = self.submenu {
                        if let Some(command) = menu.get_selected_command() {
                            // Check if this is a nested submenu
                            if command.starts_with("__SUBMENU__") {
                                let cat = command.strip_prefix("__SUBMENU__").unwrap();
                                debug!("Opening nested submenu for category: {}", cat);
                                if let Some(items) = self.menu_categories.get(cat) {
                                    let menu_pos = menu.get_position();
                                    let selected_idx = menu.get_selected_index();
                                    let nested_submenu_pos = (menu_pos.0 + 5, menu_pos.1 + selected_idx as u16 + 1);
                                    self.nested_submenu = Some(crate::ui::PopupMenu::new(items.clone(), nested_submenu_pos));
                                }
                                return Ok(());
                            }

                            debug!("Submenu item selected via Enter: {}", command);
                            if command.starts_with('.') {
                                self.handle_dot_command(&command, Some(command_tx));
                            } else {
                                command_tx.send(command)?;
                            }
                            self.popup_menu = None;
                            self.submenu = None;
                        }
                    }
                    return Ok(());
                }
                _ => {
                    // Other keys close all menus
                    self.popup_menu = None;
                    self.submenu = None;
                }
            }
        } else if self.popup_menu.is_some() {
            match key {
                KeyCode::Esc => {
                    self.popup_menu = None;
                    return Ok(());
                }
                KeyCode::Up => {
                    if let Some(ref mut menu) = self.popup_menu {
                        menu.select_previous();
                    }
                    return Ok(());
                }
                KeyCode::Down => {
                    if let Some(ref mut menu) = self.popup_menu {
                        menu.select_next();
                    }
                    return Ok(());
                }
                KeyCode::Enter | KeyCode::Right => {
                    if let Some(ref menu) = self.popup_menu {
                        if let Some(command) = menu.get_selected_command() {
                            // Check if this is a submenu
                            if command.starts_with("__SUBMENU__") {
                                let cat = command.strip_prefix("__SUBMENU__").unwrap();
                                debug!("Opening submenu for category: {}", cat);
                                debug!("Available categories: {:?}", self.menu_categories.keys().collect::<Vec<_>>());
                                if let Some(items) = self.menu_categories.get(cat) {
                                    debug!("Found {} items for category {}", items.len(), cat);
                                    // Get current menu position for offset
                                    let menu_pos = menu.get_position();
                                    let selected_idx = menu.get_selected_index();
                                    let submenu_pos = (menu_pos.0 + 5, menu_pos.1 + selected_idx as u16 + 1);
                                    self.submenu = Some(crate::ui::PopupMenu::new(items.clone(), submenu_pos));
                                } else {
                                    debug!("ERROR: Category '{}' not found in menu_categories!", cat);
                                }
                                return Ok(());
                            }

                            debug!("Menu item selected via Enter: {}", command);
                            if command.starts_with('.') {
                                self.handle_dot_command(&command, Some(command_tx));
                            } else {
                                command_tx.send(command)?;
                            }
                            self.popup_menu = None;
                        }
                    }
                    return Ok(());
                }
                _ => {
                    // Other keys close the menu
                    self.popup_menu = None;
                }
            }
        }

        // In HighlightForm mode, handle in the form directly (except Ctrl+C to quit)
        if self.input_mode == InputMode::HighlightForm {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to the form
            return self.handle_highlight_form_input(key, modifiers);
        }

        // In KeybindForm mode, handle in the form directly (except Ctrl+C to quit)
        if self.input_mode == InputMode::KeybindForm {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to the form
            return self.handle_keybind_form_input(key, modifiers);
        }

        // In SettingsEditor mode, handle in the editor directly (except Ctrl+C to quit)
        if self.input_mode == InputMode::SettingsEditor {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to the editor
            return self.handle_settings_editor_input(key, modifiers);
        }

        if self.input_mode == InputMode::HighlightBrowser {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to the browser
            return self.handle_highlight_browser_input(key, modifiers);
        }

        if self.input_mode == InputMode::KeybindBrowser {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to the browser
            return self.handle_keybind_browser_input(key, modifiers);
        }

        if self.input_mode == InputMode::ColorPaletteBrowser {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to the browser
            return self.handle_color_palette_browser_input(key, modifiers);
        }

        if self.input_mode == InputMode::ColorForm {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to the form
            return self.handle_color_form_input(key, modifiers);
        }

        if self.input_mode == InputMode::ColorBrowserFilter {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to filter input handler
            return self.handle_color_browser_filter_input(key, modifiers);
        }

        if self.input_mode == InputMode::SpellColorBrowser {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to the spell color browser
            return self.handle_spell_color_browser_input(key, modifiers);
        }

        if self.input_mode == InputMode::SpellColorForm {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }
            // Everything else goes to the spell color form
            return self.handle_spell_color_form_input(key, modifiers);
        }


        // In WindowEditor mode, handle in the editor directly (except Ctrl+C to quit)
        if self.input_mode == InputMode::WindowEditor {
            // Allow Ctrl+C to quit
            if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return Ok(());
            }

            // Convert crossterm KeyCode to ratatui::crossterm KeyCode
            use ratatui::crossterm::event as rt_event;

            let rt_key_code = match key {
                KeyCode::Backspace => rt_event::KeyCode::Backspace,
                KeyCode::Enter => rt_event::KeyCode::Enter,
                KeyCode::Left => rt_event::KeyCode::Left,
                KeyCode::Right => rt_event::KeyCode::Right,
                KeyCode::Up => rt_event::KeyCode::Up,
                KeyCode::Down => rt_event::KeyCode::Down,
                KeyCode::Home => rt_event::KeyCode::Home,
                KeyCode::End => rt_event::KeyCode::End,
                KeyCode::PageUp => rt_event::KeyCode::PageUp,
                KeyCode::PageDown => rt_event::KeyCode::PageDown,
                KeyCode::Tab => rt_event::KeyCode::Tab,
                KeyCode::BackTab => rt_event::KeyCode::BackTab,
                KeyCode::Delete => rt_event::KeyCode::Delete,
                KeyCode::Insert => rt_event::KeyCode::Insert,
                KeyCode::F(n) => rt_event::KeyCode::F(n),
                KeyCode::Char(c) => rt_event::KeyCode::Char(c),
                KeyCode::Null => rt_event::KeyCode::Null,
                KeyCode::Esc => rt_event::KeyCode::Esc,
                _ => rt_event::KeyCode::Null,
            };

            let mut rt_modifiers = rt_event::KeyModifiers::empty();
            if modifiers.contains(KeyModifiers::SHIFT) {
                rt_modifiers |= rt_event::KeyModifiers::SHIFT;
            }
            if modifiers.contains(KeyModifiers::CONTROL) {
                rt_modifiers |= rt_event::KeyModifiers::CONTROL;
            }
            if modifiers.contains(KeyModifiers::ALT) {
                rt_modifiers |= rt_event::KeyModifiers::ALT;
            }

            let key_event = rt_event::KeyEvent {
                code: rt_key_code,
                modifiers: rt_modifiers,
                kind: rt_event::KeyEventKind::Press,
                state: rt_event::KeyEventState::empty(),
            };

            // Check if we're in SelectingWindow mode and user pressed Enter
            use crate::ui::WindowEditorResult;
            if key == KeyCode::Enter {
                if let Some(window_name) = self.window_editor.get_selected_window_name() {
                    // Load window for editing
                    if let Some(window) = self.layout.windows.iter().find(|w| w.name == window_name).cloned() {
                        self.window_editor.load_window(window);
                        return Ok(());
                    }
                }
            }

            // Process key and handle result
            if let Some(result) = self.window_editor.handle_key(key_event) {
                match result {
                    WindowEditorResult::Save { mut window, is_new, original_name } => {
                        // Resolve color names to hex codes for all color fields
                        let mut resolve_opt = |v: &mut Option<String>| {
                            if let Some(ref s) = v.clone() {
                                if let Some(resolved) = self.config.resolve_color(s) { *v = Some(resolved); }
                            }
                        };

                        resolve_opt(&mut window.border_color);
                        resolve_opt(&mut window.background_color);
                        resolve_opt(&mut window.bar_color);
                        resolve_opt(&mut window.bar_background_color);
                        resolve_opt(&mut window.text_color);
                        resolve_opt(&mut window.tab_active_color);
                        resolve_opt(&mut window.tab_inactive_color);
                        resolve_opt(&mut window.tab_unread_color);
                        resolve_opt(&mut window.compass_active_color);
                        resolve_opt(&mut window.compass_inactive_color);
                        resolve_opt(&mut window.effect_default_color);
                        resolve_opt(&mut window.injury_default_color);
                        resolve_opt(&mut window.injury1_color);
                        resolve_opt(&mut window.injury2_color);
                        resolve_opt(&mut window.injury3_color);
                        resolve_opt(&mut window.scar1_color);
                        resolve_opt(&mut window.scar2_color);
                        resolve_opt(&mut window.scar3_color);

                        if let Some(ref mut vec_colors) = window.indicator_colors {
                            for c in vec_colors.iter_mut() {
                                if let Some(resolved) = self.config.resolve_color(c) { *c = resolved; }
                            }
                        }

                        // Special handling for command_input
                        if window.widget_type == "command_input" {
                            // Update the windows array
                            if let Some(idx) = self.layout.windows.iter().position(|w| w.widget_type == "command_input") {
                                self.layout.windows[idx] = window.clone();
                            } else if is_new {
                                // Creating new command_input
                                self.layout.windows.push(window.clone());
                            }

                            // Apply changes to the actual CommandInput widget immediately
                            self.command_input.set_border_config(
                                window.show_border,
                                window.border_style.clone(),
                                window.border_color.clone(),
                            );
                            if let Some(ref title) = window.title {
                                self.command_input.set_title(title.clone());
                            }
                            self.command_input.set_background_color(window.background_color.clone());

                            self.add_system_message("Updated command input box");
                            self.add_system_message("Remember to .savelayout to save this configuration!");
                        } else if is_new {
                            self.layout.windows.push(window.clone());
                            self.add_system_message(&format!("Created window '{}' - use mouse to move/resize", window.name));
                            self.add_system_message("Remember to .savelayout to save this configuration!");
                            self.update_window_manager_config();
                        } else if let Some(orig_name) = original_name {
                            if let Some(idx) = self.layout.windows.iter().position(|w| w.name == orig_name) {
                                tracing::debug!("Updating window '{}': numbers_only = {}", window.name, window.numbers_only);
                                self.layout.windows[idx] = window.clone();

                                // Also update baseline_layout so .resize doesn't lose the changes
                                if let Some(ref mut baseline) = self.baseline_layout {
                                    if let Some(baseline_idx) = baseline.windows.iter().position(|w| w.name == orig_name) {
                                        baseline.windows[baseline_idx] = window.clone();
                                    }
                                }

                                self.add_system_message(&format!("Updated window '{}'", window.name));
                                self.add_system_message("Remember to .savelayout to save this configuration!");
                                self.update_window_manager_config();
                            }
                        }
                        // Close editor
                        self.input_mode = InputMode::Normal;
                    },
                    WindowEditorResult::Cancel => {
                        // Close editor
                        self.input_mode = InputMode::Normal;
                    }
                }
                return Ok(());
            }
            return Ok(());
        }

        // Handle global keys first (work in any mode except HighlightForm)
        match (key, modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.running = false;
                return Ok(());
            }
            (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                // Enter search mode
                self.input_mode = InputMode::Search;
                self.search_input.clear();
                return Ok(());
            }
            (KeyCode::Esc, _) => {
                // Clear text selection if any
                if self.selection_state.is_some() {
                    self.selection_state = None;
                }

                // Exit search mode
                if self.input_mode == InputMode::Search {
                    self.input_mode = InputMode::Normal;
                    if let Some(window) = self.get_focused_window() {
                        window.clear_search();
                    }
                }
                return Ok(());
            }
            (KeyCode::PageUp, KeyModifiers::CONTROL) => {
                // Previous search match
                if let Some(window) = self.get_focused_window() {
                    window.prev_match();
                }
                return Ok(());
            }
            (KeyCode::PageDown, KeyModifiers::CONTROL) => {
                // Next search match
                if let Some(window) = self.get_focused_window() {
                    window.next_match();
                }
                return Ok(());
            }
            _ => {}
        }

        // Handle mode-specific keys
        match self.input_mode {
            InputMode::Search => self.handle_search_input(key, modifiers),
            InputMode::ColorBrowserFilter => self.handle_color_browser_filter_input(key, modifiers),
            InputMode::Normal => self.handle_normal_input(key, modifiers, command_tx),
            InputMode::HighlightForm => unreachable!(), // Handled above
            InputMode::KeybindForm => unreachable!(), // Handled above
            InputMode::SettingsEditor => unreachable!(), // Handled above
            InputMode::HighlightBrowser => unreachable!(), // Handled above
            InputMode::KeybindBrowser => unreachable!(), // Handled above
            InputMode::ColorPaletteBrowser => unreachable!(), // Handled above
            InputMode::ColorForm => unreachable!(), // Handled above
            InputMode::SpellColorBrowser => unreachable!(), // Handled above
            InputMode::SpellColorForm => unreachable!(), // Handled above
            InputMode::WindowEditor => unreachable!(), // Handled above
        }
    }

    fn handle_highlight_form_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        if let Some(ref mut form) = self.highlight_form {
            // Convert crossterm KeyCode to ratatui::crossterm KeyCode
            // They're the same enum, just from different crates
            use ratatui::crossterm::event as rt_event;

            let rt_key_code = match key {
                KeyCode::Backspace => rt_event::KeyCode::Backspace,
                KeyCode::Enter => rt_event::KeyCode::Enter,
                KeyCode::Left => rt_event::KeyCode::Left,
                KeyCode::Right => rt_event::KeyCode::Right,
                KeyCode::Up => rt_event::KeyCode::Up,
                KeyCode::Down => rt_event::KeyCode::Down,
                KeyCode::Home => rt_event::KeyCode::Home,
                KeyCode::End => rt_event::KeyCode::End,
                KeyCode::PageUp => rt_event::KeyCode::PageUp,
                KeyCode::PageDown => rt_event::KeyCode::PageDown,
                KeyCode::Tab => rt_event::KeyCode::Tab,
                KeyCode::BackTab => rt_event::KeyCode::BackTab,
                KeyCode::Delete => rt_event::KeyCode::Delete,
                KeyCode::Insert => rt_event::KeyCode::Insert,
                KeyCode::F(n) => rt_event::KeyCode::F(n),
                KeyCode::Char(c) => rt_event::KeyCode::Char(c),
                KeyCode::Null => rt_event::KeyCode::Null,
                KeyCode::Esc => rt_event::KeyCode::Esc,
                _ => rt_event::KeyCode::Null, // Fallback for other keys
            };

            // Convert modifiers by ORing all active flags
            let mut rt_modifiers = rt_event::KeyModifiers::empty();
            if modifiers.contains(KeyModifiers::SHIFT) {
                rt_modifiers |= rt_event::KeyModifiers::SHIFT;
            }
            if modifiers.contains(KeyModifiers::CONTROL) {
                rt_modifiers |= rt_event::KeyModifiers::CONTROL;
            }
            if modifiers.contains(KeyModifiers::ALT) {
                rt_modifiers |= rt_event::KeyModifiers::ALT;
            }

            let key_event = rt_event::KeyEvent {
                code: rt_key_code,
                modifiers: rt_modifiers,
                kind: rt_event::KeyEventKind::Press,
                state: rt_event::KeyEventState::empty(),
            };

            if let Some(result) = form.handle_key(key_event) {
                use crate::ui::FormResult;
                match result {
                    FormResult::Save { name, mut pattern } => {
                        // Resolve color names to hex for fg/bg before saving
                        let is_hex = |s: &str| -> bool { s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit()) };
                        if let Some(ref fg) = pattern.fg.clone() {
                            if let Some(resolved) = self.config.resolve_color(fg) {
                                if is_hex(&resolved) { pattern.fg = Some(resolved); }
                            }
                        }
                        if let Some(ref bg) = pattern.bg.clone() {
                            if let Some(resolved) = self.config.resolve_color(bg) {
                                if is_hex(&resolved) { pattern.bg = Some(resolved); }
                            }
                        }

                        // Save to config
                        self.config.highlights.insert(name.clone(), pattern);
                        if let Err(e) = self.config.save(None) {
                            self.add_system_message(&format!("Failed to save highlight: {}", e));
                        } else {
                            // Reload highlights in window manager
                            self.window_manager.update_highlights(self.config.highlights.clone());
                            self.add_system_message(&format!("Highlight '{}' saved", name));
                        }

                        // Close form
                        self.highlight_form = None;
                        self.input_mode = InputMode::Normal;
                    }
                    FormResult::Delete { name } => {
                        // Delete from config
                        self.config.highlights.remove(&name);
                        if let Err(e) = self.config.save(None) {
                            self.add_system_message(&format!("Failed to delete highlight: {}", e));
                        } else {
                            // Reload highlights in window manager
                            self.window_manager.update_highlights(self.config.highlights.clone());
                            self.add_system_message(&format!("Highlight '{}' deleted", name));
                        }

                        // Close form
                        self.highlight_form = None;
                        self.input_mode = InputMode::Normal;
                    }
                    FormResult::Cancel => {
                        // Close form without saving
                        self.highlight_form = None;
                        self.input_mode = InputMode::Normal;
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_keybind_form_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        if let Some(ref mut form) = self.keybind_form {
            // Convert crossterm KeyCode to ratatui::crossterm KeyCode
            use ratatui::crossterm::event as rt_event;

            let rt_key_code = match key {
                KeyCode::Backspace => rt_event::KeyCode::Backspace,
                KeyCode::Enter => rt_event::KeyCode::Enter,
                KeyCode::Left => rt_event::KeyCode::Left,
                KeyCode::Right => rt_event::KeyCode::Right,
                KeyCode::Up => rt_event::KeyCode::Up,
                KeyCode::Down => rt_event::KeyCode::Down,
                KeyCode::Home => rt_event::KeyCode::Home,
                KeyCode::End => rt_event::KeyCode::End,
                KeyCode::PageUp => rt_event::KeyCode::PageUp,
                KeyCode::PageDown => rt_event::KeyCode::PageDown,
                KeyCode::Tab => rt_event::KeyCode::Tab,
                KeyCode::BackTab => rt_event::KeyCode::BackTab,
                KeyCode::Delete => rt_event::KeyCode::Delete,
                KeyCode::Insert => rt_event::KeyCode::Insert,
                KeyCode::F(n) => rt_event::KeyCode::F(n),
                KeyCode::Char(c) => rt_event::KeyCode::Char(c),
                KeyCode::Null => rt_event::KeyCode::Null,
                KeyCode::Esc => rt_event::KeyCode::Esc,
                _ => rt_event::KeyCode::Null,
            };

            let mut rt_modifiers = rt_event::KeyModifiers::empty();
            if modifiers.contains(KeyModifiers::SHIFT) {
                rt_modifiers |= rt_event::KeyModifiers::SHIFT;
            }
            if modifiers.contains(KeyModifiers::CONTROL) {
                rt_modifiers |= rt_event::KeyModifiers::CONTROL;
            }
            if modifiers.contains(KeyModifiers::ALT) {
                rt_modifiers |= rt_event::KeyModifiers::ALT;
            }

            let key_event = rt_event::KeyEvent {
                code: rt_key_code,
                modifiers: rt_modifiers,
                kind: rt_event::KeyEventKind::Press,
                state: rt_event::KeyEventState::empty(),
            };

            if let Some(result) = form.handle_key(key_event) {
                use crate::ui::{KeybindFormResult, KeybindActionType};
                use crate::config::{KeyBindAction, MacroAction};

                match result {
                    KeybindFormResult::Save { key_combo, action_type, value } => {
                        // Create the KeyBindAction
                        let keybind_action = match action_type {
                            KeybindActionType::Action => KeyBindAction::Action(value.clone()),
                            KeybindActionType::Macro => KeyBindAction::Macro(MacroAction { macro_text: value.clone() }),
                        };

                        // Save to config
                        self.config.keybinds.insert(key_combo.clone(), keybind_action);
                        if let Err(e) = self.config.save(None) {
                            self.add_system_message(&format!("Failed to save keybind: {}", e));
                        } else {
                            // Rebuild keybind_map
                            self.rebuild_keybind_map();
                            self.add_system_message(&format!("Keybind '{}' saved", key_combo));
                        }

                        // Close form
                        self.keybind_form = None;
                        self.input_mode = InputMode::Normal;
                    }
                    KeybindFormResult::Delete { key_combo } => {
                        // Delete from config
                        self.config.keybinds.remove(&key_combo);
                        if let Err(e) = self.config.save(None) {
                            self.add_system_message(&format!("Failed to delete keybind: {}", e));
                        } else {
                            // Rebuild keybind_map
                            self.rebuild_keybind_map();
                            self.add_system_message(&format!("Keybind '{}' deleted", key_combo));
                        }

                        // Close form
                        self.keybind_form = None;
                        self.input_mode = InputMode::Normal;
                    }
                    KeybindFormResult::Cancel => {
                        // Close form without saving
                        self.keybind_form = None;
                        self.input_mode = InputMode::Normal;
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_settings_editor_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        // Collect message to send outside the editor borrow
        let mut message_to_send: Option<String> = None;

        if let Some(ref mut editor) = self.settings_editor {
            match (key, modifiers) {
                (KeyCode::Esc, _) => {
                    // Close editor without saving
                    self.settings_editor = None;
                    self.input_mode = InputMode::Normal;
                    message_to_send = Some("Settings editor closed".to_string());
                }
                (KeyCode::Up, _) => {
                    editor.previous();
                }
                (KeyCode::Down, _) => {
                    editor.next();
                }
                (KeyCode::PageUp, _) => {
                    editor.page_up();
                }
                (KeyCode::PageDown, _) => {
                    editor.page_down();
                }
                (KeyCode::Enter, _) | (KeyCode::Char(' '), KeyModifiers::NONE) => {
                    if editor.is_editing() {
                        // Finish editing and save value
                        if let Some((idx, new_value)) = editor.finish_edit() {
                            if let Some(item) = editor.get_item(idx) {
                                let key = item.key.clone();
                                let display_name = item.display_name.clone();
                                let _ = editor; // Drop borrow before calling update
                                if self.update_setting(&key, &new_value) {
                                    message_to_send = Some(format!("Setting '{}' updated to: {}", display_name, new_value));
                                    // Refresh the settings editor to show new values
                                    self.refresh_settings_editor();
                                } else {
                                    message_to_send = Some(format!("Failed to update setting '{}'", display_name));
                                }
                            }
                        }
                    } else {
                        // Check if it's a boolean - if so, toggle it
                        if let Some((idx, new_bool)) = editor.toggle_boolean() {
                            if let Some(item) = editor.get_item(idx) {
                                let key = item.key.clone();
                                let display_name = item.display_name.clone();
                                let _ = editor; // Drop borrow before calling update
                                if self.update_setting(&key, &new_bool.to_string()) {
                                    message_to_send = Some(format!("Setting '{}' toggled to: {}", display_name, new_bool));
                                    // Refresh the settings editor to show new values
                                    self.refresh_settings_editor();
                                } else {
                                    message_to_send = Some(format!("Failed to toggle setting '{}'", display_name));
                                }
                            }
                        } else {
                            // Not a boolean, start editing current item
                            editor.start_edit();
                        }
                    }
                }
                (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                    if editor.is_editing() {
                        editor.handle_edit_input(c);
                    }
                }
                (KeyCode::Backspace, _) => {
                    if editor.is_editing() {
                        editor.handle_edit_backspace();
                    }
                }
                _ => {}
            }
        }

        // Send message after editor borrow is dropped
        if let Some(msg) = message_to_send {
            self.add_system_message(&msg);
        }

        Ok(())
    }

    fn handle_highlight_browser_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        let mut message_to_send: Option<String> = None;
        let mut selected_name: Option<String> = None;
        let mut delete_name: Option<String> = None;

        if let Some(ref mut browser) = self.highlight_browser {
            match (key, modifiers) {
                (KeyCode::Esc, _) => {
                    // Close browser
                    self.highlight_browser = None;
                    self.input_mode = InputMode::Normal;
                    message_to_send = Some("Highlight browser closed".to_string());
                }
                (KeyCode::Up, _) => {
                    browser.previous();
                }
                (KeyCode::Down, _) => {
                    browser.next();
                }
                (KeyCode::PageUp, _) => {
                    browser.page_up();
                }
                (KeyCode::PageDown, _) => {
                    browser.page_down();
                }
                (KeyCode::Enter, _) => {
                    // Edit selected highlight
                    if let Some(name) = browser.get_selected() {
                        selected_name = Some(name.clone());
                    }
                }
                (KeyCode::Delete, _) => {
                    // Delete selected highlight
                    if let Some(name) = browser.get_selected() {
                        delete_name = Some(name.clone());
                    }
                }
                _ => {}
            }
        }

        // Handle edit action (after browser borrow is dropped)
        if let Some(name) = selected_name {
            if let Some(pattern) = self.config.highlights.get(&name).cloned() {
                // Close browser and open highlight form in edit mode
                self.highlight_browser = None;

                // Create form with existing pattern
                let form = crate::ui::HighlightFormWidget::with_pattern(name.clone(), pattern);
                self.highlight_form = Some(form);
                self.input_mode = InputMode::HighlightForm;
                message_to_send = Some(format!("Editing highlight: {}", name));
            }
        }

        // Handle delete action (after browser borrow is dropped)
        if let Some(name) = delete_name {
            if self.config.highlights.remove(&name).is_some() {
                // Save config
                if let Err(e) = self.config.save(self.config.character.as_deref()) {
                    message_to_send = Some(format!("Failed to save config: {}", e));
                } else {
                    // Update window manager with new highlights
                    self.window_manager.update_highlights(self.config.highlights.clone());

                    // Refresh the browser with updated list
                    if let Some(ref mut browser) = self.highlight_browser {
                        *browser = crate::ui::HighlightBrowser::new(&self.config.highlights);
                    }

                    message_to_send = Some(format!("Deleted highlight: {}", name));
                }
            } else {
                message_to_send = Some(format!("Highlight '{}' not found", name));
            }
        }

        // Send message after all borrows are dropped
        if let Some(msg) = message_to_send {
            self.add_system_message(&msg);
        }

        Ok(())
    }

    fn handle_keybind_browser_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        let mut message_to_send: Option<String> = None;
        let mut selected_key: Option<String> = None;
        let mut delete_key: Option<String> = None;

        if let Some(ref mut browser) = self.keybind_browser {
            match (key, modifiers) {
                (KeyCode::Esc, _) => {
                    // Close browser
                    self.keybind_browser = None;
                    self.input_mode = InputMode::Normal;
                    message_to_send = Some("Keybind browser closed".to_string());
                }
                (KeyCode::Up, _) => {
                    browser.previous();
                }
                (KeyCode::Down, _) => {
                    browser.next();
                }
                (KeyCode::PageUp, _) => {
                    browser.page_up();
                }
                (KeyCode::PageDown, _) => {
                    browser.page_down();
                }
                (KeyCode::Enter, _) => {
                    // Edit selected keybind
                    if let Some(key_combo) = browser.get_selected() {
                        selected_key = Some(key_combo.clone());
                    }
                }
                (KeyCode::Delete, _) => {
                    // Delete selected keybind
                    if let Some(key_combo) = browser.get_selected() {
                        delete_key = Some(key_combo.clone());
                    }
                }
                _ => {}
            }
        }

        // Handle edit action (after browser borrow is dropped)
        if let Some(key_combo) = selected_key {
            if let Some(keybind_action) = self.config.keybinds.get(&key_combo).cloned() {
                use crate::config::KeyBindAction;
                use crate::ui::KeybindActionType;

                // Close browser and open keybind form in edit mode
                self.keybind_browser = None;

                let (action_type, value) = match keybind_action {
                    KeyBindAction::Action(action) => (KeybindActionType::Action, action),
                    KeyBindAction::Macro(macro_action) => (KeybindActionType::Macro, macro_action.macro_text),
                };

                let form = crate::ui::KeybindFormWidget::new_edit(key_combo.clone(), action_type, value);
                self.keybind_form = Some(form);
                self.input_mode = InputMode::KeybindForm;
                message_to_send = Some(format!("Editing keybind: {}", key_combo));
            }
        }

        // Handle delete action (after browser borrow is dropped)
        if let Some(key_combo) = delete_key {
            if self.config.keybinds.remove(&key_combo).is_some() {
                // Save config
                if let Err(e) = self.config.save(None) {
                    message_to_send = Some(format!("Failed to save config: {}", e));
                } else {
                    // Rebuild keybind map
                    self.rebuild_keybind_map();

                    // Refresh the browser with updated list
                    if let Some(ref mut browser) = self.keybind_browser {
                        *browser = crate::ui::KeybindBrowser::new(&self.config.keybinds);
                    }

                    message_to_send = Some(format!("Deleted keybind: {}", key_combo));
                }
            } else {
                message_to_send = Some(format!("Keybind '{}' not found", key_combo));
            }
        }

        // Send message after all borrows are dropped
        if let Some(msg) = message_to_send {
            self.add_system_message(&msg);
        }

        Ok(())
    }

    fn handle_color_palette_browser_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        let mut message_to_send: Option<String> = None;

        if let Some(ref mut browser) = self.color_palette_browser {
            match (key, modifiers) {
                (KeyCode::Esc, _) => {
                    // Close browser
                    self.color_palette_browser = None;
                    self.input_mode = InputMode::Normal;
                    message_to_send = Some("Color palette browser closed".to_string());
                }
                (KeyCode::Up, _) => {
                    browser.previous();
                }
                (KeyCode::Down, _) => {
                    browser.next();
                }
                (KeyCode::PageUp, _) => {
                    browser.page_up();
                }
                (KeyCode::PageDown, _) => {
                    browser.page_down();
                }
                (KeyCode::Enter, _) => {
                    // Edit selected color
                    if let Some(color) = browser.get_selected_color() {
                        let color_to_edit = color.clone();
                        // Close browser
                        self.color_palette_browser = None;
                        // Open edit form
                        self.open_color_form_edit(color_to_edit);
                        return Ok(()); // Early return to avoid message
                    }
                }
                (KeyCode::Delete, _) => {
                    // Delete selected color
                    if let Some(color_name) = browser.get_selected() {
                        // Remove from config
                        self.config.color_palette.retain(|c| c.name != color_name);

                        // Save config
                        if let Err(e) = self.config.save(self.config.character.as_deref()) {
                            message_to_send = Some(format!("Failed to save config: {}", e));
                        } else {
                            // Refresh browser with updated palette
                            *browser = crate::ui::ColorPaletteBrowser::new(self.config.color_palette.clone());
                            message_to_send = Some(format!("Deleted color: {}", color_name));
                        }
                    }
                }
                (KeyCode::Char('f') | KeyCode::Char('F'), _) => {
                    // Toggle favorite
                    browser.toggle_favorite();
                    // Save config with updated palette
                    self.config.color_palette = browser.get_colors().clone();
                    if let Err(e) = self.config.save(self.config.character.as_deref()) {
                        message_to_send = Some(format!("Failed to save config: {}", e));
                    }
                }
                (KeyCode::Char('/'), _) => {
                    // Start filter input
                    self.command_input.clear();
                    self.input_mode = InputMode::ColorBrowserFilter;
                    message_to_send = Some("Filter colors (Enter to apply, Esc to cancel):".to_string());
                }
                _ => {}
            }
        }

        // Send message after all borrows are dropped
        if let Some(msg) = message_to_send {
            self.add_system_message(&msg);
        }

        Ok(())
    }

    fn handle_color_form_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        use crate::ui::ColorFormAction;
        use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

        if let Some(ref mut form) = self.color_form {
            let key_event = KeyEvent {
                code: key,
                modifiers,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            };
            if let Some(action) = form.handle_input(key_event) {
                match action {
                    ColorFormAction::Save { color, original_name } => {
                        // Check if we're renaming (editing with name change)
                        if let Some(ref old_name) = original_name {
                            if old_name != &color.name {
                                // Remove old color
                                self.config.color_palette.retain(|c| &c.name != old_name);
                            } else {
                                // Just updating existing color
                                self.config.color_palette.retain(|c| c.name != color.name);
                            }
                        } else {
                            // Check for duplicate name when creating new
                            if self.config.color_palette.iter().any(|c| c.name == color.name) {
                                self.add_system_message(&format!("Color '{}' already exists", color.name));
                                return Ok(());
                            }
                        }

                        // Add the color
                        self.config.color_palette.push(color.clone());

                        // Save config
                        if let Err(e) = self.config.save(self.config.character.as_deref()) {
                            self.add_system_message(&format!("Failed to save config: {}", e));
                        } else {
                            // Close form
                            self.color_form = None;
                            self.input_mode = InputMode::Normal;

                            // Refresh browser if it's open
                            if let Some(ref mut browser) = self.color_palette_browser {
                                *browser = crate::ui::ColorPaletteBrowser::new(self.config.color_palette.clone());
                            }

                            let action_verb = if original_name.is_some() { "Updated" } else { "Added" };
                            self.add_system_message(&format!("{} color: {}", action_verb, color.name));
                        }
                    }
                    ColorFormAction::Cancel => {
                        self.color_form = None;
                        self.input_mode = InputMode::Normal;
                        self.add_system_message("Color form cancelled");
                    }
                    ColorFormAction::Delete => {
                        // Get the original name from the form
                        let name = if let Some(ref form) = self.color_form {
                            form.get_original_name().unwrap_or_default()
                        } else {
                            String::new()
                        };

                        // Delete the color
                        self.config.color_palette.retain(|c| c.name != name);

                        // Save config
                        if let Err(e) = self.config.save(self.config.character.as_deref()) {
                            self.add_system_message(&format!("Failed to save config: {}", e));
                        } else {
                            // Close form
                            self.color_form = None;
                            self.input_mode = InputMode::Normal;

                            // Refresh browser if it's open
                            if let Some(ref mut browser) = self.color_palette_browser {
                                *browser = crate::ui::ColorPaletteBrowser::new(self.config.color_palette.clone());
                            }

                            self.add_system_message(&format!("Deleted color: {}", name));
                        }
                    }
                    ColorFormAction::Error(msg) => {
                        self.add_system_message(&format!("Error: {}", msg));
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_spell_color_browser_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        let mut message_to_send: Option<String> = None;
        let mut selected_index: Option<usize> = None;
        let mut delete_index: Option<usize> = None;

        if let Some(ref mut browser) = self.spell_color_browser {
            match (key, modifiers) {
                (KeyCode::Esc, _) => {
                    // Close browser
                    self.spell_color_browser = None;
                    self.input_mode = InputMode::Normal;
                    message_to_send = Some("Spell color browser closed".to_string());
                }
                (KeyCode::Up, _) => {
                    browser.previous();
                }
                (KeyCode::Down, _) => {
                    browser.next();
                }
                (KeyCode::PageUp, _) => {
                    browser.page_up();
                }
                (KeyCode::PageDown, _) => {
                    browser.page_down();
                }
                (KeyCode::Enter, _) => {
                    // Edit selected spell color
                    if let Some(index) = browser.get_selected() {
                        selected_index = Some(index);
                    }
                }
                (KeyCode::Delete, _) => {
                    // Delete selected spell color
                    if let Some(index) = browser.get_selected() {
                        delete_index = Some(index);
                    }
                }
                _ => {}
            }
        }

        // Handle edit action (after browser borrow is dropped)
        if let Some(index) = selected_index {
            if let Some(spell_color) = self.config.spell_colors.get(index).cloned() {
                // Close browser and open spell color form in edit mode
                self.spell_color_browser = None;

                let form = crate::ui::SpellColorFormWidget::new_edit(index, &spell_color);
                self.spell_color_form = Some(form);
                self.input_mode = InputMode::SpellColorForm;
                let spell_ids = spell_color.spells.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ");
                message_to_send = Some(format!("Editing spell colors for: {}", spell_ids));
            }
        }

        // Handle delete action (after browser borrow is dropped)
        if let Some(index) = delete_index {
            if index < self.config.spell_colors.len() {
                let removed = self.config.spell_colors.remove(index);
                // Save config
                if let Err(e) = self.config.save(self.config.character.as_deref()) {
                    message_to_send = Some(format!("Failed to save config: {}", e));
                } else {
                    // Refresh the browser with updated list
                    if let Some(ref mut browser) = self.spell_color_browser {
                        *browser = crate::ui::SpellColorBrowser::new(&self.config.spell_colors);
                    }

                    let spell_ids = removed.spells.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ");
                    message_to_send = Some(format!("Deleted spell colors for: {}", spell_ids));
                }
            } else {
                message_to_send = Some("Spell color not found".to_string());
            }
        }

        // Send message after all borrows are dropped
        if let Some(msg) = message_to_send {
            self.add_system_message(&msg);
        }

        Ok(())
    }

    fn handle_spell_color_form_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        if let Some(ref mut form) = self.spell_color_form {
            // Intercept Ctrl+S at app level to match other editors' behavior
            if modifiers.contains(KeyModifiers::CONTROL) {
                if let KeyCode::Char(c) = key {
                    if c == 's' || c == 'S' {
                        if let Some(result) = form.try_save() {
                            use crate::ui::SpellColorFormResult;
                            match result {
                                SpellColorFormResult::Save(spell_color) => {
                                    // Resolve color names to hex and validate
                                    let mut sc = spell_color.clone();

                                    let is_hex = |s: &str| -> bool {
                                        s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
                                    };

                                    // Resolve bar color (primary) from either bar_color or legacy color
                                    let input_bar = sc.bar_color.clone().or_else(|| if sc.color.is_empty() { None } else { Some(sc.color.clone()) });
                                    if let Some(input) = input_bar {
                                        if let Some(resolved) = self.config.resolve_color(&input) {
                                            if is_hex(&resolved) {
                                                sc.bar_color = Some(resolved.clone());
                                                sc.color = resolved; // keep legacy in sync
                                            } else {
                                                self.add_system_message(&format!("Invalid bar color '{}'. Use #RRGGBB or a palette name.", input));
                                                return Ok(());
                                            }
                                        } else {
                                            self.add_system_message(&format!("Invalid bar color '{}'. Use #RRGGBB or a palette name.", input));
                                            return Ok(());
                                        }
                                    } else {
                                        sc.bar_color = None;
                                        sc.color.clear();
                                    }

                                    // Resolve text color
                                    if let Some(input) = sc.text_color.clone() {
                                        if input.is_empty() {
                                            sc.text_color = None;
                                        } else if let Some(resolved) = self.config.resolve_color(&input) {
                                            if is_hex(&resolved) {
                                                sc.text_color = Some(resolved);
                                            } else {
                                                self.add_system_message(&format!("Invalid text color '{}'. Use #RRGGBB or a palette name.", input));
                                                return Ok(());
                                            }
                                        }
                                    }

                                    // Resolve background color
                                    if let Some(input) = sc.bg_color.clone() {
                                        if input.is_empty() {
                                            sc.bg_color = None;
                                        } else if let Some(resolved) = self.config.resolve_color(&input) {
                                            if is_hex(&resolved) {
                                                sc.bg_color = Some(resolved);
                                            } else {
                                                self.add_system_message(&format!("Invalid background color '{}'. Use #RRGGBB or a palette name.", input));
                                                return Ok(());
                                            }
                                        }
                                    }

                                    // Add or update spell color in config (same logic as below)
                                    let mut replaced_index = None;
                                    for spell_id in &spell_color.spells {
                                        if let Some(idx) = self.config.spell_colors.iter().position(|sc| sc.spells.contains(spell_id)) {
                                            replaced_index = Some(idx);
                                            break;
                                        }
                                    }
                                    if let Some(idx) = replaced_index {
                                        self.config.spell_colors[idx] = sc.clone();
                                    } else {
                                        self.config.spell_colors.push(sc.clone());
                                    }
                                    if let Err(e) = self.config.save(self.config.character.as_deref()) {
                                        self.add_system_message(&format!("Failed to save spell color: {}", e));
                                    } else {
                                        let spell_ids = sc.spells.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ");
                                        self.add_system_message(&format!("Spell color saved for spells: {}", spell_ids));
                                    }
                                    self.spell_color_form = None;
                                    self.input_mode = InputMode::Normal;
                                }
                                SpellColorFormResult::Delete(_) => { /* not used on Ctrl+S */ }
                                SpellColorFormResult::Cancel => { /* ignore */ }
                            }
                        } else {
                            // Provide feedback when validation fails (previously silent)
                            self.add_system_message("Spell color form: enter spell IDs like 905,509 and colors as #RRGGBB or palette names");
                        }
                        return Ok(());
                    }
                }
            }
            // Create crossterm KeyEvent directly (form expects crossterm::event::KeyEvent)
            use crossterm::event as ct_event;

            let key_event = ct_event::KeyEvent {
                code: key,
                modifiers,
                kind: ct_event::KeyEventKind::Press,
                state: ct_event::KeyEventState::empty(),
            };

            if let Some(result) = form.input(key_event) {
                use crate::ui::SpellColorFormResult;

                match result {
                    SpellColorFormResult::Save(spell_color) => {
                        // Resolve color names to hex and validate
                        let mut sc = spell_color.clone();

                        let is_hex = |s: &str| -> bool {
                            s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
                        };

                        // Resolve bar color (primary)
                        let input_bar = sc.bar_color.clone().or_else(|| if sc.color.is_empty() { None } else { Some(sc.color.clone()) });
                        if let Some(input) = input_bar {
                            if let Some(resolved) = self.config.resolve_color(&input) {
                                if is_hex(&resolved) {
                                    sc.bar_color = Some(resolved.clone());
                                    sc.color = resolved;
                                } else {
                                    self.add_system_message(&format!("Invalid bar color '{}'. Use #RRGGBB or a palette name.", input));
                                    return Ok(());
                                }
                            } else {
                                self.add_system_message(&format!("Invalid bar color '{}'. Use #RRGGBB or a palette name.", input));
                                return Ok(());
                            }
                        } else {
                            sc.bar_color = None;
                            sc.color.clear();
                        }

                        // Resolve text color
                        if let Some(input) = sc.text_color.clone() {
                            if input.is_empty() {
                                sc.text_color = None;
                            } else if let Some(resolved) = self.config.resolve_color(&input) {
                                if is_hex(&resolved) {
                                    sc.text_color = Some(resolved);
                                } else {
                                    self.add_system_message(&format!("Invalid text color '{}'. Use #RRGGBB or a palette name.", input));
                                    return Ok(());
                                }
                            }
                        }

                        // Resolve background color
                        if let Some(input) = sc.bg_color.clone() {
                            if input.is_empty() {
                                sc.bg_color = None;
                            } else if let Some(resolved) = self.config.resolve_color(&input) {
                                if is_hex(&resolved) {
                                    sc.bg_color = Some(resolved);
                                } else {
                                    self.add_system_message(&format!("Invalid background color '{}'. Use #RRGGBB or a palette name.", input));
                                    return Ok(());
                                }
                            }
                        }
                        // Add or update spell color in config
                        // First check if any of these spell IDs already exist
                        let mut replaced_index = None;
                        for spell_id in &spell_color.spells {
                            if let Some(idx) = self.config.spell_colors.iter().position(|sc| sc.spells.contains(spell_id)) {
                                replaced_index = Some(idx);
                                break;
                            }
                        }

                        // If we found an existing entry, replace it; otherwise add new
                        if let Some(idx) = replaced_index {
                            self.config.spell_colors[idx] = sc.clone();
                        } else {
                            self.config.spell_colors.push(sc.clone());
                        }

                        if let Err(e) = self.config.save(self.config.character.as_deref()) {
                            self.add_system_message(&format!("Failed to save spell color: {}", e));
                        } else {
                            let spell_ids = sc.spells.iter()
                                .map(|id| id.to_string())
                                .collect::<Vec<_>>()
                                .join(", ");
                            self.add_system_message(&format!("Spell color saved for spells: {}", spell_ids));
                        }

                        // Close form
                        self.spell_color_form = None;
                        self.input_mode = InputMode::Normal;
                    }
                    SpellColorFormResult::Delete(index) => {
                        // Delete from config
                        if index < self.config.spell_colors.len() {
                            let removed = self.config.spell_colors.remove(index);
                            if let Err(e) = self.config.save(self.config.character.as_deref()) {
                                self.add_system_message(&format!("Failed to delete spell color: {}", e));
                            } else {
                                let spell_ids = removed.spells.iter()
                                    .map(|id| id.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                self.add_system_message(&format!("Spell color deleted for spells: {}", spell_ids));
                            }
                        }

                        // Close form
                        self.spell_color_form = None;
                        self.input_mode = InputMode::Normal;
                    }
                    SpellColorFormResult::Cancel => {
                        // Close form without saving
                        self.spell_color_form = None;
                        self.input_mode = InputMode::Normal;
                        self.add_system_message("Spell color form cancelled");
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_color_browser_filter_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        match (key, modifiers) {
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                self.command_input.insert_char(c);
            }
            (KeyCode::Backspace, _) => {
                self.command_input.delete_char();
            }
            (KeyCode::Left, _) => {
                self.command_input.move_cursor_left();
            }
            (KeyCode::Right, _) => {
                self.command_input.move_cursor_right();
            }
            (KeyCode::Home, _) => {
                self.command_input.move_cursor_home();
            }
            (KeyCode::End, _) => {
                self.command_input.move_cursor_end();
            }
            (KeyCode::Enter, _) => {
                // Apply filter
                let filter_text = self.command_input.get_input().unwrap_or_default();
                if let Some(ref mut browser) = self.color_palette_browser {
                    browser.set_filter(filter_text);
                }
                self.input_mode = InputMode::ColorPaletteBrowser;
                self.command_input.clear();
            }
            (KeyCode::Esc, _) => {
                // Cancel filter
                self.input_mode = InputMode::ColorPaletteBrowser;
                self.command_input.clear();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_search_input(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        match (key, modifiers) {
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                self.search_input.insert_char(c);
            }
            (KeyCode::Backspace, _) => {
                self.search_input.delete_char();
            }
            (KeyCode::Left, _) => {
                self.search_input.move_cursor_left();
            }
            (KeyCode::Right, _) => {
                self.search_input.move_cursor_right();
            }
            (KeyCode::Home, _) => {
                self.search_input.move_cursor_home();
            }
            (KeyCode::End, _) => {
                self.search_input.move_cursor_end();
            }
            (KeyCode::Enter, _) => {
                // Execute search
                if let Some(pattern) = self.search_input.get_input() {
                    if !pattern.is_empty() {
                        if let Some(window) = self.get_focused_window() {
                            match window.start_search(&pattern) {
                                Ok(count) => {
                                    if count > 0 {
                                        self.add_system_message(&format!("Found {} matches", count));
                                    } else {
                                        self.add_system_message("No matches found");
                                    }
                                }
                                Err(e) => {
                                    self.add_system_message(&format!("Invalid regex: {}", e));
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn render_search_input(&self, area: Rect, buf: &mut Buffer) {
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Paragraph, Widget as RatatuiWidget};

        // Get search info from focused window
        let search_info = self.get_focused_window_const()
            .and_then(|w| w.search_info())
            .map(|(current, total)| format!(" [{}/{}]", current, total))
            .unwrap_or_default();

        // Create search prompt with info
        let prompt = format!("Search{}: ", search_info);
        let input_text = self.search_input.get_input().unwrap_or_default();

        let line = Line::from(vec![
            Span::styled(prompt, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(input_text),
        ]);

        // Check if command_input has borders - respect that setting for search input too
        let cmd_window = self.layout.windows.iter().find(|w| w.widget_type == "command_input");
        let show_border = cmd_window.map_or(true, |w| {
            w.show_border && w.border_style.as_deref() != Some("none")
        });

        let paragraph = if show_border {
            Paragraph::new(line)
                .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow)))
        } else {
            // No borders - just render the text
            Paragraph::new(line)
        };

        paragraph.render(area, buf);
    }

    fn get_focused_window_const(&self) -> Option<&Widget> {
        let names = self.window_manager.get_window_names();
        if self.focused_window_index < names.len() {
            self.window_manager.get_window_const(&names[self.focused_window_index])
        } else {
            None
        }
    }

    fn handle_normal_input(
        &mut self,
        key: KeyCode,
        modifiers: KeyModifiers,
        command_tx: &mpsc::UnboundedSender<String>,
    ) -> Result<()> {
        // Debug: Log ALL key events to help diagnose numpad vs regular keys
        match key {
            KeyCode::Char(c) if matches!(c, '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' | '.' | '+' | '-' | '*' | '/') => {
                debug!("KEY EVENT: KeyCode::Char('{}'), modifiers={:?}", c, modifiers);
            }
            KeyCode::Keypad0 | KeyCode::Keypad1 | KeyCode::Keypad2 | KeyCode::Keypad3 |
            KeyCode::Keypad4 | KeyCode::Keypad5 | KeyCode::Keypad6 | KeyCode::Keypad7 |
            KeyCode::Keypad8 | KeyCode::Keypad9 | KeyCode::KeypadPeriod | KeyCode::KeypadPlus |
            KeyCode::KeypadMinus | KeyCode::KeypadMultiply | KeyCode::KeypadDivide => {
                debug!("KEY EVENT: {:?}, modifiers={:?}", key, modifiers);
            }
            _ => {
                // Log non-char/numpad keys too
                debug!("KEY EVENT: {:?}, modifiers={:?}", key, modifiers);
            }
        }

        // Check if this key has a bound action (exact match first)
        if let Some(action) = self.keybind_map.get(&(key, modifiers)).cloned() {
            return self.execute_action(action, command_tx);
        }

        // For character keys with SHIFT, try without SHIFT modifier (for numpad +, -, *, /)
        // BUT: only if we don't have a specific shift+key binding
        if modifiers == KeyModifiers::SHIFT {
            if let Some(action) = self.keybind_map.get(&(key, KeyModifiers::NONE)).cloned() {
                return self.execute_action(action, command_tx);
            }
        }

        // No keybind found - if it's a printable character, insert it
        match (key, modifiers) {
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                self.command_input.insert_char(c);
            }
            _ => {
                // Key not bound and not a printable character - ignore
            }
        }

        Ok(())
    }

    fn execute_action(
        &mut self,
        action: KeyAction,
        command_tx: &mpsc::UnboundedSender<String>,
    ) -> Result<()> {
        match action {
            // Command input actions
            KeyAction::SendCommand => {
                if let Some(command) = self.command_input.submit() {
                    // Check if it's a local dot command
                    if command.starts_with('.') {
                        self.handle_dot_command(&command, Some(command_tx));
                    } else {
                        // Echo ">" with prompt color, then command with command echo color
                        let prompt_color = self.config.ui.prompt_colors
                            .iter()
                            .find(|pc| pc.character == ">")
                            .and_then(|pc| pc.fg.as_ref().or(pc.color.as_ref()))
                            .and_then(|color_str| Self::parse_hex_color(color_str))
                            .unwrap_or(Color::DarkGray);

                        let echo_color = Self::parse_hex_color(&self.config.ui.command_echo_color);

                        // Add ">" with prompt color
                        self.add_text_to_current_stream(StyledText {
                            content: ">".to_string(),
                            fg: Some(prompt_color),
                            bg: None,
                            bold: false,
                            span_type: SpanType::Normal,
                            link_data: None,
                        });

                        // Add command with echo color
                        self.add_text_to_current_stream(StyledText {
                            content: command.clone(),
                            fg: echo_color,
                            bg: None,
                            bold: false,
                            span_type: SpanType::Normal,
                            link_data: None,
                        });

                        // Finish the line so command appears before server response
                        if let Ok(size) = crossterm::terminal::size() {
                            let inner_width = size.0.saturating_sub(2);
                            self.finish_current_line(inner_width);
                        }

                        // Reset prompt_shown so next prompt will display
                        self.prompt_shown = false;

                        // Track bytes sent (+1 for newline added by network module)
                        self.perf_stats.record_bytes_sent((command.len() + 1) as u64);
                        let _ = command_tx.send(command);
                    }
                }
            }
            KeyAction::CursorLeft => {
                self.command_input.move_cursor_left();
            }
            KeyAction::CursorRight => {
                self.command_input.move_cursor_right();
            }
            KeyAction::CursorWordLeft => {
                self.command_input.move_cursor_word_left();
            }
            KeyAction::CursorWordRight => {
                self.command_input.move_cursor_word_right();
            }
            KeyAction::CursorHome => {
                self.command_input.move_cursor_home();
            }
            KeyAction::CursorEnd => {
                self.command_input.move_cursor_end();
            }
            KeyAction::CursorBackspace => {
                self.command_input.delete_char();
            }
            KeyAction::CursorDelete => {
                self.command_input.delete_word();
            }

            // History actions
            KeyAction::PreviousCommand => {
                self.command_input.history_previous();
            }
            KeyAction::NextCommand => {
                self.command_input.history_next();
            }
            KeyAction::SendLastCommand => {
                if let Some(cmd) = self.command_input.get_last_command() {
                    self.perf_stats.record_bytes_sent((cmd.len() + 1) as u64);
                    let _ = command_tx.send(cmd);
                }
            }
            KeyAction::SendSecondLastCommand => {
                if let Some(cmd) = self.command_input.get_second_last_command() {
                    self.perf_stats.record_bytes_sent((cmd.len() + 1) as u64);
                    let _ = command_tx.send(cmd);
                }
            }

            // Window actions
            KeyAction::SwitchCurrentWindow => {
                // If command input has text, try tab completion first
                if self.command_input.get_input().is_some() {
                    // Build lists of available completions
                    let available_commands = self.get_available_dot_commands();
                    let available_names = self.get_available_names();

                    // Try completion
                    self.command_input.try_complete(&available_commands, &available_names);
                } else {
                    // No text - cycle focused window as usual
                    self.cycle_focused_window();
                }
            }
            KeyAction::ScrollCurrentWindowUpOne => {
                if let Some(window) = self.get_focused_window() {
                    window.scroll_up(1);
                }
            }
            KeyAction::ScrollCurrentWindowDownOne => {
                if let Some(window) = self.get_focused_window() {
                    window.scroll_down(1);
                }
            }
            KeyAction::ScrollCurrentWindowUpPage => {
                if let Some(window) = self.get_focused_window() {
                    window.scroll_up(10);
                }
            }
            KeyAction::ScrollCurrentWindowDownPage => {
                if let Some(window) = self.get_focused_window() {
                    window.scroll_down(10);
                }
            }

            // Search actions
            KeyAction::StartSearch => {
                self.input_mode = InputMode::Search;
                self.search_input.clear();
            }
            KeyAction::NextSearchMatch => {
                if let Some(window) = self.get_focused_window() {
                    window.next_match();
                }
            }
            KeyAction::PrevSearchMatch => {
                if let Some(window) = self.get_focused_window() {
                    window.prev_match();
                }
            }
            KeyAction::ClearSearch => {
                if self.input_mode == InputMode::Search {
                    self.input_mode = InputMode::Normal;
                    if let Some(window) = self.get_focused_window() {
                        window.clear_search();
                    }
                }
            }

            // Debug/Performance actions
            KeyAction::TogglePerformanceStats => {
                self.show_perf_stats = !self.show_perf_stats;
            }

            // Macro - send literal text
            KeyAction::SendMacro(text) => {
                // Echo the command (strip \r for display)
                let display_text = text.replace('\r', "");
                if !display_text.is_empty() {
                    // Echo ">" with prompt color
                    let prompt_color = self.config.ui.prompt_colors
                        .iter()
                        .find(|pc| pc.character == ">")
                        .and_then(|pc| pc.fg.as_ref().or(pc.color.as_ref()))
                        .and_then(|color_str| Self::parse_hex_color(color_str))
                        .unwrap_or(Color::DarkGray);

                    let echo_color = Self::parse_hex_color(&self.config.ui.command_echo_color);

                    self.add_text_to_current_stream(StyledText {
                        content: ">".to_string(),
                        fg: Some(prompt_color),
                        bg: None,
                        bold: false,
                        span_type: SpanType::Normal,
                            link_data: None,
                    });

                    self.add_text_to_current_stream(StyledText {
                        content: display_text,
                        fg: echo_color,
                        bg: None,
                        bold: false,
                        span_type: SpanType::Normal,
                            link_data: None,
                    });

                    // Finish the line
                    if let Ok(size) = crossterm::terminal::size() {
                        let inner_width = size.0.saturating_sub(2);
                        self.finish_current_line(inner_width);
                    }

                    // Reset prompt_shown so next prompt will display
                    self.prompt_shown = false;
                }

                // Track bytes sent (+1 for newline added by network module)
                self.perf_stats.record_bytes_sent((text.len() + 1) as u64);
                let _ = command_tx.send(text);
            }
        }

        Ok(())
    }

    fn handle_mouse_event(
        &mut self,
        mouse: event::MouseEvent,
        window_layouts: &HashMap<String, ratatui::layout::Rect>,
        command_tx: &mpsc::UnboundedSender<String>,
    ) -> Result<()> {
        use event::{MouseButton, MouseEventKind};

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Window editor drag handling (highest priority)
                if self.input_mode == InputMode::WindowEditor {
                    if self.window_editor.handle_mouse(mouse.column, mouse.row, true) {
                        return Ok(());
                    }
                }

                // Settings editor drag handling (second highest priority)
                if self.input_mode == InputMode::SettingsEditor {
                    if let Some(ref mut editor) = self.settings_editor {
                        let terminal_area = Rect {
                            x: 0,
                            y: 0,
                            width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                            height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                        };
                        if editor.handle_mouse(mouse.column, mouse.row, true, terminal_area) {
                            return Ok(());
                        }
                    }
                }

                // Highlight browser drag handling (third highest priority)
                if self.input_mode == InputMode::HighlightBrowser {
                    if let Some(ref mut browser) = self.highlight_browser {
                        let terminal_area = Rect {
                            x: 0,
                            y: 0,
                            width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                            height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                        };
                        if browser.handle_mouse(mouse.column, mouse.row, true, terminal_area) {
                            return Ok(());
                        }
                    }
                }

                // Keybind browser drag handling (fourth highest priority)
                if self.input_mode == InputMode::KeybindBrowser {
                    if let Some(ref mut browser) = self.keybind_browser {
                        let terminal_area = Rect {
                            x: 0,
                            y: 0,
                            width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                            height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                        };
                        if browser.handle_mouse(mouse.column, mouse.row, true, terminal_area) {
                            return Ok(());
                        }
                    }
                }

                // Color palette browser drag handling (fourth highest priority)
                if self.input_mode == InputMode::ColorPaletteBrowser {
                    if let Some(ref mut browser) = self.color_palette_browser {
                        let terminal_area = Rect {
                            x: 0,
                            y: 0,
                            width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                            height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                        };
                        if browser.handle_mouse(mouse.column, mouse.row, true, terminal_area) {
                            return Ok(());
                        }
                    }
                }

                // Color form drag handling (fourth highest priority)
                if self.input_mode == InputMode::ColorForm {
                    if let Some(ref mut form) = self.color_form {
                        let terminal_area = Rect {
                            x: 0,
                            y: 0,
                            width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                            height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                        };
                        if form.handle_mouse(mouse.column, mouse.row, true, terminal_area) {
                            return Ok(());
                        }
                    }
                }

                // Highlight form drag handling (fifth highest priority)
                if self.input_mode == InputMode::HighlightForm {
                    if let Some(ref mut form) = self.highlight_form {
                        let terminal_area = Rect {
                            x: 0,
                            y: 0,
                            width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                            height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                        };
                        if form.handle_mouse(mouse.column, mouse.row, true, terminal_area) {
                            return Ok(());
                        }
                    }
                }

                // Keybind form drag handling (fifth highest priority)
                if self.input_mode == InputMode::KeybindForm {
                    if let Some(ref mut form) = self.keybind_form {
                        let terminal_area = Rect {
                            x: 0,
                            y: 0,
                            width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                            height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                        };
                        if form.handle_mouse(mouse.column, mouse.row, true, terminal_area) {
                            return Ok(());
                        }
                    }
                }

                // If Shift is held, let native terminal handle selection (passthrough)
                if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                    return Ok(());
                }

                // Get terminal area for menu calculations
                let terminal_area = Rect {
                    x: 0,
                    y: 0,
                    width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                    height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                };

                // Check if clicking on nested submenu first (highest priority)
                if let Some(ref nested_submenu) = self.nested_submenu {
                    let nested_submenu_rect = Self::calculate_menu_rect(nested_submenu, terminal_area);

                    if let Some(item_idx) = nested_submenu.check_click(mouse.column, mouse.row, nested_submenu_rect) {
                        // Clicked on a nested submenu item
                        if let Some(command) = nested_submenu.get_items().get(item_idx).map(|i| i.command.clone()) {
                            debug!("Nested submenu item {} selected: {}", item_idx, command);
                            command_tx.send(command)?;
                            self.popup_menu = None;
                            self.submenu = None;
                            self.nested_submenu = None;
                            return Ok(());
                        }
                    } else if mouse.column < nested_submenu_rect.x || mouse.column >= nested_submenu_rect.x + nested_submenu_rect.width
                        || mouse.row < nested_submenu_rect.y || mouse.row >= nested_submenu_rect.y + nested_submenu_rect.height {
                        // Clicked outside nested submenu - close it (keep main menu and submenu)
                        debug!("Clicked outside nested submenu, closing nested submenu");
                        self.nested_submenu = None;
                        return Ok(());
                    }
                }

                // Check if clicking on submenu (second priority)
                if let Some(ref submenu) = self.submenu {
                    let submenu_rect = Self::calculate_menu_rect(submenu, terminal_area);

                    if let Some(item_idx) = submenu.check_click(mouse.column, mouse.row, submenu_rect) {
                        // Clicked on a submenu item
                        if let Some(command) = submenu.get_items().get(item_idx).map(|i| i.command.clone()) {
                            // Check if this is a nested submenu (e.g., swear under roleplay)
                            if command.starts_with("__SUBMENU__") {
                                let cat = command.strip_prefix("__SUBMENU__").unwrap();
                                debug!("Opening nested submenu for category: {}", cat);
                                debug!("Available categories: {:?}", self.menu_categories.keys().collect::<Vec<_>>());
                                if let Some(items) = self.menu_categories.get(cat) {
                                    debug!("Found {} items for category {}", items.len(), cat);
                                    // Open nested submenu, keep both main menu and submenu
                                    let nested_submenu_pos = (submenu_rect.x + 5, submenu_rect.y + item_idx as u16 + 1);
                                    self.nested_submenu = Some(crate::ui::PopupMenu::new(items.clone(), nested_submenu_pos));
                                } else {
                                    debug!("ERROR: Category '{}' not found in menu_categories!", cat);
                                }
                                return Ok(());
                            }

                            debug!("Submenu item {} selected: {}", item_idx, command);
                            command_tx.send(command)?;
                            self.popup_menu = None;
                            self.submenu = None;
                            return Ok(());
                        }
                    } else if mouse.column < submenu_rect.x || mouse.column >= submenu_rect.x + submenu_rect.width
                        || mouse.row < submenu_rect.y || mouse.row >= submenu_rect.y + submenu_rect.height {
                        // Clicked outside submenu - close it (keep main menu)
                        debug!("Clicked outside submenu, closing submenu");
                        self.submenu = None;
                        self.nested_submenu = None; // Also close nested if open
                        return Ok(());
                    }
                }

                // Check if clicking on main popup menu
                if let Some(ref menu) = self.popup_menu {
                    let menu_rect = Self::calculate_menu_rect(menu, terminal_area);

                    if let Some(item_idx) = menu.check_click(mouse.column, mouse.row, menu_rect) {
                        // Clicked on a menu item
                        if let Some(command) = menu.get_items().get(item_idx).map(|i| i.command.clone()) {
                            // Check if this is a submenu
                            if command.starts_with("__SUBMENU__") {
                                let cat = command.strip_prefix("__SUBMENU__").unwrap();
                                debug!("Opening submenu for category: {}", cat);
                                debug!("Available categories: {:?}", self.menu_categories.keys().collect::<Vec<_>>());
                                if let Some(items) = self.menu_categories.get(cat) {
                                    debug!("Found {} items for category {}", items.len(), cat);
                                    // Open submenu at slightly offset position
                                    let submenu_pos = (menu_rect.x + 5, menu_rect.y + item_idx as u16 + 1);
                                    self.submenu = Some(crate::ui::PopupMenu::new(items.clone(), submenu_pos));
                                } else {
                                    debug!("ERROR: Category '{}' not found in menu_categories!", cat);
                                }
                                return Ok(());
                            }

                            debug!("Menu item {} selected: {}", item_idx, command);
                            command_tx.send(command)?;
                            self.popup_menu = None;
                            self.submenu = None;
                            self.nested_submenu = None;
                            return Ok(());
                        }
                    } else if mouse.column < menu_rect.x || mouse.column >= menu_rect.x + menu_rect.width
                        || mouse.row < menu_rect.y || mouse.row >= menu_rect.y + menu_rect.height {
                        // Clicked outside menu - close all menus
                        debug!("Clicked outside menu, closing all menus");
                        self.popup_menu = None;
                        self.submenu = None;
                        self.nested_submenu = None;
                        return Ok(());
                    }
                }

                // Clear any existing selection on new click
                self.selection_state = None;
                // Check if clicking on a resize border
                if let Some((window_idx, edge)) = self.check_resize_border(mouse.column, mouse.row, window_layouts) {
                    // Check if window is locked
                    if !self.is_window_locked(window_idx) {
                        self.resize_state = Some(ResizeState {
                            window_index: window_idx,
                            edge,
                            start_mouse_pos: (mouse.column, mouse.row),
                        });
                        debug!("Started resize on window {} edge {:?}", window_idx, edge);
                    } else {
                        debug!("Window {} is locked - cannot resize", window_idx);
                    }
                } else if let Some(window_idx) = self.check_title_bar(mouse.column, mouse.row, window_layouts) {
                    // Clicking on title bar - start move operation
                    if !self.is_window_locked(window_idx) {
                        let window_names = self.window_manager.get_window_names();
                        if window_idx < window_names.len() {
                            let window_name = &window_names[window_idx];
                            if let Some(rect) = window_layouts.get(window_name) {
                                self.move_state = Some(MoveState {
                                    window_index: window_idx,
                                    start_mouse_pos: (mouse.column, mouse.row),
                                });
                                debug!("Started move on window {} at {:?}", window_idx, (rect.x, rect.y));
                            }
                        }
                    } else {
                        debug!("Window {} is locked - cannot move", window_idx);
                    }
                } else {
                    // Not on border or title bar, check which window was clicked
                    debug!("Click at ({}, {}) - checking windows", mouse.column, mouse.row);

                    // Track this position for potential text selection drag
                    debug!("Setting selection_drag_start to ({}, {})", mouse.column, mouse.row);
                    self.selection_drag_start = Some((mouse.column, mouse.row));

                    for (idx, name) in self.window_manager.get_window_names().iter().enumerate() {
                        if let Some(rect) = window_layouts.get(name) {
                            debug!("  Window '{}': rect {:?}", name, rect);
                            if mouse.column >= rect.x
                                && mouse.column < rect.x + rect.width
                                && mouse.row >= rect.y
                                && mouse.row < rect.y + rect.height
                            {
                                self.focused_window_index = idx;
                                debug!("Clicked window '{}' (index {})", name, idx);

                                // Check if this is a tabbed window and if we clicked on a tab
                                if let Some(widget) = self.window_manager.get_window(name) {
                                    if let Widget::Tabbed(tabbed) = widget {
                                        // Use the tabbed window's method to get correct tab bar rect
                                        let tab_bar_rect = tabbed.get_tab_bar_rect(*rect);
                                        debug!("Tabbed window '{}': click at ({}, {}), tab_bar_rect: {:?}",
                                            name, mouse.column, mouse.row, tab_bar_rect);

                                        // Check if click was on tab bar
                                        if mouse.row >= tab_bar_rect.y && mouse.row < tab_bar_rect.y + tab_bar_rect.height {
                                            debug!("Click is on tab bar row");
                                            if let Some(tab_idx) = tabbed.get_tab_at_position(mouse.column, tab_bar_rect) {
                                                debug!("Clicked tab {} in window '{}'", tab_idx, name);
                                                tabbed.switch_to_tab(tab_idx);
                                            } else {
                                                debug!("get_tab_at_position returned None");
                                            }
                                        }
                                    }
                                }

                                // Check if we clicked on a link (text window or tabbed window)
                                // Do this after tab switching check
                                if let Some(widget) = self.window_manager.get_window_const(name) {
                                    // Determine if this is a tabbed window and calculate adjusted rect
                                    let (text_window, adjusted_rect) = match widget {
                                        Widget::Text(tw) => (Some(tw), *rect),
                                        Widget::Tabbed(tabbed) => {
                                            // For tabbed windows, get the active tab's text window
                                            // But first check we didn't click on the tab bar itself
                                            let tab_bar_rect = tabbed.get_tab_bar_rect(*rect);
                                            if mouse.row >= tab_bar_rect.y && mouse.row < tab_bar_rect.y + tab_bar_rect.height {
                                                (None, *rect) // Clicked on tab bar, don't check for links
                                            } else {
                                                // Calculate adjusted rect accounting for outer border and tab bar
                                                let has_outer_border = self.layout.windows.get(idx)
                                                    .map(|w| w.show_border)
                                                    .unwrap_or(false);

                                                let tab_bar_at_top = self.layout.windows.get(idx)
                                                    .and_then(|w| w.tab_bar_position.as_ref())
                                                    .map(|pos| pos == "top")
                                                    .unwrap_or(true);

                                                let outer_border_offset = if has_outer_border { 1 } else { 0 };
                                                let tab_bar_height = 1;

                                                let y_offset = if tab_bar_at_top {
                                                    outer_border_offset + tab_bar_height
                                                } else {
                                                    outer_border_offset
                                                };

                                                let adjusted = Rect {
                                                    x: rect.x + outer_border_offset,
                                                    y: rect.y + y_offset as u16,
                                                    width: rect.width.saturating_sub(2 * outer_border_offset as u16),
                                                    height: rect.height.saturating_sub((2 * outer_border_offset + tab_bar_height) as u16),
                                                };

                                                (tabbed.get_active_window(), adjusted)
                                            }
                                        }
                                        _ => (None, *rect),
                                    };

                                    if let Some(text_window) = text_window {
                                        debug!("Mouse down on text window '{}' at ({}, {})", name, mouse.column, mouse.row);
                                        if let Some(link_data) = Self::link_at_position(text_window, mouse.column, mouse.row, adjusted_rect) {
                                            debug!("Clicked link span: noun='{}' exist_id='{}'", link_data.noun, link_data.exist_id);
                                                // Check if the required modifier key is held for drag and drop
                                                let drag_modifier = self.config.ui.drag_modifier_key.to_lowercase();
                                                let has_modifier = match drag_modifier.as_str() {
                                                    "ctrl" => mouse.modifiers.contains(KeyModifiers::CONTROL),
                                                    "alt" => mouse.modifiers.contains(KeyModifiers::ALT),
                                                    "shift" => mouse.modifiers.contains(KeyModifiers::SHIFT),
                                                    "none" => true,  // No modifier required
                                                    _ => false,
                                                };

                                                if has_modifier {
                                                    debug!("Starting drag for link exist_id={} (modifier: {})", link_data.exist_id, drag_modifier);
                                                    // Start drag operation
                                                    self.drag_state = Some(DragState {
                                                        link_data: link_data.clone(),
                                                        start_pos: (mouse.column, mouse.row),
                                                        current_pos: (mouse.column, mouse.row),
                                                    });
                                                    // Clear selection drag start since we're dragging an object
                                                    self.selection_drag_start = None;
                                                } else {
                                                    debug!("Opening menu for link exist_id={} (no {} modifier held)", link_data.exist_id, drag_modifier);
                                                    // No modifier held - immediately request menu
                                                    if let Err(e) = self.request_menu(&link_data.exist_id, &link_data.noun, Some(command_tx)) {
                                                        self.add_system_message(&format!("Failed to request menu: {}", e));
                                                    } else {
                                                        // Store the click position for positioning the menu when it arrives
                                                        self.last_link_click_pos = Some((mouse.column, mouse.row));
                                                    }
                                                    // Clear selection drag start since we're opening a menu
                                                    self.selection_drag_start = None;
                                                }
                                        } else {
                                            debug!("No link at click position");
                                        }
                                    }
                                }

                                break;
                            }
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // Window editor drag release (highest priority)
                if self.input_mode == InputMode::WindowEditor && self.window_editor.is_dragging {
                    self.window_editor.handle_mouse(mouse.column, mouse.row, false);
                    return Ok(());
                }

                // Settings editor drag release (second priority)
                if self.input_mode == InputMode::SettingsEditor {
                    if let Some(ref mut editor) = self.settings_editor {
                        if editor.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            editor.handle_mouse(mouse.column, mouse.row, false, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Highlight browser drag release (third priority)
                if self.input_mode == InputMode::HighlightBrowser {
                    if let Some(ref mut browser) = self.highlight_browser {
                        if browser.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            browser.handle_mouse(mouse.column, mouse.row, false, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Keybind browser drag release (fourth priority)
                if self.input_mode == InputMode::KeybindBrowser {
                    if let Some(ref mut browser) = self.keybind_browser {
                        if browser.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            browser.handle_mouse(mouse.column, mouse.row, false, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Color palette browser drag release (fourth priority)
                if self.input_mode == InputMode::ColorPaletteBrowser {
                    if let Some(ref mut browser) = self.color_palette_browser {
                        if browser.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            browser.handle_mouse(mouse.column, mouse.row, false, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Color form drag release (fourth priority)
                if self.input_mode == InputMode::ColorForm {
                    if let Some(ref mut form) = self.color_form {
                        if form.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            form.handle_mouse(mouse.column, mouse.row, false, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Highlight form drag release (fourth priority)
                if self.input_mode == InputMode::HighlightForm {
                    if let Some(ref mut form) = self.highlight_form {
                        if form.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            form.handle_mouse(mouse.column, mouse.row, false, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Keybind form drag release (fifth priority)
                if self.input_mode == InputMode::KeybindForm {
                    if let Some(ref mut form) = self.keybind_form {
                        if form.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            form.handle_mouse(mouse.column, mouse.row, false, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Handle drag and drop completion (highest priority)
                if let Some(drag_state) = self.drag_state.take() {
                    debug!("Mouse up with active drag state for {}", drag_state.link_data.noun);

                    // Compute using actual release position (MouseUp), not only tracked drag updates
                    let release_col = mouse.column;
                    let release_row = mouse.row;

                    // Check if this was a drag or just a click
                    let dx = (release_col as i16 - drag_state.start_pos.0 as i16).abs();
                    let dy = (release_row as i16 - drag_state.start_pos.1 as i16).abs();
                    let drag_threshold = 2; // Minimum pixels to count as a drag

                    if dx > drag_threshold || dy > drag_threshold {
                        // This was a drag - find drop target
                        debug!("Detected drag movement: dx={}, dy={} for {}", dx, dy, drag_state.link_data.noun);

                        // Find what window/link is at the drop position
                        let mut drop_target_id: Option<String> = None;

                        for name in self.window_manager.get_window_names() {
                            if let Some(rect) = window_layouts.get(&name) {
                                if release_col >= rect.x
                                    && release_col < rect.x + rect.width
                                    && release_row >= rect.y
                                    && release_row < rect.y + rect.height
                                {
                                    if let Some(widget) = self.window_manager.get_window(&name) {
                                        if let Widget::Text(text_window) = widget {
                                            // Check if we dropped on a link
                                                if let Some(target_link) = Self::link_at_position(
                                                    text_window,
                                                    release_col,
                                                    release_row,
                                                    *rect,
                                                ) {
                                                    drop_target_id = Some(target_link.exist_id.clone());
                                                    debug!(
                                                        "Dropped {} onto link target id={} (noun={})",
                                                        drag_state.link_data.noun,
                                                        target_link.exist_id,
                                                        target_link.noun
                                                    );
                                                    break;
                                                }
                                        }
                                    }
                                }
                            }
                        }

                        // Generate command using exist ids
                        let source_id = drag_state.link_data.exist_id;
                        let command = if let Some(target_id) = drop_target_id {
                            format!("_drag #{} #{}", source_id, target_id)
                        } else {
                            format!("_drag #{} drop", source_id)
                        };

                        debug!("Sending drag-drop command: {}", command);

                        // Send command to game
                        let _ = command_tx.send(command);
                    } else {
                        // No significant drag - do nothing (menu was already opened on mouse down if no modifier was held)
                        debug!("No significant drag detected for {} - ignoring (menu already opened if applicable)", drag_state.link_data.noun);
                    }
                }

                // Copy selection to clipboard if we have one
                else if let Some(ref selection) = self.selection_state {
                    debug!("Mouse up with selection: active={}, empty={}, start=({},{},{}), end=({},{},{})",
                        selection.active, selection.is_empty(),
                        selection.start.window_index, selection.start.line, selection.start.col,
                        selection.end.window_index, selection.end.line, selection.end.col);
                    if selection.active && !selection.is_empty() {
                        debug!("Copying selection to clipboard");
                        self.copy_selection_to_clipboard(window_layouts);
                    }
                } else {
                    debug!("Mouse up with no selection state");
                }

                // Clear drag tracking
                self.selection_drag_start = None;

                // End resize or move operation
                if self.resize_state.is_some() {
                    debug!("Ended resize operation");
                    self.resize_state = None;
                }
                if self.move_state.is_some() {
                    debug!("Ended move operation");
                    self.move_state = None;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Window editor dragging (highest priority)
                if self.input_mode == InputMode::WindowEditor && self.window_editor.is_dragging {
                    self.window_editor.handle_mouse(mouse.column, mouse.row, true);
                    return Ok(());
                }

                // Settings editor dragging (second priority)
                if self.input_mode == InputMode::SettingsEditor {
                    if let Some(ref mut editor) = self.settings_editor {
                        if editor.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            editor.handle_mouse(mouse.column, mouse.row, true, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Highlight browser dragging (third priority)
                if self.input_mode == InputMode::HighlightBrowser {
                    if let Some(ref mut browser) = self.highlight_browser {
                        if browser.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            browser.handle_mouse(mouse.column, mouse.row, true, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Keybind browser dragging (fourth priority)
                if self.input_mode == InputMode::KeybindBrowser {
                    if let Some(ref mut browser) = self.keybind_browser {
                        if browser.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            browser.handle_mouse(mouse.column, mouse.row, true, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Color palette browser dragging (fourth priority)
                if self.input_mode == InputMode::ColorPaletteBrowser {
                    if let Some(ref mut browser) = self.color_palette_browser {
                        if browser.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            browser.handle_mouse(mouse.column, mouse.row, true, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Color form dragging (fourth priority)
                if self.input_mode == InputMode::ColorForm {
                    if let Some(ref mut form) = self.color_form {
                        if form.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            form.handle_mouse(mouse.column, mouse.row, true, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Highlight form dragging (fourth priority)
                if self.input_mode == InputMode::HighlightForm {
                    if let Some(ref mut form) = self.highlight_form {
                        if form.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            form.handle_mouse(mouse.column, mouse.row, true, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Keybind form dragging (fifth priority)
                if self.input_mode == InputMode::KeybindForm {
                    if let Some(ref mut form) = self.keybind_form {
                        if form.is_dragging {
                            let terminal_area = Rect {
                                x: 0,
                                y: 0,
                                width: window_layouts.values().map(|r| r.x + r.width).max().unwrap_or(80),
                                height: window_layouts.values().map(|r| r.y + r.height).max().unwrap_or(24),
                            };
                            form.handle_mouse(mouse.column, mouse.row, true, terminal_area);
                            return Ok(());
                        }
                    }
                }

                // Handle active drag and drop (highest priority)
                if let Some(ref mut drag_state) = self.drag_state {
                    drag_state.current_pos = (mouse.column, mouse.row);
                    debug!("Dragging {} to ({}, {})", drag_state.link_data.noun, mouse.column, mouse.row);
                }
                // Handle active resize
                else if let Some(ref state) = self.resize_state.clone() {
                    let delta_cols = mouse.column as i16 - state.start_mouse_pos.0 as i16;
                    let delta_rows = mouse.row as i16 - state.start_mouse_pos.1 as i16;

                    match state.edge {
                        ResizeEdge::Top | ResizeEdge::Bottom => {
                            if delta_rows != 0 {
                                self.resize_window(state.window_index, state.edge, delta_rows, 0);
                                // Update start position for next delta
                                if let Some(ref mut rs) = self.resize_state {
                                    rs.start_mouse_pos.1 = mouse.row;
                                }
                            }
                        }
                        ResizeEdge::Left | ResizeEdge::Right => {
                            if delta_cols != 0 {
                                self.resize_window(state.window_index, state.edge, 0, delta_cols);
                                // Update start position for next delta
                                if let Some(ref mut rs) = self.resize_state {
                                    rs.start_mouse_pos.0 = mouse.column;
                                }
                            }
                        }
                    }
                } else if let Some(ref state) = self.move_state.clone() {
                    // Handle active move
                    let delta_cols = mouse.column as i16 - state.start_mouse_pos.0 as i16;
                    let delta_rows = mouse.row as i16 - state.start_mouse_pos.1 as i16;

                    if delta_cols != 0 || delta_rows != 0 {
                        self.move_window(state.window_index, delta_cols, delta_rows);
                        // Update start position for next delta
                        if let Some(ref mut ms) = self.move_state {
                            ms.start_mouse_pos.0 = mouse.column;
                            ms.start_mouse_pos.1 = mouse.row;
                        }
                    }
                } else if let Some(drag_start) = self.selection_drag_start {
                    debug!("Text selection drag detected at ({}, {})", mouse.column, mouse.row);
                    // No resize/move active - handle text selection drag
                    // Start selection if we haven't yet
                    if self.selection_state.is_none() {
                        if let Some((window_idx, line, col)) = self.mouse_to_text_position(drag_start.0, drag_start.1, window_layouts) {
                            debug!("Starting text selection at window {} line {} col {}", window_idx, line, col);
                            self.selection_state = Some(SelectionState::new(window_idx, line, col));
                        } else {
                            debug!("Failed to get text position for drag start ({}, {})", drag_start.0, drag_start.1);
                        }
                    }

                    // Update selection end position
                    // Compute position first to avoid borrow conflicts
                    let mouse_pos = self.mouse_to_text_position(mouse.column, mouse.row, window_layouts);
                    if let Some(ref mut selection) = self.selection_state {
                        if let Some((window_idx, line, col)) = mouse_pos {
                            debug!("Updating selection end to window {} line {} col {}", window_idx, line, col);
                            selection.update_end(window_idx, line, col);
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                // Scroll the window under the cursor
                for name in self.window_manager.get_window_names() {
                    if let Some(rect) = window_layouts.get(&name) {
                        if mouse.column >= rect.x
                            && mouse.column < rect.x + rect.width
                            && mouse.row >= rect.y
                            && mouse.row < rect.y + rect.height
                        {
                            if let Some(window) = self.window_manager.get_window(&name) {
                                window.scroll_up(3);
                                debug!("Scrolled up window '{}'", name);
                            }
                            break;
                        }
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                // Scroll the window under the cursor
                for name in self.window_manager.get_window_names() {
                    if let Some(rect) = window_layouts.get(&name) {
                        if mouse.column >= rect.x
                            && mouse.column < rect.x + rect.width
                            && mouse.row >= rect.y
                            && mouse.row < rect.y + rect.height
                        {
                            if let Some(window) = self.window_manager.get_window(&name) {
                                window.scroll_down(3);
                                debug!("Scrolled down window '{}'", name);
                            }
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    // [removed] unused helper
    /* fn extract_word_at_position(
        text_window: &crate::ui::TextWindow,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: ratatui::layout::Rect,
    ) -> Option<String> {
        // Calculate the relative position within the window (accounting for borders)
        let border_offset = if text_window.has_border() { 1 } else { 0 };

        // Check if click is within the window content area
        if mouse_col < window_rect.x + border_offset || mouse_col >= window_rect.x + window_rect.width - border_offset {
            tracing::debug!("Click outside horizontal bounds");
            return None;
        }
        if mouse_row < window_rect.y + border_offset || mouse_row >= window_rect.y + window_rect.height - border_offset {
            tracing::debug!("Click outside vertical bounds");
            return None;
        }

        // Calculate visible height and get only visible lines
        let visible_height = (window_rect.height.saturating_sub(2 * border_offset)) as usize;
        let (_start_idx, visible_lines) = text_window.get_visible_lines_info(visible_height);

        // Calculate line index relative to visible lines (not all lines)
        let line_idx = (mouse_row - window_rect.y - border_offset) as usize;
        let col_offset = (mouse_col - window_rect.x - border_offset) as usize;

        tracing::debug!("Visible lines: {}, line_idx: {}, col_offset: {}",
            visible_lines.len(), line_idx, col_offset);

        // Get the line at this position
        if line_idx >= visible_lines.len() {
            tracing::debug!("Line index {} out of range (visible lines: {})", line_idx, visible_lines.len());
            return None;
        }

        let line = &visible_lines[line_idx];

        // Build the full text of the line from segments
        let mut full_text = String::new();
        for segment in &line.segments {
            full_text.push_str(&segment.text);
        }

        tracing::debug!("Line text: '{}'", full_text);

        // Find the word boundaries at the click position
        let chars: Vec<char> = full_text.chars().collect();
        if col_offset >= chars.len() {
            tracing::debug!("Column offset {} out of range (line length: {})", col_offset, chars.len());
            return None;
        }

        // Find start of word (go left until whitespace or punctuation)
        let mut start = col_offset;
        while start > 0 && chars[start - 1].is_alphanumeric() {
            start -= 1;
        }

        // Find end of word (go right until whitespace or punctuation)
        let mut end = col_offset;
        while end < chars.len() && chars[end].is_alphanumeric() {
            end += 1;
        }

        if start >= end {
            tracing::debug!("No word at position (start={}, end={})", start, end);
            return None;
        }

        let word: String = chars[start..end].iter().collect();
        tracing::debug!("Found word: '{}'", word);
        Some(word)
    }
    */
    /// Find a link (by precise span) at a given mouse position in a text window
    fn link_at_position(
        text_window: &crate::ui::TextWindow,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: ratatui::layout::Rect,
    ) -> Option<crate::ui::LinkData> {
        let border_offset = if text_window.has_border() { 1 } else { 0 };

        // Bounds check within content area
        if mouse_col < window_rect.x + border_offset
            || mouse_col >= window_rect.x + window_rect.width - border_offset
            || mouse_row < window_rect.y + border_offset
            || mouse_row >= window_rect.y + window_rect.height - border_offset
        {
            return None;
        }

        let visible_height = (window_rect.height.saturating_sub(2 * border_offset)) as usize;
        let (_start_idx, visible_lines) = text_window.get_visible_lines_info(visible_height);

        let line_idx = (mouse_row - window_rect.y - border_offset) as usize;
        let col_offset = (mouse_col - window_rect.x - border_offset) as usize;

        if line_idx >= visible_lines.len() {
            return None;
        }

        let line = &visible_lines[line_idx];
        let mut col = 0usize;
        for seg in &line.segments {
            let seg_len = seg.text.chars().count();
            if col_offset >= col && col_offset < col + seg_len {
                // Inside this segment
                if let Some(link) = &seg.link_data {
                    return Some(link.clone());
                }
                return None;
            }
            col += seg_len;
        }

        None
    }

    fn handle_server_message(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::Connected => {
                info!("Connected to server");
                self.add_system_message("Connected to Lich");
            }
            ServerMessage::Disconnected => {
                info!("Disconnected from server");
                self.add_system_message("Disconnected from Lich");
                self.running = false;
            }
            ServerMessage::Text(line) => {
                // Track network bytes received
                self.perf_stats.record_bytes_received(line.len() as u64);

                // Handle empty lines BEFORE parsing (like ProfanityFE)
                // Empty lines should create blank display lines
                if line.is_empty() {
                    self.add_text_to_current_stream(StyledText {
                        content: String::new(),
                        fg: None,
                        bg: None,
                        bold: false,
                        span_type: SpanType::Normal,
                            link_data: None,
                    });
                    if let Ok(size) = crossterm::terminal::size() {
                        let inner_width = size.0.saturating_sub(2);
                        self.finish_current_line(inner_width);
                    }
                    return;
                }

                // Parse XML and add to window (with timing)
                let parse_start = std::time::Instant::now();
                let elements = self.parser.parse_line(&line);
                let parse_duration = parse_start.elapsed();
                self.perf_stats.record_parse(parse_duration);
                self.perf_stats.record_elements_parsed(elements.len() as u64);

                // Check if this line has Text elements with actual content (not just empty strings)
                let has_text = elements.iter().any(|e| {
                    if let ParsedElement::Text { content, .. } = e {
                        !content.trim().is_empty()
                    } else {
                        false
                    }
                });
                let _has_prompt = elements.iter().any(|e| matches!(e, ParsedElement::Prompt { .. }));

                // Extract server time from this chunk's prompt (if present)
                // Calculate offset between server time and local time
                for element in &elements {
                    if let ParsedElement::Prompt { time, .. } = element {
                        if let Ok(server_time) = time.parse::<u64>() {
                            let local_time = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            let offset = (server_time as i64) - (local_time as i64);
                            self.server_time_offset = offset;
                            // Commented out - too spammy (logs every prompt)
                            // debug!("Server time offset: {}s (server={}, local={})", offset, server_time, local_time);
                        }
                    }
                }

                for element in elements {
                    match element {
                        ParsedElement::Text { content, fg_color, bg_color, bold, span_type, link_data, .. } => {
                            // Skip text if we're discarding the current stream (no window exists for it)
                            if self.discard_current_stream {
                                continue;
                            }

                            // Special handling for target/player streams
                            match self.current_stream.as_str() {
                                "targetcount" => {
                                    // Parse target count: "[05]"
                                    let count_text = content.trim().trim_matches(&['[', ']'][..]);
                                    if let Ok(count) = count_text.trim().parse::<u32>() {
                                        if let Some(widget) = self.window_manager.get_window("targets") {
                                            widget.set_target_count(count);
                                        }
                                    }
                                }
                                "playercount" => {
                                    // Parse player count: "[03]"
                                    let count_text = content.trim().trim_matches(&['[', ']'][..]);
                                    if let Ok(count) = count_text.trim().parse::<u32>() {
                                        if let Some(widget) = self.window_manager.get_window("players") {
                                            widget.set_player_count(count);
                                        }
                                    }
                                }
                                "combat" | "playerlist" => {
                                    // Accumulate text in buffer (will be parsed on StreamPop)
                                    self.stream_buffer.push_str(&content);
                                }
                                _ => {
                                    // Normal text handling for other streams
                                    if !content.is_empty() {
                                        // Check for sound triggers in highlights
                                        self.check_sound_triggers(&content);

                                        self.add_text_to_current_stream(StyledText {
                                            content: content.clone(),
                                            fg: fg_color.and_then(|c| Self::parse_hex_color(&c)),
                                            bg: bg_color.and_then(|c| Self::parse_hex_color(&c)),
                                            bold,
                                            span_type,
                                            link_data,
                                        });
                                        // Reset prompt_shown flag when we see actual text content (not just whitespace)
                                        if !content.trim().is_empty() {
                                            self.prompt_shown = false;
                                        }
                                    }
                                }
                            }
                        }
                        ParsedElement::Prompt { text, .. } => {
                            // Skip this prompt if we just returned from a non-main stream
                            if self.skip_next_prompt {
                                debug!("Skipping prompt after stream pop");
                                self.skip_next_prompt = false;
                                continue;
                            }

                            // Show prompt if:
                            // 1. Line has text (show prompt after text), OR
                            // 2. Line has no text AND we haven't shown a prompt yet since last text
                            let should_show = !text.trim().is_empty() &&
                                              (has_text || !self.prompt_shown);

                            if should_show {
                                // Color each character in the prompt based on configuration
                                for ch in text.chars() {
                                    let char_str = ch.to_string();

                                    // Find matching color for this character
                                    let color = self.config.ui.prompt_colors
                                        .iter()
                                        .find(|pc| pc.character == char_str)
                                        .and_then(|pc| {
                                            // Commented out - too spammy (logs every prompt)
                                            // let fg = pc.fg.as_ref().or(pc.color.as_ref()).map(|s| s.as_str()).unwrap_or("none");
                                            // debug!("Matched prompt char '{}' to fg color {}", char_str, fg);
                                            pc.fg.as_ref().or(pc.color.as_ref()).and_then(|color_str| Self::parse_hex_color(color_str))
                                        })
                                        .unwrap_or_else(|| {
                                            debug!("No match for prompt char '{}', using default", char_str);
                                            Color::DarkGray
                                        });

                                    self.add_text_to_current_stream(StyledText {
                                        content: char_str,
                                        fg: Some(color),
                                        bg: None,
                                        bold: false,
                                        span_type: SpanType::Normal,
                            link_data: None,
                                    });
                                }
                                self.prompt_shown = true;
                            }
                        }
                        ParsedElement::StreamPush { id } => {
                            // Switch to new stream
                            // debug!("Pushing stream: {}", id);  // Commented out - too spammy
                            self.current_stream = id.clone();

                            // Check if a window exists for this stream
                            // If not, discard all text until StreamPop
                            if !self.window_manager.has_window_for_stream(&id) {
                                debug!("No window exists for stream '{}', discarding text", id);
                                self.discard_current_stream = true;
                            } else {
                                self.discard_current_stream = false;
                            }

                            // Clear stream buffer for accumulation streams
                            match id.as_str() {
                                "combat" | "playerlist" => {
                                    self.stream_buffer.clear();
                                }
                                _ => {}
                            }
                        }
                        ParsedElement::StreamPop => {
                            // Return to main stream
                            // debug!("Popping stream, returning to main");  // Commented out - too spammy

                            // Process buffered stream content before popping
                            match self.current_stream.as_str() {
                                "combat" => {
                                    // Parse complete target list
                                    if let Some(widget) = self.window_manager.get_window("targets") {
                                        widget.set_targets_from_text(&self.stream_buffer);
                                    }
                                    self.stream_buffer.clear();
                                }
                                "playerlist" => {
                                    // Parse complete player list
                                    if let Some(widget) = self.window_manager.get_window("players") {
                                        widget.set_players_from_text(&self.stream_buffer);
                                    }
                                    self.stream_buffer.clear();
                                }
                                _ => {}
                            }

                            // Only skip the next prompt if the stream was routed to a non-main window
                            // If the stream fell back to main (no dedicated window), keep the prompt
                            let stream_window = self.window_manager
                                .stream_map
                                .get(&self.current_stream)
                                .cloned()
                                .unwrap_or_else(|| "main".to_string());

                            if stream_window != "main" {
                                // Stream was routed elsewhere, skip the duplicate prompt
                                self.skip_next_prompt = true;
                            }

                            // Reset discard flag when returning to main stream
                            self.discard_current_stream = false;
                            self.current_stream = "main".to_string();
                        }
                        ParsedElement::ProgressBar { id, value, max, text } => {
                            // Update progress bar if we have a window with this ID
                            // The game sends different formats:
                            // - <progressBar id='health' value='100' text='health 175/175' />
                            // - <progressBar id='mindState' value='0' text='clear as a bell' />
                            // - <progressBar id='encumlevel' value='15' text='Light' />

                            // Try to find window - try the ID first, then common aliases
                            let window_id = if id == "pbarStance" && self.window_manager.get_window("stance").is_some() {
                                "stance"
                            } else if id == "mindState" && self.window_manager.get_window("mindstate").is_some() {
                                "mindstate"
                            } else {
                                &id
                            };

                            // Get window dimensions BEFORE borrowing window mutably
                            let window_width = self.window_manager.get_window_width(&id).unwrap_or(20);
                            let has_border = self.window_manager.get_window_border(&id).unwrap_or(true);

                            if let Some(window) = self.window_manager.get_window(window_id) {
                                // Special handling for encumbrance - change color based on value
                                if id == "encumlevel" {
                                    let color = if value <= 20 {
                                        "#006400" // Green: 1-20
                                    } else if value <= 40 {
                                        "#a29900" // Yellow: 21-40
                                    } else if value <= 60 {
                                        "#8b4513" // Brown: 41-60
                                    } else {
                                        "#ff0000" // Red: 61-100
                                    };
                                    window.set_bar_colors(Some(color.to_string()), Some("#000000".to_string()));
                                    window.set_progress_with_text(value, max, Some(text.clone()));
                                    debug!("Updated encumbrance bar to {}% with color {} and text '{}'", value, color, text);
                                } else if id == "stance" || id == "pbarStance" {
                                    // Special handling for stance - show stance name based on percentage
                                    let stance_text = Self::stance_percentage_to_text(value);
                                    window.set_progress_with_text(value, max, Some(stance_text.clone()));
                                    debug!("Updated stance bar to {}% with text '{}'", value, stance_text);
                                } else {
                                    // value is percentage (0-100), max is 100
                                    // text contains display text like "mana 407/407" or "clear as a bell"

                                    // Determine display text based on available width
                                    let display_text = if text.contains('/') {
                                        let available_width = if has_border {
                                            window_width.saturating_sub(2)
                                        } else {
                                            window_width
                                        };

                                        // Try full text first (e.g., "Health 350/357")
                                        if text.len() as u16 <= available_width {
                                            text.clone()
                                        } else {
                                            // Strip prefix if too long (e.g., "350/357")
                                            text.split_whitespace().skip(1).collect::<Vec<_>>().join(" ")
                                        }
                                    } else {
                                        // Custom text like "clear as a bell" - use as-is
                                        text.clone()
                                    };

                                    if !display_text.is_empty() {
                                        window.set_progress_with_text(value, max, Some(display_text.clone()));
                                        debug!("Updated progress bar '{}' to {}% with text '{}'", id, value, display_text);
                                    } else {
                                        window.set_progress(value, max);
                                        debug!("Updated progress bar '{}' to {}/{}", id, value, max);
                                    }
                                }
                            } else {
                                debug!("No window found for progress bar id '{}'", id);
                            }
                        }
                        ParsedElement::Label { id, value } => {
                            // Handle label elements like blood points
                            // <label id='lblBPs' value='Blood Points: 100' />
                            // Parse numeric value from the string and use it for progress

                            if let Some(window) = self.window_manager.get_window(&id) {
                                // Try to extract a number from the value string
                                // Match patterns like "Blood Points: 100" or just "100"
                                let number = value.split_whitespace()
                                    .filter_map(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
                                    .last(); // Get the last number found

                                if let Some(num) = number {
                                    // Assume max is 100 for percentage-based displays
                                    // Show the original text with the extracted value
                                    window.set_progress_with_text(num, 100, Some(value.clone()));
                                    tracing::debug!("Updated label '{}' to {}% with text '{}'", id, num, value);
                                } else {
                                    // No number found, just show the text at 0%
                                    window.set_progress_with_text(0, 100, Some(value.clone()));
                                    tracing::debug!("Updated label '{}' with text '{}' (no value)", id, value);
                                }
                            } else {
                                tracing::warn!("No window found for label id '{}' - available windows: {:?}",
                                    id, self.window_manager.get_window_names());
                            }
                        }
                        ParsedElement::BloodPoints { value } => {
                            // <dialogData id='BetrayerPanel'><label id='lblBPs' value='Blood Points: 100' />
                            // Update blood points progress bar with simplified text
                            let blood_names = ["bloodpoints", "lblBPs", "blood"];
                            for name in &blood_names {
                                if let Some(window) = self.window_manager.get_window(name) {
                                    window.set_progress_with_text(value, 100, Some(format!("blood {}", value)));
                                    tracing::debug!("Updated {} blood points to {}", name, value);
                                    break;
                                }
                            }
                        }
                        ParsedElement::RoundTime { value } => {
                            // <roundTime value='1760006697'/>
                            // value is Unix timestamp when roundtime ends
                            if let Some(window) = self.window_manager.get_window("roundtime") {
                                window.set_countdown(value as u64);
                                debug!("Updated roundtime to end at {}", value);
                            }
                        }
                        ParsedElement::CastTime { value } => {
                            // <castTime value='1760331899'/>
                            // value is Unix timestamp when cast time ends
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            let remaining = (value as i64) - (now as i64);
                            if let Some(window) = self.window_manager.get_window("casttime") {
                                window.set_countdown(value as u64);
                                debug!("Updated casttime: end={}, now={}, remaining={}s", value, now, remaining);
                            }
                        }
                        ParsedElement::Compass { directions } => {
                            // <compass><dir value="n"/><dir value="e"/>...</compass>
                            // Update compass widget with available exits
                            if let Some(window) = self.window_manager.get_window("compass") {
                                window.set_compass_directions(directions.clone());
                                // debug!("Updated compass with directions: {:?}", directions);  // Commented out - too spammy
                            }
                        }
                        ParsedElement::InjuryImage { id, name } => {
                            // <image id="head" name="Injury2"/>
                            // Convert injury name to level: Injury1-3 = 1-3, Scar1-3 = 4-6
                            let level = if name.starts_with("Injury") {
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
                                0 // No injury
                            };

                            if let Some(window) = self.window_manager.get_window("injuries") {
                                window.set_injury(id.clone(), level);
                                debug!("Updated injury: {} to level {} ({})", id, level, name);
                            }
                        }
                        ParsedElement::LeftHand { item } => {
                            // Update grouped hands widget if it exists
                            if let Some(window) = self.window_manager.get_window("hands") {
                                window.set_left_hand(item.clone());
                                debug!("Updated left hand (grouped): {}", item);
                            }
                            // Update individual lefthand widget if it exists
                            if let Some(window) = self.window_manager.get_window("lefthand") {
                                window.set_hand_content(item.clone());
                                debug!("Updated left hand (individual): {}", item);
                            }
                        }
                        ParsedElement::RightHand { item } => {
                            // Update grouped hands widget if it exists
                            if let Some(window) = self.window_manager.get_window("hands") {
                                window.set_right_hand(item.clone());
                                debug!("Updated right hand (grouped): {}", item);
                            }
                            // Update individual righthand widget if it exists
                            if let Some(window) = self.window_manager.get_window("righthand") {
                                window.set_hand_content(item.clone());
                                debug!("Updated right hand (individual): {}", item);
                            }
                        }
                        ParsedElement::SpellHand { spell } => {
                            // Update grouped hands widget if it exists
                            if let Some(window) = self.window_manager.get_window("hands") {
                                window.set_spell_hand(spell.clone());
                                debug!("Updated spell hand (grouped): {}", spell);
                            }
                            // Update individual spellhand widget if it exists
                            if let Some(window) = self.window_manager.get_window("spellhand") {
                                window.set_hand_content(spell.clone());
                                debug!("Updated spell hand (individual): {}", spell);
                            }
                        }
                        ParsedElement::StatusIndicator { id, active } => {
                            // Update status indicator widgets (poisoned, diseased, bleeding, stunned)
                            let value = if active { 1 } else { 0 };

                            // Update individual indicator window if it exists
                            if let Some(window) = self.window_manager.get_window(&id) {
                                window.set_indicator(value);
                                debug!("Updated status indicator {}: {}", id, if active { "active" } else { "clear" });
                            }

                            // Update any dashboard widgets that contain this indicator
                            self.window_manager.update_dashboard_indicator(&id, value);
                        }
                        ParsedElement::ActiveEffect { category, id, value, text, time } => {
                            // Update active effects widgets
                            // Parse spell ID to lookup color
                            let spell_color = id.parse::<u32>()
                                .ok()
                                .and_then(|spell_id| self.config.get_spell_color(spell_id));

                            // Find all windows that accept this category
                            let window_names = self.window_manager.get_window_names();
                            for window_name in window_names {
                                if let Some(effect_category) = self.window_manager.get_window_effect_category(&window_name) {
                                    // Window accepts this category if it matches exactly
                                    if effect_category == *category {
                                        if let Some(window) = self.window_manager.get_window(&window_name) {
                                            window.add_or_update_effect(
                                                id.clone(),
                                                text.clone(),
                                                value,
                                                time.clone(),
                                                spell_color.clone()
                                            );
                                            debug!("Updated active effect {} in window {}: {} ({}%)", id, window_name, text, value);
                                        }
                                    }
                                }
                            }
                        }
                        ParsedElement::ClearActiveEffects { category } => {
                            // Clear active effects in matching windows
                            let window_names = self.window_manager.get_window_names();
                            for window_name in window_names {
                                if let Some(config) = self.layout.windows.iter().find(|w| w.name == window_name) {
                                    if let Some(ref effect_category) = config.effect_category {
                                        // Clear if window's category matches exactly
                                        if *effect_category == *category {
                                            if let Some(window) = self.window_manager.get_window(&window_name) {
                                                window.clear_active_effects();
                                                debug!("Cleared active effects in window {} for category {}", window_name, category);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ParsedElement::MenuResponse { id, coords } => {
                            // Handle menu response
                            self.handle_menu_response(&id, &coords);
                        }
                        ParsedElement::Event { event_type, action, duration } => {
                            // Handle event patterns (stun, webbed, etc.)
                            self.handle_event(&event_type, action, duration);
                        }
                        _ => {
                            // Other element types don't add visible content
                        }
                    }
                }

                // ALWAYS finish the line after processing a server line
                // Each TCP line from server = one display line (like ProfanityFE)
                if let Ok(size) = crossterm::terminal::size() {
                    let inner_width = size.0.saturating_sub(2);
                    self.finish_current_line(inner_width);
                }
            }
        }
    }

    fn add_system_message(&mut self, msg: &str) {
        self.add_text_to_current_stream(StyledText {
            content: format!("*** {} ***", msg),
            fg: Some(Color::Yellow),
            bg: None,
            bold: true,
            span_type: SpanType::Normal,
                            link_data: None,
        });
        // Finish the line
        if let Ok(size) = crossterm::terminal::size() {
            let inner_width = size.0.saturating_sub(2);
            self.finish_current_line(inner_width);
        }
    }

    /// Get list of available dot commands for tab completion
    fn get_available_dot_commands(&self) -> Vec<String> {
        vec![
            // Application
            ".quit".to_string(), ".q".to_string(),
            // Window management
            ".createwindow".to_string(), ".createwin".to_string(),
            ".customwindow".to_string(), ".customwin".to_string(),
            ".deletewindow".to_string(), ".deletewin".to_string(),
            ".editwindow".to_string(), ".editwin".to_string(),
            ".addwindow".to_string(), ".newwindow".to_string(),
            ".editinput".to_string(), ".editcommandbox".to_string(),
            ".windows".to_string(), ".listwindows".to_string(),
            ".templates".to_string(), ".availablewindows".to_string(),
            ".rename".to_string(),
            ".border".to_string(),
            ".contentalign".to_string(), ".align".to_string(),
            ".background".to_string(), ".bgcolor".to_string(),
            // Tabbed windows
            ".createtabbed".to_string(), ".tabbedwindow".to_string(),
            ".addtab".to_string(),
            ".removetab".to_string(),
            ".switchtab".to_string(),
            ".movetab".to_string(), ".reordertab".to_string(),
            ".tabcolors".to_string(), ".settabcolors".to_string(),
            // Layout
            ".savelayout".to_string(),
            ".loadlayout".to_string(),
            ".layouts".to_string(),
            // Progress bars
            ".setprogress".to_string(),
            ".setbarcolor".to_string(),
            // Countdowns
            ".setcountdown".to_string(),
            // Indicators
            ".indicatoron".to_string(),
            ".indicatoroff".to_string(),
            // Active effects
            ".togglespellid".to_string(), ".toggleeffectid".to_string(),
            // Highlights
            ".addhighlight".to_string(), ".addhl".to_string(),
            ".edithighlight".to_string(), ".edithl".to_string(),
            ".deletehighlight".to_string(), ".delhl".to_string(),
            ".listhighlights".to_string(), ".listhl".to_string(), ".highlights".to_string(),
            ".testhighlight".to_string(), ".testhl".to_string(),
            // Keybinds
            ".addkeybind".to_string(), ".addkey".to_string(),
            ".editkeybind".to_string(), ".editkey".to_string(),
            ".deletekeybind".to_string(), ".delkey".to_string(),
            ".listkeybinds".to_string(), ".listkeys".to_string(), ".keybinds".to_string(),
            // Settings
            ".settings".to_string(), ".config".to_string(),
            // Debug
            ".randominjuries".to_string(), ".randinjuries".to_string(),
            ".randomcompass".to_string(), ".randcompass".to_string(),
            ".randomprogress".to_string(), ".randprog".to_string(),
            ".randomcountdowns".to_string(), ".randcountdowns".to_string(),
        ]
    }

    /// Get list of available window and template names for tab completion
    fn get_available_names(&self) -> Vec<String> {
        let mut names = Vec::new();

        // Add current window names
        for window_config in &self.layout.windows {
            names.push(window_config.name.clone());
        }

        // Add template names
        names.extend(Config::available_window_templates().into_iter().map(|s| s.to_string()));

        // Deduplicate
        names.sort();
        names.dedup();

        names
    }

    /// Rebuild the keybind_map from config (called after adding/deleting keybinds)
    fn rebuild_keybind_map(&mut self) {
        use crate::config::{KeyBindAction, parse_key_string};

        let mut keybind_map = HashMap::new();
        for (key_str, keybind_action) in &self.config.keybinds {
            if let Some((key_code, modifiers)) = parse_key_string(key_str) {
                let action = match keybind_action {
                    KeyBindAction::Action(action_str) => {
                        KeyAction::from_str(action_str)
                    }
                    KeyBindAction::Macro(macro_action) => {
                        Some(KeyAction::SendMacro(macro_action.macro_text.clone()))
                    }
                };

                if let Some(action) = action {
                    keybind_map.insert((key_code, modifiers), action);
                } else {
                    tracing::warn!("Invalid keybind: {} -> {:?}", key_str, keybind_action);
                }
            } else {
                tracing::warn!("Failed to parse key string: {}", key_str);
            }
        }

        self.keybind_map = keybind_map;
        debug!("Rebuilt keybind map: {} keybindings", self.keybind_map.len());
    }

    fn parse_hex_color(hex: &str) -> Option<Color> {
        // Parse #RRGGBB format
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

        Some(Color::Rgb(r, g, b))
    }

    /// Convert stance percentage to stance name
    /// 100% = defensive, 80% = guarded, 60% = neutral, 40% = forward, 20% = advance, 0% = offensive
    fn stance_percentage_to_text(percentage: u32) -> String {
        match percentage {
            81..=100 => "defensive".to_string(),
            61..=80 => "guarded".to_string(),
            41..=60 => "neutral".to_string(),
            21..=40 => "forward".to_string(),
            1..=20 => "advance".to_string(),
            0 => "offensive".to_string(),
            _ => "unknown".to_string(),
        }
    }

    /// Check if a window is locked (cannot be moved or resized)
    fn is_window_locked(&self, window_idx: usize) -> bool {
        if window_idx >= self.layout.windows.len() {
            return false;
        }
        self.layout.windows[window_idx].locked
    }

    /// Open the settings editor with all configuration values
    fn open_settings_editor(&mut self) {
        use crate::ui::{SettingsEditor, SettingItem, SettingValue};

        let mut items = Vec::new();

        // Connection settings
        items.push(SettingItem {
            category: "Connection".to_string(),
            key: "host".to_string(),
            display_name: "Host".to_string(),
            value: SettingValue::String(self.config.connection.host.clone()),
            description: Some("Lich server hostname or IP address".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "Connection".to_string(),
            key: "port".to_string(),
            display_name: "Port".to_string(),
            value: SettingValue::Number(self.config.connection.port as i64),
            description: Some("Lich server port number".to_string()),
            editable: true,
            name_width: None,
        });

        // UI settings
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "command_echo_color".to_string(),
            display_name: "Command Echo Color".to_string(),
            value: SettingValue::Color(self.config.ui.command_echo_color.clone()),
            description: Some("Color for echoed commands (hex color code)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "default_border_color".to_string(),
            display_name: "Default Border Color".to_string(),
            value: SettingValue::Color(self.config.ui.default_border_color.clone()),
            description: Some("Global default border color (palette name or #RRGGBB)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "focused_border_color".to_string(),
            display_name: "Focused Border Color".to_string(),
            value: SettingValue::Color(self.config.ui.focused_border_color.clone()),
            description: Some("Border color for focused/active windows (palette name or #RRGGBB)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "default_text_color".to_string(),
            display_name: "Default Text Color".to_string(),
            value: SettingValue::Color(self.config.ui.default_text_color.clone()),
            description: Some("Global default text color (palette name or #RRGGBB)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "countdown_icon".to_string(),
            display_name: "Countdown Icon".to_string(),
            value: SettingValue::String(self.config.ui.countdown_icon.clone()),
            description: Some("Unicode character for countdown fill (default: \u{f0c8})".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "poll_timeout_ms".to_string(),
            display_name: "Poll Timeout (ms)".to_string(),
            value: SettingValue::Number(self.config.ui.poll_timeout_ms as i64),
            description: Some("Event poll timeout - lower = higher FPS but more CPU (16ms=60fps, 8ms=120fps, 4ms=240fps)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "selection_enabled".to_string(),
            display_name: "Selection Enabled".to_string(),
            value: SettingValue::Boolean(self.config.ui.selection_enabled),
            description: Some("Enable text selection with mouse".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "selection_bg_color".to_string(),
            display_name: "Selection Highlight Color".to_string(),
            value: SettingValue::Color(self.config.ui.selection_bg_color.clone()),
            description: Some("Text selection highlight color (palette name or #RRGGBB)".to_string()),
            editable: true,
            name_width: None,
        });

        // Sound settings
        items.push(SettingItem {
            category: "Sound".to_string(),
            key: "sound_enabled".to_string(),
            display_name: "Sound Enabled".to_string(),
            value: SettingValue::Boolean(self.config.sound.enabled),
            description: Some("Enable sound effects for highlights".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "Sound".to_string(),
            key: "volume".to_string(),
            display_name: "Volume".to_string(),
            value: SettingValue::Float(self.config.sound.volume as f64),
            description: Some("Master volume (0.0 to 1.0)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "Sound".to_string(),
            key: "sound_cooldown_ms".to_string(),
            display_name: "Sound Cooldown (ms)".to_string(),
            value: SettingValue::Number(self.config.sound.cooldown_ms as i64),
            description: Some("Minimum milliseconds between sound plays".to_string()),
            editable: true,
            name_width: None,
        });

        // Preset colors (sorted alphabetically) - show as "fg bg" format
        let mut preset_names: Vec<String> = self.config.presets.keys().cloned().collect();
        preset_names.sort();
        for preset_name in preset_names {
            if let Some(preset) = self.config.presets.get(&preset_name) {
                let fg = preset.fg.as_ref().map(|s| s.as_str()).unwrap_or("-");
                let bg = preset.bg.as_ref().map(|s| s.as_str()).unwrap_or("-");
                let display = format!("{} {}", fg, bg);
                items.push(SettingItem {
                    category: "Presets".to_string(),
                    key: format!("preset_{}", preset_name),
                    display_name: preset_name.clone(),
                    value: SettingValue::String(display),
                    description: Some("Format: #RRGGBB #RRGGBB (fg bg), use - for no color".to_string()),
                    editable: true,
            name_width: None,
                });
            }
        }

        // Spell colors
        for (idx, spell_range) in self.config.spell_colors.iter().enumerate() {
            let spell_ids = spell_range.spells.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            items.push(SettingItem {
                category: "Spells".to_string(),
                key: format!("spell_color_{}", idx),
                display_name: format!("Spells: {}", if spell_ids.len() > 40 { format!("{}...", &spell_ids[..40]) } else { spell_ids.clone() }),
                value: SettingValue::Color(spell_range.color.clone()),
                description: Some(format!("Color for spells: {}", spell_ids)),
                editable: true,
            name_width: None,
            });
        }

        // Prompt colors
        for prompt_color in &self.config.ui.prompt_colors {
            // Migrate legacy color field to fg if needed
            let fg = prompt_color.fg.as_ref().or(prompt_color.color.as_ref()).map(|s| s.as_str()).unwrap_or("-");
            let bg = prompt_color.bg.as_ref().map(|s| s.as_str()).unwrap_or("-");
            let display = format!("{} {}", fg, bg);
            items.push(SettingItem {
                category: "Prompts".to_string(),
                key: format!("prompt_{}", prompt_color.character),
                display_name: format!("Prompt '{}'", prompt_color.character),
                value: SettingValue::String(display),
                description: Some("Format: #RRGGBB #RRGGBB (fg bg), use - for no color".to_string()),
                editable: true,
            name_width: None,
            });
        }

        let editor = SettingsEditor::with_items(items);
        self.settings_editor = Some(editor);
        self.input_mode = InputMode::SettingsEditor;
        self.add_system_message("Opening settings editor (Up/Down to navigate, Enter to edit, Esc to close)");
    }

    /// Refresh the settings editor with current config values (preserves scroll position and selection)
    fn refresh_settings_editor(&mut self) {
        use crate::ui::{SettingsEditor, SettingItem, SettingValue};

        // Get current position from existing editor
        let (selected_index, scroll_offset, popup_x, popup_y) = if let Some(ref editor) = self.settings_editor {
            (editor.get_selected_index(), editor.get_scroll_offset(), editor.popup_x, editor.popup_y)
        } else {
            return; // No editor open, nothing to refresh
        };

        let mut items = Vec::new();

        // Connection settings
        items.push(SettingItem {
            category: "Connection".to_string(),
            key: "host".to_string(),
            display_name: "Host".to_string(),
            value: SettingValue::String(self.config.connection.host.clone()),
            description: Some("Lich server hostname or IP address".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "Connection".to_string(),
            key: "port".to_string(),
            display_name: "Port".to_string(),
            value: SettingValue::Number(self.config.connection.port as i64),
            description: Some("Lich server port number".to_string()),
            editable: true,
            name_width: None,
        });

        // UI settings
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "command_echo_color".to_string(),
            display_name: "Command Echo Color".to_string(),
            value: SettingValue::Color(self.config.ui.command_echo_color.clone()),
            description: Some("Color for echoed commands (hex color code)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "default_border_color".to_string(),
            display_name: "Default Border Color".to_string(),
            value: SettingValue::Color(self.config.ui.default_border_color.clone()),
            description: Some("Global default border color (palette name or #RRGGBB)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "focused_border_color".to_string(),
            display_name: "Focused Border Color".to_string(),
            value: SettingValue::Color(self.config.ui.focused_border_color.clone()),
            description: Some("Border color for focused/active windows (palette name or #RRGGBB)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "default_text_color".to_string(),
            display_name: "Default Text Color".to_string(),
            value: SettingValue::Color(self.config.ui.default_text_color.clone()),
            description: Some("Global default text color (palette name or #RRGGBB)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "countdown_icon".to_string(),
            display_name: "Countdown Icon".to_string(),
            value: SettingValue::String(self.config.ui.countdown_icon.clone()),
            description: Some("Unicode character for countdown fill (default: \u{f0c8})".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "poll_timeout_ms".to_string(),
            display_name: "Poll Timeout (ms)".to_string(),
            value: SettingValue::Number(self.config.ui.poll_timeout_ms as i64),
            description: Some("Event poll timeout - lower = higher FPS but more CPU (16ms=60fps, 8ms=120fps, 4ms=240fps)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "selection_enabled".to_string(),
            display_name: "Selection Enabled".to_string(),
            value: SettingValue::Boolean(self.config.ui.selection_enabled),
            description: Some("Enable text selection with mouse".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "UI".to_string(),
            key: "selection_bg_color".to_string(),
            display_name: "Selection Highlight Color".to_string(),
            value: SettingValue::Color(self.config.ui.selection_bg_color.clone()),
            description: Some("Text selection highlight color (palette name or #RRGGBB)".to_string()),
            editable: true,
            name_width: None,
        });

        // Sound settings
        items.push(SettingItem {
            category: "Sound".to_string(),
            key: "sound_enabled".to_string(),
            display_name: "Sound Enabled".to_string(),
            value: SettingValue::Boolean(self.config.sound.enabled),
            description: Some("Enable sound effects for highlights".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "Sound".to_string(),
            key: "volume".to_string(),
            display_name: "Volume".to_string(),
            value: SettingValue::Float(self.config.sound.volume as f64),
            description: Some("Master volume (0.0 to 1.0)".to_string()),
            editable: true,
            name_width: None,
        });
        items.push(SettingItem {
            category: "Sound".to_string(),
            key: "sound_cooldown_ms".to_string(),
            display_name: "Sound Cooldown (ms)".to_string(),
            value: SettingValue::Number(self.config.sound.cooldown_ms as i64),
            description: Some("Minimum milliseconds between sound plays".to_string()),
            editable: true,
            name_width: None,
        });

        // Preset colors (sorted alphabetically) - show as "fg bg" format
        let mut preset_names: Vec<String> = self.config.presets.keys().cloned().collect();
        preset_names.sort();
        for preset_name in preset_names {
            if let Some(preset) = self.config.presets.get(&preset_name) {
                let fg = preset.fg.as_ref().map(|s| s.as_str()).unwrap_or("-");
                let bg = preset.bg.as_ref().map(|s| s.as_str()).unwrap_or("-");
                let display = format!("{} {}", fg, bg);
                items.push(SettingItem {
                    category: "Presets".to_string(),
                    key: format!("preset_{}", preset_name),
                    display_name: preset_name.clone(),
                    value: SettingValue::String(display),
                    description: Some("Format: #RRGGBB #RRGGBB (fg bg), use - for no color".to_string()),
                    editable: true,
            name_width: None,
                });
            }
        }

        // Spell colors
        for (idx, spell_range) in self.config.spell_colors.iter().enumerate() {
            let spell_ids = spell_range.spells.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            items.push(SettingItem {
                category: "Spells".to_string(),
                key: format!("spell_color_{}", idx),
                display_name: format!("Spells: {}", if spell_ids.len() > 40 { format!("{}...", &spell_ids[..40]) } else { spell_ids.clone() }),
                value: SettingValue::Color(spell_range.color.clone()),
                description: Some(format!("Color for spells: {}", spell_ids)),
                editable: true,
            name_width: None,
            });
        }

        // Prompt colors
        for prompt_color in &self.config.ui.prompt_colors {
            // Migrate legacy color field to fg if needed
            let fg = prompt_color.fg.as_ref().or(prompt_color.color.as_ref()).map(|s| s.as_str()).unwrap_or("-");
            let bg = prompt_color.bg.as_ref().map(|s| s.as_str()).unwrap_or("-");
            let display = format!("{} {}", fg, bg);
            items.push(SettingItem {
                category: "Prompts".to_string(),
                key: format!("prompt_{}", prompt_color.character),
                display_name: format!("Prompt '{}'", prompt_color.character),
                value: SettingValue::String(display),
                description: Some("Format: #RRGGBB #RRGGBB (fg bg), use - for no color".to_string()),
                editable: true,
            name_width: None,
            });
        }

        // Create new editor with updated values but preserve position
        let mut editor = SettingsEditor::with_items(items);
        editor.set_selected_index(selected_index);
        editor.set_scroll_offset(scroll_offset);
        editor.popup_x = popup_x;
        editor.popup_y = popup_y;
        self.settings_editor = Some(editor);
    }

    /// Open the highlight browser
    fn open_highlight_browser(&mut self) {
        use crate::ui::HighlightBrowser;

        let browser = HighlightBrowser::new(&self.config.highlights);
        self.highlight_browser = Some(browser);
        self.input_mode = InputMode::HighlightBrowser;
        self.add_system_message("Opening highlight browser (Up/Down to navigate, Enter to edit, Delete to remove, Esc to close)");
    }

    /// Open the keybind browser
    fn open_keybind_browser(&mut self) {
        use crate::ui::KeybindBrowser;

        let browser = KeybindBrowser::new(&self.config.keybinds);
        self.keybind_browser = Some(browser);
        self.input_mode = InputMode::KeybindBrowser;
        self.add_system_message("Opening keybind browser (Up/Down to navigate, Enter to edit, Delete to remove, Esc to close)");
    }

    /// Open the color palette browser
    fn open_color_palette_browser(&mut self) {
        use crate::ui::ColorPaletteBrowser;

        let browser = ColorPaletteBrowser::new(self.config.color_palette.clone());
        self.color_palette_browser = Some(browser);
        self.input_mode = InputMode::ColorPaletteBrowser;
        self.add_system_message("Opening color palette (Up/Down to navigate, Enter to edit, F to favorite, / to filter, Esc to close)");
    }

    fn open_color_form_create(&mut self) {
        use crate::ui::ColorForm;

        let form = ColorForm::new_create();
        self.color_form = Some(form);
        self.input_mode = InputMode::ColorForm;
        self.add_system_message("Add new color (Tab to navigate fields, Ctrl+S to save, Esc to cancel)");
    }

    fn open_color_form_edit(&mut self, color: crate::config::PaletteColor) {
        use crate::ui::ColorForm;

        let form = ColorForm::new_edit(&color);
        self.color_form = Some(form);
        self.input_mode = InputMode::ColorForm;
        self.add_system_message("Edit color (Tab to navigate fields, Ctrl+S to save, Esc to cancel)");
    }

    /// Validate a setting value
    fn validate_setting(&self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "port" => {
                if let Ok(port) = value.parse::<u16>() {
                    if port == 0 {
                        return Err("Port must be between 1 and 65535".to_string());
                    }
                    Ok(())
                } else {
                    Err("Port must be a valid number".to_string())
                }
            }
            "poll_timeout_ms" => {
                if let Ok(timeout) = value.parse::<u64>() {
                    if timeout == 0 {
                        return Err("Poll timeout must be at least 1ms".to_string());
                    }
                    if timeout > 1000 {
                        return Err("Poll timeout should be less than 1000ms for responsiveness".to_string());
                    }
                    Ok(())
                } else {
                    Err("Poll timeout must be a valid number".to_string())
                }
            }
            "volume" => {
                if let Ok(vol) = value.parse::<f32>() {
                    if vol < 0.0 || vol > 1.0 {
                        return Err("Volume must be between 0.0 and 1.0".to_string());
                    }
                    Ok(())
                } else {
                    Err("Volume must be a valid number".to_string())
                }
            }
            // Preset and prompt colors (special "fg bg" format validation). Accept #hex, palette names, or '-'.
            key if key.starts_with("preset_") || key.starts_with("prompt_") => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                if parts.len() != 2 {
                    return Err("Format must be: #RRGGBB #RRGGBB (fg bg) or - for no color".to_string());
                }
                for (i, part) in parts.iter().enumerate() {
                    if *part == "-" { continue; }
                    let is_hex = part.starts_with('#')
                        && (part.len() == 7 || part.len() == 9)
                        && part[1..].chars().all(|c| c.is_ascii_hexdigit());
                    if is_hex { continue; }
                    // Allow palette names that resolve to #RRGGBB
                    if let Some(resolved) = self.config.resolve_color(part) {
                        let r = resolved.as_str();
                        if !(r.starts_with('#') && r.len() == 7 && r[1..].chars().all(|c| c.is_ascii_hexdigit())) {
                            return Err(format!("{} color must be #RRGGBB, #RRGGBBAA, palette name, or '-'", if i == 0 { "Foreground" } else { "Background" }));
                        }
                    } else {
                        return Err(format!("{} color must be #RRGGBB, #RRGGBBAA, palette name, or '-'", if i == 0 { "Foreground" } else { "Background" }));
                    }
                }
                Ok(())
            }
            // Color validation (single colors) – accept #hex or palette name
            key if key.contains("color") || key.starts_with("spell_color_") || key.starts_with("prompt_") => {
                let is_hex = value.starts_with('#')
                    && (value.len() == 7 || value.len() == 9)
                    && value[1..].chars().all(|c| c.is_ascii_hexdigit());
                if is_hex { return Ok(()); }
                if let Some(resolved) = self.config.resolve_color(value) {
                    let r = resolved.as_str();
                    if r.starts_with('#') && r.len() == 7 && r[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                        return Ok(());
                    }
                }
                Err("Color must be #RRGGBB or palette name".to_string())
            }
            _ => Ok(()),
        }
    }

    /// Update a setting by key and save to config file
    fn update_setting(&mut self, key: &str, value: &str) -> bool {
        // Validate first
        if let Err(msg) = self.validate_setting(key, value) {
            self.add_system_message(&format!("Validation error: {}", msg));
            return false;
        }

        let result = match key {
            // Connection settings
            "host" => {
                self.config.connection.host = value.to_string();
                true
            }
            "port" => {
                if let Ok(port) = value.parse::<u16>() {
                    self.config.connection.port = port;
                    true
                } else {
                    false
                }
            }
            // UI settings
            "command_echo_color" => {
                let resolved = self.config.resolve_color(value).unwrap_or_else(|| value.to_string());
                self.config.ui.command_echo_color = resolved;
                true
            }
            "default_border_color" => {
                let resolved = self.config.resolve_color(value).unwrap_or_else(|| value.to_string());
                self.config.ui.default_border_color = resolved;
                true
            }
            "focused_border_color" => {
                let resolved = self.config.resolve_color(value).unwrap_or_else(|| value.to_string());
                self.config.ui.focused_border_color = resolved;
                true
            }
            "default_text_color" => {
                let resolved = self.config.resolve_color(value).unwrap_or_else(|| value.to_string());
                self.config.ui.default_text_color = resolved;
                true
            }
            "selection_bg_color" => {
                let resolved = self.config.resolve_color(value).unwrap_or_else(|| value.to_string());
                self.config.ui.selection_bg_color = resolved;
                true
            }
            "countdown_icon" => {
                self.config.ui.countdown_icon = value.to_string();
                true
            }
            "poll_timeout_ms" => {
                if let Ok(timeout) = value.parse::<u64>() {
                    self.config.ui.poll_timeout_ms = timeout;
                    true
                } else {
                    false
                }
            }
            "selection_enabled" => {
                if let Ok(enabled) = value.parse::<bool>() {
                    self.config.ui.selection_enabled = enabled;
                    true
                } else {
                    false
                }
            }
            // Sound settings
            "sound_enabled" => {
                if let Ok(enabled) = value.parse::<bool>() {
                    self.config.sound.enabled = enabled;
                    true
                } else {
                    false
                }
            }
            "volume" => {
                if let Ok(vol) = value.parse::<f32>() {
                    self.config.sound.volume = vol;
                    true
                } else {
                    false
                }
            }
            "sound_cooldown_ms" => {
                if let Ok(cooldown) = value.parse::<u64>() {
                    self.config.sound.cooldown_ms = cooldown;
                    true
                } else {
                    false
                }
            }
            // Spell colors (key format: "spell_color_<idx>")
            key if key.starts_with("spell_color_") => {
                if let Some(idx_str) = key.strip_prefix("spell_color_") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        let resolved = self.config.resolve_color(value).unwrap_or_else(|| value.to_string());
                        if let Some(spell_range) = self.config.spell_colors.get_mut(idx) {
                            spell_range.color = resolved;
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            // Prompt colors (key format: "prompt_<character>", value format: "fg bg")
            key if key.starts_with("prompt_") => {
                if let Some(character) = key.strip_prefix("prompt_") {
                    // Parse "fg bg" format
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() == 2 {
                        let fg_resolved = if parts[0] == "-" { None } else { self.config.resolve_color(parts[0]) };
                        let bg_resolved = if parts[1] == "-" { None } else { self.config.resolve_color(parts[1]) };
                        if let Some(prompt) = self.config.ui.prompt_colors.iter_mut().find(|p| p.character == character) {
                            prompt.fg = fg_resolved;
                            prompt.bg = bg_resolved;
                            prompt.color = None; // Clear legacy field
                            true
                        } else {
                            false
                        }
                    } else {
                        self.add_system_message("Format must be: #RRGGBB #RRGGBB (fg bg), use - for no color");
                        false
                    }
                } else {
                    false
                }
            }
            // Preset colors (key format: "preset_<name>", value format: "fg bg" or "- bg" or "fg -")
            key if key.starts_with("preset_") => {
                if let Some(preset_name) = key.strip_prefix("preset_") {
                    // Parse "fg bg" format
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() == 2 {
                        let fg_resolved = if parts[0] == "-" { None } else { self.config.resolve_color(parts[0]) };
                        let bg_resolved = if parts[1] == "-" { None } else { self.config.resolve_color(parts[1]) };
                        if let Some(preset) = self.config.presets.get_mut(preset_name) {
                            preset.fg = fg_resolved;
                            preset.bg = bg_resolved;
                            true
                        } else {
                            false
                        }
                    } else {
                        self.add_system_message("Format must be: #RRGGBB #RRGGBB (fg bg), use - for no color");
                        false
                    }
                } else {
                    false
                }
            }
            _ => false,
        };

        if result {
            // Save config to file
            if let Err(e) = self.config.save(None) {
                tracing::error!("Failed to save config: {}", e);
                return false;
            }
        }

        result
    }
}





