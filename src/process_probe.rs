//! Feature-gated process liveness probe.
//!
//! `sysinfo` is a desktop-only dependency, and core must compile on Android
//! with `--no-default-features` (see `tests/architecture.rs`). This is the
//! designated site for that crate, in the same spirit as `crate::clipboard`,
//! `crate::sound` and `crate::tts`: core asks the question, this answers it.
//!
//! Used by the session registry to garbage-collect entries left behind by
//! instances that crashed rather than exiting cleanly, and to discover the
//! install directory of a running local Lich process without searching the
//! filesystem or interpreting a launcher shell command.

#[cfg(any(feature = "desktop", test))]
use std::path::Path;
use std::path::PathBuf;

/// Return the install directories advertised by running local Lich processes.
///
/// Process arguments are already split by the operating system, so this only
/// recognizes an argument whose basename is the Lich entrypoint. It never
/// tokenizes or evaluates shell text. A candidate must also contain `data/`,
/// which avoids treating an unrelated `lich.rbw` filename as an install.
#[cfg(feature = "desktop")]
pub fn running_lich_install_dirs() -> Vec<PathBuf> {
    let system = sysinfo::System::new_all();
    let mut dirs = std::collections::BTreeSet::new();
    for process in system.processes().values() {
        if let Some(dir) = lich_install_dir_from_argv(process.cmd(), process.cwd()) {
            dirs.insert(dir);
        }
    }
    dirs.into_iter().collect()
}

/// Mobile builds cannot have a local Lich process beside the app. Their map
/// source is an explicit path or a downloaded mapdb release.
#[cfg(not(feature = "desktop"))]
pub fn running_lich_install_dirs() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(any(feature = "desktop", test))]
fn lich_install_dir_from_argv(args: &[String], cwd: Option<&Path>) -> Option<PathBuf> {
    args.iter().find_map(|arg| {
        let entrypoint = Path::new(arg);
        let name = entrypoint.file_name()?.to_string_lossy();
        if !name.eq_ignore_ascii_case("lich.rbw") && !name.eq_ignore_ascii_case("lich.rb") {
            return None;
        }

        let entrypoint = if entrypoint.is_absolute() {
            entrypoint.to_path_buf()
        } else {
            cwd?.join(entrypoint)
        };
        let install = entrypoint.parent()?;
        if !entrypoint.is_file() || !install.join("data").is_dir() {
            return None;
        }
        Some(std::fs::canonicalize(install).unwrap_or_else(|_| install.to_path_buf()))
    })
}

/// Which of the given pids are still running.
///
/// Takes the whole set at once because the desktop implementation refreshes
/// the process table once per call; asking pid-by-pid would rescan for each.
#[cfg(feature = "desktop")]
pub fn live_pids(pids: &[u32]) -> std::collections::HashSet<u32> {
    use std::sync::Mutex;
    // One System, reused: this runs on a 5-second discovery poll for the
    // app's whole lifetime, and the old shape built a fresh System and
    // walked the ENTIRE process table each call to answer "are these five
    // pids alive". refresh_process is O(asked pids) and its return value is
    // the liveness answer.
    static SYSTEM: Mutex<Option<sysinfo::System>> = Mutex::new(None);
    let mut guard = SYSTEM.lock().expect("process probe poisoned");
    let system = guard.get_or_insert_with(sysinfo::System::new);
    pids.iter()
        .copied()
        .filter(|pid| system.refresh_process(sysinfo::Pid::from_u32(*pid)))
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

    #[test]
    fn recognizes_absolute_lich_entrypoint_with_spaces() {
        let root = tempfile::tempdir().unwrap();
        let install = root.path().join("GemStone IV").join("Lich5");
        std::fs::create_dir_all(install.join("data")).unwrap();
        let entrypoint = install.join("lich.rbw");
        std::fs::write(&entrypoint, "").unwrap();

        let args = vec![
            "/usr/bin/ruby".to_string(),
            entrypoint.to_string_lossy().to_string(),
            "--detachable-client=8000".to_string(),
        ];
        assert_eq!(
            lich_install_dir_from_argv(&args, None),
            Some(std::fs::canonicalize(install).unwrap())
        );
    }

    #[test]
    fn resolves_relative_lich_entrypoint_against_process_cwd() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("data")).unwrap();
        std::fs::write(root.path().join("lich.rb"), "").unwrap();

        assert_eq!(
            lich_install_dir_from_argv(&["lich.rb".to_string()], Some(root.path())),
            Some(std::fs::canonicalize(root.path()).unwrap())
        );
    }

    #[test]
    fn does_not_parse_lich_path_out_of_shell_text() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("data")).unwrap();
        let entrypoint = root.path().join("lich.rbw");
        std::fs::write(&entrypoint, "").unwrap();
        let args = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("ruby '{}' --detachable-client=8000", entrypoint.display()),
        ];

        assert_eq!(lich_install_dir_from_argv(&args, Some(root.path())), None);
    }
}
