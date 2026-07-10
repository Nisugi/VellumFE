//! Map Explorer: a separate native OS window (egui multi-viewport, same
//! mechanism as detached tabs) for browsing any location's generated map —
//! location picker, outdoor/interiors sheets, drag-pan, scroll-zoom, room
//! inspection, and walk-to. The override editor builds on this surface.

use eframe::egui::{self, Pos2, Rect, Sense, Vec2, ViewportBuilder, ViewportId};

use crate::core::layout_engine::scene::Sheet;
use crate::core::layout_engine::{Cell, EdgeAction, SheetChoice};
use crate::core::map_service::{DbState, OverrideEdit};
use crate::frontend::gui::map_view::{self, MapCamera, MapStyle};

use super::VellumGuiApp;

pub(super) struct MapExplorerState {
    pub open: bool,
    /// Browsed location; when `follow` is on it tracks the character.
    location: Option<String>,
    sheet: Sheet,
    follow: bool,
    /// Camera center in cell coordinates.
    center: Pos2,
    px_per_cell: f32,
    selected: Option<u32>,
    filter: String,
    /// Map-service revision last synced under follow mode.
    last_revision: u64,
    /// The camera was pointed at something meaningful for this location.
    centered: bool,
    /// Override editing: drags move groups (Alt: single room) and write the
    /// uid-keyed override diff.
    edit_mode: bool,
    drag: Option<DragState>,
    rename_buffer: String,
}

/// An in-flight edit drag; committed as one override edit on release.
struct DragState {
    group: usize,
    /// Set when Alt was held at drag start: move just this room.
    room: Option<u32>,
    /// Accumulated pointer travel in pixels.
    accum: Vec2,
}

impl Default for MapExplorerState {
    fn default() -> Self {
        MapExplorerState {
            open: false,
            location: None,
            sheet: Sheet::Outdoor,
            follow: true,
            center: Pos2::ZERO,
            px_per_cell: 24.0,
            selected: None,
            filter: String::new(),
            last_revision: 0,
            centered: false,
            edit_mode: false,
            drag: None,
            rename_buffer: String::new(),
        }
    }
}

#[derive(Default)]
struct ExplorerOutput {
    close: bool,
    walk_to: Option<u32>,
    request_location: Option<String>,
    override_edit: Option<OverrideEdit>,
}

impl VellumGuiApp {
    pub(super) fn render_map_explorer(&mut self, ctx: &egui::Context) {
        if !self.map_explorer.open {
            return;
        }

        // Follow mode: track the character's location/room whenever the map
        // service state moves.
        {
            let map = &self.app_core.map;
            let ex = &mut self.map_explorer;
            if ex.follow && ex.last_revision != map.revision {
                ex.last_revision = map.revision;
                if let Some(loc) = &map.current_location {
                    if ex.location.as_deref() != Some(loc) {
                        ex.location = Some(loc.clone());
                        ex.selected = None;
                        ex.centered = false;
                    }
                }
                if let Some(id) = map.current_room_id {
                    if let Some((sheet, room)) = map.current_scene().and_then(|s| s.room(id)) {
                        ex.sheet = sheet;
                        ex.center = Pos2::new(room.cell.x as f32, room.cell.y as f32);
                        ex.centered = true;
                    }
                }
            }
            if ex.location.is_none() {
                ex.location = map.current_location.clone();
            }
        }
        // Keep the browsed location's layout generation in flight.
        if let Some(loc) = self.map_explorer.location.clone() {
            self.app_core.map.request_location(&loc);
        }

        let app_core = &self.app_core;
        let ex = &mut self.map_explorer;
        let builder = ViewportBuilder::default()
            .with_title("VellumFE - Map Explorer")
            .with_inner_size(Vec2::new(1000.0, 720.0))
            .with_min_inner_size(Vec2::new(480.0, 360.0));
        let out = ctx.show_viewport_immediate(
            ViewportId::from_hash_of("vellum_map_explorer"),
            builder,
            |ui, _class| {
                let mut out = ExplorerOutput::default();
                if ui.input(|i| i.viewport().close_requested()) {
                    out.close = true;
                }
                Self::explorer_toolbar(ui, app_core, ex, &mut out);
                Self::explorer_side_panel(ui, app_core, ex, &mut out);
                Self::explorer_canvas(ui, app_core, ex, &mut out);
                out
            },
        );

        if out.close {
            self.map_explorer.open = false;
        }
        if let Some(loc) = out.request_location {
            self.app_core.map.request_location(&loc);
        }
        if let Some(id) = out.walk_to {
            self.dispatch_raw_command(format!(";go2 {id}"));
        }
        if let Some(edit) = out.override_edit {
            self.app_core.map.apply_override_edit(edit);
        }
    }

    fn explorer_toolbar(
        ui: &mut egui::Ui,
        app_core: &crate::core::AppCore,
        ex: &mut MapExplorerState,
        out: &mut ExplorerOutput,
    ) {
        let map = &app_core.map;
        egui::Panel::top("map_explorer_toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Location picker with a filter box inside the popup.
                let selected_text = ex.location.as_deref().unwrap_or("(no location)");
                egui::ComboBox::from_id_salt("map_explorer_location")
                    .selected_text(selected_text)
                    .width(240.0)
                    // Keep the popup open while typing in the filter box;
                    // selection closes it explicitly below.
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show_ui(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut ex.filter)
                                .hint_text("filter locations"),
                        );
                        ui.separator();
                        let filter = ex.filter.to_lowercase();
                        if let Some(db) = map.mapdb() {
                            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                for location in db.locations() {
                                    if !filter.is_empty()
                                        && !location.to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                    let is_current = ex.location.as_deref() == Some(location);
                                    if ui.selectable_label(is_current, location).clicked() {
                                        if !is_current {
                                            ex.location = Some(location.to_owned());
                                            ex.follow = false;
                                            ex.selected = None;
                                            ex.centered = false;
                                            ex.sheet = Sheet::Outdoor;
                                            out.request_location = Some(location.to_owned());
                                        }
                                        ui.close();
                                    }
                                }
                            });
                        }
                    });

                ui.separator();
                let scene = ex
                    .location
                    .as_deref()
                    .and_then(|loc| map.scene_for(loc));
                let has_interiors = scene
                    .map(|s| !s.interiors.rooms.is_empty())
                    .unwrap_or(false);
                if ui
                    .selectable_label(ex.sheet == Sheet::Outdoor, "Outdoor")
                    .clicked()
                {
                    ex.sheet = Sheet::Outdoor;
                    ex.centered = false;
                }
                ui.add_enabled_ui(has_interiors, |ui| {
                    if ui
                        .selectable_label(ex.sheet == Sheet::Interiors, "Interiors")
                        .clicked()
                    {
                        ex.sheet = Sheet::Interiors;
                        ex.centered = false;
                    }
                });

                ui.separator();
                if ui
                    .toggle_value(&mut ex.follow, "Follow")
                    .on_hover_text("Track the character's room")
                    .changed()
                    && ex.follow
                {
                    ex.last_revision = 0; // force a resync next frame
                }
                if ui.button("Center").on_hover_text("Center on the character (or the map)").clicked() {
                    ex.centered = false;
                    if let Some(id) = map.current_room_id {
                        if let Some((sheet, room)) =
                            map.current_scene().and_then(|s| s.room(id))
                        {
                            if map.current_location == ex.location {
                                ex.sheet = sheet;
                                ex.center =
                                    Pos2::new(room.cell.x as f32, room.cell.y as f32);
                                ex.centered = true;
                            }
                        }
                    }
                }

                ui.separator();
                if ui.button("\u{2212}").clicked() {
                    ex.px_per_cell = (ex.px_per_cell / 1.25).clamp(4.0, 72.0);
                }
                if ui.button("+").clicked() {
                    ex.px_per_cell = (ex.px_per_cell * 1.25).clamp(4.0, 72.0);
                }
                ui.label(format!("{:.0} px/cell", ex.px_per_cell));

                ui.separator();
                ui.toggle_value(&mut ex.edit_mode, "Edit")
                    .on_hover_text("Drag a group to move it (Alt: single room); edits save as overrides");
                if ex.edit_mode {
                    let count = ex
                        .location
                        .as_deref()
                        .and_then(|loc| map.overrides_for(loc))
                        .map(|ov| {
                            ov.group_offsets.len() + ov.room_pins.len() + ov.names.len()
                        })
                        .unwrap_or(0);
                    if count > 0 {
                        if ui
                            .button(format!("Reset overrides ({count})"))
                            .on_hover_text("Drop every override for this location")
                            .clicked()
                        {
                            if let Some(loc) = ex.location.clone() {
                                out.override_edit =
                                    Some(OverrideEdit::ResetLocation { location: loc });
                            }
                        }
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match map.db_state() {
                        DbState::NotLoaded => {
                            ui.label(
                                egui::RichText::new(
                                    "Set your Lich folder in Settings \u{25b8} Map",
                                )
                                .weak(),
                            );
                        }
                        DbState::Loading => {
                            ui.spinner();
                            ui.label("loading mapdb\u{2026}");
                        }
                        DbState::Failed => {
                            ui.label(
                                egui::RichText::new("mapdb load failed")
                                    .color(ui.visuals().error_fg_color),
                            )
                            .on_hover_text(map.db_error.clone().unwrap_or_default());
                        }
                        DbState::Loaded => {
                            if let Some(loc) = ex.location.as_deref() {
                                if map.is_pending(loc) {
                                    ui.spinner();
                                    ui.label("generating\u{2026}");
                                } else if let Some(scene) = map.scene_for(loc) {
                                    ui.label(format!(
                                        "{} rooms",
                                        scene.outdoor.rooms.len()
                                            + scene.interiors.rooms.len()
                                    ));
                                }
                            }
                        }
                    }
                });
            });
        });
    }

    fn explorer_side_panel(
        ui: &mut egui::Ui,
        app_core: &crate::core::AppCore,
        ex: &mut MapExplorerState,
        out: &mut ExplorerOutput,
    ) {
        let Some(selected) = ex.selected else {
            return;
        };
        let map = &app_core.map;
        let scene = ex.location.as_deref().and_then(|loc| map.scene_for(loc));
        let Some((_, scene_room)) = scene.and_then(|s| s.room(selected)) else {
            return;
        };
        // Full room record for exits.
        let room = ex
            .location
            .as_deref()
            .and_then(|loc| map.mapdb().and_then(|db| db.rooms(loc)))
            .and_then(|rooms| {
                rooms
                    .binary_search_by_key(&selected, |r| r.id)
                    .ok()
                    .map(|i| &rooms[i])
            });

        egui::Panel::right("map_explorer_room")
            .default_size(240.0)
            .show(ui, |ui| {
                ui.heading(if scene_room.title.is_empty() {
                    "(untitled room)"
                } else {
                    &scene_room.title
                });
                ui.label(format!("Room id: {selected}"));
                if let Some(uid) = scene_room.uid {
                    ui.label(format!("uid: {uid}"));
                }
                ui.separator();
                if ui.button("Walk here  (;go2)").clicked() {
                    out.walk_to = Some(selected);
                }
                if ui.button("Center view").clicked() {
                    ex.center =
                        Pos2::new(scene_room.cell.x as f32, scene_room.cell.y as f32);
                }
                if ex.edit_mode {
                    ui.separator();
                    ui.label(egui::RichText::new("Group").strong());
                    let scene = ex.location.as_deref().and_then(|loc| map.scene_for(loc));
                    if let Some((anchor, group)) = scene.and_then(|s| {
                        s.room(selected)
                            .and_then(|(_, r)| Some((*s.group_anchors.get(&r.group)?, r.group)))
                    }) {
                        let _ = group;
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut ex.rename_buffer);
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Set name").clicked()
                                && !ex.rename_buffer.trim().is_empty()
                            {
                                out.override_edit = Some(OverrideEdit::GroupName {
                                    location: ex.location.clone().unwrap_or_default(),
                                    anchor,
                                    name: Some(ex.rename_buffer.trim().to_string()),
                                });
                            }
                            if ui.button("Clear name").clicked() {
                                out.override_edit = Some(OverrideEdit::GroupName {
                                    location: ex.location.clone().unwrap_or_default(),
                                    anchor,
                                    name: None,
                                });
                            }
                        });
                        // Classification: where does this group belong?
                        let current_choice = ex
                            .location
                            .as_deref()
                            .and_then(|loc| map.overrides_for(loc))
                            .and_then(|ov| ov.sheets.get(&anchor).copied());
                        ui.horizontal(|ui| {
                            ui.label("Sheet:");
                            for (label, choice) in [
                                ("Auto", None),
                                ("Outdoor", Some(SheetChoice::Outdoor)),
                                ("Interior", Some(SheetChoice::Interior)),
                            ] {
                                if ui
                                    .selectable_label(current_choice == choice, label)
                                    .clicked()
                                    && current_choice != choice
                                {
                                    out.override_edit = Some(OverrideEdit::Sheet {
                                        location: ex.location.clone().unwrap_or_default(),
                                        anchor,
                                        choice,
                                    });
                                }
                            }
                        });
                    }
                    let key = scene_room.uid.unwrap_or(selected as i64);
                    let pinned = ex
                        .location
                        .as_deref()
                        .and_then(|loc| map.overrides_for(loc))
                        .map(|ov| ov.room_pins.contains_key(&key))
                        .unwrap_or(false);
                    if pinned && ui.button("Unpin room").clicked() {
                        out.override_edit = Some(OverrideEdit::RoomPin {
                            location: ex.location.clone().unwrap_or_default(),
                            key,
                            pin: None,
                        });
                    }
                }
                if let Some(room) = room {
                    ui.separator();
                    ui.label(egui::RichText::new("Exits").strong());
                    let rooms_slice = ex
                        .location
                        .as_deref()
                        .and_then(|loc| map.mapdb().map(|db| (db, loc)))
                        .and_then(|(db, loc)| db.rooms(loc));
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (target, cmd) in &room.wayto {
                            if !ex.edit_mode {
                                ui.label(format!("{cmd} \u{2192} {target}"));
                                continue;
                            }
                            // Edge action editor, keyed by the room-key pair.
                            let target_key = rooms_slice
                                .and_then(|rooms| {
                                    rooms
                                        .binary_search_by_key(target, |r| r.id)
                                        .ok()
                                        .map(|i| &rooms[i])
                                })
                                .map(|r| r.uid.first().copied().unwrap_or(r.id as i64));
                            let Some(target_key) = target_key else {
                                ui.label(format!("{cmd} \u{2192} {target}"));
                                continue;
                            };
                            let my_key = scene_room.uid.unwrap_or(selected as i64);
                            let (ka, kb) = (my_key.min(target_key), my_key.max(target_key));
                            let current_action = ex
                                .location
                                .as_deref()
                                .and_then(|loc| map.overrides_for(loc))
                                .and_then(|ov| {
                                    ov.edges
                                        .iter()
                                        .find(|e| (e.a, e.b) == (ka, kb))
                                        .map(|e| e.action)
                                });
                            ui.horizontal(|ui| {
                                ui.label(format!("{cmd} \u{2192} {target}"));
                                let text = match current_action {
                                    None => "auto".to_string(),
                                    Some(EdgeAction::Hide) => "hidden".to_string(),
                                    Some(EdgeAction::Connector) => "passage".to_string(),
                                    Some(EdgeAction::Direction(d)) => d.name().to_string(),
                                };
                                egui::ComboBox::from_id_salt(("map_edge", ka, kb, *target))
                                    .selected_text(text)
                                    .width(90.0)
                                    .show_ui(ui, |ui| {
                                        let mut pick =
                                            |ui: &mut egui::Ui,
                                             label: &str,
                                             action: Option<EdgeAction>| {
                                                if ui
                                                    .selectable_label(
                                                        current_action == action,
                                                        label,
                                                    )
                                                    .clicked()
                                                    && current_action != action
                                                {
                                                    out.override_edit =
                                                        Some(OverrideEdit::Edge {
                                                            location: ex
                                                                .location
                                                                .clone()
                                                                .unwrap_or_default(),
                                                            a: ka,
                                                            b: kb,
                                                            action,
                                                        });
                                                }
                                            };
                                        pick(ui, "auto", None);
                                        pick(ui, "hidden", Some(EdgeAction::Hide));
                                        pick(ui, "passage", Some(EdgeAction::Connector));
                                        ui.separator();
                                        for dir in [
                                            crate::core::layout_engine::direction::Dir::North,
                                            crate::core::layout_engine::direction::Dir::Northeast,
                                            crate::core::layout_engine::direction::Dir::East,
                                            crate::core::layout_engine::direction::Dir::Southeast,
                                            crate::core::layout_engine::direction::Dir::South,
                                            crate::core::layout_engine::direction::Dir::Southwest,
                                            crate::core::layout_engine::direction::Dir::West,
                                            crate::core::layout_engine::direction::Dir::Northwest,
                                            crate::core::layout_engine::direction::Dir::Up,
                                            crate::core::layout_engine::direction::Dir::Down,
                                        ] {
                                            pick(
                                                ui,
                                                dir.name(),
                                                Some(EdgeAction::Direction(dir)),
                                            );
                                        }
                                    });
                            });
                        }
                    });
                }
                ui.separator();
                if ui.button("Close").clicked() {
                    ex.selected = None;
                }
            });
    }

    fn explorer_canvas(
        ui: &mut egui::Ui,
        app_core: &crate::core::AppCore,
        ex: &mut MapExplorerState,
        out: &mut ExplorerOutput,
    ) {
        let map = &app_core.map;
        egui::CentralPanel::default().show(ui, |ui| {
            let Some(scene) = ex.location.as_deref().and_then(|loc| map.scene_for(loc))
            else {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("No map yet").weak());
                });
                return;
            };
            let sheet = scene.sheet(ex.sheet);
            if sheet.rooms.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Nothing on this sheet").weak());
                });
                return;
            }

            if !ex.centered {
                ex.center = Pos2::new(
                    (sheet.min.x + sheet.max.x) as f32 / 2.0,
                    (sheet.min.y + sheet.max.y) as f32 / 2.0,
                );
                ex.centered = true;
            }

            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());

            // A room hit at this screen position (edit-mode drag targets).
            let room_at = |pos: Pos2| -> Option<&crate::core::layout_engine::scene::SceneRoom> {
                let half = ((ex.px_per_cell * 0.55).clamp(3.0, 26.0)) / 2.0;
                sheet.rooms.iter().find(|room| {
                    let center = rect.center()
                        + Vec2::new(
                            (room.cell.x as f32 - ex.center.x) * ex.px_per_cell,
                            (room.cell.y as f32 - ex.center.y) * ex.px_per_cell,
                        );
                    (pos - center).abs().max_elem() <= half
                })
            };

            if ex.edit_mode && response.drag_started() {
                if let Some(room) = response.interact_pointer_pos().and_then(room_at) {
                    let alt = ui.input(|i| i.modifiers.alt);
                    ex.drag = Some(DragState {
                        group: room.group,
                        room: alt.then_some(room.id),
                        accum: Vec2::ZERO,
                    });
                }
            }

            // Drag: move an edit target, else pan (and stop following —
            // otherwise the camera snaps back).
            if response.dragged() && response.drag_delta() != Vec2::ZERO {
                if let Some(drag) = &mut ex.drag {
                    drag.accum += response.drag_delta();
                } else {
                    ex.center -= response.drag_delta() / ex.px_per_cell;
                    ex.follow = false;
                }
            }

            // Ghost preview of the dragged group/room at the snapped offset.
            if let Some(drag) = &ex.drag {
                let delta = Cell {
                    x: (drag.accum.x / ex.px_per_cell).round() as i32,
                    y: (drag.accum.y / ex.px_per_cell).round() as i32,
                };
                let mut min = Cell { x: i32::MAX, y: i32::MAX };
                let mut max = Cell { x: i32::MIN, y: i32::MIN };
                for room in &sheet.rooms {
                    if room.group != drag.group {
                        continue;
                    }
                    if let Some(only) = drag.room {
                        if room.id != only {
                            continue;
                        }
                    }
                    min.x = min.x.min(room.cell.x);
                    min.y = min.y.min(room.cell.y);
                    max.x = max.x.max(room.cell.x);
                    max.y = max.y.max(room.cell.y);
                }
                if min.x <= max.x {
                    let to_screen = |cx: f32, cy: f32| {
                        rect.center()
                            + Vec2::new(
                                (cx - ex.center.x) * ex.px_per_cell,
                                (cy - ex.center.y) * ex.px_per_cell,
                            )
                    };
                    let pad = ex.px_per_cell * 0.4;
                    let ghost = Rect::from_min_max(
                        to_screen((min.x + delta.x) as f32, (min.y + delta.y) as f32),
                        to_screen((max.x + delta.x) as f32, (max.y + delta.y) as f32),
                    )
                    .expand(pad);
                    ui.painter().with_clip_rect(rect).rect_stroke(
                        ghost,
                        4.0,
                        egui::Stroke::new(2.0, ui.visuals().warn_fg_color),
                        egui::StrokeKind::Outside,
                    );
                }
            }

            // Release: commit the snapped delta as one override edit.
            if response.drag_stopped() {
                if let Some(drag) = ex.drag.take() {
                    let delta = Cell {
                        x: (drag.accum.x / ex.px_per_cell).round() as i32,
                        y: (drag.accum.y / ex.px_per_cell).round() as i32,
                    };
                    if (delta.x != 0 || delta.y != 0) && ex.location.is_some() {
                        let location = ex.location.clone().unwrap_or_default();
                        out.override_edit = Some(match drag.room {
                            Some(id) => {
                                let key = scene
                                    .room(id)
                                    .and_then(|(_, r)| r.uid)
                                    .unwrap_or(id as i64);
                                let group_off = scene
                                    .group_offsets
                                    .get(&drag.group)
                                    .copied()
                                    .unwrap_or_default();
                                let final_cell = scene
                                    .room(id)
                                    .map(|(_, r)| r.cell)
                                    .unwrap_or_default();
                                OverrideEdit::RoomPin {
                                    location,
                                    key,
                                    pin: Some(Cell {
                                        x: final_cell.x - group_off.x + delta.x,
                                        y: final_cell.y - group_off.y + delta.y,
                                    }),
                                }
                            }
                            None => OverrideEdit::GroupOffset {
                                location,
                                anchor: scene
                                    .group_anchors
                                    .get(&drag.group)
                                    .copied()
                                    .unwrap_or_default(),
                                delta,
                            },
                        });
                    }
                }
            }
            // Scroll / pinch to zoom, anchored at the pointer.
            if response.hovered() {
                let (scroll, pinch) =
                    ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
                let factor = pinch * (1.0 + scroll * 0.0015);
                if (factor - 1.0).abs() > f32::EPSILON {
                    let old_ppc = ex.px_per_cell;
                    let new_ppc = (old_ppc * factor).clamp(4.0, 72.0);
                    if let Some(pointer) = response.hover_pos() {
                        let offset = pointer - rect.center();
                        let anchor = ex.center + offset / old_ppc;
                        ex.center = anchor - offset / new_ppc;
                    }
                    ex.px_per_cell = new_ppc;
                }
            }

            let camera = MapCamera {
                center: ex.center,
                px_per_cell: ex.px_per_cell,
            };
            let style = MapStyle::from_visuals(ui.visuals());
            let current = if map.current_location == ex.location {
                map.current_room_id
            } else {
                None
            };
            let exits = (current.is_some())
                .then(|| app_core.game_state.compass_dirs.as_slice());
            let result = map_view::paint_sheet(
                ui, rect, sheet, camera, current, exits, true, None, &style,
            );

            if let Some(id) = result.double_clicked_room {
                out.walk_to = Some(id);
            } else if let Some(id) = result.clicked_room {
                ex.selected = Some(id);
            }

            // Selection ring over the paint.
            if let Some((sheet_kind, room)) =
                ex.selected.and_then(|id| scene.room(id))
            {
                if sheet_kind == ex.sheet {
                    let center = rect.center()
                        + Vec2::new(
                            (room.cell.x as f32 - ex.center.x) * ex.px_per_cell,
                            (room.cell.y as f32 - ex.center.y) * ex.px_per_cell,
                        );
                    let size = (ex.px_per_cell * 0.55).clamp(3.0, 26.0) + 6.0;
                    ui.painter().with_clip_rect(rect).rect_stroke(
                        Rect::from_center_size(center, Vec2::splat(size)),
                        3.0,
                        ui.visuals().selection.stroke,
                        egui::StrokeKind::Outside,
                    );
                }
            }
        });
    }
}
