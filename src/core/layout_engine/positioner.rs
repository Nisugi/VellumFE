//! Room positioning — port of `room-positioner.js`.
//!
//! Builds connected components over directional edges with BFS, resolving
//! collisions by grid rips, then hill-climbs each component to shorten
//! stretched edges and compacts empty rows/columns (spec §4–§5).

use std::collections::{HashMap, HashSet, VecDeque};

use super::direction::{Dir, DirectionMap};
use super::mapdb::RoomTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub from: u32,
    pub to: u32,
    pub direction: Dir,
    pub actual: Cell,
}

/// How a group was placed on the shared sheet by the cluster packer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackMethod {
    Image,
    Connector,
    Seed,
    Strip,
    InteriorShelf,
}

impl PackMethod {
    pub fn name(self) -> &'static str {
        match self {
            PackMethod::Image => "image",
            PackMethod::Connector => "connector",
            PackMethod::Seed => "seed",
            PackMethod::Strip => "strip",
            PackMethod::InteriorShelf => "interior-shelf",
        }
    }
}

/// One connected component: rooms in placement order, their internal grid
/// positions, and (after packing) the offset onto the shared sheet.
#[derive(Debug, Clone)]
pub struct Group {
    pub index: usize,
    /// Room ids in BFS placement order (the reference's `componentRooms`).
    pub room_ids: Vec<u32>,
    /// Internal coordinates, before `base_offset` is applied.
    pub positions: HashMap<u32, Cell>,
    /// Compass edges whose placed geometry contradicts their stated
    /// direction — genuine data conflicts, kept visible, never fixed silently.
    pub violations: Vec<Violation>,
    pub base_offset: Option<Cell>,
    pub packing: Option<PackMethod>,
    /// Building name for interior groups (majority `[Prefix, …]` title).
    pub name: Option<String>,
}

impl Group {
    /// Internal position plus the group's sheet offset. Only valid after the
    /// packer has placed the group.
    pub fn final_cell(&self, room_id: u32) -> Cell {
        let internal = self.positions[&room_id];
        let off = self
            .base_offset
            .expect("final_cell requires a packed group");
        Cell {
            x: internal.x + off.x,
            y: internal.y + off.y,
        }
    }

    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds {
            min_x: i32::MAX,
            max_x: i32::MIN,
            min_y: i32::MAX,
            max_y: i32::MIN,
        };
        for p in self.positions.values() {
            b.min_x = b.min_x.min(p.x);
            b.max_x = b.max_x.max(p.x);
            b.min_y = b.min_y.min(p.y);
            b.max_y = b.max_y.max(p.y);
        }
        b
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
}

impl Bounds {
    pub fn width(&self) -> i32 {
        self.max_x - self.min_x + 1
    }
    pub fn height(&self) -> i32 {
        self.max_y - self.min_y + 1
    }
}

/// Port of `calculateRoomPositionsWithGroups`: repeat BFS component builds
/// until every room is placed.
pub fn position_rooms(lookup: &RoomTable, dirs: &DirectionMap) -> Vec<Group> {
    let rooms = lookup.rooms();
    let mut groups: Vec<Group> = Vec::new();
    let mut unpositioned: HashSet<u32> = rooms.iter().map(|r| r.id).collect();

    // Directional-edge counts are a pure function of the selection, so they
    // are computed once instead of per component start.
    let connection_counts: Vec<usize> = rooms
        .iter()
        .map(|room| {
            room.wayto
                .keys()
                .filter(|&&t| dirs.get(room.id, t).is_some())
                .count()
        })
        .collect();

    while !unpositioned.is_empty() {
        // Start room: the unplaced room with the most directional edges into
        // the selection. Strict `>` keeps the first encountered on ties;
        // iterating `rooms` (ascending id) matches the reference's insertion
        // order.
        let mut next_start: Option<u32> = None;
        let mut best_connections = 0usize;
        for (room, &valid) in rooms.iter().zip(&connection_counts) {
            if !unpositioned.contains(&room.id) {
                continue;
            }
            if valid > best_connections {
                best_connections = valid;
                next_start = Some(room.id);
            }
        }
        let start_id = next_start.unwrap_or_else(|| {
            // No connected rooms left; take the first remaining in room order.
            rooms
                .iter()
                .map(|r| r.id)
                .find(|id| unpositioned.contains(id))
                .expect("unpositioned is non-empty")
        });

        let mut positions: HashMap<u32, Cell> = HashMap::new();
        let mut occupied: HashSet<Cell> = HashSet::new();
        let mut room_order: Vec<u32> = Vec::new();

        let mut queue: VecDeque<u32> = VecDeque::new();
        queue.push_back(start_id);
        positions.insert(start_id, Cell { x: 0, y: 0 });
        occupied.insert(Cell { x: 0, y: 0 });
        room_order.push(start_id);
        unpositioned.remove(&start_id);

        // BFS. The queue holds ids only: grid rips move already-placed rooms,
        // so the parent position is re-read at processing time.
        while let Some(room_id) = queue.pop_front() {
            let Some(room) = lookup.get(room_id) else {
                continue;
            };
            for (&target_id, _) in &room.wayto {
                if !lookup.contains(target_id) || !unpositioned.contains(&target_id) {
                    continue;
                }
                let Some(direction) = dirs.get(room_id, target_id) else {
                    continue;
                };
                let (dx, dy) = direction.offset();

                let pos = positions[&room_id];
                let mut target = Cell {
                    x: pos.x + dx,
                    y: pos.y + dy,
                };

                if occupied.contains(&target) {
                    // Grid rip: shift a half-plane one cell so the occupant
                    // slides off the target cell and the stated direction
                    // stays true. The parent is never inside the half-plane.
                    rip_grid(&mut positions, pos, (dx, dy));
                    occupied = positions.values().copied().collect();
                    let fresh = positions[&room_id];
                    target = Cell {
                        x: fresh.x + dx,
                        y: fresh.y + dy,
                    };
                }

                if !occupied.contains(&target) {
                    positions.insert(target_id, target);
                    occupied.insert(target);
                    room_order.push(target_id);
                    unpositioned.remove(&target_id);
                    queue.push_back(target_id);
                }
            }
        }

        optimize_component(&room_order, &mut positions, lookup, dirs);
        let violations = validate_component(&room_order, &positions, lookup, dirs);

        groups.push(Group {
            index: groups.len(),
            room_ids: room_order,
            positions,
            violations,
            base_offset: None,
            packing: None,
            name: None,
        });
    }

    groups
}

fn rip_grid(positions: &mut HashMap<u32, Cell>, parent: Cell, (dx, dy): (i32, i32)) {
    let target_x = parent.x + dx;
    let target_y = parent.y + dy;
    if dx > 0 {
        for p in positions.values_mut() {
            if p.x >= target_x {
                p.x += 1;
            }
        }
    } else if dx < 0 {
        for p in positions.values_mut() {
            if p.x <= target_x {
                p.x -= 1;
            }
        }
    } else if dy > 0 {
        for p in positions.values_mut() {
            if p.y >= target_y {
                p.y += 1;
            }
        }
    } else if dy < 0 {
        for p in positions.values_mut() {
            if p.y <= target_y {
                p.y -= 1;
            }
        }
    }
}

/// Hill climb (≤12 passes): each room tries cells adjacent to its directional
/// neighbors; a move is accepted only when every compass edge keeps correct
/// signs AND total Chebyshev edge length strictly decreases, so the loop
/// terminates.
fn optimize_component(
    room_order: &[u32],
    positions: &mut HashMap<u32, Cell>,
    lookup: &RoomTable,
    dirs: &DirectionMap,
) {
    if room_order.len() < 3 {
        compact_component(positions);
        return;
    }

    #[derive(Clone, Copy)]
    struct EdgeSign {
        other: u32,
        sx: i32,
        sy: i32,
    }

    // room id -> expected sign of (other - this) per compass edge.
    let mut adjacency: HashMap<u32, Vec<EdgeSign>> = HashMap::new();
    for &room_id in room_order {
        let Some(room) = lookup.get(room_id) else {
            continue;
        };
        for (&target_id, _) in &room.wayto {
            if !positions.contains_key(&target_id) {
                continue;
            }
            let Some(direction) = dirs.get(room_id, target_id) else {
                continue;
            };
            if !direction.is_compass() {
                continue;
            }
            let (dx, dy) = direction.offset();
            adjacency.entry(room_id).or_default().push(EdgeSign {
                other: target_id,
                sx: dx.signum(),
                sy: dy.signum(),
            });
            adjacency.entry(target_id).or_default().push(EdgeSign {
                other: room_id,
                sx: -dx.signum(),
                sy: -dy.signum(),
            });
        }
    }

    let mut occupied: HashSet<Cell> = positions.values().copied().collect();

    let mut improved = true;
    let mut passes = 0;
    while improved && passes < 12 {
        improved = false;
        passes += 1;

        for &room_id in room_order {
            let Some(edges) = adjacency.get(&room_id) else {
                continue;
            };
            if edges.is_empty() {
                continue;
            }
            let current = positions[&room_id];

            let mut current_cost = 0i64;
            for e in edges {
                let other = positions[&e.other];
                current_cost +=
                    (other.x - current.x).abs().max((other.y - current.y).abs()) as i64;
            }

            // Candidates: the ideal cell beside each neighbor ± 1 ring, plus
            // the current spot ± 1 ring (so rooms drift stepwise toward
            // distant neighbors across passes). Insertion order preserved,
            // duplicates keep their first slot, like a JS Set.
            let mut candidates: Vec<Cell> = Vec::new();
            let mut seen: HashSet<Cell> = HashSet::new();
            let mut add = |c: Cell, candidates: &mut Vec<Cell>, seen: &mut HashSet<Cell>| {
                if seen.insert(c) {
                    candidates.push(c);
                }
            };
            for e in edges {
                let other = positions[&e.other];
                let ix = other.x - e.sx;
                let iy = other.y - e.sy;
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        add(
                            Cell {
                                x: ix + dx,
                                y: iy + dy,
                            },
                            &mut candidates,
                            &mut seen,
                        );
                    }
                }
            }
            for dx in -1..=1 {
                for dy in -1..=1 {
                    add(
                        Cell {
                            x: current.x + dx,
                            y: current.y + dy,
                        },
                        &mut candidates,
                        &mut seen,
                    );
                }
            }

            let mut best: Option<Cell> = None;
            let mut best_cost = current_cost;
            'candidate: for &cand in &candidates {
                if occupied.contains(&cand) {
                    continue;
                }
                let mut cost = 0i64;
                for e in edges {
                    let other = positions[&e.other];
                    let dx = other.x - cand.x;
                    let dy = other.y - cand.y;
                    if dx.signum() != e.sx || dy.signum() != e.sy {
                        continue 'candidate;
                    }
                    cost += dx.abs().max(dy.abs()) as i64;
                }
                if cost < best_cost {
                    best_cost = cost;
                    best = Some(cand);
                }
            }

            if let Some(best) = best {
                occupied.remove(&current);
                occupied.insert(best);
                positions.insert(room_id, best);
                improved = true;
            }
        }
    }

    compact_component(positions);
}

/// Collapse fully-empty rows and columns by rank-mapping the distinct x and y
/// values to 0..n−1. Relative order is preserved, so every edge keeps its
/// direction signs.
fn compact_component(positions: &mut HashMap<u32, Cell>) {
    if positions.is_empty() {
        return;
    }
    let mut xs: Vec<i32> = positions.values().map(|p| p.x).collect();
    let mut ys: Vec<i32> = positions.values().map(|p| p.y).collect();
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    let x_rank: HashMap<i32, i32> = xs.iter().enumerate().map(|(i, &x)| (x, i as i32)).collect();
    let y_rank: HashMap<i32, i32> = ys.iter().enumerate().map(|(i, &y)| (y, i as i32)).collect();
    for p in positions.values_mut() {
        p.x = x_rank[&p.x];
        p.y = y_rank[&p.y];
    }
}

/// Check every placed compass edge against its stated direction. Stretched
/// edges pass (signs match); wrong-way edges are reported.
fn validate_component(
    room_order: &[u32],
    positions: &HashMap<u32, Cell>,
    lookup: &RoomTable,
    dirs: &DirectionMap,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    for &room_id in room_order {
        let Some(room) = lookup.get(room_id) else {
            continue;
        };
        let Some(&pos) = positions.get(&room_id) else {
            continue;
        };
        for (&target_id, _) in &room.wayto {
            let Some(&target_pos) = positions.get(&target_id) else {
                continue;
            };
            let Some(direction) = dirs.get(room_id, target_id) else {
                continue;
            };
            if !direction.is_compass() {
                continue;
            }
            let (ex, ey) = direction.offset();
            let actual_x = target_pos.x - pos.x;
            let actual_y = target_pos.y - pos.y;
            if actual_x.signum() != ex.signum() || actual_y.signum() != ey.signum() {
                violations.push(Violation {
                    from: room_id,
                    to: target_id,
                    direction,
                    actual: Cell {
                        x: actual_x,
                        y: actual_y,
                    },
                });
            }
        }
    }
    violations
}
