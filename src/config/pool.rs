//! Shared image pool scanning and metadata sidecars.
//!
//! The pool (`~/.vellum-fe/global/images/<category>/`) is where `.jinx`
//! installs per-file art and where users can drop their own. This module
//! makes it *discoverable*: list a category's images, group them into sets
//! by filename prefix, and read the optional `<name>.toml` sidecar that
//! carries metadata belonging to the artwork itself — doll calibration
//! anchors, frame nine-slice insets — so art works with no skin active and
//! ships pre-configured through Jinx.
//!
//! Pure config layer: no UI-toolkit imports, shared with the web frontend.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::skins::DollDotSpec;
use super::Config;

/// Image extensions the pool recognizes (matches what the sheet registrar
/// accepts).
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp"];

/// One image in a pool category.
#[derive(Debug, Clone, PartialEq)]
pub struct PoolImage {
    /// File name inside the category folder ("dwarf_ranger.png").
    pub file_name: String,
    /// Pool-relative path ("dolls/dwarf_ranger.png") — the form skins and
    /// overrides reference, resolvable through `skins::resolve_image_path`.
    pub pool_path: String,
    /// Absolute path on disk.
    pub abs_path: PathBuf,
    /// Whether a `<stem>.toml` sidecar existed at scan time. Cached here so
    /// per-frame consumers (the frame picker) don't stat every pool image
    /// every frame.
    pub has_sidecar: bool,
    /// Set folder this image sits in (`Some("stormfront")` for
    /// `compass/stormfront/ne.png`), or `None` for a file at the category
    /// root. Legacy `<set>_<role>` files are at the root and so carry
    /// `None` here — [`PoolImage::set_role`] resolves both forms.
    pub set: Option<String>,
}

impl PoolImage {
    /// File stem ("dwarf_ranger"), the display name in pickers.
    pub fn stem(&self) -> &str {
        self.file_name
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(&self.file_name)
    }

    /// The set this image belongs to, and its role within that set.
    ///
    /// Two layouts are recognized, because a pool can be half-migrated (and
    /// because users drop files in by hand):
    ///
    /// - foldered — `compass/stormfront/ne.png` → `("stormfront", "ne")`
    /// - legacy prefix — `compass/stormfront_ne.png` → `("stormfront", "ne")`
    ///
    /// A file that is neither (no folder, no underscore) belongs to no set.
    /// Roles are lowercased; set names are compared case-insensitively by
    /// [`set_members`], so authors' casing never decides whether art loads.
    pub fn set_role(&self) -> Option<(&str, String)> {
        if let Some(set) = &self.set {
            return Some((set.as_str(), self.stem().to_ascii_lowercase()));
        }
        let (set, role) = self.stem().split_once('_')?;
        if set.is_empty() || role.is_empty() {
            return None;
        }
        Some((set, role.to_ascii_lowercase()))
    }

    /// Sidecar path: `<stem>.toml` beside the image (the same convention the
    /// vellum-assets generator reads for gallery/render metadata).
    pub fn sidecar_path(&self) -> PathBuf {
        self.abs_path.with_extension("toml")
    }

    /// Label for per-image pickers: `"meteor / spellhand"` for set art,
    /// plain `"parchment"` for standalone art.
    ///
    /// Set pieces are named for their role, so every hand set ships a
    /// `lefthand.png` — a stem-only list would be forty identical rows.
    pub fn display_label(&self) -> String {
        match &self.set {
            Some(set) => format!("{set} / {}", self.stem()),
            None => self.stem().to_owned(),
        }
    }
}

/// Category listings live briefly in a cache: pickers in the window
/// context menu re-list their category every frame while open, and a
/// directory read plus per-file stats at 60fps is real I/O for a folder
/// that almost never changes. New files dropped in externally appear
/// within the TTL; in-app pool writes call [`invalidate_cache`] so their
/// results show immediately.
const LIST_CACHE_TTL: Duration = Duration::from_secs(2);

fn list_cache() -> &'static Mutex<HashMap<String, (Instant, Vec<PoolImage>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Vec<PoolImage>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Flush the category-listing cache. Call after writing into the pool
/// (jinx installs, sidecar saves) so the new file is listed on the next
/// frame instead of after the TTL.
pub fn invalidate_cache() {
    if let Ok(mut cache) = list_cache().lock() {
        cache.clear();
    }
}

/// Images in one pool category, sorted by file name. A missing category
/// folder is just an empty list.
pub fn list_category(category: &str) -> Vec<PoolImage> {
    if let Ok(cache) = list_cache().lock() {
        if let Some((at, images)) = cache.get(category) {
            if at.elapsed() < LIST_CACHE_TTL {
                return images.clone();
            }
        }
    }
    let images = scan_category(category);
    if let Ok(mut cache) = list_cache().lock() {
        cache.insert(category.to_owned(), (Instant::now(), images.clone()));
    }
    images
}

fn scan_category(category: &str) -> Vec<PoolImage> {
    let Ok(dir) = Config::global_image_category_dir(category) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    // Root files first, then one level of set folders. Deeper nesting is
    // ignored on purpose: a set is a flat bag of roles, and unbounded
    // recursion would let a stray directory tree stall the 60fps pickers.
    let mut images: Vec<PoolImage> = Vec::new();
    let mut set_dirs: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                set_dirs.push((name.to_owned(), path));
            }
            continue;
        }
        if let Some(image) = pool_image(category, None, &path) {
            images.push(image);
        }
    }
    set_dirs.sort_by(|a, b| a.0.cmp(&b.0));
    for (set, path) in set_dirs {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(image) = pool_image(category, Some(&set), &path) {
                    images.push(image);
                }
            }
        }
    }
    // Sort by pool_path, not file_name: with folders in play, file names
    // collide across sets ("stormfront/n.png" and "stealthblue/n.png" are
    // both "n.png") and a file-name sort would interleave them.
    images.sort_by(|a, b| a.pool_path.cmp(&b.pool_path));
    images
}

/// Build a [`PoolImage`] for one path, or `None` when it isn't image art.
fn pool_image(category: &str, set: Option<&str>, path: &Path) -> Option<PoolImage> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !IMAGE_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let file_name = path.file_name()?.to_str()?.to_owned();
    let has_sidecar = path.with_extension("toml").is_file();
    let pool_path = match set {
        Some(set) => format!("{category}/{set}/{file_name}"),
        None => format!("{category}/{file_name}"),
    };
    Some(PoolImage {
        pool_path,
        abs_path: path.to_path_buf(),
        file_name,
        has_sidecar,
        set: set.map(str::to_owned),
    })
}

/// Distinct set names in a category, sorted and case-insensitively deduped.
///
/// Sets come from set folders (`compass/stormfront/`) and from legacy
/// `<set>_<role>` file names at the category root, unioned — a pool that is
/// mid-migration, or that a user has hand-populated in either style, lists
/// every set exactly once.
pub fn set_names(category: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for image in list_category(category) {
        let Some((set, _)) = image.set_role() else {
            continue;
        };
        if !names.iter().any(|n| n.eq_ignore_ascii_case(set)) {
            names.push(set.to_owned());
        }
    }
    names.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    names
}

/// One set's members as `role -> pool_path` ("ne" → "compass/stormfront/ne.png").
///
/// The single place set membership is decided. Every consumer — the compass
/// and statusicon loaders, `.saveskin`, the editors — goes through here, so
/// folder and legacy-prefix layouts resolve identically everywhere and a set
/// can never render one way in the widget and another in a saved skin.
///
/// When both layouts somehow describe the same role, the foldered file wins:
/// migration writes folders, so the folder is the newer truth.
pub fn set_members(category: &str, set: &str) -> HashMap<String, String> {
    let mut members: HashMap<String, String> = HashMap::new();
    for image in list_category(category) {
        let Some((image_set, role)) = image.set_role() else {
            continue;
        };
        if !image_set.eq_ignore_ascii_case(set) {
            continue;
        }
        if image.set.is_some() {
            members.insert(role, image.pool_path.clone());
        } else {
            members.entry(role).or_insert_with(|| image.pool_path.clone());
        }
    }
    members
}

/// Pool categories whose art groups into sets, and so gets foldered.
///
/// `hands` is a set category even though its pieces are chosen
/// independently: a hand set ships `lefthand`/`righthand`/`spellhand` under
/// one name, and every set now has a bare `lefthand.png`, so the folder is
/// what keeps them apart. Mixing pieces across sets stays a picker choice.
///
/// `frames`/`backgrounds` hold single self-contained images, `icons` holds
/// sprite sheets, and `dolls` need a manifest no filename convention can
/// express — none are sets, and foldering them would only break the paths
/// skins already reference.
pub const SET_CATEGORIES: &[&str] = &["compass", "statusicons", "hands"];

/// Suffix of the one-time pre-migration backup folder.
const MIGRATION_BACKUP_SUFFIX: &str = ".pre-sets.bak";

/// One legacy path rewritten by [`migrate_sets`], as pool-relative strings:
/// `("statusicons/runic_stunned.png", "statusicons/runic/stunned.png")`.
pub type PathRewrite = (String, String);

/// Fold legacy `<set>_<role>` files into set folders, once.
///
/// `compass/stormfront_ne.png` becomes `compass/stormfront/ne.png`, with
/// the image's `.toml` sidecar carried along. Files with no underscore stay
/// at the category root — a bare `rose.png` is not a set member and keeps
/// its path.
///
/// Returns every pool-path rewrite performed so callers can fix up saved
/// references (per-indicator icon overrides name a `pool_path` directly and
/// would otherwise silently go blank).
///
/// Safety: before touching a category, the whole folder is copied to
/// `<category>.pre-sets.bak/`. The backup is never auto-deleted — if art
/// goes missing the originals are one folder away. A category that already
/// has a backup is treated as already migrated and skipped, which is what
/// makes this idempotent across restarts.
///
/// Failures are collected, not propagated: a locked file must not stop the
/// client from starting, and the scanner reads both layouts, so a partial
/// migration still renders.
pub fn migrate_sets() -> Vec<PathRewrite> {
    let mut rewrites = Vec::new();
    for category in SET_CATEGORIES {
        match migrate_category(category) {
            Ok(mut moved) => rewrites.append(&mut moved),
            Err(err) => {
                tracing::warn!("pool: set migration skipped for '{category}': {err}");
            }
        }
    }
    if !rewrites.is_empty() {
        invalidate_cache();
        tracing::info!("pool: foldered {} set file(s)", rewrites.len());
    }
    rewrites
}

fn migrate_category(category: &str) -> Result<Vec<PathRewrite>, String> {
    let dir = Config::global_image_category_dir(category)
        .map_err(|e| format!("cannot resolve pool dir: {e}"))?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let backup = dir.with_file_name(format!(
        "{}{MIGRATION_BACKUP_SUFFIX}",
        dir.file_name().and_then(|n| n.to_str()).unwrap_or(category)
    ));
    // Already migrated on an earlier run.
    if backup.exists() {
        return Ok(Vec::new());
    }

    // Legacy members to fold: root files whose stem splits on '_'. Anything
    // already in a folder, and anything without an underscore, is left alone.
    let mut pending: Vec<(PathBuf, String, String)> = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(image) = pool_image(category, None, &path) else {
            continue;
        };
        let Some((set, _)) = image.stem().split_once('_') else {
            continue;
        };
        let role_ext = image.file_name[set.len() + 1..].to_owned();
        if set.is_empty() || role_ext.is_empty() {
            continue;
        }
        pending.push((path, set.to_ascii_lowercase(), role_ext));
    }
    if pending.is_empty() {
        return Ok(Vec::new());
    }

    copy_dir_shallow(&dir, &backup)
        .map_err(|e| format!("backup to {} failed: {e}", backup.display()))?;

    let mut rewrites = Vec::new();
    for (src, set, role_ext) in pending {
        let set_dir = dir.join(&set);
        if let Err(err) = std::fs::create_dir_all(&set_dir) {
            tracing::warn!("pool: cannot create {}: {err}", set_dir.display());
            continue;
        }
        let dest = set_dir.join(&role_ext);
        if dest.exists() {
            // A foldered file already claims this role; the folder is the
            // newer truth, so drop the legacy copy rather than clobber it.
            continue;
        }
        if let Err(err) = std::fs::rename(&src, &dest) {
            tracing::warn!("pool: cannot move {}: {err}", src.display());
            continue;
        }
        // Carry the sidecar with its image; a doll's calibration or a
        // frame's slice must not be orphaned by the move.
        let sidecar_src = src.with_extension("toml");
        if sidecar_src.is_file() {
            let _ = std::fs::rename(&sidecar_src, dest.with_extension("toml"));
        }
        let Some(old_name) = src.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        rewrites.push((
            format!("{category}/{old_name}"),
            format!("{category}/{set}/{role_ext}"),
        ));
    }
    Ok(rewrites)
}

/// Copy a directory's files (not its subdirectories) into `dest`. The
/// backup only needs the legacy flat layer — set folders, if any already
/// exist, are not what the migration touches.
fn copy_dir_shallow(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name() {
                std::fs::copy(&path, dest.join(name))?;
            }
        }
    }
    Ok(())
}

/// Doll sidecar: calibration anchors and dot styling that travel with the
/// artwork (a Jinx doll can ship pre-calibrated). Same shapes as the
/// `[injury_doll]` skin section.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DollSidecar {
    /// Body part (protocol name, lowercase) -> anchor as fractions of the
    /// image.
    #[serde(default)]
    pub anchors: HashMap<String, [f32; 2]>,
    #[serde(default)]
    pub dots: DollDotSpec,
}

/// Frame sidecar: the nine-slice geometry for a pool frame image. Mirrors
/// the manifest `vellum` block: `slice` may be one number (uniform insets)
/// or four ([top, right, bottom, left]).
#[derive(Debug, Clone, Deserialize)]
pub struct FrameSidecar {
    pub slice: SliceSpec,
    /// Source-pixels → screen-points multiplier. Optional: consumers use
    /// [`FrameSidecar::effective_scale`], which derives a sane value when
    /// the metadata omits one.
    #[serde(default)]
    pub scale: Option<f32>,
}

/// On-screen border thickness (points) a scale-less frame normalizes to —
/// matches what frame authors pick by hand (~14-16pt).
const DEFAULT_FRAME_BORDER_PT: f32 = 15.0;

impl FrameSidecar {
    /// The explicit scale, or one derived by normalizing the largest inset
    /// to ~15 points when the metadata omits it. Slice insets are measured
    /// in source pixels of 1-2K art; treating a missing scale as 1.0 turned
    /// a 635px inset into a 635-POINT border that swallowed the window.
    pub fn effective_scale(&self) -> f32 {
        if let Some(scale) = self.scale {
            return scale;
        }
        let max_inset = self.slice.insets().into_iter().fold(0.0_f32, f32::max);
        if max_inset > 0.0 {
            DEFAULT_FRAME_BORDER_PT / max_inset
        } else {
            1.0
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub enum SliceSpec {
    Uniform(f32),
    PerSide([f32; 4]),
}

impl SliceSpec {
    /// Insets as [top, right, bottom, left].
    pub fn insets(&self) -> [f32; 4] {
        match *self {
            SliceSpec::Uniform(inset) => [inset; 4],
            SliceSpec::PerSide(insets) => insets,
        }
    }
}

/// Read an image's sidecar toml into `T`. `None` when the sidecar doesn't
/// exist or doesn't parse (a broken sidecar is logged, not fatal — the
/// image still lists, just without its metadata).
pub fn read_sidecar<T: serde::de::DeserializeOwned>(image_abs_path: &Path) -> Option<T> {
    let path = image_abs_path.with_extension("toml");
    let contents = std::fs::read_to_string(&path).ok()?;
    match toml::from_str(&contents) {
        Ok(value) => Some(value),
        Err(err) => {
            tracing::warn!("ignoring invalid sidecar {}: {}", path.display(), err);
            None
        }
    }
}

/// Rewrite (or create) a doll sidecar's `anchors` and `dots` tables,
/// preserving any other content byte-for-byte — the pool twin of the
/// skin.toml calibration writer, so calibrating a pool doll saves next to
/// the artwork and travels with it.
pub fn write_doll_sidecar(
    image_abs_path: &Path,
    anchors: &HashMap<String, [f32; 2]>,
    dots: &DollDotSpec,
) -> anyhow::Result<()> {
    use toml_edit::{value, Array, DocumentMut, Item, Table};

    let path = image_abs_path.with_extension("toml");
    let contents = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: DocumentMut = contents
        .parse()
        .map_err(|err| anyhow::anyhow!("{} is not valid TOML: {}", path.display(), err))?;

    // Same rounding as the skin calibration writer: four decimals is
    // sub-pixel on any realistic doll image.
    let rounded = |v: f32, places: f64| (v as f64 * places).round() / places;

    let mut anchors_table = Table::new();
    let mut keys: Vec<&String> = anchors.keys().collect();
    keys.sort();
    for key in keys {
        let [x, y] = anchors[key];
        let mut pair = Array::new();
        pair.push(rounded(x, 10_000.0));
        pair.push(rounded(y, 10_000.0));
        anchors_table.insert(key, value(pair));
    }
    doc.insert("anchors", Item::Table(anchors_table));

    let mut dots_table = Table::new();
    dots_table.insert("wound_color", value(dots.wound_color.as_str()));
    dots_table.insert("scar_color", value(dots.scar_color.as_str()));
    dots_table.insert("opacity", value(rounded(dots.opacity, 100.0)));
    dots_table.insert("diameter", value(rounded(dots.diameter, 1_000.0)));
    doc.insert("dots", Item::Table(dots_table));

    crate::config::write_atomic(&path, doc.to_string())
        .map_err(|err| anyhow::anyhow!("cannot write {}: {}", path.display(), err))?;
    // The listing cache carries has_sidecar; a fresh sidecar must show now.
    invalidate_cache();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_spec_accepts_uniform_and_per_side() {
        #[derive(Deserialize)]
        struct Doc {
            frame: FrameSidecar,
        }
        let uniform: Doc = toml::from_str("[frame]\nslice = 310\n").unwrap();
        assert_eq!(uniform.frame.slice.insets(), [310.0; 4]);
        // No scale in the metadata: derived so the largest inset lands at
        // ~15pt on screen instead of a window-swallowing 310pt.
        assert!((uniform.frame.effective_scale() - 15.0 / 310.0).abs() < 1e-6);

        let per_side: Doc =
            toml::from_str("[frame]\nslice = [1.0, 2.0, 3.0, 4.0]\nscale = 0.5\n").unwrap();
        assert_eq!(per_side.frame.slice.insets(), [1.0, 2.0, 3.0, 4.0]);
        // Explicit scale always wins.
        assert_eq!(per_side.frame.effective_scale(), 0.5);
    }

    #[test]
    fn frame_effective_scale_derives_from_largest_inset() {
        #[derive(Deserialize)]
        struct Doc {
            frame: FrameSidecar,
        }
        // Per-side insets: the LARGEST side normalizes to 15pt so no side
        // exceeds the target.
        let doc: Doc = toml::from_str("[frame]\nslice = [100.0, 600.0, 100.0, 100.0]\n").unwrap();
        assert!((doc.frame.effective_scale() - 15.0 / 600.0).abs() < 1e-6);

        // Degenerate zero insets fall back to 1.0 (nothing to normalize).
        let doc: Doc = toml::from_str("[frame]\nslice = 0\n").unwrap();
        assert_eq!(doc.frame.effective_scale(), 1.0);
    }

    #[test]
    fn doll_sidecar_roundtrips_through_writer() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("human.png");
        std::fs::write(&image, b"png").unwrap();
        // Pre-existing sidecar content survives the calibration write.
        std::fs::write(
            dir.path().join("human.toml"),
            "# hand-written note\ntitle = \"Human\"\n",
        )
        .unwrap();

        let mut anchors = HashMap::new();
        anchors.insert("head".to_string(), [0.5, 0.1]);
        anchors.insert("chest".to_string(), [0.5, 0.3]);
        let dots = DollDotSpec {
            wound_color: "#aa0000".to_string(),
            ..DollDotSpec::default()
        };
        write_doll_sidecar(&image, &anchors, &dots).unwrap();

        let written = std::fs::read_to_string(dir.path().join("human.toml")).unwrap();
        assert!(written.contains("# hand-written note"));
        assert!(written.contains("title = \"Human\""));

        let parsed: DollSidecar = read_sidecar(&image).unwrap();
        assert_eq!(parsed.anchors["head"], [0.5, 0.1]);
        assert_eq!(parsed.anchors["chest"], [0.5, 0.3]);
        assert_eq!(parsed.dots.wound_color, "#aa0000");
    }

    #[test]
    fn read_sidecar_missing_or_broken_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("lonely.png");
        std::fs::write(&image, b"png").unwrap();
        assert!(read_sidecar::<DollSidecar>(&image).is_none());

        std::fs::write(dir.path().join("lonely.toml"), "not = = toml").unwrap();
        assert!(read_sidecar::<DollSidecar>(&image).is_none());
    }

    #[test]
    fn list_and_sets_scan_the_pool_dir() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());
        // The listing cache is process-wide and keyed by category name; a
        // redirected VELLUM_FE_DIR must not serve another test's listing.
        invalidate_cache();

        let cat = Config::global_image_category_dir("statusicons").unwrap();
        std::fs::create_dir_all(&cat).unwrap();
        for name in [
            "runic_stunned.png",
            "runic_hidden.png",
            "flat_stunned.png",
            "notes.txt",       // not an image
            "runic_stunned.toml", // sidecar, not an image
            "plain.png",       // no set prefix
        ] {
            std::fs::write(cat.join(name), b"x").unwrap();
        }

        let images = list_category("statusicons");
        let names: Vec<&str> = images.iter().map(|i| i.file_name.as_str()).collect();
        assert_eq!(
            names,
            ["flat_stunned.png", "plain.png", "runic_hidden.png", "runic_stunned.png"]
        );
        assert_eq!(images[0].pool_path, "statusicons/flat_stunned.png");
        assert_eq!(images[0].stem(), "flat_stunned");

        assert_eq!(set_names("statusicons"), ["flat", "runic"]);
        // Missing category: empty, not an error.
        assert!(list_category("no-such-category").is_empty());

        std::env::remove_var("VELLUM_FE_DIR");
    }

    /// A set folder and legacy prefixed files describe sets the same way, so
    /// a half-migrated pool (or a hand-populated one) lists each set once.
    #[test]
    fn foldered_and_legacy_sets_resolve_alike() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());
        invalidate_cache();

        let cat = Config::global_image_category_dir("compass").unwrap();
        std::fs::create_dir_all(cat.join("stormfront")).unwrap();
        std::fs::write(cat.join("stormfront/ne.png"), b"x").unwrap();
        std::fs::write(cat.join("stormfront/rose.png"), b"x").unwrap();
        // Legacy layout for a different set, plus a loose non-set file.
        std::fs::write(cat.join("stealth_ne.png"), b"x").unwrap();
        std::fs::write(cat.join("plain.png"), b"x").unwrap();

        assert_eq!(set_names("compass"), ["stealth", "stormfront"]);

        let foldered = set_members("compass", "stormfront");
        assert_eq!(foldered.get("ne").unwrap(), "compass/stormfront/ne.png");
        assert_eq!(foldered.get("rose").unwrap(), "compass/stormfront/rose.png");

        // Legacy members resolve to the same role keys.
        let legacy = set_members("compass", "stealth");
        assert_eq!(legacy.get("ne").unwrap(), "compass/stealth_ne.png");

        // Set names match case-insensitively; unknown sets are empty.
        assert_eq!(set_members("compass", "STORMFRONT").len(), 2);
        assert!(set_members("compass", "nosuchset").is_empty());

        // A file with no folder and no underscore belongs to no set, but is
        // still listed as a pool image.
        let images = list_category("compass");
        assert!(images.iter().any(|i| i.pool_path == "compass/plain.png"));
        assert!(images
            .iter()
            .find(|i| i.file_name == "plain.png")
            .unwrap()
            .set_role()
            .is_none());

        std::env::remove_var("VELLUM_FE_DIR");
    }

    /// The foldered file wins when both layouts claim a role — migration
    /// writes folders, so the folder is the newer truth.
    #[test]
    fn foldered_member_wins_over_legacy_duplicate() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());
        invalidate_cache();

        let cat = Config::global_image_category_dir("compass").unwrap();
        std::fs::create_dir_all(cat.join("dup")).unwrap();
        std::fs::write(cat.join("dup/ne.png"), b"new").unwrap();
        std::fs::write(cat.join("dup_ne.png"), b"old").unwrap();

        assert_eq!(
            set_members("compass", "dup").get("ne").unwrap(),
            "compass/dup/ne.png"
        );

        std::env::remove_var("VELLUM_FE_DIR");
    }

    /// Hand sets arrive foldered from the repo and their pieces are bare
    /// role names, so a set is discovered with no filename parsing — and a
    /// one-piece set (spellhand-only art is common) is perfectly normal.
    #[test]
    fn partial_and_single_piece_sets_are_normal() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());
        invalidate_cache();

        let cat = Config::global_image_category_dir("hands").unwrap();
        std::fs::create_dir_all(cat.join("meteor")).unwrap();
        std::fs::write(cat.join("meteor/spellhand.png"), b"x").unwrap();
        std::fs::create_dir_all(cat.join("bone")).unwrap();
        std::fs::write(cat.join("bone/lefthand.png"), b"x").unwrap();
        std::fs::write(cat.join("bone/righthand.png"), b"x").unwrap();

        assert_eq!(set_names("hands"), ["bone", "meteor"]);

        // A single-piece set resolves fine; nothing demands completeness.
        let meteor = set_members("hands", "meteor");
        assert_eq!(meteor.len(), 1);
        assert_eq!(meteor.get("spellhand").unwrap(), "hands/meteor/spellhand.png");

        // Same bare role name in another set stays distinct — the folder is
        // what keeps forty sets' "lefthand.png" apart.
        assert_eq!(
            set_members("hands", "bone").get("lefthand").unwrap(),
            "hands/bone/lefthand.png"
        );

        // Per-image pickers label by set, or every row would read the same.
        let images = list_category("hands");
        let labels: Vec<String> = images.iter().map(|i| i.display_label()).collect();
        assert!(labels.contains(&"bone / lefthand".to_string()));
        assert!(labels.contains(&"meteor / spellhand".to_string()));

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn migration_folders_legacy_sets_and_is_idempotent() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());
        invalidate_cache();

        let cat = Config::global_image_category_dir("compass").unwrap();
        std::fs::create_dir_all(&cat).unwrap();
        std::fs::write(cat.join("stormfront_ne.png"), b"ne").unwrap();
        std::fs::write(cat.join("stormfront_rose.png"), b"rose").unwrap();
        // Sidecars travel with their image.
        std::fs::write(cat.join("stormfront_rose.toml"), b"scale = 2.0").unwrap();
        // Unprefixed art is not a set member and must stay put.
        std::fs::write(cat.join("plain.png"), b"plain").unwrap();

        let rewrites = migrate_sets();
        invalidate_cache();

        assert!(cat.join("stormfront/ne.png").is_file());
        assert!(cat.join("stormfront/rose.png").is_file());
        assert!(cat.join("stormfront/rose.toml").is_file(), "sidecar follows its image");
        assert!(!cat.join("stormfront_ne.png").exists());
        assert!(cat.join("plain.png").is_file(), "non-set art stays at the root");

        // The backup keeps the originals recoverable.
        let backup = cat.with_file_name("compass.pre-sets.bak");
        assert!(backup.join("stormfront_ne.png").is_file());

        // Rewrites describe every moved pool path, for saved references.
        assert!(rewrites.contains(&(
            "compass/stormfront_ne.png".to_string(),
            "compass/stormfront/ne.png".to_string()
        )));

        // Sets resolve through the new layout.
        assert_eq!(set_names("compass"), ["stormfront"]);
        assert_eq!(
            set_members("compass", "stormfront").get("ne").unwrap(),
            "compass/stormfront/ne.png"
        );

        // Running again is a no-op: the backup marks the category done, so a
        // restart can't re-migrate or clobber the backup.
        std::fs::write(cat.join("later_n.png"), b"n").unwrap();
        invalidate_cache();
        assert!(migrate_sets().is_empty(), "second run must not move anything");
        assert!(cat.join("later_n.png").is_file());

        std::env::remove_var("VELLUM_FE_DIR");
    }
}
