//! Frontend abstraction layer
//!
//! This module defines the `Frontend` trait that both TUI and GUI frontends must implement.
//! It provides a unified interface for event polling, rendering, and cleanup.

pub mod common;
pub mod events;
pub mod gui;
pub mod tui;

use anyhow::Result;
pub use common::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
pub use events::FrontendEvent;
pub use gui::EguiApp;

/// Frontend trait - must be implemented by both TUI and GUI frontends
///
/// The Frontend trait separates rendering concerns from business logic.
/// All frontends (TUI via ratatui, GUI via egui) implement this trait
/// to provide a consistent interface to the core application.
pub trait Frontend {
    /// Poll for user input events
    ///
    /// This method should return all pending events (keyboard, mouse, resize, etc.)
    /// converted to the frontend-agnostic `FrontendEvent` enum.
    ///
    /// # Returns
    /// - `Ok(Vec<FrontendEvent>)` - List of events (empty if no events)
    /// - `Err(...)` - If event polling failed
    fn poll_events(&mut self) -> Result<Vec<FrontendEvent>>;

    /// Render the current application state
    ///
    /// This method is called once per frame to render all UI elements.
    /// The frontend should read from app state to get current state and render appropriately.
    ///
    /// # Arguments
    /// - `app` - Mutable reference to the application (allows widgets to update state during render)
    ///
    /// # Returns
    /// - `Ok(())` - Render successful
    /// - `Err(...)` - If rendering failed
    ///
    /// Note: Mutable reference is required because some widgets update internal state
    /// during rendering (e.g., countdown timers calculate current time)
    fn render(&mut self, app: &mut dyn std::any::Any) -> Result<()>;

    /// Cleanup and shutdown the frontend
    ///
    /// This method should restore the terminal (for TUI) or close windows (for GUI)
    /// and perform any necessary cleanup before the application exits.
    ///
    /// # Returns
    /// - `Ok(())` - Cleanup successful
    /// - `Err(...)` - If cleanup failed
    fn cleanup(&mut self) -> Result<()>;

    /// Get current terminal/window size
    ///
    /// Returns the current dimensions of the rendering area.
    /// For TUI: terminal size in characters
    /// For GUI: window size in pixels (may be converted to logical units)
    ///
    /// # Returns
    /// - `(width, height)` tuple
    fn size(&self) -> (u16, u16);

    /// Downcast to concrete type (for accessing frontend-specific methods)
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}
