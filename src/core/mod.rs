//! Core business logic layer
//!
//! This module contains all game logic, state management, and XML processing.
//! NO imports from frontend/ or rendering code.
//! Core updates data structures in the data layer, frontends read and render.

pub mod app_core;
pub mod bounty_parser;
pub mod character_state;
pub mod day_pass;
pub mod conditions;
pub mod doll_rules;
pub mod custom_emoji;
pub mod elanthian_time;
pub mod game_art;
pub mod inline_image;
pub mod data_pack;
pub mod emoji;
pub mod evidence;
pub mod foreach;
pub mod game_objects;
pub mod gameobj_data;
pub mod ghost_rooms;
pub mod harmony;
pub mod harmony_skin;
pub mod highlight_engine;
pub mod hotbar;
pub mod input_router;
pub mod inventory_service;
pub mod missing_spells;
pub mod jinx;
pub mod group;
pub mod layout_engine;
pub mod map_service;
pub mod mapdb;
pub mod move_feedback;
pub mod multiaccount;
pub mod session_registry;
pub mod mapdb_update;
pub mod menu_actions;
pub mod known_windows;
pub mod local_catalog;
pub mod placement;
pub mod messages;
pub mod pathing;
pub mod remote;
pub mod sorter;
pub mod spell_table;
pub mod travel;
pub mod state;
pub mod view_resolver;
pub mod uipack;
pub mod window_style;

pub use app_core::AppCore;
pub use highlight_engine::{
    apply_deferred_for_window, CoreHighlightEngine, DeferredReplacement, HighlightResult,
};
pub use messages::MessageProcessor;
pub use state::GameState;
