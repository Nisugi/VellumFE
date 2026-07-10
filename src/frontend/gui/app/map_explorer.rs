//! Map Explorer: a separate native OS window (egui multi-viewport, same
//! mechanism as detached tabs) for browsing any location's generated map —
//! location picker, outdoor/interiors sheets, drag-pan, scroll-zoom, room
//! inspection, and walk-to. The override editor builds on this surface.

use eframe::egui::{self, Pos2, Rect, Sense, Vec2, ViewportBuilder, ViewportId};

use crate::core::layout_engine::scene::Sheet;
use crate::core::map_service::DbState;
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
        }
    }
}

#[derive(Default)]
struct ExplorerOutput {
    close: bool,
    walk_to: Option<u32>,
    request_location: Option<String>,
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
                                    if ui.selectable_label(is_current, location).clicked()
                                        && !is_current
                                    {
                                        ex.location = Some(location.to_owned());
                                        ex.follow = false;
                                        ex.selected = None;
                                        ex.centered = false;
                                        ex.sheet = Sheet::Outdoor;
                                        out.request_location = Some(location.to_owned());
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
                if let Some(room) = room {
                    ui.separator();
                    ui.label(egui::RichText::new("Exits").strong());
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (target, cmd) in &room.wayto {
                            ui.label(format!("{cmd} \u{2192} {target}"));
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

            // Drag to pan (and stop following — otherwise it snaps back).
            if response.dragged() && response.drag_delta() != Vec2::ZERO {
                ex.center -= response.drag_delta() / ex.px_per_cell;
                ex.follow = false;
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
            let result =
                map_view::paint_sheet(ui, rect, sheet, camera, current, true, &style);

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
