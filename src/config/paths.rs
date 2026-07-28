//! Filesystem locations for all VellumFE config artifacts.
//!
//! Directory layout under ~/.vellum-fe (or VELLUM_FE_DIR), per-character
//! profile paths, and dialog-position persistence (widget_state.toml).

use super::*;

/// Write a user config file safely: write to a sibling `.tmp` file, back up
/// the existing file to `<name>.bak`, then rename over the target. A crash
/// mid-write can no longer truncate user data, and the previous version is
/// always one rename away.
///
/// Use this for every file a user (or an editor UI) authors. First-run
/// default extraction and cache files don't need it.
pub fn write_atomic(path: &std::path::Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contents)?;
    if path.exists() {
        let mut bak = path.as_os_str().to_owned();
        bak.push(".bak");
        // Best-effort backup: a failed copy must not block the save itself.
        let _ = std::fs::copy(path, PathBuf::from(bak));
    }
    std::fs::rename(&tmp, path)
}

/// Saved dialog position for persistence across sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogPosition {
    pub x: u16,
    pub y: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u16>,
}

/// TOML file wrapper for saved dialog positions (widget_state.toml)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedDialogPositions {
    #[serde(default)]
    pub dialogs: HashMap<String, DialogPosition>,
    /// Saved positions for ephemeral container windows (keyed by container title)
    #[serde(default)]
    pub containers: HashMap<String, DialogPosition>,
}

impl Config {
    /// Expose base directory path (~/.vellum-fe) for other systems (e.g., direct auth).
    pub fn base_dir() -> Result<PathBuf> {
        Self::config_dir()
    }

    /// Get the profile directory for a character (or "default" if none)
    /// Returns: ~/.vellum-fe/profiles/{character}/ or ~/.vellum-fe/profiles/default/
    pub(crate) fn profile_dir(character: Option<&str>) -> Result<PathBuf> {
        let profile_name = character.unwrap_or("default");
        Ok(Self::config_dir()?.join("profiles").join(profile_name))
    }

    /// Get the base vellum-fe directory (~/.vellum-fe/)
    /// Can be overridden with VELLUM_FE_DIR environment variable
    pub(super) fn config_dir() -> Result<PathBuf> {
        // Check for custom directory from environment variable
        if let Ok(custom_dir) = std::env::var("VELLUM_FE_DIR") {
            return Ok(PathBuf::from(custom_dir));
        }

        // Default to ~/.vellum-fe
        let home = dirs::home_dir().context("Could not find home directory")?;
        Ok(home.join(".vellum-fe"))
    }

    /// Get path to config.toml for a character
    /// Returns: ~/.vellum-fe/{character}/config.toml or ~/.vellum-fe/default/config.toml
    pub fn config_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("config.toml"))
    }

    /// Get path to colors.toml for a character
    /// Returns: ~/.vellum-fe/{character}/colors.toml
    pub fn colors_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("colors.toml"))
    }

    /// Get the shared layouts directory (where .savelayout saves to)
    /// Returns: ~/.vellum-fe/layouts/
    pub(super) fn layouts_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("layouts"))
    }

    /// Get the shared highlights directory (where .savehighlights saves to)
    /// Returns: ~/.vellum-fe/highlights/
    pub(super) fn highlights_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("highlights"))
    }

    /// Get the shared keybinds directory (where .savekeybinds saves to)
    /// Returns: ~/.vellum-fe/keybinds/
    pub(super) fn keybinds_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("keybinds"))
    }

    /// Get the global directory (for all shared resources)
    /// Returns: ~/.vellum-fe/global/
    pub(super) fn global_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("global"))
    }

    /// Get the shared sounds directory
    /// Returns: ~/.vellum-fe/global/sounds/
    pub fn sounds_dir() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("sounds"))
    }

    /// Get the shared skins directory (one subdirectory per skin, each with a
    /// skin.toml manifest plus its image assets)
    /// Returns: ~/.vellum-fe/skins/
    pub fn skins_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("skins"))
    }

    /// Get the shared hotbar icon-sheet store: sheet images plus an
    /// icons.toml manifest, available to every skin (and with no skin active)
    /// Returns: ~/.vellum-fe/global/icons/
    pub fn global_icons_dir() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("icons"))
    }

    /// Get the data-pack local store (gameobj-data.xml and friends; see
    /// `core::data_pack`)
    /// Returns: ~/.vellum-fe/global/data/
    pub fn global_data_dir() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("data"))
    }

    /// Get path to common (global) highlights file
    /// Returns: ~/.vellum-fe/global/highlights.toml
    pub fn common_highlights_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("highlights.toml"))
    }

    /// Get path to common (global) keybinds file
    /// Returns: ~/.vellum-fe/global/keybinds.toml
    pub fn common_keybinds_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("keybinds.toml"))
    }

    /// Get path to common (global) hotbars file
    /// Returns: ~/.vellum-fe/global/hotbars.toml
    pub fn common_hotbars_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("hotbars.toml"))
    }

    /// Get path to common (global) colors file
    /// Returns: ~/.vellum-fe/global/colors.toml
    pub fn common_colors_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("colors.toml"))
    }

    /// Get path to common (global) config file
    /// Returns: ~/.vellum-fe/global/config.toml
    pub fn common_config_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("config.toml"))
    }

    /// Get path to debug log for a character
    /// Returns: ~/.vellum-fe/{character}/debug.log
    pub fn get_log_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("debug.log"))
    }

    /// Get path to command history for a character
    /// Returns: ~/.vellum-fe/{character}/history.txt
    pub fn history_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("history.txt"))
    }

    /// Get path to widget_state.toml for a character
    /// Returns: ~/.vellum-fe/{character}/widget_state.toml
    pub fn widget_state_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("widget_state.toml"))
    }

    /// Load saved dialog positions from widget_state.toml for a character
    pub fn load_dialog_positions(character: Option<&str>) -> Result<SavedDialogPositions> {
        let path = Self::widget_state_path(character)?;
        if !path.exists() {
            return Ok(SavedDialogPositions::default());
        }

        let contents = fs::read_to_string(&path)
            .context(format!("Failed to read widget state at {:?}", path))?;
        let positions: SavedDialogPositions = toml::from_str(&contents)
            .context(format!("Failed to parse widget state at {:?}", path))?;

        Ok(positions)
    }

    /// Save dialog positions to widget_state.toml for a character
    pub fn save_dialog_positions(
        character: Option<&str>,
        positions: &SavedDialogPositions,
    ) -> Result<()> {
        let path = Self::widget_state_path(character)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(positions)
            .context("Failed to serialize dialog positions")?;
        write_atomic(&path, contents)
            .context(format!("Failed to write widget state to {:?}", path))?;
        Ok(())
    }

    /// Get path to cmdlist1.xml (single source of truth)
    /// Returns: ~/.vellum-fe/global/cmdlist1.xml
    pub fn cmdlist_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("cmdlist1.xml"))
    }

    /// Get path to spell abbreviations (perception window)
    /// Returns: ~/.vellum-fe/global/spell_abbrev.toml
    pub fn spell_abbrev_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("spell_abbrev.toml"))
    }

    /// Get path to highlights.toml for a character
    /// Returns: ~/.vellum-fe/{character}/highlights.toml
    pub fn highlights_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("highlights.toml"))
    }

    /// Get path to keybinds.toml for a character
    /// Returns: ~/.vellum-fe/{character}/keybinds.toml
    pub fn keybinds_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("keybinds.toml"))
    }

    /// Get path to hotbars.toml for a character
    /// Returns: ~/.vellum-fe/{character}/hotbars.toml
    pub fn hotbars_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("hotbars.toml"))
    }

    /// Get path to auto-saved layout.toml for a character
    /// Returns: ~/.vellum-fe/{character}/layout.toml
    pub fn auto_layout_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("layout.toml"))
    }

    /// List all saved layouts
    pub fn list_layouts() -> Result<Vec<String>> {
        let layouts_dir = Self::config_dir()?.join("layouts");

        if !layouts_dir.exists() {
            return Ok(vec![]);
        }

        let mut layouts = vec![];
        for entry in fs::read_dir(layouts_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    layouts.push(name.to_string());
                }
            }
        }

        layouts.sort();
        Ok(layouts)
    }

    pub fn layout_path(name: &str) -> Result<PathBuf> {
        let layouts_dir = Self::layouts_dir()?;
        Ok(layouts_dir.join(format!("{}.toml", name)))
    }


    /// List all saved keybind profiles
    pub fn list_saved_keybinds() -> Result<Vec<String>> {
        let keybinds_dir = Self::keybinds_dir()?;

        if !keybinds_dir.exists() {
            return Ok(vec![]);
        }

        let mut profiles = vec![];
        for entry in fs::read_dir(keybinds_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    profiles.push(name.to_string());
                }
            }
        }

        profiles.sort();
        Ok(profiles)
    }

    /// Save current keybinds to a named profile
    /// Returns path to saved keybinds
    pub fn save_keybinds_as(&self, name: &str) -> Result<PathBuf> {
        let keybinds_dir = Self::keybinds_dir()?;
        fs::create_dir_all(&keybinds_dir)?;

        let keybinds_path = keybinds_dir.join(format!("{}.toml", name));
        let contents =
            toml::to_string_pretty(&self.keybinds).context("Failed to serialize keybinds")?;
        write_atomic(&keybinds_path, contents).context("Failed to write keybinds profile")?;

        Ok(keybinds_path)
    }

    /// Load the web pairing token, generating it on first use. Lives in
    /// the shared base dir (not per-profile) so one phone pairing covers
    /// every character and switching sessions never re-prompts.
    pub fn load_or_create_web_token() -> Result<String> {
        let path = Self::base_dir()?.join("web-token");
        if let Ok(existing) = fs::read_to_string(&path) {
            let token = existing.trim().to_string();
            if !token.is_empty() {
                return Ok(token);
            }
        }
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).context("Failed to generate web token")?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic(&path, &token).context("Failed to write web-token")?;
        tracing::info!("Generated web pairing token at {:?}", path);
        Ok(token)
    }

    /// Load keybinds from a named profile
    pub fn load_keybinds_from(name: &str) -> Result<HashMap<String, KeyBindAction>> {
        let keybinds_dir = Self::keybinds_dir()?;
        let keybinds_path = keybinds_dir.join(format!("{}.toml", name));

        if !keybinds_path.exists() {
            return Err(anyhow::anyhow!("Keybind profile '{}' not found", name));
        }

        let contents =
            fs::read_to_string(&keybinds_path).context("Failed to read keybinds profile")?;
        let keybinds: HashMap<String, KeyBindAction> =
            toml::from_str(&contents).context("Failed to parse keybinds profile")?;

        Ok(keybinds)
    }
}

#[cfg(test)]
mod atomic_tests {
    use super::write_atomic;

    #[test]
    fn write_atomic_creates_file_and_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        write_atomic(&path, "a = 1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 1\n");
    }

    #[test]
    fn write_atomic_backs_up_previous_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_atomic(&path, "old\n").unwrap();
        write_atomic(&path, "new\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\n");
        let bak = dir.path().join("config.toml.bak");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), "old\n");
        // No stray temp file left behind.
        assert!(!dir.path().join("config.toml.tmp").exists());
    }
}
