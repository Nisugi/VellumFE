use anyhow::Result;
use std::time::Instant;

use super::TuiFrontend;
use crate::frontend::Frontend;

/// Run the TUI frontend with the given configuration.
/// This is the main entry point for TUI mode.
pub fn run(
    config: crate::config::Config,
    character: Option<String>,
    direct: Option<crate::network::DirectConnectConfig>,
    setup_palette: bool,
    login_key: Option<String>,
) -> Result<()> {
    // Use tokio runtime for async network I/O
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async_run(config, character, direct, setup_palette, login_key))
}

/// Async TUI main loop with network support
async fn async_run(
    config: crate::config::Config,
    character: Option<String>,
    direct: Option<crate::network::DirectConnectConfig>,
    setup_palette: bool,
    login_key: Option<String>,
) -> Result<()> {
    use crate::core::AppCore;
    use crate::network::{DirectConnection, LichConnection, ServerMessage};
    use tokio::sync::mpsc;

    // Create channels for network communication
    let (server_tx, mut server_rx) = mpsc::unbounded_channel::<ServerMessage>();
    let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();

    // Store connection info
    let host = config.connection.host.clone();
    let port = config.connection.port;

    // Set global color mode BEFORE creating frontend or any widgets
    // This ensures ALL color parsing respects the mode from config
    let raw_logger = match crate::network::RawLogger::new(&config) {
        Ok(logger) => logger,
        Err(e) => {
            tracing::error!("Failed to initialize raw logger: {}", e);
            None
        }
    };

    // Create core application state
    let mut app_core = AppCore::new(config)?;

    super::colors::set_global_color_mode(app_core.config.ui.color_mode);

    // Initialize palette lookup for Slot mode
    // This builds the hex→slot mapping from color_palette entries
    if app_core.config.ui.color_mode == crate::config::ColorMode::Slot {
        super::colors::init_palette_lookup(&app_core.config.colors.color_palette);
    }

    // Create TUI frontend
    let mut frontend = TuiFrontend::new()?;
    // Ensure frontend theme cache matches whatever layout/theme AppCore activated
    let initial_theme_id = app_core.config.active_theme.clone();
    let initial_theme = app_core.config.get_theme();
    frontend.update_theme_cache(initial_theme_id, initial_theme);

    // Initialize command input widget BEFORE any rendering
    // This ensures it exists when we start routing keys to it
    frontend.ensure_command_input_exists("command_input");

    // Setup palette if requested via --setup-palette flag
    if setup_palette {
        if let Err(e) = frontend.execute_setpalette(&app_core) {
            tracing::warn!("Failed to setup palette: {}", e);
        } else {
            tracing::info!("Terminal palette loaded from color_palette");
        }
    }

    // Load command history
    if let Err(e) = frontend.command_input_load_history("command_input", character.as_deref()) {
        tracing::warn!("Failed to load command history: {}", e);
    }

    // Get terminal size and initialize windows
    let (width, height) = frontend.size();
    app_core.init_windows(width, height);
    if direct.is_none() {
        app_core.seed_default_quickbars_if_empty();
        if app_core
            .ui_state
            .get_window_by_type(crate::data::window::WidgetType::Spells, None)
            .is_some()
        {
            let command = "_spell _spell_update_links\n".to_string();
            app_core.message_processor.skip_next_spells_clear();
            app_core
                .perf_stats
                .record_bytes_sent((command.len() + 1) as u64);
            let _ = command_tx.send(command);
        }
    }

    // Spawn network connection task
    let network_handle = match direct {
        Some(cfg) => tokio::spawn(async move {
            if let Err(e) = DirectConnection::start(cfg, server_tx, command_rx, raw_logger).await {
                tracing::error!(error = ?e, "Network connection error");
            }
        }),
        None => {
            let host_clone = host.clone();
            let login_key_clone = login_key.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    LichConnection::start(&host_clone, port, login_key_clone, server_tx, command_rx, raw_logger).await
                {
                    tracing::error!(error = ?e, "Network connection error");
                }
            })
        }
    };

    // Track time for periodic countdown updates
    let mut last_countdown_update = std::time::Instant::now();

    // Main event loop
    while app_core.running {
        // Poll for frontend events (keyboard, mouse, resize)
        let events = frontend.poll_events()?;
        app_core
            .perf_stats
            .record_event_queue_depth(events.len() as u64);

        // Poll TTS callback events for auto-play
        app_core.poll_tts_events();

        // Process frontend events
        for event in events {
            let event_start = Instant::now();
            // Handle events that need frontend access directly
            match &event {
                crate::frontend::FrontendEvent::Mouse(mouse_event) => {
                    // Phase 4.1: Delegate to TuiFrontend::handle_mouse_event
                    let (handled, command) = frontend.handle_mouse_event(
                        mouse_event,
                        &mut app_core,
                        crate::frontend::tui::menu_actions::handle_menu_action,
                    )?;

                    if let Some(cmd) = command {
                        app_core.perf_stats.record_bytes_sent((cmd.len() + 1) as u64);
                        let _ = command_tx.send(cmd);
                    }

                    if handled {
                        continue;
                    }
                }
                crate::frontend::FrontendEvent::Key { code: _code, modifiers: _modifiers } => {
                    // Key events are handled in handle_event()
                    // No early intercepts - let the 3-layer routing handle everything
                }
                _ => {}
            }

            if let Some(command) = handle_event(&mut app_core, &mut frontend, event)? {
                app_core.perf_stats.record_bytes_sent((command.len() + 1) as u64);
                let _ = command_tx.send(command);
            }

            let duration = event_start.elapsed();
            app_core.perf_stats.record_event_process_time(duration);

            // Process pending window additions after event handling (for .testline)
            let (term_width, term_height) = frontend.size();
            app_core.process_pending_window_additions(term_width, term_height);
        }

        // Poll for server messages (non-blocking)
        while let Ok(msg) = server_rx.try_recv() {
            match msg {
                ServerMessage::Text(line) => {
                    app_core
                        .perf_stats
                        .record_bytes_received((line.len() + 1) as u64);
                    let parse_start = Instant::now();
                    // Process incoming server data through parser
                    if let Err(e) = app_core.process_server_data(&line) {
                        tracing::error!("Error processing server data: {}", e);
                    }
                    let parse_duration = parse_start.elapsed();
                    app_core.perf_stats.record_parse(parse_duration);

                    // Adjust content-driven window sizes (e.g., Betrayer auto-resize)
                    app_core.adjust_content_driven_windows();

                    // Play queued sounds from highlight processing
                    for sound in app_core.game_state.drain_sound_queue() {
                        if let Some(ref player) = app_core.sound_player {
                            if let Err(e) = player.play_from_sounds_dir(&sound.file, sound.volume) {
                                tracing::warn!("Failed to play sound '{}': {}", sound.file, e);
                            }
                        }
                    }

                    // Container discovery: auto-create window for new containers
                    if app_core.ui_state.container_discovery_mode {
                        if let Some((_, title)) =
                            app_core.message_processor.newly_registered_container.take()
                        {
                            let (term_width, term_height) = frontend.size();
                            app_core.create_ephemeral_container_window(
                                &title,
                                term_width,
                                term_height,
                            );
                        }
                    } else {
                        // Clear any pending signal if discovery mode is off
                        app_core.message_processor.newly_registered_container = None;
                    }

                    // Process pending window additions from openDialog events
                    let (term_width, term_height) = frontend.size();
                    app_core.process_pending_window_additions(term_width, term_height);
                }
                ServerMessage::Connected => {
                    tracing::info!("Connected to game server");
                    app_core.game_state.connected = true;
                    app_core.needs_render = true;
                }
                ServerMessage::Disconnected => {
                    tracing::info!("Disconnected from game server");
                    app_core.game_state.connected = false;
                    app_core.needs_render = true;
                }
            }
        }

        // Force render every second for countdown widgets
        if last_countdown_update.elapsed().as_secs() >= 1 {
            app_core.needs_render = true;
            last_countdown_update = std::time::Instant::now();
        }

        // Sample system/process metrics (rate-limited internally)
        app_core.perf_stats.sample_sysinfo();

        // Reset widget caches if layout was reloaded
        if app_core.ui_state.needs_widget_reset {
            frontend.widget_manager.clear();
            app_core.ui_state.needs_widget_reset = false;
            tracing::debug!("Widget caches cleared after layout reload");
        }

        // Render if needed
        if app_core.needs_render {
            frontend.render(&mut app_core)?;
            app_core.needs_render = false;
        }

        // No sleep needed - event::poll() timeout already limits frame rate to ~60 FPS
    }

    // Save command history
    if let Err(e) = frontend.command_input_save_history("command_input", character.as_deref()) {
        tracing::warn!("Failed to save command history: {}", e);
    }

    // Cleanup
    frontend.cleanup()?;

    // Wait for network task to finish (or abort it)
    network_handle.abort();
    let _ = network_handle.await;

    Ok(())
}

/// Handle a frontend event
/// Returns Some(command) if a command should be sent to the server
fn handle_event(
    app_core: &mut crate::core::AppCore,
    frontend: &mut TuiFrontend,
    event: crate::frontend::FrontendEvent,
) -> Result<Option<String>> {
    use crate::frontend::FrontendEvent;

    match event {
        FrontendEvent::Key { code, modifiers } => {
            // Phase 4.2: Delegate all keyboard handling to TuiFrontend::handle_key_event()
            return frontend.handle_key_event(
                code,
                modifiers,
                app_core,
                crate::frontend::tui::menu_actions::handle_menu_action,
            );
        }
        FrontendEvent::Resize { width, height } => {
            // DISABLED: Automatic resize on terminal resize (manual .resize command only)
            tracing::info!(
                "Terminal resized to {}x{} (auto-resize disabled, use .resize command)",
                width,
                height
            );
        }
        _ => {}
    }

    Ok(None)
}
