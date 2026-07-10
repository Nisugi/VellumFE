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

    cache.store("Moonsedge", hash, 0, &layout).expect("store");
    let loaded = cache.load("Moonsedge", hash, 0).expect("load hit");
    assert_eq!(loaded, layout, "cache roundtrip must be lossless");

    // Wrong hash or location is a miss, not an error.
    assert!(cache.load("Moonsedge", hash ^ 1, 0).is_none());
    assert!(cache.load("the Atoll", hash, 0).is_none());
}

#[test]
fn get_or_generate_generates_once_then_hits() {
    let scratch = ScratchDir::new("hit-miss");
    let cache = LayoutCache::new(scratch.0.clone());
    let rooms = load_rooms("the-atoll");

    let (first, first_outcome) = cache.get_or_generate("the Atoll", &rooms, &Default::default());
    assert_eq!(first_outcome, CacheOutcome::Generated);

    let (second, second_outcome) = cache.get_or_generate("the Atoll", &rooms, &Default::default());
    assert_eq!(second_outcome, CacheOutcome::Hit);
    assert_eq!(second, first, "hit must equal the generated layout");
}

#[test]
fn corrupt_or_stale_entries_regenerate() {
    let scratch = ScratchDir::new("invalidation");
    let cache = LayoutCache::new(scratch.0.clone());
    let rooms = load_rooms("moonsedge");

    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms, &Default::default());
    assert_eq!(outcome, CacheOutcome::Generated);

    let entry = std::fs::read_dir(&scratch.0)
        .expect("cache dir")
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .expect("one cache entry");

    // A future engine version must not serve yesterday's layout.
    let current = format!(
        "\"engine_version\":{}",
        vellum_fe::core::layout_engine::cache::ENGINE_VERSION
    );
    let json = std::fs::read_to_string(&entry).unwrap();
    assert!(json.contains(&current), "test setup: version marker present");
    std::fs::write(&entry, json.replace(&current, "\"engine_version\":0")).unwrap();
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms, &Default::default());
    assert_eq!(outcome, CacheOutcome::Generated, "version mismatch → miss");

    // Truncated/corrupt JSON is a miss, not a panic.
    std::fs::write(&entry, "{ not json").unwrap();
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms, &Default::default());
    assert_eq!(outcome, CacheOutcome::Generated, "corrupt entry → miss");

    // And the rewrite healed it.
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms, &Default::default());
    assert_eq!(outcome, CacheOutcome::Hit);
}

#[test]
fn new_mapdb_build_prunes_the_old_entry() {
    let scratch = ScratchDir::new("prune");
    let cache = LayoutCache::new(scratch.0.clone());

    let rooms = load_rooms("moonsedge");
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms, &Default::default());
    assert_eq!(outcome, CacheOutcome::Generated);

    // Same location, changed data — as after a mapdb update.
    let mut updated = rooms.clone();
    updated[0].title = vec!["[Moonsedge, Renamed Plaza]".into()];
    let (_, outcome) = cache.get_or_generate("Moonsedge", &updated, &Default::default());
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
    let (_, outcome) = cache.get_or_generate("Moonsedge", &updated, &Default::default());
    assert_eq!(outcome, CacheOutcome::Hit);
    // ...and the original data regenerates rather than mis-hitting.
    let (_, outcome) = cache.get_or_generate("Moonsedge", &rooms, &Default::default());
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
        let (layout, outcome) = cache.get_or_generate(location, rooms, &Default::default());
        let cold = t0.elapsed();
        assert_eq!(outcome, CacheOutcome::Generated);
        let t0 = std::time::Instant::now();
        let (_, outcome) = cache.get_or_generate(location, rooms, &Default::default());
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

#[test]
fn overrides_shift_groups_pin_rooms_and_skip_orphans() {
    use vellum_fe::core::layout_engine::{overrides, LocationOverrides, RoomTable};

    let rooms = load_rooms("moonsedge");
    let mut owned = rooms.clone();
    let layout = generate_layout(&mut owned);
    let lookup = RoomTable::new(&owned);

    let group = &layout.groups[0];
    let anchor = overrides::group_anchor_key(group, &lookup);
    let room_id = group.room_ids[0];
    let room_key = overrides::room_key(room_id, &lookup);
    let before = group.final_cell(room_id);

    let mut ov = LocationOverrides::default();
    ov.group_offsets.insert(
        anchor,
        vellum_fe::core::layout_engine::Cell { x: 5, y: -3 },
    );
    ov.names.insert(anchor, "Curated Name".into());
    // Orphaned entries: anchors that resolve to nothing must be skipped.
    ov.group_offsets.insert(
        999_999_999_999,
        vellum_fe::core::layout_engine::Cell { x: 1, y: 1 },
    );
    ov.room_pins.insert(
        999_999_999_998,
        vellum_fe::core::layout_engine::Cell { x: 0, y: 0 },
    );

    let mut curated = layout.clone();
    overrides::apply(&mut curated, &lookup, &ov);

    let after = curated.groups[0].final_cell(room_id);
    assert_eq!((after.x - before.x, after.y - before.y), (5, -3));
    assert_eq!(curated.groups[0].name.as_deref(), Some("Curated Name"));

    // Room pin: place the room at a fixed group-relative cell.
    let mut ov2 = LocationOverrides::default();
    ov2.room_pins.insert(
        room_key,
        vellum_fe::core::layout_engine::Cell { x: 40, y: 40 },
    );
    let mut pinned = layout.clone();
    overrides::apply(&mut pinned, &lookup, &ov2);
    assert_eq!(
        pinned.groups[0].positions[&room_id],
        vellum_fe::core::layout_engine::Cell { x: 40, y: 40 }
    );

    // The pristine layout is untouched (overrides never mutate the cache).
    assert_eq!(layout.groups[0].final_cell(room_id), before);

    // Roundtrip the store.
    let path = std::env::temp_dir().join(format!(
        "vellum-map-overrides-test-{}.json",
        std::process::id()
    ));
    let mut store = vellum_fe::core::layout_engine::MapOverrides::default();
    store.locations.insert("Moonsedge".into(), ov);
    overrides::save(&path, &store).expect("save overrides");
    let loaded = overrides::load(&path);
    assert_eq!(
        loaded.locations["Moonsedge"].group_offsets[&anchor],
        vellum_fe::core::layout_engine::Cell { x: 5, y: -3 }
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn classification_override_moves_a_group_between_sheets() {
    use vellum_fe::core::layout_engine::{
        generate_layout_curated, overrides, LocationOverrides, RoomTable, SheetChoice,
    };

    let mut rooms = load_rooms("moonsedge");
    let pristine = generate_layout(&mut rooms.clone());
    let lookup = RoomTable::new(&rooms);
    let &interior_idx = pristine.interiors.first().expect("has interiors");
    let anchor = overrides::group_anchor_key(&pristine.groups[interior_idx], &lookup);

    let mut curated = LocationOverrides::default();
    curated.sheets.insert(anchor, SheetChoice::Outdoor);
    let flipped = generate_layout_curated(&mut rooms, &curated);

    assert_eq!(
        flipped.interiors.len(),
        pristine.interiors.len() - 1,
        "forced-outdoor group must leave the interiors sheet"
    );
    assert!(
        flipped.outdoor.contains(&interior_idx),
        "…and get packed on the outdoor sheet"
    );
    // Door markers were recomputed for the new split, not carried over.
    assert_ne!(
        flipped.classification.entrance_room_ids,
        pristine.classification.entrance_room_ids
    );
}

#[test]
fn curated_hash_gates_the_cache() {
    use vellum_fe::core::layout_engine::{EdgeAction, EdgeOverride, LocationOverrides};

    let scratch = ScratchDir::new("curated");
    let cache = LayoutCache::new(scratch.0.clone());
    let rooms = load_rooms("the-atoll");

    let (_, outcome) = cache.get_or_generate("the Atoll", &rooms, &Default::default());
    assert_eq!(outcome, CacheOutcome::Generated);
    let (_, outcome) = cache.get_or_generate("the Atoll", &rooms, &Default::default());
    assert_eq!(outcome, CacheOutcome::Hit);

    // Different generation-input overrides → different curated hash → miss.
    let mut curated = LocationOverrides::default();
    curated.edges.push(EdgeOverride {
        a: 1,
        b: 2,
        action: EdgeAction::Hide,
    });
    let (_, outcome) = cache.get_or_generate("the Atoll", &rooms, &curated);
    assert_eq!(outcome, CacheOutcome::Generated, "curated change → miss");
    let (_, outcome) = cache.get_or_generate("the Atoll", &rooms, &curated);
    assert_eq!(outcome, CacheOutcome::Hit, "same curated state → hit");
}
