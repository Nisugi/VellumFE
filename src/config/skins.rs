//! Skin manifest parsing: the frontend-neutral half of the skin system.
//!
//! A skin is a directory under `~/.vellum-fe/global/skins/<name>/` containing a
//! `skin.toml` manifest plus image assets. This module owns the manifest
//! format, loading, and the canonical injury doll part table; textures,
//! painting, and the calibrator's comment-preserving save live in
//! `frontend/gui/skin.rs`. The split matters because the web frontend
//! serves skin data too and compiles without the `gui` feature (the
//! mobile builds).
//!
//! Manifest format:
//!
//! ```toml
//! [meta]
//! name = "Parchment"
//! description = "Warm paper backgrounds for text windows"
//!
//! # Applies to every window without its own [window.<name>] entry.
//! [window.default.background]
//! image = "bg/paper.png"   # relative to the skin directory (absolute paths allowed)
//! fit = "cover"            # stretch | cover | contain | tile | center
//! opacity = 0.85           # 0.0..=1.0
//! tint = "#c0a878"         # optional multiply tint
//! scrim = 0.3              # 0.0..=1.0 theme-colored overlay for text readability
//!
//! # Windows are matched by their layout window name ("main", "thoughts", ...).
//! [window.main.background]
//! image = "bg/vellum.png"
//! scrim = 0.5
//! ```
//!
//! Image paths are usually relative to the skin directory; absolute paths
//! are allowed on purpose so a skin can reference assets from another
//! install (e.g. a user's local Wrayth art) without copying them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Parsed skin.toml.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkinManifest {
    #[serde(default)]
    pub meta: SkinMeta,
    /// Per-window graphics keyed by layout window name; the "default" entry
    /// applies to windows without their own entry.
    #[serde(default, rename = "window")]
    pub windows: HashMap<String, WindowSkin>,
    /// Status icon sprites keyed by indicator id ("kneeling", "STUNNED",
    /// ...; case-insensitive). Replace the built-in vector pictograms in
    /// the dashboard and indicator widgets.
    #[serde(default)]
    pub icons: HashMap<String, String>,
    /// Icon sprite sheets for hotbar buttons, keyed by sheet name. Each is
    /// an image tiled into fixed-size cells (barbar-style: no padding,
    /// indexed 1-based left→right then top→bottom).
    #[serde(default)]
    pub sheets: HashMap<String, SheetSpec>,
    /// Named nine-slice frames users can assign to individual windows from
    /// the GUI (right-click > Appearance > Skin frame). Independent of the
    /// per-window `[window.<name>.border]` entries, which stay the skin's
    /// authored defaults.
    #[serde(default)]
    pub frames: HashMap<String, BorderSpec>,
    /// Sprite compass replacing the vector rose.
    #[serde(default)]
    pub compass: CompassSkin,
    /// Sprite paperdoll replacing the vector injury doll.
    #[serde(default)]
    pub injury_doll: InjuryDollSkin,
}

/// One hotbar icon sprite sheet: an image path (relative to the skin dir)
/// tiled into square cells with no padding.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetSpec {
    /// Image path, relative to the skin directory (absolute allowed).
    pub path: String,
    /// Cell edge in pixels; barbar's convention is 64.
    #[serde(default = "default_sheet_cell")]
    pub cell: u32,
}

fn default_sheet_cell() -> u32 {
    64
}

/// Sprite compass: a full-square rose image plus one full-square overlay
/// per direction, drawn only while that exit is available. Overlays are
/// authored at the same canvas size as the rose, so positioning lives in
/// the art, not the manifest.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CompassSkin {
    #[serde(default)]
    pub rose: Option<String>,
    /// Direction key ("n", "ne", ... "nw") -> lit overlay image.
    #[serde(flatten)]
    pub directions: HashMap<String, String>,
}

/// Sprite injury doll: a base body image plus, per body part, either a
/// full-canvas overlay per severity or a calibrated anchor point where the
/// frontend draws a generated wound/scar dot. Overlay tables are keyed by
/// body part (protocol names: head, neck, chest, ..., leftArm, nsys) with
/// entries injury1-3 and scar1-3.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InjuryDollSkin {
    #[serde(default)]
    pub base: Option<String>,
    /// Calibrated dot positions: part -> [x, y] as fractions (0-1) of the
    /// base image. Written by the in-app calibrator; parts without an
    /// anchor use built-in defaults.
    #[serde(default)]
    pub anchors: HashMap<String, [f32; 2]>,
    /// Styling for the generated dots.
    #[serde(default)]
    pub dots: DollDotSpec,
    /// part -> { injury1 = "...", scar2 = "...", ... }
    #[serde(flatten)]
    pub parts: HashMap<String, HashMap<String, String>>,
}

/// Manifest styling for generated injury dots: a solid circle (wounds) or
/// ring (scars) with the severity numeral inside.
#[derive(Debug, Clone, Deserialize)]
pub struct DollDotSpec {
    /// Fill color for wound dots as "#rrggbb".
    #[serde(default = "default_wound_color")]
    pub wound_color: String,
    /// Ring/numeral color for scar dots as "#rrggbb".
    #[serde(default = "default_scar_color")]
    pub scar_color: String,
    /// Dot opacity, 0.0..=1.0.
    #[serde(default = "default_dot_opacity")]
    pub opacity: f32,
    /// Dot diameter as a fraction of the drawn doll height.
    #[serde(default = "default_dot_diameter")]
    pub diameter: f32,
}

impl Default for DollDotSpec {
    fn default() -> Self {
        Self {
            wound_color: default_wound_color(),
            scar_color: default_scar_color(),
            opacity: default_dot_opacity(),
            diameter: default_dot_diameter(),
        }
    }
}

fn default_wound_color() -> String {
    "#e02020".to_string()
}

fn default_scar_color() -> String {
    "#b8b8b8".to_string()
}

fn default_dot_opacity() -> f32 {
    0.9
}

fn default_dot_diameter() -> f32 {
    0.07
}

/// Canonical body parts: (protocol key, display name, default anchor as
/// fractions of the doll image). Order is the calibrator's click-through
/// order. Back and nervous system have no spot on a front silhouette; by
/// convention they sit in the bottom corners (matching the vector doll's
/// "B"/"N" letters), eyes above the head line.
pub const DOLL_PARTS: &[(&str, &str, [f32; 2])] = &[
    ("head", "head", [0.50, 0.09]),
    ("leftEye", "left eye", [0.44, 0.06]),
    ("rightEye", "right eye", [0.56, 0.06]),
    ("neck", "neck", [0.50, 0.20]),
    ("chest", "chest", [0.50, 0.30]),
    ("abdomen", "abdomen", [0.50, 0.45]),
    ("back", "back", [0.12, 0.92]),
    ("leftArm", "left arm", [0.31, 0.36]),
    ("rightArm", "right arm", [0.69, 0.36]),
    ("leftHand", "left hand", [0.25, 0.53]),
    ("rightHand", "right hand", [0.75, 0.53]),
    ("leftLeg", "left leg", [0.42, 0.75]),
    ("rightLeg", "right leg", [0.58, 0.75]),
    ("nsys", "nervous system", [0.88, 0.92]),
];

/// Built-in anchor for a body part (matched case-insensitively), used when
/// the skin hasn't calibrated one.
pub fn default_doll_anchor(part: &str) -> Option<[f32; 2]> {
    DOLL_PARTS
        .iter()
        .find(|(key, _, _)| key.eq_ignore_ascii_case(part))
        .map(|(_, _, anchor)| *anchor)
}

/// Severity level for an injury-doll overlay key: injury1-3 -> 1-3,
/// scar1-3 -> 4-6.
pub fn severity_level_from_key(key: &str) -> Option<u8> {
    match key {
        "injury1" => Some(1),
        "injury2" => Some(2),
        "injury3" => Some(3),
        "scar1" => Some(4),
        "scar2" => Some(5),
        "scar3" => Some(6),
        _ => None,
    }
}

/// Inverse of `severity_level_from_key`: 1-6 -> the manifest overlay key.
pub fn severity_key_from_level(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("injury1"),
        2 => Some("injury2"),
        3 => Some("injury3"),
        4 => Some("scar1"),
        5 => Some("scar2"),
        6 => Some("scar3"),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkinMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WindowSkin {
    #[serde(default)]
    pub background: Option<BackgroundSpec>,
    #[serde(default)]
    pub border: Option<BorderSpec>,
}

/// Nine-slice border image: the `slice` insets (source pixels, top/right/
/// bottom/left) split the image into corners (drawn fixed), edges
/// (stretched along one axis), and a center (skipped — the window fill or
/// background image shows through).
#[derive(Debug, Clone, Deserialize)]
pub struct BorderSpec {
    /// Image path, relative to the skin directory (absolute allowed).
    pub image: String,
    /// Slice insets in source pixels: [top, right, bottom, left].
    pub slice: [f32; 4],
    /// Multiplier from source pixels to on-screen points for the border
    /// thickness (1.0 = native size).
    #[serde(default = "default_border_scale")]
    pub scale: f32,
}

fn default_border_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackgroundSpec {
    /// Image path, relative to the skin directory (absolute allowed).
    pub image: String,
    #[serde(default)]
    pub fit: BackgroundFit,
    /// Image opacity, 0.0..=1.0.
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Optional multiply tint as "#rrggbb".
    #[serde(default)]
    pub tint: Option<String>,
    /// Strength (0.0..=1.0) of a theme-colored overlay painted over the
    /// image so window text stays readable. 0 disables it.
    #[serde(default)]
    pub scrim: f32,
}

fn default_opacity() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundFit {
    /// Fill the window, distorting aspect ratio.
    Stretch,
    /// Fill the window, cropping whatever overflows.
    #[default]
    Cover,
    /// Show the whole image, letterboxed and centered.
    Contain,
    /// Repeat the image at its native size from the top-left.
    Tile,
    /// Native size, centered, no scaling.
    Center,
}

/// Manifest lookup for a window: exact name, then case-insensitive, then
/// the "default" entry.
pub fn window_background<'a>(
    manifest: &'a SkinManifest,
    window_name: &str,
) -> Option<&'a BackgroundSpec> {
    window_field(manifest, window_name, |window| window.background.as_ref())
}

/// Per-field manifest lookup: the window's own entry (exact name, then
/// case-insensitive), falling back to the "default" entry when the window
/// has no entry or its entry doesn't set this field.
pub fn window_field<'a, T>(
    manifest: &'a SkinManifest,
    window_name: &str,
    field: impl Fn(&'a WindowSkin) -> Option<&'a T>,
) -> Option<&'a T> {
    let entry = manifest.windows.get(window_name).or_else(|| {
        manifest
            .windows
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(window_name))
            .map(|(_, window)| window)
    });
    entry
        .and_then(&field)
        .or_else(|| manifest.windows.get("default").and_then(&field))
}

/// Named-frame lookup (case-insensitive). The reserved name "none" never
/// matches a frame: it means "no frame" wherever an override is stored.
pub fn named_frame<'a>(manifest: &'a SkinManifest, name: &str) -> Option<&'a BorderSpec> {
    if name.eq_ignore_ascii_case(NO_FRAME) {
        return None;
    }
    manifest
        .frames
        .get(name)
        .or_else(|| {
            manifest
                .frames
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, spec)| spec)
        })
}

/// Reserved frame-override value meaning "draw no skin frame".
pub const NO_FRAME: &str = "none";

/// mtime of a skin directory's manifest, if it exists.
pub fn manifest_mtime(root: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(root.join("skin.toml"))
        .and_then(|meta| meta.modified())
        .ok()
}

/// Starter manifest written by `write_scaffold`: every section present but
/// commented out, so making a skin starts as "uncomment and point at a PNG".
/// Kept in sync with docs/SKINS.md; a test asserts it stays parseable.
const SCAFFOLD_MANIFEST: &str = r##"# VellumFE skin manifest.
# Full documentation: docs/SKINS.md in the VellumFE repository.
#
# Image paths are relative to this folder; anything not found here is
# looked up in the shared pool at ~/.vellum-fe/global/images/ (e.g.
# "frames/brass.png"), and absolute paths are allowed. Formats: PNG,
# JPEG, WebP, BMP.
# Activate with `.setskin <folder-name>`. Edits to this file reload
# automatically; after editing images run `.reloadskin`.

[meta]
name = "My Skin"
description = ""

# ---- Window backgrounds ---------------------------------------------------
# "default" applies to every window without its own [window.<name>] entry.
# Windows are matched by layout window name ("main", "thoughts", "combat", ...).
#
# [window.default.background]
# image = "bg/paper.png"
# fit = "cover"          # stretch | cover | contain | tile | center
# opacity = 1.0          # 0.0 - 1.0
# tint = "#c0a878"       # optional multiply tint
# scrim = 0.3            # 0.0 - 1.0 theme-colored overlay so text stays readable

# ---- Window borders (nine-slice) -------------------------------------------
# slice = [top, right, bottom, left] insets in source-image pixels: corners
# draw fixed, edges stretch, the center is never drawn.
#
# [window.default.border]
# image = "border/frame.png"
# slice = [8.0, 8.0, 8.0, 8.0]
# scale = 1.0            # source pixels -> screen points

# ---- Status icons -----------------------------------------------------------
# Indicator id -> sprite (ids are case-insensitive). Used by the dashboard
# and single indicator widgets; ids you don't list keep the vector pictogram.
# The hand widgets look up lefthand/righthand/spellhand here; without them
# the [L]/[R]/[S] text markers stay.
#
# [icons]
# lefthand = "icons/lefthand.png"
# righthand = "icons/righthand.png"
# spellhand = "icons/spellhand.png"
# standing = "icons/standing.png"
# kneeling = "icons/kneeling.png"
# sitting = "icons/sitting.png"
# prone = "icons/prone.png"
# dead = "icons/dead.png"
# stunned = "icons/stunned.png"
# bleeding = "icons/bleeding.png"
# hidden = "icons/hidden.png"
# invisible = "icons/invisible.png"
# webbed = "icons/webbed.png"
# poisoned = "icons/poisoned.png"
# diseased = "icons/diseased.png"
# joined = "icons/joined.png"

# Hotbar icon sprite sheets: images tiled into fixed-size square cells with
# no padding, indexed 1-based left-to-right then top-to-bottom (the barbar
# convention). Hotbar buttons reference them as
#   [bars.buttons.icon] sheet = "<name>", cell = <n>
# in hotbars.toml. "cell" below is the cell edge in pixels (default 64).
#
# [sheets.rogue]
# path = "icons/rogue.png"
# cell = 64

# ---- Compass ----------------------------------------------------------------
# Author the rose and every overlay on the same canvas size; each overlay
# draws on top of the rose only while that exit is available. The hub is
# the "out" exit.
#
# [compass]
# rose = "compass/rose.png"
# n = "compass/n.png"
# ne = "compass/ne.png"
# e = "compass/e.png"
# se = "compass/se.png"
# s = "compass/s.png"
# sw = "compass/sw.png"
# w = "compass/w.png"
# nw = "compass/nw.png"
# up = "compass/up.png"
# down = "compass/down.png"
# out = "compass/out.png"

# ---- Injury doll ------------------------------------------------------------
# A base body image; wounds and scars render as generated dots (solid
# circle = wound, ring = scar, numeral = severity) at calibrated anchor
# points. Calibrate by clicking the doll in Settings > Appearance > Skin >
# "Calibrate injury doll" - it writes the [injury_doll.anchors] and
# [injury_doll.dots] tables here for you. Parts: head, neck, chest,
# abdomen, back, leftArm, rightArm, leftHand, rightHand, leftLeg,
# rightLeg, leftEye, rightEye, nsys.
#
# [injury_doll]
# base = "doll/base.png"
#
# Anchors are [x, y] fractions (0-1) of the base image; parts you don't
# calibrate use built-in defaults.
# [injury_doll.anchors]
# head = [0.50, 0.09]
#
# [injury_doll.dots]
# wound_color = "#e02020"
# scar_color = "#b8b8b8"
# opacity = 0.9
# diameter = 0.07     # fraction of the drawn doll height
#
# A part can instead ship hand-drawn full-canvas overlays per severity
# (injury1-3, scar1-3); overlays take precedence over the generated dot.
# [injury_doll.head]
# injury1 = "doll/head_i1.png"
# injury2 = "doll/head_i2.png"
# injury3 = "doll/head_i3.png"
# scar1 = "doll/head_s1.png"
"##;

/// Create `skins/<name>/` with the commented starter skin.toml. Refuses to
/// overwrite an existing skin. Returns the manifest path.
pub fn write_scaffold(name: &str) -> anyhow::Result<PathBuf> {
    let name = name.trim();
    anyhow::ensure!(!name.is_empty(), "skin name is required");
    anyhow::ensure!(
        name.chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')),
        "skin names may only use letters, digits, '-' and '_'"
    );
    let root = crate::config::Config::skins_dir()?.join(name);
    let manifest_path = root.join("skin.toml");
    anyhow::ensure!(
        !manifest_path.exists(),
        "skin '{}' already exists at {}",
        name,
        manifest_path.display()
    );
    std::fs::create_dir_all(&root)?;
    crate::config::write_atomic(&manifest_path, SCAFFOLD_MANIFEST)?;
    Ok(manifest_path)
}

/// Marker comment identifying a skin.toml written by the harmony generator.
/// `write_harmony_skin` only overwrites manifests carrying it, so a
/// regenerate can never clobber a hand-authored skin under the same name.
pub const HARMONY_SKIN_MARKER: &str = "generated by .harmony";

/// Manifest for a harmony-generated skin: panel background on every window,
/// the plain frame as the default border, and both frames in the named-frame
/// pool so users can assign the accent variant per window (right-click >
/// Appearance > Skin frame).
pub fn harmony_skin_manifest(
    name: &str,
    scheme: &str,
    seed: &str,
    panel_top: &str,
    panel_bottom: &str,
    line: &str,
    accent: &str,
    slice: f64,
) -> String {
    format!(
        r##"# {name} - {marker}, matching a {scheme} palette.
# Colors derive from the same seed as the text presets:
#   seed {seed}   panel {panel_top} -> {panel_bottom}   line {line}   accent {accent}
# Activate with: .setskin {name}
# Regenerating via .harmony skin {name} (or the GUI Generate tab)
# overwrites this folder.

[meta]
name = "{name}"
description = "{scheme} harmony skin generated from seed {seed}"

[window.default.background]
image = "panel.png"
fit = "stretch"
opacity = 1.0
scrim = 0.0

[window.default.border]
image = "frame.png"
slice = [{slice:.1}, {slice:.1}, {slice:.1}, {slice:.1}]
scale = 1.0

# A stop darker; assign to the narrative window so it reads as the focus:
# [window.main.background]
# image = "panel-deep.png"
# fit = "stretch"

# Named frames, assignable per window from the GUI:
[frames.harmony]
image = "frame.png"
slice = [{slice:.1}, {slice:.1}, {slice:.1}, {slice:.1}]
scale = 1.0

[frames.harmony-accent]
image = "frame-accent.png"
slice = [{slice:.1}, {slice:.1}, {slice:.1}, {slice:.1}]
scale = 1.0
"##,
        marker = HARMONY_SKIN_MARKER,
    )
}

/// Write a harmony-generated skin: PNG-encode the rendered images and write
/// the manifest into `skins/<name>/`. Overwrites only skins that carry the
/// harmony marker; a hand-authored skin under the same name is refused.
/// `images` is `(file name, edge size, tightly packed RGBA8)`.
#[cfg(feature = "gui")]
pub fn write_harmony_skin(
    name: &str,
    manifest: &str,
    images: &[(&str, u32, Vec<u8>)],
) -> anyhow::Result<PathBuf> {
    let name = name.trim();
    anyhow::ensure!(!name.is_empty(), "skin name is required");
    anyhow::ensure!(
        name.chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')),
        "skin names may only use letters, digits, '-' and '_'"
    );
    let root = crate::config::Config::skins_dir()?.join(name);
    let manifest_path = root.join("skin.toml");
    if manifest_path.exists() {
        let existing = std::fs::read_to_string(&manifest_path).unwrap_or_default();
        anyhow::ensure!(
            existing.contains(HARMONY_SKIN_MARKER),
            "skin '{}' already exists and was not generated by .harmony - pick another name",
            name
        );
    }
    std::fs::create_dir_all(&root)?;
    for (file, size, rgba) in images {
        let img = image::RgbaImage::from_raw(*size, *size, rgba.clone())
            .ok_or_else(|| anyhow::anyhow!("bad image buffer for {}", file))?;
        img.save(root.join(file))
            .map_err(|err| anyhow::anyhow!("cannot write {}: {}", file, err))?;
    }
    crate::config::write_atomic(&manifest_path, manifest)?;
    Ok(manifest_path)
}

/// Read and parse `skins/<name>/skin.toml`. Returns the manifest and the
/// skin directory (for resolving relative image paths).
pub fn load_manifest(name: &str) -> anyhow::Result<(SkinManifest, PathBuf)> {
    let root = crate::config::Config::skins_dir()?.join(name);
    let manifest_path = root.join("skin.toml");
    let contents = std::fs::read_to_string(&manifest_path)
        .map_err(|err| anyhow::anyhow!("cannot read {}: {}", manifest_path.display(), err))?;
    let manifest: SkinManifest = toml::from_str(&contents)
        .map_err(|err| anyhow::anyhow!("invalid {}: {}", manifest_path.display(), err))?;
    Ok((manifest, root))
}

/// Load the shared hotbar icon sheets: `global/icons/icons.toml`, a bare
/// `[sheets]` table in the same format as skin.toml. Available to every
/// skin and with no skin active. Returns the sheets plus the shared
/// directory (manifest paths are relative to it); a missing manifest is
/// just "no shared sheets".
pub fn load_global_sheets() -> anyhow::Result<(HashMap<String, SheetSpec>, PathBuf)> {
    let root = crate::config::Config::global_icons_dir()?;
    let manifest_path = root.join("icons.toml");
    let contents = match std::fs::read_to_string(&manifest_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok((HashMap::new(), root));
        }
        Err(err) => {
            return Err(anyhow::anyhow!(
                "cannot read {}: {}",
                manifest_path.display(),
                err
            ));
        }
    };
    let manifest: SkinManifest = toml::from_str(&contents)
        .map_err(|err| anyhow::anyhow!("invalid {}: {}", manifest_path.display(), err))?;
    Ok((manifest.sheets, root))
}

/// Resolve a manifest image path to a filesystem path. Absolute paths are
/// taken as-is; relative paths resolve against the skin directory first,
/// then the shared image pool (`~/.vellum-fe/global/images/`) — so a skin
/// can reference pooled art ("icons/rogue.png", "frames/brass.png",
/// "dolls/human.png", ...) without copying it. When neither location has
/// the file, the skin-local path is returned so the caller's error names
/// the natural spot.
pub fn resolve_image_path(root: &Path, image: &str) -> PathBuf {
    let raw = Path::new(image);
    if raw.is_absolute() {
        return raw.to_path_buf();
    }
    let local = root.join(raw);
    if local.is_file() {
        return local;
    }
    if let Ok(pool) = crate::config::Config::global_images_dir() {
        let pooled = pool.join(raw);
        if pooled.is_file() {
            return pooled;
        }
    }
    local
}

/// Skin directory names that contain a skin.toml, sorted.
pub fn list_skins() -> Vec<String> {
    let Ok(dir) = crate::config::Config::skins_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut skins: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join("skin.toml").is_file())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    skins.sort();
    skins
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(toml_src: &str) -> SkinManifest {
        toml::from_str(toml_src).expect("manifest should parse")
    }

    #[test]
    fn harmony_manifest_parses_with_expected_wiring() {
        let text = harmony_skin_manifest(
            "dusk", "triadic", "#bf616a", "#333a44", "#20252c", "#4a525e", "#bf616a", 4.0,
        );
        assert!(text.contains(HARMONY_SKIN_MARKER), "overwrite marker present");
        let parsed = manifest(&text);
        assert_eq!(parsed.meta.name, "dusk");
        let bg = window_background(&parsed, "anything").expect("default background");
        assert_eq!(bg.image, "panel.png");
        assert_eq!(bg.fit, BackgroundFit::Stretch);
        let border = window_field(&parsed, "anything", |w| w.border.as_ref())
            .expect("default border");
        assert_eq!(border.image, "frame.png");
        assert_eq!(border.slice, [4.0, 4.0, 4.0, 4.0]);
        // Both frames land in the assignable pool.
        assert_eq!(named_frame(&parsed, "harmony").unwrap().image, "frame.png");
        assert_eq!(
            named_frame(&parsed, "harmony-accent").unwrap().image,
            "frame-accent.png"
        );
        // The panel-deep suggestion stays commented out: no main entry.
        assert!(window_background(&parsed, "main").is_some_and(|b| b.image == "panel.png"));
    }

    #[test]
    fn named_frames_parse_and_look_up_case_insensitively() {
        let manifest = manifest(
            r#"
            [frames.ornate]
            image = "borders/ornate.png"
            slice = [12.0, 12.0, 12.0, 12.0]
            scale = 0.5

            [frames.plain]
            image = "borders/plain.png"
            slice = [4.0, 4.0, 4.0, 4.0]
            "#,
        );
        assert_eq!(manifest.frames.len(), 2);
        assert_eq!(named_frame(&manifest, "ornate").unwrap().image, "borders/ornate.png");
        assert_eq!(named_frame(&manifest, "Ornate").unwrap().scale, 0.5);
        assert_eq!(named_frame(&manifest, "plain").unwrap().scale, 1.0);
        assert!(named_frame(&manifest, "missing").is_none());
    }

    #[test]
    fn named_frame_reserves_none() {
        let manifest = manifest(
            r#"
            [frames.none]
            image = "borders/never.png"
            slice = [4.0, 4.0, 4.0, 4.0]
            "#,
        );
        // "none" always means "no frame", even if a skin defines it.
        assert!(named_frame(&manifest, "none").is_none());
        assert!(named_frame(&manifest, "NONE").is_none());
    }

    #[test]
    fn manifest_parses_defaults_and_per_window_entries() {
        let manifest = manifest(
            r##"
            [meta]
            name = "Test"

            [window.default.background]
            image = "bg/paper.png"

            [window.main.background]
            image = "bg/vellum.png"
            fit = "tile"
            opacity = 0.5
            tint = "#ff8800"
            scrim = 0.25
            "##,
        );
        assert_eq!(manifest.meta.name, "Test");

        let default_bg = manifest.windows["default"].background.as_ref().unwrap();
        assert_eq!(default_bg.image, "bg/paper.png");
        assert_eq!(default_bg.fit, BackgroundFit::Cover);
        assert_eq!(default_bg.opacity, 1.0);
        assert_eq!(default_bg.scrim, 0.0);
        assert!(default_bg.tint.is_none());

        let main_bg = manifest.windows["main"].background.as_ref().unwrap();
        assert_eq!(main_bg.fit, BackgroundFit::Tile);
        assert_eq!(main_bg.opacity, 0.5);
        assert_eq!(main_bg.tint.as_deref(), Some("#ff8800"));
        assert_eq!(main_bg.scrim, 0.25);
    }

    #[test]
    fn window_lookup_falls_back_to_default() {
        let manifest = manifest(
            r#"
            [window.default.background]
            image = "default.png"

            [window.main.background]
            image = "main.png"
            "#,
        );
        assert_eq!(window_background(&manifest, "main").unwrap().image, "main.png");
        assert_eq!(window_background(&manifest, "Main").unwrap().image, "main.png");
        assert_eq!(
            window_background(&manifest, "thoughts").unwrap().image,
            "default.png"
        );
    }

    #[test]
    fn window_lookup_without_default_is_none() {
        let manifest = manifest(
            r#"
            [window.main.background]
            image = "main.png"
            "#,
        );
        assert!(window_background(&manifest, "thoughts").is_none());
    }

    #[test]
    fn manifest_parses_border_spec() {
        let manifest = manifest(
            r#"
            [window.default.border]
            image = "border/brass.png"
            slice = [8.0, 8.0, 8.0, 8.0]

            [window.main]
            background = { image = "main.png" }
            "#,
        );
        let border = manifest.windows["default"].border.as_ref().unwrap();
        assert_eq!(border.image, "border/brass.png");
        assert_eq!(border.slice, [8.0, 8.0, 8.0, 8.0]);
        assert_eq!(border.scale, 1.0);
        // Per-field fallback: main sets only a background, so its border
        // comes from default.
        assert_eq!(
            window_field(&manifest, "main", |w| w.border.as_ref())
                .unwrap()
                .image,
            "border/brass.png"
        );
    }

    #[test]
    fn manifest_parses_widget_art_sections() {
        let manifest = manifest(
            r#"
            [icons]
            kneeling = "icons/kneel.png"
            STUNNED = "icons/stunned.png"

            [compass]
            rose = "compass/rose.png"
            n = "compass/n.png"
            up = "compass/up.png"

            [injury_doll]
            base = "doll/base.png"

            [injury_doll.head]
            injury1 = "doll/head_i1.png"
            scar3 = "doll/head_s3.png"
            "#,
        );
        assert_eq!(manifest.icons["kneeling"], "icons/kneel.png");
        assert_eq!(manifest.icons["STUNNED"], "icons/stunned.png");
        assert_eq!(manifest.compass.rose.as_deref(), Some("compass/rose.png"));
        assert_eq!(manifest.compass.directions["n"], "compass/n.png");
        assert_eq!(manifest.compass.directions["up"], "compass/up.png");
        assert_eq!(manifest.injury_doll.base.as_deref(), Some("doll/base.png"));
        assert_eq!(manifest.injury_doll.parts["head"]["injury1"], "doll/head_i1.png");
        assert_eq!(manifest.injury_doll.parts["head"]["scar3"], "doll/head_s3.png");
    }

    #[test]
    fn manifest_parses_doll_anchors_and_dots() {
        let manifest = manifest(
            r##"
            [injury_doll]
            base = "doll/base.png"

            [injury_doll.anchors]
            head = [0.5, 0.09]
            leftArm = [0.31, 0.36]

            [injury_doll.dots]
            wound_color = "#aa0000"
            opacity = 0.5

            [injury_doll.nsys]
            injury1 = "doll/nerves_i1.png"
            "##,
        );
        assert_eq!(manifest.injury_doll.anchors["head"], [0.5, 0.09]);
        assert_eq!(manifest.injury_doll.anchors["leftArm"], [0.31, 0.36]);
        assert_eq!(manifest.injury_doll.dots.wound_color, "#aa0000");
        assert_eq!(manifest.injury_doll.dots.opacity, 0.5);
        // Unset dot fields keep their defaults.
        assert_eq!(manifest.injury_doll.dots.scar_color, "#b8b8b8");
        assert_eq!(manifest.injury_doll.dots.diameter, 0.07);
        // The flattened overlay tables still parse alongside the named
        // anchors/dots tables.
        assert_eq!(
            manifest.injury_doll.parts["nsys"]["injury1"],
            "doll/nerves_i1.png"
        );
        assert!(!manifest.injury_doll.parts.contains_key("anchors"));
        assert!(!manifest.injury_doll.parts.contains_key("dots"));
    }

    #[test]
    fn default_anchors_cover_every_part_within_unit_bounds() {
        assert_eq!(DOLL_PARTS.len(), 14);
        for (key, _, anchor) in DOLL_PARTS {
            let resolved = default_doll_anchor(key).unwrap();
            assert!(
                (0.0..=1.0).contains(&resolved[0]) && (0.0..=1.0).contains(&resolved[1]),
                "{key} anchor out of bounds"
            );
            assert_eq!(resolved, *anchor);
        }
        // Case-insensitive on the protocol key, None for unknown parts.
        assert!(default_doll_anchor("LEFTARM").is_some());
        assert!(default_doll_anchor("tail").is_none());
    }

    #[test]
    fn severity_levels_map_injuries_then_scars() {
        assert_eq!(severity_level_from_key("injury1"), Some(1));
        assert_eq!(severity_level_from_key("injury3"), Some(3));
        assert_eq!(severity_level_from_key("scar1"), Some(4));
        assert_eq!(severity_level_from_key("scar3"), Some(6));
        assert_eq!(severity_level_from_key("injury4"), None);
        assert_eq!(severity_level_from_key("base"), None);
    }

    #[test]
    fn scaffold_manifest_parses_and_is_inert() {
        // The starter file must parse and, being fully commented out,
        // define no graphics — activating a fresh scaffold changes nothing.
        let manifest: SkinManifest =
            toml::from_str(SCAFFOLD_MANIFEST).expect("scaffold should parse");
        assert_eq!(manifest.meta.name, "My Skin");
        assert!(manifest.windows.is_empty());
        assert!(manifest.icons.is_empty());
        assert!(manifest.compass.rose.is_none());
        assert!(manifest.injury_doll.base.is_none());
        assert!(manifest.injury_doll.anchors.is_empty());
    }

    #[test]
    fn write_scaffold_rejects_bad_names() {
        assert!(write_scaffold("").is_err());
        assert!(write_scaffold("   ").is_err());
        assert!(write_scaffold("no/slashes").is_err());
        assert!(write_scaffold("no spaces").is_err());
        assert!(write_scaffold("..").is_err());
    }
}
