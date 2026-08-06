//! Layout-snapshot plumbing for the GUI shell: the persisted dock-state
//! snapshot schema, main-window rect tracking, and helpers for reading
//! detached-viewport entries back out of a saved layout.
//!
//! Detached windows themselves are real OS windows managed in `detached.rs`;
//! this module only deals with (de)serializing layout state.

use super::window_manager::WindowAnchors;
use super::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct DockStateSnapshot {
    pub(super) visible_tabs: Vec<TabKey>,
    #[serde(default)]
    pub(super) main_window_rects: Vec<MainWindowRectSnapshot>,
    #[serde(default)]
    pub(super) tab_zones: Vec<TabZoneSnapshot>,
    #[serde(default)]
    pub(super) no_title_tabs: Vec<TabKey>,
    #[serde(default)]
    pub(super) shell_layout: ShellLayoutSnapshot,
    /// Windows locked together, rendered as one window per group.
    #[serde(default)]
    pub(super) tab_groups: Vec<TabGroup>,
    /// Sidebars whose windows are free-placement rects (zone-free-movement
    /// P2). A zone absent here still carries a legacy gap-stack layout and
    /// gets baked into rects on its first render pass; files from before
    /// the conversion deserialize to an empty list.
    #[serde(default)]
    pub(super) free_sidebar_zones: Vec<GuiShellZone>,
    /// Zone preferences for windows that aren't live tabs (hidden / not
    /// yet added), keyed by window name. Never filtered against
    /// available_tabs — the whole point is surviving until the window
    /// materializes.
    #[serde(default)]
    pub(super) pending_zones: Vec<PendingZoneSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MainWindowRectSnapshot {
    pub(super) key: TabKey,
    /// [x, y, width, height] in points
    pub(super) rect: [f32; 4],
    /// Sidebar stacks only: desired empty space above this window, in
    /// points (free vertical placement). Defaults to 0 so layouts saved
    /// before the field existed load unchanged.
    #[serde(default)]
    pub(super) gap_above: f32,
    /// Persisted edge anchors (snap permanence, P-A1). Absent/None = free,
    /// so layouts from before the field load unchanged, and builds from
    /// before it ignore the unknown field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) anchors: Option<WindowAnchors>,
    /// Size role (P-A2/P-A3): `Fixed` exempts this window's width/height
    /// from every proportional rescale (OS resize, zoom, pane squeeze) —
    /// HUD widgets like the compass keep their size while their position
    /// still tracks. Absent = Proportional (legacy-compatible).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) size_role: Option<SizeRole>,
}

/// How a window's SIZE responds to proportional rescales; position always
/// scales. Persisted per window beside its anchors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum SizeRole {
    #[default]
    Proportional,
    Fixed,
}


/// Everything a persisted layout file restores, reconciled against the tabs
/// that actually exist this session. Built by [`VellumGuiApp::restore_layout_state`],
/// consumed by the constructor (startup restore) and `.loadlayout` (runtime
/// apply).
pub(super) struct RestoredLayoutState {
    pub(super) hidden_tabs: HashSet<TabKey>,
    pub(super) main_window_rects: HashMap<TabKey, [f32; 4]>,
    pub(super) window_anchors: HashMap<TabKey, WindowAnchors>,
    pub(super) window_size_roles: HashMap<TabKey, SizeRole>,
    pub(super) sidebar_gap_above: HashMap<TabKey, f32>,
    /// Sidebars already converted to free-placement rects; the others bake
    /// their legacy gap stack on first render (`bake_sidebar_stack`).
    pub(super) migrated_sidebar_zones: HashSet<GuiShellZone>,
    pub(super) tab_zones: HashMap<TabKey, GuiShellZone>,
    /// Window-name-keyed zone prefs for not-yet-live windows.
    pub(super) pending_zones: HashMap<String, GuiShellZone>,
    pub(super) no_title_tabs: HashSet<TabKey>,
    pub(super) shell_layout: ShellLayoutSnapshot,
    pub(super) tab_groups: Vec<TabGroup>,
    pub(super) detached_tabs: HashMap<TabKey, DetachedWindowState>,
    pub(super) ui_font: FontRef,
    pub(super) ui_settings: GuiUiSettings,
    pub(super) tab_settings: HashMap<TabKey, TabSettings>,
    pub(super) main_viewport: Option<MainViewportState>,
}

/// Save order for the main-surface windows: the cached live z-order first
/// (filtered to windows still on the surface), then any surface window the
/// cache hasn't recorded yet, appended in stable alphabetical (lowercased
/// title) order. Front-to-back — topmost last — so `.loadlayout` restores who
/// overlaps whom. `zorder` is the cached back-to-front order; `surface` is
/// every currently visible, non-detached tab paired with its lowercased title.
fn merge_zorder_with_leftover(
    zorder: &[TabKey],
    surface: Vec<(TabKey, String)>,
) -> Vec<TabKey> {
    let on_surface: HashSet<&TabKey> = surface.iter().map(|(key, _)| key).collect();
    let mut ordered: Vec<TabKey> = zorder
        .iter()
        .filter(|key| on_surface.contains(key))
        .cloned()
        .collect();
    let already: HashSet<&TabKey> = ordered.iter().collect();
    let mut leftover: Vec<(TabKey, String)> = surface
        .iter()
        .filter(|(key, _)| !already.contains(key))
        .cloned()
        .collect();
    leftover.sort_by(|a, b| a.1.cmp(&b.1));
    drop(already);
    ordered.extend(leftover.into_iter().map(|(key, _)| key));
    ordered
}

impl VellumGuiApp {
    /// Reconcile a persisted layout against this session's available tabs.
    /// `None` yields the same defaults as a missing layout file. Saved state
    /// referencing tabs that don't exist this session is dropped; tabs the
    /// file doesn't know get their default zone.
    pub(super) fn restore_layout_state(
        persisted_layout: Option<&GuiLayoutFileV1>,
        available_tabs: &HashMap<TabKey, GuiTab>,
    ) -> RestoredLayoutState {
        let ui_font = persisted_layout
            .map(|layout| layout.ui_font.clone())
            .unwrap_or_default();
        let ui_settings = persisted_layout
            .map(|layout| layout.ui_settings.clone())
            .unwrap_or_default();
        let tab_settings = persisted_layout
            .map(|layout| layout.tab_settings_map())
            .unwrap_or_default();
        let main_viewport = persisted_layout.and_then(|layout| layout.main_viewport.clone());

        let mut hidden_tabs: HashSet<TabKey> = persisted_layout
            .map(|layout| layout.hidden_tabs.iter().cloned().collect())
            .unwrap_or_default();
        hidden_tabs.retain(|key| available_tabs.contains_key(key));

        let snapshot = persisted_layout.and_then(Self::dock_snapshot_from_layout);
        let mut main_window_rects = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .main_window_rects
                    .iter()
                    .filter(|entry| available_tabs.contains_key(&entry.key))
                    .map(|entry| (entry.key.clone(), entry.rect))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        main_window_rects.retain(|key, _| available_tabs.contains_key(key));
        // Anchors ride the same per-window entries and the same liveness
        // filter. A sibling ref to a non-live key deliberately survives
        // inside its owner's anchors (it degrades to the free edge until
        // the target reappears).
        let window_anchors: HashMap<TabKey, WindowAnchors> = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .main_window_rects
                    .iter()
                    .filter(|entry| available_tabs.contains_key(&entry.key))
                    .filter_map(|entry| {
                        let anchors = entry.anchors.clone()?;
                        (!anchors.is_free()).then(|| (entry.key.clone(), anchors))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let window_size_roles: HashMap<TabKey, SizeRole> = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .main_window_rects
                    .iter()
                    .filter(|entry| available_tabs.contains_key(&entry.key))
                    .filter_map(|entry| {
                        let role = entry.size_role?;
                        (role == SizeRole::Fixed).then(|| (entry.key.clone(), role))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let sidebar_gap_above: HashMap<TabKey, f32> = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .main_window_rects
                    .iter()
                    .filter(|entry| available_tabs.contains_key(&entry.key))
                    .filter(|entry| entry.gap_above.is_finite() && entry.gap_above > 0.0)
                    .map(|entry| (entry.key.clone(), entry.gap_above))
                    .collect()
            })
            .unwrap_or_default();
        // A missing layout file means there is nothing legacy to bake:
        // fresh sidebars are free-placement from the start.
        let migrated_sidebar_zones: HashSet<GuiShellZone> = match snapshot.as_ref() {
            Some(snapshot) => snapshot.free_sidebar_zones.iter().copied().collect(),
            None => [GuiShellZone::LeftSidebar, GuiShellZone::RightSidebar]
                .into_iter()
                .collect(),
        };
        let mut tab_zones = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .tab_zones
                    .iter()
                    .filter(|entry| available_tabs.contains_key(&entry.key))
                    .map(|entry| (entry.key.clone(), entry.zone))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        tab_zones.retain(|key, _| available_tabs.contains_key(key));
        // Pending zone prefs survive unfiltered — they exist precisely for
        // windows that aren't tabs yet.
        let pending_zones: HashMap<String, GuiShellZone> = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .pending_zones
                    .iter()
                    .map(|entry| (entry.window.clone(), entry.zone))
                    .collect()
            })
            .unwrap_or_default();
        let mut no_title_tabs: HashSet<TabKey> = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .no_title_tabs
                    .iter()
                    .filter(|key| available_tabs.contains_key(*key))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        no_title_tabs.retain(|key| available_tabs.contains_key(key));
        for (key, tab) in available_tabs {
            tab_zones.entry(key.clone()).or_insert_with(|| {
                pending_zones
                    .get(&tab.window_name)
                    .copied()
                    .unwrap_or_else(|| Self::default_zone_for_tab_key(key))
            });
        }
        let mut shell_layout = snapshot
            .as_ref()
            .map(|snapshot| snapshot.shell_layout.clone())
            .unwrap_or_default();
        // Range clamps only: the width-aware sidebar guard runs per frame
        // in the shell pass, against the real window width.
        shell_layout.clamp_ranges();

        let tab_groups = Self::sanitize_tab_groups(
            snapshot
                .as_ref()
                .map(|snapshot| snapshot.tab_groups.clone())
                .unwrap_or_default(),
            available_tabs,
        );

        let detached_tabs: HashMap<TabKey, DetachedWindowState> = persisted_layout
            .map(|layout| {
                Self::detached_viewports_from_layout(layout, available_tabs, &hidden_tabs)
            })
            .unwrap_or_default()
            .into_iter()
            .map(|viewport| {
                let viewport = Self::sanitize_viewport_state(&viewport);
                let key = viewport.tab.clone();
                let state = DetachedWindowState::new(&key, viewport);
                (key, state)
            })
            .collect();

        RestoredLayoutState {
            hidden_tabs,
            main_window_rects,
            window_anchors,
            window_size_roles,
            sidebar_gap_above,
            migrated_sidebar_zones,
            tab_zones,
            pending_zones,
            no_title_tabs,
            shell_layout,
            tab_groups,
            detached_tabs,
            ui_font,
            ui_settings,
            tab_settings,
            main_viewport,
        }
    }

    pub(super) fn dock_snapshot_from_layout(
        layout: &GuiLayoutFileV1,
    ) -> Option<DockStateSnapshot> {
        if layout.dock_state_json.is_null() {
            return None;
        }
        serde_json::from_value(layout.dock_state_json.clone()).ok()
    }

    pub(super) fn rect_to_snapshot(rect: Rect) -> [f32; 4] {
        [rect.min.x, rect.min.y, rect.width(), rect.height()]
    }

    pub(super) fn rect_from_snapshot(raw: [f32; 4]) -> Option<Rect> {
        if !raw.iter().all(|value| value.is_finite()) {
            return None;
        }
        let width = raw[2].max(120.0);
        let height = raw[3].max(MIN_DOCKED_WINDOW_HEIGHT);
        Some(Rect::from_min_size(
            Pos2::new(raw[0], raw[1]),
            Vec2::new(width, height),
        ))
    }

    pub(super) fn clamp_main_window_rect(rect: Rect, bounds: Rect) -> Rect {
        if !rect.is_finite() || !bounds.is_finite() {
            return rect;
        }

        let bounds_w = bounds.width().max(1.0);
        let bounds_h = bounds.height().max(1.0);
        let min_w = 120.0_f32.min(bounds_w);
        let min_h = MIN_DOCKED_WINDOW_HEIGHT.min(bounds_h);
        let width = rect.width().clamp(min_w, bounds_w);
        let height = rect.height().clamp(min_h, bounds_h);
        // Bounds can be narrower than the minimum window size (or inverted,
        // e.g. a center zone squeezed below zero width); f32::clamp panics
        // when min > max, so floor the upper limits at the lower ones.
        let min_x = bounds.left();
        let max_x = (bounds.right() - width).max(min_x);
        let min_y = bounds.top();
        let max_y = (bounds.bottom() - height).max(min_y);
        let x = rect.min.x.clamp(min_x, max_x);
        let y = rect.min.y.clamp(min_y, max_y);
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height))
    }

    /// Scale one stored `[x, y, w, h]` rect from a save-time canvas size to
    /// the current canvas size, per axis. Shared by `.loadlayout`/startup
    /// restore (a layout built on a differently-sized window would otherwise
    /// pin every window at absolute save-time coordinates, leaving dead space
    /// on a larger screen and clipping on a smaller one) and by the explicit
    /// `.resize` command. Degenerate or non-finite sizes yield an identity
    /// scale so the rect passes through untouched.
    pub(super) fn rescale_rect(raw: [f32; 4], from: Vec2, to: Vec2) -> [f32; 4] {
        if !raw.iter().all(|value| value.is_finite())
            || !from.is_finite()
            || !to.is_finite()
            || from.x <= 1.0
            || from.y <= 1.0
            || to.x <= 1.0
            || to.y <= 1.0
        {
            return raw;
        }
        let sx = to.x / from.x;
        let sy = to.y / from.y;
        [raw[0] * sx, raw[1] * sy, raw[2] * sx, raw[3] * sy]
    }

    /// Rescale every stored main-window rect in place from `from` to `to`.
    /// No-op (returns false) when the scale is an identity within epsilon, so
    /// resizing to the same size doesn't churn the layout or mark it dirty.
    pub(super) fn rescale_main_window_rects(
        rects: &mut HashMap<TabKey, [f32; 4]>,
        from: Vec2,
        to: Vec2,
    ) -> bool {
        if !from.is_finite() || !to.is_finite() || from.x <= 1.0 || from.y <= 1.0 {
            return false;
        }
        let sx = to.x / from.x;
        let sy = to.y / from.y;
        if (sx - 1.0).abs() < 0.001 && (sy - 1.0).abs() < 0.001 {
            return false;
        }
        for rect in rects.values_mut() {
            *rect = Self::rescale_rect(*rect, from, to);
        }
        true
    }

    /// Zone- and role-aware store rescale (P-A3 + the zoom-drift fix).
    /// Center windows scale fully proportionally as before. Sidebar
    /// windows keep their width and FOLLOW their owning edge (left: fixed
    /// x; right: translated by the width delta) — the pane they live in
    /// has a user-set fixed width, so scaling their x against the whole
    /// canvas made them drift inside the pane on zoom (Niffy's Ctrl+/-
    /// find). Header/footer windows mirror that on the y axis. A `Fixed`
    /// size role additionally exempts width/height on the proportional
    /// axes. Every rule is affine per axis with zone-constant parameters,
    /// so chains still compose exactly and round-trip losslessly.
    pub(super) fn rescale_main_window_rects_ruled(
        rects: &mut HashMap<TabKey, [f32; 4]>,
        from: Vec2,
        to: Vec2,
        mut rule_of: impl FnMut(&TabKey) -> (GuiShellZone, SizeRole),
    ) -> bool {
        if !from.is_finite() || !to.is_finite() || from.x <= 1.0 || from.y <= 1.0 {
            return false;
        }
        let sx = to.x / from.x;
        let sy = to.y / from.y;
        if (sx - 1.0).abs() < 0.001 && (sy - 1.0).abs() < 0.001 {
            return false;
        }
        for (key, rect) in rects.iter_mut() {
            let (zone, role) = rule_of(key);
            let fixed = role == SizeRole::Fixed;
            let (x, w) = match zone {
                GuiShellZone::LeftSidebar => (rect[0], rect[2]),
                GuiShellZone::RightSidebar => (rect[0] + (to.x - from.x), rect[2]),
                _ => (rect[0] * sx, if fixed { rect[2] } else { rect[2] * sx }),
            };
            let (y, h) = match zone {
                GuiShellZone::Header => (rect[1], rect[3]),
                GuiShellZone::Footer => (rect[1] + (to.y - from.y), rect[3]),
                _ => (rect[1] * sy, if fixed { rect[3] } else { rect[3] * sy }),
            };
            *rect = [x, y, w, h];
        }
        true
    }

    /// P-A3 proportional resolve: map a stored (base-pane space) rect into
    /// the current center pane. Identity when `pane == base`, so with no
    /// reserved zone open nothing moves; a reserved zone opening compresses
    /// only the affected axis. Pure and exactly invertible —
    /// [`Self::unmap_center_rect`] is the inverse used when a display-space
    /// gesture rect is written back into the store.
    pub(super) fn map_center_rect(stored: Rect, base: Rect, pane: Rect, fixed: bool) -> Rect {
        if !stored.is_finite()
            || !base.is_finite()
            || !pane.is_finite()
            || base.width() <= 1.0
            || base.height() <= 1.0
            || pane.width() <= 1.0
            || pane.height() <= 1.0
        {
            return stored;
        }
        let sx = pane.width() / base.width();
        let sy = pane.height() / base.height();
        let x = pane.min.x + (stored.min.x - base.min.x) * sx;
        let y = pane.min.y + (stored.min.y - base.min.y) * sy;
        let (w, h) = if fixed {
            (stored.width(), stored.height())
        } else {
            (stored.width() * sx, stored.height() * sy)
        };
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    /// Exact inverse of [`Self::map_center_rect`] (base and pane swap).
    pub(super) fn unmap_center_rect(display: Rect, base: Rect, pane: Rect, fixed: bool) -> Rect {
        Self::map_center_rect(display, pane, base, fixed)
    }

    /// The store mutation for a queued layout rescale: a pure proportional
    /// rescale from the reference canvas to the live content size. The store
    /// deliberately holds UNCLAMPED values — min sizes and bounds are applied
    /// on the way into egui at every feed site, never written back. Writing
    /// clamped values here was the one lossy step in the resize round trip:
    /// a rect squeezed below its minimum baked the minimum into the store,
    /// so growing the canvas back inflated it past its original size, and
    /// the distortion compounded per squeeze (it was also the compounding
    /// mechanism behind the off-screen drift the 800a38d defensive clamp
    /// papered over). Pure scales compose exactly, so any chain of applies
    /// that returns to a canvas size returns every rect to its value there.
    pub(super) fn apply_layout_rescale(
        rects: &mut HashMap<TabKey, [f32; 4]>,
        from: Vec2,
        content: Rect,
    ) -> bool {
        let content_size = Vec2::new(content.width().max(1.0), content.height().max(1.0));
        Self::rescale_main_window_rects(rects, from, content_size)
    }

    /// Per-frame canvas tracking: rescale the store from its current anchor
    /// to the live content size and re-anchor. Returns whether anything
    /// changed (the caller marks the layout dirty). Rules that keep this
    /// exact under composition:
    /// - A degenerate content rect (minimize, first frames before the OS
    ///   reports real geometry) neither rescales nor moves the anchor, so
    ///   the true reference survives until real geometry returns.
    /// - An identity-within-epsilon rescale keeps the OLD anchor: sub-epsilon
    ///   wobble accumulates against the true reference instead of being
    ///   dropped a fraction at a time.
    /// - `None` (fresh profile, no persisted layout) adopts the first real
    ///   content size without touching rects.
    pub(super) fn track_canvas_anchor(
        anchor: &mut Option<Vec2>,
        rects: &mut HashMap<TabKey, [f32; 4]>,
        content: Rect,
    ) -> bool {
        Self::track_canvas_anchor_ruled(anchor, rects, content, |_| {
            (GuiShellZone::Center, SizeRole::Proportional)
        })
    }

    /// [`Self::track_canvas_anchor`] with per-window zone/role rules (the
    /// live shell passes real rules; the plain variant above keeps the
    /// all-proportional behavior for adopt paths and tests).
    pub(super) fn track_canvas_anchor_ruled(
        anchor: &mut Option<Vec2>,
        rects: &mut HashMap<TabKey, [f32; 4]>,
        content: Rect,
        rule_of: impl FnMut(&TabKey) -> (GuiShellZone, SizeRole),
    ) -> bool {
        let content_size = Vec2::new(content.width().max(1.0), content.height().max(1.0));
        if content_size.x <= 1.0 || content_size.y <= 1.0 {
            return false;
        }
        match *anchor {
            None => {
                *anchor = Some(content_size);
                false
            }
            Some(from) => {
                if Self::rescale_main_window_rects_ruled(rects, from, content_size, rule_of) {
                    *anchor = Some(content_size);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Geometry-only merge from a saved layout (`.resize <name>`): adopt the
    /// saved rect for every window that is live this session, rescaled from
    /// the file's reference canvas into the store's anchor space (a pure
    /// map, so it composes exactly with the per-frame canvas tracking).
    /// Windows only in the file are not materialized; live windows the file
    /// doesn't position keep their current rects. Everything else about the
    /// file — defs, visibility, z-order, skin, OS geometry — is ignored.
    pub(super) fn merge_layout_geometry(
        store: &mut HashMap<TabKey, [f32; 4]>,
        saved: &HashMap<TabKey, [f32; 4]>,
        file_ref: Vec2,
        to: Vec2,
        mut is_live: impl FnMut(&TabKey) -> bool,
    ) -> usize {
        let mut applied = 0;
        for (key, rect) in saved {
            if is_live(key) {
                store.insert(key.clone(), Self::rescale_rect(*rect, file_ref, to));
                applied += 1;
            }
        }
        applied
    }

    pub(super) fn track_main_window_rect(&mut self, key: &TabKey, rect: Rect, bounds: Rect) {
        if !rect.is_finite() || !bounds.is_finite() {
            return;
        }
        let clamped = Self::clamp_main_window_rect(rect, bounds);
        if !clamped.is_finite() {
            return;
        }
        let snapshot = Self::rect_to_snapshot(clamped);
        let changed = self
            .main_window_rects
            .get(key)
            .map(|existing| {
                let dx = (existing[0] - snapshot[0]).abs();
                let dy = (existing[1] - snapshot[1]).abs();
                let dw = (existing[2] - snapshot[2]).abs();
                let dh = (existing[3] - snapshot[3]).abs();
                dx > 0.5 || dy > 0.5 || dw > 0.5 || dh > 0.5
            })
            .unwrap_or(true);
        if changed {
            self.main_window_rects.insert(key.clone(), snapshot);
            self.layout_dirty = true;
        }
    }


    pub(super) fn detached_viewports_from_layout(
        layout: &GuiLayoutFileV1,
        available_tabs: &HashMap<TabKey, GuiTab>,
        hidden_tabs: &HashSet<TabKey>,
    ) -> Vec<ViewportState> {
        let mut entries: Vec<(&String, &ViewportState)> =
            layout.detached_viewports.iter().collect();
        entries.sort_by(|(left, _), (right, _)| left.cmp(right));

        let mut detached = Vec::new();
        let mut seen = HashSet::new();
        for (_, state) in entries {
            if hidden_tabs.contains(&state.tab) || !available_tabs.contains_key(&state.tab) {
                continue;
            }
            if seen.insert(state.tab.clone()) {
                detached.push(state.clone());
            }
        }
        detached
    }

    /// The main-surface windows in save order: true front-to-back stacking
    /// (topmost last), so `.loadlayout` can restore who overlaps whom. The
    /// live z-order is cached each frame from egui's layer order
    /// (`refresh_zorder_cache`); this filters that cache to the currently
    /// visible, non-detached tabs. Before the first frame populates the cache
    /// (or for any visible tab egui hasn't laid out yet) it falls back to an
    /// alphabetical order so a save is never empty.
    pub(super) fn current_main_surface_tab_keys(&self) -> Vec<TabKey> {
        let is_surface = |key: &TabKey| {
            self.available_tabs.contains_key(key)
                && !self.hidden_tabs.contains(key)
                && !self.detached_tabs.contains_key(key)
        };
        let surface: Vec<(TabKey, String)> = self
            .available_tabs
            .iter()
            .filter(|(key, _)| is_surface(key))
            .map(|(key, tab)| (key.clone(), tab.id.title.to_ascii_lowercase()))
            .collect();
        merge_zorder_with_leftover(&self.current_zorder, surface)
    }

    pub(super) fn monitor_bounds_from_ctx(ctx: &egui::Context) -> [f32; 4] {
        ctx.input(|input| {
            if let (Some(outer_rect), Some(monitor_size)) =
                (input.viewport().outer_rect, input.viewport().monitor_size)
            {
                let bounds = [
                    outer_rect.min.x,
                    outer_rect.min.y,
                    monitor_size.x.max(1.0),
                    monitor_size.y.max(1.0),
                ];
                if bounds.iter().all(|value| value.is_finite()) {
                    return bounds;
                }
            }

            let content = input.content_rect();
            let content_bounds = [
                content.min.x,
                content.min.y,
                content.width().max(1.0),
                content.height().max(1.0),
            ];
            if content_bounds.iter().all(|value| value.is_finite()) {
                content_bounds
            } else {
                [0.0, 0.0, 1920.0, 1080.0]
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dock_state_snapshot_round_trip() {
        let snapshot = DockStateSnapshot {
            visible_tabs: vec![TabKey::TextMain, TabKey::Vitals],
            main_window_rects: Vec::new(),
            tab_zones: Vec::new(),
            no_title_tabs: Vec::new(),
            shell_layout: ShellLayoutSnapshot::default(),
            tab_groups: Vec::new(),
            free_sidebar_zones: vec![GuiShellZone::LeftSidebar],
            pending_zones: vec![PendingZoneSnapshot {
                window: "compass".to_string(),
                zone: GuiShellZone::Footer,
            }],
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: DockStateSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.visible_tabs.len(), 2);
        assert_eq!(parsed.visible_tabs[0], TabKey::TextMain);
        assert_eq!(parsed.visible_tabs[1], TabKey::Vitals);
        assert_eq!(parsed.free_sidebar_zones, vec![GuiShellZone::LeftSidebar]);
        assert_eq!(parsed.pending_zones.len(), 1);
        assert_eq!(parsed.pending_zones[0].window, "compass");

        // Files from before the sidebar conversion have no field at all:
        // they deserialize to an empty list, which is what triggers the
        // legacy gap-stack bake.
        let legacy: DockStateSnapshot =
            serde_json::from_str(r#"{"visible_tabs":[]}"#).unwrap();
        assert!(legacy.free_sidebar_zones.is_empty());
    }

    // ── rescale_rect / rescale_main_window_rects (bugs #2, #3, .resize) ──

    #[test]
    fn rescale_rect_scales_per_axis() {
        // Save-time canvas 1280x1024 -> current 1920x1080: a window keeps its
        // relative position and size, filling the larger canvas rather than
        // pinning to the smaller save-time coordinates.
        let from = Vec2::new(1280.0, 1024.0);
        let to = Vec2::new(1920.0, 1080.0);
        let scaled = VellumGuiApp::rescale_rect([640.0, 512.0, 300.0, 200.0], from, to);
        let sx = 1920.0 / 1280.0;
        let sy = 1080.0 / 1024.0;
        assert!((scaled[0] - 640.0 * sx).abs() < 0.01);
        assert!((scaled[1] - 512.0 * sy).abs() < 0.01);
        assert!((scaled[2] - 300.0 * sx).abs() < 0.01);
        assert!((scaled[3] - 200.0 * sy).abs() < 0.01);
    }

    #[test]
    fn rescale_rect_identity_on_degenerate_or_equal_sizes() {
        let raw = [10.0, 20.0, 300.0, 200.0];
        let size = Vec2::new(1000.0, 800.0);
        // Same size in and out: untouched.
        assert_eq!(VellumGuiApp::rescale_rect(raw, size, size), raw);
        // Zero/degenerate from-size: identity, never a divide-by-zero blowup.
        assert_eq!(
            VellumGuiApp::rescale_rect(raw, Vec2::new(0.0, 0.0), size),
            raw
        );
        // Non-finite input rect passes through untouched (NaN != NaN, so
        // compare element-wise rather than with assert_eq! on the array).
        let nan = [f32::NAN, 0.0, 100.0, 100.0];
        let out = VellumGuiApp::rescale_rect(nan, size, size);
        assert!(out[0].is_nan());
        assert_eq!(&out[1..], &[0.0, 100.0, 100.0]);
    }

    #[test]
    fn rescale_main_window_rects_reports_change_and_noops_on_equal() {
        let mut rects = HashMap::new();
        rects.insert(TabKey::Vitals, [100.0, 100.0, 200.0, 150.0]);
        let from = Vec2::new(1000.0, 800.0);

        // Doubling width reports a change and scales x/w.
        let changed =
            VellumGuiApp::rescale_main_window_rects(&mut rects, from, Vec2::new(2000.0, 800.0));
        assert!(changed);
        let r = rects[&TabKey::Vitals];
        assert!((r[0] - 200.0).abs() < 0.01);
        assert!((r[2] - 400.0).abs() < 0.01);
        assert!((r[1] - 100.0).abs() < 0.01, "y unchanged when height equal");

        // Same-size rescale is a no-op.
        let unchanged = VellumGuiApp::rescale_main_window_rects(&mut rects, from, from);
        assert!(!unchanged);
    }

    // ── apply_layout_rescale round-trip fidelity ──
    //
    // Scenario from Nisugi's 2026-08-02 checkpoint chain: a layout squeezed
    // through a short canvas and grown back. The store is pure — min sizes
    // and bounds clamp only at the display feed — so round trips are exact.

    #[test]
    fn apply_layout_rescale_roundtrip_is_exact_through_min_clamp_territory() {
        // 1500x1195 → 2558x664 → 1500x1195. At the short canvas the strip's
        // proportional height (~21.1) is below MIN_DOCKED_WINDOW_HEIGHT; the
        // display feed inflates it on screen, but the store keeps the pure
        // value, so growing back restores the original height exactly
        // (the old clamp write-back landed at ~43.2 instead of 38).
        let mut rects = HashMap::new();
        // command_input analogue: bottom strip, enters min-clamp territory.
        rects.insert(TabKey::Vitals, [1.0, 1153.0, 1498.0, 38.0]);
        // text_main analogue: never in clamp territory.
        rects.insert(TabKey::TextMain, [220.0, 31.0, 909.0, 887.0]);

        let large = Vec2::new(1500.0, 1195.0);
        let short = Rect::from_min_size(Pos2::ZERO, Vec2::new(2558.0, 664.0));
        let back = Rect::from_min_size(Pos2::ZERO, large);

        VellumGuiApp::apply_layout_rescale(&mut rects, large, short);
        let squeezed = rects[&TabKey::Vitals];
        assert!(
            (squeezed[3] - 38.0 * (664.0 / 1195.0)).abs() < 0.01,
            "store keeps the pure proportional height, got {}",
            squeezed[3]
        );
        // The display feed still guards usability at the small canvas.
        let displayed = VellumGuiApp::rect_from_snapshot(squeezed).unwrap();
        assert!(displayed.height() >= 24.0, "feed inflates for display only");

        VellumGuiApp::apply_layout_rescale(&mut rects, short.size(), back);
        for (key, original) in [
            (TabKey::Vitals, [1.0, 1153.0, 1498.0, 38.0]),
            (TabKey::TextMain, [220.0, 31.0, 909.0, 887.0]),
        ] {
            let got = rects[&key];
            for i in 0..4 {
                assert!(
                    (got[i] - original[i]).abs() < 0.01,
                    "{key:?}[{i}] round-trips exactly: {} vs {}",
                    got[i],
                    original[i]
                );
            }
        }
    }

    #[test]
    fn apply_layout_rescale_identity_leaves_store_untouched() {
        // A same-size apply must not rewrite anything — not even a height
        // below the display minimum (the old code min-inflated it here).
        let mut rects = HashMap::new();
        rects.insert(TabKey::Vitals, [10.0, 10.0, 300.0, 18.0]);
        let size = Vec2::new(1500.0, 1195.0);
        let content = Rect::from_min_size(Pos2::ZERO, size);
        let changed = VellumGuiApp::apply_layout_rescale(&mut rects, size, content);
        assert!(!changed, "identity scale reports no change");
        assert_eq!(
            rects[&TabKey::Vitals],
            [10.0, 10.0, 300.0, 18.0],
            "identity apply is a pure no-op"
        );
    }

    // ── track_canvas_anchor: continuous OS-resize tracking ──

    #[test]
    fn canvas_anchor_tracks_continuous_resize_losslessly() {
        // A drag-resize is hundreds of small canvas changes. Walk the
        // content size down to a squashed window, out wide, and back to the
        // start: every rect must return to its original value, because each
        // step is a pure proportional map and pure scales compose exactly.
        let originals = [
            (TabKey::Vitals, [1.0_f32, 1153.0, 1498.0, 38.0]),
            (TabKey::TextMain, [220.0, 31.0, 909.0, 887.0]),
        ];
        let mut rects: HashMap<_, _> = originals.iter().cloned().collect();
        let mut anchor = Some(Vec2::new(1500.0, 1195.0));

        let mut sizes = Vec::new();
        for i in 1..=40 {
            // shrink toward 700x400
            let t = i as f32 / 40.0;
            sizes.push(Vec2::new(1500.0 - 800.0 * t, 1195.0 - 795.0 * t));
        }
        for i in 1..=40 {
            // grow toward the wide curved-monitor canvas
            let t = i as f32 / 40.0;
            sizes.push(Vec2::new(700.0 + 1858.0 * t, 400.0 + 960.0 * t));
        }
        for i in 1..=40 {
            // and back to the start
            let t = i as f32 / 40.0;
            sizes.push(Vec2::new(2558.0 - 1058.0 * t, 1360.0 - 165.0 * t));
        }
        for size in sizes {
            VellumGuiApp::track_canvas_anchor(
                &mut anchor,
                &mut rects,
                Rect::from_min_size(Pos2::ZERO, size),
            );
        }

        assert_eq!(anchor, Some(Vec2::new(1500.0, 1195.0)));
        for (key, original) in originals {
            let got = rects[&key];
            for i in 0..4 {
                assert!(
                    (got[i] - original[i]).abs() < 0.05,
                    "{key:?}[{i}] after 120 resize steps: {} vs {}",
                    got[i],
                    original[i]
                );
            }
        }
    }

    #[test]
    fn ruled_rescale_sidebars_follow_their_edge_and_keep_width() {
        // The zoom-drift fix: a right-sidebar window translates with the
        // right edge (width delta), never scales x; left stays put. Fixed
        // windows keep w/h on proportional axes. Round trip is exact.
        let mut rects: HashMap<TabKey, [f32; 4]> = HashMap::new();
        rects.insert(TabKey::Vitals, [1700.0, 100.0, 280.0, 200.0]); // right bar
        rects.insert(TabKey::Targets, [10.0, 400.0, 280.0, 200.0]); // left bar
        rects.insert(TabKey::TextMain, [500.0, 100.0, 800.0, 600.0]); // center
        rects.insert(TabKey::Compass, [900.0, 700.0, 120.0, 120.0]); // center FIXED
        let original = rects.clone();
        let rule = |key: &TabKey| match key {
            TabKey::Vitals => (GuiShellZone::RightSidebar, SizeRole::Proportional),
            TabKey::Targets => (GuiShellZone::LeftSidebar, SizeRole::Proportional),
            TabKey::Compass => (GuiShellZone::Center, SizeRole::Fixed),
            _ => (GuiShellZone::Center, SizeRole::Proportional),
        };
        let from = Vec2::new(2000.0, 1000.0);
        let to = Vec2::new(2600.0, 1400.0);
        assert!(VellumGuiApp::rescale_main_window_rects_ruled(&mut rects, from, to, rule));

        let right = rects[&TabKey::Vitals];
        assert_eq!(right[0], 1700.0 + 600.0, "follows the right edge");
        assert_eq!(right[2], 280.0, "sidebar width never scales");
        assert!((right[1] - 100.0 * 1.4).abs() < 0.01, "y still proportional");
        let left = rects[&TabKey::Targets];
        assert_eq!(left[0], 10.0, "left sidebar x pinned");
        assert_eq!(left[2], 280.0);
        let fixed = rects[&TabKey::Compass];
        assert_eq!((fixed[2], fixed[3]), (120.0, 120.0), "Fixed keeps size");
        assert!((fixed[0] - 900.0 * 1.3).abs() < 0.01, "Fixed position scales");

        // Exact round trip back to the original canvas.
        assert!(VellumGuiApp::rescale_main_window_rects_ruled(&mut rects, to, from, rule));
        for (key, rect) in &original {
            for i in 0..4 {
                assert!(
                    (rects[key][i] - rect[i]).abs() < 0.01,
                    "{key:?}[{i}] {} vs {}",
                    rects[key][i],
                    rect[i]
                );
            }
        }
    }

    #[test]
    fn center_map_identity_compression_and_exact_inverse() {
        let base = Rect::from_min_max(Pos2::new(0.0, 30.0), Pos2::new(2000.0, 1000.0));
        let stored = Rect::from_min_max(Pos2::new(1600.0, 100.0), Pos2::new(1900.0, 500.0));
        // No reserved zones: pane == base → identity.
        assert_eq!(
            VellumGuiApp::map_center_rect(stored, base, base, false),
            stored
        );
        // A 300px right sidebar reserves space: x compresses, y untouched.
        let pane = Rect::from_min_max(Pos2::new(0.0, 30.0), Pos2::new(1700.0, 1000.0));
        let mapped = VellumGuiApp::map_center_rect(stored, base, pane, false);
        assert!(mapped.max.x <= pane.max.x + 0.01, "stays inside the pane");
        assert!((mapped.min.y - stored.min.y).abs() < 0.01, "y identity");
        assert!(mapped.width() < stored.width(), "x compressed");
        // Exact inverse: unmap(map(r)) == r.
        let back = VellumGuiApp::unmap_center_rect(mapped, base, pane, false);
        assert!((back.min.x - stored.min.x).abs() < 0.01);
        assert!((back.width() - stored.width()).abs() < 0.01);
        // Fixed keeps size through the map AND the inverse.
        let mapped_fixed = VellumGuiApp::map_center_rect(stored, base, pane, true);
        assert_eq!(mapped_fixed.width(), stored.width());
        let back_fixed = VellumGuiApp::unmap_center_rect(mapped_fixed, base, pane, true);
        assert!((back_fixed.min.x - stored.min.x).abs() < 0.01);
        assert_eq!(back_fixed.width(), stored.width());
    }

    #[test]
    fn canvas_anchor_ignores_degenerate_content_and_survives_minimize() {
        let mut rects = HashMap::new();
        rects.insert(TabKey::Vitals, [100.0, 100.0, 400.0, 300.0]);
        let mut anchor = Some(Vec2::new(1500.0, 1195.0));
        // Minimize: content collapses; nothing moves, anchor survives.
        let changed = VellumGuiApp::track_canvas_anchor(
            &mut anchor,
            &mut rects,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(0.0, 0.0)),
        );
        assert!(!changed);
        assert_eq!(anchor, Some(Vec2::new(1500.0, 1195.0)));
        assert_eq!(rects[&TabKey::Vitals], [100.0, 100.0, 400.0, 300.0]);
        // Restore at a different size: one exact map from the true anchor.
        VellumGuiApp::track_canvas_anchor(
            &mut anchor,
            &mut rects,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(3000.0, 1195.0)),
        );
        let r = rects[&TabKey::Vitals];
        assert!((r[0] - 200.0).abs() < 0.01 && (r[2] - 800.0).abs() < 0.01);
    }

    #[test]
    fn canvas_anchor_first_frame_adopts_content_without_touching_rects() {
        let mut rects = HashMap::new();
        rects.insert(TabKey::Vitals, [100.0, 100.0, 400.0, 300.0]);
        let mut anchor = None;
        let changed = VellumGuiApp::track_canvas_anchor(
            &mut anchor,
            &mut rects,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1500.0, 1195.0)),
        );
        assert!(!changed);
        assert_eq!(anchor, Some(Vec2::new(1500.0, 1195.0)));
        assert_eq!(rects[&TabKey::Vitals], [100.0, 100.0, 400.0, 300.0]);
    }

    #[test]
    fn canvas_anchor_subepsilon_wobble_accumulates_against_true_reference() {
        // ±1px wobble at 1500 wide is inside the 0.1% identity epsilon: no
        // rescale applies and the anchor must NOT advance, so a later real
        // resize maps from the true 1500 reference, not a drifted one.
        let mut rects = HashMap::new();
        rects.insert(TabKey::Vitals, [100.0, 100.0, 400.0, 300.0]);
        let mut anchor = Some(Vec2::new(1500.0, 1195.0));
        for size in [
            Vec2::new(1501.0, 1195.0),
            Vec2::new(1499.0, 1194.0),
            Vec2::new(1500.5, 1195.5),
        ] {
            let changed = VellumGuiApp::track_canvas_anchor(
                &mut anchor,
                &mut rects,
                Rect::from_min_size(Pos2::ZERO, size),
            );
            assert!(!changed, "wobble {size:?} must not apply");
            assert_eq!(anchor, Some(Vec2::new(1500.0, 1195.0)));
        }
        VellumGuiApp::track_canvas_anchor(
            &mut anchor,
            &mut rects,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(3000.0, 1195.0)),
        );
        let r = rects[&TabKey::Vitals];
        assert!(
            (r[0] - 200.0).abs() < 0.01,
            "scale comes from the true anchor, got x={}",
            r[0]
        );
    }

    #[test]
    fn merge_layout_geometry_intersects_and_rescales() {
        // `.resize <name>`: live ∩ saved windows adopt the saved geometry
        // rescaled from the file's canvas into the current anchor space;
        // saved-only windows are not materialized; live-only windows keep
        // their rects.
        let mut store = HashMap::new();
        store.insert(TabKey::Vitals, [10.0, 10.0, 100.0, 100.0]);
        store.insert(TabKey::TextMain, [500.0, 500.0, 400.0, 300.0]);

        let mut saved = HashMap::new();
        saved.insert(TabKey::Vitals, [100.0, 200.0, 300.0, 400.0]);
        saved.insert(TabKey::CommandInput, [0.0, 900.0, 1000.0, 50.0]); // not live

        let file_ref = Vec2::new(1000.0, 1000.0);
        let to = Vec2::new(2000.0, 500.0);
        let live = [TabKey::Vitals, TabKey::TextMain];
        let applied = VellumGuiApp::merge_layout_geometry(
            &mut store,
            &saved,
            file_ref,
            to,
            |key| live.contains(key),
        );

        assert_eq!(applied, 1, "only the live ∩ saved window is adopted");
        assert_eq!(
            store[&TabKey::Vitals],
            [200.0, 100.0, 600.0, 200.0],
            "adopted rect is rescaled file→anchor (2x, 0.5x)"
        );
        assert_eq!(
            store[&TabKey::TextMain],
            [500.0, 500.0, 400.0, 300.0],
            "live window absent from the file keeps its rect"
        );
        assert!(
            !store.contains_key(&TabKey::CommandInput),
            "saved-only window is not materialized"
        );
    }

    #[test]
    fn reanchoring_to_bbox_makes_next_frame_fill_the_canvas() {
        // Bare `.resize` sets the anchor to the rects' bounding box; the
        // next frame's tracking then stretches the arrangement out to the
        // full canvas, absorbing dead space.
        let mut rects = HashMap::new();
        rects.insert(TabKey::Vitals, [0.0, 0.0, 500.0, 300.0]);
        rects.insert(TabKey::TextMain, [500.0, 300.0, 500.0, 500.0]);
        // Arrangement occupies 1000x800 inside a larger canvas.
        let mut anchor = Some(Vec2::new(1000.0, 800.0));
        VellumGuiApp::track_canvas_anchor(
            &mut anchor,
            &mut rects,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1500.0, 1200.0)),
        );
        let main = rects[&TabKey::TextMain];
        assert!(
            (main[0] + main[2] - 1500.0).abs() < 0.01
                && (main[1] + main[3] - 1200.0).abs() < 0.01,
            "arrangement stretches to the full canvas, got {main:?}"
        );
    }

    #[test]
    fn apply_layout_rescale_offscreen_rect_stays_in_store_but_displays_clamped() {
        // A rect anchored past the canvas (legacy file, detached monitor)
        // stays where the user left it in the store; the feed clamp brings
        // it on screen for display without rewriting it.
        let mut rects = HashMap::new();
        rects.insert(TabKey::Vitals, [1400.0, 1100.0, 300.0, 200.0]);
        let size = Vec2::new(1500.0, 1195.0);
        let content = Rect::from_min_size(Pos2::ZERO, size);
        VellumGuiApp::apply_layout_rescale(&mut rects, size, content);
        assert_eq!(rects[&TabKey::Vitals], [1400.0, 1100.0, 300.0, 200.0]);
        let displayed = VellumGuiApp::clamp_main_window_rect(
            VellumGuiApp::rect_from_snapshot(rects[&TabKey::Vitals]).unwrap(),
            content,
        );
        assert!(displayed.max.x <= 1500.0 + 0.01 && displayed.max.y <= 1195.0 + 0.01);
    }

    #[test]
    fn restore_keeps_rect_for_hidden_tab() {
        // Bug #4: a window hidden at save time must still restore its saved
        // rect, so showing it later from the Windows menu places it where it
        // was left instead of falling back to the top-left default.
        let mut available_tabs = HashMap::new();
        available_tabs.insert(
            TabKey::Vitals,
            GuiTab {
                id: TabId::new(TabKey::Vitals),
                window_name: "vitals".to_string(),
            },
        );

        let snapshot = DockStateSnapshot {
            visible_tabs: Vec::new(),
            main_window_rects: vec![MainWindowRectSnapshot {
                key: TabKey::Vitals,
                rect: [250.0, 300.0, 400.0, 220.0],
                gap_above: 0.0,
                anchors: None,
                size_role: None,
            }],
            tab_zones: Vec::new(),
            no_title_tabs: Vec::new(),
            shell_layout: ShellLayoutSnapshot::default(),
            tab_groups: Vec::new(),
            free_sidebar_zones: Vec::new(),
            pending_zones: Vec::new(),
        };
        let mut layout = GuiLayoutFileV1::new("profile", "character");
        layout.hidden_tabs = vec![TabKey::Vitals];
        layout.dock_state_json = serde_json::to_value(snapshot).unwrap();

        let restored = VellumGuiApp::restore_layout_state(Some(&layout), &available_tabs);

        assert!(
            restored.hidden_tabs.contains(&TabKey::Vitals),
            "tab stays hidden"
        );
        assert_eq!(
            restored.main_window_rects.get(&TabKey::Vitals).copied(),
            Some([250.0, 300.0, 400.0, 220.0]),
            "hidden tab keeps its saved rect for a later show"
        );
    }

    #[test]
    fn test_detached_viewports_from_layout_filters_invalid_entries() {
        let mut available_tabs = HashMap::new();
        available_tabs.insert(
            TabKey::Vitals,
            GuiTab {
                id: TabId::new(TabKey::Vitals),
                window_name: "vitals".to_string(),
            },
        );
        available_tabs.insert(
            TabKey::Room,
            GuiTab {
                id: TabId::new(TabKey::Room),
                window_name: "room".to_string(),
            },
        );

        let mut layout = GuiLayoutFileV1::new("profile", "character");
        layout.detached_viewports.insert(
            "b_vitals".to_string(),
            ViewportState::new(TabKey::Vitals, [100.0, 100.0], [400.0, 300.0]),
        );
        layout.detached_viewports.insert(
            "a_vitals".to_string(),
            ViewportState::new(TabKey::Vitals, [200.0, 200.0], [500.0, 400.0]),
        );
        layout.detached_viewports.insert(
            "room_hidden".to_string(),
            ViewportState::new(TabKey::Room, [100.0, 100.0], [400.0, 300.0]),
        );
        layout.detached_viewports.insert(
            "missing_tab".to_string(),
            ViewportState::new(TabKey::Compass, [100.0, 100.0], [400.0, 300.0]),
        );

        let hidden_tabs = HashSet::from([TabKey::Room]);
        let detached =
            VellumGuiApp::detached_viewports_from_layout(&layout, &available_tabs, &hidden_tabs);

        assert_eq!(detached.len(), 1);
        assert_eq!(detached[0].tab, TabKey::Vitals);
        assert_eq!(detached[0].outer_pos_px, [200.0, 200.0]);
    }

    // ── P-A0 pins: snap-permanence characterization ─────────────────────
    //
    // Workstream P-A (.beads/artifacts/window-system-redesign/spec.md):
    // these pin TODAY's dock-is-just-pixels behavior so the P-A1 diff that
    // introduces persisted edge anchors provably changes it.

    #[test]
    fn pa0_snapshot_persists_raw_pixels_and_no_relationship() {
        // Schema pin, updated by P-A1 as designed: a FREE window's persisted
        // geometry is still exactly { key, rect, gap_above } — anchors are
        // skip-if-none, so layouts full of free windows are byte-identical
        // to pre-anchor files (and old builds reading them see nothing new).
        let snapshot = MainWindowRectSnapshot {
            key: TabKey::Vitals,
            rect: [10.0, 20.0, 300.0, 200.0],
            gap_above: 0.0,
            anchors: None,
            size_role: None,
        };
        let value = serde_json::to_value(&snapshot).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["gap_above", "key", "rect"]);

        // Forward tolerance: a snapshot written by a build that knows the
        // NEXT field (P-A2's size_role) still loads here — serde_json
        // ignores unknown fields, so shipping a new optional field can
        // never brick an older install.
        let mut with_future_field = value.as_object().unwrap().clone();
        with_future_field.insert("size_role".to_string(), serde_json::json!("fixed"));
        let parsed: MainWindowRectSnapshot =
            serde_json::from_value(serde_json::Value::Object(with_future_field)).unwrap();
        assert_eq!(parsed.rect, [10.0, 20.0, 300.0, 200.0]);
        assert_eq!(parsed.gap_above, 0.0);
        assert_eq!(parsed.anchors, None);
    }

    #[test]
    fn restore_filters_anchors_to_live_tabs_and_drops_free() {
        use super::super::window_manager::{AxisAnchoring, AxisSide, EdgeAnchor};
        let mut available_tabs = HashMap::new();
        available_tabs.insert(
            TabKey::Vitals,
            GuiTab {
                id: TabId::new(TabKey::Vitals),
                window_name: "vitals".to_string(),
            },
        );
        let anchored = |key: TabKey, anchors: Option<WindowAnchors>| MainWindowRectSnapshot {
            key,
            rect: [10.0, 10.0, 200.0, 200.0],
            gap_above: 0.0,
            anchors,
            size_role: None,
        };
        let snapshot = DockStateSnapshot {
            main_window_rects: vec![
                // Live tab with a real anchor: restored.
                anchored(
                    TabKey::Vitals,
                    Some(WindowAnchors {
                        x: AxisAnchoring::Hi(EdgeAnchor::pane(AxisSide::Max)),
                        y: AxisAnchoring::Free,
                    }),
                ),
                // Tab not live this session: dropped like its rect.
                anchored(
                    TabKey::Compass,
                    Some(WindowAnchors {
                        x: AxisAnchoring::Lo(EdgeAnchor::pane(AxisSide::Min)),
                        y: AxisAnchoring::Free,
                    }),
                ),
            ],
            ..Default::default()
        };
        let mut layout = GuiLayoutFileV1::new("profile", "character");
        layout.dock_state_json = serde_json::to_value(snapshot).unwrap();

        let restored = VellumGuiApp::restore_layout_state(Some(&layout), &available_tabs);
        assert_eq!(restored.window_anchors.len(), 1);
        assert!(restored.window_anchors.contains_key(&TabKey::Vitals));
    }

    #[test]
    fn snapshot_anchor_round_trip_and_legacy_files_load_free() {
        // A right-docked window's anchors survive save/load; a legacy
        // entry (no anchors field at all) deserializes to None.
        use super::super::window_manager::{AxisAnchoring, AxisSide, EdgeAnchor};
        let snapshot = MainWindowRectSnapshot {
            key: TabKey::Vitals,
            rect: [800.0, 300.0, 200.0, 200.0],
            gap_above: 0.0,
            anchors: Some(WindowAnchors {
                x: AxisAnchoring::Hi(EdgeAnchor::pane(AxisSide::Max)),
                y: AxisAnchoring::Free,
            }),
            size_role: None,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let back: MainWindowRectSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.anchors, snapshot.anchors);

        let legacy: MainWindowRectSnapshot = serde_json::from_value(serde_json::json!({
            "key": back.key,
            "rect": [1.0, 2.0, 3.0, 4.0],
        }))
        .unwrap();
        assert_eq!(legacy.anchors, None);
        assert_eq!(legacy.gap_above, 0.0);
    }
}
