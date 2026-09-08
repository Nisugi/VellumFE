//! Web frontend sidecar: an embedded HTTP + WebSocket server that lets a
//! phone browser join the session while the TUI/GUI keeps running.
//!
//! Not a third `FrontendType` — the active frontend's runtime calls
//! [`start`] when `[web] enabled = true` (or `--web-port` is passed),
//! attaches the returned `RemoteSink` to `AppCore`, and calls
//! `AppCore::flush_remote_state()` once per message batch. Everything else
//! (serving assets, per-client snapshots, delta fan-out) happens on the
//! spawned server task. See docs/mobile-web-frontend-plan.md.

pub mod doll;
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
    start_with_classic_maps(
        config,
        session_label,
        std::sync::Arc::new(crate::core::classic_maps::ClassicMapCatalog::new()),
    )
}

/// Start the sidecar with classic-map filesystem authority scoped to the
/// calling game session.
pub fn start_with_classic_maps(
    config: &WebConfig,
    session_label: String,
    classic_maps: std::sync::Arc<crate::core::classic_maps::ClassicMapCatalog>,
) -> (
    RemoteSink,
    tokio::sync::mpsc::UnboundedReceiver<RemoteEvent>,
) {
    start_with_startup_failure(config, session_label, classic_maps, None)
}

/// [`start_with_classic_maps`] plus an optional startup-failure reporter.
///
/// Successful startup is already observable through the sink's
/// launch-endpoint watch channel (published only after bind + token +
/// registry). Failure, however, used to be visible only as a tracing line
/// and a Notice event — a waiting embedder (mobile shell startup) could not
/// distinguish "still binding" from "failed". When the serve task exits
/// with an error before/instead of publishing readiness, the error text is
/// delivered on `startup_failure_tx`. On success the sender is simply held
/// for the life of the server and never fired.
pub fn start_with_startup_failure(
    config: &WebConfig,
    session_label: String,
    classic_maps: std::sync::Arc<crate::core::classic_maps::ClassicMapCatalog>,
    startup_failure_tx: Option<tokio::sync::oneshot::Sender<String>>,
) -> (
    RemoteSink,
    tokio::sync::mpsc::UnboundedReceiver<RemoteEvent>,
) {
    let (sink, handles, event_rx) = RemoteSink::new(DEFAULT_MAX_LINES_PER_STREAM);
    let config = config.clone();
    tokio::spawn(async move {
        let options = server::ServeOptions {
            status_only: config.local_status_only(),
            classic_maps,
        };
        if let Err(e) = server::serve(config, handles, session_label, options).await {
            tracing::error!("web server error: {e:#}");
            if let Some(tx) = startup_failure_tx {
                let _ = tx.send(format!("{e:#}"));
            }
        }
    });
    (sink, event_rx)
}

/// Remove this instance's dashboard registry entry (clean shutdown).
pub fn shutdown() {
    server::registry::remove_entry();
}
