//! Core business logic layer
//!
//! This module contains all game logic, state management, and XML processing.
//! NO imports from frontend/ or rendering code.
//! Core updates data structures in the data layer, frontends read and render.

pub mod alert_timers;
pub mod alerts;
pub mod app_core;
pub mod bestiary;
pub mod bounty_parser;
pub mod character_state;
pub mod classic_maps;
pub mod conditions;
pub mod creature_cards;
pub mod curated_maps;
pub mod custom_emoji;
pub mod data_pack;
pub mod day_pass;
pub mod elanthian_time;
pub mod emoji;
pub mod evidence;
pub mod foreach;
pub mod game_art;
pub mod game_objects;
pub mod gameobj_data;
pub mod ghost_rooms;
pub mod group;
pub mod harmony;
pub mod harmony_skin;
pub mod highlight_engine;
pub mod hotbar;
pub mod inline_image;
pub mod input_router;
pub mod inventory_service;
pub mod item_mover;
pub mod jinx;
pub mod known_windows;
pub mod layout_engine;
pub mod local_catalog;
pub mod map_service;
pub mod mapdb;
pub mod mapdb_update;
pub mod membership;
pub mod menu_actions;
pub mod messages;
pub mod missing_spells;
pub mod move_feedback;
pub mod multiaccount;
pub mod pathing;
pub mod placement;
pub mod remote;
pub mod satellites;
pub mod session_registry;
pub mod skill_trainer;
pub mod sorter;
pub mod spell_table;
pub mod state;
pub mod travel;
pub mod uipack;
pub mod view_resolver;
pub mod window_style;

pub use app_core::AppCore;
pub use highlight_engine::{
    apply_deferred_for_window, CoreHighlightEngine, DeferredReplacement, HighlightResult,
};
pub use messages::MessageProcessor;
pub use state::GameState;
