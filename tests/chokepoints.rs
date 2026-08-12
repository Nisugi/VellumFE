//! What does the untranspiled residue actually COST a player?
//!
//! Coverage percentages say how many scripted edges we can walk. They do not
//! say whether a user can still get where they are going — most rooms have
//! several ways in, so an uncrossable edge is usually a slower route rather
//! than a wall. This measures the difference, because it is the number that
//! decides whether the remaining residue blocks shipping.
//!
//! The scenario is the worst realistic one: a DIRECT-CONNECT user with no
//! Lich fallback, so every edge we cannot natively cross is simply gone.
//!
//! ```text
//! VELLUM_LICH_GAME_DIR=C:/Gemstone/Lich5/data/GSIV \
//!   cargo test --test chokepoints -- --ignored --nocapture
//! ```

use std::collections::{HashMap, HashSet};
use vellum_fe::core::mapdb::{is_proc_command, MapDb};
use vellum_fe::core::pathing::dijkstra::{dijkstra, dijkstra_filtered, PathTarget};
use vellum_fe::core::travel::executor::classify_edge;

/// Rooms reachable from `source`, with every uncrossable edge banned.
fn reachable(db: &MapDb, source: u32, banned: &HashSet<(u32, u32)>) -> HashSet<u32> {
    let search = dijkstra_filtered(db, source, None, &|from, to| !banned.contains(&(from, to)));
    search.distance.keys().copied().collect()
}

#[test]
#[ignore]
fn residue_costs_reachability() {
    let Ok(game_dir) = std::env::var("VELLUM_LICH_GAME_DIR") else {
        eprintln!("VELLUM_LICH_GAME_DIR not set; skipping");
        return;
    };
    let path = vellum_fe::core::mapdb::find_latest_mapdb(std::path::Path::new(&game_dir))
        .expect("a map-<timestamp>.json in the game data dir");
    let db = MapDb::load(&path).expect("parse mapdb");

    // Every edge the executor cannot cross natively.
    let mut banned: HashSet<(u32, u32)> = HashSet::new();
    let mut ids: Vec<u32> = Vec::new();
    for location in db.locations().map(str::to_owned).collect::<Vec<_>>() {
        for room in db.rooms(&location).unwrap_or(&[]) {
            ids.push(room.id);
        }
    }
    for &id in &ids {
        let room = db.room(id).expect("indexed room");
        for (dest, command) in &room.wayto {
            if is_proc_command(command)
                && !classify_edge(&db, room.id, *dest, command).is_crossable()
            {
                banned.insert((room.id, *dest));
            }
        }
    }
    println!("banned {} uncrossable edges", banned.len());

    // Anchor at the best-connected room in the mapdb rather than whichever id
    // the location index happens to yield first — that one turned out to be an
    // isolated room reaching exactly itself, which makes every delta zero and
    // the measurement meaningless.
    // Sampled every 500th room: one full dijkstra per candidate, and the
    // best-connected component is large enough that a sparse sample lands in
    // it with certainty.
    let source = *ids
        .iter()
        .step_by(500)
        .max_by_key(|&&id| dijkstra(&db, id, None).distance.len())
        .expect("a non-empty mapdb");
    println!(
        "anchor room {source} ({})",
        db.room(source)
            .and_then(|r| r.location.as_deref())
            .unwrap_or("?")
    );
    let full = reachable(&db, source, &HashSet::new());
    let degraded = reachable(&db, source, &banned);
    let lost: Vec<u32> = full.difference(&degraded).copied().collect();

    println!(
        "reachable from {source}: {} rooms normally, {} with residue banned",
        full.len(),
        degraded.len()
    );
    println!(
        "STRANDED: {} rooms ({:.2}% of reachable) become unreachable",
        lost.len(),
        lost.len() as f64 / full.len().max(1) as f64 * 100.0
    );

    // Where the losses are, so the tail can be judged by CONTENT rather than
    // by count -- 200 rooms of one remote area is a different problem from
    // 200 rooms scattered through every town.
    let mut by_area: HashMap<String, usize> = HashMap::new();
    for &room in &lost {
        let area = db
            .room(room)
            .and_then(|r| r.location.clone())
            .unwrap_or_else(|| "<unknown>".into());
        *by_area.entry(area).or_default() += 1;
    }
    let mut areas: Vec<(String, usize)> = by_area.into_iter().collect();
    areas.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!("\nstranded rooms by area:");
    for (area, count) in areas.iter().take(25) {
        println!("  {count:5}  {area}");
    }

    // Of the rooms still reachable, how much SLOWER did they get? A residue
    // edge that only adds detour time is a nuisance, not a blocker.
    let before = dijkstra(&db, source, None);
    let after = dijkstra_filtered(&db, source, None, &|from, to| !banned.contains(&(from, to)));
    let mut worse = 0usize;
    let mut worst: Vec<(u32, f64, f64)> = Vec::new();
    for (&room, &was) in &before.distance {
        if let Some(&now) = after.distance.get(&room) {
            if now > was + 1.0 {
                worse += 1;
                worst.push((room, was, now));
            }
        }
    }
    worst.sort_by(|a, b| (b.2 - b.1).total_cmp(&(a.2 - a.1)));
    println!(
        "\n{worse} still-reachable rooms take a detour (>1s slower); worst 10:"
    );
    for (room, was, now) in worst.iter().take(10) {
        println!(
            "  room {room:>7}  {was:>7.1}s -> {now:>7.1}s  (+{:.1}s)  {}",
            now - was,
            db.room(*room)
                .and_then(|r| r.location.as_deref())
                .unwrap_or("?")
        );
    }

    // Which single edges carry the loss. Un-ban one residue edge at a time and
    // count how many stranded rooms it alone restores: a chokepoint into a
    // city recovers thousands, while a redundant edge recovers none. This is
    // the work queue, ordered by what it actually buys a player.
    let lost_set: HashSet<u32> = lost.iter().copied().collect();
    let mut payoff: Vec<(usize, u32, u32)> = Vec::new();
    for &(from, to) in &banned {
        // Only an edge reachable in the degraded graph can restore anything.
        if !degraded.contains(&from) {
            continue;
        }
        let mut relaxed = banned.clone();
        relaxed.remove(&(from, to));
        let restored = reachable(&db, source, &relaxed)
            .intersection(&lost_set)
            .count();
        if restored > 0 {
            payoff.push((restored, from, to));
        }
    }
    payoff.sort_by(|a, b| b.0.cmp(&a.0));
    println!(
        "\n{} residue edges restore rooms on their own; top 20:",
        payoff.len()
    );
    for (restored, from, to) in payoff.iter().take(20) {
        let body = db
            .room(*from)
            .and_then(|r| r.wayto.get(to).cloned())
            .unwrap_or_default();
        println!(
            "  +{restored:>5} rooms  {from} -> {to}  [{}]  {}",
            db.room(*to)
                .and_then(|r| r.location.as_deref())
                .unwrap_or("?"),
            body.replace('\n', " ").chars().take(90).collect::<String>()
        );
    }

    // The full CUT: every banned edge leading from reachable ground into the
    // lost region — including ones that restore nothing alone because a
    // parallel or serial partner is also banned. When the single-edge payoff
    // list goes quiet while thousands stay stranded, this is where the
    // multi-edge cuts hide (two gates in series both being residue).
    let mut cut: Vec<(u32, u32)> = banned
        .iter()
        .filter(|(from, to)| degraded.contains(from) && lost_set.contains(to))
        .copied()
        .collect();
    cut.sort();
    println!("\nfull cut into the lost region: {} edges", cut.len());
    for (from, to) in &cut {
        let body = db
            .room(*from)
            .and_then(|r| r.wayto.get(to).cloned())
            .unwrap_or_default();
        println!(
            "  {from} -> {to}  [{}]  {}",
            db.room(*to)
                .and_then(|r| r.location.as_deref())
                .unwrap_or("?"),
            body.replace('\n', " ").chars().take(110).collect::<String>()
        );
    }

    // Peel the onion: repeatedly un-ban the current cut and recompute, so
    // SERIAL gates show as successive waves instead of one layer per fix.
    // Each wave prints what crossing it would unlock — the roadmap for the
    // remaining hard edges.
    let mut peeled = banned.clone();
    let mut wave = 0;
    loop {
        wave += 1;
        let now = reachable(&db, source, &peeled);
        let cut_now: Vec<(u32, u32)> = peeled
            .iter()
            .filter(|(f, t)| now.contains(f) && !now.contains(t))
            .copied()
            .collect();
        if cut_now.is_empty() || wave > 12 {
            println!("\nafter wave {wave}: {} rooms still unreachable, cut empty or depth cap", full.len() - now.len());
            break;
        }
        let before = now.len();
        for e in &cut_now {
            peeled.remove(e);
        }
        let after = reachable(&db, source, &peeled).len();
        println!("\nwave {wave}: crossing {} edges unlocks {} rooms:", cut_now.len(), after - before);
        let mut cut_now = cut_now;
        cut_now.sort();
        for (f, t) in &cut_now {
            println!(
                "    {f} -> {t}  [{}]",
                db.room(*t)
                    .and_then(|r| r.location.as_deref())
                    .unwrap_or("?")
            );
        }
    }

    // Sanity: banning edges can only ever remove reachability, never add it.
    assert!(
        degraded.len() <= full.len(),
        "banning edges increased reachability"
    );
}

/// The same question asked per-town: can a user still reach the places trips
/// actually target (bank, and the town's other rooms) without Lich?
#[test]
#[ignore]
fn towns_stay_internally_connected() {
    let Ok(game_dir) = std::env::var("VELLUM_LICH_GAME_DIR") else {
        eprintln!("VELLUM_LICH_GAME_DIR not set; skipping");
        return;
    };
    let path = vellum_fe::core::mapdb::find_latest_mapdb(std::path::Path::new(&game_dir))
        .expect("a map-<timestamp>.json in the game data dir");
    let db = MapDb::load(&path).expect("parse mapdb");

    let mut banned: HashSet<(u32, u32)> = HashSet::new();
    for location in db.locations().map(str::to_owned).collect::<Vec<_>>() {
        for room in db.rooms(&location).unwrap_or(&[]) {
            for (dest, command) in &room.wayto {
                if is_proc_command(command)
                    && !classify_edge(&db, room.id, *dest, command).is_crossable()
                {
                    banned.insert((room.id, *dest));
                }
            }
        }
    }

    for town in [
        "Wehnimer's Landing",
        "Icemule Trace",
        "Ta'Illistim",
        "Solhaven",
        "Kharam Dzu",
        "Ta'Vaalor",
        "Zul Logoth",
    ] {
        let Some(rooms) = db.rooms(town) else {
            println!("{town}: not in this mapdb");
            continue;
        };
        let source = rooms[0].id;
        let ids: HashSet<u32> = rooms.iter().map(|r| r.id).collect();
        let full = reachable(&db, source, &HashSet::new());
        let degraded = reachable(&db, source, &banned);
        let in_town = |set: &HashSet<u32>| set.intersection(&ids).count();
        let (was, now) = (in_town(&full), in_town(&degraded));
        println!(
            "{town:<20} {now:>4}/{was:<4} own rooms reachable without Lich{}",
            if now < was {
                format!("  ({} LOST)", was - now)
            } else {
                String::new()
            }
        );
    }
}
