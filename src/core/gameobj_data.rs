//! Item classification from Lich's `gameobj-data.xml` (the no-Lich path).
//!
//! The file is a list of `<type name="gem">` entries, each holding one
//! `<name>` regex (a large alternation) and optionally an `<exclude>`
//! regex, plus parallel `<sellable name="gemshop">` sections in the same
//! shape. Classification mirrors Lich's `GameObj#type`: first entry in
//! document order whose name regex matches and whose exclude regex (if
//! any) does not. Matching runs against the item's display name as sent
//! in `<a exist noun>` link text (no article) — e.g. "blue sapphire".
//!
//! The regexes are authored for Ruby. The `regex` crate compiles all but
//! the rare lookaround; incompatible patterns are skipped with a warning
//! and counted, and a test pins the skip count against the bundled
//! snapshot so upstream drift fails loudly instead of silently dropping
//! categories (see the accuracy-over-performance house rule).
//!
//! Content arrives through `core::data_pack` (Lich folder → local store →
//! bundled), so a Lich user's `;repo`-maintained copy wins automatically.

use regex::Regex;

/// One classification entry (a `<type>` or `<sellable>` section).
struct Entry {
    name: String,
    /// None when the section had no `<name>` or its regex didn't compile
    /// (entry then never matches, like Lich with a nil pattern).
    matcher: Option<Regex>,
    exclude: Option<Regex>,
}

pub struct GameObjData {
    types: Vec<Entry>,
    sellable: Vec<Entry>,
    /// Section names whose regex failed to compile (Ruby-only syntax).
    pub skipped: Vec<String>,
}

impl GameObjData {
    /// Parse from XML. Never fails outright: malformed sections are
    /// dropped, incompatible regexes are skipped and recorded.
    pub fn parse(xml: &str) -> GameObjData {
        use quick_xml::events::Event;

        let mut reader = quick_xml::Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut data = GameObjData {
            types: Vec::new(),
            sellable: Vec::new(),
            skipped: Vec::new(),
        };

        // (is_sellable, entry name, name pattern, exclude pattern)
        let mut current: Option<(bool, String, Option<String>, Option<String>)> = None;
        // Which child element's text we're inside ("name"/"exclude").
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
                            current = Some((tag == "sellable", name, None, None));
                        }
                        "name" | "exclude" if current.is_some() => {
                            pending = Some(tag);
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(t)) => {
                    if let (Some(entry), Some(field)) = (&mut current, &pending) {
                        let text = t.unescape().unwrap_or_default().into_owned();
                        match field.as_str() {
                            "name" => entry.2 = Some(text),
                            "exclude" => entry.3 = Some(text),
                            _ => {}
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag.as_str() {
                        "name" | "exclude" => pending = None,
                        "type" | "sellable" => {
                            if let Some((is_sellable, name, pattern, exclude)) =
                                current.take()
                            {
                                let entry = data.build_entry(name, pattern, exclude);
                                if is_sellable {
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

    fn build_entry(
        &mut self,
        name: String,
        pattern: Option<String>,
        exclude: Option<String>,
    ) -> Entry {
        let mut compile = |source: Option<String>, what: &str| -> Option<Regex> {
            let source = source?;
            match Regex::new(&source) {
                Ok(re) => Some(re),
                Err(err) => {
                    tracing::warn!(
                        "gameobj-data.xml: skipping {} regex for '{}': {}",
                        what,
                        name,
                        err
                    );
                    self.skipped.push(name.clone());
                    None
                }
            }
        };
        let matcher = compile(pattern, "name");
        let exclude = compile(exclude, "exclude");
        Entry {
            name,
            matcher,
            exclude,
        }
    }

    fn first_match<'a>(entries: &'a [Entry], item_name: &str) -> Option<&'a str> {
        entries
            .iter()
            .find(|entry| {
                entry
                    .matcher
                    .as_ref()
                    .is_some_and(|re| re.is_match(item_name))
                    && !entry
                        .exclude
                        .as_ref()
                        .is_some_and(|re| re.is_match(item_name))
            })
            .map(|entry| entry.name.as_str())
    }

    /// Item type ("gem", "box", ...) for a display name, or None.
    pub fn classify(&self, item_name: &str) -> Option<&str> {
        Self::first_match(&self.types, item_name)
    }

    /// Shop kind ("gemshop", "pawnshop", ...) the item sells at, or None.
    pub fn sellable(&self, item_name: &str) -> Option<&str> {
        Self::first_match(&self.sellable, item_name)
    }

    pub fn type_count(&self) -> usize {
        self.types.len()
    }

    pub fn sellable_count(&self) -> usize {
        self.sellable.len()
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
  <type name="junk">
    <name>\b(rock|stick)\b</name>
    <exclude>^lucky rock$</exclude>
  </type>
  <type name="broken">
    <name>^(^(?!some).*\bplate$)$</name>
  </type>
  <sellable name="gemshop">
    <name>^(blue sapphire)$</name>
  </sellable>
</data>"#;

    #[test]
    fn classify_first_match_and_exclude() {
        let data = GameObjData::parse(FIXTURE);
        assert_eq!(data.classify("blue sapphire"), Some("gem"));
        assert_eq!(data.classify("mist blue sea glass disk"), Some("gem"));
        assert_eq!(data.classify("smooth rock"), Some("junk"));
        // Exclude vetoes this entry; later entries still get a chance
        // (here none match, so the item is untyped).
        assert_eq!(data.classify("lucky rock"), None);
        assert_eq!(data.classify("vultite greatsword"), None);
        assert_eq!(data.sellable("blue sapphire"), Some("gemshop"));
        assert_eq!(data.sellable("smooth rock"), None);
    }

    #[test]
    fn incompatible_regex_is_skipped_not_fatal() {
        let data = GameObjData::parse(FIXTURE);
        // The Ruby lookahead entry parses as a never-matching section.
        assert_eq!(data.skipped, vec!["broken".to_string()]);
        assert_eq!(data.type_count(), 3);
        assert_eq!(data.classify("steel breastplate"), None);
    }

    #[test]
    fn bundled_snapshot_parses_with_known_compat() {
        let data =
            GameObjData::parse(crate::core::data_pack::GAMEOBJ_DATA.bundled);
        // 66 type + 12-ish sellable sections in the 2026-07 snapshot; keep
        // the floor loose but the incompatibility count EXACT — a new
        // Ruby-only regex upstream must fail this test, not vanish.
        assert!(data.type_count() >= 60, "types: {}", data.type_count());
        assert!(data.sellable_count() >= 3, "sellable: {}", data.sellable_count());
        assert_eq!(
            data.skipped,
            vec!["jewelry".to_string()],
            "regex compatibility drift vs bundled gameobj-data.xml"
        );
        // Spot checks against the live file's own alternations.
        assert_eq!(data.classify("quartz crystal"), Some("gem"));
    }
}
