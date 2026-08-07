//! The settings registry: the single source of truth for every scalar
//! setting in `Config`.
//!
//! Each leaf setting is declared exactly once as a [`SettingDef`] — key,
//! label, category, help text, kind, scope, and typed get/set accessors.
//! Everything else derives from this table:
//!
//! - `Config::copy_setting` (scoped single-key saves) looks keys up here,
//!   so every registered setting is saveable to global or profile.
//! - The enforcement tests at the bottom walk the serialized shape of
//!   `Config` and fail the build when a field is added without a registry
//!   entry (or an explicit exemption), and when `merge_with` drops any
//!   registered setting — the restart-amnesia bug class dies here.
//! - Future phases render the TUI/GUI settings editors and the phone's
//!   settings sheet from this same table.
//!
//! Structured data (maps, pattern lists, window layouts) is exempt: those
//! have dedicated editors and their own persistence semantics. The exempt
//! list is explicit and tested, so nothing is exempt by accident.

use super::{ColorMode, Config, TimestampPosition};
use std::sync::LazyLock;

/// A typed setting value crossing the registry boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    List(Vec<String>),
}

impl SettingValue {
    pub fn as_bool(&self) -> Result<bool, String> {
        match self {
            SettingValue::Bool(v) => Ok(*v),
            other => Err(format!("expected a boolean, got {:?}", other)),
        }
    }

    pub fn as_int(&self) -> Result<i64, String> {
        match self {
            SettingValue::Int(v) => Ok(*v),
            other => Err(format!("expected an integer, got {:?}", other)),
        }
    }

    pub fn as_float(&self) -> Result<f64, String> {
        match self {
            SettingValue::Float(v) => Ok(*v),
            SettingValue::Int(v) => Ok(*v as f64),
            other => Err(format!("expected a number, got {:?}", other)),
        }
    }

    pub fn as_text(&self) -> Result<String, String> {
        match self {
            SettingValue::Text(v) => Ok(v.clone()),
            other => Err(format!("expected text, got {:?}", other)),
        }
    }

    pub fn as_list(&self) -> Result<Vec<String>, String> {
        match self {
            SettingValue::List(v) => Ok(v.clone()),
            other => Err(format!("expected a list, got {:?}", other)),
        }
    }
}

/// What kind of value a setting holds; editors pick widgets from this.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingKind {
    Bool,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    /// Free text. `Text` settings are never empty-as-None.
    Text,
    /// Optional free text; empty input clears the value to None.
    OptionalText,
    /// One of a fixed set of string options.
    Enum { options: &'static [&'static str] },
    /// A list of free-text entries.
    List,
}

/// Where a setting may be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingScope {
    /// Saveable to global/config.toml or the character profile.
    GlobalOrCharacter,
    /// Only meaningful per character (connection identity, pinned ports).
    CharacterOnly,
}

/// Which frontends a setting applies to. Settings editors hide entries
/// that are out of scope for their frontend, so a toggle can never sit in
/// an editor where it does nothing (the perf-monitor drift bug class).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendScope {
    All,
    Tui,
    Gui,
}

impl FrontendScope {
    pub fn includes_tui(self) -> bool {
        matches!(self, FrontendScope::All | FrontendScope::Tui)
    }

    pub fn includes_gui(self) -> bool {
        matches!(self, FrontendScope::All | FrontendScope::Gui)
    }
}

/// One registered setting.
pub struct SettingDef {
    /// Dotted serde path within config.toml (e.g. "ui.buffer_size").
    pub key: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub kind: SettingKind,
    pub scope: SettingScope,
    /// Never shown back to the user in clear text by generated UIs.
    pub sensitive: bool,
    /// Which frontends this setting is meaningful in.
    pub frontend: FrontendScope,
    pub get: fn(&Config) -> SettingValue,
    pub set: fn(&mut Config, &SettingValue) -> Result<(), String>,
}

/// Serialized `Config` subtrees that are deliberately NOT in the registry:
/// structured data with dedicated editors or capture flows. Each entry is a
/// key-path prefix. The enforcement test rejects any config leaf that is
/// neither registered nor under one of these.
pub const EXEMPT_PREFIXES: &[&str] = &[
    "event_patterns",             // pattern map; highlights editor territory
    "menu_keybinds",              // 26-field struct; .menukeybinds editor (TUI menu_keybind_editor + GUI editors/menu_keybinds)
    "quickbars",                  // quickbar definitions (config template docs)
    "ui.layout",                  // legacy empty section
    "target_list.status_abbrev",  // Map (no registry Kind); bespoke editors in both frontends
    "go2.saved",                  // captured travel targets (.go2 save)
    "go2.pathcodes",              // captured maze routes, never hand-edited
    "tts.substitutions",          // pattern/replacement pairs; Accessibility panel
    "streams.routes",             // per-stream route map; dedicated Streams panel editor later
    "ui.sorter_enabled",          // legacy home of sorter.enabled; migrated at load
    "sorter.category_order",      // ordered list; `.sorter edit` editor
    "sorter.labels",              // rename map; `.sorter edit` editor
    "sorter.rules",               // structured rules; `.sorter edit` editor
];

macro_rules! bool_entry {
    ([$front:ident] $key:literal, $label:literal, $cat:literal, $desc:literal, $($f:tt)+) => {
        SettingDef {
            key: $key, label: $label, category: $cat, description: $desc,
            kind: SettingKind::Bool, scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::$front,
            get: |c| SettingValue::Bool(c.$($f)+),
            set: |c, v| { c.$($f)+ = v.as_bool()?; Ok(()) },
        }
    };
    ($key:literal, $label:literal, $cat:literal, $desc:literal, $($f:tt)+) => {
        bool_entry!([All] $key, $label, $cat, $desc, $($f)+)
    };
}

macro_rules! int_entry {
    ([$front:ident] $key:literal, $label:literal, $cat:literal, $desc:literal, $min:literal..=$max:literal, $ty:ty, $($f:tt)+) => {
        SettingDef {
            key: $key, label: $label, category: $cat, description: $desc,
            kind: SettingKind::Int { min: $min, max: $max },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::$front,
            get: |c| SettingValue::Int(c.$($f)+ as i64),
            set: |c, v| {
                let raw = v.as_int()?;
                if raw < $min || raw > $max {
                    return Err(format!("{} must be between {} and {}", $key, $min, $max));
                }
                c.$($f)+ = raw as $ty;
                Ok(())
            },
        }
    };
    ($key:literal, $label:literal, $cat:literal, $desc:literal, $min:literal..=$max:literal, $ty:ty, $($f:tt)+) => {
        int_entry!([All] $key, $label, $cat, $desc, $min..=$max, $ty, $($f)+)
    };
}

macro_rules! float_entry {
    ($key:literal, $label:literal, $cat:literal, $desc:literal, $min:literal..=$max:literal, $($f:tt)+) => {
        SettingDef {
            key: $key, label: $label, category: $cat, description: $desc,
            kind: SettingKind::Float { min: $min, max: $max },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Float(c.$($f)+ as f64),
            set: |c, v| {
                let raw = v.as_float()?;
                if raw < $min || raw > $max {
                    return Err(format!("{} must be between {} and {}", $key, $min, $max));
                }
                c.$($f)+ = raw as f32;
                Ok(())
            },
        }
    };
}

macro_rules! text_entry {
    ($key:literal, $label:literal, $cat:literal, $desc:literal, $($f:tt)+) => {
        SettingDef {
            key: $key, label: $label, category: $cat, description: $desc,
            kind: SettingKind::Text, scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.$($f)+.clone()),
            set: |c, v| { c.$($f)+ = v.as_text()?; Ok(()) },
        }
    };
}

macro_rules! opt_text_entry {
    ($key:literal, $label:literal, $cat:literal, $desc:literal, $($f:tt)+) => {
        SettingDef {
            key: $key, label: $label, category: $cat, description: $desc,
            kind: SettingKind::OptionalText, scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.$($f)+.clone().unwrap_or_default()),
            set: |c, v| {
                let text = v.as_text()?;
                c.$($f)+ = if text.trim().is_empty() { None } else { Some(text) };
                Ok(())
            },
        }
    };
}

macro_rules! enum_entry {
    ($key:literal, $label:literal, $cat:literal, $desc:literal, $options:expr, $($f:tt)+) => {
        SettingDef {
            key: $key, label: $label, category: $cat, description: $desc,
            kind: SettingKind::Enum { options: $options },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.$($f)+.clone()),
            set: |c, v| {
                let text = v.as_text()?;
                let options: &[&str] = $options;
                if !options.contains(&text.as_str()) {
                    return Err(format!("{} must be one of {:?}", $key, options));
                }
                c.$($f)+ = text;
                Ok(())
            },
        }
    };
}

macro_rules! list_entry {
    ($key:literal, $label:literal, $cat:literal, $desc:literal, $($f:tt)+) => {
        SettingDef {
            key: $key, label: $label, category: $cat, description: $desc,
            kind: SettingKind::List, scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::List(c.$($f)+.clone()),
            set: |c, v| { c.$($f)+ = v.as_list()?; Ok(()) },
        }
    };
}

static REGISTRY: LazyLock<Vec<SettingDef>> = LazyLock::new(|| {
    let mut defs = vec![
        // ---- Connection (always character-scoped) --------------------
        SettingDef {
            key: "connection.host",
            label: "Host",
            category: "Connection",
            description: "Lich proxy host to connect to",
            kind: SettingKind::Text,
            scope: SettingScope::CharacterOnly,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.connection.host.clone()),
            set: |c, v| { c.connection.host = v.as_text()?; Ok(()) },
        },
        SettingDef {
            key: "connection.port",
            label: "Port",
            category: "Connection",
            description: "Lich proxy port",
            kind: SettingKind::Int { min: 1, max: 65535 },
            scope: SettingScope::CharacterOnly,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Int(c.connection.port as i64),
            set: |c, v| {
                let raw = v.as_int()?;
                if !(1..=65535).contains(&raw) {
                    return Err("connection.port must be between 1 and 65535".to_string());
                }
                c.connection.port = raw as u16;
                Ok(())
            },
        },
        SettingDef {
            key: "connection.character",
            label: "Character",
            category: "Connection",
            description: "Character name for Lich profile and direct login",
            kind: SettingKind::OptionalText,
            scope: SettingScope::CharacterOnly,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.connection.character.clone().unwrap_or_default()),
            set: |c, v| {
                let text = v.as_text()?;
                c.connection.character =
                    if text.trim().is_empty() { None } else { Some(text) };
                Ok(())
            },
        },
        SettingDef {
            key: "connection.account",
            label: "Account",
            category: "Connection",
            description: "Account name for direct eAccess connection",
            kind: SettingKind::OptionalText,
            scope: SettingScope::CharacterOnly,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.connection.account.clone().unwrap_or_default()),
            set: |c, v| {
                let text = v.as_text()?;
                c.connection.account =
                    if text.trim().is_empty() { None } else { Some(text) };
                Ok(())
            },
        },
        SettingDef {
            key: "connection.password",
            label: "Password",
            category: "Connection",
            description: "Direct-connection password (stored in plain text; prefer the CLI prompt)",
            kind: SettingKind::OptionalText,
            scope: SettingScope::CharacterOnly,
            sensitive: true,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.connection.password.clone().unwrap_or_default()),
            set: |c, v| {
                let text = v.as_text()?;
                c.connection.password =
                    if text.trim().is_empty() { None } else { Some(text) };
                Ok(())
            },
        },
        SettingDef {
            key: "connection.game",
            label: "Game",
            category: "Connection",
            description: "Game instance for direct connection (prime, platinum, shattered, test, dr, ...)",
            kind: SettingKind::OptionalText,
            scope: SettingScope::CharacterOnly,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.connection.game.clone().unwrap_or_default()),
            set: |c, v| {
                let text = v.as_text()?;
                c.connection.game =
                    if text.trim().is_empty() { None } else { Some(text) };
                Ok(())
            },
        },
        // ---- UI ------------------------------------------------------
        int_entry!("ui.buffer_size", "Buffer Size", "UI",
            "Default lines kept in memory per text window", 100..=1000000, usize, ui.buffer_size),
        enum_entry!("ui.border_style", "Border Style", "UI",
            "Default window border style",
            &["single", "double", "rounded", "thick", "none"], ui.border_style),
        text_entry!("ui.countdown_icon", "Countdown Icon", "UI",
            "Glyph used for RT/CT timer blocks", ui.countdown_icon),
        int_entry!("ui.min_command_length", "Min Command Length", "UI",
            "Commands shorter than this are not saved to history", 0..=100, usize, ui.min_command_length),
        bool_entry!("ui.command_echo", "Command Echo", "UI",
            "Echo sent commands into the main window", ui.command_echo),
        bool_entry!("ui.keep_open_on_quit", "Keep Open On Quit", "UI",
            "After .quit, detach but keep the window open (.quit again or .exit closes it)", ui.keep_open_on_quit),
        bool_entry!("ui.history_suggestions", "History Suggestions", "UI",
            "Ghost the newest matching history entry in the command input; Tab accepts it", ui.history_suggestions),
        bool_entry!("ui.emoji_shortcodes", "Emoji Shortcodes", "UI",
            "Render :grin:-style shortcodes in incoming text as emoji", ui.emoji_shortcodes),
        bool_entry!("ui.color_emoji", "Color Emoji", "UI",
            "Draw emoji in color in the GUI (monochrome when off)", ui.color_emoji),
        float_entry!("ui.custom_emoji_size", "Custom Emoji Size", "UI",
            "Custom emoji image size as a multiple of the text row height (1.0 = line height)",
            0.5..=3.0, ui.custom_emoji_size),
        float_entry!("ui.custom_emoji_spacing", "Custom Emoji Spacing", "UI",
            "Breathing room around a custom emoji, as a fraction of row height added to its width",
            0.0..=1.5, ui.custom_emoji_spacing),
        bool_entry!("sorter.enabled", "Sort Container Looks", "UI",
            "Categorize 'look in container' output by item type (.sorter; rules/order/renames in '.sorter edit')",
            sorter.enabled),
        bool_entry!("sorter.show_counts", "Sorter Counts", "UI",
            "Show duplicate counts and category totals in sorted container looks", sorter.show_counts),
        bool_entry!("sorter.bold_labels", "Sorter Bold Labels", "UI",
            "Render sorted-look category labels in monsterbold", sorter.bold_labels),
        enum_entry!("sorter.item_sort", "Sorter Item Order", "UI",
            "Item order within a category: last_word (sorter.lic style), alpha, or none (look order)",
            &["last_word", "alpha", "none"], sorter.item_sort),
        text_entry!("ui.terminal_title", "Terminal Title", "UI",
            "Terminal title template ({character}, {room}, {health}, ...); empty leaves the title alone",
            ui.terminal_title),
        opt_text_entry!("ui.betrayer_active_color", "Betrayer Active Color", "UI",
            "Highlight color for active Betrayer panel items", ui.betrayer_active_color),
        // ---- Appearance ---------------------------------------------
        text_entry!("active_theme", "Active Theme", "Appearance",
            "Currently active theme name", active_theme),
        opt_text_entry!("active_skin", "Active Skin", "Appearance",
            "GUI skin directory under ~/.vellum-fe/global/skins (empty = plain theme colors)", active_skin),
        opt_text_entry!("doll_image", "Injury Doll Image", "Appearance",
            "Injury doll override as a pool path (dolls/<file>.png); empty = the active skin's doll", doll_image),
        // ---- Terminal (TUI) -----------------------------------------
        SettingDef {
            key: "ui.color_mode",
            label: "Color Mode",
            category: "Terminal",
            description: "Terminal color rendering: direct (24-bit), slot (OSC4 palette), indexed (xterm 256)",
            kind: SettingKind::Enum { options: &["direct", "slot", "indexed"] },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.ui.color_mode.to_string()),
            set: |c, v| {
                c.ui.color_mode = match v.as_text()?.as_str() {
                    "direct" => ColorMode::Direct,
                    "slot" => ColorMode::Slot,
                    "indexed" => ColorMode::Indexed,
                    other => return Err(format!("unknown color mode '{}'", other)),
                };
                Ok(())
            },
        },
        SettingDef {
            key: "ui.timestamp_position",
            label: "Timestamp Position",
            category: "UI",
            description: "Default side of the line timestamps render on",
            kind: SettingKind::Enum { options: &["start", "end"] },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Text(c.ui.timestamp_position.to_string()),
            set: |c, v| {
                c.ui.timestamp_position = match v.as_text()?.as_str() {
                    "start" => TimestampPosition::Start,
                    "end" => TimestampPosition::End,
                    other => return Err(format!("unknown timestamp position '{}'", other)),
                };
                Ok(())
            },
        },
        // ---- Selection ----------------------------------------------
        bool_entry!("ui.selection_enabled", "Selection Enabled", "Selection",
            "Enable mouse text selection", ui.selection_enabled),
        bool_entry!("ui.selection_respect_window_boundaries", "Respect Window Boundaries", "Selection",
            "Selections stop at window edges", ui.selection_respect_window_boundaries),
        bool_entry!("ui.selection_auto_copy", "Auto-Copy Selection", "Selection",
            "Copy selection to the clipboard on mouse-up", ui.selection_auto_copy),
        enum_entry!("ui.drag_modifier_key", "Drag Modifier", "Selection",
            "Modifier key required to drag windows",
            &["ctrl", "alt", "shift"], ui.drag_modifier_key),
        // ---- Focus ---------------------------------------------------
        list_entry!("ui.focus.order", "Focus Order", "Focus",
            "Explicit Tab-cycle order (empty = layout order)", ui.focus.order),
        list_entry!("ui.focus.types", "Focusable Types", "Focus",
            "Widget types reachable via Tab", ui.focus.types),
        list_entry!("ui.focus.exclude", "Focus Exclude", "Focus",
            "Window names excluded from Tab focus", ui.focus.exclude),
        // ---- Performance monitor ------------------------------------
        // Metric toggles mirror performance::PERF_METRICS ids; the
        // registry test in performance.rs fails the build if they drift.
        bool_entry!("ui.performance_stats_enabled", "Performance Monitor", "Performance",
            "Show the performance monitor window", ui.performance_stats_enabled),
        int_entry!([Tui] "ui.perf_stats_x", "Overlay X", "Performance",
            "Performance overlay column (TUI cells)", 0..=1000, u16, ui.perf_stats_x),
        int_entry!([Tui] "ui.perf_stats_y", "Overlay Y", "Performance",
            "Performance overlay row (TUI cells)", 0..=1000, u16, ui.perf_stats_y),
        int_entry!([Tui] "ui.perf_stats_width", "Overlay Width", "Performance",
            "Performance overlay width (TUI cells)", 10..=200, u16, ui.perf_stats_width),
        int_entry!([Tui] "ui.perf_stats_height", "Overlay Height", "Performance",
            "Performance overlay height (TUI cells)", 5..=100, u16, ui.perf_stats_height),
        bool_entry!("ui.perf_show_fps", "Show Draws/s", "Performance",
            "Monitor: frame draws per second", ui.perf_show_fps),
        bool_entry!("ui.perf_show_render_times", "Show Render Times", "Performance",
            "Monitor: frame paint cost (avg/p95/max)", ui.perf_show_render_times),
        bool_entry!([Tui] "ui.perf_show_ui_times", "Show Draw Times", "Performance",
            "Monitor: widget draw pass, before terminal flush", ui.perf_show_ui_times),
        bool_entry!([Tui] "ui.perf_show_wrap_times", "Show Wrap Times", "Performance",
            "Monitor: text wrap timings", ui.perf_show_wrap_times),
        bool_entry!("ui.perf_show_net", "Show Network", "Performance",
            "Monitor: network throughput", ui.perf_show_net),
        bool_entry!("ui.perf_show_parse", "Show Parse", "Performance",
            "Monitor: parser timings and element rates", ui.perf_show_parse),
        bool_entry!("ui.perf_show_events", "Show Events", "Performance",
            "Monitor: event processing and queue depth", ui.perf_show_events),
        bool_entry!("ui.perf_show_cpu", "Show CPU", "Performance",
            "Monitor: process and system CPU usage", ui.perf_show_cpu),
        bool_entry!("ui.perf_show_memory", "Show Memory", "Performance",
            "Monitor: process RSS and virtual memory", ui.perf_show_memory),
        bool_entry!("ui.perf_show_lines", "Show Buffers", "Performance",
            "Monitor: buffered line and window counts", ui.perf_show_lines),
        bool_entry!("ui.perf_show_uptime", "Show Uptime", "Performance",
            "Monitor: session uptime", ui.perf_show_uptime),
        bool_entry!("ui.perf_show_spike_log", "Show Spike Log", "Performance",
            "Monitor: recent slow frames/events with context", ui.perf_show_spike_log),
        bool_entry!("ui.perf_show_per_window", "Show Window Costs", "Performance",
            "Monitor: most expensive windows to render", ui.perf_show_per_window),
        bool_entry!("ui.perf_sparklines", "Sparklines", "Performance",
            "Monitor: draw trend sparklines next to rows", ui.perf_sparklines),
        // ---- Sound ---------------------------------------------------
        bool_entry!("sound.enabled", "Sound Enabled", "Sound",
            "Enable the audio system (off skips audio init entirely)", sound.enabled),
        float_entry!("sound.volume", "Master Volume", "Sound",
            "Master volume", 0.0..=1.0, sound.volume),
        SettingDef {
            key: "sound.cooldown_ms",
            label: "Sound Cooldown (ms)",
            category: "Sound",
            description: "Minimum time between plays of the same sound",
            kind: SettingKind::Int { min: 0, max: 60000 },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Int(c.sound.cooldown_ms as i64),
            set: |c, v| {
                let raw = v.as_int()?;
                if !(0..=60000).contains(&raw) {
                    return Err("sound.cooldown_ms must be between 0 and 60000".to_string());
                }
                c.sound.cooldown_ms = raw as u64;
                Ok(())
            },
        },
        bool_entry!("sound.startup_music", "Startup Music", "Sound",
            "Play music on startup", sound.startup_music),
        SettingDef {
            key: "sound.startup_music_delay_ms",
            label: "Startup Music Delay (ms)",
            category: "Sound",
            description: "Delay before startup music plays",
            kind: SettingKind::Int { min: 0, max: 600000 },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Int(c.sound.startup_music_delay_ms as i64),
            set: |c, v| {
                let raw = v.as_int()?;
                if !(0..=600000).contains(&raw) {
                    return Err("sound.startup_music_delay_ms must be between 0 and 600000".to_string());
                }
                c.sound.startup_music_delay_ms = raw as u64;
                Ok(())
            },
        },
        // ---- Speech (TTS) -------------------------------------------
        bool_entry!("tts.enabled", "Speech Enabled", "Speech",
            "Enable text-to-speech", tts.enabled),
        float_entry!("tts.rate", "Speech Rate", "Speech",
            "Speech rate (1.0 = engine normal)", 0.1..=5.0, tts.rate),
        float_entry!("tts.volume", "Speech Volume", "Speech",
            "Speech volume", 0.0..=1.0, tts.volume),
        bool_entry!("tts.speak_thoughts", "Speak Thoughts", "Speech",
            "Automatically speak the thoughts window", tts.speak_thoughts),
        bool_entry!("tts.speak_speech", "Speak Speech", "Speech",
            "Automatically speak the speech window", tts.speak_speech),
        bool_entry!("tts.speak_main", "Speak Main", "Speech",
            "Automatically speak the main window", tts.speak_main),
        opt_text_entry!("tts.voice", "Voice", "Speech",
            "Preferred voice by name (empty = engine default)", tts.voice),
        list_entry!("tts.gags", "Speech Gags", "Speech",
            "Regexes for lines shown but never spoken", tts.gags),
        // ---- Targets -------------------------------------------------
        enum_entry!("target_list.status_position", "Status Position", "Targets",
            "Where creature status renders relative to the name",
            &["start", "end"], target_list.status_position),
        enum_entry!("target_list.truncation_mode", "Truncation Mode", "Targets",
            "How long creature names shorten",
            &["full", "noun"], target_list.truncation_mode),
        list_entry!("target_list.excluded_nouns", "Excluded Nouns", "Targets",
            "Nouns never treated as targets", target_list.excluded_nouns),
        opt_text_entry!("target_list.boss_color", "Boss Color", "Targets",
            "Text color for boss creatures", target_list.boss_color),
        opt_text_entry!("target_list.challenging_color", "Challenging Color", "Targets",
            "Text color for challenging creatures", target_list.challenging_color),
        opt_text_entry!("target_list.dead_color", "Dead Color", "Targets",
            "Text color for dead players in the players window", target_list.dead_color),
        // ---- Logging -------------------------------------------------
        bool_entry!("logging.enabled", "Raw Logging", "Logging",
            "Capture raw XML from the network layer", logging.enabled),
        opt_text_entry!("logging.dir", "Log Directory", "Logging",
            "Log directory (relative to profile dir, or absolute)", logging.dir),
        int_entry!("logging.buffer_lines", "Buffer Lines", "Logging",
            "Lines buffered before a write", 1..=100000, usize, logging.buffer_lines),
        SettingDef {
            key: "logging.flush_interval_ms",
            label: "Flush Interval (ms)",
            category: "Logging",
            description: "Periodic log flush interval",
            kind: SettingKind::Int { min: 100, max: 600000 },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Int(c.logging.flush_interval_ms as i64),
            set: |c, v| {
                let raw = v.as_int()?;
                if !(100..=600000).contains(&raw) {
                    return Err("logging.flush_interval_ms must be between 100 and 600000".to_string());
                }
                c.logging.flush_interval_ms = raw as u64;
                Ok(())
            },
        },
        int_entry!("logging.max_lines_per_file", "Max Lines Per File", "Logging",
            "Rotate the log after this many lines", 1000..=10000000, usize, logging.max_lines_per_file),
        bool_entry!("logging.timestamps", "Log Timestamps", "Logging",
            "Prefix each logged line with HH:MM:SS", logging.timestamps),
        // ---- Streams -------------------------------------------------
        // LEGACY: stays registered while the field exists (the leaf-coverage
        // test requires it), but load-time migration folds every entry into
        // streams.routes and clears the list, so its runtime value is
        // always empty. Retire alongside the field once routes fully land.
        list_entry!("streams.drop_unsubscribed", "Drop Unsubscribed", "Streams",
            "Streams silently discarded when nothing subscribes", streams.drop_unsubscribed),
        text_entry!("streams.fallback", "Fallback Window", "Streams",
            "Where orphaned streams route", streams.fallback),
        bool_entry!("streams.room_in_main", "Room In Main", "Streams",
            "streamWindow 'room' text flows to main (DragonRealms legacy off)", streams.room_in_main),
        // ---- Highlights toggles -------------------------------------
        bool_entry!("highlights.sounds_enabled", "Highlight Sounds", "Highlights",
            "Play sounds on pattern match", highlight_settings.sounds_enabled),
        bool_entry!("highlights.replace_enabled", "Replacements", "Highlights",
            "Enable text replacement patterns", highlight_settings.replace_enabled),
        bool_entry!("highlights.redirect_enabled", "Redirects", "Highlights",
            "Enable redirect patterns", highlight_settings.redirect_enabled),
        bool_entry!("highlights.coloring_enabled", "Coloring", "Highlights",
            "Enable color highlighting", highlight_settings.coloring_enabled),
        // ---- Web (phone) --------------------------------------------
        bool_entry!("web.enabled", "Web Server", "Web",
            "Enable the embedded web server for phones", web.enabled),
        SettingDef {
            key: "web.port",
            label: "Web Port",
            category: "Web",
            description: "HTTP/WebSocket port (base port when unpinned)",
            kind: SettingKind::Int { min: 1, max: 65535 },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Int(c.web.port as i64),
            set: |c, v| {
                let raw = v.as_int()?;
                if !(1..=65535).contains(&raw) {
                    return Err("web.port must be between 1 and 65535".to_string());
                }
                c.web.port = raw as u16;
                Ok(())
            },
        },
        text_entry!("web.bind", "Bind Address", "Web",
            "127.0.0.1 = this machine only; 0.0.0.0 allows LAN phones", web.bind),
        SettingDef {
            key: "web.pinned",
            label: "Pin Port",
            category: "Web",
            description: "Bind exactly this port or fail loudly (stable per-character bookmarks)",
            kind: SettingKind::Bool,
            scope: SettingScope::CharacterOnly,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Bool(c.web.pinned),
            set: |c, v| { c.web.pinned = v.as_bool()?; Ok(()) },
        },
        // Roaming phone prefs: the phone client's per-character look,
        // riding the ordinary settings_get/put wire. Unset (0 / empty)
        // means "no server opinion" — the phone keeps using its own
        // per-device localStorage value. Option<u8> has no registry
        // kind, so story_size uses Int with 0 as the documented
        // "unset" sentinel (the phone clamps applied values to its own
        // legible range).
        SettingDef {
            key: "web.story_size",
            label: "Phone Text Size",
            category: "Web",
            description: "Phone story text size in px; roams with the character (0 = unset, phone keeps its own)",
            kind: SettingKind::Int { min: 0, max: 32 },
            scope: SettingScope::GlobalOrCharacter,
            sensitive: false,
            frontend: FrontendScope::All,
            get: |c| SettingValue::Int(c.web.story_size.map_or(0, |v| v as i64)),
            set: |c, v| {
                let raw = v.as_int()?;
                if !(0..=32).contains(&raw) {
                    return Err("web.story_size must be between 0 and 32 (0 = unset)".to_string());
                }
                c.web.story_size = if raw == 0 { None } else { Some(raw as u8) };
                Ok(())
            },
        },
        opt_text_entry!("web.theme", "Phone Theme", "Web",
            "Phone theme preset; roams with the character (empty = unset, phone keeps its own)", web.theme),
        list_entry!("web.chip_order", "Phone Chip Order", "Web",
            "Phone stream-chip order, leftmost first; roams with the character (empty = unset)", web.chip_order),
        // ---- Map -----------------------------------------------------
        opt_text_entry!("map.lich_dir", "Lich Directory", "Map",
            "Lich install dir for mapdb discovery (empty = auto)", map.lich_dir),
        opt_text_entry!("map.mapdb_path", "Mapdb Path", "Map",
            "Explicit mapdb JSON file (overrides discovery)", map.mapdb_path),
        text_entry!("map.mapdb_repo", "Mapdb Repo", "Map",
            "GitHub owner/repo for mapdb downloads (empty disables)", map.mapdb_repo),
        bool_entry!("map.mapping_mode", "Mapping Mode", "Map",
            "Sketch unmapped rooms as ghost overlays", map.mapping_mode),
        // ---- Travel --------------------------------------------------
        bool_entry!("go2.native_map_clicks", "Native Map Clicks", "Travel",
            "Map clicks travel natively instead of sending ;go2", go2.native_map_clicks),
        text_entry!("go2.weaponsack", "Weapon Sack", "Travel",
            "Container the hands-stow uses for a weapon when your ready sheath doesn't cover it (name)", go2.weaponsack),
        text_entry!("go2.lootsack", "Loot Sack", "Travel",
            "Fallback container the hands-stow uses for anything not routed by ready/sheath/weapon sack (name)", go2.lootsack),
        bool_entry!("go2.lich_fallback", "Lich ;go2 Fallback", "Travel",
            "When native .go2 can't cross an edge, hand off to Lich's ;go2 (Lich connections only)", go2.lich_fallback),
        bool_entry!("go2.use_seeking", "Use Voln Seeking", "Travel",
            "Route through Voln Symbol of Seeking edges (only takes effect for a Voln Master)", go2.use_seeking),
        bool_entry!("go2.use_portmasters", "Use Portmasters", "Travel",
            "Route through portmaster ship travel (costs silver - keep enough on hand)", go2.use_portmasters),
    ];
    defs.sort_by_key(|def| def.key);
    defs
});

/// The full settings catalog, sorted by key.
pub fn registry() -> &'static [SettingDef] {
    &REGISTRY
}

/// Look up one setting by its dotted key.
pub fn find(key: &str) -> Option<&'static SettingDef> {
    REGISTRY
        .binary_search_by_key(&key, |def| def.key)
        .ok()
        .map(|idx| &REGISTRY[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample value for a setting that is guaranteed to differ from its
    /// current value in `config` — used to prove settings survive copies
    /// and merges.
    fn divergent_value(def: &SettingDef, config: &Config) -> SettingValue {
        match (def.get)(config) {
            SettingValue::Bool(current) => SettingValue::Bool(!current),
            SettingValue::Int(current) => {
                // Stay in range: move up unless that would leave the max.
                let bumped = match def.kind {
                    SettingKind::Int { max, .. } if current + 1 > max => current - 1,
                    _ => current + 1,
                };
                SettingValue::Int(bumped)
            }
            SettingValue::Float(current) => {
                let bumped = match def.kind {
                    SettingKind::Float { min, max } => {
                        if current + 0.125 <= max { current + 0.125 } else { (min + max) / 3.0 }
                    }
                    _ => current + 0.125,
                };
                SettingValue::Float(bumped)
            }
            SettingValue::Text(current) => match def.kind {
                SettingKind::Enum { options } => SettingValue::Text(
                    options
                        .iter()
                        .find(|opt| **opt != current)
                        .expect("enum settings need at least two options")
                        .to_string(),
                ),
                _ => SettingValue::Text(format!("zz-test-{}", current.len())),
            },
            SettingValue::List(_) => SettingValue::List(vec!["zz-test".to_string()]),
        }
    }

    /// A Config with EVERY registered setting forced away from its default
    /// (also exercises every setter).
    fn fully_divergent_config() -> Config {
        let mut config = Config::default();
        for def in registry() {
            let value = divergent_value(def, &config);
            (def.set)(&mut config, &value)
                .unwrap_or_else(|err| panic!("set {} failed: {}", def.key, err));
        }
        config
    }

    fn collect_leaf_paths(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    collect_leaf_paths(child, &path, out);
                }
            }
            // Arrays are leaves (string lists) or exempt structures.
            _ => out.push(prefix.to_string()),
        }
    }

    /// THE drift guard: every serialized Config leaf must be registered or
    /// explicitly exempt, and every registry key must point at a real
    /// field. Adding a Config field without a registry entry (or an
    /// exemption with a reason) fails here.
    #[test]
    fn every_config_leaf_is_registered_or_exempt() {
        // Serialize a fully-populated config so optional
        // (skip_serializing_if) fields are present in the JSON shape.
        let config = fully_divergent_config();
        let json = serde_json::to_value(&config).expect("Config serializes");
        let mut leaves = Vec::new();
        collect_leaf_paths(&json, "", &mut leaves);

        let mut unregistered: Vec<&String> = leaves
            .iter()
            .filter(|leaf| {
                find(leaf).is_none()
                    && !EXEMPT_PREFIXES
                        .iter()
                        .any(|prefix| *leaf == prefix || leaf.starts_with(&format!("{}.", prefix)))
            })
            .collect();
        unregistered.sort();
        assert!(
            unregistered.is_empty(),
            "Config fields missing from the settings registry (register them \
             in config/registry.rs or add an EXEMPT_PREFIXES entry with a \
             reason): {:#?}",
            unregistered
        );

        // Reverse direction: registry keys must resolve to real leaves.
        let stale: Vec<&&'static str> = registry()
            .iter()
            .map(|def| &def.key)
            .filter(|key| !leaves.iter().any(|leaf| leaf == *key))
            .collect();
        assert!(
            stale.is_empty(),
            "Registry keys that no longer match a Config field: {:?}",
            stale
        );
    }

    /// Restart-amnesia guard: merge_with must preserve every registered
    /// setting a character profile sets. This is the bug class that ate
    /// active_skin, web, map, and go2.
    #[test]
    fn merge_with_preserves_every_registered_setting() {
        let character = fully_divergent_config();
        let mut merged = Config::default();
        merged.merge_with(character.clone());

        let mut lost: Vec<String> = Vec::new();
        for def in registry() {
            let expected = (def.get)(&character);
            let actual = (def.get)(&merged);
            if expected != actual {
                lost.push(format!(
                    "{} (profile set {:?}, merged got {:?})",
                    def.key, expected, actual
                ));
            }
        }
        assert!(
            lost.is_empty(),
            "merge_with dropped registered settings (add them to merge_with \
             in config/io.rs): {:#?}",
            lost
        );
    }

    /// Every setter round-trips its own getter.
    #[test]
    fn set_then_get_round_trips() {
        let mut config = Config::default();
        for def in registry() {
            let value = divergent_value(def, &config);
            (def.set)(&mut config, &value)
                .unwrap_or_else(|err| panic!("set {} failed: {}", def.key, err));
            let read_back = (def.get)(&config);
            // Floats pass through f32 storage; compare with tolerance.
            if let (SettingValue::Float(a), SettingValue::Float(b)) = (&read_back, &value) {
                assert!(
                    (a - b).abs() < 1e-5,
                    "{} did not round-trip through set/get ({} vs {})",
                    def.key, a, b
                );
            } else {
                assert_eq!(
                    read_back, value,
                    "{} did not round-trip through set/get",
                    def.key
                );
            }
        }
    }

    /// copy_setting (the scoped single-key save path) must work for every
    /// registered key, not the historical hand-picked 15.
    #[test]
    fn copy_setting_covers_every_registered_key() {
        let src = fully_divergent_config();
        for def in registry() {
            let mut dest = Config::default();
            assert!(
                Config::copy_setting(&mut dest, &src, def.key),
                "copy_setting refused registered key {}",
                def.key
            );
            assert_eq!(
                (def.get)(&dest),
                (def.get)(&src),
                "copy_setting did not copy {}",
                def.key
            );
        }
        let mut dest = Config::default();
        assert!(!Config::copy_setting(&mut dest, &src, "no.such.key"));
    }

    #[test]
    fn find_is_exact() {
        assert!(find("ui.buffer_size").is_some());
        assert!(find("ui.buffer").is_none());
        assert!(find("").is_none());
    }
}
