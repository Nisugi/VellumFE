//! Core business logic layer
//!
//! This module contains all game logic, state management, and XML processing.
//! NO imports from frontend/ or rendering code.
//! Core updates data structures in the data layer, frontends read and render.

pub mod app_core;
pub mod bounty_parser;
pub mod ghost_rooms;
pub mod highlight_engine;
pub mod hotbar;
pub mod input_router;
pub mod layout_engine;
pub mod map_service;
pub mod mapdb_update;
pub mod menu_actions;
pub mod messages;
pub mod remote;
pub mod state;

pub use app_core::AppCore;
pub use highlight_engine::{
    apply_deferred_for_window, CoreHighlightEngine, DeferredReplacement, HighlightResult,
};
pub use messages::MessageProcessor;
pub use state::GameState;
