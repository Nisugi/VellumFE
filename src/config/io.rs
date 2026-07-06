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

        // Extract config.toml to global directory (shared defaults for all characters)
        // Character-specific overrides can still be added to profile/config.toml
        let config_path = Self::common_config_path()?;
        if !config_path.exists() {
            fs::write(&config_path, DEFAULT_CONFIG).context("Failed to write config.toml")?;
            tracing::info!("Extracted config.toml to {:?}", config_path);
        }

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

        Ok(())
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

        // Streams config: character overrides global
        self.streams = character_config.streams;

        // Highlight settings: character overrides global
        self.highlight_settings = character_config.highlight_settings;

        // Quickbars: character overrides global
        self.quickbars = character_config.quickbars;
    }

    pub fn load_with_options(character: Option<&str>, port_override: Option<u16>) -> Result<Self> {
        // Extract defaults on first run (idempotent - only creates missing files)
        Self::extract_defaults(character)?;

        // Load global config first (defaults for all characters)
        let mut config = Self::load_common_config()?;

        // Load character-specific config and merge (character overrides global)
        if let Some(char_config) = Self::load_character_config_only(character)? {
            config.merge_with(char_config);
        }
        // If no character config exists, we use global config with default connection

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

    pub fn save(&self, character: Option<&str>) -> Result<()> {
        // Use provided character name, or fall back to stored character name
        let char_name = character.or(self.character.as_deref());
        let config_path = Self::config_path(char_name)?;

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Save main config (without highlights, keybinds, colors, color_palette - those are skipped)
        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&config_path, contents).context("Failed to write config file")?;

        // Save to separate files
        self.colors.save(char_name)?;
        self.save_highlights(char_name)?;
        self.save_keybinds(char_name)?;

        Ok(())
    }

    /// Save config to global config.toml
    pub fn save_common(&self) -> Result<()> {
        let config_path = Self::common_config_path()?;

        // Ensure global directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create global directory: {:?}", parent))?;
        }

        // Save main config
        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&config_path, contents).context("Failed to write global config file")?;
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
        // Connection settings are ALWAYS character-specific
        let actual_is_global = if key.starts_with("connection.") {
            false
        } else {
            is_global
        };

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
        // Load current character config (or create new if doesn't exist)
        let mut char_config = Self::load_character_config_only(character)?
            .unwrap_or_else(Self::default);

        // Update the specific setting
        Self::copy_setting(&mut char_config, self, key);

        // Save to character config path
        let config_path = Self::config_path(character)?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(&char_config).context("Failed to serialize config")?;
        fs::write(&config_path, contents).context("Failed to write character config file")?;
        tracing::info!("Saved setting '{}' to character config: {:?}", key, config_path);
        Ok(())
    }

    /// Copy a specific setting from source to destination config
    fn copy_setting(dest: &mut Config, src: &Config, key: &str) {
        match key {
            // Connection settings
            "connection.host" => dest.connection.host = src.connection.host.clone(),
            "connection.port" => dest.connection.port = src.connection.port,
            "connection.character" => dest.connection.character = src.connection.character.clone(),
            "connection.account" => dest.connection.account = src.connection.account.clone(),
            "connection.password" => dest.connection.password = src.connection.password.clone(),
            "connection.game" => dest.connection.game = src.connection.game.clone(),

            // UI settings
            "ui.buffer_size" => dest.ui.buffer_size = src.ui.buffer_size,
            "ui.border_style" => dest.ui.border_style = src.ui.border_style.clone(),
            "ui.countdown_icon" => dest.ui.countdown_icon = src.ui.countdown_icon.clone(),
            "ui.selection_enabled" => dest.ui.selection_enabled = src.ui.selection_enabled,
            "ui.selection_respect_window_boundaries" => {
                dest.ui.selection_respect_window_boundaries = src.ui.selection_respect_window_boundaries
            }
            "ui.selection_auto_copy" => dest.ui.selection_auto_copy = src.ui.selection_auto_copy,
            "ui.drag_modifier_key" => dest.ui.drag_modifier_key = src.ui.drag_modifier_key.clone(),
            "ui.min_command_length" => dest.ui.min_command_length = src.ui.min_command_length,

            // Sound settings
            "sound.enabled" => dest.sound.enabled = src.sound.enabled,
            "sound.volume" => dest.sound.volume = src.sound.volume,
            "sound.cooldown_ms" => dest.sound.cooldown_ms = src.sound.cooldown_ms,

            // Theme settings
            "active_theme" => dest.active_theme = src.active_theme.clone(),

            _ => {
                tracing::warn!("Unknown setting key for copy: {}", key);
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
            macros: MacrosConfig::default(), // Loaded from macros.toml
            macros_local: MacrosConfig::default(),
            event_patterns: HashMap::new(), // Empty by default - user adds via config
            layout_mappings: Vec::new(),    // Empty by default - user adds via config
            character: None,                // Set at runtime via load_with_options
            menu_keybinds: MenuKeybinds::default(),
            active_theme: default_theme_name(),
        }
    }
}
