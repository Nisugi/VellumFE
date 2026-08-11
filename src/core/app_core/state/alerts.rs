//! Alert plumbing on `AppCore`: draining pattern-matched triggers, evaluating
//! condition gates each frame, and retiring expired alerts.
//!
//! Split out of `state.rs` to keep that file a facade. The decision logic
//! itself lives in `core::alerts`; this module is the wiring between the
//! message pump, the game state, and that engine.

use super::AppCore;

/// Is this gate empty of any actual test?
///
/// `Condition::All { conditions: [] }` evaluates to TRUE by the ordinary
/// rules of `.all()`, which is correct logic and useless as a trigger: a
/// half-built gate would fire immediately and keep firing. Detecting it here
/// (rather than changing `eval_condition`'s semantics, which hotbars and the
/// injury doll also depend on) keeps the fix local to alerts.
fn is_empty_gate(condition: &crate::config::Condition) -> bool {
    match condition {
        crate::config::Condition::All { conditions }
        | crate::config::Condition::Any { conditions } => {
            conditions.is_empty() || conditions.iter().all(is_empty_gate)
        }
        _ => false,
    }
}

impl AppCore {
    /// Hand queued alert triggers to the core alert state, which applies the
    /// cooldown and cap. Mirrors `apply_pending_status_actions`: the message
    /// pump collects, AppCore admits, frontends only draw.
    pub fn apply_pending_alerts(&mut self) {
        // Config is the authority for the kill switch; sync it here rather
        // than mirroring it into a second place that can drift. Toggling it
        // off also clears what is already on screen (see `set_enabled`).
        let enabled = self.config.highlight_settings.alerts_enabled;
        if self.alerts.is_enabled() != enabled {
            self.alerts.set_enabled(enabled);
        }
        if self.message_processor.pending_alerts.is_empty() {
            return;
        }
        let triggers: Vec<_> = self.message_processor.pending_alerts.drain(..).collect();
        let now = std::time::Instant::now();
        for trigger in triggers {
            self.alerts.fire(trigger, now);
        }
    }

    /// Evaluate condition-gated alerts and fire the ones that just became
    /// true. Runs once per frame in `poll_map`.
    ///
    /// Core owns this, not a render path. `eval_condition` is pure and takes
    /// only `&GameState`, so evaluating here costs nothing extra and buys
    /// three things a frontend evaluation could never provide: detached
    /// viewports can't double-fire, painting stays side-effect free, and the
    /// phone bridge sees the same alerts the desktop does.
    ///
    /// Time-based gates (an effect with under five seconds left) are exactly
    /// why this is a frame tick rather than a post-message hook — no message
    /// arrives when a timer simply runs down.
    pub fn tick_alert_conditions(&mut self) {
        if !self.config.highlight_settings.alerts_enabled {
            return;
        }

        // Collect first, fire second: evaluation borrows game_state
        // immutably while firing needs &mut self.
        let now = std::time::Instant::now();
        let now_server = chrono::Utc::now().timestamp() + self.server_time_offset;
        let gameobj = self.gameobj_data_cached();

        // Pass 1: evaluate every gate while `game_state` and the config are
        // borrowed immutably. Nothing here touches alert state.
        let mut evaluated: Vec<(String, bool, f32, crate::config::AlertSpec)> = Vec::new();
        let mut live_keys: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for (name, pattern) in self.config.highlights.iter() {
            let Some(spec) = pattern.alert.as_ref() else {
                continue;
            };
            let Some(condition) = spec.when.as_ref() else {
                continue;
            };
            // A rule with BOTH a pattern and a `when` is a pattern alert that
            // happens to be gated; phase 1 fires it from the text match. Only
            // pattern-less rules are condition-driven, so skip those here to
            // avoid firing the same rule from two engines.
            if !pattern.pattern.is_empty() {
                continue;
            }
            // An empty group is vacuously TRUE (`[].all()` is true), so a gate
            // the user started building but left empty would fire the instant
            // it was saved and on every re-arm thereafter. A gate with no
            // tests states no requirement, so treat it as inert.
            if is_empty_gate(condition) {
                continue;
            }

            let key = spec
                .id
                .clone()
                .unwrap_or_else(|| format!("cond:{name}"));
            live_keys.insert(key.clone());

            let gate = crate::core::conditions::eval_condition(
                condition,
                &self.game_state,
                now_server,
                gameobj,
            );
            let rearm = spec
                .rearm
                .unwrap_or(crate::core::alerts::DEFAULT_REARM_SECS);

            evaluated.push((key, gate, rearm, spec.clone()));
        }

        if evaluated.is_empty() {
            // No condition rules at all: don't even prune, so the common case
            // (nobody uses condition alerts) costs one config scan.
            return;
        }

        // Stop tracking rules the user deleted or renamed.
        self.alerts.retain_edges(&live_keys);

        // Pass 2: feed each gate to the edge detector and fire the risers.
        for (key, gate, rearm, spec) in evaluated {
            if self.alerts.observe_condition(&key, gate, rearm, now) {
                let trigger = crate::core::highlight_engine::AlertTrigger {
                    key,
                    banner: spec.banner.clone(),
                    spec,
                };
                self.alerts.fire(trigger, now);
            }
        }
    }

    /// Retire alerts whose time is up. Called once per frame with the other
    /// pollers; expiry is time-based, so this must run even on idle frames.
    pub fn tick_alerts(&mut self) {
        // Also sync the kill switch here, not just on the drain path: toggling
        // alerts off must take effect immediately even when the game is quiet
        // and no lines are arriving to drive `apply_pending_alerts`.
        let enabled = self.config.highlight_settings.alerts_enabled;
        if self.alerts.is_enabled() != enabled {
            self.alerts.set_enabled(enabled);
        }
        self.alerts.tick(std::time::Instant::now());
    }
}


#[cfg(test)]
mod alert_condition_tests {
    use crate::config::{
        AlertSpec, Cmp, Condition, HighlightPattern, VitalKind, VitalUnit,
    };
    use crate::core::AppCore;

    /// A pattern-less highlight whose alert is gated on health < 30%.
    fn low_health_rule() -> HighlightPattern {
        let mut pattern = HighlightPattern {
            pattern: String::new(),
            ..sample_pattern()
        };
        pattern.alert = Some(AlertSpec {
            id: Some("low-hp".to_string()),
            banner: Some("LOW HEALTH".to_string()),
            when: Some(Condition::Vital {
                vital: VitalKind::Health,
                cmp: Cmp::Lt,
                value: 30,
                unit: VitalUnit::Percent,
            }),
            rearm: Some(0.0),
            // Edge detection and the per-rule cooldown are independent
            // guards. These tests exercise the former, so the latter is
            // opened up; `condition_alert_still_obeys_cooldown_and_cap` in
            // core::alerts covers their interaction.
            cooldown: Some(0.0),
            ..Default::default()
        });
        pattern
    }

    fn sample_pattern() -> HighlightPattern {
        HighlightPattern {
            pattern: "unused".to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: Default::default(),
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        }
    }

    fn core_with_low_health_rule() -> AppCore {
        let mut core = AppCore::new_for_test();
        core.config
            .highlights
            .insert("low-hp".to_string(), low_health_rule());
        core
    }

    #[test]
    fn condition_alert_fires_on_the_crossing_not_the_level() {
        let mut core = core_with_low_health_rule();

        // Healthy: adopt the false state, nothing fires.
        core.game_state.vitals.health = 90;
        core.tick_alert_conditions();
        assert!(core.alerts.is_empty(), "healthy state raises nothing");

        // Cross the threshold: one alert.
        core.game_state.vitals.health = 20;
        core.tick_alert_conditions();
        assert_eq!(core.alerts.active().len(), 1, "crossing fires once");

        // Still hurt over many frames: no new alerts. This is the whole
        // point of edge detection — a level trigger would stack one per frame.
        for _ in 0..30 {
            core.tick_alert_conditions();
        }
        assert_eq!(
            core.alerts.active().len(),
            1,
            "staying below the threshold must not re-fire"
        );
    }

    #[test]
    fn recovering_and_dropping_again_fires_a_second_time() {
        let mut core = core_with_low_health_rule();
        core.game_state.vitals.health = 90;
        core.tick_alert_conditions();

        core.game_state.vitals.health = 20;
        core.tick_alert_conditions();
        assert_eq!(core.alerts.active().len(), 1);

        // Heal back up (re-arms), then drop again: a genuinely new event.
        core.game_state.vitals.health = 80;
        core.tick_alert_conditions();
        core.game_state.vitals.health = 15;
        core.tick_alert_conditions();
        assert_eq!(core.alerts.active().len(), 2, "a new crossing warns again");
    }

    #[test]
    fn already_low_at_startup_does_not_fire() {
        let mut core = core_with_low_health_rule();
        // First evaluation ever, condition already true: pre-existing state.
        core.game_state.vitals.health = 10;
        core.tick_alert_conditions();
        assert!(
            core.alerts.is_empty(),
            "startup adopts reality instead of flooding the screen"
        );
    }

    #[test]
    fn kill_switch_stops_condition_evaluation() {
        let mut core = core_with_low_health_rule();
        core.config.highlight_settings.alerts_enabled = false;
        core.game_state.vitals.health = 90;
        core.tick_alert_conditions();
        core.game_state.vitals.health = 20;
        core.tick_alert_conditions();
        assert!(core.alerts.is_empty(), "disabled means no condition alerts");
    }

    #[test]
    fn pattern_rules_are_not_fired_by_the_condition_engine() {
        // A rule with BOTH a pattern and a `when` belongs to the text-match
        // engine; firing it here too would double up.
        let mut core = AppCore::new_for_test();
        let mut rule = low_health_rule();
        rule.pattern = "You are hurt".to_string();
        core.config.highlights.insert("both".to_string(), rule);

        core.game_state.vitals.health = 90;
        core.tick_alert_conditions();
        core.game_state.vitals.health = 20;
        core.tick_alert_conditions();
        assert!(
            core.alerts.is_empty(),
            "pattern-bearing rules fire from the text match, not here"
        );
    }

    #[test]
    fn an_empty_condition_group_never_fires() {
        // A gate the user started but left empty is vacuously true. Without
        // the guard it would fire the moment it was saved and keep firing.
        let mut core = AppCore::new_for_test();
        let mut rule = low_health_rule();
        rule.alert.as_mut().expect("alert").when =
            Some(Condition::All { conditions: Vec::new() });
        core.config.highlights.insert("empty".to_string(), rule);

        for _ in 0..10 {
            core.tick_alert_conditions();
        }
        assert!(core.alerts.is_empty(), "an empty gate states no requirement");
    }

    #[test]
    fn a_group_of_empty_groups_is_still_empty() {
        let mut core = AppCore::new_for_test();
        let mut rule = low_health_rule();
        rule.alert.as_mut().expect("alert").when = Some(Condition::All {
            conditions: vec![
                Condition::Any { conditions: Vec::new() },
                Condition::All { conditions: Vec::new() },
            ],
        });
        core.config.highlights.insert("nested".to_string(), rule);

        core.tick_alert_conditions();
        core.tick_alert_conditions();
        assert!(core.alerts.is_empty(), "nesting nothing is still nothing");
    }

    #[test]
    fn a_group_with_one_real_test_still_fires() {
        // The empty-gate guard must not swallow legitimate single-test gates,
        // which is exactly the shape the editor produces.
        let mut core = AppCore::new_for_test();
        let mut rule = low_health_rule();
        rule.alert.as_mut().expect("alert").when = Some(Condition::All {
            conditions: vec![Condition::Vital {
                vital: VitalKind::Health,
                cmp: Cmp::Lt,
                value: 30,
                unit: VitalUnit::Percent,
            }],
        });
        core.config.highlights.insert("real".to_string(), rule);

        core.game_state.vitals.health = 90;
        core.tick_alert_conditions();
        core.game_state.vitals.health = 20;
        core.tick_alert_conditions();
        assert_eq!(core.alerts.active().len(), 1);
    }

    #[test]
    fn rules_without_alerts_cost_nothing_and_fire_nothing() {
        let mut core = AppCore::new_for_test();
        core.config
            .highlights
            .insert("plain".to_string(), sample_pattern());
        core.game_state.vitals.health = 5;
        core.tick_alert_conditions();
        assert!(core.alerts.is_empty());
    }
}
