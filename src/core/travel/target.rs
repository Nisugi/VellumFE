//! `.go2` target resolution — go2.lic's front half, minus the guild/locker
//! specials (deferred: they need profession/CHE detection).
//!
//! Accepted forms, tried in order:
//! - `back` — where the last trip started
//! - `1234` — mapdb room id
//! - `u7150105` — game uid
//! - a saved target name (`.go2 save <name>`)
//! - a mapdb tag (`bank`, `furrier`) — nearest by travel time
//! - free text — title/description substring search; one hit travels,
//!   several become a pick list

use std::collections::BTreeMap;

use crate::core::mapdb::MapDb;
use crate::core::pathing;

#[derive(Debug, PartialEq)]
pub enum Resolved {
    Room(u32),
    /// Several candidate rooms: (id, first title), best-first.
    Ambiguous(Vec<(u32, String)>),
    NotFound(String),
}

const MAX_MATCHES: usize = 10;

pub fn resolve(
    db: &MapDb,
    current: Option<u32>,
    saved: &BTreeMap<String, u32>,
    last_start: Option<u32>,
    character: Option<&crate::core::character_state::CharacterState>,
    input: &str,
) -> Resolved {
    let input = input.trim();
    if input.is_empty() {
        return Resolved::NotFound("usage: .go2 <room id | uid | tag | name | text>".into());
    }

    // `back` (ours) and `goback` (Lich's spelling) both return to the room the
    // last trip started from.
    if input.eq_ignore_ascii_case("back") || input.eq_ignore_ascii_case("goback") {
        return match last_start {
            Some(id) => Resolved::Room(id),
            None => Resolved::NotFound("no trip to go back from yet".into()),
        };
    }

    if let Ok(id) = input.parse::<u32>() {
        return match db.room(id) {
            Some(_) => Resolved::Room(id),
            None => Resolved::NotFound(format!("room {id} is not in the mapdb")),
        };
    }

    if let Some(uid) = input
        .strip_prefix('u')
        .and_then(|rest| rest.parse::<i64>().ok())
    {
        return match db.room_id_of_uid(uid) {
            Some(id) => Resolved::Room(id),
            None => Resolved::NotFound(format!("no mapdb room carries uid {uid}")),
        };
    }

    if let Some((_, &id)) = saved
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(input))
    {
        return Resolved::Room(id);
    }

    // `guild` / `guild shop` → the character's profession guild (Lich composes
    // "<prof> guild"). Needs profession from the character feed.
    let lower = input.to_lowercase();
    if lower == "guild" || lower == "guild shop" {
        let Some(from) = current else {
            return Resolved::NotFound(
                "current room unknown - can't find the guild (see .room)".into(),
            );
        };
        let suffix = if lower == "guild shop" { "guild shop" } else { "guild" };
        return match character.and_then(|c| c.guild_tag(suffix)) {
            Some(tag) => resolve_nearest_tag(db, from, &tag),
            None => Resolved::NotFound(
                "profession unknown - run INFO so .go2 guild knows your guild".into(),
            ),
        };
    }

    // `locker` / `public locker` → nearest of the character's CHE locker tags
    // (Lich builds these from house membership). Non-members get the public
    // locker.
    if lower == "locker" || lower == "public locker" {
        let Some(from) = current else {
            return Resolved::NotFound(
                "current room unknown - can't find a locker (see .room)".into(),
            );
        };
        let tags = character
            .map(|c| c.locker_tags())
            .unwrap_or_else(|| vec!["public locker".to_string()]);
        // Nearest room across all locker tags.
        let ids: Vec<u32> = tags
            .iter()
            .flat_map(|t| db.room_ids_with_tag(t).iter().copied())
            .collect();
        if ids.is_empty() {
            return Resolved::NotFound("no reachable locker from here".into());
        }
        return match pathing::find_nearest(db, from, &ids) {
            Some(id) => Resolved::Room(id),
            None => Resolved::NotFound("no reachable locker from here".into()),
        };
    }

    // Tags are exact, lowercase by convention ("bank", "gemshop").
    let tag = lower;
    if !db.room_ids_with_tag(&tag).is_empty() {
        return match current {
            Some(from) => match pathing::find_nearest_by_tag(db, from, &tag) {
                Some(id) => Resolved::Room(id),
                None => Resolved::NotFound(format!("no reachable '{tag}' from here")),
            },
            None => Resolved::NotFound(
                "current room unknown - can't pick the nearest tagged room (see .room)".into(),
            ),
        };
    }

    // Free text over titles, then descriptions.
    let needle = input.to_lowercase();
    let mut matches: Vec<(u32, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut scan = |title_only: bool, matches: &mut Vec<(u32, String)>| {
        for location in db.locations().map(str::to_owned).collect::<Vec<_>>() {
            let Some(rooms) = db.rooms(&location) else {
                continue;
            };
            for room in rooms {
                if matches.len() >= MAX_MATCHES {
                    return;
                }
                if seen.contains(&room.id) {
                    continue;
                }
                let hit = if title_only {
                    room.title.iter().any(|t| t.to_lowercase().contains(&needle))
                } else {
                    room.description
                        .iter()
                        .any(|d| d.to_lowercase().contains(&needle))
                };
                if hit {
                    seen.insert(room.id);
                    matches.push((
                        room.id,
                        room.title.first().cloned().unwrap_or_default(),
                    ));
                }
            }
        }
    };
    scan(true, &mut matches);
    if matches.len() < MAX_MATCHES {
        scan(false, &mut matches);
    }

    match matches.len() {
        0 => Resolved::NotFound(format!("nothing in the mapdb matches '{input}'")),
        1 => Resolved::Room(matches[0].0),
        _ => Resolved::Ambiguous(matches),
    }
}

/// Nearest room carrying `tag` from `from`, as a Resolved.
fn resolve_nearest_tag(db: &MapDb, from: u32, tag: &str) -> Resolved {
    if db.room_ids_with_tag(tag).is_empty() {
        return Resolved::NotFound(format!("no '{tag}' in the mapdb"));
    }
    match pathing::find_nearest_by_tag(db, from, tag) {
        Some(id) => Resolved::Room(id),
        None => Resolved::NotFound(format!("no reachable '{tag}' from here")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> MapDb {
        MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "Town", "title": ["[Town Square]"],
                 "wayto": {"2": "east"}, "timeto": {"2": 0.2}, "paths": ""},
                {"id": 2, "uid": [9000002], "location": "Town", "title": ["[Bank, Teller]"],
                 "tags": ["bank"], "description": ["A marble counter."],
                 "wayto": {"1": "west"}, "timeto": {"1": 0.2}, "paths": ""},
                {"id": 3, "uid": [9000003], "location": "Town", "title": ["[Far Bank, Teller]"],
                 "tags": ["bank"], "wayto": {"1": "swim"}, "timeto": {"1": 9.0}, "paths": ""}
            ]"#,
        )
        .unwrap()
    }

    #[test]
    fn resolves_each_form_in_priority_order() {
        let db = db();
        let saved: BTreeMap<String, u32> = [("home".to_string(), 3u32)].into();
        // Test wrapper: no character state (guild/locker not exercised here).
        let r = |from, last, input| resolve(&db, from, &saved, last, None, input);

        assert_eq!(r(Some(1), Some(7), "back"), Resolved::Room(7));
        assert_eq!(r(Some(1), Some(7), "goback"), Resolved::Room(7));
        assert_eq!(r(Some(1), None, "2"), Resolved::Room(2));
        assert_eq!(r(Some(1), None, "u9000002"), Resolved::Room(2));
        assert_eq!(r(Some(1), None, "HOME"), Resolved::Room(3));
        // Tag: nearest wins (room 2 at 0.2 beats room 3 at 9.0).
        assert_eq!(r(Some(1), None, "bank"), Resolved::Room(2));
        // Free text: unique title match travels, multiple offer a list.
        assert_eq!(r(Some(1), None, "town square"), Resolved::Room(1));
        match r(Some(1), None, "teller") {
            Resolved::Ambiguous(list) => {
                assert_eq!(list.len(), 2);
            }
            other => panic!("expected pick list, got {other:?}"),
        }
        // Description text is searched after titles.
        assert_eq!(r(Some(1), None, "marble counter"), Resolved::Room(2));
        assert!(matches!(r(Some(1), None, "zzznope"), Resolved::NotFound(_)));
        assert!(matches!(r(Some(1), None, "99"), Resolved::NotFound(_)));
    }

    #[test]
    fn guild_and_locker_resolve_from_character_state() {
        use crate::core::character_state::CharacterState;
        // A mapdb with a ranger guild and a CHE locker for house "paupers".
        let db = MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Square]"],
                 "wayto": {"2": "east", "3": "west"}, "timeto": {"2": 0.2, "3": 0.2},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[Ranger Guild]"],
                 "tags": ["ranger guild"], "wayto": {"1": "west"}, "timeto": {"1": 0.2},
                 "paths": ""},
                {"id": 3, "uid": [9000003], "location": "T", "title": ["[Paupers Locker]"],
                 "tags": ["meta:che:paupers:locker"], "wayto": {"1": "east"},
                 "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let saved = BTreeMap::new();

        // Ranger character in House of Paupers.
        let mut ch = CharacterState::default();
        ch.parse_line("Name: Nisugi Race: Half-Elf  Profession: Ranger (shown as: Hero)");
        ch.parse_line("PERSONAL INFORMATION");
        ch.parse_line("Member of House of Paupers");

        // .go2 guild → the ranger guild.
        assert_eq!(
            resolve(&db, Some(1), &saved, None, Some(&ch), "guild"),
            Resolved::Room(2)
        );
        // .go2 locker → the paupers CHE locker.
        assert_eq!(
            resolve(&db, Some(1), &saved, None, Some(&ch), "locker"),
            Resolved::Room(3)
        );
        // Without profession known, guild reports a helpful error.
        let empty = CharacterState::default();
        assert!(matches!(
            resolve(&db, Some(1), &saved, None, Some(&empty), "guild"),
            Resolved::NotFound(_)
        ));
        // A non-member falls back to the public locker (none here → not found).
        assert!(matches!(
            resolve(&db, Some(1), &saved, None, Some(&empty), "locker"),
            Resolved::NotFound(_)
        ));
    }
}
