//! Cluster packing — port of `cluster-packer.js` (spec §7).
//!
//! Places connected components relative to each other by, in priority order:
//! image_coords anchors (hand-drawn map overlays), connector edges (edges
//! between components prove physical adjacency; smallest uid delta wins ties),
//! and a strip fallback. Interiors go on their own shelf sheet.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::direction::DirectionMap;
use super::mapdb::{Room, RoomTable};
use super::positioner::{Cell, Group, PackMethod};

const GROUP_PADDING: i32 = 3; // cells between strip-placed groups
const SEARCH_RADIUS: i32 = 30; // max spiral distance resolving collisions
const DEFAULT_SCALE: f64 = 30.0; // px per grid cell when estimation has no data
const SCALE_MIN: f64 = 5.0;
const SCALE_MAX: f64 = 300.0;
const ANCHOR_PAIR_CAP: usize = 20; // caps the O(n²) scale-estimation pair walk
const GRID_DELTA_MIN: i32 = 1;
const GRID_DELTA_MAX: i32 = 50;
const CROSSING_PENALTY: i64 = 1000;
const COURTYARD_PENALTY: i64 = 4; // per cell inside another group's bbox
const CONNECTOR_COMMIT_CAP: i32 = 30; // committed connector max length
const DIRECTIONAL_COMMIT_CAP: i32 = 8; // committed intra-group edge max length
const BRIDGED_CONTACT_CAP: usize = 10; // contact pairs per excluded component
const UID_DELTA_MISSING: u64 = u64::MAX;

#[derive(Debug, Clone, Copy)]
struct Edge {
    other_group: usize,
    room_id: u32,
    other_room_id: u32,
    uid_delta: u64,
}

#[derive(Debug, Clone)]
struct Anchor {
    room_id: u32,
    image: String,
    px: f64,
    py: f64,
}

#[derive(Debug, Clone, Copy)]
struct Segment {
    a: Cell,
    b: Cell,
    ra: u32,
    rb: u32,
}

#[derive(Debug, Clone, Copy)]
struct BBox {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
}

/// A connector line the candidate placement will create: the group-internal
/// endpoint and the already-final cell it must reach.
struct AnchorLine {
    internal: Cell,
    target: Cell,
    room_id: u32,
    other_room_id: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackInfo {
    pub primary_image: Option<String>,
    /// pack method name → group count (over the packed subset).
    pub methods: BTreeMap<String, usize>,
    pub scale: f64,
}

fn chebyshev(a: Cell, b: Cell) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// JS `Math.round`: half-up toward +∞ (Rust's `round` is half away from zero).
fn js_round(x: f64) -> i32 {
    (x + 0.5).floor() as i32
}

fn uid_delta(a: Option<&Room>, b: Option<&Room>) -> u64 {
    match (
        a.and_then(|r| r.uid.first()),
        b.and_then(|r| r.uid.first()),
    ) {
        (Some(&ua), Some(&ub)) => ua.abs_diff(ub),
        _ => UID_DELTA_MISSING,
    }
}

/// Walk ring `r` around `center` in the reference order: dx from -r to r; at
/// the vertical edges (|dx| == r) every dy, elsewhere only dy = ±r.
fn for_ring(center: Cell, r: i32, mut f: impl FnMut(Cell)) {
    for dx in -r..=r {
        if dx.abs() == r {
            for dy in -r..=r {
                f(Cell {
                    x: center.x + dx,
                    y: center.y + dy,
                });
            }
        } else {
            for dy in [-r, r] {
                f(Cell {
                    x: center.x + dx,
                    y: center.y + dy,
                });
            }
        }
    }
}

fn fits(group: &Group, offset: Cell, occupied: &HashSet<Cell>) -> bool {
    group.positions.values().all(|p| {
        !occupied.contains(&Cell {
            x: p.x + offset.x,
            y: p.y + offset.y,
        })
    })
}

/// Nearest collision-free offset to the proposed one, spiraling outward.
fn find_free_offset(group: &Group, proposed: Cell, occupied: &HashSet<Cell>) -> Option<Cell> {
    if fits(group, proposed, occupied) {
        return Some(proposed);
    }
    for r in 1..=SEARCH_RADIUS {
        let mut found: Option<Cell> = None;
        for_ring(proposed, r, |cand| {
            if found.is_none() && fits(group, cand, occupied) {
                found = Some(cand);
            }
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Strict segment crossing (shared endpoints and collinear touches excluded).
fn segments_cross(a1: Cell, b1: Cell, a2: Cell, b2: Cell) -> bool {
    fn orient(a: Cell, b: Cell, c: Cell) -> i64 {
        let v = (b.x as i64 - a.x as i64) * (c.y as i64 - a.y as i64)
            - (b.y as i64 - a.y as i64) * (c.x as i64 - a.x as i64);
        v.signum()
    }
    let o1 = orient(a1, b1, a2);
    let o2 = orient(a1, b1, b2);
    let o3 = orient(a2, b2, a1);
    let o4 = orient(a2, b2, b1);
    o1 != o2 && o3 != o4 && o1 != 0 && o2 != 0 && o3 != 0 && o4 != 0
}

fn place_group(
    groups: &mut [Group],
    idx: usize,
    offset: Cell,
    method: PackMethod,
    occupied: &mut HashSet<Cell>,
    placed: &mut HashSet<usize>,
) {
    groups[idx].base_offset = Some(offset);
    groups[idx].packing = Some(method);
    placed.insert(idx);
    for p in groups[idx].positions.values() {
        occupied.insert(Cell {
            x: p.x + offset.x,
            y: p.y + offset.y,
        });
    }
}

/// Commit the just-placed group's lines as obstacles for later placements:
/// its connectors to already-placed groups (≤ 30 cells) and its intra-group
/// directional edges (≤ 8 cells), plus its bounding box.
#[allow(clippy::too_many_arguments)]
fn commit_segments(
    groups: &[Group],
    idx: usize,
    edges: &HashMap<usize, Vec<Edge>>,
    packed_set: &HashSet<usize>,
    placed: &HashSet<usize>,
    lookup: &RoomTable,
    dirs: &DirectionMap,
    placed_segments: &mut Vec<Segment>,
    placed_boxes: &mut Vec<BBox>,
) {
    let group = &groups[idx];
    if let Some(group_edges) = edges.get(&idx) {
        for e in group_edges {
            if !placed.contains(&e.other_group) || !packed_set.contains(&e.other_group) {
                continue;
            }
            let a = group.final_cell(e.room_id);
            let b = groups[e.other_group].final_cell(e.other_room_id);
            if chebyshev(a, b) > CONNECTOR_COMMIT_CAP {
                continue;
            }
            placed_segments.push(Segment {
                a,
                b,
                ra: e.room_id,
                rb: e.other_room_id,
            });
        }
    }

    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    for &room_id in &group.room_ids {
        let Some(room) = lookup.get(room_id) else {
            continue;
        };
        for &target_id in room.wayto.keys() {
            if !group.positions.contains_key(&target_id) {
                continue;
            }
            let key = (room_id.min(target_id), room_id.max(target_id));
            if seen.contains(&key) {
                continue;
            }
            if dirs.get(room_id, target_id).is_none() {
                continue;
            }
            seen.insert(key);
            let a = group.final_cell(room_id);
            let b = group.final_cell(target_id);
            if chebyshev(a, b) > DIRECTIONAL_COMMIT_CAP {
                continue;
            }
            placed_segments.push(Segment {
                a,
                b,
                ra: room_id,
                rb: target_id,
            });
        }
    }

    let bounds = group.bounds();
    let off = groups[idx].base_offset.expect("group was just placed");
    placed_boxes.push(BBox {
        min_x: bounds.min_x + off.x,
        max_x: bounds.max_x + off.x,
        min_y: bounds.min_y + off.y,
        max_y: bounds.max_y + off.y,
    });
}

/// Connector edges: any wayto between rooms of different packed components.
/// These carry no direction but prove adjacency.
fn collect_connector_edges(
    groups: &[Group],
    packed: &[usize],
    lookup: &RoomTable,
) -> HashMap<usize, Vec<Edge>> {
    let mut component_of: HashMap<u32, usize> = HashMap::new();
    for &idx in packed {
        for &id in &groups[idx].room_ids {
            component_of.insert(id, idx);
        }
    }

    let mut edges: HashMap<usize, Vec<Edge>> = HashMap::new();
    for &idx in packed {
        for &room_id in &groups[idx].room_ids {
            let Some(room) = lookup.get(room_id) else {
                continue;
            };
            for &target_id in room.wayto.keys() {
                let Some(&other) = component_of.get(&target_id) else {
                    continue;
                };
                if other == idx {
                    continue;
                }
                edges.entry(idx).or_default().push(Edge {
                    other_group: other,
                    room_id,
                    other_room_id: target_id,
                    uid_delta: uid_delta(Some(room), lookup.get(target_id)),
                });
            }
        }
    }
    edges
}

/// Virtual edges between packed groups whose only link runs through an
/// excluded (interior) component — e.g. two shores of a ferry interior.
fn add_bridged_edges(
    edges: &mut HashMap<usize, Vec<Edge>>,
    groups: &[Group],
    packed_set: &HashSet<usize>,
    lookup: &RoomTable,
) {
    let mut component_of_all: HashMap<u32, usize> = HashMap::new();
    for group in groups {
        for &id in &group.room_ids {
            component_of_all.insert(id, group.index);
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    struct Contact {
        group: usize,
        room_id: u32,
    }
    // Excluded component → packed-side contacts, in first-encounter order.
    let mut contact_order: Vec<usize> = Vec::new();
    let mut contacts: HashMap<usize, Vec<Contact>> = HashMap::new();
    let add_contact =
        |excluded: usize,
         packed_idx: usize,
         room_id: u32,
         contact_order: &mut Vec<usize>,
         contacts: &mut HashMap<usize, Vec<Contact>>| {
            let list = contacts.entry(excluded).or_insert_with(|| {
                contact_order.push(excluded);
                Vec::new()
            });
            let c = Contact {
                group: packed_idx,
                room_id,
            };
            if !list.contains(&c) {
                list.push(c);
            }
        };

    for group in groups {
        for &room_id in &group.room_ids {
            let Some(room) = lookup.get(room_id) else {
                continue;
            };
            for &target_id in room.wayto.keys() {
                let Some(&target_group) = component_of_all.get(&target_id) else {
                    continue;
                };
                if target_group == group.index {
                    continue;
                }
                let group_packed = packed_set.contains(&group.index);
                let target_packed = packed_set.contains(&target_group);
                if group_packed && !target_packed {
                    add_contact(
                        target_group,
                        group.index,
                        room_id,
                        &mut contact_order,
                        &mut contacts,
                    );
                } else if !group_packed && target_packed {
                    add_contact(
                        group.index,
                        target_group,
                        target_id,
                        &mut contact_order,
                        &mut contacts,
                    );
                }
            }
        }
    }

    for excluded in contact_order {
        let list = &contacts[&excluded];
        let cap = list.len().min(BRIDGED_CONTACT_CAP);
        for i in 0..cap {
            for j in (i + 1)..cap {
                if list[i].group == list[j].group {
                    continue;
                }
                let delta = uid_delta(lookup.get(list[i].room_id), lookup.get(list[j].room_id));
                edges.entry(list[i].group).or_default().push(Edge {
                    other_group: list[j].group,
                    room_id: list[i].room_id,
                    other_room_id: list[j].room_id,
                    uid_delta: delta,
                });
                edges.entry(list[j].group).or_default().push(Edge {
                    other_group: list[i].group,
                    room_id: list[j].room_id,
                    other_room_id: list[i].room_id,
                    uid_delta: delta,
                });
            }
        }
    }
}

/// The geographic base map is whatever image anchors the largest component.
/// Raw per-room counts can be fooled by collage overlays.
fn find_primary_image(
    groups: &[Group],
    packed: &[usize],
    anchors: &HashMap<usize, Vec<Anchor>>,
) -> Option<String> {
    let mut largest: Option<usize> = None;
    let mut largest_rooms = 0usize;
    for &idx in packed {
        if anchors.get(&idx).map(Vec::len).unwrap_or(0) == 0 {
            continue;
        }
        let n = groups[idx].room_ids.len();
        if n > largest_rooms {
            largest_rooms = n;
            largest = Some(idx);
        }
    }
    let largest = largest?;

    let mut counts: Vec<(&str, usize)> = Vec::new();
    for a in &anchors[&largest] {
        if let Some(entry) = counts.iter_mut().find(|(img, _)| *img == a.image) {
            entry.1 += 1;
        } else {
            counts.push((&a.image, 1));
        }
    }
    let mut best: Option<&str> = None;
    let mut best_count = 0usize;
    for (image, count) in counts {
        if count > best_count {
            best_count = count;
            best = Some(image);
        }
    }
    best.map(str::to_owned)
}

/// Pixels per grid cell: median ratio of pixel delta to grid delta over
/// anchored room pairs WITHIN components (grid positions are solver-trusted,
/// pixel positions cartographer-trusted).
fn estimate_scale(
    groups: &[Group],
    packed: &[usize],
    anchors: &HashMap<usize, Vec<Anchor>>,
    primary_image: Option<&str>,
) -> f64 {
    let Some(primary) = primary_image else {
        return DEFAULT_SCALE;
    };
    let mut ratios: Vec<f64> = Vec::new();

    for &idx in packed {
        let list: Vec<&Anchor> = anchors
            .get(&idx)
            .map(|l| l.iter().filter(|a| a.image == primary).collect())
            .unwrap_or_default();
        if list.len() < 2 {
            continue;
        }
        let limit = list.len().min(ANCHOR_PAIR_CAP);
        for i in 0..limit {
            for j in (i + 1)..limit {
                let pa = groups[idx].positions[&list[i].room_id];
                let pb = groups[idx].positions[&list[j].room_id];
                let grid_dx = (pa.x - pb.x).abs();
                let grid_dy = (pa.y - pb.y).abs();
                let pix_dx = (list[i].px - list[j].px).abs();
                let pix_dy = (list[i].py - list[j].py).abs();
                if (GRID_DELTA_MIN..=GRID_DELTA_MAX).contains(&grid_dx) {
                    ratios.push(pix_dx / grid_dx as f64);
                }
                if (GRID_DELTA_MIN..=GRID_DELTA_MAX).contains(&grid_dy) {
                    ratios.push(pix_dy / grid_dy as f64);
                }
            }
        }
    }

    if ratios.is_empty() {
        return DEFAULT_SCALE;
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).expect("finite ratios"));
    let median = ratios[ratios.len() / 2];
    median.clamp(SCALE_MIN, SCALE_MAX)
}

/// Bounds of the occupied set as the reference computes them: minX and maxY
/// only, both biased toward 0 by their initial values.
fn occupied_bounds(occupied: &HashSet<Cell>) -> (i32, i32) {
    let mut min_x = 0;
    let mut max_y = 0;
    for c in occupied {
        if c.x < min_x {
            min_x = c.x;
        }
        if c.y > max_y {
            max_y = c.y;
        }
    }
    (min_x, max_y)
}

/// Connector-aware placement: among collision-free offsets near the proposed
/// one, prefer short connector lines that cross as few existing lines as
/// possible and avoid other groups' bounding boxes. Explores two rings past
/// the nearest fit so a clean spot can beat a marginally closer tangled one.
fn find_best_connector_offset(
    group: &Group,
    proposed: Cell,
    occupied: &HashSet<Cell>,
    anchor_lines: &[AnchorLine],
    placed_segments: &[Segment],
    placed_boxes: &[BBox],
) -> Option<Cell> {
    let reach = SEARCH_RADIUS + 4;
    let mut win_min_x = proposed.x;
    let mut win_max_x = proposed.x;
    let mut win_min_y = proposed.y;
    let mut win_max_y = proposed.y;
    for a in anchor_lines {
        win_min_x = win_min_x.min(a.target.x);
        win_max_x = win_max_x.max(a.target.x);
        win_min_y = win_min_y.min(a.target.y);
        win_max_y = win_max_y.max(a.target.y);
    }
    let local_segments: Vec<&Segment> = placed_segments
        .iter()
        .filter(|seg| {
            seg.a.x.max(seg.b.x) >= win_min_x - reach
                && seg.a.x.min(seg.b.x) <= win_max_x + reach
                && seg.a.y.max(seg.b.y) >= win_min_y - reach
                && seg.a.y.min(seg.b.y) <= win_max_y + reach
        })
        .collect();
    let local_boxes: Vec<&BBox> = placed_boxes
        .iter()
        .filter(|b| {
            b.max_x >= win_min_x - reach
                && b.min_x <= win_max_x + reach
                && b.max_y >= win_min_y - reach
                && b.min_y <= win_max_y + reach
        })
        .collect();

    let group_cells: Vec<Cell> = group.positions.values().copied().collect();
    let mut best: Option<Cell> = None;
    let mut best_score = i64::MAX;

    let consider = |candidate: Cell, best: &mut Option<Cell>, best_score: &mut i64| {
        if !fits(group, candidate, occupied) {
            return;
        }
        let mut score = 0i64;
        for anchor in anchor_lines {
            let endpoint = Cell {
                x: anchor.internal.x + candidate.x,
                y: anchor.internal.y + candidate.y,
            };
            score += chebyshev(endpoint, anchor.target) as i64;
            for seg in &local_segments {
                if seg.ra == anchor.room_id
                    || seg.rb == anchor.room_id
                    || seg.ra == anchor.other_room_id
                    || seg.rb == anchor.other_room_id
                {
                    continue;
                }
                if segments_cross(endpoint, anchor.target, seg.a, seg.b) {
                    score += CROSSING_PENALTY;
                }
            }
        }
        // Discourage landing inside another group's footprint (courtyards).
        for cell in &group_cells {
            let cx = cell.x + candidate.x;
            let cy = cell.y + candidate.y;
            for b in &local_boxes {
                if cx >= b.min_x && cx <= b.max_x && cy >= b.min_y && cy <= b.max_y {
                    score += COURTYARD_PENALTY;
                    break;
                }
            }
        }
        if score < *best_score {
            *best_score = score;
            *best = Some(candidate);
        }
    };

    consider(proposed, &mut best, &mut best_score);
    let mut first_fit_radius: Option<i32> = None;
    for r in 1..=SEARCH_RADIUS {
        if best.is_some() && first_fit_radius.is_none() {
            first_fit_radius = Some(r - 1);
        }
        if let Some(f) = first_fit_radius {
            if r > f + 2 {
                break;
            }
        }
        for_ring(proposed, r, |cand| consider(cand, &mut best, &mut best_score));
    }
    best
}

/// Port of `ClusterPacker.packGroups`: place the packed subset (typically the
/// outdoor components) onto one shared sheet. `packed` lists indices into
/// `groups`; all of `groups` is consulted for bridged virtual edges.
pub fn pack_groups(
    groups: &mut Vec<Group>,
    packed: &[usize],
    lookup: &RoomTable,
    dirs: &DirectionMap,
) -> PackInfo {
    if packed.is_empty() {
        return PackInfo::default();
    }
    let packed_set: HashSet<usize> = packed.iter().copied().collect();

    let mut edges = collect_connector_edges(groups, packed, lookup);
    if groups.len() > packed.len() {
        add_bridged_edges(&mut edges, groups, &packed_set, lookup);
    }

    // Anchors: rooms carrying image + image_coords, per group in room order.
    let mut anchors: HashMap<usize, Vec<Anchor>> = HashMap::new();
    for &idx in packed {
        let mut list = Vec::new();
        for &room_id in &groups[idx].room_ids {
            let Some(room) = lookup.get(room_id) else {
                continue;
            };
            if let (Some(image), Some(coords)) = (&room.image, &room.image_coords) {
                list.push(Anchor {
                    room_id,
                    image: image.clone(),
                    px: (coords[0] + coords[2]) / 2.0,
                    py: (coords[1] + coords[3]) / 2.0,
                });
            }
        }
        if !list.is_empty() {
            anchors.insert(idx, list);
        }
    }

    let primary_image = find_primary_image(groups, packed, &anchors);
    let scale = estimate_scale(groups, packed, &anchors, primary_image.as_deref());

    let mut occupied: HashSet<Cell> = HashSet::new();
    let mut placed: HashSet<usize> = HashSet::new();
    let mut placed_segments: Vec<Segment> = Vec::new();
    let mut placed_boxes: Vec<BBox> = Vec::new();

    // --- Pass 1: image-anchored groups ---
    if let Some(primary) = primary_image.as_deref() {
        let mut anchored: Vec<usize> = packed
            .iter()
            .copied()
            .filter(|idx| {
                anchors
                    .get(idx)
                    .map(|l| l.iter().any(|a| a.image == primary))
                    .unwrap_or(false)
            })
            .collect();
        let anchor_count = |idx: usize| {
            anchors
                .get(&idx)
                .map(|l| l.iter().filter(|a| a.image == primary).count())
                .unwrap_or(0)
        };
        anchored.sort_by(|&a, &b| {
            anchor_count(b)
                .cmp(&anchor_count(a))
                .then(groups[b].room_ids.len().cmp(&groups[a].room_ids.len()))
        });

        for idx in anchored {
            let group_anchors: Vec<&Anchor> = anchors[&idx]
                .iter()
                .filter(|a| a.image == primary)
                .collect();
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            for a in &group_anchors {
                let internal = groups[idx].positions[&a.room_id];
                sum_x += a.px / scale - internal.x as f64;
                sum_y += a.py / scale - internal.y as f64;
            }
            let n = group_anchors.len() as f64;
            let proposed = Cell {
                x: js_round(sum_x / n),
                y: js_round(sum_y / n),
            };
            if let Some(offset) = find_free_offset(&groups[idx], proposed, &occupied) {
                place_group(
                    groups,
                    idx,
                    offset,
                    PackMethod::Image,
                    &mut occupied,
                    &mut placed,
                );
                commit_segments(
                    groups,
                    idx,
                    &edges,
                    &packed_set,
                    &placed,
                    lookup,
                    dirs,
                    &mut placed_segments,
                    &mut placed_boxes,
                );
            }
        }
    }

    // --- Pass 2: connector-attached groups, BFS outward from placed ones ---
    if placed.is_empty() {
        let mut largest = packed[0];
        for &idx in packed {
            if groups[idx].room_ids.len() > groups[largest].room_ids.len() {
                largest = idx;
            }
        }
        // The initial seed does NOT commit its segments (reference behavior).
        place_group(
            groups,
            largest,
            Cell { x: 0, y: 0 },
            PackMethod::Seed,
            &mut occupied,
            &mut placed,
        );
    }

    let mut deferred: HashSet<usize> = HashSet::new();
    let mut seed_base_x: Option<i32> = None;
    loop {
        // The unplaced group with the most edges to placed groups; ties go to
        // the smallest uid delta, then to group order.
        struct Best {
            idx: usize,
            placed_edges: Vec<Edge>,
            min_uid_delta: u64,
        }
        let mut best: Option<Best> = None;
        for &idx in packed {
            if placed.contains(&idx) || deferred.contains(&idx) {
                continue;
            }
            let placed_edges: Vec<Edge> = edges
                .get(&idx)
                .map(|l| {
                    l.iter()
                        .filter(|e| placed.contains(&e.other_group))
                        .copied()
                        .collect()
                })
                .unwrap_or_default();
            if placed_edges.is_empty() {
                continue;
            }
            let min_uid_delta = placed_edges.iter().map(|e| e.uid_delta).min().unwrap();
            let better = match &best {
                None => true,
                Some(b) => {
                    placed_edges.len() > b.placed_edges.len()
                        || (placed_edges.len() == b.placed_edges.len()
                            && min_uid_delta < b.min_uid_delta)
                }
            };
            if better {
                best = Some(Best {
                    idx,
                    placed_edges,
                    min_uid_delta,
                });
            }
        }

        let Some(best) = best else {
            // Nothing touches the placed set; seed the largest remaining
            // connector super-cluster below the current map.
            let mut seed_group: Option<usize> = None;
            for &idx in packed {
                if placed.contains(&idx)
                    || deferred.contains(&idx)
                    || edges.get(&idx).map(Vec::len).unwrap_or(0) == 0
                {
                    continue;
                }
                if seed_group
                    .map(|s| groups[idx].room_ids.len() > groups[s].room_ids.len())
                    .unwrap_or(true)
                {
                    seed_group = Some(idx);
                }
            }
            let Some(idx) = seed_group else {
                break;
            };
            let bounds = groups[idx].bounds();
            let (extent_min_x, extent_max_y) = occupied_bounds(&occupied);
            let base_x = *seed_base_x.get_or_insert(extent_min_x);
            let proposed = Cell {
                x: base_x - bounds.min_x,
                y: extent_max_y + GROUP_PADDING - bounds.min_y,
            };
            let offset = find_free_offset(&groups[idx], proposed, &occupied).unwrap_or(proposed);
            place_group(
                groups,
                idx,
                offset,
                PackMethod::Seed,
                &mut occupied,
                &mut placed,
            );
            commit_segments(
                groups,
                idx,
                &edges,
                &packed_set,
                &placed,
                lookup,
                dirs,
                &mut placed_segments,
                &mut placed_boxes,
            );
            continue;
        };

        // Pack next to the neighbor most likely to be physically adjacent.
        let edge = *best
            .placed_edges
            .iter()
            .min_by_key(|e| e.uid_delta)
            .expect("placed_edges is non-empty");
        let neighbor_cell = groups[edge.other_group].final_cell(edge.other_room_id);
        let internal = groups[best.idx].positions[&edge.room_id];

        let anchor_lines: Vec<AnchorLine> = best
            .placed_edges
            .iter()
            .map(|e| AnchorLine {
                internal: groups[best.idx].positions[&e.room_id],
                target: groups[e.other_group].final_cell(e.other_room_id),
                room_id: e.room_id,
                other_room_id: e.other_room_id,
            })
            .collect();

        let proposed = Cell {
            x: neighbor_cell.x - internal.x,
            y: neighbor_cell.y - internal.y,
        };
        if let Some(offset) = find_best_connector_offset(
            &groups[best.idx],
            proposed,
            &occupied,
            &anchor_lines,
            &placed_segments,
            &placed_boxes,
        ) {
            place_group(
                groups,
                best.idx,
                offset,
                PackMethod::Connector,
                &mut occupied,
                &mut placed,
            );
            commit_segments(
                groups,
                best.idx,
                &edges,
                &packed_set,
                &placed,
                lookup,
                dirs,
                &mut placed_segments,
                &mut placed_boxes,
            );
        } else {
            // No room nearby; leave it for the strip pass and keep going.
            deferred.insert(best.idx);
        }
    }

    // --- Pass 3: strip fallback for whatever is left ---
    let mut leftovers: Vec<usize> = packed
        .iter()
        .copied()
        .filter(|idx| !placed.contains(idx))
        .collect();
    leftovers.sort_by(|&a, &b| groups[b].room_ids.len().cmp(&groups[a].room_ids.len()));
    if !leftovers.is_empty() {
        let mut max_y = 0;
        let mut min_x = 0;
        for c in &occupied {
            if c.y > max_y {
                max_y = c.y;
            }
            if c.x < min_x {
                min_x = c.x;
            }
        }
        let mut cursor_x = min_x;
        let strip_y = max_y + GROUP_PADDING;
        for idx in leftovers {
            let bounds = groups[idx].bounds();
            let proposed = Cell {
                x: cursor_x - bounds.min_x,
                y: strip_y - bounds.min_y,
            };
            let offset = find_free_offset(&groups[idx], proposed, &occupied).unwrap_or(proposed);
            place_group(
                groups,
                idx,
                offset,
                PackMethod::Strip,
                &mut occupied,
                &mut placed,
            );
            cursor_x += bounds.width() + GROUP_PADDING;
        }
    }

    let mut methods: BTreeMap<String, usize> = BTreeMap::new();
    for &idx in packed {
        let name = groups[idx].packing.map(PackMethod::name).unwrap_or("none");
        *methods.entry(name.to_owned()).or_insert(0) += 1;
    }
    PackInfo {
        primary_image,
        methods,
        scale,
    }
}

/// Interiors sheet: wrapped shelf rows in an independent coordinate space.
/// A cluster (one walkable building) is merged into a single floor plan
/// first — members are placed beside the rooms their passages connect to,
/// like the outdoor connector pass but cluster-local — and then each merged
/// building is shelved as one unit (spec §7).
pub fn pack_interior_shelf(
    groups: &mut Vec<Group>,
    interior: &[usize],
    clusters: &HashMap<usize, usize>,
    lookup: &RoomTable,
) {
    if interior.is_empty() {
        return;
    }

    // Cluster membership, canonical order (BTreeMap keys + sorted members).
    let mut members_of: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for &idx in interior {
        members_of
            .entry(clusters.get(&idx).copied().unwrap_or(idx))
            .or_default()
            .push(idx);
    }
    for members in members_of.values_mut() {
        members.sort_unstable();
    }

    // One shelf item per cluster: member → cluster-local frame offset.
    struct Item {
        local: Vec<(usize, Cell)>,
        width: i32,
        height: i32,
    }
    let mut items: Vec<Item> = Vec::new();
    for members in members_of.values() {
        let mut local: HashMap<usize, Cell> = HashMap::new();
        if members.len() == 1 {
            local.insert(members[0], Cell::default());
        } else {
            merge_cluster_members(groups, members, lookup, &mut local);
        }
        // Normalize the frame to a (0,0) top-left.
        let mut min = Cell {
            x: i32::MAX,
            y: i32::MAX,
        };
        let mut max = Cell {
            x: i32::MIN,
            y: i32::MIN,
        };
        for (&idx, off) in &local {
            let b = groups[idx].bounds();
            min.x = min.x.min(b.min_x + off.x);
            min.y = min.y.min(b.min_y + off.y);
            max.x = max.x.max(b.max_x + off.x);
            max.y = max.y.max(b.max_y + off.y);
        }
        let mut ordered: Vec<(usize, Cell)> = members
            .iter()
            .map(|&idx| {
                let off = local[&idx];
                (
                    idx,
                    Cell {
                        x: off.x - min.x,
                        y: off.y - min.y,
                    },
                )
            })
            .collect();
        ordered.sort_unstable_by_key(|&(idx, _)| idx);
        items.push(Item {
            local: ordered,
            width: max.x - min.x + 1,
            height: max.y - min.y + 1,
        });
    }

    // Padded area, so the sheet comes out roughly square even when padding
    // dwarfs the mostly tiny buildings.
    let total_area: i64 = items
        .iter()
        .map(|i| (i.width + GROUP_PADDING) as i64 * (i.height + GROUP_PADDING) as i64)
        .sum();
    let row_width = ((total_area as f64).sqrt().ceil() as i32).max(20);

    let mut cursor_x = 0;
    let mut cursor_y = 0;
    let mut row_height = 0;
    for item in items {
        if cursor_x > 0 && cursor_x + item.width > row_width {
            cursor_y += row_height + GROUP_PADDING;
            cursor_x = 0;
            row_height = 0;
        }
        for (idx, local) in &item.local {
            groups[*idx].base_offset = Some(Cell {
                x: cursor_x + local.x,
                y: cursor_y + local.y,
            });
            groups[*idx].packing = Some(PackMethod::InteriorShelf);
        }
        cursor_x += item.width + GROUP_PADDING;
        row_height = row_height.max(item.height);
    }
}

/// Place a multi-component building's members into one local frame: seed the
/// largest, then repeatedly land the member with the most passages into the
/// placed set beside the room its lowest-uid-delta passage connects to
/// (nearest free offset). Clusters are connected by construction, but any
/// straggler falls back to the frame's right edge.
fn merge_cluster_members(
    groups: &[Group],
    members: &[usize],
    lookup: &RoomTable,
    local: &mut HashMap<usize, Cell>,
) {
    let member_set: std::collections::HashSet<usize> = members.iter().copied().collect();
    let mut group_of: HashMap<u32, usize> = HashMap::new();
    for &idx in members {
        for &id in &groups[idx].room_ids {
            group_of.insert(id, idx);
        }
    }
    // Passages between members: (member, other_member, room, other_room, uid delta).
    let mut edges: HashMap<usize, Vec<Edge>> = HashMap::new();
    for &idx in members {
        for &room_id in &groups[idx].room_ids {
            let Some(room) = lookup.get(room_id) else {
                continue;
            };
            for &target_id in room.wayto.keys() {
                let Some(&other) = group_of.get(&target_id) else {
                    continue;
                };
                if other == idx || !member_set.contains(&other) {
                    continue;
                }
                edges.entry(idx).or_default().push(Edge {
                    other_group: other,
                    room_id,
                    other_room_id: target_id,
                    uid_delta: uid_delta(Some(room), lookup.get(target_id)),
                });
            }
        }
    }

    let mut occupied: HashSet<Cell> = HashSet::new();
    let mut place = |idx: usize, off: Cell, occupied: &mut HashSet<Cell>| {
        for p in groups[idx].positions.values() {
            occupied.insert(Cell {
                x: p.x + off.x,
                y: p.y + off.y,
            });
        }
    };

    let seed = *members
        .iter()
        .max_by_key(|&&idx| (groups[idx].room_ids.len(), std::cmp::Reverse(idx)))
        .expect("members is non-empty");
    local.insert(seed, Cell::default());
    place(seed, Cell::default(), &mut occupied);

    loop {
        // Most placed-passages first; ties to the lowest member index.
        let mut best: Option<(usize, Vec<Edge>)> = None;
        for &idx in members {
            if local.contains_key(&idx) {
                continue;
            }
            let placed_edges: Vec<Edge> = edges
                .get(&idx)
                .map(|l| {
                    l.iter()
                        .filter(|e| local.contains_key(&e.other_group))
                        .copied()
                        .collect()
                })
                .unwrap_or_default();
            if placed_edges.is_empty() {
                continue;
            }
            if best
                .as_ref()
                .map(|(_, b)| placed_edges.len() > b.len())
                .unwrap_or(true)
            {
                best = Some((idx, placed_edges));
            }
        }

        let Some((idx, placed_edges)) = best else {
            // Stragglers (shouldn't happen: clusters are connected).
            let Some(&idx) = members.iter().find(|&&m| !local.contains_key(&m)) else {
                return;
            };
            let max_x = occupied.iter().map(|c| c.x).max().unwrap_or(0);
            let bounds = groups[idx].bounds();
            let proposed = Cell {
                x: max_x + 2 - bounds.min_x,
                y: 0,
            };
            let off = find_free_offset(&groups[idx], proposed, &occupied).unwrap_or(proposed);
            local.insert(idx, off);
            place(idx, off, &mut occupied);
            continue;
        };

        let edge = *placed_edges
            .iter()
            .min_by_key(|e| e.uid_delta)
            .expect("placed_edges is non-empty");
        let neighbor_off = local[&edge.other_group];
        let neighbor_room = groups[edge.other_group].positions[&edge.other_room_id];
        let internal = groups[idx].positions[&edge.room_id];
        // Land the passage endpoints as close together as the frame allows.
        let proposed = Cell {
            x: neighbor_room.x + neighbor_off.x - internal.x,
            y: neighbor_room.y + neighbor_off.y - internal.y,
        };
        let off = find_free_offset(&groups[idx], proposed, &occupied).unwrap_or(Cell {
            x: proposed.x + groups[edge.other_group].bounds().width() + 1,
            y: proposed.y,
        });
        local.insert(idx, off);
        place(idx, off, &mut occupied);
    }
}
