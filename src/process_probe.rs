//! Feature-gated process liveness probe.
//!
//! `sysinfo` is a desktop-only dependency, and core must compile on Android
//! with `--no-default-features` (see `tests/architecture.rs`). This is the
//! designated site for that crate, in the same spirit as `crate::clipboard`,
//! `crate::sound` and `crate::tts`: core asks the question, this answers it.
//!
//! Used by the session registry to garbage-collect entries left behind by
//! instances that crashed rather than exiting cleanly.

/// Which of the given pids are still running.
///
/// Takes the whole set at once because the desktop implementation refreshes
/// the process table once per call; asking pid-by-pid would rescan for each.
#[cfg(feature = "desktop")]
pub fn live_pids(pids: &[u32]) -> std::collections::HashSet<u32> {
    let mut system = sysinfo::System::new();
    system.refresh_processes();
    pids.iter()
        .copied()
        .filter(|pid| system.process(sysinfo::Pid::from_u32(*pid)).is_some())
        .collect()
}

/// Without process inspection (Android runs as a single-process app), only
/// our own pid can be live; any other entry is a leftover from a previous
/// run of this same app.
#[cfg(not(feature = "desktop"))]
pub fn live_pids(pids: &[u32]) -> std::collections::HashSet<u32> {
    let own = std::process::id();
    pids.iter().copied().filter(|pid| *pid == own).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_own_pid_is_always_live() {
        // True on both sides of the feature gate, which is what makes this a
        // safe seam: the registry never garbage-collects its own entry.
        let own = std::process::id();
        assert!(live_pids(&[own]).contains(&own));
    }

    #[test]
    fn an_absent_pid_is_not_live() {
        // Not pid 0: on Windows that is the System Idle Process, which is
        // genuinely present, so it would report live. Use a value above the
        // platform maximum instead -- Windows pids are DWORDs but allocated
        // far below this, and Linux caps well under it via pid_max.
        let absent = u32::MAX - 1;
        assert!(!live_pids(&[absent]).contains(&absent));
    }

    #[test]
    fn a_mixed_batch_reports_only_the_live_ones() {
        // The registry hands over every pid at once, so the batch shape is
        // what actually matters: live entries kept, dead ones dropped.
        let own = std::process::id();
        let absent = u32::MAX - 1;
        let live = live_pids(&[own, absent]);
        assert!(live.contains(&own));
        assert!(!live.contains(&absent));
        assert_eq!(live.len(), 1);
    }
}
