//! GUI Window Manager - Tracks window state for egui frontend
//!
//! This module manages GUI-specific window state (pixel positions, sizes, z-order)
//! separate from the TUI-oriented WindowState in data/window.rs.

use std::collections::HashMap;

/// GUI-specific window state (positions in pixels, not character cells)
#[derive(Clone, Debug)]
pub struct GuiWindowState {
    /// Position in pixels [x, y]
    pub position: [f32; 2],
    /// Size in pixels [width, height]
    pub size: [f32; 2],
    /// Whether the window is currently visible
    pub visible: bool,
    /// Z-order for layering (higher = on top)
    pub z_order: u32,
    /// Whether the window is collapsed
    pub collapsed: bool,
    /// Whether to show the title bar (needed for dragging)
    pub show_title_bar: bool,
    /// One-frame position override (used after title bar toggle for bottom-anchored windows)
    /// This forces egui to reposition the window, then clears automatically
    pub position_override: Option<[f32; 2]>,
}

impl Default for GuiWindowState {
    fn default() -> Self {
        Self {
            position: [100.0, 100.0],
            size: [400.0, 300.0],
            visible: true,
            z_order: 0,
            collapsed: false,
            show_title_bar: true,
            position_override: None,
        }
    }
}

/// Window Manager - coordinates all GUI windows
pub struct WindowManager {
    /// Per-window GUI state
    pub windows: HashMap<String, GuiWindowState>,
    /// Next z-order value (incremented when window gains focus)
    next_z_order: u32,
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            next_z_order: 1,
        }
    }

    /// Get or create window state for a named window
    pub fn get_or_create(&mut self, name: &str) -> &mut GuiWindowState {
        if !self.windows.contains_key(name) {
            let state = self.create_default_state(name);
            self.windows.insert(name.to_string(), state);
        }
        self.windows.get_mut(name).unwrap()
    }

    /// Create default state for a window based on its name/type
    fn create_default_state(&self, name: &str) -> GuiWindowState {
        // Default positions based on common window names
        // These provide reasonable starting positions before layout is loaded
        match name {
            "main" | "story" => GuiWindowState {
                position: [20.0, 20.0],
                size: [900.0, 500.0],
                visible: true,
                z_order: 0,
                collapsed: false,
                show_title_bar: true,
                position_override: None,
            },
            "command" | "input" => GuiWindowState {
                position: [20.0, 540.0],
                size: [900.0, 60.0],
                visible: true,
                z_order: 1,
                collapsed: false,
                show_title_bar: true,
                position_override: None,
            },
            "vitals" | "health" => GuiWindowState {
                position: [940.0, 20.0],
                size: [300.0, 120.0],
                visible: true,
                z_order: 0,
                collapsed: false,
                show_title_bar: true,
                position_override: None,
            },
            "compass" => GuiWindowState {
                position: [940.0, 160.0],
                size: [120.0, 120.0],
                visible: true,
                z_order: 0,
                collapsed: false,
                show_title_bar: true,
                position_override: None,
            },
            "roundtime" | "rt" => GuiWindowState {
                position: [940.0, 300.0],
                size: [300.0, 40.0],
                visible: true,
                z_order: 0,
                collapsed: false,
                show_title_bar: true,
                position_override: None,
            },
            _ => GuiWindowState::default(),
        }
    }

    /// Toggle title bar visibility for a window
    /// If parent_height is provided, adjusts position for bottom-anchored windows
    pub fn toggle_title_bar(&mut self, name: &str) {
        if let Some(state) = self.windows.get_mut(name) {
            state.show_title_bar = !state.show_title_bar;
        }
    }

    /// Toggle title bar visibility with position adjustment for bottom-anchored windows
    ///
    /// When a window is at the bottom border and the title bar is hidden, the window
    /// would normally shift up, leaving a gap at the bottom. This method detects
    /// bottom-anchored windows and adjusts their position to keep them anchored.
    ///
    /// # Arguments
    /// * `name` - Window name
    /// * `current_pos` - The window's CURRENT position from egui (not our stored position)
    /// * `current_size` - The window's CURRENT size from egui
    /// * `parent_height` - Height of the parent/available area
    /// * `title_bar_height` - Height of the title bar
    pub fn toggle_title_bar_with_anchor(
        &mut self,
        name: &str,
        current_pos: [f32; 2],
        current_size: [f32; 2],
        parent_height: f32,
        title_bar_height: f32,
    ) {
        if let Some(state) = self.windows.get_mut(name) {
            // Use the actual current position from egui, not our stored position
            let window_bottom = current_pos[1] + current_size[1];
            // Consider "at bottom" if within 10 pixels of parent bottom
            let is_bottom_anchored = (window_bottom - parent_height).abs() < 10.0;

            let was_showing = state.show_title_bar;
            state.show_title_bar = !state.show_title_bar;

            // Adjust position for bottom-anchored windows using position_override
            if is_bottom_anchored {
                let mut new_y = current_pos[1];
                if was_showing {
                    // Hiding title bar - window content shifts up, so move window down
                    // to keep bottom edge at the same place
                    new_y += title_bar_height;
                } else {
                    // Showing title bar - window content shifts down, so move window up
                    // to keep bottom edge at the same place
                    new_y -= title_bar_height;
                    // Clamp to 0 to prevent going off-screen
                    if new_y < 0.0 {
                        new_y = 0.0;
                    }
                }
                // Set position override - this will be used for one frame then cleared
                state.position_override = Some([current_pos[0], new_y]);
            }
        }
    }

    /// Clear position override after it's been applied
    pub fn clear_position_override(&mut self, name: &str) {
        if let Some(state) = self.windows.get_mut(name) {
            state.position_override = None;
        }
    }

    /// Set position override to force window to specific position on next frame
    /// Also updates the stored position so egui remembers it
    pub fn set_position_override(&mut self, name: &str, pos: [f32; 2]) {
        if let Some(state) = self.windows.get_mut(name) {
            state.position_override = Some(pos);
            state.position = pos;
        }
    }

    /// Bring a window to front (update z-order)
    pub fn bring_to_front(&mut self, name: &str) {
        if let Some(state) = self.windows.get_mut(name) {
            state.z_order = self.next_z_order;
            self.next_z_order += 1;
        }
    }

    /// Update position from egui response
    pub fn update_position(&mut self, name: &str, pos: [f32; 2]) {
        if let Some(state) = self.windows.get_mut(name) {
            state.position = pos;
        }
    }

    /// Update size from egui response
    pub fn update_size(&mut self, name: &str, size: [f32; 2]) {
        if let Some(state) = self.windows.get_mut(name) {
            state.size = size;
        }
    }

    /// Toggle window visibility
    pub fn toggle_visibility(&mut self, name: &str) {
        if let Some(state) = self.windows.get_mut(name) {
            state.visible = !state.visible;
        }
    }

    /// Set window visibility
    pub fn set_visibility(&mut self, name: &str, visible: bool) {
        if let Some(state) = self.windows.get_mut(name) {
            state.visible = visible;
        }
    }

    /// Initialize windows from TUI layout positions
    /// Converts character cell positions to approximate pixel positions
    pub fn init_from_layout(&mut self, windows: &[(String, u16, u16, u16, u16)]) {
        // Approximate character cell to pixel conversion
        // Assuming ~8px per character width, ~16px per character height
        const CHAR_WIDTH: f32 = 8.0;
        const CHAR_HEIGHT: f32 = 18.0;

        for (name, col, row, cols, rows) in windows {
            let state = GuiWindowState {
                position: [*col as f32 * CHAR_WIDTH, *row as f32 * CHAR_HEIGHT],
                size: [*cols as f32 * CHAR_WIDTH, *rows as f32 * CHAR_HEIGHT],
                visible: true,
                z_order: 0,
                collapsed: false,
                show_title_bar: true,
                position_override: None,
            };
            self.windows.insert(name.clone(), state);
        }
    }

    /// Get windows sorted by z-order (for rendering back to front)
    pub fn windows_by_z_order(&self) -> Vec<(&String, &GuiWindowState)> {
        let mut windows: Vec<_> = self.windows.iter().collect();
        windows.sort_by_key(|(_, state)| state.z_order);
        windows
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}
