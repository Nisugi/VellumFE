//! Stateful command line widget that mimics Profanity's behavior.
//!
//! Handles multi-byte cursoring, cut/copy selection, history persistence, and
//! autocomplete for both dot-commands and window names.

use crate::config::BorderSides;
use crate::frontend::common::CommandInputModel;
use crate::frontend::tui::{
    crossterm_bridge,
    title_position::{self, TitlePosition},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{BorderType, Paragraph, Widget},
};
use std::fs;
use std::io::{BufRead, BufReader, Write as _};
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation;

fn grapheme_display_width(grapheme: &str) -> usize {
    Span::raw(grapheme.to_string()).width()
}

fn text_char_widths(text: &str) -> Vec<usize> {
    text.graphemes(true)
        .flat_map(|grapheme| {
            std::iter::once(grapheme_display_width(grapheme))
                .chain(std::iter::repeat_n(0, grapheme.chars().count().saturating_sub(1)))
        })
        .collect()
}

fn prefix_fitting_width(text: &str, max_width: usize) -> (String, usize) {
    let mut width = 0;
    let prefix = text
        .graphemes(true)
        .take_while(|grapheme| {
            let next_width = grapheme_display_width(grapheme);
            if width + next_width > max_width {
                false
            } else {
                width += next_width;
                true
            }
        })
        .collect();
    (prefix, width)
}

fn push_cursor_and_completion(
    spans: &mut Vec<Span<'static>>,
    suffix: Option<String>,
    remaining_width: usize,
    cursor_fg: Color,
    cursor_bg: Color,
    completion_color: Color,
) {
    if let Some(suffix) = suffix {
        let mut graphemes = suffix.graphemes(true);
        if let Some(first) = graphemes.next() {
            let first_width = grapheme_display_width(first).max(1);
            if first_width <= remaining_width {
                spans.push(Span::styled(
                    first.to_string(),
                    Style::default().bg(cursor_bg).fg(cursor_fg),
                ));
                let rest = graphemes.collect::<String>();
                let (visible_rest, _) =
                    prefix_fitting_width(&rest, remaining_width.saturating_sub(first_width));
                spans.push(Span::styled(
                    visible_rest,
                    Style::default().fg(completion_color),
                ));
                return;
            }
        }
    }
    spans.push(Span::styled(
        " ",
        Style::default().bg(cursor_bg).fg(cursor_fg),
    ));
}

pub struct CommandInput {
    model: CommandInputModel,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    show_title: bool,
    border_sides: BorderSides,
    title: String,
    title_position: TitlePosition,
    background_color: Option<String>,
    text_color: Option<String>,        // Input text color
    completion_color: Option<String>,  // History completion suffix color
    history_suggestions: bool,         // ui.history_suggestions (ghost off when false)
    cursor_fg_color: Option<String>,   // Cursor foreground color
    cursor_bg_color: Option<String>,   // Cursor background color
    prompt_icon: Option<String>,       // Optional prompt icon shown before input
    prompt_icon_color: Option<String>, // Color for prompt icon
}

impl CommandInput {
    pub fn new(max_history: usize) -> Self {
        Self {
            model: CommandInputModel::new(max_history),
            show_border: true,
            border_style: None,
            border_color: None,
            show_title: true,
            border_sides: BorderSides::default(),
            title: "Command".to_string(),
            title_position: TitlePosition::TopLeft,
            background_color: None,
            text_color: None,       // Will use global default
            completion_color: None, // Will use a visible muted fallback
            history_suggestions: true,
            cursor_fg_color: None,  // Default: black
            cursor_bg_color: None,  // Default: white
            prompt_icon: None,
            prompt_icon_color: None,
        }
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

    pub fn set_completion_color(&mut self, color: Option<String>) {
        self.completion_color = color;
    }

    pub fn set_history_suggestions(&mut self, enabled: bool) {
        self.history_suggestions = enabled;
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
        self.model.insert_char(c);
        tracing::debug!(
            "CommandInput: inserted '{}', input now: '{}', cursor at {}",
            c,
            self.model.text(),
            self.model.cursor_pos()
        );
    }

    pub fn delete_char(&mut self) {
        self.model.delete_char();
    }

    pub fn move_cursor_left(&mut self, extend: bool) {
        self.model.move_cursor_left(extend);
    }

    pub fn move_cursor_right(&mut self, extend: bool) {
        self.model.move_cursor_right(extend);
    }

    pub fn move_cursor_home(&mut self, extend: bool) {
        self.model.move_cursor_home(extend);
    }

    pub fn move_cursor_end(&mut self, extend: bool) {
        self.model.move_cursor_end(extend);
    }

    pub fn move_cursor_word_left(&mut self, extend: bool) {
        self.model.move_cursor_word_left(extend);
    }

    pub fn move_cursor_word_right(&mut self, extend: bool) {
        self.model.move_cursor_word_right(extend);
    }

    pub fn delete_word(&mut self) {
        self.model.delete_word_forward();
    }

    /// Parse color string (hex or named) using centralized parser
    fn parse_color(&self, color_str: &str) -> Option<Color> {
        super::colors::parse_color_to_ratatui(color_str)
    }

    pub fn clear(&mut self) {
        self.model.clear();
    }

    pub fn get_input(&self) -> Option<String> {
        self.model.get_input()
    }

    pub fn get_last_command(&self) -> Option<String> {
        self.model.get_last_command()
    }

    pub fn get_second_last_command(&self) -> Option<String> {
        self.model.get_second_last_command()
    }

    pub fn submit(&mut self) -> Option<String> {
        self.model.submit()
    }

    pub fn record_external_command(&mut self, command: &str) {
        self.model.record_external_command(command);
    }

    pub fn history_previous(&mut self) {
        self.model.history_previous();
    }

    pub fn history_next(&mut self) {
        self.model.history_next();
    }

    pub fn accept_history_completion(&mut self) -> bool {
        if !self.history_suggestions {
            return false;
        }
        self.model.accept_history_completion()
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
        let title_text = if self.show_title {
            title
        } else {
            String::new()
        };

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
            if let Some(color) = self.parse_color(color_str) {
                border_style = border_style.fg(color);
            }
        }

        // Fill background if explicitly set (do this BEFORE rendering block so it covers entire area)
        if let Some(ref color_hex) = self.background_color {
            if let Some(bg_color) = self.parse_color(color_hex) {
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
            .and_then(|c| self.parse_color(c))
            .unwrap_or(Color::White);
        let completion_color = self
            .completion_color
            .as_ref()
            .and_then(|c| self.parse_color(c))
            .unwrap_or(Color::Gray);
        let icon_text = self.prompt_icon.as_ref().and_then(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        });
        if let Some(icon) = icon_text {
            let max_icon_width = inner.width as usize;
            if max_icon_width > 0 {
                let (icon_render, icon_render_width) =
                    prefix_fitting_width(icon, max_icon_width);
                let icon_color = self
                    .prompt_icon_color
                    .as_ref()
                    .and_then(|c| self.parse_color(c))
                    .unwrap_or(text_color);
                buf.set_string(
                    inner.x,
                    inner.y,
                    &icon_render,
                    Style::default().fg(icon_color),
                );
                let mut consumed = icon_render_width;
                // Add a trailing spacer if room allows
                if consumed > 0 && consumed < max_icon_width {
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
        let chars: Vec<char> = self.model.text().chars().collect();
        let total_chars = chars.len();
        let char_widths = text_char_widths(self.model.text());
        let total_width: usize = char_widths.iter().sum();
        let selection = self.model.selection_range();
        let history_completion = if self.history_suggestions {
            self.model.history_completion()
        } else {
            None
        };
        let cursor_at_end = self.model.cursor_pos() == total_chars;
        let completion_width = history_completion
            .as_deref()
            .and_then(|suffix| suffix.graphemes(true).next())
            .map(grapheme_display_width)
            .unwrap_or(0);
        let reserved_width = if cursor_at_end {
            completion_width.max(1).min(available_width)
        } else {
            0
        };
        let visible_text_width = available_width.saturating_sub(reserved_width);

        let scroll_offset = if available_width == 0 {
            0
        } else if cursor_at_end {
            let mut start = self.model.cursor_pos();
            let mut width = 0;
            while start > 0 && width + char_widths[start - 1] <= visible_text_width {
                start -= 1;
                width += char_widths[start];
            }
            start
        } else if total_width < available_width {
            // Everything fits - no scroll needed
            0
        } else {
            // Text is longer than visible area - need to scroll
            // Keep cursor at 30% from left edge when scrolling
            let cursor_width = char_widths
                .get(self.model.cursor_pos())
                .copied()
                .unwrap_or(1);
            let target_cursor_width = (available_width * 3 / 10)
                .min(available_width.saturating_sub(cursor_width));
            let mut start = self.model.cursor_pos();
            let mut width = 0;
            while start > 0 && width + char_widths[start - 1] <= target_cursor_width {
                start -= 1;
                width += char_widths[start];
            }
            start
        };

        // Extract visible portion of text with scroll applied
        // Reserve room at the end for the cursor and at least one completion
        // character, even when the input itself fills the window.
        let mut visible_chars = Vec::new();
        let mut visible_width = 0;
        for (index, ch) in chars.iter().copied().enumerate().skip(scroll_offset) {
            let width = char_widths[index];
            if visible_width + width > visible_text_width {
                break;
            }
            visible_chars.push((index, ch));
            visible_width += width;
        }

        // Adjust cursor position relative to visible window
        let visible_cursor_pos: usize = char_widths[scroll_offset..self.model.cursor_pos()]
            .iter()
            .sum();

        // Get cursor colors
        let cursor_fg = self
            .cursor_fg_color
            .as_ref()
            .and_then(|c| self.parse_color(c))
            .unwrap_or(Color::Black);
        let cursor_bg = self
            .cursor_bg_color
            .as_ref()
            .and_then(|c| self.parse_color(c))
            .unwrap_or(Color::White);

        let selection_bg = self
            .cursor_bg_color
            .as_ref()
            .and_then(|c| self.parse_color(c))
            .unwrap_or(Color::DarkGray);

        let mut spans = Vec::new();
        if visible_chars.is_empty() {
            push_cursor_and_completion(
                &mut spans,
                history_completion,
                available_width,
                cursor_fg,
                cursor_bg,
                completion_color,
            );
        } else {
            let mut cell_pos = 0;
            for (global_idx, ch) in &visible_chars {
                let mut style = Style::default().fg(text_color);
                if let Some((start, end)) = selection {
                    if *global_idx >= start && *global_idx < end {
                        style = style.bg(selection_bg);
                    }
                }
                if cell_pos == visible_cursor_pos {
                    style = Style::default().bg(cursor_bg).fg(cursor_fg);
                }
                spans.push(Span::styled(ch.to_string(), style));
                cell_pos += char_widths[*global_idx];
            }
            let cursor_is_visible_at_end =
                self.model.cursor_pos() == total_chars && visible_width < available_width;
            if cursor_is_visible_at_end {
                push_cursor_and_completion(
                    &mut spans,
                    history_completion,
                    available_width.saturating_sub(visible_width),
                    cursor_fg,
                    cursor_bg,
                    completion_color,
                );
            }
        }

        let line = Line::from(spans);

        let paragraph = Paragraph::new(line);
        paragraph.render(text_area, buf);
    }

    /// Render the command input area in search mode, inheriting all visual settings
    /// (borders, background, etc.) from the command_input configuration.
    pub fn render_search_mode(
        &self,
        area: Rect,
        buf: &mut Buffer,
        search_input: &str,
        search_cursor: usize,
        search_info: Option<(usize, usize)>, // (current_match, total_matches)
    ) {
        // Use the same border/background rendering logic as render_with_status
        let border_is_none = self.border_style.as_ref().is_some_and(|s| s == "none");
        let show_border = self.show_border && !border_is_none;

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
            if let Some(color) = self.parse_color(color_str) {
                border_style = border_style.fg(color);
            }
        }

        // Fill background if explicitly set
        if let Some(ref color_hex) = self.background_color {
            if let Some(bg_color) = self.parse_color(color_hex) {
                for row in 0..area.height {
                    for col in 0..area.width {
                        let x = area.x + col;
                        let y = area.y + row;
                        if x < buf.area().width && y < buf.area().height {
                            buf[(x, y)].set_char(' ').set_bg(bg_color);
                        }
                    }
                }
            }
        }

        // Clear area if no background and no border to prevent artifacts
        if self.background_color.is_none() && (!self.show_border || border_is_none) {
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

        // Render block with borders (no title in search mode)
        let inner = title_position::render_block_with_title(
            area,
            buf,
            show_border,
            borders,
            &self.border_sides,
            border_type,
            border_style,
            "", // No title in search mode
            self.title_position,
        );

        // Build search prompt with match info
        let search_info_text = match search_info {
            Some((current, total)) => format!(" [{}/{}]", current + 1, total),
            None => String::new(),
        };
        let prompt = format!("Search{}: ", search_info_text);
        let placeholder = "Enter:Search, Esc:Cancel, Ctrl+PgUp/PgDn:Navigate";

        // Build search line with cursor
        let search_line = if search_input.is_empty() {
            // Show dimmed placeholder with cursor at start
            Line::from(vec![
                Span::styled(prompt, Style::default().fg(Color::Yellow)),
                Span::styled(" ", Style::default().bg(Color::White).fg(Color::DarkGray)),
                Span::styled(placeholder, Style::default().fg(Color::DarkGray)),
            ])
        } else {
            // Show user input with cursor
            let chars: Vec<char> = search_input.chars().collect();
            let before_cursor: String = chars.iter().take(search_cursor).collect();
            let cursor_char = chars.get(search_cursor).copied().unwrap_or(' ');
            let after_cursor: String = chars.iter().skip(search_cursor + 1).collect();

            Line::from(vec![
                Span::styled(prompt, Style::default().fg(Color::Yellow)),
                Span::raw(before_cursor),
                Span::styled(
                    cursor_char.to_string(),
                    Style::default().bg(Color::White).fg(Color::Black),
                ),
                Span::raw(after_cursor),
            ])
        };

        let search_paragraph = Paragraph::new(search_line);
        search_paragraph.render(inner, buf);
    }

    /// Try to complete the current input.
    /// Returns true if a completion was performed (the text changed).
    pub fn try_complete(
        &mut self,
        available_commands: &[String],
        available_names: &[String],
    ) -> bool {
        self.model.try_complete(available_commands, available_names)
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

        self.model.history_mut().clear();

        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                self.model.history_mut().push_back(line);
                if self.model.history().len() > self.model.max_history() {
                    self.model.history_mut().pop_front();
                }
            }
        }

        tracing::debug!(
            "Loaded {} commands from history",
            self.model.history().len()
        );
        Ok(())
    }

    /// Save command history to disk
    pub fn save_history(&self, character: Option<&str>) -> Result<(), std::io::Error> {
        let history_path = Self::get_history_path(character)?;

        let mut file = fs::File::create(&history_path)?;

        // Save in reverse order (most recent first in file)
        for cmd in self.model.history() {
            writeln!(file, "{}", cmd)?;
        }

        tracing::debug!("Saved {} commands to history", self.model.history().len());
        Ok(())
    }

    /// Select all text in the input
    pub fn select_all(&mut self) {
        self.model.select_all();
    }

    /// Get the currently selected text (if any)
    pub fn get_selected_text(&self) -> Option<String> {
        self.model.get_selected_text()
    }

    pub fn delete_selection(&mut self) -> bool {
        self.model.delete_selection()
    }

    pub fn insert_text(&mut self, text: &str) {
        self.model.insert_text(text);
    }

    pub fn delete_word_backward(&mut self) {
        self.model.delete_word_backward();
    }

    pub fn undo(&mut self) -> bool {
        self.model.undo()
    }

    pub fn redo(&mut self) -> bool {
        self.model.redo()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_widths_keep_grapheme_clusters_together() {
        assert_eq!(text_char_widths("👩‍💻"), vec![2, 0, 0]);
        assert_eq!(text_char_widths("e\u{301}"), vec![1, 0]);
        assert_eq!(prefix_fitting_width("👩‍💻!", 1), (String::new(), 0));
        assert_eq!(prefix_fitting_width("👩‍💻!", 2), ("👩‍💻".to_string(), 2));
    }

    #[test]
    fn completion_styles_the_first_grapheme_as_the_cursor() {
        let mut spans = Vec::new();
        push_cursor_and_completion(
            &mut spans,
            Some("👩‍💻!".to_string()),
            3,
            Color::Black,
            Color::White,
            Color::Gray,
        );

        assert_eq!(spans[0].content.as_ref(), "👩‍💻");
        assert_eq!(spans[0].style.bg, Some(Color::White));
        assert_eq!(spans[1].content.as_ref(), "!");
        assert_eq!(spans[1].style.fg, Some(Color::Gray));
    }

    #[test]
    fn completion_remains_visible_when_input_fills_the_window() {
        let mut input = CommandInput::new(10);
        input.show_border = false;
        input.show_title = false;
        input.record_external_command("abcdefg");
        input.insert_text("abcdef");

        let area = Rect::new(0, 0, 6, 1);
        let mut buf = Buffer::empty(area);
        input.render(area, &mut buf);

        assert_eq!(buf[(4, 0)].symbol(), "f");
        assert_eq!(buf[(5, 0)].symbol(), "g");
        assert_eq!(buf[(5, 0)].bg, Color::White);
    }

    #[test]
    fn completion_remains_visible_after_horizontal_scrolling() {
        let mut input = CommandInput::new(10);
        input.show_border = false;
        input.show_title = false;
        input.record_external_command("abcdefghij!");
        input.insert_text("abcdefghij");

        let area = Rect::new(0, 0, 6, 1);
        let mut buf = Buffer::empty(area);
        input.render(area, &mut buf);

        assert_eq!(buf[(5, 0)].symbol(), "!");
    }

    #[test]
    fn completion_clipping_accounts_for_wide_characters() {
        let mut input = CommandInput::new(10);
        input.show_border = false;
        input.show_title = false;
        input.record_external_command("界界界!");
        input.insert_text("界界界");

        let area = Rect::new(0, 0, 6, 1);
        let mut buf = Buffer::empty(area);
        input.render(area, &mut buf);

        assert_eq!(buf[(4, 0)].symbol(), "!");
        assert_eq!(buf[(4, 0)].bg, Color::White);
    }

    #[test]
    fn completion_uses_configured_color_without_terminal_dim() {
        let mut input = CommandInput::new(10);
        input.show_border = false;
        input.show_title = false;
        input.set_completion_color(Some("#d8dee9".to_string()));
        input.record_external_command("go well");
        input.insert_text("go ");

        let area = Rect::new(0, 0, 12, 1);
        let mut buf = Buffer::empty(area);
        input.render(area, &mut buf);

        assert_eq!(buf[(3, 0)].symbol(), "w");
        assert_eq!(buf[(3, 0)].bg, Color::White);
        assert_eq!(buf[(4, 0)].symbol(), "e");
        assert_eq!(buf[(4, 0)].fg, Color::Rgb(216, 222, 233));
        assert!(!buf[(4, 0)]
            .modifier
            .contains(ratatui::style::Modifier::DIM));
    }
}
