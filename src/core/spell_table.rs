//! GemStone spell reference table (the no-Lich path).
//!
//! Parsed lazily from `effect-list.xml` — the same spell database Lich ships
//! (`lich-5/data/effect-list.xml`). Only the statically-usable parts are
//! extracted: identity (number/name/type), plain-integer costs, and the
//! start/end message regex strings. Durations are deliberately skipped: most
//! are Ruby formulas needing Lich to evaluate, and the live feed sends real
//! expiry times anyway.
//!
//! Source resolution (like the data pack): a user-installed
//! `~/.vellum-fe/global/data/effect-list.xml` (dropped in, or downloaded by
//! `.jinx install effect-list.xml`) is preferred over the bundled default.
//! [`reload`] re-reads from disk and swaps the table in place, so a fresh
//! install takes effect without a restart. The parser ignores unknown
//! elements, so schema additions degrade gracefully.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

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

/// The live table, behind an `RwLock` so `.jinx install effect-list.xml` can
/// swap in a newer copy without a restart. An `Arc` inside lets `table()`
/// hand out a cheap snapshot without holding the lock.
static TABLE: RwLock<Option<Arc<HashMap<u16, SpellInfo>>>> = RwLock::new(None);

/// The effect-list XML to parse: a user copy in `global/data/` if present,
/// else the bundled default. Mirrors the data pack's local-store-over-bundled
/// preference (`core/data_pack.rs`).
fn resolve_xml() -> std::borrow::Cow<'static, str> {
    if let Ok(dir) = crate::config::Config::global_data_dir() {
        let path = dir.join("effect-list.xml");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            return std::borrow::Cow::Owned(contents);
        }
    }
    std::borrow::Cow::Borrowed(EFFECT_LIST_XML)
}

/// The whole table, parsed on first use from the resolved source.
pub fn table() -> Arc<HashMap<u16, SpellInfo>> {
    if let Some(table) = TABLE.read().unwrap().as_ref() {
        return Arc::clone(table);
    }
    // First access: parse and cache. A racing thread may parse too; last
    // writer wins and both see an identical table.
    let parsed = Arc::new(parse_effect_list(&resolve_xml()));
    *TABLE.write().unwrap() = Some(Arc::clone(&parsed));
    parsed
}

/// Re-read effect-list.xml from disk (preferring the installed copy) and swap
/// the table. Called after `.jinx install effect-list.xml`. Returns the spell
/// count for reporting.
pub fn reload() -> usize {
    let parsed = Arc::new(parse_effect_list(&resolve_xml()));
    let count = parsed.len();
    *TABLE.write().unwrap() = Some(parsed);
    count
}

/// Lookup by spell number. Returns an owned copy so callers don't hold the
/// table lock; `SpellInfo` is small and clones cheaply.
pub fn spell(number: u16) -> Option<SpellInfo> {
    table().get(&number).cloned()
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

    #[test]
    fn reload_prefers_installed_copy_then_falls_back() {
        // VELLUM_FE_DIR is process-global; serialize against every other env
        // test on the one shared lock (per-module locks don't mutually exclude).
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());

        // With no installed copy, reload uses the bundled default (full db).
        let bundled = reload();
        assert!(bundled > 450, "bundled db should be full, got {bundled}");

        // Install a tiny effect-list.xml into global/data/; reload prefers it.
        let data_dir = crate::config::Config::global_data_dir().unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(
            data_dir.join("effect-list.xml"),
            r#"<spells><spell number="999" name="Test Spell"><type>utility</type></spell></spells>"#,
        )
        .unwrap();
        let installed = reload();
        assert_eq!(installed, 1, "should read the 1-spell installed copy");
        assert_eq!(spell(999).unwrap().name, "Test Spell");
        assert!(spell(101).is_none(), "bundled spells gone after swap");

        // Removing the installed copy and reloading falls back to bundled.
        std::fs::remove_file(data_dir.join("effect-list.xml")).unwrap();
        let back = reload();
        assert_eq!(back, bundled);

        std::env::remove_var("VELLUM_FE_DIR");
    }
}
