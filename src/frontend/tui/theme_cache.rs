///! Theme caching to avoid HashMap lookup + clone on every render
///!
///! This module provides a simple cache for the currently active theme,
///! reducing overhead in the hot rendering path.
use crate::theme::AppTheme;
use std::sync::Arc;

/// Theme cache to avoid repeated HashMap lookups and clones during rendering
pub struct ThemeCache {
    /// Arc so render can hold the theme across `&mut self` sync calls
    /// without deep-cloning ~100 colors every frame
    cached_theme: Arc<AppTheme>,
    /// Cached theme ID to detect theme changes
    cached_theme_id: String,
    /// Bumped on every update; lets sync skip theme-derived config
    /// re-application while the theme is unchanged
    version: u64,
}

impl ThemeCache {
    /// Create a new theme cache with the dark theme as default
    pub fn new() -> Self {
        Self {
            cached_theme: Arc::new(crate::theme::ThemePresets::dark()),
            cached_theme_id: "dark".to_string(),
            version: 0,
        }
    }

    /// Update the cached theme (call this when theme changes via command/browser)
    pub fn update(&mut self, theme_id: String, theme: AppTheme) {
        self.cached_theme = Arc::new(theme);
        self.cached_theme_id = theme_id;
        self.version = self.version.wrapping_add(1);
    }

    /// Monotonic version; changes whenever the cached theme is replaced
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Get a reference to the currently cached theme
    pub fn get_theme(&self) -> &AppTheme {
        &self.cached_theme
    }

    /// Cheap per-frame handle to the cached theme
    pub fn get_theme_arc(&self) -> Arc<AppTheme> {
        Arc::clone(&self.cached_theme)
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
