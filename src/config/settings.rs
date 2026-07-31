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
    /// Render `:grin:`-style shortcodes in incoming text as emoji
    #[serde(default = "default_true")]
    pub emoji_shortcodes: bool,
    /// Draw emoji in color in the GUI (monochrome when off)
    #[serde(default = "default_true")]
    pub color_emoji: bool,
    /// LEGACY: migrated to `[sorter].enabled` at load (`SorterConfig`);
    /// kept readable one release so old files carry their value over.
    #[serde(default)]
    pub sorter_enabled: bool,
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
            emoji_shortcodes: true,
            color_emoji: true,
            sorter_enabled: false,
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
    pub rate: f32, // Speech rate (0.5 slow, 1.0 = engine normal, 3.0 = engine max)
    #[serde(default = "default_tts_volume")]
    pub volume: f32, // Volume (0.0 to 1.0)
    #[serde(default = "default_tts_speak_thoughts")]
    pub speak_thoughts: bool, // Automatically speak thought window
    #[serde(default = "default_tts_speak_speech", alias = "speak_whispers")]
    pub speak_speech: bool, // Automatically speak speech window (renamed from speak_whispers)
    #[serde(default = "default_tts_speak_main")]
    pub speak_main: bool, // Automatically speak main window
    /// Preferred voice by name (engine default when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    /// Speech-only gags: lines matching any regex are shown but not spoken.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gags: Vec<String>,
    /// Pronunciation substitutions applied before speaking, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub substitutions: Vec<TtsSubstitution>,
}

/// One pronunciation rewrite: regex pattern -> spoken replacement.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TtsSubstitution {
    pub pattern: String,
    pub replacement: String,
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
            voice: None,
            gags: Vec::new(),
            substitutions: Vec::new(),
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

/// Where an orphaned stream (no subscribed window) is routed.
///
/// Serialized as a plain string: `"discard"`, `"main"`, or
/// `"window:<name>"`. Anything else is rejected at deserialization with a
/// clear error, and the string form round-trips exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamRoute {
    /// Silently drop the stream's text.
    Discard,
    /// Route the stream to the main window.
    Main,
    /// Route the stream to the named window if one exists (its buffer
    /// receives the text even while hidden). Windows are never
    /// auto-created or auto-opened for a route; if no window by this
    /// name exists, the stream falls back to `StreamsConfig::fallback`.
    Window(String),
}

impl std::fmt::Display for StreamRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamRoute::Discard => f.write_str("discard"),
            StreamRoute::Main => f.write_str("main"),
            StreamRoute::Window(name) => write!(f, "window:{}", name),
        }
    }
}

impl std::str::FromStr for StreamRoute {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "discard" => Ok(StreamRoute::Discard),
            "main" => Ok(StreamRoute::Main),
            _ => match s.strip_prefix("window:") {
                Some("") => Err(format!(
                    "invalid stream route {:?}: window name is empty \
                     (expected \"window:<name>\")",
                    s
                )),
                Some(name) => Ok(StreamRoute::Window(name.to_string())),
                None => Err(format!(
                    "invalid stream route {:?} (expected \"discard\", \
                     \"main\", or \"window:<name>\")",
                    s
                )),
            },
        }
    }
}

impl Serialize for StreamRoute {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for StreamRoute {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// Configuration for text stream routing behavior.
/// Controls how orphaned streams (no widget subscriber) are handled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamsConfig {
    /// LEGACY: streams to silently discard if no widget subscribes.
    /// Superseded by `routes` — at config load every entry here becomes
    /// `routes.<id> = "discard"` and this list is cleared in memory (see
    /// `migrate_drop_list_to_routes`), so runtime code only consults
    /// `routes`. The field stays (and stays registered) so old config
    /// files keep loading; sparse saves age it out of user files.
    #[serde(default)]
    pub drop_unsubscribed: Vec<String>,

    /// Where to route orphaned streams that have no `routes` entry.
    /// Default: "main"
    #[serde(default = "default_streams_fallback")]
    pub fallback: String,

    /// Per-stream orphan policy: where a stream goes when no window
    /// subscribes to it. Values: "discard", "main", "window:<name>".
    /// Subscribed windows always win; streams absent from this map use
    /// `fallback`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub routes: std::collections::BTreeMap<String, StreamRoute>,

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
            routes: std::collections::BTreeMap::new(),
            room_in_main: default_room_in_main(),
        }
    }
}

impl StreamsConfig {
    /// Fold legacy `drop_unsubscribed` entries into `routes` (as
    /// `"discard"`) and clear the legacy list, so the rest of the app has
    /// one source of truth. Runs in memory at config load — never writes
    /// files; sparse saves age the old key out of user files on the next
    /// save. An existing route for a stream (any letter case) is never
    /// clobbered by its drop-list entry.
    pub fn migrate_drop_list_to_routes(&mut self) {
        for id in std::mem::take(&mut self.drop_unsubscribed) {
            let already_routed = self.routes.keys().any(|k| k.eq_ignore_ascii_case(&id));
            if !already_routed {
                self.routes.insert(id, StreamRoute::Discard);
            }
        }
    }
}

/// `[sorter]` — categorized container looks (`.sorter`, sorter.lic's
/// native cousin). The transform lives in `core/sorter.rs`; the editor is
/// `.sorter edit`. `ui.sorter_enabled` is the legacy home of `enabled`
/// and migrates here at load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SorterConfig {
    /// Master switch (`.sorter [on|off]`).
    #[serde(default)]
    pub enabled: bool,
    /// Show "(n)" duplicate counts and the "(total)" in category labels.
    #[serde(default = "default_true")]
    pub show_counts: bool,
    /// Render category labels monsterbold (sorter.lic style).
    #[serde(default = "default_true")]
    pub bold_labels: bool,
    /// Item order within a category: "last_word" (sorter.lic style),
    /// "alpha", or "none" (keep the look's order).
    #[serde(default = "default_sorter_item_sort")]
    pub item_sort: String,
    /// Explicit category display order; categories not listed follow in
    /// first-seen order after the listed ones.
    #[serde(default)]
    pub category_order: Vec<String>,
    /// Display renames, keyed by category name ("gem" -> "Gems").
    #[serde(default)]
    pub labels: std::collections::BTreeMap<String, String>,
    /// User rules, checked BEFORE the gameobj data pack; first matching
    /// rule wins. Items are never hidden — only re-categorized.
    #[serde(default)]
    pub rules: Vec<SorterRule>,
}

/// One user categorization rule. Both matchers empty = matches everything
/// (a catch-all the user can order last).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SorterRule {
    /// Case-insensitive substring of the item's display name; empty = any.
    #[serde(rename = "match", default)]
    pub name_match: String,
    /// Exact noun (case-insensitive); empty = any.
    #[serde(default)]
    pub noun: String,
    /// Target category. New names create new buckets.
    pub category: String,
}

fn default_sorter_item_sort() -> String {
    "last_word".to_string()
}

impl Default for SorterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            show_counts: true,
            bold_labels: true,
            item_sort: default_sorter_item_sort(),
            category_order: Vec::new(),
            labels: std::collections::BTreeMap::new(),
            rules: Vec::new(),
        }
    }
}

impl SorterRule {
    /// True when this rule matches the item. Empty matchers are wildcards.
    pub fn matches(&self, name: &str, noun: &str) -> bool {
        let name_ok = self.name_match.is_empty()
            || name.to_lowercase().contains(&self.name_match.to_lowercase());
        let noun_ok = self.noun.is_empty() || noun.eq_ignore_ascii_case(&self.noun);
        name_ok && noun_ok
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
    /// Phone story text size in px — a roaming pref: it rides the
    /// character profile so switching phones keeps the look. None =
    /// unset; the phone falls back to its own localStorage value.
    /// (0 on the settings wire means "unset" — see the registry entry.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_size: Option<u8>,
    /// Phone theme preset name (e.g. "dark", "black", "contrast",
    /// "light") — roaming pref like `story_size`. Free text because the
    /// client's theme set can grow without a server release. None =
    /// unset (phone's own choice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Phone stream-chip order (stream ids, leftmost first) — roaming
    /// pref like `story_size`. Empty = unset (phone's own order).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chip_order: Vec<String>,
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
            story_size: None,
            theme: None,
            chip_order: Vec::new(),
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
    /// Personal maze routes, keyed by maze name (see defaults/globals/
    /// mazes.toml). Captured automatically from the maze NPC's response
    /// ("Your route is: ...") — never hand-edited.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub pathcodes: std::collections::BTreeMap<String, Vec<String>>,
}

impl Default for Go2Config {
    fn default() -> Self {
        Self {
            saved: Default::default(),
            native_map_clicks: true,
            pathcodes: Default::default(),
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
    /// Cartography mode: sketch unmapped rooms as ghost overlays on the map.
    /// Off for everyday play — the mapdb is the truth on screen; unmapped
    /// interiors simply hold the map. Ghost *capture* always runs (the
    /// evidence feeds future mapdb submissions); this only gates rendering.
    #[serde(default)]
    pub mapping_mode: bool,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            lich_dir: None,
            mapdb_path: None,
            mapdb_repo: default_mapdb_repo(),
            mapping_mode: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- StreamRoute string form --------------------------------------

    #[test]
    fn stream_route_round_trips_exactly() {
        let cases = [
            (StreamRoute::Discard, "discard"),
            (StreamRoute::Main, "main"),
            (StreamRoute::Window("bounty".to_string()), "window:bounty"),
            // Window names keep their exact case and inner punctuation.
            (StreamRoute::Window("My Window".to_string()), "window:My Window"),
        ];
        for (route, text) in cases {
            assert_eq!(route.to_string(), text);
            assert_eq!(text.parse::<StreamRoute>().unwrap(), route);
            // serde round-trip (string repr on the wire)
            let json = serde_json::to_string(&route).unwrap();
            assert_eq!(json, format!("{:?}", text)); // JSON string literal
            let back: StreamRoute = serde_json::from_str(&json).unwrap();
            assert_eq!(back, route);
        }
    }

    #[test]
    fn stream_route_rejects_unknown_strings() {
        for bad in ["", "garbage", "Discard", "MAIN", "window:", "windows:foo", "drop"] {
            let err = bad.parse::<StreamRoute>().unwrap_err();
            assert!(
                err.contains("stream route"),
                "error for {:?} should mention stream route: {}",
                bad,
                err
            );
            assert!(
                serde_json::from_str::<StreamRoute>(&format!("{:?}", bad)).is_err(),
                "serde should reject {:?}",
                bad
            );
        }
    }

    #[test]
    fn stream_routes_deserialize_from_toml_table() {
        let cfg: StreamsConfig = toml::from_str(
            r#"
            fallback = "main"
            [routes]
            speech = "discard"
            ooc = "main"
            bounty = "window:bounty"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.routes["speech"], StreamRoute::Discard);
        assert_eq!(cfg.routes["ooc"], StreamRoute::Main);
        assert_eq!(cfg.routes["bounty"], StreamRoute::Window("bounty".to_string()));

        // And a bad value fails loudly, not silently.
        let err = toml::from_str::<StreamsConfig>(
            r#"
            [routes]
            speech = "yeet"
            "#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("stream route"), "{}", err);
    }

    // ---- drop_unsubscribed migration ----------------------------------

    #[test]
    fn migration_converts_drop_list_to_discard_routes() {
        let mut cfg = StreamsConfig::default();
        assert!(!cfg.drop_unsubscribed.is_empty(), "default drop list feeds migration");
        let dropped = cfg.drop_unsubscribed.clone();
        cfg.migrate_drop_list_to_routes();
        assert!(cfg.drop_unsubscribed.is_empty(), "legacy list cleared in memory");
        for id in dropped {
            assert_eq!(cfg.routes.get(&id), Some(&StreamRoute::Discard), "{}", id);
        }
    }

    #[test]
    fn migration_never_clobbers_existing_routes() {
        let mut cfg = StreamsConfig {
            drop_unsubscribed: vec!["speech".to_string(), "Bounty".to_string()],
            ..Default::default()
        };
        cfg.routes
            .insert("speech".to_string(), StreamRoute::Main);
        cfg.routes
            .insert("bounty".to_string(), StreamRoute::Window("bounty".to_string()));
        cfg.migrate_drop_list_to_routes();
        assert!(cfg.drop_unsubscribed.is_empty());
        // Existing route wins over the drop-list entry, case-insensitively.
        assert_eq!(cfg.routes["speech"], StreamRoute::Main);
        assert_eq!(cfg.routes["bounty"], StreamRoute::Window("bounty".to_string()));
        assert!(!cfg.routes.contains_key("Bounty"));
    }

    #[test]
    fn migration_is_idempotent_and_empty_routes_stay_unserialized() {
        let mut cfg = StreamsConfig::default();
        cfg.migrate_drop_list_to_routes();
        let routes = cfg.routes.clone();
        cfg.migrate_drop_list_to_routes();
        assert_eq!(cfg.routes, routes);

        // skip_serializing_if: an empty map writes no [streams.routes] key.
        let empty = StreamsConfig {
            routes: std::collections::BTreeMap::new(),
            ..StreamsConfig::default()
        };
        let toml_text = toml::to_string(&empty).unwrap();
        assert!(!toml_text.contains("routes"), "{}", toml_text);
    }
}
