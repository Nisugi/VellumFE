//! Session-only "ghost rooms": live sketches of unmapped interiors.
//!
//! Mapdb maintainers deliberately leave shop interiors unmapped (they churn),
//! so walking into one gives the map service nothing to resolve — the mini
//! map holds the street outside. Ghost rooms fill that hole from stream data
//! alone: the room's game uid, its title and obvious exits, the mapped room
//! it was entered from (the anchor), and the command that crossed over.
//! Traversed uid→uid edges inside build a small cluster rendered hanging off
//! the anchor in a distinct dashed/dimmed style, so mapped truth never looks
//! like inference.
//!
//! Deliberately session-only in v1 — no save, no export. A live sketch is
//! immune to staleness by construction; persistence can come later without
//! rework.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::core::layout_engine::positioner::Cell;
use crate::core::layout_engine::{MapScene, Sheet};

/// What the stream knows about the room we're standing in, captured by
/// AppCore at resolution time.
#[derive(Debug, Clone, Default)]
pub struct RoomSnapshot {
    pub title: Option<String>,
    pub exits: Vec<String>,
}

/// Where the character came from when a ghost was entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A resolved mapdb room (id) — this ghost hangs off it.
    Mapped(u32),
    /// Another ghost (uid) — extends the cluster.
    Ghost(i64),
    /// Nothing usable (e.g. login straight into an unmapped room).
    Unknown,
}

#[derive(Debug, Clone)]
pub struct GhostRoom {
    pub uid: i64,
    pub title: Option<String>,
    pub exits: Vec<String>,
    /// Set when this ghost was first entered from a mapped room; the cluster
    /// renders hanging off that room.
    pub anchor: Option<GhostAnchor>,
}

#[derive(Debug, Clone)]
pub struct GhostAnchor {
    pub room_id: u32,
    /// The command that first crossed over ("go shop"), when known.
    pub command: Option<String>,
}

/// A traversed uid→uid edge inside a ghost cluster (undirected, deduped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhostEdge {
    pub a: i64,
    pub b: i64,
    pub label: Option<String>,
}

#[derive(Debug, Default)]
pub struct GhostStore {
    rooms: HashMap<i64, GhostRoom>,
    edges: Vec<GhostEdge>,
}

impl GhostStore {
    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    pub fn get(&self, uid: i64) -> Option<&GhostRoom> {
        self.rooms.get(&uid)
    }

    /// Record arriving in unmapped room `uid`. Upserts the room (fresher
    /// title/exits win), anchors it on first entry from a mapped room, and
    /// records the traversed edge when coming from another ghost.
    pub fn visit(&mut self, uid: i64, snapshot: RoomSnapshot, from: Origin, command: Option<String>) {
        let room = self.rooms.entry(uid).or_insert(GhostRoom {
            uid,
            title: None,
            exits: Vec::new(),
            anchor: None,
        });
        if snapshot.title.is_some() {
            room.title = snapshot.title;
        }
        if !snapshot.exits.is_empty() {
            room.exits = snapshot.exits;
        }
        match from {
            Origin::Mapped(room_id) => {
                if room.anchor.is_none() {
                    room.anchor = Some(GhostAnchor { room_id, command });
                }
            }
            Origin::Ghost(prev) if prev != uid => {
                let (a, b) = (prev.min(uid), prev.max(uid));
                if !self.edges.iter().any(|e| (e.a, e.b) == (a, b)) {
                    self.edges.push(GhostEdge {
                        a,
                        b,
                        label: command,
                    });
                }
            }
            _ => {}
        }
    }

    fn adjacency(&self) -> HashMap<i64, Vec<(i64, Option<&str>)>> {
        let mut adj: HashMap<i64, Vec<(i64, Option<&str>)>> = HashMap::new();
        for edge in &self.edges {
            adj.entry(edge.a).or_default().push((edge.b, edge.label.as_deref()));
            adj.entry(edge.b).or_default().push((edge.a, edge.label.as_deref()));
        }
        adj
    }
}

// --- Overlay: cells for the sheet being drawn -------------------------------

#[derive(Debug, Clone)]
pub struct GhostNode {
    pub uid: i64,
    pub cell: Cell,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GhostOverlayEdge {
    pub a: Cell,
    pub b: Cell,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GhostOverlay {
    pub nodes: Vec<GhostNode>,
    pub edges: Vec<GhostOverlayEdge>,
}

impl GhostOverlay {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn cell_of(&self, uid: i64) -> Option<Cell> {
        self.nodes.iter().find(|n| n.uid == uid).map(|n| n.cell)
    }
}

/// Lay the session's ghost clusters onto one sheet of a scene: every ghost
/// anchored to a visible room gets a free cell near its anchor, its cluster
/// spreads by BFS over traversed edges, and each placement is deterministic
/// (uids sorted, fixed ring-search order) so the sketch doesn't jitter
/// between frames.
pub fn build_overlay(
    store: &GhostStore,
    scene: &MapScene,
    sheet: Sheet,
    group_filter: Option<&HashSet<usize>>,
) -> GhostOverlay {
    let mut overlay = GhostOverlay::default();
    if store.is_empty() {
        return overlay;
    }

    // Ghosts must not sit on real rooms (any group — filters change).
    let mut occupied: HashSet<Cell> = scene.sheet(sheet).rooms.iter().map(|r| r.cell).collect();

    let adjacency = store.adjacency();
    let mut placed: HashMap<i64, Cell> = HashMap::new();

    let mut anchored: Vec<&GhostRoom> = store
        .rooms
        .values()
        .filter(|room| room.anchor.is_some())
        .collect();
    anchored.sort_by_key(|room| room.uid);

    for root in anchored {
        if placed.contains_key(&root.uid) {
            continue;
        }
        let anchor = root.anchor.as_ref().expect("filtered to anchored");
        // Anchor must be on the sheet being drawn (and visible through the
        // mini map's building filter).
        let Some((room_sheet, anchor_room)) = scene.room(anchor.room_id) else {
            continue;
        };
        if room_sheet != sheet
            || group_filter.is_some_and(|set| !set.contains(&anchor_room.group))
        {
            continue;
        }

        // Root ghost lands next to its anchor; the anchor edge carries the
        // crossing command ("go shop").
        let root_cell = nearest_free_cell(anchor_room.cell, &occupied);
        occupied.insert(root_cell);
        placed.insert(root.uid, root_cell);
        overlay.edges.push(GhostOverlayEdge {
            a: anchor_room.cell,
            b: root_cell,
            label: anchor.command.clone(),
        });

        // Spread the cluster: each ghost lands near the ghost it was first
        // reached from.
        let mut queue = VecDeque::from([root.uid]);
        while let Some(uid) = queue.pop_front() {
            let from_cell = placed[&uid];
            let mut neighbors: Vec<(i64, Option<&str>)> =
                adjacency.get(&uid).cloned().unwrap_or_default();
            neighbors.sort_by_key(|(n, _)| *n);
            for (next, label) in neighbors {
                if let Some(&next_cell) = placed.get(&next) {
                    // Already placed: still draw the edge (once, lower uid side).
                    if uid < next {
                        overlay.edges.push(GhostOverlayEdge {
                            a: from_cell,
                            b: next_cell,
                            label: label.map(str::to_owned),
                        });
                    }
                    continue;
                }
                if !store.rooms.contains_key(&next) {
                    continue;
                }
                let cell = nearest_free_cell(from_cell, &occupied);
                occupied.insert(cell);
                placed.insert(next, cell);
                overlay.edges.push(GhostOverlayEdge {
                    a: from_cell,
                    b: cell,
                    label: label.map(str::to_owned),
                });
                queue.push_back(next);
            }
        }
    }

    let mut nodes: Vec<GhostNode> = placed
        .into_iter()
        .map(|(uid, cell)| GhostNode {
            uid,
            cell,
            title: store.rooms.get(&uid).and_then(|r| r.title.clone()),
        })
        .collect();
    nodes.sort_by_key(|n| n.uid);
    overlay.nodes = nodes;
    overlay
}

/// Nearest unoccupied cell to `from`, searching rings outward in a fixed
/// order (E, S, W, N first — reading order around the compass — then the
/// diagonals), so results are deterministic. Falls back to stacking east if
/// a huge crowd fills six rings.
fn nearest_free_cell(from: Cell, occupied: &HashSet<Cell>) -> Cell {
    for radius in 1i32..=6 {
        let mut ring: Vec<Cell> = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) == radius {
                    ring.push(Cell {
                        x: from.x + dx,
                        y: from.y + dy,
                    });
                }
            }
        }
        // Cardinal-first within the ring, then by angle-ish reading order.
        ring.sort_by_key(|c| {
            let (dx, dy) = (c.x - from.x, c.y - from.y);
            let cardinal = if dx == 0 || dy == 0 { 0 } else { 1 };
            (cardinal, dy, dx)
        });
        if let Some(cell) = ring.into_iter().find(|c| !occupied.contains(c)) {
            return cell;
        }
    }
    Cell {
        x: from.x + 7,
        y: from.y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(title: &str, exits: &[&str]) -> RoomSnapshot {
        RoomSnapshot {
            title: Some(title.to_string()),
            exits: exits.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn visits_build_an_anchored_cluster() {
        let mut store = GhostStore::default();
        // Street (mapped 369) → shop front → back room → shop front again.
        store.visit(
            633107,
            snapshot("[Shop, Front]", &["out"]),
            Origin::Mapped(369),
            Some("go shop".into()),
        );
        store.visit(
            633108,
            snapshot("[Shop, Back]", &["out"]),
            Origin::Ghost(633107),
            Some("go curtain".into()),
        );
        store.visit(633107, RoomSnapshot::default(), Origin::Ghost(633108), Some("out".into()));

        assert_eq!(store.len(), 2);
        let front = store.get(633107).unwrap();
        assert_eq!(front.title.as_deref(), Some("[Shop, Front]"));
        assert_eq!(front.anchor.as_ref().unwrap().room_id, 369);
        assert_eq!(front.anchor.as_ref().unwrap().command.as_deref(), Some("go shop"));
        // The return trip must not duplicate the edge or overwrite its label.
        assert_eq!(store.edges.len(), 1);
        assert_eq!(store.edges[0].label.as_deref(), Some("go curtain"));
        // A later mapped entry doesn't re-anchor.
        store.visit(633107, RoomSnapshot::default(), Origin::Mapped(999), None);
        assert_eq!(store.get(633107).unwrap().anchor.as_ref().unwrap().room_id, 369);
    }

    #[test]
    fn unknown_origin_records_the_room_but_nothing_else() {
        let mut store = GhostStore::default();
        store.visit(1, snapshot("[Somewhere]", &[]), Origin::Unknown, None);
        assert_eq!(store.len(), 1);
        assert!(store.get(1).unwrap().anchor.is_none());
        assert!(store.edges.is_empty());
    }

    #[test]
    fn overlay_places_clusters_off_their_anchor_without_collisions() {
        use crate::core::layout_engine::scene::{SceneRoom, SheetScene};

        // A 3-room street running east; the shop hangs off room 369.
        let street: Vec<SceneRoom> = (0..3)
            .map(|i| SceneRoom {
                id: 368 + i,
                uid: Some(731008 + i as i64),
                cell: Cell { x: i as i32, y: 0 },
                group: 0,
                entrance: i == 1,
                title: format!("[East Row {i}]"),
            })
            .collect();
        let scene = MapScene {
            location: "Mist Harbor".into(),
            outdoor: SheetScene {
                rooms: street,
                ..Default::default()
            },
            room_index: (0..3u32).map(|i| (368 + i, (Sheet::Outdoor, i as usize))).collect(),
            ..Default::default()
        };

        let mut store = GhostStore::default();
        store.visit(
            633107,
            snapshot("[Shop, Front]", &["out"]),
            Origin::Mapped(369),
            Some("go shop".into()),
        );
        store.visit(
            633108,
            snapshot("[Shop, Back]", &[]),
            Origin::Ghost(633107),
            Some("go curtain".into()),
        );

        let overlay = build_overlay(&store, &scene, Sheet::Outdoor, None);
        assert_eq!(overlay.nodes.len(), 2);
        assert_eq!(overlay.edges.len(), 2);
        // No ghost sits on a street cell, and no two ghosts share a cell.
        let street_cells: HashSet<Cell> =
            scene.outdoor.rooms.iter().map(|r| r.cell).collect();
        let mut seen = HashSet::new();
        for node in &overlay.nodes {
            assert!(!street_cells.contains(&node.cell), "ghost on a mapped room");
            assert!(seen.insert(node.cell), "two ghosts share a cell");
        }
        // The anchor edge starts at room 369's cell and carries the command.
        let anchor_cell = Cell { x: 1, y: 0 };
        let anchor_edge = overlay
            .edges
            .iter()
            .find(|e| e.a == anchor_cell)
            .expect("anchor edge");
        assert_eq!(anchor_edge.label.as_deref(), Some("go shop"));
        // Same input, same layout: placement must be deterministic.
        let again = build_overlay(&store, &scene, Sheet::Outdoor, None);
        assert_eq!(
            overlay.nodes.iter().map(|n| (n.uid, n.cell)).collect::<Vec<_>>(),
            again.nodes.iter().map(|n| (n.uid, n.cell)).collect::<Vec<_>>()
        );
        // Nothing renders on the wrong sheet, and a filter that hides the
        // anchor's group hides the sketch.
        assert!(build_overlay(&store, &scene, Sheet::Interiors, None).is_empty());
        let other_groups: HashSet<usize> = [7usize].into_iter().collect();
        assert!(build_overlay(&store, &scene, Sheet::Outdoor, Some(&other_groups)).is_empty());
    }

    #[test]
    fn ring_search_is_deterministic_and_avoids_occupied() {
        let from = Cell { x: 0, y: 0 };
        let mut occupied = HashSet::new();
        let first = nearest_free_cell(from, &occupied);
        occupied.insert(first);
        let second = nearest_free_cell(from, &occupied);
        assert_ne!(first, second);
        assert_eq!(first, nearest_free_cell(from, &HashSet::new()));
        // Cardinals fill before diagonals.
        assert!(first.x == 0 || first.y == 0);
    }
}
