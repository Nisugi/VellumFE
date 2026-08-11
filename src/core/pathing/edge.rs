//! The WalkAction DSL — what a StringProc edge *does*, as data.
//!
//! Lich stores executable Ruby on ~9% of wayto edges. VellumFE never runs
//! Ruby; the transpiler (`pathing::transpile`) pattern-matches the common
//! idioms into these declarative actions, which the walk executor interprets
//! with the same state machine that handles plain edges.

/// One step of a scripted edge.
#[derive(Debug, Clone, PartialEq)]
pub enum WalkAction {
    /// `";e true"` — the edge exists, nothing to send.
    Noop,
    /// Send a movement command and expect the room to change (the executor
    /// starts arrival-watching after the script finishes).
    Move(String),
    /// Send a movement command mid-script and WAIT for the room to change
    /// before continuing to the next action — a paced walk step (unlike `Move`,
    /// which is the terminal room-changer sent as the script ends). Used by
    /// multi-room crossings like the day-pass buy walk, where each step must
    /// land before the next command (you can't `withdraw` before reaching the
    /// bank). The room it lands in is not verified against a specific id.
    StepMove(String),
    /// Send a command with no room-change expectation ("push wall").
    Put(String),
    /// Wait out roundtime (`waitrt?`).
    WaitRt,
    /// Fixed pause in seconds (`pause 0.5`).
    Sleep(f32),
    /// Conditional branch. Unknown-answer conditions take `els` — the
    /// unconditional branch of every idiom in the corpus is the safe one.
    If {
        cond: Cond,
        then: Vec<WalkAction>,
        els: Vec<WalkAction>,
    },
    /// Stow both held items so a climb/swim can proceed (Lich's
    /// `empty_hands` → `Lich::Stash.stash_hands(both: true)`). The executor
    /// drives the StashService and remembers what it stowed so `FillHands`
    /// can put it back (LIFO). A no-op when both hands are already empty.
    EmptyHands,
    /// Retrieve whatever the matching `EmptyHands` stowed (Lich's
    /// `fill_hands` → `equip_hands(both: true)`), replaying the stashed
    /// retrieval plan.
    FillHands,
    /// Wait for a game line matching `pattern`, optionally sending `cmd`
    /// first (Lich's `await`, which subsumes `waitfor`/`dothistimeout`/
    /// `matchtimeout`). The workhorse of scripted crossings: ferry arrivals,
    /// door responses, searched exits, lever puzzles.
    ///
    /// `cmd: None` is the PASSIVE form and is not an optimization — it is
    /// required for commands that aren't idempotent (boarding a ferry twice
    /// is not the same as boarding it once), so a retry must not re-send.
    Await {
        /// Command to send when the await arms, if any.
        cmd: Option<String>,
        /// Regex the game line must match.
        pattern: Box<AwaitPattern>,
        /// How long to wait before `on_timeout` decides (seconds).
        timeout: f32,
        /// What a timeout means. Defaults to `Continue` — most corpus awaits
        /// are advisory, and failing the edge on a missed cosmetic line would
        /// ban walkable edges.
        on_timeout: OnTimeout,
        /// Extra steps to run only when the matching line ALSO matches this
        /// pattern (Lich's `if_match`). How "the response decides what to do
        /// next" is expressed: a table invitation needs accepting, a plain
        /// seating does not.
        if_match: Option<(Box<AwaitPattern>, Vec<WalkAction>)>,
    },
    /// Run `body` until `until` is satisfied, at most `max` iterations.
    ///
    /// `max` is clamped by the interpreter regardless of what the data says:
    /// bad map data may waste a route, it must never hang the client.
    Repeat {
        body: Vec<WalkAction>,
        until: RepeatUntil,
        max: u32,
    },
    /// Leave the innermost `Repeat` (Lich's `break`). A no-op outside a loop.
    Break,
    /// Walk a fixed direction table starting at whichever entry matches the
    /// current room, wrapping at the end, until a landmark object appears —
    /// then send `enter`.
    ///
    /// The corpus shape is `start_room = [ids]; dirs = [...]; if index =
    /// start_room.index(Room.current.id); until checkloot.include?('X'); move
    /// dirs[index]; index += 1; index = 0 if index >= dirs.length; end; move
    /// 'climb X'`. It's a table-driven walk, not an algorithm: the whole
    /// program is the two tables plus the landmark.
    ///
    /// `dirs` is deliberately independent of `start_room` — the corpus tables
    /// differ in length and the walk wraps, so the direction list is a cycle
    /// the character joins at its own offset.
    GuidedRoute {
        /// Room ids, positionally matched to an offset into `dirs`.
        start_rooms: Vec<u32>,
        /// The direction cycle. Longer than `start_rooms` in most corpus edges.
        dirs: Vec<String>,
        /// Landmarks that end the walk, each with the command to enter it.
        /// A list, not one: 206 corpus edges walk until EITHER a door or a
        /// mirror appears and then enter whichever one it was, so a single
        /// landmark can't express them.
        landmarks: Vec<(String, String)>,
    },
    /// Voln Symbol of Seeking: cast repeatedly, and each time the symbol
    /// offers a room, confirm if it's the one we want. 36 corpus edges.
    ///
    /// The offered room arrives as a room NAME, not an id, so the check is a
    /// title match against the destination — which is why this needs the
    /// destination's title resolved at crossing time rather than being a
    /// plain step list.
    VolnSeeking { destination: u32 },
    /// Set (or with `None`, clear) a `UserVars.mapdb_<name>` scratch variable.
    ///
    /// Load-bearing, not bookkeeping: an event area's dozen return edges all
    /// send the same command and are told apart ONLY by the variable their
    /// entry edge set, which their `timeto` reads. Skipping it makes every
    /// return edge unroutable, or makes them all look equal so you exit
    /// somewhere you didn't ask for.
    SetVar { name: String, value: Option<String> },
    /// Warp via the Isle of Four Winds trinket: retrieve it if stowed, `turn`
    /// it, then put it back where it came from. 30 corpus edges.
    ///
    /// The item is named by config (`go2.fwi_trinket`), the way Lich names it
    /// with `UserVars.mapdb_fwi_trinket`. Resolution is deferred to crossing
    /// time because the trinket's exist id and container are live state.
    TrinketWarp,
    /// Send `cmd`; if the room DIDN'T change, run `fallback` (Lich's
    /// `try_move`). 27 corpus edges — a locker door that must be closed
    /// before the exit works, and similar "try it, fix it, retry" doors.
    TryMove {
        cmd: String,
        fallback: Vec<WalkAction>,
    },
    /// Walk a room -> direction MAP until `target` is reached, prefixing each
    /// direction with `verb` ("swim north"). 75 corpus edges.
    ///
    /// Distinct from [`WalkAction::GuidedRoute`], which walks a positional
    /// CYCLE joined at an offset and stops on a landmark. Here each room
    /// names its own next direction, and arrival is a room id — a table
    /// lookup per step rather than an index that advances.
    RouteTable {
        /// `room -> direction` for every room on the route.
        dirs: Vec<(u32, String)>,
        /// Room that ends the walk.
        target: u32,
        /// Verb prefixed to each direction ("swim"); empty sends the bare dir.
        verb: String,
        /// Rooms where hands must be emptied first (a swim needs free hands).
        hands_free_in: Vec<u32>,
    },
    /// Hand this crossing to the minotaur maze walker (see travel::minotaur):
    /// learn adjacency by walking until `target` is reached. 497 corpus edges.
    MinotaurMaze { target: u32, maze_rooms: Vec<u32> },
    /// Stop and tell the user to do something the client can't (place a gem in
    /// hand, be strong enough to turn a wheel), resuming when `until` holds.
    ///
    /// This ABANDONS the trip rather than blocking forever: an automated
    /// walker silently waiting on a human is indistinguishable from a hang.
    /// The message names what's needed so the user can act and re-issue.
    PauseForUser {
        msg: String,
        /// Resume automatically if this becomes true within the timeout.
        until: Option<Cond>,
        timeout: f32,
    },
    /// Re-plan the route from the current room to the same destination
    /// (Lich's `$go2_restart = true`). A transpiled edge sets this when its
    /// script signals that the map may have changed under it; the executor
    /// wires it straight into `repath`.
    Replan,
}

/// A compiled await pattern. Holds the source alongside the [`Regex`] so the
/// action stays `Clone`/`PartialEq`/`Debug` (a `Regex` is none of those):
/// equality and display are defined by the source text, which is what the
/// mapdb actually carries.
#[derive(Debug, Clone)]
pub struct AwaitPattern {
    source: String,
    regex: regex::Regex,
}

impl AwaitPattern {
    /// Compile a pattern, `None` when the source isn't valid regex (the edge
    /// then doesn't transpile, rather than panicking at walk time).
    pub fn new(source: &str) -> Option<Self> {
        regex::Regex::new(source).ok().map(|regex| Self {
            source: source.to_string(),
            regex,
        })
    }

    pub fn is_match(&self, line: &str) -> bool {
        self.regex.is_match(line)
    }

    /// Named capture groups from the first match, for `{capture:name}`
    /// interpolation into a later command. `None` when the line doesn't match.
    ///
    /// Only NAMED groups are collected: an edge that wants a value has to say
    /// which one, and positional indices would silently shift whenever a
    /// pattern gains a group (the exact bug that shipped in Lich's converter).
    pub fn captures(&self, line: &str) -> Option<Vec<(String, String)>> {
        let caps = self.regex.captures(line)?;
        Some(
            self.regex
                .capture_names()
                .flatten()
                .filter_map(|name| {
                    caps.name(name)
                        .map(|m| (name.to_string(), m.as_str().to_string()))
                })
                .collect(),
        )
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

impl PartialEq for AwaitPattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

/// Substitute `{capture:name}` tokens in a command with values bound by an
/// earlier [`WalkAction::Await`].
///
/// An unbound token yields `None` rather than an empty string: sending
/// "pull  lever" because a capture didn't fire is worse than not sending at
/// all, since a half-formed command can do something unintended.
pub fn expand_captures(
    template: &str,
    bindings: &[(String, String)],
) -> Option<String> {
    // Cheap exit for the overwhelmingly common tokenless command.
    if !template.contains("{capture:") {
        return Some(template.to_string());
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{capture:") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "{capture:".len()..];
        let end = after.find('}')?;
        let name = &after[..end];
        let value = bindings
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())?;
        out.push_str(value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// What a timed-out [`WalkAction::Await`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnTimeout {
    /// Carry on with the next action. The default: most corpus awaits guard
    /// cosmetic text, and failing the edge would ban a walkable route.
    #[default]
    Continue,
    /// Fail the edge (ban + re-path, or hand off to Lich). For awaits whose
    /// line is the ONLY evidence the crossing worked — a ferry boarding.
    Fail,
    /// Re-send the command once, then fail if it times out again. Only valid
    /// on an active await; a passive one has nothing to re-send.
    Retry,
}

/// Termination condition for a [`WalkAction::Repeat`].
#[derive(Debug, Clone, PartialEq)]
pub enum RepeatUntil {
    /// Run exactly `max` times (the loop's own bound is the only condition).
    Count,
    /// Stop once the current room differs from where the loop started —
    /// Lich's `until_room_change`. The common "keep trying until we move".
    RoomChanged,
    /// Stop once the current room is this id (Lich's `until_room`).
    Room(u32),
    /// Stop once a condition holds.
    Cond(Cond),
}

/// Conditions the executor can answer from game state.
///
/// Deliberately a closed set answered from state we already track. An unknown
/// condition can't be "maybe" at walk time — it would either walk a route the
/// character can't actually take or refuse one it can — so anything not
/// listed here keeps its edge untranspiled, where the Lich fallback handles it.
#[derive(Debug, Clone, PartialEq)]
pub enum Cond {
    /// `checkspell(N)` — spell N currently active.
    SpellActive(u16),
    /// `checksitting`
    Sitting,
    /// `kneeling?`
    Kneeling,
    /// `standing?`
    Standing,
    /// `hidden?` or `invisible?`
    Hidden,
    /// A compass exit is available in the current room (`checkpaths`).
    /// Lowercased short form as the game reports it ("n", "se", "out").
    PathAvailable(String),
    /// A ground-loot noun is present in the current room (`checkloot` /
    /// `GameObj.loot.find`).
    RoomHasObject(String),
    /// Home-town citizenship matches (case-insensitive).
    Citizenship(String),
    /// Profession matches (case-insensitive).
    Profession(String),
    /// Society membership matches (case-insensitive).
    Society(String),
    /// Negation, so a recognizer can express "unless X" without every
    /// condition needing an inverted twin.
    Not(Box<Cond>),
    /// True when any member holds (Ruby's `a or b`). Empty is false.
    Any(Vec<Cond>),
    /// Standing in this specific room. `try_move`'s "did that work?" test and
    /// the guard on a trailing replan.
    InRoom(u32),
    /// Carrying an item matching this name (a door key). Resolved against the
    /// live inventory registry, so it covers containers too.
    HasItem(String),
}
