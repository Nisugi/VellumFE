//! GUI Widget rendering modules
//!
//! This module contains specialized rendering functions for each widget type,
//! converting data layer types to egui rendering calls.

mod text_window;

pub use text_window::{render_text_window, TextWindowResponse};
