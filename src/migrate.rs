//! Layout migration: convert old VellumFE layout TOML files into current layouts.
//!
//! This module handles migrating older VellumFE layout formats to the current
//! layout format, mapping widget types, field names, and structures.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::Deserialize;
use toml::Value;

use crate::config::{BorderSides, Config, Layout, TabbedTextTab, WindowBase, WindowDef};

/// Options for layout migration
pub struct MigrateOptions {
    pub src: PathBuf,
    pub out: PathBuf,
    pub dry_run: bool,
    pub verbose: bool,
}

/// Run the layout migration
pub fn run_migration(options: &MigrateOptions) -> Result<MigrationResult> {
    let mut result = MigrationResult::default();

    // Ensure source exists
    if !options.src.exists() {
        return Err(anyhow!("Source directory does not exist: {}", options.src.display()));
    }

    if !options.src.is_dir() {
        return Err(anyhow!("Source path is not a directory: {}", options.src.display()));
    }

    // Create output directory
    if !options.dry_run {
        fs::create_dir_all(&options.out).context("Failed to create output directory")?;
    }

    // Process all .toml files in source directory
    for entry in fs::read_dir(&options.src).context("Failed to read source directory")? {
        let entry = entry?;
        let path = entry.path();

        // Skip directories and non-toml files
        if !path.is_file() {
            continue;
        }
        if path.extension().map(|e| e != "toml").unwrap_or(true) {
            continue;
        }

        // Skip files that look like current VellumFE layouts already
        if is_current_layout(&path) {
            if options.verbose {
                println!("  Skipping (already current format): {}", path.file_name().unwrap_or_default().to_string_lossy());
            }
            result.skipped += 1;
            continue;
        }

        match process_file(&path, &options.out, options.dry_run, options.verbose) {
            Ok(info) => {
                result.succeeded += 1;
                result.files.push(info);
            }
            Err(e) => {
                if options.verbose {
                    eprintln!("  Warning: {}: {}", path.file_name().unwrap_or_default().to_string_lossy(), e);
                }
                result.failed += 1;
                result.errors.push(format!("{}: {}", path.display(), e));
            }
        }
    }

    Ok(result)
}

/// Result of a migration run
#[derive(Default, Debug)]
pub struct MigrationResult {
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub files: Vec<MigratedFile>,
    pub errors: Vec<String>,
}

/// Info about a migrated file
#[derive(Debug)]
pub struct MigratedFile {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub windows_converted: usize,
    pub windows_skipped: usize,
}

/// Check if a file appears to already be in current VellumFE format
fn is_current_layout(path: &Path) -> bool {
    if let Ok(content) = fs::read_to_string(path) {
        // Current layouts use [[windows]] array syntax with typed variants
        // Old formats use different structure
        content.contains("[[windows]]") &&
        (content.contains("[windows.data]") || content.contains("[windows.base]"))
    } else {
        false
    }
}

#[derive(Debug, Deserialize)]
struct OldLayout {
    #[serde(default)]
    terminal_width: Option<u16>,
    #[serde(default)]
    terminal_height: Option<u16>,
    #[serde(default)]
    windows: Vec<Value>,
}

fn process_file(path: &Path, out_dir: &Path, dry_run: bool, verbose: bool) -> Result<MigratedFile> {
    let txt = fs::read_to_string(path).context("Failed to read file")?;
    let mut layout: OldLayout = toml::from_str(&txt).context("Failed to parse TOML")?;

    // Infer terminal size from filename if missing
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

    let mut windows_skipped = 0;
    for win_val in layout.windows {
        match convert_window(win_val, verbose) {
            Ok(Some(w)) => out_layout.windows.push(w),
            Ok(None) => windows_skipped += 1,
            Err(e) => {
                if verbose {
                    eprintln!("    Window conversion warning: {}", e);
                }
                windows_skipped += 1;
            }
        }
    }

    let fname = path.file_name().ok_or_else(|| anyhow!("No filename"))?;
    let out_path = out_dir.join(fname);

    if !dry_run {
        let toml = toml::to_string_pretty(&out_layout).context("Failed to serialize layout")?;
        fs::write(&out_path, toml).context("Failed to write output file")?;
    }

    if verbose {
        let action = if dry_run { "Would convert" } else { "Converted" };
        println!(
            "  {} {} ({} windows{})",
            action,
            fname.to_string_lossy(),
            out_layout.windows.len(),
            if windows_skipped > 0 {
                format!(", {} skipped", windows_skipped)
            } else {
                String::new()
            }
        );
    }

    Ok(MigratedFile {
        source: path.to_path_buf(),
        destination: out_path,
        windows_converted: out_layout.windows.len(),
        windows_skipped,
    })
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
        .ok_or_else(|| anyhow!("Window entry is not a table"))?;

    let widget_type = get_str(table, "widget_type")?.unwrap_or_default();
    let name = get_str(table, "name")?.unwrap_or_else(|| widget_type.clone());

    // Map widget type/name to a target template name
    let mapping = map_widget_type(&widget_type, &name, table)?;
    if mapping.skip {
        if verbose {
            eprintln!("    Skipping unsupported widget '{}' ({})", name, widget_type);
        }
        return Ok(None);
    }
    let template_name = mapping.template_name;

    // Get the template - if not found, try a fallback based on widget_type
    let mut window = Config::get_window_template(&template_name)
        .or_else(|| {
            // Fallback to generic template by widget_type
            let fallback = match widget_type.as_str() {
                "progress" => Some("progress_custom"),
                "countdown" => Some("roundtime"),
                "text" => Some("main"),
                "tabbed" => Some("chat"),
                "active_effects" => Some("buffs"),
                "indicator" => Some("kneeling"),
                _ => None,
            };
            if let Some(fb) = fallback {
                if verbose {
                    eprintln!("    Using fallback template '{}' for '{}'", fb, template_name);
                }
                Config::get_window_template(fb)
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("No template for '{}'", template_name))?;

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
        // Always set name from source layout
        base.name = name.clone();
    }

    // Widget-specific mapping
    apply_widget_specific_fields(&mut window, table)?;

    Ok(Some(window))
}

fn apply_widget_specific_fields(window: &mut WindowDef, table: &toml::value::Table) -> Result<()> {
    match window {
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
            // Tabs: tolerate either tab array or single stream
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
                            timestamp_position: None,
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
                    timestamp_position: None,
                }];
            }
        }
        WindowDef::Progress { data, base } => {
            // progress_id -> data.id (the XML progressBar id)
            if let Some(v) = get_str(table, "progress_id")? {
                data.id = Some(v);
            }
            // bar_fill or bar_color -> data.color
            if let Some(v) = get_str(table, "bar_fill")? {
                data.color = Some(v);
            } else if let Some(v) = get_str(table, "bar_color")? {
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
            // bar_fill -> data.color (the countdown bar color)
            if let Some(v) = get_str(table, "bar_fill")? {
                data.color = Some(v);
            } else if let Some(v) = get_str(table, "bar_color")? {
                data.color = Some(v);
            }
            if let Some(v) = get_str(table, "bar_background")? {
                data.background_color = Some(v);
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
            if let Some(v) = get_str(table, "title")? {
                base.title = Some(v);
            }
        }
        _ => {}
    }
    Ok(())
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
        // Hand widgets: old format uses left_hand/right_hand/spell_hand or lefthand/righthand/spellhand
        // Current format uses "left", "right", "spell" as template names
        "lefthand" | "left_hand" => Ok(Mapping {
            template_name: "left".to_string(),
            skip: false,
        }),
        "righthand" | "right_hand" => Ok(Mapping {
            template_name: "right".to_string(),
            skip: false,
        }),
        "spellhand" | "spell_hand" => Ok(Mapping {
            template_name: "spell".to_string(),
            skip: false,
        }),
        "hand" => {
            // Generic hand - try to determine which one from name
            let lower_name = name.to_lowercase();
            let template = if lower_name.contains("left") {
                "left".to_string()
            } else if lower_name.contains("right") {
                "right".to_string()
            } else if lower_name.contains("spell") {
                "spell".to_string()
            } else {
                // Default to left hand
                "left".to_string()
            };
            Ok(Mapping {
                template_name: template,
                skip: false,
            })
        }
        "hands" => Ok(Mapping {
            template_name: String::new(),
            skip: true, // unsupported grouped hands
        }),
        "indicator" => Ok(Mapping {
            template_name: canonical_name(name),
            skip: false,
        }),
        "spacer" => Ok(Mapping {
            template_name: "spacer".to_string(),
            skip: false,
        }),
        // Unknown widget types - skip them (leave gaps in layout)
        _ => Ok(Mapping {
            template_name: canonical_name(name),
            skip: false, // Will be skipped if no template found
        }),
    }
}

// Helper functions for extracting values from TOML tables
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

// Helper trait for getting mutable base from any WindowDef variant
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
            WindowDef::Container { base, .. } => Some(base),
            WindowDef::Spacer { base, .. } => Some(base),
            WindowDef::Quickbar { base, .. } => Some(base),
            WindowDef::Spells { base, .. } => Some(base),
            WindowDef::Perception { base, .. } => Some(base),
            WindowDef::Experience { base, .. } => Some(base),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ===========================================
    // infer_size_from_filename tests
    // ===========================================

    #[test]
    fn test_infer_size_from_filename_valid() {
        let path = Path::new("/some/path/layout_120x40.toml");
        let result = infer_size_from_filename(path);
        assert_eq!(result, Some((120, 40)));
    }

    #[test]
    fn test_infer_size_from_filename_different_dimensions() {
        let path = Path::new("my_custom_layout_200x50.toml");
        let result = infer_size_from_filename(path);
        assert_eq!(result, Some((200, 50)));
    }

    #[test]
    fn test_infer_size_from_filename_no_dimensions() {
        let path = Path::new("/layouts/custom.toml");
        let result = infer_size_from_filename(path);
        assert!(result.is_none());
    }

    #[test]
    fn test_infer_size_from_filename_partial_pattern() {
        let path = Path::new("layout_120.toml");
        let result = infer_size_from_filename(path);
        assert!(result.is_none());
    }

    #[test]
    fn test_infer_size_from_filename_invalid_numbers() {
        // Very large numbers that can't fit in u16
        let path = Path::new("layout_999999x999999.toml");
        let result = infer_size_from_filename(path);
        assert!(result.is_none());
    }

    // ===========================================
    // is_current_layout tests
    // ===========================================

    #[test]
    fn test_is_current_layout_with_windows_array() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.toml");
        let content = r#"
[[windows]]
name = "main"

[windows.base]
row = 0
col = 0
"#;
        std::fs::write(&path, content).unwrap();
        assert!(is_current_layout(&path));
    }

    #[test]
    fn test_is_current_layout_with_data_section() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.toml");
        let content = r#"
[[windows]]
name = "main"

[windows.data]
streams = ["main"]
"#;
        std::fs::write(&path, content).unwrap();
        assert!(is_current_layout(&path));
    }

    #[test]
    fn test_is_current_layout_old_format() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.toml");
        let content = r#"
[[windows]]
widget_type = "text"
name = "main"
row = 0
col = 0
"#;
        std::fs::write(&path, content).unwrap();
        // Old format doesn't have [windows.base] or [windows.data]
        assert!(!is_current_layout(&path));
    }

    #[test]
    fn test_is_current_layout_nonexistent_file() {
        let path = Path::new("/nonexistent/file.toml");
        assert!(!is_current_layout(path));
    }

    // ===========================================
    // parse_border_sides tests
    // ===========================================

    #[test]
    fn test_parse_border_sides_all() {
        let arr = vec![
            Value::String("top".to_string()),
            Value::String("bottom".to_string()),
            Value::String("left".to_string()),
            Value::String("right".to_string()),
        ];
        let result = parse_border_sides(&arr);
        assert!(result.top);
        assert!(result.bottom);
        assert!(result.left);
        assert!(result.right);
    }

    #[test]
    fn test_parse_border_sides_partial() {
        let arr = vec![
            Value::String("top".to_string()),
            Value::String("left".to_string()),
        ];
        let result = parse_border_sides(&arr);
        assert!(result.top);
        assert!(!result.bottom);
        assert!(result.left);
        assert!(!result.right);
    }

    #[test]
    fn test_parse_border_sides_empty() {
        let arr: Vec<Value> = vec![];
        let result = parse_border_sides(&arr);
        assert!(!result.top);
        assert!(!result.bottom);
        assert!(!result.left);
        assert!(!result.right);
    }

    #[test]
    fn test_parse_border_sides_case_insensitive() {
        let arr = vec![
            Value::String("TOP".to_string()),
            Value::String("Bottom".to_string()),
        ];
        let result = parse_border_sides(&arr);
        assert!(result.top);
        assert!(result.bottom);
    }

    #[test]
    fn test_parse_border_sides_invalid_values() {
        let arr = vec![
            Value::String("invalid".to_string()),
            Value::Integer(123),
        ];
        let result = parse_border_sides(&arr);
        assert!(!result.top);
        assert!(!result.bottom);
        assert!(!result.left);
        assert!(!result.right);
    }

    // ===========================================
    // get_* helper function tests
    // ===========================================

    #[test]
    fn test_get_str_present() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::String("value".to_string()));
        let result = get_str(&table, "key").unwrap();
        assert_eq!(result, Some("value".to_string()));
    }

    #[test]
    fn test_get_str_missing() {
        let table = toml::value::Table::new();
        let result = get_str(&table, "key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_str_wrong_type() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::Integer(123));
        let result = get_str(&table, "key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_bool_true() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::Boolean(true));
        let result = get_bool(&table, "key").unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_get_bool_false() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::Boolean(false));
        let result = get_bool(&table, "key").unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_get_u16_valid() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::Integer(42));
        let result = get_u16(&table, "key").unwrap();
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_get_u16_overflow() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::Integer(100000)); // > u16::MAX
        let result = get_u16(&table, "key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_u16_negative() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::Integer(-5));
        let result = get_u16(&table, "key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_usize_valid() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::Integer(1000));
        let result = get_usize(&table, "key").unwrap();
        assert_eq!(result, Some(1000));
    }

    #[test]
    fn test_get_vec_str_valid() {
        let mut table = toml::value::Table::new();
        table.insert(
            "key".to_string(),
            Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
            ]),
        );
        let result = get_vec_str(&table, "key").unwrap();
        assert_eq!(result, Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn test_get_vec_str_empty() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::Array(vec![]));
        let result = get_vec_str(&table, "key").unwrap();
        assert_eq!(result, Some(vec![]));
    }

    #[test]
    fn test_get_vec_str_mixed_types() {
        let mut table = toml::value::Table::new();
        table.insert(
            "key".to_string(),
            Value::Array(vec![
                Value::String("valid".to_string()),
                Value::Integer(123), // Will be filtered out
            ]),
        );
        let result = get_vec_str(&table, "key").unwrap();
        assert_eq!(result, Some(vec!["valid".to_string()]));
    }

    #[test]
    fn test_get_char_valid() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::String("★".to_string()));
        let result = get_char(&table, "key").unwrap();
        assert_eq!(result, Some('★'));
    }

    #[test]
    fn test_get_char_multi_char_takes_first() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::String("abc".to_string()));
        let result = get_char(&table, "key").unwrap();
        assert_eq!(result, Some('a'));
    }

    #[test]
    fn test_get_char_empty_string() {
        let mut table = toml::value::Table::new();
        table.insert("key".to_string(), Value::String("".to_string()));
        let result = get_char(&table, "key").unwrap();
        assert!(result.is_none());
    }

    // ===========================================
    // map_widget_type tests
    // ===========================================

    #[test]
    fn test_map_widget_type_text() {
        let table = toml::value::Table::new();
        let result = map_widget_type("text", "main", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "main");
    }

    #[test]
    fn test_map_widget_type_compass() {
        let table = toml::value::Table::new();
        let result = map_widget_type("compass", "compass", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "compass");
    }

    #[test]
    fn test_map_widget_type_hand_left() {
        let table = toml::value::Table::new();
        let result = map_widget_type("hand", "left_hand", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "left");
    }

    #[test]
    fn test_map_widget_type_hand_right() {
        let table = toml::value::Table::new();
        let result = map_widget_type("hand", "right_hand", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "right");
    }

    #[test]
    fn test_map_widget_type_hand_spell() {
        let table = toml::value::Table::new();
        let result = map_widget_type("hand", "spell", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "spell");
    }

    #[test]
    fn test_map_widget_type_hand_default() {
        let table = toml::value::Table::new();
        let result = map_widget_type("hand", "some_hand", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "left"); // Default
    }

    #[test]
    fn test_map_widget_type_lefthand_direct() {
        let table = toml::value::Table::new();
        let result = map_widget_type("lefthand", "lefthand", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "left");
    }

    #[test]
    fn test_map_widget_type_righthand_direct() {
        let table = toml::value::Table::new();
        let result = map_widget_type("righthand", "righthand", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "right");
    }

    #[test]
    fn test_map_widget_type_hands_skipped() {
        let table = toml::value::Table::new();
        let result = map_widget_type("hands", "hands", &table).unwrap();
        assert!(result.skip);
    }

    #[test]
    fn test_map_widget_type_entity_targets() {
        let table = toml::value::Table::new();
        let result = map_widget_type("entity", "targets", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "targets");
    }

    #[test]
    fn test_map_widget_type_entity_players() {
        let mut table = toml::value::Table::new();
        table.insert(
            "streams".to_string(),
            Value::Array(vec![Value::String("players".to_string())]),
        );
        let result = map_widget_type("entity", "entity", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "players");
    }

    #[test]
    fn test_map_widget_type_injury_doll() {
        let table = toml::value::Table::new();
        let result = map_widget_type("injury_doll", "injuries", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "injuries");
    }

    #[test]
    fn test_map_widget_type_canonical_mindstate() {
        let table = toml::value::Table::new();
        let result = map_widget_type("progress", "mindstate", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "mindState");
    }

    #[test]
    fn test_map_widget_type_canonical_stance() {
        let table = toml::value::Table::new();
        let result = map_widget_type("progress", "stance", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "pbarStance");
    }

    #[test]
    fn test_map_widget_type_spacer() {
        let table = toml::value::Table::new();
        let result = map_widget_type("spacer", "spacer", &table).unwrap();
        assert!(!result.skip);
        assert_eq!(result.template_name, "spacer");
    }

    // ===========================================
    // MigrationResult tests
    // ===========================================

    #[test]
    fn test_migration_result_default() {
        let result = MigrationResult::default();
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 0);
        assert!(result.files.is_empty());
        assert!(result.errors.is_empty());
    }

    // ===========================================
    // run_migration tests
    // ===========================================

    #[test]
    fn test_run_migration_nonexistent_source() {
        let options = MigrateOptions {
            src: PathBuf::from("/nonexistent/source/dir"),
            out: PathBuf::from("/tmp/out"),
            dry_run: true,
            verbose: false,
        };
        let result = run_migration(&options);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_run_migration_source_is_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("file.toml");
        std::fs::write(&file_path, "test").unwrap();

        let options = MigrateOptions {
            src: file_path,
            out: dir.path().join("out"),
            dry_run: true,
            verbose: false,
        };
        let result = run_migration(&options);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a directory"));
    }

    #[test]
    fn test_run_migration_empty_directory() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        let out = dir.path().join("out");
        std::fs::create_dir(&src).unwrap();

        let options = MigrateOptions {
            src,
            out,
            dry_run: true,
            verbose: false,
        };
        let result = run_migration(&options).unwrap();
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 0);
    }

    #[test]
    fn test_run_migration_skips_non_toml_files() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();

        // Create a non-toml file
        std::fs::write(src.join("readme.md"), "# Readme").unwrap();

        let options = MigrateOptions {
            src,
            out: dir.path().join("out"),
            dry_run: true,
            verbose: false,
        };
        let result = run_migration(&options).unwrap();
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 0);
    }

    #[test]
    fn test_run_migration_skips_current_format() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();

        // Create a current format layout
        let content = r#"
[[windows]]
name = "main"

[windows.base]
row = 0
"#;
        std::fs::write(src.join("layout.toml"), content).unwrap();

        let options = MigrateOptions {
            src,
            out: dir.path().join("out"),
            dry_run: true,
            verbose: false,
        };
        let result = run_migration(&options).unwrap();
        assert_eq!(result.skipped, 1);
        assert_eq!(result.succeeded, 0);
    }

    #[test]
    fn test_run_migration_dry_run_no_output() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        let out = dir.path().join("out");
        std::fs::create_dir(&src).unwrap();

        // Create an old format layout (simplified - may fail to convert but won't create output)
        let content = r#"
[[windows]]
widget_type = "spacer"
name = "spacer1"
row = 0
col = 0
rows = 1
cols = 1
"#;
        std::fs::write(src.join("old_layout.toml"), content).unwrap();

        let options = MigrateOptions {
            src,
            out: out.clone(),
            dry_run: true,
            verbose: false,
        };
        let _result = run_migration(&options);

        // Output directory should not be created in dry run
        // (though run_migration might still create it before processing)
        assert!(!out.join("old_layout.toml").exists());
    }
}
