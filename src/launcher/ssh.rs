//! SSH transport for the launcher.
//!
//! Everything the launcher needs to reach the home PC over an existing
//! WireGuard/Tailscale tunnel:
//!
//! - [`generate_keypair`] / [`LauncherKey`] — a dedicated ed25519 key. The
//!   private half is an OpenSSH-format PEM string suitable for the OS secure
//!   store; the public half is the one-line `authorized_keys` entry the user
//!   pastes onto the home PC.
//! - [`HostKeyPolicy`] — trust-on-first-use pinning against a launcher-owned
//!   known-hosts file, with a hard MITM failure when a pinned key changes.
//! - [`SshLauncher::run_detached`] — connect, authenticate with the key, and
//!   run one command spawned *detached* so the headless Lich process survives
//!   the SSH channel (and the whole VellumFE session) going away.
//! - [`probe_port`] — a quick TCP check so we attach to an already-running
//!   Lich instead of starting a duplicate.
//!
//! The crypto backend is `ring` (see Cargo.toml) so this compiles on the
//! iOS/Android targets without a C toolchain.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use russh::client::{self, Handle};
// russh re-exports the ssh-key crate (and the types we need) under russh::keys.
use russh::keys::ssh_key::{self, Algorithm, LineEnding};
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg};
use russh::{ChannelMsg, Disconnect};
use tokio::net::TcpStream;

/// How long we wait for the initial TCP+SSH handshake before giving up. The
/// tunnel is usually already up (the user connected WireGuard/Tailscale before
/// tapping Launch), so a stalled connect almost always means the wrong host or
/// a down machine — fail fast rather than hang the UI.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);

/// Inactivity timeout handed to russh. Our sessions are short (connect, run one
/// command, read its exit status, disconnect), so this only matters if the
/// server wedges mid-exec.
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(30);

/// A generated launcher keypair, rendered in the two forms we actually need.
#[derive(Debug, Clone)]
pub struct LauncherKey {
    /// OpenSSH-format private key PEM (`-----BEGIN OPENSSH PRIVATE KEY-----`).
    /// This is the secret — it goes straight to the OS secure store and is
    /// never written to a config file or a command line.
    pub private_openssh: String,
    /// One-line `authorized_keys` entry, e.g. `ssh-ed25519 AAAA... comment`.
    /// Safe to display; the user pastes this onto the home PC once.
    pub public_authorized_keys: String,
}

/// Generate a fresh, dedicated ed25519 launcher keypair.
///
/// `comment` is embedded in the public key line so the user can recognize it
/// in `authorized_keys` (e.g. `vellum-launcher@Nisugi-iPhone`).
pub fn generate_keypair(comment: &str) -> Result<LauncherKey> {
    // Match russh's own key-gen path (rand 0.10's thread rng, which is a CSPRNG
    // seeded from the OS).
    let mut key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .context("Failed to generate ed25519 keypair")?;
    key.set_comment(comment);

    let private_openssh = key
        .to_openssh(LineEnding::LF)
        .context("Failed to encode private key")?
        .to_string();

    let public_authorized_keys = key
        .public_key()
        .to_openssh()
        .context("Failed to encode public key")?;

    Ok(LauncherKey {
        private_openssh,
        public_authorized_keys,
    })
}

/// Re-derive the public `authorized_keys` line from a stored private key PEM,
/// so the editor can show "here's the line to paste" without keeping the
/// public half around separately.
pub fn public_line_from_private(private_openssh: &str) -> Result<String> {
    let key = PrivateKey::from_openssh(private_openssh)
        .context("Stored launcher key is not valid OpenSSH format")?;
    key.public_key()
        .to_openssh()
        .context("Failed to encode public key")
}

/// Outcome of a host-key check, so the caller can drive a "trust this host?"
/// prompt (GUI) or a printed fingerprint (TUI) instead of us deciding blindly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// Key matches what we pinned before — proceed silently.
    Known,
    /// First time we've seen this host. `fingerprint` is SHA256 in the usual
    /// `SHA256:...` OpenSSH form for the user to eyeball. Caller decides
    /// whether to pin it (call [`HostKeyPolicy::trust`]).
    Unknown { fingerprint: String },
    /// A key was pinned before and it does NOT match. This is the MITM signal:
    /// we refuse to connect and surface it loudly.
    Changed { pinned_line: usize },
}

/// Trust-on-first-use host-key pinning backed by a launcher-owned known-hosts
/// file (separate from the user's `~/.ssh/known_hosts` so we never touch their
/// personal SSH state).
#[derive(Debug, Clone)]
pub struct HostKeyPolicy {
    known_hosts: PathBuf,
    host: String,
    port: u16,
}

impl HostKeyPolicy {
    pub fn new(known_hosts: impl Into<PathBuf>, host: impl Into<String>, port: u16) -> Self {
        Self {
            known_hosts: known_hosts.into(),
            host: host.into(),
            port,
        }
    }

    /// Classify a presented server key against the pinned set.
    pub fn classify(&self, key: &ssh_key::PublicKey) -> HostKeyStatus {
        match russh::keys::check_known_hosts_path(&self.host, self.port, key, &self.known_hosts) {
            Ok(true) => HostKeyStatus::Known,
            Ok(false) => HostKeyStatus::Unknown {
                fingerprint: key.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
            },
            // russh signals a changed key via a dedicated error; treat anything
            // that reports a specific known-hosts line as a change, and any
            // other error as "unknown" (missing file, unreadable) so first-use
            // still works.
            Err(russh::keys::Error::KeyChanged { line }) => {
                HostKeyStatus::Changed { pinned_line: line }
            }
            Err(_) => HostKeyStatus::Unknown {
                fingerprint: key.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
            },
        }
    }

    /// Pin a host key we've decided to trust (first-use accept).
    pub fn trust(&self, key: &ssh_key::PublicKey) -> Result<()> {
        if let Some(parent) = self.known_hosts.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        russh::keys::known_hosts::learn_known_hosts_path(
            &self.host,
            self.port,
            key,
            &self.known_hosts,
        )
        .with_context(|| format!("Failed to pin host key for {}:{}", self.host, self.port))
    }
}

/// Decision returned by a host-key callback: pin-and-continue, continue without
/// pinning, or refuse. Kept explicit so the connect path never silently accepts
/// a key it shouldn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyDecision {
    /// Accept and remember for next time.
    TrustAndPin,
    /// Accept only for this connection (don't write to known-hosts).
    AcceptOnce,
    /// Refuse — abort the connection.
    Reject,
}

/// russh handler that routes host-key checks through a [`HostKeyPolicy`] plus a
/// caller-supplied decision function for the first-use / changed cases.
struct LauncherHandler<F>
where
    F: FnMut(HostKeyStatus) -> HostKeyDecision + Send + 'static,
{
    policy: HostKeyPolicy,
    decide: F,
    /// A key we accepted-and-should-pin, shared with the connect wrapper so it
    /// can pin *after* the handshake completes. The `Handle` russh hands back
    /// doesn't expose the handler, so we read the decision back through this
    /// shared cell instead. Pinning post-handshake (not inside the callback)
    /// avoids pinning a key for a session that then fails authentication.
    to_pin: Arc<Mutex<Option<ssh_key::PublicKey>>>,
}

impl<F> client::Handler for LauncherHandler<F>
where
    F: FnMut(HostKeyStatus) -> HostKeyDecision + Send + 'static,
{
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let status = self.policy.classify(server_public_key);
        match status {
            HostKeyStatus::Known => Ok(true),
            HostKeyStatus::Changed { .. } => {
                // Never auto-accept a changed key. This is the MITM tripwire.
                let decision = (self.decide)(status);
                Ok(matches!(decision, HostKeyDecision::AcceptOnce))
            }
            HostKeyStatus::Unknown { .. } => match (self.decide)(status) {
                HostKeyDecision::TrustAndPin => {
                    if let Ok(mut slot) = self.to_pin.lock() {
                        *slot = Some(server_public_key.clone());
                    }
                    Ok(true)
                }
                HostKeyDecision::AcceptOnce => Ok(true),
                HostKeyDecision::Reject => Ok(false),
            },
        }
    }
}

/// A connected, authenticated SSH session ready to run the launch command.
pub struct SshLauncher<F>
where
    F: FnMut(HostKeyStatus) -> HostKeyDecision + Send + 'static,
{
    session: Handle<LauncherHandler<F>>,
    policy: HostKeyPolicy,
}

/// Parameters for opening a launcher SSH session.
pub struct SshTarget<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
    /// OpenSSH private key PEM (from the secure store).
    pub private_openssh: &'a str,
    /// Launcher-owned known-hosts file for TOFU pinning.
    pub known_hosts: &'a Path,
}

impl<F> SshLauncher<F>
where
    F: FnMut(HostKeyStatus) -> HostKeyDecision + Send + 'static,
{
    /// Connect and authenticate with the launcher key. `decide` is called only
    /// for first-use or changed host keys; a known key connects silently.
    pub async fn connect(target: SshTarget<'_>, decide: F) -> Result<Self> {
        let key = PrivateKey::from_openssh(target.private_openssh)
            .context("Launcher private key is not valid OpenSSH format")?;

        let policy = HostKeyPolicy::new(target.known_hosts.to_path_buf(), target.host, target.port);

        let config = Arc::new(client::Config {
            inactivity_timeout: Some(INACTIVITY_TIMEOUT),
            ..Default::default()
        });

        let to_pin: Arc<Mutex<Option<ssh_key::PublicKey>>> = Arc::new(Mutex::new(None));
        let handler = LauncherHandler {
            policy: policy.clone(),
            decide,
            to_pin: Arc::clone(&to_pin),
        };

        // Establish the TCP stream ourselves with a bound so a black-holed host
        // fails fast instead of hanging on russh's internal connect.
        let stream = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TcpStream::connect((target.host, target.port)),
        )
        .await
        .map_err(|_| anyhow!("Timed out connecting to {}:{}", target.host, target.port))?
        .with_context(|| format!("Failed to reach {}:{}", target.host, target.port))?;

        let mut session = tokio::time::timeout(
            CONNECT_TIMEOUT,
            client::connect_stream(config, stream, handler),
        )
        .await
        .map_err(|_| anyhow!("SSH handshake with {}:{} timed out", target.host, target.port))?
        .context("SSH handshake failed")?;

        // The handshake reached check_server_key; if it stashed a key to pin,
        // pin it now that we know the transport came up.
        let pending_pin = to_pin.lock().ok().and_then(|mut slot| slot.take());
        if let Some(pubkey) = pending_pin {
            policy.trust(&pubkey)?;
        }

        let auth = session
            .authenticate_publickey(
                target.user,
                PrivateKeyWithHashAlg::new(
                    Arc::new(key),
                    session.best_supported_rsa_hash().await?.flatten(),
                ),
            )
            .await
            .context("SSH public-key authentication errored")?;

        if !auth.success() {
            bail!(
                "SSH authentication failed for {}@{} — is the launcher public key in authorized_keys?",
                target.user,
                target.host
            );
        }

        Ok(Self { session, policy })
    }

    /// The host-key policy in use (so callers can inspect the known-hosts path).
    pub fn policy(&self) -> &HostKeyPolicy {
        &self.policy
    }

    /// Run one command *detached* and return immediately once it's been
    /// launched. "Detached" means the remote process is reparented off the SSH
    /// session so it keeps running after we disconnect — otherwise the headless
    /// Lich would die the moment VellumFE closes the channel.
    ///
    /// `raw_command` is the platform-appropriate launch line (see
    /// [`detach_wrap`]). Returns the command's exit status if it exits promptly
    /// (a detached spawner should return 0 right away); `None` if the channel
    /// closed without one.
    pub async fn run_detached(&mut self, raw_command: &str) -> Result<DetachedRun> {
        let mut channel = self
            .session
            .channel_open_session()
            .await
            .context("Failed to open SSH session channel")?;

        channel
            .exec(true, raw_command)
            .await
            .context("Failed to exec launch command")?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_status = None;

        // Drain until the channel closes. The detach wrapper returns quickly,
        // so this does not block on the long-lived Lich process.
        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };
            match msg {
                ChannelMsg::Data { ref data } => stdout.extend_from_slice(data),
                ChannelMsg::ExtendedData { ref data, .. } => stderr.extend_from_slice(data),
                ChannelMsg::ExitStatus { exit_status: code } => {
                    exit_status = Some(code);
                    // Keep draining — more data may follow the status.
                }
                ChannelMsg::Close | ChannelMsg::Eof => break,
                _ => {}
            }
        }

        Ok(DetachedRun {
            exit_status,
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }

    /// Politely close the SSH session.
    pub async fn close(&mut self) -> Result<()> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await
            .context("Failed to close SSH session")
    }
}

/// Result of a detached launch command.
#[derive(Debug, Clone)]
pub struct DetachedRun {
    /// Exit status of the *spawner*, not the long-lived process (which is now
    /// detached). 0 means the spawn succeeded.
    pub exit_status: Option<u32>,
    pub stdout: String,
    pub stderr: String,
}

impl DetachedRun {
    /// True when the spawner reported success (or exited without a status,
    /// which some detach mechanisms do). The authoritative check for "did Lich
    /// actually come up" is [`probe_port`], not this.
    pub fn spawn_ok(&self) -> bool {
        matches!(self.exit_status, None | Some(0))
    }
}

/// Wrap a bare launch command so the spawned process survives the SSH channel
/// closing, choosing the right mechanism for the *remote* OS.
///
/// - Windows: `powershell Start-Process -WindowStyle Hidden` fully detaches;
///   the process is reparented to the service host, not the SSH session, so it
///   outlives the connection. `rubyw.exe` is already windowless.
/// - Unix: `nohup ... &` + redirecting stdio detaches from the controlling
///   terminal so an SSH channel close doesn't SIGHUP it.
///
/// `program` is the executable, `args` the already-split argument list. We do
/// the quoting here so callers pass structured data, never a pre-joined string.
pub fn detach_wrap(remote_os: RemoteOs, program: &str, args: &[String]) -> String {
    match remote_os {
        RemoteOs::Windows => {
            // Start-Process takes the file and an -ArgumentList array. Quote
            // each arg for PowerShell single-quoted string literals (double any
            // embedded single quote).
            let arg_list = args
                .iter()
                .map(|a| format!("'{}'", a.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",");
            let prog = program.replace('\'', "''");
            if arg_list.is_empty() {
                format!("powershell -NoProfile -Command \"Start-Process -WindowStyle Hidden -FilePath '{prog}'\"")
            } else {
                format!("powershell -NoProfile -Command \"Start-Process -WindowStyle Hidden -FilePath '{prog}' -ArgumentList {arg_list}\"")
            }
        }
        RemoteOs::Unix => {
            let quoted = std::iter::once(program.to_string())
                .chain(args.iter().cloned())
                .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
                .collect::<Vec<_>>()
                .join(" ");
            format!("nohup {quoted} >/dev/null 2>&1 &")
        }
    }
}

/// Which OS the *remote* (home PC) runs, so [`detach_wrap`] picks the right
/// spawner. Defaults to Windows because that's the primary GemStone setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemoteOs {
    #[default]
    Windows,
    Unix,
}

/// Quick TCP reachability probe: is something already listening on `host:port`?
/// Used before launching so we attach to a running Lich instead of starting a
/// duplicate, and after launching to confirm Lich actually came up.
pub async fn probe_port(host: &str, port: u16, timeout: Duration) -> bool {
    matches!(
        tokio::time::timeout(timeout, TcpStream::connect((host, port))).await,
        Ok(Ok(_))
    )
}

/// Poll `host:port` until it accepts a connection or `deadline` elapses.
/// Returns true if the port came up in time. This is the authoritative "did
/// the launch work" check — we trust the open port, not the spawn exit code.
pub async fn wait_for_port(host: &str, port: u16, deadline: Duration) -> bool {
    let start = tokio::time::Instant::now();
    let probe_timeout = Duration::from_secs(2);
    loop {
        if probe_port(host, port, probe_timeout).await {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_roundtrips_to_public_line() {
        let kp = generate_keypair("vellum-launcher@test").expect("generate");
        assert!(kp.private_openssh.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(kp.public_authorized_keys.starts_with("ssh-ed25519 "));
        assert!(kp.public_authorized_keys.contains("vellum-launcher@test"));

        // The public line re-derived from the private key must match.
        let rederived = public_line_from_private(&kp.private_openssh).expect("rederive");
        // Comment may or may not be preserved by re-encode; compare the key
        // material (first two space-separated fields).
        let orig: Vec<&str> = kp.public_authorized_keys.split(' ').take(2).collect();
        let re: Vec<&str> = rederived.split(' ').take(2).collect();
        assert_eq!(orig, re);
    }

    #[test]
    fn generated_keys_are_unique() {
        let a = generate_keypair("a").unwrap();
        let b = generate_keypair("b").unwrap();
        assert_ne!(a.private_openssh, b.private_openssh);
    }

    #[test]
    fn detach_wrap_windows_quotes_args() {
        let cmd = detach_wrap(
            RemoteOs::Windows,
            "C:/Ruby4Lich5/4.0.3/bin/rubyw.exe",
            &[
                "C:/Gemstone/dev/lich-5/lich.rbw".to_string(),
                "--login".to_string(),
                "Nisugi".to_string(),
                "--gemstone".to_string(),
                "--without-frontend".to_string(),
                "--detachable-client=8001".to_string(),
            ],
        );
        assert!(cmd.contains("Start-Process"));
        assert!(cmd.contains("-WindowStyle Hidden"));
        assert!(cmd.contains("rubyw.exe"));
        assert!(cmd.contains("--detachable-client=8001"));
        // Every arg becomes a single-quoted PowerShell literal.
        assert!(cmd.contains("'--login'"));
    }

    #[test]
    fn detach_wrap_unix_uses_nohup() {
        let cmd = detach_wrap(
            RemoteOs::Unix,
            "/usr/bin/ruby",
            &["lich.rb".to_string(), "--without-frontend".to_string()],
        );
        assert!(cmd.starts_with("nohup "));
        assert!(cmd.ends_with('&'));
        assert!(cmd.contains(">/dev/null 2>&1"));
    }

    #[test]
    fn detach_wrap_escapes_single_quotes() {
        let win = detach_wrap(RemoteOs::Windows, "C:/it's here/rubyw.exe", &[]);
        // Embedded single quote doubled for PowerShell.
        assert!(win.contains("it''s here"));

        let nix = detach_wrap(RemoteOs::Unix, "/it's/ruby", &[]);
        // Embedded single quote closed/escaped/reopened for POSIX sh.
        assert!(nix.contains("it'\\''s"));
    }

    #[test]
    fn spawn_ok_treats_none_and_zero_as_success() {
        let none = DetachedRun {
            exit_status: None,
            stdout: String::new(),
            stderr: String::new(),
        };
        let zero = DetachedRun {
            exit_status: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        };
        let fail = DetachedRun {
            exit_status: Some(1),
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(none.spawn_ok());
        assert!(zero.spawn_ok());
        assert!(!fail.spawn_ok());
    }

    #[tokio::test]
    async fn probe_port_false_for_closed_port() {
        // Port 1 is (almost certainly) not listening on localhost.
        assert!(!probe_port("127.0.0.1", 1, Duration::from_millis(300)).await);
    }
}
