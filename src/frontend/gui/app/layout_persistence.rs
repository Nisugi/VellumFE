//! Layout snapshot save/load: assembling the persistable snapshot
//! (autosave vs named checkpoint), applying one back onto the running
//! shell, and the survivor/arrangement helpers that decide which window
//! rects and defs a snapshot keeps.

use super::*;

impl VellumGuiApp {
    /// Assemble the persistable layout snapshot. Returns None when the dock
    /// snapshot fails to serialize (never persist a null layout).
    ///
    /// One format, two masters: the per-character AUTOSAVE slot needs hidden
    /// windows (their rects/hidden state must survive a restart so unhide
    /// restores placement), while a named CHECKPOINT (`.savelayout <name>`) is
    /// an exact portable copy of what's on screen — shown windows only, no
    /// hidden residue carried to other profiles. `mode` picks the behavior.
    pub(super) fn build_layout_snapshot(
        &mut self,
        mode: LayoutSaveMode,
    ) -> Option<GuiLayoutFileV1> {
        let mut layout = GuiLayoutFileV1::new(&self.layout_profile, &self.layout_character);

        let strip_hidden = mode == LayoutSaveMode::Checkpoint;
        // The tabs this save describes. Checkpoints drop GUI-hidden tabs so
        // nothing below (defs, rects, zones, settings, groups) mentions them.
        let snapshot_tabs: HashMap<TabKey, GuiTab> = self
            .available_tabs
            .iter()
            .filter(|(key, _)| !(strip_hidden && self.hidden_tabs.contains(*key)))
            .map(|(key, tab)| (key.clone(), tab.clone()))
            .collect();

        layout.hidden_tabs = if strip_hidden {
            Vec::new()
        } else {
            let mut hidden_tabs: Vec<TabKey> = self.hidden_tabs.iter().cloned().collect();
            hidden_tabs.sort_by_key(|key| key.short_id());
            hidden_tabs
        };
        layout.ui_font = self.ui_font.clone();
        layout.ui_settings = self.ui_settings.clone();
        // The theme rides with the layout (like the skin), so a checkpoint
        // loaded on another profile reproduces the saver's look. The live
        // source of truth is config.active_theme; stamp it at save time.
        layout.ui_settings.active_theme = Some(self.app_core.config.active_theme.clone());
        layout.tab_settings = {
            let mut entries: Vec<TabSettingsEntry> = self
                .tab_settings
                .iter()
                .filter(|(key, _)| !strip_hidden || snapshot_tabs.contains_key(*key))
                .map(|(key, settings)| TabSettingsEntry {
                    key: key.clone(),
                    settings: settings.clone(),
                })
                .collect();
            entries.sort_by_key(|entry| entry.key.short_id());
            entries
        };

        let snapshot = DockStateSnapshot {
            visible_tabs: self.current_main_surface_tab_keys(),
            main_window_rects: {
                let mut rects: Vec<MainWindowRectSnapshot> = self
                    .main_window_rects
                    .iter()
                    .filter(|(key, _)| snapshot_tabs.contains_key(*key))
                    .map(|(key, rect)| MainWindowRectSnapshot {
                        key: key.clone(),
                        rect: *rect,
                        gap_above: self
                            .sidebar_gap_above
                            .get(key)
                            .copied()
                            .filter(|value| value.is_finite() && *value > 0.0)
                            .unwrap_or(0.0),
                        anchors: self
                            .window_anchors
                            .get(key)
                            .filter(|anchors| !anchors.is_free())
                            .cloned(),
                        size_role: self
                            .window_size_roles
                            .get(key)
                            .copied()
                            .filter(|role| *role == dock::SizeRole::Fixed),
                    })
                    .collect();
                rects.sort_by_key(|entry| entry.key.short_id());
                rects
            },
            tab_zones: {
                let mut zones: Vec<TabZoneSnapshot> = self
                    .tab_zones
                    .iter()
                    .filter(|(key, _)| snapshot_tabs.contains_key(*key))
                    .map(|(key, zone)| TabZoneSnapshot {
                        key: key.clone(),
                        zone: *zone,
                    })
                    .collect();
                zones.sort_by_key(|entry| entry.key.short_id());
                zones
            },
            no_title_tabs: {
                let mut keys: Vec<TabKey> = self
                    .no_title_tabs
                    .iter()
                    .filter(|key| snapshot_tabs.contains_key(*key))
                    .cloned()
                    .collect();
                keys.sort_by_key(|key| key.short_id());
                keys
            },
            shell_layout: self.shell_layout.clone(),
            tab_groups: Self::sanitize_tab_groups(self.tab_groups.clone(), &snapshot_tabs),
            // Stable order: GuiShellZone::all() filtered, not HashSet order.
            free_sidebar_zones: GuiShellZone::all()
                .into_iter()
                .filter(|zone| self.migrated_sidebar_zones.contains(zone))
                .collect(),
            pending_zones: {
                // Zone prefs for windows that aren't live tabs. Checkpoints
                // drop them — they describe hidden/never-shown windows, which
                // an exact copy of the visible arrangement doesn't carry.
                let mut entries: Vec<PendingZoneSnapshot> = if strip_hidden {
                    Vec::new()
                } else {
                    self.pending_zones
                        .iter()
                        .map(|(window, zone)| PendingZoneSnapshot {
                            window: window.clone(),
                            zone: *zone,
                        })
                        .collect()
                };
                entries.sort_by(|a, b| a.window.cmp(&b.window));
                entries
            },
        };
        layout.dock_state_json = match serde_json::to_value(snapshot) {
            Ok(value) => value,
            Err(err) => {
                // Persisting a null snapshot would wipe the saved window layout;
                // keep the existing file instead.
                tracing::error!(
                    "Failed to serialize GUI dock layout; skipping save: {}",
                    err
                );
                return None;
            }
        };
        layout.detached_viewports = self
            .detached_tabs
            .iter()
            .map(|(key, state)| (key.short_id(), state.current.clone()))
            .collect();
        layout.main_viewport = self.main_viewport_state.clone();
        // Carry the window definitions for the windows that actually take part
        // in THIS arrangement, so a named layout loaded into a profile that
        // lacks them (a fresh character) can recreate exactly those windows.
        // The dock snapshot alone only references windows by TabKey. We
        // deliberately do NOT bake the character's entire window universe:
        // doing so injected every unrelated window (voln, society, …) into any
        // profile the layout was loaded into. The arrangement's members are the
        // windows backing the live tabs.
        layout.window_defs =
            Self::arrangement_window_defs(&self.app_core.layout.windows, &snapshot_tabs);
        layout.touch();
        Some(layout)
    }

    /// Record the main OS window's current geometry. Not marked layout-dirty:
    /// it rides along with the next save (including the on-exit flush), so
    /// pure moves/resizes of the OS window don't churn the writer thread.
    pub(super) fn capture_main_viewport(&mut self, ctx: &egui::Context) {
        let (inner_rect, outer_rect, maximized) = ctx.input(|i| {
            let viewport = i.viewport();
            (
                viewport.inner_rect,
                viewport.outer_rect,
                viewport.maximized.unwrap_or(false),
            )
        });
        let Some(inner_rect) = inner_rect else {
            return;
        };
        if !inner_rect.is_finite() || inner_rect.width() < 1.0 || inner_rect.height() < 1.0 {
            return;
        }
        // The ACTUAL canvas the rects are being laid out against right now —
        // recorded in both branches so a maximized save rescales from the
        // maximized canvas, not the smaller un-maximized restore size.
        let canvas = Some([inner_rect.width(), inner_rect.height()]);
        if maximized {
            // Keep the last un-maximized geometry as the restore size.
            match &mut self.main_viewport_state {
                Some(state) => {
                    state.maximized = true;
                    state.canvas_size = canvas;
                }
                None => {
                    self.main_viewport_state = Some(MainViewportState {
                        outer_pos: None,
                        inner_size: [inner_rect.width(), inner_rect.height()],
                        maximized: true,
                        canvas_size: canvas,
                    });
                }
            }
        } else {
            self.main_viewport_state = Some(MainViewportState {
                outer_pos: outer_rect
                    .filter(|rect| rect.is_finite())
                    .map(|rect| [rect.min.x, rect.min.y]),
                inner_size: [inner_rect.width(), inner_rect.height()],
                maximized: false,
                canvas_size: canvas,
            });
        }
    }

    /// Persist the layout. Serialization happens here on the UI thread (it is
    /// cheap once debounced); the disk I/O (backup copy + temp write + rename)
    /// runs on the writer thread. Falls back to a synchronous write when the
    /// worker is gone (shutdown path).
    pub(super) fn save_layout_state(&mut self) {
        let Some(layout) = self.build_layout_snapshot(LayoutSaveMode::Autosave) else {
            return;
        };
        match &self.layout_save_tx {
            Some(tx) => {
                if let Err(send_error) = tx.send(layout) {
                    Self::write_layout_now(
                        &send_error.0,
                        &self.layout_profile,
                        &self.layout_character,
                    );
                }
            }
            None => Self::write_layout_now(&layout, &self.layout_profile, &self.layout_character),
        }
    }

    pub(super) fn write_layout_now(layout: &GuiLayoutFileV1, profile: &str, character: &str) {
        if let Err(err) = save_layout(layout, profile, character) {
            tracing::warn!("Failed to save GUI layout: {}", err);
        }
    }

    /// Apply a saved layout snapshot to the live app — the runtime half of
    /// `.loadlayout`. Reuses the constructor's reconciliation, so tabs the
    /// file doesn't know keep working and saved tabs missing this session
    /// are dropped.
    /// `keep_skin` (from `.loadlayout <name> --keep-skin`) preserves the
    /// loader's appearance cluster (skin, theme, doll/status/compass art,
    /// default frame/background) and takes only the arrangement.
    pub(super) fn apply_layout_snapshot(&mut self, layout: &GuiLayoutFileV1, keep_skin: bool) {
        // Make this profile's window set match the layout's BEFORE
        // reconciling: (1) recreate any window the file carries but this
        // profile lacks — restore_layout_state filters arrangement against
        // available_tabs, so a window that doesn't exist yet would have its
        // rect/zone/group dropped; (2) core-hide any live window the file
        // does NOT name (layout is authoritative — loading Character A's
        // layout onto B must not leave B's unrelated windows on screen).
        // Hides go through core visibility (hide = the Windows-window
        // uncheck), and the main story window / command input are never
        // hidden (tabs_absent_from_layout excludes them). Guarded on
        // non-empty window_defs — a legacy file can't describe its
        // arrangement, so we leave the current windows alone rather than
        // blanking the screen.
        if !layout.window_defs.is_empty() {
            let (w, h) = self.core_layout_size;
            let created = self
                .app_core
                .materialize_missing_windows(&layout.window_defs, w, h);
            if !created.is_empty() {
                tracing::info!(
                    "loadlayout: created {} missing window(s): {}",
                    created.len(),
                    created.join(", ")
                );
            }
            // Rebuild the tab list so the freshly-created windows are visible
            // to the extras scan below (fingerprint would otherwise skip the
            // refresh mid-frame).
            self.available_tabs_fingerprint = None;
            self.refresh_available_tabs_if_needed();
            for key in Self::tabs_absent_from_layout(&layout.window_defs, &self.available_tabs) {
                self.core_hide_tab(&key);
            }
            // And rebuild again so the reconcile below sees the exact final
            // window set (extras gone, layout's windows present).
            self.available_tabs_fingerprint = None;
            self.refresh_available_tabs_if_needed();
        }

        let restored = Self::restore_layout_state(Some(layout), &self.available_tabs);
        tracing::info!(
            "Applying GUI layout snapshot: {} window rects, {} zone assignments",
            restored.main_window_rects.len(),
            restored.tab_zones.len()
        );
        self.hidden_tabs = restored.hidden_tabs;
        self.main_window_rects = restored.main_window_rects;
        self.window_anchors = restored.window_anchors;
        self.sidebar_gap_above = restored.sidebar_gap_above;
        self.migrated_sidebar_zones = restored.migrated_sidebar_zones;
        self.last_center_window_rects.clear();
        self.zone_snap_drag = None;
        self.zone_snap_guides.clear();
        self.tab_zones = restored.tab_zones;
        self.pending_zones = restored.pending_zones;
        self.no_title_tabs = restored.no_title_tabs;
        self.shell_layout = restored.shell_layout;
        self.tab_groups = restored.tab_groups;
        self.detached_tabs = restored.detached_tabs;
        self.ui_font = restored.ui_font;
        // Appearance riding with the checkpoint: exact copy by default — the
        // saved skin/theme/art selections stand, INCLUDING a recorded
        // no-skin (the target's skin is cleared to match the saver's look).
        // `--keep-skin` opts out: the loader's whole appearance cluster
        // (skin, theme, doll, status art, compass set, default frame/bg)
        // survives and only the arrangement is taken from the file.
        let previous_look = self.ui_settings.clone();
        self.ui_settings = restored.ui_settings;
        if keep_skin {
            self.ui_settings.active_skin = previous_look.active_skin.clone();
            self.ui_settings.active_theme = previous_look.active_theme.clone();
            self.ui_settings.doll_image = previous_look.doll_image.clone();
            self.ui_settings.status_icons = previous_look.status_icons.clone();
            self.ui_settings.compass_set = previous_look.compass_set.clone();
            self.ui_settings.default_frame = previous_look.default_frame.clone();
            self.ui_settings.default_background = previous_look.default_background.clone();
        }
        if self.app_core.config.active_skin != self.ui_settings.active_skin {
            self.app_core.config.active_skin = self.ui_settings.active_skin.clone();
            self.save_config_after_skin_change();
        }
        // Theme: config.active_theme is the live source of truth (the frame
        // loop's apply_theme_if_changed watches it). A recorded theme mirrors
        // in; None (legacy file) keeps the current theme. A custom theme the
        // target profile lacks is reported by apply_theme_if_changed's warn
        // path and the current visuals stay.
        if !keep_skin {
            if let Some(theme) = self.ui_settings.active_theme.clone() {
                if self.app_core.config.active_theme != theme {
                    self.app_core.config.active_theme = theme;
                    self.save_config_after_skin_change();
                }
            }
        }
        self.tab_settings = restored.tab_settings;
        // Checkpoints can predate the move of per-window text size/font/wrap
        // onto the layout defs; migrate them the same way startup does.
        let available_tabs = &self.available_tabs;
        let (migrated_layout, _) = Self::migrate_tab_settings_to_layout(
            &mut self.tab_settings,
            &mut self.app_core.layout,
            |key| available_tabs.get(key).map(|tab| tab.window_name.clone()),
        );
        if migrated_layout {
            self.app_core.schedule_layout_autosave();
        }
        // Lazy appliers pick up the new font/zoom/density next frame.
        self.fonts_applied = false;
        self.zoom_applied = false;
        self.applied_title_font_size = None;
        self.applied_density = None;
        self.applied_window_corner_radius = None;
        // Rects load in absolute points against the save-time canvas: anchor
        // the store there and let the next frame's rescale map them onto the
        // live content size. `from` is the saved canvas; without a recorded
        // viewport (legacy checkpoints) fall back to the bounding box of the
        // saved rects so we still have a reference.
        self.canonical_canvas = Some(Self::layout_reference_canvas(
            layout,
            &self.main_window_rects,
        ));
        // Restore the saved OS-window geometry too, so "exact position on
        // screen" means exactly that. No settle-wait: the anchor rescale
        // tracks every intermediate size while the OS window resizes and
        // lands 1:1 when it reaches the saved canvas (or maps proportionally
        // into whatever size the OS allowed).
        self.pending_viewport_restore = layout.main_viewport.clone();
        // Replay the saved stacking order next frame (windows must exist as
        // layers first). visible_tabs is recorded back-to-front; filter to
        // tabs that exist this session so a cross-character load doesn't try
        // to raise an absent window.
        self.pending_zorder = Self::dock_snapshot_from_layout(layout).map(|snapshot| {
            snapshot
                .visible_tabs
                .into_iter()
                .filter(|key| self.available_tabs.contains_key(key))
                .collect::<Vec<_>>()
        });
        // The live autosave slot now reflects the loaded arrangement; the
        // checkpoint itself is only written by an explicit .savelayout.
        self.layout_dirty = true;
    }

    /// Tab keys whose stored rect should survive an available-tabs refresh.
    /// A key survives if it is still a live tab, OR if it merely went HIDDEN
    /// (its window, resolved via the pre-refresh tab list, is still present in
    /// the layout defs). A DELETED window is gone from the layout defs, so its
    /// rect is not spared here — and the delete path purges it explicitly via
    /// forget_tab_state anyway. This keeps a Windows-menu untick/retick from
    /// dropping a window to the top-left default.
    pub(super) fn rect_survivor_keys(
        previous_tabs: &HashMap<TabKey, GuiTab>,
        current_tabs: &HashMap<TabKey, GuiTab>,
        layout_windows: &[crate::config::WindowDef],
    ) -> HashSet<TabKey> {
        let layout_def_names: HashSet<&str> = layout_windows.iter().map(|def| def.name()).collect();
        previous_tabs
            .iter()
            .filter(|(key, tab)| {
                current_tabs.contains_key(key)
                    || layout_def_names.contains(tab.window_name.as_str())
            })
            .map(|(key, _)| key.clone())
            .chain(current_tabs.keys().cloned())
            .collect()
    }

    /// The window definitions that take part in a given arrangement: the
    /// subset of the character's window universe whose windows back a live tab.
    /// `.savelayout` persists only these (not every WindowDef the character
    /// owns) so loading the layout into another profile recreates exactly the
    /// arrangement's windows rather than injecting every unrelated window
    /// (voln, society, …).
    pub(super) fn arrangement_window_defs(
        all_windows: &[crate::config::WindowDef],
        available_tabs: &HashMap<TabKey, GuiTab>,
    ) -> Vec<crate::config::WindowDef> {
        let arrangement: HashSet<&str> = available_tabs
            .values()
            .map(|tab| tab.window_name.as_str())
            .collect();
        all_windows
            .iter()
            .filter(|def| arrangement.contains(def.name()))
            .cloned()
            .collect()
    }

    /// The live tabs a loaded layout does NOT name — the windows to hide so the
    /// layout is authoritative (its window_defs define the complete visible
    /// set). The main story window and the command input are always excluded:
    /// hiding them would break the main-stream-visible and always-typing
    /// invariants. Callers must guard on a non-empty `window_defs`; an empty
    /// list means a legacy file that can't describe its arrangement, and
    /// hiding against it would blank the screen.
    pub(super) fn tabs_absent_from_layout(
        window_defs: &[crate::config::WindowDef],
        available_tabs: &HashMap<TabKey, GuiTab>,
    ) -> Vec<TabKey> {
        let named: HashSet<&str> = window_defs.iter().map(|def| def.name()).collect();
        available_tabs
            .iter()
            .filter(|(key, _)| **key != TabKey::TextMain && **key != TabKey::CommandInput)
            .filter(|(_, tab)| !named.contains(tab.window_name.as_str()))
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// The canvas size a saved layout's rects were captured against, used as
    /// the "from" size when rescaling to the current window. Prefers the
    /// recorded main-viewport inner size; falls back to the bounding box of
    /// the saved rects (legacy checkpoints predate the viewport record).
    /// Returns a 1x1 sentinel when neither is usable, which `rescale_rect`
    /// treats as an identity (no scaling).
    pub(super) fn layout_reference_canvas(
        layout: &GuiLayoutFileV1,
        rects: &HashMap<TabKey, [f32; 4]>,
    ) -> egui::Vec2 {
        if let Some(viewport) = &layout.main_viewport {
            // canvas_size is the ACTUAL inner size at capture (correct even
            // for a maximized save); inner_size is the un-maximized restore
            // geometry kept for older files.
            let [w, h] = viewport.canvas_size.unwrap_or(viewport.inner_size);
            if w.is_finite() && h.is_finite() && w > 1.0 && h > 1.0 {
                return egui::Vec2::new(w, h);
            }
        }
        Self::rects_bounding_canvas(rects)
    }

    /// Bounding box of a rect set (max right / max bottom edge), used as a
    /// reference canvas: by legacy layout files with no recorded viewport,
    /// and by bare `.resize`, whose fill semantics come exactly from
    /// anchoring to the box the rects occupy rather than the canvas.
    pub(super) fn rects_bounding_canvas(rects: &HashMap<TabKey, [f32; 4]>) -> egui::Vec2 {
        let mut max_x = 0.0_f32;
        let mut max_y = 0.0_f32;
        for rect in rects.values() {
            if rect.iter().all(|value| value.is_finite()) {
                max_x = max_x.max(rect[0] + rect[2]);
                max_y = max_y.max(rect[1] + rect[3]);
            }
        }
        egui::Vec2::new(max_x.max(1.0), max_y.max(1.0))
    }

    /// egui internals section for `.performance dump`: texture allocator
    /// state, visible areas, and scale factors — the numbers that explain
    /// GPU-side memory and DPI questions a core dump can't answer.
    pub(super) fn egui_internals_report(&self) -> Option<String> {
        let ctx = self.repaint_ctx.lock().ok()?.as_ref()?.clone();
        let (tex_count, tex_bytes) = {
            let tex_manager = ctx.tex_manager();
            let tex = tex_manager.read();
            let bytes: usize = tex.allocated().map(|(_, meta)| meta.bytes_used()).sum();
            (tex.num_allocated(), bytes)
        };
        let visible_areas = ctx.memory(|m| m.areas().visible_layer_ids().len());
        Some(format!(
            "== egui internals ==\n\
             textures      {} allocated ({:.1} MB)\n\
             visible areas {}\n\
             pixels/point  {:.2}\n\
             zoom factor   {:.2}\n",
            tex_count,
            tex_bytes as f64 / (1024.0 * 1024.0),
            visible_areas,
            ctx.pixels_per_point(),
            ctx.zoom_factor()
        ))
    }

    pub(super) fn list_layout_checkpoints(&mut self) {
        let names = list_named_layouts();
        if names.is_empty() {
            self.app_core.add_system_message(
                "No saved GUI layouts. Save the current arrangement with .savelayout <name>",
            );
        } else {
            self.app_core
                .add_system_message(&format!("Saved GUI layouts: {}", names.join(", ")));
        }
    }
}
