//! Injury doll skin data for the phone client.
//!
//! The GUI renders skinned dolls straight from the manifest and loaded
//! textures; the browser needs the same facts over HTTP instead: whether
//! the active skin ships doll base art, the resolved anchor point for
//! every body part, the generated-dot styling, and which part/severity
//! combinations have hand-drawn overlay art (served as images and stacked
//! on the base exactly like the GUI does). Everything resolves through
//! `crate::config::skins`, which compiles without egui — this path is
//! what the mobile builds use.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::skins::{self, InjuryDollSkin, DOLL_PARTS};

/// JSON payload for `/doll.json`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct DollSkinPayload {
    /// True when a skin with doll base art is active. When false the
    /// client keeps its vector doll and the other fields are empty.
    pub base: bool,
    /// Protocol part key -> [x, y] fractions of the base image. All 14
    /// parts present, calibrated or built-in default.
    pub anchors: BTreeMap<String, [f32; 2]>,
    pub dots: DotStylePayload,
    /// Protocol part key -> severity levels (1-6) with overlay art on
    /// disk; those levels render the overlay instead of a dot.
    pub overlays: BTreeMap<String, Vec<u8>>,
}

/// Generated-dot styling, mirroring `[injury_doll.dots]`.
#[derive(Debug, Clone, Serialize)]
pub struct DotStylePayload {
    pub wound_color: String,
    pub scar_color: String,
    pub opacity: f32,
    pub diameter: f32,
}

impl Default for DotStylePayload {
    fn default() -> Self {
        let spec = skins::DollDotSpec::default();
        Self {
            wound_color: spec.wound_color,
            scar_color: spec.scar_color,
            opacity: spec.opacity,
            diameter: spec.diameter,
        }
    }
}

/// Resolve the payload for the active skin; `base: false` when no skin is
/// active, it has no doll art, or anything fails to load.
pub fn active_payload() -> DollSkinPayload {
    let Some((manifest, root)) = load_active_manifest() else {
        return DollSkinPayload::default();
    };
    payload_from_doll(&manifest.injury_doll, Some(&root))
}

/// Build the payload from a manifest's doll section. `root` (the skin
/// directory) enables on-disk existence checks for overlay art — a level
/// whose image is missing falls back to the dot, matching the GUI's
/// failed-texture behavior. Pass None in tests to skip the checks.
pub fn payload_from_doll(doll: &InjuryDollSkin, root: Option<&Path>) -> DollSkinPayload {
    if doll.base.is_none() {
        return DollSkinPayload::default();
    }

    let mut anchors = BTreeMap::new();
    for (key, _, default) in DOLL_PARTS {
        let anchor = doll
            .anchors
            .iter()
            .find(|(part, _)| part.eq_ignore_ascii_case(key))
            .map(|(_, [x, y])| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
            .unwrap_or(*default);
        anchors.insert((*key).to_string(), anchor);
    }

    let mut overlays: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (part, levels) in &doll.parts {
        // Unknown part names can't be placed client-side; skip them the
        // same way the GUI's canonical part loop never asks for them.
        let Some((canonical, _, _)) = DOLL_PARTS
            .iter()
            .find(|(key, _, _)| key.eq_ignore_ascii_case(part))
        else {
            continue;
        };
        let mut present: Vec<u8> = levels
            .iter()
            .filter_map(|(key, image)| {
                let level = skins::severity_level_from_key(key)?;
                match root {
                    Some(root) => resolve_image(root, image).exists().then_some(level),
                    None => Some(level),
                }
            })
            .collect();
        present.sort_unstable();
        if !present.is_empty() {
            overlays.insert((*canonical).to_string(), present);
        }
    }

    DollSkinPayload {
        base: true,
        anchors,
        dots: DotStylePayload {
            wound_color: doll.dots.wound_color.clone(),
            scar_color: doll.dots.scar_color.clone(),
            opacity: doll.dots.opacity.clamp(0.0, 1.0),
            diameter: doll.dots.diameter.clamp(0.01, 0.5),
        },
        overlays,
    }
}

/// On-disk path for a doll image request: `kind` is "base" or "overlay"
/// (the latter with part + level). None when no skin is active or the
/// manifest has no such image.
pub fn image_path(kind: &str, part: Option<&str>, level: Option<u8>) -> Option<PathBuf> {
    let (manifest, root) = load_active_manifest()?;
    let doll = &manifest.injury_doll;
    let relative = match kind {
        "base" => doll.base.clone()?,
        "overlay" => {
            let part = part?;
            let overlay_key = skins::severity_key_from_level(level?)?;
            doll.parts
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(part))
                .and_then(|(_, levels)| levels.get(overlay_key))?
                .clone()
        }
        _ => return None,
    };
    Some(resolve_image(&root, &relative))
}

/// Manifest image path -> filesystem path (absolute paths allowed, same
/// rule as the GUI texture loader).
fn resolve_image(root: &Path, image: &str) -> PathBuf {
    let raw = Path::new(image);
    if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        root.join(raw)
    }
}

fn load_active_manifest() -> Option<(skins::SkinManifest, PathBuf)> {
    let config = crate::config::Config::load().ok()?;
    let name = config.active_skin?;
    skins::load_manifest(&name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doll(toml_src: &str) -> InjuryDollSkin {
        let manifest: skins::SkinManifest =
            toml::from_str(toml_src).expect("manifest should parse");
        manifest.injury_doll
    }

    #[test]
    fn payload_without_base_is_inactive() {
        let payload = payload_from_doll(&doll(""), None);
        assert!(!payload.base);
        assert!(payload.anchors.is_empty());
        assert!(payload.overlays.is_empty());
    }

    #[test]
    fn payload_resolves_all_parts_with_calibration_overriding_defaults() {
        let payload = payload_from_doll(
            &doll(
                r#"
                [injury_doll]
                base = "doll/base.png"

                [injury_doll.anchors]
                head = [0.4, 0.2]
                leftarm = [0.3, 0.4]
                "#,
            ),
            None,
        );
        assert!(payload.base);
        assert_eq!(payload.anchors.len(), DOLL_PARTS.len());
        assert_eq!(payload.anchors["head"], [0.4, 0.2]);
        // Lowercase manifest key resolves onto the canonical protocol key.
        assert_eq!(payload.anchors["leftArm"], [0.3, 0.4]);
        // Uncalibrated part falls back to the built-in default.
        assert_eq!(
            payload.anchors["chest"],
            skins::default_doll_anchor("chest").unwrap()
        );
    }

    #[test]
    fn payload_lists_overlay_levels_under_canonical_keys() {
        let payload = payload_from_doll(
            &doll(
                r#"
                [injury_doll]
                base = "doll/base.png"

                [injury_doll.NSYS]
                injury1 = "doll/nerves1.png"
                scar2 = "doll/nerves_s2.png"
                bogus = "doll/junk.png"

                [injury_doll.tail]
                injury1 = "doll/tail.png"
                "#,
            ),
            None,
        );
        // Case-normalized to the protocol key; bogus severity keys and
        // unknown parts are dropped.
        assert_eq!(payload.overlays["nsys"], vec![1, 5]);
        assert!(!payload.overlays.contains_key("tail"));
        assert_eq!(payload.overlays.len(), 1);
    }

    #[test]
    fn overlay_existence_check_drops_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("real.png"), b"png").unwrap();
        let payload = payload_from_doll(
            &doll(
                r#"
                [injury_doll]
                base = "base.png"

                [injury_doll.head]
                injury1 = "real.png"
                injury2 = "missing.png"
                "#,
            ),
            Some(dir.path()),
        );
        assert_eq!(payload.overlays["head"], vec![1]);
    }

    #[test]
    fn dot_style_carries_manifest_values_clamped() {
        let payload = payload_from_doll(
            &doll(
                r##"
                [injury_doll]
                base = "doll/base.png"

                [injury_doll.dots]
                wound_color = "#aa0000"
                opacity = 7.0
                diameter = 0.9
                "##,
            ),
            None,
        );
        assert_eq!(payload.dots.wound_color, "#aa0000");
        assert_eq!(payload.dots.opacity, 1.0);
        assert_eq!(payload.dots.diameter, 0.5);
        // Unset fields keep defaults.
        assert_eq!(payload.dots.scar_color, "#b8b8b8");
    }
}
