//! Frontend-agnostic input types for keyboard and mouse events.
//!
//! These types abstract over platform-specific input handling (crossterm for TUI,
//! native events for GUI) to enable shared input processing logic.

/// Represents a key press, independent of the underlying frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// A character key (a-z, 0-9, symbols, etc.)
    Char(char),
    /// Backspace key
    Backspace,
    /// Enter/Return key
    Enter,
    /// Left arrow
    Left,
    /// Right arrow
    Right,
    /// Up arrow
    Up,
    /// Down arrow
    Down,
    /// Home key
    Home,
    /// End key
    End,
    /// Page Up
    PageUp,
    /// Page Down
    PageDown,
    /// Tab key
    Tab,
    /// Shift+Tab (reverse tab)
    BackTab,
    /// Delete key
    Delete,
    /// Insert key
    Insert,
    /// Function keys (F1-F12)
    F(u8),
    /// Escape key
    Esc,
    /// Null (no-op)
    Null,
    /// Keypad 0
    Keypad0,
    /// Keypad 1
    Keypad1,
    /// Keypad 2
    Keypad2,
    /// Keypad 3
    Keypad3,
    /// Keypad 4
    Keypad4,
    /// Keypad 5
    Keypad5,
    /// Keypad 6
    Keypad6,
    /// Keypad 7
    Keypad7,
    /// Keypad 8
    Keypad8,
    /// Keypad 9
    Keypad9,
    /// Keypad period/decimal
    KeypadPeriod,
    /// Keypad plus
    KeypadPlus,
    /// Keypad minus
    KeypadMinus,
    /// Keypad multiply/asterisk
    KeypadMultiply,
    /// Keypad divide/slash
    KeypadDivide,
    /// Keypad Enter
    KeypadEnter,
}

/// A keyboard event combining a key code and modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyEvent {
    /// Create a new key event
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    /// Create a key event with no modifiers
    pub fn from_code(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::NONE,
        }
    }
}

/// Keyboard modifiers (Ctrl, Shift, Alt).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl KeyModifiers {
    /// No modifiers pressed
    pub const NONE: Self = KeyModifiers {
        ctrl: false,
        shift: false,
        alt: false,
    };

    /// Only Ctrl pressed
    pub const CTRL: Self = KeyModifiers {
        ctrl: true,
        shift: false,
        alt: false,
    };

    /// Only Shift pressed
    pub const SHIFT: Self = KeyModifiers {
        ctrl: false,
        shift: true,
        alt: false,
    };

    /// Only Alt pressed
    pub const ALT: Self = KeyModifiers {
        ctrl: false,
        shift: false,
        alt: true,
    };

    /// Check if any modifiers are active
    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.shift && !self.alt
    }

    /// Check if Ctrl is pressed (regardless of other modifiers)
    pub fn contains_ctrl(&self) -> bool {
        self.ctrl
    }

    /// Check if Shift is pressed (regardless of other modifiers)
    pub fn contains_shift(&self) -> bool {
        self.shift
    }

    /// Check if Alt is pressed (regardless of other modifiers)
    pub fn contains_alt(&self) -> bool {
        self.alt
    }
}

/// Mouse button types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Mouse event kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventKind {
    /// Mouse button pressed
    Down(MouseButton),
    /// Mouse button released
    Up(MouseButton),
    /// Mouse dragged (button held while moving)
    Drag(MouseButton),
    /// Mouse moved without button pressed
    Moved,
    /// Scroll wheel up
    ScrollUp,
    /// Scroll wheel down
    ScrollDown,
}

/// A mouse event with position and modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub column: u16,
    pub row: u16,
    pub modifiers: KeyModifiers,
}

impl MouseEvent {
    /// Create a new mouse event
    pub fn new(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> Self {
        Self {
            kind,
            column,
            row,
            modifiers,
        }
    }

    /// Get the button if this is a button event
    pub fn button(&self) -> Option<MouseButton> {
        match self.kind {
            MouseEventKind::Down(btn) | MouseEventKind::Up(btn) | MouseEventKind::Drag(btn) => {
                Some(btn)
            }
            _ => None,
        }
    }

    /// Check if this is a left click (down event)
    pub fn is_left_click(&self) -> bool {
        matches!(self.kind, MouseEventKind::Down(MouseButton::Left))
    }

    /// Check if this is a right click (down event)
    pub fn is_right_click(&self) -> bool {
        matches!(self.kind, MouseEventKind::Down(MouseButton::Right))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // KeyCode tests
    // ===========================================

    #[test]
    fn test_keycode_char() {
        let key = KeyCode::Char('a');
        assert!(matches!(key, KeyCode::Char('a')));
    }

    #[test]
    fn test_keycode_char_different() {
        let key_a = KeyCode::Char('a');
        let key_b = KeyCode::Char('b');
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn test_keycode_function_keys() {
        assert!(matches!(KeyCode::F(1), KeyCode::F(1)));
        assert_ne!(KeyCode::F(1), KeyCode::F(2));
        assert!(matches!(KeyCode::F(12), KeyCode::F(12)));
    }

    #[test]
    fn test_keycode_navigation() {
        assert!(matches!(KeyCode::Up, KeyCode::Up));
        assert!(matches!(KeyCode::Down, KeyCode::Down));
        assert!(matches!(KeyCode::Left, KeyCode::Left));
        assert!(matches!(KeyCode::Right, KeyCode::Right));
    }

    #[test]
    fn test_keycode_special_keys() {
        assert!(matches!(KeyCode::Enter, KeyCode::Enter));
        assert!(matches!(KeyCode::Backspace, KeyCode::Backspace));
        assert!(matches!(KeyCode::Tab, KeyCode::Tab));
        assert!(matches!(KeyCode::BackTab, KeyCode::BackTab));
        assert!(matches!(KeyCode::Esc, KeyCode::Esc));
        assert!(matches!(KeyCode::Delete, KeyCode::Delete));
        assert!(matches!(KeyCode::Insert, KeyCode::Insert));
    }

    #[test]
    fn test_keycode_page_keys() {
        assert!(matches!(KeyCode::Home, KeyCode::Home));
        assert!(matches!(KeyCode::End, KeyCode::End));
        assert!(matches!(KeyCode::PageUp, KeyCode::PageUp));
        assert!(matches!(KeyCode::PageDown, KeyCode::PageDown));
    }

    #[test]
    fn test_keycode_keypad() {
        assert!(matches!(KeyCode::Keypad0, KeyCode::Keypad0));
        assert!(matches!(KeyCode::Keypad5, KeyCode::Keypad5));
        assert!(matches!(KeyCode::Keypad9, KeyCode::Keypad9));
        assert!(matches!(KeyCode::KeypadPlus, KeyCode::KeypadPlus));
        assert!(matches!(KeyCode::KeypadMinus, KeyCode::KeypadMinus));
        assert!(matches!(KeyCode::KeypadMultiply, KeyCode::KeypadMultiply));
        assert!(matches!(KeyCode::KeypadDivide, KeyCode::KeypadDivide));
        assert!(matches!(KeyCode::KeypadEnter, KeyCode::KeypadEnter));
        assert!(matches!(KeyCode::KeypadPeriod, KeyCode::KeypadPeriod));
    }

    #[test]
    fn test_keycode_clone() {
        let key = KeyCode::Char('x');
        let cloned = key;
        assert_eq!(key, cloned);
    }

    #[test]
    fn test_keycode_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(KeyCode::Char('a'));
        set.insert(KeyCode::Enter);
        set.insert(KeyCode::F(5));
        assert_eq!(set.len(), 3);
        assert!(set.contains(&KeyCode::Char('a')));
        assert!(set.contains(&KeyCode::Enter));
    }

    // ===========================================
    // KeyModifiers tests
    // ===========================================

    #[test]
    fn test_key_modifiers_none() {
        let mods = KeyModifiers::NONE;
        assert!(!mods.ctrl);
        assert!(!mods.shift);
        assert!(!mods.alt);
        assert!(mods.is_empty());
    }

    #[test]
    fn test_key_modifiers_ctrl() {
        let mods = KeyModifiers::CTRL;
        assert!(mods.ctrl);
        assert!(!mods.shift);
        assert!(!mods.alt);
        assert!(!mods.is_empty());
        assert!(mods.contains_ctrl());
    }

    #[test]
    fn test_key_modifiers_shift() {
        let mods = KeyModifiers::SHIFT;
        assert!(!mods.ctrl);
        assert!(mods.shift);
        assert!(!mods.alt);
        assert!(!mods.is_empty());
        assert!(mods.contains_shift());
    }

    #[test]
    fn test_key_modifiers_alt() {
        let mods = KeyModifiers::ALT;
        assert!(!mods.ctrl);
        assert!(!mods.shift);
        assert!(mods.alt);
        assert!(!mods.is_empty());
        assert!(mods.contains_alt());
    }

    #[test]
    fn test_key_modifiers_combined() {
        let mods = KeyModifiers {
            ctrl: true,
            shift: true,
            alt: false,
        };
        assert!(mods.contains_ctrl());
        assert!(mods.contains_shift());
        assert!(!mods.contains_alt());
        assert!(!mods.is_empty());
    }

    #[test]
    fn test_key_modifiers_all() {
        let mods = KeyModifiers {
            ctrl: true,
            shift: true,
            alt: true,
        };
        assert!(mods.contains_ctrl());
        assert!(mods.contains_shift());
        assert!(mods.contains_alt());
    }

    #[test]
    fn test_key_modifiers_default() {
        let mods = KeyModifiers::default();
        assert!(mods.is_empty());
        assert_eq!(mods, KeyModifiers::NONE);
    }

    #[test]
    fn test_key_modifiers_equality() {
        assert_eq!(KeyModifiers::NONE, KeyModifiers::NONE);
        assert_eq!(KeyModifiers::CTRL, KeyModifiers::CTRL);
        assert_ne!(KeyModifiers::CTRL, KeyModifiers::SHIFT);
        assert_ne!(KeyModifiers::ALT, KeyModifiers::NONE);
    }

    #[test]
    fn test_key_modifiers_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(KeyModifiers::NONE);
        set.insert(KeyModifiers::CTRL);
        set.insert(KeyModifiers::SHIFT);
        assert_eq!(set.len(), 3);
    }

    // ===========================================
    // KeyEvent tests
    // ===========================================

    #[test]
    fn test_key_event_new() {
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CTRL);
        assert_eq!(event.code, KeyCode::Char('c'));
        assert!(event.modifiers.contains_ctrl());
    }

    #[test]
    fn test_key_event_from_code() {
        let event = KeyEvent::from_code(KeyCode::Enter);
        assert_eq!(event.code, KeyCode::Enter);
        assert!(event.modifiers.is_empty());
    }

    #[test]
    fn test_key_event_equality() {
        let event1 = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let event2 = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let event3 = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SHIFT);

        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
    }

    #[test]
    fn test_key_event_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(KeyEvent::from_code(KeyCode::Char('a')));
        set.insert(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CTRL));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_key_event_clone() {
        let event = KeyEvent::new(KeyCode::F(5), KeyModifiers::ALT);
        let cloned = event;
        assert_eq!(event, cloned);
    }

    // ===========================================
    // MouseButton tests
    // ===========================================

    #[test]
    fn test_mouse_button_variants() {
        assert!(matches!(MouseButton::Left, MouseButton::Left));
        assert!(matches!(MouseButton::Right, MouseButton::Right));
        assert!(matches!(MouseButton::Middle, MouseButton::Middle));
    }

    #[test]
    fn test_mouse_button_inequality() {
        assert_ne!(MouseButton::Left, MouseButton::Right);
        assert_ne!(MouseButton::Left, MouseButton::Middle);
        assert_ne!(MouseButton::Right, MouseButton::Middle);
    }

    #[test]
    fn test_mouse_button_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(MouseButton::Left);
        set.insert(MouseButton::Right);
        set.insert(MouseButton::Middle);
        assert_eq!(set.len(), 3);
    }

    // ===========================================
    // MouseEventKind tests
    // ===========================================

    #[test]
    fn test_mouse_event_kind_down() {
        let kind = MouseEventKind::Down(MouseButton::Left);
        assert!(matches!(kind, MouseEventKind::Down(MouseButton::Left)));
    }

    #[test]
    fn test_mouse_event_kind_up() {
        let kind = MouseEventKind::Up(MouseButton::Right);
        assert!(matches!(kind, MouseEventKind::Up(MouseButton::Right)));
    }

    #[test]
    fn test_mouse_event_kind_drag() {
        let kind = MouseEventKind::Drag(MouseButton::Left);
        assert!(matches!(kind, MouseEventKind::Drag(MouseButton::Left)));
    }

    #[test]
    fn test_mouse_event_kind_moved() {
        let kind = MouseEventKind::Moved;
        assert!(matches!(kind, MouseEventKind::Moved));
    }

    #[test]
    fn test_mouse_event_kind_scroll() {
        assert!(matches!(MouseEventKind::ScrollUp, MouseEventKind::ScrollUp));
        assert!(matches!(MouseEventKind::ScrollDown, MouseEventKind::ScrollDown));
    }

    // ===========================================
    // MouseEvent tests
    // ===========================================

    #[test]
    fn test_mouse_event_new() {
        let event = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            10,
            20,
            KeyModifiers::NONE,
        );
        assert_eq!(event.column, 10);
        assert_eq!(event.row, 20);
        assert!(matches!(event.kind, MouseEventKind::Down(MouseButton::Left)));
    }

    #[test]
    fn test_mouse_event_with_modifiers() {
        let event = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Right),
            5,
            15,
            KeyModifiers::CTRL,
        );
        assert!(event.modifiers.contains_ctrl());
    }

    #[test]
    fn test_mouse_event_button_down() {
        let event = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            0,
            0,
            KeyModifiers::NONE,
        );
        assert_eq!(event.button(), Some(MouseButton::Left));
    }

    #[test]
    fn test_mouse_event_button_up() {
        let event = MouseEvent::new(
            MouseEventKind::Up(MouseButton::Right),
            0,
            0,
            KeyModifiers::NONE,
        );
        assert_eq!(event.button(), Some(MouseButton::Right));
    }

    #[test]
    fn test_mouse_event_button_drag() {
        let event = MouseEvent::new(
            MouseEventKind::Drag(MouseButton::Middle),
            0,
            0,
            KeyModifiers::NONE,
        );
        assert_eq!(event.button(), Some(MouseButton::Middle));
    }

    #[test]
    fn test_mouse_event_button_none() {
        let moved = MouseEvent::new(MouseEventKind::Moved, 0, 0, KeyModifiers::NONE);
        assert_eq!(moved.button(), None);

        let scroll_up = MouseEvent::new(MouseEventKind::ScrollUp, 0, 0, KeyModifiers::NONE);
        assert_eq!(scroll_up.button(), None);

        let scroll_down = MouseEvent::new(MouseEventKind::ScrollDown, 0, 0, KeyModifiers::NONE);
        assert_eq!(scroll_down.button(), None);
    }

    #[test]
    fn test_is_left_click() {
        let left_down = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            0,
            0,
            KeyModifiers::NONE,
        );
        assert!(left_down.is_left_click());

        let left_up = MouseEvent::new(
            MouseEventKind::Up(MouseButton::Left),
            0,
            0,
            KeyModifiers::NONE,
        );
        assert!(!left_up.is_left_click());

        let right_down = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Right),
            0,
            0,
            KeyModifiers::NONE,
        );
        assert!(!right_down.is_left_click());
    }

    #[test]
    fn test_is_right_click() {
        let right_down = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Right),
            0,
            0,
            KeyModifiers::NONE,
        );
        assert!(right_down.is_right_click());

        let right_up = MouseEvent::new(
            MouseEventKind::Up(MouseButton::Right),
            0,
            0,
            KeyModifiers::NONE,
        );
        assert!(!right_up.is_right_click());

        let left_down = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            0,
            0,
            KeyModifiers::NONE,
        );
        assert!(!left_down.is_right_click());
    }

    #[test]
    fn test_mouse_event_equality() {
        let event1 = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            10,
            20,
            KeyModifiers::NONE,
        );
        let event2 = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            10,
            20,
            KeyModifiers::NONE,
        );
        let event3 = MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            10,
            21,
            KeyModifiers::NONE,
        );

        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
    }

    #[test]
    fn test_mouse_event_position() {
        let event = MouseEvent::new(MouseEventKind::Moved, 100, 50, KeyModifiers::NONE);
        assert_eq!(event.column, 100);
        assert_eq!(event.row, 50);
    }

    #[test]
    fn test_mouse_event_max_position() {
        let event = MouseEvent::new(MouseEventKind::Moved, u16::MAX, u16::MAX, KeyModifiers::NONE);
        assert_eq!(event.column, u16::MAX);
        assert_eq!(event.row, u16::MAX);
    }
}
