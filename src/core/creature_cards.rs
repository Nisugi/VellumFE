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
        let lookup = |anchors: &std::collections::HashMap<String, [f32; 2]>| {
            anchors
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, a)| *a)
        };
        self.variant
            .and_then(|i| lookup(&self.skin.variants[i].skin.anchors))
            .or_else(|| lookup(&self.skin.anchors))
            .or_else(|| skins::default_creature_anchor(name))
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
        crate::core::conditions::eval_condition_for_creature(&v.when, gs, now_server, gameobj, flags)
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
pub fn base_candidates(skin: &CreatureCardSkin, noun: Option<&str>, family: Option<&str>) -> Vec<String> {
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

/// Whether a room creature belongs on the creature field. The targets-list
/// gate (hostile + valid_target) EXCEPT death: a corpse keeps its card,
/// tinted, until looting drops it from the roster — its square frees then.
pub fn field_member(c: &crate::core::state::Creature, excluded_nouns: &[String]) -> bool {
    let hostile = c.flags.as_ref().is_some_and(|f| f.hostile);
    hostile && (c.is_valid_target(excluded_nouns) || c.is_dead())
}

/// Card size for one creature: bosses get a visibly bigger card. Real
/// per-family sizes arrive with art (the sprite's aspect); these are the
/// placeholder-card dimensions.
pub fn card_size_for(c: &crate::core::state::Creature) -> solver::CardSize {
    if c.flags.as_ref().is_some_and(|f| f.is_boss()) {
        solver::CardSize { w: 0.78, h: 1.52 }
    } else {
        solver::CardSize::default()
    }
}

/// Event-driven roster sync: diff the field's units against the room's
/// creatures. Arrivals place (permanently), departures free their square.
/// Cheap no-op while the roster generation is unchanged, so calling it
/// once per frame costs one comparison.
pub fn sync_field(
    field: &mut solver::CreatureField,
    synced_gen: &mut u64,
    gs: &GameState,
    excluded_nouns: &[String],
) {
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
        if field.unit_of(&c.id).is_none() {
            field.arrive(&c.id, card_size_for(c));
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
    use crate::config::skins::{AnimateKind, OverlaySpace, OverlaySource, SkinManifest};

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
        toml::from_str::<SkinManifest>(MANIFEST).unwrap().creature_card
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
        let r = resolve(&skin, &flags(&[("webbed", "1"), ("stunned", "1"), ("prone", "1"), ("hovering", "1")]));
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
        assert_eq!(r.base_override(), None, "no base: cascade's ground pose is kept");
        let lift = r.lift().unwrap();
        assert_eq!(lift.offset_y, -0.22);
        assert_eq!(lift.shadow_scale, 0.55);
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
        assert_eq!(r.part_overlay("head", 4), None, "scar1 authored but must not resolve");
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
        let dir = std::env::temp_dir().join(format!(
            "vellum_cc_test_{}",
            std::process::id()
        ));
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
            "abdomen", "back", "chest", "head", "leftArm", "leftEye", "leftFoot", "leftHand",
            "leftLeg", "neck", "nerves", "rightArm", "rightEye", "rightFoot", "rightHand",
            "rightLeg",
        ] {
            assert!(
                canonical_part(part).is_some(),
                "CreatureBar part {part} must map to a doll part"
            );
        }
    }
}
