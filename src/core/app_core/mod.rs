//! Core application logic - Pure business logic without UI coupling
//! 
//! AppCore manages game state, configuration, and message processing.
//! It has NO knowledge of rendering - all state is stored in data structures
//! that frontends read from.

mod state;
mod keybinds;
mod layout;
mod commands;

pub use state::*;
