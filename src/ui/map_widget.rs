use crate::map_data::{MapCoordinate, MapConnector, MapContext, MapData, MapDb};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};
use std::collections::HashMap;

pub struct MapWidget {
    /// Current room ID we're centered on
    current_room_id: Option<String>,
    /// Current context we're displaying
    current_context_id: Option<String>,
    /// Map data (contexts and coordinates)
    map_data: MapData,
    /// Full mapdb for exit checking
    mapdb: Option<MapDb>,
    /// Border configuration
    show_border: bool,
    title: String,
    /// Styling
    current_room_color: Color,
    visited_room_color: Color,
    unvisited_room_color: Color,
    portal_color: Color,
    /// Track visited rooms
    visited_rooms: Vec<String>,
}

impl MapWidget {
    pub fn new(title: impl Into<String>) -> Self {
        let map_data = MapData::load_default().unwrap_or_else(|e| {
            tracing::error!("Failed to load map data: {}", e);
            // Return empty map data as fallback
            MapData {
                contexts: HashMap::new(),
            }
        });

        let mapdb = match MapDb::load_default() {
            Ok(db) => Some(db),
            Err(e) => {
                tracing::error!("Failed to load mapdb: {} - portal detection will be disabled", e);
                None
            }
        };

        Self {
            current_room_id: None,
            current_context_id: None,
            map_data,
            mapdb,
            show_border: true,
            title: title.into(),
            current_room_color: Color::Yellow,
            visited_room_color: Color::White,
            unvisited_room_color: Color::DarkGray,
            portal_color: Color::Cyan,
            visited_rooms: Vec::new(),
        }
    }

    pub fn set_current_room(&mut self, room_id: String) {
        // Check if we're changing contexts
        if let Some(new_context) = self.map_data.get_context_for_room(&room_id) {
            if self.current_context_id.as_ref() != Some(&new_context) {
                tracing::debug!("Map context changed to: {}", new_context);
                self.current_context_id = Some(new_context);
            }
        }

        // Add to visited rooms
        if !self.visited_rooms.contains(&room_id) {
            self.visited_rooms.push(room_id.clone());
        }

        self.current_room_id = Some(room_id);
    }

    pub fn set_border(&mut self, show: bool) {
        self.show_border = show;
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// Render the map for the current context
    fn render_map(&self, area: Rect, buf: &mut Buffer) {
        let Some(context_id) = &self.current_context_id else {
            // No context - show "No map data"
            let line = Line::from("No map data available");
            let y = area.y + area.height / 2;
            let x = area.x + (area.width.saturating_sub(line.width() as u16)) / 2;
            buf.set_line(x, y, &line, area.width);
            return;
        };

        let Some(context) = self.map_data.get_context(context_id) else {
            return;
        };

        let Some(current_room_id) = &self.current_room_id else {
            return;
        };

        let Some(center_coord) = context.rooms.get(current_room_id) else {
            return;
        };

        // Calculate viewport bounds (5 units in each direction)
        let viewport_range = 5;

        // Grid spacing (columns, rows per cell)
        let grid_spacing_x = 3;
        let grid_spacing_y = 2;

        // First pass: Draw connectors (underneath everything)
        self.draw_connectors(
            buf,
            area,
            &context.connectors,
            center_coord,
            viewport_range,
            grid_spacing_x,
            grid_spacing_y,
        );

        // Second pass: Draw rooms and portals
        for (room_id, coord) in &context.rooms {
            let rel_x = coord.x - center_coord.x;
            let rel_y = coord.y - center_coord.y;
            let rel_z = coord.z - center_coord.z;

            if rel_z != 0 || rel_x.abs() > viewport_range || rel_y.abs() > viewport_range {
                continue;
            }

            let screen_x = (area.x + area.width / 2) as i32 + (rel_x * grid_spacing_x);
            let screen_y = (area.y + area.height / 2) as i32 + (rel_y * grid_spacing_y);

            // Check bounds
            if screen_x < area.x as i32
                || screen_x >= (area.x + area.width) as i32
                || screen_y < area.y as i32
                || screen_y >= (area.y + area.height) as i32
            {
                continue;
            }

            // Skip current room for now (draw it last)
            if room_id == current_room_id {
                continue;
            }

            // Determine room symbol and color
            let (symbol, color) = if self.visited_rooms.contains(room_id) {
                ("○", self.visited_room_color)
            } else {
                ("·", self.unvisited_room_color)
            };

            // Draw room
            let style = Style::default().fg(color);
            buf.set_string(screen_x as u16, screen_y as u16, symbol, style);
        }

        // Third pass: Draw current room LAST (so it's always on top)
        if let Some(coord) = context.rooms.get(current_room_id) {
            let rel_x = coord.x - center_coord.x;
            let rel_y = coord.y - center_coord.y;
            let rel_z = coord.z - center_coord.z;

            if rel_z == 0 && rel_x.abs() <= viewport_range && rel_y.abs() <= viewport_range {
                let screen_x = (area.x + area.width / 2) as i32 + (rel_x * grid_spacing_x);
                let screen_y = (area.y + area.height / 2) as i32 + (rel_y * grid_spacing_y);

                if screen_x >= area.x as i32
                    && screen_x < (area.x + area.width) as i32
                    && screen_y >= area.y as i32
                    && screen_y < (area.y + area.height) as i32
                {
                    let style = Style::default().fg(self.current_room_color);
                    buf.set_string(screen_x as u16, screen_y as u16, "●", style);
                }
            }
        }

        // Draw legend at bottom
        self.draw_legend(buf, area, context);
    }

    fn draw_connectors(
        &self,
        buf: &mut Buffer,
        area: Rect,
        connectors: &[MapConnector],
        center_coord: &MapCoordinate,
        viewport_range: i32,
        grid_spacing_x: i32,
        grid_spacing_y: i32,
    ) {
        let style = Style::default().fg(Color::DarkGray);

        for connector in connectors {
            let rel_x = connector.x - center_coord.x;
            let rel_y = connector.y - center_coord.y;

            // Check if connector is in viewport
            if rel_x.abs() > viewport_range || rel_y.abs() > viewport_range {
                continue;
            }

            let screen_x = (area.x + area.width / 2) as i32 + (rel_x * grid_spacing_x);
            let screen_y = (area.y + area.height / 2) as i32 + (rel_y * grid_spacing_y);

            // Check bounds
            if screen_x < area.x as i32
                || screen_x >= (area.x + area.width) as i32
                || screen_y < area.y as i32
                || screen_y >= (area.y + area.height) as i32
            {
                continue;
            }

            // Map connector type to character
            let symbol = match connector.connector_type.as_str() {
                "vertical" => "│",
                "horizontal" => "─",
                "diagonal-ne" => "/",
                "diagonal-nw" => "\\",
                "cross" => "┼",
                "x" => "✕",
                _ => "?",
            };

            buf.set_string(screen_x as u16, screen_y as u16, symbol, style);
        }
    }

    fn draw_portals(
        &self,
        buf: &mut Buffer,
        area: Rect,
        room_id: &str,
        coord: &MapCoordinate,
        center_coord: &MapCoordinate,
        context: &MapContext,
        grid_spacing_x: i32,
        grid_spacing_y: i32,
    ) {
        let Some(mapdb) = &self.mapdb else { return };

        // Get portal exits for this room
        let portals = mapdb.get_portal_exits(room_id, &context.rooms);
        if portals.is_empty() {
            return;
        }

        let rel_x = coord.x - center_coord.x;
        let rel_y = coord.y - center_coord.y;

        let screen_x = (area.x + area.width / 2) as i32 + (rel_x * grid_spacing_x);
        let screen_y = (area.y + area.height / 2) as i32 + (rel_y * grid_spacing_y);

        // Draw portal markers based on direction
        for (direction, _dest_uid) in portals {
            let dir_lower = direction.to_lowercase();

            // Determine portal position offset from room
            let (offset_x, offset_y, symbol) = if dir_lower.contains("north") && dir_lower.contains("west") {
                (-2, -1, "◉") // Northwest
            } else if dir_lower.contains("north") && dir_lower.contains("east") {
                (2, -1, "◉") // Northeast
            } else if dir_lower.contains("south") && dir_lower.contains("west") {
                (-2, 1, "◉") // Southwest
            } else if dir_lower.contains("south") && dir_lower.contains("east") {
                (2, 1, "◉") // Southeast
            } else if dir_lower.contains("north") || dir_lower == "n" {
                (0, -1, "◉") // North
            } else if dir_lower.contains("south") || dir_lower == "s" {
                (0, 1, "◉") // South
            } else if dir_lower.contains("east") || dir_lower == "e" {
                (2, 0, "◉") // East
            } else if dir_lower.contains("west") || dir_lower == "w" {
                (-2, 0, "◉") // West
            } else if direction.starts_with("go ") || direction.starts_with("climb ") {
                // For "go door", "climb stairs", etc., show a generic portal marker
                (0, 0, "◐") // Show next to room
            } else {
                continue; // Skip other directions
            };

            let portal_x = screen_x + offset_x;
            let portal_y = screen_y + offset_y;

            // Check bounds
            if portal_x >= area.x as i32
                && portal_x < (area.x + area.width) as i32
                && portal_y >= area.y as i32
                && portal_y < (area.y + area.height) as i32
            {
                let style = Style::default().fg(self.portal_color);
                buf.set_string(portal_x as u16, portal_y as u16, symbol, style);
            }
        }
    }

    fn draw_legend(&self, buf: &mut Buffer, area: Rect, context: &MapContext) {
        // Draw context name at top
        let context_line = Line::from(vec![
            Span::raw("Map: "),
            Span::styled(
                &context.display_name,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let y = area.y;
        buf.set_line(area.x, y, &context_line, area.width);

        // Draw legend at bottom
        let legend_y = area.y + area.height - 1;
        if legend_y < area.y + area.height {
            let legend = Line::from(vec![
                Span::styled("●", Style::default().fg(self.current_room_color)),
                Span::raw(" You  "),
                Span::styled("○", Style::default().fg(self.visited_room_color)),
                Span::raw(" Visited  "),
                Span::styled("·", Style::default().fg(self.unvisited_room_color)),
                Span::raw(" Unvisited"),
            ]);
            buf.set_line(area.x, legend_y, &legend, area.width);
        }
    }
}

impl Widget for &MapWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Draw border if enabled
        let inner_area = if self.show_border {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(self.title.clone());
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        // Render the map
        self.render_map(inner_area, buf);
    }
}
