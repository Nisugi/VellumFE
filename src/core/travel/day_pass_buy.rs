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
    /// Give up (too poor with no funding, or a timeout). Carries a reason.
    Failed(String),
}

/// The buy conversation's phases.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    /// Just started — step from the waiting room to the clerk.
    ToClerk { sent: bool },
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
    ToWaitingRoom { sent: bool, sent_from: Option<u32> },
    /// Sent `raise pass`; waiting for the whirlwind (travelled) or a
    /// wrong-room/expired failure.
    AwaitRaise { sent_ms: u64 },
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
}

/// The day-pass buy conversation, carried on the travel step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuyState {
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
}

impl BuyState {
    pub fn new(dep: &DayPassDeparture, dest: &DayPassDestination) -> Self {
        BuyState {
            dep_room: dep.room,
            npc: dep.npc,
            ask_word: dest.ask_word,
            enter_move: dep.enter_move,
            exit_move: dep.exit_move,
            to_bank: dep.to_bank,
            from_bank: dep.from_bank,
            phase: Phase::ToClerk { sent: false },
            pass_id: None,
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
        // RT gate: don't send while in roundtime (except pure waits).
        let rt_clear = ctx.rt_remaining <= 0.0;
        let saw = |e: &F| ctx.feedback.contains(e);
        let timed_out = |sent_ms: u64| ctx.now_ms.saturating_sub(sent_ms) > RESP_TIMEOUT_MS;

        match &mut self.phase {
            Phase::ToClerk { sent } => {
                if !*sent {
                    if !rt_clear {
                        return out;
                    }
                    out.push(BuyEvent::Send(self.enter_move.to_string()));
                    *sent = true;
                    return out;
                }
                // Wait to arrive (room changed off the departure room).
                if ctx.current_room != Some(self.dep_room) {
                    out.push(BuyEvent::Send(self.ask()));
                    self.phase = Phase::AwaitOffer { sent_ms: ctx.now_ms };
                }
                out
            }
            Phase::AwaitOffer { sent_ms } => {
                if saw(&F::DayPassOffered) {
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
                    self.phase = Phase::ToWaitingRoom { sent: false, sent_from: None };
                    // fall through to send the look/step below next tick
                    self.tick_to_waiting_room(&ctx, &mut out);
                } else if saw(&F::DayPassTooPoor) {
                    if ctx.get_silvers {
                        self.phase = Phase::ToBank { i: 0, sent_from: None, sent_ms: ctx.now_ms };
                        self.tick_to_bank(&ctx, &mut out);
                    } else {
                        out.push(BuyEvent::Failed(
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
                if saw(&F::RaiseTraveled) || matches!(ctx.current_room, Some(_) if saw(&F::NavArrived)) {
                    out.push(BuyEvent::Traveled { pass_id: self.pass_id.clone() });
                } else if saw(&F::RaiseWrongRoom) {
                    out.push(BuyEvent::Failed(
                        "couldn't raise the pass here (not the Chronomage waiting room)".into(),
                    ));
                } else if timed_out(*sent_ms) {
                    out.push(BuyEvent::Failed("the pass raise didn't complete".into()));
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
            // Back at the clerk — re-ask (loop through the offer→confirm again).
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
        let Phase::ToWaitingRoom { sent, sent_from } = &mut self.phase else {
            return;
        };
        if !*sent {
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
            *sent = true;
            *sent_from = ctx.current_room;
            return;
        }
        // Arrived back at the waiting room → raise the pass to travel. Use the
        // captured id (the `pass` noun works for `raise` but the id is safer).
        if ctx.current_room != *sent_from {
            let raise = match &self.pass_id {
                Some(id) => format!("raise #{id}"),
                None => "raise pass".to_string(),
            };
            out.push(BuyEvent::Send(raise));
            self.phase = Phase::AwaitRaise { sent_ms: ctx.now_ms };
        }
    }
}
