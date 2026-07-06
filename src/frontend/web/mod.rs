//! Web frontend sidecar: an embedded HTTP + WebSocket server that lets a
//! phone browser join the session while the TUI/GUI keeps running.
//!
//! Not a third `FrontendType` — the active frontend's runtime calls
//! [`start`] when `[web] enabled = true` (or `--web-port` is passed),
//! attaches the returned `RemoteSink` to `AppCore`, and calls
//! `AppCore::flush_remote_state()` once per message batch. Everything else
//! (serving assets, per-client snapshots, delta fan-out) happens on the
//! spawned server task. See docs/mobile-web-frontend-plan.md.

pub mod protocol;
pub mod server;

use crate::config::WebConfig;
use crate::core::remote::{RemoteEvent, RemoteSink};
use crate::data::remote_buffer::DEFAULT_MAX_LINES_PER_STREAM;

/// Create the remote plumbing and spawn the web server task on the current
/// tokio runtime. Must be called from within a runtime. Bind errors are
/// reported by the spawned task via tracing plus a Notice event that the
/// main loop surfaces as a system message (the game session continues
/// without the web server).
///
/// `session_label` names this instance on the multi-session dashboard.
/// Returns the sink to attach to `AppCore` and the receiver of remote
/// client input, which the frontend's main loop must drain.
pub fn start(
    config: &WebConfig,
    session_label: String,
) -> (
    RemoteSink,
    tokio::sync::mpsc::UnboundedReceiver<RemoteEvent>,
) {
    let (sink, handles, event_rx) = RemoteSink::new(DEFAULT_MAX_LINES_PER_STREAM);
    let config = config.clone();
    tokio::spawn(async move {
        if let Err(e) = server::serve(config, handles, session_label).await {
            tracing::error!("web server error: {e:#}");
        }
    });
    (sink, event_rx)
}

/// Remove this instance's dashboard registry entry (clean shutdown).
pub fn shutdown() {
    server::registry::remove_entry();
}
