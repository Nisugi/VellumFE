//! Room Window Widget - Displays room information with component visibility
//!
//! Renders room description, objects, players, and exits with proper styling.
//! Supports toggling visibility of individual components.

use crate::data::widget::{RoomContent, StyledLine, TextSegment, LinkData};
use eframe::egui::{self, Color32, RichText, ScrollArea, Ui};
use std::collections::HashMap;

use super::text_window::parse_hex_to_color32;

/// Room component IDs
pub const COMPONENT_DESC: &str = "room desc";
pub const COMPONENT_OBJS: &str = "room objs";
pub const COMPONENT_PLAYERS: &str = "room players";
pub const COMPONENT_EXITS: &str = "room exits";

/// Result of rendering a room window - captures link interactions
#[derive(Default)]
pub struct RoomWindowResponse {
    /// Link that was clicked (left mouse button released)
    pub clicked_link: Option<LinkData>,
    /// Link where Ctrl+drag started
    pub drag_started: Option<LinkData>,
    /// Link currently being hovered (for drag target detection)
    pub hovered_link: Option<LinkData>,
}

/// Component visibility state for the room window
#[derive(Clone, Debug)]
pub struct RoomComponentVisibility {
    pub show_desc: bool,
    pub show_objs: bool,
    pub show_players: bool,
    pub show_exits: bool,
}

impl Default for RoomComponentVisibility {
    fn default() -> Self {
        Self {
            show_desc: true,
            show_objs: true,
            show_players: true,
            show_exits: true,
        }
    }
}

impl RoomComponentVisibility {
    pub fn from_hashmap(map: &HashMap<String, bool>) -> Self {
        Self {
            show_desc: map.get(COMPONENT_DESC).copied().unwrap_or(true),
            show_objs: map.get(COMPONENT_OBJS).copied().unwrap_or(true),
            show_players: map.get(COMPONENT_PLAYERS).copied().unwrap_or(true),
            show_exits: map.get(COMPONENT_EXITS).copied().unwrap_or(true),
        }
    }

    pub fn to_hashmap(&self) -> HashMap<String, bool> {
        let mut map = HashMap::new();
        map.insert(COMPONENT_DESC.to_string(), self.show_desc);
        map.insert(COMPONENT_OBJS.to_string(), self.show_objs);
        map.insert(COMPONENT_PLAYERS.to_string(), self.show_players);
        map.insert(COMPONENT_EXITS.to_string(), self.show_exits);
        map
    }
}

/// Convert a TextSegment to egui RichText with full styling
fn segment_to_rich_text(segment: &TextSegment, is_hovered: bool) -> RichText {
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

    // Use monospace font for game text
    text = text.monospace();

    text
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

/// Render a segment, making it clickable if it's a link
fn render_segment(
    ui: &mut Ui,
    segment: &TextSegment,
    modifiers: &egui::Modifiers,
) -> SegmentInteractions {
    let mut interactions = SegmentInteractions::default();
    let is_link = segment.link_data.is_some();

    if is_link {
        let text = segment_to_rich_text(segment, false);

        let response = ui.add(
            egui::Label::new(text)
                .selectable(false)
                .sense(egui::Sense::click_and_drag()),
        );

        if response.hovered() {
            if modifiers.ctrl {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            } else {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }

        if let Some(ref link_data) = segment.link_data {
            if response.hovered() {
                interactions.hovered = Some(link_data.clone());
            }

            if response.drag_started() && modifiers.ctrl {
                interactions.drag_started = Some(link_data.clone());
            }

            if response.clicked() && !modifiers.ctrl {
                interactions.clicked = Some(link_data.clone());
            }
        }
    } else {
        ui.label(segment_to_rich_text(segment, false));
    }

    interactions
}

/// Render a styled line as a horizontal layout of segments
fn render_styled_line(
    ui: &mut Ui,
    line: &StyledLine,
    modifiers: &egui::Modifiers,
) -> SegmentInteractions {
    let mut result = SegmentInteractions::default();

    if line.segments.is_empty() {
        ui.label("");
        return result;
    }

    if line.segments.len() == 1 {
        return render_segment(ui, &line.segments[0], modifiers);
    }

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for segment in &line.segments {
            let interactions = render_segment(ui, segment, modifiers);
            result.merge(interactions);
        }
    });
    result
}

/// Render plain text with optional label prefix (for exits, etc.)
fn render_plain_list(ui: &mut Ui, label: &str, items: &[String], color: Color32) {
    if items.is_empty() {
        return;
    }

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        // Label
        if !label.is_empty() {
            ui.label(RichText::new(label).color(color).monospace());
        }

        // Items joined with ", "
        let joined = items.join(", ");
        ui.label(RichText::new(joined).monospace());
    });
}

/// Render a room window with component visibility control
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `content` - The RoomContent data to render
/// * `visibility` - Which components to show
/// * `_window_name` - Window name (for future use)
///
/// # Returns
/// RoomWindowResponse with any link interactions
pub fn render_room_window(
    ui: &mut Ui,
    content: &RoomContent,
    visibility: &RoomComponentVisibility,
    _window_name: &str,
) -> RoomWindowResponse {
    let mut response = RoomWindowResponse::default();

    let modifiers = ui.ctx().input(|i| i.modifiers);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(false) // Room content doesn't auto-scroll like chat
        .show(ui, |ui| {
            let has_content = !content.description.is_empty()
                || !content.objects.is_empty()
                || !content.players.is_empty()
                || !content.exits.is_empty();

            if !has_content {
                ui.weak("Waiting for room data...");
                return;
            }

            // Description + Objects (combined on same logical block)
            // Following TUI pattern: desc and objs flow together
            if visibility.show_desc || visibility.show_objs {
                let mut has_desc_content = false;

                // Render description lines
                if visibility.show_desc {
                    for line in &content.description {
                        let interactions = render_styled_line(ui, line, &modifiers);
                        if interactions.clicked.is_some() {
                            response.clicked_link = interactions.clicked;
                        }
                        if interactions.drag_started.is_some() {
                            response.drag_started = interactions.drag_started;
                        }
                        if interactions.hovered.is_some() {
                            response.hovered_link = interactions.hovered;
                        }
                        has_desc_content = true;
                    }
                }

                // Render objects (typically "Also here: item1, item2")
                if visibility.show_objs && !content.objects.is_empty() {
                    // Add spacing if we had description
                    if has_desc_content {
                        ui.add_space(2.0);
                    }
                    render_plain_list(
                        ui,
                        "Also here: ",
                        &content.objects,
                        Color32::from_rgb(200, 200, 200),
                    );
                }
            }

            // Players (on own line)
            if visibility.show_players && !content.players.is_empty() {
                ui.add_space(4.0);
                render_plain_list(
                    ui,
                    "Also in the room: ",
                    &content.players,
                    Color32::from_rgb(180, 180, 255), // Slight blue tint for players
                );
            }

            // Exits (on own line)
            if visibility.show_exits && !content.exits.is_empty() {
                ui.add_space(4.0);
                render_plain_list(
                    ui,
                    "Obvious exits: ",
                    &content.exits,
                    Color32::from_rgb(180, 255, 180), // Slight green tint for exits
                );
            }
        });

    response
}

/// Render visibility toggle checkboxes for room components
/// Returns true if any visibility was changed
pub fn render_visibility_controls(
    ui: &mut Ui,
    visibility: &mut RoomComponentVisibility,
) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut visibility.show_desc, "Description")
            .changed()
        {
            changed = true;
        }
        if ui.checkbox(&mut visibility.show_objs, "Objects").changed() {
            changed = true;
        }
        if ui
            .checkbox(&mut visibility.show_players, "Players")
            .changed()
        {
            changed = true;
        }
        if ui.checkbox(&mut visibility.show_exits, "Exits").changed() {
            changed = true;
        }
    });

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_component_visibility_default() {
        let vis = RoomComponentVisibility::default();
        assert!(vis.show_desc);
        assert!(vis.show_objs);
        assert!(vis.show_players);
        assert!(vis.show_exits);
    }

    #[test]
    fn test_room_component_visibility_from_hashmap() {
        let mut map = HashMap::new();
        map.insert(COMPONENT_DESC.to_string(), false);
        map.insert(COMPONENT_OBJS.to_string(), true);
        map.insert(COMPONENT_PLAYERS.to_string(), false);
        map.insert(COMPONENT_EXITS.to_string(), true);

        let vis = RoomComponentVisibility::from_hashmap(&map);
        assert!(!vis.show_desc);
        assert!(vis.show_objs);
        assert!(!vis.show_players);
        assert!(vis.show_exits);
    }

    #[test]
    fn test_room_component_visibility_to_hashmap() {
        let vis = RoomComponentVisibility {
            show_desc: true,
            show_objs: false,
            show_players: true,
            show_exits: false,
        };

        let map = vis.to_hashmap();
        assert_eq!(map.get(COMPONENT_DESC), Some(&true));
        assert_eq!(map.get(COMPONENT_OBJS), Some(&false));
        assert_eq!(map.get(COMPONENT_PLAYERS), Some(&true));
        assert_eq!(map.get(COMPONENT_EXITS), Some(&false));
    }
}
