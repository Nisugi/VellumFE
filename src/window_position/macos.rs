//! macOS implementation of window positioning using AppleScript.
//!
//! Uses osascript to run AppleScript commands for window manipulation.
//! Detects the terminal application from $TERM_PROGRAM environment variable.

use anyhow::{Context, Result};
use std::process::Command;

use super::{ScreenInfo, WindowPositioner, WindowRect};

pub struct MacOSPositioner {
    terminal_app: String,
}

impl MacOSPositioner {
    pub fn new() -> Result<Self> {
        // Detect terminal application from environment
        let terminal_app = detect_terminal_app()?;
        tracing::debug!("Detected terminal app: {}", terminal_app);

        Ok(Self { terminal_app })
    }
}

/// Detect the terminal application from environment variables.
fn detect_terminal_app() -> Result<String> {
    // Check TERM_PROGRAM first (set by most modern terminals)
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        let app = match term_program.as_str() {
            "Apple_Terminal" => "Terminal",
            "iTerm.app" => "iTerm",
            "WezTerm" => "WezTerm",
            "Alacritty" => "Alacritty",
            "kitty" => "kitty",
            other => {
                // Try to use it as-is
                tracing::debug!("Unknown TERM_PROGRAM: {}, trying as app name", other);
                return Ok(other.to_string());
            }
        };
        return Ok(app.to_string());
    }

    // Fallback to Terminal.app
    tracing::debug!("TERM_PROGRAM not set, defaulting to Terminal");
    Ok("Terminal".to_string())
}

impl WindowPositioner for MacOSPositioner {
    fn get_position(&self) -> Result<WindowRect> {
        // AppleScript to get window bounds
        let script = format!(
            r#"tell application "{}"
                get bounds of front window
            end tell"#,
            self.terminal_app
        );

        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .context("Failed to run osascript")?;

        if !output.status.success() {
            let stderr = std::str::from_utf8(&output.stderr).unwrap_or("");
            anyhow::bail!("osascript failed: {}", stderr);
        }

        // Parse output like "100, 50, 1300, 850" (x1, y1, x2, y2)
        let stdout = std::str::from_utf8(&output.stdout)
            .context("Invalid UTF-8 from osascript")?
            .trim();

        let coords: Vec<i32> = stdout
            .split(", ")
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        if coords.len() != 4 {
            anyhow::bail!("Unexpected bounds format: {}", stdout);
        }

        let x = coords[0];
        let y = coords[1];
        let width = (coords[2] - coords[0]) as u32;
        let height = (coords[3] - coords[1]) as u32;

        Ok(WindowRect {
            x,
            y,
            width,
            height,
        })
    }

    fn set_position(&self, rect: &WindowRect) -> Result<()> {
        // AppleScript bounds are {x1, y1, x2, y2}
        let x2 = rect.x + rect.width as i32;
        let y2 = rect.y + rect.height as i32;

        let script = format!(
            r#"tell application "{}"
                set bounds of front window to {{{}, {}, {}, {}}}
            end tell"#,
            self.terminal_app, rect.x, rect.y, x2, y2
        );

        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .context("Failed to run osascript")?;

        if !output.status.success() {
            let stderr = std::str::from_utf8(&output.stderr).unwrap_or("");
            tracing::warn!("osascript failed: {}", stderr);
            // Don't fail completely - some terminals have limited AppleScript support
        }

        Ok(())
    }

    fn get_screen_bounds(&self) -> Result<Vec<ScreenInfo>> {
        // Use system_profiler to get display information
        let output = Command::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output()
            .context("Failed to run system_profiler")?;

        if !output.status.success() {
            // Fallback to default screen
            return Ok(vec![ScreenInfo::new(0, 0, 1920, 1080)]);
        }

        // Parse JSON output
        let stdout =
            std::str::from_utf8(&output.stdout).context("Invalid UTF-8 from system_profiler")?;

        // Simple parsing - look for resolution patterns
        // Full JSON parsing would require serde_json, so we do a simple regex-like search
        let mut screens = Vec::new();

        // Look for resolution strings like "1920 x 1080"
        // This is a simplified approach - a proper implementation would parse the JSON
        for line in stdout.lines() {
            if line.contains("_spdisplays_resolution:") || line.contains("Resolution:") {
                // Try to extract dimensions
                let parts: Vec<&str> = line.split_whitespace().collect();
                for (i, part) in parts.iter().enumerate() {
                    if let Ok(width) = part.replace(",", "").replace("\"", "").parse::<u32>() {
                        if i + 2 < parts.len() && parts[i + 1] == "x" {
                            if let Ok(height) = parts[i + 2]
                                .replace(",", "")
                                .replace("\"", "")
                                .parse::<u32>()
                            {
                                // Assume screens are laid out horizontally
                                let x = screens
                                    .iter()
                                    .map(|s: &ScreenInfo| s.x + s.width as i32)
                                    .max()
                                    .unwrap_or(0);
                                screens.push(ScreenInfo::new(x, 0, width, height));
                                break;
                            }
                        }
                    }
                }
            }
        }

        if screens.is_empty() {
            // Fallback: use AppleScript to get screen size
            let script = r#"tell application "Finder"
                get bounds of window of desktop
            end tell"#;

            let output = Command::new("osascript").args(["-e", script]).output().ok();

            if let Some(output) = output {
                if output.status.success() {
                    let stdout = std::str::from_utf8(&output.stdout).unwrap_or("");
                    let coords: Vec<i32> = stdout
                        .trim()
                        .split(", ")
                        .filter_map(|s| s.trim().parse().ok())
                        .collect();

                    if coords.len() == 4 {
                        let width = (coords[2] - coords[0]) as u32;
                        let height = (coords[3] - coords[1]) as u32;
                        screens.push(ScreenInfo::new(0, 0, width, height));
                    }
                }
            }
        }

        if screens.is_empty() {
            screens.push(ScreenInfo::new(0, 0, 1920, 1080));
        }

        Ok(screens)
    }
}
