//! Built-in Despana web presentation.
//!
//! This module owns the presentation's routes and embedded browser assets.
//! It deliberately does not own session state, authentication, or protocol
//! behavior: those remain in VellumFE's shared web server.

use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use super::{token_matches, WebState};

const WORKSPACE_LAYOUT_VERSION: u64 = 1;
const WORKSPACE_STORAGE_VERSION: u64 = 1;
const WORKSPACE_LAYOUT_MAX_BYTES: usize = 64 * 1024;
const WORKSPACE_LAYOUT_FILE: &str = "despana-workspace-v1.json";

/// Mount the optional Despana presentation.
///
/// Keeping this as a single router is the integration seam: VellumFE's shared
/// server needs one merge call, while the presentation owns its asset surface.
pub(super) fn router() -> Router<Arc<WebState>> {
    Router::new()
        .route("/despana", get(index_html))
        .route("/despana/", get(index_html))
        .route("/despana/app.js", get(app_js))
        .route("/despana/font-scale.js", get(font_scale_js))
        .route("/despana/session.js", get(session_js))
        .route("/despana/inventory-refresh.js", get(inventory_refresh_js))
        .route("/despana/inventory-tree.js", get(inventory_tree_js))
        .route("/despana/interactions.js", get(interactions_js))
        .route("/despana/layout.js", get(layout_js))
        .route(
            "/despana/workspace-persistence.js",
            get(workspace_persistence_js),
        )
        .route("/despana/map.js", get(map_js))
        .route("/despana/workspace.js", get(workspace_js))
        .route("/despana/app.css", get(app_css))
        .route(
            "/api/v1/presentations/despana/workspace",
            get(load_workspace)
                .put(save_workspace)
                .route_layer(DefaultBodyLimit::max(WORKSPACE_LAYOUT_MAX_BYTES)),
        )
}

// Embedded assets change with the binary. A cached client from an older
// protocol revision is more harmful than the small cost of re-fetching them.
async fn index_html() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Html(include_str!("../assets/despana/index.html")),
    )
}

macro_rules! embedded_asset {
    ($handler:ident, $mime:literal, $path:literal) => {
        async fn $handler() -> impl IntoResponse {
            (
                [
                    (header::CONTENT_TYPE, $mime),
                    (header::CACHE_CONTROL, "no-cache"),
                ],
                include_str!($path),
            )
        }
    };
}

embedded_asset!(
    app_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/app.js"
);
embedded_asset!(
    font_scale_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/font-scale.js"
);
embedded_asset!(
    session_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/session.js"
);
embedded_asset!(
    inventory_refresh_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/inventory-refresh.js"
);
embedded_asset!(
    inventory_tree_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/inventory-tree.js"
);
embedded_asset!(
    interactions_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/interactions.js"
);
embedded_asset!(
    layout_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/layout.js"
);
embedded_asset!(
    workspace_persistence_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/workspace-persistence.js"
);
embedded_asset!(
    map_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/map.js"
);

#[derive(Deserialize)]
struct WorkspaceLayoutIdentity {
    version: u64,
    character: String,
    tracks: serde_json::Value,
    zones: serde_json::Value,
    hidden: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WorkspaceEnvelope {
    storage_version: u64,
    revision: u64,
    layout: serde_json::Value,
}

fn authorized(headers: &HeaderMap, state: &WebState) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token_matches(token, &state.auth_token))
}

fn active_character(state: &WebState) -> Option<String> {
    let snapshot = state.handles.state_rx.borrow();
    snapshot
        .character
        .as_deref()
        .or(snapshot.session.character.as_deref())
        .map(str::trim)
        .filter(|character| !character.is_empty())
        .map(str::to_owned)
}

fn workspace_path(character: &str) -> anyhow::Result<std::path::PathBuf> {
    Ok(crate::config::Config::profile_dir(Some(character))?.join(WORKSPACE_LAYOUT_FILE))
}

fn validate_workspace_value(
    value: serde_json::Value,
    character: &str,
) -> Result<serde_json::Value, &'static str> {
    let layout: WorkspaceLayoutIdentity =
        serde_json::from_value(value.clone()).map_err(|_| "workspace layout is not valid JSON")?;
    if layout.version != WORKSPACE_LAYOUT_VERSION {
        return Err("workspace layout version is not supported");
    }
    if !layout
        .character
        .trim()
        .eq_ignore_ascii_case(character.trim())
    {
        return Err("workspace layout character does not match the active session");
    }
    if !layout.tracks.is_object() || !layout.zones.is_object() || !layout.hidden.is_array() {
        return Err("workspace layout shape is invalid");
    }
    Ok(value)
}

fn decode_workspace(bytes: &[u8], character: &str) -> Result<WorkspaceEnvelope, &'static str> {
    if bytes.is_empty() || bytes.len() > WORKSPACE_LAYOUT_MAX_BYTES {
        return Err("workspace layout has an invalid size");
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "workspace layout is not valid JSON")?;
    if value.get("storage_version").is_some() {
        let envelope: WorkspaceEnvelope =
            serde_json::from_value(value).map_err(|_| "workspace envelope is invalid")?;
        if envelope.storage_version != WORKSPACE_STORAGE_VERSION {
            return Err("workspace storage version is not supported");
        }
        if envelope.revision == 0 {
            return Err("workspace revision must be positive");
        }
        validate_workspace_value(envelope.layout.clone(), character)?;
        return Ok(envelope);
    }
    Ok(WorkspaceEnvelope {
        storage_version: WORKSPACE_STORAGE_VERSION,
        revision: 0,
        layout: validate_workspace_value(value, character)?,
    })
}

fn response(status: StatusCode, message: &'static str) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        message,
    )
        .into_response()
}

fn workspace_response(status: StatusCode, workspace: &WorkspaceEnvelope) -> Response {
    let bytes = match serde_json::to_vec(workspace) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!("unable to encode Despana workspace: {error}");
            return response(StatusCode::INTERNAL_SERVER_ERROR, "workspace unavailable");
        }
    };
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response()
}

enum WorkspaceSaveOutcome {
    Saved,
    Superseded(WorkspaceEnvelope),
}

enum WorkspaceSaveError {
    Invalid(&'static str),
    Io(std::io::Error),
}

impl From<std::io::Error> for WorkspaceSaveError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

fn compare_and_write_workspace(
    path: &Path,
    character: &str,
    incoming: WorkspaceEnvelope,
) -> Result<WorkspaceSaveOutcome, WorkspaceSaveError> {
    let parent = path.parent().ok_or_else(|| {
        WorkspaceSaveError::Io(std::io::Error::other("workspace path has no parent"))
    })?;
    std::fs::create_dir_all(parent)?;
    let lock_path = parent.join(format!("{WORKSPACE_LAYOUT_FILE}.lock"));
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock_file.lock()?;

    let current = match std::fs::read(path) {
        Ok(bytes) => {
            Some(decode_workspace(&bytes, character).map_err(WorkspaceSaveError::Invalid)?)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(WorkspaceSaveError::Io(error)),
    };
    if let Some(current) = current {
        if incoming.revision <= current.revision {
            return Ok(WorkspaceSaveOutcome::Superseded(current));
        }
    }

    let bytes = serde_json::to_vec(&incoming).map_err(|error| {
        WorkspaceSaveError::Io(std::io::Error::other(format!(
            "unable to encode workspace: {error}"
        )))
    })?;
    crate::config::write_atomic(path, bytes)?;
    Ok(WorkspaceSaveOutcome::Saved)
}

async fn load_workspace(State(state): State<Arc<WebState>>, headers: HeaderMap) -> Response {
    if !authorized(&headers, &state) {
        return response(StatusCode::FORBIDDEN, "pairing token required");
    }
    let Some(character) = active_character(&state) else {
        return response(StatusCode::CONFLICT, "no active character");
    };
    let path = match workspace_path(&character) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!("unable to resolve Despana workspace path: {error:#}");
            return response(StatusCode::INTERNAL_SERVER_ERROR, "workspace unavailable");
        }
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return response(StatusCode::NOT_FOUND, "workspace not saved");
        }
        Err(error) => {
            tracing::warn!("unable to read Despana workspace: {error}");
            return response(StatusCode::INTERNAL_SERVER_ERROR, "workspace unavailable");
        }
    };
    let workspace = match decode_workspace(&bytes, &character) {
        Ok(workspace) => workspace,
        Err(error) => {
            tracing::warn!("ignoring invalid saved Despana workspace: {error}");
            return response(StatusCode::UNPROCESSABLE_ENTITY, error);
        }
    };
    workspace_response(StatusCode::OK, &workspace)
}

async fn save_workspace(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !authorized(&headers, &state) {
        return response(StatusCode::FORBIDDEN, "pairing token required");
    }
    let Some(character) = active_character(&state) else {
        return response(StatusCode::CONFLICT, "no active character");
    };
    let incoming = match decode_workspace(&body, &character) {
        Ok(workspace) if workspace.revision > 0 => workspace,
        Ok(_) => {
            return response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "workspace revision must be positive",
            )
        }
        Err(error) => return response(StatusCode::UNPROCESSABLE_ENTITY, error),
    };
    let _write_guard = state.workspace_write_lock.lock().await;
    let path = match workspace_path(&character) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!("unable to resolve Despana workspace path: {error:#}");
            return response(StatusCode::INTERNAL_SERVER_ERROR, "workspace unavailable");
        }
    };
    let task_character = character.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        compare_and_write_workspace(&path, &task_character, incoming)
    })
    .await;
    match outcome {
        Ok(Ok(WorkspaceSaveOutcome::Saved)) => response(StatusCode::NO_CONTENT, ""),
        Ok(Ok(WorkspaceSaveOutcome::Superseded(current))) => {
            workspace_response(StatusCode::PRECONDITION_FAILED, &current)
        }
        Ok(Err(WorkspaceSaveError::Invalid(error))) => {
            tracing::warn!("unable to replace invalid saved Despana workspace: {error}");
            response(StatusCode::UNPROCESSABLE_ENTITY, error)
        }
        Ok(Err(WorkspaceSaveError::Io(error))) => {
            tracing::warn!("unable to save Despana workspace: {error}");
            response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "workspace could not be saved",
            )
        }
        Err(error) => {
            tracing::warn!("Despana workspace save task failed: {error}");
            response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "workspace could not be saved",
            )
        }
    }
}
embedded_asset!(
    workspace_js,
    "text/javascript; charset=utf-8",
    "../assets/despana/workspace.js"
);
embedded_asset!(
    app_css,
    "text/css; charset=utf-8",
    "../assets/despana/app.css"
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classic_maps::ClassicMapCatalog;
    use crate::core::remote::{RemoteSessionInfo, RemoteSink, SessionState};

    fn layout(character: &str) -> Vec<u8> {
        format!(
            r#"{{"version":1,"character":"{character}","tracks":{{}},"zones":{{}},"hidden":[]}}"#
        )
        .into_bytes()
    }

    #[test]
    fn validates_only_current_version_and_active_character() {
        assert_eq!(
            decode_workspace(&layout("Testchar"), "testchar")
                .unwrap()
                .revision,
            0
        );
        assert_eq!(
            decode_workspace(&layout("SomeoneElse"), "Testchar").unwrap_err(),
            "workspace layout character does not match the active session"
        );
        assert_eq!(
            decode_workspace(
                br#"{"version":2,"character":"Testchar","tracks":{},"zones":{},"hidden":[]}"#,
                "Testchar"
            )
            .unwrap_err(),
            "workspace layout version is not supported"
        );
    }

    #[test]
    fn rejects_unbounded_or_malformed_layouts() {
        assert!(decode_workspace(&[], "Testchar").is_err());
        assert!(decode_workspace(b"not json", "Testchar").is_err());
        assert!(decode_workspace(&vec![b'x'; WORKSPACE_LAYOUT_MAX_BYTES + 1], "Testchar").is_err());
    }

    #[tokio::test]
    async fn authenticated_round_trip_uses_the_launcher_session_character() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        let (mut sink, handles, _events) = RemoteSink::new(10);
        sink.set_session_state(RemoteSessionInfo {
            state: SessionState::Connecting,
            character: Some("Aster".to_string()),
            session_control: true,
            ..RemoteSessionInfo::default()
        });
        assert!(handles.state_rx.borrow().character.is_none());
        let state = Arc::new(WebState {
            handles,
            classic_maps: Arc::new(ClassicMapCatalog::new()),
            auth_token: "test-token".to_string(),
            auth_failures: std::sync::Mutex::new(Vec::new()),
            workspace_write_lock: tokio::sync::Mutex::new(()),
        });
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer test-token".parse().unwrap());
        let raw_layout = layout("aster");
        let bytes = serde_json::to_vec(&WorkspaceEnvelope {
            storage_version: WORKSPACE_STORAGE_VERSION,
            revision: 1,
            layout: serde_json::from_slice(&raw_layout).unwrap(),
        })
        .unwrap();

        let saved = save_workspace(
            State(Arc::clone(&state)),
            headers.clone(),
            Bytes::from(bytes.clone()),
        )
        .await;
        assert_eq!(saved.status(), StatusCode::NO_CONTENT);

        let loaded = load_workspace(State(state), headers).await;
        assert_eq!(loaded.status(), StatusCode::OK);
        let loaded = axum::body::to_bytes(loaded.into_body(), WORKSPACE_LAYOUT_MAX_BYTES)
            .await
            .unwrap();
        assert_eq!(loaded.as_ref(), bytes);

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[tokio::test]
    async fn stale_workspace_write_is_rejected() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        let (mut sink, handles, _events) = RemoteSink::new(10);
        sink.set_session_state(RemoteSessionInfo {
            state: SessionState::Connected,
            character: Some("Aster".to_string()),
            session_control: true,
            ..RemoteSessionInfo::default()
        });
        let state = Arc::new(WebState {
            handles,
            classic_maps: Arc::new(ClassicMapCatalog::new()),
            auth_token: "test-token".to_string(),
            auth_failures: std::sync::Mutex::new(Vec::new()),
            workspace_write_lock: tokio::sync::Mutex::new(()),
        });
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer test-token".parse().unwrap());
        let envelope = |revision| {
            Bytes::from(
                serde_json::to_vec(&WorkspaceEnvelope {
                    storage_version: WORKSPACE_STORAGE_VERSION,
                    revision,
                    layout: serde_json::from_slice(&layout("aster")).unwrap(),
                })
                .unwrap(),
            )
        };

        assert_eq!(
            save_workspace(State(Arc::clone(&state)), headers.clone(), envelope(20))
                .await
                .status(),
            StatusCode::NO_CONTENT
        );
        let rejected =
            save_workspace(State(Arc::clone(&state)), headers.clone(), envelope(19)).await;
        assert_eq!(rejected.status(), StatusCode::PRECONDITION_FAILED);
        let rejected = axum::body::to_bytes(rejected.into_body(), WORKSPACE_LAYOUT_MAX_BYTES)
            .await
            .unwrap();
        let rejected: WorkspaceEnvelope = serde_json::from_slice(&rejected).unwrap();
        assert_eq!(rejected.revision, 20);
        let loaded = load_workspace(State(state), headers).await;
        let loaded = axum::body::to_bytes(loaded.into_body(), WORKSPACE_LAYOUT_MAX_BYTES)
            .await
            .unwrap();
        let loaded: WorkspaceEnvelope = serde_json::from_slice(&loaded).unwrap();
        assert_eq!(loaded.revision, 20);

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn independent_web_states_share_the_filesystem_compare_and_write_lock() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        let make_state = || {
            let (mut sink, handles, _events) = RemoteSink::new(10);
            sink.set_session_state(RemoteSessionInfo {
                state: SessionState::Connected,
                character: Some("Aster".to_string()),
                session_control: true,
                ..RemoteSessionInfo::default()
            });
            Arc::new(WebState {
                handles,
                classic_maps: Arc::new(ClassicMapCatalog::new()),
                auth_token: "test-token".to_string(),
                auth_failures: std::sync::Mutex::new(Vec::new()),
                workspace_write_lock: tokio::sync::Mutex::new(()),
            })
        };
        let high_state = make_state();
        let low_state = make_state();
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer test-token".parse().unwrap());
        let envelope = |revision| {
            Bytes::from(
                serde_json::to_vec(&WorkspaceEnvelope {
                    storage_version: WORKSPACE_STORAGE_VERSION,
                    revision,
                    layout: serde_json::from_slice(&layout("aster")).unwrap(),
                })
                .unwrap(),
            )
        };
        let barrier = Arc::new(tokio::sync::Barrier::new(3));

        let high_barrier = Arc::clone(&barrier);
        let high_headers = headers.clone();
        let high_for_save = Arc::clone(&high_state);
        let high = tokio::spawn(async move {
            high_barrier.wait().await;
            save_workspace(State(high_for_save), high_headers, envelope(20))
                .await
                .status()
        });
        let low_barrier = Arc::clone(&barrier);
        let low_headers = headers.clone();
        let low = tokio::spawn(async move {
            low_barrier.wait().await;
            save_workspace(State(low_state), low_headers, envelope(19))
                .await
                .status()
        });
        barrier.wait().await;
        let (high, low) = tokio::join!(high, low);
        assert_eq!(high.unwrap(), StatusCode::NO_CONTENT);
        assert!(matches!(
            low.unwrap(),
            StatusCode::NO_CONTENT | StatusCode::PRECONDITION_FAILED
        ));

        let loaded = load_workspace(State(high_state), headers).await;
        let loaded = axum::body::to_bytes(loaded.into_body(), WORKSPACE_LAYOUT_MAX_BYTES)
            .await
            .unwrap();
        let loaded: WorkspaceEnvelope = serde_json::from_slice(&loaded).unwrap();
        assert_eq!(loaded.revision, 20);

        std::env::remove_var("VELLUM_FE_DIR");
    }
}
