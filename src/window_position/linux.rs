//! Linux X11 implementation of window positioning using xdotool.
//!
//! Uses shell commands to interact with xdotool for window manipulation.
//! Falls back gracefully if xdotool is not installed.
//!
//! Note: This does NOT work on Wayland. Wayland intentionally prevents
//! applications from positioning windows.

use anyhow::{Context, Result};
use std::process::Command;

use super::{ScreenInfo, WindowPositioner, WindowRect};

pub struct LinuxPositioner {
    window_id: String,
}

impl LinuxPositioner {
    pub fn new() -> Result<Self> {
        // Check if xdotool is available
        let output = Command::new("which")
            .arg("xdotool")
            .output()
            .context("Failed to check for xdotool")?;

        if !output.status.success() {
            anyhow::bail!("xdotool not found. Install with: sudo apt install xdotool");
        }

        // Check if we're on Wayland
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            // Try XWayland compatibility
            if std::env::var("DISPLAY").is_err() {
                anyhow::bail!(
                    "Wayland detected without XWayland. Window positioning not supported."
                );
            }
            tracing::warn!("Running on Wayland with XWayland. Window positioning may not work.");
        }

        // Find our terminal window using parent PID
        let window_id = find_terminal_window()?;

        Ok(Self { window_id })
    }
}

/// Find the terminal window by searching for our parent process's window.
fn find_terminal_window() -> Result<String> {
    // Get parent PID (the terminal emulator)
    let ppid = std::fs::read_to_string("/proc/self/stat")
        .context("Failed to read /proc/self/stat")?
        .split_whitespace()
        .nth(3) // 4th field is PPID
        .context("Failed to parse PPID")?
        .to_string();

    // Try to find window by parent PID
    let output = Command::new("xdotool")
        .args(["search", "--pid", &ppid])
        .output()
        .context("Failed to run xdotool search")?;

    if output.status.success() {
        let windows: Vec<&str> = std::str::from_utf8(&output.stdout)
            .unwrap_or("")
            .lines()
            .collect();

        if let Some(wid) = windows.first() {
            return Ok(wid.to_string());
        }
    }

    // Fallback: try to get the currently active window
    // (assumes we're launched from a terminal that's currently focused)
    let output = Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .context("Failed to run xdotool getactivewindow")?;

    if output.status.success() {
        let wid = std::str::from_utf8(&output.stdout)
            .unwrap_or("")
            .trim()
            .to_string();

        if !wid.is_empty() {
            tracing::debug!("Using active window as terminal: {}", wid);
            return Ok(wid);
        }
    }

    anyhow::bail!("Could not find terminal window. This may happen with GNOME Terminal or other terminals that don't expose window IDs to xdotool.")
}

impl WindowPositioner for LinuxPositioner {
    fn get_position(&self) -> Result<WindowRect> {
        let output = Command::new("xdotool")
            .args(["getwindowgeometry", "--shell", &self.window_id])
            .output()
            .context("Failed to run xdotool getwindowgeometry")?;

        if !output.status.success() {
            anyhow::bail!("xdotool getwindowgeometry failed");
        }

        let stdout = std::str::from_utf8(&output.stdout).context("Invalid UTF-8 from xdotool")?;

        // Parse output like:
        // WINDOW=12345
        // X=100
        // Y=200
        // WIDTH=800
        // HEIGHT=600
        let mut x = 0i32;
        let mut y = 0i32;
        let mut width = 800u32;
        let mut height = 600u32;

        for line in stdout.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "X" => x = value.parse().unwrap_or(0),
                    "Y" => y = value.parse().unwrap_or(0),
                    "WIDTH" => width = value.parse().unwrap_or(800),
                    "HEIGHT" => height = value.parse().unwrap_or(600),
                    _ => {}
                }
            }
        }

        Ok(WindowRect {
            x,
            y,
            width,
            height,
        })
    }

    fn set_position(&self, rect: &WindowRect) -> Result<()> {
        // Move window
        let status = Command::new("xdotool")
            .args([
                "windowmove",
                &self.window_id,
                &rect.x.to_string(),
                &rect.y.to_string(),
            ])
            .status()
            .context("Failed to run xdotool windowmove")?;

        if !status.success() {
            tracing::warn!("xdotool windowmove failed (may be Wayland)");
        }

        // Resize window
        let status = Command::new("xdotool")
            .args([
                "windowsize",
                &self.window_id,
                &rect.width.to_string(),
                &rect.height.to_string(),
            ])
            .status()
            .context("Failed to run xdotool windowsize")?;

        if !status.success() {
            tracing::warn!("xdotool windowsize failed (may be Wayland)");
        }

        Ok(())
    }

    fn get_screen_bounds(&self) -> Result<Vec<ScreenInfo>> {
        // Use xrandr to get monitor information
        let output = Command::new("xrandr")
            .arg("--query")
            .output()
            .context("Failed to run xrandr")?;

        if !output.status.success() {
            // Fallback to default screen
            return Ok(vec![ScreenInfo::new(0, 0, 1920, 1080)]);
        }

        let stdout = std::str::from_utf8(&output.stdout).context("Invalid UTF-8 from xrandr")?;

        let mut screens = Vec::new();

        // Parse xrandr output for connected monitors
        // Example: "DP-1 connected primary 1920x1080+0+0 ..."
        for line in stdout.lines() {
            if line.contains(" connected") {
                // Look for resolution+offset pattern like "1920x1080+0+0"
                for part in line.split_whitespace() {
                    if let Some((res, offset)) = part.split_once('+') {
                        if let Some((w, h)) = res.split_once('x') {
                            let width: u32 = w.parse().unwrap_or(0);
                            let height: u32 = h.parse().unwrap_or(0);

                            if width > 0 && height > 0 {
                                // Parse offset "+X+Y"
                                let offsets: Vec<i32> =
                                    offset.split('+').filter_map(|s| s.parse().ok()).collect();

                                let x = offsets.first().copied().unwrap_or(0);
                                let y = offsets.get(1).copied().unwrap_or(0);

                                screens.push(ScreenInfo::new(x, y, width, height));
                                break;
                            }
                        }
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
