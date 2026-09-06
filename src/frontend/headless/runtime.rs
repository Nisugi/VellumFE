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
//! `connect` message (the login screen). Web-initiated sessions are either
//! direct-mode or a Lich detachable-client attach (host+port).

use anyhow::Result;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::core::remote::{RemoteLaunchEndpoint, RemoteSessionInfo, SessionState};
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

/// Consecutive connection losses with zero user input in between before
/// the supervisor stops reconnecting. Guards the abandoned-phone case:
/// the game idle-kicks after ~30 minutes, and without this cap the
/// supervisor would re-login all night (battery + pointless auth churn).
const MAX_UNATTENDED_LOSSES: u32 = 2;

/// How often the connection bootstrap watchdog evaluates progress. This is a
/// persistent interval: ordinary incoming traffic must not postpone it.
const CONNECTION_WATCHDOG_INTERVAL: Duration = Duration::from_secs(5);

/// Maximum time allowed for a silent connection or an unanswered Lich
/// identity probe before the connection is recycled or failed closed.
const CONNECTION_STALL_TIMEOUT: Duration = Duration::from_secs(45);

/// Build a loopback browser URL only after the web sidecar has published the
/// port it actually bound. An unpinned sidecar may walk above its configured
/// base port, so formatting the configured value is not authoritative.
fn local_web_client_url(endpoint: &RemoteLaunchEndpoint, route: &str) -> String {
    endpoint.browser_url(route)
}

/// Open a launcher-requested local client without surfacing its authenticated
/// URL in errors or making browser availability part of session correctness.
fn open_local_web_client_with<F>(
    endpoint: &RemoteLaunchEndpoint,
    client: crate::config::profiles::LaunchWebClient,
    opener: F,
) -> bool
where
    F: FnOnce(&str) -> Result<()>,
{
    let url = local_web_client_url(endpoint, client.route());
    opener(&url).is_ok()
}

fn lich_session_configured(login_key: Option<&str>, launch: &super::HeadlessLaunchOptions) -> bool {
    login_key.is_some() || launch.auto_connect_lich
}

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
    /// Server messages belong to this connection generation. Keeping the
    /// receiver here (rather than on the runtime) makes it impossible for a
    /// queued message from an aborted socket to be consumed as part of the
    /// next connection.
    server_rx: mpsc::Receiver<ServerMessage>,
    task: tokio::task::JoinHandle<Result<()>>,
}

enum ConnectionEvent {
    Message(ServerMessage),
    Ended(Result<Result<()>, tokio::task::JoinError>),
}

impl Connection {
    async fn next_event(&mut self) -> ConnectionEvent {
        tokio::select! {
            message = self.server_rx.recv() => match message {
                Some(message) => ConnectionEvent::Message(message),
                // All senders are gone. The network task is ending too; wait
                // for its authoritative result instead of synthesizing one.
                None => ConnectionEvent::Ended((&mut self.task).await),
            },
            result = &mut self.task => ConnectionEvent::Ended(result),
        }
    }
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
        lich_host: Option<String>,
        lich_port: Option<u16>,
        custom_launch: Option<String>,
    },
    Disconnect,
    /// Authenticated launcher request to terminate this process while idle.
    Stop,
    /// Quit the game normally and stop this runtime only after an
    /// authoritative game/transport disconnect.
    ExitLogout,
    /// The user sent `quit` to the game: the server will close the
    /// connection shortly — treat that close as an intentional logout
    /// (no reconnect, back to the login screen), not a network drop.
    UserQuit,
    /// The user ran `.reconnect`: re-establish the session using the stored
    /// credentials, clearing any intentional-disconnect / backoff state.
    Reconnect,
    /// The user ran `.launch <character>`: run the SSH-launcher flow to
    /// cold-start a headless Lich on the home PC, then attach to the resulting
    /// detachable-client target.
    Launch(String),
}

fn session_requests_block_commands(requests: &[SessionRequest]) -> bool {
    requests.iter().any(|request| {
        matches!(
            request,
            SessionRequest::Disconnect | SessionRequest::Stop | SessionRequest::ExitLogout
        )
    })
}

/// State for Despana's explicit Exit & Log Out operation. Closing a browser
/// tab only detaches the presentation and never changes the game session.
struct SessionExitLifecycle {
    exit_requested: bool,
    quit_sent: bool,
}

impl SessionExitLifecycle {
    fn new() -> Self {
        Self {
            exit_requested: false,
            quit_sent: false,
        }
    }

    fn request_exit(&mut self) {
        self.exit_requested = true;
    }
}

/// A web-requested Lich attach target (detachable-client mode).
#[derive(Clone)]
struct LichTarget {
    host: String,
    port: u16,
}

/// Everything the supervisor tracks about the desired/current session.
struct Supervisor {
    /// Credentials for the current/last direct session; None = Lich mode.
    direct: Option<DirectConnectConfig>,
    login_key: Option<String>,
    /// Lich sessions are only auto-started when the CLI asked for one.
    lich_configured: bool,
    /// Web-supplied Lich target; None = CLI-configured host/port.
    lich_target: Option<LichTarget>,
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
    /// When the current connection attempt/session started (stall watchdog).
    phase_started: Option<Instant>,
    /// Whether any game text has arrived on the current connection — a
    /// connected-but-silent session is treated as stalled.
    first_text_seen: bool,
    /// The current connection generation delivered an authoritative game
    /// disconnect, even if its network task has not returned yet.
    game_disconnect_seen: bool,
    /// Display fields for session status pushes.
    character: Option<String>,
    /// Character requested by the profile for the current Lich attach. The
    /// game feed's `<app char>` value must confirm this before commands are
    /// allowed onto the socket. Direct sessions authenticate the character
    /// themselves and do not need this extra guard.
    expected_character: Option<String>,
    identity_confirmed: bool,
    /// Exactly one character-sheet query may bypass the identity gate for a
    /// configured Lich attach. No other command is allowed before matching.
    identity_probe_sent: bool,
    post_connect_commands_sent: bool,
    /// A terminal bootstrap failure carried through network-task teardown so
    /// the public lifecycle becomes Disconnected instead of reconnecting.
    terminal_connection_error: Option<String>,
    game: Option<String>,
}

impl Supervisor {
    fn can_reconnect(&self) -> bool {
        // Direct mode re-authenticates for a fresh ticket; detachable Lich
        // (no key) re-attaches. A Lich --key is single-use.
        !self.user_disconnected && self.can_reconnect_after_clear()
    }

    /// Whether the session is re-establishable ignoring the
    /// intentional-disconnect flag — used by an explicit `.reconnect`, which
    /// clears that flag on purpose. A single-use Lich `--key` still can't.
    fn can_reconnect_after_clear(&self) -> bool {
        self.direct.is_some() || self.login_key.is_none()
    }

    fn spawn(&mut self, app_core: &mut AppCore) {
        self.saw_input_since_connect = false;
        self.phase_started = Some(Instant::now());
        self.first_text_seen = false;
        self.game_disconnect_seen = false;
        self.post_connect_commands_sent = false;
        self.identity_probe_sent = false;
        self.terminal_connection_error = None;
        self.identity_confirmed = self.direct.is_some() || self.expected_character.is_none();
        // A WebUI bridge is scoped to the old Lich session too. Drop it and
        // any unconsumed handshake before attaching a new game transport so
        // browser component events cannot cross session generations.
        app_core.stop_webui();
        drop(app_core.take_webui_handshake());
        reset_connection_observations(app_core);
        if self.direct.is_none() {
            // Never let the previous session's observed identity satisfy the
            // next Lich attach. Only fresh feed data may confirm it.
            app_core.game_state.character_name = None;
        } else if let Some(character) = self.direct.as_ref().map(|cfg| cfg.character.clone()) {
            // Direct authentication selected this character itself, so the
            // configured name is already trustworthy while its feed starts.
            app_core.game_state.character_name = Some(character);
        }
        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();
        let (server_tx, server_rx) =
            mpsc::channel::<ServerMessage>(crate::network::SERVER_CHANNEL_CAPACITY);
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
                let (host, port) = match self.lich_target.as_ref() {
                    Some(t) => (t.host.clone(), t.port),
                    None => (
                        app_core.config.connection.host.clone(),
                        app_core.config.connection.port,
                    ),
                };
                let login_key = self.login_key.clone();
                tokio::spawn(async move {
                    LichConnection::start(&host, port, login_key, server_tx, command_rx, raw_logger)
                        .await
                })
            }
        };
        self.connection = Some(Connection {
            command_tx,
            server_rx,
            task,
        });
    }

    /// A socket is not command-capable until its requested Lich character has
    /// been confirmed by the authoritative game feed.
    fn command_connection(&self) -> Option<&Connection> {
        self.identity_confirmed
            .then_some(self.connection.as_ref())
            .flatten()
    }

    fn status(&self, state: SessionState) -> RemoteSessionInfo {
        RemoteSessionInfo {
            state,
            character: self.character.clone(),
            game: self.game.clone(),
            attempt: (self.reconnect_attempt > 0).then_some(self.reconnect_attempt),
            error: None,
            session_control: true,
            // Set from the live AppCore flag by flush_state's session overlay;
            // this constructor doesn't know the connection mode, so default
            // false and let the sink overlay the real value.
            webui_available: false,
        }
    }
}

/// Clear state that is only meaningful for the transport which observed it.
/// Persistent character metadata remains intact, but no buffered identity or
/// prior room may suppress the fresh attach's verification/bootstrap.
fn reset_connection_observations(app_core: &mut AppCore) {
    app_core
        .game_state
        .character
        .clear_connection_observations();
    app_core.message_processor.discard_pending_character_state();
    app_core.nav_room_id = None;
    app_core.lich_room_id = None;
    app_core.room_subtitle = None;
}

/// Send the sole command allowed before a configured Lich character is
/// verified. Current Lich releases do not replay `<app char=...>` to a
/// detachable client that attaches after login, but `_info character` returns
/// the same identity in its `Name:` header.
fn probe_lich_identity_if_needed(supervisor: &mut Supervisor) {
    if supervisor.direct.is_some()
        || supervisor.expected_character.is_none()
        || supervisor.identity_confirmed
        || supervisor.identity_probe_sent
    {
        return;
    }
    let Some(connection) = supervisor.connection.as_ref() else {
        return;
    };
    if connection
        .command_tx
        .send("_info character".to_string())
        .is_ok()
    {
        supervisor.identity_probe_sent = true;
        // The socket may have spent most of the startup allowance connecting.
        // Give the actual identity response its own complete window.
        supervisor.phase_started = Some(Instant::now());
    }
}

/// Keep the startup watchdog active until a configured Lich attach has both
/// received data and proven it is the requested character.
fn connection_bootstrap_pending(supervisor: &Supervisor) -> bool {
    supervisor.connection.is_some()
        && (!supervisor.first_text_seen
            || (supervisor.expected_character.is_some() && !supervisor.identity_confirmed))
}

fn connection_bootstrap_stalled(supervisor: &Supervisor) -> bool {
    connection_bootstrap_pending(supervisor)
        && supervisor
            .phase_started
            .is_some_and(|at| at.elapsed() > CONNECTION_STALL_TIMEOUT)
}

fn live_room_known(app_core: &AppCore) -> bool {
    [
        app_core.nav_room_id.as_deref(),
        app_core.lich_room_id.as_deref(),
        app_core.room_subtitle.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| !value.trim().is_empty())
}

/// Stop an inactive or not-yet-verified runtime without ever sending an
/// in-game command. A verified connected character must use Exit & Log Out.
fn stop_inactive_session(app_core: &mut AppCore, supervisor: &mut Supervisor) -> bool {
    if app_core.game_state.connected && supervisor.identity_confirmed {
        app_core.add_system_message("Stop rejected: use Exit & Log Out for an active session.");
        return false;
    }

    supervisor.user_disconnected = true;
    supervisor.reconnect_at = None;
    supervisor.reconnect_attempt = 0;
    if let Some(connection) = supervisor.connection.take() {
        connection.task.abort();
    }
    app_core.game_state.connected = false;
    app_core.add_system_message("Stopping session by launcher request.");
    app_core.running = false;
    true
}

/// Advance an orderly Exit & Log Out request without introducing a timeout
/// that could falsely report success. The child remains alive after sending
/// `quit`; only an authoritative `ServerMessage::Disconnected` or network-task
/// completion ends it.
fn progress_exit_logout(
    app_core: &mut AppCore,
    supervisor: &mut Supervisor,
    lifecycle: &mut SessionExitLifecycle,
) {
    if !lifecycle.exit_requested || lifecycle.quit_sent {
        return;
    }

    supervisor.user_disconnected = true;
    supervisor.reconnect_at = None;
    supervisor.reconnect_attempt = 0;

    if supervisor.game_disconnect_seen {
        finish_exit_on_authoritative_disconnect(app_core, supervisor, lifecycle);
        return;
    }

    let Some(connection) = supervisor.connection.as_ref() else {
        // There is no game transport to wait for; its absence is already the
        // authoritative disconnected state.
        app_core.running = false;
        return;
    };

    // A Lich socket may be connected before its configured character is
    // identified. Never send `quit` onto an unverified session; the intent is
    // retained and progresses immediately after the matching <app char>.
    if !app_core.game_state.connected || !supervisor.identity_confirmed {
        return;
    }

    app_core
        .perf_stats
        .record_bytes_sent(("quit".len() + 1) as u64);
    if connection.command_tx.send("quit".to_string()).is_ok() {
        lifecycle.quit_sent = true;
        app_core.add_system_message("Exit requested; waiting for the game to disconnect.");
    }
}

fn finish_exit_on_authoritative_disconnect(
    app_core: &mut AppCore,
    supervisor: &Supervisor,
    lifecycle: &SessionExitLifecycle,
) -> bool {
    if !lifecycle.exit_requested {
        return false;
    }
    app_core.add_system_message("Logged out.");
    app_core.set_remote_session_state(supervisor.status(SessionState::Idle));
    app_core.running = false;
    true
}

#[derive(Debug, PartialEq, Eq)]
enum IdentityCheck {
    Pending,
    Matched { newly_confirmed: bool },
    Mismatch { expected: String, actual: String },
}

/// Return the configured/observed pair only when both names are meaningful
/// and differ. GemStone character names are case-insensitive.
fn character_identity_mismatch(
    expected: Option<&str>,
    actual: Option<&str>,
) -> Option<(String, String)> {
    let expected = expected.map(str::trim).filter(|name| !name.is_empty())?;
    let actual = actual.map(str::trim).filter(|name| !name.is_empty())?;
    (!expected.eq_ignore_ascii_case(actual)).then(|| (expected.to_string(), actual.to_string()))
}

fn check_character_identity(supervisor: &mut Supervisor, app_core: &mut AppCore) -> IdentityCheck {
    // Direct authentication already selected the character. An unnamed Lich
    // target has no profile identity to verify.
    if supervisor.direct.is_some() || supervisor.expected_character.is_none() {
        if let Some(actual) = app_core
            .game_state
            .character_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            supervisor.character = Some(actual.to_string());
        }
        let newly_confirmed = !supervisor.identity_confirmed;
        supervisor.identity_confirmed = true;
        return IdentityCheck::Matched { newly_confirmed };
    }

    let actual = app_core
        .game_state
        .character_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| {
            app_core
                .game_state
                .character
                .observed_name()
                .map(str::to_string)
        });
    let Some(actual) = actual else {
        return IdentityCheck::Pending;
    };

    if let Some((expected, actual)) =
        character_identity_mismatch(supervisor.expected_character.as_deref(), Some(&actual))
    {
        return IdentityCheck::Mismatch { expected, actual };
    }

    let newly_confirmed = !supervisor.identity_confirmed;
    supervisor.identity_confirmed = true;
    supervisor.character = Some(actual.clone());
    app_core.game_state.character_name = Some(actual);
    IdentityCheck::Matched { newly_confirmed }
}

/// Reject a wrong-character Lich attach locally. Aborting the Vellum network
/// task drops only its detachable-client socket; importantly, this path never
/// queues or sends the in-game `quit` command.
fn reject_identity_mismatch(
    supervisor: &mut Supervisor,
    app_core: &mut AppCore,
    expected: String,
    actual: String,
) {
    if let Some(connection) = supervisor.connection.take() {
        connection.task.abort();
    }
    supervisor.identity_confirmed = false;
    supervisor.user_disconnected = true;
    supervisor.reconnect_at = None;
    supervisor.reconnect_attempt = 0;
    app_core.game_state.connected = false;
    app_core.clear_pending_goals_launches();

    // Do not let commands parsed or queued from the rejected session survive
    // into a later, correctly matched connection.
    drop(app_core.take_pending_client_commands());
    drop(app_core.take_outbound());
    drop(app_core.take_webui_pending_raw());
    drop(app_core.take_webui_handshake());
    app_core.stop_webui();

    let message = format!(
        "Wrong Lich character on this port: expected {expected}, received {actual}. Connection closed locally; the game session was left running."
    );
    app_core.add_system_message(&message);
    let mut info = supervisor.status(SessionState::Disconnected);
    info.error = Some(message);
    app_core.set_remote_session_state(info);
}

fn initialize_lich_session_if_ready(app_core: &mut AppCore, supervisor: &mut Supervisor) {
    if supervisor.direct.is_some()
        || !supervisor.identity_confirmed
        || !app_core.game_state.connected
        || supervisor.post_connect_commands_sent
    {
        return;
    }

    app_core.seed_default_quickbars_if_empty();
    let identity_unknown = app_core.game_state.character.profession.is_none()
        || app_core.game_state.character.level.is_none();
    if identity_unknown && !supervisor.identity_probe_sent {
        if let Some(conn) = supervisor.command_connection() {
            // This is the same quiet StormFront character-sheet request as
            // the built-in quickbar. It bypasses send_command deliberately:
            // bootstrap traffic should not echo into Story.
            let _ = conn.command_tx.send("_info character".to_string());
        }
    }
    if !live_room_known(app_core) {
        if let Some(conn) = supervisor.command_connection() {
            // A late detachable-client attach also lacks a room replay. Ask
            // only when neither live room field is known, avoiding duplicate
            // LOOKs during ordinary fresh logins.
            let _ = conn.command_tx.send("look".to_string());
        }
    }
    if app_core
        .ui_state
        .get_window_by_type(crate::data::window::WidgetType::Spells, None)
        .is_some()
    {
        if let Some(conn) = supervisor.command_connection() {
            app_core.message_processor.skip_next_spells_clear();
            let _ = conn
                .command_tx
                .send("_spell _spell_update_links".to_string());
        }
    }
    supervisor.post_connect_commands_sent = true;
}

fn process_connection_message(
    app_core: &mut AppCore,
    supervisor: &mut Supervisor,
    message: ServerMessage,
) {
    let is_text = matches!(message, ServerMessage::Text(_));
    if matches!(message, ServerMessage::Disconnected) {
        supervisor.game_disconnect_seen = true;
    }
    if is_text {
        supervisor.first_text_seen = true;
    }

    let newly_connected = handle_server_message(app_core, message);
    if newly_connected && supervisor.identity_confirmed {
        supervisor.reconnect_attempt = 0;
        app_core.set_remote_session_state(supervisor.status(SessionState::Connected));
    } else if newly_connected {
        // The TCP socket exists, but it is not yet the configured character.
        // Keep the public lifecycle in Connecting until `<app>` confirms it.
        app_core.set_remote_session_state(supervisor.status(SessionState::Connecting));
    }

    if newly_connected {
        probe_lich_identity_if_needed(supervisor);
    }

    if !is_text {
        return;
    }

    match check_character_identity(supervisor, app_core) {
        IdentityCheck::Pending => {}
        IdentityCheck::Matched { newly_confirmed } => {
            if newly_confirmed && app_core.game_state.connected {
                supervisor.reconnect_attempt = 0;
                app_core.set_remote_session_state(supervisor.status(SessionState::Connected));
            }
        }
        IdentityCheck::Mismatch { expected, actual } => {
            reject_identity_mismatch(supervisor, app_core, expected, actual);
        }
    }
}

fn process_connection_end(
    app_core: &mut AppCore,
    supervisor: &mut Supervisor,
    result: Result<Result<()>, tokio::task::JoinError>,
    quit_deadline: &mut Option<Instant>,
) {
    supervisor.connection = None;
    *quit_deadline = None;
    app_core.game_state.connected = false;
    app_core.clear_pending_goals_launches();
    // Unattended tracking: a loss with zero user input since the connection
    // came up counts toward the cap. Without it, an abandoned phone would
    // re-login forever after game idle-kicks.
    if supervisor.saw_input_since_connect {
        supervisor.unattended_losses = 0;
    } else {
        supervisor.unattended_losses += 1;
    }
    let unattended = supervisor.unattended_losses >= MAX_UNATTENDED_LOSSES;
    let mut error_text = supervisor.terminal_connection_error.take();
    let terminal_failure = error_text.is_some();
    let stop_from_result = terminal_failure
        || match result {
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
                    app_core.add_system_message(&format!("Connection error: {e:#}"));
                    error_text = Some(format!("{e:#}"));
                    !supervisor.can_reconnect()
                }
            }
            Err(join_err) if join_err.is_cancelled() => !supervisor.can_reconnect(),
            Err(join_err) => {
                tracing::error!("Network task panicked: {join_err}");
                !supervisor.can_reconnect()
            }
        };
    if stop_from_result || unattended {
        supervisor.reconnect_at = None;
        if supervisor.user_disconnected {
            app_core.add_system_message("Logged out.");
            app_core.set_remote_session_state(supervisor.status(SessionState::Idle));
        } else if unattended && !stop_from_result {
            tracing::info!(
                "No user input across {} connections; not reconnecting",
                supervisor.unattended_losses
            );
            app_core.add_system_message(
                "Session looked idle - not reconnecting. Log in from the app to continue.",
            );
            let mut info = supervisor.status(SessionState::Disconnected);
            info.error = Some("Idle session ended".to_string());
            app_core.set_remote_session_state(info);
        } else {
            app_core
                .add_system_message("Session ended. Log in again from the web UI to reconnect.");
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
        app_core.set_remote_session_state(supervisor.status(SessionState::Reconnecting));
    }
}

/// Load the global launcher SSH settings (user, port, remote OS) used by the
/// mobile per-profile launch flow. Falls back to defaults if the launcher
/// config is absent — the flow then surfaces a clear "no SSH key/user" error.
fn launcher_ssh_settings() -> crate::launcher::config::SshConfig {
    crate::launcher::config::LauncherConfig::load()
        .map(|c| c.ssh)
        .unwrap_or_default()
}

/// What a web `connect` request resolved to.
enum ResolvedConnect {
    Direct(DirectConnectConfig),
    Lich {
        target: LichTarget,
        /// Display label only — Lich owns the actual login.
        character: Option<String>,
        /// Present when this Lich target has a launch command: if the port is
        /// down, SSH-launch it before attaching (mobile cold-start). None for
        /// a plain attach-only target.
        custom_launch: Option<String>,
    },
}

/// Resolve a web `connect` request into direct credentials or a Lich
/// attach target, saving the profile/password when asked. Returns a
/// user-facing error string on failure (never echoes the password).
fn resolve_connect(req: &SessionRequest) -> Result<ResolvedConnect, String> {
    let SessionRequest::Connect {
        profile,
        account,
        password,
        character,
        game,
        save_password,
        profile_name,
        lich_host,
        lich_port,
        custom_launch,
    } = req
    else {
        return Err("not a connect request".to_string());
    };

    let data_dir = crate::config::Config::base_dir()
        .map_err(|e| format!("No data directory available: {e}"))?;

    // Saved profile path: look up the target/credentials by profile name.
    if let Some(name) = profile {
        let store = crate::config::profiles::LauncherStore::load()
            .map_err(|e| format!("Could not read saved profiles: {e}"))?;
        let saved = store
            .find(name)
            .ok_or_else(|| format!("Profile '{name}' not found"))?;
        if saved.mode == crate::config::profiles::LaunchMode::Lich {
            return Ok(ResolvedConnect::Lich {
                target: LichTarget {
                    host: crate::network::normalize_lich_host(&saved.host)?,
                    port: saved.port,
                },
                character: Some(saved.character.clone()).filter(|c| !c.is_empty()),
                custom_launch: saved.custom_launch.clone(),
            });
        }
        let password = password
            .clone()
            .or_else(|| crate::config::profiles::load_password(&saved.account))
            .ok_or_else(|| {
                format!("No saved password for '{name}' - enter it and connect again")
            })?;
        return Ok(ResolvedConnect::Direct(DirectConnectConfig {
            account: saved.account.clone(),
            password,
            character: saved.character.clone(),
            game_code: DirectConnectConfig::game_name_to_code(&saved.game).to_string(),
            data_dir,
        }));
    }

    // Inline Lich target path: host+port, no credentials involved.
    if let (Some(host), Some(port)) = (lich_host.clone(), *lich_port) {
        let host = crate::network::normalize_lich_host(&host)?;
        // An empty custom-launch string means "no launch" (the web form sends
        // "" when the box is blank); normalize to None.
        let launch = custom_launch
            .clone()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if profile_name.is_some() {
            let mut store = crate::config::profiles::LauncherStore::load().unwrap_or_default();
            let mut saved = crate::config::profiles::LauncherProfile::new_direct();
            saved.mode = crate::config::profiles::LaunchMode::Lich;
            saved.name = profile_name
                .clone()
                .unwrap_or_else(|| format!("{host}:{port}"));
            saved.character = character.clone().unwrap_or_default();
            saved.host = host.clone();
            saved.port = port;
            saved.custom_launch = launch.clone();
            store.upsert(saved, None);
            if let Err(e) = store.save() {
                tracing::warn!("failed to save launcher.toml: {e:#}");
            }
        }
        return Ok(ResolvedConnect::Lich {
            target: LichTarget { host, port },
            character: character.clone(),
            custom_launch: launch,
        });
    }

    // Inline direct credentials path.
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

    Ok(ResolvedConnect::Direct(DirectConnectConfig {
        account,
        password,
        character,
        game_code: DirectConnectConfig::game_name_to_code(&game).to_string(),
        data_dir,
    }))
}

/// Resolve only the immutable connection + character named by a web connect
/// request.  This deliberately happens before [`resolve_connect`]: an owned
/// child must reject retargeting before password lookup, profile persistence,
/// SSH launch, or any other side effect can occur.
fn owned_connect_matches_startup(
    req: &SessionRequest,
    startup: &crate::core::session_registry::SessionLaunchIdentity,
) -> Result<bool, String> {
    use crate::core::session_registry::{SessionConnectionIdentity, SessionLaunchIdentity};

    let SessionRequest::Connect {
        profile,
        account,
        character,
        game,
        lich_host,
        lich_port,
        ..
    } = req
    else {
        return Err("not a connect request".to_string());
    };

    let requested = if let Some(name) = profile {
        let store = crate::config::profiles::LauncherStore::load()
            .map_err(|e| format!("Could not read saved profiles: {e}"))?;
        let saved = store
            .find(name)
            .ok_or_else(|| format!("Profile '{name}' not found"))?;
        SessionLaunchIdentity::from_profile(name, saved)
    } else if let (Some(host), Some(port)) = (lich_host.as_deref(), *lich_port) {
        let host = crate::network::normalize_lich_host(host)?;
        SessionLaunchIdentity {
            profile: String::new(),
            character: character.clone().unwrap_or_default().trim().to_string(),
            connection: SessionConnectionIdentity::Lich {
                host: crate::core::session_registry::normalize_host(&host),
                port,
            },
        }
    } else {
        let account = account
            .as_deref()
            .ok_or_else(|| "Account is required".to_string())?;
        let character = character
            .as_deref()
            .ok_or_else(|| "Character is required".to_string())?;
        SessionLaunchIdentity {
            profile: String::new(),
            character: character.trim().to_string(),
            connection: SessionConnectionIdentity::Direct {
                game: game
                    .as_deref()
                    .unwrap_or("prime")
                    .trim()
                    .to_ascii_lowercase(),
                account: account.trim().to_ascii_lowercase(),
            },
        }
    };

    Ok(startup
        .character
        .eq_ignore_ascii_case(requested.character.trim())
        && startup.connection == requested.connection)
}

/// `.launch` resolves through a different configuration surface than web
/// `connect`, but it is subject to the same immutable launcher ownership.
fn owned_launch_matches_startup(
    character: &str,
    requested: &crate::launcher::config::ResolvedLaunch,
    startup: &crate::core::session_registry::SessionLaunchIdentity,
) -> bool {
    use crate::core::session_registry::SessionConnectionIdentity;

    startup.character.eq_ignore_ascii_case(character.trim())
        && startup.connection
            == (SessionConnectionIdentity::Lich {
                host: crate::core::session_registry::normalize_host(&requested.attach_host),
                port: requested.attach_port,
            })
}

/// Re-check the resolved target that will actually be applied to the
/// supervisor. Saved profiles are mutable files, so this closes the narrow
/// time-of-check/time-of-use window between the side-effect-free ownership
/// preflight and [`resolve_connect`]'s second load.
fn owned_resolved_connect_matches_startup(
    resolved: &ResolvedConnect,
    startup: &crate::core::session_registry::SessionLaunchIdentity,
) -> bool {
    use crate::core::session_registry::SessionConnectionIdentity;

    match (resolved, &startup.connection) {
        (
            ResolvedConnect::Lich {
                target, character, ..
            },
            SessionConnectionIdentity::Lich { host, port },
        ) => {
            character
                .as_deref()
                .is_some_and(|character| startup.character.eq_ignore_ascii_case(character.trim()))
                && crate::core::session_registry::normalize_host(&target.host) == *host
                && target.port == *port
        }
        (ResolvedConnect::Direct(config), SessionConnectionIdentity::Direct { game, account }) => {
            startup
                .character
                .eq_ignore_ascii_case(config.character.trim())
                && account.eq_ignore_ascii_case(config.account.trim())
                && crate::network::DirectConnectConfig::game_name_to_code(game)
                    .eq_ignore_ascii_case(&config.game_code)
        }
        _ => false,
    }
}

fn reject_owned_retarget(app_core: &mut AppCore, startup_character: &str) {
    app_core.add_system_message(&format!(
        "This launcher-owned session is locked to {startup_character} and its startup connection. Use the launcher to open another session."
    ));
}

pub async fn async_run(
    config: crate::config::Config,
    character: Option<String>,
    direct: Option<DirectConnectConfig>,
    login_key: Option<String>,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    async_run_with_options(
        config,
        character,
        direct,
        login_key,
        shutdown,
        super::HeadlessLaunchOptions::default(),
    )
    .await
}

pub(super) async fn async_run_with_options(
    mut config: crate::config::Config,
    character: Option<String>,
    direct: Option<DirectConnectConfig>,
    login_key: Option<String>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    launch: super::HeadlessLaunchOptions,
) -> Result<()> {
    // The web frontend is the only interface — it is not optional here.
    config.web.enabled = true;

    let mut app_core = AppCore::new(config)?;
    // This runtime drains disconnect_requested (below) into
    // SessionRequest::Disconnect, so keep-open `.quit` detaches to the login
    // screen instead of tearing the whole engine down. Before this, a phone
    // `.quit` set running=false and killed the embedded web server out from
    // under the UI — a dead "Reconnecting…" screen nothing could revive.
    app_core.detach_quit_supported = true;

    let session_label = character
        .clone()
        .or_else(|| app_core.config.connection.character.clone())
        .unwrap_or_else(|| "default".to_string());
    let (sink, mut remote_rx) = crate::frontend::web::start_with_classic_maps(
        &app_core.config.web,
        session_label,
        app_core.map.classic_maps(),
    );
    let mut launch_endpoint_rx = sink.launch_endpoint_receiver();
    app_core.enable_remote(sink);
    app_core.set_remote_session_control(true);

    // With no local UI there is no `.webinfo` to surface the pairing token.
    // Keep launch output pending until the sidecar publishes the authenticated
    // endpoint it actually installed. The server is the sole token authority;
    // loading it here as well could race first-run creation and advertise a
    // token different from the one accepted by the listener.
    let mut launch_urls_pending = true;

    app_core.init_windows(NOMINAL_COLS, NOMINAL_ROWS);

    let is_direct = direct.is_some();
    let initial_expected_character = (!is_direct)
        .then(|| {
            character
                .clone()
                .or_else(|| app_core.config.connection.character.clone())
        })
        .flatten()
        .filter(|name| !name.trim().is_empty());
    let mut supervisor = Supervisor {
        character: direct
            .as_ref()
            .map(|d| d.character.clone())
            .or_else(|| character.clone()),
        expected_character: initial_expected_character,
        identity_confirmed: is_direct,
        identity_probe_sent: false,
        post_connect_commands_sent: false,
        terminal_connection_error: None,
        game: None,
        direct,
        lich_configured: lich_session_configured(login_key.as_deref(), &launch),
        login_key,
        lich_target: None,
        connection: None,
        reconnect_attempt: 0,
        reconnect_at: None,
        user_disconnected: false,
        saw_input_since_connect: false,
        unattended_losses: 0,
        phase_started: None,
        first_text_seen: false,
        game_disconnect_seen: false,
    };

    // Lich WebUI is reachable only on a Lich-attached session (a direct
    // eAccess connection bypasses Lich). Advertise it to phone clients so
    // they show the WebUI affordance only when it will work.
    app_core.set_webui_available(!is_direct);
    // Connection mode for anything that sends `;` commands (travel's ;go2
    // fallback). Separate from WebUI: `.webui off` must not disable `;go2`.
    app_core.set_lich_connected(!is_direct);

    // Auto-connect only when the CLI asked for a session (--direct / --key);
    // otherwise idle on the login screen.
    if supervisor.direct.is_some() || supervisor.lich_configured {
        supervisor.spawn(&mut app_core);
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

    // Set when the user quits: if the server hasn't closed the connection
    // by the deadline, close it ourselves (some closes linger server-side —
    // playtests saw quits that needed a follow-up command to complete).
    let mut quit_deadline: Option<Instant> = None;
    let mut exit_lifecycle = SessionExitLifecycle::new();
    let mut stall_watchdog = tokio::time::interval(CONNECTION_WATCHDOG_INTERVAL);
    stall_watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    tracing::info!("Headless runtime started (web UI is the interface)");

    while app_core.running {
        let mut session_requests: Vec<SessionRequest> = Vec::new();

        // Wait for any wake-up source, then drain everything non-blocking
        // below so remote state flushes once per batch.
        tokio::select! {
            readiness = launch_endpoint_rx.changed(), if launch_urls_pending => {
                if readiness.is_err() {
                    tracing::warn!("Web server readiness channel closed before startup completed");
                    launch_urls_pending = false;
                }
            }
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
                            supervisor.command_connection(),
                            supervisor.identity_confirmed && !exit_lifecycle.exit_requested,
                            event,
                            &mut session_requests,
                        ) {
                            supervisor.saw_input_since_connect = true;
                            supervisor.unattended_losses = 0;
                        }
                    }
                }
            }
            connection_event = async {
                match supervisor.connection.as_mut() {
                    Some(conn) => conn.next_event().await,
                    None => std::future::pending().await,
                }
            } => {
                match connection_event {
                    ConnectionEvent::Message(message) => {
                        let authoritative_disconnect =
                            matches!(message, ServerMessage::Disconnected);
                        process_connection_message(&mut app_core, &mut supervisor, message);
                        if authoritative_disconnect {
                            finish_exit_on_authoritative_disconnect(
                                &mut app_core,
                                &supervisor,
                                &exit_lifecycle,
                            );
                        }
                    }
                    ConnectionEvent::Ended(result) => {
                        process_connection_end(
                            &mut app_core,
                            &mut supervisor,
                            result,
                            &mut quit_deadline,
                        );
                        // process_connection_end already records the
                        // authoritative close and publishes the idle/logged
                        // out state. This policy only decides whether the
                        // owning child should now terminate.
                        if exit_lifecycle.exit_requested {
                            app_core.running = false;
                        }
                    }
                }
            }
            // Stall watchdog: a connection that has produced no game text
            // within the window is stuck (auth server hang, silent socket).
            // Tear it down; the completion arm schedules a retry for a silent
            // transport or fails closed for an unidentified Lich attach.
            _ = stall_watchdog.tick(),
                if connection_bootstrap_pending(&supervisor) => {
                if connection_bootstrap_stalled(&supervisor) {
                    if supervisor.first_text_seen && !supervisor.identity_confirmed {
                        let message = supervisor.expected_character.as_deref().map_or_else(
                            || "Lich connected, but character identity could not be verified.".to_string(),
                            |expected| format!(
                                "Lich connected, but did not identify the requested character {expected}."
                            ),
                        );
                        tracing::warn!("{message} Stopping this Vellum connection.");
                        app_core.add_system_message(&format!(
                            "{message} Stop or restart it from the launcher; the game session was left running."
                        ));
                        supervisor.terminal_connection_error = Some(message);
                    } else {
                        tracing::warn!("No game data within 45s of starting the connection; recycling");
                        app_core.add_system_message(
                            "Login is not responding - retrying...",
                        );
                    }
                    if let Some(conn) = supervisor.connection.as_ref() {
                        conn.task.abort();
                    }
                    // Completion arm fires next with a cancelled result.
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
                app_core.clear_pending_goals_launches();
                app_core.add_system_message("Logged out.");
                app_core.set_remote_session_state(supervisor.status(SessionState::Idle));
            }
            // Map/travel work in flight (mapdb download, walk executor RT
            // waits): wake periodically so the post-select tick below runs
            // even when the game is quiet. Guarded, so idle sessions stay
            // dormant (phone battery).
            _ = tokio::time::sleep(Duration::from_millis(250)),
                if app_core.travel.is_traveling()
                    || app_core.map_updater.in_flight()
                    || app_core.map.has_pending() => {}
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
                supervisor.spawn(&mut app_core);
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
            let commands_blocked =
                exit_lifecycle.exit_requested || session_requests_block_commands(&session_requests);
            let connection = if commands_blocked {
                None
            } else {
                supervisor.command_connection()
            };
            if handle_remote_event(
                &mut app_core,
                connection,
                supervisor.identity_confirmed && !commands_blocked,
                event,
                &mut session_requests,
            ) {
                supervisor.saw_input_since_connect = true;
                supervisor.unattended_losses = 0;
            }
        }
        loop {
            let message = supervisor
                .connection
                .as_mut()
                .and_then(|connection| connection.server_rx.try_recv().ok());
            let Some(message) = message else {
                break;
            };
            let authoritative_disconnect = matches!(message, ServerMessage::Disconnected);
            process_connection_message(&mut app_core, &mut supervisor, message);
            if authoritative_disconnect
                && finish_exit_on_authoritative_disconnect(
                    &mut app_core,
                    &supervisor,
                    &exit_lifecycle,
                )
            {
                break;
            }
        }

        if launch_urls_pending {
            if let Some(endpoint) = launch_endpoint_rx.borrow().clone() {
                if let Some(client) = launch.web_client {
                    if !open_local_web_client_with(&endpoint, client, crate::platform::open_url) {
                        // Do not include the authenticated URL or opener error:
                        // either can expose the fragment in a detached session log.
                        tracing::warn!("Could not open {} in the default browser", client.label());
                    }
                } else {
                    let play_url = local_web_client_url(&endpoint, "play");
                    let despana = crate::config::profiles::LaunchWebClient::Despana;
                    let despana_url = local_web_client_url(&endpoint, despana.route());
                    println!("Vellum Web UI: {play_url}");
                    println!("{}: {despana_url}", despana.label());
                    if app_core.config.web.bind != "127.0.0.1" {
                        println!(
                            "LAN clients: same #token fragment with this machine's IP (bind = {})",
                            app_core.config.web.bind
                        );
                    }
                }
                launch_urls_pending = false;
            }
        }

        // Map worker, mapdb updater, and walk executor tick once per batch;
        // travel commands go out through the same path as typed ones.
        app_core.poll_map();
        let commands_blocked =
            exit_lifecycle.exit_requested || session_requests_block_commands(&session_requests);
        if !commands_blocked {
            initialize_lich_session_if_ready(&mut app_core, &mut supervisor);
        }
        if !commands_blocked && supervisor.command_connection().is_some() {
            for command in app_core.take_outbound() {
                match app_core.send_command(command) {
                    Ok(crate::data::CommandOutcome::Game(out)) => {
                        let sent = supervisor
                            .command_connection()
                            .is_some_and(|conn| conn.command_tx.send(out.clone()).is_ok());
                        app_core.finish_game_command_send(&out, sent);
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!("travel command failed: {e}"),
                }
            }

            // Feed-injected dot-commands (<vellumCmd> from Lich scripts):
            // same core path as typed input; UI actions need a local UI and
            // are quietly skipped here.
            for command in app_core.take_pending_client_commands() {
                match app_core.send_command(command) {
                    Ok(crate::data::CommandOutcome::Ui(action)) => {
                        tracing::debug!("vellumCmd UI action skipped headless: {action}");
                    }
                    Ok(crate::data::CommandOutcome::Game(outbound)) => {
                        app_core.finish_game_command_send(&outbound, false);
                    }
                    Ok(crate::data::CommandOutcome::Handled) => {}
                    Err(e) => tracing::warn!("vellumCmd failed: {e}"),
                }
            }

            // Lich WebUI tick: drain the bridge (fans renders to phone clients),
            // send any queued `;ui handshake` to the game, and start the bridge
            // once its reply arrives. Only meaningful on a Lich-attached session
            // (webui_available); a direct eAccess connection has no Lich.
            app_core.pump_webui();
            for raw in app_core.take_webui_pending_raw() {
                if let Some(conn) = supervisor.command_connection() {
                    let _ = conn.command_tx.send(format!("{raw}\n"));
                }
            }
            if let Some(handshake) = app_core.take_webui_handshake() {
                app_core.start_webui(&tokio::runtime::Handle::current(), &handshake);
            }
        }

        // Keep-open `.quit`: core asked to drop the connection without
        // exiting — on this runtime that is exactly SessionRequest::Disconnect
        // (abort the session task, suppress auto-reconnect, land the web
        // client on the login screen).
        if app_core.take_disconnect_request() {
            session_requests.push(SessionRequest::Disconnect);
        }

        // Mobile embedded engine: there is NO process to exit. `.exit` (or a
        // `.quit` while already disconnected) must mean "leave the session,
        // show the login screen" — never "kill the web server serving this
        // UI", which leaves the app a dead Reconnecting… screen until it's
        // force-closed. Desktop headless keeps real exit semantics.
        #[cfg(not(feature = "desktop"))]
        if !app_core.running {
            app_core.running = true;
            app_core.add_system_message(
                "This client has no exit — returning to the login screen instead.",
            );
            session_requests.push(SessionRequest::Disconnect);
        }

        // Apply session-control requests from web clients.
        for request in session_requests {
            match request {
                SessionRequest::Stop => {
                    stop_inactive_session(&mut app_core, &mut supervisor);
                }
                SessionRequest::ExitLogout => {
                    // This orderly path deliberately disables the legacy
                    // typed-quit timeout: no timeout may abort the socket or
                    // claim logout before the game actually disconnects.
                    quit_deadline = None;
                    exit_lifecycle.request_exit();
                }
                SessionRequest::Disconnect => {
                    supervisor.user_disconnected = true;
                    supervisor.reconnect_at = None;
                    supervisor.reconnect_attempt = 0;
                    if let Some(conn) = supervisor.connection.take() {
                        conn.task.abort();
                        app_core.add_system_message("Disconnected by request.");
                    }
                    app_core.game_state.connected = false;
                    app_core.clear_pending_goals_launches();
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
                SessionRequest::Reconnect => {
                    // `.reconnect` from a phone/web client. If a session is
                    // live there is nothing to do. Otherwise clear the
                    // intentional-disconnect / backoff state and re-establish
                    // using the stored credentials (direct re-auths for a
                    // fresh ticket; detachable Lich re-attaches).
                    if supervisor.connection.is_some() {
                        app_core.add_system_message("Already connected.");
                    } else if !supervisor.can_reconnect_after_clear() {
                        app_core
                            .add_system_message("Nothing to reconnect to — log in from the app.");
                        app_core.set_remote_session_state(supervisor.status(SessionState::Idle));
                    } else {
                        supervisor.user_disconnected = false;
                        supervisor.reconnect_attempt = 0;
                        supervisor.reconnect_at = None;
                        let state = if supervisor.direct.is_some() {
                            SessionState::Authenticating
                        } else {
                            SessionState::Connecting
                        };
                        supervisor.spawn(&mut app_core);
                        app_core.set_remote_session_state(supervisor.status(state));
                    }
                }
                connect @ SessionRequest::Connect { .. } => {
                    if supervisor.connection.is_some() {
                        app_core.add_system_message(
                            "Already connected - disconnect before starting a new session.",
                        );
                        continue;
                    }
                    if let Some(startup) = launch.startup_identity.as_ref() {
                        match owned_connect_matches_startup(&connect, startup) {
                            Ok(true) => {}
                            Ok(false) => {
                                reject_owned_retarget(&mut app_core, &startup.character);
                                continue;
                            }
                            Err(message) => {
                                app_core.add_system_message(&format!(
                                    "Connect rejected for this launcher-owned session: {message}"
                                ));
                                continue;
                            }
                        }
                    }
                    match resolve_connect(&connect) {
                        Ok(resolved) => {
                            if let Some(startup) = launch.startup_identity.as_ref() {
                                if !owned_resolved_connect_matches_startup(&resolved, startup) {
                                    reject_owned_retarget(&mut app_core, &startup.character);
                                    continue;
                                }
                            }
                            let state = match resolved {
                                ResolvedConnect::Direct(cfg) => {
                                    supervisor.character = Some(cfg.character.clone());
                                    supervisor.expected_character = None;
                                    supervisor.game = Some(cfg.game_code.clone());
                                    supervisor.direct = Some(cfg);
                                    supervisor.lich_target = None;
                                    SessionState::Authenticating
                                }
                                ResolvedConnect::Lich {
                                    target,
                                    character,
                                    custom_launch,
                                } => {
                                    // A launch-capable Lich profile runs the
                                    // cold-start flow: probe the port, and if
                                    // it's down, SSH-launch then poll every 5s
                                    // until it's up. If already up (or no
                                    // launch command), attach directly.
                                    if let Some(command) = custom_launch {
                                        let ssh = launcher_ssh_settings();
                                        let spec = crate::launcher::flow::LaunchSpec::from_command(
                                            &command,
                                            &target.host,
                                            target.port,
                                            character.as_deref().unwrap_or(""),
                                            &ssh,
                                        );
                                        app_core.set_remote_session_state(
                                            supervisor.status(SessionState::Connecting),
                                        );
                                        let trust =
                                            crate::launcher::flow::HostKeyTrust::AutoPinFirstUse;
                                        let outcome =
                                            crate::launcher::flow::launch_spec(&spec, trust, |p| {
                                                tracing::debug!("launch progress: {p:?}")
                                            })
                                            .await;
                                        match outcome {
                                            Ok(_) => {
                                                supervisor.expected_character = character
                                                    .clone()
                                                    .filter(|name| !name.trim().is_empty());
                                                supervisor.character = character;
                                                supervisor.game = None;
                                                supervisor.direct = None;
                                                supervisor.lich_target = Some(target);
                                                SessionState::Connecting
                                            }
                                            Err(err) => {
                                                let message = format!("Launch failed: {err:#}");
                                                app_core.add_system_message(&message);
                                                let mut info =
                                                    supervisor.status(SessionState::Idle);
                                                info.error = Some(message);
                                                app_core.set_remote_session_state(info);
                                                continue;
                                            }
                                        }
                                    } else {
                                        supervisor.expected_character = character
                                            .clone()
                                            .filter(|name| !name.trim().is_empty());
                                        supervisor.character = character;
                                        supervisor.game = None;
                                        supervisor.direct = None;
                                        supervisor.lich_target = Some(target);
                                        SessionState::Connecting
                                    }
                                }
                            };
                            supervisor.login_key = None;
                            supervisor.user_disconnected = false;
                            supervisor.reconnect_attempt = 0;
                            supervisor.reconnect_at = None;
                            supervisor.spawn(&mut app_core);
                            app_core.set_remote_session_state(supervisor.status(state));
                        }
                        Err(message) => {
                            app_core.add_system_message(&format!("Connect failed: {message}"));
                            let mut info = supervisor.status(SessionState::Idle);
                            info.error = Some(message);
                            app_core.set_remote_session_state(info);
                        }
                    }
                }
                SessionRequest::Launch(character) => {
                    // `.launch <character>` from a phone/web client: SSH into the
                    // home PC, cold-start its headless Lich, then attach to the
                    // resulting detachable-client target exactly like a Lich
                    // connect. The flow runs inline here (we're already async);
                    // progress is surfaced as system messages.
                    if supervisor.connection.is_some() {
                        app_core.add_system_message(
                            "Already connected - disconnect before launching a session.",
                        );
                        continue;
                    }
                    let config = match crate::launcher::config::LauncherConfig::load() {
                        Ok(cfg) => cfg,
                        Err(err) => {
                            app_core.add_system_message(&format!("Launcher config error: {err:#}"));
                            continue;
                        }
                    };
                    let requested = match config.resolve(&character) {
                        Ok(requested) => requested,
                        Err(err) => {
                            app_core.add_system_message(&format!(
                                "Launch rejected for this launcher-owned session: {err:#}"
                            ));
                            continue;
                        }
                    };
                    if let Some(startup) = launch.startup_identity.as_ref() {
                        if !owned_launch_matches_startup(&character, &requested, startup) {
                            reject_owned_retarget(&mut app_core, &startup.character);
                            continue;
                        }
                    }
                    app_core.set_remote_session_state(supervisor.status(SessionState::Connecting));
                    // Auto-pin the host key on first use: over an already-private
                    // tunnel there is no interactive prompt on this path, and a
                    // changed key is still hard-rejected inside the flow.
                    let trust = crate::launcher::flow::HostKeyTrust::AutoPinFirstUse;
                    let launch_result = {
                        // Collect progress into messages after the flow (the
                        // callback can't borrow app_core while it's borrowed by
                        // the surrounding loop).
                        let mut messages = Vec::new();
                        // Launch the exact spec checked above. Re-resolving
                        // mutable launcher config here would reopen a
                        // retargeting window after ownership validation.
                        let spec = crate::launcher::flow::LaunchSpec {
                            ssh_host: config.ssh.host.clone(),
                            ssh_port: config.ssh.port,
                            ssh_user: config.ssh.user.clone(),
                            remote_os: config.ssh.remote_os.into(),
                            program: requested.program,
                            args: requested.args,
                            local: crate::launcher::flow::is_local_host(&requested.attach_host),
                            attach_host: requested.attach_host,
                            attach_port: requested.attach_port,
                            character: character.clone(),
                        };
                        let res = crate::launcher::flow::launch_spec(&spec, trust, |p| {
                            messages.push(format!("{p:?}"))
                        })
                        .await;
                        for m in messages {
                            tracing::debug!("launch progress: {m}");
                        }
                        res
                    };
                    match launch_result {
                        Ok(target) => {
                            supervisor.character = Some(target.character.clone());
                            supervisor.expected_character = Some(target.character.clone())
                                .filter(|name| !name.trim().is_empty());
                            supervisor.game = None;
                            supervisor.direct = None;
                            supervisor.lich_target = Some(LichTarget {
                                host: target.host.clone(),
                                port: target.port,
                            });
                            supervisor.login_key = None;
                            supervisor.user_disconnected = false;
                            supervisor.reconnect_attempt = 0;
                            supervisor.reconnect_at = None;
                            app_core.add_system_message(&format!(
                                "Launched {} — attaching to {}:{}.",
                                target.character, target.host, target.port
                            ));
                            supervisor.spawn(&mut app_core);
                            app_core.set_remote_session_state(
                                supervisor.status(SessionState::Connecting),
                            );
                        }
                        Err(err) => {
                            let message = format!("Launch failed: {err:#}");
                            app_core.add_system_message(&message);
                            let mut info = supervisor.status(SessionState::Idle);
                            info.error = Some(message);
                            app_core.set_remote_session_state(info);
                        }
                    }
                }
            }
        }

        progress_exit_logout(&mut app_core, &mut supervisor, &mut exit_lifecycle);

        app_core.poll_tts_events();
        // Debounced layout autosave (layout dot-commands from web clients).
        app_core.tick_layout_autosave();
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
/// What a dispatched command asks the supervisor to do next.
#[derive(Clone, PartialEq, Eq)]
enum DispatchResult {
    /// Nothing special; keep the current session.
    None,
    /// The outbound command was `quit`: the server will close shortly and the
    /// supervisor treats that as an intentional logout, not a drop.
    Quit,
    /// The user asked to reconnect (`.reconnect`): re-establish the session
    /// using the stored credentials.
    Reconnect,
    /// The user ran `.launch <character>`: run the SSH-launcher flow to
    /// cold-start a headless Lich on the home PC, then attach to it.
    Launch(String),
}

/// `dispatch_command`. `action:`/`menu:` outputs need a local UI and get a
/// notice instead. Returns what the supervisor should do next (see
/// [`DispatchResult`]).
///
/// The `UiAction` match is EXHAUSTIVE on purpose (like the TUI and GUI
/// handlers): adding a variant forces a decision for the phone/web client too,
/// rather than silently degrading to "needs the desktop client".
fn dispatch_command(
    app_core: &mut AppCore,
    connection: Option<&Connection>,
    command: String,
) -> DispatchResult {
    dispatch_command_from(app_core, connection, None, command)
}

fn dispatch_remote_command(
    app_core: &mut AppCore,
    connection: Option<&Connection>,
    client_id: u64,
    command: String,
) -> DispatchResult {
    dispatch_command_from(app_core, connection, Some(client_id), command)
}

fn dispatch_command_from(
    app_core: &mut AppCore,
    connection: Option<&Connection>,
    remote_client_id: Option<u64>,
    command: String,
) -> DispatchResult {
    use crate::data::CommandOutcome;
    let command = command.trim_end().to_string();
    if command.is_empty() {
        return DispatchResult::None;
    }
    let outcome = match remote_client_id {
        Some(client_id) => app_core.send_remote_command(client_id, command),
        None => app_core.send_command(command),
    };
    match outcome {
        Ok(CommandOutcome::Handled) => DispatchResult::None,
        // UI packs are core-side work; everything else needs a local UI.
        Ok(CommandOutcome::Ui(action)) => dispatch_ui_action(app_core, action),
        Ok(CommandOutcome::Game(outbound)) => {
            if !should_send_to_network(&outbound) {
                return DispatchResult::None;
            }
            let is_quit = outbound.trim().eq_ignore_ascii_case("quit");
            match connection {
                Some(conn) => {
                    app_core
                        .perf_stats
                        .record_bytes_sent((outbound.len() + 1) as u64);
                    let sent = conn.command_tx.send(outbound.clone()).is_ok();
                    app_core.finish_game_command_send(&outbound, sent);
                    if is_quit {
                        DispatchResult::Quit
                    } else {
                        DispatchResult::None
                    }
                }
                None => {
                    app_core.finish_game_command_send(&outbound, false);
                    app_core.add_system_message("Not connected - command not sent.");
                    DispatchResult::None
                }
            }
        }
        Err(err) => {
            app_core.add_system_message(&format!("Command error: {}", err));
            DispatchResult::None
        }
    }
}

/// While a Lich socket is still proving which character it belongs to, keep a
/// deliberately small local-control surface available without letting an
/// arbitrary dot command arm travel, timers, GOALS, or another game-output
/// path.  `.launch` is included because launcher-owned runtimes validate its
/// immutable target before doing any work in the session-request phase.
fn dispatch_identity_pending_command(app_core: &mut AppCore, command: String) -> DispatchResult {
    let command_word = command
        .trim()
        .strip_prefix('.')
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        command_word.as_str(),
        "exit"
            | "quit"
            | "q"
            | "reconnect"
            | "launch"
            | "launcher"
            | "help"
            | "h"
            | "?"
            | "version"
            | "ver"
    ) {
        return dispatch_command(app_core, None, command);
    }

    app_core.add_system_message(
        "Waiting for Lich to confirm the configured character; command not sent.",
    );
    DispatchResult::None
}

/// Translate a [`DispatchResult`] into the matching session request (if any).
fn push_dispatch_request(result: DispatchResult, session_requests: &mut Vec<SessionRequest>) {
    match result {
        DispatchResult::None => {}
        DispatchResult::Quit => session_requests.push(SessionRequest::UserQuit),
        DispatchResult::Reconnect => session_requests.push(SessionRequest::Reconnect),
        DispatchResult::Launch(character) => {
            session_requests.push(SessionRequest::Launch(character))
        }
    }
}

/// Should this outbound string actually go to the game socket? Filters the UI
/// sentinels that never belong on the wire. Mirrors the GUI's
/// `should_send_to_network` (previously the headless copy forgot `action:`).
fn should_send_to_network(command: &str) -> bool {
    !(command.is_empty()
        || command.starts_with("__")
        || command.starts_with("action:")
        || command.starts_with("menu:"))
}

/// Decide how the phone/web client answers each `UiAction`.
///
/// EXHAUSTIVE by design — a new `UiAction` variant must be answered here, the
/// same rule the TUI (`tui/menu_actions.rs`) and GUI (`gui/app.rs`) handlers
/// enforce. Actions that open a desktop editor panel get a notice pointing the
/// phone user at the client's own on-device surface; actions the headless
/// runtime genuinely supports (reconnect, UI pack import/export, perf dump) do
/// the real work.
fn dispatch_ui_action(app_core: &mut AppCore, action: crate::data::UiAction) -> DispatchResult {
    use crate::data::UiAction as A;

    // Notice shown for actions whose editing UI lives on the desktop clients.
    // Phrased for the phone: many of these have an equivalent on-device sheet
    // reached from the app's own menus, not from a typed dot-command.
    let desktop_only = |app_core: &mut AppCore| {
        app_core.add_system_message(
            "That editor opens on the desktop client; on the phone, use the app's own menus.",
        );
        DispatchResult::None
    };

    match action {
        // --- Genuinely supported on the headless/web path ---
        A::Reconnect => {
            app_core.add_system_message("Reconnecting...");
            DispatchResult::Reconnect
        }
        A::Launch(character) => {
            let character = character.trim().to_string();
            if character.is_empty() {
                app_core.add_system_message(
                    "Usage: .launch <character>. Configure targets in ssh-launcher.toml.",
                );
                DispatchResult::None
            } else {
                app_core.add_system_message(&format!("Launching {character}…"));
                DispatchResult::Launch(character)
            }
        }
        A::UiExport(args) => {
            app_core.uiexport_with(&args, Vec::new());
            DispatchResult::None
        }
        A::UiImport(args) => {
            if app_core.uiimport(&args).is_some() {
                app_core.add_system_message(
                    "This pack also carries a GUI layout — run the import in the GUI to install it.",
                );
            }
            DispatchResult::None
        }
        A::PerformanceDump => {
            app_core.write_perf_dump(crate::performance::PerfFrontend::Headless, None);
            DispatchResult::None
        }
        // The phone HAS a touch-wheel editor (app.js openTouchWheelEditor); the
        // old wildcard wrongly told users it needed the desktop.
        A::TouchWheelEditor => {
            app_core.add_system_message(
                "Open the touch-wheel editor from the phone's radial-wheel menu.",
            );
            DispatchResult::None
        }

        // --- Editors / panels that live on the desktop clients ---
        A::Settings
        | A::Highlights
        | A::AddHighlight
        | A::EditHighlight(_)
        | A::Keybinds
        | A::AddKeybind
        | A::MenuKeybinds
        | A::EditStatusAbbrev
        | A::Controller
        | A::Hotbars
        | A::JinxPanel
        | A::Streams
        | A::Colors
        | A::AddColor
        | A::UiColors
        | A::SpellColors
        | A::AddSpellColor
        | A::Themes
        | A::SetTheme(_)
        | A::EditTheme
        | A::RoomImagesEdit
        | A::AlertPacks
        | A::SorterEdit
        | A::Skins
        | A::SetSkin(_)
        | A::MakeSkin(_)
        | A::HarmonySkin(_)
        | A::ReloadSkin
        | A::SetPalette
        | A::ResetPalette
        | A::NextTab
        | A::PrevTab
        | A::NextUnread
        | A::AddWindowPicker
        | A::EditWindow(_)
        | A::HideWindow(_)
        | A::ShowWindow(_)
        | A::CreateWindow(_)
        | A::WindowList
        | A::CustomWindows
        | A::KnownWindows
        | A::EditIndicators
        | A::StreamActions(_)
        | A::StreamPickWindow(_)
        | A::StreamRoute { .. }
        | A::StreamSubscribe { .. }
        | A::StreamNewWindow(_)
        | A::LoadLayoutToml(_)
        | A::SaveLayout(_)
        | A::LoadLayout { .. }
        | A::ListLayouts
        | A::ResizeLayout(_)
        | A::AnchorInfer
        | A::SaveSkin(_)
        | A::PackEditor
        | A::Zone { .. }
        | A::WebUiPicker
        | A::WebUiOff
        | A::WebUiOpen(_)
        | A::LauncherEditor
        | A::CreatureFieldEdit
        | A::SnapDebug => desktop_only(app_core),
    }
}

/// Returns true when the event was direct user input (command, macro,
/// link tap) — the supervisor uses this to tell attended sessions from
/// abandoned ones.
fn handle_remote_event(
    app_core: &mut AppCore,
    connection: Option<&Connection>,
    game_commands_allowed: bool,
    event: crate::core::remote::RemoteEvent,
    session_requests: &mut Vec<SessionRequest>,
) -> bool {
    use crate::core::remote::RemoteEvent;
    match event {
        RemoteEvent::Command { client_id, text } => {
            tracing::debug!("remote command: '{}'", text);
            let result = if game_commands_allowed {
                dispatch_remote_command(app_core, connection, client_id, text)
            } else {
                dispatch_identity_pending_command(app_core, text)
            };
            push_dispatch_request(result, session_requests);
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
            if !game_commands_allowed {
                app_core.add_system_message(
                    "Waiting for Lich to confirm the configured character; action not sent.",
                );
                return true;
            }
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
                let sent = if let Some(conn) = connection {
                    app_core
                        .perf_stats
                        .record_bytes_sent((cmd.len() + 1) as u64);
                    conn.command_tx.send(cmd.clone()).is_ok()
                } else {
                    false
                };
                app_core.finish_game_command_send(&cmd, sent);
            }
            true
        }
        RemoteEvent::MacroSave {
            group,
            label,
            command,
            color,
            confirm,
            insert,
            client,
            options,
            original,
        } => {
            let button = crate::config::MacroButton {
                label,
                // A client-action button carries no game command.
                command: Some(command)
                    .filter(|c| !c.is_empty())
                    .filter(|_| client.is_none()),
                client,
                options,
                color,
                confirm,
                insert,
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
            if !game_commands_allowed {
                app_core.add_system_message(
                    "Waiting for Lich to confirm the configured character; macro not sent.",
                );
                return true;
            }
            match app_core.config.macros.resolve(&id).map(String::from) {
                Some(command) => {
                    tracing::debug!("remote macro '{}': '{}'", id, command);
                    push_dispatch_request(
                        dispatch_command(app_core, connection, command),
                        session_requests,
                    );
                }
                None => {
                    tracing::warn!("remote macro id '{}' did not resolve (stale client?)", id)
                }
            }
            true
        }
        RemoteEvent::WheelPick { key, path } => {
            if !game_commands_allowed {
                app_core.add_system_message(
                    "Waiting for Lich to confirm the configured character; action not sent.",
                );
                return true;
            }
            match app_core.wheel_pick_command(&key, &path) {
                Some(command) => {
                    tracing::debug!("remote wheel pick '{}' {:?}: '{}'", key, path, command);
                    push_dispatch_request(
                        dispatch_command(app_core, connection, command),
                        session_requests,
                    );
                }
                None => tracing::warn!(
                    "remote wheel pick '{}' {:?} did not resolve (stale client?)",
                    key,
                    path
                ),
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
            lich_host,
            lich_port,
            custom_launch,
        } => {
            session_requests.push(SessionRequest::Connect {
                profile,
                account,
                password,
                character,
                game,
                save_password,
                profile_name,
                lich_host,
                lich_port,
                custom_launch,
            });
            true
        }
        RemoteEvent::SessionDisconnect => {
            session_requests.push(SessionRequest::Disconnect);
            true
        }
        RemoteEvent::SessionStop => {
            session_requests.push(SessionRequest::Stop);
            true
        }
        RemoteEvent::SessionExitLogout => {
            session_requests.push(SessionRequest::ExitLogout);
            true
        }
        RemoteEvent::LauncherSshGet {
            client_id,
            request_id,
        } => {
            app_core.handle_remote_launcher_ssh_get(client_id, request_id);
            true
        }
        RemoteEvent::LauncherSshPut {
            client_id,
            request_id,
            user,
            host,
            port,
            remote_os,
            generate_key,
        } => {
            app_core.handle_remote_launcher_ssh_put(
                client_id,
                request_id,
                user,
                host,
                port,
                remote_os,
                generate_key,
            );
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
        RemoteEvent::SettingsGet {
            client_id,
            request_id,
        } => {
            app_core.handle_remote_settings_get(client_id, request_id);
            true
        }
        RemoteEvent::SettingsPut {
            client_id,
            request_id,
            key,
            value,
            scope,
            clear,
        } => {
            app_core.handle_remote_settings_put(client_id, request_id, key, value, scope, clear);
            true
        }
        RemoteEvent::StreamsGet {
            client_id,
            request_id,
        } => {
            app_core.handle_remote_streams_get(client_id, request_id);
            true
        }
        RemoteEvent::StreamsPut {
            client_id,
            request_id,
            stream,
            target,
        } => {
            app_core.handle_remote_streams_put(client_id, request_id, stream, target);
            true
        }
        RemoteEvent::ColorsGet {
            client_id,
            request_id,
            scope,
        } => {
            app_core.handle_remote_colors_get(client_id, request_id, scope);
            true
        }
        RemoteEvent::ColorsPut {
            client_id,
            request_id,
            scope,
            colors,
        } => {
            app_core.handle_remote_colors_put(client_id, request_id, scope, colors);
            true
        }
        RemoteEvent::TouchWheelGet {
            client_id,
            request_id,
            scope,
        } => {
            app_core.handle_remote_touch_wheel_get(client_id, request_id, scope);
            true
        }
        RemoteEvent::TouchWheelPut {
            client_id,
            request_id,
            scope,
            slices,
        } => {
            app_core.handle_remote_touch_wheel_put(client_id, request_id, scope, slices);
            true
        }
        RemoteEvent::WebUiSubscribe { page } => {
            if !game_commands_allowed {
                app_core.add_system_message(
                    "Waiting for Lich to confirm the configured character; WebUI action not sent.",
                );
                return true;
            }
            // First subscription starts the bridge: trigger the handshake if
            // it isn't up yet (the raw `;ui handshake` drains next tick, the
            // reply starts the socket, then the subscribe replays via Hello).
            if !app_core.webui_is_active() {
                app_core.request_webui_handshake();
            }
            app_core.webui_subscribe(&page);
            true
        }
        RemoteEvent::WebUiUnsubscribe { page } => {
            if !game_commands_allowed {
                app_core.add_system_message(
                    "Waiting for Lich to confirm the configured character; WebUI action not sent.",
                );
                return true;
            }
            app_core.webui_unsubscribe(&page);
            true
        }
        RemoteEvent::WebUiEvent { page, cid, value } => {
            if !game_commands_allowed {
                app_core.add_system_message(
                    "Waiting for Lich to confirm the configured character; WebUI action not sent.",
                );
                return true;
            }
            app_core.webui_send_event(page, cid, value);
            true
        }
        RemoteEvent::MapLocations {
            client_id,
            request_id,
        } => {
            app_core.handle_remote_map_locations(client_id, request_id);
            true
        }
        RemoteEvent::MapView {
            client_id,
            request_id,
            location,
        } => {
            app_core.handle_remote_map_view(client_id, request_id, location);
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
        RemoteEvent::SkillTrainerOpen => {
            if !game_commands_allowed {
                app_core.add_system_message(
                    "Waiting for Lich to confirm the configured character; GOALS not sent.",
                );
                return true;
            }
            // Open: if a page is already loaded, just re-mirror it; otherwise
            // fetch a fresh one (arms the trainer + sends `goals` to the game).
            if app_core.ui_state.skill_trainer.data.is_some() {
                app_core.ui_state.skill_trainer.open = true;
                app_core.push_skill_trainer_remote();
            } else {
                let cmd = app_core.skill_trainer_reload_command();
                push_dispatch_request(
                    dispatch_command(app_core, connection, cmd),
                    session_requests,
                );
                // Loading state pushes immediately; the loaded page follows
                // via poll_skill_trainer once the worker parses it.
                app_core.push_skill_trainer_remote();
            }
            true
        }
        RemoteEvent::SkillTrainerReload => {
            if !game_commands_allowed {
                app_core.add_system_message(
                    "Waiting for Lich to confirm the configured character; GOALS not sent.",
                );
                return true;
            }
            let cmd = app_core.skill_trainer_reload_command();
            push_dispatch_request(
                dispatch_command(app_core, connection, cmd),
                session_requests,
            );
            app_core.push_skill_trainer_remote();
            true
        }
        RemoteEvent::SkillTrainerStep { id, n, raise } => {
            app_core.skill_trainer_step(id, n, raise);
            app_core.push_skill_trainer_remote();
            true
        }
        RemoteEvent::SkillTrainerApply => {
            if !game_commands_allowed {
                app_core.add_system_message(
                    "Waiting for Lich to confirm the configured character; skill goals not submitted.",
                );
                return true;
            }
            app_core.skill_trainer_apply();
            app_core.push_skill_trainer_remote();
            true
        }
        RemoteEvent::SkillTrainerProfileSave { name } => {
            app_core.skill_trainer_save_profile(&name);
            // Profile list changed but not the goal revision; force a push by
            // clearing the fingerprint so the new profile name reaches phones.
            app_core.invalidate_skill_trainer_remote();
            app_core.push_skill_trainer_remote();
            true
        }
        RemoteEvent::SkillTrainerProfileLoad { name } => {
            app_core.skill_trainer_load_profile(&name);
            app_core.push_skill_trainer_remote();
            true
        }
        RemoteEvent::SkillTrainerProfileDelete { name } => {
            app_core.skill_trainer_delete_profile(&name);
            app_core.invalidate_skill_trainer_remote();
            app_core.push_skill_trainer_remote();
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
            // Parse timing is recorded inside process_server_data.
            if let Err(e) = app_core.process_server_data(&line) {
                tracing::error!("Error processing server data: {}", e);
            }

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

            // Realize game-offered windows (openDialog-templated widgets,
            // containers whose offer the user has Shown).
            app_core.realize_offered_windows(NOMINAL_COLS, NOMINAL_ROWS);
            false
        }
        ServerMessage::Connected => {
            tracing::info!("Connected to game server");
            let newly = !app_core.game_state.connected;
            app_core.game_state.connected = true;
            // A web/headless session can be created before its launcher has
            // started the local Lich process. Re-resolve here so automatic
            // map discovery observes the process that owns this connection;
            // ensure_db is a no-op when the configured source is unchanged.
            app_core.refresh_map_source();
            newly
        }
        ServerMessage::Disconnected => {
            tracing::info!("Disconnected from game server");
            app_core.game_state.connected = false;
            app_core.clear_pending_goals_launches();
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::data::UiAction;
    use tokio::sync::mpsc::error::TryRecvError;

    fn app() -> AppCore {
        AppCore::new(Config::default()).expect("AppCore")
    }

    fn lich_supervisor(expected: Option<&str>) -> Supervisor {
        Supervisor {
            direct: None,
            login_key: None,
            lich_configured: true,
            lich_target: Some(LichTarget {
                host: "127.0.0.1".to_string(),
                port: 8000,
            }),
            connection: None,
            reconnect_attempt: 0,
            reconnect_at: None,
            user_disconnected: false,
            saw_input_since_connect: false,
            unattended_losses: 0,
            phase_started: None,
            first_text_seen: false,
            game_disconnect_seen: false,
            character: expected.map(str::to_string),
            expected_character: expected.map(str::to_string),
            identity_confirmed: expected.is_none(),
            identity_probe_sent: false,
            post_connect_commands_sent: false,
            terminal_connection_error: None,
            game: None,
        }
    }

    fn test_connection() -> (
        Connection,
        mpsc::UnboundedReceiver<String>,
        mpsc::Sender<ServerMessage>,
    ) {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (server_tx, server_rx) = mpsc::channel(8);
        let task = tokio::spawn(std::future::pending::<Result<()>>());
        (
            Connection {
                command_tx,
                server_rx,
                task,
            },
            command_rx,
            server_tx,
        )
    }

    #[test]
    fn local_web_url_requires_the_actual_port_and_preserves_the_route() {
        let endpoint = RemoteLaunchEndpoint::new("127.0.0.1".to_string(), 8041, "abc".to_string());
        assert_eq!(
            local_web_client_url(&endpoint, "despana"),
            "http://127.0.0.1:8041/despana#token=abc"
        );
        assert_eq!(
            local_web_client_url(&endpoint, "play"),
            "http://127.0.0.1:8041/play#token=abc"
        );

        let ipv6 = RemoteLaunchEndpoint::new("::1".to_string(), 8042, "def".to_string());
        assert_eq!(
            local_web_client_url(&ipv6, "despana"),
            "http://[::1]:8042/despana#token=def"
        );
    }

    #[test]
    fn ordinary_headless_does_not_auto_attach_lich_or_open_a_browser() {
        let launch = super::super::HeadlessLaunchOptions::default();

        assert!(!lich_session_configured(None, &launch));
        assert_eq!(launch.web_client, None);
        assert!(lich_session_configured(Some("one-shot-key"), &launch));
    }

    #[test]
    fn launcher_web_client_auto_attach_does_not_need_a_fake_login_key() {
        let launch = super::super::HeadlessLaunchOptions {
            auto_connect_lich: true,
            web_client: Some(crate::config::profiles::LaunchWebClient::Despana),
            startup_identity: None,
        };

        assert!(lich_session_configured(None, &launch));
    }

    #[tokio::test]
    async fn explicit_exit_sends_one_quit_and_waits_for_authoritative_disconnect() {
        let mut core = app();
        core.game_state.connected = true;
        let mut supervisor = lich_supervisor(None);
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);
        let mut lifecycle = SessionExitLifecycle::new();
        lifecycle.request_exit();

        progress_exit_logout(&mut core, &mut supervisor, &mut lifecycle);
        assert_eq!(command_rx.try_recv().as_deref(), Ok("quit"));
        assert!(core.running, "sending quit alone must not stop the child");
        progress_exit_logout(&mut core, &mut supervisor, &mut lifecycle);
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));

        process_connection_message(&mut core, &mut supervisor, ServerMessage::Disconnected);
        assert!(finish_exit_on_authoritative_disconnect(
            &mut core,
            &supervisor,
            &lifecycle,
        ));
        assert!(!core.running);
        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn exit_waits_for_matching_lich_identity_before_sending_quit() {
        let mut core = app();
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);
        let mut lifecycle = SessionExitLifecycle::new();
        lifecycle.request_exit();

        // Merely having a connection task is not proof that the game is
        // active or disconnected; wait for its authoritative messages.
        progress_exit_logout(&mut core, &mut supervisor, &mut lifecycle);
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));
        assert!(core.running);

        core.game_state.connected = true;
        progress_exit_logout(&mut core, &mut supervisor, &mut lifecycle);
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));

        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text(r#"<app char="aster" game="GS" title="[GSIV: aster]"/>"#.into()),
        );
        progress_exit_logout(&mut core, &mut supervisor, &mut lifecycle);
        assert_eq!(command_rx.try_recv().as_deref(), Ok("quit"));
        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn exit_requested_after_disconnect_message_stops_without_sending_quit() {
        let mut core = app();
        core.game_state.connected = true;
        let mut supervisor = lich_supervisor(None);
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);
        process_connection_message(&mut core, &mut supervisor, ServerMessage::Disconnected);

        let mut lifecycle = SessionExitLifecycle::new();
        lifecycle.request_exit();
        progress_exit_logout(&mut core, &mut supervisor, &mut lifecycle);

        assert!(!core.running);
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));
        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn exit_then_command_in_one_batch_never_reaches_the_socket() {
        let mut core = app();
        let (connection, mut command_rx, _server_tx) = test_connection();
        let mut requests = Vec::new();

        handle_remote_event(
            &mut core,
            Some(&connection),
            true,
            crate::core::remote::RemoteEvent::SessionExitLogout,
            &mut requests,
        );
        assert!(session_requests_block_commands(&requests));
        handle_remote_event(
            &mut core,
            None,
            false,
            crate::core::remote::RemoteEvent::Command {
                client_id: 1,
                text: "look".to_string(),
            },
            &mut requests,
        );

        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));
        connection.task.abort();
    }

    #[test]
    fn despana_open_uses_the_authoritative_endpoint_and_failure_is_nonfatal() {
        let endpoint =
            RemoteLaunchEndpoint::new("127.0.0.1".to_string(), 8057, "secret".to_string());
        let mut opened = String::new();

        let opened_ok = open_local_web_client_with(
            &endpoint,
            crate::config::profiles::LaunchWebClient::Despana,
            |url| {
                opened.push_str(url);
                anyhow::bail!("browser unavailable")
            },
        );

        assert!(!opened_ok);
        assert_eq!(opened, "http://127.0.0.1:8057/despana#token=secret");
    }

    #[tokio::test]
    async fn despana_typed_goals_launch_url_is_addressed_to_its_browser() {
        let mut core = AppCore::new_for_test();
        let (sink, handles, _event_rx) = crate::core::remote::RemoteSink::new(8);
        let mut delta_rx = handles.delta_tx.subscribe();
        core.enable_remote(sink);
        let (connection, mut command_rx, _server_tx) = test_connection();
        let mut requests = Vec::new();

        assert!(handle_remote_event(
            &mut core,
            Some(&connection),
            true,
            crate::core::remote::RemoteEvent::Command {
                client_id: 41,
                text: "GOALS".to_string(),
            },
            &mut requests,
        ));
        assert_eq!(command_rx.try_recv().as_deref(), Ok("GOALS"));
        assert!(!core.ui_state.skill_trainer.open);

        let launch_url = "/gs4/play/cm/loader.asp?ticket=despana-browser-test";
        core.process_server_data(&format!("<LaunchURL src='{launch_url}'/>"))
            .expect("parse game LaunchURL response");
        assert_eq!(
            core.message_processor
                .pending_launch_urls
                .front()
                .map(String::as_str),
            Some(launch_url),
            "the game LaunchURL must reach the action router"
        );
        core.poll_skill_trainer();

        let expected_url = format!("https://www.play.net{launch_url}");
        let addressed = std::iter::from_fn(|| delta_rx.try_recv().ok()).find_map(|delta| {
            if let crate::core::remote::RemoteDelta::OpenUrl { client_id, url } = delta {
                Some((client_id, url))
            } else {
                None
            }
        });
        assert_eq!(addressed, Some((41, expected_url)));
        connection.task.abort();
    }

    #[tokio::test]
    async fn browser_goals_web_stays_external_but_never_uses_the_host_opener() {
        let mut core = AppCore::new_for_test();
        let (sink, handles, _event_rx) = crate::core::remote::RemoteSink::new(8);
        let mut delta_rx = handles.delta_tx.subscribe();
        core.enable_remote(sink);
        let (connection, mut command_rx, _server_tx) = test_connection();
        let mut requests = Vec::new();

        assert!(handle_remote_event(
            &mut core,
            Some(&connection),
            true,
            crate::core::remote::RemoteEvent::Command {
                client_id: 41,
                text: "GOALS web".to_string(),
            },
            &mut requests,
        ));

        assert_eq!(command_rx.try_recv().as_deref(), Ok("goals"));
        let launch_url = "/gs4/play/cm/loader.asp?ticket=goals-web-test";
        core.process_server_data(&format!("<LaunchURL src='{launch_url}'/>"))
            .expect("parse game LaunchURL response");
        core.poll_skill_trainer();
        let addressed = std::iter::from_fn(|| delta_rx.try_recv().ok()).find_map(|delta| {
            if let crate::core::remote::RemoteDelta::OpenUrl { client_id, url } = delta {
                Some((client_id, url))
            } else {
                None
            }
        });
        assert_eq!(
            addressed,
            Some((41, format!("https://www.play.net{launch_url}")))
        );
        connection.task.abort();
    }

    #[test]
    fn lich_feed_for_another_character_is_rejected_case_insensitively() {
        assert_eq!(
            character_identity_mismatch(Some("Aster"), Some("Briar")),
            Some(("Aster".to_string(), "Briar".to_string()))
        );
        assert_eq!(
            character_identity_mismatch(Some("Aster"), Some("aster")),
            None
        );
        assert_eq!(character_identity_mismatch(Some("Aster"), None), None);
    }

    #[tokio::test]
    async fn configured_lich_commands_wait_for_matching_app_identity() {
        let mut core = app();
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);

        process_connection_message(&mut core, &mut supervisor, ServerMessage::Connected);
        assert!(core.game_state.connected, "transport itself is connected");
        assert!(!supervisor.identity_confirmed);
        assert!(supervisor.command_connection().is_none());
        assert_eq!(
            command_rx.try_recv().as_deref(),
            Ok("_info character"),
            "only the identity probe may bypass verification"
        );

        dispatch_command(
            &mut core,
            supervisor.command_connection(),
            "look".to_string(),
        );
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));

        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text(r#"<app char="aster" game="GS" title="[GSIV: aster]"/>"#.into()),
        );
        assert!(supervisor.identity_confirmed);
        assert_eq!(core.game_state.character_name.as_deref(), Some("aster"));

        dispatch_command(
            &mut core,
            supervisor.command_connection(),
            "look".to_string(),
        );
        assert_eq!(command_rx.try_recv().as_deref(), Ok("look"));

        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn late_lich_attach_probes_once_and_accepts_character_sheet_identity() {
        let mut core = app();
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);
        supervisor.phase_started = Some(Instant::now() - Duration::from_secs(60));

        // A detachable client that joins an already-running Lich session gets
        // the cached gauges/compass but no <app char=...>. Vellum must ask for
        // identity through its one narrowly allowed pre-verification command.
        process_connection_message(&mut core, &mut supervisor, ServerMessage::Connected);
        assert_eq!(command_rx.try_recv().as_deref(), Ok("_info character"));
        assert!(
            supervisor.phase_started.unwrap().elapsed() < Duration::from_secs(1),
            "the identity response receives a fresh timeout window"
        );
        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text("<progressBar id='health' value='100' text='100 / 100'/>".into()),
        );
        assert!(
            connection_bootstrap_pending(&supervisor),
            "Lich's partial detachable replay must not disable identity recovery"
        );
        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text(
                "Name: Aster Moon Race: Half-Elf  Profession: Ranger (shown as: Hero)".into(),
            ),
        );
        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text("<prompt time='1'>&gt;</prompt>".into()),
        );

        assert!(supervisor.identity_confirmed);
        assert_eq!(core.game_state.character_name.as_deref(), Some("Aster"));
        assert!(supervisor.command_connection().is_some());

        // Further traffic must not emit duplicate probes.
        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text("Gender: Male    Age: 42    Level: 90".into()),
        );
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));
        supervisor.connection.take().unwrap().task.abort();
    }

    #[test]
    fn connection_reset_discards_uncommitted_identity_from_prior_transport() {
        let mut core = app();
        core.process_server_data(
            "Name: Aster Moon Race: Half-Elf  Profession: Ranger (shown as: Hero)",
        )
        .expect("buffer prior identity line");

        reset_connection_observations(&mut core);
        core.process_server_data("<prompt time='1'>&gt;</prompt>")
            .expect("commit next generation prompt");

        assert_eq!(core.game_state.character.observed_name(), None);
        let mut supervisor = lich_supervisor(Some("Aster"));
        assert!(matches!(
            check_character_identity(&mut supervisor, &mut core),
            IdentityCheck::Pending
        ));
        assert!(!supervisor.identity_confirmed);
    }

    #[tokio::test]
    async fn verified_lich_attach_quietly_bootstraps_unknown_identity_once() {
        let mut core = app();
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);

        process_connection_message(&mut core, &mut supervisor, ServerMessage::Connected);
        initialize_lich_session_if_ready(&mut core, &mut supervisor);
        assert_eq!(command_rx.try_recv().as_deref(), Ok("_info character"));
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));

        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text(r#"<app char="aster" game="GS" title="[GSIV: aster]"/>"#.into()),
        );
        initialize_lich_session_if_ready(&mut core, &mut supervisor);

        let commands: Vec<String> = std::iter::from_fn(|| command_rx.try_recv().ok()).collect();
        assert_eq!(
            commands,
            ["look"],
            "the pre-verification sheet probe is not duplicated; an unknown late-attach room is refreshed once"
        );

        initialize_lich_session_if_ready(&mut core, &mut supervisor);
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));
        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn verified_lich_attach_skips_identity_bootstrap_when_cache_is_complete() {
        let mut core = app();
        core.game_state
            .character
            .parse_line("Profession: Wizard   Level: 90");
        let mut supervisor = lich_supervisor(None);
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);

        process_connection_message(&mut core, &mut supervisor, ServerMessage::Connected);
        initialize_lich_session_if_ready(&mut core, &mut supervisor);

        let commands: Vec<String> = std::iter::from_fn(|| command_rx.try_recv().ok()).collect();
        assert!(
            commands
                .iter()
                .all(|command| command.trim() != "_info character"),
            "complete cached identity must not trigger a refresh: {commands:?}"
        );
        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn verified_lich_attach_does_not_look_when_live_room_is_known() {
        let mut core = app();
        core.nav_room_id = Some("1234".to_string());
        core.game_state
            .character
            .parse_line("Profession: Wizard   Level: 90");
        let mut supervisor = lich_supervisor(None);
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);

        process_connection_message(&mut core, &mut supervisor, ServerMessage::Connected);
        initialize_lich_session_if_ready(&mut core, &mut supervisor);

        let commands: Vec<String> = std::iter::from_fn(|| command_rx.try_recv().ok()).collect();
        assert!(
            commands.iter().all(|command| command.trim() != "look"),
            "a known live room must not trigger a duplicate LOOK: {commands:?}"
        );
        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn new_connection_generation_refreshes_a_room_known_only_by_prior_transport() {
        let mut core = app();
        core.nav_room_id = Some("1234".to_string());
        core.lich_room_id = Some("5678".to_string());
        core.room_subtitle = Some("Prior Room".to_string());
        reset_connection_observations(&mut core);

        let mut supervisor = lich_supervisor(None);
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);
        process_connection_message(&mut core, &mut supervisor, ServerMessage::Connected);
        initialize_lich_session_if_ready(&mut core, &mut supervisor);

        let commands: Vec<String> = std::iter::from_fn(|| command_rx.try_recv().ok()).collect();
        assert!(
            commands.iter().any(|command| command == "look"),
            "a new transport cannot trust the previous generation's room: {commands:?}"
        );
        supervisor.connection.take().unwrap().task.abort();
    }

    #[test]
    fn live_room_gate_uses_app_core_room_identity() {
        let mut core = app();
        assert!(!live_room_known(&core));

        // These legacy fields are not the authoritative room identity used by
        // Vellum's headless/web state path.
        core.game_state.room_id = Some("1234".to_string());
        core.game_state.room_name = Some("Legacy Room".to_string());
        assert!(!live_room_known(&core));

        core.lich_room_id = Some("5678".to_string());
        assert!(live_room_known(&core));
        core.lich_room_id = None;
        core.room_subtitle = Some("Known Room".to_string());
        assert!(live_room_known(&core));
    }

    #[tokio::test]
    async fn blocked_preverification_macro_cannot_queue_a_delayed_game_command() {
        let mut core = app();
        let mut requests = Vec::new();

        handle_remote_event(
            &mut core,
            None,
            false,
            crate::core::remote::RemoteEvent::Command {
                client_id: 1,
                text: "north\rs0.001\rsouth".to_string(),
            },
            &mut requests,
        );
        tokio::time::sleep(Duration::from_millis(5)).await;

        assert!(core.take_outbound().is_empty());
        assert!(requests.is_empty());
    }

    #[tokio::test]
    async fn disconnect_then_command_in_one_batch_never_reaches_the_socket() {
        let mut core = app();
        let (connection, mut command_rx, _server_tx) = test_connection();
        let mut requests = Vec::new();

        handle_remote_event(
            &mut core,
            Some(&connection),
            true,
            crate::core::remote::RemoteEvent::SessionDisconnect,
            &mut requests,
        );
        assert!(session_requests_block_commands(&requests));
        handle_remote_event(
            &mut core,
            None,
            false,
            crate::core::remote::RemoteEvent::Command {
                client_id: 1,
                text: "look".to_string(),
            },
            &mut requests,
        );

        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));
        connection.task.abort();
    }

    #[tokio::test]
    async fn mismatched_app_aborts_only_the_vellum_socket_without_sending_quit() {
        let mut core = app();
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);

        process_connection_message(&mut core, &mut supervisor, ServerMessage::Connected);
        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text(r#"<app char="Briar" game="GS" title="[GSIV: Briar]"/>"#.into()),
        );

        assert!(supervisor.connection.is_none());
        assert!(supervisor.user_disconnected);
        assert!(!core.game_state.connected);
        assert_eq!(core.game_state.character_name.as_deref(), Some("Briar"));
        assert_eq!(command_rx.try_recv().as_deref(), Ok("_info character"));
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Disconnected));
    }

    #[tokio::test]
    async fn mismatched_character_sheet_identity_never_sends_quit() {
        let mut core = app();
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);

        process_connection_message(&mut core, &mut supervisor, ServerMessage::Connected);
        assert_eq!(command_rx.try_recv().as_deref(), Ok("_info character"));
        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text(
                "Name: Briar Rose Race: Human  Profession: Empath (shown as: Healer)".into(),
            ),
        );
        process_connection_message(
            &mut core,
            &mut supervisor,
            ServerMessage::Text("<prompt time='1'>&gt;</prompt>".into()),
        );

        assert!(supervisor.connection.is_none());
        assert!(supervisor.user_disconnected);
        assert!(!core.game_state.connected);
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Disconnected));
    }

    #[tokio::test]
    async fn partial_lich_replay_stays_under_the_identity_watchdog() {
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, _command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);
        supervisor.first_text_seen = true;

        assert!(connection_bootstrap_pending(&supervisor));
        supervisor.identity_confirmed = true;
        assert!(!connection_bootstrap_pending(&supervisor));
        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn partial_lich_replay_times_out_even_after_receiving_text() {
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, _command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);
        supervisor.first_text_seen = true;
        supervisor.phase_started =
            Some(Instant::now() - CONNECTION_STALL_TIMEOUT - Duration::from_secs(1));

        assert!(connection_bootstrap_stalled(&supervisor));
        supervisor.identity_confirmed = true;
        assert!(!connection_bootstrap_stalled(&supervisor));
        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn terminal_identity_timeout_cannot_enter_reconnect_loop() {
        let mut core = app();
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, _command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);
        supervisor.terminal_connection_error = Some("identity timed out".to_string());
        let connection = supervisor.connection.take().unwrap();
        connection.task.abort();
        let result = connection.task.await;
        let mut quit_deadline = None;

        process_connection_end(&mut core, &mut supervisor, result, &mut quit_deadline);

        assert!(supervisor.connection.is_none());
        assert!(supervisor.reconnect_at.is_none());
        assert_eq!(supervisor.reconnect_attempt, 0);
        assert!(supervisor.terminal_connection_error.is_none());
    }

    #[tokio::test]
    async fn launcher_can_stop_identity_pending_socket_without_game_command() {
        let mut core = app();
        core.game_state.connected = true;
        let mut supervisor = lich_supervisor(Some("Aster"));
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);

        assert!(stop_inactive_session(&mut core, &mut supervisor));
        assert!(!core.running);
        assert!(supervisor.connection.is_none());
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Disconnected));
    }

    #[tokio::test]
    async fn launcher_cannot_stop_verified_connection_without_exit_logout() {
        let mut core = app();
        core.game_state.connected = true;
        let mut supervisor = lich_supervisor(Some("Aster"));
        supervisor.identity_confirmed = true;
        let (connection, mut command_rx, _server_tx) = test_connection();
        supervisor.connection = Some(connection);

        assert!(!stop_inactive_session(&mut core, &mut supervisor));
        assert!(core.running);
        assert!(supervisor.connection.is_some());
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));
        supervisor.connection.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn a_replacement_connection_cannot_receive_the_old_generations_queue() {
        let (old_connection, _old_commands, old_server_tx) = test_connection();
        old_server_tx
            .send(ServerMessage::Text("old generation".to_string()))
            .await
            .unwrap();

        let (mut current, _current_commands, current_server_tx) = test_connection();
        old_connection.task.abort();
        drop(old_connection);

        assert!(
            tokio::time::timeout(Duration::from_millis(10), current.next_event())
                .await
                .is_err(),
            "the old connection's queued text must not appear on the new receiver"
        );

        current_server_tx
            .send(ServerMessage::Text("current generation".to_string()))
            .await
            .unwrap();
        match current.next_event().await {
            ConnectionEvent::Message(ServerMessage::Text(text)) => {
                assert_eq!(text, "current generation")
            }
            _ => panic!("expected current-generation text"),
        }
        current.task.abort();
    }

    fn launcher_owned_lich_identity(
        character: &str,
        host: &str,
        port: u16,
    ) -> crate::core::session_registry::SessionLaunchIdentity {
        crate::core::session_registry::SessionLaunchIdentity {
            profile: character.to_string(),
            character: character.to_string(),
            connection: crate::core::session_registry::SessionConnectionIdentity::Lich {
                host: crate::core::session_registry::normalize_host(host),
                port,
            },
        }
    }

    fn inline_lich_connect(character: &str, host: &str, port: u16) -> SessionRequest {
        SessionRequest::Connect {
            profile: None,
            account: None,
            password: None,
            character: Some(character.to_string()),
            game: None,
            save_password: false,
            profile_name: None,
            lich_host: Some(host.to_string()),
            lich_port: Some(port),
            custom_launch: None,
        }
    }

    #[test]
    fn launcher_owned_connect_accepts_only_its_startup_character_and_connection() {
        let startup = launcher_owned_lich_identity("Calvix", "127.0.0.1", 8000);

        assert!(owned_connect_matches_startup(
            &inline_lich_connect("calvix", "localhost", 8000),
            &startup,
        )
        .unwrap());
        assert!(!owned_connect_matches_startup(
            &inline_lich_connect("Rabki", "localhost", 8000),
            &startup,
        )
        .unwrap());
        assert!(!owned_connect_matches_startup(
            &inline_lich_connect("Calvix", "localhost", 8001),
            &startup,
        )
        .unwrap());
    }

    #[test]
    fn launcher_owned_connect_rechecks_the_resolved_target() {
        use crate::core::session_registry::{SessionConnectionIdentity, SessionLaunchIdentity};

        let lich_startup = launcher_owned_lich_identity("Calvix", "127.0.0.1", 8000);
        let matching_lich = ResolvedConnect::Lich {
            target: LichTarget {
                host: "localhost".to_string(),
                port: 8000,
            },
            character: Some("calvix".to_string()),
            custom_launch: None,
        };
        let changed_lich = ResolvedConnect::Lich {
            target: LichTarget {
                host: "localhost".to_string(),
                port: 8001,
            },
            character: Some("Calvix".to_string()),
            custom_launch: None,
        };
        assert!(owned_resolved_connect_matches_startup(
            &matching_lich,
            &lich_startup
        ));
        assert!(!owned_resolved_connect_matches_startup(
            &changed_lich,
            &lich_startup
        ));

        let direct_startup = SessionLaunchIdentity {
            profile: "Calvix direct".to_string(),
            character: "Calvix".to_string(),
            connection: SessionConnectionIdentity::Direct {
                game: "prime".to_string(),
                account: "calx".to_string(),
            },
        };
        let matching_direct = ResolvedConnect::Direct(DirectConnectConfig {
            account: "CALX".to_string(),
            password: "not-used".to_string(),
            character: "calvix".to_string(),
            game_code: "GS3".to_string(),
            data_dir: std::path::PathBuf::new(),
        });
        assert!(owned_resolved_connect_matches_startup(
            &matching_direct,
            &direct_startup
        ));
    }

    #[test]
    fn launcher_owned_dot_launch_accepts_only_its_startup_character_and_connection() {
        let startup = launcher_owned_lich_identity("Calvix", "127.0.0.1", 8000);
        let same = crate::launcher::config::ResolvedLaunch {
            program: "ruby".to_string(),
            args: Vec::new(),
            attach_host: "localhost".to_string(),
            attach_port: 8000,
        };
        let other_port = crate::launcher::config::ResolvedLaunch {
            attach_port: 8001,
            ..same.clone()
        };

        assert!(owned_launch_matches_startup("calvix", &same, &startup));
        assert!(!owned_launch_matches_startup("Rabki", &same, &startup));
        assert!(!owned_launch_matches_startup(
            "Calvix",
            &other_port,
            &startup
        ));
    }

    #[tokio::test]
    async fn identity_pending_allows_local_exit_without_sending_or_queueing_game_output() {
        let mut core = app();
        let (connection, mut command_rx, _server_tx) = test_connection();
        let mut requests = Vec::new();

        handle_remote_event(
            &mut core,
            Some(&connection),
            false,
            crate::core::remote::RemoteEvent::Command {
                client_id: 1,
                text: ".exit".to_string(),
            },
            &mut requests,
        );

        assert!(!core.running);
        assert_eq!(command_rx.try_recv(), Err(TryRecvError::Empty));
        assert!(core.take_outbound().is_empty());
        assert!(requests.is_empty());
        connection.task.abort();
    }

    #[test]
    fn identity_pending_blocks_skill_apply_and_webui_handshake_output() {
        let mut core = app();
        core.ui_state.skill_trainer.data = Some(crate::data::skill_trainer::SkillGoals::default());
        let mut requests = Vec::new();

        handle_remote_event(
            &mut core,
            None,
            false,
            crate::core::remote::RemoteEvent::SkillTrainerApply,
            &mut requests,
        );
        handle_remote_event(
            &mut core,
            None,
            false,
            crate::core::remote::RemoteEvent::WebUiSubscribe {
                page: "bigshot".to_string(),
            },
            &mut requests,
        );

        assert_eq!(
            core.ui_state.skill_trainer.status,
            crate::data::skill_trainer::TrainerStatus::Idle
        );
        assert!(core.take_webui_pending_raw().is_empty());
        assert!(requests.is_empty());
    }

    /// `.reconnect` from the phone must ask the supervisor to reconnect — the
    /// old `Ui(_)` wildcard wrongly answered "needs the desktop client" from
    /// the very file that owns the reconnect supervisor.
    #[test]
    fn reconnect_action_requests_reconnect() {
        let mut core = app();
        assert!(matches!(
            dispatch_ui_action(&mut core, UiAction::Reconnect),
            DispatchResult::Reconnect
        ));
    }

    /// `push_dispatch_request` maps Reconnect onto a Reconnect session request.
    #[test]
    fn reconnect_result_becomes_session_request() {
        let mut reqs = Vec::new();
        push_dispatch_request(DispatchResult::Reconnect, &mut reqs);
        assert_eq!(reqs.len(), 1);
        assert!(matches!(reqs[0], SessionRequest::Reconnect));
    }

    /// The touch-wheel editor exists on the phone, so `.touchwheel` must not be
    /// a reconnect/quit — it's a benign local notice (DispatchResult::None).
    #[test]
    fn touch_wheel_editor_is_a_local_notice_not_reconnect_or_quit() {
        let mut core = app();
        assert!(matches!(
            dispatch_ui_action(&mut core, UiAction::TouchWheelEditor),
            DispatchResult::None
        ));
    }

    /// UI-pack export/import and perf dump are handled locally (None), not
    /// bounced to the desktop.
    #[test]
    fn supported_actions_are_handled_locally() {
        let mut core = app();
        assert!(matches!(
            dispatch_ui_action(&mut core, UiAction::PerformanceDump),
            DispatchResult::None
        ));
        assert!(matches!(
            dispatch_ui_action(&mut core, UiAction::UiExport(vec![])),
            DispatchResult::None
        ));
    }

    /// The outbound sentinel filter now blocks `action:` (the drift the audit
    /// found — the GUI filtered it, the headless copy did not).
    #[test]
    fn should_send_to_network_blocks_ui_sentinels_including_action() {
        assert!(!should_send_to_network(""));
        assert!(!should_send_to_network("__internal"));
        assert!(!should_send_to_network("menu:foo"));
        assert!(!should_send_to_network("action:bar"));
        assert!(should_send_to_network("say hello"));
        assert!(should_send_to_network("north"));
    }
}
