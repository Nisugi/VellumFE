//! Headless runtime: core + web frontend with no local UI.
//!
//! This is the web sidecar's plan-doc "Phase 7" and the Android entrypoint:
//! the game session runs here and web clients (a phone WebView, a desktop
//! browser) are the only interface. Unlike the TUI/GUI runtimes it owns a
//! reconnect supervisor — on mobile radios a dropped TCP session must
//! recover without user intervention.
//!
//! Always compiled (no feature gate): it depends only on tokio, core, and
//! the web frontend, and `--no-default-features` builds — the Android
//! configuration — must include it.

pub mod embedded;
mod runtime;

use anyhow::Result;

/// Desktop entry point (`--frontend headless`). Builds a tokio runtime and
/// runs until `.quit`, Ctrl+C, or a fatal error. The web server is forced on.
pub fn run(
    config: crate::config::Config,
    character: Option<String>,
    direct: Option<crate::network::DirectConnectConfig>,
    login_key: Option<String>,
) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                let _ = shutdown_tx.send(true);
            }
        });
        runtime::async_run(config, character, direct, login_key, shutdown_rx).await
    })
}

/// Embeddable entry point. The caller owns the runtime and signals shutdown
/// via the watch channel. Mobile shells go through [`embedded`], which wraps
/// this in a managed thread + runtime.
pub use runtime::async_run;
