use ratatui::layout::Rect;
use std::collections::HashMap;
use super::{TextWindow, ProgressBar, Countdown, Indicator, Compass, InjuryDoll, Hands, Hand, HandType, Dashboard, DashboardLayout, StyledText};
use ratatui::buffer::Buffer;

/// Enum to hold different widget types
pub enum Widget {
    Text(TextWindow),
    Progress(ProgressBar),
    Countdown(Countdown),
    Indicator(Indicator),
    Compass(Compass),
    InjuryDoll(InjuryDoll),
    Hands(Hands),
    Hand(Hand),
    Dashboard(Dashboard),
}

impl Widget {
    /// Render the widget with focus indicator
    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        match self {
            Widget::Text(w) => w.render_with_focus(area, buf, focused),
            Widget::Progress(w) => w.render_with_focus(area, buf, focused),
            Widget::Countdown(w) => w.render_with_focus(area, buf, focused),
            Widget::Indicator(w) => w.render_with_focus(area, buf, focused),
            Widget::Compass(w) => w.render_with_focus(area, buf, focused),
            Widget::InjuryDoll(w) => w.render_with_focus(area, buf, focused),
            Widget::Hands(w) => w.render_with_focus(area, buf, focused),
            Widget::Hand(w) => w.render_with_focus(area, buf, focused),
            Widget::Dashboard(w) => w.render_with_focus(area, buf, focused),
        }
    }

    /// Add text to the widget (only applicable for text windows)
    pub fn add_text(&mut self, styled: StyledText) {
        if let Widget::Text(w) = self {
            w.add_text(styled);
        }
    }

    /// Finish a line (only applicable for text windows)
    pub fn finish_line(&mut self, width: u16) {
        if let Widget::Text(w) = self {
            w.finish_line(width);
        }
    }

    /// Scroll up (only applicable for text windows)
    pub fn scroll_up(&mut self, lines: usize) {
        if let Widget::Text(w) = self {
            w.scroll_up(lines);
        }
    }

    /// Scroll down (only applicable for text windows)
    pub fn scroll_down(&mut self, lines: usize) {
        if let Widget::Text(w) = self {
            w.scroll_down(lines);
        }
    }

    /// Set width (for text windows)
    pub fn set_width(&mut self, width: u16) {
        if let Widget::Text(w) = self {
            w.set_width(width);
        }
    }

    /// Set border config
    pub fn set_border_config(&mut self, show_border: bool, border_style: Option<String>, border_color: Option<String>) {
        match self {
            Widget::Text(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Progress(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Countdown(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Indicator(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Compass(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::InjuryDoll(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Hands(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Hand(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Dashboard(w) => w.set_border_config(show_border, border_style, border_color),
        }
    }

    /// Set title
    pub fn set_title(&mut self, title: String) {
        match self {
            Widget::Text(w) => w.set_title(title),
            Widget::Progress(w) => w.set_title(title),
            Widget::Countdown(w) => w.set_title(title),
            Widget::Indicator(w) => w.set_title(title),
            Widget::Compass(w) => w.set_title(title),
            Widget::InjuryDoll(w) => w.set_title(title),
            Widget::Hands(w) => w.set_title(title),
            Widget::Hand(w) => w.set_title(title),
            Widget::Dashboard(w) => w.set_title(title),
        }
    }

    /// Set progress value (only for progress bars)
    pub fn set_progress(&mut self, current: u32, max: u32) {
        if let Widget::Progress(w) = self {
            w.set_value(current, max);
        }
    }

    /// Set progress value with custom text (only for progress bars)
    pub fn set_progress_with_text(&mut self, current: u32, max: u32, custom_text: Option<String>) {
        if let Widget::Progress(w) = self {
            w.set_value_with_text(current, max, custom_text);
        }
    }

    /// Set bar colors (only for progress bars and countdowns)
    pub fn set_bar_colors(&mut self, bar_color: Option<String>, bg_color: Option<String>) {
        match self {
            Widget::Progress(w) => w.set_colors(bar_color, bg_color),
            Widget::Countdown(w) => w.set_colors(bar_color, bg_color),
            _ => {}
        }
    }

    /// Set transparent background (only for progress bars and countdowns)
    pub fn set_transparent_background(&mut self, transparent: bool) {
        match self {
            Widget::Progress(w) => w.set_transparent_background(transparent),
            Widget::Countdown(w) => w.set_transparent_background(transparent),
            _ => {}
        }
    }

    /// Set countdown end time (only for countdown widgets)
    pub fn set_countdown(&mut self, end_time: u64) {
        if let Widget::Countdown(w) = self {
            w.set_end_time(end_time);
        }
    }

    /// Set indicator value (only for indicator widgets)
    pub fn set_indicator(&mut self, value: u8) {
        if let Widget::Indicator(w) = self {
            w.set_value(value);
        }
    }

    /// Set compass directions (only for compass widgets)
    pub fn set_compass_directions(&mut self, directions: Vec<String>) {
        if let Widget::Compass(w) = self {
            w.set_directions(directions);
        }
    }

    /// Set injury doll body part (only for injury doll widgets)
    pub fn set_injury(&mut self, body_part: String, level: u8) {
        if let Widget::InjuryDoll(w) = self {
            w.set_injury(body_part, level);
        }
    }

    /// Set left hand item (only for hands widgets)
    pub fn set_left_hand(&mut self, item: String) {
        if let Widget::Hands(w) = self {
            w.set_left_hand(item);
        }
    }

    /// Set right hand item (only for hands widgets)
    pub fn set_right_hand(&mut self, item: String) {
        if let Widget::Hands(w) = self {
            w.set_right_hand(item);
        }
    }

    /// Set spell hand (only for hands widgets)
    pub fn set_spell_hand(&mut self, spell: String) {
        if let Widget::Hands(w) = self {
            w.set_spell_hand(spell);
        }
    }

    /// Set hand content (only for individual hand widgets)
    pub fn set_hand_content(&mut self, content: String) {
        if let Widget::Hand(w) = self {
            w.set_content(content);
        }
    }

    /// Set dashboard indicator value (only for dashboard widgets)
    pub fn set_dashboard_indicator(&mut self, id: &str, value: u8) {
        if let Widget::Dashboard(w) = self {
            w.set_indicator_value(id, value);
        }
    }

    /// Get mutable reference to progress bar
    pub fn as_progress_mut(&mut self) -> Option<&mut ProgressBar> {
        if let Widget::Progress(w) = self {
            Some(w)
        } else {
            None
        }
    }

    /// Get mutable reference to text window
    pub fn as_text_mut(&mut self) -> Option<&mut TextWindow> {
        if let Widget::Text(w) = self {
            Some(w)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub name: String,
    pub widget_type: String,   // "text", "indicator", "progress", "countdown", "injury"
    pub streams: Vec<String>,  // Which streams route to this window
    // Explicit positioning - each window owns its row/col dimensions
    pub row: u16,              // Starting row position (0-based)
    pub col: u16,              // Starting column position (0-based)
    pub rows: u16,             // Height in rows (this window owns these rows)
    pub cols: u16,             // Width in columns (this window owns these columns)
    // Buffer and display options
    pub buffer_size: usize,    // Lines of scrollback history for this window
    pub show_border: bool,     // Whether to show border
    pub border_style: Option<String>, // Border style
    pub border_color: Option<String>, // Hex color for border
    pub title: Option<String>, // Custom title
    pub bar_color: Option<String>,  // Progress bar color
    pub bar_background_color: Option<String>, // Progress bar background
    pub transparent_background: bool,  // If true, unfilled portions are transparent
    pub countdown_icon: Option<String>, // Icon character for countdown widgets
    pub indicator_colors: Option<Vec<String>>, // Indicator state colors
    pub dashboard_layout: Option<String>,  // Dashboard layout type
    pub dashboard_indicators: Option<Vec<crate::config::DashboardIndicatorDef>>, // Dashboard indicators
    pub dashboard_spacing: Option<u16>,  // Dashboard spacing
    pub dashboard_hide_inactive: Option<bool>,  // Hide inactive in dashboard
}

pub struct WindowManager {
    windows: HashMap<String, Widget>,
    config: Vec<WindowConfig>,
    pub stream_map: HashMap<String, String>, // stream name -> window name (public for routing)
}

impl WindowManager {
    pub fn new(configs: Vec<WindowConfig>) -> Self {
        let mut windows = HashMap::new();
        let mut stream_map = HashMap::new();

        // Create windows and build stream routing map
        for config in &configs {
            let title = config.title.clone().unwrap_or_else(|| config.name.clone());

            // Create the appropriate widget type
            let widget = match config.widget_type.as_str() {
                "progress" => {
                    let mut progress_bar = ProgressBar::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );

                    tracing::debug!("ProgressBar {}: bar_color={:?}, bg_color={:?}",
                        config.name, config.bar_color, config.bar_background_color);
                    progress_bar.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                    progress_bar.set_transparent_background(config.transparent_background);
                    Widget::Progress(progress_bar)
                }
                "countdown" => {
                    let mut countdown = Countdown::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    countdown.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                    countdown.set_transparent_background(config.transparent_background);

                    // Set countdown icon if specified
                    if let Some(ref icon_str) = config.countdown_icon {
                        if let Some(icon_char) = icon_str.chars().next() {
                            countdown.set_icon(icon_char);
                        }
                    }

                    Widget::Countdown(countdown)
                }
                "indicator" => {
                    let mut indicator = Indicator::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    if let Some(ref colors) = config.indicator_colors {
                        indicator.set_colors(colors.clone());
                    }
                    Widget::Indicator(indicator)
                }
                "compass" => {
                    let compass = Compass::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    Widget::Compass(compass)
                }
                "injury_doll" | "injuries" => {
                    let injury_doll = InjuryDoll::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    Widget::InjuryDoll(injury_doll)
                }
                "hands" => {
                    let hands = Hands::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    Widget::Hands(hands)
                }
                "lefthand" => {
                    let hand = Hand::new(&title, HandType::Left)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    Widget::Hand(hand)
                }
                "righthand" => {
                    let hand = Hand::new(&title, HandType::Right)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    Widget::Hand(hand)
                }
                "spellhand" => {
                    let hand = Hand::new(&title, HandType::Spell)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    Widget::Hand(hand)
                }
                "dashboard" => {
                    // Parse layout from config
                    let layout = if let Some(ref layout_str) = config.dashboard_layout {
                        Self::parse_dashboard_layout(layout_str)
                    } else {
                        DashboardLayout::Horizontal
                    };

                    let mut dashboard = Dashboard::new(&title, layout)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );

                    // Set spacing and hide_inactive
                    if let Some(spacing) = config.dashboard_spacing {
                        dashboard.set_spacing(spacing);
                    }
                    if let Some(hide) = config.dashboard_hide_inactive {
                        dashboard.set_hide_inactive(hide);
                    }

                    // Add indicators
                    if let Some(ref indicators) = config.dashboard_indicators {
                        for ind in indicators {
                            dashboard.add_indicator(ind.id.clone(), ind.icon.clone(), ind.colors.clone());
                        }
                    }

                    Widget::Dashboard(dashboard)
                }
                _ => {
                    // Default to text window
                    let text_window = TextWindow::new(&title, config.buffer_size)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    Widget::Text(text_window)
                }
            };

            windows.insert(config.name.clone(), widget);

            // Map each stream to this window
            for stream in &config.streams {
                stream_map.insert(stream.clone(), config.name.clone());
            }
        }

        Self {
            windows,
            config: configs,
            stream_map,
        }
    }

    /// Get window for a specific stream name
    pub fn get_window_for_stream(&mut self, stream: &str) -> Option<&mut Widget> {
        let window_name = self.stream_map.get(stream)?;
        self.windows.get_mut(window_name)
    }

    /// Parse dashboard layout string into DashboardLayout enum
    fn parse_dashboard_layout(layout_str: &str) -> DashboardLayout {
        match layout_str.to_lowercase().as_str() {
            "horizontal" => DashboardLayout::Horizontal,
            "vertical" => DashboardLayout::Vertical,
            s if s.starts_with("grid_") => {
                // Parse "grid_2x2" -> Grid { rows: 2, cols: 2 }
                let parts: Vec<&str> = s.strip_prefix("grid_").unwrap_or("2x2").split('x').collect();
                if parts.len() == 2 {
                    let rows = parts[0].parse().unwrap_or(2);
                    let cols = parts[1].parse().unwrap_or(2);
                    DashboardLayout::Grid { rows, cols }
                } else {
                    DashboardLayout::Grid { rows: 2, cols: 2 }
                }
            }
            _ => DashboardLayout::Horizontal,
        }
    }

    /// Get window by name
    pub fn get_window(&mut self, name: &str) -> Option<&mut Widget> {
        self.windows.get_mut(name)
    }

    /// Get window names in configured order
    pub fn get_window_names(&self) -> Vec<String> {
        self.config.iter().map(|c| c.name.clone()).collect()
    }

    /// Calculate layout rectangles for all windows
    /// Windows use absolute row/col positions (in terminal cells), not relative grid
    pub fn calculate_layout(&self, area: Rect) -> HashMap<String, Rect> {
        let mut result = HashMap::new();

        if self.config.is_empty() {
            return result;
        }

        // Place each window at its absolute position
        // row/col/rows/cols are in terminal cells, not relative grid positions
        for config in &self.config {
            let x = area.x + config.col;
            let y = area.y + config.row;
            // When border is disabled, shrink the window by 2 cells (1 on each side)
            // to eliminate the empty border space
            let (width, height) = if config.show_border {
                (config.cols, config.rows)
            } else {
                (config.cols.saturating_sub(2), config.rows.saturating_sub(2))
            };

            result.insert(config.name.clone(), Rect::new(x, y, width, height));
        }

        result
    }

    /// Update all window widths based on terminal size
    pub fn update_widths(&mut self, layouts: &HashMap<String, Rect>) {
        for (name, window) in &mut self.windows {
            if let Some(rect) = layouts.get(name) {
                window.set_width(rect.width.saturating_sub(2)); // Account for borders
            }
        }
    }

    /// Update window configuration (for resize/move operations and window creation/deletion)
    pub fn update_config(&mut self, configs: Vec<WindowConfig>) {
        // Check for new windows that need to be created OR existing windows to update
        for config in &configs {
            if !self.windows.contains_key(&config.name) {
                // Create new widget based on type
                let title = config.title.clone().unwrap_or_else(|| config.name.clone());

                let widget = match config.widget_type.as_str() {
                    "progress" => {
                        let mut progress_bar = ProgressBar::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        progress_bar.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                        progress_bar.set_transparent_background(config.transparent_background);
                        Widget::Progress(progress_bar)
                    }
                    "countdown" => {
                        let mut countdown = Countdown::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        countdown.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                        countdown.set_transparent_background(config.transparent_background);

                        // Set countdown icon if specified
                        if let Some(ref icon_str) = config.countdown_icon {
                            if let Some(icon_char) = icon_str.chars().next() {
                                countdown.set_icon(icon_char);
                            }
                        }

                        Widget::Countdown(countdown)
                    }
                    "indicator" => {
                        let mut indicator = Indicator::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        if let Some(ref colors) = config.indicator_colors {
                            indicator.set_colors(colors.clone());
                        }
                        Widget::Indicator(indicator)
                    }
                    "compass" => {
                        let compass = Compass::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        Widget::Compass(compass)
                    }
                    "injury_doll" | "injuries" => {
                        let injury_doll = InjuryDoll::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        Widget::InjuryDoll(injury_doll)
                    }
                    "hands" => {
                        let hands = Hands::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        Widget::Hands(hands)
                    }
                    "lefthand" => {
                        let hand = Hand::new(&title, HandType::Left)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        Widget::Hand(hand)
                    }
                    "righthand" => {
                        let hand = Hand::new(&title, HandType::Right)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        Widget::Hand(hand)
                    }
                    "spellhand" => {
                        let hand = Hand::new(&title, HandType::Spell)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        Widget::Hand(hand)
                    }
                    "dashboard" => {
                        let layout = if let Some(ref layout_str) = config.dashboard_layout {
                            Self::parse_dashboard_layout(layout_str)
                        } else {
                            DashboardLayout::Horizontal
                        };

                        let mut dashboard = Dashboard::new(&title, layout)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );

                        if let Some(spacing) = config.dashboard_spacing {
                            dashboard.set_spacing(spacing);
                        }
                        if let Some(hide) = config.dashboard_hide_inactive {
                            dashboard.set_hide_inactive(hide);
                        }

                        if let Some(ref indicators) = config.dashboard_indicators {
                            for ind in indicators {
                                dashboard.add_indicator(
                                    ind.id.clone(),
                                    ind.icon.clone(),
                                    ind.colors.clone(),
                                );
                            }
                        }

                        Widget::Dashboard(dashboard)
                    }
                    _ => {
                        // Default to text window
                        let text_window = TextWindow::new(&title, config.buffer_size)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        Widget::Text(text_window)
                    }
                };

                self.windows.insert(config.name.clone(), widget);

                // Map each stream to this window
                for stream in &config.streams {
                    self.stream_map.insert(stream.clone(), config.name.clone());
                }
            } else {
                // Window exists - update its border config and title
                if let Some(window) = self.windows.get_mut(&config.name) {
                    window.set_border_config(
                        config.show_border,
                        config.border_style.clone(),
                        config.border_color.clone(),
                    );

                    let title = config.title.clone().unwrap_or_else(|| config.name.clone());
                    window.set_title(title);
                }
            }
        }

        // Check for windows that need to be removed
        let config_names: std::collections::HashSet<String> = configs.iter().map(|c| c.name.clone()).collect();
        let current_names: Vec<String> = self.windows.keys().cloned().collect();

        for name in current_names {
            if !config_names.contains(&name) {
                self.windows.remove(&name);
                // Remove stream mappings for this window
                self.stream_map.retain(|_, win| win != &name);
            }
        }

        self.config = configs;
    }

    /// Update a specific indicator in all dashboard widgets
    pub fn update_dashboard_indicator(&mut self, indicator_id: &str, value: u8) {
        for window in self.windows.values_mut() {
            window.set_dashboard_indicator(indicator_id, value);
        }
    }
}
