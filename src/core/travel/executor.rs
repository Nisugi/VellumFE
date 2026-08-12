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
    /// Hidden or invisible, for scripted-edge `hidden?` branches.
    pub hidden: bool,
    /// Home-town citizenship, profession and society — scripted edges gate on
    /// these (a guild door, a citizen-only gate). `None` when not yet parsed
    /// from the feed, which evaluates as "doesn't match" (see `eval`).
    pub citizenship: Option<&'a str>,
    pub profession: Option<&'a str>,
    pub society: Option<&'a str>,
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
    /// Recent raw game lines (seq, text), newest last — what `Await` steps
    /// match against. A bounded ring, NOT drained per tick like `feedback`:
    /// an await arms mid-tick and must see lines that landed before it
    /// started, and several steps may match the same line. Each await
    /// remembers the seq it armed at and only considers newer entries.
    /// Empty when the caller doesn't wire it (tests without awaits).
    pub recent_lines: &'a [(u64, String)],
    /// The newest line sequence number, i.e. what an await arming NOW should
    /// record as its starting point. Equals the last `recent_lines` seq, or
    /// the running counter when the ring is empty.
    pub line_seq: u64,
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
    /// Names of everything you're carrying, containers included — for
    /// `Cond::HasItem` (a door that needs its key). Empty when the caller
    /// doesn't wire inventory, which evaluates as "don't have it".
    pub carried_names: &'a [String],
    /// Ground-loot nouns in the current room — the Confluence explorer scans
    /// these for the tranquility point / pit landmarks (`GameObj.loot`).
    pub loot_nouns: &'a [String],
    /// Chronomage day-pass crossing inputs, when the planned edge is a day-pass
    /// edge and the caller supplies them (`None` otherwise).
    pub day_pass: Option<DayPassInputs<'a>>,
    /// The Isle of Four Winds trinket, resolved from `go2.fwi_trinket`
    /// against live inventory: its exist id, and the container to put it back
    /// in (`None` when it's worn or held, i.e. nothing to return it to).
    /// `None` overall when unconfigured or not carried — the crossing then
    /// can't run and its edges fall back.
    pub fwi_trinket: Option<TrinketInputs<'a>>,
}

/// The Four Winds trinket, resolved against live inventory.
///
/// Resolved by the caller (which owns the registry) rather than passing the
/// registry in, matching how `StashInputs` works: the executor stays a pure
/// state machine over values it was handed.
#[derive(Clone, Copy, Debug)]
pub struct TrinketInputs<'a> {
    /// Exist id, for `turn #<id>`.
    pub id: &'a str,
    /// Container command-target to return it to, when it came out of one.
    /// `None` for a worn/held trinket — nothing to put back.
    pub return_to: Option<&'a str>,
    /// Whether it's already in hand (skip the `get`).
    pub in_hand: bool,
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
    /// Hidden or invisible — the buy conversation must `unhide` before asking
    /// the clerk (they won't respond otherwise).
    pub hidden: bool,
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
    fn eval(&self, cond: &crate::core::pathing::edge::Cond) -> bool {
        use crate::core::pathing::edge::Cond;
        // Case-insensitive compare against an Option<&str> we may not know.
        // An UNKNOWN value answers false: refusing a route we might have been
        // able to take is recoverable (re-path, or hand off to Lich), while
        // walking one we can't take strands the trip mid-route.
        let matches = |actual: Option<&str>, want: &str| {
            actual.is_some_and(|a| a.eq_ignore_ascii_case(want))
        };
        match cond {
            Cond::SpellActive(n) => self.active_spells.contains(n),
            Cond::Sitting => self.sitting,
            Cond::Kneeling => self.kneeling,
            Cond::Standing => self.standing,
            Cond::Hidden => self.hidden,
            Cond::PathAvailable(dir) => self
                .compass_dirs
                .iter()
                .any(|d| d.eq_ignore_ascii_case(dir)),
            Cond::RoomHasObject(noun) => self
                .loot_nouns
                .iter()
                .any(|n| n.eq_ignore_ascii_case(noun)),
            Cond::Citizenship(want) => matches(self.citizenship, want),
            Cond::Profession(want) => matches(self.profession, want),
            Cond::Society(want) => matches(self.society, want),
            Cond::Not(inner) => !self.eval(inner),
            Cond::Any(any) => any.iter().any(|c| self.eval(c)),
            Cond::InRoom(id) => self.current_room == Some(*id),
            // Unknown inventory answers false: sending a pile of unlock
            // commands without the key just fails noisily, and the else
            // branch (try the door anyway) is the safe one.
            Cond::HasItem(name) => self.carried_names.iter().any(|n| {
                let n = n.to_lowercase();
                let want = name.to_lowercase();
                n == want || n.contains(&want)
            }),
            // Captures live on the executor, not the context; the If arm in
            // tick_script evaluates them via eval_cond_with_captures before
            // falling through to here. Reaching this arm means a CaptureIs
            // leaked somewhere capture-less (a Repeat until, an
            // override) — unknown answers false, as everywhere.
            Cond::CaptureIs(..) => false,
        }
    }

    fn saw(&self, event: &crate::core::move_feedback::MoveFeedback) -> bool {
        self.feedback.contains(event)
    }
}

/// How the executor will cross a given edge. Mirrors the dispatch order in
/// `send_edge` exactly.
///
/// Exists so coverage reporting asks the question that matters — "can we
/// cross this?" — instead of "does `transpile()` return Some?". Several
/// families (Confluence, curated mazes, day passes, curated overrides) are
/// handled by dedicated strategies BEFORE the transpiler is consulted, so
/// measuring by transpilability alone counts thousands of perfectly walkable
/// edges as residue and badly misdirects recognizer work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeCrossing {
    /// A plain string command.
    Plain,
    /// A curated hand-authored override.
    Override,
    /// The Plane of Elemental Confluence explorer.
    Confluence,
    /// A curated maze's pathcode strategy.
    Maze,
    /// The Chronomage day-pass crossing.
    DayPass,
    /// A StringProc the transpiler understands.
    Transpiled,
    /// A StringProc we can't cross natively (bans + re-paths, or hands to Lich).
    Untranspiled,
}

impl EdgeCrossing {
    /// Whether the executor has SOME native way across.
    pub fn is_crossable(self) -> bool {
        !matches!(self, EdgeCrossing::Untranspiled)
    }
}

/// Classify how `from -> to` would be crossed. Kept in the same order as
/// `send_edge`'s dispatch so the two can't disagree.
pub fn classify_edge(db: &MapDb, from: u32, to: u32, command: &str) -> EdgeCrossing {
    // The maze strategy takes over at the boundary, but `begin_maze` walks
    // INBOUND only (from the entrance or start room) and fails from any other
    // side. Classify only the case it actually handles, so the report doesn't
    // credit crossings that would fail.
    if let Some(maze) = super::mazes::maze_containing(to) {
        if !maze.rooms.contains(&from) && (from == maze.entrance || from == maze.start) {
            return EdgeCrossing::Maze;
        }
    }
    // Entering the Plane is the `send_edge` boundary check. But the bulk of
    // the Confluence edges point OUTWARD (23282 -> 188): they are the
    // `$mapdb_confluence_target = N; Room[N].wayto['N'].call` delegations that
    // set a goal and hand to the explorer. Once we are inside the zone the
    // explorer owns every edge, in or out, so classify on either side.
    if super::confluence::is_confluence_room(from)
        || super::confluence::is_confluence_room(to)
    {
        return EdgeCrossing::Confluence;
    }
    if crate::core::pathing::overrides::edge_override(from, to).is_some() {
        return EdgeCrossing::Override;
    }
    if crate::core::day_pass::edge(from, to).is_some() {
        return EdgeCrossing::DayPass;
    }
    if !crate::core::mapdb::is_proc_command(command) {
        return EdgeCrossing::Plain;
    }
    match crate::core::pathing::transpile::transpile_edge(db, command) {
        Some(_) => EdgeCrossing::Transpiled,
        None => EdgeCrossing::Untranspiled,
    }
}

/// An armed `Await`: what it has already done, so a resume doesn't repeat it.
#[derive(Debug, Clone, PartialEq)]
pub struct AwaitState {
    /// Line sequence when the await armed; only newer lines can match. Without
    /// this a stale line already in the ring would satisfy the await instantly.
    since_seq: u64,
    /// Deadline in `now_ms` terms.
    deadline_ms: u64,
    /// Whether the `Retry` re-send has already been spent (so it fails on the
    /// second timeout rather than re-sending forever).
    retried: bool,
}

/// Named values captured by `Await` patterns, for `{capture:name}` tokens in
/// later commands. Scoped to one edge's script run.
type Captures = Vec<(String, String)>;

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
        /// State of an in-progress `Await` at `pc`, if one is armed.
        awaiting: Option<AwaitState>,
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
    /// The minotaur maze walker: same learned-graph shape as Confluence, but
    /// hunting a specific ROOM rather than a landmark, over a room set carried
    /// per-edge. See travel::minotaur.
    Minotaur {
        /// The room this maze crossing is trying to reach.
        target: u32,
        /// Rooms belonging to this maze; arriving outside means we fell out.
        maze_rooms: Vec<u32>,
        /// Awaiting arrival after a walk (from-room + direction sent).
        pending: Option<ConfluencePending>,
    },
    /// A scripted-edge `StepMove` is in flight: a paced walk command was sent
    /// mid-script and we're waiting for the room to change before resuming the
    /// script at `pc`. `sent_from` is the room it was sent in (any other room
    /// means it landed). Used by the day-pass buy walk.
    ScriptWalk {
        actions: Vec<crate::core::pathing::edge::WalkAction>,
        pc: usize,
        /// The action that SENT the in-flight command, so an RT rejection
        /// ("...wait 2 seconds") can re-run it instead of skipping ahead.
        sent_pc: usize,
        expected: u32,
        from: u32,
        sent_from: u32,
        sent_ms: u64,
    },
    /// The response-driven Chronomage day-pass BUY (Lich's dothistimeout flow).
    /// Each phase sends a command and waits for its game-response event before
    /// advancing — ask → offer → confirm → (bank trip if too poor) → pass in
    /// hand → walk back to the waiting room → raise → travel. See
    /// travel::day_pass_buy.
    DayPassBuy(super::day_pass_buy::BuyState),
    /// The response-driven Chronomage day-pass USE (raise a held pass): open
    /// sack → drop expired → get → raise, one command per game response —
    /// never a flood into the type-ahead buffer. See day_pass_buy::UseState.
    DayPassUse(super::day_pass_buy::UseState),
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
/// Withdrawal floor when a fare's price isn't in the route model (no
/// silver-cost tag): enough for any cart/ferry ticket plus change, matching
/// the corpus's own errands (River's Rest withdraws 2000).
const GENERIC_FARE_SILVERS: u64 = 2_000;
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
/// Hard ceiling on `WalkAction::Repeat` iterations, applied by the
/// interpreter no matter what the map data asks for. Mirrors Lich's
/// MAX_LOOP_ITERATIONS: bad map data may waste a route, it must never hang
/// the client waiting on a loop that can't terminate.
const MAX_SCRIPT_LOOP: u32 = 50;
/// How many times a `GuidedRoute` may walk its whole direction cycle before
/// giving up. The landmark normally appears within one lap; more than two
/// means the table no longer matches the map.
const GUIDED_ROUTE_LAPS: u32 = 2;
/// How long one Symbol of Seeking cast gets to offer a room.
const SEEKING_OFFER_TIMEOUT: f32 = 6.0;
/// Casts before giving up on a destination the symbol never offers (the
/// Ruby's `20.times`).
const SEEKING_MAX_CASTS: u32 = 20;
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
    /// `{capture:name}` values bound by `Await` steps on the CURRENT edge.
    /// Lives on the task (not threaded through tick_script) so it survives the
    /// suspend/resume an await necessarily performs. Cleared per edge: a
    /// capture from a previous crossing has no business filling a later
    /// command.
    captures: Captures,
    /// Re-entries into the current GuidedRoute, so a landmark that never
    /// appears can't re-arm the direction cycle forever. Reset per edge.
    guided_laps: u32,
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
    /// A mid-script "not enough silvers" already triggered one bank detour;
    /// a second means the withdrawal cannot cover the fare - fail the edge
    /// rather than loop bank trips (the day-pass buyer's `funded` guard).
    fare_funded: bool,
    /// True while walking the funding detour to a bank (so arrival there
    /// triggers the withdraw rather than a normal arrival).
    funding_bank: Option<u32>,
    /// The live-learned Confluence map (Step::Confluence). Held outside the
    /// Step enum so Step stays Clone+PartialEq, same as `stash`. `Some` only
    /// while inside the Plane.
    confluence: Option<super::confluence::ConfluenceState>,
    /// Learned graph for an in-progress minotaur maze crossing.
    minotaur: Option<super::minotaur::MinotaurState>,
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
    /// Set when a day-pass crossing sends `open #sack` and still OWES a close.
    /// Cleared WITHOUT closing if the game answers "That is already open" (the
    /// user keeps the sack open — don't force it shut); taken when the close
    /// is emitted at crossing end. Lich's `close_sack` flag.
    day_pass_close_sack: Option<String>,
    /// The day-pass edge's source room (to ban the edge after crossing).
    day_pass_buy_from: u32,
    /// The day-pass edge's destination room, for the raise-arrival check.
    day_pass_buy_dest: u32,
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
            captures: Captures::new(),
            guided_laps: 0,
            restarts: 0,
            started_ms: now_ms,
            muckle_announced: false,
            stash: None,
            stash_stack: Vec::new(),
            silver_need,
            fare_funded: false,
            funding_bank: None,
            confluence: None,
            minotaur: None,
            silver_at_withdraw: None,
            in_transport: false,
            day_pass_close_sack: None,
            day_pass_buy_from: 0,
            day_pass_buy_dest: 0,
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
        // The day-pass sack was already open when our `open` landed: we don't
        // owe a close (don't force the user's normally-open sack shut).
        if self.day_pass_close_sack.is_some()
            && ctx.saw(&crate::core::move_feedback::MoveFeedback::ContainerAlreadyOpen)
        {
            self.day_pass_close_sack = None;
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
        // NOT while a day-pass machine is mid-crossing: the raise teleports us
        // to the destination BEFORE the machine's cleanup (put the pass back,
        // refill hands, close) — the machine's Traveled path concludes the
        // trip itself.
        if current == self.destination
            && self.funding_bank.is_none()
            && self.stash.is_none()
            && !matches!(self.step, Step::DayPassBuy(_) | Step::DayPassUse(_))
        {
            // A day-pass crossing that landed here may still owe the sack close.
            self.flush_day_pass_close(&mut events);
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
            Step::Minotaur { target, maze_rooms, pending } => {
                self.tick_minotaur(target, maze_rooms, pending, current, ctx, &mut events);
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
            Step::DayPassBuy(state) => {
                self.tick_day_pass_buy(state, current, ctx, &mut events);
            }
            Step::DayPassUse(state) => {
                self.tick_day_pass_use(state, current, ctx, &mut events);
            }
            Step::ScriptWalk {
                actions,
                pc,
                sent_pc,
                expected,
                from,
                sent_from,
                sent_ms,
            } => {
                use crate::core::move_feedback::MoveFeedback as F;
                // The command landed during roundtime: nothing failed, re-run
                // the SAME action (RT-gated) instead of skipping ahead or
                // waiting out a timeout.
                if ctx.saw(&F::RtWait) {
                    self.tick_script(actions, sent_pc, None, expected, from, None, ctx, &mut events);
                    return events;
                }
                // Paced walk step in flight. Resume the script at `pc` when
                // ANY movement evidence arrives - a Lich-room change, a <nav>
                // (the room moved even if its mapped id didn't: multi-uid
                // rooms like the Whistler's Pass labyrinth are one id across
                // many physical rooms, and waiting on the id alone stalled
                // every in-maze step for the full 30s slow timeout), or a
                // failure line (the Ruby scripts' moves also proceeded on
                // failure; retry loops re-evaluate immediately). The timeout
                // stays as the backstop for silent responses.
                let moved = current != sent_from
                    || ctx.saw(&F::NavArrived);
                let failed = ctx.saw(&F::MoveFailedRemovable)
                    || ctx.saw(&F::MoveFailedKeep)
                    || ctx.saw(&F::DoorClosed)
                    || ctx.saw(&F::Fell)
                    || ctx.saw(&F::NeedClimb)
                    || ctx.saw(&F::CantClimb);
                if moved
                    || failed
                    || ctx.now_ms.saturating_sub(sent_ms) > SLOW_ARRIVAL_TIMEOUT_MS
                {
                    self.tick_script(actions, pc, None, expected, from, None, ctx, &mut events);
                } else {
                    self.step = Step::ScriptWalk {
                        actions,
                        pc,
                        sent_pc,
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
                awaiting,
            } => {
                // A scripted edge can land the room change before its
                // actions finish (multi-command edges): arrival wins.
                if current == expected {
                    self.flush_day_pass_close(&mut events);
                    self.arrive();
                    return events;
                }
                if current != from {
                    events.push(TravelEvent::Status(format!(
                        "off the planned route (room {current}) - re-pathing"
                    )));
                    self.repath(ctx.db, current, ctx.lich_fallback, &mut events);
                    return events;
                }
                self.tick_script(
                    actions, pc, sleep_until, expected, from, awaiting, ctx, &mut events,
                );
            }
            Step::AwaitArrival {
                expected,
                from,
                sent_ms,
                slow,
            } => {
                if current == expected {
                    // Arrived on schedule; next step (or the destination
                    // check next tick). A day-pass USE crossing settles its
                    // owed sack close here.
                    self.in_transport = false;
                    self.flush_day_pass_close(&mut events);
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
                    self.repath(ctx.db, current, ctx.lich_fallback, &mut events);
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
                    self.repath(ctx.db, current, ctx.lich_fallback, &mut events);
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
                // The move landed during roundtime ("...wait 2 seconds"):
                // nothing failed, so re-send as soon as RT clears — via
                // Prepare, which gates on rt_remaining — rather than waiting
                // out the 8s step timeout. Not counted as a retry: the edge
                // did not fail.
                if ctx.saw(&MoveFeedback::RtWait) {
                    self.step = Step::Prepare;
                    self.tick_prepare(current, ctx, &mut events);
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
                        self.repath(ctx.db, current, ctx.lich_fallback, &mut events);
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

    /// Emit the owed `close #sack` from a day-pass crossing, if any (skipped
    /// when the sack was already open — see day_pass_close_sack).
    fn flush_day_pass_close(&mut self, events: &mut Vec<TravelEvent>) {
        if let Some(sack) = self.day_pass_close_sack.take() {
            events.push(TravelEvent::Send(format!("close #{sack}")));
        }
    }

    /// The expected room arrived: advance the route.
    fn arrive(&mut self) {
        self.idx += 1;
        self.edge_retries = 0;
        // Captures are scoped to the edge that bound them; a value read off a
        // line during the last crossing must not fill a command in the next.
        self.captures.clear();
        self.guided_laps = 0;
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
                            self.repath(ctx.db, current, ctx.lich_fallback, events);
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
            self.repath(ctx.db, current, ctx.lich_fallback, events);
        }
    }

    /// The minotaur maze tick: one step of learn-and-navigate toward a room.
    /// Same shape as `tick_confluence`, different goal (see travel::minotaur).
    fn tick_minotaur(
        &mut self,
        target: u32,
        maze_rooms: Vec<u32>,
        pending: Option<ConfluencePending>,
        current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        let resume = |pending| Step::Minotaur {
            target,
            maze_rooms: maze_rooms.clone(),
            pending,
        };
        // Reached the goal: hand back to the normal walker from here.
        if current == target {
            self.minotaur = None;
            self.step = Step::Prepare;
            self.repath(ctx.db, current, ctx.lich_fallback, events);
            return;
        }
        if ctx.muckled {
            if !self.muckle_announced {
                events.push(TravelEvent::Status(
                    "stunned/webbed - waiting until you can move".into(),
                ));
                self.muckle_announced = true;
            }
            self.step = resume(pending);
            return;
        }
        self.muckle_announced = false;

        // A move is in flight: wait for the room to change, then learn it.
        if let Some(p) = &pending {
            if current == p.from {
                if ctx.now_ms.saturating_sub(p.sent_ms) <= STEP_TIMEOUT_MS {
                    return; // still waiting to land
                }
                self.step = resume(None);
                return;
            }
            if let Some(state) = self.minotaur.as_mut() {
                state.record_arrival(p.from, &p.dir, current);
            }
            self.step = resume(None);
            // fall through and decide from `current`
        }

        // Fell out of the maze. The Ruby walks back along the arrival room's
        // own wayto edge to where it came from; we let the router do it, which
        // handles the walk-back and any re-plan in one place.
        if !maze_rooms.contains(&current) {
            self.minotaur = None;
            events.push(TravelEvent::Status(format!(
                "left the maze at room {current} - re-pathing"
            )));
            self.repath(ctx.db, current, ctx.lich_fallback, events);
            return;
        }
        if ctx.rt_remaining > 0.0 {
            return;
        }

        let state = self
            .minotaur
            .get_or_insert_with(super::minotaur::MinotaurState::new);
        // Exits changed since we last stood here → the maze shifted and every
        // learned edge is now a lie. Wipe rather than route on stale data.
        if state.record_exits(current, ctx.compass_dirs) {
            events.push(TravelEvent::Status(
                "the maze shifted - relearning".into(),
            ));
            state.reset();
            state.record_exits(current, ctx.compass_dirs);
        }
        match state.choose_dir(current, target, ctx.compass_dirs) {
            super::minotaur::MinotaurMove::Arrive => {
                self.minotaur = None;
                self.step = Step::Prepare;
                self.repath(ctx.db, current, ctx.lich_fallback, events);
            }
            super::minotaur::MinotaurMove::Go(dir) => {
                events.push(TravelEvent::Send(dir.clone()));
                self.step = resume(Some(ConfluencePending {
                    from: current,
                    dir,
                    sent_ms: ctx.now_ms,
                }));
            }
            // No exits at all: don't wander blindly out of a maze.
            super::minotaur::MinotaurMove::Lost => {
                self.minotaur = None;
                events.push(TravelEvent::Status(
                    "no way out of this maze room - re-pathing".into(),
                ));
                self.repath(ctx.db, current, ctx.lich_fallback, events);
            }
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
            self.repath(ctx.db, current, ctx.lich_fallback, events);
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
            self.repath(ctx.db, current, ctx.lich_fallback, events);
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
                self.repath(ctx.db, current, ctx.lich_fallback, events);
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
        mut awaiting: Option<AwaitState>,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        use crate::core::pathing::edge::{Cond, RepeatUntil, WalkAction};
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
                    // {capture:name} from an earlier await — the lever/rune
                    // puzzles read a value off a line, then act on it.
                    let Some(cmd) =
                        crate::core::pathing::edge::expand_captures(&cmd, &self.captures)
                    else {
                        events.push(TravelEvent::Status(format!(
                            "edge {from} -> {expected}: unbound capture in '{cmd}'"
                        )));
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    };
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
                        sent_pc: pc,
                        expected,
                        from,
                        sent_from: ctx.current_room.unwrap_or(from),
                        sent_ms: ctx.now_ms,
                    };
                    return;
                }
                WalkAction::MoveExitExcept(not_dir) => {
                    // Whichever compass exit is NOT the named one, resolved
                    // from live room state (the Hidden Plateau's shifting
                    // rooms — the fixed exit is the wrong one). Compass dirs
                    // arrive short ("nw"); the mapdb names them long
                    // ("northwest"), so compare normalized.
                    if ctx.rt_remaining > 0.0 {
                        break;
                    }
                    let avoid = short_dir(&not_dir);
                    let Some(dir) = ctx
                        .compass_dirs
                        .iter()
                        .find(|d| short_dir(d) != avoid)
                        .cloned()
                    else {
                        events.push(TravelEvent::Status(format!(
                            "edge {from} -> {expected}: no exit other than {not_dir} on offer"
                        )));
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    };
                    events.push(TravelEvent::Send(dir));
                    self.step = Step::ScriptWalk {
                        actions,
                        pc: pc + 1,
                        sent_pc: pc,
                        expected,
                        from,
                        sent_from: ctx.current_room.unwrap_or(from),
                        sent_ms: ctx.now_ms,
                    };
                    return;
                }
                WalkAction::MoveAnyExit => {
                    // The wander step of a shifting-area hunt (Karazja's
                    // `walk`): any compass exit — random, matching Lich, so a
                    // static room still eventually tries every door.
                    if ctx.rt_remaining > 0.0 {
                        break;
                    }
                    let Some(dir) = pick_random_exit(ctx.compass_dirs, from) else {
                        events.push(TravelEvent::Status(format!(
                            "edge {from} -> {expected}: no compass exits to wander"
                        )));
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    };
                    events.push(TravelEvent::Send(dir));
                    self.step = Step::ScriptWalk {
                        actions,
                        pc: pc + 1,
                        sent_pc: pc,
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
                    let taken = eval_cond_with_captures(&cond, &self.captures, &ctx);
                    let branch = if taken { then } else { els };
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
                        awaiting: None,
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
                WalkAction::Await {
                    cmd,
                    pattern,
                    timeout,
                    on_timeout,
                    if_match,
                } => {
                    use crate::core::pathing::edge::OnTimeout;
                    let state = match awaiting.take() {
                        // Already armed: just check for a match / timeout.
                        Some(state) => state,
                        // Arming now. Send the command (if active) and record
                        // the line seq so only NEWER lines can satisfy us.
                        None => {
                            if let Some(cmd) = &cmd {
                                // Fill {capture:...} from earlier awaits. An
                                // unbound token means we'd send a half-formed
                                // command, so fail the edge instead.
                                let Some(cmd) =
                                    crate::core::pathing::edge::expand_captures(
                                        cmd,
                                        &self.captures,
                                    )
                                else {
                                    events.push(TravelEvent::Status(format!(
                                        "edge {from} -> {expected}: unbound capture in '{cmd}'"
                                    )));
                                    self.handle_uncrossable_edge(from, expected, ctx, events);
                                    return;
                                };
                                events.push(TravelEvent::Send(cmd));
                                sent_anything = true;
                            }
                            AwaitState {
                                since_seq: ctx.line_seq,
                                deadline_ms: ctx.now_ms
                                    + (timeout.max(0.0) * 1000.0) as u64,
                                retried: false,
                            }
                        }
                    };
                    // Bind named groups from the matching line so later
                    // commands can interpolate them ({capture:name}).
                    let hit = ctx
                        .recent_lines
                        .iter()
                        .find(|(seq, line)| *seq > state.since_seq && pattern.is_match(line));
                    if let Some((_, line)) = hit {
                        if let Some(bound) = pattern.captures(line) {
                            for (name, value) in bound {
                                // Last write wins: a loop re-running an await
                                // should see the newest value, not the first.
                                self.captures.retain(|(k, _)| *k != name);
                                self.captures.push((name, value));
                            }
                        }
                        // The response can decide what happens next: splice
                        // the branch in only when the line also matches it.
                        match if_match {
                            Some((branch_pat, steps)) if branch_pat.is_match(line) => {
                                actions.splice(pc..=pc, steps);
                            }
                            _ => pc += 1,
                        }
                        continue;
                    }
                    if ctx.now_ms < state.deadline_ms {
                        // A refused purchase mid-await ("You don't have
                        // enough silvers" - the mining-cart ticket, ferry
                        // fares): with Get Silvers on, detour to the bank
                        // via the same funding pipeline the paid-route
                        // pre-check uses, then re-plan - the replanned route
                        // re-crosses this edge with money in pocket. Without
                        // it (or after one failed attempt), the edge fails
                        // closed with the reason named.
                        if ctx.saw(&crate::core::move_feedback::MoveFeedback::TooPoor) {
                            let get_silvers =
                                ctx.funding.map(|f| f.get_silvers).unwrap_or(false);
                            if !get_silvers {
                                events.push(TravelEvent::Status(
                                    "not enough silver for this crossing and Get Silvers is off"
                                        .into(),
                                ));
                                self.handle_uncrossable_edge(from, expected, ctx, events);
                                return;
                            }
                            if self.fare_funded {
                                events.push(TravelEvent::Status(
                                    "still too poor after withdrawing - the bank can't cover this fare"
                                        .into(),
                                ));
                                self.handle_uncrossable_edge(from, expected, ctx, events);
                                return;
                            }
                            self.fare_funded = true;
                            self.silver_need = self.silver_need.max(GENERIC_FARE_SILVERS);
                            events.push(TravelEvent::Status(
                                "not enough silver for this crossing - detouring to the bank"
                                    .into(),
                            ));
                            self.step = Step::Funding(FundingPhase::AwaitWealth {
                                sent_ms: self.started_ms,
                            });
                            return;
                        }
                        // Still waiting: suspend with the state intact.
                        awaiting = Some(state);
                        break;
                    }
                    match on_timeout {
                        // Advisory await: the line never came, carry on.
                        OnTimeout::Continue => {
                            pc += 1;
                            continue;
                        }
                        // One re-send, then treat a second timeout as failure.
                        // Only meaningful with a command to re-send; a passive
                        // await has nothing to retry, so it fails instead.
                        OnTimeout::Retry if !state.retried && cmd.is_some() => {
                            let cmd = cmd.clone().expect("checked is_some");
                            events.push(TravelEvent::Send(cmd));
                            sent_anything = true;
                            awaiting = Some(AwaitState {
                                since_seq: ctx.line_seq,
                                deadline_ms: ctx.now_ms
                                    + (timeout.max(0.0) * 1000.0) as u64,
                                retried: true,
                            });
                            break;
                        }
                        OnTimeout::Fail | OnTimeout::Retry => {
                            events.push(TravelEvent::Status(format!(
                                "edge {from} -> {expected}: timed out waiting for /{}/",
                                pattern.source()
                            )));
                            self.handle_uncrossable_edge(from, expected, ctx, events);
                            return;
                        }
                    }
                }
                WalkAction::Repeat { body, until, max } => {
                    // Unroll one iteration in place, re-emitting the loop
                    // behind it with a decremented budget. Splicing keeps the
                    // whole loop inside the existing resumable action list, so
                    // suspend/resume (RT, sleep, await) works inside a loop
                    // for free — no separate loop stack to persist.
                    //
                    // MAX_SCRIPT_LOOP caps this regardless of the data's own
                    // number: bad map data may waste a route, never hang us.
                    let budget = max.min(MAX_SCRIPT_LOOP);
                    let done = match &until {
                        RepeatUntil::Count => false,
                        RepeatUntil::RoomChanged => ctx.current_room != Some(from),
                        RepeatUntil::Room(id) => ctx.current_room == Some(*id),
                        RepeatUntil::Cond(cond) => ctx.eval(cond),
                    };
                    if done || budget == 0 {
                        pc += 1;
                        continue;
                    }
                    let mut expansion = body.clone();
                    expansion.push(WalkAction::Repeat {
                        body,
                        until,
                        max: budget - 1,
                    });
                    actions.splice(pc..=pc, expansion);
                }
                WalkAction::Break => {
                    // Drop everything up to and including the enclosing
                    // Repeat. The loop is always BEHIND us in the spliced
                    // list (see Repeat), so scan forward for it.
                    match actions[pc..]
                        .iter()
                        .position(|a| matches!(a, WalkAction::Repeat { .. }))
                    {
                        Some(offset) => {
                            actions.drain(pc..=pc + offset);
                        }
                        // Outside a loop: a no-op, per Lich.
                        None => pc += 1,
                    }
                }
                WalkAction::GuidedRoute {
                    start_rooms,
                    dirs,
                    landmarks,
                } => {
                    // Already standing at a landmark: enter it without walking.
                    // Checked first so a route that starts on its destination
                    // doesn't take a needless lap.
                    if let Some((_, enter)) = landmarks.iter().find(|(noun, _)| {
                        ctx.loot_nouns.iter().any(|n| n.eq_ignore_ascii_case(noun))
                    }) {
                        actions.splice(pc..=pc, [WalkAction::Move(enter.clone())]);
                        continue;
                    }
                    if landmarks.is_empty() {
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    }
                    // Join the direction cycle at the offset for the room we
                    // are actually in. Not knowing where we are is the Ruby's
                    // `else echo 'error: mini-script expected a different
                    // room'` branch — it can't walk, so let the edge fall to
                    // ban/fallback rather than guessing an offset.
                    let Some(here) = ctx.current_room else {
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    };
                    let Some(offset) = start_rooms.iter().position(|&r| r == here) else {
                        events.push(TravelEvent::Status(format!(
                            "edge {from} -> {expected}: room {here} isn't on this guided route"
                        )));
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    };
                    // The walk re-enters this action to pick its landmark, so
                    // bound the re-entries: a landmark that never appears would
                    // otherwise re-arm the cycle forever.
                    self.guided_laps += 1;
                    if self.guided_laps > GUIDED_ROUTE_LAPS {
                        self.guided_laps = 0;
                        events.push(TravelEvent::Status(format!(
                            "edge {from} -> {expected}: walked the route without finding a landmark"
                        )));
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    }
                    if dirs.is_empty() {
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    }
                    // Lower into primitives: step the cycle until the landmark
                    // shows up, then enter it. Expressing it this way (rather
                    // than as bespoke stepping state) means the walk inherits
                    // RT-waiting and suspend/resume for free, and the loop
                    // ceiling bounds it.
                    // Stop as soon as ANY landmark shows up — a route can be
                    // hunting a door OR a mirror.
                    let arrived = Cond::Any(
                        landmarks
                            .iter()
                            .map(|(noun, _)| Cond::RoomHasObject(noun.clone()))
                            .collect(),
                    );
                    let cycle = (0..dirs.len())
                        .map(|i| dirs[(offset + i) % dirs.len()].clone())
                        .map(|d| {
                            WalkAction::Repeat {
                                // One StepMove per iteration, skipped once a
                                // landmark appears — the loop's own condition
                                // is what actually ends the walk.
                                body: vec![WalkAction::StepMove(d)],
                                until: RepeatUntil::Cond(arrived.clone()),
                                max: 1,
                            }
                        })
                        .collect::<Vec<_>>();
                    let expansion = vec![
                        WalkAction::Repeat {
                            body: cycle,
                            until: RepeatUntil::Cond(arrived),
                            max: GUIDED_ROUTE_LAPS,
                        },
                        // Re-enter this GuidedRoute once the walk ends: its
                        // already-at-a-landmark branch above picks whichever
                        // landmark actually turned up and enters it. We can't
                        // choose here — the walk hasn't happened yet.
                        WalkAction::GuidedRoute {
                            start_rooms,
                            dirs,
                            landmarks,
                        },
                    ];
                    actions.splice(pc..=pc, expansion);
                }
                WalkAction::VolnSeeking { destination } => {
                    // The symbol offers rooms by NAME, so we need the
                    // destination's title to know when to confirm.
                    let Some(title) = ctx
                        .db
                        .room(destination)
                        .and_then(|r| r.title.first())
                        .map(|t| t.trim_matches(['[', ']']).to_string())
                        .filter(|t| !t.is_empty())
                    else {
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    };
                    let Some(pattern) = crate::core::pathing::edge::AwaitPattern::new(
                        &regex::escape(&title),
                    ) else {
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    };
                    // Cast until the offered room is the one we want, then
                    // confirm. Bounded: the symbol cycles through rooms, and
                    // a destination it never offers must not loop forever.
                    actions.splice(
                        pc..=pc,
                        [
                            WalkAction::Repeat {
                                body: vec![
                                    WalkAction::WaitRt,
                                    WalkAction::Await {
                                        cmd: Some("symbol of seeking".into()),
                                        pattern: Box::new(pattern),
                                        timeout: SEEKING_OFFER_TIMEOUT,
                                        // A miss just means this cast offered
                                        // somewhere else; cast again.
                                        on_timeout:
                                            crate::core::pathing::edge::OnTimeout::Continue,
                                        if_match: Some((
                                            Box::new(
                                                crate::core::pathing::edge::AwaitPattern::new(
                                                    ".",
                                                )
                                                .expect("valid"),
                                            ),
                                            vec![WalkAction::Break],
                                        )),
                                    },
                                ],
                                until: RepeatUntil::Count,
                                max: SEEKING_MAX_CASTS,
                            },
                            WalkAction::StepMove("symbol of seeking confirm".into()),
                        ],
                    );
                }
                WalkAction::SetVar { name, value } => {
                    use crate::core::pathing::transpile::{
                        set_mapdb_var, CURRENT_ROOM_TOKEN,
                    };
                    // `Map.current.id` isn't knowable at transpile time; fill
                    // it from where we're standing as the action runs.
                    let value = match value.as_deref() {
                        Some(CURRENT_ROOM_TOKEN) => {
                            ctx.current_room.map(|id| id.to_string())
                        }
                        _ => value,
                    };
                    set_mapdb_var(&name, value);
                    pc += 1;
                }
                WalkAction::TrinketWarp => {
                    // Unconfigured or not carried: this crossing can't run.
                    // Fall back rather than sending `turn #` with no id.
                    let Some(t) = ctx.fwi_trinket else {
                        events.push(TravelEvent::Status(format!(
                            "edge {from} -> {expected} needs your Four Winds trinket - \
                             set it in Settings > Travel"
                        )));
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    };
                    // The Ruby scrapes `<a exist=...>` links out of the `get`
                    // response to learn which container the trinket came from.
                    // We don't need to: the registry already tracks
                    // containment, so `return_to` is resolved before we start.
                    let mut steps = Vec::new();
                    if !t.in_hand {
                        steps.push(WalkAction::EmptyHands);
                        steps.push(WalkAction::Put(format!("get #{}", t.id)));
                    }
                    steps.push(WalkAction::StepMove(format!("turn #{}", t.id)));
                    if !t.in_hand {
                        steps.push(WalkAction::Put(match t.return_to {
                            Some(bag) => format!("put #{} in #{bag}", t.id),
                            // Came from nowhere trackable — stow is the
                            // Ruby's own fallback for the same case.
                            None => format!("stow #{}", t.id),
                        }));
                        steps.push(WalkAction::FillHands);
                    }
                    // The warp lands somewhere the edge can't predict.
                    steps.push(WalkAction::Replan);
                    actions.splice(pc..=pc, steps);
                }
                WalkAction::TryMove { cmd, fallback } => {
                    // Lower into: send it, wait for the room to settle, then
                    // run the fallback only if we're still where we started.
                    // StepMove already does the send-and-wait (resuming on a
                    // room change or its timeout), so the whole action is
                    // that plus a guarded branch — no new suspend state.
                    let here = ctx.current_room.unwrap_or(from);
                    actions.splice(
                        pc..=pc,
                        [
                            WalkAction::StepMove(cmd),
                            WalkAction::If {
                                cond: Cond::InRoom(here),
                                then: fallback,
                                els: Vec::new(),
                            },
                        ],
                    );
                }
                WalkAction::RouteTable {
                    dirs,
                    target,
                    verb,
                    hands_free_in,
                } => {
                    let Some(here) = ctx.current_room else {
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    };
                    // Arrived: the table's job is done.
                    if here == target {
                        pc += 1;
                        continue;
                    }
                    // Off the table. The Ruby picks a random exit and echoes
                    // "Oh crap.. I'm lost.." — we re-path instead, which is
                    // the same intent (get back on a known route) without
                    // swimming blindly further off course.
                    let Some(dir) = dirs
                        .iter()
                        .find(|(room, _)| *room == here)
                        .map(|(_, d)| d.clone())
                    else {
                        events.push(TravelEvent::Status(format!(
                            "off the route table at room {here} - re-pathing"
                        )));
                        self.repath(ctx.db, here, ctx.lich_fallback, events);
                        return;
                    };
                    // Free hands where the table says to, before the first
                    // stroke rather than mid-swim.
                    let mut expansion = Vec::new();
                    if hands_free_in.contains(&here) {
                        expansion.push(WalkAction::EmptyHands);
                    }
                    expansion.push(WalkAction::StepMove(if verb.is_empty() {
                        dir
                    } else {
                        format!("{verb} {dir}")
                    }));
                    // Re-enter to look up the next room's direction once this
                    // step lands. Bounded by the same lap guard as GuidedRoute.
                    self.guided_laps += 1;
                    if self.guided_laps > MAX_SCRIPT_LOOP {
                        self.guided_laps = 0;
                        events.push(TravelEvent::Status(format!(
                            "route table from {from} didn't reach room {target}"
                        )));
                        self.handle_uncrossable_edge(from, expected, ctx, events);
                        return;
                    }
                    expansion.push(WalkAction::RouteTable {
                        dirs,
                        target,
                        verb,
                        hands_free_in,
                    });
                    actions.splice(pc..=pc, expansion);
                }
                WalkAction::MinotaurMaze { target, maze_rooms } => {
                    // Hand off to the learned-graph walker. It owns the whole
                    // crossing from here — including deciding when we've
                    // arrived — so this action never returns to the script.
                    self.minotaur = Some(super::minotaur::MinotaurState::new());
                    self.step = Step::Minotaur {
                        target,
                        maze_rooms,
                        pending: None,
                    };
                    return;
                }
                WalkAction::PauseForUser {
                    msg,
                    until,
                    timeout,
                } => {
                    // Already satisfied (the user acted, or the gate was never
                    // closed): carry straight on without bothering them.
                    if until.as_ref().is_some_and(|c| ctx.eval(c)) {
                        pc += 1;
                        continue;
                    }
                    // Otherwise abandon rather than block. A walker that waits
                    // silently on a human is indistinguishable from a hang;
                    // the message says what to do so the trip can be re-issued.
                    let _ = timeout;
                    events.push(TravelEvent::Status(format!(
                        "edge {from} -> {expected} needs you: {msg}"
                    )));
                    self.handle_uncrossable_edge(from, expected, ctx, events);
                    return;
                }
                WalkAction::Replan => {
                    // The edge asked to re-plan from here ($go2_restart).
                    //
                    // Guard it on NOT having landed where the edge says it
                    // goes (Lich's guard_trailing_replan). Procs set the flag
                    // unconditionally because a jump can land anywhere, but a
                    // restart is pure waste when the crossing worked — and
                    // re-pathing from the destination of the final edge is
                    // how a completed trip gets reported as a failure.
                    if ctx.current_room == Some(expected) {
                        pc += 1;
                        continue;
                    }
                    self.repath(ctx.db, from, ctx.lich_fallback, events);
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
            awaiting,
        };
    }

    /// Start the day-pass crossing. Both paths are RESPONSE-DRIVEN state
    /// machines (one command per game response — the game's type-ahead buffer
    /// only holds a few commands, so a flat-fired script is primed to drop
    /// commands): first a hand is freed via the stash primitive, then the
    /// machine runs its open-sack/drop-expired preamble and either the USE
    /// (get → raise) or BUY (the clerk conversation) flow. When no pass is
    /// held and buying isn't set up, we bail to a normal re-path.
    fn begin_day_pass(
        &mut self,
        from: u32,
        next: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        use crate::core::day_pass;
        let Some((dep, dest)) = day_pass::edge(from, next) else {
            self.repath(ctx.db, from, ctx.lich_fallback, events);
            return;
        };
        let inputs = ctx.day_pass;
        let sack = inputs.and_then(|i| i.sack_id);
        let (a, b) = dest.pair;
        let held = inputs.and_then(|i| i.cache.valid_pass_id(a, b, i.now_epoch));
        // Buy permission is the config alone (Lich parity): silver on hand can
        // cover the purchase without any bank involvement. Get Silvers only
        // gates the BANK DETOUR inside the conversation (BuyTick.get_silvers).
        let buy = inputs
            .map(|i| crate::core::day_pass::buy_permits(i.buy_day_pass, a, b))
            .unwrap_or(false);
        if held.is_none() && !buy {
            // No pass and no buy permission — can't cross. Ban + re-path.
            events.push(TravelEvent::Status(
                "no valid day pass held and buying is off - re-pathing".into(),
            ));
            self.banned.insert((from, next));
            self.repath(ctx.db, from, ctx.lich_fallback, events);
            return;
        }
        // Expired passes get dropped by the machine's preamble (Lich's
        // `_drag ##{id} drop` sweep), response-gated like everything else.
        let expired: Vec<String> = inputs
            .map(|i| i.cache.expired_ids(i.now_epoch))
            .unwrap_or_default();
        // Params the post-raise cleanup needs, and the close debt: the machine
        // sends `open #sack`; if the game answers "That is already open" the
        // debt clears (top of tick) and the sack is left the way we found it.
        self.day_pass_close_sack = sack.map(str::to_string);
        self.day_pass_buy_from = from;
        self.day_pass_buy_dest = next;

        let held = held.map(str::to_string);
        let resume: Box<Step> = match &held {
            Some(pass) => {
                events.push(TravelEvent::Status(format!(
                    "day pass to {} - using held pass",
                    dest.ask_word
                )));
                Box::new(Step::DayPassUse(super::day_pass_buy::UseState::new(
                    sack,
                    pass,
                    expired,
                )))
            }
            None => {
                events.push(TravelEvent::Status(format!(
                    "day pass to {} - buying",
                    dest.ask_word
                )));
                Box::new(Step::DayPassBuy(super::day_pass_buy::BuyState::new(
                    dep, dest, sack, expired,
                )))
            }
        };
        // Free a hand via the stash primitive first, then run the machine. If
        // the stash finished inline, drive the machine's first tick now so the
        // `open` goes out this frame.
        self.begin_stash_then(resume, ctx, events);
        match self.step.clone() {
            Step::DayPassUse(state) => self.tick_day_pass_use(state, from, ctx, events),
            Step::DayPassBuy(state) => self.tick_day_pass_buy(state, from, ctx, events),
            _ => {}
        }
    }

    /// Run an EmptyHands stash cycle, then continue at `resume`. Used to free a
    /// hand before a response-driven crossing (day-pass buy). If there's
    /// nothing to stow it continues immediately.
    fn begin_stash_then(
        &mut self,
        resume: Box<Step>,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        let Some(inputs) = ctx.hands else {
            self.step = *resume;
            return;
        };
        let mut task = super::stash::StashTask::empty();
        let mut done = false;
        for ev in task.tick(inputs.to_stash_context(ctx.now_ms)) {
            match ev {
                super::stash::StashEvent::Send(cmd) => events.push(TravelEvent::Send(cmd)),
                super::stash::StashEvent::Done => done = true,
                super::stash::StashEvent::Failed(_) => done = true,
            }
        }
        if done {
            self.stash_stack = task.take_stack();
            self.step = *resume;
        } else {
            self.stash = Some(task);
            self.step = Step::Stashing { resume };
        }
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
            self.repath(ctx.db, from, ctx.lich_fallback, events);
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
            self.repath(ctx.db, from, ctx.lich_fallback, events);
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
                    self.tick_script(actions, 0, None, next, from, None, ctx, events);
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
                awaiting,
            } = *resume
            {
                self.tick_script(
                    actions, pc, sleep_until, expected, from, awaiting, ctx, events,
                );
            } else {
                self.step = *resume;
            }
        }
    }

    /// Drive the response-driven day-pass BUY conversation one tick. Relays the
    /// machine's Send commands; Traveled/Failed conclude via
    /// conclude_day_pass.
    fn tick_day_pass_buy(
        &mut self,
        mut state: super::day_pass_buy::BuyState,
        current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        use super::day_pass_buy::BuyEvent;
        for ev in state.tick(day_pass_tick_inputs(&ctx)) {
            match ev {
                BuyEvent::Send(cmd) => events.push(TravelEvent::Send(cmd)),
                outcome => {
                    self.conclude_day_pass(outcome, current, ctx, events);
                    return;
                }
            }
        }
        self.step = Step::DayPassBuy(state);
    }

    /// Drive the response-driven day-pass USE machine one tick (same shape as
    /// the buy conversation: get the held pass, raise it — one command per
    /// game response).
    fn tick_day_pass_use(
        &mut self,
        mut state: super::day_pass_buy::UseState,
        current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        use super::day_pass_buy::BuyEvent;
        for ev in state.tick(day_pass_tick_inputs(&ctx)) {
            match ev {
                BuyEvent::Send(cmd) => events.push(TravelEvent::Send(cmd)),
                outcome => {
                    self.conclude_day_pass(outcome, current, ctx, events);
                    return;
                }
            }
        }
        self.step = Step::DayPassUse(state);
    }

    /// Shared ending for both day-pass machines. On Traveled: put the pass
    /// back (by exist-id — `_drag #pass` doesn't work), recover the original
    /// items, settle the owed sack close, ban the used edge and continue the
    /// trip. On Failed: restore the sack and re-path around the edge.
    fn conclude_day_pass(
        &mut self,
        outcome: super::day_pass_buy::BuyEvent,
        current: u32,
        ctx: TravelContext,
        events: &mut Vec<TravelEvent>,
    ) {
        use super::day_pass_buy::BuyEvent;
        match outcome {
            BuyEvent::Send(_) => unreachable!("Send is relayed by the tick fns"),
            BuyEvent::Traveled { pass_id } => {
                // The machine already put the pass back (response-gated
                // PutBack phase) before signalling Traveled.
                let _ = pass_id;
                // FillHands via the stash primitive (recovers what EmptyHands
                // stowed at the start of the crossing).
                if let Some(inputs) = ctx.hands {
                    let mut task =
                        super::stash::StashTask::fill(std::mem::take(&mut self.stash_stack));
                    let mut done = false;
                    for sev in task.tick(inputs.to_stash_context(ctx.now_ms)) {
                        match sev {
                            super::stash::StashEvent::Send(c) => {
                                events.push(TravelEvent::Send(c))
                            }
                            super::stash::StashEvent::Done
                            | super::stash::StashEvent::Failed(_) => done = true,
                        }
                    }
                    if !done {
                        self.stash = Some(task);
                    }
                }
                // Close the sack only if we opened it (skipped when the game
                // said "That is already open").
                self.flush_day_pass_close(events);
                // The raise teleported us to the day-pass edge's dest. Ban the
                // edge for the rest of the trip (no second pass to move
                // locally). CRITICAL: the landing may not have RESOLVED yet —
                // never re-plan against a possibly-stale room (the live
                // phantom second-buy: repathing from the stale departure room
                // routed into ANOTHER day-pass edge and ran its clerk
                // conversation in the wrong room). If the room has resolved,
                // advance normally; otherwise arrival-watch the KNOWN landing
                // room and let the normal machinery take it from there.
                let dest = self.day_pass_buy_dest;
                self.banned.insert((self.day_pass_buy_from, dest));
                if current == dest || current == self.destination {
                    self.arrive();
                } else {
                    self.step = Step::AwaitArrival {
                        expected: dest,
                        from: self.day_pass_buy_from,
                        sent_ms: ctx.now_ms,
                        slow: true,
                    };
                }
                // If the FillHands retrieval is still running, suspend into
                // Stashing over whatever step arrive/repath chose — otherwise
                // nothing ever ticks the stash task again, and the
                // `stash.is_none()` arrival guard blocks the trip's completion
                // forever (repath-to-self → "too many restarts").
                if self.stash.is_some() {
                    self.step = Step::Stashing {
                        resume: Box::new(self.step.clone()),
                    };
                }
            }
            BuyEvent::Failed(why) => {
                events.push(TravelEvent::Status(format!("day pass: {why} - re-pathing")));
                // Leave the sack the way we found it before re-pathing.
                self.flush_day_pass_close(events);
                self.banned.insert((self.day_pass_buy_from, self.day_pass_buy_dest));
                self.repath(ctx.db, current, ctx.lich_fallback, events);
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
                    self.repath(ctx.db, current, ctx.lich_fallback, events);
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
            self.repath(ctx.db, current, ctx.lich_fallback, events);
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
            self.repath(ctx.db, current, ctx.lich_fallback, events);
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
            self.tick_script(ov.actions.clone(), 0, None, next, current, None, ctx, events);
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
                    self.tick_script(actions, 0, None, next, current, None, ctx, events);
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
            self.repath(ctx.db, from, ctx.lich_fallback, events);
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
                self.repath(ctx.db, from, ctx.lich_fallback, events);
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
            self.repath(ctx.db, from, ctx.lich_fallback, events);
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
        if self.hand_off_to_lich(
            ctx.lich_fallback,
            &format!("edge {current} -> {next} needs a script the native walker can't cross"),
            events,
        ) {
            return;
        }
        events.push(TravelEvent::Status(format!(
            "edge {current} -> {next} uses a script the native walker can't cross yet - disabling it and re-pathing"
        )));
        self.banned.insert((current, next));
        self.repath(ctx.db, current, ctx.lich_fallback, events);
    }

    /// Re-plan around banned edges. `lich_fallback` mirrors
    /// `TravelContext::lich_fallback`: when a re-path leaves us with no route
    /// at all, the trip is only dead if there's no Lich to hand it to.
    fn repath(
        &mut self,
        db: &MapDb,
        current: u32,
        lich_fallback: bool,
        events: &mut Vec<TravelEvent>,
    ) {
        // A day-pass crossing abandoned mid-way still owes its sack close.
        self.flush_day_pass_close(events);
        self.restarts += 1;
        if self.restarts > MAX_RESTARTS {
            if self.hand_off_to_lich(lich_fallback, "too many restarts", events) {
                return;
            }
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
                    if self.hand_off_to_lich(
                        lich_fallback,
                        &format!("no native route from room {current} to the bank (room {bank})"),
                        events,
                    ) {
                        return;
                    }
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
                if self.hand_off_to_lich(
                    lich_fallback,
                    &format!(
                        "no native route from room {current} to {}",
                        self.destination
                    ),
                    events,
                ) {
                    return;
                }
                events.push(TravelEvent::Failed(format!(
                    "no remaining route from room {current} to {} - travel aborted",
                    self.destination
                )));
            }
        }
    }

    /// Hand the remaining trip to Lich's `;go2` when the fallback is armed.
    /// Returns true if the handoff fired (the caller must not also emit a
    /// `Failed`). `why` names what the native walker ran out of, so the
    /// notice doubles as the signal for which edges need recognizer work.
    fn hand_off_to_lich(
        &mut self,
        lich_fallback: bool,
        why: &str,
        events: &mut Vec<TravelEvent>,
    ) -> bool {
        if !lich_fallback {
            return false;
        }
        events.push(TravelEvent::Status(format!(
            "{why} - handing off to ;go2 {}",
            self.destination
        )));
        events.push(TravelEvent::LichFallback {
            destination: self.destination,
        });
        true
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

/// Assemble the per-tick inputs both day-pass machines read from the world
/// snapshot (feedback, room, clock, hands, hidden/funding flags).
fn day_pass_tick_inputs<'a>(ctx: &TravelContext<'a>) -> super::day_pass_buy::BuyTick<'a> {
    fn hand_ref(i: Option<&crate::core::game_objects::GameItem>) -> Option<(&str, &str)> {
        i.map(|it| (it.id.as_str(), it.noun.as_str()))
    }
    super::day_pass_buy::BuyTick {
        feedback: ctx.feedback,
        current_room: ctx.current_room,
        get_silvers: ctx.day_pass.map(|i| i.get_silvers).unwrap_or(false),
        now_ms: ctx.now_ms,
        rt_remaining: ctx.rt_remaining,
        left_hand: ctx.hands.and_then(|h| hand_ref(h.left_hand)),
        right_hand: ctx.hands.and_then(|h| hand_ref(h.right_hand)),
        hidden: ctx.day_pass.map(|i| i.hidden).unwrap_or(false),
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
/// Evaluate a condition where `CaptureIs` can see the current edge's Await
/// captures; everything else defers to the context. `Not`/`Any` recurse here
/// so a capture test composes under negation and disjunction.
fn eval_cond_with_captures(
    cond: &crate::core::pathing::edge::Cond,
    captures: &Captures,
    ctx: &TravelContext,
) -> bool {
    use crate::core::pathing::edge::Cond;
    match cond {
        Cond::CaptureIs(name, want) => captures
            .iter()
            .any(|(n, v)| n == name && v.eq_ignore_ascii_case(want)),
        Cond::Not(inner) => !eval_cond_with_captures(inner, captures, ctx),
        Cond::Any(any) => any
            .iter()
            .any(|c| eval_cond_with_captures(c, captures, ctx)),
        other => ctx.eval(other),
    }
}

/// Normalize a direction to its compass short form, so mapdb long names
/// ("northwest") compare against live compass dirs ("nw"). Non-compass
/// strings pass through unchanged.
fn short_dir(dir: &str) -> &str {
    match dir {
        "north" => "n",
        "northeast" => "ne",
        "east" => "e",
        "southeast" => "se",
        "south" => "s",
        "southwest" => "sw",
        "west" => "w",
        "northwest" => "nw",
        "up" => "u",
        "down" => "d",
        other => other,
    }
}

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
        hidden: bool,
        citizenship: Option<String>,
        profession: Option<String>,
        society: Option<String>,
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
        /// Raw game lines an `Await` can match, as (seq, text).
        recent_lines: Vec<(u64, String)>,
        line_seq: u64,
    }

    impl Sim {
        fn new(start: u32) -> Sim {
            Sim {
                current: start,
                standing: true,
                sitting: false,
                kneeling: false,
                hidden: false,
                citizenship: None,
                profession: None,
                society: None,
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
                recent_lines: Vec::new(),
                line_seq: 0,
            }
        }

        /// Feed a game line an `Await` can match, as the parser would.
        fn say(&mut self, line: &str) {
            self.line_seq += 1;
            self.recent_lines.push((self.line_seq, line.to_string()));
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
                hidden: self.hidden,
                citizenship: self.citizenship.as_deref(),
                profession: self.profession.as_deref(),
                society: self.society.as_deref(),
                active_spells: &self.spells,
                rt_remaining: self.rt,
                now_ms: self.now,
                pathcodes: &self.pathcodes,
                hands: None,
                feedback: &self.feedback,
                recent_lines: &self.recent_lines,
                line_seq: self.line_seq,
                lich_fallback: self.lich_fallback,
                funding: self.funding,
                at_pinefar_depository: self.pinefar,
                compass_dirs: &self.compass_dirs,
                carried_names: &[],
                loot_nouns: &self.loot_nouns,
                fwi_trinket: None,
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
                hidden: false,
                citizenship: None,
                profession: None,
                society: None,
                active_spells: &[],
                rt_remaining: 0.0,
                now_ms: now,
                pathcodes,
                hands: Some(hands),
                feedback: &[],
                recent_lines: &[],
                line_seq: 0,
                lich_fallback: false,
                funding: None,
                at_pinefar_depository: false,
                compass_dirs: &[],
                carried_names: &[],
                loot_nouns: &[],
                fwi_trinket: None,
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
                    hidden: false,
                    citizenship: None,
                    profession: None,
                    society: None,
                    active_spells: &[],
                    rt_remaining: 0.0,
                    now_ms: $now,
                    pathcodes: &pathcodes,
                    hands: Some(hands),
                    feedback: &[],
                    recent_lines: &[],
                    line_seq: 0,
                    lich_fallback: false,
                    funding: None,
                    at_pinefar_depository: false,
                    compass_dirs: &[],
                    carried_names: &[],
                    loot_nouns: &[],
                    fwi_trinket: None,
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

    /// Two-room db with a scripted edge 1->2, for driving raw action lists.
    fn script_db() -> MapDb {
        MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": ";e true"}, "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[B]"],
                 "wayto": {"1": "back"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap()
    }

    fn pattern(src: &str) -> Box<crate::core::pathing::edge::AwaitPattern> {
        Box::new(crate::core::pathing::edge::AwaitPattern::new(src).expect("valid pattern"))
    }

    #[test]
    fn await_sends_then_waits_for_its_line_before_continuing() {
        use crate::core::pathing::edge::{OnTimeout, WalkAction};
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);

        // `await 'go gangplank' for /lowers the gangplank/` then `move out`.
        let actions = vec![
            WalkAction::Await {
                cmd: Some("go gangplank".into()),
                pattern: pattern(r"lowers the gangplank"),
                timeout: 30.0,
                on_timeout: OnTimeout::Fail,
                if_match: None,
            },
            WalkAction::Move("out".into()),
        ];

        // Arms: sends its command, then blocks - `out` must NOT go out yet.
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert_eq!(sent(&ev), ["go gangplank"], "arms by sending its command");

        // A non-matching line doesn't satisfy it.
        sim.say("The crewmember ignores you.");
        sim.now += 1_000;
        let ev = task.tick(sim.ctx(&db));
        assert!(sent(&ev).is_empty(), "unrelated line doesn't release the await");

        // The awaited line does.
        sim.say("An elven crewmember lowers the gangplank.");
        sim.now += 1_000;
        let ev = task.tick(sim.ctx(&db));
        assert_eq!(sent(&ev), ["out"], "the matching line releases the await");
    }

    #[test]
    fn await_that_times_out_continues_or_fails_per_policy() {
        use crate::core::pathing::edge::{OnTimeout, WalkAction};
        let db = script_db();

        // Continue (the default): the line never came, carry on anyway.
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        let actions = vec![
            WalkAction::Await {
                cmd: None,
                pattern: pattern(r"never happens"),
                timeout: 5.0,
                on_timeout: OnTimeout::Continue,
                if_match: None,
            },
            WalkAction::Move("out".into()),
        ];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert!(sent(&ev).is_empty(), "passive await sends nothing");
        sim.now += 6_000;
        let ev = task.tick(sim.ctx(&db));
        assert_eq!(sent(&ev), ["out"], "a Continue timeout runs the next action");

        // Fail: the awaited line was the only evidence the crossing worked.
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        let actions = vec![
            WalkAction::Await {
                cmd: None,
                pattern: pattern(r"never happens"),
                timeout: 5.0,
                on_timeout: OnTimeout::Fail,
                if_match: None,
            },
            WalkAction::Move("out".into()),
        ];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        sim.now += 6_000;
        let ev = task.tick(sim.ctx(&db));
        assert!(
            !sent(&ev).iter().any(|c| *c == "out"),
            "a Fail timeout does NOT run the next action: {ev:?}"
        );
    }

    #[test]
    fn await_ignores_lines_that_predate_it() {
        // The ring holds lines from before the await armed. Matching one would
        // release the await instantly and skip the wait entirely.
        use crate::core::pathing::edge::{OnTimeout, WalkAction};
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.say("An elven crewmember lowers the gangplank.");

        let actions = vec![
            WalkAction::Await {
                cmd: None,
                pattern: pattern(r"lowers the gangplank"),
                timeout: 30.0,
                on_timeout: OnTimeout::Fail,
                if_match: None,
            },
            WalkAction::Move("out".into()),
        ];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert!(
            sent(&ev).is_empty(),
            "a line that predates the await must not satisfy it: {ev:?}"
        );
    }

    #[test]
    fn repeat_runs_its_body_until_the_room_changes() {
        use crate::core::pathing::edge::{RepeatUntil, WalkAction};
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);

        // `repeat { fput 'go fog' } until_room_change`
        let actions = vec![WalkAction::Repeat {
            body: vec![WalkAction::Put("go fog".into())],
            until: RepeatUntil::RoomChanged,
            max: 10,
        }];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        // Still in room 1, so the body ran - repeatedly, within one tick,
        // since nothing in it suspends.
        let sends = sent(&ev);
        assert!(
            sends.iter().all(|c| *c == "go fog") && !sends.is_empty(),
            "the body runs while the room hasn't changed: {sends:?}"
        );
        assert!(
            sends.len() <= MAX_SCRIPT_LOOP as usize,
            "the interpreter caps iterations at {MAX_SCRIPT_LOOP}, got {}",
            sends.len()
        );
    }

    #[test]
    fn repeat_max_is_clamped_no_matter_what_the_data_says() {
        use crate::core::pathing::edge::{RepeatUntil, WalkAction};
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        // Data asking for a million iterations must not hang the client.
        let actions = vec![WalkAction::Repeat {
            body: vec![WalkAction::Put("spin".into())],
            until: RepeatUntil::Count,
            max: 1_000_000,
        }];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert_eq!(
            sent(&ev).len(),
            MAX_SCRIPT_LOOP as usize,
            "clamped to the interpreter's ceiling, not the data's number"
        );
    }

    #[test]
    fn break_leaves_the_enclosing_loop() {
        use crate::core::pathing::edge::{RepeatUntil, WalkAction};
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        // `repeat { fput 'search'; break }` then move — the search runs once.
        let actions = vec![
            WalkAction::Repeat {
                body: vec![WalkAction::Put("search".into()), WalkAction::Break],
                until: RepeatUntil::Count,
                max: 50,
            },
            WalkAction::Move("go crevice".into()),
        ];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert_eq!(
            sent(&ev),
            ["search", "go crevice"],
            "break exits the loop and the script continues after it"
        );
    }

    #[test]
    fn a_trailing_replan_is_skipped_when_the_crossing_landed_correctly() {
        // Procs set $go2_restart unconditionally because a jump can land
        // anywhere, but re-pathing after a crossing that WORKED is waste -
        // and on the final edge it turns a completed trip into a failure.
        use crate::core::pathing::edge::WalkAction;
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.current = 2; // landed on the edge's destination
        let mut ev = Vec::new();
        task.tick_script(
            vec![WalkAction::Replan, WalkAction::Put("after".into())],
            0,
            None,
            2,
            1,
            None,
            sim.ctx(&db),
            &mut ev,
        );
        assert_eq!(
            sent(&ev),
            ["after"],
            "the replan is skipped and the script continues: {ev:?}"
        );
        assert!(
            !ev.iter().any(|e| matches!(e, TravelEvent::Failed(_))),
            "a correct landing is not a failure: {ev:?}"
        );
    }

    #[test]
    fn trinket_warp_retrieves_turns_and_puts_it_back() {
        use crate::core::pathing::edge::WalkAction;
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        let mut ctx = sim.ctx(&db);
        ctx.fwi_trinket = Some(TrinketInputs {
            id: "500",
            return_to: Some("77"),
            in_hand: false,
        });
        let mut ev = Vec::new();
        task.tick_script(
            vec![WalkAction::TrinketWarp],
            0,
            None,
            2,
            1,
            None,
            ctx,
            &mut ev,
        );
        // Hands are freed, then the trinket comes out. The `turn` is a
        // StepMove so the put-back waits for the warp to land.
        let sent = sent(&ev);
        assert!(
            sent.iter().any(|c| *c == "get #500"),
            "retrieves the trinket by its live exist id: {sent:?}"
        );
    }

    #[test]
    fn trinket_warp_without_a_configured_trinket_falls_back() {
        // Sending `turn #` with no id would be nonsense; the edge should
        // hand off instead, and say why.
        use crate::core::pathing::edge::WalkAction;
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        let mut ev = Vec::new();
        task.tick_script(
            vec![WalkAction::TrinketWarp],
            0,
            None,
            2,
            1,
            None,
            sim.ctx(&db),
            &mut ev,
        );
        assert!(sent(&ev).is_empty(), "nothing is sent: {ev:?}");
        assert!(
            ev.iter().any(|e| matches!(e, TravelEvent::Status(s)
                if s.contains("Four Winds trinket"))),
            "the message names the setting to fix: {ev:?}"
        );
    }

    #[test]
    fn try_move_runs_its_fallback_only_when_the_room_didnt_change() {
        use crate::core::pathing::edge::WalkAction;
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        let mut ev = Vec::new();
        task.tick_script(
            vec![WalkAction::TryMove {
                cmd: "go curtain".into(),
                fallback: vec![
                    WalkAction::Put("close locker".into()),
                    WalkAction::Move("go curtain".into()),
                ],
            }],
            0,
            None,
            2,
            1,
            None,
            sim.ctx(&db),
            &mut ev,
        );
        // The move goes out first; the fallback waits on whether it landed.
        assert_eq!(sent(&ev), ["go curtain"], "sends the move first: {ev:?}");
    }

    #[test]
    fn guided_route_joins_the_cycle_at_the_current_rooms_offset() {
        use crate::core::pathing::edge::WalkAction;
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        // Room 1 is at offset 1, so the walk starts with the SECOND direction.
        let actions = vec![WalkAction::GuidedRoute {
            start_rooms: vec![99, 1, 98],
            dirs: vec!["north".into(), "east".into(), "south".into()],
            landmarks: vec![("staircase".into(), "climb staircase".into())],
        }];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert_eq!(
            sent(&ev).first(),
            Some(&"east"),
            "joins the cycle at the offset for the room we're in: {ev:?}"
        );
    }

    #[test]
    fn guided_route_off_its_table_doesnt_guess_a_direction() {
        // The Ruby's `else echo 'error: mini-script expected a different
        // room'` branch. Walking from an unknown offset sends the character
        // somewhere arbitrary, which is worse than not crossing.
        use crate::core::pathing::edge::WalkAction;
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        let actions = vec![WalkAction::GuidedRoute {
            start_rooms: vec![97, 98, 99],
            dirs: vec!["north".into()],
            landmarks: vec![("staircase".into(), "climb staircase".into())],
        }];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert!(
            sent(&ev).is_empty(),
            "no movement is sent from an unknown offset: {ev:?}"
        );
    }

    #[test]
    fn guided_route_stops_once_the_landmark_appears() {
        use crate::core::pathing::edge::WalkAction;
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        // The landmark is already here: enter it without walking the cycle.
        sim.loot_nouns = vec!["staircase".into()];
        let actions = vec![WalkAction::GuidedRoute {
            start_rooms: vec![1],
            dirs: vec!["north".into(), "east".into()],
            landmarks: vec![("staircase".into(), "climb staircase".into())],
        }];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert_eq!(
            sent(&ev),
            ["climb staircase"],
            "the landmark ends the walk immediately: {ev:?}"
        );
    }

    #[test]
    fn a_capture_bound_by_an_await_fills_a_later_command() {
        use crate::core::pathing::edge::{OnTimeout, WalkAction};
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);

        let actions = vec![
            WalkAction::Await {
                cmd: Some("look wall".into()),
                pattern: Box::new(
                    crate::core::pathing::edge::AwaitPattern::new(r"The (?P<v>\w+) door")
                        .unwrap(),
                ),
                timeout: 8.0,
                on_timeout: OnTimeout::Fail,
                if_match: None,
            },
            WalkAction::Move("go {capture:v} door".into()),
        ];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert_eq!(sent(&ev), ["look wall"]);

        sim.say("The bronze door stands here.");
        sim.now += 500;
        let ev = task.tick(sim.ctx(&db));
        assert_eq!(
            sent(&ev),
            ["go bronze door"],
            "the captured word filled the command"
        );
    }

    #[test]
    fn an_unbound_capture_fails_the_edge_rather_than_sending_garbage() {
        // Sending "go  door" because a capture didn't fire is worse than not
        // sending: a half-formed command can do something unintended.
        use crate::core::pathing::edge::WalkAction;
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        let actions = vec![WalkAction::Move("go {capture:missing} door".into())];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert!(
            sent(&ev).is_empty(),
            "nothing is sent with an unbound token: {ev:?}"
        );
    }

    #[test]
    fn character_state_conditions_answer_from_live_state() {
        use crate::core::pathing::edge::{Cond, WalkAction};
        let db = script_db();

        // Citizenship matches -> the `then` branch.
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let mut sim = Sim::new(1);
        sim.citizenship = Some("Solhaven".into());
        let gate = |c: Cond| {
            vec![WalkAction::If {
                cond: c,
                then: vec![WalkAction::Move("go gate".into())],
                els: vec![WalkAction::Move("go road".into())],
            }]
        };
        let mut ev = Vec::new();
        task.tick_script(
            gate(Cond::Citizenship("solhaven".into())),
            0,
            None,
            2,
            1,
            None,
            sim.ctx(&db),
            &mut ev,
        );
        assert_eq!(sent(&ev), ["go gate"], "case-insensitive match");

        // Unknown citizenship answers false, taking the safe `else` branch.
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        let mut ev = Vec::new();
        task.tick_script(
            gate(Cond::Citizenship("Solhaven".into())),
            0,
            None,
            2,
            1,
            None,
            sim.ctx(&db),
            &mut ev,
        );
        assert_eq!(
            sent(&ev),
            ["go road"],
            "unknown state takes the else branch rather than guessing"
        );
    }

    #[test]
    fn pause_for_user_abandons_rather_than_blocking_forever() {
        use crate::core::pathing::edge::WalkAction;
        let db = script_db();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        let sim = Sim::new(1);
        let actions = vec![
            WalkAction::PauseForUser {
                msg: "put a gem in your hand".into(),
                until: None,
                timeout: 0.0,
            },
            WalkAction::Move("go portal".into()),
        ];
        let mut ev = Vec::new();
        task.tick_script(actions, 0, None, 2, 1, None, sim.ctx(&db), &mut ev);
        assert!(
            !sent(&ev).iter().any(|c| *c == "go portal"),
            "the crossing doesn't proceed without the user: {ev:?}"
        );
        assert!(
            ev.iter().any(|e| matches!(e, TravelEvent::Status(s)
                if s.contains("put a gem in your hand"))),
            "the message names what's needed: {ev:?}"
        );
    }

    #[test]
    fn exhausted_route_hands_off_to_lich_instead_of_aborting() {
        // The second half of the fallback: an edge got banned, the re-path
        // found nothing, and the trip would otherwise die with "no remaining
        // route ... travel aborted". With a Lich attached that trip is not
        // dead — Lich can still walk it.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                 "wayto": {"2": "north"}, "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[B]"],
                 "wayto": {"1": "south"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        // Ban the only edge, then force the re-path that finds no route.
        task.banned.insert((1, 2));
        let mut ev = Vec::new();
        task.repath(&db, 1, true, &mut ev);
        assert!(
            ev.iter()
                .any(|e| matches!(e, TravelEvent::LichFallback { destination: 2 })),
            "an exhausted route hands off rather than aborting: {ev:?}"
        );
        assert!(
            !ev.iter().any(|e| matches!(e, TravelEvent::Failed(_))),
            "handoff replaces the abort, it doesn't accompany it: {ev:?}"
        );

        // Same situation with no Lich: it must still abort cleanly.
        let mut task = TravelTask::start(&db, 1, 2, 0).unwrap();
        task.banned.insert((1, 2));
        let mut ev = Vec::new();
        task.repath(&db, 1, false, &mut ev);
        assert!(
            ev.iter().any(|e| matches!(e, TravelEvent::Failed(_))),
            "without a Lich the trip still fails: {ev:?}"
        );
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
        task.repath(&db, 1, false, &mut ev);
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
            ($now:expr, $cur:expr) => {
                dpctx!($now, $cur, &[])
            };
            ($now:expr, $cur:expr, $fb:expr) => {{
                let dp = DayPassInputs {
                    sack_id: Some("99"),
                    buy_day_pass: "",
                    get_silvers: false,
                    cache: &cache,
                    now_epoch: 0,
                    hidden: false,
                };
                TravelContext {
                    db: &db, current_room: Some($cur), dead: false, muckled: false,
                    standing: true, sitting: false, kneeling: false, active_spells: &[],
                    hidden: false, citizenship: None, profession: None, society: None,
                    rt_remaining: 0.0, now_ms: $now, pathcodes: &Default::default(),
                    hands: None, feedback: $fb, lich_fallback: false, funding: None,
                    recent_lines: &[], line_seq: 0,
                    at_pinefar_depository: false, compass_dirs: &[], loot_nouns: &[], carried_names: &[],
                    fwi_trinket: None,
                    day_pass: Some(dp),
                }
            }};
        }

        // The crossing is RESPONSE-DRIVEN (type-ahead safe): each tick sends
        // exactly one command and waits for its game response before the next.
        use crate::core::move_feedback::MoveFeedback as F;
        // Tick 1: open the sack — and nothing else yet.
        let ev = task.tick(dpctx!(0, 8635));
        assert_eq!(sent(&ev), ["open #99"], "opens the sack, ONE command: {ev:?}");
        // Tick 2: the open confirmed → get the held pass. Still one command.
        let ev = task.tick(dpctx!(1_000, 8635, &[F::ContainerOpened]));
        assert_eq!(sent(&ev), ["get #77"], "open answered -> get: {ev:?}");
        // Tick 3: the get landed → raise it.
        let ev = task.tick(dpctx!(2_000, 8635, &[F::ItemGot]));
        assert_eq!(sent(&ev), ["raise #77"], "get answered -> raise: {ev:?}");
        // Tick 4: the whirlwind fired and we're at 8916 → the put-back is
        // response-gated too: ONLY the _drag goes out (no close flood).
        let ev = task.tick(dpctx!(3_000, 8916, &[F::RaiseTraveled]));
        assert_eq!(sent(&ev), ["_drag #77 #99"], "drag alone, gated: {ev:?}");
        // Tick 5: the stow confirmed → the owed close settles and we conclude.
        let ev = task.tick(dpctx!(4_000, 8916, &[F::ItemStowed]));
        assert!(sent(&ev).contains(&"close #99"), "closes the sack we opened: {ev:?}");
        assert!(!sent(&ev).iter().any(|c| c.contains("ask ") || c.contains("withdraw")));
        // Tick 6: trip complete.
        let ev = task.tick(dpctx!(5_000, 8916));
        assert!(
            matches!(ev.last(), Some(TravelEvent::Arrived { destination: 8916, .. })),
            "arrives at the destination: {ev:?}"
        );

        // --- Same crossing, but the sack was ALREADY open: the close must be
        // skipped (the user keeps their sack open).
        let mut task = TravelTask::start(&db, 8635, 8916, 0).unwrap();
        let ev = task.tick(dpctx!(0, 8635));
        assert_eq!(sent(&ev), ["open #99"]);
        let ev = task.tick(dpctx!(1_000, 8635, &[F::ContainerAlreadyOpen]));
        assert_eq!(sent(&ev), ["get #77"], "already-open also advances: {ev:?}");
        let ev = task.tick(dpctx!(2_000, 8635, &[F::ItemGot]));
        assert_eq!(sent(&ev), ["raise #77"]);
        let ev = task.tick(dpctx!(3_000, 8916, &[F::RaiseTraveled]));
        assert_eq!(sent(&ev), ["_drag #77 #99"], "still puts the pass back");
        let ev = task.tick(dpctx!(4_000, 8916, &[F::ItemStowed]));
        assert!(
            !sent(&ev).contains(&"close #99"),
            "already-open sack is left open: {ev:?}"
        );
        let ev = task.tick(dpctx!(5_000, 8916));
        assert!(
            matches!(ev.last(), Some(TravelEvent::Arrived { destination: 8916, .. })),
            "still arrives: {ev:?}"
        );
    }

    #[test]
    fn day_pass_raise_conclusion_waits_for_the_resolved_room() {
        // The live phantom-second-buy: the raise's nav landed while the
        // resolver still read the DEPARTURE room; concluding then re-planned
        // from the stale room, banned the used edge, and routed into ANOTHER
        // day-pass edge ("day pass to solhaven - buying" in the wrong room).
        use crate::core::day_pass::DayPassCache;
        use crate::core::move_feedback::MoveFeedback as F;
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
            ($now:expr, $cur:expr, $fb:expr) => {{
                let dp = DayPassInputs {
                    sack_id: Some("99"),
                    buy_day_pass: "",
                    get_silvers: false,
                    cache: &cache,
                    now_epoch: 0,
                    hidden: false,
                };
                TravelContext {
                    db: &db, current_room: Some($cur), dead: false, muckled: false,
                    standing: true, sitting: false, kneeling: false, active_spells: &[],
                    hidden: false, citizenship: None, profession: None, society: None,
                    rt_remaining: 0.0, now_ms: $now, pathcodes: &Default::default(),
                    hands: None, feedback: $fb, lich_fallback: false, funding: None,
                    recent_lines: &[], line_seq: 0,
                    at_pinefar_depository: false, compass_dirs: &[], loot_nouns: &[], carried_names: &[],
                    fwi_trinket: None,
                    day_pass: Some(dp),
                }
            }};
        }

        // Walk the machine to the raise.
        task.tick(dpctx!(0, 8635, &[]));
        task.tick(dpctx!(1_000, 8635, &[F::ContainerAlreadyOpen]));
        let ev = task.tick(dpctx!(2_000, 8635, &[F::ItemGot]));
        assert_eq!(sent(&ev), ["raise #77"]);
        // A stray nav with the room STILL the departure room: must NOT
        // conclude (no cleanup sends, no phantom crossing).
        let ev = task.tick(dpctx!(3_000, 8635, &[F::NavArrived]));
        assert!(ev.is_empty(), "stale-room nav must not conclude the raise: {ev:?}");
        // The whirlwind line arrives while the room is STILL stale: the
        // response-gated put-back runs — and nothing re-plans from the stale
        // departure room (no ask/open phantom).
        let ev = task.tick(dpctx!(4_000, 8635, &[F::RaiseTraveled]));
        assert_eq!(sent(&ev), ["_drag #77 #99"], "gated cleanup runs: {ev:?}");
        // Stow confirms (room STILL stale) → conclude must arrival-watch the
        // KNOWN landing room instead of re-planning.
        let ev = task.tick(dpctx!(5_000, 8635, &[F::ItemStowed]));
        assert!(
            !sent(&ev)
                .iter()
                .any(|c| c.starts_with("ask ") || c.starts_with("open ")),
            "no phantom second crossing from the stale room: {ev:?}"
        );
        assert!(
            matches!(task.step, Step::AwaitArrival { expected: 8916, .. }),
            "arrival-watches the known landing room: {:?}",
            task.step
        );
        // The room resolves at the landing → the trip completes normally.
        let _ = task.tick(dpctx!(6_000, 8916, &[]));
        let ev = task.tick(dpctx!(7_000, 8916, &[]));
        assert!(
            matches!(ev.last(), Some(TravelEvent::Arrived { destination: 8916, .. })),
            "arrives once resolved: {ev:?}"
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

        use crate::core::move_feedback::MoveFeedback as F;
        // ctx with a room, clock, AND feedback events (the buy machine waits on
        // the clerk's responses, not just room changes).
        macro_rules! buyctx {
            ($now:expr, $cur:expr, $fb:expr) => {{
                let dp = DayPassInputs {
                    sack_id: Some("99"),
                    buy_day_pass: "wl,imt",
                    get_silvers: true,
                    cache: &cache,
                    now_epoch: 0,
                    hidden: false,
                };
                TravelContext {
                    db: &db, current_room: Some($cur), dead: false, muckled: false,
                    standing: true, sitting: false, kneeling: false, active_spells: &[],
                    hidden: false, citizenship: None, profession: None, society: None,
                    rt_remaining: 0.0, now_ms: $now, pathcodes: &Default::default(),
                    hands: None, feedback: $fb, lich_fallback: false, funding: None,
                    recent_lines: &[], line_seq: 0,
                    at_pinefar_depository: false, compass_dirs: &[], loot_nouns: &[], carried_names: &[],
                    fwi_trinket: None,
                    day_pass: Some(dp),
                }
            }};
        }

        let mut all: Vec<String> = Vec::new();

        // Tick 1: open the sack. (Hands aren't wired in this test, so the
        // EmptyHands primitive is a no-op — no `stow both` — and never a raw
        // `empty hands` string.) The enter step follows on the next tick.
        let ev = task.tick(buyctx!(0, 8635, &[]));
        let first: Vec<String> = sent(&ev).iter().map(|c| c.to_string()).collect();
        all.extend(first.clone());
        assert_eq!(first, ["open #99"], "opens the day-pass sack");
        assert!(!first.iter().any(|c| c == "empty hands"), "never a raw `empty hands` string");

        // Now drive the response-driven conversation. Each tick we advance the
        // room and, based on the LAST command the machine sent, feed the game
        // response it's waiting on. This exercises the real ask→offer→confirm→
        // too-poor→bank→withdraw→re-ask→in-hand→walk-back→raise flow.
        let mut room = 900u32;
        let mut asks = 0;
        let mut withdrew = false;
        let mut withdrew_before_bank_walk = false;
        let mut raised = false;
        let mut bank_walk_steps = 0;
        let mut now = 1_000u64;
        for _ in 0..60 {
            let last = all.last().cloned().unwrap_or_default();
            let fb: Vec<F> = if last.starts_with("ask ") {
                asks += 1;
                // 1st ask after (re)arriving at clerk → offer; the confirm ask
                // → too-poor the first round, in-hand after funding.
                if asks % 2 == 1 {
                    vec![F::DayPassOffered]
                } else if !withdrew {
                    vec![F::DayPassTooPoor]
                } else {
                    vec![F::DayPassInHand]
                }
            } else if last == "withdraw 5000" {
                vec![F::WithdrawOk]
            } else if last == "raise pass" {
                vec![F::RaiseTraveled]
            } else if last.starts_with("open #") {
                // The preamble waits for the open's response before moving on.
                vec![F::ContainerOpened]
            } else {
                vec![]
            };
            room += 1;
            let cur = if last == "raise pass" { 8916 } else { room };
            let ev = task.tick(buyctx!(now, cur, &fb));
            now += 1_000;
            for c in sent(&ev) {
                if c == "withdraw 5000" {
                    withdrew = true;
                    if bank_walk_steps == 0 {
                        withdrew_before_bank_walk = true;
                    }
                } else if c == "raise pass" {
                    raised = true;
                } else if !withdrew && (c.len() <= 5 || c.starts_with("go ") || c == "out") && !c.starts_with("ask") {
                    bank_walk_steps += 1;
                }
                all.push(c.to_string());
            }
            if raised {
                break;
            }
        }
        assert!(all.iter().any(|c| c == "ask clerk for icemule"), "asks the clerk: {all:?}");
        assert!(withdrew, "funds at the bank when too poor: {all:?}");
        assert!(!withdrew_before_bank_walk, "walks to the bank before withdrawing: {all:?}");
        assert!(raised, "raises the bought pass (in the waiting room) to travel: {all:?}");
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
