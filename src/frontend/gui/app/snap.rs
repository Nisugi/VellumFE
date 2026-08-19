//! Snap docking for freely placed zone windows (v2).
//!
//! Born Center-only; the header and footer zones share the same render
//! loop and joined via zone-free-movement-plan.md P1 (sidebars follow in
//! P2). The drag state carries its zone so guides and the grid overlay
//! paint into the pane the gesture lives in; sibling candidates come from
//! that zone only.
//!
//! Design (see .claude/work-units/snap-docking-v2-plan.md): while the user
//! is engaging a zone window, the shell does NOT re-feed its position —
//! egui owns the whole gesture, so every handle and move behaves natively
//! and the reported rect is pointer-true by construction. The engine is a
//! pure function of that rect: classify the gesture per axis from totals
//! since gesture start, snap the moving edges against pane bounds, sibling
//! edges, the optional pane center, and the grid, and write the snapped
//! rect into the canonical map every frame. egui ignores those writes
//! mid-gesture; the moment the gesture ends the position feed resumes and
//! the window glues onto the snapped rect (the drag state survives the
//! release frame so the final write is the snapped drop position).
//!
//! Guides: a steady faint full-pane grid while a gesture is live (when a
//! grid is set), plus one accent line per engaged snap with its coordinate.
//! Shift suspends snapping and hides the accent lines; the grid overlay
//! stays up as context.

use super::window_manager::{AxisSide, EdgeRef};
use super::*;

/// Effective snap tuning, resolved from `GuiUiSettings` per drag frame.
pub(super) struct SnapParams {
    pub radius: f32,
    pub to_siblings: bool,
    pub to_bounds: bool,
    /// Pane center lines only — sibling centers are deliberately not
    /// candidates: they sit a few px from real edge targets and make the
    /// engaged line flip while dragging (beta.21 field report).
    pub to_centers: bool,
    /// Grid pitch in points; 0 = off.
    pub grid: f32,
    /// Move gestures also pull each edge to its nearest grid line (the
    /// window resizes to conform to the grid).
    pub move_sizes_to_grid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SnapGuideKind {
    /// Pane (center zone) edge.
    Bound,
    /// Sibling window near or far edge.
    Sibling,
    /// Pane center line.
    Center,
    Grid,
}

/// Exact ties go to the more meaningful target: butting the pane edge
/// beats a sibling that happens to sit flush on it, which beats a grid
/// line running through both.
fn kind_priority(kind: SnapGuideKind) -> u8 {
    match kind {
        SnapGuideKind::Bound => 0,
        SnapGuideKind::Sibling => 1,
        SnapGuideKind::Center => 2,
        SnapGuideKind::Grid => 3,
    }
}

/// One engaged snap, drawn as an accent line with the matched coordinate.
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
    /// Anchor target this snap promotes to on release: pane edges, the
    /// pane center, and sibling edges. None = not promotable — grid lines
    /// have no referent that later re-solves.
    pub promote: Option<EdgeRef>,
}

/// How the gesture moves the rect along one axis, classified fresh every
/// frame from the totals since gesture start. Totals are unambiguous: a
/// move shifts both edges equally, a resize pins the untouched edge
/// exactly (egui owns the rect mid-gesture; nothing fights it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AxisGesture {
    /// Axis has not moved (yet) this gesture.
    Idle,
    /// Both edges move together (title-bar move).
    Translate,
    /// Left/top edge resize: min edge moves, max edge fixed.
    MinEdge,
    /// Right/bottom edge resize: max edge moves, min edge fixed.
    MaxEdge,
}

/// Live bookkeeping for the zone window currently being dragged/resized.
/// One pointer means at most one live gesture, so a single slot serves
/// every zone; `zone` scopes the overlays to the pane that owns it.
pub(super) struct ZoneSnapDrag {
    pub tab_key: TabKey,
    pub zone: GuiShellZone,
    /// Canonical rect at the moment the gesture started; the baseline the
    /// per-frame gesture classification measures totals against.
    pub start: Rect,
}

/// Total movement below this is pixel-rounding noise, not a gesture.
const MOVED_EPS: f32 = 0.6;

/// Classify one axis from its min/max edge TOTAL deltas since gesture
/// start.
pub(super) fn classify_axis(dmin: f32, dmax: f32) -> AxisGesture {
    let min_moved = dmin.abs() > MOVED_EPS;
    let max_moved = dmax.abs() > MOVED_EPS;
    match (min_moved, max_moved) {
        (false, false) => AxisGesture::Idle,
        (true, false) => AxisGesture::MinEdge,
        (false, true) => AxisGesture::MaxEdge,
        (true, true) => AxisGesture::Translate,
    }
}

struct AxisCandidate {
    value: f32,
    kind: SnapGuideKind,
    target: Option<usize>,
    promote: Option<EdgeRef>,
}

struct AxisSnap {
    delta: f32,
    line: f32,
    kind: SnapGuideKind,
    target: Option<usize>,
    edge: &'static str,
    promote: Option<EdgeRef>,
}

/// Best snap along one axis, or None when nothing is within radius.
/// Candidates whose application would violate the extent limits are
/// skipped BEFORE the closest-wins choice, so an illegal near candidate
/// never shadows a legal one.
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
    // Each moving position carries whether it is the window's CENTER:
    // centers pair only with center-line candidates and edges only with
    // edge/bound/grid candidates. Cross-pairing made move-snap "mostly not
    // work" against a grid — the window's center is nearly always closer
    // to some grid line than either edge is (any width not a multiple of
    // the pitch), so the center kept winning and the window landed with
    // both edges off-grid.
    let moving: &[(f32, &'static str, bool)] = match gesture {
        AxisGesture::Idle => return None,
        AxisGesture::Translate => &[
            (lo, edge_names[0], false),
            (hi, edge_names[1], false),
            ((lo + hi) * 0.5, edge_names[2], true),
        ],
        AxisGesture::MinEdge => &[(lo, edge_names[0], false)],
        AxisGesture::MaxEdge => &[(hi, edge_names[1], false)],
    };

    let extent_ok = |new_lo: f32, new_hi: f32| {
        let extent = new_hi - new_lo;
        extent >= min_extent - 0.01 && extent <= max_extent + 0.01
    };
    let mut best: Option<(f32, AxisSnap)> = None;
    let mut consider =
        |pos: f32, edge: &'static str, value: f32, kind, target, promote: Option<&EdgeRef>| {
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
            let wins = match &best {
                None => true,
                Some((best_distance, best_snap)) => {
                    distance + 0.001 < *best_distance
                        || ((distance - *best_distance).abs() <= 0.001
                            && kind_priority(kind) < kind_priority(best_snap.kind))
                }
            };
            if wins {
                best = Some((
                    distance,
                    AxisSnap {
                        delta,
                        line: value,
                        kind,
                        target,
                        edge,
                        promote: promote.cloned(),
                    },
                ));
            }
        };

    for &(pos, edge, is_center) in moving {
        for candidate in candidates {
            if (candidate.kind == SnapGuideKind::Center) != is_center {
                continue;
            }
            consider(
                pos,
                edge,
                candidate.value,
                candidate.kind,
                candidate.target,
                candidate.promote.as_ref(),
            );
        }
        if params.grid > 0.0 && !is_center {
            let grid_value =
                grid_origin + ((pos - grid_origin) / params.grid).round() * params.grid;
            consider(pos, edge, grid_value, SnapGuideKind::Grid, None, None);
        }
    }
    best.map(|(_, snap)| snap)
}

fn axis_candidates<'a>(
    bounds_lo: f32,
    bounds_hi: f32,
    siblings: impl Iterator<Item = (usize, &'a TabKey, f32, f32)>,
    params: &SnapParams,
) -> Vec<AxisCandidate> {
    let mut candidates = Vec::new();
    if params.to_bounds {
        candidates.push(AxisCandidate {
            value: bounds_lo,
            kind: SnapGuideKind::Bound,
            target: None,
            promote: Some(EdgeRef::Pane(AxisSide::Min)),
        });
        candidates.push(AxisCandidate {
            value: bounds_hi,
            kind: SnapGuideKind::Bound,
            target: None,
            promote: Some(EdgeRef::Pane(AxisSide::Max)),
        });
    }
    if params.to_centers {
        candidates.push(AxisCandidate {
            value: (bounds_lo + bounds_hi) * 0.5,
            kind: SnapGuideKind::Center,
            target: None,
            promote: Some(EdgeRef::PaneCenter),
        });
    }
    if params.to_siblings {
        // P-A2: sibling snaps promote to sibling anchors — "my edge =
        // that window's edge". The candidate at the sibling's min edge
        // anchors to Side::Min, its max edge to Side::Max.
        for (index, key, lo, hi) in siblings {
            candidates.push(AxisCandidate {
                value: lo,
                kind: SnapGuideKind::Sibling,
                target: Some(index),
                promote: Some(EdgeRef::Sibling {
                    key: key.clone(),
                    side: AxisSide::Min,
                }),
            });
            candidates.push(AxisCandidate {
                value: hi,
                kind: SnapGuideKind::Sibling,
                target: Some(index),
                promote: Some(EdgeRef::Sibling {
                    key: key.clone(),
                    side: AxisSide::Max,
                }),
            });
        }
    }
    candidates
}

/// Snap the pointer-true rect against pane bounds, sibling edges, the
/// pane center, and the grid. Axes are independent. With
/// `move_sizes_to_grid`, a move gesture additionally pulls each edge to
/// its nearest grid line (extent-guarded), so the window conforms to the
/// grid instead of only repositioning.
#[allow(clippy::too_many_arguments)]
pub(super) fn snap_rect(
    unsnapped: Rect,
    gesture_x: AxisGesture,
    gesture_y: AxisGesture,
    bounds: Rect,
    siblings: &[(TabKey, String, Rect)],
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
            .map(|(index, (key, _, sibling))| (index, key, sibling.min.x, sibling.max.x)),
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
            target: snap.target.map(|index| siblings[index].1.clone()),
            promote: snap.promote,
        });
    }

    let y_candidates = axis_candidates(
        bounds.min.y,
        bounds.max.y,
        siblings
            .iter()
            .enumerate()
            .map(|(index, (key, _, sibling))| (index, key, sibling.min.y, sibling.max.y)),
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
            target: snap.target.map(|index| siblings[index].1.clone()),
            promote: snap.promote,
        });
    }

    // Grid conformance on moves: after the position snap, pull each edge
    // independently to its nearest grid line within radius. The min edge
    // goes first; the max edge then re-checks the extent so a tiny window
    // over a coarse grid never collapses onto a single line.
    if params.move_sizes_to_grid && params.grid > 0.0 {
        let mut conform = |gesture: AxisGesture,
                           lo: &mut f32,
                           hi: &mut f32,
                           origin: f32,
                           min_extent: f32,
                           max_extent: f32,
                           vertical: bool,
                           names: [&'static str; 2],
                           guides: &mut Vec<SnapGuide>| {
            if gesture != AxisGesture::Translate {
                return;
            }
            let line_near =
                |pos: f32| origin + ((pos - origin) / params.grid).round() * params.grid;
            let lo_line = line_near(*lo);
            let lo_delta = lo_line - *lo;
            if lo_delta.abs() <= params.radius && lo_delta.abs() > 0.01 {
                let extent = *hi - lo_line;
                if extent >= min_extent - 0.01 && extent <= max_extent + 0.01 {
                    *lo = lo_line;
                    guides.push(SnapGuide {
                        vertical,
                        line: lo_line,
                        kind: SnapGuideKind::Grid,
                        edge: names[0],
                        target: None,
                        promote: None,
                    });
                }
            }
            let hi_line = line_near(*hi);
            let hi_delta = hi_line - *hi;
            if hi_delta.abs() <= params.radius && hi_delta.abs() > 0.01 {
                let extent = hi_line - *lo;
                if extent >= min_extent - 0.01 && extent <= max_extent + 0.01 {
                    *hi = hi_line;
                    guides.push(SnapGuide {
                        vertical,
                        line: hi_line,
                        kind: SnapGuideKind::Grid,
                        edge: names[1],
                        target: None,
                        promote: None,
                    });
                }
            }
        };
        let (mut lo, mut hi) = (rect.min.x, rect.max.x);
        conform(
            gesture_x,
            &mut lo,
            &mut hi,
            bounds.min.x,
            min_size.x,
            max_size.x,
            true,
            ["left", "right"],
            &mut guides,
        );
        (rect.min.x, rect.max.x) = (lo, hi);
        let (mut lo, mut hi) = (rect.min.y, rect.max.y);
        conform(
            gesture_y,
            &mut lo,
            &mut hi,
            bounds.min.y,
            min_size.y,
            max_size.y,
            false,
            ["top", "bottom"],
            &mut guides,
        );
        (rect.min.y, rect.max.y) = (lo, hi);
    }

    (rect, guides)
}

impl VellumGuiApp {
    /// Snap hook for the zone drag/resize tracking path. `fed` is the
    /// canonical/display rect (what a non-engaged frame would feed),
    /// `reported` is egui's rect — pointer-true for the whole gesture
    /// because the position feed is suspended while the user engages the
    /// window. `bounds` is the engaged zone's pane rect and `siblings`
    /// its windows. Returns the rect to write into the canonical map and
    /// maintains this frame's guides.
    ///
    /// On the release frame (`pointer_down` false with a live drag) the
    /// snapped rect is written one final time as the drop position and the
    /// drag state ends; guides end with it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_zone_snap(
        &mut self,
        zone: GuiShellZone,
        tab_key: &TabKey,
        fed: Rect,
        reported: Rect,
        siblings: &[(TabKey, String, Rect)],
        bounds: Rect,
        min_size: Vec2,
        max_size: Vec2,
        suspended: bool,
        pointer_down: bool,
    ) -> Rect {
        let settings = &self.ui_settings;
        let radius = settings.snap_radius.clamp(0.0, 64.0);
        if !settings.snap_enabled || radius <= 0.0 {
            self.zone_snap_drag = None;
            // With snapping off there is no way to form or re-form an
            // anchor, so any tracked gesture is free placement: forget the
            // window's anchors or the next frame's solve would yank it
            // back onto them. (The gesture's own tracking has already
            // committed the on-screen rect — commit-on-detach holds.)
            if self.window_anchors.remove(tab_key).is_some() {
                self.layout_dirty = true;
            }
            return reported;
        }

        let fresh = self
            .zone_snap_drag
            .as_ref()
            .is_none_or(|drag| drag.tab_key != *tab_key);
        if fresh {
            if !pointer_down {
                // A stray release-frame rect change with no drag to finish.
                return reported;
            }
            self.zone_snap_drag = Some(ZoneSnapDrag {
                tab_key: tab_key.clone(),
                zone,
                start: fed,
            });
        }
        let start = self
            .zone_snap_drag
            .as_ref()
            .map(|drag| drag.start)
            .expect("just ensured");

        let gesture_x = classify_axis(reported.min.x - start.min.x, reported.max.x - start.max.x);
        let gesture_y = classify_axis(reported.min.y - start.min.y, reported.max.y - start.max.y);

        let (snapped, guides) = if suspended {
            (reported, Vec::new())
        } else {
            let params = SnapParams {
                radius,
                to_siblings: settings.snap_to_siblings,
                to_bounds: settings.snap_to_bounds,
                to_centers: settings.snap_to_centers,
                grid: settings.snap_grid.max(0.0),
                move_sizes_to_grid: settings.snap_move_sizes_to_grid,
            };
            let sibling_rects: Vec<(TabKey, String, Rect)> = siblings
                .iter()
                .filter(|(key, _, _)| key != tab_key)
                .cloned()
                .collect();
            snap_rect(
                reported,
                gesture_x,
                gesture_y,
                bounds,
                &sibling_rects,
                min_size,
                max_size,
                &params,
            )
        };
        if self.snap_debug {
            let guide_list: Vec<String> = guides
                .iter()
                .map(|guide| format!("{} {:.1} {:?}", guide.edge, guide.line, guide.kind))
                .collect();
            tracing::info!(
                "snapdbg drag {:?} zone={:?} down={} susp={} gx={:?} gy={:?} start=[{:.1} {:.1} {:.1} {:.1}] fed=[{:.1} {:.1} {:.1} {:.1}] reported=[{:.1} {:.1} {:.1} {:.1}] snapped=[{:.1} {:.1} {:.1} {:.1}] guides=[{}]",
                tab_key,
                zone,
                pointer_down,
                suspended,
                gesture_x,
                gesture_y,
                start.min.x, start.min.y, start.max.x, start.max.y,
                fed.min.x, fed.min.y, fed.max.x, fed.max.y,
                reported.min.x, reported.min.y, reported.max.x, reported.max.y,
                snapped.min.x, snapped.min.y, snapped.max.x, snapped.max.y,
                guide_list.join(", "),
            );
        }
        if pointer_down {
            self.zone_snap_guides = guides;
        } else {
            // Release frame: promote engaged snaps into persisted anchors
            // (P-A1, WYSIWYG — the guides the user saw at drop are the
            // anchors they get; a Shift-suspended release has no guides
            // and clears the touched axes instead).
            self.promote_release_anchors(tab_key, gesture_x, gesture_y, &guides);
            self.zone_snap_drag = None;
        }
        snapped
    }

    /// Draw the snap overlays for one zone's pane: the steady faint grid
    /// while a gesture is live (Niffy's spec — context, never flashing),
    /// then the accent guide lines for engaged snaps with their
    /// coordinates. Shift-suspend empties the accent guides but the grid
    /// overlay deliberately stays. Zone-scoped: only the pane that owns
    /// the live gesture draws anything — guides are cleared and rebuilt
    /// by the owning zone's pass each frame, and the grid check here
    /// keeps a header drag from carpeting the center in grid lines.
    pub(super) fn paint_snap_overlays(
        &self,
        ctx: &egui::Context,
        zone: GuiShellZone,
        bounds: Rect,
    ) {
        let settings = &self.ui_settings;
        let gesture_live = self
            .zone_snap_drag
            .as_ref()
            .is_some_and(|drag| drag.zone == zone);
        let draw_grid = gesture_live && settings.snap_enabled && settings.snap_grid > 0.0;
        if !draw_grid && (!gesture_live || self.zone_snap_guides.is_empty()) {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("gui_snap_guides"),
        ));
        let style = ctx.global_style();
        let visuals = &style.visuals;

        if draw_grid {
            let pitch = settings.snap_grid.max(4.0);
            let color = visuals.weak_text_color().gamma_multiply(0.3);
            let stroke = egui::Stroke::new(1.0, color);
            let mut x = bounds.min.x + pitch;
            while x < bounds.max.x - 0.5 {
                painter.vline(x, bounds.min.y..=bounds.max.y, stroke);
                x += pitch;
            }
            let mut y = bounds.min.y + pitch;
            while y < bounds.max.y - 0.5 {
                painter.hline(bounds.min.x..=bounds.max.x, y, stroke);
                y += pitch;
            }
        }

        let label_bg = visuals.window_fill;
        for guide in &self.zone_snap_guides {
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
            move_sizes_to_grid: false,
        }
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    }

    fn sib(name: &str, r: Rect) -> (TabKey, String, Rect) {
        (
            TabKey::TextByName {
                id: name.to_string(),
            },
            name.to_string(),
            r,
        )
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
        let siblings = vec![sib("room", rect(100.0, 100.0, 305.5, 300.0))];
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
        let siblings = vec![sib("targets", rect(600.0, 197.0, 800.0, 400.0))];
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
        assert!(guides
            .iter()
            .any(|guide| !guide.vertical && guide.line == 197.0));
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
    fn illegal_near_candidate_does_not_shadow_a_legal_one() {
        // Sibling edge 3px away would shrink the window below min width;
        // the pane bound 6px away is legal and must win instead.
        let siblings = vec![sib("a", rect(0.0, 0.0, 385.0, 300.0))];
        let bounds = rect(0.0, 0.0, 394.0, 800.0);
        let unsnapped = rect(266.0, 400.0, 388.0, 500.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::MaxEdge,
            AxisGesture::Idle,
            bounds,
            &siblings,
            MIN,
            MAX,
            &params(8.0),
        );
        assert_eq!(snapped.max.x, 394.0);
        assert_eq!(guides[0].kind, SnapGuideKind::Bound);
    }

    #[test]
    fn resize_snap_rejected_when_below_min_size() {
        let siblings = vec![sib("a", rect(0.0, 0.0, 385.0, 300.0))];
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
    fn phantom_untouched_edge_delta_promotes_to_translate() {
        // Why the compact-widget fix matters: `classify_axis` reads the
        // gesture from min/max TOTAL deltas since start. If the fed/start
        // rect disagrees with egui's reported rect on the untouched edge
        // (compact widgets rendered capped/pinned while the canonical rect
        // stayed taller/wider), that stale edge shows a nonzero total and a
        // pure single-edge resize misreads as a Translate — which snap-tests
        // the far edge and the center too, drawing guides for edges the user
        // never dragged. Keeping start == reported (the zones.rs fix) holds
        // the untouched delta at 0, so it stays MaxEdge.
        assert_eq!(classify_axis(0.0, 6.0), AxisGesture::MaxEdge);
        // A phantom delta on the left edge, larger than MOVED_EPS, is what a
        // desynced fed rect injects — and it corrupts the classification.
        assert_eq!(classify_axis(4.0, 6.0), AxisGesture::Translate);
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
    fn pane_center_snaps_when_enabled() {
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
    fn sibling_centers_are_not_candidates() {
        // A sibling whose center-x is 2px from the moving window's left
        // edge: v1 snapped to it (and flapped between it and real edges);
        // v2 has no sibling-center candidates at all.
        let siblings = vec![sib("a", rect(200.0, 700.0, 424.0, 780.0))];
        let unsnapped = rect(310.0, 100.0, 460.0, 200.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::Translate,
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
    fn centers_toggle_off_disables_pane_center() {
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
    fn window_center_never_snaps_to_grid_lines() {
        // Field report (v2 first live test): move-snap "flaky and mostly
        // not working" against a grid while resize-snap worked. A move
        // tested the window's CENTER against grid lines too, and for any
        // width that isn't a multiple of the pitch the center is usually
        // the closest match — the window landed with its center on a grid
        // line and both edges off-grid. Centers pair only with center-line
        // candidates now; the grid is edges-only.
        let mut p = params(8.0);
        p.grid = 48.0;
        p.to_bounds = false;
        p.to_centers = false;
        p.to_siblings = false;
        // Left edge 5px off line 96; center sits EXACTLY on line 192
        // (width 182). The old engine snapped the center (distance 0) and
        // left the edges off-grid; the edge must win now.
        let unsnapped = rect(101.0, 400.0, 283.0, 500.0);
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
        assert_eq!(snapped.min.x, 96.0);
        assert_eq!(guides[0].edge, "left");
        assert_eq!(guides[0].kind, SnapGuideKind::Grid);
    }

    #[test]
    fn edges_do_not_snap_to_the_pane_center_line() {
        // Same pairing rule from the other side: with "Pane center" on,
        // only the window's center may match it — an edge passing over the
        // pane's center line must not stick to it.
        let unsnapped = rect(497.0, 100.0, 697.0, 300.0);
        let mut p = params(8.0);
        p.to_bounds = false;
        p.to_siblings = false;
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
    fn move_conforms_both_edges_to_grid_when_enabled() {
        let mut p = params(8.0);
        p.grid = 48.0;
        p.to_bounds = false;
        p.to_centers = false;
        p.to_siblings = false;
        p.move_sizes_to_grid = true;
        // Left edge 2px off 96, right edge 5px off 288 (width 195): the
        // translate snap lands the left edge on 96, and conformance then
        // pulls the right edge to 288 — the window resizes to 192.
        let unsnapped = rect(94.0, 400.0, 289.0, 500.0);
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
        assert_eq!(snapped.min.x, 96.0);
        assert_eq!(snapped.max.x, 288.0);
        assert!(guides.iter().any(|g| g.vertical && g.line == 288.0));
    }

    #[test]
    fn conform_never_collapses_a_small_window() {
        let mut p = params(24.0);
        p.grid = 48.0;
        p.to_bounds = false;
        p.to_centers = false;
        p.to_siblings = false;
        p.move_sizes_to_grid = true;
        // 130-wide window (min 120) between lines 96 and 240: pulling the
        // right edge from 226 down to 192 would leave 96 wide — rejected;
        // only the left edge conforms.
        let unsnapped = rect(95.0, 400.0, 225.0, 500.0);
        let (snapped, _) = snap_rect(
            unsnapped,
            AxisGesture::Translate,
            AxisGesture::Idle,
            BOUNDS,
            &[],
            MIN,
            MAX,
            &p,
        );
        assert_eq!(snapped.min.x, 96.0);
        assert!(snapped.width() >= 120.0);
    }

    #[test]
    fn side_handle_draws_one_guide_on_its_own_edge_and_walks_the_grid() {
        // Niffy's S9 (beta.21, grid-only): dragging the bottom edge drew a
        // guide at the TOP of the window, locked to the first grid line
        // forever. v1's latched per-frame classification could mislabel a
        // resize as a move; v2 classifies from totals every frame, so a
        // bottom-edge drag tests exactly one moving edge — the bottom —
        // and its snap advances line to line as the pointer walks.
        let mut p = params(8.0);
        p.grid = 48.0;
        p.to_bounds = false;
        p.to_centers = false;
        p.to_siblings = false;
        let start = rect(100.0, 100.0, 300.0, 285.0);
        for (bottom, expected_line) in [(290.0, 288.0), (338.0, 336.0), (387.0, 384.0)] {
            let reported = rect(100.0, 100.0, 300.0, bottom);
            // Classification straight from totals, as the hook computes it.
            let gesture_y =
                classify_axis(reported.min.y - start.min.y, reported.max.y - start.max.y);
            assert_eq!(gesture_y, AxisGesture::MaxEdge);
            let (snapped, guides) = snap_rect(
                reported,
                AxisGesture::Idle,
                gesture_y,
                BOUNDS,
                &[],
                MIN,
                MAX,
                &p,
            );
            assert_eq!(guides.len(), 1, "exactly one guide for a side handle");
            assert!(!guides[0].vertical);
            assert_eq!(guides[0].edge, "bottom", "guide on the dragged edge");
            assert_eq!(guides[0].line, expected_line, "guide walks the grid");
            assert_eq!(snapped.max.y, expected_line);
            assert_eq!(snapped.min.y, 100.0, "top edge untouched");
        }
    }

    #[test]
    fn exact_tie_prefers_bound_over_grid() {
        let mut p = params(8.0);
        p.grid = 50.0;
        // Pane right bound at 1000 is also a grid line (20 × 50): the
        // guide must say "pane", not "grid".
        let unsnapped = rect(700.0, 100.0, 996.0, 300.0);
        let (snapped, guides) = snap_rect(
            unsnapped,
            AxisGesture::MaxEdge,
            AxisGesture::Idle,
            BOUNDS,
            &[],
            MIN,
            MAX,
            &p,
        );
        assert_eq!(snapped.max.x, 1000.0);
        assert_eq!(guides[0].kind, SnapGuideKind::Bound);
    }

    #[test]
    fn closest_candidate_wins() {
        let siblings = vec![
            sib("far", rect(100.0, 0.0, 303.0, 50.0)),
            sib("near", rect(306.0, 0.0, 500.0, 50.0)),
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
