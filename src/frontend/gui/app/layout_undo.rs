//! Layout undo: a short stack of GUI layout snapshots taken at every
//! autosave boundary, so `.undolayout` (and the `undo_layout` keybind)
//! reverses the last batch of window moves, resizes, hides, groupings and
//! zone changes. Granularity is the autosave debounce — a burst of drags
//! that settles within it is one step, which is what people mean by "put
//! it back".
//!
//! The points are the same `GuiLayoutFileV1` the autosave writes, applied
//! back through `apply_layout_snapshot` with the appearance kept, so undo
//! never swaps a skin or theme.

use super::*;

const LAYOUT_UNDO_DEPTH: usize = 30;

/// Snapshots serialise to JSON for the file anyway; that is also the
/// cheapest faithful equality we have (the struct derives no `PartialEq`).
fn same_layout(a: &GuiLayoutFileV1, b: &GuiLayoutFileV1) -> bool {
    serde_json::to_string(a).ok() == serde_json::to_string(b).ok()
}

impl VellumGuiApp {
    /// Called right before every autosave write: if the arrangement
    /// differs from the last one written, the last one becomes an undo
    /// point. The first call only seeds the baseline.
    pub(super) fn push_layout_undo_point(&mut self) {
        let Some(now) = self.build_layout_snapshot(LayoutSaveMode::Autosave) else {
            return;
        };
        match self.last_saved_layout.take() {
            Some(prev) if !same_layout(&prev, &now) => {
                self.layout_undo_stack.push(prev);
                if self.layout_undo_stack.len() > LAYOUT_UNDO_DEPTH {
                    self.layout_undo_stack.remove(0);
                }
                self.layout_redo_stack.clear();
            }
            _ => {}
        }
        self.last_saved_layout = Some(now);
    }

    /// Seed the baseline before any change has happened, so the very first
    /// edit of the session is undoable too.
    pub(super) fn seed_layout_undo_baseline(&mut self) {
        if self.last_saved_layout.is_none() && !self.layout_dirty {
            self.last_saved_layout = self.build_layout_snapshot(LayoutSaveMode::Autosave);
        }
    }

    pub(super) fn undo_layout(&mut self) {
        let Some(target) = self.layout_undo_stack.pop() else {
            self.app_core
                .add_system_message("Nothing to undo in the layout.");
            return;
        };
        if let Some(current) = self.build_layout_snapshot(LayoutSaveMode::Autosave) {
            self.layout_redo_stack.push(current);
        }
        self.restore_layout_point(target);
        let remaining = self.layout_undo_stack.len();
        self.tab_move_toast = Some(super::tab_drag::TabMoveToast::plain(
            "Layout restored to its previous arrangement",
            if remaining > 0 {
                vec![format!(
                    "{remaining} more step{} back; .redolayout goes forward",
                    if remaining == 1 { "" } else { "s" }
                )]
            } else {
                vec!["That was the oldest step; .redolayout goes forward".to_string()]
            },
        ));
    }

    pub(super) fn redo_layout(&mut self) {
        let Some(target) = self.layout_redo_stack.pop() else {
            self.app_core
                .add_system_message("Nothing to redo in the layout.");
            return;
        };
        if let Some(current) = self.build_layout_snapshot(LayoutSaveMode::Autosave) {
            self.layout_undo_stack.push(current);
        }
        self.restore_layout_point(target);
        self.tab_move_toast = Some(super::tab_drag::TabMoveToast::plain(
            "Layout change re-applied",
            Vec::new(),
        ));
    }

    fn restore_layout_point(&mut self, target: GuiLayoutFileV1) {
        self.apply_layout_snapshot(&target, true);
        // Write it now and make it the baseline, so the restore itself is
        // not pushed as a fresh undo point by the next tick.
        self.last_saved_layout = Some(target);
        self.save_layout_state();
        self.app_core.autosave_layout();
        self.layout_dirty = false;
        self.layout_dirty_since = None;
    }
}
