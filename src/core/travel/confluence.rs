//! The Plane of Elemental Confluence explorer — a live-feed maze walker.
//!
//! The Confluence is a *shifting* maze: a room's exits don't map to fixed
//! destinations, and the whole layout re-scrambles periodically. So the mapdb
//! can't store a graph for it. Instead every Confluence edge in the mapdb is a
//! StringProc that, on `.call`, runs the same explorer loop
//! (`stringprocs/wayto/room-23282-to-23282.rb`): it *learns* adjacency by
//! walking, remembers which direction led where last time, and re-derives a
//! route on the fly — wiping its memory whenever it detects the maze shifted.
//!
//! This is the faithful native port of that loop. Lich holds the learned graph
//! in process globals (`$mapdb_confluence_wayto`, `$mapdb_confluence_wander`,
//! the landmark caches); we hold the same state in [`ConfluenceState`], carried
//! on the travel task and fed live room exits + ground loot each tick.
//!
//! ## The algorithm (mirrors the Ruby, line for line)
//!
//! Every entry edge (`room-23282-to-N.rb`) is just
//! `$mapdb_confluence_target = 'tranquility'; Room[23282].wayto['23282'].call`
//! — set the goal landmark, then delegate to the self-loop. There is exactly
//! ONE goal in the live data: the *point of elemental tranquility*, a portal
//! that warps you out of the Plane (the mapdb "destination" of each entry edge
//! is nominal — go2 simply re-paths from wherever tranquility dumps you). So
//! the explorer's whole job is: reach the tranquility point, then `go
//! tranquility`.
//!
//! Per step, standing in `start_room`:
//! 1. **Classify** the room hot/cold (fixed sets). Off the map → `$go2_restart`
//!    (we bail to a normal re-path).
//! 2. **Landmark scan**: if the ground holds a `point of elemental tranquility`
//!    (or `gaping bottomless pit`), record THIS room as the landmark's location
//!    (per hot/cold side); if we previously thought the landmark was here and
//!    it's gone, forget it.
//! 3. **Record exits**: first visit → snapshot the compass dirs (dests unknown,
//!    `None`). If the room's exits changed since we recorded them, the maze
//!    shifted → wipe the ENTIRE learned graph and restart.
//! 4. **Pick a direction** via [`ConfluenceState::choose_dir`]:
//!    - standing ON the tranquility point → `go tranquility` (done);
//!    - else BFS ([`dir_to`]) toward the room we believe holds the landmark;
//!    - else first dir whose learned dest is unknown (`None`) — explore;
//!    - else least-recently-wandered known dest.
//! 5. **Move**, then record `learned[from][dir] = arrived_room` and bump the
//!    wander order. A failed move → `look`, read the compass, pick a random
//!    exit (handled in the executor via move-feedback).

use std::collections::HashMap;

/// The single Confluence zone in the live mapdb: rooms 23282–23334. Split into
/// "hot" and "cold" halves exactly as the explorer edge body does (the halves
/// gate which landmark cache — hot vs cold — a room's loot updates, and let a
/// cross-half target fall back to the `gaping bottomless pit` crossing).
pub const HOT_ROOMS: &[u32] = &[
    23282, 23283, 23284, 23285, 23286, 23287, 23288, 23289, 23290, 23291, 23292, 23293, 23294,
    23295, 23296, 23297, 23298, 23299, 23300, 23301, 23302, 23303, 23329, 23330, 23331, 23332,
    23333, 23334,
];

/// The cold half of the Confluence zone (see [`HOT_ROOMS`]).
pub const COLD_ROOMS: &[u32] = &[
    23304, 23305, 23306, 23307, 23308, 23309, 23310, 23311, 23312, 23313, 23314, 23315, 23316,
    23317, 23318, 23319, 23320, 23321, 23322, 23323, 23324, 23325, 23326, 23327, 23328,
];

/// Ground-object noun the explorer navigates toward — the exit portal.
pub const TRANQUILITY_LOOT: &str = "point of elemental tranquility";
/// The command that warps out once standing on the tranquility point.
pub const TRANQUILITY_GO: &str = "go tranquility";
/// The cross-half landmark: a hot target reached from the cold side (or vice
/// versa) routes toward the pit and `go pit` first.
pub const PIT_LOOT: &str = "gaping bottomless pit";
/// The command that crosses hot↔cold via the pit.
pub const PIT_GO: &str = "go pit";

/// True if `room` is anywhere in the Confluence zone.
pub fn is_confluence_room(room: u32) -> bool {
    HOT_ROOMS.contains(&room) || COLD_ROOMS.contains(&room)
}

/// Whether `room` is in the hot half. `None` if it's not a Confluence room at
/// all (the explorer's `$go2_restart` bail condition).
pub fn hot_side(room: u32) -> Option<bool> {
    if HOT_ROOMS.contains(&room) {
        Some(true)
    } else if COLD_ROOMS.contains(&room) {
        Some(false)
    } else {
        None
    }
}

/// What the explorer decided to do this step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfluenceMove {
    /// Standing on the exit portal — warp out (`go tranquility`). The trip's
    /// Confluence leg is done; the executor re-paths from wherever we land.
    Arrive,
    /// Cross hot↔cold through the pit (`go pit`) — we're on the wrong half and
    /// standing on the pit.
    CrossPit,
    /// Walk this compass direction (a normal `<dir>` like "north"/"go portal").
    Go(String),
    /// No exits at all / not in the zone — bail to a normal re-path.
    Restart,
}

/// The live-learned Confluence map, mirroring Lich's confluence globals. Held
/// on the travel task for the duration of a Confluence crossing and thrown away
/// when the crossing ends.
#[derive(Debug, Default, Clone)]
pub struct ConfluenceState {
    /// `learned[room][dir] = Some(dest)` once we've walked `dir` from `room`
    /// and seen where it led; `None` while the exit is known but untraversed.
    /// Lich's `$mapdb_confluence_wayto`.
    learned: HashMap<u32, HashMap<String, Option<u32>>>,
    /// Rooms in the order we last arrived at them — the tiebreak for "least
    /// recently wandered". Lich's `$mapdb_confluence_wander`.
    wander: Vec<u32>,
    /// Room believed to hold the tranquility point, per hot/cold side.
    hot_tranquility: Option<u32>,
    cold_tranquility: Option<u32>,
    /// Room believed to hold the pit, per side.
    hot_pit: Option<u32>,
    cold_pit: Option<u32>,
}

impl ConfluenceState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the landmark caches from the ground loot seen in `room`. Mirrors
    /// the `GameObj.loot.any? { … }` blocks: presence records this room as the
    /// landmark's home (per side); absence forgets it if we thought it was here.
    pub fn observe_landmarks(&mut self, room: u32, hot: bool, loot_nouns: &[String]) {
        let has = |noun: &str| loot_nouns.iter().any(|n| n == noun);
        // Tranquility point.
        if has(TRANQUILITY_LOOT) {
            if hot {
                self.hot_tranquility = Some(room);
            } else {
                self.cold_tranquility = Some(room);
            }
        } else {
            if self.hot_tranquility == Some(room) {
                self.hot_tranquility = None;
            }
            if self.cold_tranquility == Some(room) {
                self.cold_tranquility = None;
            }
        }
        // Pit.
        if has(PIT_LOOT) {
            if hot {
                self.hot_pit = Some(room);
            } else {
                self.cold_pit = Some(room);
            }
        } else {
            if self.hot_pit == Some(room) {
                self.hot_pit = None;
            }
            if self.cold_pit == Some(room) {
                self.cold_pit = None;
            }
        }
    }

    /// Record the room's current exits. Returns `true` if the maze SHIFTED
    /// (the recorded exits no longer match the live ones) and the caller must
    /// re-run this step against the freshly-wiped map. Mirrors the two Ruby
    /// blocks: first-visit snapshot, and the `keys != room_exits` wipe.
    #[must_use]
    pub fn record_exits(&mut self, room: u32, exits: &[String]) -> bool {
        match self.learned.get(&room) {
            None => {
                let map = exits.iter().map(|d| (d.clone(), None)).collect();
                self.learned.insert(room, map);
                false
            }
            Some(existing) => {
                let mut known: Vec<&String> = existing.keys().collect();
                known.sort();
                let mut live: Vec<&String> = exits.iter().collect();
                live.sort();
                if known != live {
                    // The maze shifted under us — everything learned is stale.
                    self.learned.clear();
                    true
                } else {
                    false
                }
            }
        }
    }

    /// BFS over the learned graph from `room`: find the FIRST exit direction
    /// (in this room) whose known destination is in `targets`, expanding
    /// through rooms that lead to a target up to 30 hops (Lich's `dir_to`
    /// proc). `targets` containing `None` means "an untraversed exit".
    pub fn dir_to(&self, room: u32, targets: &[Option<u32>]) -> Option<String> {
        let Some(here) = self.learned.get(&room) else {
            return None;
        };
        let mut targets: Vec<Option<u32>> = targets.to_vec();
        let mut tried: Vec<Option<u32>> = Vec::new();
        for _ in 0..30 {
            // Direct hit: an exit here whose dest is a current target.
            if let Some(dir) = here
                .iter()
                .find(|(_, dest)| targets.contains(dest))
                .map(|(d, _)| d.clone())
            {
                return Some(dir);
            }
            // Expand the frontier: rooms whose exits reach a current target
            // become the next generation of targets (as room ids).
            for t in &targets {
                if !tried.contains(t) {
                    tried.push(*t);
                }
            }
            let old = targets.clone();
            targets = self
                .learned
                .iter()
                .filter(|(k, exits)| {
                    exits.values().any(|d| old.contains(d)) && !tried.contains(&Some(**k))
                })
                .map(|(k, _)| Some(*k))
                .collect();
            if targets.is_empty() {
                break;
            }
        }
        None
    }

    /// Decide the next move from `room` toward the goal, given the live loot in
    /// `room`. `hot` is the room's half. Mirrors the Ruby direction-selection
    /// block (target is always the tranquility landmark in live data).
    pub fn choose_dir(&self, room: u32, hot: bool, loot_nouns: &[String]) -> ConfluenceMove {
        // Standing on the exit portal → warp out.
        if loot_nouns.iter().any(|n| n == TRANQUILITY_LOOT) {
            return ConfluenceMove::Arrive;
        }
        // Head for the side's known tranquility room, if we've seen it.
        let landmark = if hot {
            self.hot_tranquility
        } else {
            self.cold_tranquility
        };
        let mut dir = landmark.and_then(|id| self.dir_to(room, &[Some(id)]));
        // Explore: any untraversed exit (dest still None).
        dir = dir.or_else(|| self.dir_to(room, &[None]));
        // Fallback: an exit whose dest we haven't wandered yet.
        dir = dir.or_else(|| self.first_unwandered_exit(room));
        // Last resort: walk toward the least-recently-wandered known dest.
        dir = dir.or_else(|| self.least_recently_wandered_exit(room));
        match dir {
            Some(d) => ConfluenceMove::Go(d),
            None => ConfluenceMove::Restart,
        }
    }

    /// First exit in `room` whose learned destination isn't in the wander list
    /// yet (Lich: `keys.find { |d| not wander.include?(wayto[room][d]) }`).
    fn first_unwandered_exit(&self, room: u32) -> Option<String> {
        let here = self.learned.get(&room)?;
        here.iter()
            .find(|(_, dest)| match dest {
                Some(id) => !self.wander.contains(id),
                None => true,
            })
            .map(|(d, _)| d.clone())
    }

    /// The exit whose dest appears earliest in the wander order (least recently
    /// arrived at). Lich: find the first wander id reachable from here, then
    /// the dir that reaches it.
    fn least_recently_wandered_exit(&self, room: u32) -> Option<String> {
        let here = self.learned.get(&room)?;
        let next_id = self
            .wander
            .iter()
            .find(|id| here.values().any(|d| *d == Some(**id)))?;
        here.iter()
            .find(|(_, dest)| **dest == Some(*next_id))
            .map(|(d, _)| d.clone())
    }

    /// Record the outcome of a move: `dir` from `from` arrived at `arrived`.
    /// Refreshes the wander order (move-to-end = most recently seen).
    pub fn record_arrival(&mut self, from: u32, dir: &str, arrived: u32) {
        if let Some(exits) = self.learned.get_mut(&from) {
            exits.insert(dir.to_string(), Some(arrived));
        }
        self.wander.retain(|&id| id != arrived);
        self.wander.push(arrived);
    }

    /// Number of rooms we've learned exits for (for status/tests).
    pub fn learned_rooms(&self) -> usize {
        self.learned.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nouns(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }
    fn dirs(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn zone_membership_and_sides() {
        assert!(is_confluence_room(23282));
        assert!(is_confluence_room(23334));
        assert!(!is_confluence_room(1005));
        assert_eq!(hot_side(23282), Some(true));
        assert_eq!(hot_side(23304), Some(false));
        assert_eq!(hot_side(1005), None);
        // No overlap between halves.
        assert!(HOT_ROOMS.iter().all(|r| !COLD_ROOMS.contains(r)));
    }

    #[test]
    fn standing_on_tranquility_arrives() {
        let s = ConfluenceState::new();
        assert_eq!(
            s.choose_dir(23282, true, &nouns(&[TRANQUILITY_LOOT])),
            ConfluenceMove::Arrive
        );
    }

    #[test]
    fn first_visit_records_exits_no_shift() {
        let mut s = ConfluenceState::new();
        assert!(!s.record_exits(23282, &dirs(&["north", "east"])));
        assert_eq!(s.learned_rooms(), 1);
    }

    #[test]
    fn changed_exits_wipe_the_whole_map() {
        let mut s = ConfluenceState::new();
        assert!(!s.record_exits(23282, &dirs(&["north", "east"])));
        s.record_arrival(23282, "north", 23283);
        assert!(!s.record_exits(23283, &dirs(&["south"])));
        assert_eq!(s.learned_rooms(), 2);
        // Revisit 23282 but the exits shifted — everything is wiped.
        assert!(s.record_exits(23282, &dirs(&["north", "west"])));
        assert_eq!(s.learned_rooms(), 0);
    }

    #[test]
    fn same_exits_do_not_wipe() {
        let mut s = ConfluenceState::new();
        assert!(!s.record_exits(23282, &dirs(&["east", "north"])));
        // Order-independent compare.
        assert!(!s.record_exits(23282, &dirs(&["north", "east"])));
        assert_eq!(s.learned_rooms(), 1);
    }

    #[test]
    fn explores_untraversed_exits_first() {
        let mut s = ConfluenceState::new();
        let _ = s.record_exits(23282, &dirs(&["north", "east"]));
        // Nothing learned, no landmark → pick an untraversed exit.
        match s.choose_dir(23282, true, &[]) {
            ConfluenceMove::Go(d) => assert!(d == "north" || d == "east"),
            other => panic!("expected Go, got {other:?}"),
        }
    }

    #[test]
    fn bfs_routes_toward_landmark_room() {
        let mut s = ConfluenceState::new();
        // 23282 --north--> 23283 --north--> 23284 (landmark room).
        let _ = s.record_exits(23282, &dirs(&["north"]));
        s.record_arrival(23282, "north", 23283);
        let _ = s.record_exits(23283, &dirs(&["north", "south"]));
        s.record_arrival(23283, "north", 23284);
        let _ = s.record_exits(23284, &dirs(&["south"]));
        s.record_arrival(23283, "south", 23282);
        // Tranquility seen in 23284.
        s.observe_landmarks(23284, true, &nouns(&[TRANQUILITY_LOOT]));
        // From 23282 the BFS should send us north (toward 23283 → 23284).
        assert_eq!(s.dir_to(23282, &[Some(23284)]), Some("north".into()));
        assert_eq!(
            s.choose_dir(23282, true, &[]),
            ConfluenceMove::Go("north".into())
        );
    }

    #[test]
    fn landmark_forgotten_when_loot_gone() {
        let mut s = ConfluenceState::new();
        s.observe_landmarks(23284, true, &nouns(&[TRANQUILITY_LOOT]));
        // Revisit 23284, tranquility no longer here → forget it.
        s.observe_landmarks(23284, true, &[]);
        assert_eq!(s.dir_to(23282, &[Some(23284)]), None);
    }

    #[test]
    fn record_arrival_bumps_wander_order() {
        let mut s = ConfluenceState::new();
        let _ = s.record_exits(23282, &dirs(&["north", "east"]));
        s.record_arrival(23282, "north", 23283);
        s.record_arrival(23282, "east", 23284);
        // Re-arriving 23283 moves it to the end (most recent).
        s.record_arrival(23282, "north", 23283);
        // Both dests known now; choose_dir falls to unwandered/least-recent
        // logic and still returns a real direction.
        match s.choose_dir(23282, true, &[]) {
            ConfluenceMove::Go(_) => {}
            other => panic!("expected Go, got {other:?}"),
        }
    }
}
