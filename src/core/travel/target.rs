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
    input: &str,
) -> Resolved {
    let input = input.trim();
    if input.is_empty() {
        return Resolved::NotFound("usage: .go2 <room id | uid | tag | name | text>".into());
    }

    if input.eq_ignore_ascii_case("back") {
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

    // Tags are exact, lowercase by convention ("bank", "gemshop").
    let tag = input.to_lowercase();
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

        assert_eq!(resolve(&db, Some(1), &saved, Some(7), "back"), Resolved::Room(7));
        assert_eq!(resolve(&db, Some(1), &saved, None, "2"), Resolved::Room(2));
        assert_eq!(resolve(&db, Some(1), &saved, None, "u9000002"), Resolved::Room(2));
        assert_eq!(resolve(&db, Some(1), &saved, None, "HOME"), Resolved::Room(3));
        // Tag: nearest wins (room 2 at 0.2 beats room 3 at 9.0).
        assert_eq!(resolve(&db, Some(1), &saved, None, "bank"), Resolved::Room(2));
        // Free text: unique title match travels, multiple offer a list.
        assert_eq!(
            resolve(&db, Some(1), &saved, None, "town square"),
            Resolved::Room(1)
        );
        match resolve(&db, Some(1), &saved, None, "teller") {
            Resolved::Ambiguous(list) => {
                assert_eq!(list.len(), 2);
            }
            other => panic!("expected pick list, got {other:?}"),
        }
        // Description text is searched after titles.
        assert_eq!(
            resolve(&db, Some(1), &saved, None, "marble counter"),
            Resolved::Room(2)
        );
        assert!(matches!(
            resolve(&db, Some(1), &saved, None, "zzznope"),
            Resolved::NotFound(_)
        ));
        assert!(matches!(
            resolve(&db, Some(1), &saved, None, "99"),
            Resolved::NotFound(_)
        ));
    }
}
