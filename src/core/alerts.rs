//! Core-owned alert state: the live set of overlay alerts plus the discipline
//! layer that keeps them from becoming noise.
//!
//! This lives in core, NOT in a frontend, for the same reason the injury doll
//! does: alerts are state, and state that is computed during rendering
//! double-fires on detached viewports, can't be pushed to the phone bridge,
//! and turns painting into a side effect. Frontends read `ActiveAlert`s and
//! draw them; they never decide what fires.
//!
//! Anti-spam is not a nicety here — it is the feature. A heavy-scroll group
//! encounter (Reim, an invasion) is precisely when alerts matter and precisely
//! when an undisciplined trigger engine buries the screen and gets the whole
//! feature switched off. Hence: per-rule cooldowns and a hard concurrent cap.

use crate::core::highlight_engine::AlertTrigger;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Default seconds between repeat fires of one rule when the author didn't
/// pick a cooldown. Tuned for combat prose: long enough that a rule matching
/// several lines of one exchange fires once, short enough that a genuinely
/// new event a few seconds later still warns you.
pub const DEFAULT_COOLDOWN_SECS: f32 = 3.0;

/// Default seconds an alert stays on screen when unspecified.
pub const DEFAULT_DURATION_SECS: f32 = 4.0;

/// Hard ceiling on simultaneous alerts. Beyond this the screen is unreadable
/// regardless of how important each individual alert is, so the cap is not
/// configurable upward past sanity — an alert you can't read isn't an alert.
pub const MAX_CONCURRENT: usize = 5;

/// One alert currently on screen. Frontends render these; the core expires
/// them. `spawned` is the authority for both fade timing and play-once art
/// progress, so every renderer agrees on the animation phase without keeping
/// its own clock.
#[derive(Clone, Debug)]
pub struct ActiveAlert {
    /// Cooldown identity of the rule that raised this.
    pub key: String,
    /// Capture-expanded banner text, if any.
    pub banner: Option<String>,
    pub banner_fg: Option<String>,
    pub banner_bg: Option<String>,
    /// Image to play once at the anchor.
    pub art: Option<String>,
    /// Viewport-edge tint color.
    pub flash: Option<String>,
    pub anchor: crate::config::AlertAnchor,
    pub offset: (f32, f32),
    /// When this alert appeared; drives fade and one-shot art phase.
    pub spawned: Instant,
    /// Total lifetime before auto-dismiss.
    pub duration: Duration,
    /// Authored priority; carried for phase-2 eviction ordering.
    pub priority: i32,
}

impl ActiveAlert {
    /// Fraction of life elapsed, 0.0 at spawn to 1.0 at dismissal.
    pub fn progress(&self, now: Instant) -> f32 {
        let total = self.duration.as_secs_f32();
        if total <= 0.0 {
            return 1.0;
        }
        (now.duration_since(self.spawned).as_secs_f32() / total).clamp(0.0, 1.0)
    }

    /// Seconds since spawn — the clock a one-shot animation advances on.
    pub fn elapsed_secs(&self, now: Instant) -> f32 {
        now.duration_since(self.spawned).as_secs_f32()
    }

    /// Opacity envelope: quick fade in, hold, fade out. Returns 0.0..=1.0
    /// BEFORE the global opacity ceiling is applied — the renderer owns that
    /// clamp, since it knows whether the alert is painted over text.
    pub fn alpha(&self, now: Instant) -> f32 {
        const FADE_IN: f32 = 0.12;
        const FADE_OUT: f32 = 0.25;
        let p = self.progress(now);
        if p < FADE_IN {
            p / FADE_IN
        } else if p > 1.0 - FADE_OUT {
            ((1.0 - p) / FADE_OUT).max(0.0)
        } else {
            1.0
        }
    }
}

/// The live alert set plus per-rule cooldown bookkeeping.
#[derive(Debug, Default)]
pub struct AlertState {
    /// Currently visible alerts, oldest first (spawn order). Eviction pops the
    /// front, so this ordering is load-bearing.
    active: Vec<ActiveAlert>,
    /// Last fire time per rule key, for cooldown enforcement. Pruned when
    /// entries age out so a long session with many one-off rules doesn't grow
    /// this map without bound.
    last_fired: HashMap<String, Instant>,
    /// Master off switch; when false, triggers are dropped at the door.
    enabled: bool,
}

impl AlertState {
    pub fn new() -> Self {
        Self { active: Vec::new(), last_fired: HashMap::new(), enabled: true }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            // A kill switch that leaves alerts on screen isn't a kill switch.
            self.active.clear();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn active(&self) -> &[ActiveAlert] {
        &self.active
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// Admit a trigger, subject to the kill switch and its cooldown. Returns
    /// whether it was raised — the caller need not care, but tests do.
    pub fn fire(&mut self, trigger: AlertTrigger, now: Instant) -> bool {
        if !self.enabled {
            return false;
        }
        let spec = &trigger.spec;

        // An alert with nothing to show is inert; admitting it would burn a
        // concurrent slot on an invisible entry.
        if spec.banner.is_none() && spec.art.is_none() && spec.flash.is_none() {
            return false;
        }

        let cooldown = Duration::from_secs_f32(
            spec.cooldown.unwrap_or(DEFAULT_COOLDOWN_SECS).max(0.0),
        );
        if let Some(last) = self.last_fired.get(&trigger.key) {
            if now.duration_since(*last) < cooldown {
                return false;
            }
        }
        self.last_fired.insert(trigger.key.clone(), now);

        let duration = Duration::from_secs_f32(
            spec.duration.unwrap_or(DEFAULT_DURATION_SECS).max(0.1),
        );
        self.active.push(ActiveAlert {
            key: trigger.key,
            banner: trigger.banner,
            banner_fg: spec.banner_fg.clone(),
            banner_bg: spec.banner_bg.clone(),
            art: spec.art.clone(),
            flash: spec.flash.clone(),
            anchor: spec.anchor,
            offset: spec.offset.unwrap_or((0.0, 0.0)),
            spawned: now,
            duration,
            priority: spec.priority.unwrap_or(0),
        });

        // Over the cap, drop the oldest: the newest alert is the one still
        // describing what is happening right now.
        while self.active.len() > MAX_CONCURRENT {
            self.active.remove(0);
        }
        true
    }

    /// Retire expired alerts and prune stale cooldown entries. Cheap enough to
    /// call every frame and required to be, since expiry is time-based.
    pub fn tick(&mut self, now: Instant) {
        self.active
            .retain(|alert| now.duration_since(alert.spawned) < alert.duration);

        // Cooldown records only matter until they can no longer block
        // anything. Keeping them past a generous multiple of the longest
        // plausible cooldown is pure leak.
        if self.last_fired.len() > 64 {
            let horizon = Duration::from_secs(300);
            self.last_fired
                .retain(|_, last| now.duration_since(*last) < horizon);
        }
    }

    /// Drop everything on screen without touching cooldowns (used by the
    /// kill switch path and on disconnect).
    pub fn clear(&mut self) {
        self.active.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AlertAnchor, AlertSpec};

    fn spec(banner: &str) -> AlertSpec {
        AlertSpec { banner: Some(banner.to_string()), ..Default::default() }
    }

    fn trigger(key: &str, spec: AlertSpec) -> AlertTrigger {
        AlertTrigger {
            key: key.to_string(),
            banner: spec.banner.clone(),
            spec,
        }
    }

    #[test]
    fn cooldown_blocks_repeat_fires_of_one_rule() {
        let mut state = AlertState::new();
        let t0 = Instant::now();
        assert!(state.fire(trigger("stun", spec("Stunned!")), t0));
        // Same rule, immediately after: the heavy-scroll case.
        assert!(!state.fire(trigger("stun", spec("Stunned!")), t0));
        assert_eq!(state.active().len(), 1);

        // Past the cooldown it may fire again.
        let later = t0 + Duration::from_secs_f32(DEFAULT_COOLDOWN_SECS + 0.1);
        assert!(state.fire(trigger("stun", spec("Stunned!")), later));
        assert_eq!(state.active().len(), 2);
    }

    #[test]
    fn authored_cooldown_overrides_default() {
        let mut state = AlertState::new();
        let t0 = Instant::now();
        let mut s = spec("ping");
        s.cooldown = Some(0.0);
        assert!(state.fire(trigger("r", s.clone()), t0));
        // Zero cooldown means every line may fire.
        assert!(state.fire(trigger("r", s), t0));
    }

    #[test]
    fn distinct_rules_do_not_share_a_cooldown() {
        let mut state = AlertState::new();
        let t0 = Instant::now();
        assert!(state.fire(trigger("a", spec("A")), t0));
        assert!(state.fire(trigger("b", spec("B")), t0));
        assert_eq!(state.active().len(), 2);
    }

    #[test]
    fn concurrent_cap_evicts_oldest() {
        let mut state = AlertState::new();
        let t0 = Instant::now();
        for i in 0..(MAX_CONCURRENT + 3) {
            let mut s = spec(&format!("alert {i}"));
            s.cooldown = Some(0.0);
            s.duration = Some(60.0); // outlive the test
            state.fire(trigger(&format!("rule{i}"), s), t0);
        }
        assert_eq!(state.active().len(), MAX_CONCURRENT);
        // The survivors are the newest ones.
        assert_eq!(state.active()[0].key, "rule3");
        assert_eq!(
            state.active().last().expect("non-empty").key,
            format!("rule{}", MAX_CONCURRENT + 2)
        );
    }

    #[test]
    fn tick_expires_by_duration() {
        let mut state = AlertState::new();
        let t0 = Instant::now();
        let mut s = spec("brief");
        s.duration = Some(1.0);
        state.fire(trigger("x", s), t0);
        assert_eq!(state.active().len(), 1);

        state.tick(t0 + Duration::from_millis(500));
        assert_eq!(state.active().len(), 1, "still within duration");

        state.tick(t0 + Duration::from_millis(1_100));
        assert!(state.is_empty(), "expired after its duration");
    }

    #[test]
    fn kill_switch_blocks_and_clears() {
        let mut state = AlertState::new();
        let t0 = Instant::now();
        state.fire(trigger("x", spec("visible")), t0);
        assert_eq!(state.active().len(), 1);

        state.set_enabled(false);
        assert!(state.is_empty(), "kill switch clears what is on screen");
        assert!(!state.fire(trigger("y", spec("blocked")), t0));
    }

    #[test]
    fn presentation_less_alert_is_rejected() {
        let mut state = AlertState::new();
        // No banner, no art, no flash: nothing to draw.
        let inert = AlertSpec { id: Some("inert".into()), ..Default::default() };
        assert!(!state.fire(trigger("inert", inert), Instant::now()));
        assert!(state.is_empty());
    }

    #[test]
    fn alpha_fades_in_and_out_and_holds_between() {
        let mut state = AlertState::new();
        let t0 = Instant::now();
        let mut s = spec("fade");
        s.duration = Some(4.0);
        state.fire(trigger("f", s), t0);
        let alert = &state.active()[0];

        assert!(alert.alpha(t0) < 0.1, "starts transparent");
        assert_eq!(alert.alpha(t0 + Duration::from_secs(2)), 1.0, "holds opaque");
        assert!(
            alert.alpha(t0 + Duration::from_millis(3_950)) < 0.2,
            "fades out at the end"
        );
    }

    #[test]
    fn anchor_and_offset_round_trip_from_spec() {
        let mut state = AlertState::new();
        let mut s = spec("placed");
        s.anchor = AlertAnchor::BottomRight;
        s.offset = Some((-12.0, 8.0));
        state.fire(trigger("p", s), Instant::now());

        let alert = &state.active()[0];
        assert_eq!(alert.anchor, AlertAnchor::BottomRight);
        assert_eq!(alert.offset, (-12.0, 8.0));
    }
}
