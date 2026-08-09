//! Per-tag handlers for the simpler wire tags: expose/window hints,
//! presets, colors, styles, streams, prompt, spell, hands, compass,
//! indicators, progress bars, labels, RT/CT, vellum extensions, nav,
//! app, streamWindow, crtrStatus, roommeta, inventory, containers, and
//! combat dropdowns.

use super::*;

impl XmlParser {
    /// The expose verbs: `<exposeDialog id='bank'/>` and kin — the game
    /// (or a lich script) saying "show this window NOW".
    pub(super) fn handle_expose(&mut self, tag: &str, elements: &mut Vec<ParsedElement>, kind: &str) {
        let id = Self::extract_attribute(tag, "id")
            .or_else(|| Self::extract_attribute(tag, "name"));
        if let Some(id) = id {
            elements.push(ParsedElement::Expose { kind: kind.to_string(), id });
        }
    }

    /// Collect the placement/persistence attributes a window-declaring tag
    /// carries (previously extracted-and-dropped) into a raw WindowHints
    /// element beside the declaration. Only attributes actually present
    /// are emitted; nothing is emitted when none are.
    pub(super) fn emit_window_hints(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        const HINT_ATTRS: &[&str] = &[
            "location", "resident", "save", "scroll", "ifClosed", "appearance",
            "target", "width", "height", "x", "y", "noResize", "noDock",
            // gswiki Wrayth-protocol page: streamWindow also carries a
            // per-window timestamp toggle (wiki-attested; never appeared
            // in the 11.4 GB log sweep).
            "timestamp",
        ];
        // The DECLARING element's attributes only: a paired openDialog
        // block carries its inner dialogData controls in the same string,
        // and their width/height (double-quoted on the wire, vs the
        // openDialog's single quotes) must never shadow the declaration's
        // own (found live: bank's declared 0x130 came out as the balance
        // label's 190x20).
        let head = match tag.find('>') {
            Some(end) => &tag[..end],
            None => tag,
        };
        let Some(id) = Self::extract_attribute(head, "id")
            .or_else(|| Self::extract_attribute(head, "name"))
        else {
            return;
        };
        let attrs: Vec<(String, String)> = HINT_ATTRS
            .iter()
            .filter_map(|name| {
                Self::extract_attribute(head, name).map(|value| (name.to_string(), value))
            })
            .collect();
        if !attrs.is_empty() {
            elements.push(ParsedElement::WindowHints { id, attrs });
        }
    }

    pub(super) fn handle_preset_open(&mut self, tag: &str) {
        // <preset id='speech'>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            // Track preset ID for semantic type detection
            self.current_preset_id = Some(id.clone());

            if let Some((fg, bg)) = self.presets.get(&id) {
                self.preset_stack.push(ColorStyle {
                    fg: fg.clone(),
                    bg: bg.clone(),
                });
            } else {
                self.preset_stack.push(ColorStyle::default());
            }
        }
    }

    pub(super) fn handle_preset_close(&mut self) {
        self.preset_stack.pop();
        // Clear preset ID when closing
        self.current_preset_id = None;
    }

    pub(super) fn handle_color_open(&mut self, tag: &str) {
        // <color fg='#FFFFFF' bg='#000000'>
        let fg = Self::extract_attribute(tag, "fg");
        let bg = Self::extract_attribute(tag, "bg");

        self.color_stack.push(ColorStyle {
            fg,
            bg,
        });
    }

    pub(super) fn handle_color_close(&mut self) {
        self.color_stack.pop();
    }

    pub(super) fn handle_style(&mut self, tag: &str) {
        // <style id='roomName'>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            if id.is_empty() {
                self.style_stack.clear();
            } else if let Some((fg, bg)) = self.presets.get(&id) {
                self.style_stack.push(ColorStyle {
                    fg: fg.clone(),
                    bg: bg.clone(),
                });
            }
        }
    }

    pub(super) fn handle_push_stream(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <pushStream id='speech'/> or <component id='room objs'/>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            self.current_stream = id.clone();
            elements.push(ParsedElement::StreamPush { id });
        }
    }

    pub(super) fn handle_clear_stream(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <clearStream id='room'/>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            elements.push(ParsedElement::ClearStream { id });
        }
    }

    pub(super) fn handle_prompt(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <prompt time="1234567890">&gt;</prompt>
        //
        // A prompt marks the end of an input round. Well-formed traffic always
        // balances its bold/color/preset tags before the prompt, so anything
        // still open here is mangled server output — most visibly the daydream
        // stream, which emits a `<pushBold/>` whose matching `<popBold/>` is
        // dropped, leaking monsterbold onto every subsequent line. Reset the
        // transient style stacks at the prompt so a missing close can never
        // bleed past the current round. (This does NOT touch stream/mono
        // state, which spans prompts legitimately.)
        if !self.bold_stack.is_empty()
            || !self.preset_stack.is_empty()
            || !self.color_stack.is_empty()
            || !self.style_stack.is_empty()
        {
            tracing::debug!(
                "[parser] clearing {} bold / {} preset / {} color / {} style entries left open at prompt (mangled server markup)",
                self.bold_stack.len(),
                self.preset_stack.len(),
                self.color_stack.len(),
                self.style_stack.len(),
            );
            self.bold_stack.clear();
            self.preset_stack.clear();
            self.color_stack.clear();
            self.style_stack.clear();
            // color_stack was cleared, so the per-link "pushed a color" flags
            // are moot — drop them too so a later close doesn't act on stale
            // bookkeeping.
            self.link_pushed_color.clear();
            self.current_preset_id = None;
        }

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
                text: Self::decode_entities(text),
            });
        }
    }

    pub(super) fn handle_spell(
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

    pub(super) fn handle_left_hand(
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
                if link.is_none() && !item.is_empty() && item != "Empty" {
                    tracing::debug!("left hand tag without exist/noun: {}", whole_tag);
                }
                elements.push(ParsedElement::LeftHand { item, link });
            }
        }
    }

    pub(super) fn handle_right_hand(
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
                if link.is_none() && !item.is_empty() && item != "Empty" {
                    tracing::debug!("right hand tag without exist/noun: {}", whole_tag);
                }
                elements.push(ParsedElement::RightHand { item, link });
            }
        }
    }

    pub(super) fn handle_compass(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
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

    pub(super) fn handle_indicator(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
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

    pub(super) fn handle_progressbar(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
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

        let is_mind_state = id == "mindState";
        elements.push(ParsedElement::ProgressBar {
            id,
            value,
            max,
            text,
        });

        // The mindState bar also carries exact experience numbers and
        // event-bonus flags. Emitted unconditionally for mindState because
        // the bonus flags are snapshot-semantics: a bar without them means
        // the bonus ended.
        if is_mind_state {
            // Single attribute scan; exact-name lookup (extract_attribute's
            // substring probe would confuse "exp" with "field_exp")
            let attrs = Self::extract_all_attributes(tag);
            let get = |name: &str| attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
            let num = |name: &str| get(name).and_then(|v| v.parse::<u64>().ok());
            elements.push(ParsedElement::MindStateExp {
                field_exp: num("field_exp"),
                max_field_exp: num("max_field_exp"),
                exp: num("exp"),
                ascension_exp: num("ascension_exp"),
                until_next: num("until_next"),
                fashlonae: get("fashlonae").and_then(|v| v.parse::<u8>().ok()),
                lumnis: get("lumnis").and_then(|v| v.parse::<u8>().ok()),
                rpa: get("rpa").and_then(|v| v.parse::<f32>().ok()),
            });
        }
    }

    }

    pub(super) fn handle_label(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
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

    pub(super) fn handle_roundtime(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <roundTime value='5'/>
        if let Some(value_str) = Self::extract_attribute(tag, "value") {
            if let Ok(value) = value_str.parse::<u32>() {
                elements.push(ParsedElement::RoundTime { value });
            }
        }
    }

    pub(super) fn handle_casttime(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <castTime value='3'/>
        if let Some(value_str) = Self::extract_attribute(tag, "value") {
            if let Ok(value) = value_str.parse::<u32>() {
                elements.push(ParsedElement::CastTime { value });
            }
        }
    }

    pub(super) fn handle_vellum_timer(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <vellumTimer id='dark-cataclyst' value='1764904999'/> - script-
        // facing countdown feed (typically sent to the client by a Lich
        // script). value is the absolute epoch end time, like roundTime;
        // 0 clears. The tag never renders as text.
        if let (Some(id), Some(value_str)) = (
            Self::extract_attribute(tag, "id"),
            Self::extract_attribute(tag, "value"),
        ) {
            if id.is_empty() {
                return;
            }
            if let Ok(value) = value_str.parse::<i64>() {
                elements.push(ParsedElement::VellumTimer { id, value });
            }
        }
    }

    pub(super) fn handle_vellum_cmd(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <vellumCmd cmd=".rightbar off"/> (also accepted: <vellum-cmd ...>)
        // - script-facing client-command feed: Lich emits the tag, the game
        // never does. The message processor only honors dot-commands, so a
        // feed can toggle zones, hide windows, switch themes, etc., but can
        // never send outbound game commands. The tag never renders as text.
        if let Some(cmd) = Self::extract_attribute(tag, "cmd") {
            let command = cmd.trim().to_string();
            if !command.is_empty() {
                elements.push(ParsedElement::VellumCommand { command });
            }
        }
    }

    pub(super) fn handle_nav(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <nav rm='7150105'/>
        // Extract room ID
        if let Some(id) = Self::extract_attribute(tag, "rm") {
            elements.push(ParsedElement::RoomId { id });
        }
    }

    pub(super) fn handle_app(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <app char="Nisugi" game="GS" title="[GSIV: Nisugi]"/>
        // Sent at login; char is empty on logout screens - skip those.
        if let Some(character) = Self::extract_attribute(tag, "char") {
            if !character.trim().is_empty() {
                elements.push(ParsedElement::AppInfo {
                    character: Self::decode_entities(character),
                });
            }
        }
    }

    pub(super) fn handle_stream_window(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <streamWindow id='room' subtitle=" - Emberthorn Refuge, Bowery" ... />
        // Extract id and subtitle. Subtitles carry entity-escaped room
        // names (e.g. Scrivener&apos;s) - decode like text content.
        if let Some(id) = Self::extract_attribute(tag, "id") {
            // extract_attribute now entity-decodes, so no extra decode here
            // (double-decoding would collapse a literal `&amp;` in a title).
            let subtitle = Self::extract_attribute(tag, "subtitle");
            let title = Self::extract_attribute(tag, "title");
            elements.push(ParsedElement::StreamWindow {
                id,
                subtitle,
                title,
            });
        }
    }
    pub(super) fn handle_crtr_status(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <crtrStatus exist="607736" hostile="1" stunned="1"/> - self-closing,
        // self-contained snapshot; a missing or "0" flag means inactive
        if let Some(id) = Self::extract_attribute(tag, "exist") {
            let attrs = Self::extract_all_attributes(tag)
                .into_iter()
                .filter(|(name, _)| name != "exist")
                .collect();
            elements.push(ParsedElement::CreatureStatus { id, attrs });
        }
    }

    pub(super) fn handle_roommeta(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <roommeta climate="3" terrain="7" weather="0" .../> - self-closing
        // numeric-code room metadata; only known fields are sent each time
        let attrs = Self::extract_all_attributes(tag);
        if !attrs.is_empty() {
            elements.push(ParsedElement::RoomMeta { attrs });
        }
    }

    pub(super) fn extract_attribute(tag: &str, attr: &str) -> Option<String> {
        // Extract attribute value from tag using simple string parsing.
        // Handles both quote styles; double quotes keep precedence to match
        // the original pattern order. The value is entity-decoded (`&apos;` ->
        // `'`, etc.) so callers that feed an attribute into a menu request or
        // an outbound `<d cmd>` game command send the real character, not the
        // literal entity. decode_entities is a no-op on entity-free values.
        if let Some(value_start) = Self::find_attr_value_start(tag, attr, b'"') {
            if let Some(end) = tag[value_start..].find('"') {
                return Some(Self::decode_entities(
                    tag[value_start..value_start + end].to_string(),
                ));
            }
        }

        if let Some(value_start) = Self::find_attr_value_start(tag, attr, b'\'') {
            if let Some(end) = tag[value_start..].find('\'') {
                return Some(Self::decode_entities(
                    tag[value_start..value_start + end].to_string(),
                ));
            }
        }

        None
    }

    // ==================== Container/Inventory Handlers ====================

    pub(super) fn handle_inv_paired(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
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

    pub(super) fn handle_container(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <container id='225766824' title='Bandolier' target='#225766824' location='right'/>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            let title = Self::extract_attribute(tag, "title").unwrap_or_default();
            let target = Self::extract_attribute(tag, "target");
            elements.push(ParsedElement::Container { id, title, target });
        }
    }

    pub(super) fn handle_clear_container(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <clearContainer id="225766824"/>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            elements.push(ParsedElement::ClearContainer { id });
        }
    }

    // ==================== Target List Handler ====================

    pub(super) fn handle_dropdown(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
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
                    target_ids,
                });
            }
            // Other dropdowns (dDBStance, etc.) are silently ignored
        }
    }
}
