//! Vector border styles for GUI window frames.
//!
//! The shared layout definition has always carried `border_style` and
//! `border_sides` for the TUI's glyph borders; this module maps them onto
//! egui painting so the same layout reads equivalently in both frontends.
//! Glyph styles with no vector analog (`quadrant_*`) approximate as wide
//! strokes. Skin border art overrides all of this: the plan collapses to
//! the frame default so `apply_skin_border_to_frame` owns the frame.

use super::*;

/// How to draw one window's frame border this frame, resolved from the
/// shared layout def before the window renders.
pub(super) struct WindowBorderPlan {
    /// Suppress egui's frame stroke (hidden borders, and custom-painted
    /// styles that replace it).
    hide_frame_stroke: bool,
    /// Force the frame's corner radius: "rounded" guarantees round corners
    /// even when the radius settings say square.
    frame_radius: Option<u8>,
    /// Extra inner margin on bordered sides so wide painted styles don't
    /// underlap content.
    margin_bump: i8,
    /// Border painted over the rendered window on its own layer; None uses
    /// the plain frame stroke.
    paint: Option<BorderSpec>,
}

pub(super) struct BorderSpec {
    style: PaintStyle,
    sides: crate::config::BorderSides,
    color: Color32,
    radius: f32,
}

#[derive(Clone, Copy)]
enum PaintStyle {
    Single,
    Double,
    Thick,
    QuadrantInside,
    QuadrantOutside,
}

impl PaintStyle {
    /// Total band width, which is also the content inset the style needs.
    fn width(self) -> f32 {
        match self {
            Self::Single => 1.0,
            Self::Double => 5.0, // outer 1px + 3px gap + inner 1px
            Self::Thick => 3.0,
            Self::QuadrantInside | Self::QuadrantOutside => 4.0,
        }
    }
}

impl WindowBorderPlan {
    /// Leave the frame exactly as egui built it.
    fn frame_default() -> Self {
        Self {
            hide_frame_stroke: false,
            frame_radius: None,
            margin_bump: 0,
            paint: None,
        }
    }

    fn hidden() -> Self {
        Self {
            hide_frame_stroke: true,
            frame_radius: None,
            margin_bump: 0,
            paint: None,
        }
    }
}

impl VellumGuiApp {
    /// Resolve how `key`'s border should draw. Windows without a layout def
    /// keep the plain egui frame; skin border art collapses the plan to the
    /// default so the skin path owns the frame instead.
    pub(super) fn window_border_plan_for_tab(&self, key: &TabKey) -> WindowBorderPlan {
        if let Some(tab) = self.available_tabs.get(key) {
            if self.skin_state.border_for(&tab.window_name).is_some() {
                return WindowBorderPlan::frame_default();
            }
        }
        let Some(def) = self.layout_def_for_tab(key) else {
            return WindowBorderPlan::frame_default();
        };
        let base = def.base();
        if !base.show_border {
            return WindowBorderPlan::hidden();
        }
        let sides = base.border_sides.clone();
        let all_sides = sides.top && sides.bottom && sides.left && sides.right;
        let radius = self
            .corner_radius_override_for_tab(key)
            .unwrap_or(self.ui_settings.window_corner_radius)
            .clamp(0.0, 12.0);
        let style = match base.border_style.to_ascii_lowercase().as_str() {
            "none" => return WindowBorderPlan::hidden(),
            "double" => Some(PaintStyle::Double),
            "thick" => Some(PaintStyle::Thick),
            "quadrant_inside" => Some(PaintStyle::QuadrantInside),
            "quadrant_outside" => Some(PaintStyle::QuadrantOutside),
            // "rounded" is single-weight; it only forces rounding below.
            // Everything else ("single", "plain", unknown) is the default.
            _ => None,
        };
        let rounded = base.border_style.eq_ignore_ascii_case("rounded");
        let radius = if rounded { radius.max(6.0) } else { radius };
        let frame_radius = rounded.then_some(radius.round() as u8);
        match style {
            // Single-weight border on all four sides: egui's own frame
            // stroke already draws exactly this, pixel-identical to today.
            None if all_sides => WindowBorderPlan {
                hide_frame_stroke: false,
                frame_radius,
                margin_bump: 0,
                paint: None,
            },
            style => {
                let paint_style = style.unwrap_or(PaintStyle::Single);
                WindowBorderPlan {
                    hide_frame_stroke: true,
                    frame_radius,
                    margin_bump: (paint_style.width() - 1.0).ceil() as i8,
                    paint: Some(BorderSpec {
                        style: paint_style,
                        sides,
                        color: self.window_border_color_for_tab(key),
                        radius,
                    }),
                }
            }
        }
    }

    /// Accent/def/colors.toml border color, else the theme's window border.
    fn window_border_color_for_tab(&self, key: &TabKey) -> Color32 {
        self.accent_color_for_tab(key)
            .unwrap_or_else(|| theme::color32(self.current_theme.window_border))
    }

    /// Apply the plan's frame-level effects (stroke visibility, forced
    /// rounding, content inset). Must run before `apply_skin_border_to_frame`
    /// so the skin path's effects win.
    pub(super) fn apply_border_plan_to_frame(
        &self,
        plan: &WindowBorderPlan,
        frame: &mut egui::Frame,
    ) {
        if plan.hide_frame_stroke {
            frame.stroke = egui::Stroke::NONE;
        }
        if let Some(radius) = plan.frame_radius {
            frame.corner_radius = egui::CornerRadius::same(radius);
        }
        if let Some(spec) = &plan.paint {
            if plan.margin_bump > 0 {
                let margin = &mut frame.inner_margin;
                if spec.sides.top {
                    margin.top = margin.top.saturating_add(plan.margin_bump);
                }
                if spec.sides.bottom {
                    margin.bottom = margin.bottom.saturating_add(plan.margin_bump);
                }
                if spec.sides.left {
                    margin.left = margin.left.saturating_add(plan.margin_bump);
                }
                if spec.sides.right {
                    margin.right = margin.right.saturating_add(plan.margin_bump);
                }
            }
        }
    }

    /// Paint the plan's border over a rendered window, on the window's own
    /// layer so it moves and stacks with it (same trick as the skin border).
    pub(super) fn paint_border_plan(
        &self,
        ctx: &egui::Context,
        plan: &WindowBorderPlan,
        response: &egui::Response,
    ) {
        if let Some(spec) = &plan.paint {
            paint_border_spec(&ctx.layer_painter(response.layer_id), response.rect, spec);
        }
    }
}

fn paint_border_spec(painter: &egui::Painter, rect: Rect, spec: &BorderSpec) {
    match spec.style {
        PaintStyle::Single => stroke_sides(painter, rect, spec, 1.0, spec.radius),
        PaintStyle::Thick => stroke_sides(painter, rect, spec, 3.0, spec.radius),
        PaintStyle::QuadrantOutside => stroke_sides(painter, rect, spec, 4.0, spec.radius),
        // "inside" sits the band off the window edge, like the TUI's
        // half-cell glyphs hug the inner edge of their cells.
        PaintStyle::QuadrantInside => stroke_sides(
            painter,
            rect.shrink(1.0),
            spec,
            4.0,
            (spec.radius - 1.0).max(0.0),
        ),
        PaintStyle::Double => {
            stroke_sides(painter, rect, spec, 1.0, spec.radius);
            stroke_sides(
                painter,
                rect.shrink(3.0),
                spec,
                1.0,
                (spec.radius - 3.0).max(0.0),
            );
        }
    }
}

/// Stroke `rect`'s enabled sides, staying inside `rect`. All four sides use
/// one rounded rect-stroke; partial sides paint per-edge segments with
/// quarter-circle arcs at corners where both adjacent sides are on.
fn stroke_sides(
    painter: &egui::Painter,
    rect: Rect,
    spec: &BorderSpec,
    width: f32,
    radius: f32,
) {
    let stroke = egui::Stroke::new(width, spec.color);
    let sides = &spec.sides;
    if sides.top && sides.bottom && sides.left && sides.right {
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(radius.round() as u8),
            stroke,
            egui::StrokeKind::Inside,
        );
        return;
    }
    // Centerline rect: line segments paint centered on their coordinates,
    // so inset by half the width to match StrokeKind::Inside above.
    let r = rect.shrink(width / 2.0);
    let radius = radius.min(r.width() / 2.0).min(r.height() / 2.0);
    let arc_nw = sides.top && sides.left && radius > 0.0;
    let arc_ne = sides.top && sides.right && radius > 0.0;
    let arc_se = sides.bottom && sides.right && radius > 0.0;
    let arc_sw = sides.bottom && sides.left && radius > 0.0;
    if sides.top {
        let x0 = r.min.x + if arc_nw { radius } else { 0.0 };
        let x1 = r.max.x - if arc_ne { radius } else { 0.0 };
        painter.line_segment([egui::pos2(x0, r.min.y), egui::pos2(x1, r.min.y)], stroke);
    }
    if sides.bottom {
        let x0 = r.min.x + if arc_sw { radius } else { 0.0 };
        let x1 = r.max.x - if arc_se { radius } else { 0.0 };
        painter.line_segment([egui::pos2(x0, r.max.y), egui::pos2(x1, r.max.y)], stroke);
    }
    if sides.left {
        let y0 = r.min.y + if arc_nw { radius } else { 0.0 };
        let y1 = r.max.y - if arc_sw { radius } else { 0.0 };
        painter.line_segment([egui::pos2(r.min.x, y0), egui::pos2(r.min.x, y1)], stroke);
    }
    if sides.right {
        let y0 = r.min.y + if arc_ne { radius } else { 0.0 };
        let y1 = r.max.y - if arc_se { radius } else { 0.0 };
        painter.line_segment([egui::pos2(r.max.x, y0), egui::pos2(r.max.x, y1)], stroke);
    }
    let mut arc = |center: egui::Pos2, quadrant: f32| {
        let mut points = Vec::new();
        egui::epaint::tessellator::path::add_circle_quadrant(
            &mut points,
            center,
            radius,
            quadrant,
        );
        painter.add(egui::Shape::line(points, stroke));
    };
    // Quadrant indices follow egui's add_circle_quadrant convention:
    // 0 = SE, 1 = SW, 2 = NW, 3 = NE.
    if arc_nw {
        arc(egui::pos2(r.min.x + radius, r.min.y + radius), 2.0);
    }
    if arc_ne {
        arc(egui::pos2(r.max.x - radius, r.min.y + radius), 3.0);
    }
    if arc_se {
        arc(egui::pos2(r.max.x - radius, r.max.y - radius), 0.0);
    }
    if arc_sw {
        arc(egui::pos2(r.min.x + radius, r.max.y - radius), 1.0);
    }
}
