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
//! P-A2 adds sibling anchors ("my edge = that window's edge"): the
//! per-zone batch solve resolves windows in dependency order (Kahn
//! toposort), promotion-time cycle refusal keeps the persisted graph a
//! DAG, dangling refs (target hidden/detached/deleted/other zone) fall
//! back to the free edge with the anchor kept so re-show re-attaches, and
//! deletion/zone-moves prune dependents' refs after committing their
//! resolved rects.

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

/// Yield rank for the Both min-size rule: when an anchored span shrinks
/// below the window minimum, the LOWER-ranked edge yields (ties: hi
/// yields). Pane edges outrank sibling/center refs.
fn hold_rank(target: &EdgeRef) -> u8 {
    match target {
        EdgeRef::Pane(_) => 2,
        EdgeRef::PaneCenter | EdgeRef::Sibling { .. } => 1,
    }
}

/// Solve one axis: anchored edges from their refs, free edges from the
/// stored (free) rect. `resolve` maps a target to a coordinate — `None` =
/// unresolvable this frame (dangling or cyclic sibling refs) and falls
/// back to the free edge, KEEPING the anchor for later re-attach. Pure.
pub(super) fn solve_axis_with(
    anchoring: &AxisAnchoring,
    free_lo: f32,
    free_hi: f32,
    min_extent: f32,
    resolve: impl Fn(&EdgeRef) -> Option<f32>,
) -> (f32, f32) {
    let extent = (free_hi - free_lo).max(0.0);
    let resolve = |anchor: &EdgeAnchor| resolve(&anchor.target).map(|pos| pos + anchor.offset);
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
            // window's minimum; past that the lower-ranked edge yields
            // (pane refs hold over sibling refs; ties: hi yields).
            if hi_pos - lo_pos < min_extent {
                if hold_rank(&hi.target) > hold_rank(&lo.target) {
                    (hi_pos - min_extent, hi_pos)
                } else {
                    (lo_pos, lo_pos + min_extent)
                }
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

fn pane_target(target: &EdgeRef, pane_lo: f32, pane_hi: f32) -> Option<f32> {
    match target {
        EdgeRef::Pane(AxisSide::Min) => Some(pane_lo),
        EdgeRef::Pane(AxisSide::Max) => Some(pane_hi),
        EdgeRef::PaneCenter => Some((pane_lo + pane_hi) * 0.5),
        EdgeRef::Sibling { .. } => None,
    }
}

/// Resolve ONE window against the pane only — sibling refs fall back to
/// the free edge. For single-window contexts (commit-on-detach outside a
/// frame's batch); the per-frame path is [`solve_zone_rects`].
pub(super) fn solve_window_rect(
    anchors: &WindowAnchors,
    free: Rect,
    pane: Rect,
    min_size: Vec2,
) -> Rect {
    let (x_lo, x_hi) = solve_axis_with(&anchors.x, free.min.x, free.max.x, min_size.x, |t| {
        pane_target(t, pane.min.x, pane.max.x)
    });
    let (y_lo, y_hi) = solve_axis_with(&anchors.y, free.min.y, free.max.y, min_size.y, |t| {
        pane_target(t, pane.min.y, pane.max.y)
    });
    Rect::from_min_max(Pos2::new(x_lo, y_lo), Pos2::new(x_hi, y_hi))
}

/// One window's inputs to the per-zone batch solve.
pub(super) struct ZoneSolveInput {
    pub key: TabKey,
    pub free: Rect,
    pub min_size: Vec2,
}

/// Collect the sibling target keys inside one window's anchors.
fn sibling_targets(anchors: &WindowAnchors, out: &mut Vec<TabKey>) {
    let mut push_axis = |axis: &AxisAnchoring| {
        let mut push_anchor = |anchor: &EdgeAnchor| {
            if let EdgeRef::Sibling { key, .. } = &anchor.target {
                out.push(key.clone());
            }
        };
        match axis {
            AxisAnchoring::Free => {}
            AxisAnchoring::Lo(a) | AxisAnchoring::Hi(a) | AxisAnchoring::Center(a) => {
                push_anchor(a)
            }
            AxisAnchoring::Both { lo, hi } => {
                push_anchor(lo);
                push_anchor(hi);
            }
        }
    };
    push_axis(&anchors.x);
    push_axis(&anchors.y);
}

/// True when `from`'s sibling-anchor graph can reach `to` (promotion-time
/// cycle refusal walks this before persisting a new sibling anchor).
pub(super) fn sibling_graph_reaches(
    anchors: &HashMap<TabKey, WindowAnchors>,
    from: &TabKey,
    to: &TabKey,
) -> bool {
    let mut stack = vec![from.clone()];
    let mut seen: HashSet<TabKey> = HashSet::new();
    while let Some(key) = stack.pop() {
        if key == *to {
            return true;
        }
        if !seen.insert(key.clone()) {
            continue;
        }
        if let Some(a) = anchors.get(&key) {
            let mut targets = Vec::new();
            sibling_targets(a, &mut targets);
            stack.extend(targets);
        }
    }
    false
}

/// Resolve every window in a zone (P-A2): pane/center refs directly,
/// sibling refs against the referenced window's RESOLVED rect. Windows
/// are processed in dependency order (Kahn toposort — one pass, order
/// independent of map iteration). Promotion-time refusal keeps the
/// persisted graph a DAG; a cycle that sneaks in anyway (hand-edited
/// file) degrades every member to fully Free for the frame, logged once
/// per session. A sibling ref whose target is not in this zone's set
/// this frame (hidden, detached, deleted, other zone) resolves to `None`
/// → free-edge fallback, anchor KEPT so hide/show re-attaches.
/// `skip` is the window whose live gesture owns its rect: it solves as
/// Free, and siblings anchored to it track its dragged rect.
pub(super) fn solve_zone_rects(
    windows: &[ZoneSolveInput],
    anchors: &HashMap<TabKey, WindowAnchors>,
    pane: Rect,
    skip: Option<&TabKey>,
) -> HashMap<TabKey, Rect> {
    use std::collections::VecDeque;

    let index_of: HashMap<&TabKey, usize> =
        windows.iter().enumerate().map(|(i, w)| (&w.key, i)).collect();
    let effective = |key: &TabKey| -> Option<&WindowAnchors> {
        if skip == Some(key) {
            return None;
        }
        anchors.get(key).filter(|a| !a.is_free())
    };

    // Dependency edges only for targets present in this zone this frame;
    // absent targets are dangling (resolve None), not ordering inputs.
    let n = windows.len();
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_degree: Vec<usize> = vec![0; n];
    for (i, w) in windows.iter().enumerate() {
        if let Some(a) = effective(&w.key) {
            let mut targets = Vec::new();
            sibling_targets(a, &mut targets);
            for target in targets {
                if let Some(&j) = index_of.get(&target) {
                    dependents[j].push(i);
                    in_degree[i] += 1;
                }
            }
        }
    }
    let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut resolved: HashMap<TabKey, Rect> = HashMap::with_capacity(n);
    let mut processed = 0usize;
    while let Some(i) = queue.pop_front() {
        processed += 1;
        let w = &windows[i];
        let rect = match effective(&w.key) {
            Some(a) => {
                let x = solve_axis_with(&a.x, w.free.min.x, w.free.max.x, w.min_size.x, |t| {
                    match t {
                        EdgeRef::Sibling { key, side } => {
                            resolved.get(key).map(|r| match side {
                                AxisSide::Min => r.min.x,
                                AxisSide::Max => r.max.x,
                            })
                        }
                        _ => pane_target(t, pane.min.x, pane.max.x),
                    }
                });
                let y = solve_axis_with(&a.y, w.free.min.y, w.free.max.y, w.min_size.y, |t| {
                    match t {
                        EdgeRef::Sibling { key, side } => {
                            resolved.get(key).map(|r| match side {
                                AxisSide::Min => r.min.y,
                                AxisSide::Max => r.max.y,
                            })
                        }
                        _ => pane_target(t, pane.min.y, pane.max.y),
                    }
                });
                Rect::from_min_max(Pos2::new(x.0, y.0), Pos2::new(x.1, y.1))
            }
            None => w.free,
        };
        resolved.insert(w.key.clone(), rect);
        for &d in &dependents[i] {
            in_degree[d] -= 1;
            if in_degree[d] == 0 {
                queue.push_back(d);
            }
        }
    }
    if processed < n {
        // Cycle members never reached in-degree 0: fully Free this frame.
        static CYCLE_LOGGED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !CYCLE_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!(
                "anchor cycle in persisted layout: {} window(s) degraded to free placement",
                n - processed
            );
        }
        for w in windows {
            resolved.entry(w.key.clone()).or_insert(w.free);
        }
    }
    resolved
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
                    // Cycle refusal (P-A2): persisting "me → sibling" when
                    // the sibling's own anchor graph already reaches me
                    // would create a cycle — the snap still happened
                    // visually, but the anchor is not persisted, keeping
                    // the graph a DAG by construction.
                    if let EdgeRef::Sibling { key, .. } = &target {
                        if sibling_graph_reaches(&self.window_anchors, key, tab_key) {
                            tracing::info!(
                                "anchor promote refused (cycle) {:?} -> {:?}",
                                tab_key,
                                key,
                            );
                            return None;
                        }
                    }
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

    /// Per-frame batch solve for one zone's windows. The window whose live
    /// snap gesture owns its rect solves as FREE: from the frame the drag
    /// state exists, gesture tracking writes the dragged rect into the
    /// store, and the solve must not re-apply the old anchors on top —
    /// most critically on the RELEASE frame, where the resuming position
    /// feed would otherwise warp the window back onto its anchor before
    /// promotion runs, making the gesture classify as Idle and the
    /// drag-away-releases rule dead ("goes right back no matter where I
    /// move it"). The drag state is created on the first tracked frame,
    /// AFTER that frame's feed was computed — so the gesture's start rect
    /// is still the solved rect the user visually grabbed, and no phantom
    /// axis deltas appear.
    pub(super) fn solve_zone_anchor_rects(
        &self,
        windows: &[ZoneSolveInput],
        pane: Rect,
    ) -> HashMap<TabKey, Rect> {
        let skip = self.zone_snap_drag.as_ref().map(|drag| &drag.tab_key);
        solve_zone_rects(windows, &self.window_anchors, pane, skip)
    }

    /// Contact inference for a window ENTERING a shell zone (owner spec:
    /// "if its borders are touching the OS side and the partitioner it
    /// should auto-anchor to those two sides; if the top is touching the
    /// top or another window, it should auto-anchor — it shouldn't stay a
    /// free agent unless it's touching nothing"). Same matching as
    /// `.anchorinfer` (pane edges outrank sibling edges, sibling refs
    /// cycle-guarded) but scoped to ONE window with a drop-friendly
    /// tolerance, and it REPLACES the window's anchors wholesale — the
    /// old zone's anchors were already released on entry, and an edge
    /// touching nothing stays free. Center entries never auto-anchor.
    pub(super) fn apply_zone_entry_anchor_inference(&mut self, key: &TabKey, zone: GuiShellZone) {
        const EPS: f32 = 8.0;
        if zone == GuiShellZone::Center {
            return;
        }
        let Some(pane) = self.last_zone_pane_rects.get(&zone).copied() else {
            return;
        };
        let Some(rect) = self
            .main_window_rects
            .get(key)
            .copied()
            .and_then(Self::rect_from_snapshot)
        else {
            return;
        };
        let rect = Self::clamp_main_window_rect(rect, pane);
        let siblings: Vec<(TabKey, Rect)> = self
            .tab_zones
            .iter()
            .filter(|(other, assigned)| **assigned == zone && *other != key)
            .filter(|(other, _)| {
                !self.hidden_tabs.contains(*other) && !self.detached_tabs.contains_key(*other)
            })
            .filter_map(|(other, _)| {
                self.main_window_rects
                    .get(other)
                    .copied()
                    .and_then(Self::rect_from_snapshot)
                    .map(|r| (other.clone(), Self::clamp_main_window_rect(r, pane)))
            })
            .collect();
        let infer_edge = |value: f32,
                          pane_lo: f32,
                          pane_hi: f32,
                          sibling_edges: &dyn Fn(&Rect) -> (f32, f32)|
         -> Option<EdgeRef> {
            if (value - pane_lo).abs() <= EPS {
                return Some(EdgeRef::Pane(AxisSide::Min));
            }
            if (value - pane_hi).abs() <= EPS {
                return Some(EdgeRef::Pane(AxisSide::Max));
            }
            for (other, other_rect) in &siblings {
                // The entering window's refs can't form a cycle unless the
                // sibling's graph already reaches it (its old refs were
                // pruned on entry, but re-entering the same zone keeps
                // dependents alive — guard anyway).
                if sibling_graph_reaches(&self.window_anchors, other, key) {
                    continue;
                }
                let (lo, hi) = sibling_edges(other_rect);
                if (value - lo).abs() <= EPS {
                    return Some(EdgeRef::Sibling { key: other.clone(), side: AxisSide::Min });
                }
                if (value - hi).abs() <= EPS {
                    return Some(EdgeRef::Sibling { key: other.clone(), side: AxisSide::Max });
                }
            }
            None
        };
        let axis = |lo_val: f32,
                    hi_val: f32,
                    pane_lo: f32,
                    pane_hi: f32,
                    edges: &dyn Fn(&Rect) -> (f32, f32)|
         -> AxisAnchoring {
            let anchor = |t: EdgeRef| EdgeAnchor { target: t, offset: 0.0 };
            match (
                infer_edge(lo_val, pane_lo, pane_hi, edges),
                infer_edge(hi_val, pane_lo, pane_hi, edges),
            ) {
                (Some(lo), Some(hi)) => AxisAnchoring::Both { lo: anchor(lo), hi: anchor(hi) },
                (Some(lo), None) => AxisAnchoring::Lo(anchor(lo)),
                (None, Some(hi)) => AxisAnchoring::Hi(anchor(hi)),
                (None, None) => AxisAnchoring::Free,
            }
        };
        let anchors = WindowAnchors {
            x: axis(rect.min.x, rect.max.x, pane.min.x, pane.max.x, &|r: &Rect| {
                (r.min.x, r.max.x)
            }),
            y: axis(rect.min.y, rect.max.y, pane.min.y, pane.max.y, &|r: &Rect| {
                (r.min.y, r.max.y)
            }),
        };
        if anchors.is_free() {
            self.window_anchors.remove(key);
        } else {
            tracing::info!("anchor zone-entry {:?} in {:?}: {:?}", key, zone, anchors);
            self.window_anchors.insert(key.clone(), anchors);
        }
    }

    /// `.anchorinfer` — one-shot, explicit opt-in (never automatic, so old
    /// layouts load unchanged): synthesize anchors for FREE axes whose
    /// edges already sit flush (±1.5px) against a pane edge or a sibling
    /// edge, and report what was anchored. Undo is the normal removal
    /// gesture: drag the window off its snap, or "Release Anchors".
    pub(super) fn anchor_infer(&mut self) {
        const EPS: f32 = 1.5;
        // Visible, non-detached windows with stored rects, per zone, using
        // the same clamped rects the free feed displays.
        let mut by_zone: HashMap<GuiShellZone, Vec<(TabKey, Rect)>> = HashMap::new();
        for key in self.available_tabs.keys() {
            if self.hidden_tabs.contains(key) || self.detached_tabs.contains_key(key) {
                continue;
            }
            let Some(rect) = self
                .main_window_rects
                .get(key)
                .copied()
                .and_then(Self::rect_from_snapshot)
            else {
                continue;
            };
            let zone = self.zone_for_tab(key);
            let Some(pane) = self.last_zone_pane_rects.get(&zone).copied() else {
                continue;
            };
            by_zone
                .entry(zone)
                .or_default()
                .push((key.clone(), Self::clamp_main_window_rect(rect, pane)));
        }

        // Edge match: pane first (higher-value anchor), then the first
        // flush sibling edge.
        let infer_edge = |value: f32,
                          pane_lo: f32,
                          pane_hi: f32,
                          siblings: &[(TabKey, f32, f32)]|
         -> Option<(EdgeRef, String)> {
            if (value - pane_lo).abs() <= EPS {
                return Some((EdgeRef::Pane(AxisSide::Min), "pane".to_string()));
            }
            if (value - pane_hi).abs() <= EPS {
                return Some((EdgeRef::Pane(AxisSide::Max), "pane".to_string()));
            }
            for (key, lo, hi) in siblings {
                if (value - lo).abs() <= EPS {
                    return Some((
                        EdgeRef::Sibling { key: key.clone(), side: AxisSide::Min },
                        key.short_id(),
                    ));
                }
                if (value - hi).abs() <= EPS {
                    return Some((
                        EdgeRef::Sibling { key: key.clone(), side: AxisSide::Max },
                        key.short_id(),
                    ));
                }
            }
            None
        };

        let mut report: Vec<String> = Vec::new();
        let mut zones: Vec<_> = by_zone.into_iter().collect();
        zones.sort_by_key(|(zone, _)| *zone as u8);
        for (zone, mut windows) in zones {
            let pane = self.last_zone_pane_rects[&zone];
            // Deterministic order so sibling-cycle refusal is stable.
            windows.sort_by_key(|(key, _)| key.short_id());
            for (key, rect) in &windows {
                let current = self.window_anchors.get(key).cloned().unwrap_or_default();
                let siblings_x: Vec<(TabKey, f32, f32)> = windows
                    .iter()
                    .filter(|(other, _)| other != key)
                    .map(|(other, r)| (other.clone(), r.min.x, r.max.x))
                    .collect();
                let siblings_y: Vec<(TabKey, f32, f32)> = windows
                    .iter()
                    .filter(|(other, _)| other != key)
                    .map(|(other, r)| (other.clone(), r.min.y, r.max.y))
                    .collect();
                let mut labels: Vec<String> = Vec::new();
                let mut infer_axis = |free: bool,
                                      lo_val: f32,
                                      hi_val: f32,
                                      pane_lo: f32,
                                      pane_hi: f32,
                                      siblings: &[(TabKey, f32, f32)],
                                      names: [&str; 2],
                                      anchors_map: &HashMap<TabKey, WindowAnchors>|
                 -> Option<AxisAnchoring> {
                    if !free {
                        return None;
                    }
                    let ok = |target: &EdgeRef| match target {
                        EdgeRef::Sibling { key: other, .. } => {
                            !sibling_graph_reaches(anchors_map, other, key)
                        }
                        _ => true,
                    };
                    let lo = infer_edge(lo_val, pane_lo, pane_hi, siblings)
                        .filter(|(t, _)| ok(t));
                    let hi = infer_edge(hi_val, pane_lo, pane_hi, siblings)
                        .filter(|(t, _)| ok(t));
                    let anchor = |t: EdgeRef| EdgeAnchor { target: t, offset: 0.0 };
                    match (lo, hi) {
                        (Some((lo_t, lo_d)), Some((hi_t, hi_d))) => {
                            labels.push(format!("{}→{}, {}→{}", names[0], lo_d, names[1], hi_d));
                            Some(AxisAnchoring::Both { lo: anchor(lo_t), hi: anchor(hi_t) })
                        }
                        (Some((lo_t, lo_d)), None) => {
                            labels.push(format!("{}→{}", names[0], lo_d));
                            Some(AxisAnchoring::Lo(anchor(lo_t)))
                        }
                        (None, Some((hi_t, hi_d))) => {
                            labels.push(format!("{}→{}", names[1], hi_d));
                            Some(AxisAnchoring::Hi(anchor(hi_t)))
                        }
                        (None, None) => None,
                    }
                };
                let x = infer_axis(
                    current.x.is_free(),
                    rect.min.x,
                    rect.max.x,
                    pane.min.x,
                    pane.max.x,
                    &siblings_x,
                    ["left", "right"],
                    &self.window_anchors,
                );
                let y = infer_axis(
                    current.y.is_free(),
                    rect.min.y,
                    rect.max.y,
                    pane.min.y,
                    pane.max.y,
                    &siblings_y,
                    ["top", "bottom"],
                    &self.window_anchors,
                );
                if x.is_none() && y.is_none() {
                    continue;
                }
                let updated = WindowAnchors {
                    x: x.unwrap_or(current.x),
                    y: y.unwrap_or(current.y),
                };
                tracing::info!("anchor infer {:?} -> {:?}", key, updated);
                self.window_anchors.insert(key.clone(), updated);
                self.layout_dirty = true;
                let title = self
                    .available_tabs
                    .get(key)
                    .map(|tab| tab.id.title.clone())
                    .unwrap_or_else(|| key.short_id());
                report.push(format!("{} ({})", title, labels.join(", ")));
            }
        }
        if report.is_empty() {
            self.app_core.add_system_message(
                "anchorinfer: no free flush edges found — nothing anchored.",
            );
        } else {
            self.app_core.add_system_message(&format!(
                "anchorinfer: anchored {} window(s): {}. Drag a window off its snap (or right-click → Release Anchors) to undo.",
                report.len(),
                report.join("; ")
            ));
        }
    }

    /// Strip every sibling anchor referencing `gone` from other windows
    /// (window deleted or moved to another zone). Commit-on-detach first:
    /// each dependent's resolved rect — with `gone` resolved one level
    /// deep against its own pane anchors — is written into the store so
    /// nothing teleports; the surviving anchors on other targets are kept.
    pub(super) fn prune_sibling_refs_to(&mut self, gone: &TabKey) {
        let refers = |anchor: &EdgeAnchor| {
            matches!(&anchor.target, EdgeRef::Sibling { key, .. } if key == gone)
        };
        let strip_axis = |axis: &AxisAnchoring| -> AxisAnchoring {
            match axis {
                AxisAnchoring::Lo(a) | AxisAnchoring::Hi(a) | AxisAnchoring::Center(a)
                    if refers(a) =>
                {
                    AxisAnchoring::Free
                }
                AxisAnchoring::Both { lo, hi } => match (refers(lo), refers(hi)) {
                    (true, true) => AxisAnchoring::Free,
                    (true, false) => AxisAnchoring::Hi(hi.clone()),
                    (false, true) => AxisAnchoring::Lo(lo.clone()),
                    (false, false) => axis.clone(),
                },
                other => other.clone(),
            }
        };

        let dependents: Vec<TabKey> = self
            .window_anchors
            .iter()
            .filter(|(key, anchors)| {
                *key != gone && {
                    let mut targets = Vec::new();
                    sibling_targets(anchors, &mut targets);
                    targets.contains(gone)
                }
            })
            .map(|(key, _)| key.clone())
            .collect();
        if dependents.is_empty() {
            return;
        }

        // `gone`'s display rect, one level deep (its own pane anchors
        // against its zone's pane); deeper sibling chains degrade to its
        // free rect — the commit is best-effort placement, not layout.
        let gone_rect = self
            .main_window_rects
            .get(gone)
            .copied()
            .and_then(Self::rect_from_snapshot)
            .map(|free| {
                let pane = self
                    .tab_zones
                    .get(gone)
                    .and_then(|zone| self.last_zone_pane_rects.get(zone))
                    .copied();
                match (self.window_anchors.get(gone), pane) {
                    (Some(anchors), Some(pane)) => solve_window_rect(
                        anchors,
                        free,
                        pane,
                        Vec2::new(120.0, MIN_DOCKED_WINDOW_HEIGHT),
                    ),
                    _ => free,
                }
            });

        for dep in dependents {
            let anchors = self.window_anchors.get(&dep).cloned().unwrap_or_default();
            tracing::info!("anchor prune {:?}: drops refs to {:?}", dep, gone);
            // Commit the dependent where it currently resolves.
            if let (Some(free), Some(pane)) = (
                self.main_window_rects
                    .get(&dep)
                    .copied()
                    .and_then(Self::rect_from_snapshot),
                self.tab_zones
                    .get(&dep)
                    .and_then(|zone| self.last_zone_pane_rects.get(zone))
                    .copied(),
            ) {
                let min = Vec2::new(120.0, MIN_DOCKED_WINDOW_HEIGHT);
                let resolve_x = |t: &EdgeRef| match t {
                    EdgeRef::Sibling { key, side } if key == gone => {
                        gone_rect.map(|r| match side {
                            AxisSide::Min => r.min.x,
                            AxisSide::Max => r.max.x,
                        })
                    }
                    EdgeRef::Sibling { .. } => None,
                    _ => pane_target(t, pane.min.x, pane.max.x),
                };
                let resolve_y = |t: &EdgeRef| match t {
                    EdgeRef::Sibling { key, side } if key == gone => {
                        gone_rect.map(|r| match side {
                            AxisSide::Min => r.min.y,
                            AxisSide::Max => r.max.y,
                        })
                    }
                    EdgeRef::Sibling { .. } => None,
                    _ => pane_target(t, pane.min.y, pane.max.y),
                };
                let x = solve_axis_with(&anchors.x, free.min.x, free.max.x, min.x, resolve_x);
                let y = solve_axis_with(&anchors.y, free.min.y, free.max.y, min.y, resolve_y);
                let resolved =
                    Rect::from_min_max(Pos2::new(x.0, y.0), Pos2::new(x.1, y.1));
                self.main_window_rects
                    .insert(dep.clone(), Self::rect_to_snapshot(resolved));
            }
            let stripped = WindowAnchors {
                x: strip_axis(&anchors.x),
                y: strip_axis(&anchors.y),
            };
            if stripped.is_free() {
                self.window_anchors.remove(&dep);
            } else {
                self.window_anchors.insert(dep, stripped);
            }
            self.layout_dirty = true;
        }
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

    fn key(id: &str) -> TabKey {
        TabKey::TextByName { id: id.to_string() }
    }

    fn input(id: &str, free: Rect) -> ZoneSolveInput {
        ZoneSolveInput { key: key(id), free, min_size: MIN }
    }

    fn sib_anchor(id: &str, side: AxisSide) -> EdgeAnchor {
        EdgeAnchor {
            target: EdgeRef::Sibling { key: key(id), side },
            offset: 0.0,
        }
    }

    #[test]
    fn zone_solve_chain_resolves_in_dependency_order() {
        // C depends on B depends on A (each left edge butted against the
        // previous window's right edge; A docked to the pane). Input order
        // is deliberately reversed: the toposort must resolve A first
        // regardless of slice or map order.
        let pane = rect(100.0, 0.0, 1000.0, 800.0);
        let mut anchors = HashMap::new();
        anchors.insert(
            key("a"),
            WindowAnchors {
                x: AxisAnchoring::Lo(EdgeAnchor::pane(AxisSide::Min)),
                y: AxisAnchoring::Free,
            },
        );
        anchors.insert(
            key("b"),
            WindowAnchors {
                x: AxisAnchoring::Lo(sib_anchor("a", AxisSide::Max)),
                y: AxisAnchoring::Free,
            },
        );
        anchors.insert(
            key("c"),
            WindowAnchors {
                x: AxisAnchoring::Lo(sib_anchor("b", AxisSide::Max)),
                y: AxisAnchoring::Free,
            },
        );
        let windows = vec![
            input("c", rect(500.0, 0.0, 700.0, 100.0)),
            input("b", rect(300.0, 0.0, 500.0, 100.0)),
            input("a", rect(150.0, 0.0, 350.0, 100.0)),
        ];
        let out = solve_zone_rects(&windows, &anchors, pane, None);
        assert_eq!(out[&key("a")].min.x, 100.0, "a docks to the pane");
        assert_eq!(out[&key("b")].min.x, out[&key("a")].max.x, "b butts a");
        assert_eq!(out[&key("c")].min.x, out[&key("b")].max.x, "c butts b");
        // The whole train follows a splitter drag: pane edge moves right.
        let out = solve_zone_rects(&windows, &anchors, rect(240.0, 0.0, 1000.0, 800.0), None);
        assert_eq!(out[&key("a")].min.x, 240.0);
        assert_eq!(out[&key("c")].min.x, 240.0 + 200.0 + 200.0);
    }

    #[test]
    fn zone_solve_cycle_degrades_members_to_free_others_unaffected() {
        // A↔B reference each other (hand-edited layout; promotion refuses
        // to create this). Both members render at their free rects; the
        // pane-anchored bystander still solves.
        let pane = rect(0.0, 0.0, 1000.0, 800.0);
        let mut anchors = HashMap::new();
        anchors.insert(
            key("a"),
            WindowAnchors {
                x: AxisAnchoring::Lo(sib_anchor("b", AxisSide::Max)),
                y: AxisAnchoring::Free,
            },
        );
        anchors.insert(
            key("b"),
            WindowAnchors {
                x: AxisAnchoring::Lo(sib_anchor("a", AxisSide::Max)),
                y: AxisAnchoring::Free,
            },
        );
        anchors.insert(
            key("dock"),
            WindowAnchors {
                x: AxisAnchoring::Hi(EdgeAnchor::pane(AxisSide::Max)),
                y: AxisAnchoring::Free,
            },
        );
        let windows = vec![
            input("a", rect(10.0, 0.0, 210.0, 100.0)),
            input("b", rect(300.0, 0.0, 500.0, 100.0)),
            input("dock", rect(700.0, 0.0, 900.0, 100.0)),
        ];
        let out = solve_zone_rects(&windows, &anchors, pane, None);
        assert_eq!(out[&key("a")], rect(10.0, 0.0, 210.0, 100.0), "cycle → free");
        assert_eq!(out[&key("b")], rect(300.0, 0.0, 500.0, 100.0), "cycle → free");
        assert_eq!(out[&key("dock")].max.x, 1000.0, "bystander still docks");
    }

    #[test]
    fn zone_solve_dangling_sibling_falls_back_to_free_edge() {
        // Target hidden/deleted/other zone: the anchored edge uses the
        // free rect this frame; the anchor itself is untouched (the map is
        // read-only to the solver), so re-show re-attaches.
        let pane = rect(0.0, 0.0, 1000.0, 800.0);
        let mut anchors = HashMap::new();
        anchors.insert(
            key("b"),
            WindowAnchors {
                x: AxisAnchoring::Lo(sib_anchor("gone", AxisSide::Max)),
                y: AxisAnchoring::Free,
            },
        );
        let windows = vec![input("b", rect(300.0, 0.0, 500.0, 100.0))];
        let out = solve_zone_rects(&windows, &anchors, pane, None);
        assert_eq!(out[&key("b")], rect(300.0, 0.0, 500.0, 100.0));
    }

    #[test]
    fn zone_solve_gesture_owner_is_free_and_siblings_track_it() {
        // The dragged window solves as free (its store tracks the drag);
        // a sibling anchored to it follows the dragged rect live.
        let pane = rect(0.0, 0.0, 1000.0, 800.0);
        let mut anchors = HashMap::new();
        anchors.insert(
            key("a"),
            WindowAnchors {
                x: AxisAnchoring::Lo(EdgeAnchor::pane(AxisSide::Min)),
                y: AxisAnchoring::Free,
            },
        );
        anchors.insert(
            key("b"),
            WindowAnchors {
                x: AxisAnchoring::Lo(sib_anchor("a", AxisSide::Max)),
                y: AxisAnchoring::Free,
            },
        );
        // a's free rect is mid-drag at x=400 (store tracks the gesture).
        let windows = vec![
            input("a", rect(400.0, 0.0, 600.0, 100.0)),
            input("b", rect(0.0, 0.0, 200.0, 100.0)),
        ];
        let a = key("a");
        let out = solve_zone_rects(&windows, &anchors, pane, Some(&a));
        assert_eq!(out[&key("a")].min.x, 400.0, "gesture owner stays free");
        assert_eq!(out[&key("b")].min.x, 600.0, "sibling tracks the drag");
    }

    #[test]
    fn both_min_yield_pane_ref_outranks_sibling_ref() {
        // lo = sibling, hi = pane, span below min: the pane edge holds and
        // the sibling edge yields (lo = hi - min).
        let pane = rect(0.0, 0.0, 500.0, 800.0);
        let mut anchors = HashMap::new();
        anchors.insert(
            key("a"),
            WindowAnchors {
                x: AxisAnchoring::Lo(EdgeAnchor::pane(AxisSide::Min)),
                y: AxisAnchoring::Free,
            },
        );
        anchors.insert(
            key("b"),
            WindowAnchors {
                x: AxisAnchoring::Both {
                    lo: sib_anchor("a", AxisSide::Max),
                    hi: EdgeAnchor::pane(AxisSide::Max),
                },
                y: AxisAnchoring::Free,
            },
        );
        // a spans to x=450; b between 450 and pane-right 500 = 50 wide,
        // below the 120 min: the pane edge holds, lo yields to 380.
        let windows = vec![
            input("a", rect(0.0, 0.0, 450.0, 100.0)),
            input("b", rect(450.0, 0.0, 500.0, 100.0)),
        ];
        let out = solve_zone_rects(&windows, &anchors, pane, None);
        assert_eq!(out[&key("b")].max.x, 500.0, "pane edge holds");
        assert_eq!(out[&key("b")].min.x, 380.0, "sibling edge yields to min");
    }

    #[test]
    fn sibling_graph_reaches_walks_transitively() {
        let mut anchors = HashMap::new();
        anchors.insert(
            key("b"),
            WindowAnchors {
                x: AxisAnchoring::Lo(sib_anchor("a", AxisSide::Max)),
                y: AxisAnchoring::Free,
            },
        );
        anchors.insert(
            key("c"),
            WindowAnchors {
                x: AxisAnchoring::Free,
                y: AxisAnchoring::Hi(sib_anchor("b", AxisSide::Min)),
            },
        );
        // c → b → a: promoting "a → c" would close a cycle and must be
        // detectable from a's side.
        assert!(sibling_graph_reaches(&anchors, &key("c"), &key("a")));
        assert!(!sibling_graph_reaches(&anchors, &key("a"), &key("c")));
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
