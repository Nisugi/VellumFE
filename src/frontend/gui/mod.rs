//! GUI Frontend - Native GUI using egui
//!
//! This module implements the GUI frontend using egui/eframe.
//! It provides a native windowed interface with moveable/resizable widgets.
//!
//! ## Numpad Key Support
//!
//! Numpad key distinction is provided by a forked eframe which intercepts
//! keyboard events at the winit level before egui-winit processes them.
//! This allows numpad keys (num_0 through num_9, num_+, etc.) to be bound
//! separately from regular number keys.

mod input;
mod runtime;
mod theme_integration;
mod widgets;
mod window_editor;
mod window_manager;

pub use runtime::run;
use theme_integration::app_theme_to_visuals;
use widgets::parse_hex_to_color32;

use crate::core::AppCore;
use crate::data::ui_state::InputMode;
use crate::data::widget::LinkData;
use crate::data::window::{WidgetType, WindowContent};
use crate::network::ServerMessage;
use anyhow::Result;
use eframe::egui;
use std::collections::HashMap;
use tokio::sync::mpsc;
use window_editor::GuiWindowEditor;
use window_manager::WindowManager;

/// Parse widget category from debug string representation
/// This converts strings like "Other" to WidgetCategory::Other
fn parse_widget_category(s: &str) -> Option<crate::config::WidgetCategory> {
    match s {
        "ActiveEffects" => Some(crate::config::WidgetCategory::ActiveEffects),
        "Countdown" => Some(crate::config::WidgetCategory::Countdown),
        "Entity" => Some(crate::config::WidgetCategory::Entity),
        "Hand" => Some(crate::config::WidgetCategory::Hand),
        "Other" => Some(crate::config::WidgetCategory::Other),
        "ProgressBar" => Some(crate::config::WidgetCategory::ProgressBar),
        "Status" => Some(crate::config::WidgetCategory::Status),
        "TextWindow" => Some(crate::config::WidgetCategory::TextWindow),
        _ => None,
    }
}

/// Check if an action command opens a submenu (should NOT close menu)
fn action_opens_submenu(command: &str) -> bool {
    matches!(
        command,
        "action:addwindow" | "action:hidewindow" | "action:editwindow"
    )
}

/// Link drag state for Ctrl+drag operations
#[derive(Clone, Debug)]
struct LinkDragState {
    link_data: LinkData,
}

/// Currently hovered link (for drag target detection)
#[derive(Clone, Debug, Default)]
struct HoveredLink {
    link_data: Option<LinkData>,
}

/// Cached texture data for an injury doll window
struct InjuryTextureCache {
    /// egui texture handle (retained for rendering)
    silhouette: egui::TextureHandle,
    /// Numbered marker texture (if using Numbers style)
    #[allow(dead_code)] // Reserved for future numbered marker feature
    markers: Option<egui::TextureHandle>,
    /// Original image dimensions (pre-scaling)
    original_size: (u32, u32),
    /// Last modification time of image file (for reload detection)
    #[allow(dead_code)] // Reserved for future hot-reload feature
    last_modified: Option<std::time::SystemTime>,
    /// Currently loaded image path (for profile change detection)
    loaded_image_path: String,

    // Phase 5: Multi-layer overlay textures
    /// Overlay layer textures (e.g., nervous_system, nerves_greyscale)
    /// Key: overlay name from config
    overlay_textures: std::collections::HashMap<String, egui::TextureHandle>,
    /// Rank indicator textures (rank1, rank2, rank3, nerves)
    rank_textures: Option<(egui::TextureHandle, egui::TextureHandle, egui::TextureHandle, egui::TextureHandle)>,
}

/// Main GUI application struct
pub struct EguiApp {
    /// Core application state (game state, UI state, config)
    app_core: AppCore,

    /// GUI window manager (tracks pixel positions, z-order)
    window_manager: WindowManager,

    /// Command input text (for the command input widget)
    command_input: String,

    /// Command history
    command_history: Vec<String>,
    history_index: Option<usize>,

    /// Receiver for server messages
    server_rx: Option<mpsc::UnboundedReceiver<ServerMessage>>,

    /// Sender for commands to server
    command_tx: Option<mpsc::UnboundedSender<String>>,

    /// Connection status
    connected: bool,

    /// Link drag state for Ctrl+drag operations
    link_drag_state: Option<LinkDragState>,

    /// Currently hovered link (for drag target detection)
    hovered_link: HoveredLink,

    /// Last link click position in pixels (for popup menu placement)
    last_link_click_pos: Option<egui::Pos2>,

    /// Request focus on command input (set after menu close, link click, etc.)
    request_command_focus: bool,

    /// Skip menu Enter key for one frame (prevents Enter from .menu command opening submenu)
    skip_menu_enter_frames: u8,

    /// Open window editor panels (window_name → editor state)
    window_editors: HashMap<String, GuiWindowEditor>,

    /// Current theme ID (e.g., "dracula", "gruvbox-dark")
    current_theme_id: String,

    /// Cached theme (avoid HashMap lookup on hot path)
    cached_theme: crate::theme::AppTheme,

    /// Texture cache for injury doll images (window_name → texture data)
    injury_textures: HashMap<String, InjuryTextureCache>,

    /// GUI state for tabbed text windows (window_name → tab state)
    tabbed_text_states: HashMap<String, widgets::GuiTabbedTextState>,

    /// Pending numpad key events intercepted by custom event loop
    /// These are processed BEFORE regular egui keys in handle_keybinds()
    pub pending_numpad_keys: Vec<crate::frontend::common::KeyEvent>,
}

impl EguiApp {
    /// Create a new GUI application (standalone mode, no network)
    pub fn new(app_core: AppCore) -> Self {
        let mut window_manager = WindowManager::new();
        // Initialize window manager with positions from app_core
        Self::init_window_manager(&app_core, &mut window_manager);

        // Load initial theme
        let current_theme_id = app_core.config.active_theme.clone();
        let cached_theme = app_core.config.get_theme();

        Self {
            app_core,
            window_manager,
            command_input: String::new(),
            command_history: Vec::new(),
            history_index: None,
            server_rx: None,
            command_tx: None,
            connected: false,
            link_drag_state: None,
            hovered_link: HoveredLink::default(),
            last_link_click_pos: None,
            request_command_focus: false,
            skip_menu_enter_frames: 0,
            window_editors: HashMap::new(),
            current_theme_id,
            cached_theme,
            injury_textures: HashMap::new(),
            tabbed_text_states: HashMap::new(),
            pending_numpad_keys: Vec::new(),
        }
    }

    /// Create a new GUI application with network channels
    pub fn new_with_network(
        app_core: AppCore,
        server_rx: mpsc::UnboundedReceiver<ServerMessage>,
        command_tx: mpsc::UnboundedSender<String>,
    ) -> Self {
        let mut window_manager = WindowManager::new();
        // Initialize window manager with positions from app_core
        Self::init_window_manager(&app_core, &mut window_manager);

        // Load initial theme
        let current_theme_id = app_core.config.active_theme.clone();
        let cached_theme = app_core.config.get_theme();

        Self {
            app_core,
            window_manager,
            command_input: String::new(),
            command_history: Vec::new(),
            history_index: None,
            server_rx: Some(server_rx),
            command_tx: Some(command_tx),
            connected: false,
            link_drag_state: None,
            hovered_link: HoveredLink::default(),
            last_link_click_pos: None,
            request_command_focus: false,
            skip_menu_enter_frames: 0,
            window_editors: HashMap::new(),
            current_theme_id,
            cached_theme,
            injury_textures: HashMap::new(),
            tabbed_text_states: HashMap::new(),
            pending_numpad_keys: Vec::new(),
        }
    }

    /// Initialize window manager from AppCore's window positions
    fn init_window_manager(app_core: &AppCore, window_manager: &mut WindowManager) {
        // Convert TUI character positions to pixel positions
        let windows: Vec<_> = app_core
            .ui_state
            .windows
            .iter()
            .map(|(name, state)| {
                (
                    name.clone(),
                    state.position.x,
                    state.position.y,
                    state.position.width,
                    state.position.height,
                )
            })
            .collect();
        window_manager.init_from_layout(&windows);
    }

    /// Run the GUI application
    pub fn run(self) -> Result<()> {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size([800.0, 600.0])
                .with_title("VellumFE"),
            persist_window: true,  // Persist main window position/size
            ..Default::default()
        };

        eframe::run_native(
            "VellumFE",
            options,
            Box::new(move |cc| {
                // Main window position/size is persisted via persist_window: true
                configure_style(&cc.egui_ctx, &self.cached_theme);
                Ok(Box::new(self))
            }),
        )
        .map_err(|e| anyhow::anyhow!("Failed to run GUI: {}", e))
    }

    /// Poll for server messages and process them
    pub fn poll_server_messages(&mut self) {
        if let Some(ref mut rx) = self.server_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    ServerMessage::Text(line) => {
                        // Process through AppCore's parser
                        if let Err(e) = self.app_core.process_server_data(&line) {
                            tracing::error!("Error processing server data: {}", e);
                        }
                    }
                    ServerMessage::Connected => {
                        tracing::info!("Connected to game server");
                        self.connected = true;
                        self.app_core.game_state.connected = true;
                    }
                    ServerMessage::Disconnected => {
                        tracing::info!("Disconnected from game server");
                        self.connected = false;
                        self.app_core.game_state.connected = false;
                    }
                }
            }
        }
    }

    /// Send a command to the server
    pub fn send_command(&mut self, command: String) {
        let trimmed = command.trim();

        // Add to history (even for dot commands)
        if !trimmed.is_empty() {
            self.command_history.push(trimmed.to_string());
            self.history_index = None;
        }

        // Check for dot commands - handle locally via AppCore
        if trimmed.starts_with('.') {
            tracing::info!("Processing dot command: {}", trimmed);

            // Handle .themes command (list available themes)
            if trimmed == ".themes" {
                self.list_themes();
                return;
            }

            // Handle .theme <name> command (switch theme)
            if let Some(theme_name) = trimmed.strip_prefix(".theme ") {
                self.switch_theme(theme_name.trim());
                return;
            }

            // Check if menu was open before command
            let had_menu = self.app_core.ui_state.has_menu();

            // Process through AppCore which handles dot commands
            match self.app_core.send_command(trimmed.to_string()) {
                Ok(response) => {
                    // AppCore may return action commands like "action:addwindow"
                    if response.starts_with("action:") {
                        tracing::info!("Dot command returned action: {}", response);
                        self.handle_action_command(&response);
                    }
                    // Other responses (like empty string) are already handled by AppCore
                }
                Err(e) => {
                    tracing::error!("Dot command error: {}", e);
                }
            }

            // If menu was just opened by this command, skip Enter for 2 frames
            // This prevents the Enter key from .menu immediately opening a submenu
            if !had_menu && self.app_core.ui_state.has_menu() {
                self.skip_menu_enter_frames = 2;
            }
            return;
        }

        // Regular command - send to server
        if let Some(ref tx) = self.command_tx {
            if let Err(e) = tx.send(command) {
                tracing::error!("Failed to send command: {}", e);
            }
        }
    }

    /// Switch to a new theme
    fn switch_theme(&mut self, theme_id: &str) {
        use crate::theme::ThemePresets;

        // Get all available themes
        let all_themes = ThemePresets::all_with_custom(self.app_core.config.character.as_deref());

        // Try to load the theme
        if let Some(new_theme) = all_themes.get(theme_id) {
            self.current_theme_id = theme_id.to_string();
            self.cached_theme = new_theme.clone();

            // Update config
            self.app_core.config.active_theme = theme_id.to_string();

            // Save config to disk
            if let Err(e) = self.app_core.config.save(self.app_core.config.character.as_deref()) {
                tracing::error!("Failed to save theme to config: {}", e);
            }

            // Add feedback message to main window
            self.app_core.add_system_message(&format!("Theme switched to: {}", theme_id));
        } else {
            // Theme not found
            self.app_core.add_system_message(&format!(
                "Error: Theme '{}' not found. Use .themes to list available themes",
                theme_id
            ));
        }
    }

    /// List all available themes
    fn list_themes(&mut self) {
        use crate::theme::ThemePresets;

        let themes = ThemePresets::all_with_custom(self.app_core.config.character.as_deref());
        let mut output = String::from("[Available themes:]\n");

        for (id, theme) in themes.iter() {
            let marker = if *id == self.current_theme_id { " *" } else { "" };
            output.push_str(&format!("  - {} ({}){}\n", id, theme.name, marker));
        }

        output.push_str(&format!("\nCurrent: {}", self.current_theme_id));
        output.push_str("\nUse .theme <name> to switch");

        self.app_core.add_system_message(&output);
    }

    /// Navigate command history up
    fn history_up(&mut self) {
        if self.command_history.is_empty() {
            return;
        }

        self.history_index = Some(match self.history_index {
            None => self.command_history.len() - 1,
            Some(0) => 0,
            Some(i) => i - 1,
        });

        if let Some(idx) = self.history_index {
            self.command_input = self.command_history[idx].clone();
        }
    }

    /// Navigate command history down
    fn history_down(&mut self) {
        if self.command_history.is_empty() {
            return;
        }

        match self.history_index {
            None => {}
            Some(i) if i >= self.command_history.len() - 1 => {
                self.history_index = None;
                self.command_input.clear();
            }
            Some(i) => {
                self.history_index = Some(i + 1);
                self.command_input = self.command_history[i + 1].clone();
            }
        }
    }

    /// Render a text window widget with styled content
    /// Returns TextWindowResponse with any link interactions
    fn render_text_window(&self, ui: &mut egui::Ui, window_name: &str) -> widgets::TextWindowResponse {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Text(content) = &window.content {
                // Try to get config from layout
                if let Some(window_def) = self.app_core.layout.get_window(window_name) {
                    if let crate::config::WindowDef::Text { base, data } = window_def {
                        let font_family = base.font_family.as_deref();
                        return widgets::render_text_window(ui, content, data, window_name, font_family);
                    }
                } else {
                    // Use defaults if not in layout
                    let default_config = crate::config::TextWidgetData::default();
                    return widgets::render_text_window(ui, content, &default_config, window_name, None);
                }
            }
        }
        // Fallback for windows without content
        ui.weak("Waiting for data...");
        widgets::TextWindowResponse::default()
    }

    /// Render a tabbed text window widget with multiple tabs
    /// Returns TabbedTextWindowResponse with link interactions and tab clicks
    fn render_tabbed_text_window(
        &mut self,
        ui: &mut egui::Ui,
        window_name: &str,
    ) -> widgets::TabbedTextWindowResponse {
        // Get or create GUI state for this window
        let gui_state = self
            .tabbed_text_states
            .entry(window_name.to_string())
            .or_insert_with(widgets::GuiTabbedTextState::new);

        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::TabbedText(content) = &window.content {
                // Try to get config from layout
                if let Some(window_def) = self.app_core.layout.get_window(window_name) {
                    if let crate::config::WindowDef::TabbedText { base, data } = window_def {
                        let font_family = base.font_family.as_deref();
                        return widgets::render_tabbed_text_window(
                            ui,
                            content,
                            data,
                            gui_state,
                            font_family,
                        );
                    }
                }

                // Use defaults if not in layout
                let default_config = crate::config::TabbedTextWidgetData::default();
                return widgets::render_tabbed_text_window(
                    ui,
                    content,
                    &default_config,
                    gui_state,
                    None,
                );
            }
        }

        // Fallback for windows without content
        ui.weak("Waiting for data...");
        widgets::TabbedTextWindowResponse::default()
    }

    /// Render an active effects window widget (buffs, debuffs, cooldowns, active spells)
    fn render_active_effects_window(&self, ui: &mut egui::Ui, window_name: &str) {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::ActiveEffects(content) = &window.content {
                // Try to get config from layout
                if let Some(config_from_layout) = self
                    .app_core
                    .layout
                    .get_window(window_name)
                    .and_then(|w| {
                        if let crate::config::WindowDef::ActiveEffects { data, .. } = w {
                            Some(data)
                        } else {
                            None
                        }
                    })
                {
                    widgets::render_active_effects(ui, content, config_from_layout, window_name);
                } else {
                    // Use defaults if not in layout
                    let default_config = crate::config::ActiveEffectsWidgetData {
                        category: content.category.clone(),
                        style: crate::config::ActiveEffectsStyle::Overlay,
                        bar_height: 18.0,
                        bar_opacity: 0.85,
                        bar_rounding: 2.0,
                        text_size: 14.0,
                        show_timer: true,
                        show_percentage: false,
                        timer_position: crate::config::TimerPosition::Right,
                        spacing: 2.0,
                        auto_contrast: true,
                        text_shadow: true,
                        outline_text: false,
                        animate_changes: false,
                        pulse_expiring: false,
                        expiring_threshold: 30,
                    };
                    widgets::render_active_effects(ui, content, &default_config, window_name);
                }
                return;
            }
        }
        // Fallback for windows without content
        ui.weak("Waiting for data...");
    }

    /// Handle a link click - delegates to AppCore for centralized logic
    fn handle_link_click(&mut self, link_data: &LinkData, click_pos: egui::Pos2) {
        // Store pixel position for GUI popup placement (in case _menu is sent)
        self.last_link_click_pos = Some(click_pos);

        // Use centralized AppCore method for consistent behavior across frontends
        // Pass (0,0) for terminal coords since GUI uses pixel coords stored above
        let command = self.app_core.handle_link_click(link_data, (0, 0));
        self.send_command(command);
    }

    /// Handle link drag end - sends _drag command to server
    fn handle_link_drag_end(&mut self, source_link: &LinkData) {
        // Check if we dropped on another link (target)
        let command = if let Some(ref target_link) = self.hovered_link.link_data {
            // Don't drag to self
            if target_link.exist_id != source_link.exist_id {
                format!("_drag #{} #{}\n", source_link.exist_id, target_link.exist_id)
            } else {
                // Dropped on self - treat as drop
                format!("_drag #{} drop\n", source_link.exist_id)
            }
        } else {
            // No target link - drop to ground
            format!("_drag #{} drop\n", source_link.exist_id)
        };

        tracing::info!("Link drag ended: {}", command.trim());
        self.send_command(command);
    }

    /// Handle action commands (action:addwindow, action:showwindow:name, etc.)
    /// Returns true if the command was handled as an action, false otherwise
    fn handle_action_command(&mut self, command: &str) -> bool {
        // Handle action:showwindow:<name>
        if let Some(window_name) = command.strip_prefix("action:showwindow:") {
            tracing::info!("Action: show window '{}'", window_name);
            // Show window from layout template (use 0,0 for terminal size - GUI uses pixels)
            self.app_core.show_window(window_name, 0, 0);
            // Close menus
            self.app_core.ui_state.close_all_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            self.request_command_focus = true;
            return true;
        }

        // Handle action:hidewindow:<name>
        if let Some(window_name) = command.strip_prefix("action:hidewindow:") {
            tracing::info!("Action: hide window '{}'", window_name);
            self.app_core.hide_window(window_name);
            // Close menus
            self.app_core.ui_state.close_all_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            self.request_command_focus = true;
            return true;
        }

        // Handle action:createwindow:<type> - create window from template
        if let Some(widget_type) = command.strip_prefix("action:createwindow:") {
            tracing::info!("Action: create window of type '{}'", widget_type);
            // For GUI, we create the window at a default position
            // In the future, could open a dialog to let user configure
            if let Some(template) = crate::config::Config::get_window_template(widget_type) {
                self.app_core.add_new_window(&template, 0, 0);
                // Initialize in window manager using get_or_create
                let name = template.name().to_string();
                let base = template.base();
                // Convert from terminal chars to approximate pixels (8px per char width, 18px height)
                let pixel_x = base.col as f32 * 8.0;
                let pixel_y = base.row as f32 * 18.0;
                let pixel_w = base.cols as f32 * 8.0;
                let pixel_h = base.rows as f32 * 18.0;
                let state = self.window_manager.get_or_create(&name);
                state.position = [pixel_x, pixel_y];
                state.size = [pixel_w, pixel_h];
                state.visible = true;
            }
            // Close menus
            self.app_core.ui_state.close_all_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            self.request_command_focus = true;
            return true;
        }

        // Handle __SUBMENU_ADD__<category> commands - open widget category submenu
        if let Some(category_str) = command.strip_prefix("__SUBMENU_ADD__") {
            tracing::info!("Action: open widget category submenu '{}'", category_str);
            if let Some(category) = parse_widget_category(category_str) {
                let items = self.app_core.build_add_window_category_menu(&category);
                if !items.is_empty() {
                    // Push submenu with the category's widgets
                    self.app_core.ui_state.push_menu(
                        crate::data::ui_state::PopupMenu::new(items, (0, 0))
                    );
                    tracing::info!("Opened widget category submenu: {:?}", category);
                }
            }
            return true;
        }

        // Handle generic __SUBMENU__<category> commands - open config submenu
        if let Some(category) = command.strip_prefix("__SUBMENU__") {
            tracing::info!("Action: open submenu '{}'", category);
            // Try build_submenu first, then menu_categories
            let items = self.app_core.build_submenu(category);
            let items = if !items.is_empty() {
                items
            } else if let Some(cached_items) = self.app_core.menu_categories.get(category) {
                cached_items.clone()
            } else {
                Vec::new()
            };

            if !items.is_empty() {
                self.app_core.ui_state.push_menu(
                    crate::data::ui_state::PopupMenu::new(items, (0, 0))
                );
                tracing::info!("Opened submenu: {}", category);
            }
            return true;
        }

        // Handle __ADD__<template> commands from menu selection
        if let Some(template_name) = command.strip_prefix("__ADD__") {
            tracing::info!("Action: add window from template '{}'", template_name);

            // Get the window template and add using core (syncs to ui_state.windows)
            if let Some(template) = crate::config::Config::get_window_template(template_name) {
                // Add window using core - this syncs to ui_state.windows
                self.app_core.add_new_window(&template, 0, 0);

                // Initialize in window manager for GUI pixel positioning
                let base = template.base();
                let pixel_x = base.col as f32 * 8.0;
                let pixel_y = base.row as f32 * 18.0;
                let pixel_w = base.cols as f32 * 8.0;
                let pixel_h = base.rows as f32 * 18.0;
                let state = self.window_manager.get_or_create(template.name());
                state.position = [pixel_x, pixel_y];
                state.size = [pixel_w, pixel_h];
                state.visible = true;

                tracing::info!("Added window '{}' from template", template_name);
            } else {
                tracing::warn!("Unknown window template: {}", template_name);
            }

            // Close menus
            self.app_core.ui_state.close_all_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            self.request_command_focus = true;
            return true;
        }

        // Handle __EDIT__<window_name> commands from menu selection
        if let Some(window_name) = command.strip_prefix("__EDIT__") {
            tracing::info!("Action: edit window '{}'", window_name);

            // Get WindowDef from layout
            if let Some(window_def) = self.app_core.layout.windows.iter()
                .find(|w| w.name() == window_name)
                .cloned()
            {
                // Get panel position/size from config (or defaults)
                let position = self.app_core.config.window_editor.panel_positions
                    .get(window_name)
                    .copied()
                    .unwrap_or(self.app_core.config.window_editor.default_position);

                let size = self.app_core.config.window_editor.panel_sizes
                    .get(window_name)
                    .copied()
                    .unwrap_or(self.app_core.config.window_editor.default_size);

                // Create editor (or replace if already exists)
                let editor = GuiWindowEditor::new(
                    window_name.to_string(),
                    window_def,
                    position,
                    size,
                );

                self.window_editors.insert(window_name.to_string(), editor);
                tracing::info!("Opened editor for window '{}'", window_name);
            } else {
                tracing::warn!("Window '{}' not found in layout", window_name);
            }

            // Close menus
            self.app_core.ui_state.close_all_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            self.request_command_focus = true;
            return true;
        }

        // Handle exact action commands
        match command {
            "action:addwindow" => {
                tracing::info!("Action: open add window picker");
                // Build the widget category menu and push as new menu level
                let items = self.app_core.build_add_window_menu();
                if !items.is_empty() {
                    self.app_core.ui_state.push_menu(
                        crate::data::ui_state::PopupMenu::new(items, (0, 0))
                    );
                    self.app_core.ui_state.input_mode = InputMode::Menu;
                }
                true
            }
            "action:hidewindow" => {
                tracing::info!("Action: open hide window picker");
                // Build the hide window menu and push as new menu level
                let items = self.app_core.build_hide_window_menu();
                if !items.is_empty() {
                    self.app_core.ui_state.push_menu(
                        crate::data::ui_state::PopupMenu::new(items, (0, 0))
                    );
                    self.app_core.ui_state.input_mode = InputMode::Menu;
                }
                true
            }
            "action:editwindow" => {
                tracing::info!("Action: open edit window picker");
                // Build the edit window menu and push as new menu level
                let items = self.app_core.build_edit_window_menu();
                if !items.is_empty() {
                    self.app_core.ui_state.push_menu(
                        crate::data::ui_state::PopupMenu::new(items, (0, 0))
                    );
                    self.app_core.ui_state.input_mode = InputMode::Menu;
                }
                true
            }
            "action:listwindows" | "action:windows" => {
                tracing::info!("Action: list windows");
                // Execute the .windows command locally
                let _ = self.app_core.send_command(".windows".to_string());
                // Close menus
                self.app_core.ui_state.close_all_menus();
                self.app_core.ui_state.input_mode = InputMode::Normal;
                self.request_command_focus = true;
                true
            }
            _ => false,
        }
    }

    /// Render a progress bar widget (health, mana, stamina, etc.)
    fn render_progress_window(&self, ui: &mut egui::Ui, window_name: &str) {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Progress(data) = &window.content {
                // Get configuration
                let progress_config = self.app_core.layout.windows.iter()
                    .find(|w| w.name() == window_name)
                    .and_then(|w| {
                        if let crate::config::WindowDef::Progress { data, .. } = w {
                            Some(data.clone())
                        } else {
                            None
                        }
                    });

                let config = progress_config.unwrap_or_default();

                // Calculate fraction
                let fraction = if data.max > 0 {
                    (data.value as f32 / data.max as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                // Build text based on custom format or defaults
                let text = if let Some(ref format) = config.text_format {
                    // Support {value}, {max}, {percent} placeholders
                    let percent = (fraction * 100.0) as u32;
                    format
                        .replace("{value}", &data.value.to_string())
                        .replace("{max}", &data.max.to_string())
                        .replace("{percent}", &percent.to_string())
                } else if config.current_only {
                    data.value.to_string()
                } else if config.numbers_only {
                    format!("{}/{}", data.value, data.max)
                } else if let Some(ref label) = config.label {
                    format!("{}: {}/{}", label, data.value, data.max)
                } else {
                    format!("{}/{}", data.value, data.max)
                };

                // Render based on text position
                match config.text_position {
                    crate::config::ProgressTextPosition::Above => {
                        // Text above bar
                        let mut text_label = egui::RichText::new(&text).size(config.text_size);
                        if let Some(ref color_hex) = config.color {
                            if let Some(color) = widgets::parse_hex_to_color32(color_hex) {
                                text_label = text_label.color(color);
                            }
                        }
                        ui.label(text_label);

                        // Progress bar below
                        let mut bar = egui::ProgressBar::new(fraction)
                            .desired_height(config.bar_height)
                            .corner_radius(config.rounding);
                        if let Some(ref color_hex) = config.color {
                            if let Some(color) = widgets::parse_hex_to_color32(color_hex) {
                                bar = bar.fill(color);
                            }
                        }
                        ui.add(bar);
                    }
                    crate::config::ProgressTextPosition::Below => {
                        // Progress bar above
                        let mut bar = egui::ProgressBar::new(fraction)
                            .desired_height(config.bar_height)
                            .corner_radius(config.rounding);
                        if let Some(ref color_hex) = config.color {
                            if let Some(color) = widgets::parse_hex_to_color32(color_hex) {
                                bar = bar.fill(color);
                            }
                        }
                        ui.add(bar);

                        // Text below
                        let mut text_label = egui::RichText::new(&text).size(config.text_size);
                        if let Some(ref color_hex) = config.color {
                            if let Some(color) = widgets::parse_hex_to_color32(color_hex) {
                                text_label = text_label.color(color);
                            }
                        }
                        ui.label(text_label);
                    }
                    crate::config::ProgressTextPosition::Inside => {
                        // Text inside bar (default egui behavior)
                        let mut bar = egui::ProgressBar::new(fraction)
                            .desired_height(config.bar_height)
                            .corner_radius(config.rounding);

                        // Apply text with size
                        let mut rich_text = egui::RichText::new(&text).size(config.text_size);
                        if let Some(ref color_hex) = config.color {
                            if let Some(color) = widgets::parse_hex_to_color32(color_hex) {
                                bar = bar.fill(color);
                                // For inside text, use auto-contrast
                                let luminance = 0.299 * color.r() as f32
                                    + 0.587 * color.g() as f32
                                    + 0.114 * color.b() as f32;
                                let text_color = if luminance > 128.0 {
                                    egui::Color32::BLACK
                                } else {
                                    egui::Color32::WHITE
                                };
                                rich_text = rich_text.color(text_color);
                            }
                        }

                        bar = bar.text(rich_text);
                        ui.add(bar);
                    }
                }
            }
        }
    }

    /// Render a countdown timer widget (RT/CT)
    fn render_countdown_window(&self, ui: &mut egui::Ui, window_name: &str) {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Countdown(data) = &window.content {
                // Get configuration
                let countdown_config = self.app_core.layout.windows.iter()
                    .find(|w| w.name() == window_name)
                    .and_then(|w| {
                        if let crate::config::WindowDef::Countdown { data, .. } = w {
                            Some(data.clone())
                        } else {
                            None
                        }
                    });

                let config = countdown_config.unwrap_or_default();

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let remaining = (data.end_time - now).max(0);

                if remaining > 0 {
                    // Format time based on config
                    let time_text = match config.format {
                        crate::config::CountdownFormat::Seconds => {
                            format!("{}s", remaining)
                        }
                        crate::config::CountdownFormat::MMss => {
                            let minutes = remaining / 60;
                            let seconds = remaining % 60;
                            format!("{:02}:{:02}", minutes, seconds)
                        }
                        crate::config::CountdownFormat::HHMMss => {
                            let hours = remaining / 3600;
                            let minutes = (remaining % 3600) / 60;
                            let seconds = remaining % 60;
                            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
                        }
                    };

                    // Build display text with label
                    let display_text = if let Some(ref label) = config.label {
                        format!("{}: {}", label, time_text)
                    } else {
                        time_text
                    };

                    // Calculate fraction using max_time
                    let fraction = (remaining as f32 / config.max_time as f32).clamp(0.0, 1.0);

                    // Determine color based on alert threshold
                    let is_alert = remaining <= config.alert_threshold as i64;
                    let bar_color = if is_alert {
                        config.alert_color.as_ref()
                            .and_then(|c| widgets::parse_hex_to_color32(c))
                            .or_else(|| Some(egui::Color32::from_rgb(255, 0, 0)))
                    } else {
                        config.color.as_ref()
                            .and_then(|c| widgets::parse_hex_to_color32(c))
                    };

                    // Create progress bar with styling
                    let mut bar = egui::ProgressBar::new(fraction);

                    if let Some(color) = bar_color {
                        bar = bar.fill(color);
                    }

                    // Apply text with size
                    let rich_text = egui::RichText::new(&display_text).size(config.text_size);
                    bar = bar.text(rich_text);

                    ui.add(bar);
                } else {
                    // Ready state
                    let ready_text = if let Some(ref label) = config.label {
                        format!("{}: Ready", label)
                    } else {
                        "Ready".to_string()
                    };

                    let mut text_label = egui::RichText::new(&ready_text).size(config.text_size);

                    // Apply color if specified
                    if let Some(ref color_hex) = config.color {
                        if let Some(color) = widgets::parse_hex_to_color32(color_hex) {
                            text_label = text_label.color(color);
                        }
                    }

                    ui.label(text_label);
                }
            }
        }
    }

    /// Render compass widget
    fn render_compass_window(&self, ui: &mut egui::Ui, window_name: &str) {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Compass(data) = &window.content {
                // Get Compass configuration from layout
                let compass_config = self.app_core.layout.windows.iter()
                    .find(|w| w.name() == window_name)
                    .and_then(|w| {
                        if let crate::config::WindowDef::Compass { data, .. } = w {
                            Some(data.clone())
                        } else {
                            None
                        }
                    });

                let config = compass_config.unwrap_or_default();

                // Determine colors
                let active_color = config.active_color.as_ref()
                    .and_then(|c| parse_hex_to_color32(c))
                    .unwrap_or(egui::Color32::from_rgb(0, 255, 0)); // Green

                let inactive_color = config.inactive_color.as_ref()
                    .and_then(|c| parse_hex_to_color32(c))
                    .unwrap_or(egui::Color32::from_rgb(85, 85, 85)); // Gray

                // Direction icons (if enabled)
                let direction_icons: std::collections::HashMap<&str, &str> = [
                    ("n", "↑"), ("s", "↓"), ("e", "→"), ("w", "←"),
                    ("ne", "↗"), ("nw", "↖"), ("se", "↘"), ("sw", "↙"),
                    ("up", "⬆"), ("down", "⬇"), ("out", "◯"),
                ].iter().cloned().collect();

                // Helper function to render a direction
                let render_dir = |ui: &mut egui::Ui, dir: &str| {
                    let active = data.directions.contains(&dir.to_string());
                    let color = if active { active_color } else { inactive_color };

                    let text = if config.use_icons {
                        direction_icons.get(dir).unwrap_or(&dir).to_string()
                    } else {
                        dir.to_string()
                    };

                    let mut rich_text = egui::RichText::new(text)
                        .size(config.text_size)
                        .color(color);

                    if active && config.bold_active {
                        rich_text = rich_text.strong();
                    }

                    ui.label(rich_text);
                    ui.add_space(config.spacing);
                };

                // Render based on layout
                match config.layout {
                    crate::config::CompassLayout::Grid3x3 => {
                        // Traditional 3x3 grid
                        ui.horizontal(|ui| {
                            for dir in &["nw", "n", "ne"] {
                                render_dir(ui, dir);
                            }
                        });
                        ui.horizontal(|ui| {
                            for dir in &["w", "out", "e"] {
                                render_dir(ui, dir);
                            }
                        });
                        ui.horizontal(|ui| {
                            for dir in &["sw", "s", "se"] {
                                render_dir(ui, dir);
                            }
                        });
                        ui.horizontal(|ui| {
                            for dir in &["up", "down"] {
                                render_dir(ui, dir);
                            }
                        });
                    }
                    crate::config::CompassLayout::Horizontal => {
                        // Single horizontal row
                        ui.horizontal(|ui| {
                            for dir in &["nw", "n", "ne", "w", "out", "e", "sw", "s", "se", "up", "down"] {
                                render_dir(ui, dir);
                            }
                        });
                    }
                    crate::config::CompassLayout::Vertical => {
                        // Single vertical column
                        for dir in &["nw", "n", "ne", "w", "out", "e", "sw", "s", "se", "up", "down"] {
                            ui.horizontal(|ui| {
                                render_dir(ui, dir);
                            });
                        }
                    }
                }
            }
        }
    }

    /// Sync room data from app_core.room_components to WindowContent::Room
    /// This is the GUI equivalent of TUI's sync_room_windows
    pub fn sync_room_data(&mut self) {
        use crate::data::widget::{RoomContent, StyledLine};

        // Only sync if room data has changed
        if !self.app_core.room_window_dirty {
            return;
        }

        // Find all room-type windows
        let room_windows: Vec<String> = self
            .app_core
            .ui_state
            .windows
            .iter()
            .filter(|(_, state)| matches!(state.widget_type, WidgetType::Room))
            .map(|(name, _)| name.clone())
            .collect();

        if room_windows.is_empty() {
            // No room windows to sync to, but clear dirty flag anyway
            self.app_core.room_window_dirty = false;
            return;
        }

        // Build RoomContent from room_components
        // Keys: "room desc", "room objs", "room players", "room exits"
        let description: Vec<StyledLine> = self
            .app_core
            .room_components
            .get("room desc")
            .map(|lines| {
                lines
                    .iter()
                    .map(|segments| StyledLine {
                        segments: segments.clone(),
                        timestamp: None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Extract plain text from objects, players, exits
        // These are stored as styled text but we need Vec<String>
        let objects: Vec<String> = self
            .app_core
            .room_components
            .get("room objs")
            .map(|lines| {
                lines
                    .iter()
                    .map(|segments| {
                        segments
                            .iter()
                            .map(|seg| seg.text.as_str())
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let players: Vec<String> = self
            .app_core
            .room_components
            .get("room players")
            .map(|lines| {
                lines
                    .iter()
                    .map(|segments| {
                        segments
                            .iter()
                            .map(|seg| seg.text.as_str())
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let exits: Vec<String> = self
            .app_core
            .room_components
            .get("room exits")
            .map(|lines| {
                lines
                    .iter()
                    .map(|segments| {
                        segments
                            .iter()
                            .map(|seg| seg.text.as_str())
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        // Build room name/title from subtitle and IDs
        let name = self.build_room_title();

        let room_content = RoomContent {
            name,
            description,
            exits,
            players,
            objects,
        };

        // Update all room windows with the new content
        for window_name in room_windows {
            if let Some(window_state) = self.app_core.ui_state.windows.get_mut(&window_name) {
                window_state.content = WindowContent::Room(room_content.clone());
            }
        }

        // Clear dirty flag
        self.app_core.room_window_dirty = false;
    }

    /// Build room window title from room data (same as TUI)
    fn build_room_title(&self) -> String {
        let subtitle = &self.app_core.room_subtitle;
        let lich_id = &self.app_core.lich_room_id;
        let nav_id = &self.app_core.nav_room_id;

        if let Some(ref subtitle_text) = subtitle {
            if let Some(ref lich) = lich_id {
                if let Some(ref nav) = nav_id {
                    format!("[{} - {}] (u{})", subtitle_text, lich, nav)
                } else {
                    format!("[{} - {}]", subtitle_text, lich)
                }
            } else if let Some(ref nav) = nav_id {
                format!("[{}] (u{})", subtitle_text, nav)
            } else {
                format!("[{}]", subtitle_text)
            }
        } else if let Some(ref lich) = lich_id {
            if let Some(ref nav) = nav_id {
                format!("[{}] (u{})", lich, nav)
            } else {
                format!("[{}]", lich)
            }
        } else if let Some(ref nav) = nav_id {
            format!("(u{})", nav)
        } else {
            String::new()
        }
    }

    /// Render indicator widget (status icons)
    fn render_indicator_window(&self, ui: &mut egui::Ui, window_name: &str) {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Indicator(data) = &window.content {
                // Get Indicator configuration from layout
                let indicator_config = self.app_core.layout.windows.iter()
                    .find(|w| w.name() == window_name)
                    .and_then(|w| {
                        if let crate::config::WindowDef::Indicator { data, .. } = w {
                            Some(data.clone())
                        } else {
                            None
                        }
                    });

                let config = indicator_config.unwrap_or_default();

                // Determine color based on active/inactive state
                let color = if data.active {
                    config.active_color.as_ref()
                        .and_then(|c| parse_hex_to_color32(c))
                        .unwrap_or(egui::Color32::from_rgb(0, 255, 0)) // Green
                } else {
                    config.inactive_color.as_ref()
                        .and_then(|c| parse_hex_to_color32(c))
                        .unwrap_or(egui::Color32::from_rgb(85, 85, 85)) // Gray
                };

                ui.horizontal(|ui| {
                    // Render based on shape type
                    match config.shape {
                        crate::config::IndicatorShape::Circle => {
                            // Draw a circle indicator
                            let (rect, _response) = ui.allocate_exact_size(
                                egui::vec2(config.indicator_size, config.indicator_size),
                                egui::Sense::hover(),
                            );
                            let center = rect.center();
                            let radius = config.indicator_size / 2.0;

                            // Draw glow if active and enabled
                            if data.active && config.glow_when_active && config.glow_radius > 0.0 {
                                ui.painter().circle_filled(
                                    center,
                                    radius + config.glow_radius,
                                    egui::Color32::from_rgba_premultiplied(
                                        color.r(),
                                        color.g(),
                                        color.b(),
                                        80, // Translucent glow
                                    ),
                                );
                            }

                            // Draw main circle
                            ui.painter().circle_filled(center, radius, color);
                        }
                        crate::config::IndicatorShape::Square => {
                            // Draw a square indicator
                            let (rect, _response) = ui.allocate_exact_size(
                                egui::vec2(config.indicator_size, config.indicator_size),
                                egui::Sense::hover(),
                            );

                            // Draw glow if active and enabled
                            if data.active && config.glow_when_active && config.glow_radius > 0.0 {
                                let glow_rect = rect.expand(config.glow_radius);
                                ui.painter().rect_filled(
                                    glow_rect,
                                    2.0,
                                    egui::Color32::from_rgba_premultiplied(
                                        color.r(),
                                        color.g(),
                                        color.b(),
                                        80,
                                    ),
                                );
                            }

                            // Draw main square
                            ui.painter().rect_filled(rect, 2.0, color);
                        }
                        crate::config::IndicatorShape::Icon => {
                            // Use icon character if provided
                            if let Some(icon) = &config.icon {
                                let text = egui::RichText::new(icon)
                                    .size(config.indicator_size)
                                    .color(color);
                                ui.label(text);
                            } else {
                                // Fallback to text
                                let text = egui::RichText::new(&data.indicator_id)
                                    .size(config.text_size)
                                    .color(color);
                                if data.active {
                                    ui.label(text.strong());
                                } else {
                                    ui.label(text);
                                }
                            }
                        }
                        crate::config::IndicatorShape::Text => {
                            // Text-only mode (no shape)
                            let text = egui::RichText::new(&data.indicator_id)
                                .size(config.text_size)
                                .color(color);
                            if data.active {
                                ui.label(text.strong());
                            } else {
                                ui.label(text);
                            }
                        }
                    }

                    // Show label if enabled
                    if config.show_label && !matches!(config.shape, crate::config::IndicatorShape::Text) {
                        ui.add_space(4.0);
                        let label_text = egui::RichText::new(&data.indicator_id)
                            .size(config.text_size)
                            .color(color);
                        if data.active {
                            ui.label(label_text.strong());
                        } else {
                            ui.label(label_text);
                        }
                    }
                });
            }
        }
    }

    /// Render hand widget (left/right hand items)
    fn render_hand_window(&self, ui: &mut egui::Ui, window_name: &str) {
        // Note: Removed ui.set_min_size(available) as it prevented resizing to smaller sizes
        // The window will now size based on its content, allowing 1-row layouts

        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Hand { item, .. } = &window.content {
                // Get Hand configuration from layout
                let hand_config = self.app_core.layout.windows.iter()
                    .find(|w| w.name() == window_name)
                    .and_then(|w| {
                        if let crate::config::WindowDef::Hand { data, .. } = w {
                            Some(data.clone())
                        } else {
                            None
                        }
                    });

                let config = hand_config.unwrap_or_default();

                // Render within frame if background is enabled
                let render_content = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        // Icon with optional color
                        if let Some(icon_text) = &config.icon {
                            let mut icon_label = egui::RichText::new(icon_text)
                                .size(config.icon_size);

                            if let Some(icon_color_hex) = &config.icon_color {
                                if let Some(icon_color) = widgets::parse_hex_to_color32(icon_color_hex) {
                                    icon_label = icon_label.color(icon_color);
                                }
                            }

                            ui.label(icon_label);
                            ui.add_space(config.spacing);
                        }

                        // Item text or empty
                        match item {
                            Some(text) => {
                                let mut item_label = egui::RichText::new(text)
                                    .size(config.text_size);

                                if let Some(text_color_hex) = &config.text_color {
                                    if let Some(text_color) = widgets::parse_hex_to_color32(text_color_hex) {
                                        item_label = item_label.color(text_color);
                                    }
                                }

                                ui.label(item_label);
                            }
                            None => {
                                let empty_text = config.empty_text.as_deref().unwrap_or("Empty");
                                let mut empty_label = egui::RichText::new(empty_text)
                                    .size(config.text_size)
                                    .weak();

                                if let Some(empty_color_hex) = &config.empty_color {
                                    if let Some(empty_color) = widgets::parse_hex_to_color32(empty_color_hex) {
                                        empty_label = empty_label.color(empty_color);
                                    }
                                }

                                ui.label(empty_label);
                            }
                        }
                    });
                };

                // Apply background frame if enabled
                if config.show_background {
                    let mut frame = egui::Frame::new()
                        .inner_margin(4.0);

                    if let Some(bg_color_hex) = &config.background_color {
                        if let Some(bg_color) = widgets::parse_hex_to_color32(bg_color_hex) {
                            frame = frame.fill(bg_color);
                        }
                    }

                    frame.show(ui, render_content);
                } else {
                    render_content(ui);
                }
            }
        }
    }

    /// Render room window widget with description, objects, players, and exits
    /// Returns RoomWindowResponse with any link interactions
    fn render_room_window(
        &self,
        ui: &mut egui::Ui,
        window_name: &str,
    ) -> widgets::RoomWindowResponse {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Room(content) = &window.content {
                // TODO: Add component visibility state to GUI window manager
                // For now, show all components by default
                let visibility = widgets::RoomComponentVisibility::default();
                return widgets::render_room_window(ui, content, &visibility, window_name);
            }
        }
        // Fallback for windows without room content
        ui.weak("Waiting for room data...");
        widgets::RoomWindowResponse::default()
    }

    /// Render injury doll window with image-based visualization
    fn render_injury_window(&mut self, ui: &mut egui::Ui, window_name: &str) {
        // Extract content and config first to avoid borrow conflicts
        let content_clone = self
            .app_core
            .ui_state
            .windows
            .get(window_name)
            .and_then(|w| {
                if let WindowContent::InjuryDoll(content) = &w.content {
                    Some(content.clone())
                } else {
                    None
                }
            });

        let Some(content) = content_clone else {
            ui.weak("Waiting for injury data...");
            return;
        };

        // Get config from layout
        let config = self
            .app_core
            .layout
            .get_window(window_name)
            .and_then(|w| {
                if let crate::config::WindowDef::InjuryDoll { data, .. } = w {
                    Some(data.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                // Fallback to default config
                crate::config::InjuryDollWidgetData {
                    injury_default_color: None,
                    injury1_color: Some("#aa5500".to_string()),
                    injury2_color: Some("#ff8800".to_string()),
                    injury3_color: Some("#ff0000".to_string()),
                    scar1_color: Some("#999999".to_string()),
                    scar2_color: Some("#777777".to_string()),
                    scar3_color: Some("#555555".to_string()),
                    image_path: Some("defaults/injuryDoll.png".to_string()),
                    scale: 1.0,
                    greyscale: false,
                    tint_color: None,
                    tint_strength: 0.3,
                    marker_tint_strength: 0.3,
                    marker_style: crate::config::InjuryMarkerStyle::Circles,
                    marker_size: 6.0,
                    show_numbers: true,
                    calibration: crate::config::InjuryCalibration::default(),
                    image_profiles: Vec::new(),
                    // Phase 5: Multi-layer system
                    tint_mode: crate::config::TintMode::Unified,
                    overlay_tint_color: None,
                    overlay_tint_strength: 0.3,
                    nerve_indicator_type: crate::config::NerveIndicatorType::Default,
                    overlay_layers: crate::config::default_overlay_layers(),
                    rank_indicators: crate::config::RankIndicatorConfig::default(),
                    background_color: None,
                }
            });

        // PHASE 2.5: Get hand data for profile selection
        let mut right_hand_item: Option<&str> = None;
        let mut left_hand_item: Option<&str> = None;

        for (name, window) in &self.app_core.ui_state.windows {
            if let WindowContent::Hand { item, .. } = &window.content {
                if name.to_lowercase().contains("right") {
                    right_hand_item = item.as_deref();
                } else if name.to_lowercase().contains("left") {
                    left_hand_item = item.as_deref();
                }
            }
        }

        // Select profile based on hand data
        let (selected_image_path, selected_calibration) = config.select_profile(right_hand_item, left_hand_item);

        // Create effective config with selected profile's image/calibration
        let mut effective_config = config.clone();
        effective_config.image_path = Some(selected_image_path.to_string());
        effective_config.calibration = selected_calibration.clone();

        // Check calibration mode BEFORE calling get_injury_texture to avoid borrow conflicts
        let (calibration_mode, calibration_target) = self
            .window_editors
            .get(window_name)
            .and_then(|editor| {
                editor.injury_doll_editor.as_ref().map(|doll_editor| {
                    if doll_editor.calibration_active {
                        let target_name = window_editor::GuiWindowEditor::get_body_part_name(doll_editor.calibration_index);
                        (true, Some(target_name))
                    } else {
                        (false, None)
                    }
                })
            })
            .unwrap_or((false, None));

        // Load texture (now we can mutably borrow self) - use effective_config with selected profile
        if let Some(texture_cache) = self.get_injury_texture(ui.ctx(), window_name, &effective_config) {
            // Render widget (use effective_config with selected profile's calibration)
            // Phase 5: Pass overlay textures and rank indicators
            let response = widgets::render_injury_doll(
                ui,
                &content,
                &effective_config,
                &texture_cache.silhouette,
                &texture_cache.overlay_textures,
                &texture_cache.rank_textures,
                calibration_mode,
                calibration_target,
            );

            // Handle calibration click response
            if let (Some(clicked_part), Some((x, y))) = (response.clicked_body_part, response.clicked_position) {
                // Update calibration data in the window editor's modified_def
                if let Some(editor) = self.window_editors.get_mut(window_name) {
                    if let Some(injury_editor) = &mut editor.injury_doll_editor {
                        // Update the calibration data in the modified WindowDef
                        if let crate::config::WindowDef::InjuryDoll { data, .. } = &mut editor.modified_def {
                            // Update the body part position in calibration data
                            data.calibration.body_parts
                                .entry(clicked_part.clone())
                                .and_modify(|bp| {
                                    bp.x = x;
                                    bp.y = y;
                                })
                                .or_insert(crate::config::InjuryBodyPart {
                                    x,
                                    y,
                                    enabled: true,
                                });

                            // AUTO-APPLY: Also update the live layout immediately for real-time feedback
                            // This makes calibration clicks visible without hitting Apply button
                            if let Some(window) = self.app_core.layout.windows.iter_mut()
                                .find(|w| w.name() == window_name)
                            {
                                if let crate::config::WindowDef::InjuryDoll { data: live_data, .. } = window {
                                    live_data.calibration.body_parts
                                        .entry(clicked_part)
                                        .and_modify(|bp| {
                                            bp.x = x;
                                            bp.y = y;
                                        })
                                        .or_insert(crate::config::InjuryBodyPart {
                                            x,
                                            y,
                                            enabled: true,
                                        });
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Texture loading failed - show error
            ui.vertical_centered(|ui| {
                ui.colored_label(egui::Color32::LIGHT_RED, "⚠ Image Load Failed");
                ui.label(format!("Path: {:?}", effective_config.image_path));
                ui.separator();
                ui.label("Check:");
                ui.label("• File exists and is readable");
                ui.label("• Path is correct (relative to config dir)");
                ui.label("• File is valid PNG format");

                // Fallback: show text-based injury list
                ui.separator();
                ui.label("Injuries (fallback):");
                for (part, level) in &content.injuries {
                    if *level > 0 {
                        ui.label(format!("{}: Level {}", part, level));
                    }
                }
            });
        }
    }

    /// Load or retrieve cached injury texture for a window
    fn get_injury_texture(
        &mut self,
        ctx: &egui::Context,
        window_name: &str,
        config: &crate::config::InjuryDollWidgetData,
    ) -> Option<&InjuryTextureCache> {
        let requested_image_path = config.image_path.as_deref().unwrap_or("defaults/injuryDoll.png");
        let resolved_path = self.resolve_image_path(requested_image_path);
        let resolved_path_str = resolved_path.to_string_lossy().to_string();

        // PHASE 2.5: Check if texture already loaded AND matches requested profile image
        if let Some(cached) = self.injury_textures.get(window_name) {
            // If cached image path matches requested path, return cached texture
            if cached.loaded_image_path == resolved_path_str {
                return self.injury_textures.get(window_name);
            }
            // Otherwise, fall through to reload with new profile image
        }

        // Load texture from file (either first load or profile changed)
        if let Ok(mut texture_cache) = self.load_injury_texture(ctx, &resolved_path) {
            // Phase 5: Also load overlay textures and rank indicators
            texture_cache.overlay_textures = self.load_overlay_textures(ctx, config, window_name);
            texture_cache.rank_textures = self.load_rank_textures(ctx, config, window_name);

            self.injury_textures
                .insert(window_name.to_string(), texture_cache);
            return self.injury_textures.get(window_name);
        }

        None
    }

    /// Resolve image path (handle relative paths, config dir, etc.)
    /// Priority: user images dir → config dir → executable dir → as-is
    fn resolve_image_path(&self, path: &str) -> std::path::PathBuf {
        let p = std::path::Path::new(path);

        if p.is_absolute() {
            return p.to_path_buf();
        }

        // Extract just the filename if path contains "defaults/" prefix
        let filename = if path.starts_with("defaults/") {
            path.strip_prefix("defaults/").unwrap_or(path)
        } else {
            path
        };

        // 1. Try user's images directory (~/.vellum-fe/global/images/)
        if let Ok(images_dir) = crate::config::Config::images_dir() {
            let user_image_path = images_dir.join(filename);
            if user_image_path.exists() {
                tracing::debug!("Loading image from user directory: {:?}", user_image_path);
                return user_image_path;
            }
        }

        // 2. Try relative to config directory (for backwards compatibility)
        if let Ok(config_dir) = crate::config::Config::base_dir() {
            let config_path = config_dir.join(path);
            if config_path.exists() {
                tracing::debug!("Loading image from config directory: {:?}", config_path);
                return config_path;
            }
        }

        // 3. Try relative to executable (embedded defaults)
        if let Ok(exe_dir) = std::env::current_exe() {
            if let Some(parent) = exe_dir.parent() {
                let exe_path = parent.join(path);
                if exe_path.exists() {
                    tracing::debug!("Loading image from executable directory: {:?}", exe_path);
                    return exe_path;
                }
            }
        }

        // Fallback: return as-is
        tracing::warn!("Image not found in any location: {}", path);
        p.to_path_buf()
    }

    /// Load texture from image file
    fn load_injury_texture(
        &self,
        ctx: &egui::Context,
        path: &std::path::Path,
    ) -> Result<InjuryTextureCache> {
        use image::GenericImageView;

        // Load image using the image crate
        let img = image::open(path)?;

        let (width, height) = img.dimensions();
        let rgba_image = img.to_rgba8();
        let pixels = rgba_image.as_flat_samples();

        // Convert to egui ColorImage
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            pixels.as_slice(),
        );

        // Upload to GPU
        let texture = ctx.load_texture(
            format!("injury_doll_{:?}", path),
            color_image,
            egui::TextureOptions::LINEAR,
        );

        // Get file modification time
        let last_modified = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok());

        Ok(InjuryTextureCache {
            silhouette: texture,
            markers: None, // TODO: Load injuryNumbers.png
            original_size: (width, height),
            last_modified,
            loaded_image_path: path.to_string_lossy().to_string(),
            // Phase 5: Overlays loaded separately per-window configuration
            overlay_textures: std::collections::HashMap::new(),
            rank_textures: None,
        })
    }

    /// Load a single texture from a file path (Phase 5: helper for overlays/rank indicators)
    fn load_single_texture(
        &self,
        ctx: &egui::Context,
        path: &std::path::Path,
        name: &str,
    ) -> Result<egui::TextureHandle> {
        use image::GenericImageView;

        let img = image::open(path)?;
        let (width, height) = img.dimensions();
        let rgba_image = img.to_rgba8();
        let pixels = rgba_image.as_flat_samples();

        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            pixels.as_slice(),
        );

        Ok(ctx.load_texture(
            name.to_string(),
            color_image,
            egui::TextureOptions::LINEAR,
        ))
    }

    /// Load overlay textures for injury doll (Phase 5)
    fn load_overlay_textures(
        &self,
        ctx: &egui::Context,
        config: &crate::config::InjuryDollWidgetData,
        window_name: &str,
    ) -> std::collections::HashMap<String, egui::TextureHandle> {
        let mut overlay_textures = std::collections::HashMap::new();

        eprintln!("[INJURY] Loading overlays for {}: {} layers in config", window_name, config.overlay_layers.len());

        for overlay in &config.overlay_layers {
            eprintln!("[INJURY]   Overlay '{}': enabled={}, path={}", overlay.name, overlay.enabled, overlay.image_path);

            if !overlay.enabled {
                continue;
            }

            let path = self.resolve_image_path(&overlay.image_path);
            let texture_name = format!("injury_overlay_{}_{}", window_name, overlay.name);

            match self.load_single_texture(ctx, &path, &texture_name) {
                Ok(texture) => {
                    eprintln!("[INJURY]     Successfully loaded overlay texture: {:?}", path);
                    overlay_textures.insert(overlay.name.clone(), texture);
                }
                Err(e) => {
                    eprintln!("[INJURY]     ERROR: Failed to load overlay texture {:?}: {}", path, e);
                }
            }
        }

        eprintln!("[INJURY] Loaded {} overlay textures for {}", overlay_textures.len(), window_name);
        overlay_textures
    }

    /// Load rank indicator textures (Phase 5)
    fn load_rank_textures(
        &self,
        ctx: &egui::Context,
        config: &crate::config::InjuryDollWidgetData,
        window_name: &str,
    ) -> Option<(egui::TextureHandle, egui::TextureHandle, egui::TextureHandle, egui::TextureHandle)> {
        if !config.rank_indicators.enabled {
            return None;
        }

        let rank1_path = self.resolve_image_path(&config.rank_indicators.rank1_path);
        let rank2_path = self.resolve_image_path(&config.rank_indicators.rank2_path);
        let rank3_path = self.resolve_image_path(&config.rank_indicators.rank3_path);
        let nerves_path = self.resolve_image_path(&config.rank_indicators.nerves_path);

        let r1_name = format!("injury_rank1_{}", window_name);
        let r2_name = format!("injury_rank2_{}", window_name);
        let r3_name = format!("injury_rank3_{}", window_name);
        let nerves_name = format!("injury_nerves_{}", window_name);

        match (
            self.load_single_texture(ctx, &rank1_path, &r1_name),
            self.load_single_texture(ctx, &rank2_path, &r2_name),
            self.load_single_texture(ctx, &rank3_path, &r3_name),
            self.load_single_texture(ctx, &nerves_path, &nerves_name),
        ) {
            (Ok(r1), Ok(r2), Ok(r3), Ok(nerves)) => Some((r1, r2, r3, nerves)),
            _ => None,
        }
    }

    /// Render a generic placeholder for unsupported widget types
    fn render_placeholder_window(&self, ui: &mut egui::Ui, widget_type: &WidgetType) {
        ui.weak(format!("{:?} widget", widget_type));
    }

    /// Render popup menu if visible (stack-based for unlimited depth)
    /// Returns the command to execute if a menu item was clicked
    fn render_popup_menu(&mut self, ctx: &egui::Context) -> Option<String> {
        // Check if there's any menu to render
        if self.app_core.ui_state.menu_stack.is_empty() {
            return None;
        }

        // Track what to return
        let mut result_command: Option<String> = None;
        let mut should_close_all = false;
        let mut submenu_to_open: Option<(usize, String)> = None; // (level, command)

        // Track all menu rectangles for click-outside detection and positioning
        let mut all_menu_rects: Vec<egui::Rect> = Vec::new();
        const MENU_OVERLAP: f32 = 2.0;

        // Active level is the deepest menu (last in stack)
        let active_level = self.app_core.ui_state.menu_stack.len() - 1;

        // Use stored pixel position for main menu, or fallback to center of screen
        let base_pos = self.last_link_click_pos.unwrap_or_else(|| {
            let rect = ctx.available_rect();
            egui::pos2(rect.width() / 2.0, rect.height() / 2.0)
        });

        // Handle keyboard input for menu navigation
        let (key_up, key_down, mut key_enter, key_escape, key_left, mut key_right) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            )
        });

        // Skip Enter/Right keys for a few frames after menu opens
        if self.skip_menu_enter_frames > 0 {
            self.skip_menu_enter_frames -= 1;
            key_enter = false;
            key_right = false;
            tracing::debug!("Skipping menu Enter/Right key (frames remaining: {})", self.skip_menu_enter_frames);
        }

        // Handle Escape - close one level at a time
        if key_escape {
            self.app_core.ui_state.pop_menu();
            if self.app_core.ui_state.menu_stack.is_empty() {
                self.app_core.ui_state.input_mode = InputMode::Normal;
                self.request_command_focus = true;
            }
            return None; // Consume the escape
        }

        // Handle Left arrow - go back one level (but not from main menu)
        if key_left && self.app_core.ui_state.menu_stack.len() > 1 {
            self.app_core.ui_state.pop_menu();
            return None; // Consume the left arrow
        }

        // Track if keyboard navigation was used this frame
        let keyboard_nav_used = key_up || key_down;

        // Clone menu stack for iteration (needed because we'll mutate during render)
        let menu_count = self.app_core.ui_state.menu_stack.len();

        // Render each menu level
        for level in 0..menu_count {
            // Clone the menu data we need for this level
            let (items, selected) = {
                let menu = &self.app_core.ui_state.menu_stack[level];
                (menu.items.clone(), menu.selected)
            };

            // Calculate position: stack horizontally with slight overlap
            let x_offset: f32 = all_menu_rects.iter()
                .map(|r| r.width() - MENU_OVERLAP)
                .sum();
            let menu_pos = egui::pos2(base_pos.x + x_offset, base_pos.y);

            // Handle keyboard only for the active (deepest) level
            if level == active_level {
                // Up/Down navigation
                if key_up && !items.is_empty() {
                    let new_sel = if selected == 0 { items.len() - 1 } else { selected - 1 };
                    if let Some(menu) = self.app_core.ui_state.menu_at_level_mut(level) {
                        menu.selected = new_sel;
                    }
                }
                if key_down && !items.is_empty() {
                    let new_sel = (selected + 1) % items.len();
                    if let Some(menu) = self.app_core.ui_state.menu_at_level_mut(level) {
                        menu.selected = new_sel;
                    }
                }

                // Enter/Right to select current item
                if key_enter || key_right {
                    // Re-read selection after up/down processing
                    let current_sel = self.app_core.ui_state.menu_at_level(level)
                        .map(|m| m.selected)
                        .unwrap_or(selected);
                    if let Some(item) = items.get(current_sel) {
                        if !item.disabled && !item.command.is_empty() {
                            if item.command.starts_with("__SUBMENU")
                               || action_opens_submenu(&item.command)
                            {
                                submenu_to_open = Some((level, item.command.clone()));
                            } else {
                                result_command = Some(item.command.clone());
                                should_close_all = true;
                            }
                        }
                    }
                }
            }

            // Render the menu for this level
            let area_response = egui::Area::new(egui::Id::new(format!("menu_level_{}", level)))
                .fixed_pos(menu_pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .show(ui, |ui| {
                            for (idx, item) in items.iter().enumerate() {
                                let is_selected = idx == selected;
                                let is_disabled = item.disabled;

                                let text = if is_disabled {
                                    egui::RichText::new(&item.text).weak()
                                } else if is_selected {
                                    egui::RichText::new(&item.text)
                                        .background_color(egui::Color32::from_rgb(60, 80, 120))
                                } else {
                                    egui::RichText::new(&item.text)
                                };

                                let response = ui.add(
                                    egui::Label::new(text)
                                        .wrap_mode(egui::TextWrapMode::Extend)
                                        .sense(if is_disabled { egui::Sense::hover() } else { egui::Sense::click() })
                                );

                                // Only update selection on hover if keyboard wasn't used this frame
                                // and this is the active level
                                if response.hovered() && !is_disabled && !keyboard_nav_used && level == active_level {
                                    if let Some(menu) = self.app_core.ui_state.menu_at_level_mut(level) {
                                        menu.selected = idx;
                                    }
                                }

                                // Handle click
                                if response.clicked() && !is_disabled && !item.command.is_empty() {
                                    tracing::debug!("Menu click at level {}: command='{}'", level, &item.command);
                                    if item.command.starts_with("__SUBMENU")
                                       || action_opens_submenu(&item.command)
                                    {
                                        submenu_to_open = Some((level, item.command.clone()));
                                    } else {
                                        result_command = Some(item.command.clone());
                                        should_close_all = true;
                                    }
                                }
                            }
                        });
                });

            all_menu_rects.push(area_response.response.rect);
        }

        // Check for click outside all menus to close
        let clicked_outside = ctx.input(|i| {
            if !i.pointer.any_click() {
                return false;
            }
            let click_pos = i.pointer.interact_pos().unwrap_or_default();
            !all_menu_rects.iter().any(|rect| rect.contains(click_pos))
        });

        if clicked_outside {
            should_close_all = true;
        }

        // Process submenu opening
        if let Some((level, cmd)) = submenu_to_open {
            tracing::debug!("Opening submenu at level {}: {}", level, cmd);

            if let Some(new_items) = self.build_menu_items_for_command(&cmd) {
                // Close any menus deeper than current level
                while self.app_core.ui_state.menu_stack.len() > level + 1 {
                    self.app_core.ui_state.pop_menu();
                }
                // Push new submenu
                self.app_core.ui_state.push_menu(
                    crate::data::ui_state::PopupMenu::new(new_items, (0, 0))
                );
                tracing::info!("Opened submenu (now at depth {})", self.app_core.ui_state.menu_depth());
            }
        }

        // Handle close all
        if should_close_all {
            self.app_core.ui_state.close_all_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            self.request_command_focus = true;
        }

        // Debug: Log what we're returning
        if result_command.is_some() {
            tracing::debug!(
                "render_popup_menu returning: result_command={:?}, should_close_all={}",
                result_command, should_close_all
            );
        }

        result_command
    }

    /// Build menu items for a command (submenu or action)
    /// Returns None if the command doesn't produce menu items
    fn build_menu_items_for_command(&self, cmd: &str) -> Option<Vec<crate::data::ui_state::PopupMenuItem>> {
        // Handle action commands that open submenus
        if let Some(action_cmd) = cmd.strip_prefix("action:") {
            match action_cmd {
                "addwindow" => {
                    let items = self.app_core.build_add_window_menu();
                    if !items.is_empty() { return Some(items); }
                }
                "hidewindow" => {
                    let items = self.app_core.build_hide_window_menu();
                    if !items.is_empty() { return Some(items); }
                }
                "editwindow" => {
                    let items = self.app_core.build_edit_window_menu();
                    if !items.is_empty() { return Some(items); }
                }
                _ => {
                    tracing::warn!("Unknown action command: {}", action_cmd);
                }
            }
            return None;
        }

        // Handle widget category submenu (e.g., __SUBMENU_ADD__Hand)
        if let Some(category_str) = cmd.strip_prefix("__SUBMENU_ADD__") {
            if let Some(category) = parse_widget_category(category_str) {
                let items = self.app_core.build_add_window_category_menu(&category);
                if !items.is_empty() {
                    tracing::info!("Built widget category submenu: {:?}", category);
                    return Some(items);
                }
            }
            return None;
        }

        // Handle generic submenu (e.g., __SUBMENU__Windows)
        if let Some(category) = cmd.strip_prefix("__SUBMENU__") {
            // Try build_submenu first (for config menus), then menu_categories
            let items = self.app_core.build_submenu(category);
            let items = if !items.is_empty() {
                items
            } else if let Some(cached_items) = self.app_core.menu_categories.get(category) {
                cached_items.clone()
            } else {
                Vec::new()
            };

            if !items.is_empty() {
                tracing::info!("Built submenu: {}", category);
                return Some(items);
            }
        }

        None
    }

    /// Get the background clear color for the window (from theme)
    pub fn get_clear_color(&self) -> [f32; 3] {
        let bg = &self.cached_theme.window_background;
        [
            bg.r as f32 / 255.0,
            bg.g as f32 / 255.0,
            bg.b as f32 / 255.0,
        ]
    }

    /// Render the main UI (called by custom event loop)
    /// This is the main rendering entry point, containing all the window rendering logic
    pub fn render_ui(&mut self, ctx: &egui::Context) {
        // Decrement skip menu enter frames counter
        if self.skip_menu_enter_frames > 0 {
            self.skip_menu_enter_frames -= 1;
        }

        // Background panel
        egui::CentralPanel::default().show(ctx, |_ui| {});

        // Connection status
        let status = if self.connected {
            "🟢 Connected"
        } else {
            "🔴 Disconnected"
        };

        // Collect window info to avoid borrow issues
        let windows_info: Vec<_> = self
            .app_core
            .ui_state
            .windows
            .iter()
            .filter(|(_, state)| state.visible)
            .map(|(name, state)| {
                let gui_state = self.window_manager.get_or_create(name);
                (
                    name.clone(),
                    state.widget_type.clone(),
                    gui_state.position,
                    gui_state.size,
                    gui_state.show_title_bar,
                    gui_state.position_override,
                )
            })
            .collect();

        // Note: This is a simplified render_ui for the custom event loop.
        // The full rendering logic from the eframe::App::update method will be called
        // when we fully transition to the custom loop.
        // For now, the eframe::App impl handles rendering via update().

        // TODO: Move full rendering logic here when custom event loop is fully enabled
        let _ = (status, windows_info); // Suppress unused warnings
    }

    /// Handle keybinds from egui input
    /// Returns commands to send to server (from macros)
    ///
    /// NOTE: Numpad keys are processed FIRST via `pending_numpad_keys` which are
    /// intercepted by the custom event loop BEFORE egui-winit merges them with
    /// regular digit keys.
    pub fn handle_keybinds(&mut self, ctx: &egui::Context) -> Vec<String> {
        let mut commands = Vec::new();

        // Skip all keybind processing in Menu/Search modes
        if self.app_core.ui_state.input_mode == InputMode::Menu
            || self.app_core.ui_state.input_mode == InputMode::Search
        {
            // Clear pending numpad keys to avoid stale events
            self.pending_numpad_keys.clear();
            return commands;
        }

        // 1. Process pending numpad keys FIRST (these have higher priority)
        // These were intercepted by the custom event loop before egui-winit
        // could merge them with regular digit keys.
        for key_event in std::mem::take(&mut self.pending_numpad_keys) {
            if let Some(action) = self.app_core.keybind_map.get(&key_event) {
                let action = action.clone();

                // Check for quit keybind
                let key_str = input::key_event_to_keybind_string(&key_event);
                if key_str == self.app_core.config.global_keybinds.quit {
                    self.app_core.quit();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    continue;
                }

                // Execute the keybind action via AppCore
                match self.app_core.execute_keybind_action(&action) {
                    Ok(cmds) => {
                        commands.extend(cmds);
                    }
                    Err(e) => {
                        tracing::error!("Error executing numpad keybind action: {}", e);
                    }
                }

                tracing::debug!("Processed numpad keybind: {} -> {:?}", key_str, action);
            }
        }

        // 2. Process regular egui key events
        let key_events = input::get_key_events(ctx);

        for key_event in key_events {
            // First check if this key has a binding
            if let Some(action) = self.app_core.keybind_map.get(&key_event) {
                let action = action.clone(); // Clone to avoid borrow issues

                // Check for special keybinds that need GUI-level handling
                // (e.g., quit, which needs to close the window)
                let key_str = input::key_event_to_keybind_string(&key_event);
                if key_str == self.app_core.config.global_keybinds.quit {
                    // Call quit() to trigger autosave before closing
                    self.app_core.quit();
                    // Request close - egui will handle this
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    continue;
                }

                // Execute the keybind action via AppCore
                match self.app_core.execute_keybind_action(&action) {
                    Ok(cmds) => {
                        commands.extend(cmds);
                    }
                    Err(e) => {
                        tracing::error!("Error executing keybind action: {}", e);
                    }
                }

                tracing::debug!("Processed keybind: {} -> {:?}", key_str, action);
            }
        }

        commands
    }
}

impl eframe::App for EguiApp {
    // The new eframe 0.33.3 API requires ui() but we still use update()
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Not used - we use update() instead which gives us access to Context
    }

    #[allow(deprecated)]
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Capture numpad keys from eframe FIRST (before any other processing)
        // These are intercepted by the forked eframe before egui-winit loses
        // the KeyLocation::Numpad information.
        for numpad_event in frame.numpad_keys() {
            // Only process key presses, not releases
            if !numpad_event.pressed {
                continue;
            }
            // Convert to our platform-independent KeyEvent
            if let Some(key_event) = input::convert_numpad_key_event(numpad_event) {
                self.pending_numpad_keys.push(key_event);
                tracing::debug!(
                    "Captured numpad key: {:?} (numlock={})",
                    numpad_event.physical_key,
                    numpad_event.numlock_on
                );
            }
        }

        // Poll for server messages
        self.poll_server_messages();

        // Apply theme at start of each frame
        ctx.set_visuals(app_theme_to_visuals(&self.cached_theme));

        // Sync room data from room_components to WindowContent::Room
        self.sync_room_data();

        // Handle keybinds (before UI rendering to ensure immediate response)
        let keybind_commands = self.handle_keybinds(ctx);
        for cmd in keybind_commands {
            self.send_command(cmd);
        }

        // Background panel
        egui::CentralPanel::default().show(ctx, |_ui| {});

        // Connection status
        let status = if self.connected {
            "🟢 Connected"
        } else {
            "🔴 Disconnected"
        };

        // Collect window info to avoid borrow issues
        // Include GUI state (position, size, show_title_bar, position_override)
        let windows_info: Vec<_> = self
            .app_core
            .ui_state
            .windows
            .iter()
            .filter(|(_, state)| state.visible)
            .map(|(name, state)| {
                let gui_state = self.window_manager.get_or_create(name);
                (
                    name.clone(),
                    state.widget_type.clone(),
                    gui_state.position,
                    gui_state.size,
                    gui_state.show_title_bar,
                    gui_state.position_override,
                )
            })
            .collect();

        // Track windows that need title bar toggled (from context menu)
        // Store name and current rect for anchor-aware toggle
        let mut title_bar_toggles: Vec<(String, egui::Rect)> = Vec::new();

        // Track link interactions from text windows
        let mut clicked_links: Vec<(LinkData, egui::Pos2)> = Vec::new();
        let mut drag_started_links: Vec<LinkData> = Vec::new();
        let mut hovered_links: Vec<LinkData> = Vec::new();

        // Track windows that need position override cleared
        let mut clear_position_overrides: Vec<String> = Vec::new();

        // Track window pixel sizes for syncing to open editors
        // (window_name, pixel_width, pixel_height)
        let mut window_pixel_sizes: Vec<(String, f32, f32)> = Vec::new();

        // Collect window names for send-to-back operation (before moving windows_info)
        let all_window_names: Vec<String> = windows_info.iter().map(|(name, _, _, _, _, _)| name.clone()).collect();

        // Render each window dynamically based on widget type
        for (name, widget_type, default_pos, default_size, show_title_bar, position_override) in windows_info {
            // Determine window title (used as ID even if title bar hidden)
            let title = if name == "main" {
                format!("{} - {}", name, status)
            } else {
                name.clone()
            };

            // Get show_border from layout's window definition
            let show_border = self.app_core.layout.get_window(&name)
                .map(|w| w.base().show_border)
                .unwrap_or(true); // Default to showing border

            // Create the egui window
            // Use current_pos if position_override is set (one-frame force), otherwise default_pos
            // IMPORTANT: scroll([false, false]) disables the Window's built-in scroll handling
            // This is critical for preventing window growth - we manage scrolling ourselves via ScrollArea
            let mut window = egui::Window::new(&title)
                .id(egui::Id::new(&name))
                .default_size(default_size)
                .default_open(true)
                .resizable(true)
                .scroll([false, false])  // Disable Window scroll - we use ScrollArea inside widgets
                .collapsible(true)
                .title_bar(show_title_bar)
                // Always movable so all edges can be resized (egui restricts pivot edge when !movable)
                // Borderless windows can still be dragged; use Alt+drag for precise positioning
                .movable(true);

            // InjuryDoll widgets need special handling:
            // - auto_sized: resize to fit content exactly (prevents clicks in empty space)
            // - movable: allow title bar dragging; content stays non-draggable unless Alt is held
            // - resizable(false): scaling controls size instead of edge drag
            if matches!(widget_type, WidgetType::InjuryDoll) {
                window = window
                    .auto_sized()
                    .movable(true)
                    .resizable(false);
            }

            // Apply frame based on show_border setting
            if !show_border {
                // Create borderless frame
                window = window.frame(egui::Frame::none().fill(ctx.style().visuals.window_fill));
            }

            // Apply position: use override for one frame, then default
            window = if let Some(override_pos) = position_override {
                clear_position_overrides.push(name.clone());
                window.current_pos(override_pos)
            } else {
                window.default_pos(default_pos)
            };

            let window_response = window
                .show(ctx, |ui| {
                    let mut should_toggle = false;
                    let mut should_edit = false;
                    let mut should_send_to_back = false;
                    let mut should_focus_command = false;

                    // Track link interactions from this window
                    let mut link_clicked: Option<(LinkData, egui::Pos2)> = None;
                    let mut link_drag_start: Option<LinkData> = None;
                    let mut link_hovered: Option<LinkData> = None;

                    // Render based on widget type
                    match &widget_type {
                        WidgetType::Text => {
                            let response = self.render_text_window(ui, &name);
                            if let Some(link) = response.clicked_link {
                                // Get current mouse position for menu placement
                                let pos = ui.ctx().input(|i| i.pointer.interact_pos().unwrap_or_default());
                                link_clicked = Some((link, pos));
                            }
                            if let Some(link) = response.drag_started {
                                link_drag_start = Some(link);
                            }
                            if let Some(link) = response.hovered_link {
                                link_hovered = Some(link);
                            }
                        }
                        WidgetType::TabbedText => {
                            let response = self.render_tabbed_text_window(ui, &name);
                            if let Some(link) = response.clicked_link {
                                let pos = ui.ctx().input(|i| i.pointer.interact_pos().unwrap_or_default());
                                link_clicked = Some((link, pos));
                            }
                            if let Some(link) = response.drag_started {
                                link_drag_start = Some(link);
                            }
                            if let Some(link) = response.hovered_link {
                                link_hovered = Some(link);
                            }
                            // Handle tab switching
                            if let Some(tab_index) = response.tab_clicked {
                                tracing::debug!("Tab clicked in {}: switching to tab {}", name, tab_index);
                                // Update the content's active tab index
                                if let Some(window) = self.app_core.ui_state.windows.get_mut(&name) {
                                    if let WindowContent::TabbedText(content) = &mut window.content {
                                        content.active_tab_index = tab_index;
                                    }
                                }
                                // Clear unread for the newly selected tab
                                if let Some(gui_state) = self.tabbed_text_states.get_mut(&name) {
                                    gui_state.clear_unread(tab_index);
                                }
                            }
                        }
                        WidgetType::Progress => self.render_progress_window(ui, &name),
                        WidgetType::Countdown => self.render_countdown_window(ui, &name),
                        WidgetType::Compass => self.render_compass_window(ui, &name),
                        WidgetType::Indicator => self.render_indicator_window(ui, &name),
                        WidgetType::Hand => self.render_hand_window(ui, &name),
                        WidgetType::CommandInput => {
                            // Special handling for command input
                            // TextEdit has its own context menu, so we add ours to the prompt label

                            // Get CommandInput configuration from layout
                            let cmd_config = self.app_core.layout.windows.iter()
                                .find(|w| w.name() == &name)
                                .and_then(|w| {
                                    if let crate::config::WindowDef::CommandInput { data, .. } = w {
                                        Some(data.clone())
                                    } else {
                                        None
                                    }
                                });

                            let config = cmd_config.unwrap_or_default();
                            let prompt_text = config.prompt_icon.as_deref().unwrap_or(">");

                            ui.horizontal(|ui| {
                                // Apply prompt icon with optional color
                                let mut prompt_label = egui::RichText::new(prompt_text);
                                if let Some(color_hex) = &config.prompt_icon_color {
                                    if let Some(color) = widgets::parse_hex_to_color32(color_hex) {
                                        prompt_label = prompt_label.color(color);
                                    }
                                }

                                let prompt_response = ui.label(prompt_label);

                                // Add context menu to the prompt label (since TextEdit captures right-click)
                                prompt_response.context_menu(|ui| {
                                    let label = if show_title_bar {
                                        "Hide Title Bar"
                                    } else {
                                        "Show Title Bar"
                                    };
                                    if ui.button(label).clicked() {
                                        should_toggle = true;
                                        ui.close();
                                    }
                                });

                                // Create TextEdit with visual customization
                                let mut text_edit = egui::TextEdit::singleline(&mut self.command_input)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("Enter command...")
                                    .font(egui::FontId::proportional(config.text_size));

                                // Apply text color if specified
                                if let Some(color_hex) = &config.text_color {
                                    if let Some(color) = widgets::parse_hex_to_color32(color_hex) {
                                        text_edit = text_edit.text_color(color);
                                    }
                                }

                                // Apply cursor color if specified (requires modifying visuals)
                                if config.cursor_color.is_some() || config.cursor_background_color.is_some() {
                                    ui.visuals_mut().selection.bg_fill = config.cursor_background_color
                                        .as_ref()
                                        .and_then(|c| widgets::parse_hex_to_color32(c))
                                        .unwrap_or(ui.visuals().selection.bg_fill);

                                    ui.visuals_mut().selection.stroke.color = config.cursor_color
                                        .as_ref()
                                        .and_then(|c| widgets::parse_hex_to_color32(c))
                                        .unwrap_or(ui.visuals().selection.stroke.color);
                                }

                                // Wrap in frame for border/background/padding customization
                                let mut frame = egui::Frame::new()
                                    .inner_margin(config.padding);

                                if let Some(bg_color_hex) = &config.background_color {
                                    if let Some(bg_color) = widgets::parse_hex_to_color32(bg_color_hex) {
                                        frame = frame.fill(bg_color);
                                    }
                                }

                                if let Some(border_color_hex) = &config.border_color {
                                    if let Some(border_color) = widgets::parse_hex_to_color32(border_color_hex) {
                                        frame = frame.stroke(egui::Stroke::new(config.border_width, border_color));
                                    }
                                } else if config.border_width > 0.0 {
                                    // Use default border color if width specified but no color
                                    frame = frame.stroke(egui::Stroke::new(config.border_width, ui.visuals().window_stroke.color));
                                }

                                let response = frame.show(ui, |ui| {
                                    ui.add(text_edit)
                                }).inner;

                                // Surrender focus when popup menu is open so arrow keys work for menu navigation
                                if self.app_core.ui_state.has_menu() {
                                    response.surrender_focus();
                                }

                                // Auto-focus if requested (after menu close, content click, etc.)
                                // Only focus if in Normal mode (not during priority windows like WindowEditor)
                                if self.request_command_focus {
                                    if self.app_core.ui_state.input_mode == InputMode::Normal {
                                        response.request_focus();
                                    }
                                    self.request_command_focus = false;
                                }

                                // Handle Enter to send command
                                if response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    if !self.command_input.is_empty() {
                                        let cmd = std::mem::take(&mut self.command_input);
                                        self.send_command(cmd);
                                    }
                                    response.request_focus();
                                }

                                // Handle Up/Down for history
                                if response.has_focus() {
                                    if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                        self.history_up();
                                    }
                                    if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                        self.history_down();
                                    }
                                }
                            });
                        }
                        WidgetType::Room => {
                            let response = self.render_room_window(ui, &name);
                            if let Some(link) = response.clicked_link {
                                let pos = ui.ctx().input(|i| i.pointer.interact_pos().unwrap_or_default());
                                link_clicked = Some((link, pos));
                            }
                            if let Some(link) = response.drag_started {
                                link_drag_start = Some(link);
                            }
                            if let Some(link) = response.hovered_link {
                                link_hovered = Some(link);
                            }
                        }
                        WidgetType::ActiveEffects => {
                            self.render_active_effects_window(ui, &name);
                        }
                        WidgetType::InjuryDoll => {
                            self.render_injury_window(ui, &name);
                        }
                        _ => self.render_placeholder_window(ui, &widget_type),
                    }

                    // Actual used content area (prevents invisible overlays consuming clicks)
                    let content_rect = ui.min_rect();

                    // Check for left-click on non-interactive content area to focus command input
                    // Only if no link was clicked in this frame
                    // Use primary_released() with latest_pos() for more reliable detection
                    // (primary_clicked() may not fire if ScrollArea consumed the press for scrolling)
                    let primary_released_in_content = ui.ctx().input(|i| {
                        i.pointer.primary_released()
                            && i.pointer.latest_pos()
                                .map(|pos| content_rect.contains(pos))
                                .unwrap_or(false)
                    });
                    if primary_released_in_content && link_clicked.is_none() {
                        should_focus_command = true;
                    }

                    // Right-click: show context menu at mouse position
                    // Avoid grabbing primary clicks; only open when secondary click is inside used rect
                    let secondary_click_pos = ui.ctx().input(|i| {
                        if i.pointer.secondary_clicked() {
                            if let Some(pos) = i.pointer.interact_pos() {
                                if content_rect.contains(pos) {
                                    return Some(pos);
                                }
                            }
                        }
                        None
                    });

                    // Context menu state with delayed click-outside detection
                    // IMPORTANT: Use GLOBAL IDs so only ONE context menu can be open at a time
                    // This prevents multiple menus opening when clicking different windows
                    let global_menu_window_id = egui::Id::new("__global_context_menu_window__");
                    let global_menu_pos_id = egui::Id::new("__global_context_menu_pos__");
                    let global_menu_open_time_id = egui::Id::new("__global_context_menu_open_time__");

                    if let Some(click_pos) = secondary_click_pos {
                        // Open menu for THIS window, store position and open time
                        // This automatically closes any other window's context menu
                        let current_time = ui.ctx().input(|i| i.time);
                        ui.memory_mut(|mem| {
                            mem.data.insert_temp(global_menu_window_id, name.clone());
                            mem.data.insert_temp(global_menu_pos_id, click_pos);
                            mem.data.insert_temp(global_menu_open_time_id, current_time);
                        });
                    }

                    // Check if THIS window's menu is open (compare window names)
                    let is_menu_open = ui.memory(|mem| {
                        mem.data.get_temp::<String>(global_menu_window_id)
                            .map(|active_window| active_window == name)
                            .unwrap_or(false)
                    });

                    if is_menu_open {
                        let menu_pos = ui.memory(|mem| {
                            mem.data.get_temp::<egui::Pos2>(global_menu_pos_id)
                                .unwrap_or(content_rect.center())
                        });

                        // Use global ID for Area to ensure only one popup exists
                        let area_response = egui::Area::new(egui::Id::new("__global_context_menu_area__"))
                            .fixed_pos(menu_pos)
                            .order(egui::Order::Foreground)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    let label = if show_title_bar {
                                        "Hide Title Bar"
                                    } else {
                                        "Show Title Bar"
                                    };
                                    if ui.button(label).clicked() {
                                        should_toggle = true;
                                        // Close menu by removing the active window
                                        ui.memory_mut(|mem| {
                                            mem.data.remove::<String>(global_menu_window_id);
                                        });
                                    }

                                    ui.separator();

                                    if ui.button("Edit Window Settings").clicked() {
                                        should_edit = true;
                                        // Close menu
                                        ui.memory_mut(|mem| {
                                            mem.data.remove::<String>(global_menu_window_id);
                                        });
                                    }

                                    ui.separator();

                                    if ui.button("Send to Back").clicked() {
                                        should_send_to_back = true;
                                        // Close menu
                                        ui.memory_mut(|mem| {
                                            mem.data.remove::<String>(global_menu_window_id);
                                        });
                                    }
                                });
                            });

                        // Click-outside detection with 1 second delay
                        let current_time = ui.ctx().input(|i| i.time);
                        let open_time = ui.memory(|mem| {
                            mem.data.get_temp::<f64>(global_menu_open_time_id).unwrap_or(current_time)
                        });

                        // Only check for click-outside after 1 second
                        if current_time - open_time > 1.0 {
                            let menu_rect = area_response.response.rect;
                            let clicked_outside = ui.ctx().input(|i| {
                                if !i.pointer.any_click() {
                                    return false;
                                }
                                let click_pos = i.pointer.interact_pos().unwrap_or_default();
                                !menu_rect.contains(click_pos)
                            });

                            if clicked_outside {
                                ui.memory_mut(|mem| {
                                    mem.data.remove::<String>(global_menu_window_id);
                                });
                            }
                        }
                    }

                    // Block normal content-area dragging on all windows (only title bar or Alt+drag should move)
                    // Skip blocking when injury doll calibration is active so clicks can reach the image
                    if !ui.ctx().input(|i| i.modifiers.alt) {
                        let mut allow_block = true;
                        if matches!(widget_type, WidgetType::InjuryDoll) {
                            let is_calibrating = self
                                .window_editors
                                .get(&name)
                                .and_then(|ed| ed.injury_doll_editor.as_ref())
                                .map(|d| d.calibration_active)
                                .unwrap_or(false);
                            if is_calibrating {
                                allow_block = false;
                            }
                        }

                        if allow_block {
                            // Swallow drag sense on content so the window doesn't move from content drags
                            let _ = ui.interact(
                                content_rect,
                                ui.id().with("block_drag"),
                                egui::Sense::drag(),
                            );
                        }
                    }

                    // Alt+drag detection for window movement (works even with hidden title bar)
                    // IMPORTANT: Only create the interaction when Alt is held, otherwise it steals
                    // mouse events from links and other interactive elements
                    let mut window_drag_delta: Option<egui::Vec2> = None;
                    let alt_held = ui.ctx().input(|i| i.modifiers.alt);

                    if alt_held {
                        let content_drag_response = ui.interact(
                            content_rect,
                            ui.id().with("alt_drag"),
                            egui::Sense::drag(),
                        );
                        if content_drag_response.dragged() {
                            window_drag_delta = Some(content_drag_response.drag_delta());
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                        } else if content_drag_response.hovered() {
                            // Show move cursor when Alt is held over content (before drag starts)
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                        }
                    }

                    // Return tuple: (toggle_title_bar, edit_window, send_to_back, clicked_link, drag_started, hovered_link, focus_command, window_drag_delta)
                    (should_toggle, should_edit, should_send_to_back, link_clicked, link_drag_start, link_hovered, should_focus_command, window_drag_delta)
                });

            // Check if title bar toggle was requested and collect link interactions
            if let Some(inner) = window_response {
                if let Some((toggle, edit, send_to_back, clicked, drag_start, hovered, focus_command, drag_delta)) = inner.inner {
                    if toggle {
                        // Store name and window rect for anchor-aware toggle
                        title_bar_toggles.push((name.clone(), inner.response.rect));
                    }
                    if edit {
                        // Trigger edit window command
                        self.handle_action_command(&format!("__EDIT__{}", name));
                    }
                    if send_to_back {
                        // Send this window to back by moving all other windows to front
                        ctx.memory_mut(|mem| {
                            // Move all windows except this one to the top
                            // Windows are in the Middle order by default
                            for other_name in &all_window_names {
                                if other_name != &name {
                                    let other_window_id = egui::Id::new(other_name);
                                    let other_layer_id = egui::LayerId::new(egui::Order::Middle, other_window_id);
                                    mem.areas_mut().move_to_top(other_layer_id);
                                }
                            }
                        });
                    }
                    if let Some(link) = clicked {
                        clicked_links.push(link);
                    }
                    if let Some(link) = drag_start {
                        drag_started_links.push(link);
                    }
                    if let Some(link) = hovered {
                        hovered_links.push(link);
                    }
                    // Only focus command input if no window editors are open
                    // This prevents stealing focus from input boxes in the editor
                    if focus_command && self.window_editors.is_empty() {
                        self.request_command_focus = true;
                    }
                    // Apply Alt+drag window movement
                    if let Some(delta) = drag_delta {
                        let rect = inner.response.rect;
                        let new_pos = [rect.left() + delta.x, rect.top() + delta.y];
                        self.window_manager.set_position_override(&name, new_pos);
                    }
                }

                // Collect window pixel sizes for syncing to open editors
                let rect = inner.response.rect;
                window_pixel_sizes.push((name.clone(), rect.width(), rect.height()));
            }
        }

        // Apply title bar toggles after the loop (to avoid borrow issues)
        // Get parent dimensions and title bar height for anchor-aware toggle
        let parent_height = ctx.available_rect().height();
        // Title bar height is typically interact_size.y + some padding
        let title_bar_height = ctx.style().spacing.interact_size.y + 8.0;
        for (name, rect) in title_bar_toggles {
            // Pass current position and size from egui's window rect
            let current_pos = [rect.left(), rect.top()];
            let current_size = [rect.width(), rect.height()];
            self.window_manager.toggle_title_bar_with_anchor(
                &name,
                current_pos,
                current_size,
                parent_height,
                title_bar_height,
            );
        }

        // Clear position overrides that were applied this frame
        for name in clear_position_overrides {
            self.window_manager.clear_position_override(&name);
        }

        // Update hovered link state (for drag target detection)
        // Take the last one if multiple windows have hovered links
        self.hovered_link.link_data = hovered_links.pop();

        // Handle link clicks - send _menu commands
        for (link, pos) in clicked_links {
            self.handle_link_click(&link, pos);
        }

        // Handle link drag starts
        for link in drag_started_links {
            self.link_drag_state = Some(LinkDragState {
                link_data: link,
            });
        }

        // Check for drag release (Ctrl+drag ended)
        if let Some(drag_state) = &self.link_drag_state {
            let released = ctx.input(|i| i.pointer.any_released());
            if released {
                self.handle_link_drag_end(&drag_state.link_data.clone());
                self.link_drag_state = None;
            }
        }

        // Render popup menu (if visible) and handle any command from it
        // Note: __SUBMENU__ commands SHOULD be handled internally by render_popup_menu,
        // but we add a safety check here in case they leak through
        if let Some(command) = self.render_popup_menu(ctx) {
            tracing::info!("Menu command selected: {}", command);
            // Check if it's an action command (handled locally)
            if command.starts_with("action:") {
                self.handle_action_command(&command);
            } else if command.starts_with("__SUBMENU__")
                || command.starts_with("__SUBMENU_ADD__")
                || command.starts_with("__ADD__")
            {
                // These are internal menu navigation commands that should NOT be sent to server
                // If we get here, it means render_popup_menu failed to handle them internally
                tracing::warn!(
                    "Internal menu command leaked through render_popup_menu: {}. Handling in update().",
                    command
                );
                self.handle_action_command(&command);
            } else {
                // Regular command - send to server
                self.send_command(format!("{}\n", command));
            }
        }

        // Sync window pixel sizes to open editors (rows/cols)
        // This ensures that when a window is mouse-resized, the editor reflects the new size
        const CHAR_WIDTH: f32 = 8.0;
        const CHAR_HEIGHT: f32 = 18.0;
        for (window_name, pixel_width, pixel_height) in &window_pixel_sizes {
            if let Some(editor) = self.window_editors.get_mut(window_name) {
                // Convert pixel dimensions to character cells
                let new_cols = (*pixel_width / CHAR_WIDTH).round() as u16;
                let new_rows = (*pixel_height / CHAR_HEIGHT).round() as u16;

                // Only update if actually changed (to avoid marking dirty unnecessarily)
                let base = editor.modified_def.base();
                if base.cols != new_cols || base.rows != new_rows {
                    editor.modified_def.base_mut().cols = new_cols;
                    editor.modified_def.base_mut().rows = new_rows;
                }
            }
        }

        // Render window editors and handle their actions
        let mut editors_to_remove = Vec::new();

        for (window_name, editor) in &mut self.window_editors {
            use crate::frontend::gui::window_editor::EditorAction;

            match editor.render(ctx, &self.app_core) {
                EditorAction::Apply => {
                    // Apply changes to layout for preview
                    editor.apply(&mut self.app_core);
                }
                EditorAction::Cancel => {
                    // Revert to original settings
                    editor.cancel(&mut self.app_core);
                }
                EditorAction::Save => {
                    // Persist changes to memory and save config
                    editor.save(&mut self.app_core);
                }
                EditorAction::Close => {
                    // Mark for removal
                    editors_to_remove.push(window_name.clone());
                }
                EditorAction::None => {}
            }
        }

        // Remove closed editors
        for name in editors_to_remove {
            self.window_editors.remove(&name);
        }

        // Request repaint to keep polling for messages
        ctx.request_repaint();
    }
}

/// Configure egui visual style
fn configure_style(ctx: &egui::Context, theme: &crate::theme::AppTheme) {
    use egui::{FontId, TextStyle};

    let mut style = (*ctx.style()).clone();

    // Apply theme-based visuals
    let visuals = app_theme_to_visuals(theme);
    ctx.set_visuals(visuals);

    // Smaller fonts for compact UI
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(10.0)),
        (TextStyle::Body, FontId::proportional(12.0)),
        (TextStyle::Monospace, FontId::monospace(12.0)),
        (TextStyle::Button, FontId::proportional(11.0)),
        (TextStyle::Heading, FontId::proportional(14.0)),  // Used for window titles
    ].into();

    // Tighter spacing for compact look
    style.spacing.item_spacing = egui::vec2(4.0, 2.0);
    style.spacing.window_margin = egui::Margin::same(3);
    style.spacing.button_padding = egui::vec2(3.0, 1.0);

    ctx.set_style(style);
}
