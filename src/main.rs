mod app;
mod validator;
mod cmdlist;
mod config;
mod map_data;
mod migration;
mod network;
mod parser;
mod performance;
mod selection;
mod sound;
mod terminal_position;
mod ui;

use anyhow::Result;
use app::App;
use clap::Parser;
use config::Config;
use std::fs::OpenOptions;
use std::io::Write;
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

    /// Disable startup music on connection
    #[arg(long, default_value = "false")]
    nomusic: bool,

    /// Disable sound system entirely (skips audio initialization)
    #[arg(long, default_value = "false")]
    nosound: bool,

    /// Validate a layout file against multiple sizes and exit
    #[arg(long, value_name = "PATH", required = false)]
    validate_layout: Option<String>,

    /// Baseline terminal size for validation (e.g., 120x40). Defaults to layout's designed size or 120x40.
    #[arg(long, value_name = "WxH", required = false)]
    baseline: Option<String>,

    /// Comma-separated list of sizes to test (e.g., 80x24,100x30,140x40)
    #[arg(long, value_name = "WxH[,WxH...]", required = false)]
    sizes: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let startup_timer = std::time::Instant::now();
    eprintln!("[{:>6}ms] Starting VellumFE...", startup_timer.elapsed().as_millis());

    // Parse command-line arguments
    let args = Args::parse();
    eprintln!("[{:>6}ms] Args parsed", startup_timer.elapsed().as_millis());

    // Check if config migration is needed (before logging setup, since paths might change)
    let config_dir = Config::config_dir()?;
    if migration::Migration::needs_migration(&config_dir)? {
        println!("\n════════════════════════════════════════════════════════════════");
        println!("  VellumFE Config Migration");
        println!("════════════════════════════════════════════════════════════════\n");
        println!("Your config directory structure needs to be updated.");
        println!("This will reorganize files for better organization.\n");

        migration::Migration::run(config_dir)?;

        println!("\n════════════════════════════════════════════════════════════════\n");
        print!("Press Enter to continue...");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
    }

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
    eprintln!("[{:>6}ms] Logging initialized", startup_timer.elapsed().as_millis());
    tracing::info!("[STARTUP {:>6}ms] Logging initialized", startup_timer.elapsed().as_millis());

    // Load configuration (with character override if specified)
    let config = Config::load_with_options(args.character.as_deref(), args.port)?;
    eprintln!("[{:>6}ms] Config loaded", startup_timer.elapsed().as_millis());
    tracing::info!("[STARTUP {:>6}ms] Config loaded", startup_timer.elapsed().as_millis());

    // Layout validation mode
    if let Some(layout_path) = args.validate_layout.as_ref() {
        // Parse sizes
        let sizes = parse_sizes_arg(args.sizes.as_deref());

        // Determine baseline
        let baseline = if let Some(b) = args.baseline.as_deref() {
            parse_size(b).unwrap_or((120, 40))
        } else {
            // Try to load layout to read designed size; else default
            let lp = std::path::Path::new(layout_path);
            let layout = config::Layout::load_from_file(lp).ok();
            if let Some(l) = layout {
                let w = l.terminal_width.unwrap_or(120);
                let h = l.terminal_height.unwrap_or(40);
                (w, h)
            } else {
                (120, 40)
            }
        };

        let results = crate::validator::validate_layout_path(std::path::Path::new(layout_path), baseline, &sizes)?;
        let mut total_errors = 0usize;
        println!("Layout validation for {} (baseline {}x{}):", layout_path, baseline.0, baseline.1);
        for r in &results {
            if r.issues.is_empty() {
                println!("- {}x{}: OK", r.width, r.height);
            } else {
                println!("- {}x{}:", r.width, r.height);
                for issue in &r.issues {
                    let kind = match issue.kind { crate::validator::IssueKind::Error => "ERR", crate::validator::IssueKind::Warning => "WARN" };
                    println!("    {} [{}] {}", kind, issue.window, issue.message);
                    if matches!(issue.kind, crate::validator::IssueKind::Error) { total_errors += 1; }
                }
            }
        }
        if total_errors > 0 { std::process::exit(2); } else { return Ok(()); }
    }

    // Create and run the application
    eprintln!("[{:>6}ms] Creating App...", startup_timer.elapsed().as_millis());
    tracing::info!("[STARTUP {:>6}ms] Creating App...", startup_timer.elapsed().as_millis());
    let mut app = App::new(config, args.nomusic, args.nosound)?;
    eprintln!("[{:>6}ms] App created", startup_timer.elapsed().as_millis());
    tracing::info!("[STARTUP {:>6}ms] App created", startup_timer.elapsed().as_millis());

    // Auto-shrink layout if terminal is smaller than designed size
    app.check_and_auto_resize()?;
    eprintln!("[{:>6}ms] Auto-resize checked", startup_timer.elapsed().as_millis());
    tracing::info!("[STARTUP {:>6}ms] Auto-resize checked", startup_timer.elapsed().as_millis());

    // Apply saved terminal position if available (before entering raw mode)
    app.apply_terminal_position();
    eprintln!("[{:>6}ms] Terminal position applied", startup_timer.elapsed().as_millis());
    tracing::info!("[STARTUP {:>6}ms] Terminal position applied", startup_timer.elapsed().as_millis());

    eprintln!("[{:>6}ms] Starting UI...", startup_timer.elapsed().as_millis());
    tracing::info!("[STARTUP {:>6}ms] Starting UI...", startup_timer.elapsed().as_millis());
    app.run().await?;

    Ok(())
}

fn parse_sizes_arg(arg: Option<&str>) -> Vec<(u16, u16)> {
    let default = vec![(80, 24), (100, 30), (120, 40), (140, 40), (160, 50)];
    match arg {
        None => default,
        Some(s) if s.trim().is_empty() => default,
        Some(s) => s.split(',').filter_map(|p| parse_size(p.trim())).collect::<Vec<_>>()
    }
}

fn parse_size(s: &str) -> Option<(u16, u16)> {
    let (w, h) = s.split_once('x')?;
    let w = w.parse::<u16>().ok()?;
    let h = h.parse::<u16>().ok()?;
    Some((w, h))
}
