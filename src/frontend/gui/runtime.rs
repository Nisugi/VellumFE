//! GUI Runtime - Network integration for egui frontend
//!
//! This module handles the tokio runtime and network connection setup for the GUI.

use anyhow::Result;
use tokio::sync::mpsc;

use super::EguiApp;
use crate::config::Config;
use crate::core::AppCore;
use crate::network::{DirectConnectConfig, LichConnection, DirectConnection, ServerMessage};

/// Run the GUI frontend with the given configuration.
/// This is the main entry point for GUI mode.
pub fn run(
    config: Config,
    character: Option<String>,
    direct: Option<DirectConnectConfig>,
) -> Result<()> {
    // Use tokio runtime for async network I/O
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async_run(config, character, direct))
}

/// Async GUI main loop with network support
async fn async_run(
    config: Config,
    _character: Option<String>,
    direct: Option<DirectConnectConfig>,
) -> Result<()> {
    // Create channels for network communication
    let (server_tx, server_rx) = mpsc::unbounded_channel::<ServerMessage>();
    let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();

    // Store connection info
    let host = config.connection.host.clone();
    let port = config.connection.port;

    // Create core application state
    let mut app_core = AppCore::new(config, crate::config::FrontendType::Gui)?;

    // Get a reasonable default size for window initialization
    // (will be updated when egui reports actual size)
    let (width, height) = (160, 50); // Approximate character grid for 1280x800
    app_core.init_windows(width, height);

    // Spawn network connection task
    let network_handle = match direct {
        Some(cfg) => tokio::spawn(async move {
            if let Err(e) = DirectConnection::start(cfg, server_tx, command_rx).await {
                tracing::error!(error = ?e, "Network connection error");
            }
        }),
        None => {
            let host_clone = host.clone();
            tokio::spawn(async move {
                if let Err(e) = LichConnection::start(&host_clone, port, server_tx, command_rx).await {
                    tracing::error!(error = ?e, "Network connection error");
                }
            })
        }
    };

    // Create GUI app with network channels
    let app = EguiApp::new_with_network(app_core, server_rx, command_tx);

    // Run the GUI (this blocks until the window is closed)
    app.run()?;

    // Cleanup: abort network task
    network_handle.abort();
    let _ = network_handle.await;

    Ok(())
}
