//! Bridge between egui events and frontend-agnostic input types.
//!
//! This module translates egui-specific event types into our UI-agnostic types
//! defined in `frontend::common::input`. This allows the rest of the application to
//! work with platform-independent input representations.

use eframe::egui;
use crate::frontend::common::{KeyCode, KeyEvent, KeyModifiers};

/// Convert egui Key to frontend-agnostic KeyCode
pub fn convert_egui_key(key: egui::Key) -> Option<KeyCode> {
    match key {
        // Arrow keys
        egui::Key::ArrowLeft => Some(KeyCode::Left),
        egui::Key::ArrowRight => Some(KeyCode::Right),
        egui::Key::ArrowUp => Some(KeyCode::Up),
        egui::Key::ArrowDown => Some(KeyCode::Down),

        // Navigation keys
        egui::Key::Home => Some(KeyCode::Home),
        egui::Key::End => Some(KeyCode::End),
        egui::Key::PageUp => Some(KeyCode::PageUp),
        egui::Key::PageDown => Some(KeyCode::PageDown),

        // Control keys
        egui::Key::Escape => Some(KeyCode::Esc),
        egui::Key::Tab => Some(KeyCode::Tab),
        egui::Key::Backspace => Some(KeyCode::Backspace),
        egui::Key::Enter => Some(KeyCode::Enter),
        egui::Key::Insert => Some(KeyCode::Insert),
        egui::Key::Delete => Some(KeyCode::Delete),
        egui::Key::Space => Some(KeyCode::Char(' ')),

        // Function keys
        egui::Key::F1 => Some(KeyCode::F(1)),
        egui::Key::F2 => Some(KeyCode::F(2)),
        egui::Key::F3 => Some(KeyCode::F(3)),
        egui::Key::F4 => Some(KeyCode::F(4)),
        egui::Key::F5 => Some(KeyCode::F(5)),
        egui::Key::F6 => Some(KeyCode::F(6)),
        egui::Key::F7 => Some(KeyCode::F(7)),
        egui::Key::F8 => Some(KeyCode::F(8)),
        egui::Key::F9 => Some(KeyCode::F(9)),
        egui::Key::F10 => Some(KeyCode::F(10)),
        egui::Key::F11 => Some(KeyCode::F(11)),
        egui::Key::F12 => Some(KeyCode::F(12)),
        egui::Key::F13 => Some(KeyCode::F(13)),
        egui::Key::F14 => Some(KeyCode::F(14)),
        egui::Key::F15 => Some(KeyCode::F(15)),
        egui::Key::F16 => Some(KeyCode::F(16)),
        egui::Key::F17 => Some(KeyCode::F(17)),
        egui::Key::F18 => Some(KeyCode::F(18)),
        egui::Key::F19 => Some(KeyCode::F(19)),
        egui::Key::F20 => Some(KeyCode::F(20)),

        // Letter keys (egui reports these as named variants)
        egui::Key::A => Some(KeyCode::Char('a')),
        egui::Key::B => Some(KeyCode::Char('b')),
        egui::Key::C => Some(KeyCode::Char('c')),
        egui::Key::D => Some(KeyCode::Char('d')),
        egui::Key::E => Some(KeyCode::Char('e')),
        egui::Key::F => Some(KeyCode::Char('f')),
        egui::Key::G => Some(KeyCode::Char('g')),
        egui::Key::H => Some(KeyCode::Char('h')),
        egui::Key::I => Some(KeyCode::Char('i')),
        egui::Key::J => Some(KeyCode::Char('j')),
        egui::Key::K => Some(KeyCode::Char('k')),
        egui::Key::L => Some(KeyCode::Char('l')),
        egui::Key::M => Some(KeyCode::Char('m')),
        egui::Key::N => Some(KeyCode::Char('n')),
        egui::Key::O => Some(KeyCode::Char('o')),
        egui::Key::P => Some(KeyCode::Char('p')),
        egui::Key::Q => Some(KeyCode::Char('q')),
        egui::Key::R => Some(KeyCode::Char('r')),
        egui::Key::S => Some(KeyCode::Char('s')),
        egui::Key::T => Some(KeyCode::Char('t')),
        egui::Key::U => Some(KeyCode::Char('u')),
        egui::Key::V => Some(KeyCode::Char('v')),
        egui::Key::W => Some(KeyCode::Char('w')),
        egui::Key::X => Some(KeyCode::Char('x')),
        egui::Key::Y => Some(KeyCode::Char('y')),
        egui::Key::Z => Some(KeyCode::Char('z')),

        // Number keys (main keyboard)
        egui::Key::Num0 => Some(KeyCode::Char('0')),
        egui::Key::Num1 => Some(KeyCode::Char('1')),
        egui::Key::Num2 => Some(KeyCode::Char('2')),
        egui::Key::Num3 => Some(KeyCode::Char('3')),
        egui::Key::Num4 => Some(KeyCode::Char('4')),
        egui::Key::Num5 => Some(KeyCode::Char('5')),
        egui::Key::Num6 => Some(KeyCode::Char('6')),
        egui::Key::Num7 => Some(KeyCode::Char('7')),
        egui::Key::Num8 => Some(KeyCode::Char('8')),
        egui::Key::Num9 => Some(KeyCode::Char('9')),

        // Symbol keys
        egui::Key::Minus => Some(KeyCode::Char('-')),
        egui::Key::Plus => Some(KeyCode::Char('+')),
        egui::Key::Equals => Some(KeyCode::Char('=')),
        egui::Key::OpenBracket => Some(KeyCode::Char('[')),
        egui::Key::CloseBracket => Some(KeyCode::Char(']')),
        egui::Key::Backslash => Some(KeyCode::Char('\\')),
        egui::Key::Semicolon => Some(KeyCode::Char(';')),
        egui::Key::Quote => Some(KeyCode::Char('\'')),
        egui::Key::Comma => Some(KeyCode::Char(','),),
        egui::Key::Period => Some(KeyCode::Char('.')),
        egui::Key::Slash => Some(KeyCode::Char('/')),
        egui::Key::Backtick => Some(KeyCode::Char('`')),

        _ => None,
    }
}

/// Convert egui Modifiers to frontend-agnostic KeyModifiers
pub fn convert_egui_modifiers(mods: &egui::Modifiers) -> KeyModifiers {
    KeyModifiers {
        ctrl: mods.ctrl || mods.command, // Treat Cmd as Ctrl for cross-platform
        shift: mods.shift,
        alt: mods.alt,
    }
}

/// Create a KeyEvent from egui key and modifiers
/// Note: egui does NOT distinguish numpad keys from main row keys - they use the same
/// Key::Num0-Num9 values. Numpad-specific keybinds won't work in GUI mode.
pub fn create_key_event(key: egui::Key, modifiers: &egui::Modifiers) -> Option<KeyEvent> {
    let code = convert_egui_key(key)?;
    let mods = convert_egui_modifiers(modifiers);

    // Handle Shift+Tab -> BackTab conversion
    if code == KeyCode::Tab && mods.shift {
        return Some(KeyEvent {
            code: KeyCode::BackTab,
            modifiers: KeyModifiers {
                ctrl: mods.ctrl,
                shift: false, // BackTab already implies shift
                alt: mods.alt,
            },
        });
    }

    Some(KeyEvent {
        code,
        modifiers: mods,
    })
}

/// Extract all key presses from the current frame's input
/// Returns a Vec of KeyEvents for all keys pressed this frame
pub fn get_key_events(ctx: &egui::Context) -> Vec<KeyEvent> {
    let mut events = Vec::new();

    ctx.input(|i| {
        // Check each key that was pressed this frame
        for event in &i.events {
            if let egui::Event::Key {
                key,
                pressed,
                modifiers: event_mods,
                ..
            } = event
            {
                if *pressed {
                    if let Some(key_event) = create_key_event(*key, event_mods) {
                        events.push(key_event);
                    }
                }
            }
        }
    });

    events
}

/// Check if a specific key was pressed this frame
pub fn key_pressed(ctx: &egui::Context, key: egui::Key) -> bool {
    ctx.input(|i| i.key_pressed(key))
}

/// Check if a specific key is currently held down
pub fn key_down(ctx: &egui::Context, key: egui::Key) -> bool {
    ctx.input(|i| i.key_down(key))
}

/// Get the current modifier state
pub fn get_modifiers(ctx: &egui::Context) -> KeyModifiers {
    ctx.input(|i| convert_egui_modifiers(&i.modifiers))
}

/// Convert a KeyEvent back to a string representation for keybind lookup
/// This produces strings compatible with parse_key_string in config/keybinds.rs
pub fn key_event_to_keybind_string(event: &KeyEvent) -> String {
    let mut parts = Vec::new();

    // Add modifiers in consistent order
    if event.modifiers.ctrl {
        parts.push("ctrl");
    }
    if event.modifiers.alt {
        parts.push("alt");
    }
    if event.modifiers.shift {
        parts.push("shift");
    }

    // Add the key name
    let key_name = match event.code {
        KeyCode::Char(c) => {
            // Handle special characters
            match c {
                ' ' => "space".to_string(),
                _ => c.to_string(),
            }
        }
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "shift+tab".to_string(), // Special handling
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::F(n) => format!("f{}", n),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Null => "null".to_string(),
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
        KeyCode::KeypadEnter => "enter".to_string(), // Treat as regular enter
    };

    // BackTab already includes shift in the name
    if event.code == KeyCode::BackTab {
        return key_name;
    }

    if parts.is_empty() {
        key_name
    } else {
        parts.push(&key_name);
        parts.join("+")
    }
}

/// Convert eframe NumpadKeyEvent to frontend-agnostic KeyEvent
///
/// This converts numpad key events from the forked eframe (which intercepts them
/// before egui-winit loses the KeyLocation::Numpad information) into our
/// platform-independent KeyEvent type.
///
/// Returns None if:
/// - NumLock is ON (key should go to text input instead)
/// - The key is not a numpad key we recognize
pub fn convert_numpad_key_event(numpad_event: &eframe::NumpadKeyEvent) -> Option<KeyEvent> {
    // When NumLock is on, let egui handle the key for text input
    if numpad_event.numlock_on {
        return None;
    }

    use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};

    let code = match numpad_event.physical_key {
        PhysicalKey::Code(WinitKeyCode::Numpad0) => KeyCode::Keypad0,
        PhysicalKey::Code(WinitKeyCode::Numpad1) => KeyCode::Keypad1,
        PhysicalKey::Code(WinitKeyCode::Numpad2) => KeyCode::Keypad2,
        PhysicalKey::Code(WinitKeyCode::Numpad3) => KeyCode::Keypad3,
        PhysicalKey::Code(WinitKeyCode::Numpad4) => KeyCode::Keypad4,
        PhysicalKey::Code(WinitKeyCode::Numpad5) => KeyCode::Keypad5,
        PhysicalKey::Code(WinitKeyCode::Numpad6) => KeyCode::Keypad6,
        PhysicalKey::Code(WinitKeyCode::Numpad7) => KeyCode::Keypad7,
        PhysicalKey::Code(WinitKeyCode::Numpad8) => KeyCode::Keypad8,
        PhysicalKey::Code(WinitKeyCode::Numpad9) => KeyCode::Keypad9,
        PhysicalKey::Code(WinitKeyCode::NumpadAdd) => KeyCode::KeypadPlus,
        PhysicalKey::Code(WinitKeyCode::NumpadSubtract) => KeyCode::KeypadMinus,
        PhysicalKey::Code(WinitKeyCode::NumpadMultiply) => KeyCode::KeypadMultiply,
        PhysicalKey::Code(WinitKeyCode::NumpadDivide) => KeyCode::KeypadDivide,
        PhysicalKey::Code(WinitKeyCode::NumpadEnter) => KeyCode::KeypadEnter,
        PhysicalKey::Code(WinitKeyCode::NumpadDecimal) => KeyCode::KeypadPeriod,
        _ => return None,
    };

    // Convert egui modifiers to our KeyModifiers
    let modifiers = KeyModifiers {
        ctrl: numpad_event.modifiers.ctrl,
        alt: numpad_event.modifiers.alt,
        shift: numpad_event.modifiers.shift,
    };

    Some(KeyEvent { code, modifiers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_egui_modifiers() {
        let mods = egui::Modifiers {
            ctrl: true,
            shift: false,
            alt: true,
            command: false,
            mac_cmd: false,
        };
        let converted = convert_egui_modifiers(&mods);
        assert!(converted.ctrl);
        assert!(!converted.shift);
        assert!(converted.alt);
    }

    #[test]
    fn test_convert_egui_key_letters() {
        assert_eq!(convert_egui_key(egui::Key::A), Some(KeyCode::Char('a')));
        assert_eq!(convert_egui_key(egui::Key::Z), Some(KeyCode::Char('z')));
    }

    #[test]
    fn test_convert_egui_key_arrows() {
        assert_eq!(convert_egui_key(egui::Key::ArrowUp), Some(KeyCode::Up));
        assert_eq!(convert_egui_key(egui::Key::ArrowDown), Some(KeyCode::Down));
        assert_eq!(convert_egui_key(egui::Key::ArrowLeft), Some(KeyCode::Left));
        assert_eq!(convert_egui_key(egui::Key::ArrowRight), Some(KeyCode::Right));
    }

    #[test]
    fn test_convert_egui_key_function() {
        assert_eq!(convert_egui_key(egui::Key::F1), Some(KeyCode::F(1)));
        assert_eq!(convert_egui_key(egui::Key::F12), Some(KeyCode::F(12)));
    }

    #[test]
    fn test_key_event_to_keybind_string() {
        let event = KeyEvent {
            code: KeyCode::Char('f'),
            modifiers: KeyModifiers { ctrl: true, shift: false, alt: false },
        };
        assert_eq!(key_event_to_keybind_string(&event), "ctrl+f");

        let event2 = KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(key_event_to_keybind_string(&event2), "enter");

        let event3 = KeyEvent {
            code: KeyCode::BackTab,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(key_event_to_keybind_string(&event3), "shift+tab");
    }
}
