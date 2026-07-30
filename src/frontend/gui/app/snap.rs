//! Snap-to-edge docking for freely placed Center-zone windows.
//!
//! The engine is pure geometry: given the pointer-true (unsnapped) rect of
//! the window being dragged, the gesture per axis (move vs. which edge is
//! resizing), and the candidate lines (pane bounds, sibling edges, center
//! lines, grid), it returns the snapped rect plus the guides to draw.
//!
//! The egui hook lives in `apply_center_snap`: egui applies drags as
//! per-frame deltas on top of the canonical rect we feed it, so a plain
//! "snap what egui reports" would make snaps inescapable (each frame's
//! delta restarts from the snapped position). `CenterSnapDrag` therefore
//! accumulates the pointer-true rect across the whole drag; snapping is a
//! pure function of it, and the pointer escapes a snap the moment the true
//! rect leaves the radius.

use super::*;

/// Effective snap tuning, resolved from `GuiUiSettings` per drag frame.
pub(super) struct SnapParams {
    pub radius: f32,
    pub to_siblings: bool,
    pub to_bounds: bool,
    pub to_centers: bool,
    /// Grid pitch in points; 0 = off.
    pub grid: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SnapGuideKind {
    /// Pane (center zone) edge.
    Bound,
    /// Sibling window near or far edge.
    Sibling,
    /// Pane or sibling center line.
    Center,
    Grid,
}

/// One engaged snap, drawn as a dashed line with the matched coordinate.
pub(super) struct SnapGuide {
    /// True: vertical line at x = `line`; false: horizontal at y = `line`.
    pub vertical: bool,
    /// Matched coordinate, in the same space as the window rects.
    pub line: f32,
    pub kind: SnapGuideKind,
    /// Which moving edge matched: "left"/"right"/"top"/"bottom"/"center".
    pub edge: &'static str,
    /// Sibling window title, when the target is a sibling.
    pub target: Option<String>,
}

/// How the current gesture moves the rect along one axis. Classified once
/// per axis from the first frame the axis actually moves (egui gestures are
/// one of title-drag or a single resize handle for the whole press).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AxisGesture {
    /// Axis has not moved yet this drag.
    Idle,
    /// Both edges move together (title-bar move).
    Translate,
    /// Left/top edge resize: min edge moves, max edge fixed.
    MinEdge,
    /// Right/bottom edge resize: max edge moves, min edge fixed.
    MaxEdge,
}

/// Live bookkeeping for the Center window currently being dragged/resized.
pub(super) struct CenterSnapDrag {
    pub tab_key: TabKey,
    /// Canonical rect at the moment the drag started.
    pub start: Rect,
    /// Pointer-true rect: where the window would be with snapping off.
    pub unsnapped: Rect,
    pub gesture_x: AxisGesture,
    pub gesture_y: AxisGesture,
}

/// Movement below this is treated as pixel-rounding noise, not a gesture:
/// egui rounds window rects to physical pixels, which at fractional DPI
/// scales reports sub-half-point jitter on rects nobody is moving.
const MOVED_EPS: f32 = 0.6;

/// Classify one axis of a gesture from its min/max edge deltas.
pub(super) fn classify_axis(dmin: f32, dmax: f32) -> AxisGesture {
    let min_moved = dmin.abs() > MOVED_EPS;
    let max_moved = dmax.abs() > MOVED_EPS;
    match (min_moved, max_moved) {
        (false, false) => AxisGesture::Idle,
        (true, false) => AxisGesture::MinEdge,
        (false, true) => AxisGesture::MaxEdge,
        // Both edges moving by the same amount is a move; unequal can only
        // be rounding noise on top of a move, so read it as one too.
        (true, true) => AxisGesture::Translate,
    }
}

struct AxisCandidate {
    value: f32,
    kind: SnapGuideKind,
    target: Option<usize>,
}

struct AxisSnap {
    delta: f32,
    line: f32,
    kind: SnapGuideKind,
    target: Option<usize>,
    edge: &'static str,
}

/// Best snap along one axis, or None when nothing is within radius.
/// `lo`/`hi` are the unsnapped interval; extent limits guard resize snaps
/// against violating the window's min/max size.
#[allow(clippy::too_many_arguments)]
fn snap_1d(
    gesture: AxisGesture,
    lo: f32,
    hi: f32,
    min_extent: f32,
    max_extent: f32,
    candidates: &[AxisCandidate],
    grid_origin: f32,
    params: &SnapParams,
    edge_names: [&'static str; 3],
) -> Option<AxisSnap> {
    let moving: &[(f32, &'static str)] = match gesture {
        AxisGesture::Idle => return None,
        AxisGesture::Translate => &[
            (lo, edge_names[0]),
            (hi, edge_names[1]),
            ((lo + hi) * 0.5, edge_names[2]),
        ],
        AxisGesture::MinEdge => &[(lo, edge_names[0])],
        AxisGesture::MaxEdge => &[(hi, edge_names[1])],
    };

    let extent_ok = |new_lo: f32, new_hi: f32| {
        let extent = new_hi - new_lo;
        extent >= min_extent - 0.01 && extent <= max_extent + 0.01
    };
    let mut best: Option<(f32, AxisSnap)> = None;
    let mut consider = |pos: f32, edge: &'static str, value: f32, kind, target| {
        let distance = (value - pos).abs();
        if distance > params.radius {
            return;
        }
        let delta = value - pos;
        let (new_lo, new_hi) = match gesture {
            AxisGesture::MinEdge => (lo + delta, hi),
            AxisGesture::MaxEdge => (lo, hi + delta),
            _ => (lo + delta, hi + delta),
        };
        if matches!(gesture, AxisGesture::MinEdge | AxisGesture::MaxEdge)
            && !extent_ok(new_lo, new_hi)
        {
            return;
        }
        if best.as_ref().is_none_or(|(d, _)| distance < *d) {
            best = Some((
                distance,
                AxisSnap { delta, line: value, kind, target, edge },
            ));
        }
    };

    for &(pos, edge) in moving {
        for candidate in candidates {
            consider(pos, edge, candidate.value, candidate.kind, candidate.target);
        }
        if params.grid > 0.0 {
            let grid_value =
                grid_origin + ((pos - grid_origin) / params.grid).round() * params.grid;
            consider(pos, edge, grid_value, SnapGuideKind::Grid, None);
        }
    }
    best.map(|(_, snap)| snap)
}

fn axis_candidates(
    bounds_lo: f32,
    bounds_hi: f32,
    siblings: impl Iterator<Item = (usize, f32, f32)>,
    params: &SnapParams,
) -> Vec<AxisCandidate> {
    let mut candidates = Vec::new();
    if params.to_bounds {
        candidates.push(AxisCandidate {
            value: bounds_lo,
            kind: SnapGuideKind::Bound,
            target: None,
        });
        candidates.push(AxisCandidate {
            value: bounds_hi,
            kind: SnapGuideKind::Bound,
            target: None,
        });
    }
    if params.to_centers {
        candidates.push(AxisCandidate {
            value: (bounds_lo + bounds_hi) * 0.5,
            kind: SnapGuideKind::Center,
            target: None,
        });
    }
    for (index, lo, hi) in siblings {
        if params.to_siblings {
            candidates.push(AxisCandidate {
                value: lo,
                kind: SnapGuideKind::Sibling,
                target: Some(index),
            });
            candidates.push(AxisCandidate {
                value: hi,
                kind: SnapGuideKind::Sibling,
                target: Some(index),
            });
        }
        if params.to_centers {
            candidates.push(AxisCandidate {
                value: (lo + hi) * 0.5,
                kind: SnapGuideKind::Center,
                target: Some(index),
            });
        }
    }
    candidates
}

/// Snap `unsnapped` against pane bounds, sibling rects, center lines, and
/// the grid. Axes are independent; each contributes at most one guide.
#[allow(clippy::too_many_arguments)]
pub(super) fn snap_rect(
    unsnapped: Rect,
    gesture_x: AxisGesture,
    gesture_y: AxisGesture,
    bounds: Rect,
    siblings: &[(String, Rect)],
    min_size: Vec2,
    max_size: Vec2,
    params: &SnapParams,
) -> (Rect, Vec<SnapGuide>) {
    let mut rect = unsnapped;
    let mut guides = Vec::new();

    let x_candidates = axis_candidates(
        bounds.min.x,
        bounds.max.x,
        siblings
            .iter()
            .enumerate()
            .map(|(index, (_, sibling))| (index, sibling.min.x, sibling.max.x)),
        params,
    );
    if let Some(snap) = snap_1d(
        gesture_x,
        unsnapped.min.x,
        unsnapped.max.x,
        min_size.x,
        max_size.x,
        &x_candidates,
        bounds.min.x,
        params,
        ["left", "right", "center"],
    ) {
        match gesture_x {
            AxisGesture::MinEdge => rect.min.x += snap.delta,
            AxisGesture::MaxEdge => rect.max.x += snap.delta,
            _ => {
                rect.min.x += snap.delta;
                rect.max.x += snap.delta;
            }
        }
        guides.push(SnapGuide {
            vertical: true,
            line: snap.line,
            kind: snap.kind,
            edge: snap.edge,
            target: snap.target.map(|index| siblings[index].0.clone()),
        });
    }

    let y_candidates = axis_candidates(
        bounds.min.y,
        bounds.max.y,
        siblings
            .iter()
            .enumerate()
            .map(|(index, (_, sibling))| (index, sibling.min.y, sibling.max.y)),
        params,
    );
    if let Some(snap) = snap_1d(
        gesture_y,
        unsnapped.min.y,
        unsnapped.max.y,
        min_size.y,
        max_size.y,
        &y_candidates,
        bounds.min.y,
        params,
        ["top", "bottom", "center"],
    ) {
        match gesture_y {
            AxisGesture::MinEdge => rect.min.y += snap.delta,
            AxisGesture::MaxEdge => rect.max.y += snap.delta,
            _ => {
                rect.min.y += snap.delta;
                rect.max.y += snap.delta;
            }
        }
        guides.push(SnapGuide {
            vertical: false,
            line: snap.line,
            kind: snap.kind,
            edge: snap.edge,
            target: snap.target.map(|index| siblings[index].0.clone()),
        });
    }

    (rect, guides)
}

impl VellumGuiApp {
    /// Snap hook for the Center-zone drag/resize tracking path. `fed` is the
    /// rect this frame's window builder was given (canonical/display),
    /// `reported` is egui's post-gesture rect. Returns the rect to write
    /// into the canonical map. Maintains the per-drag pointer-true rect and
    /// this frame's guides as a side effect.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_center_snap(
        &mut self,
        tab_key: &TabKey,
        fed: Rect,
        reported: Rect,
        siblings: &[(TabKey, String, Rect)],
        bounds: Rect,
        min_size: Vec2,
        max_size: Vec2,
        suspended: bool,
    ) -> Rect {
        let settings = &self.ui_settings;
        let radius = settings.snap_radius.clamp(0.0, 64.0);
        if !settings.snap_enabled || radius <= 0.0 {
            self.center_snap_drag = None;
            return reported;
        }

        if self
            .center_snap_drag
            .as_ref()
            .is_none_or(|drag| drag.tab_key != *tab_key)
        {
            self.center_snap_drag = Some(CenterSnapDrag {
                tab_key: tab_key.clone(),
                start: fed,
                unsnapped: fed,
                gesture_x: AxisGesture::Idle,
                gesture_y: AxisGesture::Idle,
            });
        }
        let mut drag = self.center_snap_drag.take().expect("just ensured");

        // Per-axis classification happens on the first frame that axis
        // moves (not just the first frame of the drag): a diagonal
        // title-drag often starts along one axis, and freezing the other
        // axis at Idle would pin the window to it for the whole drag.
        let dmin = reported.min - fed.min;
        let dmax = reported.max - fed.max;
        if drag.gesture_x == AxisGesture::Idle {
            drag.gesture_x = classify_axis(dmin.x, dmax.x);
        }
        if drag.gesture_y == AxisGesture::Idle {
            drag.gesture_y = classify_axis(dmin.y, dmax.y);
        }

        // Advance the pointer-true rect. Translation arrives as per-frame
        // deltas (position is re-fed from the canonical map every frame, so
        // egui's delta is relative to the snapped position); sizes are NOT
        // re-fed while the user is engaging the window, so egui's reported
        // extent already IS the pointer-true extent.
        match drag.gesture_x {
            AxisGesture::Idle => {}
            AxisGesture::Translate => {
                let width = drag.unsnapped.width();
                drag.unsnapped.min.x += dmin.x;
                drag.unsnapped.max.x = drag.unsnapped.min.x + width;
            }
            AxisGesture::MaxEdge => {
                drag.unsnapped.min.x = drag.start.min.x;
                drag.unsnapped.max.x = drag.start.min.x + reported.width();
            }
            AxisGesture::MinEdge => {
                drag.unsnapped.min.x = drag.start.max.x - reported.width();
                drag.unsnapped.max.x = drag.start.max.x;
            }
        }
        match drag.gesture_y {
            AxisGesture::Idle => {}
            AxisGesture::Translate => {
                let height = drag.unsnapped.height();
                drag.unsnapped.min.y += dmin.y;
                drag.unsnapped.max.y = drag.unsnapped.min.y + height;
            }
            AxisGesture::MaxEdge => {
                drag.unsnapped.min.y = drag.start.min.y;
                drag.unsnapped.max.y = drag.start.min.y + reported.height();
            }
            AxisGesture::MinEdge => {
                drag.unsnapped.min.y = drag.start.max.y - reported.height();
                drag.unsnapped.max.y = drag.start.max.y;
            }
        }

        let (snapped, guides) = if suspended {
            (drag.unsnapped, Vec::new())
        } else {
            let params = SnapParams {
                radius,
                to_siblings: settings.snap_to_siblings,
                to_bounds: settings.snap_to_bounds,
                to_centers: settings.snap_to_centers,
                grid: settings.snap_grid.max(0.0),
            };
            let sibling_rects: Vec<(String, Rect)> = siblings
                .iter()
                .filter(|(key, _, _)| key != tab_key)
                .map(|(_, name, rect)| (name.clone(), *rect))
                .collect();
            snap_rect(
                drag.unsnapped,
                drag.gesture_x,
                drag.gesture_y,
                bounds,
                &sibling_rects,
                min_size,
                max_size,
                &params,
            )
        };
        self.center_snap_guides = guides;
        self.center_snap_drag = Some(drag);
        snapped
    }

    /// Draw the engaged snap guides over the center pane: a dashed line
    /// along the matched coordinate with a small label beside it.
    pub(super) fn paint_snap_guides(&self, ctx: &egui::Context, bounds: Rect) {
        if self.center_snap_guides.is_empty() {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("gui_snap_guides"),
        ));
        let style = ctx.global_style();
        let visuals = &style.visuals;
        let label_bg = visuals.window_fill;
        for guide in &self.center_snap_guides {
            let color = match guide.kind {
                SnapGuideKind::Center => Color32::from_rgb(204, 136, 68),
                SnapGuideKind::Grid => visuals.weak_text_color(),
                _ => visuals.selection.stroke.color,
            };
            let (from, to) = if guide.vertical {
                (
                    Pos2::new(guide.line, bounds.min.y),
                    Pos2::new(guide.line, bounds.max.y),
                )
            } else {
                (
                    Pos2::new(bounds.min.x, guide.line),
                    Pos2::new(bounds.max.x, guide.line),
                )
            };
            painter.extend(egui::Shape::dashed_line(
                &[from, to],
                egui::Stroke::new(1.0, color),
                4.0,
                4.0,
            ));

            let target = match (&guide.target, guide.kind) {
                (Some(name), _) => format!("  {name}"),
                (None, SnapGuideKind::Bound) => "  pane".to_string(),
                (None, SnapGuideKind::Grid) => "  grid".to_string(),
                _ => String::new(),
            };
            let text = format!("{} {:.0}{}", guide.edge, guide.line, target);
            let galley = painter.layout_no_wrap(text, egui::FontId::monospace(10.0), color);
            let label_pos = if guide.vertical {
                Pos2::new(
                    (guide.line + 6.0).min(bounds.max.x - galley.size().x - 6.0),
                    bounds.min.y + 6.0,
                )
            } else {
                Pos2::new(
                    bounds.min.x + 6.0,
                    (guide.line + 6.0).min(bounds.max.y - galley.size().y - 6.0),
                )
            };
            let label_rect = Rect::from_min_size(label_pos, galley.size());
            painter.rect_filled(label_rect.expand(3.0), 2.0, label_bg);
            painter.rect_stroke(
                label_rect.expand(3.0),
                2.0,
                egui::Stroke::new(1.0, color),
                egui::StrokeKind::Outside,
            );
            painter.galley(label_pos, galley, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(radius: f32) -> SnapParams {
        SnapParams {
            radius,
            to_siblings: true,
            to_bounds: true,
            to_centers: true,
            grid: 0.0,
        }
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    }

    const MIN: Vec2 = Vec2::new(120.0, 90.0);
    const MAX: Vec2 = Vec2::new(10_000.0, 10_000.0);
    const BOUNDS: Rect = Rect {
        min: Pos2::new(0.0, 0.0),
        max: Pos2::new(1000.0, 800.0),
    };

    #[test]
    fn classify_axis_gestures() {
        assert_eq!(classify_axis(0.0, 0.0), AxisGesture::Idle);
        assert_eq!(classify_axis(0.3, -0.3), AxisGesture::Idle);
        assert_eq!(classify_axis(5.0, 5.0), AxisGesture::Translate);
        assert_eq!(classify_axis(-4.0, 0.0), AxisGesture::MinEdge);
        assert_eq!(classify_axis(0.0, 7.0), AxisGesture::MaxEdge);
    }

    #[test]
    fn move_butts_left_edge_against_sibling_right_edge() {
        let siblings = vec![("room".to_string(), rect(100.0, 100.0, 305.5, 300.0))];
        let unsnapped = rect(310.0, 500.0, 470.0, 600.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::Translate,
            AxisGesture::Translate,
            BOUNDS,
            &siblings,
            MIN,
            MAX,
            &params(8.0),
        );
        // Left edge lands exactly on the sibling's right edge; width kept.
        assert_eq!(snapped.min.x, 305.5);
        assert_eq!(snapped.width(), unsnapped.width());
        let x_guide = guides.iter().find(|guide| guide.vertical).unwrap();
        assert_eq!(x_guide.line, 305.5);
        assert_eq!(x_guide.edge, "left");
        assert_eq!(x_guide.target.as_deref(), Some("room"));
    }

    #[test]
    fn move_aligns_top_edges_flush() {
        // The far-edge case the feature exists for: tops 3px apart snap to
        // the same value.
        let siblings = vec![("targets".to_string(), rect(600.0, 197.0, 800.0, 400.0))];
        let unsnapped = rect(100.0, 200.0, 300.0, 350.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::Translate,
            AxisGesture::Translate,
            BOUNDS,
            &siblings,
            MIN,
            MAX,
            &params(8.0),
        );
        assert_eq!(snapped.min.y, 197.0);
        assert!(guides.iter().any(|guide| !guide.vertical && guide.line == 197.0));
    }

    #[test]
    fn resize_right_edge_snaps_to_pane_bound() {
        let unsnapped = rect(100.0, 100.0, 994.0, 300.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::MaxEdge,
            AxisGesture::Idle,
            BOUNDS,
            &[],
            MIN,
            MAX,
            &params(8.0),
        );
        // Right edge is written as exactly the bound; left edge untouched.
        assert_eq!(snapped.max.x, 1000.0);
        assert_eq!(snapped.min.x, 100.0);
        assert_eq!(guides.len(), 1);
        assert_eq!(guides[0].kind, SnapGuideKind::Bound);
    }

    #[test]
    fn resize_snap_rejected_when_below_min_size() {
        // Sibling edge 4px away, but snapping would shrink below 120 wide.
        let siblings = vec![("a".to_string(), rect(0.0, 0.0, 385.0, 300.0))];
        let unsnapped = rect(266.0, 400.0, 389.0, 500.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::MaxEdge,
            AxisGesture::Idle,
            BOUNDS,
            &siblings,
            MIN,
            MAX,
            &params(8.0),
        );
        assert_eq!(snapped, unsnapped);
        assert!(guides.is_empty());
    }

    #[test]
    fn nothing_snaps_outside_radius() {
        let unsnapped = rect(100.0, 100.0, 300.0, 300.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::Translate,
            AxisGesture::Translate,
            BOUNDS,
            &[],
            MIN,
            MAX,
            &params(8.0),
        );
        assert_eq!(snapped, unsnapped);
        assert!(guides.is_empty());
    }

    #[test]
    fn center_lines_snap_and_are_marked_center() {
        // Window center-x at 497 with pane center at 500.
        let unsnapped = rect(397.0, 100.0, 597.0, 300.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::Translate,
            AxisGesture::Idle,
            BOUNDS,
            &[],
            MIN,
            MAX,
            &params(8.0),
        );
        assert_eq!(snapped.center().x, 500.0);
        assert_eq!(guides[0].kind, SnapGuideKind::Center);
        assert_eq!(guides[0].edge, "center");
    }

    #[test]
    fn centers_toggle_off_disables_center_lines() {
        let unsnapped = rect(397.0, 100.0, 597.0, 300.0);
        let mut p = params(8.0);
        p.to_centers = false;
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::Translate,
            AxisGesture::Idle,
            BOUNDS,
            &[],
            MIN,
            MAX,
            &p,
        );
        assert_eq!(snapped, unsnapped);
        assert!(guides.is_empty());
    }

    #[test]
    fn grid_snaps_relative_to_pane_origin() {
        let bounds = rect(50.0, 40.0, 1050.0, 840.0);
        let mut p = params(8.0);
        p.grid = 32.0;
        p.to_bounds = false;
        p.to_centers = false;
        // Left edge at 145 → nearest pane-relative grid line is 50 + 96 = 146.
        let unsnapped = rect(145.0, 400.0, 345.0, 500.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::Translate,
            AxisGesture::Idle,
            bounds,
            &[],
            MIN,
            MAX,
            &p,
        );
        assert_eq!(snapped.min.x, 146.0);
        assert_eq!(guides[0].kind, SnapGuideKind::Grid);
    }

    #[test]
    fn closest_candidate_wins() {
        // Sibling edge at 303 and pane center at 500 are both irrelevant;
        // between sibling edges at 303 and 306, the left edge (at 305)
        // takes 306.
        let siblings = vec![
            ("far".to_string(), rect(100.0, 0.0, 303.0, 50.0)),
            ("near".to_string(), rect(306.0, 0.0, 500.0, 50.0)),
        ];
        let unsnapped = rect(305.0, 400.0, 455.0, 500.0);
        let (snapped, _) = snap_rect(
            unsnapped,
            AxisGesture::Translate,
            AxisGesture::Idle,
            BOUNDS,
            &siblings,
            MIN,
            MAX,
            &params(8.0),
        );
        assert_eq!(snapped.min.x, 306.0);
    }
}
