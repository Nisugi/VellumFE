//! Input handling for Search and Normal modes
//!
//! These methods handle keyboard input routing based on the current input mode.
//! Extracted from mod.rs to reduce file size and improve organization.

use anyhow::Result;
use crate::frontend::tui::menu_actions;

/// Input handling methods (impl extension for TuiFrontend)
impl super::TuiFrontend {
    pub(super) fn handle_search_mode_keys(
        &mut self,
        code: crate::frontend::KeyCode,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::frontend::KeyCode;

        match code {
            KeyCode::Enter => {
                let pattern = app_core.ui_state.search_input.clone();
                if !pattern.is_empty() {
                    let window_name = app_core.get_focused_window_name();
                    match self.execute_search(&window_name, &pattern) {
                        Ok(count) => {
                            if count > 0 {
                                tracing::info!("Found {} matches for '{}'", count, pattern);
                            } else {
                                tracing::info!("No matches found for '{}'", pattern);
                            }
                            app_core.needs_render = true;
                        }
                        Err(e) => {
                            tracing::warn!("Invalid search regex '{}': {}", pattern, e);
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                let pos = app_core.ui_state.search_cursor;
                app_core.ui_state.search_input.insert(pos, c);
                app_core.ui_state.search_cursor += 1;
                app_core.needs_render = true;
            }
            KeyCode::Backspace => {
                if app_core.ui_state.search_cursor > 0 {
                    app_core.ui_state.search_cursor -= 1;
                    app_core
                        .ui_state
                        .search_input
                        .remove(app_core.ui_state.search_cursor);
                    app_core.needs_render = true;
                }
            }
            KeyCode::Left => {
                if app_core.ui_state.search_cursor > 0 {
                    app_core.ui_state.search_cursor -= 1;
                    app_core.needs_render = true;
                }
            }
            KeyCode::Right => {
                if app_core.ui_state.search_cursor < app_core.ui_state.search_input.len() {
                    app_core.ui_state.search_cursor += 1;
                    app_core.needs_render = true;
                }
            }
            KeyCode::Home => {
                app_core.ui_state.search_cursor = 0;
                app_core.needs_render = true;
            }
            KeyCode::End => {
                app_core.ui_state.search_cursor = app_core.ui_state.search_input.len();
                app_core.needs_render = true;
            }
            KeyCode::Esc => {
                // Exit search mode
                app_core.ui_state.input_mode = crate::data::InputMode::Normal;
                app_core.ui_state.search_input.clear();
                app_core.ui_state.search_cursor = 0;
                app_core.needs_render = true;
                tracing::debug!("Exited search mode");
            }
            _ => {}
        }
        Ok(None)
    }

    /// Handle Normal mode keyboard events (extracted from main.rs Phase 4.2)
    pub(super) fn handle_normal_mode_keys(
        &mut self,
        code: crate::frontend::KeyCode,
        modifiers: crate::frontend::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::frontend::KeyCode;

        // Handle Enter key - always submit command
        if matches!(code, KeyCode::Enter) {
            if let Some(command) = self.command_input_submit("command_input") {
                return self.handle_command_submission(command, app_core);
            }
        } else {
            // Check for keybinds first - normalize to lowercase for consistent matching
            let normalized_code = match code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                other => other,
            };
            let key_event = crate::frontend::common::KeyEvent { code: normalized_code, modifiers };
            if let Some(action) = app_core.keybind_map.get(&key_event).cloned() {
                let is_command_input_action = matches!(&action,
                    crate::config::KeyBindAction::Action(s) if matches!(s.as_str(),
                        "cursor_left" | "cursor_right" | "cursor_word_left" | "cursor_word_right" |
                        "cursor_home" | "cursor_end" | "cursor_backspace" | "cursor_delete" |
                        "previous_command" | "next_command" | "send_last_command" | "send_second_last_command"
                    )
                );

                let is_tab_action = matches!(&action,
                    crate::config::KeyBindAction::Action(s) if matches!(s.as_str(),
                        "next_tab" | "prev_tab" | "next_unread_tab"
                    )
                );

                // Check for switch_current_window (Tab key) - smart behavior:
                // - If command input has text starting with '.', do tab completion
                // - Otherwise, cycle focused window for scrolling
                let is_switch_window_action = matches!(&action,
                    crate::config::KeyBindAction::Action(s) if s.as_str() == "switch_current_window"
                );

                // Check for search actions - must be handled by frontend
                let is_search_action = matches!(&action,
                    crate::config::KeyBindAction::Action(s) if matches!(s.as_str(),
                        "start_search" | "next_search_match" | "prev_search_match" | "clear_search"
                    )
                );

                // Check for scroll actions - must be handled by frontend (TuiFrontend.scroll_window)
                let is_scroll_action = matches!(&action,
                    crate::config::KeyBindAction::Action(s) if matches!(s.as_str(),
                        "scroll_current_window_up_one" | "scroll_current_window_down_one" |
                        "scroll_current_window_up_page" | "scroll_current_window_down_page" |
                        "scroll_current_window_home" | "scroll_current_window_end"
                    )
                );

                if is_search_action {
                    // Handle search actions
                    if let crate::config::KeyBindAction::Action(action_str) = &action {
                        match action_str.as_str() {
                            "start_search" => {
                                // Enter search mode
                                app_core.ui_state.input_mode = crate::data::InputMode::Search;
                                app_core.ui_state.search_input.clear();
                                app_core.ui_state.search_cursor = 0;
                                tracing::debug!("Entered search mode");
                            }
                            "next_search_match" => {
                                let focused_name = app_core.get_focused_window_name();
                                if self.next_search_match(&focused_name) {
                                    tracing::debug!("Jumped to next search match in '{}'", focused_name);
                                } else {
                                    tracing::debug!("No more search matches in '{}'", focused_name);
                                }
                            }
                            "prev_search_match" => {
                                let focused_name = app_core.get_focused_window_name();
                                if self.prev_search_match(&focused_name) {
                                    tracing::debug!("Jumped to previous search match in '{}'", focused_name);
                                } else {
                                    tracing::debug!("No more search matches in '{}'", focused_name);
                                }
                            }
                            "clear_search" => {
                                self.clear_all_searches();
                                tracing::debug!("Cleared all searches");
                            }
                            _ => {}
                        }
                    }
                    app_core.needs_render = true;
                } else if is_scroll_action {
                    // Get the focused window name and scroll it via frontend
                    let focused_name = app_core.get_focused_window_name();
                    if let crate::config::KeyBindAction::Action(action_str) = &action {
                        match action_str.as_str() {
                            "scroll_current_window_up_one" => {
                                self.scroll_window(&focused_name, 1);
                                tracing::debug!("Scrolled '{}' up 1 line via frontend", focused_name);
                            }
                            "scroll_current_window_down_one" => {
                                self.scroll_window(&focused_name, -1);
                                tracing::debug!("Scrolled '{}' down 1 line via frontend", focused_name);
                            }
                            "scroll_current_window_up_page" => {
                                self.scroll_window(&focused_name, 20);
                                tracing::info!("Scrolled '{}' up 20 lines via frontend", focused_name);
                            }
                            "scroll_current_window_down_page" => {
                                self.scroll_window(&focused_name, -20);
                                tracing::info!("Scrolled '{}' down 20 lines via frontend", focused_name);
                            }
                            "scroll_current_window_home" => {
                                // Scroll to top - use a large number
                                self.scroll_window(&focused_name, 100000);
                                tracing::debug!("Scrolled '{}' to top via frontend", focused_name);
                            }
                            "scroll_current_window_end" => {
                                // Scroll to bottom - use a large negative number
                                self.scroll_window(&focused_name, -100000);
                                tracing::debug!("Scrolled '{}' to bottom via frontend", focused_name);
                            }
                            _ => {}
                        }
                    }
                    app_core.needs_render = true;
                } else if is_switch_window_action {
                    // Check if command input has text that should trigger tab completion
                    let should_complete = self
                        .widget_manager
                        .command_inputs
                        .get("command_input")
                        .and_then(|cmd| cmd.get_input())
                        .map(|text| text.starts_with('.'))
                        .unwrap_or(false);

                    if should_complete {
                        // Do tab completion for dot commands
                        let available_commands = app_core.get_available_commands();
                        let available_window_names = app_core.get_window_names();
                        use crate::frontend::tui::crossterm_bridge;
                        let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                        let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                        self.command_input_key(
                            "command_input",
                            ct_code,
                            ct_mods,
                            &available_commands,
                            &available_window_names,
                        );
                    } else {
                        // No text or doesn't start with '.', cycle focused window
                        app_core.cycle_focused_window();
                    }
                    app_core.needs_render = true;
                } else if is_tab_action {
                    if let crate::config::KeyBindAction::Action(action_str) = &action {
                        match action_str.as_str() {
                            "next_tab" => {
                                self.next_tab_all();
                                self.sync_tabbed_active_state(app_core);
                                tracing::info!("Switched to next tab in all tabbed windows");
                            }
                            "prev_tab" => {
                                self.prev_tab_all();
                                self.sync_tabbed_active_state(app_core);
                                tracing::info!("Switched to previous tab in all tabbed windows");
                            }
                            "next_unread_tab" => {
                                if !self.go_to_next_unread_tab() {
                                    app_core.add_system_message("No tabs with new messages");
                                }
                                self.sync_tabbed_active_state(app_core);
                                tracing::info!("Next unread tab navigation triggered");
                            }
                            _ => {}
                        }
                    }
                    app_core.needs_render = true;
                } else if is_command_input_action {
                    let available_commands = app_core.get_available_commands();
                    let available_window_names = app_core.get_window_names();
                    use crate::frontend::tui::crossterm_bridge;
                    let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                    let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                    self.command_input_key(
                        "command_input",
                        ct_code,
                        ct_mods,
                        &available_commands,
                        &available_window_names,
                    );
                    app_core.needs_render = true;
                } else {
                    match app_core.execute_keybind_action(&action) {
                        Ok(commands) => {
                            if let Some(cmd) = commands.into_iter().next() {
                                app_core.needs_render = true;
                                return Ok(Some(cmd));
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Keybind action failed: {}", e);
                        }
                    }
                    app_core.needs_render = true;
                }
            } else {
                // No keybind - route to CommandInput for typing
                let available_commands = app_core.get_available_commands();
                let available_window_names = app_core.get_window_names();
                use crate::frontend::tui::crossterm_bridge;
                let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                self.command_input_key(
                    "command_input",
                    ct_code,
                    ct_mods,
                    &available_commands,
                    &available_window_names,
                );
                app_core.needs_render = true;
            }
        }
        Ok(None)
    }

    /// Handle command submission from CommandInput (extracted from main.rs Phase 4.2)
    pub(super) fn handle_command_submission(
        &mut self,
        command: String,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        if command.starts_with(".savelayout ") || command == ".savelayout" {
            let name = command
                .strip_prefix(".savelayout ")
                .unwrap_or("default")
                .trim();
            let (width, height) = self.size();
            app_core.save_layout(name, width, height);
            app_core.needs_render = true;
        } else if command.starts_with(".loadlayout ") || command == ".loadlayout" {
            let name = command
                .strip_prefix(".loadlayout ")
                .unwrap_or("default")
                .trim();
            let (width, height) = self.size();
            if let Some((theme_id, theme)) = app_core.load_layout(name, width, height) {
                self.update_theme_cache(theme_id, theme);
            }
            app_core.needs_render = true;
        } else if command == ".resize" {
            let (width, height) = self.size();
            app_core.resize_windows(width, height);
            app_core.needs_render = true;
        } else {
            let to_send = app_core.send_command(command)?;
            if to_send.starts_with("action:") {
                // Handle internal UI actions locally instead of sending to the game
                menu_actions::handle_menu_action(app_core, self, &to_send)?;
                app_core.needs_render = true;
                return Ok(None);
            }

            if to_send.is_empty() {
                app_core.needs_render = true;
                return Ok(None);
            }

            app_core.needs_render = true;
            return Ok(Some(to_send));
        }
        Ok(None)
    }
}
