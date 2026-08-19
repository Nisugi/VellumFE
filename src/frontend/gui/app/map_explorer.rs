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
    /// Name buffer for "New map + move" in the membership editor.
    new_map_buffer: String,
    // Room-data editor buffers (P4).
    /// Creature selected in the Creatures section; its spawn rooms tint on
    /// the map. Session-only — a viewing aid, not map data.
    selected_creature: Option<String>,
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
            new_map_buffer: String::new(),
            selected_creature: None,
        }
    }
}

#[derive(Default)]
struct ExplorerOutput {
    close: bool,
    walk_to: Option<u32>,
    request_location: Option<String>,
    override_edits: Vec<OverrideEdit>,
    /// Service-tag category to pin/unpin in config.map.pinned_tags.
    toggle_pinned_tag: Option<String>,
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
            if self.app_core.config.go2.native_map_clicks {
                self.app_core.start_travel(id);
            } else {
                self.dispatch_raw_command(format!(";go2 {id}"));
            }
        }
        for edit in out.override_edits {
            self.app_core.map.apply_override_edit(edit);
        }
        if let Some(tag) = out.toggle_pinned_tag {
            let pinned = &mut self.app_core.config.map.pinned_tags;
            match pinned.iter().position(|t| *t == tag) {
                Some(i) => {
                    pinned.remove(i);
                }
                None => pinned.push(tag),
            }
            if let Err(e) = self.app_core.save_config() {
                tracing::warn!("pinned-tags config save failed: {e}");
            }
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
                let selected_text = ex
                    .location
                    .as_deref()
                    .map(|key| map.display_name(key))
                    .unwrap_or("(no location)");
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
                        // With curated membership: curated maps first, then
                        // satellites (auto components — the set the filter
                        // box exists for). Fallback mode lists locations.
                        let entries: Vec<(String, String)> =
                            if let Some(membership) = map.membership() {
                                membership
                                    .list_maps()
                                    .into_iter()
                                    .map(|(key, name, size, curated)| {
                                        let label = if curated {
                                            name
                                        } else {
                                            format!("{name} ({size})")
                                        };
                                        (key, label)
                                    })
                                    .collect()
                            } else if let Some(db) = map.mapdb() {
                                db.locations()
                                    .map(|l| (l.to_owned(), l.to_owned()))
                                    .collect()
                            } else {
                                Vec::new()
                            };
                        let curated_count = map
                            .membership()
                            .map(|m| entries.iter().filter(|(k, _)| m.is_curated(k)).count())
                            .unwrap_or(0);
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                for (index, (key, label)) in entries.iter().enumerate() {
                                    if !filter.is_empty()
                                        && !label.to_lowercase().contains(&filter)
                                        && !key.to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                    if curated_count > 0 && filter.is_empty() {
                                        if index == 0 {
                                            ui.weak("Curated maps");
                                        } else if index == curated_count {
                                            ui.separator();
                                            ui.weak("Satellites");
                                        }
                                    }
                                    let is_current = ex.location.as_deref() == Some(key.as_str());
                                    if ui.selectable_label(is_current, label).clicked() {
                                        if !is_current {
                                            ex.location = Some(key.clone());
                                            ex.follow = false;
                                            ex.selected = None;
                                            ex.centered = false;
                                            ex.sheet = Sheet::Outdoor;
                                            out.request_location = Some(key.clone());
                                        }
                                        ui.close();
                                    }
                                }
                            });
                    });

                ui.separator();
                let scene = ex.location.as_deref().and_then(|loc| map.scene_for(loc));
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
                ui.menu_button("Markers", |ui| {
                    ui.label("Service markers on rooms");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for &tag in crate::core::mapdb::SERVICE_TAGS {
                                let mut pinned =
                                    app_core.config.map.pinned_tags.iter().any(|t| t == tag);
                                if ui.checkbox(&mut pinned, tag).changed() {
                                    out.toggle_pinned_tag = Some(tag.to_string());
                                }
                            }
                        });
                })
                .response
                .on_hover_text("Pick which service tags get room markers (bank, inn, ...)");

                ui.separator();
                if ui
                    .toggle_value(&mut ex.follow, "Follow")
                    .on_hover_text("Track the character's room")
                    .changed()
                    && ex.follow
                {
                    ex.last_revision = 0; // force a resync next frame
                }
                if ui
                    .button("Center")
                    .on_hover_text("Center on the character (or the map)")
                    .clicked()
                {
                    ex.centered = false;
                    if let Some(id) = map.current_room_id {
                        if let Some((sheet, room)) = map.current_scene().and_then(|s| s.room(id)) {
                            if map.current_location == ex.location {
                                ex.sheet = sheet;
                                ex.center = Pos2::new(room.cell.x as f32, room.cell.y as f32);
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
                ui.toggle_value(&mut ex.edit_mode, "Edit").on_hover_text(
                    "Drag a group to move it (Alt: single room); edits save as overrides",
                );
                if ex.edit_mode {
                    let count = ex
                        .location
                        .as_deref()
                        .and_then(|loc| map.overrides_for(loc))
                        .map(|ov| ov.group_offsets.len() + ov.room_pins.len() + ov.names.len())
                        .unwrap_or(0);
                    if count > 0 {
                        if ui
                            .button(format!("Reset overrides ({count})"))
                            .on_hover_text("Drop every override for this location")
                            .clicked()
                        {
                            if let Some(loc) = ex.location.clone() {
                                out.override_edits
                                    .push(OverrideEdit::ResetLocation { location: loc });
                            }
                        }
                    }
                }

                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| match map.db_state() {
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
                                        scene.outdoor.rooms.len() + scene.interiors.rooms.len()
                                    ));
                                }
                            }
                        }
                    },
                );
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
        // Full room record. Global lookup by id — the browsed key is a
        // CURATED map name under membership, not a mapdb location string,
        // so a per-location slice lookup comes back empty and every
        // sidebar section silently vanishes (the 2026-08-17 bug).
        let room = map.mapdb().and_then(|db| db.room(selected));

        egui::Panel::right("map_explorer_room")
            .default_size(240.0)
            .show(ui, |ui| {
                ui.heading(if scene_room.title.is_empty() {
                    "(untitled room)"
                } else {
                    &scene_room.title
                });
                // Mockup order: knowledge collapsibles first, identity
                // block under them, authoring controls at the bottom.
                if let Some(room) = room {
                    if !room.description.is_empty() {
                        ui.collapsing("Description", |ui| {
                            for d in &room.description {
                                ui.label(d);
                            }
                        });
                    }
                    if !room.paths.is_empty() {
                        ui.collapsing("Exits or Paths", |ui| {
                            ui.label(&room.paths);
                        });
                    }
                }
                ui.separator();
                ui.label(format!("id: {selected}"));
                if let Some(uid) = scene_room.uid {
                    ui.label(format!("uid: {uid}"));
                }
                if ui.button("Walk here").clicked() {
                    out.walk_to = Some(selected);
                }
                ui.separator();
                if let Some(loc) = map.mapdb().and_then(|db| db.location_of_room_id(selected)) {
                    ui.label(format!("Location: {loc}"));
                }
                let evidence = scene_room.uid.and_then(|uid| app_core.evidence.get(uid));
                let sense = evidence.and_then(|ev| ev.sense.as_ref());
                // Sense words win the display over mapdb fields — fresher.
                let climate = sense
                    .and_then(|s| s.data.climate.as_deref())
                    .or_else(|| room.and_then(|r| r.climate.as_deref()));
                let terrain = sense
                    .and_then(|s| s.data.terrain.as_deref())
                    .or_else(|| room.and_then(|r| r.terrain.as_deref()));
                ui.label(format!("Climate: {}", climate.unwrap_or("")));
                ui.label(format!("Terrain: {}", terrain.unwrap_or("")));
                if let Some(s) = sense {
                    if !s.data.wildlife.is_empty() {
                        ui.label(format!("Wildlife: {}", s.data.wildlife.join(", ")));
                    }
                    if let Some(o) = &s.data.overhead {
                        ui.label(format!("Overhead: {o}"));
                    }
                    if !s.data.structures.is_empty() {
                        ui.label(format!("Structures: {}", s.data.structures.join("; ")));
                    }
                }
                // Timeto: the routing costs on their own, so a missing cost
                // (the "wayto exists but pathing skips it" case) is easy to
                // spot without reading commands.
                if let Some(room) = room {
                    if !room.wayto.is_empty() {
                        ui.collapsing("Timeto", |ui| {
                            use crate::core::mapdb::TimeTo;
                            for target in room.wayto.keys() {
                                let label = match room.timeto.get(target) {
                                    Some(TimeTo::Seconds(s)) => {
                                        format!("{target}: {s:.1}s")
                                    }
                                    Some(TimeTo::Proc(_)) => {
                                        format!("{target}: script")
                                    }
                                    None => format!("{target}: none (not routable)"),
                                };
                                if room.timeto.contains_key(target) {
                                    ui.label(label);
                                } else {
                                    ui.label(egui::RichText::new(label).weak());
                                }
                            }
                        });
                    }
                }
                // Wayto: the movement commands. In edit mode each entry
                // grows a connection-type dropdown (how the edge draws and
                // lays out — not where it goes).
                if let Some(room) = room {
                    egui::CollapsingHeader::new("Wayto")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(260.0)
                                .show(ui, |ui| {
                                    for (target, cmd) in &room.wayto {
                                        // Command + cost. A wayto without a
                                        // timeto is NOT routable (Lich skips
                                        // it); procs cost script-time.
                                        use crate::core::mapdb::TimeTo;
                                        let cmd_label =
                                            if crate::core::mapdb::is_proc_command(cmd) {
                                                "(script)".to_string()
                                            } else {
                                                cmd.clone()
                                            };
                                        let target_title = map
                                            .mapdb()
                                            .and_then(|db| db.room(*target))
                                            .and_then(|r| r.title.first().cloned())
                                            .unwrap_or_default();
                                        let label = match room.timeto.get(target) {
                                            Some(TimeTo::Seconds(s)) => format!(
                                                "{cmd_label} \u{2192} {target} ({s:.0}s)"
                                            ),
                                            Some(TimeTo::Proc(_)) => format!(
                                                "{cmd_label} \u{2192} {target} (script cost)"
                                            ),
                                            None => format!(
                                                "{cmd_label} \u{2192} {target} (not routable)"
                                            ),
                                        };
                                        let text =
                                            if room.timeto.contains_key(target) {
                                                egui::RichText::new(&label)
                                            } else {
                                                egui::RichText::new(&label).weak()
                                            };
                                        if !ex.edit_mode {
                                            ui.label(text).on_hover_text(&target_title);
                                            continue;
                                        }
                                        // Edge action editor, keyed by the room-key pair.
                                        let target_key = map
                                            .mapdb()
                                            .and_then(|db| db.room(*target))
                                            .map(|r| r.uid.first().copied().unwrap_or(r.id as i64));
                                        let Some(target_key) = target_key else {
                                            ui.label(text).on_hover_text(&target_title);
                                            continue;
                                        };
                                        let my_key = scene_room.uid.unwrap_or(selected as i64);
                                        let (ka, kb) =
                                            (my_key.min(target_key), my_key.max(target_key));
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
                                            ui.label(text).on_hover_text(&target_title);
                                            let text = match current_action {
                                                None => "auto".to_string(),
                                                Some(EdgeAction::Hide) => "hidden".to_string(),
                                                Some(EdgeAction::Dash) => "dashed".to_string(),
                                                Some(EdgeAction::Dots) => "dots".to_string(),
                                                Some(EdgeAction::Connector) => {
                                                    "passage".to_string()
                                                }
                                                Some(EdgeAction::Direction(d)) => {
                                                    d.name().to_string()
                                                }
                                            };
                                            egui::ComboBox::from_id_salt((
                                                "map_edge", ka, kb, *target,
                                            ))
                                            .selected_text(text)
                                            .width(90.0)
                                            .show_ui(
                                                ui,
                                                |ui| {
                                                    let mut pick =
                                            |ui: &mut egui::Ui,
                                             label: &str,
                                             hint: &str,
                                             action: Option<EdgeAction>| {
                                                let resp = ui.selectable_label(
                                                    current_action == action,
                                                    label,
                                                );
                                                let resp = if hint.is_empty() {
                                                    resp
                                                } else {
                                                    resp.on_hover_text(hint)
                                                };
                                                if resp.clicked()
                                                    && current_action != action
                                                {
                                                    out.override_edits
                                                        .push(OverrideEdit::Edge {
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
                                                    pick(ui, "auto", "", None);
                                                    pick(
                                                        ui,
                                                        "hidden",
                                                        "Don't draw this edge (looks only)",
                                                        Some(EdgeAction::Hide),
                                                    );
                                                    pick(
                                                        ui,
                                                        "dashed",
                                                        "Draw dashed, keep the layout (looks only)",
                                                        Some(EdgeAction::Dash),
                                                    );
                                                    pick(
                                                        ui,
                                                        "dots",
                                                        "No line: matching-color dot on both rooms (looks only)",
                                                        Some(EdgeAction::Dots),
                                                    );
                                                    pick(
                                            ui,
                                            "passage",
                                            "No direction: un-weld the rooms and re-layout",
                                            Some(EdgeAction::Connector),
                                        );
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
                                                "Force this direction and re-layout",
                                                Some(EdgeAction::Direction(dir)),
                                            );
                                        }
                                                },
                                            );
                                        });
                                    }
                                });
                        });
                }
                // Tags split by exclusion: service/structured stay under
                // Tags, everything else reads as a forageable.
                let (service_tags, forage_tags) = room
                    .map(|r| crate::core::mapdb::partition_tags(&r.tags))
                    .unwrap_or_default();
                let session_forage = evidence.and_then(|ev| ev.forage.as_ref());
                if !forage_tags.is_empty() || session_forage.is_some() {
                    ui.collapsing("Forageables", |ui| {
                        for item in &forage_tags {
                            ui.label(*item);
                        }
                        if let Some(forage) = session_forage {
                            for item in &forage.items {
                                if !forage_tags.iter().any(|t| t == item) {
                                    ui.label(format!("{item} (seen this session)"));
                                }
                            }
                        }
                    });
                }
                if !service_tags.is_empty() {
                    ui.collapsing("Tags", |ui| {
                        for tag in &service_tags {
                            ui.label(*tag);
                        }
                    });
                }
                // Creatures spawning here. Not mapdb data: Saga's spawn
                // tables (uid ranges per generator) baked into the bundled
                // bestiary, so it's editorial reference, not something the
                // map editor can change.
                if let Some(uid) = scene_room.uid.filter(|u| *u > 0) {
                    let creatures =
                        crate::core::bestiary::format::shared().here(uid as u64);
                    if !creatures.is_empty() {
                        egui::CollapsingHeader::new(format!("Creatures ({})", creatures.len()))
                            .show(ui, |ui| {
                                for entry in &creatures {
                                    let label = match entry.level {
                                        Some(level) => format!("{} (level {level})", entry.name),
                                        None => entry.name.clone(),
                                    };
                                    let mut text = egui::RichText::new(label);
                                    if entry.boss {
                                        text = text.strong();
                                    }
                                    let selected =
                                        ex.selected_creature.as_deref() == Some(&entry.name);
                                    let mut hover =
                                        String::from("Click to highlight every room it \
                                                      spawns in on this map\n");
                                    if entry.undead {
                                        hover.push_str("undead\n");
                                    }
                                    if let Some(t) = &entry.creature_type {
                                        hover.push_str(&format!("type: {t}\n"));
                                    }
                                    if let Some(hp) = entry.max_hp {
                                        hover.push_str(&format!("max HP: {hp}\n"));
                                    }
                                    if ui
                                        .selectable_label(selected, text)
                                        .on_hover_text(hover.trim_end())
                                        .clicked()
                                    {
                                        // Click toggles; only one at a time.
                                        ex.selected_creature = if selected {
                                            None
                                        } else {
                                            Some(entry.name.clone())
                                        };
                                    }
                                }
                                if ex.selected_creature.is_some()
                                    && ui.button("Clear highlight").clicked()
                                {
                                    ex.selected_creature = None;
                                }
                                ui.label(
                                    egui::RichText::new("from Saga spawn tables")
                                        .weak()
                                        .small(),
                                );
                            });
                    }
                }
                if ex.edit_mode {
                    ui.separator();
                    egui::CollapsingHeader::new("Group").show(ui, |ui| {
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
                                out.override_edits.push(OverrideEdit::GroupName {
                                    location: ex.location.clone().unwrap_or_default(),
                                    anchor,
                                    name: Some(ex.rename_buffer.trim().to_string()),
                                });
                            }
                            if ui.button("Clear name").clicked() {
                                out.override_edits.push(OverrideEdit::GroupName {
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
                                    out.override_edits.push(OverrideEdit::Sheet {
                                        location: ex.location.clone().unwrap_or_default(),
                                        anchor,
                                        choice,
                                    });
                                }
                            }
                        });
                    }
                    });
                    // Map membership: move this room between curated/user
                    // maps (rosters only — the layout engine still places
                    // everything). Purgatory is the always-available staging
                    // map; "New map + move" mints a user map and moves the
                    // room there in one action.
                    ui.separator();
                    egui::CollapsingHeader::new("Map membership").show(ui, |ui| {
                    if let Some(membership) = map.membership() {
                        use crate::core::map_service::{PURGATORY_KEY, PURGATORY_NAME};
                        let current_map =
                            membership.map_of_room(selected).map(str::to_string);
                        if let Some(cur) = &current_map {
                            ui.label(format!("In: {}", membership.display_name(cur)));
                        }
                        let uids: Vec<i64> =
                            room.map(|r| r.uid.clone()).unwrap_or_default();
                        if uids.is_empty() {
                            ui.label(
                                egui::RichText::new("(room has no uid: not movable)").weak(),
                            );
                        } else {
                            let mut targets: Vec<(String, String)> = membership
                                .list_maps()
                                .into_iter()
                                .filter(|(_, _, _, curated)| *curated)
                                .map(|(key, name, _, _)| (key, name))
                                .collect();
                            if !targets.iter().any(|(k, _)| k == PURGATORY_KEY) {
                                targets.push((
                                    PURGATORY_KEY.to_string(),
                                    PURGATORY_NAME.to_string(),
                                ));
                            }
                            egui::ComboBox::from_id_salt("map_move_target")
                                .selected_text("Move to map\u{2026}")
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    for (key, name) in &targets {
                                        if current_map.as_deref() == Some(key.as_str()) {
                                            continue;
                                        }
                                        if ui.selectable_label(false, name).clicked() {
                                            out.override_edits.push(
                                                OverrideEdit::MembershipMove {
                                                    uids: uids.clone(),
                                                    to: Some(key.clone()),
                                                },
                                            );
                                            ui.close();
                                        }
                                    }
                                });
                            if uids
                                .iter()
                                .any(|&u| map.personal_membership_move(u).is_some())
                                && ui
                                    .button("Revert move")
                                    .on_hover_text(
                                        "Drop the personal move; the room returns to \
                                         its curated/community map",
                                    )
                                    .clicked()
                            {
                                out.override_edits.push(OverrideEdit::MembershipMove {
                                    uids: uids.clone(),
                                    to: None,
                                });
                            }
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut ex.new_map_buffer)
                                        .hint_text("new map name")
                                        .desired_width(130.0),
                                );
                                if ui.button("New map + move").clicked()
                                    && !ex.new_map_buffer.trim().is_empty()
                                {
                                    let name = ex.new_map_buffer.trim().to_string();
                                    let key =
                                        crate::core::map_service::MapService::user_map_key(
                                            &name,
                                        );
                                    out.override_edits
                                        .push(OverrideEdit::CreateMap {
                                            key: key.clone(),
                                            name,
                                        });
                                    out.override_edits.push(
                                        OverrideEdit::MembershipMove {
                                            uids: uids.clone(),
                                            to: Some(key),
                                        },
                                    );
                                    ex.new_map_buffer.clear();
                                }
                            });
                        }
                    }
                    });
                    let key = scene_room.uid.unwrap_or(selected as i64);                    let pinned = ex
                        .location
                        .as_deref()
                        .and_then(|loc| map.overrides_for(loc))
                        .map(|ov| ov.room_pins.contains_key(&key))
                        .unwrap_or(false);
                    if pinned && ui.button("Unpin room").clicked() {
                        out.override_edits.push(OverrideEdit::RoomPin {
                            location: ex.location.clone().unwrap_or_default(),
                            key,
                            pin: None,
                        });
                    }
                }                ui.separator();
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
            let Some(scene) = ex.location.as_deref().and_then(|loc| map.scene_for(loc)) else {
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
                let mut min = Cell {
                    x: i32::MAX,
                    y: i32::MAX,
                };
                let mut max = Cell {
                    x: i32::MIN,
                    y: i32::MIN,
                };
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
                        out.override_edits.push(match drag.room {
                            Some(id) => {
                                let key =
                                    scene.room(id).and_then(|(_, r)| r.uid).unwrap_or(id as i64);
                                let group_off = scene
                                    .group_offsets
                                    .get(&drag.group)
                                    .copied()
                                    .unwrap_or_default();
                                let final_cell =
                                    scene.room(id).map(|(_, r)| r.cell).unwrap_or_default();
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
                let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
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
            let style = MapStyle::from_visuals(ui.visuals())
                .with_accent(super::widgets::widget_accent(ui.ctx(), ui.visuals()))
                .with_highlight(
                    super::widgets::parse_hex_color(&app_core.config.map.creature_highlight)
                        .unwrap_or(egui::Color32::from_rgb(0xc2, 0x41, 0x4d)),
                );
            let browsing_here = map.current_location == ex.location;
            // Session ghost sketches: anchored clusters land on whichever
            // sheet their anchor room is drawn on (no group filter — the
            // explorer shows everything). Cartography mode only.
            let ghost_overlay = (app_core.config.map.mapping_mode && !map.ghosts().is_empty())
                .then(|| {
                    crate::core::ghost_rooms::build_overlay(map.ghosts(), scene, ex.sheet, None)
                });
            // While standing in a ghost, the ring and compass belong to the
            // sketch, not the held anchor room (mirrors the mini map).
            let in_ghost = browsing_here
                && map.current_ghost.is_some_and(|uid| {
                    ghost_overlay
                        .as_ref()
                        .is_some_and(|o| o.cell_of(uid).is_some())
                });
            let current = if browsing_here && !in_ghost {
                map.current_room_id
            } else {
                None
            };
            let exits = (current.is_some()).then(|| app_core.game_state.compass_dirs.as_slice());
            // Rooms on THIS sheet where the selected creature spawns.
            let highlight_rooms: std::collections::HashSet<u32> = match &ex.selected_creature {
                Some(name) => {
                    let entries = crate::core::bestiary::format::shared();
                    let ranges: Vec<(u64, u64)> = entries
                        .entries
                        .iter()
                        .find(|e| &e.name == name)
                        .map(|e| {
                            e.spawns
                                .iter()
                                .flat_map(|s| s.uids.iter().copied())
                                .collect()
                        })
                        .unwrap_or_default();
                    sheet
                        .rooms
                        .iter()
                        .filter(|room| {
                            room.uid.is_some_and(|uid| {
                                uid > 0
                                    && ranges
                                        .iter()
                                        .any(|&(lo, hi)| (uid as u64) >= lo && (uid as u64) <= hi)
                            })
                        })
                        .map(|room| room.id)
                        .collect()
                }
                None => std::collections::HashSet::new(),
            };
            let result = map_view::paint_sheet(
                ui,
                rect,
                sheet,
                camera,
                current,
                exits,
                true,
                None,
                &app_core.config.map.pinned_tags,
                &highlight_rooms,
                &style,
            );
            if let Some(overlay) = ghost_overlay.as_ref().filter(|o| !o.is_empty()) {
                map_view::paint_ghosts(
                    ui,
                    rect,
                    overlay,
                    camera,
                    if browsing_here {
                        map.current_ghost
                    } else {
                        None
                    },
                    if in_ghost {
                        Some(app_core.game_state.compass_dirs.as_slice())
                    } else {
                        None
                    },
                    &style,
                );
            }

            if let Some(id) = result.double_clicked_room {
                out.walk_to = Some(id);
            } else if let Some(id) = result.clicked_room {
                ex.selected = Some(id);
            }

            // Selection ring over the paint.
            if let Some((sheet_kind, room)) = ex.selected.and_then(|id| scene.room(id)) {
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
