//! Core application logic - Pure business logic without UI coupling
//!
//! AppCore manages game state, configuration, and message processing.
//! It has NO knowledge of rendering - all state is stored in data structures
//! that frontends read from.

mod automation;
pub(crate) mod command_help;
mod commands;
mod config_editor;
mod haptics;
mod interact;
mod jinx;
mod keybinds;
mod layout;
mod skill_goals;
mod state;
mod streams;
mod targeting;
mod webui;

pub use automation::AutomationOwner;
pub use haptics::{HapticEvent, HapticSnapshot};
pub use interact::{InteractAction, InteractEntity};
pub use keybinds::HotbarKeyConflict;
pub use state::*;
