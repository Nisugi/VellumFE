//! Hotbar button resolution: apply the shared condition engine
//! (`core::conditions`) to bar definitions and produce per-button display
//! state for the frontends.
//!
//! Pure logic — no frontend imports. Both TUI and GUI call `resolve_bar`
//! each frame with `now_server = local unix time + server_time_offset`
//! (the same convention as the countdown widget).

use crate::config::{HotbarCountdownSource, HotbarDef};
use crate::core::conditions::{effect_lookup, eval_condition};
use crate::core::gameobj_data::GameObjData;
use crate::core::state::GameState;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Cmp, Condition, EffectCategory, HandSlot, HotbarButton, HotbarButtonState, HotbarStyle,
        NameMatch, VitalKind, VitalUnit,
    };
    use crate::core::conditions::{eval_condition, resolve_hand, ResolvedHand};
    use crate::data::{ActiveEffect, ActiveEffectsContent};

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

    fn effect_active(category: EffectCategory, name: &str, m: NameMatch) -> Condition {
        Condition::EffectActive {
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
            &Condition::EffectInactive {
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
        let cond = |cmp: Cmp, seconds: i64| Condition::EffectTime {
            category: EffectCategory::Cooldowns,
            name: "Shadow Mastery".to_string(),
            name_match: NameMatch::Exact,
            cmp,
            seconds,
        };
        assert!(eval_condition(&cond(Cmp::Lt, 60), &gs, NOW, None));
        assert!(!eval_condition(&cond(Cmp::Gt, 60), &gs, NOW, None));
        // Missing effect -> false
        let empty = GameState::new();
        assert!(!eval_condition(&cond(Cmp::Lt, 60), &empty, NOW, None));
        // Effect without expiry -> false
        let indef = gs_with_effect("Cooldowns", "Shadow Mastery", None);
        assert!(!eval_condition(&cond(Cmp::Lt, 60), &indef, NOW, None));
    }

    #[test]
    fn rt_ct_active() {
        let mut gs = GameState::new();
        assert!(!eval_condition(&Condition::RtActive, &gs, NOW, None));
        gs.roundtime_end = Some(NOW + 5);
        assert!(eval_condition(&Condition::RtActive, &gs, NOW, None));
        gs.roundtime_end = Some(NOW - 1);
        assert!(!eval_condition(&Condition::RtActive, &gs, NOW, None));

        gs.casttime_end = Some(NOW + 3);
        assert!(eval_condition(&Condition::CtActive, &gs, NOW, None));
    }

    #[test]
    fn indicator_and_inverted() {
        let mut gs = GameState::new();
        gs.status.set("hidden", true);
        let hidden = Condition::Indicator {
            id: "hidden".to_string(),
            active: true,
        };
        let not_hidden = Condition::Indicator {
            id: "hidden".to_string(),
            active: false,
        };
        assert!(eval_condition(&hidden, &gs, NOW, None));
        assert!(!eval_condition(&not_hidden, &gs, NOW, None));
        // Unknown indicator id -> false either way
        let bogus = Condition::Indicator {
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

        let low_stamina = Condition::Vital {
            vital: VitalKind::Stamina,
            cmp: Cmp::Lt,
            value: 20,
            unit: VitalUnit::Percent,
        };
        assert!(eval_condition(&low_stamina, &gs, NOW, None));

        let low_mana_abs = Condition::Vital {
            vital: VitalKind::Mana,
            cmp: Cmp::Lt,
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
        gs.status.set("stunned", true);
        gs.roundtime_end = Some(NOW + 5);

        let cond = Condition::Any {
            conditions: vec![
                Condition::CtActive,
                Condition::All {
                    conditions: vec![
                        Condition::RtActive,
                        Condition::Indicator {
                            id: "stunned".to_string(),
                            active: true,
                        },
                    ],
                },
            ],
        };
        assert!(eval_condition(&cond, &gs, NOW, None));
        gs.status.set("stunned", false);
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
        gs.status.set("hidden", true);

        let bar = HotbarDef {
            name: "test".to_string(),
            title: None,
            icon_size: None,
            buttons: vec![button_with_states(vec![
                HotbarButtonState {
                    when: Condition::RtActive,
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
                    when: Condition::Indicator {
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
            icon: crate::data::IconRef::SheetCell {
                sheet: "sheet".to_string(),
                cell,
            },
            ..Default::default()
        };
        let icon_cell = |icon: &HotbarIcon| match &icon.icon {
            crate::data::IconRef::SheetCell { cell, .. } => *cell,
            other => panic!("expected sheet cell, got {other:?}"),
        };
        let mut button = button_with_states(vec![HotbarButtonState {
            when: Condition::RtActive,
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
        assert_eq!(icon_cell(resolved[0].icon.as_ref().unwrap()), 9);
        assert_eq!(resolved[0].icon_mode, IconMode::Icon);

        // Idle: falls back to the button's base icon.
        let idle = GameState::new();
        let resolved = resolve_bar(&bar, &idle, NOW, None);
        assert_eq!(icon_cell(resolved[0].icon.as_ref().unwrap()), 1);
    }

    #[test]
    fn state_countdown_overrides_button_countdown() {
        let mut button = button_with_states(vec![HotbarButtonState {
            when: Condition::RtActive,
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
        let cond = Condition::SpellAffordable { number: 101 }; // 1 mana

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
        let unknown = Condition::SpellAffordable { number: 9999 };
        assert!(!eval_condition(&unknown, &gs, NOW, None));

        // Formula-cost spell (Song of Luck): fail closed even with mana.
        gs.minivitals.update_vital("mana", 999, 999, String::new());
        let dynamic = Condition::SpellAffordable { number: 1006 };
        assert!(!eval_condition(&dynamic, &gs, NOW, None));
    }

    #[test]
    fn state_command_overrides_button_command() {
        let button = button_with_states(vec![HotbarButtonState {
            when: Condition::RtActive,
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
        let right = Condition::HandEmpty { hand: HandSlot::Right };
        let left = Condition::HandEmpty { hand: HandSlot::Left };
        let either = Condition::HandEmpty { hand: HandSlot::Either };
        assert!(!eval_condition(&right, &gs, NOW, None));
        assert!(eval_condition(&left, &gs, NOW, None));
        assert!(eval_condition(&either, &gs, NOW, None));

        // The game's literal "Empty" (string feed, no registry entry)
        // counts as empty.
        let mut gs = GameState::new();
        gs.right_hand = Some("Empty".to_string());
        assert!(eval_condition(&right, &gs, NOW, None));

        // Spell slot: "None" counts as nothing prepared.
        let spell_empty = Condition::HandEmpty { hand: HandSlot::Spell };
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

        let holds_weapon = |hand| Condition::HandHolds {
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
        let holds_shield = Condition::HandHolds {
            hand: HandSlot::Left,
            item_type: Some("armor".to_string()),
            name: Some("shield".to_string()),
            name_match: NameMatch::Contains,
        };
        assert!(eval_condition(&holds_shield, &gs, NOW, Some(&data)));

        // AND semantics: right hand is a weapon but not named shield.
        let sword_named_shield = Condition::HandHolds {
            hand: HandSlot::Right,
            item_type: Some("weapon".to_string()),
            name: Some("shield".to_string()),
            name_match: NameMatch::Contains,
        };
        assert!(!eval_condition(&sword_named_shield, &gs, NOW, Some(&data)));

        // No tests set = any held item, no classifier needed.
        let any_item = Condition::HandHolds {
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
        let any = Condition::SpellPrepared {
            name: None,
            name_match: NameMatch::Contains,
        };
        let named = Condition::SpellPrepared {
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
            hand_text_color: None,
            states: vec![
                crate::config::HandIconState {
                    when: Condition::HandHolds {
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
                    when: Condition::HandEmpty { hand: HandSlot::Right },
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
