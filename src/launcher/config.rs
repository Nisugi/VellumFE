//! Configuration for the SSH launcher.
//!
//! Stored in `~/.vellum-fe/ssh-launcher.toml`. Like the connection-profile
//! store in [`crate::config::profiles`], this file holds **no secrets**: the
//! ed25519 private key lives in the OS secure store (keyring on desktop,
//! Keystore/Keychain on mobile via the same seal path passwords use). The TOML
//! carries only the SSH target, the launch-command template, and a per-target
//! map of characters to their game world + detachable-client port.
//!
//! # Shape
//!
//! ```toml
//! [ssh]
//! host = "100.x.x.x"          # tailnet / WireGuard address of the home PC
//! user = "shawn"
//! port = 22
//! remote_os = "windows"
//! # The launch-command TEMPLATE. {game}, {character}, {port} are substituted.
//! lich_command = 'C:/Ruby4Lich5/4.0.3/bin/rubyw.exe C:/Gemstone/dev/lich-5/lich.rbw --login {character} --{game} --without-frontend --bind-address=lan --detachable-client={port}'
//! # host we ATTACH to after launch (usually the same tunnel address)
//! attach_host = "100.x.x.x"
//! key_saved = true            # private key present in the OS secure store
//!
//! [characters.Nisugi]
//! game = "gemstone"
//! port = 8001
//!
//! [characters.Alt]
//! game = "gemstone"
//! port = 8002
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Config;

/// File name inside the vellum-fe base directory.
const LAUNCHER_FILE: &str = "ssh-launcher.toml";

/// Keyring service id — shares the `vellum-fe` "folder" the password store
/// uses, but with a distinct account prefix so launcher keys never collide
/// with play.net account passwords.
#[cfg(feature = "desktop")]
const KEYRING_SERVICE: &str = "vellum-fe";

/// Prefix applied to the keyring account name for launcher private keys, so a
/// profile named "home" stores under "ssh-launcher-key:home".
const KEY_ACCOUNT_PREFIX: &str = "ssh-launcher-key:";

fn default_ssh_port() -> u16 {
    22
}

/// Which OS the remote (home PC) runs — decides the detach mechanism. Mirrors
/// [`super::ssh::RemoteOs`] but is serde-friendly and lives in config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteOs {
    #[default]
    Windows,
    Unix,
}

impl From<RemoteOs> for super::ssh::RemoteOs {
    fn from(value: RemoteOs) -> Self {
        match value {
            RemoteOs::Windows => super::ssh::RemoteOs::Windows,
            RemoteOs::Unix => super::ssh::RemoteOs::Unix,
        }
    }
}

/// One character's launch parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterLaunch {
    /// Game-world token substituted into `{game}` of the command template.
    /// For GemStone this is the family: "gemstone" (Lich reads --gemstone).
    /// Kept free-form so DragonRealms / instances work without code changes.
    pub game: String,
    /// Detachable-client port this character's Lich listens on. Distinct ports
    /// let several characters run headless at once.
    pub port: u16,
}

/// SSH target + launch-command template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConfig {
    pub host: String,
    pub user: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub remote_os: RemoteOs,
    /// Launch-command template. Placeholders `{character}`, `{game}`, `{port}`
    /// are substituted per character. Stored as the raw program+args string;
    /// [`SshConfig::resolve_command`] splits and templates it.
    pub lich_command: String,
    /// Host to attach to after the launch. Usually the same tunnel address as
    /// `host`; separated so an odd split-horizon setup still works.
    #[serde(default)]
    pub attach_host: String,
    /// True when a private key for this config is in the OS secure store.
    #[serde(default)]
    pub key_saved: bool,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            user: String::new(),
            port: default_ssh_port(),
            remote_os: RemoteOs::Windows,
            lich_command: String::new(),
            attach_host: String::new(),
            key_saved: false,
        }
    }
}

impl SshConfig {
    /// The host to attach to after launch — `attach_host` if set, else `host`.
    pub fn effective_attach_host(&self) -> &str {
        if self.attach_host.is_empty() {
            &self.host
        } else {
            &self.attach_host
        }
    }

    /// Split the templated command into (program, args) with placeholders
    /// substituted for one character. Splitting is whitespace-based on the
    /// template; each arg then has placeholders replaced, so a path with a
    /// space must not appear un-quoted in the template (paths with spaces are
    /// rare for Ruby/Lich installs, and the detach wrapper re-quotes anyway).
    pub fn resolve_command(&self, character: &str, launch: &CharacterLaunch) -> Result<(String, Vec<String>)> {
        if self.lich_command.trim().is_empty() {
            bail!("Launch command template is empty — set it in the launcher editor");
        }
        let substitute = |s: &str| -> String {
            s.replace("{character}", character)
                .replace("{game}", &launch.game)
                .replace("{port}", &launch.port.to_string())
        };
        let mut parts = self.lich_command.split_whitespace().map(substitute);
        let program = parts
            .next()
            .context("Launch command template has no program")?;
        let args: Vec<String> = parts.collect();
        Ok((program, args))
    }
}

/// The whole on-disk launcher config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherConfig {
    #[serde(default)]
    pub ssh: SshConfig,
    /// Character name -> launch params. BTreeMap for stable TOML ordering.
    #[serde(default)]
    pub characters: BTreeMap<String, CharacterLaunch>,
}

impl LauncherConfig {
    pub fn path() -> Result<PathBuf> {
        Ok(Config::base_dir()?.join(LAUNCHER_FILE))
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("Failed to parse {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).context("Failed to serialize launcher config")?;
        crate::config::write_atomic(path, text)
            .with_context(|| format!("Failed to write {}", path.display()))
    }

    /// Look up a character's launch parameters (case-insensitive).
    pub fn character(&self, name: &str) -> Option<&CharacterLaunch> {
        self.characters
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }

    /// Resolve the full launch for a character: (program, args, attach_host,
    /// attach_port). Errors if the character is unknown or the template is bad.
    pub fn resolve(&self, character: &str) -> Result<ResolvedLaunch> {
        let launch = self
            .character(character)
            .with_context(|| format!("No launcher entry for character '{character}'"))?;
        let (program, args) = self.ssh.resolve_command(character, launch)?;
        Ok(ResolvedLaunch {
            program,
            args,
            attach_host: self.ssh.effective_attach_host().to_string(),
            attach_port: launch.port,
        })
    }
}

/// A fully-resolved launch ready to hand to the flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLaunch {
    pub program: String,
    pub args: Vec<String>,
    pub attach_host: String,
    pub attach_port: u16,
}

// === Private-key storage (OS secure store) =================================
//
// Desktop uses the OS keyring (Windows Credential Manager / macOS Keychain /
// Linux Secret Service) exactly like the password store. Non-desktop builds
// (Android/iOS core) reuse the same ChaCha20-Poly1305 seal the password store
// uses, so the key is encrypted at rest under the Keystore/Keychain-wrapped
// master key. A "key id" — the profile name — selects which key.

/// Where the launcher-owned known-hosts file lives (TOFU pins). Separate from
/// the user's ~/.ssh/known_hosts so we never touch their personal SSH state.
pub fn known_hosts_path() -> Result<PathBuf> {
    Ok(Config::base_dir()?.join("ssh-launcher-known-hosts"))
}

#[cfg(feature = "desktop")]
fn keyring_entry(key_id: &str) -> Result<keyring::Entry> {
    let account = format!("{KEY_ACCOUNT_PREFIX}{}", key_id.to_lowercase());
    keyring::Entry::new(KEYRING_SERVICE, &account)
        .context("Failed to open OS credential store for launcher key")
}

/// Store the launcher private key (OpenSSH PEM) in the OS secure store.
#[cfg(feature = "desktop")]
pub fn save_private_key(key_id: &str, private_openssh: &str) -> Result<()> {
    keyring_entry(key_id)?
        .set_password(private_openssh)
        .context("Failed to save launcher key to OS credential store")
}

/// Fetch the launcher private key, or None if absent / no backend.
#[cfg(feature = "desktop")]
pub fn load_private_key(key_id: &str) -> Option<String> {
    match keyring_entry(key_id).and_then(|entry| {
        entry
            .get_password()
            .context("Failed to read launcher key from OS credential store")
    }) {
        Ok(pem) => Some(pem),
        Err(err) => {
            tracing::debug!(key_id, "No saved launcher key: {err:#}");
            None
        }
    }
}

/// Best-effort delete of a stored launcher key.
#[cfg(feature = "desktop")]
pub fn delete_private_key(key_id: &str) {
    if let Ok(entry) = keyring_entry(key_id) {
        if let Err(err) = entry.delete_credential() {
            tracing::debug!(key_id, "Launcher key delete skipped: {err:#}");
        }
    }
}

// Non-desktop: reuse the sealed-file approach from the password store. The key
// material is larger than a password but the mechanism is identical.
#[cfg(not(feature = "desktop"))]
mod mobile_store {
    use super::*;
    use std::collections::HashMap;

    fn keys_path() -> Result<PathBuf> {
        Ok(Config::base_dir()?.join("ssh-launcher-keys.toml"))
    }

    fn load_map() -> HashMap<String, String> {
        let Ok(path) = keys_path() else {
            return Default::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn store_map(map: &HashMap<String, String>) -> Result<()> {
        let path = keys_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string(map).context("Failed to serialize launcher key store")?;
        crate::config::write_atomic(&path, text).context("Failed to write launcher key store")
    }

    pub fn save(key_id: &str, private_openssh: &str) -> Result<()> {
        let mut map = load_map();
        map.insert(
            key_id.to_lowercase(),
            crate::config::profiles::seal_value(private_openssh),
        );
        store_map(&map)
    }

    pub fn load(key_id: &str) -> Option<String> {
        load_map()
            .get(&key_id.to_lowercase())
            .and_then(|v| crate::config::profiles::open_value(v))
    }

    pub fn delete(key_id: &str) {
        let mut map = load_map();
        if map.remove(&key_id.to_lowercase()).is_some() {
            let _ = store_map(&map);
        }
    }
}

#[cfg(not(feature = "desktop"))]
pub fn save_private_key(key_id: &str, private_openssh: &str) -> Result<()> {
    mobile_store::save(key_id, private_openssh)
}

#[cfg(not(feature = "desktop"))]
pub fn load_private_key(key_id: &str) -> Option<String> {
    mobile_store::load(key_id)
}

#[cfg(not(feature = "desktop"))]
pub fn delete_private_key(key_id: &str) {
    mobile_store::delete(key_id)
}

/// Fixed key id for the single-target launcher config. If we later allow
/// multiple SSH targets this becomes per-target; today there is one.
pub const DEFAULT_KEY_ID: &str = "default";

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LauncherConfig {
        let mut characters = BTreeMap::new();
        characters.insert(
            "Nisugi".to_string(),
            CharacterLaunch {
                game: "gemstone".to_string(),
                port: 8001,
            },
        );
        characters.insert(
            "Alt".to_string(),
            CharacterLaunch {
                game: "gemstone".to_string(),
                port: 8002,
            },
        );
        LauncherConfig {
            ssh: SshConfig {
                host: "100.64.0.5".to_string(),
                user: "shawn".to_string(),
                port: 22,
                remote_os: RemoteOs::Windows,
                lich_command:
                    "C:/Ruby/rubyw.exe C:/lich/lich.rbw --login {character} --{game} --without-frontend --detachable-client={port}"
                        .to_string(),
                attach_host: String::new(),
                key_saved: true,
            },
            characters,
        }
    }

    #[test]
    fn toml_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LAUNCHER_FILE);
        let cfg = sample();
        cfg.save_to(&path).unwrap();
        let loaded = LauncherConfig::load_from(&path).unwrap();
        assert_eq!(cfg, loaded);
    }

    #[test]
    fn missing_file_is_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = LauncherConfig::load_from(&dir.path().join("nope.toml")).unwrap();
        assert!(cfg.characters.is_empty());
        assert_eq!(cfg.ssh.port, 22);
    }

    #[test]
    fn minimal_toml_gets_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LAUNCHER_FILE);
        std::fs::write(
            &path,
            "[ssh]\nhost = \"h\"\nuser = \"u\"\nlich_command = \"ruby lich.rb\"\n",
        )
        .unwrap();
        let cfg = LauncherConfig::load_from(&path).unwrap();
        assert_eq!(cfg.ssh.port, 22);
        assert_eq!(cfg.ssh.remote_os, RemoteOs::Windows);
        assert!(!cfg.ssh.key_saved);
        assert!(cfg.characters.is_empty());
    }

    #[test]
    fn resolve_substitutes_placeholders() {
        let cfg = sample();
        let resolved = cfg.resolve("Nisugi").unwrap();
        assert!(resolved.program.ends_with("rubyw.exe"));
        assert!(resolved.args.contains(&"--login".to_string()));
        assert!(resolved.args.contains(&"Nisugi".to_string()));
        assert!(resolved.args.contains(&"--gemstone".to_string()));
        assert!(resolved
            .args
            .contains(&"--detachable-client=8001".to_string()));
        assert_eq!(resolved.attach_host, "100.64.0.5");
        assert_eq!(resolved.attach_port, 8001);
    }

    #[test]
    fn resolve_is_case_insensitive_on_character() {
        let cfg = sample();
        assert!(cfg.resolve("nisugi").is_ok());
        assert!(cfg.resolve("NISUGI").is_ok());
    }

    #[test]
    fn resolve_unknown_character_errors() {
        let cfg = sample();
        assert!(cfg.resolve("Nobody").is_err());
    }

    #[test]
    fn resolve_empty_template_errors() {
        let mut cfg = sample();
        cfg.ssh.lich_command = "   ".to_string();
        assert!(cfg.resolve("Nisugi").is_err());
    }

    #[test]
    fn attach_host_falls_back_to_ssh_host() {
        let mut cfg = sample();
        cfg.ssh.attach_host = String::new();
        assert_eq!(cfg.resolve("Nisugi").unwrap().attach_host, "100.64.0.5");
        cfg.ssh.attach_host = "10.0.0.9".to_string();
        assert_eq!(cfg.resolve("Nisugi").unwrap().attach_host, "10.0.0.9");
    }

    /// Touches the real OS credential store — run with
    /// `cargo test launcher::config::tests::key_round_trip -- --ignored`.
    #[cfg(feature = "desktop")]
    #[test]
    #[ignore]
    fn key_round_trip() {
        let id = "vellum-launcher-selftest";
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----";
        save_private_key(id, pem).expect("save");
        assert_eq!(load_private_key(id).as_deref(), Some(pem));
        delete_private_key(id);
        assert_eq!(load_private_key(id), None);
    }
}
