//! Mirrors parsed room components (description, objects, players, exits)
//! into the dedicated room text windows each frame.

use super::*;

impl VellumGuiApp {
    pub(super) fn room_component_lines(
        component: Option<&Vec<Vec<TextSegment>>>,
    ) -> Vec<StyledLine> {
        component
            .map(|lines| {
                lines
                    .iter()
                    .map(|segments| StyledLine {
                        segments: segments.clone(),
                        stream: "room".to_string(),
                        timestamp: None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// One flowing line like "Also here: Fraemen, Zugu" with each entry a
    /// clickable link. Fallback for when the styled room component is
    /// missing (e.g. state fed by Lich rather than the game stream).
    pub(super) fn room_line_from_links(
        label: &str,
        entries: impl Iterator<Item = (String, Option<crate::data::LinkData>)>,
    ) -> Vec<StyledLine> {
        let mut segments = vec![TextSegment {
            text: label.to_string(),
            ..Default::default()
        }];
        let mut first = true;
        for (text, link_data) in entries {
            if !first {
                segments.push(TextSegment {
                    text: ", ".to_string(),
                    ..Default::default()
                });
            }
            first = false;
            let span_type = if link_data.is_some() {
                crate::data::SpanType::Link
            } else {
                crate::data::SpanType::Normal
            };
            segments.push(TextSegment {
                text,
                span_type,
                link_data,
                ..Default::default()
            });
        }
        if first {
            return Vec::new();
        }
        vec![StyledLine {
            segments,
            stream: "room".to_string(),
            timestamp: None,
        }]
    }

    /// The room window's title line, matching the story window's shape:
    /// `[Kraken's Fall, Third Pier - 29043] (u7118245)`.
    ///
    /// The bracketed part is the room name plus the Lich room id; the
    /// parenthesised part is the game's own uid, prefixed `u` because that
    /// is how it is typed in commands (`go2 u7118245`). Either id may be
    /// missing — the game sends the uid via `<nav rm=>` and the Lich id only
    /// under Lich — so each is simply omitted when absent, and a room with
    /// neither renders as `[Name]`.
    ///
    /// The incoming name may already be bracketed (the `roomName` style
    /// carries them) or already carry ` - <lich id>`; both are stripped first
    /// so the format is applied exactly once and re-syncing cannot nest.
    pub(super) fn format_room_title(
        name: String,
        lich_id: Option<&str>,
        uid: Option<&str>,
    ) -> String {
        // Strip a previously-applied " (u<digits>)" suffix before anything
        // else, or the trailing ']' is not at the end and the whole format
        // nests on the next sync.
        let mut trimmed = name.trim();
        if trimmed.ends_with(')') {
            if let Some(open) = trimmed.rfind(" (u") {
                if trimmed[open + 3..trimmed.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii_digit())
                {
                    trimmed = trimmed[..open].trim_end();
                }
            }
        }
        let mut base = trimmed
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim()
            .to_string();

        // Drop a trailing " - <digits>" the name already carries so the id
        // isn't printed twice.
        if let Some(dash) = base.rfind(" - ") {
            if base[dash + 3..].trim().chars().all(|c| c.is_ascii_digit())
                && !base[dash + 3..].trim().is_empty()
            {
                base.truncate(dash);
            }
        }
        if base.is_empty() {
            return String::new();
        }

        let mut out = match lich_id.map(str::trim).filter(|id| !id.is_empty()) {
            Some(id) => format!("[{base} - {id}]"),
            None => format!("[{base}]"),
        };
        if let Some(uid) = uid.map(str::trim).filter(|id| !id.is_empty()) {
            let uid = uid.trim_start_matches('u');
            out.push_str(&format!(" (u{uid})"));
        }
        out
    }

    pub(super) fn sync_room_windows_from_components(&mut self) {
        if !self.app_core.room_window_dirty {
            return;
        }

        let room_name = Self::format_room_title(
            self.app_core
                .game_state
                .room_name
                .as_ref()
                .filter(|name| !name.trim().is_empty())
                .cloned()
                .or_else(|| self.app_core.room_subtitle.clone())
                .unwrap_or_default(),
            self.app_core.lich_room_id.as_deref(),
            self.app_core.nav_room_id.as_deref(),
        );
        let mut description =
            Self::room_component_lines(self.app_core.room_components.get("room desc"));
        // Room art (the game's own `sprite` slot) leads the description, so an
        // image there floats and the prose wraps beside it. Merging into the
        // first description line rather than standing alone is what makes it a
        // float instead of a banner with text below.
        // Room art comes from the game's `sprite` component ONLY. The
        // `<resource picture='N'/>` feed arrives in the STORY stream and stays
        // there — mixing the two would put a story-stream picture in the room
        // window, which is a different feature wearing the same clothes.
        let sprite = Self::room_component_lines(
            self.app_core
                .room_components
                .get(crate::core::messages::SPRITE_COMPONENT),
        );
        if !sprite.is_empty() {
            let mut lead: Vec<TextSegment> =
                sprite.into_iter().flat_map(|line| line.segments).collect();
            match description.first_mut() {
                Some(first) => {
                    lead.append(&mut first.segments);
                    first.segments = lead;
                }
                None => description.push(StyledLine {
                    segments: lead,
                    stream: "room".to_string(),
                    timestamp: None,
                }),
            }
        }
        // Prefer the styled component text verbatim (natural "Obvious
        // paths:" / "Also here:" phrasing, monsterbold creatures, links);
        // synthesize an equivalent line from game state only when absent.
        let mut exits = Self::room_component_lines(self.app_core.room_components.get("room exits"));
        if exits.is_empty() {
            exits = Self::room_line_from_links(
                "Obvious exits: ",
                self.app_core.game_state.exits.iter().map(|dir| {
                    (
                        dir.clone(),
                        Some(crate::data::LinkData {
                            exist_id: "_direct_".to_string(),
                            noun: dir.clone(),
                            text: dir.clone(),
                            coord: None,
                        }),
                    )
                }),
            );
        }
        let mut players =
            Self::room_component_lines(self.app_core.room_components.get("room players"));
        if players.is_empty() {
            players = Self::room_line_from_links(
                "Also here: ",
                self.app_core.game_state.room_players.iter().map(|player| {
                    (
                        player.name.clone(),
                        Some(crate::data::LinkData {
                            exist_id: player.id.clone(),
                            noun: player.name.clone(),
                            text: player.name.clone(),
                            coord: None,
                        }),
                    )
                }),
            );
        }
        let mut objects =
            Self::room_component_lines(self.app_core.room_components.get("room objs"));
        if objects.is_empty() {
            objects = Self::room_line_from_links(
                "You also see ",
                self.app_core.game_state.room_objects.iter().map(|object| {
                    (
                        object.name.clone(),
                        Some(crate::data::LinkData {
                            exist_id: object.id.clone(),
                            noun: object.noun.clone().unwrap_or_else(|| object.name.clone()),
                            text: object.name.clone(),
                            coord: None,
                        }),
                    )
                }),
            );
        }

        for window in self.app_core.ui_state.windows.values_mut() {
            let WindowContent::Room(room) = &mut window.content else {
                continue;
            };
            room.name = room_name.clone();
            room.description = description.clone();
            room.exits = exits.clone();
            room.players = players.clone();
            room.objects = objects.clone();
        }

        self.app_core.room_window_dirty = false;
    }
}
