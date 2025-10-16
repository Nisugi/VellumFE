use ratatui::layout::Rect;
use std::collections::HashMap;
use super::{TextWindow, TabbedTextWindow, TabBarPosition, ProgressBar, Countdown, Indicator, Compass, InjuryDoll, Hands, Hand, HandType, Dashboard, DashboardLayout, StyledText, Targets, Players};
use super::active_effects;
use ratatui::buffer::Buffer;

/// Enum to hold different widget types
pub enum Widget {
    Text(TextWindow),
    Tabbed(TabbedTextWindow),
    Progress(ProgressBar),
    Countdown(Countdown),
    Indicator(Indicator),
    Compass(Compass),
    InjuryDoll(InjuryDoll),
    Hands(Hands),
    Hand(Hand),
    Dashboard(Dashboard),
    ActiveEffects(active_effects::ActiveEffects),
    Targets(Targets),
    Players(Players),
}

impl Widget {
    /// Render the widget with focus indicator
    pub fn render_with_focus(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        focused: bool,
        server_time_offset: i64,
        selection_state: Option<&crate::selection::SelectionState>,
        selection_bg_color: &str,
        window_index: usize,
    ) {
        match self {
            Widget::Text(w) => w.render_with_focus(area, buf, focused, selection_state, selection_bg_color, window_index),
            Widget::Tabbed(w) => w.render_with_focus(area, buf, focused, selection_state, selection_bg_color, window_index),
            Widget::Progress(w) => w.render_with_focus(area, buf, focused),
            Widget::Countdown(w) => w.render_with_focus(area, buf, focused, server_time_offset),
            Widget::Indicator(w) => w.render_with_focus(area, buf, focused),
            Widget::Compass(w) => w.render_with_focus(area, buf, focused),
            Widget::InjuryDoll(w) => w.render_with_focus(area, buf, focused),
            Widget::Hands(w) => w.render_with_focus(area, buf, focused),
            Widget::Hand(w) => w.render_with_focus(area, buf, focused),
            Widget::Dashboard(w) => w.render_with_focus(area, buf, focused),
            Widget::ActiveEffects(w) => w.render_with_focus(area, buf, focused),
            Widget::Targets(w) => {
                w.render(area, buf);
            }
            Widget::Players(w) => {
                w.render(area, buf);
            }
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

    /// Scroll up (only applicable for text windows and scrollable containers)
    pub fn scroll_up(&mut self, lines: usize) {
        match self {
            Widget::Text(w) => w.scroll_up(lines),
            Widget::Tabbed(w) => w.scroll_up(lines),
            Widget::ActiveEffects(w) => {
                for _ in 0..lines {
                    w.scroll_up();
                }
            }
            Widget::Targets(w) => {
                for _ in 0..lines {
                    w.scroll_up();
                }
            }
            Widget::Players(w) => {
                for _ in 0..lines {
                    w.scroll_up();
                }
            }
            _ => {}
        }
    }

    /// Scroll down (only applicable for text windows and scrollable containers)
    pub fn scroll_down(&mut self, lines: usize) {
        match self {
            Widget::Text(w) => w.scroll_down(lines),
            Widget::Tabbed(w) => w.scroll_down(lines),
            Widget::ActiveEffects(w) => {
                for _ in 0..lines {
                    w.scroll_down();
                }
            }
            Widget::Targets(w) => {
                for _ in 0..lines {
                    w.scroll_down();
                }
            }
            Widget::Players(w) => {
                for _ in 0..lines {
                    w.scroll_down();
                }
            }
            _ => {}
        }
    }

    /// Set width (for text windows)
    pub fn set_width(&mut self, width: u16) {
        match self {
            Widget::Text(w) => w.set_width(width),
            Widget::Tabbed(w) => w.update_inner_width(width),
            _ => {}
        }
    }

    /// Set border config
    pub fn set_border_config(&mut self, show_border: bool, border_style: Option<String>, border_color: Option<String>) {
        match self {
            Widget::Text(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Tabbed(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Progress(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Countdown(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Indicator(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Compass(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::InjuryDoll(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Hands(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Hand(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Dashboard(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::ActiveEffects(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Targets(w) => w.set_border_config(show_border, border_style, border_color),
            Widget::Players(w) => w.set_border_config(show_border, border_style, border_color),
        }
    }

    /// Search methods - only work for Text windows
    pub fn start_search(&mut self, pattern: &str) -> Result<usize, regex::Error> {
        match self {
            Widget::Text(w) => w.start_search(pattern),
            _ => Ok(0), // Non-text widgets don't support search
        }
    }

    pub fn clear_search(&mut self) {
        if let Widget::Text(w) = self {
            w.clear_search();
        }
    }

    pub fn next_match(&mut self) -> bool {
        if let Widget::Text(w) = self {
            w.next_match()
        } else {
            false
        }
    }

    pub fn prev_match(&mut self) -> bool {
        if let Widget::Text(w) = self {
            w.prev_match()
        } else {
            false
        }
    }

    pub fn search_info(&self) -> Option<(usize, usize)> {
        if let Widget::Text(w) = self {
            w.search_info()
        } else {
            None
        }
    }

    /// Set border sides
    pub fn set_border_sides(&mut self, border_sides: Option<Vec<String>>) {
        match self {
            Widget::Text(w) => w.set_border_sides(border_sides),
            Widget::Tabbed(_) => {}, // Tabbed windows don't support custom border sides
            Widget::Progress(w) => w.set_border_sides(border_sides),
            Widget::Countdown(w) => w.set_border_sides(border_sides),
            Widget::Indicator(w) => w.set_border_sides(border_sides),
            Widget::Compass(w) => w.set_border_sides(border_sides),
            Widget::InjuryDoll(w) => w.set_border_sides(border_sides),
            Widget::Hands(w) => w.set_border_sides(border_sides),
            Widget::Hand(w) => w.set_border_sides(border_sides),
            Widget::Dashboard(w) => w.set_border_sides(border_sides),
            Widget::ActiveEffects(w) => w.set_border_sides(border_sides),
            Widget::Targets(_) => {},
            Widget::Players(_) => {},
        }
    }

    /// Set title
    pub fn set_title(&mut self, title: String) {
        match self {
            Widget::Text(w) => w.set_title(title),
            Widget::Tabbed(w) => w.set_title(title),
            Widget::Progress(w) => w.set_title(title),
            Widget::Countdown(w) => w.set_title(title),
            Widget::Indicator(w) => w.set_title(title),
            Widget::Compass(w) => w.set_title(title),
            Widget::InjuryDoll(w) => w.set_title(title),
            Widget::Hands(w) => w.set_title(title),
            Widget::Hand(w) => w.set_title(title),
            Widget::Dashboard(w) => w.set_title(title),
            Widget::ActiveEffects(w) => w.set_title(title),
            Widget::Targets(w) => w.set_title(title),
            Widget::Players(w) => w.set_title(title),
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
            Widget::ActiveEffects(w) => w.set_transparent_background(transparent),
            _ => {}
        }
    }

    /// Set content alignment
    pub fn set_content_align(&mut self, align: Option<String>) {
        match self {
            Widget::Progress(w) => w.set_content_align(align),
            Widget::Countdown(w) => w.set_content_align(align),
            Widget::Compass(w) => w.set_content_align(align),
            Widget::InjuryDoll(w) => w.set_content_align(align),
            Widget::Indicator(w) => w.set_content_align(align),
            Widget::Dashboard(w) => w.set_content_align(align),
            _ => {}
        }
    }

    /// Set background color
    pub fn set_background_color(&mut self, color: Option<String>) {
        match self {
            Widget::Hands(w) => w.set_background_color(color),
            Widget::Hand(w) => w.set_background_color(color),
            Widget::Compass(w) => w.set_background_color(color),
            Widget::InjuryDoll(w) => w.set_background_color(color),
            _ => {}
        }
    }

    /// Add or update an active effect (only for ActiveEffects widgets)
    pub fn add_or_update_effect(&mut self, id: String, name: String, value: u32, time: String, color: Option<String>) {
        if let Widget::ActiveEffects(w) = self {
            w.add_or_update_effect(id, name, value, time, color);
        }
    }

    /// Clear all active effects (only for ActiveEffects widgets)
    pub fn clear_active_effects(&mut self) {
        if let Widget::ActiveEffects(w) = self {
            w.clear();
        }
    }

    /// Toggle display between spell ID and name (only for ActiveEffects widgets)
    pub fn toggle_effect_display(&mut self) {
        if let Widget::ActiveEffects(w) = self {
            w.toggle_display();
        }
    }

    /// Get total line count (for memory tracking)
    pub fn line_count(&self) -> usize {
        match self {
            Widget::Text(w) => w.wrapped_line_count(),
            Widget::ActiveEffects(_) => 0,  // Active effects don't store lines
            _ => 0,  // Other widgets don't store significant line data
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

    /// Set target count (only for targets widgets)
    pub fn set_target_count(&mut self, count: u32) {
        if let Widget::Targets(w) = self {
            w.set_count(count);
        }
    }

    /// Set targets from text (only for targets widgets)
    pub fn set_targets_from_text(&mut self, text: &str) {
        if let Widget::Targets(w) = self {
            w.set_targets_from_text(text);
        }
    }

    /// Set player count (only for players widgets)
    pub fn set_player_count(&mut self, count: u32) {
        if let Widget::Players(w) = self {
            w.set_count(count);
        }
    }

    /// Set players from text (only for players widgets)
    pub fn set_players_from_text(&mut self, text: &str) {
        if let Widget::Players(w) = self {
            w.set_players_from_text(text);
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
    pub border_sides: Option<Vec<String>>, // Which sides to show borders on
    pub title: Option<String>, // Custom title
    pub content_align: Option<String>, // Content alignment: "top-left", "bottom-left", etc.
    pub background_color: Option<String>, // Background color for entire widget area
    pub bar_color: Option<String>,  // Progress bar color
    pub bar_background_color: Option<String>, // Progress bar background
    pub text_color: Option<String>, // Text color for progress bars and other widgets
    pub transparent_background: bool,  // If true, unfilled portions are transparent
    pub countdown_icon: Option<String>, // Icon character for countdown widgets
    pub indicator_colors: Option<Vec<String>>, // Indicator state colors
    pub compass_active_color: Option<String>, // Compass active exit color
    pub compass_inactive_color: Option<String>, // Compass inactive exit color
    pub dashboard_layout: Option<String>,  // Dashboard layout type
    pub dashboard_indicators: Option<Vec<crate::config::DashboardIndicatorDef>>, // Dashboard indicators
    pub dashboard_spacing: Option<u16>,  // Dashboard spacing
    pub dashboard_hide_inactive: Option<bool>,  // Hide inactive in dashboard
    pub visible_count: Option<usize>,  // For scrollable containers: items to show
    pub effect_category: Option<String>,  // For active_effects: filter category
    pub tabs: Option<Vec<crate::config::TabConfig>>,  // For tabbed windows: tab configurations
    pub tab_bar_position: Option<String>,  // "top" or "bottom"
    pub tab_active_color: Option<String>,  // Color for active tab
    pub tab_inactive_color: Option<String>,  // Color for inactive tabs
    pub tab_unread_color: Option<String>,  // Color for tabs with unread messages
    pub tab_unread_prefix: Option<String>,  // Prefix for tabs with unread (e.g., "* ")
    pub hand_icon: Option<String>,  // Icon for hand widgets (e.g., "L:", "R:", "S:")
    pub numbers_only: Option<bool>,  // For progress bars: strip words, show only numbers
    // Injury doll colors
    pub injury_default_color: Option<String>,
    pub injury1_color: Option<String>,
    pub injury2_color: Option<String>,
    pub injury3_color: Option<String>,
    pub scar1_color: Option<String>,
    pub scar2_color: Option<String>,
    pub scar3_color: Option<String>,
}

pub struct WindowManager {
    windows: HashMap<String, Widget>,
    config: Vec<WindowConfig>,
    pub stream_map: HashMap<String, String>, // stream name -> window name (public for routing)
    highlights: HashMap<String, crate::config::HighlightPattern>, // Highlight patterns for text windows
    global_countdown_icon: String,  // Global default countdown icon
}

impl WindowManager {
    pub fn new(
        configs: Vec<WindowConfig>,
        highlights: HashMap<String, crate::config::HighlightPattern>,
        global_countdown_icon: String,
    ) -> Self {
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
                    progress_bar.set_border_sides(config.border_sides.clone());

                    tracing::debug!("ProgressBar {}: bar_color={:?}, bg_color={:?}",
                        config.name, config.bar_color, config.bar_background_color);
                    progress_bar.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                    progress_bar.set_transparent_background(config.transparent_background);
                    progress_bar.set_text_color(config.text_color.clone());
                    progress_bar.set_content_align(config.content_align.clone());
                    progress_bar.set_numbers_only(config.numbers_only.unwrap_or(false));
                    Widget::Progress(progress_bar)
                }
                "countdown" => {
                    let mut countdown = Countdown::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    countdown.set_border_sides(config.border_sides.clone());
                    countdown.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                    countdown.set_transparent_background(config.transparent_background);
                    countdown.set_content_align(config.content_align.clone());

                    // Set countdown icon: use window-specific if set, otherwise global default
                    let icon_str = config.countdown_icon.as_ref().unwrap_or(&global_countdown_icon);
                    if let Some(icon_char) = icon_str.chars().next() {
                        countdown.set_icon(icon_char);
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
                    indicator.set_border_sides(config.border_sides.clone());
                    if let Some(ref colors) = config.indicator_colors {
                        indicator.set_colors(colors.clone());
                    }
                    indicator.set_content_align(config.content_align.clone());
                    Widget::Indicator(indicator)
                }
                "compass" => {
                    let mut compass = Compass::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    compass.set_border_sides(config.border_sides.clone());
                    compass.set_content_align(config.content_align.clone());
                    compass.set_background_color(config.background_color.clone());
                    compass.set_colors(config.compass_active_color.clone(), config.compass_inactive_color.clone());
                    Widget::Compass(compass)
                }
                "injury_doll" | "injuries" => {
                    let mut injury_doll = InjuryDoll::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    injury_doll.set_content_align(config.content_align.clone());
                    injury_doll.set_background_color(config.background_color.clone());

                    // Set custom injury/scar colors if configured
                    let colors = vec![
                        config.injury_default_color.clone().unwrap_or_else(|| "#333333".to_string()),
                        config.injury1_color.clone().unwrap_or_else(|| "#aa5500".to_string()),
                        config.injury2_color.clone().unwrap_or_else(|| "#ff8800".to_string()),
                        config.injury3_color.clone().unwrap_or_else(|| "#ff0000".to_string()),
                        config.scar1_color.clone().unwrap_or_else(|| "#999999".to_string()),
                        config.scar2_color.clone().unwrap_or_else(|| "#777777".to_string()),
                        config.scar3_color.clone().unwrap_or_else(|| "#555555".to_string()),
                    ];
                    injury_doll.set_colors(colors);

                    Widget::InjuryDoll(injury_doll)
                }
                "hands" => {
                    let mut hands = Hands::new(&title)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    hands.set_border_sides(config.border_sides.clone());
                    hands.set_background_color(config.background_color.clone());
                    hands.set_text_color(config.text_color.clone());
                    Widget::Hands(hands)
                }
                "lefthand" => {
                    let mut hand = Hand::new(&title, HandType::Left)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    hand.set_border_sides(config.border_sides.clone());
                    if let Some(ref icon) = config.hand_icon {
                        hand.set_icon(icon.clone());
                    }
                    hand.set_background_color(config.background_color.clone());
                    hand.set_text_color(config.text_color.clone());
                    Widget::Hand(hand)
                }
                "righthand" => {
                    let mut hand = Hand::new(&title, HandType::Right)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    hand.set_border_sides(config.border_sides.clone());
                    if let Some(ref icon) = config.hand_icon {
                        hand.set_icon(icon.clone());
                    }
                    hand.set_background_color(config.background_color.clone());
                    hand.set_text_color(config.text_color.clone());
                    Widget::Hand(hand)
                }
                "spellhand" => {
                    let mut hand = Hand::new(&title, HandType::Spell)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    hand.set_border_sides(config.border_sides.clone());
                    if let Some(ref icon) = config.hand_icon {
                        hand.set_icon(icon.clone());
                    }
                    hand.set_background_color(config.background_color.clone());
                    hand.set_text_color(config.text_color.clone());
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

                    dashboard.set_transparent_background(config.transparent_background);
                    dashboard.set_content_align(config.content_align.clone());
                    Widget::Dashboard(dashboard)
                }
                "active_effects" => {
                    let mut active_effects = active_effects::ActiveEffects::new(
                        &title,
                        config.effect_category.clone().unwrap_or_else(|| "All".to_string())
                    );

                    active_effects.set_border_config(
                        config.show_border,
                        config.border_style.clone(),
                        config.border_color.clone(),
                    );
                    active_effects.set_border_sides(config.border_sides.clone());

                    if let Some(ref color) = config.bar_color {
                        active_effects.set_bar_color(color.clone());
                    }

                    active_effects.set_visible_count(config.visible_count);

                    active_effects.set_transparent_background(config.transparent_background);

                    Widget::ActiveEffects(active_effects)
                }
                "tabbed" => {
                    // Parse tab bar position
                    let tab_bar_position = if let Some(ref pos_str) = config.tab_bar_position {
                        match pos_str.to_lowercase().as_str() {
                            "bottom" => TabBarPosition::Bottom,
                            _ => TabBarPosition::Top,
                        }
                    } else {
                        TabBarPosition::Top
                    };

                    let mut tabbed_window = TabbedTextWindow::new(
                        &title,
                        tab_bar_position,
                    );

                    // Set border config
                    tabbed_window.set_border_config(
                        config.show_border,
                        config.border_style.clone(),
                        config.border_color.clone(),
                    );

                    // Set background
                    tabbed_window.set_transparent_background(config.transparent_background);
                    tabbed_window.set_background_color(config.background_color.clone());

                    // Set tab colors if specified
                    if let Some(ref color) = config.tab_active_color {
                        tabbed_window.set_tab_active_color(color.clone());
                    }
                    if let Some(ref color) = config.tab_inactive_color {
                        tabbed_window.set_tab_inactive_color(color.clone());
                    }
                    if let Some(ref color) = config.tab_unread_color {
                        tabbed_window.set_tab_unread_color(color.clone());
                    }
                    if let Some(ref prefix) = config.tab_unread_prefix {
                        tabbed_window.set_unread_prefix(prefix.clone());
                    }

                    // Add tabs from config
                    if let Some(ref tabs) = config.tabs {
                        tracing::debug!("Creating tabbed window '{}' with {} tabs", config.name, tabs.len());
                        for tab in tabs {
                            tracing::debug!("  Adding tab '{}' -> stream '{}'", tab.name, tab.stream);
                            tabbed_window.add_tab(
                                tab.name.clone(),
                                tab.stream.clone(),
                                config.buffer_size,
                            );
                        }
                    } else {
                        tracing::warn!("Tabbed window '{}' has no tabs configured!", config.name);
                    }

                    Widget::Tabbed(tabbed_window)
                }
                "entity" => {
                    // Determine which entity type based on name or streams
                    if config.name.contains("player") || config.streams.iter().any(|s| s.contains("player")) {
                        let mut players = Players::new(&title);
                        players.set_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                        Widget::Players(players)
                    } else {
                        // Default to targets
                        let mut targets = Targets::new(&title);
                        targets.set_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                        Widget::Targets(targets)
                    }
                }
                "targets" => {
                    // Legacy support - still accept "targets" widget_type
                    let mut targets = Targets::new(&title);
                    targets.set_border_config(
                        config.show_border,
                        config.border_style.clone(),
                        config.border_color.clone(),
                    );
                    Widget::Targets(targets)
                }
                "players" => {
                    // Legacy support - still accept "players" widget_type
                    let mut players = Players::new(&title);
                    players.set_border_config(
                        config.show_border,
                        config.border_style.clone(),
                        config.border_color.clone(),
                    );
                    Widget::Players(players)
                }
                _ => {
                    // Default to text window
                    let mut text_window = TextWindow::new(&title, config.buffer_size)
                        .with_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                    text_window.set_background_color(config.background_color.clone());
                    text_window.set_content_align(config.content_align.clone());
                    // Set highlights (convert HashMap to Vec)
                    let highlights_vec: Vec<_> = highlights.values().cloned().collect();
                    text_window.set_highlights(highlights_vec);
                    Widget::Text(text_window)
                }
            };

            windows.insert(config.name.clone(), widget);

            // Map each stream to this window
            for stream in &config.streams {
                stream_map.insert(stream.clone(), config.name.clone());
            }

            // For tabbed windows, also map all tab streams
            if config.widget_type == "tabbed" {
                if let Some(ref tabs) = config.tabs {
                    for tab in tabs {
                        stream_map.insert(tab.stream.clone(), config.name.clone());
                    }
                }
            }
        }

        Self {
            windows,
            config: configs,
            stream_map,
            highlights,
            global_countdown_icon,
        }
    }

    /// Check if a window exists for a specific stream
    pub fn has_window_for_stream(&self, stream: &str) -> bool {
        self.stream_map.contains_key(stream)
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

    pub fn get_window_const(&self, name: &str) -> Option<&Widget> {
        self.windows.get(name)
    }

    /// Get window names in configured order
    pub fn get_window_names(&self) -> Vec<String> {
        self.config.iter().map(|c| c.name.clone()).collect()
    }

    /// Get window's effect_category from config
    pub fn get_window_effect_category(&self, name: &str) -> Option<String> {
        self.config
            .iter()
            .find(|c| c.name == name)
            .and_then(|c| c.effect_category.clone())
    }

    /// Get window's configured width (cols)
    pub fn get_window_width(&self, name: &str) -> Option<u16> {
        self.config
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.cols)
    }

    /// Get window's border setting
    pub fn get_window_border(&self, name: &str) -> Option<bool> {
        self.config
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.show_border)
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

            // CRITICAL: Ensure rect coordinates are within buffer bounds
            // Ratatui will panic if we try to render outside the buffer
            // Check absolute positions against buffer bounds
            let buffer_right = area.x + area.width;
            let buffer_bottom = area.y + area.height;

            if x >= buffer_right || y >= buffer_bottom {
                tracing::warn!(
                    "SKIPPING window '{}' - position ({}, {}) outside buffer bounds (right: {}, bottom: {})",
                    config.name, x, y, buffer_right, buffer_bottom
                );
                continue;
            }

            // Clamp window dimensions to fit within buffer bounds
            let max_width = buffer_right.saturating_sub(x);
            let max_height = buffer_bottom.saturating_sub(y);

            let width = config.cols.min(max_width);
            let height = config.rows.min(max_height);

            // Skip if dimensions would be zero
            if width == 0 || height == 0 {
                tracing::warn!(
                    "SKIPPING window '{}' - zero dimensions after clamping ({}x{})",
                    config.name, width, height
                );
                continue;
            }

            // Final safety check: ensure rect doesn't extend beyond buffer
            if x + width > buffer_right || y + height > buffer_bottom {
                tracing::warn!(
                    "SKIPPING window '{}' - rect would extend beyond buffer (x:{} y:{} w:{} h:{} right:{} bottom:{})",
                    config.name, x, y, width, height, buffer_right, buffer_bottom
                );
                continue;
            }

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
                        progress_bar.set_border_sides(config.border_sides.clone());
                        progress_bar.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                        progress_bar.set_transparent_background(config.transparent_background);
                        progress_bar.set_text_color(config.text_color.clone());
                        progress_bar.set_content_align(config.content_align.clone());
                        progress_bar.set_numbers_only(config.numbers_only.unwrap_or(false));
                        Widget::Progress(progress_bar)
                    }
                    "countdown" => {
                        let mut countdown = Countdown::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        countdown.set_border_sides(config.border_sides.clone());
                        countdown.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                        countdown.set_transparent_background(config.transparent_background);
                        countdown.set_content_align(config.content_align.clone());

                        // Set countdown icon: use window-specific if set, otherwise global default
                        let icon_str = config.countdown_icon.as_ref().unwrap_or(&self.global_countdown_icon);
                        if let Some(icon_char) = icon_str.chars().next() {
                            countdown.set_icon(icon_char);
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
                        indicator.set_border_sides(config.border_sides.clone());
                        if let Some(ref colors) = config.indicator_colors {
                            indicator.set_colors(colors.clone());
                        }
                        indicator.set_content_align(config.content_align.clone());
                        Widget::Indicator(indicator)
                    }
                    "compass" => {
                        let mut compass = Compass::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        compass.set_border_sides(config.border_sides.clone());
                        compass.set_content_align(config.content_align.clone());
                        compass.set_background_color(config.background_color.clone());
                        compass.set_colors(config.compass_active_color.clone(), config.compass_inactive_color.clone());
                        Widget::Compass(compass)
                    }
                    "injury_doll" | "injuries" => {
                        let mut injury_doll = InjuryDoll::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        injury_doll.set_content_align(config.content_align.clone());
                        injury_doll.set_background_color(config.background_color.clone());

                        // Set custom injury/scar colors if configured
                        let colors = vec![
                            config.injury_default_color.clone().unwrap_or_else(|| "#333333".to_string()),
                            config.injury1_color.clone().unwrap_or_else(|| "#aa5500".to_string()),
                            config.injury2_color.clone().unwrap_or_else(|| "#ff8800".to_string()),
                            config.injury3_color.clone().unwrap_or_else(|| "#ff0000".to_string()),
                            config.scar1_color.clone().unwrap_or_else(|| "#999999".to_string()),
                            config.scar2_color.clone().unwrap_or_else(|| "#777777".to_string()),
                            config.scar3_color.clone().unwrap_or_else(|| "#555555".to_string()),
                        ];
                        injury_doll.set_colors(colors);

                        Widget::InjuryDoll(injury_doll)
                    }
                    "hands" => {
                        let mut hands = Hands::new(&title)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        hands.set_border_sides(config.border_sides.clone());
                        hands.set_background_color(config.background_color.clone());
                        hands.set_text_color(config.text_color.clone());
                        Widget::Hands(hands)
                    }
                    "lefthand" => {
                        let mut hand = Hand::new(&title, HandType::Left)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        hand.set_border_sides(config.border_sides.clone());
                        if let Some(ref icon) = config.hand_icon {
                            hand.set_icon(icon.clone());
                        }
                        hand.set_background_color(config.background_color.clone());
                        hand.set_text_color(config.text_color.clone());
                        Widget::Hand(hand)
                    }
                    "righthand" => {
                        let mut hand = Hand::new(&title, HandType::Right)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        hand.set_border_sides(config.border_sides.clone());
                        if let Some(ref icon) = config.hand_icon {
                            hand.set_icon(icon.clone());
                        }
                        hand.set_background_color(config.background_color.clone());
                        hand.set_text_color(config.text_color.clone());
                        Widget::Hand(hand)
                    }
                    "spellhand" => {
                        let mut hand = Hand::new(&title, HandType::Spell)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        hand.set_border_sides(config.border_sides.clone());
                        if let Some(ref icon) = config.hand_icon {
                            hand.set_icon(icon.clone());
                        }
                        hand.set_background_color(config.background_color.clone());
                        hand.set_text_color(config.text_color.clone());
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

                        dashboard.set_content_align(config.content_align.clone());
                        Widget::Dashboard(dashboard)
                    }
                    "tabbed" => {
                        // Parse tab bar position
                        let tab_bar_position = if let Some(ref pos_str) = config.tab_bar_position {
                            match pos_str.to_lowercase().as_str() {
                                "bottom" => TabBarPosition::Bottom,
                                _ => TabBarPosition::Top,
                            }
                        } else {
                            TabBarPosition::Top
                        };

                        let mut tabbed_window = TabbedTextWindow::new(
                            &title,
                            tab_bar_position,
                        );

                        // Set border config
                        tabbed_window.set_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );

                        // Set background
                        tabbed_window.set_transparent_background(config.transparent_background);
                        tabbed_window.set_background_color(config.background_color.clone());

                        // Set tab colors if specified
                        if let Some(ref color) = config.tab_active_color {
                            tabbed_window.set_tab_active_color(color.clone());
                        }
                        if let Some(ref color) = config.tab_inactive_color {
                            tabbed_window.set_tab_inactive_color(color.clone());
                        }
                        if let Some(ref color) = config.tab_unread_color {
                            tabbed_window.set_tab_unread_color(color.clone());
                        }
                        if let Some(ref prefix) = config.tab_unread_prefix {
                            tabbed_window.set_unread_prefix(prefix.clone());
                        }

                        // Add tabs from config
                        if let Some(ref tabs) = config.tabs {
                            tracing::debug!("Creating tabbed window '{}' with {} tabs (update_config)", config.name, tabs.len());
                            for tab in tabs {
                                tracing::debug!("  Adding tab '{}' -> stream '{}'", tab.name, tab.stream);
                                tabbed_window.add_tab(
                                    tab.name.clone(),
                                    tab.stream.clone(),
                                    config.buffer_size,
                                );
                            }
                        } else {
                            tracing::warn!("Tabbed window '{}' has no tabs configured! (update_config)", config.name);
                        }

                        Widget::Tabbed(tabbed_window)
                    }
                    "entity" => {
                        // Determine which entity type based on name or streams
                        if config.name.contains("player") || config.streams.iter().any(|s| s.contains("player")) {
                            let mut players = Players::new(&title);
                            players.set_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                            Widget::Players(players)
                        } else {
                            // Default to targets
                            let mut targets = Targets::new(&title);
                            targets.set_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                            Widget::Targets(targets)
                        }
                    }
                    "targets" => {
                        // Legacy support
                        let mut targets = Targets::new(&title);
                        targets.set_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                        Widget::Targets(targets)
                    }
                    "players" => {
                        // Legacy support
                        let mut players = Players::new(&title);
                        players.set_border_config(
                            config.show_border,
                            config.border_style.clone(),
                            config.border_color.clone(),
                        );
                        Widget::Players(players)
                    }
                    _ => {
                        // Default to text window
                        let mut text_window = TextWindow::new(&title, config.buffer_size)
                            .with_border_config(
                                config.show_border,
                                config.border_style.clone(),
                                config.border_color.clone(),
                            );
                        text_window.set_background_color(config.background_color.clone());
                        text_window.set_content_align(config.content_align.clone());
                        // Set highlights (convert HashMap to Vec)
                        let highlights_vec: Vec<_> = self.highlights.values().cloned().collect();
                        text_window.set_highlights(highlights_vec);
                        Widget::Text(text_window)
                    }
                };

                self.windows.insert(config.name.clone(), widget);

                // Map each stream to this window
                for stream in &config.streams {
                    self.stream_map.insert(stream.clone(), config.name.clone());
                }

                // For tabbed windows, also map all tab streams
                if config.widget_type == "tabbed" {
                    if let Some(ref tabs) = config.tabs {
                        for tab in tabs {
                            self.stream_map.insert(tab.stream.clone(), config.name.clone());
                        }
                    }
                }
            } else {
                // Window exists - update its properties
                if let Some(window) = self.windows.get_mut(&config.name) {
                    window.set_border_config(
                        config.show_border,
                        config.border_style.clone(),
                        config.border_color.clone(),
                    );
                    window.set_border_sides(config.border_sides.clone());

                    let title = config.title.clone().unwrap_or_else(|| config.name.clone());
                    window.set_title(title);

                    // Update widget-specific properties
                    match window {
                        Widget::Tabbed(tabbed) => {
                            // Update background settings
                            tabbed.set_transparent_background(config.transparent_background);
                            tabbed.set_background_color(config.background_color.clone());

                            // Update tab bar position
                            let tab_bar_position = if let Some(ref pos_str) = config.tab_bar_position {
                                match pos_str.to_lowercase().as_str() {
                                    "bottom" => TabBarPosition::Bottom,
                                    _ => TabBarPosition::Top,
                                }
                            } else {
                                TabBarPosition::Top
                            };
                            tabbed.set_tab_bar_position(tab_bar_position);

                            // Update tab colors
                            if let Some(ref color) = config.tab_active_color {
                                tabbed.set_tab_active_color(color.clone());
                            }
                            if let Some(ref color) = config.tab_inactive_color {
                                tabbed.set_tab_inactive_color(color.clone());
                            }
                            if let Some(ref color) = config.tab_unread_color {
                                tabbed.set_tab_unread_color(color.clone());
                            }
                            if let Some(ref prefix) = config.tab_unread_prefix {
                                tabbed.set_unread_prefix(prefix.clone());
                            }

                            // Sync tabs from config
                            if let Some(ref tabs) = config.tabs {
                                // Get current tab names
                                let current_tabs = tabbed.get_tab_names();
                                let config_tab_names: Vec<String> = tabs.iter().map(|t| t.name.clone()).collect();

                                // Add new tabs that don't exist
                                for tab in tabs {
                                    if !current_tabs.contains(&tab.name) {
                                        tracing::debug!("Adding new tab '{}' to existing window '{}'", tab.name, config.name);
                                        tabbed.add_tab(
                                            tab.name.clone(),
                                            tab.stream.clone(),
                                            config.buffer_size,
                                        );
                                        // Map the new tab's stream
                                        self.stream_map.insert(tab.stream.clone(), config.name.clone());
                                    }
                                }

                                // Remove tabs that are no longer in config
                                for current_tab_name in &current_tabs {
                                    if !config_tab_names.contains(current_tab_name) {
                                        tracing::debug!("Removing tab '{}' from existing window '{}'", current_tab_name, config.name);
                                        tabbed.remove_tab(current_tab_name);
                                    }
                                }

                                // Reorder tabs to match config order
                                tabbed.reorder_tabs(&config_tab_names);
                            }
                        }
                        Widget::Progress(progress) => {
                            progress.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                            progress.set_transparent_background(config.transparent_background);
                            progress.set_content_align(config.content_align.clone());
                        }
                        Widget::Countdown(countdown) => {
                            countdown.set_colors(config.bar_color.clone(), config.bar_background_color.clone());
                            countdown.set_transparent_background(config.transparent_background);
                            countdown.set_content_align(config.content_align.clone());
                            if let Some(ref icon_str) = config.countdown_icon {
                                if let Some(icon_char) = icon_str.chars().next() {
                                    countdown.set_icon(icon_char);
                                }
                            }
                        }
                        Widget::Indicator(indicator) => {
                            if let Some(ref colors) = config.indicator_colors {
                                indicator.set_colors(colors.clone());
                            }
                            indicator.set_content_align(config.content_align.clone());
                        }
                        Widget::Compass(compass) => {
                            compass.set_content_align(config.content_align.clone());
                            compass.set_background_color(config.background_color.clone());
                            compass.set_colors(config.compass_active_color.clone(), config.compass_inactive_color.clone());
                        }
                        Widget::InjuryDoll(injury_doll) => {
                            injury_doll.set_content_align(config.content_align.clone());
                            injury_doll.set_background_color(config.background_color.clone());
                        }
                        Widget::Dashboard(dashboard) => {
                            dashboard.set_content_align(config.content_align.clone());
                            if let Some(spacing) = config.dashboard_spacing {
                                dashboard.set_spacing(spacing);
                            }
                            if let Some(hide) = config.dashboard_hide_inactive {
                                dashboard.set_hide_inactive(hide);
                            }
                        }
                        Widget::Text(text_window) => {
                            text_window.set_background_color(config.background_color.clone());
                            // Update highlights
                            let highlights_vec: Vec<_> = self.highlights.values().cloned().collect();
                            text_window.set_highlights(highlights_vec);
                        }
                        Widget::Hand(hand) => {
                            if let Some(ref icon) = config.hand_icon {
                                hand.set_icon(icon.clone());
                            }
                        }
                        _ => {
                            // Other widget types (targets, players, hands multi) don't have additional properties to update
                        }
                    }
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

    /// Update highlight patterns (reload after config change)
    pub fn update_highlights(&mut self, highlights: HashMap<String, crate::config::HighlightPattern>) {
        self.highlights = highlights;
        // Text windows will use the updated highlights on next render
        // (highlights are checked via window_manager reference, not stored in windows)
    }
}
