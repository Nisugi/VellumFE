//! Shell zone layout for the GUI: header/footer/sidebars/center.
//!
//! Pure-move extraction from `app.rs`: the zone model (`GuiShellZone`,
//! shell layout snapshot), per-tab zone assignment and ordering, Alt+drag
//! zone moves with the drop overlay, and the per-zone window surfaces.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GuiShellZone {
    Header,
    Footer,
    LeftSidebar,
    Center,
    RightSidebar,
}

impl GuiShellZone {
    pub(super) fn label(self) -> &'static str {
        match self {
            GuiShellZone::Header => "Header",
            GuiShellZone::Footer => "Footer",
            GuiShellZone::LeftSidebar => "Left Bar",
            GuiShellZone::Center => "Center",
            GuiShellZone::RightSidebar => "Right Bar",
        }
    }

    fn id_fragment(self) -> &'static str {
        match self {
            GuiShellZone::Header => "header",
            GuiShellZone::Footer => "footer",
            GuiShellZone::LeftSidebar => "left",
            GuiShellZone::Center => "center",
            GuiShellZone::RightSidebar => "right",
        }
    }

    pub(super) fn all() -> [GuiShellZone; 5] {
        [
            GuiShellZone::Header,
            GuiShellZone::Footer,
            GuiShellZone::LeftSidebar,
            GuiShellZone::Center,
            GuiShellZone::RightSidebar,
        ]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TabZoneSnapshot {
    pub(super) key: TabKey,
    pub(super) zone: GuiShellZone,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ShellLayoutSnapshot {
    pub(super) header_height: f32,
    pub(super) footer_height: f32,
    pub(super) left_sidebar_width: f32,
    pub(super) right_sidebar_width: f32,
    #[serde(default = "serde_default_true")]
    pub(super) header_visible: bool,
    #[serde(default = "serde_default_true")]
    pub(super) footer_visible: bool,
    pub(super) left_sidebar_collapsed: bool,
    pub(super) right_sidebar_collapsed: bool,
}

const fn serde_default_true() -> bool {
    true
}

impl Default for ShellLayoutSnapshot {
    fn default() -> Self {
        Self {
            header_height: 140.0,
            footer_height: 180.0,
            left_sidebar_width: 300.0,
            right_sidebar_width: 300.0,
            // Default to a center-only shell; users can enable regions from the toolbar.
            header_visible: false,
            footer_visible: false,
            left_sidebar_collapsed: true,
            right_sidebar_collapsed: true,
        }
    }
}

impl ShellLayoutSnapshot {
    pub(super) fn sanitize(&mut self, center_width: f32) {
        self.header_height = self.header_height.clamp(96.0, 360.0);
        self.footer_height = self.footer_height.clamp(96.0, 420.0);
        self.left_sidebar_width = self.left_sidebar_width.clamp(220.0, 700.0);
        self.right_sidebar_width = self.right_sidebar_width.clamp(220.0, 700.0);

        let max_sidebar_width = ((center_width - 220.0).max(220.0) * 0.45).max(220.0);
        self.left_sidebar_width = self.left_sidebar_width.min(max_sidebar_width);
        self.right_sidebar_width = self.right_sidebar_width.min(max_sidebar_width);
    }
}

#[derive(Clone, Debug)]
pub(super) struct GuiZoneDragState {
    tab_key: TabKey,
    from_zone: GuiShellZone,
    pointer_pos: Pos2,
}

/// Move mode: in the center/header/footer zones the window follows the
/// cursor until a click places it or Esc restores the original position;
/// in a sidebar it live-reorders the stack under the pointer instead.
/// Works with the title bar hidden.
#[derive(Clone, Debug)]
pub(super) struct GuiWindowMoveState {
    pub(super) tab_key: TabKey,
    /// Stored rect at move start, restored on cancel
    pub(super) original_rect: Option<[f32; 4]>,
    /// Sidebar zones: stack order at move start, restored on cancel.
    /// Captured lazily on the first overlay frame.
    pub(super) original_order: Option<Vec<TabKey>>,
    /// True until the first overlay frame; the menu click that started the
    /// move must not count as the placement click.
    pub(super) just_started: bool,
}

#[derive(Clone, Debug)]
pub(super) struct GuiZoneWindowRect {
    pub(super) zone: GuiShellZone,
    pub(super) tab_key: TabKey,
    pub(super) rect: Rect,
}

#[derive(Clone, Debug)]
pub(super) struct GuiZoneDropResult {
    tab_key: TabKey,
    target_zone: GuiShellZone,
    insert_before: Option<TabKey>,
}

impl VellumGuiApp {
    pub(super) fn default_zone_for_tab_key(tab_key: &TabKey) -> GuiShellZone {
        match tab_key {
            TabKey::LeftHand | TabKey::RightHand | TabKey::SpellHand => GuiShellZone::Header,
            TabKey::Compass
            | TabKey::Quickbar { .. }
            | TabKey::Indicators
            | TabKey::Vitals
            | TabKey::Countdown { .. }
            | TabKey::Dashboard
            | TabKey::Encumbrance
            | TabKey::Experience
            | TabKey::Perception
            | TabKey::InjuryDoll => GuiShellZone::Footer,
            _ => GuiShellZone::Center,
        }
    }

    pub(super) fn zone_for_tab(&self, key: &TabKey) -> GuiShellZone {
        self.tab_zones
            .get(key)
            .copied()
            .unwrap_or_else(|| Self::default_zone_for_tab_key(key))
    }

    fn target_docked_height(&self, zone: GuiShellZone) -> Option<f32> {
        match zone {
            GuiShellZone::Header => Some(
                (self.shell_layout.header_height - 12.0).max(MIN_DOCKED_WINDOW_HEIGHT),
            ),
            GuiShellZone::Footer => Some(
                (self.shell_layout.footer_height - 12.0).max(MIN_DOCKED_WINDOW_HEIGHT),
            ),
            _ => None,
        }
    }

    fn is_compact_center_widget(widget_type: &WidgetType) -> bool {
        matches!(
            widget_type,
            WidgetType::Hand
                | WidgetType::MiniVitals
                | WidgetType::Progress
                | WidgetType::Compass
                | WidgetType::Indicator
                | WidgetType::Countdown
        )
    }

    fn min_window_height_for_zone(zone: GuiShellZone, window: &WindowState) -> f32 {
        if matches!(zone, GuiShellZone::Header | GuiShellZone::Footer) {
            MIN_DOCKED_WINDOW_HEIGHT
        } else if zone == GuiShellZone::Center && Self::is_compact_center_widget(&window.widget_type)
        {
            MIN_DOCKED_WINDOW_HEIGHT
        } else {
            90.0
        }
    }

    /// Assign a tab to a zone. Grouped tabs move as a unit so the group
    /// keeps rendering on one surface.
    pub(super) fn set_tab_zone(&mut self, key: TabKey, zone: GuiShellZone) {
        let group_members = self
            .group_for_tab(&key)
            .map(|group| group.members.clone());
        if let Some(members) = group_members {
            for member in members {
                self.set_tab_zone_single(member, zone);
            }
        } else {
            self.set_tab_zone_single(key, zone);
        }
    }

    fn set_tab_zone_single(&mut self, key: TabKey, zone: GuiShellZone) {
        let current = self.zone_for_tab(&key);
        if current != zone {
            // Order value from BEFORE this tab joins the zone: append at the
            // end instead of inheriting a stale y from the previous zone.
            let append_y = self.next_zone_order_y(zone);
            self.tab_zones.insert(key.clone(), zone);
            if let Some(target_height) = self.target_docked_height(zone) {
                let entry = self
                    .main_window_rects
                    .entry(key.clone())
                    .or_insert([16.0, append_y, 240.0, target_height]);
                entry[1] = append_y;
                entry[3] = target_height;
            }
            if matches!(zone, GuiShellZone::LeftSidebar | GuiShellZone::RightSidebar) {
                let entry = self
                    .main_window_rects
                    .entry(key.clone())
                    .or_insert([16.0, append_y, 240.0, 240.0]);
                entry[1] = append_y;
                entry[3] = entry[3].clamp(40.0, 600.0);
            }
            self.layout_dirty = true;
        }
    }

    pub(super) fn apply_zone_drop(&mut self, drop_result: GuiZoneDropResult) {
        let GuiZoneDropResult {
            tab_key,
            target_zone,
            insert_before,
        } = drop_result;

        self.set_tab_zone(tab_key.clone(), target_zone);
        if matches!(target_zone, GuiShellZone::Center) {
            // Restore last center geometry if available so moves out/in of header/footer
            // do not inherit docked coordinates.
            if let Some(snapshot) = self.last_center_window_rects.get(&tab_key).copied() {
                self.main_window_rects.insert(tab_key, snapshot);
            } else {
                // Never rendered in center this session: the stored rect holds
                // synthetic docked coordinates. Drop it so the center renderer
                // assigns its default fallback rect instead.
                self.main_window_rects.remove(&tab_key);
            }
            self.layout_dirty = true;
            // Center windows are freely positioned/resized; do not normalize their order
            // into synthetic y offsets or they will collapse toward the top-left.
            return;
        }

        let detached_tabs = self.detached_tab_keys();
        let mut ordered: Vec<TabKey> = self
            .zone_surface_tabs(&detached_tabs, target_zone)
            .into_iter()
            .map(|tab| tab.id.key)
            .collect();
        let Some(existing_idx) = ordered.iter().position(|candidate| candidate == &tab_key) else {
            return;
        };
        ordered.remove(existing_idx);
        let insert_idx = insert_before
            .as_ref()
            .and_then(|before_key| ordered.iter().position(|candidate| candidate == before_key))
            .unwrap_or(ordered.len());
        ordered.insert(insert_idx, tab_key);
        self.persist_zone_order(&ordered);
    }

    fn title_bar_hidden(&self, key: &TabKey) -> bool {
        self.no_title_tabs.contains(key)
    }

    pub(super) fn toggle_title_bar(&mut self, key: TabKey) {
        if self.no_title_tabs.contains(&key) {
            self.no_title_tabs.remove(&key);
        } else {
            self.no_title_tabs.insert(key);
        }
        self.layout_dirty = true;
    }

    /// Spacing between synthetic order-encoding y values. Deliberately far
    /// larger than any TUI grid coordinate (`window.position.y`, the sort
    /// fallback for never-ordered tabs) so the two never interleave.
    const ZONE_ORDER_STEP: f32 = 1000.0;

    fn persist_zone_order(&mut self, ordered: &[TabKey]) {
        let mut y = Self::ZONE_ORDER_STEP;
        for key in ordered {
            let rect = self
                .main_window_rects
                .entry(key.clone())
                .or_insert([16.0, y, 220.0, 140.0]);
            rect[1] = y;
            y += Self::ZONE_ORDER_STEP;
        }
        self.layout_dirty = true;
    }

    /// Order value that places a tab after everything currently in `zone`.
    fn next_zone_order_y(&self, zone: GuiShellZone) -> f32 {
        self.tab_zones
            .iter()
            .filter(|(_, assigned)| **assigned == zone)
            .filter_map(|(key, _)| self.main_window_rects.get(key))
            .map(|rect| rect[1])
            .fold(0.0f32, f32::max)
            + Self::ZONE_ORDER_STEP
    }

    pub(super) fn move_tab_within_zone(&mut self, key: &TabKey, zone: GuiShellZone, move_up: bool) {
        let detached_tabs = self.detached_tab_keys();
        let mut ordered: Vec<TabKey> = self
            .zone_surface_tabs(&detached_tabs, zone)
            .into_iter()
            .map(|tab| tab.id.key)
            .collect();
        let Some(current_idx) = ordered.iter().position(|candidate| candidate == key) else {
            return;
        };
        let target_idx = if move_up {
            current_idx.checked_sub(1)
        } else if current_idx + 1 < ordered.len() {
            Some(current_idx + 1)
        } else {
            None
        };
        if let Some(target_idx) = target_idx {
            ordered.swap(current_idx, target_idx);
            self.persist_zone_order(&ordered);
        }
    }

    fn zone_surface_tabs(&self, detached_tabs: &HashSet<TabKey>, zone: GuiShellZone) -> Vec<GuiTab> {
        let mut tabs: Vec<(i32, i32, String, GuiTab)> = self
            .available_tabs
            .iter()
            .filter_map(|(key, tab)| {
                if self.hidden_tabs.contains(key)
                    || detached_tabs.contains(key)
                    || self.zone_for_tab(key) != zone
                    // Grouped followers render inside their leader's window.
                    || self.is_grouped_follower(key)
                {
                    return None;
                }
                let window = self.app_core.ui_state.windows.get(&tab.window_name)?;
                let saved_y = self
                    .main_window_rects
                    .get(key)
                    .and_then(|rect| rect.get(1).copied())
                    .filter(|v| v.is_finite())
                    .unwrap_or(window.position.y as f32);
                let saved_x = self
                    .main_window_rects
                    .get(key)
                    .and_then(|rect| rect.get(0).copied())
                    .filter(|v| v.is_finite())
                    .unwrap_or(window.position.x as f32);
                Some((
                    saved_y.round() as i32,
                    saved_x.round() as i32,
                    tab.id.title.to_ascii_lowercase(),
                    tab.clone(),
                ))
            })
            .collect();
        // sort_by_key would clone the title String on every comparison.
        tabs.sort_by(|a, b| (a.0, a.1, a.2.as_str()).cmp(&(b.0, b.1, b.2.as_str())));
        tabs.into_iter().map(|(_, _, _, tab)| tab).collect()
    }

    fn main_surface_bounds(&self, tabs: &[GuiTab]) -> (f32, f32) {
        let mut max_col = 0f32;
        let mut max_row = 0f32;
        for tab in tabs {
            let Some(window) = self.app_core.ui_state.windows.get(&tab.window_name) else {
                continue;
            };
            max_col = max_col.max((window.position.x + window.position.width).max(1) as f32);
            max_row = max_row.max((window.position.y + window.position.height).max(1) as f32);
        }
        (max_col.max(1.0), max_row.max(1.0))
    }

    fn docked_inner_size_for_outer(
        ctx: &egui::Context,
        outer_size: Vec2,
        include_title_bar: bool,
    ) -> Vec2 {
        let style = ctx.global_style();
        let window_frame = egui::Frame::window(&style).shadow(egui::epaint::Shadow::NONE);
        let mut margins = window_frame.total_margin().sum();
        if include_title_bar {
            let title_font = egui::TextStyle::Heading.resolve(&style);
            let title_bar_inner_height = ctx
                .fonts_mut(|fonts| fonts.row_height(&title_font))
                .max(style.spacing.interact_size.y);
            let title_bar_height_with_margin =
                title_bar_inner_height + window_frame.inner_margin.sum().y;
            let title_content_spacing = window_frame.stroke.width;
            margins += Vec2::new(0.0, title_bar_height_with_margin + title_content_spacing);
        }
        Vec2::new(
            (outer_size.x - margins.x).max(1.0),
            (outer_size.y - margins.y).max(1.0),
        )
    }

    fn tab_window_rect(
        root_rect: Rect,
        layout_bounds: (f32, f32),
        window: &WindowState,
    ) -> Option<Rect> {
        if !root_rect.is_finite() {
            return None;
        }
        let (max_col, max_row) = layout_bounds;
        if max_col <= 0.0 || max_row <= 0.0 {
            return None;
        }

        let left = root_rect.left() + (window.position.x as f32 / max_col) * root_rect.width();
        let top = root_rect.top() + (window.position.y as f32 / max_row) * root_rect.height();
        let width = ((window.position.width as f32 / max_col) * root_rect.width()).max(120.0);
        let height = ((window.position.height as f32 / max_row) * root_rect.height())
            .max(MIN_DOCKED_WINDOW_HEIGHT);
        if !left.is_finite() || !top.is_finite() || !width.is_finite() || !height.is_finite() {
            return None;
        }
        let rect = Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, height));
        let clipped = rect.intersect(root_rect);
        if !clipped.is_finite() {
            return None;
        }
        if clipped.width() < 60.0 || clipped.height() < MIN_DOCKED_WINDOW_HEIGHT {
            None
        } else {
            Some(clipped)
        }
    }

    fn zone_drag_pointer_for_rect(
        ctx: &egui::Context,
        window_rect: Rect,
        window_layer: egui::LayerId,
    ) -> Option<Pos2> {
        let pointer_pos = ctx.input(|i| {
            if !i.modifiers.alt || !i.pointer.button_down(egui::PointerButton::Primary) {
                return None;
            }
            let pointer_pos = i.pointer.interact_pos().or(i.pointer.latest_pos())?;
            if !window_rect.contains(pointer_pos) || i.pointer.delta().length_sq() <= f32::EPSILON {
                return None;
            }
            Some(pointer_pos)
        })?;
        // Overlapping windows both contain the pointer; without this check
        // whichever renders first in iteration order steals the drag from
        // the window visually on top. Only the top-most layer under the
        // pointer (egui's own click-routing rule) may start the drag.
        (ctx.layer_id_at(pointer_pos) == Some(window_layer)).then_some(pointer_pos)
    }

    fn zone_drop_insert_before(
        zone: GuiShellZone,
        pointer_pos: Pos2,
        window_rects: &[GuiZoneWindowRect],
        dragged_tab: &TabKey,
    ) -> Option<TabKey> {
        if matches!(zone, GuiShellZone::Center) {
            return None;
        }
        for window in window_rects
            .iter()
            .filter(|window| window.zone == zone && window.tab_key != *dragged_tab)
        {
            let should_insert_before = match zone {
                GuiShellZone::LeftSidebar | GuiShellZone::RightSidebar => {
                    pointer_pos.y < window.rect.center().y
                }
                GuiShellZone::Header | GuiShellZone::Footer => pointer_pos.x < window.rect.center().x,
                GuiShellZone::Center => false,
            };
            if should_insert_before {
                return Some(window.tab_key.clone());
            }
        }
        None
    }

    fn zone_for_pointer(
        zone_rects: &[(GuiShellZone, Rect)],
        pointer_pos: Pos2,
    ) -> Option<GuiShellZone> {
        zone_rects
            .iter()
            .find_map(|(zone, rect)| rect.contains(pointer_pos).then_some(*zone))
    }

    pub(super) fn render_zone_drop_overlay(
        &mut self,
        ctx: &egui::Context,
        zone_rects: &[(GuiShellZone, Rect)],
        window_rects: &[GuiZoneWindowRect],
    ) -> Option<GuiZoneDropResult> {
        let mut drag = self.zone_drag_state.clone()?;
        let pointer_pos = ctx
            .input(|i| i.pointer.interact_pos().or(i.pointer.latest_pos()))
            .unwrap_or(drag.pointer_pos);
        drag.pointer_pos = pointer_pos;
        self.zone_drag_state = Some(drag.clone());
        if !ctx.input(|i| i.modifiers.alt) {
            self.zone_drag_state = None;
            return None;
        }

        let hovered_zone = Self::zone_for_pointer(zone_rects, pointer_pos);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("gui_zone_drop_overlay"),
        ));
        for (zone, rect) in zone_rects {
            let tint = if Some(*zone) == hovered_zone {
                Color32::from_rgba_unmultiplied(70, 130, 220, 48)
            } else {
                Color32::from_rgba_unmultiplied(35, 35, 35, 24)
            };
            painter.rect_filled(*rect, 0.0, tint);
        }

        let drop_hint = hovered_zone
            .map(|zone| {
                if zone == drag.from_zone {
                    format!("Reorder in {}", zone.label())
                } else {
                    format!("Drop to {}", zone.label())
                }
            })
            .unwrap_or_else(|| "Release to cancel move".to_string());
        egui::Area::new(egui::Id::new("gui_zone_drop_hint"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer_pos + Vec2::new(16.0, 16.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.label(drop_hint);
            });

        let pointer_released = ctx.input(|i| i.pointer.any_released());
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if pointer_released || !pointer_down {
            self.zone_drag_state = None;
            if let Some(target_zone) = hovered_zone {
                let insert_before = Self::zone_drop_insert_before(
                    target_zone,
                    pointer_pos,
                    window_rects,
                    &drag.tab_key,
                );
                if target_zone == drag.from_zone
                    && insert_before.is_none()
                    && matches!(target_zone, GuiShellZone::Center)
                {
                    return None;
                }
                return Some(GuiZoneDropResult {
                    tab_key: drag.tab_key,
                    target_zone,
                    insert_before,
                });
            }
        }
        None
    }

    /// Drive Move mode. Center/header/footer: the window follows the cursor
    /// within its zone. Sidebars: the window live-reorders within the stack
    /// under the pointer. A click commits, Esc restores the starting state.
    /// Runs after the zone surfaces so it sees this frame's input; a
    /// full-screen catcher swallows pointer interactions so the placement
    /// click can't reach any window content.
    pub(super) fn render_window_move_overlay(
        &mut self,
        ctx: &egui::Context,
        zone_rects: &[(GuiShellZone, Rect)],
        window_rects: &[GuiZoneWindowRect],
    ) {
        let Some(mut state) = self.window_move_state.clone() else {
            return;
        };
        if !self.available_tabs.contains_key(&state.tab_key) {
            // The tab vanished (hidden/detached); abandon the move.
            self.window_move_state = None;
            return;
        }
        let zone = self.zone_for_tab(&state.tab_key);
        let Some(zone_rect) = zone_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == zone).then_some(*rect))
        else {
            self.window_move_state = None;
            return;
        };
        let is_sidebar = matches!(zone, GuiShellZone::LeftSidebar | GuiShellZone::RightSidebar);

        // Sidebar moves are reorders; remember the starting order for Esc.
        if is_sidebar && state.original_order.is_none() {
            let detached = self.detached_tab_keys();
            state.original_order = Some(
                self.zone_surface_tabs(&detached, zone)
                    .into_iter()
                    .map(|tab| tab.id.key)
                    .collect(),
            );
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if is_sidebar {
                if let Some(original) = state.original_order.take() {
                    self.persist_zone_order(&original);
                }
            } else {
                match state.original_rect {
                    Some(rect) => {
                        self.main_window_rects.insert(state.tab_key.clone(), rect);
                    }
                    None => {
                        self.main_window_rects.remove(&state.tab_key);
                    }
                }
            }
            self.window_move_state = None;
            return;
        }

        ctx.set_cursor_icon(egui::CursorIcon::Move);
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos().or(i.pointer.latest_pos()));
        if let Some(pos) = pointer_pos {
            if is_sidebar {
                // Live-reorder the stack to match the pointer.
                let detached = self.detached_tab_keys();
                let current: Vec<TabKey> = self
                    .zone_surface_tabs(&detached, zone)
                    .into_iter()
                    .map(|tab| tab.id.key)
                    .collect();
                if let Some(existing_idx) =
                    current.iter().position(|key| key == &state.tab_key)
                {
                    let insert_before =
                        Self::zone_drop_insert_before(zone, pos, window_rects, &state.tab_key);
                    let mut reordered = current.clone();
                    reordered.remove(existing_idx);
                    let insert_idx = insert_before
                        .as_ref()
                        .and_then(|before| reordered.iter().position(|key| key == before))
                        .unwrap_or(reordered.len());
                    reordered.insert(insert_idx, state.tab_key.clone());
                    if reordered != current {
                        self.persist_zone_order(&reordered);
                    }
                }
            } else if let Some(stored) = self.main_window_rects.get(&state.tab_key).copied() {
                let size = Vec2::new(stored[2].max(60.0), stored[3].max(24.0));
                // Grab point: top-center, where a title bar would be held.
                let target = Rect::from_min_size(
                    Pos2::new(pos.x - size.x * 0.5, pos.y - 10.0),
                    size,
                );
                let clamped = Self::clamp_main_window_rect(target, zone_rect);
                if clamped.is_finite() {
                    self.main_window_rects
                        .insert(state.tab_key.clone(), Self::rect_to_snapshot(clamped));
                }
            }
            egui::Area::new(egui::Id::new("gui_window_move_hint"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pos + Vec2::new(16.0, 16.0))
                .interactable(false)
                .show(ctx, |ui| {
                    ui.label("Click to place — Esc to cancel");
                });
        }

        // Swallow all pointer interaction while the move is active so hovers
        // and the placement press never reach window content.
        let screen_rect = ctx.content_rect();
        egui::Area::new(egui::Id::new("gui_window_move_catcher"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                ui.allocate_response(screen_rect.size(), egui::Sense::click_and_drag());
            });

        // The menu click that started the move is still in this frame's
        // input; only later presses place the window.
        if std::mem::take(&mut state.just_started) {
            self.window_move_state = Some(state);
            return;
        }
        if ctx.input(|i| i.pointer.any_pressed()) {
            if zone == GuiShellZone::Center {
                if let Some(rect) = self.main_window_rects.get(&state.tab_key).copied() {
                    self.last_center_window_rects
                        .insert(state.tab_key.clone(), rect);
                }
            }
            self.layout_dirty = true;
            self.window_move_state = None;
            return;
        }
        self.window_move_state = Some(state);
    }

    pub(super) fn render_zone_surface(
        &mut self,
        ctx: &egui::Context,
        detached_tabs: &HashSet<TabKey>,
        zone: GuiShellZone,
        root_rect: Rect,
        zone_window_rects: &mut Vec<GuiZoneWindowRect>,
    ) -> GuiWindowActions {
        let mut actions = GuiWindowActions::default();
        let primary_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        if !primary_down {
            self.hand_resize_tab = None;
        }
        if !root_rect.is_finite() || root_rect.width() <= 24.0 || root_rect.height() <= 24.0 {
            return actions;
        }

        let tabs = self.zone_surface_tabs(detached_tabs, zone);
        if tabs.is_empty() {
            return actions;
        }
        let layout_bounds = self.main_surface_bounds(&tabs);
        let is_sidebar = matches!(zone, GuiShellZone::LeftSidebar | GuiShellZone::RightSidebar);
        let secondary_click_pos = ctx.input(|input| {
            if input.pointer.secondary_clicked() {
                input.pointer.interact_pos()
            } else {
                None
            }
        });

        if is_sidebar {
            let margin = 0.0;
            let gap = 4.0;
            let resize_handle_height = 8.0;
            let slot_width = (root_rect.width() - margin * 2.0).max(120.0);
            let mut y = root_rect.min.y + margin;

            // Prepass: per-widget minimum/default heights so a one-line bar
            // (encumbrance, stance) does not reserve a text-window-sized slot.
            let tab_metrics: Vec<(GuiTab, f32, f32)> = tabs
                .into_iter()
                .map(|tab| {
                    let compact = self
                        .app_core
                        .ui_state
                        .windows
                        .get(&tab.window_name)
                        .map(|window| {
                            Self::is_compact_center_widget(&window.widget_type)
                                || matches!(
                                    window.widget_type,
                                    WidgetType::Encumbrance | WidgetType::Dashboard
                                )
                        })
                        .unwrap_or(false);
                    let min_height = if compact { 40.0 } else { 120.0 };
                    let default_height = if compact { 72.0 } else { 240.0 };
                    let desired_height = self
                        .main_window_rects
                        .get(&tab.id.key)
                        .map(|rect| rect[3])
                        .filter(|v| v.is_finite())
                        .unwrap_or(default_height);
                    (tab, min_height, desired_height)
                })
                .collect();
            let mut remaining_min: f32 = tab_metrics
                .iter()
                .map(|(_, min_height, _)| min_height + gap)
                .sum();

            for (tab, min_slot_height, desired_height) in tab_metrics {
                remaining_min -= min_slot_height + gap;
                if y >= root_rect.max.y - margin {
                    break;
                }
                let max_height_here =
                    (root_rect.max.y - margin - y - remaining_min).max(min_slot_height);
                let slot_height = desired_height.clamp(min_slot_height, max_height_here);
                let slot_bottom = (y + slot_height).min(root_rect.max.y - margin - remaining_min);
                let slot_rect = Rect::from_min_max(
                    Pos2::new(root_rect.min.x + margin, y),
                    Pos2::new(root_rect.min.x + margin + slot_width, slot_bottom),
                );
                if slot_rect.height() < MIN_DOCKED_WINDOW_HEIGHT {
                    y = slot_bottom + gap;
                    continue;
                }

                let mut clicked_link = None;
                let mut resize_delta_y = 0.0f32;
                let title_bar_hidden = self.title_bar_hidden(&tab.id.key);
                let window_id =
                    egui::Id::new(("gui_zone_window", zone.id_fragment(), &tab.id.key));
                let mut window_frame = egui::Frame::window(ctx.global_style().as_ref())
                    .outer_margin(egui::Margin::ZERO)
                    .shadow(egui::epaint::Shadow::NONE);
                if let Some(accent) = self.accent_color_for_tab(&tab.id.key) {
                    window_frame.stroke.color = accent;
                }
                self.apply_skin_border_to_frame(&tab.window_name, &mut window_frame);
                // Advance by what actually rendered, not by the intended slot:
                // any disagreement between our chrome math and egui's real
                // window chrome then shows up as a slightly different next-y
                // instead of windows overlapping or leaving gaps.
                let mut next_y = slot_bottom;
                if let Some(inner) = egui::Window::new(self.window_display_title(&tab))
                    .id(window_id)
                    .fixed_pos(slot_rect.min)
                    .fixed_size(Self::docked_inner_size_for_outer(
                        ctx,
                        slot_rect.size(),
                        !title_bar_hidden,
                    ))
                    .resizable(false)
                    .movable(false)
                    .title_bar(!title_bar_hidden)
                    .collapsible(false)
                    .frame(window_frame)
                    .constrain_to(root_rect)
                    .show(ctx, |ui| {
                        ui.push_id(&tab.id.key, |ui| {
                            // Reserve the resize handle's row up front; content
                            // that fills available height would otherwise push
                            // the handle past the fixed window size, clipping
                            // it out of reach entirely.
                            let content_size = Vec2::new(
                                ui.available_width().max(1.0),
                                (ui.available_height()
                                    - resize_handle_height
                                    - ui.spacing().item_spacing.y)
                                    .max(1.0),
                            );
                            let clicked = ui
                                .allocate_ui(content_size, |ui| {
                                    ui.set_min_size(content_size);
                                    self.render_window_or_group_content(ui, &tab)
                                })
                                .inner;
                            let handle_response = ui.allocate_response(
                                Vec2::new(ui.available_width().max(1.0), resize_handle_height),
                                egui::Sense::click_and_drag(),
                            );
                            let handle_active =
                                handle_response.hovered() || handle_response.dragged();
                            let stroke_color = if handle_active {
                                ui.visuals().widgets.hovered.fg_stroke.color
                            } else {
                                ui.visuals().weak_text_color()
                            };
                            let handle_center = handle_response.rect.center();
                            ui.painter().hline(
                                (handle_center.x - 16.0)..=(handle_center.x + 16.0),
                                handle_center.y,
                                egui::Stroke::new(2.0, stroke_color),
                            );
                            if handle_active {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                            }
                            if handle_response.dragged() {
                                resize_delta_y += ui.ctx().input(|i| i.pointer.delta().y);
                            }
                            clicked
                        })
                        .inner
                    })
                {
                    self.paint_skin_border(ctx, &tab.window_name, &inner.response);
                    clicked_link = inner.inner.flatten();
                    let rendered_bottom = inner.response.rect.max.y;
                    if rendered_bottom.is_finite() && rendered_bottom > slot_rect.min.y {
                        next_y = rendered_bottom;
                    }
                    zone_window_rects.push(GuiZoneWindowRect {
                        zone,
                        tab_key: tab.id.key.clone(),
                        rect: inner.response.rect,
                    });
                    if let Some(pointer_pos) = secondary_click_pos {
                        if inner.response.rect.contains(pointer_pos) {
                            actions.window_menu_request = Some(GuiWindowMenuRequest {
                                tab_key: tab.id.key.clone(),
                                zone,
                                allow_reorder: true,
                                title_bar_hidden,
                                position: pointer_pos,
                                window_rect: inner.response.rect,
                            });
                        }
                    }
                    if self.zone_drag_state.is_none() {
                        if let Some(pointer_pos) = Self::zone_drag_pointer_for_rect(
                            ctx,
                            inner.response.rect,
                            inner.response.layer_id,
                        ) {
                            self.zone_drag_state = Some(GuiZoneDragState {
                                tab_key: tab.id.key.clone(),
                                from_zone: zone,
                                pointer_pos,
                            });
                        }
                    }
                }
                y = next_y + gap;

                if let Some(click) = clicked_link {
                    actions.link_clicks.push(click);
                }
                if resize_delta_y.abs() > 0.0 {
                    let resized_height = (slot_rect.height() + resize_delta_y)
                        .clamp(min_slot_height, max_height_here);
                    let entry = self
                        .main_window_rects
                        .entry(tab.id.key.clone())
                        .or_insert([slot_rect.min.x, slot_rect.min.y, slot_rect.width(), resized_height]);
                    entry[3] = resized_height;
                    self.layout_dirty = true;
                }
            }

            return actions;
        }

        let window_bounds = if zone == GuiShellZone::Center {
            root_rect.shrink(1.0)
        } else {
            root_rect
        };
        if !window_bounds.is_finite() || window_bounds.width() <= 8.0 || window_bounds.height() <= 8.0 {
            return actions;
        }

        let mut occupied_rects: Vec<Rect> = Vec::new();
        for tab in tabs {
            let Some(window) = self.app_core.ui_state.windows.get(&tab.window_name) else {
                continue;
            };
            let group_shape = self
                .group_for_tab(&tab.id.key)
                .map(|group| (group.members.len(), group.horizontal));
            let min_window_height = {
                let base = Self::min_window_height_for_zone(zone, window);
                match group_shape {
                    // Vertical groups need room for each stacked member.
                    Some((count, false)) => base * count as f32,
                    _ => base,
                }
            };
            let min_window_size = Vec2::new(
                120.0_f32.min(window_bounds.width().max(1.0)),
                min_window_height.min(window_bounds.height().max(1.0)),
            );
            // Keep a little vertical headroom in header/footer so windows can be repositioned
            // instead of filling the entire zone and snapping back to the top.
            let max_window_height = if matches!(zone, GuiShellZone::Header | GuiShellZone::Footer) {
                (window_bounds.height() - 12.0).max(min_window_size.y)
            } else {
                window_bounds.height().max(min_window_size.y)
            };
            let max_window_size = Vec2::new(
                window_bounds.width().max(min_window_size.x),
                max_window_height,
            );
            let fallback_rect =
                Self::tab_window_rect(window_bounds, layout_bounds, window).unwrap_or_else(|| {
                    Rect::from_min_size(
                        Pos2::new(window_bounds.min.x + 8.0, window_bounds.min.y + 8.0),
                        Vec2::new(
                            (window_bounds.width() - 16.0).max(min_window_size.x),
                            (window_bounds.height() - 16.0).max(min_window_size.y),
                        ),
                    )
                });
            let initial_rect = self
                .main_window_rects
                .get(&tab.id.key)
                .copied()
                .and_then(Self::rect_from_snapshot)
                .map(|rect| Self::clamp_main_window_rect(rect, window_bounds))
                .unwrap_or(fallback_rect);
            if !initial_rect.is_finite() {
                continue;
            }

            let mut clicked_link = None;
            let mut hand_resize_delta_x = 0.0f32;
            let title_bar_hidden = self.title_bar_hidden(&tab.id.key);
            // Grouped hands lose the fixed-size hand behavior; the group is a
            // normal resizable window sized for all members.
            let is_hand_widget =
                matches!(window.content, WindowContent::Hand { .. }) && group_shape.is_none();
            let hand_resize_handle_width = 10.0f32;
            let pointer_over_hand_resize_handle = if is_hand_widget && primary_down {
                let handle_rect = Rect::from_min_max(
                    Pos2::new(initial_rect.max.x - hand_resize_handle_width, initial_rect.min.y),
                    initial_rect.max,
                );
                ctx.input(|i| {
                    i.pointer
                        .interact_pos()
                        .or(i.pointer.latest_pos())
                        .is_some_and(|pos| handle_rect.contains(pos))
                })
            } else {
                false
            };
            if is_hand_widget
                && primary_down
                && pointer_over_hand_resize_handle
                && self.hand_resize_tab.is_none()
            {
                self.hand_resize_tab = Some(tab.id.key.clone());
            }
            let hand_resize_active = is_hand_widget
                && primary_down
                && self
                    .hand_resize_tab
                    .as_ref()
                    .is_some_and(|key| key == &tab.id.key);
            let window_id =
                egui::Id::new(("gui_zone_window", zone.id_fragment(), &tab.id.key));
            let mut docked_window_frame = egui::Frame::window(ctx.global_style().as_ref())
                .outer_margin(egui::Margin::ZERO)
                .shadow(egui::epaint::Shadow::NONE);
            if let Some(accent) = self.accent_color_for_tab(&tab.id.key) {
                docked_window_frame.stroke.color = accent;
            }
            self.apply_skin_border_to_frame(&tab.window_name, &mut docked_window_frame);
            let mut window_builder = egui::Window::new(self.window_display_title(&tab))
                .id(window_id)
                .default_size(if zone == GuiShellZone::Center {
                    initial_rect.size()
                } else {
                    Self::docked_inner_size_for_outer(ctx, initial_rect.size(), !title_bar_hidden)
                })
                .min_size(min_window_size)
                .max_size(max_window_size)
                .resizable(true)
                .movable(!ctx.input(|i| i.modifiers.alt) && !hand_resize_active)
                .title_bar(!title_bar_hidden)
                .collapsible(false)
                .constrain_to(window_bounds)
                .frame(docked_window_frame);
            let being_moved = self
                .window_move_state
                .as_ref()
                .is_some_and(|state| state.tab_key == tab.id.key);
            if being_moved {
                // The placement click must not land in this window's content.
                window_builder = window_builder.interactable(false);
            }
            if is_hand_widget {
                let fixed_inner_size = if zone == GuiShellZone::Center {
                    initial_rect.size()
                } else {
                    Self::docked_inner_size_for_outer(ctx, initial_rect.size(), !title_bar_hidden)
                };
                window_builder = window_builder.fixed_size(fixed_inner_size).resizable(false);
            }
            let is_compact_center_widget =
                zone == GuiShellZone::Center && Self::is_compact_center_widget(&window.widget_type);
            if zone == GuiShellZone::Center && !is_compact_center_widget {
                // Prevent content-driven growth by making the window scroll instead of expanding.
                window_builder = window_builder.scroll([true, true]);
            }
            // Header/footer windows normally let egui manage their position
            // (default_pos); during a move the stored rect drives it instead.
            window_builder = if zone == GuiShellZone::Center || being_moved {
                window_builder.current_pos(initial_rect.min)
            } else {
                window_builder.default_pos(initial_rect.min)
            };
            if let Some(inner) = window_builder.show(ctx, |ui| {
                    ui.push_id(&tab.id.key, |ui| {
                        self.render_window_or_group_content(ui, &tab)
                    })
                    .inner
                }) {
                self.paint_skin_border(ctx, &tab.window_name, &inner.response);
                if is_hand_widget {
                    let handle_rect = Rect::from_min_max(
                        Pos2::new(
                            inner.response.rect.max.x - hand_resize_handle_width,
                            inner.response.rect.min.y,
                        ),
                        inner.response.rect.max,
                    );
                    if hand_resize_active
                        || ctx.input(|i| {
                            i.pointer
                                .interact_pos()
                                .or(i.pointer.latest_pos())
                                .is_some_and(|pos| handle_rect.contains(pos))
                        })
                    {
                        ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    if hand_resize_active {
                        hand_resize_delta_x += ctx.input(|i| i.pointer.delta().x);
                    }
                }
                let center_rect_changed = zone == GuiShellZone::Center
                    && ((inner.response.rect.min - initial_rect.min).length_sq() > 0.25
                        || (inner.response.rect.size() - initial_rect.size()).length_sq() > 0.25);
                // Center rects also change when clamping squeezes them into a
                // not-yet-final viewport (e.g. the first frames before the OS
                // window reaches its restored size). Persisting those would
                // clobber the saved geometry, so only track changes made while
                // the user is actually interacting with the mouse.
                let pointer_interacting =
                    ctx.input(|i| i.pointer.any_down() || i.pointer.any_released());
                let should_track_rect = if zone == GuiShellZone::Center {
                    center_rect_changed && pointer_interacting
                } else {
                    true
                };
                if should_track_rect {
                    self.track_main_window_rect(&tab.id.key, inner.response.rect, window_bounds);
                }
                if zone == GuiShellZone::Center && pointer_interacting {
                    let clamped = Self::clamp_main_window_rect(inner.response.rect, window_bounds);
                    if clamped.is_finite() {
                        self.last_center_window_rects
                            .insert(tab.id.key.clone(), Self::rect_to_snapshot(clamped));
                    }
                }
                clicked_link = inner.inner.flatten();
                zone_window_rects.push(GuiZoneWindowRect {
                    zone,
                    tab_key: tab.id.key.clone(),
                    rect: inner.response.rect,
                });
                if let Some(pointer_pos) = secondary_click_pos {
                    if inner.response.rect.contains(pointer_pos) {
                        actions.window_menu_request = Some(GuiWindowMenuRequest {
                            tab_key: tab.id.key.clone(),
                            zone,
                            allow_reorder: false,
                            title_bar_hidden,
                            position: pointer_pos,
                            window_rect: inner.response.rect,
                        });
                    }
                }
                if is_hand_widget && hand_resize_delta_x.abs() > 0.0 {
                    let resized_width =
                        (inner.response.rect.width() + hand_resize_delta_x).clamp(min_window_size.x, max_window_size.x);
                    let entry = self.main_window_rects.entry(tab.id.key.clone()).or_insert([
                        inner.response.rect.min.x,
                        inner.response.rect.min.y,
                        inner.response.rect.width(),
                        inner.response.rect.height(),
                    ]);
                    entry[2] = resized_width;
                    self.layout_dirty = true;
                }
                occupied_rects.push(inner.response.rect);
                if self.zone_drag_state.is_none() {
                    if let Some(pointer_pos) = Self::zone_drag_pointer_for_rect(
                        ctx,
                        inner.response.rect,
                        inner.response.layer_id,
                    ) {
                        self.zone_drag_state = Some(GuiZoneDragState {
                            tab_key: tab.id.key.clone(),
                            from_zone: zone,
                            pointer_pos,
                        });
                    }
                }
            }
            if let Some(click) = clicked_link {
                actions.link_clicks.push(click);
            }
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_zone_for_tab_key_assignments() {
        assert_eq!(
            VellumGuiApp::default_zone_for_tab_key(&TabKey::LeftHand),
            super::GuiShellZone::Header
        );
        assert_eq!(
            VellumGuiApp::default_zone_for_tab_key(&TabKey::Compass),
            super::GuiShellZone::Footer
        );
        assert_eq!(
            VellumGuiApp::default_zone_for_tab_key(&TabKey::TextMain),
            super::GuiShellZone::Center
        );
    }

    #[test]
    fn test_zone_for_pointer_returns_matching_zone() {
        let zone_rects = vec![
            (
                super::GuiShellZone::Header,
                Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(400.0, 100.0)),
            ),
            (
                super::GuiShellZone::Center,
                Rect::from_min_max(Pos2::new(0.0, 100.0), Pos2::new(400.0, 400.0)),
            ),
        ];

        let zone = VellumGuiApp::zone_for_pointer(&zone_rects, Pos2::new(80.0, 40.0));
        assert_eq!(zone, Some(super::GuiShellZone::Header));
    }

    #[test]
    fn test_zone_for_pointer_returns_none_outside_rects() {
        let zone_rects = vec![(
            super::GuiShellZone::Center,
            Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 300.0)),
        )];

        let zone = VellumGuiApp::zone_for_pointer(&zone_rects, Pos2::new(50.0, 50.0));
        assert_eq!(zone, None);
    }

    #[test]
    fn test_zone_drop_insert_before_uses_header_x_axis() {
        let window_rects = vec![
            super::GuiZoneWindowRect {
                zone: super::GuiShellZone::Header,
                tab_key: TabKey::Compass,
                rect: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 60.0)),
            },
            super::GuiZoneWindowRect {
                zone: super::GuiShellZone::Header,
                tab_key: TabKey::Room,
                rect: Rect::from_min_max(Pos2::new(120.0, 0.0), Pos2::new(220.0, 60.0)),
            },
        ];

        // x=130 is left of Room's center (170) but right of Compass's (50):
        // insert before Room. A y-axis mixup would return None (y=30 is at
        // both windows' center line), so this pins the axis choice too.
        let before = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::Header,
            Pos2::new(130.0, 30.0),
            &window_rects,
            &TabKey::TextMain,
        );
        assert_eq!(before, Some(TabKey::Room));

        // Past the last window's center: append at end (None).
        let after_last = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::Header,
            Pos2::new(180.0, 30.0),
            &window_rects,
            &TabKey::TextMain,
        );
        assert_eq!(after_last, None);
    }

    #[test]
    fn test_zone_drop_insert_before_uses_sidebar_y_axis() {
        let window_rects = vec![
            super::GuiZoneWindowRect {
                zone: super::GuiShellZone::LeftSidebar,
                tab_key: TabKey::Targets,
                rect: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(220.0, 120.0)),
            },
            super::GuiZoneWindowRect {
                zone: super::GuiShellZone::LeftSidebar,
                tab_key: TabKey::Players,
                rect: Rect::from_min_max(Pos2::new(0.0, 130.0), Pos2::new(220.0, 250.0)),
            },
        ];

        // y=100 is above Players' center (190) but below Targets' (60):
        // insert before Players. An x-axis mixup would return Some(Targets)
        // (x=80 is left of both centers), so this pins the axis choice too.
        let before = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::LeftSidebar,
            Pos2::new(80.0, 100.0),
            &window_rects,
            &TabKey::TextMain,
        );
        assert_eq!(before, Some(TabKey::Players));

        // Past the last window's center: append at end (None).
        let after_last = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::LeftSidebar,
            Pos2::new(80.0, 210.0),
            &window_rects,
            &TabKey::TextMain,
        );
        assert_eq!(after_last, None);
    }

    #[test]
    fn test_zone_drop_insert_before_ignores_center_zone() {
        let window_rects = vec![super::GuiZoneWindowRect {
            zone: super::GuiShellZone::Center,
            tab_key: TabKey::TextMain,
            rect: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(220.0, 120.0)),
        }];

        let before = VellumGuiApp::zone_drop_insert_before(
            super::GuiShellZone::Center,
            Pos2::new(40.0, 40.0),
            &window_rects,
            &TabKey::Room,
        );
        assert_eq!(before, None);
    }
}
