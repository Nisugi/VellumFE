//! Core business logic layer
//!
//! This module contains all game logic, state management, and XML processing.
//! NO imports from frontend/ or rendering code.
//! Core updates data structures in the data layer, frontends read and render.

pub mod app_core;
pub mod bounty_parser;
pub mod highlight_engine;
pub mod input_router;
pub mod menu_actions;
pub mod messages;
pub mod state;

pub use app_core::AppCore;
pub use highlight_engine::{
    apply_deferred_for_window, CoreHighlightEngine, DeferredReplacement, HighlightResult,
};
pub use messages::MessageProcessor;
pub use state::GameState;
