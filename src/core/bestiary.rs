//! The bestiary: creature stat codex bundled with the app.
//!
//! Data lineage (see the design note): the lich-5 creature templates
//! (Ruby hash literals, schema v3) provide the *what* — stats, attacks,
//! defenses, treasure, descriptions; Saga's spawn tables provide the
//! *where* — real room-uid ranges per creature, which resolve to curated
//! map names through [`crate::core::curated_maps`]. An offline maintainer
//! subcommand (`extract-bestiary`) joins the two by creature name and
//! bakes `defaults/bestiary.json`; the runtime only ever loads that file.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A stat that the source knows either exactly or as a range
/// (`ranged: (108..114)` in the templates). Serialized untagged:
/// `194` or `[108, 114]`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StatVal {
    Int(i64),
    Range(i64, i64),
}

impl StatVal {
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        if let Some(i) = v.as_i64() {
            return Some(StatVal::Int(i));
        }
        let arr = v.as_array()?;
        Some(StatVal::Range(arr.first()?.as_i64()?, arr.get(1)?.as_i64()?))
    }

    /// "194" or "108-114" — ebestiary's format_stat.
    pub fn display(&self) -> String {
        match self {
            StatVal::Int(v) => v.to_string(),
            StatVal::Range(lo, hi) => format!("{lo}-{hi}"),
        }
    }

    pub fn min(&self) -> i64 {
        match self {
            StatVal::Int(v) => *v,
            StatVal::Range(lo, _) => *lo,
        }
    }
}

/// One attack line: a named attack with its AS (or CS for warding).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Attack {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub value: Option<StatVal>,
}

/// A special ability with an optional note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Special {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Offense {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub physical: Vec<Attack>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub bolt: Vec<Attack>,
    /// Warding spells; `value` is the CS.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warding: Vec<Attack>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub spells: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub maneuvers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub specials: Vec<Special>,
}

impl Offense {
    pub fn is_empty(&self) -> bool {
        self.physical.is_empty()
            && self.bolt.is_empty()
            && self.warding.is_empty()
            && self.spells.is_empty()
            && self.maneuvers.is_empty()
            && self.specials.is_empty()
    }
}

/// Target defenses. TD keys are the schema's circle abbreviations
/// (`bar`, `cle`, `emp`, `pal`, `ran`, `sor`, `wiz`, `mje`, `mne`,
/// `mjs`, `mns`, `mnm`) — kept as a map so absent circles cost nothing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Defense {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub asg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub melee: Option<StatVal>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ranged: Option<StatVal>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bolt: Option<StatVal>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub udf: Option<StatVal>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty", default)]
    pub td: std::collections::BTreeMap<String, StatVal>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub immunities: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub spells: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub abilities: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub specials: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Treasure {
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub coins: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub gems: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub boxes: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub magic: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub skin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub other: Option<String>,
}

impl Treasure {
    pub fn is_empty(&self) -> bool {
        !self.coins && !self.gems && !self.boxes && !self.magic
            && self.skin.is_none()
            && self.other.is_none()
    }
}

/// Where a creature spawns: resolved curated-map name plus the raw
/// uid ranges from the spawn tables (inclusive).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Spawn {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub map: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub level: Option<i64>,
    pub uids: Vec<(u64, u64)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CreatureEntry {
    pub name: String,
    /// Derived from the last word of `name` when the template leaves it
    /// blank (all of them today); the targets-window join key.
    pub noun: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub level: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub family: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none", default)]
    pub creature_type: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub undead: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub boss: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_hp: Option<i64>,
    #[serde(skip_serializing_if = "Offense::is_empty", default)]
    pub offense: Offense,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub defense: Option<Defense>,
    #[serde(skip_serializing_if = "Treasure::is_empty", default)]
    pub treasure: Treasure,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub alchemy: Vec<String>,
    /// Area names authored in the template (kept even without spawn data).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub areas: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub spawns: Vec<Spawn>,
}

/// The bundled file: a version header plus the entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BestiaryFile {
    pub version: u32,
    pub entries: Vec<CreatureEntry>,
}

pub const FILE_VERSION: u32 = 1;

// ---------------------------------------------------------------------
// Runtime database
// ---------------------------------------------------------------------

/// Loaded once; all queries are index lookups or linear scans over ~600
/// entries (small enough that filters stay simple).
#[derive(Debug, Default)]
pub struct BestiaryDb {
    pub entries: Vec<CreatureEntry>,
    by_name: HashMap<String, usize>,
    by_noun: HashMap<String, Vec<usize>>,
    /// (lo, hi, entry index) — inclusive uid ranges, sorted by lo.
    uid_ranges: Vec<(u64, u64, usize)>,
}

impl BestiaryDb {
    pub fn from_entries(entries: Vec<CreatureEntry>) -> Self {
        let mut db = BestiaryDb {
            entries,
            ..Default::default()
        };
        for (i, e) in db.entries.iter().enumerate() {
            db.by_name.insert(e.name.to_ascii_lowercase(), i);
            db.by_noun
                .entry(e.noun.to_ascii_lowercase())
                .or_default()
                .push(i);
            for spawn in &e.spawns {
                for &(lo, hi) in &spawn.uids {
                    db.uid_ranges.push((lo, hi, i));
                }
            }
        }
        db.uid_ranges.sort_unstable();
        db
    }

    /// Load the bundled default file (defaults/bestiary.json).
    pub fn embedded() -> Result<Self> {
        Self::from_json(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/defaults/bestiary.json"
        )))
    }

    pub fn from_json(text: &str) -> Result<Self> {
        let file: BestiaryFile =
            serde_json::from_str(text).context("parsing bestiary json")?;
        Ok(Self::from_entries(file.entries))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Exact name match (case-insensitive), then unique-noun match, then
    /// unique substring match — the `.bestiary <name>` resolution order.
    pub fn resolve(&self, query: &str) -> Option<&CreatureEntry> {
        let q = query.trim().to_ascii_lowercase();
        if let Some(&i) = self.by_name.get(&q) {
            return Some(&self.entries[i]);
        }
        if let Some(indices) = self.by_noun.get(&q) {
            if indices.len() == 1 {
                return Some(&self.entries[indices[0]]);
            }
        }
        let matches: Vec<&CreatureEntry> = self.search(&q);
        match matches.len() {
            1 => Some(matches[0]),
            _ => None,
        }
    }

    pub fn search(&self, query: &str) -> Vec<&CreatureEntry> {
        let q = query.trim().to_ascii_lowercase();
        let mut out: Vec<&CreatureEntry> = self
            .entries
            .iter()
            .filter(|e| e.name.to_ascii_lowercase().contains(&q))
            .collect();
        out.sort_by_key(|e| (e.level.unwrap_or(i64::MAX), e.name.clone()));
        out
    }

    pub fn by_level(&self, lo: i64, hi: i64) -> Vec<&CreatureEntry> {
        let mut out: Vec<&CreatureEntry> = self
            .entries
            .iter()
            .filter(|e| e.level.is_some_and(|l| l >= lo && l <= hi))
            .collect();
        out.sort_by_key(|e| (e.level.unwrap_or(i64::MAX), e.name.clone()));
        out
    }

    pub fn by_family(&self, family: &str) -> Vec<&CreatureEntry> {
        let q = family.trim().to_ascii_lowercase();
        let mut out: Vec<&CreatureEntry> = self
            .entries
            .iter()
            .filter(|e| {
                e.family
                    .as_deref()
                    .is_some_and(|f| f.to_ascii_lowercase().contains(&q))
            })
            .collect();
        out.sort_by_key(|e| (e.level.unwrap_or(i64::MAX), e.name.clone()));
        out
    }

    /// Matches either authored area names or resolved spawn map names.
    pub fn by_area(&self, area: &str) -> Vec<&CreatureEntry> {
        let q = area.trim().to_ascii_lowercase();
        let mut out: Vec<&CreatureEntry> = self
            .entries
            .iter()
            .filter(|e| {
                e.areas.iter().any(|a| a.to_ascii_lowercase().contains(&q))
                    || e.spawns.iter().any(|s| {
                        s.map
                            .as_deref()
                            .is_some_and(|m| m.to_ascii_lowercase().contains(&q))
                    })
            })
            .collect();
        out.sort_by_key(|e| (e.level.unwrap_or(i64::MAX), e.name.clone()));
        out
    }

    pub fn undead(&self) -> Vec<&CreatureEntry> {
        let mut out: Vec<&CreatureEntry> =
            self.entries.iter().filter(|e| e.undead).collect();
        out.sort_by_key(|e| (e.level.unwrap_or(i64::MAX), e.name.clone()));
        out
    }

    /// Everything whose spawn ranges cover this room uid.
    pub fn here(&self, uid: u64) -> Vec<&CreatureEntry> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for &(lo, hi, i) in &self.uid_ranges {
            if lo > uid {
                break;
            }
            if uid <= hi && seen.insert(i) {
                out.push(&self.entries[i]);
            }
        }
        out.sort_by_key(|e| (e.level.unwrap_or(i64::MAX), e.name.clone()));
        out
    }

    /// Entries by noun — the targets-window join.
    pub fn by_noun(&self, noun: &str) -> Vec<&CreatureEntry> {
        self.by_noun
            .get(&noun.trim().to_ascii_lowercase())
            .map(|v| v.iter().map(|&i| &self.entries[i]).collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------
// Extraction: lich-5 Ruby templates -> entries
// ---------------------------------------------------------------------

/// Convert a schema-v3 creature template (a Ruby hash literal) into JSON.
///
/// The format is a strict subset of Ruby: symbol keys (line-start or
/// inline), `nil`, integer ranges `(108..114)`, double-quoted strings
/// (with `\"` escapes), arrays/hashes. A char-scan splits the text into
/// string and code regions; strings pass through untouched, and inside
/// code regions three rewrites are safe without further guards — quote
/// `identifier:` keys, `nil` -> `null`, `(a..b)` -> `[a, b]` — because
/// code regions contain only structure, numbers, and keywords. Also
/// handled, all seen in the wild: Ruby trailing commas (`},\n]`), inline
/// `# comments` in code regions, parenthesized ints `(529)`, and
/// trailing `=begin ... =end` blocks after the hash (dropped wholesale —
/// they hold raw combat captures, future trigger fodder, not data).
pub fn ruby_template_to_json(text: &str) -> String {
    let key_re = regex::Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*:").expect("static regex");
    let nil_re = regex::Regex::new(r"\bnil\b").expect("static regex");
    let range_re =
        regex::Regex::new(r"\((-?\d+)\s*\.\.\s*(-?\d+)\)").expect("static regex");
    let paren_re = regex::Regex::new(r"\((-?\d+)\)").expect("static regex");
    let comment_re = regex::Regex::new(r"#[^\r\n]*").expect("static regex");
    let trailing_re = regex::Regex::new(r",(\s*[}\]])").expect("static regex");

    // Block comments only ever trail the hash; cut there. `=begin` must
    // start a line in Ruby, and string values never contain a literal
    // newline, so a line-anchored match can't be inside a string.
    let begin_re = regex::Regex::new(r"(?m)^=begin\b").expect("static regex");
    let text = match begin_re.find(text) {
        Some(m) => &text[..m.start()],
        None => text,
    };

    type Rewrites<'a> = (
        &'a regex::Regex,
        &'a regex::Regex,
        &'a regex::Regex,
        &'a regex::Regex,
        &'a regex::Regex,
        &'a regex::Regex,
    );
    fn flush(code: &mut String, out: &mut String, res: Rewrites) {
        let (comment_re, key_re, nil_re, range_re, paren_re, trailing_re) = res;
        let stripped = comment_re.replace_all(code, "");
        let keys = key_re.replace_all(&stripped, "\"$1\":");
        let nils = nil_re.replace_all(&keys, "null");
        let ranges = range_re.replace_all(&nils, "[$1, $2]");
        let parens = paren_re.replace_all(&ranges, "$1");
        out.push_str(&trailing_re.replace_all(&parens, "$1"));
        code.clear();
    }

    let mut out = String::with_capacity(text.len() + 64);
    let mut code = String::new();
    let mut chars = text.chars();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => {
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                }
                '"' => in_string = false,
                _ => {}
            }
        } else if c == '"' {
            flush(&mut code, &mut out, (&comment_re, &key_re, &nil_re, &range_re, &paren_re, &trailing_re));
            out.push(c);
            in_string = true;
        } else {
            code.push(c);
        }
    }
    flush(&mut code, &mut out, (&comment_re, &key_re, &nil_re, &range_re, &paren_re, &trailing_re));
    out
}

fn v_str(v: &serde_json::Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn v_str_list(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    x.as_str()
                        .map(String::from)
                        .or_else(|| x.get("name").and_then(|n| n.as_str()).map(String::from))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn v_attacks(v: &serde_json::Value, value_key: &str) -> Vec<Attack> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let name = x.get("name").and_then(|n| n.as_str())?.to_string();
                    Some(Attack {
                        name,
                        value: x.get(value_key).and_then(StatVal::from_json),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Map one parsed template into a [`CreatureEntry`].
pub fn entry_from_template_json(v: &serde_json::Value) -> Option<CreatureEntry> {
    let name = v_str(v.get("name")?)?;
    let noun = v
        .get("noun")
        .and_then(v_str)
        .unwrap_or_else(|| name.split_whitespace().last().unwrap_or(&name).to_string());

    let atk = v.get("attack_attributes");
    let offense = atk
        .map(|a| Offense {
            physical: a
                .get("physical_attacks")
                .map(|x| v_attacks(x, "as"))
                .unwrap_or_default(),
            bolt: a
                .get("bolt_spells")
                .map(|x| v_attacks(x, "as"))
                .unwrap_or_default(),
            warding: a
                .get("warding_spells")
                .map(|x| v_attacks(x, "cs"))
                .unwrap_or_default(),
            spells: a.get("offensive_spells").map(v_str_list).unwrap_or_default(),
            maneuvers: a.get("maneuvers").map(v_str_list).unwrap_or_default(),
            specials: a
                .get("special_abilities")
                .and_then(|x| x.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| {
                            Some(Special {
                                name: s
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .or_else(|| s.as_str())?
                                    .to_string(),
                                note: s.get("note").and_then(|n| n.as_str()).map(String::from),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .unwrap_or_default();

    let def = v.get("defense_attributes");
    let defense = def.map(|d| {
        let mut td = std::collections::BTreeMap::new();
        for circle in [
            "bar", "cle", "emp", "pal", "ran", "sor", "wiz", "mje", "mne", "mjs", "mns", "mnm",
        ] {
            if let Some(val) = d.get(format!("{circle}_td")).and_then(StatVal::from_json) {
                td.insert(circle.to_string(), val);
            }
        }
        Defense {
            asg: d.get("asg").and_then(v_str),
            melee: d.get("melee").and_then(StatVal::from_json),
            ranged: d.get("ranged").and_then(StatVal::from_json),
            bolt: d.get("bolt").and_then(StatVal::from_json),
            udf: d.get("udf").and_then(StatVal::from_json),
            td,
            immunities: d.get("immunities").map(v_str_list).unwrap_or_default(),
            spells: d.get("defensive_spells").map(v_str_list).unwrap_or_default(),
            abilities: d
                .get("defensive_abilities")
                .map(v_str_list)
                .unwrap_or_default(),
            specials: d.get("special_defenses").map(v_str_list).unwrap_or_default(),
        }
    });
    // A defense block with nothing in it is noise, not data.
    let defense = defense.filter(|d| *d != Defense::default());

    let treas = v.get("treasure");
    let treasure = treas
        .map(|t| Treasure {
            coins: t.get("coins").and_then(|x| x.as_bool()).unwrap_or(false),
            gems: t.get("gems").and_then(|x| x.as_bool()).unwrap_or(false),
            boxes: t.get("boxes").and_then(|x| x.as_bool()).unwrap_or(false),
            magic: t
                .get("magic_items")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            skin: t.get("skin").and_then(v_str),
            other: t.get("other").and_then(v_str),
        })
        .unwrap_or_default();

    let description = v
        .get("messaging")
        .and_then(|m| m.get("description"))
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty());

    let areas = v
        .get("areas")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    Some(CreatureEntry {
        noun,
        level: v.get("level").and_then(|x| x.as_i64()),
        family: v.get("family").and_then(v_str),
        creature_type: v.get("type").and_then(v_str),
        undead: v.get("undead").and_then(|x| x.as_bool()).unwrap_or(false),
        boss: v.get("boss").and_then(|x| x.as_bool()).unwrap_or(false),
        tags: v.get("otherclass").map(v_str_list).unwrap_or_default(),
        max_hp: v.get("max_hp").and_then(|x| x.as_i64()),
        offense,
        defense,
        treasure,
        description,
        notes: v.get("special_other").and_then(v_str),
        alchemy: v.get("alchemy").map(v_str_list).unwrap_or_default(),
        areas,
        url: v.get("url").and_then(v_str),
        spawns: Vec::new(),
        name,
    })
}

// ---------------------------------------------------------------------
// Extraction: Saga spawn tables -> spawn joins
// ---------------------------------------------------------------------

/// Parse Saga's `creatures.json` (`{instance, mongens: {map: [gen,..]}}`)
/// and attach spawns to entries, joining by creature name (leading
/// article stripped). Map names resolve through the curated-maps uid
/// index; unmatched spawn names are returned for the maintainer report.
pub fn join_spawns(
    entries: &mut [CreatureEntry],
    saga_creatures_json: &str,
    curated: &crate::core::curated_maps::CuratedMaps,
) -> Result<Vec<String>> {
    let v: serde_json::Value =
        serde_json::from_str(saga_creatures_json).context("parsing saga creatures.json")?;
    let mongens = v
        .get("mongens")
        .and_then(|m| m.as_object())
        .context("creatures.json: no mongens object")?;

    let uid_index = curated.uid_index();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        by_name.insert(e.name.to_ascii_lowercase(), i);
    }

    let mut unmatched: Vec<String> = Vec::new();
    for gens in mongens.values() {
        let Some(list) = gens.as_array() else { continue };
        for gen in list {
            let Some(raw_name) = gen.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let name = raw_name
                .trim()
                .trim_start_matches("a ")
                .trim_start_matches("an ")
                .trim_start_matches("some ")
                .trim_start_matches("the ")
                .to_ascii_lowercase();
            let uids: Vec<(u64, u64)> = gen
                .get("ranges")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|pair| {
                            let p = pair.as_array()?;
                            Some((p.first()?.as_u64()?, p.get(1)?.as_u64()?))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if uids.is_empty() {
                continue;
            }
            let Some(&idx) = by_name.get(&name) else {
                unmatched.push(name);
                continue;
            };
            let map = uids
                .first()
                .and_then(|&(lo, _)| uid_index.get(&(lo as i64)).map(|s| s.to_string()));
            let level = gen.get("clevel").and_then(|c| c.as_i64());
            let entry = &mut entries[idx];
            // Same creature can have several generators per map; merge
            // ranges into one spawn per resolved map name.
            if let Some(existing) = entry
                .spawns
                .iter_mut()
                .find(|s| s.map == map)
            {
                existing.uids.extend(uids);
            } else {
                entry.spawns.push(Spawn { map, level, uids });
            }
        }
    }
    unmatched.sort();
    unmatched.dedup();
    Ok(unmatched)
}

/// Read every template in a lich-5 creatures directory.
pub fn extract_from_lich(dir: &std::path::Path) -> Result<(Vec<CreatureEntry>, Vec<String>)> {
    let mut entries = Vec::new();
    let mut failed = Vec::new();
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|x| x == "rb")
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('_'))
        })
        .collect();
    paths.sort();
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let json = ruby_template_to_json(&text);
        match serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .as_ref()
            .and_then(entry_from_template_json)
        {
            Some(entry) => entries.push(entry),
            None => failed.push(path.display().to_string()),
        }
    }
    Ok((entries, failed))
}

// ---------------------------------------------------------------------
// Formatting: entries and result tables as styled lines
// ---------------------------------------------------------------------

/// Renders bestiary output as [`crate::data::StyledLine`]s for the
/// `bestiary` stream — the ebestiary presentation (sectioned box,
/// level-separated tables, everything clickable) in native segments.
/// Clickable pieces are `_direct_` links carrying `.bestiary ...`
/// commands, which the frontends route back through dot-command dispatch.
pub mod format {
    use super::{BestiaryDb, CreatureEntry};
    use crate::data::{LinkData, SpanType, StyledLine, TextSegment};

    pub const STREAM: &str = "bestiary";
    const WIDTH: usize = 65;

    fn seg(text: impl Into<String>) -> TextSegment {
        TextSegment {
            text: text.into(),
            fg: None,
            bg: None,
            bold: false,
            mono: true,
            span_type: SpanType::System,
            link_data: None,
            custom_emoji: None,
            inline_image: None,
        }
    }

    fn bold_seg(text: impl Into<String>) -> TextSegment {
        TextSegment {
            bold: true,
            span_type: SpanType::Monsterbold,
            ..seg(text)
        }
    }

    /// Box chrome: borders, rules, padding — recedes behind the content.
    fn dim(text: impl Into<String>) -> TextSegment {
        TextSegment {
            fg: Some("#6b6250".to_string()),
            ..seg(text)
        }
    }

    /// Section headers — sage, matching the design note's palette.
    fn sage(text: impl Into<String>) -> TextSegment {
        TextSegment {
            fg: Some("#8fa872".to_string()),
            ..seg(text)
        }
    }

    /// Field labels ("Family:", "ASG:") — parchment-dim.
    fn label(text: impl Into<String>) -> TextSegment {
        TextSegment {
            fg: Some("#a79878".to_string()),
            ..seg(text)
        }
    }

    fn link(text: impl Into<String>, cmd: impl Into<String>) -> TextSegment {
        let text = text.into();
        TextSegment {
            span_type: SpanType::Link,
            link_data: Some(LinkData {
                exist_id: crate::data::DIRECT_LINK_SENTINEL.to_string(),
                noun: cmd.into(),
                text: text.clone(),
                coord: None,
            }),
            ..seg(text)
        }
    }

    fn line(segments: Vec<TextSegment>) -> StyledLine {
        StyledLine {
            segments,
            stream: STREAM.to_string(),
            timestamp: None,
        }
    }

    fn visual_len(segments: &[TextSegment]) -> usize {
        segments.iter().map(|s| s.text.chars().count()).sum()
    }

    /// `| <content><pad> |`
    fn boxed(mut segments: Vec<TextSegment>) -> StyledLine {
        let used = visual_len(&segments);
        let pad = WIDTH.saturating_sub(used + 2);
        let mut all = vec![dim("| ")];
        all.append(&mut segments);
        all.push(dim(format!("{} |", " ".repeat(pad))));
        line(all)
    }

    fn rule(ch: char) -> StyledLine {
        line(vec![dim(format!("+{}+", ch.to_string().repeat(WIDTH)))])
    }

    fn section(title: &str) -> StyledLine {
        let mut segs = vec![sage(format!("--- {title} "))];
        let used = 4 + title.chars().count() + 1;
        segs.push(dim("-".repeat(WIDTH.saturating_sub(used + 2))));
        boxed(segs)
    }

    fn wrap(text: &str, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        let mut cur = String::new();
        for word in text.split_whitespace() {
            if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        lines
    }

    /// Room-id array for one spawn table, appended below an entry card
    /// (the caller deliberately does NOT clear the stream first). One
    /// wrapped `[id, id, …]` array meant for copy/paste into scripts.
    /// `resolve` maps a game uid to a Lich room id — the mapdb lookup
    /// stays with the caller so this module keeps zero map dependencies;
    /// unresolved uids fall back to `u<uid>`.
    pub fn rooms_lines(
        map_name: &str,
        ranges: &[(u64, u64)],
        resolve: impl Fn(u64) -> Option<u32>,
    ) -> Vec<StyledLine> {
        let ids: Vec<String> = ranges
            .iter()
            .flat_map(|&(lo, hi)| lo..=hi)
            .map(|uid| {
                resolve(uid)
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| format!("u{uid}"))
            })
            .collect();
        let array = format!("[{}]", ids.join(", "));
        let mut out = vec![section(&format!("Rooms — {map_name}"))];
        for l in wrap(&array, WIDTH - 6) {
            out.push(boxed(vec![seg(format!("  {l}"))]));
        }
        out.push(rule('='));
        out
    }

    /// The full entry box.
    pub fn entry_lines(e: &CreatureEntry) -> Vec<StyledLine> {
        let mut out = vec![rule('=')];
        let mut header = vec![bold_seg(e.name.to_uppercase())];
        if let Some(level) = e.level {
            header.push(seg(" ("));
            header.push(link(format!("Level {level}"), format!(".bestiary level {level}")));
            header.push(seg(")"));
        }
        out.push(boxed(header));
        out.push(rule('-'));

        if let Some(desc) = &e.description {
            for l in wrap(desc, WIDTH - 6) {
                out.push(boxed(vec![label(format!("  {l}"))]));
            }
            out.push(boxed(vec![]));
        }

        let mut info: Vec<TextSegment> = Vec::new();
        if let Some(f) = &e.family {
            info.push(label("  Family: "));
            info.push(link(f.clone(), format!(".bestiary family {f}")));
        }
        if let Some(t) = &e.creature_type {
            info.push(label("  Type: "));
            info.push(seg(t.clone()));
        }
        if e.undead {
            info.push(bold_seg("  Undead"));
        }
        if let Some(hp) = e.max_hp {
            info.push(label("  HP: "));
            info.push(seg(hp.to_string()));
        }
        if !info.is_empty() {
            out.push(boxed(info));
        }

        let has_location = !e.spawns.is_empty() || !e.areas.is_empty();
        if has_location {
            out.push(section("Locations"));
            for spawn in &e.spawns {
                if let Some(map) = &spawn.map {
                    let rooms: u64 = spawn.uids.iter().map(|(lo, hi)| hi - lo + 1).sum();
                    // Ranges ride in the command so the room list needs no
                    // creature lookup; it appends below the card (no clear).
                    let spec: String = spawn
                        .uids
                        .iter()
                        .map(|(lo, hi)| format!("{lo}-{hi}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    let mut segs = vec![
                        seg("  "),
                        link(map.clone(), format!(".bestiary area {map}")),
                        seg(" ("),
                        link(
                            format!("{rooms} rooms"),
                            format!(".bestiary rooms {map}|{spec}"),
                        ),
                        seg(")  "),
                    ];
                    if let Some((lo, _)) = spawn.uids.first() {
                        // Native pathing straight to the hunting ground.
                        segs.push(link("[go2]", format!(".go2 u{lo}")));
                    }
                    out.push(boxed(segs));
                }
            }
            for area in &e.areas {
                // Authored names not already covered by a resolved spawn map.
                if !e
                    .spawns
                    .iter()
                    .any(|s| s.map.as_deref() == Some(area.as_str()))
                {
                    out.push(boxed(vec![
                        seg("  "),
                        link(area.clone(), format!(".bestiary area {area}")),
                    ]));
                }
            }
        }

        if !e.offense.is_empty() {
            out.push(section("Offense"));
            for a in &e.offense.physical {
                let val = a.value.map(|v| format!(" (AS: {})", v.display())).unwrap_or_default();
                out.push(boxed(vec![label("  Physical: "), seg(format!("{}{val}", a.name))]));
            }
            for a in &e.offense.bolt {
                let val = a.value.map(|v| format!(" (AS: {})", v.display())).unwrap_or_default();
                out.push(boxed(vec![label("  Bolt: "), seg(format!("{}{val}", a.name))]));
            }
            for a in &e.offense.warding {
                let val = a.value.map(|v| format!(" (CS: {})", v.display())).unwrap_or_default();
                out.push(boxed(vec![label("  Warding: "), seg(format!("{}{val}", a.name))]));
            }
            for s in &e.offense.spells {
                out.push(boxed(vec![label("  Spell: "), seg(s.clone())]));
            }
            for m in &e.offense.maneuvers {
                out.push(boxed(vec![label("  Maneuver: "), seg(m.clone())]));
            }
            for s in &e.offense.specials {
                let note = s.note.as_deref().map(|n| format!(" - {n}")).unwrap_or_default();
                out.push(boxed(vec![bold_seg("  Special"), seg(format!(": {}{note}", s.name))]));
            }
        }

        if let Some(d) = &e.defense {
            out.push(section("Defense"));
            let mut ds: Vec<String> = Vec::new();
            if let Some(asg) = &d.asg {
                ds.push(format!("ASG: {asg}"));
            }
            if let Some(v) = d.melee {
                ds.push(format!("Melee DS: {}", v.display()));
            }
            if let Some(v) = d.ranged {
                ds.push(format!("Ranged DS: {}", v.display()));
            }
            if let Some(v) = d.bolt {
                ds.push(format!("Bolt DS: {}", v.display()));
            }
            if let Some(v) = d.udf {
                ds.push(format!("UDF: {}", v.display()));
            }
            for chunk in ds.chunks(3) {
                out.push(boxed(vec![seg(format!("  {}", chunk.join("    ")))]));
            }
            if !d.td.is_empty() {
                let tds: Vec<String> = d
                    .td
                    .iter()
                    .map(|(k, v)| format!("{}: {}", capitalize(k), v.display()))
                    .collect();
                for l in wrap(&format!("TD: {}", tds.join(", ")), WIDTH - 6) {
                    out.push(boxed(vec![seg(format!("  {l}"))]));
                }
            }
            if !d.immunities.is_empty() {
                out.push(boxed(vec![seg(format!("  Immunities: {}", d.immunities.join(", ")))]));
            }
            if !d.spells.is_empty() {
                for l in wrap(&format!("Spells: {}", d.spells.join(", ")), WIDTH - 6) {
                    out.push(boxed(vec![seg(format!("  {l}"))]));
                }
            }
            for a in &d.abilities {
                out.push(boxed(vec![seg(format!("  Ability: {a}"))]));
            }
            for s in &d.specials {
                out.push(boxed(vec![bold_seg("  Special"), seg(format!(": {s}"))]));
            }
        }

        if !e.treasure.is_empty() {
            out.push(section("Treasure"));
            let mut items: Vec<String> = Vec::new();
            if e.treasure.coins {
                items.push("coins".into());
            }
            if e.treasure.gems {
                items.push("gems".into());
            }
            if e.treasure.boxes {
                items.push("boxes".into());
            }
            if e.treasure.magic {
                items.push("magic items".into());
            }
            if let Some(skin) = &e.treasure.skin {
                items.push(skin.clone());
            }
            for l in wrap(&items.join(", "), WIDTH - 6) {
                out.push(boxed(vec![seg(format!("  {l}"))]));
            }
            if let Some(other) = &e.treasure.other {
                for l in wrap(other, WIDTH - 6) {
                    out.push(boxed(vec![seg(format!("  {l}"))]));
                }
            }
        }

        if let Some(notes) = &e.notes {
            out.push(section("Notes"));
            for l in wrap(notes, WIDTH - 6) {
                out.push(boxed(vec![seg(format!("  {l}"))]));
            }
        }

        if let Some(url) = &e.url {
            // Web link: opens in the browser via the URL sentinel.
            out.push(boxed(vec![
                label("Wiki: "),
                TextSegment {
                    span_type: SpanType::Link,
                    link_data: Some(LinkData {
                        exist_id: crate::data::URL_LINK_SENTINEL.to_string(),
                        noun: url.clone(),
                        text: url.clone(),
                        coord: None,
                    }),
                    ..seg(url.clone())
                },
            ]));
        }
        out.push(rule('='));
        out
    }

    fn capitalize(s: &str) -> String {
        let mut c = s.chars();
        match c.next() {
            Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    }

    /// Level-separated result table, every cell linked.
    pub fn table_lines(rows: &[&CreatureEntry], title: &str) -> Vec<StyledLine> {
        if rows.is_empty() {
            return vec![line(vec![seg("No matching creatures.")])];
        }
        let mut out = vec![line(vec![seg(format!("{title} ({} found):", rows.len()))])];
        let name_w = rows.iter().map(|e| e.name.chars().count()).max().unwrap_or(4).max(4);
        let fam_w = rows
            .iter()
            .map(|e| e.family.as_deref().unwrap_or("").chars().count())
            .max()
            .unwrap_or(6)
            .max(6);
        let sep = format!("+-----+-{}-+-{}-+", "-".repeat(name_w), "-".repeat(fam_w));
        out.push(line(vec![seg(sep.clone())]));
        let mut last_level: Option<i64> = None;
        for e in rows {
            if last_level.is_some() && last_level != e.level {
                out.push(line(vec![seg(sep.clone())]));
            }
            last_level = e.level;
            let lvl = e.level.map(|l| l.to_string()).unwrap_or_else(|| "?".into());
            let fam = e.family.clone().unwrap_or_default();
            let mut segs = vec![seg("| ")];
            segs.push(link(format!("{lvl:>3}"), format!(".bestiary level {lvl}")));
            segs.push(seg(" | "));
            segs.push(link(format!("{:<name_w$}", e.name), format!(".bestiary {}", e.name)));
            segs.push(seg(" | "));
            if fam.is_empty() {
                segs.push(seg(format!("{fam:<fam_w$}")));
            } else {
                segs.push(link(format!("{fam:<fam_w$}"), format!(".bestiary family {fam}")));
            }
            segs.push(seg(" |"));
            out.push(line(segs));
        }
        out.push(line(vec![seg(sep)]));
        out
    }

    pub fn help_lines() -> Vec<StyledLine> {
        [
            "Bestiary - creature lookup (bundled codex)",
            "  .bestiary <name>        look up one creature",
            "  .bestiary here          creatures spawning around this room",
            "  .bestiary level <n|a-b> list by level or range",
            "  .bestiary area <name>   list by area/map",
            "  .bestiary family <name> list by family",
            "  .bestiary undead        list undead",
            "  .bestiary search <text> search names",
        ]
        .iter()
        .map(|s| line(vec![seg(*s)]))
        .collect()
    }

    /// Shared database instance, loaded from the bundled file on first use.
    pub fn shared() -> &'static BestiaryDb {
        static DB: std::sync::OnceLock<BestiaryDb> = std::sync::OnceLock::new();
        DB.get_or_init(|| {
            BestiaryDb::embedded().unwrap_or_else(|e| {
                tracing::error!("bundled bestiary failed to load: {e}");
                BestiaryDb::default()
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANTICORE: &str = r#"{
  schema_version: 3,
  name: "arctic manticore",
  noun: "",
  url: "https://gswiki.play.net/arctic_manticore",
  level: 29,
  family: "Chimeric",
  type: "Quadruped",
  undead: false,
  has_blood: nil,
  boss: false,
  otherclass: [
    "Living"
  ],
  max_hp: 340,
  areas: [
    {
      name: "Frozen Battlefield",
      rooms: []
    }
  ],
  attack_attributes: {
    physical_attacks: [
      {
        name: "Bite",
        as: 194
      },
      {
        name: "Claw",
        as: 194
      }
    ],
    bolt_spells: [],
    warding_spells: [],
    offensive_spells: [],
    maneuvers: [],
    special_abilities: [],
    special_notes: []
  },
  defense_attributes: {
    asg: "7N",
    immunities: [],
    melee: 150,
    ranged: (108..114),
    bolt: 141,
    udf: 179,
    sor_td: 101,
    mne_td: 105,
    mns_td: 97,
    defensive_spells: [],
    defensive_abilities: [],
    special_defenses: []
  },
  special_other: nil,
  treasure: {
    coins: true,
    magic_items: true,
    gems: true,
    boxes: true,
    skin: "an arctic manticore mane",
    other: "Glimmering blue essence dust"
  },
  messaging: {
    description: [
      "The first thing that will strike you: nil desperandum, a scorpion tail."
    ]
  }
}"#;

    fn manticore_entry() -> CreatureEntry {
        let json = ruby_template_to_json(MANTICORE);
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        entry_from_template_json(&v).expect("entry")
    }

    #[test]
    fn ruby_conversion_produces_valid_json() {
        let json = ruby_template_to_json(MANTICORE);
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(v["name"], "arctic manticore");
        assert_eq!(v["has_blood"], serde_json::Value::Null);
    }

    #[test]
    fn nil_inside_strings_survives() {
        let e = manticore_entry();
        // "nil desperandum" sits inside a quoted description; the nil->null
        // rewrite must not touch it.
        assert!(e.description.unwrap().contains("nil desperandum"));
    }

    #[test]
    fn entry_fields_map_through() {
        let e = manticore_entry();
        assert_eq!(e.noun, "manticore", "noun derived from last word of name");
        assert_eq!(e.level, Some(29));
        assert_eq!(e.max_hp, Some(340));
        assert_eq!(e.offense.physical.len(), 2);
        assert_eq!(e.offense.physical[0].value, Some(StatVal::Int(194)));
        let d = e.defense.expect("defense present");
        assert_eq!(d.melee, Some(StatVal::Int(150)));
        assert_eq!(
            d.ranged,
            Some(StatVal::Range(108, 114)),
            "ruby range values survive as ranges"
        );
        assert_eq!(d.ranged.unwrap().display(), "108-114");
        assert_eq!(d.td.get("sor"), Some(&StatVal::Int(101)));
        assert_eq!(d.td.len(), 3, "only present TDs are kept");
        assert!(e.treasure.coins && e.treasure.magic);
        assert_eq!(e.areas, vec!["Frozen Battlefield"]);
        assert!(e.notes.is_none(), "nil special_other drops out");
    }

    #[test]
    fn empty_sections_are_omitted_from_json() {
        let mut e = manticore_entry();
        e.offense = Offense::default();
        e.defense = None;
        e.treasure = Treasure::default();
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("offense"));
        assert!(!json.contains("defense"));
        assert!(!json.contains("treasure"));
    }

    #[test]
    fn db_queries_work() {
        let mut e = manticore_entry();
        e.spawns.push(Spawn {
            map: Some("Frozen Battlefield".into()),
            level: Some(29),
            uids: vec![(14012001, 14012088)],
        });
        let db = BestiaryDb::from_entries(vec![e]);
        assert!(db.resolve("arctic manticore").is_some());
        assert!(db.resolve("manticore").is_some(), "unique noun resolves");
        assert_eq!(db.by_level(25, 33).len(), 1);
        assert_eq!(db.by_level(30, 40).len(), 0);
        assert_eq!(db.by_family("chimeric").len(), 1);
        assert_eq!(db.by_area("frozen").len(), 1);
        assert!(db.undead().is_empty());
        assert_eq!(db.here(14012050).len(), 1, "uid inside spawn range");
        assert!(db.here(14012089).is_empty(), "uid outside spawn range");
        assert_eq!(db.by_noun("manticore").len(), 1);
    }

    #[test]
    fn spawn_join_attaches_by_name_and_reports_unmatched() {
        let mut entries = vec![manticore_entry()];
        let saga = r#"{"instance":"prime","mongens":{
            "8":[{"profile":1,"clevel":29,"name":"an arctic manticore","ranges":[[100,110]]},
                 {"profile":2,"clevel":3,"name":"a hobgoblin","ranges":[[200,210]]}]}}"#;
        let curated = crate::core::curated_maps::CuratedMaps::default();
        let unmatched = join_spawns(&mut entries, saga, &curated).expect("join");
        assert_eq!(entries[0].spawns.len(), 1);
        assert_eq!(entries[0].spawns[0].uids, vec![(100, 110)]);
        assert_eq!(entries[0].spawns[0].level, Some(29));
        assert_eq!(unmatched, vec!["hobgoblin"]);
    }

    #[test]
    fn embedded_bestiary_loads_and_is_populated() {
        let db = BestiaryDb::embedded().expect("bundled bestiary parses");
        assert!(
            db.len() >= 600,
            "shipped file should carry the full template set, got {}",
            db.len()
        );
        // The design-doc exemplar, end to end through the bake.
        let m = db.resolve("arctic manticore").expect("manticore present");
        assert_eq!(m.level, Some(29));
        assert_eq!(m.max_hp, Some(340));
        assert!(m.description.is_some(), "descriptions ship");
        assert!(m.url.is_some(), "wiki urls ship");
        // Spawn join coverage: most entries locate somewhere.
        let located = db.entries.iter().filter(|e| !e.spawns.is_empty()).count();
        assert!(
            located >= 500,
            "spawn join should locate most creatures, got {located}"
        );
    }

    #[test]
    fn entry_lines_box_up_consistently() {
        let mut e = manticore_entry();
        e.spawns.push(Spawn {
            map: Some("Frozen Battlefield".into()),
            level: Some(29),
            uids: vec![(100, 187)],
        });
        let lines = format::entry_lines(&e);
        assert!(lines.len() > 8);
        // Every boxed line renders at the same width.
        let widths: std::collections::HashSet<usize> = lines
            .iter()
            .map(|l| l.segments.iter().map(|s| s.text.chars().count()).sum())
            .collect();
        assert_eq!(widths.len(), 1, "ragged box: {widths:?}");
        // Links are direct dot-commands.
        let links: Vec<String> = lines
            .iter()
            .flat_map(|l| &l.segments)
            .filter_map(|s| s.link_data.as_ref())
            .map(|l| l.noun.clone())
            .collect();
        assert!(links.iter().any(|c| c == ".bestiary level 29"));
        assert!(links.iter().any(|c| c == ".bestiary family Chimeric"));
        assert!(links.iter().any(|c| c == ".bestiary area Frozen Battlefield"));
        assert!(
            links.iter().any(|c| c == ".go2 u100"),
            "spawn location carries a go2 link to its first uid"
        );
        for link in &links {
            assert!(
                link.starts_with('.') || link.starts_with("https://"),
                "unexpected link target {link}"
            );
        }
        // The spawn line shows the room count from the uid range.
        let all_text: String = lines
            .iter()
            .flat_map(|l| &l.segments)
            .map(|s| s.text.as_str())
            .collect();
        assert!(all_text.contains("(88 rooms)"));
    }

    #[test]
    fn table_lines_separate_levels_and_link_names() {
        let mut a = manticore_entry();
        a.level = Some(28);
        a.name = "arctic puma".into();
        let b = manticore_entry();
        let rows_owned = [a, b];
        let rows: Vec<&CreatureEntry> = rows_owned.iter().collect();
        let lines = format::table_lines(&rows, "Test");
        let seps = lines
            .iter()
            .filter(|l| l.segments.len() == 1 && l.segments[0].text.starts_with("+--"))
            .count();
        assert_eq!(seps, 3, "top, level-change, bottom separators");
        let links: Vec<String> = lines
            .iter()
            .flat_map(|l| &l.segments)
            .filter_map(|s| s.link_data.as_ref())
            .map(|l| l.noun.clone())
            .collect();
        assert!(links.iter().any(|c| c == ".bestiary arctic puma"));
    }

    #[test]
    fn file_roundtrip() {
        let file = BestiaryFile {
            version: FILE_VERSION,
            entries: vec![manticore_entry()],
        };
        let json = serde_json::to_string(&file).unwrap();
        let db = BestiaryDb::from_json(&json).unwrap();
        assert_eq!(db.len(), 1);
    }
}
