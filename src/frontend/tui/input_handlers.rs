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
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::input::KeyCode;

        // Handle Ctrl+PageUp/PageDown for cycling through search results
        if modifiers.ctrl {
            match code {
                KeyCode::PageUp => {
                    let focused_name = app_core.get_focused_window_name();
                    if self.prev_search_match(&focused_name) {
                        tracing::debug!("Jumped to previous search match in '{}'", focused_name);
                    } else {
                        tracing::debug!("No more search matches in '{}'", focused_name);
                    }
                    app_core.needs_render = true;
                    return Ok(None);
                }
                KeyCode::PageDown => {
                    let focused_name = app_core.get_focused_window_name();
                    if self.next_search_match(&focused_name) {
                        tracing::debug!("Jumped to next search match in '{}'", focused_name);
                    } else {
                        tracing::debug!("No more search matches in '{}'", focused_name);
                    }
                    app_core.needs_render = true;
                    return Ok(None);
                }
                _ => {}
            }
        }

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
                // `search_cursor` is a CHAR index (the renderer in command_input.rs
                // slices via `.chars()`), so translate it to a byte offset before
                // touching the byte-indexed String. Inserting at a byte offset that
                // lands mid-codepoint would panic.
                let pos = app_core.ui_state.search_cursor;
                let byte_pos = char_index_to_byte(&app_core.ui_state.search_input, pos);
                app_core.ui_state.search_input.insert(byte_pos, c);
                app_core.ui_state.search_cursor += 1;
                app_core.needs_render = true;
            }
            KeyCode::Backspace => {
                if app_core.ui_state.search_cursor > 0 {
                    app_core.ui_state.search_cursor -= 1;
                    let byte_pos = char_index_to_byte(
                        &app_core.ui_state.search_input,
                        app_core.ui_state.search_cursor,
                    );
                    app_core.ui_state.search_input.remove(byte_pos);
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
                // Bound against CHAR count, not byte length (cursor is a char index).
                if app_core.ui_state.search_cursor
                    < app_core.ui_state.search_input.chars().count()
                {
                    app_core.ui_state.search_cursor += 1;
                    app_core.needs_render = true;
                }
            }
            KeyCode::Home => {
                app_core.ui_state.search_cursor = 0;
                app_core.needs_render = true;
            }
            KeyCode::End => {
                app_core.ui_state.search_cursor = app_core.ui_state.search_input.chars().count();
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
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::input::KeyCode;
        use crate::data::window::WidgetType;

        // Esc cancels an active .go2 trip. Reaching Normal mode means every
        // higher-priority layer (popups, editors, menus) already declined the
        // key, and the gate on is_traveling keeps Esc inert otherwise.
        if matches!(code, KeyCode::Esc)
            && modifiers == crate::data::input::KeyModifiers::NONE
            && app_core.travel.is_traveling()
        {
            app_core.stop_travel();
            app_core.needs_render = true;
            return Ok(None);
        }

        let focused_name = app_core.get_focused_window_name();
        if let Some(window) = app_core.ui_state.get_window(&focused_name) {
            if window.widget_type == WidgetType::Quickbar {
                match code {
                    KeyCode::Left => {
                        if let Some(widget) = self.widget_manager.quickbar_widgets.get_mut(&focused_name) {
                            widget.move_selection(-1);
                            app_core.needs_render = true;
                            return Ok(None);
                        }
                    }
                    KeyCode::Right => {
                        if let Some(widget) = self.widget_manager.quickbar_widgets.get_mut(&focused_name) {
                            widget.move_selection(1);
                            app_core.needs_render = true;
                            return Ok(None);
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(widget) = self.widget_manager.quickbar_widgets.get_mut(&focused_name) {
                            if let Some(action) = widget.activate_selected() {
                                return Ok(self.handle_quickbar_action(action, &focused_name, app_core));
                            }
                        }
                    }
                    KeyCode::Esc => {
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    _ => {}
                }
            }
        }

        if matches!(code, KeyCode::BackTab) {
            app_core.cycle_focused_window_reverse();
            app_core.needs_render = true;
            return Ok(None);
        }

        // Handle Enter key - always submit command
        if matches!(code, KeyCode::Enter) {
            if let Some(command) = self.command_input_submit("command_input") {
                tracing::debug!(
                    "Command submitted (len={}, bytes={:?}): '{}'",
                    command.len(),
                    command.as_bytes().iter().take(10).collect::<Vec<_>>(),
                    command
                );
                return self.handle_command_submission(command, app_core);
            }
        } else {
            // Check for keybinds first - normalize to lowercase for consistent matching
            let normalized_code = match code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                other => other,
            };
            let key_event = crate::data::input::KeyEvent { code: normalized_code, modifiers };
            if let Some(action) = app_core.keybind_map.get(&key_event).cloned() {
                // Repeat-from-history actions submit directly; they can't go
                // through command_input_key, which re-reads the raw key and
                // would drop it (these actions never worked via that path)
                if let crate::config::KeyBindAction::Action(s) = &action {
                    if s == "send_last_command" || s == "send_second_last_command" {
                        let cmd = self
                            .widget_manager
                            .command_inputs
                            .get("command_input")
                            .and_then(|input| {
                                if s == "send_last_command" {
                                    input.get_last_command()
                                } else {
                                    input.get_second_last_command()
                                }
                            });
                        app_core.needs_render = true;
                        if let Some(cmd) = cmd {
                            return self.handle_command_submission(cmd, app_core);
                        }
                        return Ok(None);
                    }
                }

                let is_command_input_action = matches!(&action,
                    crate::config::KeyBindAction::Action(s) if matches!(s.as_str(),
                        "cursor_left" | "cursor_right" | "cursor_word_left" | "cursor_word_right" |
                        "cursor_home" | "cursor_end" | "cursor_backspace" | "cursor_delete" |
                        "previous_command" | "next_command"
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
                        Ok(outcomes) => {
                            for outcome in outcomes {
                                match outcome {
                                    crate::data::CommandOutcome::Game(cmd) => {
                                        app_core.needs_render = true;
                                        return Ok(Some(cmd));
                                    }
                                    crate::data::CommandOutcome::Handled => {}
                                    // A macro bound to a dot-command that
                                    // opens an editor: perform it here.
                                    crate::data::CommandOutcome::Ui(ui) => {
                                        menu_actions::handle_ui_action(app_core, self, ui)?;
                                    }
                                }
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
        tracing::debug!("handle_command_submission: start '{}'", command);
        // Layout commands ride the same core dispatch as everything else
        // now (parity plan D3): core emits SaveLayout/LoadLayout/... UI
        // actions and menu_actions supplies the live terminal size.
        let outcome = app_core.send_command(command)?;
        tracing::debug!("handle_command_submission: send_command returned {:?}", outcome);
        app_core.needs_render = true;
        match outcome {
            crate::data::CommandOutcome::Ui(action) => {
                // Perform internal UI actions locally instead of
                // sending anything to the game.
                menu_actions::handle_ui_action(app_core, self, action)?;
                Ok(None)
            }
            crate::data::CommandOutcome::Handled => Ok(None),
            crate::data::CommandOutcome::Game(to_send) => {
                tracing::debug!("handle_command_submission: queued for network");
                Ok(Some(to_send))
            }
        }
    }

    fn handle_quickbar_action(
        &mut self,
        action: super::quickbar::QuickbarAction,
        window_name: &str,
        app_core: &mut crate::core::AppCore,
    ) -> Option<String> {
        match action {
            super::quickbar::QuickbarAction::OpenSwitcher => {
                if let Some(window) = app_core.ui_state.get_window(window_name) {
                    self.open_quickbar_switcher(app_core, window.position.clone());
                    app_core.needs_render = true;
                }
                None
            }
            super::quickbar::QuickbarAction::ExecuteCommand(command) => Some(command),
            super::quickbar::QuickbarAction::MenuRequest { exist, noun } => {
                let click_pos = app_core
                    .ui_state
                    .get_window(window_name)
                    .map(|w| (w.position.x.get(), w.position.y.get()))
                    .unwrap_or((0, 0));
                Some(app_core.request_menu(exist, noun, click_pos))
            }
        }
    }
}

/// Translate a char index into a byte offset within `s`.
///
/// The search input stores its cursor as a char count (the renderer slices via
/// `.chars()`), so any mutation of the underlying byte-indexed `String` must go
/// through this conversion. A char index at or past the end maps to `s.len()`,
/// which is a valid insertion point. Mirrors `char_pos_to_byte_idx` in
/// `frontend/common/command_input_model.rs`.
fn char_index_to_byte(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}
