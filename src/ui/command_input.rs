use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use std::collections::VecDeque;

pub struct CommandInput {
    input: String,
    cursor_pos: usize,
    history: VecDeque<String>,
    history_index: Option<usize>,
    max_history: usize,
}

impl CommandInput {
    pub fn new(max_history: usize) -> Self {
        Self {
            input: String::new(),
            cursor_pos: 0,
            history: VecDeque::with_capacity(max_history),
            history_index: None,
            max_history,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        // Find the byte index for cursor position
        let byte_idx = self.char_pos_to_byte_idx(self.cursor_pos);
        self.input.insert(byte_idx, c);
        self.cursor_pos += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            let byte_idx = self.char_pos_to_byte_idx(self.cursor_pos - 1);
            self.input.remove(byte_idx);
            self.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        let char_count = self.input.chars().count();
        if self.cursor_pos < char_count {
            self.cursor_pos += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor_pos = self.input.chars().count();
    }

    pub fn move_cursor_word_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }

        let chars: Vec<char> = self.input.chars().collect();
        let mut pos = self.cursor_pos;

        // Skip spaces to the left
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        // Skip word characters to the left
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        self.cursor_pos = pos;
    }

    pub fn move_cursor_word_right(&mut self) {
        let chars: Vec<char> = self.input.chars().collect();
        let char_count = chars.len();

        if self.cursor_pos >= char_count {
            return;
        }

        let mut pos = self.cursor_pos;

        // Skip word characters to the right
        while pos < char_count && !chars[pos].is_whitespace() {
            pos += 1;
        }

        // Skip spaces to the right
        while pos < char_count && chars[pos].is_whitespace() {
            pos += 1;
        }

        self.cursor_pos = pos;
    }

    pub fn delete_word(&mut self) {
        // Delete from cursor to end of current word
        let chars: Vec<char> = self.input.chars().collect();
        let char_count = chars.len();

        if self.cursor_pos >= char_count {
            return;
        }

        let mut end_pos = self.cursor_pos;

        // Skip word characters
        while end_pos < char_count && !chars[end_pos].is_whitespace() {
            end_pos += 1;
        }

        // Convert positions to byte indices
        let start_byte = self.char_pos_to_byte_idx(self.cursor_pos);
        let end_byte = self.char_pos_to_byte_idx(end_pos);

        self.input.drain(start_byte..end_byte);
    }

    /// Convert character position to byte index
    fn char_pos_to_byte_idx(&self, char_pos: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_pos)
            .map(|(idx, _)| idx)
            .unwrap_or(self.input.len())
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor_pos = 0;
        self.history_index = None;
    }

    pub fn get_input(&self) -> Option<String> {
        if self.input.is_empty() {
            None
        } else {
            Some(self.input.clone())
        }
    }

    pub fn get_last_command(&self) -> Option<String> {
        self.history.get(0).cloned()
    }

    pub fn get_second_last_command(&self) -> Option<String> {
        self.history.get(1).cloned()
    }

    pub fn submit(&mut self) -> Option<String> {
        if self.input.is_empty() {
            return None;
        }

        let command = self.input.clone();

        // Add to history
        self.history.push_front(command.clone());
        if self.history.len() > self.max_history {
            self.history.pop_back();
        }

        self.clear();
        Some(command)
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
            self.input = cmd.clone();
            self.cursor_pos = self.input.chars().count();
            self.history_index = Some(new_index);
        }
    }

    pub fn history_next(&mut self) {
        match self.history_index {
            None => {}
            Some(0) => {
                self.input.clear();
                self.cursor_pos = 0;
                self.history_index = None;
            }
            Some(idx) => {
                let new_index = idx - 1;
                if let Some(cmd) = self.history.get(new_index) {
                    self.input = cmd.clone();
                    self.cursor_pos = self.input.chars().count();
                    self.history_index = Some(new_index);
                }
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_with_status(area, buf, None);
    }

    pub fn render_with_status(&self, area: Rect, buf: &mut Buffer, status: Option<&str>) {
        let title = if let Some(status_text) = status {
            format!("Command [{}]", status_text)
        } else {
            "Command".to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title);

        let inner = block.inner(area);
        block.render(area, buf);

        // Create line with cursor
        // cursor_pos is now a character position, not byte index
        let chars: Vec<char> = self.input.chars().collect();

        let before_cursor: String = chars.iter().take(self.cursor_pos).collect();
        let cursor_char = chars.get(self.cursor_pos).copied().unwrap_or(' ');
        let after_cursor: String = chars.iter().skip(self.cursor_pos + 1).collect();

        let line = Line::from(vec![
            Span::raw(before_cursor),
            Span::styled(
                cursor_char.to_string(),
                Style::default().bg(Color::White).fg(Color::Black),
            ),
            Span::raw(after_cursor),
        ]);

        let paragraph = Paragraph::new(line);
        paragraph.render(inner, buf);
    }
}
