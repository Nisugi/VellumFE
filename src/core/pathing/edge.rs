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
    /// Re-plan the route from the current room to the same destination
    /// (Lich's `$go2_restart = true`). A transpiled edge sets this when its
    /// script signals that the map may have changed under it; the executor
    /// wires it straight into `repath`.
    Replan,
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
