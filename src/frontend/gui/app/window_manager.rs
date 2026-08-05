//! Pure window-geometry policy: persisted edge anchors and the per-frame
//! anchor solve (Workstream P-A1; spec in
//! `.beads/artifacts/window-system-redesign/spec.md`).
//!
//! A snap that is engaged when a drag releases PROMOTES into a persisted
//! [`WindowAnchors`] entry: "this edge = that pane edge + offset". Geometry
//! then becomes an output — every frame the solver resolves anchored edges
//! against the live pane rect, so sidebar-splitter drags, zone toggles and
//! OS resizes keep docks docked with no re-solve event plumbing at all.
//!
//! The solve is a DISPLAY-TIME layer over the canonical rect store, in the
//! same family as `compute_center_display_rects` and the sidebar squeeze:
//! it NEVER writes `main_window_rects`. The store stays canvas-proportional
//! and unclamped (only user gestures and the pure canonical-canvas rescale
//! write it), which is what keeps resize round-trips exact. The one write
//! rule at the boundary is commit-on-detach: any operation that removes or
//! invalidates an anchor first commits the window's current RESOLVED rect
//! into the store, so nothing teleports to a stale free rect. (A drag does
//! this inherently — gesture tracking writes the on-screen rect.)
//!
//! Sibling anchors are part of the persisted schema from day one so a
//! P-A2 layout never fails to parse on a P-A1 build; the P-A1 solver just
//! treats them as unresolvable and falls back to the free edge.

use super::snap::{AxisGesture, SnapGuide};
use super::*;

/// Min (left/top) or max (right/bottom) edge of a rect along one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AxisSide {
    Min,
    Max,
}

/// What an anchored edge is pinned to. `Pane` is the edge of the zone pane
/// the window lives in — which coincides with the canvas edge when no
/// shell zone claims that side, deliberately ONE ref so promotion is
/// unambiguous.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EdgeRef {
    Pane(AxisSide),
    PaneCenter,
    /// Another window's edge, same zone. In the schema from day one for
    /// forward compatibility; resolution lands in P-A2 (until then it
    /// degrades to the free edge).
    Sibling { key: TabKey, side: AxisSide },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct EdgeAnchor {
    pub target: EdgeRef,
    /// Resolved position = target position + offset. Promotion always
    /// writes 0 (the snap put the edge exactly on the target); the field
    /// exists for `.anchorinfer` and future anchored-with-gap without a
    /// schema change.
    #[serde(default)]
    pub offset: f32,
}

impl EdgeAnchor {
    pub(super) fn pane(side: AxisSide) -> Self {
        Self { target: EdgeRef::Pane(side), offset: 0.0 }
    }
}

/// Per-axis anchoring. An enum rather than two Options: center-vs-edge
/// anchoring is mutually exclusive, and this makes the illegal combination
/// unrepresentable.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AxisAnchoring {
    #[default]
    Free,
    /// Min edge anchored; extent comes from the free rect (still
    /// canvas-proportional, so the window keeps breathing on OS resize).
    Lo(EdgeAnchor),
    /// Max edge anchored; extent from the free rect.
    Hi(EdgeAnchor),
    /// Both edges anchored: size is a solver OUTPUT (min-size resolves
    /// compress-then-push — the hi edge yields).
    Both { lo: EdgeAnchor, hi: EdgeAnchor },
    /// Window center pinned; extent from the free rect.
    Center(EdgeAnchor),
}

impl AxisAnchoring {
    pub(super) fn is_free(&self) -> bool {
        matches!(self, AxisAnchoring::Free)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct WindowAnchors {
    #[serde(default)]
    pub x: AxisAnchoring,
    #[serde(default)]
    pub y: AxisAnchoring,
}

impl WindowAnchors {
    pub(super) fn is_free(&self) -> bool {
        self.x.is_free() && self.y.is_free()
    }
}

/// Resolve one anchor target against the pane's [lo, hi] span on this
/// axis. `None` = unresolvable this frame (sibling refs until P-A2) —
/// callers fall back to the free edge, keeping the anchor for later.
fn resolve_target(target: &EdgeRef, offset: f32, pane_lo: f32, pane_hi: f32) -> Option<f32> {
    match target {
        EdgeRef::Pane(AxisSide::Min) => Some(pane_lo + offset),
        EdgeRef::Pane(AxisSide::Max) => Some(pane_hi + offset),
        EdgeRef::PaneCenter => Some((pane_lo + pane_hi) * 0.5 + offset),
        EdgeRef::Sibling { .. } => None,
    }
}

/// Solve one axis: anchored edges from their refs, free edges from the
/// stored (free) rect. Pure — never writes anything.
pub(super) fn solve_axis(
    anchoring: &AxisAnchoring,
    free_lo: f32,
    free_hi: f32,
    pane_lo: f32,
    pane_hi: f32,
    min_extent: f32,
) -> (f32, f32) {
    let extent = (free_hi - free_lo).max(0.0);
    let resolve = |anchor: &EdgeAnchor| resolve_target(&anchor.target, anchor.offset, pane_lo, pane_hi);
    match anchoring {
        AxisAnchoring::Free => (free_lo, free_hi),
        AxisAnchoring::Lo(anchor) => match resolve(anchor) {
            Some(lo) => (lo, lo + extent),
            None => (free_lo, free_hi),
        },
        AxisAnchoring::Hi(anchor) => match resolve(anchor) {
            Some(hi) => (hi - extent, hi),
            None => (free_lo, free_hi),
        },
        AxisAnchoring::Both { lo, hi } => {
            let lo_pos = resolve(lo).unwrap_or(free_lo);
            let hi_pos = resolve(hi).unwrap_or(free_hi);
            // Compress-then-push: the anchored span may shrink only to the
            // window's minimum; past that the hi edge yields (deterministic,
            // and pane refs outrank sibling refs by construction in P-A1
            // where sibling refs never resolve).
            if hi_pos - lo_pos < min_extent {
                (lo_pos, lo_pos + min_extent)
            } else {
                (lo_pos, hi_pos)
            }
        }
        AxisAnchoring::Center(anchor) => match resolve(anchor) {
            Some(mid) => (mid - extent * 0.5, mid + extent * 0.5),
            None => (free_lo, free_hi),
        },
    }
}

/// Resolve a window's display rect from its anchors, its stored free rect
/// and the live pane rect. The per-frame entry point: because it reads the
/// LIVE pane rect, splitter drags, zone toggles, the sidebar squeeze and
/// OS resizes all re-solve with no event plumbing.
pub(super) fn solve_window_rect(
    anchors: &WindowAnchors,
    free: Rect,
    pane: Rect,
    min_size: Vec2,
) -> Rect {
    let (x_lo, x_hi) = solve_axis(
        &anchors.x,
        free.min.x,
        free.max.x,
        pane.min.x,
        pane.max.x,
        min_size.x,
    );
    let (y_lo, y_hi) = solve_axis(
        &anchors.y,
        free.min.y,
        free.max.y,
        pane.min.y,
        pane.max.y,
        min_size.y,
    );
    Rect::from_min_max(Pos2::new(x_lo, y_lo), Pos2::new(x_hi, y_hi))
}

/// Which window edge an engaged snap guide matched on its axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SnapEdgeKind {
    Lo,
    Hi,
    Center,
}

/// Promotion at drag release, one axis (WYSIWYG — the snap the user saw
/// engaged at drop is the anchor they get):
/// - Translate + engaged edge guide → anchor that edge, clear the rest of
///   the axis (a translate moved both edges; anything else is stale).
///   Engaged center guide → `Center`.
/// - Edge resize + engaged guide → anchor the dragged edge, KEEP an
///   existing opposite-edge anchor (that is how a `Both` stretch window
///   gets built: dock one edge, then drag the other onto a second target).
/// - A gesture with NO engaged promotable guide clears exactly what it
///   invalidated: a translate clears the whole axis (drag-away-past-radius
///   IS the removal gesture; a Shift-suspended release has no guides and
///   behaves the same), an edge resize clears that edge and any center
///   anchor but keeps the untouched edge.
/// - Idle axis: untouched.
/// Grid guides never reach here (`SnapGuide.promote` is None for them, as
/// for sibling guides until P-A2).
pub(super) fn promote_axis(
    current: &AxisAnchoring,
    gesture: AxisGesture,
    engaged: Option<(SnapEdgeKind, EdgeRef)>,
) -> AxisAnchoring {
    let anchor = |target: EdgeRef| EdgeAnchor { target, offset: 0.0 };
    match gesture {
        AxisGesture::Idle => current.clone(),
        AxisGesture::Translate => match engaged {
            Some((SnapEdgeKind::Lo, target)) => AxisAnchoring::Lo(anchor(target)),
            Some((SnapEdgeKind::Hi, target)) => AxisAnchoring::Hi(anchor(target)),
            Some((SnapEdgeKind::Center, target)) => AxisAnchoring::Center(anchor(target)),
            None => AxisAnchoring::Free,
        },
        AxisGesture::MinEdge => match engaged {
            Some((_, target)) => match current {
                AxisAnchoring::Hi(hi) | AxisAnchoring::Both { hi, .. } => AxisAnchoring::Both {
                    lo: anchor(target),
                    hi: hi.clone(),
                },
                _ => AxisAnchoring::Lo(anchor(target)),
            },
            None => match current {
                AxisAnchoring::Hi(hi) | AxisAnchoring::Both { hi, .. } => {
                    AxisAnchoring::Hi(hi.clone())
                }
                _ => AxisAnchoring::Free,
            },
        },
        AxisGesture::MaxEdge => match engaged {
            Some((_, target)) => match current {
                AxisAnchoring::Lo(lo) | AxisAnchoring::Both { lo, .. } => AxisAnchoring::Both {
                    lo: lo.clone(),
                    hi: anchor(target),
                },
                _ => AxisAnchoring::Hi(anchor(target)),
            },
            None => match current {
                AxisAnchoring::Lo(lo) | AxisAnchoring::Both { lo, .. } => {
                    AxisAnchoring::Lo(lo.clone())
                }
                _ => AxisAnchoring::Free,
            },
        },
    }
}

impl VellumGuiApp {
    /// The per-frame display resolve for one window: anchors override the
    /// free rect against the live pane. Free windows pass through
    /// untouched, preserving today's behavior byte-for-byte.
    pub(super) fn resolved_window_rect(
        &self,
        key: &TabKey,
        free: Rect,
        pane: Rect,
        min_size: Vec2,
    ) -> Rect {
        // A live snap gesture owns this window's rect: from the frame the
        // drag state exists, gesture tracking writes the dragged rect into
        // the store, and the solve must NOT re-apply the old anchors on
        // top — most critically on the RELEASE frame, where the resuming
        // position feed would otherwise warp the window back onto its
        // anchor before promotion runs, making the gesture classify as
        // Idle and the drag-away-releases rule dead (live-test defect:
        // "goes right back no matter where I move it"). The drag state is
        // created on the first tracked frame, AFTER that frame's feed was
        // computed — so the gesture's start rect is still the solved rect
        // the user visually grabbed, and no phantom axis deltas appear.
        if self
            .zone_snap_drag
            .as_ref()
            .is_some_and(|drag| drag.tab_key == *key)
        {
            return free;
        }
        match self.window_anchors.get(key) {
            Some(anchors) if !anchors.is_free() => {
                solve_window_rect(anchors, free, pane, min_size)
            }
            _ => free,
        }
    }

    /// Promotion hook, called from `apply_zone_snap` on the release frame
    /// with that frame's gestures and engaged guides. Multiple guides can
    /// exist per axis (grid conformance adds extras); only the promotable
    /// one counts as engaged.
    pub(super) fn promote_release_anchors(
        &mut self,
        tab_key: &TabKey,
        gesture_x: AxisGesture,
        gesture_y: AxisGesture,
        guides: &[SnapGuide],
    ) {
        let engaged = |vertical: bool| {
            guides
                .iter()
                .find(|guide| guide.vertical == vertical && guide.promote.is_some())
                .and_then(|guide| {
                    let target = guide.promote.clone()?;
                    let edge = match guide.edge {
                        "left" | "top" => SnapEdgeKind::Lo,
                        "right" | "bottom" => SnapEdgeKind::Hi,
                        _ => SnapEdgeKind::Center,
                    };
                    Some((edge, target))
                })
        };
        let current = self.window_anchors.get(tab_key).cloned().unwrap_or_default();
        let promoted = WindowAnchors {
            x: promote_axis(&current.x, gesture_x, engaged(true)),
            y: promote_axis(&current.y, gesture_y, engaged(false)),
        };
        if promoted == current {
            return;
        }
        // Release frames are rare; log every anchor change unconditionally
        // so field reports come with evidence without needing `.snapdebug`.
        tracing::info!(
            "anchor promote {:?} gx={:?} gy={:?} guides={} {:?} -> {:?}",
            tab_key,
            gesture_x,
            gesture_y,
            guides.len(),
            current,
            promoted,
        );
        if promoted.is_free() {
            self.window_anchors.remove(tab_key);
        } else {
            self.window_anchors.insert(tab_key.clone(), promoted);
        }
        self.layout_dirty = true;
    }

    /// Context-menu "Release anchors": commit-on-detach, then forget. The
    /// resolved rect (anchors against the pane the window last rendered
    /// in) is written into the store first so the window stays exactly
    /// where the user sees it instead of teleporting to its stale free
    /// rect.
    /// Returns the released anchors so callers that are only *suspending*
    /// them (Move Window) can restore on cancel.
    pub(super) fn release_window_anchors(
        &mut self,
        key: &TabKey,
        zone: GuiShellZone,
    ) -> Option<WindowAnchors> {
        let anchors = self.window_anchors.remove(key)?;
        if anchors.is_free() {
            return None;
        }
        tracing::info!("anchor release {:?} was {:?}", key, anchors);
        if let (Some(free), Some(pane)) = (
            self.main_window_rects
                .get(key)
                .copied()
                .and_then(Self::rect_from_snapshot),
            self.last_zone_pane_rects.get(&zone).copied(),
        ) {
            let resolved = solve_window_rect(
                &anchors,
                free,
                pane,
                Vec2::new(120.0, MIN_DOCKED_WINDOW_HEIGHT),
            );
            self.main_window_rects
                .insert(key.clone(), Self::rect_to_snapshot(resolved));
        }
        self.layout_dirty = true;
        Some(anchors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    }

    const MIN: Vec2 = Vec2::new(120.0, 90.0);

    #[test]
    fn free_axes_pass_through_untouched() {
        let anchors = WindowAnchors::default();
        assert!(anchors.is_free());
        let free = rect(150.0, 20.0, 350.0, 220.0);
        let out = solve_window_rect(&anchors, free, rect(0.0, 0.0, 900.0, 700.0), MIN);
        assert_eq!(out, free);
    }

    #[test]
    fn lo_anchor_follows_pane_edge_extent_free() {
        // THE headline P-A1 fix, inverse of the P-A0 strand pin: a window
        // docked to the pane's left edge follows a splitter drag in BOTH
        // directions, keeping its free-rect extent.
        let anchors = WindowAnchors {
            x: AxisAnchoring::Lo(EdgeAnchor::pane(AxisSide::Min)),
            y: AxisAnchoring::Free,
        };
        let free = rect(150.0, 20.0, 350.0, 220.0);
        // Sidebar shrinks: pane's left edge moves out to 100.
        let out = solve_window_rect(&anchors, free, rect(100.0, 0.0, 1000.0, 800.0), MIN);
        assert_eq!(out, rect(100.0, 20.0, 300.0, 220.0), "follows the edge out");
        // Sidebar grows: pane's left edge moves in to 240.
        let out = solve_window_rect(&anchors, free, rect(240.0, 0.0, 1000.0, 800.0), MIN);
        assert_eq!(out, rect(240.0, 20.0, 440.0, 220.0), "follows the edge in");
    }

    #[test]
    fn hi_anchor_keeps_right_dock_across_pane_widths() {
        let anchors = WindowAnchors {
            x: AxisAnchoring::Hi(EdgeAnchor::pane(AxisSide::Max)),
            y: AxisAnchoring::Free,
        };
        let free = rect(800.0, 300.0, 1000.0, 500.0);
        for pane_right in [900.0, 1000.0, 1400.0] {
            let out =
                solve_window_rect(&anchors, free, rect(0.0, 0.0, pane_right, 800.0), MIN);
            assert_eq!(out.max.x, pane_right);
            assert_eq!(out.width(), 200.0, "extent stays the free-rect extent");
        }
    }

    #[test]
    fn both_anchors_span_pane_and_compress_then_push() {
        let anchors = WindowAnchors {
            x: AxisAnchoring::Both {
                lo: EdgeAnchor::pane(AxisSide::Min),
                hi: EdgeAnchor::pane(AxisSide::Max),
            },
            y: AxisAnchoring::Free,
        };
        let free = rect(0.0, 0.0, 600.0, 200.0);
        // Size is a solver output: the window spans whatever the pane is.
        let out = solve_window_rect(&anchors, free, rect(200.0, 0.0, 900.0, 800.0), MIN);
        assert_eq!((out.min.x, out.max.x), (200.0, 900.0));
        // Pane narrower than min width: lo holds, hi yields outward.
        let out = solve_window_rect(&anchors, free, rect(200.0, 0.0, 280.0, 800.0), MIN);
        assert_eq!((out.min.x, out.max.x), (200.0, 320.0), "hi yields at min size");
    }

    #[test]
    fn center_anchor_recenters_after_pane_resize() {
        let anchors = WindowAnchors {
            x: AxisAnchoring::Center(EdgeAnchor { target: EdgeRef::PaneCenter, offset: 0.0 }),
            y: AxisAnchoring::Free,
        };
        let free = rect(400.0, 0.0, 600.0, 100.0);
        let out = solve_window_rect(&anchors, free, rect(0.0, 0.0, 1400.0, 800.0), MIN);
        assert_eq!(out.center().x, 700.0);
        assert_eq!(out.width(), 200.0);
    }

    #[test]
    fn sibling_anchor_degrades_to_free_edge_in_pa1() {
        // The P-A2 variant is in the schema now so its layouts parse here;
        // until the sibling solver lands it resolves to the free edge and
        // the anchor is KEPT (hide/show round-trips will re-attach).
        let anchors = WindowAnchors {
            x: AxisAnchoring::Lo(EdgeAnchor {
                target: EdgeRef::Sibling { key: TabKey::Vitals, side: AxisSide::Max },
                offset: 0.0,
            }),
            y: AxisAnchoring::Free,
        };
        let free = rect(150.0, 20.0, 350.0, 220.0);
        let out = solve_window_rect(&anchors, free, rect(0.0, 0.0, 900.0, 700.0), MIN);
        assert_eq!(out, free);
    }

    #[test]
    fn promote_translate_with_edge_guide_anchors_edge_and_clears_axis() {
        // Dragged the window until its left edge snapped to the pane edge:
        // x becomes Lo(pane-min); a stale Hi anchor on that axis is gone.
        let current = AxisAnchoring::Hi(EdgeAnchor::pane(AxisSide::Max));
        let out = promote_axis(
            &current,
            AxisGesture::Translate,
            Some((SnapEdgeKind::Lo, EdgeRef::Pane(AxisSide::Min))),
        );
        assert_eq!(out, AxisAnchoring::Lo(EdgeAnchor::pane(AxisSide::Min)));
    }

    #[test]
    fn promote_translate_with_center_guide_pins_center() {
        let out = promote_axis(
            &AxisAnchoring::Free,
            AxisGesture::Translate,
            Some((SnapEdgeKind::Center, EdgeRef::PaneCenter)),
        );
        assert_eq!(
            out,
            AxisAnchoring::Center(EdgeAnchor { target: EdgeRef::PaneCenter, offset: 0.0 })
        );
    }

    #[test]
    fn promote_edge_resize_builds_both_and_keeps_other_edge() {
        // Right edge already docked to pane-right; the user drags the LEFT
        // edge until it snaps to pane-left: the window becomes a stretch
        // window anchored on both edges.
        let current = AxisAnchoring::Hi(EdgeAnchor::pane(AxisSide::Max));
        let out = promote_axis(
            &current,
            AxisGesture::MinEdge,
            Some((SnapEdgeKind::Lo, EdgeRef::Pane(AxisSide::Min))),
        );
        assert_eq!(
            out,
            AxisAnchoring::Both {
                lo: EdgeAnchor::pane(AxisSide::Min),
                hi: EdgeAnchor::pane(AxisSide::Max),
            }
        );
    }

    #[test]
    fn promote_translate_without_guide_frees_the_axis() {
        // Drag-away-past-radius (or a Shift-suspended release) is the
        // removal gesture.
        let current = AxisAnchoring::Both {
            lo: EdgeAnchor::pane(AxisSide::Min),
            hi: EdgeAnchor::pane(AxisSide::Max),
        };
        assert_eq!(
            promote_axis(&current, AxisGesture::Translate, None),
            AxisAnchoring::Free
        );
    }

    #[test]
    fn promote_edge_resize_without_guide_clears_only_that_edge() {
        // Dragging the lo edge off its snap invalidates the lo anchor but
        // the untouched hi edge keeps its dock.
        let current = AxisAnchoring::Both {
            lo: EdgeAnchor::pane(AxisSide::Min),
            hi: EdgeAnchor::pane(AxisSide::Max),
        };
        assert_eq!(
            promote_axis(&current, AxisGesture::MinEdge, None),
            AxisAnchoring::Hi(EdgeAnchor::pane(AxisSide::Max))
        );
        // And the mirror: hi-edge drag-away keeps the lo dock.
        assert_eq!(
            promote_axis(&current, AxisGesture::MaxEdge, None),
            AxisAnchoring::Lo(EdgeAnchor::pane(AxisSide::Min))
        );
    }

    #[test]
    fn promote_idle_axis_is_untouched() {
        let current = AxisAnchoring::Lo(EdgeAnchor::pane(AxisSide::Min));
        assert_eq!(promote_axis(&current, AxisGesture::Idle, None), current);
    }

    #[test]
    fn anchors_serde_round_trip_and_legacy_default() {
        let anchors = WindowAnchors {
            x: AxisAnchoring::Both {
                lo: EdgeAnchor::pane(AxisSide::Min),
                hi: EdgeAnchor {
                    target: EdgeRef::Sibling { key: TabKey::Vitals, side: AxisSide::Min },
                    offset: -4.0,
                },
            },
            y: AxisAnchoring::Center(EdgeAnchor { target: EdgeRef::PaneCenter, offset: 0.0 }),
        };
        let json = serde_json::to_string(&anchors).unwrap();
        let back: WindowAnchors = serde_json::from_str(&json).unwrap();
        assert_eq!(back, anchors);

        // Missing axes deserialize Free.
        let legacy: WindowAnchors = serde_json::from_str("{}").unwrap();
        assert!(legacy.is_free());
    }
}
