//! The Ctrl+F find bar: match scanning for the selected target window,
//! the target picker, and the next/previous controls.
//!
//! Split out of the `app` facade to keep it a facade (see
//! `tests/architecture.rs`). Match-cursor stepping itself lives in
//! `global_input.rs` alongside the keybind that drives it, so the buttons
//! here and the F3 keys can never diverge.

use super::*;

impl VellumGuiApp {

    /// Floating search bar shown while in Search mode (Ctrl+F). Matching
    /// segments highlight via the theme selection color in text windows.
    pub(super) fn render_search_bar(&mut self, ctx: &egui::Context) {
        if self.app_core.ui_state.input_mode != InputMode::Search {
            return;
        }

        // Count matching lines across visible text content, including the
        // active tab of tabbed windows (read-only pass before the window
        // closure takes mutable borrows). The scan is cached: buffer
        // generations only move when content changes, so an idle search bar
        // costs a fingerprint pass instead of a full-buffer rescan per frame.
        let query = self
            .app_core
            .ui_state
            .search_input
            .trim()
            .to_ascii_lowercase();
        // Every window the user can point the search at, and the one it is
        // pointed at now. The target is an explicit choice — never inferred
        // from focus or from which window happens to have the most hits.
        let targets = Self::search_targets(&self.app_core);
        let focus_fallback = self.app_core.get_focused_window_name();
        let target = self
            .search_target
            .clone()
            // Until the user picks, use the keyboard focus if it is itself
            // searchable, else the first target (stable, sorted).
            .or_else(|| {
                targets
                    .iter()
                    .find(|(_, id)| *id == focus_fallback)
                    .or_else(|| targets.first())
                    .map(|(_, id)| id.clone())
            });
        // Hits in the TARGET window only. The scan is cached: buffer
        // generations only move when content changes, so an idle search bar
        // costs a fingerprint comparison instead of a full rescan per frame.
        let matches: Vec<usize> = match (&target, query.is_empty()) {
            (Some(target), false) => {
                let fingerprint = self
                    .app_core
                    .ui_state
                    .windows
                    .values()
                    .filter_map(|window| match &window.content {
                        WindowContent::Text(content)
                        | WindowContent::Inventory(content)
                        | WindowContent::Reserve(content)
                        | WindowContent::Spells(content) => Some(content.generation),
                        WindowContent::TabbedText(tabbed) => tabbed
                            .tabs
                            .get(tabbed.active_tab_index)
                            .map(|tab| tab.content.generation ^ tabbed.active_tab_index as u64),
                        _ => None,
                    })
                    .fold(0u64, |acc, generation| acc.wrapping_add(generation));
                let key = format!("{target}\u{1}{query}");
                match &self.search_match_cache {
                    Some((cached_key, cached_fingerprint, cached_hits))
                        if *cached_key == key && *cached_fingerprint == fingerprint =>
                    {
                        cached_hits.clone()
                    }
                    _ => {
                        let hits = Self::search_hits_in(&self.app_core, target, &query)
                            .map(|(_, hits)| hits)
                            .unwrap_or_default();
                        // Only a NEW query or target restarts the cursor.
                        // Content changing must not: game text arrives
                        // constantly, and resetting here would throw the
                        // user back to the first hit mid-cycle. The cursor
                        // survives buffer drift on its own — it is stored as
                        // a line id, not an index (see search_match_index).
                        let scope_changed = self
                            .search_match_cache
                            .as_ref()
                            .is_none_or(|(cached_key, _, _)| *cached_key != key);
                        self.search_match_cache = Some((key, fingerprint, hits.clone()));
                        if scope_changed {
                            self.search_match_index = None;
                            self.search_match_window = None;
                        }
                        hits
                    }
                }
            }
            _ => Vec::new(),
        };
        let match_count = matches.len();
        // Position of the cursor within this target's hits, once the user
        // has stepped at least once.
        let position = self
            .search_match_index
            .filter(|_| self.search_match_window == target)
            .and_then(|absolute| {
                // The cursor is an absolute line number; map it back through
                // the current buffer so the readout stays correct as lines
                // scroll off the top.
                let content =
                    Self::search_content(&self.app_core, target.as_deref()?)?;
                let index = Self::line_index_of(content, absolute)?;
                matches.iter().position(|hit| *hit == index)
            });

        let mut close = false;
        // -1 = previous, +1 = next; applied after the window closure so the
        // jump can borrow self mutably.
        let mut step: i32 = 0;
        // Target picked from the dropdown this frame, applied below.
        let mut chose_target: Option<String> = None;
        egui::Window::new("gui_search_bar")
            .id(egui::Id::new("gui_search_bar"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 36.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Find:");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.app_core.ui_state.search_input)
                            .desired_width(200.0),
                    );
                    if self.search_bar_needs_focus {
                        response.request_focus();
                        self.search_bar_needs_focus = false;
                    }
                    // Enter advances (Shift+Enter goes back) without leaving
                    // the text field — the browser find-bar reflex.
                    if response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        step = if ui.input(|i| i.modifiers.shift) { -1 } else { 1 };
                        response.request_focus();
                    }
                    // Which window to search — an explicit choice, so a
                    // specific tab (thoughts, speech, story) or the room
                    // window can be searched without focusing it first.
                    ui.label("in");
                    let current_label = target
                        .as_ref()
                        .and_then(|id| {
                            targets
                                .iter()
                                .find(|(_, candidate)| candidate == id)
                                .map(|(label, _)| label.clone())
                        })
                        .unwrap_or_else(|| "(no text window)".to_string());
                    egui::ComboBox::from_id_salt("gui_search_target")
                        .selected_text(current_label)
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for (label, id) in &targets {
                                let selected = target.as_deref() == Some(id.as_str());
                                if ui.selectable_label(selected, label).clicked() && !selected {
                                    chose_target = Some(id.clone());
                                }
                            }
                        });
                    if query.is_empty() {
                        ui.weak("type to highlight matches");
                    } else if match_count == 0 {
                        ui.weak("no matches here");
                    } else {
                        // 1-based position once the user has stepped; the
                        // bare count before that.
                        match position {
                            Some(pos) => {
                                ui.weak(format!("{} of {}", pos + 1, match_count))
                            }
                            None => ui.weak(format!("{match_count} matching lines")),
                        };
                        if ui
                            .button("▴")
                            .on_hover_text("Previous match (Shift+F3)")
                            .clicked()
                        {
                            step = -1;
                        }
                        if ui
                            .button("▾")
                            .on_hover_text("Next match (F3)")
                            .clicked()
                        {
                            step = 1;
                        }
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });

        // Switching windows starts a fresh match list in the new target.
        if let Some(id) = chose_target {
            self.search_target = Some(id);
            self.search_match_index = None;
            self.search_match_window = None;
        } else if self.search_target.is_none() {
            // Pin the fallback the first time the bar resolves one, so the
            // dropdown and the keybinds agree on where F3 will go before the
            // user has touched the picker.
            self.search_target = target;
        }
        // Same path the F3 / Ctrl+PageDown keybinds take, so the buttons and
        // the keys can never drift apart.
        if step != 0 {
            self.step_search_match(step > 0, ctx);
        }

        if close {
            self.app_core.clear_search_mode();
        }
    }
}
