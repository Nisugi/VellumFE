use super::*;
use crate::config::Config;

/// Find the next available palette slot for a new color.
/// Searches slots 16-231 (color cube range) for the first unused slot.
/// Returns None if all slots are taken.
fn find_next_available_slot(palette: &[crate::config::PaletteColor]) -> Option<u8> {
    let used_slots: std::collections::HashSet<u8> = palette
        .iter()
        .filter_map(|c| c.slot)
        .collect();

    // Search in color cube range (16-231), avoiding ANSI colors (0-15) and grayscale (232-255)
    (16u8..=231).find(|slot| !used_slots.contains(slot))
}

/// Assign the next available slot to a color if it doesn't have one
fn auto_assign_slot(mut color: crate::config::PaletteColor, palette: &[crate::config::PaletteColor]) -> crate::config::PaletteColor {
    if color.slot.is_none() {
        color.slot = find_next_available_slot(palette);
        if let Some(slot) = color.slot {
            tracing::info!("Auto-assigned slot {} to new color '{}'", slot, color.name);
        }
    }
    color
}

// TUI-specific methods (not part of Frontend trait)
impl TuiFrontend {
    /// Handle mouse events (extracted from main.rs Phase 4.1)
    /// Returns (handled, optional_command)
    pub fn handle_mouse_event(
        &mut self,
        mouse_event: &crate::frontend::MouseEvent,
        app_core: &mut crate::core::AppCore,
        handle_menu_action_fn: impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<(bool, Option<String>)> {
        use crate::data::ui_state::InputMode;
        use crate::frontend::MouseEventKind;
        use crate::data::{DragOperation, LinkDragState, MouseDragState, PendingLinkClick, window::WidgetType};
        use ratatui::layout::Rect;

        let kind = &mouse_event.kind;
        let x = &mouse_event.column;
        let y = &mouse_event.row;
        let modifiers = &mouse_event.modifiers;

        // Create stable window index mapping (sorted by window name for consistency)
        let mut window_names: Vec<&String> = app_core.ui_state.windows.keys().collect();
        window_names.sort();
        let window_index_map: std::collections::HashMap<&String, usize> = window_names
            .iter()
            .enumerate()
            .map(|(idx, name)| (*name, idx))
            .collect();

        // Handle window editor mouse events first (if open)
        if self.window_editor.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut window_editor) = self.window_editor {
                match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        window_editor.handle_mouse(*x, *y, true, area);
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        window_editor.handle_mouse(*x, *y, true, area);
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        window_editor.handle_mouse(*x, *y, false, area);
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                    _ => {}
                }
            }
        }

        // Handle highlight form mouse events (draggable popup)
        if self.highlight_form.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut form) = self.highlight_form {
                match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, true, area);
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, true, area);
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, false, area);
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                    _ => {}
                }
            }
        }

        // Handle highlight browser mouse events (draggable popup)
        if self.highlight_browser.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut browser) = self.highlight_browser {
                match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, true, area);
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, true, area);
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, false, area);
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                    _ => {}
                }
            }
        }

        match kind {
            MouseEventKind::ScrollUp => {
                // Find which window the mouse is over
                let mut target_window = "main".to_string();
                for (name, window) in &app_core.ui_state.windows {
                    let pos = &window.position;
                    if *x >= pos.x
                        && *x < pos.x + pos.width
                        && *y >= pos.y
                        && *y < pos.y + pos.height
                    {
                        target_window = name.clone();
                        break;
                    }
                }
                self.scroll_window(&target_window, 10);
                app_core.needs_render = true;
                return Ok((true, None));
            }
            MouseEventKind::ScrollDown => {
                // Find which window the mouse is over
                let mut target_window = "main".to_string();
                for (name, window) in &app_core.ui_state.windows {
                    let pos = &window.position;
                    if *x >= pos.x
                        && *x < pos.x + pos.width
                        && *y >= pos.y
                        && *y < pos.y + pos.height
                    {
                        target_window = name.clone();
                        break;
                    }
                }
                self.scroll_window(&target_window, -10);
                app_core.needs_render = true;
                return Ok((true, None));
            }
            MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                // If in menu mode, handle menu clicks first
                if app_core.ui_state.input_mode == InputMode::Menu {
                    let mut clicked_item = None;

                    // Check popup menu first (top layer)
                    if let Some(ref menu) = app_core.ui_state.popup_menu {
                        let pos = menu.get_position();
                        let menu_height = menu.get_items().len() as u16 + 2; // +2 for borders
                        let menu_width = menu
                            .get_items()
                            .iter()
                            .map(|item| item.text.len())
                            .max()
                            .unwrap_or(10)
                            as u16
                            + 4; // +4 for borders and padding

                        let menu_area = (pos.0, pos.1, menu_width, menu_height);

                        if let Some(index) = menu.check_click(*x, *y, menu_area) {
                            clicked_item = menu.get_items().get(index).cloned();
                        }
                    }

                    if let Some(item) = clicked_item {
                        let command = item.command.clone();
                        tracing::info!(
                            "Menu item clicked: {} (command: {})",
                            item.text,
                            command
                        );

                        // Handle command same way as Enter key
                        if let Some(submenu_name) = command.strip_prefix("menu:") {
                            // Config menu submenu
                            tracing::debug!("Clicked config submenu: {}", submenu_name);
                            app_core.ui_state.popup_menu = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        } else if let Some(category) = command.strip_prefix("__SUBMENU__") {
                            // Context menu or .menu submenu
                            // Try build_submenu first (for .menu categories)
                            let items = app_core.build_submenu(category);
                            let items = if !items.is_empty() {
                                items
                            } else if let Some(items) = app_core.menu_categories.get(category) {
                                items.clone()
                            } else {
                                Vec::new()
                            };

                            if !items.is_empty() {
                                let position = app_core
                                    .ui_state
                                    .popup_menu
                                    .as_ref()
                                    .map(|m| m.get_position())
                                    .unwrap_or((40, 12));
                                let submenu_pos = (position.0 + 2, position.1);
                                app_core.ui_state.submenu =
                                    Some(crate::data::ui_state::PopupMenu::new(
                                        items,
                                        submenu_pos,
                                    ));
                                tracing::info!(
                                    "Opened submenu: {}",
                                    category
                                );
                            }
                        } else if !command.is_empty() {
                            // Close menu first
                            app_core.ui_state.popup_menu = None;
                            app_core.ui_state.submenu = None;
                            app_core.ui_state.nested_submenu = None;
                            app_core.ui_state.input_mode = InputMode::Normal;

                            // Check if this is an internal action or game command
                            if command.starts_with("action:") {
                                // Internal action - handle it
                                if let Err(e) = handle_menu_action_fn(app_core, self, &command) {
                                    tracing::error!("Menu action error: {}", e);
                                }
                                app_core.needs_render = true;
                                return Ok((true, None));
                            } else {
                                // Game command - return it for sending to server
                                app_core.needs_render = true;
                                return Ok((true, Some(format!("{}\n", command))));
                            }
                        }
                        app_core.needs_render = true;
                    } else {
                        // Click outside menu - close it
                        app_core.ui_state.popup_menu = None;
                        app_core.ui_state.submenu = None;
                        app_core.ui_state.nested_submenu = None;
                        app_core.ui_state.input_mode = InputMode::Normal;
                        app_core.needs_render = true;
                    }

                    // Don't process other clicks while in menu mode
                    return Ok((true, None));
                }

                // Mouse down handling (find links, start drags)
                app_core.ui_state.selection_state = None;

                let mut found_window = None;
                let mut drag_op = None;
                let mut clicked_window_name: Option<String> = None;
                let mut handled_tab_click: Option<(String, usize)> = None;

                for (name, window) in &app_core.ui_state.windows {
                    let pos = &window.position;
                    if *x >= pos.x
                        && *x < pos.x + pos.width
                        && *y >= pos.y
                        && *y < pos.y + pos.height
                    {
                        clicked_window_name = Some(name.clone());

                        // Handle tabbed text tab switching on click
                        if window.widget_type == WidgetType::TabbedText {
                            let rect = Rect {
                                x: pos.x,
                                y: pos.y,
                                width: pos.width,
                                height: pos.height,
                            };
                            if let Some(new_index) =
                                self.handle_tabbed_click(name, rect, *x, *y)
                            {
                                handled_tab_click = Some((name.clone(), new_index));
                                break;
                            }
                        }

                        let right_col = pos.x + pos.width - 1;
                        let bottom_row = pos.y + pos.height - 1;
                        let has_horizontal_space = pos.width > 1;
                        let has_vertical_space = pos.height > 1;

                        if has_horizontal_space
                            && has_vertical_space
                            && *x == right_col
                            && *y == bottom_row
                        {
                            drag_op = Some(DragOperation::ResizeBottomRight);
                            found_window = Some(name.clone());
                            break;
                        } else if has_horizontal_space && *x == right_col {
                            drag_op = Some(DragOperation::ResizeRight);
                            found_window = Some(name.clone());
                            break;
                        } else if has_vertical_space && *y == bottom_row {
                            drag_op = Some(DragOperation::ResizeBottom);
                            found_window = Some(name.clone());
                            break;
                        } else if *y == pos.y {
                            drag_op = Some(DragOperation::Move);
                            found_window = Some(name.clone());
                            break;
                        }
                    }
                }

                if let Some((win_name, new_index)) = handled_tab_click {
                    if let Some(window_state) = app_core.ui_state.get_window_mut(&win_name) {
                        if let crate::data::WindowContent::TabbedText(tabbed) =
                            &mut window_state.content
                        {
                            if new_index < tabbed.tabs.len() {
                                tabbed.active_tab_index = new_index;
                            }
                        }
                    }
                    app_core.needs_render = true;
                    return Ok((true, None));
                }

                if let (Some(window_name), Some(operation)) = (found_window, drag_op) {
                    if let Some(window) = app_core.ui_state.get_window(&window_name) {
                        let pos = &window.position;
                        app_core.ui_state.mouse_drag = Some(MouseDragState {
                            operation,
                            window_name,
                            start_pos: (*x, *y),
                            original_window_pos: (pos.x, pos.y, pos.width, pos.height),
                        });
                    }
                } else if let Some(window_name) = clicked_window_name {
                    if let Some(window) = app_core.ui_state.get_window(&window_name) {
                        let pos = &window.position;
                        let window_rect = ratatui::layout::Rect {
                            x: pos.x,
                            y: pos.y,
                            width: pos.width,
                            height: pos.height,
                        };

                        if let Some(link_data) =
                            self.link_at_position(&window_name, *x, *y, window_rect)
                        {
                            let has_ctrl = modifiers.ctrl;

                            if has_ctrl {
                                app_core.ui_state.link_drag_state =
                                    Some(LinkDragState {
                                        link_data,
                                        start_pos: (*x, *y),
                                        current_pos: (*x, *y),
                                    });
                            } else {
                                app_core.ui_state.pending_link_click =
                                    Some(PendingLinkClick {
                                        link_data,
                                        click_pos: (*x, *y),
                                    });
                            }
                        } else {
                            // Start text selection
                            app_core.ui_state.selection_drag_start = Some((*x, *y));

                            // Convert mouse coords to text coords for selection
                            if let Some((line, col)) = self.mouse_to_text_coords(
                                &window_name,
                                *x,
                                *y,
                                window_rect,
                            ) {
                                // Find window index from the stable mapping
                                let window_index = window_index_map
                                    .get(&window_name)
                                    .copied()
                                    .unwrap_or(0);
                                app_core.ui_state.selection_state =
                                    Some(crate::selection::SelectionState::new(
                                        window_index,
                                        line,
                                        col,
                                    ));
                            }
                        }
                    }
                }
                return Ok((true, None));
            }
            MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                if let Some(ref mut link_drag) = app_core.ui_state.link_drag_state {
                    link_drag.current_pos = (*x, *y);
                    app_core.needs_render = true;
                } else if let Some(drag_state) = app_core.ui_state.mouse_drag.clone() {
                    let dx = *x as i32 - drag_state.start_pos.0 as i32;
                    let dy = *y as i32 - drag_state.start_pos.1 as i32;

                    // Get terminal size for clamping windows within bounds
                    let (term_width, term_height) = self.size();

                    let (min_width_constraint, min_height_constraint) =
                        app_core.window_min_size(&drag_state.window_name);

                    if let Some(window) =
                        app_core.ui_state.get_window_mut(&drag_state.window_name)
                    {
                        let min_width_i32 = min_width_constraint as i32;
                        let min_height_i32 = min_height_constraint as i32;

                        match drag_state.operation {
                            DragOperation::Move => {
                                // Calculate new position
                                let new_x = (drag_state.original_window_pos.0 as i32
                                    + dx)
                                    .max(0)
                                    as u16;
                                let new_y = (drag_state.original_window_pos.1 as i32
                                    + dy)
                                    .max(0)
                                    as u16;

                                // Clamp to prevent overflow beyond terminal boundaries
                                let max_x =
                                    term_width.saturating_sub(window.position.width);
                                let max_y =
                                    term_height.saturating_sub(window.position.height);

                                window.position.x = new_x.min(max_x);
                                window.position.y = new_y.min(max_y);
                            }
                            DragOperation::ResizeRight => {
                                // Calculate new width
                                let new_width =
                                    (drag_state.original_window_pos.2 as i32 + dx)
                                        .max(min_width_i32)
                                        as u16;

                                // Clamp to prevent overflow beyond terminal edge
                                let max_width =
                                    term_width.saturating_sub(window.position.x);
                                window.position.width = new_width.min(max_width);
                            }
                            DragOperation::ResizeBottom => {
                                // Calculate new height
                                let new_height =
                                    (drag_state.original_window_pos.3 as i32 + dy)
                                        .max(min_height_i32)
                                        as u16;

                                // Clamp to prevent overflow beyond terminal edge
                                let max_height =
                                    term_height.saturating_sub(window.position.y);
                                window.position.height = new_height.min(max_height);
                            }
                            DragOperation::ResizeBottomRight => {
                                // Calculate new dimensions
                                let new_width =
                                    (drag_state.original_window_pos.2 as i32 + dx)
                                        .max(min_width_i32)
                                        as u16;
                                let new_height =
                                    (drag_state.original_window_pos.3 as i32 + dy)
                                        .max(min_height_i32)
                                        as u16;

                                // Clamp to prevent overflow beyond terminal edges
                                let max_width =
                                    term_width.saturating_sub(window.position.x);
                                let max_height =
                                    term_height.saturating_sub(window.position.y);

                                window.position.width = new_width.min(max_width);
                                window.position.height = new_height.min(max_height);
                            }
                        }
                        app_core.needs_render = true;
                    }
                } else if app_core.ui_state.pending_link_click.is_some() {
                    app_core.ui_state.pending_link_click = None;
                } else if let Some(_drag_start) = app_core.ui_state.selection_drag_start
                {
                    // Update text selection on drag
                    if let Some(ref mut selection) = app_core.ui_state.selection_state {
                        // Find which window we're dragging in
                        for (name, window) in &app_core.ui_state.windows {
                            let pos = &window.position;
                            if *x >= pos.x
                                && *x < pos.x + pos.width
                                && *y >= pos.y
                                && *y < pos.y + pos.height
                            {
                                let window_rect = ratatui::layout::Rect {
                                    x: pos.x,
                                    y: pos.y,
                                    width: pos.width,
                                    height: pos.height,
                                };
                                if let Some((line, col)) = self
                                    .mouse_to_text_coords(name, *x, *y, window_rect)
                                {
                                    let window_index = window_index_map
                                        .get(name)
                                        .copied()
                                        .unwrap_or(0);
                                    selection.update_end(window_index, line, col);
                                    app_core.needs_render = true;
                                }
                                break;
                            }
                        }
                    }
                }
                return Ok((true, None));
            }
            MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                let mut command_to_send: Option<String> = None;

                if let Some(link_drag) = app_core.ui_state.link_drag_state.take() {
                    let dx = (*x as i16 - link_drag.start_pos.0 as i16).abs();
                    let dy = (*y as i16 - link_drag.start_pos.1 as i16).abs();

                    if dx > 2 || dy > 2 {
                        let mut drop_target_hand: Option<String> = None;
                        let mut drop_target_id: Option<String> = None;

                        for (name, window) in &app_core.ui_state.windows {
                            let pos = &window.position;
                            if *x >= pos.x
                                && *x < pos.x + pos.width
                                && *y >= pos.y
                                && *y < pos.y + pos.height
                            {
                                // First check if this is a hand widget (left or right only)
                                if name == "left_hand" || name == "left" {
                                    drop_target_hand = Some("left".to_string());
                                    break;
                                } else if name == "right_hand" || name == "right" {
                                    drop_target_hand = Some("right".to_string());
                                    break;
                                }

                                // Otherwise check if we dropped on a link
                                let window_rect = ratatui::layout::Rect {
                                    x: pos.x,
                                    y: pos.y,
                                    width: pos.width,
                                    height: pos.height,
                                };
                                if let Some(target_link) =
                                    self.link_at_position(name, *x, *y, window_rect)
                                {
                                    drop_target_id = Some(target_link.exist_id);
                                    break;
                                }
                            }
                        }

                        let command = if let Some(hand_type) = drop_target_hand {
                            format!(
                                "_drag #{} {}\n",
                                link_drag.link_data.exist_id, hand_type
                            )
                        } else if let Some(target_id) = drop_target_id {
                            format!(
                                "_drag #{} #{}\n",
                                link_drag.link_data.exist_id, target_id
                            )
                        } else {
                            format!("_drag #{} drop\n", link_drag.link_data.exist_id)
                        };
                        command_to_send = Some(command);
                    }
                } else if let Some(pending_click) =
                    app_core.ui_state.pending_link_click.take()
                {
                    let dx = (*x as i16 - pending_click.click_pos.0 as i16).abs();
                    let dy = (*y as i16 - pending_click.click_pos.1 as i16).abs();

                    if dx <= 2 && dy <= 2 {
                        // Handle <d> tags differently (direct commands vs context menus)
                        if pending_click.link_data.exist_id == "_direct_" {
                            // <d> tag: Send text/noun as direct command
                            let command = if !pending_click.link_data.noun.is_empty() {
                                format!("{}\n", pending_click.link_data.noun)
                            // Use cmd attribute
                            } else {
                                format!("{}\n", pending_click.link_data.text)
                                // Use text content
                            };
                            tracing::info!(
                                "Executing <d> direct command: {}",
                                command.trim()
                            );
                            command_to_send = Some(command);
                        } else {
                            // Regular <a> tag: Request context menu
                            let command = app_core.request_menu(
                                pending_click.link_data.exist_id.clone(),
                                pending_click.link_data.noun.clone(),
                                pending_click.click_pos,
                            );
                            tracing::info!(
                                "Sending _menu command for '{}' (exist_id: {})",
                                pending_click.link_data.noun,
                                pending_click.link_data.exist_id
                            );
                            command_to_send = Some(command);
                        }
                    } else {
                        tracing::debug!(
                            "Link click cancelled - dragged {} pixels",
                            dx.max(dy)
                        );
                    }
                }

                // Sync UI state positions back to layout WindowDefs after mouse resize/move
                if let Some(drag_state) = &app_core.ui_state.mouse_drag {
                    if let Some(window) =
                        app_core.ui_state.get_window(&drag_state.window_name)
                    {
                        // Find the corresponding WindowDef in layout and update it
                        if let Some(window_def) = app_core
                            .layout
                            .windows
                            .iter_mut()
                            .find(|w| w.name() == drag_state.window_name)
                        {
                            let base = window_def.base_mut();
                            base.col = window.position.x;
                            base.row = window.position.y;
                            base.cols = window.position.width;
                            base.rows = window.position.height;
                            tracing::info!("Synced mouse resize/move for '{}' to layout: pos=({},{}) size={}x{}",
                                drag_state.window_name, base.col, base.row, base.cols, base.rows);
                            app_core.layout_modified_since_save = true;
                        }
                    }
                }

                app_core.ui_state.mouse_drag = None;
                app_core.ui_state.selection_drag_start = None;

                // Handle text selection copy to clipboard
                if let Some(ref selection) = app_core.ui_state.selection_state {
                    let auto_copy = app_core.config.ui.selection_auto_copy;

                    if auto_copy && !selection.is_empty() {
                        // Extract text from selection
                        let (start, end) = selection.normalized_range();

                        // Find the window (for now assume main window)
                        if let Some((_line, _col)) = self.mouse_to_text_coords(
                            "main",
                            *x,
                            *y,
                            ratatui::layout::Rect {
                                x: app_core
                                    .ui_state
                                    .windows
                                    .get("main")
                                    .map(|w| w.position.x)
                                    .unwrap_or(0),
                                y: app_core
                                    .ui_state
                                    .windows
                                    .get("main")
                                    .map(|w| w.position.y)
                                    .unwrap_or(0),
                                width: app_core
                                    .ui_state
                                    .windows
                                    .get("main")
                                    .map(|w| w.position.width)
                                    .unwrap_or(80),
                                height: app_core
                                    .ui_state
                                    .windows
                                    .get("main")
                                    .map(|w| w.position.height)
                                    .unwrap_or(24),
                            },
                        ) {
                            if let Some(text) = self.extract_selection_text(
                                "main", start.line, start.col, end.line, end.col,
                            ) {
                                // Copy to clipboard
                                match arboard::Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        if let Err(e) = clipboard.set_text(&text) {
                                            tracing::warn!(
                                                "Failed to copy to clipboard: {}",
                                                e
                                            );
                                        } else {
                                            tracing::info!(
                                                "Copied {} chars to clipboard",
                                                text.len()
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Failed to access clipboard: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                    // Clear selection
                    if auto_copy {
                        app_core.ui_state.selection_state = None;
                    }
                    app_core.needs_render = true;
                }

                return Ok((true, command_to_send));
            }
            _ => {}
        }

        Ok((false, None))
    }

    /// Handle keyboard events (extracted from main.rs Phase 4.2)
    /// Returns optional command to send to server
    pub fn handle_key_event(
        &mut self,
        code: crate::frontend::KeyCode,
        modifiers: crate::frontend::KeyModifiers,
        app_core: &mut crate::core::AppCore,
        handle_menu_action_fn: impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<String>> {
        use crate::data::ui_state::InputMode;
        use crate::frontend::{KeyCode, KeyModifiers};
        use crate::core::input_router;

        tracing::debug!(
            "Key event: code={:?}, modifiers={:?}, input_mode={:?}",
            code,
            modifiers,
            app_core.ui_state.input_mode
        );

        // LAYER 1 & 2: Priority windows (browsers, forms, editors) - handle ALL keys
        // These modes get first priority and consume most input
        match app_core.ui_state.input_mode {
            InputMode::HighlightBrowser => {
                if let Some(ref mut browser) = self.highlight_browser {
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextItem
                        | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                        crate::core::menu_actions::MenuAction::PreviousItem
                        | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                        crate::core::menu_actions::MenuAction::NextPage => {
                            browser.next_page()
                        }
                        crate::core::menu_actions::MenuAction::PreviousPage => {
                            browser.previous_page()
                        }
                        crate::core::menu_actions::MenuAction::Save => {
                            // Ctrl+S: persist highlights, refresh caches, close browser
                            if let Err(e) =
                                app_core.config.save_highlights(app_core.config.character.as_deref())
                            {
                                app_core.add_system_message(&format!(
                                    "Failed to save highlights: {}",
                                    e
                                ));
                            } else {
                                app_core.add_system_message("Highlights saved");
                                app_core.message_processor.update_squelch_patterns();
                                app_core.message_processor.update_redirect_cache();
                                self.refresh_highlights(&app_core.config);
                            }
                            self.highlight_browser = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        crate::core::menu_actions::MenuAction::Edit => {
                            if let Some(name) = browser.get_selected() {
                                if let Some(pattern) = app_core.config.highlights.get(&name) {
                                    self.highlight_form = Some(
                                        crate::frontend::tui::highlight_form::HighlightFormWidget::new_edit(
                                            name, pattern,
                                        ),
                                    );
                                    app_core.ui_state.input_mode = InputMode::HighlightForm;
                                }
                            }
                        }
                        crate::core::menu_actions::MenuAction::New
                        | crate::core::menu_actions::MenuAction::Add => {
                            self.highlight_form = Some(
                                crate::frontend::tui::highlight_form::HighlightFormWidget::new(),
                            );
                            app_core.ui_state.input_mode = InputMode::HighlightForm;
                        }
                        crate::core::menu_actions::MenuAction::Delete => {
                            if let Some(name) = browser.get_selected() {
                                app_core.config.highlights.remove(&name);
                                browser.update_items(&app_core.config.highlights);
                                tracing::info!("Deleted highlight: {}", name);
                                if let Err(e) =
                                    app_core.config.save_highlights(app_core.config.character.as_deref())
                                {
                                    app_core.add_system_message(&format!(
                                        "Failed to save highlights: {}",
                                        e
                                    ));
                                } else {
                                    app_core.add_system_message("Highlights saved");
                                    app_core.message_processor.update_squelch_patterns();
                                    app_core.message_processor.update_redirect_cache();
                                    self.refresh_highlights(&app_core.config);
                                }
                            }
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.highlight_browser = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::KeybindBrowser => {
                if let Some(ref mut browser) = self.keybind_browser {
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextItem
                        | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                        crate::core::menu_actions::MenuAction::PreviousItem
                        | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                        crate::core::menu_actions::MenuAction::NextPage => {
                            browser.next_page()
                        }
                        crate::core::menu_actions::MenuAction::PreviousPage => {
                            browser.previous_page()
                        }
                        crate::core::menu_actions::MenuAction::ToggleFilter => {
                            browser.toggle_filter()
                        }
                        crate::core::menu_actions::MenuAction::Select
                        | crate::core::menu_actions::MenuAction::Edit => {
                            if let Some(entry) = browser.get_selected_entry() {
                                use crate::frontend::tui::keybind_form::KeybindActionType;
                                let action_type = if entry.action_type == "Action" {
                                    KeybindActionType::Action
                                } else {
                                    KeybindActionType::Macro
                                };
                                self.keybind_form = Some(
                                    crate::frontend::tui::keybind_form::KeybindFormWidget::new_edit(
                                        entry.key_combo.clone(),
                                        action_type,
                                        entry.action_value.clone(),
                                    ),
                                );
                                app_core.ui_state.input_mode = InputMode::KeybindForm;
                            }
                        }
                        crate::core::menu_actions::MenuAction::New
                        | crate::core::menu_actions::MenuAction::Add => {
                            self.keybind_form =
                                Some(crate::frontend::tui::keybind_form::KeybindFormWidget::new());
                            app_core.ui_state.input_mode = InputMode::KeybindForm;
                        }
                        crate::core::menu_actions::MenuAction::Delete => {
                            if let Some(key_combo) = browser.get_selected() {
                                app_core.config.keybinds.remove(&key_combo);
                                app_core.rebuild_keybind_map();
                                browser.update_items(&app_core.config.keybinds);
                                tracing::info!("Deleted keybind: {}", key_combo);
                            }
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.keybind_browser = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::ColorPaletteBrowser => {
                if let Some(ref mut browser) = self.color_palette_browser {
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextItem
                        | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                        crate::core::menu_actions::MenuAction::PreviousItem
                        | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                        crate::core::menu_actions::MenuAction::NextPage => {
                            browser.next_page()
                        }
                        crate::core::menu_actions::MenuAction::PreviousPage => {
                            browser.previous_page()
                        }
                        crate::core::menu_actions::MenuAction::Select
                        | crate::core::menu_actions::MenuAction::Edit => {
                            if let Some(color) = browser.get_selected_color() {
                                self.color_form = Some(
                                    crate::frontend::tui::color_form::ColorForm::new_edit(
                                        color,
                                    ),
                                );
                                app_core.ui_state.input_mode = InputMode::ColorForm;
                            }
                        }
                        crate::core::menu_actions::MenuAction::New
                        | crate::core::menu_actions::MenuAction::Add => {
                            self.color_form =
                                Some(crate::frontend::tui::color_form::ColorForm::new_create());
                            app_core.ui_state.input_mode = InputMode::ColorForm;
                        }
                        crate::core::menu_actions::MenuAction::Delete => {
                            if let Some(color_name) = browser.get_selected() {
                                app_core
                                    .config
                                    .colors
                                    .color_palette
                                    .retain(|c| c.name != color_name);
                                browser
                                    .update_items(app_core.config.colors.color_palette.clone());
                                tracing::info!("Deleted color: {}", color_name);
                            }
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.color_palette_browser = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::UIColorsBrowser => {
                if let Some(ref mut browser) = self.uicolors_browser {
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextItem
                        | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                        crate::core::menu_actions::MenuAction::PreviousItem
                        | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                        crate::core::menu_actions::MenuAction::NextPage => {
                            browser.next_page()
                        }
                        crate::core::menu_actions::MenuAction::PreviousPage => {
                            browser.previous_page()
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.uicolors_browser = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::SpellColorsBrowser => {
                if let Some(ref mut browser) = self.spell_color_browser {
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextItem
                        | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                        crate::core::menu_actions::MenuAction::PreviousItem
                        | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                        crate::core::menu_actions::MenuAction::NextPage => {
                            browser.next_page()
                        }
                        crate::core::menu_actions::MenuAction::PreviousPage => {
                            browser.previous_page()
                        }
                        crate::core::menu_actions::MenuAction::Select
                        | crate::core::menu_actions::MenuAction::Edit => {
                            if let Some(index) = browser.get_selected() {
                                let spell_color =
                                    app_core.config.colors.spell_colors.get(index).cloned();
                                if let Some(sc) = spell_color {
                                    self.spell_color_form = Some(
                                        crate::frontend::tui::spell_color_form::SpellColorFormWidget::new_edit(
                                            index, &sc,
                                        ),
                                    );
                                    app_core.ui_state.input_mode = InputMode::SpellColorForm;
                                }
                            }
                        }
                        crate::core::menu_actions::MenuAction::New
                        | crate::core::menu_actions::MenuAction::Add => {
                            self.spell_color_form = Some(
                                crate::frontend::tui::spell_color_form::SpellColorFormWidget::new(
                                ),
                            );
                            app_core.ui_state.input_mode = InputMode::SpellColorForm;
                        }
                        crate::core::menu_actions::MenuAction::Delete => {
                            if let Some(index) = browser.get_selected() {
                                if index < app_core.config.colors.spell_colors.len() {
                                    app_core.config.colors.spell_colors.remove(index);
                                    browser
                                        .update_items(&app_core.config.colors.spell_colors);
                                    tracing::info!("Deleted spell color range at index {}", index);
                                }
                            }
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.spell_color_browser = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::ThemeBrowser => {
                if let Some(ref mut browser) = self.theme_browser {
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextItem
                        | crate::core::menu_actions::MenuAction::NavigateDown => browser.navigate_down(),
                        crate::core::menu_actions::MenuAction::PreviousItem
                        | crate::core::menu_actions::MenuAction::NavigateUp => browser.navigate_up(),
                        crate::core::menu_actions::MenuAction::NextPage => {
                            browser.next_page()
                        }
                        crate::core::menu_actions::MenuAction::PreviousPage => {
                            browser.previous_page()
                        }
                        crate::core::menu_actions::MenuAction::Select => {
                            if let Some(theme_name) = browser.get_selected() {
                                app_core.config.active_theme = theme_name.clone();
                                let theme = app_core.config.get_theme();
                                self.update_theme_cache(theme_name, theme);
                                self.theme_browser = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                tracing::info!("Switched to theme: {}", app_core.config.active_theme);
                            }
                        }
                        crate::core::menu_actions::MenuAction::Edit => {
                            if let Some(theme) = browser.get_selected_theme() {
                                self.theme_editor =
                                    Some(crate::frontend::tui::theme_editor::ThemeEditor::new_edit(theme));
                                self.theme_browser = None;
                                app_core.ui_state.input_mode = InputMode::ThemeEditor;
                            }
                        }
                        crate::core::menu_actions::MenuAction::New
                        | crate::core::menu_actions::MenuAction::Add => {
                            self.theme_editor =
                                Some(crate::frontend::tui::theme_editor::ThemeEditor::new_create());
                            self.theme_browser = None;
                            app_core.ui_state.input_mode = InputMode::ThemeEditor;
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.theme_browser = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::SettingsEditor => {
                if let Some(ref mut editor) = self.settings_editor {
                    use crate::frontend::tui::widget_traits::Navigable;
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextItem
                        | crate::core::menu_actions::MenuAction::NavigateDown => editor.navigate_down(),
                        crate::core::menu_actions::MenuAction::PreviousItem
                        | crate::core::menu_actions::MenuAction::NavigateUp => editor.navigate_up(),
                        crate::core::menu_actions::MenuAction::NextPage => {
                            editor.next_page()
                        }
                        crate::core::menu_actions::MenuAction::PreviousPage => {
                            editor.previous_page()
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.settings_editor = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::HighlightForm => {
                if let Some(ref mut form) = self.highlight_form {
                    use crate::frontend::tui::widget_traits::{
                        FieldNavigable, TextEditable, Toggleable,
                    };

                    // Fast-path Ctrl+S even if keybinds are overridden/misparsed
                    if modifiers.ctrl && matches!(code, KeyCode::Char(c) if c == 's' || c == 'S') {
                        if let Some(result) = form.handle_action(crate::core::menu_actions::MenuAction::Save) {
                            match result {
                                crate::frontend::tui::highlight_form::FormResult::Save { name, mut pattern } => {
                                    if let Some(ref fg) = pattern.fg {
                                        pattern.fg = Some(app_core.config.resolve_palette_color(fg));
                                    }
                                    if let Some(ref bg) = pattern.bg {
                                        pattern.bg = Some(app_core.config.resolve_palette_color(bg));
                                    }
                                    app_core.config.highlights.insert(name.clone(), pattern);
                                if let Some(ref mut browser) = self.highlight_browser {
                                    browser.update_items(&app_core.config.highlights);
                                }
                                if let Err(e) =
                                    app_core.config.save_highlights(app_core.config.character.as_deref())
                                {
                                    app_core.add_system_message(&format!(
                                        "Failed to save highlights: {}",
                                        e
                                    ));
                                } else {
                                    app_core.add_system_message("Highlights saved");
                                    // Refresh caches that depend on highlights
                                    app_core.message_processor.update_squelch_patterns();
                                    app_core.message_processor.update_redirect_cache();
                                }
                                // Apply updated highlights to all text windows
                                self.refresh_highlights(&app_core.config);
                                tracing::info!("Saved highlight: {}", name);
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                        InputMode::Normal
                                    };
                                }
                                _ => {}
                            }
                        }
                        app_core.needs_render = true;
                        return Ok(None);
                    }
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextField => form.next_field(),
                        crate::core::menu_actions::MenuAction::PreviousField => {
                            form.previous_field()
                        }
                        crate::core::menu_actions::MenuAction::SelectAll => form.select_all(),
                        crate::core::menu_actions::MenuAction::Copy => {
                            let _ = form.copy_to_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Cut => {
                            let _ = form.cut_to_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Paste => {
                            let _ = form.paste_from_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Toggle => {
                            form.toggle_focused();
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.highlight_form = None;
                            app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                InputMode::HighlightBrowser
                            } else {
                                InputMode::Normal
                            };
                        }
                        crate::core::menu_actions::MenuAction::NavigateUp |
                        crate::core::menu_actions::MenuAction::NavigateDown |
                        crate::core::menu_actions::MenuAction::CycleBackward |
                        crate::core::menu_actions::MenuAction::CycleForward |
                        crate::core::menu_actions::MenuAction::Select |
                        crate::core::menu_actions::MenuAction::Save |
                        crate::core::menu_actions::MenuAction::Delete => {
                            // Handle navigation, cycling, and save/delete via handle_action
                    if let Some(result) = form.handle_action(action.clone()) {
                        match result {
                            crate::frontend::tui::highlight_form::FormResult::Save {
                                name,
                                mut pattern,
                            } => {
                                // Resolve palette color names to hex codes
                                if let Some(ref fg) = pattern.fg {
                                    pattern.fg = Some(app_core.config.resolve_palette_color(fg));
                                }
                                if let Some(ref bg) = pattern.bg {
                                    pattern.bg = Some(app_core.config.resolve_palette_color(bg));
                                }

                                app_core.config.highlights.insert(name.clone(), pattern);
                                if let Some(ref mut browser) = self.highlight_browser {
                                    browser.update_items(&app_core.config.highlights);
                                }
                                if let Err(e) =
                                    app_core.config.save_highlights(app_core.config.character.as_deref())
                                {
                                    app_core.add_system_message(&format!(
                                        "Failed to save highlights: {}",
                                        e
                                    ));
                                } else {
                                    app_core.add_system_message("Highlights saved");
                                }
                                // Apply updated highlights to all text windows
                                self.refresh_highlights(&app_core.config);
                                tracing::info!("Saved highlight: {}", name);
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                            InputMode::Normal
                                        };
                                    }
                                    crate::frontend::tui::highlight_form::FormResult::Delete { .. }
                                    | crate::frontend::tui::highlight_form::FormResult::Cancel => {
                                        self.highlight_form = None;
                                        app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                            InputMode::HighlightBrowser
                                        } else {
                                            InputMode::Normal
                                        };
                                    }
                                }
                            }
                        }
                        _ => {
                            use crate::frontend::tui::crossterm_bridge;
                            let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                            let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                            let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                            if let Some(result) = form.handle_key(key) {
                                match result {
                                    crate::frontend::tui::highlight_form::FormResult::Save {
                                        name,
                                        pattern,
                                    } => {
                                        // Save to current config (save_as_common feature removed)
                                        app_core.config.highlights.insert(name.clone(), pattern);
                                        if let Some(ref mut browser) = self.highlight_browser {
                                            browser.update_items(&app_core.config.highlights);
                                        }
                                        tracing::info!("Saved highlight: {}", name);
                                        self.highlight_form = None;
                                        app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                            InputMode::HighlightBrowser
                                        } else {
                                            InputMode::Normal
                                        };
                                    }
                                    crate::frontend::tui::highlight_form::FormResult::Delete { .. }
                                    | crate::frontend::tui::highlight_form::FormResult::Cancel => {
                                        self.highlight_form = None;
                                        app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                            InputMode::HighlightBrowser
                                        } else {
                                            InputMode::Normal
                                        };
                                    }
                                }
                            }
                        }
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::KeybindForm => {
                if let Some(ref mut form) = self.keybind_form {
                    use crate::frontend::tui::widget_traits::{
                        FieldNavigable, TextEditable, Toggleable,
                    };
                    use crate::frontend::tui::keybind_form::ActionSection;

                    let ctrl_only = matches!(modifiers, KeyModifiers { ctrl: true, shift: false, alt: false });

                    if ctrl_only {
                        match code {
                            KeyCode::Char('1') => {
                                form.go_to_section(ActionSection::CommandInput);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            KeyCode::Char('2') => {
                                form.go_to_section(ActionSection::CommandHistory);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            KeyCode::Char('3') => {
                                form.go_to_section(ActionSection::WindowScrolling);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            KeyCode::Char('4') => {
                                form.go_to_section(ActionSection::TabNavigation);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            KeyCode::Char('5') => {
                                form.go_to_section(ActionSection::Search);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            KeyCode::Char('6') => {
                                form.go_to_section(ActionSection::Clipboard);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            KeyCode::Char('7') => {
                                form.go_to_section(ActionSection::TTS);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            KeyCode::Char('8') => {
                                form.go_to_section(ActionSection::SystemToggles);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            KeyCode::Char('m') | KeyCode::Char('M') => {
                                form.go_to_section(ActionSection::Meta);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            _ => {}
                        }
                    }

                    // Section navigation removed - not applicable to simple form widget
                    // (This code was likely meant for KeybindBrowser, not KeybindForm)

                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextField => form.next_field(),
                        crate::core::menu_actions::MenuAction::PreviousField => {
                            form.previous_field()
                        }
                        crate::core::menu_actions::MenuAction::SelectAll => form.select_all(),
                        crate::core::menu_actions::MenuAction::Copy => {
                            let _ = form.copy_to_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Cut => {
                            let _ = form.cut_to_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Paste => {
                            let _ = form.paste_from_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Toggle => {
                            form.toggle_focused();
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.keybind_form = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        // Route navigation, cycling, select, save, and delete through handle_action
                        crate::core::menu_actions::MenuAction::NavigateUp
                        | crate::core::menu_actions::MenuAction::NavigateDown
                        | crate::core::menu_actions::MenuAction::CycleBackward
                        | crate::core::menu_actions::MenuAction::CycleForward
                        | crate::core::menu_actions::MenuAction::Select
                        | crate::core::menu_actions::MenuAction::Save
                        | crate::core::menu_actions::MenuAction::Delete => {
                            if let Some(result) = form.handle_action(action.clone()) {
                                match result {
                                    crate::frontend::tui::keybind_form::KeybindFormResult::Save {
                                        key_combo,
                                        action_type,
                                        value,
                                    } => {
                                        use crate::frontend::tui::keybind_form::KeybindActionType;
                                        let action = match action_type {
                                            KeybindActionType::Action => {
                                                crate::config::KeyBindAction::Action(value)
                                            }
                                            KeybindActionType::Macro => {
                                                crate::config::KeyBindAction::Macro(
                                                    crate::config::MacroAction { macro_text: value },
                                                )
                                            }
                                        };
                                        app_core.config.keybinds.insert(key_combo.clone(), action);
                                        app_core.rebuild_keybind_map();
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved keybind: {}", key_combo);
                                    }
                                    crate::frontend::tui::keybind_form::KeybindFormResult::Delete {
                                        key_combo,
                                    } => {
                                        app_core.config.keybinds.remove(&key_combo);
                                        app_core.rebuild_keybind_map();
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Deleted keybind: {}", key_combo);
                                    }
                                    crate::frontend::tui::keybind_form::KeybindFormResult::Cancel => {
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                }
                            }
                        }
                        _ => {
                            use crate::frontend::tui::crossterm_bridge;
                            let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                            let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                            let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                            if let Some(result) = form.handle_key(key) {
                                match result {
                                    crate::frontend::tui::keybind_form::KeybindFormResult::Save {
                                        key_combo,
                                        action_type,
                                        value,
                                    } => {
                                        use crate::frontend::tui::keybind_form::KeybindActionType;
                                        let action = match action_type {
                                            KeybindActionType::Action => {
                                                crate::config::KeyBindAction::Action(value)
                                            }
                                            KeybindActionType::Macro => {
                                                crate::config::KeyBindAction::Macro(
                                                    crate::config::MacroAction { macro_text: value },
                                                )
                                            }
                                        };
                                        app_core.config.keybinds.insert(key_combo.clone(), action);
                                        app_core.rebuild_keybind_map();
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved keybind: {}", key_combo);
                                    }
                                    crate::frontend::tui::keybind_form::KeybindFormResult::Delete {
                                        key_combo,
                                    } => {
                                        app_core.config.keybinds.remove(&key_combo);
                                        app_core.rebuild_keybind_map();
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Deleted keybind: {}", key_combo);
                                    }
                                    crate::frontend::tui::keybind_form::KeybindFormResult::Cancel => {
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                }
                            }
                        }
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::ColorForm => {
                if let Some(ref mut form) = self.color_form {
                    use crate::frontend::tui::widget_traits::{
                        FieldNavigable, TextEditable, Toggleable,
                    };
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextField => form.next_field(),
                        crate::core::menu_actions::MenuAction::PreviousField => {
                            form.previous_field()
                        }
                        crate::core::menu_actions::MenuAction::SelectAll => form.select_all(),
                        crate::core::menu_actions::MenuAction::Copy => {
                            let _ = form.copy_to_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Cut => {
                            let _ = form.cut_to_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Paste => {
                            let _ = form.paste_from_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Toggle => {
                            form.toggle_focused();
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.color_form = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        crate::core::menu_actions::MenuAction::Select |
                        crate::core::menu_actions::MenuAction::Save => {
                            // Handle Select (Enter) and Save (Ctrl+S) via handle_action
                            if let Some(result) = form.handle_action(action.clone()) {
                                match result {
                                    crate::frontend::tui::color_form::FormAction::Save {
                                        color,
                                        original_name,
                                    } => {
                                        if let Some(old_name) = original_name {
                                            if old_name != color.name {
                                                app_core
                                                    .config
                                                    .colors
                                                    .color_palette
                                                    .retain(|c| c.name != old_name);
                                            }
                                        }
                                        if let Some(existing) = app_core
                                            .config
                                            .colors
                                            .color_palette
                                            .iter_mut()
                                            .find(|c| c.name == color.name)
                                        {
                                            *existing = color.clone();
                                        } else {
                                            // Auto-assign next available slot for new colors
                                            let color_with_slot = auto_assign_slot(color.clone(), &app_core.config.colors.color_palette);
                                            app_core.config.colors.color_palette.push(color_with_slot);
                                        }
                                        self.color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved color: {}", color.name);
                                    }
                                    crate::frontend::tui::color_form::FormAction::Delete => {
                                        self.color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                    crate::frontend::tui::color_form::FormAction::Cancel => {
                                        self.color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                    crate::frontend::tui::color_form::FormAction::Error(_) => {}
                                }
                            }
                        }
                        _ => {
                            use crate::frontend::tui::crossterm_bridge;
                            let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                            let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                            let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                            if let Some(result) = form.handle_input(key) {
                                match result {
                                    crate::frontend::tui::color_form::FormAction::Save {
                                        color,
                                        original_name,
                                    } => {
                                        if let Some(old_name) = original_name {
                                            if old_name != color.name {
                                                app_core
                                                    .config
                                                    .colors
                                                    .color_palette
                                                    .retain(|c| c.name != old_name);
                                            }
                                        }
                                        if let Some(existing) = app_core
                                            .config
                                            .colors
                                            .color_palette
                                            .iter_mut()
                                            .find(|c| c.name == color.name)
                                        {
                                            *existing = color.clone();
                                        } else {
                                            // Auto-assign next available slot for new colors
                                            let color_with_slot = auto_assign_slot(color.clone(), &app_core.config.colors.color_palette);
                                            app_core.config.colors.color_palette.push(color_with_slot);
                                        }
                                        self.color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved color: {}", color.name);
                                    }
                                    crate::frontend::tui::color_form::FormAction::Delete => {
                                        self.color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                    crate::frontend::tui::color_form::FormAction::Cancel => {
                                        self.color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                    crate::frontend::tui::color_form::FormAction::Error(_) => {}
                                }
                            }
                        }
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::SpellColorForm => {
                if let Some(ref mut form) = self.spell_color_form {
                    use crate::frontend::tui::widget_traits::{FieldNavigable, TextEditable};
                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextField => form.next_field(),
                        crate::core::menu_actions::MenuAction::PreviousField => {
                            form.previous_field()
                        }
                        crate::core::menu_actions::MenuAction::SelectAll => form.select_all(),
                        crate::core::menu_actions::MenuAction::Copy => {
                            let _ = form.copy_to_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Cut => {
                            let _ = form.cut_to_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Paste => {
                            let _ = form.paste_from_clipboard();
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.spell_color_form = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        // Route navigation, select, save, and delete through handle_action
                        crate::core::menu_actions::MenuAction::NavigateUp
                        | crate::core::menu_actions::MenuAction::NavigateDown
                        | crate::core::menu_actions::MenuAction::Select
                        | crate::core::menu_actions::MenuAction::Save
                        | crate::core::menu_actions::MenuAction::Delete => {
                            if let Some(result) = form.handle_action(action.clone()) {
                                match result {
                                    crate::frontend::tui::spell_color_form::SpellColorFormResult::Save(
                                        mut spell_color,
                                    ) => {
                                        // Resolve palette color names to hex codes
                                        spell_color.color = app_core.config.resolve_palette_color(&spell_color.color);
                                        if let Some(ref bar) = spell_color.bar_color {
                                            spell_color.bar_color = Some(app_core.config.resolve_palette_color(bar));
                                        }
                                        if let Some(ref text) = spell_color.text_color {
                                            spell_color.text_color = Some(app_core.config.resolve_palette_color(text));
                                        }
                                        if let Some(ref bg) = spell_color.bg_color {
                                            spell_color.bg_color = Some(app_core.config.resolve_palette_color(bg));
                                        }

                                        app_core.config.colors.spell_colors.push(spell_color);
                                        self.spell_color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved spell color range");
                                    }
                                    crate::frontend::tui::spell_color_form::SpellColorFormResult::Delete(
                                        index,
                                    ) => {
                                        if index < app_core.config.colors.spell_colors.len() {
                                            app_core.config.colors.spell_colors.remove(index);
                                            tracing::info!("Deleted spell color range");
                                        }
                                        self.spell_color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                    crate::frontend::tui::spell_color_form::SpellColorFormResult::Cancel => {
                                        self.spell_color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                }
                            }
                        }
                        _ => {
                            use crate::frontend::tui::crossterm_bridge;
                            let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                            let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                            let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                            if let Some(result) = form.input(key) {
                                match result {
                                    crate::frontend::tui::spell_color_form::SpellColorFormResult::Save(
                                        mut spell_color,
                                    ) => {
                                        // Resolve palette color names to hex codes
                                        spell_color.color = app_core.config.resolve_palette_color(&spell_color.color);
                                        if let Some(ref bar) = spell_color.bar_color {
                                            spell_color.bar_color = Some(app_core.config.resolve_palette_color(bar));
                                        }
                                        if let Some(ref text) = spell_color.text_color {
                                            spell_color.text_color = Some(app_core.config.resolve_palette_color(text));
                                        }
                                        if let Some(ref bg) = spell_color.bg_color {
                                            spell_color.bg_color = Some(app_core.config.resolve_palette_color(bg));
                                        }

                                        app_core.config.colors.spell_colors.push(spell_color);
                                        self.spell_color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved spell color range");
                                    }
                                    crate::frontend::tui::spell_color_form::SpellColorFormResult::Delete(
                                        index,
                                    ) => {
                                        if index < app_core.config.colors.spell_colors.len() {
                                            app_core.config.colors.spell_colors.remove(index);
                                            tracing::info!("Deleted spell color range");
                                        }
                                        self.spell_color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                    crate::frontend::tui::spell_color_form::SpellColorFormResult::Cancel => {
                                        self.spell_color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                }
                            }
                        }
                    }
                    app_core.needs_render = true;
                }
                return Ok(None);
            }
            InputMode::ThemeEditor => {
                if let Some(ref mut editor) = self.theme_editor {
                    use crate::core::input_router;

                    // Ctrl+1-6 section jumping (high priority)
                    if modifiers.ctrl {
                        match code {
                            crate::frontend::KeyCode::Char(c @ '1'..='6') => {
                                let section = c.to_digit(10).expect("char '1'..='6' is always a digit") as usize;
                                editor.jump_to_section(section);
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                            _ => {}
                        }
                    }

                    let key_event = crate::frontend::common::KeyEvent { code, modifiers };
                    let action = input_router::route_input(
                        &key_event,
                        &app_core.ui_state.input_mode,
                        &app_core.config,
                    );

                    match action {
                        crate::core::menu_actions::MenuAction::NextField => {
                            editor.next_field();
                            app_core.needs_render = true;
                        }
                        crate::core::menu_actions::MenuAction::PreviousField => {
                            editor.previous_field();
                            app_core.needs_render = true;
                        }
                        crate::core::menu_actions::MenuAction::Cancel => {
                            self.theme_editor = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                            app_core.needs_render = true;
                        }
                        // Route navigation and save through handle_action
                        crate::core::menu_actions::MenuAction::NavigateUp
                        | crate::core::menu_actions::MenuAction::NavigateDown
                        | crate::core::menu_actions::MenuAction::Save => {
                            if let Some(result) = editor.handle_action(action.clone()) {
                                match result {
                                    crate::frontend::tui::theme_editor::ThemeEditorResult::Save(mut theme_data) => {
                                        // Resolve palette color names to hex codes
                                        theme_data.resolve_palette_colors(&app_core.config);

                                        match theme_data.save_to_file(app_core.config.character.as_deref()) {
                                            Ok(path) => {
                                                tracing::info!("Saved custom theme '{}' to {:?}", theme_data.name, path);
                                                app_core.add_system_message(&format!(
                                                    "Saved custom theme: {}",
                                                    theme_data.name
                                                ));

                                                if let Some(_app_theme) = theme_data.to_app_theme() {
                                                    app_core.config.active_theme = theme_data.name.clone();
                                                    let theme = app_core.config.get_theme();
                                                    self.update_theme_cache(theme_data.name.clone(), theme);
                                                    app_core.needs_render = true;
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!("Failed to save custom theme: {}", e);
                                                app_core.add_system_message(&format!("Error saving theme: {}", e));
                                            }
                                        }
                                        self.theme_editor = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                    crate::frontend::tui::theme_editor::ThemeEditorResult::Cancel => {
                                        self.theme_editor = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                    }
                                }
                            }
                        }
                        _ => {
                            use crate::frontend::tui::crossterm_bridge;
                            let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                            let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                            let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                            let _ = editor.handle_input(key);
                            app_core.needs_render = true;
                        }
                    }
                }
                return Ok(None);
            }
            InputMode::IndicatorTemplateEditor => {
                if let Some(ref mut editor) = self.indicator_template_editor {
                    let result = editor.handle_key(code, modifiers);
                    app_core.needs_render = true;
                    if matches!(
                        result,
                        crate::frontend::tui::indicator_template_editor::EditorAction::Close
                    ) {
                        self.indicator_template_editor = None;
                        app_core.ui_state.input_mode = InputMode::Normal;
                    }
                } else {
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                return Ok(None);
            }
            _ => {}
        }

        // Menu mode keyboard navigation
        if app_core.ui_state.input_mode == InputMode::Menu {
            return self.handle_menu_mode_keys(code, modifiers, app_core, handle_menu_action_fn);
        }

        // WindowEditor mode keyboard handling
        if app_core.ui_state.input_mode == InputMode::WindowEditor {
            return self.handle_window_editor_keys(code, modifiers, app_core);
        }

        // Search mode keyboard handling
        if app_core.ui_state.input_mode == InputMode::Search {
            return self.handle_search_mode_keys(code, app_core);
        }

        // Normal mode: user keybinds + CommandInput fallback
        self.handle_normal_mode_keys(code, modifiers, app_core)
    }

    /// Handle Menu mode keyboard navigation (extracted from main.rs Phase 4.2)
    fn handle_menu_mode_keys(
        &mut self,
        code: crate::frontend::KeyCode,
        _modifiers: crate::frontend::KeyModifiers,
        app_core: &mut crate::core::AppCore,
        handle_menu_action_fn: impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<String>> {
        
        
        use crate::frontend::KeyCode;
        use crate::data::ui_state::InputMode;

        tracing::debug!("Menu mode active - handling key: {:?}", code);

        match code {
            KeyCode::Esc => {
                if app_core.ui_state.nested_submenu.is_some() {
                    // Close the deepest level first
                    app_core.ui_state.nested_submenu = None;
                } else if app_core.ui_state.submenu.is_some() {
                    app_core.ui_state.submenu = None;
                } else {
                    // Close all menus and return to normal mode
                    app_core.ui_state.popup_menu = None;
                    app_core.ui_state.submenu = None;
                    app_core.ui_state.nested_submenu = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                app_core.needs_render = true;
            }
            KeyCode::Tab | KeyCode::Down => {
                if let Some(ref mut nested) = app_core.ui_state.nested_submenu {
                    nested.select_next();
                } else if let Some(ref mut submenu) = app_core.ui_state.submenu {
                    submenu.select_next();
                } else if let Some(ref mut menu) = app_core.ui_state.popup_menu {
                    menu.select_next();
                }
                app_core.needs_render = true;
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(ref mut nested) = app_core.ui_state.nested_submenu {
                    nested.select_prev();
                } else if let Some(ref mut submenu) = app_core.ui_state.submenu {
                    submenu.select_prev();
                } else if let Some(ref mut menu) = app_core.ui_state.popup_menu {
                    menu.select_prev();
                }
                app_core.needs_render = true;
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Choose the deepest open menu
                let menu_to_use = if app_core.ui_state.nested_submenu.is_some() {
                    &app_core.ui_state.nested_submenu
                } else if app_core.ui_state.submenu.is_some() {
                    &app_core.ui_state.submenu
                } else {
                    &app_core.ui_state.popup_menu
                };

                if let Some(menu) = menu_to_use {
                    if let Some(item) = menu.selected_item() {
                        let command = item.command.clone();
                        tracing::info!("Menu command selected: {}", command);

                        return self.handle_menu_command(command, app_core, handle_menu_action_fn);
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    /// Handle menu command execution (extracted from main.rs Phase 4.2)
    fn handle_menu_command(
        &mut self,
        command: String,
        app_core: &mut crate::core::AppCore,
        handle_menu_action_fn: impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<String>> {
        
        use crate::data::ui_state::{InputMode, PopupMenu};

        if let Some(submenu_name) = command.strip_prefix("menu:") {
            let items = match submenu_name {
                "windows" => app_core.build_windows_submenu(),
                "config" => Self::build_config_submenu(),
                "layouts" => app_core.build_layouts_submenu(),
                "widgetpicker" | "addwindow" => app_core.build_add_window_menu(),
                "hidewindow" => app_core.build_hide_window_menu(),
                "editwindow" => app_core.build_edit_window_menu(),
                _ => {
                    app_core.ui_state.popup_menu = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                    return Ok(None);
                }
            };
            app_core.ui_state.popup_menu = Some(PopupMenu::new(items, (40, 12)));
            app_core.needs_render = true;
        } else if let Some(category) = command.strip_prefix("__SUBMENU__") {
            let items = app_core.build_submenu(category);
            let items = if !items.is_empty() {
                items
            } else if let Some(items) = app_core.menu_categories.get(category) {
                items.clone()
            } else {
                Vec::new()
            };

            if !items.is_empty() {
                let position = app_core
                    .ui_state
                    .popup_menu
                    .as_ref()
                    .map(|m| m.get_position())
                    .unwrap_or((40, 12));
                let submenu_pos = (position.0 + 2, position.1);
                app_core.ui_state.submenu = Some(PopupMenu::new(items, submenu_pos));
                tracing::info!("Opened submenu: {}", category);
            } else {
                app_core.ui_state.popup_menu = None;
                app_core.ui_state.input_mode = InputMode::Normal;
            }
            app_core.needs_render = true;
        } else if let Some(category_str) = command.strip_prefix("__SUBMENU_ADD__") {
            let category = Self::parse_widget_category(category_str, app_core)?;
            let items = app_core.build_add_window_category_menu(&category);
            if items.is_empty() {
                app_core.ui_state.nested_submenu = None;
            } else {
                let parent_pos = app_core
                    .ui_state
                    .submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.nested_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if command == "__SUBMENU_INDICATORS" {
            // Indicator submenu under Status
            let templates = crate::config::Config::get_addable_templates_by_category(&app_core.layout)
                .get(&crate::config::WidgetCategory::Status)
                .cloned()
                .unwrap_or_default();
            let items = app_core.build_indicator_add_menu(&templates);
            if items.is_empty() {
                app_core.ui_state.nested_submenu = None;
            } else {
                let parent_pos = app_core
                    .ui_state
                    .nested_submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.nested_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if let Some(category_str) = command.strip_prefix("__SUBMENU_HIDE__") {
            let category = Self::parse_widget_category(category_str, app_core)?;
            let items = app_core.build_hide_window_category_menu(&category);
            if items.is_empty() {
                app_core.ui_state.nested_submenu = None;
            } else {
                let parent_pos = app_core
                    .ui_state
                    .submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.nested_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if let Some(category_str) = command.strip_prefix("__SUBMENU_EDIT__") {
            let category = Self::parse_widget_category(category_str, app_core)?;
            let items = app_core.build_edit_window_category_menu(&category);
            if items.is_empty() {
                app_core.ui_state.nested_submenu = None;
            } else {
                let parent_pos = app_core
                    .ui_state
                    .submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.nested_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if command == "__SUBMENU_HIDE_INDICATORS" {
            let indicators = app_core
                .layout
                .windows
                .iter()
                .filter(|w| w.base().visible && matches!(w.widget_type(), "indicator"))
                .map(|w| w.name().to_string())
                .collect::<Vec<String>>();
            let items = app_core.build_indicator_hide_menu(&indicators);
            if items.is_empty() {
                app_core.ui_state.nested_submenu = None;
            } else {
                let parent_pos = app_core
                    .ui_state
                    .nested_submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.nested_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if command == "__SUBMENU_EDIT_INDICATORS" {
            let indicators = app_core
                .layout
                .windows
                .iter()
                .filter(|w| w.base().visible && matches!(w.widget_type(), "indicator"))
                .map(|w| w.name().to_string())
                .collect::<Vec<String>>();
            let items = app_core.build_indicator_edit_menu(&indicators);
            if items.is_empty() {
                app_core.ui_state.nested_submenu = None;
            } else {
                let parent_pos = app_core
                    .ui_state
                    .nested_submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.nested_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if command == "__INDICATOR_EDITOR" {
            self.indicator_template_editor =
                Some(crate::frontend::tui::indicator_template_editor::IndicatorTemplateEditor::new());
            app_core.ui_state.popup_menu = None;
            app_core.ui_state.submenu = None;
            app_core.ui_state.nested_submenu = None;
            app_core.ui_state.input_mode = crate::data::ui_state::InputMode::IndicatorTemplateEditor;
            app_core.needs_render = true;
        } else if let Some(widget_type) = command.strip_prefix("__ADD_CUSTOM__") {
            // Start a new blank/custom window editor for this widget type
            use crate::frontend::tui::window_editor::WindowEditor;
            let mut editor = WindowEditor::new_window(widget_type.to_string());

            // Generate a unique name like custom_<type>[_n]
            let base = format!("custom_{}", widget_type);
            let mut candidate = base.clone();
            let mut idx = 1;
            while app_core.layout.get_window(&candidate).is_some() {
                candidate = format!("{}_{}", base, idx);
                idx += 1;
            }
            editor.set_name(&candidate);

            self.window_editor = Some(editor);
            app_core.ui_state.popup_menu = None;
            app_core.ui_state.submenu = None;
            app_core.ui_state.input_mode = InputMode::WindowEditor;
            app_core.needs_render = true;
        } else if let Some(window_name) = command.strip_prefix("__ADD__") {
            match app_core.layout.add_window(window_name) {
                Ok(_) => {
                    let (width, height) = self.size();
                    // Only add the NEW window to UI state, don't overwrite existing windows
                    // (sync_layout_to_ui_state was overwriting all windows, resetting user changes)

                    // For spacers, the actual name is auto-generated (spacer_1, spacer_2, etc.)
                    // so we need to get the last window in the layout instead
                    let window_def = if window_name == "spacer" {
                        app_core.layout.windows.last().cloned()
                    } else {
                        app_core.layout.get_window(window_name).cloned()
                    };

                    if let Some(window_def) = window_def {
                        let actual_name = window_def.name().to_string();
                        app_core.add_new_window(&window_def, width, height);
                        app_core.layout_modified_since_save = true;
                        app_core.add_system_message(&format!("Window '{}' added", actual_name));
                        tracing::info!("Added window: {}", actual_name);

                        // Immediately open editor for the newly added window
                        self.window_editor = Some(
                            crate::frontend::tui::window_editor::WindowEditor::new(window_def)
                        );
                        app_core.ui_state.input_mode = InputMode::WindowEditor;
                    } else {
                        // Fallback to normal mode if something goes wrong
                        app_core.add_system_message(&format!("Window '{}' added but couldn't retrieve definition", window_name));
                        app_core.ui_state.input_mode = InputMode::Normal;
                    }
                }
                Err(e) => {
                    app_core.add_system_message(&format!("Failed to add window: {}", e));
                    tracing::error!("Failed to add window '{}': {}", window_name, e);
                }
            }
            app_core.ui_state.popup_menu = None;
            app_core.ui_state.submenu = None;
            app_core.ui_state.nested_submenu = None;
            app_core.needs_render = true;
        } else if let Some(window_name) = command.strip_prefix("__HIDE__") {
            match app_core.layout.hide_window(window_name) {
                Ok(_) => {
                    app_core.ui_state.remove_window(window_name);
                    app_core.layout_modified_since_save = true;
                    app_core.add_system_message(&format!("Window '{}' hidden", window_name));
                    tracing::info!("Hidden window: {}", window_name);
                    app_core.layout.remove_window_if_default(window_name);
                }
                Err(e) => {
                    app_core.add_system_message(&format!("Failed to hide window: {}", e));
                    tracing::error!("Failed to hide window '{}': {}", window_name, e);
                }
            }
            // Keep parent menus open so Esc can back up
            app_core.ui_state.nested_submenu = None;
            app_core.needs_render = true;
        } else if let Some(window_name) = command.strip_prefix("__EDIT__") {
            if let Some(window_def) = app_core.layout.get_window(window_name) {
                self.window_editor = Some(crate::frontend::tui::window_editor::WindowEditor::new(
                    window_def.clone(),
                ));
                app_core.ui_state.input_mode = InputMode::WindowEditor;
                tracing::info!("Opening window editor for: {}", window_name);
            } else {
                app_core.add_system_message(&format!("Window '{}' not found", window_name));
                tracing::warn!("Window '{}' not found in layout", window_name);
            }
            app_core.ui_state.popup_menu = None;
            app_core.ui_state.submenu = None;
            app_core.ui_state.nested_submenu = None;
            app_core.needs_render = true;
        } else {
            // Internal action commands should manage menus themselves
            if command.starts_with("action:") {
                handle_menu_action_fn(app_core, self, &command)?;
                app_core.needs_render = true;
            } else if command.starts_with(".") {
                let action_command = format!("action:{}", &command[1..]);
                handle_menu_action_fn(app_core, self, &action_command)?;
                app_core.needs_render = true;
            } else {
                // Game command or empty selection: close menus
                app_core.ui_state.popup_menu = None;
                app_core.ui_state.submenu = None;
                app_core.ui_state.nested_submenu = None;
                app_core.ui_state.input_mode = InputMode::Normal;
                app_core.needs_render = true;

                if !command.is_empty() {
                    tracing::info!("Sending context menu command: {}", command);
                    return Ok(Some(format!("{}\n", command)));
                }
            }
        }
        Ok(None)
    }

    /// Parse widget category from string (helper for menu commands)
    fn parse_widget_category(
        category_str: &str,
        app_core: &mut crate::core::AppCore,
    ) -> Result<crate::config::WidgetCategory> {
        use crate::config::WidgetCategory;
        use crate::data::ui_state::InputMode;

        match category_str {
            "ProgressBar" => Ok(WidgetCategory::ProgressBar),
            "TextWindow" => Ok(WidgetCategory::TextWindow),
            "Countdown" => Ok(WidgetCategory::Countdown),
            "Hand" => Ok(WidgetCategory::Hand),
            "ActiveEffects" => Ok(WidgetCategory::ActiveEffects),
            "Entity" => Ok(WidgetCategory::Entity),
            "Status" => Ok(WidgetCategory::Status),
            "Other" => Ok(WidgetCategory::Other),
            _ => {
                tracing::warn!("Unknown widget category: {}", category_str);
                app_core.ui_state.popup_menu = None;
                app_core.ui_state.input_mode = InputMode::Normal;
                app_core.needs_render = true;
                Ok(WidgetCategory::Other)
            }
        }
    }

    /// Build configuration submenu (delegates to menu_builders module)
    fn build_config_submenu() -> Vec<crate::data::ui_state::PopupMenuItem> {
        menu_builders::build_config_submenu()
    }

    /// Handle WindowEditor mode keyboard events (extracted from main.rs Phase 4.2)
    fn handle_window_editor_keys(
        &mut self,
        code: crate::frontend::KeyCode,
        modifiers: crate::frontend::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut editor) = self.window_editor {
            let key_event = crate::frontend::common::KeyEvent { code, modifiers };
            let action = input_router::route_input(
                &key_event,
                &app_core.ui_state.input_mode,
                &app_core.config,
            );

            match action {
                crate::core::menu_actions::MenuAction::NextField
                | crate::core::menu_actions::MenuAction::NavigateDown => {
                    if editor.is_sub_editor_active() {
                        editor.handle_sub_editor_navigation(true);
                        app_core.needs_render = true;
                    } else {
                        editor.navigate_down();
                        app_core.needs_render = true;
                    }
                }
                crate::core::menu_actions::MenuAction::PreviousField
                | crate::core::menu_actions::MenuAction::NavigateUp => {
                    if editor.is_sub_editor_active() {
                        editor.handle_sub_editor_navigation(false);
                        app_core.needs_render = true;
                    } else {
                        editor.navigate_up();
                        app_core.needs_render = true;
                    }
                }
                crate::core::menu_actions::MenuAction::Toggle => {
                    if editor.is_sub_editor_active() {
                        // Sub-editors handle raw keys directly
                    } else if editor.is_on_checkbox() {
                        editor.toggle_field();
                        app_core.needs_render = true;
                    } else if editor.is_on_content_align() {
                        editor.cycle_content_align(false);
                        app_core.needs_render = true;
                    } else if editor.is_on_title_position() {
                        editor.cycle_title_position(false);
                        app_core.needs_render = true;
                    } else if editor.is_on_tab_bar_position() {
                        editor.cycle_tab_bar_position();
                        app_core.needs_render = true;
                    } else if editor.is_on_edit_tabs()
                        || editor.is_on_edit_indicators()
                        || editor.is_on_perf_metrics_button()
                    {
                        editor.toggle_field();
                        app_core.needs_render = true;
                    } else if editor.is_on_border_style() {
                        editor.cycle_border_style(false);
                        app_core.needs_render = true;
                    }
                }
                crate::core::menu_actions::MenuAction::CycleForward
                | crate::core::menu_actions::MenuAction::CycleBackward => {
                    if !editor.is_sub_editor_active() {
                        let reverse =
                            matches!(action, crate::core::menu_actions::MenuAction::CycleBackward);
                        if editor.is_on_content_align() {
                            editor.cycle_content_align(reverse);
                            app_core.needs_render = true;
                        } else if editor.is_on_title_position() {
                            editor.cycle_title_position(reverse);
                            app_core.needs_render = true;
                        } else if editor.is_on_tab_bar_position() {
                            editor.cycle_tab_bar_position();
                            app_core.needs_render = true;
                        } else if editor.is_on_border_style() {
                            editor.cycle_border_style(reverse);
                            app_core.needs_render = true;
                        }
                    }
                }
                crate::core::menu_actions::MenuAction::Select => {
                    if editor.is_sub_editor_active() {
                        // Treat select as no-op; sub editor handles raw keys
                    } else {
                        if editor.is_on_checkbox()
                            || editor.is_on_content_align()
                            || editor.is_on_title_position()
                            || editor.is_on_tab_bar_position()
                            || editor.is_on_border_style()
                            || editor.is_on_edit_tabs()
                            || editor.is_on_edit_indicators()
                            || editor.is_on_perf_metrics_button()
                        {
                            if editor.is_on_checkbox() {
                                editor.toggle_field();
                            } else if editor.is_on_content_align() {
                                editor.cycle_content_align(false);
                            } else if editor.is_on_title_position() {
                                editor.cycle_title_position(false);
                            } else if editor.is_on_tab_bar_position() {
                                editor.cycle_tab_bar_position();
                            } else if editor.is_on_border_style() {
                                editor.cycle_border_style(false);
                            } else if editor.is_on_edit_tabs()
                                || editor.is_on_edit_indicators()
                                || editor.is_on_perf_metrics_button()
                            {
                                editor.toggle_field();
                            }
                            app_core.needs_render = true;
                        }
                    }
                }
                crate::core::menu_actions::MenuAction::Save => {
                    let (width, height) = self.size();
                    if let Some(ref mut editor) = self.window_editor {
                        // If a sub-editor form is active, save it and return to the sub-editor list
                        if editor.save_active_sub_editor_form() {
                            app_core.needs_render = true;
                            return Ok(None);
                        }

                        editor.commit_sub_editors();
                        // Validate name/uniqueness before saving
                        if !editor.validate_before_save(&app_core.layout) {
                            app_core.needs_render = true;
                            return Ok(None);
                        }
                        let window_def = editor.get_window_def().clone();

                        // Persist custom templates to the global store when appropriate
                        let original_template = editor.original_name().to_string();
                        let orig_is_custom = original_template.to_lowercase().contains("custom");
                        let exists_in_store = Config::window_template_exists(window_def.name());
                        let is_performance = original_template.eq_ignore_ascii_case("performance");
                        if orig_is_custom || exists_in_store || is_performance {
                            if let Err(e) = Config::upsert_window_template(&window_def) {
                                tracing::warn!("Failed to save window template {}: {}", window_def.name(), e);
                            }
                        }

                        if editor.is_new() {
                            app_core.layout.windows.insert(0, window_def.clone());
                            tracing::info!("Added new window: {}", window_def.name());
                            app_core.add_new_window(&window_def, width, height);
                        } else {
                            if let Some(existing) = app_core
                                .layout
                                .windows
                                .iter_mut()
                                .find(|w| w.name() == window_def.name())
                            {
                                *existing = window_def.clone();
                                tracing::info!("Updated window: {}", window_def.name());
                                app_core.update_window_position(&window_def, width, height);
                            }
                        }
                        app_core.mark_layout_modified();
                        self.window_editor = None;
                        app_core.ui_state.input_mode = InputMode::Normal;
                        app_core.needs_render = true;
                    }
                }
                crate::core::menu_actions::MenuAction::Delete => {
                    if let Some(ref mut editor) = self.window_editor {
                        let window_name = editor.get_window_def().name().to_string();
                        let is_locked = editor.get_window_def().base().locked;

                        if !is_locked {
                            app_core.hide_window(&window_name);
                            self.window_editor = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                    }
                    app_core.needs_render = true;
                }
                crate::core::menu_actions::MenuAction::Cancel => {
                    if let Some(ref mut editor) = self.window_editor {
                        if editor.is_sub_editor_active() {
                            if editor.handle_sub_editor_cancel() {
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                        }
                    }
                    self.window_editor = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                    app_core.needs_render = true;
                }
                _ => {
                    use crate::frontend::tui::crossterm_bridge;
                    let tf_key = crate::frontend::common::KeyEvent { code, modifiers };
                    let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                    let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                    let key_event = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                    let rt_key = crate::frontend::tui::textarea_bridge::to_textarea_event(key_event);
                    if let Some(ref mut editor) = self.window_editor {
                        if editor.is_sub_editor_active() {
                            if editor.handle_sub_editor_key(tf_key) {
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                        }
                        editor.input(rt_key);
                        app_core.needs_render = true;
                    }
                }
            }
        }
        Ok(None)
    }
}
