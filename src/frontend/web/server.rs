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
        } => state
            .handles
            .event_tx
            .send(RemoteEvent::LinkTap {
                client_id,
                request_id,
                exist_id,
                noun,
            })
            .is_ok(),
    }
}

async fn handle_client(mut socket: WebSocket, state: Arc<WebState>) {
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

    loop {
        tokio::select! {
            delta = delta_rx.recv() => match delta {
                Ok(d) => {
                    // Menus are addressed: only the requesting client's
                    // task forwards them.
                    if let RemoteDelta::Menu { client_id: target, .. } = &d {
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
