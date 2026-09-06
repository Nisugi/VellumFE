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
//! 3. **Spawn** the headless Lich, *detached* so it outlives the launcher (and
//!    VellumFE itself). A loopback attach host spawns the process directly on
//!    this machine — SSHing into localhost would require running an SSH server
//!    just to start a process sitting right here; any other host goes over SSH.
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
const PORT_WAIT: Duration = Duration::from_secs(60);

/// Quick "is it already up?" probe timeout before we bother SSHing.
const PREFLIGHT_PROBE: Duration = Duration::from_secs(2);

/// Interval between attach-port polls after launching. Lich opens the port
/// ~3-5s after spawn (Ruby boot + eAccess login), and a bare TCP connect is
/// free even over a tunnel — a coarse interval just quantizes the user's
/// wait (at 5s, up to 4 extra seconds staring at "Waiting for Lich…").
const POLL_INTERVAL: Duration = Duration::from_secs(1);

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

/// A fully-specified launch, independent of where it came from (the desktop
/// config path or the mobile per-profile-command path both build one of these).
pub struct LaunchSpec {
    /// SSH target — the home PC.
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_user: String,
    /// Remote OS, for the detach wrapper (the ssh-module enum detach_wrap uses).
    pub remote_os: super::ssh::RemoteOs,
    /// Program + args to run, already split. Placeholders (if any) resolved.
    pub program: String,
    pub args: Vec<String>,
    /// Where to attach once Lich is up.
    pub attach_host: String,
    pub attach_port: u16,
    /// Label for progress/status.
    pub character: String,
    /// Run the command on THIS machine instead of over SSH. Set when the
    /// attach host is loopback: SSHing into localhost would demand an SSH
    /// server (and a key) just to start a process sitting right here, which
    /// is absurd for the common single-PC setup.
    pub local: bool,
}

/// Spawn the launch command on THIS machine, fully detached.
///
/// Detachment matters as much here as the `detach_wrap` does over SSH: Lich
/// must outlive both this launcher process and the session that attaches to
/// it. Stdio goes to null — the launcher may have freed its console, and
/// inheriting dead handles makes CreateProcess fail (rust-lang/rust#113277).
fn spawn_local(spec: &LaunchSpec) -> Result<()> {
    let mut cmd = std::process::Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS: no console, and critically not a child of this
        // console — a Lich started here must survive the launcher closing.
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let child = cmd
        .spawn()
        .with_context(|| format!("Could not run '{}'", spec.program))?;
    let pid = child.id();
    std::thread::Builder::new()
        .name(format!("lich-reaper-{pid}"))
        .spawn(move || {
            let mut child = child;
            if let Err(error) = child.wait() {
                tracing::warn!(pid, %error, "could not reap local Lich child");
            }
        })
        .context("Could not start local Lich child reaper")?;
    Ok(())
}

/// True when a host names this machine, so the launch can skip SSH entirely.
/// Deliberately conservative: only unambiguous loopback spellings count. A
/// LAN address that happens to be this box still takes the SSH path rather
/// than guessing wrong about someone's split-horizon setup.
pub fn is_local_host(host: &str) -> bool {
    let host = host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
        || host == "0.0.0.0"
}

impl LaunchSpec {
    /// Build a spec for the mobile path: a saved Lich profile's launch command
    /// plus its attach host/port, with SSH connection details from the global
    /// launcher SSH settings. The SSH host defaults to the attach host (Lich
    /// and SSHD are the same machine) unless the settings override it.
    pub fn from_command(
        command: &str,
        attach_host: &str,
        attach_port: u16,
        character: &str,
        ssh: &super::config::SshConfig,
    ) -> Self {
        let (program, args) = super::ssh::split_command(command);
        // SSH into the attach host by default; a non-empty settings host wins
        // (split-horizon setups). Port/user/remote_os come from settings.
        let ssh_host = if ssh.host.trim().is_empty() {
            attach_host.to_string()
        } else {
            ssh.host.clone()
        };
        // Loopback attach host = this machine: spawn directly, no SSH.
        //
        // The per-profile host wins over the GLOBAL ssh settings, which are
        // shared by every profile: configuring SSH once for a remote profile
        // must not silently route an unrelated 127.0.0.1 profile over the
        // network. A genuinely tunnelled setup (attach locally, Lich runs
        // elsewhere) sets its profile host to the tunnel endpoint rather
        // than bare loopback.
        let local = is_local_host(attach_host);
        LaunchSpec {
            ssh_host,
            ssh_port: ssh.port,
            ssh_user: ssh.user.clone(),
            remote_os: ssh.remote_os.into(),
            program,
            args,
            attach_host: attach_host.to_string(),
            attach_port,
            character: character.to_string(),
            local,
        }
    }
}

/// Run the full launch flow for one character resolved from a [`LauncherConfig`]
/// (the desktop / `.launch <character>` path).
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
    progress(LaunchProgress::Resolving {
        character: character.to_string(),
    });
    let spec = match config.resolve(character) {
        Ok(resolved) => LaunchSpec {
            ssh_host: config.ssh.host.clone(),
            ssh_port: config.ssh.port,
            ssh_user: config.ssh.user.clone(),
            remote_os: config.ssh.remote_os.into(),
            program: resolved.program,
            args: resolved.args,
            local: is_local_host(&resolved.attach_host),
            attach_host: resolved.attach_host,
            attach_port: resolved.attach_port,
            character: character.to_string(),
        },
        Err(e) => {
            progress(LaunchProgress::Failed {
                reason: format!("{e:#}"),
            });
            return Err(e);
        }
    };
    let result = run_spec(&spec, trust, &mut progress).await;
    if let Err(e) = &result {
        // Log every failure: a launch that dies between phases used to leave
        // nothing in the log at all, so "it just hangs" was undiagnosable.
        tracing::warn!(target: "launch_timing", error = %format!("{e:#}"), "launch failed");
        progress(LaunchProgress::Failed {
            reason: format!("{e:#}"),
        });
    }
    result
}

/// Run the full launch flow from a fully-built [`LaunchSpec`] (the mobile
/// per-profile launch-command path, where the command is complete and the SSH
/// user comes from global settings).
pub async fn launch_spec<P>(
    spec: &LaunchSpec,
    trust: HostKeyTrust,
    mut progress: P,
) -> Result<LaunchTarget>
where
    P: FnMut(LaunchProgress) + Send,
{
    let result = run_spec(spec, trust, &mut progress).await;
    if let Err(e) = &result {
        tracing::warn!(target: "launch_timing", error = %format!("{e:#}"), "launch failed");
        progress(LaunchProgress::Failed {
            reason: format!("{e:#}"),
        });
    }
    result
}

async fn run_spec<P>(
    spec: &LaunchSpec,
    trust: HostKeyTrust,
    progress: &mut P,
) -> Result<LaunchTarget>
where
    P: FnMut(LaunchProgress) + Send,
{
    let attach_host = spec.attach_host.clone();
    let attach_port = spec.attach_port;
    // Phase timings: a slow launch is almost always Lich's own boot, but
    // "almost always" is not a diagnosis. Log each phase so a user reporting
    // "it takes forever" produces evidence instead of a guess.
    let t0 = std::time::Instant::now();

    // Step 2: already up? Attach directly — the first-probe fast path.
    let probe_open = probe_port(&attach_host, attach_port, PREFLIGHT_PROBE).await;
    tracing::info!(
        target: "launch_timing",
        elapsed_ms = t0.elapsed().as_millis() as u64,
        open = probe_open,
        "preflight probe finished"
    );
    if probe_open {
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
            character: spec.character.clone(),
        });
    }

    // Step 3a: same machine — spawn the process directly. No SSH server, no
    // key, no known_hosts. The port wait below is still the authority on
    // whether it actually worked, exactly as on the remote path.
    if spec.local {
        progress(LaunchProgress::Spawning {
            character: spec.character.clone(),
        });
        spawn_local(spec).context("Failed to start Lich on this machine")?;
        let spawned_at = t0.elapsed();
        tracing::info!(
            target: "launch_timing",
            elapsed_ms = spawned_at.as_millis() as u64,
            program = %spec.program,
            "local spawn returned; waiting for port"
        );
        progress(LaunchProgress::WaitingForPort {
            host: attach_host.clone(),
            port: attach_port,
        });
        let opened = wait_for_port(&attach_host, attach_port, PORT_WAIT, POLL_INTERVAL).await;
        tracing::info!(
            target: "launch_timing",
            elapsed_ms = t0.elapsed().as_millis() as u64,
            boot_ms = (t0.elapsed() - spawned_at).as_millis() as u64,
            opened,
            "port wait finished (boot_ms = how long Lich itself took)"
        );
        if !opened {
            anyhow::bail!(
                "Started Lich but {attach_host}:{attach_port} never opened within {}s — \
                 check the launch command and Lich's own log",
                PORT_WAIT.as_secs()
            );
        }
        progress(LaunchProgress::Ready {
            host: attach_host.clone(),
            port: attach_port,
        });
        return Ok(LaunchTarget {
            host: attach_host,
            port: attach_port,
            character: spec.character.clone(),
        });
    }

    // Step 3: SSH in and spawn. Fetch the private key from the secure store.
    let private_key = super::config::load_private_key(DEFAULT_KEY_ID).context(
        "No launcher SSH key set up — generate one in launcher settings and add its public \
         key to the home PC's authorized_keys",
    )?;
    let known_hosts = super::config::known_hosts_path()?;

    progress(LaunchProgress::Connecting {
        host: spec.ssh_host.clone(),
        port: spec.ssh_port,
    });

    // Adapt the frontend's trust policy into the ssh handler's decision fn,
    // surfacing the prompt/changed states through `progress`.
    // We can't call the &mut progress from inside the 'static Send closure the
    // handler needs, so capture the decision result out-of-band: the closure
    // records what happened, and we report it after connect. Host-key states
    // are rare (first use / attack), so a post-hoc report is fine.
    let decision_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<HostKeyStatus>::new()));
    let decide = build_decider(trust, std::sync::Arc::clone(&decision_log));

    let connect = SshLauncher::connect(
        SshTarget {
            host: &spec.ssh_host,
            port: spec.ssh_port,
            user: &spec.ssh_user,
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
        character: spec.character.clone(),
    });

    let command = detach_wrap(spec.remote_os, &spec.program, &spec.args);
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

    // Step 4: poll every 5s until Lich opens the port.
    progress(LaunchProgress::WaitingForPort {
        host: attach_host.clone(),
        port: attach_port,
    });

    if !wait_for_port(&attach_host, attach_port, PORT_WAIT, POLL_INTERVAL).await {
        // Include what the spawner said — a breakaway/CreateProcess failure
        // message here is the difference between a diagnosis and a mystery.
        let spawner = {
            let combined = format!("{} {}", run.stdout.trim(), run.stderr.trim());
            let combined = combined.trim();
            if combined.is_empty() {
                String::new()
            } else {
                format!(" (spawner said: {combined})")
            }
        };
        anyhow::bail!(
            "Launched Lich but {}:{} never opened within {}s — check the launch command and \
             that the character/game are valid{}",
            attach_host,
            attach_port,
            PORT_WAIT.as_secs(),
            spawner
        );
    }

    progress(LaunchProgress::Ready {
        host: attach_host.clone(),
        port: attach_port,
    });

    Ok(LaunchTarget {
        host: attach_host,
        port: attach_port,
        character: spec.character.clone(),
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

    #[test]
    fn loopback_hosts_are_recognized_as_local() {
        for host in [
            "localhost",
            "LOCALHOST",
            "127.0.0.1",
            "::1",
            "[::1]",
            "0.0.0.0",
            "",
            "  ",
        ] {
            assert!(is_local_host(host), "{host:?} should be local");
        }
        // A LAN address is NOT assumed local even if it happens to be this
        // box: guessing wrong would silently skip SSH on a real remote setup.
        for host in [
            "192.168.86.234",
            "10.0.0.5",
            "home.example.com",
            "127.0.0.2",
        ] {
            assert!(!is_local_host(host), "{host:?} should be remote");
        }
    }

    #[test]
    fn from_command_takes_the_local_path_for_loopback_and_ssh_otherwise() {
        let ssh = SshConfig::default();

        // Same machine: spawn directly, no SSH key required.
        let spec = LaunchSpec::from_command(
            "rubyw lich.rbw --login Nisugi",
            "127.0.0.1",
            8000,
            "Nisugi",
            &ssh,
        );
        assert!(spec.local, "loopback attach host must launch locally");
        assert_eq!(spec.program, "rubyw");
        assert_eq!(spec.attach_port, 8000);

        // Remote host: SSH path, and the SSH host defaults to the attach host.
        let spec =
            LaunchSpec::from_command("rubyw lich.rbw", "192.168.86.234", 8002, "Nisugi", &ssh);
        assert!(!spec.local, "a LAN host must still go over SSH");
        assert_eq!(spec.ssh_host, "192.168.86.234");

        // REGRESSION: the global ssh settings must NOT hijack a loopback
        // profile. ssh-launcher.toml is shared by every profile, so someone
        // who configured SSH once for a remote character would otherwise
        // find their 127.0.0.1 profile silently launching over the network —
        // a ~100s SSH round trip instead of a local spawn.
        let mut configured = SshConfig::default();
        configured.host = "192.168.86.234".to_string();
        let spec =
            LaunchSpec::from_command("rubyw lich.rbw", "127.0.0.1", 8002, "Nisugi", &configured);
        assert!(
            spec.local,
            "the per-profile host is the specific signal and must win over global ssh settings"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn local_launch_child_is_reaped_after_exit() {
        let root = tempfile::tempdir().unwrap();
        let pid_file = root.path().join("child.pid");
        let spec = LaunchSpec {
            ssh_host: String::new(),
            ssh_port: 22,
            ssh_user: String::new(),
            remote_os: crate::launcher::ssh::RemoteOs::Unix,
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!("echo $$ > {}; exit 0", pid_file.display()),
            ],
            attach_host: "127.0.0.1".to_string(),
            attach_port: 8000,
            character: "Test".to_string(),
            local: true,
        };

        spawn_local(&spec).expect("short-lived local process starts");
        for _ in 0..100 {
            if pid_file.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let pid = std::fs::read_to_string(&pid_file)
            .expect("child published its pid")
            .trim()
            .parse::<u32>()
            .expect("published pid is numeric");
        let proc_path = std::path::PathBuf::from(format!("/proc/{pid}"));
        for _ in 0..100 {
            if !proc_path.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        panic!("finished local launch child remained unreaped at {proc_path:?}");
    }

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
                lich_command:
                    "ruby lich.rb --login {character} --{game} --detachable-client={port}"
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

    fn ssh_settings() -> SshConfig {
        SshConfig {
            host: String::new(), // default to attach host
            user: "shawn".to_string(),
            port: 22,
            remote_os: RemoteOs::Windows,
            lich_command: String::new(),
            attach_host: String::new(),
            key_saved: true,
        }
    }

    /// from_command splits the launch line and defaults the SSH host to the
    /// attach host (Lich + SSHD on the same box).
    #[test]
    fn spec_from_command_defaults_ssh_host_to_attach() {
        let spec = LaunchSpec::from_command(
            "rubyw.exe \"C:\\lich\\lich.rbw\" --detachable-client=8001 \"&\"",
            "100.64.0.7",
            8001,
            "Nisugi",
            &ssh_settings(),
        );
        assert_eq!(spec.ssh_host, "100.64.0.7"); // defaulted from attach host
        assert_eq!(spec.ssh_user, "shawn");
        assert_eq!(spec.attach_port, 8001);
        assert_eq!(spec.program, "rubyw.exe");
        assert!(spec.args.contains(&"--detachable-client=8001".to_string()));
        assert!(!spec.args.iter().any(|a| a == "&"));
    }

    /// A non-empty settings host overrides the attach host (split-horizon).
    #[test]
    fn spec_from_command_settings_host_wins() {
        let mut ssh = ssh_settings();
        ssh.host = "10.0.0.9".to_string();
        let spec = LaunchSpec::from_command("ruby lich.rb", "100.64.0.7", 8001, "Nisugi", &ssh);
        assert_eq!(spec.ssh_host, "10.0.0.9");
    }

    /// launch_spec short-circuits to Ready when the port is already up, exactly
    /// like the config path — proving the mobile flow attaches directly when
    /// Lich is already running (the common "I just launched it" case).
    #[tokio::test]
    async fn launch_spec_already_running_short_circuits() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let spec = LaunchSpec::from_command(
            "rubyw.exe lich.rbw --detachable-client=1",
            "127.0.0.1",
            port,
            "Nisugi",
            &ssh_settings(),
        );
        let mut events = Vec::new();
        let target = launch_spec(&spec, HostKeyTrust::AutoPinFirstUse, |p| events.push(p))
            .await
            .expect("already-up should short-circuit");
        assert_eq!(target.port, port);
        assert!(events
            .iter()
            .any(|e| matches!(e, LaunchProgress::AlreadyRunning { .. })));
        assert!(!events
            .iter()
            .any(|e| matches!(e, LaunchProgress::Connecting { .. })));
    }
}
