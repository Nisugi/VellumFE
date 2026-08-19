//! Data layer - Pure state without UI coupling
//!
//! This module contains all the game state and UI state as pure data structures.
//! NO imports from frontend/ or any rendering code.
//! Both TUI and GUI frontends read from these structures to render.

pub mod geometry;
pub mod input;
pub mod remote_buffer;
pub mod ui_action;
pub mod ui_state;
pub mod view_kind;
pub mod webui;
pub mod widget;
pub mod window;

pub use input::*;
pub use remote_buffer::*;
pub use ui_action::{CommandOutcome, ShellZoneTarget, UiAction, ZoneOp};
pub use ui_state::*;
pub use webui::*;
pub use widget::*;
pub use window::*;
