//! The response-driven Chronomage day-pass BUY state machine.
//!
//! Buying a pass is not a fixed command list — it's a conversation: ask the
//! clerk, wait for the offer, ask again to confirm, and branch on whether the
//! pass was handed over or you're too poor (then walk to the bank, withdraw,
//! come back, re-ask). Only once the pass is in hand do you walk back to the
//! Chronomage waiting room and `raise` it to travel. Lich drives this with
//! `dothistimeout` (send a command, wait for a specific response line); we
//! mirror it here with a phase machine keyed on the typed day-pass feedback
//! events ([`MoveFeedback::DayPassOffered`] etc.), so each command waits for
//! its game response before the next — no command flooding.
//!
//! The USE path (raise a pass you already hold) is NOT here — it's a simple
//! WalkAction script in the executor. This module is only the buy conversation.

use crate::core::day_pass::{DayPassDeparture, DayPassDestination};
use crate::core::move_feedback::MoveFeedback as F;

/// How long to wait for a response before giving up on a phase (Lich's
/// dothistimeout is ~10s).
const RESP_TIMEOUT_MS: u64 = 12_000;
/// The expired-pass drop wait (Lich uses dothistimeout 2s here).
const DROP_TIMEOUT_MS: u64 = 2_500;

/// What the buy machine wants the executor to do this tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuyEvent {
    /// Send this command to the game.
    Send(String),
    /// The raise teleported us — the trip's day-pass leg is done. Carries the
    /// bought pass's exist-id so the executor can `_drag #id` it back into the
    /// sack (the `pass` noun doesn't work for `_drag`). `None` if we never
    /// captured an id (shouldn't happen once in-hand).
    Traveled { pass_id: Option<String> },
    /// Give up (a timeout or unexpected response). Carries a reason.
    Failed(String),
    /// Give up because silver ran out (no funding, or the bank couldn't
    /// cover it). The executor disables day-pass buying for the session
    /// (Lich: "Turning off buy_day_pass setting.", map_strategies.rb:740).
    FailedTooPoor(String),
}

/// The buy conversation's phases.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    /// Just started — step from the waiting room to the clerk. `sent_ms` is
    /// when the enter move went out (None until sent).
    ToClerk { sent_ms: Option<u64> },
    /// Sent the first `ask`; waiting for the offer (`/says to you/`).
    AwaitOffer { sent_ms: u64 },
    /// Sent the confirm `ask`; waiting for in-hand or too-poor.
    AwaitPass { sent_ms: u64 },
    /// Too poor: walking to the bank (index into `to_bank`), then withdrawing.
    ToBank { i: usize, sent_from: Option<u32>, sent_ms: u64 },
    /// Sent `withdraw`; waiting for the teller confirmation.
    AwaitWithdraw { sent_ms: u64 },
    /// Walking back from the bank (index into `from_bank`).
    FromBank { i: usize, sent_from: Option<u32>, sent_ms: u64 },
    /// Have the pass; stepping back to the waiting room before the raise.
    /// `sent_ms` is when the look+exit went out (None until sent).
    ToWaitingRoom { sent_ms: Option<u64>, sent_from: Option<u32> },
    /// Sent `raise pass`; waiting for the whirlwind (travelled) or a
    /// wrong-room/expired failure.
    AwaitRaise { sent_ms: u64 },
    /// Teleported; putting the bought pass back (response-gated) before
    /// concluding.
    PutBack(PutBack),
}

/// Inputs the machine reads each tick: the feedback events seen, the current
/// room, whether funding (Get Silvers) is allowed, the clock, and the ids of
/// items in each hand (to capture the bought pass's exist-id — `_drag`/`raise`
/// need the id, not the `pass` noun).
pub struct BuyTick<'a> {
    pub feedback: &'a [F],
    pub current_room: Option<u32>,
    pub get_silvers: bool,
    pub now_ms: u64,
    pub rt_remaining: f64,
    /// (id, noun) of the left/right hand items, if any.
    pub left_hand: Option<(&'a str, &'a str)>,
    pub right_hand: Option<(&'a str, &'a str)>,
    /// Hidden or invisible — the clerk won't respond; `unhide` first (Lich's
    /// `fput 'unhide' if hidden? or invisible?`).
    pub hidden: bool,
}

/// The shared open-sack → drop-expired-passes preamble both crossings run
/// first. One response-gated command at a time — the game's type-ahead buffer
/// only holds a few commands, so we never send the next until the previous
/// answered (or timed out; Lich's dothistimeout proceeds on timeout too).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Preamble {
    sack: Option<String>,
    expired: Vec<String>,
    stage: PreStage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreStage {
    Open { sent_ms: Option<u64> },
    Drop { i: usize, sent_ms: Option<u64> },
    Done,
}

impl Preamble {
    fn new(sack: Option<&str>, expired: Vec<String>) -> Self {
        Preamble {
            sack: sack.map(str::to_string),
            expired,
            stage: PreStage::Open { sent_ms: None },
        }
    }

    /// Advance one tick; true once the whole preamble has run.
    fn tick(&mut self, ctx: &BuyTick, out: &mut Vec<BuyEvent>) -> bool {
        loop {
            match &mut self.stage {
                PreStage::Open { sent_ms } => {
                    let Some(sack) = &self.sack else {
                        self.stage = PreStage::Drop { i: 0, sent_ms: None };
                        continue;
                    };
                    match *sent_ms {
                        None => {
                            if ctx.rt_remaining > 0.0 {
                                return false;
                            }
                            out.push(BuyEvent::Send(format!("open #{sack}")));
                            *sent_ms = Some(ctx.now_ms);
                            return false;
                        }
                        Some(sent) => {
                            let answered = ctx.feedback.contains(&F::ContainerOpened)
                                || ctx.feedback.contains(&F::ContainerAlreadyOpen);
                            if answered || ctx.now_ms.saturating_sub(sent) > RESP_TIMEOUT_MS {
                                self.stage = PreStage::Drop { i: 0, sent_ms: None };
                                continue;
                            }
                            return false;
                        }
                    }
                }
                PreStage::Drop { i, sent_ms } => {
                    if *i >= self.expired.len() {
                        self.stage = PreStage::Done;
                        continue;
                    }
                    match *sent_ms {
                        None => {
                            if ctx.rt_remaining > 0.0 {
                                return false;
                            }
                            out.push(BuyEvent::Send(format!("_drag #{} drop", self.expired[*i])));
                            *sent_ms = Some(ctx.now_ms);
                            return false;
                        }
                        Some(sent) => {
                            if ctx.feedback.contains(&F::ItemDropped)
                                || ctx.now_ms.saturating_sub(sent) > DROP_TIMEOUT_MS
                            {
                                *i += 1;
                                *sent_ms = None;
                                continue;
                            }
                            return false;
                        }
                    }
                }
                PreStage::Done => return true,
            }
        }
    }
}

/// The response-gated post-teleport put-back: `_drag #pass #sack`, wait for
/// the stow to land. Shared by both machines so the cleanup is one command
/// per game response (not flat-fired ahead of the stow confirmation). The
/// sack CLOSE stays with the executor (its already-open bookkeeping lives
/// there); this only covers the response-gated drag.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PutBack {
    pass_id: String,
    sack: Option<String>,
    sent_ms: Option<u64>,
}

impl PutBack {
    fn new(pass_id: String, sack: Option<String>) -> Self {
        PutBack { pass_id, sack, sent_ms: None }
    }

    /// Advance; true once the pass is back in the sack (or there's no sack to
    /// put it in — nothing to wait on).
    fn tick(&mut self, ctx: &BuyTick, out: &mut Vec<BuyEvent>) -> bool {
        let Some(sack) = self.sack.clone() else {
            return true;
        };
        match self.sent_ms {
            None => {
                if ctx.rt_remaining > 0.0 {
                    return false;
                }
                out.push(BuyEvent::Send(format!("_drag #{} #{sack}", self.pass_id)));
                self.sent_ms = Some(ctx.now_ms);
                false
            }
            Some(sent) => {
                ctx.feedback.contains(&F::ItemStowed)
                    || ctx.now_ms.saturating_sub(sent) > RESP_TIMEOUT_MS
            }
        }
    }
}

/// The response-driven USE machine (raise a pass already HELD in the cache):
/// preamble (open sack, drop expired) → get the pass → raise it. One command
/// per game response — never a flood into the type-ahead buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseState {
    pre: Preamble,
    pass_id: String,
    sack: Option<String>,
    phase: UsePhase,
    /// The RESOLVED room the raise was sent from. The teleport is confirmed
    /// only by the whirlwind line or the resolved room CHANGING off this —
    /// never by a bare nav against a stale room (the live phantom second-buy:
    /// concluding early re-planned from the departure room and routed into
    /// ANOTHER day-pass edge).
    raised_from: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UsePhase {
    /// Sent (or about to send) `get #pass`; waiting for the get to land.
    GetPass { sent_ms: Option<u64> },
    /// Sent `raise #pass`; waiting for the whirlwind or a failure.
    AwaitRaise { sent_ms: u64 },
    /// Teleported; putting the pass back (response-gated) before concluding.
    PutBack(PutBack),
}

impl UseState {
    pub fn new(sack: Option<&str>, pass_id: &str, expired: Vec<String>) -> Self {
        UseState {
            pre: Preamble::new(sack, expired),
            pass_id: pass_id.to_string(),
            sack: sack.map(str::to_string),
            phase: UsePhase::GetPass { sent_ms: None },
            raised_from: None,
        }
    }

    pub fn tick(&mut self, ctx: BuyTick) -> Vec<BuyEvent> {
        let mut out = Vec::new();
        if !self.pre.tick(&ctx, &mut out) {
            return out;
        }
        let saw = |e: &F| ctx.feedback.contains(e);
        match &mut self.phase {
            UsePhase::GetPass { sent_ms } => match *sent_ms {
                None => {
                    if ctx.rt_remaining <= 0.0 {
                        out.push(BuyEvent::Send(format!("get #{}", self.pass_id)));
                        *sent_ms = Some(ctx.now_ms);
                    }
                }
                Some(sent) => {
                    // On response — or timeout, like dothistimeout — raise.
                    // A truly failed get makes the raise answer "Raise what",
                    // which fails the crossing cleanly (RaiseWrongRoom).
                    if saw(&F::ItemGot) || ctx.now_ms.saturating_sub(sent) > RESP_TIMEOUT_MS {
                        out.push(BuyEvent::Send(format!("raise #{}", self.pass_id)));
                        self.raised_from = ctx.current_room;
                        self.phase = UsePhase::AwaitRaise { sent_ms: ctx.now_ms };
                    }
                }
            },
            UsePhase::AwaitRaise { sent_ms } => {
                // The teleport is proven by the whirlwind line, or by the
                // RESOLVED room changing off the raise room. A bare nav (or an
                // unresolved room) is NOT enough — concluding against a stale
                // room caused the live phantom second-buy.
                let moved = ctx.current_room.is_some() && ctx.current_room != self.raised_from;
                if saw(&F::RaiseTraveled) || moved {
                    // Teleported: put the pass back, response-gated, BEFORE
                    // signalling Traveled (no cleanup flood).
                    let mut put = PutBack::new(self.pass_id.clone(), self.sack.clone());
                    if put.tick(&ctx, &mut out) {
                        out.push(BuyEvent::Traveled {
                            pass_id: Some(self.pass_id.clone()),
                        });
                    } else {
                        self.phase = UsePhase::PutBack(put);
                    }
                } else if saw(&F::RaiseWrongRoom) {
                    out.push(BuyEvent::Failed(
                        "the pass raise was refused (wrong room or bad pass)".into(),
                    ));
                } else if ctx.now_ms.saturating_sub(*sent_ms) > RESP_TIMEOUT_MS {
                    out.push(BuyEvent::Failed("the pass raise didn't complete".into()));
                }
            }
            UsePhase::PutBack(put) => {
                if put.tick(&ctx, &mut out) {
                    out.push(BuyEvent::Traveled {
                        pass_id: Some(self.pass_id.clone()),
                    });
                }
            }
        }
        out
    }
}

/// The day-pass buy conversation, carried on the travel step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuyState {
    /// Open-sack/drop-expired preamble, run before the conversation.
    pre: Preamble,
    dep_room: u32,
    npc: &'static str,
    ask_word: &'static str,
    enter_move: &'static str,
    exit_move: &'static str,
    to_bank: &'static [&'static str],
    from_bank: &'static [&'static str],
    phase: Phase,
    /// The bought pass's exist-id, captured from the hand once it's handed over
    /// (`_drag`/`raise` need the id, not the `pass` noun).
    pass_id: Option<String>,
    /// A bank withdrawal already ran this conversation. A SECOND too-poor after
    /// funding fails instead of looping bank trips forever (Lich gives up and
    /// turns buy_day_pass off after one funded re-ask).
    funded: bool,
    /// The RESOLVED room the raise was sent from (see UseState::raised_from).
    raised_from: Option<u32>,
}

impl BuyState {
    pub fn new(
        dep: &DayPassDeparture,
        dest: &DayPassDestination,
        sack: Option<&str>,
        expired: Vec<String>,
    ) -> Self {
        BuyState {
            pre: Preamble::new(sack, expired),
            dep_room: dep.room,
            npc: dep.npc,
            ask_word: dest.ask_word,
            enter_move: dep.enter_move,
            exit_move: dep.exit_move,
            to_bank: dep.to_bank,
            from_bank: dep.from_bank,
            phase: Phase::ToClerk { sent_ms: None },
            pass_id: None,
            funded: false,
            raised_from: None,
        }
    }

    /// Capture the pass exist-id from whichever hand holds the `pass` noun.
    fn capture_pass(&mut self, ctx: &BuyTick) {
        for hand in [ctx.left_hand, ctx.right_hand].into_iter().flatten() {
            if hand.1 == "pass" {
                self.pass_id = Some(hand.0.to_string());
                return;
            }
        }
    }

    fn ask(&self) -> String {
        format!("ask {} for {}", self.npc, self.ask_word)
    }

    /// Advance one tick. Returns the commands/outcome for the executor.
    pub fn tick(&mut self, ctx: BuyTick) -> Vec<BuyEvent> {
        let mut out = Vec::new();
        // The open/drop preamble runs first, one response-gated command at a
        // time; the conversation starts once it's through.
        if !self.pre.tick(&ctx, &mut out) {
            return out;
        }
        // RT gate: don't send while in roundtime (except pure waits).
        let rt_clear = ctx.rt_remaining <= 0.0;
        let saw = |e: &F| ctx.feedback.contains(e);
        let timed_out = |sent_ms: u64| ctx.now_ms.saturating_sub(sent_ms) > RESP_TIMEOUT_MS;

        match &mut self.phase {
            Phase::ToClerk { sent_ms } => {
                let Some(sent) = *sent_ms else {
                    if !rt_clear {
                        return out;
                    }
                    out.push(BuyEvent::Send(self.enter_move.to_string()));
                    self.phase = Phase::ToClerk { sent_ms: Some(ctx.now_ms) };
                    return out;
                };
                // Wait to arrive (room RESOLVED off the departure room —
                // `None` is unresolved, not "left").
                if ctx.current_room.is_some() && ctx.current_room != Some(self.dep_room) {
                    if ctx.hidden {
                        out.push(BuyEvent::Send("unhide".into()));
                    }
                    out.push(BuyEvent::Send(self.ask()));
                    self.phase = Phase::AwaitOffer { sent_ms: ctx.now_ms };
                } else if timed_out(sent) {
                    out.push(BuyEvent::Failed("never reached the pass clerk".into()));
                }
                out
            }
            Phase::AwaitOffer { sent_ms } => {
                if saw(&F::DayPassInHand) {
                    // The clerk skipped the offer and handed the pass over -
                    // the post-bank re-ask does this (Lich's proc accepts
                    // both shapes). Waiting for an offer that never comes
                    // timed the whole trip out after a full bank round-trip.
                    self.capture_pass(&ctx);
                    self.phase = Phase::ToWaitingRoom { sent_ms: None, sent_from: None };
                    self.tick_to_waiting_room(&ctx, &mut out);
                } else if saw(&F::DayPassTooPoor) {
                    out.push(BuyEvent::FailedTooPoor(
                        "still too poor for the pass at the offer".into(),
                    ));
                } else if saw(&F::DayPassOffered) {
                    // Confirm the purchase.
                    out.push(BuyEvent::Send(self.ask()));
                    self.phase = Phase::AwaitPass { sent_ms: ctx.now_ms };
                } else if timed_out(*sent_ms) {
                    out.push(BuyEvent::Failed("the clerk never made an offer".into()));
                }
                out
            }
            Phase::AwaitPass { sent_ms } => {
                if saw(&F::DayPassInHand) {
                    self.capture_pass(&ctx);
                    self.phase = Phase::ToWaitingRoom { sent_ms: None, sent_from: None };
                    // fall through to send the look/step below next tick
                    self.tick_to_waiting_room(&ctx, &mut out);
                } else if saw(&F::DayPassTooPoor) {
                    if self.funded {
                        // Already withdrew once and it's STILL not enough —
                        // don't loop bank trips (Lich bails here too).
                        out.push(BuyEvent::FailedTooPoor(
                            "still too poor after withdrawing - the bank can't cover the pass".into(),
                        ));
                    } else if ctx.get_silvers {
                        self.funded = true;
                        self.phase = Phase::ToBank { i: 0, sent_from: None, sent_ms: ctx.now_ms };
                        self.tick_to_bank(&ctx, &mut out);
                    } else {
                        out.push(BuyEvent::FailedTooPoor(
                            "not enough silver for the pass and Get Silvers is off".into(),
                        ));
                    }
                } else if timed_out(*sent_ms) {
                    out.push(BuyEvent::Failed("no response to the pass purchase".into()));
                }
                out
            }
            Phase::ToBank { .. } => {
                self.tick_to_bank(&ctx, &mut out);
                out
            }
            Phase::AwaitWithdraw { sent_ms } => {
                if saw(&F::WithdrawOk) {
                    self.phase = Phase::FromBank { i: 0, sent_from: None, sent_ms: ctx.now_ms };
                    self.tick_from_bank(&ctx, &mut out);
                } else if timed_out(*sent_ms) {
                    out.push(BuyEvent::Failed("the withdrawal didn't confirm".into()));
                }
                out
            }
            Phase::FromBank { .. } => {
                self.tick_from_bank(&ctx, &mut out);
                out
            }
            Phase::ToWaitingRoom { .. } => {
                self.tick_to_waiting_room(&ctx, &mut out);
                out
            }
            Phase::AwaitRaise { sent_ms } => {
                // Teleport proven by the whirlwind line or the RESOLVED room
                // changing off the raise room — never a bare nav vs a stale
                // room (see UseState::AwaitRaise).
                let moved =
                    ctx.current_room.is_some() && ctx.current_room != self.raised_from;
                if saw(&F::RaiseTraveled) || moved {
                    // Put the bought pass back (response-gated) before
                    // Traveled — but only when we captured its id; without an
                    // id there's nothing to drag by.
                    match &self.pass_id {
                        Some(id) => {
                            let mut put =
                                PutBack::new(id.clone(), self.pre.sack.clone());
                            if put.tick(&ctx, &mut out) {
                                out.push(BuyEvent::Traveled {
                                    pass_id: self.pass_id.clone(),
                                });
                            } else {
                                self.phase = Phase::PutBack(put);
                            }
                        }
                        None => {
                            out.push(BuyEvent::Traveled { pass_id: None });
                        }
                    }
                } else if saw(&F::RaiseWrongRoom) {
                    out.push(BuyEvent::Failed(
                        "couldn't raise the pass here (not the Chronomage waiting room)".into(),
                    ));
                } else if timed_out(*sent_ms) {
                    out.push(BuyEvent::Failed("the pass raise didn't complete".into()));
                }
                out
            }
            Phase::PutBack(put) => {
                if put.tick(&ctx, &mut out) {
                    let pass_id = self.pass_id.clone();
                    out.push(BuyEvent::Traveled { pass_id });
                }
                out
            }
        }
    }

    /// Paced walk to the bank, then `withdraw 5000`.
    fn tick_to_bank(&mut self, ctx: &BuyTick, out: &mut Vec<BuyEvent>) {
        let Phase::ToBank { i, sent_from, sent_ms } = &mut self.phase else {
            return;
        };
        if ctx.rt_remaining > 0.0 {
            return;
        }
        // Waiting for the previous step to land?
        if let Some(from) = *sent_from {
            if ctx.current_room == Some(from) && ctx.now_ms.saturating_sub(*sent_ms) <= RESP_TIMEOUT_MS {
                return; // not landed yet
            }
        }
        if *i >= self.to_bank.len() {
            // Arrived at the bank — withdraw.
            out.push(BuyEvent::Send("withdraw 5000".into()));
            self.phase = Phase::AwaitWithdraw { sent_ms: ctx.now_ms };
            return;
        }
        out.push(BuyEvent::Send(self.to_bank[*i].to_string()));
        *i += 1;
        *sent_from = ctx.current_room;
        *sent_ms = ctx.now_ms;
    }

    /// Paced walk back from the bank, then re-ask (offer → confirm again).
    fn tick_from_bank(&mut self, ctx: &BuyTick, out: &mut Vec<BuyEvent>) {
        let Phase::FromBank { i, sent_from, sent_ms } = &mut self.phase else {
            return;
        };
        if ctx.rt_remaining > 0.0 {
            return;
        }
        if let Some(from) = *sent_from {
            if ctx.current_room == Some(from) && ctx.now_ms.saturating_sub(*sent_ms) <= RESP_TIMEOUT_MS {
                return;
            }
        }
        if *i >= self.from_bank.len() {
            // Back at the clerk — re-ask (loop through the offer→confirm
            // again). Unhide first if needed (Lich unhides after the bank
            // return too).
            if ctx.hidden {
                out.push(BuyEvent::Send("unhide".into()));
            }
            out.push(BuyEvent::Send(self.ask()));
            self.phase = Phase::AwaitOffer { sent_ms: ctx.now_ms };
            return;
        }
        out.push(BuyEvent::Send(self.from_bank[*i].to_string()));
        *i += 1;
        *sent_from = ctx.current_room;
        *sent_ms = ctx.now_ms;
    }

    /// Have the pass: `look` it (registers expiry) then step back to the
    /// waiting room; once there, tell the executor it's ready to raise.
    fn tick_to_waiting_room(&mut self, ctx: &BuyTick, out: &mut Vec<BuyEvent>) {
        let Phase::ToWaitingRoom { sent_ms, sent_from } = &mut self.phase else {
            return;
        };
        let Some(sent) = *sent_ms else {
            if ctx.rt_remaining > 0.0 {
                return;
            }
            // Look at the pass (registers expiry) by id if we have it.
            let look = match &self.pass_id {
                Some(id) => format!("look #{id}"),
                None => "look pass".to_string(),
            };
            out.push(BuyEvent::Send(look));
            out.push(BuyEvent::Send(self.exit_move.to_string()));
            let from = ctx.current_room;
            self.phase = Phase::ToWaitingRoom { sent_ms: Some(ctx.now_ms), sent_from: from };
            return;
        };
        // Arrived back at the waiting room (room RESOLVED off the clerk room —
        // `None` is unresolved, not "arrived") → raise the pass to travel. Use
        // the captured id (the `pass` noun works for `raise` but the id is
        // safer).
        if ctx.current_room.is_some() && ctx.current_room != *sent_from {
            let raise = match &self.pass_id {
                Some(id) => format!("raise #{id}"),
                None => "raise pass".to_string(),
            };
            out.push(BuyEvent::Send(raise));
            self.raised_from = ctx.current_room;
            self.phase = Phase::AwaitRaise { sent_ms: ctx.now_ms };
        } else if ctx.now_ms.saturating_sub(sent) > RESP_TIMEOUT_MS {
            out.push(BuyEvent::Failed(
                "never made it back to the Chronomage waiting room".into(),
            ));
        }
    }
}
