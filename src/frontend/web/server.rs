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
use axum::http::{header, HeaderMap};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::broadcast;

use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::WebConfig;
use crate::core::classic_maps::ClassicMapCatalog;
use crate::core::remote::{RemoteDelta, RemoteEvent, RemoteLaunchEndpoint, RemoteServerHandles};
use crate::data::remote_buffer::RemoteLine;

use super::protocol::{self, ClientMessage, SnapshotMode};

mod despana;

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
    /// Classic map filesystem authority for this game session only.
    classic_maps: Arc<ClassicMapCatalog>,
    /// Pairing token every WS connection must present first.
    auth_token: String,
    /// Timestamps of recent auth failures, for throttling.
    auth_failures: std::sync::Mutex<Vec<std::time::Instant>>,
    /// Preserve request order across tabs/reloads so an older workspace write
    /// cannot finish after a newer one.
    workspace_write_lock: tokio::sync::Mutex<()>,
}

/// After this many failures inside AUTH_WINDOW, reject connections until
/// the window drains.
const AUTH_MAX_FAILURES: usize = 5;
const AUTH_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
/// How long a client gets to present its token.
const AUTH_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Compare a presented token against the expected one without leaking the
/// mismatch position through timing. Length still leaks; tokens are fixed
/// length so that reveals nothing.
fn token_matches(presented: &str, expected: &str) -> bool {
    let a = presented.as_bytes();
    let b = expected.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

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

/// Optional behavior and session-owned resources for a web server.
#[derive(Clone)]
pub struct ServeOptions {
    pub status_only: bool,
    pub classic_maps: Arc<ClassicMapCatalog>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            status_only: false,
            classic_maps: Arc::new(ClassicMapCatalog::new()),
        }
    }
}

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
    options: ServeOptions,
) -> Result<()> {
    let mut listener = None;
    let mut bound_port = config.port;
    let last = if config.pinned {
        config.port
    } else {
        config.port.saturating_add(PORT_WALK_RANGE)
    };
    for port in config.port..=last {
        match tokio::net::TcpListener::bind((config.effective_bind(), port)).await {
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
        "web server listening on http://{}:{} ({})",
        config.effective_bind(),
        bound_port,
        if options.status_only {
            "multi-account status only"
        } else {
            "phone client + status"
        }
    );
    // Only surface the port walk to a user who is trying to reach a URL. In
    // status-only mode the port is an implementation detail -- siblings find
    // each other through the registry, not by typing it.
    if bound_port != config.port && !options.status_only {
        let _ = handles.event_tx.send(RemoteEvent::Notice(format!(
            "Web server on port {} (base {} was taken)",
            bound_port, config.port
        )));
    }

    // Load once here: this exact value configures authentication and is then
    // published with the bound port. Callers must not race it with their own
    // first-run token creation.
    let auth_token = match crate::config::Config::load_or_create_web_token() {
        Ok(token) => token,
        Err(e) => {
            let message = format!("Web server disabled: pairing token unavailable ({e:#})");
            tracing::error!("{message}");
            let _ = handles.event_tx.send(RemoteEvent::Notice(message.clone()));
            anyhow::bail!(message);
        }
    };

    // Publish readiness only after every prerequisite for an authenticated
    // client is available. Headless startup uses this value to advertise a
    // launchable URL; setting it before token creation could briefly surface
    // a dead endpoint when token setup fails.
    //
    // Registry publication is part of startup correctness: advertising a
    // listener that the launcher cannot discover would permit duplicate
    // runtimes for the same immutable game endpoint.
    let control_host =
        crate::core::session_registry::control_host_for_bind(config.effective_bind());
    let _registration =
        match registry::register(bound_port, &control_host, &session_label, &handles.session) {
            Ok(registration) => registration,
            Err(error) => {
                let error = error.context("Could not publish the web session registry entry");
                tracing::error!("{error:#}");
                let _ = handles.event_tx.send(RemoteEvent::Notice(format!(
                    "Web server disabled: {error:#}"
                )));
                return Err(error);
            }
        };
    handles
        .launch_endpoint_tx
        .send_replace(Some(RemoteLaunchEndpoint::new(
            control_host,
            bound_port,
            auth_token.clone(),
        )));

    serve_listener(listener, handles, auth_token, options).await
}

/// Session registry, re-exported from core so existing call sites keep
/// working. The implementation moved to `core::session_registry` because the
/// launcher lifecycle policy and multi-account hub need it, and core cannot
/// import from `frontend/`.
pub use crate::core::session_registry as registry;

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
pub async fn serve_listener(
    listener: tokio::net::TcpListener,
    handles: RemoteServerHandles,
    auth_token: String,
    options: ServeOptions,
) -> Result<()> {
    let ServeOptions {
        status_only,
        classic_maps,
    } = options;
    let state = Arc::new(WebState {
        handles,
        classic_maps,
        auth_token,
        auth_failures: std::sync::Mutex::new(Vec::new()),
        workspace_write_lock: tokio::sync::Mutex::new(()),
    });
    let router = if status_only {
        Router::new()
            .route("/health", get(health))
            .route("/api/v1/session/exit-logout", post(exit_logout_session))
            .route("/ws", get(ws_upgrade))
            .with_state(state)
    } else {
        full_router(state)
    };
    serve_router(listener, router).await
}

fn full_router(state: Arc<WebState>) -> Router {
    Router::new()
        .route("/", get(dashboard_html))
        .route("/play", get(index_html))
        .route("/api/v1/maps/classic", get(classic_map_catalog))
        .route("/api/v1/maps/classic/{name}", get(classic_map_image))
        .route("/characters", get(characters_html))
        .route("/creatures", get(creatures_html))
        .route("/sessions", get(sessions_json))
        .route("/app.js", get(app_js))
        .route("/wheel-core.js", get(wheel_core_js))
        .route("/app.css", get(app_css))
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(sw_js))
        .route("/icon.svg", get(icon_svg))
        .route("/health", get(health))
        .route("/status", get(status_json))
        .route("/api/v1/session/stop", post(stop_session))
        .route("/api/v1/session/exit-logout", post(exit_logout_session))
        .route("/sounds/{name}", get(sound_file))
        .route("/emoji", get(emoji_list))
        .route("/emoji/{name}", get(emoji_file))
        .route("/image/{name}", get(inline_image_file))
        .route("/doll.json", get(doll_json))
        .route("/doll/image", get(doll_image))
        .route("/ws", get(ws_upgrade))
        .merge(despana::router())
        .with_state(state)
}

async fn serve_router(listener: tokio::net::TcpListener, router: Router) -> Result<()> {
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

/// List classic annotated maps discovered from the active local Lich install.
/// The browser receives display names and registry keys only, never paths.
async fn classic_map_catalog(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            "[]".to_string(),
        );
    }
    let maps = state.classic_maps.entries();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&maps).unwrap_or_else(|_| "[]".to_string()),
    )
}

/// Serve one classic map by a name already discovered in the trusted maps
/// directory. Registry lookup is the traversal guard; client paths are never
/// joined to the filesystem.
async fn classic_map_image(
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    }
    let Some(asset) = state.classic_maps.get(&name) else {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    };
    match tokio::fs::read(&asset.path).await {
        Ok(bytes) => (StatusCode::OK, [(header::CONTENT_TYPE, asset.mime)], bytes),
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        ),
    }
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

async fn wheel_core_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("assets/wheel-core.js"),
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

/// The multi-account status wall: one card per running session, grouped like
/// the GUI Characters window. Entirely client-side -- the page dials every
/// registered session's `/ws` in watch mode itself, so it works no matter
/// which instance serves it and needs no hub handle in the server.
async fn characters_html() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Html(include_str!("assets/characters.html")),
    )
}

/// The creature field: the room's hostiles as host-placed cards, drawn on
/// a canvas from `field` snapshots/deltas over `/ws` in watch mode. Tap to
/// target. Entirely client-side, like the characters wall.
async fn creatures_html() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Html(include_str!("assets/creatures.html")),
    )
}

/// Session list for the dashboard. Every instance serves the same list
/// (from the shared registry dir), so it's reachable via any live port.
/// List running VellumFE instances for the dashboard picker. Token-gated for
/// the same reason as /status: the registry carries character names, pids, and
/// ports, which shouldn't be readable by arbitrary local processes — and, with
/// a 0.0.0.0 web bind, by anything on the LAN. The dashboard forwards its
/// pairing token when it fetches this.
async fn sessions_json(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
        return (
            StatusCode::FORBIDDEN,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            r#"{"error":"forbidden"}"#.to_string(),
        );
    }
    let entries = registry::list_and_gc();
    (
        StatusCode::OK,
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
    ([(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")], "ok")
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

    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    }
    // ':' rejected too: on Windows, joining "c:name" replaces the whole
    // path prefix (drive-relative), escaping the sounds dir
    if name.contains(['/', '\\', ':']) || name.contains("..") || name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    }
    let Ok(sounds_dir) = crate::config::Config::sounds_dir() else {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
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
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/plain")],
                Vec::new(),
            );
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
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            bytes,
        ),
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        ),
    }
}

/// Is `name` a valid custom-emoji shortcode? Matches the registry's own
/// alphabet (alphanumeric + `_ + -`), so nothing path-like (`/`, `\`, `.`,
/// `:`) can pass and traversal is impossible before we even hit the lookup.
fn is_emoji_shortcode(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-')
}

/// List the installed custom-emoji shortcode names as a JSON array. Reserved
/// for a future phone-side picker; token-gated like the other private
/// endpoints.
async fn emoji_list(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            "[]".to_string(),
        );
    }
    let mut names: Vec<String> = crate::core::custom_emoji::all()
        .into_iter()
        .map(|e| e.name)
        .collect();
    names.sort();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string()),
    )
}

/// Serve a custom-emoji image (`~/.vellum-fe/emoji/<name>.<ext>`) so the
/// phone can render `:name:` shortcodes as inline `<img>`. Token-gated like
/// /sounds. The name must be a valid shortcode; the registry lookup then
/// resolves it to an on-disk path (its scan already constrained names to the
/// same alphabet, so there is no path-traversal surface). Unknown names 404.
async fn emoji_file(
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;

    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    }
    if !is_emoji_shortcode(&name) {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    }
    let Some(emoji) = crate::core::custom_emoji::get(&name) else {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    };
    match std::fs::read(&emoji.path) {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, emoji.format.mime())],
            bytes,
        ),
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        ),
    }
}

/// Serve one inline image (`<vellumImg src=..>` art) by name. Same gating as
/// [`emoji_file`]: pairing token, then the shortcode alphabet, then a
/// registry lookup that resolves the name to a path the scan itself
/// discovered — the client never supplies a path, so there is no traversal
/// surface. Unknown names 404.
async fn inline_image_file(
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;

    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    }
    if !is_emoji_shortcode(&name) {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    }
    let Some(image) = crate::core::inline_image::get(&name) else {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    };
    match std::fs::read(&image.path) {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, image.format.mime())],
            bytes,
        ),
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        ),
    }
}

/// Injury doll skin data for the status drawer: whether the active skin
/// ships doll art, resolved anchors, dot styling, and overlay coverage.
/// Token-gated like /status; the client falls back to its vector doll on
/// `base: false` (or any failure).
async fn doll_json(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    // CORS-open like /health: the /characters wall (served from one port)
    // fetches every session's doll metadata cross-port. Token-gated either
    // way, so the header only permits reads the token already allows.
    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
        return (
            StatusCode::FORBIDDEN,
            [
                (header::CONTENT_TYPE, "text/plain"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            String::new(),
        );
    }
    let payload = super::doll::active_payload();
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        serde_json::to_string(&payload).unwrap_or_else(|_| r#"{"base":false}"#.to_string()),
    )
}

/// Serve a doll image from the active skin: `?kind=base` or
/// `?kind=overlay&part=<protocol name>&level=<0-6>` (0 = the healthy
/// overlay). Paths come from the
/// server operator's own skin.toml (absolute paths are a manifest
/// feature), so the only gate needed is the pairing token.
async fn doll_image(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    }
    let kind = params.get("kind").map(String::as_str).unwrap_or("base");
    let part = params.get("part").map(String::as_str);
    let level = params.get("level").and_then(|l| l.parse::<u8>().ok());
    let variant = params.get("variant").map(String::as_str);
    let Some(path) = super::doll::image_path(kind, part, level, variant) else {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        );
    };
    let content_type = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    };
    match std::fs::read(&path) {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            bytes,
        ),
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            Vec::new(),
        ),
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
    if !params
        .get("token")
        .is_some_and(|t| token_matches(t, &state.auth_token))
    {
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

/// Authenticated, state-checked process shutdown. This asks the owning
/// runtime to stop itself; it never trusts a registry PID and never sends a
/// command to the game. A verified connected session must use Exit & Log Out,
/// but a stalled startup/reconnect must always remain recoverable.
async fn stop_session(headers: HeaderMap, State(state): State<Arc<WebState>>) -> impl IntoResponse {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if !token.is_some_and(|token| token_matches(token, &state.auth_token)) {
        return (axum::http::StatusCode::FORBIDDEN, "forbidden");
    }

    let expected_instance = headers
        .get("x-vellum-instance")
        .and_then(|value| value.to_str().ok());
    if expected_instance != Some(state.handles.session.as_str()) {
        return (axum::http::StatusCode::CONFLICT, "session instance changed");
    }

    let session = state.handles.state_rx.borrow().session.clone();
    if !session.session_control
        || !matches!(
            session.state,
            crate::core::remote::SessionState::Idle
                | crate::core::remote::SessionState::Authenticating
                | crate::core::remote::SessionState::Connecting
                | crate::core::remote::SessionState::Reconnecting
                | crate::core::remote::SessionState::Disconnected
        )
    {
        return (
            axum::http::StatusCode::CONFLICT,
            "a connected session must use Exit & Log Out",
        );
    }
    if state
        .handles
        .event_tx
        .send(RemoteEvent::SessionStop)
        .is_err()
    {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "session runtime is unavailable",
        );
    }
    (axum::http::StatusCode::ACCEPTED, "stopping")
}

/// Authenticated launcher handoff for a connected session. The caller must
/// name the exact runtime instance it discovered, so a stale registry entry
/// cannot log out a newer process that reused the same walked web port.
///
/// The runtime owns logout ordering and sends the ordinary game `quit`; this
/// endpoint only queues the existing orderly Exit & Log Out request.
async fn exit_logout_session(
    headers: HeaderMap,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if !token.is_some_and(|token| token_matches(token, &state.auth_token)) {
        return (axum::http::StatusCode::FORBIDDEN, "forbidden");
    }

    let expected_instance = headers
        .get("x-vellum-instance")
        .and_then(|value| value.to_str().ok());
    if expected_instance != Some(state.handles.session.as_str()) {
        return (axum::http::StatusCode::CONFLICT, "session instance changed");
    }

    let session = state.handles.state_rx.borrow().session.clone();
    if !session.session_control
        || !matches!(session.state, crate::core::remote::SessionState::Connected)
    {
        return (
            axum::http::StatusCode::CONFLICT,
            "session must be connected before it can be logged out",
        );
    }
    if state
        .handles
        .event_tx
        .send(RemoteEvent::SessionExitLogout)
        .is_err()
    {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "session runtime is unavailable",
        );
    }
    (axum::http::StatusCode::ACCEPTED, "logging out")
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
fn build_resume_reply(state: &WebState, resume_seq: u64, sub: protocol::SubscribeMode) -> String {
    let buffer = state
        .handles
        .buffer
        .lock()
        .expect("remote buffer lock poisoned");
    let last_seq = buffer.last_seq();
    // A watcher has no scrollback to resume, so skip the tail walk entirely
    // and always report Full -- there is no gap to signal when the client
    // was never rendering text.
    let (mode, lines) = if sub == protocol::SubscribeMode::Watch {
        (SnapshotMode::Full, Vec::new())
    } else if resume_seq == 0 {
        (
            SnapshotMode::Full,
            buffer.snapshot_tail(SNAPSHOT_LINES_PER_STREAM),
        )
    } else {
        match buffer.lines_since(resume_seq) {
            Some(lines) => (SnapshotMode::Resume, lines),
            None => (
                SnapshotMode::Gap,
                buffer.snapshot_tail(SNAPSHOT_LINES_PER_STREAM),
            ),
        }
    };
    drop(buffer);
    let game_state = state.handles.state_rx.borrow().clone();
    protocol::snapshot_for(&game_state, lines, mode, last_seq, sub)
}

async fn send_snapshot(
    socket: &mut WebSocket,
    state: &WebState,
    mode: SnapshotMode,
    sub: protocol::SubscribeMode,
) -> Result<(), axum::Error> {
    // A watcher never renders scrollback, so do not even gather it -- that
    // walk is 300 lines per stream under the buffer lock.
    let (lines, last_seq) = if sub == protocol::SubscribeMode::Watch {
        let buffer = state
            .handles
            .buffer
            .lock()
            .expect("remote buffer lock poisoned");
        (Vec::new(), buffer.last_seq())
    } else {
        let (_, lines, last_seq) = gather_snapshot(state);
        (lines, last_seq)
    };
    let game_state = state.handles.state_rx.borrow().clone();
    let msg = protocol::snapshot_for(&game_state, lines, mode, last_seq, sub);
    socket.send(Message::Text(msg.into())).await
}

/// Handle one parsed client message inside the main loop.
/// Returns false when the socket should close.
async fn handle_client_message(
    socket: &mut WebSocket,
    state: &WebState,
    client_id: u64,
    msg: ClientMessage,
    sub: &mut protocol::SubscribeMode,
) -> bool {
    match msg {
        // Already authenticated; a stray re-auth is harmless.
        ClientMessage::Auth { .. } => true,
        ClientMessage::Subscribe { mode } => {
            // Declares what this connection is for. Takes effect from here
            // on; the client sends it before `resume` so its snapshot is
            // already tailored.
            *sub = mode;
            true
        }
        ClientMessage::ExitLogout => state
            .handles
            .event_tx
            .send(RemoteEvent::SessionExitLogout)
            .is_ok(),
        ClientMessage::Cmd { text } => {
            // Forward into the main loop; it runs the same path as local
            // input. Send fails only if the app is shutting down.
            state
                .handles
                .event_tx
                .send(RemoteEvent::Command { client_id, text })
                .is_ok()
        }
        ClientMessage::Resume { seq } => {
            let reply = build_resume_reply(state, seq, *sub);
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
        ClientMessage::WheelPick { key, path } => state
            .handles
            .event_tx
            .send(RemoteEvent::WheelPick { key, path })
            .is_ok(),
        ClientMessage::MapLocations { request_id } => state
            .handles
            .event_tx
            .send(RemoteEvent::MapLocations {
                client_id,
                request_id,
            })
            .is_ok(),
        ClientMessage::MapView {
            request_id,
            location,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::MapView {
                client_id,
                request_id,
                location,
            })
            .is_ok(),
        ClientMessage::MacroSave {
            group,
            label,
            command,
            color,
            confirm,
            insert,
            client,
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
                insert,
                client,
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
            lich_host,
            lich_port,
            custom_launch,
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
                lich_host,
                lich_port,
                custom_launch,
            })
            .is_ok(),
        ClientMessage::Disconnect => state
            .handles
            .event_tx
            .send(RemoteEvent::SessionDisconnect)
            .is_ok(),
        ClientMessage::LauncherSshGet { request_id } => state
            .handles
            .event_tx
            .send(RemoteEvent::LauncherSshGet {
                client_id,
                request_id,
            })
            .is_ok(),
        ClientMessage::LauncherSshPut {
            request_id,
            user,
            host,
            port,
            remote_os,
            generate_key,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::LauncherSshPut {
                client_id,
                request_id,
                user,
                host,
                port,
                remote_os,
                generate_key,
            })
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
        ClientMessage::SettingsGet { request_id } => state
            .handles
            .event_tx
            .send(RemoteEvent::SettingsGet {
                client_id,
                request_id,
            })
            .is_ok(),
        ClientMessage::SettingsPut {
            request_id,
            key,
            value,
            scope,
            clear,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::SettingsPut {
                client_id,
                request_id,
                key,
                value,
                scope,
                clear,
            })
            .is_ok(),
        ClientMessage::StreamsGet { request_id } => state
            .handles
            .event_tx
            .send(RemoteEvent::StreamsGet {
                client_id,
                request_id,
            })
            .is_ok(),
        ClientMessage::StreamsPut {
            request_id,
            stream,
            target,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::StreamsPut {
                client_id,
                request_id,
                stream,
                target,
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
        ClientMessage::TouchWheelGet { request_id, scope } => state
            .handles
            .event_tx
            .send(RemoteEvent::TouchWheelGet {
                client_id,
                request_id,
                scope,
            })
            .is_ok(),
        ClientMessage::TouchWheelPut {
            request_id,
            scope,
            slices,
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::TouchWheelPut {
                client_id,
                request_id,
                scope,
                slices,
            })
            .is_ok(),
        ClientMessage::WebUiSubscribe { page } => state
            .handles
            .event_tx
            .send(RemoteEvent::WebUiSubscribe { page })
            .is_ok(),
        ClientMessage::WebUiUnsubscribe { page } => state
            .handles
            .event_tx
            .send(RemoteEvent::WebUiUnsubscribe { page })
            .is_ok(),
        ClientMessage::WebUiEvent { page, cid, value } => state
            .handles
            .event_tx
            .send(RemoteEvent::WebUiEvent { page, cid, value })
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
        ClientMessage::SkillTrainerOpen => state
            .handles
            .event_tx
            .send(RemoteEvent::SkillTrainerOpen)
            .is_ok(),
        ClientMessage::SkillTrainerReload => state
            .handles
            .event_tx
            .send(RemoteEvent::SkillTrainerReload)
            .is_ok(),
        ClientMessage::SkillTrainerApply => state
            .handles
            .event_tx
            .send(RemoteEvent::SkillTrainerApply)
            .is_ok(),
        ClientMessage::SkillTrainerStep { id, n, raise } => state
            .handles
            .event_tx
            .send(RemoteEvent::SkillTrainerStep { id, n, raise })
            .is_ok(),
        ClientMessage::SkillTrainerProfileSave { name } => state
            .handles
            .event_tx
            .send(RemoteEvent::SkillTrainerProfileSave { name })
            .is_ok(),
        ClientMessage::SkillTrainerProfileLoad { name } => state
            .handles
            .event_tx
            .send(RemoteEvent::SkillTrainerProfileLoad { name })
            .is_ok(),
        ClientMessage::SkillTrainerProfileDelete { name } => state
            .handles
            .event_tx
            .send(RemoteEvent::SkillTrainerProfileDelete { name })
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

/// Saved profiles serialized for the session screen: direct-mode logins
/// and Lich attach targets (mode-tagged so the client renders each kind).
fn profiles_reply(state: &WebState) -> String {
    use crate::config::profiles::LaunchMode;
    let list: Vec<protocol::ProfileEntry> = crate::config::profiles::LauncherStore::load()
        .map(|store| {
            store
                .profiles
                .iter()
                .map(|p| match p.mode {
                    LaunchMode::Direct => protocol::ProfileEntry {
                        name: p.name.clone(),
                        mode: "direct".to_string(),
                        account_masked: protocol::mask_account(&p.account),
                        character: p.character.clone(),
                        game: p.game.clone(),
                        has_password: p.password_saved,
                        host: None,
                        port: None,
                        custom_launch: None,
                    },
                    LaunchMode::Lich => protocol::ProfileEntry {
                        name: p.name.clone(),
                        mode: "lich".to_string(),
                        account_masked: String::new(),
                        character: p.character.clone(),
                        game: String::new(),
                        has_password: false,
                        host: Some(p.host.clone()),
                        port: Some(p.port),
                        custom_launch: p.custom_launch.clone(),
                    },
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
                Some(ClientMessage::Auth { ref token }) if token_matches(token, &state.auth_token)
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

    // What this connection is for. Defaults to the full feed, which is what
    // every client that predates `subscribe` implies.
    let mut sub = protocol::SubscribeMode::default();

    // The client answers hello with `resume { seq }` (0 = fresh), optionally
    // preceded by `subscribe { mode }`. Fall back to a full snapshot for
    // clients that never send one.
    //
    // The loop exists so `subscribe` can arrive first without consuming the
    // client's one shot at a snapshot: it sets the mode and we wait again for
    // the `resume` that actually triggers the reply. BOUNDED: a client that
    // streams subscribe frames back-to-back would otherwise spin this task
    // forever without ever reaching the main loop -- after the budget it is
    // treated as a fresh client and handed a snapshot.
    let mut handshake_budget: u8 = 8;
    loop {
        if handshake_budget == 0 {
            if send_snapshot(&mut socket, &state, SnapshotMode::Full, sub)
                .await
                .is_err()
            {
                return;
            }
            break;
        }
        handshake_budget -= 1;
        let first = tokio::time::timeout(RESUME_WAIT, socket.recv()).await;
        match first {
            Ok(None) | Ok(Some(Err(_))) | Ok(Some(Ok(Message::Close(_)))) => return,
            Ok(Some(Ok(Message::Text(text)))) => {
                match protocol::parse_client_message(&text) {
                    Some(ClientMessage::Subscribe { mode }) => {
                        sub = mode;
                        // Keep waiting for the resume.
                        continue;
                    }
                    Some(msg) => {
                        if !handle_client_message(&mut socket, &state, client_id, msg, &mut sub)
                            .await
                        {
                            return;
                        }
                    }
                    None => {
                        if send_snapshot(&mut socket, &state, SnapshotMode::Full, sub)
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
            Ok(Some(Ok(_))) | Err(_) => {
                // Non-text frame or timeout: treat as a fresh client.
                if send_snapshot(&mut socket, &state, SnapshotMode::Full, sub)
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }
        break;
    }

    // Macro and wheel definitions follow the snapshot; updates arrive as
    // deltas.
    {
        let macros = state.handles.macros_rx.borrow().clone();
        let wheels = state.handles.wheels_rx.borrow().clone();
        let (_, _, last_seq) = gather_snapshot(&state);
        let msg = protocol::macros(&macros, last_seq);
        if socket.send(Message::Text(msg.into())).await.is_err() {
            return;
        }
        let msg = protocol::wheels(&wheels, last_seq);
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
                    | RemoteDelta::OpenUrl { client_id: target, .. }
                    | RemoteDelta::ConfigFile { client_id: target, .. }
                    | RemoteDelta::LauncherSsh { client_id: target, .. }
                    | RemoteDelta::Highlights { client_id: target, .. }
                    | RemoteDelta::Colors { client_id: target, .. }
                    | RemoteDelta::TouchWheel { client_id: target, .. }
                    | RemoteDelta::Settings { client_id: target, .. }
                    | RemoteDelta::Streams { client_id: target, .. }
                    | RemoteDelta::MapLocations { client_id: target, .. }
                    | RemoteDelta::MapBrowse { client_id: target, .. } = &d
                    {
                        if *target != client_id {
                            continue;
                        }
                    }
                    // A watcher gets the status set only; the text stream and
                    // the map are the bulk of the traffic and it renders
                    // neither.
                    if !sub.wants(&d) {
                        continue;
                    }
                    // A watcher wants to know WHERE a peer is, not the prose
                    // describing it -- slim the Room delta down to name + id
                    // before encoding, mirroring what the watch snapshot does.
                    let d = match (&d, sub) {
                        (
                            RemoteDelta::Room { name, id, .. },
                            protocol::SubscribeMode::Watch,
                        ) => RemoteDelta::Room {
                            name: name.clone(),
                            id: id.clone(),
                            exits: Vec::new(),
                            description: Vec::new(),
                        },
                        _ => d,
                    };
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
                    if send_snapshot(&mut socket, &state, SnapshotMode::Gap, sub)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Text(text))) => {
                    if let Some(msg) = protocol::parse_client_message(&text) {
                        if !handle_client_message(
                            &mut socket,
                            &state,
                            client_id,
                            msg,
                            &mut sub,
                        )
                        .await
                        {
                            break;
                        }
                    }
                }
                Some(Ok(_)) => {}
            },
        }
    }
}
