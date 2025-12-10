//! Adapter to implement frontend-agnostic TextInput trait for tui-textarea::TextArea.
//!
//! This adapter wraps tui-textarea and implements the common::TextInput trait,
//! allowing TUI widgets to use the same text editing interface that GUI widgets will use.

use crate::frontend::common::text_input::{CursorMove, TextInput};
use tui_textarea::{CursorMove as TuiCursorMove, TextArea};

/// Wrapper around tui-textarea::TextArea that implements TextInput
pub struct TextAreaAdapter<'a> {
    inner: &'a mut TextArea<'static>,
}

impl<'a> TextAreaAdapter<'a> {
    /// Create a new adapter wrapping a TextArea
    pub fn new(text_area: &'a mut TextArea<'static>) -> Self {
        Self { inner: text_area }
    }

    /// Get the underlying TextArea reference (for rendering)
    pub fn inner(&self) -> &TextArea<'static> {
        self.inner
    }

    /// Get the underlying TextArea mutable reference
    pub fn inner_mut(&mut self) -> &mut TextArea<'static> {
        self.inner
    }
}

impl<'a> TextInput for TextAreaAdapter<'a> {
    fn text(&self) -> String {
        self.inner.lines().join("\n")
    }

    fn lines(&self) -> Vec<String> {
        self.inner.lines().to_vec()
    }

    fn set_text(&mut self, text: String) {
        let lines: Vec<&str> = text.split('\n').collect();
        *self.inner = TextArea::new(lines.iter().map(|s| s.to_string()).collect());
    }

    fn insert_char(&mut self, c: char) {
        self.inner.insert_char(c);
    }

    fn insert_str(&mut self, s: &str) {
        self.inner.insert_str(s);
    }

    fn delete_char(&mut self) {
        self.inner.delete_char();
    }

    fn delete_forward_char(&mut self) {
        self.inner.delete_next_char();
    }

    fn delete_line_by_end(&mut self) {
        self.inner.delete_line_by_end();
    }

    fn delete_line_by_head(&mut self) {
        self.inner.delete_line_by_head();
    }

    fn delete_current_line(&mut self) {
        self.inner.delete_line_by_head();
        self.inner.delete_line_by_end();
    }

    fn delete_next_word(&mut self) {
        self.inner.delete_next_word();
    }

    fn delete_prev_word(&mut self) {
        self.inner.delete_word();
    }

    fn move_cursor(&mut self, mv: CursorMove) {
        let tui_move = match mv {
            CursorMove::Head => TuiCursorMove::Head,
            CursorMove::End => TuiCursorMove::End,
            CursorMove::Top => TuiCursorMove::Top,
            CursorMove::Bottom => TuiCursorMove::Bottom,
            CursorMove::Forward => TuiCursorMove::Forward,
            CursorMove::Back => TuiCursorMove::Back,
            CursorMove::Up => TuiCursorMove::Up,
            CursorMove::Down => TuiCursorMove::Down,
            CursorMove::WordForward => TuiCursorMove::WordForward,
            CursorMove::WordBack => TuiCursorMove::WordBack,
            CursorMove::ParagraphBack => TuiCursorMove::ParagraphBack,
            CursorMove::ParagraphForward => TuiCursorMove::ParagraphForward,
            CursorMove::Jump(row, col) => TuiCursorMove::Jump(row, col),
        };
        self.inner.move_cursor(tui_move);
    }

    fn cursor_position(&self) -> (u16, u16) {
        let (row, col) = self.inner.cursor();
        (row as u16, col as u16)
    }

    fn start_selection(&mut self) {
        self.inner.start_selection();
    }

    fn cancel_selection(&mut self) {
        self.inner.cancel_selection();
    }

    fn has_selection(&self) -> bool {
        // tui-textarea doesn't expose this directly, so we check if yank_text returns anything
        !self.inner.yank_text().is_empty()
    }

    fn selected_text(&self) -> String {
        self.inner.yank_text()
    }

    fn yank_text(&self) -> String {
        self.inner.yank_text()
    }

    fn set_yank_text(&mut self, text: String) {
        self.inner.set_yank_text(text);
    }

    fn clear(&mut self) {
        self.inner.move_cursor(TuiCursorMove::Top);
        self.inner.start_selection();
        self.inner.move_cursor(TuiCursorMove::Bottom);
        self.inner.move_cursor(TuiCursorMove::End);
        self.inner.delete_char(); // Deletes selection
    }
}

/// Extension trait for TextArea to easily convert to adapter
pub trait TextAreaExt {
    /// Create an adapter that implements TextInput
    fn as_input(&mut self) -> TextAreaAdapter;
}

impl TextAreaExt for TextArea<'static> {
    fn as_input(&mut self) -> TextAreaAdapter {
        TextAreaAdapter::new(self)
    }
}
