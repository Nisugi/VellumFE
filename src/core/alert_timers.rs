//! Client-authored countdown bars started by alerts.
//!
//! These reuse `ActiveEffect` wholesale rather than inventing a bar type: an
//! effect is already `{ label, percent, time string, absolute expiry }`, which
//! is exactly a timer, and `display_value`/`display_time` already drain the
//! bar smoothly and format HH:MM:SS. Both frontends' effects renderers read
//! the window's own content and treat `category` as an opaque string, so a
//! `Timers` category draws correctly with no rendering changes at all.
//!
//! What is genuinely new is ownership. Server-fed effects are removed by the
//! server — `ActiveEffect::display_time` says so explicitly, holding at
//! 00:00:00 forever. A client timer has no server, so this module reaps its
//! own: without that, every timer ever started would pile up in the window as
//! a dead 00:00:00 bar.

use crate::data::{ActiveEffect, ActiveEffectsContent};

/// Category key for client timers, in `GameState.effects` and as the
/// `category` of any window displaying them.
pub const TIMERS_CATEGORY: &str = "Timers";

/// Start (or restart) a timer in `store`.
///
/// Restarting an existing id rather than stacking a duplicate is deliberate:
/// a mechanic that re-triggers should reset its countdown, and two bars for
/// one thing is just noise.
pub fn start(
    store: &mut ActiveEffectsContent,
    id: &str,
    label: &str,
    duration_secs: f32,
    color: Option<&str>,
    now_server: i64,
) {
    let duration = duration_secs.max(0.0) as i64;
    let expires_at = now_server + duration;
    // `time` is the AT-ARRIVAL duration, which `display_value` divides by to
    // drain the bar. Writing the remaining time here instead would make every
    // bar restart at full width on each tick.
    let time = format!(
        "{:02}:{:02}:{:02}",
        duration / 3600,
        (duration % 3600) / 60,
        duration % 60
    );

    if let Some(existing) = store.effects.iter_mut().find(|e| e.id == id) {
        existing.text = label.to_string();
        existing.value = 100;
        existing.time = time;
        existing.expires_at = Some(expires_at);
        existing.bar_color = color.map(|c| c.to_string());
    } else {
        store.effects.push(ActiveEffect {
            id: id.to_string(),
            text: label.to_string(),
            value: 100,
            time,
            expires_at: Some(expires_at),
            bar_color: color.map(|c| c.to_string()),
            text_color: None,
        });
    }
    store.generation += 1;
}

/// Cancel timers by id. Returns whether anything was removed, so the caller
/// can skip a needless window sync.
pub fn cancel(store: &mut ActiveEffectsContent, ids: &[String]) -> bool {
    if ids.is_empty() {
        return false;
    }
    let before = store.effects.len();
    store
        .effects
        .retain(|e| !ids.iter().any(|id| id.eq_ignore_ascii_case(&e.id)));
    let removed = store.effects.len() != before;
    if removed {
        store.generation += 1;
    }
    removed
}

/// Drop timers whose deadline has passed. Returns whether anything expired.
///
/// This is the piece with no server-fed equivalent. Effects deliberately hold
/// at zero because the server removes them; nothing would ever remove these.
pub fn reap(store: &mut ActiveEffectsContent, now_server: i64) -> bool {
    let before = store.effects.len();
    store
        .effects
        .retain(|e| e.expires_at.is_some_and(|at| at > now_server));
    let expired = store.effects.len() != before;
    if expired {
        store.generation += 1;
    }
    expired
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> ActiveEffectsContent {
        ActiveEffectsContent {
            category: TIMERS_CATEGORY.to_string(),
            effects: Vec::new(),
            generation: 0,
        }
    }

    #[test]
    fn starting_a_timer_creates_a_full_bar_with_an_absolute_deadline() {
        let mut s = store();
        start(&mut s, "boss", "Boss cast", 12.0, None, 1_000);
        assert_eq!(s.effects.len(), 1);
        let t = &s.effects[0];
        assert_eq!(t.text, "Boss cast");
        assert_eq!(t.value, 100, "starts full");
        assert_eq!(t.expires_at, Some(1_012), "absolute, so it cannot drift");
        assert_eq!(t.time, "00:00:12", "at-arrival duration drives the drain");
        assert!(t.ticks());
    }

    #[test]
    fn the_bar_drains_over_its_duration() {
        let mut s = store();
        start(&mut s, "boss", "Boss cast", 10.0, None, 1_000);
        let t = &s.effects[0];
        // display_value is what the renderer paints; no new math needed here.
        assert_eq!(t.display_value(1_000), 100, "full at the start");
        assert_eq!(t.display_value(1_005), 50, "half way");
        assert_eq!(t.display_value(1_010), 0, "empty at the deadline");
        assert_eq!(t.display_time(1_007), "00:00:03");
    }

    #[test]
    fn restarting_an_id_resets_it_instead_of_stacking_a_duplicate() {
        let mut s = store();
        start(&mut s, "boss", "Boss cast", 10.0, None, 1_000);
        start(&mut s, "boss", "Boss cast", 10.0, None, 1_006);
        assert_eq!(s.effects.len(), 1, "one bar, not two");
        assert_eq!(s.effects[0].expires_at, Some(1_016), "deadline reset");
    }

    #[test]
    fn cancelling_removes_the_named_timer_and_leaves_the_rest() {
        let mut s = store();
        start(&mut s, "boss", "Boss cast", 10.0, None, 1_000);
        start(&mut s, "adds", "Adds spawn", 30.0, None, 1_000);

        assert!(cancel(&mut s, &["boss".to_string()]));
        assert_eq!(s.effects.len(), 1);
        assert_eq!(s.effects[0].id, "adds");
    }

    #[test]
    fn cancelling_is_case_insensitive_and_reports_when_it_did_nothing() {
        let mut s = store();
        start(&mut s, "Boss", "Boss cast", 10.0, None, 1_000);
        assert!(cancel(&mut s, &["BOSS".to_string()]));
        assert!(s.effects.is_empty());
        // Nothing to remove: the caller can skip a window sync.
        assert!(!cancel(&mut s, &["ghost".to_string()]));
        assert!(!cancel(&mut s, &[]));
    }

    #[test]
    fn expired_timers_are_reaped_because_no_server_will_remove_them() {
        let mut s = store();
        start(&mut s, "short", "Short", 5.0, None, 1_000);
        start(&mut s, "long", "Long", 60.0, None, 1_000);

        assert!(!reap(&mut s, 1_004), "nothing expired yet");
        assert_eq!(s.effects.len(), 2);

        assert!(reap(&mut s, 1_006), "the short one is done");
        assert_eq!(s.effects.len(), 1);
        assert_eq!(s.effects[0].id, "long");
    }

    #[test]
    fn every_mutation_bumps_generation_so_the_tui_rebuilds() {
        // The TUI's dirty check is generation-based; forgetting to bump it
        // means a bar that never appears or never disappears.
        let mut s = store();
        let g0 = s.generation;
        start(&mut s, "a", "A", 5.0, None, 1_000);
        assert!(s.generation > g0, "start bumps");

        let g1 = s.generation;
        reap(&mut s, 1_100);
        assert!(s.generation > g1, "reap bumps");

        start(&mut s, "b", "B", 5.0, None, 1_000);
        let g2 = s.generation;
        cancel(&mut s, &["b".to_string()]);
        assert!(s.generation > g2, "cancel bumps");
    }

    #[test]
    fn a_zero_duration_timer_expires_immediately_rather_than_sticking() {
        let mut s = store();
        start(&mut s, "instant", "Instant", 0.0, None, 1_000);
        assert!(reap(&mut s, 1_000), "already past its deadline");
        assert!(s.effects.is_empty());
    }

    #[test]
    fn a_negative_duration_is_clamped_not_turned_into_a_past_deadline() {
        let mut s = store();
        start(&mut s, "bad", "Bad", -30.0, None, 1_000);
        assert_eq!(
            s.effects[0].expires_at,
            Some(1_000),
            "clamped to zero, not 30s in the past"
        );
    }
}
