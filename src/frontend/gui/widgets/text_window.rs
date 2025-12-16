//! Text Window Widget - Styled text display with scrolling
//!
//! Renders TextContent with proper colors, bold, underlines, and auto-scroll.
//! Supports clickable links that trigger game commands.

use crate::data::widget::{LinkData, StyledLine, TextContent, TextSegment};
use eframe::egui::{self, Color32, RichText, ScrollArea, Ui};

/// Result of rendering a text window - captures link interactions
#[derive(Default)]
pub struct TextWindowResponse {
    /// Link that was clicked (left mouse button released)
    pub clicked_link: Option<LinkData>,
    /// Link where Ctrl+drag started
    pub drag_started: Option<LinkData>,
    /// Link currently being hovered (for drag target detection)
    pub hovered_link: Option<LinkData>,
}

/// Parse hex color string to egui Color32
///
/// Supports formats: "#RRGGBB", "#RGB", "RRGGBB", "RGB"
pub fn parse_hex_to_color32(hex: &str) -> Option<Color32> {
    let hex = hex.trim_start_matches('#');

    match hex.len() {
        // Short format: #RGB -> expand to #RRGGBB
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some(Color32::from_rgb(r, g, b))
        }
        // Standard format: #RRGGBB
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        _ => None,
    }
}

/// Convert a TextSegment to egui RichText with full styling
fn segment_to_rich_text(segment: &TextSegment, is_hovered: bool, font_family: Option<&str>) -> RichText {
    let mut text = RichText::new(&segment.text);

    // Apply foreground color
    if let Some(ref fg) = segment.fg {
        if let Some(color) = parse_hex_to_color32(fg) {
            text = text.color(color);
        }
    }

    // Apply background color
    if let Some(ref bg) = segment.bg {
        if let Some(color) = parse_hex_to_color32(bg) {
            text = text.background_color(color);
        }
    }

    // Apply bold
    if segment.bold {
        text = text.strong();
    }

    // Underline links on hover
    if segment.link_data.is_some() && is_hovered {
        text = text.underline();
    }

    // Apply font family (default to monospace for game text)
    match font_family {
        Some("proportional") => {
            // Use proportional font
        },
        Some("monospace") | None => {
            // Use monospace font (default for game text)
            text = text.monospace();
        },
        _ => {
            // Unknown font family, default to monospace
            text = text.monospace();
        }
    }

    text
}

/// Render a segment, making it clickable if it's a link
/// Returns SegmentInteractions with any interactions that occurred
fn render_segment(
    ui: &mut Ui,
    segment: &TextSegment,
    modifiers: &egui::Modifiers,
    font_family: Option<&str>,
) -> SegmentInteractions {
    let mut interactions = SegmentInteractions::default();
    let is_link = segment.link_data.is_some();

    if is_link {
        // Clickable link - use Label with click_and_drag sense for both click and drag detection
        let text = segment_to_rich_text(segment, false, font_family);

        // Use Label with click_and_drag sense to detect both clicks and drags
        // Use selectable(false) to prevent text highlighting during Ctrl+drag
        let response = ui.add(
            egui::Label::new(text)
                .selectable(false)
                .sense(egui::Sense::click_and_drag())
        );

        // Update styling on hover
        if response.hovered() {
            // Change cursor to pointer (or grab icon if Ctrl is held)
            if modifiers.ctrl {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            } else {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }

        if let Some(ref link_data) = segment.link_data {
            // Track hovered link (for drag target detection)
            if response.hovered() {
                interactions.hovered = Some(link_data.clone());
            }

            // Check for Ctrl+drag start
            if response.drag_started() && modifiers.ctrl {
                tracing::info!(
                    "Ctrl+drag started on link: {} (exist_id: {})",
                    link_data.noun,
                    link_data.exist_id
                );
                interactions.drag_started = Some(link_data.clone());
            }

            // Check for click (not Ctrl, and not dragging)
            if response.clicked() && !modifiers.ctrl {
                tracing::info!("Link clicked: {} (exist_id: {})", link_data.noun, link_data.exist_id);
                interactions.clicked = Some(link_data.clone());
            }
        }
    } else {
        // Non-link segment - just display
        ui.label(segment_to_rich_text(segment, false, font_family));
    }

    interactions
}

/// Interactions detected from a segment
#[derive(Default)]
struct SegmentInteractions {
    clicked: Option<LinkData>,
    drag_started: Option<LinkData>,
    hovered: Option<LinkData>,
}

impl SegmentInteractions {
    fn merge(&mut self, other: SegmentInteractions) {
        if other.clicked.is_some() {
            self.clicked = other.clicked;
        }
        if other.drag_started.is_some() {
            self.drag_started = other.drag_started;
        }
        if other.hovered.is_some() {
            self.hovered = other.hovered;
        }
    }
}

/// Format a Unix timestamp as a display string (e.g., " [7:08 AM]")
fn format_timestamp(timestamp: i64, format: Option<&str>) -> String {
    use chrono::{Local, TimeZone};
    let datetime = Local.timestamp_opt(timestamp, 0).single();
    match datetime {
        Some(dt) => {
            let fmt = format.unwrap_or("%l:%M %p");
            format!(" [{}]", dt.format(fmt).to_string().trim())
        }
        None => String::new(),
    }
}

/// Render a styled line as a horizontal layout of segments
/// Returns any link interactions that occurred
fn render_styled_line(
    ui: &mut Ui,
    line: &StyledLine,
    modifiers: &egui::Modifiers,
    font_family: Option<&str>,
    show_timestamps: bool,
    timestamp_color: Option<Color32>,
    timestamp_format: Option<&str>,
) -> SegmentInteractions {
    let mut result = SegmentInteractions::default();

    if line.segments.is_empty() {
        // Empty line - show a blank space to preserve line height
        ui.label("");
        return result;
    }

    // Use horizontal layout for all cases when timestamps might be shown
    let needs_horizontal = line.segments.len() > 1 || (show_timestamps && line.timestamp.is_some());

    if !needs_horizontal {
        // Single segment, no timestamp - optimize
        return render_segment(ui, &line.segments[0], modifiers, font_family);
    }

    // Multiple segments or timestamp - use horizontal layout
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0; // No gaps between segments
        for segment in &line.segments {
            let interactions = render_segment(ui, segment, modifiers, font_family);
            result.merge(interactions);
        }

        // Render timestamp at end of line if enabled and present
        if show_timestamps {
            if let Some(ts) = line.timestamp {
                let ts_text = format_timestamp(ts, timestamp_format);
                if !ts_text.is_empty() {
                    let color = timestamp_color.unwrap_or(Color32::DARK_GRAY);
                    let text = RichText::new(&ts_text).color(color).monospace();
                    ui.label(text);
                }
            }
        }
    });
    result
}

/// Render a text window with styled content and auto-scroll
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `content` - The TextContent data to render
/// * `config` - The TextWidgetData configuration
/// * `_window_name` - Window name (for future use)
/// * `font_family` - Optional font family override ("monospace", "proportional", or None for default monospace)
///
/// # Returns
/// TextWindowResponse with any link interactions
pub fn render_text_window(
    ui: &mut Ui,
    content: &TextContent,
    config: &crate::config::TextWidgetData,
    _window_name: &str,
    font_family: Option<&str>,
) -> TextWindowResponse {
    let mut response = TextWindowResponse::default();

    // Get current modifiers for Ctrl detection
    let modifiers = ui.ctx().input(|i| i.modifiers);

    // Apply padding via ui.add_space
    ui.add_space(config.padding);

    // Configure scroll behavior based on auto_scroll setting
    // IMPORTANT: max_height constrains the ScrollArea to the available window space
    // This prevents the window from growing unbounded with large buffers
    // NOTE: auto_shrink([false, false]) means:
    //   - horizontal: false (don't shrink width)
    //   - vertical: false (don't shrink height - always use max_height)
    // Using [false, true] causes window growth when content exactly fills the space
    // because ScrollArea expands before scroll kicks in (edge case race condition)
    let mut scroll_area = ScrollArea::vertical()
        .auto_shrink([false, false])
        .drag_to_scroll(false)  // Disable click-drag scrolling (preserves wheel/keyboard)
        .max_height(ui.available_height() - config.padding * 2.0); // Reserve space for padding

    if config.auto_scroll {
        scroll_area = scroll_area.stick_to_bottom(true);
    }

    let scroll_output = scroll_area.show(ui, |ui| {
        // Apply spacing between lines AND for wrapped text
        ui.spacing_mut().item_spacing.y = config.line_spacing;

        // CRITICAL: Set text line height to match line_spacing for consistent wrapping
        // This ensures wrapped lines have the same spacing as separate lines
        ui.style_mut().spacing.interact_size.y = config.font_size + config.line_spacing;

        // Apply font size globally for this window
        ui.style_mut().text_styles.get_mut(&egui::TextStyle::Monospace)
            .map(|font_id| font_id.size = config.font_size);

        if content.lines.is_empty() {
            ui.weak("Waiting for data...");
            return;
        }

        // Parse timestamp color once for all lines
        let timestamp_color = config.timestamp_color
            .as_ref()
            .and_then(|c| parse_hex_to_color32(c));
        let timestamp_format = config.timestamp_format.as_deref();

        // Render all lines - viewport culling removed as it was causing scroll bugs
        // (was hardcoded to bottom of buffer, breaking scroll-back and autoscroll)
        for line in content.lines.iter() {
            let interactions = render_styled_line(
                ui,
                line,
                &modifiers,
                font_family,
                config.show_timestamps,
                timestamp_color,
                timestamp_format,
            );

            // Aggregate interactions (last one wins for each type)
            if interactions.clicked.is_some() {
                response.clicked_link = interactions.clicked;
            }
            if interactions.drag_started.is_some() {
                response.drag_started = interactions.drag_started;
            }
            if interactions.hovered.is_some() {
                response.hovered_link = interactions.hovered;
            }
        }
    });

    // Apply padding after scroll area as well
    ui.add_space(config.padding);

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_colors() {
        // Standard 6-digit hex
        assert_eq!(
            parse_hex_to_color32("#FF0000"),
            Some(Color32::from_rgb(255, 0, 0))
        );
        assert_eq!(
            parse_hex_to_color32("#00FF00"),
            Some(Color32::from_rgb(0, 255, 0))
        );
        assert_eq!(
            parse_hex_to_color32("#0000FF"),
            Some(Color32::from_rgb(0, 0, 255))
        );

        // Without hash
        assert_eq!(
            parse_hex_to_color32("FFFFFF"),
            Some(Color32::from_rgb(255, 255, 255))
        );

        // Short format
        assert_eq!(
            parse_hex_to_color32("#F00"),
            Some(Color32::from_rgb(255, 0, 0))
        );
        assert_eq!(
            parse_hex_to_color32("#0F0"),
            Some(Color32::from_rgb(0, 255, 0))
        );

        // Invalid
        assert_eq!(parse_hex_to_color32("invalid"), None);
        assert_eq!(parse_hex_to_color32("#GG0000"), None);
    }
}
