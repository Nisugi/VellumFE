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

// --- cheap-win idioms (go2 plan P3) ---

// 69× ";e empty_hands; move 'climb footpath'; fill_hands"  (Niffy's footpath)
// also the waitrt? variants: ";e empty_hands; move 'X'; waitrt?" (fill implied
// by move's own recovery) and ";e empty_hands move 'X' waitrt? fill_hands".
// empty_hands stows held items, the move climbs, fill_hands puts them back.
re!(
    EMPTY_HANDS_MOVE,
    r"^;e\s+empty_hands[;\s]+move\s*\(?'([^']+)'\)?[;\s]*(waitrt\?)?[;\s]*(fill_hands;?)?$"
);
// 31× ";e Map[3600].wayto['3600'].call;" — delegation: run another edge's
// wayto script. We resolve it to that edge's transpiled actions.
re!(WAYTO_DELEGATE, r"^;e\s+Map\[(\d+)\]\.wayto\['(\d+)'\]\.call;?$");
// 29× ";e move 'go curtain'; $go2_restart = true" — move then force a replan.
re!(
    MOVE_RESTART,
    r"^;e\s+move\s*\(?'([^']+)'\)?;?\s*\$go2_restart\s*=\s*true;?$"
);
// 40× double/multi fput then move: ";e fput 'unlock door'; fput 'open door'; move 'go door'"
re!(
    FPUTS_MOVE,
    r"^;e\s+((?:fput\s*\(?'[^']+'\)?;\s*)+)move\s*\(?'([^']+)'\)?;?$"
);
// Single-quote inside the fput list, extracted per-command.
re!(FPUT_ONE, r"fput\s*\(?'([^']+)'\)?");
// 29× bare fput, no move (a lever/button that changes the room): ";e fput 'jump'"
re!(BARE_FPUT, r"^;e\s+fput\s*\(?'([^']+)'\)?;?$");
// dquote fput+move: ";e fput \"search\"; move \"go trapdoor\""
re!(
    FPUT_MOVE_DQ,
    r#"^;e\s+fput\s*\(?"([^"]+)"\)?;?\s*move\s*\(?"([^"]+)"\)?;?$"#
);
// Passive "wait for the room to change" edge: ";e wait_until{Map.current.id
// != 30812}". No command to send — a prior move's momentum (a jump landing
// across several rooms) carries you. Chained on routes like Widowmaker's
// Road. Transpiles to no actions: the executor simply awaits arrival at the
// expected room, which IS "wait until the room changes".
re!(
    WAIT_UNTIL_ROOM_CHANGE,
    r"^;e\s+wait_until\s*\{\s*Map\.current\.id\s*!=\s*\d+\s*\}\s*;?$"
);
// Stand-guard then move, with an optional $go2_restart:
// ";e fput 'stand' unless standing?;move('jump'); $go2_restart=true".
// The stand only fires when not standing; the move always runs; the restart
// (when present) forces a re-plan (the jump lands somewhere unpredictable).
re!(
    STAND_GUARD_MOVE,
    r"^;e\s+fput\s*\(?'stand'\)?\s+unless\s+standing\?;?\s*move\s*\(?'([^']+)'\)?;?\s*(\$go2_restart\s*=\s*true;?)?$"
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
    // --- cheap-win idioms (P3) ---
    if let Some(c) = EMPTY_HANDS_MOVE.captures(src) {
        let mut actions = vec![WalkAction::EmptyHands, WalkAction::Move(c[1].to_string())];
        if c.get(2).is_some() {
            actions.push(WalkAction::WaitRt);
        }
        // fill_hands runs whether or not it was written literally: an
        // empty_hands with no matching fill would strand the items, and
        // Lich's `move` refills on success anyway.
        actions.push(WalkAction::FillHands);
        return Some(actions);
    }
    if let Some(c) = MOVE_RESTART.captures(src) {
        return Some(vec![WalkAction::Move(c[1].to_string()), WalkAction::Replan]);
    }
    if let Some(c) = FPUTS_MOVE.captures(src) {
        let mut actions: Vec<WalkAction> = FPUT_ONE
            .captures_iter(&c[1])
            .map(|m| WalkAction::Put(m[1].to_string()))
            .collect();
        actions.push(WalkAction::Move(c[2].to_string()));
        return Some(actions);
    }
    if let Some(c) = FPUT_MOVE_DQ.captures(src) {
        return Some(vec![
            WalkAction::Put(c[1].to_string()),
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    if let Some(c) = BARE_FPUT.captures(src) {
        // A lone fput that changes the room (pull lever, jump). No arrival
        // command follows; the room change is the fput's own effect.
        return Some(vec![WalkAction::Put(c[1].to_string())]);
    }
    if WAIT_UNTIL_ROOM_CHANGE.is_match(src) {
        // Passive wait edge: send nothing, just await the room change. Empty
        // actions → the executor goes straight to AwaitArrival for this edge.
        return Some(Vec::new());
    }
    if let Some(c) = STAND_GUARD_MOVE.captures(src) {
        // The `fput 'stand' unless standing?` guard is redundant with the
        // executor's own pre-move stand logic (it stands before any move when
        // not upright), so we drop it and keep just the move. The trailing
        // `$go2_restart` (the jump lands unpredictably) needs no action: if we
        // arrive somewhere other than `expected`, the executor's off-route
        // handling already re-paths from the landing room — running Replan
        // inline here would re-path *before* the move lands.
        return Some(vec![WalkAction::Move(c[1].to_string())]);
    }
    None
}

/// Cheap admission check for the pathfinder: can this scripted edge be
/// walked? (Same patterns as `transpile`, minus the allocation.)
pub fn transpilable(source: &str) -> bool {
    transpile(source).is_some()
}

/// db-aware transpile: like `transpile`, but resolves a
/// `Map[N].wayto['M'].call` delegation by looking up that edge's wayto
/// command and transpiling it. Falls back to plain `transpile` for
/// everything else. Depth-limited so a delegation cycle can't loop.
pub fn transpile_edge(db: &MapDb, source: &str) -> Option<Vec<WalkAction>> {
    transpile_edge_depth(db, source, 0)
}

fn transpile_edge_depth(db: &MapDb, source: &str, depth: u8) -> Option<Vec<WalkAction>> {
    if depth >= 3 {
        return None;
    }
    let src = source.trim();
    if let Some(c) = WAYTO_DELEGATE.captures(src) {
        let room_id: u32 = c[1].parse().ok()?;
        let dest: u32 = c[2].parse().ok()?;
        let target = db.room(room_id)?.wayto.get(&dest)?;
        return transpile_edge_depth(db, target, depth + 1);
    }
    transpile(src)
}

// --- timeto cost procs ---

// 957× ";e Map[N].timeto['M'].call;"
re!(TIMETO_DELEGATE, r"^;e\s+Map\[(\d+)\]\.timeto\['(\d+)'\]\.call;?$");
// 83× ";e checksitting && Room.current.climate == '...' ? 30 : 0.2"
re!(
    TIMETO_TERNARY,
    r"^;e\s+checksitting && Room\.current\.climate == '[^']*' \? ([\d.]+) : ([\d.]+)$"
);
// Voln Symbol of Seeking edges: ";e if Society.status == 'Order of Voln' and
// Society.rank == 26 and $go2_use_seeking; 2.8; else; nil; end". Routable only
// when the character is a Voln Master AND seeking is enabled — Lich reads the
// $go2_use_seeking global directly, so we mirror it with an atomic set only
// when can_seek() holds (see set_use_seeking). Captures the enabled cost.
re!(
    TIMETO_SEEKING,
    r"^;e\s+if Society\.status == 'Order of Voln' and Society\.rank == 26 and \$go2_use_seeking;\s*([\d.]+);\s*else;\s*nil;\s*end$"
);

/// Whether Voln seeking edges are routable this session — the Rust analog of
/// Lich's `$go2_use_seeking` global (the seeking timeto procs read it
/// directly). Only settable true when the character can actually seek (a Voln
/// Master), via [`set_use_seeking`], so a true value at query time means the
/// edge is legitimately available.
static USE_SEEKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enable/disable Voln seeking routing. `enabling && !can_seek` is rejected
/// (returns false) — you can't turn seeking on without being a Voln Master.
/// Returns the resulting state.
pub fn set_use_seeking(enable: bool, can_seek: bool) -> bool {
    let value = enable && can_seek;
    USE_SEEKING.store(value, std::sync::atomic::Ordering::Relaxed);
    value
}

/// Whether seeking is currently enabled.
pub fn use_seeking() -> bool {
    USE_SEEKING.load(std::sync::atomic::Ordering::Relaxed)
}

// Portmaster edges: ";e UserVars.mapdb_use_portmasters == true ? 1200 : nil".
// The number is the silver COST (the funding routine withdraws it). Routable
// only when portmasters are enabled — mirrors Lich reading the UserVar.
re!(
    TIMETO_PORTMASTER,
    r"^;e\s+UserVars\.mapdb_use_portmasters == true \? ([\d.]+) : nil$"
);

/// Whether portmaster edges are routable (Lich's `UserVars.mapdb_use_portmasters`).
static USE_PORTMASTERS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Enable/disable portmaster routing.
pub fn set_use_portmasters(enable: bool) {
    USE_PORTMASTERS.store(enable, std::sync::atomic::Ordering::Relaxed);
}

/// Whether portmaster routing is enabled.
pub fn use_portmasters() -> bool {
    USE_PORTMASTERS.load(std::sync::atomic::Ordering::Relaxed)
}

// Urchin hub-entry edges: the full gate proc. Routable (0.1) only when urchin
// travel is currently valid — the caller folds use_urchins + not-expired +
// not-hidden/invisible into one flag (set_urchins_valid), matching Lich's
// combined condition. Also matches the hub-delegate form
// (";e Map[N].timeto['M'].call") indirectly via the delegate handler, and the
// "a travel script is running" hub-exit form (always true mid-trip for us).
re!(
    TIMETO_URCHIN,
    r"^;e\s+UserVars\.mapdb_use_urchins == true and .*mapdb_urchins_expire.* \? ([\d.]+) : nil;?$"
);
// The hub-exit gate: ";e !(Script.list... & %w{go2 route2}).empty? ? 0.1 :
// nil" — routable while a travel script runs. For us that's "always during a
// trip", so it rides the same URCHINS_VALID flag (only set while traveling).
re!(
    TIMETO_URCHIN_RUNNING,
    r"^;e\s+!\(Script\.list.*route2.*\)\.empty\? \? ([\d.]+) : nil;?$"
);

/// Whether urchin-guide travel is routable right now — the caller sets this
/// from `use_urchins && character.urchins_valid(...)` at trip start. Also lets
/// dijkstra lift its urchin-hideout exclusion (see edge_cost).
static URCHINS_VALID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enable/disable urchin routing for this session.
pub fn set_urchins_valid(valid: bool) {
    URCHINS_VALID.store(valid, std::sync::atomic::Ordering::Relaxed);
}

/// Whether urchin routing is currently valid.
pub fn urchins_valid() -> bool {
    URCHINS_VALID.load(std::sync::atomic::Ordering::Relaxed)
}

// Day-pass gate. The inlined proc ends with a decision block whose "own a
// valid pass" arm checks the town pair by full name, e.g.
// `…towns.include?("Solhaven") and towns.include?("Wehnimer's Landing")…`. We
// pull the pair out of the (large, inlined) body and look it up in a
// process-global map of routable pairs → cost, populated at trip start from
// the live day-pass cache + config (0.8 = a valid pass is held, 7.4 = no pass
// but buying is enabled for this pair). Anything not in the map is nil.
re!(
    DAY_PASS_PAIR,
    r#"towns\.include\?\("(?P<a>[^"]+)"\) and .*?towns\.include\?\("(?P<b>[^"]+)"\)"#
);

/// Routable day-pass town pairs → edge cost, keyed by a normalized
/// `"a\u{1}b"` pair (order-independent). Empty unless day-pass travel is on.
static DAY_PASS_ROUTABLE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, f64>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Order-independent key for a town pair.
fn day_pass_key(a: &str, b: &str) -> String {
    if a <= b {
        format!("{a}\u{1}{b}")
    } else {
        format!("{b}\u{1}{a}")
    }
}

/// Replace the routable day-pass pairs for this trip (called from
/// start_travel). `pairs` maps a `(town_a, town_b)` to the edge cost to use.
pub fn set_day_pass_routable(pairs: &[((String, String), f64)]) {
    let mut map = DAY_PASS_ROUTABLE.lock().expect("day-pass lock");
    map.clear();
    for ((a, b), cost) in pairs {
        map.insert(day_pass_key(a, b), *cost);
    }
}

fn day_pass_cost(a: &str, b: &str) -> Option<f64> {
    DAY_PASS_ROUTABLE
        .lock()
        .expect("day-pass lock")
        .get(&day_pass_key(a, b))
        .copied()
}

/// Serializes tests across the pathing module that touch the process-global
/// travel gates (seeking/portmaster/urchins/day-pass), since cargo runs tests
/// in parallel and they share those atomics.
#[cfg(test)]
pub(crate) static GATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
            if let Some(c) = TIMETO_SEEKING.captures(src) {
                // Routable only when seeking is enabled (which is only
                // possible for a Voln Master — set_use_seeking gates it).
                return use_seeking().then(|| c[1].parse().ok()).flatten();
            }
            if let Some(c) = TIMETO_PORTMASTER.captures(src) {
                // Routable only when portmasters are enabled; the number is
                // the silver cost the funding routine covers.
                return use_portmasters().then(|| c[1].parse().ok()).flatten();
            }
            if let Some(c) = TIMETO_URCHIN.captures(src).or_else(|| TIMETO_URCHIN_RUNNING.captures(src)) {
                // Routable only when urchin travel is currently valid (enabled,
                // access not expired, not hidden/invisible).
                return urchins_valid().then(|| c[1].parse().ok()).flatten();
            }
            // Day-pass edge: pull the town pair out of the (large, inlined)
            // decision block and look up its routable cost for this trip. The
            // caller (start_travel) precomputes which pairs are routable from
            // the live day-pass cache + config (0.8 = valid pass held, 7.4 =
            // buyable). A pair not in the map is nil (not routable).
            if let Some(c) = DAY_PASS_PAIR.captures(src) {
                return day_pass_cost(&c["a"], &c["b"]);
            }
            // Other event vars ($mapdb_instability_timeto) and everything
            // else: off.
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) use super::GATE_LOCK;

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
    fn cheap_win_idioms_transpile() {
        use WalkAction::*;
        // The footpath family (Niffy's bug): empty_hands -> move -> fill_hands.
        assert_eq!(
            transpile(";e empty_hands; move 'climb footpath'; fill_hands"),
            Some(vec![EmptyHands, Move("climb footpath".into()), FillHands])
        );
        // Live case (Widowmaker's Road 30811->30812): fput then a
        // parenthesized move, no space after the semicolon.
        assert_eq!(
            transpile(";e fput 'stance def';move('jump')"),
            Some(vec![Put("stance def".into()), Move("jump".into())])
        );
        // waitrt? variant, fill_hands implied.
        assert_eq!(
            transpile(";e empty_hands; move 'climb mountainside'; waitrt?"),
            Some(vec![
                EmptyHands,
                Move("climb mountainside".into()),
                WaitRt,
                FillHands
            ])
        );
        // no-semicolon spacing variant.
        assert_eq!(
            transpile(";e empty_hands move 'go boat' waitrt? fill_hands"),
            Some(vec![
                EmptyHands,
                Move("go boat".into()),
                WaitRt,
                FillHands
            ])
        );
        // move then $go2_restart.
        assert_eq!(
            transpile(";e move 'go curtain'; $go2_restart = true"),
            Some(vec![Move("go curtain".into()), Replan])
        );
        // multi-fput then move.
        assert_eq!(
            transpile(";e fput 'unlock ironwood door'; fput 'open ironwood door'; move 'go ironwood door'"),
            Some(vec![
                Put("unlock ironwood door".into()),
                Put("open ironwood door".into()),
                Move("go ironwood door".into())
            ])
        );
        // dquote fput+move.
        assert_eq!(
            transpile(r#";e fput "search"; move "go wooden trapdoor""#),
            Some(vec![Put("search".into()), Move("go wooden trapdoor".into())])
        );
        // bare fput (a lever/button).
        assert_eq!(transpile(";e fput 'jump'"), Some(vec![Put("jump".into())]));
        // Live Widowmaker case: fput a stance then a parenthesized jump.
        assert_eq!(
            transpile(";e fput 'stance def';move('jump')"),
            Some(vec![Put("stance def".into()), Move("jump".into())])
        );
        // Passive wait-for-room-change edge → no actions (executor awaits).
        assert_eq!(
            transpile(";e wait_until{Map.current.id != 30812}"),
            Some(vec![])
        );
        // Stand-guard + move + restart (Widowmaker 30815->30816): the stand
        // guard is dropped (executor stands anyway), the move runs, and the
        // restart is handled by off-route re-pathing after landing.
        assert_eq!(
            transpile(";e fput 'stand' unless standing?;move('jump'); $go2_restart=true"),
            Some(vec![Move("jump".into())])
        );
    }

    #[test]
    fn wayto_delegation_resolves_through_the_db() {
        use WalkAction::*;
        // Room 1's edge delegates to room 2's wayto, which is a plain move.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[R1]"],
                 "wayto": {"3": ";e Map[2].wayto['3'].call;"},
                 "timeto": {"3": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[R2]"],
                 "wayto": {"3": ";e move 'go gate'; waitrt?"},
                 "timeto": {"3": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let edge = db.room(1).unwrap().wayto.get(&3).unwrap();
        assert_eq!(
            transpile_edge(&db, edge),
            Some(vec![Move("go gate".into()), WaitRt])
        );
        // Plain edges pass straight through transpile_edge.
        assert_eq!(
            transpile_edge(&db, ";e move 'north'"),
            Some(vec![Move("north".into())])
        );
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
        // The gated globals are process-shared; lock + pin for determinism.
        let _g = GATE_LOCK.lock().unwrap();
        set_use_portmasters(false);
        assert_eq!(resolve_timeto(&db, r1, 2), Some(40.5), "delegation follows");
        assert_eq!(resolve_timeto(&db, r2, 3), Some(30.0), "ternary takes the max");
        assert_eq!(resolve_timeto(&db, r2, 4), None, "portmasters default off");
        assert_eq!(resolve_timeto(&db, r2, 1), Some(40.5));
    }

    #[test]
    fn seeking_edges_route_only_when_enabled() {
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": ";e move 'go sigil'"},
                 "timeto": {"2": ";e if Society.status == 'Order of Voln' and Society.rank == 26 and $go2_use_seeking; 2.8; else; nil; end"},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[B]"],
                 "wayto": {"1": "back"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let r1 = db.room(1).unwrap();
        let _g = GATE_LOCK.lock().unwrap();

        // Default off → the seeking edge is unroutable.
        set_use_seeking(false, true);
        assert_eq!(resolve_timeto(&db, r1, 2), None, "off by default");

        // A non-Voln can't enable it (the gate returns false).
        assert!(!set_use_seeking(true, false), "non-seeker can't enable");
        assert_eq!(resolve_timeto(&db, r1, 2), None, "still off for non-seeker");

        // A Voln Master enabling it → the edge costs 2.8 (routable).
        assert!(set_use_seeking(true, true), "Voln Master enables seeking");
        assert_eq!(resolve_timeto(&db, r1, 2), Some(2.8), "seeking routes");

        // Reset so the global doesn't leak into other tests.
        set_use_seeking(false, true);
    }

    #[test]
    fn portmaster_edges_route_when_enabled_and_transpile() {
        use WalkAction::*;
        // The crossing already transpiles via the multifput+waitfor pattern.
        assert_eq!(
            transpile(";e multifput 'ask portmaster about travel 4','ask portmaster about travel 4';waitfor 'A crew member escorts you off the ship.'"),
            Some(vec![
                Put("ask portmaster about travel 4".into()),
                Move("ask portmaster about travel 4".into())
            ])
        );
        // The timeto gates routing on the portmaster flag.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Dock]"],
                 "wayto": {"2": ";e multifput 'ask portmaster about travel 1','ask portmaster about travel 1';waitfor 'A crew member escorts you off the ship.'"},
                 "timeto": {"2": ";e UserVars.mapdb_use_portmasters == true ? 1200 : nil"},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Far Port]"],
                 "wayto": {"1": "board"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let r1 = db.room(1).unwrap();
        let _g = GATE_LOCK.lock().unwrap();
        set_use_portmasters(false);
        assert_eq!(resolve_timeto(&db, r1, 2), None, "off by default");
        set_use_portmasters(true);
        assert_eq!(resolve_timeto(&db, r1, 2), Some(1200.0), "routes when enabled");
        set_use_portmasters(false);
    }

    #[test]
    fn urchin_edges_route_only_when_valid() {
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Street]"],
                 "wayto": {"2": "urchin guide bank"},
                 "timeto": {"2": ";e UserVars.mapdb_use_urchins == true and !UserVars.mapdb_urchins_expire.nil? and Time.now.to_i < UserVars.mapdb_urchins_expire and !hidden? and !invisible? ? 0.1 : nil;"},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Bank]"],
                 "wayto": {"1": "out"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let r1 = db.room(1).unwrap();
        let _g = GATE_LOCK.lock().unwrap();
        set_urchins_valid(false);
        assert_eq!(resolve_timeto(&db, r1, 2), None, "off/expired by default");
        set_urchins_valid(true);
        assert_eq!(resolve_timeto(&db, r1, 2), Some(0.1), "routes when valid");
        set_urchins_valid(false);
    }

    #[test]
    fn day_pass_edges_route_only_for_routable_pairs() {
        // The day-pass timeto body names the town pair via `towns.include?`.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Departures]"],
                 "wayto": {"2": "raise pass"},
                 "timeto": {"2": ";e if $x.any? { |i| i.towns.include?(\"Solhaven\") and i.towns.include?(\"Wehnimer's Landing\") }; 0.8; end"},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Dest]"],
                 "wayto": {"1": "raise pass"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let r1 = db.room(1).unwrap();
        let _g = GATE_LOCK.lock().unwrap();
        // Nothing routable → nil.
        set_day_pass_routable(&[]);
        assert_eq!(resolve_timeto(&db, r1, 2), None, "off by default");
        // The exact pair routable at 0.8 (a held pass).
        set_day_pass_routable(&[(("Solhaven".into(), "Wehnimer's Landing".into()), 0.8)]);
        assert_eq!(resolve_timeto(&db, r1, 2), Some(0.8), "routes the held pair");
        // Reverse order still matches (order-independent key).
        set_day_pass_routable(&[(("Wehnimer's Landing".into(), "Solhaven".into()), 7.4)]);
        assert_eq!(resolve_timeto(&db, r1, 2), Some(7.4), "reverse pair + buy cost");
        // A different pair → nil.
        set_day_pass_routable(&[(("Solhaven".into(), "Icemule Trace".into()), 0.8)]);
        assert_eq!(resolve_timeto(&db, r1, 2), None, "other pairs stay off");
        set_day_pass_routable(&[]);
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
