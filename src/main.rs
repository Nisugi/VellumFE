mod app;
mod cmdlist;
mod config;
mod network;
mod parser;
mod performance;
mod selection;
mod sound;
mod ui;

use anyhow::Result;
use app::App;
use clap::Parser;
use config::Config;
use std::fs::OpenOptions;
use tracing_subscriber;

/// VellumFE - A modern, high-performance terminal frontend for GemStone IV
#[derive(Parser, Debug)]
#[command(name = "vellum-fe")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to connect to (Lich detached mode port)
    #[arg(short, long, default_value = "8000")]
    port: u16,

    /// Character name / config file to load (loads ./config/<character>.toml or default.toml)
    #[arg(short, long)]
    character: Option<String>,

    /// Enable link highlighting (required for proper game feed with clickable links)
    #[arg(long, default_value = "false")]
    links: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let args = Args::parse();

    // Initialize logging to character-specific file instead of stderr to not mess up TUI
    let log_file = Config::get_log_path(args.character.as_deref())?;

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

    // Load configuration (with character override if specified)
    let config = Config::load_with_options(args.character.as_deref(), args.port)?;

    // Create and run the application
    let mut app = App::new(config)?;

    // Auto-shrink layout if terminal is smaller than designed size
    app.check_and_auto_resize()?;

    app.run().await?;

    Ok(())
}
