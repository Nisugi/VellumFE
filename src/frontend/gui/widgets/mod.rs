//! GUI Widget rendering modules
//!
//! This module contains specialized rendering functions for each widget type,
//! converting data layer types to egui rendering calls.

mod active_spells;
mod injury_doll;
mod room_window;
mod tabbed_text_window;
mod text_window;

pub use active_spells::render_active_effects;
pub use injury_doll::render_injury_doll;
pub use room_window::{render_room_window, RoomComponentVisibility, RoomWindowResponse};
pub use tabbed_text_window::{
    render_tabbed_text_window, GuiTabbedTextState, TabbedTextWindowResponse,
};
pub use text_window::{parse_hex_to_color32, render_text_window, TextWindowResponse};
