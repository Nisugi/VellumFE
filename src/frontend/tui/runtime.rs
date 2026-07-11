use anyhow::Result;
use std::time::Instant;

use super::TuiFrontend;
use crate::frontend::Frontend;

/// Run the TUI frontend with the given configuration.
/// This is the main entry point for TUI mode.
///
/// `console_size_profile` is set only for launcher-spawned sessions, which
/// own their console window: the size is restored on start and saved on
/// exit, keyed by settings profile. Manual runs pass None - resizing a
/// terminal the user already owns (a tmux pane, a Windows Terminal tab)
/// would be rude.
pub fn run(
    config: crate::config::Config,
    character: Option<String>,
    direct: Option<crate::network::DirectConnectConfig>,
    setup_palette: bool,
    login_key: Option<String>,
    console_size_profile: Option<String>,
) -> Result<()> {
    if let Some(profile) = console_size_profile.as_deref() {
        restore_console_size(profile);
    }
    // Closing the console with the X button never reaches the end-of-loop
    // save; catch it and save geometry there.
    #[cfg(windows)]
    console_close::install(character.clone(), console_size_profile.clone());
    // Use tokio runtime for async network I/O
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async_run(
        config,
        character,
        direct,
        setup_palette,
        login_key,
        console_size_profile,
    ))
}

/// Saved console geometry for launcher-spawned sessions, in character cells
/// (`~/.vellum-fe/profiles/<profile>/console-size.toml`).
#[derive(serde::Serialize, serde::Deserialize)]
struct ConsoleSize {
    cols: u16,
    rows: u16,
}

fn console_size_path(profile: &str) -> Option<std::path::PathBuf> {
    crate::config::Config::profile_dir(Some(profile))
        .ok()
        .map(|dir| dir.join("console-size.toml"))
}

/// Best-effort: a freshly `start`-ed console opens at the host's default
/// size, not where the player left it. crossterm's SetSize resizes via the
/// console API or CSI 8 depending on host; hosts that support neither just
/// keep their default size.
fn restore_console_size(profile: &str) {
    let Some(path) = console_size_path(profile) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(size) = toml::from_str::<ConsoleSize>(&text) else {
        return;
    };
    if size.cols == 0 || size.rows == 0 {
        return;
    }
    if crossterm::terminal::size().ok() == Some((size.cols, size.rows)) {
        return;
    }
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::SetSize(size.cols, size.rows)
    );
}

/// Save window geometry when the console window is closed with the X
/// button. Windows delivers CTRL_CLOSE_EVENT on its own thread and kills
/// the process as soon as the handler returns (or after ~5s), so the normal
/// end-of-loop save in async_run never runs on that path.
#[cfg(windows)]
mod console_close {
    use std::sync::OnceLock;
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::System::Console::{SetConsoleCtrlHandler, CTRL_CLOSE_EVENT};

    struct SaveContext {
        character: Option<String>,
        size_profile: Option<String>,
    }

    static CONTEXT: OnceLock<SaveContext> = OnceLock::new();

    pub fn install(character: Option<String>, size_profile: Option<String>) {
        if CONTEXT
            .set(SaveContext {
                character,
                size_profile,
            })
            .is_err()
        {
            return;
        }
        unsafe {
            let _ = SetConsoleCtrlHandler(Some(on_console_event), true);
        }
    }

    unsafe extern "system" fn on_console_event(ctrl_type: u32) -> BOOL {
        if ctrl_type != CTRL_CLOSE_EVENT {
            return BOOL::from(false);
        }
        if let Some(context) = CONTEXT.get() {
            if let Some(positioner) = crate::window_position::create_positioner() {
                if let (Ok(rect), Ok(screens)) =
                    (positioner.get_position(), positioner.get_screen_bounds())
                {
                    let _ = crate::window_position::save(
                        context.character.as_deref(),
                        &crate::window_position::WindowPositionConfig {
                            window: rect,
                            monitors: screens,
                        },
                    );
                }
            }
            if let Some(profile) = context.size_profile.as_deref() {
                super::save_console_size(profile);
            }
        }
        BOOL::from(true)
    }
}

fn save_console_size(profile: &str) {
    let Ok((cols, rows)) = crossterm::terminal::size() else {
        return;
    };
    let Some(path) = console_size_path(profile) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = toml::to_string(&ConsoleSize { cols, rows }) {
        let _ = std::fs::write(path, text);
    }
}

/// Async TUI main loop with network support
async fn async_run(
    config: crate::config::Config,
    character: Option<String>,
    direct: Option<crate::network::DirectConnectConfig>,
    setup_palette: bool,
    login_key: Option<String>,
    console_size_profile: Option<String>,
) -> Result<()> {
    use crate::core::AppCore;
    use crate::network::{DirectConnection, LichConnection, ServerMessage};
    use tokio::sync::mpsc;

    // Create channels for network communication.
    // Server channel is bounded: if the UI stalls, the network read task
    // blocks on send() and TCP flow control takes over, instead of the
    // queue growing without bound.
    let (server_tx, mut server_rx) =
        mpsc::channel::<ServerMessage>(crate::network::SERVER_CHANNEL_CAPACITY);
    // Command channel stays unbounded: sends happen in the synchronous UI
    // event loop (can't await) and volume is user-typed commands only.
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

    // Start the web frontend sidecar if enabled (off by default). The
    // server runs as a tokio task; core feeds it via the attached sink,
    // and remote client commands arrive on remote_rx.
    let mut remote_rx = if app_core.config.web.enabled {
        let session_label = character
            .clone()
            .or_else(|| app_core.config.connection.character.clone())
            .unwrap_or_else(|| "default".to_string());
        let (sink, event_rx) = crate::frontend::web::start(&app_core.config.web, session_label);
        app_core.enable_remote(sink);
        Some(event_rx)
    } else {
        None
    };

    super::colors::set_global_color_mode(app_core.config.ui.color_mode);

    // Initialize palette lookup for Slot mode
    // This builds the hex→slot mapping from color_palette entries
    if app_core.config.ui.color_mode == crate::config::ColorMode::Slot {
        super::colors::init_palette_lookup(&app_core.config.colors.color_palette);
    }

    // Create TUI frontend
    let mut frontend = TuiFrontend::new()?;

    // Restore window position for this character (if saved)
    // One positioner for the whole session: restore now, then the main loop
    // saves geometry periodically. Exit-time saving alone is not enough -
    // closing the window with the X (or a crash) tears the host window down
    // before any handler can read its position.
    let positioner = crate::window_position::create_positioner();
    if let Some(positioner) = positioner.as_deref() {
        // Guard against files written by older builds that captured the
        // ConPTY pseudo-window (zero-size rects would collapse the window).
        if let Ok(Some(saved)) = crate::window_position::load(character.as_deref())
            .map(|config| config.filter(|c| c.window.is_sane()))
        {
            use crate::window_position::WindowPositionerExt;
            let rect = if positioner.is_visible(&saved.window) {
                saved.window
            } else {
                // Clamp to visible area if monitors changed
                match positioner.clamp_to_screen(&saved.window) {
                    Ok(clamped) => clamped,
                    Err(_) => saved.window,
                }
            };
            if let Err(e) = positioner.set_position(&rect) {
                tracing::debug!("Failed to restore window position: {}", e);
            }
        }
    }
    let mut last_geometry_check = Instant::now();
    let mut last_saved_window: Option<crate::window_position::WindowRect> = None;
    let mut last_saved_cells: Option<(u16, u16)> = None;

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

    // Get terminal size and initialize windows
    let (width, height) = frontend.size();
    app_core.init_windows(width, height);

    // Initial render to create widgets (needed before loading history)
    frontend.render(&mut app_core)?;

    // Load command history (must be after widgets are created)
    if let Err(e) = frontend.command_input_load_history("command_input", character.as_deref()) {
        tracing::warn!("Failed to load command history: {}", e);
    }

    // Login music plays when the game connection is established (first
    // server data), not when the client opens. The main loop arms the
    // deadline on first receive and fires it later — the player is !Send,
    // so no timer thread (an old thread::sleep here froze startup).
    let mut startup_music_pending =
        app_core.config.sound.startup_music && app_core.sound_player.is_some();
    let mut startup_music_at: Option<Instant> = None;

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
                if let Err(e) = LichConnection::start(
                    &host_clone,
                    port,
                    login_key_clone,
                    server_tx,
                    command_rx,
                    raw_logger,
                )
                .await
                {
                    tracing::error!(error = ?e, "Network connection error");
                }
            })
        }
    };

    // Track time for periodic countdown updates
    let mut last_countdown_update = std::time::Instant::now();

    // Create terminal title manager (if template is configured)
    let mut title_manager =
        super::terminal_title::TerminalTitleManager::new(app_core.config.ui.terminal_title.clone());

    // Main event loop
    while app_core.running {
        // Fire delayed startup music once its deadline passes
        if startup_music_at.is_some_and(|t| Instant::now() >= t) {
            startup_music_at = None;
            if let Some(ref player) = app_core.sound_player {
                if let Err(e) = player.play_from_sounds_dir("wizard_music", None) {
                    tracing::debug!("Startup music not available: {}", e);
                }
            }
        }

        // Persist window geometry when it changes (checked at a slow tick).
        // This is the primary save path: it survives every way a session
        // can end, including the console X button and crashes.
        if last_geometry_check.elapsed().as_secs() >= 3 {
            last_geometry_check = Instant::now();
            if let Some(positioner) = positioner.as_deref() {
                if let Ok(rect) = positioner.get_position() {
                    if rect.is_sane() && last_saved_window.as_ref() != Some(&rect) {
                        if let Ok(screens) = positioner.get_screen_bounds() {
                            let config = crate::window_position::WindowPositionConfig {
                                window: rect.clone(),
                                monitors: screens,
                            };
                            if crate::window_position::save(character.as_deref(), &config).is_ok()
                            {
                                last_saved_window = Some(rect);
                            }
                        }
                    }
                }
            }
            if let Some(profile) = console_size_profile.as_deref() {
                if let Ok(cells) = crossterm::terminal::size() {
                    if last_saved_cells != Some(cells) {
                        save_console_size(profile);
                        last_saved_cells = Some(cells);
                    }
                }
            }
        }

        // Drain the map worker + mapdb updater and tick the walk executor
        // (time-based waits like roundtime need a clock even when the game
        // is quiet), then send whatever travel queued through the same path
        // as typed commands. Without the map poll, the mapdb load event is
        // never received and .room/.go2/.mapdb are dead on the TUI.
        app_core.poll_map();
        for command in app_core.take_outbound() {
            match app_core.send_command(command) {
                Ok(out) if !out.is_empty() && !out.starts_with("action:") => {
                    app_core
                        .perf_stats
                        .record_bytes_sent((out.len() + 1) as u64);
                    let _ = command_tx.send(out);
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("travel command failed: {e}"),
            }
        }

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
                        app_core
                            .perf_stats
                            .record_bytes_sent((cmd.len() + 1) as u64);
                        let _ = command_tx.send(cmd);
                    }

                    if handled {
                        continue;
                    }
                }
                crate::frontend::FrontendEvent::Key {
                    code: _code,
                    modifiers: _modifiers,
                } => {
                    // Key events are handled in handle_event()
                    // No early intercepts - let the 3-layer routing handle everything
                }
                _ => {}
            }

            if let Some(command) = handle_event(&mut app_core, &mut frontend, event)? {
                app_core
                    .perf_stats
                    .record_bytes_sent((command.len() + 1) as u64);
                let _ = command_tx.send(command);
            }

            let duration = event_start.elapsed();
            app_core.perf_stats.record_event_process_time(duration);

            // Process pending window additions after event handling (for .testline)
            let (term_width, term_height) = frontend.size();
            app_core.process_pending_window_additions(term_width, term_height);
        }

        // Drain commands typed on remote web clients (non-blocking). Each
        // runs the exact same path as a locally submitted command (echo,
        // dot-commands, quit interception) and enters shared history so
        // desk up-arrow reaches phone-typed commands.
        if let Some(rx) = remote_rx.as_mut() {
            while let Ok(event) = rx.try_recv() {
                match event {
                    crate::core::remote::RemoteEvent::Command(text) => {
                        tracing::debug!("remote command: '{}'", text);
                        frontend.command_input_record_external("command_input", &text);
                        if let Some(cmd) = frontend.handle_command_submission(text, &mut app_core)? {
                            app_core
                                .perf_stats
                                .record_bytes_sent((cmd.len() + 1) as u64);
                            let _ = command_tx.send(cmd);
                        }
                    }
                    crate::core::remote::RemoteEvent::LinkTap {
                        client_id,
                        request_id,
                        exist_id,
                        noun,
                        text,
                        coord,
                    } => {
                        // Resolved exactly like a local click: <d>/coord
                        // links become direct commands, plain links a
                        // _menu request tagged to route back this client.
                        let link = crate::data::LinkData {
                            exist_id,
                            noun,
                            text,
                            coord,
                        };
                        if let Some(cmd) = app_core.resolve_link_activation(
                            &link,
                            crate::core::remote::MenuOrigin::Remote {
                                client_id,
                                request_id,
                            },
                        ) {
                            app_core
                                .perf_stats
                                .record_bytes_sent((cmd.len() + 1) as u64);
                            let _ = command_tx.send(cmd);
                        }
                    }
                    crate::core::remote::RemoteEvent::MacroSave {
                        group,
                        label,
                        command,
                        color,
                        confirm,
                        insert,
                        options,
                        original,
                    } => {
                        let button = crate::config::MacroButton {
                            label,
                            command: Some(command).filter(|c| !c.is_empty()),
                            color,
                            confirm,
                            insert,
                            options,
                            ..Default::default()
                        };
                        app_core.apply_macro_save(group, button, original);
                    }
                    crate::core::remote::RemoteEvent::MacroDelete { group, label } => {
                        app_core.apply_macro_delete(group, label);
                    }
                    crate::core::remote::RemoteEvent::Notice(message) => {
                        app_core.add_system_message(&message);
                    }
                    crate::core::remote::RemoteEvent::ConfigGet {
                        client_id,
                        request_id,
                        file,
                    } => {
                        app_core.handle_remote_config_get(client_id, request_id, file);
                    }
                    crate::core::remote::RemoteEvent::ConfigPut {
                        client_id,
                        request_id,
                        file,
                        content,
                    } => {
                        app_core.handle_remote_config_put(client_id, request_id, file, content);
                    }
                    crate::core::remote::RemoteEvent::HighlightsGet {
                        client_id,
                        request_id,
                        scope,
                    } => {
                        app_core.handle_remote_highlights_get(client_id, request_id, scope);
                    }
                    crate::core::remote::RemoteEvent::HighlightPut {
                        client_id,
                        request_id,
                        scope,
                        name,
                        rule,
                    } => {
                        app_core
                            .handle_remote_highlight_put(client_id, request_id, scope, name, rule);
                    }
                    crate::core::remote::RemoteEvent::ColorsGet {
                        client_id,
                        request_id,
                        scope,
                    } => {
                        app_core.handle_remote_colors_get(client_id, request_id, scope);
                    }
                    crate::core::remote::RemoteEvent::ColorsPut {
                        client_id,
                        request_id,
                        scope,
                        colors,
                    } => {
                        app_core.handle_remote_colors_put(client_id, request_id, scope, colors);
                    }
                    crate::core::remote::RemoteEvent::HighlightDelete {
                        client_id,
                        request_id,
                        scope,
                        name,
                    } => {
                        app_core
                            .handle_remote_highlight_delete(client_id, request_id, scope, name);
                    }
                    crate::core::remote::RemoteEvent::SessionConnect { .. }
                    | crate::core::remote::RemoteEvent::SessionDisconnect => {
                        // Sidecar sessions are owned by this local UI; the
                        // web client shouldn't offer these (session_control
                        // is false), but answer stray requests politely.
                        app_core.add_system_message(
                            "Session control is only available in headless mode.",
                        );
                    }
                    crate::core::remote::RemoteEvent::Macro { id } => {
                        // Resolve the id against config; the resulting
                        // command runs the same path as typed input (echo,
                        // dot-commands) but skips history — button spam
                        // shouldn't bury real typed commands.
                        let Some(command) = app_core.config.macros.resolve(&id).map(String::from)
                        else {
                            tracing::warn!("remote macro id '{}' did not resolve (stale client?)", id);
                            continue;
                        };
                        tracing::debug!("remote macro '{}': '{}'", id, command);
                        if let Some(cmd) = frontend.handle_command_submission(command, &mut app_core)? {
                            app_core
                                .perf_stats
                                .record_bytes_sent((cmd.len() + 1) as u64);
                            let _ = command_tx.send(cmd);
                        }
                    }
                }
            }
        }

        // Poll for server messages (non-blocking)
        while let Ok(msg) = server_rx.try_recv() {
            match msg {
                ServerMessage::Text(line) => {
                    // First data from the game = connection established:
                    // time the login music from here.
                    if startup_music_pending {
                        startup_music_pending = false;
                        startup_music_at = Some(
                            Instant::now()
                                + std::time::Duration::from_millis(
                                    app_core.config.sound.startup_music_delay_ms,
                                ),
                        );
                    }
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
                        if let Some((id, title)) =
                            app_core.message_processor.newly_registered_container.take()
                        {
                            tracing::info!(
                                "Container discovery: creating window for '{}' (id={})",
                                title,
                                id
                            );
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

        // Flush coalesced state deltas to web clients once per batch
        // (no-op unless [web] is enabled)
        app_core.flush_remote_state();

        // Update terminal title if configured and state changed
        if let Some(ref mut manager) = title_manager {
            if let Err(e) = manager.update(&app_core, &mut std::io::stdout()) {
                tracing::debug!("Failed to update terminal title: {}", e);
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

        // Reset specific widgets (e.g., when widget type changes)
        if !app_core.ui_state.widgets_to_reset.is_empty() {
            for name in app_core.ui_state.widgets_to_reset.drain(..) {
                frontend.widget_manager.remove_widget_from_all_caches(&name);
                tracing::debug!("Reset widget cache for '{}' (type change)", name);
            }
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

    // Final geometry save on clean exit (the loop's periodic save may be up
    // to one tick stale). Reuses the session positioner - its handle is
    // still valid here, unlike in close-event handlers.
    if let Some(positioner) = positioner.as_deref() {
        if let Ok(rect) = positioner.get_position() {
            if let Ok(screens) = positioner.get_screen_bounds() {
                let config = crate::window_position::WindowPositionConfig {
                    window: rect,
                    monitors: screens,
                };
                if let Err(e) = crate::window_position::save(character.as_deref(), &config) {
                    tracing::warn!("Failed to save window position: {}", e);
                }
            }
        }
    }

    // Remember the console size the player settled on (launcher-spawned
    // sessions only) before teardown.
    if let Some(profile) = console_size_profile.as_deref() {
        save_console_size(profile);
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
