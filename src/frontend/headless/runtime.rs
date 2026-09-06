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

/// Build a loopback browser URL only after the web sidecar has published the
/// port it actually bound. An unpinned sidecar may walk above its configured
/// base port, so formatting the configured value is not authoritative.
fn local_web_client_url(endpoint: &RemoteLaunchEndpoint, route: &str) -> String {
    format!(
        "http://127.0.0.1:{}/{route}#token={}",
        endpoint.bound_port(),
        endpoint.token()
    )
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

fn lich_session_configured(login_key: Option<&str>, launch: super::HeadlessLaunchOptions) -> bool {
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
        lich_host: Option<String>,
        lich_port: Option<u16>,
        custom_launch: Option<String>,
    },
    Disconnect,
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
    /// Display fields for session status pushes.
    character: Option<String>,
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

    fn spawn(&mut self, app_core: &AppCore, server_tx: mpsc::Sender<ServerMessage>) {
        self.saw_input_since_connect = false;
        self.phase_started = Some(Instant::now());
        self.first_text_seen = false;
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
            // Set from the live AppCore flag by flush_state's session overlay;
            // this constructor doesn't know the connection mode, so default
            // false and let the sink overlay the real value.
            webui_available: false,
        }
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
        lich_configured: lich_session_configured(login_key.as_deref(), launch),
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
                let _ = conn
                    .command_tx
                    .send("_spell _spell_update_links\n".to_string());
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
                    if matches!(msg, ServerMessage::Text(_)) {
                        supervisor.first_text_seen = true;
                    }
                    let newly_connected = handle_server_message(&mut app_core, msg);
                    if newly_connected {
                        supervisor.reconnect_attempt = 0;
                        // The game feed never carries the character name;
                        // the supervisor knows it from the login. Write it
                        // into game state so remote clients (top-bar title,
                        // hello payload) see it.
                        if app_core.game_state.character_name.is_none() {
                            app_core.game_state.character_name = supervisor.character.clone();
                        }
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
                            // Surface the real failure in the game window: a
                            // phone user has no log access, and "refused" vs
                            // "reset mid-stream" points at opposite culprits.
                            app_core.add_system_message(&format!("Connection error: {e:#}"));
                            error_text = Some(format!("{e:#}"));
                            !supervisor.can_reconnect()
                        }
                    }
                    Err(join_err) if join_err.is_cancelled() => {
                        // Watchdog or teardown aborted the task; the
                        // scheduler below decides what happens next.
                        !supervisor.can_reconnect()
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
                            "Session looked idle - not reconnecting. Log in from the app to continue.",
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
            // Stall watchdog: a connection that has produced no game text
            // within the window is stuck (auth server hang, silent socket).
            // Tear it down; the completion arm schedules the retry. Once
            // text flows this branch goes dormant (no periodic wakeups).
            _ = tokio::time::sleep(Duration::from_secs(5)),
                if supervisor.connection.is_some() && !supervisor.first_text_seen => {
                let stalled = supervisor
                    .phase_started
                    .is_some_and(|at| at.elapsed() > Duration::from_secs(45));
                if stalled {
                    tracing::warn!("No game data within 45s of starting the connection; recycling");
                    app_core.add_system_message(
                        "Login is not responding - retrying...",
                    );
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
            if matches!(msg, ServerMessage::Text(_)) {
                supervisor.first_text_seen = true;
            }
            let newly_connected = handle_server_message(&mut app_core, msg);
            if newly_connected {
                supervisor.reconnect_attempt = 0;
                if app_core.game_state.character_name.is_none() {
                    app_core.game_state.character_name = supervisor.character.clone();
                }
                supervisor.character = app_core
                    .game_state
                    .character_name
                    .clone()
                    .or(supervisor.character.take());
                app_core.set_remote_session_state(supervisor.status(SessionState::Connected));
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
        for command in app_core.take_outbound() {
            match app_core.send_command(command) {
                Ok(crate::data::CommandOutcome::Game(out)) => {
                    if let Some(conn) = supervisor.connection.as_ref() {
                        let _ = conn.command_tx.send(out);
                    }
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
                Ok(_) => {}
                Err(e) => tracing::warn!("vellumCmd failed: {e}"),
            }
        }

        // Lich WebUI tick: drain the bridge (fans renders to phone clients),
        // send any queued `;ui handshake` to the game, and start the bridge
        // once its reply arrives. Only meaningful on a Lich-attached session
        // (webui_available); a direct eAccess connection has no Lich.
        app_core.pump_webui();
        for raw in app_core.take_webui_pending_raw() {
            if let Some(conn) = supervisor.connection.as_ref() {
                let _ = conn.command_tx.send(format!("{raw}\n"));
            }
        }
        if let Some(handshake) = app_core.take_webui_handshake() {
            app_core.start_webui(&tokio::runtime::Handle::current(), &handshake);
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
                        supervisor.spawn(&app_core, server_tx.clone());
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
                    match resolve_connect(&connect) {
                        Ok(resolved) => {
                            let state = match resolved {
                                ResolvedConnect::Direct(cfg) => {
                                    supervisor.character = Some(cfg.character.clone());
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
                            supervisor.spawn(&app_core, server_tx.clone());
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
                        let res = crate::launcher::flow::launch(&config, &character, trust, |p| {
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
                            supervisor.spawn(&app_core, server_tx.clone());
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
    use crate::data::{CommandOutcome, UiAction};
    let command = command.trim_end().to_string();
    if command.is_empty() {
        return DispatchResult::None;
    }
    match app_core.send_command(command) {
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
                    let _ = conn.command_tx.send(outbound);
                    if is_quit {
                        DispatchResult::Quit
                    } else {
                        DispatchResult::None
                    }
                }
                None => {
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
    event: crate::core::remote::RemoteEvent,
    session_requests: &mut Vec<SessionRequest>,
) -> bool {
    use crate::core::remote::RemoteEvent;
    match event {
        RemoteEvent::Command(text) => {
            tracing::debug!("remote command: '{}'", text);
            push_dispatch_request(
                dispatch_command(app_core, connection, text),
                session_requests,
            );
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
                    app_core
                        .perf_stats
                        .record_bytes_sent((cmd.len() + 1) as u64);
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
            app_core.webui_unsubscribe(&page);
            true
        }
        RemoteEvent::WebUiEvent { page, cid, value } => {
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
            // Open: if a page is already loaded, just re-mirror it; otherwise
            // fetch a fresh one (arms the trainer + sends `goals` to the game).
            if app_core.ui_state.skill_trainer.data.is_some() {
                app_core.ui_state.skill_trainer.open = true;
                app_core.push_skill_trainer_remote();
            } else {
                let cmd = app_core.skill_trainer_reload_command();
                push_dispatch_request(dispatch_command(app_core, connection, cmd), session_requests);
                // Loading state pushes immediately; the loaded page follows
                // via poll_skill_trainer once the worker parses it.
                app_core.push_skill_trainer_remote();
            }
            true
        }
        RemoteEvent::SkillTrainerReload => {
            let cmd = app_core.skill_trainer_reload_command();
            push_dispatch_request(dispatch_command(app_core, connection, cmd), session_requests);
            app_core.push_skill_trainer_remote();
            true
        }
        RemoteEvent::SkillTrainerStep { id, n, raise } => {
            app_core.skill_trainer_step(id, n, raise);
            app_core.push_skill_trainer_remote();
            true
        }
        RemoteEvent::SkillTrainerApply => {
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
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::data::UiAction;

    fn app() -> AppCore {
        AppCore::new(Config::default()).expect("AppCore")
    }

    #[test]
    fn local_web_url_requires_the_actual_port_and_preserves_the_route() {
        let endpoint = RemoteLaunchEndpoint::new(8041, "abc".to_string());
        assert_eq!(
            local_web_client_url(&endpoint, "despana"),
            "http://127.0.0.1:8041/despana#token=abc"
        );
        assert_eq!(
            local_web_client_url(&endpoint, "play"),
            "http://127.0.0.1:8041/play#token=abc"
        );
    }

    #[test]
    fn ordinary_headless_does_not_auto_attach_lich_or_open_a_browser() {
        let launch = super::super::HeadlessLaunchOptions::default();

        assert!(!lich_session_configured(None, launch));
        assert_eq!(launch.web_client, None);
        assert!(lich_session_configured(Some("one-shot-key"), launch));
    }

    #[test]
    fn launcher_web_client_auto_attach_does_not_need_a_fake_login_key() {
        let launch = super::super::HeadlessLaunchOptions {
            auto_connect_lich: true,
            web_client: Some(crate::config::profiles::LaunchWebClient::Despana),
        };

        assert!(lich_session_configured(None, launch));
    }

    #[test]
    fn despana_open_uses_the_authoritative_endpoint_and_failure_is_nonfatal() {
        let endpoint = RemoteLaunchEndpoint::new(8057, "secret".to_string());
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
