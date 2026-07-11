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

use vellum_fe::core::layout_engine::{generate_layout, mapdb, Cell, Layout, LayoutStats, Room};

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

fn run_zone(file: &str) -> (Layout, LayoutStats, Vec<Room>) {
    let mut rooms = load_rooms(file);
    let layout = generate_layout(&mut rooms);
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
