//! Window position configuration storage.
//!
//! Saves and loads window position from `~/.vellum-fe/{character}/window.toml`.

use anyhow::{Context, Result};
use std::path::PathBuf;

use super::WindowPositionConfig;

/// Get the path to the window position config file.
fn config_path(character: Option<&str>) -> Result<PathBuf> {
    let profile_dir = crate::config::Config::profile_dir(character)?;
    Ok(profile_dir.join("window.toml"))
}

/// Load window position configuration for a character.
/// Returns `None` if the file doesn't exist.
pub fn load(character: Option<&str>) -> Result<Option<WindowPositionConfig>> {
    let path = config_path(character)?;

    if !path.exists() {
        tracing::debug!("No window position config at {:?}", path);
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read window config from {:?}", path))?;

    let config: WindowPositionConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse window config from {:?}", path))?;

    tracing::debug!(
        "Loaded window position from {:?}: {:?}",
        path,
        config.window
    );
    Ok(Some(config))
}

/// Save window position configuration for a character.
pub fn save(character: Option<&str>, config: &WindowPositionConfig) -> Result<()> {
    let path = config_path(character)?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {:?}", parent))?;
    }

    let content = toml::to_string_pretty(config).context("Failed to serialize window config")?;

    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write window config to {:?}", path))?;

    tracing::debug!("Saved window position to {:?}: {:?}", path, config.window);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_position::{ScreenInfo, WindowRect};

    #[test]
    fn test_config_serialization() {
        let config = WindowPositionConfig {
            window: WindowRect::new(100, 50, 1200, 800),
            monitors: vec![
                ScreenInfo::new(0, 0, 1920, 1080),
                ScreenInfo::new(1920, 0, 1920, 1080),
            ],
        };

        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: WindowPositionConfig = toml::from_str(&serialized).unwrap();

        assert_eq!(config.window, deserialized.window);
        assert_eq!(config.monitors.len(), deserialized.monitors.len());
    }
}
