//! Widget configuration data structures.
//!
//! Per-widget settings structs referenced by `WindowDef` variants, plus
//! `WindowBase` (shared window geometry/chrome) and `BorderSides`.
//! Serde default fns live next to the structs that reference them.

use super::*;
use crate::data::geometry::{Height, Width};

/// A window's persisted show/hide state. Replaces the old `visible: bool`.
/// `Hidden` means BOTH "don't render" AND "suppress the game from
/// auto-spawning it" — the unified-windows rule. `Ephemeral` marks a
/// session-only window (containers) that is never persisted and is wiped
/// on relog; it renders like `Shown` while alive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowVisibility {
    #[default]
    Shown,
    Hidden,
    Ephemeral,
}

impl WindowVisibility {
    /// Whether the window should render.
    pub fn is_shown(&self) -> bool {
        matches!(self, WindowVisibility::Shown | WindowVisibility::Ephemeral)
    }
    /// Whether the game may auto-(re)spawn this window. Hidden suppresses it.
    pub fn allows_autospawn(&self) -> bool {
        !matches!(self, WindowVisibility::Hidden)
    }
    /// Whether this window persists to layout.toml. Ephemeral does not.
    pub fn is_persistent(&self) -> bool {
        !matches!(self, WindowVisibility::Ephemeral)
    }
}

// Serde: persist as a lowercase string ("shown"/"hidden"/"ephemeral"), but
// ALSO accept the legacy `visible = true|false` bool so old layout.toml
// files keep loading. Ephemeral is never written (those windows aren't
// persisted), so it only appears at runtime.
impl serde::Serialize for WindowVisibility {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            WindowVisibility::Shown => "shown",
            WindowVisibility::Hidden => "hidden",
            WindowVisibility::Ephemeral => "ephemeral",
        })
    }
}

impl<'de> serde::Deserialize<'de> for WindowVisibility {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Compat {
            Bool(bool),
            Str(String),
        }
        match Compat::deserialize(d)? {
            // Legacy layout.toml: visible = true|false.
            Compat::Bool(true) => Ok(WindowVisibility::Shown),
            Compat::Bool(false) => Ok(WindowVisibility::Hidden),
            Compat::Str(s) => match s.to_ascii_lowercase().as_str() {
                "shown" | "visible" | "true" => Ok(WindowVisibility::Shown),
                "hidden" | "false" => Ok(WindowVisibility::Hidden),
                "ephemeral" => Ok(WindowVisibility::Ephemeral),
                other => Err(D::Error::custom(format!(
                    "invalid window visibility '{}'",
                    other
                ))),
            },
        }
    }
}

/// What game source a window is bound to, so the client can find the ONE
/// (or several) windows a dialog/stream/container feed belongs to by id,
/// independent of the user's display name. This is the identity that
/// prevents duplicate auto-spawns and lets multiple windows share a feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum WindowBinding {
    /// A game dialog id (expr, stance, combat, encum, ...).
    Dialog(String),
    /// A game stream id (thoughts, loot, bounty, ...).
    Stream(String),
    /// A container id (session-only; not persisted).
    Container(String),
}

impl WindowBinding {
    /// The bound game id, whatever the source kind.
    pub fn id(&self) -> &str {
        match self {
            WindowBinding::Dialog(id)
            | WindowBinding::Stream(id)
            | WindowBinding::Container(id) => id,
        }
    }
}

/// Border sides configuration - which borders to show
/// Serializes to/from array of strings in TOML: ["left", "right", "top", "bottom"]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "Vec<String>", into = "Vec<String>")]
pub struct BorderSides {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl Default for BorderSides {
    fn default() -> Self {
        Self {
            top: true,
            bottom: true,
            left: true,
            right: true,
        }
    }
}

// Convert from TOML array format ["left", "right"] to BorderSides struct
impl From<Vec<String>> for BorderSides {
    fn from(sides: Vec<String>) -> Self {
        let mut border = Self {
            top: false,
            bottom: false,
            left: false,
            right: false,
        };

        for side in sides {
            match side.to_lowercase().as_str() {
                "top" => border.top = true,
                "bottom" => border.bottom = true,
                "left" => border.left = true,
                "right" => border.right = true,
                _ => {} // Ignore unknown sides
            }
        }

        border
    }
}

// Convert from BorderSides struct to TOML array format
impl From<BorderSides> for Vec<String> {
    fn from(border: BorderSides) -> Self {
        let mut sides = Vec::new();
        if border.top {
            sides.push("top".to_string());
        }
        if border.bottom {
            sides.push("bottom".to_string());
        }
        if border.left {
            sides.push("left".to_string());
        }
        if border.right {
            sides.push("right".to_string());
        }
        sides
    }
}

impl BorderSides {
    /// True if any side is enabled
    pub fn any(&self) -> bool {
        self.top || self.bottom || self.left || self.right
    }
}

impl WindowBase {
    fn horizontal_border_units_for(show: bool, sides: &BorderSides) -> u16 {
        if !show {
            return 0;
        }
        (sides.top as u16) + (sides.bottom as u16)
    }

    fn vertical_border_units_for(show: bool, sides: &BorderSides) -> u16 {
        if !show {
            return 0;
        }
        (sides.left as u16) + (sides.right as u16)
    }

    /// Number of rows consumed by borders (top + bottom)
    pub fn horizontal_border_units(&self) -> u16 {
        Self::horizontal_border_units_for(self.show_border, &self.border_sides)
    }

    /// Number of columns consumed by borders (left + right)
    pub fn vertical_border_units(&self) -> u16 {
        Self::vertical_border_units_for(self.show_border, &self.border_sides)
    }

    /// Rows available for the widget's interior content
    pub fn content_rows(&self) -> u16 {
        self.rows.get().saturating_sub(self.horizontal_border_units())
    }

    /// Columns available for the widget's interior content
    pub fn content_cols(&self) -> u16 {
        self.cols.get().saturating_sub(self.vertical_border_units())
    }

    /// Apply new border visibility/sides while keeping interior size the same.
    /// Also adjusts min_rows/max_rows/min_cols/max_cols proportionally (if set).
    pub fn apply_border_configuration(&mut self, show_border: bool, border_sides: BorderSides) {
        let prev_horizontal = self.horizontal_border_units();
        let prev_vertical = self.vertical_border_units();

        // Calculate content dimensions (interior without borders)
        let content_rows = self.rows.get().saturating_sub(prev_horizontal).max(1);
        let content_cols = self.cols.get().saturating_sub(prev_vertical).max(1);

        // Calculate content-based min/max (if set) - None stays None
        let content_min_rows = self
            .min_rows
            .map(|m| m.saturating_sub(prev_horizontal).max(1));
        let content_max_rows = self
            .max_rows
            .map(|m| m.saturating_sub(prev_horizontal).max(1));
        let content_min_cols = self
            .min_cols
            .map(|m| m.saturating_sub(prev_vertical).max(1));
        let content_max_cols = self
            .max_cols
            .map(|m| m.saturating_sub(prev_vertical).max(1));

        // Apply new border configuration
        self.show_border = show_border && border_sides.any();
        self.border_sides = border_sides;

        let new_horizontal =
            Self::horizontal_border_units_for(self.show_border, &self.border_sides);
        let new_vertical = Self::vertical_border_units_for(self.show_border, &self.border_sides);

        // Adjust rows/cols (minimum 1)
        self.rows = Height::new((content_rows + new_horizontal).max(1));
        self.cols = Width::new((content_cols + new_vertical).max(1));

        // Adjust min/max if set (minimum 1, None stays None)
        self.min_rows = content_min_rows.map(|m| (m + new_horizontal).max(1));
        self.max_rows = content_max_rows.map(|m| (m + new_horizontal).max(1));
        self.min_cols = content_min_cols.map(|m| (m + new_vertical).max(1));
        self.max_cols = content_max_cols.map(|m| (m + new_vertical).max(1));

        // Enforce constraints on rows/cols
        if let Some(min_rows) = self.min_rows {
            if self.rows.get() < min_rows {
                self.rows = Height::new(min_rows);
            }
        }
        if let Some(max_rows) = self.max_rows {
            if self.rows.get() > max_rows {
                self.rows = Height::new(max_rows);
            }
        }
        if let Some(min_cols) = self.min_cols {
            if self.cols.get() < min_cols {
                self.cols = Width::new(min_cols);
            }
        }
        if let Some(max_cols) = self.max_cols {
            if self.cols.get() > max_cols {
                self.cols = Width::new(max_cols);
            }
        }
    }

    /// Apply a change to an optional content row (like show_label for encumbrance).
    /// When enabling (false -> true), adds 1 row; when disabling (true -> false), removes 1 row.
    /// Also adjusts min_rows/max_rows proportionally (if set).
    pub fn apply_optional_content_row(&mut self, new_show: bool, prev_show: bool) {
        if new_show == prev_show {
            return; // No change
        }

        let delta: i16 = if new_show { 1 } else { -1 };

        // Adjust rows (minimum 1)
        self.rows = Height::new((self.rows.get() as i16 + delta).max(1) as u16);

        // Adjust min/max if set (minimum 1, None stays None)
        self.min_rows = self.min_rows.map(|m| (m as i16 + delta).max(1) as u16);
        self.max_rows = self.max_rows.map(|m| (m as i16 + delta).max(1) as u16);

        // Enforce constraints
        if let Some(min_rows) = self.min_rows {
            if self.rows.get() < min_rows {
                self.rows = Height::new(min_rows);
            }
        }
        if let Some(max_rows) = self.max_rows {
            if self.rows.get() > max_rows {
                self.rows = Height::new(max_rows);
            }
        }
    }
}

/// Base configuration shared by ALL widget types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowBase {
    pub name: String,
    #[serde(default)]
    pub row: crate::data::geometry::Row,
    #[serde(default)]
    pub col: crate::data::geometry::Col,
    #[serde(default = "default_rows")]
    pub rows: crate::data::geometry::Height,
    #[serde(default = "default_cols")]
    pub cols: crate::data::geometry::Width,
    #[serde(default = "default_show_border")]
    pub show_border: bool,
    #[serde(default = "default_border_style")]
    pub border_style: String, // "single", "double", "rounded", "thick", "plain"
    #[serde(default)]
    pub border_sides: BorderSides,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default = "default_show_title")]
    pub show_title: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default = "default_title_position")]
    pub title_position: String,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub text_color: Option<String>,
    #[serde(default = "default_transparent_background")]
    pub transparent_background: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub min_rows: Option<u16>,
    #[serde(default)]
    pub max_rows: Option<u16>,
    #[serde(default)]
    pub min_cols: Option<u16>,
    #[serde(default)]
    pub max_cols: Option<u16>,
    /// Persisted show/hide state. Replaces the legacy `visible: bool`;
    /// serde reads either the new `visibility = "shown"|"hidden"` string
    /// or the old `visible = true|false` bool (via the alias + the enum's
    /// Deserialize compat), defaulting to Shown.
    #[serde(default, alias = "visible")]
    pub visibility: WindowVisibility,
    /// Game source this window is bound to (dialog/stream/container id), so
    /// feeds resolve to the right window(s) regardless of display name.
    /// None for hand-placed/custom windows with no game binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<WindowBinding>,
    /// Content alignment within widget area
    #[serde(default)]
    pub content_align: Option<String>,
    /// Speak new lines routed to this window via TTS (accessibility).
    /// Off by default; the classic thoughts/speech/main config toggles
    /// still apply on top for backward compatibility.
    #[serde(default)]
    pub tts_speak: bool,
    /// GUI: per-window text size override in points; None uses the global
    /// text size. The TUI ignores it (terminals have no text size).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_size: Option<f32>,
    /// GUI: per-window font family name; None uses the default font.
    /// The TUI ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
}

/// Text widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextWidgetData {
    #[serde(default)]
    pub streams: Vec<String>,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default = "default_true")]
    pub wordwrap: bool,
    #[serde(default)]
    pub show_timestamps: bool,
    /// Timestamp position (overrides ui.timestamp_position if Some)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_position: Option<TimestampPosition>,
    /// Enable compact display mode (transforms verbose bounty text to 1-4 lines)
    #[serde(default)]
    pub compact: bool,
}

/// Room widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomWidgetData {
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,

    /// Component visibility toggles (default: all true)
    #[serde(default = "default_true")]
    pub show_desc: bool,

    #[serde(default = "default_true")]
    pub show_objs: bool,

    #[serde(default = "default_true")]
    pub show_players: bool,

    #[serde(default = "default_true")]
    pub show_exits: bool,

    /// Display the room name within the window content (useful when borders are hidden)
    #[serde(default = "default_false")]
    pub show_name: bool,
}

/// Command input widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CommandInputWidgetData {
    #[serde(default)]
    pub text_color: Option<String>,
    #[serde(default)]
    pub cursor_color: Option<String>,
    #[serde(default)]
    pub cursor_background_color: Option<String>,
    #[serde(default)]
    pub prompt_icon: Option<String>,
    #[serde(default)]
    pub prompt_icon_color: Option<String>,
}

/// Inventory widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryWidgetData {
    #[serde(default)]
    pub streams: Vec<String>,
    #[serde(default)]
    pub buffer_size: usize,
    #[serde(default = "default_true")]
    pub wordwrap: bool,
    #[serde(default)]
    pub show_timestamps: bool,
}

/// TabbedText widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabbedTextWidgetData {
    #[serde(default)]
    pub tabs: Vec<TabbedTextTab>,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default = "default_tab_bar_position")]
    pub tab_bar_position: String,
    #[serde(default)]
    pub tab_separator: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_active_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_inactive_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_unread_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_unread_prefix: Option<String>,
}

fn default_tab_bar_position() -> String {
    "top".to_string()
}

/// Tab configuration for TabbedText widget
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TabbedTextTab {
    pub name: String,
    /// Single stream (for compatibility) - converts to streams array
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    /// Multiple streams (preferred) - if both set, this takes precedence
    #[serde(default)]
    pub streams: Vec<String>,
    /// Show timestamps (overrides ui.show_timestamps if Some)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_timestamps: Option<bool>,
    /// Ignore activity/unread indicators for this tab
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_activity: Option<bool>,
    /// Timestamp position (overrides ui.timestamp_position if Some)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_position: Option<TimestampPosition>,
}

impl TabbedTextTab {
    /// Get the list of streams for this tab
    /// Handles both `stream` (singular) and `streams` (plural) fields
    pub fn get_streams(&self) -> Vec<String> {
        if !self.streams.is_empty() {
            self.streams.clone()
        } else if let Some(stream) = &self.stream {
            vec![stream.clone()]
        } else {
            vec![]
        }
    }
}

/// Progress bar widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressWidgetData {
    /// Progress feed identifier (XML progressBar id); case-sensitive
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub numbers_only: bool,
    /// When true, show only the current value (no label, no max)
    #[serde(default)]
    pub current_only: bool,
}

/// Countdown timer widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountdownWidgetData {
    /// Countdown feed identifier (XML id), case-sensitive
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<char>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub background_color: Option<String>,
}

/// Compass widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompassWidgetData {
    #[serde(default)]
    pub active_color: Option<String>, // Color for available exits (default: green)
    #[serde(default)]
    pub inactive_color: Option<String>, // Color for unavailable exits (default: dark gray)
}

/// Map widget specific data
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MapWidgetData {
    /// Pixels per grid cell (default 16).
    #[serde(default)]
    pub zoom: Option<f32>,
}

/// Injury doll widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InjuryDollWidgetData {
    #[serde(default)]
    pub injury_default_color: Option<String>, // Level 0: none (default: #333333)
    #[serde(default)]
    pub injury1_color: Option<String>, // Level 1: injury 1 (default: #aa5500)
    #[serde(default)]
    pub injury2_color: Option<String>, // Level 2: injury 2 (default: #ff8800)
    #[serde(default)]
    pub injury3_color: Option<String>, // Level 3: injury 3 (default: #ff0000)
    #[serde(default)]
    pub scar1_color: Option<String>, // Level 4: scar 1 (default: #999999)
    #[serde(default)]
    pub scar2_color: Option<String>, // Level 5: scar 2 (default: #777777)
    #[serde(default)]
    pub scar3_color: Option<String>, // Level 6: scar 3 (default: #555555)
}

/// Indicator widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndicatorWidgetData {
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub indicator_id: Option<String>,
    #[serde(default = "default_indicator_inactive_color")]
    pub inactive_color: Option<String>,
    #[serde(default = "default_indicator_active_color")]
    pub active_color: Option<String>,
    #[serde(default)]
    pub default_status: Option<String>, // legacy
    #[serde(default)]
    pub default_color: Option<String>,  // legacy
}

/// Dashboard widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardWidgetData {
    /// Layout direction: "horizontal", "vertical", or "grid:RxC"
    #[serde(default = "default_dashboard_layout", rename = "dashboard_layout")]
    pub layout: String,
    /// Spacing between indicators (characters)
    #[serde(default = "default_dashboard_spacing", rename = "dashboard_spacing")]
    pub spacing: u16,
    /// Hide inactive indicators (value = 0)
    #[serde(
        default = "default_dashboard_hide_inactive",
        rename = "dashboard_hide_inactive"
    )]
    pub hide_inactive: bool,
    /// Indicator definitions (id/icon/colors)
    #[serde(default, rename = "dashboard_indicators")]
    pub indicators: Vec<DashboardIndicatorDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardIndicatorDef {
    pub id: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub colors: Vec<String>,
}

fn default_indicator_active_color() -> Option<String> {
    Some("#00ff00".to_string())
}

fn default_indicator_inactive_color() -> Option<String> {
    Some("#555555".to_string())
}

/// Hand widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandWidgetData {
    /// Optional icon prefix (e.g., "L:", "R:", "S:")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Icon color (falls back to window/text color if None)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    /// Text color override (also overrides link color if set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    /// Status-driven icon states, first match wins (hotbar-style). A
    /// matched state's icon/text replace the static icon while its
    /// condition holds; no match falls through to the static settings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<HandIconState>,
}

/// One condition-driven hand icon state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandIconState {
    pub when: super::HotbarCondition,
    /// GUI icon while the state holds (pool image / sheet cell /
    /// `IconRef::None` for no art). None = keep the resolved default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<crate::data::IconRef>,
    /// TUI text prefix while the state holds (the TUI renders no images).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Icon color override while the state holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
}

/// Active effects widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveEffectsWidgetData {
    pub category: String, // "Buffs", "Debuffs", "Cooldowns", "ActiveSpells"
}

/// Performance widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceWidgetData {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub show_fps: bool,
    #[serde(default = "default_true")]
    pub show_frame_times: bool,
    #[serde(default = "default_true")]
    pub show_render_times: bool,
    #[serde(default = "default_true")]
    pub show_ui_times: bool,
    #[serde(default = "default_true")]
    pub show_wrap_times: bool,
    #[serde(default = "default_true")]
    pub show_net: bool,
    #[serde(default = "default_true")]
    pub show_parse: bool,
    #[serde(default = "default_true")]
    pub show_events: bool,
    #[serde(default = "default_true")]
    pub show_memory: bool,
    #[serde(default = "default_true")]
    pub show_lines: bool,
    #[serde(default = "default_true")]
    pub show_uptime: bool,
    #[serde(default = "default_true")]
    pub show_jitter: bool,
    #[serde(default = "default_true")]
    pub show_frame_spikes: bool,
    #[serde(default = "default_true")]
    pub show_event_lag: bool,
    #[serde(default = "default_true")]
    pub show_memory_delta: bool,
}

/// Targets widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetsWidgetData {
    #[serde(default = "default_target_entity_id")]
    pub entity_id: String,
    /// Show count of filtered body parts (arms, tentacles, etc.) on bottom border
    #[serde(default)]
    pub show_body_part_count: bool,
    /// Status display position: "start" or "end" (overrides global config if set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_position: Option<String>,
}

/// Players widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayersWidgetData {
    #[serde(default = "default_player_entity_id")]
    pub entity_id: String,
}

/// Items widget specific data (for room objects/items on ground)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemsWidgetData {
    #[serde(default = "default_items_entity_id")]
    pub entity_id: String,
}

/// Container widget specific data (for container windows like bags, backpacks)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerWidgetData {
    /// Container title to display (e.g., "Bandolier", "Backpack")
    /// Matched case-insensitively against container titles from the game
    #[serde(default)]
    pub container_title: String,
}

/// Dialog panel widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DialogPanelWidgetData {
    /// Dialog id this panel renders (e.g. "combat"). Its content comes
    /// from ui_state.dialog_store, accumulated from the game's dialogData.
    #[serde(default)]
    pub dialog_id: String,
}

/// Spacer widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpacerWidgetData {
    // No extra fields currently
}

/// Quickbar widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickbarWidgetData {
    // No extra fields currently
}

/// Hotkeybar widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotkeybarWidgetData {
    /// Name of the bar in hotbars.toml this window displays
    #[serde(default = "default_hotkeybar_bar")]
    pub bar: String,
    /// "horizontal" (buttons flow on one row) or "vertical" (one per row)
    #[serde(default = "default_hotkeybar_orientation")]
    pub orientation: String,
}

pub(crate) fn default_hotkeybar_bar() -> String {
    "default".to_string()
}

pub(crate) fn default_hotkeybar_orientation() -> String {
    "horizontal".to_string()
}

/// Quickbar entry definition for custom quickbars
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum QuickbarEntryConfig {
    Link {
        label: String,
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        echo: Option<String>,
    },
    MenuLink {
        label: String,
        exist: String,
        noun: String,
    },
    #[serde(alias = "sep")]
    Separator,
}

/// Custom quickbar definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickbarDefinition {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub entries: Vec<QuickbarEntryConfig>,
}

/// Custom quickbar configuration
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuickbarsConfig {
    #[serde(default)]
    pub custom: Vec<QuickbarDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Spells window widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpellsWidgetData {
    // No extra fields currently - uses "spells" stream
}

/// Text replacement rule for perception widget
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextReplacement {
    pub pattern: String,   // Pattern to find (regex if metacharacters detected)
    pub replace: String,   // Replacement text (empty string to remove)
}

/// Pre-compiled text replacement for runtime use.
/// Regex is compiled once at creation, not on every application.
#[derive(Debug, Clone)]
pub struct CompiledTextReplacement {
    /// Original pattern string (for literal matching or error fallback)
    pattern: String,
    /// Replacement text
    replace: String,
    /// Pre-compiled regex (None if pattern is literal or invalid regex)
    compiled_regex: Option<regex::Regex>,
}

impl CompiledTextReplacement {
    /// Compile a TextReplacement into a CompiledTextReplacement
    pub fn compile(replacement: &TextReplacement) -> Self {
        let pattern = replacement.pattern.as_str();
        let is_regex = pattern.contains('\\')
            || pattern.contains('^')
            || pattern.contains('$')
            || pattern.contains('.')
            || pattern.contains('*')
            || pattern.contains('+')
            || pattern.contains('?')
            || pattern.contains('(')
            || pattern.contains(')')
            || pattern.contains('[')
            || pattern.contains(']')
            || pattern.contains('{')
            || pattern.contains('}')
            || pattern.contains('|');

        let compiled_regex = if is_regex {
            match regex::Regex::new(pattern) {
                Ok(re) => Some(re),
                Err(e) => {
                    tracing::warn!(
                        "Invalid regex pattern '{}': {}, will use literal match",
                        pattern,
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        Self {
            pattern: replacement.pattern.clone(),
            replace: replacement.replace.clone(),
            compiled_regex,
        }
    }

    /// Apply this replacement to the given text
    pub fn apply(&self, text: &str) -> String {
        if let Some(ref re) = self.compiled_regex {
            re.replace_all(text, self.replace.as_str()).into_owned()
        } else {
            text.replace(&self.pattern, &self.replace)
        }
    }
}

/// Compile a slice of TextReplacements into CompiledTextReplacements.
/// Call this once at config load or when replacements change.
pub fn compile_text_replacements(replacements: &[TextReplacement]) -> Vec<CompiledTextReplacement> {
    replacements.iter().map(CompiledTextReplacement::compile).collect()
}

/// Apply pre-compiled text replacements (efficient - no regex compilation).
pub fn apply_compiled_text_replacements(text: &str, replacements: &[CompiledTextReplacement]) -> String {
    let mut result = text.to_string();
    for replacement in replacements {
        result = replacement.apply(&result);
    }
    result
}

/// Sort direction for perception entries
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SortDirection {
    #[serde(rename = "ascending")]
    Ascending,   // Lowest weight first (Fading → Roisaen → Other → Indefinite → OM → Percentage)

    #[serde(rename = "descending")]
    Descending,  // Highest weight first (Percentage → OM → Indefinite → Other → Roisaen → Fading)
}

impl Default for SortDirection {
    fn default() -> Self {
        Self::Descending
    }
}

fn default_perception_stream() -> String {
    "percWindow".to_string()
}

fn default_perception_buffer_size() -> usize {
    100
}

/// Perception window widget specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerceptionWidgetData {
    #[serde(default = "default_perception_stream")]
    pub stream: String,              // Stream ID to receive perception data from

    #[serde(default = "default_perception_buffer_size")]
    pub buffer_size: usize,          // Maximum number of perception entries to keep

    #[serde(default)]
    pub sort_direction: SortDirection,  // Ascending or descending sort by weight

    #[serde(default)]
    pub text_replacements: Vec<TextReplacement>,  // User-defined find/replace rules

    #[serde(default)]
    pub use_short_spell_names: bool,  // Use abbreviated spell names (Profanity-style)
}

/// DragonRealms experience widget data
/// Displays skill/experience components from `<component id='exp XXX'>` tags
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExperienceWidgetData {
    /// Text alignment: "left", "center", or "right" (default: "left")
    #[serde(default = "default_experience_align")]
    pub align: String,
}

fn default_experience_align() -> String {
    "left".to_string()
}

/// GS4 Experience widget data (level + mind state + experience)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GS4ExperienceWidgetData {
    /// Text alignment: "left", "center", or "right" (default: "left")
    #[serde(default = "default_experience_align")]
    pub align: String,
    /// Show level text (yourLvl label) - default true
    #[serde(default = "default_true")]
    pub show_level: bool,
    /// Show experience progress bar (nextLvlPB) - default true
    #[serde(default = "default_true")]
    pub show_exp_bar: bool,
    /// Show mind state progress bar - default true
    #[serde(default = "default_true")]
    pub show_mind_bar: bool,
    /// Show total absorbed experience line - default false (new data feed;
    /// off keeps existing layouts unchanged)
    #[serde(default)]
    pub show_total_exp: bool,
    /// Show total ascension experience line - default false
    #[serde(default)]
    pub show_ascension_exp: bool,
    /// Mind bar fill color (default: cyan)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mind_bar_color: Option<String>,
    /// Exp bar fill color (default: theme background for max-level users)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp_bar_color: Option<String>,
}

/// Encumbrance widget data (progress bar + optional label)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EncumbranceWidgetData {
    /// Text alignment: "left", "center", or "right" (default: "left")
    #[serde(default = "default_experience_align")]
    pub align: String,
    /// Show descriptive blurb text - default true
    #[serde(default = "default_true")]
    pub show_label: bool,
    /// Show the encumbrance level bar - default true
    #[serde(default = "default_true")]
    pub show_bar: bool,
    /// Bar color for light encumbrance (0-20) - default green
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_light: Option<String>,
    /// Bar color for moderate encumbrance (21-50) - default yellow
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_moderate: Option<String>,
    /// Bar color for heavy encumbrance (51-80) - default orange
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_heavy: Option<String>,
    /// Bar color for critical encumbrance (81-100) - default red
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_critical: Option<String>,
}

/// MiniVitals widget data (horizontal 4-bar layout)
/// Works with both GS4 (mana) and DR (concentration)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MiniVitalsWidgetData {
    /// Show numbers only (226/300 instead of "health 226/300") - default false
    #[serde(default)]
    pub numbers_only: bool,
    /// Show current value only (226 instead of 226/300) - default false
    #[serde(default)]
    pub current_only: bool,
    /// Order of bars to display. Valid values: "health", "mana", "stamina", "spirit"
    /// Default: ["health", "mana", "stamina", "spirit"]
    /// Example: ["health", "stamina", "mana", "spirit"] puts stamina before mana
    #[serde(default = "default_minivitals_bar_order", skip_serializing_if = "is_default_bar_order")]
    pub bar_order: Vec<String>,
    /// Health bar color (default: red)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_color: Option<String>,
    /// Mana bar color (default: blue)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_color: Option<String>,
    /// Stamina bar color (default: yellow)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamina_color: Option<String>,
    /// Spirit bar color (default: magenta)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spirit_color: Option<String>,
    /// Concentration bar color (default: cyan) - DR specific
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concentration_color: Option<String>,
}

/// Betrayer widget data (blood pool progress bar + item list) - GS4 only
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BetrayerWidgetData {
    /// Show item list below progress bar (default: true)
    #[serde(default = "default_true")]
    pub show_items: bool,
    /// Progress bar color (default: dark red #8b0000)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_color: Option<String>,
}

/// Lich WebUI panel data - binds the window to one registered page
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WebUiWidgetData {
    /// Page id, "script/page" (e.g. "creaturebar/main")
    #[serde(default)]
    pub page: String,
}

pub fn default_minivitals_bar_order() -> Vec<String> {
    vec![
        "health".to_string(),
        "mana".to_string(),
        "stamina".to_string(),
        "spirit".to_string(),
    ]
}

fn is_default_bar_order(order: &Vec<String>) -> bool {
    *order == default_minivitals_bar_order()
}


#[cfg(test)]
mod visibility_tests {
    use super::*;

    // A minimal WindowBase TOML: only name (everything else defaults).
    fn parse_base(extra: &str) -> WindowBase {
        let toml = format!("name = \"w\"\n{}", extra);
        toml::from_str(&toml).expect("valid WindowBase toml")
    }

    #[test]
    fn legacy_visible_bool_still_loads() {
        // Existing layout.toml files carry `visible = true|false`.
        assert_eq!(parse_base("visible = true").visibility, WindowVisibility::Shown);
        assert_eq!(parse_base("visible = false").visibility, WindowVisibility::Hidden);
        // Absent → default Shown.
        assert_eq!(parse_base("").visibility, WindowVisibility::Shown);
    }

    #[test]
    fn new_visibility_string_loads_and_roundtrips() {
        assert_eq!(parse_base("visibility = \"hidden\"").visibility, WindowVisibility::Hidden);
        assert_eq!(parse_base("visibility = \"shown\"").visibility, WindowVisibility::Shown);
        // Round-trip through TOML preserves it.
        let mut base = parse_base("");
        base.visibility = WindowVisibility::Hidden;
        let s = toml::to_string(&base).unwrap();
        assert!(s.contains("visibility = \"hidden\""), "serialized: {s}");
        assert_eq!(toml::from_str::<WindowBase>(&s).unwrap().visibility, WindowVisibility::Hidden);
    }

    #[test]
    fn visibility_semantics() {
        assert!(WindowVisibility::Shown.is_shown());
        assert!(WindowVisibility::Ephemeral.is_shown());
        assert!(!WindowVisibility::Hidden.is_shown());
        // Hidden is the ONLY state that blocks the game from auto-spawning.
        assert!(WindowVisibility::Shown.allows_autospawn());
        assert!(WindowVisibility::Ephemeral.allows_autospawn());
        assert!(!WindowVisibility::Hidden.allows_autospawn());
        // Ephemeral is the only non-persistent state.
        assert!(WindowVisibility::Shown.is_persistent());
        assert!(WindowVisibility::Hidden.is_persistent());
        assert!(!WindowVisibility::Ephemeral.is_persistent());
    }

    #[test]
    fn binding_roundtrips_and_is_omitted_when_none() {
        let base = parse_base("binding = { kind = \"dialog\", id = \"expr\" }");
        assert_eq!(base.binding, Some(WindowBinding::Dialog("expr".to_string())));
        assert_eq!(base.binding.as_ref().unwrap().id(), "expr");
        // None binding is skip-serialized (keeps layout.toml clean).
        let none = parse_base("");
        assert!(none.binding.is_none());
        assert!(!toml::to_string(&none).unwrap().contains("binding"));
    }
}
