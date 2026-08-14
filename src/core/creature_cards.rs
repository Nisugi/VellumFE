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
