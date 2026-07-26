//! Config loading, merging, and saving.
//!
//! Global + per-character TOML loading with defaults extraction on first
//! run, section-level merge rules, and single-setting save routing.

use super::*;

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_with_options(None, None)
    }

    /// Load config from a custom file path
    /// This loads the main config.toml from the specified path,
    /// but still loads colors, highlights, and keybinds from standard locations
    pub fn load_from_path(
        path: &std::path::Path,
        character: Option<&str>,
        port_override: Option<u16>,
    ) -> Result<Self> {
        // Ensure defaults are extracted
        Self::extract_defaults(character)?;

        // Load config from custom path
        let contents =
            fs::read_to_string(path).context(format!("Failed to read config file: {:?}", path))?;
        let mut config: Config = toml::from_str(&contents)
            .context(format!("Failed to parse config file: {:?}", path))?;

        // Same legacy drop-list migration as load_with_options (in-memory
        // only; never writes files).
        config.streams.migrate_drop_list_to_routes();

        // Override port from command line (if specified)
        if let Some(port) = port_override {
            config.connection.port = port;
        }

        // Store character name for later saves
        config.character = character.map(|s| s.to_string());

        // Load from separate files (from standard locations)
        config.colors = ColorConfig::load(character)?;
        config.highlights = Self::load_highlights(character)?;
        config.keybinds = Self::load_keybinds(character)?;
        config.hotbars = Self::load_hotbars(character)?;
        config.app_keybinds = Self::load_app_keybinds(character)?;
        config.macros = MacrosConfig::load(character).unwrap_or_default();
        config.macros_local = MacrosConfig::load_local(character).unwrap_or_default();

        // Validate and auto-fix menu keybinds
        let validation = menu_keybind_validator::validate_menu_keybinds(&config.menu_keybinds);
        if validation.has_errors() {
            tracing::warn!(
                "Menu keybind validation found {} errors",
                validation.errors().len()
            );
            for error in validation.errors() {
                tracing::warn!("  {}", error.message());
            }

            // Auto-fix critical issues
            let fixed = menu_keybind_validator::auto_fix_menu_keybinds(
                &mut config.menu_keybinds,
                &validation.issues,
            );
            if fixed > 0 {
                tracing::info!("Auto-fixed {} menu keybind issues", fixed);
            }
        }
        if validation.has_warnings() {
            for warning in validation.warnings() {
                tracing::warn!("Menu keybind warning: {}", warning.message());
            }
        }

        Ok(config)
    }

    /// Load config with command-line options
    /// Checks in order:
    /// 1. ./config/<character>.toml (if character specified)
    /// 2. ./config/default.toml
    /// 3. ~/.vellum-fe/<character>.toml (if character specified)
    /// 4. ~/.vellum-fe/config.toml (fallback)
    /// Extract default files on first run
    /// Creates shared directories and profile-specific files
    ///
    /// Global resources (shared by all characters):
    /// - ~/.vellum-fe/global/cmdlist1.xml
    /// - ~/.vellum-fe/global/keybinds.toml (default keybinds, char overrides in profile)
    /// - ~/.vellum-fe/global/sounds/wizard_music.mp3
    /// - ~/.vellum-fe/global/sounds/README.md
    ///
    /// Shared layouts:
    /// - ~/.vellum-fe/layouts/layout.toml
    /// - ~/.vellum-fe/layouts/none.toml
    /// - ~/.vellum-fe/layouts/sidebar.toml
    ///
    /// Profile-specific (default or character):
    /// - ~/.vellum-fe/profiles/{profile}/config.toml
    /// - ~/.vellum-fe/profiles/{profile}/history.txt (empty)
    /// Note: keybinds.toml in profile is optional (for character-specific overrides)
    fn extract_defaults(character: Option<&str>) -> Result<()> {
        // Create shared layouts directory and extract all embedded layouts
        let layouts_dir = Self::layouts_dir()?;
        fs::create_dir_all(&layouts_dir)?;

        // Automatically extract all files from embedded layouts directory
        for file in LAYOUTS_DIR.files() {
            let filename = file
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .context("Invalid layout filename")?;
            let layout_path = layouts_dir.join(filename);

            if !layout_path.exists() {
                let content = file
                    .contents_utf8()
                    .context(format!("Failed to read embedded layout {}", filename))?;
                fs::write(&layout_path, content)
                    .context(format!("Failed to write layouts/{}", filename))?;
                tracing::info!("Extracted layout {} to {:?}", filename, layout_path);
            }
        }

        // Create shared sounds directory and extract all embedded sounds
        let sounds_dir = Self::sounds_dir()?;
        fs::create_dir_all(&sounds_dir)?;

        // Automatically extract all files from embedded sounds directory
        for file in SOUNDS_DIR.files() {
            let filename = file
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .context("Invalid sound filename")?;
            let sound_path = sounds_dir.join(filename);

            if !sound_path.exists() {
                let content = file.contents();
                fs::write(&sound_path, content)
                    .context(format!("Failed to write sounds/{}", filename))?;
                tracing::info!("Extracted sound file {} to {:?}", filename, sound_path);
            }
        }

        // Extract cmdlist1.xml to global directory (only once)
        let global_dir = Self::global_dir()?;
        fs::create_dir_all(&global_dir)?;

        let cmdlist_path = Self::cmdlist_path()?;
        if !cmdlist_path.exists() {
            fs::write(&cmdlist_path, DEFAULT_CMDLIST).context("Failed to write cmdlist1.xml")?;
            tracing::info!("Extracted cmdlist1.xml to {:?}", cmdlist_path);
        }

        let hotbars_path = Self::common_hotbars_path()?;
        if !hotbars_path.exists() {
            fs::write(&hotbars_path, DEFAULT_HOTBARS).context("Failed to write hotbars.toml")?;
            tracing::info!("Extracted hotbars.toml to {:?}", hotbars_path);
        }

        let spell_abbrev_path = Self::spell_abbrev_path()?;
        if !spell_abbrev_path.exists() {
            fs::write(&spell_abbrev_path, DEFAULT_SPELL_ABBREVS)
                .context("Failed to write spell_abbrev.toml")?;
            tracing::info!(
                "Extracted spell_abbrev.toml to {:?}",
                spell_abbrev_path
            );
        }

        // Extract documented templates to global/templates directory
        // These preserve all comments and examples for user reference
        let templates_dir = global_dir.join("templates");
        fs::create_dir_all(&templates_dir)?;

        let template_config_path = templates_dir.join("config_template.toml");
        if !template_config_path.exists() {
            fs::write(&template_config_path, DEFAULT_CONFIG_TEMPLATE)
                .context("Failed to write config_template.toml")?;
            tracing::info!(
                "Extracted documented config_template.toml to {:?}",
                template_config_path
            );
        }

        let template_layout_path = templates_dir.join("layout_template.toml");
        if !template_layout_path.exists() {
            fs::write(&template_layout_path, DEFAULT_LAYOUT_TEMPLATE)
                .context("Failed to write layout_template.toml")?;
            tracing::info!(
                "Extracted documented layout_template.toml to {:?}",
                template_layout_path
            );
        }

        // Create profile directory
        let profile = Self::profile_dir(character)?;
        fs::create_dir_all(&profile)?;
        tracing::info!("Created profile directory: {:?}", profile);

        // config.toml is deliberately NOT extracted: the embedded defaults
        // are the bottom layer at load time (see load_layered_config), so
        // shipped default changes reach every user automatically. A
        // global/config.toml appears only once someone saves a global
        // setting, and then holds just their overrides. (Existing extracted
        // files keep working as the global layer and thin on next save.)
        // The commented reference copy lives in templates/config_template.toml.

        // Extract colors.toml to global directory (shared across all characters)
        // Character-specific overrides can still be added to profile/colors.toml
        let colors_path = Self::common_colors_path()?;
        if !colors_path.exists() {
            fs::write(&colors_path, DEFAULT_COLORS).context("Failed to write colors.toml")?;
            tracing::info!("Extracted colors.toml to {:?}", colors_path);
        }

        // Extract highlights.toml to global directory (shared across all characters)
        // Character-specific overrides can still be added to profile/highlights.toml
        let highlights_path = Self::common_highlights_path()?;
        if !highlights_path.exists() {
            fs::write(&highlights_path, DEFAULT_HIGHLIGHTS)
                .context("Failed to write highlights.toml")?;
            tracing::info!("Extracted highlights.toml to {:?}", highlights_path);
        }

        // Extract keybinds.toml to global directory (shared across all characters)
        // Character-specific overrides can still be added to profile/keybinds.toml
        let keybinds_path = Self::common_keybinds_path()?;
        if !keybinds_path.exists() {
            fs::write(&keybinds_path, DEFAULT_KEYBINDS).context("Failed to write keybinds.toml")?;
            tracing::info!("Extracted keybinds.toml to {:?}", keybinds_path);
        }

        // Extract macros.toml (web frontend macro buttons) to global directory
        let macros_path = Self::global_dir()?.join("macros.toml");
        if !macros_path.exists() {
            fs::write(&macros_path, DEFAULT_MACROS).context("Failed to write macros.toml")?;
            tracing::info!("Extracted macros.toml to {:?}", macros_path);
        }

        // Create empty history.txt in profile (if it doesn't exist)
        let history_path = profile.join("history.txt");
        if !history_path.exists() {
            fs::write(&history_path, "").context("Failed to create history.txt")?;
            tracing::info!("Created empty history.txt at {:?}", history_path);
        }

        // Merge newly shipped default highlights/keybinds/hotbars into the
        // extracted files (tombstoned: user deletions stay deleted), and
        // refresh managed data files the user never modified.
        super::defaults_refresh::refresh_shipped_defaults()?;

        Ok(())
    }

    /// Read one config layer's raw file contents, if the file exists.
    fn read_layer(path: &std::path::Path) -> Result<Option<String>> {
        if path.exists() {
            Ok(Some(fs::read_to_string(path).with_context(|| {
                format!("Failed to read config layer: {:?}", path)
            })?))
        } else {
            Ok(None)
        }
    }

    /// The fully layered view for a character: defaults < global < profile.
    pub fn load_layered_config(character: Option<&str>) -> Result<Self> {
        let global = Self::read_layer(&Self::common_config_path()?)?;
        let profile = Self::read_layer(&Self::config_path(character)?)?;
        super::sparse::layer_config(global.as_deref(), profile.as_deref())
    }

    /// The global layer only: defaults < global file. This is the base a
    /// profile save diffs against.
    fn load_global_layer() -> Result<Self> {
        let global = Self::read_layer(&Self::common_config_path()?)?;
        super::sparse::layer_config(global.as_deref(), None)
    }

    /// Load common (global) config defaults
    /// Returns: Config from ~/.vellum-fe/global/config.toml, or defaults if not found
    pub fn load_common_config() -> Result<Self> {
        let global_path = Self::common_config_path()?;
        if global_path.exists() {
            let contents = fs::read_to_string(&global_path)
                .context(format!("Failed to read global config: {:?}", global_path))?;
            toml::from_str(&contents)
                .context(format!("Failed to parse global config: {:?}", global_path))
        } else {
            // Return default config if no global file exists
            Ok(Self::default())
        }
    }

    /// Load ONLY character-specific config (no merge with global)
    /// Returns: Config from ~/.vellum-fe/profiles/{char}/config.toml, or None if not found
    pub fn load_character_config_only(character: Option<&str>) -> Result<Option<Self>> {
        let config_path = Self::config_path(character)?;
        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .context(format!("Failed to read character config: {:?}", config_path))?;
            let config: Config = toml::from_str(&contents)
                .context(format!("Failed to parse character config: {:?}", config_path))?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Merge character config overrides onto self
    /// NOTE: connection section ALWAYS comes from character config (never merged)
    pub fn merge_with(&mut self, character_config: Config) {
        // Connection ALWAYS comes from character (credentials, host, game)
        self.connection = character_config.connection;

        // Other sections: character overrides global if non-default
        // For now, character config completely overrides these sections if present
        // UI settings
        self.ui = character_config.ui;

        // Sound settings
        self.sound = character_config.sound;

        // TTS settings
        self.tts = character_config.tts;

        // Target list settings
        self.target_list = character_config.target_list;

        // Logging settings
        self.logging = character_config.logging;

        // Event patterns: merge (character extends global)
        for (key, pattern) in character_config.event_patterns {
            self.event_patterns.insert(key, pattern);
        }

        // Layout mappings: character replaces global if provided
        if !character_config.layout_mappings.is_empty() {
            self.layout_mappings = character_config.layout_mappings;
        }

        // Menu keybinds: character overrides global
        self.menu_keybinds = character_config.menu_keybinds;

        // Active theme: character overrides global
        self.active_theme = character_config.active_theme;

        // Active skin: character overrides global. Without this line the
        // skin picked in-app never survived a restart — Config::save
        // writes the whole config (skin included) to the profile file,
        // and the merge silently dropped it on the next load.
        self.active_skin = character_config.active_skin;

        // Streams config: character overrides global
        self.streams = character_config.streams;

        // Highlight settings: character overrides global
        self.highlight_settings = character_config.highlight_settings;

        // Quickbars: character overrides global
        self.quickbars = character_config.quickbars;

        // Web server, map discovery, go2 travel: character overrides
        // global. Same restart-amnesia bug as active_skin — these are
        // saved to the profile config but were dropped by the merge.
        self.web = character_config.web;
        self.map = character_config.map;
        self.go2 = character_config.go2;
    }

    pub fn load_with_options(character: Option<&str>, port_override: Option<u16>) -> Result<Self> {
        // Extract defaults on first run (idempotent - only creates missing files)
        Self::extract_defaults(character)?;

        // Layer per-leaf: code defaults < global file < profile file. Only
        // keys a file actually states override the layer below — a profile
        // file that sets one value no longer resets whole sections.
        let mut config = Self::load_layered_config(character)?;

        // Legacy [streams] drop_unsubscribed entries become routes."<id>" =
        // "discard" and the drop list is cleared, so runtime code has one
        // source of truth. In-memory only — load never writes files; the
        // next sparse save carries routes and ages the old key out.
        config.streams.migrate_drop_list_to_routes();

        // Override port from command line (if specified)
        if let Some(port) = port_override {
            config.connection.port = port;
        }

        // Store character name for later saves
        config.character = character.map(|s| s.to_string());

        // Load from separate files (these already have global/character merge logic)
        config.colors = ColorConfig::load(character)?;
        config.highlights = Self::load_highlights(character)?;
        config.keybinds = Self::load_keybinds(character)?;
        config.hotbars = Self::load_hotbars(character)?;
        config.app_keybinds = Self::load_app_keybinds(character)?;
        config.menu_keybinds = Self::load_menu_keybinds(character)?;
        config.macros = MacrosConfig::load(character).unwrap_or_default();
        config.macros_local = MacrosConfig::load_local(character).unwrap_or_default();

        // Validate and auto-fix menu keybinds
        let validation = menu_keybind_validator::validate_menu_keybinds(&config.menu_keybinds);
        if validation.has_errors() {
            tracing::warn!(
                "Menu keybind validation found {} errors",
                validation.errors().len()
            );
            for error in validation.errors() {
                tracing::warn!("  {}", error.message());
            }

            // Auto-fix critical issues
            let fixed = menu_keybind_validator::auto_fix_menu_keybinds(
                &mut config.menu_keybinds,
                &validation.issues,
            );
            if fixed > 0 {
                tracing::info!("Auto-fixed {} menu keybind issues", fixed);
            }
        }
        if validation.has_warnings() {
            for warning in validation.warnings() {
                tracing::warn!("Menu keybind warning: {}", warning.message());
            }
        }

        Ok(config)
    }

    /// Write `config` to `path` sparsely: the file ends up stating exactly
    /// the keys that differ from `base`, with comments on surviving keys
    /// preserved and unknown keys left alone.
    fn save_sparse(path: &std::path::Path, config: &Config, base: &Config) -> Result<()> {
        let existing = Self::read_layer(path)?.unwrap_or_default();
        let contents = super::sparse::sparse_config_toml(&existing, config, base)?;
        write_atomic(path, contents)
            .with_context(|| format!("Failed to write config file: {:?}", path))?;
        Ok(())
    }

    pub fn save(&self, character: Option<&str>) -> Result<()> {
        // Use provided character name, or fall back to stored character name
        let char_name = character.or(self.character.as_deref());
        let config_path = Self::config_path(char_name)?;

        // The profile file keeps only what diverges from the global layer,
        // so global edits keep reaching this character. (The old full dump
        // pinned every setting into the profile forever.)
        let base = Self::load_global_layer()?;
        Self::save_sparse(&config_path, self, &base)?;

        // Save to separate files
        self.colors.save(char_name)?;
        self.save_highlights(char_name)?;
        self.save_keybinds(char_name)?;

        Ok(())
    }

    /// Save config to global config.toml (sparse against the code defaults).
    pub fn save_common(&self) -> Result<()> {
        let config_path = Self::common_config_path()?;
        Self::save_sparse(&config_path, self, &Config::default())?;
        tracing::info!("Saved config to global file: {:?}", config_path);
        Ok(())
    }

    /// Save a single setting to the appropriate file based on scope
    /// NOTE: Connection settings MUST always go to character config (never global)
    pub fn save_single_setting(
        &self,
        key: &str,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        // Character-only settings (connection identity, pinned ports)
        // never save to global, regardless of the requested scope.
        let character_only = crate::config::registry::find(key)
            .map(|def| def.scope == crate::config::registry::SettingScope::CharacterOnly)
            .unwrap_or_else(|| key.starts_with("connection."));
        let actual_is_global = if character_only { false } else { is_global };

        if actual_is_global {
            self.save_setting_to_global(key)
        } else {
            self.save_setting_to_character(key, character)
        }
    }

    /// Save a specific setting to global config
    fn save_setting_to_global(&self, key: &str) -> Result<()> {
        // Load current global config
        let mut global_config = Self::load_common_config()?;

        // Update the specific setting
        Self::copy_setting(&mut global_config, self, key);

        // Save global config
        global_config.save_common()
    }

    /// Save a specific setting to character config
    fn save_setting_to_character(&self, key: &str, character: Option<&str>) -> Result<()> {
        // Start from the character's layered view (not the raw file):
        // fields the profile never set match the global layer exactly, so
        // the sparse diff below writes this key plus existing overrides —
        // never phantom "overrides" of serde defaults.
        let mut char_config = Self::load_layered_config(character)?;
        Self::copy_setting(&mut char_config, self, key);

        let config_path = Self::config_path(character)?;
        let base = Self::load_global_layer()?;
        Self::save_sparse(&config_path, &char_config, &base)?;
        tracing::info!("Saved setting '{}' to character config: {:?}", key, config_path);
        Ok(())
    }

    /// Copy a specific setting from source to destination config, resolved
    /// through the settings registry — every registered key works, not a
    /// hand-picked subset. Returns false (with a warning) for unknown keys
    /// so callers can surface the miss instead of silently no-op saving.
    pub(crate) fn copy_setting(dest: &mut Config, src: &Config, key: &str) -> bool {
        let Some(def) = crate::config::registry::find(key) else {
            tracing::warn!("Unknown setting key for copy: {}", key);
            return false;
        };
        let value = (def.get)(src);
        match (def.set)(dest, &value) {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!("Failed to copy setting {}: {}", key, err);
                false
            }
        }
    }

}

impl Default for Config {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig {
                host: default_host(),
                port: default_port(),
                character: None,
                account: None,
                password: None,
                game: None,
            },
            ui: UiConfig {
                buffer_size: default_buffer_size(),
                layout: LayoutConfig::default(),
                border_style: default_border_style(),
                countdown_icon: default_countdown_icon(),
                selection_enabled: default_selection_enabled(),
                selection_respect_window_boundaries: default_selection_respect_window_boundaries(),
                selection_auto_copy: default_selection_auto_copy(),
                drag_modifier_key: default_drag_modifier_key(),
                min_command_length: default_min_command_length(),
                performance_stats_enabled: default_performance_stats_enabled(),
                perf_stats_x: default_perf_stats_x(),
                perf_stats_y: default_perf_stats_y(),
                perf_stats_width: default_perf_stats_width(),
                perf_stats_height: default_perf_stats_height(),
                perf_show_fps: true,
                perf_show_frame_times: false,
                perf_show_render_times: true,
                perf_show_ui_times: true,
                perf_show_wrap_times: true,
                perf_show_net: true,
                perf_show_parse: true,
                perf_show_events: true,
                perf_show_memory: true,
                perf_show_lines: true,
                perf_show_uptime: true,
                perf_show_jitter: false,
                perf_show_frame_spikes: false,
                perf_show_event_lag: false,
                perf_show_memory_delta: true,
                color_mode: ColorMode::default(),
                timestamp_position: TimestampPosition::default(),
                command_echo: default_command_echo(),
                betrayer_active_color: default_betrayer_active_color(),
                open_dialog_blocklist: default_open_dialog_blocklist(),
                focus: FocusConfig::default(),
                terminal_title: String::new(),
            },
            highlights: HashMap::new(),     // Loaded from highlights.toml
            keybinds: HashMap::new(),       // Loaded from keybinds.toml
            hotbars: HotbarsConfig::default(), // Loaded from hotbars.toml
            app_keybinds: AppKeybinds::default(), // Loaded from [app] section of keybinds.toml
            colors: ColorConfig::default(), // Loaded from colors.toml
            sound: SoundConfig::default(),
            tts: TtsConfig::default(),
            target_list: TargetListConfig::default(),
            logging: LoggingConfig::default(),
            streams: StreamsConfig::default(), // Stream routing config
            highlight_settings: HighlightsConfig::default(), // Highlight system toggles
            quickbars: QuickbarsConfig::default(),
            web: WebConfig::default(), // Web server off by default
            map: MapConfig::default(),
            go2: Go2Config::default(),
            macros: MacrosConfig::default(), // Loaded from macros.toml
            macros_local: MacrosConfig::default(),
            event_patterns: HashMap::new(), // Empty by default - user adds via config
            layout_mappings: Vec::new(),    // Empty by default - user adds via config
            character: None,                // Set at runtime via load_with_options
            menu_keybinds: MenuKeybinds::default(),
            active_theme: default_theme_name(),
            active_skin: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped defaults/globals/config.toml must parse as a Config and
    /// agree with the code defaults for the values it states explicitly.
    /// These have drifted before (port 8001 vs 8000, a stale
    /// open_dialog_blocklist, and a [ui.focus] header that was commented
    /// out while its keys were not — silently mis-sectioning everything
    /// after it).
    #[test]
    fn shipped_default_config_matches_code_defaults() {
        let shipped: Config =
            toml::from_str(crate::config::DEFAULT_CONFIG).expect("shipped config.toml must parse");
        let code = Config::default();
        assert_eq!(shipped.connection.port, code.connection.port);
        assert_eq!(shipped.ui.buffer_size, code.ui.buffer_size);
        assert_eq!(shipped.ui.open_dialog_blocklist, code.ui.open_dialog_blocklist);
        // The [ui.focus] section must actually land in ui.focus (regression
        // guard for the commented-header bug).
        assert_eq!(shipped.ui.focus.types, code.ui.focus.types);
        // Perf keys stay in [ui], not swallowed by a preceding sub-table.
        assert_eq!(shipped.ui.perf_stats_width, code.ui.perf_stats_width);
    }

    /// Every serialized root field that Config::save writes into the
    /// profile config must survive merge_with — fields missing from the
    /// merge silently revert to global/defaults on the next load (this
    /// bit active_skin, web, map, and go2 for real).
    #[test]
    fn merge_with_keeps_profile_root_fields() {
        let mut character = Config::default();
        character.active_theme = "custom".to_string();
        character.active_skin = Some("parchment".to_string());
        character.web.enabled = true;
        character.go2.saved.insert("bank".to_string(), 1234);

        let mut merged = Config::default();
        merged.merge_with(character);

        assert_eq!(merged.active_theme, "custom");
        assert_eq!(merged.active_skin.as_deref(), Some("parchment"));
        assert!(merged.web.enabled);
        assert_eq!(merged.go2.saved.get("bank").copied(), Some(1234));
    }

    /// `.setskin none` writes a profile config without the key; the merge
    /// must land on None, not resurrect a stale global value.
    #[test]
    fn merge_with_clears_skin_when_profile_has_none() {
        let mut global = Config::default();
        global.active_skin = Some("stale".to_string());
        global.merge_with(Config::default());
        assert_eq!(global.active_skin, None);
    }

    #[test]
    fn copy_setting_routes_active_skin() {
        let mut src = Config::default();
        src.active_skin = Some("parchment".to_string());
        let mut dest = Config::default();
        Config::copy_setting(&mut dest, &src, "active_skin");
        assert_eq!(dest.active_skin.as_deref(), Some("parchment"));
    }
}
