//! Streaming XML parser that converts GemStone IV data into strongly typed events.
//!
//! The parser keeps track of nested styles, open streams, dialog fragments, and
//! ad-hoc pattern detectors (e.g., event timers) so the rest of the client can
//! operate on higher-level `ParsedElement` values instead of raw XML.

use crate::config::EventAction;
use crate::data::{DialogButton, LinkData, QuickbarEntry};
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
    System,      // Client/system messages (not produced by parser; used for UI echoes)
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

/// Convenience struct used while normalizing spans before they are wrapped in a
/// higher-level `ParsedElement`.
#[derive(Clone, Debug)]
pub struct ParsedSpan {
    pub text: String,
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub span_type: SpanType,
    pub preset: Option<String>,
    pub link_data: Option<LinkData>,
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
    RoomId {
        id: String,
    },
    StreamWindow {
        id: String,
        subtitle: Option<String>,
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
    },
    DialogButtons {
        id: String,
        clear: bool,
        buttons: Vec<DialogButton>,
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
    /// Target list from combat dialog dropdown (for direct-connect users)
    TargetList {
        current_target: String,  // from value attribute
        targets: Vec<String>,    // from content_text (comma-split)
        target_ids: Vec<String>, // from content_value (comma-split)
    },
    /// Container window definition
    Container {
        id: String,
        title: String,
        target: String,
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
}

#[derive(Debug, Clone)]
pub struct DialogProgressBarSpec {
    pub id: String,
    pub value: u32,   // Percentage 0-100
    pub text: String, // Display text (e.g., "defensive (100%)")
}

/// Tracks the currently active foreground/background/bold settings while the
/// parser walks nested XML tags.
#[derive(Debug, Clone)]
#[derive(Default)]
pub(crate) struct ColorStyle {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
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

    // Semantic type tracking
    pub(crate) link_depth: usize,                   // Track nested links
    pub(crate) spell_depth: usize,                  // Track nested spells
    pub(crate) current_link_data: Option<LinkData>, // Current link metadata (exist_id, noun)
    pub(crate) current_preset_id: Option<String>, // Current preset ID (e.g., "speech", "monsterbold")
    // Menu tracking
    current_menu_id: Option<String>, // ID of menu being parsed
    current_menu_coords: Vec<(String, Option<String>)>, // (coord, optional noun) pairs for current menu

    // Container/Inventory tracking
    current_container_id: Option<String>, // ID of container currently receiving items

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
            link_depth: 0,
            spell_depth: 0,
            current_link_data: None,
            current_preset_id: None,
            current_menu_id: None,
            current_menu_coords: Vec::new(),
            current_container_id: None,
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
        let mut remaining = line.as_str();

        while !remaining.is_empty() {
            // Check for paired tags first (manually check for each type)
            let mut found_paired = false;

            for tag_name in &[
                "prompt",
                "spell",
                "left",
                "right",
                "compass",
                "openDialog",
                "dialogData",
                "component",
                "compDef",
                "inv",
            ] {
                let start_pattern = format!("<{}", tag_name);
                let end_pattern = format!("</{}>", tag_name);

                if let Some(tag_start) = remaining.find(&start_pattern) {
                    // Make sure this is the earliest match
                    if remaining.find('<').is_some_and(|pos| pos < tag_start) {
                        continue;
                    }

                    // Find the closing tag
                    if let Some(tag_end_start) = remaining[tag_start..].find(&end_pattern) {
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

    fn process_tag(
        &mut self,
        tag: &str,
        text_buffer: &mut String,
        elements: &mut Vec<ParsedElement>,
    ) {
        // Debug: log tags that might be LaunchURL
        if tag.contains("LaunchURL") || tag.contains("launch") {
            tracing::info!("process_tag received potential LaunchURL: {}", tag);
        }

        // Debug: log any tag containing "container" or "inv" for troubleshooting
        let tag_lower = tag.to_lowercase();
        if tag_lower.contains("container") || tag_lower.starts_with("<inv ") || tag_lower.starts_with("</inv") {
            tracing::info!("process_tag received container/inv tag: {}", &tag[..tag.len().min(150)]);
        }

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
            || tag == "</a>"
            || tag == "</d>"
            || tag == "<popBold/>"
            || tag == "</b>";

        // Flush before opening new colors (so old styled text is emitted with old colors)
        if color_opening && !text_buffer.is_empty() {
            self.flush_text_with_events(text_buffer.clone(), elements);
            text_buffer.clear();
        }

        // Flush before closing colors (so text gets the color before we pop it)
        if color_closing && !text_buffer.is_empty() {
            self.flush_text_with_events(text_buffer.clone(), elements);
            text_buffer.clear();
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
                self.flush_text_with_events(text_buffer.clone(), elements);
                text_buffer.clear();
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
                self.flush_text_with_events(text_buffer.clone(), elements);
                text_buffer.clear();
            }
            // Switch to the inline stream (handled same as pushStream)
            self.handle_push_stream(tag, elements);
        } else if tag == "</stream>" {
            // End of inline stream tag - flush buffer and pop stream
            if !text_buffer.is_empty() {
                self.flush_text_with_events(text_buffer.clone(), elements);
                text_buffer.clear();
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
                self.flush_text_with_events(text_buffer.clone(), elements);
                text_buffer.clear();
            }
            self.handle_push_stream(tag, elements);
            if let Some(prefix) = carried_prefix {
                *text_buffer = prefix;
            }
        } else if tag.starts_with("<popStream") || tag == "</component>" {
            if !text_buffer.is_empty() {
                self.flush_text_with_events(text_buffer.clone(), elements);
                text_buffer.clear();
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
        } else if tag.starts_with("<spell") {
            self.handle_spell(tag, text_buffer, elements);
        } else if tag.starts_with("<left") {
            self.handle_left_hand(tag, text_buffer, elements);
        } else if tag.starts_with("<right") {
            self.handle_right_hand(tag, text_buffer, elements);
        } else if tag.starts_with("<compass") {
            self.handle_compass(tag, elements);
        } else if tag.starts_with("<dialogData ") {
            // Call both handlers to cover all dialogData processing
            self.handle_dialog_data(tag, elements);
            self.handle_dialogdata(tag, elements);
        } else if tag.starts_with("<openDialog ") {
            self.handle_open_dialog(tag, elements);
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
        } else if tag.starts_with("<streamWindow ") {
            self.handle_stream_window(tag, elements);
        } else if tag.starts_with("<d ") || tag == "<d>" {
            self.handle_d_tag(tag);
        } else if tag == "</d>" {
            self.handle_d_close();
        } else if tag.starts_with("<a ") {
            self.handle_link_open(tag);
        } else if tag == "</a>" {
            self.handle_link_close();
        } else if tag.starts_with("<menu ") {
            self.handle_menu_open(tag);
        } else if tag == "</menu>" {
            self.handle_menu_close(elements);
        } else if tag.starts_with("<mi ") {
            self.handle_menu_item(tag);
        } else if tag.starts_with("<LaunchURL ") {
            self.handle_launch_url(tag, elements);
        }
        // Handle paired inv tags: <inv id='X'>content</inv>
        else if tag.starts_with("<inv ") && tag.contains("</inv>") {
            self.handle_inv_paired(tag, elements);
        }
        // Handle container tags
        else if tag.starts_with("<container ") {
            tracing::info!("Parser: Matched container tag: {}", &tag[..tag.len().min(100)]);
            self.handle_container(tag, elements);
        } else if tag.starts_with("<clearContainer ") {
            self.handle_clear_container(tag, elements);
        }
        // Handle dropDownBox for target list
        else if tag.starts_with("<dropDownBox ") {
            tracing::debug!("Parser: Matched dropDownBox tag: {}", &tag[..tag.len().min(100)]);
            self.handle_dropdown(tag, elements);
        }
        // Debug: catch any dropdown-related tags we might be missing
        else if tag.to_lowercase().contains("dropdown") || tag.contains("dDB") {
            tracing::warn!("Parser: Unhandled dropdown-like tag: {}", &tag[..tag.len().min(100)]);
        }
        // Silently ignore these tags
        else if tag.starts_with("<compDef ")
            || tag == "</compDef>"
            || tag.starts_with("<streamWindow ")
            || tag.starts_with("<skin ")
            || tag.starts_with("<exposeContainer ")
        {
            // Ignore these (UI layout tags)
        }
    }

    fn handle_preset_open(&mut self, tag: &str) {
        // <preset id='speech'>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            // Track preset ID for semantic type detection
            self.current_preset_id = Some(id.clone());

            if let Some((fg, bg)) = self.presets.get(&id) {
                self.preset_stack.push(ColorStyle {
                    fg: fg.clone(),
                    bg: bg.clone(),
                    bold: false,
                });
            } else {
                self.preset_stack.push(ColorStyle::default());
            }
        }
    }

    fn handle_preset_close(&mut self) {
        self.preset_stack.pop();
        // Clear preset ID when closing
        self.current_preset_id = None;
    }

    fn handle_color_open(&mut self, tag: &str) {
        // <color fg='#FFFFFF' bg='#000000'>
        let fg = Self::extract_attribute(tag, "fg");
        let bg = Self::extract_attribute(tag, "bg");

        self.color_stack.push(ColorStyle {
            fg,
            bg,
            bold: false,
        });
    }

    fn handle_color_close(&mut self) {
        self.color_stack.pop();
    }

    fn handle_style(&mut self, tag: &str) {
        // <style id='roomName'>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            if id.is_empty() {
                self.style_stack.clear();
            } else if let Some((fg, bg)) = self.presets.get(&id) {
                self.style_stack.push(ColorStyle {
                    fg: fg.clone(),
                    bg: bg.clone(),
                    bold: false,
                });
            }
        }
    }

    fn handle_push_stream(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <pushStream id='speech'/> or <component id='room objs'/>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            self.current_stream = id.clone();
            elements.push(ParsedElement::StreamPush { id });
        }
    }

    fn handle_clear_stream(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <clearStream id='room'/>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            elements.push(ParsedElement::ClearStream { id });
        }
    }

    fn handle_prompt(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <prompt time="1234567890">&gt;</prompt>
        // Extract time and text content
        if let Some(time) = Self::extract_attribute(tag, "time") {
            // Extract text between tags (e.g., "&gt;")
            let text = if let Some(start) = tag.find('>') {
                if let Some(end) = tag.rfind("</prompt>") {
                    tag[start + 1..end].to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            elements.push(ParsedElement::Prompt {
                time,
                text: self.decode_entities(&text),
            });
        }
    }

    fn handle_spell(
        &mut self,
        whole_tag: &str,
        _text_buffer: &mut String,
        elements: &mut Vec<ParsedElement>,
    ) {
        // <spell>text</spell> or <spell exist="...">text</spell>
        // Extract text content between tags
        if let Some(start) = whole_tag.find('>') {
            if let Some(end) = whole_tag.rfind("</spell>") {
                let text = whole_tag[start + 1..end].to_string();
                elements.push(ParsedElement::Spell { text: text.clone() });
                // Also emit SpellHand for the hands widget
                elements.push(ParsedElement::SpellHand { spell: text });
            }
        }
    }

    fn handle_left_hand(
        &mut self,
        whole_tag: &str,
        _text_buffer: &mut String,
        elements: &mut Vec<ParsedElement>,
    ) {
        // <left>text</left> or <left exist="...">text</left>
        if let Some(start) = whole_tag.find('>') {
            if let Some(end) = whole_tag.rfind("</left>") {
                let item = whole_tag[start + 1..end].to_string();
                let link = Self::extract_attribute(whole_tag, "exist")
                    .zip(Self::extract_attribute(whole_tag, "noun"))
                    .map(|(exist, noun)| LinkData {
                        exist_id: exist,
                        noun,
                        text: item.clone(),
                        coord: Self::extract_attribute(whole_tag, "coord"),
                    });
                elements.push(ParsedElement::LeftHand { item, link });
            }
        }
    }

    fn handle_right_hand(
        &mut self,
        whole_tag: &str,
        _text_buffer: &mut String,
        elements: &mut Vec<ParsedElement>,
    ) {
        // <right>text</right> or <right exist="...">text</right>
        if let Some(start) = whole_tag.find('>') {
            if let Some(end) = whole_tag.rfind("</right>") {
                let item = whole_tag[start + 1..end].to_string();
                let link = Self::extract_attribute(whole_tag, "exist")
                    .zip(Self::extract_attribute(whole_tag, "noun"))
                    .map(|(exist, noun)| LinkData {
                        exist_id: exist,
                        noun,
                        text: item.clone(),
                        coord: Self::extract_attribute(whole_tag, "coord"),
                    });
                elements.push(ParsedElement::RightHand { item, link });
            }
        }
    }

    fn handle_compass(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <compass><dir value="n"/><dir value="e"/>...</compass>
        // Debug: Log the full compass tag to check for unexpected content
        tracing::debug!("[COMPASS] Processing compass tag: '{}'", tag);

        // Extract all direction values
        static DIR_REGEX: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"<dir value="([^"]+)""#).expect("valid dir regex"));
        let directions: Vec<String> = DIR_REGEX
            .captures_iter(tag)
            .map(|cap| cap[1].to_string())
            .collect();

        tracing::debug!("[COMPASS] Extracted directions: {:?}", directions);
        elements.push(ParsedElement::Compass { directions });
    }

    fn handle_indicator(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <indicator id='IconHIDDEN' visible='y'/>
        // <indicator id='IconSTUNNED' visible='n'/>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            // Strip "Icon" prefix but preserve original casing of the remainder
            let status = id.strip_prefix("Icon").unwrap_or(&id).to_string();

            // Extract visible attribute ('y' or 'n')
            if let Some(visible) = Self::extract_attribute(tag, "visible") {
                let active = visible == "y";
                elements.push(ParsedElement::StatusIndicator { id: status, active });
            }
        }
    }

    fn handle_dialog_data(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <dialogData id='IconPOISONED' value='active'/>
        // <dialogData id='IconDISEASED' value='clear'/>
        // <dialogData id='IconBLEEDING' value='active'/>
        // <dialogData id='IconSTUNNED' value='clear'/>
        // <dialogData id='minivitals'><progressBar id='mana' value='94' text='mana 386/407' .../></dialogData>
        // <dialogData id='Buffs' clear='t'></dialogData>
        // <dialogData id='Buffs'><progressBar id='115' value='74' text="Fasthr's Reward" time='03:06:54'/></dialogData>
        // <dialogData id='injuries'><image id='head' name='Injury2' .../></dialogData>
        // <dialogData id='injuries' clear='t'></dialogData>
        // <dialogData id='MiniBounty' clear='t'></dialogData>

        let tag_head = match tag.find('>') {
            Some(idx) => &tag[..idx],
            None => tag,
        };
        if tag.contains("<cmdButton") || tag.contains("<closeButton") || tag.contains("<radio") {
            if let Some(id) = Self::extract_dialog_data_id(tag_head) {
                if !Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(tag_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let buttons = Self::parse_dialog_buttons(tag);
                    elements.push(ParsedElement::DialogButtons { id, clear, buttons });
                    return;
                }
            }
        }
        if tag.contains("<editBox") || tag.contains("<upDownEditBox") {
            if let Some(id) = Self::extract_dialog_data_id(tag_head) {
                if !Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(tag_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let (fields, labels) = Self::parse_dialog_fields(tag);
                    if !fields.is_empty() || !labels.is_empty() {
                        elements.push(ParsedElement::DialogFields {
                            id,
                            clear,
                            fields,
                            labels,
                        });
                        return;
                    }
                }
            }
        }
        if let Some(id) = Self::extract_attribute(tag_head, "id") {
            if Self::is_quickbar_id(&id) {
                let clear = Self::extract_attribute(tag_head, "clear")
                    .map(|value| {
                        matches!(value.as_str(), "t" | "true" | "1")
                            || value.eq_ignore_ascii_case("true")
                    })
                    .unwrap_or(false);
                let entries = Self::parse_quickbar_entries(tag);
                elements.push(ParsedElement::QuickbarEntries { id, clear, entries });
                return;
            }
            if id == "BetrayerPanel" {
                let clear = Self::extract_attribute(tag_head, "clear")
                    .map(|value| {
                        matches!(value.as_str(), "t" | "true" | "1")
                            || value.eq_ignore_ascii_case("true")
                    })
                    .unwrap_or(false);
                let (_, labels) = Self::parse_dialog_fields(tag);
                if clear || !labels.is_empty() {
                    elements.push(ParsedElement::DialogLabelList { id, clear, labels });
                    return;
                }
            }
            // Check for clear='t' attribute - emit ClearDialogData for generic windows
            // This handles clearing for windows like MiniBounty, and other text-based dialogData
            if let Some(clear) = Self::extract_attribute(tag_head, "clear") {
                if clear == "t" {
                    // For injuries and active effects, we have specialized handling below
                    // For everything else, emit a generic ClearDialogData event
                    if id != "injuries"
                        && id != "Active Spells"
                        && id != "Buffs"
                        && id != "Debuffs"
                        && id != "Cooldowns"
                    {
                        elements.push(ParsedElement::ClearDialogData { id: id.clone() });
                        // tracing::debug!("Clearing dialogData window: {}", id);
                    }
                }
            }
            // Handle Icon* status indicators (preserve casing after stripping prefix)
            if let Some(rest) = id.strip_prefix("Icon") {
                let status = rest.to_string();
                if let Some(value) = Self::extract_attribute(tag_head, "value") {
                    let active = value == "active";
                    elements.push(ParsedElement::StatusIndicator { id: status, active });
                }
            }

            // Handle injuries dialogData - extract all <image> tags for body parts
            if id == "injuries" {
                // tracing::debug!("Parser found dialogData for injuries");

                // Check for clear='t' attribute - this clears ALL injuries
                if let Some(clear) = Self::extract_attribute(tag_head, "clear") {
                    if clear == "t" {
                        // tracing::debug!("Clearing all injuries (clear='t')");
                        // Emit clear events for all body parts
                        let body_parts = vec![
                            "head",
                            "neck",
                            "chest",
                            "abdomen",
                            "back",
                            "leftArm",
                            "rightArm",
                            "leftHand",
                            "rightHand",
                            "leftLeg",
                            "rightLeg",
                            "leftEye",
                            "rightEye",
                            "nsys",
                        ];
                        for part in body_parts {
                            elements.push(ParsedElement::InjuryImage {
                                id: part.to_string(),
                                name: part.to_string(), // name == id means cleared
                            });
                        }
                        return;
                    }
                }

                // Extract all <image> tags for injuries
                let mut remaining = tag;
                let mut _count = 0;
                while let Some(img_start) = remaining.find("<image ") {
                    if let Some(img_end) = remaining[img_start..].find("/>") {
                        let img_tag = &remaining[img_start..img_start + img_end + 2];

                        // Extract id and name attributes from image tag
                        if let Some(body_id) = Self::extract_attribute(img_tag, "id") {
                            if let Some(name) = Self::extract_attribute(img_tag, "name") {
                                elements.push(ParsedElement::InjuryImage { id: body_id, name });
                                _count += 1;
                            }
                        }

                        remaining = &remaining[img_start + img_end + 2..];
                    } else {
                        break;
                    }
                }
                // tracing::debug!("Parsed {} injury image(s)", count);
                return;
            }

            // Handle injuries popup dialogData for OTHER players (id="injuries-PLAYERID")
            // This shows another player's injuries when you examine them
            if id.starts_with("injuries-") {
                tracing::debug!("Parser found dialogData for injuries popup: {}", id);

                // Check for clear='t' attribute
                let clear = Self::extract_attribute(tag_head, "clear")
                    .map(|v| v == "t")
                    .unwrap_or(false);

                if clear {
                    // Emit clear for popup
                    elements.push(ParsedElement::InjuryPopupData {
                        popup_id: id.clone(),
                        injuries: vec![],
                        clear: true,
                    });
                    return;
                }

                // Extract all <image> tags for injuries
                let mut injuries = Vec::new();
                let mut remaining = tag;
                while let Some(img_start) = remaining.find("<image ") {
                    if let Some(img_end) = remaining[img_start..].find("/>") {
                        let img_tag = &remaining[img_start..img_start + img_end + 2];

                        // Extract id (body part) and name (injury level) attributes
                        if let Some(body_id) = Self::extract_attribute(img_tag, "id") {
                            if let Some(name) = Self::extract_attribute(img_tag, "name") {
                                injuries.push((body_id, name));
                            }
                        }

                        remaining = &remaining[img_start + img_end + 2..];
                    } else {
                        break;
                    }
                }

                if !injuries.is_empty() || clear {
                    elements.push(ParsedElement::InjuryPopupData {
                        popup_id: id.clone(),
                        injuries,
                        clear: false,
                    });
                }
                return;
            }

            // Handle Active Effects (Active Spells, Buffs, Debuffs, Cooldowns)
            if id == "Active Spells" || id == "Buffs" || id == "Debuffs" || id == "Cooldowns" {
                // tracing::debug!("Parser found dialogData for active effects category: {}", id);

                // Normalize category name: "Active Spells" → "ActiveSpells" (remove space for consistency)
                let category = if id == "Active Spells" {
                    "ActiveSpells".to_string()
                } else {
                    id.clone()
                };

                // Check for clear='t' attribute
                if let Some(clear) = Self::extract_attribute(tag, "clear") {
                    if clear == "t" {
                        // tracing::debug!("Clearing active effects for category: {}", category);
                        elements.push(ParsedElement::ClearActiveEffects { category });
                        return;
                    }
                }

                // Extract all progressBar tags for this category
                let mut remaining = tag;
                let mut _count = 0;
                while let Some(pb_start) = remaining.find("<progressBar ") {
                    if let Some(pb_end) = remaining[pb_start..].find("/>") {
                        let pb_tag = &remaining[pb_start..pb_start + pb_end + 2];

                        // Extract attributes for active effect
                        if let (Some(effect_id), Some(value_str), Some(text), Some(time)) = (
                            Self::extract_attribute(pb_tag, "id"),
                            Self::extract_attribute(pb_tag, "value"),
                            Self::extract_attribute(pb_tag, "text"),
                            Self::extract_attribute(pb_tag, "time"),
                        ) {
                            if let Ok(value) = value_str.parse::<u32>() {
                                elements.push(ParsedElement::ActiveEffect {
                                    category: category.clone(),
                                    id: effect_id,
                                    value,
                                    text,
                                    time,
                                });
                                _count += 1;
                            }
                        }

                        remaining = &remaining[pb_start + pb_end + 2..];
                    } else {
                        break;
                    }
                }
                // tracing::debug!("Parsed {} active effect(s) for category {}", count, id);
                return;
            }
        }

        // Extract progressBar tags from within dialogData (for minivitals, etc.)
        if tag.contains("<progressBar ") {
            let mut remaining = tag;
            while let Some(pb_start) = remaining.find("<progressBar ") {
                if let Some(pb_end) = remaining[pb_start..].find("/>") {
                    let pb_tag = &remaining[pb_start..pb_start + pb_end + 2];
                    self.handle_progressbar(pb_tag, elements);
                    remaining = &remaining[pb_start + pb_end + 2..];
                } else {
                    break;
                }
            }
        }

        // Extract dropDownBox tags from within dialogData (for combat targets)
        // <dialogData id='combat'><dropDownBox id='dDBTarget' .../></dialogData>
        if tag.contains("<dropDownBox ") {
            if let Some(db_start) = tag.find("<dropDownBox ") {
                // Find the end of the dropDownBox tag (self-closing with />)
                if let Some(db_end) = tag[db_start..].find("/>") {
                    let db_tag = &tag[db_start..db_start + db_end + 2];
                    tracing::debug!(
                        "Parser: Found dropDownBox inside dialogData: {}",
                        &db_tag[..db_tag.len().min(80)]
                    );
                    self.handle_dropdown(db_tag, elements);
                }
            }
        }
    }

    fn handle_open_dialog(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        let tag_head = tag.split('>').next().unwrap_or(tag);

        // Check if this is a resident dialog (persistent panel, not a popup)
        let is_resident = Self::extract_attribute(tag_head, "resident")
            .map(|v| v == "true" || v == "t" || v == "1")
            .unwrap_or(false);

        // Check if position should be saved (save='t')
        let save_position = Self::extract_attribute(tag_head, "save")
            .map(|v| v == "true" || v == "t" || v == "1")
            .unwrap_or(false);

        if let Some(id) = Self::extract_attribute(tag_head, "id") {
            if Self::is_quickbar_id(&id) {
                let title = Self::extract_attribute(tag_head, "title")
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty());
                elements.push(ParsedElement::QuickbarOpen { id, title });
            } else if !is_resident {
                // Only emit DialogOpen for non-resident dialogs (popups)
                // Resident dialogs are persistent panels that should update widgets, not show popups
                let title = Self::extract_attribute(tag_head, "title")
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty());
                tracing::debug!("Parser emitting DialogOpen: id={}, title={:?}, save={}", id, title, save_position);
                elements.push(ParsedElement::DialogOpen { id, title, save: save_position });
            }
        }

        self.handle_embedded_quickbar_dialog_data(tag, elements);
        self.handle_embedded_dialog_buttons(tag, elements);
        self.handle_embedded_dialog_fields(tag, elements);

        // For resident dialogs, extract progressBar data for widget updates
        // For non-resident dialogs (popups), extract progressBar data for dialog rendering
        // Always call handle_embedded_resident_dialog_data to emit standalone ProgressBar/Label
        // elements for game state updates (needed for widgets like gs4_experience, encumbrance)
        self.handle_embedded_resident_dialog_data(tag, elements);
        if !is_resident {
            // Also extract for popup dialog rendering
            self.handle_embedded_dialog_progress_bars(tag, elements);
        }
    }

    /// Extract progressBar and other widget data from embedded dialogData in resident dialogs
    fn handle_embedded_resident_dialog_data(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        let mut remaining = tag;
        let end_pattern = "</dialogData>";

        while let Some(start) = remaining.find("<dialogData") {
            let Some(end_start) = remaining[start..].find(end_pattern) else {
                break;
            };
            let end = start + end_start + end_pattern.len();
            let dialog_tag = &remaining[start..end];

            // Extract progressBar elements
            if dialog_tag.contains("<progressBar ") {
                let mut pb_remaining = dialog_tag;
                while let Some(pb_start) = pb_remaining.find("<progressBar ") {
                    if let Some(pb_end) = pb_remaining[pb_start..].find("/>") {
                        let pb_tag = &pb_remaining[pb_start..pb_start + pb_end + 2];
                        self.handle_progressbar(pb_tag, elements);
                        pb_remaining = &pb_remaining[pb_start + pb_end + 2..];
                    } else {
                        break;
                    }
                }
            }

            // Extract label elements for widgets like encumbrance
            if dialog_tag.contains("<label ") {
                let mut label_remaining = dialog_tag;
                while let Some(label_start) = label_remaining.find("<label ") {
                    if let Some(label_end) = label_remaining[label_start..].find("/>") {
                        let label_tag = &label_remaining[label_start..label_start + label_end + 2];
                        self.handle_label(label_tag, elements);
                        label_remaining = &label_remaining[label_start + label_end + 2..];
                    } else {
                        break;
                    }
                }
            }

            remaining = &remaining[end..];
        }
    }

    fn handle_embedded_quickbar_dialog_data(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
        let mut remaining = tag;
        let end_pattern = "</dialogData>";

        while let Some(start) = remaining.find("<dialogData") {
            let Some(end_start) = remaining[start..].find(end_pattern) else {
                break;
            };
            let end = start + end_start + end_pattern.len();
            let dialog_tag = &remaining[start..end];

            let dialog_head = dialog_tag.split('>').next().unwrap_or(dialog_tag);
            if let Some(id) = Self::extract_attribute(dialog_head, "id") {
                if Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(dialog_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let entries = Self::parse_quickbar_entries(dialog_tag);
                    elements.push(ParsedElement::QuickbarEntries { id, clear, entries });
                }
            }

            remaining = &remaining[end..];
        }
    }

    fn handle_embedded_dialog_buttons(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
        let mut remaining = tag;
        let end_pattern = "</dialogData>";

        while let Some(start) = remaining.find("<dialogData") {
            let Some(end_start) = remaining[start..].find(end_pattern) else {
                break;
            };
            let end = start + end_start + end_pattern.len();
            let dialog_tag = &remaining[start..end];

            if !(dialog_tag.contains("<cmdButton")
                || dialog_tag.contains("<closeButton")
                || dialog_tag.contains("<radio"))
            {
                remaining = &remaining[end..];
                continue;
            }

            let dialog_head = dialog_tag.split('>').next().unwrap_or(dialog_tag);
            if let Some(id) = Self::extract_dialog_data_id(dialog_head) {
                if !Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(dialog_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let buttons = Self::parse_dialog_buttons(dialog_tag);
                    elements.push(ParsedElement::DialogButtons { id, clear, buttons });
                }
            }

            remaining = &remaining[end..];
        }
    }

    fn handle_embedded_dialog_fields(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
        let mut remaining = tag;
        let end_pattern = "</dialogData>";

        while let Some(start) = remaining.find("<dialogData") {
            let Some(end_start) = remaining[start..].find(end_pattern) else {
                break;
            };
            let end = start + end_start + end_pattern.len();
            let dialog_tag = &remaining[start..end];

            if !dialog_tag.contains("<editBox") && !dialog_tag.contains("<upDownEditBox") {
                remaining = &remaining[end..];
                continue;
            }

            let dialog_head = dialog_tag.split('>').next().unwrap_or(dialog_tag);
            if let Some(id) = Self::extract_dialog_data_id(dialog_head) {
                if !Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(dialog_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let (fields, labels) = Self::parse_dialog_fields(dialog_tag);
                    if !fields.is_empty() || !labels.is_empty() {
                        elements.push(ParsedElement::DialogFields {
                            id,
                            clear,
                            fields,
                            labels,
                        });
                    }
                }
            }

            remaining = &remaining[end..];
        }
    }

    /// Extract progressBar elements from embedded dialogData for non-resident dialogs (popups)
    fn handle_embedded_dialog_progress_bars(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
        let mut remaining = tag;
        let end_pattern = "</dialogData>";

        while let Some(start) = remaining.find("<dialogData") {
            let Some(end_start) = remaining[start..].find(end_pattern) else {
                break;
            };
            let end = start + end_start + end_pattern.len();
            let dialog_tag = &remaining[start..end];

            if !dialog_tag.contains("<progressBar ") {
                remaining = &remaining[end..];
                continue;
            }

            let dialog_head = dialog_tag.split('>').next().unwrap_or(dialog_tag);
            if let Some(id) = Self::extract_dialog_data_id(dialog_head) {
                if !Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(dialog_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let progress_bars = Self::parse_dialog_progress_bars(dialog_tag);
                    if !progress_bars.is_empty() {
                        elements.push(ParsedElement::DialogProgressBars {
                            id,
                            clear,
                            progress_bars,
                        });
                    }
                }
            }

            remaining = &remaining[end..];
        }
    }

    /// Parse progressBar elements from a dialog tag
    fn parse_dialog_progress_bars(tag: &str) -> Vec<DialogProgressBarSpec> {
        let mut progress_bars = Vec::new();
        let mut remaining = tag;

        while let Some(pb_start) = remaining.find("<progressBar ") {
            let pb_end = if let Some(end) = remaining[pb_start..].find("/>") {
                pb_start + end + 2
            } else if let Some(end) = remaining[pb_start..].find("</progressBar>") {
                pb_start + end + 14
            } else {
                break;
            };

            let pb_tag = &remaining[pb_start..pb_end];

            if let Some(id) = Self::extract_attribute(pb_tag, "id") {
                let value = Self::extract_attribute(pb_tag, "value")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
                let text = Self::extract_attribute(pb_tag, "text").unwrap_or_default();

                progress_bars.push(DialogProgressBarSpec { id, value, text });
            }

            remaining = &remaining[pb_end..];
        }

        progress_bars
    }

    fn handle_switch_quickbar(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        if let Some(id) = Self::extract_attribute(tag, "id") {
            if Self::is_quickbar_id(&id) {
                elements.push(ParsedElement::QuickbarSwitch { id });
            }
        }
    }

    fn handle_close_dialog(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        if let Some(id) = Self::extract_attribute(tag, "id") {
            elements.push(ParsedElement::CloseDialog { id });
        }
    }

    fn is_quickbar_id(id: &str) -> bool {
        id == "quick" || id.starts_with("quick-")
    }

    fn extract_dialog_data_id(tag_head: &str) -> Option<String> {
        Self::extract_attribute(tag_head, "id")
            .or_else(|| Self::extract_attribute(tag_head, "name"))
    }

    fn parse_quickbar_entries(tag: &str) -> Vec<QuickbarEntry> {
        let mut entries = Vec::new();
        let mut remaining = tag;

        loop {
            let label_pos = remaining.find("<label");
            let link_pos = remaining.find("<link");
            let menu_pos = remaining.find("<menuLink");
            let sep_pos = remaining.find("<sep");

            let mut next_pos = None;
            let mut kind = "";

            for (pos, label) in [
                (label_pos, "label"),
                (link_pos, "link"),
                (menu_pos, "menuLink"),
                (sep_pos, "sep"),
            ] {
                if let Some(pos) = pos {
                    if next_pos.map(|current| pos < current).unwrap_or(true) {
                        next_pos = Some(pos);
                        kind = label;
                    }
                }
            }

            let Some(pos) = next_pos else { break };
            remaining = &remaining[pos..];

            let (tag_slice, advance_by) = if let Some(end) = remaining.find("/>") {
                (&remaining[..end + 2], end + 2)
            } else if let Some(end) = remaining.find('>') {
                (&remaining[..end + 1], end + 1)
            } else {
                break;
            };

            if kind == "sep" {
                let value = Self::extract_attribute(tag_slice, "value").unwrap_or_default();
                if value.trim().is_empty() {
                    entries.push(QuickbarEntry::Separator);
                } else {
                    let id = Self::extract_attribute(tag_slice, "id").unwrap_or_default();
                    entries.push(QuickbarEntry::Label { id, value });
                }
            } else if kind == "label" {
                let id = Self::extract_attribute(tag_slice, "id").unwrap_or_default();
                let value = Self::extract_attribute(tag_slice, "value").unwrap_or_default();
                entries.push(QuickbarEntry::Label { id, value });
            } else if kind == "link" {
                let id = Self::extract_attribute(tag_slice, "id").unwrap_or_default();
                let value = Self::extract_attribute(tag_slice, "value").unwrap_or_default();
                let cmd = Self::extract_attribute(tag_slice, "cmd").unwrap_or_default();
                let echo = Self::extract_attribute(tag_slice, "echo");
                entries.push(QuickbarEntry::Link {
                    id,
                    value,
                    cmd,
                    echo,
                });
            } else if kind == "menuLink" {
                let id = Self::extract_attribute(tag_slice, "id").unwrap_or_default();
                let value = Self::extract_attribute(tag_slice, "value").unwrap_or_default();
                let exist = Self::extract_attribute(tag_slice, "exist").unwrap_or_default();
                let noun = Self::extract_attribute(tag_slice, "noun").unwrap_or_default();
                entries.push(QuickbarEntry::MenuLink {
                    id,
                    value,
                    exist,
                    noun,
                });
            }

            remaining = &remaining[advance_by..];
        }

        entries
    }

    fn parse_dialog_buttons(tag: &str) -> Vec<DialogButton> {
        let mut buttons = Vec::new();
        let mut remaining = tag;

        loop {
            let cmd_pos = remaining.find("<cmdButton");
            let close_pos = remaining.find("<closeButton");
            let radio_pos = remaining.find("<radio");
            let link_pos = remaining.find("<link");

            let mut next_pos = None;
            let mut kind = "";

            for (pos, label) in [
                (cmd_pos, "cmdButton"),
                (close_pos, "closeButton"),
                (radio_pos, "radio"),
                (link_pos, "link"),
            ] {
                if let Some(pos) = pos {
                    if next_pos.map(|current| pos < current).unwrap_or(true) {
                        next_pos = Some(pos);
                        kind = label;
                    }
                }
            }

            let Some(pos) = next_pos else { break };
            remaining = &remaining[pos..];

            let (tag_slice, advance_by) = if let Some(end) = remaining.find("/>") {
                (&remaining[..end + 2], end + 2)
            } else if let Some(end) = remaining.find('>') {
                (&remaining[..end + 1], end + 1)
            } else {
                break;
            };

            let id = Self::extract_attribute(tag_slice, "id").unwrap_or_default();
            let label = if kind == "radio" {
                Self::extract_attribute(tag_slice, "text").unwrap_or_else(|| id.clone())
            } else {
                Self::extract_attribute(tag_slice, "value").unwrap_or_else(|| id.clone())
            };
            let cmd = Self::extract_attribute(tag_slice, "cmd").unwrap_or_default();
            let is_close = kind == "closeButton" || cmd.trim().is_empty();
            let is_radio = kind == "radio";
            let selected = if is_radio {
                Self::extract_attribute(tag_slice, "value")
                    .map(|value| {
                        matches!(value.as_str(), "1" | "true" | "t")
                            || value.eq_ignore_ascii_case("true")
                    })
                    .unwrap_or(false)
            } else {
                false
            };
            let autosend = if is_radio {
                Self::extract_attribute(tag_slice, "autosend")
                    .map(|value| {
                        let trimmed = value.trim();
                        if trimmed.is_empty() {
                            true
                        } else {
                            !matches!(trimmed, "0" | "false" | "f")
                                && !trimmed.eq_ignore_ascii_case("false")
                        }
                    })
                    .unwrap_or(false)
            } else {
                false
            };
            let group = if is_radio {
                Self::extract_attribute(tag_slice, "group")
            } else {
                None
            };

            buttons.push(DialogButton {
                id,
                label,
                command: cmd,
                is_close,
                is_radio,
                selected,
                autosend,
                group,
            });

            remaining = &remaining[advance_by..];
        }

        buttons
    }

    fn parse_dialog_fields(tag: &str) -> (Vec<DialogFieldSpec>, Vec<DialogLabelSpec>) {
        let mut fields = Vec::new();
        let mut labels = Vec::new();
        let mut remaining = tag;

        loop {
            let edit_pos = remaining.find("<editBox");
            let updown_pos = remaining.find("<upDownEditBox");
            let label_pos = remaining.find("<label");

            let mut next_pos = None;
            let mut kind = "";

            for (pos, label) in [
                (edit_pos, "editBox"),
                (updown_pos, "upDownEditBox"),
                (label_pos, "label"),
            ] {
                if let Some(pos) = pos {
                    if next_pos.map(|current| pos < current).unwrap_or(true) {
                        next_pos = Some(pos);
                        kind = label;
                    }
                }
            }

            let Some(pos) = next_pos else { break };
            remaining = &remaining[pos..];

            let (tag_slice, advance_by) = if let Some(end) = remaining.find("/>") {
                (&remaining[..end + 2], end + 2)
            } else if let Some(end) = remaining.find('>') {
                (&remaining[..end + 1], end + 1)
            } else {
                break;
            };

            if kind == "editBox" || kind == "upDownEditBox" {
                let id = Self::extract_attribute(tag_slice, "id").unwrap_or_default();
                let value = Self::extract_attribute(tag_slice, "value").unwrap_or_default();
                let enter_button = Self::extract_attribute(tag_slice, "enterButton");
                let focused = Self::extract_attribute(tag_slice, "focus").is_some();

                fields.push(DialogFieldSpec {
                    id,
                    value,
                    enter_button,
                    focused,
                });
            } else if kind == "label" {
                let id = Self::extract_attribute(tag_slice, "id").unwrap_or_default();
                let value = Self::extract_attribute(tag_slice, "value").unwrap_or_default();
                let value = Self::sanitize_dialog_label(&value);
                labels.push(DialogLabelSpec { id, value });
            }

            remaining = &remaining[advance_by..];
        }

        (fields, labels)
    }

    fn sanitize_dialog_label(value: &str) -> String {
        let mut cleaned = value.to_string();
        if let Some(pos) = cleaned.find("&quot;") {
            cleaned.truncate(pos);
        }
        cleaned.trim().to_string()
    }

    fn handle_progressbar(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <progressBar id='health' value='100' text='health 175/175' />
        // <progressBar id='mindState' value='0' text='clear as a bell' />
        // Note: 'value' is percentage (0-100), not the actual current value
        if let Some(id) = Self::extract_attribute(tag, "id") {
            let percentage = Self::extract_attribute(tag, "value")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);
            let text = Self::extract_attribute(tag, "text").unwrap_or_default();

        // Try to extract current/max from text (format: "mana 407/407" or "175/175")
        // Also handle formats like "defensive (100%)" (label + current) and label-only strings.
        let (value, max) = parse_progress_numbers(&text, percentage);

        elements.push(ParsedElement::ProgressBar {
            id,
            value,
            max,
            text,
        });
    }

    }

    fn handle_label(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <label id='lblBPs' value='Blood Points: 100' />
        if let Some(id) = Self::extract_attribute(tag, "id") {
            if let Some(value) = Self::extract_attribute(tag, "value") {
                // Check if this is the Blood Points label - emit as ProgressBar instead
                if id == "lblBPs" && value.contains("Blood Points:") {
                    // Extract the number after "Blood Points: "
                    if let Some(bp_start) = value.find("Blood Points:") {
                        let after_bp = &value[bp_start + 14..].trim_start();
                        if let Some(end) = after_bp.find(|c: char| !c.is_ascii_digit()) {
                            let num_str = &after_bp[..end];
                            if let Ok(bp_value) = num_str.parse::<u32>() {
                                // Emit as ProgressBar so we can reuse the existing handler
                                elements.push(ParsedElement::ProgressBar {
                                    id: id.clone(),
                                    value: bp_value,
                                    max: 100,
                                    text: value.clone(),
                                });
                                return;
                            }
                        } else if let Ok(bp_value) = after_bp.parse::<u32>() {
                            // Emit as ProgressBar so we can reuse the existing handler
                            elements.push(ParsedElement::ProgressBar {
                                id: id.clone(),
                                value: bp_value,
                                max: 100,
                                text: value.clone(),
                            });
                            return;
                        }
                    }
                }

                // Otherwise just emit the label as-is
                elements.push(ParsedElement::Label { id, value });
            }
        }
    }

    fn handle_roundtime(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <roundTime value='5'/>
        if let Some(value_str) = Self::extract_attribute(tag, "value") {
            if let Ok(value) = value_str.parse::<u32>() {
                elements.push(ParsedElement::RoundTime { value });
            }
        }
    }

    fn handle_casttime(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <castTime value='3'/>
        if let Some(value_str) = Self::extract_attribute(tag, "value") {
            if let Ok(value) = value_str.parse::<u32>() {
                elements.push(ParsedElement::CastTime { value });
            }
        }
    }

    fn handle_nav(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <nav rm='7150105'/>
        // Extract room ID
        if let Some(id) = Self::extract_attribute(tag, "rm") {
            elements.push(ParsedElement::RoomId { id });
        }
    }

    fn handle_stream_window(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <streamWindow id='room' subtitle=" - Emberthorn Refuge, Bowery" ... />
        // Extract id and subtitle
        if let Some(id) = Self::extract_attribute(tag, "id") {
            let subtitle = Self::extract_attribute(tag, "subtitle");
            elements.push(ParsedElement::StreamWindow { id, subtitle });
        }
    }

    fn handle_dialogdata(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <dialogData id='BetrayerPanel'><label id='lblBPs' value='Blood Points: 100' ...
        // Extract blood points if present - emit as ProgressBar for consistency
        if tag.contains("id='BetrayerPanel'") || tag.contains("id=\"BetrayerPanel\"") {
            // Look for Blood Points label
            if let Some(bp_start) = tag.find("Blood Points:") {
                // Extract the number after "Blood Points: " (skip the colon and space = 14 chars)
                let after_bp = &tag[bp_start + 14..].trim_start();
                // Find the end of the number (first non-digit)
                if let Some(end) = after_bp.find(|c: char| !c.is_ascii_digit()) {
                    let num_str = &after_bp[..end];
                    if let Ok(value) = num_str.parse::<u32>() {
                        // Emit as ProgressBar so we can reuse the existing handler
                        elements.push(ParsedElement::ProgressBar {
                            id: "lblBPs".to_string(),
                            value,
                            max: 100,
                            text: format!("Blood Points: {}", value),
                        });
                    }
                } else {
                    // All remaining characters are digits
                    if let Ok(value) = after_bp.parse::<u32>() {
                        // Emit as ProgressBar so we can reuse the existing handler
                        elements.push(ParsedElement::ProgressBar {
                            id: "lblBPs".to_string(),
                            value,
                            max: 100,
                            text: format!("Blood Points: {}", value),
                        });
                    }
                }
            }
        }

        // Extract progressBar elements from dialogData
        // <dialogData id='minivitals'><progressBar id='mana' value='100' text='mana 414/414' ...
        if tag.contains("<progressBar ") {
            // Find all progressBar tags within this dialogData
            let mut remaining = tag;
            while let Some(pb_start) = remaining.find("<progressBar ") {
                if let Some(pb_end) = remaining[pb_start..].find("/>") {
                    let pb_tag = &remaining[pb_start..pb_start + pb_end + 2];
                    self.handle_progressbar(pb_tag, elements);
                    remaining = &remaining[pb_start + pb_end + 2..];
                } else {
                    break;
                }
            }
        }

        // Extract label elements from dialogData
        // <dialogData id='encum'>...<label id='encumblurb' value='You are not encumbered...' ...
        if tag.contains("<label ") {
            // Find all label tags within this dialogData
            let mut remaining = tag;
            while let Some(label_start) = remaining.find("<label ") {
                if let Some(label_end) = remaining[label_start..].find("/>") {
                    let label_tag = &remaining[label_start..label_start + label_end + 2];
                    // Extract id and value attributes
                    if let Some(id) = Self::extract_attribute(label_tag, "id") {
                        if let Some(value) = Self::extract_attribute(label_tag, "value") {
                            elements.push(ParsedElement::Label { id, value });
                        }
                    }
                    remaining = &remaining[label_start + label_end + 2..];
                } else {
                    break;
                }
            }
        }
    }

    fn handle_d_tag(&mut self, tag: &str) {
        // <d cmd='look' fg='#FFFFFF'>LOOK</d> - direct command tag
        // <d>SKILLS BASE</d> - direct command (uses text content as command)

        tracing::debug!(
            "handle_d_tag called: tag='{}', link_depth before={}",
            tag,
            self.link_depth
        );

        // Track link depth for semantic type (treat <d> like <a> for clickability)
        self.link_depth += 1;

        // Extract optional cmd attribute
        let cmd = Self::extract_attribute(tag, "cmd");

        // Create link data for this direct command
        // For <d>, we use a special exist_id to indicate it's a direct command
        self.current_link_data = Some(LinkData {
            exist_id: String::from("_direct_"), // Special marker for direct commands
            noun: cmd.clone().unwrap_or_default(), // Store cmd in noun field temporarily
            text: String::new(),                // Will be populated as text is rendered
            coord: None,                        // <d> tags don't use coords
        });

        // Don't apply color if we're inside monsterbold (bold has priority)
        if !self.bold_stack.is_empty() {
            return;
        }

        // Check if tag has explicit color attributes first
        let fg = Self::extract_attribute(tag, "fg");
        let bg = Self::extract_attribute(tag, "bg");

        if fg.is_some() || bg.is_some() {
            // Explicit colors
            self.color_stack.push(ColorStyle {
                fg,
                bg,
                bold: false,
            });
        } else {
            // Use commands preset (like links preset for <a> tags)
            if let Some((preset_fg, preset_bg)) = self.presets.get("commands") {
                self.color_stack.push(ColorStyle {
                    fg: preset_fg.clone(),
                    bg: preset_bg.clone(),
                    bold: false,
                });
            }
        }
    }

    fn handle_d_close(&mut self) {
        // Decrease link depth
        if self.link_depth > 0 {
            self.link_depth -= 1;
        }

        // For <d> tags without cmd attribute, populate noun from text content
        if self.link_depth == 0 {
            if let Some(ref mut link_data) = self.current_link_data {
                if link_data.noun.is_empty() && !link_data.text.is_empty() {
                    link_data.noun = link_data.text.clone();
                    tracing::debug!("Populated <d> tag noun from text: '{}'", link_data.noun);
                }
            }
        }

        // Clear link data when closing d tag
        if self.link_depth == 0 {
            self.current_link_data = None;
        }

        // Pop color if we added one
        if !self.color_stack.is_empty() {
            self.color_stack.pop();
        }
    }

    fn handle_link_open(&mut self, tag: &str) {
        // <a exist="..." noun="..." coord="..."> - apply links preset color and extract metadata
        // Track link depth for semantic type
        self.link_depth += 1;

        // Extract link metadata (exist_id, noun, and optional coord)
        let exist_id = Self::extract_attribute(tag, "exist");
        let noun = Self::extract_attribute(tag, "noun");
        let coord = Self::extract_attribute(tag, "coord");

        if let (Some(exist), Some(n)) = (exist_id, noun) {
            self.current_link_data = Some(LinkData {
                exist_id: exist,
                noun: n,
                text: String::new(), // Will be populated as text is rendered
                coord,               // Optional coord for direct commands
            });
        }

        // But don't apply color if we're inside monsterbold (bold has priority)
        if !self.bold_stack.is_empty() {
            return;
        }

        // Check if tag has explicit color attributes first
        let fg = Self::extract_attribute(tag, "fg");
        let bg = Self::extract_attribute(tag, "bg");

        if fg.is_some() || bg.is_some() {
            // Explicit colors
            self.color_stack.push(ColorStyle {
                fg,
                bg,
                bold: false,
            });
        } else {
            // Use links preset
            if let Some((preset_fg, preset_bg)) = self.presets.get("links") {
                self.color_stack.push(ColorStyle {
                    fg: preset_fg.clone(),
                    bg: preset_bg.clone(),
                    bold: false,
                });
            }
        }
    }

    fn handle_link_close(&mut self) {
        // Decrease link depth
        if self.link_depth > 0 {
            self.link_depth -= 1;
        }

        // Clear link data when closing link tag
        if self.link_depth == 0 {
            self.current_link_data = None;
        }

        // Only pop color if we're not inside monsterbold (matching handle_link_open behavior)
        if self.bold_stack.is_empty() && !self.color_stack.is_empty() {
            self.color_stack.pop();
        }
    }

    fn handle_menu_open(&mut self, tag: &str) {
        // <menu id="123" ...>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            // tracing::debug!("Starting menu collection for id={}", id);
            self.current_menu_id = Some(id);
            self.current_menu_coords.clear();
        } else {
            tracing::warn!("Menu tag missing id attribute: {}", tag);
        }
    }

    fn handle_menu_item(&mut self, tag: &str) {
        // <mi coord="2524,1898"/> or <mi coord="2524,1735" noun="gleaming steel baselard"/>
        // <mi text="chuckle" cmd="chuckle"/>
        if self.current_menu_id.is_some() {
            if let Some(coord) = Self::extract_attribute(tag, "coord") {
                let secondary_noun = Self::extract_attribute(tag, "noun");
                if let Some(ref _noun) = secondary_noun {
                    // tracing::debug!("Adding coord to menu: {} with secondary noun: {}", coord, noun);
                } else {
                    // tracing::debug!("Adding coord to menu: {}", coord);
                }
                self.current_menu_coords.push((coord, secondary_noun));
            } else if let Some(cmd) = Self::extract_attribute(tag, "cmd") {
                let text = Self::extract_attribute(tag, "text").or_else(|| {
                    let noun = Self::extract_attribute(tag, "noun");
                    noun.filter(|value| !value.trim().is_empty())
                });
                self.current_menu_coords
                    .push((format!("__direct__:{}", cmd), text));
            }
        }
    }

    fn handle_launch_url(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <LaunchURL src="/gs4/play/cm/loader.asp?uname=..."/>
        if let Some(src) = Self::extract_attribute(tag, "src") {
            tracing::info!("Parsed LaunchURL: src={}", src);
            elements.push(ParsedElement::LaunchURL { url: src });
        } else {
            tracing::warn!("LaunchURL tag without src attribute: {}", tag);
        }
    }

    fn handle_menu_close(&mut self, elements: &mut Vec<ParsedElement>) {
        // </menu>
        if let Some(id) = self.current_menu_id.take() {
            let coords = std::mem::take(&mut self.current_menu_coords);
            // tracing::debug!("Finished menu collection for id={}, {} coords", id, coords.len());

            elements.push(ParsedElement::MenuResponse { id, coords });
        }
    }

    fn handle_push_bold(&mut self) {
        // <pushBold/> - apply monsterbold preset and set bold
        self.bold_stack.push(true);

        // Apply monsterbold color preset
        if let Some((fg, bg)) = self.presets.get("monsterbold") {
            self.preset_stack.push(ColorStyle {
                fg: fg.clone(),
                bg: bg.clone(),
                bold: false,
            });
        }
    }

    fn handle_pop_bold(&mut self) {
        // <popBold/> - remove bold and color
        self.bold_stack.pop();

        // Remove monsterbold color if we added it
        if !self.preset_stack.is_empty() {
            self.preset_stack.pop();
        }
    }

    fn create_text_element(&mut self, content: String) -> ParsedElement {
        // Get current colors from stacks (last pushed takes precedence)
        let mut fg = None;
        let mut bg = None;
        let bold = !self.bold_stack.is_empty();

        // Check stacks in order: color > preset > style
        for style in &self.color_stack {
            if style.fg.is_some() {
                fg = style.fg.clone();
            }
            if style.bg.is_some() {
                bg = style.bg.clone();
            }
        }
        for style in &self.preset_stack {
            if fg.is_none() && style.fg.is_some() {
                fg = style.fg.clone();
            }
            if bg.is_none() && style.bg.is_some() {
                bg = style.bg.clone();
            }
        }
        for style in &self.style_stack {
            if fg.is_none() && style.fg.is_some() {
                fg = style.fg.clone();
            }
            if bg.is_none() && style.bg.is_some() {
                bg = style.bg.clone();
            }
        }

        // Decode HTML entities
        let content = self.decode_entities(&content);

        // If we're inside a link (<a> or <d> tag), append this text to the link's text field
        if self.link_depth > 0 {
            if let Some(ref mut link_data) = self.current_link_data {
                link_data.text.push_str(&content);
            }
        }

        // Determine semantic type based on current state
        // Priority: Monsterbold > Spell > Link > Speech > Normal
        let span_type = if !self.bold_stack.is_empty() {
            SpanType::Monsterbold
        } else if self.spell_depth > 0 {
            SpanType::Spell
        } else if self.link_depth > 0 {
            SpanType::Link
        } else if self.current_preset_id.as_deref() == Some("speech") {
            SpanType::Speech
        } else {
            SpanType::Normal
        };

        ParsedElement::Text {
            content,
            stream: self.current_stream.clone(),
            fg_color: fg,
            bg_color: bg,
            bold,
            span_type,
            link_data: self.current_link_data.clone(),
        }
    }

    fn decode_entities(&self, text: &str) -> String {
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
    }

    /// Flush text buffer and check for event patterns
    fn flush_text_with_events(&mut self, text: String, elements: &mut Vec<ParsedElement>) {
        if text.is_empty() {
            return;
        }

        // Check if we should auto-exit inventory stream
        // Inventory updates don't send <popStream/>, so we detect terminator lines
        if self.current_stream == "inv" {
            const INV_TERMINATORS: &[&str] = &[
                "You pick up",
                "You drop",
                "You retrieve",
                "You sheathe",
                "You draw",
                "You put",
            ];

            // Check if this line terminates the inventory stream
            for terminator in INV_TERMINATORS {
                if text.trim_start().starts_with(terminator) {
                    tracing::debug!(
                        "Detected inventory terminator: '{}' - switching to main stream",
                        terminator
                    );
                    self.current_stream = "main".to_string();
                    elements.push(ParsedElement::StreamPop);
                    break;
                }
            }
        }

        // Check for event patterns on the text
        let event_elements = self.check_event_patterns(&text);
        elements.extend(event_elements);

        // Add the text element itself
        elements.push(self.create_text_element(text));
    }

    /// Check text against event patterns and return any matching events
    fn check_event_patterns(&self, text: &str) -> Vec<ParsedElement> {
        let mut events = Vec::new();

        for (regex, pattern) in &self.event_matchers {
            if let Some(captures) = regex.captures(text) {
                let mut duration = pattern.duration;

                // Extract duration from capture group if specified
                if let Some(group_idx) = pattern.duration_capture {
                    if let Some(capture) = captures.get(group_idx) {
                        if let Ok(captured_value) = capture.as_str().parse::<f32>() {
                            // Apply multiplier (e.g., rounds to seconds)
                            duration = (captured_value * pattern.duration_multiplier) as u32;
                        }
                    }
                }

                // tracing::debug!(
                //                     "Event pattern '{}' matched: '{}' (duration: {}s)",
                //                     pattern.pattern,
                //                     text,
                //                     duration
                //                 );

                events.push(ParsedElement::Event {
                    event_type: pattern.event_type.clone(),
                    action: pattern.action.clone(),
                    duration,
                });
            }
        }

        events
    }

    /// Check if a line is purely a GSL protocol tag (should be skipped entirely)
    ///
    /// Returns true for lines like "GSjBCDFGH" (compass) that are GSL control messages
    fn is_gsl_tag_line(line: &str) -> bool {
        // Pattern: "GS" followed by a lowercase letter
        if line.starts_with("GS") && line.len() >= 3 {
            let third_char = line.chars().nth(2).unwrap_or(' ');
            return third_char.is_ascii_lowercase();
        }
        // Also check for lines starting with \x1C (control char prefix)
        if line.starts_with('\x1C') {
            return true;
        }
        false
    }

    /// Strip GSL (GemStone Language) protocol tags sent by Lich proxy
    ///
    /// Lich sends GSL control sequences for compass, status indicators, etc.
    /// These start with \x1C (File Separator) followed by "GS" + letter + data,
    /// OR appear as bare "GSx..." lines (where x is a letter like 'j' for compass)
    ///
    /// Examples:
    /// - "GSjBCDFGH" = compass directions (j=junctions, BCDFGH=encoded exits)
    /// - "GSg0000000050" = stance value
    /// - "GSP..." = prompt indicators
    /// - "\x1CGSB..." = character info with control char prefix
    fn strip_gsl_tags(line: &str) -> String {
        // Handle lines that are purely GSL tags (no leading \x1C in logs)
        // Pattern: "GS" followed by a lowercase letter, then optional data
        if line.starts_with("GS") && line.len() >= 3 {
            let third_char = line.chars().nth(2).unwrap_or(' ');
            if third_char.is_ascii_lowercase() {
                // This is a GSL tag line - filter it out entirely
                tracing::debug!("[GSL] Filtering GSL tag: '{}'", line);
                return String::new();
            }
        }

        // Handle embedded GSL tags with \x1C prefix
        // Strip anything from \x1C to end of line or next \x1C
        let mut result = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        let mut in_gsl_tag = false;

        while let Some(ch) = chars.next() {
            if ch == '\x1C' {
                // Start of GSL sequence - skip until end of tag
                in_gsl_tag = true;
                // Skip the "GS" + letter + data until we hit another control char or normal text
                // GSL tags typically end at newline, but we process line by line
                continue;
            }

            if in_gsl_tag {
                // We're in a GSL tag - check if this looks like normal text again
                // GSL tags are typically fixed format or end at specific delimiters
                // For safety, just skip the rest of the line after \x1C
                continue;
            }

            result.push(ch);
        }

        result
    }

    fn extract_attribute(tag: &str, attr: &str) -> Option<String> {
        // Extract attribute value from tag using simple string parsing
        // Much faster than regex compilation on every call
        // Handles both single and double quotes

        // Try double quotes: attr="value"
        let pattern_double = format!("{}=\"", attr);
        if let Some(start) = tag.find(&pattern_double) {
            let value_start = start + pattern_double.len();
            if let Some(end) = tag[value_start..].find('"') {
                return Some(tag[value_start..value_start + end].to_string());
            }
        }

        // Try single quotes: attr='value'
        let pattern_single = format!("{}='", attr);
        if let Some(start) = tag.find(&pattern_single) {
            let value_start = start + pattern_single.len();
            if let Some(end) = tag[value_start..].find('\'') {
                return Some(tag[value_start..value_start + end].to_string());
            }
        }

        None
    }

    // ==================== Container/Inventory Handlers ====================

    fn handle_inv_paired(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // Handle paired inv tag: <inv id='225766824'>content</inv>
        // Extract container ID and content, emit ContainerItem
        if let Some(id) = Self::extract_attribute(tag, "id") {
            // Extract content between <inv ...> and </inv>
            if let Some(start) = tag.find('>') {
                if let Some(end) = tag.rfind("</inv>") {
                    let content = tag[start + 1..end].to_string();
                    elements.push(ParsedElement::ContainerItem {
                        container_id: id,
                        content,
                    });
                }
            }
        }
    }

    fn handle_container(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <container id='225766824' title='Bandolier' target='#225766824' location='right'/>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            let title = Self::extract_attribute(tag, "title").unwrap_or_default();
            let target = Self::extract_attribute(tag, "target").unwrap_or_default();
            elements.push(ParsedElement::Container { id, title, target });
        }
    }

    fn handle_clear_container(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <clearContainer id="225766824"/>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            elements.push(ParsedElement::ClearContainer { id });
        }
    }

    // ==================== Target List Handler ====================

    fn handle_dropdown(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <dropDownBox id='dDBTarget' value="goblin" content_text="none,goblin,troll"
        //              content_value="target help,#123,#456" .../>
        // Only handle dDBTarget for target list - ignore other dropdowns
        if let Some(id) = Self::extract_attribute(tag, "id") {
            if id == "dDBTarget" {
                let current_target_name = Self::extract_attribute(tag, "value").unwrap_or_default();
                let content_text = Self::extract_attribute(tag, "content_text").unwrap_or_default();
                let content_value =
                    Self::extract_attribute(tag, "content_value").unwrap_or_default();

                // Split by comma to get lists
                let targets: Vec<String> =
                    content_text.split(',').map(|s| s.trim().to_string()).collect();
                let target_ids: Vec<String> =
                    content_value.split(',').map(|s| s.trim().to_string()).collect();

                // Find ID of current target by matching name to content_text
                // The first matching entry's corresponding ID is the current target
                // Only accept valid creature IDs (start with #), reject "target help" etc.
                let current_target = if !current_target_name.is_empty() {
                    targets
                        .iter()
                        .position(|name| name == &current_target_name)
                        .and_then(|idx| target_ids.get(idx))
                        .filter(|id| id.starts_with('#'))
                        .cloned()
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                tracing::debug!(
                    "Parser: dDBTarget dropdown received - current_name='{}', current_id='{}', {} targets, {} ids",
                    current_target_name,
                    current_target,
                    targets.len(),
                    target_ids.len()
                );

                elements.push(ParsedElement::TargetList {
                    current_target,
                    targets,
                    target_ids,
                });
            }
            // Other dropdowns (dDBStance, etc.) are silently ignored
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
        assert!(pb_elements.len() >= 1, "Should have at least one ProgressBar");

        // Find the mana progressbar
        let mana_pb = pb_elements.iter().find(|e| {
            if let ParsedElement::ProgressBar { id, .. } = e {
                id == "mana"
            } else {
                false
            }
        });

        assert!(mana_pb.is_some(), "Should have mana ProgressBar");
        if let Some(ParsedElement::ProgressBar { id, value, max, .. }) = mana_pb {
            assert_eq!(id, "mana");
            assert_eq!(*value, 414);
            assert_eq!(*max, 414);
        }
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

        let ParsedElement::StreamWindow { id, subtitle } = sw_elements[0] else {
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
}
