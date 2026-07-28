//! The single anchor parser + container-query helpers for the registry.
//!
//! There were two independent `<a exist noun>name</a>` parsers in the tree
//! (`ContainerData::parsed_items` and the TUI `container_window`'s
//! `parse_container_item`), and they drifted — the `.foreach` audit found
//! a header-skip bug fixed in one but not the other. This is the one
//! parser both will use once migration completes.
//!
//! It tolerates any attribute order, extra attributes, nested style tags
//! inside the name (styled item names render as `<preset>name</preset>`),
//! and decodes the entities the game uses.

use super::GameItem;

/// Parse a single `<a>` anchor into a `GameItem`, or None if the string
/// holds no well-formed anchor. Reads the first anchor only (one item per
/// `<inv>` line in the protocol).
pub fn parse_anchor(line: &str) -> Option<GameItem> {
    use std::sync::OnceLock;
    // exist before noun (the common order)...
    static FWD: OnceLock<regex::Regex> = OnceLock::new();
    let fwd = FWD.get_or_init(|| {
        regex::Regex::new(
            r#"<a\b[^>]*?\bexist=["'](-?\d+)["'][^>]*?\bnoun=["']([^"']*)["'][^>]*>(.*?)</a>"#,
        )
        .expect("static anchor regex")
    });
    // ...and noun before exist.
    static REV: OnceLock<regex::Regex> = OnceLock::new();
    let rev = REV.get_or_init(|| {
        regex::Regex::new(
            r#"<a\b[^>]*?\bnoun=["']([^"']*)["'][^>]*?\bexist=["'](-?\d+)["'][^>]*>(.*?)</a>"#,
        )
        .expect("static anchor regex")
    });

    let (id, noun, inner) = if let Some(caps) = fwd.captures(line) {
        (caps[1].to_string(), caps[2].to_string(), caps[3].to_string())
    } else if let Some(caps) = rev.captures(line) {
        (caps[2].to_string(), caps[1].to_string(), caps[3].to_string())
    } else {
        return None;
    };

    Some(GameItem {
        id,
        noun: decode_entities(&noun),
        name: decode_entities(&strip_inner_tags(&inner)).trim().to_string(),
        weight: None,
    })
}

/// True when a container-look line is the header ("In the / On the /
/// Peering into ... you see"), or its anchor is the container's own object
/// target (needed for `stow`, whose stream id != the header anchor id).
/// `container_target` is the container's `command_target()`.
pub fn is_header_line(line: &str, container_target: &str) -> bool {
    let lower = line.trim().to_lowercase();
    if lower.starts_with("in the ")
        || lower.starts_with("on the ")
        || lower.starts_with("peering into ")
    {
        return true;
    }
    parse_anchor(line).is_some_and(|item| item.id == container_target)
}

/// Remove nested tags from an anchor's inner content, keeping visible text.
pub(super) fn strip_inner_tags(inner: &str) -> String {
    if !inner.contains('<') {
        return inner.to_string();
    }
    let mut out = String::with_capacity(inner.len());
    let mut in_tag = false;
    for ch in inner.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Decode the handful of XML entities the game uses in item text.
pub(super) fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

/// Lowercase a container query and drop a single leading article
/// (`my`/`the`/`a`/`an`).
pub(super) fn strip_articles(query: &str) -> Vec<String> {
    let mut words: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if words
        .first()
        .is_some_and(|w| matches!(w.as_str(), "my" | "the" | "a" | "an"))
    {
        words.remove(0);
    }
    words
}

/// True when every query word is a prefix of some title word, consumed in
/// order (subsequence). Lets "bando" match "bandolier".
pub(super) fn title_matches_words(title_lower: &str, query_words: &[String]) -> bool {
    if query_words.is_empty() {
        return false;
    }
    let title_words: Vec<&str> = title_lower.split_whitespace().collect();
    let mut ti = 0;
    for qw in query_words {
        let mut matched = false;
        while ti < title_words.len() {
            let tw = title_words[ti];
            ti += 1;
            if tw.starts_with(qw.as_str()) {
                matched = true;
                break;
            }
        }
        if !matched {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_tolerates_order_attributes_nesting_entities() {
        // normal
        let i = parse_anchor(r#" a <a exist="101" noun="crystal">quartz crystal</a>"#).unwrap();
        assert_eq!((i.id.as_str(), i.noun.as_str(), i.name.as_str()),
                   ("101", "crystal", "quartz crystal"));
        // extra attribute after noun
        let i = parse_anchor(r#" a <a exist="102" noun="rod" class="x">iridian rod</a>"#).unwrap();
        assert_eq!(i.name, "iridian rod");
        // nested style tags
        let i = parse_anchor(r#" a <a exist="103" noun="orb"><b>glowing orb</b></a>"#).unwrap();
        assert_eq!(i.name, "glowing orb");
        // entity
        let i = parse_anchor(r#" a <a exist="104" noun="knife">knife &amp; sheath</a>"#).unwrap();
        assert_eq!(i.name, "knife & sheath");
        // noun before exist
        let i = parse_anchor(r#" a <a noun="cube" exist="105">black cube</a>"#).unwrap();
        assert_eq!((i.id.as_str(), i.noun.as_str()), ("105", "cube"));
        // no anchor
        assert!(parse_anchor("You pick up a rock.").is_none());
    }

    #[test]
    fn header_detection_covers_prose_forms_and_stow() {
        // Prose forms are headers regardless of anchor id.
        assert!(is_header_line(
            r#"In the <a exist="77" noun="bag">bag</a>:"#,
            "77"
        ));
        assert!(is_header_line(
            r#"On the <a exist="5" noun="counter">counter</a>:"#,
            "77"
        ));
        // stow: prose catches it, and so does the target match.
        assert!(is_header_line(
            r#"In the <a exist="691" noun="shroud">shroud</a>:"#,
            "691"
        ));
        // A real item line is not a header.
        assert!(!is_header_line(
            r#" a <a exist="999" noun="sword">short sword</a>"#,
            "77"
        ));
    }
}
