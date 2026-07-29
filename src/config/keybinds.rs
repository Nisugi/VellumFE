use super::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyBindAction {
    Action(String),     // Just an action: "cursor_word_left"
    Macro(MacroAction), // A macro with text
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroAction {
    pub macro_text: String, // e.g., "sw\r" for southwest movement
}

impl KeyBindAction {
    /// Returns the type name of this keybind action
    pub fn type_name(&self) -> &'static str {
        match self {
            KeyBindAction::Action(_) => "Action",
            KeyBindAction::Macro(_) => "Macro",
        }
    }

    /// Returns the display value for this keybind action
    pub fn display_value(&self) -> String {
        match self {
            KeyBindAction::Action(a) => a.clone(),
            KeyBindAction::Macro(m) => m.macro_text.clone(),
        }
    }
}

/// Application keybinds that work across all modes or are mode-specific
/// These are checked in Layer 1 of the keybind dispatch system (before menu and game keybinds)
/// Note: Previously called GlobalKeybinds, renamed to avoid confusion with "global" folder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppKeybinds {
    /// Quit the application (default: "ctrl+c")
    #[serde(default = "default_quit_keybind")]
    pub quit: String,

    /// Start search mode (default: "ctrl+f")
    #[serde(default = "default_start_search_keybind")]
    pub start_search: String,

    /// Next search match - only works in Search mode (default: "ctrl+pagedown")
    #[serde(default = "default_next_search_match_keybind")]
    pub next_search_match: String,

    /// Previous search match - only works in Search mode (default: "ctrl+pageup")
    #[serde(default = "default_prev_search_match_keybind")]
    pub prev_search_match: String,

    /// Close priority windows (menus, browsers, forms) and exit modes (default: "esc")
    #[serde(default = "default_close_window_keybind")]
    pub close_window: String,
}

fn default_quit_keybind() -> String {
    "ctrl+c".to_string()
}

fn default_start_search_keybind() -> String {
    "ctrl+f".to_string()
}

fn default_next_search_match_keybind() -> String {
    "ctrl+pagedown".to_string()
}

fn default_prev_search_match_keybind() -> String {
    "ctrl+pageup".to_string()
}

fn default_close_window_keybind() -> String {
    "esc".to_string()
}

impl Default for AppKeybinds {
    fn default() -> Self {
        Self {
            quit: default_quit_keybind(),
            start_search: default_start_search_keybind(),
            next_search_match: default_next_search_match_keybind(),
            prev_search_match: default_prev_search_match_keybind(),
            close_window: default_close_window_keybind(),
        }
    }
}

/// Actions that can be bound to keys
#[derive(Debug, Clone, PartialEq)]
pub enum KeyAction {
    // Command input actions
    SendCommand,
    CursorLeft,
    CursorRight,
    CursorWordLeft,
    CursorWordRight,
    CursorHome,
    CursorEnd,
    CursorBackspace,
    CursorDelete,
    CursorDeleteWord, // Delete from cursor to end of word
    CursorClearLine,  // Clear entire command line

    // History actions
    PreviousCommand,
    NextCommand,
    SendLastCommand,
    SendSecondLastCommand,

    // Window actions
    SwitchCurrentWindow,
    ScrollCurrentWindowUpOne,
    ScrollCurrentWindowDownOne,
    ScrollCurrentWindowUpPage,
    ScrollCurrentWindowDownPage,
    ScrollCurrentWindowHome, // Scroll to top of window
    ScrollCurrentWindowEnd,  // Scroll to bottom of window

    // Search actions (already implemented)
    StartSearch,
    NextSearchMatch,
    PrevSearchMatch,
    ClearSearch,

    // Tab navigation (for TabbedText widgets)
    NextTab,       // Switch to next tab
    PrevTab,       // Switch to previous tab
    NextUnreadTab, // Jump to next tab with unread messages

    // Clipboard actions
    Copy,      // Copy selected text to clipboard
    Paste,     // Paste from clipboard
    SelectAll, // Select all text in command input

    // System toggles
    TogglePerformanceStats, // Show/hide performance overlay
    ToggleSounds,           // Enable/disable sound system

    // Travel
    StopTravel, // Cancel the active .go2 trip (Esc does this by default)

    // Interact mode: pointer-free entity focus cycling (controller-friendly)
    InteractMode, // Toggle interact mode on/off

    // Activate the focused entity in interact mode (walk an exit, open a
    // creature/object menu) AND confirm the highlighted item in a popup
    // menu. Bindable so "select" isn't hardwired to South; the gamepad
    // layer resolves it. No-op from a keyboard key.
    InteractSelect,

    // Popup-menu navigation as bindable controller actions (so nothing in
    // menus is hardwired). Fed to the modal-nav handler as arrow keys.
    // East always cancels as a hard fallback even if MenuCancel is rebound.
    MenuUp,
    MenuDown,
    MenuLeft,
    MenuRight,
    MenuCancel,

    // Controller shift modifier: while the bound button is held, other
    // buttons resolve against [controller_shift]. Handled entirely by the
    // gamepad layer; a no-op from a keyboard key.
    ControllerShift,

    // Controller radial wheel: hold the bound button to show the command
    // wheel, pick a slice with the left stick, release to fire. Handled
    // by the gamepad layer; a no-op from a keyboard key.
    ControllerWheel,

    // Toggle the controller binding-legend overlay (curated via the
    // HUD checkboxes in the .controller editor). GUI-handled.
    ControllerOverlay,

    // TTS (Text-to-Speech) actions - Accessibility
    TtsNext,           // Next message (sequential, includes read)
    TtsPrevious,       // Previous message (sequential, includes read)
    TtsNextUnread,     // Skip to next unread message
    TtsStop,           // Stop current speech (keeps position)
    TtsMuteToggle,     // Toggle TTS mute on/off
    TtsIncreaseRate,   // Increase speech rate by 0.1
    TtsDecreaseRate,   // Decrease speech rate by 0.1
    TtsIncreaseVolume, // Increase volume by 0.1
    TtsDecreaseVolume, // Decrease volume by 0.1

    // Macro - send literal text
    SendMacro(String),
}

/// One slice of the controller radial wheel: a label drawn on the wheel
/// and either a command to fire (game text or dot-command) or a child
/// ring of slices (a folder — opened with South while the wheel is held).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WheelSlice {
    pub label: String,
    #[serde(default)]
    pub command: String,
    /// Optional wedge tint (hex or palette name) — dim normally, bright
    /// while aimed, so wheels can be color-coded by function.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Optional wedge width in degrees. Slices with a span take exactly
    /// that; whatever remains of the 360° splits evenly among span-less
    /// slices, so a config with no spans keeps today's even ring. Sums
    /// over 360 and sub-30° results warn and auto-normalize at load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<f32>,
    /// Optional per-slice aim floor, percent of full deflection: below it
    /// this slice can't be aimed or committed (a destructive action can
    /// demand a deliberate throw). None falls back to the global
    /// `[controller_tuning] deadzone`. Gates aiming only — firing stays
    /// with the active fire mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slices: Vec<WheelSlice>,
    /// An explicit "go up one level" seat: placed, sized, colored, and
    /// floored like any other slice, but dwelling it ascends instead of
    /// firing. When a folder ring contains one, the runtime uses the ring
    /// verbatim and skips the synthesized Back seat and its anchor
    /// rotation entirely — the user owns the geometry.
    #[serde(default, skip_serializing_if = "wheel_flag_is_false")]
    pub back: bool,
    /// Designer-session lock: while set, whole-ring operations (even out)
    /// leave this slice's width alone. Never persisted — locks are an
    /// editing aid, not wheel config — but kept on the slice so structural
    /// edits (move, mirror, delete) carry them along for free.
    #[serde(skip)]
    pub locked: bool,
}

impl WheelSlice {
    pub fn is_folder(&self) -> bool {
        !self.slices.is_empty()
    }
}

fn wheel_flag_is_false(v: &bool) -> bool {
    !*v
}

/// The minimum sensible wedge width in degrees. Explicit spans below this
/// are hard to hit; the layout resolver clamps up to it and the validator
/// warns. The single source shared by the frontend layout (`resolve_spans`)
/// and the load-time validator.
pub const WHEEL_MIN_SPAN_DEG: f32 = 30.0;

/// A problem with one ring's `span` numbers, surfaced as an advisory (the
/// runtime resolver always produces a usable ring anyway — it clamps and
/// scales to 360). `wheel` is the display name of the ring ("default" or a
/// named wheel, with " > folder" appended for a sub-ring).
#[derive(Debug, Clone, PartialEq)]
pub enum WheelSpanIssue {
    /// Explicit spans (each floored at the minimum) sum past 360°.
    SumOver { wheel: String, sum_deg: f32 },
    /// A slice's span resolves below the minimum and will be hard to hit.
    TooNarrow { wheel: String, label: String, span_deg: f32 },
    /// No span-less slice to absorb the remainder, and the explicit spans
    /// don't already fill 360° — the ring gets scaled to close.
    DoesNotClose { wheel: String, sum_deg: f32 },
    /// A `back` slice sits on the top-level ring, which has no parent to
    /// ascend to — it will never do anything.
    BackAtTopLevel { wheel: String, label: String },
    /// More than one `back` slice in a ring — only the ascend behavior is
    /// shared; extras are redundant.
    MultipleBack { wheel: String, count: usize },
}

/// Check one ring's slices for span problems, recursing into folders.
/// Pure and frontend-free so both the load-time validator (core) and the
/// editor (frontend) can call it. Mirrors the resolver's remainder-split so
/// the warnings match what the wheel will actually do.
pub fn validate_wheel_spans(wheel: &str, slices: &[WheelSlice]) -> Vec<WheelSpanIssue> {
    let mut issues = Vec::new();
    validate_ring(wheel, slices, false, &mut issues);
    issues
}

fn validate_ring(
    wheel: &str,
    slices: &[WheelSlice],
    in_folder: bool,
    issues: &mut Vec<WheelSpanIssue>,
) {
    // Back-slice sanity: a Back on the top ring has nothing to ascend to,
    // and more than one Back in a ring is redundant.
    let back_count = slices.iter().filter(|s| s.back).count();
    if !in_folder {
        for slice in slices.iter().filter(|s| s.back) {
            issues.push(WheelSpanIssue::BackAtTopLevel {
                wheel: wheel.to_string(),
                label: slice.label.clone(),
            });
        }
    }
    if back_count > 1 {
        issues.push(WheelSpanIssue::MultipleBack {
            wheel: wheel.to_string(),
            count: back_count,
        });
    }
    if !slices.is_empty() {
        // Explicit spans, each floored at the minimum (the resolver does the
        // same before splitting the remainder).
        let explicit: Vec<Option<f32>> = slices
            .iter()
            .map(|s| s.span.map(|v| v.max(WHEEL_MIN_SPAN_DEG)))
            .collect();
        let explicit_sum: f32 = explicit.iter().flatten().sum();
        let free_count = explicit.iter().filter(|s| s.is_none()).count();

        if explicit_sum > 360.0 + 1e-3 {
            issues.push(WheelSpanIssue::SumOver { wheel: wheel.to_string(), sum_deg: explicit_sum });
        } else if free_count == 0 && (explicit_sum - 360.0).abs() > 0.5 {
            issues.push(WheelSpanIssue::DoesNotClose {
                wheel: wheel.to_string(),
                sum_deg: explicit_sum,
            });
        } else if free_count > 0 {
            // Each free slice's resolved share; warn if it lands sub-minimum.
            let free_each = (360.0 - explicit_sum) / free_count as f32;
            if free_each < WHEEL_MIN_SPAN_DEG - 1e-3 {
                for slice in slices.iter().filter(|s| s.span.is_none()) {
                    issues.push(WheelSpanIssue::TooNarrow {
                        wheel: wheel.to_string(),
                        label: slice.label.clone(),
                        span_deg: free_each.max(0.0),
                    });
                }
            }
        }
        // An explicit span written below the minimum is clamped up by the
        // resolver — warn so the user knows their number was adjusted.
        for slice in slices {
            if let Some(span) = slice.span {
                if span < WHEEL_MIN_SPAN_DEG - 1e-3 {
                    issues.push(WheelSpanIssue::TooNarrow {
                        wheel: wheel.to_string(),
                        label: slice.label.clone(),
                        span_deg: span,
                    });
                }
            }
        }
    }
    // Recurse into folders, naming the sub-ring by its folder label.
    for slice in slices {
        if slice.is_folder() {
            let sub = format!("{wheel} > {}", slice.label);
            validate_ring(&sub, &slice.slices, true, issues);
        }
    }
}

impl WheelSpanIssue {
    /// One-line advisory for a system message / editor status.
    pub fn message(&self) -> String {
        match self {
            WheelSpanIssue::SumOver { wheel, sum_deg } => format!(
                "Wheel '{wheel}' slice spans sum to {:.0}° (over 360) — the wheel will scale them to fit.",
                sum_deg
            ),
            WheelSpanIssue::TooNarrow { wheel, label, span_deg } => format!(
                "Wheel '{wheel}' slice '{label}' is {:.0}° (under the {:.0}° minimum) — it may be hard to hit.",
                span_deg, WHEEL_MIN_SPAN_DEG
            ),
            WheelSpanIssue::DoesNotClose { wheel, sum_deg } => format!(
                "Wheel '{wheel}' spans sum to {:.0}° with no flexible slice — the wheel will scale them to fill 360°.",
                sum_deg
            ),
            WheelSpanIssue::BackAtTopLevel { wheel, label } => format!(
                "Wheel '{wheel}' has a Back slice '{label}' at the top level — there's no level to go up to, so it does nothing.",
            ),
            WheelSpanIssue::MultipleBack { wheel, count } => format!(
                "Wheel '{wheel}' has {count} Back slices — only one is needed; the extras just take up seats.",
            ),
        }
    }
}

/// Rumble (haptics) event map: pattern per game event. Patterns:
/// "off", the built-ins ("short", "long", "double"), or the name of a
/// user-defined entry in `patterns`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RumbleConfig {
    #[serde(default = "default_rumble_enabled")]
    pub enabled: bool,
    #[serde(default = "default_rumble_short")]
    pub roundtime_end: String,
    #[serde(default = "default_rumble_long")]
    pub stunned: String,
    #[serde(default = "default_rumble_double")]
    pub death: String,
    /// User-defined patterns, selectable anywhere a pattern name is
    /// (event rows, highlight rules). Built-in names win on collision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<RumblePattern>,
}

/// A user-defined vibration pattern: `pulses` buzzes of `strength`
/// lasting `pulse_ms` each, separated by `gap_ms` of silence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RumblePattern {
    pub name: String,
    #[serde(default = "default_pattern_strength")]
    pub strength: f32,
    #[serde(default = "default_pattern_pulse_ms")]
    pub pulse_ms: u32,
    #[serde(default = "default_pattern_pulses")]
    pub pulses: u32,
    #[serde(default = "default_pattern_gap_ms")]
    pub gap_ms: u32,
}

fn default_pattern_strength() -> f32 {
    0.7
}
fn default_pattern_pulse_ms() -> u32 {
    200
}
fn default_pattern_pulses() -> u32 {
    1
}
fn default_pattern_gap_ms() -> u32 {
    120
}

impl Default for RumblePattern {
    fn default() -> Self {
        Self {
            name: String::new(),
            strength: default_pattern_strength(),
            pulse_ms: default_pattern_pulse_ms(),
            pulses: default_pattern_pulses(),
            gap_ms: default_pattern_gap_ms(),
        }
    }
}

impl RumbleConfig {
    /// Resolve a pattern name to `(strength 0..=1, pulse_ms, pulses,
    /// gap_ms)`. Built-ins take precedence over user patterns of the
    /// same name; "off" and unknown names resolve to `None`. Custom
    /// values are clamped to hardware-sane ranges here so every
    /// frontend inherits the same limits.
    pub fn resolve_pattern(&self, name: &str) -> Option<(f32, u32, u32, u32)> {
        match name {
            "short" => Some((0.5, 160, 1, 120)),
            "long" => Some((0.9, 450, 1, 120)),
            "double" => Some((0.8, 180, 2, 120)),
            _ => self.patterns.iter().find(|p| p.name == name).map(|p| {
                (
                    p.strength.clamp(0.05, 1.0),
                    p.pulse_ms.clamp(20, 2000),
                    p.pulses.clamp(1, 8),
                    p.gap_ms.min(2000),
                )
            }),
        }
    }
}

fn default_rumble_enabled() -> bool {
    true
}
fn default_rumble_short() -> String {
    "short".to_string()
}
fn default_rumble_long() -> String {
    "long".to_string()
}
fn default_rumble_double() -> String {
    "double".to_string()
}

impl Default for RumbleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            roundtime_end: default_rumble_short(),
            stunned: default_rumble_long(),
            death: default_rumble_double(),
            patterns: Vec::new(),
        }
    }
}

/// Controller input-feel tuning (`[controller_tuning]`). Every field is
/// optional; an absent table (or an absent field) uses the shipped
/// default, which mostly reproduces the historical hardcoded feel — the
/// one intentional change is that `aim_dwell_ms`/`nav_dwell_ms` now gate
/// when a wheel slice commits, so sweeping across the ring no longer
/// flickers every slice into a fireable state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningConfig {
    /// Which analog stick walks the eight compass directions: "left" or
    /// "right". The other stick then aims the wheel / scrolls the story.
    #[serde(default = "default_movement_stick")]
    pub movement_stick: String,
    /// Screen anchor for the reserved "back" slice inside a non-top wheel
    /// folder: one of "up", "down", "left", "right", "up-left",
    /// "up-right", "down-left", "down-right". Back is always a real,
    /// aimable slice; the ring is rotated so it sits nearest this anchor
    /// and the other slices fill in around it, keeping Back in the same
    /// place at every level.
    #[serde(default = "default_back_slice")]
    pub back_slice: String,
    /// Stick deflection, as a percent 0–100, before a wheel slice
    /// registers (the wheel's dead zone). Stored as a percent for the
    /// config/editor; divided by 100 at read time.
    #[serde(default = "default_deadzone")]
    pub deadzone: u8,
    /// Hold a leaf slice this long (ms) before it commits and arms to
    /// fire on release. Slices merely swept through never reach it.
    #[serde(default = "default_aim_dwell_ms")]
    pub aim_dwell_ms: u32,
    /// Hold a folder (or the Back slice) this long (ms) before it
    /// auto-descends (or auto-ascends). Shared by both navigation moves.
    #[serde(default = "default_nav_dwell_ms")]
    pub nav_dwell_ms: u32,
    /// Suppress a repeat fire for this long (ms) after one fires, so a
    /// noisy button contact can't double-send.
    #[serde(default = "default_fire_debounce_ms")]
    pub fire_debounce_ms: u32,
    /// After the wheel button comes up the aiming stick is usually still
    /// deflected; this grace (ms) is the window over which movement stays
    /// seeded/hushed so firing the wheel doesn't also walk a direction.
    #[serde(default = "default_release_grace_ms")]
    pub release_grace_ms: u32,
    /// How a committed leaf slice fires. `"release"` (default) fires when
    /// the wheel button comes up; `"edge"` fires the instant deflection
    /// crosses `edge_threshold` (no dwell wait); `"retract"` dwells to
    /// commit, then fires as soon as deflection drops `retract_delta`
    /// below its peak (a small inward flick). Folders always descend on
    /// dwell and are never fired by edge/retract; cancel is unchanged.
    #[serde(default = "default_fire_mode")]
    pub fire_mode: String,
    /// For `fire_mode = "edge"`: stick deflection (percent of full throw)
    /// at which a leaf fires. Also the floor beneath which `retract` won't
    /// consider a leaf "held out" for peak tracking.
    #[serde(default = "default_edge_threshold")]
    pub edge_threshold: u8,
    /// For `fire_mode = "retract"`: how far (percent points) deflection
    /// must fall below its tracked peak to fire the committed leaf.
    #[serde(default = "default_retract_delta")]
    pub retract_delta: u8,
}

fn default_movement_stick() -> String {
    "left".to_string()
}
fn default_back_slice() -> String {
    "down".to_string()
}
fn default_deadzone() -> u8 {
    50
}
fn default_aim_dwell_ms() -> u32 {
    150
}
fn default_nav_dwell_ms() -> u32 {
    150
}
fn default_fire_debounce_ms() -> u32 {
    300
}
fn default_release_grace_ms() -> u32 {
    40
}
fn default_fire_mode() -> String {
    "release".to_string()
}
fn default_edge_threshold() -> u8 {
    90
}
fn default_retract_delta() -> u8 {
    10
}

impl Default for TuningConfig {
    fn default() -> Self {
        Self {
            movement_stick: default_movement_stick(),
            back_slice: default_back_slice(),
            deadzone: default_deadzone(),
            aim_dwell_ms: default_aim_dwell_ms(),
            nav_dwell_ms: default_nav_dwell_ms(),
            fire_debounce_ms: default_fire_debounce_ms(),
            release_grace_ms: default_release_grace_ms(),
            fire_mode: default_fire_mode(),
            edge_threshold: default_edge_threshold(),
            retract_delta: default_retract_delta(),
        }
    }
}

/// Per-wheel metadata (`[controller_wheels_meta.<name>]`): which button
/// opens the wheel and which stick aims it. Stored separately from the
/// wheel's slice array (`[[controller_wheels.<name>]]`) so old configs,
/// which have only the slice array, load unchanged with both fields None.
///
/// `button` is editor metadata — the runtime binding authority stays
/// `[controller]` (the wheel opens when its `controller_wheel:<name>`
/// action's button is held). The editor writes both, and a load-time
/// check warns when they disagree or two wheels claim one button.
/// `stick` is authoritative (nothing else stores it): while the wheel is
/// open that stick aims it, overriding the global movement-stick choice;
/// None falls back to the non-movement stick.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WheelMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stick: Option<String>,
    /// Optional ring rotation in degrees (0 = up, clockwise) applied to
    /// the whole top-level layout, so a wheel's slices can be anchored
    /// wherever the thumb likes them. None = today's slice-0-at-top.
    /// Inside folders the Back anchor keeps owning the rotation (Back
    /// stays put across levels) unless `back_slice = "none"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f32>,
}

/// Keybinds for menu system (popups, browsers, forms, editors)
/// These are separate from game keybinds and only active when menus have focus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuKeybinds {
    // Navigation
    #[serde(default = "default_navigate_up")]
    pub navigate_up: String,
    #[serde(default = "default_navigate_down")]
    pub navigate_down: String,
    #[serde(default = "default_navigate_left")]
    pub navigate_left: String,
    #[serde(default = "default_navigate_right")]
    pub navigate_right: String,
    #[serde(default = "default_page_up")]
    pub page_up: String,
    #[serde(default = "default_page_down")]
    pub page_down: String,
    #[serde(default = "default_home")]
    pub home: String,
    #[serde(default = "default_end")]
    pub end: String,

    // Field Navigation
    #[serde(default = "default_next_field")]
    pub next_field: String,
    #[serde(default = "default_previous_field")]
    pub previous_field: String,

    // Actions
    #[serde(default = "default_select")]
    pub select: String,
    #[serde(default = "default_cancel")]
    pub cancel: String,
    #[serde(default = "default_save")]
    pub save: String,
    #[serde(default = "default_delete")]
    pub delete: String,

    // Text Editing (Clipboard)
    #[serde(default = "default_select_all")]
    pub select_all: String,
    #[serde(default = "default_copy")]
    pub copy: String,
    #[serde(default = "default_cut")]
    pub cut: String,
    #[serde(default = "default_paste")]
    pub paste: String,

    // Toggles/Cycling
    #[serde(default = "default_toggle")]
    pub toggle: String,
    #[serde(default = "default_toggle_filter")]
    pub toggle_filter: String,
    #[serde(default = "default_cycle_forward")]
    pub cycle_forward: String,
    #[serde(default = "default_cycle_backward")]
    pub cycle_backward: String,

    // Reordering (WindowEditor)
    #[serde(default = "default_move_up")]
    pub move_up: String,
    #[serde(default = "default_move_down")]
    pub move_down: String,

    // List Management (WindowEditor)
    #[serde(default = "default_add")]
    pub add: String,
    #[serde(default = "default_edit")]
    pub edit: String,
}

// Default keybind functions
fn default_navigate_up() -> String {
    "Up".to_string()
}
fn default_navigate_down() -> String {
    "Down".to_string()
}
fn default_navigate_left() -> String {
    "Left".to_string()
}
fn default_navigate_right() -> String {
    "Right".to_string()
}
fn default_page_up() -> String {
    "PageUp".to_string()
}
fn default_page_down() -> String {
    "PageDown".to_string()
}
fn default_home() -> String {
    "Home".to_string()
}
fn default_end() -> String {
    "End".to_string()
}
fn default_next_field() -> String {
    "Tab".to_string()
}
fn default_previous_field() -> String {
    "Shift+Tab".to_string()
}
fn default_select() -> String {
    "Enter".to_string()
}
fn default_cancel() -> String {
    "Esc".to_string()
}
fn default_save() -> String {
    "Ctrl+s".to_string()
}
fn default_delete() -> String {
    "Delete".to_string()
}
fn default_select_all() -> String {
    "Ctrl+A".to_string()
}
fn default_copy() -> String {
    "Ctrl+C".to_string()
}
fn default_cut() -> String {
    "Ctrl+X".to_string()
}
fn default_paste() -> String {
    "Ctrl+V".to_string()
}
fn default_toggle() -> String {
    "Space".to_string()
}
fn default_toggle_filter() -> String {
    "F".to_string()
}
fn default_cycle_forward() -> String {
    "Right".to_string()
}
fn default_cycle_backward() -> String {
    "Left".to_string()
}
fn default_move_up() -> String {
    "Shift+Up".to_string()
}
fn default_move_down() -> String {
    "Shift+Down".to_string()
}
fn default_add() -> String {
    "A".to_string()
}
fn default_edit() -> String {
    "E".to_string()
}

impl Default for MenuKeybinds {
    fn default() -> Self {
        Self {
            navigate_up: default_navigate_up(),
            navigate_down: default_navigate_down(),
            navigate_left: default_navigate_left(),
            navigate_right: default_navigate_right(),
            page_up: default_page_up(),
            page_down: default_page_down(),
            home: default_home(),
            end: default_end(),
            next_field: default_next_field(),
            previous_field: default_previous_field(),
            select: default_select(),
            cancel: default_cancel(),
            save: default_save(),
            delete: default_delete(),
            select_all: default_select_all(),
            copy: default_copy(),
            cut: default_cut(),
            paste: default_paste(),
            toggle: default_toggle(),
            toggle_filter: default_toggle_filter(),
            cycle_forward: default_cycle_forward(),
            cycle_backward: default_cycle_backward(),
            move_up: default_move_up(),
            move_down: default_move_down(),
            add: default_add(),
            edit: default_edit(),
        }
    }
}

impl MenuKeybinds {
    /// Resolve a KeyEvent to a MenuAction based on the current context
    pub fn resolve_action(
        &self,
        key: &crate::data::input::KeyEvent,
        context: crate::core::menu_actions::ActionContext,
    ) -> crate::core::menu_actions::MenuAction {
        use crate::core::menu_actions::{key_event_to_string, ActionContext, MenuAction};

        let key_str = key_event_to_string(*key);
        let key_lower = key_str.to_lowercase();

        // DEBUG: Log what we're resolving
        tracing::debug!(
            "🔍 resolve_action: key_str='{}', context={:?}",
            key_str,
            context
        );
        tracing::debug!(
            "   Config values: navigate_up='{}', navigate_down='{}', select='{}', cancel='{}'",
            self.navigate_up,
            self.navigate_down,
            self.select,
            self.cancel
        );

        // Special handling for BackTab (Shift+Tab)
        if matches!(key.code, KeyCode::BackTab)
            && (key_lower == self.previous_field.to_lowercase() || key_lower == "shift+tab")
        {
            return MenuAction::PreviousField;
        }

        // Context-specific bindings first (override general bindings)
        match context {
            ActionContext::Dropdown => {
                // In dropdown, Up/Down cycle through options instead of navigating
                if key_lower == self.navigate_up.to_lowercase() {
                    return MenuAction::NavigateUp; // Will be interpreted as cycle prev
                }
                if key_lower == self.navigate_down.to_lowercase() {
                    return MenuAction::NavigateDown; // Will be interpreted as cycle next
                }
            }
            ActionContext::TextInput => {
                // Clipboard operations only valid in text input
                if key_lower == self.select_all.to_lowercase() {
                    return MenuAction::SelectAll;
                }
                if key_lower == self.copy.to_lowercase() {
                    return MenuAction::Copy;
                }
                if key_lower == self.cut.to_lowercase() {
                    return MenuAction::Cut;
                }
                if key_lower == self.paste.to_lowercase() {
                    return MenuAction::Paste;
                }
            }
            _ => {}
        }

        // Global menu keybindings
        if key_lower == self.cancel.to_lowercase() {
            return MenuAction::Cancel;
        }
        if key_lower == self.save.to_lowercase() {
            return MenuAction::Save;
        }
        if key_lower == self.select.to_lowercase() {
            return MenuAction::Select;
        }
        if key_lower == self.delete.to_lowercase() {
            return MenuAction::Delete;
        }

        if key_lower == self.navigate_up.to_lowercase() {
            return MenuAction::NavigateUp;
        }
        if key_lower == self.navigate_down.to_lowercase() {
            return MenuAction::NavigateDown;
        }
        if key_lower == self.navigate_left.to_lowercase() {
            return MenuAction::NavigateLeft;
        }
        if key_lower == self.navigate_right.to_lowercase() {
            return MenuAction::NavigateRight;
        }
        if key_lower == self.page_up.to_lowercase() {
            return MenuAction::PageUp;
        }
        if key_lower == self.page_down.to_lowercase() {
            return MenuAction::PageDown;
        }
        if key_lower == self.home.to_lowercase() {
            return MenuAction::Home;
        }
        if key_lower == self.end.to_lowercase() {
            return MenuAction::End;
        }

        if key_lower == self.next_field.to_lowercase() {
            return MenuAction::NextField;
        }
        if key_lower == self.previous_field.to_lowercase() {
            return MenuAction::PreviousField;
        }

        if key_lower == self.toggle.to_lowercase() {
            return MenuAction::Toggle;
        }

        if key_lower == self.move_up.to_lowercase() {
            return MenuAction::MoveUp;
        }
        if key_lower == self.move_down.to_lowercase() {
            return MenuAction::MoveDown;
        }

        // Browser-only actions (don't trigger in forms where text input is needed)
        if matches!(context, ActionContext::Browser) {
            if key_lower == self.add.to_lowercase() {
                return MenuAction::Add;
            }
            if key_lower == self.edit.to_lowercase() {
                return MenuAction::Edit;
            }
            if key_lower == self.toggle_filter.to_lowercase() {
                return MenuAction::ToggleFilter;
            }
        }

        if key_lower == self.cycle_forward.to_lowercase() {
            return MenuAction::CycleForward;
        }
        if key_lower == self.cycle_backward.to_lowercase() {
            return MenuAction::CycleBackward;
        }

        // No matching keybind
        MenuAction::None
    }
}

impl KeyAction {
    /// Action names that execute fully inside AppCore — the set that does
    /// something useful from a controller button (everything else is
    /// keyboard-widget-level and would no-op). Drives the controller
    /// editor's action dropdown; a test keeps every entry parseable.
    pub const CONTROLLER_ACTION_NAMES: &'static [&'static str] = &[
        "interact_mode",
        "interact_select",
        "menu_up",
        "menu_down",
        "menu_left",
        "menu_right",
        "menu_cancel",
        "controller_shift",
        // controller_wheel / controller_wheel:<name> are configured in the
        // Wheels tab (each wheel's "Opens with" button writes the matching
        // [controller] entry), so they are intentionally NOT offered here —
        // that keeps a single source of truth for wheel buttons.
        "controller_overlay",
        "stop_travel",
        "scroll_current_window_up_page",
        "scroll_current_window_down_page",
        "scroll_current_window_up_one",
        "scroll_current_window_down_one",
        "scroll_current_window_home",
        "scroll_current_window_end",
        "toggle_sounds",
        "toggle_performance_stats",
        "tts_next",
        "tts_previous",
        "tts_next_unread",
        "tts_stop",
        "tts_mute_toggle",
        "tts_increase_rate",
        "tts_decrease_rate",
        "tts_increase_volume",
        "tts_decrease_volume",
    ];

    pub fn from_str(action: &str) -> Option<Self> {
        match action {
            "send_command" => Some(Self::SendCommand),
            "cursor_left" => Some(Self::CursorLeft),
            "cursor_right" => Some(Self::CursorRight),
            "cursor_word_left" => Some(Self::CursorWordLeft),
            "cursor_word_right" => Some(Self::CursorWordRight),
            "cursor_home" => Some(Self::CursorHome),
            "cursor_end" => Some(Self::CursorEnd),
            "cursor_backspace" => Some(Self::CursorBackspace),
            "cursor_delete" => Some(Self::CursorDelete),
            "cursor_delete_word" => Some(Self::CursorDeleteWord),
            "cursor_clear_line" => Some(Self::CursorClearLine),
            "previous_command" => Some(Self::PreviousCommand),
            "next_command" => Some(Self::NextCommand),
            "send_last_command" => Some(Self::SendLastCommand),
            "send_second_last_command" => Some(Self::SendSecondLastCommand),
            "switch_current_window" => Some(Self::SwitchCurrentWindow),
            "scroll_current_window_up_one" => Some(Self::ScrollCurrentWindowUpOne),
            "scroll_current_window_down_one" => Some(Self::ScrollCurrentWindowDownOne),
            "scroll_current_window_up_page" => Some(Self::ScrollCurrentWindowUpPage),
            "scroll_current_window_down_page" => Some(Self::ScrollCurrentWindowDownPage),
            "scroll_current_window_home" => Some(Self::ScrollCurrentWindowHome),
            "scroll_current_window_end" => Some(Self::ScrollCurrentWindowEnd),
            "start_search" => Some(Self::StartSearch),
            "next_search_match" => Some(Self::NextSearchMatch),
            "prev_search_match" => Some(Self::PrevSearchMatch),
            "clear_search" => Some(Self::ClearSearch),
            "next_tab" => Some(Self::NextTab),
            "prev_tab" => Some(Self::PrevTab),
            "next_unread_tab" => Some(Self::NextUnreadTab),
            "copy" => Some(Self::Copy),
            "paste" => Some(Self::Paste),
            "select_all" => Some(Self::SelectAll),
            "toggle_performance_stats" => Some(Self::TogglePerformanceStats),
            "toggle_sounds" => Some(Self::ToggleSounds),
            "stop_travel" => Some(Self::StopTravel),
            "interact_mode" => Some(Self::InteractMode),
            "interact_select" => Some(Self::InteractSelect),
            "menu_up" => Some(Self::MenuUp),
            "menu_down" => Some(Self::MenuDown),
            "menu_left" => Some(Self::MenuLeft),
            "menu_right" => Some(Self::MenuRight),
            "menu_cancel" => Some(Self::MenuCancel),
            "controller_shift" => Some(Self::ControllerShift),
            // "controller_wheel" opens the default wheel;
            // "controller_wheel:<name>" opens a named [controller_wheels.<name>].
            s if s == "controller_wheel" || s.starts_with("controller_wheel:") => {
                Some(Self::ControllerWheel)
            }
            "controller_overlay" => Some(Self::ControllerOverlay),
            "tts_next" => Some(Self::TtsNext),
            "tts_previous" => Some(Self::TtsPrevious),
            "tts_next_unread" => Some(Self::TtsNextUnread),
            "tts_stop" => Some(Self::TtsStop),
            "tts_pause_resume" => Some(Self::TtsStop), // Legacy support
            "tts_mute_toggle" => Some(Self::TtsMuteToggle),
            "tts_increase_rate" => Some(Self::TtsIncreaseRate),
            "tts_decrease_rate" => Some(Self::TtsDecreaseRate),
            "tts_increase_volume" => Some(Self::TtsIncreaseVolume),
            "tts_decrease_volume" => Some(Self::TtsDecreaseVolume),
            _ => None,
        }
    }
}

/// Parse a key string like "ctrl+f" or "num_1" into KeyCode and KeyModifiers
pub fn parse_key_string(key_str: &str) -> Option<(KeyCode, KeyModifiers)> {
    // Normalize to lowercase for consistent comparisons
    let key_str_lower = key_str.to_lowercase();
    let key_str = key_str_lower.as_str();

    // Special case: "num_+" contains a '+' but it's not a modifier separator
    // If the string is exactly a numpad key (no modifiers), handle it first
    if key_str.starts_with("num_")
        && !key_str.contains("shift+")
        && !key_str.contains("ctrl+")
        && !key_str.contains("alt+")
    {
        let key_code = match key_str {
            "num_0" => KeyCode::Keypad0,
            "num_1" => KeyCode::Keypad1,
            "num_2" => KeyCode::Keypad2,
            "num_3" => KeyCode::Keypad3,
            "num_4" => KeyCode::Keypad4,
            "num_5" => KeyCode::Keypad5,
            "num_6" => KeyCode::Keypad6,
            "num_7" => KeyCode::Keypad7,
            "num_8" => KeyCode::Keypad8,
            "num_9" => KeyCode::Keypad9,
            "num_." => KeyCode::KeypadPeriod,
            "num_+" => KeyCode::KeypadPlus,
            "num_-" => KeyCode::KeypadMinus,
            "num_*" => KeyCode::KeypadMultiply,
            "num_/" => KeyCode::KeypadDivide,
            _ => return None,
        };
        return Some((key_code, KeyModifiers::NONE));
    }

    // For keys with modifiers, we need to carefully parse
    // Split by + but be aware that num_+ contains a literal +
    let parts: Vec<&str> = key_str.split('+').collect();
    let mut modifiers = KeyModifiers::NONE;
    let mut key_part = key_str;

    // Parse modifiers
    if parts.len() > 1 {
        for part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "alt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                _ => return None,
            }
        }
        key_part = parts[parts.len() - 1];
    }

    // Parse the actual key
    let key_code = match key_part {
        // Special keys
        "enter" => KeyCode::Enter,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "tab" => KeyCode::Tab,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "page_up" | "pageup" => KeyCode::PageUp,
        "page_down" | "pagedown" => KeyCode::PageDown,

        // Numpad keys (when used with modifiers like shift+num_1)
        "num_0" => KeyCode::Keypad0,
        "num_1" => KeyCode::Keypad1,
        "num_2" => KeyCode::Keypad2,
        "num_3" => KeyCode::Keypad3,
        "num_4" => KeyCode::Keypad4,
        "num_5" => KeyCode::Keypad5,
        "num_6" => KeyCode::Keypad6,
        "num_7" => KeyCode::Keypad7,
        "num_8" => KeyCode::Keypad8,
        "num_9" => KeyCode::Keypad9,
        "num_." => KeyCode::KeypadPeriod,
        "num_+" => KeyCode::KeypadPlus,
        "num_-" => KeyCode::KeypadMinus,
        "num_*" => KeyCode::KeypadMultiply,
        "num_/" => KeyCode::KeypadDivide,

        // Function keys
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),

        // Single character
        s if s.len() == 1 => {
            let ch = s.chars().next().unwrap();
            KeyCode::Char(ch)
        }

        _ => return None,
    };

    Some((key_code, modifiers))
}

impl Config {
    /// Load common (global) keybinds that apply to all characters
    /// Returns: HashMap of global keybinds, or empty if file doesn't exist
    pub fn load_common_keybinds() -> Result<HashMap<String, KeyBindAction>> {
        let path = Self::common_keybinds_path()?;

        if !path.exists() {
            return Ok(HashMap::new());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common keybinds: {:?}", path))?;

        // Parse the entire TOML file to get the [user] section
        let toml_value: toml::Value =
            toml::from_str(&contents).context("Failed to parse common keybinds TOML")?;

        // Extract [user] section if it exists
        if let Some(user_section) = toml_value.get("user") {
            let keybinds: HashMap<String, KeyBindAction> = user_section
                .clone()
                .try_into()
                .context("Failed to parse [user] section from common keybinds")?;
            Ok(keybinds)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Load keybinds for a character, merging global + character-specific
    /// Character-specific keybinds override global ones with the same key
    pub fn load_keybinds(character: Option<&str>) -> Result<HashMap<String, KeyBindAction>> {
        // Start with global/common keybinds
        let mut keybinds = Self::load_common_keybinds()?;

        // Load character-specific keybinds
        let keybinds_path = Self::keybinds_path(character)?;

        if keybinds_path.exists() {
            let contents =
                fs::read_to_string(&keybinds_path).context("Failed to read keybinds.toml")?;

            // Parse the entire TOML file to get the [user] section
            let toml_value: toml::Value =
                toml::from_str(&contents).context("Failed to parse keybinds.toml")?;

            // Extract [user] section if it exists
            if let Some(user_section) = toml_value.get("user") {
                let character_keybinds: HashMap<String, KeyBindAction> = user_section
                    .clone()
                    .try_into()
                    .context("Failed to parse [user] section")?;
                // Character keybinds override global (HashMap::extend)
                keybinds.extend(character_keybinds);
            }
        } else if keybinds.is_empty() {
            // No global and no character keybinds - use embedded defaults
            keybinds = toml::from_str(DEFAULT_KEYBINDS).unwrap_or_else(|_| default_keybinds());
        }

        Ok(keybinds)
    }

    /// Load only character-specific keybinds (not merged with global)
    /// Returns: HashMap of character keybinds, or empty if file doesn't exist
    pub fn load_character_keybinds_only(
        character: Option<&str>,
    ) -> Result<HashMap<String, KeyBindAction>> {
        let keybinds_path = Self::keybinds_path(character)?;

        if !keybinds_path.exists() {
            return Ok(HashMap::new());
        }

        let contents = fs::read_to_string(&keybinds_path)
            .with_context(|| format!("Failed to read character keybinds: {:?}", keybinds_path))?;

        // Parse the entire TOML file to get the [user] section
        let toml_value: toml::Value =
            toml::from_str(&contents).context("Failed to parse character keybinds TOML")?;

        // Extract [user] section if it exists
        if let Some(user_section) = toml_value.get("user") {
            let keybinds: HashMap<String, KeyBindAction> = user_section
                .clone()
                .try_into()
                .context("Failed to parse [user] section from character keybinds")?;
            Ok(keybinds)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Load the global controller.toml as a plain `toml::value::Table` for
    /// the section savers (creating the parent dir when the file is absent).
    /// A file that fails to parse yields an empty table rather than an error
    /// so a single bad edit can't wedge every controller save.
    fn load_controller_table() -> Result<(std::path::PathBuf, toml::value::Table)> {
        let path = Self::common_controller_path()?;
        let table = if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read controller file: {:?}", path))?;
            toml::from_str(&contents).unwrap_or_else(|_| toml::value::Table::new())
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {:?}", parent))?;
            }
            toml::value::Table::new()
        };
        Ok((path, table))
    }

    /// Serialize and atomically write a controller.toml table.
    fn write_controller_table(path: &std::path::Path, table: &toml::value::Table) -> Result<()> {
        let contents =
            toml::to_string_pretty(table).context("Failed to serialize controller config")?;
        write_atomic(path, contents)
            .with_context(|| format!("Failed to write controller file: {:?}", path))?;
        Ok(())
    }

    /// Load the radial wheel slices from `[[controller_wheel]]` of the
    /// global controller.toml, falling back to the shipped defaults when
    /// absent.
    pub fn load_controller_wheel() -> Result<Vec<WheelSlice>> {
        let slices_from = |contents: &str| -> Option<Vec<WheelSlice>> {
            let toml_value: toml::Value = toml::from_str(contents).ok()?;
            let array = toml_value.get("controller_wheel")?;
            array.clone().try_into().ok()
        };
        let path = Self::common_controller_path()?;
        if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read controller file: {:?}", path))?;
            if let Some(slices) = slices_from(&contents) {
                return Ok(slices);
            }
        }
        Ok(slices_from(DEFAULT_CONTROLLER).unwrap_or_default())
    }

    /// Load the overlay legend's curated entries from
    /// `[controller_overlay] buttons` of the global keybinds.toml:
    /// button names, with a `shift/` prefix for shift-layer entries.
    pub fn load_controller_overlay() -> Result<Vec<String>> {
        let list_from = |contents: &str| -> Option<Vec<String>> {
            let toml_value: toml::Value = toml::from_str(contents).ok()?;
            toml_value
                .get("controller_overlay")?
                .get("buttons")?
                .clone()
                .try_into()
                .ok()
        };
        let path = Self::common_controller_path()?;
        if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read controller file: {:?}", path))?;
            if let Some(list) = list_from(&contents) {
                return Ok(list);
            }
        }
        Ok(list_from(DEFAULT_CONTROLLER).unwrap_or_default())
    }

    /// Replace the overlay legend's curated entry list.
    pub fn save_controller_overlay(buttons: &[String]) -> Result<()> {
        let (path, mut toml_table) = Self::load_controller_table()?;
        let section = toml_table
            .entry("controller_overlay".to_string())
            .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
        if let toml::Value::Table(table) = section {
            table.insert(
                "buttons".to_string(),
                toml::Value::try_from(buttons).context("Failed to serialize overlay list")?,
            );
        }
        Self::write_controller_table(&path, &toml_table)
    }

    /// Load the rumble event map from `[controller_rumble]` of the global
    /// controller.toml (shipped defaults when absent).
    pub fn load_controller_rumble() -> Result<RumbleConfig> {
        let section_from = |contents: &str| -> Option<RumbleConfig> {
            let toml_value: toml::Value = toml::from_str(contents).ok()?;
            toml_value.get("controller_rumble")?.clone().try_into().ok()
        };
        let path = Self::common_controller_path()?;
        if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read controller file: {:?}", path))?;
            if let Some(config) = section_from(&contents) {
                return Ok(config);
            }
        }
        Ok(section_from(DEFAULT_CONTROLLER).unwrap_or_default())
    }

    /// Replace the `[controller_rumble]` section.
    pub fn save_controller_rumble(rumble: &RumbleConfig) -> Result<()> {
        let (path, mut toml_table) = Self::load_controller_table()?;
        toml_table.insert(
            "controller_rumble".to_string(),
            toml::Value::try_from(rumble).context("Failed to serialize rumble config")?,
        );
        Self::write_controller_table(&path, &toml_table)
    }

    /// Load the input-feel tuning from `[controller_tuning]` of the global
    /// controller.toml (shipped defaults when absent).
    pub fn load_controller_tuning() -> Result<TuningConfig> {
        let section_from = |contents: &str| -> Option<TuningConfig> {
            let toml_value: toml::Value = toml::from_str(contents).ok()?;
            toml_value.get("controller_tuning")?.clone().try_into().ok()
        };
        let path = Self::common_controller_path()?;
        if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read controller file: {:?}", path))?;
            if let Some(config) = section_from(&contents) {
                return Ok(config);
            }
        }
        Ok(section_from(DEFAULT_CONTROLLER).unwrap_or_default())
    }

    /// Replace the `[controller_tuning]` section.
    pub fn save_controller_tuning(tuning: &TuningConfig) -> Result<()> {
        let (path, mut toml_table) = Self::load_controller_table()?;
        toml_table.insert(
            "controller_tuning".to_string(),
            toml::Value::try_from(tuning).context("Failed to serialize tuning config")?,
        );
        Self::write_controller_table(&path, &toml_table)
    }

    /// Load the named wheels from `[controller_wheels.<name>]` arrays of
    /// the global controller.toml (bound via "controller_wheel:<name>").
    pub fn load_controller_wheels() -> Result<HashMap<String, Vec<WheelSlice>>> {
        let wheels_from = |contents: &str| -> Option<HashMap<String, Vec<WheelSlice>>> {
            let toml_value: toml::Value = toml::from_str(contents).ok()?;
            let table = toml_value.get("controller_wheels")?;
            table.clone().try_into().ok()
        };
        let path = Self::common_controller_path()?;
        if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read controller file: {:?}", path))?;
            if let Some(wheels) = wheels_from(&contents) {
                return Ok(wheels);
            }
        }
        Ok(wheels_from(DEFAULT_CONTROLLER).unwrap_or_default())
    }

    /// Load per-wheel metadata from `[controller_wheels_meta.<name>]`
    /// (button/stick). Absent = empty map = today's behavior.
    pub fn load_controller_wheels_meta() -> Result<HashMap<String, WheelMeta>> {
        let meta_from = |contents: &str| -> Option<HashMap<String, WheelMeta>> {
            let toml_value: toml::Value = toml::from_str(contents).ok()?;
            let table = toml_value.get("controller_wheels_meta")?;
            table.clone().try_into().ok()
        };
        let path = Self::common_controller_path()?;
        if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read controller file: {:?}", path))?;
            if let Some(meta) = meta_from(&contents) {
                return Ok(meta);
            }
        }
        Ok(meta_from(DEFAULT_CONTROLLER).unwrap_or_default())
    }

    /// Replace the `[controller_wheels_meta]` section. Entries with both
    /// fields None are dropped so the section stays tidy.
    pub fn save_controller_wheels_meta(meta: &HashMap<String, WheelMeta>) -> Result<()> {
        let (path, mut toml_table) = Self::load_controller_table()?;
        let pruned: HashMap<&String, &WheelMeta> = meta
            .iter()
            .filter(|(_, m)| m.button.is_some() || m.stick.is_some() || m.start.is_some())
            .collect();
        if pruned.is_empty() {
            toml_table.remove("controller_wheels_meta");
        } else {
            toml_table.insert(
                "controller_wheels_meta".to_string(),
                toml::Value::try_from(&pruned).context("Failed to serialize wheel meta")?,
            );
        }
        Self::write_controller_table(&path, &toml_table)
    }

    /// The slice list at a folder path within a wheel. Key "" is the
    /// default wheel (`[[controller_wheel]]`, falling back to
    /// `[controller_wheels.default]`), anything else a named wheel.
    /// Canonical lookup shared by the GUI wheel and remote clients.
    pub fn wheel_level_slices(&self, key: &str, path: &[usize]) -> Option<&Vec<WheelSlice>> {
        let mut level = if key.is_empty() {
            if self.controller_wheel.is_empty() {
                self.controller_wheels.get("default")?
            } else {
                &self.controller_wheel
            }
        } else {
            self.controller_wheels.get(key)?
        };
        for &index in path {
            level = &level.get(index)?.slices;
        }
        Some(level)
    }

    /// Resolve a wheel pick from a remote client: `path` indexes down to
    /// a leaf slice, whose non-empty command is returned. Folder slices
    /// and empty commands resolve to None (nothing to fire).
    pub fn wheel_pick_command(&self, key: &str, path: &[usize]) -> Option<String> {
        let (&leaf, folders) = path.split_last()?;
        let slice = self.wheel_level_slices(key, folders)?.get(leaf)?;
        (!slice.is_folder() && !slice.command.is_empty()).then(|| slice.command.clone())
    }

    /// Replace one wheel's slice list in the global keybinds.toml:
    /// None = the default wheel ([[controller_wheel]]), Some(name) =
    /// [controller_wheels.<name>]. An empty slice list deletes a named
    /// wheel outright.
    /// Serialize a wheel slice array to a TOML fragment under the given
    /// top-level key, with nested folder `slices` emitted as inline arrays
    /// of inline tables. `toml::to_string_pretty` writes a folder's nested
    /// `[[key.slices]]` blocks in file order that, once re-parsed, can bind
    /// to a LATER sibling instead of the folder (it corrupted stance's
    /// children onto exp/health). Inline tables keep each slice's children
    /// syntactically inside that slice, so the parent-child grouping is
    /// unambiguous no matter the sibling order.
    fn wheel_slices_to_inline(slices: &[WheelSlice]) -> toml_edit::Value {
        use toml_edit::{Array, InlineTable, Value};
        let mut arr = Array::new();
        for slice in slices {
            let mut t = InlineTable::new();
            t.insert("label", Value::from(slice.label.clone()));
            if !slice.command.is_empty() {
                t.insert("command", Value::from(slice.command.clone()));
            }
            if let Some(color) = &slice.color {
                t.insert("color", Value::from(color.clone()));
            }
            if let Some(span) = slice.span {
                t.insert("span", Value::from(span as f64));
            }
            if let Some(inner) = slice.inner {
                t.insert("inner", Value::from(inner as i64));
            }
            if slice.back {
                t.insert("back", Value::from(true));
            }
            if !slice.slices.is_empty() {
                t.insert("slices", Self::wheel_slices_to_inline(&slice.slices));
            }
            arr.push(Value::InlineTable(t));
        }
        Value::Array(arr)
    }

    /// Set a top-level key to a value AND guarantee it renders above every
    /// `[table]` in the document. A bare `key = ...` written after a
    /// `[section]` header parses as a member of that section, not the
    /// document root — so a wheel array appended to a doc ending in
    /// `[controller_shift.south]` would silently nest there and the loader
    /// would miss it (falling back to the shipped default wheel).
    ///
    /// `toml_edit`'s per-item render position is fiddly to reorder across a
    /// mixed doc, so take the unambiguous route: drop any existing copy of
    /// the key, render just `key = value` from a one-key document, and
    /// prepend that line to the rest of the file. A root-level key at the
    /// very top can never be captured by a later section header.
    fn set_root_value_before_tables(
        doc: &mut toml_edit::DocumentMut,
        key: &str,
        value: toml_edit::Value,
    ) {
        // Remove any stale copy (top-level or mis-nested) so we don't leave
        // a duplicate behind.
        doc.as_table_mut().remove(key);
        let rest = doc.to_string();
        let mut head = toml_edit::DocumentMut::new();
        head.insert(key, toml_edit::Item::Value(value));
        let head_str = head.to_string();
        // Re-parse the concatenation so `doc` reflects the final layout.
        let combined = format!("{head_str}\n{rest}");
        *doc = combined
            .parse::<toml_edit::DocumentMut>()
            .unwrap_or_else(|_| {
                // Fallback: if concatenation somehow fails to parse, at
                // least set the key (nesting risk) rather than lose data.
                let mut d = toml_edit::DocumentMut::new();
                d.insert(key, head[key].clone());
                d
            });
    }

    /// Load the controller file as a comment-preserving `toml_edit` document
    /// (empty doc when absent), ensuring the parent dir exists. Shared by
    /// the wheel savers so they can splice wheel arrays with inline-table
    /// slices without disturbing the rest of the file.
    fn load_controller_document() -> Result<(std::path::PathBuf, toml_edit::DocumentMut)> {
        let path = Self::common_controller_path()?;
        let doc = if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read controller file: {:?}", path))?;
            contents
                .parse::<toml_edit::DocumentMut>()
                .unwrap_or_default()
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {:?}", parent))?;
            }
            toml_edit::DocumentMut::new()
        };
        Ok((path, doc))
    }

    pub fn save_controller_wheel_named(name: Option<&str>, slices: &[WheelSlice]) -> Result<()> {
        let (path, mut doc) = Self::load_controller_document()?;
        match name {
            None => {
                Self::set_root_value_before_tables(
                    &mut doc,
                    "controller_wheel",
                    Self::wheel_slices_to_inline(slices),
                );
            }
            Some(wheel) => {
                if slices.is_empty() {
                    if let Some(t) = doc
                        .get_mut("controller_wheels")
                        .and_then(|i| i.as_table_mut())
                    {
                        t.remove(wheel);
                    }
                } else {
                    // Ensure the parent table exists, then set the named
                    // wheel's slice array (inline tables so nested folders
                    // stay bound to their parent).
                    if doc.get("controller_wheels").is_none() {
                        doc["controller_wheels"] = toml_edit::table();
                    }
                    doc["controller_wheels"][wheel] =
                        toml_edit::Item::Value(Self::wheel_slices_to_inline(slices));
                }
            }
        }
        write_atomic(&path, doc.to_string())
            .with_context(|| format!("Failed to write controller file: {:?}", path))?;
        Ok(())
    }

    /// Replace the whole `[[controller_wheel]]` array in the global
    /// controller.toml (the wheel editor saves the full slice list). Nested
    /// folder slices are written as inline tables so a folder's children
    /// can never re-bind to a later sibling on reload.
    pub fn save_controller_wheel(slices: &[WheelSlice]) -> Result<()> {
        let (path, mut doc) = Self::load_controller_document()?;
        Self::set_root_value_before_tables(
            &mut doc,
            "controller_wheel",
            Self::wheel_slices_to_inline(slices),
        );
        write_atomic(&path, doc.to_string())
            .with_context(|| format!("Failed to write controller file: {:?}", path))?;
        Ok(())
    }

    fn controller_section_name(shift: bool) -> &'static str {
        if shift {
            "controller_shift"
        } else {
            "controller"
        }
    }

    /// Load controller (gamepad) bindings from a `[controller]`-family
    /// section of the global controller.toml. Controller binds are
    /// global-only: pads are per-desk, not per-character. Falls back to
    /// the shipped defaults when the file lacks the section
    /// (pre-refresh installs).
    pub fn load_controller_binds_layer(shift: bool) -> Result<HashMap<String, KeyBindAction>> {
        let section = Self::controller_section_name(shift);
        let section_from = |contents: &str| -> Option<HashMap<String, KeyBindAction>> {
            let toml_value: toml::Value = toml::from_str(contents).ok()?;
            let table = toml_value.get(section)?;
            table.clone().try_into().ok()
        };

        let path = Self::common_controller_path()?;
        if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read controller file: {:?}", path))?;
            if let Some(binds) = section_from(&contents) {
                return Ok(binds);
            }
        }
        Ok(section_from(DEFAULT_CONTROLLER).unwrap_or_default())
    }

    /// Base-layer controller bindings (`[controller]`).
    pub fn load_controller_binds() -> Result<HashMap<String, KeyBindAction>> {
        Self::load_controller_binds_layer(false)
    }

    /// Save one controller binding into a `[controller]`-family section of
    /// the global controller.toml (created if missing).
    pub fn save_single_controller_bind(
        button: &str,
        action: &KeyBindAction,
        shift: bool,
    ) -> Result<()> {
        let (path, mut toml_table) = Self::load_controller_table()?;
        let section = toml_table
            .entry(Self::controller_section_name(shift).to_string())
            .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
        if let toml::Value::Table(table) = section {
            let action_value = match action {
                KeyBindAction::Action(a) => toml::Value::String(a.clone()),
                KeyBindAction::Macro(m) => {
                    let mut macro_table = toml::value::Table::new();
                    macro_table.insert(
                        "macro_text".to_string(),
                        toml::Value::String(m.macro_text.clone()),
                    );
                    toml::Value::Table(macro_table)
                }
            };
            table.insert(button.to_string(), action_value);
        }
        Self::write_controller_table(&path, &toml_table)?;
        tracing::info!("Saved controller bind '{}' to {:?}", button, path);
        Ok(())
    }

    /// Delete one controller binding from a `[controller]`-family section
    /// of the global controller.toml.
    pub fn delete_single_controller_bind(button: &str, shift: bool) -> Result<()> {
        let path = Self::common_controller_path()?;
        if !path.exists() {
            return Ok(());
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read controller file: {:?}", path))?;
        let mut toml_table: toml::value::Table = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse controller file: {:?}", path))?;
        if let Some(toml::Value::Table(table)) =
            toml_table.get_mut(Self::controller_section_name(shift))
        {
            if table.remove(button).is_some() {
                Self::write_controller_table(&path, &toml_table)?;
                tracing::info!("Deleted controller bind '{}' from {:?}", button, path);
            }
        }
        Ok(())
    }

    /// Save keybinds to keybinds.toml for a character
    pub(crate) fn save_keybinds(&self, character: Option<&str>) -> Result<()> {
        let keybinds_path = Self::keybinds_path(character)?;
        let contents =
            toml::to_string_pretty(&self.keybinds).context("Failed to serialize keybinds")?;
        write_atomic(&keybinds_path, contents).context("Failed to write keybinds.toml")?;
        Ok(())
    }

    /// Save a single keybind to the appropriate file based on scope
    ///
    /// # Arguments
    /// * `key` - The key combo (e.g., "f5", "ctrl+e")
    /// * `action` - The keybind action
    /// * `is_global` - If true, save to global/keybinds.toml; if false, save to character profile
    /// * `character` - Character name (required if is_global is false)
    pub fn save_single_keybind(
        key: &str,
        action: &KeyBindAction,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        let path = if is_global {
            Self::common_keybinds_path()?
        } else {
            Self::keybinds_path(character)?
        };

        // Load existing content or create new
        let mut toml_table: toml::value::Table = if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read keybinds file: {:?}", path))?;
            toml::from_str(&contents).unwrap_or_else(|_| toml::value::Table::new())
        } else {
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {:?}", parent))?;
            }
            toml::value::Table::new()
        };

        // Get or create [user] section
        let user_section = toml_table
            .entry("user".to_string())
            .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));

        if let toml::Value::Table(user_table) = user_section {
            // Convert KeyBindAction to TOML value
            let action_value = match action {
                KeyBindAction::Action(a) => toml::Value::String(a.clone()),
                KeyBindAction::Macro(m) => {
                    let mut macro_table = toml::value::Table::new();
                    macro_table.insert(
                        "macro_text".to_string(),
                        toml::Value::String(m.macro_text.clone()),
                    );
                    toml::Value::Table(macro_table)
                }
            };
            user_table.insert(key.to_string(), action_value);
        }

        // Write back to file
        let contents =
            toml::to_string_pretty(&toml_table).context("Failed to serialize keybinds")?;
        write_atomic(&path, contents)
            .with_context(|| format!("Failed to write keybinds file: {:?}", path))?;

        tracing::info!(
            "Saved keybind '{}' to {} keybinds file: {:?}",
            key,
            if is_global { "global" } else { "character" },
            path
        );

        Ok(())
    }

    /// Delete a single keybind from the appropriate file based on scope
    ///
    /// # Arguments
    /// * `key` - The key combo to delete
    /// * `is_global` - If true, delete from global/keybinds.toml; if false, from character profile
    /// * `character` - Character name (required if is_global is false)
    pub fn delete_single_keybind(
        key: &str,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        let path = if is_global {
            Self::common_keybinds_path()?
        } else {
            Self::keybinds_path(character)?
        };

        if !path.exists() {
            tracing::warn!(
                "Cannot delete keybind '{}' - file does not exist: {:?}",
                key,
                path
            );
            return Ok(());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read keybinds file: {:?}", path))?;

        let mut toml_table: toml::value::Table = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse keybinds file: {:?}", path))?;

        // Get [user] section and remove the key
        if let Some(toml::Value::Table(user_table)) = toml_table.get_mut("user") {
            if user_table.remove(key).is_some() {
                // Write back to file
                let contents =
                    toml::to_string_pretty(&toml_table).context("Failed to serialize keybinds")?;
                write_atomic(&path, contents)
                    .with_context(|| format!("Failed to write keybinds file: {:?}", path))?;

                tracing::info!(
                    "Deleted keybind '{}' from {} keybinds file: {:?}",
                    key,
                    if is_global { "global" } else { "character" },
                    path
                );
            } else {
                tracing::warn!(
                    "Keybind '{}' not found in [user] section of {:?}",
                    key,
                    path
                );
            }
        } else {
            tracing::warn!(
                "No [user] section found in {:?} - cannot delete keybind '{}'",
                path,
                key
            );
        }

        Ok(())
    }

    /// Validate app keybinds and log warnings for any issues
    fn validate_app_keybinds(keybinds: &AppKeybinds) {
        // Check each critical global keybind
        if keybinds.quit.is_empty() {
            tracing::warn!("Global keybind 'quit' is empty - application may be difficult to exit");
        } else if parse_key_string(&keybinds.quit).is_none() {
            tracing::warn!(
                "Global keybind 'quit' has invalid value: '{}' - using default 'ctrl+c'",
                keybinds.quit
            );
        }

        if keybinds.start_search.is_empty() {
            tracing::warn!("Global keybind 'start_search' is empty - search feature disabled");
        } else if parse_key_string(&keybinds.start_search).is_none() {
            tracing::warn!(
                "Global keybind 'start_search' has invalid value: '{}'",
                keybinds.start_search
            );
        }

        if keybinds.close_window.is_empty() {
            tracing::warn!(
                "Global keybind 'close_window' is empty - may not be able to close dialogs"
            );
        } else if parse_key_string(&keybinds.close_window).is_none() {
            tracing::warn!(
                "Global keybind 'close_window' has invalid value: '{}'",
                keybinds.close_window
            );
        }

        if keybinds.next_search_match.is_empty() {
            tracing::debug!("Global keybind 'next_search_match' is empty");
        } else if parse_key_string(&keybinds.next_search_match).is_none() {
            tracing::warn!(
                "Global keybind 'next_search_match' has invalid value: '{}'",
                keybinds.next_search_match
            );
        }

        if keybinds.prev_search_match.is_empty() {
            tracing::debug!("Global keybind 'prev_search_match' is empty");
        } else if parse_key_string(&keybinds.prev_search_match).is_none() {
            tracing::warn!(
                "Global keybind 'prev_search_match' has invalid value: '{}'",
                keybinds.prev_search_match
            );
        }
    }

    /// Load common (global) app keybinds from global/keybinds.toml [app] section
    /// Returns: AppKeybinds from global, or default if file doesn't exist
    fn load_common_app_keybinds() -> Result<AppKeybinds> {
        let path = Self::common_keybinds_path()?;

        if !path.exists() {
            return Ok(AppKeybinds::default());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common keybinds: {:?}", path))?;

        let toml_value: toml::Value =
            toml::from_str(&contents).context("Failed to parse common keybinds TOML")?;

        // Try [app] section first
        if let Some(app_section) = toml_value.get("app") {
            let app_keybinds: AppKeybinds = app_section
                .clone()
                .try_into()
                .context("Failed to parse [app] section from common keybinds")?;
            Ok(app_keybinds)
        } else if let Some(global_section) = toml_value.get("global") {
            // Backward compatibility
            tracing::warn!("Using deprecated [global] section in global keybinds.toml - please rename to [app]");
            let app_keybinds: AppKeybinds = global_section
                .clone()
                .try_into()
                .context("Failed to parse [global] section from common keybinds")?;
            Ok(app_keybinds)
        } else {
            Ok(AppKeybinds::default())
        }
    }

    /// Load app keybinds, checking character file first, then global, then defaults
    /// For backward compatibility, also checks for deprecated [global] section
    pub fn load_app_keybinds(character: Option<&str>) -> Result<AppKeybinds> {
        // First, try character-specific keybinds
        let keybinds_path = Self::keybinds_path(character)?;

        if keybinds_path.exists() {
            let contents =
                fs::read_to_string(&keybinds_path).context("Failed to read keybinds.toml")?;

            let toml_value: toml::Value =
                toml::from_str(&contents).context("Failed to parse keybinds.toml")?;

            // Check if character file has [app] or [global] section
            if let Some(app_section) = toml_value.get("app") {
                let app_keybinds: AppKeybinds = app_section
                    .clone()
                    .try_into()
                    .context("Failed to parse [app] section")?;
                Self::validate_app_keybinds(&app_keybinds);
                return Ok(app_keybinds);
            } else if let Some(global_section) = toml_value.get("global") {
                tracing::warn!(
                    "Using deprecated [global] section in keybinds.toml - please rename to [app]"
                );
                let app_keybinds: AppKeybinds = global_section
                    .clone()
                    .try_into()
                    .context("Failed to parse [global] section")?;
                Self::validate_app_keybinds(&app_keybinds);
                return Ok(app_keybinds);
            }
            // Character file exists but has no [app] section - fall through to global
        }

        // Try global keybinds
        let app_keybinds = Self::load_common_app_keybinds()?;
        Self::validate_app_keybinds(&app_keybinds);
        Ok(app_keybinds)
    }

    /// Load common (global) menu keybinds from global/keybinds.toml [menu] section
    /// Returns: MenuKeybinds from global, or default if file doesn't exist
    fn load_common_menu_keybinds() -> Result<MenuKeybinds> {
        let path = Self::common_keybinds_path()?;

        if !path.exists() {
            return Ok(MenuKeybinds::default());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common keybinds: {:?}", path))?;

        let toml_value: toml::Value =
            toml::from_str(&contents).context("Failed to parse common keybinds TOML")?;

        if let Some(menu_section) = toml_value.get("menu") {
            let menu_keybinds: MenuKeybinds = menu_section
                .clone()
                .try_into()
                .context("Failed to parse [menu] section from common keybinds")?;
            Ok(menu_keybinds)
        } else {
            Ok(MenuKeybinds::default())
        }
    }

    /// Load menu keybinds, checking character file first, then global, then defaults
    pub fn load_menu_keybinds(character: Option<&str>) -> Result<MenuKeybinds> {
        tracing::debug!("load_menu_keybinds() called for character: {:?}", character);

        // First, try character-specific keybinds
        let keybinds_path = Self::keybinds_path(character)?;

        if keybinds_path.exists() {
            let contents =
                fs::read_to_string(&keybinds_path).context("Failed to read keybinds.toml")?;

            let toml_value: toml::Value =
                toml::from_str(&contents).context("Failed to parse keybinds.toml")?;

            // Check if character file has [menu] section
            if let Some(menu_section) = toml_value.get("menu") {
                tracing::debug!("Found [menu] section in character keybinds");
                let menu_keybinds: MenuKeybinds = menu_section
                    .clone()
                    .try_into()
                    .context("Failed to parse [menu] section")?;
                return Ok(menu_keybinds);
            }
            // Character file exists but has no [menu] section - fall through to global
        }

        // Try global keybinds
        Self::load_common_menu_keybinds()
    }
}

/// Get default keybindings (based on ProfanityFE defaults)
pub fn default_keybinds() -> HashMap<String, KeyBindAction> {
    let mut map = HashMap::new();

    // Basic command input
    map.insert(
        "enter".to_string(),
        KeyBindAction::Action("send_command".to_string()),
    );
    map.insert(
        "left".to_string(),
        KeyBindAction::Action("cursor_left".to_string()),
    );
    map.insert(
        "right".to_string(),
        KeyBindAction::Action("cursor_right".to_string()),
    );
    map.insert(
        "ctrl+left".to_string(),
        KeyBindAction::Action("cursor_word_left".to_string()),
    );
    map.insert(
        "ctrl+right".to_string(),
        KeyBindAction::Action("cursor_word_right".to_string()),
    );
    map.insert(
        "home".to_string(),
        KeyBindAction::Action("cursor_home".to_string()),
    );
    map.insert(
        "end".to_string(),
        KeyBindAction::Action("cursor_end".to_string()),
    );
    map.insert(
        "backspace".to_string(),
        KeyBindAction::Action("cursor_backspace".to_string()),
    );
    map.insert(
        "delete".to_string(),
        KeyBindAction::Action("cursor_delete".to_string()),
    );

    // Window management
    map.insert(
        "tab".to_string(),
        KeyBindAction::Action("switch_current_window".to_string()),
    );
    map.insert(
        "alt+page_up".to_string(),
        KeyBindAction::Action("scroll_current_window_up_one".to_string()),
    );
    map.insert(
        "alt+page_down".to_string(),
        KeyBindAction::Action("scroll_current_window_down_one".to_string()),
    );
    map.insert(
        "page_up".to_string(),
        KeyBindAction::Action("scroll_current_window_up_page".to_string()),
    );
    map.insert(
        "page_down".to_string(),
        KeyBindAction::Action("scroll_current_window_down_page".to_string()),
    );

    // Command history
    map.insert(
        "up".to_string(),
        KeyBindAction::Action("previous_command".to_string()),
    );
    map.insert(
        "down".to_string(),
        KeyBindAction::Action("next_command".to_string()),
    );

    // Search
    map.insert(
        "ctrl+f".to_string(),
        KeyBindAction::Action("start_search".to_string()),
    );
    map.insert(
        "ctrl+page_up".to_string(),
        KeyBindAction::Action("prev_search_match".to_string()),
    );
    map.insert(
        "ctrl+page_down".to_string(),
        KeyBindAction::Action("next_search_match".to_string()),
    );

    // Debug/Performance
    map.insert(
        "f12".to_string(),
        KeyBindAction::Action("toggle_performance_stats".to_string()),
    );

    // Numpad movement macros
    map.insert(
        "num_1".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "sw\r".to_string(),
        }),
    );
    map.insert(
        "num_2".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "s\r".to_string(),
        }),
    );
    map.insert(
        "num_3".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "se\r".to_string(),
        }),
    );
    map.insert(
        "num_4".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "w\r".to_string(),
        }),
    );
    map.insert(
        "num_5".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "out\r".to_string(),
        }),
    );
    map.insert(
        "num_6".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "e\r".to_string(),
        }),
    );
    map.insert(
        "num_7".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "nw\r".to_string(),
        }),
    );
    map.insert(
        "num_8".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "n\r".to_string(),
        }),
    );
    map.insert(
        "num_9".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "ne\r".to_string(),
        }),
    );
    map.insert(
        "num_0".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "down\r".to_string(),
        }),
    );
    map.insert(
        "num_.".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "up\r".to_string(),
        }),
    );
    map.insert(
        "num_+".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "look\r".to_string(),
        }),
    );
    map.insert(
        "num_-".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "info\r".to_string(),
        }),
    );
    map.insert(
        "num_*".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "exp\r".to_string(),
        }),
    );
    map.insert(
        "num_/".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "health\r".to_string(),
        }),
    );

    // Note: Shift+numpad doesn't work on Windows - the OS doesn't report SHIFT modifier for numpad numeric keys
    // If you want peer keybinds, use alt+numpad or ctrl+numpad instead (those modifiers work with numpad)

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rumble_resolve_builtins_and_off() {
        let config = RumbleConfig::default();
        assert_eq!(config.resolve_pattern("short"), Some((0.5, 160, 1, 120)));
        assert_eq!(config.resolve_pattern("long"), Some((0.9, 450, 1, 120)));
        assert_eq!(config.resolve_pattern("double"), Some((0.8, 180, 2, 120)));
        assert_eq!(config.resolve_pattern("off"), None);
        assert_eq!(config.resolve_pattern("no-such-pattern"), None);
    }

    #[test]
    fn rumble_resolve_custom_clamps_to_sane_ranges() {
        let mut config = RumbleConfig::default();
        config.patterns.push(RumblePattern {
            name: "heartbeat".to_string(),
            strength: 2.0,  // clamps to 1.0
            pulse_ms: 5,    // clamps to 20
            pulses: 99,     // clamps to 8
            gap_ms: 10_000, // clamps to 2000
        });
        assert_eq!(
            config.resolve_pattern("heartbeat"),
            Some((1.0, 20, 8, 2000))
        );
    }

    #[test]
    fn rumble_builtin_names_shadow_custom_patterns() {
        let mut config = RumbleConfig::default();
        config.patterns.push(RumblePattern {
            name: "short".to_string(),
            strength: 1.0,
            pulse_ms: 999,
            pulses: 8,
            gap_ms: 0,
        });
        assert_eq!(config.resolve_pattern("short"), Some((0.5, 160, 1, 120)));
    }

    fn wheel_config() -> crate::config::Config {
        let mut config = crate::config::Config::default();
        config.controller_wheel = vec![
            WheelSlice {
                label: "look".into(),
                command: "look".into(),
                ..Default::default()
            },
            WheelSlice {
                label: "stance".into(),
                command: String::new(),
                slices: vec![WheelSlice {
                    label: "defensive".into(),
                    command: "stance defensive".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        ];
        config.controller_wheels.insert(
            "spells".into(),
            vec![WheelSlice {
                label: "prep".into(),
                command: "prep 101".into(),
                ..Default::default()
            }],
        );
        config
    }

    #[test]
    fn wheel_level_slices_walks_folders_and_named_wheels() {
        let config = wheel_config();
        assert_eq!(config.wheel_level_slices("", &[]).unwrap().len(), 2);
        assert_eq!(
            config.wheel_level_slices("", &[1]).unwrap()[0].label,
            "defensive"
        );
        assert_eq!(
            config.wheel_level_slices("spells", &[]).unwrap()[0].label,
            "prep"
        );
        assert!(config.wheel_level_slices("missing", &[]).is_none());
        assert!(config.wheel_level_slices("", &[9]).is_none());

        // Empty default falls back to [controller_wheels.default].
        let mut config = wheel_config();
        config.controller_wheel.clear();
        assert!(config.wheel_level_slices("", &[]).is_none());
        config.controller_wheels.insert(
            "default".into(),
            vec![WheelSlice {
                label: "hide".into(),
                command: "hide".into(),
                ..Default::default()
            }],
        );
        assert_eq!(config.wheel_level_slices("", &[]).unwrap()[0].label, "hide");
    }

    #[test]
    fn wheel_pick_resolves_leaves_only() {
        let config = wheel_config();
        assert_eq!(config.wheel_pick_command("", &[0]), Some("look".into()));
        assert_eq!(
            config.wheel_pick_command("", &[1, 0]),
            Some("stance defensive".into())
        );
        assert_eq!(
            config.wheel_pick_command("spells", &[0]),
            Some("prep 101".into())
        );
        // Folders, empty paths and out-of-range indexes never fire.
        assert_eq!(config.wheel_pick_command("", &[1]), None);
        assert_eq!(config.wheel_pick_command("", &[]), None);
        assert_eq!(config.wheel_pick_command("", &[7]), None);
        assert_eq!(config.wheel_pick_command("missing", &[0]), None);
    }

    #[test]
    fn controller_action_names_all_parse() {
        for name in KeyAction::CONTROLLER_ACTION_NAMES {
            assert!(
                KeyAction::from_str(name).is_some(),
                "'{name}' in CONTROLLER_ACTION_NAMES does not parse"
            );
        }
    }

    #[test]
    fn interact_and_menu_nav_actions_map_correctly() {
        // Configurable interact/menu nav actions must map to their exact
        // variants (a from_str typo would silently break menu control).
        assert_eq!(KeyAction::from_str("interact_select"), Some(KeyAction::InteractSelect));
        assert_eq!(KeyAction::from_str("menu_up"), Some(KeyAction::MenuUp));
        assert_eq!(KeyAction::from_str("menu_down"), Some(KeyAction::MenuDown));
        assert_eq!(KeyAction::from_str("menu_left"), Some(KeyAction::MenuLeft));
        assert_eq!(KeyAction::from_str("menu_right"), Some(KeyAction::MenuRight));
        assert_eq!(KeyAction::from_str("menu_cancel"), Some(KeyAction::MenuCancel));
        // And they're all offered in the controller editor dropdown.
        for n in ["interact_select","menu_up","menu_down","menu_left","menu_right","menu_cancel"] {
            assert!(KeyAction::CONTROLLER_ACTION_NAMES.contains(&n), "{n} missing from dropdown list");
        }
    }

    #[test]
    fn nested_wheel_slices_round_trip_to_correct_parent() {
        // A folder slice ("stance") followed by sibling leaves, where the
        // folder's children must stay bound to the folder — not scatter
        // onto later siblings. Regression for the keybinds.toml corruption
        // where stance's stances landed on exp/health.
        let stance = WheelSlice {
            label: "stance".into(),
            command: String::new(),
            slices: vec![
                WheelSlice { label: "offensive".into(), command: "stance offensive".into(), ..Default::default() },
                WheelSlice { label: "defensive".into(), command: "stance defensive".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let leaf = |l: &str| WheelSlice { label: l.into(), command: l.into(), ..Default::default() };
        let wheel = vec![leaf("look"), stance, leaf("exp"), leaf("health")];

        // Serialize the way the writer does, re-parse, and confirm the
        // folder kept its two children and the leaves kept none.
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert(
            "controller_wheel",
            toml_edit::Item::Value(Config::wheel_slices_to_inline(&wheel)),
        );
        let serialized = doc.to_string();
        let reparsed: Vec<WheelSlice> = {
            let doc: toml::Value = toml::from_str(&serialized).expect("valid TOML");
            doc.get("controller_wheel").expect("wheel array").clone().try_into().expect("parse slices")
        };
        assert_eq!(reparsed.len(), 4, "top-level slice count preserved");
        assert_eq!(reparsed[0].label, "look");
        assert_eq!(reparsed[0].slices.len(), 0, "look is a leaf");
        assert_eq!(reparsed[1].label, "stance");
        assert_eq!(reparsed[1].slices.len(), 2, "stance keeps BOTH children");
        assert_eq!(reparsed[1].slices[0].label, "offensive");
        assert_eq!(reparsed[2].label, "exp");
        assert_eq!(reparsed[2].slices.len(), 0, "exp is NOT a folder");
        assert_eq!(reparsed[3].label, "health");
        assert_eq!(reparsed[3].slices.len(), 0, "health is NOT a folder");
    }

    #[test]
    fn validate_spans_flags_over_narrow_and_nonclosing_rings() {
        let sp = |label: &str, span: Option<f32>| WheelSlice {
            label: label.into(),
            command: label.into(),
            span,
            ..Default::default()
        };

        // All span-less: no issue (even ring).
        assert!(validate_wheel_spans("w", &[sp("a", None), sp("b", None)]).is_empty());

        // One 120 + three free @ 80 each: fine.
        let ok = vec![sp("a", Some(120.0)), sp("b", None), sp("c", None), sp("d", None)];
        assert!(validate_wheel_spans("w", &ok).is_empty());

        // Explicit spans sum over 360 (200 + 200): SumOver.
        let over = vec![sp("a", Some(200.0)), sp("b", Some(200.0))];
        assert!(matches!(
            validate_wheel_spans("w", &over).as_slice(),
            [WheelSpanIssue::SumOver { .. }]
        ));

        // Free slices exist but their share is sub-minimum (350 explicit
        // leaves 10 for one free slice): TooNarrow names that slice.
        let narrow = vec![sp("wide", Some(350.0)), sp("tiny", None)];
        let issues = validate_wheel_spans("w", &narrow);
        assert!(issues.iter().any(|i| matches!(
            i,
            WheelSpanIssue::TooNarrow { label, .. } if label == "tiny"
        )));

        // No free slice and explicit spans don't fill 360: DoesNotClose.
        let short = vec![sp("a", Some(60.0)), sp("b", Some(60.0))];
        assert!(matches!(
            validate_wheel_spans("w", &short).as_slice(),
            [WheelSpanIssue::DoesNotClose { .. }]
        ));

        // A single explicit span written below the minimum is flagged.
        let tiny = vec![sp("t", Some(10.0)), sp("b", None)];
        assert!(validate_wheel_spans("w", &tiny)
            .iter()
            .any(|i| matches!(i, WheelSpanIssue::TooNarrow { label, .. } if label == "t")));
    }

    #[test]
    fn validate_flags_back_at_top_level_and_duplicate_back() {
        let back = |label: &str| WheelSlice {
            label: label.into(),
            back: true,
            ..Default::default()
        };
        let leaf = |label: &str| WheelSlice {
            label: label.into(),
            command: label.into(),
            ..Default::default()
        };

        // A Back on the top ring is useless — nothing to ascend to.
        let top = validate_wheel_spans("w", &[leaf("a"), back("◂ Back")]);
        assert!(top
            .iter()
            .any(|i| matches!(i, WheelSpanIssue::BackAtTopLevel { .. })));

        // Inside a folder, a single Back is fine (no Back issue).
        let folder = WheelSlice {
            label: "f".into(),
            slices: vec![leaf("a"), back("◂ Back")],
            ..Default::default()
        };
        let nested = validate_wheel_spans("w", &[folder]);
        assert!(!nested
            .iter()
            .any(|i| matches!(i, WheelSpanIssue::BackAtTopLevel { .. }
                | WheelSpanIssue::MultipleBack { .. })));

        // Two Backs in one ring: MultipleBack.
        let folder2 = WheelSlice {
            label: "f".into(),
            slices: vec![back("b1"), leaf("a"), back("b2")],
            ..Default::default()
        };
        let dup = validate_wheel_spans("w", &[folder2]);
        assert!(dup
            .iter()
            .any(|i| matches!(i, WheelSpanIssue::MultipleBack { count: 2, .. })));
    }

    #[test]
    fn validate_spans_recurses_into_folders_with_names() {
        let folder = WheelSlice {
            label: "stance".into(),
            command: String::new(),
            slices: vec![
                WheelSlice { label: "def".into(), command: "d".into(), span: Some(200.0), ..Default::default() },
                WheelSlice { label: "off".into(), command: "o".into(), span: Some(200.0), ..Default::default() },
            ],
            ..Default::default()
        };
        let issues = validate_wheel_spans("default", &[folder]);
        // The over-sum is reported against the folder's sub-ring name.
        assert!(issues.iter().any(|i| matches!(
            i,
            WheelSpanIssue::SumOver { wheel, .. } if wheel == "default > stance"
        )));
    }

    #[test]
    fn span_and_inner_round_trip_and_stay_absent_when_unset() {
        // A slice with explicit span/inner survives the inline writer and
        // re-parses identically; a slice without them re-serializes with
        // NEITHER key — the byte-shape guarantee that keeps old configs
        // untouched by the new fields.
        let wheel = vec![
            WheelSlice {
                label: "attack".into(),
                command: "attack".into(),
                span: Some(120.0),
                inner: Some(20),
                ..Default::default()
            },
            WheelSlice {
                label: "◂ Back".into(),
                back: true,
                span: Some(60.0),
                ..Default::default()
            },
            WheelSlice { label: "hide".into(), command: "hide".into(), ..Default::default() },
        ];
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert(
            "controller_wheel",
            toml_edit::Item::Value(Config::wheel_slices_to_inline(&wheel)),
        );
        let serialized = doc.to_string();
        assert!(serialized.contains("span = 120.0"), "explicit span written: {serialized}");
        assert!(serialized.contains("inner = 20"), "explicit inner written: {serialized}");
        assert!(serialized.contains("back = true"), "back flag written: {serialized}");
        // The span-less slice's inline table must not mention either key.
        let hide_entry = serialized
            .split("label = \"hide\"")
            .nth(1)
            .expect("hide slice present");
        let hide_entry = hide_entry.split('}').next().unwrap();
        assert!(!hide_entry.contains("span"), "no span on unset slice: {hide_entry}");
        assert!(!hide_entry.contains("inner"), "no inner on unset slice: {hide_entry}");
        assert!(!hide_entry.contains("back"), "no back on a normal slice: {hide_entry}");

        let reparsed: Vec<WheelSlice> = {
            let doc: toml::Value = toml::from_str(&serialized).expect("valid TOML");
            doc.get("controller_wheel").unwrap().clone().try_into().expect("parse slices")
        };
        assert_eq!(reparsed, wheel, "wheel round-trips exactly");

        // An old-style config (no new keys) parses to None for both.
        let legacy: Vec<WheelSlice> = {
            let doc: toml::Value =
                toml::from_str("controller_wheel = [{ label = \"look\", command = \"look\" }]")
                    .unwrap();
            doc.get("controller_wheel").unwrap().clone().try_into().expect("legacy parses")
        };
        assert_eq!((legacy[0].span, legacy[0].inner), (None, None));
    }

    #[test]
    fn wheel_meta_start_round_trips_and_survives_prune() {
        // start serializes when set, is absent when unset, and — the prune
        // trap — a meta with ONLY start must survive save (the predicate
        // that drops empty metas must count it).
        let meta = WheelMeta { button: None, stick: None, start: Some(-30.0) };
        let serialized = toml::to_string(&meta).unwrap();
        assert!(serialized.contains("start = -30.0"), "{serialized}");
        assert!(!serialized.contains("button"), "{serialized}");
        let back: WheelMeta = toml::from_str(&serialized).unwrap();
        assert_eq!(back.start, Some(-30.0));

        // Unset start emits nothing (old files stay byte-identical).
        let plain = WheelMeta { button: Some("l3".into()), stick: None, start: None };
        assert!(!toml::to_string(&plain).unwrap().contains("start"));

        // Legacy metas (no start key) load with None.
        let legacy: WheelMeta = toml::from_str("button = \"r3\"").unwrap();
        assert_eq!(legacy.start, None);

        // The save-path prune keeps a start-only meta (same predicate as
        // save_controller_wheels_meta).
        let keeps = |m: &WheelMeta| m.button.is_some() || m.stick.is_some() || m.start.is_some();
        assert!(keeps(&meta), "start-only meta must not be pruned on save");
        assert!(!keeps(&WheelMeta::default()));
    }

    #[test]
    fn wheel_written_at_root_survives_trailing_section() {
        // A doc that ends in a nested section header ([controller_shift.
        // south]) is exactly what bit the real config: a bare
        // `controller_wheel = [...]` appended after it parses as a member
        // of that section, so the loader finds no top-level wheel and falls
        // back to defaults. set_root_value_before_tables must keep the key
        // at document root.
        let existing = "[controller_shift]\ndpad_up = \"x\"\n\n[controller_shift.south]\nmacro_text = \"stand\\r\"\n";
        let mut doc: toml_edit::DocumentMut = existing.parse().unwrap();
        let stance = WheelSlice {
            label: "stance".into(), command: String::new(),
            slices: vec![
                WheelSlice { label: "offensive".into(), command: "stance offensive".into(), ..Default::default() },
                WheelSlice { label: "defensive".into(), command: "stance defensive".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let wheel = vec![
            WheelSlice { label: "look".into(), command: "look".into(), ..Default::default() },
            stance,
            WheelSlice { label: "exp".into(), command: "exp".into(), ..Default::default() },
        ];
        Config::set_root_value_before_tables(
            &mut doc, "controller_wheel", Config::wheel_slices_to_inline(&wheel),
        );
        let out = doc.to_string();

        // Parse like the loader does: top-level key must exist and be the
        // full wheel (NOT nested under controller_shift.south).
        let v: toml::Value = toml::from_str(&out).expect("valid TOML");
        let arr = v.get("controller_wheel").expect("controller_wheel at ROOT");
        assert!(
            v.get("controller_shift")
                .and_then(|s| s.get("south"))
                .and_then(|s| s.get("controller_wheel"))
                .is_none(),
            "wheel must NOT nest under the trailing section"
        );
        let slices: Vec<WheelSlice> = arr.clone().try_into().expect("parse slices");
        assert_eq!(slices.len(), 3);
        assert_eq!(slices[1].label, "stance");
        assert_eq!(slices[1].slices.len(), 2, "stance keeps its children");
        assert_eq!(slices[2].label, "exp");
        assert_eq!(slices[2].slices.len(), 0, "exp stays a leaf");
        // The trailing section survived intact.
        assert_eq!(
            v.get("controller_shift").and_then(|s| s.get("south")).and_then(|s| s.get("macro_text")).and_then(|m| m.as_str()),
            Some("stand\r")
        );
    }

    #[test]
    fn wheel_meta_round_trips_and_prunes() {
        // A [controller_wheels_meta.NAME] table deserializes to WheelMeta.
        let toml_src = r#"
[controller_wheels_meta.combat]
button = "r2"
stick = "left"

[controller_wheels_meta.exits]
stick = "right"
"#;
        let value: toml::Value = toml::from_str(toml_src).unwrap();
        let map: HashMap<String, WheelMeta> = value
            .get("controller_wheels_meta")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(map["combat"].button.as_deref(), Some("r2"));
        assert_eq!(map["combat"].stick.as_deref(), Some("left"));
        assert_eq!(map["exits"].button, None);
        assert_eq!(map["exits"].stick.as_deref(), Some("right"));

        // Absent fields default to None (old configs with no meta).
        let empty: HashMap<String, WheelMeta> = HashMap::new();
        assert!(empty.get("combat").is_none());

        // An all-None meta serializes to an empty table (both fields skip).
        let bare = WheelMeta::default();
        let serialized = toml::Value::try_from(&bare).unwrap();
        assert_eq!(serialized.as_table().map(|t| t.len()), Some(0));
    }

    // ===========================================
    // parse_key_string - basic keys
    // ===========================================

    #[test]
    fn test_parse_key_string_single_char() {
        let result = parse_key_string("a");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Char('a'));
        assert!(mods.is_empty());
    }

    #[test]
    fn test_parse_key_string_uppercase_normalized() {
        let result = parse_key_string("A");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        // Normalized to lowercase
        assert_eq!(key, KeyCode::Char('a'));
    }

    #[test]
    fn test_parse_key_string_enter() {
        let result = parse_key_string("enter");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Enter);
        assert!(mods.is_empty());
    }

    #[test]
    fn test_parse_key_string_backspace() {
        let result = parse_key_string("backspace");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        assert_eq!(key, KeyCode::Backspace);
    }

    #[test]
    fn test_parse_key_string_delete() {
        let result = parse_key_string("delete");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        assert_eq!(key, KeyCode::Delete);
    }

    #[test]
    fn test_parse_key_string_tab() {
        let result = parse_key_string("tab");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        assert_eq!(key, KeyCode::Tab);
    }

    #[test]
    fn test_parse_key_string_escape() {
        assert!(parse_key_string("esc").is_some());
        assert!(parse_key_string("escape").is_some());
        let (key, _) = parse_key_string("esc").unwrap();
        assert_eq!(key, KeyCode::Esc);
    }

    #[test]
    fn test_parse_key_string_space() {
        let result = parse_key_string("space");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        assert_eq!(key, KeyCode::Char(' '));
    }

    // ===========================================
    // parse_key_string - arrow keys
    // ===========================================

    #[test]
    fn test_parse_key_string_arrows() {
        assert_eq!(parse_key_string("left").unwrap().0, KeyCode::Left);
        assert_eq!(parse_key_string("right").unwrap().0, KeyCode::Right);
        assert_eq!(parse_key_string("up").unwrap().0, KeyCode::Up);
        assert_eq!(parse_key_string("down").unwrap().0, KeyCode::Down);
    }

    #[test]
    fn test_parse_key_string_navigation() {
        assert_eq!(parse_key_string("home").unwrap().0, KeyCode::Home);
        assert_eq!(parse_key_string("end").unwrap().0, KeyCode::End);
    }

    #[test]
    fn test_parse_key_string_page_keys() {
        assert_eq!(parse_key_string("page_up").unwrap().0, KeyCode::PageUp);
        assert_eq!(parse_key_string("pageup").unwrap().0, KeyCode::PageUp);
        assert_eq!(parse_key_string("page_down").unwrap().0, KeyCode::PageDown);
        assert_eq!(parse_key_string("pagedown").unwrap().0, KeyCode::PageDown);
    }

    // ===========================================
    // parse_key_string - function keys
    // ===========================================

    #[test]
    fn test_parse_key_string_function_keys() {
        for i in 1..=12 {
            let key_str = format!("f{}", i);
            let result = parse_key_string(&key_str);
            assert!(result.is_some(), "F{} should parse", i);
            let (key, _) = result.unwrap();
            assert_eq!(key, KeyCode::F(i as u8));
        }
    }

    // ===========================================
    // parse_key_string - numpad keys
    // ===========================================

    #[test]
    fn test_parse_key_string_numpad_digits() {
        assert_eq!(parse_key_string("num_0").unwrap().0, KeyCode::Keypad0);
        assert_eq!(parse_key_string("num_1").unwrap().0, KeyCode::Keypad1);
        assert_eq!(parse_key_string("num_5").unwrap().0, KeyCode::Keypad5);
        assert_eq!(parse_key_string("num_9").unwrap().0, KeyCode::Keypad9);
    }

    #[test]
    fn test_parse_key_string_numpad_operators() {
        assert_eq!(parse_key_string("num_+").unwrap().0, KeyCode::KeypadPlus);
        assert_eq!(parse_key_string("num_-").unwrap().0, KeyCode::KeypadMinus);
        assert_eq!(
            parse_key_string("num_*").unwrap().0,
            KeyCode::KeypadMultiply
        );
        assert_eq!(parse_key_string("num_/").unwrap().0, KeyCode::KeypadDivide);
        assert_eq!(parse_key_string("num_.").unwrap().0, KeyCode::KeypadPeriod);
    }

    // ===========================================
    // parse_key_string - modifiers
    // ===========================================

    #[test]
    fn test_parse_key_string_ctrl_modifier() {
        let result = parse_key_string("ctrl+a");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Char('a'));
        assert!(mods.ctrl);
        assert!(!mods.shift);
        assert!(!mods.alt);
    }

    #[test]
    fn test_parse_key_string_alt_modifier() {
        let result = parse_key_string("alt+x");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Char('x'));
        assert!(mods.alt);
        assert!(!mods.ctrl);
    }

    #[test]
    fn test_parse_key_string_shift_modifier() {
        let result = parse_key_string("shift+tab");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Tab);
        assert!(mods.shift);
    }

    #[test]
    fn test_parse_key_string_control_alias() {
        let result = parse_key_string("control+c");
        assert!(result.is_some());
        let (_, mods) = result.unwrap();
        assert!(mods.ctrl);
    }

    #[test]
    fn test_parse_key_string_multiple_modifiers() {
        let result = parse_key_string("ctrl+shift+a");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Char('a'));
        assert!(mods.ctrl);
        assert!(mods.shift);
        assert!(!mods.alt);
    }

    #[test]
    fn test_parse_key_string_all_modifiers() {
        let result = parse_key_string("ctrl+alt+shift+f5");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::F(5));
        assert!(mods.ctrl);
        assert!(mods.alt);
        assert!(mods.shift);
    }

    #[test]
    fn test_parse_key_string_modifier_with_special_key() {
        let result = parse_key_string("ctrl+page_up");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::PageUp);
        assert!(mods.ctrl);
    }

    // ===========================================
    // parse_key_string - case insensitivity
    // ===========================================

    #[test]
    fn test_parse_key_string_case_insensitive() {
        assert!(parse_key_string("CTRL+A").is_some());
        assert!(parse_key_string("Ctrl+A").is_some());
        assert!(parse_key_string("ENTER").is_some());
        assert!(parse_key_string("Enter").is_some());
    }

    // ===========================================
    // parse_key_string - invalid inputs
    // ===========================================

    #[test]
    fn test_parse_key_string_invalid() {
        assert!(parse_key_string("invalid_key").is_none());
        assert!(parse_key_string("").is_none());
        assert!(parse_key_string("ctrl+").is_none());
    }

    #[test]
    fn test_parse_key_string_invalid_modifier() {
        assert!(parse_key_string("meta+a").is_none());
        assert!(parse_key_string("super+a").is_none());
    }

    // ===========================================
    // KeyAction::from_str tests
    // ===========================================

    #[test]
    fn test_key_action_from_str_command_input() {
        assert_eq!(
            KeyAction::from_str("send_command"),
            Some(KeyAction::SendCommand)
        );
        assert_eq!(
            KeyAction::from_str("cursor_left"),
            Some(KeyAction::CursorLeft)
        );
        assert_eq!(
            KeyAction::from_str("cursor_right"),
            Some(KeyAction::CursorRight)
        );
        assert_eq!(
            KeyAction::from_str("cursor_home"),
            Some(KeyAction::CursorHome)
        );
        assert_eq!(
            KeyAction::from_str("cursor_end"),
            Some(KeyAction::CursorEnd)
        );
        assert_eq!(
            KeyAction::from_str("cursor_backspace"),
            Some(KeyAction::CursorBackspace)
        );
        assert_eq!(
            KeyAction::from_str("cursor_delete"),
            Some(KeyAction::CursorDelete)
        );
    }

    #[test]
    fn test_key_action_from_str_word_movement() {
        assert_eq!(
            KeyAction::from_str("cursor_word_left"),
            Some(KeyAction::CursorWordLeft)
        );
        assert_eq!(
            KeyAction::from_str("cursor_word_right"),
            Some(KeyAction::CursorWordRight)
        );
        assert_eq!(
            KeyAction::from_str("cursor_delete_word"),
            Some(KeyAction::CursorDeleteWord)
        );
        assert_eq!(
            KeyAction::from_str("cursor_clear_line"),
            Some(KeyAction::CursorClearLine)
        );
    }

    #[test]
    fn test_key_action_from_str_history() {
        assert_eq!(
            KeyAction::from_str("previous_command"),
            Some(KeyAction::PreviousCommand)
        );
        assert_eq!(
            KeyAction::from_str("next_command"),
            Some(KeyAction::NextCommand)
        );
        assert_eq!(
            KeyAction::from_str("send_last_command"),
            Some(KeyAction::SendLastCommand)
        );
        assert_eq!(
            KeyAction::from_str("send_second_last_command"),
            Some(KeyAction::SendSecondLastCommand)
        );
    }

    #[test]
    fn test_key_action_from_str_window() {
        assert_eq!(
            KeyAction::from_str("switch_current_window"),
            Some(KeyAction::SwitchCurrentWindow)
        );
        assert_eq!(
            KeyAction::from_str("scroll_current_window_up_one"),
            Some(KeyAction::ScrollCurrentWindowUpOne)
        );
        assert_eq!(
            KeyAction::from_str("scroll_current_window_down_one"),
            Some(KeyAction::ScrollCurrentWindowDownOne)
        );
        assert_eq!(
            KeyAction::from_str("scroll_current_window_up_page"),
            Some(KeyAction::ScrollCurrentWindowUpPage)
        );
        assert_eq!(
            KeyAction::from_str("scroll_current_window_down_page"),
            Some(KeyAction::ScrollCurrentWindowDownPage)
        );
        assert_eq!(
            KeyAction::from_str("scroll_current_window_home"),
            Some(KeyAction::ScrollCurrentWindowHome)
        );
        assert_eq!(
            KeyAction::from_str("scroll_current_window_end"),
            Some(KeyAction::ScrollCurrentWindowEnd)
        );
    }

    #[test]
    fn test_key_action_from_str_search() {
        assert_eq!(
            KeyAction::from_str("start_search"),
            Some(KeyAction::StartSearch)
        );
        assert_eq!(
            KeyAction::from_str("next_search_match"),
            Some(KeyAction::NextSearchMatch)
        );
        assert_eq!(
            KeyAction::from_str("prev_search_match"),
            Some(KeyAction::PrevSearchMatch)
        );
        assert_eq!(
            KeyAction::from_str("clear_search"),
            Some(KeyAction::ClearSearch)
        );
    }

    #[test]
    fn test_key_action_from_str_tabs() {
        assert_eq!(KeyAction::from_str("next_tab"), Some(KeyAction::NextTab));
        assert_eq!(KeyAction::from_str("prev_tab"), Some(KeyAction::PrevTab));
        assert_eq!(
            KeyAction::from_str("next_unread_tab"),
            Some(KeyAction::NextUnreadTab)
        );
    }

    #[test]
    fn test_key_action_from_str_clipboard() {
        assert_eq!(KeyAction::from_str("copy"), Some(KeyAction::Copy));
        assert_eq!(KeyAction::from_str("paste"), Some(KeyAction::Paste));
        assert_eq!(
            KeyAction::from_str("select_all"),
            Some(KeyAction::SelectAll)
        );
    }

    #[test]
    fn test_key_action_from_str_toggles() {
        assert_eq!(
            KeyAction::from_str("toggle_performance_stats"),
            Some(KeyAction::TogglePerformanceStats)
        );
        assert_eq!(
            KeyAction::from_str("toggle_sounds"),
            Some(KeyAction::ToggleSounds)
        );
    }

    #[test]
    fn test_key_action_from_str_tts() {
        assert_eq!(KeyAction::from_str("tts_next"), Some(KeyAction::TtsNext));
        assert_eq!(
            KeyAction::from_str("tts_previous"),
            Some(KeyAction::TtsPrevious)
        );
        assert_eq!(
            KeyAction::from_str("tts_next_unread"),
            Some(KeyAction::TtsNextUnread)
        );
        assert_eq!(KeyAction::from_str("tts_stop"), Some(KeyAction::TtsStop));
        assert_eq!(
            KeyAction::from_str("stop_travel"),
            Some(KeyAction::StopTravel)
        );
        assert_eq!(
            KeyAction::from_str("tts_mute_toggle"),
            Some(KeyAction::TtsMuteToggle)
        );
        assert_eq!(
            KeyAction::from_str("tts_increase_rate"),
            Some(KeyAction::TtsIncreaseRate)
        );
        assert_eq!(
            KeyAction::from_str("tts_decrease_rate"),
            Some(KeyAction::TtsDecreaseRate)
        );
        assert_eq!(
            KeyAction::from_str("tts_increase_volume"),
            Some(KeyAction::TtsIncreaseVolume)
        );
        assert_eq!(
            KeyAction::from_str("tts_decrease_volume"),
            Some(KeyAction::TtsDecreaseVolume)
        );
    }

    #[test]
    fn test_key_action_from_str_legacy() {
        // Legacy alias
        assert_eq!(
            KeyAction::from_str("tts_pause_resume"),
            Some(KeyAction::TtsStop)
        );
    }

    #[test]
    fn test_key_action_from_str_invalid() {
        assert_eq!(KeyAction::from_str("invalid_action"), None);
        assert_eq!(KeyAction::from_str(""), None);
        assert_eq!(KeyAction::from_str("SEND_COMMAND"), None); // Case sensitive
    }

    // ===========================================
    // AppKeybinds tests
    // ===========================================

    #[test]
    fn test_app_keybinds_default() {
        let keybinds = AppKeybinds::default();
        assert_eq!(keybinds.quit, "ctrl+c");
        assert_eq!(keybinds.start_search, "ctrl+f");
        assert_eq!(keybinds.next_search_match, "ctrl+pagedown");
        assert_eq!(keybinds.prev_search_match, "ctrl+pageup");
        assert_eq!(keybinds.close_window, "esc");
    }

    #[test]
    fn test_app_keybinds_clone() {
        let keybinds = AppKeybinds::default();
        let cloned = keybinds.clone();
        assert_eq!(cloned.quit, keybinds.quit);
        assert_eq!(cloned.start_search, keybinds.start_search);
    }

    // ===========================================
    // MenuKeybinds tests
    // ===========================================

    #[test]
    fn test_menu_keybinds_default() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.navigate_up, "Up");
        assert_eq!(keybinds.navigate_down, "Down");
        assert_eq!(keybinds.navigate_left, "Left");
        assert_eq!(keybinds.navigate_right, "Right");
        assert_eq!(keybinds.page_up, "PageUp");
        assert_eq!(keybinds.page_down, "PageDown");
        assert_eq!(keybinds.home, "Home");
        assert_eq!(keybinds.end, "End");
    }

    #[test]
    fn test_menu_keybinds_field_navigation() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.next_field, "Tab");
        assert_eq!(keybinds.previous_field, "Shift+Tab");
    }

    #[test]
    fn test_menu_keybinds_actions() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.select, "Enter");
        assert_eq!(keybinds.cancel, "Esc");
        assert_eq!(keybinds.save, "Ctrl+s");
        assert_eq!(keybinds.delete, "Delete");
    }

    #[test]
    fn test_menu_keybinds_clipboard() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.select_all, "Ctrl+A");
        assert_eq!(keybinds.copy, "Ctrl+C");
        assert_eq!(keybinds.cut, "Ctrl+X");
        assert_eq!(keybinds.paste, "Ctrl+V");
    }

    #[test]
    fn test_menu_keybinds_toggles() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.toggle, "Space");
        assert_eq!(keybinds.toggle_filter, "F");
    }

    #[test]
    fn test_menu_keybinds_reordering() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.move_up, "Shift+Up");
        assert_eq!(keybinds.move_down, "Shift+Down");
    }

    #[test]
    fn test_menu_keybinds_list_management() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.add, "A");
        assert_eq!(keybinds.edit, "E");
    }

    // ===========================================
    // default_keybinds tests
    // ===========================================

    #[test]
    fn test_default_keybinds_basic() {
        let keybinds = default_keybinds();
        assert!(keybinds.contains_key("enter"));
        assert!(keybinds.contains_key("left"));
        assert!(keybinds.contains_key("right"));
        assert!(keybinds.contains_key("backspace"));
    }

    #[test]
    fn test_default_keybinds_history() {
        let keybinds = default_keybinds();
        assert!(keybinds.contains_key("up"));
        assert!(keybinds.contains_key("down"));
    }

    #[test]
    fn test_default_keybinds_numpad() {
        let keybinds = default_keybinds();
        for i in 0..=9 {
            let key = format!("num_{}", i);
            assert!(keybinds.contains_key(&key), "Missing numpad key: {}", key);
        }
        assert!(keybinds.contains_key("num_+"));
        assert!(keybinds.contains_key("num_-"));
        assert!(keybinds.contains_key("num_*"));
        assert!(keybinds.contains_key("num_/"));
        assert!(keybinds.contains_key("num_."));
    }

    #[test]
    fn test_default_keybinds_numpad_movement() {
        let keybinds = default_keybinds();

        // Check numpad movement macros
        if let Some(KeyBindAction::Macro(m)) = keybinds.get("num_8") {
            assert_eq!(m.macro_text, "n\r"); // North
        } else {
            panic!("num_8 should be a Macro action");
        }

        if let Some(KeyBindAction::Macro(m)) = keybinds.get("num_2") {
            assert_eq!(m.macro_text, "s\r"); // South
        }
    }

    #[test]
    fn test_default_keybinds_search() {
        let keybinds = default_keybinds();
        assert!(keybinds.contains_key("ctrl+f"));
        assert!(keybinds.contains_key("ctrl+page_up"));
        assert!(keybinds.contains_key("ctrl+page_down"));
    }

    // ===========================================
    // KeyBindAction tests
    // ===========================================

    #[test]
    fn test_key_bind_action_action() {
        let action = KeyBindAction::Action("send_command".to_string());
        match action {
            KeyBindAction::Action(s) => assert_eq!(s, "send_command"),
            _ => panic!("Expected Action variant"),
        }
    }

    #[test]
    fn test_key_bind_action_macro() {
        let action = KeyBindAction::Macro(MacroAction {
            macro_text: "look\r".to_string(),
        });
        match action {
            KeyBindAction::Macro(m) => assert_eq!(m.macro_text, "look\r"),
            _ => panic!("Expected Macro variant"),
        }
    }

    #[test]
    fn test_macro_action_clone() {
        let macro_action = MacroAction {
            macro_text: "test\r".to_string(),
        };
        let cloned = macro_action.clone();
        assert_eq!(cloned.macro_text, macro_action.macro_text);
    }

    // ===========================================
    // KeyAction equality tests
    // ===========================================

    #[test]
    fn test_key_action_equality() {
        assert_eq!(KeyAction::SendCommand, KeyAction::SendCommand);
        assert_ne!(KeyAction::SendCommand, KeyAction::CursorLeft);
        assert_ne!(KeyAction::Copy, KeyAction::Paste);
    }

    #[test]
    fn test_key_action_send_macro_equality() {
        let macro1 = KeyAction::SendMacro("test".to_string());
        let macro2 = KeyAction::SendMacro("test".to_string());
        let macro3 = KeyAction::SendMacro("other".to_string());
        assert_eq!(macro1, macro2);
        assert_ne!(macro1, macro3);
    }

    #[test]
    fn test_key_action_clone() {
        let action = KeyAction::ScrollCurrentWindowUpPage;
        let cloned = action.clone();
        assert_eq!(action, cloned);
    }
}


