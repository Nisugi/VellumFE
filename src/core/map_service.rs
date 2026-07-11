//! Live map state: tracks the current room from the game stream, loads the
//! Lich mapdb, and generates location layouts on a worker thread through the
//! disk cache — generate on entry, instant thereafter.
//!
//! Frontends drive it with three calls: `ensure_db` once configuration is
//! known, `note_room` as room identifiers arrive (AppCore does this), and
//! `poll` each frame to drain worker results. Everything else is read-only
//! state for rendering.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use crate::core::layout_engine::positioner::Cell;
use crate::core::layout_engine::{
    build_scene, overrides, Layout, LayoutCache, LocationOverrides, MapOverrides, MapScene,
};
use crate::core::mapdb::{find_latest_mapdb, MapDb, RoomTable};

/// Lich's per-game data subdirectory for a VellumFE game code
/// (`--game prime` → `data/GSIV`).
pub fn lich_game_dir_name(game: Option<&str>) -> &'static str {
    match game.unwrap_or("prime").to_ascii_lowercase().as_str() {
        "test" => "GST",
        "platinum" => "GSPlat",
        "shattered" => "GSF",
        "dr" => "DR",
        "drplatinum" => "DRPlat",
        "drfallen" => "DRF",
        "drtest" => "DRT",
        _ => "GSIV",
    }
}

/// Resolve which mapdb to load from the configured options. Priority:
/// explicit file > downloaded release > Lich folder. Downloaded releases
/// carry GemStone data, so DragonRealms sessions skip straight to the
/// Lich folder (which is per-game).
pub fn resolve_source(
    mapdb_path: Option<&str>,
    lich_dir: Option<&str>,
    game: Option<&str>,
    download_dir: &std::path::Path,
) -> MapDbSource {
    fn non_empty(s: &str) -> Option<&str> {
        let t = s.trim();
        (!t.is_empty()).then_some(t)
    }
    if let Some(path) = mapdb_path.and_then(non_empty) {
        return MapDbSource::File(PathBuf::from(path));
    }
    let game_dir = lich_game_dir_name(game);
    if !game_dir.starts_with("DR") {
        if let Some((_, path)) = crate::core::mapdb_update::latest_downloaded(download_dir) {
            return MapDbSource::File(path);
        }
    }
    if let Some(dir) = lich_dir.and_then(non_empty) {
        return MapDbSource::GameDataDir(std::path::Path::new(dir).join("data").join(game_dir));
    }
    MapDbSource::Unconfigured
}

enum MapJob {
    LoadDb(PathBuf),
    Generate {
        location: String,
        db: Arc<MapDb>,
        overrides: LocationOverrides,
    },
}

enum MapEvent {
    DbLoaded(Result<Arc<MapDb>, String>),
    LayoutReady {
        location: String,
        layout: Arc<Layout>,
        scene: Arc<MapScene>,
    },
}

/// How the mapdb file is located, resolved from config by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MapDbSource {
    /// Map support off until configured.
    #[default]
    Unconfigured,
    /// Explicit mapdb JSON file.
    File(PathBuf),
    /// A Lich per-game data dir (`<lich>/data/GSIV`); newest build wins.
    GameDataDir(PathBuf),
}

/// One editor action against the override store.
#[derive(Debug, Clone)]
pub enum OverrideEdit {
    /// Accumulate a group frame shift (cells); a net zero removes the entry.
    GroupOffset {
        location: String,
        anchor: i64,
        delta: Cell,
    },
    /// Pin (or unpin with `None`) a room, group-relative.
    RoomPin {
        location: String,
        key: i64,
        pin: Option<Cell>,
    },
    /// Rename (or reset with `None`) a group.
    GroupName {
        location: String,
        anchor: i64,
        name: Option<String>,
    },
    /// Set (or clear with `None`) the edge action for a room-key pair.
    Edge {
        location: String,
        a: i64,
        b: i64,
        action: Option<crate::core::layout_engine::EdgeAction>,
    },
    /// Force (or reset with `None`) a group's sheet.
    Sheet {
        location: String,
        anchor: i64,
        choice: Option<crate::core::layout_engine::SheetChoice>,
    },
    /// Drop every override for the location.
    ResetLocation { location: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbState {
    NotLoaded,
    Loading,
    Loaded,
    Failed,
}

pub struct MapService {
    job_tx: mpsc::Sender<MapJob>,
    event_rx: mpsc::Receiver<MapEvent>,
    // Worker detaches on drop; it exits when job_tx closes.
    _worker: std::thread::JoinHandle<()>,

    source: MapDbSource,
    db_state: DbState,
    mapdb: Option<Arc<MapDb>>,
    pub db_error: Option<String>,

    /// Generated layouts by location (backed by the disk cache on the worker).
    layouts: HashMap<String, Arc<Layout>>,
    /// Drawable scenes matching `layouts`.
    scenes: HashMap<String, Arc<MapScene>>,
    /// Locations with a generation job in flight.
    pending: std::collections::HashSet<String>,

    // Last room identifiers seen on the stream, resolved lazily once the db
    // arrives. nav uid is the stable, preferred identity.
    last_uid: Option<i64>,
    last_lich_id: Option<u32>,

    overrides: MapOverrides,
    overrides_path: PathBuf,

    pub current_location: Option<String>,
    /// Lich room id of the current room (layouts and `;go2` speak room ids).
    pub current_room_id: Option<u32>,
    /// Bumped whenever current room/location/layout state changes; frontends
    /// compare against their last-seen value to recenter or repaint.
    pub revision: u64,
}

impl MapService {
    pub fn new(cache_dir: PathBuf, overrides_path: PathBuf) -> MapService {
        let loaded_overrides = overrides::load(&overrides_path);
        let (job_tx, job_rx) = mpsc::channel::<MapJob>();
        let (event_tx, event_rx) = mpsc::channel::<MapEvent>();
        let worker = std::thread::Builder::new()
            .name("map-layout".into())
            .spawn(move || {
                let cache = LayoutCache::new(cache_dir);
                while let Ok(job) = job_rx.recv() {
                    let event = match job {
                        MapJob::LoadDb(path) => {
                            MapEvent::DbLoaded(match MapDb::load(&path) {
                                Ok(db) => Ok(Arc::new(db)),
                                Err(e) => Err(format!("{}: {e}", path.display())),
                            })
                        }
                        MapJob::Generate {
                            location,
                            db,
                            overrides: location_overrides,
                        } => {
                            let Some(rooms) = db.rooms(&location) else {
                                continue;
                            };
                            let (mut layout, _) = cache.get_or_generate(
                                &location,
                                rooms,
                                &location_overrides.generation_subset(),
                            );
                            let lookup = RoomTable::new(rooms);
                            overrides::apply(&mut layout, &lookup, &location_overrides);
                            let scene = build_scene(
                                &location,
                                &layout,
                                &lookup,
                                &location_overrides.edges,
                            );
                            MapEvent::LayoutReady {
                                location,
                                layout: Arc::new(layout),
                                scene: Arc::new(scene),
                            }
                        }
                    };
                    if event_tx.send(event).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn map-layout worker");

        MapService {
            job_tx,
            event_rx,
            _worker: worker,
            source: MapDbSource::Unconfigured,
            db_state: DbState::NotLoaded,
            mapdb: None,
            db_error: None,
            layouts: HashMap::new(),
            scenes: HashMap::new(),
            pending: Default::default(),
            overrides: loaded_overrides,
            overrides_path,
            last_uid: None,
            last_lich_id: None,
            current_location: None,
            current_room_id: None,
            revision: 0,
        }
    }

    pub fn db_state(&self) -> DbState {
        self.db_state
    }

    pub fn mapdb(&self) -> Option<&Arc<MapDb>> {
        self.mapdb.as_ref()
    }

    /// Kick off (or re-kick after a source change) the mapdb load. Cheap to
    /// call repeatedly; only acts on a state change.
    pub fn ensure_db(&mut self, source: MapDbSource) {
        if source == self.source && !matches!(self.db_state, DbState::NotLoaded) {
            return;
        }
        self.source = source;
        self.db_error = None;
        let path = match &self.source {
            MapDbSource::Unconfigured => {
                self.db_state = DbState::NotLoaded;
                return;
            }
            MapDbSource::File(path) => Some(path.clone()),
            MapDbSource::GameDataDir(dir) => find_latest_mapdb(dir),
        };
        let Some(path) = path else {
            self.db_state = DbState::Failed;
            self.db_error = Some(format!(
                "no map-<timestamp>.json found under {}",
                match &self.source {
                    MapDbSource::GameDataDir(dir) => dir.display().to_string(),
                    _ => String::new(),
                }
            ));
            self.revision += 1;
            return;
        };
        self.db_state = DbState::Loading;
        self.mapdb = None;
        self.layouts.clear();
        self.scenes.clear();
        self.pending.clear();
        self.revision += 1;
        let _ = self.job_tx.send(MapJob::LoadDb(path));
    }

    /// Feed the room identifiers the stream reports. `<nav rm='…'/>` carries
    /// the game uid; the `[Name - 12345]` scrape carries the Lich room id.
    /// Either (or both) may be present; uid wins when both resolve.
    pub fn note_room(&mut self, nav_uid: Option<i64>, lich_id: Option<u32>) {
        if nav_uid == self.last_uid && lich_id == self.last_lich_id {
            return;
        }
        self.last_uid = nav_uid;
        self.last_lich_id = lich_id;
        self.resolve_current_room();
    }

    fn resolve_current_room(&mut self) {
        let Some(db) = self.mapdb.clone() else {
            return;
        };
        // Lich reports id 0 for rooms missing from its mapdb, but 0 is also a
        // real room id — the fallback must never trust it. A uid miss plus id
        // 0 means "somewhere unmapped": hold the last known room, so stepping
        // into an unmapped shop keeps the street outside on screen.
        let resolved = self
            .last_uid
            .and_then(|uid| db.room_id_of_uid(uid))
            .or(self.last_lich_id.filter(|&id| id != 0));
        let Some(room_id) = resolved else {
            return;
        };
        let location = db.location_of_room_id(room_id).map(str::to_owned);

        if Some(room_id) != self.current_room_id || location != self.current_location {
            self.current_room_id = Some(room_id);
            self.current_location = location.clone();
            self.revision += 1;
        }
        if let Some(location) = location {
            self.request_location(&location);
        }
    }

    /// Ask for a location's layout (used for the current location and by the
    /// explorer's browser). No-op if generated or already in flight.
    pub fn request_location(&mut self, location: &str) {
        if self.layouts.contains_key(location) || self.pending.contains(location) {
            return;
        }
        let Some(db) = self.mapdb.clone() else {
            return;
        };
        if db.rooms(location).is_none() {
            return;
        }
        self.pending.insert(location.to_owned());
        let location_overrides = self
            .overrides
            .locations
            .get(location)
            .cloned()
            .unwrap_or_default();
        let _ = self.job_tx.send(MapJob::Generate {
            location: location.to_owned(),
            db,
            overrides: location_overrides,
        });
    }

    pub fn layout_for(&self, location: &str) -> Option<&Arc<Layout>> {
        self.layouts.get(location)
    }

    pub fn scene_for(&self, location: &str) -> Option<&Arc<MapScene>> {
        self.scenes.get(location)
    }

    /// The layout for wherever the character currently is.
    pub fn current_layout(&self) -> Option<&Arc<Layout>> {
        self.layouts.get(self.current_location.as_deref()?)
    }

    /// The drawable scene for wherever the character currently is.
    pub fn current_scene(&self) -> Option<&Arc<MapScene>> {
        self.scenes.get(self.current_location.as_deref()?)
    }

    pub fn is_pending(&self, location: &str) -> bool {
        self.pending.contains(location)
    }

    pub fn overrides_for(&self, location: &str) -> Option<&LocationOverrides> {
        self.overrides.locations.get(location)
    }

    /// Apply one editor action to the override store, persist it, and
    /// regenerate the affected location (cache makes this cheap: the clean
    /// layout reloads and the new diff re-applies).
    pub fn apply_override_edit(&mut self, edit: OverrideEdit) {
        let location = match &edit {
            OverrideEdit::GroupOffset { location, .. }
            | OverrideEdit::RoomPin { location, .. }
            | OverrideEdit::GroupName { location, .. }
            | OverrideEdit::Edge { location, .. }
            | OverrideEdit::Sheet { location, .. }
            | OverrideEdit::ResetLocation { location } => location.clone(),
        };
        {
            let entry = self
                .overrides
                .locations
                .entry(location.clone())
                .or_default();
            match edit {
                OverrideEdit::GroupOffset { anchor, delta, .. } => {
                    let cur = entry.group_offsets.entry(anchor).or_default();
                    cur.x += delta.x;
                    cur.y += delta.y;
                    if cur.x == 0 && cur.y == 0 {
                        entry.group_offsets.remove(&anchor);
                    }
                }
                OverrideEdit::RoomPin { key, pin, .. } => match pin {
                    Some(pin) => {
                        entry.room_pins.insert(key, pin);
                    }
                    None => {
                        entry.room_pins.remove(&key);
                    }
                },
                OverrideEdit::GroupName { anchor, name, .. } => match name {
                    Some(name) => {
                        entry.names.insert(anchor, name);
                    }
                    None => {
                        entry.names.remove(&anchor);
                    }
                },
                OverrideEdit::Edge { a, b, action, .. } => {
                    let (a, b) = crate::core::layout_engine::overrides::edge_pair(a, b);
                    entry.edges.retain(|e| (e.a, e.b) != (a, b));
                    if let Some(action) = action {
                        entry
                            .edges
                            .push(crate::core::layout_engine::EdgeOverride { a, b, action });
                    }
                }
                OverrideEdit::Sheet { anchor, choice, .. } => match choice {
                    Some(choice) => {
                        entry.sheets.insert(anchor, choice);
                    }
                    None => {
                        entry.sheets.remove(&anchor);
                    }
                },
                OverrideEdit::ResetLocation { .. } => {
                    *entry = LocationOverrides::default();
                }
            }
            if entry.is_empty() {
                self.overrides.locations.remove(&location);
            }
        }
        if let Err(e) = overrides::save(&self.overrides_path, &self.overrides) {
            tracing::warn!("map overrides save failed: {e}");
        }
        // Regenerate with the new diff.
        self.layouts.remove(&location);
        self.scenes.remove(&location);
        self.revision += 1;
        self.request_location(&location);
    }

    /// Work is in flight (db load or generation); callers should keep
    /// repainting until it drains.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty() || matches!(self.db_state, DbState::Loading)
    }

    /// Drain worker results. Call once per frame/tick.
    pub fn poll(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                MapEvent::DbLoaded(Ok(db)) => {
                    self.mapdb = Some(db);
                    self.db_state = DbState::Loaded;
                    self.revision += 1;
                    // Room identifiers may have arrived while loading.
                    self.resolve_current_room();
                }
                MapEvent::DbLoaded(Err(e)) => {
                    tracing::warn!("mapdb load failed: {e}");
                    self.db_state = DbState::Failed;
                    self.db_error = Some(e);
                    self.revision += 1;
                }
                MapEvent::LayoutReady {
                    location,
                    layout,
                    scene,
                } => {
                    self.pending.remove(&location);
                    self.layouts.insert(location.clone(), layout);
                    self.scenes.insert(location, scene);
                    self.revision += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_dir_names() {
        assert_eq!(lich_game_dir_name(None), "GSIV");
        assert_eq!(lich_game_dir_name(Some("prime")), "GSIV");
        assert_eq!(lich_game_dir_name(Some("Test")), "GST");
        assert_eq!(lich_game_dir_name(Some("platinum")), "GSPlat");
        assert_eq!(lich_game_dir_name(Some("unknown")), "GSIV");
    }

    #[test]
    fn source_resolution_prefers_explicit_then_downloaded_then_lich() {
        let downloads = tempfile::tempdir().unwrap();
        let empty = tempfile::tempdir().unwrap();

        // Nothing configured, nothing downloaded.
        assert_eq!(
            resolve_source(None, None, None, empty.path()),
            MapDbSource::Unconfigured
        );
        // Lich folder alone resolves per-game.
        assert_eq!(
            resolve_source(None, Some("C:/lich"), Some("prime"), empty.path()),
            MapDbSource::GameDataDir(std::path::Path::new("C:/lich").join("data").join("GSIV"))
        );
        // A downloaded release outranks the Lich folder...
        let downloaded = downloads.path().join("mapdb-v0.4.0.json");
        std::fs::write(&downloaded, "[]").unwrap();
        assert_eq!(
            resolve_source(None, Some("C:/lich"), Some("prime"), downloads.path()),
            MapDbSource::File(downloaded.clone())
        );
        // ...but never leaks GemStone rooms into a DragonRealms session.
        assert_eq!(
            resolve_source(None, Some("C:/lich"), Some("dr"), downloads.path()),
            MapDbSource::GameDataDir(std::path::Path::new("C:/lich").join("data").join("DR"))
        );
        // An explicit file outranks everything; blank strings don't count.
        assert_eq!(
            resolve_source(Some("D:/my.json"), Some("C:/lich"), None, downloads.path()),
            MapDbSource::File(PathBuf::from("D:/my.json"))
        );
        assert_eq!(
            resolve_source(Some("  "), Some(""), None, downloads.path()),
            MapDbSource::File(downloaded)
        );
    }

    #[test]
    fn service_is_inert_without_a_db() {
        let tmp = std::env::temp_dir();
        let mut svc = MapService::new(
            tmp.join("vellum-map-svc-test"),
            tmp.join("vellum-map-svc-test-overrides.json"),
        );
        // Room reports before the db loads are remembered, not resolved.
        svc.note_room(Some(4577251), None);
        svc.poll();
        assert_eq!(svc.current_room_id, None);
        assert_eq!(svc.current_location, None);
        assert!(svc.current_layout().is_none());
        // Unconfigured source stays NotLoaded and errors nothing.
        svc.ensure_db(MapDbSource::Unconfigured);
        assert_eq!(svc.db_state(), DbState::NotLoaded);
        assert!(svc.db_error.is_none());
        // A missing game data dir fails cleanly with a message.
        svc.ensure_db(MapDbSource::GameDataDir(
            std::env::temp_dir().join("vellum-nonexistent-lich-dir"),
        ));
        assert_eq!(svc.db_state(), DbState::Failed);
        assert!(svc.db_error.is_some());
    }

    /// Lich reports id 0 for rooms it can't find in the mapdb, and the GSIV
    /// mapdb also has a REAL room 0 (the Moonglae Inn Atrium). Walking into
    /// an unmapped shop must hold the map on the last known room, not
    /// teleport it to the inn; standing in the actual Atrium still resolves
    /// through its uid.
    #[test]
    fn unmapped_room_reports_hold_the_last_known_room() {
        let tmp = std::env::temp_dir();
        let db_path = tmp.join("vellum-map-svc-id0-test.json");
        std::fs::write(
            &db_path,
            r#"[
                {"id": 0, "uid": [13107012], "location": "the Moonglae Inn",
                 "title": ["[Moonglae Inn, Atrium]"], "wayto": {}, "paths": "Obvious exits: out"},
                {"id": 369, "uid": [731009], "location": "Mist Harbor",
                 "title": ["[East Row, Fel Road]"], "wayto": {}, "paths": "Obvious paths: north"}
            ]"#,
        )
        .unwrap();
        let mut svc = MapService::new(
            tmp.join("vellum-map-svc-id0-cache"),
            tmp.join("vellum-map-svc-id0-overrides.json"),
        );
        svc.mapdb = Some(Arc::new(MapDb::load(&db_path).unwrap()));

        // On the street: uid resolves normally.
        svc.note_room(Some(731009), Some(369));
        assert_eq!(svc.current_room_id, Some(369));
        assert_eq!(svc.current_location.as_deref(), Some("Mist Harbor"));

        // Inside an unmapped shop: unknown uid, Lich placeholder id 0.
        svc.note_room(Some(633107), Some(0));
        assert_eq!(svc.current_room_id, Some(369), "id 0 must not be trusted");
        assert_eq!(svc.current_location.as_deref(), Some("Mist Harbor"));

        // Genuinely in the Atrium: its uid resolves to room 0 directly.
        svc.note_room(Some(13107012), Some(0));
        assert_eq!(svc.current_room_id, Some(0));
        assert_eq!(svc.current_location.as_deref(), Some("the Moonglae Inn"));
    }
}
