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
        Condition::SpellAffordable { number } => spell_affordable(gs, *number),
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
