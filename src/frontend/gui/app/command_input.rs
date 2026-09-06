//! Command-line input plumbing: submit/dispatch, persistent command
//! history, and tab completion for the command input widget.

use super::*;

impl VellumGuiApp {
    pub(super) fn submit_command(&mut self) {
        let input = std::mem::take(&mut self.command_input);
        self.record_command_history(&input);
        self.history_pos = None;
        self.history_draft.clear();
        self.dispatch_command(input);
    }

    const MAX_COMMAND_HISTORY: usize = 100;

    pub(super) fn history_path_for(character: Option<&str>) -> Option<std::path::PathBuf> {
        crate::config::Config::history_path(character).ok()
    }

    /// Load history from the shared per-profile file (newest first, same
    /// format the TUI reads and writes).
    pub(super) fn load_command_history(
        character: Option<&str>,
    ) -> std::collections::VecDeque<String> {
        let mut history = std::collections::VecDeque::new();
        let Some(path) = Self::history_path_for(character) else {
            return history;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return history;
        };
        for line in text.lines() {
            if !line.trim().is_empty() {
                history.push_back(line.to_string());
                if history.len() >= Self::MAX_COMMAND_HISTORY {
                    break;
                }
            }
        }
        history
    }

    /// Record a submitted command: min-length and consecutive-dedupe rules
    /// matching the TUI's input model, then persist.
    pub(super) fn record_command_history(&mut self, command: &str) {
        let command = command.trim_end();
        if command.is_empty() || command.len() < self.app_core.config.ui.min_command_length {
            return;
        }
        if self.command_history.front().map(String::as_str) == Some(command) {
            return;
        }
        self.command_history.push_front(command.to_string());
        self.command_history.truncate(Self::MAX_COMMAND_HISTORY);
        if let Some(path) = Self::history_path_for(self.app_core.config.character.as_deref()) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let joined: String = self
                .command_history
                .iter()
                .map(|c| format!("{c}\n"))
                .collect();
            let _ = std::fs::write(path, joined);
        }
    }

    /// Plain Tab on dot input (focused, cursor at end): advance dot-command /
    /// window-name completion, falling back to accepting the history ghost
    /// once completion has nothing new. Returns true when Tab did something
    /// (and was consumed); false lets keybind dispatch handle it.
    pub(super) fn advance_input_completion(&mut self, ctx: &egui::Context) -> bool {
        // Any text change since our last completion output invalidates the
        // candidate set (typing, history nav, submit).
        if self.command_input != self.input_completion_text {
            self.input_completion.reset();
        }

        let commands = self.app_core.get_available_commands();
        let window_names = self.app_core.get_window_names();
        if let Some(new_text) =
            self.input_completion
                .advance(&self.command_input, &commands, &window_names)
        {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
            self.command_input = new_text.clone();
            self.input_completion_text = new_text;
            self.command_cursor_to_end(ctx);
            return true;
        }

        // Completion settled — accept the ghost, if the feature is on and a
        // suggestion exists.
        if !self.app_core.config.ui.history_suggestions {
            return false;
        }
        let Some(suffix) = crate::frontend::common::find_history_completion(
            &self.command_input,
            &self.command_history,
        ) else {
            return false;
        };
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        self.command_input.push_str(&suffix);
        self.input_completion_text = self.command_input.clone();
        self.history_pos = None;
        self.history_draft.clear();
        self.command_cursor_to_end(ctx);
        true
    }

    pub(super) fn command_completion_ready(&self, ctx: &egui::Context) -> bool {
        if !self.app_core.config.ui.history_suggestions {
            return false;
        }
        if crate::frontend::common::find_history_completion(
            &self.command_input,
            &self.command_history,
        )
        .is_none()
        {
            return false;
        }

        Self::command_completion_cursor_ready(ctx, self.command_input.chars().count())
    }

    pub(super) fn command_completion_cursor_ready(ctx: &egui::Context, end: usize) -> bool {
        if !ctx.memory(|memory| memory.focused() == Some(egui::Id::new(COMMAND_INPUT_EDIT_ID))) {
            return false;
        }

        egui::TextEdit::load_state(ctx, egui::Id::new(COMMAND_INPUT_EDIT_ID))
            .and_then(|state| state.cursor.char_range())
            .is_some_and(|range| range.primary.index.0 == end && range.secondary.index.0 == end)
    }

    /// Up arrow: step back through history (stashing the in-progress text
    /// on entry).
    pub(super) fn history_previous(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        let next = match self.history_pos {
            None => {
                self.history_draft = std::mem::take(&mut self.command_input);
                0
            }
            Some(i) if i + 1 < self.command_history.len() => i + 1,
            Some(i) => i,
        };
        self.history_pos = Some(next);
        self.command_input = self.command_history[next].clone();
    }

    /// Down arrow: step toward newest; at the newest entry (or when not
    /// browsing) clear the input so it's ready for fresh typing.
    pub(super) fn history_next(&mut self) {
        match self.history_pos {
            Some(0) | None => {
                self.history_pos = None;
                self.command_input.clear();
                self.history_draft.clear();
            }
            Some(i) => {
                self.history_pos = Some(i - 1);
                self.command_input = self.command_history[i - 1].clone();
            }
        }
    }

    /// Put the caret at the end of the input after programmatic text swaps.
    pub(super) fn command_cursor_to_end(&self, ctx: &egui::Context) {
        let Some(id) = self.command_input_id else {
            return;
        };
        if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
            let ccursor = egui::text::CCursor::new(self.command_input.chars().count());
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
            state.store(ctx, id);
        }
    }

    /// Run a command through the shared core path (echo, dot-commands,
    /// quit interception). Used by the local input bar and by commands
    /// arriving from remote web clients.
    pub(super) fn dispatch_command(&mut self, command: String) {
        self.dispatch_command_from(None, command);
    }

    /// Run a browser-originated command while retaining the WebSocket
    /// connection id for addressed replies such as GOALS' LaunchURL.
    pub(super) fn dispatch_remote_command(&mut self, client_id: u64, command: String) {
        self.dispatch_command_from(Some(client_id), command);
    }

    fn dispatch_command_from(&mut self, remote_client_id: Option<u64>, command: String) {
        let command = command.trim_end().to_string();
        if command.is_empty() {
            return;
        }

        let outcome = match remote_client_id {
            Some(client_id) => self.app_core.send_remote_command(client_id, command),
            None => self.app_core.send_command(command),
        };
        match outcome {
            Ok(crate::data::CommandOutcome::Ui(action)) => self.handle_ui_action(action),
            Ok(crate::data::CommandOutcome::Handled) => {}
            Ok(crate::data::CommandOutcome::Game(outbound)) => {
                let sent = if Self::should_send_to_network(&outbound) {
                    self.app_core
                        .perf_stats
                        .record_bytes_sent((outbound.len() + 1) as u64);
                    self.command_tx.send(outbound.clone()).is_ok()
                } else {
                    false
                };
                self.app_core.finish_game_command_send(&outbound, sent);
            }
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Command error: {}", err));
            }
        }

        if !self.app_core.running {
            self.close_requested = true;
        }
    }
}
