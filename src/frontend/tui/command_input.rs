//! Stateful command line widget that mimics Profanity's behavior.
//!
//! Handles multi-byte cursoring, cut/copy selection, history persistence, and
//! autocomplete for both dot-commands and window names.

use crate::frontend::tui::{
    colors::parse_color_to_ratatui,
    crossterm_bridge,
    title_position::{self, TitlePosition},
};
use crate::config::BorderSides;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{BorderType, Paragraph, Widget},
};
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Write as _};
use std::path::PathBuf;

pub struct CommandInput {
    input: String,
    cursor_pos: usize,
    history: VecDeque<String>,
    history_index: Option<usize>,
    max_history: usize,
    min_command_length: usize, // Minimum command length to save to history
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    show_title: bool,
    border_sides: BorderSides,
    title: String,
    title_position: TitlePosition,
    background_color: Option<String>,
    text_color: Option<String>,         // Input text color
    cursor_fg_color: Option<String>,    // Cursor foreground color
    cursor_bg_color: Option<String>,    // Cursor background color
    prompt_icon: Option<String>,        // Optional prompt icon shown before input
    prompt_icon_color: Option<String>,  // Color for prompt icon
    completion_candidates: Vec<String>, // Current completion candidates
    completion_index: Option<usize>,    // Index of current completion
    completion_prefix: Option<String>,  // Original text before completion started
    is_user_typed: bool,                // True if current text was typed by user (not from history)
    selection_start: Option<usize>,     // Start of selection (None if no selection)
}

impl CommandInput {
    pub fn new(max_history: usize) -> Self {
        Self {
            input: String::new(),
            cursor_pos: 0,
            history: VecDeque::with_capacity(max_history),
            history_index: None,
            max_history,
            min_command_length: 3, // Default to 3 characters
            show_border: true,
            border_style: None,
            border_color: None,
            show_title: true,
            border_sides: BorderSides::default(),
            title: "Command".to_string(),
            title_position: TitlePosition::TopLeft,
            background_color: None,
            text_color: None,      // Will use global default
            cursor_fg_color: None, // Default: black
            cursor_bg_color: None, // Default: white
            prompt_icon: None,
            prompt_icon_color: None,
            completion_candidates: Vec::new(),
            completion_index: None,
            completion_prefix: None,
            is_user_typed: false,
            selection_start: None,
        }
    }

    pub fn set_min_command_length(&mut self, min_length: usize) {
        self.min_command_length = min_length;
    }

    pub fn set_border_config(
        &mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color;
    }

    pub fn set_border_sides(&mut self, border_sides: BorderSides) {
        self.border_sides = border_sides;
    }

    pub fn set_show_title(&mut self, show_title: bool) {
        self.show_title = show_title;
    }

    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn set_title_position(&mut self, position: TitlePosition) {
        self.title_position = position;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color;
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color;
    }

    pub fn set_cursor_colors(&mut self, fg: Option<String>, bg: Option<String>) {
        self.cursor_fg_color = fg;
        self.cursor_bg_color = bg;
    }

    pub fn set_prompt_icon(&mut self, icon: Option<String>) {
        self.prompt_icon = icon;
    }

    pub fn set_prompt_icon_color(&mut self, color: Option<String>) {
        self.prompt_icon_color = color;
    }

    pub fn insert_char(&mut self, c: char) {
        // Find the byte index for cursor position
        let byte_idx = self.char_pos_to_byte_idx(self.cursor_pos);
        self.input.insert(byte_idx, c);
        self.cursor_pos += 1;
        // Reset completion state when typing
        self.reset_completion();
        // Mark as user-typed
        self.is_user_typed = true;
        tracing::debug!(
            "CommandInput: inserted '{}', input now: '{}', cursor at {}",
            c,
            self.input,
            self.cursor_pos
        );
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

    /// Parse color string (hex or named) using centralized parser
    fn parse_color(color_str: &str) -> Option<Color> {
        parse_color_to_ratatui(color_str)
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor_pos = 0;
        self.history_index = None;
        self.is_user_typed = false;
    }

    pub fn get_input(&self) -> Option<String> {
        if self.input.is_empty() {
            None
        } else {
            Some(self.input.clone())
        }
    }

    pub fn get_last_command(&self) -> Option<String> {
        self.history.front().cloned()
    }

    pub fn get_second_last_command(&self) -> Option<String> {
        self.history.get(1).cloned()
    }

    pub fn submit(&mut self) -> Option<String> {
        if self.input.is_empty() {
            return None;
        }

        let command = self.input.clone();

        // Add to history only if:
        // 1. Command meets minimum length requirement
        // 2. Command is different from the last command in history (avoid consecutive duplicates)
        if command.len() >= self.min_command_length {
            let should_add = self
                .history
                .front()
                .map(|last_cmd| last_cmd != &command)
                .unwrap_or(true); // If history is empty, add the command

            if should_add {
                self.history.push_front(command.clone());
                if self.history.len() > self.max_history {
                    self.history.pop_back();
                }
            }
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
            self.is_user_typed = false; // Text is now from history
        }
    }

    pub fn history_next(&mut self) {
        match self.history_index {
            None => {
                // Not in history navigation - if user typed something, clear it
                if self.is_user_typed && !self.input.is_empty() {
                    self.clear();
                }
            }
            Some(0) => {
                // At most recent history entry, go back to empty
                self.input.clear();
                self.cursor_pos = 0;
                self.history_index = None;
                self.is_user_typed = false;
            }
            Some(idx) => {
                // Cycle down through history
                let new_index = idx - 1;
                if let Some(cmd) = self.history.get(new_index) {
                    self.input = cmd.clone();
                    self.cursor_pos = self.input.chars().count();
                    self.history_index = Some(new_index);
                    self.is_user_typed = false;
                }
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_with_status(area, buf, None);
    }

    pub fn render_with_status(&self, area: Rect, buf: &mut Buffer, status: Option<&str>) {
        let title = if let Some(status_text) = status {
            format!("{} [{}]", self.title, status_text)
        } else {
            self.title.clone()
        };

        // Check if border_style is "none" - that should disable borders too
        let border_is_none = self.border_style.as_ref().is_some_and(|s| s == "none");
        let show_border = self.show_border && !border_is_none;
        let title_text = if self.show_title { title } else { String::new() };

        let borders = crossterm_bridge::to_ratatui_borders(&self.border_sides);
        let border_type = match self.border_style.as_deref() {
            Some("double") => BorderType::Double,
            Some("rounded") => BorderType::Rounded,
            Some("thick") => BorderType::Thick,
            Some("quadrant_inside") => BorderType::QuadrantInside,
            Some("quadrant_outside") => BorderType::QuadrantOutside,
            _ => BorderType::Plain,
        };

        let mut border_style = Style::default();
        if let Some(color_str) = &self.border_color {
            if let Some(color) = Self::parse_color(color_str) {
                border_style = border_style.fg(color);
            }
        }

        // Fill background if explicitly set (do this BEFORE rendering block so it covers entire area)
        if let Some(ref color_hex) = self.background_color {
            if let Some(bg_color) = Self::parse_color(color_hex) {
                for row in 0..area.height {
                    for col in 0..area.width {
                        let x = area.x + col;
                        let y = area.y + row;
                        if x < buf.area().width && y < buf.area().height {
                            // Clear character and set background to prevent border artifacts
                            buf[(x, y)].set_char(' ').set_bg(bg_color);
                        }
                    }
                }
            }
        }

        // Clear area if no background and no border to prevent artifacts
        if self.background_color.is_none() && (!self.show_border || border_is_none) {
            // If no border and no background color, clear the area to prevent artifacts
            for row in 0..area.height {
                for col in 0..area.width {
                    let x = area.x + col;
                    let y = area.y + row;
                    if x < buf.area().width && y < buf.area().height {
                        buf[(x, y)].set_char(' ').reset();
                    }
                }
            }
        }

        // Only render block if it has borders (otherwise it's just empty)
        let inner = title_position::render_block_with_title(
            area,
            buf,
            show_border,
            borders,
            &self.border_sides,
            border_type,
            border_style,
            &title_text,
            self.title_position,
        );

        // Calculate horizontal scroll to keep cursor visible (account for optional icon)
        let mut text_area = inner;
        let text_color = self
            .text_color
            .as_ref()
            .and_then(|c| Self::parse_color(c))
            .unwrap_or(Color::White);
        let icon_text = self
            .prompt_icon
            .as_ref()
            .and_then(|s| {
                let t = s.trim();
                if t.is_empty() { None } else { Some(t) }
            });
        if let Some(icon) = icon_text {
            let max_icon_width = inner.width as usize;
            if max_icon_width > 0 {
                let icon_render: String = icon.chars().take(max_icon_width).collect();
                let icon_render_width = icon_render.chars().count();
                let icon_color = self
                    .prompt_icon_color
                    .as_ref()
                    .and_then(|c| Self::parse_color(c))
                    .unwrap_or(text_color);
                buf.set_string(
                    inner.x,
                    inner.y,
                    &icon_render,
                    Style::default().fg(icon_color),
                );
                let mut consumed = icon_render_width;
                // Add a trailing spacer if room allows
                if consumed < max_icon_width {
                    buf.set_string(
                        inner.x + consumed as u16,
                        inner.y,
                        " ",
                        Style::default().fg(icon_color),
                    );
                    consumed += 1;
                }
                text_area.x = text_area.x.saturating_add(consumed as u16);
                text_area.width = text_area.width.saturating_sub(consumed as u16);
            }
        }

        let available_width = text_area.width as usize;
        let chars: Vec<char> = self.input.chars().collect();
        let total_chars = chars.len();

        // We need space for: text before cursor + cursor block + text after cursor
        // The cursor block takes 1 position, so max visible cursor position is (available_width - 1)
        let max_visible_cursor_pos = available_width.saturating_sub(1);

        let scroll_offset = if available_width == 0 {
            0
        } else if total_chars < available_width {
            // Everything fits - no scroll needed
            0
        } else {
            // Text is longer than visible area - need to scroll
            // Keep cursor at 30% from left edge when scrolling
            let target_cursor_pos = (available_width * 3 / 10).min(max_visible_cursor_pos);

            // Calculate scroll to position cursor at target_cursor_pos from left
            if self.cursor_pos < target_cursor_pos {
                // Near start - show from beginning
                0
            } else if self.cursor_pos
                >= total_chars.saturating_sub(available_width - target_cursor_pos)
            {
                // Near end - anchor to end, ensuring cursor stays within bounds
                total_chars.saturating_sub(available_width)
            } else {
                // Middle - keep cursor at target position from left
                self.cursor_pos.saturating_sub(target_cursor_pos)
            }
        };

        // Extract visible portion of text with scroll applied
        // Take up to available_width chars, which includes the cursor position
        let visible_chars: Vec<char> = chars
            .iter()
            .skip(scroll_offset)
            .take(available_width)
            .copied()
            .collect();

        // Adjust cursor position relative to visible window
        let visible_cursor_pos = self.cursor_pos.saturating_sub(scroll_offset);

        // Ensure cursor position doesn't exceed available space
        let visible_cursor_pos = visible_cursor_pos.min(available_width.saturating_sub(1));

        let before_cursor: String = visible_chars.iter().take(visible_cursor_pos).collect();
        let cursor_char = visible_chars
            .get(visible_cursor_pos)
            .copied()
            .unwrap_or(' ');
        let after_cursor: String = visible_chars.iter().skip(visible_cursor_pos + 1).collect();

        // Get cursor colors
        let cursor_fg = self
            .cursor_fg_color
            .as_ref()
            .and_then(|c| Self::parse_color(c))
            .unwrap_or(Color::Black);
        let cursor_bg = self
            .cursor_bg_color
            .as_ref()
            .and_then(|c| Self::parse_color(c))
            .unwrap_or(Color::White);

        let line = Line::from(vec![
            Span::styled(before_cursor, Style::default().fg(text_color)),
            Span::styled(
                cursor_char.to_string(),
                Style::default().bg(cursor_bg).fg(cursor_fg),
            ),
            Span::styled(after_cursor, Style::default().fg(text_color)),
        ]);

        let paragraph = Paragraph::new(line);
        paragraph.render(text_area, buf);
    }

    /// Reset completion state
    fn reset_completion(&mut self) {
        self.completion_candidates.clear();
        self.completion_index = None;
        self.completion_prefix = None;
    }

    /// Try to complete the current input
    /// Returns true if a completion was performed
    pub fn try_complete(
        &mut self,
        available_commands: &[String],
        available_names: &[String],
    ) -> bool {
        // Only complete if cursor is at the end
        if self.cursor_pos != self.input.chars().count() {
            return false;
        }

        // If we're not in a completion session, start one
        if self.completion_candidates.is_empty() {
            let input = self.input.trim();

            // Find what we're trying to complete
            let (prefix, word_to_complete) = if let Some(pos) = input.rfind(char::is_whitespace) {
                // Completing a word after a space (e.g., ".createwindow mai" -> complete "mai")
                let prefix = &input[..=pos];
                let word = &input[pos + 1..];
                (prefix.to_string(), word)
            } else {
                // Completing the first word (e.g., ".createw" -> complete ".createw")
                ("".to_string(), input)
            };

            if word_to_complete.is_empty() {
                return false;
            }

            // Find candidates
            let mut candidates = Vec::new();

            // If completing a dot command (starts with .)
            if word_to_complete.starts_with('.') {
                for cmd in available_commands {
                    if cmd.starts_with(word_to_complete) {
                        candidates.push(cmd.clone());
                    }
                }
            } else {
                // Completing a window/template name
                for name in available_names {
                    if name.starts_with(word_to_complete) {
                        candidates.push(name.clone());
                    }
                }
            }

            if candidates.is_empty() {
                return false;
            }

            candidates.sort();
            self.completion_candidates = candidates;
            self.completion_prefix = Some(prefix);
            self.completion_index = Some(0);
        } else {
            // Already in completion session, cycle to next candidate
            if let Some(ref mut index) = self.completion_index {
                *index = (*index + 1) % self.completion_candidates.len();
            }
        }

        // Apply the current completion
        if let (Some(index), Some(ref prefix)) = (self.completion_index, &self.completion_prefix) {
            if let Some(candidate) = self.completion_candidates.get(index) {
                self.input = format!("{}{}", prefix, candidate);
                self.cursor_pos = self.input.chars().count();
                return true;
            }
        }

        false
    }

    /// Get the history file path (~/.vellum-fe/history/<character>.txt or default.txt)
    fn get_history_path(character: Option<&str>) -> Result<PathBuf, std::io::Error> {
        // Use the vellum-fe profile structure: ~/.vellum-fe/{character}/history.txt
        crate::config::Config::history_path(character)
            .map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// Load command history from disk
    pub fn load_history(&mut self, character: Option<&str>) -> Result<(), std::io::Error> {
        let history_path = Self::get_history_path(character)?;

        if !history_path.exists() {
            return Ok(()); // No history file yet, that's fine
        }

        let file = fs::File::open(&history_path)?;
        let reader = BufReader::new(file);

        self.history.clear();

        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                self.history.push_back(line);
                if self.history.len() > self.max_history {
                    self.history.pop_front();
                }
            }
        }

        tracing::debug!("Loaded {} commands from history", self.history.len());
        Ok(())
    }

    /// Save command history to disk
    pub fn save_history(&self, character: Option<&str>) -> Result<(), std::io::Error> {
        let history_path = Self::get_history_path(character)?;

        let mut file = fs::File::create(&history_path)?;

        // Save in reverse order (most recent first in file)
        for cmd in &self.history {
            writeln!(file, "{}", cmd)?;
        }

        tracing::debug!("Saved {} commands to history", self.history.len());
        Ok(())
    }

    /// Select all text in the input
    pub fn select_all(&mut self) {
        if !self.input.is_empty() {
            self.selection_start = Some(0);
            self.cursor_pos = self.input.len();
        }
    }

    /// Get the currently selected text (if any)
    pub fn get_selected_text(&self) -> Option<String> {
        if let Some(start) = self.selection_start {
            let end = self.cursor_pos;
            if start != end {
                let (from, to) = if start < end {
                    (start, end)
                } else {
                    (end, start)
                };
                return Some(self.input.chars().skip(from).take(to - from).collect());
            }
        }
        None
    }

    /// Clear the current selection
    pub fn clear_selection(&mut self) {
        self.selection_start = None;
    }
}
