//! Frontend-agnostic text input handling.
//!
//! Provides a trait-based abstraction over text editing backends (tui-textarea for TUI,
//! native widgets for GUI) to enable shared text editing logic across frontends.

use super::input::{KeyCode, KeyModifiers};

/// Cursor movement operations for text editing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMove {
    /// Move to beginning of current line
    Head,
    /// Move to end of current line
    End,
    /// Move to beginning of document
    Top,
    /// Move to end of document
    Bottom,
    /// Move forward one character
    Forward,
    /// Move backward one character
    Back,
    /// Move up one line
    Up,
    /// Move down one line
    Down,
    /// Move forward one word
    WordForward,
    /// Move backward one word
    WordBack,
    /// Move to previous paragraph
    ParagraphBack,
    /// Move to next paragraph
    ParagraphForward,
    /// Jump to specific (row, col) position
    Jump(u16, u16),
}

/// Frontend-agnostic text input field interface.
///
/// This trait abstracts over different text editing backends:
/// - TUI: `tui-textarea::TextArea`
/// - GUI: Native text widgets (egui::TextEdit, iced::TextInput, etc.)
pub trait TextInput {
    /// Get the current text content
    fn text(&self) -> String;

    /// Get lines of text (for multi-line editors)
    fn lines(&self) -> Vec<String>;

    /// Set the entire text content
    fn set_text(&mut self, text: String);

    /// Insert a character at the current cursor position
    fn insert_char(&mut self, c: char);

    /// Insert a string at the current cursor position
    fn insert_str(&mut self, s: &str);

    /// Delete the character before the cursor (backspace)
    fn delete_char(&mut self);

    /// Delete the character at the cursor (delete key)
    fn delete_forward_char(&mut self);

    /// Delete from cursor to end of line
    fn delete_line_by_end(&mut self);

    /// Delete from cursor to beginning of line
    fn delete_line_by_head(&mut self);

    /// Delete the entire current line
    fn delete_current_line(&mut self);

    /// Delete the next word
    fn delete_next_word(&mut self);

    /// Delete the previous word
    fn delete_prev_word(&mut self);

    /// Move the cursor
    fn move_cursor(&mut self, mv: CursorMove);

    /// Get current cursor position (row, col)
    fn cursor_position(&self) -> (u16, u16);

    /// Start text selection at current cursor
    fn start_selection(&mut self);

    /// Cancel current selection
    fn cancel_selection(&mut self);

    /// Check if text is currently selected
    fn has_selection(&self) -> bool;

    /// Get selected text (empty string if no selection)
    fn selected_text(&self) -> String;

    /// Get yanked text (clipboard-like internal buffer)
    fn yank_text(&self) -> String;

    /// Set yanked text (clipboard-like internal buffer)
    fn set_yank_text(&mut self, text: String);

    /// Clear all text
    fn clear(&mut self);

    /// Check if the field is empty
    fn is_empty(&self) -> bool {
        self.text().is_empty()
    }

    /// Get the number of lines
    fn line_count(&self) -> usize {
        self.lines().len()
    }
}

/// Frontend-agnostic text editing operations (high-level actions).
///
/// These operations build on top of TextInput to provide common editing workflows
/// like select-all, copy, cut, paste that work across any frontend.
pub trait TextEditor: TextInput {
    /// Select all text in the field (Ctrl+A)
    fn select_all(&mut self) {
        self.move_cursor(CursorMove::End);
        self.start_selection();
        self.move_cursor(CursorMove::Head);
    }

    /// Copy selected text to internal buffer (like clipboard)
    fn copy(&mut self) -> String {
        let text = self.selected_text();
        if !text.is_empty() {
            self.set_yank_text(text.clone());
        }
        text
    }

    /// Cut selected text to internal buffer
    fn cut(&mut self) -> String {
        let text = self.copy();
        if !text.is_empty() {
            self.insert_str(""); // Replace selection with empty string
        }
        text
    }

    /// Paste text from internal buffer
    fn paste(&mut self, text: &str) {
        self.insert_str(text);
    }

    /// Undo last operation (if supported by backend)
    fn undo(&mut self) {
        // Default: no-op (backends can override)
    }

    /// Redo last undone operation (if supported by backend)
    fn redo(&mut self) {
        // Default: no-op (backends can override)
    }
}

/// Auto-implement TextEditor for any type that implements TextInput
impl<T: TextInput> TextEditor for T {}

/// Helper to convert frontend-agnostic KeyCode to text input operations
pub fn handle_text_input_key(
    input: &mut dyn TextInput,
    key: KeyCode,
    modifiers: KeyModifiers,
) -> bool {
    match (key, modifiers.ctrl, modifiers.shift, modifiers.alt) {
        // Character input
        (KeyCode::Char(c), false, _, false) => {
            input.insert_char(c);
            true
        }

        // Ctrl+A: Select all
        (KeyCode::Char('a'), true, _, _) => {
            input.move_cursor(CursorMove::End);
            input.start_selection();
            input.move_cursor(CursorMove::Head);
            true
        }

        // Movement
        (KeyCode::Left, false, false, false) => {
            input.move_cursor(CursorMove::Back);
            true
        }
        (KeyCode::Right, false, false, false) => {
            input.move_cursor(CursorMove::Forward);
            true
        }
        (KeyCode::Up, false, false, false) => {
            input.move_cursor(CursorMove::Up);
            true
        }
        (KeyCode::Down, false, false, false) => {
            input.move_cursor(CursorMove::Down);
            true
        }
        (KeyCode::Home, false, false, false) => {
            input.move_cursor(CursorMove::Head);
            true
        }
        (KeyCode::End, false, false, false) => {
            input.move_cursor(CursorMove::End);
            true
        }

        // Word movement (Ctrl+Arrow)
        (KeyCode::Left, true, false, false) => {
            input.move_cursor(CursorMove::WordBack);
            true
        }
        (KeyCode::Right, true, false, false) => {
            input.move_cursor(CursorMove::WordForward);
            true
        }

        // Deletion
        (KeyCode::Backspace, false, false, false) => {
            input.delete_char();
            true
        }
        (KeyCode::Delete, false, false, false) => {
            input.delete_forward_char();
            true
        }
        (KeyCode::Backspace, true, false, false) => {
            input.delete_prev_word();
            true
        }
        (KeyCode::Delete, true, false, false) => {
            input.delete_next_word();
            true
        }

        // Ctrl+U: Delete to line start
        (KeyCode::Char('u'), true, false, false) => {
            input.delete_line_by_head();
            true
        }

        // Ctrl+K: Delete to line end
        (KeyCode::Char('k'), true, false, false) => {
            input.delete_line_by_end();
            true
        }

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // CursorMove enum tests
    // ===========================================

    #[test]
    fn test_cursor_move_head() {
        let mv = CursorMove::Head;
        assert!(matches!(mv, CursorMove::Head));
    }

    #[test]
    fn test_cursor_move_end() {
        let mv = CursorMove::End;
        assert!(matches!(mv, CursorMove::End));
    }

    #[test]
    fn test_cursor_move_top() {
        let mv = CursorMove::Top;
        assert!(matches!(mv, CursorMove::Top));
    }

    #[test]
    fn test_cursor_move_bottom() {
        let mv = CursorMove::Bottom;
        assert!(matches!(mv, CursorMove::Bottom));
    }

    #[test]
    fn test_cursor_move_navigation() {
        assert!(matches!(CursorMove::Forward, CursorMove::Forward));
        assert!(matches!(CursorMove::Back, CursorMove::Back));
        assert!(matches!(CursorMove::Up, CursorMove::Up));
        assert!(matches!(CursorMove::Down, CursorMove::Down));
    }

    #[test]
    fn test_cursor_move_word() {
        assert!(matches!(CursorMove::WordForward, CursorMove::WordForward));
        assert!(matches!(CursorMove::WordBack, CursorMove::WordBack));
    }

    #[test]
    fn test_cursor_move_paragraph() {
        assert!(matches!(CursorMove::ParagraphBack, CursorMove::ParagraphBack));
        assert!(matches!(CursorMove::ParagraphForward, CursorMove::ParagraphForward));
    }

    #[test]
    fn test_cursor_move_jump() {
        let mv = CursorMove::Jump(10, 20);
        match mv {
            CursorMove::Jump(row, col) => {
                assert_eq!(row, 10);
                assert_eq!(col, 20);
            }
            _ => panic!("Expected Jump variant"),
        }
    }

    #[test]
    fn test_cursor_move_equality() {
        assert_eq!(CursorMove::Head, CursorMove::Head);
        assert_ne!(CursorMove::Head, CursorMove::End);
        assert_eq!(CursorMove::Jump(5, 10), CursorMove::Jump(5, 10));
        assert_ne!(CursorMove::Jump(5, 10), CursorMove::Jump(5, 11));
    }

    #[test]
    fn test_cursor_move_clone() {
        let mv = CursorMove::WordForward;
        let cloned = mv;
        assert_eq!(mv, cloned);
    }

    #[test]
    fn test_cursor_move_copy() {
        let mv = CursorMove::Forward;
        let copied = mv;
        assert_eq!(mv, copied); // Original still usable (Copy trait)
    }

    // ===========================================
    // Mock TextInput for testing handle_text_input_key
    // ===========================================

    struct MockTextInput {
        text: String,
        cursor_pos: (u16, u16),
        selection_active: bool,
        yank_buffer: String,
        last_operation: Option<String>,
    }

    impl MockTextInput {
        fn new() -> Self {
            Self {
                text: String::new(),
                cursor_pos: (0, 0),
                selection_active: false,
                yank_buffer: String::new(),
                last_operation: None,
            }
        }

        fn with_text(text: &str) -> Self {
            Self {
                text: text.to_string(),
                cursor_pos: (0, 0),
                selection_active: false,
                yank_buffer: String::new(),
                last_operation: None,
            }
        }
    }

    impl TextInput for MockTextInput {
        fn text(&self) -> String {
            self.text.clone()
        }

        fn lines(&self) -> Vec<String> {
            self.text.lines().map(|s| s.to_string()).collect()
        }

        fn set_text(&mut self, text: String) {
            self.text = text;
            self.last_operation = Some("set_text".to_string());
        }

        fn insert_char(&mut self, c: char) {
            self.text.push(c);
            self.last_operation = Some(format!("insert_char:{}", c));
        }

        fn insert_str(&mut self, s: &str) {
            self.text.push_str(s);
            self.last_operation = Some(format!("insert_str:{}", s));
        }

        fn delete_char(&mut self) {
            self.text.pop();
            self.last_operation = Some("delete_char".to_string());
        }

        fn delete_forward_char(&mut self) {
            if !self.text.is_empty() {
                self.text.remove(0);
            }
            self.last_operation = Some("delete_forward_char".to_string());
        }

        fn delete_line_by_end(&mut self) {
            self.last_operation = Some("delete_line_by_end".to_string());
        }

        fn delete_line_by_head(&mut self) {
            self.last_operation = Some("delete_line_by_head".to_string());
        }

        fn delete_current_line(&mut self) {
            self.text.clear();
            self.last_operation = Some("delete_current_line".to_string());
        }

        fn delete_next_word(&mut self) {
            self.last_operation = Some("delete_next_word".to_string());
        }

        fn delete_prev_word(&mut self) {
            self.last_operation = Some("delete_prev_word".to_string());
        }

        fn move_cursor(&mut self, mv: CursorMove) {
            self.last_operation = Some(format!("move_cursor:{:?}", mv));
        }

        fn cursor_position(&self) -> (u16, u16) {
            self.cursor_pos
        }

        fn start_selection(&mut self) {
            self.selection_active = true;
            self.last_operation = Some("start_selection".to_string());
        }

        fn cancel_selection(&mut self) {
            self.selection_active = false;
            self.last_operation = Some("cancel_selection".to_string());
        }

        fn has_selection(&self) -> bool {
            self.selection_active
        }

        fn selected_text(&self) -> String {
            if self.selection_active {
                self.text.clone()
            } else {
                String::new()
            }
        }

        fn yank_text(&self) -> String {
            self.yank_buffer.clone()
        }

        fn set_yank_text(&mut self, text: String) {
            self.yank_buffer = text;
        }

        fn clear(&mut self) {
            self.text.clear();
            self.last_operation = Some("clear".to_string());
        }
    }

    // ===========================================
    // TextInput trait default implementations
    // ===========================================

    #[test]
    fn test_text_input_is_empty_true() {
        let input = MockTextInput::new();
        assert!(input.is_empty());
    }

    #[test]
    fn test_text_input_is_empty_false() {
        let input = MockTextInput::with_text("hello");
        assert!(!input.is_empty());
    }

    #[test]
    fn test_text_input_line_count_empty() {
        let input = MockTextInput::new();
        assert_eq!(input.line_count(), 0);
    }

    #[test]
    fn test_text_input_line_count_single() {
        let input = MockTextInput::with_text("hello world");
        assert_eq!(input.line_count(), 1);
    }

    #[test]
    fn test_text_input_line_count_multiple() {
        let input = MockTextInput::with_text("line1\nline2\nline3");
        assert_eq!(input.line_count(), 3);
    }

    // ===========================================
    // handle_text_input_key tests - Character input
    // ===========================================

    #[test]
    fn test_handle_char_input() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(result);
        assert_eq!(input.text, "a");
    }

    #[test]
    fn test_handle_char_input_with_shift() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Char('A'), KeyModifiers::SHIFT);
        assert!(result);
        assert_eq!(input.text, "A");
    }

    #[test]
    fn test_handle_char_input_multiple() {
        let mut input = MockTextInput::new();
        handle_text_input_key(&mut input, KeyCode::Char('h'), KeyModifiers::NONE);
        handle_text_input_key(&mut input, KeyCode::Char('i'), KeyModifiers::NONE);
        assert_eq!(input.text, "hi");
    }

    // ===========================================
    // handle_text_input_key tests - Ctrl+A select all
    // ===========================================

    #[test]
    fn test_handle_ctrl_a_select_all() {
        let mut input = MockTextInput::with_text("hello");
        let result = handle_text_input_key(&mut input, KeyCode::Char('a'), KeyModifiers::CTRL);
        assert!(result);
        assert!(input.selection_active);
    }

    // ===========================================
    // handle_text_input_key tests - Movement
    // ===========================================

    #[test]
    fn test_handle_left_arrow() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Left, KeyModifiers::NONE);
        assert!(result);
        assert!(input.last_operation.unwrap().contains("Back"));
    }

    #[test]
    fn test_handle_right_arrow() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Right, KeyModifiers::NONE);
        assert!(result);
        assert!(input.last_operation.unwrap().contains("Forward"));
    }

    #[test]
    fn test_handle_up_arrow() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Up, KeyModifiers::NONE);
        assert!(result);
        assert!(input.last_operation.unwrap().contains("Up"));
    }

    #[test]
    fn test_handle_down_arrow() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Down, KeyModifiers::NONE);
        assert!(result);
        assert!(input.last_operation.unwrap().contains("Down"));
    }

    #[test]
    fn test_handle_home() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Home, KeyModifiers::NONE);
        assert!(result);
        assert!(input.last_operation.unwrap().contains("Head"));
    }

    #[test]
    fn test_handle_end() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::End, KeyModifiers::NONE);
        assert!(result);
        assert!(input.last_operation.unwrap().contains("End"));
    }

    // ===========================================
    // handle_text_input_key tests - Word movement
    // ===========================================

    #[test]
    fn test_handle_ctrl_left_word_back() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Left, KeyModifiers::CTRL);
        assert!(result);
        assert!(input.last_operation.unwrap().contains("WordBack"));
    }

    #[test]
    fn test_handle_ctrl_right_word_forward() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Right, KeyModifiers::CTRL);
        assert!(result);
        assert!(input.last_operation.unwrap().contains("WordForward"));
    }

    // ===========================================
    // handle_text_input_key tests - Deletion
    // ===========================================

    #[test]
    fn test_handle_backspace() {
        let mut input = MockTextInput::with_text("hello");
        let result = handle_text_input_key(&mut input, KeyCode::Backspace, KeyModifiers::NONE);
        assert!(result);
        assert_eq!(input.last_operation, Some("delete_char".to_string()));
    }

    #[test]
    fn test_handle_delete() {
        let mut input = MockTextInput::with_text("hello");
        let result = handle_text_input_key(&mut input, KeyCode::Delete, KeyModifiers::NONE);
        assert!(result);
        assert_eq!(input.last_operation, Some("delete_forward_char".to_string()));
    }

    #[test]
    fn test_handle_ctrl_backspace_delete_word() {
        let mut input = MockTextInput::with_text("hello world");
        let result = handle_text_input_key(&mut input, KeyCode::Backspace, KeyModifiers::CTRL);
        assert!(result);
        assert_eq!(input.last_operation, Some("delete_prev_word".to_string()));
    }

    #[test]
    fn test_handle_ctrl_delete_delete_next_word() {
        let mut input = MockTextInput::with_text("hello world");
        let result = handle_text_input_key(&mut input, KeyCode::Delete, KeyModifiers::CTRL);
        assert!(result);
        assert_eq!(input.last_operation, Some("delete_next_word".to_string()));
    }

    // ===========================================
    // handle_text_input_key tests - Line deletion
    // ===========================================

    #[test]
    fn test_handle_ctrl_u_delete_to_head() {
        let mut input = MockTextInput::with_text("hello");
        let result = handle_text_input_key(&mut input, KeyCode::Char('u'), KeyModifiers::CTRL);
        assert!(result);
        assert_eq!(input.last_operation, Some("delete_line_by_head".to_string()));
    }

    #[test]
    fn test_handle_ctrl_k_delete_to_end() {
        let mut input = MockTextInput::with_text("hello");
        let result = handle_text_input_key(&mut input, KeyCode::Char('k'), KeyModifiers::CTRL);
        assert!(result);
        assert_eq!(input.last_operation, Some("delete_line_by_end".to_string()));
    }

    // ===========================================
    // handle_text_input_key tests - Unhandled keys
    // ===========================================

    #[test]
    fn test_handle_unhandled_key() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::F(5), KeyModifiers::NONE);
        assert!(!result);
    }

    #[test]
    fn test_handle_escape_unhandled() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!result);
    }

    #[test]
    fn test_handle_enter_unhandled() {
        let mut input = MockTextInput::new();
        let result = handle_text_input_key(&mut input, KeyCode::Enter, KeyModifiers::NONE);
        assert!(!result);
    }

    #[test]
    fn test_handle_ctrl_char_with_alt_unhandled() {
        let mut input = MockTextInput::new();
        // Char with alt modifier should not insert
        let modifiers = KeyModifiers {
            ctrl: false,
            shift: false,
            alt: true,
        };
        let result = handle_text_input_key(&mut input, KeyCode::Char('x'), modifiers);
        assert!(!result);
    }

    // ===========================================
    // TextEditor trait tests (auto-implemented)
    // ===========================================

    #[test]
    fn test_text_editor_select_all() {
        let mut input = MockTextInput::with_text("hello world");
        input.select_all();
        assert!(input.selection_active);
    }

    #[test]
    fn test_text_editor_copy() {
        let mut input = MockTextInput::with_text("hello");
        input.selection_active = true;
        let copied = input.copy();
        assert_eq!(copied, "hello");
        assert_eq!(input.yank_buffer, "hello");
    }

    #[test]
    fn test_text_editor_copy_no_selection() {
        let mut input = MockTextInput::with_text("hello");
        input.selection_active = false;
        let copied = input.copy();
        assert_eq!(copied, "");
        assert!(input.yank_buffer.is_empty());
    }

    #[test]
    fn test_text_editor_paste() {
        let mut input = MockTextInput::new();
        input.paste("pasted text");
        assert_eq!(input.text, "pasted text");
    }

    #[test]
    fn test_text_editor_undo_noop() {
        let mut input = MockTextInput::with_text("hello");
        input.undo(); // Should not panic (no-op by default)
        assert_eq!(input.text, "hello"); // Unchanged
    }

    #[test]
    fn test_text_editor_redo_noop() {
        let mut input = MockTextInput::with_text("hello");
        input.redo(); // Should not panic (no-op by default)
        assert_eq!(input.text, "hello"); // Unchanged
    }
}
