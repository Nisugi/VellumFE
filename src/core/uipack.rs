//! Shareable UI packs (`.vellumpack`): a zip of the files that make a
//! UI — TUI layout, highlights, keybinds, controller, hotbars, colors,
//! macros, the
//! active skin, and (when exported from the GUI) the live GUI layout.
//! Built by `.uiexport`, inspected and applied by `.uiimport`; meant
//! for posting on the community Discord so good setups can become
//! shipped defaults. Connection settings never ride along: config.toml
//! is deliberately not a part.
//!
//! Zip layout:
//!   manifest.toml            format/version/parts/skin
//!   layout.toml              the exporting session's TUI layout
//!   gui-layout.json          the GUI's live arrangement (GUI exports)
//!   global/<file>.toml       ~/.vellum-fe/global/ layer
//!   profile/<file>.toml      the exporting character's layer
//!   skins/<name>/**          the active skin's directory (extracted to
//!                            global/skins/<name>/; the entry name is kept
//!                            for compatibility with older packs)
//!
//! Import maps entries back to the same layers (profile entries land on
//! the *importing* character), backs up anything it overwrites, and
//! whitelists every destination — unknown entries are skipped, names
//! are sanitized, nothing can escape the config directory.

use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Parts a pack can carry, in manifest order.
pub const PARTS: &[&str] = &[
    "layout",
    "highlights",
    "keybinds",
    "controller",
    "hotbars",
    "colors",
    "macros",
    "skin",
    "theme",
    "sounds",
    "quickbars",
    "settings",
];

/// Human labels for the pack editor panels, aligned with [`PARTS`].
pub fn part_label(part: &str) -> &'static str {
    match part {
        "layout" => "Layout (windows + arrangement)",
        "highlights" => "Highlights",
        "keybinds" => "Keybinds",
        "controller" => "Controller binds",
        "hotbars" => "Hotbars",
        "colors" => "Colors",
        "macros" => "Macros",
        "skin" => "Active skin (art)",
        "theme" => "Active theme",
        "sounds" => "Sound files",
        "quickbars" => "Quickbars",
        "settings" => "General settings",
        _ => "Unknown",
    }
}

/// Zip entry name for the GUI's live layout (frontends own its format;
/// the pack just carries the bytes).
pub const GUI_LAYOUT_ENTRY: &str = "gui-layout.json";

/// The per-domain TOML files that ride in `global/` and `profile/`.
const LAYERED_FILES: &[(&str, &str)] = &[
    ("highlights", "highlights.toml"),
    ("keybinds", "keybinds.toml"),
    // Controller config is global-only (pads are per-desk); the export loop
    // just skips the absent profile layer.
    ("controller", "controller.toml"),
    ("hotbars", "hotbars.toml"),
    ("colors", "colors.toml"),
    ("macros", "macros.toml"),
];

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PackManifest {
    pub format: u32,
    /// VellumFE version that wrote the pack (informational).
    pub version: String,
    pub parts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
    /// Active theme name; its file rides as `themes/<name>.toml` when it
    /// is a custom (on-disk) theme. Built-ins carry only the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

pub struct PackPreview {
    pub manifest: PackManifest,
    pub entries: Vec<String>,
}

/// The config.toml sections a pack's `settings` part carries: UI feel and
/// behavior, never identity. Connection/account/password, logging paths,
/// web-server config, and personal travel data (go2) are deliberately
/// absent; active skin/theme ride their own parts so selecting only
/// "settings" can't switch appearance.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ShareableSettings {
    pub ui: crate::config::UiConfig,
    #[serde(default)]
    pub sound: crate::config::SoundConfig,
    #[serde(default)]
    pub tts: crate::config::TtsConfig,
    #[serde(default)]
    pub target_list: crate::config::TargetListConfig,
    #[serde(default)]
    pub streams: crate::config::StreamsConfig,
    #[serde(default)]
    pub sorter: crate::config::SorterConfig,
    #[serde(default, rename = "highlights")]
    pub highlight_settings: crate::config::HighlightsConfig,
    #[serde(default)]
    pub menu_keybinds: crate::config::MenuKeybinds,
    #[serde(default)]
    pub event_patterns: std::collections::HashMap<String, crate::config::EventPattern>,
}

impl ShareableSettings {
    pub fn from_config(config: &crate::config::Config) -> Self {
        Self {
            ui: config.ui.clone(),
            sound: config.sound.clone(),
            tts: config.tts.clone(),
            target_list: config.target_list.clone(),
            streams: config.streams.clone(),
            sorter: config.sorter.clone(),
            highlight_settings: config.highlight_settings.clone(),
            menu_keybinds: config.menu_keybinds.clone(),
            event_patterns: config.event_patterns.clone(),
        }
    }

    /// Overwrite the matching sections of `config`; everything not in
    /// the bundle (connection, identity, quickbars, ...) is untouched.
    pub fn apply_to(self, config: &mut crate::config::Config) {
        config.ui = self.ui;
        config.sound = self.sound;
        config.tts = self.tts;
        config.target_list = self.target_list;
        config.streams = self.streams;
        config.sorter = self.sorter;
        config.highlight_settings = self.highlight_settings;
        config.menu_keybinds = self.menu_keybinds;
        config.event_patterns = self.event_patterns;
    }
}

/// Standalone serialization wrapper for the quickbars section, so packs
/// carry it as `quickbars.toml` without touching config.toml itself.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct QuickbarsFile {
    #[serde(default)]
    pub quickbars: crate::config::QuickbarsConfig,
}

pub struct ApplyOutcome {
    /// Human-readable notes about what landed where.
    pub notes: Vec<String>,
    /// The TUI layout name the pack installed (load with .loadlayout).
    pub layout_name: Option<String>,
    /// Raw GUI layout bytes for the GUI frontend to install as a named
    /// checkpoint; the TUI reports it as GUI-only.
    pub gui_layout: Option<Vec<u8>>,
    /// Skin the pack shipped (already extracted); callers set
    /// active_skin.
    pub skin: Option<String>,
    /// Theme to activate (its custom file, if any, is already written).
    pub theme: Option<String>,
    /// `quickbars.toml` body for the caller to merge into config.toml
    /// (never written to disk directly — config.toml is not pack-writable).
    pub quickbars_toml: Option<String>,
    /// Sanitized `settings.toml` body for the caller to merge into
    /// config.toml (same rule).
    pub settings_toml: Option<String>,
    /// Where overwritten files were backed up (None when nothing was).
    pub backup_dir: Option<PathBuf>,
}

/// Pack (and layout) names double as file stems — keep them boring.
pub fn is_valid_pack_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Manifest-supplied skin/theme names become path components, so they
/// must be a single boring file/dir name: ASCII alphanumeric plus
/// `-`, `_`, `.` and space, no leading/trailing dot or space, and never
/// pure dots. This covers every built-in theme (lowercase alnum-dash)
/// and every name `save_to_file`'s sanitizer can produce, while
/// excluding separators, drive prefixes (`:`), NTFS alternate streams
/// (`:`), traversal (`..`), and rooted forms.
fn is_safe_manifest_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
        && !name.starts_with([' ', '.'])
        && !name.ends_with([' ', '.'])
}

/// True when `part` is a single normal relative path component: no
/// separators (either slash), no traversal, no empties, no Windows
/// drive/UNC/rooted forms, no NTFS alternate-stream (`name:stream`)
/// syntax. Every archive-derived component must pass this.
fn is_safe_component(part: &str) -> bool {
    if part.is_empty()
        || part == "."
        || part == ".."
        || part.contains(['/', '\\', ':'])
        || part.ends_with('.')
        || part.ends_with(' ')
    {
        return false;
    }
    // Belt and suspenders: the component must parse as exactly one
    // Normal component on this platform (catches prefixes/root forms).
    let mut comps = Path::new(part).components();
    matches!(
        (comps.next(), comps.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

/// True when every path component is a plain safe name.
fn sane_relative(parts: &[&str]) -> bool {
    !parts.is_empty() && parts.iter().all(|p| is_safe_component(p))
}

/// The deepest ancestor of `path` that already exists on disk (used to
/// resolve symlinks/junctions in parents before we create anything).
fn deepest_existing_ancestor(path: &Path) -> PathBuf {
    let mut cur = path;
    loop {
        // symlink_metadata: a dangling symlink still "exists" here and
        // gets resolved (and rejected) by canonicalize below.
        if cur.symlink_metadata().is_ok() {
            return cur.to_path_buf();
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return cur.to_path_buf(),
        }
    }
}

/// Enforce that `dest` really resolves beneath `canon_base` before any
/// directory creation, backup, or write. Lexical containment is not
/// enough: an existing symlink or junction in a parent directory could
/// redirect the write outside the config dir, so canonicalize the
/// deepest existing ancestor and check the real location.
fn ensure_contained(canon_base: &Path, dest: &Path, entry: &str) -> Result<()> {
    let anchor = deepest_existing_ancestor(dest);
    let real = std::fs::canonicalize(&anchor)
        .with_context(|| format!("Failed to resolve path for pack entry '{entry}'"))?;
    if !real.starts_with(canon_base) {
        bail!("Pack entry '{entry}' resolves outside the config directory ({dest:?}) — refusing");
    }
    Ok(())
}

fn zip_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

/// Everything `.uiexport` (or the pack editor panel) hands to [`export`].
#[derive(Default)]
pub struct ExportRequest<'a> {
    pub name: &'a str,
    pub parts: &'a [String],
    /// The exporting character; their profile layer rides in `profile/`.
    pub character: Option<&'a str>,
    /// The exporting session's TUI layout, already serialized.
    pub layout_toml: Option<String>,
    pub active_skin: Option<&'a str>,
    /// Active theme name; a matching `<base>/themes/<name>.toml` (custom
    /// theme) is bundled, a built-in travels as just the name.
    pub active_theme: Option<&'a str>,
    /// Pre-serialized quickbars section (callers own Config access).
    pub quickbars_toml: Option<String>,
    /// Pre-serialized sanitized settings (never the raw config.toml).
    pub settings_toml: Option<String>,
    /// Frontend-owned entries like the GUI layout.
    pub extra_files: &'a [(String, Vec<u8>)],
    /// Where to write the pack; None = `<base>/exports/`.
    pub dest_dir: Option<&'a Path>,
}

/// Build a pack (default destination `<base>/exports/<name>.vellumpack`).
///
/// Returns the pack path and the parts actually included (a part with
/// nothing on disk drops out silently).
pub fn export(base: &Path, req: &ExportRequest) -> Result<(PathBuf, Vec<String>)> {
    if !is_valid_pack_name(req.name) {
        bail!("Pack names use letters, digits, '-' and '_' only");
    }
    for part in req.parts {
        if !PARTS.contains(&part.as_str()) {
            bail!("Unknown part '{}' — parts are: {}", part, PARTS.join(", "));
        }
    }
    // Skin/theme names ride the manifest and become zip entry names and,
    // on import, path components — keep export and import policy aligned.
    let wants_part = |part: &str| req.parts.iter().any(|p| p == part);
    if wants_part("skin") {
        if let Some(skin) = req.active_skin.filter(|s| !s.is_empty()) {
            if !is_safe_manifest_name(skin) || !is_safe_component(skin) {
                bail!("Active skin name '{skin}' is not a safe file name — cannot export it");
            }
        }
    }
    if wants_part("theme") {
        if let Some(theme) = req.active_theme.filter(|t| !t.is_empty()) {
            if !is_safe_manifest_name(theme) || !is_safe_component(theme) {
                bail!("Active theme name '{theme}' is not a safe file name — cannot export it");
            }
        }
    }

    let dest_dir = match req.dest_dir {
        Some(dir) => dir.to_path_buf(),
        None => base.join("exports"),
    };
    std::fs::create_dir_all(&dest_dir).with_context(|| format!("Failed to create {dest_dir:?}"))?;
    let pack_path = dest_dir.join(format!("{}.vellumpack", req.name));
    let file = std::fs::File::create(&pack_path)
        .with_context(|| format!("Failed to create {pack_path:?}"))?;
    let mut zip = zip::ZipWriter::new(file);

    let mut included: Vec<String> = Vec::new();
    let wants = |part: &str| req.parts.iter().any(|p| p == part);

    if wants("layout") {
        if let Some(toml) = req.layout_toml.as_deref() {
            zip.start_file("layout.toml", zip_options())?;
            zip.write_all(toml.as_bytes())?;
            included.push("layout".to_string());
        }
        for (entry, bytes) in req.extra_files {
            zip.start_file(entry.as_str(), zip_options())?;
            zip.write_all(bytes)?;
            if !included.contains(&"layout".to_string()) {
                included.push("layout".to_string());
            }
        }
    }

    for (part, file_name) in LAYERED_FILES {
        if !wants(part) {
            continue;
        }
        let mut any = false;
        let global = base.join("global").join(file_name);
        if global.is_file() {
            zip.start_file(format!("global/{file_name}"), zip_options())?;
            zip.write_all(&std::fs::read(&global)?)?;
            any = true;
        }
        if let Some(character) = req.character {
            let profile = base.join(character).join(file_name);
            if profile.is_file() {
                zip.start_file(format!("profile/{file_name}"), zip_options())?;
                zip.write_all(&std::fs::read(&profile)?)?;
                any = true;
            }
        }
        if any {
            included.push(part.to_string());
        }
    }

    let mut skin_included: Option<String> = None;
    if wants("skin") {
        if let Some(skin) = req.active_skin.filter(|s| !s.is_empty()) {
            let skin_dir = base.join("global").join("skins").join(skin);
            if skin_dir.is_dir() {
                let mut files = Vec::new();
                collect_files(&skin_dir, &mut files)?;
                files.sort();
                for path in files {
                    let rel = path
                        .strip_prefix(&skin_dir)
                        .expect("collected under skin_dir");
                    let entry =
                        format!("skins/{skin}/{}", rel.to_string_lossy().replace('\\', "/"));
                    zip.start_file(entry, zip_options())?;
                    zip.write_all(&std::fs::read(&path)?)?;
                }
                included.push("skin".to_string());
                skin_included = Some(skin.to_string());
            }
        }
    }

    let mut theme_included: Option<String> = None;
    if wants("theme") {
        if let Some(theme) = req.active_theme.filter(|t| !t.is_empty()) {
            // A custom theme's file travels; a built-in travels as just
            // the manifest name (present in every install).
            let theme_file = base.join("themes").join(format!("{theme}.toml"));
            if theme_file.is_file() {
                zip.start_file(format!("themes/{theme}.toml"), zip_options())?;
                zip.write_all(&std::fs::read(&theme_file)?)?;
            }
            included.push("theme".to_string());
            theme_included = Some(theme.to_string());
        }
    }

    if wants("sounds") {
        let sounds_dir = base.join("global").join("sounds");
        if sounds_dir.is_dir() {
            let mut files = Vec::new();
            collect_files(&sounds_dir, &mut files)?;
            files.sort();
            let mut any = false;
            for path in files {
                let rel = path
                    .strip_prefix(&sounds_dir)
                    .expect("collected under sounds_dir");
                let entry = format!("sounds/{}", rel.to_string_lossy().replace('\\', "/"));
                zip.start_file(entry, zip_options())?;
                zip.write_all(&std::fs::read(&path)?)?;
                any = true;
            }
            if any {
                included.push("sounds".to_string());
            }
        }
    }

    if wants("quickbars") {
        if let Some(toml) = req
            .quickbars_toml
            .as_deref()
            .filter(|t| !t.trim().is_empty())
        {
            zip.start_file("quickbars.toml", zip_options())?;
            zip.write_all(toml.as_bytes())?;
            included.push("quickbars".to_string());
        }
    }

    if wants("settings") {
        if let Some(toml) = req
            .settings_toml
            .as_deref()
            .filter(|t| !t.trim().is_empty())
        {
            zip.start_file("settings.toml", zip_options())?;
            zip.write_all(toml.as_bytes())?;
            included.push("settings".to_string());
        }
    }

    let manifest = PackManifest {
        format: 1,
        version: env!("CARGO_PKG_VERSION").to_string(),
        parts: included.clone(),
        skin: skin_included,
        theme: theme_included,
    };
    zip.start_file("manifest.toml", zip_options())?;
    zip.write_all(
        toml::to_string_pretty(&manifest)
            .context("Failed to serialize manifest")?
            .as_bytes(),
    )?;
    zip.finish().context("Failed to finish pack")?;
    Ok((pack_path, included))
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Resolve `.uiimport`'s argument: a pack name from `imports/` or
/// `exports/`, or a path (absolute, or relative to the config dir).
pub fn resolve_pack_path(base: &Path, arg: &str) -> Option<PathBuf> {
    if is_valid_pack_name(arg) {
        for dir in ["imports", "exports"] {
            let named = base.join(dir).join(format!("{arg}.vellumpack"));
            if named.is_file() {
                return Some(named);
            }
        }
    }
    let direct = PathBuf::from(arg);
    if direct.is_file() {
        return Some(direct);
    }
    let relative = base.join(arg);
    relative.is_file().then_some(relative)
}

/// Packs dropped into `<base>/imports/` (the pack editor's dropdown),
/// sorted by name. The directory is created so users can find it.
pub fn list_import_packs(base: &Path) -> Vec<String> {
    let dir = base.join("imports");
    let _ = std::fs::create_dir_all(&dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "vellumpack" {
                return None;
            }
            let stem = path.file_stem()?.to_str()?;
            is_valid_pack_name(stem).then(|| stem.to_string())
        })
        .collect();
    names.sort();
    names
}

/// Read a pack's manifest and entry list without touching anything.
pub fn preview(path: &Path) -> Result<PackPreview> {
    let file = std::fs::File::open(path).with_context(|| format!("Failed to open {path:?}"))?;
    let mut zip = zip::ZipArchive::new(file).context("Not a valid pack (zip)")?;
    let mut manifest_text = String::new();
    zip.by_name("manifest.toml")
        .context("Pack has no manifest.toml")?
        .read_to_string(&mut manifest_text)?;
    let manifest: PackManifest =
        toml::from_str(&manifest_text).context("Pack manifest did not parse")?;
    if manifest.format != 1 {
        bail!(
            "Pack format {} is newer than this VellumFE understands",
            manifest.format
        );
    }
    let entries = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| n != "manifest.toml")
        .collect();
    Ok(PackPreview { manifest, entries })
}

/// Where a whitelisted pack entry lands, relative to the config base.
/// None = not a file this format writes (unknown, unsafe, or handled
/// out-of-band like the GUI layout).
fn entry_destination(
    entry: &str,
    pack_name: &str,
    character: Option<&str>,
    skin: Option<&str>,
    theme: Option<&str>,
) -> Option<PathBuf> {
    let known_file = |name: &str| LAYERED_FILES.iter().any(|(_, f)| *f == name);
    if entry == "layout.toml" {
        return Some(PathBuf::from("layouts").join(format!("{pack_name}.toml")));
    }
    if let Some(name) = entry.strip_prefix("global/") {
        return known_file(name).then(|| PathBuf::from("global").join(name));
    }
    if let Some(name) = entry.strip_prefix("profile/") {
        // Profile entries land on the importing character; without one
        // they fall back to the shared profile dir ("default").
        let character = character.filter(|c| is_safe_component(c)).unwrap_or("default");
        return known_file(name).then(|| PathBuf::from(character).join(name));
    }
    if let (Some(rest), Some(skin)) = (entry.strip_prefix("skins/"), skin) {
        // Only the manifest's own skin (itself a safe single name), and
        // only sane relative paths.
        if !is_safe_manifest_name(skin) || !is_safe_component(skin) {
            return None;
        }
        let mut parts = rest.split('/');
        if parts.next() != Some(skin) {
            return None;
        }
        let tail: Vec<&str> = parts.collect();
        if !sane_relative(&tail) {
            return None;
        }
        let mut path = PathBuf::from("global").join("skins").join(skin);
        for part in tail {
            path.push(part);
        }
        return Some(path);
    }
    if let Some(rest) = entry.strip_prefix("sounds/") {
        let tail: Vec<&str> = rest.split('/').collect();
        if !sane_relative(&tail) {
            return None;
        }
        let mut path = PathBuf::from("global").join("sounds");
        for part in tail {
            path.push(part);
        }
        return Some(path);
    }
    if let (Some(name), Some(theme)) = (entry.strip_prefix("themes/"), theme) {
        // Only the manifest's own theme file (a safe single name), flat
        // under themes/.
        if !is_safe_manifest_name(theme) || !is_safe_component(name) {
            return None;
        }
        return (name == format!("{theme}.toml")).then(|| PathBuf::from("themes").join(name));
    }
    None
}

/// Which part an entry belongs to, for selective installs.
fn entry_part(entry: &str) -> Option<&'static str> {
    if entry == "layout.toml" || entry == GUI_LAYOUT_ENTRY {
        return Some("layout");
    }
    if entry == "quickbars.toml" {
        return Some("quickbars");
    }
    if entry == "settings.toml" {
        return Some("settings");
    }
    if entry.starts_with("skins/") {
        return Some("skin");
    }
    if entry.starts_with("sounds/") {
        return Some("sounds");
    }
    if entry.starts_with("themes/") {
        return Some("theme");
    }
    let name = entry
        .strip_prefix("global/")
        .or_else(|| entry.strip_prefix("profile/"))?;
    LAYERED_FILES
        .iter()
        .find(|(_, file)| *file == name)
        .map(|(part, _)| *part)
}

/// Apply a pack: back up whatever it overwrites (under
/// `<base>/backups/uipack-<epoch>/`), write the whitelisted entries,
/// and report what happened. `selected` limits the install to those
/// parts (None = everything in the pack). Reloads are the caller's job —
/// this layer only moves files.
pub fn apply(
    base: &Path,
    path: &Path,
    character: Option<&str>,
    selected: Option<&[String]>,
) -> Result<ApplyOutcome> {
    let PackPreview { manifest, .. } = preview(path)?;
    // Manifest skin/theme names become destination path components;
    // refuse the whole pack before touching anything if either is not a
    // plain safe name (traversal, separators, drive/stream syntax, ...).
    if let Some(skin) = manifest.skin.as_deref() {
        if !is_safe_manifest_name(skin) || !is_safe_component(skin) {
            bail!("Pack manifest carries an unsafe skin name '{skin}' — refusing to import");
        }
    }
    if let Some(theme) = manifest.theme.as_deref() {
        if !is_safe_manifest_name(theme) || !is_safe_component(theme) {
            bail!("Pack manifest carries an unsafe theme name '{theme}' — refusing to import");
        }
    }
    let pack_name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| is_valid_pack_name(s))
        .unwrap_or_else(|| "imported".to_string());

    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;

    // Canonical config base for containment checks (created first so it
    // can be canonicalized).
    std::fs::create_dir_all(base).with_context(|| format!("Failed to create {base:?}"))?;
    let canon_base = std::fs::canonicalize(base)
        .with_context(|| format!("Failed to resolve config directory {base:?}"))?;

    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_root = base.join("backups").join(format!("uipack-{epoch}"));
    let mut backed_up = false;

    let part_selected = |part: &str| {
        selected
            .map(|list| list.iter().any(|p| p == part))
            .unwrap_or(true)
    };

    let mut outcome = ApplyOutcome {
        notes: Vec::new(),
        layout_name: None,
        gui_layout: None,
        skin: manifest.skin.clone().filter(|_| part_selected("skin")),
        theme: manifest.theme.clone().filter(|_| part_selected("theme")),
        quickbars_toml: None,
        settings_toml: None,
        backup_dir: None,
    };

    // Pass 1 (preflight): resolve and validate every selected
    // destination BEFORE anything is created, backed up, or written, so
    // a bad later entry cannot leave earlier files installed. `None`
    // means "handled out-of-band or skipped"; `Some(rel)` is a planned
    // disk write.
    enum Plan {
        Skip,
        OutOfBand,
        Write(PathBuf),
    }
    let mut plans: Vec<Plan> = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        if entry.is_dir() || name == "manifest.toml" {
            plans.push(Plan::Skip);
            continue;
        }
        if let Some(part) = entry_part(&name) {
            if !part_selected(part) {
                plans.push(Plan::Skip);
                continue;
            }
        }
        if name == GUI_LAYOUT_ENTRY || name == "quickbars.toml" || name == "settings.toml" {
            plans.push(Plan::OutOfBand);
            continue;
        }
        let Some(rel) = entry_destination(
            &name,
            &pack_name,
            character,
            manifest.skin.as_deref(),
            manifest.theme.as_deref(),
        ) else {
            outcome
                .notes
                .push(format!("skipped unknown entry '{name}'"));
            plans.push(Plan::Skip);
            continue;
        };
        // Every component of the whitelisted destination must be a
        // normal relative name (no drive/UNC/rooted/stream smuggling).
        let all_normal = rel.components().all(|c| {
            matches!(&c, std::path::Component::Normal(os) if os.to_str().is_some_and(is_safe_component))
        });
        if !all_normal {
            bail!("Pack entry '{name}' maps to an unsafe destination {rel:?} — refusing");
        }
        let dest = base.join(&rel);
        // Real (symlink-resolved) containment beneath the config base,
        // for both the destination and its backup location.
        ensure_contained(&canon_base, &dest, &name)?;
        ensure_contained(&canon_base, &backup_root.join(&rel), &name)?;
        plans.push(Plan::Write(rel));
    }

    // Pass 2: perform the pre-validated backups and writes.
    for (i, plan) in plans.iter().enumerate() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        let rel = match plan {
            Plan::Skip => continue,
            Plan::OutOfBand => {
                if name == GUI_LAYOUT_ENTRY {
                    let mut bytes = Vec::new();
                    entry.read_to_end(&mut bytes)?;
                    outcome.gui_layout = Some(bytes);
                } else {
                    // Config-merging parts are returned to the caller,
                    // never written to disk here (config.toml is not
                    // pack-writable).
                    let mut text = String::new();
                    entry.read_to_string(&mut text)?;
                    if name == "quickbars.toml" {
                        outcome.quickbars_toml = Some(text);
                    } else {
                        outcome.settings_toml = Some(text);
                    }
                }
                continue;
            }
            Plan::Write(rel) => rel,
        };
        let dest = base.join(rel);
        // Re-check right before touching disk: parents may have gained
        // links between preflight and now (and preflight-created state
        // is our own real directories).
        ensure_contained(&canon_base, &dest, &name)?;
        if dest.is_file() {
            let backup = backup_root.join(rel);
            ensure_contained(&canon_base, &backup, &name)?;
            if let Some(parent) = backup.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&dest, &backup).with_context(|| format!("Failed to back up {dest:?}"))?;
            backed_up = true;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        std::fs::write(&dest, &bytes).with_context(|| format!("Failed to write {dest:?}"))?;
        if name == "layout.toml" {
            outcome.layout_name = Some(pack_name.clone());
        }
    }

    if backed_up {
        outcome.backup_dir = Some(backup_root);
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn seed(base: &Path) {
        std::fs::create_dir_all(base.join("global")).unwrap();
        std::fs::create_dir_all(base.join("Testy")).unwrap();
        std::fs::create_dir_all(base.join("global/skins/parchment/icons")).unwrap();
        std::fs::create_dir_all(base.join("global/sounds/alerts")).unwrap();
        std::fs::create_dir_all(base.join("themes")).unwrap();
        std::fs::write(base.join("global/keybinds.toml"), "[controller]\n").unwrap();
        std::fs::write(base.join("global/highlights.toml"), "# hl\n").unwrap();
        std::fs::write(base.join("Testy/colors.toml"), "# colors\n").unwrap();
        std::fs::write(
            base.join("global/skins/parchment/skin.toml"),
            "name = 'parchment'\n",
        )
        .unwrap();
        std::fs::write(
            base.join("global/skins/parchment/icons/a.png"),
            b"png-bytes",
        )
        .unwrap();
        std::fs::write(base.join("global/sounds/alerts/ding.mp3"), b"mp3-bytes").unwrap();
        std::fs::write(base.join("themes/midnight.toml"), "# theme\n").unwrap();
    }

    fn all_parts() -> Vec<String> {
        PARTS.iter().map(|s| s.to_string()).collect()
    }

    fn full_request<'a>(parts: &'a [String], extra: &'a [(String, Vec<u8>)]) -> ExportRequest<'a> {
        ExportRequest {
            name: "my-ui",
            parts,
            character: Some("Testy"),
            layout_toml: Some("# layout\n".to_string()),
            active_skin: Some("parchment"),
            active_theme: Some("midnight"),
            quickbars_toml: Some("[[quickbars]]\nname = 'qb'\n".to_string()),
            settings_toml: Some("[ui]\nbuffer_size = 5000\n".to_string()),
            extra_files: extra,
            dest_dir: None,
        }
    }

    #[test]
    fn round_trip_export_preview_apply() {
        let dir = base();
        seed(dir.path());
        let parts = all_parts();
        let extra = vec![(GUI_LAYOUT_ENTRY.to_string(), b"{\"gui\":1}".to_vec())];
        let (path, included) = export(dir.path(), &full_request(&parts, &extra)).unwrap();
        assert!(
            path.ends_with("exports/my-ui.vellumpack")
                || path.ends_with("exports\\my-ui.vellumpack")
        );
        // Only parts with files on disk are included (hotbars/macros had
        // none).
        assert_eq!(
            included,
            vec![
                "layout",
                "highlights",
                "keybinds",
                "colors",
                "skin",
                "theme",
                "sounds",
                "quickbars",
                "settings"
            ]
        );

        let preview = preview(&path).unwrap();
        assert_eq!(preview.manifest.skin.as_deref(), Some("parchment"));
        assert_eq!(preview.manifest.theme.as_deref(), Some("midnight"));
        assert!(preview.entries.iter().any(|e| e == "global/keybinds.toml"));
        assert!(preview
            .entries
            .iter()
            .any(|e| e == "skins/parchment/icons/a.png"));
        assert!(preview
            .entries
            .iter()
            .any(|e| e == "sounds/alerts/ding.mp3"));
        assert!(preview.entries.iter().any(|e| e == "themes/midnight.toml"));

        // Import into a fresh base as a different character.
        let target = base();
        std::fs::create_dir_all(target.path().join("global")).unwrap();
        std::fs::write(target.path().join("global/keybinds.toml"), "old\n").unwrap();
        let outcome = apply(target.path(), &path, Some("Newchar"), None).unwrap();
        assert_eq!(outcome.layout_name.as_deref(), Some("my-ui"));
        assert_eq!(outcome.skin.as_deref(), Some("parchment"));
        assert_eq!(outcome.theme.as_deref(), Some("midnight"));
        assert_eq!(outcome.gui_layout.as_deref(), Some(&b"{\"gui\":1}"[..]));
        assert!(outcome.quickbars_toml.as_deref().unwrap().contains("qb"));
        assert!(outcome
            .settings_toml
            .as_deref()
            .unwrap()
            .contains("buffer_size"));
        assert_eq!(
            std::fs::read_to_string(target.path().join("layouts/my-ui.toml")).unwrap(),
            "# layout\n"
        );
        assert_eq!(
            std::fs::read_to_string(target.path().join("global/keybinds.toml")).unwrap(),
            "[controller]\n"
        );
        // Profile entries land on the importing character.
        assert_eq!(
            std::fs::read_to_string(target.path().join("Newchar/colors.toml")).unwrap(),
            "# colors\n"
        );
        assert_eq!(
            std::fs::read(target.path().join("global/skins/parchment/icons/a.png")).unwrap(),
            b"png-bytes"
        );
        assert_eq!(
            std::fs::read(target.path().join("global/sounds/alerts/ding.mp3")).unwrap(),
            b"mp3-bytes"
        );
        assert_eq!(
            std::fs::read_to_string(target.path().join("themes/midnight.toml")).unwrap(),
            "# theme\n"
        );
        // Config-merging parts never land as loose files.
        assert!(!target.path().join("quickbars.toml").exists());
        assert!(!target.path().join("settings.toml").exists());
        // The overwritten global keybinds got backed up.
        let backup = outcome.backup_dir.expect("backup dir");
        assert_eq!(
            std::fs::read_to_string(backup.join("global/keybinds.toml")).unwrap(),
            "old\n"
        );
    }

    #[test]
    fn apply_selected_parts_only() {
        let dir = base();
        seed(dir.path());
        let parts = all_parts();
        let extra: Vec<(String, Vec<u8>)> = Vec::new();
        let (path, _) = export(dir.path(), &full_request(&parts, &extra)).unwrap();

        let target = base();
        let selected = vec!["highlights".to_string(), "theme".to_string()];
        let outcome = apply(target.path(), &path, Some("Newchar"), Some(&selected)).unwrap();

        // Selected parts landed...
        assert!(target.path().join("global/highlights.toml").is_file());
        assert!(target.path().join("themes/midnight.toml").is_file());
        assert_eq!(outcome.theme.as_deref(), Some("midnight"));
        // ...everything else stayed out.
        assert!(!target.path().join("global/keybinds.toml").exists());
        assert!(!target.path().join("layouts/my-ui.toml").exists());
        assert!(!target.path().join("global/skins").exists());
        assert!(!target.path().join("global/sounds").exists());
        assert!(outcome.layout_name.is_none());
        assert!(outcome.skin.is_none());
        assert!(outcome.quickbars_toml.is_none());
        assert!(outcome.settings_toml.is_none());
    }

    #[test]
    fn export_to_custom_destination() {
        let dir = base();
        seed(dir.path());
        let dest = tempfile::tempdir().unwrap();
        let parts = vec!["highlights".to_string()];
        let extra: Vec<(String, Vec<u8>)> = Vec::new();
        let mut req = full_request(&parts, &extra);
        req.dest_dir = Some(dest.path());
        let (path, _) = export(dir.path(), &req).unwrap();
        assert!(path.starts_with(dest.path()));
        assert!(path.is_file());
    }

    #[test]
    fn export_rejects_bad_names_and_parts() {
        let dir = base();
        let ok_parts = all_parts();
        let bad_part = vec!["passwords".to_string()];
        let extra: Vec<(String, Vec<u8>)> = Vec::new();
        let mut bad_name = full_request(&ok_parts, &extra);
        bad_name.name = "../evil";
        assert!(export(dir.path(), &bad_name).is_err());
        let mut bad_parts = full_request(&bad_part, &extra);
        bad_parts.name = "ok";
        assert!(export(dir.path(), &bad_parts).is_err());
    }

    #[test]
    fn apply_skips_traversal_and_unknown_entries() {
        let dir = base();
        // Hand-build a hostile pack.
        let pack_path = dir.path().join("evil.vellumpack");
        let file = std::fs::File::create(&pack_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let manifest = PackManifest {
            format: 1,
            version: "0".into(),
            parts: vec!["skin".into()],
            skin: Some("nice".into()),
            theme: Some("cool".into()),
        };
        zip.start_file("manifest.toml", zip_options()).unwrap();
        zip.write_all(toml::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        for evil in [
            "global/../../outside.toml",
            "global/config.toml",       // config.toml is never a pack file
            "profile/passwords.toml",   // nor secrets
            "skins/other/skin.toml",    // wrong skin name
            "skins/nice/../escape.png", // traversal inside the skin
            "sounds/../escape.mp3",     // traversal in the sounds pool
            "themes/other.toml",        // wrong theme name
            "themes/cool/../evil.toml", // theme entries are flat files only
            "random.bin",
        ] {
            zip.start_file(evil, zip_options()).unwrap();
            zip.write_all(b"nope").unwrap();
        }
        zip.finish().unwrap();

        let outcome = apply(dir.path(), &pack_path, Some("Testy"), None).unwrap();
        assert_eq!(
            outcome.notes.len(),
            9,
            "all nine entries skipped: {:?}",
            outcome.notes
        );
        assert!(!dir.path().join("global/config.toml").exists());
        assert!(!dir.path().join("Testy/passwords.toml").exists());
        assert!(!dir.path().parent().unwrap().join("outside.toml").exists());
        assert!(!dir.path().join("global/skins/other").exists());
        assert!(!dir.path().join("themes/other.toml").exists());
        assert!(!dir.path().parent().unwrap().join("escape.mp3").exists());
    }

    /// Build a pack at `path` with the given manifest names and entries.
    fn build_pack(path: &Path, skin: Option<&str>, theme: Option<&str>, entries: &[&str]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let manifest = PackManifest {
            format: 1,
            version: "0".into(),
            parts: vec!["skin".into(), "theme".into(), "sounds".into()],
            skin: skin.map(String::from),
            theme: theme.map(String::from),
        };
        zip.start_file("manifest.toml", zip_options()).unwrap();
        zip.write_all(toml::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        for entry in entries {
            zip.start_file(*entry, zip_options()).unwrap();
            zip.write_all(b"payload").unwrap();
        }
        zip.finish().unwrap();
    }

    /// A root tempdir holding `cfg/` (the config base) and an outside
    /// sentinel file that any escape would have to disturb.
    fn base_with_sentinel() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let cfg = root.path().join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        let sentinel = root.path().join("outside.toml");
        std::fs::write(&sentinel, "sentinel\n").unwrap();
        (root, cfg, sentinel)
    }

    #[test]
    fn apply_rejects_traversal_theme_name_in_manifest() {
        let (root, cfg, sentinel) = base_with_sentinel();
        let pack = root.path().join("evil.vellumpack");
        // Theme "../../outside" makes "themes/../../outside.toml" match
        // the manifest — the old code escaped the config base with it.
        build_pack(
            &pack,
            None,
            Some("../../outside"),
            &["themes/../../outside.toml"],
        );
        let Err(err) = apply(&cfg, &pack, Some("Testy"), None) else {
            panic!("apply must be rejected");
        };
        assert!(err.to_string().contains("unsafe theme name"), "{err}");
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "sentinel\n");
        assert!(!cfg.join("themes").exists());
    }

    #[test]
    fn apply_rejects_unsafe_skin_names_in_manifest() {
        for bad in ["..", "C:", "a/b", "a\\b", "nice:stream", " lead", "trail."] {
            let (root, cfg, sentinel) = base_with_sentinel();
            let pack = root.path().join("evil.vellumpack");
            build_pack(&pack, Some(bad), None, &[&format!("skins/{bad}/x.toml")]);
            let Err(err) = apply(&cfg, &pack, Some("Testy"), None) else {
            panic!("apply must be rejected");
        };
            assert!(err.to_string().contains("unsafe skin name"), "{bad}: {err}");
            assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "sentinel\n");
            assert!(!cfg.join("global").join("skins").exists(), "{bad}");
        }
    }

    #[test]
    fn apply_skips_drive_rooted_unc_and_stream_entries() {
        let (root, cfg, sentinel) = base_with_sentinel();
        let pack = root.path().join("evil.vellumpack");
        let entries = [
            "sounds/C:/boot.mp3",              // drive prefix component
            "sounds/c:evil.mp3",               // drive-relative / stream form
            "sounds//rooted.mp3",              // empty component (rooted-ish)
            "sounds/\\\\srv\\share\\x.mp3",    // UNC via backslashes
            "sounds/a\\b.mp3",                 // backslash separator smuggling
            "sounds/na:me.mp3",                // NTFS alternate stream
            "skins/nice/C:s.png",              // drive form inside skin tail
            "skins/nice/art:stream.png",       // stream inside skin tail
            "skins/nice/trail. ",              // trailing-dot/space quirk
        ];
        build_pack(&pack, Some("nice"), None, &entries);
        let outcome = apply(&cfg, &pack, Some("Testy"), None).unwrap();
        assert_eq!(
            outcome.notes.len(),
            entries.len(),
            "all hostile entries skipped: {:?}",
            outcome.notes
        );
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "sentinel\n");
        // Nothing landed under sounds/skins, and no drive-root writes.
        assert!(!cfg.join("global").join("sounds").exists());
        assert!(!cfg.join("global").join("skins").join("nice").exists());
        assert!(!Path::new("C:/boot.mp3").exists());
    }

    #[test]
    fn manifest_rejection_precedes_any_install() {
        // A valid entry rides in front of the poisoned manifest name;
        // validation must fire before anything at all is written.
        let (root, cfg, sentinel) = base_with_sentinel();
        let pack = root.path().join("mixed.vellumpack");
        let file = std::fs::File::create(&pack).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let manifest = PackManifest {
            format: 1,
            version: "0".into(),
            parts: vec!["highlights".into(), "theme".into()],
            skin: None,
            theme: Some("../escape".into()),
        };
        zip.start_file("manifest.toml", zip_options()).unwrap();
        zip.write_all(toml::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        zip.start_file("global/highlights.toml", zip_options()).unwrap();
        zip.write_all(b"# hl\n").unwrap();
        zip.start_file("themes/../escape.toml", zip_options()).unwrap();
        zip.write_all(b"nope").unwrap();
        zip.finish().unwrap();

        assert!(apply(&cfg, &pack, Some("Testy"), None).is_err());
        assert!(
            !cfg.join("global/highlights.toml").exists(),
            "valid entry must not install when the pack is rejected"
        );
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "sentinel\n");
    }

    #[cfg(windows)]
    fn make_junction(link: &Path, target: &Path) -> bool {
        std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &link.to_string_lossy(),
                &target.to_string_lossy(),
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(windows)]
    #[test]
    fn apply_refuses_junction_redirect_and_prevents_partial_install() {
        let (root, cfg, sentinel) = base_with_sentinel();
        let outside = root.path().join("outside-dir");
        std::fs::create_dir_all(&outside).unwrap();
        // global/sounds is a junction escaping the config base — lexical
        // containment passes, real resolution must not.
        std::fs::create_dir_all(cfg.join("global")).unwrap();
        if !make_junction(&cfg.join("global").join("sounds"), &outside) {
            eprintln!("mklink /J unavailable; skipping junction test");
            return;
        }
        let pack = root.path().join("redirect.vellumpack");
        let file = std::fs::File::create(&pack).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let manifest = PackManifest {
            format: 1,
            version: "0".into(),
            parts: vec!["highlights".into(), "sounds".into()],
            skin: None,
            theme: None,
        };
        zip.start_file("manifest.toml", zip_options()).unwrap();
        zip.write_all(toml::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        // Valid entry FIRST: preflight must stop it from installing when
        // the later entry fails containment.
        zip.start_file("global/highlights.toml", zip_options()).unwrap();
        zip.write_all(b"# hl\n").unwrap();
        zip.start_file("sounds/ding.mp3", zip_options()).unwrap();
        zip.write_all(b"mp3").unwrap();
        zip.finish().unwrap();

        let Err(err) = apply(&cfg, &pack, Some("Testy"), None) else {
            panic!("apply must be rejected");
        };
        assert!(
            err.to_string().contains("sounds/ding.mp3"),
            "error names the rejected entry: {err}"
        );
        assert!(
            !outside.join("ding.mp3").exists(),
            "junction target must stay untouched"
        );
        assert!(
            !cfg.join("global/highlights.toml").exists(),
            "preflight must prevent partial install"
        );
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "sentinel\n");
    }

    #[test]
    fn export_rejects_unsafe_active_names() {
        let dir = base();
        seed(dir.path());
        let extra: Vec<(String, Vec<u8>)> = Vec::new();
        let parts = vec!["skin".to_string()];
        let mut req = full_request(&parts, &extra);
        req.active_skin = Some("../parchment");
        assert!(export(dir.path(), &req).is_err());
        let parts = vec!["theme".to_string()];
        let mut req = full_request(&parts, &extra);
        req.active_theme = Some("mid/night");
        assert!(export(dir.path(), &req).is_err());
    }

    #[test]
    fn resolve_finds_named_and_pathed_packs() {
        let dir = base();
        std::fs::create_dir_all(dir.path().join("exports")).unwrap();
        std::fs::create_dir_all(dir.path().join("imports")).unwrap();
        std::fs::write(dir.path().join("exports/mine.vellumpack"), b"x").unwrap();
        std::fs::write(dir.path().join("imports/theirs.vellumpack"), b"x").unwrap();
        assert!(resolve_pack_path(dir.path(), "mine").is_some());
        assert!(resolve_pack_path(dir.path(), "theirs").is_some());
        assert!(resolve_pack_path(dir.path(), "missing").is_none());
        let abs = dir.path().join("exports").join("mine.vellumpack");
        assert!(resolve_pack_path(dir.path(), abs.to_str().unwrap()).is_some());
        assert_eq!(list_import_packs(dir.path()), vec!["theirs".to_string()]);
    }
}
