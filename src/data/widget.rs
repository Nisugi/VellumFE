//! Widget data structures - State for all widget types
//!
//! These are pure data structures with NO rendering logic.
//! Frontends read from these to render appropriately.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::config::TimestampPosition;

/// Format-agnostic reference to icon art, resolved by frontends through
/// the active skin/pool art tables. What a status stores: the picker can
/// offer standalone pool images and hotbar-style sheet cells alike.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconRef {
    /// Resolve by the indicator's own id: skin `[icons]`, then the active
    /// statusicons pool set, then the built-in vector pictogram.
    #[default]
    Default,
    /// Explicitly no art: suppress skin/pool icons for this id so the
    /// widget renders its artless fallback (vector pictogram, text).
    None,
    /// Explicit image: pool-relative ("statusicons/runic_stunned.png") or
    /// absolute path.
    Image { path: String },
    /// One cell of a hotbar-style icon sheet (1-based, barbar order).
    SheetCell { sheet: String, cell: u32 },
}

/// Styled text content for text-based widgets
#[derive(Clone, Debug)]
pub struct TextContent {
    /// Wrapped lines ready for display
    pub lines: VecDeque<StyledLine>,
    /// Scroll offset from bottom (0 = live view, showing newest)
    pub scroll_offset: usize,
    /// Maximum lines to keep in buffer
    pub max_lines: usize,
    /// Title for the window
    pub title: String,
    /// Generation counter - increments on every add_line call
    /// Used to detect changes even when line count stays constant (at max_lines)
    pub generation: u64,
    /// Stream IDs this window listens to (e.g., ["thoughts"], ["main"], ["combat"])
    /// Used for routing incoming game text to the correct window
    pub streams: Vec<String>,
    /// Enable compact display mode (transforms verbose bounty text to 1-4 lines)
    pub compact: bool,
    /// Render per-line arrival timestamps
    pub show_timestamps: bool,
    /// Where the timestamp goes on the line (start or end)
    pub timestamp_position: TimestampPosition,
}

/// A single display line with styled segments
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StyledLine {
    pub segments: Vec<TextSegment>,
    /// The stream this line originated from (e.g., "death", "thoughts", "main")
    /// Used for stream-filtered highlights
    pub stream: String,
    /// Arrival time (unix seconds), stamped when the line enters a text
    /// buffer. Rendered when a window enables timestamps; None on lines
    /// recorded before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

/// A segment of text with styling
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct TextSegment {
    #[serde(default)]
    pub text: String,
    pub fg: Option<String>, // Hex color "#RRGGBB"
    pub bg: Option<String>, // Hex color "#RRGGBB"
    #[serde(default)]
    pub bold: bool,
    /// Render in monospace font (for GUI dual-font rendering)
    /// TUI ignores this field since terminal uses monospace by default.
    #[serde(default)]
    pub mono: bool,
    #[serde(default)]
    pub span_type: SpanType, // Semantic type for priority layering
    pub link_data: Option<LinkData>,
    /// Custom-emoji shortcode name (without the surrounding colons) when this
    /// segment's `text` is a resolved custom emoji like `:VibeCat:`.
    ///
    /// Custom emoji have no Unicode codepoint, so the resolver keeps the
    /// literal `:name:` in `text` (the universal fallback) and tags the
    /// segment here. Frontends that can render images (GUI from disk, web via
    /// `<img>`) swap the run for the emoji's picture; the TUI just shows the
    /// `:name:` text. `None` for ordinary text and gemoji (which resolve to a
    /// real Unicode glyph in `text`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_emoji: Option<String>,
    /// Inline image this segment stands in for, from a `<vellumImg>` tag.
    ///
    /// Like [`Self::custom_emoji`], the segment's `text` stays a readable
    /// fallback (`[img:name]`) so the TUI and any unresolved case show
    /// something rather than a blank. Unlike custom emoji, the image carries
    /// its own size and float alignment, and frontends lay text out *beside*
    /// it rather than inside one text row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_image: Option<InlineImage>,
}

/// Which side of the text an inline image floats on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FloatAlign {
    #[default]
    Left,
    Right,
}

/// An image floated into a text window by `<vellumImg src=.. rows=.. align=..>`.
///
/// `rows` is the *requested* height in text rows; the renderer clamps it to
/// the window's own visible row count (and a configured ceiling) and scales
/// the image to fit with its aspect ratio preserved, so a script that asks
/// for more than fits gets a smaller image rather than a broken window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InlineImage {
    /// Pool image name, validated to the shortcode alphabet. Never a path:
    /// the frontend resolves it through the image registry, so a feed can
    /// name art but can never read an arbitrary file.
    pub name: String,
    /// Requested height in text rows, before clamping.
    pub rows: f32,
    #[serde(default)]
    pub align: FloatAlign,
}

/// Default ceiling on an inline image's height, in text rows. The window's
/// own visible row count clamps further; this only bounds a feed that asks
/// for something absurd in a very tall window.
pub const INLINE_IMAGE_MAX_ROWS: f32 = 8.0;

/// A float narrower than this fraction of the window is not worth wrapping
/// text beside — below it the renderer drops the float and puts the image on
/// its own rows, so text never degrades into a one-word-per-line column.
pub const INLINE_IMAGE_MIN_TEXT_FRACTION: f32 = 0.45;

impl InlineImage {
    /// Final on-screen size for this image, in points.
    ///
    /// Height is `rows` clamped by both the configured ceiling and the
    /// window's own visible height, so a script asking for 40 rows in a
    /// 6-row window gets a 6-row image rather than a broken window. Width
    /// follows from the texture's aspect, then a width clamp scales BOTH
    /// dimensions down if the image would crowd out the text — a wide image
    /// can overflow horizontally even at a legal row count.
    ///
    /// `natural` is the texture's pixel size; a degenerate one falls back to
    /// square so a corrupt file cannot produce a zero or infinite rect.
    pub fn fitted_size(
        &self,
        natural: (f32, f32),
        row_height: f32,
        available_width: f32,
        available_height: f32,
        max_rows: f32,
    ) -> (f32, f32) {
        let window_rows = if row_height > 0.0 {
            (available_height / row_height).floor().max(1.0)
        } else {
            1.0
        };
        let rows = self.rows.max(0.0).min(max_rows).min(window_rows).max(1.0);
        let mut height = rows * row_height;

        let (nat_w, nat_h) = natural;
        let aspect = if nat_w > 0.0 && nat_h > 0.0 {
            nat_w / nat_h
        } else {
            1.0
        };
        let mut width = height * aspect;

        // Width clamp: leave at least MIN_TEXT_FRACTION of the width for text.
        let max_width = available_width * (1.0 - INLINE_IMAGE_MIN_TEXT_FRACTION);
        if max_width > 0.0 && width > max_width {
            let scale = max_width / width;
            width = max_width;
            height *= scale;
        }
        (width.max(1.0), height.max(1.0))
    }

    /// True when the float should collapse to its own rows instead of having
    /// text wrapped beside it, because the remaining text column would be too
    /// narrow to read.
    pub fn should_collapse(image_width: f32, available_width: f32) -> bool {
        available_width - image_width < available_width * INLINE_IMAGE_MIN_TEXT_FRACTION
    }
}

impl TextSegment {
    /// Create a plain text segment with no styling.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }

    /// Create a styled text segment.
    pub fn styled(text: impl Into<String>, fg: Option<String>, bold: bool) -> Self {
        Self {
            text: text.into(),
            fg,
            bold,
            ..Default::default()
        }
    }

    /// Create a text segment with full styling options.
    pub fn with_style(
        text: impl Into<String>,
        fg: Option<String>,
        bg: Option<String>,
        bold: bool,
        mono: bool,
        span_type: SpanType,
    ) -> Self {
        Self {
            text: text.into(),
            fg,
            bg,
            bold,
            mono,
            span_type,
            link_data: None,
            custom_emoji: None,
            inline_image: None,
        }
    }
}

/// Semantic type of text span (for highlight priority)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpanType {
    #[default]
    Normal, // Regular text
    Link,        // <a> tag from parser (clickable game objects)
    Monsterbold, // <preset id="monsterbold"> from parser (monsters)
    Spell,       // <spell> tag from parser (spells)
    Speech,      // <preset id="speech"> from parser (player speech)
    PlayerTitle, // Non-clickable title prefix adjacent to a linked player name
    System,      // Client/system messages; skip highlight transforms
}

/// Link metadata for clickable text
///
/// Two sentinel `exist_id` values ride this struct instead of extra fields:
/// [`DIRECT_LINK_SENTINEL`] (`<d>` tags — `noun`/`text` is a game command)
/// and [`URL_LINK_SENTINEL`] (`<a href>` web links — `noun` is an http(s)
/// URL each frontend opens on its own side: browser on desktop,
/// `window.open` on the phone).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkData {
    pub exist_id: String,
    pub noun: String,
    pub text: String,
    pub coord: Option<String>, // Optional coord for direct commands (e.g., "2524,1864" for movement)
}

/// `exist_id` marker: the link is a `<d>` direct command.
pub const DIRECT_LINK_SENTINEL: &str = "_direct_";

/// `exist_id` marker: the link is a web URL (carried in `noun`).
pub const URL_LINK_SENTINEL: &str = "_url_";

/// Only URLs a browser should ever be handed: plain http(s). Blocks
/// `javascript:`, `file:`, custom schemes, and relative junk regardless of
/// what the game (or a Lich script) injects.
pub fn is_web_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Quickbar entry data (links, menu links, separators)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuickbarEntry {
    Label {
        id: String,
        value: String,
    },
    Link {
        id: String,
        value: String,
        cmd: String,
        echo: Option<String>,
    },
    MenuLink {
        id: String,
        value: String,
        exist: String,
        noun: String,
    },
    Separator,
}

/// Quickbar data for a single quickbar id
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickbarData {
    pub id: String,
    pub title: Option<String>,
    pub entries: Vec<QuickbarEntry>,
}

/// Progress bar state
#[derive(Clone, Debug)]
pub struct ProgressData {
    pub value: u32,            // Current value (actual value, not percentage)
    pub max: u32,              // Maximum value (actual max, not percentage)
    pub label: String,         // Display label
    pub color: Option<String>, // Hex color override (or custom text like "clear as a bell")
    pub progress_id: String,   // Feed id (XML progressBar id), case-sensitive
    pub numbers_only: bool,    // Show "value/max" instead of the label
    pub current_only: bool,    // Show only the current value
}

/// Countdown timer state
#[derive(Clone, Debug)]
pub struct CountdownData {
    pub end_time: i64,         // Unix timestamp when timer expires
    pub label: String,         // Display label
    pub countdown_id: String,  // Feed id (XML event id), case-sensitive
    pub color: Option<String>, // Fill color override; None = id-based default
    pub show_when_zero: bool,  // Keep visible at rest as "label: 0"; else hide
    pub count_past_zero: bool, // Run negative after expiry (window timers like pulse)
}

/// Compass directions
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompassData {
    pub directions: Vec<String>, // Available exits: "n", "s", "e", "w", etc.
}

/// Mini map view state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapData {
    /// Pixels per grid cell.
    pub zoom: f32,
}

impl Default for MapData {
    fn default() -> Self {
        MapData { zoom: 16.0 }
    }
}

/// Injury doll state
#[derive(Clone, Debug)]
pub struct InjuryDollData {
    pub injuries: std::collections::HashMap<String, u8>, // body_part -> level (0-6)
                                                         // Injury levels: 0=none, 1-3=injury levels, 4-6=scar levels
}

impl InjuryDollData {
    pub fn new() -> Self {
        Self {
            injuries: std::collections::HashMap::new(),
        }
    }

    pub fn set_injury(&mut self, body_part: String, level: u8) {
        self.injuries.insert(body_part, level.min(6));
    }

    pub fn get_injury(&self, body_part: &str) -> u8 {
        self.injuries.get(body_part).copied().unwrap_or(0)
    }

    pub fn clear_all(&mut self) {
        self.injuries.clear();
    }
}

/// Status indicator state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndicatorData {
    pub indicator_id: String,  // Feed id, e.g., "kneeling", "hidden"
    pub active: bool,          // Whether indicator is on
    pub color: Option<String>, // Optional color override
}

/// Room description content. Exits/players/objects keep their styled
/// segments (with link data) so the GUI can render the component text
/// verbatim, Wrayth-style, with clickable links.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoomContent {
    pub name: String,
    pub description: Vec<StyledLine>,
    pub exits: Vec<StyledLine>,
    pub players: Vec<StyledLine>,
    pub objects: Vec<StyledLine>,
}

/// Active effect (buff/debuff/cooldown/active spell)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActiveEffect {
    pub id: String,   // Unique identifier
    pub text: String, // Display text (e.g., "Fasthr's Reward")
    pub value: u32,   // Progress/percentage (0-100)
    pub time: String, // Time remaining (e.g., "03:06:54")
    /// Absolute expiry (server unix time), derived from `time` when the
    /// effect arrives. The protocol only re-sends effects on change, so the
    /// `time` string goes stale; this stays comparable against current time.
    /// None for unparseable durations (e.g. "Indefinite").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    pub bar_color: Option<String>,
    pub text_color: Option<String>,
}

impl ActiveEffect {
    /// Seconds remaining right now, derived from the absolute expiry captured
    /// at the last server update. `None` when the duration was unparseable
    /// ("Indefinite", stack counts) — those never tick.
    pub fn remaining_seconds(&self, now_server: i64) -> Option<i64> {
        self.expires_at.map(|at| (at - now_server).max(0))
    }

    /// The time string to display at `now_server`: the ticking remainder when
    /// there is one, the server's own string otherwise. Always HH:MM:SS so the
    /// text is stable against the wire's format. Holds at 00:00:00 after
    /// expiry — the server owns removal; the display only owns the number.
    pub fn display_time(&self, now_server: i64) -> String {
        match self.remaining_seconds(now_server) {
            Some(s) => format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60),
            None => self.time.clone(),
        }
    }

    /// The bar percent at `now_server`: the server's percent scaled by how much
    /// of the arrival remainder is left, so the bar drains smoothly between
    /// refreshes and snaps to the server's number on each one. Falls back to
    /// the raw percent when there is nothing to tick.
    pub fn display_value(&self, now_server: i64) -> u32 {
        let (Some(remaining), Some(at_arrival)) = (
            self.remaining_seconds(now_server),
            parse_time_seconds(&self.time),
        ) else {
            return self.value;
        };
        if at_arrival <= 0 {
            return self.value;
        }
        ((self.value as i64 * remaining) / at_arrival).clamp(0, self.value as i64) as u32
    }

    /// Whether this effect has anything to tick — gates the once-a-second
    /// repaint so a board of Indefinite effects schedules nothing.
    pub fn ticks(&self) -> bool {
        self.expires_at.is_some()
    }
}

/// Parse an effect duration string ("HH:MM:SS" or "MM:SS") into seconds.
/// Returns None for anything non-numeric (e.g. "Indefinite", "").
pub fn parse_time_seconds(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut total: i64 = 0;
    for part in &parts {
        let n: i64 = part.trim().parse().ok()?;
        total = total * 60 + n;
    }
    Some(total)
}

/// Active effects content (for buffs, debuffs, cooldowns, active spells)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActiveEffectsContent {
    pub category: String, // "Buffs", "Debuffs", "Cooldowns", "ActiveSpells"
    pub effects: Vec<ActiveEffect>,
    /// Bumped on every effect change; sync skips unchanged rebuilds
    pub generation: u64,
}

impl ActiveEffectsContent {
    /// The window/template name that renders an effects `category` string
    /// (from `<dialogData>` / `<clearContainer>`), or `None` for an unknown
    /// category. Single source of truth for this mapping so the two effect
    /// handlers in the message pipeline don't drift.
    pub fn window_name_for_category(category: &str) -> Option<&'static str> {
        match category {
            "Buffs" => Some("buffs"),
            "Debuffs" => Some("debuffs"),
            "Cooldowns" => Some("cooldowns"),
            "ActiveSpells" => Some("active_spells"),
            _ => None,
        }
    }
}

/// One reward line on a quest objective (`<reward type='experience' amount='5000'/>`)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveReward {
    pub reward_type: String, // "experience", "fame", ...
    pub amount: u64,
}

/// An action the player can take on an objective
/// (`<action type='accept' cmd='QUEST ACCEPT s24352'/>`)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveAction {
    pub action_type: String, // "accept", ...
    pub cmd: String,         // Verbatim game command to send
}

/// One entry from the `<objectives>` quest panel feed
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Objective {
    pub id: String,
    pub kind: String,  // type attribute: "QUEST"
    pub state: String, // "available", ...
    pub name: String,
    pub description: String, // May contain embedded newlines (multi-step lists)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cadence: Option<String>, // "weekly", "monthly"
    pub rewards: Vec<ObjectiveReward>,
    pub actions: Vec<ObjectiveAction>,
}

/// Quest objectives content (Saga quest panel feed)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ObjectivesContent {
    pub objectives: Vec<Objective>,
    /// Bumped on every change; sync skips unchanged rebuilds
    pub generation: u64,
}

/// Tab definition for tabbed text window
#[derive(Clone, Debug)]
pub struct TabDefinition {
    pub name: String,                          // Display name of tab
    pub streams: Vec<String>,                  // Stream IDs this tab listens to
    pub show_timestamps: bool,                 // Whether to render timestamps for this tab
    pub ignore_activity: bool,                 // Skip unread indicators/counts
    pub timestamp_position: TimestampPosition, // Position of timestamps (start or end)
}

/// Holds the state for a single tab, including its definition and content.
#[derive(Clone, Debug)]
pub struct TabState {
    pub definition: TabDefinition,
    pub content: TextContent,
    pub has_unread: bool, // Whether tab has unread messages
}

/// Tabbed text window content
#[derive(Clone, Debug)]
pub struct TabbedTextContent {
    pub tabs: Vec<TabState>,
    pub active_tab_index: usize,
}

impl TabbedTextContent {
    pub fn new(
        tabs: Vec<(String, Vec<String>, bool, bool, TimestampPosition)>,
        max_lines_per_tab: usize,
    ) -> Self {
        let tabs = tabs
            .into_iter()
            .map(
                |(name, streams, show_timestamps, ignore_activity, timestamp_position)| {
                    let definition = TabDefinition {
                        name: name.clone(),
                        streams,
                        show_timestamps,
                        ignore_activity,
                        timestamp_position,
                    };
                    let mut content = TextContent::new(name, max_lines_per_tab);
                    // Mirror timestamp settings onto the tab's content so the
                    // shared text renderer can honor them uniformly.
                    content.show_timestamps = definition.show_timestamps;
                    content.timestamp_position = definition.timestamp_position;
                    TabState {
                        definition,
                        content,
                        has_unread: false,
                    }
                },
            )
            .collect();
        Self {
            tabs,
            active_tab_index: 0,
        }
    }

    /// Mark a specific tab as having unread messages
    pub fn mark_tab_unread(&mut self, tab_index: usize) {
        if let Some(tab) = self.tabs.get_mut(tab_index) {
            // Only mark as unread if not the active tab and activity tracking is enabled
            if tab_index != self.active_tab_index && !tab.definition.ignore_activity {
                tab.has_unread = true;
            }
        }
    }

    /// Clear unread status for a specific tab (called when user switches to it)
    pub fn clear_tab_unread(&mut self, tab_index: usize) {
        if let Some(tab) = self.tabs.get_mut(tab_index) {
            tab.has_unread = false;
        }
    }

    /// Clear unread status for the currently active tab
    pub fn clear_active_tab_unread(&mut self) {
        self.clear_tab_unread(self.active_tab_index);
    }

    /// Get the index of the next tab with unread messages
    pub fn next_unread_tab(&self) -> Option<usize> {
        // Start searching from the tab after the active one
        let start = self.active_tab_index + 1;

        // Search from active+1 to end
        for i in start..self.tabs.len() {
            if self.tabs[i].has_unread {
                return Some(i);
            }
        }

        // Wrap around and search from beginning to active
        for i in 0..self.active_tab_index {
            if self.tabs[i].has_unread {
                return Some(i);
            }
        }

        None
    }

    /// Update tabs from new layout definition while preserving content for existing tabs.
    /// New tabs are added with empty content. Tabs not in the new definition are removed.
    /// Returns true if the tabs structure changed (requiring widget cache reset).
    pub fn update_tabs(
        &mut self,
        new_tabs: Vec<(String, Vec<String>, bool, bool, TimestampPosition)>,
        max_lines_per_tab: usize,
    ) -> bool {
        // Quick check: if tab count and names match, no structural change needed
        let old_names: Vec<&str> = self
            .tabs
            .iter()
            .map(|t| t.definition.name.as_str())
            .collect();
        let new_names: Vec<&str> = new_tabs
            .iter()
            .map(|(name, _, _, _, _)| name.as_str())
            .collect();

        if old_names == new_names {
            // Just update definitions (streams, settings) without recreating
            for (tab, (_, streams, show_ts, ignore, ts_pos)) in
                self.tabs.iter_mut().zip(new_tabs.iter())
            {
                tab.definition.streams = streams.clone();
                tab.definition.show_timestamps = *show_ts;
                tab.definition.ignore_activity = *ignore;
                tab.definition.timestamp_position = *ts_pos;
                tab.content.show_timestamps = *show_ts;
                tab.content.timestamp_position = *ts_pos;
            }
            return false; // No structural change
        }

        // Structural change - rebuild tabs, preserving content where possible
        let mut old_tabs: std::collections::HashMap<String, TabState> = self
            .tabs
            .drain(..)
            .map(|t| (t.definition.name.clone(), t))
            .collect();

        self.tabs = new_tabs
            .into_iter()
            .map(
                |(name, streams, show_timestamps, ignore_activity, timestamp_position)| {
                    let definition = TabDefinition {
                        name: name.clone(),
                        streams,
                        show_timestamps,
                        ignore_activity,
                        timestamp_position,
                    };

                    // Reuse existing tab content if available
                    if let Some(mut old_tab) = old_tabs.remove(&name) {
                        old_tab.content.show_timestamps = definition.show_timestamps;
                        old_tab.content.timestamp_position = definition.timestamp_position;
                        old_tab.definition = definition;
                        old_tab
                    } else {
                        // New tab - empty content
                        let mut content = TextContent::new(&name, max_lines_per_tab);
                        content.show_timestamps = definition.show_timestamps;
                        content.timestamp_position = definition.timestamp_position;
                        TabState {
                            definition,
                            content,
                            has_unread: false,
                        }
                    }
                },
            )
            .collect();

        // Ensure active_tab_index is valid
        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len().saturating_sub(1);
        }

        true // Structural change occurred
    }
}

impl TextContent {
    pub fn new(title: impl Into<String>, max_lines: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max_lines),
            scroll_offset: 0,
            max_lines,
            title: title.into(),
            generation: 0,
            streams: vec![], // Default to empty - will be set during window creation
            compact: false,  // Default to disabled - set during window creation from layout
            show_timestamps: false,
            timestamp_position: TimestampPosition::default(),
        }
    }

    pub fn add_line(&mut self, mut line: StyledLine) {
        // Stamp arrival time once, centrally, so any window that enables
        // timestamps (now or later) can render when each line arrived.
        if line.timestamp.is_none() {
            line.timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|elapsed| elapsed.as_secs() as i64);
        }
        self.lines.push_back(line);
        // Only prune if max_lines > 0 (0 means unlimited - content managed by clearStream)
        if self.max_lines > 0 && self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
        // Increment generation counter on every add_line call
        // This allows frontend to detect changes even when line count stays constant
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn scroll_up(&mut self, amount: usize) {
        let max_scroll = self.lines.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_to_top(&mut self) {
        let max_scroll = self.lines.len().saturating_sub(1);
        self.scroll_offset = max_scroll;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }
}

impl StyledLine {
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            segments: vec![TextSegment {
                text: text.into(),
                fg: None,
                bg: None,
                bold: false,
                mono: false,
                span_type: SpanType::Normal,
                link_data: None,
                custom_emoji: None,
                inline_image: None,
            }],
            stream: String::from("main"),
            timestamp: None,
        }
    }

    /// Create a StyledLine with a specific stream
    pub fn from_text_with_stream(text: impl Into<String>, stream: impl Into<String>) -> Self {
        Self {
            segments: vec![TextSegment {
                text: text.into(),
                fg: None,
                bg: None,
                bold: false,
                mono: false,
                span_type: SpanType::Normal,
                link_data: None,
                custom_emoji: None,
                inline_image: None,
            }],
            stream: stream.into(),
            timestamp: None,
        }
    }
}

// ==================== Perception Window Structures ====================

/// Perception entry with parsed format and calculated weight for sorting
#[derive(Clone, Debug, PartialEq)]
pub struct PerceptionEntry {
    pub name: String,                // "Bless", "Monkey"
    pub format: PerceptionFormat,    // Parsed format type
    pub raw_text: String,            // Original text
    pub weight: i32,                 // Sort priority
    pub link_data: Option<LinkData>, // Optional clickable link
}

/// Perception format types detected from parenthetical suffixes
#[derive(Clone, Debug, PartialEq)]
pub enum PerceptionFormat {
    OngoingMagic,   // (OM)
    Indefinite,     // (Indefinite) or (Cyclic)
    Fading,         // (Fading)
    Percentage(u8), // (94%)
    Roisaen(u32),   // (82 roisaen)
    Other(String),  // Unknown formats
}

/// Perception window content (sorted entries)
#[derive(Clone, Debug, PartialEq)]
pub struct PerceptionData {
    pub entries: Vec<PerceptionEntry>, // Sorted by weight
    pub last_update: i64,              // Unix timestamp
    /// Bumped on every entries change; sync skips unchanged reprocessing
    /// (last_update is whole-second and can miss same-second updates)
    pub generation: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(rows: f32) -> InlineImage {
        InlineImage {
            name: "banner".to_string(),
            rows,
            align: FloatAlign::Left,
        }
    }

    /// A square image at a legal row count takes exactly that many rows.
    #[test]
    fn inline_image_honors_requested_rows() {
        let (w, h) = img(4.0).fitted_size((64.0, 64.0), 20.0, 800.0, 400.0, 8.0);
        assert_eq!(h, 80.0);
        assert_eq!(w, 80.0, "square art stays square");
    }

    /// Aspect ratio is preserved: a 2:1 image is twice as wide as it is tall.
    #[test]
    fn inline_image_preserves_aspect() {
        let (w, h) = img(2.0).fitted_size((128.0, 64.0), 20.0, 800.0, 400.0, 8.0);
        assert_eq!(h, 40.0);
        assert_eq!(w, 80.0);
    }

    /// The window's own visible height caps the image: 40 rows in a 6-row
    /// window yields a 6-row image, not a broken window.
    #[test]
    fn inline_image_clamps_to_window_rows() {
        let (_, h) = img(40.0).fitted_size((64.0, 64.0), 20.0, 800.0, 120.0, 64.0);
        assert_eq!(h, 120.0, "6 rows * 20pt");
    }

    /// The configured ceiling applies even when the window is tall enough.
    #[test]
    fn inline_image_clamps_to_configured_max() {
        let (_, h) = img(40.0).fitted_size((64.0, 64.0), 20.0, 800.0, 4000.0, 8.0);
        assert_eq!(h, 160.0, "8 rows * 20pt");
    }

    /// A very wide image scales BOTH dimensions down so text keeps a
    /// readable column — a legal row count is not enough on its own.
    #[test]
    fn inline_image_width_clamp_scales_height_too() {
        // 10:1 art at 4 rows would be 800pt wide in an 800pt window.
        let (w, h) = img(4.0).fitted_size((640.0, 64.0), 20.0, 800.0, 400.0, 8.0);
        assert!(
            w <= 800.0 * (1.0 - INLINE_IMAGE_MIN_TEXT_FRACTION) + 0.01,
            "w={w}"
        );
        assert!(h < 80.0, "height must shrink with width, got {h}");
        // Aspect preserved through the clamp.
        assert!((w / h - 10.0).abs() < 0.01, "aspect drifted: {w}x{h}");
    }

    /// Degenerate texture sizes fall back to square rather than producing a
    /// zero or infinite rect.
    #[test]
    fn inline_image_survives_degenerate_texture() {
        let (w, h) = img(2.0).fitted_size((0.0, 0.0), 20.0, 800.0, 400.0, 8.0);
        assert_eq!((w, h), (40.0, 40.0));
    }

    /// Collapse triggers only when the leftover text column is too narrow.
    #[test]
    fn inline_image_collapse_threshold() {
        assert!(!InlineImage::should_collapse(300.0, 800.0), "roomy");
        assert!(InlineImage::should_collapse(600.0, 800.0), "too narrow");
    }

    #[test]
    fn effect_category_maps_to_its_window() {
        assert_eq!(
            ActiveEffectsContent::window_name_for_category("Buffs"),
            Some("buffs")
        );
        assert_eq!(
            ActiveEffectsContent::window_name_for_category("Debuffs"),
            Some("debuffs")
        );
        assert_eq!(
            ActiveEffectsContent::window_name_for_category("Cooldowns"),
            Some("cooldowns")
        );
        assert_eq!(
            ActiveEffectsContent::window_name_for_category("ActiveSpells"),
            Some("active_spells")
        );
        assert_eq!(
            ActiveEffectsContent::window_name_for_category("Nonsense"),
            None
        );
    }

    // ==================== Serde Round-Trip Tests ====================
    // The web frontend ships StyledLine over WebSocket as JSON; these
    // pin the wire format (docs/mobile-web-frontend-plan.md, Phase 0).

    #[test]
    fn test_styled_line_json_round_trip() {
        let line = StyledLine {
            stream: "main".to_string(),
            timestamp: Some(1_720_000_000),
            segments: vec![
                TextSegment::plain("You see "),
                TextSegment {
                    text: "a kobold".to_string(),
                    fg: Some("#ff0000".to_string()),
                    bg: Some("#000000".to_string()),
                    bold: true,
                    mono: false,
                    span_type: SpanType::Monsterbold,
                    link_data: Some(LinkData {
                        exist_id: "12345".to_string(),
                        noun: "kobold".to_string(),
                        text: "a kobold".to_string(),
                        coord: Some("2524,1864".to_string()),
                    }),
                    custom_emoji: None,
                    inline_image: None,
                },
                TextSegment::styled(" here.", Some("#a0a0a0".to_string()), false),
            ],
        };

        let json = serde_json::to_string(&line).expect("StyledLine must serialize");
        let back: StyledLine = serde_json::from_str(&json).expect("StyledLine must deserialize");
        assert_eq!(line, back);
    }

    // ==================== TextContent Tests ====================

    #[test]
    fn test_text_content_new() {
        let content = TextContent::new("Main", 1000);

        assert_eq!(content.title, "Main");
        assert_eq!(content.max_lines, 1000);
        assert_eq!(content.scroll_offset, 0);
        assert_eq!(content.generation, 0);
        assert!(content.lines.is_empty());
    }

    #[test]
    fn test_text_content_add_line() {
        let mut content = TextContent::new("Test", 100);

        content.add_line(StyledLine::from_text("Hello"));
        assert_eq!(content.lines.len(), 1);
        assert_eq!(content.generation, 1);

        content.add_line(StyledLine::from_text("World"));
        assert_eq!(content.lines.len(), 2);
        assert_eq!(content.generation, 2);
    }

    #[test]
    fn test_text_content_max_lines_limit() {
        let mut content = TextContent::new("Test", 3);

        content.add_line(StyledLine::from_text("Line 1"));
        content.add_line(StyledLine::from_text("Line 2"));
        content.add_line(StyledLine::from_text("Line 3"));
        assert_eq!(content.lines.len(), 3);

        // Adding a 4th line should remove the oldest
        content.add_line(StyledLine::from_text("Line 4"));
        assert_eq!(content.lines.len(), 3);

        // First line should now be "Line 2"
        assert_eq!(content.lines[0].segments[0].text, "Line 2");
        assert_eq!(content.lines[2].segments[0].text, "Line 4");
    }

    #[test]
    fn test_text_content_generation_increments() {
        let mut content = TextContent::new("Test", 5);

        for i in 0..10 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
            assert_eq!(content.generation, (i + 1) as u64);
        }
    }

    #[test]
    fn test_text_content_scroll_up() {
        let mut content = TextContent::new("Test", 100);
        for i in 0..20 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
        }

        assert_eq!(content.scroll_offset, 0);

        content.scroll_up(5);
        assert_eq!(content.scroll_offset, 5);

        content.scroll_up(5);
        assert_eq!(content.scroll_offset, 10);

        // Scroll beyond max should clamp
        content.scroll_up(100);
        assert_eq!(content.scroll_offset, 19); // max is lines.len() - 1
    }

    #[test]
    fn test_text_content_scroll_down() {
        let mut content = TextContent::new("Test", 100);
        for i in 0..20 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
        }

        content.scroll_offset = 15;

        content.scroll_down(5);
        assert_eq!(content.scroll_offset, 10);

        content.scroll_down(5);
        assert_eq!(content.scroll_offset, 5);

        // Scroll below 0 should clamp to 0
        content.scroll_down(100);
        assert_eq!(content.scroll_offset, 0);
    }

    #[test]
    fn test_text_content_scroll_to_top() {
        let mut content = TextContent::new("Test", 100);
        for i in 0..20 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
        }

        content.scroll_to_top();
        assert_eq!(content.scroll_offset, 19); // lines.len() - 1
    }

    #[test]
    fn test_text_content_scroll_to_bottom() {
        let mut content = TextContent::new("Test", 100);
        for i in 0..20 {
            content.add_line(StyledLine::from_text(format!("Line {}", i)));
        }
        content.scroll_offset = 15;

        content.scroll_to_bottom();
        assert_eq!(content.scroll_offset, 0);
    }

    // ==================== StyledLine Tests ====================

    #[test]
    fn test_styled_line_from_text() {
        let line = StyledLine::from_text("Hello, world!");

        assert_eq!(line.segments.len(), 1);
        assert_eq!(line.segments[0].text, "Hello, world!");
        assert_eq!(line.segments[0].fg, None);
        assert_eq!(line.segments[0].bg, None);
        assert!(!line.segments[0].bold);
        assert_eq!(line.segments[0].span_type, SpanType::Normal);
        assert!(line.segments[0].link_data.is_none());
    }

    // ==================== TextSegment Tests ====================

    #[test]
    fn test_text_segment_with_link() {
        let segment = TextSegment {
            text: "a rusty sword".to_string(),
            fg: Some("#477ab3".to_string()),
            bg: None,
            bold: false,
            mono: false,
            span_type: SpanType::Link,
            link_data: Some(LinkData {
                exist_id: "12345".to_string(),
                noun: "sword".to_string(),
                text: "a rusty sword".to_string(),
                coord: None,
            }),
            custom_emoji: None,
            inline_image: None,
        };

        assert_eq!(segment.span_type, SpanType::Link);
        let link = segment.link_data.as_ref().unwrap();
        assert_eq!(link.exist_id, "12345");
        assert_eq!(link.noun, "sword");
    }

    #[test]
    fn test_text_segment_equality() {
        let seg1 = TextSegment {
            text: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            mono: false,
            span_type: SpanType::Monsterbold,
            link_data: None,
            custom_emoji: None,
            inline_image: None,
        };

        let seg2 = TextSegment {
            text: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            mono: false,
            span_type: SpanType::Monsterbold,
            link_data: None,
            custom_emoji: None,
            inline_image: None,
        };

        let seg3 = TextSegment {
            text: "different".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            mono: false,
            span_type: SpanType::Monsterbold,
            link_data: None,
            custom_emoji: None,
            inline_image: None,
        };

        assert_eq!(seg1, seg2);
        assert_ne!(seg1, seg3);
    }

    // ==================== LinkData Tests ====================

    #[test]
    fn test_link_data_gs4_style() {
        let link = LinkData {
            exist_id: "67890".to_string(),
            noun: "chest".to_string(),
            text: "an iron chest".to_string(),
            coord: Some("1234,5678".to_string()),
        };

        assert_eq!(link.exist_id, "67890");
        assert_eq!(link.noun, "chest");
        assert_eq!(link.text, "an iron chest");
        assert_eq!(link.coord, Some("1234,5678".to_string()));
    }

    #[test]
    fn test_link_data_dr_style() {
        // DragonRealms uses _direct_ marker with cmd in noun
        let link = LinkData {
            exist_id: "_direct_".to_string(),
            noun: "get #8735861 in #8735860 in watery portal".to_string(),
            text: "Some arzumodine cloth".to_string(),
            coord: None,
        };

        assert_eq!(link.exist_id, "_direct_");
        assert!(link.noun.contains("#8735861"));
        assert_eq!(link.coord, None);
    }

    #[test]
    fn test_link_data_equality() {
        let link1 = LinkData {
            exist_id: "123".to_string(),
            noun: "sword".to_string(),
            text: "a sword".to_string(),
            coord: None,
        };

        let link2 = LinkData {
            exist_id: "123".to_string(),
            noun: "sword".to_string(),
            text: "a sword".to_string(),
            coord: None,
        };

        let link3 = LinkData {
            exist_id: "456".to_string(),
            noun: "sword".to_string(),
            text: "a sword".to_string(),
            coord: None,
        };

        assert_eq!(link1, link2);
        assert_ne!(link1, link3);
    }

    // ==================== SpanType Tests ====================

    #[test]
    fn test_span_type_variants() {
        assert_eq!(SpanType::Normal, SpanType::Normal);
        assert_ne!(SpanType::Normal, SpanType::Link);
        assert_ne!(SpanType::Link, SpanType::Monsterbold);
        assert_ne!(SpanType::Monsterbold, SpanType::Spell);
        assert_ne!(SpanType::Spell, SpanType::Speech);
    }

    // ==================== InjuryDollData Tests ====================

    #[test]
    fn test_injury_doll_new() {
        let doll = InjuryDollData::new();
        assert!(doll.injuries.is_empty());
    }

    #[test]
    fn test_injury_doll_set_get() {
        let mut doll = InjuryDollData::new();

        doll.set_injury("head".to_string(), 2);
        assert_eq!(doll.get_injury("head"), 2);

        doll.set_injury("leftArm".to_string(), 5);
        assert_eq!(doll.get_injury("leftArm"), 5);

        // Non-existent body part returns 0
        assert_eq!(doll.get_injury("nonexistent"), 0);
    }

    #[test]
    fn test_injury_doll_level_clamped() {
        let mut doll = InjuryDollData::new();

        // Level should be clamped to max 6
        doll.set_injury("head".to_string(), 10);
        assert_eq!(doll.get_injury("head"), 6);
    }

    #[test]
    fn test_injury_doll_clear_all() {
        let mut doll = InjuryDollData::new();

        doll.set_injury("head".to_string(), 2);
        doll.set_injury("chest".to_string(), 3);
        doll.set_injury("leftArm".to_string(), 1);

        assert_eq!(doll.injuries.len(), 3);

        doll.clear_all();
        assert!(doll.injuries.is_empty());
        assert_eq!(doll.get_injury("head"), 0);
    }

    // ==================== TabbedTextContent Tests ====================

    #[test]
    fn test_tabbed_text_content_new() {
        let tabs = vec![
            (
                "Main".to_string(),
                vec!["main".to_string()],
                false,
                false,
                TimestampPosition::End,
            ),
            (
                "Combat".to_string(),
                vec!["combat".to_string(), "death".to_string()],
                true,
                true,
                TimestampPosition::Start,
            ),
        ];

        let content = TabbedTextContent::new(tabs, 1000);

        assert_eq!(content.tabs.len(), 2);
        assert_eq!(content.active_tab_index, 0);

        assert_eq!(content.tabs[0].definition.name, "Main");
        assert_eq!(content.tabs[0].definition.streams, vec!["main"]);
        assert!(!content.tabs[0].definition.show_timestamps);
        assert!(!content.tabs[0].definition.ignore_activity);
        assert_eq!(
            content.tabs[0].definition.timestamp_position,
            TimestampPosition::End
        );

        assert_eq!(content.tabs[1].definition.name, "Combat");
        assert_eq!(content.tabs[1].definition.streams, vec!["combat", "death"]);
        assert!(content.tabs[1].definition.show_timestamps);
        assert!(content.tabs[1].definition.ignore_activity);
        assert_eq!(
            content.tabs[1].definition.timestamp_position,
            TimestampPosition::Start
        );
    }

    // ==================== ProgressData Tests ====================

    #[test]
    fn test_progress_data() {
        let progress = ProgressData {
            value: 75,
            max: 100,
            label: "Health".to_string(),
            color: Some("#00FF00".to_string()),
            progress_id: "health".to_string(),
            numbers_only: false,
            current_only: false,
        };

        assert_eq!(progress.value, 75);
        assert_eq!(progress.max, 100);
        assert_eq!(progress.label, "Health");
        assert_eq!(progress.color, Some("#00FF00".to_string()));
        assert_eq!(progress.progress_id, "health");
    }

    // ==================== CompassData Tests ====================

    #[test]
    fn test_compass_data() {
        let compass = CompassData {
            directions: vec!["n".to_string(), "e".to_string(), "out".to_string()],
        };

        assert_eq!(compass.directions.len(), 3);
        assert!(compass.directions.contains(&"n".to_string()));
        assert!(compass.directions.contains(&"e".to_string()));
        assert!(compass.directions.contains(&"out".to_string()));
    }

    // ==================== ActiveEffect Tests ====================

    #[test]
    fn test_active_effect() {
        let effect = ActiveEffect {
            id: "115".to_string(),
            text: "Fasthr's Reward".to_string(),
            value: 74,
            time: "03:06:54".to_string(),
            expires_at: None,
            bar_color: Some("#00FF00".to_string()),
            text_color: None,
        };

        assert_eq!(effect.id, "115");
        assert_eq!(effect.text, "Fasthr's Reward");
        assert_eq!(effect.value, 74);
        assert_eq!(effect.time, "03:06:54");
    }

    #[test]
    fn test_parse_time_seconds() {
        assert_eq!(parse_time_seconds("03:06:54"), Some(11214));
        assert_eq!(parse_time_seconds("00:01:05"), Some(65));
        assert_eq!(parse_time_seconds("01:05"), Some(65));
        assert_eq!(parse_time_seconds("0:24:10"), Some(1450));
        assert_eq!(parse_time_seconds("45"), Some(45));
        assert_eq!(parse_time_seconds("Indefinite"), None);
        assert_eq!(parse_time_seconds(""), None);
        assert_eq!(parse_time_seconds("1:2:3:4"), None);
    }

    // ==================== Effect countdown derivation ====================

    fn timed_effect(value: u32, time: &str, expires_at: Option<i64>) -> ActiveEffect {
        ActiveEffect {
            id: "test".to_string(),
            text: "Test Effect".to_string(),
            value,
            time: time.to_string(),
            expires_at,
            bar_color: None,
            text_color: None,
        }
    }

    /// The displayed time ticks down from the absolute expiry, holds at zero
    /// after it (the server owns removal), and falls back to the server's own
    /// string when there is nothing to tick.
    #[test]
    fn effect_display_time_ticks_from_expiry() {
        // Arrived at t=1000 with 60s remaining.
        let effect = timed_effect(100, "00:01:00", Some(1060));
        assert_eq!(effect.display_time(1000), "00:01:00");
        assert_eq!(effect.display_time(1030), "00:00:30");
        assert_eq!(effect.display_time(1059), "00:00:01");
        // Past expiry: hold at zero, never negative.
        assert_eq!(effect.display_time(1100), "00:00:00");

        // Hours format.
        let long = timed_effect(100, "03:06:54", Some(1000 + 11214));
        assert_eq!(long.display_time(1000), "03:06:54");
        assert_eq!(long.display_time(1001), "03:06:53");

        // Indefinite: the server's string, untouched, at any time.
        let indefinite = timed_effect(100, "Indefinite", None);
        assert_eq!(indefinite.display_time(999_999), "Indefinite");
        assert!(!indefinite.ticks());
    }

    /// The bar percent scales the server's value by the fraction of the
    /// arrival remainder still left — smooth drain, snapping to the server's
    /// number on each refresh (where now == arrival and the scale is 1).
    #[test]
    fn effect_display_value_drains_proportionally() {
        // Arrived at t=1000: 60s left, bar at 50%.
        let effect = timed_effect(50, "00:01:00", Some(1060));
        assert_eq!(effect.display_value(1000), 50); // at arrival: server's number
        assert_eq!(effect.display_value(1030), 25); // half the time -> half the fill
        assert_eq!(effect.display_value(1060), 0); // expired -> empty
        assert_eq!(effect.display_value(1100), 0); // held, never negative

        // Unparseable duration: the raw value, untouched.
        let indefinite = timed_effect(75, "Indefinite", None);
        assert_eq!(indefinite.display_value(999_999), 75);

        // Degenerate zero-length arrival can't divide by zero.
        let zero = timed_effect(40, "00:00:00", Some(1000));
        assert_eq!(zero.display_value(1000), 40);
    }

    // ==================== TabbedTextContent update_tabs Tests ====================

    #[test]
    fn test_tabbed_text_content_update_tabs_no_change() {
        use super::TabbedTextContent;
        use crate::config::TimestampPosition;

        let mut tabbed = TabbedTextContent::new(
            vec![
                (
                    "Main".to_string(),
                    vec!["main".to_string()],
                    false,
                    false,
                    TimestampPosition::End,
                ),
                (
                    "Combat".to_string(),
                    vec!["combat".to_string()],
                    true,
                    false,
                    TimestampPosition::Start,
                ),
            ],
            1000,
        );

        // Same tabs - should return false (no structural change)
        let changed = tabbed.update_tabs(
            vec![
                (
                    "Main".to_string(),
                    vec!["main".to_string()],
                    false,
                    false,
                    TimestampPosition::End,
                ),
                (
                    "Combat".to_string(),
                    vec!["combat".to_string()],
                    true,
                    false,
                    TimestampPosition::Start,
                ),
            ],
            1000,
        );
        assert!(!changed);
        assert_eq!(tabbed.tabs.len(), 2);
    }

    #[test]
    fn test_tabbed_text_content_update_tabs_add_tab() {
        use super::TabbedTextContent;
        use crate::config::TimestampPosition;

        let mut tabbed = TabbedTextContent::new(
            vec![(
                "Main".to_string(),
                vec!["main".to_string()],
                false,
                false,
                TimestampPosition::End,
            )],
            1000,
        );

        // Add a new tab - should return true (structural change)
        let changed = tabbed.update_tabs(
            vec![
                (
                    "Main".to_string(),
                    vec!["main".to_string()],
                    false,
                    false,
                    TimestampPosition::End,
                ),
                (
                    "Combat".to_string(),
                    vec!["combat".to_string()],
                    true,
                    false,
                    TimestampPosition::Start,
                ),
            ],
            1000,
        );
        assert!(changed);
        assert_eq!(tabbed.tabs.len(), 2);
        assert_eq!(tabbed.tabs[0].definition.name, "Main");
        assert_eq!(tabbed.tabs[1].definition.name, "Combat");
    }

    #[test]
    fn test_tabbed_text_content_update_tabs_remove_tab() {
        use super::TabbedTextContent;
        use crate::config::TimestampPosition;

        let mut tabbed = TabbedTextContent::new(
            vec![
                (
                    "Main".to_string(),
                    vec!["main".to_string()],
                    false,
                    false,
                    TimestampPosition::End,
                ),
                (
                    "Combat".to_string(),
                    vec!["combat".to_string()],
                    true,
                    false,
                    TimestampPosition::Start,
                ),
            ],
            1000,
        );
        tabbed.active_tab_index = 1; // Set to Combat tab

        // Remove Combat tab - should return true and fix active_tab_index
        let changed = tabbed.update_tabs(
            vec![(
                "Main".to_string(),
                vec!["main".to_string()],
                false,
                false,
                TimestampPosition::End,
            )],
            1000,
        );
        assert!(changed);
        assert_eq!(tabbed.tabs.len(), 1);
        assert_eq!(tabbed.active_tab_index, 0); // Should be clamped
    }
}
