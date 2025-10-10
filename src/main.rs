mod app;
mod config;
mod network;
mod parser;
mod performance;
mod ui;

use anyhow::Result;
use app::App;
use config::Config;
use std::fs::OpenOptions;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to file instead of stderr to not mess up TUI
    let log_file = dirs::home_dir()
        .unwrap()
        .join(".profanity-rs")
        .join("debug.log");

    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)?;

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(file)
        .with_ansi(false)
        .init();

    // Load configuration
    let config = Config::load()?;

    // Create and run the application
    let mut app = App::new(config)?;
    app.run().await?;

    Ok(())
}
