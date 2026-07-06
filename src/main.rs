//! VellumFE - Multi-frontend GemStone IV client
//!
//! Supports both TUI (ratatui) and GUI (egui) frontends with shared core logic.

mod clipboard;
mod cmdlist;
mod config;
mod core;
mod data;
mod frontend;
mod migrate;
mod network;
mod parser;
mod performance;
mod selection;
mod session_cache;
mod sound;
mod spell_abbrevs;
mod theme;
mod tts;
mod window_position;

use anyhow::{Context, Result};
use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;

#[derive(ClapParser)]
#[command(name = "vellum-fe")]
#[command(about = "Multi-frontend GemStone IV client", long_about = None)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Frontend to use
    #[arg(short, long, default_value = "tui")]
    frontend: FrontendType,

    /// Port number to connect to (overrides config.toml, default: 8000)
    #[arg(short, long)]
    port: Option<u16>,

    /// Host to connect to (overrides config.toml, default: 127.0.0.1)
    #[arg(long)]
    host: Option<String>,

    /// Character name (used for direct connection login)
    /// When using --direct, this is the character to log in as.
    /// For config directory, use --profile (defaults to --character if not specified).
    #[arg(long)]
    character: Option<String>,

    /// Profile name for config directory selection.
    /// Use this to separate config profiles from character login names.
    /// If not specified, falls back to --character for config directory.
    #[arg(long)]
    profile: Option<String>,

    /// Custom data directory (default: ~/.vellum-fe)
    /// Can also be set via VELLUM_FE_DIR environment variable
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,

    /// Connect directly without Lich
    #[arg(long)]
    direct: bool,

    /// Account name for direct connections
    #[arg(long, requires = "direct")]
    account: Option<String>,

    /// Password for direct connections (omit to be prompted securely)
    #[arg(long, requires = "direct")]
    password: Option<String>,

    /// Game world for direct connections
    /// GemStone IV: prime, platinum, shattered, test
    /// DragonRealms: dr, drplatinum, drfallen, drtest
    #[arg(long, value_enum, requires = "direct")]
    game: Option<DirectGameArg>,

    /// Disable sound system entirely (skip audio device initialization)
    #[arg(long, help = config::profiles::help::NOSOUND)]
    nosound: bool,

    /// Launch a saved launcher profile by name (from launcher.toml).
    /// Connection settings come from the profile; the password is resolved
    /// from the OS credential store, or prompted for if not saved.
    #[arg(long, value_name = "NAME", conflicts_with_all = ["direct", "key", "launcher"])]
    launch_profile: Option<String>,

    /// Open the graphical launcher (also the default when run with no arguments)
    #[arg(long)]
    launcher: bool,

    /// Login key for Lich proxy connections (provided by Lich as %key%)
    /// This key is sent to the game server for authentication when connecting via Lich
    #[arg(long)]
    key: Option<String>,

    /// Color rendering mode: direct (true color RGB) or slot (256-color palette)
    #[arg(long, value_enum)]
    color_mode: Option<config::ColorMode>,

    /// Enable the embedded web server on this port (overrides [web] in config.toml)
    #[arg(long, value_name = "PORT", help = config::profiles::help::WEB_PORT)]
    web_port: Option<u16>,

    /// Setup terminal palette on startup using .setpalette (use with --color-mode slot)
    #[arg(long, help = config::profiles::help::SETUP_PALETTE)]
    setup_palette: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum FrontendType {
    Tui,
    Gui,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum DirectGameArg {
    // GemStone IV
    Prime,
    Platinum,
    Shattered,
    Test,
    // DragonRealms
    Dr,
    DrPlatinum,
    DrFallen,
    DrTest,
}

impl DirectGameArg {
    fn code(self) -> &'static str {
        match self {
            // GemStone IV
            DirectGameArg::Prime => "GS3",
            DirectGameArg::Platinum => "GSX",
            DirectGameArg::Shattered => "GSF",
            DirectGameArg::Test => "GST",
            // DragonRealms
            DirectGameArg::Dr => "DR",
            DirectGameArg::DrPlatinum => "DRX",
            DirectGameArg::DrFallen => "DRF",
            DirectGameArg::DrTest => "DRT",
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Validate layout configuration
    ValidateLayout {
        /// Layout file to validate
        #[arg(value_name = "FILE")]
        layout: Option<PathBuf>,
    },

    /// Migrate old VellumFE layouts to current format
    MigrateLayout {
        /// Source directory containing old layout files
        #[arg(long, value_name = "DIR")]
        src: PathBuf,

        /// Output directory for migrated layouts (default: <src>/migrated)
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,

        /// Show what would be done without making changes
        #[arg(long)]
        dry_run: bool,

        /// Print detailed progress information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Import highlights from a Wrayth/StormFront settings XML file
    ImportHighlights {
        /// Wrayth settings XML file (e.g. 70682.xml)
        #[arg(value_name = "FILE")]
        src: PathBuf,

        /// Output TOML file (default: <FILE>-highlights.toml next to source)
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,

        /// Show what would be imported without writing anything
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    // Initialize logging to file (use RUST_LOG env var to control level, e.g. RUST_LOG=debug)
    // TUI apps can't log to stdout, so we write to a file in the config directory (~/.vellum-fe/)
    let log_dir = config::Config::base_dir()?;
    std::fs::create_dir_all(&log_dir)?;
    // Non-blocking appender: log writes go to a dedicated thread instead of
    // doing a syscall on the caller's thread. The guard must stay alive for
    // the duration of main so buffered lines flush on exit.
    let file_appender = tracing_appender::rolling::never(&log_dir, "vellum-fe.log");
    let (non_blocking, _log_guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(non_blocking)
        .with_ansi(false) // No color codes in log file
        .init();

    // Parse CLI arguments
    let mut cli = Cli::parse();

    // Handle subcommands
    if let Some(command) = cli.command {
        match command {
            Commands::ValidateLayout { layout } => {
                // Load the layout file
                let layout_result = if let Some(path) = layout {
                    println!("Validating layout file: {:?}", path);
                    config::Layout::load_from_file(&path)
                } else {
                    println!("Validating default layout");
                    config::Layout::load(cli.character.as_deref())
                };

                match layout_result {
                    Ok(layout) => {
                        if let Err(e) = layout.validate_and_print() {
                            eprintln!("✗ Validation failed: {}", e);
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Failed to load layout: {}", e);
                        std::process::exit(1);
                    }
                }

                return Ok(());
            }

            Commands::MigrateLayout {
                src,
                out,
                dry_run,
                verbose,
            } => {
                // Default output to <src>/migrated if not specified
                let out_dir = out.unwrap_or_else(|| src.join("migrated"));

                println!("VellumFE Layout Migration");
                println!("=========================");
                println!("Source:      {}", src.display());
                println!("Destination: {}", out_dir.display());
                if dry_run {
                    println!("Mode:        DRY RUN (no changes will be made)");
                }
                println!();

                let options = migrate::MigrateOptions {
                    src,
                    out: out_dir,
                    dry_run,
                    verbose,
                };

                match migrate::run_migration(&options) {
                    Ok(result) => {
                        println!();
                        println!("Migration Complete");
                        println!("------------------");
                        println!("  Converted: {}", result.succeeded);
                        println!("  Skipped:   {} (already current format)", result.skipped);
                        println!("  Failed:    {}", result.failed);

                        if !result.errors.is_empty() && verbose {
                            println!();
                            println!("Errors:");
                            for err in &result.errors {
                                println!("  - {}", err);
                            }
                        }

                        if result.failed > 0 {
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Migration failed: {}", e);
                        std::process::exit(1);
                    }
                }

                return Ok(());
            }

            Commands::ImportHighlights { src, out, dry_run } => {
                let xml = std::fs::read_to_string(&src)
                    .with_context(|| format!("Failed to read {}", src.display()))?;
                let result = config::wrayth_import::import_wrayth_settings(&xml)?;

                println!("Wrayth Highlight Import");
                println!("=======================");
                println!("Source: {}", src.display());
                println!();
                println!(
                    "  Strings:  {} imported ({} skipped)",
                    result.string_count - result.skipped,
                    result.skipped
                );
                println!(
                    "  Names:    {} merged into {} patterns (grouped by color)",
                    result.name_count, result.name_group_count
                );

                if !result.palette_misses.is_empty() {
                    println!(
                        "  Warning:  unresolved palette references (color dropped): {}",
                        result.palette_misses.join(", ")
                    );
                }
                if !result.sound_files.is_empty() {
                    let sounds_dir = config::Config::sounds_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "~/.vellum-fe/sounds".to_string());
                    println!();
                    println!("  Sounds referenced (copy these into {}):", sounds_dir);
                    for sound in &result.sound_files {
                        println!("    - {}", sound);
                    }
                }

                if dry_run {
                    println!();
                    println!("Dry run: no file written.");
                    return Ok(());
                }

                let out_path = out.unwrap_or_else(|| {
                    let stem = src
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("wrayth");
                    src.with_file_name(format!("{}-highlights.toml", stem))
                });
                let toml_str = config::wrayth_import::to_toml_string(&result.highlights)?;
                std::fs::write(&out_path, toml_str)
                    .with_context(|| format!("Failed to write {}", out_path.display()))?;

                println!();
                println!("Wrote {} highlights to {}", result.highlights.len(), out_path.display());
                if let Ok(global) = config::Config::common_highlights_path() {
                    println!(
                        "To activate for all characters, merge or copy it to {}",
                        global.display()
                    );
                }

                return Ok(());
            }
        }
    }

    // Set custom data directory if specified (via CLI or environment variable)
    if let Some(data_dir) = &cli.data_dir {
        std::env::set_var("VELLUM_FE_DIR", data_dir);
        tracing::info!("Using custom data directory: {:?}", data_dir);
    } else if let Ok(env_dir) = std::env::var("VELLUM_FE_DIR") {
        tracing::info!("Using data directory from VELLUM_FE_DIR: {}", env_dir);
    }

    // Launcher mode: explicit --launcher, or a bare double-click/no-args
    // start. Sessions are spawned from there as separate processes.
    if cli.launcher || std::env::args_os().len() <= 1 {
        #[cfg(windows)]
        detach_exclusive_console();
        return frontend::gui::launcher::run_launcher();
    }

    // Apply a saved launcher profile: fills the same fields the equivalent
    // CLI switches would have set (explicit CLI switches win over profile
    // values). Returns the resolved game code for direct connections.
    let profile_game_code = match cli.launch_profile.clone() {
        Some(name) => apply_launch_profile(&mut cli, &name)?,
        None => None,
    };

    // Load configuration
    // Profile (for config directory) uses --profile if specified, otherwise falls back to --character
    let profile = cli.profile.as_deref().or(cli.character.as_deref());
    let mut config = if let Some(config_path) = &cli.config {
        config::Config::load_from_path(config_path, profile, cli.port)?
    } else {
        config::Config::load_with_options(profile, cli.port)?
    };

    // Apply CLI flag overrides (CLI takes precedence over config.toml)
    if let Some(port) = cli.port {
        config.connection.port = port;
    }
    if let Some(ref host) = cli.host {
        config.connection.host = host.clone();
    }
    if cli.nosound {
        config.sound.enabled = false;
    }
    if let Some(mode) = cli.color_mode {
        config.ui.color_mode = mode;
    }
    if let Some(web_port) = cli.web_port {
        config.web.enabled = true;
        config.web.port = web_port;
    }
    // Store setup_palette flag for frontend to use after initialization
    let setup_palette = cli.setup_palette;

    // Build direct connection config if enabled
    // Uses --character for login (not --profile, which is only for config directory)
    let game_code_arg = cli
        .game
        .map(|g| g.code().to_string())
        .or(profile_game_code);
    let direct_config = network::DirectConnectConfig::from_cli(
        cli.direct,
        cli.account.clone(),
        cli.password.clone(),
        cli.character.clone(), // Character for direct connect login
        cli.character.clone(), // Fallback for character resolution
        game_code_arg.as_deref(),
        &config,
    )?;

    // Run appropriate frontend
    // Character is used for Lich proxy selection and display (not profile)
    let character = cli.character.clone();
    let login_key = cli.key.clone();
    match cli.frontend {
        FrontendType::Tui => {
            frontend::tui::run(config, character, direct_config, setup_palette, login_key)?
        }
        FrontendType::Gui => {
            #[cfg(windows)]
            detach_exclusive_console();
            run_gui(config, direct_config, login_key)?
        }
    }

    // Clean shutdown: drop this instance's entry from the web session
    // dashboard registry (no-op when the web server never ran).
    frontend::web::shutdown();

    Ok(())
}

/// Drop the console Windows auto-creates for a double-clicked console-
/// subsystem exe, so no empty black window sits behind the launcher/GUI.
/// Only detaches when this process is the console's sole owner - launching
/// from a terminal keeps that terminal attached (count > 1), so prompts and
/// --help output still work there.
#[cfg(windows)]
fn detach_exclusive_console() {
    use windows::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
    // SAFETY: plain Win32 queries; a 2-slot buffer suffices because only
    // "exactly one attached process" matters.
    unsafe {
        let mut pids = [0u32; 2];
        if GetConsoleProcessList(&mut pids) == 1 {
            let _ = FreeConsole();
        }
    }
}

/// Apply a saved launcher profile onto the parsed CLI arguments.
///
/// Fills only fields the user did not set explicitly, so switches passed
/// alongside `--launch-profile` still win. Password resolution order:
/// explicit CLI/env handoff from the launcher → OS credential store →
/// (later, in `DirectConnectConfig::from_cli`) interactive prompt.
///
/// Returns the game code ("GS3", "DRX", ...) for direct profiles.
fn apply_launch_profile(cli: &mut Cli, name: &str) -> Result<Option<String>> {
    use config::profiles::{self, LaunchFrontend, LaunchMode, LauncherStore};

    let store = LauncherStore::load()?;
    let profile = store
        .find(name)
        .with_context(|| {
            let path = LauncherStore::path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "launcher.toml".to_string());
            format!("Launcher profile '{}' not found in {}", name, path)
        })?
        .clone();

    // Password handed off by the launcher process (only used for GUI
    // sessions whose password is not in the credential store). Consume it
    // immediately so it does not linger in this process's environment.
    let env_password = std::env::var(profiles::PASSWORD_ENV).ok();
    std::env::remove_var(profiles::PASSWORD_ENV);

    let mut game_code = None;
    match profile.mode {
        LaunchMode::Direct => {
            cli.direct = true;
            if cli.account.is_none() {
                cli.account = Some(profile.account.clone());
            }
            if cli.password.is_none() {
                cli.password = env_password.or_else(|| {
                    if profile.password_saved {
                        profiles::load_password(&profile.account)
                    } else {
                        None
                    }
                });
            }
            if !profile.game.is_empty() {
                game_code = Some(
                    network::DirectConnectConfig::game_name_to_code(&profile.game).to_string(),
                );
            }
        }
        LaunchMode::Lich => {
            if cli.host.is_none() {
                cli.host = Some(profile.host.clone());
            }
            if cli.port.is_none() {
                cli.port = Some(profile.port);
            }
        }
    }

    if cli.character.is_none() && !profile.character.is_empty() {
        cli.character = Some(profile.character.clone());
    }
    if cli.profile.is_none() {
        cli.profile = profile.settings_profile.clone();
    }
    if cli.web_port.is_none() {
        cli.web_port = profile.web_port;
    }
    cli.nosound |= profile.nosound;
    cli.setup_palette |= profile.setup_palette;
    if cli.color_mode.is_none() {
        if let Some(mode) = profile.color_mode.as_deref() {
            match <config::ColorMode as clap::ValueEnum>::from_str(mode, true) {
                Ok(parsed) => cli.color_mode = Some(parsed),
                Err(_) => {
                    tracing::warn!("Ignoring unknown color_mode '{}' in launcher profile", mode)
                }
            }
        }
    }
    cli.frontend = match profile.frontend {
        LaunchFrontend::Gui => FrontendType::Gui,
        LaunchFrontend::Tui => FrontendType::Tui,
    };
    if cli.data_dir.is_none() {
        if let Some(dir) = profile.data_dir.as_deref().filter(|d| !d.is_empty()) {
            std::env::set_var("VELLUM_FE_DIR", dir);
            tracing::info!("Using data directory from launcher profile: {}", dir);
        }
    }

    Ok(game_code)
}

/// Run GUI frontend
fn run_gui(
    config: config::Config,
    direct: Option<network::DirectConnectConfig>,
    login_key: Option<String>,
) -> Result<()> {
    use core::AppCore;
    use frontend::EguiApp;

    // Create core application state
    let app_core = AppCore::new(config)?;

    // Create and run GUI
    let app = EguiApp::new(app_core, direct, login_key);
    app.run()?;

    Ok(())
}
