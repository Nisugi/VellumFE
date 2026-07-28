//! Item classification from Lich's `gameobj-data.xml` (the no-Lich path).
//!
//! Mirrors Lich's `GameObj#type` / `#sellable` exactly
//! (lich-5 `lib/common/gameobj.rb`, `matching_data_keys`):
//!
//! - an entry matches when the item's **name** matches `<name>` OR its
//!   **noun** matches `<noun>`, and the name does NOT match `<exclude>`;
//! - an item collects **all** matching entries in document order, joined
//!   with commas (`"gem,valuable"`), not just the first.
//!
//! Names/nouns are the display strings from `<a exist noun>` links (no
//! article) — e.g. name "blue sapphire", noun "sapphire".
//!
//! The regexes are authored for Ruby. Each compiles with the fast `regex`
//! crate first, falling back to `fancy-regex` for lookarounds (currently
//! one pattern: jewelry's `(?!some)`); a pattern both engines reject is
//! skipped with a warning and counted, and a test pins that count at zero
//! against the bundled snapshot so upstream drift fails loudly.
//!
//! Content arrives through `core::data_pack` (Lich folder → local store →
//! bundled), so a Lich user's `;repo`-maintained copy wins automatically.

/// A compiled pattern: the fast engine when possible, the backtracking
/// engine for Ruby-only syntax.
enum Pattern {
    Std(regex::Regex),
    Fancy(fancy_regex::Regex),
}

impl Pattern {
    fn compile(source: &str) -> Result<Pattern, String> {
        match regex::Regex::new(source) {
            Ok(re) => Ok(Pattern::Std(re)),
            Err(_) => fancy_regex::Regex::new(source)
                .map(Pattern::Fancy)
                .map_err(|err| err.to_string()),
        }
    }

    fn is_match(&self, text: &str) -> bool {
        match self {
            Pattern::Std(re) => re.is_match(text),
            // Backtracking limits error out on pathological input; treat
            // that as a non-match rather than a crash.
            Pattern::Fancy(re) => re.is_match(text).unwrap_or(false),
        }
    }
}

/// One classification entry (a `<type>` or `<sellable>` section).
struct Entry {
    name: String,
    /// Matches the item's display name.
    name_re: Option<Pattern>,
    /// Matches the item's noun.
    noun_re: Option<Pattern>,
    /// Vetoes by display name (Lich never excludes by noun).
    exclude: Option<Pattern>,
}

impl Entry {
    fn matches(&self, item_name: &str, item_noun: &str) -> bool {
        let hit = self
            .name_re
            .as_ref()
            .is_some_and(|re| re.is_match(item_name))
            || self
                .noun_re
                .as_ref()
                .is_some_and(|re| re.is_match(item_noun));
        hit && !self
            .exclude
            .as_ref()
            .is_some_and(|re| re.is_match(item_name))
    }
}

pub struct GameObjData {
    types: Vec<Entry>,
    sellable: Vec<Entry>,
    /// Section names holding a regex neither engine could compile.
    pub skipped: Vec<String>,
}

impl GameObjData {
    /// Parse from XML. Never fails outright: malformed sections are
    /// dropped, uncompilable regexes are skipped and recorded.
    pub fn parse(xml: &str) -> GameObjData {
        use quick_xml::events::Event;

        let mut reader = quick_xml::Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut data = GameObjData {
            types: Vec::new(),
            sellable: Vec::new(),
            skipped: Vec::new(),
        };

        struct Building {
            is_sellable: bool,
            name: String,
            name_src: Option<String>,
            noun_src: Option<String>,
            exclude_src: Option<String>,
        }
        let mut current: Option<Building> = None;
        // Which child element's text we're inside ("name"/"noun"/"exclude").
        let mut pending: Option<String> = None;

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag.as_str() {
                        "type" | "sellable" => {
                            let name = e
                                .attributes()
                                .flatten()
                                .find(|a| a.key.as_ref() == b"name")
                                .and_then(|a| a.unescape_value().ok())
                                .map(|v| v.into_owned())
                                .unwrap_or_default();
                            current = Some(Building {
                                is_sellable: tag == "sellable",
                                name,
                                name_src: None,
                                noun_src: None,
                                exclude_src: None,
                            });
                        }
                        "name" | "noun" | "exclude" if current.is_some() => {
                            pending = Some(tag);
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(t)) => {
                    if let (Some(entry), Some(field)) = (&mut current, &pending) {
                        let text = t.unescape().unwrap_or_default().into_owned();
                        match field.as_str() {
                            "name" => entry.name_src = Some(text),
                            "noun" => entry.noun_src = Some(text),
                            "exclude" => entry.exclude_src = Some(text),
                            _ => {}
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag.as_str() {
                        "name" | "noun" | "exclude" => pending = None,
                        "type" | "sellable" => {
                            if let Some(b) = current.take() {
                                let entry = Entry {
                                    name: b.name.clone(),
                                    name_re: compile_or_skip(
                                        &mut data.skipped,
                                        &b.name,
                                        b.name_src.as_deref(),
                                        "name",
                                    ),
                                    noun_re: compile_or_skip(
                                        &mut data.skipped,
                                        &b.name,
                                        b.noun_src.as_deref(),
                                        "noun",
                                    ),
                                    exclude: compile_or_skip(
                                        &mut data.skipped,
                                        &b.name,
                                        b.exclude_src.as_deref(),
                                        "exclude",
                                    ),
                                };
                                if b.is_sellable {
                                    data.sellable.push(entry);
                                } else {
                                    data.types.push(entry);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(err) => {
                    tracing::warn!("gameobj-data.xml parse error: {}", err);
                    break;
                }
                _ => {}
            }
            buf.clear();
        }
        data
    }

    fn matching<'a>(entries: &'a [Entry], name: &str, noun: &str) -> Vec<&'a str> {
        entries
            .iter()
            .filter(|entry| entry.matches(name, noun))
            .map(|entry| entry.name.as_str())
            .collect()
    }

    /// All matching type tags in document order ("gem", "valuable", ...).
    pub fn types_of(&self, name: &str, noun: &str) -> Vec<&str> {
        Self::matching(&self.types, name, noun)
    }

    /// Comma-joined type string exactly like Lich's `GameObj#type`
    /// ("gem,valuable"), or None when nothing matches.
    pub fn classify(&self, name: &str, noun: &str) -> Option<String> {
        let matches = self.types_of(name, noun);
        (!matches.is_empty()).then(|| matches.join(","))
    }

    /// True when the item carries the given type tag (Lich's `type?`).
    pub fn is_type(&self, name: &str, noun: &str, tag: &str) -> bool {
        self.types_of(name, noun).contains(&tag)
    }

    /// Comma-joined sellable shops like Lich's `GameObj#sellable`.
    pub fn sellable(&self, name: &str, noun: &str) -> Option<String> {
        let matches = Self::matching(&self.sellable, name, noun);
        (!matches.is_empty()).then(|| matches.join(","))
    }

    pub fn type_count(&self) -> usize {
        self.types.len()
    }

    pub fn sellable_count(&self) -> usize {
        self.sellable.len()
    }
}

/// Compile one section regex, recording the section in `skipped` when
/// neither engine accepts it.
fn compile_or_skip(
    skipped: &mut Vec<String>,
    section: &str,
    source: Option<&str>,
    what: &str,
) -> Option<Pattern> {
    let source = source?;
    match Pattern::compile(source) {
        Ok(pattern) => Some(pattern),
        Err(err) => {
            tracing::warn!(
                "gameobj-data.xml: skipping {} regex for '{}': {}",
                what,
                section,
                err
            );
            skipped.push(section.to_string());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"<?xml version="1.0"?>
<data>
  <type name="gem">
    <name>^(blue sapphire|quartz crystal|(?:round amber|mist blue) sea glass disk)$</name>
  </type>
  <type name="valuable">
    <name>^(blue sapphire|silver ingot)$</name>
  </type>
  <type name="jewelry">
    <name>^(^(?!some).*\bplate$)$</name>
    <noun>^(pendant|ring)$</noun>
    <exclude>^(gold ring)$</exclude>
  </type>
  <type name="junk">
    <name>\b(rock|stick)\b</name>
    <exclude>^lucky rock$</exclude>
  </type>
  <sellable name="gemshop">
    <name>^(blue sapphire)$</name>
  </sellable>
</data>"#;

    #[test]
    fn matches_by_name_or_noun_with_exclude_veto() {
        let data = GameObjData::parse(FIXTURE);
        // Noun path: name regex misses, noun list hits.
        assert_eq!(
            data.classify("sapphire pendant", "pendant").as_deref(),
            Some("jewelry")
        );
        // Exclude vetoes by NAME even when the noun matched.
        assert_eq!(data.classify("gold ring", "ring"), None);
        // Name path with exclude veto; later entries still get a chance
        // (here none match, so the item is untyped).
        assert_eq!(data.classify("lucky rock", "rock"), None);
        assert_eq!(data.classify("smooth rock", "rock").as_deref(), Some("junk"));
        assert_eq!(data.classify("vultite greatsword", "greatsword"), None);
        assert_eq!(
            data.sellable("blue sapphire", "sapphire").as_deref(),
            Some("gemshop")
        );
    }

    #[test]
    fn multi_match_joins_all_types_in_document_order() {
        let data = GameObjData::parse(FIXTURE);
        assert_eq!(
            data.classify("blue sapphire", "sapphire").as_deref(),
            Some("gem,valuable")
        );
        assert!(data.is_type("blue sapphire", "sapphire", "gem"));
        assert!(data.is_type("blue sapphire", "sapphire", "valuable"));
        assert!(!data.is_type("blue sapphire", "sapphire", "junk"));
    }

    #[test]
    fn ruby_lookaheads_compile_via_fancy_fallback() {
        let data = GameObjData::parse(FIXTURE);
        assert!(data.skipped.is_empty(), "skipped: {:?}", data.skipped);
        // The (?!some) lookahead works: names ending in "plate" are
        // jewelry unless they start with "some".
        assert_eq!(
            data.classify("ornate silver plate", "plate").as_deref(),
            Some("jewelry")
        );
        assert_eq!(data.classify("some plate", "plate"), None);
    }

    #[test]
    fn bundled_snapshot_compiles_completely() {
        let data =
            GameObjData::parse(crate::core::data_pack::GAMEOBJ_DATA.bundled);
        assert!(data.type_count() >= 60, "types: {}", data.type_count());
        assert!(data.sellable_count() >= 3, "sellable: {}", data.sellable_count());
        // Every regex in the snapshot must compile on one of the two
        // engines. A future upstream copy that breaks this should fail
        // here, not silently drop categories.
        assert!(data.skipped.is_empty(), "skipped: {:?}", data.skipped);
        // Name path, noun path, and the lookahead against the real data.
        assert_eq!(data.classify("quartz crystal", "crystal").as_deref(), Some("gem"));
        assert!(data.is_type("sparkling ruby pendant", "pendant", "jewelry"));
        assert!(data.is_type("ornate silver plate", "plate", "jewelry"));
        assert!(!data.is_type("some plate", "plate", "jewelry"));
    }
}
