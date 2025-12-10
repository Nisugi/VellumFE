//! Shared action vocabulary for the configuration popups and browsers.
//!
//! Translates raw `KeyEvent`s and textual keybinds into semantic `MenuAction`s
//! so every widget can react consistently regardless of current context.

use crate::frontend::common::{KeyCode, KeyEvent};

/// All possible menu/widget actions
#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    // Navigation
    NavigateUp,
    NavigateDown,
    NavigateLeft,
    NavigateRight,
    PageUp,
    PageDown,
    Home,
    End,

    // Item Navigation (browsers - alternative naming)
    NextItem,     // Same as NavigateDown for browser context
    PreviousItem, // Same as NavigateUp for browser context
    NextPage,     // Same as PageDown for browser context
    PreviousPage, // Same as PageUp for browser context

    // Field Navigation (forms)
    NextField,
    PreviousField,

    // Selection/Confirmation
    Select, // Enter - select item or accept dropdown
    Cancel, // Esc - close widget

    // Editing
    Save,   // Ctrl+s
    Delete, // Delete key or Ctrl+D

    // Text Editing (always available in TextAreas)
    SelectAll, // Ctrl+A
    Copy,      // Ctrl+C
    Cut,       // Ctrl+X
    Paste,     // Ctrl+V

    // Toggles/Cycling
    Toggle,        // Space - toggle boolean
    ToggleFilter,  // 'F' - toggle filter in browsers
    CycleForward,  // Right arrow - cycle dropdown forward
    CycleBackward, // Left arrow - cycle dropdown backward

    // Reordering (WindowEditor)
    MoveUp,   // Shift+Up
    MoveDown, // Shift+Down

    // List Management (WindowEditor)
    Add,  // 'A'
    Edit, // 'E'
    New,  // 'N' - Alternative naming for Add

    // No action (key not bound or not applicable in this context)
    None,
}

/// Context for action resolution - determines which actions are valid
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionContext {
    Browser,        // In a browser widget (navigate + select/delete)
    Form,           // In a form widget (field nav + save/cancel)
    TextInput,      // Focused on a TextArea field (clipboard ops)
    Dropdown,       // Focused on dropdown field (up/down cycles)
    SettingsEditor, // In settings editor (hybrid navigation/editing)
    WindowEditor,   // In window editor (most complex - all actions)
}

/// Convert KeyEvent to string representation for matching against keybinds
pub fn key_event_to_string(key: KeyEvent) -> String {
    let mut parts = Vec::new();

    // Add modifiers (lowercase to match keybinds.toml convention)
    if key.modifiers.ctrl {
        parts.push("ctrl");
    }
    if key.modifiers.shift {
        parts.push("shift");
    }
    if key.modifiers.alt {
        parts.push("alt");
    }

    // Add key code (lowercase to match keybinds.toml convention)
    let key_str = match key.code {
        KeyCode::Char(c) => {
            // Always lowercase for consistent comparisons
            c.to_ascii_lowercase().to_string()
        }
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => {
            // BackTab is Shift+Tab - return the full key string
            return "shift+tab".to_string();
        }
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::F(n) => format!("f{}", n),
        // Keypad keys (lowercase to match keybinds.toml)
        KeyCode::Keypad0 => "num_0".to_string(),
        KeyCode::Keypad1 => "num_1".to_string(),
        KeyCode::Keypad2 => "num_2".to_string(),
        KeyCode::Keypad3 => "num_3".to_string(),
        KeyCode::Keypad4 => "num_4".to_string(),
        KeyCode::Keypad5 => "num_5".to_string(),
        KeyCode::Keypad6 => "num_6".to_string(),
        KeyCode::Keypad7 => "num_7".to_string(),
        KeyCode::Keypad8 => "num_8".to_string(),
        KeyCode::Keypad9 => "num_9".to_string(),
        KeyCode::KeypadPeriod => "num_.".to_string(),
        KeyCode::KeypadPlus => "num_+".to_string(),
        KeyCode::KeypadMinus => "num_-".to_string(),
        KeyCode::KeypadMultiply => "num_*".to_string(),
        KeyCode::KeypadDivide => "num_/".to_string(),
        KeyCode::KeypadEnter => "num_enter".to_string(),
        KeyCode::Null => return String::new(), // Null key
    };

    parts.push(&key_str);
    parts.join("+")
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::common::KeyModifiers;

    #[test]
    fn test_key_event_to_string() {
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CTRL);
        assert_eq!(key_event_to_string(key), "ctrl+s");

        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT);
        assert_eq!(key_event_to_string(key), "shift+up");

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_event_to_string(key), "enter");
    }
}
