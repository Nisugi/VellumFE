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
}

impl PoolImage {
    /// File stem ("dwarf_ranger"), the display name in pickers.
    pub fn stem(&self) -> &str {
        self.file_name
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(&self.file_name)
    }

    /// Sidecar path: `<stem>.toml` beside the image (the same convention the
    /// vellum-assets generator reads for gallery/render metadata).
    pub fn sidecar_path(&self) -> PathBuf {
        self.abs_path.with_extension("toml")
    }
}

/// Images in one pool category, sorted by file name. A missing category
/// folder is just an empty list.
pub fn list_category(category: &str) -> Vec<PoolImage> {
    let Ok(dir) = Config::global_image_category_dir(category) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut images: Vec<PoolImage> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            if !IMAGE_EXTS.contains(&ext.as_str()) {
                return None;
            }
            let file_name = path.file_name()?.to_str()?.to_owned();
            Some(PoolImage {
                pool_path: format!("{}/{}", category, file_name),
                abs_path: path,
                file_name,
            })
        })
        .collect();
    images.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    images
}

/// Distinct `<set>_` prefixes in a category, sorted — how compass and
/// statusicon art groups into sets ("runic_stunned.png" → set "runic").
/// Files without an underscore don't form a set.
pub fn set_names(category: &str) -> Vec<String> {
    let mut names: Vec<String> = list_category(category)
        .iter()
        .filter_map(|image| image.stem().split_once('_').map(|(set, _)| set.to_owned()))
        .collect();
    names.sort();
    names.dedup();
    names
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
        .map_err(|err| anyhow::anyhow!("cannot write {}: {}", path.display(), err))
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
}
