use super::*;

impl TuiFrontend {
    pub fn ensure_command_input_exists(&mut self, window_name: &str) {
        if !self.widget_manager.command_inputs.contains_key(window_name) {
            let mut cmd_input = command_input::CommandInput::new(100);
            cmd_input.set_title("Command".to_string());
            self.widget_manager
                .command_inputs
                .insert(window_name.to_string(), cmd_input);
            tracing::debug!("Created CommandInput widget for '{}'", window_name);
        }
    }

    /// Dispatch a BOUND command-input editing action by NAME — never by
    /// re-reading the raw key, so a rebind works no matter which physical
    /// key fired it (the old path matched the raw key against hardcoded
    /// arms and only worked when the binding coincided with them).
    /// `extend` = shift was held (selection-extending cursor moves).
    pub fn apply_command_input_action(&mut self, window_name: &str, action: &str, extend: bool) {
        let Some(cmd_input) = self.widget_manager.command_inputs.get_mut(window_name) else {
            return;
        };
        match action {
            "cursor_left" => cmd_input.move_cursor_left(extend),
            "cursor_right" => cmd_input.move_cursor_right(extend),
            "cursor_word_left" => cmd_input.move_cursor_word_left(extend),
            "cursor_word_right" => cmd_input.move_cursor_word_right(extend),
            "cursor_home" => cmd_input.move_cursor_home(extend),
            "cursor_end" => cmd_input.move_cursor_end(extend),
            "cursor_backspace" => cmd_input.delete_char(),
            // Honest semantics per the action labels: Delete = one char
            // forward, Delete Word = the classic ctrl+w backward word.
            "cursor_delete" => cmd_input.delete_forward(),
            "cursor_delete_word" => cmd_input.delete_word_backward(),
            "cursor_clear_line" => cmd_input.clear(),
            "previous_command" => cmd_input.history_previous(),
            "next_command" => cmd_input.history_next(),
            "select_all" => cmd_input.select_all(),
            "copy" => {
                if let Some(selected) = cmd_input.get_selected_text() {
                    if let Err(e) = crate::clipboard::copy(&selected) {
                        tracing::warn!("Failed to copy to clipboard: {}", e);
                    }
                }
            }
            "paste" => match crate::clipboard::paste() {
                Ok(text) => cmd_input.insert_text(&text),
                Err(e) => tracing::warn!("Failed to paste from clipboard: {}", e),
            },
            _ => {}
        }
    }

    /// Handle keyboard input for command input widget
    pub fn command_input_key(
        &mut self,
        window_name: &str,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
        available_commands: &[String],
        available_window_names: &[String],
    ) {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Widget should already exist (created during init)
        if !self.widget_manager.command_inputs.contains_key(window_name) {
            tracing::warn!(
                "CommandInput widget '{}' doesn't exist, creating it now",
                window_name
            );
            self.ensure_command_input_exists(window_name);
        }

        if let Some(cmd_input) = self.widget_manager.command_inputs.get_mut(window_name) {
            match code {
                KeyCode::Char(c) => {
                    if modifiers.contains(KeyModifiers::CONTROL) {
                        match c {
                            'a' => cmd_input.select_all(),
                            'c' => {
                                if let Some(selected) = cmd_input.get_selected_text() {
                                    if let Err(e) = crate::clipboard::copy(&selected) {
                                        tracing::warn!("Failed to copy to clipboard: {}", e);
                                    }
                                }
                            }
                            'x' => {
                                if let Some(selected) = cmd_input.get_selected_text() {
                                    if let Err(e) = crate::clipboard::copy(&selected) {
                                        tracing::warn!("Failed to copy to clipboard: {}", e);
                                    } else {
                                        cmd_input.delete_selection();
                                    }
                                }
                            }
                            'v' => match crate::clipboard::paste() {
                                Ok(text) => cmd_input.insert_text(&text),
                                Err(e) => tracing::warn!("Failed to paste from clipboard: {}", e),
                            },
                            'z' => {
                                if modifiers.contains(KeyModifiers::SHIFT) {
                                    cmd_input.redo();
                                } else {
                                    cmd_input.undo();
                                }
                            }
                            'e' => cmd_input.move_cursor_end(false),
                            'u' => cmd_input.clear(),
                            'w' => cmd_input.delete_word_backward(),
                            _ => {}
                        }
                    } else {
                        cmd_input.insert_char(c);
                    }
                }
                KeyCode::Backspace => cmd_input.delete_char(),
                // Standard Delete: one char forward (delete-word lives on
                // the cursor_delete_word action / ctrl+w).
                KeyCode::Delete => cmd_input.delete_forward(),
                KeyCode::Left => {
                    let extend = modifiers.contains(KeyModifiers::SHIFT);
                    if modifiers.contains(KeyModifiers::CONTROL) {
                        cmd_input.move_cursor_word_left(extend);
                    } else {
                        cmd_input.move_cursor_left(extend);
                    }
                }
                KeyCode::Right => {
                    let extend = modifiers.contains(KeyModifiers::SHIFT);
                    if modifiers.contains(KeyModifiers::CONTROL) {
                        cmd_input.move_cursor_word_right(extend);
                    } else {
                        cmd_input.move_cursor_right(extend);
                    }
                }
                KeyCode::Home => {
                    cmd_input.move_cursor_home(modifiers.contains(KeyModifiers::SHIFT))
                }
                KeyCode::End => cmd_input.move_cursor_end(modifiers.contains(KeyModifiers::SHIFT)),
                KeyCode::Up => cmd_input.history_previous(),
                KeyCode::Down => cmd_input.history_next(),
                KeyCode::Tab => {
                    // Completion first, ghost second: Tab advances dot-command
                    // / window-name completion while it has something NEW to
                    // offer; once it's settled (or never applied), Tab accepts
                    // the inline history suggestion. `.la` Tab Tab → ".launch"
                    // → ".launch nisugi".
                    if !cmd_input.try_complete(available_commands, available_window_names) {
                        cmd_input.accept_history_completion();
                    }
                }
                _ => {}
            }
        }
    }

    /// Submit command from command input and return the command string
    pub fn command_input_submit(&mut self, window_name: &str) -> Option<String> {
        self.widget_manager
            .command_inputs
            .get_mut(window_name)?
            .submit()
    }

    /// Record a remotely-submitted command into local history so desk
    /// up-arrow reaches phone-typed commands.
    pub fn command_input_record_external(&mut self, window_name: &str, command: &str) {
        if let Some(cmd_input) = self.widget_manager.command_inputs.get_mut(window_name) {
            cmd_input.record_external_command(command);
        }
    }

    /// Load command history for a character
    pub fn command_input_load_history(
        &mut self,
        window_name: &str,
        character: Option<&str>,
    ) -> Result<()> {
        if let Some(cmd_input) = self.widget_manager.command_inputs.get_mut(window_name) {
            cmd_input.load_history(character)?;
        }
        Ok(())
    }

    /// Save command history for a character
    pub fn command_input_save_history(
        &self,
        window_name: &str,
        character: Option<&str>,
    ) -> Result<()> {
        if let Some(cmd_input) = self.widget_manager.command_inputs.get(window_name) {
            cmd_input.save_history(character)?;
        }
        Ok(())
    }
}
