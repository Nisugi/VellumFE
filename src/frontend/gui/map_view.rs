//! Shared egui renderer for generated map scenes (spec §8 presentation).
//!
//! Both the mini map widget and the map explorer paint through here: rooms as
//! squares, solid directional edges, dashed connectors with movement labels,
//! stub arrows for stretched edges, door markers on entrance rooms, and the
//! current-room highlight. The caller owns the camera (center cell + pixels
//! per cell); this module just draws one sheet into a rect and reports what
//! the pointer did.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use crate::core::layout_engine::scene::{SceneEdgeKind, SheetScene};

/// Camera over cell space: which cell sits at the rect center, and the zoom.
#[derive(Debug, Clone, Copy)]
pub struct MapCamera {
    pub center: Pos2, // cell coordinates (fractional)
    pub px_per_cell: f32,
}

impl MapCamera {
    pub fn centered_on_cell(x: i32, y: i32, px_per_cell: f32) -> Self {
        MapCamera {
            center: Pos2::new(x as f32, y as f32),
            px_per_cell: px_per_cell.clamp(2.0, 96.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MapViewResult {
    pub clicked_room: Option<u32>,
    pub double_clicked_room: Option<u32>,
    pub hovered_room: Option<u32>,
}

/// Colors derived from the current egui theme so the map respects light/dark
/// and stays readable without its own palette config (skin hooks can come
/// later).
pub struct MapStyle {
    pub room_fill: Color32,
    pub room_stroke: Stroke,
    pub entrance: Color32,
    pub directional: Stroke,
    pub connector: Stroke,
    pub label: Color32,
    pub current_fill: Color32,
    pub current_ring: Stroke,
}

impl MapStyle {
    pub fn from_visuals(visuals: &egui::Visuals) -> Self {
        let accent = visuals.selection.bg_fill;
        MapStyle {
            room_fill: visuals.widgets.inactive.bg_fill,
            room_stroke: Stroke::new(1.0, visuals.widgets.inactive.fg_stroke.color),
            entrance: visuals.warn_fg_color,
            directional: Stroke::new(1.5, visuals.widgets.noninteractive.fg_stroke.color),
            connector: Stroke::new(1.0, visuals.weak_text_color()),
            label: visuals.weak_text_color(),
            current_fill: accent,
            current_ring: Stroke::new(2.0, visuals.strong_text_color()),
        }
    }
}

/// Paint one sheet of a scene into `rect`. When `interactive`, visible rooms
/// respond to hover (title tooltip) and click.
pub fn paint_sheet(
    ui: &mut egui::Ui,
    rect: Rect,
    sheet: &SheetScene,
    camera: MapCamera,
    current_room: Option<u32>,
    interactive: bool,
    // group_filter: draw only these groups (the mini map shows just the
    // building — cluster of groups — the character is in on the interiors
    // sheet); None = the whole sheet.
    group_filter: Option<&std::collections::HashSet<usize>>,
    style: &MapStyle,
) -> MapViewResult {
    let painter = ui.painter().with_clip_rect(rect);
    let ppc = camera.px_per_cell;
    let to_screen = |cx: f32, cy: f32| -> Pos2 {
        rect.center() + Vec2::new((cx - camera.center.x) * ppc, (cy - camera.center.y) * ppc)
    };

    // Visible cell window (with one cell of slack) for culling.
    let half_w = rect.width() / 2.0 / ppc + 1.0;
    let half_h = rect.height() / 2.0 / ppc + 1.0;
    let visible = |cx: f32, cy: f32| -> bool {
        (cx - camera.center.x).abs() <= half_w && (cy - camera.center.y).abs() <= half_h
    };

    let room_size = (ppc * 0.55).clamp(3.0, 26.0);
    let show_labels = ppc >= 12.0;

    // --- Edges (under rooms) ---
    for edge in &sheet.edges {
        if group_filter.is_some_and(|set| !set.contains(&edge.group)) {
            continue;
        }
        let (ax, ay) = (edge.a.x as f32, edge.a.y as f32);
        let (bx, by) = (edge.b.x as f32, edge.b.y as f32);
        if !visible(ax, ay) && !visible(bx, by) {
            continue;
        }
        let a = to_screen(ax, ay);
        let b = to_screen(bx, by);
        match edge.kind {
            SceneEdgeKind::Directional => {
                painter.line_segment([a, b], style.directional);
            }
            SceneEdgeKind::Connector => {
                painter.extend(egui::Shape::dashed_line(
                    &[a, b],
                    style.connector,
                    ppc * 0.25,
                    ppc * 0.18,
                ));
                if show_labels {
                    if let Some(label) = &edge.label {
                        painter.text(
                            a.lerp(b, 0.5),
                            Align2::CENTER_CENTER,
                            label,
                            FontId::proportional((ppc * 0.5).clamp(8.0, 14.0)),
                            style.label,
                        );
                    }
                }
            }
            SceneEdgeKind::Stub => {
                // Short dashed arrow leaving each end toward the partner,
                // labeled with the partner's room id (spec §8).
                let dir = (b - a).normalized();
                let stub_len = ppc * 0.9;
                for (from, toward, partner) in
                    [(a, dir, edge.b_room), (b, -dir, edge.a_room)]
                {
                    let tip = from + toward * stub_len;
                    painter.extend(egui::Shape::dashed_line(
                        &[from, tip],
                        style.connector,
                        ppc * 0.18,
                        ppc * 0.12,
                    ));
                    if show_labels {
                        painter.text(
                            tip + toward * 2.0,
                            Align2::CENTER_CENTER,
                            partner.to_string(),
                            FontId::proportional((ppc * 0.45).clamp(7.0, 12.0)),
                            style.label,
                        );
                    }
                }
            }
        }
    }

    // --- Group labels (interiors sheet) ---
    if show_labels {
        for label in &sheet.labels {
            if group_filter.is_some_and(|set| !set.contains(&label.group)) {
                continue;
            }
            let (cx, cy) = (label.cell.x as f32, label.cell.y as f32);
            if !visible(cx, cy) {
                continue;
            }
            painter.text(
                to_screen(cx, cy) - Vec2::new(room_size / 2.0, room_size),
                Align2::LEFT_BOTTOM,
                &label.text,
                FontId::proportional((ppc * 0.5).clamp(9.0, 14.0)),
                style.label,
            );
        }
    }

    // --- Rooms ---
    let mut result = MapViewResult::default();
    for room in &sheet.rooms {
        if group_filter.is_some_and(|set| !set.contains(&room.group)) {
            continue;
        }
        let (cx, cy) = (room.cell.x as f32, room.cell.y as f32);
        if !visible(cx, cy) {
            continue;
        }
        let center = to_screen(cx, cy);
        let room_rect = Rect::from_center_size(center, Vec2::splat(room_size));
        let is_current = current_room == Some(room.id);

        if is_current {
            painter.rect(
                room_rect.expand(2.0),
                2.0,
                style.current_fill,
                style.current_ring,
                egui::StrokeKind::Outside,
            );
        }
        painter.rect(
            room_rect,
            1.5,
            style.room_fill,
            style.room_stroke,
            egui::StrokeKind::Middle,
        );
        if room.entrance {
            // Door marker: a small warm dot on the room's top edge.
            painter.circle_filled(
                room_rect.center_top(),
                (room_size * 0.18).clamp(1.5, 4.0),
                style.entrance,
            );
        }

        if interactive {
            let response = ui.interact(
                room_rect,
                ui.id().with(("map_room", room.id)),
                Sense::click(),
            );
            if response.double_clicked() && result.double_clicked_room.is_none() {
                result.double_clicked_room = Some(room.id);
            } else if response.clicked() && result.clicked_room.is_none() {
                result.clicked_room = Some(room.id);
            }
            if response.hovered() {
                result.hovered_room = Some(room.id);
                response.on_hover_text(format!("{} ({})", room.title, room.id));
            }
        }
    }

    result
}
