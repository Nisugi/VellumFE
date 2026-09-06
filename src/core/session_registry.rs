//! Session registry: which VellumFE instances are running on this machine.
//!
//! Each instance writes one process entry when its web sidecar binds and
//! removes it when that listener stops.  The registry lives in the user's
//! machine-local runtime/data directory rather than `VELLUM_FE_DIR`: launcher
//! profiles may use different data roots, but they still need one shared view
//! of which processes and Lich detachable-client endpoints are already owned.
//! Crashed instances leave their file behind, so reads garbage-collect entries
//! whose pid is gone. For one release, reads also merge the former
//! `~/.vellum-fe/web-sessions` registry (and the active `VELLUM_FE_DIR`
//! equivalent) so an older running Vellum remains visible during upgrade.
//!
//! Lives in core rather than the web frontend: the file IS written when the
//! sidecar starts, but it is plain filesystem discovery, and the
//! multi-account hub reads it to find sibling instances. Core must not import
//! from `frontend/` (see tests/architecture.rs), so the shared thing lives
//! here and the server re-exports it.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SessionConnectionIdentity {
    Direct {
        game: String,
        /// Direct logins are scoped to an account as well as a game.  Keep a
        /// default for registry entries written before this field existed.
        #[serde(default)]
        account: String,
    },
    Lich {
        host: String,
        port: u16,
    },
}

impl PartialEq for SessionConnectionIdentity {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Direct {
                    game: left_game,
                    account: left_account,
                },
                Self::Direct {
                    game: right_game,
                    account: right_account,
                },
            ) => {
                canonical_direct_connection(left_game, left_account)
                    == canonical_direct_connection(right_game, right_account)
            }
            (
                Self::Lich {
                    host: left_host,
                    port: left_port,
                },
                Self::Lich {
                    host: right_host,
                    port: right_port,
                },
            ) => left_host == right_host && left_port == right_port,
            _ => false,
        }
    }
}

impl Eq for SessionConnectionIdentity {}

#[derive(PartialEq, Eq)]
struct CanonicalDirectConnection {
    game: String,
    account: String,
}

/// Canonical direct connection used by identity equality and ownership.
///
/// Saved profiles use friendly world names while CLI overrides use eAccess
/// wire codes. Unknown names also connect to GS Prime (the direct-login
/// fallback), so they must claim that same endpoint rather than a distinct
/// one. Account normalization also keeps older registry values compatible
/// with identities produced by current profile construction.
fn canonical_direct_connection(game: &str, account: &str) -> CanonicalDirectConnection {
    let game = crate::network::DirectConnectConfig::game_name_to_code(game).to_ascii_lowercase();
    CanonicalDirectConnection {
        game,
        account: account.trim().to_ascii_lowercase(),
    }
}

/// Immutable identity captured by the child from the launcher profile it was
/// actually started with.  Never reconstruct this by joining a character name
/// back to today's launcher.toml: profiles can be renamed or edited while a
/// child remains alive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLaunchIdentity {
    pub profile: String,
    pub character: String,
    pub connection: SessionConnectionIdentity,
}

impl SessionLaunchIdentity {
    pub fn from_profile(
        profile_name: &str,
        profile: &crate::config::profiles::LauncherProfile,
    ) -> Self {
        let connection = match profile.mode {
            crate::config::profiles::LaunchMode::Direct => {
                let canonical = canonical_direct_connection(&profile.game, &profile.account);
                SessionConnectionIdentity::Direct {
                    game: canonical.game,
                    account: canonical.account,
                }
            }
            crate::config::profiles::LaunchMode::Lich => SessionConnectionIdentity::Lich {
                host: normalize_host(&profile.host),
                port: profile.port,
            },
        };
        Self {
            profile: profile_name.to_string(),
            character: profile.character.trim().to_string(),
            connection,
        }
    }
}

/// Normalize only what endpoint equality needs.  All loopback spellings are
/// one machine-local endpoint; DNS host names are case-insensitive.
pub fn normalize_host(host: &str) -> String {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]") {
        "loopback".to_string()
    } else {
        host
    }
}

/// Convert a listener bind address into a host reachable by this machine's
/// launcher and browser. Wildcard listeners are not valid URL destinations,
/// so they use the matching loopback family.
pub fn control_host_for_bind(bind: &str) -> String {
    let host = bind
        .trim()
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or_else(|| bind.trim());
    match host {
        "" | "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "::1".to_string(),
        _ => host.to_string(),
    }
}

/// Format an HTTP URL for a session control host, including the brackets
/// required around IPv6 literals.
pub fn control_url(host: &str, port: u16, path: &str) -> String {
    let host = control_host_for_bind(host);
    let authority_host = if host.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{host}]")
    } else {
        host
    };
    format!("http://{authority_host}:{port}{path}")
}

impl SessionEntry {
    /// Authenticated local-control URL, backward compatible with registry
    /// entries created before the control host was published.
    pub fn control_url(&self, path: &str) -> String {
        control_url(
            self.control_host.as_deref().unwrap_or("127.0.0.1"),
            self.port,
            path,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionLifecycleState {
    #[default]
    Unknown,
    Idle,
    Connecting,
    Connected,
    Reconnecting,
    Disconnected,
}

impl SessionLifecycleState {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Idle => 1,
            Self::Connecting => 2,
            Self::Connected => 3,
            Self::Reconnecting => 4,
            Self::Disconnected => 5,
        }
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Idle,
            2 => Self::Connecting,
            3 => Self::Connected,
            4 => Self::Reconnecting,
            5 => Self::Disconnected,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEntry {
    pub character: String,
    pub port: u16,
    /// Host the local launcher should use for authenticated HTTP controls.
    /// Missing on older entries, which were always loopback-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_host: Option<String>,
    pub pid: u32,
    pub started_at: String,
    /// Per-runtime identifier, distinct even when the OS later reuses a PID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// OS process start time used with `pid` to reject PID-reuse ghosts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_started_at: Option<u64>,
    /// Missing only on entries left by an older Vellum build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<SessionLaunchIdentity>,
    /// The process's effective data root locates its pairing token when the
    /// launcher reopens an existing browser client.  The token itself is never
    /// written to this registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_root: Option<PathBuf>,
    #[serde(default)]
    pub lifecycle: SessionLifecycleState,
}

static LAUNCH_IDENTITY: OnceLock<SessionLaunchIdentity> = OnceLock::new();
static LIFECYCLE: AtomicU8 = AtomicU8::new(SessionLifecycleState::Unknown.as_u8());
static ENTRY_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn set_launch_identity(identity: SessionLaunchIdentity) {
    if LAUNCH_IDENTITY.set(identity.clone()).is_err() && LAUNCH_IDENTITY.get() != Some(&identity) {
        tracing::warn!("launcher identity was already set; keeping the original child identity");
    }
}

fn current_launch_identity() -> Option<SessionLaunchIdentity> {
    LAUNCH_IDENTITY.get().cloned()
}

/// The registry directory, resolved once. Creation is [`register`]'s job --
/// readers only list, and the old shape issued a create_dir_all syscall on
/// every 5-second discovery poll for a directory that exists after first use.
pub fn dir() -> Option<PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        std::env::var_os("VELLUM_FE_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .map(|base| PathBuf::from(base).join("web-sessions"))
            .or_else(|| {
                dirs::data_local_dir()
                    .or_else(dirs::cache_dir)
                    .map(|base| base.join("vellum-fe").join("runtime").join("web-sessions"))
            })
    })
    .clone()
}

/// Registry roots used before the machine-local registry was introduced.
///
/// Keep this compatibility read for one release containing the migration.
/// The configured root protects users running an old build with
/// `VELLUM_FE_DIR`; the home root protects the normal old default even when a
/// newer launcher is currently using an override. Saved launcher profiles may
/// select their own data roots, so include each profile's effective legacy
/// registry too.
fn legacy_dirs() -> Vec<PathBuf> {
    crate::config::profiles::known_data_roots()
        .into_iter()
        .map(|root| root.join("web-sessions"))
        .collect()
}

fn entry_path(pid: u32, instance_id: &str) -> Option<PathBuf> {
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(instance_id.as_bytes());
    Some(dir()?.join(format!("{pid}-{digest:x}.json")))
}

fn build_entry(port: u16, control_host: &str, character: &str, instance_id: &str) -> SessionEntry {
    let pid = std::process::id();
    SessionEntry {
        character: character.to_string(),
        port,
        control_host: Some(control_host_for_bind(control_host)),
        pid,
        started_at: chrono::Utc::now().to_rfc3339(),
        instance_id: Some(instance_id.to_string()),
        process_started_at: crate::process_probe::process_start_time(pid),
        launch: current_launch_identity(),
        data_root: crate::config::Config::base_dir().ok(),
        lifecycle: SessionLifecycleState::from_u8(LIFECYCLE.load(Ordering::Relaxed)),
    }
}

fn write_entry_path(path: &std::path::Path, entry: &SessionEntry) -> bool {
    let Ok(json) = serde_json::to_string_pretty(entry) else {
        return false;
    };
    if let Err(e) = write_entry_atomic(path, json.as_bytes()) {
        tracing::warn!("failed to write session registry entry: {e}");
        return false;
    }
    true
}

/// Atomically replace an ephemeral registry entry without the persistent
/// backups used for user-authored configuration. A unique sibling keeps two
/// process tasks from truncating each other's temporary file; syncing it
/// before rename ensures a visible entry is always complete after a crash.
fn write_entry_atomic(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut temp_path = None;
    let mut temp_file = None;
    for _ in 0..16 {
        let sequence = ENTRY_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".tmp-{}-{sequence}", std::process::id()));
        let candidate = PathBuf::from(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp_path = Some(candidate);
                temp_file = Some(file);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    let temp_path = temp_path.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate a unique registry temporary file",
        )
    })?;
    let mut temp_file = temp_file.expect("temporary path and file are created together");
    let result = (|| {
        temp_file.write_all(contents)?;
        temp_file.sync_all()?;
        drop(temp_file);
        fs::rename(&temp_path, path)?;
        sync_parent_directory(path);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    if result.is_ok() {
        remove_legacy_entry_sidecars(path);
    }
    result
}

fn sync_parent_directory(path: &std::path::Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn sidecar_path(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn remove_legacy_entry_sidecars(path: &std::path::Path) {
    // These deterministic names came from config::write_atomic. This writer
    // never uses them, so removing them cannot race one of its own updates.
    let _ = fs::remove_file(sidecar_path(path, ".bak"));
    let _ = fs::remove_file(sidecar_path(path, ".tmp"));
}

/// A listener-owned registration.  Dropping the serving future removes the
/// entry even when its caller returns early, so `.exit` and server failures do
/// not depend on a later best-effort cleanup call in `main`.
pub struct SessionRegistration {
    path: PathBuf,
}

impl Drop for SessionRegistration {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        remove_legacy_entry_sidecars(&self.path);
    }
}

pub fn register(
    port: u16,
    control_host: &str,
    character: &str,
    instance_id: &str,
) -> anyhow::Result<SessionRegistration> {
    let entry = build_entry(port, control_host, character, instance_id);
    let path = entry_path(entry.pid, instance_id)
        .ok_or_else(|| anyhow::anyhow!("Could not resolve Vellum runtime directory"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&entry)?;
    write_entry_atomic(&path, json.as_bytes())?;
    if let Some(identity) = entry.launch.as_ref() {
        clear_launch_claim(identity);
    }
    Ok(SessionRegistration { path })
}

/// Remove this instance's entry (clean shutdown).
pub fn remove_entry() {
    let Some(dir) = dir() else { return };
    let prefix = format!("{}-", std::process::id());
    if let Ok(files) = fs::read_dir(dir) {
        for file in files.flatten() {
            if file.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = fs::remove_file(file.path());
            }
        }
    }
}

/// Update the process entry's mutable state while preserving the immutable
/// launch identity.  The atomic also covers the startup race where the
/// supervisor publishes state before the web listener creates its entry.
pub fn set_lifecycle(state: SessionLifecycleState) {
    LIFECYCLE.store(state.as_u8(), Ordering::Relaxed);
    let Some(dir) = dir() else { return };
    let prefix = format!("{}-", std::process::id());
    let Ok(files) = fs::read_dir(dir) else { return };
    for file in files.flatten() {
        if !file.file_name().to_string_lossy().starts_with(&prefix) {
            continue;
        }
        let path = file.path();
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut entry) = serde_json::from_str::<SessionEntry>(&text) else {
            continue;
        };
        entry.lifecycle = state;
        let _ = write_entry_path(&path, &entry);
    }
}

/// All current entries. Also garbage-collects files whose pid is no
/// longer running (crashed instances).
pub fn list_and_gc() -> Vec<SessionEntry> {
    let current = dir();
    let legacy = legacy_dirs();
    list_and_gc_roots(current.as_deref(), &legacy, |pids| {
        crate::process_probe::live_pids(pids)
    })
}

#[cfg(test)]
fn list_and_gc_in(
    dir: &std::path::Path,
    live_pids: impl FnOnce(&[u32]) -> std::collections::HashSet<u32>,
) -> Vec<SessionEntry> {
    list_and_gc_directories(
        &[RegistryRoot::new(dir, RegistryRootKind::Current)],
        unix_now(),
        live_pids,
    )
}

fn list_and_gc_roots(
    current: Option<&std::path::Path>,
    legacy: &[PathBuf],
    live_pids: impl FnOnce(&[u32]) -> std::collections::HashSet<u32>,
) -> Vec<SessionEntry> {
    list_and_gc_roots_at(current, legacy, unix_now(), live_pids)
}

fn list_and_gc_roots_at(
    current: Option<&std::path::Path>,
    legacy: &[PathBuf],
    now: u64,
    live_pids: impl FnOnce(&[u32]) -> std::collections::HashSet<u32>,
) -> Vec<SessionEntry> {
    let mut roots: Vec<RegistryRoot> = Vec::new();
    if let Some(current) = current {
        add_registry_root(&mut roots, current, RegistryRootKind::Current);
    }
    for root in legacy {
        add_registry_root(&mut roots, root, RegistryRootKind::Legacy);
    }

    list_and_gc_directories(&roots, now, live_pids)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegistryRootKind {
    Current,
    Legacy,
}

impl RegistryRootKind {
    fn merge(self, other: Self) -> Self {
        if self == Self::Legacy || other == Self::Legacy {
            Self::Legacy
        } else {
            Self::Current
        }
    }
}

#[derive(Clone, Debug)]
struct RegistryRoot {
    path: PathBuf,
    identity: PathBuf,
    kind: RegistryRootKind,
}

impl RegistryRoot {
    fn new(path: &Path, kind: RegistryRootKind) -> Self {
        Self {
            path: path.to_path_buf(),
            identity: fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
            kind,
        }
    }
}

fn add_registry_root(roots: &mut Vec<RegistryRoot>, path: &Path, kind: RegistryRootKind) {
    let candidate = RegistryRoot::new(path, kind);
    if let Some(existing) = roots
        .iter_mut()
        .find(|existing| existing.identity == candidate.identity)
    {
        existing.kind = existing.kind.merge(kind);
    } else {
        roots.push(candidate);
    }
}

#[derive(Debug)]
struct RegistryCandidate {
    path: PathBuf,
    entry: SessionEntry,
    root_kind: RegistryRootKind,
}

fn list_and_gc_directories(
    directories: &[RegistryRoot],
    now: u64,
    live_pids: impl FnOnce(&[u32]) -> std::collections::HashSet<u32>,
) -> Vec<SessionEntry> {
    // Read every entry first, then ask about all the pids at once: the
    // liveness probe refreshes the whole process table per call, so asking
    // pid-by-pid would rescan for each file.
    let mut candidates: Vec<RegistryCandidate> = Vec::new();
    for directory in directories {
        let Ok(read) = fs::read_dir(&directory.path) else {
            continue;
        };
        for file in read.flatten() {
            let path = file.path();
            if cleanup_entry_cache_artifact(&path, now) {
                continue;
            }
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            match serde_json::from_str::<SessionEntry>(&text) {
                Ok(entry) => candidates.push(RegistryCandidate {
                    path,
                    entry,
                    root_kind: directory.kind,
                }),
                // Current entries are atomically published and therefore
                // malformed only after a failed/crashed write. Old builds
                // wrote directly into the legacy path, so a new reader may
                // observe their file mid-write; give those files a bounded
                // opportunity to become valid before cleaning them up.
                Err(_) => {
                    if directory.kind == RegistryRootKind::Current
                        || file_is_stale(&path, now, LEGACY_REGISTRY_GRACE_SECS)
                    {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }

    let pids: Vec<u32> = candidates
        .iter()
        .map(|candidate| candidate.entry.pid)
        .collect();
    let live = live_pids(&pids);

    let mut entries = Vec::new();
    for candidate in candidates {
        let RegistryCandidate {
            path,
            entry,
            root_kind,
        } = candidate;
        let same_process_instance = entry.process_started_at.is_none_or(|expected| {
            crate::process_probe::process_start_time(entry.pid) == Some(expected)
        });
        if live.contains(&entry.pid) && same_process_instance {
            entries.push(entry);
        } else if root_kind == RegistryRootKind::Current
            || file_is_stale(&path, now, LEGACY_REGISTRY_GRACE_SECS)
        {
            let _ = fs::remove_file(&path);
        }
    }
    entries.sort_by(|a, b| a.character.cmp(&b.character));
    entries
}

const ENTRY_TEMP_TTL_SECS: u64 = 120;
const LEGACY_REGISTRY_GRACE_SECS: u64 = ENTRY_TEMP_TTL_SECS;

/// Remove only sidecars whose base name has the exact current session-entry
/// shape. Backups are never live. Temporary files may belong to a concurrent
/// writer, so they are removed only after a bounded crash-recovery age.
fn cleanup_entry_cache_artifact(path: &std::path::Path, now: u64) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let (base, remove) = if let Some(base) = name.strip_suffix(".bak") {
        (base, true)
    } else if let Some(base) = name.strip_suffix(".tmp") {
        (base, file_is_stale(path, now, ENTRY_TEMP_TTL_SECS))
    } else if let Some((base, _unique)) = name.split_once(".tmp-") {
        (base, file_is_stale(path, now, ENTRY_TEMP_TTL_SECS))
    } else {
        return false;
    };
    if !is_session_entry_file_name(base) {
        return false;
    }
    if remove {
        let _ = fs::remove_file(path);
    }
    true
}

fn is_session_entry_file_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".json") else {
        return false;
    };
    let Some((pid, digest)) = stem.split_once('-') else {
        return false;
    };
    !pid.is_empty()
        && pid.bytes().all(|byte| byte.is_ascii_digit())
        && digest.len() == 40
        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn file_is_stale(path: &std::path::Path, now: u64, ttl: u64) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|modified| now.saturating_sub(modified.as_secs()) >= ttl)
}

const CLAIM_TTL_SECS: u64 = 120;

#[derive(Debug, Serialize, Deserialize)]
struct ClaimRecord {
    profile: String,
    character: String,
    created_at: u64,
}

pub struct LaunchClaim {
    path: PathBuf,
    remove_on_drop: bool,
}

impl LaunchClaim {
    /// Leave the claim for the child to clear when its authoritative registry
    /// entry is published.  If the child never starts, the TTL makes it
    /// recoverable without killing or probing any game process.
    pub fn persist_until_registration(mut self) {
        self.remove_on_drop = false;
    }
}

impl Drop for LaunchClaim {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub enum LaunchClaimResult {
    Acquired(LaunchClaim),
    Existing { profile: String, character: String },
}

fn claim_resource(identity: &SessionLaunchIdentity) -> String {
    match &identity.connection {
        SessionConnectionIdentity::Lich { host, port } => format!("lich:{host}:{port}"),
        SessionConnectionIdentity::Direct { game, account } => {
            let canonical = canonical_direct_connection(game, account);
            format!(
                "direct:{}:{}:{}",
                canonical.game,
                canonical.account,
                identity.character.to_ascii_lowercase()
            )
        }
    }
}

fn claim_path_in(dir: &std::path::Path, identity: &SessionLaunchIdentity) -> PathBuf {
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(claim_resource(identity).as_bytes());
    dir.join("launch-claims").join(format!("{digest:x}.json"))
}

fn claim_lock_path(claim_path: &std::path::Path) -> PathBuf {
    sidecar_path(claim_path, ".lock")
}

pub fn acquire_launch_claim(identity: &SessionLaunchIdentity) -> anyhow::Result<LaunchClaimResult> {
    let dir = dir().ok_or_else(|| anyhow::anyhow!("Could not resolve Vellum runtime directory"))?;
    acquire_launch_claim_in(&dir, identity, unix_now())
}

fn acquire_launch_claim_in(
    dir: &std::path::Path,
    identity: &SessionLaunchIdentity,
    now: u64,
) -> anyhow::Result<LaunchClaimResult> {
    let path = claim_path_in(dir, identity);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Serialize claim inspection and replacement on a stable inode. The lock
    // file deliberately remains: unlinking it could let two contenders lock
    // different inodes. The JSON record still owns the TTL and user-facing
    // identity; this lock covers only the short acquisition transaction.
    let claim_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(claim_lock_path(&path))?;
    claim_lock.lock()?;
    let record = ClaimRecord {
        profile: identity.profile.clone(),
        character: identity.character.clone(),
        created_at: now,
    };
    let body = serde_json::to_vec(&record)?;

    for _ in 0..2 {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&body)?;
                file.sync_all()?;
                return Ok(LaunchClaimResult::Acquired(LaunchClaim {
                    path,
                    remove_on_drop: true,
                }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = fs::read(&path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<ClaimRecord>(&bytes).ok());
                let stale = existing.as_ref().map_or_else(
                    || file_is_stale(&path, now, CLAIM_TTL_SECS),
                    |claim| now.saturating_sub(claim.created_at) >= CLAIM_TTL_SECS,
                );
                if stale {
                    let _ = fs::remove_file(&path);
                    continue;
                }
                if let Some(existing) = existing {
                    return Ok(LaunchClaimResult::Existing {
                        profile: existing.profile,
                        character: existing.character,
                    });
                }
                anyhow::bail!(
                    "A launch reservation is being created or is not old enough to recover"
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("Could not acquire launch reservation")
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn clear_launch_claim(identity: &SessionLaunchIdentity) {
    let Some(dir) = dir() else { return };
    let _ = fs::remove_file(claim_path_in(&dir, identity));
}

#[derive(Debug, Serialize, Deserialize)]
struct EndpointLeaseRecord {
    pid: u32,
    process_started_at: Option<u64>,
    profile: String,
    character: String,
}

/// Process-lifetime ownership of a game connection resource.  This is
/// acquired by the child itself, so manual `--launch-profile` starts and two
/// independent launchers are protected even if launcher preflight races.
pub struct EndpointLease {
    /// The OS releases the exclusive lock when this handle closes.  The lock
    /// file itself deliberately remains: unlinking a locked inode lets a
    /// third process create and lock a different inode at the same path,
    /// producing two simultaneous owners.
    _file: std::fs::File,
}

pub fn acquire_endpoint_lease(identity: &SessionLaunchIdentity) -> anyhow::Result<EndpointLease> {
    let root =
        dir().ok_or_else(|| anyhow::anyhow!("Could not resolve Vellum runtime directory"))?;
    acquire_endpoint_lease_in(&root, identity, std::process::id())
}

fn acquire_endpoint_lease_in(
    root: &std::path::Path,
    identity: &SessionLaunchIdentity,
    pid: u32,
) -> anyhow::Result<EndpointLease> {
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(claim_resource(identity).as_bytes());
    let path = root
        .join("endpoint-leases")
        .join(format!("{digest:x}.lock"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let record = EndpointLeaseRecord {
        pid,
        process_started_at: crate::process_probe::process_start_time(pid),
        profile: identity.profile.clone(),
        character: identity.character.clone(),
    };
    // Owner metadata lives in a sidecar, NOT inside the lock file: on
    // Windows an exclusive lock blocks reads from every other handle
    // (ERROR_LOCK_VIOLATION), so a loser could never learn who owns the
    // endpoint if the record sat behind the lock itself.
    let owner_path = path.with_extension("owner.json");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)?;
    if let Err(lock_error) = file.try_lock() {
        // The winning process writes the sidecar immediately after taking
        // the OS lock. Give that tiny metadata write a bounded opportunity
        // to finish, and ignore a record left by an older dead owner.
        for _ in 0..5 {
            let existing = fs::read(&owner_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<EndpointLeaseRecord>(&bytes).ok())
                .filter(endpoint_owner_is_live);
            if let Some(existing) = existing {
                anyhow::bail!(
                    "{} ({}) already owns this connection endpoint",
                    existing.character,
                    existing.profile
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        anyhow::bail!("another Vellum process already owns this connection endpoint: {lock_error}")
    }

    let body = serde_json::to_vec(&record)?;
    let tmp_path = path.with_extension(format!("owner.tmp-{pid}"));
    fs::write(&tmp_path, &body)?;
    if let Err(error) = fs::rename(&tmp_path, &owner_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(error.into());
    }
    Ok(EndpointLease { _file: file })
}

fn endpoint_owner_is_live(owner: &EndpointLeaseRecord) -> bool {
    crate::process_probe::live_pids(&[owner.pid]).contains(&owner.pid)
        && owner.process_started_at.is_none_or(|expected| {
            crate::process_probe::process_start_time(owner.pid) == Some(expected)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha1::Digest as _;
    use std::io::Seek;

    fn identity(character: &str, port: u16) -> SessionLaunchIdentity {
        SessionLaunchIdentity {
            profile: character.to_string(),
            character: character.to_string(),
            connection: SessionConnectionIdentity::Lich {
                host: "loopback".to_string(),
                port,
            },
        }
    }

    fn direct_identity(game: &str) -> SessionLaunchIdentity {
        let mut profile = crate::config::profiles::LauncherProfile::new_direct();
        profile.account = "Account".to_string();
        profile.character = "Aster".to_string();
        profile.game = game.to_string();
        SessionLaunchIdentity::from_profile("Aster", &profile)
    }

    fn entry(pid: u32) -> SessionEntry {
        SessionEntry {
            character: "Aster".to_string(),
            port: 8040,
            control_host: None,
            pid,
            started_at: "2026-09-05T00:00:00Z".to_string(),
            instance_id: Some(format!("instance-{pid}")),
            process_started_at: None,
            launch: Some(identity("Aster", 8000)),
            data_root: None,
            lifecycle: SessionLifecycleState::Connected,
        }
    }

    #[test]
    fn saved_profile_data_dirs_are_included_in_legacy_registry_roots() {
        let root = tempfile::tempdir().unwrap();
        let launcher_root = root.path().join("launcher-data");
        let profile_root = root.path().join("profile-data");
        fs::create_dir_all(&launcher_root).unwrap();

        let mut profile = crate::config::profiles::LauncherProfile::new_direct();
        profile.name = "Aster".to_string();
        profile.data_dir = Some(profile_root.to_string_lossy().into_owned());
        let store = crate::config::profiles::LauncherStore {
            profiles: vec![profile],
            ..Default::default()
        };
        store.save_to(&launcher_root.join("launcher.toml")).unwrap();

        let roots =
            crate::config::profiles::known_data_roots_from(Some(&launcher_root), None, None)
                .into_iter()
                .map(|root| root.join("web-sessions"))
                .collect::<Vec<_>>();

        assert!(roots.contains(&launcher_root.join("web-sessions")));
        assert!(roots.contains(&profile_root.join("web-sessions")));
    }

    #[test]
    fn dead_registry_entries_are_removed_but_live_entries_survive() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path();
        fs::write(dir.join("10.json"), serde_json::to_vec(&entry(10)).unwrap()).unwrap();
        fs::write(dir.join("20.json"), serde_json::to_vec(&entry(20)).unwrap()).unwrap();

        let got = list_and_gc_in(dir, |_| [20].into_iter().collect());
        assert_eq!(got, vec![entry(20)]);
        assert!(!dir.join("10.json").exists());
        assert!(dir.join("20.json").exists());
    }

    #[test]
    fn current_and_live_legacy_registries_are_merged_for_upgrade() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("runtime/web-sessions");
        let legacy = root.path().join(".vellum-fe/web-sessions");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&legacy).unwrap();

        let mut current_entry = entry(20);
        current_entry.character = "Briar".to_string();
        let current_path = current.join(format!("20-{}.json", "a".repeat(40)));
        fs::write(&current_path, serde_json::to_vec(&current_entry).unwrap()).unwrap();
        let legacy_path = legacy.join("10.json");
        fs::write(
            &legacy_path,
            br#"{"character":"Aster","port":8040,"pid":10,"started_at":"then"}"#,
        )
        .unwrap();

        let got = list_and_gc_roots(Some(&current), std::slice::from_ref(&legacy), |pids| {
            assert_eq!(pids.len(), 2);
            [10, 20].into_iter().collect()
        });

        assert_eq!(
            got.iter()
                .map(|entry| entry.character.as_str())
                .collect::<Vec<_>>(),
            ["Aster", "Briar"]
        );
        assert!(current_path.exists());
        assert!(
            legacy_path.exists(),
            "a live old build must remain registered"
        );
        assert!(
            legacy.exists(),
            "a live legacy registry must not be removed"
        );
    }

    #[test]
    fn a_root_seen_as_current_and_legacy_is_scanned_once_with_legacy_safety() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("web-sessions");
        fs::create_dir_all(&shared).unwrap();
        let live_path = shared.join("10.json");
        fs::write(&live_path, serde_json::to_vec(&entry(10)).unwrap()).unwrap();

        let got = list_and_gc_roots_at(
            Some(&shared),
            std::slice::from_ref(&shared),
            unix_now(),
            |pids| {
                assert_eq!(pids, [10]);
                [10].into_iter().collect()
            },
        );
        assert_eq!(got, vec![entry(10)]);

        fs::write(&live_path, b"truncated").unwrap();
        let got = list_and_gc_roots_at(
            Some(&shared),
            std::slice::from_ref(&shared),
            unix_now(),
            |pids| {
                assert!(pids.is_empty());
                std::collections::HashSet::new()
            },
        );
        assert!(got.is_empty());
        assert!(
            live_path.exists(),
            "legacy safety must win when the current and legacy roots coincide"
        );
    }

    #[cfg(unix)]
    #[test]
    fn aliased_registry_roots_are_scanned_once() {
        let root = tempfile::tempdir().unwrap();
        let actual = root.path().join("actual");
        let alias = root.path().join("alias");
        fs::create_dir_all(&actual).unwrap();
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        fs::write(
            actual.join("10.json"),
            serde_json::to_vec(&entry(10)).unwrap(),
        )
        .unwrap();

        let got = list_and_gc_roots_at(
            Some(&actual),
            std::slice::from_ref(&alias),
            unix_now(),
            |pids| {
                assert_eq!(pids, [10]);
                [10].into_iter().collect()
            },
        );

        assert_eq!(got, vec![entry(10)]);
    }

    #[test]
    fn aged_dead_legacy_entries_are_cleaned() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("runtime/web-sessions");
        let legacy = root.path().join(".vellum-fe/web-sessions");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&legacy).unwrap();
        let legacy_path = legacy.join("10.json");
        fs::write(
            &legacy_path,
            br#"{"character":"Aster","port":8040,"pid":10,"started_at":"then"}"#,
        )
        .unwrap();

        let got = list_and_gc_roots_at(
            Some(&current),
            std::slice::from_ref(&legacy),
            u64::MAX,
            |_| std::collections::HashSet::new(),
        );

        assert!(got.is_empty());
        assert!(
            current.exists(),
            "the current registry root is not migration debris"
        );
        assert!(!legacy_path.exists());
    }

    #[test]
    fn fresh_dead_legacy_entry_is_retained_during_old_writer_grace() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("runtime/web-sessions");
        let legacy = root.path().join(".vellum-fe/web-sessions");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&legacy).unwrap();
        let legacy_path = legacy.join("10.json");
        fs::write(&legacy_path, serde_json::to_vec(&entry(10)).unwrap()).unwrap();

        let got = list_and_gc_roots_at(
            Some(&current),
            std::slice::from_ref(&legacy),
            unix_now(),
            |_| std::collections::HashSet::new(),
        );

        assert!(got.is_empty());
        assert!(legacy_path.exists());
    }

    #[test]
    fn fresh_malformed_legacy_entry_is_retained_during_old_writer_grace() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("runtime/web-sessions");
        let legacy = root.path().join(".vellum-fe/web-sessions");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&legacy).unwrap();
        let malformed = legacy.join("10.json");
        fs::write(&malformed, b"truncated").unwrap();

        let got = list_and_gc_roots_at(
            Some(&current),
            std::slice::from_ref(&legacy),
            unix_now(),
            |pids| {
                assert!(pids.is_empty());
                std::collections::HashSet::new()
            },
        );

        assert!(got.is_empty());
        assert!(malformed.exists());
        assert!(legacy.exists());
    }

    #[test]
    fn aged_malformed_legacy_entry_is_cleaned() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("runtime/web-sessions");
        let legacy = root.path().join(".vellum-fe/web-sessions");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&legacy).unwrap();
        let malformed = legacy.join("10.json");
        fs::write(&malformed, b"truncated").unwrap();

        let got = list_and_gc_roots_at(
            Some(&current),
            std::slice::from_ref(&legacy),
            u64::MAX,
            |_| std::collections::HashSet::new(),
        );

        assert!(got.is_empty());
        assert!(!malformed.exists());
    }

    #[test]
    fn malformed_current_entry_is_removed_eagerly() {
        let root = tempfile::tempdir().unwrap();
        let malformed = root.path().join("10.json");
        fs::write(&malformed, b"truncated").unwrap();

        let got = list_and_gc_in(root.path(), |_| std::collections::HashSet::new());

        assert!(got.is_empty());
        assert!(!malformed.exists());
    }

    #[test]
    fn legacy_directory_is_never_removed_during_compatibility_release() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("runtime/web-sessions");
        let legacy = root.path().join(".vellum-fe/web-sessions");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&legacy).unwrap();

        let got = list_and_gc_roots_at(
            Some(&current),
            std::slice::from_ref(&legacy),
            u64::MAX,
            |_| std::collections::HashSet::new(),
        );

        assert!(got.is_empty());
        assert!(legacy.exists());
    }

    #[test]
    fn reused_pid_does_not_resurrect_an_old_process_instance() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path();
        let mut old = entry(20);
        old.process_started_at = Some(u64::MAX);
        let path = dir.join("20.json");
        fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();

        let got = list_and_gc_in(dir, |_| [20].into_iter().collect());
        assert!(got.is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn old_registry_json_remains_readable() {
        let old = r#"{"character":"Aster","port":8040,"pid":42,"started_at":"then"}"#;
        let parsed: SessionEntry = serde_json::from_str(old).unwrap();
        assert_eq!(parsed.launch, None);
        assert_eq!(parsed.data_root, None);
        assert_eq!(parsed.instance_id, None);
        assert_eq!(parsed.process_started_at, None);
        assert_eq!(parsed.control_host, None);
        assert_eq!(
            parsed.control_url("/despana"),
            "http://127.0.0.1:8040/despana"
        );
        assert_eq!(parsed.lifecycle, SessionLifecycleState::Unknown);
    }

    #[test]
    fn control_urls_use_reachable_hosts_and_bracket_ipv6() {
        assert_eq!(control_host_for_bind("0.0.0.0"), "127.0.0.1");
        assert_eq!(control_host_for_bind("::"), "::1");
        assert_eq!(control_host_for_bind("[::1]"), "::1");
        assert_eq!(control_host_for_bind("192.168.1.25"), "192.168.1.25");
        assert_eq!(
            control_url("::1", 8040, "/despana"),
            "http://[::1]:8040/despana"
        );
        assert_eq!(
            control_url("192.168.1.25", 8040, "/api/v1/session/stop"),
            "http://192.168.1.25:8040/api/v1/session/stop"
        );
    }

    #[test]
    fn old_direct_identity_without_account_remains_readable() {
        let old = r#"{"profile":"Aster","character":"Aster","connection":{"kind":"direct","game":"gs4"}}"#;
        let parsed: SessionLaunchIdentity = serde_json::from_str(old).unwrap();
        assert_eq!(
            parsed.connection,
            SessionConnectionIdentity::Direct {
                game: "gs4".to_string(),
                account: String::new(),
            }
        );
    }

    #[test]
    fn direct_endpoint_claims_are_scoped_by_account() {
        let direct = |account: &str| SessionLaunchIdentity {
            profile: account.to_string(),
            character: "Aster".to_string(),
            connection: SessionConnectionIdentity::Direct {
                game: "gs4".to_string(),
                account: account.to_string(),
            },
        };
        assert_ne!(
            claim_resource(&direct("one")),
            claim_resource(&direct("two"))
        );
    }

    #[test]
    fn direct_identity_canonicalizes_all_profile_and_wire_world_names() {
        for (profile_name, wire_code) in [
            ("prime", "GS3"),
            ("platinum", "GSX"),
            (" platinum ", "GSX"),
            ("shattered", "GSF"),
            ("test", "GST"),
            ("dr", "DR"),
            ("drprime", "DR"),
            ("drplatinum", "DRX"),
            ("drfallen", "DRF"),
            ("drtest", "DRT"),
        ] {
            let named_world = direct_identity(profile_name);
            let coded_world = direct_identity(wire_code);

            assert_eq!(named_world, coded_world, "{profile_name} / {wire_code}");
            assert!(matches!(
                named_world.connection,
                SessionConnectionIdentity::Direct { ref game, .. }
                    if game == &wire_code.to_ascii_lowercase()
            ));
        }
    }

    #[test]
    fn old_direct_game_alias_cannot_bypass_launch_claim() {
        let old_named_world: SessionLaunchIdentity = serde_json::from_str(
            r#"{"profile":"Aster","character":"Aster","connection":{"kind":"direct","game":"prime","account":" ACCOUNT "}}"#,
        )
        .unwrap();
        let coded_world = direct_identity("GS3");
        assert_eq!(old_named_world, coded_world);

        let root = tempfile::tempdir().unwrap();
        let first = acquire_launch_claim_in(root.path(), &old_named_world, 100).unwrap();
        let LaunchClaimResult::Acquired(first) = first else {
            panic!("first claim must acquire")
        };
        first.persist_until_registration();

        assert!(matches!(
            acquire_launch_claim_in(root.path(), &coded_world, 101).unwrap(),
            LaunchClaimResult::Existing { character, .. } if character == "Aster"
        ));
    }

    #[test]
    fn endpoint_claim_is_atomic_and_recovers_after_ttl() {
        let root = tempfile::tempdir().unwrap();
        let first = acquire_launch_claim_in(root.path(), &identity("Aster", 8000), 100).unwrap();
        let LaunchClaimResult::Acquired(first) = first else {
            panic!("first claim must acquire")
        };
        first.persist_until_registration();

        assert!(matches!(
            acquire_launch_claim_in(root.path(), &identity("Briar", 8000), 101).unwrap(),
            LaunchClaimResult::Existing { character, .. } if character == "Aster"
        ));
        assert!(matches!(
            acquire_launch_claim_in(root.path(), &identity("Briar", 8000), 220).unwrap(),
            LaunchClaimResult::Acquired(_)
        ));
    }

    #[test]
    fn fresh_partial_claim_is_not_stolen_by_a_contender() {
        let root = tempfile::tempdir().unwrap();
        let current = identity("Aster", 8000);
        let contender = identity("Briar", 8000);
        let path = claim_path_in(root.path(), &current);
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        // Model the interval after create_new succeeds but before the first
        // launcher has finished writing its record.
        let mut creator = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        creator.write_all(b"{").unwrap();
        creator.sync_all().unwrap();

        let error = acquire_launch_claim_in(root.path(), &contender, unix_now())
            .err()
            .expect("a fresh partial claim must remain owned");
        assert!(error.to_string().contains("being created"));
        assert_eq!(fs::read(&path).unwrap(), b"{");

        let record = ClaimRecord {
            profile: current.profile.clone(),
            character: current.character.clone(),
            created_at: unix_now(),
        };
        creator.set_len(0).unwrap();
        creator.rewind().unwrap();
        creator
            .write_all(&serde_json::to_vec(&record).unwrap())
            .unwrap();
        creator.sync_all().unwrap();
        drop(creator);

        assert!(matches!(
            acquire_launch_claim_in(root.path(), &contender, unix_now()).unwrap(),
            LaunchClaimResult::Existing { character, .. } if character == "Aster"
        ));
    }

    #[test]
    fn crashed_partial_claim_recovers_after_ttl() {
        let root = tempfile::tempdir().unwrap();
        let current = identity("Aster", 8000);
        let contender = identity("Briar", 8000);
        let path = claim_path_in(root.path(), &current);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{").unwrap();

        let now = 1_000;
        let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs(now - CLAIM_TTL_SECS);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();

        assert!(matches!(
            acquire_launch_claim_in(root.path(), &contender, now).unwrap(),
            LaunchClaimResult::Acquired(_)
        ));
    }

    #[test]
    fn stale_claim_replacement_has_exactly_one_winner() {
        let root = tempfile::tempdir().unwrap();
        let stale = acquire_launch_claim_in(root.path(), &identity("Aster", 8000), 100).unwrap();
        let LaunchClaimResult::Acquired(stale) = stale else {
            panic!("initial claim must acquire")
        };
        stale.persist_until_registration();

        let root = std::sync::Arc::new(root);
        let start = std::sync::Arc::new(std::sync::Barrier::new(3));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut workers = Vec::new();
        for character in ["Briar", "Cedar"] {
            let root = std::sync::Arc::clone(&root);
            let start = std::sync::Arc::clone(&start);
            let tx = tx.clone();
            workers.push(std::thread::spawn(move || {
                start.wait();
                let result =
                    acquire_launch_claim_in(root.path(), &identity(character, 8000), 220).unwrap();
                match result {
                    LaunchClaimResult::Acquired(claim) => {
                        tx.send(true).unwrap();
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        drop(claim);
                    }
                    LaunchClaimResult::Existing { .. } => tx.send(false).unwrap(),
                }
            }));
        }
        start.wait();
        drop(tx);

        let results: Vec<bool> = rx.iter().collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(results.iter().filter(|won| **won).count(), 1, "{results:?}");
    }

    #[test]
    fn entry_rewrites_do_not_leave_config_backups() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("99-{}.json", "a".repeat(40)));
        let backup = sidecar_path(&path, ".bak");
        let legacy_temp = sidecar_path(&path, ".tmp");

        assert!(write_entry_path(&path, &entry(99)));
        fs::write(&backup, b"private old metadata").unwrap();
        fs::write(&legacy_temp, b"partial old metadata").unwrap();
        let mut updated = entry(99);
        updated.lifecycle = SessionLifecycleState::Idle;
        assert!(write_entry_path(&path, &updated));

        assert_eq!(
            serde_json::from_slice::<SessionEntry>(&fs::read(&path).unwrap())
                .unwrap()
                .lifecycle,
            SessionLifecycleState::Idle
        );
        assert!(!backup.exists());
        assert!(!legacy_temp.exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn registry_gc_cleans_only_recognized_safe_sidecars() {
        let root = tempfile::tempdir().unwrap();
        let main = root.path().join(format!("20-{}.json", "b".repeat(40)));
        fs::write(&main, serde_json::to_vec(&entry(20)).unwrap()).unwrap();
        let backup = sidecar_path(&main, ".bak");
        let stale_legacy_temp = sidecar_path(&main, ".tmp");
        let stale_unique_temp = sidecar_path(&main, ".tmp-7-1");
        let fresh_unique_temp = sidecar_path(&main, ".tmp-7-2");
        let unrelated = root.path().join("notes.json.bak");
        for path in [
            &backup,
            &stale_legacy_temp,
            &stale_unique_temp,
            &fresh_unique_temp,
            &unrelated,
        ] {
            fs::write(path, b"metadata").unwrap();
        }
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1);
        for path in [&stale_legacy_temp, &stale_unique_temp] {
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
        }

        assert_eq!(
            list_and_gc_in(root.path(), |_| [20].into_iter().collect()).len(),
            1
        );
        assert!(!backup.exists());
        assert!(!stale_legacy_temp.exists());
        assert!(!stale_unique_temp.exists());
        assert!(fresh_unique_temp.exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn registration_guard_removes_its_exact_entry_on_drop() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("99.json");
        write_entry_path(&path, &entry(99));
        let guard = SessionRegistration { path: path.clone() };
        assert!(path.exists());
        drop(guard);
        assert!(!path.exists());
    }

    #[test]
    fn child_endpoint_lease_allows_only_one_live_owner() {
        let root = tempfile::tempdir().unwrap();
        let lock_path = root.path().join("endpoint-leases").join(format!(
            "{:x}.lock",
            sha1::Sha1::digest(claim_resource(&identity("Aster", 8000)).as_bytes())
        ));
        let lease =
            acquire_endpoint_lease_in(root.path(), &identity("Aster", 8000), std::process::id())
                .unwrap();
        let error =
            acquire_endpoint_lease_in(root.path(), &identity("Briar", 8000), std::process::id())
                .err()
                .expect("second owner must be rejected");
        assert!(error.to_string().contains("Aster"));
        drop(lease);
        assert!(
            lock_path.exists(),
            "lease lock inode must never be unlinked"
        );
        assert!(acquire_endpoint_lease_in(
            root.path(),
            &identity("Briar", 8000),
            std::process::id(),
        )
        .is_ok());
    }

    #[test]
    fn distinct_connection_endpoints_can_hold_leases_simultaneously() {
        let root = tempfile::tempdir().unwrap();
        let first =
            acquire_endpoint_lease_in(root.path(), &identity("Aster", 8000), std::process::id())
                .unwrap();
        let second =
            acquire_endpoint_lease_in(root.path(), &identity("Briar", 8001), std::process::id())
                .unwrap();

        drop((first, second));
    }

    #[test]
    fn concurrent_endpoint_lease_race_has_exactly_one_owner() {
        let root = tempfile::tempdir().unwrap();
        let root = std::sync::Arc::new(root);
        let start = std::sync::Arc::new(std::sync::Barrier::new(3));
        let (tx, rx) = std::sync::mpsc::channel();

        let mut workers = Vec::new();
        for character in ["Aster", "Briar"] {
            let root = std::sync::Arc::clone(&root);
            let start = std::sync::Arc::clone(&start);
            let tx = tx.clone();
            workers.push(std::thread::spawn(move || {
                start.wait();
                let result = acquire_endpoint_lease_in(
                    root.path(),
                    &identity(character, 8000),
                    std::process::id(),
                );
                tx.send(result.is_ok()).unwrap();
                if let Ok(lease) = result {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    drop(lease);
                }
            }));
        }
        start.wait();
        drop(tx);

        let results: Vec<bool> = rx.iter().collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(results.iter().filter(|won| **won).count(), 1, "{results:?}");
    }
}
