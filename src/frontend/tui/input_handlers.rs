//! Input handling for Search and Normal modes
//!
//! These methods handle keyboard input routing based on the current input mode.
//! Extracted from mod.rs to reduce file size and improve organization.

use crate::frontend::tui::menu_actions;
use anyhow::Result;

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
                if app_core.ui_state.search_cursor < app_core.ui_state.search_input.chars().count()
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
                        if let Some(widget) =
                            self.widget_manager.quickbar_widgets.get_mut(&focused_name)
                        {
                            widget.move_selection(-1);
                            app_core.needs_render = true;
                            return Ok(None);
                        }
                    }
                    KeyCode::Right => {
                        if let Some(widget) =
                            self.widget_manager.quickbar_widgets.get_mut(&focused_name)
                        {
                            widget.move_selection(1);
                            app_core.needs_render = true;
                            return Ok(None);
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(widget) =
                            self.widget_manager.quickbar_widgets.get_mut(&focused_name)
                        {
                            if let Some(action) = widget.activate_selected() {
                                return Ok(self.handle_quickbar_action(
                                    action,
                                    &focused_name,
                                    app_core,
                                ));
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
            // For plain (non-dot) input, a visible history suggestion owns
            // plain Tab regardless of how Tab is rebound. Dot input is left
            // for the switch-window path below, which runs dot-command
            // completion FIRST and only falls back to the suggestion once
            // completion has nothing new — so `.la` Tab Tab gives ".launch"
            // then ".launch nisugi".
            if matches!(code, KeyCode::Tab)
                && modifiers == crate::data::input::KeyModifiers::NONE
                && self
                    .widget_manager
                    .command_inputs
                    .get_mut("command_input")
                    .is_some_and(|input| {
                        !input.get_input().is_some_and(|text| text.starts_with('.'))
                            && input.accept_history_completion()
                    })
            {
                app_core.needs_render = true;
                return Ok(None);
            }

            // Check for keybinds first - normalize to lowercase for consistent matching
            let normalized_code = match code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                other => other,
            };
            let key_event = crate::data::input::KeyEvent {
                code: normalized_code,
                modifiers,
            };
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
                        "cursor_delete_word" | "cursor_clear_line" |
                        "previous_command" | "next_command" |
                        "copy" | "paste" | "select_all"
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
                                    tracing::debug!(
                                        "Jumped to next search match in '{}'",
                                        focused_name
                                    );
                                } else {
                                    tracing::debug!("No more search matches in '{}'", focused_name);
                                }
                            }
                            "prev_search_match" => {
                                let focused_name = app_core.get_focused_window_name();
                                if self.prev_search_match(&focused_name) {
                                    tracing::debug!(
                                        "Jumped to previous search match in '{}'",
                                        focused_name
                                    );
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
                                tracing::debug!(
                                    "Scrolled '{}' up 1 line via frontend",
                                    focused_name
                                );
                            }
                            "scroll_current_window_down_one" => {
                                self.scroll_window(&focused_name, -1);
                                tracing::debug!(
                                    "Scrolled '{}' down 1 line via frontend",
                                    focused_name
                                );
                            }
                            "scroll_current_window_up_page" => {
                                self.scroll_window(&focused_name, 20);
                                tracing::info!(
                                    "Scrolled '{}' up 20 lines via frontend",
                                    focused_name
                                );
                            }
                            "scroll_current_window_down_page" => {
                                self.scroll_window(&focused_name, -20);
                                tracing::info!(
                                    "Scrolled '{}' down 20 lines via frontend",
                                    focused_name
                                );
                            }
                            "scroll_current_window_home" => {
                                // Scroll to top - use a large number
                                self.scroll_window(&focused_name, 100000);
                                tracing::debug!("Scrolled '{}' to top via frontend", focused_name);
                            }
                            "scroll_current_window_end" => {
                                // Scroll to bottom - use a large negative number
                                self.scroll_window(&focused_name, -100000);
                                tracing::debug!(
                                    "Scrolled '{}' to bottom via frontend",
                                    focused_name
                                );
                            }
                            _ => {}
                        }
                    }
                    app_core.needs_render = true;
                } else if is_switch_window_action {
                    // Keep the configured switch-window key in the command
                    // input for dot input. command_input_key's Tab arm runs
                    // dot-command completion first and falls back to the
                    // history suggestion once completion is settled. (Plain
                    // non-dot Tab history accept ran before keybind dispatch.)
                    let should_complete = self
                        .widget_manager
                        .command_inputs
                        .get("command_input")
                        .and_then(|cmd| cmd.get_input())
                        .is_some_and(|text| text.starts_with('.'));

                    if should_complete {
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
                    if let crate::config::KeyBindAction::Action(name) = &action {
                        self.apply_command_input_action("command_input", name, modifiers.shift);
                    }
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
        self.handle_command_submission_from(None, command, app_core)
    }

    pub(super) fn handle_remote_command_submission(
        &mut self,
        client_id: u64,
        command: String,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        self.handle_command_submission_from(Some(client_id), command, app_core)
    }

    fn handle_command_submission_from(
        &mut self,
        remote_client_id: Option<u64>,
        command: String,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        tracing::debug!("handle_command_submission: start '{}'", command);
        // Layout commands ride the same core dispatch as everything else
        // now (parity plan D3): core emits SaveLayout/LoadLayout/... UI
        // actions and menu_actions supplies the live terminal size.
        let outcome = match remote_client_id {
            Some(client_id) => app_core.send_remote_command(client_id, command)?,
            None => app_core.send_command(command)?,
        };
        tracing::debug!(
            "handle_command_submission: send_command returned {:?}",
            outcome
        );
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

    /// Highlight browser keys: navigate, save (Ctrl+S persists + recompiles),
    /// edit/new (opens the highlight form), delete (per-scope), cancel.
    pub(super) fn handle_highlight_browser_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut browser) = self.highlight_browser {
            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextItem
                | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                crate::core::menu_actions::MenuAction::PreviousItem
                | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                crate::core::menu_actions::MenuAction::NextPage => browser.next_page(),
                crate::core::menu_actions::MenuAction::PreviousPage => browser.previous_page(),
                crate::core::menu_actions::MenuAction::Save => {
                    // Ctrl+S: persist highlights, refresh caches, close browser
                    if let Err(e) = app_core
                        .config
                        .save_highlights(app_core.config.character.as_deref())
                    {
                        app_core.add_system_message(&format!("Failed to save highlights: {}", e));
                    } else {
                        app_core.add_system_message("Highlights saved");
                        crate::config::Config::compile_highlight_patterns(
                            &mut app_core.config.highlights,
                        );
                        app_core
                            .message_processor
                            .apply_config(app_core.config.clone());
                    }
                    self.highlight_browser = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                crate::core::menu_actions::MenuAction::Edit => {
                    if let Some(name) = browser.get_selected() {
                        if let Some(pattern) = app_core.config.highlights.get(&name) {
                            let is_global = browser.get_selected_is_global().unwrap_or(true);
                            let mut form =
                                crate::frontend::tui::highlight_form::HighlightFormWidget::new_edit(
                                    name, pattern,
                                );
                            form.set_scope(is_global);
                            form.set_rumble_options(
                                app_core.config.controller_rumble.pattern_names(),
                            );
                            self.highlight_form = Some(form);
                            app_core.ui_state.input_mode = InputMode::HighlightForm;
                        }
                    }
                }
                crate::core::menu_actions::MenuAction::New
                | crate::core::menu_actions::MenuAction::Add => {
                    let mut form = crate::frontend::tui::highlight_form::HighlightFormWidget::new();
                    form.set_rumble_options(app_core.config.controller_rumble.pattern_names());
                    self.highlight_form = Some(form);
                    app_core.ui_state.input_mode = InputMode::HighlightForm;
                }
                crate::core::menu_actions::MenuAction::Delete => {
                    if let Some(name) = browser.get_selected() {
                        // Both scopes: a character override can shadow a
                        // same-named global entry, and deleting only one
                        // copy uncovers the other on reload.
                        if let Err(e) = crate::config::Config::delete_highlight_everywhere(
                            &name,
                            app_core.config.character.as_deref(),
                        ) {
                            app_core
                                .add_system_message(&format!("Failed to delete highlight: {}", e));
                        } else {
                            app_core.add_system_message(&format!("Highlight '{}' deleted.", name));
                            app_core.config.highlights.remove(&name);
                            crate::config::Config::compile_highlight_patterns(
                                &mut app_core.config.highlights,
                            );
                            app_core
                                .message_processor
                                .apply_config(app_core.config.clone());
                            let global =
                                crate::config::Config::load_common_highlights().unwrap_or_default();
                            let character = crate::config::Config::load_character_highlights_only(
                                app_core.config.character.as_deref(),
                            )
                            .unwrap_or_default();
                            browser.update_items_with_source(&global, &character);
                        }
                        tracing::info!("Deleted highlight '{}' from both scopes", name);
                    }
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.highlight_browser = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                _ => {}
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_keybind_browser_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut browser) = self.keybind_browser {
            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextItem
                | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                crate::core::menu_actions::MenuAction::PreviousItem
                | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                crate::core::menu_actions::MenuAction::NextPage => browser.next_page(),
                crate::core::menu_actions::MenuAction::PreviousPage => browser.previous_page(),
                crate::core::menu_actions::MenuAction::ToggleFilter => browser.toggle_filter(),
                crate::core::menu_actions::MenuAction::Select
                | crate::core::menu_actions::MenuAction::Edit => {
                    if let Some(entry) = browser.get_selected_entry() {
                        use crate::frontend::tui::keybind_form::KeybindActionType;
                        let action_type = if entry.action_type == "Action" {
                            KeybindActionType::Action
                        } else {
                            KeybindActionType::Macro
                        };
                        self.keybind_form = Some(
                            crate::frontend::tui::keybind_form::KeybindFormWidget::new_edit(
                                entry.key_combo.clone(),
                                action_type,
                                entry.action_value.clone(),
                                entry.is_global,
                            ),
                        );
                        app_core.ui_state.input_mode = InputMode::KeybindForm;
                    }
                }
                crate::core::menu_actions::MenuAction::New
                | crate::core::menu_actions::MenuAction::Add => {
                    self.keybind_form =
                        Some(crate::frontend::tui::keybind_form::KeybindFormWidget::new());
                    app_core.ui_state.input_mode = InputMode::KeybindForm;
                }
                crate::core::menu_actions::MenuAction::Delete => {
                    if let Some(entry) = browser.get_selected_entry() {
                        let key_combo = entry.key_combo.clone();
                        // Remove from the file the entry came from, or the
                        // reload below immediately resurrects it in the list
                        if let Err(e) = crate::config::Config::delete_single_keybind(
                            &key_combo,
                            entry.is_global,
                            app_core.config.character.as_deref(),
                        ) {
                            tracing::error!("Failed to delete keybind '{}': {}", key_combo, e);
                        }
                        app_core.config.keybinds.remove(&key_combo);
                        app_core.rebuild_keybind_map();
                        // Reload with source tracking for proper [G]/[C] display
                        let global_keybinds =
                            crate::config::Config::load_common_keybinds().unwrap_or_default();
                        let character_keybinds =
                            crate::config::Config::load_character_keybinds_only(
                                app_core.config.character.as_deref(),
                            )
                            .unwrap_or_default();
                        browser.update_items_with_source(&global_keybinds, &character_keybinds);
                        tracing::info!("Deleted keybind: {}", key_combo);
                    }
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.keybind_browser = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                _ => {}
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_hotbar_editor_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::ui_state::InputMode;
        if let Some(mut editor) = self.hotbar_editor.take() {
            let ct_event = crossterm::event::KeyEvent::new(
                super::crossterm_bridge::to_crossterm_keycode(code),
                super::crossterm_bridge::to_crossterm_modifiers(modifiers),
            );
            match editor.handle_key(ct_event, &app_core.config) {
                crate::frontend::tui::hotbar_editor::HotbarEditorResult::None => {
                    self.hotbar_editor = Some(editor);
                }
                crate::frontend::tui::hotbar_editor::HotbarEditorResult::Close => {
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                crate::frontend::tui::hotbar_editor::HotbarEditorResult::SaveBar(
                    bar,
                    is_global,
                ) => {
                    match crate::config::Config::save_hotbar(
                        &bar,
                        is_global,
                        app_core.config.character.as_deref(),
                    ) {
                        Ok(()) => app_core.reload_hotbars(),
                        Err(e) => {
                            app_core.add_system_message(&format!("Failed to save hotbar: {}", e))
                        }
                    }
                    editor.refresh_bars(&app_core.config);
                    self.hotbar_editor = Some(editor);
                }
                crate::frontend::tui::hotbar_editor::HotbarEditorResult::DeleteBar(name) => {
                    let character = app_core.config.character.clone();
                    let (in_global, in_character) =
                        crate::config::Config::hotbar_scope(&name, character.as_deref());
                    let mut failed = false;
                    if in_character {
                        if let Err(e) =
                            crate::config::Config::delete_hotbar(&name, false, character.as_deref())
                        {
                            app_core.add_system_message(&format!("Failed to delete hotbar: {}", e));
                            failed = true;
                        }
                    }
                    if in_global && !failed {
                        if let Err(e) =
                            crate::config::Config::delete_hotbar(&name, true, character.as_deref())
                        {
                            app_core.add_system_message(&format!("Failed to delete hotbar: {}", e));
                        }
                    }
                    app_core.reload_hotbars();
                    editor.refresh_bars(&app_core.config);
                    self.hotbar_editor = Some(editor);
                }
            }
            app_core.needs_render = true;
        } else {
            app_core.ui_state.input_mode = InputMode::Normal;
        }
        Ok(None)
    }

    /// Skill trainer panel keys. The panel is ui_state-driven (open flag set
    /// by core when the user sends `goals`), not an InputMode: it claims all
    /// keys while `ui_state.skill_trainer.open`.
    pub(super) fn handle_skill_trainer_panel_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        let panel = self.skill_trainer_panel.get_or_insert_with(Default::default);
        let ct_event = crossterm::event::KeyEvent::new(
            super::crossterm_bridge::to_crossterm_keycode(code),
            super::crossterm_bridge::to_crossterm_modifiers(modifiers),
        );
        let result = panel.handle_key(ct_event, app_core);
        app_core.needs_render = true;
        match result {
            crate::frontend::tui::skill_trainer_panel::SkillTrainerPanelResult::None => Ok(None),
            crate::frontend::tui::skill_trainer_panel::SkillTrainerPanelResult::Close => {
                // Keep the loaded data; `.goals` reopens instantly.
                app_core.ui_state.skill_trainer.open = false;
                self.skill_trainer_panel = None;
                Ok(None)
            }
            crate::frontend::tui::skill_trainer_panel::SkillTrainerPanelResult::Send(cmd) => {
                Ok(Some(cmd))
            }
        }
    }

    pub(super) fn handle_color_palette_browser_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut browser) = self.color_palette_browser {
            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextItem
                | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                crate::core::menu_actions::MenuAction::PreviousItem
                | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                crate::core::menu_actions::MenuAction::NextPage => browser.next_page(),
                crate::core::menu_actions::MenuAction::PreviousPage => browser.previous_page(),
                crate::core::menu_actions::MenuAction::Select
                | crate::core::menu_actions::MenuAction::Edit => {
                    if let Some(color) = browser.get_selected_color() {
                        let is_global = browser.get_selected_is_global().unwrap_or(true);
                        self.color_form = Some(
                            crate::frontend::tui::color_form::ColorForm::new_edit_with_scope(
                                color, is_global,
                            ),
                        );
                        app_core.ui_state.input_mode = InputMode::ColorForm;
                    }
                }
                crate::core::menu_actions::MenuAction::New
                | crate::core::menu_actions::MenuAction::Add => {
                    self.color_form =
                        Some(crate::frontend::tui::color_form::ColorForm::new_create());
                    app_core.ui_state.input_mode = InputMode::ColorForm;
                }
                crate::core::menu_actions::MenuAction::Delete => {
                    if let Some(color_name) = browser.get_selected() {
                        let is_global = browser.get_selected_is_global().unwrap_or(true);

                        // Delete from appropriate file based on scope
                        if let Err(e) = crate::config::ColorConfig::delete_single_palette_color(
                            &color_name,
                            is_global,
                            app_core.config.character.as_deref(),
                        ) {
                            tracing::error!("Failed to delete color: {}", e);
                        }

                        // Reload colors to update in-memory state
                        if let Ok(colors) = crate::config::ColorConfig::load_with_merge(
                            app_core.config.character.as_deref(),
                        ) {
                            app_core.config.colors = colors;
                        }

                        // Refresh browser with updated colors
                        let global_colors = crate::config::ColorConfig::load_common_colors()
                            .map(|c| c.color_palette)
                            .unwrap_or_default();
                        let char_colors = crate::config::ColorConfig::load_character_colors_only(
                            app_core.config.character.as_deref(),
                        )
                        .map(|c| c.color_palette)
                        .unwrap_or_default();
                        browser.update_items_with_source(&global_colors, &char_colors);

                        tracing::info!(
                            "Deleted color: {} ({})",
                            color_name,
                            if is_global { "global" } else { "character" }
                        );
                    }
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.color_palette_browser = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                _ => {}
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_uicolors_browser_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut browser) = self.uicolors_browser {
            // The inline editor owns the keyboard while open.
            if browser.editor.is_some() {
                let ct_event = crossterm::event::KeyEvent::new(
                    super::crossterm_bridge::to_crossterm_keycode(code),
                    super::crossterm_bridge::to_crossterm_modifiers(modifiers),
                );
                let result = browser
                    .editor
                    .as_mut()
                    .and_then(|editor| editor.handle_key(ct_event));
                match result {
                    Some(super::uicolors_browser::UIColorEditorResult::Save { fg, bg }) => {
                        // save_editor closes the editor and reports
                        // which entry was being edited.
                        if let Some((category, name, _, _)) = browser.save_editor() {
                            Self::apply_ui_color_edit(app_core, &category, &name, fg, bg);
                            browser.refresh(&app_core.config.colors);
                        }
                    }
                    Some(super::uicolors_browser::UIColorEditorResult::Cancel) => {
                        browser.close_editor();
                    }
                    None => {}
                }
                app_core.needs_render = true;
                return Ok(None);
            }

            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextItem
                | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                crate::core::menu_actions::MenuAction::PreviousItem
                | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                crate::core::menu_actions::MenuAction::NextPage => browser.next_page(),
                crate::core::menu_actions::MenuAction::PreviousPage => browser.previous_page(),
                crate::core::menu_actions::MenuAction::Select
                | crate::core::menu_actions::MenuAction::Edit => {
                    let textarea_bg = app_core.config.colors.ui.textarea_background.clone();
                    browser.open_editor(&textarea_bg);
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.uicolors_browser = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                _ => {}
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_spell_colors_browser_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut browser) = self.spell_color_browser {
            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextItem
                | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                crate::core::menu_actions::MenuAction::PreviousItem
                | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                crate::core::menu_actions::MenuAction::NextPage => browser.next_page(),
                crate::core::menu_actions::MenuAction::PreviousPage => browser.previous_page(),
                crate::core::menu_actions::MenuAction::Select
                | crate::core::menu_actions::MenuAction::Edit => {
                    if let Some(index) = browser.get_selected() {
                        let spell_color = app_core.config.colors.spell_colors.get(index).cloned();
                        if let Some(sc) = spell_color {
                            self.spell_color_form = Some(
                                        crate::frontend::tui::spell_color_form::SpellColorFormWidget::new_edit(
                                            index, &sc,
                                        ),
                                    );
                            app_core.ui_state.input_mode = InputMode::SpellColorForm;
                        }
                    }
                }
                crate::core::menu_actions::MenuAction::New
                | crate::core::menu_actions::MenuAction::Add => {
                    self.spell_color_form =
                        Some(crate::frontend::tui::spell_color_form::SpellColorFormWidget::new());
                    app_core.ui_state.input_mode = InputMode::SpellColorForm;
                }
                crate::core::menu_actions::MenuAction::Delete => {
                    if let Some(index) = browser.get_selected() {
                        if index < app_core.config.colors.spell_colors.len() {
                            app_core.config.colors.spell_colors.remove(index);
                            Self::persist_colors(app_core);
                            browser.update_items(&app_core.config.colors.spell_colors);
                            tracing::info!("Deleted spell color range at index {}", index);
                        }
                    }
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.spell_color_browser = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                _ => {}
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_theme_browser_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut browser) = self.theme_browser {
            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextItem
                | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                crate::core::menu_actions::MenuAction::PreviousItem
                | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                crate::core::menu_actions::MenuAction::NextPage => browser.next_page(),
                crate::core::menu_actions::MenuAction::PreviousPage => browser.previous_page(),
                crate::core::menu_actions::MenuAction::Select => {
                    if let Some(theme_name) = browser.get_selected() {
                        app_core.config.active_theme = theme_name.clone();
                        let theme = app_core.config.get_theme();
                        self.update_theme_cache(theme_name, theme);
                        self.theme_browser = None;
                        app_core.ui_state.input_mode = InputMode::Normal;
                        tracing::info!("Switched to theme: {}", app_core.config.active_theme);
                    }
                }
                crate::core::menu_actions::MenuAction::Edit => {
                    if let Some(theme) = browser.get_selected_theme() {
                        self.theme_editor = Some(
                            crate::frontend::tui::theme_editor::ThemeEditor::new_edit(theme),
                        );
                        self.theme_browser = None;
                        app_core.ui_state.input_mode = InputMode::ThemeEditor;
                    }
                }
                crate::core::menu_actions::MenuAction::New
                | crate::core::menu_actions::MenuAction::Add => {
                    self.theme_editor =
                        Some(crate::frontend::tui::theme_editor::ThemeEditor::new_create());
                    self.theme_browser = None;
                    app_core.ui_state.input_mode = InputMode::ThemeEditor;
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.theme_browser = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                _ => {}
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_settings_editor_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::input::KeyCode;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut editor) = self.settings_editor {
            // First let the editor handle input directly
            // Convert our KeyCode/KeyModifiers to crossterm's
            let crossterm_code = super::crossterm_bridge::to_crossterm_keycode(code);
            let crossterm_modifiers = super::crossterm_bridge::to_crossterm_modifiers(modifiers);
            let key_event = crossterm::event::KeyEvent::new(crossterm_code, crossterm_modifiers);
            let handled = editor.handle_input(key_event);

            if handled {
                // Check if this was a value change - apply to config immediately
                for err in editor.apply_to_config(&mut app_core.config) {
                    app_core.add_system_message(&format!("Setting rejected: {}", err));
                }
                app_core.needs_render = true;
                return Ok(None);
            }

            // Check for Ctrl+S to save all settings
            if modifiers.ctrl && matches!(code, KeyCode::Char('s') | KeyCode::Char('S')) {
                // Apply and save all settings with their scopes
                for err in editor.apply_to_config(&mut app_core.config) {
                    app_core.add_system_message(&format!("Setting rejected: {}", err));
                }

                // Get all items and save each with its scope
                let items_to_save: Vec<_> = editor
                    .all_items()
                    .map(|item| (item.key.clone(), item.is_global))
                    .collect();

                let mut save_errors = Vec::new();
                for (key, is_global) in items_to_save {
                    if let Err(e) = app_core.config.save_single_setting(
                        &key,
                        is_global,
                        app_core.config.character.as_deref(),
                    ) {
                        save_errors.push(format!("{}: {}", key, e));
                    }
                }

                if save_errors.is_empty() {
                    app_core.add_system_message("Settings saved");
                } else {
                    app_core.add_system_message(&format!(
                        "Some settings failed to save: {}",
                        save_errors.join(", ")
                    ));
                }
                app_core.needs_render = true;
                return Ok(None);
            }

            // Handle Cancel/Escape to close editor
            if matches!(code, KeyCode::Esc) {
                // Apply changes to in-memory config before closing
                for err in editor.apply_to_config(&mut app_core.config) {
                    app_core.add_system_message(&format!("Setting rejected: {}", err));
                }
                self.settings_editor = None;
                app_core.ui_state.input_mode = InputMode::Normal;
            }

            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_pack_editor_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::ui_state::InputMode;
        if let Some(ref mut editor) = self.pack_editor {
            use crate::frontend::tui::pack_editor::PackEditorAction;
            let key_event = crate::data::input::KeyEvent { code, modifiers };
            match editor.handle_key(key_event.code, key_event.modifiers) {
                PackEditorAction::None => {}
                PackEditorAction::Close => {
                    self.pack_editor = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                PackEditorAction::Export { name, parts, dest } => {
                    // TUI packs carry no GUI layout entry.
                    let ok = app_core.uiexport_pack(&name, &parts, dest, Vec::new());
                    if let Some(ref mut editor) = self.pack_editor {
                        editor.set_status(if ok {
                            format!("Exported '{name}'.")
                        } else {
                            "Export failed — see the main window.".to_string()
                        });
                    }
                }
                PackEditorAction::Install { path, parts } => {
                    let gui_layout = app_core.uiimport_apply(&path, Some(&parts));
                    if gui_layout.is_some() {
                        app_core.add_system_message(
                                    "This pack also carries a GUI layout — run the import in the GUI to install it.",
                                );
                    }
                    if let Some(ref mut editor) = self.pack_editor {
                        editor.set_status("Install finished — see the main window.");
                    }
                }
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_highlight_form_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::input::KeyCode;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut form) = self.highlight_form {
            use crate::frontend::tui::widget_traits::{FieldNavigable, TextEditable, Toggleable};

            // Fast-path Ctrl+S even if keybinds are overridden/misparsed
            if modifiers.ctrl && matches!(code, KeyCode::Char(c) if c == 's' || c == 'S') {
                if let Some(result) =
                    form.handle_action(crate::core::menu_actions::MenuAction::Save)
                {
                    match result {
                        crate::frontend::tui::highlight_form::FormResult::Save {
                            name,
                            mut pattern,
                            is_global,
                        } => {
                            if let Some(ref fg) = pattern.fg {
                                pattern.fg = Some(app_core.config.resolve_palette_color(fg));
                            }
                            if let Some(ref bg) = pattern.bg {
                                pattern.bg = Some(app_core.config.resolve_palette_color(bg));
                            }
                            // Save to appropriate file based on scope
                            if let Err(e) = crate::config::Config::save_single_highlight(
                                &name,
                                &pattern,
                                is_global,
                                app_core.config.character.as_deref(),
                            ) {
                                app_core.add_system_message(&format!(
                                    "Failed to save highlight: {}",
                                    e
                                ));
                            } else {
                                let scope = if is_global { "global" } else { "character" };
                                app_core.add_system_message(&format!(
                                    "Highlight saved to {} config",
                                    scope
                                ));
                                // Update in-memory config
                                app_core.config.highlights.insert(name.clone(), pattern);
                                crate::config::Config::compile_highlight_patterns(
                                    &mut app_core.config.highlights,
                                );
                                app_core
                                    .message_processor
                                    .apply_config(app_core.config.clone());
                                // Refresh browser with source tracking
                                if let Some(ref mut browser) = self.highlight_browser {
                                    let global = crate::config::Config::load_common_highlights()
                                        .unwrap_or_default();
                                    let character =
                                        crate::config::Config::load_character_highlights_only(
                                            app_core.config.character.as_deref(),
                                        )
                                        .unwrap_or_default();
                                    browser.update_items_with_source(&global, &character);
                                }
                            }
                            tracing::info!("Saved highlight: {} (global={})", name, is_global);
                            self.highlight_form = None;
                            app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                InputMode::HighlightBrowser
                            } else {
                                InputMode::Normal
                            };
                        }
                        _ => {}
                    }
                }
                app_core.needs_render = true;
                return Ok(None);
            }
            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextField => form.next_field(),
                crate::core::menu_actions::MenuAction::PreviousField => form.previous_field(),
                crate::core::menu_actions::MenuAction::SelectAll => form.select_all(),
                crate::core::menu_actions::MenuAction::Copy => {
                    let _ = form.copy_to_clipboard();
                }
                crate::core::menu_actions::MenuAction::Cut => {
                    let _ = form.cut_to_clipboard();
                }
                crate::core::menu_actions::MenuAction::Paste => {
                    let _ = form.paste_from_clipboard();
                }
                crate::core::menu_actions::MenuAction::Toggle => {
                    form.toggle_focused();
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.highlight_form = None;
                    app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                        InputMode::HighlightBrowser
                    } else {
                        InputMode::Normal
                    };
                }
                crate::core::menu_actions::MenuAction::NavigateUp
                | crate::core::menu_actions::MenuAction::NavigateDown
                | crate::core::menu_actions::MenuAction::CycleBackward
                | crate::core::menu_actions::MenuAction::CycleForward
                | crate::core::menu_actions::MenuAction::Select
                | crate::core::menu_actions::MenuAction::Save
                | crate::core::menu_actions::MenuAction::Delete => {
                    // Handle navigation, cycling, and save/delete via handle_action
                    if let Some(result) = form.handle_action(action.clone()) {
                        match result {
                            crate::frontend::tui::highlight_form::FormResult::Save {
                                name,
                                mut pattern,
                                is_global,
                            } => {
                                // Resolve palette color names to hex codes
                                if let Some(ref fg) = pattern.fg {
                                    pattern.fg = Some(app_core.config.resolve_palette_color(fg));
                                }
                                if let Some(ref bg) = pattern.bg {
                                    pattern.bg = Some(app_core.config.resolve_palette_color(bg));
                                }

                                // Save to appropriate file based on scope
                                if let Err(e) = crate::config::Config::save_single_highlight(
                                    &name,
                                    &pattern,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    app_core.add_system_message(&format!(
                                        "Failed to save highlight: {}",
                                        e
                                    ));
                                } else {
                                    let scope = if is_global { "global" } else { "character" };
                                    app_core.add_system_message(&format!(
                                        "Highlight saved to {} config",
                                        scope
                                    ));
                                    // Update in-memory config
                                    app_core.config.highlights.insert(name.clone(), pattern);
                                    crate::config::Config::compile_highlight_patterns(
                                        &mut app_core.config.highlights,
                                    );
                                    app_core
                                        .message_processor
                                        .apply_config(app_core.config.clone());
                                    // Refresh browser with source tracking
                                    if let Some(ref mut browser) = self.highlight_browser {
                                        let global =
                                            crate::config::Config::load_common_highlights()
                                                .unwrap_or_default();
                                        let character =
                                            crate::config::Config::load_character_highlights_only(
                                                app_core.config.character.as_deref(),
                                            )
                                            .unwrap_or_default();
                                        browser.update_items_with_source(&global, &character);
                                    }
                                }
                                tracing::info!("Saved highlight: {} (global={})", name, is_global);
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                    InputMode::Normal
                                };
                            }
                            crate::frontend::tui::highlight_form::FormResult::Delete {
                                name,
                                ..
                            } => {
                                // Both scopes: deleting only one copy of a
                                // shadowed name uncovers the other on reload.
                                if let Err(e) = crate::config::Config::delete_highlight_everywhere(
                                    &name,
                                    app_core.config.character.as_deref(),
                                ) {
                                    app_core.add_system_message(&format!(
                                        "Failed to delete highlight: {}",
                                        e
                                    ));
                                } else {
                                    app_core.add_system_message(&format!(
                                        "Highlight '{}' deleted.",
                                        name
                                    ));
                                    // Update in-memory config
                                    app_core.config.highlights.remove(&name);
                                    crate::config::Config::compile_highlight_patterns(
                                        &mut app_core.config.highlights,
                                    );
                                    app_core
                                        .message_processor
                                        .apply_config(app_core.config.clone());
                                    // Refresh browser with source tracking
                                    if let Some(ref mut browser) = self.highlight_browser {
                                        let global =
                                            crate::config::Config::load_common_highlights()
                                                .unwrap_or_default();
                                        let character =
                                            crate::config::Config::load_character_highlights_only(
                                                app_core.config.character.as_deref(),
                                            )
                                            .unwrap_or_default();
                                        browser.update_items_with_source(&global, &character);
                                    }
                                }
                                tracing::info!("Deleted highlight '{}' from both scopes", name);
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                    InputMode::Normal
                                };
                            }
                            crate::frontend::tui::highlight_form::FormResult::Cancel => {
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                    InputMode::Normal
                                };
                            }
                        }
                    }
                }
                _ => {
                    use crate::frontend::tui::crossterm_bridge;
                    let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                    let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                    let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                    if let Some(result) = form.handle_key(key) {
                        match result {
                            crate::frontend::tui::highlight_form::FormResult::Save {
                                name,
                                mut pattern,
                                is_global,
                            } => {
                                // Resolve palette color names to hex codes
                                if let Some(ref fg) = pattern.fg {
                                    pattern.fg = Some(app_core.config.resolve_palette_color(fg));
                                }
                                if let Some(ref bg) = pattern.bg {
                                    pattern.bg = Some(app_core.config.resolve_palette_color(bg));
                                }

                                // Save to appropriate file based on scope
                                if let Err(e) = crate::config::Config::save_single_highlight(
                                    &name,
                                    &pattern,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    app_core.add_system_message(&format!(
                                        "Failed to save highlight: {}",
                                        e
                                    ));
                                } else {
                                    let scope = if is_global { "global" } else { "character" };
                                    app_core.add_system_message(&format!(
                                        "Highlight saved to {} config",
                                        scope
                                    ));
                                    // Update in-memory config
                                    app_core.config.highlights.insert(name.clone(), pattern);
                                    crate::config::Config::compile_highlight_patterns(
                                        &mut app_core.config.highlights,
                                    );
                                    app_core
                                        .message_processor
                                        .apply_config(app_core.config.clone());
                                    // Refresh browser with source tracking
                                    if let Some(ref mut browser) = self.highlight_browser {
                                        let global =
                                            crate::config::Config::load_common_highlights()
                                                .unwrap_or_default();
                                        let character =
                                            crate::config::Config::load_character_highlights_only(
                                                app_core.config.character.as_deref(),
                                            )
                                            .unwrap_or_default();
                                        browser.update_items_with_source(&global, &character);
                                    }
                                }
                                tracing::info!("Saved highlight: {} (global={})", name, is_global);
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                    InputMode::Normal
                                };
                            }
                            crate::frontend::tui::highlight_form::FormResult::Delete {
                                name,
                                ..
                            } => {
                                // Both scopes: deleting only one copy of a
                                // shadowed name uncovers the other on reload.
                                if let Err(e) = crate::config::Config::delete_highlight_everywhere(
                                    &name,
                                    app_core.config.character.as_deref(),
                                ) {
                                    app_core.add_system_message(&format!(
                                        "Failed to delete highlight: {}",
                                        e
                                    ));
                                } else {
                                    app_core.add_system_message(&format!(
                                        "Highlight '{}' deleted.",
                                        name
                                    ));
                                    // Update in-memory config
                                    app_core.config.highlights.remove(&name);
                                    crate::config::Config::compile_highlight_patterns(
                                        &mut app_core.config.highlights,
                                    );
                                    app_core
                                        .message_processor
                                        .apply_config(app_core.config.clone());
                                    // Refresh browser with source tracking
                                    if let Some(ref mut browser) = self.highlight_browser {
                                        let global =
                                            crate::config::Config::load_common_highlights()
                                                .unwrap_or_default();
                                        let character =
                                            crate::config::Config::load_character_highlights_only(
                                                app_core.config.character.as_deref(),
                                            )
                                            .unwrap_or_default();
                                        browser.update_items_with_source(&global, &character);
                                    }
                                }
                                tracing::info!("Deleted highlight '{}' from both scopes", name);
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                    InputMode::Normal
                                };
                            }
                            crate::frontend::tui::highlight_form::FormResult::Cancel => {
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                    InputMode::Normal
                                };
                            }
                        }
                    }
                }
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_keybind_form_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::input::KeyCode;
        use crate::data::input::KeyModifiers;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut form) = self.keybind_form {
            use crate::frontend::tui::keybind_form::ActionSection;
            use crate::frontend::tui::widget_traits::{FieldNavigable, TextEditable, Toggleable};

            let ctrl_only = matches!(
                modifiers,
                KeyModifiers {
                    ctrl: true,
                    shift: false,
                    alt: false
                }
            );

            if ctrl_only {
                match code {
                    KeyCode::Char('1') => {
                        form.go_to_section(ActionSection::CommandInput);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    KeyCode::Char('2') => {
                        form.go_to_section(ActionSection::CommandHistory);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    KeyCode::Char('3') => {
                        form.go_to_section(ActionSection::WindowScrolling);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    KeyCode::Char('4') => {
                        form.go_to_section(ActionSection::TabNavigation);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    KeyCode::Char('5') => {
                        form.go_to_section(ActionSection::Search);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    KeyCode::Char('6') => {
                        form.go_to_section(ActionSection::Clipboard);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    KeyCode::Char('7') => {
                        form.go_to_section(ActionSection::TTS);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    KeyCode::Char('8') => {
                        form.go_to_section(ActionSection::SystemToggles);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        form.go_to_section(ActionSection::Meta);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    _ => {}
                }
            }

            // Section navigation removed - not applicable to simple form widget
            // (This code was likely meant for KeybindBrowser, not KeybindForm)

            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextField => form.next_field(),
                crate::core::menu_actions::MenuAction::PreviousField => form.previous_field(),
                crate::core::menu_actions::MenuAction::SelectAll => form.select_all(),
                crate::core::menu_actions::MenuAction::Copy => {
                    let _ = form.copy_to_clipboard();
                }
                crate::core::menu_actions::MenuAction::Cut => {
                    let _ = form.cut_to_clipboard();
                }
                crate::core::menu_actions::MenuAction::Paste => {
                    let _ = form.paste_from_clipboard();
                }
                crate::core::menu_actions::MenuAction::Toggle => {
                    form.toggle_focused();
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.keybind_form = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                // Route navigation, cycling, select, save, and delete through handle_action
                crate::core::menu_actions::MenuAction::NavigateUp
                | crate::core::menu_actions::MenuAction::NavigateDown
                | crate::core::menu_actions::MenuAction::CycleBackward
                | crate::core::menu_actions::MenuAction::CycleForward
                | crate::core::menu_actions::MenuAction::Select
                | crate::core::menu_actions::MenuAction::Save
                | crate::core::menu_actions::MenuAction::Delete => {
                    if let Some(result) = form.handle_action(action.clone()) {
                        match result {
                            crate::frontend::tui::keybind_form::KeybindFormResult::Save {
                                key_combo,
                                action_type,
                                value,
                                is_global,
                            } => {
                                use crate::frontend::tui::keybind_form::KeybindActionType;
                                let action = match action_type {
                                    KeybindActionType::Action => {
                                        crate::config::KeyBindAction::Action(value)
                                    }
                                    KeybindActionType::Macro => {
                                        crate::config::KeyBindAction::Macro(
                                            crate::config::MacroAction { macro_text: value },
                                        )
                                    }
                                };
                                // Save to correct file based on scope
                                if let Err(e) = crate::config::Config::save_single_keybind(
                                    &key_combo,
                                    &action,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    tracing::error!("Failed to save keybind to file: {}", e);
                                }
                                // Also update in-memory config
                                app_core.config.keybinds.insert(key_combo.clone(), action);
                                app_core.rebuild_keybind_map();
                                self.keybind_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                tracing::info!(
                                    "Saved keybind: {} (global={})",
                                    key_combo,
                                    is_global
                                );
                            }
                            crate::frontend::tui::keybind_form::KeybindFormResult::Delete {
                                key_combo,
                                is_global,
                            } => {
                                // Delete from correct file based on scope
                                if let Err(e) = crate::config::Config::delete_single_keybind(
                                    &key_combo,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    tracing::error!("Failed to delete keybind from file: {}", e);
                                }
                                // Also update in-memory config
                                app_core.config.keybinds.remove(&key_combo);
                                app_core.rebuild_keybind_map();
                                self.keybind_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                tracing::info!(
                                    "Deleted keybind: {} (global={})",
                                    key_combo,
                                    is_global
                                );
                            }
                            crate::frontend::tui::keybind_form::KeybindFormResult::Cancel => {
                                self.keybind_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                            }
                        }
                    }
                }
                _ => {
                    use crate::frontend::tui::crossterm_bridge;
                    let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                    let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                    let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                    if let Some(result) = form.handle_key(key) {
                        match result {
                            crate::frontend::tui::keybind_form::KeybindFormResult::Save {
                                key_combo,
                                action_type,
                                value,
                                is_global,
                            } => {
                                use crate::frontend::tui::keybind_form::KeybindActionType;
                                let action = match action_type {
                                    KeybindActionType::Action => {
                                        crate::config::KeyBindAction::Action(value)
                                    }
                                    KeybindActionType::Macro => {
                                        crate::config::KeyBindAction::Macro(
                                            crate::config::MacroAction { macro_text: value },
                                        )
                                    }
                                };
                                // Save to correct file based on scope
                                if let Err(e) = crate::config::Config::save_single_keybind(
                                    &key_combo,
                                    &action,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    tracing::error!("Failed to save keybind to file: {}", e);
                                }
                                // Also update in-memory config
                                app_core.config.keybinds.insert(key_combo.clone(), action);
                                app_core.rebuild_keybind_map();
                                self.keybind_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                tracing::info!(
                                    "Saved keybind: {} (global={})",
                                    key_combo,
                                    is_global
                                );
                            }
                            crate::frontend::tui::keybind_form::KeybindFormResult::Delete {
                                key_combo,
                                is_global,
                            } => {
                                // Delete from correct file based on scope
                                if let Err(e) = crate::config::Config::delete_single_keybind(
                                    &key_combo,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    tracing::error!("Failed to delete keybind from file: {}", e);
                                }
                                // Also update in-memory config
                                app_core.config.keybinds.remove(&key_combo);
                                app_core.rebuild_keybind_map();
                                self.keybind_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                tracing::info!(
                                    "Deleted keybind: {} (global={})",
                                    key_combo,
                                    is_global
                                );
                            }
                            crate::frontend::tui::keybind_form::KeybindFormResult::Cancel => {
                                self.keybind_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                            }
                        }
                    }
                }
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_color_form_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut form) = self.color_form {
            use crate::frontend::tui::widget_traits::{FieldNavigable, TextEditable, Toggleable};
            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextField => form.next_field(),
                crate::core::menu_actions::MenuAction::PreviousField => form.previous_field(),
                crate::core::menu_actions::MenuAction::SelectAll => form.select_all(),
                crate::core::menu_actions::MenuAction::Copy => {
                    let _ = form.copy_to_clipboard();
                }
                crate::core::menu_actions::MenuAction::Cut => {
                    let _ = form.cut_to_clipboard();
                }
                crate::core::menu_actions::MenuAction::Paste => {
                    let _ = form.paste_from_clipboard();
                }
                crate::core::menu_actions::MenuAction::Toggle => {
                    form.toggle_focused();
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.color_form = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                crate::core::menu_actions::MenuAction::Select
                | crate::core::menu_actions::MenuAction::Save => {
                    // Handle Select (Enter) and Save (Ctrl+S) via handle_action
                    if let Some(result) = form.handle_action(action.clone()) {
                        match result {
                            crate::frontend::tui::color_form::FormAction::Save {
                                color,
                                original_name,
                                is_global,
                            } => {
                                // Save to appropriate file based on scope
                                let color_with_slot = super::input::auto_assign_slot(
                                    color.clone(),
                                    &app_core.config.colors.color_palette,
                                );
                                if let Err(e) =
                                    crate::config::ColorConfig::save_single_palette_color(
                                        &color_with_slot,
                                        is_global,
                                        app_core.config.character.as_deref(),
                                    )
                                {
                                    tracing::error!("Failed to save color: {}", e);
                                }

                                // Handle rename: delete old name if changed
                                if let Some(ref old_name) = original_name {
                                    if old_name != &color.name {
                                        let _ =
                                            crate::config::ColorConfig::delete_single_palette_color(
                                                old_name,
                                                is_global,
                                                app_core.config.character.as_deref(),
                                            );
                                    }
                                }

                                // Reload colors to update in-memory state
                                if let Ok(colors) = crate::config::ColorConfig::load_with_merge(
                                    app_core.config.character.as_deref(),
                                ) {
                                    app_core.config.colors = colors;
                                }

                                // Refresh browser if open
                                if let Some(ref mut browser) = self.color_palette_browser {
                                    let global_colors =
                                        crate::config::ColorConfig::load_common_colors()
                                            .map(|c| c.color_palette)
                                            .unwrap_or_default();
                                    let char_colors =
                                        crate::config::ColorConfig::load_character_colors_only(
                                            app_core.config.character.as_deref(),
                                        )
                                        .map(|c| c.color_palette)
                                        .unwrap_or_default();
                                    browser.update_items_with_source(&global_colors, &char_colors);
                                }

                                self.color_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                tracing::info!(
                                    "Saved color: {} ({})",
                                    color.name,
                                    if is_global { "global" } else { "character" }
                                );
                            }
                            crate::frontend::tui::color_form::FormAction::Delete => {
                                self.color_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                            }
                            crate::frontend::tui::color_form::FormAction::Cancel => {
                                self.color_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                            }
                            crate::frontend::tui::color_form::FormAction::Error(_) => {}
                        }
                    }
                }
                _ => {
                    use crate::frontend::tui::crossterm_bridge;
                    let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                    let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                    let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                    if let Some(result) = form.handle_input(key) {
                        match result {
                            crate::frontend::tui::color_form::FormAction::Save {
                                color,
                                original_name,
                                is_global,
                            } => {
                                // Save to appropriate file based on scope
                                let color_with_slot = super::input::auto_assign_slot(
                                    color.clone(),
                                    &app_core.config.colors.color_palette,
                                );
                                if let Err(e) =
                                    crate::config::ColorConfig::save_single_palette_color(
                                        &color_with_slot,
                                        is_global,
                                        app_core.config.character.as_deref(),
                                    )
                                {
                                    tracing::error!("Failed to save color: {}", e);
                                }

                                // Handle rename: delete old name if changed
                                if let Some(ref old_name) = original_name {
                                    if old_name != &color.name {
                                        let _ =
                                            crate::config::ColorConfig::delete_single_palette_color(
                                                old_name,
                                                is_global,
                                                app_core.config.character.as_deref(),
                                            );
                                    }
                                }

                                // Reload colors to update in-memory state
                                if let Ok(colors) = crate::config::ColorConfig::load_with_merge(
                                    app_core.config.character.as_deref(),
                                ) {
                                    app_core.config.colors = colors;
                                }

                                // Refresh browser if open
                                if let Some(ref mut browser) = self.color_palette_browser {
                                    let global_colors =
                                        crate::config::ColorConfig::load_common_colors()
                                            .map(|c| c.color_palette)
                                            .unwrap_or_default();
                                    let char_colors =
                                        crate::config::ColorConfig::load_character_colors_only(
                                            app_core.config.character.as_deref(),
                                        )
                                        .map(|c| c.color_palette)
                                        .unwrap_or_default();
                                    browser.update_items_with_source(&global_colors, &char_colors);
                                }

                                self.color_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                tracing::info!(
                                    "Saved color: {} ({})",
                                    color.name,
                                    if is_global { "global" } else { "character" }
                                );
                            }
                            crate::frontend::tui::color_form::FormAction::Delete => {
                                self.color_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                            }
                            crate::frontend::tui::color_form::FormAction::Cancel => {
                                self.color_form = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                            }
                            crate::frontend::tui::color_form::FormAction::Error(_) => {}
                        }
                    }
                }
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_spell_color_form_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut form) = self.spell_color_form {
            use crate::frontend::tui::widget_traits::{FieldNavigable, TextEditable};
            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextField => form.next_field(),
                crate::core::menu_actions::MenuAction::PreviousField => form.previous_field(),
                crate::core::menu_actions::MenuAction::SelectAll => form.select_all(),
                crate::core::menu_actions::MenuAction::Copy => {
                    let _ = form.copy_to_clipboard();
                }
                crate::core::menu_actions::MenuAction::Cut => {
                    let _ = form.cut_to_clipboard();
                }
                crate::core::menu_actions::MenuAction::Paste => {
                    let _ = form.paste_from_clipboard();
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.spell_color_form = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                // Route navigation, select, save, and delete through handle_action
                crate::core::menu_actions::MenuAction::NavigateUp
                | crate::core::menu_actions::MenuAction::NavigateDown
                | crate::core::menu_actions::MenuAction::Select
                | crate::core::menu_actions::MenuAction::Save
                | crate::core::menu_actions::MenuAction::Delete => {
                    if let Some(result) = form.handle_action(action.clone()) {
                        self.handle_spell_color_form_result(app_core, result);
                    }
                }
                _ => {
                    use crate::frontend::tui::crossterm_bridge;
                    let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                    let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                    let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                    if let Some(result) = form.input(key) {
                        self.handle_spell_color_form_result(app_core, result);
                    }
                }
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_menu_keybind_editor_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut editor) = self.menu_keybind_editor {
            // While capturing, the next key press IS the binding — take
            // it verbatim (including Esc/Enter, which are valid menu
            // keys) rather than routing it as a menu action.
            if editor.is_capturing() {
                let key_event = crate::data::input::KeyEvent { code, modifiers };
                let key = crate::core::menu_actions::key_event_to_string(key_event);
                editor.apply_captured_key(key);
                app_core.needs_render = true;
                return Ok(None);
            }

            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );
            match action {
                crate::core::menu_actions::MenuAction::NavigateUp
                | crate::core::menu_actions::MenuAction::PreviousItem => editor.navigate_up(),
                crate::core::menu_actions::MenuAction::NavigateDown
                | crate::core::menu_actions::MenuAction::NextItem => editor.navigate_down(),
                crate::core::menu_actions::MenuAction::Select
                | crate::core::menu_actions::MenuAction::Edit => editor.arm_capture(),
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.menu_keybind_editor = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                crate::core::menu_actions::MenuAction::Save => {
                    // Validate, then persist to the chosen scope.
                    let validation = crate::config::menu_keybind_validator::validate_menu_keybinds(
                        &editor.working,
                    );
                    if validation.has_errors() {
                        let first = validation
                            .errors()
                            .first()
                            .map(|e| e.message())
                            .unwrap_or_default();
                        editor.set_status(format!("Cannot save: {}", first));
                    } else {
                        let character = app_core.config.character.clone();
                        match crate::config::Config::save_menu_keybinds(
                            &editor.working,
                            editor.is_global,
                            character.as_deref(),
                        ) {
                            Ok(()) => {
                                app_core.config.menu_keybinds = editor.working.clone();
                                self.menu_keybind_editor = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                app_core.add_system_message("Menu keybinds saved.");
                            }
                            Err(err) => editor.set_status(format!("Save failed: {}", err)),
                        }
                    }
                }
                _ => {
                    // Plain 'd' resets the selected row to default; 'g'
                    // toggles scope. These aren't menu-nav actions, so
                    // they fall through to here.
                    if modifiers.is_empty() {
                        match code {
                            crate::data::input::KeyCode::Char('d')
                            | crate::data::input::KeyCode::Char('D') => {
                                editor.reset_selected_to_default()
                            }
                            crate::data::input::KeyCode::Char('g')
                            | crate::data::input::KeyCode::Char('G') => editor.toggle_scope(),
                            _ => {}
                        }
                    }
                }
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    /// Status abbreviation editor keys. This edits text cells, so plain
    /// character keys type into the focused cell; commands use non-char keys
    /// to avoid colliding with typing (Ctrl+A add, Ctrl+D delete, Tab column,
    /// Up/Down navigate, Ctrl+S save, Esc cancel).
    pub(super) fn handle_status_abbrev_editor_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::input::KeyCode;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut editor) = self.status_abbrev_editor {
            let ctrl = modifiers.contains_ctrl();
            match code {
                KeyCode::Esc => {
                    self.status_abbrev_editor = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                KeyCode::Char('s') | KeyCode::Char('S') if ctrl => {
                    app_core.config.target_list.status_abbrev = editor.to_map();
                    match app_core.save_config() {
                        Ok(()) => {
                            self.status_abbrev_editor = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                            app_core.add_system_message("Status abbreviations saved.");
                        }
                        Err(err) => editor.set_status(format!("Save failed: {}", err)),
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') if ctrl => editor.add_row(),
                KeyCode::Char('d') | KeyCode::Char('D') if ctrl => editor.delete_row(),
                KeyCode::Up => editor.navigate_up(),
                KeyCode::Down => editor.navigate_down(),
                KeyCode::Tab | KeyCode::BackTab => editor.toggle_column(),
                KeyCode::Backspace => editor.backspace(),
                KeyCode::Char(c) if !ctrl => editor.insert_char(c),
                _ => {}
            }
            app_core.needs_render = true;
        }
        Ok(None)
    }

    pub(super) fn handle_theme_editor_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::ui_state::InputMode;
        if let Some(ref mut editor) = self.theme_editor {
            use crate::core::input_router;

            // Ctrl+1-6 section jumping (high priority)
            if modifiers.ctrl {
                match code {
                    crate::data::input::KeyCode::Char(c @ '1'..='6') => {
                        let section =
                            c.to_digit(10).expect("char '1'..='6' is always a digit") as usize;
                        editor.jump_to_section(section);
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    _ => {}
                }
            }

            let key_event = crate::data::input::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextField => {
                    editor.next_field();
                    app_core.needs_render = true;
                }
                crate::core::menu_actions::MenuAction::PreviousField => {
                    editor.previous_field();
                    app_core.needs_render = true;
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    self.theme_editor = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                    app_core.needs_render = true;
                }
                // Route navigation and save through handle_action
                crate::core::menu_actions::MenuAction::NavigateUp
                | crate::core::menu_actions::MenuAction::NavigateDown
                | crate::core::menu_actions::MenuAction::Save => {
                    if let Some(result) = editor.handle_action(action.clone()) {
                        match result {
                            crate::frontend::tui::theme_editor::ThemeEditorResult::Save(
                                mut theme_data,
                            ) => {
                                // Resolve palette color names to hex codes
                                theme_data.resolve_palette_colors(&app_core.config);

                                match theme_data.save_to_file(app_core.config.character.as_deref())
                                {
                                    Ok(path) => {
                                        tracing::info!(
                                            "Saved custom theme '{}' to {:?}",
                                            theme_data.name,
                                            path
                                        );
                                        app_core.add_system_message(&format!(
                                            "Saved custom theme: {}",
                                            theme_data.name
                                        ));

                                        if let Some(_app_theme) = theme_data.to_app_theme() {
                                            app_core.config.active_theme = theme_data.name.clone();
                                            let theme = app_core.config.get_theme();
                                            self.update_theme_cache(theme_data.name.clone(), theme);
                                            app_core.needs_render = true;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to save custom theme: {}", e);
                                        app_core.add_system_message(&format!(
                                            "Error saving theme: {}",
                                            e
                                        ));
                                    }
                                }
                                self.theme_editor = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                            }
                            crate::frontend::tui::theme_editor::ThemeEditorResult::Cancel => {
                                self.theme_editor = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                            }
                        }
                    }
                }
                _ => {
                    use crate::frontend::tui::crossterm_bridge;
                    let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                    let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                    let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                    let _ = editor.handle_input(key);
                    app_core.needs_render = true;
                }
            }
        }
        Ok(None)
    }

    pub(super) fn handle_indicator_template_editor_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::ui_state::InputMode;
        if let Some(ref mut editor) = self.indicator_template_editor {
            let result = editor.handle_key(code, modifiers);
            app_core.needs_render = true;
            if matches!(
                result,
                crate::frontend::tui::indicator_template_editor::EditorAction::Close
            ) {
                self.indicator_template_editor = None;
                app_core.ui_state.input_mode = InputMode::Normal;
                // The editor may have saved the store; refresh the
                // shared render cache so resolution stays coherent.
                app_core.refresh_indicator_templates();
            }
        } else {
            app_core.ui_state.input_mode = InputMode::Normal;
        }
        Ok(None)
    }

    pub(super) fn handle_window_editor_mouse(
        &mut self,
        mouse_event: &crate::data::input::MouseEvent,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

        let kind = &mouse_event.kind;
        let x = &mouse_event.column;
        let y = &mouse_event.row;

        let (width, height) = self.size();
        let area = ratatui::layout::Rect {
            x: 0,
            y: 0,
            width,
            height,
        };

        if let Some(ref mut window_editor) = self.window_editor {
            use crate::frontend::tui::window_editor::WindowEditorMouseAction;

            let action = match kind {
                MouseEventKind::Down(crate::data::input::MouseButton::Left) => {
                    let action = window_editor.handle_mouse(*x, *y, true, area);
                    app_core.needs_render = true;
                    action
                }
                MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    let action = window_editor.handle_mouse(*x, *y, true, area);
                    app_core.needs_render = true;
                    action
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                    let action = window_editor.handle_mouse(*x, *y, false, area);
                    app_core.needs_render = true;
                    action
                }
                _ => WindowEditorMouseAction::None,
            };

            // Handle Save/Cancel actions from mouse clicks
            match action {
                WindowEditorMouseAction::Save => {
                    // Trigger save via simulated Ctrl+S key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_window_editor_keys(
                        KeyCode::Char('s'),
                        KeyModifiers::CTRL,
                        app_core,
                    );
                    return Ok(Some((true, None)));
                }
                WindowEditorMouseAction::Cancel => {
                    // Trigger cancel via simulated Esc key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ =
                        self.handle_window_editor_keys(KeyCode::Esc, KeyModifiers::NONE, app_core);
                    return Ok(Some((true, None)));
                }
                WindowEditorMouseAction::None => {
                    return Ok(Some((true, None)));
                }
            }
        }

        Ok(None)
    }

    pub(super) fn handle_spell_color_form_mouse(
        &mut self,
        mouse_event: &crate::data::input::MouseEvent,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

        let kind = &mouse_event.kind;
        let x = &mouse_event.column;
        let y = &mouse_event.row;

        let (width, height) = self.size();
        let area = ratatui::layout::Rect {
            x: 0,
            y: 0,
            width,
            height,
        };
        if let Some(ref mut form) = self.spell_color_form {
            match kind {
                MouseEventKind::Down(crate::data::input::MouseButton::Left)
                | MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(*x, *y, true, area)
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(*x, *y, false, area)
                }
                _ => {}
            }
        }
        app_core.needs_render = true;
        Ok(Some((true, None)))
    }

    pub(super) fn handle_spell_color_browser_mouse(
        &mut self,
        mouse_event: &crate::data::input::MouseEvent,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

        let kind = &mouse_event.kind;
        let x = &mouse_event.column;
        let y = &mouse_event.row;

        let (width, height) = self.size();
        let area = ratatui::layout::Rect {
            x: 0,
            y: 0,
            width,
            height,
        };
        if let Some(ref mut browser) = self.spell_color_browser {
            let scroll: i8 = match kind {
                MouseEventKind::ScrollUp => -1,
                MouseEventKind::ScrollDown => 1,
                _ => 0,
            };
            match kind {
                MouseEventKind::Down(crate::data::input::MouseButton::Left)
                | MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(*x, *y, true, scroll, area)
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown => browser.handle_mouse(*x, *y, false, scroll, area),
                _ => {}
            }
        }
        app_core.needs_render = true;
        Ok(Some((true, None)))
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
