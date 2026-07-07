//! The headless main loop and reconnect supervisor.
//!
//! Modeled on `frontend/tui/runtime.rs::async_run` with all rendering,
//! terminal, and geometry concerns removed, plus a session supervisor that
//! the one-shot TUI/GUI network spawn doesn't have. Command dispatch follows
//! the GUI's `dispatch_command` shape (no local echo helpers).

use anyhow::Result;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::core::AppCore;
use crate::network::{
    AuthFailed, DirectConnectConfig, DirectConnection, LichConnection, RawLogger, ServerMessage,
};

/// Windows are layout containers for stream routing; with no terminal we
/// still initialize them at a nominal size so highlight/stream processing
/// behaves exactly like a desktop session.
const NOMINAL_COLS: u16 = 120;
const NOMINAL_ROWS: u16 = 40;

/// Reconnect backoff schedule (capped at the last entry), ±20% jitter.
const BACKOFF: &[u64] = &[1, 2, 5, 10, 30];

fn backoff_delay(attempt: u32) -> Duration {
    let base = BACKOFF[(attempt as usize).min(BACKOFF.len() - 1)];
    // ±20% jitter from OS randomness (rand isn't a dependency; getrandom is).
    let mut byte = [0u8; 1];
    let _ = getrandom::fill(&mut byte);
    let jitter = 0.8 + (byte[0] as f64 / 255.0) * 0.4;
    Duration::from_millis((base as f64 * 1000.0 * jitter) as u64)
}

/// One live connection: a fresh command channel and the running network task.
struct Connection {
    command_tx: mpsc::UnboundedSender<String>,
    task: tokio::task::JoinHandle<Result<()>>,
}

fn spawn_connection(
    app_core: &AppCore,
    direct: Option<&DirectConnectConfig>,
    login_key: Option<&str>,
    server_tx: mpsc::Sender<ServerMessage>,
) -> Connection {
    let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();
    let raw_logger = match RawLogger::new(&app_core.config) {
        Ok(logger) => logger,
        Err(e) => {
            tracing::error!("Failed to initialize raw logger: {}", e);
            None
        }
    };
    let task = match direct {
        Some(cfg) => {
            let cfg = cfg.clone();
            tokio::spawn(
                async move { DirectConnection::start(cfg, server_tx, command_rx, raw_logger).await },
            )
        }
        None => {
            let host = app_core.config.connection.host.clone();
            let port = app_core.config.connection.port;
            let login_key = login_key.map(str::to_string);
            tokio::spawn(async move {
                LichConnection::start(&host, port, login_key, server_tx, command_rx, raw_logger)
                    .await
            })
        }
    };
    Connection { command_tx, task }
}

pub async fn async_run(
    mut config: crate::config::Config,
    character: Option<String>,
    direct: Option<DirectConnectConfig>,
    login_key: Option<String>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    // The web frontend is the only interface — it is not optional here.
    config.web.enabled = true;

    let mut app_core = AppCore::new(config)?;

    let session_label = character
        .clone()
        .or_else(|| app_core.config.connection.character.clone())
        .unwrap_or_else(|| "default".to_string());
    let (sink, mut remote_rx) = crate::frontend::web::start(&app_core.config.web, session_label);
    app_core.enable_remote(sink);

    app_core.init_windows(NOMINAL_COLS, NOMINAL_ROWS);

    let (server_tx, mut server_rx) =
        mpsc::channel::<ServerMessage>(crate::network::SERVER_CHANNEL_CAPACITY);

    // Lich-without-key sessions request the spells window sync like the TUI
    // does; direct sessions get it from the game's own login stream.
    let is_direct = direct.is_some();
    // Reconnecting is possible when we can re-authenticate (direct mode gets
    // a fresh eAccess ticket per attempt) or re-attach (detachable Lich).
    // A Lich `--key` is single-use, so those sessions end on disconnect.
    let can_reconnect = is_direct || login_key.is_none();

    let mut connection = Some(spawn_connection(
        &app_core,
        direct.as_ref(),
        login_key.as_deref(),
        server_tx.clone(),
    ));

    if !is_direct {
        app_core.seed_default_quickbars_if_empty();
        if app_core
            .ui_state
            .get_window_by_type(crate::data::window::WidgetType::Spells, None)
            .is_some()
        {
            if let Some(conn) = connection.as_ref() {
                app_core.message_processor.skip_next_spells_clear();
                let _ = conn.command_tx.send("_spell _spell_update_links\n".to_string());
            }
        }
    }

    let mut reconnect_attempt: u32 = 0;
    let mut reconnect_at: Option<Instant> = None;

    tracing::info!("Headless runtime started (web UI is the interface)");

    while app_core.running {
        // Wait for any wake-up source, then drain everything non-blocking
        // below so remote state flushes once per batch.
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("Shutdown requested");
                    break;
                }
            }
            maybe_event = remote_rx.recv() => {
                if maybe_event.is_none() {
                    tracing::warn!("Web server event channel closed");
                    break;
                }
                // Handled in the drain below (recv consumed one; re-inject
                // by handling it now, then draining the rest).
                if let Some(event) = maybe_event {
                    handle_remote_event(&mut app_core, connection.as_ref(), event);
                }
            }
            maybe_msg = server_rx.recv() => {
                if let Some(msg) = maybe_msg {
                    handle_server_message(&mut app_core, msg);
                }
            }
            // Network task ended: session over — classify and maybe reconnect.
            result = async {
                match connection.as_mut() {
                    Some(conn) => (&mut conn.task).await,
                    None => std::future::pending().await,
                }
            } => {
                connection = None;
                app_core.game_state.connected = false;
                let stop = match result {
                    Ok(Ok(())) => {
                        app_core.add_system_message("Connection closed.");
                        !can_reconnect
                    }
                    Ok(Err(e)) => {
                        let auth_failure = e.chain().any(|c| c.is::<AuthFailed>());
                        if auth_failure {
                            app_core.add_system_message(&format!("Login failed: {e:#}"));
                            tracing::error!("Auth failure, not retrying: {e:#}");
                            true
                        } else {
                            tracing::warn!("Connection error: {e:#}");
                            !can_reconnect
                        }
                    }
                    Err(join_err) => {
                        tracing::error!("Network task panicked: {join_err}");
                        !can_reconnect
                    }
                };
                if stop {
                    app_core.add_system_message(
                        "Session ended. Restart the client to reconnect.",
                    );
                    reconnect_at = None;
                } else {
                    let delay = backoff_delay(reconnect_attempt);
                    reconnect_attempt += 1;
                    app_core.add_system_message(&format!(
                        "Disconnected. Reconnecting in {}s (attempt {})...",
                        delay.as_secs().max(1),
                        reconnect_attempt
                    ));
                    reconnect_at = Some(Instant::now() + delay);
                }
            }
            // Reconnect timer fired: start a fresh attempt.
            _ = async {
                match reconnect_at {
                    Some(at) => tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await,
                    None => std::future::pending().await,
                }
            } => {
                reconnect_at = None;
                app_core.add_system_message(&format!(
                    "Reconnecting (attempt {})...",
                    reconnect_attempt
                ));
                connection = Some(spawn_connection(
                    &app_core,
                    direct.as_ref(),
                    login_key.as_deref(),
                    server_tx.clone(),
                ));
            }
        }

        // Drain whatever else queued up while we were handling the wake-up.
        while let Ok(event) = remote_rx.try_recv() {
            handle_remote_event(&mut app_core, connection.as_ref(), event);
        }
        while let Ok(msg) = server_rx.try_recv() {
            handle_server_message(&mut app_core, msg);
        }
        // A successful stretch of connected time resets the backoff ladder.
        if app_core.game_state.connected && reconnect_attempt > 0 && reconnect_at.is_none() {
            reconnect_attempt = 0;
        }

        app_core.poll_tts_events();
        // Flush coalesced state deltas to web clients once per batch.
        app_core.flush_remote_state();
    }

    if let Some(conn) = connection.take() {
        conn.task.abort();
    }
    app_core.save_on_quit();
    tracing::info!("Headless runtime stopped");
    Ok(())
}

/// Command dispatch without a local frontend: same core path as typed input
/// (echo, dot-commands, quit interception), modeled on the GUI's
/// `dispatch_command`. `action:`/`menu:` outputs need a local UI and get a
/// notice instead.
fn dispatch_command(app_core: &mut AppCore, connection: Option<&Connection>, command: String) {
    let command = command.trim_end().to_string();
    if command.is_empty() {
        return;
    }
    match app_core.send_command(command) {
        Ok(outbound) => {
            if outbound.is_empty() || outbound.starts_with("__") {
                return;
            }
            if outbound.starts_with("action:") || outbound.starts_with("menu:") {
                app_core.add_system_message("That action needs the desktop client.");
                return;
            }
            match connection {
                Some(conn) => {
                    app_core
                        .perf_stats
                        .record_bytes_sent((outbound.len() + 1) as u64);
                    let _ = conn.command_tx.send(outbound);
                }
                None => {
                    app_core.add_system_message("Not connected — command not sent.");
                }
            }
        }
        Err(err) => {
            app_core.add_system_message(&format!("Command error: {}", err));
        }
    }
}

fn handle_remote_event(
    app_core: &mut AppCore,
    connection: Option<&Connection>,
    event: crate::core::remote::RemoteEvent,
) {
    use crate::core::remote::RemoteEvent;
    match event {
        RemoteEvent::Command(text) => {
            tracing::debug!("remote command: '{}'", text);
            dispatch_command(app_core, connection, text);
        }
        RemoteEvent::LinkTap {
            client_id,
            request_id,
            exist_id,
            noun,
            text,
            coord,
        } => {
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
                if let Some(conn) = connection {
                    app_core.perf_stats.record_bytes_sent((cmd.len() + 1) as u64);
                    let _ = conn.command_tx.send(cmd);
                }
            }
        }
        RemoteEvent::MacroSave {
            group,
            label,
            command,
            color,
            confirm,
            options,
            original,
        } => {
            let button = crate::config::MacroButton {
                label,
                command: Some(command).filter(|c| !c.is_empty()),
                color,
                confirm,
                options,
                ..Default::default()
            };
            app_core.apply_macro_save(group, button, original);
        }
        RemoteEvent::MacroDelete { group, label } => {
            app_core.apply_macro_delete(group, label);
        }
        RemoteEvent::Notice(message) => {
            app_core.add_system_message(&message);
        }
        RemoteEvent::Macro { id } => {
            match app_core.config.macros.resolve(&id).map(String::from) {
                Some(command) => {
                    tracing::debug!("remote macro '{}': '{}'", id, command);
                    dispatch_command(app_core, connection, command);
                }
                None => {
                    tracing::warn!("remote macro id '{}' did not resolve (stale client?)", id)
                }
            }
        }
    }
}

fn handle_server_message(app_core: &mut AppCore, msg: ServerMessage) {
    match msg {
        ServerMessage::Text(line) => {
            app_core
                .perf_stats
                .record_bytes_received((line.len() + 1) as u64);
            let parse_start = Instant::now();
            if let Err(e) = app_core.process_server_data(&line) {
                tracing::error!("Error processing server data: {}", e);
            }
            app_core.perf_stats.record_parse(parse_start.elapsed());

            // Content-driven sizing still runs: it feeds stream routing
            // decisions, not just TUI pane geometry.
            app_core.adjust_content_driven_windows();

            for sound in app_core.game_state.drain_sound_queue() {
                if let Some(ref player) = app_core.sound_player {
                    if let Err(e) = player.play_from_sounds_dir(&sound.file, sound.volume) {
                        tracing::warn!("Failed to play sound '{}': {}", sound.file, e);
                    }
                }
            }

            // openDialog and similar can request new windows.
            app_core.process_pending_window_additions(NOMINAL_COLS, NOMINAL_ROWS);
        }
        ServerMessage::Connected => {
            tracing::info!("Connected to game server");
            app_core.game_state.connected = true;
        }
        ServerMessage::Disconnected => {
            tracing::info!("Disconnected from game server");
            app_core.game_state.connected = false;
        }
    }
}
