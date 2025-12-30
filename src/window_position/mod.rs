//! Terminal window position persistence across sessions.
//!
//! This module provides cross-platform support for saving and restoring
//! the terminal window position per character profile.
//!
//! # Platform Support
//! - **Windows**: Full support via Win32 APIs
//! - **Linux X11**: Support via xdotool command
//! - **macOS**: Support via AppleScript/osascript
//! - **Wayland**: Not supported (Wayland prohibits window positioning)

mod storage;

#[cfg(windows)]
mod windows;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;

#[cfg(target_os = "macos")]
mod macos;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub use storage::{load, save};

/// Window position and size in screen pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WindowRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if this rect overlaps with another rect.
    pub fn overlaps(&self, other: &WindowRect) -> bool {
        let self_right = self.x + self.width as i32;
        let self_bottom = self.y + self.height as i32;
        let other_right = other.x + other.width as i32;
        let other_bottom = other.y + other.height as i32;

        self.x < other_right
            && self_right > other.x
            && self.y < other_bottom
            && self_bottom > other.y
    }

    /// Calculate overlap area with another rect.
    pub fn overlap_area(&self, other: &WindowRect) -> u64 {
        let self_right = self.x + self.width as i32;
        let self_bottom = self.y + self.height as i32;
        let other_right = other.x + other.width as i32;
        let other_bottom = other.y + other.height as i32;

        let overlap_left = self.x.max(other.x);
        let overlap_right = self_right.min(other_right);
        let overlap_top = self.y.max(other.y);
        let overlap_bottom = self_bottom.min(other_bottom);

        if overlap_right > overlap_left && overlap_bottom > overlap_top {
            ((overlap_right - overlap_left) as u64) * ((overlap_bottom - overlap_top) as u64)
        } else {
            0
        }
    }
}

/// Screen/monitor bounds information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenInfo {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenInfo {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Convert to WindowRect for overlap calculations.
    pub fn as_rect(&self) -> WindowRect {
        WindowRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// Saved window position configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowPositionConfig {
    pub window: WindowRect,
    pub monitors: Vec<ScreenInfo>,
}

/// Platform-specific window positioning implementation.
/// Note: This trait is NOT Send+Sync because window handles are thread-local on some platforms.
/// The positioner should only be used on the main thread.
pub trait WindowPositioner {
    /// Get current terminal window position and size.
    fn get_position(&self) -> Result<WindowRect>;

    /// Set terminal window position and size.
    fn set_position(&self, rect: &WindowRect) -> Result<()>;

    /// Get all available screen/monitor bounds.
    fn get_screen_bounds(&self) -> Result<Vec<ScreenInfo>>;
}

/// Extension methods for WindowPositioner.
pub trait WindowPositionerExt: WindowPositioner {
    /// Check if a rect is at least partially visible on any screen.
    fn is_visible(&self, rect: &WindowRect) -> bool {
        const MIN_VISIBLE_PIXELS: u64 = 100 * 100; // At least 100x100 area visible

        if let Ok(screens) = self.get_screen_bounds() {
            for screen in &screens {
                let overlap = rect.overlap_area(&screen.as_rect());
                if overlap >= MIN_VISIBLE_PIXELS {
                    return true;
                }
            }
        }
        false
    }

    /// Clamp rect to the nearest visible screen position.
    /// Preserves size, only adjusts position.
    fn clamp_to_screen(&self, rect: &WindowRect) -> Result<WindowRect> {
        let screens = self.get_screen_bounds()?;
        if screens.is_empty() {
            return Ok(rect.clone());
        }

        // Find screen with most overlap, or nearest screen
        let mut best_screen = &screens[0];
        let mut best_overlap = 0u64;

        for screen in &screens {
            let overlap = rect.overlap_area(&screen.as_rect());
            if overlap > best_overlap {
                best_overlap = overlap;
                best_screen = screen;
            }
        }

        // If no overlap, find nearest screen by center distance
        if best_overlap == 0 {
            let rect_center_x = rect.x + (rect.width as i32 / 2);
            let rect_center_y = rect.y + (rect.height as i32 / 2);
            let mut min_distance = i64::MAX;

            for screen in &screens {
                let screen_center_x = screen.x + (screen.width as i32 / 2);
                let screen_center_y = screen.y + (screen.height as i32 / 2);
                let dx = (rect_center_x - screen_center_x) as i64;
                let dy = (rect_center_y - screen_center_y) as i64;
                let distance = dx * dx + dy * dy;

                if distance < min_distance {
                    min_distance = distance;
                    best_screen = screen;
                }
            }
        }

        // Clamp position to keep window within screen bounds
        let mut new_x = rect.x;
        let mut new_y = rect.y;

        // Ensure window fits within screen (allow partial visibility at edges)
        let margin = 50i32; // Keep at least 50px visible

        // Clamp X
        let screen_right = best_screen.x + best_screen.width as i32;
        if new_x + rect.width as i32 - margin < best_screen.x {
            new_x = best_screen.x - rect.width as i32 + margin;
        }
        if new_x + margin > screen_right {
            new_x = screen_right - margin;
        }

        // Clamp Y
        let screen_bottom = best_screen.y + best_screen.height as i32;
        if new_y + rect.height as i32 - margin < best_screen.y {
            new_y = best_screen.y - rect.height as i32 + margin;
        }
        if new_y + margin > screen_bottom {
            new_y = screen_bottom - margin;
        }

        Ok(WindowRect {
            x: new_x,
            y: new_y,
            width: rect.width,
            height: rect.height,
        })
    }
}

// Implement extension trait for all WindowPositioner implementations
impl<T: WindowPositioner + ?Sized> WindowPositionerExt for T {}

/// Create a platform-appropriate window positioner.
/// Returns `None` if the platform is not supported.
pub fn create_positioner() -> Option<Box<dyn WindowPositioner>> {
    #[cfg(windows)]
    {
        Some(Box::new(windows::WindowsPositioner::new()))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        match linux::LinuxPositioner::new() {
            Ok(positioner) => Some(Box::new(positioner)),
            Err(e) => {
                tracing::debug!("Linux window positioner not available: {}", e);
                None
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        match macos::MacOSPositioner::new() {
            Ok(positioner) => Some(Box::new(positioner)),
            Err(e) => {
                tracing::debug!("macOS window positioner not available: {}", e);
                None
            }
        }
    }

    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_rect_overlaps() {
        let a = WindowRect::new(0, 0, 100, 100);
        let b = WindowRect::new(50, 50, 100, 100);
        let c = WindowRect::new(200, 200, 100, 100);

        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
        assert!(!a.overlaps(&c));
        assert!(!c.overlaps(&a));
    }

    #[test]
    fn test_window_rect_overlap_area() {
        let a = WindowRect::new(0, 0, 100, 100);
        let b = WindowRect::new(50, 50, 100, 100);

        // Overlap is 50x50 = 2500
        assert_eq!(a.overlap_area(&b), 2500);
        assert_eq!(b.overlap_area(&a), 2500);

        let c = WindowRect::new(200, 200, 100, 100);
        assert_eq!(a.overlap_area(&c), 0);
    }

    #[test]
    fn test_screen_info_as_rect() {
        let screen = ScreenInfo::new(100, 200, 1920, 1080);
        let rect = screen.as_rect();

        assert_eq!(rect.x, 100);
        assert_eq!(rect.y, 200);
        assert_eq!(rect.width, 1920);
        assert_eq!(rect.height, 1080);
    }
}
