//! Core application logic - Pure business logic without UI coupling
//!
//! AppCore manages game state, configuration, and message processing.
//! It has NO knowledge of rendering - all state is stored in data structures
//! that frontends read from.

mod commands;
mod config_editor;
mod haptics;
mod interact;
mod keybinds;
mod layout;
mod state;

pub use haptics::{HapticEvent, HapticSnapshot};
pub use interact::{InteractAction, InteractEntity};
pub use keybinds::HotbarKeyConflict;
pub use state::*;
