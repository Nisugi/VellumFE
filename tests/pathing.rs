//! Pathfinding against a real Lich mapdb — env-gated like the layout
//! engine's real-install test. Run with:
//!
//! ```text
//! VELLUM_LICH_GAME_DIR=C:/Gemstone/Lich5/data/GSIV cargo test --test pathing -- --ignored --nocapture
//! ```
//!
//! No hardcoded room ids: routes are derived from tags so the test survives
//! mapdb rebuilds. Once the Cartographer pipeline is live upstream, fixture
//! routes captured from Lich's `Map.findpath` can pin exact parity.

use vellum_fe::core::mapdb::{find_latest_mapdb, is_proc_command, MapDb, TimeTo};
use vellum_fe::core::pathing::{estimate_time, find_nearest_by_tag, path_to, transpile};

/// Transpiler corpus report (go2 plan phase 5): every scripted edge in the
/// real mapdb goes through the transpiler — zero panics required, and the
/// measured idioms must stay covered. Prints the breakdown.
#[test]
#[ignore]
fn real_mapdb_transpiler_coverage() {
    let Ok(game_dir) = std::env::var("VELLUM_LICH_GAME_DIR") else {
        eprintln!("VELLUM_LICH_GAME_DIR not set; skipping");
        return;
    };
    let path = find_latest_mapdb(std::path::Path::new(&game_dir))
        .expect("a map-<timestamp>.json in the game data dir");
    let db = MapDb::load(&path).expect("parse mapdb");

    let mut plain = 0usize;
    let mut proc_supported = 0usize;
    let mut proc_unsupported = 0usize;
    let mut timeto_procs = 0usize;
    let mut timeto_resolved = 0usize;
    let mut ids: Vec<u32> = Vec::new();
    for location in db.locations().map(str::to_owned).collect::<Vec<_>>() {
        for room in db.rooms(&location).unwrap_or(&[]) {
            ids.push(room.id);
        }
    }
    for id in ids {
        let room = db.room(id).expect("indexed room");
        for (dest, command) in &room.wayto {
            if is_proc_command(command) {
                if transpile::transpile(command).is_some() {
                    proc_supported += 1;
                } else {
                    proc_unsupported += 1;
                }
            } else {
                plain += 1;
            }
            if matches!(room.timeto.get(dest), Some(TimeTo::Proc(_))) {
                timeto_procs += 1;
                if transpile::resolve_timeto(&db, room, *dest).is_some() {
                    timeto_resolved += 1;
                }
            }
        }
    }
    let total_procs = proc_supported + proc_unsupported;
    println!(
        "wayto edges: {plain} plain, {total_procs} scripted ({proc_supported} transpiled = {:.1}%)",
        proc_supported as f64 / total_procs.max(1) as f64 * 100.0
    );
    println!(
        "timeto procs: {timeto_procs} ({timeto_resolved} resolved = {:.1}%)",
        timeto_resolved as f64 / timeto_procs.max(1) as f64 * 100.0
    );
    let routable = plain + proc_supported;
    println!(
        "graph coverage: {routable}/{} edges walkable in principle ({:.1}%)",
        plain + total_procs,
        routable as f64 / (plain + total_procs).max(1) as f64 * 100.0
    );
    // The measured corpus shapes must stay covered; dropping below this
    // after a mapdb rebuild means new idioms appeared — extend the
    // transpiler.
    assert!(
        proc_supported as f64 / total_procs.max(1) as f64 > 0.20,
        "transpiler coverage regressed: {proc_supported}/{total_procs}"
    );
}

#[test]
#[ignore]
fn real_mapdb_routes_to_the_nearest_bank() {
    let Ok(game_dir) = std::env::var("VELLUM_LICH_GAME_DIR") else {
        eprintln!("VELLUM_LICH_GAME_DIR not set; skipping");
        return;
    };
    let path = find_latest_mapdb(std::path::Path::new(&game_dir))
        .expect("a map-<timestamp>.json in the game data dir");
    let t0 = std::time::Instant::now();
    let db = MapDb::load(&path).expect("parse mapdb");
    println!(
        "parsed {} rooms in {}ms",
        db.room_count(),
        t0.elapsed().as_millis()
    );

    // From the first mappable room of each big town, walk to the nearest
    // bank and verify every hop is a real, numerically-costed wayto edge.
    for town in ["Wehnimer's Landing", "Icemule Trace", "Ta'Illistim"] {
        let Some(rooms) = db.rooms(town) else {
            eprintln!("{town}: not in this mapdb, skipping");
            continue;
        };
        let source = rooms[0].id;
        let t0 = std::time::Instant::now();
        let Some(bank) = find_nearest_by_tag(&db, source, "bank") else {
            panic!("{town}: no reachable bank from room {source}");
        };
        let route = path_to(&db, source, bank);
        let elapsed = t0.elapsed();
        let Some(route) = route else {
            assert_eq!(source, bank, "{town}: nearest bank must be reachable");
            continue;
        };
        assert_eq!(*route.last().unwrap(), bank);
        let mut previous = source;
        for &step in &route {
            let room = db.room(previous).expect("room on route");
            assert!(
                room.wayto.contains_key(&step),
                "{town}: {previous} → {step} is not a wayto edge"
            );
            assert!(
                room.timeto.contains_key(&step),
                "{town}: {previous} → {step} has no timeto cost"
            );
            previous = step;
        }
        let mut timed = vec![source];
        timed.extend(&route);
        println!(
            "{town}: room {source} → bank {bank}: {} rooms, est {:.1}s, searched in {}ms",
            route.len(),
            estimate_time(&db, &timed),
            elapsed.as_millis()
        );
    }
}
