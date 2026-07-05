//! egui_dock integration and detached-viewport management for the GUI.
//!
//! Pure-move extraction from `app.rs`: the dock-state snapshot schema,
//! detached window viewports (restore, sanitize, save), main-window rect
//! tracking, and the `TabViewer` used to render detached windows.

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MainWindowRectSnapshot {
    pub(super) key: TabKey,
    /// [x, y, width, height] in points
    pub(super) rect: [f32; 4],
}

impl VellumGuiApp {
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
        let min_x = bounds.left();
        let max_x = bounds.right() - width;
        let min_y = bounds.top();
        let max_y = bounds.bottom() - height;
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
        let detached_tabs = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .unwrap_or_default();

        if let Some(dock_state) = &self.dock_state {
            let mut visible = Vec::new();
            let mut seen = HashSet::new();
            for ((surface, _), tab) in dock_state.iter_all_tabs() {
                if surface.is_main()
                    && !detached_tabs.contains(&tab.id.key)
                    && seen.insert(tab.id.key.clone())
                {
                    visible.push(tab.id.key.clone());
                }
            }
            if !visible.is_empty() {
                return visible;
            }
        }

        let mut visible: Vec<(String, TabKey)> = self
            .available_tabs
            .iter()
            .filter_map(|(key, tab)| {
                if self.hidden_tabs.contains(key) || detached_tabs.contains(key) {
                    None
                } else {
                    Some((tab.id.title.clone(), key.clone()))
                }
            })
            .collect();
        visible.sort_by_key(|(title, _)| title.to_ascii_lowercase());
        visible.into_iter().map(|(_, key)| key).collect()
    }

    pub(super) fn collect_detached_tab_keys(dock_state: &DockState<GuiTab>) -> HashSet<TabKey> {
        let mut detached = HashSet::new();
        for ((surface, _), tab) in dock_state.iter_all_tabs() {
            if !surface.is_main() {
                detached.insert(tab.id.key.clone());
            }
        }
        detached
    }

    fn sanitize_viewport_state(
        state: &ViewportState,
        monitor_bounds: Option<[f32; 4]>,
    ) -> ViewportState {
        let mut viewport = state.clone();
        if !viewport.outer_pos_px[0].is_finite() {
            viewport.outer_pos_px[0] = 120.0;
        }
        if !viewport.outer_pos_px[1].is_finite() {
            viewport.outer_pos_px[1] = 120.0;
        }
        if !viewport.outer_size_px[0].is_finite() {
            viewport.outer_size_px[0] = 640.0;
        }
        if !viewport.outer_size_px[1].is_finite() {
            viewport.outer_size_px[1] = 480.0;
        }

        viewport.outer_size_px[0] = viewport.outer_size_px[0].max(MIN_VIEWPORT_WIDTH);
        viewport.outer_size_px[1] = viewport.outer_size_px[1].max(MIN_VIEWPORT_HEIGHT);
        if let Some(bounds) = monitor_bounds.filter(|bounds| bounds.iter().all(|v| v.is_finite())) {
            viewport.clamp_to_bounds(bounds, MIN_VISIBLE_VIEWPORT_PX);
        }
        viewport
    }

    fn apply_viewport_to_surface(
        dock_state: &mut DockState<GuiTab>,
        surface: SurfaceIndex,
        viewport: &ViewportState,
        monitor_bounds: Option<[f32; 4]>,
    ) {
        let viewport = Self::sanitize_viewport_state(viewport, monitor_bounds);
        if !viewport.outer_pos_px.iter().all(|value| value.is_finite())
            || !viewport.outer_size_px.iter().all(|value| value.is_finite())
        {
            return;
        }
        if let Some(window_state) = dock_state.get_window_state_mut(surface) {
            window_state
                .set_position(Pos2::new(
                    viewport.outer_pos_px[0],
                    viewport.outer_pos_px[1],
                ))
                .set_size(Vec2::new(
                    viewport.outer_size_px[0],
                    viewport.outer_size_px[1],
                ));
        }
    }

    pub(super) fn attach_detached_windows(
        dock_state: &mut DockState<GuiTab>,
        available_tabs: &HashMap<TabKey, GuiTab>,
        detached_viewports: &[ViewportState],
        monitor_bounds: Option<[f32; 4]>,
    ) {
        let mut attached = HashSet::new();
        for viewport in detached_viewports {
            if !attached.insert(viewport.tab.clone()) {
                continue;
            }
            let Some(tab) = available_tabs.get(&viewport.tab).cloned() else {
                continue;
            };
            let surface = dock_state.add_window(vec![tab]);
            Self::apply_viewport_to_surface(dock_state, surface, viewport, monitor_bounds);
        }
    }

    pub(super) fn collect_detached_viewports_for_save(
        dock_state: &mut DockState<GuiTab>,
        monitor_bounds: Option<[f32; 4]>,
    ) -> HashMap<String, ViewportState> {
        let mut detached = HashMap::new();
        let surface_count = dock_state.surfaces_count();

        for raw_index in 1..surface_count {
            let surface = SurfaceIndex(raw_index);
            let tabs: Vec<TabKey> = match dock_state.get_surface(surface) {
                Some(Surface::Window(tree, _)) => tree
                    .iter()
                    .flat_map(|node| node.iter_tabs())
                    .map(|tab| tab.id.key.clone())
                    .collect(),
                _ => Vec::new(),
            };
            if tabs.is_empty() {
                continue;
            }

            let rect = dock_state
                .get_window_state(surface)
                .map(|state| state.rect())
                .unwrap_or(Rect::NOTHING);
            let fallback = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(600.0, 400.0));
            let safe_rect =
                if rect.is_finite() && rect.width().is_finite() && rect.height().is_finite() {
                    rect
                } else {
                    fallback
                };

            for tab_key in tabs {
                let mut viewport = ViewportState::new(
                    tab_key.clone(),
                    [safe_rect.min.x, safe_rect.min.y],
                    [safe_rect.width(), safe_rect.height()],
                );
                if let Some(bounds) = monitor_bounds {
                    viewport.clamp_to_bounds(bounds, MIN_VISIBLE_VIEWPORT_PX);
                }
                let id = format!("vp_surface{}_{}", raw_index, tab_key.short_id());
                detached.insert(id, viewport);
            }
        }

        detached
    }

    pub(super) fn rebuild_dock_state(&mut self) {
        let detached_viewports = self
            .dock_state
            .as_mut()
            .map(|dock_state| {
                Self::collect_detached_viewports_for_save(dock_state, self.last_monitor_bounds)
            })
            .unwrap_or_default();
        let detached_viewports: Vec<ViewportState> = detached_viewports
            .into_values()
            .filter(|viewport| {
                !self.hidden_tabs.contains(&viewport.tab)
                    && self.available_tabs.contains_key(&viewport.tab)
            })
            .collect();
        self.dock_state = if detached_viewports.is_empty() {
            None
        } else {
            let mut dock_state = DockState::new(Vec::new());
            Self::attach_detached_windows(
                &mut dock_state,
                &self.available_tabs,
                &detached_viewports,
                self.last_monitor_bounds,
            );
            Some(dock_state)
        };
    }

    pub(super) fn detach_tab(&mut self, key: TabKey) {
        let Some(tab) = self.available_tabs.get(&key).cloned() else {
            return;
        };
        let already_detached = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .is_some_and(|detached| detached.contains(&key));
        if already_detached {
            return;
        }

        let mut dock_state = self
            .dock_state
            .take()
            .unwrap_or_else(|| DockState::new(Vec::new()));
        let surface = dock_state.add_window(vec![tab]);
        let bounds = self
            .last_monitor_bounds
            .unwrap_or([0.0, 0.0, 1200.0, 800.0]);
        let viewport = ViewportState::new(
            key,
            [bounds[0] + 120.0, bounds[1] + 120.0],
            [
                bounds[2].min(640.0).max(320.0),
                bounds[3].min(480.0).max(240.0),
            ],
        );
        Self::apply_viewport_to_surface(&mut dock_state, surface, &viewport, Some(bounds));
        self.dock_state = Some(dock_state);
        self.layout_dirty = true;
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

    pub(super) fn apply_pending_detached_viewports(&mut self, monitor_bounds: [f32; 4]) {
        if self.pending_detached_viewports.is_empty() {
            return;
        }
        if let Some(dock_state) = &mut self.dock_state {
            for viewport in &self.pending_detached_viewports {
                let mut target_surface = None;
                for ((surface, _), tab) in dock_state.iter_all_tabs() {
                    if !surface.is_main() && tab.id.key == viewport.tab {
                        target_surface = Some(surface);
                        break;
                    }
                }
                if let Some(surface) = target_surface {
                    Self::apply_viewport_to_surface(
                        dock_state,
                        surface,
                        viewport,
                        Some(monitor_bounds),
                    );
                }
            }
        }
        self.pending_detached_viewports.clear();
        self.layout_dirty = true;
    }

    pub(super) fn hide_removed_detached_tabs(&mut self, detached_before_frame: &HashSet<TabKey>) {
        if detached_before_frame.is_empty() {
            return;
        }

        let detached_after_frame = self
            .dock_state
            .as_ref()
            .map(Self::collect_detached_tab_keys)
            .unwrap_or_default();
        let all_tabs_after: HashSet<TabKey> = self
            .dock_state
            .as_ref()
            .map(|dock_state| {
                dock_state
                    .iter_all_tabs()
                    .map(|(_, tab)| tab.id.key.clone())
                    .collect()
            })
            .unwrap_or_default();

        for key in detached_before_frame {
            if detached_after_frame.contains(key) || all_tabs_after.contains(key) {
                continue;
            }
            self.hide_tab(key.clone());
        }
    }

    pub(super) fn render_detached_window_host(
        &mut self,
        ui: &mut egui::Ui,
    ) -> (Vec<TabKey>, Vec<GuiLinkClick>) {
        let mut closed_tabs = Vec::new();
        let mut link_clicks = Vec::new();
        let Some(dock_state) = &mut self.dock_state else {
            return (closed_tabs, link_clicks);
        };

        let max_rect = ui.max_rect();
        if !max_rect.is_finite() {
            return (closed_tabs, link_clicks);
        }
        let host_rect = Rect::from_min_size(max_rect.min, Vec2::new(1.0, 1.0));
        ui.allocate_ui_at_rect(host_rect, |ui| {
            let mut viewer = GuiDockTabViewer::new(&self.app_core);
            DockArea::new(dock_state).show_inside(ui, &mut viewer);
            closed_tabs = viewer.take_closed_tabs();
            link_clicks = viewer.take_link_clicks();
        });

        (closed_tabs, link_clicks)
    }
}

struct GuiDockTabViewer<'a> {
    app_core: &'a AppCore,
    closed_tabs: Vec<TabKey>,
    link_clicks: Vec<GuiLinkClick>,
}

impl<'a> GuiDockTabViewer<'a> {
    fn new(app_core: &'a AppCore) -> Self {
        Self {
            app_core,
            closed_tabs: Vec::new(),
            link_clicks: Vec::new(),
        }
    }

    fn take_closed_tabs(&mut self) -> Vec<TabKey> {
        std::mem::take(&mut self.closed_tabs)
    }

    fn take_link_clicks(&mut self) -> Vec<GuiLinkClick> {
        std::mem::take(&mut self.link_clicks)
    }
}

impl TabViewer for GuiDockTabViewer<'_> {
    type Tab = GuiTab;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        if let Some(click) = VellumGuiApp::render_window_content(self.app_core, ui, tab) {
            self.link_clicks.push(click);
        }
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.id.title.clone().into()
    }

    fn is_closeable(&self, _tab: &Self::Tab) -> bool {
        true
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> OnCloseResponse {
        self.closed_tabs.push(tab.id.key.clone());
        OnCloseResponse::Close
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
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: DockStateSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.visible_tabs.len(), 2);
        assert_eq!(parsed.visible_tabs[0], TabKey::TextMain);
        assert_eq!(parsed.visible_tabs[1], TabKey::Vitals);
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

    #[test]
    fn test_sanitize_viewport_state_clamps_and_enforces_min_size() {
        let viewport = ViewportState::new(TabKey::Vitals, [-500.0, -500.0], [20.0, 30.0]);
        let sanitized =
            VellumGuiApp::sanitize_viewport_state(&viewport, Some([0.0, 0.0, 1920.0, 1080.0]));

        assert!(sanitized.outer_size_px[0] >= MIN_VIEWPORT_WIDTH);
        assert!(sanitized.outer_size_px[1] >= MIN_VIEWPORT_HEIGHT);

        let min_x = 0.0 - sanitized.outer_size_px[0] + MIN_VISIBLE_VIEWPORT_PX;
        let max_x = 1920.0 - MIN_VISIBLE_VIEWPORT_PX;
        let min_y = 0.0 - sanitized.outer_size_px[1] + MIN_VISIBLE_VIEWPORT_PX;
        let max_y = 1080.0 - MIN_VISIBLE_VIEWPORT_PX;
        assert!(sanitized.outer_pos_px[0] >= min_x - 0.01);
        assert!(sanitized.outer_pos_px[0] <= max_x + 0.01);
        assert!(sanitized.outer_pos_px[1] >= min_y - 0.01);
        assert!(sanitized.outer_pos_px[1] <= max_y + 0.01);
    }

    #[test]
    fn test_collect_detached_viewports_for_save_includes_window_tabs() {
        let mut dock_state = DockState::new(vec![GuiTab {
            id: TabId::new(TabKey::TextMain),
            window_name: "main".to_string(),
        }]);
        let detached_surface = dock_state.add_window(vec![GuiTab {
            id: TabId::new(TabKey::Vitals),
            window_name: "vitals".to_string(),
        }]);
        dock_state
            .get_window_state_mut(detached_surface)
            .expect("detached surface should have a window state")
            .set_position(Pos2::new(250.0, 120.0))
            .set_size(Vec2::new(640.0, 480.0));

        let detached = VellumGuiApp::collect_detached_viewports_for_save(&mut dock_state, None);
        assert_eq!(detached.len(), 1);
        let saved = detached.values().next().expect("detached viewport entry");
        assert_eq!(saved.tab, TabKey::Vitals);
        // `egui_dock::WindowState` reports `Rect::NOTHING` until first rendered frame,
        // so collection falls back to a safe default rectangle in headless unit tests.
        assert_eq!(saved.outer_pos_px, [100.0, 100.0]);
        assert_eq!(saved.outer_size_px, [600.0, 400.0]);
    }
}
