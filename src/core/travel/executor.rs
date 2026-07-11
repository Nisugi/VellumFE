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
    /// A move was sent; waiting to arrive in `expected`.
    AwaitArrival {
        expected: u32,
        /// Room the move was sent from — still being here just means the
        /// move hasn't landed; anywhere else is off-route.
        from: u32,
        sent_ms: u64,
    },
}

/// How long a move may take before it counts as failed. Generous: RT from
/// the move itself plus lag both land inside this window.
const STEP_TIMEOUT_MS: u64 = 8_000;
/// `stand` gets a shorter window (go2 uses dothistimeout 2s).
const STAND_TIMEOUT_MS: u64 = 2_500;
const MAX_STAND_ATTEMPTS: u32 = 5;
/// Same-edge failures before the edge is disabled for the session.
const MAX_EDGE_RETRIES: u32 = 2;
/// Re-path budget — a trip that restarts this often is going nowhere.
const MAX_RESTARTS: u32 = 10;

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
        let path = pathing::path_to(db, from, destination).ok_or_else(|| {
            format!("no route from room {from} to {destination} (see .room for how this room resolved)")
        })?;
        Ok(TravelTask {
            destination,
            path,
            idx: 0,
            step: Step::Prepare,
            banned: HashSet::new(),
            edge_retries: 0,
            restarts: 0,
            started_ms: now_ms,
            muckle_announced: false,
        })
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
            events.push(TravelEvent::Failed("you're dead — travel aborted".into()));
            return events;
        }
        let Some(current) = ctx.current_room else {
            // Unresolved (unmapped room / db still loading): hold.
            return events;
        };
        if current == self.destination {
            events.push(TravelEvent::Arrived {
                destination: self.destination,
                seconds: (ctx.now_ms.saturating_sub(self.started_ms)) as f64 / 1000.0,
            });
            return events;
        }

        match self.step.clone() {
            Step::Prepare => self.tick_prepare(current, ctx, &mut events),
            Step::AwaitStand { sent_ms, attempts } => {
                if ctx.standing {
                    self.step = Step::Prepare;
                    self.tick_prepare(current, ctx, &mut events);
                } else if ctx.now_ms.saturating_sub(sent_ms) > STAND_TIMEOUT_MS {
                    if attempts >= MAX_STAND_ATTEMPTS {
                        events.push(TravelEvent::Failed(
                            "can't stand up — travel aborted".into(),
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
                        "off the planned route (room {current}) — re-pathing"
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
            } => {
                if current == expected {
                    // Arrived on schedule; next step (or the destination
                    // check next tick).
                    self.arrive();
                    return events;
                }
                if current != from {
                    // Somewhere unexpected but mapped (fled, teleported,
                    // moved by hand mid-trip): re-path from here.
                    events.push(TravelEvent::Status(format!(
                        "off the planned route (room {current}) — re-pathing"
                    )));
                    self.repath(ctx.db, current, &mut events);
                    return events;
                }
                if ctx.now_ms.saturating_sub(sent_ms) > STEP_TIMEOUT_MS {
                    if self.edge_retries >= MAX_EDGE_RETRIES {
                        // go2: "changing Room[..].timeto[..] to nil" + restart.
                        events.push(TravelEvent::Status(format!(
                            "move {from} → {expected} keeps failing — disabling that edge for this session and re-pathing"
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
        self.step = Step::Prepare;
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
        loop {
            let Some(action) = actions.get(pc).cloned() else {
                // Script done: the room change is now the edge's job.
                self.step = Step::AwaitArrival {
                    expected,
                    from,
                    sent_ms: ctx.now_ms,
                };
                return;
            };
            match action {
                WalkAction::Noop => pc += 1,
                WalkAction::Move(cmd) | WalkAction::Put(cmd) => {
                    events.push(TravelEvent::Send(cmd));
                    pc += 1;
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

    fn tick_prepare(&mut self, current: u32, ctx: TravelContext, events: &mut Vec<TravelEvent>) {
        if ctx.muckled {
            if !self.muckle_announced {
                events.push(TravelEvent::Status(
                    "stunned/webbed — waiting until you can move".into(),
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
        if crate::core::mapdb::is_proc_command(&command) {
            // Scripted edge: run its transpiled actions. The pathfinder only
            // admits transpilable procs, so a miss here means the graph and
            // transpiler disagree — treat it like a broken edge.
            match crate::core::pathing::transpile::transpile(&command) {
                Some(actions) => {
                    self.tick_script(actions, 0, None, next, current, ctx, events);
                }
                None => {
                    events.push(TravelEvent::Status(format!(
                        "edge {current} → {next} uses an unsupported script — disabling it and re-pathing"
                    )));
                    self.banned.insert((current, next));
                    self.repath(ctx.db, current, events);
                }
            }
            return;
        }
        events.push(TravelEvent::Send(command));
        self.step = Step::AwaitArrival {
            expected: next,
            from: current,
            sent_ms: ctx.now_ms,
        };
    }

    fn repath(&mut self, db: &MapDb, current: u32, events: &mut Vec<TravelEvent>) {
        self.restarts += 1;
        if self.restarts > MAX_RESTARTS {
            events.push(TravelEvent::Failed(
                "too many restarts — travel aborted".into(),
            ));
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
                    "no remaining route from room {current} to {} — travel aborted",
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
            .any(|e| matches!(e, TravelEvent::Arrived { .. } | TravelEvent::Failed(_)))
    }
}

/// go2 skips standing for swim/pedal movement commands.
fn command_is_swim_or_pedal(command: &str) -> bool {
    let lower = command.to_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|word| word == "swim" || word == "pedal")
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
}
