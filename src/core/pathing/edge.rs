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
