//! Window → tab join — tab-tear-out spec §7.
//!
//! Dragging a `Text` window (by its title bar or body, plain or Alt+drag)
//! and **dwelling** over another tabbedtext window's strip for
//! [`JOIN_DWELL_SECS`] arms a join; releasing while still over that strip
//! runs `join_window_as_tab` and the receipt goes through the shared
//! handler in `tab_drag.rs`. An unarmed release is today's drop. The
//! target strip renders as a drop band during the drag even when it has
//! a single tab (the strip is normally hidden then).
//!
//! The dwell itself is a pure step function so it can be tested without
//! an app or a pointer. The shell's engagement latch is cleared on the
//! release frame, so the state remembers the source window itself and the
//! release consults it rather than re-deriving the drag.

use super::*;
use crate::core::app_core::tab_moves::TabMoveKind;

/// How long the pointer must sit over a strip before a release joins.
pub(super) const JOIN_DWELL_SECS: f64 = 0.35;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct JoinDwell {
    pub source: String,
    pub target: String,
    pub since: f64,
    pub armed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct JoinRequest {
    pub source: String,
    pub target: String,
}

/// One frame of the dwell. `source` is the `Text` window being dragged
/// (None when nothing draggable is in flight — including the release
/// frame); `hovered` is the tabbedtext strip under the pointer; `now` is
/// egui's clock. Returns a request only on an armed release that is still
/// over the armed target.
pub(super) fn step_join_dwell(
    state: &mut Option<JoinDwell>,
    source: Option<&str>,
    hovered: Option<&str>,
    now: f64,
    released: bool,
) -> Option<JoinRequest> {
    if released {
        return state
            .take()
            .filter(|dwell| dwell.armed && hovered == Some(dwell.target.as_str()))
            .map(|dwell| JoinRequest {
                source: dwell.source,
                target: dwell.target,
            });
    }
    let Some(source) = source else {
        *state = None;
        return None;
    };
    let Some(target) = hovered.filter(|target| *target != source) else {
        *state = None;
        return None;
    };
    match state {
        Some(dwell) if dwell.source == source && dwell.target == target => {
            if now - dwell.since >= JOIN_DWELL_SECS {
                dwell.armed = true;
            }
        }
        _ => {
            *state = Some(JoinDwell {
                source: source.to_string(),
                target: target.to_string(),
                since: now,
                armed: false,
            });
        }
    }
    None
}

// ---- what the strip reads (previous frame's state) --------------------------

fn drag_source_id() -> egui::Id {
    egui::Id::new("gui_join_drag_source")
}

fn dwell_target_id() -> egui::Id {
    egui::Id::new("gui_join_dwell_target")
}

/// The `Text` window being dragged, if any, as of the last poll. The strip
/// uses this to render as a drop band even with a single tab.
pub(crate) fn join_drag_source(ctx: &egui::Context) -> Option<String> {
    ctx.data(|data| data.get_temp::<Option<String>>(drag_source_id()))
        .flatten()
}

/// `(target, armed)` from the last poll, for the band's highlight.
pub(crate) fn join_dwell_target(ctx: &egui::Context) -> Option<(String, bool)> {
    ctx.data(|data| data.get_temp::<Option<(String, bool)>>(dwell_target_id()))
        .flatten()
}

impl VellumGuiApp {
    /// The `Text` window whose body is being dragged this frame: an
    /// Alt+drag's subject, or the window egui reports as dragged.
    fn join_drag_source_window(&self) -> Option<String> {
        let key = self
            .zone_drag_state
            .as_ref()
            .map(|drag| drag.tab_key().clone())
            .or_else(|| self.zone_window_dragged.clone())?;
        let name = self.available_tabs.get(&key)?.window_name.clone();
        let is_text = self.app_core.layout.windows.iter().any(|window| {
            window.name() == name && matches!(window, crate::config::WindowDef::Text { .. })
        });
        is_text.then_some(name)
    }

    /// The visible tabbedtext window whose strip contains the pointer.
    pub(super) fn tabbed_strip_under_pointer(
        &self,
        ctx: &egui::Context,
        pointer: Pos2,
    ) -> Option<String> {
        self.available_tabs
            .iter()
            .filter(|(key, _)| !self.hidden_tabs.contains(key))
            .map(|(_, tab)| tab.window_name.as_str())
            .filter(|name| {
                self.app_core.layout.windows.iter().any(|window| {
                    window.name() == *name
                        && matches!(window, crate::config::WindowDef::TabbedText { .. })
                })
            })
            .find(|name| {
                super::tab_drag::tab_strip_rect(ctx, name)
                    .is_some_and(|rect| rect.contains(pointer))
            })
            .map(str::to_string)
    }

    /// Each frame after the zone surfaces: advance the dwell, draw the
    /// hint, and hand back an armed release. Must run BEFORE the zone drop
    /// overlay consumes the same release.
    pub(super) fn poll_join_dwell(&mut self, ctx: &egui::Context) -> Option<JoinRequest> {
        let source = self.join_drag_source_window();
        let (pointer, released, now) = ctx.input(|input| {
            (
                input
                    .pointer
                    .interact_pos()
                    .or_else(|| input.pointer.latest_pos()),
                input.pointer.any_released(),
                input.time,
            )
        });
        let hovered = pointer.and_then(|pos| self.tabbed_strip_under_pointer(ctx, pos));
        let request = step_join_dwell(
            &mut self.join_dwell,
            source.as_deref(),
            hovered.as_deref(),
            now,
            released,
        );
        ctx.data_mut(|data| {
            data.insert_temp(drag_source_id(), source.clone());
            data.insert_temp(
                dwell_target_id(),
                self.join_dwell
                    .as_ref()
                    .map(|dwell| (dwell.target.clone(), dwell.armed)),
            );
        });
        if let (Some(dwell), Some(pointer_pos)) = (&self.join_dwell, pointer) {
            if !dwell.armed {
                ctx.request_repaint_after(std::time::Duration::from_millis(40));
            }
            let hint = if dwell.armed {
                format!("Release to add as a tab in '{}'", dwell.target)
            } else {
                format!("Hold to join '{}'…", dwell.target)
            };
            egui::Area::new(egui::Id::new("gui_join_dwell_hint"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pointer_pos + Vec2::new(16.0, 36.0))
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(RichText::new(hint).strong());
                    });
                });
        }
        request
    }

    /// An armed join was released: capture the source's placement for
    /// undo, run the core transaction, and reconcile the shell.
    pub(super) fn handle_join_drop(&mut self, ctx: &egui::Context, request: JoinRequest) {
        let old_keys = self.window_keys_by_name();
        // A plain drag has already moved the window; the snapshot taken when
        // the drag began is where undo should put it back.
        let capture = self
            .drag_start_placement
            .take()
            .filter(|snapshot| snapshot.window == request.source)
            .unwrap_or_else(|| self.capture_placement(&request.source));
        let receipt = match self
            .app_core
            .join_window_as_tab(&request.source, &request.target, None)
        {
            Ok(receipt) => receipt,
            Err(e) => {
                self.app_core.add_system_message(&format!("{e}"));
                return;
            }
        };
        debug_assert_eq!(receipt.kind, TabMoveKind::Join);
        if let Ok(value) = serde_json::to_value(&capture) {
            self.app_core.set_tab_undo_gui(value);
        }
        let zone_rects: Vec<(GuiShellZone, Rect)> = self
            .last_zone_pane_rects
            .iter()
            .map(|(zone, rect)| (*zone, *rect))
            .collect();
        self.apply_tab_move_receipt(
            ctx,
            &receipt,
            super::tab_drag::GestureContext {
                drop: None,
                size: None,
            },
            &old_keys,
            None,
            &zone_rects,
        );
        self.app_core.add_system_message(&format!(
            "Joined '{}' into '{}' as a tab. `.undotabmove` puts it back.",
            request.source, request.target
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dwell_arms_after_threshold_and_fires_on_release_over_target() {
        let mut state = None;
        assert!(step_join_dwell(&mut state, Some("loot"), Some("story"), 0.0, false).is_none());
        assert_eq!(state.as_ref().map(|d| d.armed), Some(false));
        // Not long enough yet.
        assert!(step_join_dwell(&mut state, Some("loot"), Some("story"), 0.2, false).is_none());
        assert_eq!(state.as_ref().map(|d| d.armed), Some(false));
        // Threshold reached: armed, still no fire.
        assert!(step_join_dwell(&mut state, Some("loot"), Some("story"), 0.4, false).is_none());
        assert_eq!(state.as_ref().map(|d| d.armed), Some(true));
        // Release frame: the shell has dropped its latch (source None).
        let fired = step_join_dwell(&mut state, None, Some("story"), 0.5, true);
        assert_eq!(
            fired,
            Some(JoinRequest {
                source: "loot".into(),
                target: "story".into()
            })
        );
        assert!(state.is_none());
    }

    #[test]
    fn leaving_the_strip_or_changing_target_resets_the_clock() {
        let mut state = None;
        step_join_dwell(&mut state, Some("loot"), Some("story"), 0.0, false);
        // Different strip: the clock restarts from now.
        step_join_dwell(&mut state, Some("loot"), Some("chat"), 0.3, false);
        assert_eq!(
            state.as_ref().map(|d| (d.target.as_str(), d.since)),
            Some(("chat", 0.3))
        );
        step_join_dwell(&mut state, Some("loot"), Some("chat"), 0.5, false);
        assert_eq!(state.as_ref().map(|d| d.armed), Some(false));
        // Off every strip: disarmed entirely.
        step_join_dwell(&mut state, Some("loot"), None, 0.6, false);
        assert!(state.is_none());
    }

    #[test]
    fn unarmed_release_and_release_off_target_do_not_fire() {
        let mut state = None;
        step_join_dwell(&mut state, Some("loot"), Some("story"), 0.0, false);
        assert!(step_join_dwell(&mut state, None, Some("story"), 0.1, true).is_none());
        assert!(state.is_none());

        step_join_dwell(&mut state, Some("loot"), Some("story"), 1.0, false);
        step_join_dwell(&mut state, Some("loot"), Some("story"), 1.5, false);
        assert_eq!(state.as_ref().map(|d| d.armed), Some(true));
        // Armed, but the pointer slid off the strip before release.
        assert!(step_join_dwell(&mut state, None, None, 1.6, true).is_none());
        assert!(state.is_none());
    }

    #[test]
    fn own_strip_and_non_text_source_never_arm() {
        let mut state = None;
        step_join_dwell(&mut state, Some("story"), Some("story"), 0.0, false);
        assert!(state.is_none(), "a window cannot join itself");
        step_join_dwell(&mut state, None, Some("story"), 0.0, false);
        assert!(state.is_none(), "no draggable source, no dwell");
    }
}
