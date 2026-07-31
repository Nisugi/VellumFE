//! Layout-snapshot plumbing for the GUI shell: the persisted dock-state
//! snapshot schema, main-window rect tracking, and helpers for reading
//! detached-viewport entries back out of a saved layout.
//!
//! Detached windows themselves are real OS windows managed in `detached.rs`;
//! this module only deals with (de)serializing layout state.

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
}

/// One Center-zone window's inputs to [`VellumGuiApp::compute_center_display_rects`].
#[derive(Clone, Debug)]
pub(super) struct CenterWindowInfo {
    pub(super) key: TabKey,
    /// Canonical user-intended rect; never mutated by shell-zone toggles.
    pub(super) stored: Rect,
    /// Smallest size the main window may shrink to (group-aware).
    pub(super) min_size: Vec2,
    /// Hosts the main story stream: absorbs displacement by shrinking.
    pub(super) is_main: bool,
}

/// Everything a persisted layout file restores, reconciled against the tabs
/// that actually exist this session. Built by [`VellumGuiApp::restore_layout_state`],
/// consumed by the constructor (startup restore) and `.loadlayout` (runtime
/// apply).
pub(super) struct RestoredLayoutState {
    pub(super) hidden_tabs: HashSet<TabKey>,
    pub(super) main_window_rects: HashMap<TabKey, [f32; 4]>,
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

    /// Displacement propagation for freely placed Center windows when a
    /// shell zone (header/footer/sidebar) claims part of the center area.
    ///
    /// `stored` rects are canonical and never mutated by zone toggles; this
    /// computes per-frame *display* rects purely from (stored, bounds), so
    /// closing a zone restores every window automatically. Per edge, windows
    /// displaced over other windows push them along (only where the stored
    /// rects did NOT already overlap — deliberate overlap is preserved),
    /// and the main story window absorbs the push by shrinking from the
    /// encroached edge (opposite edge fixed) down to its minimum size;
    /// beyond that it translates and the push continues past it.
    pub(super) fn compute_center_display_rects(
        windows: &[CenterWindowInfo],
        bounds: Rect,
    ) -> HashMap<TabKey, Rect> {
        const EPS: f32 = 0.5;

        // Work in [min.x, min.y, max.x, max.y] form so one pass routine
        // serves both axes. `display` starts at stored and only ever moves
        // away from the encroaching edge.
        let n = windows.len();
        let stored: Vec<[f32; 4]> = windows
            .iter()
            .map(|w| [w.stored.min.x, w.stored.min.y, w.stored.max.x, w.stored.max.y])
            .collect();
        let mut display = stored.clone();

        // True when two 1D intervals genuinely share space (touching edges
        // don't count).
        let overlaps =
            |a0: f32, a1: f32, b0: f32, b1: f32| -> bool { a1.min(b1) - a0.max(b0) > EPS };

        // One directional pass along `axis` (0 = x, 1 = y), pushing away
        // from the bounds' min edge (`from_start`) or max edge.
        let mut pass = |display: &mut Vec<[f32; 4]>, axis: usize, from_start: bool| {
            let cross = 1 - axis;
            let (lo, hi) = (axis, axis + 2);
            let (clo, chi) = (cross, cross + 2);
            let edge = if from_start {
                [bounds.min.x, bounds.min.y][axis]
            } else {
                [bounds.max.x, bounds.max.y][axis]
            };
            // Sweep outward from the moving edge so every window that can
            // push the current one already has its final display position.
            let mut order: Vec<usize> = (0..n).collect();
            if from_start {
                order.sort_by(|&a, &b| stored[a][lo].total_cmp(&stored[b][lo]));
            } else {
                order.sort_by(|&a, &b| stored[b][hi].total_cmp(&stored[a][hi]));
            }
            for &i in &order {
                let mut required = edge;
                for &j in &order {
                    if j == i {
                        continue;
                    }
                    // Only windows strictly on the edge side of `i` in
                    // STORED coords may push it — displacement must never
                    // separate a deliberately overlapped pair.
                    let j_on_edge_side = if from_start {
                        stored[j][hi] <= stored[i][lo] + EPS
                    } else {
                        stored[j][lo] >= stored[i][hi] - EPS
                    };
                    if !j_on_edge_side
                        || !overlaps(display[j][clo], display[j][chi], display[i][clo], display[i][chi])
                    {
                        continue;
                    }
                    if from_start {
                        required = required.max(display[j][hi]);
                    } else {
                        required = required.min(display[j][lo]);
                    }
                }
                let min_extent = [windows[i].min_size.x, windows[i].min_size.y][axis];
                if from_start {
                    if required <= display[i][lo] + EPS {
                        continue;
                    }
                    if windows[i].is_main {
                        // Absorb: near edge moves in, far edge fixed, floored
                        // at min size (or the current extent if already
                        // smaller); any leftover translates the window.
                        let extent = display[i][hi] - display[i][lo];
                        let min_extent = min_extent.min(extent);
                        let max_lo = display[i][hi] - min_extent;
                        let leftover = (required - max_lo).max(0.0);
                        display[i][lo] = required.min(max_lo) + leftover;
                        display[i][hi] += leftover;
                    } else {
                        let delta = required - display[i][lo];
                        display[i][lo] += delta;
                        display[i][hi] += delta;
                    }
                } else {
                    if required >= display[i][hi] - EPS {
                        continue;
                    }
                    if windows[i].is_main {
                        let extent = display[i][hi] - display[i][lo];
                        let min_extent = min_extent.min(extent);
                        let min_hi = display[i][lo] + min_extent;
                        let leftover = (min_hi - required).max(0.0);
                        display[i][hi] = required.max(min_hi) - leftover;
                        display[i][lo] -= leftover;
                    } else {
                        let delta = display[i][hi] - required;
                        display[i][lo] -= delta;
                        display[i][hi] -= delta;
                    }
                }
            }
        };

        // Vertical passes first (header/footer are the common toggles), then
        // horizontal (sidebars); later passes read the earlier results so
        // combined toggles compose.
        pass(&mut display, 1, true);
        pass(&mut display, 1, false);
        pass(&mut display, 0, true);
        pass(&mut display, 0, false);

        windows
            .iter()
            .zip(display)
            .map(|(w, d)| {
                let rect =
                    Rect::from_min_max(Pos2::new(d[0], d[1]), Pos2::new(d[2], d[3]));
                // Final safety net: whatever the cascade produced stays
                // on-screen at a usable size (also the fallback for windows
                // buried entirely inside a claimed strip).
                (w.key.clone(), Self::clamp_main_window_rect(rect, bounds))
            })
            .collect()
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

    pub(super) fn current_main_surface_tab_keys(&self) -> Vec<TabKey> {
        let mut visible: Vec<(String, TabKey)> = self
            .available_tabs
            .iter()
            .filter_map(|(key, tab)| {
                if self.hidden_tabs.contains(key) || self.detached_tabs.contains_key(key) {
                    None
                } else {
                    Some((tab.id.title.clone(), key.clone()))
                }
            })
            .collect();
        visible.sort_by_key(|(title, _)| title.to_ascii_lowercase());
        visible.into_iter().map(|(_, key)| key).collect()
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

    // ── compute_center_display_rects ────────────────────────────────────

    fn info(id: &str, stored: Rect, is_main: bool) -> CenterWindowInfo {
        CenterWindowInfo {
            key: if is_main {
                TabKey::TextMain
            } else {
                TabKey::TextByName { id: id.to_string() }
            },
            stored,
            min_size: Vec2::new(120.0, 90.0),
            is_main,
        }
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    }

    fn key(id: &str) -> TabKey {
        TabKey::TextByName { id: id.to_string() }
    }

    #[test]
    fn display_rects_identity_when_inside_bounds() {
        let bounds = rect(0.0, 0.0, 1000.0, 800.0);
        let windows = vec![
            info("a", rect(10.0, 10.0, 300.0, 200.0), false),
            info("main", rect(320.0, 10.0, 900.0, 700.0), true),
        ];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&key("a")], rect(10.0, 10.0, 300.0, 200.0));
        assert_eq!(out[&TabKey::TextMain], rect(320.0, 10.0, 900.0, 700.0));
    }

    #[test]
    fn main_shrinks_from_top_bottom_fixed() {
        // Header open: bounds start 100px lower than the stored rect.
        let bounds = rect(0.0, 100.0, 1000.0, 800.0);
        let windows = vec![info("main", rect(0.0, 0.0, 600.0, 500.0), true)];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&TabKey::TextMain], rect(0.0, 100.0, 600.0, 500.0));
    }

    #[test]
    fn main_shrinks_from_bottom_top_fixed() {
        // Footer open: bounds end 100px above the stored rect.
        let bounds = rect(0.0, 0.0, 1000.0, 400.0);
        let windows = vec![info("main", rect(0.0, 0.0, 600.0, 500.0), true)];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&TabKey::TextMain], rect(0.0, 0.0, 600.0, 400.0));
    }

    #[test]
    fn non_main_pushes_and_story_absorbs() {
        // Room window at the very top, story right below it. Opening the
        // header pushes the room down un-shrunk; the story absorbs the
        // whole delta by shrinking from the top.
        let bounds = rect(0.0, 100.0, 1000.0, 800.0);
        let windows = vec![
            info("room", rect(0.0, 0.0, 300.0, 80.0), false),
            info("main", rect(0.0, 80.0, 600.0, 500.0), true),
        ];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&key("room")], rect(0.0, 100.0, 300.0, 180.0));
        assert_eq!(out[&TabKey::TextMain], rect(0.0, 180.0, 600.0, 500.0));
    }

    #[test]
    fn push_chain_propagates_to_story() {
        let bounds = rect(0.0, 100.0, 1000.0, 800.0);
        let windows = vec![
            info("a", rect(0.0, 0.0, 300.0, 60.0), false),
            info("b", rect(0.0, 60.0, 300.0, 140.0), false),
            info("main", rect(0.0, 140.0, 600.0, 600.0), true),
        ];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&key("a")], rect(0.0, 100.0, 300.0, 160.0));
        assert_eq!(out[&key("b")], rect(0.0, 160.0, 300.0, 240.0));
        assert_eq!(out[&TabKey::TextMain], rect(0.0, 240.0, 600.0, 600.0));
    }

    #[test]
    fn sidebar_pushes_horizontally_story_absorbs_from_left() {
        let bounds = rect(150.0, 0.0, 1000.0, 800.0);
        let windows = vec![
            info("w", rect(0.0, 0.0, 140.0, 300.0), false),
            info("main", rect(140.0, 0.0, 700.0, 500.0), true),
        ];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&key("w")], rect(150.0, 0.0, 290.0, 300.0));
        assert_eq!(out[&TabKey::TextMain], rect(290.0, 0.0, 700.0, 500.0));
    }

    #[test]
    fn header_and_sidebar_compose() {
        let bounds = rect(100.0, 50.0, 1000.0, 800.0);
        let windows = vec![info("main", rect(0.0, 0.0, 600.0, 500.0), true)];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&TabKey::TextMain], rect(100.0, 50.0, 600.0, 500.0));
    }

    #[test]
    fn deliberate_overlap_is_preserved() {
        // A and B overlap by 50px in stored coords (user placed them so);
        // pushing A down must not push B.
        let bounds = rect(0.0, 100.0, 1000.0, 800.0);
        let windows = vec![
            info("a", rect(0.0, 0.0, 300.0, 200.0), false),
            info("b", rect(0.0, 150.0, 300.0, 400.0), false),
        ];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&key("a")], rect(0.0, 100.0, 300.0, 300.0));
        assert_eq!(out[&key("b")], rect(0.0, 150.0, 300.0, 400.0));
    }

    #[test]
    fn no_main_everything_pushes() {
        let bounds = rect(0.0, 100.0, 1000.0, 800.0);
        let windows = vec![info("a", rect(0.0, 20.0, 300.0, 220.0), false)];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&key("a")], rect(0.0, 100.0, 300.0, 300.0));
    }

    #[test]
    fn main_at_floor_translates_and_keeps_pushing() {
        // Header claims so much that the story hits its 90px floor; the
        // remainder translates it, and the window below is pushed on.
        let bounds = rect(0.0, 160.0, 1000.0, 800.0);
        let windows = vec![
            info("main", rect(0.0, 50.0, 300.0, 200.0), true),
            info("c", rect(0.0, 200.0, 300.0, 300.0), false),
        ];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&TabKey::TextMain], rect(0.0, 160.0, 300.0, 250.0));
        assert_eq!(out[&key("c")], rect(0.0, 250.0, 300.0, 350.0));
    }

    #[test]
    fn group_sized_floor_respected() {
        // A grouped main window carries a larger minimum (90 × members).
        let bounds = rect(0.0, 300.0, 1000.0, 800.0);
        let windows = vec![CenterWindowInfo {
            key: TabKey::TextMain,
            stored: rect(0.0, 0.0, 600.0, 400.0),
            min_size: Vec2::new(120.0, 270.0),
            is_main: true,
        }];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        // Shrinks only to 270 tall (bottom fixed would give 100), so the
        // leftover translates it down: [130..400] shifted to [300..570].
        assert_eq!(out[&TabKey::TextMain], rect(0.0, 300.0, 600.0, 570.0));
    }

    #[test]
    fn window_fully_inside_header_strip_falls_back_to_clamp() {
        let bounds = rect(0.0, 200.0, 1000.0, 800.0);
        let windows = vec![info("a", rect(0.0, 0.0, 300.0, 150.0), false)];
        let out = VellumGuiApp::compute_center_display_rects(&windows, bounds);
        assert_eq!(out[&key("a")], rect(0.0, 200.0, 300.0, 350.0));
    }
}
