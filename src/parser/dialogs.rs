//! dialogData parsing: resident and specialized dialogs, openDialog,
//! embedded controls/dropdowns/buttons/fields/progress bars, and
//! quickbar entries.

use super::*;

impl XmlParser {
    pub(super) fn handle_dialog_data(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
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
        // <dialogData id='BetrayerPanel'><label id='lblBPs' value='Blood Points: 100' .../></dialogData>
        // <dialogData id='encum'>...<label id='encumblurb' value='You are not encumbered...' .../></dialogData>

        let tag_head = match tag.find('>') {
            Some(idx) => &tag[..idx],
            None => tag,
        };
        let specialized = self.handle_dialog_data_specialized(tag, tag_head, elements);

        // Shared extraction below runs for every dialogData exactly once
        // (previously a second handler re-parsed the same tag, emitting
        // duplicate ProgressBar elements for e.g. every minivitals update).

        // BetrayerPanel publishes blood points inside a label; emit it as a
        // ProgressBar so the existing progress plumbing carries it.
        if tag.contains("id='BetrayerPanel'") || tag.contains("id=\"BetrayerPanel\"") {
            if let Some(bp_start) = tag.find("Blood Points:") {
                // Extract the number after "Blood Points: " (skip the colon and space = 14 chars)
                let after_bp = &tag[bp_start + 14..].trim_start();
                // Find the end of the number (first non-digit)
                let num_str = match after_bp.find(|c: char| !c.is_ascii_digit()) {
                    Some(end) => &after_bp[..end],
                    None => after_bp,
                };
                if let Ok(value) = num_str.parse::<u32>() {
                    elements.push(ParsedElement::ProgressBar {
                        id: "lblBPs".to_string(),
                        value,
                        max: 100,
                        text: format!("Blood Points: {}", value),
                    });
                }
            }
        }

        // Extract progressBar tags (minivitals, expr, encum, stance, effect
        // durations, ...). Progress windows match on progress_id, so every
        // bar is forwarded even inside specialized dialogs.
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
            // Also feed the dialog store so a shown dialog (e.g. combat's
            // stance bar) can render it — additive alongside the widget
            // ProgressBar elements above, and skipped for quickbars.
            if let Some(id) = Self::extract_dialog_data_id(tag_head) {
                if !Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(tag_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let progress_bars = Self::parse_dialog_progress_bars(tag);
                    if !progress_bars.is_empty() {
                        elements.push(ParsedElement::DialogProgressBars {
                            id,
                            clear,
                            progress_bars,
                        });
                    }
                }
            }
        }

        // Extract label elements (encumbrance blurb, experience level, ...)
        if tag.contains("<label ") {
            let mut remaining = tag;
            while let Some(label_start) = remaining.find("<label ") {
                if let Some(label_end) = remaining[label_start..].find("/>") {
                    let label_tag = &remaining[label_start..label_start + label_end + 2];
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
            // Also feed the dialog store so a shown dialog panel (UberBar and
            // other resident dynamic dialogs) can render its label rows
            // positioned — additive alongside the flat Label elements above,
            // which existing widgets (encumbrance, experience) still consume.
            // Only emit when a label carries anchor-grid geometry, so plain
            // status labels don't churn the store or steal display space.
            if let Some(id) = Self::extract_dialog_data_id(tag_head) {
                if !Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(tag_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let (fields, labels) = Self::parse_dialog_fields(tag);
                    let positioned = labels.iter().any(|l| l.layout.is_some());
                    if positioned {
                        elements.push(ParsedElement::DialogFields {
                            id,
                            clear,
                            fields,
                            labels,
                        });
                    }
                }
            }
        }

        // Extract dropDownBox tags (combat targets). These only appear in
        // generic dialogs; specialized dialogs never carried one.
        // <dialogData id='combat'><dropDownBox id='dDBTarget' .../></dialogData>
        if !specialized && tag.contains("<dropDownBox ") {
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

    /// Specialized dialogData handling (buttons, fields, quickbars, injuries,
    /// active effects, ...). Returns true when a specialized branch consumed
    /// the dialog; the caller still runs the shared progressBar/label pass.
    pub(super) fn handle_dialog_data_specialized(
        &mut self,
        tag: &str,
        tag_head: &str,
        elements: &mut Vec<ParsedElement>,
    ) -> bool {
        // dropDownBoxes can share a chunk with buttons or arrive alone.
        // Emit them ADDITIVELY (no early return, and without marking the
        // chunk specialized) so button parsing below and the legacy
        // dDBTarget target-list extraction both keep running.
        if tag.contains("<dropDownBox") {
            if let Some(id) = Self::extract_dialog_data_id(tag_head) {
                if !Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(tag_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let dropdowns = Self::parse_dialog_dropdowns(tag);
                    if !dropdowns.is_empty() {
                        elements.push(ParsedElement::DialogDropDowns { id, clear, dropdowns });
                    }
                }
            }
        }
        // Links/images/spinboxes (combat's icon row, footer, quickstrike).
        // Additive, no early return, so buttons below still parse.
        if tag.contains("<link ") || tag.contains("<image ") || tag.contains("<upDownEditBox ") {
            if let Some(id) = Self::extract_dialog_data_id(tag_head) {
                if !Self::is_quickbar_id(&id) {
                    let clear = Self::extract_attribute(tag_head, "clear")
                        .map(|value| {
                            matches!(value.as_str(), "t" | "true" | "1")
                                || value.eq_ignore_ascii_case("true")
                        })
                        .unwrap_or(false);
                    let (links, images, spinboxes, skins) = Self::parse_dialog_controls(tag);
                    if !links.is_empty()
                        || !images.is_empty()
                        || !spinboxes.is_empty()
                        || !skins.is_empty()
                    {
                        elements.push(ParsedElement::DialogControls {
                            id,
                            clear,
                            links,
                            images,
                            spinboxes,
                            skins,
                        });
                    }
                }
            }
        }
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
                    return true;
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
                        return true;
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
                return true;
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
                    return true;
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
                        return true;
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
                return true;
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
                    return true;
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
                return true;
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
                        return true;
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
                return true;
            }
        }

        false
    }

    pub(super) fn handle_open_dialog(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
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
                // Titles arrive entity-escaped (and sometimes doubly so:
                // "Friends &amp;amp;&amp;amp; Enemies" on the wire) — decode
                // until stable so the stored title is the human string.
                let title = Self::extract_attribute(tag_head, "title")
                    .map(|t| {
                        let mut text = t.trim().to_string();
                        loop {
                            let decoded = Self::decode_entities(text.clone());
                            if decoded == text {
                                break text;
                            }
                            text = decoded;
                        }
                    })
                    .filter(|t| !t.is_empty());
                elements.push(ParsedElement::QuickbarOpen { id, title });
            } else {
                // Titles arrive entity-escaped (and sometimes doubly so:
                // "Friends &amp;amp;&amp;amp; Enemies" on the wire) — decode
                // until stable so the stored title is the human string.
                let title = Self::extract_attribute(tag_head, "title")
                    .map(|t| {
                        let mut text = t.trim().to_string();
                        loop {
                            let decoded = Self::decode_entities(text.clone());
                            if decoded == text {
                                break text;
                            }
                            text = decoded;
                        }
                    })
                    .filter(|t| !t.is_empty());
                if is_resident {
                    // Resident dialogs are persistent PANELS (combat, Buffs,
                    // injuries, ...). Announce them so they register as a
                    // resident offer and can be enabled as a dockable panel
                    // — distinct from the transient popup path below.
                    elements.push(ParsedElement::DialogPanelOpen {
                        id,
                        title,
                        save: save_position,
                    });
                } else {
                    let location = Self::extract_attribute(tag_head, "location");
                    tracing::debug!("Parser emitting DialogOpen: id={}, title={:?}, save={}", id, title, save_position);
                    elements.push(ParsedElement::DialogOpen {
                        id,
                        title,
                        save: save_position,
                        location,
                    });
                }
            }
        }

        self.handle_embedded_quickbar_dialog_data(tag, elements);
        self.handle_embedded_dialog_buttons(tag, elements);
        self.handle_embedded_dialog_dropdowns(tag, elements);
        self.handle_embedded_dialog_controls(tag, elements);
        self.handle_embedded_dialog_fields(tag, elements);

        // For resident dialogs, extract progressBar data for widget updates
        // For non-resident dialogs (popups), extract progressBar data for dialog rendering
        // Always call handle_embedded_resident_dialog_data to emit standalone ProgressBar/Label
        // elements for game state updates (needed for widgets like gs4_experience, encumbrance)
        self.handle_embedded_resident_dialog_data(tag, elements);
        // Ingest progressBars into the dialog store so a shown panel can render
        // them positioned. For resident dialogs this is additive alongside the
        // flat ProgressBar widget emit above; for popups it's the render feed.
        self.handle_embedded_dialog_progress_bars(tag, elements);
    }

    /// Extract progressBar and other widget data from embedded dialogData in resident dialogs
    pub(super) fn handle_embedded_resident_dialog_data(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
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

    pub(super) fn handle_embedded_quickbar_dialog_data(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
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

    /// Emit DialogControls (links/images/spinboxes) for dialogData blocks
    /// embedded in an openDialog tag — combat's login-time icon+configure
    /// chunk arrives this way (mirrors handle_embedded_dialog_dropdowns).
    pub(super) fn handle_embedded_dialog_controls(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
        let mut remaining = tag;
        let end_pattern = "</dialogData>";
        while let Some(start) = remaining.find("<dialogData") {
            let Some(end_start) = remaining[start..].find(end_pattern) else {
                break;
            };
            let end = start + end_start + end_pattern.len();
            let dialog_tag = &remaining[start..end];

            if !(dialog_tag.contains("<link ")
                || dialog_tag.contains("<image ")
                || dialog_tag.contains("<upDownEditBox ")
                || dialog_tag.contains("<skin "))
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
                    let (links, images, spinboxes, skins) =
                        Self::parse_dialog_controls(dialog_tag);
                    if !links.is_empty()
                        || !images.is_empty()
                        || !spinboxes.is_empty()
                        || !skins.is_empty()
                    {
                        elements.push(ParsedElement::DialogControls {
                            id,
                            clear,
                            links,
                            images,
                            spinboxes,
                            skins,
                        });
                    }
                }
            }
            remaining = &remaining[end..];
        }
    }

    /// Emit DialogDropDowns for dropDownBoxes inside dialogData blocks
    /// embedded in an openDialog tag (mirrors handle_embedded_dialog_buttons).
    pub(super) fn handle_embedded_dialog_dropdowns(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
        let mut remaining = tag;
        let end_pattern = "</dialogData>";

        while let Some(start) = remaining.find("<dialogData") {
            let Some(end_start) = remaining[start..].find(end_pattern) else {
                break;
            };
            let end = start + end_start + end_pattern.len();
            let dialog_tag = &remaining[start..end];

            if !dialog_tag.contains("<dropDownBox") {
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
                    let dropdowns = Self::parse_dialog_dropdowns(dialog_tag);
                    if !dropdowns.is_empty() {
                        elements.push(ParsedElement::DialogDropDowns { id, clear, dropdowns });
                    }
                }
            }

            remaining = &remaining[end..];
        }
    }

    pub(super) fn handle_embedded_dialog_buttons(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
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

    pub(super) fn handle_embedded_dialog_fields(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
        let mut remaining = tag;
        let end_pattern = "</dialogData>";

        while let Some(start) = remaining.find("<dialogData") {
            let Some(end_start) = remaining[start..].find(end_pattern) else {
                break;
            };
            let end = start + end_start + end_pattern.len();
            let dialog_tag = &remaining[start..end];

            // Fields imply an input dialog; positioned label rows (a resident
            // dynamic dialog's grid, e.g. UberBar) also belong in the store.
            // Plain unpositioned labels stay out — they feed widgets via the
            // flat Label emit instead.
            let has_inputs =
                dialog_tag.contains("<editBox") || dialog_tag.contains("<upDownEditBox");
            if !has_inputs && !dialog_tag.contains("<label ") {
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
                    // Without input fields, only ingest when a label carries
                    // anchor geometry — that's what marks a positioned grid.
                    let positioned_labels = labels.iter().any(|l| l.layout.is_some());
                    if !fields.is_empty() || positioned_labels {
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
    pub(super) fn handle_embedded_dialog_progress_bars(&self, tag: &str, elements: &mut Vec<ParsedElement>) {
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
    pub(super) fn parse_dialog_progress_bars(tag: &str) -> Vec<DialogProgressBarSpec> {
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
                let layout = Self::parse_control_layout(pb_tag);

                progress_bars.push(DialogProgressBarSpec { id, value, text, layout });
            }

            remaining = &remaining[pb_end..];
        }

        progress_bars
    }

    pub(super) fn handle_switch_quickbar(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        if let Some(id) = Self::extract_attribute(tag, "id") {
            if Self::is_quickbar_id(&id) {
                elements.push(ParsedElement::QuickbarSwitch { id });
            }
        }
    }

    pub(super) fn handle_close_dialog(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        if let Some(id) = Self::extract_attribute(tag, "id") {
            elements.push(ParsedElement::CloseDialog { id });
        }
    }

    pub(super) fn is_quickbar_id(id: &str) -> bool {
        id == "quick" || id.starts_with("quick-")
    }

    pub(super) fn extract_dialog_data_id(tag_head: &str) -> Option<String> {
        Self::extract_attribute(tag_head, "id")
            .or_else(|| Self::extract_attribute(tag_head, "name"))
    }

    pub(super) fn parse_quickbar_entries(tag: &str) -> Vec<QuickbarEntry> {
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

    pub(super) fn parse_dialog_buttons(tag: &str) -> Vec<DialogButton> {
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
                layout: Self::parse_control_layout(tag_slice),
            });

            remaining = &remaining[advance_by..];
        }

        buttons
    }

    /// Pull the pixel layout hints (top/left/size/align/anchors) off a
    /// dialog control tag. None when the tag carries none.
    pub(super) fn parse_control_layout(tag_slice: &str) -> Option<crate::data::DialogControlLayout> {
        let layout = crate::data::DialogControlLayout {
            top: Self::extract_attribute(tag_slice, "top").and_then(|v| v.parse().ok()),
            left: Self::extract_attribute(tag_slice, "left").and_then(|v| v.parse().ok()),
            width: Self::extract_attribute(tag_slice, "width").and_then(|v| v.parse().ok()),
            height: Self::extract_attribute(tag_slice, "height").and_then(|v| v.parse().ok()),
            align: Self::extract_attribute(tag_slice, "align").filter(|v| !v.is_empty()),
            anchor_top: Self::extract_attribute(tag_slice, "anchor_top").filter(|v| !v.is_empty()),
            anchor_left: Self::extract_attribute(tag_slice, "anchor_left")
                .filter(|v| !v.is_empty()),
            anchor_right: Self::extract_attribute(tag_slice, "anchor_right")
                .filter(|v| !v.is_empty()),
        };
        (!layout.is_empty()).then_some(layout)
    }

    /// Parse `<link>`, `<image>`, and `<upDownEditBox>` controls from a
    /// dialogData chunk (combat's icon row, footer commands, quickstrike
    /// spinner). Links inside quickbar dialogData are the quickbar's own
    /// buttons and handled elsewhere, so callers gate on non-quickbar ids.
    pub(super) fn parse_dialog_controls(
        tag: &str,
    ) -> (
        Vec<crate::data::DialogLink>,
        Vec<crate::data::DialogImage>,
        Vec<crate::data::DialogSpinBox>,
        Vec<crate::data::DialogSkin>,
    ) {
        let mut links = Vec::new();
        let mut images = Vec::new();
        let mut spinboxes = Vec::new();
        let mut skins = Vec::new();

        let mut remaining = tag;
        while let Some(start) = remaining.find("<link ") {
            remaining = &remaining[start..];
            let Some(end) = Self::self_closing_end(remaining) else { break };
            let slice = &remaining[..end];
            if let Some(id) = Self::extract_attribute(slice, "id") {
                links.push(crate::data::DialogLink {
                    id,
                    label: Self::extract_attribute(slice, "value").unwrap_or_default(),
                    command: Self::extract_attribute(slice, "cmd").unwrap_or_default(),
                    layout: Self::parse_control_layout(slice),
                });
            }
            remaining = &remaining[end..];
        }

        let mut remaining = tag;
        while let Some(start) = remaining.find("<image ") {
            remaining = &remaining[start..];
            let Some(end) = Self::self_closing_end(remaining) else { break };
            let slice = &remaining[..end];
            if let Some(id) = Self::extract_attribute(slice, "id") {
                images.push(crate::data::DialogImage {
                    id,
                    name: Self::extract_attribute(slice, "name").unwrap_or_default(),
                    command: Self::extract_attribute(slice, "cmd").unwrap_or_default(),
                    tooltip: Self::extract_attribute(slice, "tooltip").filter(|v| !v.is_empty()),
                    layout: Self::parse_control_layout(slice),
                });
            }
            remaining = &remaining[end..];
        }

        let mut remaining = tag;
        while let Some(start) = remaining.find("<upDownEditBox ") {
            remaining = &remaining[start..];
            let Some(end) = Self::self_closing_end(remaining) else { break };
            let slice = &remaining[..end];
            if let Some(id) = Self::extract_attribute(slice, "id") {
                spinboxes.push(crate::data::DialogSpinBox {
                    id,
                    value: Self::extract_attribute(slice, "value")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                    min: Self::extract_attribute(slice, "min")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(i32::MIN),
                    max: Self::extract_attribute(slice, "max")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(i32::MAX),
                    layout: Self::parse_control_layout(slice),
                });
            }
            remaining = &remaining[end..];
        }

        let mut remaining = tag;
        while let Some(start) = remaining.find("<skin ") {
            remaining = &remaining[start..];
            let Some(end) = Self::self_closing_end(remaining) else { break };
            let slice = &remaining[..end];
            if let Some(id) = Self::extract_attribute(slice, "id") {
                let name = Self::extract_attribute(slice, "name").unwrap_or_default();
                let controls = Self::extract_attribute(slice, "controls")
                    .map(|c| {
                        c.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                skins.push(crate::data::DialogSkin {
                    id,
                    name,
                    controls,
                    layout: Self::parse_control_layout(slice),
                });
            }
            remaining = &remaining[end..];
        }

        (links, images, spinboxes, skins)
    }

    /// End offset (exclusive) of a self-closing or open tag at the start
    /// of `s`: prefers `/>`, falls back to `>`.
    pub(super) fn self_closing_end(s: &str) -> Option<usize> {
        s.find("/>").map(|e| e + 2).or_else(|| s.find('>').map(|e| e + 1))
    }

    /// Parse every `<dropDownBox>` in a dialogData chunk into option
    /// pickers: current value, (text, value) option pairs zipped from the
    /// content_text/content_value CSVs, the selection command, and layout.
    pub(super) fn parse_dialog_dropdowns(tag: &str) -> Vec<crate::data::DialogDropDown> {
        let mut dropdowns = Vec::new();
        let mut remaining = tag;
        while let Some(start) = remaining.find("<dropDownBox") {
            remaining = &remaining[start..];
            let Some(end) = remaining.find("/>").map(|e| e + 2).or_else(|| {
                remaining.find('>').map(|e| e + 1)
            }) else {
                break;
            };
            let tag_slice = &remaining[..end];

            if let Some(id) = Self::extract_attribute(tag_slice, "id") {
                let texts = Self::extract_attribute(tag_slice, "content_text").unwrap_or_default();
                let values =
                    Self::extract_attribute(tag_slice, "content_value").unwrap_or_default();
                let texts: Vec<&str> = texts.split(',').filter(|s| !s.is_empty()).collect();
                let values: Vec<&str> = values.split(',').filter(|s| !s.is_empty()).collect();
                // Pair text with value; a mismatched/missing value list
                // falls back to the display text as the submit value.
                let options: Vec<(String, String)> = texts
                    .iter()
                    .enumerate()
                    .map(|(i, text)| {
                        let value = values.get(i).copied().unwrap_or(text);
                        (text.to_string(), value.to_string())
                    })
                    .collect();
                dropdowns.push(crate::data::DialogDropDown {
                    id,
                    value: Self::extract_attribute(tag_slice, "value").unwrap_or_default(),
                    options,
                    command: Self::extract_attribute(tag_slice, "cmd").unwrap_or_default(),
                    tooltip: Self::extract_attribute(tag_slice, "tooltip")
                        .filter(|v| !v.is_empty()),
                    layout: Self::parse_control_layout(tag_slice),
                });
            }
            remaining = &remaining[end..];
        }
        dropdowns
    }

    pub(super) fn parse_dialog_fields(tag: &str) -> (Vec<DialogFieldSpec>, Vec<DialogLabelSpec>) {
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
                let layout = Self::parse_control_layout(tag_slice);
                let justify =
                    Self::extract_attribute(tag_slice, "justify").and_then(|v| v.parse().ok());
                labels.push(DialogLabelSpec { id, value, layout, justify });
            }

            remaining = &remaining[advance_by..];
        }

        (fields, labels)
    }

    pub(super) fn sanitize_dialog_label(value: &str) -> String {
        // Some dialog labels embed pseudo-attributes after a quote, e.g.
        // `Label" anchor_top"displayedit_text`. Keep only the leading text.
        // extract_attribute now entity-decodes, so the quote arrives as a real
        // `"` (it used to be the literal `&quot;`); truncate at either form so
        // this is robust regardless of decode timing.
        let mut cleaned = value.to_string();
        let cut = cleaned
            .find('"')
            .or_else(|| cleaned.find("&quot;"));
        if let Some(pos) = cut {
            cleaned.truncate(pos);
        }
        cleaned.trim().to_string()
    }

}
