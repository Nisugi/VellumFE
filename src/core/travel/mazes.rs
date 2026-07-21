//! Curated maze definitions — regions where the mapdb's movement edges are
//! junk (the game scrambles movement) and the walk executor must use a
//! per-maze strategy instead of stepping edges. The pilot strategy is
//! "pathcode": each character asks an NPC at the entrance for a personal
//! route ("ask beyor about path" → "Your route is: go clearing, west, ...")
//! and walks it verbatim, recovering with `search` per the NPC's own
//! instructions.
//!
//! Loaded once per process like travel overrides:
//! `defaults/globals/mazes.toml` (embedded, ships with the Ranger Guild
//! pilot entry) with `~/.vellum-fe/mazes.toml` entries replacing shipped
//! ones by name. Long-term these definitions belong in the mapdb as
//! `meta:maze` tags curated through the submission pipeline; the TOML is
//! the pilot's vehicle.

use std::collections::HashSet;
use std::sync::LazyLock;

const DEFAULT_MAZES: &str = include_str!("../../../defaults/globals/mazes.toml");

#[derive(Debug, Clone)]
pub struct MazeDef {
    /// Stable identifier; pathcodes persist under this key.
    pub name: String,
    /// Rooms whose movement edges must never be walked normally.
    pub rooms: HashSet<u32>,
    /// Room the route is walked from.
    pub start: u32,
    /// Room just outside the maze where the pathcode NPC stands.
    pub entrance: u32,
    /// Command that makes the NPC reveal the character's route.
    pub ask: String,
}

#[derive(serde::Deserialize)]
struct FileFormat {
    #[serde(default)]
    maze: Vec<MazeEntry>,
}

#[derive(serde::Deserialize)]
struct MazeEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    rooms: Vec<u32>,
    start: u32,
    entrance: u32,
    ask: String,
}

fn parse_table(source: &str, origin: &str, table: &mut Vec<MazeDef>) {
    let parsed: FileFormat = match toml::from_str(source) {
        Ok(parsed) => parsed,
        Err(e) => {
            tracing::warn!("{origin}: maze definitions ignored (parse error: {e})");
            return;
        }
    };
    for entry in parsed.maze {
        if entry.kind != "pathcode" {
            tracing::warn!(
                "{origin}: maze '{}' skipped (unsupported type '{}')",
                entry.name,
                entry.kind
            );
            continue;
        }
        if entry.rooms.is_empty() || entry.ask.trim().is_empty() {
            tracing::warn!("{origin}: maze '{}' skipped (empty rooms or ask)", entry.name);
            continue;
        }
        let def = MazeDef {
            name: entry.name,
            rooms: entry.rooms.into_iter().collect(),
            start: entry.start,
            entrance: entry.entrance,
            ask: entry.ask,
        };
        // User entries replace shipped ones by name.
        if let Some(existing) = table.iter_mut().find(|m| m.name == def.name) {
            *existing = def;
        } else {
            table.push(def);
        }
    }
}

static TABLE: LazyLock<Vec<MazeDef>> = LazyLock::new(|| {
    let mut table = Vec::new();
    parse_table(DEFAULT_MAZES, "defaults", &mut table);
    #[cfg(not(test))]
    if let Ok(base) = crate::config::Config::base_dir() {
        let user_path = base.join("mazes.toml");
        if let Ok(source) = std::fs::read_to_string(&user_path) {
            parse_table(&source, "mazes.toml", &mut table);
        }
    }
    table
});

/// Every known maze.
pub fn all() -> &'static [MazeDef] {
    &TABLE
}

/// The maze containing `room`, if any.
pub fn maze_containing(room: u32) -> Option<&'static MazeDef> {
    TABLE.iter().find(|m| m.rooms.contains(&room))
}

/// The maze whose NPC entrance is `room`, if any (pathcode capture uses
/// this to decide which maze a freshly heard route belongs to).
pub fn maze_at_entrance(room: u32) -> Option<&'static MazeDef> {
    TABLE.iter().find(|m| m.entrance == room)
}

/// Parse the pathcode NPC's response line into the route's literal
/// commands. Format (Master Tracker Beyorci, captured live 2026-07-21):
/// `Your route is:  go clearing, west, west, south, go path, southeast.  If
/// you become lost on the way, ...` — commands up to the first period,
/// comma-separated, sendable verbatim.
pub fn parse_pathcode_line(line: &str) -> Option<Vec<String>> {
    let idx = line.find("Your route is:")?;
    let tail = &line[idx + "Your route is:".len()..];
    let route_text = tail.split('.').next()?.trim();
    let commands: Vec<String> = route_text
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
        .collect();
    (!commands.is_empty()).then_some(commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_defaults_carry_the_ranger_pilot() {
        let mut table = Vec::new();
        parse_table(DEFAULT_MAZES, "defaults", &mut table);
        let ranger = table
            .iter()
            .find(|m| m.name == "ranger-guild-mist-harbor")
            .expect("pilot entry ships");
        assert_eq!(ranger.start, 15606);
        assert_eq!(ranger.entrance, 20886);
        assert!(ranger.rooms.contains(&20894));
        assert!(ranger.rooms.contains(&19415), "the uid-less clearing is in");
        assert!(!ranger.rooms.contains(&20886), "the NPC room is outside");
        assert_eq!(ranger.ask, "ask beyor about path");
    }

    #[test]
    fn tracker_response_parses_to_literal_commands() {
        let line = r#"Your route is:  go clearing, west, west, south, go path, southeast.  If you become lost on the way, or make a wrong turn, you can always search around to get your bearings, then start again.""#;
        assert_eq!(
            parse_pathcode_line(line).unwrap(),
            vec!["go clearing", "west", "west", "south", "go path", "southeast"]
        );
        assert!(parse_pathcode_line("He points off towards the north.").is_none());
        assert!(parse_pathcode_line("Your route is:  .").is_none());
    }
}
