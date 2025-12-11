//! GUI Widget rendering modules
//!
//! This module contains specialized rendering functions for each widget type,
//! converting data layer types to egui rendering calls.

mod active_spells;
mod room_window;
mod tabbed_text;
mod text_window;

pub use active_spells::render_active_effects;
pub use room_window::{render_room_window, RoomComponentVisibility, RoomWindowResponse};
pub use tabbed_text::{render_tabbed_text_window, TabbedTextWindowResponse};
pub use text_window::{render_text_window, TextWindowResponse};
