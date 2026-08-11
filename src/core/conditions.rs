//! Condition evaluation: the shared engine behind hotbar button states and
//! hand-widget icon states. Pure logic — no frontend imports. Callers pass
//! `now_server = local unix time + server_time_offset` (the countdown
//! convention) and, for hand item-type tests, the gameobj-data classifier.

use crate::config::{Condition, EffectCategory, HandSlot, NameMatch, VitalKind, VitalUnit};
use crate::core::game_objects::Hand;
use crate::core::gameobj_data::GameObjData;
use crate::core::state::GameState;
use crate::data::ActiveEffect;

/// A hand widget's resolved status-driven appearance for this frame.
/// Every field None = no state matched (or the state left it unset);
/// fall through to the widget's static icon settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedHand {
    pub icon: Option<crate::data::IconRef>,
    pub text: Option<String>,
    pub icon_color: Option<String>,
}

/// A status template's resolved appearance for this frame, after applying its
/// condition-driven states over the static defaults. `icon` follows the same
/// precedence a renderer wants: a matched state's icon, else the template's
/// pickable `icon_ref`, else `None` (renderer falls back to the legacy text
/// glyph / id-keyed skin sprite / built-in pictogram). `text` is the TUI
/// glyph, `color` the resolved color string.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedStatusArt {
    pub icon: Option<crate::data::IconRef>,
    pub text: Option<String>,
    pub color: Option<String>,
    /// Whether any state matched (the status is in a driven state). Callers
    /// that only track binary on/off can ignore this; it lets a renderer know
    /// a state (not just the static default) is in effect.
    pub state_matched: bool,
}

/// Resolve a status template's appearance against the game state. First
/// matching `states` entry wins (hotbar/hand-style); when none match, falls
/// back to the template's static `icon_ref`/legacy-icon/colors. `active`
/// selects the static color (active vs inactive) when no state supplies one.
pub fn resolve_status(
    template: &crate::config::IndicatorTemplateEntry,
    active: bool,
    gs: &GameState,
    now_server: i64,
    gameobj: Option<&GameObjData>,
) -> ResolvedStatusArt {
    if let Some(state) = template
        .states
        .iter()
        .find(|state| eval_condition(&state.when, gs, now_server, gameobj))
    {
        return ResolvedStatusArt {
            icon: state.icon.clone().or_else(|| template.icon_ref.clone()),
            text: state.text.clone().or_else(|| template.icon.clone()),
            color: state
                .color
                .clone()
                .or_else(|| static_status_color(template, active)),
            state_matched: true,
        };
    }
    // No condition matched: pick the static icon by active/inactive. Active
    // uses `icon_ref` (the "active/Y" icon); inactive uses `inactive_icon_ref`
    // — which defaults to None, i.e. NO image while inactive (inactive art is
    // opt-in, never a dimmed copy of the active icon).
    ResolvedStatusArt {
        icon: static_status_icon(template, active),
        text: template.icon.clone(),
        color: static_status_color(template, active),
        state_matched: false,
    }
}

/// The template's static icon for the active/inactive state. Active → the
/// `icon_ref` (Y icon); inactive → `inactive_icon_ref` (N icon), which is
/// None by default so an inactive indicator shows no image unless one was
/// explicitly chosen.
fn static_status_icon(
    template: &crate::config::IndicatorTemplateEntry,
    active: bool,
) -> Option<crate::data::IconRef> {
    if active {
        template.icon_ref.clone()
    } else {
        template.inactive_icon_ref.clone()
    }
}

/// The template's static color for the active/inactive state.
fn static_status_color(
    template: &crate::config::IndicatorTemplateEntry,
    active: bool,
) -> Option<String> {
    if active {
        template.active_color.clone()
    } else {
        template.inactive_color.clone()
    }
}

/// Resolve a hand widget's icon states against the game state; first
/// matching state wins (hotbar-style).
pub fn resolve_hand(
    data: &crate::config::HandWidgetData,
    gs: &GameState,
    now_server: i64,
    gameobj: Option<&GameObjData>,
) -> ResolvedHand {
    data.states
        .iter()
        .find(|state| eval_condition(&state.when, gs, now_server, gameobj))
        .map(|state| ResolvedHand {
            icon: state.icon.clone(),
            text: state.text.clone(),
            icon_color: state.icon_color.clone(),
        })
        .unwrap_or_default()
}

/// Evaluate one condition tree against the game state. `gameobj` powers the
/// hand-holds item-type tests; None fails those closed (every other
/// condition works without it).
pub fn eval_condition(
    cond: &Condition,
    gs: &GameState,
    now_server: i64,
    gameobj: Option<&GameObjData>,
) -> bool {
    match cond {
        Condition::All { conditions } => conditions
            .iter()
            .all(|c| eval_condition(c, gs, now_server, gameobj)),
        Condition::Any { conditions } => conditions
            .iter()
            .any(|c| eval_condition(c, gs, now_server, gameobj)),
        Condition::EffectActive {
            category,
            name,
            name_match,
        } => effect_lookup(gs, category, name, name_match)
            .is_some_and(|e| effect_is_active(e, now_server)),
        Condition::EffectInactive {
            category,
            name,
            name_match,
        } => !effect_lookup(gs, category, name, name_match)
            .is_some_and(|e| effect_is_active(e, now_server)),
        Condition::EffectTime {
            category,
            name,
            name_match,
            cmp,
            seconds,
        } => effect_lookup(gs, category, name, name_match)
            .and_then(|e| e.expires_at)
            .map(|expiry| cmp.eval(expiry - now_server, *seconds))
            .unwrap_or(false),
        Condition::RtActive => gs.roundtime_end.is_some_and(|end| end > now_server),
        Condition::CtActive => gs.casttime_end.is_some_and(|end| end > now_server),
        Condition::Indicator { id, active } => {
            indicator_value(gs, id).map(|v| v == *active).unwrap_or(false)
        }
        Condition::Vital {
            vital,
            cmp,
            value,
            unit,
        } => vital_value(gs, *vital, *unit)
            .map(|v| cmp.eval(v, *value as i64))
            .unwrap_or(false),
        Condition::Injury { area, cmp, level } => {
            // Absent = healthy = level 0; compare so `>= 2` is false when the
            // part isn't in the map.
            let current = gs.injuries.get(area).copied().unwrap_or(0);
            cmp.eval(current as i64, *level as i64)
        }
        Condition::SpellAffordable { number } => spell_affordable(gs, *number),
        Condition::TimeOfDay { phase } => {
            // `now_server` is the game clock the rest of the evaluator uses,
            // so a time-of-day rule agrees with effect expiries rather than
            // drifting against them.
            crate::core::elanthian_time::phase_at(
                now_server,
                crate::core::elanthian_time::PhaseBoundaries::default(),
            ) == *phase
        }
        Condition::HandEmpty { hand } => match hand {
            HandSlot::Right => hand_item(gs, Hand::Right).is_none(),
            HandSlot::Left => hand_item(gs, Hand::Left).is_none(),
            HandSlot::Either => {
                hand_item(gs, Hand::Right).is_none() || hand_item(gs, Hand::Left).is_none()
            }
            HandSlot::Spell => prepared_spell(gs).is_none(),
        },
        Condition::HandHolds {
            hand,
            item_type,
            name,
            name_match,
        } => hand_holds(
            gs,
            *hand,
            item_type.as_deref(),
            name.as_deref(),
            name_match,
            gameobj,
        ),
        Condition::SpellPrepared { name, name_match } => prepared_spell(gs)
            .is_some_and(|spell| {
                name.as_deref()
                    .is_none_or(|needle| name_matches(spell, needle, name_match))
            }),
    }
}

/// The held item's (name, noun) for one physical hand; None when empty.
/// The GameObjects registry is checked first (it carries the noun from the
/// hand link); the plain display string is the fallback, where the game's
/// literal "Empty" counts as empty.
fn hand_item(gs: &GameState, hand: Hand) -> Option<(&str, Option<&str>)> {
    if let Some(item) = gs.objects.hand(hand) {
        let noun = (!item.noun.is_empty()).then_some(item.noun.as_str());
        return Some((item.name.as_str(), noun));
    }
    let text = match hand {
        Hand::Left => gs.left_hand.as_deref(),
        Hand::Right => gs.right_hand.as_deref(),
    }?;
    (!text.is_empty() && !text.eq_ignore_ascii_case("empty")).then_some((text, None))
}

/// The prepared spell's name; the game's literal "None" counts as none.
fn prepared_spell(gs: &GameState) -> Option<&str> {
    let spell = gs.spell.as_deref()?;
    (!spell.is_empty() && !spell.eq_ignore_ascii_case("none")).then_some(spell)
}

fn name_matches(hay: &str, needle: &str, mode: &NameMatch) -> bool {
    let hay = hay.to_lowercase();
    let needle = needle.to_lowercase();
    match mode {
        NameMatch::Exact => hay == needle,
        NameMatch::Contains => hay.contains(&needle),
    }
}

/// HandHolds: item-type test via gameobj-data (fails closed without the
/// classifier) AND optional name match. The spell "hand" holds a spell,
/// not an item — the name test runs against the prepared spell and a type
/// test can never match there.
fn hand_holds(
    gs: &GameState,
    hand: HandSlot,
    item_type: Option<&str>,
    name: Option<&str>,
    name_match: &NameMatch,
    gameobj: Option<&GameObjData>,
) -> bool {
    let check = |h: Hand| -> bool {
        let Some((held_name, noun)) = hand_item(gs, h) else {
            return false;
        };
        if let Some(tag) = item_type {
            let Some(data) = gameobj else {
                return false;
            };
            if !data.is_type(held_name, noun.unwrap_or(""), tag) {
                return false;
            }
        }
        name.is_none_or(|needle| name_matches(held_name, needle, name_match))
    };
    match hand {
        HandSlot::Right => check(Hand::Right),
        HandSlot::Left => check(Hand::Left),
        HandSlot::Either => check(Hand::Right) || check(Hand::Left),
        HandSlot::Spell => {
            item_type.is_none()
                && prepared_spell(gs).is_some_and(|spell| {
                    name.is_none_or(|needle| name_matches(spell, needle, name_match))
                })
        }
    }
}

/// True when the bundled spell table knows this spell's static costs and
/// the character's current absolute vitals (minivitals feed) cover them.
/// Fails closed on unknown spells, formula costs, and vitals not yet seen
/// (max == 0) — this is deliberately an approximation of Lich's
/// `Spell[n].affordable?` without its feat/debuff adjustments.
fn spell_affordable(gs: &GameState, number: u16) -> bool {
    let Some(spell) = crate::core::spell_table::spell(number) else {
        return false;
    };
    if spell.dynamic_cost {
        return false;
    }
    let covers = |cost: Option<u16>, entry: &crate::core::state::VitalEntry| match cost {
        None => true,
        Some(c) => entry.max > 0 && entry.value >= c as u32,
    };
    let mv = &gs.minivitals;
    covers(spell.mana, &mv.mana)
        && covers(spell.stamina, &mv.stamina)
        && covers(spell.spirit, &mv.spirit)
}

/// Case-insensitive lookup of an effect by display name within a category.
/// `pub(crate)` for the hotbar countdown-source resolution.
pub(crate) fn effect_lookup<'a>(
    gs: &'a GameState,
    category: &EffectCategory,
    name: &str,
    name_match: &NameMatch,
) -> Option<&'a ActiveEffect> {
    let store = gs.effects.get(category.state_key())?;
    let needle = name.to_lowercase();
    store.effects.iter().find(|e| {
        let hay = e.text.to_lowercase();
        match name_match {
            NameMatch::Exact => hay == needle,
            NameMatch::Contains => hay.contains(&needle),
        }
    })
}

/// An effect entry is active unless its derived expiry has already passed.
/// Effects without a parseable expiry (e.g. "Indefinite") count as active
/// while present — the game removes them via dialog clears.
fn effect_is_active(effect: &ActiveEffect, now_server: i64) -> bool {
    effect.expires_at.map(|end| end > now_server).unwrap_or(true)
}

fn indicator_value(gs: &GameState, id: &str) -> Option<bool> {
    let s = &gs.status;
    Some(match id {
        "standing" => s.standing,
        "kneeling" => s.kneeling,
        "sitting" => s.sitting,
        "prone" => s.prone,
        "stunned" => s.stunned,
        "bleeding" => s.bleeding,
        "hidden" => s.hidden,
        "invisible" => s.invisible,
        "webbed" => s.webbed,
        "joined" => s.joined,
        "dead" => s.dead,
        _ => return None,
    })
}

/// Percent comes from the vitals bars; absolute from minivitals (GS4).
/// Absolute returns None until minivitals data has arrived (max == 0).
fn vital_value(gs: &GameState, vital: VitalKind, unit: VitalUnit) -> Option<i64> {
    match unit {
        VitalUnit::Percent => Some(match vital {
            VitalKind::Health => gs.vitals.health as i64,
            VitalKind::Mana => gs.vitals.mana as i64,
            VitalKind::Stamina => gs.vitals.stamina as i64,
            VitalKind::Spirit => gs.vitals.spirit as i64,
        }),
        VitalUnit::Absolute => {
            let entry = match vital {
                VitalKind::Health => &gs.minivitals.health,
                VitalKind::Mana => &gs.minivitals.mana,
                VitalKind::Stamina => &gs.minivitals.stamina,
                VitalKind::Spirit => &gs.minivitals.spirit,
            };
            (entry.max > 0).then_some(entry.value as i64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Cmp;

    fn injury(area: &str, cmp: Cmp, level: u8) -> Condition {
        Condition::Injury {
            area: area.to_string(),
            cmp,
            level,
        }
    }

    fn indicator_cond(id: &str, active: bool) -> Condition {
        Condition::Indicator {
            id: id.to_string(),
            active,
        }
    }

    /// CHARACTERIZATION: every indicator id the game can send, evaluated
    /// through `Condition::Indicator`. These pin the pre-refactor contract so
    /// the move to a general indicator map is provably behavior-preserving
    /// except where the change is deliberate. Several assertions below encode
    /// *defects* rather than intent; each is labelled.
    #[test]
    fn characterize_indicator_lowercase_ids_resolve() {
        let mut gs = GameState::new();
        gs.status.standing = true;
        gs.status.stunned = true;

        // Lowercase ids hit `indicator_value`'s match arms and resolve.
        assert!(eval_condition(&indicator_cond("standing", true), &gs, 0, None));
        assert!(eval_condition(&indicator_cond("stunned", true), &gs, 0, None));
        // Negation works: asking active=false on a set flag is false.
        assert!(!eval_condition(&indicator_cond("standing", false), &gs, 0, None));
        // An unset flag reads false, and active=false matches it.
        assert!(!eval_condition(&indicator_cond("kneeling", true), &gs, 0, None));
        assert!(eval_condition(&indicator_cond("kneeling", false), &gs, 0, None));
    }

    /// CHARACTERIZATION OF A DEFECT: config and presets store indicator ids in
    /// UPPERCASE (`presets.rs` uses `"STANDING"`), but `indicator_value`
    /// matches lowercase literals only and does not normalize. So every
    /// uppercase indicator condition currently resolves to `None` and is
    /// coerced to `false` by the `unwrap_or(false)` at the call site --
    /// including `active: false`, which a user would reasonably expect to be
    /// true when the flag is unset.
    ///
    /// The refactor FIXES this by normalizing case in the accessor. When that
    /// lands, these assertions flip; that flip is the intended behavior change.
    #[test]
    fn characterize_indicator_uppercase_ids_silently_false_today() {
        let mut gs = GameState::new();
        gs.status.standing = true;

        // Standing IS set, but the uppercase id never matches -> false.
        assert!(!eval_condition(&indicator_cond("STANDING", true), &gs, 0, None));
        // And the negation is ALSO false -- the tell-tale sign of `None`
        // rather than a real boolean answer.
        assert!(!eval_condition(&indicator_cond("STANDING", false), &gs, 0, None));
    }

    /// CHARACTERIZATION OF A DEFECT: `element.rs` has no match arm writing
    /// `status.joined`, so the field is permanently `false` even though the
    /// game sends `<indicator id='IconJOINED'>` and the field is serialized to
    /// remote clients. The condition path resolves the id (it is a real match
    /// arm) but always reads the stale default.
    ///
    /// The refactor FIXES this: a general map stores whatever the game sends.
    /// Group membership conditions start working, which is a prerequisite for
    /// the multi-account roster.
    #[test]
    fn characterize_joined_never_set_by_parser() {
        let gs = GameState::new();
        // The id resolves (unlike an unknown id) but the value is always false.
        assert!(!eval_condition(&indicator_cond("joined", true), &gs, 0, None));
        assert!(eval_condition(&indicator_cond("joined", false), &gs, 0, None));
    }

    /// CHARACTERIZATION OF A DEFECT: POISONED and DISEASED are shipped
    /// indicator templates (`presets.rs`) and real game indicators, but have no
    /// `StatusInfo` field, so `indicator_value` returns `None` and conditions
    /// on them are silently false in BOTH directions. They reach the dashboard
    /// widget path only.
    ///
    /// The refactor FIXES this -- a general map has no fixed arity.
    #[test]
    fn characterize_unmapped_game_indicators_are_dead_conditions() {
        let gs = GameState::new();
        for id in ["poisoned", "diseased", "POISONED", "DISEASED"] {
            assert!(
                !eval_condition(&indicator_cond(id, true), &gs, 0, None),
                "{id} active=true should be false pre-refactor"
            );
            assert!(
                !eval_condition(&indicator_cond(id, false), &gs, 0, None),
                "{id} active=false should ALSO be false pre-refactor (None, not a boolean)"
            );
        }
    }

    /// CHARACTERIZATION: a genuinely unknown id must stay false in both
    /// directions even after the refactor. A general map must NOT invent
    /// `true` for ids the game never sent. This assertion is expected to
    /// survive the refactor unchanged.
    #[test]
    fn characterize_unknown_indicator_id_stays_false() {
        let gs = GameState::new();
        assert!(!eval_condition(&indicator_cond("not_a_real_id", true), &gs, 0, None));
        assert!(!eval_condition(&indicator_cond("not_a_real_id", false), &gs, 0, None));
    }

    /// CHARACTERIZATION: the full set of ids that DO resolve today, asserted
    /// one by one against a fully-set status. This is the regression net for
    /// the accessor rewrite -- if the map loses any of these, this fails.
    #[test]
    fn characterize_all_eleven_statusinfo_fields_readable() {
        let mut gs = GameState::new();
        gs.status.standing = true;
        gs.status.kneeling = true;
        gs.status.sitting = true;
        gs.status.prone = true;
        gs.status.stunned = true;
        gs.status.bleeding = true;
        gs.status.hidden = true;
        gs.status.invisible = true;
        gs.status.webbed = true;
        gs.status.joined = true;
        gs.status.dead = true;

        for id in [
            "standing",
            "kneeling",
            "sitting",
            "prone",
            "stunned",
            "bleeding",
            "hidden",
            "invisible",
            "webbed",
            "joined",
            "dead",
        ] {
            assert!(
                eval_condition(&indicator_cond(id, true), &gs, 0, None),
                "indicator {id} should read true when its field is set"
            );
        }
    }

    #[test]
    fn injury_absent_part_is_healthy() {
        let gs = GameState::new();
        // No injuries: `>= 1` is false, `< 1` is true (level 0).
        assert!(!eval_condition(&injury("neck", Cmp::Ge, 1), &gs, 0, None));
        assert!(eval_condition(&injury("neck", Cmp::Lt, 1), &gs, 0, None));
    }

    #[test]
    fn injury_threshold_matches_rank() {
        let mut gs = GameState::new();
        gs.injuries.insert("neck".to_string(), 2);
        // The motivating example: a rank-2 wound on the neck.
        assert!(eval_condition(&injury("neck", Cmp::Ge, 2), &gs, 0, None));
        assert!(!eval_condition(&injury("neck", Cmp::Ge, 3), &gs, 0, None));
        // A different part is unaffected.
        assert!(!eval_condition(&injury("head", Cmp::Ge, 1), &gs, 0, None));
    }

    #[test]
    fn injury_scar_levels_compare_above_wounds() {
        let mut gs = GameState::new();
        gs.injuries.insert("leftArm".to_string(), 5); // Scar2
        assert!(eval_condition(&injury("leftArm", Cmp::Ge, 4), &gs, 0, None));
        assert!(eval_condition(&injury("leftArm", Cmp::Gt, 3), &gs, 0, None));
    }

    fn template_with_states(states: Vec<crate::config::StatusIconState>) -> crate::config::IndicatorTemplateEntry {
        crate::config::IndicatorTemplateEntry {
            id: "BLEEDING".to_string(),
            icon: Some("*".to_string()),
            icon_ref: Some(crate::data::IconRef::Image {
                path: "statusicons/bleeding.png".to_string(),
            }),
            active_color: Some("#ff0000".to_string()),
            inactive_color: Some("#555555".to_string()),
            states,
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn resolve_status_no_states_uses_static_defaults() {
        let gs = GameState::new();
        let t = template_with_states(vec![]);
        let active = resolve_status(&t, true, &gs, 0, None);
        assert_eq!(active.color.as_deref(), Some("#ff0000"));
        assert_eq!(active.text.as_deref(), Some("*"));
        assert!(!active.state_matched);
        assert!(matches!(active.icon, Some(crate::data::IconRef::Image { .. })));
        let inactive = resolve_status(&t, false, &gs, 0, None);
        assert_eq!(inactive.color.as_deref(), Some("#555555"));
    }

    #[test]
    fn resolve_status_active_uses_icon_ref_inactive_uses_inactive_icon_ref() {
        let gs = GameState::new();
        // No inactive icon set → active shows icon_ref, inactive shows NOTHING
        // (inactive art is opt-in, never a dimmed copy of the active icon).
        let t = template_with_states(vec![]);
        assert!(
            matches!(
                resolve_status(&t, true, &gs, 0, None).icon,
                Some(crate::data::IconRef::Image { .. })
            ),
            "active should use icon_ref"
        );
        assert!(
            resolve_status(&t, false, &gs, 0, None).icon.is_none(),
            "inactive should be blank when no inactive_icon_ref is set"
        );

        // With an inactive icon configured, inactive shows THAT image.
        let mut t2 = template_with_states(vec![]);
        t2.inactive_icon_ref = Some(crate::data::IconRef::Image {
            path: "statusicons/bleeding_off.png".to_string(),
        });
        let inactive = resolve_status(&t2, false, &gs, 0, None);
        assert!(
            matches!(
                inactive.icon,
                Some(crate::data::IconRef::Image { ref path }) if path.ends_with("bleeding_off.png")
            ),
            "inactive should use the configured inactive_icon_ref, got {:?}",
            inactive.icon
        );
    }

    #[test]
    fn resolve_status_first_matching_state_wins() {
        let mut gs = GameState::new();
        gs.injuries.insert("neck".to_string(), 2);
        // rank>=2 state first; a rank>=1 state second. Both match at rank 2,
        // first wins.
        let t = template_with_states(vec![
            crate::config::StatusIconState {
                when: injury("neck", Cmp::Ge, 2),
                icon: Some(crate::data::IconRef::Image {
                    path: "statusicons/neck2.png".to_string(),
                }),
                text: Some("N2".to_string()),
                color: Some("#ff00ff".to_string()),
            },
            crate::config::StatusIconState {
                when: injury("neck", Cmp::Ge, 1),
                icon: Some(crate::data::IconRef::Image {
                    path: "statusicons/neck1.png".to_string(),
                }),
                text: None,
                color: None,
            },
        ]);
        let r = resolve_status(&t, true, &gs, 0, None);
        assert!(r.state_matched);
        assert_eq!(r.color.as_deref(), Some("#ff00ff"));
        assert_eq!(r.text.as_deref(), Some("N2"));
        match r.icon {
            Some(crate::data::IconRef::Image { path }) => assert_eq!(path, "statusicons/neck2.png"),
            other => panic!("expected neck2 image, got {other:?}"),
        }
    }

    #[test]
    fn resolve_status_state_inherits_template_defaults_when_unset() {
        let mut gs = GameState::new();
        gs.injuries.insert("neck".to_string(), 1);
        // Matching state leaves icon/color None → inherit the template's.
        let t = template_with_states(vec![crate::config::StatusIconState {
            when: injury("neck", Cmp::Ge, 1),
            icon: None,
            text: None,
            color: None,
        }]);
        let r = resolve_status(&t, true, &gs, 0, None);
        assert!(r.state_matched);
        assert_eq!(r.color.as_deref(), Some("#ff0000")); // template active_color
        assert!(matches!(r.icon, Some(crate::data::IconRef::Image { .. }))); // template icon_ref
    }

    #[test]
    fn injury_area_list_is_the_parser_clear_set() {
        // Guards against the editor dropdown drifting from the ids the feed
        // actually emits (parser's full-clear body-part list).
        assert!(crate::config::INJURY_AREAS.contains(&"neck"));
        assert!(crate::config::INJURY_AREAS.contains(&"nsys"));
        assert_eq!(crate::config::INJURY_AREAS.len(), 14);
    }
}

#[cfg(test)]
mod time_of_day_tests {
    use super::*;
    use crate::core::elanthian_time::DayPhase;

    /// A `TimeOfDay` condition matches the phase of the in-game clock, and
    /// only that phase — evaluated through the REAL shared evaluator, so
    /// hotbars, the injury doll, and room art all get it identically.
    #[test]
    fn time_of_day_matches_only_its_phase() {
        let gs = GameState::new();
        // 2026-08-10 20:44 UTC = 16:44 Eastern = Day under the defaults.
        let noonish = 1_786_394_661_i64;

        assert!(eval_condition(
            &Condition::TimeOfDay { phase: DayPhase::Day },
            &gs,
            noonish,
            None
        ));
        for other in [DayPhase::Dawn, DayPhase::Dusk, DayPhase::Night] {
            assert!(
                !eval_condition(
                    &Condition::TimeOfDay { phase: other },
                    &gs,
                    noonish,
                    None
                ),
                "{other:?} must not match a Day timestamp"
            );
        }
    }

    /// It composes with All/Any like every other leaf, which is the whole
    /// reason for reusing the shared condition system.
    #[test]
    fn time_of_day_composes_with_other_conditions() {
        let gs = GameState::new();
        let night = 1_786_394_661_i64 - 10 * 3600; // 06:44 Eastern -> Dawn
        let dawn = Condition::TimeOfDay { phase: DayPhase::Dawn };

        assert!(eval_condition(&dawn, &gs, night, None), "fixture is Dawn");
        assert!(eval_condition(
            &Condition::Any {
                conditions: vec![
                    Condition::TimeOfDay { phase: DayPhase::Night },
                    dawn.clone(),
                ],
            },
            &gs,
            night,
            None
        ));
        assert!(!eval_condition(
            &Condition::All {
                conditions: vec![
                    Condition::TimeOfDay { phase: DayPhase::Night },
                    dawn,
                ],
            },
            &gs,
            night,
            None
        ));
    }
}
