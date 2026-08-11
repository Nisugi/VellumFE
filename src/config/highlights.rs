use super::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RedirectMode {
    /// Only send to redirect window (remove from original window)
    #[serde(rename = "redirect_only")]
    RedirectOnly,
    /// Send to both original window and redirect window (duplicate)
    #[serde(rename = "redirect_copy")]
    RedirectCopy,
}

impl Default for RedirectMode {
    fn default() -> Self {
        RedirectMode::RedirectOnly
    }
}

/// Where an alert overlay is pinned on the main viewport. A 9-grid anchor
/// keeps authored packs resolution-independent: the same pack lands sensibly
/// on a 1080p laptop and an ultrawide. Screen-anchored is the v1 model;
/// window-anchored overlays are deliberately deferred (they need
/// hidden/closed/detached fallback rules that have no customer yet).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AlertAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    #[default]
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Presentation + discipline for one alert. Attached to a `HighlightPattern`,
/// so alerts inherit the whole highlight system: the same editors, the same
/// per-stream filtering, the same fast-parse matching path. No new trigger
/// infrastructure exists or should exist.
///
/// Every presentation field is optional and they compose: an alert may show a
/// banner AND art AND a flash. An alert with no presentation at all is inert
/// (and rejected by the editor) rather than an error.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlertSpec {
    /// Stable identity for cooldown state and (later) timer `cancels`
    /// references and pack-update diffing. Falls back to the pattern text
    /// when absent, which is stable enough for cooldowns but changes if the
    /// user edits the pattern — hence the explicit id for authored packs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Banner text shown at the anchor. Supports `$1`-style capture groups
    /// expanded from the triggering match, so "%s is casting" style alerts
    /// can name the caster.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    /// Banner foreground/background color names (same palette as highlights).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_fg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_bg: Option<String>,

    /// Image played once at the anchor. Any decodable format: a static PNG is
    /// simply a one-frame animation and takes the decoder's static path, so
    /// "icon popup" and "animated flourish" are the same field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art: Option<String>,

    /// Viewport-edge tint pulse color. Photosensitivity-sensitive: always
    /// scaled by the global intensity ceiling and killable outright.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flash: Option<String>,

    /// Where on the viewport this alert is pinned.
    #[serde(default, skip_serializing_if = "is_default_anchor")]
    pub anchor: AlertAnchor,
    /// Pixel nudge from the anchor point, `(x, y)`, +y down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<(f32, f32)>,

    /// Seconds on screen before auto-dismiss. Alerts ALWAYS expire; there is
    /// no infinite alert, because a stuck overlay over combat prose is
    /// unrecoverable without a config edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,

    /// Minimum seconds between fires of this rule. Anti-spam is the product:
    /// a heavy-scroll Reim pull must not machine-gun one rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown: Option<f32>,

    /// Higher wins when the concurrent cap evicts. Carried in v1 but not yet
    /// consulted (eviction is oldest-first); present now because retrofitting
    /// a field into already-authored TOML is far costlier than reserving it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,

    /// Game-state gate. When present WITHOUT a pattern, this alert is
    /// condition-driven: it fires once on the false->true transition ("health
    /// crossed 30%"), stays silent while it remains true, and re-arms when it
    /// clears. Level-triggered firing would repaint the warning every frame,
    /// which is why the engine is edge-triggered by construction.
    ///
    /// Evaluated in core once per frame, never in a render path — a condition
    /// evaluated while painting fires twice on a detached viewport and cannot
    /// reach the phone bridge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<crate::config::Condition>,

    /// Seconds the gate must stay continuously FALSE before this rule may
    /// fire again. The anti-flap guard: a vital hovering on its threshold
    /// (29/31/29%) would otherwise machine-gun the screen. Defaults to
    /// `DEFAULT_REARM_SECS`; 0 means re-arm the instant it clears.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rearm: Option<f32>,
}

fn is_default_anchor(anchor: &AlertAnchor) -> bool {
    *anchor == AlertAnchor::default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightPattern {
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub color_entire_line: bool, // If true, apply colors to entire line, not just matched text
    #[serde(default, skip_serializing_if = "is_false")]
    pub fast_parse: bool, // If true, split pattern on | and use Aho-Corasick for literal matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>, // Sound file to play when pattern matches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_volume: Option<f32>, // Volume override for this sound (0.0 to 1.0)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rumble: Option<String>, // Controller rumble pattern name to play when pattern matches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>, // Category for grouping highlights (e.g., "Combat", "Healing", "Death")
    #[serde(default, skip_serializing_if = "is_false")]
    pub squelch: bool, // If true, completely hide lines matching this pattern (ignore/filter)
    #[serde(default, skip_serializing_if = "is_false")]
    pub silent_prompt: bool, // If true, lines matching don't trigger prompt display (prompt suppressed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_to: Option<String>, // Window name to redirect matching lines to
    #[serde(default, skip_serializing_if = "is_default_redirect_mode")]
    pub redirect_mode: RedirectMode, // How to handle redirect: only or copy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>, // If set, replace matched text with this string (supports $1, $2 capture groups)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>, // If set, only apply this highlight to lines from this stream (e.g., "death", "thoughts")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>, // If set with replace, only apply replacement in this window (colors apply everywhere)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_status: Option<String>, // Custom status id to activate on match (indicator/dashboard widgets with this id light up)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_duration: Option<f32>, // Seconds until the set status auto-clears; None = until cleared
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_status: Option<String>, // Custom status id to deactivate on match
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<AlertSpec>, // Overlay alert (banner/art/flash) to raise on match

    // Performance optimization: cache compiled regex (not serialized)
    #[serde(skip)]
    pub compiled_regex: Option<regex::Regex>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPattern {
    pub pattern: String,     // Regex pattern to match
    pub event_type: String,  // Event type: "stun", "webbed", "prone", etc.
    pub action: EventAction, // Action to perform: set/clear/increment
    #[serde(default)]
    pub duration: u32, // Duration in seconds (0 = don't change)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_capture: Option<usize>, // Regex capture group for duration (1-based)
    #[serde(default = "default_duration_multiplier")]
    pub duration_multiplier: f32, // Multiply captured duration (e.g., 5.0 for rounds->seconds)
    #[serde(default = "default_enabled")]
    pub enabled: bool, // Can disable without deleting
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum EventAction {
    #[default]
    Set, // Set state/timer (e.g., start stun countdown)
    Clear,     // Clear state/timer (e.g., recover from stun)
    Increment, // Add to existing value (future use)
}

fn default_duration_multiplier() -> f32 {
    1.0
}
fn default_enabled() -> bool {
    true
}

fn is_false(b: &bool) -> bool {
    !b
}

fn is_default_redirect_mode(mode: &RedirectMode) -> bool {
    *mode == RedirectMode::default()
}

// ===================================================================
// Highlight field catalog — the parity guard.
//
// Unlike scalar settings (registry.rs, which DRIVES generic rendering),
// each highlight editor lays its fields out by hand in its own toolkit
// (the GUI grid, the TUI form, the phone's static HTML). So the catalog
// can't render;
// instead it is the single list of what a highlight rule EXPOSES, and a
// test cross-checks it against each editor's coverage manifest — a small
// list every editor keeps next to its form. If an editor omits a field
// that applies to it (and it isn't exempt), the build fails. This is the
// keybind-action / registry pattern applied at the enforcement layer.
// ===================================================================

/// The three editor frontends a highlight field can appear in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlFrontend {
    Tui,
    Gui,
    Web,
}

/// One user-facing field of a `HighlightPattern`. Kept intentionally minimal
/// — just the identity (`name`) and where it must be editable (`applies_to`)
/// — because that is all the parity guard consumes. Widget-kind/label metadata
/// is deliberately NOT carried here: no code renders from this catalog today,
/// so adding it would be speculative dead weight. A future editor-generation
/// pass can extend `HlFieldDef` when it has a real consumer.
pub struct HlFieldDef {
    /// Serde field name on `HighlightPattern` (the catalog's identity).
    pub name: &'static str,
    /// Frontends whose editors are expected to edit this field. Every
    /// listed frontend's coverage manifest must contain `name` or the
    /// parity test fails (unless listed in `HL_FIELD_EXEMPTIONS`).
    pub applies_to: &'static [HlFrontend],
}

use HlFrontend::*;

/// Every field a highlight rule exposes to users. `compiled_regex` is a
/// non-serialized cache and is deliberately absent. `scope` (global vs
/// character) is editor routing, not a struct field, so it is tracked by
/// each editor but not catalogued here.
pub const HIGHLIGHT_FIELDS: &[HlFieldDef] = &[
    HlFieldDef { name: "pattern", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "fg", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "bg", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "bold", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "color_entire_line", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "fast_parse", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "sound", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "sound_volume", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "rumble", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "category", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "squelch", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "silent_prompt", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "redirect_to", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "redirect_mode", applies_to: &[Tui, Gui, Web] },
    // Custom-status actions (all three editors since 2026-07-31).
    HlFieldDef { name: "set_status", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "status_duration", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "clear_status", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "replace", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "stream", applies_to: &[Tui, Gui, Web] },
    HlFieldDef { name: "window", applies_to: &[Tui, Gui, Web] },
    // Overlay alerts (2026-08-11). GUI-only for now, by scope and not by
    // neglect: the GUI is the only frontend with an overlay renderer. The
    // TUI's constrained form (banner line, border flash) and the web DOM
    // overlay each need a renderer AND a bridge message carrying active-alert
    // state before their editors could honestly claim to author this. Widen
    // `applies_to` as those land — tracking it per-frontend rather than
    // exempting it is exactly so this stays visible instead of forgotten.
    HlFieldDef { name: "alert", applies_to: &[Gui] },
];

/// Deliberate coverage gaps, each with a reason. A `(field, frontend)`
/// here is excused from the parity test. Mirror of registry.rs's
/// EXEMPT_PREFIXES — an explicit, reviewed escape hatch. Empty today:
/// full parity is the goal, so any entry is a conscious retreat from it.
pub const HL_FIELD_EXEMPTIONS: &[(&str, HlFrontend, &str)] = &[];

/// The highlight fields the web/phone form should render, in catalog order —
/// every field that applies to `Web` and isn't exempted there. Shipped to the
/// phone in the highlights payload so the client renders from the canonical
/// catalog rather than a hand-kept list that could drift. This is the
/// catalog's runtime consumer (the parity test is the compile-time one).
pub fn highlight_web_fields() -> Vec<&'static str> {
    HIGHLIGHT_FIELDS
        .iter()
        .filter(|def| def.applies_to.contains(&HlFrontend::Web))
        .filter(|def| {
            !HL_FIELD_EXEMPTIONS
                .iter()
                .any(|(f, fe, _)| *f == def.name && *fe == HlFrontend::Web)
        })
        .map(|def| def.name)
        .collect()
}

// ---- Per-editor coverage manifests ---------------------------------
//
// Each editor keeps a manifest of the catalog fields it edits. These live
// here (not in frontend/, which core cannot import) as the contract the
// editors must honor; the doc comment names the file that must match. The
// parity test proves manifest ⊇ (catalog ∩ frontend), and for the web
// manifest a second test greps the embedded app.js so the manifest can't
// claim a field the JS doesn't actually render.

/// Fields edited by the TUI highlight form (`frontend/tui/highlight_form.rs`).
pub const HL_TUI_COVERED: &[&str] = &[
    "pattern", "fg", "bg", "bold", "color_entire_line", "fast_parse", "sound",
    "sound_volume", "rumble", "category", "squelch", "silent_prompt",
    "redirect_to", "redirect_mode", "replace", "stream", "window",
    "set_status", "status_duration", "clear_status",
];

/// Fields edited by the GUI highlight editor
/// (`frontend/gui/app/editors/highlights.rs`).
pub const HL_GUI_COVERED: &[&str] = &[
    "pattern", "fg", "bg", "bold", "color_entire_line", "fast_parse", "sound",
    "sound_volume", "rumble", "category", "squelch", "silent_prompt",
    "redirect_to", "redirect_mode", "replace", "stream", "window",
    "set_status", "status_duration", "clear_status", "alert",
];

/// Fields edited by the web/phone highlight form
/// (`frontend/web/assets/app.js`, `openHlForm` / `hlSaveRule`). Each entry
/// is cross-checked against the embedded app.js by
/// `web_highlight_manifest_matches_app_js`.
pub const HL_WEB_COVERED: &[&str] = &[
    "pattern", "fg", "bg", "bold", "color_entire_line", "fast_parse", "sound",
    "sound_volume", "rumble", "category", "squelch", "silent_prompt",
    "redirect_to", "redirect_mode", "replace", "stream", "window",
    "set_status", "status_duration", "clear_status",
];

/// The DOM element id the web form uses for a given catalog field, so the
/// app.js cross-check knows what to grep for. Web input ids follow the
/// `hl-<field>` convention with a few historical exceptions.
pub fn hl_web_element_id(field: &str) -> &'static str {
    match field {
        "pattern" => "hl-pattern",
        "fg" => "hl-fg-pick",
        "bg" => "hl-bg-pick",
        "bold" => "hl-bold",
        "color_entire_line" => "hl-line",
        "fast_parse" => "hl-fast-parse",
        "sound" => "hl-sound",
        "sound_volume" => "hl-sound-volume",
        "rumble" => "hl-rumble",
        "category" => "hl-category",
        "squelch" => "hl-squelch",
        "silent_prompt" => "hl-silent-prompt",
        "redirect_to" => "hl-redirect-to",
        "redirect_mode" => "hl-redirect-mode",
        "replace" => "hl-replace",
        "stream" => "hl-stream",
        "window" => "hl-window",
        "set_status" => "hl-set-status",
        "status_duration" => "hl-status-duration",
        "clear_status" => "hl-clear-status",
        other => panic!("no web element id mapped for highlight field '{other}'"),
    }
}

impl Config {
    /// Load common (global) highlights that apply to all characters
    /// Returns: HashMap of global highlights, or empty if file doesn't exist
    pub fn load_common_highlights() -> Result<HashMap<String, HighlightPattern>> {
        let path = Self::common_highlights_path()?;

        if !path.exists() {
            return Ok(HashMap::new());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common highlights: {:?}", path))?;

        let highlights: HashMap<String, HighlightPattern> =
            toml::from_str(&contents).context("Failed to parse common highlights TOML")?;

        Ok(highlights)
    }

    /// Load highlights for a character, merging global + character-specific
    /// Character-specific highlights override global ones with the same name
    pub fn load_highlights(character: Option<&str>) -> Result<HashMap<String, HighlightPattern>> {
        // Start with global/common highlights
        let mut highlights = Self::load_common_highlights()?;

        // Load character-specific highlights
        let highlights_path = Self::highlights_path(character)?;

        if highlights_path.exists() {
            let contents =
                fs::read_to_string(&highlights_path).context("Failed to read highlights.toml")?;
            let character_highlights: HashMap<String, HighlightPattern> =
                toml::from_str(&contents).context("Failed to parse highlights.toml")?;

            // Character highlights override global (HashMap::extend)
            highlights.extend(character_highlights);
        } else if highlights.is_empty() {
            // No global and no character highlights - use embedded defaults
            highlights = toml::from_str(DEFAULT_HIGHLIGHTS).unwrap_or_default();
        }

        // Compile all regex patterns for performance
        Self::compile_highlight_patterns(&mut highlights);

        Ok(highlights)
    }

    /// Compile regex patterns for all highlights (performance optimization)
    pub fn compile_highlight_patterns(highlights: &mut HashMap<String, HighlightPattern>) {
        for (name, pattern) in highlights.iter_mut() {
            // A condition-driven alert has no pattern at all. An empty regex
            // is valid and matches EVERY line, so compiling one would turn a
            // condition alert into a rule that fires on all text. Leave it
            // uncompiled: the engine skips rules without a regex.
            if pattern.pattern.is_empty() {
                pattern.compiled_regex = None;
                continue;
            }
            if !pattern.fast_parse {
                // Only compile regex for non-fast_parse patterns
                match regex::Regex::new(&pattern.pattern) {
                    Ok(regex) => {
                        pattern.compiled_regex = Some(regex);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to compile regex for highlight '{}': {}", name, e);
                        pattern.compiled_regex = None;
                    }
                }
            }
        }
    }

    /// Save highlights to highlights.toml for a character
    pub(crate) fn save_highlights(&self, character: Option<&str>) -> Result<()> {
        let highlights_path = Self::highlights_path(character)?;
        let contents =
            toml::to_string_pretty(&self.highlights).context("Failed to serialize highlights")?;
        write_atomic(&highlights_path, contents).context("Failed to write highlights.toml")?;
        Ok(())
    }

    /// Save a highlight to common (global) highlights file
    /// This makes the highlight available to all characters
    pub fn save_common_highlight(name: &str, pattern: &HighlightPattern) -> Result<()> {
        // Ensure global directory exists
        let global_dir = Self::global_dir()?;
        fs::create_dir_all(&global_dir)
            .with_context(|| format!("Failed to create global directory: {:?}", global_dir))?;

        // Load existing common highlights
        let mut highlights = Self::load_common_highlights()?;

        // Add or update the pattern
        highlights.insert(name.to_string(), pattern.clone());

        // Write back to file
        let path = Self::common_highlights_path()?;
        let toml =
            toml::to_string_pretty(&highlights).context("Failed to serialize common highlights")?;

        write_atomic(&path, toml)
            .with_context(|| format!("Failed to write common highlights: {:?}", path))?;

        Ok(())
    }

    /// Delete a highlight from common (global) highlights file
    pub fn delete_common_highlight(name: &str) -> Result<()> {
        let mut highlights = Self::load_common_highlights()?;
        highlights.remove(name);

        let path = Self::common_highlights_path()?;
        let toml =
            toml::to_string_pretty(&highlights).context("Failed to serialize common highlights")?;

        write_atomic(&path, toml)
            .with_context(|| format!("Failed to write common highlights: {:?}", path))?;

        Ok(())
    }

    /// Load ONLY character-specific highlights (no merge with global)
    /// Used for source tracking in UI to distinguish [G] vs [C] highlights
    pub fn load_character_highlights_only(
        character: Option<&str>,
    ) -> Result<HashMap<String, HighlightPattern>> {
        let highlights_path = Self::highlights_path(character)?;

        if !highlights_path.exists() {
            return Ok(HashMap::new());
        }

        let contents =
            fs::read_to_string(&highlights_path).context("Failed to read highlights.toml")?;
        let highlights: HashMap<String, HighlightPattern> =
            toml::from_str(&contents).context("Failed to parse highlights.toml")?;

        Ok(highlights)
    }

    /// Save a single highlight to the appropriate file based on scope
    /// is_global = true: saves to global/highlights.toml
    /// is_global = false: saves to profiles/{char}/highlights.toml
    pub fn save_single_highlight(
        name: &str,
        pattern: &HighlightPattern,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        if is_global {
            Self::save_common_highlight(name, pattern)
        } else {
            Self::save_character_highlight(name, pattern, character)
        }
    }

    /// Save a single highlight to character-specific highlights file
    fn save_character_highlight(
        name: &str,
        pattern: &HighlightPattern,
        character: Option<&str>,
    ) -> Result<()> {
        let highlights_path = Self::highlights_path(character)?;

        // Ensure parent directory exists
        if let Some(parent) = highlights_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        // Load existing character highlights
        let mut highlights = Self::load_character_highlights_only(character)?;

        // Add or update the pattern
        highlights.insert(name.to_string(), pattern.clone());

        // Write back to file
        let toml = toml::to_string_pretty(&highlights)
            .context("Failed to serialize character highlights")?;

        write_atomic(&highlights_path, toml)
            .with_context(|| format!("Failed to write highlights: {:?}", highlights_path))?;

        tracing::info!(
            "Saved highlight '{}' to character highlights file: {:?}",
            name,
            highlights_path
        );

        Ok(())
    }

    /// Delete a single highlight from the appropriate file based on scope
    /// is_global = true: deletes from global/highlights.toml
    /// is_global = false: deletes from profiles/{char}/highlights.toml
    /// Delete a highlight from BOTH scopes — the global file and this
    /// character's override file. A character override can shadow a
    /// same-named global entry; deleting only the override just uncovers
    /// the global copy on the next reload, which reads as "delete didn't
    /// work". User-facing Delete buttons mean "make this highlight gone",
    /// so every copy goes. (Scoped deletion below stays available for the
    /// editor's internal rename/re-scope cleanup.)
    pub fn delete_highlight_everywhere(name: &str, character: Option<&str>) -> Result<()> {
        Self::delete_character_highlight(name, character)?;
        Self::delete_common_highlight(name)
    }

    pub fn delete_single_highlight(
        name: &str,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        if is_global {
            Self::delete_common_highlight(name)
        } else {
            Self::delete_character_highlight(name, character)
        }
    }

    /// Delete a single highlight from character-specific highlights file
    fn delete_character_highlight(name: &str, character: Option<&str>) -> Result<()> {
        let highlights_path = Self::highlights_path(character)?;

        if !highlights_path.exists() {
            tracing::warn!(
                "Cannot delete highlight '{}' - file does not exist: {:?}",
                name,
                highlights_path
            );
            return Ok(());
        }

        let mut highlights = Self::load_character_highlights_only(character)?;

        if highlights.remove(name).is_some() {
            let toml = toml::to_string_pretty(&highlights)
                .context("Failed to serialize character highlights")?;

            write_atomic(&highlights_path, toml)
                .with_context(|| format!("Failed to write highlights: {:?}", highlights_path))?;

            tracing::info!(
                "Deleted highlight '{}' from character highlights file: {:?}",
                name,
                highlights_path
            );
        }

        Ok(())
    }

    /// List all saved highlight profiles
    pub fn list_saved_highlights() -> Result<Vec<String>> {
        let highlights_dir = Self::highlights_dir()?;

        if !highlights_dir.exists() {
            return Ok(vec![]);
        }

        let mut profiles = vec![];
        for entry in fs::read_dir(highlights_dir)? {
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

    /// Save current highlights to a named profile
    /// Returns path to saved highlights
    pub fn save_highlights_as(&self, name: &str) -> Result<PathBuf> {
        let highlights_dir = Self::highlights_dir()?;
        fs::create_dir_all(&highlights_dir)?;

        let highlights_path = highlights_dir.join(format!("{}.toml", name));
        let contents =
            toml::to_string_pretty(&self.highlights).context("Failed to serialize highlights")?;
        write_atomic(&highlights_path, contents).context("Failed to write highlights profile")?;

        Ok(highlights_path)
    }

    /// Load highlights from a named profile
    pub fn load_highlights_from(name: &str) -> Result<HashMap<String, HighlightPattern>> {
        let highlights_dir = Self::highlights_dir()?;
        let highlights_path = highlights_dir.join(format!("{}.toml", name));

        if !highlights_path.exists() {
            return Err(anyhow::anyhow!("Highlight profile '{}' not found", name));
        }

        let contents =
            fs::read_to_string(&highlights_path).context("Failed to read highlights profile")?;
        let highlights: HashMap<String, HighlightPattern> =
            toml::from_str(&contents).context("Failed to parse highlights profile")?;

        Ok(highlights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // Highlight field-catalog parity guard
    // ===========================================

    fn covered_for(frontend: HlFrontend) -> &'static [&'static str] {
        match frontend {
            HlFrontend::Tui => HL_TUI_COVERED,
            HlFrontend::Gui => HL_GUI_COVERED,
            HlFrontend::Web => HL_WEB_COVERED,
        }
    }

    fn is_exempt(field: &str, frontend: HlFrontend) -> bool {
        HL_FIELD_EXEMPTIONS
            .iter()
            .any(|(f, fe, _)| *f == field && *fe == frontend)
    }

    /// THE parity guard: every catalog field must be edited by every editor
    /// it applies to, unless explicitly exempt. Adding a HighlightPattern
    /// field to the catalog without wiring it into an editor fails here — the
    /// highlight analogue of registry.rs's leaf-coverage test.
    #[test]
    fn every_highlight_field_is_editable_where_it_applies() {
        let mut missing = Vec::new();
        for def in HIGHLIGHT_FIELDS {
            for &frontend in def.applies_to {
                if is_exempt(def.name, frontend) {
                    continue;
                }
                if !covered_for(frontend).contains(&def.name) {
                    missing.push(format!("{} in {:?}", def.name, frontend));
                }
            }
        }
        missing.sort();
        assert!(
            missing.is_empty(),
            "highlight fields missing from an editor's coverage (wire them into \
             the editor and add to its HL_*_COVERED manifest, or add an \
             HL_FIELD_EXEMPTIONS entry with a reason): {:#?}",
            missing
        );
    }

    /// Reverse guard: no manifest may claim a field the catalog doesn't have
    /// (a typo or a removed field), and no exemption may reference a field or
    /// frontend pairing that isn't real.
    #[test]
    fn manifests_and_exemptions_reference_real_fields() {
        let names: std::collections::HashSet<&str> =
            HIGHLIGHT_FIELDS.iter().map(|d| d.name).collect();
        for frontend in [HlFrontend::Tui, HlFrontend::Gui, HlFrontend::Web] {
            for field in covered_for(frontend) {
                assert!(names.contains(field), "manifest {:?} lists unknown field '{}'", frontend, field);
            }
        }
        for (field, frontend, reason) in HL_FIELD_EXEMPTIONS {
            assert!(names.contains(field), "exemption for unknown field '{}'", field);
            assert!(!reason.is_empty(), "exemption for '{}' in {:?} needs a reason", field, frontend);
            let def = HIGHLIGHT_FIELDS.iter().find(|d| d.name == *field).unwrap();
            assert!(
                def.applies_to.contains(frontend),
                "'{}' exempted from {:?} but doesn't apply there anyway",
                field, frontend
            );
        }
    }

    /// The web manifest can't lie: every field it claims must have its DOM
    /// element id actually present in the embedded app.js. This turns "the
    /// phone form is missing field X" into a red build even though the form
    /// itself is JavaScript.
    #[test]
    fn web_highlight_manifest_matches_app_js() {
        let app_js = include_str!("../frontend/web/assets/app.js");
        let mut absent = Vec::new();
        for field in HL_WEB_COVERED {
            let id = hl_web_element_id(field);
            // ids appear as getElementById("<id>") in the form code.
            if !app_js.contains(id) {
                absent.push(format!("{field} (id '{id}')"));
            }
        }
        assert!(
            absent.is_empty(),
            "web manifest lists highlight fields whose element id is not in \
             app.js — add the control to the phone form or drop it from \
             HL_WEB_COVERED: {:#?}",
            absent
        );
    }

    // ===========================================
    // RedirectMode tests
    // ===========================================

    #[test]
    fn test_redirect_mode_default() {
        let mode = RedirectMode::default();
        assert_eq!(mode, RedirectMode::RedirectOnly);
    }

    #[test]
    fn test_redirect_mode_equality() {
        assert_eq!(RedirectMode::RedirectOnly, RedirectMode::RedirectOnly);
        assert_eq!(RedirectMode::RedirectCopy, RedirectMode::RedirectCopy);
        assert_ne!(RedirectMode::RedirectOnly, RedirectMode::RedirectCopy);
    }

    #[test]
    fn test_redirect_mode_clone() {
        let mode = RedirectMode::RedirectCopy;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    // ===========================================
    // HighlightPattern tests
    // ===========================================

    #[test]
    fn test_highlight_pattern_basic() {
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        };

        assert_eq!(pattern.pattern, "test");
        assert_eq!(pattern.fg, Some("#FF0000".to_string()));
        assert!(pattern.bg.is_none());
        assert!(!pattern.bold);
    }

    #[test]
    fn test_highlight_pattern_with_all_options() {
        let pattern = HighlightPattern {
            pattern: r"\d+ damage".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: Some("#330000".to_string()),
            bold: true,
            color_entire_line: true,
            fast_parse: false,
            sound: Some("damage.wav".to_string()),
            sound_volume: Some(0.8),
            rumble: None,
            category: Some("Combat".to_string()),
            squelch: false,
            silent_prompt: false,
            redirect_to: Some("combat".to_string()),
            redirect_mode: RedirectMode::RedirectCopy,
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        };

        assert!(pattern.bold);
        assert!(pattern.color_entire_line);
        assert_eq!(pattern.sound, Some("damage.wav".to_string()));
        assert_eq!(pattern.sound_volume, Some(0.8));
        assert_eq!(pattern.category, Some("Combat".to_string()));
        assert_eq!(pattern.redirect_to, Some("combat".to_string()));
        assert_eq!(pattern.redirect_mode, RedirectMode::RedirectCopy);
    }

    #[test]
    fn test_highlight_pattern_squelch() {
        let pattern = HighlightPattern {
            pattern: "spam message".to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: Some("Ignore".to_string()),
            squelch: true,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        };

        assert!(pattern.squelch);
    }

    #[test]
    fn test_highlight_pattern_fast_parse() {
        let pattern = HighlightPattern {
            pattern: "word1|word2|word3".to_string(),
            fg: Some("#00FF00".to_string()),
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: true, // Uses Aho-Corasick
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        };

        assert!(pattern.fast_parse);
    }

    #[test]
    fn test_highlight_pattern_clone() {
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: Some("Test".to_string()),
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        };

        let cloned = pattern.clone();
        assert_eq!(cloned.pattern, pattern.pattern);
        assert_eq!(cloned.fg, pattern.fg);
        assert_eq!(cloned.bold, pattern.bold);
        assert_eq!(cloned.category, pattern.category);
    }

    // ===========================================
    // EventAction tests
    // ===========================================

    #[test]
    fn test_event_action_default() {
        let action = EventAction::default();
        assert!(matches!(action, EventAction::Set));
    }

    #[test]
    fn test_event_action_variants() {
        let set = EventAction::Set;
        let clear = EventAction::Clear;
        let increment = EventAction::Increment;

        assert!(matches!(set, EventAction::Set));
        assert!(matches!(clear, EventAction::Clear));
        assert!(matches!(increment, EventAction::Increment));
    }

    #[test]
    fn test_event_action_clone() {
        let action = EventAction::Clear;
        let cloned = action.clone();
        assert!(matches!(cloned, EventAction::Clear));
    }

    // ===========================================
    // EventPattern tests
    // ===========================================

    #[test]
    fn test_event_pattern_basic() {
        let pattern = EventPattern {
            pattern: r"You are stunned for (\d+) seconds".to_string(),
            event_type: "stun".to_string(),
            action: EventAction::Set,
            duration: 0,
            duration_capture: Some(1),
            duration_multiplier: 1.0,
            enabled: true,
        };

        assert_eq!(pattern.event_type, "stun");
        assert!(matches!(pattern.action, EventAction::Set));
        assert_eq!(pattern.duration_capture, Some(1));
        assert!(pattern.enabled);
    }

    #[test]
    fn test_event_pattern_with_fixed_duration() {
        let pattern = EventPattern {
            pattern: "You fall prone".to_string(),
            event_type: "prone".to_string(),
            action: EventAction::Set,
            duration: 3,
            duration_capture: None,
            duration_multiplier: 1.0,
            enabled: true,
        };

        assert_eq!(pattern.duration, 3);
        assert!(pattern.duration_capture.is_none());
    }

    #[test]
    fn test_event_pattern_with_multiplier() {
        let pattern = EventPattern {
            pattern: r"Webbed for (\d+) rounds".to_string(),
            event_type: "webbed".to_string(),
            action: EventAction::Set,
            duration: 0,
            duration_capture: Some(1),
            duration_multiplier: 5.0, // Convert rounds to seconds
            enabled: true,
        };

        assert_eq!(pattern.duration_multiplier, 5.0);
    }

    #[test]
    fn test_event_pattern_disabled() {
        let pattern = EventPattern {
            pattern: "test".to_string(),
            event_type: "test".to_string(),
            action: EventAction::Set,
            duration: 0,
            duration_capture: None,
            duration_multiplier: 1.0,
            enabled: false,
        };

        assert!(!pattern.enabled);
    }

    #[test]
    fn test_event_pattern_clear_action() {
        let pattern = EventPattern {
            pattern: "You recover from the stun".to_string(),
            event_type: "stun".to_string(),
            action: EventAction::Clear,
            duration: 0,
            duration_capture: None,
            duration_multiplier: 1.0,
            enabled: true,
        };

        assert!(matches!(pattern.action, EventAction::Clear));
    }

    // ===========================================
    // Helper function tests
    // ===========================================

    #[test]
    fn test_is_false_helper() {
        assert!(is_false(&false));
        assert!(!is_false(&true));
    }

    #[test]
    fn test_default_duration_multiplier() {
        assert_eq!(default_duration_multiplier(), 1.0);
    }

    #[test]
    fn test_default_enabled() {
        assert!(default_enabled());
    }

    // ===========================================
    // Serialization tests (via TOML)
    // ===========================================

    #[test]
    fn test_highlight_pattern_serialization() {
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: Some("#FF0000".to_string()),
            bg: None,
            bold: true,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        };

        let toml_str = toml::to_string(&pattern).unwrap();
        assert!(toml_str.contains("pattern = \"test\""));
        assert!(toml_str.contains("fg = \"#FF0000\""));
        assert!(toml_str.contains("bold = true"));
        // Options with false should be skipped (skip_serializing_if = "is_false")
    }

    #[test]
    fn test_highlight_pattern_deserialization() {
        let toml_str = r##"
            pattern = "damage"
            fg = "#FF0000"
            bold = true
        "##;

        let pattern: HighlightPattern = toml::from_str(toml_str).unwrap();
        assert_eq!(pattern.pattern, "damage");
        assert_eq!(pattern.fg, Some("#FF0000".to_string()));
        assert!(pattern.bold);
        assert!(!pattern.squelch); // Default
        assert!(!pattern.fast_parse); // Default
    }

    #[test]
    fn test_redirect_mode_serialization() {
        // TOML can't serialize bare enums - test it as part of a HighlightPattern
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: Some("combat".to_string()),
            redirect_mode: RedirectMode::RedirectCopy,
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        };
        let toml_str = toml::to_string(&pattern).unwrap();
        assert!(toml_str.contains("redirect_mode = \"redirect_copy\""));
    }

    #[test]
    fn test_event_pattern_serialization() {
        let pattern = EventPattern {
            pattern: "stunned".to_string(),
            event_type: "stun".to_string(),
            action: EventAction::Set,
            duration: 5,
            duration_capture: None,
            duration_multiplier: 1.0,
            enabled: true,
        };

        let toml_str = toml::to_string(&pattern).unwrap();
        assert!(toml_str.contains("pattern = \"stunned\""));
        assert!(toml_str.contains("event_type = \"stun\""));
    }

    // ===========================================
    // Debug trait tests
    // ===========================================

    #[test]
    fn test_redirect_mode_debug() {
        let mode = RedirectMode::RedirectOnly;
        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("RedirectOnly"));
    }

    #[test]
    fn test_highlight_pattern_debug() {
        let pattern = HighlightPattern {
            pattern: "test".to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        };

        let debug_str = format!("{:?}", pattern);
        assert!(debug_str.contains("HighlightPattern"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_event_action_debug() {
        let action = EventAction::Increment;
        let debug_str = format!("{:?}", action);
        assert!(debug_str.contains("Increment"));
    }
}
