//! Creature cards: sprite-based creature display (the `creaturefield`
//! widget). This module grows phase by phase; P0 is the vocabulary layer.
//!
//! Settled decisions (see the creature-cards plan):
//! - Noun/family for art resolution come from Vellum's own room-objs parse,
//!   never from Lich/CreatureBar's Ruby side.
//! - Creatures take wounds only (injury1-3 + healthy) — no scars. The doll
//!   loader's scar states are optional-and-absent on the creature side.
//! - Status overlay art is shared across all families, never per-family.
//! - CreatureBar's 16-part vocabulary maps onto the doll's 14 parts at this
//!   adapter rather than rippling foot parts and a `nerves` rename into the
//!   player-doll ecosystem and its published assets.

pub mod geometry;
pub mod naming;
pub mod solver;

use std::path::{Path, PathBuf};

use crate::config::skins::{
    self, CardOverlay, CreatureCardSkin, LiftSpec, CREATURE_RESOLVE_DEFAULT,
};
use crate::core::gameobj_data::GameObjData;
use crate::core::state::{CreatureFlags, GameState};

/// One creature's resolved card for this frame: which variant (if any) is
/// active, its lift, and which overlay layers draw. Borrowed from the skin
/// manifest — resolve per creature per frame, render from the result.
#[derive(Debug, Clone)]
pub struct ResolvedCard<'a> {
    skin: &'a CreatureCardSkin,
    /// Index into `skin.variants` when one matched (first match wins).
    variant: Option<usize>,
    /// Active overlays in declaration order (stacking, unlike variants).
    pub overlays: Vec<&'a CardOverlay>,
}

impl<'a> ResolvedCard<'a> {
    /// Active variant name, None = default set.
    pub fn variant_name(&self) -> Option<&'a str> {
        self.variant.map(|i| self.skin.variants[i].name.as_str())
    }

    /// The active set's base override: a matched variant with authored art
    /// replaces the resolve cascade wholesale; a variant without `base`
    /// (pure-lift airborne) keeps the cascade's ground pose.
    pub fn base_override(&self) -> Option<&'a str> {
        self.variant
            .and_then(|i| self.skin.variants[i].skin.base.as_deref())
    }

    /// Screen-space lift of the active variant (airborne), if any.
    pub fn lift(&self) -> Option<LiftSpec> {
        self.variant.and_then(|i| self.skin.variants[i].skin.lift)
    }

    /// Anchor point by name: active variant's calibration, else the default
    /// set's, else the built-in resting position. Unknown names → None.
    pub fn anchor(&self, name: &str) -> Option<[f32; 2]> {
        self.authored_anchor(name)
            .or_else(|| skins::default_creature_anchor(name))
    }

    /// Human-authored calibration only (variant set, then default set) — no
    /// built-in fallback. Renderers slot per-image sidecar anchors between
    /// this and the global defaults, so skin calibration wins over the art's
    /// own metadata, which in turn wins over guesses.
    pub fn authored_anchor(&self, name: &str) -> Option<[f32; 2]> {
        let lookup = |anchors: &std::collections::HashMap<String, [f32; 2]>| {
            anchors
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, a)| *a)
        };
        self.variant
            .and_then(|i| lookup(&self.skin.variants[i].skin.anchors))
            .or_else(|| lookup(&self.skin.anchors))
    }

    /// Injury overlay image for a part at a wound level (1-3), from the
    /// active set's part tables. Creatures take wounds only: level 0
    /// (healthy) resolves its art when authored, scar levels (4-6) always
    /// return None — the key space is reserved, not honored.
    pub fn part_overlay(&self, part: &str, level: u8) -> Option<&'a str> {
        if level > 3 {
            return None;
        }
        let key = skins::severity_key_from_level(level)?;
        let parts = match self.variant {
            Some(i) => &self.skin.variants[i].skin.parts,
            None => &self.skin.parts,
        };
        let spec = parts
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(part))
            .map(|(_, spec)| spec)?;
        spec.overlays.get(key).map(String::as_str)
    }
}

/// Resolve one creature's card against the skin template: first matching
/// variant wins (doll-style), every matching overlay stacks. Overlay and
/// variant conditions are creature-scoped (`crtr_status` tests this
/// creature); player-scoped leaves still read the player, so cards can mix
/// in RT, time-of-day, and friends.
pub fn resolve_card<'a>(
    skin: &'a CreatureCardSkin,
    flags: &CreatureFlags,
    gs: &GameState,
    now_server: i64,
    gameobj: Option<&GameObjData>,
) -> ResolvedCard<'a> {
    let variant = skin.variants.iter().position(|v| {
        crate::core::conditions::eval_condition_for_creature(
            &v.when, gs, now_server, gameobj, flags,
        )
    });
    let overlays = skin
        .overlays
        .iter()
        .filter(|o| {
            crate::core::conditions::eval_condition_for_creature(
                &o.when, gs, now_server, gameobj, flags,
            )
        })
        .collect();
    ResolvedCard {
        skin,
        variant,
        overlays,
    }
}

/// Expand the base-image resolve cascade for one creature. Placeholders:
/// `{noun}` and `{family}`; a candidate whose placeholder can't be filled
/// is skipped, so a family-less creature just falls through to the next
/// tier. The manifest's `base` rides at the end as the final fallback.
pub fn base_candidates(
    skin: &CreatureCardSkin,
    noun: Option<&str>,
    family: Option<&str>,
) -> Vec<String> {
    let cascade: Vec<&str> = if skin.resolve.is_empty() {
        CREATURE_RESOLVE_DEFAULT.to_vec()
    } else {
        skin.resolve.iter().map(String::as_str).collect()
    };
    let mut out = Vec::new();
    for template in cascade {
        let mut path = template.to_string();
        let mut ok = true;
        for (placeholder, value) in [("{noun}", noun), ("{family}", family)] {
            if path.contains(placeholder) {
                match value {
                    Some(v) if !v.is_empty() => path = path.replace(placeholder, v),
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
        }
        if ok {
            out.push(path);
        }
    }
    if let Some(base) = skin.base.as_deref() {
        if !out.iter().any(|p| p == base) {
            out.push(base.to_string());
        }
    }
    out
}

/// First cascade candidate that exists on disk under the skin root (the
/// same relative/absolute path rules as every other skin image). None =
/// no art for this creature; the renderer falls back to its generated
/// placeholder card.
pub fn resolve_base_image(
    root: &Path,
    skin: &CreatureCardSkin,
    noun: Option<&str>,
    family: Option<&str>,
) -> Option<PathBuf> {
    base_candidates(skin, noun, family)
        .into_iter()
        .map(|candidate| skins::resolve_image_path(root, &candidate))
        .find(|path| path.is_file())
}

/// One resolved, tier-LOCKED art source: the base image plus the
/// directory and token that own every overlay for this creature. The
/// cascade picks one tier, not one file — all wound/pose overlays come
/// from `{dir}/{token}_*.png`, never mixed across tiers.
#[derive(Debug, Clone)]
pub struct ResolvedTierArt {
    /// Absolute path of the tier's base image.
    pub base: PathBuf,
    /// Absolute directory the tier owns (overlays live here).
    pub dir: PathBuf,
    /// File-name prefix within the tier ("mongrel_kobold", "default").
    pub token: String,
}

/// Tier-locked resolution (the Niffy scheme): `variant → noun → family →
/// default`, each a `creatures/…` folder whose files are prefixed with
/// the folder's own token. Legacy flat files (`creatures/kobold.png`)
/// keep working as that tier with the category root as its directory.
/// A skin authoring its own `resolve` cascade takes precedence — its
/// first hit becomes the locked tier, token = the file stem.
pub fn resolve_tier_art(
    root: &Path,
    skin: &CreatureCardSkin,
    name: Option<&str>,
    noun: Option<&str>,
    family: Option<&str>,
) -> Option<ResolvedTierArt> {
    // Authored cascade wins (the power tier); the hit's directory and
    // stem own the overlays so tier locking applies to skins too.
    if !skin.resolve.is_empty() || skin.base.is_some() {
        let base = resolve_base_image(root, skin, noun, family)?;
        let token = base.file_stem()?.to_string_lossy().to_string();
        let dir = base.parent()?.to_path_buf();
        return Some(ResolvedTierArt { base, dir, token });
    }

    let noun_token = noun.map(naming::slug).filter(|t| !t.is_empty());
    let family_token = family.map(naming::slug).filter(|t| !t.is_empty());
    let name_tokens = name.map(naming::name_token_variants).unwrap_or_default();

    // (folder-relative dir, token) per tier, in lock order. Full-name art
    // is the primary tier (`creatures/<name>/<name>.png` — multiple
    // creatures share a noun, so the full name is the identity); noun and
    // family are the fallbacks.
    let mut tiers: Vec<(String, String)> = Vec::new();
    for name in &name_tokens {
        if noun_token.as_ref() == Some(name) {
            continue;
        }
        tiers.push((format!("creatures/{name}"), name.clone()));
    }
    if let Some(noun) = &noun_token {
        tiers.push((format!("creatures/{noun}"), noun.clone()));
    }
    if let Some(family) = &family_token {
        tiers.push((format!("creatures/{family}"), family.clone()));
    }
    tiers.push(("creatures/default".to_string(), "default".to_string()));

    for (dir, token) in tiers {
        // Folder form: creatures/<tier>/<token>.png.
        let foldered = skins::resolve_image_path(root, &format!("{dir}/{token}.png"));
        if foldered.is_file() {
            let parent = foldered.parent()?.to_path_buf();
            return Some(ResolvedTierArt {
                base: foldered,
                dir: parent,
                token,
            });
        }
        // Legacy flat form: creatures/<token>.png — the tier still locks
        // (overlays resolve as creatures/<token>_*.png beside it).
        let flat = skins::resolve_image_path(root, &format!("creatures/{token}.png"));
        if flat.is_file() {
            let parent = flat.parent()?.to_path_buf();
            return Some(ResolvedTierArt {
                base: flat,
                dir: parent,
                token,
            });
        }
    }
    None
}

/// Zero-config status overlays from the pool convention: any image at
/// `creatures/status/<id>.png` binds to the creature-status flag of the
/// same name (body-wrapped, feed-sourced) with no TOML at all — drop
/// `rooted.png` in the folder and rooted creatures wear it. An authored
/// overlay already testing that flag suppresses the convention one, so
/// explicit configs (custom anchors, animation, ranked art) stay the
/// power tier. Flag ids are open-ended, matching `CrtrStatus` semantics.
pub fn convention_status_overlays(existing: &[CardOverlay]) -> Vec<CardOverlay> {
    let mut covered: Vec<String> = Vec::new();
    for overlay in existing {
        overlay.when.referenced_crtr_status_ids(&mut covered);
    }
    let mut out: Vec<CardOverlay> = Vec::new();
    for image in crate::config::pool::list_category("creatures") {
        if !image
            .set
            .as_deref()
            .is_some_and(|set| set.eq_ignore_ascii_case("status"))
        {
            continue;
        }
        let id = image.stem().to_ascii_lowercase();
        if covered.iter().any(|c| c.eq_ignore_ascii_case(&id)) {
            continue;
        }
        out.push(CardOverlay {
            image: image.pool_path.clone(),
            space: Default::default(),
            anchor: None,
            // Above authored layer-0 art by default; explicit overlays
            // pick their own layer to reorder.
            layer: 10,
            source: Default::default(),
            timeout_s: None,
            animate: None,
            when: crate::config::Condition::CrtrStatus { id, active: true },
        });
    }
    out.sort_by(|a, b| a.image.cmp(&b.image));
    out
}

/// The `{family}` value for a noun, from the bundled bestiary: filled only
/// when every bestiary entry sharing the noun agrees on a family (an
/// ambiguous noun like "troll" resolves art by noun alone rather than
/// guessing a family). Sanitized for the resolve cascade's file paths:
/// lowercase, spaces -> underscores.
pub fn family_for_noun(noun: &str) -> Option<String> {
    let entries = crate::core::bestiary::format::shared().by_noun(noun);
    let mut families = entries.iter().filter_map(|e| e.family.as_deref());
    let first = families.next()?;
    if families.all(|f| f.eq_ignore_ascii_case(first)) {
        Some(first.to_ascii_lowercase().replace(' ', "_"))
    } else {
        None
    }
}

/// Whether a room creature belongs on the creature field. The targets-list
/// gate (hostile + valid_target) EXCEPT death: a corpse keeps its card,
/// tinted, until looting drops it from the roster — its square frees then.
pub fn field_member(c: &crate::core::state::Creature, excluded_nouns: &[String]) -> bool {
    let hostile = c.flags.as_ref().is_some_and(|f| f.hostile);
    hostile && (c.is_valid_target(excluded_nouns) || c.is_dead())
}

/// Card size for one creature, resolved at arrival — synchronously, so
/// the solver reserves the right room before any art has loaded.
///
/// Scale cascade: bestiary `height` (feet; a ~6 ft human is the 1.2-unit
/// default card) → bestiary `size` bucket → default. Clamped so extremes
/// stay readable — a rat still gets a visible card, a dragon still fits
/// the stage; relative size within the clamp is information and is never
/// normalized away. Bosses keep at least the old visibility bump on top.
pub fn card_size_for(c: &crate::core::state::Creature) -> solver::CardSize {
    let (standing, prone) = card_boxes_for(c);
    let lying = c.is_dead() || c.flags.as_ref().is_some_and(|f| f.has_flag("prone"));
    if lying {
        prone
    } else {
        standing
    }
}

/// Both pose boxes for one creature, pose-agnostic: (standing, prone).
/// The solver reserves the union of the two (the fall envelope) at
/// arrival, so a later pose swap never needs to move anyone.
///
/// Resolved through the SHARED geometry contract (`geometry` module):
/// bestiary reference height + the art's cached calibration (sidecar
/// `size` absolute override, content aspect, footprint span), so the
/// reservation and hit rectangle agree with what the renderer draws.
/// Calibration comes from the process-wide store, fed by the frontend's
/// art prepare pass (sidecar TOML + alpha bounds, read there — never
/// inside placement loops); tokens without an entry get the historical
/// deterministic fallback, and the store revision recalibrates placed
/// units in place when metadata arrives later.
pub fn card_boxes_for(
    c: &crate::core::state::Creature,
) -> (solver::CardSize, solver::CardSize) {
    let g = geometry_for(c);
    (g.standing, g.prone)
}

/// The full shared geometry for one creature, calibration included.
pub fn geometry_for(c: &crate::core::state::Creature) -> geometry::CreatureGeometry {
    let token = naming::name_token(&c.name);
    let cal = geometry::calibrations()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .resolve(&token);
    geometry::resolve_geometry(c, cal.as_ref())
}

/// Standing card height in world units from bestiary policy alone (no art
/// calibration), pose-agnostic. Renderers should prefer the placed unit's
/// `standing.h` — the calibrated shared geometry — over this.
pub fn standing_height_for(c: &crate::core::state::Creature) -> f32 {
    geometry::resolve_geometry(c, None).world_h
}

/// Bestiary body type (`biped`/`quadruped`/`avian`/`ooze`…), lowercased,
/// matched with the same exact-name-then-noun-agreement discipline as
/// [`bestiary_height_units`].
fn bestiary_body_type(name: &str, noun: Option<&str>) -> Option<String> {
    let canonical = naming::canonical_name(name);
    let noun = noun
        .filter(|n| !n.trim().is_empty())
        .map(str::to_string)
        .or_else(|| canonical.split_whitespace().last().map(str::to_string))?;
    let db = crate::core::bestiary::format::shared();
    let entries = db.by_noun(&noun);
    let of = |e: &&crate::core::bestiary::CreatureEntry| -> Option<String> {
        e.creature_type
            .as_deref()
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
    };
    if let Some(entry) = entries
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(&canonical))
    {
        return of(&entry);
    }
    let mut types = entries.iter().filter_map(|e| of(&e));
    let first = types.next()?;
    types.all(|t| t == first).then_some(first)
}

/// World-unit height for a creature from the bundled bestiary, plus
/// whether it came from the `height` field (true) or the size bucket
/// (false). Matching keys on the boon-stripped canonical name (the wire
/// name may carry a boon adjective the templates never do), falling back
/// to the noun when every entry sharing it agrees — the same discipline
/// as [`family_for_noun`], so an ambiguous noun never guesses.
fn bestiary_height_units(name: &str, noun: Option<&str>) -> Option<(f32, bool)> {
    let canonical = naming::canonical_name(name);
    let noun = noun
        .filter(|n| !n.trim().is_empty())
        .map(str::to_string)
        .or_else(|| canonical.split_whitespace().last().map(str::to_string))?;
    let db = crate::core::bestiary::format::shared();
    let entries = db.by_noun(&noun);
    let of = |e: &crate::core::bestiary::CreatureEntry| -> Option<(f32, bool)> {
        if let Some(feet) = e.height {
            // 6 ft ≡ the 1.2-unit default card.
            return Some((feet as f32 * 0.2, true));
        }
        match e.size.as_deref().map(str::trim) {
            Some(s) if s.eq_ignore_ascii_case("tiny") => Some((0.55, false)),
            Some(s) if s.eq_ignore_ascii_case("small") => Some((0.85, false)),
            Some(s) if s.eq_ignore_ascii_case("medium") => Some((1.2, false)),
            Some(s) if s.eq_ignore_ascii_case("large") => Some((1.6, false)),
            Some(s) if s.eq_ignore_ascii_case("huge") => Some((2.1, false)),
            _ => None,
        }
    };
    if let Some(entry) = entries
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(&canonical))
    {
        return of(entry);
    }
    // No exact name: only trust the noun when its entries agree.
    let mut heights = entries.iter().filter_map(|e| of(e));
    let first = heights.next()?;
    heights
        .all(|(h, _)| (h - first.0).abs() < 0.05)
        .then_some(first)
}

/// Event-driven roster sync: diff the field's units against the room's
/// creatures. Arrivals place (permanently), departures free their square.
/// Cheap no-op while the roster generation is unchanged, so calling it
/// once per frame costs one comparison.
/// Resolve the creature field's params from the layered camera/solver
/// sources: built-in defaults → the "default" scene (the global camera
/// home) → the active scene's embedded camera+solver → the blanket
/// override → that scene's own override. Field-by-field via
/// `apply_camera`/`apply_solver`, so unset keys at any layer fall through
/// (room scenes save sparse — only what differs from the default scene).
///
/// Pass `default_scene: None` when the active scene IS the default, so it
/// isn't applied twice. `scene_name` keys the per-scene override lookup;
/// disabled overrides keep their values but skip.
pub fn resolve_field_params(
    default_scene: Option<&crate::config::scenes::StageScene>,
    scene: Option<&crate::config::scenes::StageScene>,
    scene_name: Option<&str>,
    overrides: &crate::config::creature_field::FieldOverrides,
) -> solver::FieldParams {
    let mut params = solver::FieldParams::default();
    if let Some(scene) = default_scene {
        params.apply_camera(&scene.camera);
        params.apply_solver(&scene.solver);
    }
    if let Some(scene) = scene {
        params.apply_camera(&scene.camera);
        params.apply_solver(&scene.solver);
    }
    if overrides.blanket.enabled {
        params.apply_camera(&overrides.blanket.camera);
        params.apply_solver(&overrides.blanket.solver);
    }
    if let Some(entry) = scene_name.and_then(|name| overrides.scenes.get(name)) {
        if entry.enabled {
            params.apply_camera(&entry.camera);
            params.apply_solver(&entry.solver);
        }
    }
    params
}

/// Zero-config GLOBAL wound overlays, mirroring the status convention:
/// any image at `creatures/wounds/<part><rank>.png` (`chest2.png`,
/// `leftarm1.png`, ranks 1-3, doll part vocabulary) becomes the wound art
/// for every creature whose locked tier ships no wound extras of its own.
/// Per-creature tier art (`{token}_{part}{rank}`) always wins — a tier
/// with ANY wound art suppresses this layer wholesale (tier locking), and
/// creatures with neither fall through to the procedural rank dot, so
/// wounds are never invisible. Authored part-table entries are never
/// overwritten — the convention only fills empty slots.
pub fn convention_wound_parts(
    parts: &mut std::collections::HashMap<String, crate::config::skins::DollPartSpec>,
) {
    for image in crate::config::pool::list_category("creatures") {
        if !image
            .set
            .as_deref()
            .is_some_and(|set| set.eq_ignore_ascii_case("wounds"))
        {
            continue;
        }
        let stem = image.stem().to_ascii_lowercase();
        let Some(rank) = stem.chars().last().filter(|c| ('1'..='3').contains(c)) else {
            continue;
        };
        let part_key = &stem[..stem.len() - 1];
        // Canonical part casing (leftarm -> leftArm) so the lookup in
        // `part_overlay` (case-insensitive on canonical keys) matches.
        let Some(part) = crate::config::INJURY_AREAS
            .iter()
            .find(|part| part.to_ascii_lowercase() == part_key)
        else {
            tracing::debug!(
                "creatures/wounds/{} does not name a body part + rank; ignored",
                image.stem()
            );
            continue;
        };
        parts
            .entry((*part).to_string())
            .or_default()
            .overlays
            .entry(format!("injury{rank}"))
            .or_insert_with(|| image.pool_path.clone());
    }
}

/// AUTHORED exclusion metadata for a scene's props, from each prop
/// image's sidecar: calibrated `exclude` edges + recorded `aspect` give
/// the exclusion extents in WORLD units around the prop's foot point.
/// Projection-free by design (finding 3): the solver projects these
/// through the CURRENT placement geometry — column growth included — via
/// `CreatureField::obstacle_span`, so spans can never go stale against
/// the camera or the floor. Props without calibration (or without a
/// sidecar at all) exclude nothing — the calibrator IS the data.
pub fn scene_obstacles(
    scene: Option<&crate::config::scenes::StageScene>,
) -> Vec<solver::Obstacle> {
    let Some(scene) = scene else {
        return Vec::new();
    };
    let Ok(pool_root) = crate::config::Config::global_images_dir() else {
        return Vec::new();
    };
    scene
        .props
        .iter()
        .filter_map(|prop| {
            let abs = pool_root.join(&prop.image);
            let sidecar: crate::config::pool::CreatureSidecar =
                crate::config::pool::read_sidecar(&abs)?;
            let [left, right] = sidecar.exclude?;
            let aspect = sidecar.aspect?;
            let world_h = sidecar.size.unwrap_or(1.0) * prop.scale.max(0.01);
            // The prop's drawn width in world units; exclusion edges are
            // fractions of it, measured from the feet anchor.
            let world_w = world_h * aspect.max(0.01);
            let feet_x = sidecar
                .anchors
                .get("feet")
                .map(|anchor| anchor[0])
                .unwrap_or(0.5);
            let lo = left.min(right).clamp(0.0, 1.0);
            let hi = left.max(right).clamp(0.0, 1.0);
            Some(solver::Obstacle {
                x: prop.x,
                z: prop.z,
                left_w: (feet_x - lo) * world_w,
                right_w: (hi - feet_x) * world_w,
            })
        })
        .collect()
}

pub fn sync_field(
    field: &mut solver::CreatureField,
    synced_gen: &mut u64,
    synced_cal: &mut u64,
    gs: &GameState,
    excluded_nouns: &[String],
) {
    // Calibration metadata arrived or reloaded since the last sync:
    // re-derive every placed unit's boxes through the shared geometry.
    // A unit whose boxes still fit stays exactly put; one whose enlarged
    // envelope now breaks the arrival hard bound is re-homed by
    // `recalibrate`'s conflict policy (nearest valid square; neighbours
    // never move) — the fall envelopes future arrivals reserve against
    // stop being stale and no overlap outlives the reload.
    let cal_rev = geometry::calibrations()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .revision();
    let recalibrate = *synced_cal != cal_rev;
    *synced_cal = cal_rev;
    if recalibrate {
        for c in gs
            .room_creatures
            .iter()
            .filter(|c| field_member(c, excluded_nouns))
        {
            let primary = field
                .unit_of(&c.id)
                .is_some_and(|u| u.members.first().map(String::as_str) == Some(c.id.as_str()));
            if primary {
                let (standing, prone) = card_boxes_for(c);
                field.recalibrate(&c.id, standing, prone, card_size_for(c));
            }
        }
    }
    if *synced_gen == gs.room_creatures_generation {
        return;
    }
    *synced_gen = gs.room_creatures_generation;
    let wanted: Vec<&crate::core::state::Creature> = gs
        .room_creatures
        .iter()
        .filter(|c| field_member(c, excluded_nouns))
        .collect();
    // Departures first, so a full room swap frees the floor before the new
    // room's creatures fit themselves in.
    let gone: Vec<String> = field
        .units()
        .iter()
        .flat_map(|u| u.members.iter())
        .filter(|m| !wanted.iter().any(|c| &c.id == *m))
        .cloned()
        .collect();
    for exist in gone {
        field.depart(&exist);
    }
    for c in wanted {
        // Pose changes (prone/stand/death) bump the generation via
        // crtrStatus, so already-placed units re-derive their box here;
        // resize only through the unit's primary member so a rider's pose
        // never clobbers the mount's footprint.
        let primary = field
            .unit_of(&c.id)
            .map(|u| u.members.first().map(String::as_str) == Some(c.id.as_str()));
        match primary {
            None => {
                let (standing, prone) = card_boxes_for(c);
                field.arrive(&c.id, standing, prone);
                // The creature may already be down on arrival (we walked
                // in on it); the box swaps to the current pose here.
                field.resize(&c.id, card_size_for(c));
            }
            Some(true) => field.resize(&c.id, card_size_for(c)),
            Some(false) => {}
        }
    }
}

/// Map an external creature body-part name (CreatureBar vocabulary) onto
/// the canonical doll part key used everywhere in Vellum. Differences:
/// `nerves` -> `nsys`, and foot wounds fold into the matching leg. Canonical
/// names pass through unchanged (case-insensitively); unknown parts return
/// None and are dropped, same as the doll loader does.
pub fn canonical_part(name: &str) -> Option<&'static str> {
    let folded = match name.to_ascii_lowercase().as_str() {
        "nerves" => "nsys",
        "leftfoot" => "leftLeg",
        "rightfoot" => "rightLeg",
        other => {
            return crate::config::INJURY_AREAS
                .iter()
                .find(|part| part.eq_ignore_ascii_case(other))
                .copied();
        }
    };
    Some(folded)
}

#[cfg(test)]
mod card_tests {
    use super::*;
    use crate::config::skins::{AnimateKind, OverlaySource, OverlaySpace, SkinManifest};

    #[test]
    fn convention_status_overlays_bind_flags_and_respect_authored() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());
        crate::config::pool::invalidate_cache();

        let status = crate::config::Config::global_image_category_dir("creatures")
            .unwrap()
            .join("status");
        std::fs::create_dir_all(&status).unwrap();
        std::fs::write(status.join("rooted.png"), b"x").unwrap();
        std::fs::write(status.join("stunned.png"), b"x").unwrap();

        // The author already handles rooted; only stunned synthesizes.
        let skin: CreatureCardSkin = toml::from_str(
            r#"
            [[overlays]]
            image = "fancy_roots.png"
            anchor = "feet"
            when = { type = "crtr_status", id = "ROOTED" }
            "#,
        )
        .unwrap();
        let extra = convention_status_overlays(&skin.overlays);
        assert_eq!(extra.len(), 1);
        let overlay = &extra[0];
        assert_eq!(overlay.image, "creatures/status/stunned.png");
        assert!(overlay.anchor.is_none(), "convention art body-wraps");
        assert_eq!(overlay.source, OverlaySource::Feed);
        assert!(matches!(
            &overlay.when,
            crate::config::Condition::CrtrStatus { id, active: true } if id == "stunned"
        ));

        // No authored overlays: both flags bind.
        assert_eq!(convention_status_overlays(&[]).len(), 2);

        std::env::remove_var("VELLUM_FE_DIR");
    }

    /// The plan's reference manifest, parsed for real: cascade, anchors,
    /// wound art, a feed overlay, a screen-space animated overlay, a
    /// message-derived ranked overlay, and both posture variants.
    const MANIFEST: &str = r#"
        [creature_card]
        base = "creatures/default.png"
        resolve = ["creatures/{noun}.png", "creatures/{family}.png", "creatures/default.png"]

        [creature_card.anchors]
        head   = [0.50, 0.09]
        saddle = [0.52, 0.34]

        [creature_card.head]
        injury1 = "creatures/fx/head_i1.png"
        scar1   = "creatures/fx/head_s1.png"   # reserved key: must be dead

        [[creature_card.overlays]]
        image = "fx/webbed.png"
        when  = { type = "crtr_status", id = "webbed", active = true }

        [[creature_card.overlays]]
        image   = "fx/stun_star.png"
        space   = "screen"
        anchor  = "head"
        layer   = 70
        animate = { kind = "orbit", count = 3, period_ms = 2400 }
        when    = { type = "crtr_status", id = "stunned", active = true }

        [[creature_card.overlays]]
        image     = "fx/bleed_r{severity}.png"
        source    = "message"
        timeout_s = 15
        when      = { type = "crtr_status", id = "bleeding", active = true }

        [[creature_card.variants]]
        name = "downed"
        [creature_card.variants.when]
        type = "any"
        conditions = [
          { type = "crtr_status", id = "prone",    active = true },
          { type = "crtr_status", id = "kneeling", active = true },
        ]
        [creature_card.variants.skin]
        base = "creatures/{family}_prone.png"
        [creature_card.variants.skin.anchors]
        head = [0.20, 0.60]

        [[creature_card.variants]]
        name = "airborne"
        [creature_card.variants.when]
        type = "any"
        conditions = [
          { type = "crtr_status", id = "flying",   active = true },
          { type = "crtr_status", id = "hovering", active = true },
        ]
        [creature_card.variants.skin]
        lift = { offset_y = -0.22, shadow_scale = 0.55, shadow_opacity = 0.4 }
    "#;

    fn card() -> CreatureCardSkin {
        toml::from_str::<SkinManifest>(MANIFEST)
            .unwrap()
            .creature_card
    }

    fn flags(attrs: &[(&str, &str)]) -> CreatureFlags {
        CreatureFlags::from_xml_attrs(attrs.iter().copied())
    }

    fn resolve<'a>(skin: &'a CreatureCardSkin, f: &CreatureFlags) -> ResolvedCard<'a> {
        resolve_card(skin, f, &GameState::new(), 0, None)
    }

    #[test]
    fn manifest_parses_with_typed_overlays() {
        let skin = card();
        assert_eq!(skin.overlays.len(), 3);
        assert_eq!(skin.overlays[0].space, OverlaySpace::Quad);
        assert_eq!(skin.overlays[0].source, OverlaySource::Feed);
        assert_eq!(skin.overlays[1].space, OverlaySpace::Screen);
        assert_eq!(skin.overlays[1].layer, 70);
        let anim = skin.overlays[1].animate.as_ref().unwrap();
        assert_eq!(anim.kind, AnimateKind::Orbit);
        assert_eq!(anim.count, 3);
        assert_eq!(skin.overlays[2].source, OverlaySource::Message);
        assert_eq!(skin.overlays[2].timeout_s, Some(15));
    }

    #[test]
    fn healthy_creature_gets_default_set_and_no_overlays() {
        let skin = card();
        let r = resolve(&skin, &flags(&[("hostile", "1")]));
        assert_eq!(r.variant_name(), None);
        assert!(r.overlays.is_empty());
        assert_eq!(r.base_override(), None);
        assert_eq!(r.lift(), None);
    }

    #[test]
    fn matching_overlays_stack_while_variants_pick_first() {
        let skin = card();
        let r = resolve(
            &skin,
            &flags(&[
                ("webbed", "1"),
                ("stunned", "1"),
                ("prone", "1"),
                ("hovering", "1"),
            ]),
        );
        // Both status overlays active, in declaration order.
        assert_eq!(r.overlays.len(), 2);
        assert_eq!(r.overlays[0].image, "fx/webbed.png");
        assert_eq!(r.overlays[1].image, "fx/stun_star.png");
        // downed declared first: it wins over airborne, wholesale.
        assert_eq!(r.variant_name(), Some("downed"));
        assert_eq!(r.base_override(), Some("creatures/{family}_prone.png"));
        assert_eq!(r.lift(), None, "airborne's lift must not leak into downed");
    }

    #[test]
    fn airborne_variant_is_pure_lift_keeping_ground_pose() {
        let skin = card();
        let r = resolve(&skin, &flags(&[("flying", "1")]));
        assert_eq!(r.variant_name(), Some("airborne"));
        assert_eq!(
            r.base_override(),
            None,
            "no base: cascade's ground pose is kept"
        );
        let lift = r.lift().unwrap();
        assert_eq!(lift.offset_y, -0.22);
        assert_eq!(lift.shadow_scale, 0.55);
    }

    /// authored_anchor exposes only human calibration — renderers slot
    /// per-image sidecar anchors between it and the built-in defaults.
    #[test]
    fn authored_anchor_has_no_builtin_fallback() {
        let skin = card();
        let r = resolve(&skin, &flags(&[]));
        assert_eq!(r.authored_anchor("head"), Some([0.50, 0.09]));
        assert_eq!(r.authored_anchor("feet"), None, "builtin must not leak");
        // Variant calibration still wins inside authored_anchor.
        let r = resolve(&skin, &flags(&[("prone", "1")]));
        assert_eq!(r.authored_anchor("head"), Some([0.20, 0.60]));
    }

    #[test]
    fn anchors_cascade_variant_then_default_then_builtin() {
        let skin = card();
        // Default set: calibrated head.
        let r = resolve(&skin, &flags(&[]));
        assert_eq!(r.anchor("head"), Some([0.50, 0.09]));
        assert_eq!(r.anchor("saddle"), Some([0.52, 0.34]));
        // "feet" is uncalibrated: built-in resting position.
        assert_eq!(r.anchor("feet"), Some([0.50, 0.98]));
        assert_eq!(r.anchor("nonsense"), None);
        // Downed variant recalibrates head; saddle falls back to default set.
        let r = resolve(&skin, &flags(&[("prone", "1")]));
        assert_eq!(r.anchor("head"), Some([0.20, 0.60]));
        assert_eq!(r.anchor("saddle"), Some([0.52, 0.34]));
    }

    /// Creatures take wounds only: authored scar art is dead, wound art
    /// resolves, and levels outside 0-3 never produce an image.
    #[test]
    fn scar_keys_are_reserved_but_dead() {
        let skin = card();
        let r = resolve(&skin, &flags(&[]));
        assert_eq!(r.part_overlay("head", 1), Some("creatures/fx/head_i1.png"));
        assert_eq!(r.part_overlay("HEAD", 1), Some("creatures/fx/head_i1.png"));
        assert_eq!(
            r.part_overlay("head", 4),
            None,
            "scar1 authored but must not resolve"
        );
        assert_eq!(r.part_overlay("head", 2), None);
        assert_eq!(r.part_overlay("chest", 1), None);
    }

    #[test]
    fn base_cascade_expands_and_skips_unfillable_tiers() {
        let skin = card();
        assert_eq!(
            base_candidates(&skin, Some("kobold"), Some("goblinkin")),
            vec![
                "creatures/kobold.png",
                "creatures/goblinkin.png",
                "creatures/default.png",
            ]
        );
        // No family: that tier is skipped, not emitted half-expanded.
        assert_eq!(
            base_candidates(&skin, Some("kobold"), None),
            vec!["creatures/kobold.png", "creatures/default.png"]
        );
        // Nothing known: only the literal tiers + fallback survive.
        assert_eq!(
            base_candidates(&skin, None, None),
            vec!["creatures/default.png"]
        );
    }

    #[test]
    fn empty_resolve_uses_builtin_cascade_and_appends_distinct_base() {
        let skin: CreatureCardSkin = toml::from_str::<SkinManifest>(
            r#"
            [creature_card]
            base = "creatures/fallback.png"
            "#,
        )
        .unwrap()
        .creature_card;
        assert_eq!(
            base_candidates(&skin, Some("troll"), None),
            vec![
                "creatures/troll.png",
                "creatures/default.png",
                "creatures/fallback.png",
            ]
        );
    }

    #[test]
    fn resolve_base_image_finds_first_existing_candidate() {
        let dir = std::env::temp_dir().join(format!("vellum_cc_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("creatures"));
        std::fs::write(dir.join("creatures/default.png"), b"png").unwrap();
        std::fs::write(dir.join("creatures/troll.png"), b"png").unwrap();
        let skin = card();

        // Noun art exists: it wins over the fallback.
        let hit = resolve_base_image(&dir, &skin, Some("troll"), None).unwrap();
        assert!(hit.ends_with(Path::new("creatures/troll.png")));
        // Unknown noun: falls through the cascade to default.png.
        let hit = resolve_base_image(&dir, &skin, Some("bandersnatch"), None).unwrap();
        assert!(hit.ends_with(Path::new("creatures/default.png")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Tier order pin: full-name art (`creatures/<name>/<name>.png`)
    /// beats noun art — multiple creatures share a noun, so the full name
    /// is the identity. A hyphenated on-disk name resolves too.
    #[test]
    fn resolve_tier_art_prefers_full_name_over_noun() {
        let dir = std::env::temp_dir().join(format!("vellum_tier_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("creatures/boar"));
        let _ = std::fs::create_dir_all(dir.join("creatures/gold-bristled_hinterboar"));
        std::fs::write(dir.join("creatures/boar/boar.png"), b"png").unwrap();
        let skin = CreatureCardSkin::default();

        // Only noun art: noun tier locks.
        let art =
            resolve_tier_art(&dir, &skin, Some("a gold-bristled hinterboar"), Some("boar"), None)
                .unwrap();
        assert!(art.base.ends_with(Path::new("creatures/boar/boar.png")));
        assert_eq!(art.token, "boar");

        // A name starting with a boon adjective still finds its own art:
        // the unstripped full name probes before the boon-stripped form.
        let _ = std::fs::create_dir_all(dir.join("creatures/shining_winged_disir"));
        std::fs::write(
            dir.join("creatures/shining_winged_disir/shining_winged_disir.png"),
            b"png",
        )
        .unwrap();
        let art = resolve_tier_art(&dir, &skin, Some("a shining winged disir"), Some("disir"), None)
            .unwrap();
        assert_eq!(art.token, "shining_winged_disir");

        // Full-name folder (hyphen-preserving spelling): beats noun.
        std::fs::write(
            dir.join("creatures/gold-bristled_hinterboar/gold-bristled_hinterboar.png"),
            b"png",
        )
        .unwrap();
        let art =
            resolve_tier_art(&dir, &skin, Some("a gold-bristled hinterboar"), Some("boar"), None)
                .unwrap();
        assert_eq!(art.token, "gold-bristled_hinterboar");
        assert!(art.dir.ends_with(Path::new("creatures/gold-bristled_hinterboar")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Creature rules may mix in player state: the same tree that gates on
    /// the creature's flags can also gate on the player's indicators.
    #[test]
    fn overlay_conditions_can_mix_player_state() {
        let skin: CreatureCardSkin = toml::from_str::<SkinManifest>(
            r#"
            [creature_card]
            [[creature_card.overlays]]
            image = "fx/ambush_target.png"
            [creature_card.overlays.when]
            type = "all"
            conditions = [
              { type = "crtr_status", id = "hostile", active = true },
              { type = "indicator", id = "hidden", active = true },
            ]
            "#,
        )
        .unwrap()
        .creature_card;
        let f = flags(&[("hostile", "1")]);
        let mut gs = GameState::new();
        assert!(resolve_card(&skin, &f, &gs, 0, None).overlays.is_empty());
        gs.status.set("hidden", true);
        assert_eq!(resolve_card(&skin, &f, &gs, 0, None).overlays.len(), 1);
    }
}

#[cfg(test)]
mod wound_convention_tests {
    use super::*;

    #[test]
    fn wounds_folder_fills_part_tables_without_clobbering_authored_art() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());
        let wounds = crate::config::Config::global_images_dir()
            .unwrap()
            .join("creatures/wounds");
        std::fs::create_dir_all(&wounds).unwrap();
        // 1x1 PNG is enough for the lister.
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D',
            b'R', 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0,
            b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
        ];
        for name in ["chest2.png", "leftarm1.png", "nsys3.png", "notapart2.png", "chest9.png"] {
            std::fs::write(wounds.join(name), png).unwrap();
        }
        crate::config::pool::invalidate_cache();

        let mut parts: std::collections::HashMap<String, crate::config::skins::DollPartSpec> =
            Default::default();
        // Authored entry survives the fill.
        parts
            .entry("chest".to_string())
            .or_default()
            .overlays
            .insert("injury2".to_string(), "creatures/fx/custom.png".to_string());
        convention_wound_parts(&mut parts);

        assert_eq!(
            parts["chest"].overlays["injury2"], "creatures/fx/custom.png",
            "authored art never overwritten"
        );
        assert_eq!(
            parts["leftArm"].overlays["injury1"], "creatures/wounds/leftarm1.png",
            "lowercased stem maps to the canonical part key"
        );
        assert_eq!(parts["nsys"].overlays["injury3"], "creatures/wounds/nsys3.png");
        assert!(!parts.contains_key("notapart"), "unknown parts ignored");
        assert!(
            !parts["chest"].overlays.contains_key("injury9"),
            "ranks outside 1-3 ignored"
        );

        std::env::remove_var("VELLUM_FE_DIR");
        crate::config::pool::invalidate_cache();
    }
}

#[cfg(test)]
mod resolve_params_tests {
    use super::*;
    use crate::config::creature_field::{FieldOverride, FieldOverrides};
    use crate::config::scenes::StageScene;
    use crate::config::skins::{CreatureFieldCamera, CreatureFieldSolver};

    fn scene_with_camera() -> StageScene {
        StageScene {
            camera: CreatureFieldCamera {
                focal: Some(500.0),
                horizon: Some(160.0),
                ..Default::default()
            },
            solver: CreatureFieldSolver {
                zone: Some("grid".to_string()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn no_layers_is_default() {
        let params = resolve_field_params(None, None, None, &FieldOverrides::default());
        assert_eq!(params, solver::FieldParams::default());
    }

    #[test]
    fn default_scene_layers_under_the_matched_scene() {
        // Default scene = the global camera home; room scenes save sparse
        // and inherit anything they didn't pin.
        let default_scene = StageScene {
            camera: CreatureFieldCamera {
                focal: Some(480.0),
                eye_height: Some(0.9),
                ..Default::default()
            },
            ..Default::default()
        };
        let room = StageScene {
            camera: CreatureFieldCamera {
                focal: Some(520.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let params = resolve_field_params(
            Some(&default_scene),
            Some(&room),
            Some("cave"),
            &FieldOverrides::default(),
        );
        assert_eq!(params.focal, 520.0, "room scene pins focal");
        assert_eq!(params.cam_h, 0.9, "unpinned eye_height inherits from the default scene");
    }

    #[test]
    fn scene_layers_over_defaults_field_by_field() {
        let scene = scene_with_camera();
        let params =
            resolve_field_params(None, Some(&scene), Some("cave"), &FieldOverrides::default());
        assert_eq!(params.focal, 500.0);
        assert_eq!(params.horizon, 160.0);
        // Unset scene keys keep defaults.
        assert_eq!(params.cam_h, solver::FieldParams::default().cam_h);
        assert!(!params.solver.zone_on, "scene solver zone=grid applied");
    }

    #[test]
    fn blanket_beats_scene_and_per_scene_beats_blanket() {
        let scene = scene_with_camera();
        let mut overrides = FieldOverrides::default();
        overrides.blanket.camera.focal = Some(600.0);
        overrides.scenes.insert(
            "cave".to_string(),
            FieldOverride {
                camera: CreatureFieldCamera {
                    focal: Some(700.0),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let params = resolve_field_params(None, Some(&scene), Some("cave"), &overrides);
        assert_eq!(params.focal, 700.0, "per-scene wins");
        assert_eq!(params.horizon, 160.0, "unset override keys fall through to the scene");
        // A different scene name only gets the blanket.
        let params = resolve_field_params(None, Some(&scene), Some("desert"), &overrides);
        assert_eq!(params.focal, 600.0, "blanket wins where no per-scene entry");
    }

    #[test]
    fn disabled_overrides_are_skipped() {
        let mut overrides = FieldOverrides::default();
        overrides.blanket.camera.focal = Some(600.0);
        overrides.blanket.enabled = false;
        let params = resolve_field_params(None, None, None, &overrides);
        assert_eq!(params.focal, solver::FieldParams::default().focal);
    }

    #[test]
    fn camera_snapshot_roundtrips_through_apply() {
        // Studio Save embeds to_camera()/to_solver_toml(); Load applies
        // them onto defaults — the roundtrip must reproduce the params.
        let mut tuned = solver::FieldParams::default();
        tuned.focal = 450.0;
        tuned.cam_h = 0.93;
        tuned.horizon = 160.0;
        tuned.solver.zone_on = false;
        tuned.solver.relax_steps = 7;
        let mut roundtripped = solver::FieldParams::default();
        roundtripped.apply_camera(&tuned.to_camera());
        roundtripped.apply_solver(&tuned.to_solver_toml());
        assert_eq!(roundtripped, tuned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creaturebar_specific_parts_fold_onto_doll_parts() {
        assert_eq!(canonical_part("nerves"), Some("nsys"));
        assert_eq!(canonical_part("leftFoot"), Some("leftLeg"));
        assert_eq!(canonical_part("rightFoot"), Some("rightLeg"));
    }

    #[test]
    fn canonical_parts_pass_through_any_casing() {
        assert_eq!(canonical_part("head"), Some("head"));
        assert_eq!(canonical_part("leftArm"), Some("leftArm"));
        assert_eq!(canonical_part("leftarm"), Some("leftArm"));
        assert_eq!(canonical_part("NSYS"), Some("nsys"));
    }

    #[test]
    fn unknown_parts_are_dropped() {
        assert_eq!(canonical_part("tail"), None);
        assert_eq!(canonical_part(""), None);
    }

    /// Every CreatureBar part resolves to a doll part — the adapter is
    /// total over the external vocabulary, so no wound is ever lost.
    #[test]
    fn adapter_is_total_over_creaturebar_vocabulary() {
        for part in [
            "abdomen",
            "back",
            "chest",
            "head",
            "leftArm",
            "leftEye",
            "leftFoot",
            "leftHand",
            "leftLeg",
            "neck",
            "nerves",
            "rightArm",
            "rightEye",
            "rightFoot",
            "rightHand",
            "rightLeg",
        ] {
            assert!(
                canonical_part(part).is_some(),
                "CreatureBar part {part} must map to a doll part"
            );
        }
    }

    /// Card scale resolves the bundled bestiary's height (feet, 6 ft ≡ the
    /// 1.2-unit default) with the size bucket as fallback; boon adjectives
    /// on the wire name never break the match; unknowns keep the default.
    #[test]
    fn card_size_resolves_bestiary_height_with_boon_and_fallback() {
        let creature = |name: &str, noun: &str| crate::core::state::Creature {
            name: name.to_string(),
            noun: (!noun.is_empty()).then(|| noun.to_string()),
            id: "1".into(),
            status: None,
            flags: None,
        };
        // big ugly kobold: height 4 ft -> 0.8 units.
        let kobold = card_size_for(&creature("big ugly kobold", "kobold"));
        assert!((kobold.h - 0.8).abs() < 0.01, "kobold h = {}", kobold.h);
        // A boon adjective on the wire name still matches the template.
        let boon = card_size_for(&creature("dazzling big ugly kobold", "kobold"));
        assert!((boon.h - kobold.h).abs() < 0.001);
        // Unknown creature: the default card.
        let unknown = card_size_for(&creature("test dummy", "dummy"));
        assert!((unknown.h - solver::CardSize::default().h).abs() < 0.001);
        // Clamp: nothing renders below 0.55 or above 2.6 units.
        for e in crate::core::bestiary::format::shared().by_noun("rat") {
            let c = card_size_for(&creature(&e.name, "rat"));
            assert!(c.h >= 0.55 && c.h <= 2.6);
        }
    }

    /// A prone creature's card box goes short and wide, scaled by the
    /// bestiary body type (a biped flattens more than a quadruped); the
    /// standing height stays available as the sprite pixel-scale anchor.
    #[test]
    fn prone_card_box_derives_from_body_type() {
        let prone = |name: &str, noun: &str| crate::core::state::Creature {
            name: name.to_string(),
            noun: Some(noun.to_string()),
            id: "1".into(),
            status: None,
            flags: Some(crate::core::state::CreatureFlags {
                statuses: vec!["prone".to_string()],
                hostile: true,
                ..Default::default()
            }),
        };
        // Biped kobold (4 ft -> 0.8 standing): prone = 0.35x tall, 0.9x wide.
        let c = prone("big ugly kobold", "kobold");
        let standing = standing_height_for(&c);
        assert!((standing - 0.8).abs() < 0.01);
        let box_ = card_size_for(&c);
        assert!(
            (box_.h - (standing * 0.35).max(0.30)).abs() < 0.01,
            "h = {}",
            box_.h
        );
        assert!((box_.w - standing * 0.9).abs() < 0.01, "w = {}", box_.w);
        assert!(box_.h < box_.w, "prone box must be wider than tall");
        // Standing pose is unchanged by the prone plumbing.
        let up = card_size_for(&crate::core::state::Creature {
            flags: None,
            ..prone("big ugly kobold", "kobold")
        });
        assert!((up.h - standing).abs() < 0.001);
    }
}
