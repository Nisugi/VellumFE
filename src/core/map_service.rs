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

use crate::core::classic_maps::ClassicMapCatalog;
use crate::core::curated_maps::CuratedMaps;
use crate::core::layout_engine::positioner::Cell;
use crate::core::layout_engine::{
    build_scene, overrides, Layout, LayoutCache, LocationOverrides, MapOverrides, MapScene,
};
use crate::core::mapdb::{find_latest_mapdb, MapDb, Room, RoomTable};
use crate::core::membership::Membership;

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

const AMBIGUOUS_LICH_WARNING: &str =
    "Multiple viable running Lich installs detected; set map.lich_dir to select one";

/// Resolve which mapdb to load from the configured options.
///
/// This preserves the public API used by embedders. Vellum runtimes should use
/// [`resolve_source_resolution`] so the mapdb and its classic-art authority are
/// selected as one coherent session-scoped decision.
pub fn resolve_source(
    mapdb_path: Option<&str>,
    lich_dir: Option<&str>,
    game: Option<&str>,
    download_dir: &std::path::Path,
) -> MapDbSource {
    resolve_source_resolution(mapdb_path, lich_dir, game, download_dir).mapdb
}

/// Resolve which mapdb and classic-art directory to load from the configured
/// options. Mapdb priority:
/// explicit path (a folder scans for the newest map data inside; a file
/// pins that exact build) > downloaded release > configured Lich folder >
/// a running local Lich process. Downloaded releases carry GemStone data, so
/// DragonRealms sessions skip straight to the Lich folder (which is per-game).
///
/// Classic art follows an explicit `lich_dir` first, then a Lich-owned mapdb,
/// then automatic discovery. Automatic discovery is accepted only when
/// exactly one running installation has map data for this game; ambiguity is
/// reported instead of silently mixing one installation's data with another's
/// images.
pub fn resolve_source_resolution(
    mapdb_path: Option<&str>,
    lich_dir: Option<&str>,
    game: Option<&str>,
    download_dir: &std::path::Path,
) -> MapSourceResolution {
    resolve_source_with_auto(
        mapdb_path,
        lich_dir,
        game,
        download_dir,
        crate::process_probe::running_lich_install_dirs,
    )
}

/// Find the sibling `maps/` directory for a Lich map database. Downloaded or
/// otherwise standalone databases have no sibling art and return `None`;
/// callers may then fall back to a running local Lich installation.
fn classic_maps_dir_for(mapdb_path: &std::path::Path) -> Option<PathBuf> {
    let data_dir = mapdb_path
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "data"))?;
    let maps = data_dir.parent()?.join("maps");
    maps.is_dir().then_some(maps)
}

fn resolve_source_with_auto<F>(
    mapdb_path: Option<&str>,
    lich_dir: Option<&str>,
    game: Option<&str>,
    download_dir: &std::path::Path,
    auto_lich_dirs: F,
) -> MapSourceResolution
where
    F: FnOnce() -> Vec<PathBuf>,
{
    fn non_empty(s: &str) -> Option<&str> {
        let t = s.trim();
        (!t.is_empty()).then_some(t)
    }
    let explicit_mapdb = mapdb_path.and_then(non_empty).map(PathBuf::from);
    let explicit_lich = lich_dir.and_then(non_empty).map(PathBuf::from);
    let game_dir = lich_game_dir_name(game);

    let mut mapdb = if let Some(path) = explicit_mapdb {
        // A folder means "the newest map data inside" — the primary way to
        // point at a Lich data dir, which rotates map-<timestamp>.json on
        // every update, so pinning one file there is guaranteed to dangle
        // eventually. An explicit file stays available for the odd case.
        if path.is_dir() {
            MapDbSource::GameDataDir(path)
        } else {
            MapDbSource::File(path)
        }
    } else if !game_dir.starts_with("DR") {
        if let Some((_, path)) = crate::core::mapdb_update::latest_downloaded(download_dir) {
            MapDbSource::File(path)
        } else if let Some(dir) = explicit_lich.as_ref() {
            MapDbSource::GameDataDir(dir.join("data").join(game_dir))
        } else {
            MapDbSource::Unconfigured
        }
    } else if let Some(dir) = explicit_lich.as_ref() {
        MapDbSource::GameDataDir(dir.join("data").join(game_dir))
    } else {
        MapDbSource::Unconfigured
    };

    let source_classic_dir = mapdb_path_hint(&mapdb)
        .as_deref()
        .and_then(classic_maps_dir_for);
    let needs_auto = explicit_lich.is_none()
        && (matches!(mapdb, MapDbSource::Unconfigured) || source_classic_dir.is_none());
    let auto_dirs = needs_auto.then(auto_lich_dirs).unwrap_or_default();
    let viable_auto: Vec<PathBuf> = auto_dirs
        .into_iter()
        .filter(|dir| find_latest_mapdb(&dir.join("data").join(game_dir)).is_some())
        .collect();

    let mut warning = None;
    if matches!(mapdb, MapDbSource::Unconfigured) {
        match viable_auto.as_slice() {
            [dir] => mapdb = MapDbSource::GameDataDir(dir.join("data").join(game_dir)),
            dirs if dirs.len() > 1 => warning = Some(AMBIGUOUS_LICH_WARNING.to_string()),
            _ => {}
        }
    }

    let classic_maps_dir = if let Some(dir) = explicit_lich {
        let maps = dir.join("maps");
        maps.is_dir().then_some(maps)
    } else if let Some(dir) = mapdb_path_hint(&mapdb)
        .as_deref()
        .and_then(classic_maps_dir_for)
    {
        Some(dir)
    } else {
        match viable_auto.as_slice() {
            [dir] => {
                let maps = dir.join("maps");
                maps.is_dir().then_some(maps)
            }
            dirs if dirs.len() > 1 => {
                warning = Some(AMBIGUOUS_LICH_WARNING.to_string());
                None
            }
            _ => None,
        }
    };

    MapSourceResolution {
        mapdb,
        classic_maps_dir,
        warning,
    }
}

fn mapdb_path_hint(source: &MapDbSource) -> Option<PathBuf> {
    match source {
        MapDbSource::File(path) => Some(path.clone()),
        MapDbSource::GameDataDir(dir) => find_latest_mapdb(dir),
        MapDbSource::Unconfigured => None,
    }
}

enum MapJob {
    LoadDb(PathBuf),
    /// Decompose curated coverage + satellites off the UI thread.
    BuildMembership {
        db: Arc<MapDb>,
        curated: CuratedMaps,
    },
    Generate {
        /// Map key: a curated slug, a satellite key, or (fallback mode) a
        /// mapdb location. Opaque to generation, cache, and overrides.
        location: String,
        /// The map's rooms, resolved by the caller through membership (or
        /// `db.rooms(location)` in fallback mode).
        rooms: Vec<Room>,
        overrides: LocationOverrides,
    },
}

enum MapEvent {
    DbLoaded(Result<Arc<MapDb>, String>),
    MembershipReady(Arc<Membership>),
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

/// One coherent map-source decision for a session.
///
/// Keeping mapdb and classic-art selection together prevents frontends from
/// rediscovering a different Lich installation behind the core's back.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapSourceResolution {
    pub mapdb: MapDbSource,
    pub classic_maps_dir: Option<PathBuf>,
    pub warning: Option<String>,
}

impl From<MapDbSource> for MapSourceResolution {
    fn from(mapdb: MapDbSource) -> Self {
        let classic_maps_dir = mapdb_path_hint(&mapdb)
            .as_deref()
            .and_then(classic_maps_dir_for);
        Self {
            mapdb,
            classic_maps_dir,
            warning: None,
        }
    }
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
    /// Move rooms (by uid) to another map's roster; `None` reverts the
    /// personal move so the rooms fall back to community/curated placement.
    MembershipMove { uids: Vec<i64>, to: Option<String> },
    /// Create a user map (empty roster; fill via MembershipMove).
    CreateMap { key: String, name: String },
    /// Replace (or clear with `None`) a room's data edits, keyed by uid.
    RoomEdit {
        uid: i64,
        edit: Option<crate::core::layout_engine::overrides::RoomDataEdit>,
    },
}

/// The always-available staging map for rooms between homes.
pub const PURGATORY_KEY: &str = "user-purgatory";
pub const PURGATORY_NAME: &str = "Purgatory";

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

    source: MapSourceResolution,
    /// Lich's classic annotated-map directory associated with the loaded
    /// mapdb. Renderers see only registry names, never this path.
    classic_maps_dir: Option<PathBuf>,
    classic_maps: Arc<ClassicMapCatalog>,
    classic_maps_warning: Option<String>,
    db_state: DbState,
    mapdb: Option<Arc<MapDb>>,
    pub db_error: Option<String>,

    /// Curated base-map rosters, when available (Saga snapshot). Set once
    /// by the app at startup; None = pure location fallback, today's world.
    curated: Option<CuratedMaps>,
    /// Built on the worker after each db load when `curated` is set.
    membership: Option<Arc<Membership>>,
    /// True between db load and MembershipReady: room resolution is
    /// deferred so the first layout generated is the right one.
    membership_pending: bool,

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
    /// Community-curated overrides shipped with the mapdb release
    /// (`overrides-<tag>.json` beside the downloaded db). Read-only: the
    /// personal `overrides` layer merges on top at generation time, and
    /// editor actions only ever touch the personal file.
    community_overrides: MapOverrides,

    /// Session-only sketches of unmapped rooms (see `core::ghost_rooms`).
    ghosts: crate::core::ghost_rooms::GhostStore,
    /// Ghost uid the character is standing in, when the current room is
    /// unmapped. `current_room_id` keeps the last mapped room (the anchor).
    pub current_ghost: Option<i64>,
    /// Last command sent to the game; consumed as the edge label when the
    /// next room turns out to be a ghost ("go shop").
    last_command: Option<String>,

    pub current_location: Option<String>,
    /// Lich room id of the current room (layouts and `;go2` speak room ids).
    pub current_room_id: Option<u32>,
    /// Bumped whenever current room/location/layout state changes; frontends
    /// compare against their last-seen value to recenter or repaint.
    pub revision: u64,
}

impl MapService {
    /// The promote staging file: personal edits move here on `.mappromote`
    /// and it loads as a community layer every session, so a promotion is
    /// durable on this machine immediately — merging it into
    /// defaults/map_overrides.json (+ rebuild) is only what ships it to
    /// everyone else.
    fn staging_path(overrides_path: &std::path::Path) -> PathBuf {
        overrides_path.with_file_name("map_overrides_promoted.json")
    }

    /// Community base = embedded shipped curation, overlaid by this
    /// machine's promote staging (the owner's newer, not-yet-shipped work).
    fn base_community(overrides_path: &std::path::Path) -> MapOverrides {
        overrides::overlay(
            overrides::embedded_community(),
            overrides::load(&Self::staging_path(overrides_path)),
        )
    }

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
                        MapJob::LoadDb(path) => MapEvent::DbLoaded(match MapDb::load(&path) {
                            Ok(db) => Ok(Arc::new(db)),
                            Err(e) => Err(format!("{}: {e}", path.display())),
                        }),
                        MapJob::BuildMembership { db, curated } => {
                            MapEvent::MembershipReady(Membership::build(&db, &curated))
                        }
                        MapJob::Generate {
                            location,
                            rooms,
                            overrides: location_overrides,
                        } => {
                            let rooms: &[Room] = &rooms;
                            // Curated maze rooms never lay out: their edges
                            // are movement-scramble junk that draws as a
                            // spiderweb. Filtering here changes the content
                            // hash, so caches regenerate on their own when
                            // maze definitions change.
                            let maze_free: Vec<crate::core::mapdb::Room>;
                            let rooms: &[crate::core::mapdb::Room] = if rooms.iter().any(|r| {
                                crate::core::travel::mazes::maze_containing(r.id).is_some()
                            }) {
                                maze_free = rooms
                                    .iter()
                                    .filter(|r| {
                                        crate::core::travel::mazes::maze_containing(r.id).is_none()
                                    })
                                    .cloned()
                                    .collect();
                                &maze_free
                            } else {
                                rooms
                            };
                            let (mut layout, _) = cache.get_or_generate(
                                &location,
                                rooms,
                                &location_overrides.generation_subset(),
                            );
                            let lookup = RoomTable::new(rooms);
                            overrides::apply(&mut layout, &lookup, &location_overrides);
                            let scene =
                                build_scene(&location, &layout, &lookup, &location_overrides.edges);
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
            source: MapSourceResolution::default(),
            classic_maps_dir: None,
            classic_maps: Arc::new(ClassicMapCatalog::new()),
            classic_maps_warning: None,
            db_state: DbState::NotLoaded,
            mapdb: None,
            db_error: None,
            curated: None,
            membership: None,
            membership_pending: false,
            layouts: HashMap::new(),
            scenes: HashMap::new(),
            pending: Default::default(),
            community_overrides: Self::base_community(&overrides_path),
            overrides: loaded_overrides,
            overrides_path,
            ghosts: Default::default(),
            current_ghost: None,
            last_command: None,
            last_uid: None,
            last_lich_id: None,
            current_location: None,
            current_room_id: None,
            revision: 0,
        }
    }

    /// The real server uid of the room we're standing in, when known.
    /// (Bestiary "here" queries key spawn ranges off this.)
    pub fn current_uid(&self) -> Option<i64> {
        self.last_uid
    }

    pub fn db_state(&self) -> DbState {
        self.db_state
    }

    pub fn mapdb(&self) -> Option<&Arc<MapDb>> {
        self.mapdb.as_ref()
    }

    /// The trusted classic-art catalog belonging to this game session.
    /// Frontends clone the `Arc`; they never perform their own discovery.
    pub fn classic_maps(&self) -> Arc<ClassicMapCatalog> {
        Arc::clone(&self.classic_maps)
    }

    /// Non-fatal discovery warning. Generated/downloaded maps remain usable
    /// when classic art is disabled because automatic Lich discovery was
    /// ambiguous.
    pub fn classic_maps_warning(&self) -> Option<&str> {
        self.classic_maps_warning.as_deref()
    }

    /// The current room's classic annotated map and its pixel rectangle.
    /// Missing art is normal and lets callers fall back to the local graph.
    pub fn current_classic_map(&self) -> Option<(String, [f64; 4])> {
        let room = self.mapdb.as_ref()?.room(self.current_room_id?)?;
        let name = room.image.as_ref()?;
        let coords = room.image_coords?;
        self.classic_maps.get(name)?;
        Some((name.clone(), coords))
    }

    /// The curated/satellite membership, once built. None in fallback mode
    /// (no curated data) or while the build is still in flight.
    pub fn membership(&self) -> Option<&Arc<Membership>> {
        self.membership.as_ref()
    }

    /// Provide curated base-map rosters. Call once at startup (and again if
    /// the snapshot is refreshed); kicks the membership build if the db is
    /// already loaded.
    pub fn set_curated(&mut self, curated: CuratedMaps) {
        if curated.is_empty() || self.curated.as_ref() == Some(&curated) {
            return;
        }
        self.curated = Some(curated);
        self.membership = None;
        if let Some(db) = self.mapdb.clone() {
            self.membership_pending = true;
            let _ = self.job_tx.send(MapJob::BuildMembership {
                db,
                curated: self.effective_curated().expect("just set"),
            });
        }
    }

    /// The curated rosters with membership overrides applied — what the
    /// membership build actually consumes. Community moves under personal
    /// ones, same layering as every other override.
    fn effective_curated(&self) -> Option<CuratedMaps> {
        let base = self.curated.as_ref()?;
        let (moves, custom) = crate::core::layout_engine::overrides::merged_membership(
            &self.community_overrides,
            &self.overrides,
        );
        if moves.is_empty() && custom.is_empty() {
            return Some(base.clone());
        }
        Some(crate::core::curated_maps::apply_membership_overrides(
            base, &moves, &custom,
        ))
    }

    /// Map key for a mappable room: membership when built, else the mapdb
    /// location — one resolution rule for switching and generation alike.
    fn map_key_of_room(&self, db: &MapDb, room_id: u32) -> Option<String> {
        if let Some(membership) = &self.membership {
            if let Some(key) = membership.map_of_room(room_id) {
                return Some(key.to_string());
            }
        }
        db.location_of_room_id(room_id).map(str::to_owned)
    }

    /// Display name for a map key ("Wehnimers Landing Town" for a curated
    /// slug, the auto satellite name, or the location string itself).
    pub fn display_name<'a>(&'a self, key: &'a str) -> &'a str {
        match &self.membership {
            Some(membership) => membership.display_name(key),
            None => key,
        }
    }

    /// Inject a mapdb directly (tests only — the live path loads from disk).
    #[cfg(test)]
    pub fn set_mapdb_for_test(&mut self, db: MapDb) {
        self.mapdb = Some(Arc::new(db));
    }

    /// Force a fresh mapdb reload from the current source (`.go2 reload`,
    /// Lich's `Map.reload`). Drops the loaded db and re-kicks the load.
    pub fn reload(&mut self) {
        let source = self.source.clone();
        self.mapdb = None;
        self.db_state = DbState::NotLoaded;
        self.db_error = None;
        self.source = MapSourceResolution::default(); // force ensure_db past its guard
        self.ensure_db(source);
    }

    /// Kick off (or re-kick after a source change) the mapdb load. Cheap to
    /// call repeatedly; only acts on a state change.
    pub fn ensure_db(&mut self, source: impl Into<MapSourceResolution>) {
        let source = source.into();
        let mapdb_changed = source.mapdb != self.source.mapdb;
        let classic_dir_missing = self
            .classic_maps_dir
            .as_ref()
            .is_some_and(|dir| !dir.is_dir());
        let resolution_changed = source != self.source;
        if resolution_changed || classic_dir_missing {
            let classic_maps_dir = source.classic_maps_dir.clone();
            let classic_dir_changed = classic_maps_dir != self.classic_maps_dir;
            if classic_dir_changed {
                self.classic_maps_dir = classic_maps_dir;
            }
            // A mapdb reload may add image files without changing the maps
            // directory path, so source changes must refresh the registry too.
            if resolution_changed || classic_dir_changed || classic_dir_missing {
                let _ = self
                    .classic_maps
                    .reload_from_dir(self.classic_maps_dir.as_deref());
                if classic_dir_changed {
                    self.revision += 1;
                }
            }
            if source.warning != self.classic_maps_warning {
                if let Some(warning) = source.warning.as_deref() {
                    tracing::warn!("{warning}; classic map art is disabled for this session");
                }
                self.classic_maps_warning = source.warning.clone();
            }
        }
        if !mapdb_changed && !matches!(self.db_state, DbState::NotLoaded) {
            // Art selection and its warning are independent of the mapdb
            // worker. They were reconciled above; don't reload a healthy
            // generated map merely because the classic directory changed.
            self.source = source;
            return;
        }
        self.source = source;
        self.db_error = None;
        let path = match &self.source.mapdb {
            MapDbSource::Unconfigured => {
                self.db_state = DbState::NotLoaded;
                return;
            }
            MapDbSource::File(path) => {
                // Fail a dangling explicit file here with a teaching error
                // instead of the worker's bare OS error: the common cause is
                // a pin to a Lich map-<timestamp>.json that Lich has since
                // rotated away, and the fix is folder mode.
                if !path.is_file() {
                    let msg = format!(
                        "mapdb file not found: {} — set the map data path to its \
                         folder instead to always load the newest map data there",
                        path.display()
                    );
                    self.db_state = DbState::Failed;
                    self.db_error = Some(msg);
                    self.revision += 1;
                    return;
                }
                Some(path.clone())
            }
            MapDbSource::GameDataDir(dir) => find_latest_mapdb(dir),
        };
        let Some(path) = path else {
            self.db_state = DbState::Failed;
            self.db_error = Some(format!(
                "no map-<timestamp>.json found under {}",
                match &self.source.mapdb {
                    MapDbSource::GameDataDir(dir) => dir.display().to_string(),
                    _ => String::new(),
                }
            ));
            self.revision += 1;
            return;
        };
        self.db_state = DbState::Loading;
        self.mapdb = None;
        self.membership = None;
        self.membership_pending = false;
        self.layouts.clear();
        self.scenes.clear();
        self.pending.clear();
        self.revision += 1;
        // Community layers: shipped curation + local promote staging,
        // overlaid by any overrides traveling with the db they were curated
        // against.
        self.community_overrides = overrides::overlay(
            Self::base_community(&self.overrides_path),
            match crate::core::mapdb_update::community_overrides_for(&path) {
                Some(p) => overrides::load(&p),
                None => MapOverrides::default(),
            },
        );
        let _ = self.job_tx.send(MapJob::LoadDb(path));
    }

    /// Feed the room identifiers the stream reports. `<nav rm='…'/>` carries
    /// the game uid; the `[Name - 12345]` scrape carries the Lich room id.
    /// Either (or both) may be present; uid wins when both resolve.
    /// `snapshot` carries what the stream said about the room (title, obvious
    /// exits) — that's all a ghost room has to go on.
    pub fn note_room(
        &mut self,
        nav_uid: Option<i64>,
        lich_id: Option<u32>,
        snapshot: crate::core::ghost_rooms::RoomSnapshot,
    ) {
        // A report with no usable id at all can't be deduped by identity —
        // consecutive uid-less rooms look identical here, so every report
        // must reach the content-matching fallback in resolve_current_room.
        let identity_less = nav_uid.is_none() && !lich_id.is_some_and(|id| id != 0);
        if !identity_less && nav_uid == self.last_uid && lich_id == self.last_lich_id {
            // Same room; exits/title often arrive a line after the nav tag,
            // so keep the current ghost's sketch fresh.
            if let Some(uid) = self.current_ghost {
                self.ghosts.visit(
                    uid,
                    snapshot,
                    crate::core::ghost_rooms::Origin::Unknown,
                    None,
                );
            }
            return;
        }
        self.last_uid = nav_uid;
        self.last_lich_id = lich_id;
        self.resolve_current_room(snapshot);
    }

    /// Remember the last outbound command; if the next room resolution turns
    /// out to be a ghost, this is the command that walked into it.
    pub fn note_command(&mut self, command: &str) {
        let command = command.trim();
        if !command.is_empty() {
            self.last_command = Some(command.to_owned());
        }
    }

    fn resolve_current_room(&mut self, snapshot: crate::core::ghost_rooms::RoomSnapshot) {
        use crate::core::ghost_rooms::Origin;
        let Some(db) = self.mapdb.clone() else {
            return;
        };
        // Membership is being built: hold. MembershipReady re-resolves the
        // remembered identifiers, so nothing is lost — this only prevents a
        // throwaway location layout in the gap.
        if self.membership_pending {
            return;
        }
        // Lich reports id 0 for rooms missing from its mapdb, but 0 is also a
        // real room id — the fallback must never trust it. A uid miss plus id
        // 0 means "somewhere unmapped".
        let resolved = self
            .last_uid
            .and_then(|uid| db.room_id_of_uid(uid))
            .or(self.last_lich_id.filter(|&id| id != 0));
        let Some(room_id) = resolved else {
            // Unmapped. With a usable uid, sketch a ghost room hanging off
            // the held room; without one the room has no wire identity —
            // try matching what it looks like (title/description/exits),
            // the way Lich resolves rooms for FEs that never see a uid.
            // Failing that, hold: stepping into an unmapped shop keeps the
            // street outside on screen.
            let Some(uid) = self.last_uid.filter(|&u| u != 0) else {
                if let Some(room_id) = self.match_room_by_content(&db, &snapshot) {
                    if self.current_ghost.take().is_some() {
                        self.revision += 1;
                    }
                    self.apply_resolved_room(&db, room_id);
                }
                return;
            };
            let from = match self.current_ghost {
                Some(prev) => Origin::Ghost(prev),
                None => match self.current_room_id {
                    Some(anchor) => Origin::Mapped(anchor),
                    None => Origin::Unknown,
                },
            };
            let command = self.last_command.take();
            self.ghosts.visit(uid, snapshot, from, command);
            if self.current_ghost != Some(uid) {
                self.current_ghost = Some(uid);
                self.revision += 1;
            }
            return;
        };
        // Back on the map: the sketch stays for the session, but we're no
        // longer standing in it.
        if self.current_ghost.take().is_some() {
            self.revision += 1;
        }
        self.apply_resolved_room(&db, room_id);
    }

    /// Commit a resolved room id: update current room/location and kick off
    /// the location's layout if it isn't built yet.
    fn apply_resolved_room(&mut self, db: &crate::core::mapdb::MapDb, room_id: u32) {
        let location = self.map_key_of_room(db, room_id);

        if Some(room_id) != self.current_room_id || location != self.current_location {
            self.current_room_id = Some(room_id);
            self.current_location = location.clone();
            self.revision += 1;
        }
        if let Some(location) = location {
            self.request_location(&location);
        }
    }

    /// Match a room that arrived with no usable uid or Lich id by its
    /// content: title first (the candidate pool), then description (mapdb
    /// descriptions were captured verbatim from the game), then the obvious
    /// exits, then adjacency to the room we were just in. Returns a match
    /// only when it is unambiguous — resolving to the wrong room would walk
    /// the map away from the player, holding is strictly safer.
    fn match_room_by_content(
        &self,
        db: &crate::core::mapdb::MapDb,
        snapshot: &crate::core::ghost_rooms::RoomSnapshot,
    ) -> Option<u32> {
        let title = snapshot
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())?;
        let mut candidates: Vec<u32> = db.room_ids_with_title(title).to_vec();
        if candidates.is_empty() {
            return None;
        }

        if let Some(desc) = snapshot
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            let filtered: Vec<u32> = candidates
                .iter()
                .copied()
                .filter(|&id| {
                    db.room(id)
                        .is_some_and(|r| r.description.iter().any(|d| d.trim() == desc))
                })
                .collect();
            // An empty filter result means the description didn't match any
            // candidate (dynamic inserts, stale mapdb capture) — keep the
            // pool rather than concluding "nowhere".
            if !filtered.is_empty() {
                candidates = filtered;
            }
        }

        if candidates.len() > 1 && !snapshot.exits.is_empty() {
            let mut exits: Vec<String> = snapshot
                .exits
                .iter()
                .map(|e| e.to_ascii_lowercase())
                .collect();
            exits.sort();
            let filtered: Vec<u32> = candidates
                .iter()
                .copied()
                .filter(|&id| {
                    db.room(id).is_some_and(|r| {
                        let mut room_exits: Vec<String> = r
                            .paths
                            .split_once(':')
                            .map(|(_, rest)| rest)
                            .unwrap_or("")
                            .split(',')
                            .map(|e| e.trim().to_ascii_lowercase())
                            .filter(|e| !e.is_empty() && e != "none")
                            .collect();
                        room_exits.sort();
                        room_exits == exits
                    })
                })
                .collect();
            if !filtered.is_empty() {
                candidates = filtered;
            }
        }

        if candidates.len() == 1 {
            return candidates.pop();
        }

        // Identity-less rooms are re-reported as their pieces arrive across
        // several lines — if where we already are still matches, stay put
        // rather than sliding to an identical-looking neighbor.
        if let Some(current) = self.current_room_id {
            if candidates.contains(&current) {
                return Some(current);
            }
        }

        // Still ambiguous ("[A Dark Tunnel]" twenty times over): the room we
        // just walked out of usually has an edge to the one we're now in.
        let prev_room = self.current_room_id.and_then(|id| db.room(id))?;
        let mut adjacent = candidates
            .into_iter()
            .filter(|id| prev_room.wayto.contains_key(id));
        let first = adjacent.next();
        first.filter(|_| adjacent.next().is_none())
    }

    /// The session's ghost-room sketches (unmapped interiors).
    pub fn ghosts(&self) -> &crate::core::ghost_rooms::GhostStore {
        &self.ghosts
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
        // Resolve the map's rooms here (worker jobs carry them): membership
        // key first, mapdb location as the fallback namespace.
        let rooms: Vec<Room> = match self
            .membership
            .as_ref()
            .and_then(|m| m.rooms_of_map(location))
        {
            Some(ids) => ids.iter().filter_map(|&id| db.room(id).cloned()).collect(),
            None => match db.rooms(location) {
                Some(rooms) => rooms.to_vec(),
                None => return,
            },
        };
        if rooms.is_empty() {
            return;
        }
        self.pending.insert(location.to_owned());
        // Community layer under the personal one; editor writes stay personal.
        let location_overrides = overrides::merge_location(
            self.community_overrides.locations.get(location),
            self.overrides.locations.get(location),
        );
        let _ = self.job_tx.send(MapJob::Generate {
            location: location.to_owned(),
            rooms,
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

    /// The personal membership move for a uid, if any (drives "Revert move").
    pub fn personal_membership_move(&self, uid: i64) -> Option<&str> {
        self.overrides
            .membership_moves
            .get(&uid)
            .map(String::as_str)
    }

    /// The EFFECTIVE room-data edit for a uid (community under personal) —
    /// what the editor composes its next whole-entry write from.
    pub fn room_edit(
        &self,
        uid: i64,
    ) -> Option<crate::core::layout_engine::overrides::RoomDataEdit> {
        self.overrides
            .room_edits
            .get(&uid)
            .or_else(|| self.community_overrides.room_edits.get(&uid))
            .cloned()
    }

    /// True when the uid has a PERSONAL room-data edit (drives the revert UI).
    pub fn has_personal_room_edit(&self, uid: i64) -> bool {
        self.overrides.room_edits.contains_key(&uid)
    }

    /// Key for a user-created map: "user-" + kebab of the name, so user keys
    /// can never collide with curated slugs or `sat-*` satellite keys.
    pub fn user_map_key(name: &str) -> String {
        let kebab: String = name
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let mut kebab = kebab.trim_matches('-').to_string();
        while kebab.contains("--") {
            kebab = kebab.replace("--", "-");
        }
        format!("user-{kebab}")
    }

    /// Apply one editor action to the override store, persist it, and
    /// regenerate the affected location (cache makes this cheap: the clean
    /// layout reloads and the new diff re-applies).
    pub fn apply_override_edit(&mut self, edit: OverrideEdit) {
        // Membership edits change which rooms belong to which maps, so they
        // save and then rebuild the membership (satellites recompute) and
        // drop every generated layout — cheap via the layout cache.
        match edit {
            OverrideEdit::MembershipMove { uids, to } => {
                match to {
                    Some(key) => {
                        // Purgatory materializes on first use.
                        if key == PURGATORY_KEY {
                            self.overrides
                                .custom_maps
                                .entry(PURGATORY_KEY.to_string())
                                .or_insert_with(|| PURGATORY_NAME.to_string());
                        }
                        for uid in uids {
                            self.overrides.membership_moves.insert(uid, key.clone());
                        }
                    }
                    None => {
                        for uid in uids {
                            self.overrides.membership_moves.remove(&uid);
                        }
                    }
                }
                self.after_membership_edit();
                return;
            }
            OverrideEdit::CreateMap { key, name } => {
                self.overrides.custom_maps.insert(key, name);
                self.after_membership_edit();
                return;
            }
            OverrideEdit::RoomEdit { uid, edit } => {
                match edit {
                    Some(edit) if !edit.is_empty() => {
                        self.overrides.room_edits.insert(uid, edit);
                    }
                    _ => {
                        self.overrides.room_edits.remove(&uid);
                    }
                }
                if let Err(e) = overrides::save(&self.overrides_path, &self.overrides) {
                    tracing::warn!("map overrides save failed: {e}");
                }
                // Room data lives inside the loaded db; reverting needs the
                // pristine copy back, so reload from source — the worker
                // re-reads, edits reapply on DbLoaded, membership rebuilds.
                self.reload();
                return;
            }
            _ => {}
        }
        let location = match &edit {
            OverrideEdit::GroupOffset { location, .. }
            | OverrideEdit::RoomPin { location, .. }
            | OverrideEdit::GroupName { location, .. }
            | OverrideEdit::Edge { location, .. }
            | OverrideEdit::Sheet { location, .. }
            | OverrideEdit::ResetLocation { location } => location.clone(),
            OverrideEdit::MembershipMove { .. }
            | OverrideEdit::CreateMap { .. }
            | OverrideEdit::RoomEdit { .. } => {
                unreachable!("handled above")
            }
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
                OverrideEdit::MembershipMove { .. }
                | OverrideEdit::CreateMap { .. }
                | OverrideEdit::RoomEdit { .. } => {
                    unreachable!("handled before the location block")
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

    /// Save membership-editing state and rebuild the world's membership:
    /// scenes and layouts all drop (stale rosters), and the effective
    /// curated set goes back through the worker.
    fn after_membership_edit(&mut self) {
        if let Err(e) = overrides::save(&self.overrides_path, &self.overrides) {
            tracing::warn!("map overrides save failed: {e}");
        }
        self.layouts.clear();
        self.scenes.clear();
        self.pending.clear();
        self.revision += 1;
        if let (Some(db), Some(curated)) = (self.mapdb.clone(), self.effective_curated()) {
            // Drop the stale membership so resolution holds until the
            // rebuilt one lands (same as set_curated).
            self.membership = None;
            self.membership_pending = true;
            let _ = self.job_tx.send(MapJob::BuildMembership { db, curated });
        }
    }

    /// Promote personal map edits into the staging export that feeds the
    /// shipped community layer (defaults/map_overrides.json).
    ///
    /// Each promoted map's personal state MERGES into its staging entry
    /// with the same semantics the renderer uses at use time
    /// (`overrides::merge_location`: personal wins per key, group offsets
    /// ADD) — after a first promote empties the personal layer, later
    /// edits are only deltas relative to the staged curation, and the old
    /// wholesale replacement threw the whole staged map away on the second
    /// promote, reverting everything but the newest nudge to auto-layout.
    /// The personal entry is then cleared — the promoted data reaches the
    /// user through the community layer instead (leaving it personal too
    /// would double-apply group-offset deltas). `key = None` promotes
    /// every map with personal edits. Returns the promoted keys and the
    /// staging path.
    pub fn promote_overrides(
        &mut self,
        key: Option<&str>,
    ) -> Result<(Vec<String>, PathBuf), String> {
        let staging_path = Self::staging_path(&self.overrides_path);
        // Membership state (moves, custom maps, room edits) is global — it
        // promotes on a specific-map promote only when it targets that map,
        // and wholesale on a full promote.
        let has_membership_edits = |ov: &MapOverrides, key: Option<&str>| match key {
            None => {
                !ov.membership_moves.is_empty()
                    || !ov.custom_maps.is_empty()
                    || !ov.room_edits.is_empty()
            }
            Some(k) => {
                ov.membership_moves.values().any(|v| v == k) || ov.custom_maps.contains_key(k)
            }
        };
        let keys: Vec<String> = match key {
            Some(key) => {
                if !self.overrides.locations.contains_key(key)
                    && !has_membership_edits(&self.overrides, Some(key))
                {
                    return Err(format!("no personal edits for '{key}'"));
                }
                vec![key.to_owned()]
            }
            None => self.overrides.locations.keys().cloned().collect(),
        };
        if keys.is_empty() && !has_membership_edits(&self.overrides, None) {
            return Err("no personal map edits to promote".into());
        }
        let mut staging = overrides::load(&staging_path);
        // Membership promotion. Both layers merge as a union at use time, so
        // moving entries between them leaves the effective membership
        // unchanged — no rebuild needed.
        {
            let uids: Vec<i64> = match key {
                None => self.overrides.membership_moves.keys().copied().collect(),
                Some(k) => self
                    .overrides
                    .membership_moves
                    .iter()
                    .filter(|(_, v)| v.as_str() == k)
                    .map(|(&u, _)| u)
                    .collect(),
            };
            for uid in uids {
                if let Some(target) = self.overrides.membership_moves.remove(&uid) {
                    staging.membership_moves.insert(uid, target.clone());
                    self.community_overrides
                        .membership_moves
                        .insert(uid, target);
                }
            }
            let customs: Vec<String> = match key {
                None => self.overrides.custom_maps.keys().cloned().collect(),
                Some(k) => self
                    .overrides
                    .custom_maps
                    .keys()
                    .filter(|c| c.as_str() == k)
                    .cloned()
                    .collect(),
            };
            for ck in customs {
                if let Some(name) = self.overrides.custom_maps.remove(&ck) {
                    staging.custom_maps.insert(ck.clone(), name.clone());
                    self.community_overrides.custom_maps.insert(ck, name);
                }
            }
            // Room-data edits promote on full promotes only (they're keyed
            // by uid, not map).
            if key.is_none() {
                let uids: Vec<i64> = self.overrides.room_edits.keys().copied().collect();
                for uid in uids {
                    if let Some(edit) = self.overrides.room_edits.remove(&uid) {
                        staging.room_edits.insert(uid, edit.clone());
                        self.community_overrides.room_edits.insert(uid, edit);
                    }
                }
            }
        }
        for key in &keys {
            if let Some(entry) = self.overrides.locations.remove(key) {
                let merged = overrides::merge_location(staging.locations.get(key), Some(&entry));
                staging.locations.insert(key.clone(), merged);
            }
        }
        overrides::save(&staging_path, &staging)
            .map_err(|e| format!("staging save failed: {e}"))?;
        if let Err(e) = overrides::save(&self.overrides_path, &self.overrides) {
            return Err(format!("personal overrides save failed: {e}"));
        }
        // Promoted maps render from the community layer now — but this
        // session's community store predates the promotion, so overlay the
        // promoted entries in memory and regenerate.
        for key in &keys {
            if let Some(entry) = staging.locations.get(key) {
                self.community_overrides
                    .locations
                    .insert(key.clone(), entry.clone());
            }
            self.layouts.remove(key);
            self.scenes.remove(key);
            self.request_location(key);
        }
        self.revision += 1;
        Ok((keys, staging_path))
    }

    /// Re-home overrides stored under keys that are no longer maps (legacy
    /// location names, or a satellite whose key churned) to whichever map
    /// now contains their anchor uid. Anchors that resolve nowhere stay
    /// under their old key — the apply path already skips orphans silently,
    /// and a future membership may claim them.
    fn remap_overrides_to_membership(&mut self) {
        let (Some(db), Some(membership)) = (self.mapdb.clone(), self.membership.clone()) else {
            return;
        };
        let changed = remap_overrides(&mut self.overrides, &db, &membership);
        remap_overrides(&mut self.community_overrides, &db, &membership);
        if changed {
            if let Err(e) = overrides::save(&self.overrides_path, &self.overrides) {
                tracing::warn!("map overrides save after remap failed: {e}");
            }
        }
    }

    /// Work is in flight (db load or generation); callers should keep
    /// repainting until it drains.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
            || self.membership_pending
            || matches!(self.db_state, DbState::Loading)
    }

    /// Drain worker results. Call once per frame/tick.
    pub fn poll(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                MapEvent::DbLoaded(Ok(db)) => {
                    // Room-data edits bake into the loaded db (pathing and
                    // layout generation read it); pristine data returns via
                    // reload when an edit is reverted.
                    let edits = crate::core::layout_engine::overrides::merged_room_edits(
                        &self.community_overrides,
                        &self.overrides,
                    );
                    let db = if edits.is_empty() {
                        db
                    } else {
                        let mut edited = (*db).clone();
                        edited.apply_room_edits(&edits);
                        Arc::new(edited)
                    };
                    self.mapdb = Some(db.clone());
                    self.db_state = DbState::Loaded;
                    self.revision += 1;
                    if let Some(curated) = self.effective_curated() {
                        // Defer room resolution until membership lands so the
                        // first layout generated is the curated one, not a
                        // throwaway location layout.
                        self.membership_pending = true;
                        let _ = self.job_tx.send(MapJob::BuildMembership { db, curated });
                    } else {
                        // Room identifiers may have arrived while loading.
                        // No stream snapshot here; if this resolves into a
                        // ghost, the next same-room report backfills
                        // title/exits.
                        self.resolve_current_room(Default::default());
                    }
                }
                MapEvent::MembershipReady(membership) => {
                    self.membership = Some(membership);
                    self.membership_pending = false;
                    // Overrides authored against location maps re-home to
                    // whichever map now holds their anchor uid — personal
                    // ones persist; community ones remap in memory only.
                    self.remap_overrides_to_membership();
                    self.revision += 1;
                    self.resolve_current_room(Default::default());
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

/// Move each override item stored under a non-map key to the map that now
/// contains its anchor uid. Existing destination entries win on conflict
/// (never clobber something the user authored against the new map). Returns
/// true when anything moved.
fn remap_overrides(store: &mut MapOverrides, db: &MapDb, membership: &Membership) -> bool {
    let dest_of = |uid: i64| -> Option<String> {
        let id = db.room_id_of_uid(uid)?;
        membership.map_of_room(id).map(str::to_owned)
    };
    let legacy_keys: Vec<String> = store
        .locations
        .keys()
        .filter(|key| membership.rooms_of_map(key).is_none())
        .cloned()
        .collect();
    let mut moved = false;
    for key in legacy_keys {
        let Some(entry) = store.locations.remove(&key) else {
            continue;
        };
        let mut keep = LocationOverrides::default();
        for (anchor, cell) in entry.group_offsets {
            match dest_of(anchor) {
                Some(dest) if dest != key => {
                    store
                        .locations
                        .entry(dest)
                        .or_default()
                        .group_offsets
                        .entry(anchor)
                        .or_insert(cell);
                    moved = true;
                }
                _ => {
                    keep.group_offsets.insert(anchor, cell);
                }
            }
        }
        for (room, pin) in entry.room_pins {
            match dest_of(room) {
                Some(dest) if dest != key => {
                    store
                        .locations
                        .entry(dest)
                        .or_default()
                        .room_pins
                        .entry(room)
                        .or_insert(pin);
                    moved = true;
                }
                _ => {
                    keep.room_pins.insert(room, pin);
                }
            }
        }
        for (anchor, name) in entry.names {
            match dest_of(anchor) {
                Some(dest) if dest != key => {
                    store
                        .locations
                        .entry(dest)
                        .or_default()
                        .names
                        .entry(anchor)
                        .or_insert(name);
                    moved = true;
                }
                _ => {
                    keep.names.insert(anchor, name);
                }
            }
        }
        for (anchor, choice) in entry.sheets {
            match dest_of(anchor) {
                Some(dest) if dest != key => {
                    store
                        .locations
                        .entry(dest)
                        .or_default()
                        .sheets
                        .entry(anchor)
                        .or_insert(choice);
                    moved = true;
                }
                _ => {
                    keep.sheets.insert(anchor, choice);
                }
            }
        }
        for edge in entry.edges {
            match dest_of(edge.a) {
                Some(dest) if dest != key => {
                    let dest_entry = store.locations.entry(dest).or_default();
                    if !dest_entry
                        .edges
                        .iter()
                        .any(|e| (e.a, e.b) == (edge.a, edge.b))
                    {
                        dest_entry.edges.push(edge);
                    }
                    moved = true;
                }
                _ => {
                    keep.edges.push(edge);
                }
            }
        }
        if !keep.is_empty() {
            store.locations.insert(key, keep);
        }
    }
    moved
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

        let resolve = |mapdb_path, lich_dir, game, download_dir| {
            resolve_source_with_auto(mapdb_path, lich_dir, game, download_dir, Vec::new).mapdb
        };

        // Nothing configured, nothing downloaded.
        assert_eq!(
            resolve(None, None, None, empty.path()),
            MapDbSource::Unconfigured
        );
        // Lich folder alone resolves per-game.
        assert_eq!(
            resolve(None, Some("C:/lich"), Some("prime"), empty.path()),
            MapDbSource::GameDataDir(std::path::Path::new("C:/lich").join("data").join("GSIV"))
        );
        // A downloaded release outranks the Lich folder...
        let downloaded = downloads.path().join("mapdb-v0.4.0.json");
        std::fs::write(&downloaded, "[]").unwrap();
        assert_eq!(
            resolve(None, Some("C:/lich"), Some("prime"), downloads.path()),
            MapDbSource::File(downloaded.clone())
        );
        // ...but never leaks GemStone rooms into a DragonRealms session.
        assert_eq!(
            resolve(None, Some("C:/lich"), Some("dr"), downloads.path()),
            MapDbSource::GameDataDir(std::path::Path::new("C:/lich").join("data").join("DR"))
        );
        // An explicit file outranks everything; blank strings don't count.
        assert_eq!(
            resolve(Some("D:/my.json"), Some("C:/lich"), None, downloads.path()),
            MapDbSource::File(PathBuf::from("D:/my.json"))
        );
        assert_eq!(
            resolve(Some("  "), Some(""), None, downloads.path()),
            MapDbSource::File(downloaded)
        );
        // An explicit path that is a FOLDER means "newest map data inside" —
        // it scans instead of pinning, so Lich rotating map-<timestamp>.json
        // never dangles the config.
        let scan_dir = tempfile::tempdir().unwrap();
        let dir_str = scan_dir.path().to_string_lossy().to_string();
        assert_eq!(
            resolve(Some(&dir_str), Some("C:/lich"), None, downloads.path()),
            MapDbSource::GameDataDir(scan_dir.path().to_path_buf())
        );
    }

    #[test]
    fn source_resolution_uses_running_lich_install_last() {
        let empty = tempfile::tempdir().unwrap();
        let auto = tempfile::tempdir().unwrap();
        let auto_game_dir = auto.path().join("data").join("GSIV");
        std::fs::create_dir_all(&auto_game_dir).unwrap();
        std::fs::write(auto_game_dir.join("map-123.json"), "[]").unwrap();

        assert_eq!(
            resolve_source_with_auto(None, None, Some("prime"), empty.path(), || {
                vec![auto.path().to_path_buf()]
            })
            .mapdb,
            MapDbSource::GameDataDir(auto_game_dir)
        );

        let explicit = empty.path().join("explicit.json");
        std::fs::write(&explicit, "[]").unwrap();
        assert_eq!(
            resolve_source_with_auto(
                explicit.to_str(),
                None,
                Some("prime"),
                empty.path(),
                Vec::new,
            )
            .mapdb,
            MapDbSource::File(explicit)
        );

        let downloads = tempfile::tempdir().unwrap();
        let downloaded = downloads.path().join("mapdb-v1.json");
        std::fs::write(&downloaded, "[]").unwrap();
        assert_eq!(
            resolve_source_with_auto(None, None, Some("prime"), downloads.path(), Vec::new).mapdb,
            MapDbSource::File(downloaded)
        );

        let configured = empty.path().join("configured-lich");
        assert_eq!(
            resolve_source_with_auto(
                None,
                configured.to_str(),
                Some("prime"),
                empty.path(),
                || panic!("auto-discovery must not run when lich_dir is configured"),
            )
            .mapdb,
            MapDbSource::GameDataDir(configured.join("data").join("GSIV"))
        );
    }

    #[test]
    fn auto_discovery_skips_installs_without_current_game_map_data() {
        let empty = tempfile::tempdir().unwrap();
        let wrong_game = tempfile::tempdir().unwrap();
        let right_game = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(wrong_game.path().join("data").join("DR")).unwrap();
        std::fs::write(
            wrong_game.path().join("data").join("DR").join("map-1.json"),
            "[]",
        )
        .unwrap();
        let right_game_dir = right_game.path().join("data").join("GSIV");
        std::fs::create_dir_all(&right_game_dir).unwrap();
        std::fs::write(right_game_dir.join("map-2.json"), "[]").unwrap();

        assert_eq!(
            resolve_source_with_auto(None, None, Some("prime"), empty.path(), || {
                vec![
                    wrong_game.path().to_path_buf(),
                    right_game.path().to_path_buf(),
                ]
            })
            .mapdb,
            MapDbSource::GameDataDir(right_game_dir)
        );
    }

    #[test]
    fn auto_discovery_fails_closed_when_viable_installs_are_ambiguous() {
        let downloads = tempfile::tempdir().unwrap();
        let downloaded = downloads.path().join("mapdb-v1.json");
        std::fs::write(&downloaded, "[]").unwrap();
        let empty = tempfile::tempdir().unwrap();
        let first = viable_lich_install("GSIV", "first.png");
        let second = viable_lich_install("GSIV", "second.png");

        let resolution =
            resolve_source_with_auto(None, None, Some("prime"), downloads.path(), || {
                vec![first.path().to_path_buf(), second.path().to_path_buf()]
            });

        assert_eq!(resolution.mapdb, MapDbSource::File(downloaded));
        assert_eq!(resolution.classic_maps_dir, None);
        assert_eq!(resolution.warning.as_deref(), Some(AMBIGUOUS_LICH_WARNING));

        let no_download = resolve_source_with_auto(None, None, Some("prime"), empty.path(), || {
            vec![first.path().to_path_buf(), second.path().to_path_buf()]
        });
        assert_eq!(no_download.mapdb, MapDbSource::Unconfigured);
        assert_eq!(no_download.classic_maps_dir, None);
        assert_eq!(no_download.warning.as_deref(), Some(AMBIGUOUS_LICH_WARNING));
    }

    #[test]
    fn explicit_lich_dir_wins_over_ambiguous_auto_art_discovery() {
        let downloads = tempfile::tempdir().unwrap();
        std::fs::write(downloads.path().join("mapdb-v1.json"), "[]").unwrap();
        let configured = viable_lich_install("GSIV", "configured.png");

        let resolution = resolve_source_with_auto(
            None,
            configured.path().to_str(),
            Some("prime"),
            downloads.path(),
            || panic!("explicit map.lich_dir must suppress automatic discovery"),
        );

        assert_eq!(
            resolution.classic_maps_dir,
            Some(configured.path().join("maps"))
        );
        assert!(resolution.warning.is_none());
    }

    #[test]
    fn auto_discovery_uses_the_only_install_viable_for_this_game() {
        let downloads = tempfile::tempdir().unwrap();
        std::fs::write(downloads.path().join("mapdb-v1.json"), "[]").unwrap();
        let wrong_game = viable_lich_install("DR", "wrong.png");
        let right_game = viable_lich_install("GSIV", "right.png");

        let resolution =
            resolve_source_with_auto(None, None, Some("prime"), downloads.path(), || {
                vec![
                    wrong_game.path().to_path_buf(),
                    right_game.path().to_path_buf(),
                ]
            });

        assert_eq!(
            resolution.classic_maps_dir,
            Some(right_game.path().join("maps"))
        );
        assert!(resolution.warning.is_none());
    }

    fn viable_lich_install(game_dir: &str, image: &str) -> tempfile::TempDir {
        let install = tempfile::tempdir().unwrap();
        let data = install.path().join("data").join(game_dir);
        let maps = install.path().join("maps");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&maps).unwrap();
        std::fs::write(data.join("map-1.json"), "[]").unwrap();
        std::fs::write(maps.join(image), b"image").unwrap();
        install
    }

    #[test]
    fn dangling_explicit_file_fails_with_folder_mode_hint() {
        let tmp = std::env::temp_dir();
        let mut svc = MapService::new(
            tmp.join("vellum-map-svc-dangle-test"),
            tmp.join("vellum-map-svc-dangle-overrides.json"),
        );
        svc.ensure_db(MapDbSource::File(
            tmp.join("vellum-nonexistent-map-1785611370.json"),
        ));
        assert_eq!(svc.db_state(), DbState::Failed);
        let err = svc.db_error.as_deref().unwrap();
        assert!(err.contains("not found"), "got: {err}");
        assert!(err.contains("folder"), "hint missing: {err}");
    }

    #[test]
    fn service_is_inert_without_a_db() {
        let tmp = std::env::temp_dir();
        let mut svc = MapService::new(
            tmp.join("vellum-map-svc-test"),
            tmp.join("vellum-map-svc-test-overrides.json"),
        );
        // Room reports before the db loads are remembered, not resolved.
        svc.note_room(Some(4577251), None, Default::default());
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
        svc.note_room(Some(731009), Some(369), Default::default());
        assert_eq!(svc.current_room_id, Some(369));
        assert_eq!(svc.current_location.as_deref(), Some("Mist Harbor"));

        // Inside an unmapped shop: unknown uid, Lich placeholder id 0.
        svc.note_room(Some(633107), Some(0), Default::default());
        assert_eq!(svc.current_room_id, Some(369), "id 0 must not be trusted");
        assert_eq!(svc.current_location.as_deref(), Some("Mist Harbor"));

        // Genuinely in the Atrium: its uid resolves to room 0 directly.
        svc.note_room(Some(13107012), Some(0), Default::default());
        assert_eq!(svc.current_room_id, Some(0));
        assert_eq!(svc.current_location.as_deref(), Some("the Moonglae Inn"));
    }

    /// The full ghost lifecycle: an unmapped room becomes a session sketch
    /// anchored on the held room, deeper rooms extend the cluster, exits
    /// arriving a line late refresh the sketch, and stepping back onto the
    /// map ends ghost mode without discarding the sketch.
    #[test]
    fn unmapped_rooms_sketch_an_anchored_ghost_cluster() {
        use crate::core::ghost_rooms::RoomSnapshot;
        let tmp = std::env::temp_dir();
        let db_path = tmp.join("vellum-map-svc-ghost-test.json");
        std::fs::write(
            &db_path,
            r#"[
                {"id": 369, "uid": [731009], "location": "Mist Harbor",
                 "title": ["[East Row, Fel Road]"], "wayto": {}, "paths": "Obvious paths: north"}
            ]"#,
        )
        .unwrap();
        let mut svc = MapService::new(
            tmp.join("vellum-map-svc-ghost-cache"),
            tmp.join("vellum-map-svc-ghost-overrides.json"),
        );
        svc.mapdb = Some(Arc::new(MapDb::load(&db_path).unwrap()));

        svc.note_room(Some(731009), Some(369), Default::default());
        assert_eq!(svc.current_ghost, None);

        // "go shop" into an unmapped interior: ghost anchored on the street.
        svc.note_command("go shop");
        svc.note_room(
            Some(633107),
            Some(0),
            RoomSnapshot {
                title: Some("[Shop, Front]".into()),
                exits: vec![],
                ..Default::default()
            },
        );
        assert_eq!(svc.current_ghost, Some(633107));
        assert_eq!(svc.current_room_id, Some(369), "anchor room is held");
        let front = svc.ghosts().get(633107).unwrap();
        assert_eq!(front.anchor.as_ref().unwrap().room_id, 369);
        assert_eq!(
            front.anchor.as_ref().unwrap().command.as_deref(),
            Some("go shop")
        );

        // Exits often arrive a line after the nav tag: same ids, richer data.
        svc.note_room(
            Some(633107),
            Some(0),
            RoomSnapshot {
                title: Some("[Shop, Front]".into()),
                exits: vec!["out".into()],
                ..Default::default()
            },
        );
        assert_eq!(svc.ghosts().get(633107).unwrap().exits, vec!["out"]);

        // Deeper in: ghost→ghost edge labeled with the crossing command.
        svc.note_command("go curtain");
        svc.note_room(
            Some(633108),
            Some(0),
            RoomSnapshot {
                title: Some("[Shop, Back]".into()),
                exits: vec![],
                ..Default::default()
            },
        );
        assert_eq!(svc.current_ghost, Some(633108));
        assert_eq!(svc.ghosts().len(), 2);

        // Back out to the street: ghost mode ends, the sketch survives.
        svc.note_room(Some(731009), Some(369), Default::default());
        assert_eq!(svc.current_ghost, None);
        assert_eq!(svc.current_room_id, Some(369));
        assert_eq!(svc.ghosts().len(), 2);
    }

    /// Curated membership rewires switching: covered rooms resolve to the
    /// curated slug, un-covered clusters to their satellite key, and tiny
    /// one-room closets hold the base map they portal from. Without curated
    /// data everything stays location-bucketed (the other tests).
    #[test]
    fn curated_membership_drives_room_to_map_switching() {
        let tmp = std::env::temp_dir();
        let db_path = tmp.join("vellum-map-svc-membership-test.json");
        std::fs::write(
            &db_path,
            r#"[
                {"id": 1, "uid": [100], "location": "Town",
                 "title": ["[Town, Square]"],
                 "wayto": {"10": "go well", "20": "go closet"},
                 "timeto": {"10": 0.2, "20": 0.2}, "paths": ""},
                {"id": 10, "uid": [200], "location": "Town",
                 "title": ["[Town, Well Top]"], "wayto": {"1": "out", "11": "down"},
                 "timeto": {"1": 0.2, "11": 0.2}, "paths": ""},
                {"id": 11, "uid": [201], "location": "Town",
                 "title": ["[Town, Well Bottom]"], "wayto": {"10": "up"},
                 "timeto": {"10": 0.2}, "paths": ""},
                {"id": 20, "uid": [300], "location": "Town",
                 "title": ["[Town, Closet]"], "wayto": {"1": "out"},
                 "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut svc = MapService::new(
            tmp.join("vellum-map-svc-membership-cache"),
            tmp.join("vellum-map-svc-membership-overrides.json"),
        );
        svc.mapdb = Some(Arc::new(MapDb::load(&db_path).unwrap()));
        svc.set_curated(
            crate::core::curated_maps::CuratedMaps::from_saga_layouts_json(
                r#"{"layoutVersion": 1, "layouts": {"town||i:1": {"pos": [[100, 0, 0]]}}}"#,
            )
            .unwrap(),
        );
        // Membership builds on the worker; wait for it to land.
        for _ in 0..500 {
            svc.poll();
            if svc.membership().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(svc.membership().is_some(), "membership build timed out");

        // While the build was pending, resolution held (nothing generated).
        svc.note_room(Some(100), Some(1), Default::default());
        assert_eq!(
            svc.current_location.as_deref(),
            Some("town"),
            "covered → curated slug"
        );
        assert_eq!(svc.display_name("town"), "Town");

        svc.note_room(Some(200), Some(10), Default::default());
        assert_eq!(
            svc.current_location.as_deref(),
            Some("sat-200"),
            "well → satellite"
        );

        svc.note_room(Some(300), Some(20), Default::default());
        assert_eq!(
            svc.current_location.as_deref(),
            Some("town"),
            "tiny closet holds the base map"
        );
        assert_eq!(
            svc.current_room_id,
            Some(20),
            "but the room itself is tracked"
        );
    }

    /// Promote moves a map's personal edits into the staging export, clears
    /// them from the personal store (group offsets would double-apply
    /// across layers otherwise), and the community layer serves them for
    /// the rest of the session.
    #[test]
    fn promote_moves_personal_edits_to_staging_and_community() {
        let dir = tempfile::tempdir().unwrap();
        let mut svc = MapService::new(
            dir.path().join("cache"),
            dir.path().join("map_overrides.json"),
        );
        svc.apply_override_edit(OverrideEdit::GroupOffset {
            location: "town".into(),
            anchor: 100,
            delta: Cell { x: 2, y: 1 },
        });
        svc.apply_override_edit(OverrideEdit::GroupName {
            location: "sat-200".into(),
            anchor: 200,
            name: Some("The Well".into()),
        });

        let (promoted, staging_path) = svc.promote_overrides(Some("town")).unwrap();
        assert_eq!(promoted, vec!["town".to_string()]);
        assert!(
            svc.overrides_for("town").is_none(),
            "personal entry cleared"
        );
        assert!(
            svc.overrides_for("sat-200").is_some(),
            "other maps untouched"
        );
        assert_eq!(
            svc.community_overrides.locations["town"].group_offsets[&100],
            Cell { x: 2, y: 1 },
            "community layer serves the promoted edits immediately"
        );
        let staged = overrides::load(&staging_path);
        assert_eq!(
            staged.locations["town"].group_offsets[&100],
            Cell { x: 2, y: 1 }
        );

        // A fresh service (= app restart) loads the staging file as a
        // community layer: the promotion survives without any rebuild.
        let restarted = MapService::new(
            dir.path().join("cache"),
            dir.path().join("map_overrides.json"),
        );
        assert_eq!(
            restarted.community_overrides.locations["town"].group_offsets[&100],
            Cell { x: 2, y: 1 },
            "promoted edits persist across restart via the staging layer"
        );

        // `all` sweeps the rest; nothing left to promote errors cleanly.
        let (rest, _) = svc.promote_overrides(None).unwrap();
        assert_eq!(rest, vec!["sat-200".to_string()]);
        assert!(svc.promote_overrides(None).is_err());
        let staged = overrides::load(&staging_path);
        assert_eq!(
            staged.locations.len(),
            2,
            "staging accumulates across promotes"
        );
    }

    /// Promote → edit → promote again must FOLD the new deltas onto the
    /// staged curation, not replace it: after the first promote the
    /// personal layer holds only the edits made since, and wholesale
    /// replacement threw the whole staged map away on the second promote
    /// (the .mappromote map-mangling bug).
    #[test]
    fn second_promote_merges_into_staged_curation() {
        let dir = tempfile::tempdir().unwrap();
        let mut svc = MapService::new(
            dir.path().join("cache"),
            dir.path().join("map_overrides.json"),
        );
        // Session 1: real curation — an offset, a pin, a name — promoted.
        svc.apply_override_edit(OverrideEdit::GroupOffset {
            location: "town".into(),
            anchor: 100,
            delta: Cell { x: 2, y: 1 },
        });
        svc.apply_override_edit(OverrideEdit::RoomPin {
            location: "town".into(),
            key: 29217,
            pin: Some(Cell { x: 5, y: 5 }),
        });
        svc.apply_override_edit(OverrideEdit::GroupName {
            location: "town".into(),
            anchor: 100,
            name: Some("Hornwort Cavern".into()),
        });
        let (_, staging_path) = svc.promote_overrides(Some("town")).unwrap();

        // One incremental nudge afterwards, promoted again.
        svc.apply_override_edit(OverrideEdit::RoomPin {
            location: "town".into(),
            key: 29217,
            pin: Some(Cell { x: 4, y: 5 }),
        });
        svc.promote_overrides(Some("town")).unwrap();

        let staged = overrides::load(&staging_path);
        let town = &staged.locations["town"];
        assert_eq!(
            town.room_pins[&29217],
            Cell { x: 4, y: 5 },
            "the nudge lands"
        );
        assert_eq!(
            town.group_offsets[&100],
            Cell { x: 2, y: 1 },
            "the first promote's offset survives the second"
        );
        assert_eq!(
            town.names[&100], "Hornwort Cavern",
            "the first promote's name survives the second"
        );
        assert_eq!(
            svc.community_overrides.locations["town"].group_offsets[&100],
            Cell { x: 2, y: 1 },
            "in-memory community layer serves the merged entry"
        );
    }

    /// Community layering: shipped defaults under a mapdb release's
    /// overrides (whole-location replace), personal merged on top at use.
    #[test]
    fn community_overlay_replaces_per_location() {
        let mut base = MapOverrides::default();
        base.locations
            .entry("town".into())
            .or_default()
            .names
            .insert(1, "Old".into());
        base.locations
            .entry("keep".into())
            .or_default()
            .names
            .insert(2, "Kept".into());
        let mut top = MapOverrides::default();
        top.locations
            .entry("town".into())
            .or_default()
            .names
            .insert(1, "New".into());
        let merged = overrides::overlay(base, top);
        assert_eq!(merged.locations["town"].names[&1], "New");
        assert_eq!(merged.locations["keep"].names[&2], "Kept");
    }

    /// Legacy location-keyed overrides re-home to the map now holding their
    /// anchor uid; unresolvable anchors stay put under the old key.
    #[test]
    fn overrides_remap_to_membership_keys() {
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [100], "location": "Town",
                 "title": ["[Town, Square]"], "wayto": {"10": "go well"},
                 "timeto": {"10": 0.2}, "paths": ""},
                {"id": 10, "uid": [200], "location": "Town",
                 "title": ["[Town, Well Top]"], "wayto": {"1": "out", "11": "down"},
                 "timeto": {"1": 0.2, "11": 0.2}, "paths": ""},
                {"id": 11, "uid": [201], "location": "Town",
                 "title": ["[Town, Well Bottom]"], "wayto": {"10": "up"},
                 "timeto": {"10": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let curated = crate::core::curated_maps::CuratedMaps::from_saga_layouts_json(
            r#"{"layoutVersion": 1, "layouts": {"town||i:1": {"pos": [[100, 0, 0]]}}}"#,
        )
        .unwrap();
        let membership = crate::core::membership::Membership::build(&db, &curated);

        let mut store = MapOverrides::default();
        let entry = store.locations.entry("Town".to_string()).or_default();
        entry.group_offsets.insert(100, Cell { x: 1, y: 0 }); // → curated "town"
        entry.group_offsets.insert(200, Cell { x: 0, y: 2 }); // → sat-200
        entry.names.insert(999_999, "Nowhere".into()); // unresolvable: stays

        assert!(remap_overrides(&mut store, &db, &membership));
        assert_eq!(
            store.locations["town"].group_offsets[&100],
            Cell { x: 1, y: 0 }
        );
        assert_eq!(
            store.locations["sat-200"].group_offsets[&200],
            Cell { x: 0, y: 2 }
        );
        assert_eq!(store.locations["Town"].names[&999_999], "Nowhere");
        assert!(store.locations["Town"].group_offsets.is_empty());
        // Second pass is a no-op: everything resolvable already moved.
        assert!(!remap_overrides(&mut store, &db, &membership));
    }

    /// Rooms that arrive with no uid and no Lich id (interfaces that never
    /// see one) resolve by content — title, description, exits, adjacency —
    /// and only when unambiguous. A wrong room is worse than holding.
    #[test]
    fn uidless_rooms_resolve_by_content_when_unambiguous() {
        use crate::core::ghost_rooms::RoomSnapshot;
        let tmp = std::env::temp_dir();
        let db_path = tmp.join("vellum-map-svc-content-test.json");
        std::fs::write(
            &db_path,
            r#"[
                {"id": 10, "uid": [111], "location": "Zul Logoth",
                 "title": ["[A Dark Tunnel]"], "description": ["Rough-hewn walls."],
                 "wayto": {"11": "north"}, "timeto": {"11": 0.2},
                 "paths": "Obvious exits: north, south"},
                {"id": 11, "uid": [222], "location": "Zul Logoth",
                 "title": ["[A Dark Tunnel]"], "description": ["Rough-hewn walls."],
                 "wayto": {"10": "south", "12": "north"}, "timeto": {"10": 0.2, "12": 0.2},
                 "paths": "Obvious exits: north, south"},
                {"id": 12, "uid": [333], "location": "Zul Logoth",
                 "title": ["[Gem Shop]"], "description": ["Gems glitter on every shelf."],
                 "wayto": {"11": "out"}, "timeto": {"11": 0.2},
                 "paths": "Obvious exits: out"}
            ]"#,
        )
        .unwrap();
        let mut svc = MapService::new(
            tmp.join("vellum-map-svc-content-cache"),
            tmp.join("vellum-map-svc-content-overrides.json"),
        );
        svc.mapdb = Some(Arc::new(MapDb::load(&db_path).unwrap()));

        // Ambiguous title, no prior position: hold, never guess.
        svc.note_room(
            None,
            None,
            RoomSnapshot {
                title: Some("[A Dark Tunnel]".into()),
                ..Default::default()
            },
        );
        assert_eq!(svc.current_room_id, None);

        // A unique title resolves outright — this must not be swallowed by
        // the (None, None) == (None, None) identity dedup.
        svc.note_room(
            None,
            None,
            RoomSnapshot {
                title: Some("[Gem Shop]".into()),
                ..Default::default()
            },
        );
        assert_eq!(svc.current_room_id, Some(12));
        assert_eq!(svc.current_location.as_deref(), Some("Zul Logoth"));

        // Ambiguous title disambiguated by adjacency: from the shop the only
        // reachable tunnel is 11.
        svc.note_room(
            None,
            None,
            RoomSnapshot {
                title: Some("[A Dark Tunnel]".into()),
                ..Default::default()
            },
        );
        assert_eq!(svc.current_room_id, Some(11));

        // The same room re-reported (exits arrive a line later): stay put,
        // don't slide to the identical-looking neighbor 10.
        svc.note_room(
            None,
            None,
            RoomSnapshot {
                title: Some("[A Dark Tunnel]".into()),
                exits: vec!["north".into(), "south".into()],
                ..Default::default()
            },
        );
        assert_eq!(svc.current_room_id, Some(11));

        // A wrong description keeps the pool instead of matching nowhere,
        // and the current room still wins the tie.
        svc.note_room(
            None,
            None,
            RoomSnapshot {
                title: Some("[A Dark Tunnel]".into()),
                description: Some("Not in the mapdb at all.".into()),
                ..Default::default()
            },
        );
        assert_eq!(svc.current_room_id, Some(11));
    }

    /// Membership editing (P3): moving a room's uid to Purgatory rewrites
    /// the effective rosters, rebuilds membership, and persists; reverting
    /// restores curated placement. Purgatory materializes on first use and
    /// lists even while empty.
    #[test]
    fn membership_move_to_purgatory_and_revert() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("db.json");
        std::fs::write(
            &db_path,
            r#"[
                {"id": 1, "uid": [100], "location": "Town",
                 "title": ["[Town, Square]"], "wayto": {"2": "east"},
                 "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [101], "location": "Town",
                 "title": ["[Town, East]"], "wayto": {"1": "west"},
                 "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut svc = MapService::new(
            dir.path().join("cache"),
            dir.path().join("map_overrides.json"),
        );
        svc.mapdb = Some(Arc::new(MapDb::load(&db_path).unwrap()));
        svc.set_curated(
            crate::core::curated_maps::CuratedMaps::from_saga_layouts_json(
                r#"{"layoutVersion": 1, "layouts":
                    {"town||i:1": {"pos": [[100, 0, 0], [101, 1, 0]]}}}"#,
            )
            .unwrap(),
        );
        let wait = |svc: &mut MapService| {
            for _ in 0..500 {
                svc.poll();
                if svc.membership().is_some() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            panic!("membership build timed out");
        };
        wait(&mut svc);
        assert_eq!(svc.membership().unwrap().map_of_room(2), Some("town"));

        svc.apply_override_edit(OverrideEdit::MembershipMove {
            uids: vec![101],
            to: Some(PURGATORY_KEY.to_string()),
        });
        wait(&mut svc);
        let m = svc.membership().unwrap();
        assert_eq!(m.map_of_room(2), Some(PURGATORY_KEY), "room moved");
        assert_eq!(
            m.rooms_of_map("town"),
            Some(&[1u32][..]),
            "left the town roster"
        );
        assert!(
            m.is_curated(PURGATORY_KEY),
            "purgatory lists with the curated group"
        );
        assert_eq!(m.display_name(PURGATORY_KEY), PURGATORY_NAME);
        // Persisted.
        let reloaded =
            crate::core::layout_engine::overrides::load(&dir.path().join("map_overrides.json"));
        assert_eq!(
            reloaded.membership_moves.get(&101).map(String::as_str),
            Some(PURGATORY_KEY)
        );
        assert!(reloaded.custom_maps.contains_key(PURGATORY_KEY));

        svc.apply_override_edit(OverrideEdit::MembershipMove {
            uids: vec![101],
            to: None,
        });
        wait(&mut svc);
        let m = svc.membership().unwrap();
        assert_eq!(
            m.map_of_room(2),
            Some("town"),
            "revert restores curated placement"
        );
        assert_eq!(
            m.rooms_of_map(PURGATORY_KEY),
            Some(&[][..]),
            "purgatory stays listed, now empty"
        );
    }

    /// CreateMap + move: a user map mints from the editor and receives rooms.
    #[test]
    fn create_map_and_move_room_into_it() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("db.json");
        std::fs::write(
            &db_path,
            r#"[
                {"id": 1, "uid": [100], "location": "Town",
                 "title": ["[Town, Square]"], "wayto": {"2": "east"},
                 "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [101], "location": "Town",
                 "title": ["[Town, East]"], "wayto": {"1": "west"},
                 "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut svc = MapService::new(
            dir.path().join("cache"),
            dir.path().join("map_overrides.json"),
        );
        svc.mapdb = Some(Arc::new(MapDb::load(&db_path).unwrap()));
        svc.set_curated(
            crate::core::curated_maps::CuratedMaps::from_saga_layouts_json(
                r#"{"layoutVersion": 1, "layouts":
                    {"town||i:1": {"pos": [[100, 0, 0], [101, 1, 0]]}}}"#,
            )
            .unwrap(),
        );
        let wait = |svc: &mut MapService| {
            for _ in 0..500 {
                svc.poll();
                if svc.membership().is_some() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            panic!("membership build timed out");
        };
        wait(&mut svc);

        let key = MapService::user_map_key("My Arena Notes!");
        assert_eq!(key, "user-my-arena-notes");
        svc.apply_override_edit(OverrideEdit::CreateMap {
            key: key.clone(),
            name: "My Arena Notes!".into(),
        });
        wait(&mut svc);
        svc.apply_override_edit(OverrideEdit::MembershipMove {
            uids: vec![101],
            to: Some(key.clone()),
        });
        wait(&mut svc);
        let m = svc.membership().unwrap();
        assert_eq!(m.map_of_room(2), Some(key.as_str()));
        assert_eq!(m.display_name(&key), "My Arena Notes!");
        assert_eq!(m.rooms_of_map(&key), Some(&[2u32][..]));
    }

    /// Room-data edits (P4): tags/exits/description bake into the reloaded
    /// db; revert restores pristine data.
    #[test]
    fn room_edits_apply_and_revert() {
        use crate::core::layout_engine::overrides::{RoomDataEdit, WaytoEdit};
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("db.json");
        std::fs::write(
            &db_path,
            r#"[
                {"id": 1, "uid": [100], "location": "Town",
                 "title": ["[Town, Square]"], "description": ["Old square."],
                 "tags": ["bank"],
                 "wayto": {"2": "east"}, "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [101], "location": "Town",
                 "title": ["[Town, East]"], "wayto": {"1": "west"},
                 "timeto": {"1": 0.2}, "paths": ""},
                {"id": 3, "uid": [102], "location": "Town",
                 "title": ["[Town, Alley]"], "wayto": {},
                 "timeto": {}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let mut svc = MapService::new(
            dir.path().join("cache"),
            dir.path().join("map_overrides.json"),
        );
        svc.ensure_db(MapDbSource::File(db_path.clone()));
        let wait_db = |svc: &mut MapService| {
            for _ in 0..500 {
                svc.poll();
                if svc.mapdb().is_some() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            panic!("db load timed out");
        };
        wait_db(&mut svc);

        let mut edit = RoomDataEdit::default();
        edit.add_tags.push("acantha leaf".into());
        edit.remove_tags.push("bank".into());
        edit.wayto.insert(
            102,
            Some(WaytoEdit {
                command: "go alley".into(),
                seconds: 1.5,
            }),
        );
        edit.wayto.insert(101, None);
        edit.description = Some("A rebuilt square.".into());
        svc.apply_override_edit(OverrideEdit::RoomEdit {
            uid: 100,
            edit: Some(edit),
        });
        wait_db(&mut svc);

        let db = svc.mapdb().unwrap();
        let room = db.room(1).unwrap();
        assert_eq!(room.tags, vec!["acantha leaf"], "tag added, bank removed");
        assert_eq!(room.wayto.get(&3).map(String::as_str), Some("go alley"));
        assert_eq!(
            room.timeto.get(&3),
            Some(&crate::core::mapdb::TimeTo::Seconds(1.5))
        );
        assert!(room.wayto.get(&2).is_none(), "edge to East removed");
        assert_eq!(room.description, vec!["A rebuilt square."]);
        assert!(db.room_ids_with_tag("bank").is_empty(), "tag index rebuilt");
        assert_eq!(db.room_ids_with_tag("acantha leaf"), &[1]);

        svc.apply_override_edit(OverrideEdit::RoomEdit {
            uid: 100,
            edit: None,
        });
        wait_db(&mut svc);
        let db = svc.mapdb().unwrap();
        let room = db.room(1).unwrap();
        assert_eq!(room.tags, vec!["bank"], "pristine tags restored");
        assert_eq!(room.wayto.get(&2).map(String::as_str), Some("east"));
        assert!(room.wayto.get(&3).is_none());
        assert_eq!(room.description, vec!["Old square."]);
    }

    /// Promote carries membership moves, custom maps, and room edits into
    /// staging (room edits on full promotes), and clears them personally.
    #[test]
    fn promote_carries_membership_state() {
        use crate::core::layout_engine::overrides::RoomDataEdit;
        let dir = tempfile::tempdir().unwrap();
        let overrides_path = dir.path().join("map_overrides.json");
        let mut svc = MapService::new(dir.path().join("cache"), overrides_path.clone());
        svc.overrides
            .membership_moves
            .insert(101, PURGATORY_KEY.to_string());
        svc.overrides
            .custom_maps
            .insert(PURGATORY_KEY.to_string(), PURGATORY_NAME.to_string());
        let mut edit = RoomDataEdit::default();
        edit.add_tags.push("acantha leaf".into());
        svc.overrides.room_edits.insert(100, edit);

        let (_, staging_path) = svc.promote_overrides(None).unwrap();
        assert!(svc.overrides.membership_moves.is_empty());
        assert!(svc.overrides.custom_maps.is_empty());
        assert!(svc.overrides.room_edits.is_empty());
        let staged = crate::core::layout_engine::overrides::load(&staging_path);
        assert_eq!(
            staged.membership_moves.get(&101).map(String::as_str),
            Some(PURGATORY_KEY)
        );
        assert!(staged.custom_maps.contains_key(PURGATORY_KEY));
        assert!(staged.room_edits.contains_key(&100));
        // Served from the community layer immediately.
        assert_eq!(svc.personal_membership_move(101), None);
        assert!(
            svc.room_edit(100).is_some(),
            "effective edit survives via community"
        );
    }
}
