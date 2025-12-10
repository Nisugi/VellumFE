//! Frontend-agnostic input events used by both the TUI and GUI layers.
//!
//! Individual frontends translate their native event streams (crossterm, egui,
//! etc.) into this enum so the core logic only handles one event shape.

use super::common::{KeyCode, KeyModifiers, MouseEvent};

/// Frontend-agnostic event system
/// Events emitted by frontends (TUI, GUI) are converted to this unified format
#[derive(Debug, Clone, PartialEq)]
pub enum FrontendEvent {
    /// Keyboard input
    Key {
        code: KeyCode,
        modifiers: KeyModifiers,
    },
    /// Mouse input
    Mouse(MouseEvent),
    /// Terminal/window resize
    Resize { width: u16, height: u16 },
    /// Paste event (text from clipboard)
    Paste { text: String },
    /// Application quit signal
    Quit,
}

impl FrontendEvent {
    /// Create a key event
    pub fn key(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self::Key { code, modifiers }
    }

    /// Create a mouse event
    pub fn mouse(event: MouseEvent) -> Self {
        Self::Mouse(event)
    }

    /// Create a resize event
    pub fn resize(width: u16, height: u16) -> Self {
        Self::Resize { width, height }
    }

    /// Create a paste event
    pub fn paste(text: String) -> Self {
        Self::Paste { text }
    }

    /// Create a quit event
    pub fn quit() -> Self {
        Self::Quit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::common::{MouseEventKind, MouseButton};

    #[test]
    fn test_event_creation() {
        let key_event = FrontendEvent::key(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(matches!(key_event, FrontendEvent::Key { .. }));

        let mouse_event = FrontendEvent::mouse(MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            10,
            20,
            KeyModifiers::NONE,
        ));
        assert!(matches!(mouse_event, FrontendEvent::Mouse(_)));

        let resize_event = FrontendEvent::resize(120, 40);
        assert!(matches!(
            resize_event,
            FrontendEvent::Resize {
                width: 120,
                height: 40
            }
        ));

        let quit_event = FrontendEvent::quit();
        assert!(matches!(quit_event, FrontendEvent::Quit));
    }
}
