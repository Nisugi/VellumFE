//! Layout cache behavior: content-hash stability, disk roundtrip fidelity,
//! hit/miss outcomes, version invalidation, and stale-entry pruning.

use std::path::PathBuf;

use vellum_fe::core::layout_engine::{
    generate_layout, mapdb, rooms_content_hash, CacheOutcome, LayoutCache, Room,
};

fn load_rooms(file: &str) -> Vec<Room> {
    let path = format!(
        "{}/tests/fixtures/layout/{file}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let json = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    mapdb::rooms_from_array(&json).expect("valid room fixture JSON")
}

/// Per-test scratch directory, cleaned up on drop.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "vellum-layout-cache-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        ScratchDir(dir)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn content_hash_ignores_room_order_but_sees_data_changes() {
    let rooms = load_rooms("moonsedge");
    let baseline = rooms_content_hash(&rooms);

    // Reversed input order hashes identically (canonical ascending-id walk).
    let mut reversed = rooms.clone();
    reversed.reverse();
    assert_eq!(rooms_content_hash(&reversed), baseline);

    // Any layout-relevant change moves the hash.
    let mut retitled = rooms.clone();
    retitled[0].title = vec!["[Somewhere Else]".into()];
    assert_ne!(rooms_content_hash(&retitled), baseline);

    let mut rewired = rooms.clone();
    let (&target, _) = rewired[0].wayto.iter().next().expect("room has exits");
    rewired[0].wayto.insert(target, "go the other way".into());
    assert_ne!(rooms_content_hash(&rewired), baseline);
}

#[test]
fn store_then_load_roundtrips_the_layout_exactly() {
    let scratch = ScratchDir::new("roundtrip");
    let cache = LayoutCache::new(scratch.0.clone());

    let rooms = load_rooms("moonsedge");
    let mut owned = rooms.clone();
    let layout = generate_layout(&mut owned);
    let hash = rooms_content_hash(&rooms);

    cache.store("Moonsedge", hash, &layout).expect("store");
    let loaded = cache.load("Moonsedge", hash).expect("load hit");
    assert_eq!(loaded, layout, "cache roundtrip must be lossless");

    // Wrong hash or location is a miss, not an error.
    assert!(cache.load("Moonsedge", hash ^ 1).is_none());
    assert!(cache.load("the Atoll", hash).is_none());
}

#[test]
fn get_or_generate_generates_once_then_hits() {
    let scratch = ScratchDir::new("hit-miss");
    let cache = LayoutCache::new(scratch.0.clone());
    let rooms = load_rooms("the-atoll");

    let (first, first_outcome) = cache.get_or_generate("the Atoll", &rooms);
    assert_eq!(first_outcome, CacheOutcome::Generated);

    let (second, second_outcome) = cache.get_or_generate("the Atoll", &rooms);
    assert_eq!(second_outcome, CacheOutcome::Hit);
    assert_eq!(second, first, "hit must equal the generated layout");
}

#[test]
fn corrupt_or_stale_entries_regenerate() {
    let scratch = ScratchDir::new("invalidation");
    let cache = LayoutCache::new(scratch.0.clone());
    let rooms = load_rooms("moonsedge");

    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms);
    assert_eq!(outcome, CacheOutcome::Generated);

    let entry = std::fs::read_dir(&scratch.0)
        .expect("cache dir")
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .expect("one cache entry");

    // A future engine version must not serve yesterday's layout.
    let json = std::fs::read_to_string(&entry).unwrap();
    std::fs::write(
        &entry,
        json.replace("\"engine_version\":1", "\"engine_version\":0"),
    )
    .unwrap();
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms);
    assert_eq!(outcome, CacheOutcome::Generated, "version mismatch → miss");

    // Truncated/corrupt JSON is a miss, not a panic.
    std::fs::write(&entry, "{ not json").unwrap();
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms);
    assert_eq!(outcome, CacheOutcome::Generated, "corrupt entry → miss");

    // And the rewrite healed it.
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms);
    assert_eq!(outcome, CacheOutcome::Hit);
}

#[test]
fn new_mapdb_build_prunes_the_old_entry() {
    let scratch = ScratchDir::new("prune");
    let cache = LayoutCache::new(scratch.0.clone());

    let rooms = load_rooms("moonsedge");
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms);
    assert_eq!(outcome, CacheOutcome::Generated);

    // Same location, changed data — as after a mapdb update.
    let mut updated = rooms.clone();
    updated[0].title = vec!["[Moonsedge, Renamed Plaza]".into()];
    let (_, outcome) = cache.get_or_generate("Moonsedge", &updated);
    assert_eq!(outcome, CacheOutcome::Generated);

    let entries: Vec<_> = std::fs::read_dir(&scratch.0)
        .expect("cache dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "old-hash entry for the same location should be pruned: {entries:?}"
    );

    // The survivor serves the updated data.
    let (_, outcome) = cache.get_or_generate("Moonsedge", &updated);
    assert_eq!(outcome, CacheOutcome::Hit);
    // ...and the original data regenerates rather than mis-hitting.
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms);
    assert_eq!(outcome, CacheOutcome::Generated);
}

/// End-to-end against a real Lich install: discover the newest mapdb, parse
/// and index it, then generate + cache a layout for every location that has
/// a fixture. Machine-specific, so it only runs when pointed at a Lich
/// per-game data dir:
///   VELLUM_LICH_GAME_DIR="C:/path/to/lich/data/GSIV" \
///     cargo test --release --test layout_cache -- --ignored --nocapture
#[test]
#[ignore]
fn real_lich_mapdb_end_to_end() {
    let Ok(game_dir) = std::env::var("VELLUM_LICH_GAME_DIR") else {
        eprintln!("VELLUM_LICH_GAME_DIR not set; skipping");
        return;
    };
    let path = vellum_fe::core::layout_engine::find_latest_mapdb(std::path::Path::new(&game_dir))
        .expect("a map-<timestamp>.json in the game data dir");
    println!("newest mapdb: {}", path.display());

    let t0 = std::time::Instant::now();
    let db = vellum_fe::core::layout_engine::MapDb::load(&path).expect("parse mapdb");
    println!(
        "parsed + indexed in {}ms ({} locations)",
        t0.elapsed().as_millis(),
        db.locations().count()
    );

    let scratch = ScratchDir::new("real-mapdb");
    let cache = LayoutCache::new(scratch.0.clone());
    for location in ["Moonsedge", "Wehnimer's Landing"] {
        let rooms = db.rooms(location).expect("location exists");
        let t0 = std::time::Instant::now();
        let (layout, outcome) = cache.get_or_generate(location, rooms);
        let cold = t0.elapsed();
        assert_eq!(outcome, CacheOutcome::Generated);
        let t0 = std::time::Instant::now();
        let (_, outcome) = cache.get_or_generate(location, rooms);
        let warm = t0.elapsed();
        assert_eq!(outcome, CacheOutcome::Hit);
        println!(
            "{location}: {} rooms, {} groups — cold {}ms, warm {}ms",
            rooms.len(),
            layout.groups.len(),
            cold.as_millis(),
            warm.as_millis()
        );
    }
}
