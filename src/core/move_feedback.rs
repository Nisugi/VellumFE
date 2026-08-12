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
    /// The command landed during roundtime ("...wait 2 seconds."): nothing
    /// failed, re-send once the RT clears instead of waiting out a timeout.
    RtWait,
    /// A purchase was refused for lack of silver ("You don't have enough
    /// silvers") - the generic fare/ticket form, distinct from the
    /// day-pass clerk's DayPassTooPoor phrasing.
    TooPoor,
    /// A gated entrance refused a hidden/invisible character - `unhide` and
    /// retry (Lich move(), global_defs.rb:615-617).
    MustUnhide,
    /// A postural rejection ("will have to stand up first") - stand and
    /// retry (global_defs.rb:714-717).
    MustStand,
    /// "Sorry, you may only type ahead" - back off ~1s and re-send; never a
    /// failure (global_defs.rb:729-731).
    TypeAhead,
    /// "You are still stunned." - wait the stun out, then retry
    /// (global_defs.rb:732-734).
    StillStunned,
    /// "You're still recovering from your recent..." - short backoff, retry
    /// (global_defs.rb:718-720).
    StillRecovering,
    /// "You don't seem to be able to move to do that." - senses lost;
    /// brief wait, retry (global_defs.rb:756-762).
    NoControl,
    /// A transient environmental refusal - guard check-ins, fog
    /// entanglement, vertigo backouts, sinking panic, energy fields. Lich
    /// retries these forever inside move() with sleep 1 (global_defs.rb:
    /// 639-642); they must NEVER ban or re-path away from the edge.
    TransientRetry,
    /// "It's pitch dark and you can't see a thing!" - needs a light source;
    /// Lich surfaces the message and treats the move as done
    /// (global_defs.rb:763-765).
    PitchDark,
    /// "You are too injured to be doing any climbing!" - Lich casts Resolve
    /// if known, else keeps the edge (returns nil, global_defs.rb:662-669).
    TooInjured,
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
    // --- Chronomage day-pass buy responses (Lich's dothistimeout waits) ---
    /// The clerk/agent responded to `ask ... for <town>` — the offer/confirm
    /// prompt ("...says to you, ...will be free of charge / cost N silvers.
    /// ASK me again..."). Lich's first-ask wait `/says to you/`.
    DayPassOffered,
    /// The pass was handed over: "quickly hands you a Chronomage day pass".
    DayPassInHand,
    /// Can't afford it: "...don't have enough...". Triggers the bank trip.
    DayPassTooPoor,
    /// A bank withdrawal completed ("the teller carefully records" / "flips
    /// through the books"). Ends the day-pass funding wait.
    WithdrawOk,
    /// `raise`ing the pass teleported us: "whirlwind of color subsides".
    RaiseTraveled,
    /// `open <container>` reported it was ALREADY open ("That is already
    /// open.") — the day-pass sack was open before we touched it, so skip the
    /// close at the end (Lich's `close_sack = true` only when IT opened it).
    ContainerAlreadyOpen,
    /// `open <container>` succeeded ("You open ..."). With AlreadyOpen, the
    /// response the day-pass preamble waits on (Lich's open_regex).
    ContainerOpened,
    /// A `get`/`take` landed (or was moot) — the get_regex shapes Lich waits
    /// on before raising the pass.
    ItemGot,
    /// A drop landed ("As you let go..." / "You drop") — Lich's expired-pass
    /// `_drag #id drop` wait.
    ItemDropped,
    /// A put/stow landed ("You put/slip/tuck ...") — the put_regex shapes Lich
    /// waits on after `_drag #pass #sack`.
    ItemStowed,
    /// `raise` failed because we're not in the Chronomage waiting room
    /// ("not a Chronomage teleportation waiting room" / "not valid for
    /// departures" / "As you go to raise your pass, you realize").
    RaiseWrongRoom,
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
        // The climb-failure family (global_defs.rb:643-648): sleep, stand,
        // retry - never a failure.
        "slip after a few feet and fall",
        "but quickly realize",
        "but without success",
        "wrong approach",
        "slowly retreat back, reassessing",
        "can't seem to find purchase",
        "you make your way back up",
    ],
    NeedClimb => [
        "have to climb that",
    ],
    MustUnhide => [
        "and remain hidden or invisible",
        "if he can't see you!",
        "when you can't be seen",
        "You can't do that without being seen",
        "no one can see you right now",
    ],
    MustStand => [
        "will have to stand up first",
        "must be standing first",
        "You'll have to get up first",
        "But you're already sitting!",
        "Shouldn't you be standing first",
        "Try standing up",
        "Perhaps you should stand up",
        "Standing up might help",
        "You should really stand up first",
        "You can't do that while sitting",
        "You must be standing to do that",
        "You can't do that while lying down",
        "You must be standing",
        "You can't do that from that position",
    ],
    TypeAhead => [
        "Sorry, you may only type ahead",
    ],
    StillStunned => [
        "You are still stunned",
    ],
    StillRecovering => [
        "You're still recovering from your recent",
    ],
    NoControl => [
        "You don't seem to be able to move to do that",
    ],
    TransientRetry => [
        "you begin to sink!",
        "You need to make sure you check in",
        "no way to climb the slippery tendrils",
        "back to safe ground",
        "your persistence will pay off",
        "field of magical crimson and gold energy",
        "quickly become entangled",
        "A wave of dizziness hits you",
        "but the steepness is intimidating",
        "Struck by vertigo",
        "your footing is questionable",
        "doesn't budge",
        "flounder around in the water",
        "blunder around in the water",
        "struggle against the swift current to swim",
        "slap at the water in a sad failure",
        "work against the swift current to swim",
        "current catches you and whips you back to shore",
        "disk only wobbles briefly",
    ],
    PitchDark => [
        "pitch dark and you can't see a thing",
    ],
    TooInjured => [
        "too injured to be doing any climbing",
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
    DayPassInHand => [
        "quickly hands you a Chronomage day pass",
    ],
    DayPassTooPoor => [
        "don't have enough",
    ],
    DayPassOffered => [
        // The clerk/agent's offer or confirm prompt.
        "ASK me again",
        "day pass to",
    ],
    WithdrawOk => [
        "carefully records the transaction",
        "flips through the books",
    ],
    RaiseTraveled => [
        "whirlwind of color subsides",
    ],
    ContainerAlreadyOpen => [
        "That is already open",
    ],
    ContainerOpened => [
        "You open",
    ],
    ItemGot => [
        "You remove",
        "You retrieve",
        "You grab",
        "You already have that",
        "You need a free hand",
    ],
    ItemDropped => [
        "As you let go",
        "You drop",
    ],
    // Just "You put": "You slip"/"You tuck" collide with fall messages
    // ("You slip on a patch of ice..."). The timeout covers exotic stows.
    ItemStowed => [
        "You put",
    ],
    RaiseWrongRoom => [
        "not a Chronomage teleportation waiting room",
        "not valid for departures",
        "As you go to raise your pass, you realize",
        // Lich's raise wait also matches these failure shapes.
        "pass is expired",
        "Raise what",
    ],
    RtWait => [
        "...wait",
    ],
    TooPoor => [
        "You don't have enough silvers",
        "you don't have enough silver",
    ],
    MoveFailedRemovable => [
        "You can't go there",
        // Specific to a move rejection ("You can't go/swim in that direction");
        // the bare "in that direction" alone matched combat/ambient prose.
        "go in that direction",
        "swim in that direction",
        "Where are you trying to go",
        "I could not find what you were referring to",
        "How do you plan to do that here",
        // "You cannot do that" alone is far too common in combat/social text.
        // The move-rejection form is "You cannot do that while ..." — but that
        // is handled by more specific events (Mounted etc.), so drop the bare
        // substring entirely.
        "is too far away",
        "You may not pass",
        "become impassable",
        "is too far above you to attempt that",
        "Definitely NOT a good idea",
        "Your attempt fails",
        "There doesn't seem to be any way to do that at the moment",
        "You settle yourself on",
        "You shouldn't annoy",
        "That's probably not a very good idea",
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
        "only performers should go",
        "carelessly bump into the guard",
        "Your reputation precedes you",
        "reads, \"Abandoned.\"",
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
