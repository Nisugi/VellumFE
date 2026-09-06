//! Launcher policy for reusing and reserving session runtimes.
//!
//! The live registry is authoritative.  Saved profiles describe only the
//! requested launch; they are never joined back onto a live entry because a
//! profile can be edited or renamed while its child keeps running.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use sha1::{Digest, Sha1};

use crate::config::profiles::LaunchWebClient;
use crate::core::session_registry::{
    LaunchClaim, LaunchClaimResult, SessionConnectionIdentity, SessionEntry, SessionLaunchIdentity,
    SessionLifecycleState,
};

const SWITCH_HANDOFF_TIMEOUT: Duration = Duration::from_secs(20);
const SWITCH_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CONTROL_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const ENDPOINT_PROBE_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchDisposition {
    Spawn,
    Resume(SessionEntry),
    /// A matching process still owns its web runtime, but the game session is
    /// over. Stop that exact dormant runtime before launching a replacement.
    Replace(SessionEntry),
    EndpointConflict(SwitchRequest),
}

/// A proposed handoff from the exact runtime currently owning a Lich endpoint
/// to the requested character.  Its internals are deliberately private: the
/// launcher may display the proposal or pass it back for execution, but cannot
/// retarget it after the user confirms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchRequest {
    current: SessionEntry,
    target: SessionLaunchIdentity,
}

impl SwitchRequest {
    pub fn current_character(&self) -> &str {
        self.current
            .launch
            .as_ref()
            .map_or(&self.current.character, |launch| &launch.character)
    }

    pub fn current_profile(&self) -> &str {
        self.current
            .launch
            .as_ref()
            .map_or("unknown profile", |launch| launch.profile.as_str())
    }

    pub fn target_character(&self) -> &str {
        &self.target.character
    }

    pub fn target_profile(&self) -> &str {
        &self.target.profile
    }

    pub fn host(&self) -> &str {
        match &self.target.connection {
            SessionConnectionIdentity::Lich { host, .. } => host,
            SessionConnectionIdentity::Direct { .. } => {
                unreachable!("switch requests are only created for Lich endpoints")
            }
        }
    }

    pub fn port(&self) -> u16 {
        match self.target.connection {
            SessionConnectionIdentity::Lich { port, .. } => port,
            SessionConnectionIdentity::Direct { .. } => {
                unreachable!("switch requests are only created for Lich endpoints")
            }
        }
    }

    /// Whether the discovered owner advertises the lifecycle state and
    /// immutable process metadata required by the authenticated handoff API.
    /// Native GUI/TUI sidecars never publish these headless lifecycle states,
    /// so the launcher must not offer a destructive switch for them.
    pub fn owner_supports_character_handoff(&self) -> bool {
        self.current.instance_id.is_some()
            && self.current.data_root.is_some()
            && matches!(
                self.current.lifecycle,
                SessionLifecycleState::Connected
                    | SessionLifecycleState::Idle
                    | SessionLifecycleState::Disconnected
            )
    }
}

/// Stable digest of the effective connection target handed to a launcher
/// child.
///
/// The launcher passes this non-secret fingerprint alongside
/// `--launch-profile`; after loading the named profile, the child refuses to
/// continue if its immutable launch identity changed. This closes the
/// confirmation-to-spawn gap without exposing account names or other target
/// contents in process lists.
pub fn launch_target_fingerprint(profile: &crate::config::profiles::LauncherProfile) -> String {
    let target = SessionLaunchIdentity::from_profile(&profile.name, profile);
    launch_identity_fingerprint(&target)
}

pub fn launch_identity_fingerprint(target: &SessionLaunchIdentity) -> String {
    let encoded = serde_json::to_vec(target)
        .expect("SessionLaunchIdentity contains only infallibly serializable fields");
    format!("{:x}", Sha1::digest(encoded))
}

/// Decide from immutable identities only.  An old registry entry without a
/// launch identity is deliberately not matched by character name alone.
pub fn decide_launch(
    requested: &SessionLaunchIdentity,
    live: &[SessionEntry],
) -> LaunchDisposition {
    // Prefer the requested session regardless of registry ordering.  A
    // leftover conflicting entry from an older buggy launcher must not hide
    // the exact live process the user is trying to reopen.
    let matching = || {
        live.iter().filter(|entry| {
            entry.launch.as_ref().is_some_and(|existing| {
                existing == requested
                    || (existing.connection == requested.connection
                        && existing
                            .character
                            .eq_ignore_ascii_case(&requested.character))
            })
        })
    };
    let oldest = |left: &&SessionEntry, right: &&SessionEntry| {
        (&left.started_at, left.pid, left.port).cmp(&(&right.started_at, right.pid, right.port))
    };

    // Prefer an active matching runtime over a dormant duplicate left by an
    // earlier buggy launcher. Unknown is kept resumable for older registries
    // that predate lifecycle publication.
    let reusable = matching()
        .filter(|entry| {
            matches!(
                entry.lifecycle,
                SessionLifecycleState::Unknown
                    | SessionLifecycleState::Connecting
                    | SessionLifecycleState::Connected
                    | SessionLifecycleState::Reconnecting
            )
        })
        .min_by(oldest);
    if let Some(entry) = reusable {
        return LaunchDisposition::Resume(entry.clone());
    }

    // A different active character on the requested Lich endpoint owns the
    // connection even when an older dormant matching runtime also exists.
    // Surface that handoff first; never stop the dormant process while the
    // active owner is still present.
    let conflicts = || {
        live.iter().filter_map(|entry| {
            let Some(existing) = entry.launch.as_ref() else {
                return None;
            };
            if let (
                SessionConnectionIdentity::Lich { host, port },
                SessionConnectionIdentity::Lich {
                    host: requested_host,
                    port: requested_port,
                },
            ) = (&existing.connection, &requested.connection)
            {
                let matches_request = existing == requested
                    || (existing.connection == requested.connection
                        && existing
                            .character
                            .eq_ignore_ascii_case(&requested.character));
                if host == requested_host && port == requested_port && !matches_request {
                    return Some((entry, existing, host, *port));
                }
            }
            None
        })
    };
    let oldest_conflict =
        |left: &(&SessionEntry, &SessionLaunchIdentity, &String, u16),
         right: &(&SessionEntry, &SessionLaunchIdentity, &String, u16)| {
            (&left.0.started_at, left.0.pid, left.0.port).cmp(&(
                &right.0.started_at,
                right.0.pid,
                right.0.port,
            ))
        };
    if let Some((entry, _existing, _host, _port)) = conflicts()
        .filter(|(entry, _, _, _)| {
            matches!(
                entry.lifecycle,
                SessionLifecycleState::Unknown
                    | SessionLifecycleState::Connecting
                    | SessionLifecycleState::Connected
                    | SessionLifecycleState::Reconnecting
            )
        })
        .min_by(oldest_conflict)
    {
        return LaunchDisposition::EndpointConflict(SwitchRequest {
            current: entry.clone(),
            target: requested.clone(),
        });
    }

    let dormant = matching()
        .filter(|entry| {
            matches!(
                entry.lifecycle,
                SessionLifecycleState::Idle | SessionLifecycleState::Disconnected
            )
        })
        .min_by(oldest);
    if let Some(entry) = dormant {
        return LaunchDisposition::Replace(entry.clone());
    }

    if let Some((entry, _existing, _host, _port)) = conflicts().min_by(oldest_conflict) {
        return LaunchDisposition::EndpointConflict(SwitchRequest {
            current: entry.clone(),
            target: requested.clone(),
        });
    }

    LaunchDisposition::Spawn
}

/// Perform a confirmed same-endpoint character handoff.
///
/// This is intentionally blocking and bounded; the native launcher must call
/// it from a worker thread.  Success means the old runtime has disappeared
/// from the authoritative registry, the Lich endpoint has stopped accepting
/// connections, and the returned launch claim is still held for the target.
pub fn handoff_character_switch(request: &SwitchRequest) -> Result<LaunchClaim> {
    let mut operations = SystemSwitchOperations::new()?;
    handoff_character_switch_with(request, &mut operations, SWITCH_HANDOFF_TIMEOUT)
}

/// Stop a matching idle/disconnected runtime and reserve its exact launch for
/// a replacement. Unlike a character handoff, the external connection
/// endpoint does not need to disappear: an attach-only Lich profile may be
/// deliberately restarting its frontend while Lich remains available.
pub fn replace_dormant_session(
    current: &SessionEntry,
    target: &SessionLaunchIdentity,
) -> Result<LaunchClaim> {
    let mut operations = SystemSwitchOperations::new()?;
    replace_dormant_session_with(current, target, &mut operations, SWITCH_HANDOFF_TIMEOUT)
}

trait SwitchOperations {
    type Claim;

    fn live_sessions(&mut self) -> Result<Vec<SessionEntry>>;
    fn acquire_claim(&mut self, target: &SessionLaunchIdentity) -> Result<Self::Claim>;
    fn request_shutdown(&mut self, current: &SessionEntry) -> Result<()>;
    fn endpoint_released(&mut self, connection: &SessionConnectionIdentity) -> Result<bool>;
    fn process_exited(&mut self, current: &SessionEntry) -> Result<bool>;
    fn elapsed(&self) -> Duration;
    fn wait(&mut self, duration: Duration);
}

fn handoff_character_switch_with<O: SwitchOperations>(
    request: &SwitchRequest,
    operations: &mut O,
    timeout: Duration,
) -> Result<O::Claim> {
    validate_current_owner(request, &operations.live_sessions()?)?;

    // Reserve the target before asking the current runtime to change state.
    // The child-held endpoint lease remains authoritative, while this claim
    // prevents a second cooperative launcher racing into the handoff gap.
    let claim = operations.acquire_claim(&request.target)?;
    let fresh_live = operations.live_sessions()?;
    let current = validate_current_owner(request, &fresh_live)?;

    operations.request_shutdown(current).with_context(|| {
        format!(
            "Could not gracefully hand off {}",
            request.current_character()
        )
    })?;

    loop {
        let live = operations.live_sessions()?;
        let matching_instance = live
            .iter()
            .find(|entry| same_process_instance(entry, &request.current));
        if let Some(entry) = matching_instance {
            anyhow::ensure!(
                same_immutable_runtime(entry, &request.current),
                "The current session changed identity during character handoff"
            );
        }

        let owners = endpoint_owners(&request.target.connection, &live);
        anyhow::ensure!(
            owners
                .iter()
                .all(|entry| same_process_instance(entry, &request.current)),
            "Another runtime took ownership of the Lich endpoint during character handoff"
        );

        if matching_instance.is_none()
            && operations.endpoint_released(&request.target.connection)?
        {
            return Ok(claim);
        }

        anyhow::ensure!(
            operations.elapsed() < timeout,
            "Timed out waiting for {} to release Lich endpoint {}:{}; the target was not launched",
            request.current_character(),
            request.host(),
            request.port()
        );
        operations.wait(SWITCH_POLL_INTERVAL);
    }
}

fn replace_dormant_session_with<O: SwitchOperations>(
    current: &SessionEntry,
    target: &SessionLaunchIdentity,
    operations: &mut O,
    timeout: Duration,
) -> Result<O::Claim> {
    validate_dormant_replacement(current, target, &operations.live_sessions()?)?;

    // Hold the target claim throughout teardown so another cooperative
    // launcher cannot replace the same session in parallel.
    let claim = operations.acquire_claim(target)?;
    let fresh_live = operations.live_sessions()?;
    let fresh_current = validate_dormant_replacement(current, target, &fresh_live)?;
    operations
        .request_shutdown(fresh_current)
        .context("Could not stop the dormant session")?;

    loop {
        let live = operations.live_sessions()?;
        let owners = endpoint_owners(&target.connection, &live);
        anyhow::ensure!(
            owners
                .iter()
                .all(|entry| same_process_instance(entry, current)),
            "Another runtime took ownership while the dormant session was stopping"
        );
        let matching_instance = live
            .iter()
            .find(|entry| same_process_instance(entry, current));
        if let Some(entry) = matching_instance {
            anyhow::ensure!(
                same_immutable_runtime(entry, current),
                "The dormant session changed identity while it was stopping"
            );
        }
        if matching_instance.is_none() && operations.process_exited(current)? {
            return Ok(claim);
        }

        anyhow::ensure!(
            operations.elapsed() < timeout,
            "Timed out waiting for the dormant {} session to stop; the replacement was not launched",
            current.character
        );
        operations.wait(SWITCH_POLL_INTERVAL);
    }
}

fn validate_dormant_replacement<'a>(
    current: &SessionEntry,
    target: &SessionLaunchIdentity,
    live: &'a [SessionEntry],
) -> Result<&'a SessionEntry> {
    anyhow::ensure!(
        matches!(
            current.lifecycle,
            SessionLifecycleState::Idle | SessionLifecycleState::Disconnected
        ),
        "The matching session is no longer dormant"
    );
    let existing = current
        .launch
        .as_ref()
        .context("The dormant session has no immutable launch identity")?;
    anyhow::ensure!(
        existing == target
            || (existing.connection == target.connection
                && existing.character.eq_ignore_ascii_case(&target.character)),
        "The dormant session no longer matches the requested launch"
    );
    anyhow::ensure!(
        current.instance_id.is_some(),
        "The dormant session predates safe lifecycle control; stop it normally and relaunch"
    );
    anyhow::ensure!(
        current.data_root.is_some(),
        "The dormant session has no authenticated lifecycle control"
    );

    let owners = endpoint_owners(&target.connection, live);
    anyhow::ensure!(
        owners.len() == 1 && same_process_instance(owners[0], current),
        "Dormant session ownership changed before replacement began"
    );
    let matching = owners[0];
    anyhow::ensure!(
        same_immutable_runtime(matching, current),
        "The dormant session changed identity before replacement began"
    );
    anyhow::ensure!(
        matches!(
            matching.lifecycle,
            SessionLifecycleState::Idle | SessionLifecycleState::Disconnected
        ),
        "The matching session is no longer dormant"
    );
    Ok(matching)
}

fn validate_current_owner<'a>(
    request: &SwitchRequest,
    live: &'a [SessionEntry],
) -> Result<&'a SessionEntry> {
    anyhow::ensure!(
        request.owner_supports_character_handoff(),
        "The current session does not support launcher lifecycle control; log out or close it normally"
    );
    anyhow::ensure!(
        request.current.instance_id.is_some(),
        "The current session predates safe character switching; stop it normally and relaunch"
    );
    anyhow::ensure!(
        request.current.launch.is_some(),
        "The current session has no immutable launch identity"
    );

    let owners = endpoint_owners(&request.target.connection, live);
    anyhow::ensure!(
        owners.len() == 1,
        "Expected exactly one owner of Lich endpoint {}:{}, found {}",
        request.host(),
        request.port(),
        owners.len()
    );
    anyhow::ensure!(
        same_process_instance(owners[0], &request.current)
            && same_immutable_runtime(owners[0], &request.current),
        "The Lich endpoint owner changed before character handoff began"
    );
    anyhow::ensure!(
        matches!(
            owners[0].lifecycle,
            SessionLifecycleState::Connected
                | SessionLifecycleState::Idle
                | SessionLifecycleState::Disconnected
        ),
        "The current session is not in a stable state for character switching"
    );
    Ok(owners[0])
}

fn endpoint_owners<'a>(
    connection: &SessionConnectionIdentity,
    live: &'a [SessionEntry],
) -> Vec<&'a SessionEntry> {
    live.iter()
        .filter(|entry| {
            entry
                .launch
                .as_ref()
                .is_some_and(|launch| &launch.connection == connection)
        })
        .collect()
}

fn same_process_instance(left: &SessionEntry, right: &SessionEntry) -> bool {
    left.pid == right.pid
        && left.instance_id.is_some()
        && left.instance_id == right.instance_id
        && left.process_started_at == right.process_started_at
}

fn same_immutable_runtime(left: &SessionEntry, right: &SessionEntry) -> bool {
    same_process_instance(left, right)
        && left.character == right.character
        && left.port == right.port
        && left.started_at == right.started_at
        && left.launch == right.launch
        && left.data_root == right.data_root
}

struct SystemSwitchOperations {
    started: Instant,
    runtime: tokio::runtime::Runtime,
}

impl SystemSwitchOperations {
    fn new() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Could not start the endpoint-release probe runtime")?;
        Ok(Self {
            started: Instant::now(),
            runtime,
        })
    }

    fn probe_endpoint(&mut self, host: &str, port: u16) -> Result<bool> {
        if host == "loopback" {
            let ipv4 = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            let ipv6 = std::net::SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port));
            return Ok(self.probe_address(ipv4)? && self.probe_address(ipv6)?);
        }
        if host == "0.0.0.0" {
            return self.probe_address(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
        }

        let result = self.runtime.block_on(tokio::time::timeout(
            ENDPOINT_PROBE_TIMEOUT,
            tokio::net::TcpStream::connect((host, port)),
        ));
        endpoint_probe_result(result, host, port)
    }

    fn probe_address(&mut self, address: std::net::SocketAddr) -> Result<bool> {
        let result = self.runtime.block_on(tokio::time::timeout(
            ENDPOINT_PROBE_TIMEOUT,
            tokio::net::TcpStream::connect(address),
        ));
        endpoint_probe_result(result, &address.ip().to_string(), address.port())
    }
}

fn endpoint_probe_result(
    result: std::result::Result<
        std::io::Result<tokio::net::TcpStream>,
        tokio::time::error::Elapsed,
    >,
    host: &str,
    port: u16,
) -> Result<bool> {
    match result {
        Ok(Ok(_stream)) => Ok(false),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => Ok(true),
        // A host without this address family (e.g. Linux with ipv6.disable=1)
        // cannot be holding the port: treat "family unavailable" as released
        // rather than failing the whole handoff.
        Ok(Err(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::AddrNotAvailable
                    | std::io::ErrorKind::NetworkUnreachable
                    | std::io::ErrorKind::Unsupported
            ) =>
        {
            Ok(true)
        }
        Ok(Err(error)) => Err(error).with_context(|| {
            format!("Could not confirm whether Lich endpoint {host}:{port} was released")
        }),
        Err(_) => anyhow::bail!(
            "Timed out probing Lich endpoint {host}:{port}; release could not be confirmed"
        ),
    }
}

impl SwitchOperations for SystemSwitchOperations {
    type Claim = LaunchClaim;

    fn live_sessions(&mut self) -> Result<Vec<SessionEntry>> {
        Ok(crate::core::session_registry::list_and_gc())
    }

    fn acquire_claim(&mut self, target: &SessionLaunchIdentity) -> Result<Self::Claim> {
        match crate::core::session_registry::acquire_launch_claim(target)? {
            LaunchClaimResult::Acquired(claim) => Ok(claim),
            LaunchClaimResult::Existing { profile, character } => {
                anyhow::bail!("A launch is already in progress for {character} ({profile})")
            }
        }
    }

    fn request_shutdown(&mut self, current: &SessionEntry) -> Result<()> {
        let token = pairing_token(current)?;
        let instance_id = current
            .instance_id
            .as_deref()
            .context("The current runtime has no process-instance identity")?;
        let path = shutdown_path(current.lifecycle)?;
        let url = current.control_url(path);
        ureq::post(&url)
            .set("Authorization", &format!("Bearer {token}"))
            .set("X-Vellum-Instance", instance_id)
            .timeout(CONTROL_REQUEST_TIMEOUT)
            .call()
            .context("The current runtime refused the handoff request")?;
        Ok(())
    }

    fn endpoint_released(&mut self, connection: &SessionConnectionIdentity) -> Result<bool> {
        match connection {
            SessionConnectionIdentity::Lich { host, port } => self.probe_endpoint(host, *port),
            SessionConnectionIdentity::Direct { .. } => {
                anyhow::bail!("Character switching is only supported for Lich endpoints")
            }
        }
    }

    fn process_exited(&mut self, current: &SessionEntry) -> Result<bool> {
        if !crate::process_probe::live_pids(&[current.pid]).contains(&current.pid) {
            return Ok(true);
        }
        Ok(matches!(
            (
                current.process_started_at,
                crate::process_probe::process_start_time(current.pid)
            ),
            (Some(expected), Some(actual)) if actual != expected
        ))
    }

    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn wait(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

fn shutdown_path(lifecycle: SessionLifecycleState) -> Result<&'static str> {
    match lifecycle {
        SessionLifecycleState::Connected => Ok("/api/v1/session/exit-logout"),
        SessionLifecycleState::Idle | SessionLifecycleState::Disconnected => {
            Ok("/api/v1/session/stop")
        }
        _ => anyhow::bail!("The current session is not in a stable state for character switching"),
    }
}

/// Pair a fresh browser origin with the matched runtime.  The registry stores
/// only the data-root locator; the secret remains in that root's `web-token`.
pub fn resume_url(entry: &SessionEntry, client: LaunchWebClient) -> Result<String> {
    let token = pairing_token(entry)?;
    Ok(entry.control_url(&format!("/{}#token={token}", client.route())))
}

pub fn pairing_token(entry: &SessionEntry) -> Result<String> {
    let root = entry
        .data_root
        .as_ref()
        .context("The live session predates resumable pairing; exit it normally and relaunch")?;
    let token = std::fs::read_to_string(root.join("web-token"))
        .context("Could not read the live session's pairing token")?;
    let token = token.trim();
    anyhow::ensure!(
        !token.is_empty(),
        "The live session's pairing token is empty"
    );
    Ok(token.to_string())
}

/// Ask a non-connected runtime to terminate itself. This is HTTP-authenticated
/// and state-checked by the owning process; callers never signal a registry
/// PID. Verified connected sessions must use Exit & Log Out instead.
pub fn stop_inactive_runtime(entry: &SessionEntry) -> Result<()> {
    let token = pairing_token(entry)?;
    let instance_id = entry
        .instance_id
        .as_deref()
        .context("The idle runtime has no process-instance identity")?;
    let url = entry.control_url("/api/v1/session/stop");
    ureq::post(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .set("X-Vellum-Instance", instance_id)
        .timeout(std::time::Duration::from_secs(2))
        .call()
        .context("The runtime refused to stop")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::core::session_registry::SessionLifecycleState;

    #[test]
    fn ipv6_unavailable_error_kinds_fail_the_probe_instead_of_reporting_released() {
        // On a host with the IPv6 stack absent (e.g. Linux booted with
        // ipv6.disable=1), connect(::1) returns EADDRNOTAVAIL or
        // ENETUNREACH, never ECONNREFUSED. probe_endpoint("loopback")
        // requires BOTH family probes to report Ok(true), so these kinds
        // must not hard-error or handoff can never complete on such hosts.
        for kind in [
            std::io::ErrorKind::AddrNotAvailable,
            std::io::ErrorKind::NetworkUnreachable,
            std::io::ErrorKind::Unsupported,
        ] {
            let result: std::result::Result<
                std::io::Result<tokio::net::TcpStream>,
                tokio::time::error::Elapsed,
            > = Ok(Err(std::io::Error::from(kind)));
            let outcome = endpoint_probe_result(result, "::1", 8000);
            assert!(
                matches!(outcome, Ok(true)),
                "probe of ::1 with {kind:?} should report released, got {outcome:?}"
            );
        }
    }

    fn identity(profile: &str, character: &str, host: &str, port: u16) -> SessionLaunchIdentity {
        SessionLaunchIdentity {
            profile: profile.to_string(),
            character: character.to_string(),
            connection: SessionConnectionIdentity::Lich {
                host: host.to_string(),
                port,
            },
        }
    }

    fn entry(identity: SessionLaunchIdentity, port: u16) -> SessionEntry {
        let instance = format!("instance-{}-{port}", identity.character);
        SessionEntry {
            character: identity.character.clone(),
            port,
            control_host: None,
            pid: u32::from(port),
            started_at: "then".to_string(),
            instance_id: Some(instance),
            process_started_at: None,
            launch: Some(identity),
            data_root: Some(std::path::PathBuf::from("headless-data")),
            lifecycle: SessionLifecycleState::Connected,
        }
    }

    fn switch_request(current: SessionEntry, target: SessionLaunchIdentity) -> SwitchRequest {
        match decide_launch(&target, &[current]) {
            LaunchDisposition::EndpointConflict(request) => request,
            disposition => panic!("expected switch request, got {disposition:?}"),
        }
    }

    struct FakeSwitchOperations {
        snapshots: VecDeque<Vec<SessionEntry>>,
        last_snapshot: Vec<SessionEntry>,
        endpoint_results: VecDeque<std::result::Result<bool, String>>,
        process_exit_results: VecDeque<std::result::Result<bool, String>>,
        shutdown_error: Option<String>,
        elapsed: Duration,
        events: Vec<&'static str>,
        shutdown_port: Option<u16>,
        shutdown_path: Option<&'static str>,
    }

    impl FakeSwitchOperations {
        fn new(
            snapshots: Vec<Vec<SessionEntry>>,
            endpoint_results: Vec<std::result::Result<bool, String>>,
        ) -> Self {
            Self {
                snapshots: snapshots.into(),
                last_snapshot: Vec::new(),
                endpoint_results: endpoint_results.into(),
                process_exit_results: VecDeque::new(),
                shutdown_error: None,
                elapsed: Duration::ZERO,
                events: Vec::new(),
                shutdown_port: None,
                shutdown_path: None,
            }
        }
    }

    impl SwitchOperations for FakeSwitchOperations {
        type Claim = &'static str;

        fn live_sessions(&mut self) -> Result<Vec<SessionEntry>> {
            self.events.push("registry");
            if let Some(snapshot) = self.snapshots.pop_front() {
                self.last_snapshot = snapshot;
            }
            Ok(self.last_snapshot.clone())
        }

        fn acquire_claim(&mut self, _target: &SessionLaunchIdentity) -> Result<Self::Claim> {
            self.events.push("claim");
            Ok("held claim")
        }

        fn request_shutdown(&mut self, current: &SessionEntry) -> Result<()> {
            self.shutdown_path = Some(shutdown_path(current.lifecycle)?);
            self.shutdown_port = Some(current.port);
            self.events.push("shutdown");
            if let Some(error) = self.shutdown_error.take() {
                anyhow::bail!(error);
            }
            Ok(())
        }

        fn endpoint_released(&mut self, _connection: &SessionConnectionIdentity) -> Result<bool> {
            self.events.push("probe");
            match self.endpoint_results.pop_front().unwrap_or(Ok(false)) {
                Ok(released) => Ok(released),
                Err(error) => anyhow::bail!(error),
            }
        }

        fn process_exited(&mut self, _current: &SessionEntry) -> Result<bool> {
            self.events.push("process");
            match self.process_exit_results.pop_front().unwrap_or(Ok(true)) {
                Ok(exited) => Ok(exited),
                Err(error) => anyhow::bail!(error),
            }
        }

        fn elapsed(&self) -> Duration {
            self.elapsed
        }

        fn wait(&mut self, duration: Duration) {
            self.events.push("wait");
            self.elapsed += duration;
        }
    }

    #[test]
    fn same_identity_resumes_the_actual_walked_port() {
        let requested = identity("Aster", "Aster", "loopback", 8000);
        let live = entry(requested.clone(), 8043);
        assert_eq!(
            decide_launch(&requested, &[live.clone()]),
            LaunchDisposition::Resume(live)
        );
    }

    #[test]
    fn active_matching_sessions_resume_but_dormant_sessions_require_replacement() {
        let requested = identity("Aster", "Aster", "loopback", 8000);

        for lifecycle in [
            SessionLifecycleState::Unknown,
            SessionLifecycleState::Connecting,
            SessionLifecycleState::Connected,
            SessionLifecycleState::Reconnecting,
        ] {
            let mut live = entry(requested.clone(), 8043);
            live.lifecycle = lifecycle;
            assert!(matches!(
                decide_launch(&requested, &[live]),
                LaunchDisposition::Resume(_)
            ));
        }

        for lifecycle in [
            SessionLifecycleState::Idle,
            SessionLifecycleState::Disconnected,
        ] {
            let mut live = entry(requested.clone(), 8043);
            live.lifecycle = lifecycle;
            assert!(matches!(
                decide_launch(&requested, &[live]),
                LaunchDisposition::Replace(_)
            ));
        }
    }

    #[test]
    fn active_match_wins_over_an_older_dormant_duplicate() {
        let requested = identity("Aster", "Aster", "loopback", 8000);
        let mut dormant = entry(requested.clone(), 8040);
        dormant.lifecycle = SessionLifecycleState::Disconnected;
        let active = entry(requested.clone(), 8043);

        assert_eq!(
            decide_launch(&requested, &[dormant, active.clone()]),
            LaunchDisposition::Resume(active)
        );
    }

    #[test]
    fn active_endpoint_owner_wins_over_a_matching_dormant_session() {
        let requested = identity("Aster", "Aster", "loopback", 8000);
        let mut dormant = entry(requested.clone(), 8040);
        dormant.lifecycle = SessionLifecycleState::Disconnected;
        let active = entry(identity("Briar", "Briar", "loopback", 8000), 8043);

        for live in [
            vec![dormant.clone(), active.clone()],
            vec![active.clone(), dormant.clone()],
        ] {
            let LaunchDisposition::EndpointConflict(request) = decide_launch(&requested, &live)
            else {
                panic!("active endpoint owner must require a character handoff");
            };
            assert_eq!(request.current_character(), "Briar");
        }
    }

    #[test]
    fn dormant_replacement_stops_exact_instance_and_waits_for_registry_removal() {
        let target = identity("Aster", "Aster", "loopback", 8000);
        let mut current = entry(target.clone(), 8043);
        current.lifecycle = SessionLifecycleState::Disconnected;
        let captured = current.clone();
        let mut operations = FakeSwitchOperations::new(
            vec![
                vec![current.clone()],
                vec![current.clone()],
                vec![current],
                vec![],
            ],
            Vec::new(),
        );

        let claim = replace_dormant_session_with(
            &captured,
            &target,
            &mut operations,
            Duration::from_secs(1),
        )
        .unwrap();

        assert_eq!(claim, "held claim");
        assert_eq!(operations.shutdown_port, Some(8043));
        assert_eq!(operations.shutdown_path, Some("/api/v1/session/stop"));
        assert_eq!(
            operations.events,
            [
                "registry", "claim", "registry", "shutdown", "registry", "wait", "registry",
                "process"
            ]
        );
        assert!(!operations.events.contains(&"probe"));
    }

    #[test]
    fn dormant_replacement_waits_for_process_exit_after_registry_removal() {
        let target = identity("Aster", "Aster", "loopback", 8000);
        let mut current = entry(target.clone(), 8043);
        current.lifecycle = SessionLifecycleState::Disconnected;
        let mut operations = FakeSwitchOperations::new(
            vec![vec![current.clone()], vec![current.clone()], vec![], vec![]],
            Vec::new(),
        );
        operations.process_exit_results = [Ok(false), Ok(true)].into();

        replace_dormant_session_with(&current, &target, &mut operations, Duration::from_secs(1))
            .unwrap();

        assert_eq!(
            operations.events,
            [
                "registry", "claim", "registry", "shutdown", "registry", "process", "wait",
                "registry", "process"
            ]
        );
    }

    #[test]
    fn dormant_replacement_fails_closed_if_instance_changes() {
        let target = identity("Aster", "Aster", "loopback", 8000);
        let mut current = entry(target.clone(), 8043);
        current.lifecycle = SessionLifecycleState::Disconnected;
        let mut changed = current.clone();
        changed.instance_id = Some("replacement-instance".to_string());
        let mut operations =
            FakeSwitchOperations::new(vec![vec![current.clone()], vec![changed]], Vec::new());

        let error = replace_dormant_session_with(
            &current,
            &target,
            &mut operations,
            Duration::from_secs(1),
        )
        .unwrap_err();

        assert!(error.to_string().contains("ownership changed"));
        assert!(!operations.events.contains(&"shutdown"));
    }

    #[test]
    fn dormant_replacement_fails_if_another_runtime_appears_during_teardown() {
        let target = identity("Aster", "Aster", "loopback", 8000);
        let mut current = entry(target.clone(), 8043);
        current.lifecycle = SessionLifecycleState::Disconnected;
        let mut replacement = entry(target.clone(), 8044);
        replacement.instance_id = Some("replacement-instance".to_string());
        let mut operations = FakeSwitchOperations::new(
            vec![
                vec![current.clone()],
                vec![current.clone()],
                vec![current.clone()],
                vec![replacement],
            ],
            Vec::new(),
        );

        let error = replace_dormant_session_with(
            &current,
            &target,
            &mut operations,
            Duration::from_secs(1),
        )
        .unwrap_err();

        assert!(error.to_string().contains("Another runtime took ownership"));
    }

    #[test]
    fn dormant_replacement_never_launches_after_shutdown_failure_or_timeout() {
        let target = identity("Aster", "Aster", "loopback", 8000);
        let mut current = entry(target.clone(), 8043);
        current.lifecycle = SessionLifecycleState::Disconnected;

        let mut rejected = FakeSwitchOperations::new(
            vec![vec![current.clone()], vec![current.clone()]],
            Vec::new(),
        );
        rejected.shutdown_error = Some("HTTP rejected".to_string());
        let error =
            replace_dormant_session_with(&current, &target, &mut rejected, Duration::from_secs(1))
                .unwrap_err();
        assert!(format!("{error:#}").contains("HTTP rejected"));

        let mut timed_out = FakeSwitchOperations::new(
            vec![
                vec![current.clone()],
                vec![current.clone()],
                vec![current.clone()],
                vec![current],
            ],
            Vec::new(),
        );
        let captured = timed_out.snapshots[0][0].clone();
        let error =
            replace_dormant_session_with(&captured, &target, &mut timed_out, SWITCH_POLL_INTERVAL)
                .unwrap_err();
        assert!(error.to_string().contains("Timed out waiting"));
    }

    #[test]
    fn same_endpoint_and_character_resumes_after_profile_rename() {
        let requested = identity("Aster new", "ASTER", "loopback", 8000);
        let live = entry(identity("Aster old", "Aster", "loopback", 8000), 8041);
        assert!(matches!(
            decide_launch(&requested, &[live]),
            LaunchDisposition::Resume(_)
        ));
    }

    #[test]
    fn same_profile_and_character_does_not_resume_after_endpoint_edit() {
        let requested = identity("Aster", "ASTER", "loopback", 8001);
        let live = entry(identity("Aster", "Aster", "loopback", 8000), 8041);
        assert_eq!(decide_launch(&requested, &[live]), LaunchDisposition::Spawn);
    }

    #[test]
    fn different_character_on_owned_lich_endpoint_is_rejected() {
        let requested = identity("Aster", "Aster", "loopback", 8000);
        let live = entry(identity("Briar", "Briar", "loopback", 8000), 8040);
        let LaunchDisposition::EndpointConflict(request) = decide_launch(&requested, &[live])
        else {
            panic!("expected endpoint conflict")
        };
        assert_eq!(request.current_character(), "Briar");
        assert_eq!(request.current_profile(), "Briar");
        assert_eq!(request.target_character(), "Aster");
        assert_eq!(request.target_profile(), "Aster");
        assert_eq!(request.host(), "loopback");
        assert_eq!(request.port(), 8000);
    }

    #[test]
    fn only_headless_lifecycle_owners_are_switchable() {
        let target = identity("Aster", "Aster", "loopback", 8000);
        let mut native = entry(identity("Briar", "Briar", "loopback", 8000), 8040);
        native.lifecycle = SessionLifecycleState::Unknown;
        let native_request = switch_request(native, target.clone());
        assert!(!native_request.owner_supports_character_handoff());

        let headless_request = switch_request(
            {
                let mut owner = entry(identity("Briar", "Briar", "loopback", 8000), 8040);
                owner.data_root = Some(std::path::PathBuf::from("headless-data"));
                owner
            },
            target,
        );
        assert!(headless_request.owner_supports_character_handoff());
    }

    #[test]
    fn target_fingerprint_freezes_the_confirmed_launch_identity() {
        let mut confirmed = crate::config::profiles::LauncherProfile::new_direct();
        confirmed.name = "Aster".to_string();
        confirmed.account = "account".to_string();
        confirmed.character = "Aster".to_string();
        confirmed.mode = crate::config::profiles::LaunchMode::Lich;
        confirmed.host = "127.0.0.1".to_string();
        confirmed.port = 8000;
        confirmed.custom_launch = Some("start-aster".to_string());

        let frozen = launch_target_fingerprint(&confirmed);
        let mutations: [fn(&mut crate::config::profiles::LauncherProfile); 3] = [
            |profile: &mut crate::config::profiles::LauncherProfile| {
                profile.character = "Briar".to_string()
            },
            |profile: &mut crate::config::profiles::LauncherProfile| profile.port = 8001,
            |profile: &mut crate::config::profiles::LauncherProfile| {
                profile.host = "lich.example.test".to_string()
            },
        ];
        for mutate in mutations {
            let mut changed = confirmed.clone();
            mutate(&mut changed);
            assert_ne!(launch_target_fingerprint(&changed), frozen);
        }
    }

    #[test]
    fn reusable_session_wins_over_an_earlier_conflicting_legacy_duplicate() {
        let requested = identity("Rabki", "Rabki", "loopback", 8000);
        let conflict = entry(identity("Calvix", "Calvix", "loopback", 8000), 8040);
        let matching = entry(requested.clone(), 8043);
        assert_eq!(
            decide_launch(&requested, &[conflict, matching.clone()]),
            LaunchDisposition::Resume(matching)
        );
    }

    #[test]
    fn legacy_character_name_is_not_treated_as_live_identity() {
        let requested = identity("Aster", "Aster", "loopback", 8000);
        let mut legacy = entry(requested.clone(), 8040);
        legacy.launch = None;
        assert_eq!(
            decide_launch(&requested, &[legacy]),
            LaunchDisposition::Spawn
        );
    }

    #[test]
    fn resume_url_uses_walked_port_and_token_from_that_sessions_data_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("web-token"), "paired-secret\n").unwrap();
        let mut live = entry(identity("Aster", "Aster", "loopback", 8000), 8043);
        live.data_root = Some(root.path().to_path_buf());
        assert_eq!(
            resume_url(&live, LaunchWebClient::Despana).unwrap(),
            "http://127.0.0.1:8043/despana#token=paired-secret"
        );

        live.control_host = Some("::1".to_string());
        assert_eq!(
            resume_url(&live, LaunchWebClient::Despana).unwrap(),
            "http://[::1]:8043/despana#token=paired-secret"
        );
    }

    #[test]
    fn confirmed_handoff_claims_before_shutdown_and_waits_for_both_release_signals() {
        let current = entry(identity("Briar", "Briar", "loopback", 8000), 8043);
        let request = switch_request(
            current.clone(),
            identity("Aster", "Aster", "loopback", 8000),
        );
        let mut operations = FakeSwitchOperations::new(
            vec![
                vec![current.clone()],
                vec![current.clone()],
                vec![current],
                vec![],
            ],
            vec![Ok(true)],
        );

        let claim =
            handoff_character_switch_with(&request, &mut operations, Duration::from_secs(1))
                .unwrap();

        assert_eq!(claim, "held claim");
        assert_eq!(operations.shutdown_port, Some(8043));
        assert_eq!(
            operations.shutdown_path,
            Some("/api/v1/session/exit-logout")
        );
        assert_eq!(
            operations.events,
            [
                "registry", "claim", "registry", "shutdown", "registry", "wait", "registry",
                "probe"
            ]
        );
    }

    #[test]
    fn idle_handoff_uses_authenticated_stop_path() {
        let current = entry(identity("Briar", "Briar", "loopback", 8000), 8043);
        let request = switch_request(
            current.clone(),
            identity("Aster", "Aster", "loopback", 8000),
        );
        let mut fresh_idle = current.clone();
        fresh_idle.lifecycle = SessionLifecycleState::Idle;
        let mut operations = FakeSwitchOperations::new(
            vec![
                vec![current],
                vec![fresh_idle.clone()],
                vec![fresh_idle],
                vec![],
            ],
            vec![Ok(true)],
        );

        handoff_character_switch_with(&request, &mut operations, Duration::from_secs(1)).unwrap();

        assert_eq!(operations.shutdown_path, Some("/api/v1/session/stop"));
    }

    #[test]
    fn ambiguous_endpoint_ownership_fails_before_claim_or_shutdown() {
        let current = entry(identity("Briar", "Briar", "loopback", 8000), 8040);
        let other = entry(identity("Cedar", "Cedar", "loopback", 8000), 8041);
        let request = switch_request(
            current.clone(),
            identity("Aster", "Aster", "loopback", 8000),
        );
        let mut operations = FakeSwitchOperations::new(vec![vec![current, other]], Vec::new());

        let error =
            handoff_character_switch_with(&request, &mut operations, Duration::from_secs(1))
                .unwrap_err();

        assert!(error.to_string().contains("exactly one owner"));
        assert_eq!(operations.events, ["registry"]);
    }

    #[test]
    fn changed_process_instance_fails_before_claim_or_shutdown() {
        let current = entry(identity("Briar", "Briar", "loopback", 8000), 8040);
        let request = switch_request(
            current.clone(),
            identity("Aster", "Aster", "loopback", 8000),
        );
        let mut replacement = current;
        replacement.instance_id = Some("replacement".to_string());
        let mut operations = FakeSwitchOperations::new(vec![vec![replacement]], Vec::new());

        let error =
            handoff_character_switch_with(&request, &mut operations, Duration::from_secs(1))
                .unwrap_err();

        assert!(error.to_string().contains("owner changed"));
        assert_eq!(operations.events, ["registry"]);
    }

    #[test]
    fn handoff_timeout_never_reports_target_ready() {
        let current = entry(identity("Briar", "Briar", "loopback", 8000), 8040);
        let request = switch_request(
            current.clone(),
            identity("Aster", "Aster", "loopback", 8000),
        );
        let mut operations = FakeSwitchOperations::new(
            vec![vec![current.clone()], vec![current.clone()], vec![current]],
            Vec::new(),
        );

        let error = handoff_character_switch_with(&request, &mut operations, SWITCH_POLL_INTERVAL)
            .unwrap_err();

        assert!(error.to_string().contains("Timed out waiting"));
        assert!(operations.events.contains(&"shutdown"));
        assert!(!operations.events.contains(&"probe"));
    }

    #[test]
    fn uncertain_endpoint_probe_fails_closed() {
        let current = entry(identity("Briar", "Briar", "loopback", 8000), 8040);
        let request = switch_request(
            current.clone(),
            identity("Aster", "Aster", "loopback", 8000),
        );
        let mut operations = FakeSwitchOperations::new(
            vec![vec![current.clone()], vec![current], vec![]],
            vec![Err("endpoint uncertainty".to_string())],
        );

        let error =
            handoff_character_switch_with(&request, &mut operations, Duration::from_secs(1))
                .unwrap_err();

        assert_eq!(error.to_string(), "endpoint uncertainty");
        assert!(operations.events.contains(&"shutdown"));
        assert_eq!(operations.events.last(), Some(&"probe"));
    }
}
