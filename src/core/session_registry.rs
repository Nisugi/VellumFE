//! Session registry: which VellumFE instances are running on this machine.
//!
//! Each instance writes `~/.vellum-fe/web-sessions/<pid>.json` when its web
//! sidecar binds, and removes it on clean shutdown. Crashed instances leave
//! their file behind, so reads garbage-collect entries whose pid is gone.
//!
//! Lives in core rather than the web frontend: the file IS written when the
//! sidecar starts, but it is plain filesystem discovery, and the
//! multi-account hub reads it to find sibling instances. Core must not import
//! from `frontend/` (see tests/architecture.rs), so the shared thing lives
//! here and the server re-exports it.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionEntry {
    pub character: String,
    pub port: u16,
    pub pid: u32,
    pub started_at: String,
}

/// The registry directory, resolved once. Creation is `write_entry`'s job --
/// readers only list, and the old shape issued a create_dir_all syscall on
/// every 5-second discovery poll for a directory that exists after first use.
pub fn dir() -> Option<PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        crate::config::Config::base_dir()
            .ok()
            .map(|base| base.join("web-sessions"))
    })
    .clone()
}

fn entry_path(pid: u32) -> Option<PathBuf> {
    Some(dir()?.join(format!("{pid}.json")))
}

pub fn write_entry(port: u16, character: &str) {
    let pid = std::process::id();
    let entry = SessionEntry {
        character: character.to_string(),
        port,
        pid,
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    let Some(path) = entry_path(pid) else { return };
    // The one path that needs the directory to exist.
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&entry) {
        if let Err(e) = fs::write(&path, json) {
            tracing::warn!("failed to write session registry entry: {e}");
        }
    }
}

/// Remove this instance's entry (clean shutdown).
pub fn remove_entry() {
    if let Some(path) = entry_path(std::process::id()) {
        let _ = fs::remove_file(path);
    }
}

/// All current entries. Also garbage-collects files whose pid is no
/// longer running (crashed instances).
pub fn list_and_gc() -> Vec<SessionEntry> {
    let Some(dir) = dir() else { return Vec::new() };
    let Ok(read) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    // Read every entry first, then ask about all the pids at once: the
    // liveness probe refreshes the whole process table per call, so asking
    // pid-by-pid would rescan for each file.
    let mut candidates: Vec<(PathBuf, SessionEntry)> = Vec::new();
    for file in read.flatten() {
        let path = file.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        match serde_json::from_str::<SessionEntry>(&text) {
            Ok(entry) => candidates.push((path, entry)),
            // Unparseable file: a truncated write or an older format. Either
            // way it names no live session, so drop it.
            Err(_) => {
                let _ = fs::remove_file(&path);
            }
        }
    }

    let pids: Vec<u32> = candidates.iter().map(|(_, e)| e.pid).collect();
    let live = crate::process_probe::live_pids(&pids);

    let mut entries = Vec::new();
    for (path, entry) in candidates {
        if live.contains(&entry.pid) {
            entries.push(entry);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
    entries.sort_by(|a, b| a.character.cmp(&b.character));
    entries
}
