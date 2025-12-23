use super::*;

impl TuiFrontend {
    pub fn ensure_command_input_exists(&mut self, window_name: &str) {
        if !self.widget_manager.command_inputs.contains_key(window_name) {
            let mut cmd_input = command_input::CommandInput::new(1000);
            cmd_input.set_title("Command".to_string());
            self.widget_manager.command_inputs
                .insert(window_name.to_string(), cmd_input);
            tracing::debug!("Created CommandInput widget for '{}'", window_name);
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
                KeyCode::Delete => cmd_input.delete_word(), // Delete forward is delete word
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
                KeyCode::Home => cmd_input.move_cursor_home(modifiers.contains(KeyModifiers::SHIFT)),
                KeyCode::End => cmd_input.move_cursor_end(modifiers.contains(KeyModifiers::SHIFT)),
                KeyCode::Up => cmd_input.history_previous(),
                KeyCode::Down => cmd_input.history_next(),
                KeyCode::Tab => {
                    // Tab completion for commands and window names
                    cmd_input.try_complete(available_commands, available_window_names);
                }
                _ => {}
            }
        }
    }

    /// Submit command from command input and return the command string
    pub fn command_input_submit(&mut self, window_name: &str) -> Option<String> {
        self.widget_manager.command_inputs.get_mut(window_name)?.submit()
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
