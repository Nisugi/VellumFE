//! Hotbar button resolution: evaluate structured conditions against
//! GameState and produce per-button display state for the frontends.
//!
//! Pure logic — no frontend imports. Both TUI and GUI call `resolve_bar`
//! each frame with `now_server = local unix time + server_time_offset`
//! (the same convention as the countdown widget).

use crate::config::{
    EffectCategory, HandSlot, HotbarCondition, HotbarCountdownSource, HotbarDef, NameMatch,
    VitalKind, VitalUnit,
};
use crate::core::game_objects::Hand;
use crate::core::gameobj_data::GameObjData;
use crate::core::state::GameState;
use crate::data::ActiveEffect;

/// A button after state resolution: what the frontends actually draw.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedHotbarButton {
    pub id: String,
    /// Label with any matching state's override applied.
    pub label: String,
    pub command: String,
    pub tooltip: Option<String>,
    /// Raw hotkey string for tooltip/editor display (e.g. "alt+h").
    pub hotkey: Option<String>,
    /// Hex color strings; frontends parse with their own color helpers and
    /// fall back to widget/theme colors when None.
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub dim: bool,
    /// Seconds remaining for the countdown overlay; None or <= 0 means
    /// no overlay.
    pub countdown_secs: Option<i64>,
    /// Icon to draw (GUI only; TUI ignores): the active state's override,
    /// else the button's base icon.
    pub icon: Option<crate::config::HotbarIcon>,
    /// How the GUI draws the face (text / icon / icon+label).
    pub icon_mode: crate::config::IconMode,
}

/// Resolve every button on a bar against the current game state.
/// `gameobj` powers the hand-holds item-type tests; None fails those
/// closed (every other condition works without it).
pub fn resolve_bar(
    bar: &HotbarDef,
    gs: &GameState,
    now_server: i64,
    gameobj: Option<&GameObjData>,
) -> Vec<ResolvedHotbarButton> {
    bar.buttons
        .iter()
        .map(|button| {
            let matched = button
                .states
                .iter()
                .find(|state| eval_condition(&state.when, gs, now_server, gameobj));

            let default_style = button.default_style.as_ref();
            let style = matched.map(|s| &s.style);

            let pick =
                |f: fn(&crate::config::HotbarStyle) -> Option<String>| -> Option<String> {
                    style.and_then(f).or_else(|| default_style.and_then(f))
                };

            ResolvedHotbarButton {
                id: button.id.clone(),
                label: pick(|s| s.label.clone()).unwrap_or_else(|| button.label.clone()),
                // Active state's command override wins (literal text; Lich
                // evaluates `;eq ...` commands itself).
                command: matched
                    .and_then(|s| s.command.clone())
                    .unwrap_or_else(|| button.command.clone()),
                tooltip: button.tooltip.clone(),
                hotkey: button.hotkey.clone(),
                fg: pick(|s| s.fg.clone()),
                bg: pick(|s| s.bg.clone()),
                dim: style.map(|s| s.dim).unwrap_or(false)
                    || (style.is_none() && default_style.map(|s| s.dim).unwrap_or(false)),
                // Active state's countdown wins (barbar-style per-state
                // timers); fall back to the button-level source.
                countdown_secs: matched
                    .and_then(|s| s.countdown.as_ref())
                    .or(button.countdown.as_ref())
                    .and_then(|src| countdown_secs(src, gs, now_server)),
                // State icon override, else default_style's, else the button's.
                icon: style
                    .and_then(|s| s.icon.clone())
                    .or_else(|| default_style.and_then(|s| s.icon.clone()))
                    .or_else(|| button.icon.clone()),
                icon_mode: button.icon_mode,
            }
        })
        .collect()
}

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

/// Evaluate one condition tree against the game state.
pub fn eval_condition(
    cond: &HotbarCondition,
    gs: &GameState,
    now_server: i64,
    gameobj: Option<&GameObjData>,
) -> bool {
    match cond {
        HotbarCondition::All { conditions } => conditions
            .iter()
            .all(|c| eval_condition(c, gs, now_server, gameobj)),
        HotbarCondition::Any { conditions } => conditions
            .iter()
            .any(|c| eval_condition(c, gs, now_server, gameobj)),
        HotbarCondition::EffectActive {
            category,
            name,
            name_match,
        } => effect_lookup(gs, category, name, name_match)
            .is_some_and(|e| effect_is_active(e, now_server)),
        HotbarCondition::EffectInactive {
            category,
            name,
            name_match,
        } => !effect_lookup(gs, category, name, name_match)
            .is_some_and(|e| effect_is_active(e, now_server)),
        HotbarCondition::EffectTime {
            category,
            name,
            name_match,
            cmp,
            seconds,
        } => effect_lookup(gs, category, name, name_match)
            .and_then(|e| e.expires_at)
            .map(|expiry| cmp.eval(expiry - now_server, *seconds))
            .unwrap_or(false),
        HotbarCondition::RtActive => gs.roundtime_end.is_some_and(|end| end > now_server),
        HotbarCondition::CtActive => gs.casttime_end.is_some_and(|end| end > now_server),
        HotbarCondition::Indicator { id, active } => {
            indicator_value(gs, id).map(|v| v == *active).unwrap_or(false)
        }
        HotbarCondition::Vital {
            vital,
            cmp,
            value,
            unit,
        } => vital_value(gs, *vital, *unit)
            .map(|v| cmp.eval(v, *value as i64))
            .unwrap_or(false),
        HotbarCondition::SpellAffordable { number } => spell_affordable(gs, *number),
        HotbarCondition::HandEmpty { hand } => match hand {
            HandSlot::Right => hand_item(gs, Hand::Right).is_none(),
            HandSlot::Left => hand_item(gs, Hand::Left).is_none(),
            HandSlot::Either => {
                hand_item(gs, Hand::Right).is_none() || hand_item(gs, Hand::Left).is_none()
            }
            HandSlot::Spell => prepared_spell(gs).is_none(),
        },
        HotbarCondition::HandHolds {
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
        HotbarCondition::SpellPrepared { name, name_match } => prepared_spell(gs)
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

/// Seconds remaining for a countdown source; None when idle/absent.
fn countdown_secs(
    src: &HotbarCountdownSource,
    gs: &GameState,
    now_server: i64,
) -> Option<i64> {
    let end = match src {
        HotbarCountdownSource::Effect {
            category,
            name,
            name_match,
        } => effect_lookup(gs, category, name, name_match).and_then(|e| e.expires_at)?,
        HotbarCountdownSource::Roundtime => gs.roundtime_end?,
        HotbarCountdownSource::Casttime => gs.casttime_end?,
    };
    let remaining = end - now_server;
    (remaining > 0).then_some(remaining)
}

/// Case-insensitive lookup of an effect by display name within a category.
fn effect_lookup<'a>(
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
    use crate::config::{HotbarButton, HotbarButtonState, HotbarCmp, HotbarStyle};
    use crate::data::ActiveEffectsContent;

    const NOW: i64 = 1_000_000;

    fn gs_with_effect(category: &str, text: &str, expires_at: Option<i64>) -> GameState {
        let mut gs = GameState::new();
        gs.effects.insert(
            category.to_string(),
            ActiveEffectsContent {
                category: category.to_string(),
                effects: vec![ActiveEffect {
                    id: "509".to_string(),
                    text: text.to_string(),
                    value: 90,
                    time: "00:10:00".to_string(),
                    expires_at,
                    bar_color: None,
                    text_color: None,
                }],
                generation: 1,
            },
        );
        gs
    }

    fn effect_active(category: EffectCategory, name: &str, m: NameMatch) -> HotbarCondition {
        HotbarCondition::EffectActive {
            category,
            name: name.to_string(),
            name_match: m,
        }
    }

    #[test]
    fn effect_active_exact_and_contains_case_insensitive() {
        let gs = gs_with_effect("Buffs", "Strength of the Bull", Some(NOW + 600));
        assert!(eval_condition(
            &effect_active(EffectCategory::Buffs, "strength of the bull", NameMatch::Exact),
            &gs,
            NOW,
            None
        ));
        assert!(eval_condition(
            &effect_active(EffectCategory::Buffs, "BULL", NameMatch::Contains),
            &gs,
            NOW,
            None
        ));
        assert!(!eval_condition(
            &effect_active(EffectCategory::Buffs, "Bull", NameMatch::Exact),
            &gs,
            NOW,
            None
        ));
        // Wrong category
        assert!(!eval_condition(
            &effect_active(EffectCategory::Cooldowns, "Strength of the Bull", NameMatch::Exact),
            &gs,
            NOW,
            None
        ));
    }

    #[test]
    fn expired_effect_counts_as_inactive() {
        let gs = gs_with_effect("Buffs", "Song of Luck", Some(NOW - 5));
        let cond = effect_active(EffectCategory::Buffs, "Song of Luck", NameMatch::Exact);
        assert!(!eval_condition(&cond, &gs, NOW, None));
        // ...and effect_inactive is its negation
        assert!(eval_condition(
            &HotbarCondition::EffectInactive {
                category: EffectCategory::Buffs,
                name: "Song of Luck".to_string(),
                name_match: NameMatch::Exact,
            },
            &gs,
            NOW,
            None
        ));
    }

    #[test]
    fn indefinite_effect_counts_as_active_while_present() {
        let gs = gs_with_effect("Buffs", "Prestidigitation", None);
        assert!(eval_condition(
            &effect_active(EffectCategory::Buffs, "Prestidigitation", NameMatch::Exact),
            &gs,
            NOW,
            None
        ));
    }

    #[test]
    fn effect_time_compares_remaining_seconds() {
        let gs = gs_with_effect("Cooldowns", "Shadow Mastery", Some(NOW + 30));
        let cond = |cmp: HotbarCmp, seconds: i64| HotbarCondition::EffectTime {
            category: EffectCategory::Cooldowns,
            name: "Shadow Mastery".to_string(),
            name_match: NameMatch::Exact,
            cmp,
            seconds,
        };
        assert!(eval_condition(&cond(HotbarCmp::Lt, 60), &gs, NOW, None));
        assert!(!eval_condition(&cond(HotbarCmp::Gt, 60), &gs, NOW, None));
        // Missing effect -> false
        let empty = GameState::new();
        assert!(!eval_condition(&cond(HotbarCmp::Lt, 60), &empty, NOW, None));
        // Effect without expiry -> false
        let indef = gs_with_effect("Cooldowns", "Shadow Mastery", None);
        assert!(!eval_condition(&cond(HotbarCmp::Lt, 60), &indef, NOW, None));
    }

    #[test]
    fn rt_ct_active() {
        let mut gs = GameState::new();
        assert!(!eval_condition(&HotbarCondition::RtActive, &gs, NOW, None));
        gs.roundtime_end = Some(NOW + 5);
        assert!(eval_condition(&HotbarCondition::RtActive, &gs, NOW, None));
        gs.roundtime_end = Some(NOW - 1);
        assert!(!eval_condition(&HotbarCondition::RtActive, &gs, NOW, None));

        gs.casttime_end = Some(NOW + 3);
        assert!(eval_condition(&HotbarCondition::CtActive, &gs, NOW, None));
    }

    #[test]
    fn indicator_and_inverted() {
        let mut gs = GameState::new();
        gs.status.hidden = true;
        let hidden = HotbarCondition::Indicator {
            id: "hidden".to_string(),
            active: true,
        };
        let not_hidden = HotbarCondition::Indicator {
            id: "hidden".to_string(),
            active: false,
        };
        assert!(eval_condition(&hidden, &gs, NOW, None));
        assert!(!eval_condition(&not_hidden, &gs, NOW, None));
        // Unknown indicator id -> false either way
        let bogus = HotbarCondition::Indicator {
            id: "flying".to_string(),
            active: true,
        };
        assert!(!eval_condition(&bogus, &gs, NOW, None));
    }

    #[test]
    fn vitals_percent_and_absolute() {
        let mut gs = GameState::new();
        gs.vitals.stamina = 15;
        gs.minivitals.update_vital("mana", 8, 100, "mana 8/100".to_string());

        let low_stamina = HotbarCondition::Vital {
            vital: VitalKind::Stamina,
            cmp: HotbarCmp::Lt,
            value: 20,
            unit: VitalUnit::Percent,
        };
        assert!(eval_condition(&low_stamina, &gs, NOW, None));

        let low_mana_abs = HotbarCondition::Vital {
            vital: VitalKind::Mana,
            cmp: HotbarCmp::Lt,
            value: 9,
            unit: VitalUnit::Absolute,
        };
        assert!(eval_condition(&low_mana_abs, &gs, NOW, None));

        // Absolute with no minivitals data yet -> false
        let no_data = GameState::new();
        assert!(!eval_condition(&low_mana_abs, &no_data, NOW, None));
    }

    #[test]
    fn all_any_nesting() {
        let mut gs = GameState::new();
        gs.status.stunned = true;
        gs.roundtime_end = Some(NOW + 5);

        let cond = HotbarCondition::Any {
            conditions: vec![
                HotbarCondition::CtActive,
                HotbarCondition::All {
                    conditions: vec![
                        HotbarCondition::RtActive,
                        HotbarCondition::Indicator {
                            id: "stunned".to_string(),
                            active: true,
                        },
                    ],
                },
            ],
        };
        assert!(eval_condition(&cond, &gs, NOW, None));
        gs.status.stunned = false;
        assert!(!eval_condition(&cond, &gs, NOW, None));
    }

    fn button_with_states(states: Vec<HotbarButtonState>) -> HotbarButton {
        HotbarButton {
            id: "b1".to_string(),
            label: "Base".to_string(),
            command: "look".to_string(),
            states,
            default_style: Some(HotbarStyle {
                fg: Some("#default".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn first_matching_state_wins_and_style_falls_through() {
        let mut gs = GameState::new();
        gs.roundtime_end = Some(NOW + 5);
        gs.status.hidden = true;

        let bar = HotbarDef {
            name: "test".to_string(),
            title: None,
            icon_size: None,
            buttons: vec![button_with_states(vec![
                HotbarButtonState {
                    when: HotbarCondition::RtActive,
                    style: HotbarStyle {
                        label: Some("InRT".to_string()),
                        // fg unset: falls through to default_style fg
                        dim: true,
                        ..Default::default()
                    },
                    countdown: None,
                    command: None,
                },
                HotbarButtonState {
                    when: HotbarCondition::Indicator {
                        id: "hidden".to_string(),
                        active: true,
                    },
                    style: HotbarStyle {
                        label: Some("Hidden".to_string()),
                        ..Default::default()
                    },
                    countdown: None,
                    command: None,
                },
            ])],
        };

        let resolved = resolve_bar(&bar, &gs, NOW, None);
        assert_eq!(resolved.len(), 1);
        // Both states match; the first (RT) wins
        assert_eq!(resolved[0].label, "InRT");
        assert!(resolved[0].dim);
        // fg not set on the state -> falls through to default_style
        assert_eq!(resolved[0].fg.as_deref(), Some("#default"));

        // No state matches -> base label + default style
        let idle = GameState::new();
        let resolved = resolve_bar(&bar, &idle, NOW, None);
        assert_eq!(resolved[0].label, "Base");
        assert!(!resolved[0].dim);
        assert_eq!(resolved[0].fg.as_deref(), Some("#default"));
    }

    #[test]
    fn state_icon_overrides_button_icon_and_falls_back() {
        use crate::config::{HotbarIcon, IconMode};

        let icon = |cell: u32| HotbarIcon {
            sheet: "sheet".to_string(),
            cell,
            ..Default::default()
        };
        let mut button = button_with_states(vec![HotbarButtonState {
            when: HotbarCondition::RtActive,
            style: HotbarStyle {
                icon: Some(icon(9)),
                ..Default::default()
            },
            countdown: None,
            command: None,
        }]);
        button.icon = Some(icon(1));
        button.icon_mode = IconMode::Icon;
        let bar = HotbarDef {
            name: "t".to_string(),
            title: None,
            icon_size: None,
            buttons: vec![button],
        };

        // State active: its icon wins.
        let mut gs = GameState::new();
        gs.roundtime_end = Some(NOW + 5);
        let resolved = resolve_bar(&bar, &gs, NOW, None);
        assert_eq!(resolved[0].icon.as_ref().unwrap().cell, 9);
        assert_eq!(resolved[0].icon_mode, IconMode::Icon);

        // Idle: falls back to the button's base icon.
        let idle = GameState::new();
        let resolved = resolve_bar(&bar, &idle, NOW, None);
        assert_eq!(resolved[0].icon.as_ref().unwrap().cell, 1);
    }

    #[test]
    fn state_countdown_overrides_button_countdown() {
        let mut button = button_with_states(vec![HotbarButtonState {
            when: HotbarCondition::RtActive,
            style: HotbarStyle::default(),
            countdown: Some(HotbarCountdownSource::Roundtime),
            command: None,
        }]);
        button.countdown = Some(HotbarCountdownSource::Casttime);
        let bar = HotbarDef {
            name: "t".to_string(),
            title: None,
            icon_size: None,
            buttons: vec![button],
        };

        let mut gs = GameState::new();
        gs.roundtime_end = Some(NOW + 7);
        gs.casttime_end = Some(NOW + 30);

        // State active: per-state roundtime source wins over casttime.
        let resolved = resolve_bar(&bar, &gs, NOW, None);
        assert_eq!(resolved[0].countdown_secs, Some(7));

        // State inactive: button-level casttime source applies.
        let mut idle = GameState::new();
        idle.casttime_end = Some(NOW + 30);
        let resolved = resolve_bar(&bar, &idle, NOW, None);
        assert_eq!(resolved[0].countdown_secs, Some(30));
    }

    #[test]
    fn spell_affordable_checks_static_costs_and_fails_closed() {
        let cond = HotbarCondition::SpellAffordable { number: 101 }; // 1 mana

        // No vitals data yet (max == 0): fail closed.
        let mut gs = GameState::new();
        assert!(!eval_condition(&cond, &gs, NOW, None));

        // Enough mana: affordable.
        gs.minivitals.update_vital("mana", 8, 54, "mana 8/54".to_string());
        assert!(eval_condition(&cond, &gs, NOW, None));

        // Not enough mana.
        gs.minivitals.update_vital("mana", 0, 54, "mana 0/54".to_string());
        assert!(!eval_condition(&cond, &gs, NOW, None));

        // Unknown spell number: fail closed.
        let unknown = HotbarCondition::SpellAffordable { number: 9999 };
        assert!(!eval_condition(&unknown, &gs, NOW, None));

        // Formula-cost spell (Song of Luck): fail closed even with mana.
        gs.minivitals.update_vital("mana", 999, 999, String::new());
        let dynamic = HotbarCondition::SpellAffordable { number: 1006 };
        assert!(!eval_condition(&dynamic, &gs, NOW, None));
    }

    #[test]
    fn state_command_overrides_button_command() {
        let button = button_with_states(vec![HotbarButtonState {
            when: HotbarCondition::RtActive,
            style: HotbarStyle::default(),
            countdown: None,
            command: Some(";eq Spell[906].force_incant".to_string()),
        }]);
        let bar = HotbarDef {
            name: "t".to_string(),
            title: None,
            icon_size: None,
            buttons: vec![button],
        };

        // State active: its command replaces the button's.
        let mut gs = GameState::new();
        gs.roundtime_end = Some(NOW + 5);
        let resolved = resolve_bar(&bar, &gs, NOW, None);
        assert_eq!(resolved[0].command, ";eq Spell[906].force_incant");

        // Idle: button command.
        let resolved = resolve_bar(&bar, &GameState::new(), NOW, None);
        assert_eq!(resolved[0].command, "look");
    }

    #[test]
    fn countdown_from_each_source() {
        let mut gs = gs_with_effect("Cooldowns", "Shadow Mastery", Some(NOW + 42));
        gs.roundtime_end = Some(NOW + 7);
        gs.casttime_end = Some(NOW - 1); // already elapsed

        let mk = |countdown: HotbarCountdownSource| HotbarDef {
            name: "t".to_string(),
            title: None,
            icon_size: None,
            buttons: vec![HotbarButton {
                id: "x".to_string(),
                label: "X".to_string(),
                command: "x".to_string(),
                countdown: Some(countdown),
                ..Default::default()
            }],
        };

        let effect = resolve_bar(
            &mk(HotbarCountdownSource::Effect {
                category: EffectCategory::Cooldowns,
                name: "shadow".to_string(),
                name_match: NameMatch::Contains,
            }),
            &gs,
            NOW,
            None,
        );
        assert_eq!(effect[0].countdown_secs, Some(42));

        let rt = resolve_bar(&mk(HotbarCountdownSource::Roundtime), &gs, NOW, None);
        assert_eq!(rt[0].countdown_secs, Some(7));

        // Elapsed casttime -> no overlay
        let ct = resolve_bar(&mk(HotbarCountdownSource::Casttime), &gs, NOW, None);
        assert_eq!(ct[0].countdown_secs, None);
    }

    // ==================== Hand conditions ====================

    const GAMEOBJ_FIXTURE: &str = r#"<?xml version="1.0"?>
<data>
  <type name="weapon">
    <noun>^(sword|axe|falchion)$</noun>
  </type>
  <type name="armor">
    <noun>^(shield|buckler)$</noun>
  </type>
</data>"#;

    fn gs_with_hands(right: Option<(&str, &str)>, left: Option<(&str, &str)>) -> GameState {
        let mut gs = GameState::new();
        let item = |(name, noun): (&str, &str)| {
            crate::core::game_objects::GameItem::new("1234", noun, name)
        };
        gs.objects.set_hands(left.map(item), right.map(item));
        gs.right_hand = right.map(|(name, _)| name.to_string());
        gs.left_hand = left.map(|(name, _)| name.to_string());
        gs
    }

    #[test]
    fn hand_empty_per_slot_and_literal_empty() {
        let gs = gs_with_hands(Some(("a rusty sword", "sword")), None);
        let right = HotbarCondition::HandEmpty { hand: HandSlot::Right };
        let left = HotbarCondition::HandEmpty { hand: HandSlot::Left };
        let either = HotbarCondition::HandEmpty { hand: HandSlot::Either };
        assert!(!eval_condition(&right, &gs, NOW, None));
        assert!(eval_condition(&left, &gs, NOW, None));
        assert!(eval_condition(&either, &gs, NOW, None));

        // The game's literal "Empty" (string feed, no registry entry)
        // counts as empty.
        let mut gs = GameState::new();
        gs.right_hand = Some("Empty".to_string());
        assert!(eval_condition(&right, &gs, NOW, None));

        // Spell slot: "None" counts as nothing prepared.
        let spell_empty = HotbarCondition::HandEmpty { hand: HandSlot::Spell };
        assert!(eval_condition(&spell_empty, &gs, NOW, None));
        gs.spell = Some("Minor Shock".to_string());
        assert!(!eval_condition(&spell_empty, &gs, NOW, None));
    }

    #[test]
    fn hand_holds_type_and_name_tests() {
        let data = crate::core::gameobj_data::GameObjData::parse(GAMEOBJ_FIXTURE);
        let gs = gs_with_hands(
            Some(("a rusty sword", "sword")),
            Some(("a tower shield", "shield")),
        );

        let holds_weapon = |hand| HotbarCondition::HandHolds {
            hand,
            item_type: Some("weapon".to_string()),
            name: None,
            name_match: NameMatch::Contains,
        };
        assert!(eval_condition(&holds_weapon(HandSlot::Right), &gs, NOW, Some(&data)));
        assert!(!eval_condition(&holds_weapon(HandSlot::Left), &gs, NOW, Some(&data)));
        assert!(eval_condition(&holds_weapon(HandSlot::Either), &gs, NOW, Some(&data)));
        // Type tests fail closed without the classifier.
        assert!(!eval_condition(&holds_weapon(HandSlot::Right), &gs, NOW, None));

        // "Shield" via armor type + name match (no shield type exists).
        let holds_shield = HotbarCondition::HandHolds {
            hand: HandSlot::Left,
            item_type: Some("armor".to_string()),
            name: Some("shield".to_string()),
            name_match: NameMatch::Contains,
        };
        assert!(eval_condition(&holds_shield, &gs, NOW, Some(&data)));

        // AND semantics: right hand is a weapon but not named shield.
        let sword_named_shield = HotbarCondition::HandHolds {
            hand: HandSlot::Right,
            item_type: Some("weapon".to_string()),
            name: Some("shield".to_string()),
            name_match: NameMatch::Contains,
        };
        assert!(!eval_condition(&sword_named_shield, &gs, NOW, Some(&data)));

        // No tests set = any held item, no classifier needed.
        let any_item = HotbarCondition::HandHolds {
            hand: HandSlot::Right,
            item_type: None,
            name: None,
            name_match: NameMatch::Contains,
        };
        assert!(eval_condition(&any_item, &gs, NOW, None));
    }

    #[test]
    fn spell_prepared_any_and_named() {
        let mut gs = GameState::new();
        let any = HotbarCondition::SpellPrepared {
            name: None,
            name_match: NameMatch::Contains,
        };
        let named = HotbarCondition::SpellPrepared {
            name: Some("shock".to_string()),
            name_match: NameMatch::Contains,
        };
        assert!(!eval_condition(&any, &gs, NOW, None));
        gs.spell = Some("None".to_string());
        assert!(!eval_condition(&any, &gs, NOW, None));
        gs.spell = Some("Minor Shock".to_string());
        assert!(eval_condition(&any, &gs, NOW, None));
        assert!(eval_condition(&named, &gs, NOW, None));
        gs.spell = Some("Spirit Warding I".to_string());
        assert!(!eval_condition(&named, &gs, NOW, None));
    }

    #[test]
    fn resolve_hand_first_match_wins_and_falls_through() {
        let data = crate::core::gameobj_data::GameObjData::parse(GAMEOBJ_FIXTURE);
        let hand_data = crate::config::HandWidgetData {
            icon: Some("R:".to_string()),
            icon_color: None,
            text_color: None,
            states: vec![
                crate::config::HandIconState {
                    when: HotbarCondition::HandHolds {
                        hand: HandSlot::Right,
                        item_type: Some("weapon".to_string()),
                        name: None,
                        name_match: NameMatch::Contains,
                    },
                    icon: Some(crate::data::IconRef::Image {
                        path: "hands/sword.png".to_string(),
                    }),
                    text: Some("R⚔".to_string()),
                    icon_color: None,
                },
                crate::config::HandIconState {
                    when: HotbarCondition::HandEmpty { hand: HandSlot::Right },
                    icon: Some(crate::data::IconRef::None),
                    text: Some("R-".to_string()),
                    icon_color: None,
                },
            ],
        };

        // Weapon held: first state matches.
        let gs = gs_with_hands(Some(("a rusty sword", "sword")), None);
        let resolved = resolve_hand(&hand_data, &gs, NOW, Some(&data));
        assert_eq!(
            resolved.icon,
            Some(crate::data::IconRef::Image { path: "hands/sword.png".to_string() })
        );
        assert_eq!(resolved.text.as_deref(), Some("R⚔"));

        // Empty hand: second state matches with an explicit artless icon.
        let gs = gs_with_hands(None, None);
        let resolved = resolve_hand(&hand_data, &gs, NOW, Some(&data));
        assert_eq!(resolved.icon, Some(crate::data::IconRef::None));
        assert_eq!(resolved.text.as_deref(), Some("R-"));

        // Non-weapon held: no state matches, everything falls through.
        let gs = gs_with_hands(Some(("a tower shield", "shield")), None);
        let resolved = resolve_hand(&hand_data, &gs, NOW, Some(&data));
        assert_eq!(resolved, ResolvedHand::default());
    }
}
