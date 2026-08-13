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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    // Renamed from `text_color` to avoid a `#[serde(flatten)]` key collision
    // with `WindowBase.text_color` (both flatten into the same JSON/TOML map,
    // which produced a duplicate-key parse error on GUI-layout round-trip).
    // `alias` keeps old configs readable; `skip_serializing_if` guarantees no
    // collision even if a value is set.
    #[serde(
        default,
        alias = "text_color",
        skip_serializing_if = "Option::is_none"
    )]
    pub input_text_color: Option<String>,
    #[serde(default)]
    pub completion_color: Option<String>,
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
    // Renamed from `background_color` to avoid a `#[serde(flatten)]` key
    // collision with `WindowBase.background_color` (both flatten into the same
    // map → duplicate-key parse error on layout round-trip). `alias` keeps old
    // configs readable; `skip_serializing_if` guarantees no collision.
    #[serde(
        default,
        alias = "background_color",
        skip_serializing_if = "Option::is_none"
    )]
    pub countdown_background_color: Option<String>,
    /// Keep the timer visible at rest, showing "label: 0" with an empty bar,
    /// instead of hiding when it reaches zero. Default false (hide on zero).
    #[serde(default)]
    pub show_when_zero: Option<bool>,
    /// Keep counting below zero (-1, -2, ...) after expiry instead of
    /// clamping at 0. For timers whose expiry is a window, not a moment -
    /// the pulse clock reads 0 at the earliest possible arrival and runs
    /// negative through the min..max window (up to ~-29). Default false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_past_zero: Option<bool>,
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

/// The default injury/scar severity palette, indexed by level 0..=6
/// (0 = uninjured, 1-3 injuries brown→orange→red, 4-6 scars light→dark gray).
/// Single source of truth: every frontend resolves through
/// [`InjuryDollWidgetData::resolved_colors`] so the palette can't drift.
pub const DEFAULT_INJURY_PALETTE: [&str; 7] = [
    "#333333", // 0: none
    "#aa5500", // 1: injury 1 (brown)
    "#ff8800", // 2: injury 2 (orange)
    "#ff0000", // 3: injury 3 (bright red)
    "#999999", // 4: scar 1 (light gray)
    "#777777", // 5: scar 2 (medium gray)
    "#555555", // 6: scar 3 (darker gray)
];

/// Injury doll widget specific data
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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

impl InjuryDollWidgetData {
    /// The 7-entry severity palette (level 0..=6) with per-level config
    /// overrides applied over [`DEFAULT_INJURY_PALETTE`]. Every frontend calls
    /// this so the palette and the user's `injury*_color`/`scar*_color`
    /// settings are honored identically (the GUI used to ignore them).
    pub fn resolved_colors(&self) -> [String; 7] {
        let overrides = [
            &self.injury_default_color,
            &self.injury1_color,
            &self.injury2_color,
            &self.injury3_color,
            &self.scar1_color,
            &self.scar2_color,
            &self.scar3_color,
        ];
        std::array::from_fn(|i| {
            overrides[i]
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_INJURY_PALETTE[i].to_string())
        })
    }
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

/// Resolved dashboard layout, parsed from the config `layout` string. Shared
/// so both frontends interpret `dashboard_layout` identically (the config
/// stores the raw string; each renderer parses it through here).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DashboardLayout {
    Horizontal,
    Vertical,
    Grid { rows: usize, cols: usize },
    Flow,
}

impl DashboardLayout {
    /// Parse a `dashboard_layout` string: `horizontal`/`vertical`/`flow`, or
    /// `grid:RxC` (e.g. `grid:2x3`). Anything unrecognized falls back to
    /// horizontal.
    pub fn from_str(value: &str) -> Self {
        let lower = value.to_lowercase();
        if lower.starts_with("grid") {
            if let Some(spec) = lower.split(':').nth(1) {
                let parts: Vec<_> = spec.split('x').collect();
                if parts.len() == 2 {
                    if let (Ok(r), Ok(c)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                        if r > 0 && c > 0 {
                            return DashboardLayout::Grid { rows: r, cols: c };
                        }
                    }
                }
            }
        }
        match lower.as_str() {
            "vertical" => DashboardLayout::Vertical,
            "flow" => DashboardLayout::Flow,
            "horizontal" => DashboardLayout::Horizontal,
            _ => DashboardLayout::Horizontal,
        }
    }
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
    /// Optional layer-stack group. Entries sharing a `stack` name render into
    /// ONE cell, their active icons painted over each other (Wrayth-style:
    /// blood/poison/disease share a square, each PNG authored to sit in a
    /// different part of it so they don't collide). Empty = its own cell.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stack: String,
}

impl DashboardWidgetData {
    /// Number of rendered cells: unstacked entries each count once; entries
    /// sharing a `stack` name collapse into one cell. Used for row-count
    /// height math in both frontends so a stacked dashboard hugs its content.
    pub fn cell_count(&self) -> usize {
        let mut seen: Vec<String> = Vec::new();
        let mut count = 0;
        for def in &self.indicators {
            if def.stack.is_empty() {
                count += 1;
            } else {
                let key = def.stack.to_lowercase();
                if !seen.contains(&key) {
                    seen.push(key);
                    count += 1;
                }
            }
        }
        count
    }
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
    /// Text color override (also overrides link color if set).
    // Renamed from `text_color` to avoid a `#[serde(flatten)]` key collision
    // with `WindowBase.text_color`. `alias` keeps old configs readable.
    #[serde(default, alias = "text_color", skip_serializing_if = "Option::is_none")]
    pub hand_text_color: Option<String>,
    /// Status-driven icon states, first match wins (hotbar-style). A
    /// matched state's icon/text replace the static icon while its
    /// condition holds; no match falls through to the static settings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<HandIconState>,
}

/// One condition-driven hand icon state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandIconState {
    pub when: super::Condition,
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
    pub show_cpu: bool,
    #[serde(default = "default_true")]
    pub show_memory: bool,
    #[serde(default = "default_true")]
    pub show_lines: bool,
    #[serde(default = "default_true")]
    pub show_uptime: bool,
    #[serde(default = "default_true")]
    pub show_spike_log: bool,
    #[serde(default = "default_true")]
    pub show_per_window: bool,
    /// Draw trend sparklines next to rows that have a series.
    #[serde(default = "default_true")]
    pub sparklines: bool,
}

impl Default for PerformanceWidgetData {
    fn default() -> Self {
        Self {
            enabled: true,
            show_fps: true,
            show_render_times: true,
            show_ui_times: true,
            show_wrap_times: true,
            show_net: true,
            show_parse: true,
            show_events: true,
            show_cpu: true,
            show_memory: true,
            show_lines: true,
            show_uptime: true,
            show_spike_log: true,
            show_per_window: true,
            sparklines: true,
        }
    }
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

/// Missing-spells window: no per-widget fields — the watch list lives in
/// per-character state (.spellwatch) and the display derives from the
/// live effect feeds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MissingSpellsWidgetData {}

/// Containers tree widget (managed-inventory snapshot) - no options yet
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContainersWidgetData {}

/// Multi-account cards: which rows each character's card shows.
///
/// Every element is opt-in per field rather than a fixed card template,
/// because what matters differs by playstyle -- a healer wants dolls, a
/// caster wants mind state. Defaults are the four that answer "is this
/// character in trouble right now": vitals, RT, status glyphs and the group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiAccountWidgetData {
    /// Health/mana/stamina/spirit bars.
    pub show_vitals: bool,
    /// Roundtime and casttime, interpolated from the peer's own clock.
    pub show_rt: bool,
    /// Compact colored glyph row (stunned, bleeding, hidden, ...).
    pub show_status: bool,
    /// Injury doll per card, drawn with YOUR installed doll art and the
    /// peer's reported wounds. Peers ship an injuries map, not art, so a
    /// character using custom doll art shows on yours -- sending each peer's
    /// variant and calibration is a later addition.
    pub show_injuries: bool,
    /// Mind state bar.
    pub show_mind: bool,
    /// Combat stance bar.
    pub show_stance: bool,
    /// Unabsorbed field experience, with a warning when at or near the cap --
    /// the point of watching it is knowing when to go absorb.
    pub show_field_exp: bool,
    /// Encumbrance bar.
    pub show_encumbrance: bool,
    /// Room name, and the "not with you" cue when it differs from yours.
    pub show_room: bool,
    /// Include a card for this character, sorted first and accent-marked.
    /// On by default: without it a group of three shows only two cards, and
    /// the others have nothing to be compared against.
    pub show_self: bool,
    /// Show absolute vitals ("51/51") instead of percentages, where the peer
    /// has reported them. Percentages remain the fallback.
    pub show_absolute_vitals: bool,
    /// Hands, and the spell being prepared.
    pub show_hands: bool,
    /// Debuffs and cooldowns, filtered by `effect_filter`.
    pub show_effects: bool,
    /// Which effect categories to draw, in order. Defaults to the two that
    /// answer "is this character in trouble" -- active spells and buffs are
    /// long lists that would bury the card.
    pub effect_categories: Vec<String>,
    /// Case-insensitive substrings; an effect shows only if its name contains
    /// one of these. Empty means show everything in the chosen categories.
    ///
    /// A filter rather than a fixed list because effect names vary by
    /// profession and society, and a hardcoded set would be wrong for
    /// somebody. Empty-means-all keeps the first run useful: you see
    /// everything, then narrow it.
    pub effect_filter: Vec<String>,
    /// Cap on effects drawn per card, after filtering. Six characters with
    /// unbounded lists is unreadable regardless of the filter.
    pub max_effects: usize,
    /// Rows drawn on the same line as the row above them, by row id.
    ///
    /// Short rows waste a full line each: RT is one label and the status
    /// icons are a compact strip, so they pair naturally. Defaults to pairing
    /// those two, which is the combination every card wants.
    pub merged_rows: Vec<String>,
    /// Row order within a card, top to bottom. Names match the toggles:
    /// "status", "vitals", "rt", "hands", "effects", "mind", "stance",
    /// "field_exp", "encumbrance", "injuries". The room id is not a row --
    /// it renders in the card header, gated by `show_room`.
    ///
    /// Rows not listed are appended in their default order, so an old config
    /// (or a partial list) still shows everything -- a missing name hides
    /// nothing, it just does not reposition it.
    pub row_order: Vec<String>,
    /// Relative widths of the card's row columns; the LENGTH is the column
    /// count. `[1.0]` (default) is the classic single vertical panel;
    /// `[1.0, 1.4]` is a doll column with a wider info column beside it.
    /// Same idea as the window Group system's size weights.
    pub card_column_weights: Vec<f32>,
    /// Row id -> column index (0-based). Rows not listed sit in column 0;
    /// indices past the last column clamp to it, so shrinking the column
    /// count never hides a row.
    pub card_row_columns: std::collections::BTreeMap<String, usize>,
    /// Card order: "group" keeps clustered characters together (default),
    /// "name" sorts alphabetically, "port" is connection order.
    pub sort_by: String,
    /// Cards per row before wrapping. 0 means fit as many as the window is
    /// wide enough for.
    pub columns: usize,
    /// Card width in points.
    pub card_width: f32,
}

impl Default for MultiAccountWidgetData {
    fn default() -> Self {
        Self {
            show_vitals: true,
            show_rt: true,
            show_status: true,
            show_injuries: true,
            show_mind: false,
            show_stance: false,
            show_field_exp: false,
            merged_rows: vec!["status".to_string()],
            row_order: Vec::new(),
            sort_by: "group".to_string(),
            show_encumbrance: false,
            show_room: true,
            show_self: true,
            show_absolute_vitals: true,
            show_hands: false,
            show_effects: true,
            effect_categories: vec!["Debuffs".to_string(), "Cooldowns".to_string()],
            effect_filter: Vec::new(),
            max_effects: 4,
            columns: 0,
            card_width: 150.0,
            card_column_weights: vec![1.0],
            card_row_columns: std::collections::BTreeMap::new(),
        }
    }
}



/// One row of a multi-account card, as a real type.
///
/// Rows used to be bare strings matched in SIX parallel tables across three
/// files (order list, label, shown, set_shown, stretches, render arm), and
/// five of the six failed silently on a missed arm -- a checkbox that
/// toggled nothing, a row that never drew. The enum makes every table an
/// exhaustive match the compiler enforces; the TOML representation stays the
/// same strings via `id`/`from_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardRow {
    Rt,
    Status,
    Vitals,
    Hands,
    Effects,
    Mind,
    Stance,
    FieldExp,
    Encumbrance,
    Injuries,
}

impl CardRow {
    /// Default top-to-bottom order. Roundtime leads -- the most time-critical
    /// value on the card -- with the status icons sharing its line.
    pub const ALL: [CardRow; 10] = [
        CardRow::Rt,
        CardRow::Status,
        CardRow::Vitals,
        CardRow::Hands,
        CardRow::Effects,
        CardRow::Mind,
        CardRow::Stance,
        CardRow::FieldExp,
        CardRow::Encumbrance,
        CardRow::Injuries,
    ];

    /// The TOML/config string for this row (row_order, merged_rows).
    pub fn id(self) -> &'static str {
        match self {
            CardRow::Rt => "rt",
            CardRow::Status => "status",
            CardRow::Vitals => "vitals",
            CardRow::Hands => "hands",
            CardRow::Effects => "effects",
            CardRow::Mind => "mind",
            CardRow::Stance => "stance",
            CardRow::FieldExp => "field_exp",
            CardRow::Encumbrance => "encumbrance",
            CardRow::Injuries => "injuries",
        }
    }

    /// Parse a config string; unknown (stale) names yield None and are
    /// dropped rather than rendering as phantom rows.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|row| row.id() == id)
    }

    /// Human label for the editor list.
    pub fn label(self) -> &'static str {
        match self {
            CardRow::Rt => "Roundtime",
            CardRow::Status => "Status icons",
            CardRow::Vitals => "Vitals",
            CardRow::Hands => "Hands / casting",
            CardRow::Effects => "Debuffs & cooldowns",
            CardRow::Mind => "Mind state",
            CardRow::Stance => "Stance",
            CardRow::FieldExp => "Field experience",
            CardRow::Encumbrance => "Encumbrance",
            CardRow::Injuries => "Injury doll",
        }
    }

    /// Whether this is one of the BIG rows -- the vitals block and the
    /// injury doll. A big row never squeezes onto a compact horizontal
    /// strip (an RT label's line); the doll instead opens a side column
    /// (see `may_join`).
    pub fn full_width(self) -> bool {
        matches!(self, CardRow::Vitals | CardRow::Injuries)
    }

    /// Whether this row may share the given horizontal line: compact rows
    /// only. Big rows (vitals, doll) never squeeze onto a strip -- putting
    /// the doll BESIDE other rows is what card columns are for
    /// (`card_column_weights` / `card_row_columns`), where each column
    /// stacks vertically at a proper width.
    pub fn may_join(self, line: &[CardRow]) -> bool {
        !self.full_width() && line.iter().all(|other| !other.full_width())
    }

    /// Whether the row fills the width it is given (bars and the doll) or
    /// sizes to its own content (labels, icon strips). Decides width shares
    /// when rows share a line.
    pub fn stretches(self) -> bool {
        match self {
            CardRow::Vitals
            | CardRow::Mind
            | CardRow::Stance
            | CardRow::FieldExp
            | CardRow::Encumbrance
            | CardRow::Injuries => true,
            CardRow::Rt | CardRow::Status | CardRow::Hands | CardRow::Effects => false,
        }
    }
}

impl MultiAccountWidgetData {
    /// Rows in display order, paired with whether each is shown. Rows missing
    /// from `row_order` keep their default position, so a partial list never
    /// hides anything -- the checkbox is what hides.
    pub fn ordered_rows(&self) -> Vec<(CardRow, bool)> {
        let mut order: Vec<CardRow> = self
            .row_order
            .iter()
            .filter_map(|name| CardRow::from_id(name))
            .collect();
        for row in CardRow::ALL {
            if !order.contains(&row) {
                order.push(row);
            }
        }
        order
            .into_iter()
            .map(|row| (row, self.row_shown(row)))
            .collect()
    }

    /// Whether a row is currently enabled. Exhaustive: a new row cannot ship
    /// with a checkbox that silently toggles nothing.
    pub fn row_shown(&self, row: CardRow) -> bool {
        match row {
            CardRow::Status => self.show_status,
            CardRow::Vitals => self.show_vitals,
            CardRow::Rt => self.show_rt,
            CardRow::Hands => self.show_hands,
            CardRow::Effects => self.show_effects,
            CardRow::Mind => self.show_mind,
            CardRow::Stance => self.show_stance,
            CardRow::FieldExp => self.show_field_exp,
            CardRow::Encumbrance => self.show_encumbrance,
            CardRow::Injuries => self.show_injuries,
        }
    }

    /// Enable or disable a row.
    pub fn set_row_shown(&mut self, row: CardRow, on: bool) {
        match row {
            CardRow::Status => self.show_status = on,
            CardRow::Vitals => self.show_vitals = on,
            CardRow::Rt => self.show_rt = on,
            CardRow::Hands => self.show_hands = on,
            CardRow::Effects => self.show_effects = on,
            CardRow::Mind => self.show_mind = on,
            CardRow::Stance => self.show_stance = on,
            CardRow::FieldExp => self.show_field_exp = on,
            CardRow::Encumbrance => self.show_encumbrance = on,
            CardRow::Injuries => self.show_injuries = on,
        }
    }

    /// Raw membership in the merge set -- what the config STORES. The editor
    /// reads and writes this; only the renderer applies the positional "first
    /// row cannot merge" rule. Conflating the two was a data-loss bug: the
    /// view snapshot read the positional answer (false for whatever row was
    /// first) and wrote it back on any unrelated edit, deleting the stored
    /// flag.
    pub fn row_merge_flag(&self, row: CardRow) -> bool {
        self.merged_rows.iter().any(|r| r == row.id())
    }

    /// Whether this row RENDERS on the line above it: stored flag, unless the
    /// row is first (nothing above it to join).
    pub fn row_merged(&self, row: CardRow) -> bool {
        if self
            .ordered_rows()
            .first()
            .is_some_and(|(first, _)| *first == row)
        {
            return false;
        }
        self.row_merge_flag(row)
    }

    /// Set or clear a row's merge-with-above flag.
    pub fn set_row_merged(&mut self, row: CardRow, merged: bool) {
        let present = self.merged_rows.iter().position(|r| r == row.id());
        match (merged, present) {
            (true, None) => self.merged_rows.push(row.id().to_string()),
            (false, Some(idx)) => {
                self.merged_rows.remove(idx);
            }
            _ => {}
        }
    }

    /// Column weights sanitized for layout: at least one column, every
    /// weight positive. Garbage in the config degrades to equal columns
    /// rather than a zero-width or vanished one.
    pub fn column_weights(&self) -> Vec<f32> {
        let mut weights: Vec<f32> = self
            .card_column_weights
            .iter()
            .map(|w| if w.is_finite() && *w > 0.0 { *w } else { 1.0 })
            .collect();
        if weights.is_empty() {
            weights.push(1.0);
        }
        weights
    }

    /// Which column a row renders in, clamped to the columns that exist --
    /// an assignment to a removed column lands in the last one instead of
    /// hiding the row.
    pub fn row_column(&self, row: CardRow) -> usize {
        let last = self.column_weights().len() - 1;
        self.card_row_columns
            .get(row.id())
            .copied()
            .unwrap_or(0)
            .min(last)
    }

    /// Assign a row to a column. Column 0 is the default, so assigning it
    /// removes the entry rather than storing a redundant zero.
    pub fn set_row_column(&mut self, row: CardRow, column: usize) {
        if column == 0 {
            self.card_row_columns.remove(row.id());
        } else {
            self.card_row_columns.insert(row.id().to_string(), column);
        }
    }

    /// One column's rows grouped into lines: each inner vec is one
    /// horizontal run. Order within the column follows `row_order`.
    ///
    /// Hidden rows are dropped BEFORE grouping, so hiding the row a merged
    /// row was attached to promotes it to its own line rather than leaving a
    /// dangling continuation. The same applies to rows moved to another
    /// column: "share line with the row above" chains only within a column.
    pub fn row_lines(&self, column: usize) -> Vec<Vec<CardRow>> {
        let mut lines: Vec<Vec<CardRow>> = Vec::new();
        for (row, shown) in self.ordered_rows() {
            if !shown || self.row_column(row) != column {
                continue;
            }
            // A merged row joins the line above only when `may_join` allows
            // it. This is the data-level guard: a config asking vitals to
            // join the RT strip self-heals to its own line instead of
            // rendering crushed.
            let joinable = self.row_merged(row)
                && lines.last().is_some_and(|line| row.may_join(line));
            if joinable {
                lines.last_mut().expect("non-empty").push(row);
            } else {
                lines.push(vec![row]);
            }
        }
        lines
    }

    /// Move a row one place up or down, materializing the full order first so
    /// a previously-empty `row_order` becomes explicit rather than shifting
    /// against an implied list.
    pub fn move_row(&mut self, row: CardRow, up: bool) {
        let mut order: Vec<CardRow> = self.ordered_rows().into_iter().map(|(r, _)| r).collect();
        let Some(idx) = order.iter().position(|r| *r == row) else {
            return;
        };
        let target = if up {
            idx.checked_sub(1)
        } else if idx + 1 < order.len() {
            Some(idx + 1)
        } else {
            None
        };
        if let Some(target) = target {
            order.swap(idx, target);
            self.row_order = order.into_iter().map(|r| r.id().to_string()).collect();
        }
    }
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
    /// Background color for unfilled cells inside each vital bar.
    /// When unset, the window background or terminal default is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depleted_color: Option<String>,
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
mod tests {
    use super::{CommandInputWidgetData, MiniVitalsWidgetData};

    #[test]
    fn command_input_completion_color_round_trips() {
        let data: CommandInputWidgetData =
            toml::from_str("completion_color = \"#6b7280\"").unwrap();
        assert_eq!(data.completion_color.as_deref(), Some("#6b7280"));

        let serialized = toml::to_string(&data).unwrap();
        let round_trip: CommandInputWidgetData = toml::from_str(&serialized).unwrap();
        assert_eq!(round_trip, data);
    }

    #[test]
    fn minivitals_depleted_color_defaults_to_none() {
        let data: MiniVitalsWidgetData = toml::from_str("numbers_only = false").unwrap();

        assert_eq!(data.depleted_color, None);
    }

    #[test]
    fn minivitals_depleted_color_round_trips_and_none_is_omitted() {
        let data: MiniVitalsWidgetData =
            toml::from_str("depleted_color = \"#202020\"").unwrap();
        assert_eq!(data.depleted_color.as_deref(), Some("#202020"));

        let serialized = toml::to_string(&MiniVitalsWidgetData::default()).unwrap();
        assert!(!serialized.contains("depleted_color"));
    }
}

#[cfg(test)]
mod dashboard_layout_tests {
    use super::*;

    #[test]
    fn parses_named_layouts() {
        assert_eq!(DashboardLayout::from_str("horizontal"), DashboardLayout::Horizontal);
        assert_eq!(DashboardLayout::from_str("VERTICAL"), DashboardLayout::Vertical);
        assert_eq!(DashboardLayout::from_str("Flow"), DashboardLayout::Flow);
    }

    #[test]
    fn parses_grid_spec() {
        assert_eq!(
            DashboardLayout::from_str("grid:2x3"),
            DashboardLayout::Grid { rows: 2, cols: 3 }
        );
    }

    #[test]
    fn unrecognized_and_bad_grid_fall_back_to_horizontal() {
        assert_eq!(DashboardLayout::from_str("nonsense"), DashboardLayout::Horizontal);
        assert_eq!(DashboardLayout::from_str("grid:0x3"), DashboardLayout::Horizontal);
        assert_eq!(DashboardLayout::from_str("grid:2"), DashboardLayout::Horizontal);
    }

    #[test]
    fn cell_count_collapses_stack_groups() {
        let ind = |id: &str, stack: &str| DashboardIndicatorDef {
            id: id.to_string(),
            icon: String::new(),
            colors: Vec::new(),
            stack: stack.to_string(),
        };
        let data = DashboardWidgetData {
            layout: "grid:2x3".to_string(),
            spacing: 0,
            hide_inactive: true,
            indicators: vec![
                ind("BLEEDING", "affliction"),
                ind("POISONED", "affliction"),
                ind("DISEASED", "affliction"),
                ind("STUNNED", ""),
                ind("WEBBED", ""),
            ],
        };
        // Three afflictions collapse to one cell; two singletons = 3 cells.
        assert_eq!(data.cell_count(), 3);
    }
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

#[cfg(test)]
mod multiaccount_row_tests {
    use super::*;
    use CardRow as R;

    #[test]
    fn an_empty_order_yields_every_row_in_default_order() {
        let data = MultiAccountWidgetData::default();
        let rows: Vec<R> = data.ordered_rows().into_iter().map(|(r, _)| r).collect();
        assert_eq!(rows, R::ALL);
    }

    #[test]
    fn a_partial_order_keeps_unlisted_rows() {
        // Omitting a row must not hide it -- the checkbox is what hides.
        let mut data = MultiAccountWidgetData::default();
        data.row_order = vec!["injuries".to_string(), "vitals".to_string()];
        let rows: Vec<R> = data.ordered_rows().into_iter().map(|(r, _)| r).collect();
        assert_eq!(&rows[..2], &[R::Injuries, R::Vitals]);
        assert_eq!(rows.len(), R::ALL.len(), "every row still present: {rows:?}");
    }

    #[test]
    fn unknown_row_names_are_dropped() {
        // A stale config naming a row that no longer exists must not add a
        // phantom entry the editor would render blank.
        let mut data = MultiAccountWidgetData::default();
        data.row_order = vec!["nonsense".to_string(), "injuries".to_string()];
        let rows: Vec<R> = data.ordered_rows().into_iter().map(|(r, _)| r).collect();
        assert_eq!(rows[0], R::Injuries);
        assert_eq!(rows.len(), R::ALL.len());
    }

    #[test]
    fn every_row_round_trips_through_its_id() {
        // The wire/TOML representation stays strings; the enum must map onto
        // them losslessly or a saved order comes back rearranged.
        for row in R::ALL {
            assert_eq!(R::from_id(row.id()), Some(row));
        }
        assert_eq!(R::from_id("not_a_row"), None);
    }

    #[test]
    fn moving_a_row_materializes_the_full_order() {
        // Starting from an empty (implied) order, one move must write the
        // whole list, or later moves would shift against a different list.
        let mut data = MultiAccountWidgetData::default();
        assert!(data.row_order.is_empty());
        let idx = R::ALL.iter().position(|r| *r == R::Vitals).expect("vitals");
        let above = R::ALL[idx - 1];

        data.move_row(R::Vitals, true);
        assert_eq!(data.row_order.len(), R::ALL.len());
        assert_eq!(data.row_order[idx - 1], R::Vitals.id(), "moved up one");
        assert_eq!(data.row_order[idx], above.id(), "displaced its neighbour");
    }

    #[test]
    fn moving_past_an_edge_is_a_no_op() {
        let mut data = MultiAccountWidgetData::default();
        data.move_row(R::Rt, true);
        let first: Vec<R> = data.ordered_rows().into_iter().map(|(r, _)| r).collect();
        assert_eq!(first[0], R::Rt, "already first, stays first");

        data.move_row(R::Injuries, false);
        let last: Vec<R> = data.ordered_rows().into_iter().map(|(r, _)| r).collect();
        assert_eq!(last[last.len() - 1], R::Injuries, "already last, stays last");
    }

    #[test]
    fn rt_shares_a_line_with_status_by_default() {
        // Both are short -- one label and a strip of icons -- so a full line
        // each is wasted space on an already narrow card.
        let data = MultiAccountWidgetData::default();
        let lines = data.row_lines(0);
        assert_eq!(lines[0], vec![R::Rt, R::Status], "{lines:?}");
    }

    #[test]
    fn hiding_the_row_above_promotes_a_merged_row_to_its_own_line() {
        // Otherwise "status" would dangle as a continuation of a line that
        // is no longer drawn.
        let mut data = MultiAccountWidgetData::default();
        data.set_row_shown(R::Rt, false);
        let lines = data.row_lines(0);
        assert_eq!(lines[0], vec![R::Status], "{lines:?}");
    }

    #[test]
    fn the_first_row_never_renders_merged_but_keeps_its_flag() {
        // Positional rule for RENDERING only. The stored flag must survive a
        // stint at the top -- the old positional read-back deleted it when
        // any unrelated option changed while the row sat first.
        let mut data = MultiAccountWidgetData::default();
        data.set_row_merged(R::Status, true);
        data.row_order = vec!["status".to_string()];
        assert!(!data.row_merged(R::Status), "first row cannot render merged");
        assert!(
            data.row_merge_flag(R::Status),
            "the stored flag survives being first"
        );
        // Move it back down: the pairing resumes without re-configuring.
        data.row_order = vec!["rt".to_string(), "status".to_string()];
        assert!(data.row_merged(R::Status));
    }

    #[test]
    fn merging_round_trips_and_hidden_rows_never_appear() {
        let mut data = MultiAccountWidgetData::default();
        data.set_row_merged(R::Mind, true);
        assert!(data.row_merged(R::Mind));
        data.set_row_merged(R::Mind, false);
        assert!(!data.row_merged(R::Mind));

        data.set_row_shown(R::Injuries, false);
        let flat: Vec<R> = data.row_lines(0).into_iter().flatten().collect();
        assert!(!flat.contains(&R::Injuries));
    }

    /// Big rows (vitals, doll) never squeeze onto a shared horizontal
    /// strip -- putting things beside them is what card columns are for.
    #[test]
    fn big_rows_never_join_a_shared_line() {
        // Vitals asked to join a compact line self-heals to its own.
        let mut data = MultiAccountWidgetData::default();
        data.set_row_merged(R::Vitals, true); // above it: rt + status
        let lines = data.row_lines(0);
        assert!(
            lines.iter().any(|line| line == &vec![R::Vitals]),
            "vitals must not join the RT line: {lines:?}"
        );

        // Same for the doll, even directly under the vitals.
        data.row_order = vec!["vitals".to_string(), "injuries".to_string()];
        data.set_row_merged(R::Injuries, true);
        let lines = data.row_lines(0);
        assert!(
            lines.iter().any(|line| line == &vec![R::Injuries]),
            "the doll always gets its own line: {lines:?}"
        );

        // Compact rows still mix freely.
        data.set_row_shown(R::Mind, true);
        data.set_row_shown(R::Stance, true);
        data.set_row_merged(R::Stance, true);
        assert!(data.row_merged(R::Stance));
    }

    /// Card columns: rows land in their assigned column, assignments to a
    /// removed column clamp to the last one, and weights are sanitized.
    #[test]
    fn rows_split_across_card_columns() {
        let mut data = MultiAccountWidgetData::default();
        data.card_column_weights = vec![1.0, 1.4];
        data.set_row_column(R::Injuries, 1);
        data.set_row_column(R::Vitals, 1);

        let col0: Vec<R> = data.row_lines(0).into_iter().flatten().collect();
        let col1: Vec<R> = data.row_lines(1).into_iter().flatten().collect();
        assert!(!col0.contains(&R::Injuries) && !col0.contains(&R::Vitals));
        assert_eq!(col1, vec![R::Vitals, R::Injuries], "column order follows row_order");

        // Line sharing chains only within a column: rt+status stay paired
        // in column 0 regardless of what moved to column 1.
        assert_eq!(data.row_lines(0)[0], vec![R::Rt, R::Status]);

        // Shrinking to one column strands no rows -- assignments clamp.
        data.card_column_weights = vec![1.0];
        let all: Vec<R> = data.row_lines(0).into_iter().flatten().collect();
        assert!(all.contains(&R::Injuries) && all.contains(&R::Vitals));

        // Garbage weights degrade to usable columns instead of vanishing.
        data.card_column_weights = vec![0.0, f32::NAN, -2.0];
        assert_eq!(data.column_weights(), vec![1.0, 1.0, 1.0]);
        data.card_column_weights = Vec::new();
        assert_eq!(data.column_weights(), vec![1.0]);
    }

    /// Column assignment round-trips, and column 0 is stored implicitly.
    #[test]
    fn row_column_round_trips_and_zero_is_implicit() {
        let mut data = MultiAccountWidgetData::default();
        data.card_column_weights = vec![1.0, 1.0];
        assert_eq!(data.row_column(R::Mind), 0, "unlisted rows sit in column 0");
        data.set_row_column(R::Mind, 1);
        assert_eq!(data.row_column(R::Mind), 1);
        data.set_row_column(R::Mind, 0);
        assert_eq!(data.row_column(R::Mind), 0);
        assert!(
            data.card_row_columns.is_empty(),
            "column 0 stores no entry: {:?}",
            data.card_row_columns
        );
    }

    #[test]
    fn row_visibility_round_trips_through_the_helpers() {
        let mut data = MultiAccountWidgetData::default();
        assert!(data.row_shown(R::Vitals));
        data.set_row_shown(R::Vitals, false);
        assert!(!data.row_shown(R::Vitals));
        assert!(!data.show_vitals, "the helper writes the real field");
    }
}
