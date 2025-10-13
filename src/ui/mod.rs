mod text_window;
mod tabbed_text_window;
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
mod targets;
mod players;
mod highlight_form;
mod keybind_form;
mod popup_menu;
mod window_editor_v2;
mod settings_editor;
mod highlight_browser;

pub use text_window::{TextWindow, StyledText, SpanType, LinkData, LineSegments, TextSegment};
pub use popup_menu::{PopupMenu, MenuItem};
pub use tabbed_text_window::{TabbedTextWindow, TabBarPosition};
pub use command_input::CommandInput;
pub use window_manager::{WindowManager, WindowConfig, Widget};
#[allow(unused_imports)]
pub use progress_bar::{ProgressBar, TextAlignment};
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
#[allow(unused_imports)]
pub use targets::Targets;
#[allow(unused_imports)]
pub use players::Players;
pub use highlight_form::{HighlightFormWidget, FormResult};
pub use keybind_form::{KeybindFormWidget, KeybindFormResult, KeybindActionType};
pub use window_editor_v2::{WindowEditor, WindowEditorResult};
pub use settings_editor::{SettingsEditor, SettingItem, SettingValue};
pub use highlight_browser::{HighlightBrowser, HighlightEntry};

use ratatui::{
    layout::Rect,
};

pub struct UiLayout {
    pub main_area: Rect,
    pub input_area: Rect,
}

impl UiLayout {
    pub fn calculate(area: Rect, cmd_row: u16, cmd_col: u16, cmd_height: u16, cmd_width: u16) -> Self {
        // Clamp cmd_col to fit within area
        let clamped_col = cmd_col.min(area.width.saturating_sub(1));

        // Calculate actual command input area based on config
        let input_row = if cmd_row == 0 {
            // Default: bottom of screen
            area.height.saturating_sub(cmd_height)
        } else {
            cmd_row.min(area.height.saturating_sub(cmd_height))
        };

        // Calculate available width from the starting column
        let available_width = area.width.saturating_sub(clamped_col);

        let input_width = if cmd_width == 0 {
            // 0 means use full width (from starting column to edge)
            available_width
        } else {
            cmd_width.min(available_width)
        };

        let input_area = Rect {
            x: area.x + clamped_col,
            y: area.y + input_row,
            width: input_width,
            height: cmd_height.min(area.height),
        };

        // Main area is everything above the command input
        let main_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: input_row.saturating_sub(area.y),
        };

        Self {
            main_area,
            input_area,
        }
    }
}
