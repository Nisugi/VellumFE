//! Chronomage day-pass expiry cache — the live-feed half of Lich's day-pass
//! travel (`stringprocs/timeto/room-*.rb`, the `mapdb_day_pass_monitor`
//! DownstreamHook + `$mapdb_day_passes`).
//!
//! A Chronomage day pass grants one day of unlimited travel between a specific
//! pair of towns. go2 routes through the day-pass edges only when a valid pass
//! for that town pair is held (or is buyable). Lich learns each pass's town
//! pair and expiry by `look`ing at it and parsing the description; we do the
//! same, feeding the pass-description / EXPIRED / expiry lines through here into
//! a per-pass cache keyed by the pass's game object id.
//!
//! Wire format (verbatim from the proc's hook_proc):
//! - `Bold calligraphy states simply, "This <a exist="ID" noun="pass">pass</a>
//!   entitles the original purchaser to one (1) day of unlimited travel between
//!   the towns of TOWN_A and TOWN_B, commencing…` → record `{towns:[A,B]}` for
//!   the pass id (expiry filled by the following line).
//! - `Bold red block letters spelling out "EXPIRED" appear … the <a exist="ID"
//!   noun="pass">pass…` → the pass is expired.
//! - `[Your pass will expire on Wkday Mon DD HH:MM:SS ET YYYY.` → the expiry,
//!   parsed as Eastern (`-05:00`, matching Lich's `Time.new(…, "-05:00")`).

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

macro_rules! re {
    ($name:ident, $pattern:literal) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($pattern).expect("valid pattern"));
    };
}

// The description in RAW (markup) form — the pass id is in the `<a>` tag.
re!(
    PASS_DESC,
    r#"This <a exist="(?P<id>-?\d+)" noun="pass">pass</a> entitles the original purchaser to one \(1\) day of unlimited travel between the towns of (?P<a>.+?) and (?P<b>.+?), commencing"#
);
// The description in STRIPPED (plain-text) form — the id comes from the line's
// pass link, supplied by the caller (the feed strips `<a>` markup to text).
// NOT anchored: the stripped line keeps the lead-in (`Bold calligraphy states
// simply, "This pass entitles…`), so the phrase sits mid-line.
re!(
    PASS_DESC_TEXT,
    r#"This pass entitles the original purchaser to one \(1\) day of unlimited travel between the towns of (?P<a>.+?) and (?P<b>.+?), commencing"#
);
// "…the <a exist="12345" noun="pass">pass…" preceded by the EXPIRED stamp.
re!(
    PASS_EXPIRED,
    r#"EXPIRED" appear.*<a exist="(?P<id>-?\d+)" noun="pass">pass"#
);
// "[Your pass will expire on Fri Aug 22 14:30:00 ET 2026."
re!(
    PASS_EXPIRES,
    r"^\[Your pass will expire on \w+ (?P<mon>\w+) *(?P<day>\d+) (?P<h>\d+):(?P<min>\d+):(?P<s>\d+) ET (?P<year>\d+)"
);

fn month_num(mon: &str) -> Option<u32> {
    Some(match mon {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    })
}

/// The `buy_day_pass` short code for a full town name (`Solhaven` → `sol`).
fn town_code(town: &str) -> Option<&'static str> {
    Some(match town {
        "Solhaven" => "sol",
        "Wehnimer's Landing" => "wl",
        "Icemule Trace" => "imt",
        // The Elven Nations pairs go2's config validation accepts
        // (go2.lic:440). The live mapdb ships NO eastern day-pass procs
        // (verified 2026-08-12: the only buy_day_pass edges are the six
        // western ones), so DEPARTURES stays western until the data exists —
        // but held eastern passes and config pairs must still resolve.
        "Ta'Illistim" => "ill",
        "Ta'Vaalor" => "val",
        "Cysaegir" => "cys",
        _ => return None,
    })
}

/// A Chronomage departure room (Solhaven / Wehnimer's / Icemule) and its baked
/// buy-trip geography — the literal command lists from each edge's wayto proc.
/// The USE path is identical everywhere (open sack → raise pass → put back); the
/// BUY path differs only in the NPC noun, the enter/exit move, and this town's
/// own Chronomage→bank→back walk.
pub struct DayPassDeparture {
    /// The mapdb room id of the Chronomage departure room.
    pub room: u32,
    /// The NPC you ask ("agent" / "clerk" / "halfling").
    pub npc: &'static str,
    /// The move that steps from the departure room out to the NPC ("out" /
    /// "south" / "go corridor").
    pub enter_move: &'static str,
    /// The move that departs after buying/holding a pass ("go arch" / "north" /
    /// "go corridor").
    pub exit_move: &'static str,
    /// Directions from the NPC room to this town's bank teller (the last entry
    /// lands at the teller; then `withdraw 5000`).
    pub to_bank: &'static [&'static str],
    /// Directions back from the bank to the NPC room.
    pub from_bank: &'static [&'static str],
    /// The destinations reachable from this departure room.
    pub destinations: &'static [DayPassDestination],
}

/// A destination from a departure room: the dest mapdb room, the town short
/// word you `ask <npc> for <word>`, the pair (for the cache/gate), and the buy
/// cost (the proc's timeto weight for buying this route).
pub struct DayPassDestination {
    pub dest_room: u32,
    /// The town short word in `ask <npc> for <word>` ("wehnimer"/"icemule"/
    /// "solhaven").
    pub ask_word: &'static str,
    /// Full town names of the pair (for cache/gate lookup + `raise` matching).
    pub pair: (&'static str, &'static str),
    /// Estimated buy cost (the proc's timeto weight; display + routing).
    pub buy_cost: f64,
}

/// The three Chronomage departure rooms and their edges (verbatim from the six
/// day-pass procs). USE cost is always 0.8; BUY cost is per-source.
pub const DEPARTURES: &[DayPassDeparture] = &[
    DayPassDeparture {
        room: 14359, // Solhaven, Departures
        npc: "agent",
        enter_move: "out",
        exit_move: "go arch",
        to_bank: &[
            "out",
            "down",
            "down",
            "down",
            "down",
            "go ramp",
            "east",
            "northwest",
            "north",
            "north",
            "northwest",
            "north",
            "north",
            "north",
            "north",
            "north",
            "north",
            "northwest",
            "east",
            "go doors",
        ],
        from_bank: &[
            "out",
            "west",
            "southeast",
            "south",
            "south",
            "south",
            "south",
            "south",
            "south",
            "southeast",
            "south",
            "south",
            "southeast",
            "west",
            "climb ramp",
            "climb stairway",
            "up",
            "up",
            "up",
            "go building",
        ],
        destinations: &[
            DayPassDestination {
                dest_room: 8634,
                ask_word: "wehnimer",
                pair: ("Solhaven", "Wehnimer's Landing"),
                buy_cost: 7.4,
            },
            DayPassDestination {
                dest_room: 8916,
                ask_word: "icemule",
                pair: ("Solhaven", "Icemule Trace"),
                buy_cost: 7.4,
            },
        ],
    },
    DayPassDeparture {
        room: 8635, // Wehnimer's Shop, Waiting Area
        npc: "clerk",
        enter_move: "south",
        exit_move: "north",
        to_bank: &["up", "north", "out", "north", "go bank", "go arch"],
        from_bank: &[
            "go arch",
            "out",
            "south",
            "go wood-sided shop",
            "south",
            "down",
        ],
        destinations: &[
            DayPassDestination {
                dest_room: 14360,
                ask_word: "solhaven",
                pair: ("Solhaven", "Wehnimer's Landing"),
                buy_cost: 4.4,
            },
            DayPassDestination {
                dest_room: 8916,
                ask_word: "icemule",
                pair: ("Icemule Trace", "Wehnimer's Landing"),
                buy_cost: 4.4,
            },
        ],
    },
    DayPassDeparture {
        room: 15619, // Icemule, Loci Workshop (Confluence)
        npc: "halfling",
        enter_move: "go corridor",
        exit_move: "go corridor",
        to_bank: &[
            "out",
            "go gate",
            "southeast",
            "southeast",
            "northeast",
            "southeast",
            "south",
            "west",
            "south",
            "east",
            "east",
            "south",
            "south",
            "east",
            "north",
            "north",
            "go archway",
        ],
        from_bank: &[
            "out",
            "south",
            "south",
            "west",
            "north",
            "north",
            "west",
            "west",
            "north",
            "east",
            "north",
            "northwest",
            "southwest",
            "northwest",
            "northwest",
            "go gate",
            "go door",
        ],
        destinations: &[
            DayPassDestination {
                dest_room: 14360,
                ask_word: "solhaven",
                pair: ("Solhaven", "Icemule Trace"),
                buy_cost: 8.8,
            },
            DayPassDestination {
                dest_room: 8634,
                ask_word: "wehnimer",
                pair: ("Icemule Trace", "Wehnimer's Landing"),
                buy_cost: 8.8,
            },
        ],
    },
];

/// The departure + destination for a given `(source_room, dest_room)` edge, if
/// it's a day-pass edge.
pub fn edge(
    source: u32,
    dest: u32,
) -> Option<(&'static DayPassDeparture, &'static DayPassDestination)> {
    let dep = DEPARTURES.iter().find(|d| d.room == source)?;
    let d = dep.destinations.iter().find(|d| d.dest_room == dest)?;
    Some((dep, d))
}

/// Is `room` a Chronomage day-pass departure room?
pub fn is_departure(room: u32) -> bool {
    DEPARTURES.iter().any(|d| d.room == room)
}

/// Does the `buy_day_pass` config value permit buying for the town pair
/// `(a, b)`? Mirrors the proc's `buy_day_pass.to_s =~ /^(yes|true)$|\bPAIR\b/i`:
/// a bare on/yes/true permits any pair; otherwise the value must contain the
/// pair's short code (`sol,wl`) in either orientation.
pub fn buy_permits(buy_day_pass: &str, a: &str, b: &str) -> bool {
    let v = buy_day_pass.trim().to_ascii_lowercase();
    if v == "yes" || v == "true" || v == "on" {
        return true;
    }
    let (Some(ca), Some(cb)) = (town_code(a), town_code(b)) else {
        return false;
    };
    let codes = [format!("{ca},{cb}"), format!("{cb},{ca}")];
    v.split([' ', ';', '|'])
        .any(|tok| codes.iter().any(|c| tok.trim() == c))
}

/// One learned pass: the town pair it covers and when it expires (Unix epoch;
/// `0` = expired/unknown). Mirrors `$mapdb_day_passes[id] = {towns, expires}`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DayPass {
    pub towns: Vec<String>,
    /// Expiry as a Unix timestamp; `0` if expired or not yet parsed.
    pub expires: i64,
}

/// The live day-pass cache, keyed by pass game-object id. Populated by
/// `look`ing at passes; the travel gate reads it to decide routability.
#[derive(Debug, Clone, Default)]
pub struct DayPassCache {
    passes: HashMap<String, DayPass>,
    /// The town pair captured from the most recent description line, awaiting
    /// its expiry line (Lich's `last_id`).
    pending_id: Option<String>,
    /// A buy attempt failed for lack of silver this session — Lich turns the
    /// buy_day_pass setting off for the run (map_strategies.rb:740) so the
    /// router stops planning through buy edges it can't pay for.
    buy_disabled: bool,
}

impl DayPassCache {
    /// Session suppression of day-pass BUYING after a too-poor failure
    /// (Lich's "Turning off buy_day_pass setting."). Held passes still route.
    pub fn buy_disabled(&self) -> bool {
        self.buy_disabled
    }

    pub fn disable_buy(&mut self) {
        self.buy_disabled = true;
    }

    /// Feed one game line as the client sees it after `<a>` markup is stripped
    /// to plain text. `pass_id` is the exist-id of the line's `noun="pass"`
    /// link, when present (the description/EXPIRED lines carry one). The expiry
    /// line has no link and needs no id. Returns true if it touched the cache.
    pub fn observe(&mut self, line: &str, pass_id: Option<&str>) -> bool {
        if let Some(c) = PASS_DESC_TEXT.captures(line) {
            let Some(id) = pass_id else { return false };
            let towns = vec![c["a"].to_string(), c["b"].to_string()];
            self.passes
                .insert(id.to_string(), DayPass { towns, expires: 0 });
            self.pending_id = Some(id.to_string());
            return true;
        }
        if line.contains("\"EXPIRED\" appear") {
            if let Some(id) = pass_id {
                self.passes.insert(
                    id.to_string(),
                    DayPass {
                        towns: Vec::new(),
                        expires: 0,
                    },
                );
                self.pending_id = None;
                return true;
            }
        }
        if let Some(c) = PASS_EXPIRES.captures(line) {
            if let Some(id) = self.pending_id.take() {
                if let Some(epoch) = expiry_epoch(&c) {
                    if let Some(pass) = self.passes.get_mut(&id) {
                        pass.expires = epoch;
                    }
                }
            }
            return true;
        }
        false
    }

    /// Feed one RAW game line (markup intact) — the id is read from the `<a>`
    /// tag. Used by tests and any raw-line path. Mirrors the proc's `hook_proc`.
    pub fn parse_line(&mut self, line: &str) -> bool {
        if let Some(c) = PASS_DESC.captures(line) {
            let id = c["id"].to_string();
            let towns = vec![c["a"].to_string(), c["b"].to_string()];
            self.passes
                .insert(id.clone(), DayPass { towns, expires: 0 });
            self.pending_id = Some(id);
            return true;
        }
        if let Some(c) = PASS_EXPIRED.captures(line) {
            let id = c["id"].to_string();
            self.passes.insert(
                id,
                DayPass {
                    towns: Vec::new(),
                    expires: 0,
                },
            );
            self.pending_id = None;
            return true;
        }
        if let Some(c) = PASS_EXPIRES.captures(line) {
            if let Some(id) = self.pending_id.take() {
                if let Some(epoch) = expiry_epoch(&c) {
                    if let Some(pass) = self.passes.get_mut(&id) {
                        pass.expires = epoch;
                    }
                }
            }
            return true;
        }
        false
    }

    /// Is a valid (unexpired, `now`) pass for the `a`↔`b` town pair held?
    /// Lich: `passes.any? { towns.include?(A) and towns.include?(B) and
    /// expires > Time.now + 10 }`. `now_epoch` is the current Unix time.
    pub fn has_valid_pass(&self, a: &str, b: &str, now_epoch: i64) -> bool {
        self.passes.values().any(|p| {
            p.towns.iter().any(|t| t == a)
                && p.towns.iter().any(|t| t == b)
                && p.expires > now_epoch + 10
        })
    }

    /// The id of a valid pass for the pair, if any (for the `raise #id` USE
    /// path).
    pub fn valid_pass_id(&self, a: &str, b: &str, now_epoch: i64) -> Option<&str> {
        self.passes
            .iter()
            .find(|(_, p)| {
                p.towns.iter().any(|t| t == a)
                    && p.towns.iter().any(|t| t == b)
                    && p.expires > now_epoch + 10
            })
            .map(|(id, _)| id.as_str())
    }

    /// Ids of passes that are expired as of `now` (Lich drops these before
    /// buying: `_drag #id drop`). A pass whose expiry never parsed
    /// (`expires == 0`, description seen but the expiry line was missed) is
    /// NOT dropped — dropping it would throw away a possibly-valid pass.
    pub fn expired_ids(&self, now_epoch: i64) -> Vec<String> {
        self.passes
            .iter()
            .filter(|(_, p)| {
                // A real expiry in the past, or the EXPIRED stamp (towns
                // cleared, expires 0). Towns known but expiry unparsed
                // (expires 0) is NOT dropped — it may still be valid.
                (p.expires != 0 && p.expires < now_epoch - 10)
                    || (p.expires == 0 && p.towns.is_empty())
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Has this pass id been learned (valid OR expired)? The sack scan uses
    /// this to decide which passes still need a `look`.
    pub fn contains(&self, id: &str) -> bool {
        self.passes.contains_key(id)
    }

    /// All learned pass ids, for the sack-sweep prune.
    pub fn ids(&self) -> impl Iterator<Item = &String> {
        self.passes.keys()
    }

    pub fn forget(&mut self, id: &str) {
        self.passes.remove(id);
    }

    pub fn len(&self) -> usize {
        self.passes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }
}

/// Cheap gate: does this line look like day-pass description text? Keeps the
/// regex work off the hot path for ordinary lines. Matches the STRIPPED text
/// shapes the feed sees (markup gone), plus the raw-markup form for raw-line
/// callers.
pub fn line_is_day_pass(line: &str) -> bool {
    line.contains("This pass entitles the original purchaser")
        || line.contains("\"EXPIRED\" appear")
        || line.starts_with("[Your pass will expire on")
        || line.contains("noun=\"pass\">pass")
}

fn expiry_epoch(c: &regex::Captures) -> Option<i64> {
    use chrono::{FixedOffset, TimeZone};
    let g = |name: &str| c.name(name)?.as_str().parse::<u32>().ok();
    let mon = month_num(c.name("mon")?.as_str())?;
    let (day, h, min, s, year) = (g("day")?, g("h")?, g("min")?, g("s")?, g("year")?);
    // Eastern time, matching Lich's `Time.new(…, "-05:00")`.
    let eastern = FixedOffset::west_opt(5 * 3600)?;
    let dt = eastern
        .with_ymd_and_hms(year as i32, mon, day, h, min, s)
        .single()?;
    Some(dt.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESC: &str = r#"Bold calligraphy states simply, "This <a exist="12345" noun="pass">pass</a> entitles the original purchaser to one (1) day of unlimited travel between the towns of Solhaven and Wehnimer's Landing, commencing at the eleventh hour."#;
    const EXPIRES: &str = "[Your pass will expire on Fri Aug 22 14:30:00 ET 2038.";

    #[test]
    fn description_then_expiry_builds_a_valid_pass() {
        let mut c = DayPassCache::default();
        assert!(c.parse_line(DESC));
        assert!(c.parse_line(EXPIRES));
        assert_eq!(c.len(), 1);
        // Far-future expiry → valid now, in either town order.
        let now = 0; // 1970; the 2038 pass is well in the future
        assert!(c.has_valid_pass("Solhaven", "Wehnimer's Landing", now));
        assert!(c.has_valid_pass("Wehnimer's Landing", "Solhaven", now));
        assert_eq!(
            c.valid_pass_id("Solhaven", "Wehnimer's Landing", now),
            Some("12345")
        );
        // A pair it doesn't cover.
        assert!(!c.has_valid_pass("Solhaven", "Icemule Trace", now));
    }

    #[test]
    fn expired_stamp_marks_the_pass_dead() {
        let mut c = DayPassCache::default();
        c.parse_line(DESC);
        c.parse_line(EXPIRES);
        let expired = r#"Bold red block letters spelling out "EXPIRED" appear to have been stamped across the face and reverse of the <a exist="12345" noun="pass">pass</a>."#;
        assert!(c.parse_line(expired));
        assert!(!c.has_valid_pass("Solhaven", "Wehnimer's Landing", 0));
        assert!(c.expired_ids(i64::MAX / 2).contains(&"12345".to_string()));
    }

    #[test]
    fn expiry_before_now_is_not_valid() {
        let mut c = DayPassCache::default();
        c.parse_line(DESC);
        c.parse_line("[Your pass will expire on Fri Aug 22 14:30:00 ET 2020.");
        // 2020 expiry vs a 2026-era now → expired.
        let now = 1_760_000_000; // ~2025
        assert!(!c.has_valid_pass("Solhaven", "Wehnimer's Landing", now));
    }

    #[test]
    fn line_gate_matches_only_day_pass_lines() {
        assert!(line_is_day_pass(DESC));
        assert!(line_is_day_pass(EXPIRES));
        assert!(!line_is_day_pass("A kobold wanders in."));
    }

    #[test]
    fn observe_uses_the_supplied_pass_id() {
        // The feed path strips <a> markup to plain text and supplies the id.
        let mut c = DayPassCache::default();
        let desc = "This pass entitles the original purchaser to one (1) day of unlimited travel between the towns of Solhaven and Wehnimer's Landing, commencing at dawn.";
        assert!(c.observe(desc, Some("999")));
        assert!(c.observe(EXPIRES, None)); // expiry line has no link
        assert_eq!(
            c.valid_pass_id("Solhaven", "Wehnimer's Landing", 0),
            Some("999")
        );
        // No id supplied → the description is ignored (can't key it).
        let mut c2 = DayPassCache::default();
        assert!(!c2.observe(desc, None));
    }

    #[test]
    fn observe_matches_the_wire_shaped_stripped_lines() {
        // The feed strips <a> markup but KEEPS the lead-in text — the lines the
        // client actually hands to observe() look like these (the live bug:
        // the old gate wanted raw markup and the old regex was ^-anchored).
        let desc = r#"Bold calligraphy states simply, "This pass entitles the original purchaser to one (1) day of unlimited travel between the towns of Solhaven and Wehnimer's Landing, commencing at the eleventh hour."#;
        assert!(line_is_day_pass(desc));
        let mut c = DayPassCache::default();
        assert!(c.observe(desc, Some("321")));
        assert!(c.observe(EXPIRES, None));
        assert!(c.has_valid_pass("Solhaven", "Wehnimer's Landing", 0));
        // The stripped EXPIRED stamp line.
        let expired = r#"Bold red block letters spelling out "EXPIRED" appear to have been stamped across the face and reverse of the pass."#;
        assert!(line_is_day_pass(expired));
        assert!(c.observe(expired, Some("321")));
        assert!(!c.has_valid_pass("Solhaven", "Wehnimer's Landing", 0));
        // A random line containing EXPIRED elsewhere is NOT treated as a stamp.
        assert!(!c.observe("The sign reads EXPIRED PERMITS DESK", Some("9")));
    }

    #[test]
    fn buy_permits_matches_bare_on_and_pair_codes() {
        let (a, b) = ("Solhaven", "Wehnimer's Landing"); // codes sol,wl / wl,sol
        assert!(buy_permits("yes", a, b));
        assert!(buy_permits("on", a, b));
        assert!(buy_permits("true", a, b));
        assert!(buy_permits("sol,wl", a, b));
        assert!(buy_permits("wl,sol", a, b)); // reverse code also permits
        assert!(buy_permits("imt,wl sol,wl", a, b)); // in a list
                                                     // Off / empty / a different pair → not permitted.
        assert!(!buy_permits("", a, b));
        assert!(!buy_permits("off", a, b));
        assert!(!buy_permits("no", a, b));
        assert!(!buy_permits("sol,imt", a, b));
        // A different pair with a matching config.
        assert!(buy_permits("sol,imt", "Solhaven", "Icemule Trace"));
        // The Elven Nations pairs go2's validation accepts (go2.lic:440):
        // codes resolve even though the mapdb ships no eastern buy procs yet.
        assert!(buy_permits("ill,val", "Ta'Illistim", "Ta'Vaalor"));
        assert!(buy_permits("cys,ill", "Ta'Illistim", "Cysaegir"));
        assert!(buy_permits("val,cys", "Cysaegir", "Ta'Vaalor"));
        assert!(!buy_permits("ill,val", "Ta'Illistim", "Cysaegir"));
    }

    #[test]
    fn buy_disabled_flag_starts_clear_and_sticks() {
        let mut cache = DayPassCache::default();
        assert!(!cache.buy_disabled());
        cache.disable_buy();
        assert!(cache.buy_disabled());
    }

    #[test]
    fn departure_table_covers_the_six_edges() {
        // Each departure has 2 destinations; all 6 edges resolve.
        assert_eq!(
            DEPARTURES
                .iter()
                .map(|d| d.destinations.len())
                .sum::<usize>(),
            6
        );
        assert!(is_departure(14359) && is_departure(8635) && is_departure(15619));
        let (dep, dest) = edge(8635, 8916).expect("Wehnimer's -> Icemule edge");
        assert_eq!(dep.npc, "clerk");
        assert_eq!(dest.ask_word, "icemule");
        assert_eq!(dest.pair, ("Icemule Trace", "Wehnimer's Landing"));
        assert!(edge(8635, 99999).is_none());
    }
}
