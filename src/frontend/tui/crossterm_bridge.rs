//! Bridge between crossterm events and frontend-agnostic input types.
//!
//! This module translates crossterm-specific event types into our UI-agnostic types
//! defined in `frontend::common::input`. This allows the rest of the application to
//! work with platform-independent input representations.
//!
//! NOTE: Also provides reverse conversion (frontend -> crossterm) for legacy code
//! that still expects crossterm types. This is temporary until Phase 2 (Core Decoupling)
//! removes crossterm from core.

use crossterm::event as ct;
use ratatui::style as ratatui_style;
use crate::frontend::common::*;

/// Convert crossterm KeyCode to frontend-agnostic KeyCode
pub fn convert_keycode(code: ct::KeyCode) -> Option<KeyCode> {
    match code {
        ct::KeyCode::Char(c) => Some(KeyCode::Char(c)),
        ct::KeyCode::Backspace => Some(KeyCode::Backspace),
        ct::KeyCode::Enter => Some(KeyCode::Enter),
        ct::KeyCode::Left => Some(KeyCode::Left),
        ct::KeyCode::Right => Some(KeyCode::Right),
        ct::KeyCode::Up => Some(KeyCode::Up),
        ct::KeyCode::Down => Some(KeyCode::Down),
        ct::KeyCode::Home => Some(KeyCode::Home),
        ct::KeyCode::End => Some(KeyCode::End),
        ct::KeyCode::PageUp => Some(KeyCode::PageUp),
        ct::KeyCode::PageDown => Some(KeyCode::PageDown),
        ct::KeyCode::Tab => Some(KeyCode::Tab),
        ct::KeyCode::BackTab => Some(KeyCode::BackTab),
        ct::KeyCode::Delete => Some(KeyCode::Delete),
        ct::KeyCode::Insert => Some(KeyCode::Insert),
        ct::KeyCode::F(n) => Some(KeyCode::F(n)),
        ct::KeyCode::Esc => Some(KeyCode::Esc),
        ct::KeyCode::Null => Some(KeyCode::Null),
        // Keypad variants (from justinpopa/crossterm custom fork)
        ct::KeyCode::KeypadBegin => None, // Numlock off, num5 - not useful
        ct::KeyCode::KeypadEnter => Some(KeyCode::KeypadEnter),
        // Keypad number keys (numlock on)
        ct::KeyCode::Keypad0 => Some(KeyCode::Keypad0),
        ct::KeyCode::Keypad1 => Some(KeyCode::Keypad1),
        ct::KeyCode::Keypad2 => Some(KeyCode::Keypad2),
        ct::KeyCode::Keypad3 => Some(KeyCode::Keypad3),
        ct::KeyCode::Keypad4 => Some(KeyCode::Keypad4),
        ct::KeyCode::Keypad5 => Some(KeyCode::Keypad5),
        ct::KeyCode::Keypad6 => Some(KeyCode::Keypad6),
        ct::KeyCode::Keypad7 => Some(KeyCode::Keypad7),
        ct::KeyCode::Keypad8 => Some(KeyCode::Keypad8),
        ct::KeyCode::Keypad9 => Some(KeyCode::Keypad9),
        ct::KeyCode::KeypadPeriod => Some(KeyCode::KeypadPeriod),
        ct::KeyCode::KeypadPlus => Some(KeyCode::KeypadPlus),
        ct::KeyCode::KeypadMinus => Some(KeyCode::KeypadMinus),
        ct::KeyCode::KeypadMultiply => Some(KeyCode::KeypadMultiply),
        ct::KeyCode::KeypadDivide => Some(KeyCode::KeypadDivide),
        _ => None, // Ignore unsupported keys
    }
}

/// Convert crossterm KeyModifiers to frontend-agnostic KeyModifiers
pub fn convert_modifiers(mods: ct::KeyModifiers) -> KeyModifiers {
    KeyModifiers {
        ctrl: mods.contains(ct::KeyModifiers::CONTROL),
        shift: mods.contains(ct::KeyModifiers::SHIFT),
        alt: mods.contains(ct::KeyModifiers::ALT),
    }
}

/// Convert crossterm KeyEvent to frontend-agnostic KeyEvent
pub fn convert_key_event(event: ct::KeyEvent) -> Option<KeyEvent> {
    let code = convert_keycode(event.code)?;
    let modifiers = convert_modifiers(event.modifiers);
    Some(KeyEvent { code, modifiers })
}

/// Convert crossterm MouseEventKind to frontend-agnostic MouseEventKind
pub fn convert_mouse_kind(kind: ct::MouseEventKind) -> Option<MouseEventKind> {
    match kind {
        ct::MouseEventKind::Down(btn) => convert_mouse_button(btn).map(MouseEventKind::Down),
        ct::MouseEventKind::Up(btn) => convert_mouse_button(btn).map(MouseEventKind::Up),
        ct::MouseEventKind::Drag(btn) => convert_mouse_button(btn).map(MouseEventKind::Drag),
        ct::MouseEventKind::Moved => Some(MouseEventKind::Moved),
        ct::MouseEventKind::ScrollUp => Some(MouseEventKind::ScrollUp),
        ct::MouseEventKind::ScrollDown => Some(MouseEventKind::ScrollDown),
        _ => None,
    }
}

/// Convert crossterm MouseButton to frontend-agnostic MouseButton
pub fn convert_mouse_button(btn: ct::MouseButton) -> Option<MouseButton> {
    match btn {
        ct::MouseButton::Left => Some(MouseButton::Left),
        ct::MouseButton::Right => Some(MouseButton::Right),
        ct::MouseButton::Middle => Some(MouseButton::Middle),
    }
}

// ============================================================================
// REVERSE CONVERSIONS (Frontend -> Crossterm)
// These are temporary during Phase 1 to support legacy code that expects
// crossterm types. Will be removed in Phase 2 when core is fully decoupled.
// ============================================================================

/// Convert frontend-agnostic KeyCode back to crossterm KeyCode
/// Used by legacy code that still expects crossterm types
pub fn to_crossterm_keycode(code: KeyCode) -> ct::KeyCode {
    match code {
        KeyCode::Char(c) => ct::KeyCode::Char(c),
        KeyCode::Backspace => ct::KeyCode::Backspace,
        KeyCode::Enter => ct::KeyCode::Enter,
        KeyCode::Left => ct::KeyCode::Left,
        KeyCode::Right => ct::KeyCode::Right,
        KeyCode::Up => ct::KeyCode::Up,
        KeyCode::Down => ct::KeyCode::Down,
        KeyCode::Home => ct::KeyCode::Home,
        KeyCode::End => ct::KeyCode::End,
        KeyCode::PageUp => ct::KeyCode::PageUp,
        KeyCode::PageDown => ct::KeyCode::PageDown,
        KeyCode::Tab => ct::KeyCode::Tab,
        KeyCode::BackTab => ct::KeyCode::BackTab,
        KeyCode::Delete => ct::KeyCode::Delete,
        KeyCode::Insert => ct::KeyCode::Insert,
        KeyCode::F(n) => ct::KeyCode::F(n),
        KeyCode::Esc => ct::KeyCode::Esc,
        KeyCode::Null => ct::KeyCode::Null,
        // Keypad keys - map to char equivalents if crossterm doesn't have dedicated variants
        // This assumes numlock is on. If the custom fork has Keypad variants, update these.
        KeyCode::Keypad0 => ct::KeyCode::Char('0'),
        KeyCode::Keypad1 => ct::KeyCode::Char('1'),
        KeyCode::Keypad2 => ct::KeyCode::Char('2'),
        KeyCode::Keypad3 => ct::KeyCode::Char('3'),
        KeyCode::Keypad4 => ct::KeyCode::Char('4'),
        KeyCode::Keypad5 => ct::KeyCode::Char('5'),
        KeyCode::Keypad6 => ct::KeyCode::Char('6'),
        KeyCode::Keypad7 => ct::KeyCode::Char('7'),
        KeyCode::Keypad8 => ct::KeyCode::Char('8'),
        KeyCode::Keypad9 => ct::KeyCode::Char('9'),
        KeyCode::KeypadPeriod => ct::KeyCode::Char('.'),
        KeyCode::KeypadPlus => ct::KeyCode::Char('+'),
        KeyCode::KeypadMinus => ct::KeyCode::Char('-'),
        KeyCode::KeypadMultiply => ct::KeyCode::Char('*'),
        KeyCode::KeypadDivide => ct::KeyCode::Char('/'),
        KeyCode::KeypadEnter => ct::KeyCode::Enter,
    }
}

/// Convert frontend-agnostic KeyModifiers back to crossterm KeyModifiers
/// Used by legacy code that still expects crossterm types
pub fn to_crossterm_modifiers(mods: KeyModifiers) -> ct::KeyModifiers {
    let mut result = ct::KeyModifiers::empty();
    if mods.ctrl {
        result |= ct::KeyModifiers::CONTROL;
    }
    if mods.shift {
        result |= ct::KeyModifiers::SHIFT;
    }
    if mods.alt {
        result |= ct::KeyModifiers::ALT;
    }
    result
}

/// Convert frontend-agnostic KeyEvent back to crossterm KeyEvent
/// Used by legacy code that still expects crossterm types
pub fn to_crossterm_key_event(event: &KeyEvent) -> ct::KeyEvent {
    ct::KeyEvent::new(
        to_crossterm_keycode(event.code),
        to_crossterm_modifiers(event.modifiers),
    )
}

// ============================================================================
// COLOR CONVERSIONS (Frontend <-> Ratatui)
// ============================================================================

/// Convert frontend-agnostic Color to ratatui Color
/// Respects global color mode (Direct = true color RGB, Slot = 256-color indexed)
pub fn to_ratatui_color(color: Color) -> ratatui_style::Color {
    super::colors::rgb_to_ratatui_color(color.r, color.g, color.b)
}

/// Convert ratatui Color to frontend-agnostic Color
pub fn from_ratatui_color(color: ratatui_style::Color) -> Color {
    match color {
        ratatui_style::Color::Reset => Color::WHITE,
        ratatui_style::Color::Black => Color::BLACK,
        ratatui_style::Color::Red => Color::RED,
        ratatui_style::Color::Green => Color::GREEN,
        ratatui_style::Color::Yellow => Color::YELLOW,
        ratatui_style::Color::Blue => Color::BLUE,
        ratatui_style::Color::Magenta => Color::MAGENTA,
        ratatui_style::Color::Cyan => Color::CYAN,
        ratatui_style::Color::Gray => Color::GRAY,
        ratatui_style::Color::DarkGray => Color::DARK_GRAY,
        ratatui_style::Color::LightRed => Color::LIGHT_RED,
        ratatui_style::Color::LightGreen => Color::LIGHT_GREEN,
        ratatui_style::Color::LightYellow => Color::LIGHT_YELLOW,
        ratatui_style::Color::LightBlue => Color::LIGHT_BLUE,
        ratatui_style::Color::LightMagenta => Color::LIGHT_MAGENTA,
        ratatui_style::Color::LightCyan => Color::LIGHT_CYAN,
        ratatui_style::Color::White => Color::WHITE,
        ratatui_style::Color::Rgb(r, g, b) => Color::rgb(r, g, b),
        ratatui_style::Color::Indexed(idx) => {
            // Convert indexed color to RGB approximation
            NamedColor::Indexed(idx).to_rgb()
        }
    }
}

/// Convert NamedColor to ratatui Color (optimized for common cases)
pub fn named_to_ratatui(color: NamedColor) -> ratatui_style::Color {
    match color {
        NamedColor::Black => ratatui_style::Color::Black,
        NamedColor::Red => ratatui_style::Color::Red,
        NamedColor::Green => ratatui_style::Color::Green,
        NamedColor::Yellow => ratatui_style::Color::Yellow,
        NamedColor::Blue => ratatui_style::Color::Blue,
        NamedColor::Magenta => ratatui_style::Color::Magenta,
        NamedColor::Cyan => ratatui_style::Color::Cyan,
        NamedColor::Gray => ratatui_style::Color::Gray,
        NamedColor::DarkGray => ratatui_style::Color::DarkGray,
        NamedColor::LightRed => ratatui_style::Color::LightRed,
        NamedColor::LightGreen => ratatui_style::Color::LightGreen,
        NamedColor::LightYellow => ratatui_style::Color::LightYellow,
        NamedColor::LightBlue => ratatui_style::Color::LightBlue,
        NamedColor::LightMagenta => ratatui_style::Color::LightMagenta,
        NamedColor::LightCyan => ratatui_style::Color::LightCyan,
        NamedColor::White => ratatui_style::Color::White,
        NamedColor::Rgb(r, g, b) => super::colors::rgb_to_ratatui_color(r, g, b),
        NamedColor::Indexed(idx) => ratatui_style::Color::Indexed(idx),
        NamedColor::Reset => ratatui_style::Color::Reset,
    }
}

// ============================================================================
// BORDER CONVERSIONS (Config -> Ratatui)
// ============================================================================

/// Convert BorderSides config to ratatui Borders bitflags
/// This is a TUI-specific conversion that belongs in the bridge layer.
pub fn to_ratatui_borders(sides: &crate::config::BorderSides) -> ratatui::widgets::Borders {
    use ratatui::widgets::Borders;

    let mut borders = Borders::empty();
    if sides.top {
        borders |= Borders::TOP;
    }
    if sides.bottom {
        borders |= Borders::BOTTOM;
    }
    if sides.left {
        borders |= Borders::LEFT;
    }
    if sides.right {
        borders |= Borders::RIGHT;
    }

    if borders.is_empty() {
        Borders::ALL // Fallback if somehow all are false
    } else {
        borders
    }
}

// ============================================================================
// RECT CONVERSIONS (Frontend <-> Ratatui)
// ============================================================================

/// Convert frontend-agnostic Rect to ratatui Rect
pub fn to_ratatui_rect(rect: crate::frontend::common::Rect) -> ratatui::layout::Rect {
    ratatui::layout::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

/// Convert ratatui Rect to frontend-agnostic Rect
pub fn from_ratatui_rect(rect: ratatui::layout::Rect) -> crate::frontend::common::Rect {
    crate::frontend::common::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}
