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
    pub room_id: Color32,
    pub room_stroke: Stroke,
    pub entrance: Color32,
    pub directional: Stroke,
    pub connector: Stroke,
    pub label: Color32,
    /// Chip painted behind label text so it stays readable over rooms.
    pub label_bg: Color32,
    pub current_fill: Color32,
    pub current_ring: Stroke,
    /// Fill for rooms in `highlight_rooms` (creature spawn highlighting).
    pub highlight_fill: Color32,
}

impl MapStyle {
    pub fn from_visuals(visuals: &egui::Visuals) -> Self {
        let accent = visuals.selection.bg_fill;
        MapStyle {
            room_fill: visuals.widgets.inactive.bg_fill,
            room_id: visuals.weak_text_color(),
            room_stroke: Stroke::new(1.0, visuals.widgets.inactive.fg_stroke.color),
            entrance: visuals.warn_fg_color,
            directional: Stroke::new(1.5, visuals.widgets.noninteractive.fg_stroke.color),
            connector: Stroke::new(1.0, visuals.weak_text_color()),
            label: visuals.weak_text_color(),
            label_bg: visuals.extreme_bg_color.gamma_multiply(0.75),
            current_fill: accent,
            current_ring: Stroke::new(2.0, visuals.strong_text_color()),
            highlight_fill: Color32::from_rgb(0xc2, 0x41, 0x4d),
        }
    }

    /// Override the creature-highlight fill (config `map.creature_highlight`).
    pub fn with_highlight(mut self, color: Color32) -> Self {
        self.highlight_fill = color;
        self
    }

    /// Override the accent-derived colors. `from_visuals` falls back to
    /// `selection.bg_fill`, which under a skin carries the readability-
    /// adjusted menu fill — callers with access to the raw accent (the
    /// app's `widget_accent`) pass it here.
    pub fn with_accent(mut self, accent: Color32) -> Self {
        self.current_fill = accent;
        self
    }
}

/// Terrain hue for a mapdb terrain string, or None for neutral ground
/// ("hard, flat" streets, "none", unknown). The full GS4 vocabulary is
/// ~19 values; matching is by keyword so variants ("deciduous" vs
/// "deciduous forest") share a family.
fn terrain_hue(terrain: &str) -> Option<Color32> {
    let t = terrain.trim().to_ascii_lowercase();
    if t.is_empty() || t == "none" || t == "hard, flat" {
        return None;
    }
    let c = |r, g, b| Some(Color32::from_rgb(r, g, b));
    if t.contains("coniferous") {
        c(46, 110, 74) // deep pine green
    } else if t.contains("deciduous") || t.contains("tropical") {
        c(84, 140, 66) // leafy green
    } else if t.contains("grass") || t.contains("cultivated") {
        c(128, 154, 62) // yellow-green fields
    } else if t.contains("scrub") {
        c(140, 132, 78) // dusty olive
    } else if t.contains("riparian") || t.contains("wetland") {
        c(64, 128, 128) // waterside teal
    } else if t.contains("sandy") {
        c(190, 168, 110) // sand
    } else if t.contains("dirt") || t.contains("muddy") {
        c(134, 100, 66) // earth brown
    } else if t.contains("mountain") || t.contains("hilly") || t.contains("rough") {
        c(122, 116, 110) // slate
    } else if t.contains("subterranean") {
        c(96, 90, 104) // cave violet-grey
    } else if t.contains("icy") || t.contains("glacier") {
        c(150, 190, 210) // pale ice
    } else {
        None
    }
}

/// Room fill for a terrain: the terrain hue pulled toward the theme fill so
/// tinted rooms stay readable on both light and dark grounds and untinted
/// rooms don't look foreign next to them.
fn terrain_fill(terrain: Option<&str>, base: Color32) -> Color32 {
    let Some(hue) = terrain.and_then(terrain_hue) else {
        return base;
    };
    let mix = |a: u8, b: u8| ((a as u16 * 60 + b as u16 * 40) / 100) as u8;
    Color32::from_rgb(
        mix(hue.r(), base.r()),
        mix(hue.g(), base.g()),
        mix(hue.b(), base.b()),
    )
}

/// Connectors at or under this cell length keep the dashed line (a local
/// "go arch" between adjacent rooms reads better as a line); anything
/// longer renders as paired dots unless the edge carries a Dash override.
const CONNECTOR_DASH_MAX_CELLS: f32 = 2.5;

/// Stable per-category color for service-tag markers, same recipe as the
/// connector pair colors: hash the tag name onto a golden-ratio hue wheel,
/// darker/steadier than the pair dots so markers read as chrome, not links.
fn service_tag_color(tag: &str) -> Color32 {
    let mut h: u32 = 0x811c9dc5;
    for byte in tag.bytes() {
        h ^= byte as u32;
        h = h.wrapping_mul(0x01000193);
    }
    let hue = (h as f32 / u32::MAX as f32 * 0.618_034).fract();
    egui::ecolor::Hsva::new(hue, 0.65, 0.62, 1.0).into()
}

/// Stable per-pair color for long-connector dots: the same two rooms get
/// the same hue every session, and different pairs spread across a wheel of
/// well-separated hues. Saturation/value sit high enough to read on both
/// light and dark grounds.
fn connector_pair_color(a: u32, b: u32) -> Color32 {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    // FNV-1a over the pair for a stable, well-mixed hue index.
    let mut h: u32 = 0x811c9dc5;
    for byte in lo.to_le_bytes().into_iter().chain(hi.to_le_bytes()) {
        h ^= byte as u32;
        h = h.wrapping_mul(0x01000193);
    }
    // Golden-ratio spacing keeps nearby hash values far apart on the wheel.
    let hue = (h as f32 / u32::MAX as f32 * 0.618_034).fract();
    egui::ecolor::Hsva::new(hue, 0.75, 0.85, 1.0).into()
}

/// A text label whose paint is deferred until after the rooms. `candidates`
/// lists alternative anchor positions in preference order; the first whose
/// spot is free of room squares wins, so labels land in empty space instead
/// of on rooms. The chip background is the fallback when every spot is taken.
struct DeferredLabel {
    candidates: Vec<Pos2>,
    align: Align2,
    text: String,
    font_size: f32,
}

fn paint_deferred_labels(
    painter: &egui::Painter,
    labels: Vec<DeferredLabel>,
    style: &MapStyle,
    is_free: impl Fn(Rect) -> bool,
) {
    // Labels also avoid each other (short adjacent connectors would stack
    // their labels otherwise); the first candidate + chip is the fallback.
    let mut placed: Vec<Rect> = Vec::new();
    for label in labels {
        let galley = painter.layout_no_wrap(
            label.text,
            FontId::proportional(label.font_size),
            style.label,
        );
        let mut rect = label.align.anchor_size(label.candidates[0], galley.size());
        for candidate in &label.candidates {
            let r = label.align.anchor_size(*candidate, galley.size());
            if is_free(r) && !placed.iter().any(|p| p.intersects(r)) {
                rect = r;
                break;
            }
        }
        placed.push(rect);
        painter.rect_filled(rect.expand(2.0), 3.0, style.label_bg);
        painter.galley(rect.min, galley, style.label);
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
    // Available exits of the current room ("n", "sw", ...), drawn as accent
    // ticks around its square so the live compass reads on the map.
    current_exits: Option<&[String]>,
    interactive: bool,
    // group_filter: draw only these groups (the mini map shows just the
    // building — cluster of groups — the character is in on the interiors
    // sheet); None = the whole sheet.
    group_filter: Option<&std::collections::HashSet<usize>>,
    // Service-tag categories drawn as room markers (config pinned_tags);
    // empty = no markers.
    pinned_tags: &[String],
    // Rooms to tint in the highlight color (creature spawn rooms for the
    // selected creature); empty = no highlighting.
    highlight_rooms: &std::collections::HashSet<u32>,
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
    let show_connector_labels = ppc >= 14.0;
    let show_room_ids = ppc >= 20.0;
    // Text painted after the rooms, so a label can never be buried under a
    // room square (they used to paint in the edge pass, beneath everything).
    let mut deferred_labels: Vec<DeferredLabel> = Vec::new();
    // Long-connector pair dots, painted after the rooms so they sit on the
    // squares' shoulders instead of underneath them.
    let mut deferred_dots: Vec<(Pos2, f32, Color32)> = Vec::new();

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
            SceneEdgeKind::Connector | SceneEdgeKind::DotPair | SceneEdgeKind::ForcedDash => {
                // Auto rule: long connectors read as dot pairs (a dashed
                // line that long just crosses the sheet as noise); short
                // local hops keep the quiet dash. Per-edge overrides trump
                // it in both directions: Dots forces dots at any length,
                // Dash (ForcedDash) forces the line at any length.
                let as_dots = match edge.kind {
                    SceneEdgeKind::DotPair => true,
                    SceneEdgeKind::ForcedDash => false,
                    _ => (ax - bx).hypot(ay - by) > CONNECTOR_DASH_MAX_CELLS,
                };
                if as_dots {
                    // No line at all: a matching-color dot on each linked
                    // room's shoulder (toward its partner) says "these two
                    // connect" without crossing the sheet.
                    let color = connector_pair_color(edge.a_room, edge.b_room);
                    let dot_r = (ppc * 0.14).clamp(2.0, 5.0);
                    let dir = (b - a).normalized();
                    for (from, toward) in [(a, dir), (b, -dir)] {
                        let dot = from + toward * (room_size * 0.5 + dot_r + 1.5);
                        deferred_dots.push((dot, dot_r, color));
                        if show_connector_labels {
                            if let Some(label) = &edge.label {
                                deferred_labels.push(DeferredLabel {
                                    candidates: vec![
                                        dot + toward * (dot_r + ppc * 0.45),
                                        dot + Vec2::new(0.0, -ppc * 0.55),
                                        dot + Vec2::new(0.0, ppc * 0.55),
                                    ],
                                    align: Align2::CENTER_CENTER,
                                    text: label.clone(),
                                    font_size: (ppc * 0.4).clamp(7.0, 12.0),
                                });
                            }
                        }
                    }
                } else {
                    painter.extend(egui::Shape::dashed_line(
                        &[a, b],
                        style.connector,
                        ppc * 0.25,
                        ppc * 0.18,
                    ));
                    if show_connector_labels {
                        if let Some(label) = &edge.label {
                            // Slide along the line, then perpendicular of
                            // the midpoint, hunting for a room-free spot.
                            let perp = {
                                let d = (b - a).normalized();
                                Vec2::new(-d.y, d.x) * ppc * 0.75
                            };
                            let mid = a.lerp(b, 0.5);
                            deferred_labels.push(DeferredLabel {
                                candidates: vec![
                                    mid,
                                    a.lerp(b, 0.35),
                                    a.lerp(b, 0.65),
                                    mid + perp,
                                    mid - perp,
                                ],
                                align: Align2::CENTER_CENTER,
                                text: label.clone(),
                                font_size: (ppc * 0.45).clamp(8.0, 13.0),
                            });
                        }
                    }
                }
            }
            SceneEdgeKind::Stub => {
                // Short dashed arrow leaving each end toward the partner,
                // labeled with the partner's room id (spec §8).
                let dir = (b - a).normalized();
                let stub_len = ppc * 0.9;
                for (from, toward, partner) in [(a, dir, edge.b_room), (b, -dir, edge.a_room)] {
                    let tip = from + toward * stub_len;
                    painter.extend(egui::Shape::dashed_line(
                        &[from, tip],
                        style.connector,
                        ppc * 0.18,
                        ppc * 0.12,
                    ));
                    // Arrowhead at the tip.
                    let head = (ppc * 0.22).clamp(3.0, 7.0);
                    let perp = Vec2::new(-toward.y, toward.x);
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            tip + toward * head,
                            tip - toward * head * 0.4 + perp * head * 0.6,
                            tip - toward * head * 0.4 - perp * head * 0.6,
                        ],
                        style.connector.color,
                        Stroke::NONE,
                    ));
                    if show_labels {
                        deferred_labels.push(DeferredLabel {
                            candidates: vec![tip + toward * 2.0, tip + toward * (ppc * 0.6)],
                            align: Align2::CENTER_CENTER,
                            text: partner.to_string(),
                            font_size: (ppc * 0.45).clamp(7.0, 12.0),
                        });
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
            // Anchored above the cluster's top-left; walk upward (then aside)
            // until the text sits on empty cells.
            let base = to_screen(cx, cy) - Vec2::new(room_size / 2.0, room_size);
            deferred_labels.push(DeferredLabel {
                candidates: vec![
                    base,
                    base - Vec2::new(0.0, ppc),
                    base - Vec2::new(0.0, ppc * 2.0),
                    base - Vec2::new(ppc, 0.0),
                ],
                align: Align2::LEFT_BOTTOM,
                text: label.text.clone(),
                font_size: (ppc * 0.5).clamp(9.0, 14.0),
            });
        }
    }

    // --- Rooms ---
    let mut result = MapViewResult::default();
    // Visible room rects, kept for label collision avoidance below.
    let mut room_rects: Vec<Rect> = Vec::new();
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
        room_rects.push(room_rect);
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
        // Creature-spawn highlight replaces the terrain fill so tinted
        // rooms read at a glance; the ring/markers are untouched.
        let fill = if highlight_rooms.contains(&room.id) {
            style.highlight_fill
        } else {
            terrain_fill(room.terrain.as_deref(), style.room_fill)
        };
        painter.rect(
            room_rect,
            1.5,
            fill,
            style.room_stroke,
            egui::StrokeKind::Middle,
        );
        if !pinned_tags.is_empty() {
            if let Some(tag) = room
                .service_tags
                .iter()
                .find(|t| pinned_tags.iter().any(|p| p == *t))
            {
                // Service marker: a category-colored disc on the room's
                // top-right corner, with the category's initial when
                // zoomed in enough to read it.
                let color = service_tag_color(tag);
                let r = (room_size * 0.32).clamp(2.5, 7.0);
                let center = room_rect.right_top();
                painter.circle(center, r, color, Stroke::new(1.0, style.label_bg));
                if ppc >= 14.0 {
                    painter.text(
                        center,
                        Align2::CENTER_CENTER,
                        tag.chars().next().unwrap_or('?').to_uppercase().to_string(),
                        FontId::proportional((r * 1.4).clamp(7.0, 11.0)),
                        Color32::WHITE,
                    );
                }
            }
        }
        if room.entrance {
            // Door marker: a small warm dot on the room's top edge.
            painter.circle_filled(
                room_rect.center_top(),
                (room_size * 0.18).clamp(1.5, 4.0),
                style.entrance,
            );
        }
        if show_room_ids {
            painter.text(
                room_rect.center_bottom() + Vec2::new(0.0, 1.0),
                Align2::CENTER_TOP,
                room.id.to_string(),
                FontId::proportional((ppc * 0.32).clamp(8.0, 11.0)),
                style.room_id,
            );
        }
        if is_current {
            if let Some(exits) = current_exits {
                paint_exit_ticks(&painter, room_rect, exits, style);
            }
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

    // --- Long-connector pair dots, on the rooms' shoulders ---
    for (pos, r, color) in deferred_dots {
        painter.circle_filled(pos, r, color);
    }

    // --- Labels, on top and hunting for empty space (a label under a room
    // is decoration; the chip is only the last resort) ---
    paint_deferred_labels(&painter, deferred_labels, style, |r| {
        !room_rects.iter().any(|room| room.intersects(r))
    });

    result
}

/// Paint the session's ghost-room sketch over a sheet: dashed dim squares
/// hanging off their anchor rooms, dashed traversal edges with the crossing
/// command as label. Deliberately unmistakable from mapped rooms — truth is
/// solid, inference is dashed.
pub fn paint_ghosts(
    ui: &mut egui::Ui,
    rect: Rect,
    overlay: &crate::core::ghost_rooms::GhostOverlay,
    camera: MapCamera,
    current_ghost: Option<i64>,
    // Live compass exits, drawn on the current ghost like on a mapped room.
    current_exits: Option<&[String]>,
    style: &MapStyle,
) {
    let painter = ui.painter().with_clip_rect(rect);
    let ppc = camera.px_per_cell;
    let to_screen = |cx: f32, cy: f32| -> Pos2 {
        rect.center() + Vec2::new((cx - camera.center.x) * ppc, (cy - camera.center.y) * ppc)
    };
    let half_w = rect.width() / 2.0 / ppc + 1.0;
    let half_h = rect.height() / 2.0 / ppc + 1.0;
    let visible = |cx: f32, cy: f32| -> bool {
        (cx - camera.center.x).abs() <= half_w && (cy - camera.center.y).abs() <= half_h
    };

    let room_size = (ppc * 0.55).clamp(3.0, 26.0);
    let show_labels = ppc >= 14.0;
    let fill = style.room_fill.gamma_multiply(0.5);
    let outline = Stroke::new(1.0, style.connector.color);

    for edge in &overlay.edges {
        let (ax, ay) = (edge.a.x as f32, edge.a.y as f32);
        let (bx, by) = (edge.b.x as f32, edge.b.y as f32);
        if !visible(ax, ay) && !visible(bx, by) {
            continue;
        }
        let a = to_screen(ax, ay);
        let b = to_screen(bx, by);
        painter.extend(egui::Shape::dashed_line(
            &[a, b],
            style.connector,
            ppc * 0.18,
            ppc * 0.14,
        ));
        if show_labels && chebyshev_px(a, b) >= ppc * 1.9 {
            if let Some(label) = &edge.label {
                painter.text(
                    a.lerp(b, 0.5),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional((ppc * 0.45).clamp(8.0, 13.0)),
                    style.label,
                );
            }
        }
    }

    for node in &overlay.nodes {
        let (cx, cy) = (node.cell.x as f32, node.cell.y as f32);
        if !visible(cx, cy) {
            continue;
        }
        let center = to_screen(cx, cy);
        let room_rect = Rect::from_center_size(center, Vec2::splat(room_size));
        let is_current = current_ghost == Some(node.uid);

        if is_current {
            painter.rect(
                room_rect.expand(2.0),
                2.0,
                style.current_fill.gamma_multiply(0.6),
                style.current_ring,
                egui::StrokeKind::Outside,
            );
        }
        painter.rect_filled(room_rect, 1.5, fill);
        dashed_rect_outline(&painter, room_rect, outline, ppc);
        if is_current {
            if let Some(exits) = current_exits {
                paint_exit_ticks(&painter, room_rect, exits, style);
            }
        }

        let response = ui.interact(
            room_rect,
            ui.id().with(("ghost_room", node.uid)),
            Sense::hover(),
        );
        if response.hovered() {
            let title = node.title.as_deref().unwrap_or("unknown room");
            response.on_hover_text(format!("{title} - unmapped (session sketch)"));
        }
    }
}

fn dashed_rect_outline(painter: &egui::Painter, rect: Rect, stroke: Stroke, ppc: f32) {
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    for side in corners.windows(2) {
        painter.extend(egui::Shape::dashed_line(
            &[side[0], side[1]],
            stroke,
            (ppc * 0.12).max(2.0),
            (ppc * 0.08).max(1.5),
        ));
    }
}

fn chebyshev_px(a: Pos2, b: Pos2) -> f32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// Accent ticks on the current room's square for each available compass
/// exit (up/down/out have no planar heading and are skipped).
fn paint_exit_ticks(painter: &egui::Painter, room_rect: Rect, exits: &[String], style: &MapStyle) {
    let half = room_rect.width() / 2.0;
    let len = (half * 0.8).clamp(3.0, 8.0);
    let stroke = Stroke::new(2.0, style.current_ring.color);
    for exit in exits {
        let dir = match exit.as_str() {
            "n" => Vec2::new(0.0, -1.0),
            "s" => Vec2::new(0.0, 1.0),
            "e" => Vec2::new(1.0, 0.0),
            "w" => Vec2::new(-1.0, 0.0),
            "ne" => Vec2::new(1.0, -1.0).normalized(),
            "nw" => Vec2::new(-1.0, -1.0).normalized(),
            "se" => Vec2::new(1.0, 1.0).normalized(),
            "sw" => Vec2::new(-1.0, 1.0).normalized(),
            _ => continue,
        };
        let from = room_rect.center() + dir * (half + 2.0);
        painter.line_segment([from, from + dir * len], stroke);
    }
}
