//! Vector status icons for the dashboard and indicator widgets.
//!
//! Each standard GS4 indicator id (parser output: "KNEELING", "HIDDEN", ...)
//! gets a small pictogram drawn with painter geometry — no image assets,
//! scales with the widget, and takes whatever color the caller resolved
//! (severity or custom indicator color). Unknown ids are not painted;
//! callers fall back to the old text label so custom indicators keep
//! working. Skins can override these with sprites in a later phase.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

/// Ids with a pictogram. Matching is case-insensitive.
const SUPPORTED: &[&str] = &[
    "STANDING",
    "KNEELING",
    "SITTING",
    "PRONE",
    "DEAD",
    "STUNNED",
    "BLEEDING",
    "HIDDEN",
    "INVISIBLE",
    "WEBBED",
    "POISONED",
    "DISEASED",
    "JOINED",
];

pub(super) fn supported(id: &str) -> bool {
    SUPPORTED.iter().any(|known| known.eq_ignore_ascii_case(id))
}

/// Tooltip label: "KNEELING" -> "Kneeling".
pub(super) fn display_name(id: &str) -> String {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// Paint the icon for `id` into `rect` (uses the centered square of its
/// shorter side). `color` fills the pictogram; `bg` is used for carved-out
/// details (eyes, bubbles) and should match what's behind the icon.
/// Returns false without painting when the id has no pictogram.
pub(super) fn paint(
    painter: &egui::Painter,
    rect: Rect,
    id: &str,
    color: Color32,
    bg: Color32,
) -> bool {
    let side = rect.width().min(rect.height());
    if side < 6.0 {
        return supported(id);
    }
    let rect = Rect::from_center_size(rect.center(), Vec2::splat(side));
    let s = side;
    // Unit-coordinate helpers: everything below is specified in 0..=1.
    let at = |x: f32, y: f32| rect.min + Vec2::new(x * s, y * s);
    let limb = Stroke::new((s * 0.09).max(1.0), color);
    let thin = Stroke::new((s * 0.06).max(1.0), color);
    let line = |a: (f32, f32), b: (f32, f32), stroke: Stroke| {
        painter.line_segment([at(a.0, a.1), at(b.0, b.1)], stroke);
    };

    match id.to_ascii_uppercase().as_str() {
        "STANDING" => {
            painter.circle_filled(at(0.5, 0.16), s * 0.12, color);
            line((0.5, 0.28), (0.5, 0.62), limb);
            line((0.5, 0.38), (0.32, 0.52), limb);
            line((0.5, 0.38), (0.68, 0.52), limb);
            line((0.5, 0.62), (0.36, 0.90), limb);
            line((0.5, 0.62), (0.64, 0.90), limb);
        }
        "KNEELING" => {
            painter.circle_filled(at(0.52, 0.20), s * 0.12, color);
            line((0.52, 0.32), (0.52, 0.62), limb);
            line((0.52, 0.40), (0.36, 0.54), limb);
            line((0.52, 0.40), (0.68, 0.54), limb);
            // Front leg: knee up, shin planted.
            line((0.52, 0.62), (0.70, 0.70), limb);
            line((0.70, 0.70), (0.70, 0.90), limb);
            // Back leg: kneeling, shin along the ground.
            line((0.52, 0.62), (0.40, 0.90), limb);
            line((0.40, 0.90), (0.24, 0.90), limb);
        }
        "SITTING" => {
            painter.circle_filled(at(0.40, 0.20), s * 0.12, color);
            line((0.40, 0.32), (0.40, 0.62), limb);
            line((0.40, 0.42), (0.55, 0.54), limb);
            // Lap, then lower legs down.
            line((0.40, 0.62), (0.62, 0.62), limb);
            line((0.62, 0.62), (0.62, 0.90), limb);
        }
        "PRONE" => {
            painter.circle_filled(at(0.16, 0.66), s * 0.11, color);
            line((0.27, 0.66), (0.86, 0.66), limb);
            line((0.44, 0.66), (0.52, 0.78), limb);
            line((0.70, 0.66), (0.80, 0.78), limb);
            line((0.06, 0.88), (0.94, 0.88), thin);
        }
        "DEAD" => {
            // Tombstone: dome + slab, cross carved out in the background color.
            painter.circle_filled(at(0.5, 0.44), s * 0.28, color);
            painter.rect_filled(
                Rect::from_min_max(at(0.22, 0.44), at(0.78, 0.88)),
                0.0,
                color,
            );
            let carve = Stroke::new((s * 0.08).max(1.0), bg);
            painter.line_segment([at(0.5, 0.32), at(0.5, 0.62)], carve);
            painter.line_segment([at(0.40, 0.42), at(0.60, 0.42)], carve);
        }
        "STUNNED" => {
            // Impact burst: eight rays around a solid core.
            painter.circle_filled(at(0.5, 0.5), s * 0.10, color);
            for index in 0..8 {
                let angle = index as f32 * std::f32::consts::FRAC_PI_4;
                let dir = Vec2::new(angle.cos(), angle.sin());
                let inner = rect.center() + dir * s * 0.20;
                let outer = rect.center() + dir * s * if index % 2 == 0 { 0.44 } else { 0.32 };
                painter.line_segment([inner, outer], thin);
            }
        }
        "BLEEDING" => {
            // Teardrop: triangle tip over a circle.
            painter.add(egui::Shape::convex_polygon(
                vec![at(0.5, 0.10), at(0.30, 0.52), at(0.70, 0.52)],
                color,
                Stroke::NONE,
            ));
            painter.circle_filled(at(0.5, 0.62), s * 0.24, color);
        }
        "HIDDEN" => {
            // Eye with a slash through it.
            painter.add(egui::epaint::EllipseShape::stroke(
                rect.center(),
                Vec2::new(s * 0.40, s * 0.24),
                thin,
            ));
            painter.circle_filled(rect.center(), s * 0.11, color);
            painter.line_segment([at(0.14, 0.82), at(0.86, 0.18)], thin);
        }
        "INVISIBLE" => {
            // Dashed outline of a figure that isn't there.
            let center = at(0.5, 0.52);
            let radius = s * 0.34;
            let points: Vec<Pos2> = (0..=32)
                .map(|step| {
                    let angle = step as f32 / 32.0 * std::f32::consts::TAU;
                    center + Vec2::new(angle.cos(), angle.sin()) * radius
                })
                .collect();
            for shape in egui::Shape::dashed_line(&points, thin, s * 0.10, s * 0.08) {
                painter.add(shape);
            }
            painter.circle_filled(at(0.5, 0.52), s * 0.06, color);
        }
        "WEBBED" => {
            // Spiderweb: spokes plus two rings.
            let center = rect.center();
            for index in 0..6 {
                let angle = index as f32 * std::f32::consts::TAU / 6.0;
                let dir = Vec2::new(angle.cos(), angle.sin());
                painter.line_segment([center, center + dir * s * 0.44], thin);
            }
            painter.circle_stroke(center, s * 0.20, thin);
            painter.circle_stroke(center, s * 0.34, thin);
        }
        "POISONED" => {
            // Flask with bubbles.
            painter.rect_filled(
                Rect::from_min_max(at(0.44, 0.12), at(0.56, 0.32)),
                0.0,
                color,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    at(0.44, 0.32),
                    at(0.56, 0.32),
                    at(0.74, 0.72),
                    at(0.66, 0.88),
                    at(0.34, 0.88),
                    at(0.26, 0.72),
                ],
                color,
                Stroke::NONE,
            ));
            painter.circle_filled(at(0.44, 0.68), s * 0.045, bg);
            painter.circle_filled(at(0.56, 0.76), s * 0.045, bg);
        }
        "DISEASED" => {
            // Germ: spiked blob with vacuoles.
            let center = rect.center();
            painter.circle_filled(center, s * 0.26, color);
            for index in 0..8 {
                let angle = index as f32 * std::f32::consts::FRAC_PI_4 + 0.4;
                let dir = Vec2::new(angle.cos(), angle.sin());
                painter.line_segment(
                    [center + dir * s * 0.24, center + dir * s * 0.40],
                    thin,
                );
            }
            painter.circle_filled(at(0.44, 0.46), s * 0.05, bg);
            painter.circle_filled(at(0.58, 0.56), s * 0.04, bg);
        }
        "JOINED" => {
            // Two chain links.
            let link = Stroke::new((s * 0.08).max(1.0), color);
            painter.circle_stroke(at(0.36, 0.5), s * 0.18, link);
            painter.circle_stroke(at(0.64, 0.5), s * 0.18, link);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_matches_case_insensitively() {
        assert!(supported("KNEELING"));
        assert!(supported("kneeling"));
        assert!(supported("Hidden"));
        assert!(!supported("MYCUSTOM"));
        assert!(!supported(""));
    }

    #[test]
    fn display_name_title_cases() {
        assert_eq!(display_name("KNEELING"), "Kneeling");
        assert_eq!(display_name("hidden"), "Hidden");
        assert_eq!(display_name(""), "");
    }

    #[test]
    fn paint_covers_every_supported_id_headless() {
        // Run one headless egui frame and paint each icon; catches geometry
        // panics (bad polygons, zero-size strokes) without a GPU.
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(24.0));
                for id in SUPPORTED {
                    assert!(
                        paint(ui.painter(), rect, id, Color32::RED, Color32::BLACK),
                        "paint returned false for supported id {id}"
                    );
                }
                assert!(!paint(
                    ui.painter(),
                    rect,
                    "MYCUSTOM",
                    Color32::RED,
                    Color32::BLACK
                ));
            });
        });
    }
}
