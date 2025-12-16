//! Tabbed Text Window Widget - Multi-tab text display with stream routing
//!
//! A container widget that holds multiple text windows with tab-based navigation.
//! Each tab can subscribe to different streams (e.g., "thoughts", "combat", "loot").
//! Supports unread indicators, tab switching, and full styling customization.

use crate::config::TabbedTextWidgetData;
use crate::data::widget::{LinkData, TabbedTextContent, TextContent};
use eframe::egui::{self, Color32, RichText, Ui};
use std::collections::HashMap;

/// Response from rendering a tabbed text window
#[derive(Default)]
pub struct TabbedTextWindowResponse {
    /// Link that was clicked (left mouse button released)
    pub clicked_link: Option<LinkData>,
    /// Link where Ctrl+drag started
    pub drag_started: Option<LinkData>,
    /// Link currently being hovered (for drag target detection)
    pub hovered_link: Option<LinkData>,
    /// Tab that was clicked (index)
    pub tab_clicked: Option<usize>,
}

/// GUI-specific state for a tabbed text window
/// This is stored per-window in EguiApp to track unread counts etc.
#[derive(Clone, Debug)]
pub struct GuiTabbedTextState {
    /// Unread message counts per tab (by tab index)
    pub unread_counts: HashMap<usize, usize>,
    /// Last known generation per tab (to detect new content)
    pub last_generations: HashMap<usize, u64>,
}

impl Default for GuiTabbedTextState {
    fn default() -> Self {
        Self {
            unread_counts: HashMap::new(),
            last_generations: HashMap::new(),
        }
    }
}

impl GuiTabbedTextState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update unread counts based on content changes
    /// Call this each frame before rendering
    pub fn update_unread(&mut self, content: &TabbedTextContent) {
        for (idx, tab) in content.tabs.iter().enumerate() {
            let current_gen = tab.content.generation;
            let last_gen = self.last_generations.get(&idx).copied().unwrap_or(0);

            // If content changed and not the active tab, increment unread
            if current_gen != last_gen {
                self.last_generations.insert(idx, current_gen);

                // Only count as unread if not currently viewing this tab
                // and the tab doesn't ignore activity
                if idx != content.active_tab_index && !tab.definition.ignore_activity {
                    let count = self.unread_counts.entry(idx).or_insert(0);
                    *count = count.saturating_add(1);
                }
            }
        }
    }

    /// Clear unread count for a tab (called when switching to it)
    pub fn clear_unread(&mut self, tab_index: usize) {
        self.unread_counts.remove(&tab_index);
    }

    /// Get unread count for a tab
    pub fn get_unread(&self, tab_index: usize) -> usize {
        self.unread_counts.get(&tab_index).copied().unwrap_or(0)
    }

    /// Check if any tab has unread messages
    pub fn has_any_unread(&self) -> bool {
        self.unread_counts.values().any(|&c| c > 0)
    }
}

/// Parse hex color string to egui Color32
fn parse_hex_to_color32(hex: &str) -> Option<Color32> {
    let hex = hex.trim_start_matches('#');

    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some(Color32::from_rgb(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        _ => None,
    }
}

/// Render the tab bar and return which tab was clicked (if any)
/// Uses content.tabs for tab definitions (runtime state)
fn render_tab_bar(
    ui: &mut Ui,
    content: &TabbedTextContent,
    config: &TabbedTextWidgetData,
    gui_state: &GuiTabbedTextState,
) -> Option<usize> {
    let mut clicked_tab: Option<usize> = None;
    tracing::trace!("render_tab_bar: {} tabs, active_index={}", content.tabs.len(), content.active_tab_index);

    // Parse tab colors from config
    let active_color = config
        .tab_active_color
        .as_ref()
        .and_then(|c| parse_hex_to_color32(c))
        .unwrap_or(Color32::YELLOW);

    let inactive_color = config
        .tab_inactive_color
        .as_ref()
        .and_then(|c| parse_hex_to_color32(c))
        .unwrap_or(Color32::GRAY);

    let unread_color = config
        .tab_unread_color
        .as_ref()
        .and_then(|c| parse_hex_to_color32(c))
        .unwrap_or(Color32::WHITE);

    let unread_prefix = config
        .tab_unread_prefix
        .as_deref()
        .unwrap_or("* ");

    let tab_text_size = config.tab_text_size;
    let tab_padding = config.tab_padding;
    let tab_rounding = config.tab_rounding;

    // Use content.tabs for rendering (runtime state)
    let tab_count = content.tabs.len();

    // Horizontal layout for tabs
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0; // Space between tabs
        tracing::trace!("Tab bar horizontal layout, rendering {} tabs", tab_count);

        for idx in 0..tab_count {
            let is_active = idx == content.active_tab_index;
            let unread_count = gui_state.get_unread(idx);

            // Get tab from content (runtime state)
            let tab = &content.tabs[idx];
            let tab_name = tab.definition.name.as_str();
            let ignore_activity = tab.definition.ignore_activity;

            let has_unread = unread_count > 0 && !ignore_activity;

            // Build tab label
            let label = if has_unread {
                format!("{}{}", unread_prefix, tab_name)
            } else {
                tab_name.to_string()
            };

            // Choose color based on state
            let text_color = if is_active {
                active_color
            } else if has_unread {
                unread_color
            } else {
                inactive_color
            };

            // Build styled text
            let mut text = RichText::new(&label)
                .size(tab_text_size)
                .color(text_color);

            if is_active {
                text = text.strong();
            }

            // Create tab button with custom styling
            // All tabs have transparent background - text color indicates active state
            let button = egui::Button::new(text)
                .corner_radius(tab_rounding)
                .min_size(egui::vec2(0.0, config.tab_bar_height - tab_padding * 2.0))
                .fill(Color32::TRANSPARENT)
                .sense(egui::Sense::click());  // Explicitly enable click detection

            let response = ui.add(button);
            if response.hovered() {
                tracing::debug!("Tab {} hovered", idx);
            }
            if response.clicked() {
                tracing::info!("Tab button {} clicked!", idx);
                clicked_tab = Some(idx);
            }

            // Add separator between tabs (except after last) - only if enabled
            if config.tab_separator && idx < tab_count - 1 {
                ui.separator();
            }
        }
    });

    if clicked_tab.is_some() {
        tracing::info!("render_tab_bar returning clicked_tab={:?}", clicked_tab);
    }
    clicked_tab
}

/// Render the content of the active tab
fn render_tab_content(
    ui: &mut Ui,
    content: &TextContent,
    config: &TabbedTextWidgetData,
    show_timestamps: bool,
    font_family: Option<&str>,
) -> super::TextWindowResponse {
    // Reuse the text window rendering logic
    // Create a compatible TextWidgetData from TabbedTextWidgetData
    let text_config = crate::config::TextWidgetData {
        streams: vec![], // Not used during rendering
        buffer_size: config.buffer_size,
        wordwrap: true,
        show_timestamps, // Per-tab setting from TabDefinition
        font_size: config.content_font_size,
        line_spacing: 2.0,
        padding: 4.0,
        text_color: None,
        link_color: None,
        link_underline_on_hover: true,
        auto_scroll: true,
        timestamp_color: None,
        timestamp_format: None,
    };

    super::render_text_window(ui, content, &text_config, "", font_family)
}

/// Render a tabbed text window with full styling
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `content` - The TabbedTextContent data from the data layer
/// * `config` - The TabbedTextWidgetData configuration
/// * `gui_state` - Mutable GUI-specific state (unread counts, etc.)
/// * `font_family` - Optional font family override
///
/// # Returns
/// TabbedTextWindowResponse with link interactions and tab clicks
pub fn render_tabbed_text_window(
    ui: &mut Ui,
    content: &TabbedTextContent,
    config: &TabbedTextWidgetData,
    gui_state: &mut GuiTabbedTextState,
    font_family: Option<&str>,
) -> TabbedTextWindowResponse {
    let mut response = TabbedTextWindowResponse::default();

    tracing::trace!("render_tabbed_text_window called: {} tabs, active={}",
        content.tabs.len(), content.active_tab_index);

    // Check for empty tabs in content (runtime state)
    if content.tabs.is_empty() {
        ui.weak("No tabs configured");
        tracing::warn!("render_tabbed_text_window: no tabs configured!");
        return response;
    }

    // Update unread counts based on content changes
    gui_state.update_unread(content);

    // Determine tab bar position
    let tab_bar_at_top = config.tab_bar_position.to_lowercase() != "bottom";

    // Render tab bar (top position)
    if tab_bar_at_top {
        if let Some(clicked) = render_tab_bar(ui, content, config, gui_state) {
            tracing::info!("Tab bar click detected at top: tab {}", clicked);
            response.tab_clicked = Some(clicked);
        }
    }

    // Render active tab content in remaining space
    // The content area fills all available vertical space after tab bar
    if let Some(active_tab) = content.tabs.get(content.active_tab_index) {
        let show_timestamps = active_tab.definition.show_timestamps;
        let text_response = render_tab_content(ui, &active_tab.content, config, show_timestamps, font_family);

        // Forward link interactions
        response.clicked_link = text_response.clicked_link;
        response.drag_started = text_response.drag_started;
        response.hovered_link = text_response.hovered_link;
    } else {
        // No matching content tab - show placeholder
        ui.weak("Tab content loading...");
    }

    // Render tab bar (bottom position)
    if !tab_bar_at_top {
        if let Some(clicked) = render_tab_bar(ui, content, config, gui_state) {
            response.tab_clicked = Some(clicked);
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::TabDefinition;

    #[test]
    fn test_gui_tabbed_text_state_default() {
        let state = GuiTabbedTextState::default();
        assert!(state.unread_counts.is_empty());
        assert!(state.last_generations.is_empty());
        assert!(!state.has_any_unread());
    }

    #[test]
    fn test_unread_tracking() {
        let mut state = GuiTabbedTextState::new();

        // Simulate unread on tab 1
        state.unread_counts.insert(1, 5);

        assert_eq!(state.get_unread(0), 0);
        assert_eq!(state.get_unread(1), 5);
        assert!(state.has_any_unread());

        // Clear unread
        state.clear_unread(1);
        assert_eq!(state.get_unread(1), 0);
        assert!(!state.has_any_unread());
    }

    #[test]
    fn test_parse_hex_colors() {
        assert_eq!(
            parse_hex_to_color32("#FF0000"),
            Some(Color32::from_rgb(255, 0, 0))
        );
        assert_eq!(
            parse_hex_to_color32("#F00"),
            Some(Color32::from_rgb(255, 0, 0))
        );
        assert_eq!(parse_hex_to_color32("invalid"), None);
    }
}
