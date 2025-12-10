//! Migration helper: convert VellumFE layout TOML files into VellumFE layouts.
//! Usage:
//!   cargo run --bin migrate_vellum_layouts --release -- --src "<vellum dir>" --out "<dest dir>" [--dry-run] [--verbose]
//! Notes:
//! - Attempts to map Vellum widget types/fields to VellumFE equivalents.
//! - Skips unsupported widgets (e.g., grouped "hands") with a warning.
//! - Uses filename suffix _{width}x{height}.toml to set terminal size if not present in the file.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::Deserialize;
use toml::Value;

use vellum_fe::config::{BorderSides, Config, Layout, TabbedTextTab, WindowBase, WindowDef};

fn main() -> Result<()> {
    let args = parse_args();
    if args.src.is_none() || args.out.is_none() {
        eprintln!("Usage: migrate_vellum_layouts --src <dir> --out <dir> [--dry-run] [--verbose]");
        return Ok(());
    }

    let src = args.src.as_ref().unwrap();
    let out = args.out.as_ref().unwrap();
    fs::create_dir_all(out).context("create output dir")?;

    let mut count = 0usize;
    for entry in fs::read_dir(src).context("read src dir")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "toml").unwrap_or(false) {
            if let Err(err) = process_file(&path, out, args.dry_run, args.verbose) {
                eprintln!("WARN: {}: {}", path.display(), err);
            } else {
                count += 1;
            }
        }
    }

    if args.verbose {
        eprintln!("Processed {} layouts", count);
    }
    Ok(())
}

#[derive(Default)]
struct CliArgs {
    src: Option<PathBuf>,
    out: Option<PathBuf>,
    dry_run: bool,
    verbose: bool,
}

fn parse_args() -> CliArgs {
    let mut args = CliArgs::default();
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--src" => args.src = iter.next().map(PathBuf::from),
            "--out" => args.out = iter.next().map(PathBuf::from),
            "--dry-run" => args.dry_run = true,
            "--verbose" => args.verbose = true,
            _ => {}
        }
    }
    args
}

#[derive(Debug, Deserialize)]
struct VellumLayout {
    #[serde(default)]
    terminal_width: Option<u16>,
    #[serde(default)]
    terminal_height: Option<u16>,
    #[serde(default)]
    windows: Vec<Value>,
}

fn process_file(path: &Path, out_dir: &Path, dry_run: bool, verbose: bool) -> Result<()> {
    let txt = fs::read_to_string(path).context("read file")?;
    let mut layout: VellumLayout = toml::from_str(&txt).context("parse toml")?;

    // Infer terminal size from filename if missing.
    if layout.terminal_width.is_none() || layout.terminal_height.is_none() {
        if let Some((w, h)) = infer_size_from_filename(path) {
            layout.terminal_width.get_or_insert(w);
            layout.terminal_height.get_or_insert(h);
        }
    }

    let mut out_layout = Layout {
        windows: Vec::new(),
        terminal_width: layout.terminal_width,
        terminal_height: layout.terminal_height,
        base_layout: None,
        theme: None,
    };

    for win_val in layout.windows {
        match convert_window(win_val, verbose) {
            Ok(Some(w)) => out_layout.windows.push(w),
            Ok(None) => {} // skipped
            Err(e) => eprintln!("WARN: {}: {}", path.display(), e),
        }
    }

    if dry_run {
        if verbose {
            eprintln!(
                "DRY-RUN converted {} windows from {}",
                out_layout.windows.len(),
                path.display()
            );
        }
        return Ok(());
    }

    let fname = path.file_name().ok_or_else(|| anyhow!("no filename"))?;
    let out_path = out_dir.join(fname);
    let toml = toml::to_string_pretty(&out_layout).context("serialize layout")?;
    fs::write(&out_path, toml).context("write output")?;
    if verbose {
        eprintln!(
            "Wrote {} windows to {}",
            out_layout.windows.len(),
            out_path.display()
        );
    }
    Ok(())
}

fn infer_size_from_filename(path: &Path) -> Option<(u16, u16)> {
    let stem = path.file_stem()?.to_string_lossy();
    let re = Regex::new(r".*_(\d+)x(\d+)$").ok()?;
    if let Some(caps) = re.captures(&stem) {
        let w = caps.get(1)?.as_str().parse().ok()?;
        let h = caps.get(2)?.as_str().parse().ok()?;
        return Some((w, h));
    }
    None
}

fn convert_window(win_val: Value, verbose: bool) -> Result<Option<WindowDef>> {
    let table = win_val
        .as_table()
        .ok_or_else(|| anyhow!("window entry is not a table"))?;

    let widget_type = get_str(table, "widget_type")?.unwrap_or_default();
    let name = get_str(table, "name")?.unwrap_or_else(|| widget_type.clone());

    // Map widget type/name to a target template name.
    let mapping = map_widget_type(&widget_type, &name, table)?;
    if mapping.skip {
        if verbose {
            eprintln!("Skipping widget '{}' ({})", name, widget_type);
        }
        return Ok(None);
    }
    let template_name = mapping.template_name;

    let mut window =
        Config::get_window_template(&template_name).ok_or_else(|| anyhow!("no template for {}", template_name))?;

    // Override base fields
    if let Some(base) = window.base_mut_opt() {
        if let Some(v) = get_u16(table, "row")? {
            base.row = v;
        }
        if let Some(v) = get_u16(table, "col")? {
            base.col = v;
        }
        if let Some(v) = get_u16(table, "rows")? {
            base.rows = v;
        }
        if let Some(v) = get_u16(table, "cols")? {
            base.cols = v;
        }
        if let Some(v) = get_bool(table, "show_border")? {
            base.show_border = v;
        }
        if let Some(v) = get_str(table, "border_style")? {
            base.border_style = v;
        }
        if let Some(v) = get_str(table, "border_color")? {
            base.border_color = Some(v);
        }
        if let Some(v) = get_bool(table, "transparent_background")? {
            base.transparent_background = v;
        }
        if let Some(v) = get_bool(table, "locked")? {
            base.locked = v;
        }
        if let Some(v) = get_str(table, "title")? {
            base.title = Some(v);
        }
        if let Some(v) = get_u16(table, "min_rows")? {
            base.min_rows = Some(v);
        }
        if let Some(v) = get_u16(table, "max_rows")? {
            base.max_rows = Some(v);
        }
        if let Some(v) = get_u16(table, "min_cols")? {
            base.min_cols = Some(v);
        }
        if let Some(v) = get_u16(table, "max_cols")? {
            base.max_cols = Some(v);
        }
        if let Some(v) = get_str(table, "content_align")? {
            base.content_align = Some(v);
        }
        if let Some(v) = get_str(table, "background_color")? {
            base.background_color = Some(v);
        }
        if let Some(v) = table.get("border_sides").and_then(|v| v.as_array()) {
            base.border_sides = parse_border_sides(v);
        }
        // Always set name from source layout.
        base.name = name.clone();
    }

    // Widget-specific mapping
    match &mut window {
        WindowDef::Text { data, .. } => {
            if let Some(v) = get_vec_str(table, "streams")? {
                data.streams = v;
            }
            if let Some(v) = get_usize(table, "buffer_size")? {
                data.buffer_size = v;
            }
            if let Some(v) = get_bool(table, "wordwrap")? {
                data.wordwrap = v;
            }
            if let Some(v) = get_bool(table, "show_timestamps")? {
                data.show_timestamps = v;
            }
        }
        WindowDef::TabbedText { data, .. } => {
            if let Some(v) = get_usize(table, "buffer_size")? {
                data.buffer_size = v;
            }
            if let Some(v) = get_str(table, "tab_bar_position")? {
                data.tab_bar_position = v;
            }
            if let Some(v) = get_str(table, "tab_unread_prefix")? {
                data.tab_unread_prefix = Some(v);
            }
            if let Some(v) = get_str(table, "tab_active_color")? {
                data.tab_active_color = Some(v);
            }
            if let Some(v) = get_str(table, "tab_inactive_color")? {
                data.tab_inactive_color = Some(v);
            }
            if let Some(v) = get_str(table, "tab_unread_color")? {
                data.tab_unread_color = Some(v);
            }
            // Tabs: tolerate either tab array or single stream (rare).
            if let Some(tabs) = table.get("tabs").and_then(|v| v.as_array()) {
                data.tabs = tabs
                    .iter()
                    .filter_map(|t| {
                        let t = t.as_table()?;
                        let name = get_str(t, "name").ok().flatten().unwrap_or_else(|| "tab".to_string());
                        let streams = get_vec_str(t, "streams").ok().flatten().unwrap_or_else(|| {
                            get_str(t, "stream").ok().flatten().map(|s| vec![s]).unwrap_or_default()
                        });
                        let show_timestamps = get_bool(t, "show_timestamps").ok().flatten();
                        let ignore_activity = get_bool(t, "ignore_activity").ok().flatten();
                        Some(TabbedTextTab {
                            name,
                            stream: None,
                            streams,
                            show_timestamps,
                            ignore_activity,
                        })
                    })
                    .collect();
            } else if let Some(v) = get_vec_str(table, "streams")? {
                data.tabs = vec![TabbedTextTab {
                    name: "Tab".to_string(),
                    stream: None,
                    streams: v,
                    show_timestamps: None,
                    ignore_activity: None,
                }];
            }
        }
        WindowDef::Progress { data, base } => {
            if let Some(v) = get_str(table, "bar_color")? {
                data.color = Some(v);
            }
            if let Some(v) = get_bool(table, "numbers_only")? {
                data.numbers_only = v;
            }
            if let Some(v) = get_str(table, "title")? {
                data.label = Some(v.clone());
                base.title = Some(v);
            }
            if let Some(v) = get_str(table, "bar_background_color")? {
                base.background_color = Some(v);
            }
        }
        WindowDef::Countdown { data, base } => {
            if let Some(v) = get_str(table, "title")? {
                data.label = Some(v.clone());
                base.title = Some(v);
            }
            if let Some(v) = get_str(table, "bar_color")? {
                base.background_color = Some(v);
            }
            if let Some(v) = get_char(table, "icon")? {
                data.icon = Some(v);
            }
        }
        WindowDef::Compass { data, .. } => {
            if let Some(v) = get_str(table, "compass_active_color")? {
                data.active_color = Some(v);
            }
            if let Some(v) = get_str(table, "compass_inactive_color")? {
                data.inactive_color = Some(v);
            }
        }
        WindowDef::InjuryDoll { data, .. } => {
            if let Some(v) = get_str(table, "injury_default_color")? {
                data.injury_default_color = Some(v);
            }
            if let Some(v) = get_str(table, "injury1_color")? {
                data.injury1_color = Some(v);
            }
            if let Some(v) = get_str(table, "injury2_color")? {
                data.injury2_color = Some(v);
            }
            if let Some(v) = get_str(table, "injury3_color")? {
                data.injury3_color = Some(v);
            }
            if let Some(v) = get_str(table, "scar1_color")? {
                data.scar1_color = Some(v);
            }
            if let Some(v) = get_str(table, "scar2_color")? {
                data.scar2_color = Some(v);
            }
            if let Some(v) = get_str(table, "scar3_color")? {
                data.scar3_color = Some(v);
            }
        }
        WindowDef::Dashboard { data, .. } => {
            if let Some(v) = get_str(table, "dashboard_layout")? {
                data.layout = v;
            }
            if let Some(v) = get_u16(table, "dashboard_spacing")? {
                data.spacing = v;
            }
            if let Some(v) = get_bool(table, "dashboard_hide_inactive")? {
                data.hide_inactive = v;
            }
        }
        WindowDef::ActiveEffects { data, .. } => {
            if let Some(v) = get_str(table, "effect_category")? {
                data.category = v;
            }
        }
        WindowDef::Performance { data, .. } => {
            if let Some(v) = get_bool(table, "show_fps")? {
                data.show_fps = v;
            }
            if let Some(v) = get_bool(table, "show_frame_times")? {
                data.show_frame_times = v;
            }
            if let Some(v) = get_bool(table, "show_render_times")? {
                data.show_render_times = v;
            }
            if let Some(v) = get_bool(table, "show_ui_times")? {
                data.show_ui_times = v;
            }
            if let Some(v) = get_bool(table, "show_wrap_times")? {
                data.show_wrap_times = v;
            }
            if let Some(v) = get_bool(table, "show_net")? {
                data.show_net = v;
            }
            if let Some(v) = get_bool(table, "show_parse")? {
                data.show_parse = v;
            }
            if let Some(v) = get_bool(table, "show_events")? {
                data.show_events = v;
            }
            if let Some(v) = get_bool(table, "show_memory")? {
                data.show_memory = v;
            }
            if let Some(v) = get_bool(table, "show_lines")? {
                data.show_lines = v;
            }
            if let Some(v) = get_bool(table, "show_uptime")? {
                data.show_uptime = v;
            }
            if let Some(v) = get_bool(table, "show_jitter")? {
                data.show_jitter = v;
            }
            if let Some(v) = get_bool(table, "show_frame_spikes")? {
                data.show_frame_spikes = v;
            }
            if let Some(v) = get_bool(table, "show_event_lag")? {
                data.show_event_lag = v;
            }
            if let Some(v) = get_bool(table, "show_memory_delta")? {
                data.show_memory_delta = v;
            }
        }
        WindowDef::Hand { base, .. } => {
            // Name already set above; no widget-specific fields.
            if let Some(v) = get_str(table, "title")? {
                base.title = Some(v);
            }
        }
        _ => {}
    }

    Ok(Some(window))
}

struct Mapping {
    template_name: String,
    skip: bool,
}

fn map_widget_type(widget_type: &str, name: &str, table: &toml::value::Table) -> Result<Mapping> {
    // Canonicalize common template name variants
    let canonical_name = |n: &str| -> String {
        match n.to_lowercase().as_str() {
            "mindstate" | "mind_state" => "mindState".to_string(),
            "stance" | "pbarstance" => "pbarStance".to_string(),
            "encumbrance" | "encum" => "encumlevel".to_string(),
            "lblbps" | "bloodpoints" | "blood_points" => "lblBPs".to_string(),
            other => other.to_string(),
        }
    };

    match widget_type {
        "text" => Ok(Mapping {
            template_name: canonical_name(name),
            skip: false,
        }),
        "tabbed" => Ok(Mapping {
            template_name: canonical_name(name),
            skip: false,
        }),
        "command_input" => Ok(Mapping {
            template_name: "command_input".to_string(),
            skip: false,
        }),
        "compass" => Ok(Mapping {
            template_name: "compass".to_string(),
            skip: false,
        }),
        "countdown" => Ok(Mapping {
            template_name: canonical_name(name),
            skip: false,
        }),
        "progress" => Ok(Mapping {
            template_name: canonical_name(name),
            skip: false,
        }),
        "injury_doll" => Ok(Mapping {
            template_name: "injuries".to_string(),
            skip: false,
        }),
        "active_effects" => Ok(Mapping {
            template_name: name.to_string(),
            skip: false,
        }),
        "dashboard" => Ok(Mapping {
            template_name: "dashboard".to_string(),
            skip: false,
        }),
        "targets" => Ok(Mapping {
            template_name: "targets".to_string(),
            skip: false,
        }),
        "players" => Ok(Mapping {
            template_name: "players".to_string(),
            skip: false,
        }),
        "entity" => {
            // Decide targets vs players based on streams or name
            let streams = get_vec_str(table, "streams").unwrap_or(None).unwrap_or_default();
            let template = if streams.iter().any(|s| s.contains("player")) || name.to_lowercase().contains("player") {
                "players".to_string()
            } else {
                "targets".to_string()
            };
            Ok(Mapping {
                template_name: template,
                skip: false,
            })
        }
        "lefthand" => Ok(Mapping {
            template_name: "left_hand".to_string(),
            skip: false,
        }),
        "righthand" => Ok(Mapping {
            template_name: "right_hand".to_string(),
            skip: false,
        }),
        "spellhand" => Ok(Mapping {
            template_name: "spell_hand".to_string(),
            skip: false,
        }),
        "hands" => Ok(Mapping {
            template_name: String::new(),
            skip: true, // unsupported grouped hands
        }),
        _ => Err(anyhow!("unsupported widget_type {}", widget_type)),
    }
}

fn get_str(table: &toml::value::Table, key: &str) -> Result<Option<String>> {
    Ok(table.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()))
}
fn get_bool(table: &toml::value::Table, key: &str) -> Result<Option<bool>> {
    Ok(table.get(key).and_then(|v| v.as_bool()))
}
fn get_u16(table: &toml::value::Table, key: &str) -> Result<Option<u16>> {
    Ok(table.get(key).and_then(|v| v.as_integer()).and_then(|i| u16::try_from(i).ok()))
}
fn get_usize(table: &toml::value::Table, key: &str) -> Result<Option<usize>> {
    Ok(table.get(key).and_then(|v| v.as_integer()).and_then(|i| usize::try_from(i).ok()))
}
fn get_vec_str(table: &toml::value::Table, key: &str) -> Result<Option<Vec<String>>> {
    Ok(table.get(key).and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
    }))
}
fn get_char(table: &toml::value::Table, key: &str) -> Result<Option<char>> {
    Ok(table.get(key).and_then(|v| v.as_str()).and_then(|s| s.chars().next()))
}

fn parse_border_sides(arr: &[Value]) -> BorderSides {
    let mut sides = BorderSides::default();
    sides.top = false;
    sides.bottom = false;
    sides.left = false;
    sides.right = false;
    for v in arr {
        if let Some(s) = v.as_str() {
            match s.to_lowercase().as_str() {
                "top" => sides.top = true,
                "bottom" => sides.bottom = true,
                "left" => sides.left = true,
                "right" => sides.right = true,
                _ => {}
            }
        }
    }
    sides
}

// Helper to get mutable base regardless of variant.
trait BaseMut {
    fn base_mut_opt(&mut self) -> Option<&mut WindowBase>;
}

impl BaseMut for WindowDef {
    fn base_mut_opt(&mut self) -> Option<&mut WindowBase> {
        match self {
            WindowDef::Text { base, .. } => Some(base),
            WindowDef::TabbedText { base, .. } => Some(base),
            WindowDef::Room { base, .. } => Some(base),
            WindowDef::Inventory { base, .. } => Some(base),
            WindowDef::CommandInput { base, .. } => Some(base),
            WindowDef::Progress { base, .. } => Some(base),
            WindowDef::Countdown { base, .. } => Some(base),
            WindowDef::Compass { base, .. } => Some(base),
            WindowDef::Indicator { base, .. } => Some(base),
            WindowDef::Dashboard { base, .. } => Some(base),
            WindowDef::InjuryDoll { base, .. } => Some(base),
            WindowDef::Hand { base, .. } => Some(base),
            WindowDef::ActiveEffects { base, .. } => Some(base),
            WindowDef::Performance { base, .. } => Some(base),
            WindowDef::Targets { base, .. } => Some(base),
            WindowDef::Players { base, .. } => Some(base),
            WindowDef::Spacer { base, .. } => Some(base),
            WindowDef::Spells { base, .. } => Some(base),
        }
    }
}
