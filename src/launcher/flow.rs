//! Launch flow: the state machine that turns "launch character X" into a live
//! `host:port` to attach to.
//!
//! Steps, in order:
//!
//! 1. **Resolve** the character against [`LauncherConfig`] → program, args,
//!    attach host + port.
//! 2. **Probe** the attach port. If a Lich is already listening there, skip the
//!    spawn entirely and go straight to attach — attaching to a running Lich is
//!    exactly what detachable-client mode is for, and spawning a second one on
//!    the same port would fail.
//! 3. **SSH + spawn** the headless Lich, *detached* so it outlives the SSH
//!    channel (and VellumFE itself).
//! 4. **Poll** the attach port until Lich comes up (bounded) — an open port,
//!    not the spawn exit code, is the authoritative success signal.
//! 5. **Emit** the resulting [`LaunchTarget`]; the caller feeds it into the
//!    unchanged Lich attach path.
//!
//! Every transition calls the progress callback so a frontend can surface it
//! (e.g. over `RemoteDelta` to the phone). The flow never touches the network
//! attach itself — it stays transport-agnostic and returns the target.

use std::time::Duration;

use anyhow::{Context, Result};

use super::config::{LauncherConfig, DEFAULT_KEY_ID};
use super::ssh::{
    detach_wrap, probe_port, wait_for_port, HostKeyDecision, HostKeyStatus, SshLauncher, SshTarget,
};

/// How long we wait for Lich to open its detachable-client port after spawn.
/// Lich's boot (Ruby VM + login + game connect) can take a while on a cold
/// start; be generous but bounded.
const PORT_WAIT: Duration = Duration::from_secs(45);

/// Quick "is it already up?" probe timeout before we bother SSHing.
const PREFLIGHT_PROBE: Duration = Duration::from_secs(2);

/// Progress the flow reports as it runs. Frontends map these to user-facing
/// status (a phone toast, a TUI line, a GUI spinner label).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchProgress {
    /// Resolved the character; about to check whether Lich is already up.
    Resolving { character: String },
    /// A Lich is already listening — skipping spawn, attaching directly.
    AlreadyRunning { host: String, port: u16 },
    /// Opening the SSH connection to the home PC.
    Connecting { host: String, port: u16 },
    /// First-use host key: caller must decide whether to trust it. This is
    /// surfaced (not auto-accepted) so the user sees the fingerprint.
    HostKeyPrompt { fingerprint: String },
    /// A pinned host key changed — MITM tripwire. The launch is aborted.
    HostKeyChanged,
    /// Running the detached launch command over SSH.
    Spawning { character: String },
    /// Waiting for Lich to open its detachable-client port.
    WaitingForPort { host: String, port: u16 },
    /// Success: Lich is up and this is where to attach.
    Ready { host: String, port: u16 },
    /// Terminal failure with a user-facing reason.
    Failed { reason: String },
}

/// The attach target the flow produces. Feed into the Lich attach path
/// (`LichConnection::start(host, port, None, …)` or the supervisor's
/// `ResolvedConnect::Lich`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchTarget {
    pub host: String,
    pub port: u16,
    /// The character we launched (display / status label).
    pub character: String,
}

/// How the flow should resolve host-key trust. Frontends supply this: a GUI
/// can prompt interactively, a headless run can auto-pin on first use (still
/// safe over a private tunnel), and either can hard-refuse a changed key.
///
/// The closure receives the fingerprint on first use and returns whether to
/// pin it. It is NEVER called for an already-known key, and a changed key is
/// always rejected regardless of the closure.
pub enum HostKeyTrust {
    /// Pin any new host key automatically (first-use). Appropriate for a
    /// headless/remote launch over an already-private WireGuard/Tailscale
    /// tunnel where there's no human to eyeball a fingerprint.
    AutoPinFirstUse,
    /// Ask via the closure, passing the SHA256 fingerprint; true = pin.
    Ask(Box<dyn FnMut(&str) -> bool + Send>),
}

/// Run the full launch flow for one character.
///
/// `progress` is invoked on every transition (best-effort; a frontend that
/// doesn't care can pass a no-op). Returns the attach target on success.
pub async fn launch<P>(
    config: &LauncherConfig,
    character: &str,
    trust: HostKeyTrust,
    mut progress: P,
) -> Result<LaunchTarget>
where
    P: FnMut(LaunchProgress) + Send,
{
    let result = launch_inner(config, character, trust, &mut progress).await;
    if let Err(e) = &result {
        progress(LaunchProgress::Failed {
            reason: format!("{e:#}"),
        });
    }
    result
}

async fn launch_inner<P>(
    config: &LauncherConfig,
    character: &str,
    trust: HostKeyTrust,
    progress: &mut P,
) -> Result<LaunchTarget>
where
    P: FnMut(LaunchProgress) + Send,
{
    progress(LaunchProgress::Resolving {
        character: character.to_string(),
    });

    let resolved = config.resolve(character)?;
    let attach_host = resolved.attach_host.clone();
    let attach_port = resolved.attach_port;

    // Step 2: already up? Attach directly.
    if probe_port(&attach_host, attach_port, PREFLIGHT_PROBE).await {
        progress(LaunchProgress::AlreadyRunning {
            host: attach_host.clone(),
            port: attach_port,
        });
        progress(LaunchProgress::Ready {
            host: attach_host.clone(),
            port: attach_port,
        });
        return Ok(LaunchTarget {
            host: attach_host,
            port: attach_port,
            character: character.to_string(),
        });
    }

    // Step 3: SSH in and spawn. Fetch the private key from the secure store.
    let private_key = super::config::load_private_key(DEFAULT_KEY_ID).context(
        "No launcher SSH key found — generate one in the launcher editor and add its public \
         key to the home PC's authorized_keys",
    )?;
    let known_hosts = super::config::known_hosts_path()?;

    progress(LaunchProgress::Connecting {
        host: config.ssh.host.clone(),
        port: config.ssh.port,
    });

    // Adapt the frontend's trust policy into the ssh handler's decision fn,
    // surfacing the prompt/changed states through `progress`.
    let ssh_host = config.ssh.host.clone();
    let ssh_port = config.ssh.port;
    // We can't call the &mut progress from inside the 'static Send closure the
    // handler needs, so capture the decision result out-of-band: the closure
    // records what happened, and we report it after connect. Host-key states
    // are rare (first use / attack), so a post-hoc report is fine.
    let decision_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<HostKeyStatus>::new()));
    let decide = build_decider(trust, std::sync::Arc::clone(&decision_log));

    let connect = SshLauncher::connect(
        SshTarget {
            host: &ssh_host,
            port: ssh_port,
            user: &config.ssh.user,
            private_openssh: &private_key,
            known_hosts: &known_hosts,
        },
        decide,
    )
    .await;

    // Report any host-key events the decider logged, before surfacing errors.
    if let Ok(log) = decision_log.lock() {
        for status in log.iter() {
            match status {
                HostKeyStatus::Unknown { fingerprint } => {
                    progress(LaunchProgress::HostKeyPrompt {
                        fingerprint: fingerprint.clone(),
                    });
                }
                HostKeyStatus::Changed { .. } => {
                    progress(LaunchProgress::HostKeyChanged);
                }
                HostKeyStatus::Known => {}
            }
        }
    }

    let mut launcher = connect?;

    progress(LaunchProgress::Spawning {
        character: character.to_string(),
    });

    let command = detach_wrap(
        config.ssh.remote_os.into(),
        &resolved.program,
        &resolved.args,
    );
    let run = launcher.run_detached(&command).await?;
    // Best-effort close; the detached process is already reparented.
    let _ = launcher.close().await;

    if !run.spawn_ok() {
        // The spawner reported failure. Surface stderr if any — but the port
        // poll below is still the real arbiter (some detach paths exit nonzero
        // yet still launch), so we don't bail here on a nonzero code alone.
        tracing::warn!(
            "launch spawner exit {:?}; stderr: {}",
            run.exit_status,
            run.stderr.trim()
        );
    }

    // Step 4: poll until Lich opens the port.
    progress(LaunchProgress::WaitingForPort {
        host: attach_host.clone(),
        port: attach_port,
    });

    if !wait_for_port(&attach_host, attach_port, PORT_WAIT).await {
        anyhow::bail!(
            "Launched Lich but {}:{} never opened within {}s — check the launch command and \
             that the character/game are valid",
            attach_host,
            attach_port,
            PORT_WAIT.as_secs()
        );
    }

    progress(LaunchProgress::Ready {
        host: attach_host.clone(),
        port: attach_port,
    });

    Ok(LaunchTarget {
        host: attach_host,
        port: attach_port,
        character: character.to_string(),
    })
}

/// Build the ssh host-key decision closure from the frontend trust policy,
/// logging every non-trivial status so the flow can report it afterward.
fn build_decider(
    trust: HostKeyTrust,
    log: std::sync::Arc<std::sync::Mutex<Vec<HostKeyStatus>>>,
) -> Box<dyn FnMut(HostKeyStatus) -> HostKeyDecision + Send> {
    match trust {
        HostKeyTrust::AutoPinFirstUse => Box::new(move |status: HostKeyStatus| {
            if let Ok(mut l) = log.lock() {
                l.push(status.clone());
            }
            match status {
                HostKeyStatus::Unknown { .. } => HostKeyDecision::TrustAndPin,
                // Never auto-accept a changed key, even in auto-pin mode.
                HostKeyStatus::Changed { .. } => HostKeyDecision::Reject,
                HostKeyStatus::Known => HostKeyDecision::AcceptOnce,
            }
        }),
        HostKeyTrust::Ask(mut ask) => Box::new(move |status: HostKeyStatus| {
            if let Ok(mut l) = log.lock() {
                l.push(status.clone());
            }
            match &status {
                HostKeyStatus::Unknown { fingerprint } => {
                    if ask(fingerprint) {
                        HostKeyDecision::TrustAndPin
                    } else {
                        HostKeyDecision::Reject
                    }
                }
                HostKeyStatus::Changed { .. } => HostKeyDecision::Reject,
                HostKeyStatus::Known => HostKeyDecision::AcceptOnce,
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::config::{CharacterLaunch, RemoteOs, SshConfig};
    use std::collections::BTreeMap;

    fn cfg_with_port(port: u16) -> LauncherConfig {
        let mut characters = BTreeMap::new();
        characters.insert(
            "Nisugi".to_string(),
            CharacterLaunch {
                game: "gemstone".to_string(),
                port,
            },
        );
        LauncherConfig {
            ssh: SshConfig {
                host: "127.0.0.1".to_string(),
                user: "u".to_string(),
                port: 22,
                remote_os: RemoteOs::Windows,
                lich_command: "ruby lich.rb --login {character} --{game} --detachable-client={port}"
                    .to_string(),
                attach_host: "127.0.0.1".to_string(),
                key_saved: false,
            },
            characters,
        }
    }

    /// When something is already listening on the attach port, the flow must
    /// short-circuit to Ready without needing SSH or a key. We bind a throwaway
    /// listener to a real free port and point the config at it.
    #[tokio::test]
    async fn already_running_short_circuits() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let cfg = cfg_with_port(port);

        let mut events = Vec::new();
        let target = launch(&cfg, "Nisugi", HostKeyTrust::AutoPinFirstUse, |p| {
            events.push(p)
        })
        .await
        .expect("should short-circuit to Ready");

        assert_eq!(target.port, port);
        assert_eq!(target.host, "127.0.0.1");
        assert!(events
            .iter()
            .any(|e| matches!(e, LaunchProgress::AlreadyRunning { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, LaunchProgress::Ready { .. })));
        // Must NOT have tried to connect over SSH.
        assert!(!events
            .iter()
            .any(|e| matches!(e, LaunchProgress::Connecting { .. })));
    }

    /// Unknown character resolves to a Failed progress event and an Err.
    #[tokio::test]
    async fn unknown_character_fails() {
        let cfg = cfg_with_port(59999);
        let mut events = Vec::new();
        let res = launch(&cfg, "Nobody", HostKeyTrust::AutoPinFirstUse, |p| {
            events.push(p)
        })
        .await;
        assert!(res.is_err());
        assert!(events
            .iter()
            .any(|e| matches!(e, LaunchProgress::Failed { .. })));
    }

    /// With the attach port closed, the flow advances past resolve + preflight
    /// probe into the SSH step and fails there (no key in the store, or the
    /// SSH host unreachable). Either way it must surface an Err AND a Failed
    /// progress event — and crucially must NOT report Ready. We avoid asserting
    /// a specific error string so the test is robust whether or not a
    /// "default" launcher key happens to exist in the dev keyring.
    #[tokio::test]
    async fn closed_port_advances_to_ssh_and_fails() {
        // Point the SSH host at a closed loopback port so connect() fails fast
        // even if a "default" key exists. 127.0.0.1:2 is not listening.
        let mut cfg = cfg_with_port(2);
        cfg.ssh.host = "127.0.0.1".to_string();
        cfg.ssh.port = 2;

        let mut events = Vec::new();
        let res = launch(&cfg, "Nisugi", HostKeyTrust::AutoPinFirstUse, |p| {
            events.push(p)
        })
        .await;

        assert!(res.is_err(), "closed port + no reachable SSH must fail");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, LaunchProgress::Failed { .. })),
            "must emit a Failed progress event"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, LaunchProgress::Ready { .. })),
            "must not report Ready on failure"
        );
    }
}
