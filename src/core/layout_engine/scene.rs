//! Presentation model (spec §8): the drawable form of a generated layout.
//!
//! Pure data — no UI types — so both the mini map and the explorer render
//! from the same scene, and it can be built on the layout worker thread.
//! Rooms carry final sheet cells; edges are pre-classified: solid directional
//! edges, stubs for directional edges stretched past `LONG_EDGE_CELLS`, and
//! dashed labeled connectors (skipped past `CONNECTOR_MAX_CELLS`).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::direction::DirectionMap;
use super::mapdb::RoomTable;
use super::overrides::group_anchor_key;
use super::positioner::Cell;
use super::Layout;

/// Directional edges longer than this render as stubs, not lines.
pub const LONG_EDGE_CELLS: i32 = 8;
/// Connectors longer than this are not drawn at all.
pub const CONNECTOR_MAX_CELLS: i32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sheet {
    Outdoor,
    Interiors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneRoom {
    pub id: u32,
    pub uid: Option<i64>,
    pub cell: Cell,
    pub group: usize,
    /// Outdoor room hosting a doorway into an interior (gets a door marker).
    pub entrance: bool,
    /// First room title, for hover text.
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneEdgeKind {
    /// Solid line: a compass-true edge within a group.
    Directional,
    /// Stretched directional edge: draw short dashed arrows at both ends,
    /// each labeled with the partner's room id, instead of a long line.
    Stub,
    /// Dashed inter-group connector ("go door" adjacency, no direction).
    Connector,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneEdge {
    pub a: Cell,
    pub b: Cell,
    pub a_room: u32,
    pub b_room: u32,
    pub kind: SceneEdgeKind,
    /// Movement label for connectors ("dock", "gate") when one is worth
    /// showing.
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupLabel {
    pub text: String,
    /// Top-left cell of the group's bounds; render above it.
    pub cell: Cell,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SheetScene {
    pub rooms: Vec<SceneRoom>,
    pub edges: Vec<SceneEdge>,
    pub labels: Vec<GroupLabel>,
    pub min: Cell,
    pub max: Cell,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MapScene {
    pub location: String,
    pub outdoor: SheetScene,
    pub interiors: SheetScene,
    /// room id → (sheet, index into that sheet's `rooms`).
    pub room_index: HashMap<u32, (Sheet, usize)>,
    /// Group index → its sheet frame offset (base_offset), for translating
    /// final cells back into group-relative coordinates when editing.
    pub group_offsets: HashMap<usize, Cell>,
    /// Group index → stable anchor key (lowest uid, fallback lowest id) —
    /// the key group overrides are stored under.
    pub group_anchors: HashMap<usize, i64>,
}

impl MapScene {
    pub fn sheet(&self, sheet: Sheet) -> &SheetScene {
        match sheet {
            Sheet::Outdoor => &self.outdoor,
            Sheet::Interiors => &self.interiors,
        }
    }

    pub fn room(&self, id: u32) -> Option<(Sheet, &SceneRoom)> {
        let &(sheet, idx) = self.room_index.get(&id)?;
        Some((sheet, &self.sheet(sheet).rooms[idx]))
    }
}

/// A connector label worth drawing: the movement command when it's short,
/// not a cardinal, and not a stringproc (a simplified port of the
/// reference's `getConnectionLabel`).
fn connector_label(cmd: &str) -> Option<String> {
    let trimmed = cmd.trim();
    if trimmed.starts_with(";e") || trimmed.is_empty() {
        return None;
    }
    let lowered = trimmed.to_lowercase();
    if super::direction::Dir::from_exact(&lowered).is_some() || lowered == "out" {
        return None;
    }
    let rest = lowered
        .strip_prefix("go ")
        .or_else(|| lowered.strip_prefix("climb "))
        .or_else(|| lowered.strip_prefix("move "))
        .unwrap_or(&lowered);
    if rest.len() <= 20 {
        Some(rest.to_owned())
    } else {
        None
    }
}

pub fn build_scene(location: &str, layout: &Layout, lookup: &RoomTable) -> MapScene {
    let dirs = DirectionMap::build(lookup);
    let mut scene = MapScene {
        location: location.to_owned(),
        ..Default::default()
    };

    // Which sheet each group is on.
    let interiors: HashSet<usize> = layout.interiors.iter().copied().collect();
    let sheet_of = |group: usize| {
        if interiors.contains(&group) {
            Sheet::Interiors
        } else {
            Sheet::Outdoor
        }
    };

    // Rooms.
    for group in &layout.groups {
        let sheet = sheet_of(group.index);
        scene
            .group_offsets
            .insert(group.index, group.base_offset.unwrap_or_default());
        scene
            .group_anchors
            .insert(group.index, group_anchor_key(group, lookup));
        for &id in &group.room_ids {
            let room = lookup.get(id);
            let scene_room = SceneRoom {
                id,
                uid: room.and_then(|r| r.uid.first().copied()),
                cell: group.final_cell(id),
                group: group.index,
                entrance: layout.classification.entrance_room_ids.contains(&id),
                title: room
                    .and_then(|r| r.title.first())
                    .cloned()
                    .unwrap_or_default(),
            };
            let target = match sheet {
                Sheet::Outdoor => &mut scene.outdoor,
                Sheet::Interiors => &mut scene.interiors,
            };
            scene
                .room_index
                .insert(id, (sheet, target.rooms.len()));
            target.rooms.push(scene_room);
        }
    }

    // Edges: directional within a group; connectors across groups on the
    // same sheet. Deduped by unordered pair, direction checked either way.
    let group_of: HashMap<u32, usize> = layout
        .groups
        .iter()
        .flat_map(|g| g.room_ids.iter().map(move |&id| (id, g.index)))
        .collect();
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    for room in lookup.rooms() {
        let Some(&room_group) = group_of.get(&room.id) else {
            continue;
        };
        for (&target_id, cmd) in &room.wayto {
            let Some(&target_group) = group_of.get(&target_id) else {
                continue;
            };
            let key = (room.id.min(target_id), room.id.max(target_id));
            let a_sheet = sheet_of(room_group);
            if room_group == target_group {
                // Directional edge (or nothing drawable).
                if dirs.get(room.id, target_id).is_none() {
                    continue;
                }
                if !seen.insert(key) {
                    continue;
                }
                let group = &layout.groups[room_group];
                let a = group.final_cell(room.id);
                let b = group.final_cell(target_id);
                let len = (a.x - b.x).abs().max((a.y - b.y).abs());
                push_edge(
                    &mut scene,
                    a_sheet,
                    SceneEdge {
                        a,
                        b,
                        a_room: room.id,
                        b_room: target_id,
                        kind: if len > LONG_EDGE_CELLS {
                            SceneEdgeKind::Stub
                        } else {
                            SceneEdgeKind::Directional
                        },
                        label: None,
                    },
                );
            } else {
                // Connector — only drawn when both ends share a sheet.
                if a_sheet != sheet_of(target_group) {
                    continue;
                }
                if !seen.insert(key) {
                    continue;
                }
                let a = layout.groups[room_group].final_cell(room.id);
                let b = layout.groups[target_group].final_cell(target_id);
                let len = (a.x - b.x).abs().max((a.y - b.y).abs());
                if len > CONNECTOR_MAX_CELLS {
                    continue;
                }
                push_edge(
                    &mut scene,
                    a_sheet,
                    SceneEdge {
                        a,
                        b,
                        a_room: room.id,
                        b_room: target_id,
                        kind: SceneEdgeKind::Connector,
                        label: connector_label(cmd),
                    },
                );
            }
        }
    }

    // Interior group labels (building names).
    for &idx in &layout.interiors {
        let group = &layout.groups[idx];
        let Some(name) = group.name.clone() else {
            continue;
        };
        let bounds = group.bounds();
        let off = group.base_offset.unwrap_or_default();
        scene.interiors.labels.push(GroupLabel {
            text: name,
            cell: Cell {
                x: bounds.min_x + off.x,
                y: bounds.min_y + off.y,
            },
        });
    }

    for sheet in [&mut scene.outdoor, &mut scene.interiors] {
        let mut min = Cell {
            x: i32::MAX,
            y: i32::MAX,
        };
        let mut max = Cell {
            x: i32::MIN,
            y: i32::MIN,
        };
        for room in &sheet.rooms {
            min.x = min.x.min(room.cell.x);
            min.y = min.y.min(room.cell.y);
            max.x = max.x.max(room.cell.x);
            max.y = max.y.max(room.cell.y);
        }
        if sheet.rooms.is_empty() {
            min = Cell::default();
            max = Cell::default();
        }
        sheet.min = min;
        sheet.max = max;
    }

    scene
}

fn push_edge(scene: &mut MapScene, sheet: Sheet, edge: SceneEdge) {
    match sheet {
        Sheet::Outdoor => scene.outdoor.edges.push(edge),
        Sheet::Interiors => scene.interiors.edges.push(edge),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_labels() {
        assert_eq!(connector_label("go dock"), Some("dock".into()));
        assert_eq!(connector_label("climb rope ladder"), Some("rope ladder".into()));
        assert_eq!(connector_label("north"), None);
        assert_eq!(connector_label("out"), None);
        assert_eq!(connector_label(";e fput 'go gate'"), None);
        assert_eq!(
            connector_label("go some extremely long movement command"),
            None
        );
    }
}
