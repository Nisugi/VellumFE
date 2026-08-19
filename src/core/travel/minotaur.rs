//! The minotaur maze walker — a learned-graph explorer for a fixed room set.
//!
//! 497 mapdb edges carry the same StringProc: a maze whose exits don't map to
//! fixed destinations, so the mapdb can't store a usable graph for it. The
//! proc *learns* adjacency by walking and re-derives a route on the fly.
//!
//! Structurally this is the Confluence explorer with a different goal. The
//! Confluence hunts a LANDMARK (a tranquility point that moves around); the
//! minotaur maze hunts a specific ROOM ID, and the maze's room set is carried
//! per-edge in the proc rather than being a fixed zone. Everything else — the
//! learned `room -> dir -> dest` map, the BFS toward a goal, explore-unknown
//! before wander — is the same shape, so this module mirrors
//! [`super::confluence`] deliberately rather than inventing new structure.
//!
//! ## The algorithm (mirrors the Ruby)
//!
//! Standing in `start_room`, with `target_room_id` the goal:
//! 1. Pick a direction, in strict preference order:
//!    a. an exit whose learned destination IS the target;
//!    b. an exit we've never traversed (explore);
//!    c. an exit leading to a room that reaches the target (BFS);
//!    d. a random exit (the Ruby's `XMLData.room_exits[rand(...)]`).
//! 2. Move, then record `learned[start][dir] = arrived`.
//! 3. Arrived at the target → done.
//! 4. Arrived OUTSIDE the maze room set → we fell out; walk back via the
//!    arrival room's own wayto edge to where we came from, and resume.
//!
//! The `child` NPC blocks in the Ruby (`50.times { break if ... }`) wait for a
//! escorted bounty child to follow. We don't model the bounty child: the
//! executor's paced `StepMove` already waits for the room to settle between
//! commands, which is the same practical effect for walking purposes.

use std::collections::HashMap;

/// What the walker wants to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MinotaurMove {
    /// Reached the goal room; the maze leg is done.
    Arrive,
    /// Walk this compass direction.
    Go(String),
    /// We're outside the maze and don't know a way back — let the caller
    /// re-path rather than wandering blindly.
    Lost,
}

/// Learned adjacency for one maze traversal.
///
/// Lich keeps this in a process global (`$minotaur_maze_dirs`) that persists
/// across crossings; we hold it on the travel task. That is a deliberate
/// difference: a maze that re-scrambles between trips would make stale
/// learning actively harmful, and a fresh graph costs only the first few
/// exploratory moves.
#[derive(Debug, Clone, Default)]
pub struct MinotaurState {
    /// `learned[room][dir] = Some(dest)` once we've walked `dir` from `room`;
    /// `None` while the exit is known but untraversed.
    learned: HashMap<u32, HashMap<String, Option<u32>>>,
    /// Rooms in arrival order — the tiebreak for "least recently wandered".
    wander: Vec<u32>,
    /// The room we were in before the current move, so a fall-out can walk back.
    came_from: Option<u32>,
}

impl MinotaurState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot a room's exits on first visit. Returns true when the room's
    /// exits CHANGED since we recorded them — the maze shifted under us and
    /// the learned graph is now lies.
    pub fn record_exits(&mut self, room: u32, exits: &[String]) -> bool {
        match self.learned.get(&room) {
            None => {
                self.learned
                    .insert(room, exits.iter().map(|d| (d.clone(), None)).collect());
                false
            }
            Some(existing) => {
                let mut known: Vec<&String> = existing.keys().collect();
                known.sort();
                let mut live: Vec<&String> = exits.iter().collect();
                live.sort();
                known != live
            }
        }
    }

    /// Forget everything (the maze shifted).
    pub fn reset(&mut self) {
        self.learned.clear();
        self.wander.clear();
        self.came_from = None;
    }

    /// BFS over the learned graph: the first exit in `room` whose known
    /// destination is in `targets`, expanding through rooms that reach a
    /// target, up to 30 hops. Mirrors the Ruby's `dir_to` proc.
    pub fn dir_to(&self, room: u32, targets: &[u32]) -> Option<String> {
        let here = self.learned.get(&room)?;
        let mut targets: Vec<u32> = targets.to_vec();
        let mut tried: Vec<u32> = Vec::new();
        for _ in 0..30 {
            if let Some(dir) = here
                .iter()
                .find(|(_, dest)| dest.is_some_and(|d| targets.contains(&d)))
                .map(|(d, _)| d.clone())
            {
                return Some(dir);
            }
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
                    exits.values().any(|d| d.is_some_and(|x| old.contains(&x)))
                        && !tried.contains(k)
                })
                .map(|(k, _)| *k)
                .collect();
            if targets.is_empty() {
                break;
            }
        }
        None
    }

    /// Decide the next move from `room` toward `target`, given the room's live
    /// compass exits. Preference order mirrors the Ruby's `||` chain.
    pub fn choose_dir(&self, room: u32, target: u32, exits: &[String]) -> MinotaurMove {
        if room == target {
            return MinotaurMove::Arrive;
        }
        let here = self.learned.get(&room);
        // (a) An exit we've already learned goes straight to the target.
        if let Some(dir) = here.and_then(|h| {
            h.iter()
                .find(|(_, dest)| **dest == Some(target))
                .map(|(d, _)| d.clone())
        }) {
            return MinotaurMove::Go(dir);
        }
        // (b) An exit we've never traversed — explore. Ordered by the live
        // exit list, not the map's iteration order, so the walk is stable.
        if let Some(dir) = exits.iter().find(|d| {
            here.and_then(|h| h.get(*d))
                .map(Option::is_none)
                .unwrap_or(true)
        }) {
            return MinotaurMove::Go(dir.clone());
        }
        // (c) BFS toward a room that reaches the target.
        if let Some(dir) = self.dir_to(room, &[target]) {
            return MinotaurMove::Go(dir);
        }
        // (d) Everything is known and nothing reaches the target yet: take the
        // least-recently-visited neighbour. The Ruby picks at random; going
        // least-recently-wandered covers the maze faster and, unlike random,
        // is deterministic enough to test.
        let mut best: Option<(usize, &String)> = None;
        for dir in exits {
            let dest = here.and_then(|h| h.get(dir)).and_then(|d| *d);
            let age = dest
                .and_then(|d| self.wander.iter().position(|&w| w == d))
                .unwrap_or(0);
            if best.is_none_or(|(a, _)| age < a) {
                best = Some((age, dir));
            }
        }
        match best {
            Some((_, dir)) => MinotaurMove::Go(dir.clone()),
            None => MinotaurMove::Lost,
        }
    }

    /// Record where a move landed.
    pub fn record_arrival(&mut self, from: u32, dir: &str, arrived: u32) {
        self.learned
            .entry(from)
            .or_default()
            .insert(dir.to_string(), Some(arrived));
        self.came_from = Some(from);
        self.wander.retain(|&id| id != arrived);
        self.wander.push(arrived);
    }

    /// The room we walked in from, for backtracking out of a fall-out.
    pub fn came_from(&self) -> Option<u32> {
        self.came_from
    }

    /// Rooms we've learned exits for (status/tests).
    pub fn known_rooms(&self) -> usize {
        self.learned.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_learned_exit_to_the_target_wins_over_exploring() {
        let mut s = MinotaurState::new();
        let exits = vec!["north".to_string(), "east".to_string()];
        s.record_exits(1, &exits);
        s.record_arrival(1, "east", 99);
        // "north" is unexplored, but "east" is known to reach the goal.
        assert_eq!(
            s.choose_dir(1, 99, &exits),
            MinotaurMove::Go("east".into()),
            "a known route to the target beats exploration"
        );
    }

    #[test]
    fn unexplored_exits_are_taken_before_revisiting_known_ones() {
        let mut s = MinotaurState::new();
        let exits = vec!["north".to_string(), "east".to_string()];
        s.record_exits(1, &exits);
        s.record_arrival(1, "north", 50); // north known, east not
        assert_eq!(
            s.choose_dir(1, 99, &exits),
            MinotaurMove::Go("east".into()),
            "explore the untraversed exit rather than re-walking a known one"
        );
    }

    #[test]
    fn bfs_finds_a_two_hop_route_to_the_target() {
        let mut s = MinotaurState::new();
        let exits = vec!["north".to_string()];
        s.record_exits(1, &exits);
        s.record_arrival(1, "north", 2);
        s.record_exits(2, &["east".to_string()]);
        s.record_arrival(2, "east", 99);
        // Every exit of room 1 is explored, so (b) can't fire; BFS must see
        // that north -> 2 -> 99 reaches the goal.
        assert_eq!(
            s.choose_dir(1, 99, &exits),
            MinotaurMove::Go("north".into()),
            "routes through a room that reaches the target"
        );
    }

    #[test]
    fn arriving_at_the_target_is_done() {
        let s = MinotaurState::new();
        assert_eq!(
            s.choose_dir(99, 99, &["north".into()]),
            MinotaurMove::Arrive
        );
    }

    #[test]
    fn a_shifted_maze_is_detected_and_wiped() {
        let mut s = MinotaurState::new();
        assert!(
            !s.record_exits(1, &["north".into(), "east".into()]),
            "first visit is never a shift"
        );
        assert!(
            !s.record_exits(1, &["east".into(), "north".into()]),
            "same exits in a different order is not a shift"
        );
        assert!(
            s.record_exits(1, &["north".into(), "west".into()]),
            "different exits means the maze moved under us"
        );
        s.record_arrival(1, "north", 2);
        s.reset();
        assert_eq!(s.known_rooms(), 0, "a shift wipes the learned graph");
    }

    #[test]
    fn no_exits_at_all_reports_lost_rather_than_guessing() {
        let s = MinotaurState::new();
        assert_eq!(s.choose_dir(1, 99, &[]), MinotaurMove::Lost);
    }
}
