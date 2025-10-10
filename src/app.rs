use crate::config::Config;
use crate::network::{LichConnection, ServerMessage};
use crate::parser::{ParsedElement, XmlParser};
use crate::ui::{CommandInput, StyledText, UiLayout, Widget, WindowManager, WindowConfig};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    style::Color,
    Terminal,
};
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};
use rand::Rng;

pub struct App {
    config: Config,
    window_manager: WindowManager,
    command_input: CommandInput,
    parser: XmlParser,
    running: bool,
    prompt_shown: bool, // Track if we've shown a prompt since last real text
    current_stream: String, // Track which stream we're currently writing to
    focused_window_index: usize, // Index of currently focused window for scrolling
    mouse_mode_enabled: bool, // Whether mouse features are enabled (vs text selection)
    resize_state: Option<ResizeState>, // Track active resize operation
    move_state: Option<MoveState>, // Track active window move operation
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
    start_window_pos: (u16, u16), // (col, row) original window position
}

#[derive(Debug, Clone, Copy)]
enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
}

impl App {
    pub fn new(mut config: Config) -> Result<Self> {
        // Try to load autosave layout
        match config.load_autosave_layout() {
            Ok(true) => info!("Loaded autosaved layout"),
            Ok(false) => debug!("No autosaved layout found, using default"),
            Err(e) => tracing::warn!("Failed to load autosaved layout: {}", e),
        }

        // Convert config presets to parser format
        let presets: Vec<(String, Option<String>, Option<String>)> = config
            .presets
            .iter()
            .map(|p| (p.id.clone(), p.fg.clone(), p.bg.clone()))
            .collect();

        debug!("Loaded {} prompt color mappings:", config.ui.prompt_colors.len());
        for pc in &config.ui.prompt_colors {
            debug!("  '{}' -> {}", pc.character, pc.color);
        }

        // Convert window configs
        let countdown_icon = Some(config.ui.countdown_icon.clone());
        let window_configs: Vec<WindowConfig> = config
            .ui
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
                title: w.title.clone(),
                bar_color: w.bar_color.clone(),
                bar_background_color: w.bar_background_color.clone(),
                transparent_background: w.transparent_background,
                countdown_icon: countdown_icon.clone(),
                indicator_colors: w.indicator_colors.clone(),
                dashboard_layout: w.dashboard_layout.clone(),
                dashboard_indicators: w.dashboard_indicators.clone(),
                dashboard_spacing: w.dashboard_spacing,
                dashboard_hide_inactive: w.dashboard_hide_inactive,
            })
            .collect();

        debug!("Creating {} windows:", window_configs.len());
        for wc in &window_configs {
            debug!("  '{}' ({}) - streams: {:?}, pos: ({},{}) size: ({}x{}), buffer: {}",
                wc.name, wc.widget_type, wc.streams, wc.row, wc.col, wc.rows, wc.cols, wc.buffer_size);
        }

        Ok(Self {
            window_manager: WindowManager::new(window_configs),
            command_input: CommandInput::new(100),
            parser: XmlParser::with_presets(presets),
            config,
            running: true,
            prompt_shown: false,
            current_stream: "main".to_string(),
            focused_window_index: 0, // Start with first window focused
            mouse_mode_enabled: false, // Start with mouse mode off (text selection enabled)
            resize_state: None, // No active resize initially
            move_state: None, // No active move initially
        })
    }

    /// Get the window for the current stream, falling back to main window
    fn get_current_window(&mut self) -> &mut Widget {
        // First, determine which window name to use
        let window_name = {
            let stream = &self.current_stream;
            self.window_manager
                .stream_map
                .get(stream)
                .cloned()
                .unwrap_or_else(|| "main".to_string())
        };

        // Then get the window
        self.window_manager
            .get_window(&window_name)
            .expect("Window must exist")
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
        &self,
        mouse_col: u16,
        mouse_row: u16,
        window_layouts: &HashMap<String, ratatui::layout::Rect>,
    ) -> Option<usize> {
        let window_names = self.window_manager.get_window_names();

        for (idx, name) in window_names.iter().enumerate() {
            if let Some(rect) = window_layouts.get(name) {
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

    /// Update window manager configs from current config
    fn update_window_manager_config(&mut self) {
        let countdown_icon = Some(self.config.ui.countdown_icon.clone());
        let window_configs: Vec<WindowConfig> = self.config
            .ui
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
                title: w.title.clone(),
                bar_color: w.bar_color.clone(),
                bar_background_color: w.bar_background_color.clone(),
                transparent_background: w.transparent_background,
                countdown_icon: countdown_icon.clone(),
                indicator_colors: w.indicator_colors.clone(),
                dashboard_layout: w.dashboard_layout.clone(),
                dashboard_indicators: w.dashboard_indicators.clone(),
                dashboard_spacing: w.dashboard_spacing,
                dashboard_hide_inactive: w.dashboard_hide_inactive,
            })
            .collect();

        self.window_manager.update_config(window_configs);
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
        for window_def in &mut self.config.ui.windows {
            if window_def.name == window_name {
                match edge {
                    ResizeEdge::Top => {
                        // Moving top edge: adjust position and height
                        let new_row = (window_def.row as i16 + delta_rows).max(0) as u16;
                        let row_change = new_row as i16 - window_def.row as i16;
                        let new_rows = (window_def.rows as i16 - row_change).max(1) as u16;

                        // Ensure window doesn't exceed terminal bounds
                        let max_rows = term_height.saturating_sub(new_row);
                        let bounded_rows = new_rows.min(max_rows);

                        debug!("Resizing {} top: row {} -> {}, rows {} -> {} (max: {})",
                            window_name, window_def.row, new_row, window_def.rows, bounded_rows, max_rows);
                        window_def.row = new_row;
                        window_def.rows = bounded_rows;
                    }
                    ResizeEdge::Bottom => {
                        let new_rows = (window_def.rows as i16 + delta_rows).max(1) as u16;

                        // Ensure window doesn't exceed terminal bounds
                        let max_rows = term_height.saturating_sub(window_def.row);
                        let bounded_rows = new_rows.min(max_rows);

                        debug!("Resizing {} bottom: {} -> {} rows (max: {})",
                            window_name, window_def.rows, bounded_rows, max_rows);
                        window_def.rows = bounded_rows;
                    }
                    ResizeEdge::Left => {
                        // Moving left edge: adjust position and width
                        let new_col = (window_def.col as i16 + delta_cols).max(0) as u16;
                        let col_change = new_col as i16 - window_def.col as i16;
                        let new_cols = (window_def.cols as i16 - col_change).max(1) as u16;

                        // Ensure window doesn't exceed terminal bounds
                        let max_cols = term_width.saturating_sub(new_col);
                        let bounded_cols = new_cols.min(max_cols);

                        debug!("Resizing {} left: col {} -> {}, cols {} -> {} (max: {})",
                            window_name, window_def.col, new_col, window_def.cols, bounded_cols, max_cols);
                        window_def.col = new_col;
                        window_def.cols = bounded_cols;
                    }
                    ResizeEdge::Right => {
                        let new_cols = (window_def.cols as i16 + delta_cols).max(1) as u16;

                        // Ensure window doesn't exceed terminal bounds
                        let max_cols = term_width.saturating_sub(window_def.col);
                        let bounded_cols = new_cols.min(max_cols);

                        debug!("Resizing {} right: {} -> {} cols (max: {})",
                            window_name, window_def.cols, bounded_cols, max_cols);
                        window_def.cols = bounded_cols;
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
        for window_def in &mut self.config.ui.windows {
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
    fn handle_dot_command(&mut self, command: &str) {
        let parts: Vec<&str> = command[1..].split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "quit" | "q" => {
                self.running = false;
            }
            "savelayout" => {
                let name = parts.get(1).unwrap_or(&"default");
                match self.config.save_layout(name) {
                    Ok(_) => self.add_system_message(&format!("Layout saved as '{}'", name)),
                    Err(e) => self.add_system_message(&format!("Failed to save layout: {}", e)),
                }
            }
            "loadlayout" => {
                let name = parts.get(1).unwrap_or(&"default");
                match self.config.load_layout(name) {
                    Ok(_) => {
                        self.add_system_message(&format!("Layout '{}' loaded", name));
                        self.update_window_manager_config();
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
                if self.config.ui.windows.iter().any(|w| w.name == window_name) {
                    self.add_system_message(&format!("Window '{}' already exists", window_name));
                    return;
                }

                // Get template
                if let Some(window_def) = Config::get_window_template(window_name) {
                    let actual_name = window_def.name.clone();
                    self.config.ui.windows.push(window_def);
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
                if self.config.ui.windows.iter().any(|w| w.name == window_name) {
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
                    title: Some(window_name.to_string()),
                    bar_color: None,
                    bar_background_color: None,
                    transparent_background: true,
                    indicator_colors: None,
                    dashboard_layout: None,
                    dashboard_indicators: None,
                    dashboard_spacing: None,
                    dashboard_hide_inactive: None,
                };

                self.config.ui.windows.push(window_def);
                self.update_window_manager_config();
                self.add_system_message(&format!("Created custom window '{}' - use mouse to move/resize", window_name));
            }
            "deletewindow" | "deletewin" => {
                if parts.len() < 2 {
                    self.add_system_message("Usage: .deletewindow <name>");
                    return;
                }

                let window_name = parts[1];
                let initial_len = self.config.ui.windows.len();
                self.config.ui.windows.retain(|w| w.name != window_name);

                if self.config.ui.windows.len() < initial_len {
                    self.update_window_manager_config();
                    self.add_system_message(&format!("Deleted window '{}'", window_name));

                    // Adjust focused window index if needed
                    if self.focused_window_index >= self.config.ui.windows.len() && self.focused_window_index > 0 {
                        self.focused_window_index = self.config.ui.windows.len() - 1;
                    }
                } else {
                    self.add_system_message(&format!("Window '{}' not found", window_name));
                }
            }
            "windows" | "listwindows" => {
                let windows: Vec<String> = self.config.ui.windows.iter().map(|w| w.name.clone()).collect();
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

                // Stun: 15-25 seconds
                let stun_seconds = rng.gen_range(15..=25);
                if let Some(window) = self.window_manager.get_window("stun") {
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
                for window_def in &mut self.config.ui.windows {
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
                    self.add_system_message("Usage: .border <window> <style> [color]");
                    self.add_system_message("Styles: single, double, rounded, thick, none");
                    self.add_system_message("Example: .border main rounded #00ff00");
                    return;
                }

                let window_name = parts[1];
                let style = parts[2];
                let color = parts.get(3).map(|s| s.to_string());

                // Validate style
                let valid_styles = vec!["single", "double", "rounded", "thick", "none"];
                if !valid_styles.contains(&style) {
                    self.add_system_message(&format!("Invalid style: {}", style));
                    self.add_system_message("Valid styles: single, double, rounded, thick, none");
                    return;
                }

                // Find and update the window
                let mut found = false;
                for window_def in &mut self.config.ui.windows {
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

                        found = true;
                        break;
                    }
                }

                if found {
                    self.update_window_manager_config();
                    if style == "none" {
                        self.add_system_message(&format!("Removed border from '{}'", window_name));
                    } else if let Some(ref c) = color {
                        self.add_system_message(&format!("Set '{}' border to {} ({})", window_name, style, c));
                    } else {
                        self.add_system_message(&format!("Set '{}' border to {}", window_name, style));
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
                for window_def in &mut self.config.ui.windows {
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
            _ => {
                self.add_system_message(&format!("Unknown command: .{}", parts[0]));
            }
        }
    }

    /// Toggle mouse mode on/off
    fn toggle_mouse_mode(&mut self) -> Result<()> {
        self.mouse_mode_enabled = !self.mouse_mode_enabled;

        if self.mouse_mode_enabled {
            execute!(io::stdout(), EnableMouseCapture)?;
            info!("Mouse mode enabled (click/scroll windows)");
            self.add_system_message("Mouse mode: ON (Scroll Lock to toggle)");
        } else {
            execute!(io::stdout(), DisableMouseCapture)?;
            info!("Mouse mode disabled (text selection enabled)");
            self.add_system_message("Mouse mode: OFF - Text selection enabled (Scroll Lock to toggle)");
        }

        Ok(())
    }

    /// Check if a key matches the configured toggle key
    fn is_toggle_key(&self, key: KeyCode) -> bool {
        let config_key = &self.config.ui.mouse_mode_toggle_key;
        debug!("Checking toggle key: config='{}', pressed={:?}", config_key, key);

        match config_key.as_str() {
            "ScrollLock" => matches!(key, KeyCode::ScrollLock),
            "F12" => matches!(key, KeyCode::F(12)),
            "F11" => matches!(key, KeyCode::F(11)),
            "F10" => matches!(key, KeyCode::F(10)),
            "F9" => matches!(key, KeyCode::F(9)),
            "F8" => matches!(key, KeyCode::F(8)),
            "F7" => matches!(key, KeyCode::F(7)),
            "F6" => matches!(key, KeyCode::F(6)),
            "F5" => matches!(key, KeyCode::F(5)),
            "F4" => matches!(key, KeyCode::F(4)),
            "F3" => matches!(key, KeyCode::F(3)),
            "F2" => matches!(key, KeyCode::F(2)),
            "F1" => matches!(key, KeyCode::F(1)),
            _ => false,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

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
            let layout = UiLayout::calculate(terminal_rect);

            // Calculate window layouts using proportional sizing
            let window_layouts = self.window_manager.calculate_layout(layout.main_area);
            self.window_manager.update_widths(&window_layouts);

            // Draw UI
            terminal.draw(|f| {
                let layout = UiLayout::calculate(f.area());
                let window_layouts = self.window_manager.calculate_layout(layout.main_area);

                // Render all windows in order with focus indicator
                let window_names = self.window_manager.get_window_names();
                for (idx, name) in window_names.iter().enumerate() {
                    if let Some(rect) = window_layouts.get(name) {
                        if let Some(window) = self.window_manager.get_window(name) {
                            let focused = idx == self.focused_window_index;
                            window.render_with_focus(*rect, f.buffer_mut(), focused);
                        }
                    }
                }

                self.command_input.render(layout.input_area, f.buffer_mut());
            })?;

            // Handle events with timeout
            if event::poll(std::time::Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => {
                        // Only handle key press events, not release or repeat
                        if key.kind == KeyEventKind::Press {
                            self.handle_key_event(key.code, key.modifiers, &command_tx)?;
                        }
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse_event(mouse, &window_layouts)?;
                    }
                    _ => {}
                }
            }

            // Handle server messages
            while let Ok(msg) = server_rx.try_recv() {
                self.handle_server_message(msg);
            }
        }

        // Autosave layout before exiting
        if let Err(e) = self.config.autosave_layout() {
            tracing::error!("Failed to autosave layout: {}", e);
        } else {
            tracing::info!("Layout autosaved");
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
        match (key, modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.running = false;
            }
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
            (KeyCode::Up, _) => {
                self.command_input.history_previous();
            }
            (KeyCode::Down, _) => {
                self.command_input.history_next();
            }
            (KeyCode::PageUp, _) => {
                if let Some(window) = self.get_focused_window() {
                    window.scroll_up(10);
                }
            }
            (KeyCode::PageDown, _) => {
                if let Some(window) = self.get_focused_window() {
                    window.scroll_down(10);
                }
            }
            (KeyCode::Tab, KeyModifiers::NONE) => {
                self.cycle_focused_window();
            }
            (KeyCode::Enter, _) => {
                if let Some(command) = self.command_input.submit() {
                    debug!("Sending command: {}", command);

                    // Check if it's a local dot command
                    if command.starts_with('.') {
                        self.handle_dot_command(&command);
                    } else {
                        // Echo ">" with prompt color, then command with command echo color
                        let prompt_color = self.config.ui.prompt_colors
                            .iter()
                            .find(|pc| pc.character == ">")
                            .and_then(|pc| Self::parse_hex_color(&pc.color))
                            .unwrap_or(Color::DarkGray);

                        let echo_color = Self::parse_hex_color(&self.config.ui.command_echo_color);

                        // Add ">" with prompt color
                        self.get_current_window().add_text(StyledText {
                            content: ">".to_string(),
                            fg: Some(prompt_color),
                            bg: None,
                            bold: false,
                        });

                        // Add command with echo color
                        self.get_current_window().add_text(StyledText {
                            content: command.clone(),
                            fg: echo_color,
                            bg: None,
                            bold: false,
                        });

                        // Finish the line so command appears before server response
                        if let Ok(size) = crossterm::terminal::size() {
                            let inner_width = size.0.saturating_sub(2);
                            self.get_current_window().finish_line(inner_width);
                        }

                        // Reset prompt_shown so next prompt will display
                        self.prompt_shown = false;

                        let _ = command_tx.send(command);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_mouse_event(
        &mut self,
        mouse: event::MouseEvent,
        window_layouts: &HashMap<String, ratatui::layout::Rect>,
    ) -> Result<()> {
        use event::{MouseButton, MouseEventKind};

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if clicking on a resize border
                if let Some((window_idx, edge)) = self.check_resize_border(mouse.column, mouse.row, window_layouts) {
                    self.resize_state = Some(ResizeState {
                        window_index: window_idx,
                        edge,
                        start_mouse_pos: (mouse.column, mouse.row),
                    });
                    debug!("Started resize on window {} edge {:?}", window_idx, edge);
                } else if let Some(window_idx) = self.check_title_bar(mouse.column, mouse.row, window_layouts) {
                    // Clicking on title bar - start move operation
                    let window_names = self.window_manager.get_window_names();
                    if window_idx < window_names.len() {
                        let window_name = &window_names[window_idx];
                        if let Some(rect) = window_layouts.get(window_name) {
                            self.move_state = Some(MoveState {
                                window_index: window_idx,
                                start_mouse_pos: (mouse.column, mouse.row),
                                start_window_pos: (rect.x, rect.y),
                            });
                            debug!("Started move on window {} at {:?}", window_idx, (rect.x, rect.y));
                        }
                    }
                } else {
                    // Not on border or title bar, check which window was clicked
                    for (idx, name) in self.window_manager.get_window_names().iter().enumerate() {
                        if let Some(rect) = window_layouts.get(name) {
                            if mouse.column >= rect.x
                                && mouse.column < rect.x + rect.width
                                && mouse.row >= rect.y
                                && mouse.row < rect.y + rect.height
                            {
                                self.focused_window_index = idx;
                                debug!("Clicked window '{}' (index {})", name, idx);
                                break;
                            }
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
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
                // Handle active resize
                if let Some(ref state) = self.resize_state.clone() {
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
                // Parse XML and add to window
                let elements = self.parser.parse_line(&line);

                // Check if this line has Text elements with actual content (not just empty strings)
                let has_text = elements.iter().any(|e| {
                    if let ParsedElement::Text { content, .. } = e {
                        !content.trim().is_empty()
                    } else {
                        false
                    }
                });
                let _has_prompt = elements.iter().any(|e| matches!(e, ParsedElement::Prompt { .. }));

                // Track if we actually added anything to display
                let mut added_content = false;

                for element in elements {
                    match element {
                        ParsedElement::Text { content, fg_color, bg_color, bold, .. } => {
                            // Only add text if it has actual content
                            if !content.trim().is_empty() {
                                self.get_current_window().add_text(StyledText {
                                    content: content.clone(),
                                    fg: fg_color.and_then(|c| Self::parse_hex_color(&c)),
                                    bg: bg_color.and_then(|c| Self::parse_hex_color(&c)),
                                    bold,
                                });
                                added_content = true;
                                // Reset prompt_shown flag when we see actual text content
                                self.prompt_shown = false;
                            }
                        }
                        ParsedElement::Prompt { text, .. } => {
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
                                            debug!("Matched prompt char '{}' to color {}", char_str, pc.color);
                                            Self::parse_hex_color(&pc.color)
                                        })
                                        .unwrap_or_else(|| {
                                            debug!("No match for prompt char '{}', using default", char_str);
                                            Color::DarkGray
                                        });

                                    self.get_current_window().add_text(StyledText {
                                        content: char_str,
                                        fg: Some(color),
                                        bg: None,
                                        bold: false,
                                    });
                                }
                                added_content = true;
                                self.prompt_shown = true;
                            }
                        }
                        ParsedElement::StreamPush { id } => {
                            // Switch to new stream
                            debug!("Pushing stream: {}", id);
                            self.current_stream = id;
                        }
                        ParsedElement::StreamPop => {
                            // Return to main stream
                            debug!("Popping stream, returning to main");
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

                                    // Strip the prefix from text (e.g., "mana 407/407" -> "407/407")
                                    let display_text = if text.contains('/') {
                                        // Has numbers - strip the prefix
                                        text.split_whitespace().skip(1).collect::<Vec<_>>().join(" ")
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
                                    debug!("Updated label '{}' to {}% with text '{}'", id, num, value);
                                } else {
                                    // No number found, just show the text at 0%
                                    window.set_progress_with_text(0, 100, Some(value.clone()));
                                    debug!("Updated label '{}' with text '{}' (no value)", id, value);
                                }
                            } else {
                                debug!("No window found for label id '{}'", id);
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
                            // <castTime value='3'/>
                            // value is Unix timestamp when cast time ends
                            if let Some(window) = self.window_manager.get_window("casttime") {
                                window.set_countdown(value as u64);
                                debug!("Updated casttime to end at {}", value);
                            }
                        }
                        ParsedElement::Compass { directions } => {
                            // <compass><dir value="n"/><dir value="e"/>...</compass>
                            // Update compass widget with available exits
                            if let Some(window) = self.window_manager.get_window("compass") {
                                window.set_compass_directions(directions.clone());
                                debug!("Updated compass with directions: {:?}", directions);
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
                        _ => {
                            // Other element types don't add visible content
                        }
                    }
                }

                // Only finish the line if we actually added visible content
                if added_content {
                    // Finish the line after processing all elements
                    // Get current terminal width for wrapping
                    if let Ok(size) = crossterm::terminal::size() {
                        let inner_width = size.0.saturating_sub(2); // Account for borders
                        self.get_current_window().finish_line(inner_width);
                    }
                }
            }
        }
    }

    fn add_system_message(&mut self, msg: &str) {
        self.get_current_window().add_text(StyledText {
            content: format!("*** {} ***", msg),
            fg: Some(Color::Yellow),
            bg: None,
            bold: true,
        });
        // Finish the line
        if let Ok(size) = crossterm::terminal::size() {
            let inner_width = size.0.saturating_sub(2);
            self.get_current_window().finish_line(inner_width);
        }
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
}
