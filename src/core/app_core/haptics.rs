//! Haptic (rumble) event detection: core watches game-state transitions
//! and queues events; frontends drain the queue and drive whatever
//! vibration hardware they have (gilrs force feedback on desktop, the
//! Gamepad vibration API on phones). Detection is frontend-agnostic so
//! every frontend sees the same moments.

use super::AppCore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HapticEvent {
    /// Roundtime or casttime just finished — hands free again.
    RoundtimeEnd,
    /// The character just became stunned.
    Stunned,
    /// The character just died.
    Death,
    /// A highlight rule with a rumble pattern matched; carries the
    /// pattern name (resolved via `RumbleConfig::resolve_pattern`).
    Highlight(String),
}

/// Last-seen values for transition detection.
#[derive(Default)]
pub struct HapticSnapshot {
    rt_active: bool,
    stunned: bool,
    dead: bool,
}

impl AppCore {
    /// Watch for state transitions and queue haptic events. Called once
    /// per frontend frame; cheap.
    pub fn poll_haptics(&mut self) {
        let now = chrono::Utc::now().timestamp() + self.server_time_offset;
        let rt_active = self
            .game_state
            .roundtime_end
            .map(|end| end > now)
            .unwrap_or(false)
            || self
                .game_state
                .casttime_end
                .map(|end| end > now)
                .unwrap_or(false);
        let stunned = self.game_state.status.stunned;
        let dead = self.game_state.status.dead;

        if self.haptic_prev.rt_active && !rt_active {
            self.pending_haptics.push(HapticEvent::RoundtimeEnd);
        }
        if stunned && !self.haptic_prev.stunned {
            self.pending_haptics.push(HapticEvent::Stunned);
        }
        if dead && !self.haptic_prev.dead {
            self.pending_haptics.push(HapticEvent::Death);
        }

        self.haptic_prev = HapticSnapshot {
            rt_active,
            stunned,
            dead,
        };
    }

    /// Take the queued events (frontends call this after poll_haptics).
    pub fn drain_haptics(&mut self) -> Vec<HapticEvent> {
        std::mem::take(&mut self.pending_haptics)
    }

    /// Move highlight-matched rumble patterns from the message pipeline
    /// into the haptic queue. A chatty rule can match many lines per
    /// second, so at most one highlight rumble passes per cooldown
    /// window; the rest are dropped, not queued.
    pub fn queue_highlight_rumbles(&mut self) {
        const COOLDOWN: std::time::Duration = std::time::Duration::from_millis(1500);
        if self.message_processor.pending_rumbles.is_empty() {
            return;
        }
        let mut names = std::mem::take(&mut self.message_processor.pending_rumbles);
        let now = std::time::Instant::now();
        if let Some(last) = self.last_highlight_rumble {
            if now.duration_since(last) < COOLDOWN {
                return;
            }
        }
        self.last_highlight_rumble = Some(now);
        self.pending_haptics
            .push(HapticEvent::Highlight(names.swap_remove(0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_fire_once() {
        let mut core = AppCore::new_for_test();
        core.poll_haptics();
        assert!(core.drain_haptics().is_empty());

        core.game_state.status.stunned = true;
        core.poll_haptics();
        assert_eq!(core.drain_haptics(), vec![HapticEvent::Stunned]);
        core.poll_haptics();
        assert!(core.drain_haptics().is_empty(), "no repeat while held");

        core.game_state.status.stunned = false;
        core.poll_haptics();
        assert!(core.drain_haptics().is_empty(), "recovery is silent");
    }

    #[test]
    fn highlight_rumbles_rate_limited_by_cooldown() {
        let mut core = AppCore::new_for_test();
        core.queue_highlight_rumbles();
        assert!(core.drain_haptics().is_empty(), "empty queue is a no-op");

        // A burst of matches in one line: only the first pattern fires.
        core.message_processor
            .pending_rumbles
            .extend(["short".to_string(), "long".to_string()]);
        core.queue_highlight_rumbles();
        assert_eq!(
            core.drain_haptics(),
            vec![HapticEvent::Highlight("short".to_string())]
        );

        // Within the cooldown window: dropped entirely, not queued for later.
        core.message_processor.pending_rumbles.push("short".to_string());
        core.queue_highlight_rumbles();
        assert!(core.drain_haptics().is_empty(), "cooldown drops repeats");
        assert!(
            core.message_processor.pending_rumbles.is_empty(),
            "dropped rumbles must not pile up"
        );
    }

    #[test]
    fn roundtime_end_fires_on_expiry() {
        let mut core = AppCore::new_for_test();
        let now = chrono::Utc::now().timestamp();
        core.game_state.roundtime_end = Some(now + 60);
        core.poll_haptics();
        assert!(core.drain_haptics().is_empty(), "still in RT");
        core.game_state.roundtime_end = Some(now - 1);
        core.poll_haptics();
        assert_eq!(core.drain_haptics(), vec![HapticEvent::RoundtimeEnd]);
    }
}
