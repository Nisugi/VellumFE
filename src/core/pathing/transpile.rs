//! StringProc → WalkAction transpiler (go2 plan §2 tier 1).
//!
//! Pattern-matches the mapdb's common embedded-Ruby idioms — measured on a
//! real snapshot, the shapes below cover roughly a quarter of the ~8k
//! scripted wayto edges (the rest are Confluence/event/maze code that stays
//! out of scope; those edges remain unroutable and path failures say so).
//! Every pattern was taken from the live corpus, not imagined; see the
//! env-gated corpus test in `tests/pathing.rs` for the coverage report.
//!
//! timeto procs get their own small evaluator: cost delegation
//! (`Map[N].timeto['M'].call`) resolves through the referenced entry, the
//! sitting/climate ternary takes its larger constant (pessimistic ETA,
//! same walkability), and settings-gated costs (portmasters, seeking)
//! evaluate as "off" — their v1 defaults.

use std::sync::LazyLock;

use regex::Regex;

use super::edge::{Cond, WalkAction};
use crate::core::mapdb::{MapDb, Room, TimeTo};

macro_rules! re {
    ($name:ident, $pattern:literal) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($pattern).expect("valid pattern"));
    };
}

// --- wayto command idioms (counts from map-1783474051.json) ---

// 523× ";e true"
re!(TRUE, r"^;e\s+true$");
// 459× ";e move 'climb rope'; waitrt?"  /  96× ";e move 'go door'"
re!(MOVE, r"^;e\s+move\s*\(?'([^']+)'\)?\s*(;\s*waitrt\?)?;?$");
// 113× ";e if checkspell(103) then move 'go mist' else move 'go arch' end; waitrt?"
re!(
    SPELL_MOVE,
    r"^;e\s+if checkspell\((\d+)\) then move '([^']+)' else move '([^']+)' end\s*(;\s*waitrt\?)?;?$"
);
// 82× ";e dothistimeout 'push wall',3,/you push|you can't push/i;waitrt?"
re!(
    DOTHIS,
    r"^;e\s+dothistimeout '([^']+)',\s*[\d.]+,\s*/.*/[a-z]*\s*(;\s*waitrt\?)?;?$"
);
// 80× ";e if checksitting;while Room.current.id == N;fput('out');waitrt?;end;else;move('out');end;"
re!(
    SITTING_GUARD,
    r"^;e\s+if checksitting;while Room\.current\.id == \d+;fput\('([^']+)'\);waitrt\?;end;else;move\('([^']+)'\);end;?$"
);
// 62× ";e multifput 'pull lever','go gate';waitfor 'The gate'"
re!(
    MULTIFPUT,
    r"^;e\s+multifput '([^']+)','([^']+)'\s*(?:;waitfor '[^']*')?;?$"
);
// 61× ";e fput 'open door'; move 'go door'"  /  35× with a newline
re!(
    FPUT_MOVE,
    r"^;e\s+fput '([^']+)'(?:;\s*|\n)move '([^']+)';?$"
);
// 60× ";e 3.times { move 'swim north'; break if Room.current.id == N }"
re!(
    TIMES_MOVE,
    r"^;e\s+\d+\.times \{ move '([^']+)'; break if Room\.current\.id == \d+ \};?$"
);
// 50× ";e pause 0.5; waitrt?; fput 'go turnstile'"
re!(
    PAUSE_FPUT,
    r"^;e\s+pause ([\d.]+); waitrt\?; fput '([^']+)';?$"
);
// 37× ";e fput 'stoop' unless kneeling? or (Stats.race =~ /.../); move 'crawl west'"
// Race is unknowable client-side; kneeling gates the fput, the move always runs.
re!(
    KNEEL_GUARD,
    r"^;e\s+fput '([^']+)' unless kneeling\?[^;]*;\s*move '([^']+)';?$"
);
// 150× ice-mode: the conditional only warns and sleeps; the move always runs.
re!(
    ICE_MODE,
    r"^;e\s+if \(UserVars\.mapdb_ice_mode == '[^']*'\).*end;\s*move '([^']+)';?$"
);
// 156× pedal loops: ";e direction=\"southeast\";start=Room.current.id; dothistimeout \"pedal #{direction}\", 15, /pedal/ while Room.current.id == start"
re!(
    PEDAL,
    r#"^;e\s+direction="(\w+)";start=Room\.current\.id;\s*dothistimeout "pedal \#\{direction\}",\s*\d+,\s*/pedal/ while Room\.current\.id == start$"#
);

/// Transpile a StringProc wayto command. `None` = unsupported (edge stays
/// out of the graph).
pub fn transpile(source: &str) -> Option<Vec<WalkAction>> {
    let src = source.trim();
    if TRUE.is_match(src) {
        return Some(vec![WalkAction::Noop]);
    }
    if let Some(c) = MOVE.captures(src) {
        let mut actions = vec![WalkAction::Move(c[1].to_string())];
        if c.get(2).is_some() {
            actions.push(WalkAction::WaitRt);
        }
        return Some(actions);
    }
    if let Some(c) = SPELL_MOVE.captures(src) {
        let spell: u16 = c[1].parse().ok()?;
        return Some(vec![WalkAction::If {
            cond: Cond::SpellActive(spell),
            then: vec![WalkAction::Move(c[2].to_string())],
            els: vec![WalkAction::Move(c[3].to_string())],
        }]);
    }
    if let Some(c) = DOTHIS.captures(src) {
        // The executor's retry-on-timeout replays the edge, which is the
        // dothistimeout loop with a longer period.
        return Some(vec![WalkAction::Move(c[1].to_string())]);
    }
    if let Some(c) = SITTING_GUARD.captures(src) {
        return Some(vec![WalkAction::If {
            cond: Cond::Sitting,
            then: vec![WalkAction::Move(c[1].to_string())],
            els: vec![WalkAction::Move(c[2].to_string())],
        }]);
    }
    if let Some(c) = MULTIFPUT.captures(src) {
        return Some(vec![
            WalkAction::Put(c[1].to_string()),
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    if let Some(c) = FPUT_MOVE.captures(src) {
        return Some(vec![
            WalkAction::Put(c[1].to_string()),
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    if let Some(c) = TIMES_MOVE.captures(src) {
        return Some(vec![WalkAction::Move(c[1].to_string())]);
    }
    if let Some(c) = PAUSE_FPUT.captures(src) {
        let seconds: f32 = c[1].parse().ok()?;
        return Some(vec![
            WalkAction::Sleep(seconds),
            WalkAction::WaitRt,
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    if let Some(c) = KNEEL_GUARD.captures(src) {
        return Some(vec![
            WalkAction::If {
                cond: Cond::Kneeling,
                then: vec![],
                els: vec![WalkAction::Put(c[1].to_string())],
            },
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    if let Some(c) = ICE_MODE.captures(src) {
        return Some(vec![WalkAction::Move(c[1].to_string())]);
    }
    if let Some(c) = PEDAL.captures(src) {
        return Some(vec![WalkAction::Move(format!("pedal {}", &c[1]))]);
    }
    None
}

/// Cheap admission check for the pathfinder: can this scripted edge be
/// walked? (Same patterns as `transpile`, minus the allocation.)
pub fn transpilable(source: &str) -> bool {
    transpile(source).is_some()
}

// --- timeto cost procs ---

// 957× ";e Map[N].timeto['M'].call;"
re!(TIMETO_DELEGATE, r"^;e\s+Map\[(\d+)\]\.timeto\['(\d+)'\]\.call;?$");
// 83× ";e checksitting && Room.current.climate == '...' ? 30 : 0.2"
re!(
    TIMETO_TERNARY,
    r"^;e\s+checksitting && Room\.current\.climate == '[^']*' \? ([\d.]+) : ([\d.]+)$"
);

/// Resolve a timeto entry to seconds, following one level of delegation.
/// `None` = edge disabled (settings-gated costs default off in v1).
pub fn resolve_timeto(db: &MapDb, room: &Room, dest: u32) -> Option<f64> {
    resolve_timeto_depth(db, room.timeto.get(&dest)?, 0)
}

fn resolve_timeto_depth(db: &MapDb, timeto: &TimeTo, depth: u8) -> Option<f64> {
    match timeto {
        TimeTo::Seconds(s) if *s >= 0.0 => Some(*s),
        TimeTo::Seconds(_) => None,
        TimeTo::Proc(src) => {
            if depth >= 3 {
                return None;
            }
            let src = src.trim();
            if let Some(c) = TIMETO_DELEGATE.captures(src) {
                let room_id: u32 = c[1].parse().ok()?;
                let dest: u32 = c[2].parse().ok()?;
                let target = db.room(room_id)?.timeto.get(&dest)?;
                return resolve_timeto_depth(db, target, depth + 1);
            }
            if let Some(c) = TIMETO_TERNARY.captures(src) {
                let a: f64 = c[1].parse().ok()?;
                let b: f64 = c[2].parse().ok()?;
                // Pessimistic constant: same walkability, honest ETA.
                return Some(a.max(b));
            }
            // Settings gates (portmasters, seeking), event vars
            // ($mapdb_instability_timeto), and everything else: off.
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_idioms_transpile() {
        use WalkAction::*;
        assert_eq!(transpile(";e true"), Some(vec![Noop]));
        assert_eq!(
            transpile(";e move 'climb rope'; waitrt?"),
            Some(vec![Move("climb rope".into()), WaitRt])
        );
        assert_eq!(transpile(";e move 'go door'"), Some(vec![Move("go door".into())]));
        assert_eq!(
            transpile(";e if checkspell(103) then move 'go mist' else move 'go arch' end; waitrt?"),
            Some(vec![If {
                cond: Cond::SpellActive(103),
                then: vec![Move("go mist".into())],
                els: vec![Move("go arch".into())],
            }])
        );
        assert_eq!(
            transpile(";e dothistimeout 'push wall',3,/you push|you can't push/i;waitrt?"),
            Some(vec![Move("push wall".into())])
        );
        assert_eq!(
            transpile(
                ";e if checksitting;while Room.current.id == 8836;fput('out');waitrt?;end;else;move('out');end;"
            ),
            Some(vec![If {
                cond: Cond::Sitting,
                then: vec![Move("out".into())],
                els: vec![Move("out".into())],
            }])
        );
        assert_eq!(
            transpile(";e multifput 'pull lever','go gate';waitfor 'The gate grinds open'"),
            Some(vec![Put("pull lever".into()), Move("go gate".into())])
        );
        assert_eq!(
            transpile(";e fput 'open door'; move 'go door'"),
            Some(vec![Put("open door".into()), Move("go door".into())])
        );
        assert_eq!(
            transpile(";e fput 'open door'\nmove 'go door'"),
            Some(vec![Put("open door".into()), Move("go door".into())])
        );
        assert_eq!(
            transpile(";e 3.times { move 'swim north'; break if Room.current.id == 10538 }"),
            Some(vec![Move("swim north".into())])
        );
        assert_eq!(
            transpile(";e pause 0.5; waitrt?; fput 'go turnstile'"),
            Some(vec![
                Sleep(0.5),
                WaitRt,
                Move("go turnstile".into())
            ])
        );
        assert_eq!(
            transpile(
                ";e fput 'stoop' unless kneeling? or (Stats.race =~ /Dwarf|Halfling|Gnome/); move 'crawl west'"
            ),
            Some(vec![
                If {
                    cond: Cond::Kneeling,
                    then: vec![],
                    els: vec![Put("stoop".into())],
                },
                Move("crawl west".into())
            ])
        );
        assert_eq!(
            transpile(
                ";e if (UserVars.mapdb_ice_mode == 'on') or ((UserVars.mapdb_ice_mode != 'off') and ((XMLData.encumbrance_value > 20) or ((Skills.survival < 50) and not Spell['9504'].active?))); sleep 0.2; echo 'Slippery!'; sleep 2; end; move 'climb slope'"
            ),
            Some(vec![Move("climb slope".into())])
        );
        assert_eq!(
            transpile(
                r#";e direction="southeast";start=Room.current.id; dothistimeout "pedal #{direction}", 15, /pedal/ while Room.current.id == start"#
            ),
            Some(vec![Move("pedal southeast".into())])
        );
        // Out-of-scope code stays out.
        assert_eq!(
            transpile(";e $mapdb_confluence_target = 123; Room[456].wayto['789'].call"),
            None
        );
        assert_eq!(transpile(";e target_room_id = 5; maze_rooms = [1, 2]"), None);
    }

    #[test]
    fn timeto_delegation_and_gates_resolve() {
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[R1]"],
                 "wayto": {"2": "north"},
                 "timeto": {"2": ";e Map[2].timeto['1'].call;"}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[R2]"],
                 "wayto": {"1": "south", "3": "east", "4": "west"},
                 "timeto": {"1": 40.5,
                            "3": ";e checksitting && Room.current.climate == 'snowy' ? 30 : 0.2",
                            "4": ";e UserVars.mapdb_use_portmasters == true ? 240 : nil"},
                 "paths": ""}
            ]"#,
        )
        .unwrap();
        let r1 = db.room(1).unwrap();
        let r2 = db.room(2).unwrap();
        assert_eq!(resolve_timeto(&db, r1, 2), Some(40.5), "delegation follows");
        assert_eq!(resolve_timeto(&db, r2, 3), Some(30.0), "ternary takes the max");
        assert_eq!(resolve_timeto(&db, r2, 4), None, "portmasters default off");
        assert_eq!(resolve_timeto(&db, r2, 1), Some(40.5));
    }

    #[test]
    fn delegation_cycles_terminate() {
        // 1 delegates to 2 delegates back to 1: must not loop forever.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[R1]"],
                 "wayto": {"2": "north"},
                 "timeto": {"2": ";e Map[2].timeto['1'].call;"}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[R2]"],
                 "wayto": {"1": "south"},
                 "timeto": {"1": ";e Map[1].timeto['2'].call;"}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let r1 = db.room(1).unwrap();
        assert_eq!(resolve_timeto(&db, r1, 2), None);
    }
}
