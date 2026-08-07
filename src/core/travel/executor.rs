//! The walk loop — go2.lic's executor (lines ~2311–2505) as an event-driven
//! state machine. No sleeps, no blocking: the frontend ticks it with a
//! snapshot of the world (`TravelContext`) and it answers with commands to
//! send and messages to show.
//!
//! Ported behavior, one room at a time (typeahead pipelining is go2's most
//! fragile code and deliberately v1-out):
//! - Dead aborts. Stunned/webbed waits (go2's `muckled?` gate).
//! - Not standing (and the edge isn't swim/pedal): wait out RT, `stand`,
//!   re-check; repeated failures abort.
//! - Wait out RT, send the edge command, await the expected room.
//! - A step that times out retries; repeated failure on the same edge
//!   disables that edge for the session and re-paths — go2's
//!   "changing timeto to nil" + `$go2_restart` loop.
//! - Ending up in an unexpected (but mapped) room re-paths from there.

use std::collections::HashSet;

use crate::core::mapdb::MapDb;
use crate::core::pathing;

/// What the executor sees each tick. `now_ms` is any monotonic clock the
/// caller keeps (tests drive it by hand).
#[derive(Clone, Copy)]
pub struct TravelContext<'a> {
    pub db: &'a MapDb,
    /// Resolved mapdb room id (holds the last known room while unresolved).
    pub current_room: Option<u32>,
    pub dead: bool,
    /// Stunned or webbed — go2's `muckled?` gate, minus states VellumFE
    /// doesn't track yet (bound, sleeping).
    pub muckled: bool,
    pub standing: bool,
    pub sitting: bool,
    pub kneeling: bool,
    /// Active spell numbers, for scripted-edge `checkspell(N)` branches.
    pub active_spells: &'a [u16],
    /// Roundtime remaining in seconds (0 when free).
    pub rt_remaining: f64,
    pub now_ms: u64,
    /// Personal maze routes by maze name (config.go2.pathcodes) — the maze
    /// strategy reads these; capture writes them outside the executor.
    pub pathcodes: &'a std::collections::BTreeMap<String, Vec<String>>,
    /// Inputs for the hands stow/retrieve cascade (EmptyHands/FillHands
    /// actions). `None` when the caller doesn't supply them (tests that don't
    /// exercise hands) — the actions then fall back to a bare stow/get.
    pub hands: Option<StashInputs<'a>>,
    /// Move-feedback events since the last tick (drained from GameState) —
    /// nav arrivals + recovery signals. Empty when idle (§09/§12).
    pub feedback: &'a [crate::core::move_feedback::MoveFeedback],
    /// When native travel reaches an edge it can't cross, hand off to Lich's
    /// `;go2 <dest>` instead of banning + re-pathing. Only true on a Lich
    /// connection with the setting enabled (the caller gates on direct-mode).
    pub lich_fallback: bool,
    /// Silver-funding inputs for paid travel (portmaster/day-pass). `None`
    /// when the caller doesn't supply them.
    pub funding: Option<FundingInputs>,
    /// True at the Pinefar Depository, whose withdraw uses `ask banker for`
    /// with a 20-silver minimum (go2's special case).
    pub at_pinefar_depository: bool,
    /// Live compass exits from the current room (`XMLData.room_exits`) — the
    /// Confluence explorer's only view of the shifting maze.
    pub compass_dirs: &'a [String],
    /// Ground-loot nouns in the current room — the Confluence explorer scans
    /// these for the tranquility point / pit landmarks (`GameObj.loot`).
    pub loot_nouns: &'a [String],
    /// Chronomage day-pass crossing inputs, when the planned edge is a day-pass
    /// edge and the caller supplies them (`None` otherwise).
    pub day_pass: Option<DayPassInputs<'a>>,
}

/// What the day-pass crossing needs from live state. The specific pass id and
/// buy-permission are computed per-edge in `begin_day_pass` from the pair, so
/// this stays edge-agnostic: the sack id, the buy config + funding flag, the
/// live cache, and `now` for expiry checks.
#[derive(Clone, Copy)]
pub struct DayPassInputs<'a> {
    /// The resolved `day_pass_sack` container command-id, if found.
    pub sack_id: Option<&'a str>,
    /// The `buy_day_pass` config value (on/off/pair-list).
    pub buy_day_pass: &'a str,
    /// Whether Get Silvers is on (required to fund a buy shortfall).
    pub get_silvers: bool,
    /// The live day-pass cache (for the held-pass lookup by pair).
    pub cache: &'a crate::core::day_pass::DayPassCache,
    /// Current Unix time for expiry checks.
    pub now_epoch: i64,
}

/// Inputs for the silver-funding pre-flight, from GameState + config.
#[derive(Clone, Copy)]
pub struct FundingInputs {
    /// Silver on hand (`game_state.silver`); None until a wealth line is seen.
    pub silver: Option<u64>,
    /// Permission to withdraw from the bank when short (`go2.get_silvers`).
    pub get_silvers: bool,
    /// Also pre-fund the return trip (`go2.get_return_trip_silvers`).
    pub get_return_trip: bool,
}

/// The world snapshot the hands stow cascade needs, threaded through
/// `TravelContext`. Assembled by the caller from `GameState` each tick.
#[derive(Clone, Copy)]
pub struct StashInputs<'a> {
    pub left_hand: Option<&'a crate::core::game_objects::GameItem>,
    pub right_hand: Option<&'a crate::core::game_objects::GameItem>,
    pub ready_stow: &'a crate::core::game_objects::ReadyStow,
    pub weaponsack: Option<&'a str>,
    pub lootsack: Option<&'a str>,
    pub other_containers: &'a [String],
    pub left_bandolier: Option<&'a str>,
    pub right_bandolier: Option<&'a str>,
    pub left_is_weapon: bool,
    pub right_is_weapon: bool,
}

impl<'a> StashInputs<'a> {
    fn to_stash_context(self, now_ms: u64) -> super::stash::StashContext<'a> {
        super::stash::StashContext {
            left_hand: self.left_hand,
            right_hand: self.right_hand,
            ready_stow: self.ready_stow,
            weaponsack: self.weaponsack,
            lootsack: self.lootsack,
            other_containers: self.other_containers,
            left_bandolier: self.left_bandolier,
            right_bandolier: self.right_bandolier,
            left_is_weapon: self.left_is_weapon,
            right_is_weapon: self.right_is_weapon,
            now_ms,
        }
    }
}

impl TravelContext<'_> {
    fn eval(&self, cond: crate::core::pathing::edge::Cond) -> bool {
        use crate::core::pathing::edge::Cond;
        match cond {
            Cond::SpellActive(n) => self.active_spells.contains(&n),
            Cond::Sitting => self.sitting,
            Cond::Kneeling => self.kneeling,
        }
    }

    fn saw(&self, event: &crate::core::move_feedback::MoveFeedback) -> bool {
        self.feedback.contains(event)
    }
}

/// What a tick produced, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum TravelEvent {
    /// Send this command to the game.
    Send(String),
    /// Show this to the user.
    Status(String),
    /// Trip finished (rooms actually traversed, wall seconds).
    Arrived { destination: u32, seconds: f64 },
    /// Trip abandoned.
    Failed(String),
    /// Native travel can't cross an edge; hand the whole trip to Lich's
    /// `;go2 <destination>` (the P6 fallback bandaid). The owner sends it and
    /// drops the native task.
    LichFallback { destination: u32 },
}

/// Waiting-for-what within the current step.
#[derive(Debug, Clone, PartialEq)]
enum Step {
    /// Pre-flight checks (muckled/stand/RT), then send the move.
    Prepare,
    /// `stand` was sent; waiting to be upright.
    AwaitStand { sent_ms: u64, attempts: u32 },
    /// A scripted edge's actions are running (transpiled StringProc).
    RunScript {
        actions: Vec<crate::core::pathing::edge::WalkAction>,
        pc: usize,
        /// Wake time for an in-progress `Sleep`.
        sleep_until: Option<u64>,
        expected: u32,
        from: u32,
    },
    /// The hands stow/retrieve cascade (an EmptyHands/FillHands action) is
    /// running via the StashService (held on the task). When it finishes we
    /// return to `resume` — the script step that requested it — and continue
    /// the edge. This is the §11 suspend/resume seam, same shape as
    /// AwaitStand suspending for a `stand`.
    Stashing { resume: Box<Step> },
    /// The silver-funding pre-flight (paid travel). Runs before the walk:
    /// check the trip cost, `wealth quiet`, and if short + permitted, redirect
    /// to a bank, withdraw, then re-plan to the real destination.
    Funding(FundingPhase),
    /// A move was sent; waiting to arrive in `expected`.
    AwaitArrival {
        expected: u32,
        /// Room the move was sent from — still being here just means the
        /// move hasn't landed; anywhere else is off-route.
        from: u32,
        sent_ms: u64,
        /// A "slow" crossing — an urchin guide, portmaster escort, or other
        /// pass-through whose confirmation can take many seconds (especially in
        /// a busy room). It gets a longer arrival window so a slow-but-fine
        /// crossing isn't re-sent (the live "urchin guide bank" double-send).
        slow: bool,
    },
    /// Walking a curated maze by personal pathcode (movement inside is
    /// scrambled; edges are never stepped normally). See travel::mazes.
    Maze {
        maze_name: String,
        phase: MazePhase,
        route: Vec<String>,
        /// Next route command to send.
        i: usize,
        /// Recovery cycles used (search-and-restart).
        attempts: u32,
        /// The trip's destination is itself inside the maze: finish at the
        /// far side instead of re-pathing back into the scramble.
        dest_inside: bool,
    },
    /// Exploring the Plane of Elemental Confluence — a shifting maze with no
    /// fixed graph. Learns adjacency live and re-derives a route each step
    /// toward the tranquility exit portal. See travel::confluence.
    Confluence {
        /// Awaiting arrival after a walk: the room we moved FROM (still being
        /// there means the move hasn't landed) and the direction we sent, so
        /// on arrival we can record `learned[from][dir] = arrived`.
        pending: Option<ConfluencePending>,
    },
    /// A scripted-edge `StepMove` is in flight: a paced walk command was sent
    /// mid-script and we're waiting for the room to change before resuming the
    /// script at `pc`. `sent_from` is the room it was sent in (any other room
    /// means it landed). Used by the day-pass buy walk.
    ScriptWalk {
        actions: Vec<crate::core::pathing::edge::WalkAction>,
        pc: usize,
        expected: u32,
        from: u32,
        sent_from: u32,
        sent_ms: u64,
    },
}

/// A Confluence move in flight: where it was sent from and which direction, so
/// the learned graph can be updated when we land.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfluencePending {
    from: u32,
    dir: String,
    sent_ms: u64,
}

/// The silver-funding pre-flight state (Step::Funding). Ported from go2.lic's
/// bank-withdraw routine (2210-2293).
#[derive(Debug, Clone, PartialEq)]
enum FundingPhase {
    /// `wealth quiet` sent; waiting for `game_state.silver` to update.
    AwaitWealth { sent_ms: u64 },
    /// Walking to the chosen bank (the task's path was redirected there).
    /// `real_dest` is the destination to re-plan to once funded.
    RoutingToBank { real_dest: u32, need: u64 },
    /// `withdraw`/`ask banker for` sent at the bank; waiting for the silver to
    /// reflect the withdrawal, then re-plan to `real_dest`.
    AwaitWithdraw { real_dest: u32, need: u64, sent_ms: u64 },
}

#[derive(Debug, Clone, PartialEq)]
enum MazePhase {
    /// The ask command was sent; waiting for the capture layer to store the
    /// spoken route.
    AwaitCode { sent_ms: u64 },
    /// Moving from the NPC entrance to the route's start room.
    ToStart { sent_ms: u64 },
    /// Sending route commands. Paced by the room CHANGING after each send
    /// (every maze move lands somewhere, even mid-scramble), with the timer
    /// only as a fallback — so the walk runs at type-ahead speed.
    Walk {
        wait_until: u64,
        /// Room the last command was sent from; a different current room
        /// means it landed.
        sent_from: Option<u32>,
    },
    /// Route exhausted; judging the landing room (early on room data, timer
    /// as fallback).
    Verify { until: u64 },
    /// `search` sent after a failed walk; waiting to re-orient.
    PostSearch { until: u64 },
}

/// How long a move may take before it counts as failed. Generous: RT from
/// the move itself plus lag both land inside this window.
const STEP_TIMEOUT_MS: u64 = 8_000;
/// Slow crossings (urchin guide, portmaster escort, pass-through) can take many
/// seconds to confirm in a busy room; they get a longer arrival window so a
/// slow-but-fine crossing isn't re-sent.
const SLOW_ARRIVAL_TIMEOUT_MS: u64 = 30_000;
/// `stand` gets a shorter window (go2 uses dothistimeout 2s).
const STAND_TIMEOUT_MS: u64 = 2_500;
const MAX_STAND_ATTEMPTS: u32 = 5;
/// Same-edge failures before the edge is disabled for the session.
const MAX_EDGE_RETRIES: u32 = 2;
/// Re-path budget — a trip that restarts this often is going nowhere.
const MAX_RESTARTS: u32 = 10;
/// How long the maze NPC gets to speak a route before the walk gives up.
const MAZE_ASK_TIMEOUT_MS: u64 = 12_000;
/// Gap between maze route commands beyond waiting out RT.
const MAZE_STEP_GAP_MS: u64 = 1_400;
/// Settle time after the last route command (or a `search`) before the
/// landing room is judged.
const MAZE_SETTLE_MS: u64 = 2_500;
/// Search-and-restart cycles before the maze walk is abandoned.
const MAZE_MAX_ATTEMPTS: u32 = 3;

#[derive(Debug)]
pub struct TravelTask {
    pub destination: u32,
    /// Rooms to traverse, excluding the current room, including the
    /// destination (Lich `path_to` shape).
    path: Vec<u32>,
    /// Next entry in `path` to move into.
    idx: usize,
    step: Step,
    /// Edges disabled for this session after repeated failures.
    banned: HashSet<(u32, u32)>,
    /// Failures on the current edge (reset on arrival and re-path).
    edge_retries: u32,
    restarts: u32,
    started_ms: u64,
    /// Set once while waiting out a muckled state so the status line doesn't
    /// repeat every tick.
    muckle_announced: bool,
    /// The running hands stow/retrieve task (Step::Stashing). Held here rather
    /// than in the Step enum so Step stays Clone+PartialEq.
    stash: Option<super::stash::StashTask>,
    /// The LIFO stow stack carried from an EmptyHands to its later FillHands
    /// on the same edge (Lich's $fill_hands_actions).
    stash_stack: Vec<super::stash::Stowed>,
    /// The silver the trip needs (0 = free). Set at start; drives funding.
    silver_need: u64,
    /// True while walking the funding detour to a bank (so arrival there
    /// triggers the withdraw rather than a normal arrival).
    funding_bank: Option<u32>,
    /// The live-learned Confluence map (Step::Confluence). Held outside the
    /// Step enum so Step stays Clone+PartialEq, same as `stash`. `Some` only
    /// while inside the Plane.
    confluence: Option<super::confluence::ConfluenceState>,
    /// The silver reading at the moment a bank withdrawal was sent. The
    /// withdraw confirmation isn't a wealth line, so we re-probe `wealth quiet`
    /// and wait for `game_state.silver` to change from this value (proving the
    /// fresh reading landed) before judging whether the withdrawal covered the
    /// trip.
    silver_at_withdraw: Option<u64>,
    /// True while riding a `meta:transport` room (a portmaster ship, ferry,
    /// caravan). We wait silently aboard and re-plan the route the moment the
    /// ride drops us in a normal room.
    in_transport: bool,
}

impl TravelTask {
    /// Plan a trip. Fails when there's no route (or we're already there —
    /// callers check that first for a friendlier message).
    pub fn start(
        db: &MapDb,
        from: u32,
        destination: u32,
        now_ms: u64,
    ) -> Result<TravelTask, String> {
        let path = pathing::path_to(db, from, destination)
            .or_else(|| Self::plan_via_maze(db, from, destination))
            .ok_or_else(|| {
                format!("no route from room {from} to {destination} (see .room for how this room resolved)")
            })?;
        // Silver the trip needs (from silver-cost path tags). If non-zero, the
        // funding pre-flight runs before the walk.
        let full_path: Vec<u32> = std::iter::once(from).chain(path.iter().copied()).collect();
        let silver_need = pathing::silver_cost(db, &full_path);
        let step = if silver_need > 0 {
            Step::Funding(FundingPhase::AwaitWealth { sent_ms: now_ms })
        } else {
            Step::Prepare
        };
        Ok(TravelTask {
            destination,
            path,
            idx: 0,
            step,
            banned: HashSet::new(),
            edge_retries: 0,
            restarts: 0,
            started_ms: now_ms,
            muckle_announced: false,
            stash: None,
            stash_stack: Vec::new(),
            silver_need,
            funding_bank: None,
            confluence: None,
            silver_at_withdraw: None,
            in_transport: false,
        })
    }

    /// Destinations inside or behind a curated maze usually have no graph
    /// route (the maze's edges are scramble junk that often doesn't reach
    /// the far side at all). Plan to the maze's entrance instead and end
    /// the path on its start room — the boundary interception takes over
    /// there, and the far side re-paths normally after the walk.
    fn plan_via_maze(db: &MapDb, from: u32, destination: u32) -> Option<Vec<u32>> {
        for maze in super::mazes::all() {
            let behind = maze.rooms.contains(&destination)
                || destination == maze.inside
                || pathing::path_to(db, maze.inside, destination).is_some();
            if !behind {
                continue;
            }
            let to_entrance = if from == maze.entrance {
                Some(Vec::new())
            } else {
                pathing::path_to(db, from, maze.entrance)
            };
            if let Some(mut path) = to_entrance {
                path.push(maze.start);
                return Some(path);
            }
        }
        None
    }

    /// Estimated seconds for the remaining route (display only).
    pub fn eta_seconds(&self, db: &MapDb, current: u32) -> f64 {
        let mut rooms = vec![current];
        rooms.extend(&self.path[self.idx.min(self.path.len())..]);
        pathing::estimate_time(db, &rooms)
    }

    pub fn rooms_remaining(&self) -> usize {
        self.path.len().saturating_sub(self.idx)
    }

    pub fn rooms_total(&self) -> usize {
        self.path.len()
    }

    /// Advance the state machine. Returns events in order; `Arrived` or
    /// `Failed` is always the last event of a finished task, and the caller
    /// drops the task after either.
    pub fn tick(&mut self, ctx: TravelContext) -> Vec<TravelEvent> {
        let mut events = Vec::new();

        if ctx.dead {
            events.push(TravelEvent::Failed("you're dead - travel aborted".into()));
            return events;
        }
        let Some(current) = ctx.current_room else {
            // Unresolved (unmapped room / db still loading): hold.
            return events;
        };
        // Arrival at the real destination ends the trip — UNLESS we're on a
        // funding detour (the destination stays the real one; the bank is a
        // separate waypoint), OR a hands stow/retrieve cycle is still running.
        // The final edge often IS `empty_hands; move; fill_hands`: the move
        // lands us at the destination while fill_hands is mid-retrieval, so
        // ending the trip here would abandon the refill and leave items in the
        // pack (the live footpath bug — fill_hands got only 1 of 2 items).
        // Let the Stashing step finish; it resumes and re-checks arrival.
        if current == self.destination && self.funding_bank.is_none() && self.stash.is_none() {
            events.push(TravelEvent::Arrived {
                destination: self.destination,
                seconds: (ctx.now_ms.saturating_sub(self.started_ms)) as f64 / 1000.0,
            });
            return events;
        }

        match self.step.clone() {
            Step::Prepare => self.tick_prepare(current, ctx, &mut events),
            Step::Maze {
                maze_name,
                phase,
                route,
                i,
                attempts,
                dest_inside,
            } => {
                self.tick_maze(
                    maze_name,
                    phase,
                    route,
                    i,
                    attempts,
                    dest_inside,
                    current,
                    ctx,
                    &mut events,
                );
            }
            Step::Confluence { pending } => {
                self.tick_confluence(pending, current, ctx, &mut events);
            }
            Step::AwaitStand { sent_ms, attempts } => {
                if ctx.standing {
                    self.step = Step::Prepare;
                    self.tick_prepare(current, ctx, &mut events);
                } else if ctx.now_ms.saturating_sub(sent_ms) > STAND_TIMEOUT_MS {
                    if attempts >= MAX_STAND_ATTEMPTS {
                        events.push(TravelEvent::Failed(
                            "can't stand up - travel aborted".into(),
                        ));
                    } else if ctx.rt_remaining <= 0.0 {
                        events.push(TravelEvent::Send("stand".into()));
                        self.step = Step::AwaitStand {
                            sent_ms: ctx.now_ms,
                            attempts: attempts + 1,
                        };
                    }
                }
            }
            Step::Stashing { resume } => {
                self.tick_stashing(resume, current, ctx, &mut events);
            }
            Step::Funding(phase) => {
                self.tick_funding(phase, current, ctx, &mut events);
            }
            Step::ScriptWalk {
                actions,
                pc,
                expected,
                from,
                sent_from,
                sent_ms,
            } => {
                // Paced walk step in flight. When the room changes off
                // `sent_from`, resume the script at `pc`. A generous timeout
                // re-sends by resuming (the next StepMove/Move handles it).
                if current != sent_from
                    || ctx.now_ms.saturating_sub(sent_ms) > SLOW_ARRIVAL_TIMEOUT_MS
                {
                    self.tick_script(actions, pc, None, expected, from, ctx, &mut events);
                } else {
                    self.step = Step::ScriptWalk {
                        actions,
                        pc,
                        expected,
                        from,
                        sent_from,
                        sent_ms,
                    };
                }
            }
            Step::RunScript {
                actions,
                pc,
                sleep_until,
                expected,
                from,
            } => {
                // A scripted edge can land the room change before its
                // actions finish (multi-command edges): arrival wins.
                if current == expected {
                    self.arrive();
                    return events;
                }
                if current != from {
                    events.push(TravelEvent::Status(format!(
                        "off the planned route (room {current}) - re-pathing"
                    )));
                    self.repath(ctx.db, current, &mut events);
                    return events;
                }
                self.tick_script(actions, pc, sleep_until, expected, from, ctx, &mut events);
            }
            Step::AwaitArrival {
                expected,
                from,
                sent_ms,
                slow,
            } => {
                if current == expected {
                    // Arrived on schedule; next step (or the destination
                    // check next tick).
                    self.in_transport = false;
                    self.arrive();
                    return events;
                }
                // A vehicle is carrying us. `meta:transport` rooms (portmaster
                // ships, ferries, caravans) move you on the game's own schedule
                // and have no walkable exits — so wait aboard silently and, the
                // moment the ride drops us in a normal room, re-plan the route
                // to the destination from wherever we landed (Nisugi's model).
                let on_transport = ctx
                    .current_room
                    .and_then(|r| ctx.db.room(r))
                    .is_some_and(|r| r.is_transport());
                if on_transport {
                    if !self.in_transport {
                        events.push(TravelEvent::Status(
                            "aboard a transport - waiting for the ride to finish".into(),
                        ));
                        self.in_transport = true;
                    }
                    return events;
                }
                if self.in_transport {
                    // The ride ended in a normal room (not the planned edge's
                    // `expected`, which we already checked). Re-plan from here.
                    self.in_transport = false;
                    events.push(TravelEvent::Status(format!(
                        "off the transport (room {current}) - re-pathing to the destination"
                    )));
                    self.repath(ctx.db, current, &mut events);
                    return events;
                }
                if current != from {
                    // A slow escort that walks you through NON-transport
                    // intermediate rooms (rare): keep waiting for it to finish
                    // rather than re-pathing mid-escort. A non-slow move ending
                    // up elsewhere IS off-route (fled, hand-moved).
                    if slow {
                        return events;
                    }
                    events.push(TravelEvent::Status(format!(
                        "off the planned route (room {current}) - re-pathing"
                    )));
                    self.repath(ctx.db, current, &mut events);
                    return events;
                }
                // Recovery from move feedback (Lich's `move` retry loop). Only
                // when we're still in `from` (the move hasn't landed).
                if self.recover_from_feedback(from, ctx, &mut events) {
                    return events;
                }
                // A nav fired but the room is UNRESOLVED (an UID-less room the
                // resolver couldn't place): a nav means we moved, so trust the
                // planned edge and treat it as arrival at `expected` (§12) —
                // rather than timing out and wrongly banning a good edge. We
                // only do this when current_room is None (truly unresolved),
                // never when it still reads `from` (that's a failed move, or a
                // same-room nav, handled by the timeout path).
                use crate::core::move_feedback::MoveFeedback;
                if ctx.saw(&MoveFeedback::NavArrived) && ctx.current_room.is_none() {
                    self.arrive();
                    return events;
                }
                let timeout = if slow { SLOW_ARRIVAL_TIMEOUT_MS } else { STEP_TIMEOUT_MS };
                if ctx.now_ms.saturating_sub(sent_ms) > timeout {
                    if self.edge_retries >= MAX_EDGE_RETRIES {
                        // go2: "changing Room[..].timeto[..] to nil" + restart.
                        events.push(TravelEvent::Status(format!(
                            "move {from} -> {expected} keeps failing - disabling that edge for this session and re-pathing"
                        )));
                        self.banned.insert((from, expected));
                        self.repath(ctx.db, current, &mut events);
                    } else {
                        // Retry the same edge (a scripted edge replays its
                        // whole action sequence).
                        self.edge_retries += 1;
                        self.step = Step::Prepare;
                        self.tick_prepare(current, ctx, &mut events);
                    }
                }
            }
        }
        events
    }

    /// The expected room arrived: advance the route.
    fn arrive(&mut self) {
        self.idx += 1;
        self.edge_retries = 0;
        // On a funding detour, reaching the bank returns to the funding phase
        // (to withdraw) rather than continuing as a normal walk.
        if let Some(bank) = self.funding_bank {
            if self.idx >= self.path.len() {
                // Path to the bank is done — hand back to funding. real_dest /
                // need are recovered from silver_need + the stored destination.
                self.step = Step::Funding(FundingPhase::RoutingToBank {
                    real_dest: self.destination,
                    need: self.silver_need,
                });
                let _ = bank;
                return;
            }
        }
        self.step = Step::Prepare;
    }

    /// Enter maze mode at its boundary. With a stored pathcode the walk
    /// starts immediately; without one the NPC is asked and the capture
    /// layer fills the store (polled by AwaitCode).
    fn begin_maze(
        &mut self,
        maze: &super::mazes::MazeDef,
        current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        if current != maze.entrance && current != maze.start {
            // Approaching from an unsupported side (e.g. leaving the guild
            // outward). v1 walks inbound only.
            events.push(TravelEvent::Failed(format!(
                "the route crosses the {} maze from a side the walker doesn't support yet - walk it manually",
                maze.name
            )));
            return;
        }
        let dest_inside = maze.rooms.contains(&self.destination);
        if dest_inside {
            events.push(TravelEvent::Status(format!(
                "destination is inside the {} maze - walking the pathcode through it",
                maze.name
            )));
        }
        let (phase, route) = match ctx.pathcodes.get(&maze.name) {
            Some(route) => {
                events.push(TravelEvent::Status(format!(
                    "{} maze - walking your pathcode ({} steps)",
                    maze.name,
                    route.len()
                )));
                (self.maze_entry_phase(maze, current, ctx, events), route.clone())
            }
            None => {
                events.push(TravelEvent::Status(format!(
                    "{} maze - no pathcode stored; asking",
                    maze.name
                )));
                events.push(TravelEvent::Send(maze.ask.clone()));
                (MazePhase::AwaitCode { sent_ms: ctx.now_ms }, Vec::new())
            }
        };
        self.step = Step::Maze {
            maze_name: maze.name.clone(),
            phase,
            route,
            i: 0,
            attempts: 0,
            dest_inside,
        };
    }

    /// The phase that gets a known route moving: step from the entrance to
    /// the start room first when needed, else walk immediately.
    fn maze_entry_phase(
        &mut self,
        maze: &super::mazes::MazeDef,
        current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) -> MazePhase {
        if current == maze.start {
            return MazePhase::Walk {
                wait_until: 0,
                sent_from: None,
            };
        }
        // entrance → start via the mapdb edge (a real, unscrambled edge).
        match ctx
            .db
            .room(maze.entrance)
            .and_then(|r| r.wayto.get(&maze.start).cloned())
        {
            Some(cmd) => {
                events.push(TravelEvent::Send(cmd));
                MazePhase::ToStart { sent_ms: ctx.now_ms }
            }
            None => {
                // Shouldn't happen with sane maze data; walk from here.
                MazePhase::Walk {
                    wait_until: 0,
                    sent_from: None,
                }
            }
        }
    }

    /// Maze state machine tick. Movement inside is scrambled, so route
    /// commands are paced (RT + a fixed gap) without per-step verification;
    /// only the final room is checked. Recovery follows the NPC's own
    /// protocol: `search` to re-orient, then restart the route.
    #[allow(clippy::too_many_arguments)]
    fn tick_maze(
        &mut self,
        maze_name: String,
        phase: MazePhase,
        route: Vec<String>,
        i: usize,
        attempts: u32,
        dest_inside: bool,
        current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        let Some(maze) = super::mazes::all().iter().find(|m| m.name == maze_name) else {
            events.push(TravelEvent::Failed(format!(
                "maze '{maze_name}' vanished from the definitions - travel aborted"
            )));
            return;
        };
        if ctx.muckled {
            if !self.muckle_announced {
                events.push(TravelEvent::Status(
                    "stunned/webbed - waiting until you can move".into(),
                ));
                self.muckle_announced = true;
            }
            self.step = Step::Maze { maze_name, phase, route, i, attempts, dest_inside };
            return;
        }
        self.muckle_announced = false;

        let mut phase = phase;
        let mut route = route;
        let mut i = i;
        let mut attempts = attempts;

        match &phase {
            MazePhase::AwaitCode { sent_ms } => {
                if let Some(stored) = ctx.pathcodes.get(&maze.name) {
                    route = stored.clone();
                    events.push(TravelEvent::Status(format!(
                        "pathcode captured - walking ({} steps)",
                        route.len()
                    )));
                    phase = self.maze_entry_phase(maze, current, ctx, events);
                } else if ctx.now_ms.saturating_sub(*sent_ms) > MAZE_ASK_TIMEOUT_MS {
                    events.push(TravelEvent::Failed(format!(
                        "no pathcode heard from the {} NPC - ask manually, then rerun .go2",
                        maze.name
                    )));
                    return;
                }
            }
            MazePhase::ToStart { sent_ms } => {
                if current == maze.start {
                    phase = MazePhase::Walk {
                        wait_until: 0,
                        sent_from: None,
                    };
                } else if ctx.now_ms.saturating_sub(*sent_ms) > STEP_TIMEOUT_MS {
                    events.push(TravelEvent::Failed(format!(
                        "couldn't reach the {} maze start room - travel aborted",
                        maze.name
                    )));
                    return;
                }
            }
            MazePhase::Walk {
                wait_until,
                sent_from,
            } => {
                let landed = sent_from.map_or(true, |from| current != from);
                if ctx.rt_remaining > 0.0 || (!landed && ctx.now_ms < *wait_until) {
                    // waiting on RT, or the last move hasn't visibly landed
                } else if let Some(cmd) = route.get(i) {
                    events.push(TravelEvent::Send(cmd.clone()));
                    i += 1;
                    phase = MazePhase::Walk {
                        wait_until: ctx.now_ms + MAZE_STEP_GAP_MS,
                        sent_from: Some(current),
                    };
                } else if landed {
                    // Final move landed: judge it immediately.
                    phase = MazePhase::Verify { until: ctx.now_ms };
                } else {
                    phase = MazePhase::Verify {
                        until: ctx.now_ms + MAZE_SETTLE_MS,
                    };
                }
            }
            MazePhase::Verify { until } => {
                if ctx.now_ms >= *until {
                    if !maze.rooms.contains(&current) && current != maze.entrance {
                        // Through. Either this WAS the goal, or normal
                        // routing resumes from the far side.
                        if dest_inside || current == self.destination {
                            events.push(TravelEvent::Status(format!(
                                "through the {} maze",
                                maze.name
                            )));
                            events.push(TravelEvent::Arrived {
                                destination: current,
                                seconds: (ctx.now_ms - self.started_ms) as f64 / 1000.0,
                            });
                        } else {
                            events.push(TravelEvent::Status(format!(
                                "through the {} maze - continuing",
                                maze.name
                            )));
                            self.repath(ctx.db, current, events);
                        }
                        return;
                    }
                    // Still inside (or bounced to the entrance): recover per
                    // the NPC - search to re-orient, then start again.
                    attempts += 1;
                    if attempts > MAZE_MAX_ATTEMPTS {
                        events.push(TravelEvent::Failed(format!(
                            "couldn't get through the {} maze - return to the entrance and try again",
                            maze.name
                        )));
                        return;
                    }
                    events.push(TravelEvent::Status(format!(
                        "wrong turn in the {} maze - searching to re-orient (attempt {attempts})",
                        maze.name
                    )));
                    events.push(TravelEvent::Send("search".into()));
                    i = 0;
                    phase = MazePhase::PostSearch {
                        until: ctx.now_ms + MAZE_SETTLE_MS,
                    };
                }
            }
            MazePhase::PostSearch { until } => {
                if ctx.now_ms >= *until {
                    if current == maze.start {
                        phase = MazePhase::Walk {
                            wait_until: 0,
                            sent_from: None,
                        };
                    } else if current == maze.entrance {
                        phase = self.maze_entry_phase(maze, current, ctx, events);
                    } else {
                        // Still lost somewhere inside: search again (counted
                        // by the same attempts budget via Verify).
                        phase = MazePhase::Verify {
                            until: ctx.now_ms + MAZE_SETTLE_MS,
                        };
                    }
                }
            }
        }

        self.step = Step::Maze { maze_name, phase, route, i, attempts, dest_inside };
    }

    /// Cross the Confluence threshold: step into the Plane (the planned edge is
    /// real — it's the entry from outside), then hand off to the explorer.
    fn begin_confluence(
        &mut self,
        current: u32,
        next: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        // The entry edge itself is a normal mapdb edge (walk it if we're not in
        // yet). Once we're the first zone room, the explorer takes over.
        if let Some(command) = ctx
            .db
            .room(current)
            .and_then(|room| room.wayto.get(&next).cloned())
        {
            if ctx.rt_remaining > 0.0 {
                return; // wait out RT before entering
            }
            events.push(TravelEvent::Status(
                "entering the Plane of Elemental Confluence - exploring for the exit".into(),
            ));
            events.push(TravelEvent::Send(command));
            self.confluence = Some(super::confluence::ConfluenceState::new());
            self.step = Step::Confluence { pending: None };
        } else {
            // No entry edge from here — re-path.
            self.repath(ctx.db, current, events);
        }
    }

    /// The Confluence explorer tick: one step of learn-and-navigate. Mirrors
    /// the Ruby self-loop (`stringprocs/wayto/room-23282-to-23282.rb`).
    fn tick_confluence(
        &mut self,
        pending: Option<ConfluencePending>,
        current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        // Warped out of the Plane (the `go tranquility` portal, or any exit):
        // the explorer is done; re-path from wherever we landed.
        if !super::confluence::is_confluence_room(current) {
            self.confluence = None;
            events.push(TravelEvent::Status(
                "left the Plane - re-pathing to the destination".into(),
            ));
            self.repath(ctx.db, current, events);
            return;
        }
        if ctx.muckled {
            if !self.muckle_announced {
                events.push(TravelEvent::Status(
                    "stunned/webbed - waiting until you can move".into(),
                ));
                self.muckle_announced = true;
            }
            self.step = Step::Confluence { pending };
            return;
        }
        self.muckle_announced = false;

        // A move is in flight: wait for the room to change, then record where
        // the direction led and clear the pending move.
        if let Some(p) = &pending {
            if current == p.from {
                // Move failed hard (aho-corasick MoveFailed) → random compass
                // exit, per the Ruby `if r == false` fallback.
                if ctx.feedback.iter().any(|f| {
                    matches!(
                        f,
                        crate::core::move_feedback::MoveFeedback::MoveFailedRemovable
                            | crate::core::move_feedback::MoveFeedback::MoveFailedKeep
                    )
                }) {
                    if let Some(dir) = pick_random_exit(ctx.compass_dirs, current) {
                        events.push(TravelEvent::Send(dir.clone()));
                        self.step = Step::Confluence {
                            pending: Some(ConfluencePending {
                                from: current,
                                dir,
                                sent_ms: ctx.now_ms,
                            }),
                        };
                    } else {
                        self.step = Step::Confluence { pending: None };
                    }
                    return;
                }
                if ctx.now_ms.saturating_sub(p.sent_ms) <= STEP_TIMEOUT_MS {
                    return; // still waiting to land
                }
                // Timed out sitting in the same room — try again fresh.
                self.step = Step::Confluence { pending: None };
                return;
            }
            // Arrived somewhere new: learn the edge.
            if let Some(state) = self.confluence.as_mut() {
                state.record_arrival(p.from, &p.dir, current);
            }
            self.step = Step::Confluence { pending: None };
            // fall through to choose the next step from `current`
        }

        // RT gate before the next decision/move.
        if ctx.rt_remaining > 0.0 {
            return;
        }

        let Some(hot) = super::confluence::hot_side(current) else {
            // Off the zone map (Ruby `$go2_restart = true`) — re-path.
            self.confluence = None;
            self.repath(ctx.db, current, events);
            return;
        };

        let Some(state) = self.confluence.as_mut() else {
            // Shouldn't happen; recover by re-seeding.
            self.confluence = Some(super::confluence::ConfluenceState::new());
            self.step = Step::Confluence { pending: None };
            return;
        };

        // Landmark scan from live loot, then record exits (which may detect a
        // maze shift and wipe the learned map — we just re-run next tick).
        state.observe_landmarks(current, hot, ctx.loot_nouns);
        if state.record_exits(current, ctx.compass_dirs) {
            events.push(TravelEvent::Status(
                "the Plane shifted - relearning the maze".into(),
            ));
            self.step = Step::Confluence { pending: None };
            return;
        }

        match state.choose_dir(current, hot, ctx.loot_nouns) {
            super::confluence::ConfluenceMove::Arrive => {
                events.push(TravelEvent::Send(
                    super::confluence::TRANQUILITY_GO.to_string(),
                ));
                // Next tick we'll be outside the Plane → re-path.
                self.step = Step::Confluence { pending: None };
            }
            super::confluence::ConfluenceMove::CrossPit => {
                events.push(TravelEvent::Send(super::confluence::PIT_GO.to_string()));
                self.step = Step::Confluence { pending: None };
            }
            super::confluence::ConfluenceMove::Go(dir) => {
                events.push(TravelEvent::Send(dir.clone()));
                self.step = Step::Confluence {
                    pending: Some(ConfluencePending {
                        from: current,
                        dir,
                        sent_ms: ctx.now_ms,
                    }),
                };
            }
            super::confluence::ConfluenceMove::Restart => {
                // No exits at all — bail out to a normal re-path.
                self.confluence = None;
                self.repath(ctx.db, current, events);
            }
        }
    }

    /// Run a transpiled edge script until it blocks (RT wait, sleep) or
    /// finishes (→ arrival watching).
    #[allow(clippy::too_many_arguments)]
    fn tick_script(
        &mut self,
        mut actions: Vec<crate::core::pathing::edge::WalkAction>,
        mut pc: usize,
        mut sleep_until: Option<u64>,
        expected: u32,
        from: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        use crate::core::pathing::edge::WalkAction;
        // Did this run emit any command? A script that finishes without sending
        // anything (a pure `;e true` pass-through, e.g. the virtual urchin
        // hideout entry) causes no room change — so we must NOT arrival-watch
        // for `expected` (that would time out and ban the edge). Instead we
        // advance the route and let the NEXT edge's command (the `urchin guide
        // <dest>` that actually moves us) do the work. Mirrors go2.lic, which
        // records `moves_sent = $room_count` after a `.call` and moves on
        // without verifying it reached `next_id` (map_gs.rb ~2398).
        let mut sent_anything = false;
        loop {
            let Some(action) = actions.get(pc).cloned() else {
                if sent_anything {
                    // Something was sent → a room change is expected. Scripted
                    // edges (portmaster escorts, timed jumps) can be slow, so
                    // give them the longer arrival window.
                    self.step = Step::AwaitArrival {
                        expected,
                        from,
                        sent_ms: ctx.now_ms,
                        slow: true,
                    };
                } else {
                    // Pure pass-through (`;e true`): the edge target is a
                    // VIRTUAL room you never physically occupy — a `hideout`
                    // whose only purpose is to host the real crossing command
                    // on ITS wayto. `expected` (the hideout) will never be the
                    // game's current room, so we must collapse: send the
                    // hideout's command for the route's NEXT room and watch for
                    // arrival there. This is the urchin `;e true` -> `urchin
                    // guide <dest>` pair, and confluence's nominal entry.
                    self.pass_through(expected, from, ctx, events);
                }
                return;
            };
            match action {
                WalkAction::Noop => pc += 1,
                WalkAction::Move(cmd) | WalkAction::Put(cmd) => {
                    events.push(TravelEvent::Send(cmd));
                    sent_anything = true;
                    pc += 1;
                }
                WalkAction::StepMove(cmd) => {
                    // Paced walk step: send it, then suspend until the room
                    // changes before running the next action (so a multi-room
                    // crossing doesn't flood commands ahead of arrival).
                    if ctx.rt_remaining > 0.0 {
                        break; // wait out RT; resume this same StepMove next tick
                    }
                    events.push(TravelEvent::Send(cmd));
                    self.step = Step::ScriptWalk {
                        actions,
                        pc: pc + 1,
                        expected,
                        from,
                        sent_from: ctx.current_room.unwrap_or(from),
                        sent_ms: ctx.now_ms,
                    };
                    return;
                }
                WalkAction::WaitRt => {
                    if ctx.rt_remaining > 0.0 {
                        break;
                    }
                    pc += 1;
                }
                WalkAction::Sleep(seconds) => match sleep_until {
                    None => {
                        sleep_until = Some(ctx.now_ms + (seconds.max(0.0) * 1000.0) as u64);
                        break;
                    }
                    Some(until) if ctx.now_ms < until => break,
                    Some(_) => {
                        sleep_until = None;
                        pc += 1;
                    }
                },
                WalkAction::If { cond, then, els } => {
                    let branch = if ctx.eval(cond) { then } else { els };
                    actions.splice(pc..=pc, branch);
                }
                WalkAction::EmptyHands | WalkAction::FillHands => {
                    let is_empty = matches!(action, WalkAction::EmptyHands);
                    // No hands inputs (a headless caller / test that doesn't
                    // wire GameState): fall back to a bare stow/get so the
                    // edge still walks.
                    let Some(inputs) = ctx.hands else {
                        events.push(TravelEvent::Send(
                            if is_empty { "stow both" } else { "get both" }.to_string(),
                        ));
                        pc += 1;
                        continue;
                    };
                    // Suspend the script and run the StashService. `resume` is
                    // the RunScript step that continues this edge once hands
                    // are done — advanced past this action.
                    let mut task = if is_empty {
                        super::stash::StashTask::empty()
                    } else {
                        super::stash::StashTask::fill(std::mem::take(&mut self.stash_stack))
                    };
                    // Drive the first tick now so a command goes out this frame.
                    let sctx = inputs.to_stash_context(ctx.now_ms);
                    let mut done_immediately = false;
                    for ev in task.tick(sctx) {
                        match ev {
                            super::stash::StashEvent::Send(cmd) => {
                                events.push(TravelEvent::Send(cmd))
                            }
                            super::stash::StashEvent::Done => done_immediately = true,
                            super::stash::StashEvent::Failed(why) => {
                                events.push(TravelEvent::Status(format!("hands: {why}")));
                                done_immediately = true;
                            }
                        }
                    }
                    let resume = Box::new(Step::RunScript {
                        actions: actions.clone(),
                        pc: pc + 1,
                        sleep_until,
                        expected,
                        from,
                    });
                    if done_immediately {
                        // Empty stack (nothing to stow) — carry on inline.
                        if is_empty {
                            self.stash_stack = task.take_stack();
                        }
                        pc += 1;
                        continue;
                    }
                    self.stash = Some(task);
                    self.step = Step::Stashing { resume };
                    return;
                }
                WalkAction::Replan => {
                    // The edge asked to re-plan from here ($go2_restart).
                    self.repath(ctx.db, from, events);
                    return;
                }
            }
        }
        self.step = Step::RunScript {
            actions,
            pc,
            sleep_until,
            expected,
            from,
        };
    }

    /// Build the day-pass crossing command queue and enter Step::DayPass. The
    /// queue is the edge's literal script (from the departure metadata) with
    /// the two live substitutions — the sack id (config) and the held pass id
    /// (cache) — resolved from `ctx.day_pass`. When no pass is held and buying
    /// isn't set up, we bail to a normal re-path with a message.
    fn begin_day_pass(
        &mut self,
        from: u32,
        next: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        use crate::core::day_pass;
        use crate::core::pathing::edge::WalkAction as W;
        let Some((dep, dest)) = day_pass::edge(from, next) else {
            self.repath(ctx.db, from, events);
            return;
        };
        let inputs = ctx.day_pass;
        let sack = inputs.and_then(|i| i.sack_id);
        let (a, b) = dest.pair;
        // Held valid pass for THIS edge's pair, and whether buying is permitted
        // (config matches the pair AND Get Silvers is on to cover a shortfall).
        let held = inputs.and_then(|i| i.cache.valid_pass_id(a, b, i.now_epoch));
        let buy = inputs
            .map(|i| i.get_silvers && crate::core::day_pass::buy_permits(i.buy_day_pass, a, b))
            .unwrap_or(false);

        // Build the crossing as a WalkAction script and run it through the
        // normal scripted-edge machinery (tick_script) — so it reuses the
        // EmptyHands/FillHands stash primitives, RT waits, and arrival watching
        // instead of hand-rolled command strings. `Put` = send, no room change;
        // `Move` = send + expect the room to change (the pass `raise` is the
        // mover). Pass/sack ids are substituted here from live state.
        let mut script: Vec<W> = Vec::new();
        if let Some(sack) = sack {
            script.push(W::Put(format!("open #{sack}"))); // harmless if already open
        }
        // Free a hand for the pass (the clerk won't hand it over otherwise) —
        // our real stash primitive, not a raw `empty hands` string.
        script.push(W::EmptyHands);

        if let Some(pass) = held {
            // USE: get the held pass, raise it (this travels), put it back.
            script.push(W::Put(format!("get #{pass}")));
            script.push(W::Move(format!("raise #{pass}")));
            if let Some(sack) = sack {
                script.push(W::Put(format!("_drag #{pass} #{sack}")));
            }
        } else if buy {
            // BUY: step to the NPC, ask (twice — the confirm), fund at this
            // town's bank if short, re-ask, then the pass is in hand; look at
            // it and raise it (travels). Ids unknown pre-buy, so use the `pass`
            // noun (Lich resolves it from the hand). Every walk is a StepMove
            // (waits to arrive before the next command) so we don't `withdraw`
            // before reaching the bank; only the final `raise` is the terminal
            // Move that lands us at the destination.
            script.push(W::StepMove(dep.enter_move.to_string()));
            script.push(W::Put(format!("ask {} for {}", dep.npc, dest.ask_word)));
            script.push(W::Put(format!("ask {} for {}", dep.npc, dest.ask_word)));
            for dir in dep.to_bank {
                script.push(W::StepMove((*dir).to_string()));
            }
            script.push(W::Put("withdraw 5000".into()));
            for dir in dep.from_bank {
                script.push(W::StepMove((*dir).to_string()));
            }
            script.push(W::Put(format!("ask {} for {}", dep.npc, dest.ask_word)));
            script.push(W::Put("look pass".into()));
            script.push(W::Move("raise pass".into()));
            if let Some(sack) = sack {
                script.push(W::Put(format!("_drag #pass #{sack}")));
            }
        } else {
            // No pass and no buy permission — can't cross. Ban + re-path.
            events.push(TravelEvent::Status(
                "no valid day pass held and buying is off - re-pathing".into(),
            ));
            self.banned.insert((from, next));
            self.repath(ctx.db, from, events);
            return;
        }

        // Recover the original held items (LIFO, from the EmptyHands above).
        script.push(W::FillHands);
        if let Some(sack) = sack {
            script.push(W::Put(format!("close #{sack}")));
        }

        events.push(TravelEvent::Status(format!(
            "day pass to {} - {}",
            dest.ask_word,
            if held.is_some() { "using held pass" } else { "buying" }
        )));
        self.tick_script(script, 0, None, next, from, ctx, events);
    }

    /// Collapse a `;e true` pass-through edge (`from` -> `virtual`) with the
    /// real crossing command that lives on the virtual room's wayto. Sends the
    /// virtual room's command for the route's next room and arrival-watches for
    /// it, staying anchored at the physical `from` (we never occupy `virtual`).
    fn pass_through(
        &mut self,
        virtual_room: u32,
        from: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        // Advance past the virtual room in the route.
        self.idx += 1;
        self.edge_retries = 0;
        let Some(&next) = self.path.get(self.idx) else {
            // The virtual room WAS the destination (shouldn't happen for a
            // hideout, but be safe): nothing more to do; re-path to confirm.
            self.repath(ctx.db, from, events);
            return;
        };
        // The crossing command is on the virtual room's wayto for `next`.
        let Some(command) = ctx
            .db
            .room(virtual_room)
            .and_then(|room| room.wayto.get(&next).cloned())
        else {
            // No command bridges the virtual room to the next hop — re-path
            // around it from where we physically are.
            self.repath(ctx.db, from, events);
            return;
        };
        if ctx.rt_remaining > 0.0 {
            // Wait out RT; re-enter this edge next tick (idx already advanced,
            // so re-derive by stepping back to the virtual entry is unneeded —
            // we simply hold in a fresh Prepare that will re-collapse).
            self.idx -= 1;
            self.step = Step::Prepare;
            return;
        }
        // A proc command on the virtual->next edge (e.g. the urchin guide is a
        // plain string, but confluence/other hideouts could be scripted).
        if crate::core::mapdb::is_proc_command(&command) {
            match crate::core::pathing::transpile::transpile_edge(ctx.db, &command) {
                Some(actions) => {
                    self.tick_script(actions, 0, None, next, from, ctx, events);
                    return;
                }
                None => {
                    self.handle_uncrossable_edge(from, next, ctx, events);
                    return;
                }
            }
        }
        events.push(TravelEvent::Send(command));
        // Urchin guides / other pass-through crossings are slow to confirm.
        self.step = Step::AwaitArrival {
            expected: next,
            from,
            sent_ms: ctx.now_ms,
            slow: true,
        };
    }

    /// Drive the running StashService one tick. On completion, resume the
    /// script step that requested the hands cycle.
    fn tick_stashing(
        &mut self,
        resume: Box<Step>,
        _current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        let Some(inputs) = ctx.hands else {
            // Hands inputs vanished mid-cycle (shouldn't happen): resume.
            self.step = *resume;
            return;
        };
        let Some(task) = self.stash.as_mut() else {
            self.step = *resume;
            return;
        };
        let sctx = inputs.to_stash_context(ctx.now_ms);
        let mut finished = false;
        for ev in task.tick(sctx) {
            match ev {
                super::stash::StashEvent::Send(cmd) => events.push(TravelEvent::Send(cmd)),
                super::stash::StashEvent::Done => finished = true,
                super::stash::StashEvent::Failed(why) => {
                    events.push(TravelEvent::Status(format!("hands: {why}")));
                    finished = true;
                }
            }
        }
        if finished {
            // An EmptyHands hands its stow stack to the later FillHands.
            if matches!(task.op(), super::stash::StashOp::Empty) {
                self.stash_stack = task.take_stack();
            }
            self.stash = None;
            // Resume the suspended script step immediately this tick, so the
            // next action (the climb, or the fill) doesn't wait a tick.
            if let Step::RunScript {
                actions,
                pc,
                sleep_until,
                expected,
                from,
            } = *resume
            {
                self.tick_script(actions, pc, sleep_until, expected, from, ctx, events);
            } else {
                self.step = *resume;
            }
        }
    }

    /// The silver-funding pre-flight (Step::Funding). Ported from go2.lic's
    /// bank-withdraw routine.
    fn tick_funding(
        &mut self,
        phase: FundingPhase,
        current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        let funding = ctx.funding;
        match phase {
            FundingPhase::AwaitWealth { sent_ms } => {
                // Send `wealth quiet` once, then wait for the silver to update.
                // (sent_ms == started: first entry — fire the check.)
                if sent_ms == self.started_ms {
                    events.push(TravelEvent::Send("wealth quiet".into()));
                    // Move sent_ms forward so we don't re-send every tick.
                    self.step = Step::Funding(FundingPhase::AwaitWealth {
                        sent_ms: ctx.now_ms.max(self.started_ms + 1),
                    });
                    return;
                }
                let Some(silver) = funding.and_then(|f| f.silver) else {
                    // Still waiting for the wealth line (or no funding inputs).
                    if ctx.now_ms.saturating_sub(sent_ms) > STEP_TIMEOUT_MS {
                        // No wealth response — proceed and hope for the best
                        // (Lich's routine only runs under GS; we don't block).
                        self.begin_walk(current, ctx, events);
                    }
                    return;
                };
                if silver >= self.silver_need {
                    // Funded — walk the real trip.
                    events.push(TravelEvent::Status(format!(
                        "trip costs {} silver, you have {silver} - funded",
                        self.silver_need
                    )));
                    self.begin_walk(current, ctx, events);
                    return;
                }
                // Short on silver.
                let get_silvers = funding.map(|f| f.get_silvers).unwrap_or(false);
                if !get_silvers {
                    events.push(TravelEvent::Status(format!(
                        "trip costs {} silver, you have {silver} - short; enable Get Silvers to auto-withdraw. Continuing anyway.",
                        self.silver_need
                    )));
                    self.begin_walk(current, ctx, events);
                    return;
                }
                // Find the nearest bank we can afford to WALK to (its own path
                // cost must be within current silver — Lich's affordability
                // check), then redirect the trip there.
                let real_dest = self.destination;
                let need = self.silver_need;
                match self.nearest_affordable_bank(current, silver, ctx.db) {
                    Some(bank) => {
                        // Already standing in the nearest bank (e.g. logged in
                        // at the teller): don't try to walk to it — path_to to
                        // the same room is empty and would look like "lost the
                        // route". Hand straight to the withdraw phase here.
                        if bank == current {
                            self.funding_bank = Some(bank);
                            self.step =
                                Step::Funding(FundingPhase::RoutingToBank { real_dest, need });
                            self.tick_funding(
                                FundingPhase::RoutingToBank { real_dest, need },
                                current,
                                ctx,
                                events,
                            );
                            return;
                        }
                        // Redirect the WALK to the bank (path only — the real
                        // destination stays put; funding_bank marks the detour).
                        let Some(bank_path) = pathing::path_to(ctx.db, current, bank) else {
                            events.push(TravelEvent::Failed(
                                "lost the route to the bank - travel aborted".into(),
                            ));
                            return;
                        };
                        events.push(TravelEvent::Status(format!(
                            "short {} silver - routing to the nearest bank (room {bank}) to withdraw",
                            need.saturating_sub(silver)
                        )));
                        self.path = bank_path;
                        self.idx = 0;
                        self.funding_bank = Some(bank);
                        self.step = Step::Funding(FundingPhase::RoutingToBank { real_dest, need });
                        self.tick_funding(
                            FundingPhase::RoutingToBank { real_dest, need },
                            current,
                            ctx,
                            events,
                        );
                    }
                    None => {
                        events.push(TravelEvent::Failed(
                            "you're too poor to even reach a bank - travel aborted".into(),
                        ));
                    }
                }
            }
            FundingPhase::RoutingToBank { real_dest, need } => {
                // Arrived at the bank? Withdraw. Else keep walking (Prepare).
                if Some(current) == self.funding_bank {
                    let have = funding.and_then(|f| f.silver).unwrap_or(0);
                    let amount = need.saturating_sub(have);
                    let cmd = if ctx.at_pinefar_depository {
                        format!("ask banker for {} silvers", amount.max(20))
                    } else {
                        format!("withdraw {amount} silvers")
                    };
                    events.push(TravelEvent::Send(cmd));
                    // Re-check wealth after withdrawing: the withdraw
                    // confirmation ("hands you N silvers") isn't a wealth line,
                    // so game_state.silver won't update on its own. Lich does
                    // the same — it calls go2_check_silver() (wealth quiet)
                    // again right after the withdraw (go2.lic:2278-2280).
                    events.push(TravelEvent::Send("wealth quiet".into()));
                    self.funding_bank = None;
                    self.silver_at_withdraw = funding.and_then(|f| f.silver);
                    self.step = Step::Funding(FundingPhase::AwaitWithdraw {
                        real_dest,
                        need,
                        sent_ms: ctx.now_ms,
                    });
                } else {
                    // Still en route — let the normal walk machinery run.
                    self.step = Step::Prepare;
                    self.tick_prepare(current, ctx, events);
                }
            }
            FundingPhase::AwaitWithdraw {
                real_dest,
                need,
                sent_ms,
            } => {
                let reading = funding.and_then(|f| f.silver);
                // The post-withdraw `wealth quiet` hasn't reflected yet if the
                // reading still equals what we had when we sent the withdraw
                // (and we DID have a pre-withdraw reading). Wait for it to move.
                let refreshed = reading != self.silver_at_withdraw
                    || (self.silver_at_withdraw.is_none() && reading.is_some());
                let have = reading.unwrap_or(0);
                if refreshed && have >= need {
                    // Funded — re-plan to the real destination and walk.
                    events.push(TravelEvent::Status(format!(
                        "withdrew to {have} silver - continuing to room {real_dest}"
                    )));
                    self.destination = real_dest;
                    self.repath(ctx.db, current, events);
                } else if refreshed && have < need {
                    // The fresh reading came back and it's still short — the
                    // bank couldn't cover it (Lich's "too poor" bail).
                    events.push(TravelEvent::Failed(format!(
                        "withdrew what the bank had ({have} silver) but the trip needs {need} - aborted"
                    )));
                } else if ctx.now_ms.saturating_sub(sent_ms) > STEP_TIMEOUT_MS {
                    // No fresh wealth reading at all — re-probe once more rather
                    // than abort on a dropped line.
                    events.push(TravelEvent::Send("wealth quiet".into()));
                    self.step = Step::Funding(FundingPhase::AwaitWithdraw {
                        real_dest,
                        need,
                        sent_ms: ctx.now_ms,
                    });
                }
                // else keep waiting for the fresh wealth reading.
            }
        }
    }

    /// Leave the funding pre-flight and start walking the current path.
    fn begin_walk(&mut self, current: u32, ctx: TravelContext, events: &mut Vec<TravelEvent>) {
        self.step = Step::Prepare;
        self.tick_prepare(current, ctx, events);
    }

    /// Nearest bank reachable within `silver` (its walk cost affordable), the
    /// go2 affordability check. None if every bank costs more to reach than we
    /// have.
    ///
    /// Performance: ONE Dijkstra with a multi-target (`AnyOf`) early-exit, not
    /// one per bank. The old version ran `path_to` (a full-graph Dijkstra) up
    /// to 72 times — 36 banks × sort-key + 36 × affordability — which, on the
    /// fully-inlined 36k-room Cartographer graph, froze the UI thread for
    /// 10-20s. We rank the banks the single search actually reached by
    /// distance, then only reconstruct/affordability-check paths in that order
    /// (each reconstruction is a cheap previous-pointer walk, not a search).
    fn nearest_affordable_bank(&self, from: u32, silver: u64, db: &MapDb) -> Option<u32> {
        let banks: Vec<u32> = db.room_ids_with_tag("bank").to_vec();
        // Single search toward whichever bank is nearest; `distance` is then
        // populated for every bank the frontier reached.
        let search = pathing::dijkstra(db, from, Some(pathing::PathTarget::AnyOf(&banks)));
        let mut reached: Vec<(u32, f64)> = banks
            .iter()
            .filter_map(|&b| search.distance.get(&b).map(|&d| (b, d)))
            .collect();
        reached.sort_by(|a, b| a.1.total_cmp(&b.1));
        // Nearest affordable first. Path reconstruction here is a cheap walk
        // back through `previous`, and silver_cost is O(path length).
        for (bank, _) in reached {
            if let Some(path) = search.reconstruct(from, bank) {
                let full: Vec<u32> = std::iter::once(from).chain(path).collect();
                if pathing::silver_cost(db, &full) <= silver {
                    return Some(bank);
                }
            }
        }
        None
    }

    fn tick_prepare(&mut self, current: u32, ctx: TravelContext, events: &mut Vec<TravelEvent>) {
        if ctx.muckled {
            if !self.muckle_announced {
                events.push(TravelEvent::Status(
                    "stunned/webbed - waiting until you can move".into(),
                ));
                self.muckle_announced = true;
            }
            return;
        }
        self.muckle_announced = false;

        let Some(&next) = self.path.get(self.idx) else {
            // Path exhausted without reaching the destination — re-path.
            self.repath(ctx.db, current, events);
            return;
        };
        // Curated maze boundary: the planned edges inside are junk (movement
        // scrambles), so the plan is abandoned at the threshold and the
        // maze's pathcode strategy takes over.
        if let Some(maze) = super::mazes::maze_containing(next) {
            if !maze.rooms.contains(&current) {
                self.begin_maze(maze, current, ctx, events);
                return;
            }
        }
        // Confluence boundary: the planned edge steps INTO the Plane, whose
        // exits are a shifting maze with no fixed graph. Once we're across the
        // threshold the explorer takes over (it learns adjacency live and warps
        // out via the tranquility portal, after which we re-path normally).
        if super::confluence::is_confluence_room(next)
            && !super::confluence::is_confluence_room(current)
        {
            self.begin_confluence(current, next, ctx, events);
            return;
        }
        let Some(command) = ctx
            .db
            .room(current)
            .and_then(|room| room.wayto.get(&next).cloned())
        else {
            // The planned edge doesn't exist from where we actually are.
            self.repath(ctx.db, current, events);
            return;
        };

        // go2: swim/pedal edges skip the stand dance.
        let needs_stand = !ctx.standing && !command_is_swim_or_pedal(&command);
        if ctx.rt_remaining > 0.0 {
            return; // waitrt?
        }
        if needs_stand {
            events.push(TravelEvent::Send("stand".into()));
            self.step = Step::AwaitStand {
                sent_ms: ctx.now_ms,
                attempts: 1,
            };
            return;
        }
        // Curated override beats whatever the mapdb says about this edge.
        if let Some(ov) = crate::core::pathing::overrides::edge_override(current, next) {
            self.tick_script(ov.actions.clone(), 0, None, next, current, ctx, events);
            return;
        }
        // Chronomage day-pass edge: a self-contained per-town script (open sack
        // → use/buy a pass → raise it → put it back). Its proc is far too large
        // to transpile; instead we build the literal command queue from the
        // edge's departure metadata + the live pass/sack ids and pace it.
        if crate::core::day_pass::edge(current, next).is_some() {
            self.begin_day_pass(current, next, ctx, events);
            return;
        }
        if crate::core::mapdb::is_proc_command(&command) {
            // Scripted edge. The router weights on `timeto` alone (Lich
            // parity), so it may plan a route through a proc we can't yet
            // transpile — interpreting the proc is our job here, at the edge.
            match crate::core::pathing::transpile::transpile_edge(ctx.db, &command) {
                Some(actions) => {
                    self.tick_script(actions, 0, None, next, current, ctx, events);
                }
                None => {
                    // Can't interpret this proc natively. This is where the
                    // optional Lich `;go2` fallback fires (P6); until then, and
                    // whenever it's off/unavailable, disable the edge for the
                    // session and re-path around it.
                    self.handle_uncrossable_edge(current, next, ctx, events);
                }
            }
            return;
        }
        events.push(TravelEvent::Send(command));
        self.step = Step::AwaitArrival {
            expected: next,
            from: current,
            sent_ms: ctx.now_ms,
            slow: false,
        };
    }

    /// React to move-feedback while awaiting arrival, the way Lich's `move`
    /// reacts to game lines (global_defs.rb:603-771). Sends a recovery command
    /// and resets the arrival timer so the retry gets a fresh window. Returns
    /// true if it handled a feedback event this tick (the caller returns).
    fn recover_from_feedback(
        &mut self,
        from: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) -> bool {
        use crate::core::move_feedback::MoveFeedback as F;
        // The command that failed is the wayto for this edge.
        let expected = match &self.step {
            Step::AwaitArrival { expected, sent_ms, slow, .. } => (*expected, *sent_ms, *slow),
            _ => return false,
        };
        let (expected, sent_ms, slow) = expected;
        // Arrival wins over any failure line (Lich's `room_count > room_count`
        // guard, global_defs.rb:587): if a room-change signal fired this tick,
        // the move succeeded — ignore a co-queued failure line, which is stale
        // or spurious (a lagged/raced rejection for a move that actually
        // landed). Reset the arrival window and wait for the room to resolve.
        // Without this, a failure racing the room change bans a good edge and
        // strands the trip (the live Trollfang 1280->1281 abort).
        if ctx.saw(&F::NavArrived) {
            self.edge_retries = 0;
            self.step = Step::AwaitArrival {
                expected,
                from,
                sent_ms: ctx.now_ms,
                slow,
            };
            return true;
        }
        let _ = (sent_ms, slow);
        let command = ctx
            .db
            .room(from)
            .and_then(|r| r.wayto.get(&expected).cloned())
            .unwrap_or_default();

        // Mounted → urchin travel is incompatible. Drop urchins for the rest
        // of the trip and re-path on foot (Lich go2:2336-2346). Only acts when
        // urchins were actually in play.
        if ctx.saw(&F::Mounted) && crate::core::pathing::transpile::urchins_valid() {
            crate::core::pathing::transpile::set_urchins_valid(false);
            events.push(TravelEvent::Status(
                "you're mounted - urchin guides don't work mounted; re-routing on foot".into(),
            ));
            self.repath(ctx.db, from, events);
            return true;
        }

        // Apparent hard failure. A move-rejection line can be spurious: it may
        // be combat/ambient text that merely contains a failure phrase, or a
        // real rejection that raced a lagged room change (the move actually
        // succeeded — Lich guards this with `room_count > room_count`). So we
        // do NOT ban a cardinal edge on the first failure. We retry the move a
        // couple of times; if the room already changed, next tick's arrival
        // check wins and the edge is never touched. Only after the retries are
        // exhausted (a genuinely dead edge) do we ban it and re-path — matching
        // Lich's `return false` but after its retry loop, not before it.
        if ctx.saw(&F::MoveFailedRemovable) {
            if self.edge_retries >= MAX_EDGE_RETRIES {
                events.push(TravelEvent::Status(format!(
                    "move {from} -> {expected} keeps failing - disabling that edge and re-pathing"
                )));
                self.banned.insert((from, expected));
                self.repath(ctx.db, from, events);
            } else {
                self.edge_retries += 1;
                if !command.is_empty() && ctx.rt_remaining <= 0.0 {
                    events.push(TravelEvent::Send(command));
                }
                self.reset_arrival_timer(from, expected, ctx.now_ms);
            }
            return true;
        }
        // Blocked-but-keep → don't ban; re-path around it for now (Lich `nil`:
        // "don't delete the edge"). A future attempt may succeed.
        if ctx.saw(&F::MoveFailedKeep) {
            events.push(TravelEvent::Status(format!(
                "move {from} -> {expected} is blocked right now - re-pathing (edge kept)"
            )));
            self.repath(ctx.db, from, events);
            return true;
        }
        // Hands full → run the stash cascade, then retry the move.
        if ctx.saw(&F::HandsFull) {
            if let Some(inputs) = ctx.hands {
                let mut task = super::stash::StashTask::empty();
                for ev in task.tick(inputs.to_stash_context(ctx.now_ms)) {
                    if let super::stash::StashEvent::Send(cmd) = ev {
                        events.push(TravelEvent::Send(cmd));
                    }
                }
                self.stash = Some(task);
                self.step = Step::Stashing {
                    resume: Box::new(Step::AwaitArrival {
                        expected,
                        from,
                        sent_ms: ctx.now_ms,
                        slow: false,
                    }),
                };
            } else {
                events.push(TravelEvent::Send("empty hands".into()));
            }
            return true;
        }
        // Closed door → send the `open` variant, then retry the move.
        if ctx.saw(&F::DoorClosed) {
            let open = command.replacen("go", "open", 1).replacen("climb", "open", 1);
            events.push(TravelEvent::Send(open));
            events.push(TravelEvent::Send(command));
            self.reset_arrival_timer(from, expected, ctx.now_ms);
            return true;
        }
        // Fell / knocked down → stand, then retry.
        if ctx.saw(&F::Fell) {
            if !ctx.standing {
                events.push(TravelEvent::Send("stand".into()));
            }
            events.push(TravelEvent::Send(command));
            self.reset_arrival_timer(from, expected, ctx.now_ms);
            return true;
        }
        // Verb swaps: go <-> climb.
        if ctx.saw(&F::NeedClimb) {
            events.push(TravelEvent::Send(command.replacen("go", "climb", 1)));
            self.reset_arrival_timer(from, expected, ctx.now_ms);
            return true;
        }
        if ctx.saw(&F::CantClimb) {
            events.push(TravelEvent::Send(command.replacen("climb", "go", 1)));
            self.reset_arrival_timer(from, expected, ctx.now_ms);
            return true;
        }
        // Item at feet → stow it, then retry.
        if ctx.saw(&F::ItemAtFeet) {
            events.push(TravelEvent::Send("stow feet".into()));
            events.push(TravelEvent::Send(command));
            self.reset_arrival_timer(from, expected, ctx.now_ms);
            return true;
        }
        false
    }

    fn reset_arrival_timer(&mut self, from: u32, expected: u32, now_ms: u64) {
        self.step = Step::AwaitArrival {
            expected,
            from,
            sent_ms: now_ms,
            slow: false,
        };
    }

    /// An edge the router planned (it has a `timeto`) but we can't cross
    /// natively (its `wayto` proc doesn't transpile). Disable it for the
    /// session and re-path around it. The Lich `;go2` fallback (P6) hooks in
    /// ahead of the ban when enabled and the connection is via Lich.
    fn handle_uncrossable_edge(
        &mut self,
        current: u32,
        next: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        // With the Lich fallback enabled (Lich connection only), hand the
        // whole trip to `;go2 <dest>` rather than banning the edge.
        if ctx.lich_fallback {
            events.push(TravelEvent::Status(format!(
                "edge {current} -> {next} needs Lich - handing off to ;go2 {}",
                self.destination
            )));
            events.push(TravelEvent::LichFallback {
                destination: self.destination,
            });
            return;
        }
        events.push(TravelEvent::Status(format!(
            "edge {current} -> {next} uses a script the native walker can't cross yet - disabling it and re-pathing"
        )));
        self.banned.insert((current, next));
        self.repath(ctx.db, current, events);
    }

    fn repath(&mut self, db: &MapDb, current: u32, events: &mut Vec<TravelEvent>) {
        self.restarts += 1;
        if self.restarts > MAX_RESTARTS {
            events.push(TravelEvent::Failed(
                "too many restarts - travel aborted".into(),
            ));
            return;
        }
        // On a funding detour, a re-path heads for the BANK, not the real
        // destination — otherwise any off-route hop abandons the withdraw and
        // walks toward the (unaffordable) paid destination, which re-triggers
        // funding and loops. If we're already AT the bank, don't re-path: hand
        // straight back to the funding phase to withdraw.
        if let Some(bank) = self.funding_bank {
            if current == bank {
                self.step = Step::Funding(FundingPhase::RoutingToBank {
                    real_dest: self.destination,
                    need: self.silver_need,
                });
                return;
            }
            let banned = self.banned.clone();
            match pathing::path_to_filtered(db, current, bank, &|a, b| {
                !banned.contains(&(a, b))
            }) {
                Some(path) => {
                    self.path = path;
                    self.idx = 0;
                    self.step = Step::Funding(FundingPhase::RoutingToBank {
                        real_dest: self.destination,
                        need: self.silver_need,
                    });
                }
                None => {
                    events.push(TravelEvent::Failed(format!(
                        "no remaining route from room {current} to the bank (room {bank}) - travel aborted"
                    )));
                }
            }
            return;
        }
        let banned = self.banned.clone();
        match pathing::path_to_filtered(db, current, self.destination, &|a, b| {
            !banned.contains(&(a, b))
        }) {
            Some(path) => {
                self.path = path;
                self.idx = 0;
                self.step = Step::Prepare;
            }
            None => {
                events.push(TravelEvent::Failed(format!(
                    "no remaining route from room {current} to {} - travel aborted",
                    self.destination
                )));
            }
        }
    }

    /// A `Failed`/`Arrived` event ends the task; the owner uses this to know
    /// whether the tick's events retired it.
    pub fn is_finished(events: &[TravelEvent]) -> bool {
        events
            .iter()
            .any(|e| {
                matches!(
                    e,
                    TravelEvent::Arrived { .. }
                        | TravelEvent::Failed(_)
                        | TravelEvent::LichFallback { .. }
                )
            })
    }
}

/// go2 skips standing for swim/pedal movement commands.
fn command_is_swim_or_pedal(command: &str) -> bool {
    let lower = command.to_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|word| word == "swim" || word == "pedal")
}

/// Pick a random compass exit for the Confluence explorer's move-failed
/// fallback (Ruby: `look` the compass, `move options[rand(length)]`). `_room`
/// is unused but kept for a possible future seed; the pick is uniform-random
/// over the live `<dir>` values.
fn pick_random_exit(compass_dirs: &[String], _room: u32) -> Option<String> {
    use rand::seq::IndexedRandom;
    compass_dirs.choose(&mut rand::rng()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chain 1→2→3→4 with 0.2s edges, plus a slow alternate 1→5→3.
    fn db() -> MapDb {
        MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[R1]"],
                 "wayto": {"2": "north", "5": "east"}, "timeto": {"2": 0.2, "5": 5.0}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[R2]"],
                 "wayto": {"1": "south", "3": "north"}, "timeto": {"1": 0.2, "3": 0.2}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[R3]"],
                 "wayto": {"2": "south", "4": "swim river"}, "timeto": {"2": 0.2, "4": 0.2}, "paths": ""},
                {"id": 4, "uid": [9000004], "location": "T", "title": ["[R4]"],
                 "wayto": {"3": "swim back"}, "timeto": {"3": 0.2}, "paths": ""},
                {"id": 5, "uid": [9000005], "location": "T", "title": ["[R5]"],
                 "wayto": {"1": "west", "3": "north"}, "timeto": {"1": 5.0, "3": 5.0}, "paths": ""}
            ]"#,
        )
        .unwrap()
    }

    struct Sim {
        current: u32,
        standing: bool,
        sitting: bool,
        kneeling: bool,
        muckled: bool,
        dead: bool,
        spells: Vec<u16>,
        rt: f64,
        now: u64,
        pathcodes: std::collections::BTreeMap<String, Vec<String>>,
        feedback: Vec<crate::core::move_feedback::MoveFeedback>,
        lich_fallback: bool,
        funding: Option<FundingInputs>,
        pinefar: bool,
        compass_dirs: Vec<String>,
        loot_nouns: Vec<String>,
    }

    impl Sim {
        fn new(start: u32) -> Sim {
            Sim {
                current: start,
                standing: true,
                sitting: false,
                kneeling: false,
                muckled: false,
                dead: false,
                spells: Vec::new(),
                rt: 0.0,
                now: 0,
                pathcodes: Default::default(),
                feedback: Vec::new(),
                lich_fallback: false,
                funding: None,
                pinefar: false,
                compass_dirs: Vec::new(),
                loot_nouns: Vec::new(),
            }
        }

        fn ctx<'a>(&'a self, db: &'a MapDb) -> TravelContext<'a> {
            TravelContext {
                db,
                current_room: Some(self.current),
                dead: self.dead,
                muckled: self.muckled,
                standing: self.standing,
                sitting: self.sitting,
                kneeling: self.kneeling,
                active_spells: &self.spells,
                rt_remaining: self.rt,
                now_ms: self.now,
                pathcodes: &self.pathcodes,
                hands: None,
                feedback: &self.feedback,
                lich_fallback: self.lich_fallback,
                funding: self.funding,
                at_pinefar_depository: self.pinefar,
                compass_dirs: &self.compass_dirs,
                loot_nouns: &self.loot_nouns,
                day_pass: None,
            }
        }
    }

    /// Drive the task, applying every Send as an instant successful move.
    /// Returns the full event log.
    fn walk_to_completion(db: &MapDb, task: &mut TravelTask, sim: &mut Sim) -> Vec<TravelEvent> {
        let mut log = Vec::new();
        for _ in 0..200 {
            let events = task.tick(sim.ctx(db));
            for event in &events {
                if let TravelEvent::Send(cmd) = event {
                    if cmd == "stand" {
                        sim.standing = true;
                    } else if let Some(room) = db.room(sim.current) {
                        // Find which neighbor this command walks into.
                        if let Some((&dest, _)) =
                            room.wayto.iter().find(|(_, c)| c.as_str() == cmd)
                        {
                            sim.current = dest;
                        }
                    }
                }
            }
            let finished = TravelTask::is_finished(&events);
            log.extend(events);
            if finished {
                break;
            }
            sim.now += 100;
        }
        log
    }

    fn sent(log: &[TravelEvent]) -> Vec<&str> {
        log.iter()
            .filter_map(|e| match e {
                TravelEvent::Send(c) => Some(c.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn walks_the_shortest_path_and_reports_arrival() {
        let db = db();
        let mut task = TravelTask::start(&db, 1, 4, 0).unwrap();
        assert_eq!(task.rooms_total(), 3); // 2, 3, 4
        let mut sim = Sim::new(1);
        let log = walk_to_completion(&db, &mut task, &mut sim);
        assert_eq!(sent(&log), ["north", "north", "swim river"]);
        assert!(matches!(
            log.last(),
            Some(TravelEvent::Arrived { destination: 4, .. })
        ));
    }

    #[test]
    fn waits_for_rt_and_muckled_and_stands_first() {
        let db = db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.standing = false;
        sim.muckled = true;

        // Muckled: nothing but one status line.
        let events = task.tick(sim.ctx(&db));
        assert!(matches!(events.as_slice(), [TravelEvent::Status(_)]));
        assert!(task.tick(sim.ctx(&db)).is_empty(), "status not repeated");

        // Free but in RT: still waiting.
        sim.muckled = false;
        sim.rt = 3.0;
        assert!(task.tick(sim.ctx(&db)).is_empty());

        // RT over: stands before moving.
        sim.rt = 0.0;
        let events = task.tick(sim.ctx(&db));
        assert_eq!(events, vec![TravelEvent::Send("stand".into())]);
        sim.standing = true;
        let events = task.tick(sim.ctx(&db));
        assert_eq!(events, vec![TravelEvent::Send("north".into())]);
    }

    #[test]
    fn swim_edges_skip_the_stand_dance() {
        let db = db();
        let mut task = TravelTask::start(&db, 3, 4, 0).unwrap();
        let mut sim = Sim::new(3);
        sim.standing = false;
        let events = task.tick(sim.ctx(&db));
        assert_eq!(events, vec![TravelEvent::Send("swim river".into())]);
    }

    #[test]
    fn failing_edge_gets_banned_and_the_trip_repaths() {
        let db = db();
        // Route 1→2→3: the 1→2 edge will never actually move us.
        let mut task = TravelTask::start(&db, 1, 3, 0).unwrap();
        let mut sim = Sim::new(1);

        let mut sends = 0;
        let mut log = Vec::new();
        for _ in 0..400 {
            let events = task.tick(sim.ctx(&db));
            for event in &events {
                if let TravelEvent::Send(cmd) = event {
                    sends += 1;
                    // Only the slow detour edges actually work.
                    if cmd == "east" {
                        sim.current = 5;
                    } else if sim.current == 5 && cmd == "north" {
                        sim.current = 3;
                    }
                }
            }
            let finished = TravelTask::is_finished(&events);
            log.extend(events);
            if finished {
                break;
            }
            sim.now += 1000;
        }
        // 1 first try + 2 retries on the broken edge, then the detour.
        assert!(sends >= 5, "retries then detour, got {sends} sends");
        assert!(
            log.iter().any(
                |e| matches!(e, TravelEvent::Status(s) if s.contains("disabling that edge"))
            ),
            "edge ban should be announced"
        );
        assert!(matches!(
            log.last(),
            Some(TravelEvent::Arrived { destination: 3, .. })
        ));
    }

    #[test]
    fn wandering_off_route_repaths_and_death_aborts() {
        let db = db();
        let mut task = TravelTask::start(&db, 1, 4, 0).unwrap();
        let mut sim = Sim::new(1);

        // First move fires (north → room 2 expected)…
        let events = task.tick(sim.ctx(&db));
        assert_eq!(events, vec![TravelEvent::Send("north".into())]);
        // …but the character ends up in room 5 instead (fled).
        sim.current = 5;
        sim.now += 100;
        let events = task.tick(sim.ctx(&db));
        assert!(
            matches!(&events[..], [TravelEvent::Status(s)] if s.contains("re-pathing")),
            "{events:?}"
        );
        // The new route leaves from room 5.
        sim.now += 100;
        let events = task.tick(sim.ctx(&db));
        assert_eq!(events, vec![TravelEvent::Send("north".into())]); // 5 → 3

        sim.dead = true;
        let events = task.tick(sim.ctx(&db));
        assert!(matches!(&events[..], [TravelEvent::Failed(_)]));
    }

    /// Scripted edges: 1 → 2 via "fput; move", 2 → 3 via a checkspell
    /// branch, plus a paused turnstile edge 3 → 4.
    fn scripted_db() -> MapDb {
        MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9100001], "location": "T", "title": ["[S1]"],
                 "wayto": {"2": ";e fput 'open door'; move 'go door'"},
                 "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [9100002], "location": "T", "title": ["[S2]"],
                 "wayto": {"3": ";e if checkspell(103) then move 'go mist' else move 'go arch' end; waitrt?"},
                 "timeto": {"3": 0.2}, "paths": ""},
                {"id": 3, "uid": [9100003], "location": "T", "title": ["[S3]"],
                 "wayto": {"4": ";e pause 0.5; waitrt?; fput 'go turnstile'"},
                 "timeto": {"4": 0.2}, "paths": ""},
                {"id": 4, "uid": [9100004], "location": "T", "title": ["[S4]"],
                 "wayto": {}, "timeto": {}, "paths": ""}
            ]"#,
        )
        .unwrap()
    }

    #[test]
    fn scripted_edges_run_their_transpiled_actions() {
        let db = scripted_db();
        let mut task = TravelTask::start(&db, 1, 3, 0).unwrap();
        assert_eq!(task.rooms_total(), 2, "proc edges are routable");
        let mut sim = Sim::new(1);

        // fput + move fire together, then the executor waits for the room.
        let events = task.tick(sim.ctx(&db));
        assert_eq!(
            sent(&events),
            ["open door", "go door"],
            "script sends both commands"
        );
        sim.current = 2;
        sim.now += 100;
        task.tick(sim.ctx(&db)); // arrival → next edge

        // Spell 103 inactive: else branch.
        sim.now += 100;
        let events = task.tick(sim.ctx(&db));
        assert_eq!(sent(&events), ["go arch"]);
        sim.current = 3;
        sim.now += 100;
        let events = task.tick(sim.ctx(&db));
        assert!(matches!(
            events.last(),
            Some(TravelEvent::Arrived { destination: 3, .. })
        ));

        // Same edge with the spell active: then branch.
        let mut task = TravelTask::start(&db, 2, 3, 0).unwrap();
        let mut sim = Sim::new(2);
        sim.spells = vec![103];
        let events = task.tick(sim.ctx(&db));
        assert_eq!(sent(&events), ["go mist"]);
    }

    #[test]
    fn footpath_edge_drives_the_stash_service_end_to_end() {
        use crate::core::game_objects::{GameItem, GameObjects, Hand};
        // Room 1 -> 2 is the footpath: empty_hands; move 'climb footpath';
        // fill_hands, with a numeric timeto so it routes.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[West Road]"],
                 "wayto": {"2": ";e empty_hands; move 'climb footpath'; fill_hands"},
                 "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Footpath Top]"],
                 "wayto": {"1": "climb down"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        assert_eq!(task.rooms_total(), 1, "footpath edge is routable");

        // A player holding a torch, with a lootsack container.
        let mut objs = GameObjects::default();
        objs.set_hand(Hand::Left, Some(GameItem::new("10", "torch", "a torch")));
        let others: Vec<String> = vec![];
        let pathcodes: std::collections::BTreeMap<String, Vec<String>> = Default::default();

        fn ctx<'a>(
            objs: &'a GameObjects,
            others: &'a [String],
            pathcodes: &'a std::collections::BTreeMap<String, Vec<String>>,
            now: u64,
            db: &'a MapDb,
        ) -> TravelContext<'a> {
            let hands = StashInputs {
                left_hand: objs.hand(Hand::Left),
                right_hand: objs.hand(Hand::Right),
                ready_stow: objs.ready_stow(),
                weaponsack: None,
                lootsack: Some("99"),
                other_containers: others,
                left_bandolier: None,
                right_bandolier: None,
                left_is_weapon: false,
                right_is_weapon: false,
            };
            TravelContext {
                db,
                current_room: Some(1),
                dead: false,
                muckled: false,
                standing: true,
                sitting: false,
                kneeling: false,
                active_spells: &[],
                rt_remaining: 0.0,
                now_ms: now,
                pathcodes,
                hands: Some(hands),
                feedback: &[],
                lich_fallback: false,
                funding: None,
                at_pinefar_depository: false,
                compass_dirs: &[],
                loot_nouns: &[],
                day_pass: None,
            }
        }

        // Tick 1: EmptyHands runs — stows the torch into the lootsack.
        let events = task.tick(ctx(&objs, &others, &pathcodes, 0, &db));
        assert_eq!(
            sent(&events),
            ["_drag #10 #99"],
            "empty_hands stows the held torch"
        );
        // The stow confirms (hand clears).
        objs.set_hand(Hand::Left, None);

        // Tick 2: hands empty -> the climb fires, then the script's FillHands
        // runs (hands already empty of the climb's making) and retrieves the
        // torch. Both commands queue to the game in order — the same sequence
        // ;go2 sends: empty_hands, climb, fill_hands.
        let events = task.tick(ctx(&objs, &others, &pathcodes, 100, &db));
        assert_eq!(
            sent(&events),
            ["climb footpath", "get #10"],
            "the climb, then fill_hands retrieves the torch"
        );
        objs.set_hand(Hand::Left, Some(GameItem::new("10", "torch", "a torch")));

        // Arrival at room 2 finishes the edge.
        // (Script is complete; the executor is now awaiting the room change.)
    }

    #[test]
    fn arrival_at_destination_waits_for_fill_hands_to_finish() {
        use crate::core::game_objects::{GameItem, GameObjects, Hand};
        // The live footpath bug: the final edge is empty_hands; climb; fill_hands
        // and the climb LANDS at the destination while fill_hands is still
        // retrieving the SECOND of two stowed items. The trip must NOT end until
        // the refill completes — otherwise an item is stranded in the pack.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[West Road]"],
                 "wayto": {"2": ";e empty_hands; move 'climb footpath'; fill_hands"},
                 "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Footpath Top]"],
                 "wayto": {"1": "climb down"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();

        // Two held items: a wand (left) and a pick (right).
        let mut objs = GameObjects::default();
        objs.set_hand(Hand::Left, Some(GameItem::new("18", "wand", "a wand")));
        objs.set_hand(Hand::Right, Some(GameItem::new("4", "pick", "a pick")));
        let others: Vec<String> = vec![];
        let pathcodes: std::collections::BTreeMap<String, Vec<String>> = Default::default();

        // Build the TravelContext inline per tick (assembling StashInputs in a
        // closure tangles borrow lifetimes, so do it in the macro directly).
        macro_rules! ctx {
            ($objs:expr, $now:expr, $cur:expr) => {{
                let hands = StashInputs {
                    left_hand: $objs.hand(Hand::Left),
                    right_hand: $objs.hand(Hand::Right),
                    ready_stow: $objs.ready_stow(),
                    weaponsack: None,
                    lootsack: Some("99"),
                    other_containers: &others,
                    left_bandolier: None,
                    right_bandolier: None,
                    left_is_weapon: false,
                    right_is_weapon: false,
                };
                TravelContext {
                    db: &db,
                    current_room: Some($cur),
                    dead: false,
                    muckled: false,
                    standing: true,
                    sitting: false,
                    kneeling: false,
                    active_spells: &[],
                    rt_remaining: 0.0,
                    now_ms: $now,
                    pathcodes: &pathcodes,
                    hands: Some(hands),
                    feedback: &[],
                    lich_fallback: false,
                    funding: None,
                    at_pinefar_depository: false,
                    compass_dirs: &[],
                    loot_nouns: &[],
                    day_pass: None,
                }
            }};
        }

        // Tick 1: empty_hands stows the first hand (left wand).
        assert_eq!(sent(&task.tick(ctx!(&objs, 0, 1))), ["_drag #18 #99"]);
        objs.set_hand(Hand::Left, None);
        // Tick 2: stows the second hand (right pick).
        assert_eq!(sent(&task.tick(ctx!(&objs, 100, 1))), ["_drag #4 #99"]);
        objs.set_hand(Hand::Right, None);
        // Tick 3: hands empty -> climb, then fill_hands sends the FIRST retrieve
        // (LIFO: the pick, stowed last, comes back first).
        let ev = task.tick(ctx!(&objs, 200, 1));
        assert_eq!(sent(&ev), ["climb footpath", "get #4"], "climb + first retrieve: {ev:?}");
        // The climb LANDS us at the destination (room 2) NOW, while the pick
        // retrieval is still in flight and the wand hasn't been retrieved yet.
        // The pick comes back to the right hand.
        objs.set_hand(Hand::Right, Some(GameItem::new("4", "pick", "a pick")));
        let ev = task.tick(ctx!(&objs, 300, 2));
        // The trip must NOT be Arrived yet — fill_hands still owes the wand.
        assert!(
            !ev.iter().any(|e| matches!(e, TravelEvent::Arrived { .. })),
            "trip does not end mid-refill: {ev:?}"
        );
        assert_eq!(sent(&ev), ["get #18"], "retrieves the SECOND item (the wand): {ev:?}");
        // The wand comes back; the refill completes (stash cycle ends) and the
        // suspended script resumes — no more actions left.
        objs.set_hand(Hand::Left, Some(GameItem::new("18", "wand", "a wand")));
        let _ = task.tick(ctx!(&objs, 400, 2));
        // With the stash done, the next tick's arrival check fires and the trip
        // finishes — both items back in hand.
        let ev = task.tick(ctx!(&objs, 500, 2));
        assert!(
            matches!(ev.last(), Some(TravelEvent::Arrived { destination: 2, .. })),
            "now the trip finishes, both items back in hand: {ev:?}"
        );
    }

    #[test]
    fn closed_door_feedback_opens_then_retries() {
        use crate::core::move_feedback::MoveFeedback;
        // Edge 1 -> 2 is "go door".
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": "go door"}, "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[B]"],
                 "wayto": {"1": "go door"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        // Tick 1: sends the move.
        assert_eq!(sent(&task.tick(sim.ctx(&db))), ["go door"]);
        // The door is closed: feedback fires while we're still in room 1.
        sim.now += 100;
        sim.feedback = vec![MoveFeedback::DoorClosed];
        let events = task.tick(sim.ctx(&db));
        assert_eq!(
            sent(&events),
            ["open door", "go door"],
            "opens the door then retries the move"
        );
    }

    #[test]
    fn hard_failure_feedback_bans_the_edge_and_repaths() {
        use crate::core::move_feedback::MoveFeedback;
        // 1 -> 2 direct, or 1 -> 3 -> 2 around.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": "north", "3": "east"}, "timeto": {"2": 0.2, "3": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[B]"],
                 "wayto": {"1": "south"}, "timeto": {"1": 0.2}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[C]"],
                 "wayto": {"2": "north"}, "timeto": {"2": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        // Tick 1: tries the direct edge.
        assert_eq!(sent(&task.tick(sim.ctx(&db))), ["north"]);
        // "You can't go there" — but a failure line can race a lagged room
        // change or be spurious, so we RETRY the move rather than banning the
        // edge on the first hit. The retries re-send `north`.
        for _ in 0..MAX_EDGE_RETRIES {
            sim.now += 100;
            sim.feedback = vec![MoveFeedback::MoveFailedRemovable];
            assert_eq!(
                sent(&task.tick(sim.ctx(&db))),
                ["north"],
                "a failure retries the move before giving up"
            );
        }
        // Now the retries are exhausted: ban 1->2 and re-path via 3.
        sim.now += 100;
        sim.feedback = vec![MoveFeedback::MoveFailedRemovable];
        let events = task.tick(sim.ctx(&db));
        assert!(
            events.iter().any(|e| matches!(e, TravelEvent::Status(s) if s.contains("keeps failing"))),
            "repeated failure finally bans + re-paths: {events:?}"
        );
        // The re-plan sends the detour's first hop.
        sim.now += 100;
        sim.feedback.clear();
        assert_eq!(sent(&task.tick(sim.ctx(&db))), ["east"], "re-routes via room 3");
    }

    #[test]
    fn arrival_wins_over_a_raced_failure_line() {
        use crate::core::move_feedback::MoveFeedback;
        // The live Trollfang bug: `south` succeeds (room changes) but a failure
        // line races in the same tick. The nav must win — the good edge is NOT
        // banned.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": "south", "3": "east"}, "timeto": {"2": 0.2, "3": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[B]"],
                 "wayto": {"1": "north"}, "timeto": {"1": 0.2}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[C]"],
                 "wayto": {"2": "north"}, "timeto": {"2": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        assert_eq!(sent(&task.tick(sim.ctx(&db))), ["south"]);
        // A failure line AND a nav (room changed) arrive the same tick, but the
        // resolver hasn't updated current_room to 2 yet (still reads 1).
        sim.now += 100;
        sim.feedback = vec![
            MoveFeedback::MoveFailedRemovable,
            MoveFeedback::NavArrived,
        ];
        let ev = task.tick(sim.ctx(&db));
        assert!(
            !ev.iter().any(|e| matches!(e, TravelEvent::Status(s) if s.contains("disabling") || s.contains("keeps failing"))),
            "the raced failure does NOT ban the edge: {ev:?}"
        );
        assert!(
            !task.banned.contains(&(1, 2)),
            "edge 1->2 stays in the graph"
        );
        // The room resolves to 2 → arrival.
        sim.current = 2;
        sim.now += 100;
        sim.feedback.clear();
        let ev = task.tick(sim.ctx(&db));
        assert!(
            matches!(ev.last(), Some(TravelEvent::Arrived { destination: 2, .. })),
            "arrives normally: {ev:?}"
        );
    }

    #[test]
    fn nav_to_unresolved_room_counts_as_arrival() {
        use crate::core::move_feedback::MoveFeedback;
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": "go shop"}, "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Shop]"],
                 "wayto": {"1": "out"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        assert_eq!(sent(&task.tick(sim.ctx(&db))), ["go shop"]);
        // We stepped into an UID-less shop the resolver can't place:
        // current_room becomes None, but a <nav> fired. Trust the edge.
        sim.now += 100;
        sim.current = 2; // sim always resolves; simulate unresolved via ctx override
        // Force the "unresolved" case: build a ctx with current_room = None.
        let events = {
            let mut ctx = sim.ctx(&db);
            ctx.current_room = None;
            let fb = vec![MoveFeedback::NavArrived];
            ctx.feedback = &fb;
            task.tick(ctx)
        };
        assert!(
            events.is_empty() || !events.iter().any(|e| matches!(e, TravelEvent::Failed(_))),
            "a nav into an unresolved room advances rather than failing"
        );
    }

    #[test]
    fn uncrossable_edge_hands_off_to_lich_when_fallback_on() {
        // A proc edge we can't transpile, but with a numeric timeto so it
        // routes (post-P1). Only a route through it exists.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": ";e some_confluence_thing_we_cannot_parse"},
                 "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[B]"],
                 "wayto": {"1": "back"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.lich_fallback = true;
        let events = task.tick(sim.ctx(&db));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TravelEvent::LichFallback { destination: 2 })),
            "hands off to ;go2 2: {events:?}"
        );
        assert!(TravelTask::is_finished(&events), "the fallback ends the task");
    }

    #[test]
    fn uncrossable_edge_bans_and_repaths_when_fallback_off() {
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": ";e cannot_parse", "3": "east"},
                 "timeto": {"2": 0.2, "3": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[B]"],
                 "wayto": {"1": "back"}, "timeto": {"1": 0.2}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[C]"],
                 "wayto": {"2": "north"}, "timeto": {"2": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1); // fallback off
        let events = task.tick(sim.ctx(&db));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, TravelEvent::LichFallback { .. })),
            "no fallback when off"
        );
        // It re-paths around via room 3.
        sim.now += 100;
        assert_eq!(sent(&task.tick(sim.ctx(&db))), ["east"], "re-routes natively");
    }

    fn funding_db() -> MapDb {
        // Room 1 (dock) -> 2 (far port) costs 25000 silver. Room 1 -> 3 is a
        // free walk to a bank.
        MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Dock]"],
                 "tags": ["silver-cost:2:25000"],
                 "wayto": {"2": "board", "3": "west"}, "timeto": {"2": 1.0, "3": 0.2},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Far Port]"],
                 "wayto": {"1": "board"}, "timeto": {"1": 1.0}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[Bank, Teller]"],
                 "tags": ["bank"], "wayto": {"1": "east"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap()
    }

    fn fund(silver: Option<u64>, get_silvers: bool) -> FundingInputs {
        FundingInputs { silver, get_silvers, get_return_trip: false }
    }

    #[test]
    fn funded_trip_walks_after_wealth_check() {
        let db = funding_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.funding = Some(fund(None, false));
        // Tick 1: funding pre-flight sends `wealth quiet`.
        assert_eq!(sent(&task.tick(sim.ctx(&db))), ["wealth quiet"]);
        // The wealth line lands: 40000 silver, enough for the 25000 trip.
        sim.now += 100;
        sim.funding = Some(fund(Some(40000), false));
        // Next tick: funded → the trip walks (sends the board move).
        let events = task.tick(sim.ctx(&db));
        assert_eq!(sent(&events), ["board"], "funded → walks the trip");
    }

    #[test]
    fn short_without_permission_warns_and_proceeds() {
        let db = funding_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.funding = Some(fund(None, false));
        task.tick(sim.ctx(&db)); // wealth quiet
        sim.now += 100;
        sim.funding = Some(fund(Some(100), false)); // short, no get_silvers
        let events = task.tick(sim.ctx(&db));
        assert!(
            events.iter().any(|e| matches!(e, TravelEvent::Status(s) if s.contains("short"))),
            "warns about being short"
        );
        assert_eq!(sent(&events), ["board"], "proceeds anyway");
    }

    #[test]
    fn funding_detour_to_a_bank_reached_by_urchin_guide() {
        // Reproduces Nisugi's live bug: the paid trip is short, the nearest
        // bank is reached via an urchin guide (hideout `;e true` + `urchin
        // guide bank`), and the withdraw must fire on arrival — NOT get
        // hijacked into walking toward the real (paid) destination.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Pier]"],
                 "tags": ["silver-cost:2:25000"],
                 "wayto": {"2": "ask portmaster about travel 1", "30716": ";e true"},
                 "timeto": {"2": 1.0, "30716": 0.1}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Far Port]"],
                 "wayto": {"1": "board"}, "timeto": {"1": 1.0}, "paths": ""},
                {"id": 30716, "uid": [9030716], "location": "KF", "title": ["[Hideout]"],
                 "wayto": {"1": "urchin guide pier", "3": "urchin guide bank"},
                 "timeto": {"1": 0.1, "3": 0.1}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[Bank, Teller]"],
                 "tags": ["bank"], "wayto": {"30716": ";e true"},
                 "timeto": {"30716": 0.1}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.funding = Some(fund(None, false));
        assert_eq!(sent(&task.tick(sim.ctx(&db))), ["wealth quiet"]);
        sim.now += 100;
        sim.funding = Some(fund(Some(0), true)); // broke, but get_silvers on
        // Should collapse the `;e true` hideout and send `urchin guide bank`,
        // NOT `urchin guide pier` or the portmaster ask.
        let ev = task.tick(sim.ctx(&db));
        assert_eq!(
            sent(&ev),
            ["urchin guide bank"],
            "funding routes to the bank via the urchin guide: {ev:?}"
        );
        // The guide lands us at the bank (room 3).
        sim.current = 3;
        sim.now += 100;
        task.tick(sim.ctx(&db)); // arrival → hand back to funding
        sim.now += 100;
        let ev = task.tick(sim.ctx(&db));
        // Withdraw the shortfall, then re-probe wealth (the withdraw
        // confirmation isn't a wealth line).
        assert_eq!(
            sent(&ev),
            ["withdraw 25000 silvers", "wealth quiet"],
            "withdraws + re-checks wealth, not walking off to the paid dest: {ev:?}"
        );
        // The fresh wealth reading reflects the withdrawal → continue.
        sim.now += 100;
        sim.funding = Some(fund(Some(25000), true));
        let ev = task.tick(sim.ctx(&db));
        assert!(
            ev.iter().any(|e| matches!(e, TravelEvent::Status(s) if s.contains("continuing"))),
            "funded → continues to the real destination: {ev:?}"
        );
    }

    #[test]
    fn off_route_during_funding_detour_repaths_to_the_bank_not_the_dest() {
        // Nisugi's freeze bug: an off-route hop while walking the funding
        // detour must re-target the BANK, never the (unaffordable) paid
        // destination — else it walks back toward the portmaster, re-triggers
        // funding, and loops (client froze on the wealth check).
        let db = funding_db(); // 1=dock(pays 25000 ->2), 3=bank, free walk 1->3
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.funding = Some(fund(None, false));
        task.tick(sim.ctx(&db)); // wealth quiet
        sim.now += 100;
        sim.funding = Some(fund(Some(0), true)); // broke, get_silvers on
        // Redirect to the bank (room 3): sends the walk toward it.
        let ev = task.tick(sim.ctx(&db));
        assert_eq!(sent(&ev), ["west"], "routes toward the bank: {ev:?}");
        assert_eq!(task.funding_bank, Some(3));
        // Force an off-route re-path directly (the executor path that Nisugi's
        // live off-route hop hit). It must re-target the BANK, not the paid
        // destination (2): funding_bank stays set and the new path ends at the
        // bank, never routing toward the portmaster edge.
        let mut ev = Vec::new();
        task.repath(&db, 1, &mut ev);
        assert_eq!(task.funding_bank, Some(3), "still targeting the bank");
        assert_eq!(
            task.path.last(),
            Some(&3),
            "re-path aims at the bank (room 3), not the paid dest: path={:?}",
            task.path
        );
        assert!(
            matches!(task.step, Step::Funding(FundingPhase::RoutingToBank { .. })),
            "stays in the funding detour, not a normal walk to the dest"
        );
    }

    #[test]
    fn short_with_permission_routes_to_bank_and_withdraws() {
        let db = funding_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.funding = Some(fund(None, false));
        task.tick(sim.ctx(&db)); // wealth quiet
        sim.now += 100;
        sim.funding = Some(fund(Some(100), true)); // short, but get_silvers on
        // Redirects to the bank (room 3) and starts walking there.
        let events = task.tick(sim.ctx(&db));
        assert_eq!(sent(&events), ["west"], "routes toward the bank");
        // Arrive at the bank (this tick registers arrival + hands back to
        // the funding phase).
        sim.current = 3;
        sim.now += 100;
        task.tick(sim.ctx(&db));
        // Next tick at the bank: withdraw the shortfall.
        sim.now += 100;
        let events = task.tick(sim.ctx(&db));
        assert_eq!(
            sent(&events),
            ["withdraw 24900 silvers", "wealth quiet"],
            "withdraws the shortfall (25000 - 100) then re-checks wealth"
        );
        // The withdrawal reflects: now 25000 silver → re-plan to the real dest.
        sim.now += 100;
        sim.funding = Some(fund(Some(25000), true));
        let events = task.tick(sim.ctx(&db));
        assert!(
            events.iter().any(|e| matches!(e, TravelEvent::Status(s) if s.contains("continuing"))),
            "funded → continues to the real destination: {events:?}"
        );
    }

    #[test]
    fn broke_while_standing_in_the_bank_withdraws_here() {
        // Nisugi's live bug: logged in AT the bank teller (room 3), broke, then
        // a paid trip. Funding must withdraw right here, not "lose the route to
        // the bank" because path_to(bank, bank) is empty.
        let db = funding_db(); // room 3 is bank-tagged; 1 pays 25000 -> 2
        // Plan a paid trip starting FROM the bank room (3). Route 3 -> 2 goes
        // 3 -> 1 (east) -> 2 (board, the paid edge).
        let mut task = TravelTask::start(&db, 3, 2, 0).unwrap();
        let mut sim = Sim::new(3);
        sim.funding = Some(fund(None, false));
        assert_eq!(sent(&task.tick(sim.ctx(&db))), ["wealth quiet"]);
        sim.now += 100;
        sim.funding = Some(fund(Some(0), true)); // broke, get_silvers on
        // Must NOT abort — the nearest bank IS the current room, so it
        // withdraws in place this same tick (no walk needed).
        let ev = task.tick(sim.ctx(&db));
        assert!(
            !ev.iter().any(|e| matches!(e, TravelEvent::Failed(s) if s.contains("lost the route"))),
            "does not abort when already at the bank: {ev:?}"
        );
        assert!(
            sent(&ev).iter().any(|c| c.starts_with("withdraw")),
            "withdraws in place right here: {ev:?}"
        );
    }

    #[test]
    fn scripted_sleep_actually_waits() {
        let db = scripted_db();
        let mut task = TravelTask::start(&db, 3, 4, 0).unwrap();
        let mut sim = Sim::new(3);

        // pause 0.5: nothing sends until the clock passes the wake time.
        assert!(sent(&task.tick(sim.ctx(&db))).is_empty());
        sim.now = 200;
        assert!(sent(&task.tick(sim.ctx(&db))).is_empty());
        sim.now = 600;
        let events = task.tick(sim.ctx(&db));
        assert_eq!(sent(&events), ["go turnstile"]);
    }

    #[test]
    fn already_there_reports_arrival_immediately() {
        let db = db();
        // start() from elsewhere, but the character is standing at the
        // destination by the first tick.
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(2);
        sim.now = 1500;
        let events = task.tick(sim.ctx(&db));
        assert_eq!(
            events,
            vec![TravelEvent::Arrived {
                destination: 2,
                seconds: 1.5
            }]
        );
    }

    /// Synthetic map around the shipped Ranger Guild maze definition (its
    /// room ids are real so the static maze table matches): town 100 →
    /// entrance 20886 → maze (15606 → 19415) → guild side 30870. Like the
    /// real data, the maze/guild edges carry NO timeto — the graph cannot
    /// route to 30870, so planning must fall back to the maze entrance.
    /// The route commands are still wayto edges so the Sim's command→room
    /// mapping walks them.
    fn maze_db() -> MapDb {
        MapDb::from_json(
            r#"[
                {"id": 100, "uid": [9100], "location": "T",
                 "title": ["[Town]"], "wayto": {"20886": "south"},
                 "timeto": {"20886": 0.2}, "paths": "Obvious paths: south"},
                {"id": 20886, "uid": [279900], "location": "T",
                 "title": ["[Entry Path]"],
                 "wayto": {"100": "out", "15606": "north"},
                 "timeto": {"100": 0.2, "15606": 0.2},
                 "paths": "Obvious paths: north, out"},
                {"id": 15606, "uid": [279901], "location": "T",
                 "title": ["[Jungle Approach]"],
                 "wayto": {"19415": "go clearing"},
                 "timeto": {}, "paths": "Obvious paths: north"},
                {"id": 19415, "uid": [279999], "location": "T",
                 "title": ["[Jungle Approach]"],
                 "wayto": {"30870": "west"},
                 "timeto": {}, "paths": "Obvious paths: west"},
                {"id": 30870, "uid": [279004], "location": "T",
                 "title": ["[Teak Tree Grove]"],
                 "wayto": {"19415": "east"},
                 "timeto": {}, "paths": "Obvious exits: east"}
            ]"#,
        )
        .unwrap()
    }

    #[test]
    fn maze_walks_the_stored_pathcode_and_arrives() {
        let db = maze_db();
        let mut task = TravelTask::start(&db, 100, 30870, 0).unwrap();
        let mut sim = Sim::new(100);
        sim.pathcodes.insert(
            "ranger-guild-mist-harbor".into(),
            vec!["go clearing".into(), "west".into()],
        );
        let log = walk_to_completion(&db, &mut task, &mut sim);

        let sends: Vec<&str> = log
            .iter()
            .filter_map(|e| match e {
                TravelEvent::Send(c) => Some(c.as_str()),
                _ => None,
            })
            .collect();
        // Normal walk to the entrance, then entrance→start, then the code.
        assert_eq!(sends, ["south", "north", "go clearing", "west"]);
        assert!(
            log.iter().any(|e| matches!(
                e,
                TravelEvent::Arrived { destination: 30870, .. }
            )),
            "maze walk reaches the guild side: {log:?}"
        );
        // The junk maze edges were never re-pathed through.
        assert!(!log.iter().any(|e| matches!(
            e,
            TravelEvent::Status(s) if s.contains("re-pathing")
        )));
    }

    #[test]
    fn maze_without_pathcode_asks_then_times_out_cleanly() {
        let db = maze_db();
        let mut task = TravelTask::start(&db, 100, 30870, 0).unwrap();
        let mut sim = Sim::new(100);
        let log = walk_to_completion(&db, &mut task, &mut sim);

        assert!(
            log.iter().any(|e| matches!(
                e,
                TravelEvent::Send(c) if c == "ask beyor about path"
            )),
            "the NPC gets asked automatically: {log:?}"
        );
        // No capture layer in this harness, so the wait must end in a clear
        // failure rather than hanging or thrashing.
        assert!(log.iter().any(|e| matches!(
            e,
            TravelEvent::Failed(s) if s.contains("no pathcode heard")
        )));
    }

    // ---- Transport rooms (portmaster ships, ferries): wait + recalc --------

    #[test]
    fn waits_aboard_transport_then_repaths_on_arrival() {
        // Pier (1) boards a ship via the portmaster; the ship (2) is a
        // meta:transport room with no exits — the game sails it. It deposits us
        // at a landing (3) that ISN'T the portmaster edge's nominal dest, and
        // from there we walk to the real destination (4).
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Pier]"],
                 "wayto": {"9": "ask portmaster about travel 1"}, "timeto": {"9": 1.0},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Sloop, Ocean Voyage]"],
                 "tags": ["meta:transport"], "wayto": {}, "timeto": {}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[Landing]"],
                 "wayto": {"4": "north"}, "timeto": {"4": 0.2}, "paths": ""},
                {"id": 4, "uid": [9000004], "location": "T", "title": ["[Town Square]"],
                 "wayto": {"3": "south"}, "timeto": {"3": 0.2}, "paths": ""},
                {"id": 9, "uid": [9000009], "location": "T", "title": ["[Far Dock]"],
                 "wayto": {"3": "off"}, "timeto": {"3": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 4, 0).unwrap();
        let mut sim = Sim::new(1);
        // Board: sends the portmaster ask, awaits arrival at the edge dest (9).
        let ev = task.tick(sim.ctx(&db));
        assert!(sent(&ev).iter().any(|c| c.contains("portmaster")), "boards: {ev:?}");
        // The escort puts us on the SHIP (room 2, meta:transport) first.
        sim.current = 2;
        sim.now += 100;
        let ev = task.tick(sim.ctx(&db));
        assert!(
            ev.iter().any(|e| matches!(e, TravelEvent::Status(s) if s.contains("aboard a transport"))),
            "waits aboard the transport: {ev:?}"
        );
        assert!(sent(&ev).is_empty(), "sends nothing while sailing: {ev:?}");
        // Still sailing next tick — still silent.
        sim.now += 5_000;
        assert!(sent(&task.tick(sim.ctx(&db))).is_empty(), "still waiting aboard");
        // The ride drops us at the landing (3), a normal room → re-plan to the
        // real destination (4) and walk.
        sim.current = 3;
        sim.now += 100;
        let ev = task.tick(sim.ctx(&db));
        assert!(
            ev.iter().any(|e| matches!(e, TravelEvent::Status(s) if s.contains("off the transport"))),
            "re-paths on leaving the transport: {ev:?}"
        );
        // Then it walks the final leg to the destination.
        let log = walk_to_completion(&db, &mut task, &mut sim);
        assert!(
            matches!(log.last(), Some(TravelEvent::Arrived { destination: 4, .. })),
            "reaches the real destination: {log:?}"
        );
    }

    // ---- Chronomage day-pass crossing --------------------------------------

    #[test]
    fn day_pass_use_crossing_raises_a_held_pass() {
        use crate::core::day_pass::DayPassCache;
        // Edge 8635 (Wehnimer's departure) -> 8916 (Icemule), a real day-pass
        // edge. A valid Icemule/Wehnimer's pass (#77) is held in sack #99.
        let db = MapDb::from_json(
            r#"[
                {"id": 8635, "uid": [98635], "location": "T", "title": ["[Wehnimer's Shop]"],
                 "wayto": {"8916": ";e day_pass_proc"}, "timeto": {"8916": 0.8}, "paths": ""},
                {"id": 8916, "uid": [98916], "location": "T", "title": ["[Icemule Loci]"],
                 "wayto": {"8635": ";e day_pass_proc"}, "timeto": {"8635": 0.8}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 8635, 8916, 0).unwrap();
        let mut cache = DayPassCache::default();
        cache.parse_line(r#"This <a exist="77" noun="pass">pass</a> entitles the original purchaser to one (1) day of unlimited travel between the towns of Icemule Trace and Wehnimer's Landing, commencing now."#);
        cache.parse_line("[Your pass will expire on Fri Aug 22 14:30:00 ET 2038.");

        macro_rules! dpctx {
            ($now:expr, $cur:expr) => {{
                let dp = DayPassInputs {
                    sack_id: Some("99"),
                    buy_day_pass: "",
                    get_silvers: false,
                    cache: &cache,
                    now_epoch: 0,
                };
                TravelContext {
                    db: &db, current_room: Some($cur), dead: false, muckled: false,
                    standing: true, sitting: false, kneeling: false, active_spells: &[],
                    rt_remaining: 0.0, now_ms: $now, pathcodes: &Default::default(),
                    hands: None, feedback: &[], lich_fallback: false, funding: None,
                    at_pinefar_depository: false, compass_dirs: &[], loot_nouns: &[],
                    day_pass: Some(dp),
                }
            }};
        }

        // The crossing runs as a WalkAction script through tick_script: open
        // sack, EmptyHands (our stash primitive — bare `stow both` here since
        // the test wires no hands), get the held pass, raise it, put it back,
        // FillHands (`get both`), close.
        let ev = task.tick(dpctx!(0, 8635));
        let all: Vec<String> = sent(&ev).iter().map(|s| s.to_string()).collect();
        assert_eq!(
            all,
            vec![
                "open #99",
                "stow both", // EmptyHands primitive (no hands wired → fallback)
                "get #77",
                "raise #77",
                "_drag #77 #99",
                "get both", // FillHands primitive
                "close #99",
            ],
            "USE crossing uses the stash primitives + raises the held pass: {ev:?}"
        );
        // Crucially: NOT a raw `empty hands` string, and no ask/withdraw.
        assert!(!all.iter().any(|c| c == "empty hands"), "no invalid `empty hands` cmd");
        assert!(!all.iter().any(|c| c.contains("ask ") || c.contains("withdraw")));
        // The raise lands us at 8916 → arrival finishes the trip.
        let ev = task.tick(dpctx!(2_000, 8916));
        assert!(
            matches!(ev.last(), Some(TravelEvent::Arrived { destination: 8916, .. })),
            "arrives at the destination: {ev:?}"
        );
    }

    #[test]
    fn day_pass_buy_crossing_asks_clerk_and_funds() {
        use crate::core::day_pass::DayPassCache;
        // Same edge but NO held pass; buying enabled for wl,imt + get_silvers.
        let db = MapDb::from_json(
            r#"[
                {"id": 8635, "uid": [98635], "location": "T", "title": ["[Wehnimer's Shop]"],
                 "wayto": {"8916": ";e day_pass_proc"}, "timeto": {"8916": 4.4}, "paths": ""},
                {"id": 8916, "uid": [98916], "location": "T", "title": ["[Icemule Loci]"],
                 "wayto": {"8635": ";e day_pass_proc"}, "timeto": {"8635": 4.4}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 8635, 8916, 0).unwrap();
        let cache = DayPassCache::default(); // no passes held

        macro_rules! buyctx {
            ($now:expr, $cur:expr) => {{
                let dp = DayPassInputs {
                    sack_id: Some("99"),
                    buy_day_pass: "wl,imt",
                    get_silvers: true,
                    cache: &cache,
                    now_epoch: 0,
                };
                TravelContext {
                    db: &db, current_room: Some($cur), dead: false, muckled: false,
                    standing: true, sitting: false, kneeling: false, active_spells: &[],
                    rt_remaining: 0.0, now_ms: $now, pathcodes: &Default::default(),
                    hands: None, feedback: &[], lich_fallback: false, funding: None,
                    at_pinefar_depository: false, compass_dirs: &[], loot_nouns: &[],
                    day_pass: Some(dp),
                }
            }};
        }

        // The BUY crossing paces each walk move (StepMove) — it sends the move
        // then WAITS for the room to change before the next command, so it never
        // floods (no `withdraw` before reaching the bank). Tick 1: open sack,
        // EmptyHands primitive, then the enter step-move to the clerk, and STOP.
        let ev = task.tick(buyctx!(0, 8635));
        assert_eq!(
            sent(&ev),
            ["open #99", "stow both", "south"],
            "opens sack, stows, steps to the clerk — then waits to arrive: {ev:?}"
        );
        assert!(!sent(&ev).iter().any(|c| *c == "empty hands"), "uses the stash primitive");
        // Still in 8635 (the step hasn't landed): nothing more is sent.
        assert!(sent(&task.tick(buyctx!(1_000, 8635))).is_empty(), "waits for the room change");
        // Arrive at the clerk room (any new room) → the two asks fire, then it
        // steps toward the bank and waits again.
        let ev = task.tick(buyctx!(2_000, 900));
        let s: Vec<&str> = sent(&ev);
        assert_eq!(
            s,
            ["ask clerk for icemule", "ask clerk for icemule", "up"],
            "asks the clerk (twice), then steps toward the bank: {ev:?}"
        );
        // Walk the bank path: each StepMove waits to arrive before the next.
        // Drive ticks, giving a fresh room each time the previous step landed,
        // and record when `withdraw 5000` appears relative to the moves.
        let mut all: Vec<String> = Vec::new();
        let mut room = 902u32;
        let mut moves_before_withdraw = 0usize;
        let mut withdrawn = false;
        for i in 0..30 {
            // Advance to a new room each tick so a pending StepMove resolves.
            room += 1;
            let ev = task.tick(buyctx!(3_000 + i * 1_000, room));
            for c in sent(&ev) {
                if c == "withdraw 5000" {
                    withdrawn = true;
                } else if !withdrawn && (c.len() <= 4 || c.starts_with("go ") || c == "out") {
                    moves_before_withdraw += 1;
                }
                all.push(c.to_string());
            }
            if all.iter().any(|c| c == "raise pass") {
                break;
            }
        }
        assert!(withdrawn, "withdraws at the bank during the buy walk: {all:?}");
        // The withdraw came AFTER walking (there were bank-walk moves first).
        assert!(moves_before_withdraw >= 1, "walks toward the bank before withdrawing: {all:?}");
        assert!(all.iter().any(|c| c == "raise pass"), "eventually raises the bought pass: {all:?}");
    }

    // ---- Pass-through (`;e true`) edges: virtual urchin hideouts ------------

    #[test]
    fn true_passthrough_edge_advances_without_arrival_watch() {
        // The urchin geography: a bank (1) enters a VIRTUAL hideout (2) via a
        // `;e true` no-op, then `urchin guide town` (on 2→3) jumps to the real
        // destination (3). The hideout is never physically occupied, so the
        // no-op entry must NOT arrival-watch for room 2 (that would time out
        // and ban the edge — the live "move 3637 -> 30718 keeps failing" bug).
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Bank]"],
                 "wayto": {"2": ";e true"}, "timeto": {"2": 0.1}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Hideout]"],
                 "wayto": {"3": "urchin guide town"}, "timeto": {"3": 0.1}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[Town]"],
                 "wayto": {}, "timeto": {}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 3, 0).unwrap();
        let mut sim = Sim::new(1);
        // Tick 1: at room 1, the `;e true` entry collapses -> sends the
        // hideout's `urchin guide town` while still standing at room 1.
        let ev1 = task.tick(sim.ctx(&db));
        assert_eq!(sent(&ev1), ["urchin guide town"], "collapses to the guide: {ev1:?}");
        // The guide jumps us straight to the real destination (room 3),
        // skipping the virtual hideout (room 2) entirely.
        sim.current = 3;
        sim.now += 100;
        let ev2 = task.tick(sim.ctx(&db));
        assert!(
            matches!(ev2.last(), Some(TravelEvent::Arrived { destination: 3, .. })),
            "arrives at the real destination: {ev2:?}"
        );
    }

    // ---- Confluence explorer ------------------------------------------------

    /// Room 1 has an entry edge into the Plane's first room (23282). Beyond the
    /// threshold the explorer takes over (db edges inside the zone are junk).
    fn confluence_db() -> MapDb {
        // Routing 1 → 1005 must pass through the Plane: room 1 enters at 23282,
        // and the (nominal) confluence exit edge 23282 → 1005 completes it. The
        // explorer replaces the in-zone walk entirely.
        MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Edge]"],
                 "wayto": {"23282": "go rift"}, "timeto": {"23282": 0.2}, "paths": ""},
                {"id": 23282, "uid": [9023282], "location": "P", "title": ["[Plane]"],
                 "wayto": {"1005": "confluence"}, "timeto": {"1005": 0.2}, "paths": ""},
                {"id": 1005, "uid": [9001005], "location": "T", "title": ["[Out]"],
                 "wayto": {}, "timeto": {}, "paths": ""}
            ]"#,
        )
        .unwrap()
    }

    fn strs(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn entering_the_plane_starts_the_explorer() {
        let db = confluence_db();
        // Destination 1005 is nominal; the entry edge steps into 23282.
        let mut task = TravelTask::start(&db, 1, 1005, 0).unwrap();
        let mut sim = Sim::new(1);
        let events = task.tick(sim.ctx(&db));
        // We're outside, the next planned room is the zone → walk the entry edge.
        assert!(
            sent(&events).contains(&"go rift"),
            "walks the entry edge into the Plane: {events:?}"
        );
        assert!(matches!(task.step, Step::Confluence { .. }));
        sim.current = 23282;
    }

    #[test]
    fn explorer_walks_an_untraversed_exit_then_learns_it() {
        let db = confluence_db();
        let mut task = TravelTask::start(&db, 1, 1005, 0).unwrap();
        let mut sim = Sim::new(1);
        task.tick(sim.ctx(&db)); // enter
        sim.current = 23282;
        sim.compass_dirs = strs(&["north"]);
        // First explorer step: no landmark, one untraversed exit → go north.
        let events = task.tick(sim.ctx(&db));
        assert!(sent(&events).contains(&"north"), "explores north: {events:?}");
        // Land in 23283; the executor records learned[23282][north]=23283.
        sim.current = 23283;
        sim.compass_dirs = strs(&["south"]);
        sim.now += 100;
        let _ = task.tick(sim.ctx(&db));
        // Back at 23282, north is now known — but with no landmark it still
        // finds another route; the key check is we didn't crash and stayed in
        // the explorer.
        assert!(matches!(task.step, Step::Confluence { .. }));
    }

    #[test]
    fn standing_on_tranquility_warps_out_and_repaths() {
        let db = confluence_db();
        let mut task = TravelTask::start(&db, 1, 1005, 0).unwrap();
        let mut sim = Sim::new(1);
        task.tick(sim.ctx(&db)); // enter
        sim.current = 23282;
        sim.compass_dirs = strs(&["north"]);
        sim.loot_nouns = strs(&[super::super::confluence::TRANQUILITY_LOOT]);
        // The tranquility point is here → go tranquility.
        let events = task.tick(sim.ctx(&db));
        assert!(
            sent(&events).contains(&super::super::confluence::TRANQUILITY_GO),
            "warps out via the portal: {events:?}"
        );
        // The portal dumps us outside the Plane at the destination (1005);
        // the top-of-tick arrival check ends the trip.
        sim.current = 1005;
        sim.loot_nouns.clear();
        sim.now += 100;
        let events = task.tick(sim.ctx(&db));
        assert!(
            matches!(events.last(), Some(TravelEvent::Arrived { destination: 1005, .. })),
            "warping to the destination completes the trip: {events:?}"
        );
    }

    #[test]
    fn leaving_the_plane_short_of_destination_repaths() {
        // Same as above but the portal drops us at a non-destination zone-exit
        // room (2005), from which we must re-path to the real destination.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Edge]"],
                 "wayto": {"23282": "go rift"}, "timeto": {"23282": 0.2}, "paths": ""},
                {"id": 23282, "uid": [9023282], "location": "P", "title": ["[Plane]"],
                 "wayto": {"1005": "confluence"}, "timeto": {"1005": 0.2}, "paths": ""},
                {"id": 2005, "uid": [9002005], "location": "T", "title": ["[Drop]"],
                 "wayto": {"1005": "north"}, "timeto": {"1005": 0.2}, "paths": ""},
                {"id": 1005, "uid": [9001005], "location": "T", "title": ["[Out]"],
                 "wayto": {"2005": "south"}, "timeto": {"2005": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 1005, 0).unwrap();
        let mut sim = Sim::new(1);
        task.tick(sim.ctx(&db)); // enter
        sim.current = 23282;
        sim.compass_dirs = strs(&["north"]);
        sim.loot_nouns = strs(&[super::super::confluence::TRANQUILITY_LOOT]);
        task.tick(sim.ctx(&db)); // go tranquility
        // Dropped at 2005 (not the destination) → explorer relinquishes + repaths.
        sim.current = 2005;
        sim.loot_nouns.clear();
        sim.now += 100;
        let events = task.tick(sim.ctx(&db));
        assert!(
            task.confluence.is_none(),
            "explorer state dropped on leaving the Plane"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                TravelEvent::Status(s) if s.contains("left the Plane")
            )),
            "re-paths from the drop room: {events:?}"
        );
    }

    #[test]
    fn a_shifted_maze_wipes_and_relearns() {
        let db = confluence_db();
        let mut task = TravelTask::start(&db, 1, 1005, 0).unwrap();
        let mut sim = Sim::new(1);
        task.tick(sim.ctx(&db)); // enter
        sim.current = 23282;
        sim.compass_dirs = strs(&["north", "east"]);
        let _ = task.tick(sim.ctx(&db)); // records 23282's exits, sends a move (pending)
        // The move lands in a neighbor (23283) — the executor learns the edge
        // and clears the pending move.
        sim.current = 23283;
        sim.compass_dirs = strs(&["south"]);
        sim.now += 100;
        let _ = task.tick(sim.ctx(&db)); // records 23283, sends a move
        // Walk back to 23282, but its exits have SHIFTED since we recorded them.
        sim.current = 23282;
        sim.compass_dirs = strs(&["north", "west"]);
        sim.now += 100;
        let events = task.tick(sim.ctx(&db));
        assert!(
            events.iter().any(|e| matches!(
                e,
                TravelEvent::Status(s) if s.contains("shifted")
            )),
            "detects the shift and relearns: {events:?}"
        );
        assert!(matches!(task.step, Step::Confluence { .. }));
    }
}
