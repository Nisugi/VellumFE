//! VellumFE - Multi-frontend GemStone IV client
//!
//! Supports both TUI (ratatui) and GUI (egui) frontends with shared core logic.

mod clipboard;
mod cmdlist;
mod config;
mod core;
mod data;
mod frontend;
mod launcher;
mod migrate;
mod network;
mod parser;
mod performance;
mod platform;
mod process_probe;
mod selection;
mod session_cache;
mod sound;
mod spell_abbrevs;
mod theme;
mod tts;
mod webui;
mod window_position;

use anyhow::{Context, Result};
use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;

#[derive(ClapParser)]
#[command(name = "vellum-fe")]
#[command(version)]
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
    /// DragonRealms: dr, dr-platinum, dr-fallen, dr-test
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

    /// Internal launcher-to-child guard against profile edits between
    /// confirmation and child startup.
    #[arg(long, value_name = "DIGEST", requires = "launch_profile", hide = true)]
    launch_target_fingerprint: Option<String>,

    /// Open the graphical launcher (also the default when run with no arguments)
    #[arg(long)]
    launcher: bool,

    /// Login key for Lich proxy connections (provided by Lich as %key%)
    /// This key is sent to the game server for authentication when connecting via Lich
    #[arg(long)]
    key: Option<String>,

    /// Color rendering mode: direct (true color RGB), slot (256-color custom palette), or indexed (256-color standard palette)
    #[arg(long, value_enum)]
    color_mode: Option<config::ColorMode>,

    /// Enable the embedded web server on this port (overrides [web] in config.toml)
    #[arg(long, value_name = "PORT", help = config::profiles::help::WEB_PORT)]
    web_port: Option<u16>,

    /// Address the web server binds to (overrides [web] bind in config.toml)
    #[arg(long, value_name = "ADDR", help = config::profiles::help::WEB_BIND)]
    web_bind: Option<String>,

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
    /// Core + web server only, no local UI — a browser at /play or /despana
    /// (or the Android WebView shell at /play) is the interface.
    Headless,
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
    /// Vellum Studio: standalone art authoring — calibrate pool frames and
    /// creature sprites without launching the game
    Studio,

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

    /// Maintainer tool: extract curated base-map membership (uid rosters
    /// only, no coordinates) from a local Saga install. Use --out
    /// defaults/curated_maps.toml to refresh the file shipped with the app;
    /// the no --out form writes a user-side override instead. End users
    /// never need this — the shipped rosters are built in.
    ExtractCuratedMaps {
        /// Saga resources directory (default: auto-detect the stock install)
        #[arg(long, value_name = "DIR")]
        saga_dir: Option<PathBuf>,

        /// Output TOML file (default: ~/.vellum-fe/global/data/curated_maps.toml)
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,

        /// Show what would be extracted without writing anything
        #[arg(long)]
        dry_run: bool,
    },

    /// Maintainer tool: bake defaults/bestiary.json from the lich-5
    /// creature templates plus Saga's spawn tables (joined by creature
    /// name, spawn uids resolved through the curated maps). End users
    /// never need this — the shipped bestiary is built in.
    ExtractBestiary {
        /// lich-5 creatures directory (the schema-v3 .rb templates)
        #[arg(long, value_name = "DIR")]
        creatures_dir: PathBuf,

        /// Saga resources directory holding map-data/prime/creatures.json
        /// (omit to skip the spawn join)
        #[arg(long, value_name = "DIR")]
        saga_dir: Option<PathBuf>,

        /// Output file (default: defaults/bestiary.json)
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,

        /// Show what would be extracted without writing anything
        #[arg(long)]
        dry_run: bool,
    },

    /// Validate a skin pack (a zip or an unzipped pack directory): every
    /// assignment must resolve to art inside the pack, embedded/sidecar
    /// metadata must parse per its category's schema. Exit 1 on errors.
    /// Used by vellum-assets CI on submissions.
    ValidateSkin {
        /// Skin pack zip file, or a directory laid out like one
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },

    /// Convert a legacy live-manifest skin (global/skins/<NAME>/) into a
    /// shareable skin pack zip. Maps doll + calibration, compass, frames,
    /// default background/border, status icons, edges and control faces;
    /// prints what has no pack equivalent. The legacy skin is not touched.
    MigrateSkin {
        /// Installed skin name (a directory under global/skins/)
        #[arg(value_name = "NAME")]
        skin: String,

        /// Output zip (default: <config>/exports/<NAME>-skin.zip)
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
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

    // Write panics to the log file synchronously: the non-blocking appender's
    // flush thread may not survive the crash, and GUI/TUI builds have no
    // visible stderr, so this is the only durable record of a panic.
    let panic_log_path = log_dir.join("vellum-fe.log");
    let default_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let message = format!("PANIC: {info}\nbacktrace:\n{backtrace}");
        tracing::error!("{message}");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&panic_log_path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{message}");
        }
        default_panic_hook(info);
    }));

    // Parse CLI arguments
    let mut cli = Cli::parse();

    // Custom data directory FIRST — subcommands (studio, validate-skin,
    // migrate-skin, …) read the pool/config through the same resolver, so
    // --data-dir must land in the env var before any of them dispatch.
    if let Some(data_dir) = &cli.data_dir {
        std::env::set_var("VELLUM_FE_DIR", data_dir);
        tracing::info!("Using custom data directory: {:?}", data_dir);
    } else if let Ok(env_dir) = std::env::var("VELLUM_FE_DIR") {
        tracing::info!("Using data directory from VELLUM_FE_DIR: {}", env_dir);
    }

    // Handle subcommands
    if let Some(command) = cli.command {
        match command {
            Commands::Studio => {
                #[cfg(feature = "gui")]
                {
                    #[cfg(windows)]
                    detach_exclusive_console();
                    return frontend::gui::studio::run_studio();
                }
                #[cfg(not(feature = "gui"))]
                {
                    eprintln!(
                        "✗ This build has no GUI support; Vellum Studio needs the gui feature"
                    );
                    std::process::exit(1);
                }
            }

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
                        for entry in &layout.unknown_windows {
                            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let widget_type = entry
                                .get("widget_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            eprintln!(
                                "! Window '{}' skipped: widget type '{}' not supported by this build",
                                name, widget_type
                            );
                        }
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

            Commands::ValidateSkin { path } => {
                println!("Validating skin pack: {}", path.display());
                let pack = if path.is_dir() {
                    config::skin_pack::read_pack_dir(&path)
                } else {
                    match std::fs::read(&path) {
                        Ok(bytes) => config::skin_pack::read_pack_bytes(&bytes),
                        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
                    }
                };
                let pack = match pack {
                    Ok(pack) => pack,
                    Err(e) => {
                        eprintln!("✗ {e}");
                        std::process::exit(1);
                    }
                };
                let findings = config::skin_pack::validate(&pack);
                for warning in &findings.warnings {
                    println!("! {warning}");
                }
                for error in &findings.errors {
                    eprintln!("✗ {error}");
                }
                if findings.ok() {
                    println!(
                        "✓ '{}' is valid: {} file(s), format {}",
                        pack.manifest.meta.name,
                        pack.files.len(),
                        pack.manifest.format
                    );
                } else {
                    eprintln!(
                        "✗ {} error(s), {} warning(s)",
                        findings.errors.len(),
                        findings.warnings.len()
                    );
                    std::process::exit(1);
                }
                return Ok(());
            }

            Commands::MigrateSkin { skin, out } => {
                println!("Migrating legacy skin '{skin}' to a skin pack");
                let (pack, warnings) = match config::skin_pack::migrate_legacy(&skin) {
                    Ok(result) => result,
                    Err(e) => {
                        eprintln!("✗ {e:#}");
                        std::process::exit(1);
                    }
                };
                for warning in &warnings {
                    println!("! {warning}");
                }
                let findings = config::skin_pack::validate(&pack);
                for warning in &findings.warnings {
                    println!("! {warning}");
                }
                if !findings.ok() {
                    for error in &findings.errors {
                        eprintln!("✗ {error}");
                    }
                    std::process::exit(1);
                }
                let dest = match out {
                    Some(path) => path,
                    None => {
                        let dir = config::Config::base_dir()?.join("exports");
                        std::fs::create_dir_all(&dir)?;
                        dir.join(format!("{skin}-skin.zip"))
                    }
                };
                if let Err(e) = config::skin_pack::write_pack_zip(&pack, &dest) {
                    eprintln!("✗ {e:#}");
                    std::process::exit(1);
                }
                println!(
                    "✓ wrote {} ({} file(s)) — install with .importskin, or share it",
                    dest.display(),
                    pack.files.len()
                );
                return Ok(());
            }

            Commands::ExtractCuratedMaps {
                saga_dir,
                out,
                dry_run,
            } => {
                use crate::core::curated_maps;

                println!("Curated Map Membership Extraction");
                println!("=================================");
                let layouts_json = curated_maps::find_saga_layouts(saga_dir.as_deref()).context(
                    "No Saga install found. Pass --saga-dir <resources dir> \
                         or set SAGA_RESOURCES_DIR",
                )?;
                println!("Source: {}", layouts_json.display());

                let extracted = curated_maps::extract_from_saga(&layouts_json)?;
                println!(
                    "Extracted {} maps covering {} rooms (Saga layoutVersion {})",
                    extracted.maps.len(),
                    extracted.coverage_len(),
                    extracted
                        .source_layout_version
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "?".into()),
                );

                let out_path = match out {
                    Some(path) => path,
                    None => config::Config::global_data_dir()?.join("curated_maps.toml"),
                };

                // Merge over any existing snapshot so maps Saga has since
                // retired aren't dropped.
                let mut snapshot = match std::fs::read_to_string(&out_path) {
                    Ok(text) => {
                        let existing =
                            curated_maps::CuratedMaps::from_toml(&text).with_context(|| {
                                format!(
                                    "existing {} is corrupt; move it aside or pass --out",
                                    out_path.display()
                                )
                            })?;
                        println!(
                            "Merging over existing snapshot ({} maps)",
                            existing.maps.len()
                        );
                        existing
                    }
                    Err(_) => curated_maps::CuratedMaps::default(),
                };
                snapshot.merge_from(extracted);
                snapshot.version = curated_maps::SNAPSHOT_VERSION;

                if dry_run {
                    println!(
                        "DRY RUN — would write {} maps to {}",
                        snapshot.maps.len(),
                        out_path.display()
                    );
                    for (slug, map) in &snapshot.maps {
                        println!("  {:<44} {:>5} rooms  ({})", slug, map.uids.len(), map.name);
                    }
                } else {
                    config::write_atomic(&out_path, snapshot.to_toml()?)
                        .with_context(|| format!("Failed to write {}", out_path.display()))?;
                    println!(
                        "Wrote {} maps to {}",
                        snapshot.maps.len(),
                        out_path.display()
                    );
                }
                return Ok(());
            }

            Commands::ExtractBestiary {
                creatures_dir,
                saga_dir,
                out,
                dry_run,
            } => {
                use crate::core::bestiary;

                println!("Bestiary Extraction");
                println!("===================");
                let (mut entries, failed) = bestiary::extract_from_lich(&creatures_dir)?;
                println!(
                    "Parsed {} templates from {}",
                    entries.len(),
                    creatures_dir.display()
                );
                if !failed.is_empty() {
                    println!("FAILED to parse {} templates:", failed.len());
                    for f in &failed {
                        println!("  {f}");
                    }
                }

                if let Some(saga) = saga_dir {
                    let creatures_json = saga.join("map-data").join("prime").join("creatures.json");
                    let text = std::fs::read_to_string(&creatures_json)
                        .with_context(|| format!("reading {}", creatures_json.display()))?;
                    let curated = core::curated_maps::CuratedMaps::embedded().unwrap_or_default();
                    let unmatched = bestiary::join_spawns(&mut entries, &text, &curated)?;
                    let with_spawns = entries.iter().filter(|e| !e.spawns.is_empty()).count();
                    println!(
                        "Spawn join: {} entries located, {} spawn names had no template",
                        with_spawns,
                        unmatched.len()
                    );
                    if dry_run {
                        for name in &unmatched {
                            println!("  no template: {name}");
                        }
                    }
                }

                let out_path = out.unwrap_or_else(|| PathBuf::from("defaults/bestiary.json"));
                if dry_run {
                    println!(
                        "DRY RUN — would write {} entries to {}",
                        entries.len(),
                        out_path.display()
                    );
                } else {
                    let file = bestiary::BestiaryFile {
                        version: bestiary::FILE_VERSION,
                        entries,
                    };
                    config::write_atomic(&out_path, serde_json::to_string_pretty(&file)?)
                        .with_context(|| format!("Failed to write {}", out_path.display()))?;
                    println!(
                        "Wrote {} entries to {}",
                        file.entries.len(),
                        out_path.display()
                    );
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
                    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("wrayth");
                    src.with_file_name(format!("{}-highlights.toml", stem))
                });
                let toml_str = config::wrayth_import::to_toml_string(&result.highlights)?;
                std::fs::write(&out_path, toml_str)
                    .with_context(|| format!("Failed to write {}", out_path.display()))?;

                println!();
                println!(
                    "Wrote {} highlights to {}",
                    result.highlights.len(),
                    out_path.display()
                );
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

    // (custom data directory was applied before subcommand dispatch)

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
    let applied_profile = match cli.launch_profile.clone() {
        Some(name) => apply_launch_profile(&mut cli, &name)?,
        None => AppliedLaunchProfile::default(),
    };
    let _endpoint_lease = if let Some(identity) = applied_profile.registry_identity.clone() {
        core::session_registry::set_launch_identity(identity.clone());
        Some(
            core::session_registry::acquire_endpoint_lease(&identity).with_context(|| {
                format!("Could not start launcher profile '{}'", identity.profile)
            })?,
        )
    } else {
        None
    };

    // Load configuration
    // Profile (for config directory) uses --profile if specified, otherwise falls back to --character
    let profile = cli.profile.as_deref().or(cli.character.as_deref());
    let mut config = if let Some(config_path) = &cli.config {
        config::Config::load_from_path(config_path, profile, cli.port)?
    } else {
        config::Config::load_with_options(profile, cli.port)?
    };

    // Fold legacy `<set>_<role>` pool art into set folders, once, before any
    // frontend reads the pool. Set art (compass, statusicons) installs and
    // lists as one unit now; this brings art installed under the old flat
    // layout along. Returns the pool-path rewrites so saved per-image
    // references can be fixed up to match.
    let pool_rewrites = config::pool::migrate_sets();

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
    if let Some(web_bind) = cli.web_bind.as_deref() {
        // Setting a bind address implies wanting the web server, same as
        // --web-port. Without this, a profile that set only web_bind="0.0.0.0"
        // (default port) would apply the address but never start the server.
        config.web.enabled = true;
        config.web.bind = web_bind.to_string();
    }
    // Store setup_palette flag for frontend to use after initialization
    let setup_palette = cli.setup_palette;

    // Build direct connection config if enabled
    // Uses --character for login (not --profile, which is only for config directory)
    let game_code_arg = cli
        .game
        .map(|g| g.code().to_string())
        .or(applied_profile.game_code);
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
            // Launcher-spawned sessions own their console window, so the TUI
            // restores/saves its size; manual runs leave the terminal alone.
            let console_size_profile = cli.launch_profile.is_some().then(|| {
                cli.profile
                    .clone()
                    .or_else(|| cli.character.clone())
                    .unwrap_or_else(|| "default".to_string())
            });
            frontend::tui::run(
                config,
                character,
                direct_config,
                setup_palette,
                login_key,
                console_size_profile,
            )?
        }
        FrontendType::Gui => {
            #[cfg(windows)]
            detach_exclusive_console();
            run_gui(config, direct_config, login_key)?
        }
        FrontendType::Headless => {
            if let Some(web_client) = applied_profile.web_client {
                frontend::headless::run_launcher_web_client(
                    config,
                    character,
                    direct_config,
                    login_key,
                    web_client,
                    applied_profile.auto_connect_lich,
                    applied_profile.registry_identity.clone().expect(
                        "a launcher web client always comes from an applied launch profile",
                    ),
                )?
            } else {
                frontend::headless::run(config, character, direct_config, login_key)?
            }
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

#[derive(Debug, Default)]
struct AppliedLaunchProfile {
    game_code: Option<String>,
    /// Private launcher-to-runtime intent. Browser clients remain profile
    /// choices rather than public `--frontend` values, so ordinary and
    /// embedded headless starts keep their existing login-screen behavior.
    web_client: Option<config::profiles::LaunchWebClient>,
    /// A saved Lich profile supplies an attach target without needing a fake
    /// one-shot login key to trigger the headless runtime's auto-connect path.
    auto_connect_lich: bool,
    /// Immutable identity captured from the exact saved profile loaded by
    /// this child.  The runtime registry publishes it without consulting the
    /// launcher store again.
    registry_identity: Option<core::session_registry::SessionLaunchIdentity>,
}

fn apply_profile_frontend(
    cli: &mut Cli,
    profile: &config::profiles::LauncherProfile,
) -> Option<config::profiles::LaunchWebClient> {
    use config::profiles::LaunchFrontend;

    if let Some(web_client) = profile.web_client {
        cli.frontend = FrontendType::Headless;
        Some(web_client)
    } else {
        cli.frontend = match profile.frontend {
            LaunchFrontend::Gui => FrontendType::Gui,
            LaunchFrontend::Tui => FrontendType::Tui,
        };
        None
    }
}

fn profile_auto_connects_lich(
    profile: &config::profiles::LauncherProfile,
    web_client: Option<config::profiles::LaunchWebClient>,
) -> bool {
    web_client.is_some() && profile.mode == config::profiles::LaunchMode::Lich
}

/// Freeze the connection identity after profile defaults and documented CLI
/// overrides have been combined. The endpoint lease, live registry, and
/// headless retarget guard must describe the connection the child will
/// actually use, not the saved profile values that were overridden.
fn effective_launch_identity(
    name: &str,
    profile: &config::profiles::LauncherProfile,
    cli: &Cli,
) -> core::session_registry::SessionLaunchIdentity {
    let mut effective = profile.clone();
    if let Some(character) = cli.character.as_deref() {
        effective.character = character.to_string();
    }
    match profile.mode {
        config::profiles::LaunchMode::Direct => {
            if let Some(account) = cli.account.as_deref() {
                effective.account = account.to_string();
            }
            if let Some(game) = cli.game {
                effective.game = game.code().to_string();
            }
        }
        config::profiles::LaunchMode::Lich => {
            if let Some(host) = cli.host.as_deref() {
                effective.host = host.to_string();
            }
            if let Some(port) = cli.port {
                effective.port = port;
            }
        }
    }
    core::session_registry::SessionLaunchIdentity::from_profile(name, &effective)
}

/// Apply a saved launcher profile onto the parsed CLI arguments.
///
/// Fills only fields the user did not set explicitly, so switches passed
/// alongside `--launch-profile` still win. Password resolution order:
/// explicit CLI/env handoff from the launcher → OS credential store →
/// (later, in `DirectConnectConfig::from_cli`) interactive prompt.
///
/// Returns the resolved game code plus any private browser-client launch intent.
fn apply_launch_profile(cli: &mut Cli, name: &str) -> Result<AppliedLaunchProfile> {
    use config::profiles::{self, LaunchMode, LauncherStore};

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
            anyhow::ensure!(
                !profile.character.trim().is_empty(),
                "Direct launcher profile '{}' has no character; edit the saved connection before launching",
                name
            );
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
            // A blank game can survive from an old or hand-edited launcher
            // profile. Treat it exactly like a newly-created profile: Prime.
            // Do this here so the child cannot inherit an unrelated game from
            // its per-character config after the launcher already claimed the
            // profile's canonical Prime identity.
            game_code =
                Some(network::DirectConnectConfig::game_name_to_code(&profile.game).to_string());
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
    if cli.web_bind.is_none() {
        cli.web_bind = profile.web_bind.clone();
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
    let web_client = apply_profile_frontend(cli, &profile);
    let auto_connect_lich = profile_auto_connects_lich(&profile, web_client);
    if cli.data_dir.is_none() {
        if let Some(dir) = profile.data_dir.as_deref().filter(|d| !d.is_empty()) {
            profiles::remember_launcher_root(&config::Config::base_dir()?);
            std::env::set_var("VELLUM_FE_DIR", dir);
            tracing::info!("Using data directory from launcher profile: {}", dir);
        }
    }

    let registry_identity = effective_launch_identity(name, &profile, cli);
    if let Some(expected) = cli.launch_target_fingerprint.as_deref() {
        let actual = launcher::session_lifecycle::launch_identity_fingerprint(&registry_identity);
        anyhow::ensure!(
            actual.eq_ignore_ascii_case(expected),
            "Launcher profile '{}' changed after launch confirmation; the confirmed target was not launched",
            name
        );
    }
    Ok(AppliedLaunchProfile {
        game_code,
        web_client,
        auto_connect_lich,
        registry_identity: Some(registry_identity),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profiles::{LaunchFrontend, LaunchMode, LaunchWebClient, LauncherProfile};

    fn cli() -> Cli {
        Cli::try_parse_from(["vellum-fe", "--frontend", "tui"]).expect("test CLI")
    }

    #[test]
    fn despana_lich_profile_maps_to_private_headless_web_client() {
        let mut profile = LauncherProfile::new_direct();
        profile.mode = LaunchMode::Lich;
        profile.select_web_client(LaunchWebClient::Despana);
        let mut cli = cli();

        let web_client = apply_profile_frontend(&mut cli, &profile).expect("web client");

        assert!(matches!(cli.frontend, FrontendType::Headless));
        assert_eq!(web_client, LaunchWebClient::Despana);
        assert!(profile_auto_connects_lich(&profile, Some(web_client)));
    }

    #[test]
    fn despana_direct_profile_maps_to_private_headless_web_client() {
        let mut profile = LauncherProfile::new_direct();
        profile.select_web_client(LaunchWebClient::Despana);
        let mut cli = cli();

        let web_client = apply_profile_frontend(&mut cli, &profile).expect("web client");

        assert!(matches!(cli.frontend, FrontendType::Headless));
        assert_eq!(web_client, LaunchWebClient::Despana);
        assert!(!profile_auto_connects_lich(&profile, Some(web_client)));
    }

    #[test]
    fn native_profile_frontends_preserve_their_existing_behavior() {
        let mut profile = LauncherProfile::new_direct();
        let mut cli = cli();

        profile.select_frontend(LaunchFrontend::Gui);
        assert_eq!(apply_profile_frontend(&mut cli, &profile), None);
        assert!(matches!(cli.frontend, FrontendType::Gui));

        profile.select_frontend(LaunchFrontend::Tui);
        assert_eq!(apply_profile_frontend(&mut cli, &profile), None);
        assert!(matches!(cli.frontend, FrontendType::Tui));
    }

    #[test]
    fn launch_identity_uses_effective_lich_cli_overrides() {
        use crate::core::session_registry::{SessionConnectionIdentity, SessionLaunchIdentity};

        let mut profile = LauncherProfile::new_direct();
        profile.name = "Calvix".to_string();
        profile.mode = LaunchMode::Lich;
        profile.character = "SavedName".to_string();
        profile.host = "127.0.0.1".to_string();
        profile.port = 8000;
        let mut cli = cli();
        cli.character = Some("OverrideName".to_string());
        cli.host = Some("lich.example.test".to_string());
        cli.port = Some(8111);

        assert_eq!(
            effective_launch_identity("Calvix", &profile, &cli),
            SessionLaunchIdentity {
                profile: "Calvix".to_string(),
                character: "OverrideName".to_string(),
                connection: SessionConnectionIdentity::Lich {
                    host: "lich.example.test".to_string(),
                    port: 8111,
                },
            }
        );
    }

    #[test]
    fn blank_direct_profile_game_defaults_to_prime_at_profile_application() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = tempfile::tempdir().expect("temp config root");
        let original_root = std::env::var_os("VELLUM_FE_DIR");
        std::env::set_var("VELLUM_FE_DIR", root.path());
        let mut profile = LauncherProfile::new_direct();
        profile.name = "Aster".to_string();
        profile.account = "Account".to_string();
        profile.character = "Aster".to_string();
        profile.game.clear();
        let store = crate::config::profiles::LauncherStore {
            profiles: vec![profile],
            ..Default::default()
        };
        store.save().expect("save launcher profile");

        let mut args = cli();
        let applied = apply_launch_profile(&mut args, "Aster").expect("apply profile");
        let mut config = config::Config::default();
        config.connection.game = Some("platinum".to_string());
        let resolved = network::DirectConnectConfig::from_cli(
            args.direct,
            args.account,
            Some("password".to_string()),
            args.character.clone(),
            args.character,
            applied.game_code.as_deref(),
            &config,
        )
        .expect("resolve direct config")
        .expect("direct config enabled");

        match original_root {
            Some(path) => std::env::set_var("VELLUM_FE_DIR", path),
            None => std::env::remove_var("VELLUM_FE_DIR"),
        }
        assert_eq!(applied.game_code.as_deref(), Some("GS3"));
        assert_eq!(resolved.game_code, "GS3");
        assert!(matches!(
            applied
                .registry_identity
                .expect("launcher profile identity")
                .connection,
            crate::core::session_registry::SessionConnectionIdentity::Direct { game, .. }
                if game == "gs3"
        ));
    }

    #[test]
    fn profile_data_dir_keeps_nondefault_launcher_root_and_sibling_roots_discoverable() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = tempfile::tempdir().expect("temporary roots");
        let launcher_root = root.path().join("launcher");
        let selected_root = root.path().join("selected");
        let sibling_root = root.path().join("sibling");
        let original_data_root = std::env::var_os("VELLUM_FE_DIR");
        let original_launcher_root = std::env::var_os(crate::config::profiles::LAUNCHER_ROOT_ENV);
        std::env::set_var("VELLUM_FE_DIR", &launcher_root);
        std::env::remove_var(crate::config::profiles::LAUNCHER_ROOT_ENV);

        let mut selected = LauncherProfile::new_direct();
        selected.name = "Aster".to_string();
        selected.account = "Account".to_string();
        selected.character = "Aster".to_string();
        selected.data_dir = Some(selected_root.to_string_lossy().into_owned());
        let mut sibling = LauncherProfile::new_direct();
        sibling.name = "Briar".to_string();
        sibling.account = "Account".to_string();
        sibling.character = "Briar".to_string();
        sibling.data_dir = Some(sibling_root.to_string_lossy().into_owned());
        crate::config::profiles::LauncherStore {
            profiles: vec![selected, sibling],
            ..Default::default()
        }
        .save()
        .expect("save launcher profiles");

        apply_launch_profile(&mut cli(), "Aster").expect("apply selected profile");
        let effective_root = config::Config::base_dir().expect("effective profile root");
        let known_roots = crate::config::profiles::known_data_roots();

        match original_data_root {
            Some(path) => std::env::set_var("VELLUM_FE_DIR", path),
            None => std::env::remove_var("VELLUM_FE_DIR"),
        }
        match original_launcher_root {
            Some(path) => std::env::set_var(crate::config::profiles::LAUNCHER_ROOT_ENV, path),
            None => std::env::remove_var(crate::config::profiles::LAUNCHER_ROOT_ENV),
        }

        assert_eq!(effective_root, selected_root);
        assert!(known_roots.contains(&launcher_root));
        assert!(known_roots.contains(&selected_root));
        assert!(known_roots.contains(&sibling_root));
    }

    #[test]
    fn blank_direct_profile_character_is_rejected_before_identity_or_login_resolution() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = tempfile::tempdir().expect("temp config root");
        let original_root = std::env::var_os("VELLUM_FE_DIR");
        std::env::set_var("VELLUM_FE_DIR", root.path());

        let mut profile = LauncherProfile::new_direct();
        profile.name = "Aster".to_string();
        profile.account = "Account".to_string();
        profile.character = "  ".to_string();
        crate::config::profiles::LauncherStore {
            profiles: vec![profile],
            ..Default::default()
        }
        .save()
        .expect("save malformed launcher profile");

        let error = apply_launch_profile(&mut cli(), "Aster").unwrap_err();

        match original_root {
            Some(path) => std::env::set_var("VELLUM_FE_DIR", path),
            None => std::env::remove_var("VELLUM_FE_DIR"),
        }
        assert!(error.to_string().contains("has no character"));
    }

    #[test]
    fn child_rejects_profile_edited_after_launcher_confirmation() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = tempfile::tempdir().expect("temp config root");
        let original_root = std::env::var_os("VELLUM_FE_DIR");
        std::env::set_var("VELLUM_FE_DIR", root.path());

        let mut confirmed = LauncherProfile::new_direct();
        confirmed.name = "Aster".to_string();
        confirmed.account = "Account".to_string();
        confirmed.character = "Aster".to_string();
        let fingerprint = launcher::session_lifecycle::launch_target_fingerprint(&confirmed);

        let mut edited = confirmed.clone();
        edited.character = "Briar".to_string();
        crate::config::profiles::LauncherStore {
            profiles: vec![edited],
            ..Default::default()
        }
        .save()
        .expect("save edited launcher profile");

        let mut args = cli();
        args.launch_target_fingerprint = Some(fingerprint);
        let error = apply_launch_profile(&mut args, "Aster").unwrap_err();

        match original_root {
            Some(path) => std::env::set_var("VELLUM_FE_DIR", path),
            None => std::env::remove_var("VELLUM_FE_DIR"),
        }
        assert!(error
            .to_string()
            .contains("changed after launch confirmation"));
    }
}
