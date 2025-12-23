//! Common frontend abstractions shared between TUI and GUI implementations.
//!
//! This module contains UI-agnostic types and traits that enable code reuse
//! across different frontend implementations (TUI with ratatui, GUI with egui/iced).

pub mod color;
pub mod command_input_model;
pub mod input;
pub mod rect;
pub mod text_input;
pub mod widget_data;

pub use color::{Color, NamedColor};
pub use command_input_model::CommandInputModel;
pub use input::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
pub use rect::Rect;
pub use text_input::TextInput;
