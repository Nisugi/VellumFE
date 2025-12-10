///! Theme caching to avoid HashMap lookup + clone on every render
///!
///! This module provides a simple cache for the currently active theme,
///! reducing overhead in the hot rendering path.

use crate::theme::AppTheme;

/// Theme cache to avoid repeated HashMap lookups and clones during rendering
pub struct ThemeCache {
    /// Cached theme to avoid HashMap lookup + clone every render
    cached_theme: AppTheme,
    /// Cached theme ID to detect theme changes
    cached_theme_id: String,
}

impl ThemeCache {
    /// Create a new theme cache with the dark theme as default
    pub fn new() -> Self {
        Self {
            cached_theme: crate::theme::ThemePresets::dark(),
            cached_theme_id: "dark".to_string(),
        }
    }

    /// Update the cached theme (call this when theme changes via command/browser)
    pub fn update(&mut self, theme_id: String, theme: AppTheme) {
        self.cached_theme = theme;
        self.cached_theme_id = theme_id;
    }

    /// Get a reference to the currently cached theme
    pub fn get_theme(&self) -> &AppTheme {
        &self.cached_theme
    }

    /// Get the ID of the currently cached theme
    pub fn get_theme_id(&self) -> &str {
        &self.cached_theme_id
    }
}

impl Default for ThemeCache {
    fn default() -> Self {
        Self::new()
    }
}
