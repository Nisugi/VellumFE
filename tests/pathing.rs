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

use vellum_fe::core::mapdb::{find_latest_mapdb, MapDb};
use vellum_fe::core::pathing::{estimate_time, find_nearest_by_tag, path_to};

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
