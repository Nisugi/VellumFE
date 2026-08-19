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
        .map_err(|_| {
            anyhow!(
                "SSH handshake with {}:{} timed out",
                target.host,
                target.port
            )
        })?
        .context("SSH handshake failed")?;

        // The handshake reached check_server_key; if it stashed a key to pin,
        // pin it now that we know the transport came up.
        let pending_pin = to_pin.lock().ok().and_then(|mut slot| slot.take());
        if let Some(pubkey) = pending_pin {
            policy.trust(&pubkey)?;
        }

        // The hash-algorithm hint applies ONLY to RSA keys (rsa-sha2-256 /
        // rsa-sha2-512 negotiation). ed25519 (and ecdsa) have a single fixed
        // signature algorithm and MUST be given `None` — passing an RSA hash
        // alongside an ed25519 key makes russh sign with a mismatched
        // algorithm, which the server rejects as `Failed publickey` even
        // though the public key is in authorized_keys. Only query the RSA
        // hash when the key is actually RSA.
        let hash_alg = if key.algorithm().is_rsa() {
            session.best_supported_rsa_hash().await?.flatten()
        } else {
            None
        };
        let auth = session
            .authenticate_publickey(
                target.user,
                PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
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
        //
        // Do NOT break on Eof: OpenSSH sends `exit-status` after EOF, so
        // breaking there loses the exit code (and a silent `None` used to be
        // treated as spawn success). Only Close / stream-end terminate.
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
                // The server refused the exec request outright (bad shell,
                // restricted key, MaxSessions…). Surface it — pretending it
                // ran and timing out on the port poll is a debugging trap.
                ChannelMsg::Failure => {
                    bail!("SSH server rejected the exec request (channel failure)");
                }
                ChannelMsg::Close => break,
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
/// - Windows: Win32-OpenSSH places every exec in a job object with
///   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` — ALL children (including ones from
///   `Start-Process`) are killed the instant the exec command exits, before
///   the launcher even starts polling the port. The job does set
///   `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, so the one sanctioned escape is spawning
///   with `CREATE_BREAKAWAY_FROM_JOB`. No stock CLI passes that flag, so we
///   emit a small PowerShell Add-Type/P-Invoke `CreateProcessW` call, encoded
///   as `-EncodedCommand` (UTF-16LE base64) so no quoting survives the cmd.exe
///   hop. `CREATE_NO_WINDOW` keeps console apps (ruby.exe) invisible.
///   (Task Scheduler and WMI `Win32_Process.Create` are NOT alternatives here:
///   both are access-denied or inert for a standard user's network logon.)
/// - Unix: `nohup ... &` + redirecting stdio detaches from the controlling
///   terminal so an SSH channel close doesn't SIGHUP it.
///
/// `program` is the executable, `args` the already-split argument list. We do
/// the quoting here so callers pass structured data, never a pre-joined string.
pub fn detach_wrap(remote_os: RemoteOs, program: &str, args: &[String]) -> String {
    match remote_os {
        RemoteOs::Windows => {
            use base64::Engine as _;
            let script = windows_breakaway_script(&windows_cmdline(program, args));
            let utf16: Vec<u8> = script
                .encode_utf16()
                .flat_map(|u| u.to_le_bytes())
                .collect();
            format!(
                "powershell -NoProfile -EncodedCommand {}",
                base64::engine::general_purpose::STANDARD.encode(utf16)
            )
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

/// Build a Windows command line from program + args: every token
/// double-quoted, embedded double quotes backslash-escaped. Pragmatic C-rules
/// quoting for launch commands (paths + flags), same spirit as
/// [`split_command`], not a full CommandLineToArgvW inverse.
fn windows_cmdline(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(|t| format!("\"{}\"", t.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// PowerShell script that spawns `cmdline` with `CREATE_BREAKAWAY_FROM_JOB` so
/// it escapes sshd's kill-on-close job object (see [`detach_wrap`]). Prints
/// `spawned pid=N` and exits 0 on success; prints the Win32 error and exits 1
/// on failure, so [`DetachedRun::spawn_ok`] is meaningful.
///
/// The whole CreateProcessW call lives inside the C# helper: PowerShell runs
/// pipeline machinery between a P/Invoke returning and any later
/// `GetLastWin32Error()` read, so the error code must be captured in the same
/// managed frame. `lpCommandLine` is a StringBuilder because CreateProcessW is
/// allowed to scribble on it (an immutable string marshals as read-only).
/// What the emitted C# does (kept terse in the payload — sshd runs exec lines
/// through `cmd.exe /c`, whose 8191-char limit the UTF-16 base64 encoding
/// approaches fast, so the script carries no comments or long names):
///
/// - `V.S(cmdline)` calls `CreateProcessW` with flags `0x09000000` =
///   `CREATE_BREAKAWAY_FROM_JOB (0x01000000)` — escapes sshd's kill-on-close
///   job — `| CREATE_NO_WINDOW (0x08000000)` — keeps console apps invisible.
/// - The child gets REAL std handles (`STARTF_USESTDHANDLES`): rubyw.exe has
///   no console, and with null stdio the first write (e.g. Lich printing its
///   nonfatal Gtk.init backtrace when there's no desktop) kills the process.
///   stdout/stderr go to `%TEMP%\vellum-launcher-spawn.log` (a breadcrumb for
///   remote failures), stdin reads NUL; both fall back to NUL/null.
/// - Struct field names are positional abbreviations of STARTUPINFO /
///   PROCESS_INFORMATION / SECURITY_ATTRIBUTES in their documented order.
fn windows_breakaway_script(cmdline: &str) -> String {
    // Embed as a PowerShell single-quoted literal: double embedded quotes.
    let ps_literal = cmdline.replace('\'', "''");
    format!(
        r#"$ProgressPreference='SilentlyContinue'
$d=@'
using System;using System.Text;using System.Runtime.InteropServices;
public class V{{
[StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)]public struct SI{{public int cb;public string r1;public string r2;public string r3;public int x;public int y;public int xs;public int ys;public int xc;public int yc;public int fa;public int fl;public short sw;public short r4;public IntPtr r5;public IntPtr hi;public IntPtr ho;public IntPtr he;}}
[StructLayout(LayoutKind.Sequential)]public struct PI{{public IntPtr hp;public IntPtr ht;public int pid;public int tid;}}
[StructLayout(LayoutKind.Sequential)]public struct SA{{public int n;public IntPtr sd;public bool ih;}}
[DllImport("kernel32.dll",SetLastError=true,CharSet=CharSet.Unicode)]static extern bool CreateProcessW(string a,StringBuilder c,IntPtr pa,IntPtr ta,bool ih,uint f,IntPtr e,string wd,ref SI si,out PI pi);
[DllImport("kernel32.dll",SetLastError=true,CharSet=CharSet.Unicode)]static extern IntPtr CreateFileW(string n,uint a,uint s,ref SA sa,uint d,uint fl,IntPtr t);
static IntPtr O(string n,uint a,uint d){{SA s=new SA();s.n=Marshal.SizeOf(typeof(SA));s.ih=true;return CreateFileW(n,a,3,ref s,d,0x80,IntPtr.Zero);}}
public static string S(string c){{
SI si=new SI();si.cb=Marshal.SizeOf(typeof(SI));
string t=Environment.GetEnvironmentVariable("TEMP");if(t==null)t="C:\\Windows\\Temp";
IntPtr b=new IntPtr(-1);
IntPtr o=O(t+"\\vellum-launcher-spawn.log",0x40000000u,2);if(o==b)o=O("NUL",0x40000000u,3);
IntPtr i=O("NUL",0x80000000u,3);
si.fl=0x100;si.hi=(i==b)?IntPtr.Zero:i;si.ho=(o==b)?IntPtr.Zero:o;si.he=(o==b)?IntPtr.Zero:o;
PI pi;StringBuilder sb=new StringBuilder(c);
if(!CreateProcessW(null,sb,IntPtr.Zero,IntPtr.Zero,true,0x09000000u,IntPtr.Zero,null,ref si,out pi))return "err="+Marshal.GetLastWin32Error();
return "pid="+pi.pid;}}}}
'@
Add-Type -TypeDefinition $d
$r=[V]::S('{ps_literal}')
if($r.StartsWith('pid=')){{Write-Output ('spawned '+$r);exit 0}}
Write-Output ('CreateProcess failed, Win32 '+$r)
exit 1
"#
    )
}

/// Which OS the *remote* (home PC) runs, so [`detach_wrap`] picks the right
/// spawner. Defaults to Windows because that's the primary GemStone setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemoteOs {
    #[default]
    Windows,
    Unix,
}

/// Split a launch command line into (program, args), honoring double quotes so
/// a path with spaces stays one token. A trailing `&` (users copy the whole
/// PowerShell line, `… --detachable-client=8001 "&"`) is dropped — the detach
/// wrapper backgrounds the process itself. This is a pragmatic splitter for
/// the launch-command box, not a full shell parser: double quotes group,
/// everything else splits on whitespace.
pub fn split_command(line: &str) -> (String, Vec<String>) {
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut has_char = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                in_quote = !in_quote;
                has_char = true;
            }
            c if c.is_whitespace() && !in_quote => {
                if has_char {
                    tokens.push(std::mem::take(&mut cur));
                    has_char = false;
                }
            }
            c => {
                cur.push(c);
                has_char = true;
            }
        }
    }
    if has_char {
        tokens.push(cur);
    }
    // Drop a lone trailing "&" (or "&" the user pasted from the PowerShell line).
    if tokens.last().map(|t| t == "&").unwrap_or(false) {
        tokens.pop();
    }
    let program = tokens.first().cloned().unwrap_or_default();
    let args = tokens.into_iter().skip(1).collect();
    (program, args)
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

/// Poll `host:port` every `interval` until it accepts a connection or
/// `deadline` elapses. Returns true if the port came up in time. This is the
/// authoritative "did the launch work" check — we trust the open port, not the
/// spawn exit code.
pub async fn wait_for_port(host: &str, port: u16, deadline: Duration, interval: Duration) -> bool {
    let start = tokio::time::Instant::now();
    let probe_timeout = Duration::from_secs(2);
    loop {
        if probe_port(host, port, probe_timeout).await {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        tokio::time::sleep(interval).await;
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

    /// Decode the `-EncodedCommand` payload back to the PowerShell script.
    fn decode_encoded_command(wrapped: &str) -> String {
        use base64::Engine as _;
        let b64 = wrapped
            .rsplit(' ')
            .next()
            .expect("wrapper ends with the base64 payload");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("payload is valid base64");
        let utf16: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&utf16).expect("payload is valid UTF-16LE")
    }

    #[test]
    fn detach_wrap_windows_breaks_away_from_job() {
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
        // Encoded so quoting can't be mangled by the remote cmd.exe hop.
        assert!(cmd.starts_with("powershell -NoProfile -EncodedCommand "));

        // sshd runs exec through `cmd.exe /c`, which hard-fails past 8191
        // chars ("The command line is too long") — keep real headroom for
        // longer user launch commands.
        assert!(
            cmd.len() < 6500,
            "wrapped command is {} chars; approaching cmd.exe's 8191 limit",
            cmd.len()
        );

        let script = decode_encoded_command(&cmd);
        // Escapes sshd's kill-on-close job object (BREAKAWAY|NO_WINDOW) —
        // the whole point.
        assert!(script.contains("0x09000000u"));
        assert!(script.contains("CreateProcessW"));
        // Child must get real std handles — rubyw dies on its first write
        // otherwise (STARTF_USESTDHANDLES + breadcrumb log / NUL).
        assert!(script.contains("si.fl=0x100"));
        assert!(script.contains("vellum-launcher-spawn.log"));
        // The command line reaches the script with every token double-quoted.
        assert!(script.contains(r#""C:/Ruby4Lich5/4.0.3/bin/rubyw.exe" "C:/Gemstone/dev/lich-5/lich.rbw" "--login" "Nisugi" "--gemstone" "--without-frontend" "--detachable-client=8001""#));
        // Failure must exit nonzero so spawn_ok() reports it.
        assert!(script.contains("exit 1"));
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
        // Embedded single quote doubled inside the PowerShell literal that
        // carries the command line.
        let script = decode_encoded_command(&win);
        assert!(script.contains("it''s here"));

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

    #[test]
    fn split_command_handles_quoted_paths_and_trailing_amp() {
        // The exact line a user pastes from PowerShell.
        let (prog, args) = split_command(
            "C:\\Ruby4Lich5\\4.0.3\\bin\\rubyw.exe \"C:\\Gemstone\\dev\\lich-5\\lich.rbw\" \
             --login Nisugi --gemstone --without-frontend --bind-address=lan \
             --detachable-client=8001 \"&\"",
        );
        assert_eq!(prog, "C:\\Ruby4Lich5\\4.0.3\\bin\\rubyw.exe");
        // The quoted path with no spaces stays intact; the trailing "&" is dropped.
        assert_eq!(args[0], "C:\\Gemstone\\dev\\lich-5\\lich.rbw");
        assert!(args.contains(&"--login".to_string()));
        assert!(args.contains(&"Nisugi".to_string()));
        assert!(args.contains(&"--detachable-client=8001".to_string()));
        assert!(!args.iter().any(|a| a == "&"));
    }

    #[test]
    fn split_command_keeps_spaces_inside_quotes() {
        let (prog, args) = split_command("\"C:\\Program Files\\ruby\\rubyw.exe\" \"a b\" c");
        assert_eq!(prog, "C:\\Program Files\\ruby\\rubyw.exe");
        assert_eq!(args, vec!["a b".to_string(), "c".to_string()]);
    }

    #[test]
    fn split_command_empty_is_empty() {
        let (prog, args) = split_command("   ");
        assert_eq!(prog, "");
        assert!(args.is_empty());
    }
}
