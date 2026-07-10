//! Direction analysis — port of `connection-analyzer.js`.
//!
//! Resolves the direction of a `wayto` edge from curated `dirto` overrides,
//! the movement command text, or the reverse edge (spec §3).

use serde::{Deserialize, Serialize};

use super::mapdb::{Room, RoomTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    North,
    South,
    East,
    West,
    Northeast,
    Northwest,
    Southeast,
    Southwest,
    Up,
    Down,
}

impl Dir {
    /// The 10 recognized cardinal names (note: `out` is NOT one of them).
    pub fn from_exact(s: &str) -> Option<Dir> {
        Some(match s {
            "north" => Dir::North,
            "south" => Dir::South,
            "east" => Dir::East,
            "west" => Dir::West,
            "northeast" => Dir::Northeast,
            "northwest" => Dir::Northwest,
            "southeast" => Dir::Southeast,
            "southwest" => Dir::Southwest,
            "up" => Dir::Up,
            "down" => Dir::Down,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Dir::North => "north",
            Dir::South => "south",
            Dir::East => "east",
            Dir::West => "west",
            Dir::Northeast => "northeast",
            Dir::Northwest => "northwest",
            Dir::Southeast => "southeast",
            Dir::Southwest => "southwest",
            Dir::Up => "up",
            Dir::Down => "down",
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::North => Dir::South,
            Dir::South => Dir::North,
            Dir::East => Dir::West,
            Dir::West => Dir::East,
            Dir::Northeast => Dir::Southwest,
            Dir::Southwest => Dir::Northeast,
            Dir::Northwest => Dir::Southeast,
            Dir::Southeast => Dir::Northwest,
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
        }
    }

    /// Grid offset. Up/down borrow the N/S offsets as placement conveniences.
    pub fn offset(self) -> (i32, i32) {
        match self {
            Dir::North | Dir::Up => (0, -1),
            Dir::South | Dir::Down => (0, 1),
            Dir::East => (1, 0),
            Dir::West => (-1, 0),
            Dir::Northeast => (1, -1),
            Dir::Northwest => (-1, -1),
            Dir::Southeast => (1, 1),
            Dir::Southwest => (-1, 1),
        }
    }

    /// True 2D geometry only — up/down are excluded from validation and
    /// optimization.
    pub fn is_compass(self) -> bool {
        !matches!(self, Dir::Up | Dir::Down)
    }
}

/// Scan order for extracting a direction from command text: longest names
/// first so "northeast" wins over "north".
const SCAN_ORDER: [Dir; 10] = [
    Dir::Northeast,
    Dir::Northwest,
    Dir::Southeast,
    Dir::Southwest,
    Dir::North,
    Dir::South,
    Dir::East,
    Dir::West,
    Dir::Down,
    Dir::Up,
];

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `\b word \b` containment, ASCII word semantics like JS regex `\b`.
fn contains_word(haystack: &str, word: &str) -> bool {
    let hay = haystack.as_bytes();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(word) {
        let i = start + pos;
        let end = i + word.len();
        let before_ok = i == 0 || !is_word_byte(hay[i - 1]);
        let after_ok = end >= hay.len() || !is_word_byte(hay[end]);
        if before_ok && after_ok {
            return true;
        }
        start = i + 1;
    }
    false
}

fn lower_trim(s: &str) -> String {
    s.trim().to_lowercase()
}

/// A curated dirto entry that resolves to a usable cardinal, applying the
/// `cross-group` / `none` / `skip` filtering. Returns the raw lowered value
/// too so callers can distinguish "present but unusable".
fn usable_dirto(room: &Room, target_id: u32) -> Option<(String, Option<Dir>)> {
    let raw = room.dirto.get(&target_id)?;
    if raw.is_empty() {
        // JS truthiness: empty string behaves like no dirto at all.
        return None;
    }
    let lowered = lower_trim(raw);
    let dir = if lowered != "none" && lowered != "skip" {
        Dir::from_exact(&lowered)
    } else {
        None
    };
    Some((lowered, dir))
}

/// Port of `ConnectionAnalyzer.getDirectionForConnection`.
pub fn direction_for_connection(room: &Room, target_id: u32, lookup: &RoomTable) -> Option<Dir> {
    // 1. Curated dirto override.
    if let Some((lowered, dir)) = usable_dirto(room, target_id) {
        if lowered == "cross-group" {
            return None; // don't use for positioning
        }
        if let Some(d) = dir {
            return Some(d);
        }
        // none/skip/unrecognized: fall through to wayto.
    }

    // 2. The movement command itself.
    if let Some(wayto) = room.wayto.get(&target_id).filter(|w| !w.is_empty()) {
        let cmd = lower_trim(wayto);

        // Stringprocs are only usable via a dirto override.
        if cmd.starts_with(";e") {
            if let Some((_, dir)) = usable_dirto(room, target_id) {
                return dir;
            }
            return None;
        }

        if let Some(d) = Dir::from_exact(&cmd) {
            return Some(d);
        }

        // Word-boundary scan: "go northeast gate" → northeast,
        // "go upper hallway" → nothing.
        for d in SCAN_ORDER {
            if contains_word(&cmd, d.name()) {
                return Some(d);
            }
        }
    }

    // 3. Reverse inference from the target's edge back to us.
    infer_direction_from_reverse(room, target_id, lookup)
}

/// Port of `ConnectionAnalyzer.inferDirectionFromReverse`: a bare cardinal on
/// the reverse edge reverses; a curated reverse dirto is authoritative either
/// way (never guess past none/skip/cross-group). Extracted word-boundary hints
/// are too weak to reverse.
fn infer_direction_from_reverse(room: &Room, target_id: u32, lookup: &RoomTable) -> Option<Dir> {
    let target = lookup.get(target_id)?;

    if let Some(back_dirto) = target.dirto.get(&room.id).filter(|d| !d.is_empty()) {
        return Dir::from_exact(&lower_trim(back_dirto)).map(Dir::opposite);
    }

    let back_wayto = target.wayto.get(&room.id)?;
    Dir::from_exact(&lower_trim(back_wayto)).map(Dir::opposite)
}

/// Every edge's resolved direction, computed once up front. Direction
/// analysis is a pure function of the mapdb, and every pipeline stage queries
/// it along `wayto` edges whose target is inside the selection, so one pass
/// covers all later lookups (this is purely a performance cache — semantics
/// are identical to calling `direction_for_connection` each time).
pub struct DirectionMap {
    map: std::collections::HashMap<(u32, u32), Dir>,
}

impl DirectionMap {
    pub fn build(lookup: &RoomTable) -> DirectionMap {
        let mut map = std::collections::HashMap::new();
        for room in lookup.rooms() {
            for &target_id in room.wayto.keys() {
                if !lookup.contains(target_id) {
                    continue;
                }
                if let Some(dir) = direction_for_connection(room, target_id, lookup) {
                    map.insert((room.id, target_id), dir);
                }
            }
        }
        DirectionMap { map }
    }

    pub fn get(&self, from: u32, to: u32) -> Option<Dir> {
        self.map.get(&(from, to)).copied()
    }

    /// Apply curation edge overrides (spec §8) before positioning:
    /// `Direction` forces a heading on whichever wayto edges exist between
    /// the pair, `Connector` strips any inferred heading, `Hide`/`Dash` are
    /// presentation matters and change nothing here. Unresolvable keys are
    /// skipped silently.
    pub fn apply_edge_overrides(
        &mut self,
        lookup: &RoomTable,
        edges: &[super::overrides::EdgeOverride],
    ) {
        use super::overrides::EdgeAction;
        if edges.is_empty() {
            return;
        }
        let keys = super::overrides::room_key_index(lookup);
        for edge in edges {
            let (Some(&a), Some(&b)) = (keys.get(&edge.a), keys.get(&edge.b)) else {
                continue;
            };
            match edge.action {
                EdgeAction::Hide | EdgeAction::Dash => {}
                EdgeAction::Connector => {
                    self.map.remove(&(a, b));
                    self.map.remove(&(b, a));
                }
                EdgeAction::Direction(dir) => {
                    let has = |from: u32, to: u32| {
                        lookup
                            .get(from)
                            .map(|r| r.wayto.contains_key(&to))
                            .unwrap_or(false)
                    };
                    if has(a, b) {
                        self.map.insert((a, b), dir);
                    }
                    if has(b, a) {
                        self.map.insert((b, a), dir.opposite());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_boundaries() {
        assert!(contains_word("go northeast gate", "northeast"));
        assert!(!contains_word("go upper hallway", "up"));
        assert!(contains_word("climb up", "up"));
        assert!(!contains_word("go soupbone", "up"));
    }

    #[test]
    fn longest_direction_wins() {
        // "northeast" contains "north" and "east" as substrings but not as
        // words, so the scan order only matters for strings holding several
        // real words; verify "north" does not fire inside "northeast".
        assert!(!contains_word("northeast", "north"));
    }
}
