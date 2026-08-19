//! Link and menu tracking: the d/a link stack, menu open/item/close,
//! launchURL, and the Lich WebUI tag.

use super::*;

impl XmlParser {
    /// Mirror the OUTERMOST open link into `current_link_data`, which is what
    /// text spans attach. For nested links (`store list`'s
    /// `<d cmd=...>a <a ...>item</a> ...</d>`) the enclosing `<d>` command is
    /// the actionable one — clicking the inner item text fires the outer
    /// command, matching Wrayth — so the wrapper wins over the inner `<a>`.
    /// Empty sentinels (a bare `<a>` with no exist/noun) are skipped.
    pub(super) fn sync_current_link_from_stack(&mut self) {
        self.current_link_data = self
            .link_stack
            .iter()
            .find(|d| !d.exist_id.is_empty())
            .cloned();
    }

    pub(super) fn handle_d_tag(&mut self, tag: &str) {
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

        // Push link data for this direct command onto the link stack (so a
        // nested <a> inside a <d> doesn't clobber the outer command).
        // For <d>, we use a special exist_id to indicate it's a direct command.
        let data = LinkData {
            exist_id: String::from("_direct_"), // Special marker for direct commands
            noun: cmd.clone().unwrap_or_default(), // Store cmd in noun field temporarily
            text: String::new(),                // Will be populated as text is rendered
            coord: None,                        // <d> tags don't use coords
        };
        self.link_stack.push(data);
        self.sync_current_link_from_stack();

        // Don't apply color if we're inside monsterbold (bold has priority)
        if !self.bold_stack.is_empty() {
            return;
        }

        // Check if tag has explicit color attributes first
        let fg = Self::extract_attribute(tag, "fg");
        let bg = Self::extract_attribute(tag, "bg");

        if fg.is_some() || bg.is_some() {
            // Explicit colors
            self.color_stack.push(ColorStyle { fg, bg });
        } else {
            // Use commands preset (like links preset for <a> tags)
            if let Some((preset_fg, preset_bg)) = self.presets.get("commands") {
                self.color_stack.push(ColorStyle {
                    fg: preset_fg.clone(),
                    bg: preset_bg.clone(),
                });
            }
        }
    }

    pub(super) fn handle_d_close(&mut self) {
        // Decrease link depth
        if self.link_depth > 0 {
            self.link_depth -= 1;
        }

        // Pop this <d>'s entry off the link stack and restore the enclosing
        // link (if any) as the current one — so text after a nested <a> gets
        // the outer <d> command back.
        if let Some(mut popped) = self.link_stack.pop() {
            // For <d> tags without a cmd attribute, the command IS the text
            // content; populate noun from it (matches the historical
            // single-slot behavior, kept for logging/consumers).
            if popped.noun.is_empty() && !popped.text.is_empty() {
                popped.noun = popped.text.clone();
                tracing::debug!("Populated <d> tag noun from text: '{}'", popped.noun);
            }
        }
        self.sync_current_link_from_stack();

        // Pop color if we added one
        if !self.color_stack.is_empty() {
            self.color_stack.pop();
        }
    }

    pub(super) fn handle_link_open(&mut self, tag: &str) {
        // <a exist="..." noun="..." coord="..."> - apply links preset color and extract metadata
        // Track link depth for semantic type
        self.link_depth += 1;

        // Extract link metadata (exist_id, noun, and optional coord)
        let exist_id = Self::extract_attribute(tag, "exist");
        let noun = Self::extract_attribute(tag, "noun");
        let coord = Self::extract_attribute(tag, "coord");

        // Push one entry per <a> open so the link stack stays balanced with
        // link_depth (and pop-on-close restores the enclosing link). A bare
        // <a> with no exist/noun pushes an empty sentinel — text inside it is
        // still a Link span (link_depth), but carries no clickable data, and
        // popping it restores the outer link for following text.
        let data = match (exist_id, noun) {
            (Some(exist), Some(n)) => LinkData {
                exist_id: exist,
                noun: n,
                text: String::new(), // Will be populated as text is rendered
                coord,               // Optional coord for direct commands
            },
            // Web link (game HELP text carries gswiki anchors): the URL rides
            // the noun behind the sentinel; each frontend opens it on its own
            // side. Non-http(s) schemes get the empty sentinel instead and
            // stay styled-but-inert.
            _ => match Self::extract_attribute(tag, "href")
                .filter(|url| crate::data::is_web_url(url))
            {
                Some(href) => LinkData {
                    exist_id: crate::data::URL_LINK_SENTINEL.to_string(),
                    noun: href,
                    text: String::new(),
                    coord: None,
                },
                None => LinkData {
                    exist_id: String::new(),
                    noun: String::new(),
                    text: String::new(),
                    coord: None,
                },
            },
        };
        self.link_stack.push(data);
        // Mirror the OUTERMOST link (the enclosing <d> wins over this <a>);
        // an all-empty stack surfaces as None.
        self.sync_current_link_from_stack();

        // Don't apply color if we're inside monsterbold (bold has priority).
        // Record that this link pushed NO color so its close doesn't pop one.
        if !self.bold_stack.is_empty() {
            self.link_pushed_color.push(false);
            return;
        }

        // Check if tag has explicit color attributes first
        let fg = Self::extract_attribute(tag, "fg");
        let bg = Self::extract_attribute(tag, "bg");

        if fg.is_some() || bg.is_some() {
            // Explicit colors
            self.color_stack.push(ColorStyle { fg, bg });
            self.link_pushed_color.push(true);
        } else if let Some((preset_fg, preset_bg)) = self.presets.get("links") {
            // Use links preset
            self.color_stack.push(ColorStyle {
                fg: preset_fg.clone(),
                bg: preset_bg.clone(),
            });
            self.link_pushed_color.push(true);
        } else {
            // No preset configured: pushed nothing.
            self.link_pushed_color.push(false);
        }
    }

    pub(super) fn handle_link_close(&mut self) {
        // Decrease link depth
        if self.link_depth > 0 {
            self.link_depth -= 1;
        }

        // Pop this <a>'s entry and restore the enclosing link (if any) as the
        // current one, so text after a nested link keeps the outer link.
        self.link_stack.pop();
        self.sync_current_link_from_stack();

        // Pop color iff THIS link's open pushed one — regardless of the current
        // bold state. Keying off bold at close time (the old behavior) leaked
        // the color whenever a link opened outside bold but closed inside it.
        if self.link_pushed_color.pop().unwrap_or(false) && !self.color_stack.is_empty() {
            self.color_stack.pop();
        }
    }

    pub(super) fn handle_menu_open(&mut self, tag: &str) {
        // <menu id="123" ...>
        if let Some(id) = Self::extract_attribute(tag, "id") {
            // tracing::debug!("Starting menu collection for id={}", id);
            self.current_menu_id = Some(id);
            self.current_menu_coords.clear();
        } else {
            tracing::warn!("Menu tag missing id attribute: {}", tag);
        }
    }

    pub(super) fn handle_menu_item(&mut self, tag: &str) {
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

    pub(super) fn handle_launch_url(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <LaunchURL src="/gs4/play/cm/loader.asp?uname=..."/>
        if let Some(src) = Self::extract_attribute(tag, "src") {
            tracing::debug!("Parsed LaunchURL: src={}", src);
            elements.push(ParsedElement::LaunchURL { url: src });
        } else {
            tracing::warn!("LaunchURL tag without src attribute: {}", tag);
        }
    }

    pub(super) fn handle_lich_webui(&mut self, tag: &str, elements: &mut Vec<ParsedElement>) {
        // <LichWebUI status="ok" port="51423" url="http://127.0.0.1:51423/"
        //            auth="http://127.0.0.1:51423/auth?token=<64 hex>" schema="1"/>
        // Also: <LichWebUI status="disabled"/> and <LichWebUI status="stopped"/>
        let status = Self::extract_attribute(tag, "status").unwrap_or_default();
        if status.is_empty() {
            tracing::warn!("LichWebUI tag without status attribute: {}", tag);
            return;
        }
        let handshake = crate::data::webui::WebUiHandshake {
            status,
            port: Self::extract_attribute(tag, "port")
                .and_then(|p| p.parse().ok())
                .unwrap_or(0),
            url: Self::extract_attribute(tag, "url").unwrap_or_default(),
            auth: Self::extract_attribute(tag, "auth").unwrap_or_default(),
            schema: Self::extract_attribute(tag, "schema")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        };
        tracing::info!(
            "Parsed LichWebUI handshake: status={} port={} schema={}",
            handshake.status,
            handshake.port,
            handshake.schema
        );
        elements.push(ParsedElement::LichWebUI(handshake));
    }

    pub(super) fn handle_menu_close(&mut self, elements: &mut Vec<ParsedElement>) {
        // </menu>
        if let Some(id) = self.current_menu_id.take() {
            let coords = std::mem::take(&mut self.current_menu_coords);
            // tracing::debug!("Finished menu collection for id={}, {} coords", id, coords.len());

            elements.push(ParsedElement::MenuResponse { id, coords });
        }
    }
}
