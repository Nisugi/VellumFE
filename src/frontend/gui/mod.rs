//! GUI Frontend - Native GUI using egui
//!
//! This module implements the GUI frontend using egui/eframe.
//! It provides a native windowed interface with moveable/resizable widgets.

mod input;
mod runtime;
mod widgets;
mod window_manager;

pub use runtime::run;

use crate::core::AppCore;
use crate::data::ui_state::InputMode;
use crate::data::widget::LinkData;
use crate::data::window::{WidgetType, WindowContent};
use crate::network::ServerMessage;
use anyhow::Result;
use eframe::egui;
use tokio::sync::mpsc;
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
}

impl EguiApp {
    /// Create a new GUI application (standalone mode, no network)
    pub fn new(app_core: AppCore) -> Self {
        let mut window_manager = WindowManager::new();
        // Initialize window manager with positions from app_core
        Self::init_window_manager(&app_core, &mut window_manager);

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
            Box::new(|cc| {
                // Main window position/size is persisted via persist_window: true
                configure_style(&cc.egui_ctx);
                Ok(Box::new(self))
            }),
        )
        .map_err(|e| anyhow::anyhow!("Failed to run GUI: {}", e))
    }

    /// Poll for server messages and process them
    fn poll_server_messages(&mut self) {
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
    fn send_command(&mut self, command: String) {
        let trimmed = command.trim();

        // Add to history (even for dot commands)
        if !trimmed.is_empty() {
            self.command_history.push(trimmed.to_string());
            self.history_index = None;
        }

        // Check for dot commands - handle locally via AppCore
        if trimmed.starts_with('.') {
            tracing::info!("Processing dot command: {}", trimmed);
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
                return widgets::render_text_window(ui, content, window_name);
            }
        }
        // Fallback for windows without content
        ui.weak("Waiting for data...");
        widgets::TextWindowResponse::default()
    }

    /// Render a tabbed text window widget
    fn render_tabbed_text_window(&self, ui: &mut egui::Ui, window_name: &str) -> widgets::TabbedTextWindowResponse {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::TabbedText(content) = &window.content {
                return widgets::render_tabbed_text_window(ui, content, window_name);
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
                let fraction = if data.max > 0 {
                    data.value as f32 / data.max as f32
                } else {
                    0.0
                };
                let text = format!("{}/{}", data.value, data.max);
                ui.add(egui::ProgressBar::new(fraction).text(text));
            }
        }
    }

    /// Render a countdown timer widget (RT/CT)
    fn render_countdown_window(&self, ui: &mut egui::Ui, window_name: &str) {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Countdown(data) = &window.content {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let remaining = (data.end_time - now).max(0);
                if remaining > 0 {
                    // Show countdown with label
                    let text = format!("{}: {}s", data.label, remaining);
                    // Use a simple fraction based on remaining time (assume max 30s for visual)
                    let fraction = (remaining as f32 / 30.0).min(1.0);
                    ui.add(egui::ProgressBar::new(fraction).text(text));
                } else {
                    ui.label(format!("{}: Ready", data.label));
                }
            }
        }
    }

    /// Render compass widget
    fn render_compass_window(&self, ui: &mut egui::Ui, window_name: &str) {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Compass(data) = &window.content {
                ui.horizontal(|ui| {
                    for dir in &["nw", "n", "ne"] {
                        let active = data.directions.contains(&dir.to_string());
                        if active {
                            ui.strong(*dir);
                        } else {
                            ui.weak(*dir);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    for dir in &["w", "out", "e"] {
                        let active = data.directions.contains(&dir.to_string());
                        if active {
                            ui.strong(*dir);
                        } else {
                            ui.weak(*dir);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    for dir in &["sw", "s", "se"] {
                        let active = data.directions.contains(&dir.to_string());
                        if active {
                            ui.strong(*dir);
                        } else {
                            ui.weak(*dir);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    for dir in &["up", "down"] {
                        let active = data.directions.contains(&dir.to_string());
                        if active {
                            ui.strong(*dir);
                        } else {
                            ui.weak(*dir);
                        }
                    }
                });
            }
        }
    }

    /// Sync room data from app_core.room_components to WindowContent::Room
    /// This is the GUI equivalent of TUI's sync_room_windows
    fn sync_room_data(&mut self) {
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
                ui.horizontal(|ui| {
                    if data.active {
                        ui.strong(&data.indicator_id);
                    } else {
                        ui.weak(&data.indicator_id);
                    }
                });
            }
        }
    }

    /// Render hand widget (left/right hand items)
    fn render_hand_window(&self, ui: &mut egui::Ui, window_name: &str) {
        if let Some(window) = self.app_core.ui_state.windows.get(window_name) {
            if let WindowContent::Hand { item, .. } = &window.content {
                match item {
                    Some(text) => ui.label(text),
                    None => ui.weak("Empty"),
                };
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

    /// Handle keybinds from egui input
    /// Returns commands to send to server (from macros)
    fn handle_keybinds(&mut self, ctx: &egui::Context) -> Vec<String> {
        let mut commands = Vec::new();

        // Get all key events from this frame
        let key_events = input::get_key_events(ctx);

        for key_event in key_events {
            // First check if this key has a binding
            if let Some(action) = self.app_core.keybind_map.get(&key_event) {
                let action = action.clone(); // Clone to avoid borrow issues

                // Skip keybind processing if:
                // 1. We're in Menu mode (popup menus handle their own input)
                // 2. We're in Search mode (search handles its own input)
                // This allows normal mode keybinds to work
                if self.app_core.ui_state.input_mode == InputMode::Menu
                    || self.app_core.ui_state.input_mode == InputMode::Search
                {
                    continue;
                }

                // Check for special keybinds that need GUI-level handling
                // (e.g., quit, which needs to close the window)
                let key_str = input::key_event_to_keybind_string(&key_event);
                if key_str == self.app_core.config.global_keybinds.quit {
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll for server messages
        self.poll_server_messages();

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

        // Render each window dynamically based on widget type
        for (name, widget_type, default_pos, default_size, show_title_bar, position_override) in windows_info {
            // Determine window title (used as ID even if title bar hidden)
            let title = if name == "main" {
                format!("{} - {}", name, status)
            } else {
                name.clone()
            };

            // Create the egui window
            // Use current_pos if position_override is set (one-frame force), otherwise default_pos
            let mut window = egui::Window::new(&title)
                .id(egui::Id::new(&name))
                .default_size(default_size)
                .default_open(true)
                .resizable(true)
                .collapsible(true)
                .title_bar(show_title_bar)
                .movable(show_title_bar); // Only draggable when title bar visible

            // Apply position: use override for one frame, then default
            window = if let Some(override_pos) = position_override {
                clear_position_overrides.push(name.clone());
                window.current_pos(override_pos)
            } else {
                window.default_pos(default_pos)
            };

            let window_response = window
                .show(ctx, |ui| {
                    // Store the content area rect for context menu detection
                    let content_rect = ui.max_rect();

                    let mut should_toggle = false;
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
                        WidgetType::Progress => self.render_progress_window(ui, &name),
                        WidgetType::Countdown => self.render_countdown_window(ui, &name),
                        WidgetType::Compass => self.render_compass_window(ui, &name),
                        WidgetType::Indicator => self.render_indicator_window(ui, &name),
                        WidgetType::Hand => self.render_hand_window(ui, &name),
                        WidgetType::CommandInput => {
                            // Special handling for command input
                            // TextEdit has its own context menu, so we add ours to the prompt label
                            ui.horizontal(|ui| {
                                let prompt_response = ui.label(">");

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

                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut self.command_input)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("Enter command..."),
                                );

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
                        }
                        WidgetType::ActiveEffects => {
                            self.render_active_effects_window(ui, &name);
                        }
                        _ => self.render_placeholder_window(ui, &widget_type),
                    }

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
                    let secondary_click_pos = ui.ctx().input(|i| {
                        if i.pointer.secondary_clicked() {
                            i.pointer.interact_pos()
                                .filter(|pos| content_rect.contains(*pos))
                        } else {
                            None
                        }
                    });

                    // Context menu state with delayed click-outside detection
                    let menu_state_id = ui.id().with("context_menu_state");
                    let menu_pos_id = ui.id().with("context_menu_pos");
                    let menu_open_time_id = ui.id().with("context_menu_open_time");

                    if let Some(click_pos) = secondary_click_pos {
                        // Open menu, store position and open time
                        let current_time = ui.ctx().input(|i| i.time);
                        ui.memory_mut(|mem| {
                            mem.data.insert_temp::<bool>(menu_state_id, true);
                            mem.data.insert_temp(menu_pos_id, click_pos);
                            mem.data.insert_temp(menu_open_time_id, current_time);
                        });
                    }

                    // Check if menu is open
                    let is_menu_open = ui.memory(|mem| {
                        mem.data.get_temp::<bool>(menu_state_id).unwrap_or(false)
                    });

                    if is_menu_open {
                        let menu_pos = ui.memory(|mem| {
                            mem.data.get_temp::<egui::Pos2>(menu_pos_id)
                                .unwrap_or(content_rect.center())
                        });

                        let area_response = egui::Area::new(menu_state_id)
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
                                        // Close menu
                                        ui.memory_mut(|mem| {
                                            mem.data.insert_temp::<bool>(menu_state_id, false);
                                        });
                                    }
                                });
                            });

                        // Click-outside detection with 1 second delay
                        let current_time = ui.ctx().input(|i| i.time);
                        let open_time = ui.memory(|mem| {
                            mem.data.get_temp::<f64>(menu_open_time_id).unwrap_or(current_time)
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
                                    mem.data.insert_temp::<bool>(menu_state_id, false);
                                });
                            }
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

                    // Return tuple: (toggle_title_bar, clicked_link, drag_started, hovered_link, focus_command, window_drag_delta)
                    (should_toggle, link_clicked, link_drag_start, link_hovered, should_focus_command, window_drag_delta)
                });

            // Check if title bar toggle was requested and collect link interactions
            if let Some(inner) = window_response {
                if let Some((toggle, clicked, drag_start, hovered, focus_command, drag_delta)) = inner.inner {
                    if toggle {
                        // Store name and window rect for anchor-aware toggle
                        title_bar_toggles.push((name.clone(), inner.response.rect));
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
                    if focus_command {
                        self.request_command_focus = true;
                    }
                    // Apply Alt+drag window movement
                    if let Some(delta) = drag_delta {
                        let rect = inner.response.rect;
                        let new_pos = [rect.left() + delta.x, rect.top() + delta.y];
                        self.window_manager.set_position_override(&name, new_pos);
                    }
                }
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

        // Request repaint to keep polling for messages
        ctx.request_repaint();
    }
}

/// Configure egui visual style
fn configure_style(ctx: &egui::Context) {
    use egui::{FontId, TextStyle};

    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    // Make window title bars more compact
    visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(60));

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
