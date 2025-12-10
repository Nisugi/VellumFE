//! Data layer - Pure state without UI coupling
//!
//! This module contains all the game state and UI state as pure data structures.
//! NO imports from frontend/ or any rendering code.
//! Both TUI and GUI frontends read from these structures to render.

pub mod ui_state;
pub mod widget;
pub mod window;

pub use ui_state::*;
pub use widget::*;
pub use window::*;
