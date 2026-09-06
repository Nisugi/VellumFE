//! Launcher profiles - saved connection entries for the GUI launcher
//!
//! Stored in `~/.vellum-fe/launcher.toml` (distinct from `profiles/`, which
//! holds per-character settings directories). Passwords are never written to
//! this file: when the user opts in, they are stored in the OS credential
//! store (Windows Credential Manager / macOS Keychain / Linux Secret Service)
//! keyed by account name. All keyring access is best-effort - on platforms
//! without a secret service (headless, WSL, bare window managers) the
//! launcher degrades to prompting for the password at launch time.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::Config;

/// File name inside the vellum-fe base directory.
const LAUNCHER_FILE: &str = "launcher.toml";

/// Keyring service identifier (the "folder" credentials appear under).
#[cfg(feature = "desktop")]
const KEYRING_SERVICE: &str = "vellum-fe";

/// Environment variable the launcher uses to hand a just-prompted password
/// to a spawned GUI session. Never placed on a command line (process lists
/// are world-readable); the child consumes and removes it immediately.
pub const PASSWORD_ENV: &str = "VELLUM_FE_PASSWORD";

/// Shared help text: used both for clap `--help` and launcher tooltips so
/// the two can never drift apart.
pub mod help {
    pub const MODE_DIRECT: &str =
        "Connect directly to the game via eAccess (no Lich). Requires account, password, game, and character.";
    pub const MODE_LICH: &str =
        "Connect to a running Lich instance in detachable-client mode (host and port).";
    pub const ACCOUNT: &str = "Play.net account name used for direct connections";
    pub const GAME: &str = "Game world for direct connections. GemStone IV: prime, platinum, shattered, test. DragonRealms: dr, drplatinum, drfallen, drtest";
    pub const CHARACTER: &str =
        "Character name to log in as; also selects the character's saved layout and settings";
    pub const HOST: &str = "Host where Lich is listening (default: 127.0.0.1)";
    pub const PORT: &str = "Port of Lich's detachable client (default: 8000)";
    pub const CUSTOM_LAUNCH: &str =
        "Optional: the command that starts Lich (e.g. `rubyw lich.rbw --login NAME --detachable-client=8000`). \
         When set, connecting first checks whether the port is already open; if not, Vellum runs this, \
         waits for the port, then attaches. If Host is localhost/127.0.0.1 the command runs on this machine \
         directly; any other host runs it over SSH (set the SSH user and key with .launcher). \
         Leave blank to attach only.";
    pub const FRONTEND: &str =
        "GUI opens Vellum's native window; Terminal runs the text interface in its own console window; Despana opens the Despana browser interface";
    pub const WEB_PORT: &str =
        "Enable the embedded web server on this port: serves a browser view of this session at localhost:PORT (e.g. for a phone on your LAN)";
    pub const WEB_BIND: &str =
        "Address the web server binds to (default: 127.0.0.1, this machine only). Set to 0.0.0.0 to let other devices on your network (e.g. a phone) connect.";
    pub const NOSOUND: &str =
        "Disable the sound system entirely (skips audio device initialization; use if audio causes startup trouble)";
    pub const SETTINGS_PROFILE: &str =
        "Use this settings folder instead of the character name - lets several characters share one layout and config";
    pub const DATA_DIR: &str =
        "Directory for configs, layouts, and logs (default: ~/.vellum-fe; also settable via VELLUM_FE_DIR)";
    pub const COLOR_MODE: &str =
        "Terminal color rendering: direct (true color RGB) or slot (256-color palette)";
    pub const SETUP_PALETTE: &str =
        "Set up the terminal palette on startup using .setpalette (use with slot color mode)";
    pub const SAVE_PASSWORD: &str =
        "Store the password in the operating system's secure credential store (never written to a file)";
}

/// Connection mode for a launcher profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMode {
    Direct,
    Lich,
}

/// Which frontend the spawned session uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchFrontend {
    Gui,
    Tui,
}

impl Default for LaunchFrontend {
    fn default() -> Self {
        LaunchFrontend::Gui
    }
}

/// Optional browser client selected for a launcher profile.
///
/// This is deliberately additive to [`LaunchFrontend`], rather than another
/// variant of that enum. Older Vellum builds ignore unknown profile fields,
/// so a Despana profile still falls back to its serialized native GUI choice
/// instead of making the whole `launcher.toml` unreadable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchWebClient {
    Despana,
}

impl LaunchWebClient {
    /// Stable browser route owned by this built-in presentation.
    pub const fn route(self) -> &'static str {
        match self {
            Self::Despana => "despana",
        }
    }

    /// User-facing launcher label for this browser presentation.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Despana => "Despana (Web)",
        }
    }
}

/// Game worlds selectable in the launcher, as (CLI value, display label).
/// The CLI value matches `--game` / `DirectConnectConfig::game_name_to_code`.
pub const GAME_CHOICES: &[(&str, &str)] = &[
    ("prime", "GemStone IV"),
    ("platinum", "GemStone IV Platinum"),
    ("shattered", "GemStone IV Shattered"),
    ("test", "GemStone IV Test"),
    ("dr", "DragonRealms"),
    ("drplatinum", "DragonRealms Platinum"),
    ("drfallen", "DragonRealms Fallen"),
    ("drtest", "DragonRealms Test"),
];

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8000
}

/// One saved connection in the launcher list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherProfile {
    /// Display name; also the key used by `--launch-profile`.
    pub name: String,
    pub mode: LaunchMode,

    // Direct-mode fields
    #[serde(default)]
    pub account: String,
    /// CLI game value ("prime", "platinum", ... - see GAME_CHOICES)
    #[serde(default)]
    pub game: String,
    /// True when the password for `account` was saved to the OS keyring.
    #[serde(default)]
    pub password_saved: bool,

    // Shared
    #[serde(default)]
    pub character: String,
    #[serde(default)]
    pub frontend: LaunchFrontend,
    /// An optional browser presentation layered over Vellum's headless core.
    /// `frontend` remains `gui` for Despana as a safe fallback for older
    /// Vellum builds that do not know this additive field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_client: Option<LaunchWebClient>,

    // Lich-mode fields
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Optional launch command for a Lich attach: the full `rubyw lich.rbw …
    /// --detachable-client=PORT` line the user runs to bring Lich up. When
    /// present, connecting to this profile probes the port first and, if Lich
    /// isn't up, SSHes to `host` and runs this command before attaching (the
    /// mobile cold-start QoL flow). The command carries everything Lich needs
    /// (`--login`, instance, `--detachable-client`); host/port here are the
    /// attach target. Never a secret — the SSH key lives in the secure store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_launch: Option<String>,

    // Advanced options (all map 1:1 onto CLI switches)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_port: Option<u16>,
    /// Bind address for the web server; None = 127.0.0.1 (loopback only).
    /// Set to "0.0.0.0" to allow other devices on the LAN to connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_bind: Option<String>,
    #[serde(default)]
    pub nosound: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_mode: Option<String>,
    #[serde(default)]
    pub setup_palette: bool,
}

impl LauncherProfile {
    pub fn new_direct() -> Self {
        Self {
            name: String::new(),
            mode: LaunchMode::Direct,
            account: String::new(),
            game: "prime".to_string(),
            password_saved: false,
            character: String::new(),
            frontend: LaunchFrontend::Gui,
            web_client: None,
            host: default_host(),
            port: default_port(),
            custom_launch: None,
            web_port: None,
            web_bind: None,
            nosound: false,
            settings_profile: None,
            data_dir: None,
            color_mode: None,
            setup_palette: false,
        }
    }

    /// One-line summary shown in the launcher list under the profile name.
    pub fn summary(&self) -> String {
        match self.mode {
            LaunchMode::Direct => {
                let game_label = GAME_CHOICES
                    .iter()
                    .find(|(value, _)| *value == self.game)
                    .map(|(_, label)| *label)
                    .unwrap_or(self.game.as_str());
                // No account name here: this list is on screen (and in
                // screenshots) constantly — the account stays in the edit form.
                format!("{} @ {}", self.character, game_label)
            }
            LaunchMode::Lich => {
                if self.character.is_empty() {
                    format!("Lich @ {}:{}", self.host, self.port)
                } else {
                    format!("{} via Lich @ {}:{}", self.character, self.host, self.port)
                }
            }
        }
    }

    /// Select a native frontend and clear any browser-client override.
    pub fn select_frontend(&mut self, frontend: LaunchFrontend) {
        self.frontend = frontend;
        self.web_client = None;
    }

    /// Select a built-in browser client. `frontend = "gui"` remains the
    /// serialized fallback for older Vellum builds that ignore `web_client`.
    pub fn select_web_client(&mut self, web_client: LaunchWebClient) {
        self.frontend = LaunchFrontend::Gui;
        self.web_client = Some(web_client);
    }
}

/// On-disk container: `[[profiles]]` entries in launcher.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LauncherStore {
    #[serde(default)]
    pub profiles: Vec<LauncherProfile>,
}

impl LauncherStore {
    /// Path of the launcher store (respects VELLUM_FE_DIR via base_dir).
    pub fn path() -> Result<PathBuf> {
        Ok(Config::base_dir()?.join(LAUNCHER_FILE))
    }

    /// Load from the default location; a missing file is an empty store.
    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("Failed to parse {}", path.display()))
    }

    /// Save to the default location.
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).context("Failed to serialize launcher profiles")?;
        crate::config::write_atomic(path, text)
            .with_context(|| format!("Failed to write {}", path.display()))
    }

    pub fn find(&self, name: &str) -> Option<&LauncherProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.name.eq_ignore_ascii_case(name))
    }

    /// Insert or replace by name (case-insensitive). `original_name` is the
    /// pre-edit name so renames replace instead of duplicating.
    pub fn upsert(&mut self, profile: LauncherProfile, original_name: Option<&str>) {
        let key = original_name.unwrap_or(profile.name.as_str());
        if let Some(existing) = self
            .profiles
            .iter_mut()
            .find(|entry| entry.name.eq_ignore_ascii_case(key))
        {
            *existing = profile;
        } else {
            self.profiles.push(profile);
        }
    }

    /// Remove a profile. Returns the removed entry.
    pub fn remove(&mut self, name: &str) -> Option<LauncherProfile> {
        let index = self
            .profiles
            .iter()
            .position(|profile| profile.name.eq_ignore_ascii_case(name))?;
        Some(self.profiles.remove(index))
    }

    /// True if any remaining profile still relies on a saved password for
    /// this account - used to decide whether deleting a profile should also
    /// delete the keyring entry.
    pub fn account_password_in_use(&self, account: &str) -> bool {
        self.profiles
            .iter()
            .any(|profile| profile.password_saved && profile.account.eq_ignore_ascii_case(account))
    }
}

/// Keyring entry for an account. Keyed by account (not per-character or
/// per-game) because play.net passwords are account-wide.
#[cfg(feature = "desktop")]
fn keyring_entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, &account.to_lowercase())
        .context("Failed to open OS credential store")
}

/// Store a password in the OS credential store. Errors are surfaced so the
/// launcher can tell the user the password was NOT saved.
#[cfg(feature = "desktop")]
pub fn save_password(account: &str, password: &str) -> Result<()> {
    keyring_entry(account)?
        .set_password(password)
        .context("Failed to save password to OS credential store")
}

/// Fetch a saved password. Any failure (no backend, no entry, access denied)
/// is treated as "not saved here" - callers fall back to prompting.
#[cfg(feature = "desktop")]
pub fn load_password(account: &str) -> Option<String> {
    match keyring_entry(account).and_then(|entry| {
        entry
            .get_password()
            .context("Failed to read password from OS credential store")
    }) {
        Ok(password) => Some(password),
        Err(err) => {
            tracing::debug!(account, "No saved password available: {err:#}");
            None
        }
    }
}

/// Best-effort delete; a missing entry or backend is not an error.
#[cfg(feature = "desktop")]
pub fn delete_password(account: &str) {
    if let Ok(entry) = keyring_entry(account) {
        if let Err(err) = entry.delete_credential() {
            tracing::debug!(account, "Keyring delete skipped: {err:#}");
        }
    }
}

// Without the `desktop` feature there is no OS credential store. Passwords
// live in `<VELLUM_FE_DIR>/passwords.toml` instead — on Android that resolves
// to the app's private internal storage, which is sandboxed per-app (the same
// trust level as Lich's config on desktop). Keystore-backed encryption at
// rest is a planned hardening (see the Android port plan).

#[cfg(not(feature = "desktop"))]
fn passwords_path() -> Result<PathBuf> {
    Ok(Config::base_dir()?.join("passwords.toml"))
}

#[cfg(not(feature = "desktop"))]
fn load_password_map() -> std::collections::HashMap<String, String> {
    let Ok(path) = passwords_path() else {
        return Default::default();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(not(feature = "desktop"))]
fn store_password_map(map: &std::collections::HashMap<String, String>) -> Result<()> {
    let path = passwords_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(map).context("Failed to serialize password store")?;
    crate::config::write_atomic(&path, text).context("Failed to write password store")
}

/// Sealing for stored password values (non-desktop builds). The 32-byte
/// key arrives as hex in VELLUM_PASSWORD_KEY — on Android the Kotlin shell
/// derives it from an Android-Keystore-wrapped master key before starting
/// the core. Without the key (desktop headless testing), values stay
/// plaintext; legacy plaintext entries always remain readable and are
/// re-sealed on the next save.
#[cfg(not(feature = "desktop"))]
mod seal {
    use base64::Engine as _;
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

    const PREFIX: &str = "enc:";

    fn decode_hex(s: &str) -> Option<Vec<u8>> {
        if s.len() % 2 != 0 {
            return None;
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect()
    }

    fn cipher() -> Option<ChaCha20Poly1305> {
        let hex = std::env::var("VELLUM_PASSWORD_KEY").ok()?;
        let bytes = decode_hex(hex.trim())?;
        if bytes.len() != 32 {
            return None;
        }
        Some(ChaCha20Poly1305::new(Key::from_slice(&bytes)))
    }

    pub fn seal(value: &str) -> String {
        let Some(c) = cipher() else {
            return value.to_string();
        };
        let mut nonce = [0u8; 12];
        if getrandom::fill(&mut nonce).is_err() {
            return value.to_string();
        }
        match c.encrypt(Nonce::from_slice(&nonce), value.as_bytes()) {
            Ok(ct) => {
                let mut blob = nonce.to_vec();
                blob.extend(ct);
                format!(
                    "{PREFIX}{}",
                    base64::engine::general_purpose::STANDARD.encode(blob)
                )
            }
            Err(_) => value.to_string(),
        }
    }

    pub fn open(value: &str) -> Option<String> {
        let Some(b64) = value.strip_prefix(PREFIX) else {
            // Legacy plaintext entry.
            return Some(value.to_string());
        };
        let blob = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
        if blob.len() < 13 {
            return None;
        }
        let (nonce, ct) = blob.split_at(12);
        let pt = cipher()?.decrypt(Nonce::from_slice(nonce), ct).ok()?;
        String::from_utf8(pt).ok()
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn seal_roundtrip_with_key() {
            std::env::set_var(
                "VELLUM_PASSWORD_KEY",
                "0101010101010101010101010101010101010101010101010101010101010101",
            );
            let sealed = super::seal("hunter2 with spaces");
            assert!(sealed.starts_with("enc:"));
            assert_eq!(super::open(&sealed).as_deref(), Some("hunter2 with spaces"));
            // Legacy plaintext passes through.
            assert_eq!(super::open("plain").as_deref(), Some("plain"));
            std::env::remove_var("VELLUM_PASSWORD_KEY");
        }
    }
}

#[cfg(not(feature = "desktop"))]
pub fn save_password(account: &str, password: &str) -> Result<()> {
    let mut map = load_password_map();
    map.insert(account.to_lowercase(), seal::seal(password));
    store_password_map(&map)
}

#[cfg(not(feature = "desktop"))]
pub fn load_password(account: &str) -> Option<String> {
    load_password_map()
        .get(&account.to_lowercase())
        .and_then(|v| seal::open(v))
}

#[cfg(not(feature = "desktop"))]
pub fn delete_password(account: &str) {
    let mut map = load_password_map();
    if map.remove(&account.to_lowercase()).is_some() {
        if let Err(err) = store_password_map(&map) {
            tracing::debug!(account, "Password store delete failed: {err:#}");
        }
    }
}

/// Seal an arbitrary secret with the same ChaCha20-Poly1305 key the password
/// store uses (non-desktop builds). Exposed so the SSH launcher can store its
/// private key sealed-at-rest without duplicating the crypto. On desktop the
/// launcher uses the OS keyring instead, so these are non-desktop only.
#[cfg(not(feature = "desktop"))]
pub fn seal_value(value: &str) -> String {
    seal::seal(value)
}

/// Open a value sealed by [`seal_value`] (or a legacy plaintext entry).
#[cfg(not(feature = "desktop"))]
pub fn open_value(value: &str) -> Option<String> {
    seal::open(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_direct() -> LauncherProfile {
        LauncherProfile {
            name: "Main".to_string(),
            account: "MYACCT".to_string(),
            game: "prime".to_string(),
            character: "Nisugi".to_string(),
            password_saved: true,
            web_port: Some(8080),
            web_bind: Some("0.0.0.0".to_string()),
            ..LauncherProfile::new_direct()
        }
    }

    fn sample_lich() -> LauncherProfile {
        LauncherProfile {
            name: "Lich local".to_string(),
            mode: LaunchMode::Lich,
            character: "Nisugi".to_string(),
            frontend: LaunchFrontend::Tui,
            port: 8003,
            ..LauncherProfile::new_direct()
        }
    }

    /// Touches the real OS credential store - run explicitly with
    /// `cargo test keyring_round_trip -- --ignored`.
    #[test]
    #[ignore]
    fn keyring_round_trip() {
        let account = "vellum-fe-selftest";
        save_password(account, "s3cret").expect("save to OS credential store");
        assert_eq!(load_password(account).as_deref(), Some("s3cret"));
        delete_password(account);
        assert_eq!(load_password(account), None);
    }

    #[test]
    fn toml_round_trip_preserves_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.toml");

        let store = LauncherStore {
            profiles: vec![sample_direct(), sample_lich()],
        };
        store.save_to(&path).unwrap();
        let loaded = LauncherStore::load_from(&path).unwrap();

        assert_eq!(loaded.profiles.len(), 2);
        let direct = &loaded.profiles[0];
        assert_eq!(direct.name, "Main");
        assert_eq!(direct.mode, LaunchMode::Direct);
        assert_eq!(direct.account, "MYACCT");
        assert_eq!(direct.game, "prime");
        assert_eq!(direct.character, "Nisugi");
        assert!(direct.password_saved);
        assert_eq!(direct.web_port, Some(8080));
        assert_eq!(direct.web_bind.as_deref(), Some("0.0.0.0"));
        assert_eq!(direct.frontend, LaunchFrontend::Gui);
        assert_eq!(direct.web_client, None);

        let lich = &loaded.profiles[1];
        assert_eq!(lich.mode, LaunchMode::Lich);
        assert_eq!(lich.frontend, LaunchFrontend::Tui);
        assert_eq!(lich.web_client, None);
        assert_eq!(lich.host, "127.0.0.1");
        assert_eq!(lich.port, 8003);
    }

    #[test]
    fn missing_file_loads_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = LauncherStore::load_from(&dir.path().join("nope.toml")).unwrap();
        assert!(store.profiles.is_empty());
    }

    #[test]
    fn minimal_toml_gets_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.toml");
        std::fs::write(&path, "[[profiles]]\nname = \"Bare\"\nmode = \"lich\"\n").unwrap();

        let store = LauncherStore::load_from(&path).unwrap();
        let profile = &store.profiles[0];
        assert_eq!(profile.host, "127.0.0.1");
        assert_eq!(profile.port, 8000);
        assert_eq!(profile.frontend, LaunchFrontend::Gui);
        assert_eq!(profile.web_client, None);
        assert!(!profile.password_saved);
        assert!(profile.web_port.is_none());
        assert!(profile.web_bind.is_none());
    }

    #[test]
    fn upsert_replaces_by_original_name_on_rename() {
        let mut store = LauncherStore {
            profiles: vec![sample_direct()],
        };
        let mut renamed = sample_direct();
        renamed.name = "Renamed".to_string();
        store.upsert(renamed, Some("Main"));

        assert_eq!(store.profiles.len(), 1);
        assert_eq!(store.profiles[0].name, "Renamed");
    }

    #[test]
    fn remove_and_password_in_use() {
        let mut store = LauncherStore {
            profiles: vec![sample_direct(), {
                let mut second = sample_direct();
                second.name = "Alt".to_string();
                second
            }],
        };
        assert!(store.account_password_in_use("myacct"));
        store.remove("Main");
        assert!(store.account_password_in_use("MYACCT"));
        store.remove("alt");
        assert!(!store.account_password_in_use("MYACCT"));
    }

    #[test]
    fn despana_web_client_round_trips_with_gui_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.toml");
        let mut profile = sample_lich();
        profile.select_web_client(LaunchWebClient::Despana);
        let store = LauncherStore {
            profiles: vec![profile],
        };

        store.save_to(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("frontend = \"gui\""));
        assert!(text.contains("web_client = \"despana\""));

        let loaded = LauncherStore::load_from(&path).unwrap();
        let profile = &loaded.profiles[0];
        assert_eq!(profile.frontend, LaunchFrontend::Gui);
        assert_eq!(profile.web_client, Some(LaunchWebClient::Despana));
        assert_eq!(profile.mode, LaunchMode::Lich);
        assert_eq!(profile.character, "Nisugi");
        assert_eq!(profile.port, 8003);
    }

    #[test]
    fn selecting_native_frontend_clears_the_browser_override() {
        let mut profile = sample_direct();

        profile.select_web_client(LaunchWebClient::Despana);
        assert_eq!(profile.frontend, LaunchFrontend::Gui);
        assert_eq!(profile.web_client, Some(LaunchWebClient::Despana));

        profile.select_frontend(LaunchFrontend::Tui);
        assert_eq!(profile.frontend, LaunchFrontend::Tui);
        assert_eq!(profile.web_client, None);

        profile.select_frontend(LaunchFrontend::Gui);
        assert_eq!(profile.frontend, LaunchFrontend::Gui);
        assert_eq!(profile.web_client, None);
    }

    #[test]
    fn built_in_web_client_has_stable_identity_metadata() {
        assert_eq!(LaunchWebClient::Despana.route(), "despana");
        assert_eq!(LaunchWebClient::Despana.label(), "Despana (Web)");
    }
}
