//! The headless main loop and reconnect supervisor.
//!
//! Modeled on `frontend/tui/runtime.rs::async_run` with all rendering,
//! terminal, and geometry concerns removed, plus a session supervisor that
//! the one-shot TUI/GUI network spawn doesn't have. Command dispatch follows
//! the GUI's `dispatch_command` shape (no local echo helpers).
//!
//! Session lifecycle: the runtime starts connecting immediately when the
//! CLI provided credentials (`--direct`) or a Lich key (`--key`); otherwise
//! it idles with `session_control` advertised and waits for a web client's
//! `connect` message (the login screen). Web-initiated sessions are always
//! direct-mode.

use anyhow::Result;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::core::remote::{RemoteSessionInfo, SessionState};
use crate::network::{
    AuthFailed, DirectConnectConfig, DirectConnection, LichConnection, RawLogger, ServerMessage,
};
use crate::core::AppCore;

/// Windows are layout containers for stream routing; with no terminal we
/// still initialize them at a nominal size so highlight/stream processing
/// behaves exactly like a desktop session.
const NOMINAL_COLS: u16 = 120;
const NOMINAL_ROWS: u16 = 40;

/// Reconnect backoff schedule (capped at the last entry), ±20% jitter.
const BACKOFF: &[u64] = &[1, 2, 5, 10, 30];

/// Consecutive connection losses with zero user input in between before
/// the supervisor stops reconnecting. Guards the abandoned-phone case:
/// the game idle-kicks after ~30 minutes, and without this cap the
/// supervisor would re-login all night (battery + pointless auth churn).
const MAX_UNATTENDED_LOSSES: u32 = 2;

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

/// A session-control request from a web client, extracted from the remote
/// event drain and applied by the supervisor (which owns connection state).
enum SessionRequest {
    Connect {
        profile: Option<String>,
        account: Option<String>,
        password: Option<String>,
        character: Option<String>,
        game: Option<String>,
        save_password: bool,
        profile_name: Option<String>,
    },
    Disconnect,
    /// The user sent `quit` to the game: the server will close the
    /// connection shortly — treat that close as an intentional logout
    /// (no reconnect, back to the login screen), not a network drop.
    UserQuit,
}

/// Everything the supervisor tracks about the desired/current session.
struct Supervisor {
    /// Credentials for the current/last direct session; None = Lich mode.
    direct: Option<DirectConnectConfig>,
    login_key: Option<String>,
    /// Lich sessions are only auto-started when the CLI asked for one.
    lich_configured: bool,
    connection: Option<Connection>,
    reconnect_attempt: u32,
    reconnect_at: Option<Instant>,
    /// Set by a user-initiated disconnect: suppresses reconnection.
    user_disconnected: bool,
    /// Any command/macro/link since the current connection came up.
    saw_input_since_connect: bool,
    /// Consecutive connection losses without user input (see
    /// MAX_UNATTENDED_LOSSES).
    unattended_losses: u32,
    /// Display fields for session status pushes.
    character: Option<String>,
    game: Option<String>,
}

impl Supervisor {
    fn can_reconnect(&self) -> bool {
        // Direct mode re-authenticates for a fresh ticket; detachable Lich
        // (no key) re-attaches. A Lich --key is single-use.
        !self.user_disconnected && (self.direct.is_some() || self.login_key.is_none())
    }

    fn spawn(&mut self, app_core: &AppCore, server_tx: mpsc::Sender<ServerMessage>) {
        self.saw_input_since_connect = false;
        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();
        let raw_logger = match RawLogger::new(&app_core.config) {
            Ok(logger) => logger,
            Err(e) => {
                tracing::error!("Failed to initialize raw logger: {}", e);
                None
            }
        };
        let task = match self.direct.as_ref() {
            Some(cfg) => {
                let cfg = cfg.clone();
                tokio::spawn(async move {
                    DirectConnection::start(cfg, server_tx, command_rx, raw_logger).await
                })
            }
            None => {
                let host = app_core.config.connection.host.clone();
                let port = app_core.config.connection.port;
                let login_key = self.login_key.clone();
                tokio::spawn(async move {
                    LichConnection::start(&host, port, login_key, server_tx, command_rx, raw_logger)
                        .await
                })
            }
        };
        self.connection = Some(Connection { command_tx, task });
    }

    fn status(&self, state: SessionState) -> RemoteSessionInfo {
        RemoteSessionInfo {
            state,
            character: self.character.clone(),
            game: self.game.clone(),
            attempt: (self.reconnect_attempt > 0).then_some(self.reconnect_attempt),
            error: None,
            session_control: true,
        }
    }
}

/// Resolve a web `connect` request into direct credentials, saving the
/// profile/password when asked. Returns a user-facing error string on
/// failure (never echoes the password).
fn resolve_connect(req: &SessionRequest) -> Result<DirectConnectConfig, String> {
    let SessionRequest::Connect {
        profile,
        account,
        password,
        character,
        game,
        save_password,
        profile_name,
    } = req
    else {
        return Err("not a connect request".to_string());
    };

    let data_dir = crate::config::Config::base_dir()
        .map_err(|e| format!("No data directory available: {e}"))?;

    // Saved profile path: look up credentials by profile name.
    if let Some(name) = profile {
        let store = crate::config::profiles::LauncherStore::load()
            .map_err(|e| format!("Could not read saved profiles: {e}"))?;
        let saved = store
            .find(name)
            .ok_or_else(|| format!("Profile '{name}' not found"))?;
        let password = password
            .clone()
            .or_else(|| crate::config::profiles::load_password(&saved.account))
            .ok_or_else(|| {
                format!("No saved password for '{name}' — enter it and connect again")
            })?;
        return Ok(DirectConnectConfig {
            account: saved.account.clone(),
            password,
            character: saved.character.clone(),
            game_code: DirectConnectConfig::game_name_to_code(&saved.game).to_string(),
            data_dir,
        });
    }

    // Inline credentials path.
    let account = account.clone().ok_or("Account is required")?;
    let character = character.clone().ok_or("Character is required")?;
    let password = password
        .clone()
        .or_else(|| crate::config::profiles::load_password(&account))
        .ok_or("Password is required")?;
    let game = game.clone().unwrap_or_else(|| "prime".to_string());

    // Optionally persist as a launcher profile (shared with the desktop
    // launcher) and store the password.
    if profile_name.is_some() || *save_password {
        let mut store = crate::config::profiles::LauncherStore::load().unwrap_or_default();
        let mut saved = crate::config::profiles::LauncherProfile::new_direct();
        saved.name = profile_name.clone().unwrap_or_else(|| character.clone());
        saved.account = account.clone();
        saved.character = character.clone();
        saved.game = game.clone();
        saved.password_saved = *save_password;
        store.upsert(saved, None);
        if let Err(e) = store.save() {
            tracing::warn!("failed to save launcher.toml: {e:#}");
        }
        if *save_password {
            if let Err(e) = crate::config::profiles::save_password(&account, &password) {
                tracing::warn!("failed to save password: {e:#}");
            }
        }
    }

    Ok(DirectConnectConfig {
        account,
        password,
        character,
        game_code: DirectConnectConfig::game_name_to_code(&game).to_string(),
        data_dir,
    })
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
    app_core.set_remote_session_control(true);

    // With no local UI there is no `.webinfo` to surface the pairing token —
    // print the ready-to-open URL instead. (Unpinned instances may port-walk
    // above the base port if it's taken; the log from the server task shows
    // the actual bind.)
    match crate::config::Config::load_or_create_web_token() {
        Ok(token) => {
            let url = format!(
                "http://127.0.0.1:{}/play#token={}",
                app_core.config.web.port, token
            );
            tracing::info!("Web client URL: {url}");
            println!("Web UI: {url}");
            if app_core.config.web.bind != "127.0.0.1" {
                println!(
                    "LAN clients: same #token fragment with this machine's IP (bind = {})",
                    app_core.config.web.bind
                );
            }
        }
        Err(e) => tracing::warn!("Could not load web pairing token: {e:#}"),
    }

    app_core.init_windows(NOMINAL_COLS, NOMINAL_ROWS);

    let (server_tx, mut server_rx) =
        mpsc::channel::<ServerMessage>(crate::network::SERVER_CHANNEL_CAPACITY);

    let is_direct = direct.is_some();
    let mut supervisor = Supervisor {
        character: direct
            .as_ref()
            .map(|d| d.character.clone())
            .or_else(|| character.clone()),
        game: None,
        direct,
        lich_configured: login_key.is_some(),
        login_key,
        connection: None,
        reconnect_attempt: 0,
        reconnect_at: None,
        user_disconnected: false,
        saw_input_since_connect: false,
        unattended_losses: 0,
    };

    // Auto-connect only when the CLI asked for a session (--direct / --key);
    // otherwise idle on the login screen.
    if supervisor.direct.is_some() || supervisor.lich_configured {
        supervisor.spawn(&app_core, server_tx.clone());
        let state = if is_direct {
            SessionState::Authenticating
        } else {
            SessionState::Connecting
        };
        app_core.set_remote_session_state(supervisor.status(state));
    } else {
        app_core.set_remote_session_state(supervisor.status(SessionState::Idle));
        tracing::info!("No credentials on the command line; waiting for web login");
    }

    if !is_direct && supervisor.connection.is_some() {
        app_core.seed_default_quickbars_if_empty();
        if app_core
            .ui_state
            .get_window_by_type(crate::data::window::WidgetType::Spells, None)
            .is_some()
        {
            if let Some(conn) = supervisor.connection.as_ref() {
                app_core.message_processor.skip_next_spells_clear();
                let _ = conn.command_tx.send("_spell _spell_update_links\n".to_string());
            }
        }
    }

    // Set when the user quits: if the server hasn't closed the connection
    // by the deadline, close it ourselves (some closes linger server-side —
    // playtests saw quits that needed a follow-up command to complete).
    let mut quit_deadline: Option<Instant> = None;

    tracing::info!("Headless runtime started (web UI is the interface)");

    while app_core.running {
        let mut session_requests: Vec<SessionRequest> = Vec::new();

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
                match maybe_event {
                    None => {
                        tracing::warn!("Web server event channel closed");
                        break;
                    }
                    Some(event) => {
                        if handle_remote_event(
                            &mut app_core,
                            supervisor.connection.as_ref(),
                            event,
                            &mut session_requests,
                        ) {
                            supervisor.saw_input_since_connect = true;
                            supervisor.unattended_losses = 0;
                        }
                    }
                }
            }
            maybe_msg = server_rx.recv() => {
                if let Some(msg) = maybe_msg {
                    let newly_connected = handle_server_message(&mut app_core, msg);
                    if newly_connected {
                        supervisor.reconnect_attempt = 0;
                        supervisor.character = app_core
                            .game_state
                            .character_name
                            .clone()
                            .or(supervisor.character.take());
                        app_core.set_remote_session_state(
                            supervisor.status(SessionState::Connected),
                        );
                    }
                }
            }
            // Network task ended: session over — classify and maybe reconnect.
            result = async {
                match supervisor.connection.as_mut() {
                    Some(conn) => (&mut conn.task).await,
                    None => std::future::pending().await,
                }
            } => {
                supervisor.connection = None;
                quit_deadline = None;
                app_core.game_state.connected = false;
                // Unattended tracking: a loss with zero user input since
                // the connection came up counts toward the cap — without
                // it, an abandoned phone would re-login in a loop all
                // night as the game idle-kicks every ~30 minutes.
                if supervisor.saw_input_since_connect {
                    supervisor.unattended_losses = 0;
                } else {
                    supervisor.unattended_losses += 1;
                }
                let unattended = supervisor.unattended_losses >= MAX_UNATTENDED_LOSSES;
                let mut error_text = None;
                let stop_from_result = match result {
                    Ok(Ok(())) => {
                        app_core.add_system_message("Connection closed.");
                        !supervisor.can_reconnect()
                    }
                    Ok(Err(e)) => {
                        if e.chain().any(|c| c.is::<AuthFailed>()) {
                            app_core.add_system_message(&format!("Login failed: {e:#}"));
                            tracing::error!("Auth failure, not retrying: {e:#}");
                            error_text = Some(format!("{e:#}"));
                            true
                        } else {
                            tracing::warn!("Connection error: {e:#}");
                            error_text = Some(format!("{e:#}"));
                            !supervisor.can_reconnect()
                        }
                    }
                    Err(join_err) => {
                        tracing::error!("Network task panicked: {join_err}");
                        !supervisor.can_reconnect()
                    }
                };
                if stop_from_result || unattended {
                    supervisor.reconnect_at = None;
                    if supervisor.user_disconnected {
                        // Intentional logout (quit / disconnect button):
                        // clean return to the login screen, no error.
                        app_core.add_system_message("Logged out.");
                        app_core.set_remote_session_state(
                            supervisor.status(SessionState::Idle),
                        );
                    } else if unattended && !stop_from_result {
                        tracing::info!(
                            "No user input across {} connections; not reconnecting",
                            supervisor.unattended_losses
                        );
                        app_core.add_system_message(
                            "Session looked idle — not reconnecting. Log in from the app to continue.",
                        );
                        let mut info = supervisor.status(SessionState::Disconnected);
                        info.error = Some("Idle session ended".to_string());
                        app_core.set_remote_session_state(info);
                    } else {
                        app_core.add_system_message(
                            "Session ended. Log in again from the web UI to reconnect.",
                        );
                        let mut info = supervisor.status(SessionState::Disconnected);
                        info.error = error_text;
                        app_core.set_remote_session_state(info);
                    }
                } else {
                    let delay = backoff_delay(supervisor.reconnect_attempt);
                    supervisor.reconnect_attempt += 1;
                    app_core.add_system_message(&format!(
                        "Disconnected. Reconnecting in {}s (attempt {})...",
                        delay.as_secs().max(1),
                        supervisor.reconnect_attempt
                    ));
                    supervisor.reconnect_at = Some(Instant::now() + delay);
                    app_core.set_remote_session_state(
                        supervisor.status(SessionState::Reconnecting),
                    );
                }
            }
            // Quit grace expired: the server never closed after our quit —
            // tear the connection down ourselves and land on the login
            // screen without needing a nudge command.
            _ = async {
                match quit_deadline {
                    Some(at) => tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await,
                    None => std::future::pending().await,
                }
            } => {
                quit_deadline = None;
                if let Some(conn) = supervisor.connection.take() {
                    conn.task.abort();
                    tracing::info!("Server didn't close after quit; closing locally");
                }
                app_core.game_state.connected = false;
                app_core.add_system_message("Logged out.");
                app_core.set_remote_session_state(supervisor.status(SessionState::Idle));
            }
            // Reconnect timer fired: start a fresh attempt.
            _ = async {
                match supervisor.reconnect_at {
                    Some(at) => tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await,
                    None => std::future::pending().await,
                }
            } => {
                supervisor.reconnect_at = None;
                app_core.add_system_message(&format!(
                    "Reconnecting (attempt {})...",
                    supervisor.reconnect_attempt
                ));
                supervisor.spawn(&app_core, server_tx.clone());
                let state = if supervisor.direct.is_some() {
                    SessionState::Authenticating
                } else {
                    SessionState::Connecting
                };
                app_core.set_remote_session_state(supervisor.status(state));
            }
        }

        // Drain whatever else queued up while we were handling the wake-up.
        while let Ok(event) = remote_rx.try_recv() {
            if handle_remote_event(
                &mut app_core,
                supervisor.connection.as_ref(),
                event,
                &mut session_requests,
            ) {
                supervisor.saw_input_since_connect = true;
                supervisor.unattended_losses = 0;
            }
        }
        while let Ok(msg) = server_rx.try_recv() {
            let newly_connected = handle_server_message(&mut app_core, msg);
            if newly_connected {
                supervisor.reconnect_attempt = 0;
                supervisor.character = app_core
                    .game_state
                    .character_name
                    .clone()
                    .or(supervisor.character.take());
                app_core.set_remote_session_state(supervisor.status(SessionState::Connected));
            }
        }

        // Apply session-control requests from web clients.
        for request in session_requests {
            match request {
                SessionRequest::Disconnect => {
                    supervisor.user_disconnected = true;
                    supervisor.reconnect_at = None;
                    supervisor.reconnect_attempt = 0;
                    if let Some(conn) = supervisor.connection.take() {
                        conn.task.abort();
                        app_core.add_system_message("Disconnected by request.");
                    }
                    app_core.game_state.connected = false;
                    app_core.set_remote_session_state(supervisor.status(SessionState::Idle));
                }
                SessionRequest::UserQuit => {
                    // Don't abort yet: the quit command is in flight and
                    // the game closes the connection once it processes it.
                    // The flag makes that close land on the login screen.
                    // The deadline covers servers that linger without
                    // closing (observed in playtests): if no close arrives
                    // in time, tear the connection down ourselves.
                    supervisor.user_disconnected = true;
                    supervisor.reconnect_at = None;
                    quit_deadline = Some(Instant::now() + Duration::from_secs(8));
                }
                connect @ SessionRequest::Connect { .. } => {
                    if supervisor.connection.is_some() {
                        app_core.add_system_message(
                            "Already connected — disconnect before starting a new session.",
                        );
                        continue;
                    }
                    match resolve_connect(&connect) {
                        Ok(cfg) => {
                            supervisor.character = Some(cfg.character.clone());
                            supervisor.game = Some(cfg.game_code.clone());
                            supervisor.direct = Some(cfg);
                            supervisor.login_key = None;
                            supervisor.user_disconnected = false;
                            supervisor.reconnect_attempt = 0;
                            supervisor.reconnect_at = None;
                            supervisor.spawn(&app_core, server_tx.clone());
                            app_core.set_remote_session_state(
                                supervisor.status(SessionState::Authenticating),
                            );
                        }
                        Err(message) => {
                            app_core.add_system_message(&format!("Connect failed: {message}"));
                            let mut info = supervisor.status(SessionState::Idle);
                            info.error = Some(message);
                            app_core.set_remote_session_state(info);
                        }
                    }
                }
            }
        }

        app_core.poll_tts_events();
        // Flush coalesced state deltas to web clients once per batch.
        app_core.flush_remote_state();
    }

    if let Some(conn) = supervisor.connection.take() {
        conn.task.abort();
    }
    app_core.save_on_quit();
    tracing::info!("Headless runtime stopped");
    Ok(())
}

/// Command dispatch without a local frontend: same core path as typed input
/// (echo, dot-commands, quit interception), modeled on the GUI's
/// `dispatch_command`. `action:`/`menu:` outputs need a local UI and get a
/// notice instead. Returns true when the outbound command was `quit`, so
/// the supervisor treats the server's coming close as an intentional
/// logout instead of a drop to reconnect from.
fn dispatch_command(
    app_core: &mut AppCore,
    connection: Option<&Connection>,
    command: String,
) -> bool {
    let command = command.trim_end().to_string();
    if command.is_empty() {
        return false;
    }
    match app_core.send_command(command) {
        Ok(outbound) => {
            if outbound.is_empty() || outbound.starts_with("__") {
                return false;
            }
            if outbound.starts_with("action:") || outbound.starts_with("menu:") {
                app_core.add_system_message("That action needs the desktop client.");
                return false;
            }
            let is_quit = outbound.trim().eq_ignore_ascii_case("quit");
            match connection {
                Some(conn) => {
                    app_core
                        .perf_stats
                        .record_bytes_sent((outbound.len() + 1) as u64);
                    let _ = conn.command_tx.send(outbound);
                    is_quit
                }
                None => {
                    app_core.add_system_message("Not connected — command not sent.");
                    false
                }
            }
        }
        Err(err) => {
            app_core.add_system_message(&format!("Command error: {}", err));
            false
        }
    }
}

/// Returns true when the event was direct user input (command, macro,
/// link tap) — the supervisor uses this to tell attended sessions from
/// abandoned ones.
fn handle_remote_event(
    app_core: &mut AppCore,
    connection: Option<&Connection>,
    event: crate::core::remote::RemoteEvent,
    session_requests: &mut Vec<SessionRequest>,
) -> bool {
    use crate::core::remote::RemoteEvent;
    match event {
        RemoteEvent::Command(text) => {
            tracing::debug!("remote command: '{}'", text);
            if dispatch_command(app_core, connection, text) {
                session_requests.push(SessionRequest::UserQuit);
            }
            true
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
            true
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
            true
        }
        RemoteEvent::MacroDelete { group, label } => {
            app_core.apply_macro_delete(group, label);
            true
        }
        RemoteEvent::Notice(message) => {
            app_core.add_system_message(&message);
            false
        }
        RemoteEvent::Macro { id } => {
            match app_core.config.macros.resolve(&id).map(String::from) {
                Some(command) => {
                    tracing::debug!("remote macro '{}': '{}'", id, command);
                    if dispatch_command(app_core, connection, command) {
                        session_requests.push(SessionRequest::UserQuit);
                    }
                }
                None => {
                    tracing::warn!("remote macro id '{}' did not resolve (stale client?)", id)
                }
            }
            true
        }
        RemoteEvent::SessionConnect {
            profile,
            account,
            password,
            character,
            game,
            save_password,
            profile_name,
        } => {
            session_requests.push(SessionRequest::Connect {
                profile,
                account,
                password,
                character,
                game,
                save_password,
                profile_name,
            });
            true
        }
        RemoteEvent::SessionDisconnect => {
            session_requests.push(SessionRequest::Disconnect);
            true
        }
        RemoteEvent::ConfigGet {
            client_id,
            request_id,
            file,
        } => {
            app_core.handle_remote_config_get(client_id, request_id, file);
            true
        }
        RemoteEvent::ConfigPut {
            client_id,
            request_id,
            file,
            content,
        } => {
            app_core.handle_remote_config_put(client_id, request_id, file, content);
            true
        }
        RemoteEvent::HighlightsGet {
            client_id,
            request_id,
            scope,
        } => {
            app_core.handle_remote_highlights_get(client_id, request_id, scope);
            true
        }
        RemoteEvent::HighlightPut {
            client_id,
            request_id,
            scope,
            name,
            rule,
        } => {
            app_core.handle_remote_highlight_put(client_id, request_id, scope, name, rule);
            true
        }
        RemoteEvent::HighlightDelete {
            client_id,
            request_id,
            scope,
            name,
        } => {
            app_core.handle_remote_highlight_delete(client_id, request_id, scope, name);
            true
        }
    }
}

/// Returns true when this message flipped the session to connected.
fn handle_server_message(app_core: &mut AppCore, msg: ServerMessage) -> bool {
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
                // Web clients play sounds themselves (the Android build has
                // no native audio); local playback still runs when the
                // desktop headless build has the sound feature.
                app_core.push_remote_sound(&sound.file, sound.volume);
                if let Some(ref player) = app_core.sound_player {
                    if let Err(e) = player.play_from_sounds_dir(&sound.file, sound.volume) {
                        tracing::warn!("Failed to play sound '{}': {}", sound.file, e);
                    }
                }
            }

            // openDialog and similar can request new windows.
            app_core.process_pending_window_additions(NOMINAL_COLS, NOMINAL_ROWS);
            false
        }
        ServerMessage::Connected => {
            tracing::info!("Connected to game server");
            let newly = !app_core.game_state.connected;
            app_core.game_state.connected = true;
            newly
        }
        ServerMessage::Disconnected => {
            tracing::info!("Disconnected from game server");
            app_core.game_state.connected = false;
            false
        }
    }
}
