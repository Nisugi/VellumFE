//! Tab drag — Phase 2 of the tab-tear-out spec
//! (`.beads/artifacts/tab-tear-out/spec.md`, §4–§6).
//!
//! Dragging a tab off a tabbedtext strip tears it out into its own
//! zone-placed window at the drop point. The gesture rides egui's native
//! drag-and-drop: the strip sets a [`TabDragPayload`] once a press has
//! travelled egui's click/drag threshold (so a click still selects), and
//! [`VellumGuiApp::poll_tab_drag`] consumes the release wherever it lands.
//!
//! Every tab move — this gesture, a context-menu conversion, or an
//! `.undotabmove` — then flows through ONE handler,
//! [`VellumGuiApp::apply_tab_move_receipt`], which keeps the shell's
//! per-window placement state consistent. That matters because shell keys
//! are not stable across a move: a text window's `TabKey` is the singleton
//! `TextMain` iff it subscribes to `main`, so tearing the `main` tab out of
//! `story` re-keys `story` to `TextByName{story}` and hands `TextMain` to
//! whichever window now carries `main` — possibly one no receipt mentions.
//! The handler therefore diffs the whole name→key mapping (§5.4), moves
//! every keyed map for every changed name as a set, restores a captured
//! placement by *window name* (§5.5), places a torn-out window at the drop
//! (§5.6), and queues reading-position migration (§4).

use super::*;
use crate::core::app_core::tab_moves::{TabMoveKind, TabMoveReceipt};
use crate::data::widget::TabId as CoreTabId;
use std::collections::HashMap;

// ---- drag payload and strip registry ----------------------------------------

/// What a dragged tab carries. Set by the strip; consumed on release.
#[derive(Clone, Debug)]
pub(crate) struct TabDragPayload {
    pub window: String,
    pub tab_id: CoreTabId,
    pub label: String,
}

/// A tab released somewhere.
pub(super) struct TabDropRequest {
    pub window: String,
    pub tab_id: CoreTabId,
    pub pointer: Pos2,
}

fn strip_rect_id(window: &str) -> egui::Id {
    egui::Id::new(("gui_tab_strip_rect", window))
}

/// Record a strip's on-screen rect for this frame.
pub(crate) fn record_tab_strip_rect(ctx: &egui::Context, window: &str, rect: Rect) {
    ctx.data_mut(|data| data.insert_temp(strip_rect_id(window), rect));
}

pub(super) fn tab_strip_rect(ctx: &egui::Context, window: &str) -> Option<Rect> {
    ctx.data(|data| data.get_temp(strip_rect_id(window)))
}

fn tab_rects_id(window: &str) -> egui::Id {
    egui::Id::new(("gui_tab_rects", window))
}

/// Record each tab's on-screen rect, in strip order, for reorder drops.
pub(crate) fn record_tab_rects(ctx: &egui::Context, window: &str, rects: Vec<(CoreTabId, Rect)>) {
    ctx.data_mut(|data| data.insert_temp(tab_rects_id(window), rects));
}

fn tab_rects(ctx: &egui::Context, window: &str) -> Vec<(CoreTabId, Rect)> {
    ctx.data(|data| data.get_temp(tab_rects_id(window)))
        .unwrap_or_default()
}

// ---- reorder within a strip (§8) ---------------------------------------------

/// Slot a foreign tab would take in a strip: before the first tab whose
/// centre lies right of the pointer. `None` (append) past the last.
pub(super) fn reorder_insert_index(rects: &[(CoreTabId, Rect)], pointer_x: f32) -> Option<usize> {
    rects
        .iter()
        .position(|(_, rect)| pointer_x < rect.center().x)
}

/// Where a tab released at `pointer_x` lands: the new id order and the
/// x of the insertion caret. The dragged tab slots in before the first
/// tab (itself excluded) whose centre lies right of the pointer. `None`
/// when the order would not change.
pub(super) fn reorder_for_drop(
    rects: &[(CoreTabId, Rect)],
    dragged: CoreTabId,
    pointer_x: f32,
) -> Option<(Vec<CoreTabId>, f32)> {
    if !rects.iter().any(|(id, _)| *id == dragged) {
        return None;
    }
    let others: Vec<&(CoreTabId, Rect)> = rects.iter().filter(|(id, _)| *id != dragged).collect();
    let insert_at = others
        .iter()
        .position(|(_, rect)| pointer_x < rect.center().x)
        .unwrap_or(others.len());
    let caret_x = others
        .get(insert_at)
        .map(|(_, rect)| rect.min.x)
        .or_else(|| others.last().map(|(_, rect)| rect.max.x))
        .unwrap_or(pointer_x);
    let mut order: Vec<CoreTabId> = others.iter().map(|(id, _)| *id).collect();
    order.insert(insert_at, dragged);
    let current: Vec<CoreTabId> = rects.iter().map(|(id, _)| *id).collect();
    (order != current).then_some((order, caret_x))
}

// ---- reading-position migration (§4) ----------------------------------------

fn migrations_id() -> egui::Id {
    egui::Id::new("gui_pending_scroll_migrations")
}

/// Fold a move `old → new` into the pending map, which maps every renderer
/// id awaiting migration to the id that *actually owns* the state:
/// - `old == new` (a reorder) is nothing;
/// - if `old` is itself pending (`old → orig`), the chain collapses to
///   `new → orig` — only the original owner's state exists;
/// - if that collapse gives `new == orig`, an inverse pair has cancelled
///   and the original state is left untouched.
pub(super) fn compose_scroll_migration(map: &mut HashMap<String, String>, old: &str, new: &str) {
    if old == new {
        return;
    }
    let origin = map.remove(old).unwrap_or_else(|| old.to_string());
    if origin == new {
        return;
    }
    map.insert(new.to_string(), origin);
}

pub(super) fn queue_scroll_migration(ctx: &egui::Context, old: &str, new: &str) {
    ctx.data_mut(|data| {
        let map: &mut HashMap<String, String> = data.get_temp_mut_or_default(migrations_id());
        compose_scroll_migration(map, old, new);
    });
}

/// The id whose state `scroll_id` should adopt, if a migration is pending.
pub(crate) fn pending_scroll_migration(ctx: &egui::Context, scroll_id: &str) -> Option<String> {
    ctx.data(|data| {
        data.get_temp::<HashMap<String, String>>(migrations_id())
            .and_then(|map| map.get(scroll_id).cloned())
    })
}

/// Drop `scroll_id`'s pending entry once its panes have adopted the state.
pub(crate) fn finish_scroll_migration(ctx: &egui::Context, scroll_id: &str) {
    ctx.data_mut(|data| {
        let map: &mut HashMap<String, String> = data.get_temp_mut_or_default(migrations_id());
        map.remove(scroll_id);
    });
}

// ---- toast -------------------------------------------------------------------

/// What the last tab move did, shown briefly with an Undo button.
#[derive(Clone, Debug)]
pub(super) struct TabMoveToast {
    pub headline: String,
    pub notes: Vec<String>,
    pub shown_at: Instant,
    /// False for the toast that reports an undo (nothing to undo again).
    pub undoable: bool,
}

impl TabMoveToast {
    const LIFETIME: Duration = Duration::from_secs(8);

    /// A toast with no undo button (layout undo/redo confirmations).
    pub(super) fn plain(headline: impl Into<String>, notes: Vec<String>) -> Self {
        Self {
            headline: headline.into(),
            notes,
            shown_at: Instant::now(),
            undoable: false,
        }
    }

    fn for_receipt(receipt: &TabMoveReceipt) -> Self {
        let headline = match receipt.kind {
            TabMoveKind::TearOut => format!("Opened '{}' as its own window", receipt.window),
            TabMoveKind::ConvertedInPlace => format!("'{}' is a text window now", receipt.window),
            TabMoveKind::Join => match &receipt.removed_window {
                Some(source) => format!("Added '{source}' as a tab of '{}'", receipt.window),
                None => format!("Added a tab to '{}'", receipt.window),
            },
            TabMoveKind::WrappedInPlace => format!("'{}' is a tabbed window now", receipt.window),
            TabMoveKind::Reorder => format!("Reordered tabs in '{}'", receipt.window),
            TabMoveKind::Move => match &receipt.removed_window {
                Some(source) => format!("Moved the last tab of '{source}' into '{}'", receipt.window),
                None => format!("Moved a tab into '{}'", receipt.window),
            },
        };
        let notes = receipt
            .capacity_changed
            .iter()
            .map(|(window, from, to)| {
                format!("'{window}' scrollback capacity {from} → {to} lines; nothing was lost")
            })
            .collect();
        Self {
            headline,
            notes,
            shown_at: Instant::now(),
            undoable: true,
        }
    }
}

// ---- placement snapshot (§5.1 / §5.5) ---------------------------------------

/// Everything the shell knows about one window's placement, captured by
/// window name before a move removes or re-places it, and restored under
/// whatever key the window has *after* undo. Cross-window references are
/// stored by name — `TextMain` may belong to a different window by then.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct GuiPlacementSnapshot {
    pub window: String,
    pub rect: Option<[f32; 4]>,
    pub zone: Option<GuiShellZone>,
    pub last_center: Option<[f32; 4]>,
    pub gap_above: Option<f32>,
    pub no_title: bool,
    pub hidden: bool,
    pub settings: Option<TabSettings>,
    pub size_role: Option<super::dock::SizeRole>,
    pub anchors: Option<super::window_manager::WindowAnchors>,
    /// Other windows whose anchors referenced this one, with their anchors
    /// as they stood (the prune at forget time frees those refs).
    pub referrers: Vec<(String, super::window_manager::WindowAnchors)>,
    /// Every sibling key inside `anchors`/`referrers`, resolved to a window
    /// name at capture so it can be re-resolved to a key at restore.
    pub key_names: Vec<(TabKey, String)>,
    pub group: Option<GroupSnapshot>,
    pub detached: Option<ViewportState>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct GroupSnapshot {
    pub leader: String,
    /// Position among the group's members, so restore keeps the order.
    #[serde(default)]
    pub index: usize,
}

/// How a receipt reached the handler: a drop point and size for a gesture,
/// nothing for a menu conversion or an undo.
pub(super) struct GestureContext {
    pub drop: Option<DropPoint>,
    pub size: Option<Vec2>,
}

pub(super) struct DropPoint {
    pub pointer: Pos2,
    pub zone: Option<GuiShellZone>,
}

// ---- pure re-keying over the placement maps (§5.4) --------------------------

/// Borrowed views of every `TabKey`-keyed map the shell keeps, so re-keying
/// is a pure function that tests can drive without an app.
pub(super) struct PlacementMaps<'a> {
    pub main_window_rects: &'a mut HashMap<TabKey, [f32; 4]>,
    pub tab_zones: &'a mut HashMap<TabKey, GuiShellZone>,
    pub window_anchors: &'a mut HashMap<TabKey, super::window_manager::WindowAnchors>,
    pub window_size_roles: &'a mut HashMap<TabKey, super::dock::SizeRole>,
    pub sidebar_gap_above: &'a mut HashMap<TabKey, f32>,
    pub last_center_window_rects: &'a mut HashMap<TabKey, [f32; 4]>,
    pub no_title_tabs: &'a mut HashSet<TabKey>,
    pub hidden_tabs: &'a mut HashSet<TabKey>,
    pub tab_settings: &'a mut HashMap<TabKey, TabSettings>,
    pub tab_groups: &'a mut Vec<TabGroup>,
}

fn move_map<V>(map: &mut HashMap<TabKey, V>, moves: &[(TabKey, TabKey)]) {
    // Vacate every old key first, then write, so a handoff
    // (`X: TextMain → TextByName{X}`, `B: WindowByName{B} → TextMain`)
    // never overwrites a value that has not moved out yet.
    let vacated: Vec<(TabKey, V)> = moves
        .iter()
        .filter_map(|(old, new)| map.remove(old).map(|value| (new.clone(), value)))
        .collect();
    for (key, value) in vacated {
        map.insert(key, value);
    }
}

fn move_set(set: &mut HashSet<TabKey>, moves: &[(TabKey, TabKey)]) {
    let vacated: Vec<TabKey> = moves
        .iter()
        .filter_map(|(old, new)| set.remove(old).then(|| new.clone()))
        .collect();
    set.extend(vacated);
}

fn remap(key: &TabKey, moves: &[(TabKey, TabKey)]) -> TabKey {
    moves
        .iter()
        .find(|(old, _)| old == key)
        .map(|(_, new)| new.clone())
        .unwrap_or_else(|| key.clone())
}

/// Rewrite every sibling reference inside `anchors` through `resolve`;
/// a reference `resolve` cannot map is stripped to a free edge.
fn rewrite_sibling_refs(
    anchors: &mut super::window_manager::WindowAnchors,
    resolve: &dyn Fn(&TabKey) -> Option<TabKey>,
) {
    use super::window_manager::{AxisAnchoring, EdgeAnchor, EdgeRef};
    let map_anchor = |anchor: &EdgeAnchor| -> Option<EdgeAnchor> {
        match &anchor.target {
            EdgeRef::Sibling { key, side } => resolve(key).map(|key| EdgeAnchor {
                target: EdgeRef::Sibling {
                    key,
                    side: side.clone(),
                },
                offset: anchor.offset,
            }),
            _ => Some(anchor.clone()),
        }
    };
    let rewrite_axis = |axis: &AxisAnchoring| -> AxisAnchoring {
        match axis {
            AxisAnchoring::Free => AxisAnchoring::Free,
            AxisAnchoring::Lo(a) => map_anchor(a).map(AxisAnchoring::Lo).unwrap_or_default(),
            AxisAnchoring::Hi(a) => map_anchor(a).map(AxisAnchoring::Hi).unwrap_or_default(),
            AxisAnchoring::Center(a) => map_anchor(a)
                .map(AxisAnchoring::Center)
                .unwrap_or_default(),
            AxisAnchoring::Both { lo, hi } => match (map_anchor(lo), map_anchor(hi)) {
                (Some(lo), Some(hi)) => AxisAnchoring::Both { lo, hi },
                (Some(lo), None) => AxisAnchoring::Lo(lo),
                (None, Some(hi)) => AxisAnchoring::Hi(hi),
                (None, None) => AxisAnchoring::Free,
            },
        }
    };
    anchors.x = rewrite_axis(&anchors.x);
    anchors.y = rewrite_axis(&anchors.y);
}

/// Every sibling key an anchor set refers to.
fn sibling_keys(anchors: &super::window_manager::WindowAnchors) -> Vec<TabKey> {
    use super::window_manager::{AxisAnchoring, EdgeAnchor, EdgeRef};
    let mut keys = Vec::new();
    let mut push = |anchor: &EdgeAnchor| {
        if let EdgeRef::Sibling { key, .. } = &anchor.target {
            keys.push(key.clone());
        }
    };
    for axis in [&anchors.x, &anchors.y] {
        match axis {
            AxisAnchoring::Free => {}
            AxisAnchoring::Lo(a) | AxisAnchoring::Hi(a) | AxisAnchoring::Center(a) => push(a),
            AxisAnchoring::Both { lo, hi } => {
                push(lo);
                push(hi);
            }
        }
    }
    keys
}

/// Move every keyed entry for every `(old, new)` pair, as a set. Group
/// membership and sibling anchor references follow too.
pub(super) fn rekey_placement(moves: &[(TabKey, TabKey)], maps: PlacementMaps<'_>) {
    if moves.is_empty() {
        return;
    }
    move_map(maps.main_window_rects, moves);
    move_map(maps.tab_zones, moves);
    move_map(maps.window_anchors, moves);
    move_map(maps.window_size_roles, moves);
    move_map(maps.sidebar_gap_above, moves);
    move_map(maps.last_center_window_rects, moves);
    move_map(maps.tab_settings, moves);
    move_set(maps.no_title_tabs, moves);
    move_set(maps.hidden_tabs, moves);
    let resolve = |key: &TabKey| Some(remap(key, moves));
    for anchors in maps.window_anchors.values_mut() {
        rewrite_sibling_refs(anchors, &resolve);
    }
    for group in maps.tab_groups.iter_mut() {
        for key in group
            .members
            .iter_mut()
            .chain(group.merged.iter_mut())
            .chain(group.end_anchored.iter_mut())
        {
            *key = remap(key, moves);
        }
        for (key, _) in group.weights.iter_mut() {
            *key = remap(key, moves);
        }
    }
}

// ---- the handler -------------------------------------------------------------

impl VellumGuiApp {
    /// The shell's current window-name → key mapping.
    pub(super) fn window_keys_by_name(&self) -> HashMap<String, TabKey> {
        self.available_tabs
            .iter()
            .map(|(key, tab)| (tab.window_name.clone(), key.clone()))
            .collect()
    }

    /// Snapshot a window's placement by name (§5.1), before a move removes
    /// or re-places it.
    pub(super) fn capture_placement(&self, window: &str) -> GuiPlacementSnapshot {
        let names_by_key: HashMap<TabKey, String> = self
            .available_tabs
            .iter()
            .map(|(key, tab)| (key.clone(), tab.window_name.clone()))
            .collect();
        let Some(key) = self.window_keys_by_name().get(window).cloned() else {
            return GuiPlacementSnapshot {
                window: window.to_string(),
                ..Default::default()
            };
        };
        let anchors = self.window_anchors.get(&key).cloned();
        let referrers: Vec<(String, super::window_manager::WindowAnchors)> = self
            .window_anchors
            .iter()
            .filter(|(other, a)| *other != &key && sibling_keys(a).contains(&key))
            .filter_map(|(other, a)| names_by_key.get(other).map(|n| (n.clone(), a.clone())))
            .collect();
        let mut key_names: Vec<(TabKey, String)> = Vec::new();
        for set in anchors.iter().chain(referrers.iter().map(|(_, a)| a)) {
            for sibling in sibling_keys(set) {
                if let Some(name) = names_by_key.get(&sibling) {
                    if !key_names.iter().any(|(k, _)| *k == sibling) {
                        key_names.push((sibling, name.clone()));
                    }
                }
            }
        }
        let group = self.group_for_tab(&key).and_then(|group| {
            let leader = group.members.first().filter(|leader| *leader != &key)?;
            let index = group.members.iter().position(|member| member == &key)?;
            names_by_key.get(leader).map(|leader| GroupSnapshot {
                leader: leader.clone(),
                index,
            })
        });
        GuiPlacementSnapshot {
            window: window.to_string(),
            rect: self.main_window_rects.get(&key).copied(),
            zone: self.tab_zones.get(&key).copied(),
            last_center: self.last_center_window_rects.get(&key).copied(),
            gap_above: self.sidebar_gap_above.get(&key).copied(),
            no_title: self.no_title_tabs.contains(&key),
            hidden: self.hidden_tabs.contains(&key),
            settings: self.tab_settings.get(&key).cloned(),
            size_role: self.window_size_roles.get(&key).copied(),
            anchors,
            referrers,
            key_names,
            group,
            detached: self.detached_tabs.get(&key).map(|d| d.current.clone()),
        }
    }

    /// Consume a tab move: forget a removed window, reconcile every shell
    /// key globally, restore a captured placement, place a dropped window,
    /// and queue reading-position migration (§5, in that order).
    /// `old_keys` is the name→key map from *before* the core call.
    pub(super) fn apply_tab_move_receipt(
        &mut self,
        ctx: &egui::Context,
        receipt: &TabMoveReceipt,
        gesture: GestureContext,
        old_keys: &HashMap<String, TabKey>,
        restore: Option<GuiPlacementSnapshot>,
        zone_rects: &[(GuiShellZone, Rect)],
    ) {
        // 3. Forget the window that no longer exists.
        if let Some(gone) = &receipt.removed_window {
            if let Some(key) = old_keys.get(gone).cloned() {
                self.forget_tab_state(&key, gone);
            }
        }

        // 4. Reconcile keys over the whole window set.
        let new_tabs = Self::collect_available_tabs(&self.app_core);
        let new_keys: HashMap<String, TabKey> = new_tabs
            .iter()
            .map(|(key, tab)| (tab.window_name.clone(), key.clone()))
            .collect();
        let mut moves: Vec<(TabKey, TabKey)> = old_keys
            .iter()
            .filter(|(name, _)| receipt.removed_window.as_deref() != Some(name.as_str()))
            .filter_map(|(name, old)| {
                new_keys
                    .get(name)
                    .filter(|new| *new != old)
                    .map(|new| (old.clone(), new.clone()))
            })
            .collect();
        moves.sort_by(|a, b| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)));
        if !moves.is_empty() {
            tracing::info!("tab move re-keys {} window(s): {:?}", moves.len(), moves);
            self.rekey_placement_state(&moves);
        }
        self.available_tabs = new_tabs;

        // 5. Restore a captured placement under the window's final key.
        if let Some(snapshot) = restore {
            if let Some(final_key) = new_keys.get(&snapshot.window).cloned() {
                self.restore_placement(snapshot, &final_key, &new_keys);
            }
        }

        // 6. Place the window holding the content at the drop point.
        if let Some(drop) = gesture.drop {
            if let Some(key) = new_keys.get(&receipt.window).cloned() {
                let zone = drop.zone.unwrap_or_else(|| self.zone_for_tab(&key));
                self.set_tab_zone(key.clone(), zone);
                self.place_window_at_pointer(&key, zone, drop.pointer, gesture.size, zone_rects);
            }
        }

        // 7. Reading position follows the content.
        queue_scroll_migration(ctx, &receipt.old_scroll_id, &receipt.new_scroll_id);
        for (window, from, to) in &receipt.capacity_changed {
            self.app_core.add_system_message(&format!(
                "'{window}' scrollback capacity {from} → {to} lines (no history was lost)."
            ));
        }
        self.tab_move_toast = Some(TabMoveToast::for_receipt(receipt));
        if matches!(
            receipt.kind,
            TabMoveKind::TearOut | TabMoveKind::Join | TabMoveKind::Move
        ) {
            self.close_popup_menus_if_host_gone();
        }
        self.layout_dirty = true;
    }

    fn rekey_placement_state(&mut self, moves: &[(TabKey, TabKey)]) {
        rekey_placement(
            moves,
            PlacementMaps {
                main_window_rects: &mut self.main_window_rects,
                tab_zones: &mut self.tab_zones,
                window_anchors: &mut self.window_anchors,
                window_size_roles: &mut self.window_size_roles,
                sidebar_gap_above: &mut self.sidebar_gap_above,
                last_center_window_rects: &mut self.last_center_window_rects,
                no_title_tabs: &mut self.no_title_tabs,
                hidden_tabs: &mut self.hidden_tabs,
                tab_settings: &mut self.tab_settings,
                tab_groups: &mut self.tab_groups,
            },
        );
        for (old, new) in moves {
            if self.popup_menu_host.as_ref() == Some(old) {
                self.popup_menu_host = Some(new.clone());
            }
            if self.pending_raise_tab.as_ref() == Some(old) {
                self.pending_raise_tab = Some(new.clone());
            }
            if self.zone_engaged_tab.as_ref() == Some(old) {
                self.zone_engaged_tab = Some(new.clone());
            }
            if let Some(state) = self.detached_tabs.remove(old) {
                let mut viewport = state.current.clone();
                viewport.tab = new.clone();
                self.detached_tabs
                    .insert(new.clone(), DetachedWindowState::new(new, viewport));
            }
        }
    }

    fn restore_placement(
        &mut self,
        snapshot: GuiPlacementSnapshot,
        final_key: &TabKey,
        new_keys: &HashMap<String, TabKey>,
    ) {
        let key_names = snapshot.key_names;
        let resolve = |key: &TabKey| -> Option<TabKey> {
            key_names
                .iter()
                .find(|(captured, _)| captured == key)
                .and_then(|(_, name)| new_keys.get(name).cloned())
        };

        if let Some(zone) = snapshot.zone {
            self.tab_zones.insert(final_key.clone(), zone);
        }
        if let Some(rect) = snapshot.rect {
            self.main_window_rects.insert(final_key.clone(), rect);
        }
        if let Some(rect) = snapshot.last_center {
            self.last_center_window_rects.insert(final_key.clone(), rect);
        }
        if let Some(gap) = snapshot.gap_above {
            self.sidebar_gap_above.insert(final_key.clone(), gap);
        }
        if snapshot.no_title {
            self.no_title_tabs.insert(final_key.clone());
        } else {
            self.no_title_tabs.remove(final_key);
        }
        if snapshot.hidden {
            self.hidden_tabs.insert(final_key.clone());
        }
        if let Some(settings) = snapshot.settings {
            self.tab_settings.insert(final_key.clone(), settings);
        }
        if let Some(role) = snapshot.size_role {
            self.window_size_roles.insert(final_key.clone(), role);
        }
        if let Some(mut anchors) = snapshot.anchors {
            rewrite_sibling_refs(&mut anchors, &resolve);
            self.window_anchors.insert(final_key.clone(), anchors);
        }
        for (referrer, mut anchors) in snapshot.referrers {
            if let Some(referrer_key) = new_keys.get(&referrer).cloned() {
                rewrite_sibling_refs(&mut anchors, &resolve);
                self.window_anchors.insert(referrer_key, anchors);
            }
        }
        if let Some(group) = snapshot.group {
            if let Some(leader) = new_keys.get(&group.leader).cloned() {
                if leader != *final_key {
                    self.group_tabs(&leader, final_key.clone());
                    if let Some(members) = self
                        .tab_groups
                        .iter_mut()
                        .find(|g| g.members.contains(&leader))
                        .map(|g| &mut g.members)
                    {
                        if let Some(at) = members.iter().position(|m| m == final_key) {
                            let key = members.remove(at);
                            let index = group.index.clamp(1, members.len());
                            members.insert(index, key);
                        }
                    }
                }
            }
        }
        if let Some(mut viewport) = snapshot.detached {
            viewport.tab = final_key.clone();
            self.detached_tabs
                .insert(final_key.clone(), DetachedWindowState::new(final_key, viewport));
        }
        self.layout_dirty = true;
    }

    // ---- gesture -------------------------------------------------------------

    /// Each frame while a tab is being dragged: draw the ghost chip, let
    /// Esc cancel, and on release hand back where it landed.
    pub(super) fn poll_tab_drag(&mut self, ctx: &egui::Context) -> Option<TabDropRequest> {
        if !egui::DragAndDrop::has_any_payload(ctx) {
            return None;
        }
        let payload = egui::DragAndDrop::payload::<TabDragPayload>(ctx)?;
        let pointer = ctx.input(|input| {
            input
                .pointer
                .interact_pos()
                .or_else(|| input.pointer.latest_pos())
        });
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            egui::DragAndDrop::clear_payload(ctx);
            return None;
        }
        let over_own_strip = pointer.is_some_and(|pos| {
            tab_strip_rect(ctx, &payload.window).is_some_and(|rect| rect.contains(pos))
        });
        let over_other_strip = pointer
            .and_then(|pos| self.tabbed_strip_under_pointer(ctx, pos))
            .filter(|target| *target != payload.window);
        let skin_tab = self
            .skin_state
            .widget_art()
            .and_then(|art| art.control_border("tab", "active").cloned());
        if let Some(pointer_pos) = pointer {
            let caret_target = if over_own_strip {
                Some((payload.window.clone(), true))
            } else {
                over_other_strip.clone().map(|target| (target, false))
            };
            if let Some((strip_window, own)) = caret_target {
                let rects = tab_rects(ctx, &strip_window);
                let caret = if own {
                    reorder_for_drop(&rects, payload.tab_id, pointer_pos.x).map(|(_, x)| x)
                } else {
                    match reorder_insert_index(&rects, pointer_pos.x) {
                        Some(i) => rects.get(i).map(|(_, r)| r.min.x),
                        None => rects.last().map(|(_, r)| r.max.x),
                    }
                };
                if let Some(caret_x) = caret {
                    if let Some(strip) = tab_strip_rect(ctx, &strip_window) {
                        ctx.layer_painter(egui::LayerId::new(
                            egui::Order::Tooltip,
                            egui::Id::new("gui_tab_reorder_caret"),
                        ))
                        .line_segment(
                            [Pos2::new(caret_x, strip.min.y), Pos2::new(caret_x, strip.max.y)],
                            egui::Stroke::new(2.0, Color32::from_rgb(90, 200, 110)),
                        );
                    }
                }
            }
            // The ghost is drawn as the tab itself, lifted: a chip in the
            // strip's own look (skin tab art when there is one, the theme's
            // selected-tab fill otherwise) with a soft shadow, offset so
            // the pointer sits on its left edge the way it grabbed the tab.
            egui::Area::new(egui::Id::new("gui_tab_drag_ghost"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pointer_pos + Vec2::new(-8.0, -10.0))
                .interactable(false)
                .show(ctx, |ui| {
                    ui.set_opacity(0.92);
                    let font = egui::TextStyle::Body.resolve(ui.style());
                    let galley = ui.painter().layout_no_wrap(
                        payload.label.clone(),
                        font,
                        ui.visuals().strong_text_color(),
                    );
                    let size = galley.size() + egui::vec2(18.0, 8.0);
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().rect_filled(
                        rect.translate(Vec2::new(2.0, 3.0)),
                        4.0,
                        Color32::from_black_alpha(90),
                    );
                    match &skin_tab {
                        Some(border) => {
                            crate::frontend::gui::skin::paint_nine_slice_filled(
                                ui.painter(),
                                rect,
                                border,
                            );
                        }
                        None => {
                            let visuals = &ui.visuals().widgets.active;
                            ui.painter().rect(
                                rect,
                                4.0,
                                ui.visuals().selection.bg_fill,
                                egui::Stroke::new(1.0, visuals.fg_stroke.color),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                    let text_pos = rect.center() - galley.size() / 2.0;
                    ui.painter()
                        .galley(text_pos, galley, ui.visuals().strong_text_color());
                    ui.add_space(2.0);
                    ui.weak(match (&over_other_strip, over_own_strip) {
                        (Some(target), _) => format!("Release to move into '{target}'"),
                        (None, true) => "Release to reorder".to_string(),
                        (None, false) => "Release to open in its own window".to_string(),
                    });
                });
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if !ctx.input(|input| input.pointer.any_released()) {
            return None;
        }
        let payload = egui::DragAndDrop::take_payload::<TabDragPayload>(ctx)?;
        let pointer = pointer?;
        Some(TabDropRequest {
            window: payload.window.clone(),
            tab_id: payload.tab_id,
            pointer,
        })
    }

    /// A tab was released. On its own strip that is a reorder (Phase 4);
    /// anywhere else it tears out at the drop point.
    pub(super) fn handle_tab_drop(
        &mut self,
        ctx: &egui::Context,
        request: TabDropRequest,
        zone_rects: &[(GuiShellZone, Rect)],
    ) {
        if tab_strip_rect(ctx, &request.window).is_some_and(|rect| rect.contains(request.pointer)) {
            let Some((order, _)) =
                reorder_for_drop(&tab_rects(ctx, &request.window), request.tab_id, request.pointer.x)
            else {
                return;
            };
            let old_keys = self.window_keys_by_name();
            match self.app_core.reorder_tabs(&request.window, &order) {
                Ok(receipt) => self.apply_tab_move_receipt(
                    ctx,
                    &receipt,
                    GestureContext { drop: None, size: None },
                    &old_keys,
                    None,
                    zone_rects,
                ),
                Err(e) => self.app_core.add_system_message(&format!("{e}")),
            }
            return;
        }
        // Onto another tabbed window's strip: a straight move.
        if let Some(target) = self
            .tabbed_strip_under_pointer(ctx, request.pointer)
            .filter(|target| *target != request.window)
        {
            let old_keys = self.window_keys_by_name();
            let capture = self.capture_placement(&request.window);
            let insert_at = reorder_insert_index(&tab_rects(ctx, &target), request.pointer.x);
            match self
                .app_core
                .move_tab(&request.window, request.tab_id, &target, insert_at)
            {
                Ok(receipt) => {
                    // The source may have ceased to exist; undo needs its
                    // placement to bring it back where it was.
                    if let Ok(value) = serde_json::to_value(&capture) {
                        self.app_core.set_tab_undo_gui(value);
                    }
                    self.apply_tab_move_receipt(
                        ctx,
                        &receipt,
                        GestureContext { drop: None, size: None },
                        &old_keys,
                        None,
                        zone_rects,
                    );
                }
                Err(e) => self.app_core.add_system_message(&format!("{e}")),
            }
            return;
        }
        let old_keys = self.window_keys_by_name();
        let source_key = old_keys.get(&request.window).cloned();
        let size = source_key
            .as_ref()
            .and_then(|key| self.main_window_rects.get(key))
            .map(|rect| Vec2::new(rect[2], rect[3]));
        let capture = self.capture_placement(&request.window);

        let receipt = match self.app_core.tear_out_tab(&request.window, request.tab_id) {
            Ok(receipt) => receipt,
            Err(e) => {
                self.app_core.add_system_message(&format!("{e}"));
                return;
            }
        };
        // A last-tab conversion re-places the same window; undo puts it back.
        if receipt.kind == TabMoveKind::ConvertedInPlace {
            if let Ok(value) = serde_json::to_value(&capture) {
                self.app_core.set_tab_undo_gui(value);
            }
        }
        let zone = Self::zone_for_pointer(zone_rects, request.pointer);
        self.apply_tab_move_receipt(
            ctx,
            &receipt,
            GestureContext {
                drop: Some(DropPoint {
                    pointer: request.pointer,
                    zone,
                }),
                size,
            },
            &old_keys,
            None,
            zone_rects,
        );
    }

    /// An `.undotabmove` ran in the core; reconcile the shell to it. Must
    /// run before this frame refreshes `available_tabs`, so the map still
    /// describes the pre-undo keys.
    pub(super) fn apply_pending_tab_undo(&mut self, ctx: &egui::Context) {
        let Some(outcome) = self.app_core.tab_undo_to_apply.take() else {
            return;
        };
        let old_keys = self.window_keys_by_name();
        let restore: Option<GuiPlacementSnapshot> = serde_json::from_value(outcome.gui).ok();
        let zone_rects: Vec<(GuiShellZone, Rect)> = self
            .last_zone_pane_rects
            .iter()
            .map(|(zone, rect)| (*zone, *rect))
            .collect();
        self.apply_tab_move_receipt(
            ctx,
            &outcome.receipt,
            GestureContext {
                drop: None,
                size: None,
            },
            &old_keys,
            restore,
            &zone_rects,
        );
        self.tab_move_toast = Some(TabMoveToast {
            headline: format!("Undid the last tab move; '{}' holds the content again", outcome.receipt.window),
            notes: Vec::new(),
            shown_at: Instant::now(),
            undoable: false,
        });
    }

    /// Bottom-centre toast for the last tab move, with Undo. Hovering it
    /// holds it open; otherwise it fades after a few seconds.
    pub(super) fn render_tab_move_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = self.tab_move_toast.clone() else {
            return;
        };
        let age = toast.shown_at.elapsed();
        if age > TabMoveToast::LIFETIME {
            self.tab_move_toast = None;
            return;
        }
        let fade_in = Duration::from_millis(150);
        let fade_out = Duration::from_millis(400);
        let alpha = if age < fade_in {
            age.as_secs_f32() / fade_in.as_secs_f32()
        } else if age > TabMoveToast::LIFETIME - fade_out {
            (TabMoveToast::LIFETIME - age).as_secs_f32() / fade_out.as_secs_f32()
        } else {
            1.0
        };
        let screen = ctx.content_rect();
        let mut undo_clicked = false;
        let mut dismiss = false;
        let mut hovered = false;
        let response = egui::Area::new(egui::Id::new("gui_tab_move_toast"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -28.0))
            .constrain_to(screen)
            .show(ctx, |ui| {
                ui.set_opacity(alpha.clamp(0.0, 1.0));
                let frame = egui::Frame::popup(ui.style())
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(14, 10));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&toast.headline).strong());
                            for note in &toast.notes {
                                ui.weak(note);
                            }
                        });
                        ui.add_space(12.0);
                        if toast.undoable {
                            if ui.button("Undo").on_hover_text(".undotabmove").clicked() {
                                undo_clicked = true;
                            }
                        }
                        if ui.small_button("✕").clicked() {
                            dismiss = true;
                        }
                    });
                    hovered = ui.rect_contains_pointer(ui.min_rect());
                });
            });
        if hovered && response.response.contains_pointer() {
            // Hold the clock while the pointer rests on it.
            if let Some(live) = self.tab_move_toast.as_mut() {
                live.shown_at = Instant::now() - fade_in.min(age);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(50));
        if dismiss {
            self.tab_move_toast = None;
        }
        if undo_clicked {
            self.tab_move_toast = None;
            match self.app_core.undo_tab_move() {
                Ok(outcome) => self.app_core.tab_undo_to_apply = Some(outcome),
                Err(e) => self.app_core.add_system_message(&format!("{e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::gui::app::window_manager::{AxisAnchoring, AxisSide, EdgeAnchor, EdgeRef};

    fn by_name(id: &str) -> TabKey {
        TabKey::TextByName { id: id.to_string() }
    }

    fn maps() -> (
        HashMap<TabKey, [f32; 4]>,
        HashMap<TabKey, GuiShellZone>,
        HashMap<TabKey, super::super::window_manager::WindowAnchors>,
        HashMap<TabKey, super::super::dock::SizeRole>,
        HashMap<TabKey, f32>,
        HashMap<TabKey, [f32; 4]>,
        HashSet<TabKey>,
        HashSet<TabKey>,
        HashMap<TabKey, TabSettings>,
        Vec<TabGroup>,
    ) {
        Default::default()
    }

    macro_rules! placement {
        ($m:ident) => {
            PlacementMaps {
                main_window_rects: &mut $m.0,
                tab_zones: &mut $m.1,
                window_anchors: &mut $m.2,
                window_size_roles: &mut $m.3,
                sidebar_gap_above: &mut $m.4,
                last_center_window_rects: &mut $m.5,
                no_title_tabs: &mut $m.6,
                hidden_tabs: &mut $m.7,
                tab_settings: &mut $m.8,
                tab_groups: &mut $m.9,
            }
        };
    }

    /// The singleton handoff: `story` releases `TextMain` and an untouched
    /// `mirror` inherits it. Both must land under their new keys, nothing
    /// may be overwritten, and nothing may be left under an old key.
    #[test]
    fn rekey_moves_a_handoff_as_a_set() {
        let mut m = maps();
        m.0.insert(TabKey::TextMain, [1.0, 1.0, 100.0, 100.0]);
        m.0.insert(by_name("mirror"), [2.0, 2.0, 50.0, 50.0]);
        m.1.insert(TabKey::TextMain, GuiShellZone::Center);
        m.1.insert(by_name("mirror"), GuiShellZone::LeftSidebar);
        m.6.insert(TabKey::TextMain);
        let moves = vec![
            (TabKey::TextMain, by_name("story")),
            (by_name("mirror"), TabKey::TextMain),
        ];
        rekey_placement(&moves, placement!(m));
        assert_eq!(m.0[&by_name("story")], [1.0, 1.0, 100.0, 100.0]);
        assert_eq!(m.0[&TabKey::TextMain], [2.0, 2.0, 50.0, 50.0]);
        assert!(!m.0.contains_key(&by_name("mirror")));
        assert_eq!(m.1[&by_name("story")], GuiShellZone::Center);
        assert_eq!(m.1[&TabKey::TextMain], GuiShellZone::LeftSidebar);
        assert!(m.6.contains(&by_name("story")) && !m.6.contains(&TabKey::TextMain));
    }

    /// Sibling anchor references and group membership follow the moved keys.
    #[test]
    fn rekey_follows_anchor_refs_and_groups() {
        let mut m = maps();
        let mut anchors = super::super::window_manager::WindowAnchors::default();
        anchors.x = AxisAnchoring::Lo(EdgeAnchor {
            target: EdgeRef::Sibling {
                key: TabKey::TextMain,
                side: AxisSide::Max,
            },
            offset: 0.0,
        });
        m.2.insert(by_name("loot"), anchors);
        m.9.push(TabGroup {
            members: vec![TabKey::TextMain, by_name("loot")],
            horizontal: false,
            merged: vec![],
            end_anchored: vec![TabKey::TextMain],
            weights: vec![(TabKey::TextMain, 2.0)],
        });
        rekey_placement(&[(TabKey::TextMain, by_name("story"))], placement!(m));
        let moved = &m.2[&by_name("loot")];
        assert!(matches!(
            &moved.x,
            AxisAnchoring::Lo(EdgeAnchor { target: EdgeRef::Sibling { key, .. }, .. }) if key == &by_name("story")
        ));
        assert_eq!(m.9[0].members[0], by_name("story"));
        assert_eq!(m.9[0].end_anchored[0], by_name("story"));
        assert_eq!(m.9[0].weights[0].0, by_name("story"));
    }

    /// Restore resolves a reference by name; one whose window is gone is
    /// stripped to a free edge rather than left dangling.
    #[test]
    fn rewrite_strips_unresolvable_refs() {
        let mut anchors = super::super::window_manager::WindowAnchors::default();
        anchors.y = AxisAnchoring::Both {
            lo: EdgeAnchor {
                target: EdgeRef::Sibling {
                    key: by_name("gone"),
                    side: AxisSide::Min,
                },
                offset: 0.0,
            },
            hi: EdgeAnchor::pane(AxisSide::Max),
        };
        rewrite_sibling_refs(&mut anchors, &|_| None);
        assert!(matches!(&anchors.y, AxisAnchoring::Hi(a) if a.target == EdgeRef::Pane(AxisSide::Max)));
    }

    #[test]
    fn reorder_slots_before_first_centre_right_of_pointer() {
        let r = |x0: f32| Rect::from_min_max(Pos2::new(x0, 0.0), Pos2::new(x0 + 40.0, 20.0));
        let rects = vec![(1, r(0.0)), (2, r(50.0)), (3, r(100.0))];
        // Tab 1 dropped past tab 3's centre goes to the end; caret at 3's right edge.
        assert_eq!(reorder_for_drop(&rects, 1, 130.0), Some((vec![2, 3, 1], 140.0)));
        // Tab 3 dropped left of tab 1's centre goes first; caret at 1's left edge.
        assert_eq!(reorder_for_drop(&rects, 3, 5.0), Some((vec![3, 1, 2], 0.0)));
        // Tab 2 dropped on itself: unchanged.
        assert_eq!(reorder_for_drop(&rects, 2, 70.0), None);
        // Unknown tab: nothing.
        assert_eq!(reorder_for_drop(&rects, 9, 70.0), None);
    }

    #[test]
    fn migrations_compose_and_cancel() {
        let mut map = HashMap::new();
        compose_scroll_migration(&mut map, "a", "a");
        assert!(map.is_empty(), "reorder is a no-op");

        compose_scroll_migration(&mut map, "a", "b");
        assert_eq!(map["b"], "a");
        compose_scroll_migration(&mut map, "b", "c");
        assert_eq!(map.len(), 1, "chain collapses");
        assert_eq!(map["c"], "a");

        compose_scroll_migration(&mut map, "c", "a");
        assert!(map.is_empty(), "inverse pair cancels; a's state untouched");
    }
}
