use regex::Regex;
use std::collections::HashMap;
use crate::ui::{SpanType, LinkData};
use crate::config::EventAction;

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
    },
    RightHand {
        item: String,
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
    RoomId {
        id: String,
    },
    StreamWindow {
        id: String,
        subtitle: Option<String>,
    },
    BloodPoints {
        value: u32,
    },
    InjuryImage {
        id: String,      // Body part: "head", "leftArm", etc.
        name: String,    // Injury level: "Injury1", "Injury2", "Injury3", "Scar1", "Scar2", "Scar3"
    },
    StatusIndicator {
        id: String,      // Status type: "poisoned", "diseased", "bleeding", "stunned"
        active: bool,    // true = active, false = clear
    },
    ActiveEffect {
        category: String,  // "ActiveSpells", "Buffs", "Debuffs", "Cooldowns"
        id: String,
        value: u32,
        text: String,
        time: String,      // Format: "HH:MM:SS"
    },
    ClearActiveEffects {
        category: String,  // Which category to clear
    },
    MenuResponse {
        id: String,                              // Correlation ID (counter)
        coords: Vec<(String, Option<String>)>,  // List of (coord, optional noun) pairs from <mi> tags
    },
    Event {
        event_type: String,  // "stun", "webbed", "prone", etc.
        action: EventAction, // Set/Clear/Increment
        duration: u32,       // Duration in seconds (for countdowns)
    },
    LaunchURL {
        url: String,  // URL path to append to https://www.play.net
    },
}

// Color/style tracking for nested tags
#[derive(Debug, Clone)]
pub(crate) struct ColorStyle {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
}

impl Default for ColorStyle {
    fn default() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
        }
    }
}

pub struct XmlParser {
    current_stream: String,
    presets: HashMap<String, (Option<String>, Option<String>)>, // id -> (fg, bg)

    // State tracking for nested tags
    pub(crate) color_stack: Vec<ColorStyle>,
    pub(crate) preset_stack: Vec<ColorStyle>,
    pub(crate) style_stack: Vec<ColorStyle>,
    pub(crate) bold_stack: Vec<bool>,

    // Semantic type tracking
    pub(crate) link_depth: usize,      // Track nested links
    pub(crate) spell_depth: usize,     // Track nested spells
    pub(crate) current_link_data: Option<LinkData>,  // Current link metadata (exist_id, noun)
    // Menu tracking
    current_menu_id: Option<String>,  // ID of menu being parsed
    current_menu_coords: Vec<(String, Option<String>)>, // (coord, optional noun) pairs for current menu

    // Inventory tag tracking (to discard content)
    in_inv_tag: bool,  // True when inside <inv>...</inv> tags

    // Event pattern matching
    event_matchers: Vec<(Regex, crate::config::EventPattern)>,  // Compiled regexes + patterns
}

impl XmlParser {
    pub fn new() -> Self {
        Self::with_presets(vec![], HashMap::new())
    }

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
                    tracing::warn!(
                        "Invalid event pattern '{}': {}",
                        name,
                        e
                    );
                }
            }
        }

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
            current_menu_id: None,
            current_menu_coords: Vec::new(),
            in_inv_tag: false,
            event_matchers,
        }
    }

    pub fn parse_line(&mut self, line: &str) -> Vec<ParsedElement> {
        let mut elements = Vec::new();
        let mut text_buffer = String::new();
        let mut remaining = line;

        while !remaining.is_empty() {
            // Check for paired tags first (manually check for each type)
            let mut found_paired = false;

            for tag_name in &["prompt", "spell", "left", "right", "compass", "dialogData", "component", "compDef"] {
                let start_pattern = format!("<{}", tag_name);
                let end_pattern = format!("</{}>", tag_name);

                if let Some(tag_start) = remaining.find(&start_pattern) {
                    // Make sure this is the earliest match
                    if remaining.find('<').map_or(false, |pos| pos < tag_start) {
                        continue;
                    }

                    // Find the closing tag
                    if let Some(tag_end_start) = remaining[tag_start..].find(&end_pattern) {
                        let tag_end = tag_start + tag_end_start + end_pattern.len();

                        // Add text before the paired tag (unless inside inv tag)
                        if tag_start > 0 && !self.in_inv_tag {
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
                // Add text before tag to buffer (unless we're inside an inv tag)
                if tag_start > 0 && !self.in_inv_tag {
                    text_buffer.push_str(&remaining[..tag_start]);
                }

                // Find tag end
                if let Some(tag_end) = remaining[tag_start..].find('>') {
                    let tag = &remaining[tag_start..tag_start + tag_end + 1];

                    // Process the tag (may flush buffer)
                    self.process_tag(tag, &mut text_buffer, &mut elements);

                    remaining = &remaining[tag_start + tag_end + 1..];
                } else {
                    // No closing >, treat rest as text (unless inside inv tag)
                    if !self.in_inv_tag {
                        text_buffer.push_str(remaining);
                    }
                    break;
                }
            } else {
                // No more tags, add remaining as text (unless inside inv tag)
                if !self.in_inv_tag {
                    text_buffer.push_str(remaining);
                }
                break;
            }
        }

        // Flush any remaining text
        self.flush_text_with_events(text_buffer, &mut elements);

        elements
    }

    fn process_tag(&mut self, tag: &str, text_buffer: &mut String, elements: &mut Vec<ParsedElement>) {
        // Determine if this tag changes color state
        let color_opening = tag.starts_with("<preset ") || tag.starts_with("<color ") ||
                           tag.starts_with("<style ") || tag.starts_with("<pushBold") ||
                           tag.starts_with("<b>") ||
                           tag.starts_with("<a ") || tag.starts_with("<d ");

        let color_closing = tag == "</preset>" || tag == "</color>" || tag == "</a>" ||
                           tag == "</d>" || tag == "<popBold/>" || tag == "</b>";

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
        } else if tag.starts_with("<pushStream ") {
            if !text_buffer.is_empty() {
                self.flush_text_with_events(text_buffer.clone(), elements);
                text_buffer.clear();
            }
            self.handle_push_stream(tag, elements);
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
        // Handle inventory tags - need to discard content between <inv> and </inv>
        else if tag.starts_with("<inv ") {
            // Flush text before entering inv tag
            if !text_buffer.is_empty() {
                self.flush_text_with_events(text_buffer.clone(), elements);
                text_buffer.clear();
            }
            // Set flag to discard content
            self.in_inv_tag = true;
        } else if tag == "</inv>" {
            // Clear text buffer (discard inv content) and clear flag
            text_buffer.clear();
            self.in_inv_tag = false;
        }
        // Silently ignore these tags
        else if tag.starts_with("<compDef ") || tag == "</compDef>" ||
                tag.starts_with("<streamWindow ") || tag.starts_with("<dropDownBox ") ||
                tag.starts_with("<skin ") ||
                tag.starts_with("<clearContainer ") ||
                tag.starts_with("<container ") || tag.starts_with("<exposeContainer ") {
            // Ignore these entirely (inventory window tags)
        }
    }

    fn handle_preset_open(&mut self, tag: &str) {
        // <preset id='speech'>
        if let Some(id) = Self::extract_attribute(tag, "id") {
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
            elements.push(ParsedElement::Prompt { time, text: self.decode_entities(&text) });
        }
    }

    fn handle_spell(&mut self, whole_tag: &str, _text_buffer: &mut String, elements: &mut Vec<ParsedElement>) {
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

    fn handle_left_hand(&mut self, whole_tag: &str, _text_buffer: &mut String, elements: &mut Vec<ParsedElement>) {
        // <left>text</left> or <left exist="...">text</left>
        if let Some(start) = whole_tag.find('>') {
            if let Some(end) = whole_tag.rfind("</left>") {
                let item = whole_tag[start + 1..end].to_string();
                elements.push(ParsedElement::LeftHand { item });
            }
        }
    }

    fn handle_right_hand(&mut self, whole_tag: &str, _text_buffer: &mut String, elements: &mut Vec<ParsedElement>) {
        // <right>text</right> or <right exist="...">text</right>
        if let Some(start) = whole_tag.find('>') {
            if let Some(end) = whole_tag.rfind("</right>") {
                let item = whole_tag[start + 1..end].to_string();
                elements.push(ParsedElement::RightHand { item });
            }
        }
    }

    fn handle_compass(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <compass><dir value="n"/><dir value="e"/>...</compass>
        // Extract all direction values
        let dir_regex = Regex::new(r#"<dir value="([^"]+)""#).unwrap();
        let directions: Vec<String> = dir_regex
            .captures_iter(tag)
            .map(|cap| cap[1].to_string())
            .collect();
        elements.push(ParsedElement::Compass { directions });
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

        if let Some(id) = Self::extract_attribute(tag, "id") {
            // Handle Icon* status indicators
            if id.starts_with("Icon") {
                let status = id.strip_prefix("Icon").unwrap_or(&id).to_lowercase();
                if let Some(value) = Self::extract_attribute(tag, "value") {
                    let active = value == "active";
                    elements.push(ParsedElement::StatusIndicator { id: status, active });
                }
            }

            // Handle injuries dialogData - extract all <image> tags for body parts
            if id == "injuries" {
                tracing::debug!("Parser found dialogData for injuries");

                // Check for clear='t' attribute - this clears ALL injuries
                if let Some(clear) = Self::extract_attribute(tag, "clear") {
                    if clear == "t" {
                        tracing::debug!("Clearing all injuries (clear='t')");
                        // Emit clear events for all body parts
                        let body_parts = vec![
                            "head", "neck", "chest", "abdomen", "back",
                            "leftArm", "rightArm", "leftHand", "rightHand",
                            "leftLeg", "rightLeg", "leftEye", "rightEye", "nsys"
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
                let mut count = 0;
                while let Some(img_start) = remaining.find("<image ") {
                    if let Some(img_end) = remaining[img_start..].find("/>") {
                        let img_tag = &remaining[img_start..img_start + img_end + 2];

                        // Extract id and name attributes from image tag
                        if let Some(body_id) = Self::extract_attribute(img_tag, "id") {
                            if let Some(name) = Self::extract_attribute(img_tag, "name") {
                                elements.push(ParsedElement::InjuryImage {
                                    id: body_id,
                                    name,
                                });
                                count += 1;
                            }
                        }

                        remaining = &remaining[img_start + img_end + 2..];
                    } else {
                        break;
                    }
                }
                tracing::debug!("Parsed {} injury image(s)", count);
                return;
            }

            // Handle Active Effects (Active Spells, Buffs, Debuffs, Cooldowns)
            if id == "Active Spells" || id == "Buffs" || id == "Debuffs" || id == "Cooldowns" {
                tracing::debug!("Parser found dialogData for active effects category: {}", id);

                // Normalize category name: "Active Spells" → "ActiveSpells" (remove space for consistency)
                let category = if id == "Active Spells" {
                    "ActiveSpells".to_string()
                } else {
                    id.clone()
                };

                // Check for clear='t' attribute
                if let Some(clear) = Self::extract_attribute(tag, "clear") {
                    if clear == "t" {
                        tracing::debug!("Clearing active effects for category: {}", category);
                        elements.push(ParsedElement::ClearActiveEffects {
                            category
                        });
                        return;
                    }
                }

                // Extract all progressBar tags for this category
                let mut remaining = tag;
                let mut count = 0;
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
                                count += 1;
                            }
                        }

                        remaining = &remaining[pb_start + pb_end + 2..];
                    } else {
                        break;
                    }
                }
                tracing::debug!("Parsed {} active effect(s) for category {}", count, id);
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
            let (value, max) = if let Some(slash_pos) = text.rfind('/') {
                // Find the number before the slash
                let before_slash = &text[..slash_pos];
                // Extract the last number before the slash (current value)
                let current = before_slash.split_whitespace()
                    .rev()
                    .find_map(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
                    .unwrap_or(percentage);

                // Extract the number after the slash (max value)
                let after_slash = &text[slash_pos + 1..];
                let maximum = after_slash.split_whitespace()
                    .find_map(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
                    .unwrap_or(100);

                (current, maximum)
            } else {
                // No slash found - use percentage as value, 100 as max
                (percentage, 100)
            };

            elements.push(ParsedElement::ProgressBar { id, value, max, text });
        }
    }

    fn handle_label(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <label id='lblBPs' value='Blood Points: 100' />
        if let Some(id) = Self::extract_attribute(tag, "id") {
            if let Some(value) = Self::extract_attribute(tag, "value") {
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
        // Extract blood points if present
        if tag.contains("id='BetrayerPanel'") || tag.contains("id=\"BetrayerPanel\"") {
            // Look for Blood Points label
            if let Some(bp_start) = tag.find("Blood Points:") {
                // Extract the number after "Blood Points: " (skip the colon and space = 14 chars)
                let after_bp = &tag[bp_start + 14..].trim_start();
                // Find the end of the number (first non-digit)
                if let Some(end) = after_bp.find(|c: char| !c.is_ascii_digit()) {
                    let num_str = &after_bp[..end];
                    if let Ok(value) = num_str.parse::<u32>() {
                        elements.push(ParsedElement::BloodPoints { value });
                    }
                } else {
                    // All remaining characters are digits
                    if let Ok(value) = after_bp.parse::<u32>() {
                        elements.push(ParsedElement::BloodPoints { value });
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
    }

    fn handle_d_tag(&mut self, tag: &str) {
        // <d cmd='look' fg='#FFFFFF'>LOOK</d> - direct command tag
        // <d>SKILLS BASE</d> - direct command (uses text content as command)

        // Track link depth for semantic type (treat <d> like <a> for clickability)
        self.link_depth += 1;

        // Extract optional cmd attribute
        let cmd = Self::extract_attribute(tag, "cmd");

        // Create link data for this direct command
        // For <d>, we use a special exist_id to indicate it's a direct command
        self.current_link_data = Some(LinkData {
            exist_id: String::from("_direct_"),  // Special marker for direct commands
            noun: cmd.clone().unwrap_or_default(),  // Store cmd in noun field temporarily
            text: String::new(),  // Will be populated as text is rendered
            coord: None,  // <d> tags don't use coords
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
                text: String::new(),  // Will be populated as text is rendered
                coord,  // Optional coord for direct commands
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
            tracing::debug!("Starting menu collection for id={}", id);
            self.current_menu_id = Some(id);
            self.current_menu_coords.clear();
        } else {
            tracing::warn!("Menu tag missing id attribute: {}", tag);
        }
    }

    fn handle_menu_item(&mut self, tag: &str) {
        // <mi coord="2524,1898"/> or <mi coord="2524,1735" noun="gleaming steel baselard"/>
        if self.current_menu_id.is_some() {
            if let Some(coord) = Self::extract_attribute(tag, "coord") {
                let secondary_noun = Self::extract_attribute(tag, "noun");
                if let Some(ref noun) = secondary_noun {
                    tracing::debug!("Adding coord to menu: {} with secondary noun: {}", coord, noun);
                } else {
                    tracing::debug!("Adding coord to menu: {}", coord);
                }
                self.current_menu_coords.push((coord, secondary_noun));
            }
        }
    }

    fn handle_launch_url(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <LaunchURL src="/gs4/play/cm/loader.asp?uname=..."/>
        if let Some(src) = Self::extract_attribute(tag, "src") {
            tracing::debug!("Parsed LaunchURL: src={}", src);
            elements.push(ParsedElement::LaunchURL { url: src });
        }
    }

    fn handle_menu_close(&mut self, elements: &mut Vec<ParsedElement>) {
        // </menu>
        if let Some(id) = self.current_menu_id.take() {
            let coords = std::mem::take(&mut self.current_menu_coords);
            tracing::debug!("Finished menu collection for id={}, {} coords", id, coords.len());

            elements.push(ParsedElement::MenuResponse {
                id,
                coords,
            });
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
            if style.fg.is_some() { fg = style.fg.clone(); }
            if style.bg.is_some() { bg = style.bg.clone(); }
        }
        for style in &self.preset_stack {
            if fg.is_none() && style.fg.is_some() { fg = style.fg.clone(); }
            if bg.is_none() && style.bg.is_some() { bg = style.bg.clone(); }
        }
        for style in &self.style_stack {
            if fg.is_none() && style.fg.is_some() { fg = style.fg.clone(); }
            if bg.is_none() && style.bg.is_some() { bg = style.bg.clone(); }
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
        // Priority: Monsterbold > Spell > Link > Normal
        let span_type = if !self.bold_stack.is_empty() {
            SpanType::Monsterbold
        } else if self.spell_depth > 0 {
            SpanType::Spell
        } else if self.link_depth > 0 {
            SpanType::Link
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

                tracing::debug!(
                    "Event pattern '{}' matched: '{}' (duration: {}s)",
                    pattern.pattern,
                    text,
                    duration
                );

                events.push(ParsedElement::Event {
                    event_type: pattern.event_type.clone(),
                    action: pattern.action.clone(),
                    duration,
                });
            }
        }

        events
    }

    fn extract_attribute(tag: &str, attr: &str) -> Option<String> {
        // Extract attribute value from tag
        // Handles both single and double quotes
        let pattern_double = format!(r#"{}="([^"]*)""#, attr);
        let pattern_single = format!(r#"{}='([^']*)'"#, attr);

        if let Ok(re) = Regex::new(&pattern_double) {
            if let Some(caps) = re.captures(tag) {
                return Some(caps[1].to_string());
            }
        }

        if let Ok(re) = Regex::new(&pattern_single) {
            if let Some(caps) = re.captures(tag) {
                return Some(caps[1].to_string());
            }
        }

        None
    }
}

impl Default for XmlParser {
    fn default() -> Self {
        Self::new()
    }
}
