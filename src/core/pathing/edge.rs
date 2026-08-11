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

    pub fn source(&self) -> &str {
        &self.source
    }
}

impl PartialEq for AwaitPattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cond {
    /// `checkspell(N)` — spell N currently active.
    SpellActive(u16),
    /// `checksitting`
    Sitting,
    /// `kneeling?`
    Kneeling,
}
