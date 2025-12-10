//! TUI-specific renderers for frontend-agnostic widget data.
//!
//! These modules consume the widget data structures from `frontend::common::widget_data`
//! and render them using ratatui's Buffer/Rect/Style types.

pub mod countdown;
pub mod hand;
pub mod indicator;
pub mod progress_bar;

pub use countdown::render_countdown;
pub use hand::render_hand;
pub use indicator::render_indicator;
pub use progress_bar::render_progress_bar;
