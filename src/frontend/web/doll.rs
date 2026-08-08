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
    /// Protocol part keys with a `healthy` (level 0) overlay on disk,
    /// drawn while the part is uninjured. Kept separate from `overlays`
    /// so severity-dot logic never sees a level 0. A part listed in
    /// EITHER field is fully hand-drawn: a level with no art lets the
    /// base show through instead of falling back to a dot.
    pub healthy: Vec<String>,
    /// Condition-driven doll variants by name: each a complete
    /// replacement set. Which one is ACTIVE is not decided here — the
    /// host resolves conditions and pushes the active variant name on
    /// the state stream (`doll` message), so the client never evaluates
    /// conditions itself. `/doll/image` takes `&variant=<name>`.
    pub variants: BTreeMap<String, DollVariantPayload>,
}

/// One variant's set facts, mirroring the top-level default set fields.
#[derive(Debug, Clone, Serialize, Default)]
pub struct DollVariantPayload {
    pub anchors: BTreeMap<String, [f32; 2]>,
    pub dots: DotStylePayload,
    pub overlays: BTreeMap<String, Vec<u8>>,
    pub healthy: Vec<String>,
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

/// Resolve the payload for the active doll; `base: false` when nothing is
/// selected (no override, no skin doll) or anything fails to load.
pub fn active_payload() -> DollSkinPayload {
    let Some((doll, root)) = active_doll() else {
        return DollSkinPayload::default();
    };
    payload_from_doll(&doll, Some(&root))
}

/// Build the payload from a manifest's doll section. `root` (the skin
/// directory) enables on-disk existence checks for overlay art — a level
/// whose image is missing falls back to the dot, matching the GUI's
/// failed-texture behavior. Pass None in tests to skip the checks.
pub fn payload_from_doll(doll: &InjuryDollSkin, root: Option<&Path>) -> DollSkinPayload {
    if doll.base.is_none() {
        return DollSkinPayload::default();
    }

    let (anchors, dots, overlays, healthy) =
        set_facts(&doll.anchors, &doll.dots, &doll.parts, root);

    // Variants: same facts per named set. A variant without a base can't
    // render skinned, so it isn't offered.
    let mut variants = BTreeMap::new();
    for variant in &doll.variants {
        if variant.skin.base.is_none() {
            continue;
        }
        let (anchors, dots, overlays, healthy) = set_facts(
            &variant.skin.anchors,
            &variant.skin.dots,
            &variant.skin.parts,
            root,
        );
        variants.insert(
            variant.name.clone(),
            DollVariantPayload {
                anchors,
                dots,
                overlays,
                healthy,
            },
        );
    }

    DollSkinPayload {
        base: true,
        anchors,
        dots,
        overlays,
        healthy,
        variants,
    }
}

/// Resolve one doll set's client-facing facts: every part's anchor
/// (calibrated or default), clamped dot styling, and which part/level
/// combinations have overlay art on disk (level 0 split into `healthy`).
#[allow(clippy::type_complexity)]
fn set_facts(
    set_anchors: &std::collections::HashMap<String, [f32; 2]>,
    set_dots: &skins::DollDotSpec,
    set_parts: &std::collections::HashMap<String, skins::DollPartSpec>,
    root: Option<&Path>,
) -> (
    BTreeMap<String, [f32; 2]>,
    DotStylePayload,
    BTreeMap<String, Vec<u8>>,
    Vec<String>,
) {
    let mut anchors = BTreeMap::new();
    for (key, _, default) in DOLL_PARTS {
        let anchor = set_anchors
            .iter()
            .find(|(part, _)| part.eq_ignore_ascii_case(key))
            .map(|(_, [x, y])| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
            .unwrap_or(*default);
        anchors.insert((*key).to_string(), anchor);
    }

    let mut overlays: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut healthy: Vec<String> = Vec::new();
    for (part, spec) in set_parts {
        // Unknown part names can't be placed client-side; skip them the
        // same way the GUI's canonical part loop never asks for them.
        let Some((canonical, _, _)) = DOLL_PARTS
            .iter()
            .find(|(key, _, _)| key.eq_ignore_ascii_case(part))
        else {
            continue;
        };
        let mut present: Vec<u8> = spec
            .overlays
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
        // Level 0 (the `healthy` overlay) rides its own field so the
        // severity-dot path never sees it.
        if present.first() == Some(&0) {
            present.remove(0);
            healthy.push((*canonical).to_string());
        }
        if !present.is_empty() {
            overlays.insert((*canonical).to_string(), present);
        }
    }
    healthy.sort_unstable();

    let dots = DotStylePayload {
        wound_color: set_dots.wound_color.clone(),
        scar_color: set_dots.scar_color.clone(),
        opacity: set_dots.opacity.clamp(0.0, 1.0),
        diameter: set_dots.diameter.clamp(0.01, 0.5),
    };
    (anchors, dots, overlays, healthy)
}

/// On-disk path for a doll image request: `kind` is "base" or "overlay"
/// (the latter with part + level; level 0 = the healthy overlay).
/// `variant` picks a named `[[injury_doll.variants]]` set instead of the
/// default. None when no skin is active or the manifest has no such
/// image.
pub fn image_path(
    kind: &str,
    part: Option<&str>,
    level: Option<u8>,
    variant: Option<&str>,
) -> Option<PathBuf> {
    let (doll, root) = active_doll()?;
    // Borrow the requested set's fields: the default, or the named
    // variant's complete replacement set.
    let (set_base, set_parts) = match variant {
        None => (&doll.base, &doll.parts),
        Some(name) => {
            let found = doll.variants.iter().find(|v| v.name == name)?;
            (&found.skin.base, &found.skin.parts)
        }
    };
    let relative = match kind {
        "base" => set_base.clone()?,
        "overlay" => {
            let part = part?;
            let overlay_key = skins::severity_key_from_level(level?)?;
            set_parts
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(part))
                .and_then(|(_, spec)| spec.overlays.get(overlay_key))?
                .clone()
        }
        _ => return None,
    };
    Some(resolve_image(&root, &relative))
}

/// Manifest image path -> filesystem path (absolute paths allowed, plus
/// the shared image pool — same rule as the GUI texture loader).
fn resolve_image(root: &Path, image: &str) -> PathBuf {
    skins::resolve_image_path(root, image)
}

/// The doll the frontends should show: the config.doll_image override
/// (pool image + sidecar calibration, mirroring the GUI's resolution),
/// else the active skin's `[injury_doll]` section.
fn active_doll() -> Option<(InjuryDollSkin, PathBuf)> {
    let config = crate::config::Config::load().ok()?;
    if let Some(image) = config.doll_image {
        let root = crate::config::Config::global_images_dir().ok()?;
        let abs = skins::resolve_image_path(&root, &image);
        let sidecar: crate::config::pool::DollSidecar =
            crate::config::pool::read_sidecar(&abs).unwrap_or_default();
        let doll = InjuryDollSkin {
            base: Some(image),
            anchors: sidecar.anchors,
            dots: sidecar.dots,
            // Pool dolls carry no overlay art and no variants.
            variants: Default::default(),
            parts: Default::default(),
        };
        return Some((doll, root));
    }
    let name = config.active_skin?;
    let (manifest, root) = skins::load_manifest(&name).ok()?;
    Some((manifest.injury_doll, root))
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
        assert!(payload.healthy.is_empty());
    }

    #[test]
    fn healthy_levels_ride_their_own_field() {
        let payload = payload_from_doll(
            &doll(
                r#"
                [injury_doll]
                base = "doll/base.png"

                [injury_doll.leftArm]
                healthy = "doll/arm_ok.png"
                injury2 = "doll/arm_i2.png"

                [injury_doll.leftLeg]
                healthy = "doll/leg_ok.png"

                [injury_doll.head]
                injury1 = "doll/head_i1.png"
                "#,
            ),
            None,
        );
        // Level 0 never appears in overlays; it lists under `healthy`.
        assert_eq!(payload.healthy, vec!["leftArm", "leftLeg"]);
        assert_eq!(payload.overlays["leftArm"], vec![2]);
        // A healthy-only part contributes no overlays entry at all.
        assert!(!payload.overlays.contains_key("leftLeg"));
        // A part with no healthy art is untouched by the new field.
        assert_eq!(payload.overlays["head"], vec![1]);
    }

    #[test]
    fn variants_payload_lists_each_named_set() {
        let payload = payload_from_doll(
            &doll(
                r#"
                [injury_doll]
                base = "doll/standing.png"

                [injury_doll.head]
                injury1 = "doll/head_i1.png"

                [[injury_doll.variants]]
                name = "downed"
                when = { type = "indicator", id = "prone", active = true }
                [injury_doll.variants.skin]
                base = "doll/downed.png"
                [injury_doll.variants.skin.anchors]
                head = [0.2, 0.7]
                [injury_doll.variants.skin.leftArm]
                healthy = "doll/downed_arm_ok.png"

                [[injury_doll.variants]]
                name = "baseless"
                when = { type = "indicator", id = "dead", active = true }
                [injury_doll.variants.skin]
                # no base -> can't render skinned -> not offered
                "#,
            ),
            None,
        );
        // Default set facts unchanged by variants.
        assert_eq!(payload.overlays["head"], vec![1]);
        // The based variant is offered with its own complete facts...
        let downed = &payload.variants["downed"];
        assert_eq!(downed.anchors["head"], [0.2, 0.7]);
        assert_eq!(downed.healthy, vec!["leftArm"]);
        assert!(downed.overlays.is_empty());
        // ...anchors it didn't calibrate fall back to built-in defaults.
        assert_eq!(
            downed.anchors["chest"],
            skins::default_doll_anchor("chest").unwrap()
        );
        // A variant without a base is not offered.
        assert!(!payload.variants.contains_key("baseless"));
        assert_eq!(payload.variants.len(), 1);
    }

    #[test]
    fn payload_without_healthy_art_matches_previous_shape() {
        // Characterization: a skin authored before the healthy slot
        // existed produces the same overlays map and an empty healthy
        // list — the feature is invisible unless opted into.
        let payload = payload_from_doll(
            &doll(
                r#"
                [injury_doll]
                base = "doll/base.png"

                [injury_doll.head]
                injury1 = "doll/head_i1.png"
                scar3 = "doll/head_s3.png"
                "#,
            ),
            None,
        );
        assert!(payload.healthy.is_empty());
        assert_eq!(payload.overlays["head"], vec![1, 6]);
        assert_eq!(payload.overlays.len(), 1);
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
                healthy = "missing_ok.png"
                injury1 = "real.png"
                injury2 = "missing.png"
                "#,
            ),
            Some(dir.path()),
        );
        assert_eq!(payload.overlays["head"], vec![1]);
        // A healthy image missing from disk drops out the same way.
        assert!(payload.healthy.is_empty());
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
