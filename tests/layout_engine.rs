//! Layout engine validation against the reference implementation's fixtures
//! (docs/layout-fixtures.json, spec §9).
//!
//! Hard invariants for any zone: zero rooms sharing a cell per sheet, every
//! room placed exactly once, and deterministic output across runs. The
//! statistical targets are asserted exactly: the port currently reproduces
//! the reference stats bit-for-bit on all seven zones. If a legitimate
//! algorithm change moves a number, regenerate the fixtures with the
//! reference's `tools/export-fixtures.mjs` and update the room extracts in
//! `tests/fixtures/layout/` from the same mapdb snapshot.

use std::collections::{HashMap, HashSet};

use vellum_fe::core::layout_engine::{
    generate_layout, generate_layout_reference, Cell, Layout, LayoutStats,
};
use vellum_fe::core::mapdb::{self, Room};

/// (fixture zone name, room-extract file stem)
const ZONES: [(&str, &str); 7] = [
    ("Moonsedge", "moonsedge"),
    ("the Atoll", "the-atoll"),
    ("Mist Harbor", "mist-harbor"),
    ("Icemule Trace", "icemule-trace"),
    ("Wehnimer's Landing", "wehnimers-landing"),
    ("Solhaven", "solhaven"),
    ("Ta'Illistim", "ta-illistim"),
];

fn fixture_stats(zone: &str) -> LayoutStats {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/layout-fixtures.json"
    ))
    .expect("docs/layout-fixtures.json");
    let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    serde_json::from_value(doc["zones"][zone].clone())
        .unwrap_or_else(|e| panic!("fixture entry for {zone}: {e}"))
}

fn load_rooms(file: &str) -> Vec<Room> {
    let path = format!(
        "{}/tests/fixtures/layout/{file}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let json = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    mapdb::rooms_from_array(&json).expect("valid room fixture JSON")
}

/// Every room placed exactly once, and no two rooms share a cell per sheet.
fn assert_hard_invariants(layout: &Layout, rooms: &[Room]) {
    let mut placed: HashSet<u32> = HashSet::new();
    for group in &layout.groups {
        assert!(
            group.base_offset.is_some(),
            "group {} was never packed",
            group.index
        );
        for &id in &group.room_ids {
            assert!(placed.insert(id), "room {id} placed in two groups");
        }
    }
    assert_eq!(placed.len(), rooms.len(), "every room must be placed");

    for (label, sheet) in [
        ("outdoor", &layout.outdoor),
        ("interiors", &layout.interiors),
    ] {
        let mut cells: HashMap<Cell, u32> = HashMap::new();
        for &idx in sheet {
            let group = &layout.groups[idx];
            for &id in &group.room_ids {
                let cell = group.final_cell(id);
                if let Some(other) = cells.insert(cell, id) {
                    panic!("{label} sheet: rooms {other} and {id} share cell {cell:?}");
                }
            }
        }
    }
}

/// Full placement snapshot for determinism comparison.
fn placement_snapshot(layout: &Layout) -> Vec<(usize, u32, Cell)> {
    let mut snap = Vec::new();
    for group in &layout.groups {
        for &id in &group.room_ids {
            snap.push((group.index, id, group.final_cell(id)));
        }
    }
    snap
}

/// Fixture parity runs the pipeline exactly as the JS reference does —
/// without the try-inline pass, which is a deliberate divergence tested
/// separately below.
fn run_zone(file: &str) -> (Layout, LayoutStats, Vec<Room>) {
    let mut rooms = load_rooms(file);
    let layout = generate_layout_reference(&mut rooms);
    let stats = LayoutStats::compute(&layout, &rooms);
    (layout, stats, rooms)
}

#[test]
fn small_zones_match_reference_fixtures() {
    for (zone, file) in [("Moonsedge", "moonsedge"), ("the Atoll", "the-atoll")] {
        let expected = fixture_stats(zone);
        let (layout, stats, rooms) = run_zone(file);
        assert_hard_invariants(&layout, &rooms);
        assert_eq!(stats, expected, "{zone} diverges from the reference");
    }
}

// The big zones take a few seconds each without optimization, so they get
// their own tests (parallel by default) instead of one serial loop.
macro_rules! zone_test {
    ($test_name:ident, $zone:expr, $file:expr) => {
        #[test]
        fn $test_name() {
            let expected = fixture_stats($zone);
            let (layout, stats, rooms) = run_zone($file);
            assert_hard_invariants(&layout, &rooms);
            assert_eq!(stats, expected, "{} diverges from the reference", $zone);
        }
    };
}

zone_test!(mist_harbor_matches_reference, "Mist Harbor", "mist-harbor");
zone_test!(icemule_trace_matches_reference, "Icemule Trace", "icemule-trace");
zone_test!(
    wehnimers_landing_matches_reference,
    "Wehnimer's Landing",
    "wehnimers-landing"
);
zone_test!(solhaven_matches_reference, "Solhaven", "solhaven");
zone_test!(ta_illistim_matches_reference, "Ta'Illistim", "ta-illistim");

#[test]
fn layout_is_deterministic_across_runs() {
    let (first_layout, first_stats, _) = run_zone("moonsedge");
    let (second_layout, second_stats, _) = run_zone("moonsedge");

    assert_eq!(first_stats, second_stats);
    assert_eq!(
        placement_snapshot(&first_layout),
        placement_snapshot(&second_layout),
        "same mapdb bytes in must give the same layout out"
    );
}

// ---- Try-inline pass (deliberate divergence from the reference) ----
//
// Production layouts (`generate_layout`) seat clean-fitting interior
// buildings on the outdoor sheet; the fixture parity above pins the shared
// core, these pin the divergence.

#[test]
fn atoll_inlines_its_grottos() {
    let mut rooms = load_rooms("the-atoll");
    let layout = generate_layout(&mut rooms);
    assert_hard_invariants(&layout, &rooms);
    assert!(
        !layout.inlined.is_empty(),
        "the Atoll's grotto buildings should seat on the main sheet"
    );
    let outdoor: HashSet<usize> = layout.outdoor.iter().copied().collect();
    for &idx in &layout.inlined {
        assert!(outdoor.contains(&idx), "inlined group {idx} not on the outdoor sheet");
        assert!(
            !layout.interiors.contains(&idx),
            "inlined group {idx} still on the shelf"
        );
        assert!(
            !layout.classification.interior_groups.contains(&idx),
            "inlined group {idx} still classified interior (stale door markers)"
        );
    }
}

/// Every inlined building must sit within doorway reach of the outdoor
/// room it enters from — the whole point of the budget.
#[test]
fn inlined_buildings_sit_beside_their_doorways() {
    let mut rooms = load_rooms("the-atoll");
    let layout = generate_layout(&mut rooms);
    let inlined: HashSet<usize> = layout.inlined.iter().copied().collect();
    if inlined.is_empty() {
        return; // covered by atoll_inlines_its_grottos
    }
    let mut cell_of: HashMap<u32, (usize, Cell)> = HashMap::new();
    for &idx in &layout.outdoor {
        let group = &layout.groups[idx];
        for &id in &group.room_ids {
            cell_of.insert(id, (idx, group.final_cell(id)));
        }
    }
    let mut best_door: HashMap<usize, i32> = HashMap::new();
    for room in &rooms {
        let Some(&(ga, a)) = cell_of.get(&room.id) else {
            continue;
        };
        for &target in room.wayto.keys() {
            let Some(&(gb, b)) = cell_of.get(&target) else {
                continue;
            };
            if ga == gb || inlined.contains(&ga) == inlined.contains(&gb) {
                continue; // want doorways between an inlined and a plain outdoor group
            }
            let len = (a.x - b.x).abs().max((a.y - b.y).abs());
            let key = if inlined.contains(&ga) { ga } else { gb };
            let entry = best_door.entry(key).or_insert(i32::MAX);
            *entry = (*entry).min(len);
        }
    }
    for &idx in &layout.inlined {
        let len = best_door.get(&idx).copied().unwrap_or(i32::MAX);
        assert!(
            len <= 8,
            "inlined group {idx}: nearest doorway connector is {len} cells"
        );
    }
}

/// Towns keep their shelf wholesale: a location with many interior
/// buildings never inlines, even where the ground looks open.
#[test]
fn towns_keep_their_interior_shelf() {
    for file in ["wehnimers-landing", "icemule-trace", "solhaven", "ta-illistim"] {
        let mut rooms = load_rooms(file);
        let layout = generate_layout(&mut rooms);
        assert_hard_invariants(&layout, &rooms);
        assert!(
            layout.inlined.is_empty(),
            "{file}: town inlined {} buildings",
            layout.inlined.len()
        );
    }
}

#[test]
fn inline_pass_is_deterministic() {
    let mut first_rooms = load_rooms("the-atoll");
    let first = generate_layout(&mut first_rooms);
    let mut second_rooms = load_rooms("the-atoll");
    let second = generate_layout(&mut second_rooms);
    assert_eq!(first.inlined, second.inlined);
    assert_eq!(placement_snapshot(&first), placement_snapshot(&second));
}

/// A curated Interior flip pins the building to the shelf; the inline pass
/// must not override explicit choice.
#[test]
fn curated_interior_flip_pins_the_building_to_the_shelf() {
    use vellum_fe::core::layout_engine::overrides::group_anchor_key;
    use vellum_fe::core::layout_engine::{
        generate_layout_curated, LocationOverrides, SheetChoice,
    };
    use vellum_fe::core::mapdb::RoomTable;

    let mut rooms = load_rooms("the-atoll");
    let baseline = generate_layout(&mut rooms);
    let &pinned = baseline
        .inlined
        .first()
        .expect("atoll inlines at least one building");

    let lookup = RoomTable::new(&rooms);
    let anchor = group_anchor_key(&baseline.groups[pinned], &lookup);
    let mut curated = LocationOverrides::default();
    curated.sheets.insert(anchor, SheetChoice::Interior);

    let layout = generate_layout_curated(&mut rooms, &curated);
    assert!(
        !layout.inlined.contains(&pinned),
        "curated Interior flip must exempt the group from inlining"
    );
    assert!(layout.interiors.contains(&pinned));
}

/// Diagnostic for the deferred "edge-aware satellite placement" work: for
/// every connector-placed (satellite) group, compare where it actually landed
/// against the direction its connecting edges resolve to
/// (`direction_for_connection`: dirto → command text → reverse edge). Prints
/// per-zone agreement counts and the concrete disagreeing edges, so the
/// eventual scoring change can be designed against real cases.
/// Run with: cargo test --release --test layout_engine satellite_edge_audit -- --ignored --nocapture
#[test]
#[ignore]
fn satellite_edge_audit() {
    use vellum_fe::core::layout_engine::direction::{direction_for_connection, Dir};
    use vellum_fe::core::layout_engine::PackMethod;
    use vellum_fe::core::mapdb::RoomTable;

    // Octant index for planar directions, matching screen coordinates
    // (+y = south). Up/Down have no bearing on the sheet.
    fn octant(d: Dir) -> Option<i32> {
        Some(match d {
            Dir::East => 0,
            Dir::Southeast => 1,
            Dir::South => 2,
            Dir::Southwest => 3,
            Dir::West => 4,
            Dir::Northwest => 5,
            Dir::North => 6,
            Dir::Northeast => 7,
            Dir::Up | Dir::Down => return None,
        })
    }

    /// 8-sector bearing of the vector a→b; None for a zero vector.
    fn bearing_octant(a: Cell, b: Cell) -> Option<i32> {
        let dx = (b.x - a.x) as f64;
        let dy = (b.y - a.y) as f64;
        if dx == 0.0 && dy == 0.0 {
            return None;
        }
        let angle = dy.atan2(dx);
        Some(((angle / std::f64::consts::FRAC_PI_4).round() as i32).rem_euclid(8))
    }

    for (zone, file) in ZONES {
        let mut rooms = load_rooms(file);
        let layout = generate_layout(&mut rooms);
        let lookup = RoomTable::new(&rooms);

        // room id → group index, and group index → sheet membership.
        let mut group_of: HashMap<u32, usize> = HashMap::new();
        for group in &layout.groups {
            for &id in &group.room_ids {
                group_of.insert(id, group.index);
            }
        }
        let outdoor: HashSet<usize> = layout.outdoor.iter().copied().collect();

        let satellites = layout
            .groups
            .iter()
            .filter(|g| outdoor.contains(&g.index) && g.packing == Some(PackMethod::Connector))
            .count();
        let mut resolvable = 0usize;
        let mut agree = 0usize;
        let mut adjacent = 0usize;
        let mut disagreements: Vec<String> = Vec::new();

        for group in &layout.groups {
            if !outdoor.contains(&group.index) || group.packing != Some(PackMethod::Connector) {
                continue;
            }
            for &room_id in &group.room_ids {
                let Some(room) = lookup.get(room_id) else {
                    continue;
                };
                for &target in room.wayto.keys() {
                    let Some(&other_group) = group_of.get(&target) else {
                        continue;
                    };
                    // Only inter-group edges to another group on this sheet —
                    // cross-sheet coordinates don't compare.
                    if other_group == group.index || !outdoor.contains(&other_group) {
                        continue;
                    }
                    let Some(dir) = direction_for_connection(room, target, &lookup) else {
                        continue;
                    };
                    let Some(stated) = octant(dir) else {
                        continue;
                    };
                    let a = group.final_cell(room_id);
                    let b = layout.groups[other_group].final_cell(target);
                    let Some(actual) = bearing_octant(a, b) else {
                        continue;
                    };
                    resolvable += 1;
                    let diff = (stated - actual).rem_euclid(8).min((actual - stated).rem_euclid(8));
                    match diff {
                        0 => agree += 1,
                        1 => adjacent += 1,
                        _ => disagreements.push(format!(
                            "    {room_id} -> {target}: edge says {}, landed {} cells at bearing {} sectors off (cmd {:?})",
                            dir.name(),
                            chebyshev_dist(a, b),
                            diff,
                            room.wayto.get(&target).map(String::as_str).unwrap_or("?"),
                        )),
                    }
                }
            }
        }

        println!("== {zone} ==");
        println!(
            "  satellite groups: {satellites}; direction-resolvable connector edges: {resolvable}"
        );
        println!(
            "  exact: {agree}; within one sector: {adjacent}; disagree (>=2 sectors): {}",
            disagreements.len()
        );
        for line in &disagreements {
            println!("{line}");
        }

        // Second measure: edge occupancy. Connector lines that pass through
        // room cells they don't terminate at — the "short stretch crossings"
        // the spec lists as future work. Count both directions of the harm.
        let mut room_cells: HashMap<Cell, u32> = HashMap::new();
        for group in &layout.groups {
            if !outdoor.contains(&group.index) {
                continue;
            }
            for &id in &group.room_ids {
                room_cells.insert(group.final_cell(id), id);
            }
        }
        let mut segments = 0usize;
        let mut trampling = 0usize;
        let mut tramples: Vec<String> = Vec::new();
        let mut seen_pairs: HashSet<(u32, u32)> = HashSet::new();
        for group in &layout.groups {
            if !outdoor.contains(&group.index) {
                continue;
            }
            for &room_id in &group.room_ids {
                let Some(room) = lookup.get(room_id) else {
                    continue;
                };
                for &target in room.wayto.keys() {
                    let Some(&other_group) = group_of.get(&target) else {
                        continue;
                    };
                    if other_group == group.index || !outdoor.contains(&other_group) {
                        continue;
                    }
                    let pair = (room_id.min(target), room_id.max(target));
                    if !seen_pairs.insert(pair) {
                        continue;
                    }
                    let a = group.final_cell(room_id);
                    let b = layout.groups[other_group].final_cell(target);
                    if chebyshev_dist(a, b) > 30 {
                        continue; // beyond the commit cap; never drawn
                    }
                    segments += 1;
                    // Supercover walk of the segment, endpoints excluded.
                    let hits = cells_on_segment(a, b)
                        .into_iter()
                        .filter(|c| *c != a && *c != b)
                        .filter_map(|c| room_cells.get(&c).copied())
                        .collect::<Vec<_>>();
                    if !hits.is_empty() {
                        trampling += 1;
                        if tramples.len() < 8 {
                            tramples.push(format!(
                                "    line {room_id} -> {target} ({} cells) passes through rooms {:?}",
                                chebyshev_dist(a, b),
                                &hits[..hits.len().min(5)],
                            ));
                        }
                    }
                }
            }
        }
        println!(
            "  drawn connector segments: {segments}; passing through other rooms: {trampling}"
        );
        for line in &tramples {
            println!("{line}");
        }
    }
}

/// Cells a segment passes through (integer supercover, endpoint-inclusive):
/// samples the line densely and collects the rounded cells.
fn cells_on_segment(a: Cell, b: Cell) -> Vec<Cell> {
    let steps = chebyshev_dist(a, b).max(1) * 2;
    let mut out: Vec<Cell> = Vec::new();
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let x = a.x as f64 + (b.x - a.x) as f64 * t;
        let y = a.y as f64 + (b.y - a.y) as f64 * t;
        let cell = Cell {
            x: x.round() as i32,
            y: y.round() as i32,
        };
        if out.last() != Some(&cell) {
            out.push(cell);
        }
    }
    out
}

fn chebyshev_dist(a: Cell, b: Cell) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// Diagnostic: inspect how specific rooms were placed (group, cell, packing,
/// violations). Zone and room ids via env: AUDIT_ZONE (file stem, default
/// mist-harbor) and AUDIT_ROOMS (comma-separated ids).
/// Run with: cargo test --test layout_engine room_placement_audit -- --ignored --nocapture
#[test]
#[ignore]
fn room_placement_audit() {
    let zone = std::env::var("AUDIT_ZONE").unwrap_or_else(|_| "mist-harbor".into());
    let ids: Vec<u32> = std::env::var("AUDIT_ROOMS")
        .unwrap_or_else(|_| "12271,12272".into())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let mut rooms = load_rooms(&zone);
    // Simulate the live playershop location split (MapDb::from_json strips
    // tagged rooms out of the town's room set).
    if std::env::var("AUDIT_EXCLUDE_PLAYERSHOPS").is_ok() {
        rooms.retain(|r| !r.tags.iter().any(|t| t == "meta:playershop"));
    }
    let layout = generate_layout(&mut rooms);
    for group in &layout.groups {
        let hits: Vec<u32> = group
            .room_ids
            .iter()
            .copied()
            .filter(|id| ids.contains(id))
            .collect();
        if hits.is_empty() {
            continue;
        }
        println!(
            "group {} ({} rooms, packing {:?}, offset {:?}):",
            group.index,
            group.room_ids.len(),
            group.packing,
            group.base_offset
        );
        for id in hits {
            println!("  room {id} at {:?}", group.final_cell(id));
        }
        for v in &group.violations {
            if ids.contains(&v.from) || ids.contains(&v.to) {
                println!("  VIOLATION: {v:?}");
            }
        }
        println!("  total violations in group: {}", group.violations.len());
        if std::env::var("AUDIT_FULL_GROUP").is_ok() {
            let table = mapdb::RoomTable::new(&rooms);
            // room_ids is BFS placement order - print it as such, so cell
            // conflicts can be reconstructed step by step.
            for (order, &id) in group.room_ids.iter().enumerate() {
                let title = table
                    .get(id)
                    .and_then(|r| r.title.first().cloned())
                    .unwrap_or_default();
                println!("    #{order:<3} {id} {:?} {title}", group.final_cell(id));
            }
        }
    }
}

/// Diagnostic: print every zone's stats and generation time next to the
/// reference fixture.
/// Run with: cargo test --release --test layout_engine -- --ignored --nocapture
#[test]
#[ignore]
fn print_all_zone_stats() {
    for (zone, file) in ZONES {
        let expected = fixture_stats(zone);
        let mut rooms = load_rooms(file);
        let t0 = std::time::Instant::now();
        let layout = generate_layout(&mut rooms);
        let ms = t0.elapsed().as_millis();
        let stats = LayoutStats::compute(&layout, &rooms);
        assert_hard_invariants(&layout, &rooms);
        println!("== {zone} ({ms}ms) ==");
        println!("  ours:     {stats:?}");
        println!("  expected: {expected:?}");
    }
}
