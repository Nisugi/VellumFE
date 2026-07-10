//! Disk-backed layout cache (spec §1): layouts are keyed by a content hash
//! of the location's rooms, so the engine runs once per mapdb build —
//! generate on entry, instant thereafter. Cached layouts are derived,
//! per-machine data and are never shared as artifacts; human curation lives
//! in the separate uid-keyed override diff (spec §8), applied after loading.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::mapdb::Room;
use super::{generate_layout, Layout};

/// Bump when the algorithms change so stale cached layouts regenerate.
/// v3: interior clusters merge into one floor plan before shelving.
pub const ENGINE_VERSION: u32 = 3;
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheOutcome {
    /// Loaded from disk without running the engine.
    Hit,
    /// Generated (and stored) because no valid cache entry existed.
    Generated,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    format_version: u32,
    engine_version: u32,
    location: String,
    /// Hex content hash of the location's rooms (see `rooms_content_hash`).
    rooms_hash: String,
    /// Hex hash of the generation-input overrides baked into this layout
    /// (edge directions, classification flips); zeros = pristine.
    #[serde(default)]
    curated: String,
    layout: Layout,
}

/// FNV-1a 64 over a canonical byte stream. Stability across runs, platforms,
/// and Rust versions is the requirement here (std's SipHash keys differ per
/// process); collisions only cost a wrong cache hit against another build of
/// the same location, and the engine version + location name also gate loads.
/// One-shot FNV-1a 64 over a byte slice (shared with the override hasher).
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h = Fnv1a::new();
    h.write(bytes);
    h.0
}

struct Fnv1a(u64);

impl Fnv1a {
    fn new() -> Self {
        Fnv1a(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Length-prefixed write so field boundaries can't alias.
    fn write_str(&mut self, s: &str) {
        self.write(&(s.len() as u64).to_le_bytes());
        self.write(s.as_bytes());
    }

    fn write_opt_str(&mut self, s: Option<&str>) {
        match s {
            Some(s) => {
                self.write(&[1]);
                self.write_str(s);
            }
            None => self.write(&[0]),
        }
    }
}

/// Deterministic content hash of a location's rooms, over every field the
/// layout engine reads. Room order does not matter (rooms are visited in
/// ascending id order); any change to layout-relevant data changes the hash.
pub fn rooms_content_hash(rooms: &[Room]) -> u64 {
    let mut sorted: Vec<&Room> = rooms.iter().collect();
    sorted.sort_by_key(|r| r.id);

    let mut h = Fnv1a::new();
    h.write(&(sorted.len() as u64).to_le_bytes());
    for room in sorted {
        h.write(&room.id.to_le_bytes());
        h.write(&(room.uid.len() as u64).to_le_bytes());
        for &uid in &room.uid {
            h.write(&uid.to_le_bytes());
        }
        h.write(&(room.title.len() as u64).to_le_bytes());
        for t in &room.title {
            h.write_str(t);
        }
        for map in [&room.wayto, &room.dirto] {
            h.write(&(map.len() as u64).to_le_bytes());
            for (&target, cmd) in map {
                h.write(&target.to_le_bytes());
                h.write_str(cmd);
            }
        }
        h.write_str(&room.paths);
        h.write_opt_str(room.climate.as_deref());
        h.write_opt_str(room.terrain.as_deref());
        h.write_opt_str(room.image.as_deref());
        match &room.image_coords {
            Some(coords) => {
                h.write(&[1]);
                for c in coords {
                    h.write(&c.to_bits().to_le_bytes());
                }
            }
            None => h.write(&[0]),
        }
    }
    h.0
}

/// A directory of cached layouts, one JSON file per (location, rooms-hash).
pub struct LayoutCache {
    dir: PathBuf,
}

impl LayoutCache {
    /// `dir` is created lazily on first store (e.g.
    /// `~/.vellum-fe/cache/layouts`, resolved by the caller).
    pub fn new(dir: PathBuf) -> Self {
        LayoutCache { dir }
    }

    /// Load the cached layout, or generate (and best-effort store) it. The
    /// engine is deterministic, so a hit is byte-equivalent to regenerating.
    /// `gen_overrides` is the generation-input override subset baked into
    /// the produced layout (`LocationOverrides::generation_subset`).
    pub fn get_or_generate(
        &self,
        location: &str,
        rooms: &[Room],
        gen_overrides: &super::LocationOverrides,
    ) -> (Layout, CacheOutcome) {
        let hash = rooms_content_hash(rooms);
        let curated = gen_overrides.curated_hash();
        if let Some(layout) = self.load(location, hash, curated) {
            return (layout, CacheOutcome::Hit);
        }
        let mut owned = rooms.to_vec();
        let layout = super::generate_layout_curated(&mut owned, gen_overrides);
        if let Err(e) = self.store(location, hash, curated, &layout) {
            tracing::warn!("layout cache store failed for {location}: {e}");
        }
        (layout, CacheOutcome::Generated)
    }

    /// A valid entry must match the format, engine version, location, and
    /// content hash; anything else (including parse errors) is a miss.
    pub fn load(&self, location: &str, rooms_hash: u64, curated: u64) -> Option<Layout> {
        let path = self.entry_path(location, rooms_hash);
        let json = std::fs::read_to_string(&path).ok()?;
        let file: CacheFile = serde_json::from_str(&json).ok()?;
        (file.format_version == FORMAT_VERSION
            && file.engine_version == ENGINE_VERSION
            && file.location == location
            && file.rooms_hash == format!("{rooms_hash:016x}")
            && file.curated == format!("{curated:016x}"))
        .then_some(file.layout)
    }

    /// Write the entry and prune other (stale-hash) entries for the same
    /// location. Returns the entry path.
    pub fn store(
        &self,
        location: &str,
        rooms_hash: u64,
        curated: u64,
        layout: &Layout,
    ) -> io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.entry_path(location, rooms_hash);
        let file = CacheFile {
            format_version: FORMAT_VERSION,
            engine_version: ENGINE_VERSION,
            location: location.to_owned(),
            rooms_hash: format!("{rooms_hash:016x}"),
            curated: format!("{curated:016x}"),
            layout: layout.clone(),
        };
        let json = serde_json::to_string(&file)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        // Write-then-rename so a crash never leaves a torn entry.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;

        self.prune_stale(location, &path);
        Ok(path)
    }

    fn entry_path(&self, location: &str, rooms_hash: u64) -> PathBuf {
        self.dir
            .join(format!("{}-{rooms_hash:016x}.json", slug(location)))
    }

    /// Remove other entries for this location (older mapdb builds).
    fn prune_stale(&self, location: &str, keep: &Path) {
        let prefix = format!("{}-", slug(location));
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path == keep {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // Only `<slug>-<16 hex>.json` siblings; never touch other files.
            let Some(hash_part) = name
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(".json"))
            else {
                continue;
            };
            if hash_part.len() == 16 && hash_part.bytes().all(|b| b.is_ascii_hexdigit()) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// Filesystem-safe location name: lowercased, runs of non-alphanumerics
/// collapsed to single hyphens ("Wehnimer's Landing" → "wehnimer-s-landing").
fn slug(location: &str) -> String {
    let mut out = String::with_capacity(location.len());
    let mut pending_sep = false;
    for c in location.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_sep = true;
        }
    }
    if out.is_empty() {
        out.push_str("location");
    }
    out
}

/// Hash of the raw layout-relevant fields, exposed for callers that track
/// mapdb identity themselves (e.g. to skip re-hashing between rooms of the
/// same location).
pub fn location_cache_key(rooms: &[Room]) -> String {
    format!("{:016x}", rooms_content_hash(rooms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Wehnimer's Landing"), "wehnimer-s-landing");
        assert_eq!(slug("the Atoll"), "the-atoll");
        assert_eq!(slug("Ta'Illistim"), "ta-illistim");
        assert_eq!(slug("---"), "location");
    }

    #[test]
    fn fnv_reference_values() {
        // Published FNV-1a 64 test vectors.
        let mut h = Fnv1a::new();
        h.write(b"");
        assert_eq!(h.0, 0xcbf29ce484222325);
        let mut h = Fnv1a::new();
        h.write(b"a");
        assert_eq!(h.0, 0xaf63dc4c8601ec8c);
    }
}
