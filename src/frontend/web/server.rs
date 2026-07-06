//! Embedded axum server: serves the phone client (static assets) over HTTP
//! and streams game state over `/ws`.
//!
//! The server task owns only channel ends (`RemoteServerHandles`) — it
//! never touches `AppCore`. Each WebSocket client gets: `hello`, a full
//! `snapshot` (latest state + recent scrollback from the shared ring),
//! then live deltas from the broadcast channel. A client that lags behind
//! the broadcast capacity is re-synced with a fresh snapshot.

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::header;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use tokio::sync::broadcast;

use crate::config::WebConfig;
use crate::core::remote::RemoteServerHandles;
use crate::data::remote_buffer::RemoteLine;

use super::protocol;

/// Scrollback lines per stream included in a connect-time snapshot.
const SNAPSHOT_LINES_PER_STREAM: usize = 300;

struct WebState {
    handles: RemoteServerHandles,
}

/// Bind and serve until the process exits. Runs as a detached tokio task.
pub async fn serve(config: WebConfig, handles: RemoteServerHandles) -> Result<()> {
    let addr = format!("{}:{}", config.bind, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("web server failed to bind {addr}"))?;
    tracing::info!("web server listening on http://{addr}");
    serve_listener(listener, handles).await
}

/// Serve on an already-bound listener (integration tests bind port 0).
pub async fn serve_listener(
    listener: tokio::net::TcpListener,
    handles: RemoteServerHandles,
) -> Result<()> {
    let state = Arc::new(WebState { handles });
    let router = Router::new()
        .route("/", get(index_html))
        .route("/app.js", get(app_js))
        .route("/app.css", get(app_css))
        .route("/health", get(health))
        .route("/ws", get(ws_upgrade))
        .with_state(state);
    axum::serve(listener, router)
        .await
        .context("web server exited")?;
    Ok(())
}

async fn index_html() -> Html<&'static str> {
    Html(include_str!("assets/index.html"))
}

async fn app_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("assets/app.js"),
    )
}

async fn app_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("assets/app.css"),
    )
}

async fn health() -> &'static str {
    "ok"
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

async fn send_snapshot(socket: &mut WebSocket, state: &WebState) -> Result<(), axum::Error> {
    let (_, lines, last_seq) = gather_snapshot(state);
    let game_state = state.handles.state_rx.borrow().clone();
    let msg = protocol::snapshot(&game_state, lines, last_seq);
    socket.send(Message::Text(msg.into())).await
}

async fn handle_client(mut socket: WebSocket, state: Arc<WebState>) {
    // Subscribe BEFORE building the snapshot so no delta can fall in the
    // gap. Deltas that overlap the snapshot are deduped client-side by seq.
    let mut delta_rx = state.handles.delta_tx.subscribe();

    let (streams, _, last_seq) = gather_snapshot(&state);
    let character = state.handles.state_rx.borrow().character.clone();
    let hello = protocol::hello(character, streams, last_seq);
    if socket.send(Message::Text(hello.into())).await.is_err() {
        return;
    }
    if send_snapshot(&mut socket, &state).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            delta = delta_rx.recv() => match delta {
                Ok(d) => {
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
                    if send_snapshot(&mut socket, &state).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                // Client → server input lands in Phase 2; ignore for now.
                Some(Ok(_)) => {}
            },
        }
    }
}
