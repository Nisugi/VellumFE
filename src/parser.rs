//! Streaming XML parser that converts GemStone IV data into strongly typed events.
//!
//! The parser keeps track of nested styles, open streams, dialog fragments, and
//! ad-hoc pattern detectors (e.g., event timers) so the rest of the client can
//! operate on higher-level `ParsedElement` values instead of raw XML.

mod dialogs;
mod handlers;
mod links;
mod text;

use crate::config::EventAction;
use crate::data::{DialogButton, DialogDropDown, LinkData, QuickbarEntry};
use regex::Regex;
use std::sync::LazyLock;
use std::collections::HashMap;

/// Text categories emitted by the XML stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanType {
    Normal,      // Regular text
    Link,        // <a> tag from parser
    Monsterbold, // <preset id="monsterbold"> from parser
    Spell,       // <spell> tag from parser
    Speech,      // <preset id="speech"> from parser
}

/// Parse numeric current/max out of a progress bar text string.
/// Supports:
/// - "label 324/326" -> (324, 326)
/// - "324/326" -> (324, 326)
/// - "label (100%)" or "label 100%" -> (100, 100)
/// - "label" -> (percentage, 100)
fn parse_progress_numbers(text: &str, percentage: u32) -> (u32, u32) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return (percentage, 100);
    }

    // Slash form: current/max
    if let Some(slash_pos) = trimmed.rfind('/') {
        let before_slash = &trimmed[..slash_pos];
        let after_slash = &trimmed[slash_pos + 1..];

        let current = last_number(before_slash).unwrap_or(percentage);
        let maximum = first_number(after_slash).unwrap_or(100);
        return (current, maximum);
    }

    // Percent or single number form: treat as current, max = 100
    if let Some(num) = first_number(trimmed) {
        return (num, 100);
    }

    // Label-only: fall back to percentage/max
    (percentage, 100)
}

fn first_number(input: &str) -> Option<u32> {
    input
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '%')
        .find_map(|token| token.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
}

fn last_number(input: &str) -> Option<u32> {
    input
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '%')
        .rev()
        .find_map(|token| token.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
}

/// Top-level representation of any XML fragment we care about.
#[derive(Debug, Clone)]
pub enum ParsedElement {
    Text {
        content: String,
        stream: String,
        fg_color: Option<String>,
        bg_color: Option<String>,
        bold: bool,
        /// Inside an `<output class="mono"/>` region: render monospace.
        mono: bool,
        span_type: SpanType,
        link_data: Option<LinkData>,
    },
    Prompt {
        time: String,
        text: String,
    },
    Spell {
        text: String,
    },
    LeftHand {
        item: String,
        link: Option<LinkData>,
    },
    RightHand {
        item: String,
        link: Option<LinkData>,
    },
    SpellHand {
        spell: String,
    },
    RoundTime {
        value: u32,
    },
    CastTime {
        value: u32,
    },
    /// `<vellumTimer id='...' value='...'/>` - VellumFE extension for
    /// script-driven countdowns. `id` names the countdown feed id, `value`
    /// is the absolute epoch end time in seconds (0 or past clears).
    VellumTimer {
        id: String,
        value: i64,
    },
    /// Client command injected by the feed (`<vellumCmd cmd=".header off"/>`,
    /// typically emitted by a Lich script). Only dot-commands are honored
    /// downstream, so the feed can drive client UI but never send game
    /// commands.
    VellumCommand {
        command: String,
    },
    ProgressBar {
        id: String,
        value: u32,
        max: u32,
        text: String,
    },
    Label {
        id: String,
        value: String,
    },
    Compass {
        directions: Vec<String>,
    },
    Component {
        id: String,
        value: String,
    },
    StreamPush {
        id: String,
    },
    /// `<crtrStatus exist="..." stunned="1" .../>` - full snapshot of one
    /// creature's status flags. `attrs` holds every attribute except `exist`,
    /// raw, so the core layer owns the flag-name mapping.
    CreatureStatus {
        id: String,
        attrs: Vec<(String, String)>,
    },
    /// `<roommeta climate="3" terrain="7" .../>` - self-closing room
    /// metadata snapshot (numeric codes). Attributes are passed raw so the
    /// core layer owns the field mapping, like CreatureStatus.
    RoomMeta {
        attrs: Vec<(String, String)>,
    },
    /// Exact-experience attributes riding on `<progressBar id='mindState'>`,
    /// emitted alongside the ProgressBar element for every mindState bar.
    /// The exp numbers are sticky (None = not sent this update); the
    /// event-bonus flags are a snapshot - absent means the bonus ended and
    /// stored state must clear, which is why this fires even with no attrs.
    MindStateExp {
        field_exp: Option<u64>,
        max_field_exp: Option<u64>,
        exp: Option<u64>,
        ascension_exp: Option<u64>,
        until_next: Option<u64>,
        /// Fash'lonae orb: 1 = redeemed (inactive), 2 = active
        fashlonae: Option<u8>,
        lumnis: Option<u8>,
        /// RPA bonus multiplier; can be fractional (e.g. 1.5)
        rpa: Option<f32>,
    },
    StreamPop,
    ClearStream {
        id: String,
    },
    ClearDialogData {
        id: String,
    },
    CloseDialog {
        id: String,
    },
    /// Game verb `<exposeDialog id=.../>` / `<exposeStream>` /
    /// `<exposeContainer>` — "show this window NOW" (wire-verified: bank
    /// sends exposeDialog ×4,265). `kind` is "dialog"/"stream"/
    /// "container". DARK in redesign Phase 1; expose-=-show semantics
    /// land in Phase 4.
    Expose {
        kind: String,
        id: String,
    },
    /// `<deleteContainer id=.../>` (×7,559 on the wire) — container
    /// removal; clearContainer was handled, delete was silently dropped.
    /// DARK in Phase 1.
    DeleteContainer {
        id: String,
    },
    /// Placement/persistence attributes riding a window-declaring tag
    /// (`streamWindow`/`openDialog`/`container`), raw — the PlacementHint
    /// input (location/resident/save/scroll/ifClosed/appearance/size…).
    /// Emitted alongside the declaration's own element; the core layer
    /// owns the mapping, like CreatureStatus. DARK in Phase 1.
    WindowHints {
        id: String,
        attrs: Vec<(String, String)>,
    },
    /// Login-time application info: `<app char="Nisugi" game="GS" .../>`.
    /// The authoritative source of the character name in the game feed.
    AppInfo {
        character: String,
    },
    RoomId {
        id: String,
    },
    StreamWindow {
        id: String,
        subtitle: Option<String>,
        /// Human-friendly stream label (e.g. title="Room"). Used to give
        /// custom-window authoring a readable name for a stream id.
        title: Option<String>,
    },
    InjuryImage {
        id: String,   // Body part: "head", "leftArm", etc.
        name: String, // Injury level: "Injury1", "Injury2", "Injury3", "Scar1", "Scar2", "Scar3"
    },
    /// Injury data for another player's injuries popup dialog
    InjuryPopupData {
        popup_id: String,                               // Dialog ID: "injuries-10154507"
        injuries: Vec<(String, String)>,                // Vec of (body_part, injury_level)
        clear: bool,                                    // true if clearing injuries
    },
    StatusIndicator {
        id: String,   // Status type: "poisoned", "diseased", "bleeding", "stunned"
        active: bool, // true = active, false = clear
    },
    ActiveEffect {
        category: String, // "ActiveSpells", "Buffs", "Debuffs", "Cooldowns"
        id: String,
        value: u32,
        text: String,
        time: String, // Format: "HH:MM:SS"
    },
    ClearActiveEffects {
        category: String, // Which category to clear
    },
    MenuResponse {
        id: String,                            // Correlation ID (counter)
        coords: Vec<(String, Option<String>)>, // List of (coord, optional noun) pairs from <mi> tags
    },
    QuickbarOpen {
        id: String,
        title: Option<String>,
    },
    QuickbarEntries {
        id: String,
        clear: bool,
        entries: Vec<QuickbarEntry>,
    },
    QuickbarSwitch {
        id: String,
    },
    DialogOpen {
        id: String,
        title: Option<String>,
        save: bool, // true if save='t' - position should be persisted
        /// openDialog `location` (right/center/detach/…): detach marks the
        /// utility-popup class (bugDialogBox) that pops without opt-in.
        location: Option<String>,
    },
    DialogButtons {
        id: String,
        clear: bool,
        buttons: Vec<DialogButton>,
    },
    DialogDropDowns {
        id: String,
        clear: bool,
        dropdowns: Vec<DialogDropDown>,
    },
    DialogControls {
        id: String,
        clear: bool,
        links: Vec<crate::data::DialogLink>,
        images: Vec<crate::data::DialogImage>,
        spinboxes: Vec<crate::data::DialogSpinBox>,
        skins: Vec<crate::data::DialogSkin>,
    },
    /// A resident dialog announcing itself as a persistent panel (combat,
    /// Buffs, injuries, ...) — registers a resident window offer rather
    /// than a transient popup.
    DialogPanelOpen {
        id: String,
        title: Option<String>,
        save: bool,
    },
    DialogFields {
        id: String,
        clear: bool,
        fields: Vec<DialogFieldSpec>,
        labels: Vec<DialogLabelSpec>,
    },
    DialogLabelList {
        id: String,
        clear: bool,
        labels: Vec<DialogLabelSpec>,
    },
    DialogProgressBars {
        id: String,
        clear: bool,
        progress_bars: Vec<DialogProgressBarSpec>,
    },
    Event {
        event_type: String,  // "stun", "webbed", "prone", etc.
        action: EventAction, // Set/Clear/Increment
        duration: u32,       // Duration in seconds (for countdowns)
    },
    LaunchURL {
        url: String, // URL path to append to https://www.play.net
    },
    /// Lich WebUI handshake reply (`;ui handshake` -> one `<LichWebUI .../>` line)
    LichWebUI(crate::data::webui::WebUiHandshake),
    /// Target list from combat dialog dropdown (for direct-connect users)
    TargetList {
        current_target: String,  // from value attribute
        target_ids: Vec<String>, // from content_value (comma-split)
    },
    /// Container window definition
    Container {
        id: String,
        title: String,
        /// The `target` attribute (`#<exist-id>`), the id game commands
        /// use. Equals `#id` for normal containers, but differs for
        /// `stow` (id is the string "stow", target is the real object).
        target: Option<String>,
    },
    /// Clear container contents
    ClearContainer {
        id: String,
    },
    /// Item in a container (from <inv id='X'> tags)
    ContainerItem {
        container_id: String,
        content: String, // Full line with links preserved
    },
}

#[derive(Debug, Clone)]
pub struct DialogFieldSpec {
    pub id: String,
    pub value: String,
    pub enter_button: Option<String>,
    pub focused: bool,
}

#[derive(Debug, Clone)]
pub struct DialogLabelSpec {
    pub id: String,
    pub value: String,
    /// Anchor-grid layout hints (None when the tag carried none).
    pub layout: Option<crate::data::DialogControlLayout>,
    /// Wrayth `justify` bitfield: low two bits = alignment (0 = left,
    /// 1 = center, 2 = right), bit 4 = flag (so 4/5/6 = flagged variants).
    /// Decoded by `DialogLabel::align`.
    pub justify: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct DialogProgressBarSpec {
    pub id: String,
    pub value: u32,   // Percentage 0-100
    pub text: String, // Display text (e.g., "defensive (100%)")
    /// Anchor-grid layout hints (None when the tag carried none).
    pub layout: Option<crate::data::DialogControlLayout>,
}

/// Tracks the currently active foreground/background/bold settings while the
/// parser walks nested XML tags.
#[derive(Debug, Clone)]
#[derive(Default)]
pub(crate) struct ColorStyle {
    fg: Option<String>,
    bg: Option<String>,
}


/// Stateful streaming parser that consumes wizard XML chunks and emits
/// high-level `ParsedElement` values.
#[derive(Clone)]
pub struct XmlParser {
    current_stream: String,
    presets: HashMap<String, (Option<String>, Option<String>)>, // id -> (fg, bg)

    // State tracking for nested tags
    pub(crate) color_stack: Vec<ColorStyle>,
    pub(crate) preset_stack: Vec<ColorStyle>,
    pub(crate) style_stack: Vec<ColorStyle>,
    pub(crate) bold_stack: Vec<bool>,
    /// Inside an `<output class="mono"/>` region (game tables/ASCII art);
    /// cleared by `<output class=""/>`. Stamped onto text so the GUI can
    /// render these spans in its monospace font.
    pub(crate) mono_output: bool,

    // Semantic type tracking
    pub(crate) link_depth: usize,                   // Track nested links
    pub(crate) spell_depth: usize,                  // Track nested spells
    pub(crate) current_link_data: Option<LinkData>, // Current link metadata (exist_id, noun) = top of link_stack
    /// Stack of open link metadata, one entry per open `<a>`/`<d>`. Nested
    /// links (e.g. `store list`'s `<d cmd=...>a <a ...>item</a> ...</d>`) are
    /// two different links; a stack lets each text span attach the INNERMOST
    /// open link, and pop-on-close restores the outer command for the text
    /// that follows the inner link. `current_link_data` mirrors the top.
    pub(crate) link_stack: Vec<LinkData>,
    /// Parallel to `link_stack`: whether each open `<a>` pushed a color onto
    /// `color_stack`. The close pops color iff its own open pushed one, so a
    /// link that opens outside bold but closes inside it (or vice versa) still
    /// balances — without this, the color leaked onto every later line.
    pub(crate) link_pushed_color: Vec<bool>,
    pub(crate) current_preset_id: Option<String>, // Current preset ID (e.g., "speech", "monsterbold")
    // Menu tracking
    current_menu_id: Option<String>, // ID of menu being parsed
    current_menu_coords: Vec<(String, Option<String>)>, // (coord, optional noun) pairs for current menu

    // Event pattern matching
    event_matchers: Vec<(Regex, crate::config::EventPattern)>, // Compiled regexes + patterns
}

impl XmlParser {
    fn compile_event_matchers(
        event_patterns: HashMap<String, crate::config::EventPattern>,
    ) -> Vec<(Regex, crate::config::EventPattern)> {
        let mut event_matchers = Vec::new();
        for (name, pattern) in event_patterns {
            if !pattern.enabled {
                continue;
            }

            match Regex::new(&pattern.pattern) {
                Ok(regex) => {
                    event_matchers.push((regex, pattern));
                }
                Err(e) => {
                    tracing::warn!("Invalid event pattern '{}': {}", name, e);
                }
            }
        }
        event_matchers
    }

    /// Create a parser with empty preset/event tables.
    pub fn new() -> Self {
        Self::with_presets(vec![], HashMap::new())
    }

    /// Create a parser primed with preset definitions and event patterns.
    pub fn with_presets(
        preset_list: Vec<(String, Option<String>, Option<String>)>,
        event_patterns: HashMap<String, crate::config::EventPattern>,
    ) -> Self {
        let mut presets = HashMap::new();

        // Load presets from config
        for (id, fg, bg) in preset_list {
            presets.insert(id, (fg, bg));
        }

        // Compile event pattern regexes
        let event_matchers = Self::compile_event_matchers(event_patterns);

        Self {
            current_stream: "main".to_string(),
            presets,
            color_stack: vec![],
            preset_stack: vec![],
            style_stack: vec![],
            bold_stack: vec![],
            mono_output: false,
            link_depth: 0,
            spell_depth: 0,
            current_link_data: None,
            link_stack: Vec::new(),
            link_pushed_color: Vec::new(),
            current_preset_id: None,
            current_menu_id: None,
            current_menu_coords: Vec::new(),
            event_matchers,
        }
    }

    /// Update presets after loading new color config
    pub fn update_presets(&mut self, preset_list: Vec<(String, Option<String>, Option<String>)>) {
        let mut presets = HashMap::new();
        for (id, fg, bg) in preset_list {
            presets.insert(id, (fg, bg));
        }
        self.presets = presets;
    }

    /// Update event patterns after reloading configuration
    pub fn update_event_patterns(
        &mut self,
        event_patterns: HashMap<String, crate::config::EventPattern>,
    ) {
        self.event_matchers = Self::compile_event_matchers(event_patterns);
    }

    pub fn parse_line(&mut self, line: &str) -> Vec<ParsedElement> {
        // Filter out GSL (GemStone Language) protocol tags from Lich proxy
        // GSL tags start with \x1C (File Separator, ASCII 28) followed by "GS" + letter + data
        // Examples: \x1CGSB (char info), \x1CGSj (compass), \x1CGSg (stance), \x1CGSP (prompt)
        // These are internal protocol messages not meant for display

        // Check if line is purely a GSL tag - if so, skip it entirely (no blank line)
        if Self::is_gsl_tag_line(line) {
            tracing::debug!("[GSL] Skipping GSL tag line: '{}'", line);
            return vec![];
        }

        let line = Self::strip_gsl_tags(line);

        // Preserve intentional blank lines from the server output.
        // Without this, empty lines would be dropped and formatting that relies on vertical spacing
        // would collapse.
        if line.is_empty() {
            return vec![self.create_text_element(String::new())];
        }

        let mut elements = Vec::new();
        let mut text_buffer = String::new();
        let mut remaining: &str = &line;

        while !remaining.is_empty() {
            // Check for paired tags first (manually check for each type)
            let mut found_paired = false;

            // Static start/end patterns - building these with format! allocated
            // 2 Strings x 10 tags per loop iteration in the hottest parse loop
            const PAIRED_TAGS: [(&str, &str); 10] = [
                ("<prompt", "</prompt>"),
                ("<spell", "</spell>"),
                ("<left", "</left>"),
                ("<right", "</right>"),
                ("<compass", "</compass>"),
                ("<openDialog", "</openDialog>"),
                ("<dialogData", "</dialogData>"),
                ("<component", "</component>"),
                ("<compDef", "</compDef>"),
                ("<inv", "</inv>"),
            ];

            // A paired tag is only handled when the first '<' in the tail
            // starts one, so find that '<' once and prefix-check the ten
            // patterns there instead of scanning the whole tail per pattern.
            if let Some(tag_start) = remaining.find('<') {
                for (start_pattern, end_pattern) in PAIRED_TAGS {
                    if !remaining[tag_start..].starts_with(start_pattern) {
                        continue;
                    }

                    // Find the closing tag
                    if let Some(tag_end_start) = remaining[tag_start..].find(end_pattern) {
                        let tag_end = tag_start + tag_end_start + end_pattern.len();

                        // Add text before the paired tag
                        if tag_start > 0 {
                            text_buffer.push_str(&remaining[..tag_start]);
                        }

                        // Process the complete paired tag
                        let whole_tag = &remaining[tag_start..tag_end];
                        self.process_tag(whole_tag, &mut text_buffer, &mut elements);

                        remaining = &remaining[tag_end..];
                        found_paired = true;
                        break;
                    }
                }
            }

            if found_paired {
                continue;
            }

            // Find next single XML tag
            if let Some(tag_start) = remaining.find('<') {
                // Add text before tag to buffer
                if tag_start > 0 {
                    text_buffer.push_str(&remaining[..tag_start]);
                }

                // Find tag end
                if let Some(tag_end) = remaining[tag_start..].find('>') {
                    let tag = &remaining[tag_start..tag_start + tag_end + 1];

                    // Process the tag (may flush buffer)
                    self.process_tag(tag, &mut text_buffer, &mut elements);

                    remaining = &remaining[tag_start + tag_end + 1..];
                } else {
                    // No closing >, treat rest as text
                    text_buffer.push_str(remaining);
                    break;
                }
            } else {
                // No more tags, add remaining as text
                text_buffer.push_str(remaining);
                break;
            }
        }

        // Flush any remaining text
        self.flush_text_with_events(text_buffer, &mut elements);

        elements
    }

    /// Close-tag match that tolerates mangled trailing junk before the '>'.
    /// Game data occasionally ships broken escaping — e.g. ability HELP text
    /// with `$<a href=$Q...$>Recent Evasion$</a$>` — and a strict `"</a>"`
    /// comparison leaves the link style open, bleeding link color over
    /// everything after it. The name must end at a non-alphanumeric char so
    /// `</a$>` closes a link but `</app>` never does.
    fn is_close_tag(tag: &str, name: &str) -> bool {
        tag.strip_prefix("</")
            .and_then(|rest| rest.strip_prefix(name))
            .is_some_and(|rest| !rest.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()))
    }

    fn process_tag(
        &mut self,
        tag: &str,
        text_buffer: &mut String,
        elements: &mut Vec<ParsedElement>,
    ) {
        // Determine if this tag changes color state
        let color_opening = tag.starts_with("<preset ")
            || tag.starts_with("<color ")
            || tag.starts_with("<style ")
            || tag.starts_with("<pushBold")
            || tag.starts_with("<b>")
            || tag.starts_with("<a ")
            || tag == "<a>"
            || tag.starts_with("<d ")
            || tag == "<d>";

        let color_closing = tag == "</preset>"
            || tag == "</color>"
            || Self::is_close_tag(tag, "a")
            || Self::is_close_tag(tag, "d")
            || tag == "<popBold/>"
            || tag == "</b>";

        // Flush before opening new colors (so old styled text is emitted with old colors)
        if color_opening && !text_buffer.is_empty() {
            self.flush_text_with_events(std::mem::take(text_buffer), elements);
        }

        // Flush before closing colors (so text gets the color before we pop it)
        if color_closing && !text_buffer.is_empty() {
            self.flush_text_with_events(std::mem::take(text_buffer), elements);
        }

        // Parse tag and update state
        if tag.starts_with("<preset ") {
            self.handle_preset_open(tag);
        } else if tag == "</preset>" {
            self.handle_preset_close();
        } else if tag.starts_with("<color ") || tag.starts_with("<color>") {
            self.handle_color_open(tag);
        } else if tag == "</color>" {
            self.handle_color_close();
        } else if tag.starts_with("<style ") {
            // Flush before style change
            if !text_buffer.is_empty() {
                self.flush_text_with_events(std::mem::take(text_buffer), elements);
            }
            self.handle_style(tag);
        } else if tag.starts_with("<pushBold") || tag.starts_with("<b>") {
            self.handle_push_bold();
        } else if tag == "<popBold/>" || tag == "</b>" {
            self.handle_pop_bold();
        } else if tag.starts_with("<component ") && tag.contains("</component>") {
            // Emit Component element with content for room window updates
            if let Some(id) = Self::extract_attribute(tag, "id") {
                // Extract content between tags
                let content = if let Some(start) = tag.find('>') {
                    if let Some(end) = tag.rfind("</component>") {
                        tag[start + 1..end].to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                elements.push(ParsedElement::Component { id, value: content });
            }
        } else if tag.starts_with("<compDef ") && tag.contains("</compDef>") {
            // Emit Component element with content for room window full updates
            if let Some(id) = Self::extract_attribute(tag, "id") {
                // Extract content between tags
                let content = if let Some(start) = tag.find('>') {
                    if let Some(end) = tag.rfind("</compDef>") {
                        tag[start + 1..end].to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                elements.push(ParsedElement::Component { id, value: content });
            }
        } else if tag.starts_with("<stream ") {
            // Inline stream tag: <stream id="Spells">content</stream>
            // Flush any buffered text to the current stream before switching
            if !text_buffer.is_empty() {
                self.flush_text_with_events(std::mem::take(text_buffer), elements);
            }
            // Switch to the inline stream (handled same as pushStream)
            self.handle_push_stream(tag, elements);
        } else if tag == "</stream>" {
            // End of inline stream tag - flush buffer and pop stream
            if !text_buffer.is_empty() {
                self.flush_text_with_events(std::mem::take(text_buffer), elements);
            }
            elements.push(ParsedElement::StreamPop);
            self.current_stream = "main".to_string();
        } else if tag.starts_with("<pushStream ") {
            // If we encounter a mid-line stream switch into the speech stream, carry the
            // buffered text forward so the speech window gets the full line (including
            // the speaker). Without this, a pushStream that occurs after "You " will
            // leave the pronoun in the previous stream, cutting it off in the speech tab.
            let target_stream = Self::extract_attribute(tag, "id");
            let mut carried_prefix: Option<String> = None;
            if target_stream.as_deref() == Some("speech") && !text_buffer.is_empty() {
                // Hold onto the current buffer; don't flush to the previous stream.
                carried_prefix = Some(std::mem::take(text_buffer));
            } else if !text_buffer.is_empty() {
                self.flush_text_with_events(std::mem::take(text_buffer), elements);
            }
            self.handle_push_stream(tag, elements);
            if let Some(prefix) = carried_prefix {
                *text_buffer = prefix;
            }
        } else if tag.starts_with("<popStream") || tag == "</component>" {
            if !text_buffer.is_empty() {
                self.flush_text_with_events(std::mem::take(text_buffer), elements);
            }
            elements.push(ParsedElement::StreamPop);
            self.current_stream = "main".to_string();
        } else if tag.starts_with("<clearStream ") {
            self.handle_clear_stream(tag, elements);
        } else if tag.starts_with("<prompt ") {
            self.handle_prompt(tag, elements);
        } else if tag.starts_with("<roundTime ") {
            self.handle_roundtime(tag, elements);
        } else if tag.starts_with("<castTime ") {
            self.handle_casttime(tag, elements);
        } else if tag.starts_with("<vellumTimer ") {
            self.handle_vellum_timer(tag, elements);
        } else if tag.starts_with("<vellumCmd ") || tag.starts_with("<vellum-cmd ") {
            self.handle_vellum_cmd(tag, elements);
        } else if tag.starts_with("<spell") {
            self.handle_spell(tag, text_buffer, elements);
        } else if tag.starts_with("<left") {
            self.handle_left_hand(tag, text_buffer, elements);
        } else if tag.starts_with("<right") {
            self.handle_right_hand(tag, text_buffer, elements);
        } else if tag.starts_with("<compass") {
            self.handle_compass(tag, elements);
        } else if tag.starts_with("<dialogData ") {
            // A few dialogs key on name= instead of id= (bugDialogBox);
            // normalize so every downstream id extraction sees them.
            if !tag.contains(" id=") && tag.contains(" name=") {
                let patched = tag.replacen(" name=", " id=", 1);
                self.handle_dialog_data(&patched, elements);
            } else {
                self.handle_dialog_data(tag, elements);
            }
        } else if tag.starts_with("<openDialog ") {
            // Normalize name-keyed inner dialogData for the embedded
            // extractors too (bugDialogBox: openDialog carries id= but its
            // dialogData uses name= — the popup arrived EMPTY and never
            // showed).
            if tag.contains("<dialogData name=") {
                let patched = tag.replace("<dialogData name=", "<dialogData id=");
                self.handle_open_dialog(&patched, elements);
                self.emit_window_hints(&patched, elements);
            } else {
                self.handle_open_dialog(tag, elements);
                self.emit_window_hints(tag, elements);
            }
        } else if tag.starts_with("<closeDialog ") {
            self.handle_close_dialog(tag, elements);
        } else if tag.starts_with("<switchQuickBar ") {
            self.handle_switch_quickbar(tag, elements);
        } else if tag.starts_with("<indicator ") {
            self.handle_indicator(tag, elements);
        } else if tag.starts_with("<progressBar ") {
            self.handle_progressbar(tag, elements);
        } else if tag.starts_with("<label ") {
            self.handle_label(tag, elements);
        } else if tag.starts_with("<nav ") {
            self.handle_nav(tag, elements);
        } else if tag.starts_with("<app ") {
            self.handle_app(tag, elements);
        } else if tag.starts_with("<streamWindow ") {
            self.handle_stream_window(tag, elements);
            self.emit_window_hints(tag, elements);
        } else if tag.starts_with("<exposeDialog ") {
            self.handle_expose(tag, elements, "dialog");
        } else if tag.starts_with("<exposeStream ") {
            self.handle_expose(tag, elements, "stream");
        } else if tag.starts_with("<exposeContainer ") {
            self.handle_expose(tag, elements, "container");
        } else if tag.starts_with("<deleteContainer ") {
            if let Some(id) = Self::extract_attribute(tag, "id") {
                elements.push(ParsedElement::DeleteContainer { id });
            }
        } else if tag.starts_with("<d ") || tag == "<d>" {
            self.handle_d_tag(tag);
        } else if Self::is_close_tag(tag, "d") {
            self.handle_d_close();
        } else if tag.starts_with("<a ") {
            self.handle_link_open(tag);
        } else if Self::is_close_tag(tag, "a") {
            self.handle_link_close();
        } else if tag.starts_with("<menu ") {
            self.handle_menu_open(tag);
        } else if tag == "</menu>" {
            self.handle_menu_close(elements);
        } else if tag.starts_with("<mi ") {
            self.handle_menu_item(tag);
        } else if tag.starts_with("<LaunchURL ") {
            self.handle_launch_url(tag, elements);
        } else if tag.starts_with("<LichWebUI ") || tag.starts_with("<LichWebUI/") {
            self.handle_lich_webui(tag, elements);
        } else if tag.starts_with("<output ") || tag.starts_with("<output/") {
            self.handle_output(tag);
        }
        // Handle paired inv tags: <inv id='X'>content</inv>
        else if tag.starts_with("<inv ") && tag.contains("</inv>") {
            self.handle_inv_paired(tag, elements);
        }
        // Handle container tags
        else if tag.starts_with("<container ") {
            self.handle_container(tag, elements);
            self.emit_window_hints(tag, elements);
        } else if tag.starts_with("<clearContainer ") {
            self.handle_clear_container(tag, elements);
        }
        // Handle dropDownBox for target list
        else if tag.starts_with("<dropDownBox ") {
            tracing::debug!("Parser: Matched dropDownBox tag: {}", &tag[..tag.len().min(100)]);
            self.handle_dropdown(tag, elements);
        }
        // Creature status snapshot (usually embedded in room objs components,
        // which are captured whole; this arm catches any sent standalone)
        else if tag.starts_with("<crtrStatus ") {
            self.handle_crtr_status(tag, elements);
        }
        else if tag.starts_with("<roommeta ") {
            self.handle_roommeta(tag, elements);
        }
        // Debug: catch any dropdown-related tags we might be missing
        // (case-sensitive checks - avoids a per-tag to_lowercase allocation)
        else if tag.contains("dropDown") || tag.contains("dropdown") || tag.contains("dDB") {
            tracing::warn!("Parser: Unhandled dropdown-like tag: {}", &tag[..tag.len().min(100)]);
        }
        // Silently ignore these tags
        else if tag.starts_with("<compDef ")
            || tag == "</compDef>"
            || tag.starts_with("<streamWindow ")
            || tag.starts_with("<skin ")
        {
            // Ignore these (UI layout tags)
        }
    }

}

impl Default for XmlParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a parser with common presets for testing
    fn test_parser() -> XmlParser {
        let presets = vec![
            ("speech".to_string(), Some("#53a684".to_string()), None),
            ("links".to_string(), Some("#477ab3".to_string()), None),
            ("commands".to_string(), Some("#477ab3".to_string()), None),
            ("monsterbold".to_string(), Some("#a29900".to_string()), None),
            ("roomName".to_string(), Some("#9BA2B2".to_string()), Some("#395573".to_string())),
        ];
        XmlParser::with_presets(presets, std::collections::HashMap::new())
    }

    // ==================== Dialog dropdowns (P3a) ====================

    #[test]
    fn parses_combat_dropdowns_with_options_anchors_and_buttons() {
        // Real shapes from a 2026-07-28 session log: one dialogData chunk
        // carrying both cmdButtons and dropDownBoxes.
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<dialogData id='combat'>\
             <cmdButton id='cmdDefStance' value='defense' cmd='_stance defensive' tooltip='Assume a Defensive Stance' echo='stance defensive' height='20' width='55' top='70' left='0' align='nw'/>\
             <dropDownBox id='dDBStance' value=\"defensive\" cmd='_stance %dDBStance%' content_text='offensive,advance,forward,neutral,guarded,defensive' content_value='offensive,advance,forward,neutral,guarded,defensive' align='n' top='70' left='0' anchor_left='cmdDefStance' anchor_right='cmdOffStance' height='20' width='80' tooltip='Stance Selection'/>\
             <dropDownBox id='dDBCman0' value=\"none\" cmd=\"_cmbtpl ddbcman 0 %dDBCman0%\" content_text=\"none,Combat Movement\" content_value=\"usage,cmovement\" align='ne' anchor_left='cmdCman0' anchor_right='imgSpacer' top='208' left='0' height='20' width='80' tooltip='Maneuver Selection'/>\
             </dialogData>",
        );

        // Both dropdowns captured, alongside the button element.
        let dropdowns: Vec<_> = elements
            .iter()
            .filter_map(|e| {
                if let ParsedElement::DialogDropDowns { id, dropdowns, .. } = e {
                    Some((id, dropdowns))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(dropdowns.len(), 1);
        let (id, boxes) = &dropdowns[0];
        assert_eq!(*id, "combat");
        assert_eq!(boxes.len(), 2);

        let stance = &boxes[0];
        assert_eq!(stance.id, "dDBStance");
        assert_eq!(stance.value, "defensive");
        assert_eq!(stance.command, "_stance %dDBStance%");
        assert_eq!(stance.options.len(), 6);
        assert_eq!(stance.options[0], ("offensive".to_string(), "offensive".to_string()));
        let layout = stance.layout.as_ref().expect("layout captured");
        assert_eq!(layout.top, Some(70));
        assert_eq!(layout.anchor_left.as_deref(), Some("cmdDefStance"));
        assert_eq!(layout.anchor_right.as_deref(), Some("cmdOffStance"));

        // content_text/content_value pair by position (display, submit).
        let cman = &boxes[1];
        assert_eq!(cman.options[1], ("Combat Movement".to_string(), "cmovement".to_string()));

        // Buttons still parse from the same chunk, now with layout.
        let buttons = elements.iter().find_map(|e| {
            if let ParsedElement::DialogButtons { buttons, .. } = e {
                Some(buttons)
            } else {
                None
            }
        });
        let buttons = buttons.expect("buttons emitted alongside dropdowns");
        assert_eq!(buttons[0].id, "cmdDefStance");
        let layout = buttons[0].layout.as_ref().expect("button layout captured");
        assert_eq!(layout.top, Some(70));
        assert_eq!(layout.align.as_deref(), Some("nw"));
    }

    #[test]
    fn negative_pixel_offsets_parse() {
        // Real: <image ... left='-50'> — layout offsets can be negative.
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<dialogData id='x'><dropDownBox id='d' value='a' cmd='c %d%' content_text='a' content_value='a' top='3' left='-50'/></dialogData>",
        );
        let layout = elements
            .iter()
            .find_map(|e| {
                if let ParsedElement::DialogDropDowns { dropdowns, .. } = e {
                    dropdowns[0].layout.clone()
                } else {
                    None
                }
            })
            .expect("layout");
        assert_eq!(layout.left, Some(-50));
    }

    // ==================== Entity Decoding ====================

    #[test]
    fn test_decode_entities_basic() {
        assert_eq!(XmlParser::decode_entities("a &lt;b&gt; &amp; &quot;c&quot; &apos;d&apos;".to_string()), "a <b> & \"c\" 'd'");
    }

    #[test]
    fn test_decode_entities_no_entities_passthrough() {
        assert_eq!(XmlParser::decode_entities("plain game text".to_string()), "plain game text");
    }

    #[test]
    fn test_decode_entities_double_encoded() {
        // &amp;lt; decodes the &amp; only, yielding a literal &lt; - the
        // product of one decode must not be re-decoded (matches the old
        // chained-replace behavior)
        assert_eq!(XmlParser::decode_entities("&amp;lt;".to_string()), "&lt;");
        assert_eq!(XmlParser::decode_entities("&amp;gt;".to_string()), "&gt;");
    }

    #[test]
    fn test_decode_entities_unknown_and_trailing() {
        assert_eq!(XmlParser::decode_entities("&foo; stays".to_string()), "&foo; stays");
        assert_eq!(XmlParser::decode_entities("trailing &".to_string()), "trailing &");
        assert_eq!(XmlParser::decode_entities("&&lt;".to_string()), "&<");
    }

    // ==================== Basic Text Parsing ====================

    #[test]
    fn test_plain_text_no_tags() {
        let mut parser = test_parser();
        let elements = parser.parse_line("Hello, world!");

        assert_eq!(elements.len(), 1);
        let ParsedElement::Text { content, span_type, .. } = &elements[0] else {
            panic!("Expected Text element, got {:?}", &elements[0]);
        };
        assert_eq!(content, "Hello, world!");
        assert_eq!(*span_type, SpanType::Normal);
    }

    #[test]
    fn test_empty_line_preserved_as_blank_text() {
        let mut parser = test_parser();
        let elements = parser.parse_line("");

        assert_eq!(elements.len(), 1);
        let ParsedElement::Text { content, span_type, .. } = &elements[0] else {
            panic!("Expected Text element for blank line, got {:?}", &elements[0]);
        };
        assert_eq!(content, "");
        assert_eq!(*span_type, SpanType::Normal);
    }

    #[test]
    fn test_text_with_html_entities() {
        let mut parser = test_parser();
        let elements = parser.parse_line("&lt;test&gt; &amp; &quot;quoted&quot;");

        assert_eq!(elements.len(), 1);
        let ParsedElement::Text { content, .. } = &elements[0] else {
            panic!("Expected Text element, got {:?}", &elements[0]);
        };
        assert_eq!(content, "<test> & \"quoted\"");
    }

    // ==================== Preset Tag Parsing ====================

    #[test]
    fn test_preset_speech_applies_color() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<preset id='speech'>Someone says, \"Hello\"</preset>");

        // Should have one text element with speech color
        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 1);

        let ParsedElement::Text { content, fg_color, span_type, .. } = text_elements[0] else {
            panic!("Expected Text element, got {:?}", text_elements[0]);
        };
        assert_eq!(content, "Someone says, \"Hello\"");
        assert_eq!(fg_color.as_deref(), Some("#53a684"));
        assert_eq!(*span_type, SpanType::Speech);
    }

    // ==================== Color Tag Parsing ====================

    #[test]
    fn test_explicit_color_tag() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<color fg='#FF0000'>Red text</color>");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 1);

        let ParsedElement::Text { content, fg_color, .. } = text_elements[0] else {
            panic!("Expected Text element, got {:?}", text_elements[0]);
        };
        assert_eq!(content, "Red text");
        assert_eq!(fg_color.as_deref(), Some("#FF0000"));
    }

    #[test]
    fn test_color_tag_with_background() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<color fg='#FFFFFF' bg='#0000FF'>White on blue</color>");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 1);

        let ParsedElement::Text { content, fg_color, bg_color, .. } = text_elements[0] else {
            panic!("Expected Text element, got {:?}", text_elements[0]);
        };
        assert_eq!(content, "White on blue");
        assert_eq!(fg_color.as_deref(), Some("#FFFFFF"));
        assert_eq!(bg_color.as_deref(), Some("#0000FF"));
    }

    // ==================== Bold Tag Parsing ====================

    #[test]
    fn test_pushbold_popbold() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<pushBold/>A goblin<popBold/> attacks!");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 2);

        // First text should be bold (monsterbold)
        let ParsedElement::Text { content, bold, span_type, .. } = text_elements[0] else {
            panic!("Expected Text element, got {:?}", text_elements[0]);
        };
        assert_eq!(content, "A goblin");
        assert!(*bold);
        assert_eq!(*span_type, SpanType::Monsterbold);

        // Second text should not be bold
        let ParsedElement::Text { content, bold, span_type, .. } = text_elements[1] else {
            panic!("Expected Text element, got {:?}", text_elements[1]);
        };
        assert_eq!(content, " attacks!");
        assert!(!*bold);
        assert_eq!(*span_type, SpanType::Normal);
    }

    #[test]
    fn daydream_split_bold_does_not_leak_color_to_later_lines() {
        // Real game traffic when daydreaming: pushBold and popBold arrive on
        // SEPARATE lines with a blank line between them, and popBold shares a
        // line with an <output> tag. The monsterbold preset must be fully
        // popped so the prompt and everything after render normally.
        let mut parser = test_parser();

        parser.parse_line("<pushBold/>You continue to daydream...");
        parser.parse_line(""); // blank line between push and pop
        parser.parse_line(r#"<popBold/><output class="mono"/>"#);
        parser.parse_line(r#"<output class=""/>"#);

        // A later, unrelated line (the next prompt / normal output) must not
        // carry the monsterbold color.
        let elements = parser.parse_line("Vonnorik asks, \"How is ya?\"");
        let ParsedElement::Text { span_type, bold, fg_color, .. } = &elements[0] else {
            panic!("Expected Text element, got {:?}", elements[0]);
        };
        assert!(!*bold, "bold must be cleared after popBold");
        assert_eq!(*span_type, SpanType::Normal, "must not still be monsterbold");
        assert!(fg_color.is_none(), "monsterbold color leaked to a later line: {fg_color:?}");
    }

    #[test]
    fn real_daydream_block_two_rounds_no_leak() {
        // Verbatim traffic from a real capture (2026-08-03): two consecutive
        // daydream rounds, each `<pushBold/>` … blank … `<popBold/>` split
        // across lines with the popBold sharing its line with an <output> tag.
        // The status/prompt lines after the block must render normally.
        let mut parser = test_parser();
        let lines = [
            r#"<prompt time="1785795872">s&gt;</prompt>"#,
            "<pushBold/>You continue to daydream...",
            "",
            r#"<popBold/><output class="mono"/>"#,
            "          Level: 63                          Fame: 11,956,606",
            r#"<output class=""/>"#,
            "",
            "Your mind is numbed.",
            "",
            "You have been experiencing the Wisdom of the Ages for 2 months.",
            r#"<prompt time="1785795874">s&gt;</prompt>"#,
            "<pushBold/>You continue to daydream...",
            "",
            r#"<popBold/><output class="mono"/>"#,
            "Health: 264/<pushBold/>264<popBold/>     Mana: 70/<pushBold/>70<popBold/>",
            r#"<output class=""/>"#,
            r#"<prompt time="1785795874">s&gt;</prompt>"#,
        ];
        let mut all = Vec::new();
        for l in lines {
            all.extend(parser.parse_line(l));
        }

        // "Your mind is numbed." sits between the two daydream rounds, right
        // after the first block closes — the most sensitive spot for a leak.
        let numbed = all.iter().find_map(|e| match e {
            ParsedElement::Text { content, span_type, fg_color, .. }
                if content.contains("numbed") =>
            {
                Some((*span_type, fg_color.clone()))
            }
            _ => None,
        });
        let (span_type, fg_color) = numbed.expect("'numbed' line should be present");
        assert_eq!(span_type, SpanType::Normal, "monsterbold leaked onto post-daydream text");
        assert!(fg_color.is_none(), "color leaked onto post-daydream text: {fg_color:?}");

        // And after the whole block, the parser's stacks must be clean.
        assert!(parser.bold_stack.is_empty(), "bold_stack left dirty after daydream block");
        assert!(parser.preset_stack.is_empty(), "preset_stack left dirty after daydream block");
    }

    #[test]
    fn link_opening_outside_bold_closing_inside_bold_does_not_leak_color() {
        // A link that opens outside bold (pushes the links color) but closes
        // INSIDE bold must still pop its color. The old code keyed the pop on
        // the bold state at close time, so this leaked the links color onto
        // every following line.
        let mut parser = test_parser();
        // <a ...> opens (pushes links color), then bold starts, then </a>
        // closes inside bold, then bold ends. Text after must be uncolored.
        parser.parse_line(
            "<a exist=\"1\" noun=\"cat\">a cat<pushBold/> pauncing</a><popBold/> and it leaves.",
        );
        let elements = parser.parse_line("Plain following line.");
        let ParsedElement::Text { fg_color, span_type, .. } = &elements[0] else {
            panic!("expected Text, got {:?}", elements[0]);
        };
        assert!(fg_color.is_none(), "links color leaked to a later line: {fg_color:?}");
        assert_eq!(*span_type, SpanType::Normal);
        assert!(parser.color_stack.is_empty(), "color_stack left dirty");
    }

    #[test]
    fn attribute_values_are_entity_decoded() {
        // A link noun carrying &apos; must decode to a real apostrophe so menu
        // requests / outbound <d cmd> commands use the real character, not the
        // literal entity.
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<a exist=\"5\" noun=\"orc&apos;s helm\">the helm</a>",
        );
        let link = elements.iter().find_map(|e| match e {
            ParsedElement::Text { link_data: Some(l), .. } => Some(l.clone()),
            _ => None,
        });
        let link = link.expect("link present");
        assert_eq!(link.noun, "orc's helm", "noun should be entity-decoded");
    }

    #[test]
    fn orphaned_bold_does_not_leak_past_prompt() {
        // If the game leaves a pushBold open (no matching popBold) — the real
        // failure mode behind the daydream color leak — the monsterbold color
        // must not bleed through the next prompt into all subsequent output.
        // The prompt is the game's fresh-round boundary; transient bold/color
        // from the previous round is stale and must be dropped there.
        let mut parser = test_parser();

        parser.parse_line("<pushBold/>You continue to daydream...");
        // No popBold arrives (dropped/mangled by the server). Next comes the
        // prompt for the following round.
        parser.parse_line(r#"<prompt time="1785795874">s&gt;</prompt>"#);
        let elements = parser.parse_line("Vonnorik asks, \"How is ya?\"");

        let ParsedElement::Text { span_type, bold, fg_color, .. } = &elements[0] else {
            panic!("Expected Text element, got {:?}", elements[0]);
        };
        assert!(!*bold, "orphaned bold leaked past the prompt");
        assert_eq!(*span_type, SpanType::Normal, "monsterbold leaked past the prompt");
        assert!(fg_color.is_none(), "color leaked past the prompt: {fg_color:?}");
    }

    #[test]
    fn daydream_full_traffic_does_not_leak_color() {
        // Fuller repro matching the actual mid-turn traffic, including the
        // surrounding prompts and the second output-close/prompt tail. The
        // pushBold has NO matching popBold on its own line; the popBold
        // arrives two lines later sharing a line with <output class="mono"/>.
        let mut parser = test_parser();

        parser.parse_line(r#"<prompt time="1785795874">s&gt;</prompt>"#);
        parser.parse_line("<pushBold/>You continue to daydream...");
        parser.parse_line("");
        parser.parse_line(r#"<popBold/><output class="mono"/>"#);
        parser.parse_line("Health: 264/<pushBold/>264<popBold/>     Mana: 70/<pushBold/>70<popBold/>");
        parser.parse_line(r#"<output class=""/>"#);
        let elements = parser.parse_line(r#"<prompt time="1785795874">s&gt;</prompt>"#);

        // The prompt after the whole block must be normal, not monsterbold.
        for el in &elements {
            if let ParsedElement::Text { span_type, fg_color, content, .. } = el {
                assert_ne!(
                    *span_type,
                    SpanType::Monsterbold,
                    "monsterbold leaked onto prompt text {content:?}"
                );
                assert!(
                    fg_color.is_none(),
                    "color leaked onto prompt text {content:?}: {fg_color:?}"
                );
            }
        }
    }

    #[test]
    fn test_output_mono_region_marks_text() {
        let mut parser = test_parser();

        // <output class="mono"/> opens a monospace region; text lines that
        // follow are stamped mono until <output class=""/> closes it.
        parser.parse_line(r#"<output class="mono"/>"#);
        let elements = parser.parse_line("| Script/File      | Author          |");
        let ParsedElement::Text { content, mono, .. } = &elements[0] else {
            panic!("Expected Text element, got {:?}", elements[0]);
        };
        assert_eq!(content, "| Script/File      | Author          |");
        assert!(*mono, "text inside a mono output region must be mono");

        parser.parse_line(r#"<output class=""/>"#);
        let elements = parser.parse_line("You are standing in a field.");
        let ParsedElement::Text { mono, .. } = &elements[0] else {
            panic!("Expected Text element, got {:?}", elements[0]);
        };
        assert!(!*mono, "text after the region closes must not be mono");
    }

    // ==================== GemStone IV Link Parsing (<a> tags) ====================

    #[test]
    fn test_a_tag_link_with_exist_noun() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<a exist='12345' noun='sword'>a rusty sword</a>");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 1);

        let ParsedElement::Text { content, span_type, link_data, .. } = text_elements[0] else {
            panic!("Expected Text element, got {:?}", text_elements[0]);
        };
        assert_eq!(content, "a rusty sword");
        assert_eq!(*span_type, SpanType::Link);

        let link = link_data.as_ref().expect("Should have link_data");
        assert_eq!(link.exist_id, "12345");
        assert_eq!(link.noun, "sword");
        assert_eq!(link.text, "a rusty sword");
    }

    #[test]
    fn mangled_close_tag_does_not_bleed_link_color() {
        // Real game data (weapon HELP radialsweep, 2026-08): broken $-escaping
        // ships `$<a href=$Q...$>Recent Evasion$</a$>`. The `</a$>` close must
        // still pop the link style, or everything after renders link-colored.
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "Reaction: Requires attacker to have a $<a href=$Qhttps://gswiki.play.net/Recent_Evasion$Q$>Recent Evasion$</a$>.  Reaction triggers are removed.",
        );
        let texts: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::Text { content, span_type, .. } => Some((content.as_str(), *span_type)),
                _ => None,
            })
            .collect();
        let trailing = texts
            .iter()
            .find(|(content, _)| content.contains("Reaction triggers"))
            .expect("trailing text present");
        assert_eq!(
            trailing.1,
            SpanType::Normal,
            "text after the mangled </a$> must not stay link-styled"
        );
        // And the anchor content itself still styles as a link.
        let inner = texts
            .iter()
            .find(|(content, _)| content.contains("Recent Evasion"))
            .expect("anchor text present");
        assert_eq!(inner.1, SpanType::Link);
    }

    #[test]
    fn href_links_carry_the_url_sentinel() {
        // Game HELP text ships wiki anchors; they must be clickable to open
        // the page, not just styled.
        let mut parser = test_parser();
        let elements = parser.parse_line(
            r#"Name: <a href="https://gswiki.play.net/Radial_Sweep">Radial Sweep</a> [radialsweep]"#,
        );
        let link = elements
            .iter()
            .find_map(|e| match e {
                ParsedElement::Text { link_data: Some(link), .. } => Some(link),
                _ => None,
            })
            .expect("href anchor produces link data");
        assert_eq!(link.exist_id, crate::data::URL_LINK_SENTINEL);
        assert_eq!(link.noun, "https://gswiki.play.net/Radial_Sweep");
        assert_eq!(link.text, "Radial Sweep");
    }

    #[test]
    fn non_http_hrefs_stay_styled_but_inert() {
        let mut parser = test_parser();
        let elements =
            parser.parse_line(r#"<a href="javascript:alert(1)">totally safe</a> text"#);
        let anchor = elements
            .iter()
            .find_map(|e| match e {
                ParsedElement::Text { content, span_type, link_data, .. }
                    if content.contains("totally safe") =>
                {
                    Some((*span_type, link_data.clone()))
                }
                _ => None,
            })
            .expect("anchor text present");
        assert_eq!(anchor.0, SpanType::Link, "still styled as a link");
        assert!(anchor.1.is_none(), "but carries no activation data");
    }

    #[test]
    fn close_tag_tolerance_never_matches_longer_tag_names() {
        // `</app>` and other real tags starting with 'a'/'d' must not be
        // mistaken for anchor/command closes.
        assert!(XmlParser::is_close_tag("</a>", "a"));
        assert!(XmlParser::is_close_tag("</a$>", "a"));
        assert!(XmlParser::is_close_tag("</d>", "d"));
        assert!(XmlParser::is_close_tag("</d$>", "d"));
        assert!(!XmlParser::is_close_tag("</app>", "a"));
        assert!(!XmlParser::is_close_tag("</dialogData>", "d"));
        assert!(!XmlParser::is_close_tag("<a>", "a"));
    }

    #[test]
    fn test_a_tag_with_coord() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<a exist='67890' noun='chest' coord='1234,5678'>an iron chest</a>");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 1);

        let ParsedElement::Text { link_data, .. } = text_elements[0] else {
            panic!("Expected Text element, got {:?}", text_elements[0]);
        };
        let link = link_data.as_ref().expect("Should have link_data");
        assert_eq!(link.coord.as_deref(), Some("1234,5678"));
    }

    // ==================== DragonRealms Link Parsing (<d> tags) ====================

    #[test]
    fn test_d_cmd_tag_direct_command() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<d cmd='get #123'>Some item</d>");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 1);

        let ParsedElement::Text { content, span_type, link_data, .. } = text_elements[0] else {
            panic!("Expected Text element, got {:?}", text_elements[0]);
        };
        assert_eq!(content, "Some item");
        assert_eq!(*span_type, SpanType::Link);

        let link = link_data.as_ref().expect("Should have link_data for <d> tag");
        assert_eq!(link.exist_id, "_direct_");
        assert_eq!(link.noun, "get #123");
    }

    #[test]
    fn test_d_cmd_tag_with_complex_command() {
        let mut parser = test_parser();
        // This is the exact format from DragonRealms inventory search
        let elements = parser.parse_line("<d cmd='get #8735861 in #8735860 in watery portal'>Some arzumodine cloth</d> is in a lumpy canvas sack.");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 2); // Link text + rest of line

        // First element should be the link
        let ParsedElement::Text { content, span_type, link_data, .. } = text_elements[0] else {
            panic!("Expected Text element for link, got {:?}", text_elements[0]);
        };
        assert_eq!(content, "Some arzumodine cloth");
        assert_eq!(*span_type, SpanType::Link);

        let link = link_data.as_ref().expect("Should have link_data");
        assert_eq!(link.exist_id, "_direct_");
        assert_eq!(link.noun, "get #8735861 in #8735860 in watery portal");

        // Second element should be normal text
        let ParsedElement::Text { content, span_type, link_data, .. } = text_elements[1] else {
            panic!("Expected Text element for trailing text, got {:?}", text_elements[1]);
        };
        assert_eq!(content, " is in a lumpy canvas sack.");
        assert_eq!(*span_type, SpanType::Normal);
        assert!(link_data.is_none());
    }

    #[test]
    fn test_d_tag_without_cmd_uses_text() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<d>SKILLS BASE</d>");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 1);

        let ParsedElement::Text { content, span_type, link_data, .. } = text_elements[0] else {
            panic!("Expected Text element, got {:?}", text_elements[0]);
        };
        assert_eq!(content, "SKILLS BASE");
        assert_eq!(*span_type, SpanType::Link);

        let link = link_data.as_ref().expect("Should have link_data");
        assert_eq!(link.exist_id, "_direct_");
        // NOTE: In current implementation, noun is empty when cmd is not specified
        // because link_data is cloned to ParsedElement before </d> close updates it.
        // The text content is stored in link.text instead.
        assert_eq!(link.noun, "");
        assert_eq!(link.text, "SKILLS BASE");
    }

    #[test]
    fn test_nested_d_and_a_links_outer_command_wins() {
        // From `store list`: a <d> command link wrapping an item <a> link.
        // The <d> is the actionable link but carries no clickable text of its
        // own; the inner <a> supplies the visible text. In Wrayth, clicking
        // that text fires the OUTER <d> command (store SHEATH clear), not the
        // item's <a> menu — so every span here, item text included, must
        // carry the enclosing <d> command. A single-slot model let the inner
        // <a> clobber the outer <d>; the link-data stack + outermost-wins
        // mirror fixes it.
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<d cmd=\"store SHEATH clear\">a <a exist=\"18540109\" noun=\"bandolier\">quilled iron boar hide bandolier</a> shrouded by impaled leaves</d>",
        );

        let text: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::Text { content, link_data, span_type, .. } => {
                    Some((content.as_str(), link_data.clone(), *span_type))
                }
                _ => None,
            })
            .collect();

        // Every span (leading "a ", the bandolier item text, and the trailing
        // "shrouded…") is a Link carrying the outer <d> store command.
        assert!(!text.is_empty(), "expected link spans");
        for (content, link, span_type) in &text {
            assert_eq!(*span_type, SpanType::Link, "span {content:?} not a link");
            let link = link
                .as_ref()
                .unwrap_or_else(|| panic!("span {content:?} lost its link_data"));
            assert_eq!(
                link.exist_id, "_direct_",
                "span {content:?} should carry the <d> command, not the <a> item"
            );
            assert_eq!(
                link.noun, "store SHEATH clear",
                "span {content:?} should run the store command"
            );
        }

        // Sanity: the visible item text is present as one of the spans.
        assert!(
            text.iter()
                .any(|(c, _, _)| *c == "quilled iron boar hide bandolier"),
            "item text span missing: {text:?}"
        );
    }

    // ==================== Prompt Parsing ====================

    #[test]
    fn test_prompt_parsing() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<prompt time='1234567890'>&gt;</prompt>");

        let prompt_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Prompt { .. })).collect();
        assert_eq!(prompt_elements.len(), 1);

        let ParsedElement::Prompt { time, text } = prompt_elements[0] else {
            panic!("Expected Prompt element, got {:?}", prompt_elements[0]);
        };
        assert_eq!(time, "1234567890");
        assert_eq!(text, ">");
    }

    // ==================== RoundTime Parsing ====================

    #[test]
    fn test_roundtime_parsing() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<roundTime value='1764904999'/>");

        let rt_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::RoundTime { .. })).collect();
        assert_eq!(rt_elements.len(), 1);

        let ParsedElement::RoundTime { value } = rt_elements[0] else {
            panic!("Expected RoundTime element, got {:?}", rt_elements[0]);
        };
        assert_eq!(*value, 1764904999);
    }

    // ==================== VellumTimer Parsing ====================

    #[test]
    fn test_vellum_timer_parsing() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<vellumTimer id='dark-cataclyst' value='1764904999'/>");

        let timers: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::VellumTimer { .. }))
            .collect();
        assert_eq!(timers.len(), 1);
        let ParsedElement::VellumTimer { id, value } = timers[0] else {
            panic!("Expected VellumTimer element, got {:?}", timers[0]);
        };
        assert_eq!(id, "dark-cataclyst");
        assert_eq!(*value, 1764904999);

        // Clear form
        let elements = parser.parse_line("<vellumTimer id='dark-cataclyst' value='0'/>");
        assert!(elements
            .iter()
            .any(|e| matches!(e, ParsedElement::VellumTimer { value: 0, .. })));
    }

    #[test]
    fn test_vellum_timer_malformed_ignored() {
        let mut parser = test_parser();
        // Missing value, missing id, empty id, junk value: no element, no text.
        for line in [
            "<vellumTimer id='x'/>",
            "<vellumTimer value='123'/>",
            "<vellumTimer id='' value='123'/>",
            "<vellumTimer id='x' value='soon'/>",
        ] {
            let elements = parser.parse_line(line);
            assert!(
                !elements
                    .iter()
                    .any(|e| matches!(e, ParsedElement::VellumTimer { .. })),
                "line {:?} should not produce a timer",
                line
            );
        }
    }

    // ==================== VellumCmd Parsing ====================

    #[test]
    fn test_vellum_cmd_parsing() {
        let mut parser = test_parser();
        // Both spellings, self-closing; command carries spaces.
        for line in [
            "<vellumCmd cmd='.rightbar off'/>",
            "<vellum-cmd cmd='.rightbar off'/>",
        ] {
            let elements = parser.parse_line(line);
            let commands: Vec<_> = elements
                .iter()
                .filter(|e| matches!(e, ParsedElement::VellumCommand { .. }))
                .collect();
            assert_eq!(commands.len(), 1, "line {:?}", line);
            let ParsedElement::VellumCommand { command } = commands[0] else {
                panic!("Expected VellumCommand element, got {:?}", commands[0]);
            };
            assert_eq!(command, ".rightbar off");
        }
    }

    #[test]
    fn test_vellum_cmd_malformed_ignored() {
        let mut parser = test_parser();
        // Missing/empty cmd: no element, no text.
        for line in ["<vellumCmd/>", "<vellumCmd cmd=''/>", "<vellumCmd cmd='  '/>"] {
            let elements = parser.parse_line(line);
            assert!(
                !elements
                    .iter()
                    .any(|e| matches!(e, ParsedElement::VellumCommand { .. })),
                "line {:?} should not produce a command",
                line
            );
        }
    }

    // ==================== Stream Parsing ====================

    #[test]
    fn test_push_stream() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<pushStream id='inv'/>");

        let stream_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StreamPush { .. })).collect();
        assert_eq!(stream_elements.len(), 1);

        let ParsedElement::StreamPush { id } = stream_elements[0] else {
            panic!("Expected StreamPush element, got {:?}", stream_elements[0]);
        };
        assert_eq!(id, "inv");
    }

    #[test]
    fn test_pop_stream() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<popStream/>");

        assert!(elements.iter().any(|e| matches!(e, ParsedElement::StreamPop)));
    }

    // ==================== Compass Parsing ====================

    #[test]
    fn test_compass_directions() {
        let mut parser = test_parser();
        // Note: The regex uses double quotes for dir value matching
        let elements = parser.parse_line("<compass><dir value=\"n\"/><dir value=\"e\"/><dir value=\"out\"/></compass>");

        let compass_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Compass { .. })).collect();
        assert_eq!(compass_elements.len(), 1);

        let ParsedElement::Compass { directions } = compass_elements[0] else {
            panic!("Expected Compass element, got {:?}", compass_elements[0]);
        };
        assert_eq!(directions.len(), 3);
        assert!(directions.contains(&"n".to_string()));
        assert!(directions.contains(&"e".to_string()));
        assert!(directions.contains(&"out".to_string()));
    }

    // ==================== GSL Tag Filtering ====================

    #[test]
    fn test_gsl_compass_tag_filtered() {
        // GSL compass tags from Lich should be filtered out entirely (no blank line)
        let mut parser = test_parser();
        let elements = parser.parse_line("GSjBCDFGH");

        // Should produce completely empty result - no elements at all
        assert!(elements.is_empty(), "GSL tag should produce no elements (got {:?})", elements);
    }

    #[test]
    fn test_gsl_stance_tag_filtered() {
        // GSL stance tags should be filtered (no blank line)
        let mut parser = test_parser();
        let elements = parser.parse_line("GSg0000000050");

        // Should produce completely empty result - no elements at all
        assert!(elements.is_empty(), "GSL stance tag should produce no elements (got {:?})", elements);
    }

    #[test]
    fn test_normal_text_not_filtered() {
        // Normal text starting with "GS" but not a GSL tag should pass through
        let mut parser = test_parser();
        let elements = parser.parse_line("GSW is awesome");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 1);

        let ParsedElement::Text { content, .. } = text_elements[0] else {
            panic!("Expected Text element");
        };
        assert_eq!(content, "GSW is awesome");
    }

    // ==================== Complex Scenarios ====================

    #[test]
    fn test_mixed_text_and_links() {
        let mut parser = test_parser();
        let elements = parser.parse_line("You see <a exist='1' noun='goblin'>a goblin</a> and <a exist='2' noun='orc'>an orc</a> here.");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        // 5 text elements: "You see ", "a goblin", " and ", "an orc", " here."
        assert_eq!(text_elements.len(), 5);

        // Verify exactly 2 links exist with correct data
        let links: Vec<_> = text_elements.iter().filter(|e| {
            if let ParsedElement::Text { link_data, .. } = e {
                link_data.is_some()
            } else {
                false
            }
        }).collect();
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_nested_color_and_link() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<color fg='#FF0000'><a exist='123' noun='item'>glowing item</a></color>");

        let text_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Text { .. })).collect();
        assert_eq!(text_elements.len(), 1);

        let ParsedElement::Text { content, fg_color, span_type, link_data, .. } = text_elements[0] else {
            panic!("Expected Text element, got {:?}", text_elements[0]);
        };
        assert_eq!(content, "glowing item");
        // Link should still work inside color
        assert_eq!(*span_type, SpanType::Link);
        assert!(link_data.is_some());
        // NOTE: The <a> tag pushes the "links" preset color on top of the color stack,
        // so the actual color is the links preset (#477ab3) not the outer color (#FF0000)
        assert_eq!(fg_color.as_deref(), Some("#477ab3"));
    }

    // ==================== Attribute Extraction ====================

    #[test]
    fn test_extract_attribute_double_quotes() {
        let tag = r#"<a exist="12345" noun="sword">"#;
        assert_eq!(XmlParser::extract_attribute(tag, "exist"), Some("12345".to_string()));
        assert_eq!(XmlParser::extract_attribute(tag, "noun"), Some("sword".to_string()));
    }

    #[test]
    fn test_extract_attribute_single_quotes() {
        let tag = "<a exist='12345' noun='sword'>";
        assert_eq!(XmlParser::extract_attribute(tag, "exist"), Some("12345".to_string()));
        assert_eq!(XmlParser::extract_attribute(tag, "noun"), Some("sword".to_string()));
    }

    #[test]
    fn test_extract_attribute_with_special_chars() {
        // DragonRealms style command with # and spaces
        let tag = "<d cmd='get #8735861 in #8735860 in watery portal'>";
        let cmd = XmlParser::extract_attribute(tag, "cmd");
        assert_eq!(cmd, Some("get #8735861 in #8735860 in watery portal".to_string()));
    }

    #[test]
    fn test_extract_attribute_missing() {
        let tag = "<a exist='12345'>";
        assert_eq!(XmlParser::extract_attribute(tag, "noun"), None);
        assert_eq!(XmlParser::extract_attribute(tag, "nonexistent"), None);
    }

    // ==================== Helper Functions ====================

    #[test]
    fn test_first_number_simple() {
        assert_eq!(first_number("123"), Some(123));
        assert_eq!(first_number("health 175"), Some(175));
        assert_eq!(first_number("abc 42 def"), Some(42));
    }

    #[test]
    fn test_first_number_with_delimiters() {
        assert_eq!(first_number("(100%)"), Some(100));
        assert_eq!(first_number("value (50)"), Some(50));
        assert_eq!(first_number("  99  "), Some(99));
    }

    #[test]
    fn test_first_number_no_number() {
        assert_eq!(first_number("no numbers here"), None);
        assert_eq!(first_number(""), None);
        assert_eq!(first_number("   "), None);
    }

    #[test]
    fn test_last_number_simple() {
        assert_eq!(last_number("123"), Some(123));
        assert_eq!(last_number("health 175"), Some(175));
        assert_eq!(last_number("42 def 99"), Some(99));
    }

    #[test]
    fn test_last_number_slash_format() {
        // Note: last_number doesn't split on slash - it handles tokens
        // "175/200" as a single token that can't be parsed
        assert_eq!(last_number("health 175/200"), None);  // Can't parse "175/200"
        assert_eq!(last_number("mana 386"), Some(386));
        assert_eq!(last_number("health 175"), Some(175));  // Without slash works
    }

    #[test]
    fn test_last_number_no_number() {
        assert_eq!(last_number("no numbers"), None);
        assert_eq!(last_number(""), None);
    }

    #[test]
    fn test_parse_progress_numbers_slash_format() {
        // "label current/max" format
        assert_eq!(parse_progress_numbers("health 175/326", 50), (175, 326));
        assert_eq!(parse_progress_numbers("mana 386/407", 94), (386, 407));
        assert_eq!(parse_progress_numbers("stamina 100/100", 100), (100, 100));
    }

    #[test]
    fn test_parse_progress_numbers_no_label() {
        // "current/max" without label
        assert_eq!(parse_progress_numbers("324/326", 99), (324, 326));
        assert_eq!(parse_progress_numbers("0/100", 0), (0, 100));
    }

    #[test]
    fn test_parse_progress_numbers_percent_format() {
        // Percentage format
        assert_eq!(parse_progress_numbers("defensive (100%)", 100), (100, 100));
        assert_eq!(parse_progress_numbers("75%", 75), (75, 100));
        assert_eq!(parse_progress_numbers("(50%)", 50), (50, 100));
    }

    #[test]
    fn test_parse_progress_numbers_label_only() {
        // Label without numbers - fallback to percentage/100
        assert_eq!(parse_progress_numbers("clear as a bell", 0), (0, 100));
        assert_eq!(parse_progress_numbers("focused", 50), (50, 100));
    }

    #[test]
    fn test_parse_progress_numbers_empty() {
        // Empty string
        assert_eq!(parse_progress_numbers("", 75), (75, 100));
        assert_eq!(parse_progress_numbers("   ", 50), (50, 100));
    }

    // ==================== ProgressBar Parsing ====================

    #[test]
    fn test_progressbar_health() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<progressBar id='health' value='100' text='health 175/175' />");

        let pb_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ProgressBar { .. })).collect();
        assert_eq!(pb_elements.len(), 1);

        let ParsedElement::ProgressBar { id, value, max, text } = pb_elements[0] else {
            panic!("Expected ProgressBar element, got {:?}", pb_elements[0]);
        };
        assert_eq!(id, "health");
        assert_eq!(*value, 175);
        assert_eq!(*max, 175);
        assert_eq!(text, "health 175/175");
    }

    #[test]
    fn test_progressbar_mana_partial() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<progressBar id='mana' value='94' text='mana 386/407' />");

        let pb_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ProgressBar { .. })).collect();
        assert_eq!(pb_elements.len(), 1);

        let ParsedElement::ProgressBar { id, value, max, text } = pb_elements[0] else {
            panic!("Expected ProgressBar element, got {:?}", pb_elements[0]);
        };
        assert_eq!(id, "mana");
        assert_eq!(*value, 386);
        assert_eq!(*max, 407);
        assert_eq!(text, "mana 386/407");
    }

    #[test]
    fn test_progressbar_stamina() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<progressBar id='stamina' value='75' text='stamina 75/100' />");

        let pb_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ProgressBar { .. })).collect();
        assert_eq!(pb_elements.len(), 1);

        let ParsedElement::ProgressBar { id, value, max, text } = pb_elements[0] else {
            panic!("Expected ProgressBar element, got {:?}", pb_elements[0]);
        };
        assert_eq!(id, "stamina");
        assert_eq!(*value, 75);
        assert_eq!(*max, 100);
        assert_eq!(text, "stamina 75/100");
    }

    #[test]
    fn test_progressbar_spirit() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<progressBar id='spirit' value='100' text='spirit 100/100' />");

        let pb_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ProgressBar { .. })).collect();
        assert_eq!(pb_elements.len(), 1);

        let ParsedElement::ProgressBar { id, value, max, .. } = pb_elements[0] else {
            panic!("Expected ProgressBar element, got {:?}", pb_elements[0]);
        };
        assert_eq!(id, "spirit");
        assert_eq!(*value, 100);
        assert_eq!(*max, 100);
    }

    #[test]
    fn test_progressbar_mindstate() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<progressBar id='mindState' value='0' text='clear as a bell' />");

        let pb_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ProgressBar { .. })).collect();
        assert_eq!(pb_elements.len(), 1);

        let ParsedElement::ProgressBar { id, value, max, text } = pb_elements[0] else {
            panic!("Expected ProgressBar element, got {:?}", pb_elements[0]);
        };
        assert_eq!(id, "mindState");
        assert_eq!(*value, 0);  // Falls back to percentage
        assert_eq!(*max, 100);
        assert_eq!(text, "clear as a bell");
    }

    #[test]
    fn test_progressbar_concentration() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<progressBar id='concentration' value='100' text='concentration (100%)' />");

        let pb_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ProgressBar { .. })).collect();
        assert_eq!(pb_elements.len(), 1);

        let ParsedElement::ProgressBar { id, value, max, .. } = pb_elements[0] else {
            panic!("Expected ProgressBar element, got {:?}", pb_elements[0]);
        };
        assert_eq!(id, "concentration");
        assert_eq!(*value, 100);
        assert_eq!(*max, 100);
    }

    #[test]
    fn test_progressbar_inside_dialogdata() {
        let mut parser = test_parser();
        // This is the format used in minivitals updates
        let elements = parser.parse_line("<dialogData id='minivitals'><progressBar id='mana' value='100' text='mana 414/414' left='76.7%' top='0%' width='23.3%' height='100%'/></dialogData>");

        let pb_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ProgressBar { .. })).collect();
        // Exactly one: the dialogData handler must not double-parse bars.
        assert_eq!(pb_elements.len(), 1, "Each progressBar should be emitted exactly once");

        let ParsedElement::ProgressBar { id, value, max, .. } = pb_elements[0] else {
            panic!("Expected ProgressBar element, got {:?}", pb_elements[0]);
        };
        assert_eq!(id, "mana");
        assert_eq!(*value, 414);
        assert_eq!(*max, 414);
    }

    #[test]
    fn test_effects_dialogdata_emits_effect_and_duration_bar_once() {
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<dialogData id='Buffs'><progressBar id='115' value='74' text='Fasthr&#39;s Reward' time='03:06:54'/></dialogData>",
        );

        // The effect itself feeds the active-effects windows...
        let effects: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::ActiveEffect { .. }))
            .collect();
        assert_eq!(effects.len(), 1, "Should emit exactly one ActiveEffect");

        // ...and the duration bar is still published once for user-configured
        // progress widgets keyed on the spell id.
        let bars: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::ProgressBar { .. }))
            .collect();
        assert_eq!(bars.len(), 1, "Effect duration bar should be emitted exactly once");
        let ParsedElement::ProgressBar { id, .. } = bars[0] else {
            panic!("Expected ProgressBar element");
        };
        assert_eq!(id, "115");
    }

    // ==================== CastTime Parsing ====================

    #[test]
    fn test_casttime_parsing() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<castTime value='3'/>");

        let ct_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::CastTime { .. })).collect();
        assert_eq!(ct_elements.len(), 1);

        let ParsedElement::CastTime { value } = ct_elements[0] else {
            panic!("Expected CastTime element, got {:?}", ct_elements[0]);
        };
        assert_eq!(*value, 3);
    }

    #[test]
    fn test_casttime_long_duration() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<castTime value='10'/>");

        let ct_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::CastTime { .. })).collect();
        assert_eq!(ct_elements.len(), 1);

        let ParsedElement::CastTime { value } = ct_elements[0] else {
            panic!("Expected CastTime element, got {:?}", ct_elements[0]);
        };
        assert_eq!(*value, 10);
    }

    #[test]
    fn test_casttime_zero() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<castTime value='0'/>");

        let ct_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::CastTime { .. })).collect();
        assert_eq!(ct_elements.len(), 1);

        let ParsedElement::CastTime { value } = ct_elements[0] else {
            panic!("Expected CastTime element, got {:?}", ct_elements[0]);
        };
        assert_eq!(*value, 0);
    }

    // ==================== Hand Item Parsing ====================

    #[test]
    fn test_left_hand_simple() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<left>Empty</left>");

        let hand_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::LeftHand { .. })).collect();
        assert_eq!(hand_elements.len(), 1);

        let ParsedElement::LeftHand { item, link } = hand_elements[0] else {
            panic!("Expected LeftHand element, got {:?}", hand_elements[0]);
        };
        assert_eq!(item, "Empty");
        assert!(link.is_none());
    }

    #[test]
    fn test_left_hand_with_item() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<left exist='12345' noun='sword'>a gleaming steel sword</left>");

        let hand_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::LeftHand { .. })).collect();
        assert_eq!(hand_elements.len(), 1);

        let ParsedElement::LeftHand { item, link } = hand_elements[0] else {
            panic!("Expected LeftHand element, got {:?}", hand_elements[0]);
        };
        assert_eq!(item, "a gleaming steel sword");
        let link_data = link.as_ref().expect("Should have link data");
        assert_eq!(link_data.exist_id, "12345");
        assert_eq!(link_data.noun, "sword");
    }

    #[test]
    fn test_right_hand_simple() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<right>Empty</right>");

        let hand_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::RightHand { .. })).collect();
        assert_eq!(hand_elements.len(), 1);

        let ParsedElement::RightHand { item, link } = hand_elements[0] else {
            panic!("Expected RightHand element, got {:?}", hand_elements[0]);
        };
        assert_eq!(item, "Empty");
        assert!(link.is_none());
    }

    #[test]
    fn test_right_hand_with_item() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<right exist='67890' noun='shield'>an iron-banded shield</right>");

        let hand_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::RightHand { .. })).collect();
        assert_eq!(hand_elements.len(), 1);

        let ParsedElement::RightHand { item, link } = hand_elements[0] else {
            panic!("Expected RightHand element, got {:?}", hand_elements[0]);
        };
        assert_eq!(item, "an iron-banded shield");
        let link_data = link.as_ref().expect("Should have link data");
        assert_eq!(link_data.exist_id, "67890");
        assert_eq!(link_data.noun, "shield");
    }

    #[test]
    fn test_left_hand_with_coord() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<left exist='11111' noun='dagger' coord='1234,5678'>a silver dagger</left>");

        let hand_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::LeftHand { .. })).collect();
        assert_eq!(hand_elements.len(), 1);

        let ParsedElement::LeftHand { item, link } = hand_elements[0] else {
            panic!("Expected LeftHand element, got {:?}", hand_elements[0]);
        };
        assert_eq!(item, "a silver dagger");
        let link_data = link.as_ref().expect("Should have link data");
        assert_eq!(link_data.coord.as_deref(), Some("1234,5678"));
    }

    // ==================== SpellHand Parsing ====================

    #[test]
    fn test_spell_hand_simple() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<spell>Minor Shock (901)</spell>");

        // Should emit both Spell and SpellHand elements
        let spell_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Spell { .. })).collect();
        let spellhand_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::SpellHand { .. })).collect();

        assert_eq!(spell_elements.len(), 1);
        assert_eq!(spellhand_elements.len(), 1);

        let ParsedElement::Spell { text } = spell_elements[0] else {
            panic!("Expected Spell element, got {:?}", spell_elements[0]);
        };
        assert_eq!(text, "Minor Shock (901)");

        let ParsedElement::SpellHand { spell } = spellhand_elements[0] else {
            panic!("Expected SpellHand element, got {:?}", spellhand_elements[0]);
        };
        assert_eq!(spell, "Minor Shock (901)");
    }

    #[test]
    fn test_spell_hand_empty() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<spell></spell>");

        let spellhand_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::SpellHand { .. })).collect();
        assert_eq!(spellhand_elements.len(), 1);

        let ParsedElement::SpellHand { spell: _ } = spellhand_elements[0] else {
            panic!("Expected SpellHand element, got {:?}", spellhand_elements[0]);
        };
    }

    #[test]
    fn test_spell_with_exist_attribute() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<spell exist='99999'>Fire Spirit (111)</spell>");

        let spell_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Spell { .. })).collect();
        assert_eq!(spell_elements.len(), 1);

        let ParsedElement::Spell { text } = spell_elements[0] else {
            panic!("Expected Spell element, got {:?}", spell_elements[0]);
        };
        assert_eq!(text, "Fire Spirit (111)");
    }

    // ==================== StatusIndicator Parsing ====================

    #[test]
    fn test_indicator_hidden_active() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<indicator id='IconHIDDEN' visible='y'/>");

        let ind_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StatusIndicator { .. })).collect();
        assert_eq!(ind_elements.len(), 1);

        let ParsedElement::StatusIndicator { id, active } = ind_elements[0] else {
            panic!("Expected StatusIndicator element, got {:?}", ind_elements[0]);
        };
        assert_eq!(id, "HIDDEN");  // Icon prefix stripped, casing preserved
        assert!(*active);
    }

    #[test]
    fn test_indicator_stunned_inactive() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<indicator id='IconSTUNNED' visible='n'/>");

        let ind_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StatusIndicator { .. })).collect();
        assert_eq!(ind_elements.len(), 1);

        let ParsedElement::StatusIndicator { id, active } = ind_elements[0] else {
            panic!("Expected StatusIndicator element, got {:?}", ind_elements[0]);
        };
        assert_eq!(id, "STUNNED");
        assert!(!*active);
    }

    #[test]
    fn test_indicator_standing() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<indicator id='IconSTANDING' visible='y'/>");

        let ind_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StatusIndicator { .. })).collect();
        assert_eq!(ind_elements.len(), 1);

        let ParsedElement::StatusIndicator { id, active } = ind_elements[0] else {
            panic!("Expected StatusIndicator element, got {:?}", ind_elements[0]);
        };
        assert_eq!(id, "STANDING");
        assert!(*active);
    }

    #[test]
    fn test_indicator_kneeling() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<indicator id='IconKNEELING' visible='y'/>");

        let ind_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StatusIndicator { .. })).collect();
        assert_eq!(ind_elements.len(), 1);

        let ParsedElement::StatusIndicator { id, active } = ind_elements[0] else {
            panic!("Expected StatusIndicator element, got {:?}", ind_elements[0]);
        };
        assert_eq!(id, "KNEELING");
        assert!(*active);
    }

    #[test]
    fn test_indicator_prone() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<indicator id='IconPRONE' visible='y'/>");

        let ind_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StatusIndicator { .. })).collect();
        assert_eq!(ind_elements.len(), 1);

        let ParsedElement::StatusIndicator { id, active } = ind_elements[0] else {
            panic!("Expected StatusIndicator element, got {:?}", ind_elements[0]);
        };
        assert_eq!(id, "PRONE");
        assert!(*active);
    }

    #[test]
    fn test_dialogdata_status_indicator_poisoned() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='IconPOISONED' value='active'/>");

        let ind_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StatusIndicator { .. })).collect();
        assert_eq!(ind_elements.len(), 1);

        let ParsedElement::StatusIndicator { id, active } = ind_elements[0] else {
            panic!("Expected StatusIndicator element, got {:?}", ind_elements[0]);
        };
        assert_eq!(id, "POISONED");
        assert!(*active);
    }

    #[test]
    fn test_dialogdata_status_indicator_diseased_clear() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='IconDISEASED' value='clear'/>");

        let ind_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StatusIndicator { .. })).collect();
        assert_eq!(ind_elements.len(), 1);

        let ParsedElement::StatusIndicator { id, active } = ind_elements[0] else {
            panic!("Expected StatusIndicator element, got {:?}", ind_elements[0]);
        };
        assert_eq!(id, "DISEASED");
        assert!(!*active);
    }

    #[test]
    fn test_dialogdata_status_indicator_bleeding() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='IconBLEEDING' value='active'/>");

        let ind_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StatusIndicator { .. })).collect();
        assert_eq!(ind_elements.len(), 1);

        let ParsedElement::StatusIndicator { id, active } = ind_elements[0] else {
            panic!("Expected StatusIndicator element, got {:?}", ind_elements[0]);
        };
        assert_eq!(id, "BLEEDING");
        assert!(*active);
    }

    // ==================== InjuryImage Parsing ====================

    #[test]
    fn test_injury_image_head() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='injuries'><image id='head' name='Injury2' /></dialogData>");

        let injury_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::InjuryImage { .. })).collect();
        assert_eq!(injury_elements.len(), 1);

        let ParsedElement::InjuryImage { id: _, name: _ } = injury_elements[0] else {
            panic!("Expected InjuryImage element, got {:?}", injury_elements[0]);
        };
    }

    #[test]
    fn test_injury_image_multiple() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='injuries'><image id='leftArm' name='Injury1' /><image id='chest' name='Injury3' /></dialogData>");

        let injury_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::InjuryImage { .. })).collect();
        assert_eq!(injury_elements.len(), 2);

        // First injury
        let ParsedElement::InjuryImage { id, name } = injury_elements[0] else {
            panic!("Expected InjuryImage element, got {:?}", injury_elements[0]);
        };
        assert_eq!(id, "leftArm");
        assert_eq!(name, "Injury1");

        // Second injury
        let ParsedElement::InjuryImage { id, name } = injury_elements[1] else {
            panic!("Expected InjuryImage element, got {:?}", injury_elements[1]);
        };
        assert_eq!(id, "chest");
        assert_eq!(name, "Injury3");
    }

    #[test]
    fn test_injury_image_scar() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='injuries'><image id='rightLeg' name='Scar1' /></dialogData>");

        let injury_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::InjuryImage { .. })).collect();
        assert_eq!(injury_elements.len(), 1);

        let ParsedElement::InjuryImage { id, name } = injury_elements[0] else {
            panic!("Expected InjuryImage element, got {:?}", injury_elements[0]);
        };
        assert_eq!(id, "rightLeg");
        assert_eq!(name, "Scar1");
    }

    #[test]
    fn test_injuries_clear() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='injuries' clear='t'></dialogData>");

        let injury_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::InjuryImage { .. })).collect();

        // Should emit clear events for all body parts (14 parts)
        assert!(injury_elements.len() >= 14, "Should clear all body parts, got {}", injury_elements.len());

        // Verify body parts are cleared (name == id indicates cleared)
        let cleared_parts: Vec<_> = injury_elements.iter().filter_map(|e| {
            if let ParsedElement::InjuryImage { id, name } = e {
                if id == name {
                    Some(id.clone())
                } else {
                    None
                }
            } else {
                None
            }
        }).collect();

        assert!(cleared_parts.contains(&"head".to_string()));
        assert!(cleared_parts.contains(&"chest".to_string()));
        assert!(cleared_parts.contains(&"leftArm".to_string()));
        assert!(cleared_parts.contains(&"rightArm".to_string()));
    }

    // ==================== Label Parsing ====================

    #[test]
    fn test_label_blood_points() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<label id='lblBPs' value='Blood Points: 100' />");

        // Blood Points label is emitted as ProgressBar for consistency
        let pb_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ProgressBar { .. })).collect();
        assert_eq!(pb_elements.len(), 1);

        let ParsedElement::ProgressBar { id, value, max, text } = pb_elements[0] else {
            panic!("Expected ProgressBar element, got {:?}", pb_elements[0]);
        };
        assert_eq!(id, "lblBPs");
        assert_eq!(*value, 100);
        assert_eq!(*max, 100);
        assert!(text.contains("Blood Points"));
    }

    #[test]
    fn test_label_regular() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<label id='someLabel' value='Some Value' />");

        let label_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Label { .. })).collect();
        assert_eq!(label_elements.len(), 1);

        let ParsedElement::Label { id, value } = label_elements[0] else {
            panic!("Expected Label element, got {:?}", label_elements[0]);
        };
        assert_eq!(id, "someLabel");
        assert_eq!(value, "Some Value");
    }

    #[test]
    fn test_dialogdata_betrayerpanel_labels() {
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<dialogData id='BetrayerPanel'><label id='lblBPs' value='Blood Points: 100'/><label id='lblitem1' value='!a patchwork dwarf skin backpack'/></dialogData>",
        );

        let label_elements: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::DialogLabelList { .. }))
            .collect();
        assert_eq!(label_elements.len(), 1);

        let ParsedElement::DialogLabelList { id, clear, labels } = label_elements[0] else {
            panic!("Expected DialogLabelList element, got {:?}", label_elements[0]);
        };
        assert_eq!(id, "BetrayerPanel");
        assert!(!clear);
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].value, "Blood Points: 100");
        assert_eq!(labels[1].value, "!a patchwork dwarf skin backpack");
    }

    // ==================== crtrStatus Parsing ====================

    #[test]
    fn test_crtr_status_standalone_tag() {
        let mut parser = XmlParser::new();
        let elements =
            parser.parse_line(r#"<crtrStatus exist="607736" hostile="1" stunned="1"/>"#);

        let statuses: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::CreatureStatus { .. }))
            .collect();
        assert_eq!(statuses.len(), 1);
        let ParsedElement::CreatureStatus { id, attrs } = statuses[0] else {
            unreachable!()
        };
        assert_eq!(id, "607736");
        assert_eq!(
            attrs,
            &vec![
                ("hostile".to_string(), "1".to_string()),
                ("stunned".to_string(), "1".to_string()),
            ]
        );

        // The tag must not leak into the text stream
        assert!(!elements
            .iter()
            .any(|e| matches!(e, ParsedElement::Text { content, .. } if !content.trim().is_empty())));
    }

    #[test]
    fn test_crtr_status_inside_component_stays_in_component_value() {
        let mut parser = XmlParser::new();
        let elements = parser.parse_line(
            r#"<component id='room objs'>  You notice<crtrStatus exist="607736" hostile="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b>.</component>"#,
        );

        // Captured whole: the component keeps the raw tag, and no separate
        // CreatureStatus element is emitted (core parses the component value)
        let ParsedElement::Component { id, value } = elements
            .iter()
            .find(|e| matches!(e, ParsedElement::Component { .. }))
            .expect("component element")
        else {
            unreachable!()
        };
        assert_eq!(id, "room objs");
        assert!(value.contains("<crtrStatus exist=\"607736\""));
        assert!(!elements
            .iter()
            .any(|e| matches!(e, ParsedElement::CreatureStatus { .. })));
    }

    #[test]
    fn test_extract_all_attributes_mixed_quotes() {
        let attrs = XmlParser::extract_all_attributes(
            r#"<crtrStatus exist='607736' hostile="1" MiniBoss='0'/>"#,
        );
        assert_eq!(
            attrs,
            vec![
                ("exist".to_string(), "607736".to_string()),
                ("hostile".to_string(), "1".to_string()),
                ("MiniBoss".to_string(), "0".to_string()),
            ]
        );
    }

    // ==================== roommeta / mindState exp Parsing ====================

    #[test]
    fn test_roommeta_standalone_tag() {
        let mut parser = XmlParser::new();
        let elements =
            parser.parse_line(r#"<roommeta climate="3" terrain="7" water="1" sanctuary="0"/>"#);

        let metas: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::RoomMeta { .. }))
            .collect();
        assert_eq!(metas.len(), 1);
        let ParsedElement::RoomMeta { attrs } = metas[0] else {
            unreachable!()
        };
        assert_eq!(
            attrs,
            &vec![
                ("climate".to_string(), "3".to_string()),
                ("terrain".to_string(), "7".to_string()),
                ("water".to_string(), "1".to_string()),
                ("sanctuary".to_string(), "0".to_string()),
            ]
        );

        // The tag must not leak into the text stream
        assert!(!elements
            .iter()
            .any(|e| matches!(e, ParsedElement::Text { content, .. } if !content.trim().is_empty())));
    }

    #[test]
    fn test_mindstate_progressbar_exp_attrs() {
        let mut parser = XmlParser::new();
        let elements = parser.parse_line(
            "<progressBar id='mindState' value='34' text='muddled' field_exp='340' max_field_exp='1000' exp='1234567' ascension_exp='150000' until_next='4321' lumnis='1' rpa='1.5'/>",
        );

        let ParsedElement::MindStateExp {
            field_exp,
            max_field_exp,
            exp,
            ascension_exp,
            until_next,
            fashlonae,
            lumnis,
            rpa,
        } = elements
            .iter()
            .find(|e| matches!(e, ParsedElement::MindStateExp { .. }))
            .expect("MindStateExp element")
        else {
            unreachable!()
        };
        assert_eq!(*field_exp, Some(340));
        assert_eq!(*max_field_exp, Some(1000));
        assert_eq!(*exp, Some(1_234_567));
        assert_eq!(*ascension_exp, Some(150_000));
        assert_eq!(*until_next, Some(4321));
        assert_eq!(*fashlonae, None);
        assert_eq!(*lumnis, Some(1));
        assert_eq!(*rpa, Some(1.5));

        // The plain ProgressBar element still comes through unchanged
        assert!(elements.iter().any(|e| matches!(
            e,
            ParsedElement::ProgressBar { id, text, .. } if id == "mindState" && text == "muddled"
        )));
    }

    #[test]
    fn test_mindstate_progressbar_without_exp_attrs_still_emits_snapshot() {
        let mut parser = XmlParser::new();
        let elements =
            parser.parse_line("<progressBar id='mindState' value='0' text='clear as a bell'/>");

        // Emitted with all None so the core can clear snapshot-semantics
        // bonus flags when the game omits them
        let ParsedElement::MindStateExp {
            field_exp,
            lumnis,
            rpa,
            ..
        } = elements
            .iter()
            .find(|e| matches!(e, ParsedElement::MindStateExp { .. }))
            .expect("MindStateExp element")
        else {
            unreachable!()
        };
        assert_eq!(*field_exp, None);
        assert_eq!(*lumnis, None);
        assert_eq!(*rpa, None);
    }

    #[test]
    fn test_non_mindstate_progressbar_emits_no_exp_element() {
        let mut parser = XmlParser::new();
        let elements =
            parser.parse_line("<progressBar id='health' value='100' text='health 175/175'/>");
        assert!(!elements
            .iter()
            .any(|e| matches!(e, ParsedElement::MindStateExp { .. })));
    }

    // ==================== Component Parsing ====================

    #[test]
    fn test_component_room_title() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<component id='room title'>Town Square</component>");

        let comp_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Component { .. })).collect();
        assert_eq!(comp_elements.len(), 1);

        let ParsedElement::Component { id, value } = comp_elements[0] else {
            panic!("Expected Component element, got {:?}", comp_elements[0]);
        };
        assert_eq!(id, "room title");
        assert_eq!(value, "Town Square");
    }

    #[test]
    fn test_compdef_room_desc() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<compDef id='room desc'>A description of the room with <a exist='1' noun='statue'>a marble statue</a>.</compDef>");

        let comp_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::Component { .. })).collect();
        assert_eq!(comp_elements.len(), 1);

        let ParsedElement::Component { id, value } = comp_elements[0] else {
            panic!("Expected Component element, got {:?}", comp_elements[0]);
        };
        assert_eq!(id, "room desc");
        assert!(value.contains("marble statue"));
    }

    // ==================== Active Effects Parsing ====================

    #[test]
    fn test_active_spell() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='Active Spells'><progressBar id='115' value='74' text=\"Fasthr's Reward\" time='03:06:54'/></dialogData>");

        let effect_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ActiveEffect { .. })).collect();
        assert_eq!(effect_elements.len(), 1);

        let ParsedElement::ActiveEffect { category, id, value, text, time } = effect_elements[0] else {
            panic!("Expected ActiveEffect element, got {:?}", effect_elements[0]);
        };
        assert_eq!(category, "ActiveSpells");  // Normalized
        assert_eq!(id, "115");
        assert_eq!(*value, 74);
        assert_eq!(text, "Fasthr's Reward");
        assert_eq!(time, "03:06:54");
    }

    #[test]
    fn test_buff_effect() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='Buffs'><progressBar id='buff1' value='100' text='Strength' time='01:00:00'/></dialogData>");

        let effect_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ActiveEffect { .. })).collect();
        assert_eq!(effect_elements.len(), 1);

        let ParsedElement::ActiveEffect { category, .. } = effect_elements[0] else {
            panic!("Expected ActiveEffect element, got {:?}", effect_elements[0]);
        };
        assert_eq!(category, "Buffs");
    }

    #[test]
    fn test_clear_active_spells() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<dialogData id='Active Spells' clear='t'></dialogData>");

        let clear_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ClearActiveEffects { .. })).collect();
        assert_eq!(clear_elements.len(), 1);

        let ParsedElement::ClearActiveEffects { category } = clear_elements[0] else {
            panic!("Expected ClearActiveEffects element, got {:?}", clear_elements[0]);
        };
        assert_eq!(category, "ActiveSpells");
    }

    // ==================== StreamWindow Parsing ====================

    #[test]
    fn test_stream_window_room() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<streamWindow id='room' subtitle=' - Emberthorn Refuge, Bowery' />");

        let sw_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::StreamWindow { .. })).collect();
        assert_eq!(sw_elements.len(), 1);

        let ParsedElement::StreamWindow { id, subtitle, .. } = sw_elements[0] else {
            panic!("Expected StreamWindow element, got {:?}", sw_elements[0]);
        };
        assert_eq!(id, "room");
        assert_eq!(subtitle.as_deref(), Some(" - Emberthorn Refuge, Bowery"));
    }

    // ==================== Nav/RoomId Parsing ====================

    #[test]
    fn test_nav_room_id() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<nav rm='7150105'/>");

        let room_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::RoomId { .. })).collect();
        assert_eq!(room_elements.len(), 1);

        let ParsedElement::RoomId { id } = room_elements[0] else {
            panic!("Expected RoomId element, got {:?}", room_elements[0]);
        };
        assert_eq!(id, "7150105");
    }

    #[test]
    fn test_app_info_character() {
        let mut parser = test_parser();
        let elements = parser.parse_line(r#"<app char="Nisugi" game="GS" title="[GSIV: Nisugi]"/>"#);
        let app: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::AppInfo { .. }))
            .collect();
        assert_eq!(app.len(), 1);
        let ParsedElement::AppInfo { character } = app[0] else {
            panic!("Expected AppInfo, got {:?}", app[0]);
        };
        assert_eq!(character, "Nisugi");

        // Logout screens send an empty char - no element.
        let elements = parser.parse_line(r#"<app char="" game="" title=""/>"#);
        assert!(!elements
            .iter()
            .any(|e| matches!(e, ParsedElement::AppInfo { .. })));
    }

    // ==================== ClearStream Parsing ====================

    #[test]
    fn test_clear_stream() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<clearStream id='room'/>");

        let clear_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::ClearStream { .. })).collect();
        assert_eq!(clear_elements.len(), 1);

        let ParsedElement::ClearStream { id } = clear_elements[0] else {
            panic!("Expected ClearStream element, got {:?}", clear_elements[0]);
        };
        assert_eq!(id, "room");
    }

    // ==================== LaunchURL Parsing ====================

    #[test]
    fn test_launch_url() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<LaunchURL src='/gs4/play/cm/loader.asp?uname=test'/>");

        let url_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::LaunchURL { .. })).collect();
        assert_eq!(url_elements.len(), 1);

        let ParsedElement::LaunchURL { url } = url_elements[0] else {
            panic!("Expected LaunchURL element, got {:?}", url_elements[0]);
        };
        assert_eq!(url, "/gs4/play/cm/loader.asp?uname=test");
    }

    // ==================== LichWebUI Handshake Parsing ====================

    #[test]
    fn test_lich_webui_handshake_ok() {
        let mut parser = test_parser();
        let elements = parser.parse_line(
            r#"<LichWebUI status="ok" port="51423" url="http://127.0.0.1:51423/" auth="http://127.0.0.1:51423/auth?token=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" schema="1"/>"#,
        );

        let webui: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::LichWebUI(hs) => Some(hs),
                _ => None,
            })
            .collect();
        assert_eq!(webui.len(), 1);
        assert_eq!(webui[0].status, "ok");
        assert_eq!(webui[0].port, 51423);
        assert_eq!(webui[0].url, "http://127.0.0.1:51423/");
        assert_eq!(webui[0].schema, 1);
        assert_eq!(
            webui[0].token(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        // The handshake line is a control tag: no visible text should be emitted.
        assert!(!elements.iter().any(|e| matches!(
            e,
            ParsedElement::Text { content, .. } if !content.trim().is_empty()
        )));
    }

    #[test]
    fn test_lich_webui_handshake_disabled() {
        let mut parser = test_parser();
        let elements = parser.parse_line(r#"<LichWebUI status="disabled"/>"#);

        let webui: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::LichWebUI(hs) => Some(hs),
                _ => None,
            })
            .collect();
        assert_eq!(webui.len(), 1);
        assert_eq!(webui[0].status, "disabled");
        assert_eq!(webui[0].port, 0);
        assert_eq!(webui[0].token(), None);
    }

    // ==================== Menu Response Parsing ====================

    #[test]
    fn test_menu_response() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<menu id='123'><mi coord='2524,1898'/><mi coord='2524,1735' noun='gleaming steel baselard'/></menu>");

        let menu_elements: Vec<_> = elements.iter().filter(|e| matches!(e, ParsedElement::MenuResponse { .. })).collect();
        assert_eq!(menu_elements.len(), 1);

        let ParsedElement::MenuResponse { id, coords } = menu_elements[0] else {
            panic!("Expected MenuResponse element, got {:?}", menu_elements[0]);
        };
        assert_eq!(id, "123");
        assert_eq!(coords.len(), 2);
        assert_eq!(coords[0].0, "2524,1898");
        assert!(coords[0].1.is_none());
        assert_eq!(coords[1].0, "2524,1735");
        assert_eq!(coords[1].1.as_deref(), Some("gleaming steel baselard"));
    }

    // ==================== Dialog Parsing ====================

    #[test]
    fn test_dialog_open_with_buttons() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<openDialog type='dynamic' id='choosemode' title='Custom Actions Menu' location='center' height='50' width='300'><dialogData name='choosemode'><cmdButton id='addcustom' value='Add New' cmd='_custom dialog add qmech'/><closeButton id='cancelcustom' value='Cancel' cmd=''/></dialogData></openDialog>");

        let dialog_open = elements.iter().find(|e| matches!(e, ParsedElement::DialogOpen { .. }));
        assert!(dialog_open.is_some());

        let dialog_buttons: Vec<_> = elements
            .iter()
            .filter_map(|e| {
                if let ParsedElement::DialogButtons { id, clear, buttons } = e {
                    Some((id, clear, buttons))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(dialog_buttons.len(), 1);
        assert_eq!(dialog_buttons[0].0, "choosemode");
        assert!(!*dialog_buttons[0].1);
        assert_eq!(dialog_buttons[0].2.len(), 2);
        assert_eq!(dialog_buttons[0].2[0].label, "Add New");
        assert_eq!(dialog_buttons[0].2[0].command, "_custom dialog add qmech");
        assert!(!dialog_buttons[0].2[0].is_close);
        assert!(!dialog_buttons[0].2[0].is_radio);
        assert!(!dialog_buttons[0].2[0].selected);
        assert!(!dialog_buttons[0].2[0].autosend);
        assert!(dialog_buttons[0].2[0].group.is_none());
        assert_eq!(dialog_buttons[0].2[1].label, "Cancel");
        assert!(dialog_buttons[0].2[1].is_close);
        assert!(!dialog_buttons[0].2[1].is_radio);
        assert!(!dialog_buttons[0].2[1].selected);
        assert!(!dialog_buttons[0].2[1].autosend);
        assert!(dialog_buttons[0].2[1].group.is_none());
    }

    #[test]
    fn test_dialog_radio_parsing() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<openDialog type='dynamic' id='dialogedit' title='Edit Custom Actions' location='center'><dialogData name='dialogedit'><radio id='hide' value='0' text='hide' cmd='_custom dialog edit2 qmech hide;hide' group='rpedit' autosend=''/><radio id='stand' value='1' text='stand' cmd='_custom dialog edit2 qmech stand;stand' group='rpedit' autosend='t'/></dialogData></openDialog>");

        let dialog_buttons: Vec<_> = elements
            .iter()
            .filter_map(|e| {
                if let ParsedElement::DialogButtons { id, buttons, .. } = e {
                    Some((id, buttons))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(dialog_buttons.len(), 1);
        assert_eq!(dialog_buttons[0].0, "dialogedit");
        assert_eq!(dialog_buttons[0].1.len(), 2);
        assert!(dialog_buttons[0].1[0].is_radio);
        assert!(!dialog_buttons[0].1[0].selected);
        assert!(dialog_buttons[0].1[0].autosend);
        assert_eq!(dialog_buttons[0].1[0].group.as_deref(), Some("rpedit"));
        assert!(dialog_buttons[0].1[1].is_radio);
        assert!(dialog_buttons[0].1[1].selected);
        assert!(dialog_buttons[0].1[1].autosend);
        assert_eq!(dialog_buttons[0].1[1].group.as_deref(), Some("rpedit"));
    }

    #[test]
    fn test_dialog_editbox_parsing() {
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<openDialog type='dynamic' id='displayedit' title='Edit Custom Actions' location='center'><dialogData id='displayedit'><editBox id='displayedit_text' focus='' enterButton='displayeditok' value='hide'/><label id='Label' value='Label&quot; anchor_top&quot;displayedit_text'/><editBox id='commandedit_text' enterButton='displayeditok' value='hide'/><label id='Command' value='Command&quot; anchor_left&quot;commandedit'/></dialogData></openDialog>",
        );

        let dialog_fields: Vec<_> = elements
            .iter()
            .filter_map(|e| {
                if let ParsedElement::DialogFields { id, fields, labels, .. } = e {
                    Some((id, fields, labels))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(dialog_fields.len(), 1);
        assert_eq!(dialog_fields[0].0, "displayedit");
        assert_eq!(dialog_fields[0].1.len(), 2);
        assert_eq!(dialog_fields[0].1[0].id, "displayedit_text");
        assert!(dialog_fields[0].1[0].focused);
        assert_eq!(dialog_fields[0].1[0].enter_button.as_deref(), Some("displayeditok"));
        assert_eq!(dialog_fields[0].1[1].id, "commandedit_text");
        assert_eq!(dialog_fields[0].1[1].value, "hide");
        assert_eq!(dialog_fields[0].2.len(), 2);
        assert_eq!(dialog_fields[0].2[0].value, "Label");
        assert_eq!(dialog_fields[0].2[1].value, "Command");
    }

    #[test]
    fn test_dialog_updowneditbox_parsing() {
        // Test that upDownEditBox is parsed the same as editBox, including enterButton
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<openDialog type='dynamic' id='bank' title='Bank' location='center'><dialogData id='bank'><label id='balance' value='Balance: 12345'/><upDownEditBox id='depositAmount' enterButton='deposit' value='5000'/><upDownEditBox id='withdrawAmount' enterButton='withdraw' value='1000'/><cmdButton id='deposit' value='Deposit' cmd='bank deposit $depositAmount'/><cmdButton id='withdraw' value='Withdraw' cmd='bank withdraw $withdrawAmount'/><closeButton id='close' value='Close'/></dialogData></openDialog>",
        );

        let dialog_fields: Vec<_> = elements
            .iter()
            .filter_map(|e| {
                if let ParsedElement::DialogFields { id, fields, labels, .. } = e {
                    Some((id, fields, labels))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(dialog_fields.len(), 1, "Should emit DialogFields for upDownEditBox");
        assert_eq!(dialog_fields[0].0, "bank");
        assert_eq!(dialog_fields[0].1.len(), 2, "Should have 2 upDownEditBox fields");

        // Verify first field (deposit)
        assert_eq!(dialog_fields[0].1[0].id, "depositAmount");
        assert_eq!(dialog_fields[0].1[0].value, "5000");
        assert_eq!(dialog_fields[0].1[0].enter_button.as_deref(), Some("deposit"));

        // Verify second field (withdraw)
        assert_eq!(dialog_fields[0].1[1].id, "withdrawAmount");
        assert_eq!(dialog_fields[0].1[1].value, "1000");
        assert_eq!(dialog_fields[0].1[1].enter_button.as_deref(), Some("withdraw"));

        // Verify we also got the balance label as standalone
        assert_eq!(dialog_fields[0].2.len(), 1);
        assert_eq!(dialog_fields[0].2[0].id, "balance");
        assert_eq!(dialog_fields[0].2[0].value, "Balance: 12345");

        // Verify buttons were also parsed
        let dialog_buttons: Vec<_> = elements
            .iter()
            .filter_map(|e| {
                if let ParsedElement::DialogButtons { id, buttons, .. } = e {
                    Some((id, buttons))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(dialog_buttons.len(), 1);
        assert_eq!(dialog_buttons[0].1.len(), 3); // deposit, withdraw, close
        assert_eq!(dialog_buttons[0].1[0].id, "deposit");
        assert_eq!(dialog_buttons[0].1[1].id, "withdraw");
        assert_eq!(dialog_buttons[0].1[2].id, "close");
    }

    // ==================== Resident Dialog Parsing ====================

    #[test]
    fn test_resident_dialog_no_popup() {
        // Resident dialogs should NOT emit DialogOpen (no popup)
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<openDialog type='dynamic' id='stance' title='Stance' location='right' height='50' width='190' resident='true'><dialogData id='stance'><progressBar id='pbarStance' value='100' text='defensive (100%)' top='5' left='-5' height='16' width='160' align='n' tooltip='Percent of stance contributing to defense'/></dialogData></openDialog>",
        );

        // Should NOT have DialogOpen (no popup for resident dialogs)
        let dialog_open = elements.iter().find(|e| matches!(e, ParsedElement::DialogOpen { .. }));
        assert!(dialog_open.is_none(), "Resident dialogs should not emit DialogOpen");

        // SHOULD have ProgressBar extracted from the embedded dialogData
        let progress_bars: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::ProgressBar { .. }))
            .collect();
        assert_eq!(progress_bars.len(), 1, "Should extract progressBar from resident dialog");

        if let ParsedElement::ProgressBar { id, value, text, .. } = progress_bars[0] {
            assert_eq!(id, "pbarStance");
            assert_eq!(*value, 100);
            assert_eq!(text, "defensive (100%)");
        } else {
            panic!("Expected ProgressBar");
        }
    }

    #[test]
    fn test_non_resident_dialog_creates_popup() {
        // Non-resident dialogs SHOULD emit DialogOpen (popup)
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<openDialog type='dynamic' id='choosemode' title='Custom Actions Menu' location='center'><dialogData name='choosemode'><cmdButton id='addcustom' value='Add New' cmd='_custom dialog add qmech'/></dialogData></openDialog>",
        );

        // SHOULD have DialogOpen for non-resident dialogs
        let dialog_open = elements.iter().find(|e| matches!(e, ParsedElement::DialogOpen { .. }));
        assert!(dialog_open.is_some(), "Non-resident dialogs should emit DialogOpen");
    }

    #[test]
    fn test_resident_encumbrance_dialog() {
        // Test encumbrance resident dialog with progressBar and label
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<openDialog type='dynamic' id='encum' title='Encumbrance' location='right' height='100' width='190' resident='true'><dialogData id='encum'><progressBar id='encumlevel' value='0' text='None' top='5' left='-5' align='n' width='160' height='15'/><label id='encumblurb' value='You are not encumbered enough to notice.' top='10' left='0' align='n' width='160' height='50' justify='0' anchor_top='encumlevel'/></dialogData></openDialog>",
        );

        // Should NOT have DialogOpen
        let dialog_open = elements.iter().find(|e| matches!(e, ParsedElement::DialogOpen { .. }));
        assert!(dialog_open.is_none(), "Resident dialogs should not emit DialogOpen");

        // Should have ProgressBar
        let progress_bars: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::ProgressBar { .. }))
            .collect();
        assert_eq!(progress_bars.len(), 1);

        // Should have Label
        let labels: Vec<_> = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::Label { .. }))
            .collect();
        assert_eq!(labels.len(), 1);
    }

    #[test]
    fn effect_duration_label_justify_passes_through() {
        // Verbatim wire line (Buffs effect row, the single most common
        // justify usage on the wire: ×10.5M in the 2026-08 log census).
        // justify='2' = right (bitfield low bits); anchor_right is EMPTY —
        // parse_control_layout currently drops empty anchors, pinned below.
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<dialogData id='Buffs' clear='t'></dialogData><dialogData id='Buffs'><progressBar id='220997' value='100' text=\"Enhancive Stats Boost\" left='22%' top='0' width='76%' height='15' time='00:05:04'/><label id='l220997' value='0:05 ' top='0' left='0' justify='2' anchor_right=''/></dialogData>",
        );

        let labels: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::DialogFields { id, labels, .. } if id == "Buffs" => Some(labels),
                _ => None,
            })
            .flatten()
            .collect();
        let duration = labels
            .iter()
            .find(|l| l.id == "l220997")
            .expect("duration label ingested into the dialog store");
        assert_eq!(
            duration.justify,
            Some(2),
            "wire justify='2' (right) must reach the label spec"
        );
        // Characterization: empty-string anchors are dropped at parse today
        // (parse_control_layout filters them). If Wrayth treats an empty
        // anchor target as "anchor to the parent edge", honoring it is a
        // future, deliberate change — this assert makes that diff visible.
        assert_eq!(
            duration.layout.as_ref().and_then(|l| l.anchor_right.as_deref()),
            None,
            "empty anchor_right is currently discarded (pinned behavior)"
        );
    }

    // ==================== UberBar (resident dynamic dialog) ====================
    //
    // Characterization of how uberbar_eo.lic's feed parses TODAY, before any
    // UberBar-support work. UberBar is a resident, non-templated openDialog
    // (id='UberBar') whose dialogData mixes <skin>, <image>, <label>, and
    // <progressBar> positioned by an anchor grid (anchor_left/anchor_top).
    //
    // These asserts LOCK CURRENT BEHAVIOR so the Tier-1 change (ingest resident
    // labels/bars into the DialogState WITH layout) is a visible, reviewed diff
    // rather than a silent behavior swap. They are expected to be *updated*
    // when that change lands — that is the point of a characterization test.

    /// A trimmed but faithful slice of one real UberBar frame: the panel open,
    /// the injury skin, one wound image, two label rows, and two vitals bars,
    /// carrying the same align/anchor/top/left geometry the script emits.
    const UBERBAR_FRAME: &str = "<openDialog type='dynamic' id='UberBar' title=\"Nisugi's Uberbar\" target='UberBar' location='main' height='282' width='190' resident='true'>\
        <dialogData id='UberBar' clear='t'>\
        <skin id='ubinjury' name='InjuriesPanel' controls='nsys,leftArm,rightArm' top='5' left='5' width='100' height='150' align='nw'/>\
        <image id='nsys' name='Injury3' cmd='cure nerves' tooltip='cure nerves' height='0' width='0'/>\
        <label id='ublog' value='Today:' justify='4' anchor_left='ubinjury' align='n' top='5' left='5' height='15' width='50'/>\
        <label id='ublogv' value='1234' justify='6' anchor_left='ublog' align='n' top='5' left='0' height='15' width='50'/>\
        <progressBar id='health' value='95' text='95/100' customText='t' anchor_left='ubinjury' anchor_top='ubbars' top='3' left='4' width='100' height='15'/>\
        <progressBar id='mana' value='80' text='80/100' customText='t' anchor_left='ubinjury' anchor_top='health' top='3' left='4' width='100' height='15'/>\
        </dialogData></openDialog>";

    #[test]
    fn uberbar_resident_dialog_ingests_bars_and_labels_with_layout() {
        // Tier 1 landed: resident non-templated dialogData bars + labels are
        // now ADDITIVELY routed into the DialogState carrying anchor geometry,
        // while the flat stream ProgressBar/Label emit is preserved (widgets
        // like encumbrance/experience still consume it).
        let mut parser = test_parser();
        let elements = parser.parse_line(UBERBAR_FRAME);

        // Resident + no template => announced as a dockable DialogPanel, not a
        // transient popup (no DialogOpen).
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, ParsedElement::DialogPanelOpen { id, .. } if id == "UberBar")),
            "resident UberBar should announce a DialogPanel"
        );
        assert!(
            !elements.iter().any(|e| matches!(e, ParsedElement::DialogOpen { .. })),
            "resident dialogs must not emit a popup DialogOpen"
        );

        // <image> reaches the DialogState path (DialogControls) with layout —
        // unchanged by Tier 1.
        let image_ctrls: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::DialogControls { id, images, .. } if id == "UberBar" => Some(images),
                _ => None,
            })
            .collect();
        assert_eq!(image_ctrls.len(), 1, "UberBar images should arrive as DialogControls");
        assert!(
            image_ctrls[0].iter().any(|img| img.id == "nsys" && img.command == "cure nerves"),
            "the wound image keeps its cmd"
        );

        // PRESERVED: the flat stream emit still happens (widgets depend on it).
        let flat_bars = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::ProgressBar { .. }))
            .count();
        assert_eq!(flat_bars, 2, "flat stream ProgressBar emit is preserved for widgets");
        let flat_labels = elements
            .iter()
            .filter(|e| matches!(e, ParsedElement::Label { .. }))
            .count();
        assert_eq!(flat_labels, 2, "flat stream Label emit is preserved for widgets");

        // NEW: bars are ALSO ingested into the dialog store, carrying layout.
        let dlg_bars: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::DialogProgressBars { id, progress_bars, .. } if id == "UberBar" => {
                    Some(progress_bars)
                }
                _ => None,
            })
            .collect();
        assert_eq!(dlg_bars.len(), 1, "resident bars now reach the dialog store");
        let health = dlg_bars[0]
            .iter()
            .find(|b| b.id == "health")
            .expect("health bar ingested");
        let hlayout = health.layout.as_ref().expect("health bar carries layout");
        assert_eq!(hlayout.anchor_top.as_deref(), Some("ubbars"));
        assert_eq!(hlayout.anchor_left.as_deref(), Some("ubinjury"));

        // NEW: standalone labels are ALSO ingested (as DialogFields with empty
        // fields), carrying layout.
        let dlg_labels: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::DialogFields { id, labels, fields, .. } if id == "UberBar" => {
                    Some((labels, fields))
                }
                _ => None,
            })
            .collect();
        assert_eq!(dlg_labels.len(), 1, "resident labels now reach the dialog store");
        let (labels, fields) = &dlg_labels[0];
        assert!(fields.is_empty(), "UberBar carries no input fields");
        let logv = labels
            .iter()
            .find(|l| l.id == "ublogv")
            .expect("the value label ingested");
        assert_eq!(logv.value, "1234");
        assert_eq!(
            logv.layout.as_ref().and_then(|l| l.anchor_left.as_deref()),
            Some("ublog"),
            "the value label keeps its anchor chain"
        );

        // Tier 3: <skin> now reaches the dialog store via DialogControls,
        // carrying its asset name + backed control ids + layout.
        let skins: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::DialogControls { id, skins, .. } if id == "UberBar" => Some(skins),
                _ => None,
            })
            .flatten()
            .collect();
        let injury = skins
            .iter()
            .find(|s| s.id == "ubinjury")
            .expect("the InjuriesPanel skin is ingested");
        assert_eq!(injury.name, "InjuriesPanel");
        assert!(
            injury.controls.contains(&"nsys".to_string())
                && injury.controls.contains(&"rightArm".to_string()),
            "the skin lists the body-part controls it backs"
        );
        assert_eq!(injury.layout.as_ref().and_then(|l| l.width), Some(100));
    }

    // ==================== Redesign Phase 1c: window vocabulary ====================

    #[test]
    fn expose_verbs_parse_for_all_three_kinds() {
        // Wire-verbatim: the game sends exposeDialog for the bank ×4,265;
        // exposeStream carries popup-like streams (charprofile).
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<exposeDialog id='bank'/><exposeStream id='charprofile'/><exposeContainer id='stow'/>",
        );
        let exposes: Vec<_> = elements
            .iter()
            .filter_map(|e| match e {
                ParsedElement::Expose { kind, id } => Some((kind.as_str(), id.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            exposes,
            [("dialog", "bank"), ("stream", "charprofile"), ("container", "stow")]
        );
    }

    #[test]
    fn delete_container_parses() {
        let mut parser = test_parser();
        let elements = parser.parse_line("<deleteContainer id='stow'/>");
        assert!(elements
            .iter()
            .any(|e| matches!(e, ParsedElement::DeleteContainer { id } if id == "stow")));
    }

    #[test]
    fn dialog_data_name_attribute_normalizes_to_id() {
        // bugDialogBox keys on name= instead of id= (×6 on the wire); it
        // must flow through the same paths as an id= dialog.
        let mut parser = test_parser();
        let by_name = parser.parse_line("<dialogData name='bugDialogBox' clear='t'></dialogData>");
        let mut parser = test_parser();
        let by_id = parser.parse_line("<dialogData id='bugDialogBox' clear='t'></dialogData>");
        assert!(!by_id.is_empty(), "id= form produces elements");
        assert_eq!(
            format!("{by_name:?}"),
            format!("{by_id:?}"),
            "name= form produces exactly the id= form's elements"
        );
    }

    #[test]
    fn stream_window_placement_attrs_become_window_hints() {
        // These attributes were previously extracted-and-dropped; the
        // PlacementHint pipeline (Phase 3) reads them from WindowHints.
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<streamWindow id='charprofile' title='Profile' location='force-center' resident='false' save='' scroll='manual' ifClosed=''/>",
        );
        let hints = elements
            .iter()
            .find_map(|e| match e {
                ParsedElement::WindowHints { id, attrs } => Some((id.clone(), attrs.clone())),
                _ => None,
            })
            .expect("streamWindow emits hints");
        assert_eq!(hints.0, "charprofile");
        let get = |k: &str| hints.1.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("location"), Some("force-center"));
        assert_eq!(get("resident"), Some("false"));
        assert_eq!(get("scroll"), Some("manual"));
        assert_eq!(get("save"), Some(""));
        // The declaration element itself still arrives beside the hints.
        assert!(elements
            .iter()
            .any(|e| matches!(e, ParsedElement::StreamWindow { id, .. } if id == "charprofile")));
    }

    #[test]
    fn open_dialog_size_hints_are_captured_and_absent_attrs_emit_nothing() {
        let mut parser = test_parser();
        let elements = parser.parse_line(
            "<openDialog type='dynamic' id='espMasterDialog' title='ESP' location='right' height='2100' resident='true'><dialogData id='espMasterDialog'></dialogData></openDialog>",
        );
        let hints = elements
            .iter()
            .find_map(|e| match e {
                ParsedElement::WindowHints { id, attrs } => Some((id.clone(), attrs.clone())),
                _ => None,
            })
            .expect("openDialog emits hints");
        assert_eq!(hints.0, "espMasterDialog");
        let get = |k: &str| hints.1.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("location"), Some("right"));
        assert_eq!(get("height"), Some("2100"), "viewport-busting sizes arrive raw; clamping is the placement engine's job");

        // A declaration with no placement attributes emits NO hints.
        let mut parser = test_parser();
        let bare = parser.parse_line("<streamWindow id='thoughts' title='Thoughts'/>");
        assert!(!bare.iter().any(|e| matches!(e, ParsedElement::WindowHints { .. })));
    }
}
