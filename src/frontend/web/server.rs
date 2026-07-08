//! Embedded axum server: serves the phone client (static assets) over HTTP
//! and streams game state over `/ws`.
//!
//! The server task owns only channel ends (`RemoteServerHandles`) — it
//! never touches `AppCore`. Each WebSocket client gets: `hello`, a full
//! `snapshot` (latest state + recent scrollback from the shared ring),
//! then live deltas from the broadcast channel. A client that lags behind
//! the broadcast capacity is re-synced with a fresh snapshot.

use std::future::IntoFuture;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use tokio::sync::broadcast;

use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::WebConfig;
use crate::core::remote::{RemoteDelta, RemoteEvent, RemoteServerHandles};
use crate::data::remote_buffer::RemoteLine;

use super::protocol::{self, ClientMessage, SnapshotMode};

/// Scrollback lines per stream included in a connect-time snapshot.
const SNAPSHOT_LINES_PER_STREAM: usize = 300;

/// How long to wait for the client's `resume` before sending a full
/// snapshot anyway.
const RESUME_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Per-connection id, used to route menu responses to the client whose
/// link tap requested them.
static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(1);

struct WebState {
    handles: RemoteServerHandles,
    /// Pairing token every WS connection must present first.
    auth_token: String,
    /// Timestamps of recent auth failures, for throttling.
    auth_failures: std::sync::Mutex<Vec<std::time::Instant>>,
}

/// After this many failures inside AUTH_WINDOW, reject connections until
/// the window drains.
const AUTH_MAX_FAILURES: usize = 5;
const AUTH_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
/// How long a client gets to present its token.
const AUTH_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

impl WebState {
    fn auth_locked_out(&self) -> bool {
        let mut failures = self.auth_failures.lock().expect("auth lock poisoned");
        let now = std::time::Instant::now();
        failures.retain(|t| now.duration_since(*t) < AUTH_WINDOW);
        failures.len() >= AUTH_MAX_FAILURES
    }

    fn record_auth_failure(&self) {
        self.auth_failures
            .lock()
            .expect("auth lock poisoned")
            .push(std::time::Instant::now());
    }
}

/// How many ports above the base an unpinned instance will try.
const PORT_WALK_RANGE: u16 = 20;

/// Bind and serve until the process exits. Runs as a detached tokio task.
///
/// Unpinned: tries `config.port` and walks upward (multiple characters
/// launch without config). Pinned: binds exactly `config.port` or fails
/// loudly via a Notice event — never silently takes a neighboring port,
/// so a per-character /play bookmark stays trustworthy.
pub async fn serve(
    config: WebConfig,
    handles: RemoteServerHandles,
    session_label: String,
) -> Result<()> {
    let mut listener = None;
    let mut bound_port = config.port;
    let last = if config.pinned {
        config.port
    } else {
        config.port.saturating_add(PORT_WALK_RANGE)
    };
    for port in config.port..=last {
        match tokio::net::TcpListener::bind((config.bind.as_str(), port)).await {
            Ok(l) => {
                listener = Some(l);
                bound_port = port;
                break;
            }
            Err(e) => tracing::debug!("port {} unavailable: {}", port, e),
        }
    }
    let Some(listener) = listener else {
        let message = if config.pinned {
            format!(
                "Web server disabled: pinned port {} is taken (pinned instances never take a neighboring port)",
                config.port
            )
        } else {
            format!(
                "Web server disabled: no free port in {}-{}",
                config.port, last
            )
        };
        tracing::error!("{message}");
        let _ = handles.event_tx.send(RemoteEvent::Notice(message.clone()));
        anyhow::bail!(message);
    };

    tracing::info!(
        "web server listening on http://{}:{}",
        config.bind,
        bound_port
    );
    if bound_port != config.port {
        let _ = handles.event_tx.send(RemoteEvent::Notice(format!(
            "Web server on port {} (base {} was taken)",
            bound_port, config.port
        )));
    }

    // Session registry entry: one file per instance so the dashboard can
    // list sessions by character. Best-effort; the dashboard also
    // health-checks each port, so a stale entry only costs a hidden card.
    registry::write_entry(bound_port, &session_label);
    let _ = handles.bound_port.set(bound_port);

    let auth_token = match crate::config::Config::load_or_create_web_token() {
        Ok(token) => token,
        Err(e) => {
            let message = format!("Web server disabled: pairing token unavailable ({e:#})");
            tracing::error!("{message}");
            let _ = handles.event_tx.send(RemoteEvent::Notice(message.clone()));
            anyhow::bail!(message);
        }
    };

    serve_listener_with_token(listener, handles, auth_token).await
}

/// Session registry: files in ~/.vellum-fe/web-sessions/, one per running
/// instance, keyed by pid.
pub mod registry {
    use serde::{Deserialize, Serialize};
    use std::fs;
    use std::path::PathBuf;

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct SessionEntry {
        pub character: String,
        pub port: u16,
        pub pid: u32,
        pub started_at: String,
    }

    pub fn dir() -> Option<PathBuf> {
        let dir = crate::config::Config::base_dir().ok()?.join("web-sessions");
        fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    fn entry_path(pid: u32) -> Option<PathBuf> {
        Some(dir()?.join(format!("{pid}.json")))
    }

    pub fn write_entry(port: u16, character: &str) {
        let pid = std::process::id();
        let entry = SessionEntry {
            character: character.to_string(),
            port,
            pid,
            started_at: chrono::Utc::now().to_rfc3339(),
        };
        let Some(path) = entry_path(pid) else { return };
        if let Ok(json) = serde_json::to_string_pretty(&entry) {
            if let Err(e) = fs::write(&path, json) {
                tracing::warn!("failed to write session registry entry: {e}");
            }
        }
    }

    /// Remove this instance's entry (clean shutdown).
    pub fn remove_entry() {
        if let Some(path) = entry_path(std::process::id()) {
            let _ = fs::remove_file(path);
        }
    }

    /// All current entries. Also garbage-collects files whose pid is no
    /// longer running (crashed instances).
    pub fn list_and_gc() -> Vec<SessionEntry> {
        let Some(dir) = dir() else { return Vec::new() };
        let Ok(read) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        #[cfg(feature = "desktop")]
        let mut system = sysinfo::System::new();
        #[cfg(feature = "desktop")]
        system.refresh_processes();
        let mut entries = Vec::new();
        for file in read.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(entry) = serde_json::from_str::<SessionEntry>(&text) else {
                let _ = fs::remove_file(&path);
                continue;
            };
            #[cfg(feature = "desktop")]
            let alive = system
                .process(sysinfo::Pid::from_u32(entry.pid))
                .is_some();
            // Without process inspection (Android: single-process app), only
            // our own entry can be live; anything else is a stale leftover.
            #[cfg(not(feature = "desktop"))]
            let alive = entry.pid == std::process::id();
            if alive {
                entries.push(entry);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
        entries.sort_by(|a, b| a.character.cmp(&b.character));
        entries
    }
}

/// Serve on an already-bound listener with a fixed token (integration
/// tests bind port 0 and pass a known token).
///
/// Serving runs in a supervised loop: iOS marks every open socket defunct
/// when the app is suspended past its background grace window, so the
/// listener silently stops accepting and never recovers while the web
/// client retries against a dead port forever (the only way out used to
/// be force-closing the app). There is no reliable accept-side error to
/// react to, so a watchdog dials our own port and rebinds the listener
/// when it stops answering.
pub async fn serve_listener_with_token(
    listener: tokio::net::TcpListener,
    handles: RemoteServerHandles,
    auth_token: String,
) -> Result<()> {
    let state = Arc::new(WebState {
        handles,
        auth_token,
        auth_failures: std::sync::Mutex::new(Vec::new()),
    });
    let router = Router::new()
        .route("/", get(dashboard_html))
        .route("/play", get(index_html))
        .route("/sessions", get(sessions_json))
        .route("/app.js", get(app_js))
        .route("/app.css", get(app_css))
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(sw_js))
        .route("/icon.svg", get(icon_svg))
        .route("/health", get(health))
        .route("/status", get(status_json))
        .route("/sounds/{name}", get(sound_file))
        .route("/ws", get(ws_upgrade))
        .with_state(state);
    let addr = listener
        .local_addr()
        .context("web listener has no local address")?;
    let mut listener = Some(listener);
    loop {
        let current = match listener.take() {
            Some(l) => l,
            None => rebind(addr).await,
        };
        let serve_task = tokio::spawn(axum::serve(current, router.clone()).into_future());
        probe_until_unreachable(addr).await;
        // Abort only the accept loop; per-connection tasks are spawned
        // independently (and are dead anyway if the listener went defunct).
        serve_task.abort();
        let _ = serve_task.await;
        tracing::warn!("web listener on {addr} stopped answering; rebinding");
    }
}

/// Returns once the listener stops accepting connections. A single failed
/// dial could be transient, so a failure only counts when a confirming
/// dial fails too.
async fn probe_until_unreachable(addr: std::net::SocketAddr) {
    const PROBE_EVERY: std::time::Duration = std::time::Duration::from_secs(10);
    const CONFIRM_GAP: std::time::Duration = std::time::Duration::from_secs(1);
    loop {
        tokio::time::sleep(PROBE_EVERY).await;
        if probe(addr).await {
            continue;
        }
        tokio::time::sleep(CONFIRM_GAP).await;
        if !probe(addr).await {
            return;
        }
    }
}

/// One self-dial: can anything connect to the listener right now?
async fn probe(mut addr: std::net::SocketAddr) -> bool {
    if addr.ip().is_unspecified() {
        addr.set_ip(match addr.ip() {
            std::net::IpAddr::V4(_) => std::net::Ipv4Addr::LOCALHOST.into(),
            std::net::IpAddr::V6(_) => std::net::Ipv6Addr::LOCALHOST.into(),
        });
    }
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::net::TcpStream::connect(addr),
        )
        .await,
        Ok(Ok(_))
    )
}

/// Re-bind the same address, retrying until it works (right after resume
/// the defunct socket may not have released the port yet).
async fn rebind(addr: std::net::SocketAddr) -> tokio::net::TcpListener {
    loop {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => {
                tracing::info!("web listener rebound on {addr}");
                return l;
            }
            Err(e) => {
                tracing::warn!("rebinding web listener on {addr} failed: {e}; retrying");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
}

// no-cache: assets are embedded in the binary and change with every
// rebuild; a phone serving yesterday's cached app.js against today's
// protocol is much worse than re-fetching a few KB.
async fn index_html() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Html(include_str!("assets/index.html")),
    )
}

async fn app_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("assets/app.js"),
    )
}

async fn app_css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("assets/app.css"),
    )
}

async fn manifest() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/manifest+json"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("assets/manifest.webmanifest"),
    )
}

async fn sw_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("assets/sw.js"),
    )
}

async fn icon_svg() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "max-age=86400"),
        ],
        include_str!("assets/icon.svg"),
    )
}

async fn dashboard_html() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Html(include_str!("assets/dashboard.html")),
    )
}

/// Session list for the dashboard. Every instance serves the same list
/// (from the shared registry dir), so it's reachable via any live port.
async fn sessions_json() -> impl IntoResponse {
    let entries = registry::list_and_gc();
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string()),
    )
}

/// Health check. CORS-open so the dashboard (served from one port) can
/// probe sibling instances on other ports from the browser.
async fn health() -> impl IntoResponse {
    (
        [(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
        "ok",
    )
}

/// Serve a sound file from the shared sounds directory for client-side
/// playback (`RemoteDelta::Sound`). Token-gated like /status. The name is
/// a bare filename — anything path-like is rejected — and extension
/// resolution matches SoundPlayer::play_from_sounds_dir (a highlight may
/// reference "alert" meaning "alert.mp3").
async fn sound_file(
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;

    if params.get("token").map(String::as_str) != Some(state.auth_token.as_str()) {
        return (StatusCode::FORBIDDEN, [(header::CONTENT_TYPE, "text/plain")], Vec::new());
    }
    if name.contains(['/', '\\']) || name.contains("..") || name.is_empty() {
        return (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "text/plain")], Vec::new());
    }
    let Ok(sounds_dir) = crate::config::Config::sounds_dir() else {
        return (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain")], Vec::new());
    };

    let mut path = sounds_dir.join(&name);
    if !path.exists() {
        let mut found = false;
        for ext in ["mp3", "wav", "ogg", "flac"] {
            let candidate = sounds_dir.join(format!("{name}.{ext}"));
            if candidate.exists() {
                path = candidate;
                found = true;
                break;
            }
        }
        if !found {
            return (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain")], Vec::new());
        }
    }

    let content_type = match path.extension().and_then(|e| e.to_str()) {
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg") => "audio/ogg",
        Some("flac") => "audio/flac",
        _ => "application/octet-stream",
    };
    match std::fs::read(&path) {
        Ok(bytes) => (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], bytes),
        Err(_) => (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain")], Vec::new()),
    }
}

/// Session status for native shells (the Android foreground service polls
/// this to scope its wakelock and to self-stop when the session is idle
/// and the app was swiped away). Token-gated: session state and character
/// name shouldn't be readable by arbitrary local processes.
async fn status_json(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    if params.get("token").map(String::as_str) != Some(state.auth_token.as_str()) {
        return (
            axum::http::StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"forbidden"}"#.to_string(),
        );
    }
    let session = state.handles.state_rx.borrow().session.clone();
    (
        axum::http::StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&session).unwrap_or_else(|_| "{}".to_string()),
    )
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<Arc<WebState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_client(socket, state))
}

/// Snapshot data gathered under the buffer lock (no awaits while locked).
fn gather_snapshot(state: &WebState) -> (Vec<String>, Vec<RemoteLine>, u64) {
    let buffer = state
        .handles
        .buffer
        .lock()
        .expect("remote buffer lock poisoned");
    (
        buffer.stream_names(),
        buffer.snapshot_tail(SNAPSHOT_LINES_PER_STREAM),
        buffer.last_seq(),
    )
}

/// Build the snapshot reply for a `resume { seq }` request. Locks the
/// buffer briefly; never holds it across an await.
fn build_resume_reply(state: &WebState, resume_seq: u64) -> String {
    let buffer = state
        .handles
        .buffer
        .lock()
        .expect("remote buffer lock poisoned");
    let last_seq = buffer.last_seq();
    let (mode, lines) = if resume_seq == 0 {
        (SnapshotMode::Full, buffer.snapshot_tail(SNAPSHOT_LINES_PER_STREAM))
    } else {
        match buffer.lines_since(resume_seq) {
            Some(lines) => (SnapshotMode::Resume, lines),
            None => (SnapshotMode::Gap, buffer.snapshot_tail(SNAPSHOT_LINES_PER_STREAM)),
        }
    };
    drop(buffer);
    let game_state = state.handles.state_rx.borrow().clone();
    protocol::snapshot(&game_state, lines, mode, last_seq)
}

async fn send_snapshot(
    socket: &mut WebSocket,
    state: &WebState,
    mode: SnapshotMode,
) -> Result<(), axum::Error> {
    let (_, lines, last_seq) = gather_snapshot(state);
    let game_state = state.handles.state_rx.borrow().clone();
    let msg = protocol::snapshot(&game_state, lines, mode, last_seq);
    socket.send(Message::Text(msg.into())).await
}

/// Handle one parsed client message inside the main loop.
/// Returns false when the socket should close.
async fn handle_client_message(
    socket: &mut WebSocket,
    state: &WebState,
    client_id: u64,
    msg: ClientMessage,
) -> bool {
    match msg {
        // Already authenticated; a stray re-auth is harmless.
        ClientMessage::Auth { .. } => true,
        ClientMessage::Cmd { text } => {
            // Forward into the main loop; it runs the same path as local
            // input. Send fails only if the app is shutting down.
            state
                .handles
                .event_tx
                .send(RemoteEvent::Command(text))
                .is_ok()
        }
        ClientMessage::Resume { seq } => {
            let reply = build_resume_reply(state, seq);
            socket.send(Message::Text(reply.into())).await.is_ok()
        }
        ClientMessage::LinkTap {
            request_id,
            exist_id,
            noun,
            text,
            coord,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::LinkTap {
                client_id,
                request_id,
                exist_id,
                noun,
                text,
                coord,
            })
            .is_ok(),
        ClientMessage::Macro { id } => state
            .handles
            .event_tx
            .send(RemoteEvent::Macro { id })
            .is_ok(),
        ClientMessage::MacroSave {
            group,
            label,
            command,
            color,
            confirm,
            options,
            original,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::MacroSave {
                group,
                label,
                command,
                color,
                confirm,
                options,
                original,
            })
            .is_ok(),
        ClientMessage::MacroDelete { group, label } => state
            .handles
            .event_tx
            .send(RemoteEvent::MacroDelete { group, label })
            .is_ok(),
        ClientMessage::Connect {
            profile,
            account,
            password,
            character,
            game,
            save_password,
            profile_name,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::SessionConnect {
                profile,
                account,
                password,
                character,
                game,
                save_password,
                profile_name,
            })
            .is_ok(),
        ClientMessage::Disconnect => state
            .handles
            .event_tx
            .send(RemoteEvent::SessionDisconnect)
            .is_ok(),
        ClientMessage::ConfigGet { request_id, file } => state
            .handles
            .event_tx
            .send(RemoteEvent::ConfigGet {
                client_id,
                request_id,
                file,
            })
            .is_ok(),
        ClientMessage::ConfigPut {
            request_id,
            file,
            content,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::ConfigPut {
                client_id,
                request_id,
                file,
                content,
            })
            .is_ok(),
        ClientMessage::HighlightsGet { request_id, scope } => state
            .handles
            .event_tx
            .send(RemoteEvent::HighlightsGet {
                client_id,
                request_id,
                scope,
            })
            .is_ok(),
        ClientMessage::HighlightPut {
            request_id,
            scope,
            name,
            rule,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::HighlightPut {
                client_id,
                request_id,
                scope,
                name,
                rule,
            })
            .is_ok(),
        ClientMessage::ColorsGet { request_id, scope } => state
            .handles
            .event_tx
            .send(RemoteEvent::ColorsGet {
                client_id,
                request_id,
                scope,
            })
            .is_ok(),
        ClientMessage::ColorsPut {
            request_id,
            scope,
            colors,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::ColorsPut {
                client_id,
                request_id,
                scope,
                colors,
            })
            .is_ok(),
        ClientMessage::HighlightDelete {
            request_id,
            scope,
            name,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::HighlightDelete {
                client_id,
                request_id,
                scope,
                name,
            })
            .is_ok(),
        // Profile list/delete touch only launcher.toml via the config
        // layer — answered here without a round-trip through the app loop.
        ClientMessage::GetProfiles => {
            let reply = profiles_reply(state);
            socket.send(Message::Text(reply.into())).await.is_ok()
        }
        ClientMessage::DeleteProfile { name } => {
            delete_profile(&name);
            let reply = profiles_reply(state);
            socket.send(Message::Text(reply.into())).await.is_ok()
        }
    }
}

/// Direct-mode saved profiles serialized for the session screen. Lich
/// profiles are desktop-launcher concerns and are filtered out.
fn profiles_reply(state: &WebState) -> String {
    let list: Vec<protocol::ProfileEntry> = crate::config::profiles::LauncherStore::load()
        .map(|store| {
            store
                .profiles
                .iter()
                .filter(|p| p.mode == crate::config::profiles::LaunchMode::Direct)
                .map(|p| protocol::ProfileEntry {
                    name: p.name.clone(),
                    account_masked: protocol::mask_account(&p.account),
                    character: p.character.clone(),
                    game: p.game.clone(),
                    has_password: p.password_saved,
                })
                .collect()
        })
        .unwrap_or_default();
    let last_seq = state
        .handles
        .buffer
        .lock()
        .expect("remote buffer lock poisoned")
        .last_seq();
    protocol::profiles(&list, last_seq)
}

/// Remove a saved profile; drop its stored password too unless another
/// profile shares the account.
fn delete_profile(name: &str) {
    let Ok(mut store) = crate::config::profiles::LauncherStore::load() else {
        return;
    };
    let Some(removed) = store.remove(name) else {
        return;
    };
    if let Err(e) = store.save() {
        tracing::warn!("failed to save launcher.toml after delete: {e:#}");
        return;
    }
    if removed.password_saved && !store.account_password_in_use(&removed.account) {
        crate::config::profiles::delete_password(&removed.account);
    }
}

/// The pairing gate: the very first message must be `auth { token }`.
/// Wrong/missing token or an active lockout gets a `denied` message and
/// a closed socket. Returns true when the client may proceed.
async fn authenticate(socket: &mut WebSocket, state: &WebState) -> bool {
    // Read the first message even when locked out: closing with unread
    // bytes in the receive buffer RSTs the connection on Windows and the
    // client never sees the denied frame.
    let first = tokio::time::timeout(AUTH_WAIT, socket.recv()).await;
    if state.auth_locked_out() {
        tracing::warn!("web auth locked out; dropping connection");
        let _ = socket.send(Message::Text(protocol::denied().into())).await;
        return false;
    }
    let ok = matches!(
        first,
        Ok(Some(Ok(Message::Text(ref text))))
            if matches!(
                protocol::parse_client_message(text),
                Some(ClientMessage::Auth { ref token }) if *token == state.auth_token
            )
    );
    if !ok {
        state.record_auth_failure();
        tracing::warn!("web client failed pairing auth");
        let _ = socket.send(Message::Text(protocol::denied().into())).await;
    }
    ok
}

async fn handle_client(mut socket: WebSocket, state: Arc<WebState>) {
    if !authenticate(&mut socket, &state).await {
        return;
    }

    let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);

    // Subscribe BEFORE building any snapshot so no delta can fall in the
    // gap. Deltas that overlap a snapshot are deduped client-side by seq.
    let mut delta_rx = state.handles.delta_tx.subscribe();

    let (streams, _, last_seq) = gather_snapshot(&state);
    let character = state.handles.state_rx.borrow().character.clone();
    let hello = protocol::hello(character, streams, state.handles.session.clone(), last_seq);
    if socket.send(Message::Text(hello.into())).await.is_err() {
        return;
    }

    // The client answers hello with `resume { seq }` (0 = fresh). Fall
    // back to a full snapshot for clients that never send one.
    let first = tokio::time::timeout(RESUME_WAIT, socket.recv()).await;
    match first {
        Ok(None) | Ok(Some(Err(_))) | Ok(Some(Ok(Message::Close(_)))) => return,
        Ok(Some(Ok(Message::Text(text)))) => {
            match protocol::parse_client_message(&text) {
                Some(msg) => {
                    if !handle_client_message(&mut socket, &state, client_id, msg).await {
                        return;
                    }
                }
                None => {
                    if send_snapshot(&mut socket, &state, SnapshotMode::Full).await.is_err() {
                        return;
                    }
                }
            }
        }
        Ok(Some(Ok(_))) | Err(_) => {
            // Non-text frame or timeout: treat as a fresh client.
            if send_snapshot(&mut socket, &state, SnapshotMode::Full).await.is_err() {
                return;
            }
        }
    }

    // Macro definitions follow the snapshot; updates arrive as deltas.
    {
        let macros = state.handles.macros_rx.borrow().clone();
        let (_, _, last_seq) = gather_snapshot(&state);
        let msg = protocol::macros(&macros, last_seq);
        if socket.send(Message::Text(msg.into())).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            delta = delta_rx.recv() => match delta {
                Ok(d) => {
                    // Menus and config/highlight replies are addressed:
                    // only the requesting client's task forwards them.
                    if let RemoteDelta::Menu { client_id: target, .. }
                    | RemoteDelta::ConfigFile { client_id: target, .. }
                    | RemoteDelta::Highlights { client_id: target, .. }
                    | RemoteDelta::Colors { client_id: target, .. } = &d
                    {
                        if *target != client_id {
                            continue;
                        }
                    }
                    let last_seq = state
                        .handles
                        .buffer
                        .lock()
                        .expect("remote buffer lock poisoned")
                        .last_seq();
                    let msg = protocol::delta(&d, last_seq);
                    if socket.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::debug!("web client lagged {missed} deltas; re-syncing");
                    // Gap mode: the client keeps its pane, shows a missed-
                    // output marker, and seq-dedupes the overlap.
                    if send_snapshot(&mut socket, &state, SnapshotMode::Gap).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Text(text))) => {
                    if let Some(msg) = protocol::parse_client_message(&text) {
                        if !handle_client_message(&mut socket, &state, client_id, msg).await {
                            break;
                        }
                    }
                }
                Some(Ok(_)) => {}
            },
        }
    }
}
