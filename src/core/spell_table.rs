//! Bundled GemStone spell reference table (the no-Lich path).
//!
//! Parsed once, lazily, from `defaults/globals/effect-list.xml` — the same
//! spell database Lich ships (`lich-5/data/effect-list.xml`). Only the
//! statically-usable parts are extracted: identity (number/name/type),
//! plain-integer costs, and the start/end message regex strings. Durations
//! are deliberately skipped: most are Ruby formulas needing Lich to
//! evaluate, and the live feed sends real expiry times anyway.
//!
//! Refresh flow (mapdb-style): replace the XML with a newer copy from the
//! lich-5 repo at release time. The parser ignores unknown elements, so
//! schema additions degrade gracefully.

use std::collections::HashMap;
use std::sync::OnceLock;

const EFFECT_LIST_XML: &str = include_str!("../../defaults/globals/effect-list.xml");

/// Static facts about one spell. Costs are `None` when the entry has no
/// such cost; `dynamic_cost` is set when any cost was a formula we cannot
/// evaluate (affordability checks fail closed on those).
#[derive(Debug, Clone, Default)]
pub struct SpellInfo {
    pub number: u16,
    pub name: String,
    /// Functional category ("attack", "defense", "utility", ...).
    pub spell_type: Option<String>,
    pub mana: Option<u16>,
    pub stamina: Option<u16>,
    pub spirit: Option<u16>,
    /// True when any cost element held a Ruby formula instead of a number.
    pub dynamic_cost: bool,
    /// Regex source strings for the lines shown when the effect starts/ends.
    pub start_messages: Vec<String>,
    pub end_messages: Vec<String>,
}

/// number -> spell, parsed on first use.
pub fn table() -> &'static HashMap<u16, SpellInfo> {
    static TABLE: OnceLock<HashMap<u16, SpellInfo>> = OnceLock::new();
    TABLE.get_or_init(|| parse_effect_list(EFFECT_LIST_XML))
}

/// Lookup by spell number.
pub fn spell(number: u16) -> Option<&'static SpellInfo> {
    table().get(&number)
}

fn parse_effect_list(xml: &str) -> HashMap<u16, SpellInfo> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut spells = HashMap::new();
    let mut current: Option<SpellInfo> = None;
    // (element, attr "type") whose text content we are waiting for.
    let mut pending: Option<(String, String)> = None;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let attr = |name: &str| -> Option<String> {
                    e.attributes().flatten().find_map(|a| {
                        (a.key.as_ref() == name.as_bytes())
                            .then(|| a.unescape_value().ok())
                            .flatten()
                            .map(|v| v.into_owned())
                    })
                };
                match tag.as_str() {
                    "spell" => {
                        let number = attr("number").and_then(|n| n.parse::<u16>().ok());
                        current = number.map(|number| SpellInfo {
                            number,
                            name: attr("name").unwrap_or_default(),
                            spell_type: attr("type"),
                            ..Default::default()
                        });
                    }
                    "cost" | "message" if current.is_some() => {
                        pending = attr("type").map(|kind| (tag, kind));
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if let (Some(spell), Some((element, kind))) = (&mut current, &pending) {
                    let text = t.unescape().map(|s| s.into_owned()).unwrap_or_default();
                    match element.as_str() {
                        "cost" => match text.trim().parse::<u16>() {
                            Ok(value) => match kind.as_str() {
                                "mana" => spell.mana = Some(value),
                                "stamina" => spell.stamina = Some(value),
                                "spirit" => spell.spirit = Some(value),
                                _ => {} // "renew" and friends: not needed
                            },
                            // Ruby formula: affordability is unknowable here.
                            Err(_) if matches!(kind.as_str(), "mana" | "stamina" | "spirit") => {
                                spell.dynamic_cost = true;
                            }
                            Err(_) => {}
                        },
                        "message" => match kind.as_str() {
                            "start" => spell.start_messages.push(text),
                            "end" => spell.end_messages.push(text),
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                match e.name().as_ref() {
                    b"spell" => {
                        if let Some(spell) = current.take() {
                            spells.insert(spell.number, spell);
                        }
                    }
                    b"cost" | b"message" => pending = None,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                tracing::warn!("effect-list.xml parse stopped: {}", err);
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    spells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_parses_the_bundled_database() {
        let table = table();
        // 511 numbered spells in the current data; allow drift on refresh.
        assert!(table.len() > 450, "got {} spells", table.len());

        // Spirit Warding I: static mana cost, messages, Ruby duration skipped.
        let sw1 = spell(101).expect("spell 101");
        assert_eq!(sw1.name, "Spirit Warding I");
        assert_eq!(sw1.spell_type.as_deref(), Some("defense"));
        assert_eq!(sw1.mana, Some(1));
        assert!(!sw1.dynamic_cost);
        assert!(sw1
            .start_messages
            .iter()
            .any(|m| m.contains("light blue glow")));
        assert!(!sw1.end_messages.is_empty());
    }

    #[test]
    fn formula_costs_mark_dynamic_and_fail_closed_data() {
        // Song of Luck (1006): bard cost formula -> dynamic_cost.
        let song = spell(1006).expect("spell 1006");
        assert!(song.dynamic_cost);
    }
}
