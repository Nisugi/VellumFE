//! Shared action vocabulary for the configuration popups and browsers.
//!
//! Translates raw `KeyEvent`s and textual keybinds into semantic `MenuAction`s
//! so every widget can react consistently regardless of current context.

use crate::data::input::{KeyCode, KeyEvent};

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
        KeyCode::Null => return String::new(), // Null key
        // Numpad keys use the canonical word form ("num_plus"), which contains no
        // '+' and so round-trips through the modifier parser.
        code => match code.keypad_name() {
            Some(name) => name.to_string(),
            None => return String::new(),
        },
    };

    parts.push(&key_str);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::input::KeyModifiers;

    #[test]
    fn test_key_event_to_string() {
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CTRL);
        assert_eq!(key_event_to_string(key), "ctrl+s");

        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT);
        assert_eq!(key_event_to_string(key), "shift+up");

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_event_to_string(key), "enter");
    }

    #[test]
    fn keypad_keys_use_canonical_word_form() {
        let key = KeyEvent::new(KeyCode::KeypadPlus, KeyModifiers::NONE);
        assert_eq!(key_event_to_string(key), "num_plus");

        let key = KeyEvent::new(KeyCode::KeypadPeriod, KeyModifiers::NONE);
        assert_eq!(key_event_to_string(key), "num_decimal");

        let key = KeyEvent::new(KeyCode::Keypad7, KeyModifiers::NONE);
        assert_eq!(key_event_to_string(key), "num_7");
    }

    /// The invariant that makes a recorded chord actually fire: whatever the editor
    /// writes for a key event must parse back into that same key event. Symbol names
    /// broke this - "ctrl+num_+" was emitted but could not be parsed.
    #[test]
    fn keypad_chords_round_trip_through_the_parser() {
        let codes = [
            KeyCode::Keypad0,
            KeyCode::Keypad1,
            KeyCode::Keypad2,
            KeyCode::Keypad3,
            KeyCode::Keypad4,
            KeyCode::Keypad5,
            KeyCode::Keypad6,
            KeyCode::Keypad7,
            KeyCode::Keypad8,
            KeyCode::Keypad9,
            KeyCode::KeypadPlus,
            KeyCode::KeypadMinus,
            KeyCode::KeypadMultiply,
            KeyCode::KeypadDivide,
            KeyCode::KeypadPeriod,
            KeyCode::KeypadEnter,
        ];

        for code in codes {
            for modifiers in [
                KeyModifiers::NONE,
                KeyModifiers {
                    ctrl: true,
                    alt: false,
                    shift: false,
                },
                KeyModifiers {
                    ctrl: false,
                    alt: true,
                    shift: false,
                },
                KeyModifiers {
                    ctrl: false,
                    alt: false,
                    shift: true,
                },
                KeyModifiers {
                    ctrl: true,
                    alt: true,
                    shift: true,
                },
            ] {
                let event = KeyEvent::new(code, modifiers);
                let rendered = key_event_to_string(event);
                let (parsed_code, parsed_mods) = crate::config::parse_key_string(&rendered)
                    .unwrap_or_else(|| panic!("{rendered:?} did not parse back"));

                assert_eq!(parsed_code, code, "{rendered:?} lost its key code");
                assert_eq!(parsed_mods.ctrl, modifiers.ctrl, "{rendered:?} ctrl");
                assert_eq!(parsed_mods.alt, modifiers.alt, "{rendered:?} alt");
                assert_eq!(parsed_mods.shift, modifiers.shift, "{rendered:?} shift");
            }
        }
    }
}
