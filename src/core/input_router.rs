//! Input routing for menu widgets
//!
//! Routes keyboard input to the appropriate MenuAction based on:
//! - Current InputMode (which widget has focus)
//! - Menu keybinds configuration
//! - Widget context (browser vs form vs editor)

use crate::config::Config;
use crate::core::menu_actions::{ActionContext, MenuAction};
use crate::data::ui_state::InputMode;
use crate::frontend::common::KeyEvent;

/// Route a key event to a MenuAction based on current context
pub fn route_input(key: &KeyEvent, mode: &InputMode, config: &Config) -> MenuAction {
    // Determine the action context based on InputMode
    let context = get_action_context(mode);

    // Resolve the key to an action using menu keybinds
    config.menu_keybinds.resolve_action(key, context)
}

/// Map InputMode to ActionContext for keybind resolution
fn get_action_context(mode: &InputMode) -> ActionContext {
    match mode {
        // Browser widgets
        InputMode::HighlightBrowser
        | InputMode::KeybindBrowser
        | InputMode::ColorPaletteBrowser
        | InputMode::SpellColorsBrowser
        | InputMode::UIColorsBrowser
        | InputMode::ThemeBrowser
        | InputMode::IndicatorTemplateEditor => ActionContext::Browser,

        // Form widgets
        InputMode::HighlightForm
        | InputMode::KeybindForm
        | InputMode::ColorForm
        | InputMode::SpellColorForm
        | InputMode::ThemeEditor => ActionContext::Form,

        // Settings editor (hybrid - has both navigation and inline editing)
        InputMode::SettingsEditor => ActionContext::SettingsEditor,

        // Window editor (most complex - has navigation, inline editing, reordering)
        InputMode::WindowEditor => ActionContext::WindowEditor,

        // Normal modes - should not route through menu system
        InputMode::Normal
        | InputMode::Navigation
        | InputMode::History
        | InputMode::Search
        | InputMode::Menu => ActionContext::Browser, // Fallback (shouldn't be called)
    }
}

/// Check if a priority window (editor/browser/form) is currently open
/// Priority windows intercept input before normal keybinds
pub fn has_priority_window(mode: &InputMode) -> bool {
    !matches!(mode, InputMode::Normal | InputMode::Navigation)
}

/// Alias for backwards compatibility
#[deprecated(note = "Use has_priority_window() instead")]
pub fn should_use_menu_keybinds(mode: &InputMode) -> bool {
    has_priority_window(mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::common::KeyModifiers;

    #[test]
    fn test_context_mapping() {
        assert!(matches!(
            get_action_context(&InputMode::HighlightBrowser),
            ActionContext::Browser
        ));

        assert!(matches!(
            get_action_context(&InputMode::HighlightForm),
            ActionContext::Form
        ));

        assert!(matches!(
            get_action_context(&InputMode::SettingsEditor),
            ActionContext::SettingsEditor
        ));
    }

    #[test]
    fn test_priority_window_detection() {
        // Normal modes - no priority window
        assert!(!has_priority_window(&InputMode::Normal));
        assert!(!has_priority_window(&InputMode::Navigation));

        // Priority windows - intercept input
        assert!(has_priority_window(&InputMode::HighlightBrowser));
        assert!(has_priority_window(&InputMode::WindowEditor));
        assert!(has_priority_window(&InputMode::SettingsEditor));
        assert!(has_priority_window(&InputMode::Search));
        assert!(has_priority_window(&InputMode::Menu));
    }
}
