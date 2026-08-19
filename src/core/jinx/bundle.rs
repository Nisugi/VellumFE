//! Extracting composed multi-file assets (skins) from a verified zip.
//!
//! A skin is distributed as one zip (a `.vellumpack`) of `skin.toml` plus the
//! art it references. `.jinx` downloads and digest-verifies the whole zip
//! (installer.rs), then this module extracts it into `skins/<name>/` with the
//! same traversal-safety discipline `core/uipack.rs` uses: every entry path is
//! validated to stay inside the skin directory — no `..`, no absolute paths,
//! no backslashes, no empty components. Layouts/UI-packs go through
//! `uipack::apply` instead (they're full `.vellumpack` files with an internal
//! manifest); this is the leaner path for a standalone skin.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::config::Config;

/// Extract a verified skin zip (bytes in memory) into `skins/<name>/`, where
/// `<name>` is the archive's file stem. Returns the skin directory name for
/// the caller to report / suggest activating. Rejects unsafe entry paths.
pub fn install_skin(archive_stem: &str, zip_bytes: &[u8]) -> Result<String, String> {
    let name = sanitize_skin_name(archive_stem)
        .ok_or_else(|| format!("invalid skin name '{archive_stem}'"))?;
    let skins_dir = Config::skins_dir().map_err(|e| format!("cannot resolve skins dir: {e}"))?;
    let dest_root = skins_dir.join(&name);

    let reader = std::io::Cursor::new(zip_bytes);
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("not a valid skin zip: {e}"))?;

    // Collect (safe relative path, bytes) first, so a bad entry aborts before
    // anything is written — no half-extracted skin.
    let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut saw_manifest = false;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("reading skin zip entry {i}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let raw = entry.name().to_string();
        let rel =
            safe_relative(&raw).ok_or_else(|| format!("skin zip has an unsafe path: '{raw}'"))?;
        if rel == Path::new("skin.toml") {
            saw_manifest = true;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("reading '{raw}': {e}"))?;
        files.push((rel, bytes));
    }
    if !saw_manifest {
        return Err("skin archive has no skin.toml".to_string());
    }

    // Fresh install: clear any prior copy so removed files don't linger, then
    // write every entry under skins/<name>/.
    if dest_root.exists() {
        std::fs::remove_dir_all(&dest_root)
            .map_err(|e| format!("cannot replace existing skin '{name}': {e}"))?;
    }
    for (rel, bytes) in files {
        let dest = dest_root.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {} failed: {e}", parent.display()))?;
        }
        std::fs::write(&dest, &bytes)
            .map_err(|e| format!("write {} failed: {e}", dest.display()))?;
    }
    Ok(name)
}

/// Skin directory name from an archive stem: letters, digits, `-`, `_` only
/// (matching `skins::write_scaffold`'s rule), so it's a safe single path
/// component.
fn sanitize_skin_name(stem: &str) -> Option<String> {
    let name = stem.trim();
    if name.is_empty() || name.len() > 64 {
        return None;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .then(|| name.to_string())
}

/// A zip entry path validated to stay within the extraction root: relative,
/// no `..`/`.`/empty components, no backslashes, no drive/absolute prefix.
/// Returns the normalized relative path, or `None` if unsafe.
fn safe_relative(raw: &str) -> Option<PathBuf> {
    if raw.contains('\\') || raw.starts_with('/') {
        return None;
    }
    let mut out = PathBuf::new();
    let mut any = false;
    for part in raw.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        out.push(part);
        any = true;
    }
    any.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VELLUM_FE_DIR_TEST_LOCK as ENV_LOCK;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    /// Build an in-memory zip from (name, bytes) entries.
    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            for (name, bytes) in entries {
                w.start_file(*name, SimpleFileOptions::default()).unwrap();
                w.write_all(bytes).unwrap();
            }
            w.finish().unwrap();
        }
        buf
    }

    #[test]
    fn safe_relative_rejects_traversal_and_absolute() {
        assert!(safe_relative("skin.toml").is_some());
        assert!(safe_relative("bg/paper.png").is_some());
        assert!(safe_relative("../escape.png").is_none());
        assert!(safe_relative("bg/../../escape.png").is_none());
        assert!(safe_relative("/abs.png").is_none());
        assert!(safe_relative("a\\b.png").is_none());
        assert!(safe_relative("").is_none());
        assert!(safe_relative("a//b.png").is_none()); // empty component
    }

    #[test]
    fn sanitize_rejects_bad_names() {
        assert_eq!(
            sanitize_skin_name("parchment").as_deref(),
            Some("parchment")
        );
        assert_eq!(
            sanitize_skin_name("my-skin_2").as_deref(),
            Some("my-skin_2")
        );
        assert!(sanitize_skin_name("no/slash").is_none());
        assert!(sanitize_skin_name("no space").is_none());
        assert!(sanitize_skin_name("..").is_none());
        assert!(sanitize_skin_name("").is_none());
    }

    #[test]
    fn install_extracts_skin_dir_and_replaces_prior() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());

        let zip = make_zip(&[
            ("skin.toml", b"[meta]\nname=\"Parchment\"\n"),
            ("bg/paper.png", b"PNG1"),
        ]);
        let name = install_skin("parchment", &zip).unwrap();
        assert_eq!(name, "parchment");

        let root = crate::config::Config::skins_dir()
            .unwrap()
            .join("parchment");
        assert!(root.join("skin.toml").is_file());
        assert_eq!(std::fs::read(root.join("bg/paper.png")).unwrap(), b"PNG1");

        // Re-install with fewer files: the old bg/paper.png must be gone.
        let zip2 = make_zip(&[("skin.toml", b"[meta]\nname=\"P2\"\n")]);
        install_skin("parchment", &zip2).unwrap();
        assert!(root.join("skin.toml").is_file());
        assert!(
            !root.join("bg/paper.png").exists(),
            "stale file must be cleared"
        );

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn install_rejects_manifestless_and_unsafe_archives() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());

        // No skin.toml -> rejected, nothing written.
        let no_manifest = make_zip(&[("readme.txt", b"hi")]);
        assert!(install_skin("x", &no_manifest)
            .unwrap_err()
            .contains("no skin.toml"));

        // A traversal entry aborts the whole extraction.
        let evil = make_zip(&[("skin.toml", b"x"), ("../evil.png", b"bad")]);
        assert!(install_skin("y", &evil)
            .unwrap_err()
            .contains("unsafe path"));
        assert!(!crate::config::Config::skins_dir()
            .unwrap()
            .join("y")
            .exists());

        std::env::remove_var("VELLUM_FE_DIR");
    }
}
