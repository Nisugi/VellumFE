mod text_window;
mod command_input;
mod window_manager;
mod progress_bar;
mod countdown;
mod indicator;
mod compass;
mod injury_doll;
mod hands;
mod hand;
mod dashboard;
mod scrollable_container;
mod active_effects;
mod performance_stats;

pub use text_window::{TextWindow, StyledText};
pub use command_input::CommandInput;
pub use window_manager::{WindowManager, WindowConfig, Widget};
#[allow(unused_imports)]
pub use progress_bar::ProgressBar;
#[allow(unused_imports)]
pub use countdown::Countdown;
#[allow(unused_imports)]
pub use indicator::Indicator;
#[allow(unused_imports)]
pub use compass::Compass;
#[allow(unused_imports)]
pub use injury_doll::InjuryDoll;
#[allow(unused_imports)]
pub use hands::Hands;
#[allow(unused_imports)]
pub use hand::{Hand, HandType};
#[allow(unused_imports)]
pub use dashboard::{Dashboard, DashboardLayout};
pub use performance_stats::PerformanceStatsWidget;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
};

pub struct UiLayout {
    pub main_area: Rect,
    pub input_area: Rect,
}

impl UiLayout {
    pub fn calculate(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),      // Main text area
                Constraint::Length(3),   // Input area
            ])
            .split(area);

        Self {
            main_area: chunks[0],
            input_area: chunks[1],
        }
    }
}
