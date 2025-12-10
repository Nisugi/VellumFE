//! Type bridge for converting between crossterm KeyEvents
//!
//! Converts `crossterm::event::KeyEvent` (from custom fork with Keypad support)
//! to `ratatui::crossterm::event::KeyEvent` (expected by tui-textarea)

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::crossterm::event as rt_event;

/// Convert a crossterm KeyEvent to a ratatui::crossterm KeyEvent for tui-textarea
pub fn to_textarea_event(key: KeyEvent) -> rt_event::KeyEvent {
    // Convert KeyCode
    let rt_code = match key.code {
        KeyCode::Backspace => rt_event::KeyCode::Backspace,
        KeyCode::Enter => rt_event::KeyCode::Enter,
        KeyCode::Left => rt_event::KeyCode::Left,
        KeyCode::Right => rt_event::KeyCode::Right,
        KeyCode::Up => rt_event::KeyCode::Up,
        KeyCode::Down => rt_event::KeyCode::Down,
        KeyCode::Home => rt_event::KeyCode::Home,
        KeyCode::End => rt_event::KeyCode::End,
        KeyCode::PageUp => rt_event::KeyCode::PageUp,
        KeyCode::PageDown => rt_event::KeyCode::PageDown,
        KeyCode::Tab => rt_event::KeyCode::Tab,
        KeyCode::BackTab => rt_event::KeyCode::BackTab,
        KeyCode::Delete => rt_event::KeyCode::Delete,
        KeyCode::Insert => rt_event::KeyCode::Insert,
        KeyCode::F(n) => rt_event::KeyCode::F(n),
        KeyCode::Char(c) => rt_event::KeyCode::Char(c),
        KeyCode::Null => rt_event::KeyCode::Null,
        KeyCode::Esc => rt_event::KeyCode::Esc,
        // Keypad variants from custom fork - map to regular equivalents for textarea
        KeyCode::Keypad0 => rt_event::KeyCode::Char('0'),
        KeyCode::Keypad1 => rt_event::KeyCode::Char('1'),
        KeyCode::Keypad2 => rt_event::KeyCode::Char('2'),
        KeyCode::Keypad3 => rt_event::KeyCode::Char('3'),
        KeyCode::Keypad4 => rt_event::KeyCode::Char('4'),
        KeyCode::Keypad5 => rt_event::KeyCode::Char('5'),
        KeyCode::Keypad6 => rt_event::KeyCode::Char('6'),
        KeyCode::Keypad7 => rt_event::KeyCode::Char('7'),
        KeyCode::Keypad8 => rt_event::KeyCode::Char('8'),
        KeyCode::Keypad9 => rt_event::KeyCode::Char('9'),
        KeyCode::KeypadPlus => rt_event::KeyCode::Char('+'),
        KeyCode::KeypadMinus => rt_event::KeyCode::Char('-'),
        KeyCode::KeypadMultiply => rt_event::KeyCode::Char('*'),
        KeyCode::KeypadDivide => rt_event::KeyCode::Char('/'),
        KeyCode::KeypadPeriod => rt_event::KeyCode::Char('.'),
        _ => rt_event::KeyCode::Null,
    };

    // Convert KeyModifiers
    let mut rt_modifiers = rt_event::KeyModifiers::empty();
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        rt_modifiers |= rt_event::KeyModifiers::SHIFT;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        rt_modifiers |= rt_event::KeyModifiers::CONTROL;
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        rt_modifiers |= rt_event::KeyModifiers::ALT;
    }

    rt_event::KeyEvent {
        code: rt_code,
        modifiers: rt_modifiers,
        kind: rt_event::KeyEventKind::Press,
        state: rt_event::KeyEventState::empty(),
    }
}
