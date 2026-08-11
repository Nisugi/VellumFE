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
use vellum_fe::core::travel::executor::{classify_edge, EdgeCrossing};

/// Corpus report over the real mapdb. Post-routing-split (go2 plan P1),
/// two DISTINCT numbers are tracked:
///
/// - **Graph coverage** — how many edges the router will PLAN through. This
///   depends only on `timeto` resolving to a number (Lich parity): a plain
///   edge with a cost, or a proc edge whose `timeto` resolves. It does NOT
///   depend on whether we can transpile the `wayto`.
/// - **Execution coverage** — of the proc edges, how many the transpiler can
///   actually WALK. An edge can be in the graph (routable) yet not natively
///   crossable (execution falls back / bans + re-paths).
///
/// Every scripted edge goes through the transpiler — zero panics required —
/// and the measured idioms must stay covered.
/// Collapse a StringProc to its IDIOM, so instances of the same shape cluster
/// together: whitespace normalized, numbers to `N`, quoted strings to `'S'`,
/// truncated to 160 chars.
///
/// Deliberately identical to the key Lich's converter uses for its own
/// residue report, so the two reports can be diffed edge-family by
/// edge-family — a recognizer either side writes is a hint for the other.
fn residue_key(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut last_was_space = false;
    while let Some(c) = chars.next() {
        match c {
            // Quoted string -> 'S' (single and double quotes alike).
            '\'' | '"' => {
                let quote = c;
                out.push_str("'S'");
                while let Some(n) = chars.next() {
                    if n == '\\' {
                        chars.next();
                    } else if n == quote {
                        break;
                    }
                }
                last_was_space = false;
            }
            // Run of digits -> N.
            d if d.is_ascii_digit() => {
                out.push('N');
                while chars.peek().is_some_and(|n| n.is_ascii_digit() || *n == '.') {
                    chars.next();
                }
                last_was_space = false;
            }
            w if w.is_whitespace() => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            other => {
                out.push(other);
                last_was_space = false;
            }
        }
    }
    out.chars().take(160).collect()
}

#[test]
#[ignore]
fn real_mapdb_coverage() {
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
    // Residue: untranspiled procs clustered by normalized shape, so the
    // report names IDIOM FAMILIES with counts rather than listing thousands
    // of near-identical lines. Value -> (count, one sample edge).
    let mut residue: std::collections::HashMap<String, (usize, String)> =
        std::collections::HashMap::new();
    // How each scripted edge gets crossed, so the report shows what carries
    // the load rather than implying the transpiler does all of it.
    let mut by_strategy: std::collections::HashMap<EdgeCrossing, usize> =
        std::collections::HashMap::new();
    // Graph reachability: an edge whose timeto resolves to a number.
    let mut graph_routable = 0usize;
    let mut ids: Vec<u32> = Vec::new();
    for location in db.locations().map(str::to_owned).collect::<Vec<_>>() {
        for room in db.rooms(&location).unwrap_or(&[]) {
            ids.push(room.id);
        }
    }
    for id in ids {
        let room = db.room(id).expect("indexed room");
        for (dest, command) in &room.wayto {
            // Execution coverage: can the EXECUTOR cross this edge? Not "does
            // transpile() return Some" — Confluence, curated mazes, day passes
            // and overrides are dispatched by dedicated strategies before the
            // transpiler is consulted, and counting them as residue misdirects
            // recognizer work at the scale of thousands of edges.
            if is_proc_command(command) {
                let how = classify_edge(&db, room.id, *dest, command);
                *by_strategy.entry(how).or_insert(0usize) += 1;
                if how.is_crossable() {
                    proc_supported += 1;
                } else {
                    proc_unsupported += 1;
                    let entry = residue
                        .entry(residue_key(command))
                        .or_insert_with(|| (0, format!("{}:{dest} {command}", room.id)));
                    entry.0 += 1;
                }
            } else {
                plain += 1;
            }
            // Graph coverage: does the timeto resolve? A plain numeric cost,
            // or a proc timeto that resolves, makes the edge routable — no
            // matter whether the wayto is interpretable.
            let routable = match room.timeto.get(dest) {
                Some(TimeTo::Seconds(s)) => *s >= 0.0,
                Some(TimeTo::Proc(_)) => transpile::resolve_timeto(&db, room, *dest).is_some(),
                None => false,
            };
            if routable {
                graph_routable += 1;
            }
        }
    }
    let total_procs = proc_supported + proc_unsupported;
    let total_edges = plain + total_procs;
    println!(
        "wayto edges: {plain} plain, {total_procs} scripted ({proc_supported} transpiled = {:.1}%)",
        proc_supported as f64 / total_procs.max(1) as f64 * 100.0
    );
    println!(
        "GRAPH coverage (routable by timeto): {graph_routable}/{total_edges} ({:.1}%)",
        graph_routable as f64 / total_edges.max(1) as f64 * 100.0
    );
    println!(
        "EXECUTION coverage (proc edges we can walk): {proc_supported}/{total_procs} ({:.1}%)",
        proc_supported as f64 / total_procs.max(1) as f64 * 100.0
    );
    // Which mechanism carries each scripted edge. The transpiler is only one
    // of several; a report that hid the strategies made Confluence look like
    // 57% of unsolved work when it is in fact fully handled.
    let mut strategies: Vec<(&EdgeCrossing, &usize)> = by_strategy.iter().collect();
    strategies.sort_by(|a, b| b.1.cmp(a.1));
    println!("  by mechanism:");
    for (how, count) in strategies {
        println!(
            "    {how:?}: {count} ({:.1}%)",
            *count as f64 / total_procs.max(1) as f64 * 100.0
        );
    }
    // Residue report: the top idiom families we can't yet cross, biggest
    // first. This is the work queue — write a recognizer for the top cluster,
    // re-run, repeat. Counts, not raw lines, so the tail stays readable.
    let mut clusters: Vec<(&String, &(usize, String))> = residue.iter().collect();
    clusters.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    let covered_by_top: usize = clusters.iter().take(25).map(|(_, (n, _))| n).sum();
    println!(
        "\nRESIDUE: {} untranspiled edges in {} idiom families; \
         the top 25 families are {covered_by_top} edges ({:.1}% of residue)",
        proc_unsupported,
        clusters.len(),
        covered_by_top as f64 / proc_unsupported.max(1) as f64 * 100.0
    );
    for (i, (key, (count, sample))) in clusters.iter().take(25).enumerate() {
        println!("\n{:>3}. {count}x  {key}", i + 1);
        println!("     e.g. {sample}");
    }

    // The measured corpus shapes must stay covered; dropping below this
    // after a mapdb rebuild means new idioms appeared — extend the
    // transpiler.
    assert!(
        proc_supported as f64 / total_procs.max(1) as f64 > 0.20,
        "execution (transpiler) coverage regressed: {proc_supported}/{total_procs}"
    );
    // Graph coverage should be high — most edges have a numeric timeto.
    assert!(
        graph_routable as f64 / total_edges.max(1) as f64 > 0.80,
        "graph coverage regressed: {graph_routable}/{total_edges}"
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
