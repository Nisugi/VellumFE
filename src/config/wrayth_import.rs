//! Import highlights from a Wrayth/StormFront settings XML file.
//!
//! Wrayth stores per-character settings in an XML file named after the
//! character id (e.g. `70682.xml`). Highlights live in two sections:
//!
//! - `<strings>` — one `<h>` per highlight string
//! - `<names>`   — one `<h>` per highlighted name (players, etc.)
//!
//! `<h>` attributes: `text` (the literal to match), `color` / `bgcolor`
//! (either `#rrggbb`, a palette reference like `@13`, `"skin"`, or empty),
//! `line="y"` (color the whole line), `case="y"` (case-sensitive in Wrayth),
//! and `sound` (absolute path to a sound file on the original machine).
//!
//! Palette references resolve through the `<palette>` section
//! (`<i id="13" color="#39CC00"/>`).
//!
//! Wrayth highlights are literal strings, so they map onto `fast_parse`
//! (Aho-Corasick) patterns — the cheapest kind for the highlight engine.
//! Name entries are merged into one pattern per distinct style, since a
//! typical file has hundreds of names sharing a single color pair.

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

use super::{HighlightPattern, RedirectMode};

/// A raw `<h>` entry from the settings file, before conversion.
#[derive(Debug, Clone)]
struct RawHighlight {
    text: String,
    color: Option<String>,
    bgcolor: Option<String>,
    whole_line: bool,
    sound: Option<String>,
}

/// Result of importing a Wrayth settings file.
pub struct WraythImport {
    /// Converted highlights in output order (strings first, then name groups).
    pub highlights: Vec<(String, HighlightPattern)>,
    /// How many `<h>` entries came from `<strings>`.
    pub string_count: usize,
    /// How many names were merged into how many grouped patterns.
    pub name_count: usize,
    pub name_group_count: usize,
    /// Entries skipped for having no usable text.
    pub skipped: usize,
    /// Palette references (`@N`) that had no `<palette>` entry.
    pub palette_misses: Vec<String>,
    /// Distinct sound file basenames referenced by imported highlights.
    /// These must be copied into the sounds directory by hand.
    pub sound_files: Vec<String>,
}

/// Parse a Wrayth settings XML string and convert its highlight sections
/// into VellumFE highlight patterns.
pub fn import_wrayth_settings(xml: &str) -> Result<WraythImport> {
    let (palette, strings, names) = parse_sections(xml)?;

    let mut palette_misses = Vec::new();
    let mut sound_files = Vec::new();
    let mut highlights: Vec<(String, HighlightPattern)> = Vec::new();
    let mut used_keys: HashMap<String, u32> = HashMap::new();
    let mut skipped = 0usize;

    // <strings>: one pattern per entry, keyed by a slug of the matched text.
    let string_count = strings.len();
    for raw in &strings {
        if raw.text.trim().is_empty() {
            skipped += 1;
            continue;
        }
        let mut pattern = convert_entry(raw, &palette, &mut palette_misses);
        pattern.category = Some("wrayth".to_string());
        if let Some(sound) = &pattern.sound {
            if !sound_files.contains(sound) {
                sound_files.push(sound.clone());
            }
        }
        let key = unique_key(&format!("wrayth_{}", slug(&raw.text)), &mut used_keys);
        highlights.push((key, pattern));
    }

    // <names>: merge into one fast_parse pattern per distinct style, since
    // hundreds of names typically share a single color pair.
    let mut name_groups: Vec<((Option<String>, Option<String>, bool), Vec<String>)> = Vec::new();
    let mut name_count = 0usize;
    for raw in &names {
        if raw.text.trim().is_empty() {
            skipped += 1;
            continue;
        }
        name_count += 1;
        let fg = resolve_color(raw.color.as_deref(), &palette, &mut palette_misses);
        let bg = resolve_color(raw.bgcolor.as_deref(), &palette, &mut palette_misses);
        let style = (fg, bg, raw.whole_line);
        match name_groups.iter_mut().find(|(s, _)| *s == style) {
            Some((_, texts)) => texts.push(raw.text.clone()),
            None => name_groups.push((style, vec![raw.text.clone()])),
        }
    }
    let name_group_count = name_groups.len();
    for (i, ((fg, bg, whole_line), texts)) in name_groups.into_iter().enumerate() {
        let pattern = HighlightPattern {
            pattern: texts.join("|"),
            fg,
            bg,
            bold: false,
            color_entire_line: whole_line,
            fast_parse: true,
            sound: None,
            sound_volume: None,
            category: Some("wrayth-names".to_string()),
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        };
        let key = unique_key(&format!("wrayth_names_{:02}", i + 1), &mut used_keys);
        highlights.push((key, pattern));
    }

    palette_misses.sort();
    palette_misses.dedup();

    Ok(WraythImport {
        highlights,
        string_count,
        name_count,
        name_group_count,
        skipped,
        palette_misses,
        sound_files,
    })
}

/// Serialize imported highlights as a highlights.toml document, preserving
/// import order.
pub fn to_toml_string(highlights: &[(String, HighlightPattern)]) -> Result<String> {
    let mut root = toml::map::Map::new();
    for (key, pattern) in highlights {
        let value = toml::Value::try_from(pattern).context("Failed to serialize highlight")?;
        root.insert(key.clone(), value);
    }
    toml::to_string_pretty(&toml::Value::Table(root)).context("Failed to render highlights TOML")
}

/// Walk the XML and collect the palette plus raw `<h>` entries from the
/// `<strings>` and `<names>` sections.
fn parse_sections(
    xml: &str,
) -> Result<(HashMap<u32, String>, Vec<RawHighlight>, Vec<RawHighlight>)> {
    #[derive(PartialEq)]
    enum Section {
        None,
        Strings,
        Names,
        Palette,
    }

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut section = Section::None;
    let mut palette: HashMap<u32, String> = HashMap::new();
    let mut strings = Vec::new();
    let mut names = Vec::new();

    loop {
        match reader.read_event().context("Malformed settings XML")? {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                let tag = e.name();
                match tag.as_ref() {
                    b"strings" => section = Section::Strings,
                    b"names" => section = Section::Names,
                    b"palette" => section = Section::Palette,
                    b"i" if section == Section::Palette => {
                        let mut id = None;
                        let mut color = None;
                        for attr in e.attributes().flatten() {
                            let value = attr.unescape_value()?.into_owned();
                            match attr.key.as_ref() {
                                b"id" => id = value.parse::<u32>().ok(),
                                b"color" => color = Some(value),
                                _ => {}
                            }
                        }
                        if let (Some(id), Some(color)) = (id, color) {
                            palette.insert(id, color);
                        }
                    }
                    b"h" if section == Section::Strings || section == Section::Names => {
                        let mut raw = RawHighlight {
                            text: String::new(),
                            color: None,
                            bgcolor: None,
                            whole_line: false,
                            sound: None,
                        };
                        for attr in e.attributes().flatten() {
                            let value = attr.unescape_value()?.into_owned();
                            match attr.key.as_ref() {
                                b"text" => raw.text = value,
                                b"color" => raw.color = Some(value),
                                b"bgcolor" => raw.bgcolor = Some(value),
                                b"line" => raw.whole_line = value == "y",
                                b"sound" if !value.is_empty() => raw.sound = Some(value),
                                _ => {}
                            }
                        }
                        if section == Section::Strings {
                            strings.push(raw);
                        } else {
                            names.push(raw);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                if matches!(e.name().as_ref(), b"strings" | b"names" | b"palette") {
                    section = Section::None;
                }
            }
            _ => {}
        }
    }

    Ok((palette, strings, names))
}

fn convert_entry(
    raw: &RawHighlight,
    palette: &HashMap<u32, String>,
    palette_misses: &mut Vec<String>,
) -> HighlightPattern {
    // Literal text with no '|' maps onto fast_parse (Aho-Corasick). A '|'
    // would be split by the fast_parse engine, so those fall back to an
    // escaped regex to stay a single literal match.
    let (pattern, fast_parse) = if raw.text.contains('|') {
        (regex::escape(&raw.text), false)
    } else {
        (raw.text.clone(), true)
    };

    HighlightPattern {
        pattern,
        fg: resolve_color(raw.color.as_deref(), palette, palette_misses),
        bg: resolve_color(raw.bgcolor.as_deref(), palette, palette_misses),
        bold: false,
        color_entire_line: raw.whole_line,
        fast_parse,
        sound: raw.sound.as_deref().map(sound_basename),
        sound_volume: None,
        category: None,
        squelch: false,
        silent_prompt: false,
        redirect_to: None,
        redirect_mode: RedirectMode::default(),
        replace: None,
        stream: None,
        window: None,
        compiled_regex: None,
    }
}

/// Resolve a Wrayth color attribute to a hex string usable in highlights.toml.
/// Empty and "skin" (inherit the default skin color) both mean no override.
fn resolve_color(
    value: Option<&str>,
    palette: &HashMap<u32, String>,
    palette_misses: &mut Vec<String>,
) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("skin") {
        return None;
    }
    if let Some(index) = value.strip_prefix('@') {
        return match index.parse::<u32>().ok().and_then(|i| palette.get(&i)) {
            Some(color) => Some(color.clone()),
            None => {
                palette_misses.push(value.to_string());
                None
            }
        };
    }
    Some(value.to_string())
}

/// Wrayth stores absolute sound paths from the original machine; VellumFE
/// plays sounds by filename from its sounds directory, so keep the basename.
fn sound_basename(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
}

/// Reduce highlight text to a short TOML-key-safe slug.
fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = true;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "entry".to_string()
    } else {
        out
    }
}

fn unique_key(base: &str, used: &mut HashMap<String, u32>) -> String {
    let count = used.entry(base.to_string()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base.to_string()
    } else {
        format!("{}_{}", base, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"<settings client='1.0.1.28' major='1570'>
<strings>
<h bgcolor="" color="#ff8080" text="GSIV"/>
<h bgcolor="" color="#00ffff" line="y" text="[Help]"/>
<h color="@13" text="AS:" line="y"/>
<h bgcolor="" color="#800000" line="y" text="Striking with a serpent&apos;s quickness"/>
<h bgcolor="" color="skin" text=""/>
<h bgcolor="" color="@999" text="missing palette"/>
<h bgcolor="" color="#ffffff" sound="C:\Users\Shawn\Desktop\Lich5\fx\Windows Ding.wav" text="You hear a ding"/>
<h bgcolor="" color="#ffffff" text="either|or"/>
</strings>
<names>
<h bgcolor="#311c1c" case="y" color="#ecc013" text="Bastique"/>
<h bgcolor="#311c1c" case="y" color="#ecc013" text="Goblyn"/>
<h bgcolor="" color="#00ff00" text="Nisugi"/>
</names>
<palette><i id="13" color="#39CC00"/></palette>
</settings>"##;

    fn find<'a>(
        result: &'a WraythImport,
        key: &str,
    ) -> &'a HighlightPattern {
        &result
            .highlights
            .iter()
            .find(|(k, _)| k == key)
            .unwrap_or_else(|| panic!("missing key {key}"))
            .1
    }

    #[test]
    fn test_import_strings_basic() {
        let result = import_wrayth_settings(SAMPLE).unwrap();
        assert_eq!(result.string_count, 8);
        assert_eq!(result.skipped, 1); // the empty-text entry

        let gsiv = find(&result, "wrayth_gsiv");
        assert_eq!(gsiv.pattern, "GSIV");
        assert_eq!(gsiv.fg.as_deref(), Some("#ff8080"));
        assert_eq!(gsiv.bg, None); // bgcolor="" means no override
        assert!(gsiv.fast_parse);
        assert!(!gsiv.color_entire_line);
        assert_eq!(gsiv.category.as_deref(), Some("wrayth"));

        let help = find(&result, "wrayth_help");
        assert!(help.color_entire_line);
    }

    #[test]
    fn test_palette_reference_resolution() {
        let result = import_wrayth_settings(SAMPLE).unwrap();
        let as_line = find(&result, "wrayth_as");
        assert_eq!(as_line.fg.as_deref(), Some("#39CC00"));

        // Unknown palette index is dropped and reported
        let miss = find(&result, "wrayth_missing_palette");
        assert_eq!(miss.fg, None);
        assert_eq!(result.palette_misses, vec!["@999".to_string()]);
    }

    #[test]
    fn test_entity_unescaping() {
        let result = import_wrayth_settings(SAMPLE).unwrap();
        let (_, serpent) = result
            .highlights
            .iter()
            .find(|(_, p)| p.pattern.starts_with("Striking"))
            .unwrap();
        assert_eq!(serpent.pattern, "Striking with a serpent's quickness");
    }

    #[test]
    fn test_sound_basename() {
        let result = import_wrayth_settings(SAMPLE).unwrap();
        let ding = find(&result, "wrayth_you_hear_a_ding");
        assert_eq!(ding.sound.as_deref(), Some("Windows Ding.wav"));
        assert_eq!(result.sound_files, vec!["Windows Ding.wav".to_string()]);
    }

    #[test]
    fn test_pipe_text_falls_back_to_regex() {
        let result = import_wrayth_settings(SAMPLE).unwrap();
        let pipe = find(&result, "wrayth_either_or");
        assert!(!pipe.fast_parse);
        assert_eq!(pipe.pattern, r"either\|or");
    }

    #[test]
    fn test_names_grouped_by_style() {
        let result = import_wrayth_settings(SAMPLE).unwrap();
        assert_eq!(result.name_count, 3);
        assert_eq!(result.name_group_count, 2);

        let group1 = find(&result, "wrayth_names_01");
        assert_eq!(group1.pattern, "Bastique|Goblyn");
        assert_eq!(group1.fg.as_deref(), Some("#ecc013"));
        assert_eq!(group1.bg.as_deref(), Some("#311c1c"));
        assert!(group1.fast_parse);
        assert_eq!(group1.category.as_deref(), Some("wrayth-names"));

        let group2 = find(&result, "wrayth_names_02");
        assert_eq!(group2.pattern, "Nisugi");
        assert_eq!(group2.fg.as_deref(), Some("#00ff00"));
        assert_eq!(group2.bg, None);
    }

    #[test]
    fn test_toml_round_trip() {
        let result = import_wrayth_settings(SAMPLE).unwrap();
        let toml_str = to_toml_string(&result.highlights).unwrap();
        let parsed: std::collections::HashMap<String, HighlightPattern> =
            toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.len(), result.highlights.len());
        assert_eq!(parsed["wrayth_gsiv"].fg.as_deref(), Some("#ff8080"));
    }

    #[test]
    fn test_duplicate_slugs_get_suffixes() {
        let xml = r##"<settings><strings>
            <h color="#ffffff" text="[Merchant]"/>
            <h color="#ffffff" text="[Merchant]-"/>
            <h color="#ffffff" text="[Merchant]"/>
        </strings></settings>"##;
        let result = import_wrayth_settings(xml).unwrap();
        let keys: Vec<&str> = result.highlights.iter().map(|(k, _)| k.as_str()).collect();
        // "[Merchant]" and "[Merchant]-" slug identically; all three stay distinct
        assert_eq!(keys, vec!["wrayth_merchant", "wrayth_merchant_2", "wrayth_merchant_3"]);
    }
}
