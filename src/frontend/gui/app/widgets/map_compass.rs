//! Mini-map and compass widgets, including the compass rose and
//! direction-arrow painters.

use super::*;

impl VellumGuiApp {
    /// Mini map: follows the current room (auto-centered, auto-switching
    /// between the outdoor sheet and the interior group the character is
    /// in); clicking a room walks there via `;go2`.
    pub(in crate::frontend::gui::app) fn render_map_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        map_data: &crate::data::MapData,
        zoom_override: Option<f32>,
    ) -> Option<GuiLinkClick> {
        use crate::core::map_service::DbState;
        use crate::frontend::gui::map_view::{self, MapCamera, MapStyle};

        let map = &app_core.map;
        let hint = |ui: &mut egui::Ui, text: &str| {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new(text).weak());
            });
        };
        match map.db_state() {
            DbState::NotLoaded => {
                hint(
                    ui,
                    "Download map data in Settings > Map (or point at your Lich folder)",
                );
                return None;
            }
            DbState::Loading => {
                ui.centered_and_justified(|ui| ui.spinner());
                return None;
            }
            DbState::Failed => {
                let msg = map
                    .db_error
                    .clone()
                    .unwrap_or_else(|| "mapdb load failed".to_string());
                hint(ui, &format!("Map unavailable: {msg}"));
                return None;
            }
            DbState::Loaded => {}
        }
        let Some(scene) = map.current_scene() else {
            let msg = if map.current_location.is_some() {
                "Generating map..."
            } else {
                "Waiting for a mapped room..."
            };
            hint(ui, msg);
            return None;
        };

        let current = map.current_room_id;
        let (sheet_kind, center, group_filter) = match current.and_then(|id| scene.room(id)) {
            // Indoors, show just the building the character is in (its whole
            // cluster of connected interior groups) — the full interiors
            // shelf is explorer territory.
            Some((sheet, room)) => (
                sheet,
                room.cell,
                (sheet == crate::core::layout_engine::Sheet::Interiors)
                    .then(|| scene.cluster_groups(room.group)),
            ),
            None => {
                let b = &scene.outdoor;
                (
                    crate::core::layout_engine::Sheet::Outdoor,
                    crate::core::layout_engine::Cell {
                        x: (b.min.x + b.max.x) / 2,
                        y: (b.min.y + b.max.y) / 2,
                    },
                    None,
                )
            }
        };

        // Unmapped interiors: session ghost sketches hang off their anchor
        // room; standing in one moves the camera and the current ring to it.
        // Rendered only in cartography mode — everyday play shows mapdb truth.
        let ghost_overlay = (app_core.config.map.mapping_mode && !map.ghosts().is_empty()).then(|| {
            crate::core::ghost_rooms::build_overlay(
                map.ghosts(),
                scene,
                sheet_kind,
                group_filter.as_ref(),
            )
        });
        let current_ghost_cell = map
            .current_ghost
            .and_then(|uid| ghost_overlay.as_ref()?.cell_of(uid));
        let center = current_ghost_cell.unwrap_or(center);

        let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        if rect.width() < 8.0 || rect.height() < 8.0 {
            return None;
        }
        // Glide toward the new room instead of jump-cutting.
        let cx = ui.ctx().animate_value_with_time(
            ui.id().with("map_center_x"),
            center.x as f32,
            0.25,
        );
        let cy = ui.ctx().animate_value_with_time(
            ui.id().with("map_center_y"),
            center.y as f32,
            0.25,
        );
        let camera = map_view::MapCamera {
            center: egui::Pos2::new(cx, cy),
            px_per_cell: zoom_override.unwrap_or(map_data.zoom).clamp(2.0, 96.0),
        };
        let style = MapStyle::from_visuals(ui.visuals())
            .with_accent(widget_accent(ui.ctx(), ui.visuals()));
        let compass = Some(app_core.game_state.compass_dirs.as_slice());
        // While standing in a ghost, the ring and compass ticks belong to the
        // sketch, not the held anchor room.
        let in_ghost = current_ghost_cell.is_some();
        let result = map_view::paint_sheet(
            ui,
            rect,
            scene.sheet(sheet_kind),
            camera,
            if in_ghost { None } else { current },
            if in_ghost { None } else { compass },
            true,
            group_filter.as_ref(),
            &style,
        );
        if let Some(overlay) = ghost_overlay.as_ref().filter(|o| !o.is_empty()) {
            map_view::paint_ghosts(
                ui,
                rect,
                overlay,
                camera,
                map.current_ghost,
                if in_ghost { compass } else { None },
                &style,
            );
        }

        // Travel progress rides on the map while the walk executor runs.
        if let (Some(task), Some(current)) = (app_core.travel.task(), map.current_room_id) {
            if let Some(db) = map.mapdb() {
                let done = task.rooms_total().saturating_sub(task.rooms_remaining());
                let label = format!(
                    "-> {} | {}/{} rooms | ETA {}",
                    task.destination,
                    done,
                    task.rooms_total(),
                    crate::core::travel::format_eta(task.eta_seconds(db, current))
                );
                let font = egui::FontId::proportional(12.0);
                let galley = ui.painter().layout_no_wrap(
                    label,
                    font.clone(),
                    ui.visuals().strong_text_color(),
                );
                let pad = egui::vec2(6.0, 3.0);
                let banner = egui::Rect::from_min_size(
                    rect.left_top() + egui::vec2(4.0, 4.0),
                    galley.size() + pad * 2.0,
                );
                let painter = ui.painter().with_clip_rect(rect);
                painter.rect_filled(
                    banner,
                    3.0,
                    ui.visuals().extreme_bg_color.gamma_multiply(0.85),
                );
                painter.galley(banner.min + pad, galley, ui.visuals().strong_text_color());
            }
        }

        result.clicked_room.map(|id| GuiLinkClick {
            link_data: Self::direct_command_link(if app_core.config.go2.native_map_clicks {
                format!(".go2 {id}")
            } else {
                format!(";go2 {id}")
            }),
            click_pos: Self::click_pos_to_grid(
                ui.ctx().pointer_latest_pos().unwrap_or(Pos2::ZERO),
            ),
        })
    }

    pub(in crate::frontend::gui::app) fn render_compass_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        compass_data: &crate::data::CompassData,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) -> Option<GuiLinkClick> {
        let mut clicked_link = None;
        let source_directions: &[String] = if compass_data.directions.is_empty() {
            &app_core.game_state.compass_dirs
        } else {
            &compass_data.directions
        };
        let available: HashSet<String> = source_directions
            .iter()
            .map(|direction| direction.to_ascii_lowercase())
            .collect();

        // A skinned compass integrates up/down into the rose sprite the way
        // Wrayth authored them, so there is no separate arrow column. Vector
        // mode keeps the rose square plus an up/down triangle column to its
        // right. Either way the art occupies a CENTERED square of
        // min(width, height): the window itself is free-form (grow one axis
        // and the art holds its size, grow both and it scales), so extra
        // space pads evenly around the rose instead of trailing off one
        // side.
        let has_skin_rose = skin_art.and_then(|art| art.compass_rose).is_some();

        if has_skin_rose {
            let side = ui
                .available_height()
                .min(ui.available_width())
                .max(40.0);
            let (outer, _) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
            let rect = Rect::from_center_size(outer.center(), Vec2::splat(side));
            if let Some(click) = Self::paint_compass_rose(ui, rect, &available, skin_art) {
                clicked_link = Some(click);
            }
        } else {
            ui.horizontal_centered(|ui| {
                // Rose square: whatever height we have, leaving room for the
                // up/down arrow column to the right. Out is the rose's hub.
                let arrow_side = (ui.available_height() * 0.28).clamp(14.0, 30.0);
                let side = ui
                    .available_height()
                    .min(ui.available_width() - arrow_side - 8.0)
                    .max(40.0);
                // Center the rose + arrow column pair in the leftover width.
                let content_width = side + arrow_side + 8.0;
                let lead = ((ui.available_width() - content_width) * 0.5).max(0.0);
                if lead > 0.0 {
                    ui.add_space(lead);
                }
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), egui::Sense::hover());
                if let Some(click) = Self::paint_compass_rose(ui, rect, &available, skin_art) {
                    clicked_link = Some(click);
                }

                ui.vertical(|ui| {
                    for (direction, points_up) in [("up", true), ("down", false)] {
                        if let Some(click) = Self::paint_vertical_arrow(
                            ui, arrow_side, direction, points_up, &available,
                        ) {
                            if clicked_link.is_none() {
                                clicked_link = Some(click);
                            }
                        }
                    }
                });
            });
        }

        clicked_link
    }

    /// One up/down movement arrow beside the compass rose: a triangle in
    /// the same color language as the rose (link color when the exit is
    /// available, faint outline otherwise), clickable like a rose arrow.
    pub(super) fn paint_vertical_arrow(
        ui: &mut egui::Ui,
        side: f32,
        direction: &str,
        points_up: bool,
        available: &HashSet<String>,
    ) -> Option<GuiLinkClick> {
        let is_available = available.contains(direction);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::splat(side),
            if is_available {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );

        let visuals = ui.visuals();
        let available_fill = visuals.hyperlink_color;
        let idle_stroke = visuals.widgets.noninteractive.bg_stroke.color;
        let (fill, stroke) = if !is_available {
            (Color32::TRANSPARENT, egui::Stroke::new(1.0, idle_stroke))
        } else if response.hovered() {
            let hover = Self::lighten(available_fill, 0.35);
            (hover, egui::Stroke::new(1.0, hover))
        } else {
            (available_fill, egui::Stroke::new(1.0, available_fill))
        };

        let inner = rect.shrink(side * 0.18);
        let points = if points_up {
            vec![
                Pos2::new(inner.center().x, inner.min.y),
                Pos2::new(inner.min.x, inner.max.y),
                Pos2::new(inner.max.x, inner.max.y),
            ]
        } else {
            vec![
                Pos2::new(inner.min.x, inner.min.y),
                Pos2::new(inner.max.x, inner.min.y),
                Pos2::new(inner.center().x, inner.max.y),
            ]
        };
        ui.painter()
            .add(egui::Shape::convex_polygon(points, fill, stroke));

        if is_available {
            let response = response
                .on_hover_text(direction.to_string())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                return Some(Self::gui_link_click_from_response(
                    &response,
                    ui,
                    Self::direct_command_link(direction.to_string()),
                ));
            }
        }
        None
    }

    /// Draw the compass rose into `rect`. Sprite mode (skin `[compass]`
    /// with a rose image) paints the rose plus a lit overlay per available
    /// direction, all aspect-fit to the same canvas. Vector mode draws
    /// eight arrows around a hub, available exits filled with the theme
    /// link color, the rest as faint outlines. Both modes share the same
    /// clickable hit regions and send the same movement commands.
    pub(super) fn paint_compass_rose(
        ui: &mut egui::Ui,
        rect: Rect,
        available: &HashSet<String>,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) -> Option<GuiLinkClick> {
        const DIRECTIONS: [(&str, &str); 8] = [
            ("n", "north"),
            ("ne", "northeast"),
            ("e", "east"),
            ("se", "southeast"),
            ("s", "south"),
            ("sw", "southwest"),
            ("w", "west"),
            ("nw", "northwest"),
        ];

        let mut clicked_link = None;
        let painter = ui.painter().with_clip_rect(rect);
        let center = rect.center();
        let radius = rect.width().min(rect.height()) * 0.5 - 2.0;
        if radius < 8.0 {
            return None;
        }

        let rose_sprite = skin_art.and_then(|art| art.compass_rose);
        if let Some(rose) = &rose_sprite {
            let dest = crate::frontend::gui::skin::sprite_dest(rose, rect);
            crate::frontend::gui::skin::paint_sprite(&painter, dest, rose, Color32::WHITE);
            let overlay_dirs = DIRECTIONS
                .iter()
                .map(|(direction, _)| *direction)
                .chain(["up", "down", "out"]);
            for direction in overlay_dirs {
                if !available.contains(direction) {
                    continue;
                }
                if let Some(overlay) = skin_art.and_then(|art| art.compass_dir(direction)) {
                    crate::frontend::gui::skin::paint_sprite(
                        &painter,
                        dest,
                        &overlay,
                        Color32::WHITE,
                    );
                }
            }
        }

        let visuals = ui.visuals().clone();
        let available_fill = visuals.hyperlink_color;
        let hover_fill = Self::lighten(available_fill, 0.35);
        let idle_stroke = visuals.widgets.noninteractive.bg_stroke.color;

        for (index, (direction, full_name)) in DIRECTIONS.iter().enumerate() {
            let is_cardinal = index % 2 == 0;
            let angle = index as f32 * std::f32::consts::FRAC_PI_4;
            let dir = Vec2::new(angle.sin(), -angle.cos());
            let perp = Vec2::new(-dir.y, dir.x);

            let tip_r = if is_cardinal { radius } else { radius * 0.78 };
            let base_r = radius * 0.3;
            let half_w = if is_cardinal { radius * 0.15 } else { radius * 0.11 };

            let is_available = available.contains(*direction);
            let hit_center = center + dir * ((tip_r + base_r) * 0.5);
            let hit_rect =
                Rect::from_center_size(hit_center, Vec2::splat((radius * 0.4).max(12.0)));
            let response = ui.interact(
                hit_rect,
                ui.id().with(("compass_rose", direction)),
                if is_available {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );

            if rose_sprite.is_none() {
                let points = vec![
                    center + dir * tip_r,
                    center + dir * base_r + perp * half_w,
                    center + dir * base_r - perp * half_w,
                ];
                let (fill, stroke) = if !is_available {
                    (
                        Color32::TRANSPARENT,
                        egui::Stroke::new(1.0, idle_stroke),
                    )
                } else if response.hovered() {
                    (hover_fill, egui::Stroke::new(1.0, hover_fill))
                } else {
                    (available_fill, egui::Stroke::new(1.0, available_fill))
                };
                painter.add(egui::Shape::convex_polygon(points, fill, stroke));
            }

            if is_available {
                let response = response
                    .on_hover_text(*full_name)
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.clicked() && clicked_link.is_none() {
                    clicked_link = Some(Self::gui_link_click_from_response(
                        &response,
                        ui,
                        Self::direct_command_link(direction.to_string()),
                    ));
                }
            }
        }

        // Hub doubles as the OUT exit: lit and clickable when the room has
        // one. Vector mode puts it over the arrow bases (rose center); the
        // integrated sprite compass draws OUT in the middle of the up/down
        // bar, so the hit region moves there to match the art.
        let out_available = available.contains("out");
        let hub_radius = radius * 0.18;
        let hub_center = if rose_sprite.is_some() {
            center + Vec2::new(radius * 0.72, 0.0)
        } else {
            center
        };
        let hub_response = ui.interact(
            Rect::from_center_size(hub_center, Vec2::splat(hub_radius * 2.0)),
            ui.id().with(("compass_rose", "out")),
            if out_available {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        if rose_sprite.is_none() {
            let hub_fill = if !out_available {
                visuals.window_fill()
            } else if hub_response.hovered() {
                hover_fill
            } else {
                available_fill
            };
            painter.circle(center, hub_radius, hub_fill, egui::Stroke::new(1.0, idle_stroke));
        }
        if out_available {
            let hub_response = hub_response
                .on_hover_text("out")
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if hub_response.clicked() && clicked_link.is_none() {
                clicked_link = Some(Self::gui_link_click_from_response(
                    &hub_response,
                    ui,
                    Self::direct_command_link("out".to_string()),
                ));
            }
        }

        // Sprite compasses integrate up/down into the rose art (Wrayth-style),
        // so their overlays are already painted above with the eight compass
        // directions. Add the matching clickable hit regions here — the
        // right-side spikes of the rose, where Wrayth draws the up/down arrows.
        // (Vector mode has its own separate up/down triangle column.)
        if rose_sprite.is_some() {
            for (direction, points_up) in [("up", true), ("down", false)] {
                if !available.contains(direction) {
                    continue;
                }
                let hit_center = center
                    + Vec2::new(radius * 0.72, if points_up { -radius * 0.5 } else { radius * 0.5 });
                let hit_rect =
                    Rect::from_center_size(hit_center, Vec2::splat((radius * 0.4).max(12.0)));
                let response = ui.interact(
                    hit_rect,
                    ui.id().with(("compass_rose", direction)),
                    egui::Sense::click(),
                );
                let response = response
                    .on_hover_text(direction)
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.clicked() && clicked_link.is_none() {
                    clicked_link = Some(Self::gui_link_click_from_response(
                        &response,
                        ui,
                        Self::direct_command_link(direction.to_string()),
                    ));
                }
            }
        }

        clicked_link
    }

    /// Blend a color toward white by `t` (0..=1), preserving alpha.
    pub(super) fn lighten(color: Color32, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        let channel = |c: u8| c.saturating_add(((255 - c) as f32 * t) as u8);
        Color32::from_rgba_unmultiplied(
            channel(color.r()),
            channel(color.g()),
            channel(color.b()),
            color.a(),
        )
    }

    /// Blend `a` toward `b` by `t` (0 = a, 1 = b), per channel. Used to dim an
    /// inactive tab's text toward the window background so only the active /
    /// unread tabs stand out.
    pub(super) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
        Color32::from_rgba_unmultiplied(
            mix(a.r(), b.r()),
            mix(a.g(), b.g()),
            mix(a.b(), b.b()),
            a.a(),
        )
    }

}
