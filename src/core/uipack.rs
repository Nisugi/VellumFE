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
//!   skins/<name>/**          the active skin's directory
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
];

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
}

pub struct PackPreview {
    pub manifest: PackManifest,
    pub entries: Vec<String>,
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

fn zip_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
}

/// Build a pack under `<base>/exports/<name>.vellumpack`.
///
/// `layout_toml` is the exporting session's TUI layout (already
/// serialized); `extra_files` carries frontend-owned entries like the
/// GUI layout. Returns the pack path and the parts actually included
/// (a part with no files on disk drops out silently).
pub fn export(
    base: &Path,
    name: &str,
    parts: &[String],
    character: Option<&str>,
    layout_toml: Option<String>,
    active_skin: Option<&str>,
    extra_files: &[(String, Vec<u8>)],
) -> Result<(PathBuf, Vec<String>)> {
    if !is_valid_pack_name(name) {
        bail!("Pack names use letters, digits, '-' and '_' only");
    }
    for part in parts {
        if !PARTS.contains(&part.as_str()) {
            bail!(
                "Unknown part '{}' — parts are: {}",
                part,
                PARTS.join(", ")
            );
        }
    }

    let exports_dir = base.join("exports");
    std::fs::create_dir_all(&exports_dir).context("Failed to create exports directory")?;
    let pack_path = exports_dir.join(format!("{name}.vellumpack"));
    let file = std::fs::File::create(&pack_path)
        .with_context(|| format!("Failed to create {pack_path:?}"))?;
    let mut zip = zip::ZipWriter::new(file);

    let mut included: Vec<String> = Vec::new();
    let wants = |part: &str| parts.iter().any(|p| p == part);

    if wants("layout") {
        if let Some(toml) = layout_toml.as_deref() {
            zip.start_file("layout.toml", zip_options())?;
            zip.write_all(toml.as_bytes())?;
            included.push("layout".to_string());
        }
        for (entry, bytes) in extra_files {
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
        if let Some(character) = character {
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
        if let Some(skin) = active_skin.filter(|s| !s.is_empty()) {
            let skin_dir = base.join("skins").join(skin);
            if skin_dir.is_dir() {
                let mut files = Vec::new();
                collect_files(&skin_dir, &mut files)?;
                files.sort();
                for path in files {
                    let rel = path
                        .strip_prefix(&skin_dir)
                        .expect("collected under skin_dir");
                    let entry = format!(
                        "skins/{skin}/{}",
                        rel.to_string_lossy().replace('\\', "/")
                    );
                    zip.start_file(entry, zip_options())?;
                    zip.write_all(&std::fs::read(&path)?)?;
                }
                included.push("skin".to_string());
                skin_included = Some(skin.to_string());
            }
        }
    }

    let manifest = PackManifest {
        format: 1,
        version: env!("CARGO_PKG_VERSION").to_string(),
        parts: included.clone(),
        skin: skin_included,
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

/// Resolve `.uiimport`'s argument: an exports/ pack name, or a path
/// (absolute, or relative to the config dir).
pub fn resolve_pack_path(base: &Path, arg: &str) -> Option<PathBuf> {
    if is_valid_pack_name(arg) {
        let named = base.join("exports").join(format!("{arg}.vellumpack"));
        if named.is_file() {
            return Some(named);
        }
    }
    let direct = PathBuf::from(arg);
    if direct.is_file() {
        return Some(direct);
    }
    let relative = base.join(arg);
    relative.is_file().then_some(relative)
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
) -> Option<PathBuf> {
    let known_file = |name: &str| LAYERED_FILES.iter().any(|(_, f)| *f == name);
    if entry == "layout.toml" {
        return Some(
            PathBuf::from("layouts").join(format!("{pack_name}.toml")),
        );
    }
    if let Some(name) = entry.strip_prefix("global/") {
        return known_file(name).then(|| PathBuf::from("global").join(name));
    }
    if let Some(name) = entry.strip_prefix("profile/") {
        // Profile entries land on the importing character; without one
        // they fall back to the shared profile dir ("default").
        return known_file(name)
            .then(|| PathBuf::from(character.unwrap_or("default")).join(name));
    }
    if let (Some(rest), Some(skin)) = (entry.strip_prefix("skins/"), skin) {
        // Only the manifest's own skin, and only sane relative paths.
        let mut parts = rest.split('/');
        if parts.next() != Some(skin) {
            return None;
        }
        let tail: Vec<&str> = parts.collect();
        if tail.is_empty()
            || tail
                .iter()
                .any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains('\\'))
        {
            return None;
        }
        let mut path = PathBuf::from("skins").join(skin);
        for part in tail {
            path.push(part);
        }
        return Some(path);
    }
    None
}

/// Apply a pack: back up whatever it overwrites (under
/// `<base>/backups/uipack-<epoch>/`), write the whitelisted entries,
/// and report what happened. Reloads are the caller's job — this layer
/// only moves files.
pub fn apply(base: &Path, path: &Path, character: Option<&str>) -> Result<ApplyOutcome> {
    let PackPreview { manifest, .. } = preview(path)?;
    let pack_name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| is_valid_pack_name(s))
        .unwrap_or_else(|| "imported".to_string());

    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_root = base.join("backups").join(format!("uipack-{epoch}"));
    let mut backed_up = false;

    let mut outcome = ApplyOutcome {
        notes: Vec::new(),
        layout_name: None,
        gui_layout: None,
        skin: manifest.skin.clone(),
        backup_dir: None,
    };

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if name == "manifest.toml" {
            continue;
        }
        if name == GUI_LAYOUT_ENTRY {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            outcome.gui_layout = Some(bytes);
            continue;
        }
        let Some(rel) = entry_destination(
            &name,
            &pack_name,
            character,
            manifest.skin.as_deref(),
        ) else {
            outcome.notes.push(format!("skipped unknown entry '{name}'"));
            continue;
        };
        let dest = base.join(&rel);
        if dest.is_file() {
            let backup = backup_root.join(&rel);
            if let Some(parent) = backup.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&dest, &backup)
                .with_context(|| format!("Failed to back up {dest:?}"))?;
            backed_up = true;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        std::fs::write(&dest, &bytes)
            .with_context(|| format!("Failed to write {dest:?}"))?;
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
        std::fs::create_dir_all(base.join("skins/parchment/icons")).unwrap();
        std::fs::write(base.join("global/keybinds.toml"), "[controller]\n").unwrap();
        std::fs::write(base.join("global/highlights.toml"), "# hl\n").unwrap();
        std::fs::write(base.join("Testy/colors.toml"), "# colors\n").unwrap();
        std::fs::write(base.join("skins/parchment/skin.toml"), "name = 'parchment'\n").unwrap();
        std::fs::write(base.join("skins/parchment/icons/a.png"), b"png-bytes").unwrap();
    }

    fn all_parts() -> Vec<String> {
        PARTS.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn round_trip_export_preview_apply() {
        let dir = base();
        seed(dir.path());
        let (path, included) = export(
            dir.path(),
            "my-ui",
            &all_parts(),
            Some("Testy"),
            Some("# layout\n".to_string()),
            Some("parchment"),
            &[(GUI_LAYOUT_ENTRY.to_string(), b"{\"gui\":1}".to_vec())],
        )
        .unwrap();
        assert!(path.ends_with("exports/my-ui.vellumpack") || path.ends_with("exports\\my-ui.vellumpack"));
        // Only parts with files on disk are included (hotbars/macros had
        // none).
        assert_eq!(
            included,
            vec!["layout", "highlights", "keybinds", "colors", "skin"]
        );

        let preview = preview(&path).unwrap();
        assert_eq!(preview.manifest.skin.as_deref(), Some("parchment"));
        assert!(preview.entries.iter().any(|e| e == "global/keybinds.toml"));
        assert!(preview.entries.iter().any(|e| e == "skins/parchment/icons/a.png"));

        // Import into a fresh base as a different character.
        let target = base();
        std::fs::create_dir_all(target.path().join("global")).unwrap();
        std::fs::write(target.path().join("global/keybinds.toml"), "old\n").unwrap();
        let outcome = apply(target.path(), &path, Some("Newchar")).unwrap();
        assert_eq!(outcome.layout_name.as_deref(), Some("my-ui"));
        assert_eq!(outcome.skin.as_deref(), Some("parchment"));
        assert_eq!(outcome.gui_layout.as_deref(), Some(&b"{\"gui\":1}"[..]));
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
            std::fs::read(target.path().join("skins/parchment/icons/a.png")).unwrap(),
            b"png-bytes"
        );
        // The overwritten global keybinds got backed up.
        let backup = outcome.backup_dir.expect("backup dir");
        assert_eq!(
            std::fs::read_to_string(backup.join("global/keybinds.toml")).unwrap(),
            "old\n"
        );
    }

    #[test]
    fn export_rejects_bad_names_and_parts() {
        let dir = base();
        assert!(export(dir.path(), "../evil", &all_parts(), None, None, None, &[]).is_err());
        assert!(export(
            dir.path(),
            "ok",
            &["passwords".to_string()],
            None,
            None,
            None,
            &[]
        )
        .is_err());
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
            "random.bin",
        ] {
            zip.start_file(evil, zip_options()).unwrap();
            zip.write_all(b"nope").unwrap();
        }
        zip.finish().unwrap();

        let outcome = apply(dir.path(), &pack_path, Some("Testy")).unwrap();
        assert_eq!(outcome.notes.len(), 6, "all six entries skipped: {:?}", outcome.notes);
        assert!(!dir.path().join("global/config.toml").exists());
        assert!(!dir.path().join("Testy/passwords.toml").exists());
        assert!(!dir.path().parent().unwrap().join("outside.toml").exists());
        assert!(!dir.path().join("skins/other").exists());
    }

    #[test]
    fn resolve_finds_named_and_pathed_packs() {
        let dir = base();
        std::fs::create_dir_all(dir.path().join("exports")).unwrap();
        std::fs::write(dir.path().join("exports/mine.vellumpack"), b"x").unwrap();
        assert!(resolve_pack_path(dir.path(), "mine").is_some());
        assert!(resolve_pack_path(dir.path(), "missing").is_none());
        let abs = dir.path().join("exports").join("mine.vellumpack");
        assert!(resolve_pack_path(dir.path(), abs.to_str().unwrap()).is_some());
    }
}
