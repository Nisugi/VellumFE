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

use super::edge::{AwaitPattern, Cond, OnTimeout, RepeatUntil, WalkAction};
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
// Also 66x with a global-setting preamble (`$mapdb_seeking_destination = N;`)
// and in the `Room[N]` spelling. The preamble only records a goal for the
// delegated script to read; the delegation is the whole crossing.
re!(
    WAYTO_DELEGATE,
    r#"^;e\s+(?:\$\w+\s*=\s*(?:'[^']*'|"[^"]*"|\d+)\s*;\s*)?(?:Map|Room)\[(\d+)\]\.wayto\['(\d+)'\]\.call;?$"#
);
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

// --- await/repeat idioms (phase 3a) ---

// ";e waitfor 'The gate swings open'; move 'go gate'" — a bare `waitfor` is
// an UNBOUNDED block in Lich. We give it a long-but-finite timeout and treat
// a miss as a failure: the awaited line is the only evidence the crossing
// happened, so continuing past it would walk into the wrong room.
re!(
    WAITFOR_MOVE,
    r#"^;e\s+waitfor\s+['"]([^'"]+)['"]\s*;\s*move\s*\(?['"]([^'"]+)['"]\)?;?$"#
);
// ";e fput 'go gangplank'; waitfor 'lowers the gangplank'; move 'out'" — the
// ferry idiom. The `fput` is the active command, the `waitfor` is its await.
re!(
    FPUT_WAITFOR_MOVE,
    r#"^;e\s+fput\s*\(?['"]([^'"]+)['"]\)?;\s*waitfor\s+['"]([^'"]+)['"]\s*;\s*move\s*\(?['"]([^'"]+)['"]\)?;?$"#
);
// ";e fput 'search' until GameObj.loot.find { |o| o.noun == 'crevice' }; move
// 'go crevice'" — search until something appears, then enter it. Bounded by
// the interpreter's loop ceiling.
re!(
    SEARCH_UNTIL_MOVE,
    r#"^;e\s+fput\s*\(?['"]([^'"]+)['"]\)?\s+until\s+.+?;\s*move\s*\(?['"]([^'"]+)['"]\)?;?$"#
);
// ";e fput 'go fog' while Room.current.id == 24675" — retry a command until
// the room changes (the Red Forest family).
re!(
    FPUT_WHILE_SAME_ROOM,
    r#"^;e\s+(?:fput|dothistimeout)\s*\(?['"]([^'"]+)['"]\)?[^;]*?\s+while\s+Room\.current\.id\s*==\s*\d+;?$"#
);

// ";e result = dothistimeout 'look wall', 5, /The (\w+) door/; move "go #{$1}
// door"" — read a value off a line, then act on it. The capture-binding
// idiom: without it these edges can't be expressed at all, because the
// command isn't knowable until the game answers.
re!(
    MATCH_THEN_MOVE,
    r#"^;e\s+(?:\w+\s*=\s*)?dothistimeout\s+['"]([^'"]+)['"]\s*,\s*[\d.]+\s*,\s*/(.+?)/[a-z]*\s*;\s*move\s*\(?["']([^"']*#\{\$1\}[^"']*)["']\)?;?$"#
);
// ";e if Char.citizenship == 'Solhaven'; move 'go gate'; else; move 'go
// road'; end" — a citizenship/profession/society gate.
re!(
    CITIZENSHIP_GATE,
    r#"^;e\s+if\s+Char\.citizenship\s*==\s*['"]([^'"]+)['"];?\s*move\s*\(?['"]([^'"]+)['"]\)?;?\s*else;?\s*move\s*\(?['"]([^'"]+)['"]\)?;?\s*end;?$"#
);
// ";e echo 'Put a gem in your hand'; pause_script; move 'go portal'" — the
// crossing needs something only the user can do.
re!(
    PAUSE_SCRIPT_MOVE,
    r#"^;e\s+echo\s+['"]([^'"]+)['"]\s*;\s*pause_script;?\s*(?:move\s*\(?['"]([^'"]+)['"]\)?;?)?$"#
);

/// How long a bare `waitfor` waits before giving up. Lich's `waitfor` blocks
/// forever; a client that hangs a trip indefinitely is worse than one that
/// gives up and hands off, so this is "effectively forever, but bounded" —
/// long enough for a scheduled ferry, short enough to not strand a trip.
const WAITFOR_TIMEOUT_SECS: f32 = 1800.0;

// 28x ";e if Spell[112].active?; move 'north';else; move 'swim north';end" —
// the modern Spell[] form of the checkspell ternary.
re!(
    SPELL_ACTIVE_MOVE,
    r#"^;e\s+if\s+Spell\[(\d+)\]\.active\?;\s*move\s*\(?['"]([^'"]+)['"]\)?;?\s*else;\s*move\s*\(?['"]([^'"]+)['"]\)?;?\s*end;?$"#
);
// 26x ";e if resolve = Spell[9704] and resolve.known? and resolve.affordable?
// and not resolve.active?; resolve.cast; end; move 'climb wall'; waitrt?" —
// buff-if-you-can then move. We can't cast, so the move is what matters: the
// spell is an optimization (it makes the climb safer), not a gate.
re!(
    BUFF_THEN_MOVE,
    r#"^;e\s+if\s+\w+\s*=\s*Spell\[\d+\][^;]*;\s*\w+\.cast;\s*end;\s*move\s*\(?['"]([^'"]+)['"]\)?;?\s*(?:waitrt\?;?)?$"#
);
// 27x ";e multifput 'go darkened alleyway', 'go darkened alleyway'" — the
// spaced-comma variant the existing MULTIFPUT regex misses.
re!(
    MULTIFPUT_SPACED,
    r#"^;e\s+multifput\s+['"]([^'"]+)['"]\s*,\s*['"]([^'"]+)['"]\s*$"#
);
// 14x ";e fput 'search';fput 'go hatch'" — two bare fputs, no move.
re!(
    FPUT_FPUT,
    r#"^;e\s+fput\s*\(?['"]([^'"]+)['"]\)?\s*;\s*fput\s*\(?['"]([^'"]+)['"]\)?;?$"#
);

// 1x directly (plus 29 delegations to it): the Isle of Four Winds trinket
// portal. Matched by its distinctive UserVar rather than the whole ~1.5KB
// body, which is inventory-link scraping we replace with registry lookups.
re!(FWI_TRINKET, r"^;e\s+worn\s*=\s*!GameObj\[UserVars\.mapdb_fwi_trinket\]");

// 27x: try a move; if the room didn't change, fix something and retry. The
// trailing `$go2_restart` is optional (it splits the family in the residue
// report but is the same shape).
re!(
    TRY_MOVE,
    r#"^;e\s+room\s*=\s*Room\.current\.id;\s*fput\s*\(?['"]([^'"]+)['"]\)?;\s*if\s*\(\s*room\s*==\s*Room\.current\.id\s*\);\s*fput\s*\(?['"]([^'"]+)['"]\)?;\s*move\s*\(?['"]([^'"]+)['"]\)?;\s*end;?\s*(\$go2_restart\s*=\s*true;?)?$"#
);
// 16x: stand until upright (bounded), move, stand again.
re!(
    STAND_RETRY_MOVE,
    r#"^;e\s+waitrt\?;\s*(\d+)\.times\s*\{\s*if\s+standing\?;\s*break;\s*else;\s*fput\s+'stand';\s*sleep\s+([\d.]+);\s*waitrt\?;\s*end\s*\};\s*move\s*\(?['"]([^'"]+)['"]\)?;.*$"#
);

// 75x: a room -> direction swim table walked until a target room.
re!(
    SWIM_TABLE,
    r#"^;e\s+empty_hand\s+if\s+\[([^\]]*)\]\.include\?\(Room\.current\.id\);\s*swim_dir\s*=\s*\{([^}]*)\};\s*while\s+Room\.current\.id\s*!=\s*(\d+);"#
);
// `12662 => 'whirlpool'` pairs inside the table body.
re!(ROUTE_PAIR, r#"(\d+)\s*=>\s*['"]([^'"]+)['"]"#);

// 23x: cast whatever buffs you can, then branch on whether one is active.
// Lich's CAST_BUFF chain; ours is deliberately permissive about whitespace
// and the exact buff list, since only the final branch changes the command.
re!(
    CAST_BUFF_BRANCH,
    r#"^;e\s+(?:if\s+\w+\s*=\s*Spell\[\d+\][^;]*;\s*\w+\.cast;\s*end;\s*)+fput\s*\(\s*Spell\[(\d+)\]\.active\?\s*\?\s*['"]([^'"]+)['"]\s*:\s*['"]([^'"]+)['"]\s*\);?$"#
);

// 497x: the minotaur maze. A learned-graph walker whose whole configuration
// is the goal room and the maze's room set — everything else in the ~1.5KB
// proc is the exploration algorithm, which lives in travel::minotaur.
re!(
    MINOTAUR_MAZE,
    r"^;e\s+target_room_id\s*=\s*(\d+);\s*maze_rooms\s*=\s*\[([^\]]*)\];\s*\$minotaur_maze_dirs\s*\|\|="
);

// 478x, the largest residue family: joining a gaming table. Send `go <table>
// table`; the response is either "you head over to" (seated, done) or an
// invitation, which must be accepted by sending the SAME command again.
re!(
    TABLE_JOIN,
    r#"^;e\s+table\s*=\s*["']((?:[^"'\\]|\\.)*)["']\s*;\s*fput\s+"go \#\{table\} table"\s+if\s+dothistimeout\(.*?\)\s*=~\s*/([^/]+)/\s*$"#
);

// 554x across 4 residue families: a table-driven guided walk. `start_room`
// positions you in the `dirs` cycle; you step it until a landmark object
// appears, then enter it. Not an algorithm — two tables and a noun.
re!(
    GUIDED_ROUTE,
    r#"^;e\s*start_room\s*=\s*\[([^\]]*)\]\s*;\s*dirs\s*=\s*\[([^\]]*)\]\s*;\s*if\s+index\s*=\s*start_room\.index\(Room\.current\.id\);\s*until\s+(checkloot\.include\?.+?);\s*move dirs\[index\];.*?end;\s*(.*?)\s*else;\s*echo"#
);
// Landmark nouns in the `until` clause: `checkloot.include?('door') or
// checkloot.include?('mirror')`.
re!(CHECKLOOT_NOUN, r#"checkloot\.include\?\(['"]([^'"]+)['"]\)"#);
// The enter command(s) after the walk. Either a bare `move 'go door'` or a
// branch picking per landmark: `if checkloot.include?('door'); move 'go
// door'; elsif ...`.
re!(ENTER_MOVE, r#"move\s*\(?['"]([^'"]+)['"]\)?"#);
/// Parse a Ruby array literal of ids, mapping `nil` to a sentinel that can
/// never match a room id. Positions matter — the index into `dirs` is
/// positional — so holes must be preserved, not skipped.
fn parse_id_table(body: &str) -> Vec<u32> {
    body.split(',')
        .map(|t| t.trim().parse::<u32>().unwrap_or(u32::MAX))
        .collect()
}

/// Parse a Ruby array literal of quoted direction strings.
fn parse_dir_table(body: &str) -> Vec<String> {
    body.split(',')
        .filter_map(|t| {
            let t = t.trim();
            t.strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .or_else(|| t.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                .map(str::to_string)
        })
        .collect()
}

/// How long to wait for a gaming table to seat or invite us. The corpus proc
/// uses 25s; a table with other players can take a moment to respond.
const TABLE_JOIN_TIMEOUT_SECS: f32 = 25.0;

/// Timeout for a `dothistimeout`-derived await whose captured value a later
/// command needs. Short: the response to a `look` is immediate or not coming.
const DOTHIS_TIMEOUT_SECS: f32 = 8.0;

/// Rewrite the first unnamed capture group in `pattern` into a named one, so
/// `{capture:<name>}` can reference it. Returns `None` when there is no
/// unnamed group to name (the edge then doesn't transpile rather than
/// producing a command with an unfillable token).
///
/// Named rather than positional deliberately: a positional binding silently
/// shifts whenever the pattern gains a group, which is a live bug class in
/// Lich's own converter.
fn name_first_group(pattern: &str, name: &str) -> Option<String> {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            // Skip an escaped character: `\(` is a literal paren, not a group.
            b'\\' => i += 2,
            b'(' => {
                // `(?:`, `(?=`, `(?<name>` … are not unnamed capture groups.
                if pattern[i + 1..].starts_with('?') {
                    i += 1;
                    continue;
                }
                return Some(format!(
                    "{}(?P<{name}>{}",
                    &pattern[..i],
                    &pattern[i + 1..]
                ));
            }
            _ => i += 1,
        }
    }
    None
}

/// Iterations for a "retry until the room changes" loop. The interpreter
/// clamps this again; keeping it lower here matches the corpus, where these
/// loops succeed within a few tries or not at all.
const MAX_RETRY_LOOP: u32 = 20;

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
    if let Some(c) = SPELL_ACTIVE_MOVE.captures(src) {
        let spell: u16 = c[1].parse().ok()?;
        return Some(vec![WalkAction::If {
            cond: Cond::SpellActive(spell),
            then: vec![WalkAction::Move(c[2].to_string())],
            els: vec![WalkAction::Move(c[3].to_string())],
        }]);
    }
    if let Some(c) = BUFF_THEN_MOVE.captures(src) {
        // The cast is dropped deliberately: we have no cast primitive, and the
        // buff is an optimization rather than a gate — the move is the whole
        // crossing. Worst case the walk is slower or takes a slip, which the
        // executor's move-feedback recovery already handles.
        return Some(vec![WalkAction::Move(c[1].to_string()), WalkAction::WaitRt]);
    }
    if let Some(c) = MULTIFPUT_SPACED.captures(src) {
        return Some(vec![
            WalkAction::Put(c[1].to_string()),
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    if let Some(c) = FPUT_FPUT.captures(src) {
        return Some(vec![
            WalkAction::Put(c[1].to_string()),
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    if FWI_TRINKET.is_match(src) {
        return Some(vec![WalkAction::TrinketWarp]);
    }
    if let Some(c) = TRY_MOVE.captures(src) {
        let mut out = vec![WalkAction::TryMove {
            cmd: c[1].to_string(),
            fallback: vec![
                WalkAction::Put(c[2].to_string()),
                WalkAction::Move(c[3].to_string()),
            ],
        }];
        if c.get(4).is_some() {
            out.push(WalkAction::Replan);
        }
        return Some(out);
    }
    if let Some(c) = STAND_RETRY_MOVE.captures(src) {
        let times: u32 = c[1].parse().ok()?;
        let seconds: f32 = c[2].parse().ok()?;
        let stand_up = WalkAction::Repeat {
            body: vec![
                WalkAction::Put("stand".into()),
                WalkAction::Sleep(seconds),
                WalkAction::WaitRt,
            ],
            until: RepeatUntil::Cond(Cond::Standing),
            max: times,
        };
        return Some(vec![
            WalkAction::WaitRt,
            stand_up.clone(),
            WalkAction::Move(c[3].to_string()),
            WalkAction::Sleep(seconds),
            WalkAction::WaitRt,
            stand_up,
            WalkAction::WaitRt,
        ]);
    }
    if let Some(c) = SWIM_TABLE.captures(src) {
        let dirs: Vec<(u32, String)> = ROUTE_PAIR
            .captures_iter(&c[2])
            .filter_map(|m| Some((m[1].parse().ok()?, m[2].to_string())))
            .collect();
        let target: u32 = c[3].parse().ok()?;
        if !dirs.is_empty() {
            return Some(vec![WalkAction::RouteTable {
                dirs,
                target,
                verb: "swim".into(),
                hands_free_in: parse_id_table(&c[1]),
            }]);
        }
    }
    if let Some(c) = CAST_BUFF_BRANCH.captures(src) {
        // The casts are dropped: we have no cast primitive, and the branch
        // reads the spell's LIVE state, so an uncast buff simply takes the
        // other arm (`swim out` instead of `go out`) — which is the correct
        // command for a character who doesn't have the buff up.
        let spell: u16 = c[1].parse().ok()?;
        return Some(vec![WalkAction::If {
            cond: Cond::SpellActive(spell),
            then: vec![WalkAction::Move(c[2].to_string())],
            els: vec![WalkAction::Move(c[3].to_string())],
        }]);
    }
    if let Some(c) = MINOTAUR_MAZE.captures(src) {
        let target: u32 = c[1].parse().ok()?;
        let maze_rooms = parse_id_table(&c[2]);
        if !maze_rooms.is_empty() {
            return Some(vec![WalkAction::MinotaurMaze { target, maze_rooms }]);
        }
    }
    if let Some(c) = TABLE_JOIN.captures(src) {
        // The table name carries Ruby's escaped quotes ("Cat\'s Paw").
        let table = c[1].replace("\\'", "'").replace("\\\"", "\"");
        let go = format!("go {table} table");
        return Some(vec![WalkAction::Await {
            cmd: Some(go.clone()),
            // Either response ends the wait; which one decides whether we
            // must accept an invitation.
            pattern: Box::new(AwaitPattern::new(
                r"head over to|invites you|inviting you",
            )?),
            timeout: TABLE_JOIN_TIMEOUT_SECS,
            // Missing the line means we never sat down; the crossing failed.
            on_timeout: OnTimeout::Fail,
            if_match: Some((
                Box::new(AwaitPattern::new(&c[2])?),
                // An invitation is accepted by re-sending the same command.
                vec![WalkAction::Move(go)],
            )),
        }]);
    }
    if let Some(c) = GUIDED_ROUTE.captures(src) {
        let start_rooms = parse_id_table(&c[1]);
        let dirs = parse_dir_table(&c[2]);
        // Landmarks in `until`, paired positionally with the enter commands
        // in the tail. One landmark -> one `move 'go X'`; two landmarks -> an
        // if/elsif picking per landmark, in the same order.
        let nouns: Vec<String> = CHECKLOOT_NOUN
            .captures_iter(&c[3])
            .map(|m| m[1].to_string())
            .collect();
        let enters: Vec<String> = ENTER_MOVE
            .captures_iter(&c[4])
            .map(|m| m[1].to_string())
            .collect();
        // Mismatched counts mean we misread the tail; walking with a wrong
        // enter command would strand the character, so leave it untranspiled.
        if !start_rooms.is_empty() && !dirs.is_empty() && !nouns.is_empty() {
            let landmarks: Vec<(String, String)> = match enters.len() {
                // A single enter command serves every landmark.
                1 => nouns
                    .iter()
                    .map(|n| (n.clone(), enters[0].clone()))
                    .collect(),
                n if n == nouns.len() => {
                    nouns.iter().cloned().zip(enters.iter().cloned()).collect()
                }
                _ => Vec::new(),
            };
            if !landmarks.is_empty() {
                return Some(vec![WalkAction::GuidedRoute {
                    start_rooms,
                    dirs,
                    landmarks,
                }]);
            }
        }
    }
    if let Some(c) = MATCH_THEN_MOVE.captures(src) {
        // Ruby's positional $1 becomes a NAMED group so the binding can't
        // shift if the pattern later gains a group. The first unnamed capture
        // group in the source pattern is the one $1 refers to.
        let named = name_first_group(&c[2], "v")?;
        let cmd = c[3].replace("#{$1}", "{capture:v}");
        return Some(vec![
            WalkAction::Await {
                cmd: Some(c[1].to_string()),
                pattern: Box::new(AwaitPattern::new(&named)?),
                timeout: DOTHIS_TIMEOUT_SECS,
                // The move interpolates the captured value, so without a match
                // there is no command to send: this await must fail, not
                // continue.
                on_timeout: OnTimeout::Fail,
                if_match: None,
            },
            WalkAction::Move(cmd),
        ]);
    }
    if let Some(c) = CITIZENSHIP_GATE.captures(src) {
        return Some(vec![WalkAction::If {
            cond: Cond::Citizenship(c[1].to_string()),
            then: vec![WalkAction::Move(c[2].to_string())],
            els: vec![WalkAction::Move(c[3].to_string())],
        }]);
    }
    if let Some(c) = PAUSE_SCRIPT_MOVE.captures(src) {
        let mut out = vec![WalkAction::PauseForUser {
            msg: c[1].to_string(),
            until: None,
            timeout: 0.0,
        }];
        if let Some(m) = c.get(2) {
            out.push(WalkAction::Move(m.as_str().to_string()));
        }
        return Some(out);
    }
    // Await idioms first: they are strictly more specific than the plain
    // fput/move shapes below, which would otherwise match and silently drop
    // the wait (walking on before the ferry arrives).
    if let Some(c) = FPUT_WAITFOR_MOVE.captures(src) {
        return Some(vec![
            WalkAction::Await {
                cmd: Some(c[1].to_string()),
                pattern: Box::new(AwaitPattern::new(&regex::escape(&c[2]))?),
                timeout: WAITFOR_TIMEOUT_SECS,
                on_timeout: OnTimeout::Fail,
                if_match: None,
            },
            WalkAction::Move(c[3].to_string()),
        ]);
    }
    if let Some(c) = WAITFOR_MOVE.captures(src) {
        return Some(vec![
            WalkAction::Await {
                cmd: None,
                pattern: Box::new(AwaitPattern::new(&regex::escape(&c[1]))?),
                timeout: WAITFOR_TIMEOUT_SECS,
                on_timeout: OnTimeout::Fail,
                if_match: None,
            },
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    if let Some(c) = SEARCH_UNTIL_MOVE.captures(src) {
        return Some(vec![
            WalkAction::Repeat {
                body: vec![WalkAction::Put(c[1].to_string()), WalkAction::WaitRt],
                // The Ruby condition tests for a room object we can't see from
                // here; the loop ceiling bounds it and the move that follows
                // is the real test of whether the search worked.
                until: RepeatUntil::Count,
                max: 15,
            },
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    if let Some(c) = FPUT_WHILE_SAME_ROOM.captures(src) {
        return Some(vec![WalkAction::Repeat {
            body: vec![WalkAction::Put(c[1].to_string()), WalkAction::WaitRt],
            until: RepeatUntil::RoomChanged,
            max: MAX_RETRY_LOOP,
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
    // The real form is `h[:towns].include?("Wehnimer's Landing") and
    // h[:towns].include?('Icemule Trace')` — note `[:towns].include`, and each
    // town double- OR single-quoted (single for a town with no apostrophe).
    // Anchor the FIRST side on `towns].include?(` so the body's other
    // `.include?` calls (e.g. `DownstreamHook.list.include?('mapdb_day_pass_
    // monitor')` near the top) can never bind as town A, even if the inlined
    // src arrives newline-joined. Accept either quote per side: Rust regex has
    // no backreferences, so each side is a double|single alternation — keeping
    // the apostrophe inside the double-quoted `"Wehnimer's Landing"`.
    DAY_PASS_PAIR,
    r#"towns\]\.include\?\((?:"(?P<a>[^"]+)"|'(?P<a2>[^']+)')\) and .*?\.include\?\((?:"(?P<b>[^"]+)"|'(?P<b2>[^']+)')\)"#
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
                let a = c.name("a").or_else(|| c.name("a2")).map(|m| m.as_str());
                let b = c.name("b").or_else(|| c.name("b2")).map(|m| m.as_str());
                if let (Some(a), Some(b)) = (a, b) {
                    return day_pass_cost(a, b);
                }
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
    fn small_residue_families_transpile() {
        use WalkAction::*;
        // Modern Spell[] form of the checkspell ternary.
        assert_eq!(
            transpile(";e if Spell[112].active?; move 'north';else; move 'swim north';end"),
            Some(vec![If {
                cond: Cond::SpellActive(112),
                then: vec![Move("north".into())],
                els: vec![Move("swim north".into())],
            }])
        );
        // Buff-if-you-can then move. We have no cast primitive; the buff is an
        // optimization, not a gate, so the move is the whole crossing.
        assert_eq!(
            transpile(
                ";e if resolve = Spell[9704] and resolve.known? and resolve.affordable? \
                 and not resolve.active?; resolve.cast; end; move 'climb wall'; waitrt?"
            ),
            Some(vec![Move("climb wall".into()), WaitRt])
        );
        // Spaced-comma multifput, which the original MULTIFPUT regex misses.
        assert_eq!(
            transpile(";e multifput 'go darkened alleyway', 'go darkened alleyway'"),
            Some(vec![
                Put("go darkened alleyway".into()),
                Move("go darkened alleyway".into())
            ])
        );
        // Two bare fputs, no move.
        assert_eq!(
            transpile(";e fput 'search';fput 'go hatch'"),
            Some(vec![Put("search".into()), Move("go hatch".into())])
        );
    }

    #[test]
    fn swim_table_becomes_a_room_keyed_route_table() {
        let got = transpile(
            ";e empty_hand if [ 12662, 20786 ].include?(Room.current.id); \
             swim_dir = { 20786 => 'down', 12662 => 'whirlpool', 12987 => 'south' }; \
             while Room.current.id != 12677; if swim_dir[Room.current.id]; \
             put \"swim #{swim_dir[Room.current.id]}\"; else; echo \"lost\"; end; \
             sleep 1; waitrt?; end; fill_hand",
        )
        .expect("swim table transpiles");
        match &got[0] {
            WalkAction::RouteTable {
                dirs,
                target,
                verb,
                hands_free_in,
            } => {
                assert_eq!(*target, 12677);
                assert_eq!(verb, "swim", "each direction is prefixed with the verb");
                assert_eq!(hands_free_in, &[12662, 20786]);
                // A room -> direction MAP, not a positional cycle: each room
                // names its own next direction.
                assert_eq!(dirs.len(), 3);
                assert!(dirs.contains(&(12662, "whirlpool".to_string())));
            }
            other => panic!("expected a RouteTable, got {other:?}"),
        }
    }

    #[test]
    fn cast_buff_branch_keeps_the_branch_and_drops_the_casts() {
        // We can't cast, but the branch reads the spell's LIVE state, so an
        // uncast buff simply takes the other arm - which is the correct
        // command for a character who doesn't have it up.
        let got = transpile(
            ";e if resolve = Spell[9704] and resolve.known? and resolve.affordable? \
             and not resolve.active?; resolve.cast; end; if waterwalking = Spell[112] \
             and waterwalking.known? and waterwalking.affordable? and not \
             waterwalking.active?; waterwalking.cast; end; \
             fput (Spell[112].active? ? 'go out' : 'swim out')",
        )
        .expect("cast-buff branch transpiles");
        assert_eq!(
            got,
            vec![WalkAction::If {
                cond: Cond::SpellActive(112),
                then: vec![WalkAction::Move("go out".into())],
                els: vec![WalkAction::Move("swim out".into())],
            }]
        );
    }

    #[test]
    fn try_move_carries_its_fallback_and_optional_replan() {
        use WalkAction::*;
        let base = ";e room = Room.current.id;fput 'go curtain'; \
                    if ( room == Room.current.id ); fput 'close locker';\
                    move 'go curtain'; end";
        let expected = TryMove {
            cmd: "go curtain".into(),
            fallback: vec![Put("close locker".into()), Move("go curtain".into())],
        };
        assert_eq!(transpile(base), Some(vec![expected.clone()]));
        // The trailing $go2_restart splits this into two residue families but
        // is the same shape plus a replan.
        assert_eq!(
            transpile(&format!("{base}; $go2_restart = true")),
            Some(vec![expected, Replan])
        );
    }

    #[test]
    fn stand_retry_move_bounds_its_stand_loop() {
        let got = transpile(
            ";e waitrt?; 8.times { if standing?; break; else; fput 'stand'; \
             sleep 0.2; waitrt?; end }; move 'up'; sleep 0.2; waitrt?; \
             8.times { if standing?; break; else; fput 'stand'; sleep 0.2; \
             waitrt?; end }; waitrt?",
        )
        .expect("stand-retry transpiles");
        assert!(
            matches!(&got[1], WalkAction::Repeat { until, max, .. }
                if *until == RepeatUntil::Cond(Cond::Standing) && *max == 8),
            "the stand loop ends when upright, bounded by the proc's count: {:?}",
            got[1]
        );
        assert_eq!(got[2], WalkAction::Move("up".into()));
    }

    #[test]
    fn minotaur_maze_extracts_only_its_goal_and_room_set() {
        // ~1.5KB of exploration algorithm whose entire per-edge configuration
        // is two values; the algorithm itself lives in travel::minotaur.
        let got = transpile(
            ";e target_room_id = 6192; maze_rooms = [6191, 6254, 6192]; \
             $minotaur_maze_dirs ||= Hash.new; loop { if (bounty? =~ /^You have made \
             contact with the child/); child = GameObj.npcs.find { |npc| npc.noun == \
             'child' }; else; child = nil; end; start_room = Room.current; }",
        )
        .expect("minotaur maze transpiles");
        assert_eq!(
            got,
            vec![WalkAction::MinotaurMaze {
                target: 6192,
                maze_rooms: vec![6191, 6254, 6192],
            }]
        );
    }

    #[test]
    fn table_join_accepts_an_invitation_but_not_a_plain_seating() {
        // The largest residue family (478). The response decides: "head over
        // to" means we're seated, an invitation must be accepted by
        // re-sending the same command.
        let got = transpile(
            r##";e table = "Cat\'s Paw"; fput "go #{table} table" if dothistimeout("go #{table} table", 25, /You (?:and your group )?head over to|waves.*you.*(?:invites|inviting) you(?: and your group)? to (?:join|come sit at)/) =~ /inviting you|invites you/"##,
        )
        .expect("table join transpiles");
        match &got[0] {
            WalkAction::Await {
                cmd,
                pattern,
                if_match,
                ..
            } => {
                assert_eq!(
                    cmd.as_deref(),
                    Some("go Cat's Paw table"),
                    "the escaped quote in the table name is unescaped"
                );
                assert!(pattern.is_match("You head over to the table."));
                let (branch_pat, steps) =
                    if_match.as_ref().expect("an invitation branch exists");
                assert!(
                    branch_pat.is_match("A dwarf waves at you, inviting you to join"),
                    "an invitation triggers the branch"
                );
                assert!(
                    !branch_pat.is_match("You head over to the table."),
                    "a plain seating does NOT re-send"
                );
                assert_eq!(steps, &[WalkAction::Move("go Cat's Paw table".into())]);
            }
            other => panic!("expected an Await, got {other:?}"),
        }
    }

    #[test]
    fn guided_route_extracts_both_tables_and_the_landmark() {
        // 554 edges across 4 residue families - the biggest non-algorithmic
        // cluster in the corpus.
        let got = transpile(
            ";e start_room = [ 12095, 12096, nil, 12097 ]; \
             dirs = [ 'southwest', 'west', 'northwest', 'southeast', 'east' ]; \
             if index = start_room.index(Room.current.id); \
             until checkloot.include?('thread'); move dirs[index]; index += 1; \
             index = 0 if index >= dirs.length; end; move 'climb thread'; waitrt?; \
             fput 'stand'; else; echo 'error: mini-script expected a different room'; \
             end; $go2_restart = true",
        )
        .expect("guided route transpiles");
        match &got[0] {
            WalkAction::GuidedRoute {
                start_rooms,
                dirs,
                landmarks,
            } => {
                // `nil` holds a POSITION - the index into dirs is positional,
                // so dropping it would shift every later room's direction.
                assert_eq!(
                    start_rooms,
                    &[12095, 12096, u32::MAX, 12097],
                    "nil keeps its slot as an unmatchable sentinel"
                );
                assert_eq!(dirs.len(), 5, "dirs is its own cycle, longer than start_room");
                assert_eq!(dirs[0], "southwest");
                assert_eq!(
                    landmarks,
                    &[("thread".to_string(), "climb thread".to_string())]
                );
            }
            other => panic!("expected a GuidedRoute, got {other:?}"),
        }
    }

    #[test]
    fn guided_route_pairs_two_landmarks_with_their_own_enter_commands() {
        // 206 edges walk until EITHER a door or a mirror appears, then enter
        // whichever it was. A single landmark can't express that.
        let got = transpile(
            ";e start_room = [ 2579, 2580 ]; dirs = [ 'southwest', 'east' ]; \
             if index = start_room.index(Room.current.id); \
             until checkloot.include?('door') or checkloot.include?('mirror'); \
             move dirs[index]; index += 1; index = 0 if index >= dirs.length; end; \
             if checkloot.include?('door'); move 'go door'; \
             elsif checkloot.include?('mirror'); move 'go mirror'; end;; \
             else; echo 'error: mini-script expected a different room'; end; \
             $go2_restart = true",
        )
        .expect("two-landmark guided route transpiles");
        match &got[0] {
            WalkAction::GuidedRoute { landmarks, .. } => assert_eq!(
                landmarks,
                &[
                    ("door".to_string(), "go door".to_string()),
                    ("mirror".to_string(), "go mirror".to_string()),
                ],
                "each landmark keeps its own enter command, in order"
            ),
            other => panic!("expected a GuidedRoute, got {other:?}"),
        }
    }

    #[test]
    fn capture_idiom_binds_a_value_and_interpolates_it() {
        // The lever/rune family: the command isn't knowable until the game
        // answers, so the value must be read off the line and substituted.
        let got = transpile(
            r##";e result = dothistimeout 'look wall', 5, /The (\w+) door/; move "go #{$1} door""##,
        )
        .expect("capture idiom transpiles");
        assert_eq!(got.len(), 2, "an await that binds, then the move: {got:?}");
        let bindings = match &got[0] {
            WalkAction::Await { pattern, cmd, .. } => {
                assert_eq!(cmd.as_deref(), Some("look wall"));
                pattern
                    .captures("The bronze door stands here.")
                    .expect("pattern matches and binds")
            }
            other => panic!("expected an Await, got {other:?}"),
        };
        match &got[1] {
            WalkAction::Move(cmd) => {
                assert_eq!(
                    crate::core::pathing::edge::expand_captures(cmd, &bindings).as_deref(),
                    Some("go bronze door"),
                    "the captured word fills the command"
                );
            }
            other => panic!("expected a Move, got {other:?}"),
        }
    }

    #[test]
    fn positional_dollar_one_becomes_a_named_group() {
        // Named, not positional: a positional binding shifts whenever the
        // pattern gains a group, which is a live bug class in Lich's converter.
        assert_eq!(
            name_first_group(r"The (\w+) door", "v").as_deref(),
            Some(r"The (?P<v>\w+) door")
        );
        // Non-capturing and lookahead groups are not $1.
        assert_eq!(
            name_first_group(r"(?:a|b)(\d+)", "v").as_deref(),
            Some(r"(?:a|b)(?P<v>\d+)")
        );
        // An escaped paren is a literal, not a group.
        assert_eq!(name_first_group(r"costs \(5\)", "v"), None);
        // Nothing to bind: the edge must not transpile into an unfillable token.
        assert_eq!(name_first_group(r"no groups here", "v"), None);
    }

    #[test]
    fn citizenship_gate_becomes_a_condition() {
        let got = transpile(
            ";e if Char.citizenship == 'Solhaven'; move 'go gate'; else; move 'go road'; end",
        )
        .expect("transpiles");
        assert_eq!(
            got,
            vec![WalkAction::If {
                cond: Cond::Citizenship("Solhaven".into()),
                then: vec![WalkAction::Move("go gate".into())],
                els: vec![WalkAction::Move("go road".into())],
            }]
        );
    }

    #[test]
    fn ferry_idiom_becomes_an_await_not_a_bare_move() {
        // The motivating case for Await: `fput` boards, `waitfor` blocks until
        // the crew lowers the gangplank, `move` disembarks. Transpiling this
        // as fput+move (which the plain FPUT_MOVE shape would do) walks off
        // the pier before the boat arrives.
        let got = transpile(
            ";e fput 'go gangplank'; waitfor 'lowers the gangplank'; move 'out'",
        )
        .expect("ferry idiom transpiles");
        assert_eq!(got.len(), 2, "an await and the disembark: {got:?}");
        match &got[0] {
            WalkAction::Await {
                cmd,
                pattern,
                on_timeout,
                ..
            } => {
                assert_eq!(cmd.as_deref(), Some("go gangplank"), "boards actively");
                assert!(
                    pattern.is_match("An elven crewmember lowers the gangplank."),
                    "matches the real arrival line"
                );
                assert_eq!(
                    *on_timeout,
                    OnTimeout::Fail,
                    "the awaited line is the only evidence the boat came"
                );
            }
            other => panic!("expected an Await, got {other:?}"),
        }
        assert_eq!(got[1], WalkAction::Move("out".into()));
    }

    #[test]
    fn waitfor_text_is_escaped_not_treated_as_a_pattern() {
        // waitfor takes a literal string. Regex metacharacters in it (periods
        // especially) must match literally, or the pattern silently over-matches.
        let got = transpile(";e waitfor 'the gate. opens'; move 'go gate'")
            .expect("transpiles");
        match &got[0] {
            WalkAction::Await { pattern, .. } => {
                assert!(pattern.is_match("the gate. opens"), "literal text matches");
                assert!(
                    !pattern.is_match("the gateX opens"),
                    "the '.' is escaped, not a wildcard"
                );
            }
            other => panic!("expected an Await, got {other:?}"),
        }
    }

    #[test]
    fn retry_until_room_change_becomes_a_bounded_repeat() {
        // The Red Forest family: keep sending until the room changes.
        let got = transpile(";e fput 'go fog' while Room.current.id == 24675")
            .expect("transpiles");
        assert_eq!(got.len(), 1);
        match &got[0] {
            WalkAction::Repeat { body, until, max } => {
                assert_eq!(*until, RepeatUntil::RoomChanged);
                assert!(*max > 0 && *max <= MAX_RETRY_LOOP, "bounded: {max}");
                assert_eq!(body[0], WalkAction::Put("go fog".into()));
            }
            other => panic!("expected a Repeat, got {other:?}"),
        }
    }

    #[test]
    fn search_until_becomes_a_bounded_repeat_then_the_move() {
        let got = transpile(
            ";e fput 'search' until GameObj.loot.find { |o| o.noun == 'crevice' }; move 'go crevice'",
        )
        .expect("transpiles");
        assert_eq!(got.len(), 2, "the search loop and the entry: {got:?}");
        assert!(
            matches!(&got[0], WalkAction::Repeat { .. }),
            "the search is a loop: {:?}",
            got[0]
        );
        assert_eq!(got[1], WalkAction::Move("go crevice".into()));
    }

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
                 "timeto": {"2": ";e if $x.any? { |id,h| h[:towns].include?(\"Wehnimer's Landing\") and h[:towns].include?('Icemule Trace') }; 0.8; end"},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Dest]"],
                 "wayto": {"1": "raise pass"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let r1 = db.room(1).unwrap();
        let _g = GATE_LOCK.lock().unwrap();
        // The edge names Wehnimer's Landing (double-quoted, apostrophe inside)
        // and Icemule Trace (single-quoted) — the exact mixed-quote live form.
        // Nothing routable → nil.
        set_day_pass_routable(&[]);
        assert_eq!(resolve_timeto(&db, r1, 2), None, "off by default");
        // The exact pair routable at 0.8 (a held pass). Proves the mixed-quote
        // parse (the live bug: single-quoted Icemule wasn't matching).
        set_day_pass_routable(&[(("Wehnimer's Landing".into(), "Icemule Trace".into()), 0.8)]);
        assert_eq!(resolve_timeto(&db, r1, 2), Some(0.8), "routes the held pair");
        // Reverse order still matches (order-independent key).
        set_day_pass_routable(&[(("Icemule Trace".into(), "Wehnimer's Landing".into()), 4.4)]);
        assert_eq!(resolve_timeto(&db, r1, 2), Some(4.4), "reverse pair + buy cost");
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
