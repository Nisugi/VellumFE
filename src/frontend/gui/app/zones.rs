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

/// A zone preference for a window that isn't a live tab (hidden or not
/// yet added) — set from the Windows window's zone dropdown, keyed by
/// window name, applied when the tab materializes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct PendingZoneSnapshot {
    pub(super) window: String,
    pub(super) zone: GuiShellZone,
}

/// Display-only sidebar widths for the shell pass on a window too
/// narrow for both sidebars plus the center's minimum: shrink the
/// EXPANDED sidebars proportionally so the center keeps `min_center`
/// and the three regions can never overlap or invert. Collapsed
/// sidebars (width 0) stay collapsed — the old math floored every
/// sidebar at 220, resurrecting collapsed ones and driving
/// `center_min_x` past `center_max_x` on narrow windows
/// (gui-shell-zone-overflow-quirk). Persisted widths are the caller's
/// business and must NOT be updated from these values: a transiently
/// narrow window must not destroy the stored layout.
pub(super) fn squeezed_sidebar_widths(
    root_width: f32,
    min_center: f32,
    left: f32,
    right: f32,
) -> (f32, f32) {
    let available = (root_width - min_center).max(0.0);
    let total = left + right;
    if total <= available || total <= 0.0 {
        return (left, right);
    }
    let scale = available / total;
    (left * scale, right * scale)
}

/// Effective per-tab gaps for a legacy sidebar stack. Each tab's desired
/// `gap_above` is granted top-down out of whatever height the windows
/// leave free in the zone, so a shrinking zone collapses gaps
/// (bottom-most tabs starve first) before any window height is
/// compromised, and windows can never overlap or spill past the zone.
/// `items` are ordered `(desired_gap_above, occupied_height)` pairs.
/// Survives only for [`VellumGuiApp::bake_sidebar_stack`], which converts
/// pre-P2 gap-stack layouts into free-placement rects.
/// The order in which to raise other Center windows so `target` ends up at
/// the bottom. `ordered_middle` is every middle-order layer back-to-front
/// (egui's `layer_ids()` order); `center` is the set of layer ids that are
/// actual Center windows. We keep only the Center windows that aren't the
/// target, in their existing bottom-to-top order — raising them in that
/// order preserves their relative stacking and leaves the target lowest.
fn send_to_back_raise_order(
    ordered_middle: &[egui::Id],
    target: egui::Id,
    center: &std::collections::HashSet<egui::Id>,
) -> Vec<egui::Id> {
    ordered_middle
        .iter()
        .copied()
        .filter(|id| *id != target && center.contains(id))
        .collect()
}

/// How far the pointer must travel from its press origin before a press
/// counts as a real drag (rather than a click). Matches egui's own default
/// click/drag threshold closely enough to feel native.
const PRESS_DRAG_THRESHOLD: f32 = 6.0;

/// Whether a press on a window has become a genuine drag, and so may relax
/// that window's size pin and hand resizing to egui. A stationary click
/// (press origin == current pointer, within the threshold) must NOT relax
/// the pin: relaxing lets egui fall back to its remembered `desired_size`,
/// which — for grouped windows, whose max height is the whole zone with no
/// compact cap — snaps the window to a different height on a mere title-bar
/// click. Requiring travel keeps a click inert while leaving real
/// resize/move drags fully native.
///
/// `latched_drag` short-circuits to true once THIS window owns the
/// engagement latch and a drag has already been observed this press, so a
/// resize drag that briefly slows to a stop mid-gesture does not re-pin and
/// stall. Callers pass the window's engagement state; the pure travel test
/// is all this helper decides.
fn press_became_drag(
    press_origin: Option<egui::Pos2>,
    pointer_pos: Option<egui::Pos2>,
    latched_drag: bool,
) -> bool {
    if latched_drag {
        return true;
    }
    match (press_origin, pointer_pos) {
        (Some(origin), Some(current)) => {
            (current - origin).length() > PRESS_DRAG_THRESHOLD
        }
        _ => false,
    }
}

pub(super) fn effective_sidebar_gaps(zone_height: f32, items: &[(f32, f32)]) -> Vec<f32> {
    let occupied: f32 = items.iter().map(|(_, height)| height.max(0.0)).sum();
    let mut free = (zone_height - occupied).max(0.0);
    items
        .iter()
        .map(|(gap, _)| {
            let granted = if gap.is_finite() {
                gap.clamp(0.0, free)
            } else {
                0.0
            };
            free -= granted;
            granted
        })
        .collect()
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
    /// Range-clamp persisted dimensions only. This is what layout restore
    /// uses: at restore time the real window width is not knowable (the OS
    /// window may not exist yet, or restores maximized to a size we can't
    /// predict), and clamping against a guessed width permanently destroys
    /// persisted sidebar widths — the constructor used to pass the TUI
    /// cell-grid constant (160), which reset every sidebar to its minimum
    /// on startup. The width-aware guard runs in [`Self::sanitize`] every
    /// frame in the shell pass, where the true width is available.
    pub(super) fn clamp_ranges(&mut self) {
        self.header_height = self.header_height.clamp(96.0, 360.0);
        self.footer_height = self.footer_height.clamp(96.0, 420.0);
        self.left_sidebar_width = self.left_sidebar_width.clamp(220.0, 700.0);
        self.right_sidebar_width = self.right_sidebar_width.clamp(220.0, 700.0);
    }

    /// Per-frame sanitize: range clamps ONLY.
    ///
    /// The small-window squeeze that used to live here wrote the width-aware
    /// clamp back into the persisted `left/right_sidebar_width` every frame, so
    /// on a narrow window the stored widths were permanently shrunk and never
    /// sprang back (build_layout_snapshot then persisted the shrunk value on
    /// the next drag). The squeeze is display-only and already handled on local
    /// copies by [`squeezed_sidebar_widths`] in the shell pass — doing it here
    /// too was both redundant and the regression the "widths never written
    /// back" fix was meant to prevent. So sanitize now only range-clamps.
    pub(super) fn sanitize(&mut self) {
        self.clamp_ranges();
    }
}

#[derive(Clone, Debug)]
pub(super) struct GuiZoneDragState {
    tab_key: TabKey,
    from_zone: GuiShellZone,
    pointer_pos: Pos2,
}

/// Move mode: the window follows the cursor within its zone until a click
/// places it or Esc restores the original position. Works with the title
/// bar hidden.
#[derive(Clone, Debug)]
pub(super) struct GuiWindowMoveState {
    pub(super) tab_key: TabKey,
    /// Stored rect at move start, restored on cancel
    pub(super) original_rect: Option<[f32; 4]>,
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
    /// Where the drop happened; free-placement zones place the window
    /// there (Center restores its remembered geometry instead).
    pointer: Pos2,
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

    /// Where a window of this widget type would land by default — the
    /// widget-type mirror of `default_zone_for_tab_key`, for windows that
    /// aren't live tabs yet (the Windows window's zone dropdown).
    pub(super) fn default_zone_for_widget_type(widget_type: &str) -> GuiShellZone {
        match widget_type {
            "hand" => GuiShellZone::Header,
            "compass" | "quickbar" | "hotkeybar" | "indicator" | "minivitals" | "countdown"
            | "dashboard" | "encum" | "experience" | "gs4_experience" | "perception"
            | "injury_doll" => GuiShellZone::Footer,
            _ => GuiShellZone::Center,
        }
    }

    fn target_docked_height(&self, zone: GuiShellZone) -> Option<f32> {
        // Fill the zone's full height; render-time clamping (max_window_height)
        // keeps it within the zone, and filling avoids a bottom-edge gap.
        match zone {
            GuiShellZone::Header => {
                Some(self.shell_layout.header_height.max(MIN_DOCKED_WINDOW_HEIGHT))
            }
            GuiShellZone::Footer => {
                Some(self.shell_layout.footer_height.max(MIN_DOCKED_WINDOW_HEIGHT))
            }
            _ => None,
        }
    }

    pub(super) fn is_compact_center_widget(widget_type: &WidgetType) -> bool {
        matches!(
            widget_type,
            WidgetType::Hand
                | WidgetType::MiniVitals
                | WidgetType::Progress
                | WidgetType::Compass
                | WidgetType::Indicator
                | WidgetType::Countdown
                | WidgetType::CommandInput
        )
    }

    /// Hard height cap for single-row widgets in any docked zone. Their
    /// content never grows past one row (or one row per bar for a vertical
    /// vitals stack), so letting the frame stretch taller only manufactures
    /// empty space — and, with a skin background or border, a slab of art
    /// around one row of content. The chrome allowance is measured from the
    /// same frame the render path builds, because hand-drawn border plans
    /// and skin nine-slice art both widen the inner margin and a fixed
    /// constant would clip the row. Grouped windows stack members and
    /// manage their own height; text-like widgets (effects, entities,
    /// targets, ...) legitimately grow. None = no cap.
    fn compact_height_cap(
        &self,
        ctx: &egui::Context,
        tab_key: &TabKey,
        window: &WindowState,
        title_bar_hidden: bool,
    ) -> Option<f32> {
        use crate::frontend::gui::persistence::VitalsOrientation;
        let row = match window.widget_type {
            // Hands are freely resizable — no height cap. The icon scales to
            // fill the window (render_hand_content), so a taller window means a
            // bigger icon (2 lines, 4 lines, ...), a shorter one a small icon.
            WidgetType::Hand => return None,
            WidgetType::Countdown
            | WidgetType::Progress
            | WidgetType::Indicator
            | WidgetType::CommandInput => 28.0,
            WidgetType::MiniVitals => {
                let vitals = &self.ui_settings.vitals;
                let bar = vitals.bar_height.clamp(8.0, 60.0);
                match vitals.orientation {
                    VitalsOrientation::Horizontal => bar,
                    VitalsOrientation::Vertical => {
                        let rows = vitals.bars.len().max(1) as f32;
                        rows * bar + (rows - 1.0) * 6.0
                    }
                }
            }
            // A dashboard's height is its row count × the icon row, so the
            // frame hugs the grid instead of leaving a slab below the last
            // row. Rows come from the config (indicator count + layout): a
            // vertical stack is N rows, a grid is ceil(N/cols), horizontal is
            // one. Flow wraps by width — not knowable here — so it stays
            // uncapped (grows as content needs). Empty/absent config: 1 row.
            WidgetType::Dashboard => {
                use crate::config::DashboardLayout;
                let data = self
                    .app_core
                    .layout
                    .windows
                    .iter()
                    .find(|def| def.name() == window.name)
                    .and_then(|def| match def {
                        crate::config::WindowDef::Dashboard { data, .. } => Some(data),
                        _ => None,
                    });
                let Some(data) = data else { return None };
                let count = data.cell_count().max(1);
                let rows = match DashboardLayout::from_str(&data.layout) {
                    DashboardLayout::Flow => return None,
                    DashboardLayout::Horizontal => 1,
                    DashboardLayout::Vertical => count,
                    DashboardLayout::Grid { cols, .. } => count.div_ceil(cols.max(1)),
                };
                let icon_row = 24.0;
                let spacing = data.spacing as f32 * icon_row * 0.35;
                rows as f32 * icon_row + (rows.saturating_sub(1) as f32) * spacing
            }
            _ => return None,
        };
        let mut frame = egui::Frame::window(ctx.global_style().as_ref());
        self.apply_border_plan_to_frame(&self.window_border_plan_for_tab(tab_key), &mut frame);
        self.apply_skin_border_to_frame(
            tab_key,
            self.skin_border_sides_for_tab(tab_key),
            &mut frame,
        );
        let margins = f32::from(frame.inner_margin.top) + f32::from(frame.inner_margin.bottom);
        let title_bar = if title_bar_hidden {
            0.0
        } else {
            // None/0 = "derive from the title font"; 22 is a generous stand-in.
            self.title_bar_height_for_tab(tab_key)
                .filter(|height| *height > 0.0)
                .unwrap_or(22.0)
                + 6.0
        };
        // A few spare pixels so the row never clips; they are invisible.
        Some(row + margins + title_bar + 6.0)
    }

    fn min_window_height_for_zone(zone: GuiShellZone, window: &WindowState) -> f32 {
        // Center and the sidebars share the free-placement minimums;
        // header/footer strips accept anything down to the docked floor.
        // Dashboards are content-height (capped to their row count), so they
        // get the docked floor too — otherwise the 90px text-window minimum
        // leaves irreducible padding below a short grid.
        if matches!(zone, GuiShellZone::Header | GuiShellZone::Footer)
            || Self::is_compact_center_widget(&window.widget_type)
            || matches!(window.widget_type, WidgetType::Dashboard)
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
            self.tab_zones.insert(key.clone(), zone);
            if let Some(target_height) = self.target_docked_height(zone) {
                // Header/footer: append after the zone's right-most window
                // in real screen coords (the render clamp pulls the rect
                // into the strip), filling the strip's height.
                let after = self
                    .tab_zones
                    .iter()
                    .filter(|(other, assigned)| **assigned == zone && **other != key)
                    .filter_map(|(other, _)| self.main_window_rects.get(other))
                    .map(|rect| rect[0] + rect[2])
                    .filter(|value| value.is_finite())
                    .fold(0.0f32, f32::max);
                let entry = self
                    .main_window_rects
                    .entry(key.clone())
                    .or_insert([after + 4.0, 0.0, 240.0, target_height]);
                entry[0] = after + 4.0;
                entry[1] = 0.0;
                entry[3] = target_height;
            }
            if matches!(zone, GuiShellZone::LeftSidebar | GuiShellZone::RightSidebar) {
                // Sidebars are free-placement (P2): place the newcomer just
                // below the zone's lowest window in real screen coords (the
                // render clamp pulls the rect into the pane), defaulting to
                // the zone's full width.
                let below = self
                    .tab_zones
                    .iter()
                    .filter(|(other, assigned)| **assigned == zone && **other != key)
                    .filter_map(|(other, _)| self.main_window_rects.get(other))
                    .map(|rect| rect[1] + rect[3])
                    .filter(|value| value.is_finite())
                    .fold(0.0f32, f32::max);
                let zone_width = match zone {
                    GuiShellZone::LeftSidebar => self.shell_layout.left_sidebar_width,
                    _ => self.shell_layout.right_sidebar_width,
                };
                let entry = self
                    .main_window_rects
                    .entry(key.clone())
                    .or_insert([16.0, below + 4.0, zone_width, 160.0]);
                entry[1] = below + 4.0;
                entry[3] = entry[3].clamp(40.0, 600.0);
            }
            self.layout_dirty = true;
        }
    }

    pub(super) fn apply_zone_drop(
        &mut self,
        drop_result: GuiZoneDropResult,
        zone_rects: &[(GuiShellZone, Rect)],
    ) {
        let GuiZoneDropResult {
            tab_key,
            target_zone,
            pointer,
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
            return;
        }

        // Free-placement zones: the drop point IS the placement, grabbed
        // top-center like Move mode. Size comes from the entry set_tab_zone
        // just wrote (prior size where one existed, zone-shaped defaults
        // otherwise), clamped into the target pane.
        let zone_rect = zone_rects
            .iter()
            .find_map(|(zone, rect)| (*zone == target_zone).then_some(*rect));
        let stored = self.main_window_rects.get(&tab_key).copied();
        let width = stored
            .map(|rect| rect[2])
            .filter(|value| value.is_finite())
            .unwrap_or(240.0)
            .max(60.0);
        let height = stored
            .map(|rect| rect[3])
            .filter(|value| value.is_finite())
            .unwrap_or(160.0)
            .max(24.0);
        let target = Rect::from_min_size(
            Pos2::new(pointer.x - width * 0.5, pointer.y - 10.0),
            Vec2::new(width, height),
        );
        let placed = match zone_rect {
            Some(bounds) => Self::clamp_main_window_rect(target, bounds),
            None => target,
        };
        if placed.is_finite() {
            self.main_window_rects
                .insert(tab_key, Self::rect_to_snapshot(placed));
        }
        self.layout_dirty = true;
    }

    pub(super) fn title_bar_hidden(&self, key: &TabKey) -> bool {
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

    /// The egui `Id` of the window drawn for `tab_key` in `zone`. This is the
    /// single source of the formula — the render pass and the send-to-back
    /// logic both go through here so they can never drift apart.
    pub(super) fn zone_window_id(zone: GuiShellZone, tab_key: &TabKey) -> egui::Id {
        egui::Id::new(("gui_zone_window", zone.id_fragment(), tab_key))
    }

    /// Send the window for `tab_key` behind the windows it overlaps within
    /// its zone. egui has no move-to-bottom, so instead we raise every
    /// *other* window of that zone above it, preserving their existing
    /// relative order — which leaves this one at the bottom of the stack.
    /// Works in every zone; all five are free-placement now. Live-session
    /// only.
    pub(super) fn send_window_to_back(&mut self, ctx: &egui::Context, tab_key: &TabKey) {
        let zone = self.zone_for_tab(tab_key);
        let target = Self::zone_window_id(zone, tab_key);
        // Layer ids of this zone's windows, keyed for a quick membership
        // test; windows in other zones can't overlap this one and stay out.
        let center_layers: std::collections::HashSet<egui::Id> = self
            .available_tabs
            .keys()
            .filter(|key| self.zone_for_tab(key) == zone)
            .map(|key| Self::zone_window_id(zone, key))
            .collect();
        // layer_ids() is back-to-front (top is last).
        let ordered_middle: Vec<egui::Id> = ctx.memory(|mem| {
            mem.layer_ids()
                .filter(|layer| layer.order == egui::Order::Middle)
                .map(|layer| layer.id)
                .collect()
        });
        // Raising each other Center window in bottom-to-top order preserves
        // their relative stacking while pushing them all above the target.
        for id in send_to_back_raise_order(&ordered_middle, target, &center_layers) {
            ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, id));
        }
        ctx.request_repaint();
    }

    /// Map every live main-surface tab to the egui layer id its window renders
    /// under, across all zones. The inverse of `zone_window_id` (which hashes,
    /// so it can't be reversed) — built forward so a layer id from
    /// `mem.layer_ids()` can be resolved back to a TabKey.
    fn surface_layer_to_tab(&self) -> HashMap<egui::Id, TabKey> {
        self.available_tabs
            .keys()
            .filter(|key| {
                !self.hidden_tabs.contains(key) && !self.detached_tabs.contains_key(key)
            })
            .map(|key| (Self::zone_window_id(self.zone_for_tab(key), key), key.clone()))
            .collect()
    }

    /// Cache the live front-to-back stacking order (topmost last) of the
    /// main-surface windows from egui's layer order. Read by the save snapshot
    /// so `visible_tabs` records true z-order. Only refreshes when the order
    /// actually changed, to avoid churning `current_zorder` every frame.
    pub(super) fn refresh_zorder_cache(&mut self, ctx: &egui::Context) {
        let layer_to_tab = self.surface_layer_to_tab();
        // layer_ids() is back-to-front (top is last) — the order we persist.
        let ordered: Vec<TabKey> = ctx.memory(|mem| {
            mem.layer_ids()
                .filter(|layer| layer.order == egui::Order::Middle)
                .filter_map(|layer| layer_to_tab.get(&layer.id).cloned())
                .collect()
        });
        // A frame before any surface window has been laid out yields nothing;
        // don't clobber a good cache with an empty read.
        if !ordered.is_empty() && ordered != self.current_zorder {
            self.current_zorder = ordered;
        }
    }

    /// Replay a saved stacking order: raise each window in back-to-front order
    /// so the last-listed ends up on top. Keys not currently on the main
    /// surface (hidden/detached/absent) are skipped. Deferred one frame from
    /// the load because the windows must exist as layers first.
    pub(super) fn apply_stacking_order(&mut self, ctx: &egui::Context, order: &[TabKey]) {
        let mut raised_any = false;
        for key in order {
            if self.hidden_tabs.contains(key)
                || self.detached_tabs.contains_key(key)
                || !self.available_tabs.contains_key(key)
            {
                continue;
            }
            let layer = egui::LayerId::new(
                egui::Order::Middle,
                Self::zone_window_id(self.zone_for_tab(key), key),
            );
            ctx.move_to_top(layer);
            raised_any = true;
        }
        if raised_any {
            // Seed the cache so a save right after load re-persists this order
            // even before the next capture pass runs.
            self.current_zorder = order.to_vec();
            ctx.request_repaint();
        }
    }

    /// Raise a single main-surface window to the front (switch_current_window).
    /// Unlike `apply_stacking_order` this does NOT overwrite `current_zorder`;
    /// the next frame's `refresh_zorder_cache` reads the resulting order.
    pub(super) fn raise_tab_to_front(&mut self, ctx: &egui::Context, key: &TabKey) {
        if self.hidden_tabs.contains(key)
            || self.detached_tabs.contains_key(key)
            || !self.available_tabs.contains_key(key)
        {
            return;
        }
        let layer = egui::LayerId::new(
            egui::Order::Middle,
            Self::zone_window_id(self.zone_for_tab(key), key),
        );
        ctx.move_to_top(layer);
        ctx.request_repaint();
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
                    .unwrap_or(window.position.y.get() as f32);
                let saved_x = self
                    .main_window_rects
                    .get(key)
                    .and_then(|rect| rect.get(0).copied())
                    .filter(|v| v.is_finite())
                    .unwrap_or(window.position.x.get() as f32);
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
            max_col = max_col
                .max((window.position.x.get() + window.position.width.get()).max(1) as f32);
            max_row = max_row
                .max((window.position.y.get() + window.position.height.get()).max(1) as f32);
        }
        (max_col.max(1.0), max_row.max(1.0))
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

        let left =
            root_rect.left() + (window.position.x.get() as f32 / max_col) * root_rect.width();
        let top = root_rect.top() + (window.position.y.get() as f32 / max_row) * root_rect.height();
        let width = ((window.position.width.get() as f32 / max_col) * root_rect.width()).max(120.0);
        let height = ((window.position.height.get() as f32 / max_row) * root_rect.height())
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
                    // Within-zone moves are plain drags now; Alt+drag is
                    // only the between-zones gesture.
                    format!("Already in {} — release to cancel", zone.label())
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
                if target_zone == drag.from_zone {
                    return None;
                }
                return Some(GuiZoneDropResult {
                    tab_key: drag.tab_key,
                    target_zone,
                    pointer: pointer_pos,
                });
            }
        }
        None
    }

    /// Drive Move mode: the window follows the cursor within its zone —
    /// every zone is free-placement now. A click commits, Esc restores the
    /// starting rect. Runs after the zone surfaces so it sees this frame's
    /// input; a full-screen catcher swallows pointer interactions so the
    /// placement click can't reach any window content.
    pub(super) fn render_window_move_overlay(
        &mut self,
        ctx: &egui::Context,
        zone_rects: &[(GuiShellZone, Rect)],
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

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            match state.original_rect {
                Some(rect) => {
                    self.main_window_rects.insert(state.tab_key.clone(), rect);
                }
                None => {
                    self.main_window_rects.remove(&state.tab_key);
                }
            }
            self.window_move_state = None;
            return;
        }

        ctx.set_cursor_icon(egui::CursorIcon::Move);
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos().or(i.pointer.latest_pos()));
        if let Some(pos) = pointer_pos {
            if let Some(stored) = self.main_window_rects.get(&state.tab_key).copied() {
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

    /// Bake a sidebar's legacy gap-stack layout into per-window rects.
    ///
    /// One-time conversion (zone-free-movement-plan P2): computes the
    /// exact slots the pre-P2 fixed-stack sidebar renderer would have
    /// shown — same metrics, same gap grants out of the free space — and
    /// writes them into `main_window_rects`, consuming the zone's
    /// `sidebar_gap_above` entries. Layouts saved before the conversion
    /// carry order-encoding y values and slot-ignored x/width in their
    /// sidebar rects, so the on-screen stack MUST be reconstructed from
    /// the stack math, not read from the stored rects.
    ///
    /// Must never run on an already-converted zone (it would re-stack the
    /// user's free arrangement): `migrated_sidebar_zones` gates it within
    /// a session and persists in the layout snapshot across sessions.
    fn bake_sidebar_stack(&mut self, ctx: &egui::Context, zone: GuiShellZone, root_rect: Rect) {
        let detached = self.detached_tab_keys();
        let tabs = self.zone_surface_tabs(&detached, zone);
        let margin = 0.0;
        let gap = 4.0;
        let slot_width = (root_rect.width() - margin * 2.0).max(120.0);
        // Per-widget metrics, mirroring the deleted stack renderer's
        // prepass: compact one-line widgets get small slots, text-like
        // windows real ones, and the frame-aware cap keeps a single-row
        // widget from reserving a phantom slab.
        let tab_metrics: Vec<(TabKey, f32, f32)> = tabs
            .iter()
            .map(|tab| {
                let window = self.app_core.ui_state.windows.get(&tab.window_name);
                let compact = window
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
                let height_cap = window.and_then(|window| {
                    self.compact_height_cap(
                        ctx,
                        &tab.id.key,
                        window,
                        self.title_bar_hidden(&tab.id.key),
                    )
                });
                let desired_height = self
                    .main_window_rects
                    .get(&tab.id.key)
                    .map(|rect| rect[3])
                    .filter(|v| v.is_finite())
                    .unwrap_or(default_height);
                let desired_height = match height_cap {
                    Some(cap) => desired_height.min(cap.max(min_height)),
                    None => desired_height,
                };
                (tab.id.key.clone(), min_height, desired_height)
            })
            .collect();
        let mut remaining_min: f32 = tab_metrics
            .iter()
            .map(|(_, min_height, _)| min_height + gap)
            .sum();
        let zone_inner_height = (root_rect.height() - margin * 2.0).max(0.0);
        let stack_items: Vec<(f32, f32)> = tab_metrics
            .iter()
            .enumerate()
            .map(|(index, (key, min_height, desired_height))| {
                let desired_gap = self
                    .sidebar_gap_above
                    .get(key)
                    .copied()
                    .filter(|value| value.is_finite())
                    .unwrap_or(0.0)
                    .max(0.0);
                let trailing_gap = if index + 1 == tab_metrics.len() {
                    0.0
                } else {
                    gap
                };
                (desired_gap, desired_height.max(*min_height) + trailing_gap)
            })
            .collect();
        let effective_gaps = effective_sidebar_gaps(zone_inner_height, &stack_items);
        let mut y = root_rect.min.y + margin;
        let mut changed = false;
        for (index, (key, min_height, desired_height)) in tab_metrics.iter().enumerate() {
            remaining_min -= min_height + gap;
            y += effective_gaps.get(index).copied().unwrap_or(0.0);
            let max_height_here =
                (root_rect.max.y - margin - y - remaining_min).max(*min_height);
            let slot_height = desired_height.clamp(*min_height, max_height_here);
            let slot_bottom = (y + slot_height).min(root_rect.max.y - margin - remaining_min);
            // Unlike the old renderer (which skipped windows whose slot
            // collapsed off the bottom), every window gets a rect: free
            // placement needs one, and the render clamp pulls it back on
            // screen.
            let slot_bottom = slot_bottom.max(y + MIN_DOCKED_WINDOW_HEIGHT);
            let rect = [root_rect.min.x + margin, y, slot_width, slot_bottom - y];
            let differs = self.main_window_rects.get(key).is_none_or(|stored| {
                stored
                    .iter()
                    .zip(rect.iter())
                    .any(|(a, b)| (a - b).abs() > 0.5)
            });
            if differs {
                self.main_window_rects.insert(key.clone(), rect);
                changed = true;
            }
            y = slot_bottom + gap;
        }
        // Consume every gap entry for tabs assigned to this zone (hidden
        // ones included); nothing reads them after the bake.
        let zone_keys: Vec<TabKey> = self
            .tab_zones
            .iter()
            .filter(|(_, assigned)| **assigned == zone)
            .map(|(key, _)| key.clone())
            .collect();
        for key in zone_keys {
            changed |= self.sidebar_gap_above.remove(&key).is_some();
        }
        if changed {
            self.layout_dirty = true;
        }
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

        if is_sidebar && !self.migrated_sidebar_zones.contains(&zone) {
            // Legacy gap-stack conversion (zone-free-movement-plan P2):
            // the first pass over a sidebar after a layout apply bakes the
            // slots the old fixed-stack renderer would have shown into
            // per-window rects and consumes the persisted gaps. From then
            // on sidebar windows flow through the shared free-placement
            // loop below like every other zone.
            self.bake_sidebar_stack(ctx, zone, root_rect);
            self.migrated_sidebar_zones.insert(zone);
        }

        let window_bounds = if zone == GuiShellZone::Center {
            root_rect.shrink(1.0)
        } else {
            root_rect
        };
        if !window_bounds.is_finite() || window_bounds.width() <= 8.0 || window_bounds.height() <= 8.0 {
            return actions;
        }

        // Clicks anywhere count as "interacting"; used both for rect
        // tracking (only user actions persist geometry) and for relaxing
        // the per-window size forcing below.
        let pointer_interacting =
            ctx.input(|i| i.pointer.any_down() || i.pointer.any_released());
        // Where the current press started, for telling "user is engaging
        // this window" apart from "user clicked a toolbar toggle".
        let press_origin = ctx.input(|i| i.pointer.press_origin());
        // Current pointer position, paired with the press origin to tell a
        // stationary click apart from a real drag (see `press_became_drag`).
        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        // The engagement latch lives for one press; release clears it.
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        // True only on the frame the primary button goes down — used to raise a
        // window whose resize edge sits under an overlapping neighbor, so the
        // grabbed handle wins the interaction instead of the neighbor on top.
        let just_pressed = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        if !pointer_down {
            self.zone_engaged_tab = None;
            // The snap drag must survive the RELEASE frame: the hook's
            // final pass writes the snapped rect as the drop position and
            // drops the state itself. This is only the backstop for a
            // release the owning zone's pass never got to process.
            if !ctx.input(|i| i.pointer.any_released()) {
                self.zone_snap_drag = None;
            }
        }

        // Center windows render at *display* rects computed from their
        // canonical rects and the current bounds: shell zones claiming
        // space displace windows for the frame (story window shrinks,
        // others push) and everything springs back when the zone closes,
        // because `main_window_rects` itself is never touched.
        let center_displays: HashMap<TabKey, Rect> = if zone == GuiShellZone::Center {
            let mut infos: Vec<super::dock::CenterWindowInfo> = Vec::new();
            for tab in &tabs {
                let Some(window) = self.app_core.ui_state.windows.get(&tab.window_name) else {
                    continue;
                };
                let group = self.group_for_tab(&tab.id.key);
                // Mirrors the min-size computation in the render loop below.
                let min_height = {
                    let base = Self::min_window_height_for_zone(zone, window);
                    match group.map(|g| (g.members.len(), g.horizontal)) {
                        Some((count, false)) => base * count as f32,
                        _ => base,
                    }
                };
                let min_size = Vec2::new(
                    120.0_f32.min(window_bounds.width().max(1.0)),
                    min_height.min(window_bounds.height().max(1.0)),
                );
                let Some(stored) = self
                    .main_window_rects
                    .get(&tab.id.key)
                    .copied()
                    .and_then(Self::rect_from_snapshot)
                else {
                    // No canonical rect yet: the loop's fallback placement
                    // already lives inside the current bounds.
                    continue;
                };
                let is_main = tab.id.key == TabKey::TextMain
                    || group.is_some_and(|g| g.members.contains(&TabKey::TextMain));
                infos.push(super::dock::CenterWindowInfo {
                    key: tab.id.key.clone(),
                    stored,
                    min_size,
                    is_main,
                });
            }
            Self::compute_center_display_rects(&infos, window_bounds)
        } else {
            HashMap::new()
        };

        // Snap candidates: every window's on-screen rect and title, scoped
        // to THIS zone — snapping matches what the user sees, and a header
        // window never snaps against a center sibling it can't reach. For
        // Center that is the display rect (displacement applied); for
        // header/footer it mirrors the position feed (canonical clamped to
        // the pane, fallback otherwise) with the compact height cap applied
        // so a hand's phantom stored height is not a snap target. Guides
        // live for one frame; each non-sidebar pass clears them and only
        // the pass owning the live drag repopulates (and paints) them, so
        // they can never leak into another zone's pane.
        self.zone_snap_guides.clear();
        let snap_suspended = ctx.input(|i| i.modifiers.shift);
        let snap_siblings: Vec<(TabKey, String, Rect)> = if self.ui_settings.snap_enabled {
            if zone == GuiShellZone::Center {
                tabs.iter()
                    .filter_map(|tab| {
                        center_displays.get(&tab.id.key).map(|rect| {
                            (tab.id.key.clone(), self.window_display_title(tab), *rect)
                        })
                    })
                    .collect()
            } else {
                tabs.iter()
                    .filter_map(|tab| {
                        let window = self.app_core.ui_state.windows.get(&tab.window_name)?;
                        let mut rect = self
                            .main_window_rects
                            .get(&tab.id.key)
                            .copied()
                            .and_then(Self::rect_from_snapshot)
                            .map(|rect| Self::clamp_main_window_rect(rect, window_bounds))
                            .or_else(|| {
                                Self::tab_window_rect(window_bounds, layout_bounds, window)
                            })?;
                        let grouped = self
                            .group_for_tab(&tab.id.key)
                            .is_some_and(|group| group.members.len() > 1);
                        if !grouped {
                            if let Some(cap) = self.compact_height_cap(
                                ctx,
                                &tab.id.key,
                                window,
                                self.title_bar_hidden(&tab.id.key),
                            ) {
                                let capped =
                                    rect.height().min(cap.max(MIN_DOCKED_WINDOW_HEIGHT));
                                rect.set_height(capped);
                            }
                        }
                        Some((tab.id.key.clone(), self.window_display_title(tab), rect))
                    })
                    .collect()
            }
        } else {
            Vec::new()
        };

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
            let title_bar_hidden = self.title_bar_hidden(&tab.id.key);
            let window_locked = self.window_locked(&tab.id.key);
            let grouped = group_shape.is_some_and(|(count, _)| count > 1);
            // Docked text-like windows fill their zone's full height — no
            // reserved headroom, which otherwise left a gap at the bottom edge.
            // Single-row widgets are capped in every zone so they can't be
            // stretched into empty space around their one row of content.
            let max_window_height = {
                let zone_max = window_bounds.height().max(min_window_size.y);
                let cap = if grouped {
                    None
                } else {
                    self.compact_height_cap(ctx, &tab.id.key, window, title_bar_hidden)
                };
                match cap {
                    Some(cap) => cap.clamp(min_window_size.y, zone_max),
                    None => zone_max,
                }
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
            let is_compact_widget = Self::is_compact_center_widget(&window.widget_type);
            let mut initial_rect = if zone == GuiShellZone::Center {
                center_displays
                    .get(&tab.id.key)
                    .copied()
                    .unwrap_or_else(|| Self::clamp_main_window_rect(fallback_rect, window_bounds))
            } else {
                self.main_window_rects
                    .get(&tab.id.key)
                    .copied()
                    .and_then(Self::rect_from_snapshot)
                    .map(|rect| Self::clamp_main_window_rect(rect, window_bounds))
                    .unwrap_or(fallback_rect)
            };
            if !initial_rect.is_finite() {
                continue;
            }
            // Fold the live height cap into the fed rect for compact
            // single-row widgets: egui renders them capped (max_size), so a
            // taller stored/canonical height would leave the fed rect and
            // egui's reported rect disagreeing on the Y axis for the whole
            // session. That desync made `classify_axis` (which measures
            // gesture totals against the fed rect) see a phantom vertical
            // delta and draw guides for edges the user never dragged. Cap
            // here and the two rects agree at gesture start. Grouped windows
            // manage their own (uncapped) height, matching the sibling
            // candidate list above.
            if is_compact_widget && !grouped {
                let capped = initial_rect.height().min(max_window_size.y);
                initial_rect.set_height(capped);
            }

            let mut clicked_link = None;
            // WebUI windows get a title-bar close button: unlike layout
            // widgets (hidden/restored via the Windows menu), script pages
            // are transient - closing one removes it and unsubscribes.
            let is_webui_window = matches!(window.content, WindowContent::WebUi(_));
            let window_id = Self::zone_window_id(zone, &tab.id.key);
            let mut docked_window_frame = egui::Frame::window(ctx.global_style().as_ref())
                .outer_margin(egui::Margin::ZERO)
                .shadow(egui::epaint::Shadow::NONE);
            if let Some(accent) = self.accent_color_for_tab(&tab.id.key) {
                docked_window_frame.stroke.color = accent;
            }
            if let Some(radius) = self.corner_radius_override_for_tab(&tab.id.key) {
                docked_window_frame.corner_radius =
                    egui::CornerRadius::same(radius.clamp(0.0, 12.0).round() as u8);
            }
            let border_plan = self.window_border_plan_for_tab(&tab.id.key);
            self.apply_border_plan_to_frame(&border_plan, &mut docked_window_frame);
            let skin_sides = self.skin_border_sides_for_tab(&tab.id.key);
            self.apply_skin_border_to_frame(&tab.id.key, skin_sides, &mut docked_window_frame);
            // `default_size` (like `fixed_size`) is the whole window rect in
            // this egui fork, so every zone passes the outer size directly.
            // Declared before the builder so the close-button borrow
            // (`Window::open`) outlives it.
            let mut webui_open = true;
            let mut window_builder = egui::Window::new(self.window_display_title(&tab))
                .id(window_id)
                .default_size(initial_rect.size())
                .min_size(min_window_size)
                .max_size(max_window_size)
                .resizable(!window_locked)
                .movable(!ctx.input(|i| i.modifiers.alt) && !window_locked)
                // Drag from anywhere — body and title bar alike — so every
                // move goes through the anchored area-move path. Title-bar
                // drag mode routes through a separate pre-`Area::begin`
                // per-frame delta hand-off that loses against the canonical
                // position feed, leaving titled windows unmovable/unsnappable.
                // Interactive content (text selection, links, scrollbars)
                // still wins hit-testing over the body drag.
                .drag_area(egui::WindowDrag::Anywhere)
                .title_bar(!title_bar_hidden)
                .collapsible(false)
                .constrain_to(window_bounds)
                .frame(docked_window_frame);
            window_builder = self.style_window_title_bar(&tab.id.key, window_builder);
            let being_moved = self
                .window_move_state
                .as_ref()
                .is_some_and(|state| state.tab_key == tab.id.key);
            if being_moved {
                // The placement click must not land in this window's content.
                window_builder = window_builder.interactable(false);
            }
            if !is_compact_widget {
                // Prevent content-driven growth by making the window scroll instead of expanding.
                window_builder = window_builder.scroll([true, true]);
            }
            // Pin the window to its display size whenever the user is not
            // engaging it: egui's Resize state clamps its remembered
            // desired_size through min/max every frame, so this both makes
            // the story window actually shrink while a zone is open and
            // grows it (and any window egui clamped to a small center)
            // right back afterwards. A press that started on or near this
            // window (resize handles included) relaxes the pin so drags
            // behave normally — and the drag's tracking then updates the
            // canonical rect the display derives from. Engagement LATCHES
            // for the whole press (zone_engaged_tab): a shrink drag pulls
            // the grabbed edge away from the press origin, and re-testing
            // the origin against the shrinking rect would re-pin the size
            // mid-drag, stalling the resize after ~12px per grab.
            // Claim the engagement latch only if no window has claimed it this
            // press. Overlapping edge bands mean a press near a shared border
            // is inside BOTH windows' expand(12) rings; without the
            // "unclaimed" guard the later-rendered window stole the latch, the
            // earlier one re-pinned, and its resize stalled after ~12px (grab
            // again for another ~12px — the reported symptom). First claim
            // wins and holds for the whole press; the raise-on-edge-grab below
            // puts the actually-grabbed window on top so egui resizes the same
            // one that holds the latch.
            // The latch is claimed post-show (topmost-at-press, egui-correct);
            // here we only READ it to relax the pin for the latched window, or
            // to catch a press on a non-overlapping window on its first frame
            // (the common case, where topmost is unambiguous anyway). The
            // post-show claim corrects any ambiguous edge-band press.
            let engaging_press = press_origin
                .is_some_and(|pos| initial_rect.expand(12.0).contains(pos));
            let already_latched = self.zone_engaged_tab.as_ref() == Some(&tab.id.key);
            let user_engaging_window = !window_locked
                && pointer_interacting
                && (already_latched || (self.zone_engaged_tab.is_none() && engaging_press));
            // The size pin only relaxes once the press becomes a real drag —
            // a stationary title-bar click keeps the pin, so egui can't snap
            // the window (grouped windows especially, whose max height is the
            // whole zone) to its remembered desired_size. `user_engaging_window`
            // still gates position feed and rect tracking below; this narrower
            // gate governs the SIZE pin alone.
            let relax_size_pin = user_engaging_window
                && press_became_drag(press_origin, pointer_pos, already_latched);
            if !being_moved && !relax_size_pin {
                // Pin every window to its display size when the user isn't
                // engaging it: egui's Resize state re-clamps its remembered
                // desired_size each frame, so without this a release-snap's new
                // size (or a .loadlayout restore) wouldn't stick. Compact
                // widgets carry their capped height in `initial_rect`; hands
                // (now freely resizable) pin to whatever size the user set.
                window_builder = window_builder
                    .min_size(initial_rect.size())
                    .max_size(initial_rect.size());
            }
            // The canonical rect drives the position every frame, in every
            // zone: egui's remembered position must never win, or a
            // `.loadlayout` restore leaves the window wherever it was
            // (`default_pos` is ignored once a window exists).
            //
            // EXCEPT while the user is engaging the window: egui does not
            // accept an externally fed position mid-gesture — a move drag
            // anchors to drag-start + total pointer delta and ignores the
            // feed, and left/top resizes (position + size) get yanked
            // back each frame with the next delta applied on top, which is
            // why only size-only handles behaved in beta.21 (and why
            // header/footer left/top resizes stayed broken until this gate
            // covered them too). During the gesture egui owns the rect
            // (every handle native by construction); the engagement-gated
            // tracking below adopts the (snapped) result into the
            // canonical map, and the feed resumes on release — gluing the
            // window to any engaged snap.
            if !(user_engaging_window && !being_moved) {
                window_builder = window_builder.current_pos(initial_rect.min);
            }
            if is_webui_window && !title_bar_hidden {
                window_builder = window_builder.open(&mut webui_open);
            }
            // Per-window render cost for the performance monitor (chrome +
            // content; detached viewports are not timed).
            let window_render_start = std::time::Instant::now();
            let window_shown = window_builder.show(ctx, |ui| {
                ui.push_id(&tab.id.key, |ui| {
                    self.render_window_or_group_content(ui, &tab)
                })
                .inner
            });
            self.app_core
                .perf_stats
                .record_window_render(&tab.window_name, window_render_start.elapsed());
            if let Some(inner) = window_shown {
                self.paint_skin_border(ctx, &tab.id.key, skin_sides, &inner.response);
                self.paint_border_plan(ctx, &border_plan, &inner.response);
                // Claim the engagement latch here, where the real rendered rect
                // and layer id are known, gated on this window being TOPMOST at
                // the press origin — i.e. the window egui actually resizes. The
                // pre-show `engaging_press` test can't tell overlapping windows
                // apart (both contain the press in their edge ring), which let
                // the wrong window latch, re-pin the resized one, and stall it
                // after ~12px. Topmost-at-press is exactly egui's own choice,
                // so the latch and egui's resize target always agree.
                if just_pressed && !window_locked && self.zone_engaged_tab.is_none() {
                    const RESIZE_GRAB: f32 = 6.0;
                    let rect = inner.response.rect;
                    if let Some(pos) = press_origin {
                        // The press hits this window's body or its resize ring
                        // (the edge band extends a few px outside the frame).
                        let in_window = rect.expand(RESIZE_GRAB).contains(pos);
                        let topmost = ctx.layer_id_at(pos) == Some(inner.response.layer_id);
                        if in_window && topmost {
                            self.zone_engaged_tab = Some(tab.id.key.clone());
                            ctx.request_repaint();
                        }
                    }
                }
                // `.snapdebug`: the three rects whose divergence explains
                // every "can't snap to window X" report — the canonical
                // (candidate source), the display rect fed to egui, and
                // what egui actually rendered (title bar, pins, caps).
                if self.snap_debug && pointer_interacting {
                    let rendered = inner.response.rect;
                    tracing::info!(
                        "snapdbg win {:?} zone={:?} title_hidden={} canon={:?} display=[{:.1} {:.1} {:.1} {:.1}] rendered=[{:.1} {:.1} {:.1} {:.1}]",
                        tab.id.key,
                        zone,
                        title_bar_hidden,
                        self.main_window_rects.get(&tab.id.key),
                        initial_rect.min.x,
                        initial_rect.min.y,
                        initial_rect.max.x,
                        initial_rect.max.y,
                        rendered.min.x,
                        rendered.min.y,
                        rendered.max.x,
                        rendered.max.y,
                    );
                }
                let rect_changed = (inner.response.rect.min - initial_rect.min).length_sq() > 0.25
                    || (inner.response.rect.size() - initial_rect.size()).length_sq() > 0.25;
                // Only geometry the user changed by grabbing THIS window may
                // become canonical. Rendered rects also diverge when a shell
                // zone displaces the window, when clamping squeezes it into a
                // not-yet-final viewport, and right after `.loadlayout`
                // replaces the canonical map — tracking those on a mere
                // click-anywhere (the old gate) baked the displaced rect in,
                // so windows never sprang back when the zone closed and a
                // loaded layout was overwritten by the on-screen geometry.
                // Track only when the press became a real drag (resize or
                // move): a stationary click never relaxes the pin, so any rect
                // divergence on a click is egui noise, not user intent, and
                // must not be baked in.
                let should_track_rect = rect_changed && relax_size_pin;
                // While a snap drag is live the hook runs every frame — even
                // when the pointer holds still (guides must not flicker off)
                // and on the release frame, where the engagement gate is
                // already down but the drag's final snapped rect still has
                // to be written as the drop position.
                let snap_drag_live = self
                    .zone_snap_drag
                    .as_ref()
                    .is_some_and(|drag| drag.tab_key == tab.id.key);
                // Compact single-row widgets are height-derived: their
                // canonical height is always the live cap, never a stored
                // value. Normalizing here — the one choke point where a rect
                // becomes canonical — means `.savelayout`, the snap candidate
                // list, and next frame's fed rect all read the same capped
                // height. Nothing downstream can resurrect a stale height
                // (e.g. a layout saved under a larger icon-size setting).
                let compact_derived = is_compact_widget && !grouped;
                let normalize_height = |mut rect: Rect| -> Rect {
                    if compact_derived {
                        rect.set_height(rect.height().min(max_window_size.y));
                    }
                    rect
                };
                if !being_moved && (snap_drag_live || (rect_changed && relax_size_pin)) {
                    let tracked = self.apply_zone_snap(
                        zone,
                        &tab.id.key,
                        initial_rect,
                        inner.response.rect,
                        &snap_siblings,
                        window_bounds,
                        min_window_size,
                        max_window_size,
                        snap_suspended,
                        pointer_down,
                    );
                    self.track_main_window_rect(
                        &tab.id.key,
                        normalize_height(tracked),
                        window_bounds,
                    );
                } else if should_track_rect {
                    self.track_main_window_rect(
                        &tab.id.key,
                        normalize_height(inner.response.rect),
                        window_bounds,
                    );
                }
                if zone == GuiShellZone::Center && pointer_interacting {
                    // Mirror the CANONICAL rect (post-tracking), not the
                    // rendered one: while a shell zone is open the rendered
                    // rect is displaced, and zone drag/drop restores from
                    // this map — a displaced rect must not leak into the
                    // canonical geometry via "move to a zone and back".
                    let snapshot = self
                        .main_window_rects
                        .get(&tab.id.key)
                        .copied()
                        .unwrap_or_else(|| {
                            Self::rect_to_snapshot(Self::clamp_main_window_rect(
                                inner.response.rect,
                                window_bounds,
                            ))
                        });
                    if snapshot.iter().all(|value| value.is_finite()) {
                        self.last_center_window_rects
                            .insert(tab.id.key.clone(), snapshot);
                    }
                }
                clicked_link = inner.inner.flatten();
                zone_window_rects.push(GuiZoneWindowRect {
                    zone,
                    tab_key: tab.id.key.clone(),
                    rect: inner.response.rect,
                });
                if let Some(pointer_pos) = secondary_click_pos {
                    // Same top-layer gate as the sidebar path: overlapping
                    // windows must not steal the menu from the one on top.
                    if inner.response.rect.contains(pointer_pos)
                        && ctx.layer_id_at(pointer_pos) == Some(inner.response.layer_id)
                    {
                        actions.window_menu_request = Some(GuiWindowMenuRequest {
                            tab_key: tab.id.key.clone(),
                            zone,
                            position: pointer_pos,
                            window_rect: inner.response.rect,
                        });
                    }
                }
                occupied_rects.push(inner.response.rect);
                if self.zone_drag_state.is_none() && !window_locked {
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
            if is_webui_window && !webui_open {
                actions.webui_closes.push(tab.window_name.clone());
            }
            if let Some(click) = clicked_link {
                actions.link_clicks.push(click);
            }
        }

        if self.ui_settings.snap_show_guides {
            self.paint_snap_overlays(ctx, zone, window_bounds);
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_preserves_stored_sidebar_width_on_narrow_window() {
        // Regression: the old width-aware squeeze in sanitize() wrote the clamp
        // back into the persisted width, so a narrow window permanently shrank
        // the stored sidebar and it never sprang back. sanitize() must only
        // range-clamp now; the display squeeze lives in squeezed_sidebar_widths.
        let mut layout = ShellLayoutSnapshot {
            left_sidebar_width: 300.0,
            right_sidebar_width: 300.0,
            ..Default::default()
        };
        // A window far too narrow to hold two 300px sidebars.
        layout.sanitize();
        assert_eq!(layout.left_sidebar_width, 300.0, "stored width must not shrink");
        assert_eq!(layout.right_sidebar_width, 300.0);

        // The display-only squeeze still narrows the RENDERED widths on a
        // narrow window, without touching the stored values.
        let (l, r) = squeezed_sidebar_widths(700.0, 220.0, 300.0, 300.0);
        assert!(l < 300.0 && r < 300.0, "display squeeze should shrink render widths: {l},{r}");
        assert_eq!(layout.left_sidebar_width, 300.0, "stored width still intact");
    }

    #[test]
    fn sanitize_still_range_clamps_out_of_bounds_widths() {
        let mut layout = ShellLayoutSnapshot {
            left_sidebar_width: 9999.0, // above the 700 max
            right_sidebar_width: 10.0,  // below the 220 min
            ..Default::default()
        };
        layout.sanitize();
        assert_eq!(layout.left_sidebar_width, 700.0);
        assert_eq!(layout.right_sidebar_width, 220.0);
    }

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
    fn stationary_click_does_not_relax_size_pin() {
        // A title-bar click: press origin and current pointer coincide. This
        // must NOT count as a drag, so the size pin stays and the (grouped)
        // window can't jump to egui's remembered desired_size. Regression
        // guard for "clicking the title bar resizes a grouped window".
        let origin = egui::pos2(100.0, 50.0);
        assert!(!super::press_became_drag(Some(origin), Some(origin), false));
        // Tiny jitter under the threshold is still a click, not a drag.
        let jitter = egui::pos2(102.0, 51.0);
        assert!(!super::press_became_drag(Some(origin), Some(jitter), false));
    }

    #[test]
    fn real_drag_relaxes_size_pin() {
        // Pointer traveled well past the threshold: a genuine resize/move
        // drag, so the pin relaxes and egui owns the geometry.
        let origin = egui::pos2(100.0, 50.0);
        let dragged = egui::pos2(140.0, 90.0);
        assert!(super::press_became_drag(Some(origin), Some(dragged), false));
    }

    #[test]
    fn latched_drag_stays_relaxed_when_pointer_stalls() {
        // Once this window owns the drag latch, a mid-gesture pause (pointer
        // momentarily back near the origin) must not re-pin and stall the
        // resize — the latch short-circuits the travel test.
        let origin = egui::pos2(100.0, 50.0);
        assert!(super::press_became_drag(Some(origin), Some(origin), true));
    }

    #[test]
    fn missing_pointer_is_not_a_drag() {
        // No press origin or no pointer position → not a drag (pin holds).
        assert!(!super::press_became_drag(None, Some(egui::pos2(1.0, 1.0)), false));
        assert!(!super::press_became_drag(Some(egui::pos2(1.0, 1.0)), None, false));
    }

    #[test]
    fn send_to_back_raises_other_center_windows_in_order() {
        let a = egui::Id::new("a");
        let b = egui::Id::new("b");
        let c = egui::Id::new("c");
        let popup = egui::Id::new("some_popup"); // a middle layer that isn't a window
        // Stack back-to-front: a (bottom), b, c (top), plus an unrelated popup.
        let ordered = vec![a, b, c, popup];
        let center: std::collections::HashSet<egui::Id> = [a, b, c].into_iter().collect();

        // Send the top window (c) to back: a and b are raised, in that order,
        // so their a-below-b relationship survives and c lands beneath both.
        assert_eq!(super::send_to_back_raise_order(&ordered, c, &center), vec![a, b]);
        // Send the bottom window (a) to back: b then c raised; the popup is
        // not a Center window, so it is never raised.
        assert_eq!(super::send_to_back_raise_order(&ordered, a, &center), vec![b, c]);
        // A target that isn't in the set (e.g. no overlap) still just raises
        // the others; nothing panics.
        assert_eq!(
            super::send_to_back_raise_order(&ordered, egui::Id::new("missing"), &center),
            vec![a, b, c]
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
    fn squeezed_widths_fit_untouched_when_there_is_room() {
        assert_eq!(
            super::squeezed_sidebar_widths(1920.0, 220.0, 300.0, 300.0),
            (300.0, 300.0)
        );
    }

    #[test]
    fn squeezed_widths_never_resurrect_a_collapsed_sidebar() {
        // The old floor forced a collapsed (0-width) sidebar back to 220
        // on narrow windows; a zero input must stay zero at ANY width.
        let (left, right) = super::squeezed_sidebar_widths(400.0, 220.0, 0.0, 300.0);
        assert_eq!(left, 0.0);
        assert!((right - 180.0).abs() < 0.01, "right takes all the room");
    }

    #[test]
    fn squeezed_widths_never_invert_the_center() {
        // Sweep narrow widths: left + right must never exceed the space
        // outside the center minimum, so center_min_x <= center_max_x by
        // construction (the old math inverted at ~<660px).
        for root in [100.0f32, 220.0, 300.0, 440.0, 660.0, 659.0] {
            let (left, right) = super::squeezed_sidebar_widths(root, 220.0, 300.0, 300.0);
            assert!(
                left + right <= (root - 220.0).max(0.0) + 0.01,
                "root {root}: {left} + {right} overflows"
            );
            assert!(left >= 0.0 && right >= 0.0);
        }
    }

    #[test]
    fn squeezed_widths_shrink_proportionally() {
        // 2:1 sidebars keep their ratio under squeeze.
        let (left, right) = super::squeezed_sidebar_widths(520.0, 220.0, 400.0, 200.0);
        assert!((left / right - 2.0).abs() < 0.01);
        assert!((left + right - 300.0).abs() < 0.01);
    }

    #[test]
    fn test_effective_sidebar_gaps_granted_when_space_is_free() {
        // 1000 tall zone, two 200-tall windows: 600 free, both gaps fit.
        let gaps = super::effective_sidebar_gaps(1000.0, &[(100.0, 200.0), (150.0, 200.0)]);
        assert_eq!(gaps, vec![100.0, 150.0]);
    }

    #[test]
    fn test_effective_sidebar_gaps_shrink_bottom_up_before_heights() {
        // Only 120 free: the top gap is granted in full, the lower one gets
        // the remainder; heights are never asked to give anything up.
        let gaps = super::effective_sidebar_gaps(520.0, &[(100.0, 200.0), (150.0, 200.0)]);
        assert_eq!(gaps, vec![100.0, 20.0]);

        // No free space at all: every gap collapses to zero.
        let gaps = super::effective_sidebar_gaps(400.0, &[(100.0, 200.0), (150.0, 200.0)]);
        assert_eq!(gaps, vec![0.0, 0.0]);

        // Heights already overflow the zone: still just zeros, never negative.
        let gaps = super::effective_sidebar_gaps(300.0, &[(100.0, 200.0), (150.0, 200.0)]);
        assert_eq!(gaps, vec![0.0, 0.0]);
    }

    #[test]
    fn test_effective_sidebar_gaps_ignore_non_finite_and_negative() {
        let gaps = super::effective_sidebar_gaps(
            1000.0,
            &[(f32::NAN, 200.0), (-50.0, 200.0), (f32::INFINITY, 200.0)],
        );
        assert_eq!(gaps, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_sidebar_flush_bottom_stack_survives_gap_grant() {
        // Items mirror how bake_sidebar_stack builds them: the 4pt
        // inter-window gap is folded into every occupied height except the
        // last window's. A persisted stack whose gaps + heights equal the
        // zone height exactly keeps every gap, so a flush-bottom legacy
        // layout bakes to a flush-bottom set of rects.
        let gaps = super::effective_sidebar_gaps(400.0, &[(96.0, 204.0), (0.0, 100.0)]);
        assert_eq!(gaps, vec![96.0, 0.0]);
    }
}
