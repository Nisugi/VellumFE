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

use crate::config::profiles::LaunchWebClient;
use anyhow::Result;

/// Internal launcher behavior. Public/embedded headless entrypoints use the
/// default so they continue to wait on `/play` and never open a browser.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct HeadlessLaunchOptions {
    auto_connect_lich: bool,
    web_client: Option<LaunchWebClient>,
    /// Immutable identity of a launcher-profile-owned child.  When present,
    /// web session controls may reconnect only this character and connection;
    /// they cannot retarget the process away from the registry/endpoint lease
    /// acquired at startup.
    startup_identity: Option<crate::core::session_registry::SessionLaunchIdentity>,
}

/// Desktop entry point (`--frontend headless`). Builds a tokio runtime and
/// runs until `.quit`, Ctrl+C, or a fatal error. The web server is forced on.
pub fn run(
    config: crate::config::Config,
    character: Option<String>,
    direct: Option<crate::network::DirectConnectConfig>,
    login_key: Option<String>,
) -> Result<()> {
    run_with_options(
        config,
        character,
        direct,
        login_key,
        HeadlessLaunchOptions::default(),
    )
}

/// Launcher-only browser-client entrypoint. The selected saved profile is
/// already resolved into `config`/`direct`; the attach flag distinguishes a
/// saved Lich profile from a credential-less manual headless start.
pub(crate) fn run_launcher_web_client(
    config: crate::config::Config,
    character: Option<String>,
    direct: Option<crate::network::DirectConnectConfig>,
    login_key: Option<String>,
    web_client: LaunchWebClient,
    auto_connect_lich: bool,
    startup_identity: crate::core::session_registry::SessionLaunchIdentity,
) -> Result<()> {
    run_with_options(
        config,
        character,
        direct,
        login_key,
        HeadlessLaunchOptions {
            auto_connect_lich,
            web_client: Some(web_client),
            startup_identity: Some(startup_identity),
        },
    )
}

fn run_with_options(
    config: crate::config::Config,
    character: Option<String>,
    direct: Option<crate::network::DirectConnectConfig>,
    login_key: Option<String>,
    launch: HeadlessLaunchOptions,
) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                let _ = shutdown_tx.send(true);
            }
        });
        runtime::async_run_with_options(config, character, direct, login_key, shutdown_rx, launch)
            .await
    })
}

/// Embeddable entry point. The caller owns the runtime and signals shutdown
/// via the watch channel. Mobile shells go through [`embedded`], which wraps
/// this in a managed thread + runtime.
pub use runtime::async_run;
