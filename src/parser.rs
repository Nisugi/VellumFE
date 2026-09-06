//! Streaming XML parser that converts GemStone IV data into strongly typed events.
//!
//! The parser keeps track of nested styles, open streams, dialog fragments, and
//! ad-hoc pattern detectors (e.g., event timers) so the rest of the client can
//! operate on higher-level `ParsedElement` values instead of raw XML.

mod builders;
mod dialogs;
mod handlers;

pub(crate) use builders::{InvManagerBuilder, InvViewItemBuilder, PairedCapture};
mod links;
mod numbers;
mod text;
pub(crate) use numbers::parse_progress_numbers;
#[cfg(test)]
pub(crate) use numbers::{first_number, last_number};

use crate::config::EventAction;
use crate::data::{DialogButton, DialogDropDown, LinkData, QuickbarEntry};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Text categories emitted by the XML stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanType {
    Normal,      // Regular text
    Link,        // <a> tag from parser
    Monsterbold, // <preset id="monsterbold"> from parser
    Spell,       // <spell> tag from parser
    Speech,      // <preset id="speech"> from parser
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
    /// AimTimerDialog timer (aimed-shot countdown): absolute server end
    /// time, 0 = cleared (dialog closed).
    AimTime {
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
    /// `<vellumImg src='banner' rows='4' align='left'/>` - VellumFE extension
    /// letting a script float a real image into a text window.
    ///
    /// `src` is a pool image NAME (shortcode alphabet), never a path: the
    /// frontend resolves it through the image registry, so a feed can name
    /// art but can never read an arbitrary file. `rows` is the requested
    /// height in text rows, clamped by the renderer to what the window can
    /// actually show.
    VellumImage {
        src: String,
        rows: f32,
        align: crate::data::FloatAlign,
    },
    /// `<resource picture='32'/>` - the game's own room-picture feed.
    ///
    /// Wrayth resolves the id against Simu's art and shows it beside the room
    /// name; the wire carries only the NUMBER, never the image or a URL. So
    /// VellumFE treats it as "this room has picture N" and resolves N against
    /// the user's own image pool. `0` is the overwhelmingly common value and
    /// means *no* picture — it clears whatever the last room set.
    RoomPicture {
        id: u32,
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
    /// A stream redirect force-closed by `<prompt>` because its popStream was
    /// eaten upstream. Consumers must treat any per-stream buffer as TORN
    /// (possibly truncated), not as a complete authoritative snapshot.
    StreamPopForced,
    /// A pop uncovered an enclosing stream redirect: subsequent text belongs
    /// to `id`, but WITHOUT the arrival side effects of a fresh StreamPush
    /// (buffer clears for room/inv/reserve). Emitted by the parser's stream
    /// stack so scalar consumers stay correct under nested redirects.
    StreamResume {
        id: String,
    },
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
        popup_id: String,                // Dialog ID: "injuries-10154507"
        injuries: Vec<(String, String)>, // Vec of (body_part, injury_level)
        clear: bool,                     // true if clearing injuries
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
    /// Quest/objectives feed: `<objectives action='full-refresh'>` with
    /// nested `<objective>` entries (Saga quest panel).
    ObjectivesUpdate {
        action: String, // "full-refresh" replaces; anything else upserts
        entries: Vec<crate::data::Objective>,
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
    /// `<pulse min="46" max="75" mana="0|1"/>` — the game's pulse
    /// announcement (extended feed, served to clients identifying as
    /// WRAYTH 1.0.1.28+). Every pulse absorbs field experience (when any is
    /// pooled), and every OTHER pulse is also a mana pulse. `min`/`max`
    /// bound the seconds until the NEXT pulse (missing/invalid values fall
    /// back to the 46/75 defaults Saga uses), and `mana` announces whether
    /// that next pulse restores mana — the server declares the alternation,
    /// nothing is inferred. Replaces the old trick of inferring pulses from
    /// observed mana gain or exp absorption.
    Pulse {
        mana: bool,
        min: u32,
        max: u32,
    },
    /// `<inventoryManager id='<token>' room='...'>` ... `</inventoryManager>`
    /// — structured inventory snapshot (extended feed), sent only in response
    /// to a client `_inventory manager <token>` request. Each entry in
    /// `items` is the raw attributes of one `<i .../>` child; `continuations`
    /// carries raw `<continuation root=... last=.../>` cursors from paginated
    /// responses. Attributes are passed raw so the core layer owns the field
    /// mapping, like CreatureStatus.
    InventoryManager {
        token: String,
        room: String,
        /// Continuation-envelope echo: the requested subtree root exist id.
        /// Present only on responses to a `... continue ...` request.
        root: Option<String>,
        /// Continuation-envelope echo: the last item already delivered.
        after: Option<String>,
        /// Error/status marker. `"stale"` = the continuation cursor is no
        /// longer valid (response must be empty; reload from scratch). Any
        /// other non-empty value is a failure.
        state: Option<String>,
        items: Vec<Vec<(String, String)>>,
        continuations: Vec<Vec<(String, String)>>,
    },
    /// `<inventoryViewItem>` response (extended feed); see
    /// [`InventoryViewItemResponse`].
    InventoryViewItem(InventoryViewItemResponse),
    /// `<worldEvent realm="..." expires="MIN" time="...">text</worldEvent>`
    /// (extended feed) — a realm-wide event announcement. `expires` is in
    /// MINUTES (Saga computes expiresAt = now + 60000 * expires).
    WorldEvent {
        realm: Option<String>,
        expires_min: Option<u32>,
        text: String,
    },
    /// `<PantheonStatus value="N"/>` (extended feed) — pantheon meter.
    PantheonStatus {
        value: u32,
    },
}

/// `<inventoryViewItem id exist [state]>` ... `</inventoryViewItem>` —
/// per-item detail response to `_inventory viewitem <token> <exist>`
/// (extended feed). Each `<result command="look|read|...">` section's text
/// is captured with inline markup flattened (`<br/>` = newline); the body
/// never reaches the text stream. The envelope's bare `closed` attribute
/// (when the item is a container) is the authoritative open/closed signal —
/// Saga probes containers with viewitem precisely to read it.
#[derive(Debug, Clone, PartialEq)]
pub struct InventoryViewItemResponse {
    pub token: String,
    pub exist: String,
    /// Non-empty = failure ("malformed" is synthesized for prompt-torn
    /// captures, mirroring Saga)
    pub state: Option<String>,
    /// The envelope carried a `closed` attribute (container closed);
    /// absence on a container response means open.
    pub closed_attr: bool,
    /// (command, flattened text) per `<result>` section, in feed order
    pub results: Vec<(String, String)>,
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
#[derive(Debug, Clone, Default)]
pub(crate) struct ColorStyle {
    fg: Option<String>,
    bg: Option<String>,
}

/// Stateful streaming parser that consumes wizard XML chunks and emits
/// high-level `ParsedElement` values.
#[derive(Clone)]
pub struct XmlParser {
    current_stream: String,
    /// Open stream redirects, innermost last; `current_stream` mirrors the
    /// top (or "main"), so a pop restores the enclosing redirect.
    stream_stack: Vec<String>,
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
    /// In-flight `<inventoryManager>` block (None outside one)
    pub(crate) inv_manager: Option<InvManagerBuilder>,
    pub(crate) inv_viewitem: Option<InvViewItemBuilder>,
    pub(crate) paired_capture: Option<PairedCapture>,

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
            stream_stack: Vec::new(),
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
            inv_manager: None,
            inv_viewitem: None,
            paired_capture: None,
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
        // GSL protocol tags (\x1C + "GS" + letter, from Lich proxy) are
        // internal messages, never display text: skip pure-GSL lines.
        if Self::is_gsl_tag_line(line) {
            tracing::debug!("[GSL] Skipping GSL tag line: '{}'", line);
            return vec![];
        }

        let line = Self::strip_gsl_tags(line);

        // An inventoryViewItem capture (active or opening on this line) owns
        // the whole line: its styled body must never leak into the text
        // stream. Checked BEFORE the blank-line early-return so blank lines
        // inside a capture become newlines in the section, not stream text.
        // The dedicated walker hands back any post-close remainder (e.g. a
        // trailing prompt) for normal parsing.
        if self.inv_viewitem.is_some() || line.contains("<inventoryViewItem") {
            return self.parse_viewitem_line(&line);
        }

        // An active multi-line paired capture owns the whole line (checked
        // before the blank-line return so blank lines stay in the capture).
        if self.paired_capture.is_some() {
            return self.continue_paired_capture(&line);
        }

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
            const PAIRED_TAGS: [(&str, &str); 12] = [
                ("<objectives", "</objectives>"),
                ("<prompt", "</prompt>"),
                ("<worldEvent", "</worldEvent>"),
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
            if let Some(tag_start) = Self::find_tag_start(remaining) {
                for (start_pattern, end_pattern) in PAIRED_TAGS {
                    if !remaining[tag_start..].starts_with(start_pattern) {
                        continue;
                    }

                    // Closing tag on a LATER line: enter multi-line capture
                    // (eligible tags only; self-closing opens fall through).
                    if !remaining[tag_start..].contains(end_pattern)
                        && Self::multi_line_paired(start_pattern)
                        && self.try_begin_paired_capture(end_pattern, &remaining[tag_start..])
                    {
                        if tag_start > 0 {
                            text_buffer.push_str(&remaining[..tag_start]);
                        }
                        remaining = "";
                        found_paired = true;
                        break;
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
            if let Some(tag_start) = Self::find_tag_start(remaining) {
                // Add text before tag to buffer
                if tag_start > 0 {
                    text_buffer.push_str(&remaining[..tag_start]);
                }

                // Find tag end
                if let Some(tag_end) = remaining[tag_start..].find('>') {
                    let tag = &remaining[tag_start..tag_start + tag_end + 1];

                    // Simu splits one logical line into fragments with
                    // <popStream/><pushStream id="same"/> pairs mid-sentence
                    // (arena spectate on the familiar stream). Wrayth glues
                    // those fragments back together; treat the pair as a
                    // no-op so the line stays whole instead of breaking at
                    // every fragment boundary.
                    if tag.starts_with("<popStream") {
                        let after = &remaining[tag_start + tag_end + 1..];
                        if let Some(push_len) =
                            Self::same_stream_repush_len(after, &self.current_stream)
                        {
                            remaining = &after[push_len..];
                            continue;
                        }
                    }

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
            self.pop_stream(elements);
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
            self.pop_stream(elements);
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
        } else if tag.starts_with("<resource") {
            self.handle_resource(tag, elements);
        } else if tag.starts_with("<vellumImg ") || tag.starts_with("<vellum-img ") {
            // The image becomes its own segment, so any text buffered before
            // it must land first or the two would merge into one run. This
            // tag is not in the color open/close sets that flush above.
            if !text_buffer.is_empty() {
                self.flush_text_with_events(std::mem::take(text_buffer), elements);
            }
            self.handle_vellum_img(tag, elements);
        } else if tag.starts_with("<spell") {
            self.handle_spell(tag, text_buffer, elements);
        } else if tag.starts_with("<left") {
            self.handle_left_hand(tag, text_buffer, elements);
        } else if tag.starts_with("<right") {
            self.handle_right_hand(tag, text_buffer, elements);
        } else if tag.starts_with("<compass") {
            self.handle_compass(tag, elements);
        } else if tag.starts_with("<objectives") {
            self.handle_objectives(tag, elements);
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
            tracing::debug!(
                "Parser: Matched dropDownBox tag: {}",
                &tag[..tag.len().min(100)]
            );
            self.handle_dropdown(tag, elements);
        }
        // Creature status snapshot (usually embedded in room objs components,
        // which are captured whole; this arm catches any sent standalone)
        else if tag.starts_with("<crtrStatus ") {
            self.handle_crtr_status(tag, elements);
        } else if tag.starts_with("<roommeta ") {
            self.handle_roommeta(tag, elements);
        }
        // Extended feed (WRAYTH 1.0.1.28+ banner): pulse tick and the
        // structured inventory response to `_inventory manager <token>`
        else if tag.starts_with("<pulse") {
            self.handle_pulse(tag, elements);
        } else if tag.starts_with("<worldEvent") {
            self.handle_world_event(tag, elements);
        } else if tag.starts_with("<PantheonStatus") {
            if let Some(value) =
                Self::extract_attribute(tag, "value").and_then(|v| v.trim().parse().ok())
            {
                elements.push(ParsedElement::PantheonStatus { value });
            }
        } else if tag.starts_with("<inventoryManager") {
            self.handle_inventory_manager_open(tag, elements);
        } else if Self::is_close_tag(tag, "inventoryManager") {
            self.handle_inventory_manager_close(elements);
        }
        // `<i>`/`<continuation>` children only mean something inside an
        // inventoryManager block; the guard keeps a bare `<i ...>` elsewhere
        // from being swallowed here.
        else if self.inv_manager.is_some()
            && (tag.starts_with("<i ") || tag.starts_with("<continuation "))
        {
            self.handle_inventory_manager_child(tag);
        }
        // Debug: catch any dropdown-related tags we might be missing
        // (case-sensitive checks - avoids a per-tag to_lowercase allocation)
        else if tag.contains("dropDown") || tag.contains("dropdown") || tag.contains("dDB") {
            tracing::warn!(
                "Parser: Unhandled dropdown-like tag: {}",
                &tag[..tag.len().min(100)]
            );
        }
        // Silently ignore these tags
        else if tag.starts_with("<compDef ")
            || tag == "</compDef>"
            || tag.starts_with("<streamWindow ")
            || tag.starts_with("<skin ")
        {
            // Ignore these (UI layout tags)
        }
        // Known wire tags we deliberately don't act on: swallow quietly.
        // Unknown names mean new server markup — pass through as visible
        // text so nothing is ever silently dropped.
        else {
            let name = Self::tag_name(tag);
            if Self::is_known_wire_tag(name) {
                tracing::debug!("Parser: known unhandled tag <{}>", name);
            } else {
                tracing::warn!(
                    "Parser: unknown tag passed through as text: {}",
                    &tag[..tag.len().min(120)]
                );
                text_buffer.push_str(tag);
            }
        }
    }
}

impl Default for XmlParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
