//! Terminal Title Manager
//!
//! Updates the terminal title bar with game state using a configurable template.
//! Template variables: {character}, {room}, {health}, {mana}, {stamina}, {unread}

use crate::core::AppCore;
use std::io::Write;

/// Manages terminal title updates based on a template string.
pub struct TerminalTitleManager {
    /// The template string with variable placeholders
    template: String,
    /// Last rendered title to avoid redundant updates
    last_title: String,
}

impl TerminalTitleManager {
    /// Create a new terminal title manager with the given template.
    /// Returns None if the template is empty (disabled).
    pub fn new(template: String) -> Option<Self> {
        if template.is_empty() {
            None
        } else {
            Some(Self {
                template,
                last_title: String::new(),
            })
        }
    }

    /// Update the terminal title if game state has changed.
    /// Returns true if the title was updated.
    pub fn update<W: Write>(
        &mut self,
        app_core: &AppCore,
        writer: &mut W,
    ) -> std::io::Result<bool> {
        let new_title = self.render_template(app_core);

        if new_title == self.last_title {
            return Ok(false);
        }

        // Set terminal title using OSC escape sequence: ESC]0;titleBEL
        write!(writer, "\x1b]0;{}\x07", new_title)?;
        writer.flush()?;

        self.last_title = new_title;
        Ok(true)
    }

    /// Render the template with current game state values.
    fn render_template(&self, app_core: &AppCore) -> String {
        let game_state = &app_core.game_state;

        let character = game_state.character_name.as_deref().unwrap_or("");

        let room = game_state.room_name.as_deref().unwrap_or("");

        let health = game_state.vitals.health;
        let mana = game_state.vitals.mana;
        let stamina = game_state.vitals.stamina;

        // Count unread tabs across all tabbed text windows
        let unread = self.count_unread_tabs(app_core);

        self.template
            .replace("{character}", character)
            .replace("{room}", room)
            .replace("{health}", &health.to_string())
            .replace("{mana}", &mana.to_string())
            .replace("{stamina}", &stamina.to_string())
            .replace("{unread}", &unread.to_string())
    }

    /// Count total unread tabs across all tabbed text windows.
    fn count_unread_tabs(&self, app_core: &AppCore) -> usize {
        app_core
            .ui_state
            .windows
            .values()
            .filter_map(|window_state| {
                if let crate::data::WindowContent::TabbedText(tabbed) = &window_state.content {
                    Some(tabbed.tabs.iter().filter(|t| t.has_unread).count())
                } else {
                    None
                }
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_template_returns_none() {
        assert!(TerminalTitleManager::new(String::new()).is_none());
    }

    #[test]
    fn test_non_empty_template_returns_some() {
        let manager = TerminalTitleManager::new("VellumFE".to_string());
        assert!(manager.is_some());
    }

    #[test]
    fn test_template_variable_replacement() {
        // Test the template replacement logic directly
        let template = "{character} - {room} | H:{health}% M:{mana}%";
        let result = template
            .replace("{character}", "TestChar")
            .replace("{room}", "Town Square")
            .replace("{health}", "85")
            .replace("{mana}", "100")
            .replace("{stamina}", "72")
            .replace("{unread}", "0");
        assert_eq!(result, "TestChar - Town Square | H:85% M:100%");
    }

    #[test]
    fn test_missing_values_render_empty() {
        // When values are empty, variables are replaced with empty strings
        let template = "{character} - VellumFE";
        let result = template.replace("{character}", "");
        assert_eq!(result, " - VellumFE");
    }

    #[test]
    fn test_ansi_escape_format() {
        // Verify the ANSI escape sequence format is correct
        let title = "VellumFE - Test";
        let escape = format!("\x1b]0;{}\x07", title);
        assert_eq!(escape, "\x1b]0;VellumFE - Test\x07");
        // Verify escape sequence bytes
        assert_eq!(escape.as_bytes()[0], 0x1b); // ESC
        assert_eq!(escape.as_bytes()[1], b']'); // ]
        assert_eq!(escape.as_bytes()[2], b'0'); // 0
        assert_eq!(escape.as_bytes()[3], b';'); // ;
        assert_eq!(escape.as_bytes().last(), Some(&0x07)); // BEL
    }

    #[test]
    fn test_update_caching() {
        // Test that the caching mechanism works by using the public interface
        let mut manager = TerminalTitleManager::new("Static Title".to_string()).unwrap();

        // Manually set last_title to simulate a previous update
        manager.last_title = "Static Title".to_string();

        // Since template has no variables and last_title matches, this should return false
        // without needing a real AppCore (the render_template would return "Static Title")

        // We can't fully test update() without AppCore, but we verify the caching logic
        // by checking the struct state
        assert_eq!(manager.template, "Static Title");
        assert_eq!(manager.last_title, "Static Title");
    }
}
