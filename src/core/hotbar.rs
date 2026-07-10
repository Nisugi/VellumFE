//! Hotbar button resolution: evaluate structured conditions against
//! GameState and produce per-button display state for the frontends.
//!
//! Pure logic — no frontend imports. Both TUI and GUI call `resolve_bar`
//! each frame with `now_server = local unix time + server_time_offset`
//! (the same convention as the countdown widget).

use crate::config::{
    EffectCategory, HotbarCondition, HotbarCountdownSource, HotbarDef, NameMatch, VitalKind,
    VitalUnit,
};
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
}

/// Resolve every button on a bar against the current game state.
pub fn resolve_bar(bar: &HotbarDef, gs: &GameState, now_server: i64) -> Vec<ResolvedHotbarButton> {
    bar.buttons
        .iter()
        .map(|button| {
            let matched = button
                .states
                .iter()
                .find(|state| eval_condition(&state.when, gs, now_server));

            let default_style = button.default_style.as_ref();
            let style = matched.map(|s| &s.style);

            let pick =
                |f: fn(&crate::config::HotbarStyle) -> Option<String>| -> Option<String> {
                    style.and_then(f).or_else(|| default_style.and_then(f))
                };

            ResolvedHotbarButton {
                id: button.id.clone(),
                label: pick(|s| s.label.clone()).unwrap_or_else(|| button.label.clone()),
                command: button.command.clone(),
                tooltip: button.tooltip.clone(),
                hotkey: button.hotkey.clone(),
                fg: pick(|s| s.fg.clone()),
                bg: pick(|s| s.bg.clone()),
                dim: style.map(|s| s.dim).unwrap_or(false)
                    || (style.is_none() && default_style.map(|s| s.dim).unwrap_or(false)),
                countdown_secs: button
                    .countdown
                    .as_ref()
                    .and_then(|src| countdown_secs(src, gs, now_server)),
            }
        })
        .collect()
}

/// Evaluate one condition tree against the game state.
pub fn eval_condition(cond: &HotbarCondition, gs: &GameState, now_server: i64) -> bool {
    match cond {
        HotbarCondition::All { conditions } => conditions
            .iter()
            .all(|c| eval_condition(c, gs, now_server)),
        HotbarCondition::Any { conditions } => conditions
            .iter()
            .any(|c| eval_condition(c, gs, now_server)),
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
    }
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
            NOW
        ));
        assert!(eval_condition(
            &effect_active(EffectCategory::Buffs, "BULL", NameMatch::Contains),
            &gs,
            NOW
        ));
        assert!(!eval_condition(
            &effect_active(EffectCategory::Buffs, "Bull", NameMatch::Exact),
            &gs,
            NOW
        ));
        // Wrong category
        assert!(!eval_condition(
            &effect_active(EffectCategory::Cooldowns, "Strength of the Bull", NameMatch::Exact),
            &gs,
            NOW
        ));
    }

    #[test]
    fn expired_effect_counts_as_inactive() {
        let gs = gs_with_effect("Buffs", "Song of Luck", Some(NOW - 5));
        let cond = effect_active(EffectCategory::Buffs, "Song of Luck", NameMatch::Exact);
        assert!(!eval_condition(&cond, &gs, NOW));
        // ...and effect_inactive is its negation
        assert!(eval_condition(
            &HotbarCondition::EffectInactive {
                category: EffectCategory::Buffs,
                name: "Song of Luck".to_string(),
                name_match: NameMatch::Exact,
            },
            &gs,
            NOW
        ));
    }

    #[test]
    fn indefinite_effect_counts_as_active_while_present() {
        let gs = gs_with_effect("Buffs", "Prestidigitation", None);
        assert!(eval_condition(
            &effect_active(EffectCategory::Buffs, "Prestidigitation", NameMatch::Exact),
            &gs,
            NOW
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
        assert!(eval_condition(&cond(HotbarCmp::Lt, 60), &gs, NOW));
        assert!(!eval_condition(&cond(HotbarCmp::Gt, 60), &gs, NOW));
        // Missing effect -> false
        let empty = GameState::new();
        assert!(!eval_condition(&cond(HotbarCmp::Lt, 60), &empty, NOW));
        // Effect without expiry -> false
        let indef = gs_with_effect("Cooldowns", "Shadow Mastery", None);
        assert!(!eval_condition(&cond(HotbarCmp::Lt, 60), &indef, NOW));
    }

    #[test]
    fn rt_ct_active() {
        let mut gs = GameState::new();
        assert!(!eval_condition(&HotbarCondition::RtActive, &gs, NOW));
        gs.roundtime_end = Some(NOW + 5);
        assert!(eval_condition(&HotbarCondition::RtActive, &gs, NOW));
        gs.roundtime_end = Some(NOW - 1);
        assert!(!eval_condition(&HotbarCondition::RtActive, &gs, NOW));

        gs.casttime_end = Some(NOW + 3);
        assert!(eval_condition(&HotbarCondition::CtActive, &gs, NOW));
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
        assert!(eval_condition(&hidden, &gs, NOW));
        assert!(!eval_condition(&not_hidden, &gs, NOW));
        // Unknown indicator id -> false either way
        let bogus = HotbarCondition::Indicator {
            id: "flying".to_string(),
            active: true,
        };
        assert!(!eval_condition(&bogus, &gs, NOW));
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
        assert!(eval_condition(&low_stamina, &gs, NOW));

        let low_mana_abs = HotbarCondition::Vital {
            vital: VitalKind::Mana,
            cmp: HotbarCmp::Lt,
            value: 9,
            unit: VitalUnit::Absolute,
        };
        assert!(eval_condition(&low_mana_abs, &gs, NOW));

        // Absolute with no minivitals data yet -> false
        let no_data = GameState::new();
        assert!(!eval_condition(&low_mana_abs, &no_data, NOW));
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
        assert!(eval_condition(&cond, &gs, NOW));
        gs.status.stunned = false;
        assert!(!eval_condition(&cond, &gs, NOW));
    }

    fn button_with_states(states: Vec<HotbarButtonState>) -> HotbarButton {
        HotbarButton {
            id: "b1".to_string(),
            label: "Base".to_string(),
            command: "look".to_string(),
            hotkey: None,
            tooltip: None,
            category: None,
            countdown: None,
            states,
            default_style: Some(HotbarStyle {
                label: None,
                fg: Some("#default".to_string()),
                bg: None,
                dim: false,
            }),
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
            buttons: vec![button_with_states(vec![
                HotbarButtonState {
                    when: HotbarCondition::RtActive,
                    style: HotbarStyle {
                        label: Some("InRT".to_string()),
                        fg: None, // falls through to default_style fg
                        bg: None,
                        dim: true,
                    },
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
                },
            ])],
        };

        let resolved = resolve_bar(&bar, &gs, NOW);
        assert_eq!(resolved.len(), 1);
        // Both states match; the first (RT) wins
        assert_eq!(resolved[0].label, "InRT");
        assert!(resolved[0].dim);
        // fg not set on the state -> falls through to default_style
        assert_eq!(resolved[0].fg.as_deref(), Some("#default"));

        // No state matches -> base label + default style
        let idle = GameState::new();
        let resolved = resolve_bar(&bar, &idle, NOW);
        assert_eq!(resolved[0].label, "Base");
        assert!(!resolved[0].dim);
        assert_eq!(resolved[0].fg.as_deref(), Some("#default"));
    }

    #[test]
    fn countdown_from_each_source() {
        let mut gs = gs_with_effect("Cooldowns", "Shadow Mastery", Some(NOW + 42));
        gs.roundtime_end = Some(NOW + 7);
        gs.casttime_end = Some(NOW - 1); // already elapsed

        let mk = |countdown: HotbarCountdownSource| HotbarDef {
            name: "t".to_string(),
            title: None,
            buttons: vec![HotbarButton {
                id: "x".to_string(),
                label: "X".to_string(),
                command: "x".to_string(),
                hotkey: None,
                tooltip: None,
                category: None,
                countdown: Some(countdown),
                states: vec![],
                default_style: None,
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
        );
        assert_eq!(effect[0].countdown_secs, Some(42));

        let rt = resolve_bar(&mk(HotbarCountdownSource::Roundtime), &gs, NOW);
        assert_eq!(rt[0].countdown_secs, Some(7));

        // Elapsed casttime -> no overlay
        let ct = resolve_bar(&mk(HotbarCountdownSource::Casttime), &gs, NOW);
        assert_eq!(ct[0].countdown_secs, None);
    }
}
