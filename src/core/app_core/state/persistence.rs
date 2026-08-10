//! Layout and config persistence: .savelayout, the debounced autosave,
//! save-on-quit, and the reload_* family for every config domain.

use super::*;

impl AppCore {
    /// Save current layout
    pub fn save_layout(&mut self, name: &str, terminal_width: u16, terminal_height: u16) {
        tracing::info!("========== SAVE LAYOUT: '{}' START ==========", name);
        tracing::info!(
            "Current terminal size: {}x{}",
            terminal_width,
            terminal_height
        );
        tracing::info!("Layout has {} windows defined", self.layout.windows.len());
        tracing::info!(
            "UI state has {} windows rendered",
            self.ui_state.windows.len()
        );

        // IMPORTANT: Capture actual window positions from UI state before saving
        // (user may have moved/resized windows with mouse)
        for window_def in &mut self.layout.windows {
            let window_name = window_def.name().to_string();
            let base = window_def.base();

            tracing::debug!(
                "Window '{}' BEFORE capture: pos=({},{}) size={}x{}",
                window_name,
                base.col.get(),
                base.row.get(),
                base.cols.get(),
                base.rows.get()
            );

            if let Some(window_state) = self.ui_state.windows.get(&window_name) {
                let ui_pos = &window_state.position;
                tracing::info!(
                    "Window '{}' - Capturing from UI state: pos=({},{}) size={}x{}",
                    window_name,
                    ui_pos.x.get(),
                    ui_pos.y.get(),
                    ui_pos.width.get(),
                    ui_pos.height.get()
                );

                // Clamp window position and size to terminal boundaries before saving
                let clamped_x = ui_pos.x.get().min(terminal_width.saturating_sub(1));
                let clamped_y = ui_pos.y.get().min(terminal_height.saturating_sub(1));

                // Ensure width doesn't exceed available space
                // Use window's min_cols constraint (default 1) instead of hardcoded 10
                let max_width = terminal_width.saturating_sub(clamped_x);
                let min_width = base.min_cols.unwrap_or(1);
                let clamped_width = ui_pos.width.get().min(max_width).max(min_width);

                // Ensure height doesn't exceed available space
                // Use window's min_rows constraint (default 1)
                let max_height = terminal_height.saturating_sub(clamped_y);
                let min_height = base.min_rows.unwrap_or(1);
                let clamped_height = ui_pos.height.get().min(max_height).max(min_height);

                if clamped_x != ui_pos.x.get()
                    || clamped_y != ui_pos.y.get()
                    || clamped_width != ui_pos.width.get()
                    || clamped_height != ui_pos.height.get()
                {
                    tracing::warn!(
                        "Window '{}' clamped: ({},{} {}x{}) -> ({},{} {}x{}) to fit terminal {}x{}",
                        window_name,
                        ui_pos.x.get(),
                        ui_pos.y.get(),
                        ui_pos.width.get(),
                        ui_pos.height.get(),
                        clamped_x,
                        clamped_y,
                        clamped_width,
                        clamped_height,
                        terminal_width,
                        terminal_height
                    );
                }

                let base = window_def.base_mut();
                base.row = crate::data::geometry::Row::new(clamped_y);
                base.col = crate::data::geometry::Col::new(clamped_x);
                base.rows = crate::data::geometry::Height::new(clamped_height);
                base.cols = crate::data::geometry::Width::new(clamped_width);

                tracing::debug!(
                    "Window '{}' AFTER capture: pos=({},{}) size={}x{}",
                    window_name,
                    base.col.get(),
                    base.row.get(),
                    base.cols.get(),
                    base.rows.get()
                );
            } else {
                tracing::warn!(
                    "Window '{}' is in layout but NOT in ui_state! Cannot capture position.",
                    window_name
                );
            }
        }

        let layout_path = match Config::layout_path(name) {
            Ok(path) => path,
            Err(e) => {
                tracing::error!("Failed to get layout path for '{}': {}", name, e);
                self.add_system_message(&format!("Failed to get layout path: {}", e));
                return;
            }
        };

        tracing::info!("Saving layout to: {}", layout_path.display());

        // Pass actual terminal size with force=true so it always updates to current terminal size
        self.layout.theme = Some(self.config.active_theme.clone());
        match self
            .layout
            .save(name, Some((terminal_width, terminal_height)), true)
        {
            Ok(_) => {
                tracing::info!(
                    "Layout '{}' saved successfully to {}",
                    name,
                    layout_path.display()
                );
                tracing::info!("========== SAVE LAYOUT: '{}' SUCCESS ==========", name);
                self.add_system_message(&format!("Layout saved as '{}'", name));
                // Clear modified flag and update base layout name
                self.layout_modified_since_save = false;
                self.base_layout_name = Some(name.to_string());
                // Mirror the just-saved arrangement into the auto-save slot
                // startup reads, so the save sticks even if this session
                // ends without a clean quit.
                self.autosave_layout();
            }
            Err(e) => {
                tracing::error!("Failed to save layout '{}': {}", name, e);
                tracing::info!("========== SAVE LAYOUT: '{}' FAILED ==========", name);
                self.add_system_message(&format!("Failed to save layout: {}", e));
            }
        }
    }

    /// Load a saved layout and update window positions/configs
    ///
    /// Loads layout at exact positions specified in file.
    /// Use .resize command for delta-based proportional scaling after loading.

    /// Resize all windows proportionally based on current terminal size (VellumFE algorithm)
    ///
    /// This command resets to the baseline layout and applies delta-based proportional distribution.
    /// This is the ONLY place (besides initial load) that should perform scaling operations.

    /// Helper to get minimum widget size based on widget type (from VellumFE)


    /// Apply proportional height resize (from VellumFE apply_height_resize)
    /// Adapted for WindowDef enum structure

    /// Apply proportional width resize (from VellumFE apply_width_resize)
    /// Adapted for WindowDef enum structure
    /// baseline_rows: Vec of (name, baseline_row, baseline_rows) for grouping windows by original row

    /// Sync layout WindowDefs to ui_state WindowStates without destroying content
    ///
    /// Uses exact positions from layout file.
    /// Use .resize command for delta-based proportional scaling.

    /// Load a saved layout with terminal size for immediate reinitialization

    /// List all saved layouts

    /// Resize layout using delta-based proportional distribution
    /// This method is called by the .resize command and requires manual invocation

    /// Wrapper for resize command - gets terminal size from layout

    /// List all windows
    pub(in crate::core::app_core) fn list_windows(&mut self) {
        let window_count = self.ui_state.windows.len();

        // Collect window info first to avoid borrow checker issues
        let mut window_info = Vec::new();
        for (name, window) in &self.ui_state.windows {
            let pos = &window.position;
            let visible = if window.visible { "visible" } else { "hidden" };
            window_info.push(format!(
                "  {} - {}x{} at ({},{}) - {} - {}",
                name,
                pos.width.get(),
                pos.height.get(),
                pos.x.get(),
                pos.y.get(),
                visible,
                format!("{:?}", window.widget_type)
            ));
        }

        // Now add all messages
        self.add_system_message(&format!("=== Windows ({}) ===", window_count));
        for info in window_info {
            self.add_system_message(&info);
        }
    }

    /// How long the layout must be stable before the debounced autosave fires.
    pub const LAYOUT_AUTOSAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(3);

    /// Mark the layout changed and (re)arm the debounced autosave. Every
    /// mutation site routes through this or sets the flag via it, so window
    /// moves/resizes/edits persist a few seconds later instead of only on a
    /// clean quit.
    pub fn schedule_layout_autosave(&mut self) {
        self.layout_modified_since_save = true;
        self.layout_autosave_pending = Some(std::time::Instant::now());
    }

    /// Debounce driver: frontends call this from their event loop. Writes the
    /// profile auto-save slot once the layout has been stable for
    /// LAYOUT_AUTOSAVE_DEBOUNCE.
    pub fn tick_layout_autosave(&mut self) {
        // Discovery-memory flush rides the same driver (no debounce —
        // registrations are rare, one-shot, and tiny).
        if std::mem::take(&mut self.window_registry_dirty) {
            if let Err(e) = Config::save_window_registry(
                self.config.character.as_deref(),
                &self.window_registry,
            ) {
                tracing::warn!("could not write window_registry.toml: {e:#}");
            }
        }
        // Persist character state when it actually changed (rare — the parser
        // only bumps the generation on a real value change). No debounce: the
        // write is tiny and infrequent, same as the registry above.
        if self.game_state.character.generation != self.character_state_saved_gen {
            self.persist_character_state();
        }
        if let Some(changed_at) = self.layout_autosave_pending {
            if changed_at.elapsed() >= Self::LAYOUT_AUTOSAVE_DEBOUNCE {
                self.autosave_layout();
            }
        }
    }

    /// Mark layout as modified and show reminder (once per session)
    pub fn mark_layout_modified(&mut self) {
        self.schedule_layout_autosave();

        // Show reminder once per session
        if !self.save_reminder_shown {
            self.add_system_message(
                "Tip: Use .savelayout <name> to preserve changes as a reusable template",
            );
            self.save_reminder_shown = true;
        }
    }

    /// Adjust window rows for content-driven widgets (like Betrayer)
    /// Called after message processing when content count may have changed
    pub fn adjust_content_driven_windows(&mut self) {
        // Collect changes first to avoid borrow issues
        let mut changes: Vec<(String, u16)> = Vec::new();

        for window_def in &self.layout.windows {
            if let crate::config::WindowDef::Betrayer { base, data } = window_def {
                let bar_rows = 1u16;
                let item_rows = if data.show_items {
                    self.game_state.betrayer.items.len().max(1) as u16
                } else {
                    0
                };
                let border_rows = base.horizontal_border_units();
                let ideal_rows = bar_rows + item_rows + border_rows;

                // Clamp to min/max
                let new_rows = ideal_rows
                    .max(base.min_rows.unwrap_or(1))
                    .min(base.max_rows.unwrap_or(u16::MAX));

                if base.rows.get() != new_rows {
                    changes.push((base.name.clone(), new_rows));
                }
            }
        }

        // Apply changes to both layout and ui_state
        for (name, new_rows) in changes {
            // Update layout
            for window_def in &mut self.layout.windows {
                if window_def.name() == name {
                    if let crate::config::WindowDef::Betrayer { base, .. } = window_def {
                        base.rows = crate::data::geometry::Height::new(new_rows);
                    }
                    break;
                }
            }

            // Update ui_state window position height
            if let Some(window) = self.ui_state.windows.get_mut(&name) {
                window.position.height = crate::data::geometry::Height::new(new_rows);
            }

            // Mark modified but don't show the save reminder for auto-resizes
            self.schedule_layout_autosave();
            self.needs_render = true;
        }
    }

    /// Write the current layout to the profile auto-save slot
    /// (~/.vellum-fe/profiles/{character}/layout.toml) — the file startup
    /// reads. Called on quit, after .savelayout/.loadlayout, and by the
    /// debounced tick_layout_autosave, so layout changes stick even if the
    /// session ends without reaching the quit path (console X, crash).
    pub fn autosave_layout(&mut self) {
        self.layout_autosave_pending = None;
        let profile = self
            .config
            .character
            .clone()
            .unwrap_or_else(|| "default".to_string());

        let terminal_size = self
            .layout
            .terminal_width
            .and_then(|w| self.layout.terminal_height.map(|h| (w, h)));

        let base_layout_name = self
            .base_layout_name
            .clone()
            .or_else(|| self.layout.base_layout.clone())
            .unwrap_or_else(|| "default".to_string());

        self.layout.theme = Some(self.config.active_theme.clone());
        if let Err(e) = self
            .layout
            .save_auto(&profile, &base_layout_name, terminal_size)
        {
            tracing::warn!("Failed to autosave layout: {}", e);
        } else {
            tracing::info!(
                "Layout autosaved to profile '{}' (base: {}, terminal: {:?})",
                profile,
                base_layout_name,
                terminal_size
            );
        }
    }

    /// Save settings (layout, session cache) without exiting.
    /// Called by quit() and when intercepting game "quit" command.
    pub fn save_on_quit(&mut self) {
        // Show reminder if layout was modified
        if self.layout_modified_since_save {
            self.add_system_message(
                "Layout modified - use .savelayout <name> to create reusable template",
            );
        }

        // Autosave to profile layout.toml ("default" profile when no character is set)
        self.autosave_layout();

        let allowed_ids = self.allowed_quickbar_ids();
        let quickbars: HashMap<String, QuickbarData> = self
            .ui_state
            .quickbars
            .iter()
            .filter(|(id, _)| allowed_ids.contains(*id))
            .map(|(id, data)| (id.clone(), data.clone()))
            .collect();
        let quickbar_order: Vec<String> = self
            .ui_state
            .quickbar_order
            .iter()
            .filter(|id| allowed_ids.contains(*id))
            .cloned()
            .collect();
        let active_quickbar_id = self
            .ui_state
            .active_quickbar_id
            .as_ref()
            .and_then(|id| if allowed_ids.contains(id) { Some(id.clone()) } else { None });

        let character = (self.game_state.character != Default::default())
            .then(|| self.game_state.character.clone());
        let cache = crate::session_cache::SessionCache {
            quickbars,
            quickbar_order,
            active_quickbar_id,
            character,
        };
        if let Err(err) = crate::session_cache::save(self.config.character.as_deref(), &cache) {
            tracing::warn!("Failed to save session cache: {}", err);
        }
    }

    /// Persist just the character state into the session cache, preserving the
    /// rest (quickbars). Called by the autosave tick when the state changed.
    pub(super) fn persist_character_state(&mut self) {
        let character = self.config.character.as_deref();
        // Merge into the existing cache so we don't clobber quickbars.
        let mut cache = crate::session_cache::load(character).unwrap_or_default();
        cache.character = (self.game_state.character != Default::default())
            .then(|| self.game_state.character.clone());
        match crate::session_cache::save(character, &cache) {
            Ok(()) => self.character_state_saved_gen = self.game_state.character.generation,
            Err(e) => tracing::warn!("could not persist character state: {e:#}"),
        }
    }

    /// Quit the application
    pub fn quit(&mut self) {
        self.save_on_quit();
        self.running = false;
    }

    /// Save configuration to disk
    pub fn save_config(&mut self) -> Result<()> {
        self.config.save(self.config.character.as_deref())?;
        // Update squelch patterns after config save (in case highlights changed)
        self.message_processor.update_squelch_patterns();
        // Update redirect cache after config save (in case highlights changed)
        self.message_processor.update_redirect_cache();
        Ok(())
    }

    // ===========================================================================================
    // Config Reload Methods
    // ===========================================================================================

    /// Reload all configuration from disk
    pub fn reload_all(&mut self) {
        self.add_system_message("Reloading all configuration...");
        self.reload_highlights();
        self.reload_keybinds();
        self.reload_hotbars();
        self.reload_settings();
        self.reload_colors();
        self.reload_layout();
        let emoji_count = crate::core::custom_emoji::reload();
        let image_count = crate::core::inline_image::reload();
        self.add_system_message(&format!(
            "All configuration reloaded ({emoji_count} custom emoji, \
             {image_count} inline images)"
        ));
    }

    /// Reload highlights from disk
    pub fn reload_highlights(&mut self) {
        tracing::debug!("reload_highlights: start");
        match crate::config::Config::load_highlights(self.config.character.as_deref()) {
            Ok(highlights) => {
                self.config.highlights = highlights;
                crate::config::Config::compile_highlight_patterns(&mut self.config.highlights);
                self.message_processor.apply_config(self.config.clone());
                tracing::debug!("reload_highlights: apply_config done");
                self.add_system_message("Highlights reloaded");
                tracing::debug!("reload_highlights: system message queued");
                let has_perception_window = self.ui_state.windows.values().any(|window| {
                    matches!(window.content, crate::data::WindowContent::Perception(_))
                });
                let is_dr_game = self
                    .config
                    .connection
                    .game
                    .as_deref()
                    .map(|game| game.to_ascii_lowercase().starts_with("dr"))
                    .unwrap_or(false);
                if has_perception_window && is_dr_game {
                    tracing::debug!("reload_highlights: before reload_spell_abbrevs");
                    match crate::spell_abbrevs::reload_spell_abbrevs() {
                        Ok(()) => {
                            self.add_system_message("Spell abbreviations reloaded");
                            tracing::debug!("reload_highlights: spell abbrevs reloaded");
                        }
                        Err(e) => self.add_system_message(&format!(
                            "Failed to reload spell abbreviations: {}",
                            e
                        )),
                    }
                    tracing::debug!("reload_highlights: after reload_spell_abbrevs");
                } else {
                    tracing::debug!(
                        "reload_highlights: skipping spell abbrevs (perception_window={}, dr_game={})",
                        has_perception_window,
                        is_dr_game
                    );
                }
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload highlights: {}", e));
            }
        }
        tracing::debug!("reload_highlights: end");
    }

    /// Reload keybinds from disk
    pub fn reload_keybinds(&mut self) {
        match crate::config::Config::load_keybinds(self.config.character.as_deref()) {
            Ok(keybinds) => {
                self.config.keybinds = keybinds;
                let character = self.config.character.clone();
                let character = character.as_deref();
                crate::config::Config::migrate_controller_shift_layers(character);
                self.config.controller_binds =
                    crate::config::Config::load_controller_binds(character).unwrap_or_default();
                self.config.controller_wheel =
                    crate::config::Config::load_controller_wheel(character).unwrap_or_default();
                self.config.controller_wheels =
                    crate::config::Config::load_controller_wheels(character).unwrap_or_default();
                self.config.controller_wheels_meta =
                    crate::config::Config::load_controller_wheels_meta(character)
                        .unwrap_or_default();
                self.config.touch_wheel =
                    crate::config::Config::load_touch_wheel(character).unwrap_or_default();
                self.config.controller_overlay =
                    crate::config::Config::load_controller_overlay(character).unwrap_or_default();
                self.config.controller_rumble =
                    crate::config::Config::load_controller_rumble(character).unwrap_or_default();
                self.config.controller_tuning =
                    crate::config::Config::load_controller_tuning(character).unwrap_or_default();
                // Rebuild keybind map for O(1) lookups (re-merges hotbar keys)
                self.rebuild_keybind_map();
                // Web clients render the wheel from a shipped copy.
                self.push_remote_wheels();
                self.warn_wheel_binding_conflicts();
                self.warn_wheel_span_conflicts();
                self.add_system_message("Keybinds reloaded");
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload keybinds: {}", e));
            }
        }
    }

    /// Reload hotbars from disk and re-register their hotkeys
    pub fn reload_hotbars(&mut self) {
        match crate::config::Config::load_hotbars(self.config.character.as_deref()) {
            Ok(hotbars) => {
                self.config.hotbars = hotbars;
                self.rebuild_keybind_map();
                for conflict in &self.hotbar_key_conflicts.clone() {
                    self.add_system_message(&format!(
                        "Hotbar key '{}' ({}:{}) not registered - already bound by {}",
                        conflict.key, conflict.bar, conflict.button, conflict.conflicts_with
                    ));
                }
                self.add_system_message("Hotbars reloaded");
                self.needs_render = true;
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload hotbars: {}", e));
            }
        }
    }

    /// Reload settings (UI, connection, sound) from disk
    pub fn reload_settings(&mut self) {
        let config_path = match crate::config::Config::config_path(self.config.character.as_deref()) {
            Ok(path) => path,
            Err(e) => {
                self.add_system_message(&format!("Failed to get config path: {}", e));
                return;
            }
        };

        match std::fs::read_to_string(&config_path) {
            Ok(contents) => {
                match toml::from_str::<crate::config::Config>(&contents) {
                    Ok(new_config) => {
                        // Update only the settings sections, preserve character name and runtime state
                        self.config.connection = new_config.connection;
                        self.config.ui = new_config.ui;
                        self.config.sound = new_config.sound;
                        self.config.event_patterns = new_config.event_patterns;
                        self.parser
                            .update_event_patterns(self.config.event_patterns.clone());
                        self.message_processor.apply_config(self.config.clone());
                        self.add_system_message("Settings reloaded");
                    }
                    Err(e) => {
                        self.add_system_message(&format!("Failed to parse config: {}", e));
                    }
                }
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to read config file: {}", e));
            }
        }
    }

    /// Reload colors (presets, spell colors, prompt colors, UI colors) from disk
    pub fn reload_colors(&mut self) {
        match crate::config::ColorConfig::load(self.config.character.as_deref()) {
            Ok(colors) => {
                self.config.colors = colors;
                // Update parser with new presets - resolve palette names to hex values
                let presets: Vec<(String, Option<String>, Option<String>)> = self
                    .config
                    .colors
                    .presets
                    .iter()
                    .map(|(id, preset)| {
                        let resolved_fg = preset
                            .fg
                            .as_ref()
                            .map(|c| self.config.resolve_palette_color(c));
                        let resolved_bg = preset
                            .bg
                            .as_ref()
                            .map(|c| self.config.resolve_palette_color(c));
                        (id.clone(), resolved_fg, resolved_bg)
                    })
                    .collect();
                self.parser.update_presets(presets);
                self.message_processor.apply_config(self.config.clone());
                self.add_system_message("Colors reloaded");
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload colors: {}", e));
            }
        }
    }

    /// Reload layout from the auto-saved layout.toml file
    ///
    /// This reloads the character's layout from ~/.vellum-fe/{character}/layout.toml
    /// using the current terminal size stored in the layout.
    pub fn reload_layout(&mut self) {
        let layout_path =
            match crate::config::Config::auto_layout_path(self.config.character.as_deref()) {
                Ok(path) => path,
                Err(e) => {
                    self.add_system_message(&format!("Failed to get layout path: {}", e));
                    return;
                }
            };

        if !layout_path.exists() {
            self.add_system_message("No auto-saved layout found");
            self.add_system_message("Use .savelayout to save the current layout first");
            return;
        }

        match crate::config::Layout::load_from_file(&layout_path) {
            Ok(new_layout) => {
                // Get terminal size from current layout (use current if available)
                let width = self.layout.terminal_width.unwrap_or(80);
                let height = self.layout.terminal_height.unwrap_or(24);

                // Apply theme if specified
                self.apply_layout_theme(new_layout.theme.as_deref());

                // Update layout and baseline
                self.layout = new_layout.clone();
                self.baseline_layout = Some(new_layout);

                // Clear modified flag
                self.layout_modified_since_save = false;

                // Reinitialize windows with current terminal size
                self.init_windows(width, height);
                self.needs_render = true;

                // Signal frontend to reset widget caches
                self.ui_state.needs_widget_reset = true;

                self.add_system_message("Layout reloaded from disk");
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to reload layout: {}", e));
            }
        }
    }
}
