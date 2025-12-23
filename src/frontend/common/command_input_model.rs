use std::collections::VecDeque;

#[derive(Clone, Debug)]
struct CommandInputSnapshot {
    text: String,
    cursor_pos: usize,
    selection: Option<(usize, usize)>,
}

/// Frontend-agnostic command input state and editing logic.
#[derive(Clone, Debug)]
pub struct CommandInputModel {
    text: String,
    cursor_pos: usize,
    selection_anchor: Option<usize>,
    selection: Option<(usize, usize)>,
    undo_stack: Vec<CommandInputSnapshot>,
    redo_stack: Vec<CommandInputSnapshot>,
    history: VecDeque<String>,
    history_index: Option<usize>,
    max_history: usize,
    min_command_length: usize,
    is_user_typed: bool,
    completion_candidates: Vec<String>,
    completion_index: Option<usize>,
    completion_prefix: Option<String>,
}

impl CommandInputModel {
    pub fn new(max_history: usize) -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            selection_anchor: None,
            selection: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            history: VecDeque::with_capacity(max_history),
            history_index: None,
            max_history,
            min_command_length: 3,
            is_user_typed: false,
            completion_candidates: Vec::new(),
            completion_index: None,
            completion_prefix: None,
        }
    }

    pub fn set_min_command_length(&mut self, min_length: usize) {
        self.min_command_length = min_length;
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor_pos(&self) -> usize {
        self.cursor_pos
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some()
    }

    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
        self.selection = None;
    }

    pub fn select_all(&mut self) {
        if self.text.is_empty() {
            self.clear_selection();
            return;
        }
        self.selection_anchor = Some(0);
        self.cursor_pos = self.text.chars().count();
        self.update_selection_from_anchor();
    }

    pub fn insert_char(&mut self, c: char) {
        self.insert_text(&c.to_string());
    }

    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.push_undo_snapshot();
        if self.delete_selection_internal() {
            // Selection deleted, cursor_pos already updated.
        }
        let byte_idx = self.char_pos_to_byte_idx(self.cursor_pos);
        self.text.insert_str(byte_idx, text);
        self.cursor_pos += text.chars().count();
        self.clear_selection();
        self.reset_completion();
        self.is_user_typed = true;
        self.redo_stack.clear();
    }

    pub fn delete_char(&mut self) {
        if self.selection.is_some() {
            self.delete_selection();
            return;
        }
        if self.cursor_pos == 0 {
            return;
        }
        self.push_undo_snapshot();
        let byte_idx = self.char_pos_to_byte_idx(self.cursor_pos - 1);
        self.text.remove(byte_idx);
        self.cursor_pos -= 1;
        self.reset_completion();
        self.is_user_typed = true;
        self.redo_stack.clear();
    }

    pub fn delete_word_forward(&mut self) {
        if self.selection.is_some() {
            self.delete_selection();
            return;
        }
        let chars: Vec<char> = self.text.chars().collect();
        let char_count = chars.len();
        if self.cursor_pos >= char_count {
            return;
        }
        self.push_undo_snapshot();
        let mut end_pos = self.cursor_pos;
        while end_pos < char_count && !chars[end_pos].is_whitespace() {
            end_pos += 1;
        }
        let start_byte = self.char_pos_to_byte_idx(self.cursor_pos);
        let end_byte = self.char_pos_to_byte_idx(end_pos);
        self.text.drain(start_byte..end_byte);
        self.reset_completion();
        self.is_user_typed = true;
        self.redo_stack.clear();
    }

    pub fn delete_word_backward(&mut self) {
        if self.selection.is_some() {
            self.delete_selection();
            return;
        }
        if self.cursor_pos == 0 {
            return;
        }
        self.push_undo_snapshot();
        let chars: Vec<char> = self.text.chars().collect();
        let mut start_pos = self.cursor_pos;
        while start_pos > 0 && chars[start_pos - 1].is_whitespace() {
            start_pos -= 1;
        }
        while start_pos > 0 && !chars[start_pos - 1].is_whitespace() {
            start_pos -= 1;
        }
        let start_byte = self.char_pos_to_byte_idx(start_pos);
        let end_byte = self.char_pos_to_byte_idx(self.cursor_pos);
        self.text.drain(start_byte..end_byte);
        self.cursor_pos = start_pos;
        self.reset_completion();
        self.is_user_typed = true;
        self.redo_stack.clear();
    }

    pub fn move_cursor_left(&mut self, extend: bool) {
        let anchor_cursor = self.cursor_pos;
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
        self.update_selection(extend, anchor_cursor);
    }

    pub fn move_cursor_right(&mut self, extend: bool) {
        let anchor_cursor = self.cursor_pos;
        let char_count = self.text.chars().count();
        if self.cursor_pos < char_count {
            self.cursor_pos += 1;
        }
        self.update_selection(extend, anchor_cursor);
    }

    pub fn move_cursor_home(&mut self, extend: bool) {
        let anchor_cursor = self.cursor_pos;
        self.cursor_pos = 0;
        self.update_selection(extend, anchor_cursor);
    }

    pub fn move_cursor_end(&mut self, extend: bool) {
        let anchor_cursor = self.cursor_pos;
        self.cursor_pos = self.text.chars().count();
        self.update_selection(extend, anchor_cursor);
    }

    pub fn move_cursor_word_left(&mut self, extend: bool) {
        if self.cursor_pos == 0 {
            return;
        }
        let anchor_cursor = self.cursor_pos;
        let chars: Vec<char> = self.text.chars().collect();
        let mut pos = self.cursor_pos;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        self.cursor_pos = pos;
        self.update_selection(extend, anchor_cursor);
    }

    pub fn move_cursor_word_right(&mut self, extend: bool) {
        let chars: Vec<char> = self.text.chars().collect();
        let char_count = chars.len();
        if self.cursor_pos >= char_count {
            return;
        }
        let anchor_cursor = self.cursor_pos;
        let mut pos = self.cursor_pos;
        while pos < char_count && !chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < char_count && chars[pos].is_whitespace() {
            pos += 1;
        }
        self.cursor_pos = pos;
        self.update_selection(extend, anchor_cursor);
    }

    pub fn clear(&mut self) {
        if self.text.is_empty() && self.selection.is_none() {
            return;
        }
        self.push_undo_snapshot();
        self.text.clear();
        self.cursor_pos = 0;
        self.history_index = None;
        self.is_user_typed = false;
        self.clear_selection();
        self.reset_completion();
        self.redo_stack.clear();
    }

    pub fn get_input(&self) -> Option<String> {
        if self.text.is_empty() {
            None
        } else {
            Some(self.text.clone())
        }
    }

    pub fn get_selected_text(&self) -> Option<String> {
        let (start, end) = self.selection?;
        if start >= end {
            return None;
        }
        let start_byte = self.char_pos_to_byte_idx(start);
        let end_byte = self.char_pos_to_byte_idx(end);
        Some(self.text[start_byte..end_byte].to_string())
    }

    pub fn delete_selection(&mut self) -> bool {
        if self.selection.is_none() {
            return false;
        }
        self.push_undo_snapshot();
        let deleted = self.delete_selection_internal();
        if deleted {
            self.redo_stack.clear();
            self.reset_completion();
            self.is_user_typed = true;
        }
        deleted
    }

    pub fn submit(&mut self) -> Option<String> {
        if self.text.is_empty() {
            return None;
        }
        let command = self.text.clone();
        if command.len() >= self.min_command_length {
            let should_add = self
                .history
                .front()
                .map(|last_cmd| last_cmd != &command)
                .unwrap_or(true);
            if should_add {
                self.history.push_front(command.clone());
                if self.history.len() > self.max_history {
                    self.history.pop_back();
                }
            }
        }
        self.text.clear();
        self.cursor_pos = 0;
        self.history_index = None;
        self.is_user_typed = false;
        self.clear_selection();
        self.reset_completion();
        self.undo_stack.clear();
        self.redo_stack.clear();
        Some(command)
    }

    pub fn get_last_command(&self) -> Option<String> {
        self.history.front().cloned()
    }

    pub fn get_second_last_command(&self) -> Option<String> {
        self.history.get(1).cloned()
    }

    pub fn history_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let new_index = match self.history_index {
            None => 0,
            Some(idx) if idx < self.history.len() - 1 => idx + 1,
            Some(idx) => idx,
        };
        if let Some(cmd) = self.history.get(new_index) {
            self.text = cmd.clone();
            self.cursor_pos = self.text.chars().count();
            self.history_index = Some(new_index);
            self.is_user_typed = false;
            self.clear_selection();
            self.reset_completion();
            self.undo_stack.clear();
            self.redo_stack.clear();
        }
    }

    pub fn history_next(&mut self) {
        match self.history_index {
            None => {
                if self.is_user_typed && !self.text.is_empty() {
                    self.clear();
                }
            }
            Some(0) => {
                self.text.clear();
                self.cursor_pos = 0;
                self.history_index = None;
                self.is_user_typed = false;
                self.clear_selection();
                self.reset_completion();
                self.undo_stack.clear();
                self.redo_stack.clear();
            }
            Some(idx) => {
                let new_index = idx - 1;
                if let Some(cmd) = self.history.get(new_index) {
                    self.text = cmd.clone();
                    self.cursor_pos = self.text.chars().count();
                    self.history_index = Some(new_index);
                    self.is_user_typed = false;
                    self.clear_selection();
                    self.reset_completion();
                    self.undo_stack.clear();
                    self.redo_stack.clear();
                }
            }
        }
    }

    pub fn undo(&mut self) -> bool {
        let snapshot = match self.undo_stack.pop() {
            Some(snapshot) => snapshot,
            None => return false,
        };
        let current = self.snapshot();
        self.redo_stack.push(current);
        self.apply_snapshot(snapshot);
        true
    }

    pub fn redo(&mut self) -> bool {
        let snapshot = match self.redo_stack.pop() {
            Some(snapshot) => snapshot,
            None => return false,
        };
        let current = self.snapshot();
        self.undo_stack.push(current);
        self.apply_snapshot(snapshot);
        true
    }

    pub fn try_complete(&mut self, available_commands: &[String], window_names: &[String]) {
        if self.cursor_pos != self.text.chars().count() {
            return;
        }
        if self.completion_candidates.is_empty() {
            let input = self.text.trim();
            let (prefix, word_to_complete) = if let Some(pos) = input.rfind(char::is_whitespace) {
                let prefix = &input[..=pos];
                let word = &input[pos + 1..];
                (prefix.to_string(), word)
            } else {
                ("".to_string(), input)
            };

            if word_to_complete.is_empty() {
                return;
            }

            let mut candidates = Vec::new();
            if word_to_complete.starts_with('.') {
                for cmd in available_commands {
                    if cmd.starts_with(word_to_complete) {
                        candidates.push(cmd.clone());
                    }
                }
            } else {
                for name in window_names {
                    if name.starts_with(word_to_complete) {
                        candidates.push(name.clone());
                    }
                }
            }

            if candidates.is_empty() {
                return;
            }

            candidates.sort();
            self.completion_candidates = candidates;
            self.completion_prefix = Some(prefix);
            self.completion_index = Some(0);
        } else if let Some(index) = self.completion_index.as_mut() {
            *index = (*index + 1) % self.completion_candidates.len();
        }

        if let (Some(index), Some(prefix)) = (self.completion_index, &self.completion_prefix) {
            if let Some(candidate) = self.completion_candidates.get(index) {
                let prefix = prefix.clone();
                let candidate = candidate.clone();
                self.push_undo_snapshot();
                self.text = format!("{}{}", prefix, candidate);
                self.cursor_pos = self.text.chars().count();
                self.clear_selection();
                self.redo_stack.clear();
            }
        }
    }

    pub fn reset_completion(&mut self) {
        self.completion_candidates.clear();
        self.completion_index = None;
        self.completion_prefix = None;
    }

    pub fn history(&self) -> &VecDeque<String> {
        &self.history
    }

    pub fn history_mut(&mut self) -> &mut VecDeque<String> {
        &mut self.history
    }

    pub fn max_history(&self) -> usize {
        self.max_history
    }

    fn update_selection(&mut self, extend: bool, anchor_cursor: usize) {
        if extend {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(anchor_cursor);
            }
            self.update_selection_from_anchor();
        } else {
            self.clear_selection();
        }
    }

    fn update_selection_from_anchor(&mut self) {
        if let Some(anchor) = self.selection_anchor {
            if anchor == self.cursor_pos {
                self.selection = None;
            } else {
                let start = anchor.min(self.cursor_pos);
                let end = anchor.max(self.cursor_pos);
                self.selection = Some((start, end));
            }
        }
    }

    fn delete_selection_internal(&mut self) -> bool {
        let (start, end) = match self.selection {
            Some(range) => range,
            None => return false,
        };
        let start_byte = self.char_pos_to_byte_idx(start);
        let end_byte = self.char_pos_to_byte_idx(end);
        self.text.drain(start_byte..end_byte);
        self.cursor_pos = start;
        self.clear_selection();
        true
    }

    fn snapshot(&self) -> CommandInputSnapshot {
        CommandInputSnapshot {
            text: self.text.clone(),
            cursor_pos: self.cursor_pos,
            selection: self.selection,
        }
    }

    fn push_undo_snapshot(&mut self) {
        self.undo_stack.push(self.snapshot());
    }

    fn apply_snapshot(&mut self, snapshot: CommandInputSnapshot) {
        self.text = snapshot.text;
        self.cursor_pos = snapshot.cursor_pos;
        self.selection = snapshot.selection;
        self.selection_anchor = None;
        self.reset_completion();
    }

    fn char_pos_to_byte_idx(&self, char_pos: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_pos)
            .map(|(idx, _)| idx)
            .unwrap_or(self.text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::CommandInputModel;

    #[test]
    fn select_all_and_delete() {
        let mut model = CommandInputModel::new(10);
        model.insert_text("hello");
        model.select_all();
        assert!(model.has_selection());
        model.delete_selection();
        assert_eq!(model.text(), "");
        assert_eq!(model.cursor_pos(), 0);
    }

    #[test]
    fn undo_redo_basic() {
        let mut model = CommandInputModel::new(10);
        model.insert_text("hi");
        assert_eq!(model.text(), "hi");
        assert!(model.undo());
        assert_eq!(model.text(), "");
        assert!(model.redo());
        assert_eq!(model.text(), "hi");
    }

    #[test]
    fn shift_select_extends_from_anchor() {
        let mut model = CommandInputModel::new(10);
        model.insert_text("abcd");
        model.move_cursor_left(false);
        model.move_cursor_left(false);
        model.move_cursor_left(true);
        assert_eq!(model.selection_range(), Some((1, 2)));
        model.move_cursor_left(true);
        assert_eq!(model.selection_range(), Some((0, 2)));
    }

    #[test]
    fn selection_replace_is_undoable() {
        let mut model = CommandInputModel::new(10);
        model.insert_text("hello");
        model.select_all();
        model.insert_text("yo");
        assert_eq!(model.text(), "yo");
        assert!(model.undo());
        assert_eq!(model.text(), "hello");
    }

    #[test]
    fn completion_cycles_dot_commands() {
        let mut model = CommandInputModel::new(10);
        let commands = vec![".create".to_string(), ".clear".to_string()];
        let windows = Vec::new();

        model.insert_text(".c");
        model.try_complete(&commands, &windows);
        assert_eq!(model.text(), ".clear");
        model.try_complete(&commands, &windows);
        assert_eq!(model.text(), ".create");
    }

    #[test]
    fn completion_replaces_last_word() {
        let mut model = CommandInputModel::new(10);
        let commands = Vec::new();
        let windows = vec!["main".to_string(), "map".to_string()];

        model.insert_text(".window m");
        model.try_complete(&commands, &windows);
        assert_eq!(model.text(), ".window main");
    }
}
