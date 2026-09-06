//! Component and dialog handling: `handle_component` mirrors room and
//! character components into state, crtrStatus tag parsing, and the
//! shown-dialog reflection rules.

use super::*;

/// Component id VellumFE uses for room-window art.
///
/// GemStone declares `sprite` on every room change and has never once been
/// observed filling it (785k empty occurrences in the wire logs); Wrayth
/// renders no room-window images, showing its room pictures in the STORY
/// stream via `<resource picture>`. The name suggests it was meant for art
/// of some kind, but whatever the intent, the slot is inert in practice —
/// so scripts get a room-art channel and the game's own empty
/// re-declaration clears it on every move.
///
/// If Simu ever starts populating it, non-empty values already win over our
/// injection (see `handle_component`), so the game's content would take
/// precedence rather than being overwritten.
pub const SPRITE_COMPONENT: &str = "sprite";

impl MessageProcessor {
    /// Publish the room uid -> art index built from room_images.toml. Called
    /// at startup and on `.reload`; also after the editor saves, so mappings
    /// take effect without a restart.
    pub fn set_room_image_index(&mut self, index: crate::config::room_images::RoomImageIndex) {
        self.room_image_index = index;
    }

    /// The room uid the client currently believes it is in (last `<nav rm=>`).
    /// Used by `.roomimages set` to map "here" without the user typing a uid.
    pub fn current_room_uid(&self) -> Option<u64> {
        self.current_room_uid
    }

    /// After ingesting a dialogData delta into the store, reflect it into
    /// the visible `active_dialog` if this dialog should be shown. When
    /// first materializing a shown dialog, seed its saved position/size.
    /// Hidden dialogs stay in the store only. If the currently-shown
    /// dialog is a *different* id, leave it be (one popup at a time).
    pub(super) fn sync_shown_dialog(&self, ui_state: &mut UiState, id: &str, show: bool) {
        // Content arriving for the popup that is ALREADY on screen always
        // refreshes it — the openDialog block emits DialogOpen first and
        // its controls after, so without this the popup kept the empty
        // clone it was born with (bugDialogBox rendered blank).
        let refreshing_active = ui_state.active_dialog.as_ref().is_some_and(|d| d.id == id);
        if !show && !refreshing_active {
            return;
        }
        // Don't steal the screen from a different open dialog.
        if ui_state.active_dialog.as_ref().is_some_and(|d| d.id != id) {
            return;
        }
        let first_show = ui_state
            .active_dialog
            .as_ref()
            .map(|d| d.id != id)
            .unwrap_or(true);
        ui_state.show_dialog_from_store(id);
        if first_show {
            if let Some(dialog) = ui_state.active_dialog.as_mut() {
                if let Some(p) = self.saved_dialog_positions.dialogs.get(id) {
                    dialog.position = Some((p.x, p.y));
                    dialog.size = p.width.zip(p.height);
                    dialog.save_position = true;
                }
            }
        }
    }

    /// Whether a dialog's data may be shown as a transient popup. The
    /// always-ingest store keeps every dialog's state regardless; this only
    /// gates the popup. U6: nothing pops up unless the user has SHOWN it via
    /// the Windows list (its id is in `shown_dialog_ids`) — hidden-by-default,
    /// replacing the old blocklist.
    pub(super) fn dialog_should_popup(ui_state: &UiState, id: &str) -> bool {
        ui_state
            .shown_dialog_ids
            .iter()
            .any(|shown| shown.eq_ignore_ascii_case(id))
    }

    /// Scan raw component content for `<crtrStatus exist="..." .../>` tags,
    /// keyed by exist id. Component values are captured with embedded tags
    /// intact, so this runs over the same string the creature scan uses.
    pub(super) fn parse_crtr_status_tags(
        value: &str,
    ) -> std::collections::HashMap<String, crate::core::state::CreatureFlags> {
        let mut map = std::collections::HashMap::new();
        let mut remaining = value;
        while let Some(start) = remaining.find("<crtrStatus") {
            let Some(end_offset) = remaining[start..].find('>') else {
                break;
            };
            let tag = &remaining[start..start + end_offset + 1];
            let attrs = crate::parser::XmlParser::extract_all_attributes(tag);
            let exist = attrs
                .iter()
                .find(|(name, _)| name == "exist")
                .map(|(_, value)| value.clone());
            if let Some(exist) = exist {
                let flags = crate::core::state::CreatureFlags::from_xml_attrs(
                    attrs
                        .iter()
                        .filter(|(name, _)| name != "exist")
                        .map(|(n, v)| (n.as_str(), v.as_str())),
                );
                map.insert(exist, flags);
            }
            remaining = &remaining[start + end_offset + 1..];
        }
        map
    }

    /// Handle component data for room window and exp window (DR)
    pub(super) fn handle_component(
        &mut self,
        id: &str,
        value: &str,
        game_state: &mut GameState,
        room_components: &mut std::collections::HashMap<String, Vec<Vec<TextSegment>>>,
        current_room_component: &mut Option<String>,
        room_window_dirty: &mut bool,
    ) {
        // Mark ALL components as silent updates (shouldn't trigger prompts in main window)
        // This includes DR experience components (exp Brawling, exp tdp, etc.)
        self.chunk_has_silent_updates = true;

        // Handle DragonRealms experience components (exp Stealth, exp tdp, etc.)
        if let Some(field_name) = id.strip_prefix("exp ") {
            // Register the field order (will be a no-op after first occurrence)
            game_state
                .dr_experience
                .register_field(field_name.to_string());

            // Update the value (only triggers generation bump if changed)
            if game_state
                .dr_experience
                .update_field(field_name, value.to_string())
            {
                tracing::debug!("Exp component updated: {} = {}", field_name, value);
            } else {
                tracing::trace!("Exp component unchanged: {}", field_name);
            }
            return;
        }

        // Only process room-related components for room window updates.
        // `sprite` rides along: the game declares it on every room change but
        // has never once filled it (785k empty occurrences in the wire logs),
        // so it is the natural slot for room art — a script writes a
        // `<vellumImg>` there and it lands in the room window's own stream
        // instead of detouring through the story feed.
        if !id.starts_with("room ") && id != SPRITE_COMPONENT {
            tracing::trace!("Ignoring non-room component: {}", id);
            return;
        }

        // Skip processing if we're discarding the current stream (no window exists)
        if self.discard_current_stream {
            tracing::debug!("Skipping room component {} - no room window exists", id);
            return;
        }

        // Check if component value has changed (avoid unnecessary processing).
        // `sprite` is exempt: the game sends it EMPTY on every room change, so
        // "unchanged" would short-circuit before room-art injection runs and
        // art would only ever appear in the first mapped room of a session.
        if let Some(previous_value) = self.previous_room_components.get(id) {
            if previous_value == value && id != SPRITE_COMPONENT {
                tracing::trace!("Room component {} unchanged - skipping processing", id);
                return;
            }
            // Debug: log when room objs changes (especially to empty)
            if id == "room objs" {
                tracing::debug!(
                    "Room objs changed: prev_len={}, new_len={}, new_empty={}",
                    previous_value.len(),
                    value.len(),
                    value.is_empty()
                );
            }
        } else if id == "room objs" {
            tracing::debug!(
                "Room objs first seen: len={}, empty={}",
                value.len(),
                value.is_empty()
            );
        }

        tracing::debug!(
            "Processing room component: {} (value length: {})",
            id,
            value.len()
        );

        // Store current value for next comparison
        self.previous_room_components
            .insert(id.to_string(), value.to_string());

        // Extract creatures from room objs (for targets widget)
        // Room objs contains items/creatures on ground. Creatures are in bold:
        // <b><pushBold/>a <a exist='ID' noun='...'>name</a><popBold/></b> (status)
        if id == "room objs" {
            let had_objs = !game_state.room_creatures.is_empty();
            game_state.room_creatures.clear();
            // handle_component early-returns on unchanged values, so this
            // block only runs on real changes - the bump is accurate
            game_state.room_creatures_generation += 1;

            // Log when room objs becomes empty (item picked up, etc.)
            if value.is_empty() {
                tracing::debug!(
                    "Room objs now empty (previously had creatures: {})",
                    had_objs
                );
            }

            // Pre-scan for <crtrStatus exist="..." .../> snapshots embedded in
            // the component (the tag precedes each creature's bold name).
            // Keyed by exist id; the tag is self-contained so pairing by id
            // beats positional pairing.
            let crtr_flags = Self::parse_crtr_status_tags(value);

            let mut remaining = value;
            while let Some(bold_start) = remaining.find("<b>") {
                // Find the matching </b>
                if let Some(bold_end_offset) = remaining[bold_start..].find("</b>") {
                    let bold_end = bold_start + bold_end_offset;
                    let bold_section = &remaining[bold_start..bold_end + 4]; // Include </b>

                    // Extract <a exist='...' noun='...'>name</a> within the bold section
                    if let Some(link_start) = bold_section.find("<a ") {
                        if let Some(link_end) = bold_section[link_start..].find("</a>") {
                            let link_tag_end = bold_section[link_start..link_start + link_end]
                                .find('>')
                                .unwrap_or(0);
                            let link_tag = &bold_section[link_start..link_start + link_tag_end];
                            let link_text_start = link_start + link_tag_end + 1;
                            let link_text_end = link_start + link_end;
                            let creature_name = &bold_section[link_text_start..link_text_end];

                            // Extract exist ID from the link tag
                            if let Some(exist_pos) = link_tag.find("exist=") {
                                let after_exist = &link_tag[exist_pos + 6..];
                                if let Some(quote) = after_exist.chars().next() {
                                    if quote == '\'' || quote == '"' {
                                        if let Some(end_quote) = after_exist[1..].find(quote) {
                                            let exist_id = &after_exist[1..=end_quote];

                                            // Extract noun from the link tag (optional)
                                            let noun = if let Some(noun_pos) =
                                                link_tag.find("noun=")
                                            {
                                                let after_noun = &link_tag[noun_pos + 5..];
                                                if let Some(noun_quote) = after_noun.chars().next()
                                                {
                                                    if noun_quote == '\'' || noun_quote == '"' {
                                                        if let Some(noun_end_quote) =
                                                            after_noun[1..].find(noun_quote)
                                                        {
                                                            Some(
                                                                after_noun[1..=noun_end_quote]
                                                                    .to_string(),
                                                            )
                                                        } else {
                                                            None
                                                        }
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            };

                                            // Check for status after </b>: " (stunned)" or " (dead)"
                                            let after_bold = &remaining[bold_end + 4..];
                                            let status = if after_bold.trim_start().starts_with('(')
                                            {
                                                // Extract text between ( and )
                                                after_bold.find('(').and_then(|start| {
                                                    let after_paren = &after_bold[start + 1..];
                                                    after_paren
                                                        .find(')')
                                                        .map(|end| after_paren[..end].to_string())
                                                })
                                            } else {
                                                None
                                            };

                                            // Check if noun should be excluded (configurable filter for non-creatures)
                                            if let Some(ref noun_val) = noun {
                                                if self
                                                    .config
                                                    .target_list
                                                    .excluded_nouns
                                                    .iter()
                                                    .any(|excluded| {
                                                        excluded.eq_ignore_ascii_case(noun_val)
                                                    })
                                                {
                                                    tracing::debug!(
                                                        "Skipping creature with excluded noun: '{}' (name: '{}')",
                                                        noun_val, creature_name
                                                    );
                                                    remaining = &remaining[bold_end + 4..];
                                                    continue;
                                                }
                                            }

                                            let creature = crate::core::state::Creature {
                                                id: format!("#{}", exist_id),
                                                name: creature_name.to_string(),
                                                noun: noun.clone(),
                                                status: status.clone(),
                                                flags: crtr_flags.get(exist_id).cloned(),
                                            };

                                            tracing::debug!(
                                                "Parsed creature from room objs: name='{}', noun={:?}, id='{}', status={:?}",
                                                creature.name, creature.noun, creature.id, creature.status
                                            );

                                            game_state.room_creatures.push(creature);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    remaining = &remaining[bold_end + 4..];
                } else {
                    break;
                }
            }

            tracing::debug!(
                "Extracted {} creatures from room objs",
                game_state.room_creatures.len()
            );

            // Now extract room objects (non-bold links = items on ground)
            // Strategy: remove all <b>...</b> sections, then parse remaining <a> links
            game_state.room_objects.clear();
            game_state.room_objects_generation += 1;

            // Create a version of the value with bold sections removed
            let mut no_bold = String::new();
            let mut pos = 0usize;
            while pos < value.len() {
                if let Some(bold_start) = value[pos..].find("<b>") {
                    // Add everything before <b>
                    no_bold.push_str(&value[pos..pos + bold_start]);
                    // Find matching </b>
                    if let Some(bold_end) = value[pos + bold_start..].find("</b>") {
                        pos = pos + bold_start + bold_end + 4; // Skip past </b>
                    } else {
                        break;
                    }
                } else {
                    // No more bold sections, add the rest
                    no_bold.push_str(&value[pos..]);
                    break;
                }
            }

            // Now parse <a> links from the non-bold content
            let mut remaining = no_bold.as_str();
            while let Some(link_start) = remaining.find("<a ") {
                if let Some(link_end) = remaining[link_start..].find("</a>") {
                    let link_section = &remaining[link_start..link_start + link_end + 4];

                    // Extract the tag part and text part
                    if let Some(tag_end) = link_section.find('>') {
                        let link_tag = &link_section[..tag_end];
                        let link_text = &link_section[tag_end + 1..link_section.len() - 4]; // Remove </a>

                        // Extract exist ID
                        if let Some(exist_pos) = link_tag.find("exist=") {
                            let after_exist = &link_tag[exist_pos + 6..];
                            if let Some(quote) = after_exist.chars().next() {
                                if quote == '\'' || quote == '"' {
                                    if let Some(end_quote) = after_exist[1..].find(quote) {
                                        let exist_id = &after_exist[1..=end_quote];

                                        // Extract noun
                                        let noun = if let Some(noun_pos) = link_tag.find("noun=") {
                                            let after_noun = &link_tag[noun_pos + 5..];
                                            if let Some(noun_quote) = after_noun.chars().next() {
                                                if noun_quote == '\'' || noun_quote == '"' {
                                                    if let Some(noun_end) =
                                                        after_noun[1..].find(noun_quote)
                                                    {
                                                        Some(after_noun[1..=noun_end].to_string())
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };

                                        let room_object = crate::core::state::RoomObject {
                                            id: exist_id.to_string(),
                                            name: link_text.to_string(),
                                            noun,
                                        };

                                        tracing::debug!(
                                            "Parsed room object: name='{}', noun={:?}, id='{}'",
                                            room_object.name,
                                            room_object.noun,
                                            room_object.id
                                        );

                                        game_state.room_objects.push(room_object);
                                    }
                                }
                            }
                        }
                    }

                    remaining = &remaining[link_start + link_end + 4..];
                } else {
                    break;
                }
            }

            tracing::debug!(
                "Extracted {} room objects from room objs",
                game_state.room_objects.len()
            );

            // Dual-write ground items into the registry (the `floor`/
            // `ground`/`room` foreach targets). Room loot = NOT yours,
            // distinct from at-feet. Consumers still read room_objects.
            let ground: Vec<crate::core::game_objects::GameItem> = game_state
                .room_objects
                .iter()
                .map(|o| {
                    crate::core::game_objects::GameItem::new(
                        o.id.clone(),
                        o.noun.clone().unwrap_or_default(),
                        o.name.clone(),
                    )
                })
                .collect();
            game_state.objects.set_ground(ground);
        }

        // Extract players from room players component
        // Format: "Also here: <a exist='-ID' noun='Name'>Name</a> (prone), a stunned <a exist='...' noun='...'>Name2</a> (prone)"
        if id == "room players" {
            game_state.room_players.clear();
            game_state.room_players_generation += 1;

            let mut remaining = value;

            // Skip "Also here:" prefix if present
            if let Some(pos) = remaining.find(':') {
                remaining = &remaining[pos + 1..];
            }

            // Parse players - separated by commas or end of component
            while let Some(link_start) = remaining.find("<a ") {
                if let Some(link_end) = remaining[link_start..].find("</a>") {
                    let link_section_end = link_start + link_end + 4;
                    let link_section = &remaining[link_start..link_section_end];

                    // Extract exist ID
                    if let Some(exist_pos) = link_section.find("exist=") {
                        let after_exist = &link_section[exist_pos + 6..];
                        if let Some(quote) = after_exist.chars().next() {
                            if quote == '\'' || quote == '"' {
                                if let Some(end_quote) = after_exist[1..].find(quote) {
                                    let exist_id = &after_exist[1..=end_quote];

                                    // Extract player name
                                    if let Some(name_start) = link_section.find('>') {
                                        let name_end = link_section.find("</a>").unwrap();
                                        let player_name = &link_section[name_start + 1..name_end];

                                        // Prefix zone (text before the link):
                                        // may carry titles ("Lord ", "Arena
                                        // Occultist ") and/or the corpse marker
                                        // ("the body of "). Titles are stripped;
                                        // "the body of" sets the dead flag.
                                        let before_link = &remaining[..link_start];
                                        let (dead, primary_status) =
                                            Self::parse_player_prefix(before_link);

                                        // Suffix zone (text after the link, up to
                                        // the next comma that separates players).
                                        // Holds either the brief "(prone)" form or
                                        // the verbose "who is lying down" clause.
                                        let after_link = &remaining[link_section_end..];
                                        let suffix = match after_link.find(',') {
                                            Some(comma) => &after_link[..comma],
                                            None => after_link,
                                        };
                                        let secondary_status =
                                            Self::parse_player_suffix_status(suffix);

                                        let player = crate::core::state::Player {
                                            id: exist_id.to_string(),
                                            name: player_name.to_string(),
                                            primary_status,
                                            secondary_status,
                                            dead,
                                        };

                                        game_state.room_players.push(player);
                                    }
                                }
                            }
                        }
                    }

                    remaining = &remaining[link_section_end..];
                } else {
                    break;
                }
            }

            tracing::debug!(
                "Extracted {} players from room players",
                game_state.room_players.len()
            );
        }

        // If we're starting a new component, finish the current one first
        if current_room_component
            .as_ref()
            .map(|c| c != id)
            .unwrap_or(false)
        {
            // Finish current component
            *current_room_component = None;
        }

        // ALWAYS clear the component buffer when receiving new data (game sends full replacement, not append)
        room_components.entry(id.to_string()).or_default().clear();
        *current_room_component = Some(id.to_string());
        tracing::debug!("Started/replaced room component: {}", id);

        // Mark room window dirty when component is cleared (even if empty)
        // This ensures the room window updates when items are picked up, etc.
        *room_window_dirty = true;

        // Room art: the game hands us an EMPTY `sprite` slot on every room
        // change, and the uid arrived earlier in the same block via <nav rm=>.
        // If this room is mapped, fill the slot with the same <vellumImg> a
        // script would have sent — no rewriting of game text anywhere.
        //
        // A non-empty sprite means a script claimed it; script art always
        // wins. An unmapped room, missing art file, or the feature being off
        // all leave the slot empty rather than showing a broken label.
        let injected;
        let value = if id == SPRITE_COMPONENT
            && value.trim().is_empty()
            && self.config.room_images.enabled
        {
            // Resolve conditional art (day/night and friends) BEFORE the
            // installed-file check, so a night variant is what gets tested
            // rather than the daytime default.
            let now_server = chrono::Utc::now().timestamp() + self.server_time_offset;
            match self
                .current_room_uid
                .and_then(|uid| self.room_image_index.get(uid))
                .and_then(|art| {
                    let resolved = art.resolve_name(game_state, now_server, None).to_string();
                    if crate::core::inline_image::contains(&resolved) {
                        Some((art, resolved))
                    } else if resolved != art.name && crate::core::inline_image::contains(&art.name)
                    {
                        // A variant matched but its file isn't installed (typo,
                        // deleted art). Fall back to the entry's base image rather
                        // than dropping the room's art for the whole phase.
                        Some((art, art.name.clone()))
                    } else {
                        None
                    }
                }) {
                Some((art, name)) => {
                    let align = match art.align_or_default() {
                        crate::data::FloatAlign::Right => "right",
                        crate::data::FloatAlign::Left => "left",
                    };
                    injected = format!(
                        "<vellumImg src='{}' rows='{}' align='{align}'/>",
                        name,
                        art.rows_or_default()
                    );
                    tracing::debug!(
                        "room art: room {:?} -> '{}' (mapping '{}')",
                        self.current_room_uid,
                        name,
                        art.name
                    );
                    injected.as_str()
                }
                None => value,
            }
        } else {
            value
        };

        // Lich uses this sentence as a disabled-stream sentinel. Treat it as
        // an absent component so it cannot replace real room prose or leak
        // into Story while the native main-stream room block is in flight.
        let value = if id == "room desc" && super::room_description_is_disabled(value) {
            ""
        } else {
            value
        };

        // An empty "room desc" component clears the mirrored prose (the parse
        // block below is skipped for empty values, so clear it here). Room
        // art survives on its own: a room with a picture but no description
        // should still show the picture.
        if id == "room desc" && value.trim().is_empty() {
            let art = std::mem::take(&mut self.pending_room_art);
            let new_desc: Vec<crate::data::widget::StyledLine> = if art.is_empty() {
                Vec::new()
            } else {
                vec![crate::data::widget::StyledLine {
                    segments: art,
                    stream: "room".to_string(),
                    timestamp: None,
                }]
            };
            if game_state.room_description != new_desc {
                game_state.room_description = new_desc;
                game_state.room_description_generation += 1;
            }
        }

        // Parse the component value to extract styled segments
        if !value.trim().is_empty() {
            // Save parser state before parsing component (components are self-contained)
            let saved_color_stack = self.parser.color_stack.clone();
            let saved_preset_stack = self.parser.preset_stack.clone();
            let saved_style_stack = self.parser.style_stack.clone();
            let saved_bold_stack = self.parser.bold_stack.clone();
            let saved_link_depth = self.parser.link_depth;
            let saved_spell_depth = self.parser.spell_depth;
            let saved_link_data = self.parser.current_link_data.clone();

            // Clear stacks for component parsing (start with clean state)
            self.parser.color_stack.clear();
            self.parser.preset_stack.clear();
            self.parser.style_stack.clear();
            self.parser.bold_stack.clear();
            self.parser.link_depth = 0;
            self.parser.spell_depth = 0;
            self.parser.current_link_data = None;

            // Parse the component value as XML to get styled elements
            let parsed_elements = self.parser.parse_line(value);

            // Extract text segments from parsed elements
            let mut current_line_segments = Vec::new();

            for element in parsed_elements {
                match element {
                    crate::parser::ParsedElement::Text {
                        content,
                        fg_color,
                        bg_color,
                        bold,
                        span_type,
                        link_data,
                        ..
                    } => {
                        // Map parser SpanType to data layer SpanType
                        use crate::data::SpanType as DataSpanType;
                        use crate::parser::SpanType as ParserSpanType;
                        let data_span_type = match span_type {
                            ParserSpanType::Normal => DataSpanType::Normal,
                            ParserSpanType::Link => DataSpanType::Link,
                            ParserSpanType::Monsterbold => DataSpanType::Monsterbold,
                            ParserSpanType::Spell => DataSpanType::Spell,
                            ParserSpanType::Speech => DataSpanType::Speech,
                        };

                        // Link data is already the correct type from parser
                        let link = link_data.clone();

                        let segment = TextSegment {
                            text: content.clone(),
                            fg: fg_color.clone(),
                            bg: bg_color.clone(),
                            bold,
                            mono: false,
                            span_type: data_span_type,
                            link_data: link.clone(),
                            custom_emoji: None,
                            inline_image: None,
                        };

                        // Debug logging for room exits to understand link coloring
                        if id == "room exits" {
                            tracing::debug!(
                                "Room exits segment: text='{}', fg={:?}, span_type={:?}, has_link={}",
                                content,
                                fg_color,
                                data_span_type,
                                link.is_some()
                            );
                        }

                        current_line_segments.push(segment);
                    }
                    crate::parser::ParsedElement::VellumImage { src, rows, align } => {
                        // Inline image inside a room component, so a script
                        // can float art into the room window the same way it
                        // can into the story window.
                        current_line_segments.push(TextSegment {
                            text: format!("[img:{src}]"),
                            inline_image: Some(crate::data::InlineImage {
                                name: src,
                                rows,
                                align,
                            }),
                            ..Default::default()
                        });
                    }
                    _ => {
                        // Ignore other parsed elements (we only care about Text)
                    }
                }
            }

            // Mirror the room description prose onto GameState as STYLED lines
            // so headless/remote clients get the room "look" — with its
            // clickable scenery links and coloring — without a room window.
            // The game sends a full component replacement and handle_component
            // early-returns on unchanged values, so this runs only on real
            // changes — the generation bump stays accurate.
            if id == SPRITE_COMPONENT {
                // Remember the room's art so the `room desc` mirror below can
                // lead with it. Sprite arrives BEFORE the description in the
                // room block, so storing it into room_description here would
                // just be overwritten a moment later.
                self.pending_room_art = current_line_segments
                    .iter()
                    .filter(|s| s.inline_image.is_some())
                    .cloned()
                    .collect();
            }

            if id == "room desc" {
                let is_blank = current_line_segments
                    .iter()
                    .all(|s| s.text.trim().is_empty());
                // Lead with the room's art (if any) so non-GUI frontends —
                // the phone especially — float it beside the prose exactly
                // like the GUI room window does. The GUI merges the same way
                // in `room_sync`; this is the copy every OTHER frontend
                // reads.
                let mut segments = std::mem::take(&mut self.pending_room_art);
                let has_art = !segments.is_empty();
                let new_desc: Vec<crate::data::widget::StyledLine> = if is_blank && !has_art {
                    Vec::new()
                } else {
                    segments.extend(current_line_segments.iter().cloned());
                    vec![crate::data::widget::StyledLine {
                        segments,
                        stream: "room".to_string(),
                        timestamp: None,
                    }]
                };
                if game_state.room_description != new_desc {
                    game_state.room_description = new_desc;
                    game_state.room_description_generation += 1;
                }
            }

            // Add the line if we got any segments
            if !current_line_segments.is_empty() {
                if let Some(buffer) = room_components.get_mut(id) {
                    buffer.push(current_line_segments);
                    *room_window_dirty = true;
                }
            }

            // Restore parser state after parsing component
            self.parser.color_stack = saved_color_stack;
            self.parser.preset_stack = saved_preset_stack;
            self.parser.style_stack = saved_style_stack;
            self.parser.bold_stack = saved_bold_stack;
            self.parser.link_depth = saved_link_depth;
            self.parser.spell_depth = saved_spell_depth;
            self.parser.current_link_data = saved_link_data;
        }
    }
}
