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
}
