//! The ONE creature geometry contract (creature-field proposal, finding 6).
//!
//! Everything that needs a creature's physical size — the solver's
//! standing/prone reservation boxes, the renderer's sprite scale, the
//! click rectangle, shadows and overlays — derives from a single
//! frontend-independent [`CreatureGeometry`], resolved here from:
//!
//!   1. bestiary reference height (feet; 6 ft ≡ the 1.2-unit card) or the
//!      size-bucket fallback, with the boss emphasis and the 0.55–2.6
//!      readability clamp recorded as explicit POLICY flags, and
//!   2. per-pose image calibration ([`ArtCalibration`]): the sidecar's
//!      absolute `size` override, the art's content aspect (alpha bounds —
//!      transparent padding is neutral by construction), and the contact
//!      span from the calibrated footprint.
//!
//! POLICY (owner-pinned): an authored sidecar `size` is an ABSOLUTE world
//! height with its existing visual meaning — it is never reinterpreted as
//! a multiplier, and the solver now agrees with the renderer about it.
//! Shared family/default art contributes only SHAPE (aspect, span): its
//! calibration never sets a species' height, so creatures sharing fallback
//! art keep their own bestiary heights unless the art explicitly authors
//! `size` (which the renderer has always honored absolutely).
//!
//! Calibration metadata is reachable from core with no frontend imports
//! and no image decoding or filesystem reads in the game loop: the
//! frontend feeds the [`CalibrationStore`] from its art prepare pass
//! (where it reads the sidecar TOML and decodes the art anyway); core
//! only reads it, with the historical boxes as the deterministic
//! fallback for tokens without an entry. Revision handling:
//! `revision()` bumps on any change; `sync_field` re-derives placed units'
//! boxes through `CreatureField::recalibrate`, which never moves anyone.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::solver::CardSize;

/// Calibration of one pose image, in art-content terms. Every field is
/// optional: absent means "use the deterministic fallback".
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PoseCalibration {
    /// Authored ABSOLUTE world height of the image's content (sidecar
    /// `size`). Overrides the bestiary height outright.
    pub size: Option<f32>,
    /// Content width/height (alpha bounds — transparent padding excluded,
    /// so padding and art resolution are both neutral).
    pub aspect: Option<f32>,
    /// Ground-contact span as a fraction of the content width (from the
    /// calibrated footprint ellipse).
    pub span: Option<f32>,
    /// Measured pose-to-standing CONTENT ratio: this pose image's content
    /// pixel height ÷ the standing image's content pixel height, under the
    /// art set's common canvas-scale convention (Niffy's rule: every pose
    /// in a set is drawn at one pixel scale). This is exactly the factor
    /// the renderer's inherited pixel scale produces, so bounds derived
    /// from it match the drawn pose to the pixel. A pose file authored at
    /// a genuinely different scale must use `size` — the explicit
    /// calibration path — because resolution can never imply thickness.
    /// `None` for the standing pose itself and when the pose art has not
    /// been measured.
    pub content_ratio: Option<f32>,
}

/// Calibration of one creature's resolved art set (the locked tier).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArtCalibration {
    pub standing: PoseCalibration,
    /// The tier's prone pose image, when it ships one.
    pub prone: Option<PoseCalibration>,
}

/// Which source decided the effective world height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeightSource {
    /// Sidecar `size` on the resolved art: absolute, wins outright.
    SidecarSize,
    /// Bestiary `height` (feet × 0.2).
    BestiaryHeight,
    /// Bestiary size bucket (tiny/small/medium/large/huge).
    BestiarySizeBucket,
    /// No data: the default 1.2-unit card.
    Default,
}

/// The shared geometry description: one resolution, consumed by
/// placement, drawing, targeting, and Studio alike (each derives its own
/// bounds from it — they may differ deliberately, but always agree on
/// world scale).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreatureGeometry {
    /// Effective standing world height (the sprite pixel-scale anchor).
    pub world_h: f32,
    /// Which source won.
    pub source: HeightSource,
    /// Readability POLICY records — presentation choices, not measurements:
    /// the boss emphasis bump was applied / the 0.55–2.6 clamp engaged.
    /// Never applied on top of an absolute sidecar override.
    pub boss_emphasis: bool,
    pub readability_clamped: bool,
    /// The solver's reservation boxes (standing + prone), derived from
    /// `world_h` and the art's calibrated shape.
    pub standing: CardSize,
    pub prone: CardSize,
}

/// The readability clamp, recorded (not hidden) policy.
pub const READABILITY_CLAMP: (f32, f32) = (0.55, 2.6);

/// Resolve one creature's shared geometry from bestiary identity plus
/// (optional) art calibration. Pure and deterministic: no IO, callable
/// from any layer, with the no-calibration path reproducing the field's
/// historical boxes exactly (the deterministic fallback when art is
/// unavailable).
pub fn resolve_geometry(
    c: &crate::core::state::Creature,
    cal: Option<&ArtCalibration>,
) -> CreatureGeometry {
    let boss = c.flags.as_ref().is_some_and(|f| f.is_boss());
    let bestiary = super::bestiary_height_units(&c.name, c.noun.as_deref());
    let quad = super::bestiary_body_type(&c.name, c.noun.as_deref())
        .is_some_and(|t| t == "quadruped");

    // ---- reference height + readability policy --------------------------
    let (mut world_h, source, mut boss_emphasis) = match bestiary {
        Some((h, from_height)) => (
            if boss { (h * 1.15).max(1.52) } else { h },
            if from_height {
                HeightSource::BestiaryHeight
            } else {
                HeightSource::BestiarySizeBucket
            },
            boss,
        ),
        None => (
            if boss { 1.52 } else { CardSize::default().h },
            HeightSource::Default,
            boss,
        ),
    };
    let unclamped = world_h;
    world_h = world_h.clamp(READABILITY_CLAMP.0, READABILITY_CLAMP.1);
    let mut readability_clamped = world_h != unclamped;
    let mut source = source;

    // ---- absolute art override ------------------------------------------
    // Sidecar `size` keeps its existing visual meaning: the rendered world
    // height, exactly as authored — no boss bump, no clamp. The solver's
    // boxes follow it so reservation, hit rect, and drawn sprite agree.
    let standing_cal = cal.map(|c| c.standing).unwrap_or_default();
    if let Some(s) = standing_cal.size.filter(|s| s.is_finite() && *s > 0.0) {
        world_h = s;
        source = HeightSource::SidecarSize;
        boss_emphasis = false;
        readability_clamped = false;
    }

    // ---- boxes from shape calibration -----------------------------------
    // Standing width from the art's content aspect when calibrated (wide
    // quadruped art reserves the room it draws in), the historical 0.5 × h
    // biped guess otherwise. Span from the calibrated footprint, the 0.65
    // default otherwise.
    let standing_w = standing_cal
        .aspect
        .filter(|a| a.is_finite() && *a > 0.0)
        .map(|a| (world_h * a).max(0.1))
        .unwrap_or(world_h * 0.5);
    let standing = CardSize {
        w: standing_w,
        h: world_h,
        span: sanitize_span(standing_cal.span),
    };

    // Prone height, in renderer precedence (finding 7 — bounds derive
    // from the SAME resolved pose scale the sprite draws at):
    //   1. the pose's own authored `size`: absolute, exactly as drawn —
    //      the explicit calibration path for pose files at a different
    //      canvas scale; no minimum inflates it (a minimum HIT target is
    //      an input policy, never part of visible bounds);
    //   2. the measured pose-to-standing content ratio × the shared world
    //      height — precisely the height the renderer's inherited pixel
    //      scale draws the pose at (a prone image with half the standing
    //      content height renders, reserves, and hit-tests at 50%);
    //   3. the body-type fraction guess, floored at 0.30 — the
    //      deterministic fallback for unmeasured art, where the floor is
    //      part of the guess (kept for historical box compatibility), not
    //      a policy applied to real measurements.
    let prone_cal = cal.and_then(|c| c.prone).unwrap_or_default();
    let prone_h = if let Some(s) = prone_cal.size.filter(|s| s.is_finite() && *s > 0.0) {
        s
    } else if let Some(r) = prone_cal
        .content_ratio
        .filter(|r| r.is_finite() && *r > 0.0)
    {
        world_h * r
    } else {
        (if quad { world_h * 0.70 } else { world_h * 0.35 }).max(0.30)
    };
    let prone_w = prone_cal
        .aspect
        .filter(|a| a.is_finite() && *a > 0.0)
        .map(|a| (prone_h * a).max(0.1))
        .unwrap_or((world_h * 0.90).max(0.35));
    let prone = CardSize {
        w: prone_w,
        h: prone_h,
        span: sanitize_span(prone_cal.span),
    };

    CreatureGeometry {
        world_h,
        source,
        boss_emphasis,
        readability_clamped,
        standing,
        prone,
    }
}

fn sanitize_span(span: Option<f32>) -> f32 {
    span.filter(|s| s.is_finite())
        .map(|s| s.clamp(0.2, 1.0))
        .unwrap_or(0.65)
}

/// Process-wide calibration metadata, keyed by the creature's NAME token
/// (the same boon-stripped slug the art tiers key on). The frontend fills
/// it when it resolves/decodes art anyway (sidecar TOML + alpha bounds,
/// in its prepare pass — never in the game's placement loops); core only
/// reads it, keeping the deterministic fallback for tokens with no entry
/// (headless/TUI, art still loading, no art at all). Every actual change
/// bumps the revision; `sync_field` watches it and recalibrates placed
/// units in place.
#[derive(Default)]
pub struct CalibrationStore {
    entries: HashMap<String, Option<ArtCalibration>>,
    revision: u64,
}

impl CalibrationStore {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Calibration for a token; `None` = not (yet) known: deterministic
    /// fallback geometry.
    pub fn resolve(&self, token: &str) -> Option<ArtCalibration> {
        self.entries.get(token).copied().flatten()
    }

    /// Record calibration (the frontend's sidecar + alpha-derived content
    /// data) for a token. Bumps the revision only when the stored value
    /// actually changes, so settled frames cost a lookup.
    pub fn refine(&mut self, token: &str, cal: ArtCalibration) {
        let slot = self.entries.entry(token.to_string()).or_default();
        if *slot != Some(cal) {
            *slot = Some(cal);
            self.revision += 1;
        }
    }

    /// Drop everything (skin/sidecar reload): the frontend re-feeds on
    /// its next prepare pass and placed units re-derive their boxes.
    pub fn invalidate(&mut self) {
        if !self.entries.is_empty() {
            self.entries.clear();
        }
        self.revision += 1;
    }
}

/// The shared store. A process global (like the bestiary) because the
/// game client and Studio each own AppCores while art loading happens in
/// the frontend: core reads, the frontend refines, nobody imports across
/// the boundary.
pub fn calibrations() -> &'static Mutex<CalibrationStore> {
    static STORE: OnceLock<Mutex<CalibrationStore>> = OnceLock::new();
    STORE.get_or_init(Default::default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::{Creature, CreatureFlags};

    fn creature(name: &str, noun: &str) -> Creature {
        Creature {
            name: name.to_string(),
            noun: (!noun.is_empty()).then(|| noun.to_string()),
            id: "1".into(),
            status: None,
            flags: None,
        }
    }

    fn boss(name: &str, noun: &str) -> Creature {
        Creature {
            flags: Some(CreatureFlags {
                mini_boss: true,
                hostile: true,
                ..Default::default()
            }),
            ..creature(name, noun)
        }
    }

    /// Precedence against the owner's real creature data (agresh_bear.rb:
    /// height 4 ft, size large, Quadruped): bestiary height wins over the
    /// size bucket, the body type shapes the prone box, and the fallback
    /// boxes match the field's historical derivation exactly.
    #[test]
    fn bestiary_height_and_body_type_resolve_for_agresh_bear() {
        let g = resolve_geometry(&creature("agresh bear", "bear"), None);
        assert_eq!(g.source, HeightSource::BestiaryHeight);
        assert!((g.world_h - 0.8).abs() < 1e-3, "4 ft -> 0.8 units");
        assert!(!g.boss_emphasis && !g.readability_clamped);
        // Deterministic fallback shape = the historical boxes.
        assert!((g.standing.w - 0.4).abs() < 1e-3);
        assert_eq!(g.standing.span, 0.65);
        // Quadruped prone keeps 0.70 of its height.
        assert!((g.prone.h - 0.8 * 0.70).abs() < 1e-3, "prone h {}", g.prone.h);
        assert!((g.prone.w - (0.8f32 * 0.90).max(0.35)).abs() < 1e-3);
    }

    /// A sidecar `size` is an ABSOLUTE override: it moves the standing
    /// reservation (and thus hit/envelope geometry) to exactly the
    /// rendered height, bypassing boss emphasis and the readability clamp
    /// — the renderer's existing visual meaning, now shared.
    #[test]
    fn sidecar_size_overrides_reservation_absolutely() {
        let cal = ArtCalibration {
            standing: PoseCalibration {
                size: Some(3.4), // beyond the 2.6 readability clamp
                aspect: None,
                span: None,
                content_ratio: None,
            },
            prone: None,
        };
        let g = resolve_geometry(&boss("agresh bear", "bear"), Some(&cal));
        assert_eq!(g.source, HeightSource::SidecarSize);
        assert_eq!(g.world_h, 3.4);
        assert_eq!(g.standing.h, 3.4);
        assert!(!g.boss_emphasis && !g.readability_clamped);
        // The prone envelope follows the shared height too.
        assert!((g.prone.h - 3.4 * 0.70).abs() < 1e-3);
        // Garbage overrides are ignored, not honored.
        for bad in [0.0, -1.0, f32::NAN] {
            let cal = ArtCalibration {
                standing: PoseCalibration {
                    size: Some(bad),
                    ..Default::default()
                },
                prone: None,
            };
            let g = resolve_geometry(&creature("agresh bear", "bear"), Some(&cal));
            assert_eq!(g.source, HeightSource::BestiaryHeight);
        }
    }

    /// Boss emphasis and the min/max clamp are recorded as policy, not
    /// hidden in the number.
    #[test]
    fn readability_policy_is_recorded() {
        // Boss bump on a real height (agresh bear 0.8 -> max(0.92, 1.52)).
        let g = resolve_geometry(&boss("agresh bear", "bear"), None);
        assert!(g.boss_emphasis);
        assert!((g.world_h - 1.52).abs() < 1e-3);
        // Agresh troll chieftain: 9 ft -> 1.8; boss bump 2.07, no clamp.
        let g = resolve_geometry(&boss("agresh troll chieftain", "chieftain"), None);
        assert!((g.world_h - 2.07).abs() < 1e-3);
        assert!(!g.readability_clamped);
        // A giant rat (1 ft -> 0.2) clamps up to 0.55 and says so.
        let g = resolve_geometry(&creature("giant rat", "rat"), None);
        assert_eq!(g.world_h, READABILITY_CLAMP.0);
        assert!(g.readability_clamped);
        // Unknown creature: the default card, unclamped.
        let g = resolve_geometry(&creature("test dummy", "dummy"), None);
        assert_eq!(g.source, HeightSource::Default);
        assert!((g.world_h - 1.2).abs() < 1e-3);
    }

    /// Wide quadruped art: content aspect widens the reservation so the
    /// solver reserves the room the renderer draws in; the footprint span
    /// rides along. Prone art keeps its own proportions — lying down never
    /// stretches body thickness to standing height.
    #[test]
    fn content_aspect_and_span_shape_the_boxes() {
        let cal = ArtCalibration {
            standing: PoseCalibration {
                size: None,
                aspect: Some(2.0),
                span: Some(0.8),
                content_ratio: None,
            },
            prone: Some(PoseCalibration {
                size: None,
                aspect: Some(3.0),
                span: None,
                content_ratio: None,
            }),
        };
        let g = resolve_geometry(&creature("agresh bear", "bear"), Some(&cal));
        assert!((g.standing.w - 1.6).abs() < 1e-3, "0.8h x 2.0 aspect");
        assert_eq!(g.standing.span, 0.8);
        // Prone: quadruped height share, width from the pose's own aspect.
        assert!((g.prone.h - 0.56).abs() < 1e-3);
        assert!((g.prone.w - 0.56 * 3.0).abs() < 1e-3);
        assert!(g.prone.h < g.standing.h, "no thickness stretch");
        assert_eq!(g.prone.span, 0.65);
    }

    /// Finding 7's worked example: standing content 200px, prone content
    /// 100px in one art set (common canvas scale) → the measured ratio is
    /// 0.5, and the prone bounds resolve to HALF the standing height —
    /// exactly what the renderer's inherited pixel scale draws — not the
    /// 35% biped guess. Same for quadrupeds (not 70%), and the pose keeps
    /// its own drawn width via its aspect.
    #[test]
    fn measured_pose_ratio_drives_prone_bounds() {
        for (name, noun) in [("mongrel kobold", "kobold"), ("agresh bear", "bear")] {
            let cal = ArtCalibration {
                standing: PoseCalibration {
                    aspect: Some(0.5),
                    ..Default::default()
                },
                prone: Some(PoseCalibration {
                    aspect: Some(2.4),
                    content_ratio: Some(100.0 / 200.0),
                    ..Default::default()
                }),
            };
            let g = resolve_geometry(&creature(name, noun), Some(&cal));
            assert!(
                (g.prone.h - g.standing.h * 0.5).abs() < 1e-4,
                "{name}: prone {} vs standing {}",
                g.prone.h,
                g.standing.h
            );
            // Visible width follows the pose's own drawn proportions.
            assert!((g.prone.w - g.prone.h * 2.4).abs() < 1e-3);
        }
    }

    /// The measured ratio is exact — no hidden minimum inflates real
    /// measurements (minimum hit-target policy is separate from visible
    /// bounds), while the unmeasured fallback keeps its historical 0.30
    /// floor as part of the guess. An authored prone `size` (the explicit
    /// calibration path for independently scaled pose files — resolution
    /// never implies thickness) beats the ratio, exact too.
    #[test]
    fn pose_size_beats_ratio_and_no_minimum_on_measurements() {
        // giant rat: clamps up to 0.55 standing; a 0.4 measured ratio gives
        // 0.22 prone — BELOW the fallback's 0.30 floor, kept exact.
        let cal = ArtCalibration {
            standing: PoseCalibration::default(),
            prone: Some(PoseCalibration {
                content_ratio: Some(0.4),
                ..Default::default()
            }),
        };
        let g = resolve_geometry(&creature("giant rat", "rat"), Some(&cal));
        assert!((g.prone.h - 0.55 * 0.4).abs() < 1e-4, "exact, no 0.30 floor");
        // A prone image at a different resolution with authored size: the
        // absolute size wins over the (meaningless) pixel ratio, exact.
        let cal = ArtCalibration {
            standing: PoseCalibration::default(),
            prone: Some(PoseCalibration {
                size: Some(0.25),
                content_ratio: Some(2.0), // hi-res pose file: bogus ratio
                ..Default::default()
            }),
        };
        let g = resolve_geometry(&creature("agresh bear", "bear"), Some(&cal));
        assert_eq!(g.prone.h, 0.25, "authored pose size is absolute");
        // Garbage ratios fall through to the body-type guess (+floor).
        for bad in [0.0, -1.0, f32::NAN] {
            let cal = ArtCalibration {
                standing: PoseCalibration::default(),
                prone: Some(PoseCalibration {
                    content_ratio: Some(bad),
                    ..Default::default()
                }),
            };
            let g = resolve_geometry(&creature("agresh bear", "bear"), Some(&cal));
            assert!((g.prone.h - 0.8 * 0.70).abs() < 1e-3);
        }
    }

    /// A sidecar `size` on the STANDING pose scales the whole envelope,
    /// and a measured prone ratio rides on that shared height — override
    /// and ratio compose instead of fighting.
    #[test]
    fn ratio_composes_with_standing_override() {
        let cal = ArtCalibration {
            standing: PoseCalibration {
                size: Some(2.0),
                ..Default::default()
            },
            prone: Some(PoseCalibration {
                content_ratio: Some(0.5),
                ..Default::default()
            }),
        };
        let g = resolve_geometry(&creature("agresh bear", "bear"), Some(&cal));
        assert_eq!(g.world_h, 2.0);
        assert!((g.prone.h - 1.0).abs() < 1e-4);
        // Both pose boxes sit inside the standing/prone envelope union by
        // construction (the solver reserves max of the two per axis).
        assert!(g.prone.h <= g.standing.h.max(g.prone.h));
        assert!(g.standing.w <= g.standing.w.max(g.prone.w));
    }

    /// Shared fallback art must not force every species to one height:
    /// shape-only calibration (aspect/span, no `size`) leaves each
    /// species' bestiary height intact.
    #[test]
    fn shared_art_shape_never_sets_species_height() {
        let cal = ArtCalibration {
            standing: PoseCalibration {
                size: None,
                aspect: Some(1.4),
                span: Some(0.7),
                content_ratio: None,
            },
            prone: None,
        };
        let bear = resolve_geometry(&creature("agresh bear", "bear"), Some(&cal));
        let kobold = resolve_geometry(&creature("mongrel kobold", "kobold"), Some(&cal));
        assert!((bear.world_h - 0.8).abs() < 1e-3);
        assert!((kobold.world_h - 0.6).abs() < 1e-3, "3 ft kobold");
        assert_ne!(bear.world_h, kobold.world_h);
        // Same shape, each at its own scale.
        assert!((bear.standing.w / bear.world_h - 1.4).abs() < 1e-3);
        assert!((kobold.standing.w / kobold.world_h - 1.4).abs() < 1e-3);
    }

    /// Revision handling: refine bumps only on change; invalidate always
    /// bumps and forces re-seeding.
    #[test]
    fn store_revision_semantics() {
        let mut store = CalibrationStore::default();
        let r0 = store.revision();
        let cal = ArtCalibration {
            standing: PoseCalibration {
                size: Some(1.5),
                aspect: Some(1.0),
                span: None,
                content_ratio: None,
            },
            prone: None,
        };
        store.refine("geometry_test_token", cal);
        assert_eq!(store.revision(), r0 + 1);
        store.refine("geometry_test_token", cal);
        assert_eq!(store.revision(), r0 + 1, "no-change refine must not bump");
        assert_eq!(store.resolve("geometry_test_token"), Some(cal));
        assert_eq!(store.resolve("unseen_token"), None, "miss = fallback");
        store.invalidate();
        assert_eq!(store.revision(), r0 + 2);
    }
}
