//! MiniVitals widget.
//!
//! Displays health, mana, stamina, spirit as horizontal progress bars.
//! This is a compact "stats bar" view showing all 4 vitals side by side.
//! Works with both GS4 (mana) and DR (concentration mapped to mana slot).
//!
//! Reads data from GameState.minivitals.

use crate::config::BorderSides;
use crate::core::state::MiniVitalsState;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Widget},
};

/// Display mode for vital bars
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VitalDisplayMode {
    /// Show full text as-is (e.g., "health 226/226")
    Full,
    /// Show numbers only (e.g., "226/226")
    NumbersOnly,
    /// Show current value only (e.g., "226")
    CurrentOnly,
}

impl Default for VitalDisplayMode {
    fn default() -> Self {
        VitalDisplayMode::Full
    }
}

/// MiniVitals widget - shows 4 horizontal progress bars
pub struct MiniVitals {
    title: String,
    /// Whether to show title
    show_title: bool,
    /// Whether to show border
    show_border: bool,
    /// Which border sides to show
    border_sides: BorderSides,
    /// Cached vitals for rendering
    health_value: u32,
    health_max: u32,
    health_text: String,
    mana_value: u32,
    mana_max: u32,
    mana_text: String,
    stamina_value: u32,
    stamina_max: u32,
    stamina_text: String,
    spirit_value: u32,
    spirit_max: u32,
    spirit_text: String,
    /// Generation counter for change detection
    generation: u64,
    /// Border color
    border_color: Color,
    /// Colors for each vital
    health_color: Color,
    mana_color: Color,
    stamina_color: Color,
    spirit_color: Color,
    /// Text color
    text_color: Color,
    /// Display mode for text
    display_mode: VitalDisplayMode,
    /// Background color (from theme)
    background_color: Option<Color>,
    /// Order of bars (e.g., ["health", "mana", "stamina", "spirit"])
    bar_order: Vec<String>,
}

impl MiniVitals {
    pub fn new(title: &str, show_border: bool) -> Self {
        Self {
            title: title.to_string(),
            show_title: true,
            show_border,
            border_sides: BorderSides::default(),
            health_value: 0,
            health_max: 100,
            health_text: String::new(),
            mana_value: 0,
            mana_max: 100,
            mana_text: String::new(),
            stamina_value: 0,
            stamina_max: 100,
            stamina_text: String::new(),
            spirit_value: 0,
            spirit_max: 100,
            spirit_text: String::new(),
            generation: 0,
            border_color: Color::White,
            health_color: Color::Rgb(110, 2, 2),      // #6e0202
            mana_color: Color::Rgb(8, 8, 109),        // #08086d
            stamina_color: Color::Rgb(189, 123, 0),   // #bd7b00
            spirit_color: Color::Rgb(110, 114, 124),  // #6e727c
            text_color: Color::White,
            display_mode: VitalDisplayMode::Full,
            background_color: None,
            bar_order: vec![
                "health".to_string(),
                "mana".to_string(),
                "stamina".to_string(),
                "spirit".to_string(),
            ],
        }
    }

    /// Set the border color
    pub fn set_border_color(&mut self, color: Color) {
        self.border_color = color;
    }

    /// Set the text color
    pub fn set_text_color(&mut self, color: Color) {
        self.text_color = color;
    }

    /// Set whether to show the border
    pub fn set_show_border(&mut self, show: bool) {
        self.show_border = show;
    }

    /// Set whether to show the title
    pub fn set_show_title(&mut self, show: bool) {
        self.show_title = show;
    }

    /// Set which border sides to show
    pub fn set_border_sides(&mut self, sides: BorderSides) {
        self.border_sides = sides;
    }

    /// Set display mode (full, numbers_only, current_only)
    pub fn set_display_mode(&mut self, numbers_only: bool, current_only: bool) {
        self.display_mode = if current_only {
            VitalDisplayMode::CurrentOnly
        } else if numbers_only {
            VitalDisplayMode::NumbersOnly
        } else {
            VitalDisplayMode::Full
        };
    }

    /// Set the background color (from theme)
    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color.and_then(|c| super::colors::parse_color_to_ratatui(&c));
    }

    /// Set the bar order (e.g., ["health", "mana", "stamina", "spirit"])
    pub fn set_bar_order(&mut self, order: Vec<String>) {
        if !order.is_empty() {
            self.bar_order = order;
        }
    }

    /// Set the health bar color
    pub fn set_health_color(&mut self, color: Color) {
        self.health_color = color;
    }

    /// Set the mana bar color
    pub fn set_mana_color(&mut self, color: Color) {
        self.mana_color = color;
    }

    /// Set the stamina bar color
    pub fn set_stamina_color(&mut self, color: Color) {
        self.stamina_color = color;
    }

    /// Set the spirit bar color
    pub fn set_spirit_color(&mut self, color: Color) {
        self.spirit_color = color;
    }

    /// Get vital data by name (value, max, text, color)
    fn get_vital_by_name(&self, name: &str) -> Option<(u32, u32, &String, Color)> {
        match name {
            "health" => Some((self.health_value, self.health_max, &self.health_text, self.health_color)),
            "mana" | "concentration" => Some((self.mana_value, self.mana_max, &self.mana_text, self.mana_color)),
            "stamina" => Some((self.stamina_value, self.stamina_max, &self.stamina_text, self.stamina_color)),
            "spirit" => Some((self.spirit_value, self.spirit_max, &self.spirit_text, self.spirit_color)),
            _ => None,
        }
    }

    /// Update the widget from MiniVitalsState.
    /// Returns true if the display changed.
    pub fn update_from_state(&mut self, state: &MiniVitalsState) -> bool {
        if self.generation == state.generation {
            return false;
        }

        self.generation = state.generation;

        self.health_value = state.health.value;
        self.health_max = state.health.max;
        self.health_text = state.health.text.clone();

        self.mana_value = state.mana.value;
        self.mana_max = state.mana.max;
        self.mana_text = state.mana.text.clone();

        self.stamina_value = state.stamina.value;
        self.stamina_max = state.stamina.max;
        self.stamina_text = state.stamina.text.clone();

        self.spirit_value = state.spirit.value;
        self.spirit_max = state.spirit.max;
        self.spirit_text = state.spirit.text.clone();

        true
    }

    /// Format the display text based on display mode
    fn format_display_text(&self, value: u32, max: u32, text: &str) -> String {
        match self.display_mode {
            VitalDisplayMode::Full => {
                if text.is_empty() {
                    format!("{}/{}", value, max)
                } else {
                    text.to_string()
                }
            }
            VitalDisplayMode::NumbersOnly => {
                format!("{}/{}", value, max)
            }
            VitalDisplayMode::CurrentOnly => {
                format!("{}", value)
            }
        }
    }

    /// Render a single progress bar segment
    fn render_bar(
        &self,
        area: Rect,
        buf: &mut Buffer,
        value: u32,
        max: u32,
        text: &str,
        fill_color: Color,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let bar_width = area.width as usize;
        let percent = if max > 0 { value * 100 / max } else { 0 };
        let filled_width = (bar_width as u32 * percent / 100) as usize;

        // Get display text based on mode
        let display_text = self.format_display_text(value, max, text);

        // Truncate if needed
        let display_text = if display_text.len() > bar_width {
            display_text[..bar_width].to_string()
        } else {
            display_text
        };

        // Center the text
        let text_start = (bar_width.saturating_sub(display_text.len())) / 2;

        for col in 0..bar_width {
            let x = area.x + col as u16;
            let y = area.y;

            if x >= buf.area().width || y >= buf.area().height {
                continue;
            }

            let is_filled = col < filled_width;

            // Determine character at this position
            let ch = if col >= text_start && col < text_start + display_text.len() {
                display_text.chars().nth(col - text_start).unwrap_or(' ')
            } else {
                ' '
            };

            let (fg, bg) = if is_filled {
                (self.text_color, fill_color)
            } else {
                (self.text_color, Color::DarkGray)
            };

            buf[(x, y)].set_char(ch);
            buf[(x, y)].set_fg(fg);
            buf[(x, y)].set_bg(bg);
        }
    }

    /// Render the minivitals widget
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Apply background color to full area (including borders) before rendering block
        if let Some(bg_color) = self.background_color {
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(bg_color);
                    }
                }
            }
        }

        let inner = if self.show_border && self.border_sides.any() {
            let borders = super::crossterm_bridge::to_ratatui_borders(&self.border_sides);
            let mut block = Block::default()
                .borders(borders)
                .border_style(Style::default().fg(self.border_color));
            if self.show_title {
                block = block.title(self.title.as_str());
            }
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Build vitals list based on bar_order
        let vitals: Vec<(u32, u32, &String, Color)> = self
            .bar_order
            .iter()
            .filter_map(|name| self.get_vital_by_name(name))
            .collect();

        if vitals.is_empty() {
            return;
        }

        // Calculate bar widths - divide evenly with small gaps
        let total_width = inner.width as usize;
        let num_bars = vitals.len();
        let gap = 1; // 1 char gap between bars
        let total_gaps = if num_bars > 1 { (num_bars - 1) * gap } else { 0 };
        let available_width = total_width.saturating_sub(total_gaps);
        let bar_width = available_width / num_bars;
        let remainder = available_width % num_bars;

        if bar_width == 0 {
            return;
        }

        let mut x_offset = inner.x;
        for (idx, (value, max, text, color)) in vitals.iter().enumerate() {
            // Distribute extra columns starting from the last bar backwards
            // e.g., remainder=2 with 4 bars: bars at idx 2,3 get +1
            let extra = if idx >= num_bars - remainder { 1 } else { 0 };
            let this_bar_width = bar_width + extra;

            let bar_area = Rect {
                x: x_offset,
                y: inner.y,
                width: this_bar_width as u16,
                height: 1,
            };

            self.render_bar(bar_area, buf, *value, *max, text, *color);

            x_offset += this_bar_width as u16 + gap as u16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::VitalEntry;

    #[test]
    fn test_new_default() {
        let mv = MiniVitals::new("Stats", true);
        assert_eq!(mv.title, "Stats");
        assert!(mv.show_border);
        assert_eq!(mv.health_value, 0);
        assert_eq!(mv.generation, 0);
    }

    #[test]
    fn test_update_from_state() {
        let mut mv = MiniVitals::new("Stats", true);
        let mut state = MiniVitalsState::default();
        state.health = VitalEntry { value: 226, max: 300, text: "health 226/300".to_string() };
        state.mana = VitalEntry { value: 100, max: 100, text: "mana 100/100".to_string() };
        state.generation = 1;

        let changed = mv.update_from_state(&state);
        assert!(changed);
        assert_eq!(mv.health_value, 226);
        assert_eq!(mv.health_max, 300);
        assert_eq!(mv.health_text, "health 226/300");
        assert_eq!(mv.generation, 1);
    }

    #[test]
    fn test_update_no_change_same_generation() {
        let mut mv = MiniVitals::new("Stats", true);
        let mut state = MiniVitalsState::default();
        state.generation = 1;

        mv.update_from_state(&state);
        let changed = mv.update_from_state(&state);
        assert!(!changed);
    }

    #[test]
    fn test_display_mode_full() {
        let mut mv = MiniVitals::new("Stats", true);
        mv.set_display_mode(false, false);
        assert_eq!(mv.display_mode, VitalDisplayMode::Full);
        assert_eq!(mv.format_display_text(226, 300, "health 226/300"), "health 226/300");
    }

    #[test]
    fn test_display_mode_numbers_only() {
        let mut mv = MiniVitals::new("Stats", true);
        mv.set_display_mode(true, false);
        assert_eq!(mv.display_mode, VitalDisplayMode::NumbersOnly);
        assert_eq!(mv.format_display_text(226, 300, "health 226/300"), "226/300");
    }

    #[test]
    fn test_display_mode_current_only() {
        let mut mv = MiniVitals::new("Stats", true);
        mv.set_display_mode(false, true);
        assert_eq!(mv.display_mode, VitalDisplayMode::CurrentOnly);
        assert_eq!(mv.format_display_text(226, 300, "health 226/300"), "226");
    }

    #[test]
    fn test_set_colors() {
        let mut mv = MiniVitals::new("Stats", true);
        mv.set_border_color(Color::Cyan);
        mv.set_text_color(Color::Green);
        assert_eq!(mv.border_color, Color::Cyan);
        assert_eq!(mv.text_color, Color::Green);
    }

    #[test]
    fn test_default_bar_order() {
        let mv = MiniVitals::new("Stats", true);
        assert_eq!(mv.bar_order, vec!["health", "mana", "stamina", "spirit"]);
    }

    #[test]
    fn test_custom_bar_order() {
        let mut mv = MiniVitals::new("Stats", true);
        mv.set_bar_order(vec![
            "spirit".to_string(),
            "stamina".to_string(),
            "mana".to_string(),
            "health".to_string(),
        ]);
        assert_eq!(mv.bar_order, vec!["spirit", "stamina", "mana", "health"]);
    }

    #[test]
    fn test_bar_order_ignores_empty() {
        let mut mv = MiniVitals::new("Stats", true);
        let original_order = mv.bar_order.clone();
        mv.set_bar_order(vec![]);
        assert_eq!(mv.bar_order, original_order);
    }

    #[test]
    fn test_get_vital_by_name() {
        let mut mv = MiniVitals::new("Stats", true);
        let mut state = MiniVitalsState::default();
        state.health = VitalEntry { value: 100, max: 200, text: "health 100/200".to_string() };
        state.mana = VitalEntry { value: 50, max: 100, text: "mana 50/100".to_string() };
        state.generation = 1;
        mv.update_from_state(&state);

        // Test valid vitals
        assert!(mv.get_vital_by_name("health").is_some());
        assert!(mv.get_vital_by_name("mana").is_some());
        assert!(mv.get_vital_by_name("concentration").is_some()); // Maps to mana
        assert!(mv.get_vital_by_name("stamina").is_some());
        assert!(mv.get_vital_by_name("spirit").is_some());

        // Test invalid vital
        assert!(mv.get_vital_by_name("invalid").is_none());
    }

    #[test]
    fn test_bar_width_distribution() {
        // Test that extra columns are distributed from last bar backwards
        // With 4 bars and 1-char gaps, available = total - 3
        //
        // For 4 bars:
        // - 40 cols available: 10, 10, 10, 10 (remainder 0)
        // - 41 cols available: 10, 10, 10, 11 (remainder 1 -> bar 4 gets +1)
        // - 42 cols available: 10, 10, 11, 11 (remainder 2 -> bars 3,4 get +1)
        // - 43 cols available: 10, 11, 11, 11 (remainder 3 -> bars 2,3,4 get +1)
        // - 44 cols available: 11, 11, 11, 11 (remainder 0)

        let num_bars = 4;

        // Helper to calculate widths for a given available width
        fn calc_widths(available: usize, num_bars: usize) -> Vec<usize> {
            let bar_width = available / num_bars;
            let remainder = available % num_bars;
            (0..num_bars)
                .map(|idx| {
                    let extra = if idx >= num_bars - remainder { 1 } else { 0 };
                    bar_width + extra
                })
                .collect()
        }

        // Test various widths
        assert_eq!(calc_widths(40, num_bars), vec![10, 10, 10, 10]);
        assert_eq!(calc_widths(41, num_bars), vec![10, 10, 10, 11]);
        assert_eq!(calc_widths(42, num_bars), vec![10, 10, 11, 11]);
        assert_eq!(calc_widths(43, num_bars), vec![10, 11, 11, 11]);
        assert_eq!(calc_widths(44, num_bars), vec![11, 11, 11, 11]);
        assert_eq!(calc_widths(45, num_bars), vec![11, 11, 11, 12]);
    }
}
