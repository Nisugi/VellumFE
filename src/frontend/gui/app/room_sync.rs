//! Mirrors parsed room components (description, objects, players, exits)
//! into the dedicated room text windows each frame.

use super::*;

impl VellumGuiApp {
    pub(super) fn room_component_lines(component: Option<&Vec<Vec<TextSegment>>>) -> Vec<StyledLine> {
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

    pub(super) fn sync_room_windows_from_components(&mut self) {
        if !self.app_core.room_window_dirty {
            return;
        }

        let room_name = self
            .app_core
            .game_state
            .room_name
            .as_ref()
            .filter(|name| !name.trim().is_empty())
            .cloned()
            .or_else(|| self.app_core.room_subtitle.clone())
            .unwrap_or_default();
        let description =
            Self::room_component_lines(self.app_core.room_components.get("room desc"));
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
