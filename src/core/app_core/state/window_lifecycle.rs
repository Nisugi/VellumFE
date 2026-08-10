//! Window lifecycle beyond construction: show/hide, delete-and-stash
//! with restore, ephemeral containers and dialog panels, known-window
//! toggling, discovery registration, declared-size hints, and WebUI
//! windows.

use super::*;

impl AppCore {
    /// Push a Text def's content settings (streams, buffer, compact,
    /// timestamps) onto the live window, rebuild stream routing, and re-feed
    /// bounty data. Editors that only replace the layout def otherwise leave
    /// the live window on its old settings until it is recreated.
    pub fn apply_text_content_settings(&mut self, def: &crate::config::WindowDef) {
        let crate::config::WindowDef::Text { data, .. } = def else {
            return;
        };
        let Some(window) = self.ui_state.windows.get_mut(def.name()) else {
            return;
        };
        let WindowContent::Text(text) = &mut window.content else {
            return;
        };
        text.streams = data.streams.clone();
        text.max_lines = data.buffer_size;
        text.compact = data.compact;
        text.show_timestamps = data.show_timestamps;
        if let Some(pos) = data.timestamp_position {
            text.timestamp_position = pos;
        }
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);
        self.refresh_bounty_window(def.name());
    }

    /// Rebuild a bounty-fed text window's lines from the cached bounty data,
    /// honoring its current compact flag. Compaction is applied at line
    /// ingestion, so toggling the flag otherwise only affects the NEXT
    /// bounty update — which made the editor's condense checkbox look inert
    /// until the window was closed and reopened.
    pub fn refresh_bounty_window(&mut self, name: &str) {
        if !self.game_state.bounty.has_data() {
            return;
        }
        let Some(window) = self.ui_state.windows.get_mut(name) else {
            return;
        };
        let WindowContent::Text(text) = &mut window.content else {
            return;
        };
        // Only rebuild windows fed solely by the bounty stream: mixed-stream
        // history can't be reconstructed from the bounty cache.
        let bounty_only = text.streams.len() == 1
            && text.streams[0].eq_ignore_ascii_case("bounty");
        if !bounty_only {
            return;
        }
        let lines: Vec<String> = if text.compact {
            self.game_state.bounty.compact_lines.clone()
        } else {
            vec![self.game_state.bounty.raw_text.clone()]
        };
        text.lines.clear();
        for line_text in lines {
            text.add_line(crate::data::widget::StyledLine::from_text_with_stream(
                line_text, "bounty",
            ));
        }
    }

    /// True if a shown window other than `excluding` carries the "main"
    /// stream — a text window subscribed to it, or a tabbedtext with a
    /// subscribed tab. The story feed must always have a live subscriber;
    /// hide_window gates on this instead of hard-protecting the window
    /// NAMED "main" (the feed may live in a tabbedtext tab instead).
    pub(super) fn main_stream_has_subscriber_excluding(&self, excluding: &str) -> bool {
        self.ui_state.windows.iter().any(|(win_name, window)| {
            if win_name == excluding {
                return false;
            }
            Self::window_subscribes_to_main(&window.content)
        })
    }

    pub(super) fn window_subscribes_to_main(content: &crate::data::WindowContent) -> bool {
        match content {
            crate::data::WindowContent::Text(text) => {
                text.streams.iter().any(|s| s.eq_ignore_ascii_case("main"))
            }
            crate::data::WindowContent::TabbedText(tabbed) => tabbed.tabs.iter().any(|tab| {
                tab.definition
                    .streams
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case("main"))
            }),
            _ => false,
        }
    }

    /// Hide a window (keep in layout for persistence, remove from UI)
    pub fn hide_window(&mut self, name: &str) {
        // Main-stream invariant: hiding the last shown subscriber of the
        // story feed would silently eat all main text.
        let hides_main_subscriber = self
            .ui_state
            .windows
            .get(name)
            .map(|w| Self::window_subscribes_to_main(&w.content))
            .unwrap_or(false);
        if hides_main_subscriber && !self.main_stream_has_subscriber_excluding(name) {
            self.add_system_message(
                "Cannot hide the only window showing the story (main) feed. \
                 Add the main stream to another window first.",
            );
            return;
        }

        // Find ALL windows with this name and mark as hidden (handles duplicates)
        let mut found_count = 0;
        let mut is_command_input = false;
        for window_def in self.layout.windows.iter_mut() {
            if window_def.name() == name && window_def.base().visibility.is_shown() {
                window_def.base_mut().visibility = crate::config::WindowVisibility::Hidden;
                is_command_input |= window_def.widget_type() == "command_input";
                found_count += 1;
            }
        }

        if found_count > 0 {
            // TUI force-show: persist the hidden flag (so the GUI honors
            // it) but keep the input line on screen — the TUI has no
            // fallback bar and would otherwise leave the user typing blind.
            if is_command_input && self.force_show_command_input {
                self.add_system_message(
                    "Command input hidden in the layout (GUI shows its fallback bar); \
                     the TUI keeps it visible.",
                );
                self.mark_layout_modified();
                self.needs_render = true;
                return;
            }
            // Remove from UI state (but keep in layout!)
            self.ui_state.remove_window(name);

            let msg = if found_count > 1 {
                format!(
                    "Window '{}' hidden ({} duplicates removed)",
                    name, found_count
                )
            } else {
                format!("Window '{}' hidden", name)
            };
            self.add_system_message(&msg);
            self.mark_layout_modified();
            self.needs_render = true;
            tracing::info!(
                "Hid {} instance(s) of window '{}' - template(s) preserved in layout",
                found_count,
                name
            );
        } else {
            self.add_system_message(&format!("Window '{}' not found or already hidden", name));
        }
    }

    /// Show a window (unhide it - restore from layout template)
    pub fn show_window(&mut self, name: &str, terminal_width: u16, terminal_height: u16) {
        // Use Layout's add_window() which handles both:
        // 1. Existing windows (just marks visible)
        // 2. New windows (creates from template and adds to layout)
        if let Err(e) = self.layout.add_window(name) {
            self.add_system_message(&format!("Failed to add window '{}': {}", name, e));
            return;
        }

        // Get the window definition (now guaranteed to exist)
        let window_def_clone = self
            .layout
            .windows
            .iter()
            .find(|w| w.name() == name)
            .expect("Window should exist after add_window")
            .clone();

        // Create in UI state from layout template
        self.add_new_window(&window_def_clone, terminal_width, terminal_height);

        self.add_system_message(&format!("Window '{}' shown", name));
        self.mark_layout_modified();
        self.needs_render = true;
        tracing::info!("Showed window '{}' - added to layout and UI state", name);
    }

    /// Create any window definitions this layout lacks, from a saved layout's
    /// captured defs. Used by the GUI `.loadlayout`: a named layout saved on
    /// one character carries the full window definitions, so loading it into a
    /// fresh profile (which only has the default windows) recreates the missing
    /// windows before the arrangement is reconciled. Windows already present
    /// are left untouched — their live content (buffered text, etc.) survives.
    /// Returns the names actually created.
    pub fn materialize_missing_windows(
        &mut self,
        defs: &[crate::config::WindowDef],
        terminal_width: u16,
        terminal_height: u16,
    ) -> Vec<String> {
        let mut created = Vec::new();
        for def in defs {
            let name = def.name().to_string();
            if self.ui_state.windows.contains_key(&name) {
                continue;
            }
            // Keep the layout's def list authoritative so a later .savelayout
            // (or autosave) re-persists the window; add_new_window only writes
            // ui_state.
            if !self.layout.windows.iter().any(|w| w.name() == name) {
                self.layout.windows.push(def.clone());
            }
            self.add_new_window(def, terminal_width, terminal_height);
            created.push(name);
        }
        if !created.is_empty() {
            self.needs_render = true;
        }
        created
    }

    /// Process pending window additions from openDialog events.
    /// Called by the frontend each frame with terminal dimensions.
    /// Whether a layout window equivalent to `template_name` already exists,
    /// regardless of its display name. Dialog-driven singleton widgets
    /// (experience/stance/encum/minivitals/injuries/buffs/…) get placed by
    /// the user under an auto-generated `custom-*` name, so a bare
    /// `w.name() == template_name` check misses them and the game re-adds a
    /// duplicate on every dialog re-send. Match on the template's WIDGET
    /// TYPE instead — plus the distinguishing data field for the two types
    /// that legitimately allow multiple instances (Progress `id`,
    /// ActiveEffects `category`), so a Buffs window doesn't shadow Debuffs
    /// and a stance bar doesn't shadow an unrelated progress bar.
    pub(super) fn layout_has_equivalent_window(&self, template_name: &str) -> bool {
        self.layout_equivalent_window_name(template_name).is_some()
    }

    /// The NAME of an existing layout window equivalent to `template_name`
    /// (see layout_has_equivalent_window for the identity rules), or None.
    pub(super) fn layout_equivalent_window_name(&self, template_name: &str) -> Option<String> {
        use crate::config::WindowDef;
        let template = crate::core::local_catalog::seed(template_name)?;
        let tmpl_type = template.widget_type();
        self.layout
            .windows
            .iter()
            .find(|w| {
                if w.widget_type() != tmpl_type {
                    return false;
                }
                match (&template, *w) {
                    // Disambiguate the shared types by their identity field.
                    (WindowDef::Progress { data: t, .. }, WindowDef::Progress { data: w, .. }) => {
                        t.id == w.id
                    }
                    (
                        WindowDef::ActiveEffects { data: t, .. },
                        WindowDef::ActiveEffects { data: w, .. },
                    ) => t.category.eq_ignore_ascii_case(&w.category),
                    // All other singleton types: one per layout, type is enough.
                    _ => true,
                }
            })
            .map(|w| w.name().to_string())
    }

    pub fn process_pending_window_additions(&mut self, terminal_width: u16, terminal_height: u16) {
        use crate::config::WindowBinding;
        // Drain pending additions. As of U2 these are DIALOG IDS (e.g.
        // "expr", "stance"), not template names — so we can bind the created
        // window to its game feed.
        let pending: Vec<String> = self.ui_state.pending_window_additions.drain(..).collect();

        for dialog_id in pending {
            // The claimed view's seed key, via the resolver (Phase 4).
            let template_name =
                Self::seed_template_for(&WindowBinding::Dialog(dialog_id.clone()));

            // Already have a window bound to this feed? The game only ever
            // needs one home per feed to create — refresh flows to all bound
            // windows via the normal data path, so just ensure UI state
            // exists for any shown bound window and move on (no duplicate).
            if self.layout.has_window_bound_to(&dialog_id) {
                let bound_shown: Vec<String> = self
                    .layout
                    .windows
                    .iter()
                    .filter(|w| {
                        w.base().binding.as_ref().is_some_and(|b| b.id() == dialog_id)
                            && w.base().visibility.is_shown()
                    })
                    .map(|w| w.name().to_string())
                    .collect();
                for name in bound_shown {
                    if !self.ui_state.windows.contains_key(&name) {
                        if let Some(def) = self.layout.windows.iter().find(|w| w.name() == name) {
                            let def = def.clone();
                            self.add_new_window(&def, terminal_width, terminal_height);
                            self.needs_render = true;
                            self.ui_state.needs_widget_reset = true;
                        }
                    }
                }
                continue;
            }

            // No bound window yet. A user may have an EQUIVALENT widget placed
            // under a renamed custom-* name (U0) — adopt it by tagging the
            // binding, so future feeds resolve by id and we never duplicate.
            if let Some(existing_name) = self.layout_equivalent_window_name(&template_name) {
                if let Some(def) = self
                    .layout
                    .windows
                    .iter_mut()
                    .find(|w| w.name() == existing_name)
                {
                    def.base_mut().binding = Some(WindowBinding::Dialog(dialog_id.clone()));
                }
                // Ensure UI state if it's shown.
                let shown = self
                    .layout
                    .windows
                    .iter()
                    .find(|w| w.name() == existing_name)
                    .map(|w| w.base().visibility.is_shown())
                    .unwrap_or(false);
                if shown && !self.ui_state.windows.contains_key(&existing_name) {
                    if let Some(def) = self.layout.windows.iter().find(|w| w.name() == existing_name)
                    {
                        let def = def.clone();
                        self.add_new_window(&def, terminal_width, terminal_height);
                        self.needs_render = true;
                        self.ui_state.needs_widget_reset = true;
                    }
                }
                continue;
            }

            // Genuinely new: add the templated window, bound to this feed.
            // (U2a keeps the current visible-spawn behavior; U2b gates on
            // visibility so a hidden binding suppresses the auto-spawn.)
            if let Err(e) = self.layout.add_window(&template_name) {
                tracing::warn!("Failed to auto-add window '{}': {}", template_name, e);
                continue;
            }
            let created = self
                .layout
                .windows
                .iter()
                .rev()
                .find(|w| w.widget_type() == template_name || w.name() == template_name)
                .or_else(|| self.layout.windows.last())
                .map(|w| w.name().to_string());
            if let Some(name) = created {
                if let Some(def) = self.layout.windows.iter_mut().find(|w| w.name() == name) {
                    def.base_mut().binding = Some(WindowBinding::Dialog(dialog_id.clone()));
                }
                if let Some(def) = self.layout.windows.iter().find(|w| w.name() == name) {
                    let def = def.clone();
                    self.add_new_window(&def, terminal_width, terminal_height);
                    tracing::info!("Auto-added bound window '{}' from openDialog '{}'", name, dialog_id);
                    self.needs_render = true;
                    self.ui_state.needs_widget_reset = true;
                }
            }
        }
    }

    /// Delete a window (legacy - use hide_window instead)
    pub(in crate::core::app_core) fn delete_window(&mut self, name: &str) {
        // For backwards compatibility, redirect to hide
        self.hide_window(name);
    }

    /// Permanently delete a window from the layout, but STASH its full def in
    /// `layout.deleted_windows` so it can be restored later. This is the honest
    /// "delete" (distinct from hide): the window leaves the Windows menu and
    /// stops rendering, yet a custom window — the only record of a moved
    /// command_input or a user-authored window that `+ Custom window` can't
    /// recreate — is never actually lost. Returns true if a window was deleted.
    pub fn delete_and_stash_window(&mut self, name: &str) -> bool {
        // Remove the live UI window.
        self.remove_window(name);
        // Pull the def out of the layout and stash it (newest last). If a def
        // with this name is already stashed, replace it (a re-delete after a
        // restore keeps one copy).
        let Some(pos) = self.layout.windows.iter().position(|w| w.name() == name) else {
            return false;
        };
        let def = self.layout.windows.remove(pos);
        // A dialog-bound window carries an entry in the popup allow-set
        // (shown_dialog_ids) whenever it was shown. hide_window clears that,
        // but delete must too — otherwise the id lingers and the next
        // dialogData the game sends re-pops the dialog as a bare popup
        // (titled "Dialog"), resurrecting a window the user deleted.
        if let Some(crate::config::WindowBinding::Dialog(id)) = def.base().binding.clone() {
            self.ui_state.shown_dialog_ids.remove(&id);
            if self
                .ui_state
                .active_dialog
                .as_ref()
                .is_some_and(|d| d.id == id)
            {
                self.ui_state.active_dialog = None;
            }
        }
        self.layout.deleted_windows.retain(|w| w.name() != name);
        self.layout.deleted_windows.push(def);
        self.mark_layout_modified();
        self.schedule_layout_autosave();
        true
    }

    /// Deleted windows the user can restore, newest first, as
    /// `(name, display_title)`: restore by the stable `name`, show the human
    /// `title` (falling back to the name when no title was set).
    pub fn deleted_windows_for_restore(&self) -> Vec<(String, String)> {
        self.layout
            .deleted_windows
            .iter()
            .rev()
            .map(|w| {
                let name = w.name().to_string();
                let title = w
                    .base()
                    .title
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| name.clone());
                (name, title)
            })
            .collect()
    }

    /// Just the internal names of restorable deleted windows, newest first.
    /// (Kept for tests / callers that only need the key.)
    pub fn deleted_window_names(&self) -> Vec<String> {
        self.layout
            .deleted_windows
            .iter()
            .rev()
            .map(|w| w.name().to_string())
            .collect()
    }

    /// Restore a previously deleted window by name: move its def out of the
    /// stash and back into the layout (Shown), then materialize the live
    /// window. Returns true if it was restored. If a live window with the same
    /// name now exists (the name was reused), the restore is refused so it
    /// can't clobber the current window.
    pub fn restore_deleted_window(&mut self, name: &str, width: u16, height: u16) -> bool {
        if self.layout.windows.iter().any(|w| w.name() == name) {
            self.add_system_message(&format!(
                "Can't restore '{name}': a window with that name already exists."
            ));
            return false;
        }
        let Some(pos) = self
            .layout
            .deleted_windows
            .iter()
            .position(|w| w.name() == name)
        else {
            return false;
        };
        let mut def = self.layout.deleted_windows.remove(pos);
        // A restored window comes back visible (Ephemeral would not persist).
        def.base_mut().visibility = crate::config::WindowVisibility::Shown;
        self.layout.windows.push(def);
        self.mark_layout_modified();
        // Rebuild the live windows so the restored def gets a UI window.
        self.init_windows(width, height);
        self.schedule_layout_autosave();
        self.add_system_message(&format!("Restored window '{name}'."));
        true
    }

    /// Create an ephemeral container window at screen center (or saved position if available)
    pub fn create_ephemeral_container_window(
        &mut self,
        container_title: &str,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        use crate::data::{WidgetType, WindowContent, WindowPosition, WindowState};

        // Use simple lowercase name for internal tracking (e.g., "bandolier")
        let window_name = container_title.replace(' ', "_").to_lowercase();

        // Skip if already exists
        if self.ui_state.windows.contains_key(&window_name) {
            tracing::debug!(
                "Container window '{}' already exists, skipping creation",
                window_name
            );
            return;
        }

        // Check for saved position, otherwise center with reasonable defaults
        let (x, y, w, h) = if let Some(saved) = self.saved_dialog_positions.containers.get(&window_name) {
            let width = saved.width.unwrap_or(40);
            let height = saved.height.unwrap_or(15);
            // Clamp to terminal bounds
            let x = saved.x.min(terminal_width.saturating_sub(width));
            let y = saved.y.min(terminal_height.saturating_sub(height));
            tracing::debug!("Using saved position for container '{}': ({}, {}) {}x{}", window_name, x, y, width, height);
            (x, y, width, height)
        } else {
            // Redesign Phase 3e: one placement policy, honoring the
            // declaration's own hints when the game sent any. (Container
            // hints are keyed by container id; only the title reaches
            // here, so containers ride the kind default until the id is
            // plumbed through — the panels below DO consume hints.)
            crate::core::placement::ephemeral_placement(
                None,
                (40, 15),
                crate::core::placement::PlacementAnchor::Center,
                (terminal_width, terminal_height),
            )
        };

        let window = WindowState {
            name: window_name.clone(),
            widget_type: WidgetType::Container,
            content: WindowContent::Container {
                container_title: container_title.to_string(),
            },
            position: WindowPosition {
                x: crate::data::geometry::Col::new(x),
                y: crate::data::geometry::Row::new(y),
                width: crate::data::geometry::Width::new(w),
                height: crate::data::geometry::Height::new(h),
            },
            visible: true,
            focused: false,
            content_align: None,
            ephemeral: true,
        };

        self.ui_state.set_window(window_name.clone(), window);
        self.ui_state.ephemeral_windows.insert(window_name);
        self.add_system_message(&format!("Created container window: {}", container_title));
        self.needs_render = true;

        tracing::info!(
            "Created ephemeral container window for '{}' at ({}, {})",
            container_title,
            x,
            y
        );
    }

    /// Create an ephemeral dockable panel window for a resident dialog
    /// (combat, befriend, ...). Positioned like an ephemeral container
    /// window; content renders from ui_state.dialog_store by `dialog_id`.
    pub fn create_dialog_panel_window(
        &mut self,
        dialog_id: &str,
        title: &str,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        use crate::data::{WidgetType, WindowContent, WindowPosition, WindowState};

        let window_name = format!("panel_{}", dialog_id.replace(' ', "_").to_lowercase());
        if self.ui_state.windows.contains_key(&window_name) {
            return;
        }

        // Redesign Phase 3e: seed rect from the single placement policy —
        // the dialog's own declaration hints (location/width/height from
        // openDialog, captured as WindowHints) win over the tall-narrow
        // kind default (26x20, right edge — combat is ~190x288 px).
        let (hx, hy, w, h) = crate::core::placement::ephemeral_placement(
            self.ui_state.window_hints.get(dialog_id).map(|v| v.as_slice()),
            (26, 20),
            crate::core::placement::PlacementAnchor::RightEdge,
            (terminal_width, terminal_height),
        );
        // A saved per-id position still beats the hint (user geometry is
        // always first in the placement precedence).
        let (x, y) = if let Some(saved) = self.saved_dialog_positions.dialogs.get(dialog_id) {
            (
                saved.x.min(terminal_width.saturating_sub(w)),
                saved.y.min(terminal_height.saturating_sub(h)),
            )
        } else {
            (hx, hy)
        };

        let window = WindowState {
            name: window_name.clone(),
            widget_type: WidgetType::DialogPanel,
            content: WindowContent::DialogPanel {
                dialog_id: dialog_id.to_string(),
            },
            position: WindowPosition {
                x: crate::data::geometry::Col::new(x),
                y: crate::data::geometry::Row::new(y),
                width: crate::data::geometry::Width::new(w),
                height: crate::data::geometry::Height::new(h),
            },
            visible: true,
            focused: false,
            content_align: None,
            ephemeral: true,
        };
        self.ui_state.set_window(window_name.clone(), window);
        self.ui_state.ephemeral_windows.insert(window_name);
        self.add_system_message(&format!("Opened {} panel", title));
        self.needs_render = true;
    }

    /// Apply a user show/hide choice from the "known windows" list: record
    /// the policy on the offer and create or close the corresponding
    /// window. Currently wires container offers to ephemeral container
    /// windows; dialog/stream offers record policy for now (their window
    /// wiring lands as those consumption paths migrate).
    /// U3: show or hide a known window by NAME (from enumerate_known_windows),
    /// with no offer registry. Dispatches on where the window lives:
    /// - a persistent LAYOUT window (incl. bound streams / dialog panels):
    ///   flip visibility via show_window / hide_window.
    /// - a session-only EPHEMERAL window (container, ad-hoc panel): create
    ///   or remove the runtime window.
    /// - the bank-style POPUP: materialize from / clear active_dialog.
    pub fn set_known_window_shown(
        &mut self,
        name: &str,
        shown: bool,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        // Persistent layout window? (streams, dialog panels, plain widgets)
        if let Some(win) = self.layout.windows.iter().find(|w| w.name() == name) {
            // Keep the dialog-popup allow-set in sync — but ONLY for dialogs
            // that render as a transient popup (bank/shop). A DialogPanel
            // widget (combat, UberBar) renders the dialog store IN THE PANEL;
            // adding its id here would ALSO pop it up as an active_dialog,
            // producing a duplicate (an empty panel + a populated popup, or
            // vice-versa). So a panel-bound dialog must stay out of the set.
            let is_dialog_panel =
                matches!(win, crate::config::WindowDef::DialogPanel { .. });
            if let Some(crate::config::WindowBinding::Dialog(id)) = win.base().binding.clone() {
                if !is_dialog_panel {
                    if shown {
                        self.ui_state.shown_dialog_ids.insert(id);
                    } else {
                        self.ui_state.shown_dialog_ids.remove(&id);
                    }
                }
            }
            if shown {
                self.show_window(name, terminal_width, terminal_height);
            } else {
                self.hide_window(name);
            }
            return;
        }

        // Ephemeral runtime window already present (container/panel): just
        // toggle its presence.
        if let Some(win) = self.ui_state.windows.get(name) {
            // A container also drops out of the session "shown" set so it
            // doesn't re-open on the next sighting.
            let container_title = match &win.content {
                crate::data::WindowContent::Container { container_title } => {
                    Some(container_title.clone())
                }
                _ => None,
            };
            if !shown {
                self.ui_state.remove_window(name);
                self.ui_state.ephemeral_windows.remove(name);
                if let Some(t) = container_title {
                    self.ui_state.shown_container_titles.remove(&t);
                }
                self.needs_render = true;
            }
            // (Re-showing an already-present ephemeral window is a no-op.)
            return;
        }

        // Not yet materialized. Conjure when shown, MOST-SPECIFIC FIRST.
        if shown {
            // A real widget template ALWAYS wins. It is the least ambiguous
            // meaning of a name, so it must beat both the generic dialog panel
            // and the container branches below. This matters because a
            // deleted-then-reshown widget whose id the game ALSO feeds as a
            // resident dialog (minivitals, expr, encum, Buffs, injuries,
            // stance, status indicators, ...) leaves an entry in the always-on
            // dialog store; without template-first, that store entry would
            // resurrect the widget as a bare `panel_<id>` instead of the real
            // widget. (A future container title colliding with a template name
            // would be the same trap one branch down — template-first closes
            // both.) show_window adds the def from the template + materializes.
            if crate::core::local_catalog::seed(name).is_some() {
                self.show_window(name, terminal_width, terminal_height);
                return;
            }
            // A remembered binding from discovery memory
            // (window_registry.toml): conjure the bound PERSISTENT window
            // exactly as a live discovery would, then show it. This
            // outranks the ephemeral dialog-store branch below — live-test
            // finding (Nisugi, bank): delete + reshow used to fall to the
            // store branch and produce a different, session-only
            // `panel_<id>` window instead of the persistent bound one the
            // first Show created.
            if let Some(entry) = self
                .window_registry
                .bindings
                .iter()
                .find(|b| b.id == name)
                .cloned()
            {
                let binding = match entry.kind.as_str() {
                    "stream" => Some(crate::config::WindowBinding::Stream(entry.id.clone())),
                    "dialog" => Some(crate::config::WindowBinding::Dialog(entry.id.clone())),
                    _ => None,
                };
                if let Some(binding) = binding {
                    let template = Self::seed_template_for(&binding);
                    if let Some(win_name) =
                        self.layout.register_discovered_window(binding, &template)
                    {
                        if !entry.title.is_empty() {
                            if let Some(def) = self
                                .layout
                                .windows
                                .iter_mut()
                                .find(|w| w.name() == win_name)
                            {
                                def.base_mut().title = Some(entry.title.clone());
                            }
                        }
                        self.apply_declared_size_hint(&win_name, &entry.id);
                        self.mark_layout_modified();
                        self.show_window(&win_name, terminal_width, terminal_height);
                        self.needs_render = true;
                    }
                    return;
                }
            }
            // A dialog-store entry the registry does NOT remember (rare:
            // discoveries record into the registry) → the legacy GENERIC
            // ephemeral panel rendered from the store by id.
            if self.ui_state.dialog_store.contains_key(name) {
                self.create_dialog_panel_window(name, name, terminal_width, terminal_height);
                self.needs_render = true;
                return;
            }
            // A sighted registry container (window name is title-derived) →
            // remember the opt-in and open it.
            let container_title = self
                .game_state
                .objects
                .containers()
                .find(|c| c.title.replace(' ', "_").to_lowercase() == name)
                .map(|c| c.title.clone());
            if let Some(title) = container_title {
                self.ui_state.shown_container_titles.insert(title.clone());
                self.create_ephemeral_container_window(&title, terminal_width, terminal_height);
                self.needs_render = true;
            }
        }
    }

    /// Realize game-offered windows after a batch of server messages, once
    /// terminal dimensions are known (called from every frontend's tick).
    /// Replaces the old all-or-nothing container discovery mode: a sighted
    /// container auto-(re)opens only if its offer policy says Shown, and
    /// openDialog-templated widgets queued by the message processor get
    /// added to the layout.
    pub fn realize_offered_windows(&mut self, terminal_width: u16, terminal_height: u16) {
        // Drain game-window discoveries the message processor observed into
        // the layout (it can't reach the layout itself). U3: streams and
        // resident dialog panels become bound, Hidden-by-default layout
        // entries — known forever, not auto-shown. Idempotent per binding.
        let discoveries: Vec<crate::data::WindowDiscovery> =
            self.ui_state.pending_window_discoveries.drain(..).collect();
        for d in discoveries {
            self.register_window_discovery(d);
        }

        // Redesign Phase 4d — expose = show. Rules (owner-decided, wire
        // verified): a KNOWN window's Show flag is the permission — Hidden
        // blocks the expose (Hidden already means "suppress game
        // auto-spawn", the U3 unified rule); an id arriving via expose for
        // the FIRST time registers bound and shows (default allowed). A
        // popup currently active under this id stays the popup path's
        // business (bank: openDialog popup + exposeDialog ride together;
        // U5's persistent bank row remains deferred).
        let exposes: Vec<(String, String)> =
            self.ui_state.pending_exposes.drain(..).collect();
        for (kind, id) in exposes {
            if self
                .ui_state
                .active_dialog
                .as_ref()
                .is_some_and(|dialog| dialog.id == id)
            {
                continue;
            }
            if self.layout.has_window_bound_to(&id) {
                let target = self
                    .layout
                    .windows
                    .iter()
                    .find(|w| w.base().binding.as_ref().is_some_and(|b| b.id() == id))
                    .map(|w| (w.name().to_string(), w.base().visibility));
                if let Some((name, visibility)) = target {
                    use crate::config::WindowVisibility;
                    if visibility == WindowVisibility::Hidden {
                        tracing::debug!("expose {kind} {id}: blocked (user hid the window)");
                        continue;
                    }
                    if !self.ui_state.windows.contains_key(&name) {
                        self.show_window(&name, terminal_width, terminal_height);
                        self.needs_render = true;
                    }
                    self.ui_state.expose_shown_ids.insert(id);
                }
                continue;
            }
            // First arrival via expose. Streams (charprofile-class, the
            // wire-verified case) register bound and show — the expose
            // default. Unknown DIALOG ids stay with the popup machinery
            // for now: bank's exposeDialog rides beside its popup flow,
            // and registering it as a panel would duplicate the popup
            // (its persistent hidden-unless-exposed row is U5's save-attr
            // work).
            if kind == "stream" {
                use crate::config::WindowBinding;
                let binding = WindowBinding::Stream(id.clone());
                let template = Self::seed_template_for(&binding);
                if let Some(name) =
                    self.layout.register_discovered_window(binding, &template)
                {
                    self.apply_declared_size_hint(&name, &id);
                    self.mark_layout_modified();
                    self.show_window(&name, terminal_width, terminal_height);
                    self.ui_state.expose_shown_ids.insert(id);
                    self.needs_render = true;
                }
            } else {
                tracing::debug!("expose {kind} {id}: unbound dialog left to the popup path");
            }
        }

        // The matching dismissals: dematerialize exactly the windows an
        // expose showed this session — WITHOUT flipping their persisted
        // visibility to Hidden (Hidden is the user's block lever; a game
        // dismissal must not eat the NEXT expose — bank re-exposes every
        // visit). Never-opened ids the game closes defensively
        // (withdraw/deposit) no-op here.
        let closes: Vec<String> = self.ui_state.pending_expose_closes.drain(..).collect();
        for id in closes {
            if !self.ui_state.expose_shown_ids.remove(&id) {
                continue;
            }
            let name = self
                .layout
                .windows
                .iter()
                .find(|w| w.base().binding.as_ref().is_some_and(|b| b.id() == id))
                .map(|w| w.name().to_string());
            if let Some(name) = name {
                self.ui_state.remove_window(&name);
                self.needs_render = true;
            }
        }

        if let Some((_id, title)) = self.message_processor.newly_registered_container.take() {
            // U3: a sighted container (re)opens only if the user opted it in
            // this session (via the Windows list). Ephemeral, wiped on relog.
            if self.ui_state.shown_container_titles.contains(&title) {
                self.create_ephemeral_container_window(&title, terminal_width, terminal_height);
            }
        }
        self.process_pending_window_additions(terminal_width, terminal_height);
    }

    /// Register a game-window discovery into the layout as a bound entry.
    /// Streams and resident dialog panels become persistent Hidden layout
    /// windows (known forever); hidden-until-shown is the universal
    /// default. No-op if a window is already bound to this id.
    /// Apply the game's DECLARED size (openDialog/streamWindow
    /// width/height px, captured as WindowHints) to a newly created bound
    /// window. CREATION TIME ONLY — the precedence is saved local
    /// geometry → declared size → default, and applying only at creation
    /// means user resizes and saved layouts always win afterward and a
    /// re-sent hint can never clobber them. Generic views only: dedicated
    /// widgets (expr, minivitals, …) keep their curated sizes — the
    /// binding is the game's, the presentation is ours.
    /// The declared (width, height) px for a game id from THIS session's
    /// hints; components <= 1 are treated as unset.
    pub(super) fn declared_size_from_hints(&self, game_id: &str) -> Option<(f32, f32)> {
        let hints = self.ui_state.window_hints.get(game_id)?;
        let dim = |name: &str| {
            hints
                .iter()
                .find(|(k, _)| k == name)
                .and_then(|(_, v)| v.parse::<f32>().ok())
                .filter(|v| *v > 1.0)
                .unwrap_or(0.0)
        };
        let (w, h) = (dim("width"), dim("height"));
        (w > 1.0 || h > 1.0).then_some((w, h))
    }

    pub(super) fn apply_declared_size_hint(&mut self, window_name: &str, game_id: &str) {
        // Session hints first; the cross-session registry memory second
        // (a fresh session's conjure still gets the declared shape).
        let declared = self.declared_size_from_hints(game_id).or_else(|| {
            self.window_registry
                .bindings
                .iter()
                .find(|b| b.id == game_id)
                .and_then(|b| b.declared_size)
        });
        let Some((wpx, hpx)) = declared else {
            return;
        };
        let (w, h) = (
            (wpx > 1.0).then_some(wpx),
            (hpx > 1.0).then_some(hpx),
        );
        let Some(def) = self
            .layout
            .windows
            .iter_mut()
            .find(|def| def.name() == window_name)
        else {
            return;
        };
        if !matches!(def.widget_type(), "text" | "dialogpanel" | "container") {
            return;
        }
        let base = def.base_mut();
        if let Some(hpx) = h {
            // Content px → cells (~16px rows) + title-bar row.
            let rows = ((hpx / 16.0).ceil() as u16 + 1).clamp(3, 80);
            base.rows = crate::data::geometry::Height::new(rows);
        }
        if let Some(wpx) = w {
            let cols = ((wpx / 8.0).ceil() as u16 + 2).clamp(12, 240);
            base.cols = crate::data::geometry::Width::new(cols);
        }
        self.mark_layout_modified();
    }

    /// The seed key a bound window is created from, via the presentation
    /// resolver (redesign Phase 3): a dedicated view's widget template,
    /// or the generic view for the binding's kind.
    pub(super) fn seed_template_for(binding: &crate::config::WindowBinding) -> String {
        use crate::core::view_resolver::resolve_view;
        use crate::data::view_kind::ViewKind;
        match resolve_view(binding, None) {
            ViewKind::Dedicated(key) => key,
            ViewKind::Text => "text_custom".to_string(),
            ViewKind::DialogPanel => "dialogpanel".to_string(),
            ViewKind::Container => "container".to_string(),
        }
    }

    pub(super) fn register_window_discovery(&mut self, d: crate::data::WindowDiscovery) {
        use crate::config::{WindowBinding, WindowVisibility, WindowDef};
        use crate::data::WindowDiscoveryKind;

        // Discovery memory (Phase 1b): every sighting is recorded — even
        // for ids that already have a bound window (a re-declaration can
        // carry a better title) and for popup dialogs that never become
        // layout windows. The write is deferred to the frontend-driven
        // autosave tick (same driver as the layout), so pure-core tests
        // never touch the filesystem and a failed write never disturbs
        // the session.
        let registry_kind = match d.kind {
            WindowDiscoveryKind::Stream => "stream",
            WindowDiscoveryKind::DialogPanel | WindowDiscoveryKind::DialogPopup => "dialog",
        };
        if self.window_registry.record(registry_kind, &d.id, &d.title) {
            self.window_registry_dirty = true;
        }
        if let Some((w, h)) = self.declared_size_from_hints(&d.id) {
            if self.window_registry.record_declared_size(&d.id, (w, h)) {
                self.window_registry_dirty = true;
            }
        }

        // Windows the user conjured from the catalog THIS session aren't
        // binding-tagged until the next load (backfill runs at load time),
        // so the game's own declaration would create a bound DUPLICATE
        // under the same name (fresh-profile live test: two 'Room' rows +
        // a GUI widget-id clash). Backfill is idempotent and cheap — tag now
        // so has_window_bound_to adopts instead of duplicating.
        if crate::config::Layout::backfill_bindings(&mut self.layout) > 0 {
            self.mark_layout_modified();
        }

        if self.layout.has_window_bound_to(&d.id) {
            return;
        }

        // ADOPT an existing window instead of creating a duplicate:
        // - a stream whose id a text/inventory window already subscribes to
        //   (the default layout ships thoughts/speech/society/inv/... windows
        //   that predate binding — tag them so the discovery doesn't make a
        //   second "thoughts" beside the shipped "Thoughts").
        if d.kind == WindowDiscoveryKind::Stream {
            // A single-stream window already showing this id: ADOPT it (tag
            // the binding) so it becomes the one true home for the stream.
            let single = self.layout.windows.iter_mut().find(|w| match w {
                WindowDef::Text { data, .. } => data.streams.iter().any(|s| s == &d.id),
                WindowDef::Inventory { data, .. } | WindowDef::Reserve { data, .. } => {
                    data.streams.iter().any(|s| s == &d.id)
                }
                _ => false,
            });
            if let Some(w) = single {
                if w.base().binding.is_none() {
                    w.base_mut().binding = Some(WindowBinding::Stream(d.id.clone()));
                    self.mark_layout_modified();
                }
                return;
            }
            // A MULTI-stream window (tabbedtext) already routes this stream
            // through a tab: don't create a duplicate, and don't bind the
            // whole window (it carries many streams). The tab handles it.
            let in_tab = self.layout.windows.iter().any(|w| match w {
                WindowDef::TabbedText { data, .. } => data.tabs.iter().any(|t| {
                    t.streams.iter().any(|s| s == &d.id)
                        || t.stream.as_deref() == Some(d.id.as_str())
                }),
                _ => false,
            });
            if in_tab {
                return;
            }
        }

        // Pick the seed view + binding for this discovery kind through the
        // presentation resolver (redesign Phase 3: seed_template_for routes
        // the binding through resolve_view, not scattered id-maps — a
        // dedicated view resolves to its widget's seed, everything else to
        // the generic view for its kind).
        let binding = match d.kind {
            WindowDiscoveryKind::Stream => WindowBinding::Stream(d.id.clone()),
            WindowDiscoveryKind::DialogPanel => WindowBinding::Dialog(d.id.clone()),
            // Popups (bank) aren't layout widgets; they're handled by the
            // active_dialog popup path. Skip layout registration for now
            // (U5 gives bank a first-class row).
            WindowDiscoveryKind::DialogPopup => return,
        };
        let template = Self::seed_template_for(&binding);

        if let Some(name) = self.layout.register_discovered_window(binding, &template) {
            // A new discovery changes the layout — mark it so the autosave
            // (or .savelayout) persists it, making the window known forever.
            self.mark_layout_modified();
            // Size from the game's own declaration when it sent one.
            self.apply_declared_size_hint(&name, &d.id);
            // Set a friendly title + Shown/Hidden default.
            if let Some(def) = self.layout.windows.iter_mut().find(|w| w.name() == name) {
                if !d.title.is_empty() {
                    def.base_mut().title = Some(d.title.clone());
                }
                // Freshly discovered windows are Hidden (U3:
                // hidden-by-default); this is where a future policy
                // (e.g. resident streams shown) would flip it.
                def.base_mut().visibility = WindowVisibility::Hidden;
                // Wire the widget to its game feed by id.
                match (d.kind, def) {
                    // A stream text window subscribes to the stream id.
                    (
                        WindowDiscoveryKind::Stream,
                        crate::config::WindowDef::Text { data, .. },
                    ) => {
                        if !data.streams.contains(&d.id) {
                            data.streams.push(d.id.clone());
                        }
                    }
                    // A dialog panel renders from the dialog store by id.
                    (
                        WindowDiscoveryKind::DialogPanel,
                        crate::config::WindowDef::DialogPanel { data, .. },
                    ) => {
                        data.dialog_id = d.id.clone();
                    }
                    _ => {}
                }
            }
        }
    }

    /// Close all ephemeral container windows
    pub fn close_all_ephemeral_windows(&mut self) {
        let names: Vec<_> = self.ui_state.ephemeral_windows.iter().cloned().collect();
        let count = names.len();

        for name in names {
            self.ui_state.remove_window(&name);
        }
        self.ui_state.ephemeral_windows.clear();

        if count > 0 {
            self.add_system_message(&format!("Closed {} container window(s)", count));
            self.needs_render = true;
        } else {
            self.add_system_message("No container windows to close");
        }
    }

    /// Close ephemeral container window by title (case-insensitive partial match)
    pub fn close_ephemeral_window_by_title(&mut self, title: &str) {
        // Window names are built as lowercase-with-underscores (see
        // create_ephemeral_container_window), so normalize the needle the
        // same way or multi-word titles like "My Pack" never match.
        let title_lower = title.to_lowercase().replace(' ', "_");

        // Find matching ephemeral windows
        let matches: Vec<_> = self
            .ui_state
            .ephemeral_windows
            .iter()
            .filter(|name| name.to_lowercase().contains(&title_lower))
            .cloned()
            .collect();

        if matches.is_empty() {
            self.add_system_message(&format!("No container window matching '{}'", title));
            return;
        }

        for name in &matches {
            self.ui_state.remove_window(name);
            self.ui_state.ephemeral_windows.remove(name);
        }

        self.add_system_message(&format!("Closed {} container window(s)", matches.len()));
        self.needs_render = true;
    }

    /// Add a new window
    pub(in crate::core::app_core) fn add_window(
        &mut self,
        name: &str,
        widget_type_str: &str,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) {
        use crate::config::WindowDef;
        use crate::data::{
            CompassData, CountdownData, IndicatorData, PerceptionData, ProgressData, RoomContent,
            TextContent, WidgetType, WindowContent, WindowPosition, WindowState,
        };

        // Check if window already exists
        if self.ui_state.windows.contains_key(name) {
            self.add_system_message(&format!("Window '{}' already exists", name));
            return;
        }

        // Parse widget type
        let widget_type = match WidgetType::try_from_str(widget_type_str) {
            Some(wt) => wt,
            None => {
                self.add_system_message(&format!("Unknown widget type: {}", widget_type_str));
                self.add_system_message(&format!("Valid types: {}", WidgetType::VALID_TYPES.join(", ")));
                return;
            }
        };

        // Create window content based on type
        let content = match widget_type {
            WidgetType::Text => WindowContent::Text(TextContent::new(name, 1000)),
            WidgetType::Progress => WindowContent::Progress(ProgressData {
                value: 100,
                max: 100,
                label: name.to_string(),
                color: None,
                progress_id: name.to_string(),
                numbers_only: false,
                current_only: false,
            }),
            WidgetType::Countdown => WindowContent::Countdown(CountdownData {
                end_time: 0,
                label: name.to_string(),
                countdown_id: name.to_string(),
                color: None,
                show_when_zero: false,
            }),
            WidgetType::Map => WindowContent::Map(crate::data::MapData::default()),
            WidgetType::Compass => WindowContent::Compass(CompassData {
                directions: Vec::new(),
            }),
            WidgetType::InjuryDoll => WindowContent::InjuryDoll(InjuryDollData::new()),
            WidgetType::Hand => WindowContent::Hand {
                item: None,
                link: None,
            },
            WidgetType::Room => WindowContent::Room(RoomContent {
                name: String::new(),
                description: Vec::new(),
                exits: Vec::new(),
                players: Vec::new(),
                objects: Vec::new(),
            }),
            WidgetType::Indicator => WindowContent::Indicator(IndicatorData {
                indicator_id: name.to_string(),
                active: false,
                color: None,
            }),
            WidgetType::Performance => WindowContent::Performance,
            WidgetType::Perception => WindowContent::Perception(PerceptionData {
                entries: Vec::new(),
                last_update: 0,
                generation: 0,
            }),
            WidgetType::CommandInput => WindowContent::CommandInput {
                text: String::new(),
                cursor: 0,
                history: Vec::new(),
                history_index: None,
            },
            WidgetType::Inventory => {
                let mut content = TextContent::new(name, 0);
                content.streams = vec!["inv".to_string()];
                WindowContent::Inventory(content)
            }
            WidgetType::Reserve => {
                let mut content = TextContent::new(name, 0);
                content.streams = vec!["reserve".to_string()];
                WindowContent::Reserve(content)
            }
            WidgetType::Spells => {
                let mut content = TextContent::new(name, 0);
                content.streams = vec!["Spells".to_string()];
                WindowContent::Spells(content)
            }
            WidgetType::Dashboard => WindowContent::Dashboard {
                indicators: Vec::new(),
            },
            WidgetType::ActiveEffects => WindowContent::ActiveEffects(crate::data::ActiveEffectsContent {
                category: "Unknown".to_string(),
                effects: Vec::new(),
                generation: 0,
            }),
            WidgetType::Targets => WindowContent::Targets,
            WidgetType::Players => WindowContent::Players,
            WidgetType::Items => WindowContent::Items,
            WidgetType::Container => WindowContent::Container {
                container_title: String::new(),
            },
            WidgetType::Experience => WindowContent::Experience,
            WidgetType::GS4Experience => WindowContent::GS4Experience,
            WidgetType::Encumbrance => WindowContent::Encumbrance,
            WidgetType::MiniVitals => WindowContent::MiniVitals,
            WidgetType::Betrayer => WindowContent::Betrayer,
            // A dot-command-created hotkeybar binds to the bar with the
            // same name as the window
            WidgetType::Hotkeybar => WindowContent::Hotkeybar {
                bar: name.to_string(),
            },
            WidgetType::WebUi => WindowContent::WebUi(
                crate::data::webui::WebUiPanelContent::new(name, name),
            ),
            // Name-based creation path: bind the panel to the window name.
            WidgetType::DialogPanel => WindowContent::DialogPanel {
                dialog_id: name.to_string(),
            },
            _ => WindowContent::Empty,
        };

        if widget_type == WidgetType::Performance {
            // Restart peaks/spike log so they describe this viewing session.
            self.perf_stats.reset_peaks();
        }

        // Create window state
        let window = WindowState {
            name: name.to_string(),
            widget_type: widget_type.clone(),
            content,
            position: WindowPosition {
                x: crate::data::geometry::Col::new(x),
                y: crate::data::geometry::Row::new(y),
                width: crate::data::geometry::Width::new(width),
                height: crate::data::geometry::Height::new(height),
            },
            visible: true,
            content_align: None,
            focused: false,
            ephemeral: false,
        };

        // Add to UI state
        self.ui_state.set_window(name.to_string(), window);

        // Create window definition for layout
        use crate::config::{BorderSides, TextWidgetData, WindowBase};

        let base = WindowBase {
            name: name.to_string(),
            row: crate::data::geometry::Row::new(y),
            col: crate::data::geometry::Col::new(x),
            rows: crate::data::geometry::Height::new(height),
            cols: crate::data::geometry::Width::new(width),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: true,
            title: Some(name.to_string()),
            title_position: "top-left".to_string(),
            background_color: None,
            text_color: None,
            transparent_background: false,
            locked: false,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            visibility: crate::config::WindowVisibility::Shown,
            binding: None,
            content_align: None,
            tts_speak: false,
            text_size: None,
            font_family: None,
        };

        // Persist the window with its REAL widget type. Previously only
        // text/room/command_input/webui were handled and every other type fell
        // back to WindowDef::Text, so progress/countdown/compass/indicator/hand
        // windows reloaded as empty text boxes (and landed in the wrong resize
        // bucket). WindowDef::blank builds the correct variant for each type.
        //
        // `widget_type_str` was already validated by WidgetType::try_from_str
        // near the top of this function, so blank() cannot return None here;
        // fall back to a plain text def defensively rather than panicking.
        let fallback_base = base.clone();
        let window_def = WindowDef::blank(widget_type_str, base).unwrap_or_else(|| {
            WindowDef::Text {
                base: fallback_base,
                data: TextWidgetData {
                    streams: vec![],
                    buffer_size: 10_000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            }
        });

        // Add to layout at the front (so new windows appear on top)
        self.layout.windows.insert(0, window_def);

        self.add_system_message(&format!(
            "Window '{}' added ({}x{} at {},{}) - type: {}",
            name, width, height, x, y, widget_type_str
        ));
        self.needs_render = true;

        // Update text stream subscriber map (new window may have stream subscriptions)
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);

        // Clear inventory cache if this is an inventory window to force initial render
        if widget_type == WidgetType::Inventory {
            self.message_processor.clear_inventory_cache();
        }

        // Populate spells window from buffer if this is a spells window
        // Spells are sent once at login, so we populate immediately from buffer
        if widget_type == WidgetType::Spells {
            if let Some(window) = self.ui_state.windows.get_mut(name) {
                if let WindowContent::Spells(ref mut content) = window.content {
                    self.message_processor.populate_spells_window(content);
                }
            }
        }
    }

    /// Create (or reuse) a window bound to a Lich WebUI page. Returns the
    /// window name (`webui:<page_id>`). The size hint is the descriptor's
    /// preferred content size in CSS pixels, mapped to core layout cells.
    pub fn add_webui_window(
        &mut self,
        page_id: &str,
        title: &str,
        size_hint: Option<[f32; 2]>,
        kind: Option<String>,
    ) -> String {
        use crate::data::{WidgetType, WindowContent, WindowPosition, WindowState};

        let name = format!("webui:{}", page_id);
        if self.ui_state.windows.contains_key(&name) {
            return name;
        }

        // Rough CSS-px -> layout-cell mapping (8x16 px cells), floored to a
        // usable minimum so tiny hints still get a visible window.
        let (width, height) = match size_hint {
            Some([w, h]) => (
                ((w / 8.0).ceil() as u16).clamp(20, 120),
                ((h / 16.0).ceil() as u16).clamp(4, 60),
            ),
            None => (40, 12),
        };

        let mut content = crate::data::webui::WebUiPanelContent::new(page_id, title);
        content.kind = kind;
        let window = WindowState {
            name: name.clone(),
            widget_type: WidgetType::WebUi,
            content: WindowContent::WebUi(content),
            position: WindowPosition {
                x: crate::data::geometry::Col::new(0),
                y: crate::data::geometry::Row::new(0),
                width: crate::data::geometry::Width::new(width),
                height: crate::data::geometry::Height::new(height),
            },
            visible: true,
            content_align: None,
            focused: false,
            ephemeral: false,
        };
        self.ui_state.set_window(name.clone(), window);

        let base = crate::config::WindowBase {
            name: name.clone(),
            row: crate::data::geometry::Row::new(0),
            col: crate::data::geometry::Col::new(0),
            rows: crate::data::geometry::Height::new(height),
            cols: crate::data::geometry::Width::new(width),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: crate::config::BorderSides::default(),
            border_color: None,
            show_title: true,
            title: Some(title.to_string()),
            title_position: "top-left".to_string(),
            background_color: None,
            text_color: None,
            transparent_background: false,
            locked: false,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            visibility: crate::config::WindowVisibility::Shown,
            binding: None,
            content_align: None,
            tts_speak: false,
            text_size: None,
            font_family: None,
        };
        self.layout.windows.insert(
            0,
            crate::config::WindowDef::WebUi {
                base,
                data: crate::config::WebUiWidgetData {
                    page: page_id.to_string(),
                },
            },
        );
        self.needs_render = true;
        name
    }

    /// Rename a window's title
    pub(in crate::core::app_core) fn rename_window(&mut self, window_name: &str, new_title: &str) {
        // Update in layout definition
        if let Some(window_def) = self
            .layout
            .windows
            .iter_mut()
            .find(|w| w.name() == window_name)
        {
            window_def.base_mut().title = Some(new_title.to_string());
            self.add_system_message(&format!(
                "Window '{}' renamed to '{}'",
                window_name, new_title
            ));
            self.needs_render = true;
        } else {
            self.add_system_message(&format!("Window '{}' not found", window_name));
        }
    }

    /// Set window border style and color
    pub(in crate::core::app_core) fn set_window_border(&mut self, window_name: &str, style: &str, color: Option<String>) {
        if let Some(window_def) = self
            .layout
            .windows
            .iter_mut()
            .find(|w| w.name() == window_name)
        {
            use crate::config::BorderSides;

            let style_lower = style.to_lowercase();
            let (new_show, new_sides) = match style_lower.as_str() {
                "none" => (false, window_def.base().border_sides.clone()),
                "all" => (true, BorderSides::default()),
                "top" => (
                    true,
                    BorderSides {
                        top: true,
                        bottom: false,
                        left: false,
                        right: false,
                    },
                ),
                "bottom" => (
                    true,
                    BorderSides {
                        top: false,
                        bottom: true,
                        left: false,
                        right: false,
                    },
                ),
                "left" => (
                    true,
                    BorderSides {
                        top: false,
                        bottom: false,
                        left: true,
                        right: false,
                    },
                ),
                "right" => (
                    true,
                    BorderSides {
                        top: false,
                        bottom: false,
                        left: false,
                        right: true,
                    },
                ),
                _ => {
                    self.add_system_message(&format!("Unknown border style: {}", style));
                    return;
                }
            };

            window_def
                .base_mut()
                .apply_border_configuration(new_show, new_sides);

            // Set border color if provided
            if let Some(c) = color {
                window_def.base_mut().border_color = Some(c);
            }

            // Recalculate and update window positions since rows/cols changed
            let width = self.layout.terminal_width.unwrap_or(80);
            let height = self.layout.terminal_height.unwrap_or(24);
            let positions = self.calculate_window_positions(width, height);
            for (name, position) in positions {
                if let Some(window) = self.ui_state.get_window_mut(&name) {
                    window.position = position;
                }
            }

            self.add_system_message(&format!("Border updated for window '{}'", window_name));
            self.mark_layout_modified();
            self.ui_state.needs_widget_reset = true;
            self.needs_render = true;
        } else {
            self.add_system_message(&format!("Window '{}' not found", window_name));
        }
    }

    /// Toggle transparent_background for all windows in the current layout.
    pub(in crate::core::app_core) fn toggle_transparent_background_all(&mut self) {
        if self.layout.windows.is_empty() {
            self.add_system_message("No windows found in layout");
            return;
        }

        let enable = self
            .layout
            .windows
            .iter()
            .any(|w| !w.base().transparent_background);

        for window_def in &mut self.layout.windows {
            window_def.base_mut().transparent_background = enable;
        }

        let status = if enable { "enabled" } else { "disabled" };
        self.add_system_message(&format!(
            "Background transparency {} for all windows",
            status
        ));
        self.needs_render = true;
    }
}
