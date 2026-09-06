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

/// One message-derived creature effect (`<effect availability="creature">`
/// in effect-list.xml): bleeding and friends. Unlike crtrStatus flags these
/// come from lossy combat messaging, so every one carries a timeout — a
/// missed end message can never leave a stale layer. A start match re-arms
/// the timer (Major Bleed ticks often); it must refresh, not stack.
#[derive(Debug, Clone)]
pub struct CreatureEffectSpec {
    /// Status name injected into the creature's open-vocabulary statuses
    /// ("bleeding") — drives `crtr_status` conditions, badges, and art.
    pub name: String,
    /// Seconds an unrefreshed instance survives (`<duration>`, integer).
    pub timeout_s: u32,
    /// Start messages: (compiled regex, severity rank 1-3).
    pub starts: Vec<(regex::Regex, u8)>,
    /// End messages.
    pub ends: Vec<regex::Regex>,
}

/// Everything parsed from effect-list.xml: the spell reference plus the
/// creature-effect specs. Derefs to the spell map for existing callers.
#[derive(Default)]
pub struct Tables {
    spells: HashMap<u16, SpellInfo>,
    creature_effects: Vec<CreatureEffectSpec>,
}

/// The live table, behind an `RwLock` so `.jinx install effect-list.xml` can
/// swap in a newer copy without a restart. An `Arc` inside lets `table()`
/// hand out a cheap snapshot without holding the lock.
static TABLE: RwLock<Option<Arc<Tables>>> = RwLock::new(None);

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

/// Parsed tables snapshot, parsed on first use from the resolved source.
fn tables() -> Arc<Tables> {
    if let Some(table) = TABLE.read().unwrap().as_ref() {
        return Arc::clone(table);
    }
    // First access: parse and cache. A racing thread may parse too; last
    // writer wins and both see an identical table.
    let parsed = Arc::new(parse_effect_list(&resolve_xml()));
    *TABLE.write().unwrap() = Some(Arc::clone(&parsed));
    parsed
}

/// The spell reference table.
pub fn table() -> Arc<Tables> {
    tables()
}

impl std::ops::Deref for Tables {
    type Target = HashMap<u16, SpellInfo>;
    fn deref(&self) -> &Self::Target {
        &self.spells
    }
}

/// Message-derived creature effect specs (bleeding and friends). Empty
/// until an effect-list.xml with `<effect availability="creature">` entries
/// is installed; the scanner is a no-op then.
pub fn creature_effects() -> Arc<Tables> {
    tables()
}

impl Tables {
    pub fn creature_effects(&self) -> &[CreatureEffectSpec] {
        &self.creature_effects
    }
}

/// Re-read effect-list.xml from disk (preferring the installed copy) and swap
/// the table. Called after `.jinx install effect-list.xml`. Returns the spell
/// count for reporting.
pub fn reload() -> usize {
    let parsed = Arc::new(parse_effect_list(&resolve_xml()));
    let count = parsed.spells.len();
    *TABLE.write().unwrap() = Some(parsed);
    count
}

/// Lookup by spell number. Returns an owned copy so callers don't hold the
/// table lock; `SpellInfo` is small and clones cheaply.
pub fn spell(number: u16) -> Option<SpellInfo> {
    tables().spells.get(&number).cloned()
}

/// Default timeout for a creature effect whose `<duration>` is absent or a
/// Ruby formula — the safety net must exist even when the data is sloppy.
const DEFAULT_CREATURE_TIMEOUT_S: u32 = 15;

fn parse_effect_list(xml: &str) -> Tables {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut tables = Tables::default();
    let mut current: Option<SpellInfo> = None;
    // Creature effect being built: (spec, raw start (pattern, severity)).
    let mut current_effect: Option<(String, u32, Vec<(String, u8)>, Vec<String>)> = None;
    // (element, attr "type", attr "severity") whose text we wait for.
    let mut pending: Option<(String, String, u8)> = None;

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
                    // Message-derived creature effect (schema extension;
                    // older clients ignore the unknown tag, Lich untouched).
                    "effect" if attr("availability").as_deref() == Some("creature") => {
                        if let Some(name) = attr("name").filter(|n| !n.is_empty()) {
                            current_effect =
                                Some((name, DEFAULT_CREATURE_TIMEOUT_S, Vec::new(), Vec::new()));
                        }
                    }
                    "cost" | "message" if current.is_some() || current_effect.is_some() => {
                        let severity = attr("severity")
                            .and_then(|s| s.parse::<u8>().ok())
                            .unwrap_or(1)
                            .clamp(1, 3);
                        pending = attr("type").map(|kind| (tag, kind, severity));
                    }
                    "duration" if current_effect.is_some() => {
                        pending = Some((tag, String::new(), 1));
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                let text = || t.unescape().map(|s| s.into_owned()).unwrap_or_default();
                if let (Some(effect), Some((element, kind, severity))) =
                    (&mut current_effect, &pending)
                {
                    match element.as_str() {
                        // Absent/formula durations keep the default net.
                        "duration" => {
                            if let Ok(secs) = text().trim().parse::<u32>() {
                                effect.1 = secs.max(1);
                            }
                        }
                        "message" => match kind.as_str() {
                            "start" => effect.2.push((text(), *severity)),
                            "end" => effect.3.push(text()),
                            _ => {}
                        },
                        _ => {}
                    }
                } else if let (Some(spell), Some((element, kind, _))) = (&mut current, &pending) {
                    let text = text();
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
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"spell" => {
                    if let Some(spell) = current.take() {
                        tables.spells.insert(spell.number, spell);
                    }
                }
                b"effect" => {
                    if let Some((name, timeout_s, starts, ends)) = current_effect.take() {
                        let compile = |src: &str| -> Option<regex::Regex> {
                            regex::Regex::new(src)
                                .map_err(|e| {
                                    tracing::warn!(
                                        "effect-list creature effect '{}': bad regex {:?}: {}",
                                        name,
                                        src,
                                        e
                                    )
                                })
                                .ok()
                        };
                        let spec = CreatureEffectSpec {
                            starts: starts
                                .iter()
                                .filter_map(|(src, sev)| compile(src).map(|re| (re, *sev)))
                                .collect(),
                            ends: ends.iter().filter_map(|src| compile(src)).collect(),
                            name,
                            timeout_s,
                        };
                        if !spec.starts.is_empty() {
                            tables.creature_effects.push(spec);
                        }
                    }
                }
                b"cost" | b"message" | b"duration" => pending = None,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(err) => {
                tracing::warn!("effect-list.xml parse stopped: {}", err);
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    tables
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_parses_the_bundled_database() {
        // The reload test swaps the process-global table to a 1-spell fixture
        // mid-run; serialize on the same lock and force bundled state so this
        // test reads the real database regardless of thread ordering.
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());
        reload();
        std::env::remove_var("VELLUM_FE_DIR");
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
        // Reads the process-global table; same guard as
        // table_parses_the_bundled_database.
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());
        reload();
        std::env::remove_var("VELLUM_FE_DIR");
        // Song of Luck (1006): bard cost formula -> dynamic_cost.
        let song = spell(1006).expect("spell 1006");
        assert!(song.dynamic_cost);
    }

    #[test]
    fn creature_effects_parse_with_severity_duration_and_bad_regex_skip() {
        let tables = parse_effect_list(
            r#"<spells>
                 <spell number="101" name="Spirit Warding I"><cost type="mana">1</cost></spell>
                 <effect availability="creature" name="bleeding">
                   <duration>20</duration>
                   <message type="start" severity="3">gushes blood</message>
                   <message type="start" severity="1">a trickle of blood</message>
                   <message type="start">unranked default</message>
                   <message type="end">the bleeding stops</message>
                 </effect>
                 <effect availability="creature" name="broken">
                   <message type="start">[unclosed(</message>
                 </effect>
                 <effect availability="self-cast" name="not-creature">
                   <message type="start">ignored</message>
                 </effect>
               </spells>"#,
        );
        // Spells co-exist untouched.
        assert_eq!(tables.spells.get(&101).unwrap().mana, Some(1));
        // "broken" (only regex invalid) and "not-creature" both dropped.
        assert_eq!(tables.creature_effects.len(), 1);
        let bleed = &tables.creature_effects[0];
        assert_eq!(bleed.name, "bleeding");
        assert_eq!(bleed.timeout_s, 20);
        assert_eq!(bleed.starts.len(), 3);
        assert_eq!(bleed.starts[0].1, 3);
        assert_eq!(bleed.starts[1].1, 1);
        // No severity attr -> rank 1.
        assert_eq!(bleed.starts[2].1, 1);
        assert_eq!(bleed.ends.len(), 1);
        assert!(bleed.starts[0]
            .0
            .is_match("A troll gushes blood everywhere!"));
    }

    #[test]
    fn creature_effect_duration_defaults_when_absent_or_formula() {
        let tables = parse_effect_list(
            r#"<spells>
                 <effect availability="creature" name="poisoned">
                   <duration>30+LEVEL*2</duration>
                   <message type="start">looks sickly</message>
                 </effect>
               </spells>"#,
        );
        // Formula duration keeps the safety-net default.
        assert_eq!(
            tables.creature_effects[0].timeout_s,
            DEFAULT_CREATURE_TIMEOUT_S
        );
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
