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

use crate::core::layout_engine::{find_latest_mapdb, Layout, LayoutCache, MapDb};

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

enum MapJob {
    LoadDb(PathBuf),
    Generate { location: String, db: Arc<MapDb> },
}

enum MapEvent {
    DbLoaded(Result<Arc<MapDb>, String>),
    LayoutReady {
        location: String,
        layout: Arc<Layout>,
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
    /// Locations with a generation job in flight.
    pending: std::collections::HashSet<String>,

    // Last room identifiers seen on the stream, resolved lazily once the db
    // arrives. nav uid is the stable, preferred identity.
    last_uid: Option<i64>,
    last_lich_id: Option<u32>,

    pub current_location: Option<String>,
    /// Lich room id of the current room (layouts and `;go2` speak room ids).
    pub current_room_id: Option<u32>,
    /// Bumped whenever current room/location/layout state changes; frontends
    /// compare against their last-seen value to recenter or repaint.
    pub revision: u64,
}

impl MapService {
    pub fn new(cache_dir: PathBuf) -> MapService {
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
                        MapJob::Generate { location, db } => {
                            let Some(rooms) = db.rooms(&location) else {
                                continue;
                            };
                            let (layout, _) = cache.get_or_generate(&location, rooms);
                            MapEvent::LayoutReady {
                                location,
                                layout: Arc::new(layout),
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
            pending: Default::default(),
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
        let room_id = self
            .last_uid
            .and_then(|uid| db.room_id_of_uid(uid))
            .or(self.last_lich_id);
        let location = room_id
            .and_then(|id| db.location_of_room_id(id))
            .map(str::to_owned);

        if room_id != self.current_room_id || location != self.current_location {
            self.current_room_id = room_id;
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
        let _ = self.job_tx.send(MapJob::Generate {
            location: location.to_owned(),
            db,
        });
    }

    pub fn layout_for(&self, location: &str) -> Option<&Arc<Layout>> {
        self.layouts.get(location)
    }

    /// The layout for wherever the character currently is.
    pub fn current_layout(&self) -> Option<&Arc<Layout>> {
        self.layouts.get(self.current_location.as_deref()?)
    }

    pub fn is_pending(&self, location: &str) -> bool {
        self.pending.contains(location)
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
                MapEvent::LayoutReady { location, layout } => {
                    self.pending.remove(&location);
                    self.layouts.insert(location, layout);
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
    fn service_is_inert_without_a_db() {
        let mut svc = MapService::new(std::env::temp_dir().join("vellum-map-svc-test"));
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
}
