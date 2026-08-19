//! Highlight-driven custom statuses: apply queued set/clear actions, tick
//! auto-clear deadlines, and flip the matching indicator/dashboard entries.
//!
//! Split out of `state.rs` to keep that file a facade (the same reason the
//! alert plumbing lives in `state/alerts.rs`).

use super::AppCore;

impl AppCore {
    /// Apply queued custom-status changes from matched highlights: flip any
    /// indicator/dashboard entry whose id matches, and track auto-clear
    /// deadlines. Statuses ride the exact indicator machinery the server's
    /// IconXXX updates use, so icons, grayscale, and TUI glyphs all apply.
    pub fn apply_pending_status_actions(&mut self) {
        let actions: Vec<_> = self
            .message_processor
            .pending_status_actions
            .drain(..)
            .collect();
        for action in actions {
            if let Some((id, duration)) = action.set {
                self.set_custom_status(&id, true);
                match duration {
                    Some(secs) if secs > 0.0 => {
                        self.custom_status_expiries.insert(
                            id.to_ascii_uppercase(),
                            std::time::Instant::now() + std::time::Duration::from_secs_f32(secs),
                        );
                    }
                    _ => {
                        self.custom_status_expiries.remove(&id.to_ascii_uppercase());
                    }
                }
            }
            if let Some(id) = action.clear {
                self.set_custom_status(&id, false);
                self.custom_status_expiries.remove(&id.to_ascii_uppercase());
            }
        }
    }

    /// Deactivate custom statuses whose duration ran out. Called once per
    /// frame alongside the other pollers.
    pub fn tick_custom_statuses(&mut self) {
        if self.custom_status_expiries.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let expired: Vec<String> = self
            .custom_status_expiries
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            self.custom_status_expiries.remove(&id);
            self.set_custom_status(&id, false);
        }
    }

    /// Flip every indicator/dashboard entry whose id matches (the same
    /// update the server's status indicators perform).
    fn set_custom_status(&mut self, id: &str, active: bool) {
        let claimed = self.indicator_id_is_claimed(id);
        for window in self.ui_state.windows.values_mut() {
            match &mut window.content {
                crate::data::WindowContent::Indicator(ref mut indicator) => {
                    if indicator.indicator_id.eq_ignore_ascii_case(id) {
                        indicator.active = active;
                    }
                }
                crate::data::WindowContent::Dashboard { indicators } => {
                    let mut found = false;
                    for (indicator_id, value) in indicators.iter_mut() {
                        if indicator_id.eq_ignore_ascii_case(id) {
                            *value = if active { 1 } else { 0 };
                            found = true;
                            break;
                        }
                    }
                    // Same claim guard as the server-indicator path: don't
                    // auto-add an id a combined indicator template owns.
                    if !found && active && !claimed {
                        indicators.push((id.to_string(), 1));
                    }
                }
                _ => {}
            }
        }
        self.needs_render = true;
    }
}
