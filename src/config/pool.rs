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

/// Creature art listing for pickers. Unlike other categories, creatures
/// nest two levels (`creatures/<noun>/<variant>/` — the tier scheme), so
/// the generic one-level scan misses variant folders. Uncached: only
/// editors and the Studio call it, never per-frame paths.
pub fn list_creature_images() -> Vec<PoolImage> {
    let Ok(root) = Config::global_image_category_dir("creatures") else {
        return Vec::new();
    };
    let mut images: Vec<PoolImage> = Vec::new();
    let mut walk = |dir: &Path, set: Option<&str>, images: &mut Vec<PoolImage>| {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                subdirs.push(path);
            } else if let Some(image) = pool_image("creatures", set, &path) {
                images.push(image);
            }
        }
        subdirs
    };
    for noun_dir in walk(&root, None, &mut images) {
        let Some(noun) = noun_dir.file_name().and_then(|n| n.to_str()).map(String::from) else {
            continue;
        };
        for variant_dir in walk(&noun_dir, Some(&noun), &mut images) {
            let Some(variant) = variant_dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let set = format!("{noun}/{variant}");
            walk(&variant_dir, Some(&set), &mut images);
        }
    }
    images.sort_by(|a, b| a.pool_path.cmp(&b.pool_path));
    images
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
            members
                .entry(role)
                .or_insert_with(|| image.pool_path.clone());
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
pub const SET_CATEGORIES: &[&str] = &["compass", "statusicons", "hands", "edges"];

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

/// A sidecar schema's `kind` discriminator. All sidecar types share the
/// `<image>.toml` slot; the discriminator keeps a doll's metadata from
/// silently parsing as a frame's. Legacy sidecars without a `kind` field
/// still load (every writer stamps it going forward).
pub trait SidecarKind {
    const KIND: &'static str;
    /// The `kind` the file declared, if any.
    fn declared_kind(&self) -> Option<&str>;
}

/// Doll sidecar: calibration anchors and dot styling that travel with the
/// artwork (a Jinx doll can ship pre-calibrated). Same shapes as the
/// `[injury_doll]` skin section.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DollSidecar {
    /// Schema discriminator; see [`SidecarKind`].
    #[serde(default)]
    pub kind: Option<String>,
    /// Body part (protocol name, lowercase) -> anchor as fractions of the
    /// image.
    #[serde(default)]
    pub anchors: HashMap<String, [f32; 2]>,
    #[serde(default)]
    pub dots: DollDotSpec,
}

impl SidecarKind for DollSidecar {
    const KIND: &'static str = "doll";
    fn declared_kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }
}

/// Creature-sprite sidecar: pose anchoring metadata that travels with one
/// creature image (`<image>.toml` next to `<image>.png`). Every pose
/// variant image carries its own sidecar, so swapping a standing base for
/// prone art re-hangs the sprite off the correct ground contact with no
/// code-side offsets. Anchors are fractions of the full image canvas;
/// names share the creature-card vocabulary (feet/head/mouth/saddle) plus
/// doll part names for wound placement.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreatureSidecar {
    /// Schema discriminator; see [`SidecarKind`].
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub anchors: HashMap<String, [f32; 2]>,
    /// Contact-shadow / floor-footprint ellipse. Absent = the renderer's
    /// generic standee shadow.
    #[serde(default)]
    pub footprint: Option<CreatureFootprint>,
    /// World-unit height of THIS image's creature, overriding the
    /// per-family size the field otherwise uses — art from different
    /// sources stays in scale with each other. Absent = family default.
    #[serde(default)]
    pub size: Option<f32>,
    /// Template-canvas calibration (`docs/art/scale-spec.md`): the image
    /// was authored on the standard canvas where this many pixels equal
    /// one foot (spec default 32), baseline on the bottom edge. The
    /// loader derives world size from the CANVAS height (not the alpha
    /// bbox) and grounds the sprite on the bottom edge, so template art
    /// self-calibrates. An explicit `size` still wins when both are set.
    #[serde(default)]
    pub px_per_foot: Option<f32>,
    /// Ground clearance for a neutral pose that floats (wisps, spectres),
    /// as a fraction of the drawn sprite height. Absent = grounded.
    #[serde(default)]
    pub lift: Option<f32>,
    /// Overlay art only (creatures/wounds, creatures/status): drawn width
    /// as a fraction of the wearing creature's drawn width — sized
    /// relative to the creature so world-size scaling is inherent, with
    /// the fraction itself user-calibrated instead of hardcoded. Absent =
    /// the renderer's per-role default.
    #[serde(default)]
    pub overlay_scale: Option<f32>,
    /// Scenery-only: exclusion edges as [left, right] fractions of the
    /// image width. Present = a placed prop blocks creature placement
    /// BEHIND that lateral span (never in front or beside). Written by the
    /// scenery calibrator.
    #[serde(default)]
    pub exclude: Option<[f32; 2]>,
    /// Scenery-only: image aspect (width/height), recorded at calibration
    /// so core can size the exclusion span without decoding the image.
    #[serde(default)]
    pub aspect: Option<f32>,
}

impl SidecarKind for CreatureSidecar {
    const KIND: &'static str = "creature";
    fn declared_kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }
}

/// Footprint ellipse for a creature sprite: how much floor the pose
/// occupies. Radii are fractions of the sprite's drawn width so the same
/// numbers work at any stage zoom; a prone pose authors a wider, longer
/// ellipse than its standing twin.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CreatureFootprint {
    /// Half-width of the ellipse as a fraction of the drawn sprite width.
    pub rx: f32,
    /// Half-depth as a fraction of the drawn sprite width. Omitted =
    /// rx * 0.24, matching the generic shadow's squash.
    #[serde(default)]
    pub ry: Option<f32>,
    /// Ellipse centre in image fractions (x only is honored — the shadow
    /// always lies on the ground line). Omitted = the feet anchor.
    #[serde(default)]
    pub center: Option<[f32; 2]>,
}

impl CreatureFootprint {
    /// The half-depth, defaulting to the generic shadow's 0.24 squash.
    pub fn effective_ry(&self) -> f32 {
        self.ry.unwrap_or(self.rx * 0.24)
    }
}

/// Frame sidecar: the nine-slice geometry for a pool frame image. Mirrors
/// the manifest `vellum` block: `slice` may be one number (uniform insets)
/// or four ([top, right, bottom, left]).
#[derive(Debug, Clone, Deserialize)]
pub struct FrameSidecar {
    /// Schema discriminator; see [`SidecarKind`].
    #[serde(default)]
    pub kind: Option<String>,
    pub slice: SliceSpec,
    /// Source-pixels → screen-points multiplier. Optional: consumers use
    /// [`FrameSidecar::effective_scale`], which derives a sane value when
    /// the metadata omits one.
    #[serde(default)]
    pub scale: Option<f32>,
}

impl SidecarKind for FrameSidecar {
    const KIND: &'static str = "frame";
    fn declared_kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }
}

/// Edge-strip sidecar: paint parameters for one pool edge image
/// (`edges/<set>/<side>.png`). Mirrors the manifest `[edges.*]` fields
/// that describe the strip rather than name images.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EdgeSidecar {
    /// Schema discriminator; see [`SidecarKind`].
    #[serde(default)]
    pub kind: Option<String>,
    /// `true` tiles the strip along the edge; `false` (default) stretches.
    #[serde(default)]
    pub tile: bool,
    /// Which end the corner ornament anchors to: "start" (top/left,
    /// default) or "end" (bottom/right).
    #[serde(default)]
    pub anchor: Option<String>,
    /// Inward reach of the overlay in source px (× `scale`); absent = the
    /// strip's own cross-axis size.
    #[serde(default)]
    pub thickness: Option<f32>,
    /// Source-px → on-screen-point multiplier.
    #[serde(default)]
    pub scale: Option<f32>,
}

impl SidecarKind for EdgeSidecar {
    const KIND: &'static str = "edge";
    fn declared_kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }
}

/// Background sidecar: how one pool background image wants to be painted
/// into a window (`<image>.toml` next to `backgrounds/<image>.png`). Fit
/// is a property of the artwork — a seamless mesh tiles, a vista covers —
/// so it travels with the image instead of living in any layout.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BackgroundSidecar {
    /// Schema discriminator; see [`SidecarKind`].
    #[serde(default)]
    pub kind: Option<String>,
    /// Paint mode: "stretch" | "cover" | "contain" | "tile" | "center".
    /// Absent (or unrecognized) = the renderer's cover default.
    #[serde(default)]
    pub fit: Option<String>,
    /// Tile-mode scale multiplier over the image's native size. Absent =
    /// 1.0; consumers clamp via [`BackgroundSidecar::effective_scale`].
    #[serde(default)]
    pub scale: Option<f32>,
}

impl SidecarKind for BackgroundSidecar {
    const KIND: &'static str = "background";
    fn declared_kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }
}

impl BackgroundSidecar {
    /// The tile scale, defaulted and clamped to a sane range — hand-edited
    /// metadata must never zero-size (or explode) the tile grid.
    pub fn effective_scale(&self) -> f32 {
        self.scale.unwrap_or(1.0).clamp(0.05, 8.0)
    }
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

/// Read an image's sidecar toml into `T`. `None` when no metadata exists
/// or it doesn't parse (a broken sidecar is logged, not fatal — the image
/// still lists, just without its metadata).
///
/// Resolution order: the `.toml` sidecar file (the working copy) wins;
/// with no sidecar file, metadata embedded in the PNG itself (the travel
/// format — see `config::png_meta`) is read and extracted to a fresh
/// sidecar file, so a shared image self-hydrates on first use. A `kind`
/// mismatch (a doll sidecar read as a frame) is rejected with a warning;
/// legacy metadata without a `kind` still loads.
pub fn read_sidecar<T: serde::de::DeserializeOwned + SidecarKind>(
    image_abs_path: &Path,
) -> Option<T> {
    let path = image_abs_path.with_extension("toml");
    if let Ok(contents) = std::fs::read_to_string(&path) {
        return parse_sidecar::<T>(&contents, &path.display().to_string());
    }
    // No sidecar file: hydrate from metadata embedded in the image.
    let embedded = crate::config::png_meta::read_embedded(image_abs_path)?;
    let value = parse_sidecar::<T>(&embedded, &image_abs_path.display().to_string())?;
    match crate::config::write_atomic(&path, embedded) {
        Ok(()) => invalidate_cache(), // has_sidecar flags must show the new file
        Err(err) => tracing::warn!(
            "cannot extract embedded metadata to {}: {}",
            path.display(),
            err
        ),
    }
    Some(value)
}

fn parse_sidecar<T: serde::de::DeserializeOwned + SidecarKind>(
    contents: &str,
    source: &str,
) -> Option<T> {
    let value: T = match toml::from_str(contents) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!("ignoring invalid sidecar {}: {}", source, err);
            return None;
        }
    };
    match value.declared_kind() {
        Some(kind) if !kind.eq_ignore_ascii_case(T::KIND) => {
            tracing::warn!(
                "sidecar {} declares kind '{}', wanted '{}' — ignoring",
                source,
                kind,
                T::KIND
            );
            None
        }
        _ => Some(value),
    }
}

/// Round for TOML output in f64: the raw f32 -> f64 cast would smear 0.09
/// into 0.09000000357... in the written file. Four decimals is sub-pixel
/// on any realistic art. Shared by every calibration writer.
pub fn toml_rounded(v: f32, places: f64) -> f64 {
    (v as f64 * places).round() / places
}

/// Build the `anchors` TOML table every calibration writer emits: sorted
/// keys, `[x, y]` pairs rounded to four decimals.
pub fn anchors_toml_table(anchors: &HashMap<String, [f32; 2]>) -> toml_edit::Table {
    use toml_edit::{value, Array, Table};
    let mut table = Table::new();
    let mut keys: Vec<&String> = anchors.keys().collect();
    keys.sort();
    for key in keys {
        let [x, y] = anchors[key];
        let mut pair = Array::new();
        pair.push(toml_rounded(x, 10_000.0));
        pair.push(toml_rounded(y, 10_000.0));
        table.insert(key, value(pair));
    }
    table
}

/// Build the `dots` TOML table (doll dot styling), shared by every
/// calibration writer.
pub fn dots_toml_table(dots: &DollDotSpec) -> toml_edit::Table {
    use toml_edit::{value, Table};
    let mut table = Table::new();
    table.insert("wound_color", value(dots.wound_color.as_str()));
    table.insert("scar_color", value(dots.scar_color.as_str()));
    table.insert("opacity", value(toml_rounded(dots.opacity, 100.0)));
    table.insert("diameter", value(toml_rounded(dots.diameter, 1_000.0)));
    table
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
    use toml_edit::Item;
    write_sidecar_tables(image_abs_path, DollSidecar::KIND, |doc| {
        doc.insert("anchors", Item::Table(anchors_toml_table(anchors)));
        doc.insert("dots", Item::Table(dots_toml_table(dots)));
        Ok(())
    })
}

/// Rewrite (or create) a creature sidecar: anchors, footprint, and the
/// per-image field-scale fields, preserving any other content.
pub fn write_creature_sidecar(
    image_abs_path: &Path,
    sidecar: &CreatureSidecar,
) -> anyhow::Result<()> {
    use toml_edit::{value, Item, Table};
    write_sidecar_tables(image_abs_path, CreatureSidecar::KIND, |doc| {
        doc.insert("anchors", Item::Table(anchors_toml_table(&sidecar.anchors)));
        match &sidecar.footprint {
            Some(fp) => {
                let mut table = Table::new();
                table.insert("rx", value(toml_rounded(fp.rx, 10_000.0)));
                if let Some(ry) = fp.ry {
                    table.insert("ry", value(toml_rounded(ry, 10_000.0)));
                }
                if let Some([x, y]) = fp.center {
                    let mut pair = toml_edit::Array::new();
                    pair.push(toml_rounded(x, 10_000.0));
                    pair.push(toml_rounded(y, 10_000.0));
                    table.insert("center", value(pair));
                }
                doc.insert("footprint", Item::Table(table));
            }
            None => {
                doc.remove("footprint");
            }
        }
        for (key, field) in [
            ("size", sidecar.size),
            ("px_per_foot", sidecar.px_per_foot),
            ("lift", sidecar.lift),
            ("aspect", sidecar.aspect),
            ("overlay_scale", sidecar.overlay_scale),
        ] {
            match field {
                Some(v) => {
                    doc.insert(key, value(toml_rounded(v, 10_000.0)));
                }
                None => {
                    doc.remove(key);
                }
            }
        }
        match sidecar.exclude {
            Some([left, right]) => {
                let mut pair = toml_edit::Array::new();
                pair.push(toml_rounded(left, 10_000.0));
                pair.push(toml_rounded(right, 10_000.0));
                doc.insert("exclude", value(pair));
            }
            None => {
                doc.remove("exclude");
            }
        }
        Ok(())
    })
}

/// Rewrite (or create) a frame sidecar's nine-slice geometry, preserving
/// any other content.
pub fn write_frame_sidecar(
    image_abs_path: &Path,
    slice: [f32; 4],
    scale: Option<f32>,
) -> anyhow::Result<()> {
    use toml_edit::value;
    write_sidecar_tables(image_abs_path, FrameSidecar::KIND, |doc| {
        let uniform = slice.iter().all(|inset| *inset == slice[0]);
        if uniform {
            doc.insert("slice", value(toml_rounded(slice[0], 10.0)));
        } else {
            let mut arr = toml_edit::Array::new();
            for inset in slice {
                arr.push(toml_rounded(inset, 10.0));
            }
            doc.insert("slice", value(arr));
        }
        match scale {
            Some(scale) => {
                doc.insert("scale", value(toml_rounded(scale, 10_000.0)));
            }
            None => {
                doc.remove("scale");
            }
        }
        Ok(())
    })
}

/// Rewrite (or create) a background sidecar's fit/scale fields, preserving
/// any other content. `None` removes a field (revert to the default).
pub fn write_background_sidecar(
    image_abs_path: &Path,
    sidecar: &BackgroundSidecar,
) -> anyhow::Result<()> {
    use toml_edit::value;
    write_sidecar_tables(image_abs_path, BackgroundSidecar::KIND, |doc| {
        match sidecar.fit.as_deref() {
            Some(fit) => {
                doc.insert("fit", value(fit));
            }
            None => {
                doc.remove("fit");
            }
        }
        match sidecar.scale {
            Some(scale) => {
                doc.insert("scale", value(toml_rounded(scale, 10_000.0)));
            }
            None => {
                doc.remove("scale");
            }
        }
        Ok(())
    })
}

/// Shared sidecar-writer plumbing: parse the existing sidecar (preserving
/// hand-written content byte-for-byte), let `fill` upsert its tables,
/// stamp the `kind` discriminator, write atomically, and bake the same
/// TOML into the image as embedded metadata (`png_meta`) so the file
/// stays shareable with its calibration inside. A non-PNG image skips the
/// bake with a debug note — the sidecar file is the working copy either
/// way.
fn write_sidecar_tables(
    image_abs_path: &Path,
    kind: &str,
    fill: impl FnOnce(&mut toml_edit::DocumentMut) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    use toml_edit::{value, DocumentMut};

    let path = image_abs_path.with_extension("toml");
    let contents = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: DocumentMut = contents
        .parse()
        .map_err(|err| anyhow::anyhow!("{} is not valid TOML: {}", path.display(), err))?;
    doc.insert("kind", value(kind));
    fill(&mut doc)?;
    let toml = doc.to_string();
    crate::config::write_atomic(&path, &toml)
        .map_err(|err| anyhow::anyhow!("cannot write {}: {}", path.display(), err))?;
    // The listing cache carries has_sidecar; a fresh sidecar must show now.
    invalidate_cache();
    if let Err(err) = crate::config::png_meta::write_embedded(image_abs_path, &toml) {
        tracing::debug!(
            "not embedding metadata in {}: {}",
            image_abs_path.display(),
            err
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creature_sidecar_parses_anchors_and_footprint() {
        let sidecar: CreatureSidecar = toml::from_str(
            r#"
            [anchors]
            feet    = [0.48, 0.63]
            head    = [0.18, 0.22]
            leftLeg = [0.40, 0.55]

            [footprint]
            rx = 0.46
            center = [0.50, 0.63]
            "#,
        )
        .unwrap();
        assert_eq!(sidecar.anchors["feet"], [0.48, 0.63]);
        assert_eq!(sidecar.anchors["leftLeg"], [0.40, 0.55]);
        let fp = sidecar.footprint.unwrap();
        assert_eq!(fp.rx, 0.46);
        assert_eq!(fp.center, Some([0.50, 0.63]));
        // ry omitted: the generic shadow squash applies.
        assert!((fp.effective_ry() - 0.46 * 0.24).abs() < 1e-6);
    }

    #[test]
    fn creature_sidecar_defaults_are_empty() {
        let sidecar: CreatureSidecar = toml::from_str("").unwrap();
        assert!(sidecar.anchors.is_empty());
        assert!(sidecar.footprint.is_none());
    }

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
    fn sidecar_kind_mismatch_is_rejected_legacy_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("art.png");
        std::fs::write(&image, b"png").unwrap();

        // Legacy sidecar without a kind: accepted by any schema (that is
        // how every pre-discriminator sidecar in the wild reads).
        std::fs::write(dir.path().join("art.toml"), "[anchors]\nhead = [0.5, 0.1]\n").unwrap();
        assert!(read_sidecar::<DollSidecar>(&image).is_some());
        assert!(read_sidecar::<CreatureSidecar>(&image).is_some());

        // A declared kind gates cross-schema reads, case-insensitively.
        std::fs::write(
            dir.path().join("art.toml"),
            "kind = \"doll\"\n[anchors]\nhead = [0.5, 0.1]\n",
        )
        .unwrap();
        assert!(read_sidecar::<DollSidecar>(&image).is_some());
        assert!(read_sidecar::<CreatureSidecar>(&image).is_none());
        assert!(read_sidecar::<FrameSidecar>(&image).is_none());
    }

    #[test]
    fn creature_sidecar_roundtrips_size_lift_footprint_through_writer() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("coyote.png");
        std::fs::write(&image, b"png").unwrap();

        let mut sidecar = CreatureSidecar {
            size: Some(1.25),
            px_per_foot: Some(32.0),
            lift: Some(0.1),
            footprint: Some(CreatureFootprint {
                rx: 0.46,
                ry: None,
                center: Some([0.5, 0.63]),
            }),
            ..Default::default()
        };
        sidecar.anchors.insert("feet".to_string(), [0.48, 0.63]);
        sidecar.anchors.insert("mouth".to_string(), [0.2, 0.3]);
        write_creature_sidecar(&image, &sidecar).unwrap();

        let read: CreatureSidecar = read_sidecar(&image).unwrap();
        assert_eq!(read.kind.as_deref(), Some("creature"));
        assert_eq!(read.size, Some(1.25));
        assert_eq!(read.px_per_foot, Some(32.0));
        assert_eq!(read.lift, Some(0.1));
        assert_eq!(read.anchors["mouth"], [0.2, 0.3]);
        let fp = read.footprint.unwrap();
        assert_eq!(fp.rx, 0.46);
        assert_eq!(fp.center, Some([0.5, 0.63]));
        assert!(fp.ry.is_none());

        // Clearing optional fields removes them from the file.
        sidecar.size = None;
        sidecar.px_per_foot = None;
        sidecar.footprint = None;
        write_creature_sidecar(&image, &sidecar).unwrap();
        let read: CreatureSidecar = read_sidecar(&image).unwrap();
        assert!(read.size.is_none());
        assert!(read.px_per_foot.is_none());
        assert!(read.footprint.is_none());
        assert_eq!(read.lift, Some(0.1));
    }

    #[test]
    fn background_sidecar_roundtrips_through_writer() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("mesh.png");
        std::fs::write(&image, b"png").unwrap();

        let sidecar = BackgroundSidecar {
            fit: Some("tile".to_string()),
            scale: Some(2.0),
            ..Default::default()
        };
        write_background_sidecar(&image, &sidecar).unwrap();
        let read: BackgroundSidecar = read_sidecar(&image).unwrap();
        assert_eq!(read.kind.as_deref(), Some("background"));
        assert_eq!(read.fit.as_deref(), Some("tile"));
        assert_eq!(read.scale, Some(2.0));
        assert_eq!(read.effective_scale(), 2.0);

        // Clearing optional fields removes them from the file.
        write_background_sidecar(&image, &BackgroundSidecar::default()).unwrap();
        let read: BackgroundSidecar = read_sidecar(&image).unwrap();
        assert!(read.fit.is_none());
        assert!(read.scale.is_none());
        assert_eq!(read.effective_scale(), 1.0);

        // The kind discriminator gates cross-schema reads; legacy
        // kind-less metadata still loads.
        assert!(read_sidecar::<DollSidecar>(&image).is_none());
        std::fs::write(dir.path().join("mesh.toml"), "fit = \"tile\"\n").unwrap();
        let read: BackgroundSidecar = read_sidecar(&image).unwrap();
        assert_eq!(read.fit.as_deref(), Some("tile"));
    }

    #[test]
    fn background_sidecar_scale_clamps_at_read() {
        let sidecar = BackgroundSidecar {
            scale: Some(0.0001),
            ..Default::default()
        };
        assert_eq!(sidecar.effective_scale(), 0.05);
        let sidecar = BackgroundSidecar {
            scale: Some(1000.0),
            ..Default::default()
        };
        assert_eq!(sidecar.effective_scale(), 8.0);
    }

    #[test]
    fn frame_sidecar_writer_emits_uniform_or_per_side() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("brass.png");
        std::fs::write(&image, b"png").unwrap();

        write_frame_sidecar(&image, [300.0; 4], None).unwrap();
        let read: FrameSidecar = read_sidecar(&image).unwrap();
        assert_eq!(read.kind.as_deref(), Some("frame"));
        assert_eq!(read.slice.insets(), [300.0; 4]);
        assert!(read.scale.is_none());
        // The uniform form writes one number, not a 4-array.
        let text = std::fs::read_to_string(dir.path().join("brass.toml")).unwrap();
        assert!(text.contains("slice = 300"), "uniform slice: {text}");

        write_frame_sidecar(&image, [1.0, 2.0, 3.0, 4.0], Some(0.5)).unwrap();
        let read: FrameSidecar = read_sidecar(&image).unwrap();
        assert_eq!(read.slice.insets(), [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(read.scale, Some(0.5));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn embedded_metadata_hydrates_a_sidecar_on_first_read() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("shared.png");
        image::save_buffer(&image, &[0xff; 4], 1, 1, image::ExtendedColorType::Rgba8).unwrap();

        // A shared image arrives with metadata inside and no sidecar file.
        crate::config::png_meta::write_embedded(
            &image,
            "kind = \"creature\"\nsize = 2.0\n[anchors]\nfeet = [0.5, 0.9]\n",
        )
        .unwrap();
        assert!(!dir.path().join("shared.toml").exists());

        let read: CreatureSidecar = read_sidecar(&image).unwrap();
        assert_eq!(read.size, Some(2.0));
        assert_eq!(read.anchors["feet"], [0.5, 0.9]);
        // First read extracted the working copy next to the art.
        assert!(dir.path().join("shared.toml").exists());

        // The sidecar file now wins: divergent edits there are what loads.
        std::fs::write(dir.path().join("shared.toml"), "kind = \"creature\"\nsize = 3.0\n")
            .unwrap();
        let read: CreatureSidecar = read_sidecar(&image).unwrap();
        assert_eq!(read.size, Some(3.0));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn sidecar_writers_bake_embedded_metadata_into_png_images() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("cal.png");
        image::save_buffer(&image, &[0xff; 4], 1, 1, image::ExtendedColorType::Rgba8).unwrap();

        let mut anchors = HashMap::new();
        anchors.insert("head".to_string(), [0.5, 0.1]);
        write_doll_sidecar(&image, &anchors, &DollDotSpec::default()).unwrap();

        // The image now carries the identical TOML inside: share the PNG
        // alone and the calibration travels.
        let embedded = crate::config::png_meta::read_embedded(&image).unwrap();
        let sidecar_file = std::fs::read_to_string(dir.path().join("cal.toml")).unwrap();
        assert_eq!(embedded, sidecar_file);
        assert!(embedded.contains("kind = \"doll\""));
        // And the baked PNG still decodes.
        assert!(image::open(&image).is_ok());
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
            "notes.txt",          // not an image
            "runic_stunned.toml", // sidecar, not an image
            "plain.png",          // no set prefix
        ] {
            std::fs::write(cat.join(name), b"x").unwrap();
        }

        let images = list_category("statusicons");
        let names: Vec<&str> = images.iter().map(|i| i.file_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "flat_stunned.png",
                "plain.png",
                "runic_hidden.png",
                "runic_stunned.png"
            ]
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
        assert_eq!(
            meteor.get("spellhand").unwrap(),
            "hands/meteor/spellhand.png"
        );

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
        assert!(
            cat.join("stormfront/rose.toml").is_file(),
            "sidecar follows its image"
        );
        assert!(!cat.join("stormfront_ne.png").exists());
        assert!(
            cat.join("plain.png").is_file(),
            "non-set art stays at the root"
        );

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
        assert!(
            migrate_sets().is_empty(),
            "second run must not move anything"
        );
        assert!(cat.join("later_n.png").is_file());

        std::env::remove_var("VELLUM_FE_DIR");
    }
}
