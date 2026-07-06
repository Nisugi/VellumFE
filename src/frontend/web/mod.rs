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
use crate::core::remote::RemoteSink;
use crate::data::remote_buffer::DEFAULT_MAX_LINES_PER_STREAM;

/// Create the remote plumbing and spawn the web server task on the current
/// tokio runtime. Must be called from within a runtime. Bind errors are
/// reported by the spawned task via tracing (the game session continues
/// without the web server).
pub fn start(config: &WebConfig) -> RemoteSink {
    let (sink, handles) = RemoteSink::new(DEFAULT_MAX_LINES_PER_STREAM);
    let config = config.clone();
    tokio::spawn(async move {
        if let Err(e) = server::serve(config, handles).await {
            tracing::error!("web server error: {e:#}");
        }
    });
    sink
}
