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
    /// Snapshot where the player is, for pack scoping.
    ///
    /// `location` and `tags` come from the mapdb (absent when the room is
    /// unmapped), `realm` straight off the wire via `<roommeta>` — which is
    /// why zone scoping still works in places the mapdb has never seen.
    pub fn current_room_scope(&self) -> crate::config::RoomScope {
        let uid = self
            .nav_room_id
            .as_deref()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .filter(|&u| u != 0);
        let (location, tags) = match (self.map.current_room_id, self.map.mapdb()) {
            (Some(id), Some(db)) => match db.room(id) {
                Some(room) => (room.location.clone(), room.tags.clone()),
                None => (self.map.current_location.clone(), Vec::new()),
            },
            _ => (self.map.current_location.clone(), Vec::new()),
        };
        crate::config::RoomScope {
            location,
            realm: self.game_state.room_meta.realm,
            tags,
            uid,
        }
    }

    /// Re-arm alert packs for the current room, if the scope actually changed.
    ///
    /// The matcher has no per-pack partitioning — membership of the pattern
    /// Vec IS the arming mechanism — so re-arming means rebuilding the rule
    /// set from packs whose scope admits this room. That rebuild is an
    /// Aho-Corasick construction, cheap but not free, and the player moves
    /// constantly; gating on a scope EQUALITY check means walking twenty
    /// rooms inside one area rebuilds nothing.
    ///
    /// Deliberately reuses the already-loaded `config.highlights` source of
    /// truth rather than calling `reload_highlights`, which re-reads from
    /// disk — far too expensive to do per room.
    pub fn rearm_alert_packs(&mut self) {
        let scope = self.current_room_scope();
        if self.last_pack_scope.as_ref() == Some(&scope) {
            return;
        }
        // First call of the session loads the pack cache. Self-loading here
        // rather than at construction keeps the disk read off every AppCore
        // (tests build many) and out of the startup path.
        if !self.alert_packs_loaded {
            self.alert_packs_loaded = true;
            self.alert_packs = crate::config::Config::load_alert_packs();
            self.alertpack_approvals = crate::config::Config::load_alertpack_approvals();
        }

        let packs = self.alert_packs.clone();
        if packs.is_empty() {
            // Nothing installed: record the scope so we don't recompute it
            // every frame, and skip the rebuild entirely.
            self.last_pack_scope = Some(scope);
            return;
        }

        // Personal rules are everything not contributed by a pack; rebuild
        // the pack contribution on top of them for the new room.
        let mut highlights: std::collections::HashMap<String, crate::config::HighlightPattern> =
            self.config
                .highlights
                .iter()
                .filter(|(name, _)| !name.starts_with("pack:"))
                .map(|(name, rule)| (name.clone(), rule.clone()))
                .collect();

        crate::config::Config::merge_alert_packs(
            &mut highlights,
            &packs,
            &self.alertpack_approvals,
            &scope,
        );
        crate::config::Config::compile_highlight_patterns(&mut highlights);

        self.config.highlights = highlights;
        self.message_processor.apply_highlights_config(
            self.config.highlights.clone(),
            self.config.highlight_settings.clone(),
        );
        // A pack entering scope brings condition rules whose gates may already
        // be true. Reset edges so those adopt current state silently instead
        // of firing a burst the moment you walk through a door.
        self.alerts.reset_edges();
        self.last_pack_scope = Some(scope);
    }

    /// Reload pack files and approvals from disk into the in-memory cache the
    /// per-room re-arm reads. Called after any pack enable/approve change.
    pub fn refresh_alert_packs(&mut self) {
        self.alert_packs_loaded = true;
        self.alert_packs = crate::config::Config::load_alert_packs();
        self.alertpack_approvals = crate::config::Config::load_alertpack_approvals();
        // Force the next re-arm to rebuild even if the room didn't change.
        self.last_pack_scope = None;
    }

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


impl AppCore {
    /// `.alertpacks` — list installed packs, toggle them, and work the trust
    /// gate. Text-driven for now; a browser UI is a separate piece of work,
    /// but packs must be usable and INSPECTABLE the moment they can load,
    /// since an un-inspectable trust gate is not a trust gate.
    pub fn handle_alertpacks_command(&mut self, parts: &[&str]) {
        let packs = crate::config::Config::load_alert_packs();
        let mut approvals = crate::config::Config::load_alertpack_approvals();

        match parts.get(1).map(|s| s.to_ascii_lowercase()).as_deref() {
            None | Some("list") => {
                if packs.is_empty() {
                    self.add_system_message(
                        "No alert packs installed. Drop a .toml in global/alertpacks/.",
                    );
                    return;
                }
                self.add_system_message("Alert packs:");
                for pack in &packs {
                    let on = approvals.is_enabled(&pack.name);
                    let sensitive = pack.needs_approval();
                    let approved = approvals.is_approved(&pack.name, &pack.hash);
                    let state = match (on, sensitive, approved) {
                        (false, _, _) => "off".to_string(),
                        (true, false, _) => "on".to_string(),
                        (true, true, true) => "on, approved".to_string(),
                        (true, true, false) => {
                            "on, UNAPPROVED (replace/redirect withheld)".to_string()
                        }
                    };
                    self.add_system_message(&format!(
                        "  {} [{}] - {} rules",
                        pack.name,
                        state,
                        pack.rules.len()
                    ));
                }
                self.add_system_message(
                    "Use: .alertpacks <on|off|show|approve|revoke> <name>",
                );
            }
            Some("on") | Some("off") => {
                let on = parts[1].eq_ignore_ascii_case("on");
                let Some(name) = parts.get(2) else {
                    self.add_system_message("Usage: .alertpacks on|off <name>");
                    return;
                };
                if !packs.iter().any(|p| p.name == *name) {
                    self.add_system_message(&format!("No alert pack named '{name}'."));
                    return;
                }
                approvals.set_enabled(name, on);
                self.persist_and_reload_packs(&approvals);
                self.add_system_message(&format!(
                    "Alert pack '{name}' {}.",
                    if on { "enabled" } else { "disabled" }
                ));
                // Point at the gate rather than letting the user wonder why a
                // pack they just enabled isn't fully working.
                if on {
                    if let Some(pack) = packs.iter().find(|p| p.name == *name) {
                        if pack.needs_approval()
                            && !approvals.is_approved(&pack.name, &pack.hash)
                        {
                            self.add_system_message(&format!(
                                "  '{name}' contains replace/redirect rules, withheld until \
                                 you review them: .alertpacks show {name}"
                            ));
                        }
                    }
                }
            }
            Some("show") => {
                let Some(name) = parts.get(2) else {
                    self.add_system_message("Usage: .alertpacks show <name>");
                    return;
                };
                let Some(pack) = packs.iter().find(|p| p.name == *name) else {
                    self.add_system_message(&format!("No alert pack named '{name}'."));
                    return;
                };
                self.add_system_message(&format!(
                    "Pack '{}' ({} rules, hash {})",
                    pack.name,
                    pack.rules.len(),
                    &pack.hash[..8.min(pack.hash.len())]
                ));
                if pack.scope.is_unscoped() {
                    self.add_system_message("  Active everywhere.");
                } else {
                    let scope = &pack.scope;
                    let mut parts: Vec<String> = Vec::new();
                    if !scope.area.is_empty() {
                        parts.push(scope.area.join(", "));
                    }
                    if !scope.zone.is_empty() {
                        parts.push(format!(
                            "zone {}",
                            scope
                                .zone
                                .iter()
                                .map(|z| z.to_string())
                                .collect::<Vec<_>>()
                                .join("/")
                        ));
                    }
                    if !scope.tags.is_empty() {
                        parts.push(format!("tagged {}", scope.tags.join("/")));
                    }
                    if !scope.rooms.is_empty() {
                        parts.push(format!("{} specific room(s)", scope.rooms.len()));
                    }
                    self.add_system_message(&format!("  Active in: {}", parts.join("; ")));
                }
                let sensitive = pack.sensitive_rules();
                if sensitive.is_empty() {
                    self.add_system_message(
                        "  No sensitive rules: this pack cannot alter game text.",
                    );
                    return;
                }
                self.add_system_message("  Sensitive rules needing approval:");
                for (rule, what) in sensitive {
                    self.add_system_message(&format!("    {rule}: {what}"));
                }
                if approvals.is_approved(&pack.name, &pack.hash) {
                    self.add_system_message("  Status: approved.");
                } else {
                    self.add_system_message(&format!(
                        "  Status: NOT approved. Approve with: .alertpacks approve {name}"
                    ));
                }
            }
            Some("approve") | Some("revoke") => {
                let approving = parts[1].eq_ignore_ascii_case("approve");
                let Some(name) = parts.get(2) else {
                    self.add_system_message("Usage: .alertpacks approve|revoke <name>");
                    return;
                };
                let Some(pack) = packs.iter().find(|p| p.name == *name) else {
                    self.add_system_message(&format!("No alert pack named '{name}'."));
                    return;
                };
                if approving {
                    approvals.approve(&pack.name, &pack.hash);
                } else {
                    approvals.revoke(&pack.name);
                }
                self.persist_and_reload_packs(&approvals);
                self.add_system_message(&format!(
                    "Alert pack '{name}' {}.",
                    if approving {
                        "approved for its current contents"
                    } else {
                        "approval revoked"
                    }
                ));
            }
            Some(other) => {
                self.add_system_message(&format!(
                    "Unknown .alertpacks option '{other}'. \
                     Use list, on, off, show, approve, or revoke."
                ));
            }
        }
    }

    /// Save the approval record and rebuild the highlight engine so the
    /// change takes effect immediately rather than at next launch.
    fn persist_and_reload_packs(&mut self, approvals: &crate::config::AlertPackApprovals) {
        if let Err(err) = crate::config::Config::save_alertpack_approvals(approvals) {
            self.add_system_message(&format!("Failed to save alert pack settings: {err}"));
            return;
        }
        // Refresh the in-memory cache first so the reload below (and the next
        // per-room re-arm) both see the new enable/approval state.
        self.refresh_alert_packs();
        self.reload_highlights();
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
    fn rearming_is_skipped_when_the_room_scope_did_not_change() {
        // The player moves constantly; rebuilding the matcher on every step
        // would be a real regression. The equality gate is what prevents it.
        let mut core = AppCore::new_for_test();
        core.alert_packs_loaded = true; // don't touch disk in a unit test

        core.rearm_alert_packs();
        let first = core.last_pack_scope.clone();
        assert!(first.is_some(), "first call records the scope");

        // Nothing about the room changed, so this must be a no-op.
        core.rearm_alert_packs();
        assert_eq!(core.last_pack_scope, first);
    }

    #[test]
    fn changing_realm_changes_the_scope_and_re_arms() {
        let mut core = AppCore::new_for_test();
        core.alert_packs_loaded = true;

        core.game_state.room_meta.realm = Some(1);
        core.rearm_alert_packs();
        assert_eq!(
            core.last_pack_scope.as_ref().and_then(|s| s.realm),
            Some(1)
        );

        // Walking into a different realm is a scope change even with no
        // mapdb data at all — that is what zone scoping buys.
        core.game_state.room_meta.realm = Some(2);
        core.rearm_alert_packs();
        assert_eq!(
            core.last_pack_scope.as_ref().and_then(|s| s.realm),
            Some(2)
        );
    }

    #[test]
    fn the_room_scope_reads_realm_straight_off_the_wire() {
        let mut core = AppCore::new_for_test();
        core.game_state.room_meta.realm = Some(7);
        let scope = core.current_room_scope();
        assert_eq!(scope.realm, Some(7));
        // No mapdb in a test core, so the map-derived fields stay empty
        // rather than inventing values.
        assert!(scope.tags.is_empty());
    }

    #[test]
    fn refreshing_packs_forces_the_next_rearm_to_rebuild() {
        let mut core = AppCore::new_for_test();
        core.alert_packs_loaded = true;
        core.rearm_alert_packs();
        assert!(core.last_pack_scope.is_some());

        // Enabling or approving a pack must take effect without waiting for
        // the player to walk somewhere else.
        core.last_pack_scope = Some(crate::config::RoomScope {
            realm: Some(123),
            ..Default::default()
        });
        core.refresh_alert_packs();
        assert!(
            core.last_pack_scope.is_none(),
            "cleared so the next tick re-arms"
        );
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
