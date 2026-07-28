//! The automation lease: one answer to "who is driving the game
//! connection?" (foreach/sorter plan P2, 2026-07-28).
//!
//! Ownership is **derived** from live services (travel today, foreach
//! later) instead of stored in a parallel slot, so it can never drift
//! from reality. The composition rule that makes the chain meaningful:
//! automation features invoke each other's core functions under the root
//! owner (`.foreach` will call the travel executor as a function), never
//! by re-dispatching dot-command strings — so `automation_chain()` reads
//! root-first, e.g. `["foreach: gems in pack", "go2 -> room 3854"]`.
//!
//! Contract for every top-level automation entry point:
//! - check `automation_blocked_by(KIND)` before starting; a different
//!   root owner means a clean rejection naming it, never interleaving;
//! - same-kind re-entry stays allowed (`.go2 X` mid-trip retargets, as
//!   it always has);
//! - `.stop` (and feature-specific cancels like Esc) cancel the root,
//!   which tears down everything running under it.

use super::state::AppCore;

/// One link in the ownership chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationOwner {
    /// Feature key gates compare against ("go2", "foreach").
    pub kind: &'static str,
    /// Human description for rejections and `.stop` ("go2 -> room 3854").
    pub desc: String,
}

impl AppCore {
    /// Ownership chain, root first. Empty when nothing is driving.
    pub fn automation_chain(&self) -> Vec<AutomationOwner> {
        let mut chain = Vec::new();
        // Root-first: a travel leg driven by foreach reads as its child.
        if let Some(task) = self.foreach.task() {
            chain.push(AutomationOwner {
                kind: "foreach",
                desc: format!("foreach {}", task.desc),
            });
        }
        if let Some(task) = self.travel.task() {
            chain.push(AutomationOwner {
                kind: "go2",
                desc: format!("go2 -> room {}", task.destination),
            });
        }
        chain
    }

    /// The root owner, or None when idle.
    pub fn automation_owner(&self) -> Option<AutomationOwner> {
        self.automation_chain().into_iter().next()
    }

    /// The gate for top-level starts: Some(owner) when a DIFFERENT
    /// feature than `kind` currently owns the connection.
    pub fn automation_blocked_by(&self, kind: &str) -> Option<AutomationOwner> {
        self.automation_owner().filter(|owner| owner.kind != kind)
    }

    /// `.stop`: cancel the root owner (and with it anything it drives).
    /// Returns the cancelled owner's description, or None if idle.
    pub fn stop_automation(&mut self) -> Option<String> {
        let owner = self.automation_owner()?;
        match owner.kind {
            "go2" => {
                self.travel.stop();
            }
            "foreach" => {
                // Cancel the whole chain: the run plus any travel leg it
                // (or the user) started underneath.
                self.foreach.stop();
                self.travel.stop();
            }
            other => {
                tracing::warn!("stop_automation: unknown owner kind '{}'", other);
            }
        }
        Some(owner.desc)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::mapdb::MapDb;
    use crate::core::travel::TravelTask;

    fn db() -> MapDb {
        MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": "east"}, "timeto": {"2": 1.0}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[B]"],
                 "wayto": {"1": "west"}, "timeto": {"1": 1.0}, "paths": ""}
            ]"#,
        )
        .unwrap()
    }

    #[test]
    fn chain_derives_from_travel_and_stop_clears_it() {
        let mut core = crate::core::AppCore::new_for_test();
        assert!(core.automation_chain().is_empty());
        assert!(core.automation_owner().is_none());
        assert!(core.stop_automation().is_none());

        let db = db();
        let task = TravelTask::start(&db, 1, 2, 0).unwrap();
        core.travel.set_task(task);

        let owner = core.automation_owner().expect("travel owns the chain");
        assert_eq!(owner.kind, "go2");
        assert_eq!(owner.desc, "go2 -> room 2");
        // go2 itself is not blocked (retargeting), other features are.
        assert!(core.automation_blocked_by("go2").is_none());
        assert_eq!(
            core.automation_blocked_by("foreach").map(|o| o.desc),
            Some("go2 -> room 2".to_string())
        );

        let stopped = core.stop_automation();
        assert_eq!(stopped.as_deref(), Some("go2 -> room 2"));
        assert!(core.automation_owner().is_none());
        assert!(!core.travel.is_traveling());
    }
}
