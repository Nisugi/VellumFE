//! Typed move-feedback events — the executor's window into game text.
//!
//! Lich's `move` (global_defs.rb:569-793) is a text-observing loop: it reads
//! game lines and reacts (hands-full → empty_hands, closed door → open, …).
//! VellumFE's walk executor is state-observing — it never sees raw lines — so
//! this module bridges the gap: an aho-corasick matcher (the same engine our
//! highlights use) turns each game line into a typed [`MoveFeedback`] event,
//! and the executor reacts to the typed event.
//!
//! Reset semantics (§09/§12): these are EDGE-triggered events, true for one
//! instant, consumed exactly once. So they live on a drained queue on
//! `GameState`, not on the per-tick `TravelContext` (which is rebuilt each
//! tick and resets level-triggered state for free). The parser pushes; the
//! executor's owner drains once per tick.

use std::sync::LazyLock;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};

/// A typed movement-feedback event distilled from a game line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveFeedback {
    /// A `<nav>` room-change tag arrived (Lich's `room_count` increment). The
    /// universal "you moved" signal — fires even for UID-less rooms, so
    /// arrival detection never hangs waiting to resolve an unmapped room.
    NavArrived,
    /// "Maybe if your hands were empty…" — empty hands, retry, refill.
    HandsFull,
    /// A door/gate that "appears/seems to be closed" — `open` it, retry.
    DoorClosed,
    /// A climb/slip that dropped you — stand, retry.
    Fell,
    /// "You'll have to climb that." — swap the `go` verb to `climb`.
    NeedClimb,
    /// "You can't climb that." — swap `climb` back to `go`.
    CantClimb,
    /// A hard failure ("You can't go there", impassable, …). The edge is bad;
    /// safe to remove from the graph (Lich's `move` returns `false`).
    MoveFailedRemovable,
    /// A blocked-but-transient failure (guard stops you, can't be dragged, try
    /// later). Keep the edge (Lich returns `nil`, "don't delete").
    MoveFailedKeep,
    /// An item at your feet you don't want to leave — `stow feet`, retry.
    ItemAtFeet,
    /// "You cannot do that while mounted." — urchin travel is incompatible
    /// with being mounted; drop it and re-path (Lich go2:2336-2346).
    Mounted,
}

macro_rules! patterns {
    ($($variant:ident => [$($pat:literal),+ $(,)?]),+ $(,)?) => {
        /// (pattern, event) pairs, flattened for the matcher.
        static PAIRS: LazyLock<Vec<(&'static str, MoveFeedback)>> = LazyLock::new(|| {
            let mut v = Vec::new();
            $(
                $( v.push(($pat, MoveFeedback::$variant)); )+
            )+
            v
        });
    };
}

// Literal substrings taken from Lich's `move` regexes (global_defs.rb). Each
// maps a game-message fragment to a typed event. Aho-corasick matches the
// literal; the fragments are chosen to be unambiguous.
patterns! {
    HandsFull => [
        "Maybe if your hands were empty",
        "freeing up both hands might help",
        "with your hands full",
        "need empty hands to climb",
        "too difficult to swim holding",
        "need both hands free for such a difficult task",
    ],
    DoorClosed => [
        "appears to be closed",
        "seems to be closed",
        "cannot quite manage to squeeze between the stone doors",
    ],
    Fell => [
        "you fall to the ground",
        "fall unceremoniously to the ground",
        "drop back to the ground",
        "land on your rear",
        "landing flat on your face",
        "fall flat on your back",
        "The ground approaches you at an alarming rate",
        "You go flying down several feet",
    ],
    NeedClimb => [
        "have to climb that",
    ],
    CantClimb => [
        "You can't climb that",
    ],
    ItemAtFeet => [
        "at your feet, and do not wish to leave it behind",
        "As you prepare to move away, you remember",
    ],
    Mounted => [
        "You cannot do that while mounted",
    ],
    MoveFailedRemovable => [
        "You can't go there",
        "in that direction",
        "Where are you trying to go",
        "I could not find what you were referring to",
        "How do you plan to do that here",
        "You cannot do that",
        "is too far away",
        "You may not pass",
        "become impassable",
        "prevents you from entering",
        "There doesn't seem to be any way to do that at the moment",
    ],
    MoveFailedKeep => [
        "is unable to follow you",
        "An unseen force prevents you",
        "aren't allowed to enter here",
        "your grip gives way and you fall down",
        "I'll need to see your ticket",
        "Only members of registered groups may enter",
        "preventing him from being dragged",
        "preventing her from being dragged",
        "perhaps you should try again later",
    ],
}

/// The compiled matcher. Built once; leftmost-longest so a longer pattern
/// wins over a shorter prefix.
static MATCHER: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasickBuilder::new()
        .match_kind(MatchKind::LeftmostLongest)
        .ascii_case_insensitive(false)
        .build(PAIRS.iter().map(|(p, _)| *p))
        .expect("valid move-feedback patterns")
});

/// Classify a game line into a move-feedback event, if any. Returns the FIRST
/// (leftmost) matching pattern's event — the earliest signal on the line.
pub fn classify_line(line: &str) -> Option<MoveFeedback> {
    let m = MATCHER.find(line)?;
    Some(PAIRS[m.pattern().as_usize()].1.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_move_recovery_messages() {
        assert_eq!(
            classify_line("Maybe if your hands were empty you could do that."),
            Some(MoveFeedback::HandsFull)
        );
        assert_eq!(
            classify_line("The oaken door appears to be closed."),
            Some(MoveFeedback::DoorClosed)
        );
        assert_eq!(
            classify_line("You slip on a patch of ice and flail uselessly as you land on your rear."),
            Some(MoveFeedback::Fell)
        );
        assert_eq!(
            classify_line("You're going to have to climb that."),
            Some(MoveFeedback::NeedClimb)
        );
        assert_eq!(
            classify_line("You can't go there."),
            Some(MoveFeedback::MoveFailedRemovable)
        );
        assert_eq!(
            classify_line("A guardsman is unable to follow you."),
            Some(MoveFeedback::MoveFailedKeep)
        );
        assert_eq!(
            classify_line("You notice a gem at your feet, and do not wish to leave it behind."),
            Some(MoveFeedback::ItemAtFeet)
        );
        // Ordinary prose doesn't match.
        assert_eq!(classify_line("A cat wanders by."), None);
    }
}
