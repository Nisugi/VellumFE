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

// 43x: enter/leave an event area, recording (or clearing) which room you came
// from. `N.times{fput "..."}` on the way in, `move(...)` on the way out.
re!(
    ORIGIN_VAR_ENTER,
    r#"^;e\s+(?:(\d+)\.times\s*\{\s*)?(?:fput|move)\s*\(?['"]([^'"]+)['"]\)?\s*\}?\s*;\s*UserVars\.(mapdb_\w+)\s*=\s*(\d+|Map\.current\.id|nil);?$"#
);

// 36x: `$mapdb_seeking_destination = N; Map[3600].wayto['3600'].call` — set
// the goal, then run the Symbol of Seeking loop. The delegate is a ~2KB
// room-name scraper we reimplement as a strategy, so only the goal matters.
re!(
    SEEKING_DELEGATE,
    r"^;e\s+\$mapdb_seeking_destination\s*=\s*(\d+);\s*Map\[\d+\]\.wayto\['\d+'\]\.call;?$"
);

// 6x: save the stance, switch to one the climb needs, move, restore. The
// restore matters — leaving someone in offensive after a climb is a real
// change to their defences.
re!(
    STANCE_PRESERVING_MOVE,
    r#"^;e\s+cur_stance\s*=\s*XMLData\.stance_text;\s*empty_hands;\s*fput\(['"]stance ([a-z]+)['"]\)\s*if\s+cur_stance\s*!=\s*['"][a-z]+['"];\s*move\(['"]([^'"]+)['"]\);\s*fill_hands;.*$"#
);
// 5x: `id=Room.current.id; move "east" until Room.current.id != id` — repeat
// the move until it actually takes.
re!(
    MOVE_UNTIL_ROOM_CHANGE,
    r#"^;e\s+\w+\s*=\s*Room\.current\.id;\s*move\s*\(?["']([^"']+)["']\)?\s+until\s+Room\.current\.id\s*!=\s*\w+;.*$"#
);
// 6x: `fput 'a';fput 'b';move 'c'` — two setup commands then the move.
re!(
    FPUT_FPUT_MOVE,
    r#"^;e\s+fput\s*\(?["']([^"']+)["']\)?;\s*fput\s*\(?["']([^"']+)["']\)?;\s*move\s*\(?["']([^"']+)["']\)?;?$"#
);

// Tier-1 quoting variants: the same shapes we already support, written with
// double quotes. An edge failing over a quote character is a bug in our
// regexes, not a limit of the transpiler.
re!(
    BARE_MOVE_ANY,
    r#"^;e\s+move\s*\(?["']([^"']+)["']\)?\s*(?:;\s*waitrt\?)?;?$"#
);
re!(BARE_FPUT_ANY, r#"^;e\s+fput\s*\(?["']([^"']+)["']\)?;?$"#);
// Leading `waitrt?` before a move (9x), and `fput 'x' waitfor 'y'` with no
// separator (6x).
re!(
    WAITRT_MOVE,
    r#"^;e\s+waitrt\?;\s*move\s*\(?["']([^"']+)["']\)?;?$"#
);
re!(
    FPUT_WAITFOR_ONLY,
    r#"^;e\s+fput\s*\(?["']([^"']+)["']\)?\s+waitfor\s+["']([^"']+)["'];?$"#
);
// `move 'a'; move 'b' if/unless checkpaths.include?('dir')` (12x).
re!(
    MOVE_THEN_COND_MOVE,
    r#"^;e\s+move\s*\(?["']([^"']+)["']\)?;\s*move\s*\(?["']([^"']+)["']\)?\s+(if|unless)\s+checkpaths\.include\?\(['"]([^'"]+)['"]\);?$"#
);
// `move 'x' while checkpaths.include?('dir')` (7x) — keep going while an exit
// is still there.
re!(
    MOVE_WHILE_PATH,
    r#"^;e\s+move\s*\(?["']([^"']+)["']\)?\s+while\s+checkpaths\.include\?\(['"]([^'"]+)['"]\);?$"#
);
// `x=XMLData.room_count; fput "north" until XMLData.room_count > x` (7x) —
// retry a command until it actually moves you.
re!(
    FPUT_UNTIL_MOVED,
    r#"^;e\s+\w+\s*=\s*XMLData\.room_count;\s*fput\s*\(?["']([^"']+)["']\)?\s+until\s+XMLData\.room_count\s*>\s*\w+;?$"#
);
// `unless (move 'go door'); ...; end` (8x) — try it, and on failure run a
// recovery. Same shape as try_move.
re!(
    UNLESS_MOVE,
    r#"^;e\s+unless\s*\(\s*move\s*\(?["']([^"']+)["']\)?\s*\);.*?move\s*\(?["']([^"']+)["']\)?;\s*end;?$"#
);
// Key-in-inventory door (8x): `door='X';key=GameObj.inv.find{...name=='K'};
// if !key.nil? then multifput 'a','b',...; end`.
re!(
    KEYED_DOOR,
    r#"^;e\s+door\s*=\s*['"]([^'"]+)['"];\s*key\s*=\s*GameObj\.inv\.find\{[^}]*name\s*==\s*['"]([^'"]+)['"];?\s*\};\s*if\s+!key\.nil\?\s+then\s+multifput\s+(.+?);\s*end;?$"#
);
re!(QUOTED_ITEM, r#"['"]([^'"]+)['"]"#);

// --- Statement-level vocabulary -------------------------------------------
//
// These match ONE statement of a straight-line body, not a whole edge. The
// full-body recognizers elsewhere in this file each pin a fixed arity, so
// `fput 'search'; move 'go path'; move 'ne'; move 'ne'` matches none of them.
// Splitting first and matching per statement makes arity irrelevant, which is
// what the corpus actually needs: a third of the residue is this shape.

// Control flow the unit parser does NOT model. `if/unless/while/until/else/
// end` are absent — `parse_units` handles those structurally, and modifiers
// stay attached to their statement through the split. `loop`/`break` are
// absent too: they only occur inside braced statements, which the loop
// statement recognizers either accept whole or refuse whole. What remains is
// genuinely beyond the parser: multi-arm elsif, begin/rescue, iterators,
// boolean chains.
re!(
    HARD_CONTROL_FLOW,
    r"(?:^|[^\w.])(?:elsif|begin|rescue|case|when|next|return|exit|pause_script)(?:[^\w?!]|$)|\.each|\|\||&&"
);
// A statement that OPENS a block: the keyword leads. A modifier never does —
// its statement comes first (`fput 'stand' unless standing?`).
re!(BLOCK_OPEN, r"^(if|unless|while|until)\s+(.+)$");
// `XMLData.room_title == '…'` — "still in the starting room".
re!(ROOM_TITLE_COND, r"^XMLData\.room_title\s*==\s*'[^']*'$");
// A move used AS a condition (`unless move 'go door'` = "if it failed").
re!(COND_MOVE, r#"^move\s*\(?\s*['"]([^'"]+)['"]\s*\)?$"#);
// Group scaffolding, stripped textually before parsing. The preamble scrapes
// who followed us in; the wait block holds for their arrival lines. A native
// walker does not escort groups, so solo semantics — neither block runs —
// are the crossing. Both the `group_members` and `$group_members` spellings
// occur.
re!(
    GROUP_PREAMBLE_STRIP,
    r"(?s)^\$?group_members = nil;\s*clear\.reverse\.each \{[^{}]*(?:\{[^{}]*\}[^{}]*)*\};(.*)$"
);
re!(
    GROUP_WAIT_STRIP,
    r"(?s)(?:if \$?group_members\b.*?end while \$?group_members\.length > 0\s*;?\s*end;?|begin\b.*?end while \$?group_members\.length > 0\s*;?\s*end;?)"
);
// Locksmehr mist trail (1042): the trail's direction is read from a look,
// then walked — the capture machinery's home turf.
re!(
    MIST_TRAIL,
    r#"(?s)^move 'climb boulder'\s+fput 'look trail'\s+dir = matchfindword "You peer into the mist and see that the trail heads off to the \?"\s+move 'down'\s+sleep \d+ if running\?\('[^']+'\)\s+move dir$"#
);
// Melgorehn's Reach cable cab, boarding side (18180): if the cab isn't
// here, close the dam and wait out its multi-minute crawl up; then open the
// dam and board.
re!(
    MELGOREHN_CAB,
    r#"(?s)^sleep\(0\.2\);\s*refill_hands=false;\(refill_hands = true;empty_hands;\) if GameObj\.right_hand\.id or GameObj\.left_hand\.id;\s*if !GameObj\.loot\.find\{\|o\| o\.name=='wooden cab'\};\s*dothistimeout "close dam",3,/[^/]+/;\s*_respond [^;]+;\s*waitfor "([^"]+)";\s*end;\s*fput "open dam";\s*sleep\(0\.5\);\s*waitrt\?;\s*fill_hands if refill_hands;\s*move\('go cab'\);?\s*$"#
);
// The dothistimeout spelling of the begin-until search hunt (Vipershroud
// crack, 1230).
re!(
    BEGIN_DOTHIS_UNTIL,
    r"(?s)^begin;\s*(\w+) = dothistimeout '([^']+)', ([\d.]+), /([^/]+)/;\s*waitrt\?;\s*end until (\w+) =~ /([^/]+)/;\s*move\s*\(?'([^']+)'\)?$"
);
// Script bookkeeping with no travel meaning. `$go2_restart = true` and
// `$SILVERWOOD_TOWN=:imt` are Lich-side flags; a comment or bare `nil` is
// nothing. These are dropped, not refused.
re!(
    IGNORABLE_STATEMENT,
    r#"^(?:#.*|nil|true|\$\w+\s*=.*|UserVars\.\w+\s*=.*|echo\s+['"].*|_respond.*)$"#
);
// `dothistimeout 'cmd', 10, /pattern/` as a lone statement: send and await,
// advisory on timeout — matching the full-body DOTHIS treatment, where the
// executor's retry replays the edge rather than failing it.
re!(
    STMT_DOTHIS,
    r#"^dothistimeout\s+['"]([^'"]+)['"]\s*,\s*(\d+(?:\.\d+)?)\s*,\s*/(.*)/$"#
);
// `line = get until line =~ /pattern/` — read game lines until one matches.
// Both variable names are captured and compared in code: Rust regex has no
// backreferences.
re!(
    STMT_GET_UNTIL,
    r"^(\w+)\s*=\s*get\s+until\s+(\w+)\s*=~\s*/(.*)/$"
);
// `r = dothistimeout ...` / `result = nil` — an assignment whose right side
// is a statement (or nothing). Rewritten away by `transpile_sequence` when
// the variable is never read again.
re!(ASSIGNED_RESULT, r"^(\w+)\s*=\s*(dothistimeout\b.+|nil)$");
// `loop { <statements>; if Room.current.id == N; break; end }` — retry until
// a specific room is reached (the Sea Caves swim-tunnel).
re!(
    LOOP_BREAK_ROOM,
    r"(?s)^loop\s*\{(.+?);\s*if Room\.current\.id == (\d+)\s*;\s*break\s*;?\s*end\s*;?\s*\}$"
);
// ---- family recognizers, pinned to verbatim mapdb text ------------------
// Krag slopes crevice hunt (6135): group preamble, search-or-step-n/s loop
// keyed on matchtimeout, group escort tail.
re!(
    KRAG_CREVICE,
    r"(?s)^group_members = nil; clear\.reverse\.each \{ .*? break; end \}; result = nil; while !result; if celerity = Spell\[\d+\].*?end; fput 'search'; result = matchtimeout\([\d.]+,/[^/]+/\); waitrt\?; if !result then multimove '(\w+)','(\w+)'; end; end; fput 'point crevice' if group_members; move '([^']+)'; if group_members;.*end$"
);
// Sleeping Lady ice descents (21 edges): wait-or-buff-or-haste prep around a
// single directional step, with slip recovery.
re!(
    ICE_DESCENT,
    r"(?s)^resolve=Spell\['Sigil of Resolve'\]\s+haste=Spell\['Haste'\]\s+if UserVars\.mapdb_ice_mode == 'wait'.*?result = fput '([^']+)'\s+if result =~ /\^Rushing heedlessly/.*$"
);
// Upper Trollfang footpath (1216): search N times for the discover-line,
// then enter what was found.
re!(
    TIMES_SEARCH_ENTER,
    r"(?s)^(\d+)\.times \{ (\w+) = dothistimeout '([^']+)', ([\d.]+), /([^/]+)/; waitrt\?; break if (\w+) =~ /([^/]+)/ \}; move '([^']+)'$"
);
// Aenatumgana icy ledge (2679): the same hunt, but the break is the move
// itself succeeding.
re!(
    TIMES_SEARCH_BREAKMOVE,
    r"(?s)^(\d+)\.times \{ if \w+ = Spell\[\d+\].*?end; dothistimeout '([^']+)', ([\d.]+), /([^/]+)/; break if move '([^']+)' \}$"
);
// Bring-your-own-key doors (Solhaven / Spindrift): key on you -> door;
// otherwise the script digs through UserVars-named sacks we can't resolve.
re!(
    KEY_DOOR,
    r#"(?s)^if GameObj\.inv\.find \{\|obj\| obj\.noun == "key"\};fput "go ([^"]+)";else;empty_hand;multifput .*end$"#
);
// Zeltoph hidden stone door (19657): open, and on "locked" run the lockpick
// dance before going through.
re!(
    ZELTOPH_DOOR,
    r"^fput 'open door';while line = get;if \['You open the nearly invisible stone door\.', 'That is already open\.'\]\.include\?\(line\);fput 'go door';break;elsif line == 'It appears to be locked\.';empty_hands;fput 'get lockpick';fput 'pick door';fput 'stow lockpick';fill_hands;fput 'open door';fput 'go door';break;end;end$"
);
// `r = dothistimeout 'CMD', T, /…/; if r =~ /good/; move 'M'; elsif …` — the
// success arm of a response branch (Rolaren gate).
re!(
    DOTHIS_BRANCH_MOVE,
    r"(?s)^(\w+) = dothistimeout '([^']+)', ([\d.]+), /([^/]+)/; if (\w+) =~ /([^/]+)/; move '([^']+)'; (?:elsif|else).*$"
);
// Red Forest fog (7892): retry `go fog` until it stops bouncing us back.
re!(
    FOG_RETRY,
    r#"(?s)^(?:UserVars\.\w+ = '[^']*';)?result = nil;until result =~ /[^/]+/;fput "stand" until standing\?;result = dothistimeout "([^"]+)", ([\d.]+), /([^/]+)/;if result =~ /[^/]+/;sleep [\d.]+;waitrt\?;end;end(?:;\s*UserVars\.\w+\s*=\s*\S+?)?(?:;\s*\$go2_restart\s*=\s*true)?;?\s*$"#
);
// `begin; fput 'search'; r = waitfor '…', '…'; waitrt?; end until r =~ /found/;
// move 'go X'` — the begin-until spelling of the search hunt (Vipershroud,
// Spider Temple x3).
re!(
    BEGIN_SEARCH_UNTIL,
    r#"(?s)^begin; fput '([^']+)'; (\w+) = waitfor .+?; waitrt\?; end until (\w+) =~ /([^/]+)/; move '([^']+)'$"#
);
// Spell-prep prefix: `if b = Spell[N] and b.known? …; b.cast; end; <rest>` —
// the buff speeds the crossing up, it does not gate it (BUFF_THEN_MOVE
// stance), so strip it and transpile the rest.
re!(
    SPELL_PREP_PREFIX,
    r"(?s)^if \w+ = Spell\[\d+\][^;]*;\s*\w+\.cast;\s*end;\s*(.+)$"
);
// `loop { move 'a'; break if Room.current.id != N; move 'b'; break if …; }` —
// try the shifting exits in turn until one takes (Lower Dragonsclaw).
re!(
    LOOP_TRY_EXITS,
    r"(?s)^loop \{ (?:move '[^']+'; break if Room\.current\.id != \d+; ?)+\}$"
);
re!(LOOP_TRY_EXITS_STEP, r"move '([^']+)'; break if Room\.current\.id != (\d+)");
// `loop { ...; break unless checkpaths == [ 'out' ] }` — the spike-trap
// chute: exits changing means we fell through.
re!(
    LOOP_BREAK_PATHS,
    r"(?s)^loop \{ (.+); break unless checkpaths == \[ [^\]]*\] \}$"
);
re!(STMT_WAIT_WHILE_STANDING, r"^wait_while \{ standing\? \}$");
// `N.times do; <prefix>; r = dothistimeout "CMD", T, /…/; break if r =~ /…/;end`
// — the do…end spelling, success = the room changes (Coastal Cliffs climb).
re!(
    TIMES_DO_CLIMB,
    r#"(?s)^(\d+)\.times do;(.+?)(\w+) = dothistimeout "([^"]+)", ([\d.]+), /([^/]+)/;\s*break if (\w+) =~ /([^/]+)/\s*;?\s*end$"#
);
// WL sewers trapdoor (22349): search past the junk finds until the trapdoor
// line, then in.
re!(
    WL_TRAPDOOR,
    r#"(?s)^put "search";while line=get;if line=~ /[^/]+/;put "search";elsif line=~/([^/!]+)!?\$?/;put "go (\w+)";break;end;end$"#
);
// `fput 'lie' until checkprone; r = dothistimeout 'search',T, /X/ until r;
// waitrt?; fput 'stand' until standing?; waitrt?; fput 'go X'` (30581).
re!(
    LIE_SEARCH_ENTER,
    r"(?s)^fput 'lie' until checkprone; ?(\w+) = dothistimeout '([^']+)',\s*([\d.]+),\s*/(\w+)/ until (\w+);waitrt\?;fput 'stand' until standing\?;waitrt\?;fput 'go (\w+)'$"
);
// Citadel ledge (11355): offensive stance, climb until it takes, restore.
// We can't read the previous stance, so restore to defensive — the safe
// traveling stance — rather than leave the user parked offensive.
re!(
    STANCE_CLIMB,
    r"(?s)^empty_hands\s+old_stance = XMLData\.stance_text\s+fput 'stance offensive' unless old_stance == 'offensive'\s+until !checkpaths\s+fput 'stand' unless standing\?\s+fput '([^']+)'\s+sleep ([\d.]+)\s+waitrt\?\s+end\s+fput 'stance ' \+ old_stance unless old_stance == 'offensive'$"
);
// Darkstone drawbridge (6997): short races jump, everyone else pulls the
// rope, until the bridge answers. We pull — the jump is a body-size
// alternative for the same lever — and gate the exit on the drawbridge
// becoming a room object. Short characters whose pull draws "what were you
// referring to" run out the retry budget and fail closed.
re!(
    DARKSTONE_BRIDGE,
    r"(?s)^bridgedown = false; until bridgedown; if \[[^\]]+\]\.include\?\(Stats\.race\); fput 'jump'; else; fput 'pull rope'; end; bridgedown = matchtimeout\([\d.]+, /[^/]+/i?\); unless standing\?; fput 'stand'; waitrt\?; end; waitrt\?; end; move '([^']+)';.*$"
);
// `move (XMLData.room_exits - [ 'west' ]).first` — take whichever compass
// exit is NOT the named one (Hidden Plateau's shifting rooms).
re!(
    MOVE_EXIT_EXCEPT,
    r"^move \(XMLData\.room_exits - \[ '(\w+)' \]\)\.first$"
);
// `Spell[N].cast` as its own statement (guarded or not): optional prep,
// dropped — the BUFF_THEN_MOVE stance.
re!(STMT_SPELL_CAST, r"^Spell\[\d+\]\.cast$");
// A skill-check modifier (`fput 'stance offensive' if Skills.climbing < 20`):
// unevaluable natively, but when it guards pure preparation the cautious
// branch — always do the prep — is safe for everyone.
re!(SKILL_COND, r"^Skills\.\w+\s*[<>=]");
// `running?('agoto')` — is a Lich script running? Natively: never.
re!(RUNNING_COND, r"^running\?\('[^']+'\)$");
// `loop { r = dothistimeout 'CMD', T, /…/; waitrt?; break if r =~ /good/ };
// move 'M'` — retry a lever/mechanism until it answers, then through
// (Darkstone's second drawbridge lever).
re!(
    LOOP_DOTHIS_BREAK,
    r"(?s)^loop \{ (\w+) = dothistimeout '([^']+)', ([\d.]+), /([^/]+)/; waitrt\?; break if (\w+) =~ /([^/]+)/ \}; move '([^']+)'$"
);
// Melgorehn's Reach bridge (14726): `go bridge` normally just works; when
// the bridge is pulled open, detour up the platform and turn the wheel.
re!(
    MELGOREHN_BRIDGE,
    r"(?s)^flip_tags = !Script\.current\.want_downstream_xml;\s*status_tags if flip_tags;\s*go_result = dothistimeout 'go bridge',[\d.]+,/[^/]+/;.*climb platform.*turn wheel.*$"
);
// `walk until checkloot.include?('trail'); move 'go trail'` — wander the
// shifting jungle until the feature shows (Karazja).
re!(
    WALK_UNTIL_LOOT,
    r"(?s)^walk until checkloot\.include\?\('(\w+)'\); move '([^']+)'$"
);
// The group-follow preamble + wait: scrape who followed us in, move, then
// wait for each follower's arrival line. A native walker does not escort
// groups, so the solo semantics — move and wait out roundtime — are the
// whole crossing; grouped followers keep up by their own client's follow.
re!(
    GROUP_FOLLOW_MOVE,
    r#"(?s)^group_members = nil; clear\.reverse\.each \{ .*? break; end \}; move '([^']+)'; if group_members; .*? end while group_members\.length > 0; end; waitrt\?$"#
);
// Conditions used by statement modifiers (`fput 'stand' unless standing?` —
// the modifier itself is found by `split_modifier`, a quote-aware scan, not a
// regex: ` if ` can occur inside a quoted argument). Each maps to a `Cond` we
// already have.
re!(COND_STANDING, r"^standing\?$");
re!(COND_KNEELING, r"^kneeling\?$");
re!(COND_SITTING, r"^sitting\?$");
re!(COND_HIDDEN, r"^(?:hidden|invisible)\?$");
re!(COND_CHECKSPELL_NUM, r"^checkspell\s*\(?\s*(\d+)\s*\)?$");
re!(COND_CHECKSPELL_NAME, r#"^checkspell\s*\(?\s*['"]([^'"]+)['"]\s*\)?$"#);
re!(COND_SPELL_ACTIVE, r"^Spell\[(\d+)\]\.active\?$");
re!(COND_HANDS_FULL, r"^GameObj\.(?:right|left)_hand\.id(?:\s+or\s+GameObj\.(?:right|left)_hand\.id)?$");
re!(COND_CHECKLOOT, r#"^checkloot\.include\?\s*\(?\s*['"]([^'"]+)['"]\s*\)?$"#);
re!(COND_IN_ROOM, r"^Room\.current\.id\s*(==|!=)\s*(\d+)$");
re!(COND_ROOM_OBJ, r"^Room\.current\s*(==|!=)\s*Room\[(\d+)\]$");
re!(COND_CHECKPATHS, r#"^checkpaths\.include\?\s*\(\s*['"]([^'"]+)['"]\s*\)$"#);
re!(
    COND_LOOT_FIND,
    r#"^GameObj\.loot\.find\s*\{\s*\|\w+\|\s*\w+\.(?:name|noun)\s*==\s*['"]([^'"]+)['"]\s*\}$"#
);
re!(
    COND_INV_NOUN,
    r#"^GameObj\.inv\.find\s*\{\s*\|\w+\|\s*\w+\.noun\s*==\s*['"]([^'"]+)['"]\s*\}$"#
);
// `2.times{ fput 'ask sailor about boat' }` — a bounded repeat of a known
// statement, with no loop condition to interpret.
re!(
    STMT_TIMES,
    r"^(\d+)\s*\.\s*times\s*\{\s*(.+?)\s*;?\s*\}$"
);
re!(STMT_MOVE, r#"^move\s*\(?\s*(['"][^'"]+['"])\s*\)?$"#);
re!(STMT_FPUT, r#"^(?:fput|put)\s*\(?\s*(['"][^'"]+['"])\s*\)?$"#);
re!(STMT_MULTIFPUT, r#"^multifput\s*\(?\s*(['"].*['"])\s*\)?$"#);
// `waitfor 'a'` and `waitfor 'a','b','c'` alike: Lich's waitfor takes any
// number of alternatives and returns on the first to appear. The single-arg
// form alone missed the Hinterwilds caravans, which gate a whole region.
re!(STMT_WAITFOR, r#"^waitfor\s*\(?\s*(['"].*['"])\s*\)?$"#);
re!(STMT_WAITRT, r"^waitrt\??$");
re!(STMT_SLEEP, r"^(?:sleep|pause)\s*\(?\s*([0-9.]+)\s*\)?$");
re!(STMT_EMPTY_HANDS, r"^empty_hands?(?:\s*\(\s*\))?$");
re!(STMT_FILL_HANDS, r"^fill_hands?(?:\s*\(\s*\))?$");

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
    // A bounty-escort variant inserts a `child = ...` clause between the
    // table and the loop, and parenthesises the condition. Neither changes
    // the walk — we don't model the escorted child (paced StepMove already
    // waits for the room to settle) — so both are skipped.
    r#"^;e\s+empty_hand\s+if\s+\[([^\]]*)\]\.include\?\(Room\.current\.id\);\s*swim_dir\s*=\s*\{([^}]*)\};\s*(?:child\s*=[^;]*;\s*)?while\s+\(?Room\.current\.id\s*!=\s*(\d+)\)?;"#
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
    // Double-quoted names may contain a bare apostrophe ("Cat's Paw"), so
    // the body is anchored on the closing double quote rather than a class
    // that excludes both quote characters.
    r#"^;e\s+table\s*=\s*(?:"([^"]*)"|'((?:[^'\\]|\\.)*)')\s*;\s*fput\s+"go \#\{table\} table"\s+if\s+dothistimeout\(.*?\)\s*=~\s*/([^/]+)/\s*$"#
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
        // Either quoting style; a single-quoted name escapes its apostrophes.
        let table = c
            .get(1)
            .or_else(|| c.get(2))?
            .as_str()
            .replace("\\'", "'")
            .replace("\\\"", "\"");
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
                // Group 3: the two name alternatives above take 1 and 2.
                Box::new(AwaitPattern::new(&c[3])?),
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

    // --- event-area origin markers (43x) ---
    if let Some(c) = ORIGIN_VAR_ENTER.captures(src) {
        let times: u32 = c.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(1);
        let cmd = c[2].to_string();
        let mut out: Vec<WalkAction> = (0..times.min(5).max(1))
            .map(|i| {
                // The last send is the one that moves us.
                if i + 1 == times.min(5).max(1) {
                    WalkAction::Move(cmd.clone())
                } else {
                    WalkAction::Put(cmd.clone())
                }
            })
            .collect();
        // Record where we came from so the return edge's timeto can tell
        // itself apart from the other eleven identical exits.
        out.push(WalkAction::SetVar {
            name: c[3].to_string(),
            value: match &c[4] {
                "nil" => None,
                // `Map.current.id` is resolved at crossing time, not here.
                "Map.current.id" => Some(CURRENT_ROOM_TOKEN.to_string()),
                literal => Some(literal.to_string()),
            },
        });
        return Some(out);
    }

    if let Some(c) = STANCE_PRESERVING_MOVE.captures(src) {
        // We can't read the prior stance to restore it exactly, so restore to
        // defensive — the safe default, and where a traveller wants to be
        // between rooms. Leaving them in the climb's offensive stance would
        // silently change their defences for the rest of the trip.
        return Some(vec![
            WalkAction::EmptyHands,
            WalkAction::Put(format!("stance {}", &c[1])),
            WalkAction::Move(c[2].to_string()),
            WalkAction::FillHands,
            WalkAction::Put("stance defensive".into()),
        ]);
    }
    if let Some(c) = MOVE_UNTIL_ROOM_CHANGE.captures(src) {
        return Some(vec![WalkAction::Repeat {
            body: vec![WalkAction::StepMove(c[1].to_string()), WalkAction::WaitRt],
            until: RepeatUntil::RoomChanged,
            max: MAX_RETRY_LOOP,
        }]);
    }
    if let Some(c) = FPUT_FPUT_MOVE.captures(src) {
        return Some(vec![
            WalkAction::Put(c[1].to_string()),
            WalkAction::Put(c[2].to_string()),
            WalkAction::Move(c[3].to_string()),
        ]);
    }

    // --- tier 1: quoting/whitespace variants of supported shapes ---
    if let Some(c) = WAITRT_MOVE.captures(src) {
        return Some(vec![WalkAction::WaitRt, WalkAction::Move(c[1].to_string())]);
    }
    if let Some(c) = FPUT_WAITFOR_ONLY.captures(src) {
        return Some(vec![WalkAction::Await {
            cmd: Some(c[1].to_string()),
            pattern: Box::new(AwaitPattern::new(&regex::escape(&c[2]))?),
            timeout: WAITFOR_TIMEOUT_SECS,
            on_timeout: OnTimeout::Fail,
            if_match: None,
        }]);
    }
    if let Some(c) = MOVE_THEN_COND_MOVE.captures(src) {
        let cond = Cond::PathAvailable(c[4].to_string());
        let second = vec![WalkAction::Move(c[2].to_string())];
        return Some(vec![
            WalkAction::Move(c[1].to_string()),
            WalkAction::If {
                cond: if &c[3] == "if" {
                    cond
                } else {
                    Cond::Not(Box::new(cond))
                },
                then: second,
                els: Vec::new(),
            },
        ]);
    }
    if let Some(c) = MOVE_WHILE_PATH.captures(src) {
        return Some(vec![WalkAction::Repeat {
            body: vec![WalkAction::StepMove(c[1].to_string())],
            // Stop once the exit we were following is gone.
            until: RepeatUntil::Cond(Cond::Not(Box::new(Cond::PathAvailable(
                c[2].to_string(),
            )))),
            max: MAX_RETRY_LOOP,
        }]);
    }
    if let Some(c) = FPUT_UNTIL_MOVED.captures(src) {
        return Some(vec![WalkAction::Repeat {
            body: vec![WalkAction::StepMove(c[1].to_string()), WalkAction::WaitRt],
            until: RepeatUntil::RoomChanged,
            max: MAX_RETRY_LOOP,
        }]);
    }
    if let Some(c) = UNLESS_MOVE.captures(src) {
        return Some(vec![WalkAction::TryMove {
            cmd: c[1].to_string(),
            fallback: vec![WalkAction::Move(c[2].to_string())],
        }]);
    }
    if let Some(c) = KEYED_DOOR.captures(src) {
        // Only unlock when the key is actually on us; otherwise the multifput
        // sends a pile of commands that all fail.
        let steps: Vec<WalkAction> = QUOTED_ITEM
            .captures_iter(&c[3])
            .map(|m| WalkAction::Put(m[1].to_string()))
            .collect();
        if !steps.is_empty() {
            return Some(vec![WalkAction::If {
                cond: Cond::HasItem(c[2].to_string()),
                then: steps,
                els: vec![WalkAction::Move(format!("go {}", &c[1]))],
            }]);
        }
    }
    if let Some(c) = BARE_MOVE_ANY.captures(src) {
        return Some(vec![WalkAction::Move(c[1].to_string())]);
    }
    if let Some(c) = BARE_FPUT_ANY.captures(src) {
        return Some(vec![WalkAction::Put(c[1].to_string())]);
    }
    // Last resort: a straight-line sequence of statements we each understand.
    // Every recognizer above matches a whole body at a fixed arity, so a body
    // that is merely `fput 'search'; move 'go path'; move 'ne'; move 'ne'` has
    // no rule and fails outright — 34% of the residue is exactly this shape.
    transpile_sequence(src)
}

/// Split a body on top-level `;` / newline, respecting quotes and nesting.
///
/// Naive splitting corrupts `dothistimeout 'x', 3, /a;b/` and every block
/// containing a `;`, so depth and quote state are tracked. Regex literals are
/// NOT tracked (`/` is ambiguous with division without a full lexer); bodies
/// whose regexes contain `;` therefore split wrongly, which is safe here only
/// because the resulting fragments fail to transpile and the whole body is
/// rejected — the same outcome as today.
fn split_statements(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == '\\' {
                    if let Some(esc) = chars.next() {
                        cur.push(esc);
                    }
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '{' | '[' | '(' => {
                    depth += 1;
                    cur.push(c);
                }
                '}' | ']' | ')' => {
                    depth -= 1;
                    cur.push(c);
                }
                ';' | '\n' if depth == 0 => {
                    out.push(std::mem::take(&mut cur));
                }
                _ => cur.push(c),
            },
        }
    }
    out.push(cur);
    out.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Transpile a body as a sequence of independently-understood statements.
///
/// Returns `None` unless EVERY statement is recognized: a partial crossing
/// that silently drops a step would strand the walker somewhere unmapped,
/// which is worse than declining the edge and letting the router re-path.
fn transpile_sequence(src: &str) -> Option<Vec<WalkAction>> {
    let body = src.strip_prefix(";e").unwrap_or(src).trim();
    // Whole-body block shapes that the fragment gate would refuse, recognized
    // first. Each delegates its innards back to the statement machinery
    // where it can, rather than pinning a fixed arity.
    if let Some(actions) = transpile_block_families(body) {
        return Some(actions);
    }
    // Buff-if-you-can prefix: strip it and transpile what remains. The cast
    // is an optimization, not a gate (the BUFF_THEN_MOVE stance). Handled
    // here rather than in the parser because the condition carries an
    // assignment (`if b = Spell[N] and …`).
    if let Some(c) = SPELL_PREP_PREFIX.captures(body) {
        return transpile_sequence(&c[1]);
    }
    // Group scaffolding strips: solo semantics are the crossing (see the
    // regex comments). Textual, before parsing, because the wait block's
    // innards (begin…end while, elsif) are beyond the unit parser and never
    // need to run.
    if let Some(c) = GROUP_PREAMBLE_STRIP.captures(body) {
        return transpile_sequence(c[1].trim());
    }
    if GROUP_WAIT_STRIP.is_match(body) {
        let stripped = GROUP_WAIT_STRIP.replace_all(body, "").to_string();
        return transpile_sequence(stripped.trim());
    }
    transpile_fragment(body)
}

/// One parsed unit of a fragment: a statement, or an if/unless/while/until
/// block with its arms. Blocks nest (bounded); `elsif` and everything the
/// hard gate names stay refused.
#[derive(Debug)]
enum Unit {
    Stmt(String),
    Block {
        kw: String,
        cond: String,
        then_arm: Vec<Unit>,
        else_arm: Vec<Unit>,
    },
}

/// What ended a `parse_units` sequence.
#[derive(Debug, PartialEq)]
enum Term {
    End,
    Else,
    Eof,
}

/// Recursive descent over the statement list: `if <cond>` opens a block,
/// `else` switches arms, `end` closes. Depth-bounded; anything structurally
/// off (stray end, elsif, unclosed block) returns None and the body is
/// refused — malformed control flow must never half-cross an edge.
fn parse_units(stmts: &[String], i: &mut usize, depth: u8) -> Option<(Vec<Unit>, Term)> {
    let mut units = Vec::new();
    while *i < stmts.len() {
        let s = stmts[*i].trim();
        if s == "end" {
            *i += 1;
            return Some((units, Term::End));
        }
        if s == "else" {
            *i += 1;
            return Some((units, Term::Else));
        }
        if s.starts_with("elsif ") || s.starts_with("elsif\t") {
            return None;
        }
        if let Some(c) = BLOCK_OPEN.captures(s) {
            if depth >= 3 {
                return None;
            }
            let (kw, cond) = (c[1].to_string(), c[2].to_string());
            *i += 1;
            let (then_arm, term) = parse_units(stmts, i, depth + 1)?;
            let else_arm = match term {
                Term::Else => {
                    let (arm, term2) = parse_units(stmts, i, depth + 1)?;
                    if term2 != Term::End {
                        return None;
                    }
                    arm
                }
                Term::End => Vec::new(),
                Term::Eof => return None,
            };
            units.push(Unit::Block {
                kw,
                cond,
                then_arm,
                else_arm,
            });
            continue;
        }
        units.push(Unit::Stmt(s.to_string()));
        *i += 1;
    }
    Some((units, Term::Eof))
}

/// Transpile a parsed unit tree. Statements go through
/// `transpile_statement`; blocks become `If`/`Repeat` with conditions from
/// `statement_condition` — unknown conditions refuse the whole body.
fn transpile_units(units: &[Unit]) -> Option<Vec<WalkAction>> {
    let mut actions = Vec::new();
    for unit in units {
        match unit {
            Unit::Stmt(s) => actions.extend(transpile_statement(s)?),
            Unit::Block {
                kw,
                cond,
                then_arm,
                else_arm,
            } => {
                let then = transpile_units(then_arm)?;
                let els = transpile_units(else_arm)?;
                // `while XMLData.room_title == '…'` is "while still here":
                // the room-change IS the loop condition (Citadel ravine).
                if ROOM_TITLE_COND.is_match(cond) {
                    if kw != "while" || !els.is_empty() {
                        return None;
                    }
                    actions.push(WalkAction::Repeat {
                        body: moves_to_steps(then),
                        until: RepeatUntil::RoomChanged,
                        max: MAX_RETRY_LOOP,
                    });
                    continue;
                }
                // `unless move 'go door'; <recovery>; end` — a move as the
                // condition is Ruby for "if it didn't work": TryMove, with
                // the block as the fallback.
                if let Some(m) = COND_MOVE.captures(cond) {
                    if kw != "unless" || !els.is_empty() || then.is_empty() {
                        return None;
                    }
                    actions.push(WalkAction::TryMove {
                        cmd: m[1].to_string(),
                        fallback: then,
                    });
                    continue;
                }
                let parsed = statement_condition(cond)?;
                match kw.as_str() {
                    "if" => actions.push(WalkAction::If {
                        cond: parsed,
                        then,
                        els,
                    }),
                    "unless" => actions.push(WalkAction::If {
                        cond: Cond::Not(Box::new(parsed)),
                        then,
                        els,
                    }),
                    // Loops take no else arm.
                    "while" if els.is_empty() => actions.push(WalkAction::Repeat {
                        body: moves_to_steps(then),
                        until: RepeatUntil::Cond(Cond::Not(Box::new(parsed))),
                        max: MAX_RETRY_LOOP,
                    }),
                    "until" if els.is_empty() => actions.push(WalkAction::Repeat {
                        body: moves_to_steps(then),
                        until: RepeatUntil::Cond(parsed),
                        max: MAX_RETRY_LOOP,
                    }),
                    _ => return None,
                }
            }
        }
    }
    Some(actions)
}

/// A run of `;`-separated statements and simple blocks: gate the hard
/// keywords, split, clean, parse into units, transpile. The unit every
/// recognizer builds on — nested weirdness fails closed through it.
fn transpile_fragment(body: &str) -> Option<Vec<WalkAction>> {
    // Constructs the unit parser doesn't model. `if/unless/while/until/else/
    // end` are handled structurally now; what remains here is genuinely
    // beyond it (multi-arm elsif, begin/rescue, iterators, boolean chains).
    if HARD_CONTROL_FLOW.is_match(body) {
        return None;
    }
    let mut statements = split_statements(body);
    if statements.is_empty() {
        return None;
    }
    // `r = dothistimeout ...` where `r` is never read again: the assignment
    // is noise, the send-and-await is the statement. Only rewritten when the
    // variable is genuinely unused — a later `if r =~ ...` means the body
    // branches on the response, which needs real interpretation.
    for i in 0..statements.len() {
        if let Some(c) = ASSIGNED_RESULT.captures(&statements[i]) {
            let (var, rest) = (c[1].to_string(), c[2].to_string());
            let used_later = statements[i + 1..].iter().any(|s| {
                regex::Regex::new(&format!(r"\b{}\b", regex::escape(&var)))
                    .map(|re| re.is_match(s))
                    .unwrap_or(true)
            });
            if !used_later {
                statements[i] = rest;
            }
        }
    }
    let mut i = 0;
    let (units, term) = parse_units(&statements, &mut i, 0)?;
    if term != Term::Eof {
        // A stray `end`/`else` means the split misread the structure.
        return None;
    }
    let actions = transpile_units(&units)?;
    if actions.is_empty() {
        return None;
    }
    Some(actions)
}

/// Family recognizers for specific block-bearing corpus shapes — each one a
/// sole entrance somewhere. Matched against the exact mapdb text, so a mapdb
/// edit shows up as a coverage regression rather than a wrong walk.
fn transpile_block_families(body: &str) -> Option<Vec<WalkAction>> {
    if let Some(c) = GROUP_FOLLOW_MOVE.captures(body) {
        return Some(vec![WalkAction::Move(c[1].to_string()), WalkAction::WaitRt]);
    }
    // Krag slopes / crevice hunt: search; if the crevice didn't show, step
    // n/s to re-roll the room; repeat; then enter. The discovered crevice is
    // gated on it appearing as a room object (the Caligos precedent — found
    // features join GameObj.loot). Group escort reduces to solo, as in
    // GROUP_FOLLOW_MOVE.
    if let Some(c) = KRAG_CREVICE.captures(body) {
        let found = Cond::RoomHasObject("crevice".into());
        return Some(vec![
            WalkAction::Repeat {
                body: vec![
                    WalkAction::Put("search".into()),
                    WalkAction::WaitRt,
                    WalkAction::If {
                        cond: Cond::Not(Box::new(found.clone())),
                        then: vec![
                            WalkAction::StepMove(c[1].to_string()),
                            WalkAction::StepMove(c[2].to_string()),
                        ],
                        els: Vec::new(),
                    },
                ],
                until: RepeatUntil::Cond(found),
                max: MAX_RETRY_LOOP,
            },
            WalkAction::Move(c[3].to_string()),
        ]);
    }
    // Sleeping Lady ice descents (21 edges): the script offers three speeds —
    // wait out the slip window, or buff with Sigil of Resolve, or blast down
    // hasted. We can't cast, so ALWAYS take the cautious branch: pause, then
    // step. Worst case we descend slower than a buffed Lich user; the slip
    // recovery (stand + replan) is the executor's normal move-failure path.
    if let Some(c) = ICE_DESCENT.captures(body) {
        return Some(vec![
            WalkAction::Sleep(6.0),
            WalkAction::Move(c[1].to_string()),
            WalkAction::WaitRt,
        ]);
    }
    // `N.times { r = dothistimeout 'search', T, /…/; waitrt?; break if r =~
    // /found-line/ }; move 'go X'` — search until the feature is discovered,
    // then enter it (Upper Trollfang footpath).
    if let Some(c) = TIMES_SEARCH_ENTER.captures(body) {
        // Group indices follow the regex: 1=count, 2=var, 3=cmd, 4=timeout,
        // 5=full pattern, 6=var again, 7=success pattern, 8=move. The break
        // must test the SAME variable the dothistimeout assigned.
        if c[2] != c[6] {
            return None;
        }
        let max: u32 = c[1].parse().ok()?;
        let noun = c[8].rsplit(' ').next()?.to_string();
        return Some(vec![
            WalkAction::Repeat {
                body: vec![
                    WalkAction::Await {
                        cmd: Some(c[3].to_string()),
                        pattern: Box::new(AwaitPattern::new(&c[7])?),
                        timeout: c[4].parse().ok()?,
                        on_timeout: OnTimeout::Continue,
                        if_match: None,
                    },
                    WalkAction::WaitRt,
                ],
                until: RepeatUntil::Cond(Cond::RoomHasObject(noun)),
                max: max.clamp(1, MAX_RETRY_LOOP),
            },
            WalkAction::Move(c[8].to_string()),
        ]);
    }
    // The `break if move 'go X'` variant of the same hunt (Aenatumgana icy
    // ledge): spell prep is dropped — the buff speeds the search up, it does
    // not gate it (the BUFF_THEN_MOVE stance).
    if let Some(c) = TIMES_SEARCH_BREAKMOVE.captures(body) {
        let max: u32 = c[1].parse().ok()?;
        let noun = c[5].rsplit(' ').next()?.to_string();
        return Some(vec![
            WalkAction::Repeat {
                body: vec![
                    WalkAction::Await {
                        cmd: Some(c[2].to_string()),
                        pattern: Box::new(AwaitPattern::new(&c[4])?),
                        timeout: c[3].parse().ok()?,
                        on_timeout: OnTimeout::Continue,
                    if_match: None,
                    },
                    WalkAction::WaitRt,
                ],
                until: RepeatUntil::Cond(Cond::RoomHasObject(noun)),
                max: max.clamp(1, MAX_RETRY_LOOP),
            },
            WalkAction::Move(c[5].to_string()),
        ]);
    }
    // Bring-your-own-key doors (Solhaven cellars, Spindrift Sanctuary): with
    // the key anywhere on you, it's just the door; without it the script
    // fetches from UserVars-named sacks we can't resolve, so abandon with
    // the fix named instead of firing doomed commands.
    if let Some(c) = KEY_DOOR.captures(body) {
        return Some(vec![WalkAction::If {
            cond: Cond::HasItem("key".into()),
            then: vec![WalkAction::Move(format!("go {}", &c[1]))],
            els: vec![WalkAction::PauseForUser {
                msg: format!(
                    "the {} needs your key in hand or an open container \
                     (native travel can't fetch it from your keysack) - \
                     get it out and re-run",
                    &c[1]
                ),
                until: None,
                timeout: 0.0,
            }],
        }]);
    }
    // The Zeltoph hidden stone door: open it, and if the response is
    // "locked", run the lockpick dance before going through. The lockpick
    // branch is exactly what `if_match` exists for.
    if ZELTOPH_DOOR.is_match(body) {
        return Some(vec![
            WalkAction::Await {
                cmd: Some("open door".into()),
                pattern: Box::new(AwaitPattern::new(
                    "You open|already open|It appears to be locked",
                )?),
                timeout: 10.0,
                on_timeout: OnTimeout::Fail,
                if_match: Some((
                    Box::new(AwaitPattern::new("It appears to be locked")?),
                    vec![
                        WalkAction::EmptyHands,
                        WalkAction::Put("get lockpick".into()),
                        WalkAction::Put("pick door".into()),
                        WalkAction::Put("stow lockpick".into()),
                        WalkAction::FillHands,
                        WalkAction::Put("open door".into()),
                    ],
                )),
            },
            WalkAction::Move("go door".into()),
        ]);
    }
    // `r = dothistimeout 'CMD', T, /…/; if r =~ /good/; move 'M'; elsif …` —
    // branch on the response, taking only the success arm natively (the
    // Rolaren gate: open → go; locked → a sigil-climb we don't attempt).
    // Timeout or a non-matching response fails the edge: ban, re-path, or
    // hand off — the locked case degrades instead of walking wrong.
    if let Some(c) = DOTHIS_BRANCH_MOVE.captures(body) {
        if c[1] != c[5] {
            return None;
        }
        return Some(vec![
            WalkAction::Await {
                cmd: Some(c[2].to_string()),
                pattern: Box::new(AwaitPattern::new(&c[6])?),
                timeout: c[3].parse().ok()?,
                on_timeout: OnTimeout::Fail,
                if_match: None,
            },
            WalkAction::Move(c[7].to_string()),
        ]);
    }
    // `begin; search; waitfor …; end until found; move` — the begin-until
    // spelling of the search hunt (Vipershroud path, Spider Temple tunnel).
    if let Some(c) = BEGIN_SEARCH_UNTIL.captures(body) {
        if c[2] != c[3] {
            return None;
        }
        let noun = c[5].rsplit(' ').next()?.to_string();
        return Some(vec![
            WalkAction::Repeat {
                body: vec![
                    WalkAction::Await {
                        cmd: Some(c[1].to_string()),
                        pattern: Box::new(AwaitPattern::new(&c[4])?),
                        timeout: 10.0,
                        on_timeout: OnTimeout::Continue,
                        if_match: None,
                    },
                    WalkAction::WaitRt,
                ],
                until: RepeatUntil::Cond(Cond::RoomHasObject(noun)),
                max: MAX_RETRY_LOOP,
            },
            WalkAction::Move(c[5].to_string()),
        ]);
    }
    // The do…end spelling of the retry-climb (Coastal Cliffs): prefix
    // statements each lap, then the attempt; success = the room changes.
    if let Some(c) = TIMES_DO_CLIMB.captures(body) {
        if c[3] != c[7] {
            return None;
        }
        let max: u32 = c[1].parse().ok()?;
        let mut lap = transpile_fragment(&c[2])?;
        lap.push(WalkAction::Await {
            cmd: Some(c[4].to_string()),
            pattern: Box::new(AwaitPattern::new(&c[6])?),
            timeout: c[5].parse().ok()?,
            on_timeout: OnTimeout::Continue,
            if_match: None,
        });
        return Some(vec![WalkAction::Repeat {
            body: lap,
            until: RepeatUntil::RoomChanged,
            max: max.clamp(1, MAX_RETRY_LOOP),
        }]);
    }
    // WL sewers: search past the junk finds until the trapdoor line.
    if let Some(c) = WL_TRAPDOOR.captures(body) {
        return Some(vec![
            WalkAction::Repeat {
                body: vec![
                    WalkAction::Await {
                        cmd: Some("search".into()),
                        pattern: Box::new(AwaitPattern::new(&regex::escape(
                            c[1].trim_end_matches(['!', '$']),
                        ))?),
                        timeout: 10.0,
                        on_timeout: OnTimeout::Continue,
                        if_match: None,
                    },
                    WalkAction::WaitRt,
                ],
                until: RepeatUntil::Cond(Cond::RoomHasObject(c[2].to_string())),
                max: MAX_RETRY_LOOP,
            },
            WalkAction::Move(format!("go {}", &c[2])),
        ]);
    }
    // Lie down, search out the staircase, stand, descend (30581).
    if let Some(c) = LIE_SEARCH_ENTER.captures(body) {
        if c[1] != c[5] {
            return None;
        }
        return Some(vec![
            WalkAction::Put("lie".into()),
            WalkAction::WaitRt,
            WalkAction::Repeat {
                body: vec![
                    WalkAction::Await {
                        cmd: Some(c[2].to_string()),
                        pattern: Box::new(AwaitPattern::new(&c[4])?),
                        timeout: c[3].parse().ok()?,
                        on_timeout: OnTimeout::Continue,
                        if_match: None,
                    },
                    WalkAction::WaitRt,
                ],
                until: RepeatUntil::Cond(Cond::RoomHasObject(c[4].to_string())),
                max: MAX_RETRY_LOOP,
            },
            WalkAction::If {
                cond: Cond::Not(Box::new(Cond::Standing)),
                then: vec![WalkAction::Put("stand".into()), WalkAction::WaitRt],
                els: Vec::new(),
            },
            WalkAction::Move(format!("go {}", &c[6])),
        ]);
    }
    // Citadel ledge: offensive stance, climb until it takes. We can't read
    // the previous stance, so restore to defensive — the safe traveling
    // stance — rather than leave the user parked offensive.
    if let Some(c) = STANCE_CLIMB.captures(body) {
        return Some(vec![
            WalkAction::EmptyHands,
            WalkAction::Put("stance offensive".into()),
            WalkAction::Repeat {
                body: vec![
                    WalkAction::If {
                        cond: Cond::Not(Box::new(Cond::Standing)),
                        then: vec![WalkAction::Put("stand".into())],
                        els: Vec::new(),
                    },
                    WalkAction::StepMove(c[1].to_string()),
                    WalkAction::WaitRt,
                ],
                until: RepeatUntil::RoomChanged,
                max: MAX_RETRY_LOOP,
            },
            WalkAction::Put("stance defensive".into()),
            WalkAction::FillHands,
        ]);
    }
    // Darkstone drawbridge: pull the rope until the bridge answers, then
    // cross. (Short races jump instead — same lever, body-size variant — and
    // a shorty's failed pull runs out the retry budget and fails closed.)
    if let Some(c) = DARKSTONE_BRIDGE.captures(body) {
        let found = Cond::RoomHasObject("drawbridge".into());
        return Some(vec![
            WalkAction::Repeat {
                body: vec![
                    WalkAction::Put("pull rope".into()),
                    WalkAction::Sleep(1.0),
                    WalkAction::WaitRt,
                    WalkAction::If {
                        cond: Cond::Not(Box::new(Cond::Standing)),
                        then: vec![WalkAction::Put("stand".into()), WalkAction::WaitRt],
                        els: Vec::new(),
                    },
                ],
                until: RepeatUntil::Cond(found),
                max: MAX_RETRY_LOOP,
            },
            WalkAction::Move(c[1].to_string()),
        ]);
    }
    // The dothistimeout spelling of the begin-until hunt (Vipershroud crack).
    if let Some(c) = BEGIN_DOTHIS_UNTIL.captures(body) {
        if c[1] != c[5] {
            return None;
        }
        let noun = c[7].rsplit(' ').next()?.to_string();
        return Some(vec![
            WalkAction::Repeat {
                body: vec![
                    WalkAction::Await {
                        cmd: Some(c[2].to_string()),
                        pattern: Box::new(AwaitPattern::new(&c[6])?),
                        timeout: c[3].parse().ok()?,
                        on_timeout: OnTimeout::Continue,
                        if_match: None,
                    },
                    WalkAction::WaitRt,
                ],
                until: RepeatUntil::Cond(Cond::RoomHasObject(noun)),
                max: MAX_RETRY_LOOP,
            },
            WalkAction::Move(c[7].to_string()),
        ]);
    }
    // Darkstone's inner lever: pull until it gives, then through the
    // portcullis. Await's Retry re-sends once; a lever that won't budge
    // twice (a strength check) fails the edge closed.
    if let Some(c) = LOOP_DOTHIS_BREAK.captures(body) {
        if c[1] != c[5] {
            return None;
        }
        return Some(vec![
            WalkAction::Await {
                cmd: Some(c[2].to_string()),
                pattern: Box::new(AwaitPattern::new(&c[6])?),
                timeout: c[3].parse().ok()?,
                on_timeout: OnTimeout::Retry,
                if_match: None,
            },
            WalkAction::WaitRt,
            WalkAction::Move(c[7].to_string()),
        ]);
    }
    // Melgorehn's Reach bridge: normally one command; the pulled-open detour
    // climbs the platform and turns the wheel, then crosses from above.
    if MELGOREHN_BRIDGE.is_match(body) {
        return Some(vec![WalkAction::TryMove {
            cmd: "go bridge".into(),
            fallback: vec![
                WalkAction::EmptyHands,
                WalkAction::StepMove("go opening".into()),
                WalkAction::Repeat {
                    body: vec![
                        WalkAction::Await {
                            cmd: Some("climb platform".into()),
                            pattern: Box::new(AwaitPattern::new(
                                "you were able to pull yourself up\
                                 |I could not find what you were referring to",
                            )?),
                            timeout: 2.0,
                            on_timeout: OnTimeout::Continue,
                            if_match: None,
                        },
                        WalkAction::WaitRt,
                    ],
                    until: RepeatUntil::RoomChanged,
                    max: MAX_RETRY_LOOP,
                },
                WalkAction::Await {
                    cmd: Some("turn wheel".into()),
                    pattern: Box::new(AwaitPattern::new(
                        "The wheel begins to move|already been turned",
                    )?),
                    timeout: 3.0,
                    on_timeout: OnTimeout::Retry,
                    if_match: None,
                },
                WalkAction::StepMove("down".into()),
                WalkAction::StepMove("out".into()),
                WalkAction::Move("go bridge".into()),
                WalkAction::FillHands,
            ],
        }]);
    }
    // Locksmehr mist trail: climb the boulder, read which way the trail
    // heads, climb down, walk that way. The direction is a named capture
    // interpolated into the final move.
    if MIST_TRAIL.is_match(body) {
        return Some(vec![
            WalkAction::StepMove("climb boulder".into()),
            WalkAction::Await {
                cmd: Some("look trail".into()),
                pattern: Box::new(AwaitPattern::new(
                    r"You peer into the mist and see that the trail heads off to the (?P<dir>\w+)",
                )?),
                timeout: 10.0,
                on_timeout: OnTimeout::Retry,
                if_match: None,
            },
            WalkAction::StepMove("down".into()),
            WalkAction::Move("{capture:dir}".into()),
        ]);
    }
    // Melgorehn's cable cab, boarding side: summon the cab if it isn't
    // here (close dam, wait out the ~4.5 minute crawl), then open and board.
    if let Some(c) = MELGOREHN_CAB.captures(body) {
        return Some(vec![
            WalkAction::EmptyHands,
            WalkAction::If {
                cond: Cond::Not(Box::new(Cond::RoomHasObject("wooden cab".into()))),
                then: vec![
                    WalkAction::Await {
                        cmd: Some("close dam".into()),
                        pattern: Box::new(AwaitPattern::new(
                            "slides closed|already closed",
                        )?),
                        timeout: 5.0,
                        on_timeout: OnTimeout::Retry,
                        if_match: None,
                    },
                    WalkAction::Await {
                        cmd: None,
                        pattern: Box::new(AwaitPattern::new(&regex::escape(&c[1]))?),
                        timeout: WAITFOR_TIMEOUT_SECS,
                        on_timeout: OnTimeout::Fail,
                        if_match: None,
                    },
                ],
                els: Vec::new(),
            },
            WalkAction::Put("open dam".into()),
            WalkAction::Sleep(0.5),
            WalkAction::WaitRt,
            WalkAction::FillHands,
            WalkAction::Move("go cab".into()),
        ]);
    }
    // Karazja: wander the shifting jungle until the feature shows.
    if let Some(c) = WALK_UNTIL_LOOT.captures(body) {
        return Some(vec![
            WalkAction::Repeat {
                body: vec![WalkAction::MoveAnyExit, WalkAction::WaitRt],
                until: RepeatUntil::Cond(Cond::RoomHasObject(c[1].to_string())),
                max: MAX_RETRY_LOOP,
            },
            WalkAction::Move(c[2].to_string()),
        ]);
    }
    // Red Forest fog: keep trying `go fog` until it stops bouncing us back
    // (success = the room actually changes; the paths-line the script keys
    // on is just its way of noticing that).
    if let Some(c) = FOG_RETRY.captures(body) {
        return Some(vec![WalkAction::Repeat {
            body: vec![
                WalkAction::If {
                    cond: Cond::Not(Box::new(Cond::Standing)),
                    then: vec![WalkAction::Put("stand".into())],
                    els: Vec::new(),
                },
                WalkAction::Await {
                    cmd: Some(c[1].to_string()),
                    pattern: Box::new(AwaitPattern::new(&c[3])?),
                    timeout: c[2].parse().ok()?,
                    on_timeout: OnTimeout::Continue,
                    if_match: None,
                },
                WalkAction::WaitRt,
            ],
            until: RepeatUntil::RoomChanged,
            max: MAX_RETRY_LOOP,
        }]);
    }
    None
}

/// One statement of a straight-line body.
fn transpile_statement(statement: &str) -> Option<Vec<WalkAction>> {
    let s = statement.trim().trim_end_matches(';').trim();
    if s.is_empty() || IGNORABLE_STATEMENT.is_match(s) {
        // Script bookkeeping with no travel meaning: `$go2_restart = true`,
        // comments, bare `nil`. Dropping these is what lets an otherwise
        // ordinary body stop being residue.
        return Some(Vec::new());
    }
    if STMT_SPELL_CAST.is_match(s) {
        // A cast is optional preparation, never the crossing itself — the
        // BUFF_THEN_MOVE stance, statement-sized.
        return Some(Vec::new());
    }
    // Braced `loop { … }` forms — a single statement to the splitter, so
    // they compose anywhere in a body (the Darkstone spike-trap buries one
    // mid-sequence).
    if s.starts_with("loop") {
        // `loop { <stmts>; if Room.current.id == N; break; end }` — retry
        // until we land in the room (Sea Caves swim).
        if let Some(c) = LOOP_BREAK_ROOM.captures(s) {
            let inner = transpile_fragment(&c[1])?;
            if inner.is_empty() {
                return None;
            }
            return Some(vec![WalkAction::Repeat {
                body: moves_to_steps(inner),
                until: RepeatUntil::Room(c[2].parse().ok()?),
                max: MAX_RETRY_LOOP,
            }]);
        }
        // `loop { <stmts>; break unless checkpaths == [ 'out' ] }` — work
        // the trap room until its exits change, i.e. we fell through
        // (Darkstone spike chute).
        if let Some(c) = LOOP_BREAK_PATHS.captures(s) {
            let inner = transpile_fragment(&c[1])?;
            if inner.is_empty() {
                return None;
            }
            return Some(vec![WalkAction::Repeat {
                body: moves_to_steps(inner),
                until: RepeatUntil::RoomChanged,
                max: MAX_RETRY_LOOP,
            }]);
        }
        // Shifting exits tried in turn until one takes (Lower Dragonsclaw).
        if LOOP_TRY_EXITS.is_match(s) {
            let mut steps = Vec::new();
            let mut room = None;
            for c in LOOP_TRY_EXITS_STEP.captures_iter(s) {
                let id: u32 = c[2].parse().ok()?;
                room = Some(id);
                // Guard each alternative on still being in the start room,
                // so a successful early step doesn't walk us further off.
                steps.push(WalkAction::If {
                    cond: Cond::InRoom(id),
                    then: vec![WalkAction::StepMove(c[1].to_string())],
                    els: Vec::new(),
                });
            }
            let room = room?;
            return Some(vec![WalkAction::Repeat {
                body: steps,
                until: RepeatUntil::Cond(Cond::Not(Box::new(Cond::InRoom(room)))),
                max: MAX_RETRY_LOOP,
            }]);
        }
        return None;
    }
    if STMT_WAIT_WHILE_STANDING.is_match(s) {
        // `wait_while { standing? }` — riding a fall/knockdown out.
        return Some(vec![WalkAction::Repeat {
            body: vec![WalkAction::Sleep(0.5)],
            until: RepeatUntil::Cond(Cond::Not(Box::new(Cond::Standing))),
            max: MAX_RETRY_LOOP,
        }]);
    }
    // Peel a trailing modifier before the bare forms match, so the condition
    // survives instead of the statement running unconditionally.
    if let Some((left, keyword, cond_src)) = split_modifier(s) {
        // Only when the guarded part is itself a statement we understand;
        // otherwise fall through and let the whole body be refused.
        if let Some(inner) = transpile_statement(left) {
            // A dropped statement stays dropped under any guard —
            // `Spell[N].cast if Spell[N].known? and …` needs no condition
            // because either answer produces nothing.
            if inner.is_empty() {
                return Some(Vec::new());
            }
            // `... if group_members` runs only when grouped. A native walker
            // does not escort groups, so the solo behavior — skip it — is the
            // crossing; `unless group_members` inverts to "always". The same
            // logic covers `running?('script')`: no Lich script runs here,
            // so the condition is simply false.
            let cond_trim = cond_src.trim();
            if cond_trim == "group_members"
                || cond_trim == "$group_members"
                || RUNNING_COND.is_match(cond_trim)
            {
                return Some(match keyword {
                    "if" => Vec::new(),
                    _ => inner,
                });
            }
            let Some(cond) = statement_condition(cond_src) else {
                // A skill check guarding pure preparation (`fput 'stance
                // offensive' if Skills.climbing < 20`): unevaluable, but the
                // cautious branch — always do the prep — is safe for the
                // skilled and required for the rest. Only for command-only
                // statements; a guarded MOVE must never run unconditionally.
                if SKILL_COND.is_match(cond_src.trim())
                    && keyword == "if"
                    && inner.iter().all(|a| matches!(a, WalkAction::Put(_)))
                {
                    return Some(inner);
                }
                return None;
            };
            return Some(match keyword {
                "if" => vec![WalkAction::If {
                    cond,
                    then: inner,
                    els: Vec::new(),
                }],
                "unless" => vec![WalkAction::If {
                    cond: Cond::Not(Box::new(cond)),
                    then: inner,
                    els: Vec::new(),
                }],
                // `move 'north' while Room.current.id == 2925`: repeat while
                // the condition holds — i.e. until it stops holding.
                "while" => vec![WalkAction::Repeat {
                    body: moves_to_steps(inner),
                    until: RepeatUntil::Cond(Cond::Not(Box::new(cond))),
                    max: MAX_RETRY_LOOP,
                }],
                // `fput 'stand' until standing?`: repeat until it holds.
                _ => vec![WalkAction::Repeat {
                    body: moves_to_steps(inner),
                    until: RepeatUntil::Cond(cond),
                    max: MAX_RETRY_LOOP,
                }],
            });
        }
    }
    if let Some(c) = STMT_TIMES.captures(s) {
        // A bounded repeat with no loop condition: just unroll it. `max` in a
        // `Repeat` would need an until-condition we do not have, and the counts
        // in the corpus are small (2-3).
        let count: usize = c[1].parse().ok()?;
        if count == 0 || count > 10 {
            return None;
        }
        let once = transpile_statement(&c[2])?;
        let mut unrolled = Vec::with_capacity(once.len() * count);
        for _ in 0..count {
            unrolled.extend(once.iter().cloned());
        }
        return Some(unrolled);
    }
    if let Some(c) = STMT_MOVE.captures(s) {
        return Some(vec![WalkAction::Move(unquote(&c[1]))]);
    }
    if let Some(c) = MOVE_EXIT_EXCEPT.captures(s) {
        // `move (XMLData.room_exits - ['west']).first` — resolved from the
        // live compass at walk time.
        return Some(vec![WalkAction::MoveExitExcept(c[1].to_string())]);
    }
    if let Some(c) = STMT_FPUT.captures(s) {
        return Some(vec![WalkAction::Put(unquote(&c[1]))]);
    }
    if let Some(c) = STMT_MULTIFPUT.captures(s) {
        // Variadic by construction. The two fixed-arity MULTIFPUT recognizers
        // above miss every three-or-more-argument call.
        let args: Vec<WalkAction> = QUOTED_ITEM
            .captures_iter(&c[1])
            .map(|m| WalkAction::Put(m[1].to_string()))
            .collect();
        return if args.is_empty() { None } else { Some(args) };
    }
    if let Some(c) = STMT_WAITFOR.captures(s) {
        // Passive await: the command that provokes the line is a separate
        // statement that already ran. `Fail` on timeout matches the other
        // waitfor recognizers — a missed arrival line means we are not where
        // the script thinks we are.
        //
        // Multiple alternatives become one alternation, matching Lich's
        // "return on whichever appears first".
        let alternatives: Vec<String> = QUOTED_ITEM
            .captures_iter(&c[1])
            .map(|m| regex::escape(&m[1]))
            .collect();
        if alternatives.is_empty() {
            return None;
        }
        return Some(vec![WalkAction::Await {
            cmd: None,
            pattern: Box::new(AwaitPattern::new(&alternatives.join("|"))?),
            timeout: WAITFOR_TIMEOUT_SECS,
            on_timeout: OnTimeout::Fail,
            if_match: None,
        }]);
    }
    if STMT_WAITRT.is_match(s) {
        return Some(vec![WalkAction::WaitRt]);
    }
    if let Some(c) = STMT_DOTHIS.captures(s) {
        // Advisory await, matching the full-body DOTHIS treatment: the
        // executor's retry-on-timeout replays the edge, which is the
        // dothistimeout loop with a longer period.
        return Some(vec![WalkAction::Await {
            cmd: Some(c[1].to_string()),
            pattern: Box::new(AwaitPattern::new(&c[3])?),
            timeout: c[2].parse().ok()?,
            on_timeout: OnTimeout::Continue,
            if_match: None,
        }]);
    }
    if let Some(c) = STMT_GET_UNTIL.captures(s) {
        // Self-referential read loop: `line = get until line =~ /pat/`. Only
        // when both names agree is this the idiom (the regex cannot
        // backreference).
        if c[1] != c[2] {
            return None;
        }
        return Some(vec![WalkAction::Await {
            cmd: None,
            pattern: Box::new(AwaitPattern::new(&c[3])?),
            timeout: WAITFOR_TIMEOUT_SECS,
            on_timeout: OnTimeout::Fail,
            if_match: None,
        }]);
    }
    if let Some(c) = STMT_SLEEP.captures(s) {
        return Some(vec![WalkAction::Sleep(c[1].parse().ok()?)]);
    }
    if STMT_EMPTY_HANDS.is_match(s) {
        return Some(vec![WalkAction::EmptyHands]);
    }
    if STMT_FILL_HANDS.is_match(s) {
        return Some(vec![WalkAction::FillHands]);
    }
    None
}

/// Map a statement-modifier condition to a `Cond`.
///
/// Returns `None` for anything unrecognized, which refuses the whole body —
/// guessing a condition wrong would cross an edge the script gated, or skip a
/// step it required.
fn statement_condition(raw: &str) -> Option<Cond> {
    let c = raw.trim().trim_end_matches(';').trim();
    // `!cond` — plain Ruby negation.
    if let Some(rest) = c.strip_prefix('!') {
        return statement_condition(rest).map(|inner| Cond::Not(Box::new(inner)));
    }
    if COND_STANDING.is_match(c) {
        return Some(Cond::Standing);
    }
    if COND_KNEELING.is_match(c) {
        return Some(Cond::Kneeling);
    }
    if COND_SITTING.is_match(c) {
        return Some(Cond::Sitting);
    }
    if COND_HIDDEN.is_match(c) {
        return Some(Cond::Hidden);
    }
    if let Some(m) = COND_CHECKSPELL_NUM.captures(c) {
        return Some(Cond::SpellActive(m[1].parse().ok()?));
    }
    if let Some(m) = COND_SPELL_ACTIVE.captures(c) {
        return Some(Cond::SpellActive(m[1].parse().ok()?));
    }
    if let Some(m) = COND_CHECKSPELL_NAME.captures(c) {
        // Named spells: only the ones that gate a crossing in the corpus.
        // Invisibility is the whole set today ("unhide if invisible").
        return match m[1].to_ascii_lowercase().as_str() {
            "invisibility" => Some(Cond::Hidden),
            _ => None,
        };
    }
    // `empty_hands if GameObj.right_hand.id` guards against stowing when the
    // hands are already empty — which `EmptyHands` already handles as a no-op.
    // Rather than add a Cond variant the executor would have to evaluate, the
    // guard is dropped and the statement runs unconditionally. Sound ONLY
    // because the guarded action is idempotent; do not extend this to others.
    if COND_HANDS_FULL.is_match(c) {
        // `Any([])` is false, so the always-true form is its negation.
        return Some(Cond::Not(Box::new(Cond::Any(Vec::new()))));
    }
    if let Some(m) = COND_CHECKLOOT.captures(c) {
        return Some(Cond::RoomHasObject(m[1].to_string()));
    }
    if let Some(m) = COND_IN_ROOM.captures(c) {
        let cond = Cond::InRoom(m[2].parse().ok()?);
        return Some(if &m[1] == "!=" {
            Cond::Not(Box::new(cond))
        } else {
            cond
        });
    }
    if let Some(m) = COND_ROOM_OBJ.captures(c) {
        // `Room.current == Room[N]` — the object-comparison spelling of the
        // same test.
        let cond = Cond::InRoom(m[2].parse().ok()?);
        return Some(if &m[1] == "!=" {
            Cond::Not(Box::new(cond))
        } else {
            cond
        });
    }
    if let Some(m) = COND_CHECKPATHS.captures(c) {
        return Some(Cond::PathAvailable(m[1].to_string()));
    }
    if let Some(m) = COND_LOOT_FIND.captures(c) {
        return Some(Cond::RoomHasObject(m[1].to_string()));
    }
    if let Some(m) = COND_INV_NOUN.captures(c) {
        return Some(Cond::HasItem(m[1].to_string()));
    }
    None
}

/// Find the last top-level modifier keyword in a statement, splitting it into
/// (statement, keyword, condition). Quote- and depth-aware for the same
/// reason `split_statements` is: ` if ` can occur inside a quoted argument
/// (`fput 'say meet me if you can'`), and a regex would split there. The LAST
/// occurrence wins because Ruby modifiers bind loosest — everything to their
/// left is the guarded statement.
fn split_modifier(s: &str) -> Option<(&str, &str, &str)> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut found: Option<(usize, usize, &'static str)> = None;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        let c = bytes[i] as char;
        match quote {
            Some(q) => {
                if c == '\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => quote = Some(c),
                '{' | '[' | '(' => depth += 1,
                '}' | ']' | ')' => depth -= 1,
                ' ' if depth == 0 => {
                    for kw in ["if", "unless", "while", "until"] {
                        let end = i + 1 + kw.len();
                        if s[i + 1..].starts_with(kw)
                            && s.as_bytes().get(end).copied() == Some(b' ')
                            && i > 0
                        {
                            found = Some((i, end + 1, kw));
                        }
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    let (split_at, cond_from, kw) = found?;
    let left = s[..split_at].trim();
    let cond = s[cond_from..].trim();
    if left.is_empty() || cond.is_empty() {
        return None;
    }
    Some((left, kw, cond))
}

/// Convert terminal `Move`s to paced `StepMove`s for use inside a `Repeat`
/// body: a `Move` is the script's final room-changer, but a repeated move is
/// a step whose landing must settle before the loop re-evaluates.
fn moves_to_steps(actions: Vec<WalkAction>) -> Vec<WalkAction> {
    actions
        .into_iter()
        .map(|a| match a {
            WalkAction::Move(cmd) => WalkAction::StepMove(cmd),
            other => other,
        })
        .collect()
}

/// Strip one layer of matching quotes from a captured argument.
fn unquote(raw: &str) -> String {
    let t = raw.trim();
    let t = t.strip_prefix('(').unwrap_or(t).trim();
    let t = t.strip_suffix(')').unwrap_or(t).trim();
    for q in ['\'', '"'] {
        if let Some(inner) = t.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner.to_string();
        }
    }
    t.to_string()
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
    // Before the generic delegation: this one's target is a ~2KB room-name
    // scraper we reimplement as a strategy, so following it would just fail.
    if let Some(c) = SEEKING_DELEGATE.captures(src) {
        return Some(vec![WalkAction::VolnSeeking {
            destination: c[1].parse().ok()?,
        }]);
    }
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

// `;e (!UserVars.mapdb_duskruin_origin.nil? and UserVars.mapdb_duskruin_origin
// == 7) ? 0.2 : nil;` — the return edge is only routable when the variable
// says THIS is where you came in.
re!(
    TIMETO_ORIGIN_VAR,
    // No backreference (Rust's regex crate has none): both var names are
    // captured and compared in code.
    r"^;e\s+\(!UserVars\.(mapdb_\w+)\.nil\?\s+and\s+UserVars\.(mapdb_\w+)\s*==\s*(\d+)\)\s*\?\s*([\d.]+)\s*:\s*nil;?$"
);

/// Placeholder for `Map.current.id` in a [`WalkAction::SetVar`] value: the
/// room isn't known at transpile time, so the executor substitutes the live
/// one when the action runs.
pub const CURRENT_ROOM_TOKEN: &str = "{current_room}";

/// Lich's `UserVars.mapdb_*` scratch variables, as the router sees them.
///
/// These are NOT decoration: several event areas (Duskruin, Ebon Gate,
/// Talondown) have ONE physical exit shared by a dozen return edges, and the
/// only thing distinguishing them is a variable the entry edge set. Their
/// `timeto` reads it — `(origin == 7) ? 0.2 : nil` — so without the variable
/// every return edge is either unroutable or, worse, all of them look equally
/// good and the router picks one at random. You come out somewhere you didn't
/// ask for.
///
/// A process global rather than task state because `resolve_timeto` is a free
/// function the dijkstra calls with no context, matching `USE_SEEKING` above.
/// Values are per-session and cheap; a stale one only mis-costs an edge that
/// re-planning corrects.
static MAPDB_VARS: std::sync::LazyLock<
    std::sync::RwLock<std::collections::HashMap<String, String>>,
> = std::sync::LazyLock::new(Default::default);

/// Read a `UserVars.mapdb_<name>` value, `None` when never set (Ruby `nil`).
pub fn mapdb_var(name: &str) -> Option<String> {
    MAPDB_VARS.read().ok()?.get(name).cloned()
}

/// Set a `UserVars.mapdb_<name>`; `None` clears it (Ruby `= nil`).
pub fn set_mapdb_var(name: &str, value: Option<String>) {
    if let Ok(mut vars) = MAPDB_VARS.write() {
        match value {
            Some(v) => {
                vars.insert(name.to_string(), v);
            }
            None => {
                vars.remove(name);
            }
        }
    }
}

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
            if let Some(c) = TIMETO_ORIGIN_VAR.captures(src) {
                // Routable only from the room the entry edge recorded — this
                // is what keeps a dozen identical `go wagon` exits distinct.
                // Both halves must name the same variable (the backreference
                // the regex crate can't express).
                if c[1] == c[2] {
                    return (mapdb_var(&c[1]).as_deref() == Some(&c[3]))
                        .then(|| c[4].parse().ok())
                        .flatten();
                }
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
    fn tier3_families_transpile() {
        use WalkAction::*;
        // Voln seeking: the ~2KB room-name-scraping delegate is a strategy,
        // so only the goal is extracted. Handled in transpile_edge (the
        // delegation-aware entry), ahead of the generic delegate follow.
        let db = MapDb::from_json("[]").unwrap();
        assert_eq!(
            transpile_edge(
                &db,
                ";e $mapdb_seeking_destination = 2635;Map[3600].wayto['3600'].call;"
            ),
            Some(vec![VolnSeeking { destination: 2635 }])
        );
        // Retry a move until it takes.
        assert!(matches!(
            transpile(
                ";e id=Room.current.id;move \"east\" until Room.current.id != id;\
                 $go2_restart=true"
            )
            .as_deref(),
            Some([Repeat { until: RepeatUntil::RoomChanged, .. }])
        ));
        assert_eq!(
            transpile(";e fput 'pull lever';fput 'open gate';move 'go gate'"),
            Some(vec![
                Put("pull lever".into()),
                Put("open gate".into()),
                Move("go gate".into()),
            ])
        );
    }

    #[test]
    fn stance_preserving_move_restores_a_safe_stance() {
        // Leaving someone in the climb's offensive stance would silently
        // change their defences for the rest of the trip.
        let got = transpile(
            ";e cur_stance = XMLData.stance_text;empty_hands;\
             fput('stance offensive') if cur_stance != 'offensive';\
             move('climb rockslide');fill_hands;\
             fput('stance ' + cur_stance) if cur_stance != 'offensive';$go2_restart = true",
        )
        .expect("stance-preserving move transpiles");
        assert_eq!(got[1], WalkAction::Put("stance offensive".into()));
        assert_eq!(got[2], WalkAction::Move("climb rockslide".into()));
        assert_eq!(
            got.last(),
            Some(&WalkAction::Put("stance defensive".into())),
            "the stance is restored, not left where the climb needed it"
        );
    }

    #[test]
    fn a_table_name_with_an_apostrophe_still_transpiles() {
        // "Cat's Paw" - the apostrophe is data, not a delimiter.
        let got = transpile(
            r##";e table = "Cat's Paw"; fput "go #{table} table" if dothistimeout("go #{table} table", 25, /You (?:and your group )?head over to|waves.*you.*(?:invites|inviting) you(?: and your group)? to (?:join|come sit at)/) =~ /inviting you|invites you/"##,
        )
        .expect("apostrophe table name transpiles");
        match &got[0] {
            WalkAction::Await { cmd, .. } => {
                assert_eq!(cmd.as_deref(), Some("go Cat's Paw table"))
            }
            other => panic!("expected an Await, got {other:?}"),
        }
    }

    #[test]
    fn event_origin_var_gates_which_return_edge_is_routable() {
        // Duskruin has ELEVEN entrances all landing in 26905, and 26905 has
        // twelve exits back that ALL send `go wagon`. The only thing telling
        // them apart is the variable the entry edge set, read by the return
        // edge's timeto. Drop it and every return edge is unroutable - or
        // worse, they all look equal and you exit somewhere you didn't ask for.
        let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let enter = transpile(
            ";e 2.times{fput \"event transport duskruin\"};\
             UserVars.mapdb_duskruin_origin = 2426;",
        )
        .expect("entry edge transpiles");
        assert_eq!(
            enter.last(),
            Some(&WalkAction::SetVar {
                name: "mapdb_duskruin_origin".into(),
                value: Some("2426".into()),
            }),
            "the entry records where we came from: {enter:?}"
        );

        let cost = TimeTo::Proc(
            ";e (!UserVars.mapdb_duskruin_origin.nil? and \
             UserVars.mapdb_duskruin_origin == 2426) ? 0.2 : nil;"
                .into(),
        );
        let db = MapDb::from_json("[]").unwrap();

        set_mapdb_var("mapdb_duskruin_origin", None);
        assert_eq!(
            resolve_timeto_depth(&db, &cost, 0),
            None,
            "with no origin recorded the return edge is not routable"
        );
        set_mapdb_var("mapdb_duskruin_origin", Some("9410".into()));
        assert_eq!(
            resolve_timeto_depth(&db, &cost, 0),
            None,
            "a DIFFERENT origin doesn't open this exit"
        );
        set_mapdb_var("mapdb_duskruin_origin", Some("2426".into()));
        assert_eq!(
            resolve_timeto_depth(&db, &cost, 0),
            Some(0.2),
            "the matching origin makes exactly this exit routable"
        );
        set_mapdb_var("mapdb_duskruin_origin", None);
    }

    #[test]
    fn leaving_an_event_area_clears_its_origin() {
        let got = transpile(";e move('go wagon');UserVars.mapdb_duskruin_origin = nil;")
            .expect("exit edge transpiles");
        assert_eq!(
            got,
            vec![
                WalkAction::Move("go wagon".into()),
                WalkAction::SetVar {
                    name: "mapdb_duskruin_origin".into(),
                    value: None,
                },
            ],
            "leaving clears the marker so a later trip can't reuse it"
        );
    }

    #[test]
    fn quoting_variants_of_supported_shapes_transpile() {
        use WalkAction::*;
        // Double quotes are the SAME shapes we already handled; an edge
        // failing over a quote character was a bug in our regexes.
        assert_eq!(
            transpile(r#";e move "go vortex""#),
            Some(vec![Move("go vortex".into())])
        );
        assert_eq!(
            transpile(r#";e fput "go gang""#),
            Some(vec![Put("go gang".into())])
        );
        assert_eq!(
            transpile(";e waitrt?; move 'east'"),
            Some(vec![WaitRt, Move("east".into())])
        );
    }

    #[test]
    fn keyed_door_only_unlocks_when_the_key_is_on_you() {
        let got = transpile(
            ";e door='iron door';key=GameObj.inv.find{|k| k.name=='brass key';};\
             if !key.nil? then multifput 'unlock door', 'open door', 'go door'; end;",
        )
        .expect("keyed door transpiles");
        match &got[0] {
            WalkAction::If { cond, then, els } => {
                assert_eq!(*cond, Cond::HasItem("brass key".into()));
                assert_eq!(then.len(), 3, "the unlock sequence: {then:?}");
                // Without the key, sending the whole sequence just fails
                // noisily; try the door instead.
                assert_eq!(els, &[WalkAction::Move("go iron door".into())]);
            }
            other => panic!("expected an If, got {other:?}"),
        }
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

    // --- statement sequences ---------------------------------------------

    #[test]
    fn sequence_of_moves_after_a_search() {
        use WalkAction::*;
        // The shape that made up a third of the residue: no fixed-arity
        // recognizer matches it, but every statement is known.
        assert_eq!(
            transpile(";e fput 'search';move 'go path';move 'northeast';move 'northeast'"),
            Some(vec![
                Put("search".into()),
                Move("go path".into()),
                Move("northeast".into()),
                Move("northeast".into()),
            ])
        );
    }

    #[test]
    fn multifput_is_variadic() {
        use WalkAction::*;
        // Three arguments: both fixed-arity MULTIFPUT recognizers miss this.
        assert_eq!(
            transpile(";e multifput 'unlatch door', 'open door', 'go door'"),
            Some(vec![
                Put("unlatch door".into()),
                Put("open door".into()),
                Put("go door".into()),
            ])
        );
    }

    #[test]
    fn hand_management_around_a_climb() {
        use WalkAction::*;
        assert_eq!(
            transpile(";e empty_hands\nmove \"climb well\"\nwaitrt?\nfill_hands"),
            Some(vec![
                EmptyHands,
                Move("climb well".into()),
                WaitRt,
                FillHands,
            ])
        );
    }

    #[test]
    fn script_bookkeeping_is_dropped_not_refused() {
        use WalkAction::*;
        // `$go2_restart = true` has no travel meaning; refusing the whole body
        // over it is what kept these edges in the residue.
        assert_eq!(
            transpile(";e fput 'go vortex';fput 'go vortex';$go2_restart=true"),
            Some(vec![Put("go vortex".into()), Put("go vortex".into())])
        );
        assert_eq!(
            transpile(";e $SILVERWOOD_TOWN=:imt;move 'go door'"),
            Some(vec![Move("go door".into())])
        );
    }

    #[test]
    fn parenthesised_and_double_quoted_arguments() {
        use WalkAction::*;
        assert_eq!(
            transpile(";e empty_hand;fput('go river');move('go river')"),
            Some(vec![
                EmptyHands,
                Put("go river".into()),
                Move("go river".into())
            ])
        );
    }

    #[test]
    fn block_control_flow_is_still_refused() {
        use WalkAction::*;
        // Concatenating these statements would drop the condition and cross an
        // edge the script deliberately gated.
        assert_eq!(
            transpile(";e if checksitting; fput 'stand'; end; move 'north'"),
            None
        );
        assert_eq!(
            transpile(";e move 'n' if x; else; move 's'; end"),
            None
        );
        // A loop whose exit condition we cannot evaluate stays refused.
        assert_eq!(
            transpile(";e loop { move 'north' }; move 'east'"),
            None
        );
        // ...but a bounded `.times` is just an unrolled repeat, with no
        // condition to lose.
        assert_eq!(
            transpile(";e 3.times { fput 'ask sailor about boat' }"),
            Some(vec![
                Put("ask sailor about boat".into()),
                Put("ask sailor about boat".into()),
                Put("ask sailor about boat".into()),
            ])
        );
    }

    #[test]
    fn statement_modifiers_keep_their_condition() {
        use WalkAction::*;
        assert_eq!(
            transpile(";e move 'crawl south'; waitrt?; fput 'stand' unless standing?"),
            Some(vec![
                Move("crawl south".into()),
                WaitRt,
                If {
                    cond: Cond::Not(Box::new(Cond::Standing)),
                    then: vec![Put("stand".into())],
                    els: Vec::new(),
                },
            ])
        );
        assert_eq!(
            transpile(";e fput 'unhide' if checkspell(916); move 'go gate'"),
            Some(vec![
                If {
                    cond: Cond::SpellActive(916),
                    then: vec![Put("unhide".into())],
                    els: Vec::new(),
                },
                Move("go gate".into()),
            ])
        );
    }

    #[test]
    fn an_unknown_modifier_condition_refuses_the_body() {
        // Guessing a condition wrong crosses an edge the script gated.
        assert_eq!(
            transpile(";e fput 'stand' if Char.name == 'Bob'; move 'north'"),
            None
        );
    }

    #[test]
    fn waitfor_accepts_alternatives() {
        // The Hinterwilds caravans list several arrival lines; matching only
        // the first would hang the crossing until timeout.
        let Some(actions) =
            transpile(";e multifput 'inquire','order 2';waitfor 'It halts','It stops'")
        else {
            panic!("expected a transpile");
        };
        let WalkAction::Await { pattern, .. } = &actions[2] else {
            panic!("expected an await, got {:?}", actions[2]);
        };
        assert!(pattern.is_match("It stops"));
        assert!(pattern.is_match("It halts"));
    }

    #[test]
    fn an_unknown_statement_refuses_the_whole_body() {
        // Partial crossings are worse than none: dropping the unrecognized
        // step would leave the walker somewhere the router does not expect.
        assert_eq!(
            transpile(";e fput 'search'; frobnicate 'widget'; move 'north'"),
            None
        );
    }

    #[test]
    fn splitter_respects_quotes_and_nesting() {
        // A `;` inside a quoted argument must not split the statement.
        assert_eq!(
            split_statements("fput 'say a;b'; move 'north'"),
            vec!["fput 'say a;b'".to_string(), "move 'north'".to_string()]
        );
        // ...nor one inside a block.
        assert_eq!(
            split_statements("foo { a; b }; move 'north'"),
            vec!["foo { a; b }".to_string(), "move 'north'".to_string()]
        );
        // Escaped quotes do not end the string.
        assert_eq!(
            split_statements(r#"fput "say \"x;y\""; move 'north'"#),
            vec![r#"fput "say \"x;y\"""#.to_string(), "move 'north'".to_string()]
        );
    }

    #[test]
    fn while_modifier_becomes_a_bounded_repeat() {
        use WalkAction::*;
        // 2925 -> 2924, sole route through the Sleeping Lady Mountains pass.
        assert_eq!(
            transpile(";e fput 'north'; move 'north' while Room.current.id == 2925"),
            Some(vec![
                Put("north".into()),
                Repeat {
                    body: vec![StepMove("north".into())],
                    until: RepeatUntil::Cond(Cond::Not(Box::new(Cond::InRoom(2925)))),
                    max: MAX_RETRY_LOOP,
                },
            ])
        );
    }

    #[test]
    fn until_modifier_repeats_to_a_condition() {
        use WalkAction::*;
        assert_eq!(
            transpile(r#";e move "crawl hollow";fput "stand" until standing?"#),
            Some(vec![
                Move("crawl hollow".into()),
                Repeat {
                    body: vec![Put("stand".into())],
                    until: RepeatUntil::Cond(Cond::Standing),
                    max: MAX_RETRY_LOOP,
                },
            ])
        );
        // 26765 -> 27623 (Caligos Isle): search until the opening appears.
        assert_eq!(
            transpile(
                ";e fput 'search' until GameObj.loot.find{|x| x.name == 'craggy rough-hewn opening'}; fput 'go opening'"
            ),
            Some(vec![
                Repeat {
                    body: vec![Put("search".into())],
                    until: RepeatUntil::Cond(Cond::RoomHasObject(
                        "craggy rough-hewn opening".into()
                    )),
                    max: MAX_RETRY_LOOP,
                },
                Put("go opening".into()),
            ])
        );
    }

    #[test]
    fn modifier_keyword_inside_a_quote_does_not_split() {
        use WalkAction::*;
        assert_eq!(
            transpile(";e fput 'say meet me if you can'; move 'north'"),
            Some(vec![Put("say meet me if you can".into()), Move("north".into())])
        );
    }

    #[test]
    fn get_until_is_a_passive_await() {
        // 24239 -> 24240 (Feywrot Mire): the current carries you; the ladder
        // line is the arrival evidence. The echo is user chatter, dropped.
        let Some(actions) = transpile(
            r#";e echo "Waiting for current to carry you to the new room...";line = get until line =~ /sturdy ladder/"#,
        ) else {
            panic!("expected a transpile");
        };
        assert_eq!(actions.len(), 1);
        let WalkAction::Await { cmd, pattern, on_timeout, .. } = &actions[0] else {
            panic!("expected an await, got {:?}", actions[0]);
        };
        assert_eq!(*cmd, None);
        assert_eq!(*on_timeout, OnTimeout::Fail);
        assert!(pattern.is_match("You grab hold of a sturdy ladder."));
        // Mismatched variable names are NOT the idiom.
        assert_eq!(transpile(";e line = get until other =~ /ladder/"), None);
    }

    #[test]
    fn assigned_dothistimeout_rewrites_only_when_unused() {
        // Assigned but never read again: the assignment is noise.
        let Some(actions) = transpile(
            r#";e r = dothistimeout 'open gate', 10, /^You open|already open/; move 'go gate'"#,
        ) else {
            panic!("expected a transpile");
        };
        assert!(matches!(actions[0], WalkAction::Await { .. }));
        assert!(matches!(actions[1], WalkAction::Move(_)));
        // Read again afterwards: the body branches on the response, which
        // statement concatenation cannot represent. (The `if` block also
        // trips the gate; this pins the rewrite's own guard.)
        assert_eq!(
            transpile(
                ";e r = dothistimeout 'open gate', 10, /You open/; if r; move 'go gate'; end"
            ),
            None
        );
    }

    #[test]
    fn while_path_block_repeats_until_the_exit_is_gone() {
        use WalkAction::*;
        // 8544 -> 8546 (foothills of Zeltoph): work the room until the nw
        // exit stops being offered, i.e. we left through the crevice.
        assert_eq!(
            transpile(
                ";e while checkpaths.include?('nw'); fput 'search'; sleep 1; waitrt?; move 'go crevice'; end"
            ),
            Some(vec![Repeat {
                body: vec![
                    Put("search".into()),
                    Sleep(1.0),
                    WaitRt,
                    StepMove("go crevice".into()),
                ],
                until: RepeatUntil::Cond(Cond::Not(Box::new(Cond::PathAvailable("nw".into())))),
                max: MAX_RETRY_LOOP,
            }])
        );
    }

    #[test]
    fn group_follow_reduces_to_the_solo_crossing() {
        use WalkAction::*;
        // 3565 -> 3566 (Stone Valley), 11x in the corpus: scrape who
        // followed, move, wait for their arrival lines. Solo semantics —
        // move and wait out roundtime — are the crossing; a native walker
        // does not escort groups.
        let body = r#";e group_members = nil; clear.reverse.each { |line| if line =~ /^Obvious (paths|exits)/; break; elsif line =~ /^([A-Za-z ,]+) followed\.$/; group_members = $1.split(/, | and /); group_members.delete_if { |m| m =~ /^[Yy]our / }; group_members = nil if group_members.empty?; break; end }; move 'go pile'; if group_members; echo "Waiting for your group... "; begin; if get =~ /^(You reach out and hold )?([A-z][a-z]+)('s hand| joins your group)\.$/; group_members.delete $2; end; end while group_members.length > 0; end; waitrt?"#;
        assert_eq!(
            transpile(body),
            Some(vec![Move("go pile".into()), WaitRt])
        );
    }

    #[test]
    fn loop_break_room_becomes_repeat_until_room() {
        use WalkAction::*;
        // 11068 -> 11069 (Sea Caves): swim the tunnel until it sticks.
        assert_eq!(
            transpile(
                ";e loop{fput 'swim tunnel';pause 0.2;if Room.current.id == 11069;break;end}"
            ),
            Some(vec![Repeat {
                body: vec![Put("swim tunnel".into()), Sleep(0.2)],
                until: RepeatUntil::Room(11069),
                max: MAX_RETRY_LOOP,
            }])
        );
    }

    #[test]
    fn trailing_unless_block_keeps_its_condition() {
        use WalkAction::*;
        // 8227 -> 8245 (Wolves' Den): try the far door, correct if it was
        // the wrong one.
        assert_eq!(
            transpile(
                ";e move 'go second iron door'; unless Room.current.id == 8245; move 'go iron door'; move 'go first iron door'; end"
            ),
            Some(vec![
                Move("go second iron door".into()),
                If {
                    cond: Cond::Not(Box::new(Cond::InRoom(8245))),
                    then: vec![
                        Move("go iron door".into()),
                        Move("go first iron door".into())
                    ],
                    els: Vec::new(),
                },
            ])
        );
    }

    #[test]
    fn while_room_object_block_repeats_the_swim() {
        use WalkAction::*;
        // 6481 -> 6484 (Lysierian Hills): swim until it takes. The VERBATIM
        // mapdb body — it ends with a fill_hands AFTER the block, which a
        // truncated test body once hid.
        assert_eq!(
            transpile(
                ";e empty_hands; while Room.current == Room[6481]; put 'swim opening'; sleep 1; waitrt?; end; fill_hands"
            ),
            Some(vec![
                EmptyHands,
                Repeat {
                    body: vec![Put("swim opening".into()), Sleep(1.0), WaitRt],
                    until: RepeatUntil::Cond(Cond::Not(Box::new(Cond::InRoom(6481)))),
                    max: MAX_RETRY_LOOP,
                },
                FillHands,
            ])
        );
    }

    #[test]
    fn ice_descent_takes_the_cautious_branch() {
        use WalkAction::*;
        // 2877 -> 2878 (Sleeping Lady, 21-edge family): we can't cast, so
        // always wait out the slip window before stepping.
        let body = ";e \n\t\tresolve=Spell['Sigil of Resolve']\n\t\thaste=Spell['Haste']\n\t\tif UserVars.mapdb_ice_mode == 'wait' || Skills.survival < 50 || XMLData.encumbrance_value >= 50\n\t\t\techo 'trying not to slip...'; sleep 6\n\t\telsif resolve.known? && resolve.affordable? && !resolve.active?\n\t\t\tresolve.cast\n\t\tend\n\t\tresult = fput 'down'\n\t\tif result =~ /^Rushing heedlessly/\n\t\t\thaste.cast if haste.known? && haste.affordable? && !haste.active?\n\t\t\tfput 'stand'\n\t\t\t$go2_restart = true\n\t\tend\n\t";
        assert_eq!(
            transpile(body),
            Some(vec![Sleep(6.0), Move("down".into()), WaitRt])
        );
    }

    #[test]
    fn times_search_then_enter_the_found_feature() {
        use WalkAction::*;
        // 1216 -> 1217 (Upper Trollfang): search for the footpath, then take
        // it. The loop exits when the footpath shows up as a room object.
        let Some(actions) = transpile(
            ";e 10.times { result = dothistimeout 'search', 5, /don't find anything|discover a small footpath|Round ?time/; waitrt?; break if result =~ /discover a small footpath/ }; move 'go footpath'",
        ) else {
            panic!("expected a transpile");
        };
        let Repeat { until, max, .. } = &actions[0] else {
            panic!("expected a repeat, got {:?}", actions[0]);
        };
        assert_eq!(
            *until,
            RepeatUntil::Cond(Cond::RoomHasObject("footpath".into()))
        );
        assert_eq!(*max, 10);
        assert_eq!(actions[1], Move("go footpath".into()));
    }

    #[test]
    fn key_door_gates_on_the_key_and_names_the_fix() {
        use WalkAction::*;
        let body = r##";e if GameObj.inv.find {|obj| obj.noun == "key"};fput "go beechwood door";else;empty_hand;multifput "get my #{UserVars.journeys_end} from my #{UserVars.keysack}","go beechwood door","put my key in my #{UserVars.keysack}";fill_hand;end"##;
        let Some(actions) = transpile(body) else {
            panic!("expected a transpile");
        };
        let If { cond, then, els } = &actions[0] else {
            panic!("expected an if, got {:?}", actions[0]);
        };
        assert_eq!(*cond, Cond::HasItem("key".into()));
        assert_eq!(*then, vec![Move("go beechwood door".into())]);
        assert!(
            matches!(&els[0], PauseForUser { msg, .. } if msg.contains("beechwood door")),
            "missing-key message names the door"
        );
    }

    #[test]
    fn zeltoph_door_picks_the_lock_on_the_locked_response() {
        use WalkAction::*;
        let body = ";e fput 'open door';while line = get;if ['You open the nearly invisible stone door.', 'That is already open.'].include?(line);fput 'go door';break;elsif line == 'It appears to be locked.';empty_hands;fput 'get lockpick';fput 'pick door';fput 'stow lockpick';fill_hands;fput 'open door';fput 'go door';break;end;end";
        let Some(actions) = transpile(body) else {
            panic!("expected a transpile");
        };
        let Await { cmd, if_match, .. } = &actions[0] else {
            panic!("expected an await, got {:?}", actions[0]);
        };
        assert_eq!(cmd.as_deref(), Some("open door"));
        let (locked, dance) = if_match.as_ref().expect("lockpick branch");
        assert!(locked.is_match("It appears to be locked."));
        assert_eq!(dance.len(), 6, "empty, get, pick, stow, fill, reopen");
        assert_eq!(actions[1], Move("go door".into()));
    }

    #[test]
    fn dothis_branch_takes_only_the_success_arm() {
        use WalkAction::*;
        // 3239 -> 3264 (Rolaren gate): open -> go; the locked/sigil-climb
        // tail is not attempted — timeout fails the edge instead.
        let body = ";e r = dothistimeout 'open rolaren gate', 10, /^You open|^That is already open|^It appears to be locked/; if r =~ /^You open|^That is already open/; move 'go rolaren gate'; elsif r =~ /^It appears to be locked/; fput 'climb gate'; end";
        let Some(actions) = transpile(body) else {
            panic!("expected a transpile");
        };
        let Await { cmd, pattern, on_timeout, .. } = &actions[0] else {
            panic!("expected an await, got {:?}", actions[0]);
        };
        assert_eq!(cmd.as_deref(), Some("open rolaren gate"));
        assert_eq!(*on_timeout, OnTimeout::Fail);
        assert!(pattern.is_match("You open the gate."));
        assert!(!pattern.is_match("It appears to be locked."));
        assert_eq!(actions[1], Move("go rolaren gate".into()));
        // Mismatched variables are not the idiom.
        assert_eq!(
            transpile(";e r = dothistimeout 'open gate', 10, /x/; if q =~ /x/; move 'go gate'; else; end"),
            None
        );
    }

    #[test]
    fn krag_crevice_hunt_reduces_to_search_step_reset() {
        use WalkAction::*;
        let body = r#";e group_members = nil; clear.reverse.each { |line| if line =~ /^Obvious (paths|exits)/; break; elsif line =~ /^([A-Za-z ,]+) followed\.$/; group_members = $1.split(/, | and /); group_members.delete_if { |m| m =~ /^[Yy]our / }; group_members = nil if group_members.empty?; break; end }; result = nil; while !result; if celerity = Spell[506] and celerity.known? and celerity.affordable? and not celerity.active?; celerity.cast; end; fput 'search'; result = matchtimeout(1,/you discover a narrow crevice/); waitrt?; if !result then multimove 'n','s'; end; end; fput 'point crevice' if group_members; move 'go crevice'; if group_members; echo 'Waiting for your group... To ditch them, ;send go '; while (group_members.length > 0) and (line = get); if line =~ /^(You reach out and hold )?([A-z][a-z]+)('s hand| joins your group)\.$/; group_members.delete $2; elsif line == 'go'; break; end; end; end"#;
        let Some(actions) = transpile(body) else {
            panic!("expected a transpile");
        };
        let Repeat { body: inner, until, .. } = &actions[0] else {
            panic!("expected a repeat, got {:?}", actions[0]);
        };
        assert_eq!(
            *until,
            RepeatUntil::Cond(Cond::RoomHasObject("crevice".into()))
        );
        assert_eq!(inner[0], Put("search".into()));
        assert_eq!(actions[1], Move("go crevice".into()));
    }

    #[test]
    fn fog_retry_repeats_until_the_room_changes() {
        use WalkAction::*;
        let body = r#";e UserVars.mapdb_redforest_location = 'WL';result = nil;until result =~ /Obvious paths: northeast, southeast/;fput "stand" until standing?;result = dothistimeout "go fog", 5, /You attempt to navigate your way through the fog, but get turned around and come right back out where you started!|Obvious paths: northeast, southeast/;if result =~ /You attempt to navigate your way through the fog, but get turned around and come right back out where you started!/;sleep 0.5;waitrt?;end;end"#;
        let Some(actions) = transpile(body) else {
            panic!("expected a transpile");
        };
        let Repeat { body: inner, until, .. } = &actions[0] else {
            panic!("expected a repeat, got {:?}", actions[0]);
        };
        assert_eq!(*until, RepeatUntil::RoomChanged);
        assert!(
            matches!(&inner[1], Await { cmd: Some(c), .. } if c == "go fog"),
            "the fog attempt is re-sent each lap"
        );
    }

    #[test]
    fn single_statement_bodies_do_not_recurse() {
        // A lone unrecognized statement must return None rather than loop
        // back into the chain that just declined it.
        assert_eq!(transpile(";e frobnicate 'widget'"), None);
    }
}
