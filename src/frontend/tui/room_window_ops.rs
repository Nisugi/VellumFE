use super::*;

impl TuiFrontend {
    pub fn ensure_room_window_exists(
        &mut self,
        window_name: &str,
        window_def: &crate::config::WindowDef,
    ) {
        if !self.widget_manager.room_windows.contains_key(window_name) {
            let mut room_window = room_window::RoomWindow::new("Room".to_string());

            // Configure RoomWindow with settings from WindowDef
            if let crate::config::WindowDef::Room { data, .. } = window_def {
                // Set component visibility from config
                room_window.set_component_visible("room desc", data.show_desc);
                room_window.set_component_visible("room objs", data.show_objs);
                room_window.set_component_visible("room players", data.show_players);
                room_window.set_component_visible("room exits", data.show_exits);
                room_window.set_show_name(data.show_name);
            }

            self.widget_manager.room_windows
                .insert(window_name.to_string(), room_window);
            tracing::debug!("Created RoomWindow widget for '{}'", window_name);
        }
    }

    /// Clear all components in a room window (called when pushStream id="room")
    pub fn room_window_clear_components(&mut self, window_name: &str) {
        if let Some(room_window) = self.widget_manager.room_windows.get_mut(window_name) {
            room_window.clear_all_components();
            tracing::debug!("Cleared all components for room window '{}'", window_name);
        }
    }

    /// Start building a room component
    pub fn room_window_start_component(&mut self, window_name: &str, component_id: String) {
        if let Some(room_window) = self.widget_manager.room_windows.get_mut(window_name) {
            room_window.start_component(component_id);
        }
    }

    /// Add a segment to the current component in a room window
    pub fn room_window_add_segment(
        &mut self,
        window_name: &str,
        segment: crate::data::widget::TextSegment,
    ) {
        if let Some(room_window) = self.widget_manager.room_windows.get_mut(window_name) {
            room_window.add_segment(segment);
        }
    }

    /// Finish the current line in a room component
    pub fn room_window_finish_line(&mut self, window_name: &str) {
        if let Some(room_window) = self.widget_manager.room_windows.get_mut(window_name) {
            room_window.finish_line();
        }
    }

    /// Finish building the current component in a room window
    pub fn room_window_finish_component(&mut self, window_name: &str) {
        if let Some(room_window) = self.widget_manager.room_windows.get_mut(window_name) {
            room_window.finish_component();
        }
    }

    /// Set the title of a room window
    pub fn room_window_set_title(&mut self, window_name: &str, title: String) {
        if let Some(room_window) = self.widget_manager.room_windows.get_mut(window_name) {
            room_window.set_title(title);
        }
    }

    /// Find a link at a given mouse position in a text or room window
    pub fn link_at_position(
        &self,
        window_name: &str,
        mouse_col: u16,
        mouse_row: u16,
        window_rect: ratatui::layout::Rect,
    ) -> Option<crate::data::LinkData> {
        // Try text window first
        if let Some(text_window) = self.widget_manager.text_windows.get(window_name) {
            let border_offset = if text_window.has_border() { 1 } else { 0 };

            // Bounds check within content area
            if mouse_col < window_rect.x + border_offset
                || mouse_col >= window_rect.x + window_rect.width - border_offset
                || mouse_row < window_rect.y + border_offset
                || mouse_row >= window_rect.y + window_rect.height - border_offset
            {
                return None;
            }

            let visible_height = (window_rect.height.saturating_sub(2 * border_offset)) as usize;
            let (_start_idx, visible_lines) = text_window.get_visible_lines_info(visible_height);

            let line_idx = (mouse_row - window_rect.y - border_offset) as usize;
            let col_offset = (mouse_col - window_rect.x - border_offset) as usize;

            if line_idx >= visible_lines.len() {
                return None;
            }

            let line = &visible_lines[line_idx];
            let mut col = 0usize;
            for seg in &line.segments {
                let seg_len = seg.text.chars().count();
                if col_offset >= col && col_offset < col + seg_len {
                    // Inside this segment
                    if let Some(link) = seg.link_data.clone() {
                        // Convert from TextWindow's LinkData to data layer's LinkData
                        let mut data_link = crate::data::LinkData {
                            exist_id: link.exist_id,
                            noun: link.noun,
                            text: link.text,
                            coord: link.coord,
                        };
                        // For <d> tags without cmd attribute, populate text from segment
                        if data_link.text.is_empty() {
                            data_link.text = seg.text.clone();
                        }
                        return Some(data_link);
                    }
                    return None;
                }
                col += seg_len;
            }

            return None;
        }

        // Try room window
        if let Some(room_window) = self.widget_manager.room_windows.get(window_name) {
            tracing::debug!(
                "Checking room window '{}' for link at ({}, {})",
                window_name,
                mouse_col,
                mouse_row
            );
            let border_offset = 1u16; // Room windows always have borders

            // Bounds check within content area
            if mouse_col < window_rect.x + border_offset
                || mouse_col >= window_rect.x + window_rect.width - border_offset
                || mouse_row < window_rect.y + border_offset
                || mouse_row >= window_rect.y + window_rect.height - border_offset
            {
                tracing::debug!("Mouse click outside room window content area");
                return None;
            }

            let wrapped_lines = room_window.get_wrapped_lines();
            let start_line = room_window.get_start_line(); // Get scroll offset
            tracing::debug!(
                "Room window has {} wrapped lines, start_line={}",
                wrapped_lines.len(),
                start_line
            );

            // Map visual row to actual wrapped line index (accounting for scroll/overflow)
            let visual_line_idx = (mouse_row - window_rect.y - border_offset) as usize;
            let line_idx = start_line + visual_line_idx;
            let col_offset = (mouse_col - window_rect.x - border_offset) as usize;

            if line_idx >= wrapped_lines.len() {
                tracing::debug!(
                    "Line index {} (visual={}, start={}) out of range",
                    line_idx,
                    visual_line_idx,
                    start_line
                );
                return None;
            }

            let line = &wrapped_lines[line_idx];
            tracing::debug!(
                "Checking line {} with {} segments, col_offset={}",
                line_idx,
                line.len(),
                col_offset
            );
            let mut col = 0usize;
            for (seg_idx, seg) in line.iter().enumerate() {
                let seg_len = seg.text.chars().count();
                tracing::debug!(
                    "  Segment {}: text='{}', col={}, len={}, has_link={}",
                    seg_idx,
                    seg.text,
                    col,
                    seg_len,
                    seg.link_data.is_some()
                );

                if col_offset >= col && col_offset < col + seg_len {
                    // Inside this segment
                    tracing::debug!("  Click is inside this segment!");
                    if let Some(link) = seg.link_data.clone() {
                        tracing::debug!(
                            "  Found link: exist_id={}, noun={}",
                            link.exist_id,
                            link.noun
                        );
                        let mut data_link = crate::data::LinkData {
                            exist_id: link.exist_id.clone(),
                            noun: link.noun.clone(),
                            text: link.text.clone(),
                            coord: link.coord.clone(),
                        };
                        // For <d> tags without cmd attribute, populate text from segment
                        if data_link.text.is_empty() {
                            data_link.text = seg.text.clone();
                        }
                        return Some(data_link);
                    }
                    tracing::debug!("  Segment has no link data");
                    return None;
                }
                col += seg_len;
            }

            tracing::debug!("No segment matched at col_offset={}", col_offset);
            return None;
        }

        // Try inventory window
        if let Some(inventory_window) = self.widget_manager.inventory_windows.get(window_name) {
            tracing::debug!(
                "Checking inventory window '{}' for link at ({}, {})",
                window_name,
                mouse_col,
                mouse_row
            );
            let border_offset = 1u16; // Inventory windows always have borders

            // Bounds check within content area
            if mouse_col < window_rect.x + border_offset
                || mouse_col >= window_rect.x + window_rect.width - border_offset
                || mouse_row < window_rect.y + border_offset
                || mouse_row >= window_rect.y + window_rect.height - border_offset
            {
                tracing::debug!("Mouse click outside inventory window content area");
                return None;
            }

            let wrapped_lines = inventory_window.get_wrapped_lines();
            let start_line = inventory_window.get_start_line(); // Get scroll offset
            tracing::debug!(
                "Inventory window has {} wrapped lines, start_line={}",
                wrapped_lines.len(),
                start_line
            );

            // Map visual row to actual line index (accounting for scroll/overflow)
            let visual_line_idx = (mouse_row - window_rect.y - border_offset) as usize;
            let line_idx = start_line + visual_line_idx;
            let col_offset = (mouse_col - window_rect.x - border_offset) as usize;

            if line_idx >= wrapped_lines.len() {
                tracing::debug!(
                    "Line index {} (visual={}, start={}) out of range",
                    line_idx,
                    visual_line_idx,
                    start_line
                );
                return None;
            }

            let line = &wrapped_lines[line_idx];
            tracing::debug!(
                "Checking line {} with {} segments, col_offset={}",
                line_idx,
                line.len(),
                col_offset
            );
            let mut col = 0usize;
            for (seg_idx, seg) in line.iter().enumerate() {
                let seg_len = seg.text.chars().count();
                tracing::debug!(
                    "  Segment {}: text='{}', col={}, len={}, has_link={}",
                    seg_idx,
                    seg.text,
                    col,
                    seg_len,
                    seg.link_data.is_some()
                );

                if col_offset >= col && col_offset < col + seg_len {
                    // Inside this segment
                    tracing::debug!("  Click is inside this segment!");
                    if let Some(link) = seg.link_data.clone() {
                        tracing::debug!(
                            "  Found link: exist_id={}, noun={}",
                            link.exist_id,
                            link.noun
                        );
                        let data_link = crate::data::LinkData {
                            exist_id: link.exist_id.clone(),
                            noun: link.noun.clone(),
                            text: link.text.clone(),
                            coord: link.coord.clone(),
                        };
                        return Some(data_link);
                    }
                    tracing::debug!("  Segment has no link data");
                    return None;
                }
                col += seg_len;
            }

            tracing::debug!("No segment matched at col_offset={}", col_offset);
            return None;
        }

        // Try hand widget
        if let Some(hand_widget) = self.widget_manager.hand_widgets.get(window_name) {
            if let Some(link) = hand_widget.link_data() {
                let border_offset = if hand_widget.has_border() { 1 } else { 0 };
                if mouse_col >= window_rect.x + border_offset
                    && mouse_col < window_rect.x + window_rect.width - border_offset
                    && mouse_row >= window_rect.y + border_offset
                    && mouse_row < window_rect.y + window_rect.height - border_offset
                {
                    return Some(link);
                }
            }
        }

        // Try container widget
        if let Some(container_widget) = self.widget_manager.container_widgets.get(window_name) {
            tracing::debug!(
                "Checking container window '{}' for link at ({}, {})",
                window_name,
                mouse_col,
                mouse_row
            );
            let border_offset = if container_widget.has_border() { 1 } else { 0 };

            // Bounds check within content area
            if mouse_col < window_rect.x + border_offset
                || mouse_col >= window_rect.x + window_rect.width - border_offset
                || mouse_row < window_rect.y + border_offset
                || mouse_row >= window_rect.y + window_rect.height - border_offset
            {
                tracing::debug!("Mouse click outside container window content area");
                return None;
            }

            let wrapped_lines = container_widget.get_wrapped_lines();
            let start_line = container_widget.get_start_line(); // Get scroll offset
            tracing::debug!(
                "Container window has {} lines, start_line={}",
                wrapped_lines.len(),
                start_line
            );

            // Map visual row to actual line index (accounting for scroll/overflow)
            let visual_line_idx = (mouse_row - window_rect.y - border_offset) as usize;
            let line_idx = start_line + visual_line_idx;
            let col_offset = (mouse_col - window_rect.x - border_offset) as usize;

            if line_idx >= wrapped_lines.len() {
                tracing::debug!(
                    "Line index {} (visual={}, start={}) out of range",
                    line_idx,
                    visual_line_idx,
                    start_line
                );
                return None;
            }

            let line = &wrapped_lines[line_idx];
            tracing::debug!(
                "Checking line {} with {} segments, col_offset={}",
                line_idx,
                line.len(),
                col_offset
            );
            let mut col = 0usize;
            for (seg_idx, seg) in line.iter().enumerate() {
                let seg_len = seg.text.chars().count();
                tracing::debug!(
                    "  Segment {}: text='{}', col={}, len={}, has_link={}",
                    seg_idx,
                    seg.text,
                    col,
                    seg_len,
                    seg.link_data.is_some()
                );

                if col_offset >= col && col_offset < col + seg_len {
                    // Inside this segment
                    tracing::debug!("  Click is inside this segment!");
                    if let Some(link) = seg.link_data.clone() {
                        tracing::debug!(
                            "  Found link: exist_id={}, noun={}",
                            link.exist_id,
                            link.noun
                        );
                        let data_link = crate::data::LinkData {
                            exist_id: link.exist_id.clone(),
                            noun: link.noun.clone(),
                            text: link.text.clone(),
                            coord: link.coord.clone(),
                        };
                        return Some(data_link);
                    }
                    tracing::debug!("  Segment has no link data");
                    return None;
                }
                col += seg_len;
            }

            tracing::debug!("No segment matched at col_offset={}", col_offset);
            return None;
        }

        // Try spells window
        if let Some(spells_window) = self.widget_manager.spells_windows.get(window_name) {
            tracing::debug!(
                "Checking spells window '{}' for link at ({}, {})",
                window_name,
                mouse_col,
                mouse_row
            );

            // Delegate to spells_window's handle_click method
            if let Some(link) = spells_window.handle_click(mouse_col, mouse_row, window_rect) {
                tracing::debug!(
                    "Found spell link: exist_id={}, noun={}, coord={:?}",
                    link.exist_id,
                    link.noun,
                    link.coord
                );
                return Some(link);
            }
            tracing::debug!("No spell link found at click position");
            return None;
        }

        // Try targets widget (component-based, for direct targeting)
        if let Some(targets_widget) = self.widget_manager.targets_widgets.get(window_name) {
            // handle_click returns "target #id" command if creature clicked
            if let Some(command) = targets_widget.handle_click(mouse_row, window_rect) {
                // Return as direct command link (like <d> tags)
                return Some(crate::data::LinkData {
                    exist_id: "_direct_".to_string(),
                    noun: command, // "target #123456"
                    text: String::new(),
                    coord: None,
                });
            }
        }

        None
    }
}
