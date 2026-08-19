//! Dijkstra over the wayto graph — a faithful port of Lich's
//! `map_base.rb` (`Room#dijkstra`, `path_to`, `find_nearest`,
//! `find_nearest_by_tag`, `estimate_time`), including its quirks:
//!
//! - Multi-target searches early-accept the first target popped at distance
//!   < 20; farther targets are found by exhausting the graph.
//! - `path_to` returns the rooms to traverse *excluding* the source and
//!   *including* the destination — and `None` when source == destination
//!   (go2 handles "already there" before pathing).
//! - `estimate_time` defaults missing timeto entries to 0.2s (the dijkstra
//!   itself does NOT — an edge without a cost is unroutable).
//! - Routing weights edges by `timeto` ALONE, exactly like Lich. A proc
//!   `wayto` with a numeric `timeto` IS routable; interpreting the proc is
//!   the walk executor's job at the edge, not the router's.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::core::mapdb::{MapDb, Room, TimeTo};

/// What the search is looking for.
#[derive(Debug, Clone, Copy)]
pub enum PathTarget<'a> {
    /// Stop when this room is reached.
    Room(u32),
    /// Stop early when any of these is reached within 20s of travel;
    /// otherwise compute distances to everything reachable.
    AnyOf(&'a [u32]),
}

/// Search result: predecessor tree + shortest distances, keyed by room id.
#[derive(Debug, Default)]
pub struct Dijkstra {
    pub previous: HashMap<u32, u32>,
    pub distance: HashMap<u32, f64>,
}

impl Dijkstra {
    /// Reconstruct the path from `source` to `destination` using this search's
    /// `previous` pointers (a cheap walk, no new search). Excludes `source`,
    /// includes `destination` — the same shape as [`path_to`]. `None` if the
    /// search never reached `destination`.
    pub fn reconstruct(&self, source: u32, destination: u32) -> Option<Vec<u32>> {
        if destination == source {
            return Some(Vec::new());
        }
        self.previous.get(&destination)?;
        let mut path = vec![destination];
        loop {
            let &prev = self.previous.get(path.last().expect("non-empty"))?;
            if prev == source {
                break;
            }
            path.push(prev);
        }
        path.reverse();
        Some(path)
    }
}

/// The cost of stepping from `room` to its wayto neighbor `dest`.
/// `None` = edge not routable (see module docs for the rules).
///
/// Routing depends ONLY on `timeto`, never on whether we can interpret the
/// `wayto` command — a faithful match for Lich's `Room#dijkstra`, which
/// weights edges by `timeto[adj]` and evaluates proc costs lazily, never
/// consulting `wayto`. Whether a proc edge can actually be *walked* is an
/// execution concern: the walk executor transpiles the command when it
/// reaches the edge, and only there does an un-interpretable proc fail
/// (falling back to Lich `;go2` if enabled, else banning the edge and
/// re-pathing). Scripted costs resolve through `transpile::resolve_timeto`
/// (delegation follows, settings gates default off, negative costs — which
/// would corrupt the search — are rejected).
fn edge_cost(db: &MapDb, room: &Room, dest: u32, _command: &str) -> Option<f64> {
    // Urchin-hideout hubs are excluded by default (they'd shortcut every
    // route), but when urchin travel is valid this session we let them route —
    // the per-edge timeto proc gates the actual cost (transpile::urchins_valid
    // and TIMETO_URCHIN).
    if !super::transpile::urchins_valid()
        && (room.is_urchin_hideout() || db.room(dest)?.is_urchin_hideout())
    {
        return None;
    }
    // Curated overrides make an edge walkable regardless of its script.
    if let Some(ov) = super::overrides::edge_override(room.id, dest) {
        return ov
            .cost
            .or_else(|| super::transpile::resolve_timeto(db, room, dest))
            .or(Some(0.2));
    }
    super::transpile::resolve_timeto(db, room, dest)
}

/// Non-NaN f64 ordering for the heap (timeto costs are plain numbers).
#[derive(PartialEq)]
struct Cost(f64);
impl Eq for Cost {}
impl PartialOrd for Cost {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Cost {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

pub fn dijkstra(db: &MapDb, source: u32, target: Option<PathTarget>) -> Dijkstra {
    dijkstra_filtered(db, source, target, &|_, _| true)
}

/// `dijkstra` with an edge admittance filter — the walk executor uses it to
/// exclude edges that failed repeatedly this session (go2's "changing
/// timeto to nil" restart behavior).
pub fn dijkstra_filtered(
    db: &MapDb,
    source: u32,
    target: Option<PathTarget>,
    admit: &dyn Fn(u32, u32) -> bool,
) -> Dijkstra {
    let mut result = Dijkstra::default();
    if db.room(source).is_none() {
        return result;
    }
    let mut visited: HashMap<u32, bool> = HashMap::new();
    let mut heap: BinaryHeap<Reverse<(Cost, u32)>> = BinaryHeap::new();
    heap.push(Reverse((Cost(0.0), source)));
    result.distance.insert(source, 0.0);

    let is_done = |room: u32, dist: f64| -> bool {
        match target {
            Some(PathTarget::Room(dest)) => room == dest,
            Some(PathTarget::AnyOf(list)) => list.contains(&room) && dist < 20.0,
            None => false,
        }
    };

    while let Some(Reverse((Cost(current_dist), room_id))) = heap.pop() {
        if visited.get(&room_id).copied().unwrap_or(false) {
            continue;
        }
        if is_done(room_id, current_dist) {
            break;
        }
        visited.insert(room_id, true);

        let Some(room) = db.room(room_id) else {
            continue;
        };
        for (&adjacent, command) in &room.wayto {
            if visited.get(&adjacent).copied().unwrap_or(false) {
                continue;
            }
            if !admit(room_id, adjacent) {
                continue;
            }
            let Some(weight) = edge_cost(db, room, adjacent, command) else {
                continue;
            };
            let new_distance = current_dist + weight;
            let better = result
                .distance
                .get(&adjacent)
                .map(|&d| d > new_distance)
                .unwrap_or(true);
            if better {
                result.distance.insert(adjacent, new_distance);
                result.previous.insert(adjacent, room_id);
                heap.push(Reverse((Cost(new_distance), adjacent)));
            }
        }
    }
    result
}

/// Rooms to traverse from `source` to `destination` — excluding the source,
/// including the destination. `None` when unreachable (or when already
/// there, matching Lich).
pub fn path_to(db: &MapDb, source: u32, destination: u32) -> Option<Vec<u32>> {
    path_to_filtered(db, source, destination, &|_, _| true)
}

/// `path_to` with an edge admittance filter (see `dijkstra_filtered`).
pub fn path_to_filtered(
    db: &MapDb,
    source: u32,
    destination: u32,
    admit: &dyn Fn(u32, u32) -> bool,
) -> Option<Vec<u32>> {
    let search = dijkstra_filtered(db, source, Some(PathTarget::Room(destination)), admit);
    search.previous.get(&destination)?;
    let mut path = vec![destination];
    loop {
        let &prev = search.previous.get(path.last().expect("non-empty"))?;
        if prev == source {
            break;
        }
        path.push(prev);
    }
    path.reverse();
    Some(path)
}

/// Nearest of `targets` by travel time; the source itself wins if listed.
pub fn find_nearest(db: &MapDb, source: u32, targets: &[u32]) -> Option<u32> {
    if targets.contains(&source) {
        return Some(source);
    }
    let search = dijkstra(db, source, Some(PathTarget::AnyOf(targets)));
    targets
        .iter()
        .filter_map(|&id| search.distance.get(&id).map(|&d| (id, d)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Nearest room tagged `tag` (`.go2 bank`).
pub fn find_nearest_by_tag(db: &MapDb, source: u32, tag: &str) -> Option<u32> {
    find_nearest(db, source, db.room_ids_with_tag(tag))
}

/// Total estimated seconds along `rooms` (consecutive pairs). Missing
/// timeto entries count 0.2s and proc costs count 0.2s — this is a display
/// estimate, not the routing cost.
pub fn estimate_time(db: &MapDb, rooms: &[u32]) -> f64 {
    rooms
        .windows(2)
        .map(|pair| {
            db.room(pair[0])
                .and_then(|room| room.timeto.get(&pair[1]))
                .map(|t| match t {
                    TimeTo::Seconds(s) => *s,
                    TimeTo::Proc(_) => 0.2,
                })
                .unwrap_or(0.2)
        })
        .sum()
}

/// Total silver cost along `rooms` (a full path INCLUDING the source), read
/// from `silver-cost:<next_room_id>:<value>` room tags — a port of go2's
/// `get_silver_cost`. Each room is scanned for a tag naming the next room in
/// the path; a numeric value adds directly. Non-numeric (StringProc) cost
/// values are the rare settings-gated dynamic ones we can't evaluate — they
/// contribute 0 (v1), same as an unhandled proc elsewhere.
pub fn silver_cost(db: &MapDb, rooms: &[u32]) -> u64 {
    let mut total = 0u64;
    for pair in rooms.windows(2) {
        let (Some(room), next) = (db.room(pair[0]), pair[1]) else {
            continue;
        };
        let prefix = format!("silver-cost:{next}:");
        for tag in &room.tags {
            if let Some(value) = tag.strip_prefix(&prefix) {
                // go2 evals non-numeric values as StringProcs
                // (go2.lic:958-964). The live mapdb has ZERO such tags
                // (checked 2026-08-12), so full parity is: plain integers,
                // Ruby underscore literals ("2_500"), and a loud warning for
                // anything else so a future dynamic tag is never a silent 0.
                let cleaned: String = value.chars().filter(|c| *c != '_').collect();
                if let Ok(n) = cleaned.trim().parse::<u64>() {
                    total += n;
                } else {
                    tracing::warn!(
                        "unevaluatable silver-cost tag on room {}: {tag:?} (counted as 0)",
                        pair[0]
                    );
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    /// wayto/timeto both present unless a cost is None.
    fn graph(edges: &[(u32, u32, &str, Option<f64>)], extra: &str) -> MapDb {
        use std::collections::BTreeMap;
        let mut rooms: BTreeMap<u32, (BTreeMap<u32, String>, BTreeMap<u32, f64>)> = BTreeMap::new();
        for &(from, to, cmd, cost) in edges {
            let entry = rooms.entry(from).or_default();
            entry.0.insert(to, cmd.to_string());
            if let Some(cost) = cost {
                entry.1.insert(to, cost);
            }
            rooms.entry(to).or_default();
        }
        let mut json_rooms: Vec<String> = rooms
            .into_iter()
            .map(|(id, (wayto, timeto))| {
                let wayto: Vec<String> = wayto
                    .iter()
                    .map(|(k, v)| format!(r#""{k}": "{v}""#))
                    .collect();
                let timeto: Vec<String> = timeto
                    .iter()
                    .map(|(k, v)| format!(r#""{k}": {v}"#))
                    .collect();
                format!(
                    r#"{{"id": {id}, "uid": [{}], "location": "Test", "title": ["[Room {id}]"],
                        "wayto": {{{}}}, "timeto": {{{}}}, "paths": ""}}"#,
                    9_000_000 + id as i64,
                    wayto.join(","),
                    timeto.join(",")
                )
            })
            .collect();
        if !extra.is_empty() {
            json_rooms.push(extra.to_string());
        }
        MapDb::from_json(&format!("[{}]", json_rooms.join(","))).unwrap()
    }

    #[test]
    fn shortest_path_prefers_cheap_detours_over_short_expensive_hops() {
        // 1 → 2 costs 10 directly, but 1 → 3 → 4 → 2 costs 0.6.
        let db = graph(
            &[
                (1, 2, "east", Some(10.0)),
                (1, 3, "north", Some(0.2)),
                (3, 4, "east", Some(0.2)),
                (4, 2, "south", Some(0.2)),
            ],
            "",
        );
        assert_eq!(path_to(&db, 1, 2), Some(vec![3, 4, 2]));
        let d = dijkstra(&db, 1, Some(PathTarget::Room(2)));
        assert!((d.distance[&2] - 0.6).abs() < 1e-9);
        // Path excludes source, includes destination; self-path is None
        // (Lich parity — go2 checks "already there" first).
        assert_eq!(path_to(&db, 1, 1), None);
        assert_eq!(path_to(&db, 2, 99999), None);
    }

    #[test]
    fn routing_depends_on_timeto_not_wayto_interpretability() {
        // Lich parity: an edge routes when its `timeto` resolves to a number,
        // regardless of whether we can yet transpile the `wayto` command.
        // Interpreting the proc is the executor's job at the edge.
        let db = graph(
            &[
                (1, 2, "east", None), // wayto but no timeto: Lich skips it
                // proc wayto with a NUMERIC timeto: routable (executor
                // transpiles/handles it, or falls back, at the edge).
                (1, 3, ";e fput 'go boat'; move 'go boat'", Some(0.2)),
            ],
            r#"{"id": 4, "uid": [9000004], "location": "Test", "title": ["[Room 4]"],
                "wayto": {"1": "west"}, "timeto": {"1": ";e Settings[:foo] ? 0.2 : nil"},
                "paths": ""}"#,
        );
        assert_eq!(path_to(&db, 1, 2), None, "missing timeto is unroutable");
        assert_eq!(
            path_to(&db, 1, 3),
            Some(vec![3]),
            "proc wayto with numeric timeto IS routable (Lich weights on timeto only)"
        );
        assert_eq!(
            path_to(&db, 4, 1),
            None,
            "proc timeto that resolves to nil (settings gate off) is unroutable"
        );
    }

    #[test]
    fn urchin_hideouts_never_shortcut_a_route() {
        // Share the pathing gate lock: urchin exclusion depends on the global
        // urchins_valid() flag, which the transpile tests also toggle.
        let _g = super::super::transpile::GATE_LOCK.lock().unwrap();
        super::super::transpile::set_urchins_valid(false);
        // Long honest road 1→2→3 (0.4) vs a hideout teleport 1→99→3 (0.2).
        let db = graph(
            &[
                (1, 2, "east", Some(0.2)),
                (2, 3, "east", Some(0.2)),
                (1, 99, "urchin guide", Some(0.1)),
            ],
            r#"{"id": 99, "uid": [9000099], "location": "Test",
                "title": ["[Test - Urchin Hideout]"],
                "wayto": {"3": "bwahaha"}, "timeto": {"3": 0.1},
                "paths": "Obvious exits: bwahaha"}"#,
        );
        assert_eq!(path_to(&db, 1, 3), Some(vec![2, 3]));
    }

    #[test]
    fn nearest_target_wins_and_source_beats_everything() {
        let db = graph(
            &[
                (1, 2, "east", Some(0.2)),
                (2, 3, "east", Some(0.2)),
                (1, 4, "west", Some(5.0)),
            ],
            "",
        );
        assert_eq!(find_nearest(&db, 1, &[3, 4]), Some(3));
        assert_eq!(
            find_nearest(&db, 1, &[4, 1]),
            Some(1),
            "already standing on one"
        );
        assert_eq!(find_nearest(&db, 1, &[]), None);
    }

    #[test]
    fn tags_resolve_to_the_nearest_tagged_room() {
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "Test", "title": ["[Street]"],
                 "wayto": {"2": "east", "3": "west"}, "timeto": {"2": 0.2, "3": 5.0},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "Test", "title": ["[Near Bank]"],
                 "tags": ["bank"], "wayto": {"1": "west"}, "timeto": {"1": 0.2}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "Test", "title": ["[Far Bank]"],
                 "tags": ["bank"], "wayto": {"1": "east"}, "timeto": {"1": 5.0}, "paths": ""}
            ]"#,
        )
        .unwrap();
        assert_eq!(find_nearest_by_tag(&db, 1, "bank"), Some(2));
        assert_eq!(find_nearest_by_tag(&db, 1, "gemshop"), None);
    }

    #[test]
    fn estimate_time_uses_the_lich_defaults() {
        let db = graph(
            &[
                (1, 2, "east", Some(0.5)),
                (2, 3, "east", None), // missing timeto → 0.2 in the estimate
            ],
            "",
        );
        let time = estimate_time(&db, &[1, 2, 3]);
        assert!((time - 0.7).abs() < 1e-9);
        assert_eq!(estimate_time(&db, &[1]), 0.0);
    }

    #[test]
    fn silver_cost_sums_the_path_tags() {
        // Room 1 → 2 costs 25000 (portmaster), 2 → 3 is free.
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Dock]"],
                 "tags": ["silver-cost:2:25000", "silver-cost:9:12500"],
                 "wayto": {"2": "board"}, "timeto": {"2": 1.0}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Port]"],
                 "wayto": {"3": "north"}, "timeto": {"3": 0.2}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[Town]"],
                 "wayto": {"2": "south"}, "timeto": {"2": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        // The full path includes the source room (Lich's get_silver_cost shape).
        assert_eq!(silver_cost(&db, &[1, 2, 3]), 25000);
        // Only the 1→2 tag counts; the 1→9 tag is a different destination.
        assert_eq!(silver_cost(&db, &[2, 3]), 0);
        assert_eq!(silver_cost(&db, &[1]), 0);
    }

    #[test]
    fn reconstruct_matches_path_to_from_one_search() {
        // A single AnyOf search's `reconstruct` must yield the same path as a
        // dedicated path_to — the basis of the one-Dijkstra bank search that
        // replaced 72 separate path_to calls (the UI-freeze fix).
        let db = graph(
            &[
                (1, 2, "n", Some(0.2)),
                (2, 3, "n", Some(0.2)),
                (3, 4, "n", Some(0.2)),
                (1, 5, "e", Some(5.0)),
                (5, 4, "n", Some(5.0)),
            ],
            "",
        );
        let targets = [4u32];
        let search = dijkstra(&db, 1, Some(PathTarget::AnyOf(&targets)));
        assert_eq!(
            search.reconstruct(1, 4),
            path_to(&db, 1, 4),
            "reconstruct equals path_to"
        );
        // Source-to-source is the empty path.
        assert_eq!(search.reconstruct(1, 1), Some(Vec::new()));
        // An unreached room reconstructs to None.
        let none_search = dijkstra(&db, 1, Some(PathTarget::Room(2)));
        assert_eq!(none_search.reconstruct(1, 999), None);
    }
}
