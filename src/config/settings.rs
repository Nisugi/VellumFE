//! Settings structs for the `[section]` tables of config.toml.
//!
//! Each struct maps to one TOML section (connection, ui, sound, tts,
//! target_list, logging, streams, highlights toggles, focus), with its
//! serde default fns alongside.

use super::*;

// Default functions for HighlightsConfig
fn default_highlights_enabled() -> bool {
    true
}

/// Configuration for highlight system toggles.
/// Allows disabling specific highlight features without deleting patterns.
/// Note: System highlights (monsterbold, links, roomname) are NOT affected by these toggles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightsConfig {
    /// Enable sound triggers on pattern match
    #[serde(default = "default_highlights_enabled")]
    pub sounds_enabled: bool,
    /// Enable text replacement patterns
    #[serde(default = "default_highlights_enabled")]
    pub replace_enabled: bool,
    /// Enable redirect patterns (route lines to other windows)
    #[serde(default = "default_highlights_enabled")]
    pub redirect_enabled: bool,
    /// Enable color highlighting
    #[serde(default = "default_highlights_enabled")]
    pub coloring_enabled: bool,
}

impl Default for HighlightsConfig {
    fn default() -> Self {
        Self {
            sounds_enabled: true,
            replace_enabled: true,
            redirect_enabled: true,
            coloring_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Character name (used for Lich proxy profile and direct connect login)
    pub character: Option<String>,

    // --- Direct Connection (all optional) ---
    // Credentials can be stored here or passed via CLI. CLI arguments override these values.

    /// Account name for direct connection
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Password for direct connection (OPTIONAL, stored in PLAIN TEXT - use CLI for security)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Game instance: GS4: "prime", "platinum", "shattered", "test"; DR: "dr", "drplatinum", "drfallen", "drtest"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default = "default_border_style")]
    pub border_style: String, // Default border style: "single", "double", "rounded", "thick", "none"
    #[serde(default = "default_countdown_icon")]
    pub countdown_icon: String, // Unicode character for countdown blocks (e.g., "\u{f0c8}")
    // Text selection settings
    #[serde(default = "default_selection_enabled")]
    pub selection_enabled: bool,
    #[serde(default = "default_selection_respect_window_boundaries")]
    pub selection_respect_window_boundaries: bool,
    /// Automatically copy mouse selection to clipboard on mouse-up
    #[serde(default = "default_selection_auto_copy")]
    pub selection_auto_copy: bool,
    // Drag and drop settings
    #[serde(default = "default_drag_modifier_key")]
    pub drag_modifier_key: String, // Modifier key required for drag and drop (e.g., "ctrl", "alt", "shift")
    // Command history settings
    #[serde(default = "default_min_command_length")]
    pub min_command_length: usize, // Minimum command length to save to history (commands shorter than this are not saved)
    // Command echo settings
    #[serde(default = "default_command_echo")]
    pub command_echo: bool, // Echo sent commands into main window
    // Performance stats settings
    #[serde(default = "default_performance_stats_enabled")]
    pub performance_stats_enabled: bool, // Global toggle for performance overlay
    #[serde(default = "default_perf_stats_x")]
    pub perf_stats_x: u16,
    #[serde(default = "default_perf_stats_y")]
    pub perf_stats_y: u16,
    #[serde(default = "default_perf_stats_width")]
    pub perf_stats_width: u16,
    #[serde(default = "default_perf_stats_height")]
    pub perf_stats_height: u16,
    // Performance overlay metric toggles
    #[serde(default = "default_true")]
    pub perf_show_fps: bool,
    #[serde(default)]
    pub perf_show_frame_times: bool,
    #[serde(default = "default_true")]
    pub perf_show_render_times: bool,
    #[serde(default = "default_true")]
    pub perf_show_ui_times: bool,
    #[serde(default = "default_true")]
    pub perf_show_wrap_times: bool,
    #[serde(default = "default_true")]
    pub perf_show_net: bool,
    #[serde(default = "default_true")]
    pub perf_show_parse: bool,
    #[serde(default = "default_true")]
    pub perf_show_events: bool,
    #[serde(default = "default_true")]
    pub perf_show_memory: bool,
    #[serde(default = "default_true")]
    pub perf_show_lines: bool,
    #[serde(default = "default_true")]
    pub perf_show_uptime: bool,
    #[serde(default)]
    pub perf_show_jitter: bool,
    #[serde(default)]
    pub perf_show_frame_spikes: bool,
    #[serde(default)]
    pub perf_show_event_lag: bool,
    #[serde(default = "default_true")]
    pub perf_show_memory_delta: bool,
    // Color rendering mode
    #[serde(default)]
    pub color_mode: ColorMode, // "direct" (true color) or "slot" (256-color palette)
    // Timestamp position (start or end of line)
    #[serde(default)]
    pub timestamp_position: TimestampPosition, // "start" or "end" (default: end)
    #[serde(default = "default_betrayer_active_color")]
    pub betrayer_active_color: Option<String>,
    #[serde(default = "default_open_dialog_blocklist")]
    pub open_dialog_blocklist: Vec<String>,
    #[serde(default)]
    pub focus: FocusConfig, // Tab focus behavior and order
    /// Terminal title template with variables: {character}, {room}, {health}, {mana}, {stamina}, {unread}
    /// Empty string = don't modify terminal title
    #[serde(default)]
    pub terminal_title: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            buffer_size: default_buffer_size(),
            layout: LayoutConfig::default(),
            border_style: default_border_style(),
            countdown_icon: default_countdown_icon(),
            selection_enabled: default_selection_enabled(),
            selection_respect_window_boundaries: default_selection_respect_window_boundaries(),
            selection_auto_copy: default_selection_auto_copy(),
            drag_modifier_key: default_drag_modifier_key(),
            min_command_length: default_min_command_length(),
            command_echo: default_command_echo(),
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
            betrayer_active_color: default_betrayer_active_color(),
            open_dialog_blocklist: default_open_dialog_blocklist(),
            focus: FocusConfig::default(),
            terminal_title: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusConfig {
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default = "default_focus_types")]
    pub types: Vec<String>,
    #[serde(default = "default_focus_exclude")]
    pub exclude: Vec<String>,
}

impl Default for FocusConfig {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            types: default_focus_types(),
            exclude: default_focus_exclude(),
        }
    }
}

/// Sound configuration for audio playback.
///
/// When `enabled = false`, the audio system is not initialized at all.
/// This avoids the ~10 second timeout on systems without audio hardware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundConfig {
    #[serde(default = "default_sound_enabled")]
    pub enabled: bool, // false = skip audio init entirely
    #[serde(default = "default_sound_volume")]
    pub volume: f32, // Master volume (0.0 to 1.0)
    #[serde(default = "default_sound_cooldown")]
    pub cooldown_ms: u64, // Cooldown between same sound plays (milliseconds)
    #[serde(default = "default_startup_music")]
    pub startup_music: bool, // Play music on startup
    #[serde(default = "default_startup_music_delay")]
    pub startup_music_delay_ms: u64, // Delay before startup music (0 = immediate)
}

fn default_sound_enabled() -> bool {
    true
}

fn default_sound_volume() -> f32 {
    0.7
}

fn default_sound_cooldown() -> u64 {
    500 // 500ms default cooldown
}

fn default_startup_music() -> bool {
    true
}

fn default_startup_music_delay() -> u64 {
    0 // 0 = immediate
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: default_sound_enabled(),
            volume: default_sound_volume(),
            cooldown_ms: default_sound_cooldown(),
            startup_music: default_startup_music(),
            startup_music_delay_ms: default_startup_music_delay(),
        }
    }
}

/// Text-to-Speech Configuration
///
/// Controls accessibility features for visually impaired users.
/// When disabled (default), has zero performance impact.
/// TTS operates independently of the sound system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    #[serde(default = "default_tts_enabled")]
    pub enabled: bool,
    #[serde(default = "default_tts_rate")]
    pub rate: f32, // Speech rate (0.5 to 2.0, 1.0 = normal)
    #[serde(default = "default_tts_volume")]
    pub volume: f32, // Volume (0.0 to 1.0)
    #[serde(default = "default_tts_speak_thoughts")]
    pub speak_thoughts: bool, // Automatically speak thought window
    #[serde(default = "default_tts_speak_speech", alias = "speak_whispers")]
    pub speak_speech: bool, // Automatically speak speech window (renamed from speak_whispers)
    #[serde(default = "default_tts_speak_main")]
    pub speak_main: bool, // Automatically speak main window
}

fn default_tts_enabled() -> bool {
    false // Disabled by default (opt-in)
}

fn default_tts_rate() -> f32 {
    1.0 // Normal speech rate
}

fn default_tts_volume() -> f32 {
    1.0 // Full volume
}

fn default_tts_speak_thoughts() -> bool {
    true // Thoughts are high priority for screen reader users
}

fn default_tts_speak_speech() -> bool {
    true // Speech window is important for communications
}

fn default_tts_speak_main() -> bool {
    false // Main window can be overwhelming, off by default
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: default_tts_enabled(),
            rate: default_tts_rate(),
            volume: default_tts_volume(),
            speak_thoughts: default_tts_speak_thoughts(),
            speak_speech: default_tts_speak_speech(),
            speak_main: default_tts_speak_main(),
        }
    }
}

/// Target list widget configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetListConfig {
    /// Status display position: "start" or "end"
    #[serde(default = "default_target_status_position")]
    pub status_position: String,
    /// Truncation mode: "full" or "noun"
    #[serde(default = "default_target_truncation_mode")]
    pub truncation_mode: String,
    /// Map of full status names to 3-character abbreviations
    #[serde(default = "default_status_abbrev")]
    pub status_abbrev: HashMap<String, String>,
    /// Nouns to exclude from room objs parsing (e.g., "arm", "coal")
    #[serde(default = "default_excluded_nouns")]
    pub excluded_nouns: Vec<String>,
    /// Text color for AscensionBoss/MiniBoss creatures (from <crtrStatus>)
    #[serde(default = "default_boss_color")]
    pub boss_color: Option<String>,
    /// Text color for "challenging" creatures (from <crtrStatus>)
    #[serde(default = "default_challenging_color")]
    pub challenging_color: Option<String>,
}

fn default_boss_color() -> Option<String> {
    Some("#ff5555".to_string())
}

fn default_challenging_color() -> Option<String> {
    Some("#ffaa55".to_string())
}

fn default_target_status_position() -> String {
    "end".to_string()
}

fn default_target_truncation_mode() -> String {
    "noun".to_string()
}

fn default_status_abbrev() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("stunned".to_string(), "stu".to_string());
    map.insert("frozen".to_string(), "frz".to_string());
    map.insert("dead".to_string(), "ded".to_string());
    map.insert("sitting".to_string(), "sit".to_string());
    map.insert("kneeling".to_string(), "kne".to_string());
    map.insert("prone".to_string(), "prn".to_string());
    map.insert("webbed".to_string(), "web".to_string());
    map.insert("immobilized".to_string(), "imm".to_string());
    map.insert("bleeding".to_string(), "ble".to_string());
    map.insert("standing".to_string(), "std".to_string());
    map.insert("sleeping".to_string(), "slp".to_string());
    map.insert("poisoned".to_string(), "poi".to_string());
    map.insert("diseased".to_string(), "dis".to_string());
    map.insert("bound".to_string(), "bnd".to_string());
    map.insert("calmed".to_string(), "cal".to_string());
    map
}

fn default_excluded_nouns() -> Vec<String> {
    vec!["arm".to_string(), "coal".to_string()]
}

impl Default for TargetListConfig {
    fn default() -> Self {
        Self {
            status_position: default_target_status_position(),
            truncation_mode: default_target_truncation_mode(),
            status_abbrev: default_status_abbrev(),
            excluded_nouns: default_excluded_nouns(),
            boss_color: default_boss_color(),
            challenging_color: default_challenging_color(),
        }
    }
}

/// Raw XML logging configuration for network input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_logging_enabled")]
    pub enabled: bool,
    /// Directory for log files (relative to profile dir if not absolute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(default = "default_logging_buffer_lines")]
    pub buffer_lines: usize,
    #[serde(default = "default_logging_flush_interval_ms")]
    pub flush_interval_ms: u64,
    #[serde(default = "default_logging_max_lines_per_file")]
    pub max_lines_per_file: usize,
    #[serde(default = "default_logging_timestamps")]
    pub timestamps: bool,
}

fn default_logging_enabled() -> bool {
    false
}

fn default_logging_buffer_lines() -> usize {
    200
}

fn default_logging_flush_interval_ms() -> u64 {
    2000
}

fn default_logging_max_lines_per_file() -> usize {
    30000
}

fn default_logging_timestamps() -> bool {
    true
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: default_logging_enabled(),
            dir: None,
            buffer_lines: default_logging_buffer_lines(),
            flush_interval_ms: default_logging_flush_interval_ms(),
            max_lines_per_file: default_logging_max_lines_per_file(),
            timestamps: default_logging_timestamps(),
        }
    }
}

impl LoggingConfig {
    pub fn resolve_dir(&self, character: Option<&str>) -> Result<PathBuf> {
        let base = Config::profile_dir(character)?;
        if let Some(dir) = &self.dir {
            let path = PathBuf::from(dir);
            if path.is_absolute() {
                Ok(path)
            } else {
                Ok(base.join(path))
            }
        } else {
            Ok(base.join("logs"))
        }
    }
}

/// Configuration for text stream routing behavior.
/// Controls how orphaned streams (no widget subscriber) are handled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamsConfig {
    /// Streams to silently discard if no widget subscribes to them.
    /// Example: ["speech", "bounty", "whisper"]
    #[serde(default)]
    pub drop_unsubscribed: Vec<String>,

    /// Where to route orphaned streams that aren't in the drop list.
    /// Default: "main"
    #[serde(default = "default_streams_fallback")]
    pub fallback: String,

    /// When true (default), <streamWindow id='room'> does NOT change current_stream.
    /// Room text will flow to main window (room window uses components, not text).
    /// Set to false for legacy behavior where streamWindow pushes the stream.
    /// DragonRealms-specific - GemStone IV doesn't use streamWindow room.
    #[serde(default = "default_room_in_main")]
    pub room_in_main: bool,
}

fn default_streams_fallback() -> String {
    "main".to_string()
}

fn default_room_in_main() -> bool {
    true
}

impl Default for StreamsConfig {
    fn default() -> Self {
        Self {
            // Match defaults/config.toml - drop streams that duplicate main content
            drop_unsubscribed: vec![
                "targetcount".to_string(),
                "playercount".to_string(),
                "targetlist".to_string(),
                "playerlist".to_string(),
                "speech".to_string(),
                "whisper".to_string(),
                "talk".to_string(),
                "conversation".to_string(),
            ],
            fallback: default_streams_fallback(),
            room_in_main: default_room_in_main(),
        }
    }
}

/// Configuration for the embedded web server (mobile web frontend).
///
/// Off by default; when disabled the web sidecar costs nothing (no server
/// task, no remote scrollback buffer). See docs/mobile-web-frontend-plan.md.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Enable the embedded web server sidecar.
    #[serde(default)]
    pub enabled: bool,
    /// Port to serve HTTP + WebSocket on. Unpinned instances treat this
    /// as a base port and walk upward when it's taken (so several
    /// characters can launch without config); see `pinned`.
    #[serde(default = "default_web_port")]
    pub port: u16,
    /// Bind address. 127.0.0.1 (default) = this machine only;
    /// set to "0.0.0.0" consciously to allow phones on the LAN.
    #[serde(default = "default_web_bind")]
    pub bind: String,
    /// Pin this instance to exactly `port`: bind it or fail loudly (web
    /// disabled for the session), never silently take a neighboring
    /// port. Pinning is what makes a per-character /play bookmark
    /// stable; set it in the character's profile config.
    #[serde(default)]
    pub pinned: bool,
}

fn default_web_port() -> u16 {
    8040
}

fn default_web_bind() -> String {
    "127.0.0.1".to_string()
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_web_port(),
            bind: default_web_bind(),
            pinned: false,
        }
    }
}

/// Native travel (`.go2`) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Go2Config {
    /// Saved travel targets: name → mapdb room id (`.go2 save <name>`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub saved: std::collections::BTreeMap<String, u32>,
    /// Mini map / explorer clicks travel natively instead of sending `;go2`
    /// to Lich. Native works everywhere (mobile!); Lich's go2 knows silvers,
    /// day passes, and other special travel that native v1 does not.
    #[serde(default = "default_true")]
    pub native_map_clicks: bool,
}

impl Default for Go2Config {
    fn default() -> Self {
        Self {
            saved: Default::default(),
            native_map_clicks: true,
        }
    }
}

/// Testing-phase default for `MapConfig::mapdb_repo`; flip to
/// `elanthia-online/mapdb` when the Cartographer pipeline launches upstream.
pub const DEFAULT_MAPDB_REPO: &str = "Nisugi/mapdb";

fn default_mapdb_repo() -> String {
    DEFAULT_MAPDB_REPO.to_string()
}

/// Map system configuration (mini map widget + map explorer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapConfig {
    /// Lich install directory (the folder containing `data/`). The newest
    /// `data/<GAME>/map-<timestamp>.json` build for the connected game is
    /// used. Edited from the GUI settings editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lich_dir: Option<String>,
    /// Explicit mapdb JSON file; overrides `lich_dir` discovery when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mapdb_path: Option<String>,
    /// GitHub repository (`owner/repo`) whose releases carry a `mapdb.json`
    /// asset; the Download button in Settings > Map pulls from here.
    /// Downloaded data outranks `lich_dir`. Empty disables downloads.
    #[serde(default = "default_mapdb_repo")]
    pub mapdb_repo: String,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            lich_dir: None,
            mapdb_path: None,
            mapdb_repo: default_mapdb_repo(),
        }
    }
}
